use libloading::Library;
use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};
use std::io::{BufReader, Read};
use std::path::Path;

/// USB vendor ids this driver claims. Hosts that need raw USB access
/// (udev rules on Linux) must cover these; see
/// `usb_discovery::builtin_usb_vendor_claims`.
pub fn usb_vendor_ids() -> Vec<u16> {
    vec![protocol::MCL_VENDOR_ID]
}

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const MCL_VENDOR_ID: u16 = 0x1569;
    pub const MICRODRIVE_ENCODER_REQUEST: u8 = 0xe7;
    pub const MICRODRIVE_STATUS_REQUEST: u8 = 0xcd;
    pub const MICRODRIVE_STOP_REQUEST: u8 = 0xc9;
    pub const MICRODRIVE_RESET_ENCODERS_REQUEST: u8 = 0xca;
    pub const MICRODRIVE_RESET_X_ENCODER_REQUEST: u8 = 0xcb;
    pub const MICRODRIVE_RESET_Y_ENCODER_REQUEST: u8 = 0xcc;
    pub const MICRODRIVE_RESET_Z_ENCODER_REQUEST: u8 = 0xd3;
    pub const MICRODRIVE_8BIT_MOVEMENT_STATUS_REQUEST: u8 = 0xcf;
    pub const MICRODRIVE_MOVE_STATUS_REQUEST: u8 = 0xd2;
    pub const MICRODRIVE_ASSIGNMENTS_REQUEST: u8 = 0xd5;
    pub const MICRODRIVE_WAIT_TIME_REQUEST: u8 = 0xd7;
    pub const MICRODRIVE_TEMPERATURE_REQUEST: u8 = 0xda;
    pub const MICRODRIVE_MODE_REQUEST: u8 = 0xdd;
    pub const MICRODRIVE_ROTATIONS_REQUEST: u8 = 0xde;
    pub const MICRODRIVE_MD8_RESET_ENCODER_REQUEST: u8 = 0xe8;
    pub const MICRODRIVE_MMT_STATE_REQUEST: u8 = 0xe9;
    pub const MICRODRIVE_ENCODER_LEN: usize = 24;
    pub const MICRODRIVE_BULK_ENCODER_LEN: usize = 512;
    pub const MICRODRIVE_GLOBAL_OUT_ENDPOINT: u8 = 0x02;
    pub const MICRODRIVE_GLOBAL_IN_ENDPOINT: u8 = 0x86;

    pub fn parse_microdrive_encoder_values(payload: &[u8]) -> Result<[i32; 8]> {
        if payload.len() < MICRODRIVE_ENCODER_LEN {
            return Err(Error::new(
                ErrorCode::Transport,
                "MCL MicroDrive encoder payload must be at least 24 bytes",
            ));
        }
        let mut values = [0_i32; 8];
        for (index, chunk) in payload[..MICRODRIVE_ENCODER_LEN]
            .chunks_exact(3)
            .enumerate()
        {
            values[index] =
                ((chunk[2] as i8 as i32) << 16) | ((chunk[1] as i32) << 8) | chunk[0] as i32;
        }
        Ok(values)
    }

    pub fn axis_status_bits(raw_status: u16, axis_index: usize) -> u8 {
        ((raw_status >> (axis_index * 2)) & 0x03) as u8
    }

    pub fn microdrive_has_two_byte_status(product_id: u16) -> bool {
        matches!(product_id, 0x2504 | 0x2506 | 0x2580 | 0x2581 | 0x2588)
    }

    pub fn is_microdrive_product(product_id: u16) -> bool {
        matches!(
            product_id,
            0x2500 | 0x2501 | 0x2503 | 0x2504 | 0x2506 | 0x2522 | 0x2580 | 0x2581 | 0x2588 | 0x3500
        )
    }

    pub fn is_nanodrive_product(vendor_id: u16, product_id: u16) -> bool {
        matches!(
            (vendor_id, product_id),
            (MCL_VENDOR_ID, 0x0001)
                | (MCL_VENDOR_ID, 0x1000)
                | (MCL_VENDOR_ID, 0x1020)
                | (MCL_VENDOR_ID, 0x1030)
                | (MCL_VENDOR_ID, 0x1230)
                | (MCL_VENDOR_ID, 0x1253)
                | (MCL_VENDOR_ID, 0x2000)
                | (MCL_VENDOR_ID, 0x2001)
                | (MCL_VENDOR_ID, 0x2003)
                | (MCL_VENDOR_ID, 0x2004)
                | (MCL_VENDOR_ID, 0x2053)
                | (MCL_VENDOR_ID, 0x2100)
                | (MCL_VENDOR_ID, 0x2201)
                | (MCL_VENDOR_ID, 0x2203)
                | (MCL_VENDOR_ID, 0x2253)
                | (MCL_VENDOR_ID, 0x2401)
                | (MCL_VENDOR_ID, 0x2601)
                | (MCL_VENDOR_ID, 0x3003)
        )
    }

    pub fn is_prefirmware_product(vendor_id: u16, product_id: u16) -> bool {
        matches!((vendor_id, product_id), (0x0547, 0x8613) | (0x04b4, 0x2235))
    }

    pub fn is_mcl_candidate(vendor_id: u16, product_id: u16) -> bool {
        (vendor_id == MCL_VENDOR_ID && is_microdrive_product(product_id))
            || is_nanodrive_product(vendor_id, product_id)
            || is_prefirmware_product(vendor_id, product_id)
    }

    pub fn family_for_product(vendor_id: u16, product_id: u16) -> &'static str {
        if vendor_id == MCL_VENDOR_ID && is_microdrive_product(product_id) {
            "microdrive"
        } else if is_nanodrive_product(vendor_id, product_id) {
            "nanodrive"
        } else if is_prefirmware_product(vendor_id, product_id) {
            "prefirmware"
        } else {
            "unknown"
        }
    }

    pub fn parse_status_word(payload: &[u8]) -> Result<u16> {
        let Some(first) = payload.first().copied() else {
            return Err(Error::new(
                ErrorCode::Transport,
                "MCL MicroDrive status reply is empty",
            ));
        };
        let second = payload.get(1).copied().unwrap_or(0);
        Ok(u16::from_le_bytes([first, second]))
    }

    pub fn fixed_length_control_command(
        command: &str,
        product_id: u16,
    ) -> Option<(u8, u16, usize)> {
        let status_len = if microdrive_has_two_byte_status(product_id) {
            2
        } else {
            1
        };
        match command {
            "stop" => Some((MICRODRIVE_STOP_REQUEST, 0, 1)),
            "refresh_status" => Some((MICRODRIVE_STATUS_REQUEST, 0, status_len)),
            "refresh_8bit_movement_status" => Some((MICRODRIVE_8BIT_MOVEMENT_STATUS_REQUEST, 0, 1)),
            "refresh_move_status" => Some((MICRODRIVE_MOVE_STATUS_REQUEST, 0, 2)),
            "refresh_assignments" => Some((MICRODRIVE_ASSIGNMENTS_REQUEST, 0, 6)),
            "refresh_wait_time" => Some((MICRODRIVE_WAIT_TIME_REQUEST, 0, 4)),
            "refresh_temperature" => Some((MICRODRIVE_TEMPERATURE_REQUEST, 0, 4)),
            "refresh_mode" => Some((MICRODRIVE_MODE_REQUEST, 0, 2)),
            "refresh_rotations" => Some((MICRODRIVE_ROTATIONS_REQUEST, 0, 10)),
            "refresh_mmt_state" => Some((MICRODRIVE_MMT_STATE_REQUEST, 1, 64)),
            _ => None,
        }
    }

    pub fn generic_command_names() -> &'static [&'static str] {
        &[
            "refresh_readbacks",
            "refresh_status",
            "refresh_encoders",
            "refresh_8bit_movement_status",
            "refresh_move_status",
            "refresh_assignments",
            "refresh_wait_time",
            "refresh_temperature",
            "refresh_mode",
            "refresh_rotations",
            "refresh_mmt_state",
            "stop",
        ]
    }

    pub fn is_generic_command(command: &str) -> bool {
        generic_command_names().contains(&command)
    }
}

trait MclUsbIo: Send {
    fn control_in(&mut self, request: u8, value: u16, index: u16, len: usize) -> Result<Vec<u8>>;
    fn bulk_in(&mut self, endpoint: u8, len: usize) -> Result<Vec<u8>>;
}

#[derive(Debug, Clone)]
struct MclUsbIdentity {
    product: String,
    serial: Option<String>,
    vendor_id: u16,
    product_id: u16,
    bus_number: u8,
    device_address: u8,
    family: String,
}

impl MclUsbIdentity {
    fn value(&self) -> Value {
        let mut fields = BTreeMap::from([
            ("product".into(), Value::String(self.product.clone())),
            ("family".into(), Value::String(self.family.clone())),
            ("vendor_id".into(), Value::I64(self.vendor_id as i64)),
            ("product_id".into(), Value::I64(self.product_id as i64)),
            (
                "vendor_id_hex".into(),
                Value::String(format!("0x{:04x}", self.vendor_id)),
            ),
            (
                "product_id_hex".into(),
                Value::String(format!("0x{:04x}", self.product_id)),
            ),
            ("bus_number".into(), Value::I64(self.bus_number as i64)),
            (
                "device_address".into(),
                Value::I64(self.device_address as i64),
            ),
        ]);
        if let Some(serial) = &self.serial {
            fields.insert("serial".into(), Value::String(serial.clone()));
        }
        Value::Map(fields)
    }
}

#[derive(Debug, Clone)]
pub struct MclProbe {
    label: String,
    product: String,
    serial_number: Option<String>,
    family: String,
    vendor_id: u16,
    product_id: u16,
    interface: u8,
    in_endpoint: u8,
    connect_real_transport: bool,
    axis_count: i64,
    raw_status: u16,
    encoder_counts: [i32; 8],
    vendor_runtime_path: Option<String>,
    vendor_runtime_sha256: Option<String>,
    firmware_blob_path: Option<String>,
    firmware_blob_sha256: Option<String>,
    load_vendor_runtime: bool,
    read_firmware_blob: bool,
    usb_identity: Option<MclUsbIdentity>,
}

pub struct MclDiscovery {
    next_id: DriverId,
    probes: Vec<MclProbe>,
    #[cfg(feature = "os-usb")]
    active_usb: bool,
}

impl MclDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![MclProbe::fixture()],
            #[cfg(feature = "os-usb")]
            active_usb: false,
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| {
                matches!(
                    device.driver.as_str(),
                    "mcl" | "mcl_microdrive" | "mcl-nanodrive" | "mcl_nanodrive"
                )
            })
            .map(MclProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            next_id,
            probes,
            #[cfg(feature = "os-usb")]
            active_usb: false,
        })
    }

    #[cfg(feature = "os-usb")]
    pub fn os_usb(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: Vec::new(),
            active_usb: true,
        }
    }
}

impl DriverDiscovery for MclDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        let mut probes = std::mem::take(&mut self.probes);
        #[cfg(feature = "os-usb")]
        if self.active_usb {
            probes.extend(active_usb_probes()?);
        }

        probes
            .drain(..)
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = format!("{} ({})", probe.label, probe.product);
                let driver: Box<dyn Driver> = if probe.connect_real_transport {
                    Box::new(MclDriver::usb(id, probe)?)
                } else {
                    Box::new(MclDriver::configured(id, probe))
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

impl MclProbe {
    fn fixture() -> Self {
        Self {
            label: "Configured MCL reverse engineered support".into(),
            product: "Mad City Labs MicroDrive/NanoDrive".into(),
            serial_number: None,
            family: "unknown".into(),
            vendor_id: protocol::MCL_VENDOR_ID,
            product_id: 0x2588,
            interface: 0,
            in_endpoint: protocol::MICRODRIVE_GLOBAL_IN_ENDPOINT,
            connect_real_transport: false,
            axis_count: 0,
            raw_status: 0,
            encoder_counts: [0; 8],
            vendor_runtime_path: None,
            vendor_runtime_sha256: None,
            firmware_blob_path: None,
            firmware_blob_sha256: None,
            load_vendor_runtime: false,
            read_firmware_blob: false,
            usb_identity: None,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = Self::fixture();
        if !device.label.is_empty() {
            probe.label = device.label.clone();
        }
        probe.product = string_prop(device, "product").unwrap_or(probe.product);
        probe.serial_number = optional_string_prop(device, "serial_number", probe.serial_number);
        probe.family = string_prop(device, "family").unwrap_or(probe.family);
        probe.vendor_id = u16_prop(device, "vendor_id").unwrap_or(probe.vendor_id);
        probe.product_id = u16_prop(device, "product_id").unwrap_or(probe.product_id);
        probe.interface = u8_prop(device, "interface").unwrap_or(probe.interface);
        probe.in_endpoint = u8_prop(device, "in_endpoint").unwrap_or(probe.in_endpoint);
        probe.connect_real_transport = bool_prop(device, "connect").unwrap_or(false);
        if probe.connect_real_transport && !protocol::is_microdrive_product(probe.product_id) {
            return Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "MCL connect=true currently supports MicroDrive product IDs only; got 0x{:04x}",
                    probe.product_id
                ),
            ));
        }
        probe.axis_count = i64_prop(device, "axis_count")
            .unwrap_or(probe.axis_count)
            .clamp(0, 5);
        probe.raw_status = u16_prop(device, "raw_status").unwrap_or(probe.raw_status);
        probe.vendor_runtime_path =
            optional_string_prop(device, "vendor_runtime_path", probe.vendor_runtime_path);
        probe.vendor_runtime_sha256 =
            optional_string_prop(device, "vendor_runtime_sha256", probe.vendor_runtime_sha256);
        probe.firmware_blob_path =
            optional_string_prop(device, "firmware_blob_path", probe.firmware_blob_path);
        probe.firmware_blob_sha256 =
            optional_string_prop(device, "firmware_blob_sha256", probe.firmware_blob_sha256);
        probe.load_vendor_runtime =
            bool_prop(device, "load_vendor_runtime").unwrap_or(probe.load_vendor_runtime);
        probe.read_firmware_blob =
            bool_prop(device, "read_firmware_blob").unwrap_or(probe.read_firmware_blob);
        for index in 0..probe.encoder_counts.len() {
            let key = format!("encoder_count_{}", index + 1);
            if let Some(value) = i64_prop(device, &key) {
                probe.encoder_counts[index] = value.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
            }
        }
        Ok(probe)
    }
}

#[cfg(feature = "os-usb")]
fn active_usb_probes() -> Result<Vec<MclProbe>> {
    let devices = nusb::list_devices().map_err(|error| {
        Error::new(
            ErrorCode::Transport,
            format!("MCL USB device listing failed: {error}"),
        )
    })?;
    Ok(devices
        .filter(|device| protocol::is_mcl_candidate(device.vendor_id(), device.product_id()))
        .map(|device| {
            let vendor_id = device.vendor_id();
            let product_id = device.product_id();
            let family = protocol::family_for_product(vendor_id, product_id);
            let product = device
                .product_string()
                .map(str::to_string)
                .unwrap_or_else(|| mcl_product_name(family).into());
            let serial_number = device.serial_number().map(str::to_string);
            let label = format!(
                "MCL {} {:04x}:{:04x} bus {} addr {}",
                family,
                vendor_id,
                product_id,
                device.bus_number(),
                device.device_address()
            );
            MclProbe {
                label,
                product: product.clone(),
                serial_number: serial_number.clone(),
                family: family.into(),
                vendor_id,
                product_id,
                interface: 0,
                in_endpoint: protocol::MICRODRIVE_GLOBAL_IN_ENDPOINT,
                connect_real_transport: false,
                axis_count: 0,
                raw_status: 0,
                encoder_counts: [0; 8],
                vendor_runtime_path: None,
                vendor_runtime_sha256: None,
                firmware_blob_path: None,
                firmware_blob_sha256: None,
                load_vendor_runtime: false,
                read_firmware_blob: false,
                usb_identity: Some(MclUsbIdentity {
                    product,
                    serial: serial_number,
                    vendor_id,
                    product_id,
                    bus_number: device.bus_number(),
                    device_address: device.device_address(),
                    family: family.into(),
                }),
            }
        })
        .collect())
}

#[cfg(feature = "os-usb")]
fn mcl_product_name(family: &str) -> &'static str {
    match family {
        "microdrive" => "Mad City Labs MicroDrive",
        "nanodrive" => "Mad City Labs NanoDrive",
        "prefirmware" => "Mad City Labs pre-firmware USB device",
        _ => "Mad City Labs USB device",
    }
}

pub struct MclDriver {
    id: DriverId,
    hub: DeviceId,
    axes: Vec<DeviceId>,
    usb: ResourceId,
    probe: MclProbe,
    io: Option<Box<dyn MclUsbIo>>,
    next_token: u64,
    events: VecDeque<DriverEvent>,
}

impl MclDriver {
    pub fn configured(id: DriverId, probe: MclProbe) -> Self {
        Self {
            id,
            hub: DeviceId(NodeId(id.0 * 1000 + 730)),
            axes: (0..probe.axis_count.max(0) as u64)
                .map(|index| DeviceId(NodeId(id.0 * 1000 + 732 + index)))
                .collect(),
            usb: ResourceId(NodeId(id.0 * 1000 + 731)),
            probe,
            io: None,
            next_token: 1,
            events: VecDeque::new(),
        }
    }

    #[cfg(feature = "os-usb")]
    pub fn usb(id: DriverId, probe: MclProbe) -> Result<Self> {
        let io = live_mcl::LiveMclUsb::open(&probe)?;
        let mut driver = Self::configured(id, probe);
        driver.io = Some(Box::new(io));
        driver.refresh_microdrive_readbacks()?;
        Ok(driver)
    }

    #[cfg(not(feature = "os-usb"))]
    pub fn usb(_id: DriverId, _probe: MclProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "MCL real USB readback requires the numanager-drivers os-usb feature",
        ))
    }

    fn descriptor(&self) -> DeviceDescriptor {
        DeviceDescriptor {
            id: self.hub,
            driver: self.id,
            label: self.probe.label.clone(),
            vendor: Some("Mad City Labs".into()),
            model: Some(self.probe.product.clone()),
            serial: self.probe.serial_number.clone(),
            kinds: vec![
                "hub".into(),
                "motion.controller".into(),
                "reverse.engineered".into(),
            ],
            properties: vec![
                string_property("model", "Model"),
                string_property("serial_number", "Serial number"),
                string_property("family", "Family"),
                property("axis_count", "Axis count", ValueType::I64),
                property("vendor_id", "USB vendor ID", ValueType::I64),
                property("product_id", "USB product ID", ValueType::I64),
                property("usb_identity", "USB identity", ValueType::Map),
                property("connected", "Connected", ValueType::Bool),
                string_property("support_level", "Support level"),
                string_property("motion_gate", "Motion gate"),
                string_property("status_gate", "Status gate"),
                property("raw_status", "Raw status", ValueType::I64),
                property("encoder_summary", "Encoder summary", ValueType::Map),
                string_property("vendor_runtime_path", "Vendor runtime path"),
                string_property("vendor_runtime_sha256", "Vendor runtime SHA-256"),
                property(
                    "load_vendor_runtime",
                    "Load vendor runtime",
                    ValueType::Bool,
                ),
                string_property("vendor_runtime_file_status", "Vendor runtime file status"),
                string_property("vendor_runtime_digest_state", "Vendor runtime digest state"),
                property(
                    "vendor_runtime_file_size",
                    "Vendor runtime file size",
                    ValueType::ByteCount,
                ),
                string_property("vendor_runtime_probe_state", "Vendor runtime probe state"),
                string_property("firmware_blob_path", "Firmware package path"),
                string_property("firmware_blob_sha256", "Firmware package SHA-256"),
                property(
                    "read_firmware_blob",
                    "Firmware package read enabled",
                    ValueType::Bool,
                ),
                string_property("firmware_blob_file_status", "Firmware package file status"),
                string_property(
                    "firmware_blob_digest_state",
                    "Firmware package digest state",
                ),
                property(
                    "firmware_blob_file_size",
                    "Firmware package file size",
                    ValueType::ByteCount,
                ),
                string_property("firmware_blob_probe_state", "Firmware package probe state"),
                string_property("package_strategy", "Package strategy"),
                string_property("vendor_runtime_state", "Vendor runtime state"),
                string_property("firmware_package_state", "Firmware package state"),
                property("feature_summary", "Feature summary", ValueType::Map),
            ],
            metadata: {
                let mut metadata = BTreeMap::from([
                (
                    "support_level".into(),
                    Value::String(
                        "MCL active USB descriptor discovery, raw MicroDrive status/encoder readback, documented raw MicroDrive control-read/action commands, and firmware/runtime package checks".into(),
                    ),
                ),
                (
                    "active_usb_detected".into(),
                    Value::Bool(self.probe.usb_identity.is_some()),
                ),
                ("connected".into(), Value::Bool(self.io.is_some())),
                ("hardware_validated".into(), Value::Bool(false)),
                ]);
                if let Some(identity) = &self.probe.usb_identity {
                    metadata.insert("usb_identity".into(), identity.value());
                }
                metadata
            },
        }
    }

    fn read_property(&self, key: &str) -> Result<Value> {
        match key {
            "model" => Ok(Value::String(self.probe.product.clone())),
            "serial_number" => Ok(Value::String(
                self.probe.serial_number.clone().unwrap_or_default(),
            )),
            "family" => Ok(Value::String(self.probe.family.clone())),
            "axis_count" => Ok(Value::I64(self.probe.axis_count)),
            "vendor_id" => Ok(Value::I64(self.probe.vendor_id as i64)),
            "product_id" => Ok(Value::I64(self.probe.product_id as i64)),
            "usb_identity" => Ok(self.usb_identity_value()),
            "connected" => Ok(Value::Bool(self.io.is_some())),
            "support_level" => Ok(Value::String(
                "active USB descriptor discovery, raw MicroDrive encoder/status readback, documented raw MicroDrive control-read/action commands, and firmware/runtime package checks; typed motion is not exposed because units, status meanings, and completion behavior evidence is absent".into(),
            )),
            "motion_gate" => Ok(Value::String(
                "typed motion is not exposed because move payloads, units, and limits are not evidenced".into(),
            )),
            "status_gate" => Ok(Value::String(
                "raw status word is exposed, but per-bit busy/fault/limit meanings are unknown".into(),
            )),
            "raw_status" => Ok(Value::I64(self.probe.raw_status as i64)),
            "encoder_summary" => Ok(self.encoder_summary()),
            "vendor_runtime_path" => Ok(Value::String(
                self.probe.vendor_runtime_path.clone().unwrap_or_default(),
            )),
            "vendor_runtime_sha256" => Ok(Value::String(
                self.probe.vendor_runtime_sha256.clone().unwrap_or_default(),
            )),
            "load_vendor_runtime" => Ok(Value::Bool(self.probe.load_vendor_runtime)),
            "vendor_runtime_file_status" => Ok(Value::String(Self::package_file_status(
                self.probe.vendor_runtime_path.as_deref(),
            ))),
            "vendor_runtime_digest_state" => Ok(Value::String(self.vendor_runtime_digest_state())),
            "vendor_runtime_file_size" => Self::package_file_size(
                self.probe.vendor_runtime_path.as_deref(),
            ),
            "vendor_runtime_probe_state" => Ok(Value::String(self.vendor_runtime_probe_state())),
            "firmware_blob_path" => Ok(Value::String(
                self.probe.firmware_blob_path.clone().unwrap_or_default(),
            )),
            "firmware_blob_sha256" => Ok(Value::String(
                self.probe.firmware_blob_sha256.clone().unwrap_or_default(),
            )),
            "read_firmware_blob" => Ok(Value::Bool(self.probe.read_firmware_blob)),
            "firmware_blob_file_status" => Ok(Value::String(Self::package_file_status(
                self.probe.firmware_blob_path.as_deref(),
            ))),
            "firmware_blob_digest_state" => Ok(Value::String(self.firmware_blob_digest_state())),
            "firmware_blob_file_size" => {
                Self::package_file_size(self.probe.firmware_blob_path.as_deref())
            },
            "firmware_blob_probe_state" => Ok(Value::String(self.firmware_blob_probe_state())),
            "package_strategy" => Ok(Value::String(self.package_strategy().into())),
            "vendor_runtime_state" => Ok(Value::String(Self::package_identity_state(
                self.probe.vendor_runtime_path.as_ref(),
                self.probe.vendor_runtime_sha256.as_ref(),
            )
            .into())),
            "firmware_package_state" => Ok(Value::String(self.firmware_package_state().into())),
            "feature_summary" => Ok(Value::Map(BTreeMap::from([
                ("usb_transport_identified".into(), Value::Bool(true)),
                (
                    "active_usb_detected".into(),
                    Value::Bool(self.probe.usb_identity.is_some()),
                ),
                ("encoder_payload_known".into(), Value::Bool(true)),
                ("status_word_layout_known".into(), Value::Bool(true)),
                (
                    "fixed_length_control_requests_known".into(),
                    Value::Bool(true),
                ),
                ("live_readback_connected".into(), Value::Bool(self.io.is_some())),
                (
                    "vendor_runtime_configured".into(),
                    Value::Bool(self.probe.vendor_runtime_path.is_some()),
                ),
                (
                    "vendor_runtime_backend_enabled".into(),
                    Value::Bool(self.probe.load_vendor_runtime),
                ),
                (
                    "vendor_runtime_digest_state".into(),
                    Value::String(self.vendor_runtime_digest_state()),
                ),
                (
                    "firmware_blob_configured".into(),
                    Value::Bool(self.probe.firmware_blob_path.is_some()),
                ),
                (
                    "firmware_blob_read_enabled".into(),
                    Value::Bool(self.probe.read_firmware_blob),
                ),
                (
                    "firmware_blob_digest_state".into(),
                    Value::String(self.firmware_blob_digest_state()),
                ),
                ("move_payloads_known".into(), Value::Bool(false)),
                ("units_known".into(), Value::Bool(false)),
                ("motion_supported".into(), Value::Bool(false)),
            ]))),
            _ => Err(Error::new(ErrorCode::InvalidProperty, "unknown MCL property")),
        }
    }

    fn package_identity_state(path: Option<&String>, sha256: Option<&String>) -> &'static str {
        match (path, sha256) {
            (Some(_), Some(_)) => "configured_with_digest",
            (Some(_), None) => "configured_without_digest",
            (None, Some(_)) => "digest_without_path",
            (None, None) => "not_configured",
        }
    }

    fn usb_identity_value(&self) -> Value {
        self.probe
            .usb_identity
            .as_ref()
            .map(MclUsbIdentity::value)
            .unwrap_or(Value::Null)
    }

    fn firmware_package_state(&self) -> &'static str {
        if !matches!(self.probe.product_id, 0x8613 | 0x2235) {
            return "not_required_for_configured_pid";
        }
        Self::package_identity_state(
            self.probe.firmware_blob_path.as_ref(),
            self.probe.firmware_blob_sha256.as_ref(),
        )
    }

    fn package_file_status(path: Option<&str>) -> String {
        let Some(path) = path else {
            return "not_configured".into();
        };
        match std::fs::metadata(Path::new(path)) {
            Ok(metadata) if metadata.is_file() => "present".into(),
            Ok(_) => "not_a_file".into(),
            Err(error) => format!("unavailable:{}", error.kind()),
        }
    }

    fn package_file_size(path: Option<&str>) -> Result<Value> {
        let Some(path) = path else {
            return Ok(Value::ByteCount(ByteCount::new(0)));
        };
        match std::fs::metadata(Path::new(path)) {
            Ok(metadata) if metadata.is_file() => {
                Ok(Value::ByteCount(ByteCount::new(metadata.len())))
            }
            Ok(_) => Ok(Value::ByteCount(ByteCount::new(0))),
            Err(error) => Err(Error::new(
                ErrorCode::Transport,
                format!("MCL package file metadata unavailable for {path}: {error}"),
            )),
        }
    }

    fn normalized_sha256(raw: &str) -> Option<String> {
        let mut digest = raw.trim();
        if let Some(stripped) = digest.strip_prefix("sha256:") {
            digest = stripped;
        }
        let digest = digest.replace([' ', ':', '-'], "").to_ascii_lowercase();
        if digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            Some(digest)
        } else {
            None
        }
    }

    fn package_sha256(path: &str) -> Result<String> {
        let file = std::fs::File::open(Path::new(path)).map_err(|error| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("MCL package file is unavailable for digest verification: {error}"),
            )
        })?;
        let mut reader = BufReader::new(file);
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let bytes = reader.read(&mut buffer).map_err(|error| {
                Error::new(
                    ErrorCode::Transport,
                    format!("MCL package digest read failed: {error}"),
                )
            })?;
            if bytes == 0 {
                break;
            }
            hasher.update(&buffer[..bytes]);
        }
        Ok(hex::encode(hasher.finalize()))
    }

    fn package_digest_state(path: Option<&str>, configured_sha256: Option<&str>) -> String {
        let Some(configured_sha256) = configured_sha256 else {
            return "not_configured".into();
        };
        let Some(expected) = Self::normalized_sha256(configured_sha256) else {
            return "invalid_configured_sha256".into();
        };
        let Some(path) = path else {
            return "digest_without_path".into();
        };
        match Self::package_sha256(path) {
            Ok(actual) if actual == expected => "verified".into(),
            Ok(actual) => format!("mismatch:{actual}"),
            Err(error) => format!("unavailable:{}", compact_error(&error.message)),
        }
    }

    fn package_digest_allows_use(path: Option<&str>, configured_sha256: Option<&str>) -> String {
        let Some(configured_sha256) = configured_sha256 else {
            return "missing_sha256".into();
        };
        let Some(expected) = Self::normalized_sha256(configured_sha256) else {
            return "invalid_configured_sha256".into();
        };
        let Some(path) = path else {
            return "missing_path".into();
        };
        match Self::package_sha256(path) {
            Ok(actual) if actual == expected => "verified".into(),
            Ok(_) => "digest_mismatch".into(),
            Err(error) => format!("digest_unavailable:{}", compact_error(&error.message)),
        }
    }

    fn vendor_runtime_digest_state(&self) -> String {
        Self::package_digest_state(
            self.probe.vendor_runtime_path.as_deref(),
            self.probe.vendor_runtime_sha256.as_deref(),
        )
    }

    fn firmware_blob_digest_state(&self) -> String {
        Self::package_digest_state(
            self.probe.firmware_blob_path.as_deref(),
            self.probe.firmware_blob_sha256.as_deref(),
        )
    }

    fn vendor_runtime_probe_state(&self) -> String {
        if !self.probe.load_vendor_runtime {
            return "disabled".into();
        }
        let digest_state = Self::package_digest_allows_use(
            self.probe.vendor_runtime_path.as_deref(),
            self.probe.vendor_runtime_sha256.as_deref(),
        );
        if digest_state != "verified" {
            return digest_state;
        }
        let Some(path) = self.probe.vendor_runtime_path.as_deref() else {
            return "missing_path".into();
        };
        if let Err(error) = std::fs::metadata(Path::new(path)) {
            return format!("file_unavailable:{}", error.kind());
        }

        // Loading is the explicit vendor-runtime boundary. No MCL ABI or
        // hardware operation is invoked by this read-only probe.
        match unsafe { Library::new(path) } {
            Ok(_library) => "loaded".into(),
            Err(error) => format!("load_error:{}", compact_error(&error.to_string())),
        }
    }

    fn firmware_blob_probe_state(&self) -> String {
        if !self.probe.read_firmware_blob {
            return "disabled".into();
        }
        let digest_state = Self::package_digest_allows_use(
            self.probe.firmware_blob_path.as_deref(),
            self.probe.firmware_blob_sha256.as_deref(),
        );
        if digest_state != "verified" {
            return digest_state;
        }
        let Some(path) = self.probe.firmware_blob_path.as_deref() else {
            return "missing_path".into();
        };
        let mut file = match std::fs::File::open(Path::new(path)) {
            Ok(file) => file,
            Err(error) => return format!("file_unavailable:{}", error.kind()),
        };
        let mut scratch = [0_u8; 4096];
        match file.read(&mut scratch) {
            Ok(bytes) => format!("readable:{bytes}"),
            Err(error) => format!("read_error:{}", compact_error(&error.to_string())),
        }
    }

    fn package_strategy(&self) -> &'static str {
        "interim third-party vendor firmware/runtime package; explicit read/load probes only until a project-owned firmware or open replacement exists"
    }

    fn next_token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn encoder_summary(&self) -> Value {
        Value::Map(
            self.probe
                .encoder_counts
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    (
                        format!("encoder_count_{}", index + 1),
                        Value::ControllerScalar(ControllerScalar::new(*value as i64)),
                    )
                })
                .collect(),
        )
    }

    fn axis_index(&self, device: DeviceId) -> Option<usize> {
        self.axes.iter().position(|candidate| *candidate == device)
    }

    fn axis_descriptor(&self, index: usize, id: DeviceId) -> DeviceDescriptor {
        let axis_name = match index {
            0 => "x",
            1 => "y",
            2 => "z",
            3 => "axis-4",
            _ => "axis-5",
        };
        DeviceDescriptor {
            id,
            driver: self.id,
            label: format!("mcl-{axis_name}"),
            vendor: Some("Mad City Labs".into()),
            model: Some(self.probe.product.clone()),
            serial: self
                .probe
                .serial_number
                .clone()
                .map(|serial| format!("{serial}:{axis_name}")),
            kinds: vec![
                "stage.axis".into(),
                format!("stage.{axis_name}"),
                "reverse.engineered".into(),
            ],
            properties: vec![
                property(
                    "raw_encoder_count",
                    "Raw encoder count",
                    ValueType::ControllerScalar,
                ),
                property("status_bits", "Status bits", ValueType::I64),
                string_property("position_gate", "Position gate"),
                string_property("motion_gate", "Motion gate"),
            ],
            metadata: BTreeMap::from([
                ("axis_index".into(), Value::I64((index + 1) as i64)),
                ("source".into(), Value::String("reverse engineered".into())),
                ("connected".into(), Value::Bool(self.io.is_some())),
            ]),
        }
    }

    fn refresh_microdrive_readbacks(&mut self) -> Result<()> {
        let Some(io) = self.io.as_mut() else {
            return Ok(());
        };
        let status_len = if protocol::microdrive_has_two_byte_status(self.probe.product_id) {
            2
        } else {
            1
        };
        let status = io.control_in(protocol::MICRODRIVE_STATUS_REQUEST, 0, 0, status_len)?;
        self.probe.raw_status = protocol::parse_status_word(&status)?;

        let encoder_payload = if self.probe.product_id == 0x2588 {
            io.control_in(
                protocol::MICRODRIVE_ENCODER_REQUEST,
                0,
                0,
                protocol::MICRODRIVE_ENCODER_LEN,
            )?
        } else {
            io.bulk_in(
                self.probe.in_endpoint,
                protocol::MICRODRIVE_BULK_ENCODER_LEN,
            )?
        };
        self.probe.encoder_counts = protocol::parse_microdrive_encoder_values(&encoder_payload)?;
        Ok(())
    }

    fn read_microdrive_control(
        &mut self,
        command: &str,
        request: u8,
        value: u16,
        len: usize,
    ) -> Result<Value> {
        let Some(io) = self.io.as_mut() else {
            return Ok(Value::Map(BTreeMap::from([
                ("command".into(), Value::String(command.into())),
                ("connected".into(), Value::Bool(false)),
                ("request".into(), Value::String(format!("0x{request:02x}"))),
                ("value".into(), Value::String(format!("0x{value:04x}"))),
                ("reply_len".into(), Value::I64(len as i64)),
                ("reply".into(), Value::Null),
            ])));
        };
        let reply = io.control_in(request, value, 0, len)?;
        let mut fields = BTreeMap::from([
            ("command".into(), Value::String(command.into())),
            ("connected".into(), Value::Bool(true)),
            ("request".into(), Value::String(format!("0x{request:02x}"))),
            ("value".into(), Value::String(format!("0x{value:04x}"))),
            ("reply_len".into(), Value::I64(len as i64)),
            (
                "reply".into(),
                Value::List(reply.iter().map(|byte| Value::I64(*byte as i64)).collect()),
            ),
            ("reply_hex".into(), Value::String(hex::encode(&reply))),
        ]);
        if request == protocol::MICRODRIVE_STATUS_REQUEST {
            self.probe.raw_status = protocol::parse_status_word(&reply)?;
            fields.insert(
                "raw_status".into(),
                Value::I64(self.probe.raw_status as i64),
            );
        }
        Ok(Value::Map(fields))
    }

    fn invoke_action_command(
        &mut self,
        command: &str,
        request: u8,
        value: u16,
        len: usize,
    ) -> Result<Value> {
        let mut result = match self.read_microdrive_control(command, request, value, len)? {
            Value::Map(fields) => fields,
            _ => BTreeMap::new(),
        };
        if self.io.is_some() {
            self.refresh_microdrive_readbacks()?;
        }
        result.insert(
            "raw_status".into(),
            Value::I64(self.probe.raw_status as i64),
        );
        result.insert("encoder_summary".into(), self.encoder_summary());
        Ok(Value::Map(result))
    }

    fn invoke_generic(&mut self, request: GenericCommandRequest) -> Result<Value> {
        Self::validate_generic_command(&request)?;
        if !request.params.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "MCL GenericCommand commands do not accept params",
            ));
        }
        let refreshed = match request.command.as_str() {
            "refresh_readbacks" => {
                self.refresh_microdrive_readbacks()?;
                vec!["raw_status", "encoder_summary"]
            }
            "refresh_status" => {
                self.refresh_microdrive_readbacks()?;
                vec!["raw_status"]
            }
            "refresh_encoders" => {
                self.refresh_microdrive_readbacks()?;
                vec!["encoder_summary"]
            }
            command => {
                let Some((usb_request, value, len)) =
                    protocol::fixed_length_control_command(command, self.probe.product_id)
                else {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        format!("unsupported MCL GenericCommand {command}"),
                    ));
                };
                if command == "stop" {
                    return self.invoke_action_command(command, usb_request, value, len);
                }
                return self.read_microdrive_control(command, usb_request, value, len);
            }
        };
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("connected".into(), Value::Bool(self.io.is_some())),
            (
                "refreshed".into(),
                Value::List(
                    refreshed
                        .into_iter()
                        .map(|key| Value::String(key.into()))
                        .collect(),
                ),
            ),
            (
                "raw_status".into(),
                Value::I64(self.probe.raw_status as i64),
            ),
            ("encoder_summary".into(), self.encoder_summary()),
        ])))
    }

    fn validate_generic_command(request: &GenericCommandRequest) -> Result<()> {
        if request.is_hidden_maintenance() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "MCL GenericCommand {} is a hidden maintenance operation",
                    request.command
                ),
            ));
        }
        if !protocol::is_generic_command(&request.command) {
            return Err(Error::new(
                ErrorCode::Unsupported,
                format!("unsupported MCL GenericCommand {}", request.command),
            ));
        }
        if !request.params.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "MCL GenericCommand commands do not accept params",
            ));
        }
        Ok(())
    }

    fn refresh_for_read(&mut self, device: DeviceId, key: &str) -> Result<()> {
        if self.io.is_none() {
            return Ok(());
        }
        let hub_readback = device == self.hub
            && matches!(key, "raw_status" | "encoder_summary" | "feature_summary");
        let axis_readback =
            self.axis_index(device).is_some() && matches!(key, "raw_encoder_count" | "status_bits");
        if hub_readback || axis_readback {
            self.refresh_microdrive_readbacks()?;
        }
        Ok(())
    }

    fn read_axis_property(&self, index: usize, key: &str) -> Result<Value> {
        match key {
            "raw_encoder_count" => Ok(Value::ControllerScalar(ControllerScalar::new(
                self.probe.encoder_counts[index] as i64,
            ))),
            "status_bits" => Ok(Value::I64(
                protocol::axis_status_bits(self.probe.raw_status, index) as i64,
            )),
            "position_gate" => Ok(Value::String(
                "encoder counts are exposed as native counts; position conversion is not exposed because scaling evidence is absent".into(),
            )),
            "motion_gate" => Ok(Value::String(
                "typed motion is not exposed because move payloads, units, limits, and completion are not evidenced"
                    .into(),
            )),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                "unknown MCL axis property",
            )),
        }
    }
}

impl Driver for MclDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        let mut descriptors = vec![self.descriptor()];
        descriptors.extend(
            self.axes
                .iter()
                .enumerate()
                .map(|(index, device)| self.axis_descriptor(index, *device)),
        );
        descriptors
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        let mut metadata = BTreeMap::from([
            (
                "protocol_note".into(),
                Value::String("docs/reverse/mcl-protocol.md".into()),
            ),
            ("vendor_id".into(), Value::I64(self.probe.vendor_id as i64)),
            (
                "product_id".into(),
                Value::I64(self.probe.product_id as i64),
            ),
            ("interface".into(), Value::I64(self.probe.interface as i64)),
            (
                "in_endpoint".into(),
                Value::I64(self.probe.in_endpoint as i64),
            ),
            (
                "usb_vendor_id".into(),
                Value::I64(self.probe.vendor_id as i64),
            ),
            (
                "usb_product_id".into(),
                Value::I64(self.probe.product_id as i64),
            ),
            (
                "usb_interface".into(),
                Value::I64(self.probe.interface as i64),
            ),
            (
                "usb_in_endpoint".into(),
                Value::I64(self.probe.in_endpoint as i64),
            ),
            (
                "active_usb_detected".into(),
                Value::Bool(self.probe.usb_identity.is_some()),
            ),
            ("connected".into(), Value::Bool(self.io.is_some())),
            (
                "readback_scope".into(),
                Value::String(
                    "raw MicroDrive status, encoder counts, and documented control-read replies"
                        .into(),
                ),
            ),
            (
                "vendor_runtime_digest_state".into(),
                Value::String(self.vendor_runtime_digest_state()),
            ),
            (
                "firmware_blob_digest_state".into(),
                Value::String(self.firmware_blob_digest_state()),
            ),
        ]);
        if let Some(identity) = &self.probe.usb_identity {
            metadata.insert("usb_identity".into(), identity.value());
        }

        vec![ResourceDescriptor {
            id: self.usb,
            driver: self.id,
            label: "MCL USB transport candidate".into(),
            kind: "usb.vendor".into(),
            metadata,
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.hub {
            vec![capability(1, device, CapabilityKind::GenericCommand)]
        } else {
            Vec::new()
        }
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut physical_transactions = Vec::new();
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } if *device == self.hub => {
                    let _ = self.read_property(key)?;
                    if matches!(
                        key.as_str(),
                        "raw_status" | "encoder_summary" | "feature_summary"
                    ) {
                        physical_transactions.push(PhysicalTransaction {
                            resource: Some(self.usb),
                            description: format!("mcl MicroDrive readback {key}"),
                            payload: Value::String(key.clone()),
                        });
                    }
                }
                Command::ReadProperty { device, key } => {
                    if let Some(index) = self.axis_index(*device) {
                        let _ = self.read_axis_property(index, key)?;
                        if matches!(key.as_str(), "raw_encoder_count" | "status_bits") {
                            physical_transactions.push(PhysicalTransaction {
                                resource: Some(self.usb),
                                description: format!(
                                    "mcl MicroDrive axis {} readback {key}",
                                    index + 1
                                ),
                                payload: Value::String(key.clone()),
                            });
                        }
                    }
                }
                Command::Invoke {
                    device,
                    request: CapabilityRequest::GenericCommand(request),
                    ..
                } if *device == self.hub => {
                    Self::validate_generic_command(request)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.usb),
                        description: format!("mcl documented {}", request.command),
                        payload: Value::String(request.command.clone()),
                    });
                }
                Command::WriteProperty { device, .. } if *device == self.hub => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "MCL typed motion/control is not exposed because payload and unit evidence is absent",
                    ));
                }
                Command::Invoke { device, .. } if *device == self.hub => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "MCL typed capabilities are not exposed because motion/status behavior evidence is absent",
                    ));
                }
                Command::WriteProperty { device, .. } if self.axis_index(*device).is_some() => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "MCL axis typed motion/control is not exposed because payload, unit, and completion evidence is absent",
                    ));
                }
                Command::Invoke { device, .. } if self.axis_index(*device).is_some() => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "MCL axis typed capabilities are not exposed because motion/status behavior evidence is absent",
                    ));
                }
                _ => {}
            }
        }
        Ok(PreparedBatch {
            id: batch.id,
            commands: batch.commands.clone(),
            physical_transactions,
        })
    }

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.next_token();
        let mut result = Value::Null;
        for command in prepared.commands {
            match command {
                Command::ReadProperty { device, key } if device == self.hub => {
                    self.refresh_for_read(device, &key)?;
                    result = self.read_property(&key)?;
                }
                Command::ReadProperty { device, key } => {
                    if let Some(index) = self.axis_index(device) {
                        self.refresh_for_read(device, &key)?;
                        result = self.read_axis_property(index, &key)?;
                    }
                }
                Command::Invoke {
                    device,
                    request: CapabilityRequest::GenericCommand(request),
                    ..
                } if device == self.hub => {
                    result = self.invoke_generic(request)?;
                }
                Command::WriteProperty { device, .. } if device == self.hub => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "MCL typed motion/control is not exposed because payload and unit evidence is absent",
                    ));
                }
                Command::Invoke { device, .. } if device == self.hub => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "MCL typed capabilities are not exposed because motion/status behavior evidence is absent",
                    ));
                }
                Command::WriteProperty { device, .. } if self.axis_index(device).is_some() => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "MCL axis typed motion/control is not exposed because payload, unit, and completion evidence is absent",
                    ));
                }
                Command::Invoke { device, .. } if self.axis_index(device).is_some() => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "MCL axis typed capabilities are not exposed because motion/status behavior evidence is absent",
                    ));
                }
                _ => {}
            }
        }
        self.events.push_back(DriverEvent::TokenCompleted {
            token,
            value: result,
        });
        Ok(token)
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        self.events.drain(..).collect()
    }
}

fn string_prop(device: &DeviceConfig, key: &str) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn optional_string_prop(
    device: &DeviceConfig,
    key: &str,
    fallback: Option<String>,
) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) if value.is_empty() || value == "none" => None,
        Some(Value::String(value)) => Some(value.clone()),
        Some(Value::Null) => None,
        _ => fallback,
    }
}

fn i64_prop(device: &DeviceConfig, key: &str) -> Option<i64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => Some(*value),
        _ => None,
    }
}

fn u16_prop(device: &DeviceConfig, key: &str) -> Option<u16> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => (*value).try_into().ok(),
        _ => None,
    }
}

fn u8_prop(device: &DeviceConfig, key: &str) -> Option<u8> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => (*value).try_into().ok(),
        _ => None,
    }
}

fn bool_prop(device: &DeviceConfig, key: &str) -> Option<bool> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn property(key: &str, display_name: &str, value_type: ValueType) -> PropertySchema {
    PropertySchema {
        key: key.into(),
        display_name: display_name.into(),
        value_type,
        unit: None,
        range: None,
        increment: None,
        enum_values: Vec::new(),
        readable: true,
        writable: false,
        volatile: false,
        sequenceable: false,
        hardware_address: None,
    }
}

fn string_property(key: &str, display_name: &str) -> PropertySchema {
    property(key, display_name, ValueType::String)
}

fn capability(id: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    CapabilityDescriptor::new(CapabilityId(id), device, kind, ValueType::Map)
}

fn compact_error(error: &str) -> String {
    error
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(feature = "os-usb")]
mod live_mcl {
    use super::*;
    use futures_lite::future::block_on;
    use nusb::transfer::{ControlIn, ControlType, Recipient, RequestBuffer};
    use nusb::Interface;

    pub struct LiveMclUsb {
        iface: Interface,
    }

    impl LiveMclUsb {
        pub fn open(probe: &MclProbe) -> Result<Self> {
            let device = nusb::list_devices()
                .map_err(|error| usb_error(error.to_string()))?
                .find(|device| {
                    device.vendor_id() == probe.vendor_id && device.product_id() == probe.product_id
                })
                .ok_or_else(|| {
                    Error::new(
                        ErrorCode::Transport,
                        format!(
                            "no MCL USB device found for {:04x}:{:04x}",
                            probe.vendor_id, probe.product_id
                        ),
                    )
                })?;
            let device = device.open().map_err(|error| {
                usb_error(format!(
                    "open MCL {:04x}:{:04x} failed: {error}",
                    probe.vendor_id, probe.product_id
                ))
            })?;
            let iface = device
                .detach_and_claim_interface(probe.interface)
                .map_err(|error| {
                    usb_error(format!(
                        "claim MCL USB interface {} failed: {error}",
                        probe.interface
                    ))
                })?;
            Ok(Self { iface })
        }
    }

    impl MclUsbIo for LiveMclUsb {
        fn control_in(
            &mut self,
            request: u8,
            value: u16,
            index: u16,
            len: usize,
        ) -> Result<Vec<u8>> {
            block_on(self.iface.control_in(ControlIn {
                control_type: ControlType::Vendor,
                recipient: Recipient::Device,
                request,
                value,
                index,
                length: len as u16,
            }))
            .into_result()
            .map_err(|error| {
                usb_error(format!(
                    "MCL control_in req=0x{request:02x} val=0x{value:04x} idx=0x{index:04x} failed: {error}"
                ))
            })
        }

        fn bulk_in(&mut self, endpoint: u8, len: usize) -> Result<Vec<u8>> {
            block_on(self.iface.bulk_in(endpoint, RequestBuffer::new(len)))
                .into_result()
                .map_err(|error| {
                    usb_error(format!(
                        "MCL bulk_in endpoint 0x{endpoint:02x} failed: {error}"
                    ))
                })
        }
    }

    fn usb_error(message: impl Into<String>) -> Error {
        Error::new(ErrorCode::Transport, message.into())
    }
}
