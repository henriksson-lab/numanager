use libloading::Library;
use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};
use std::io::{BufReader, Read};
use std::path::Path;

pub const ANDOR_VID: u16 = 0x136e;
pub const CYPRESS_VID: u16 = 0x04b4;
pub const CYPRESS_FX2_PID: u16 = 0x8613;
pub const CYPRESS_FX3_PID: u16 = 0x00f3;
/// USB vendor ids this driver claims. Hosts that need raw USB access
/// (udev rules on Linux) must cover these; see
/// `usb_discovery::builtin_usb_vendor_claims`.
pub fn usb_vendor_ids() -> Vec<u16> {
    vec![ANDOR_VID]
}

pub const SDK2_BULK_IN_ENDPOINT: u8 = 0x82;
pub const SDK2_BULK_OUT_ENDPOINT: u8 = 0x01;
pub const SDK2_STATUS_BULK_IN_ENDPOINT: u8 = 0x86;
pub const SDK2_READOUT_ALIGNMENT_PIXELS: u32 = 512;
pub const SDK2_READOUT_BYTES_PER_PIXEL: u32 = 2;
pub const SDK2_IDENTITY_REQUEST: u8 = 0xb7;
pub const SDK2_FIFO_RESET_REQUEST: u8 = 0xb4;
pub const SDK2_STATUS_REQUEST: u8 = 0xc7;
pub const SDK2_ACQUISITION_CONTROL_REQUEST: u8 = 0xc6;
pub const SDK2_ACQUISITION_START: u16 = 0x0001;
pub const SDK2_ACQUISITION_STOP: u16 = 0x0000;
pub const SDK2_ACQUISITION_CLEAR: u16 = 0x0003;
pub const SDK2_MIN_TEMPERATURE_C: i32 = -120;
pub const SDK2_MAX_TEMPERATURE_C: i32 = 30;
pub const SDK3_STATUS8_REQUEST: u8 = 0xfa;
pub const SDK3_STATUS32_REQUEST: u8 = 0xfd;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AndorSdkFamily {
    Sdk2,
    Sdk3,
    Unknown,
}

impl AndorSdkFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sdk2 => "Sdk2",
            Self::Sdk3 => "Sdk3",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AndorModel {
    EepromMissing,
    Idus,
    Newton,
    Luca,
    SurCam,
    UsbIStar,
    Pcu200,
    IKon,
    IVac,
    Clara,
    IXonUltra,
    IVacUltra,
    Zyla,
    IXonUltraUsb3,
    IStarScmos,
    Mosaic3,
    Scmos,
    Sona,
    Marana,
    Balor,
    Ccd,
    Unknown(u16),
}

impl AndorModel {
    pub fn from_pid(pid: u16) -> Self {
        match pid {
            0x0000 => Self::EepromMissing,
            0x0001 | 0x0004 => Self::Idus,
            0x0005 | 0x0006 => Self::Newton,
            0x0007 => Self::Luca,
            0x0008 => Self::SurCam,
            0x0009 | 0x000a | 0x000f => Self::UsbIStar,
            0x000b => Self::Pcu200,
            0x000c => Self::IKon,
            0x000d => Self::IVac,
            0x000e => Self::Clara,
            0x0011 => Self::IVacUltra,
            0x0012 => Self::IXonUltra,
            0x0014 => Self::Zyla,
            0x0015 => Self::IXonUltraUsb3,
            0x0018 => Self::IStarScmos,
            0x0019 => Self::Mosaic3,
            0x0020 => Self::Scmos,
            0x0021 => Self::Sona,
            0x0022 => Self::Marana,
            0x0023 => Self::Balor,
            0x0025 => Self::Ccd,
            other => Self::Unknown(other),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::EepromMissing => "Andor USB Cam (EEPROM missing)",
            Self::Idus => "Andor iDus",
            Self::Newton => "Andor Newton",
            Self::Luca => "Andor Luca",
            Self::SurCam => "Andor SurCam",
            Self::UsbIStar => "Andor USB iStar",
            Self::Pcu200 => "Andor PCU 200",
            Self::IKon => "Andor iKon",
            Self::IVac => "Andor iVac",
            Self::Clara => "Andor Clara",
            Self::IXonUltra => "Andor iXon Ultra",
            Self::IVacUltra => "Andor iVac Ultra",
            Self::Zyla => "Andor Zyla USB3",
            Self::IXonUltraUsb3 => "Andor iXon Ultra USB3",
            Self::IStarScmos => "Andor iStar-sCMOS",
            Self::Mosaic3 => "Andor Mosaic3",
            Self::Scmos => "Andor sCMOS Camera",
            Self::Sona => "Andor Sona",
            Self::Marana => "Andor Marana",
            Self::Balor => "Andor Balor",
            Self::Ccd => "Andor CCD Camera",
            Self::Unknown(_) => "Unknown Andor device",
        }
    }

    pub fn sdk_family(self) -> AndorSdkFamily {
        match self {
            Self::Zyla
            | Self::IStarScmos
            | Self::Mosaic3
            | Self::Scmos
            | Self::Sona
            | Self::Marana
            | Self::Balor => AndorSdkFamily::Sdk3,
            Self::EepromMissing | Self::Unknown(_) => AndorSdkFamily::Unknown,
            _ => AndorSdkFamily::Sdk2,
        }
    }
}

#[derive(Debug, Clone)]
struct AndorUsbIdentity {
    product: String,
    serial: Option<String>,
    vendor_id: u16,
    product_id: u16,
    bus_number: u8,
    device_address: u8,
    firmware_loaded: bool,
}

impl AndorUsbIdentity {
    fn value(&self) -> Value {
        let mut fields = BTreeMap::from([
            ("product".into(), Value::String(self.product.clone())),
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
            ("firmware_loaded".into(), Value::Bool(self.firmware_loaded)),
        ]);
        if let Some(serial) = &self.serial {
            fields.insert("serial".into(), Value::String(serial.clone()));
        }
        Value::Map(fields)
    }
}

#[derive(Debug, Clone)]
pub struct AndorCameraConfiguredProbe {
    label: String,
    vendor_id: u16,
    product_id: u16,
    product: String,
    serial_number: Option<String>,
    identity: Vec<u8>,
    status_byte: Option<u8>,
    sdk3_status_word: Option<u32>,
    firmware_loaded: bool,
    vendor_runtime_path: Option<String>,
    vendor_runtime_sha256: Option<String>,
    firmware_blob_path: Option<String>,
    firmware_blob_sha256: Option<String>,
    sdk_family_hint: Option<AndorSdkFamily>,
    load_vendor_runtime: bool,
    connect: bool,
    camera_index: i32,
    width: u32,
    height: u32,
    exposure: TimeInterval,
    frame_count: i64,
    pixel_format: String,
    cycle_mode: String,
    trigger_mode: String,
    sensor_cooling: bool,
    temperature_control: Option<String>,
    usb_identity: Option<AndorUsbIdentity>,
}

pub struct AndorCameraDiscovery {
    next_id: DriverId,
    probes: Vec<AndorCameraConfiguredProbe>,
    #[cfg(feature = "os-usb")]
    active_usb: bool,
}

impl AndorCameraDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![AndorCameraConfiguredProbe::fixture()],
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
                    "andor"
                        | "andor_camera"
                        | "andor-camera"
                        | "andor_sdk2"
                        | "andor-sdk2"
                        | "andor_sdk3"
                        | "andor-sdk3"
                )
            })
            .map(AndorCameraConfiguredProbe::from_device_config)
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

impl DriverDiscovery for AndorCameraDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        #[cfg(not(feature = "os-usb"))]
        let probes = self.probes.clone();
        #[cfg(feature = "os-usb")]
        let mut probes = self.probes.clone();
        #[cfg(feature = "os-usb")]
        if self.active_usb {
            probes.extend(active_usb_probes()?);
        }

        probes
            .iter()
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let mut driver = AndorCameraDriver::configured(id, probe.clone());
                driver.initialize_hidden_firmware()?;
                Ok(DriverCandidate::from_driver(
                    driver.configured.discovery_label(),
                    Box::new(driver),
                ))
            })
            .collect()
    }
}

impl AndorCameraConfiguredProbe {
    pub fn fixture() -> Self {
        let product_id = 0x0012;
        Self {
            label: "Configured Andor SDK2 camera".into(),
            vendor_id: ANDOR_VID,
            product_id,
            product: AndorModel::from_pid(product_id).name().into(),
            serial_number: Some("ANDOR-CONFIG-0001".into()),
            identity: vec![0, 0, 0, 0, 0, 0],
            status_byte: Some(0),
            sdk3_status_word: None,
            firmware_loaded: true,
            vendor_runtime_path: None,
            vendor_runtime_sha256: None,
            firmware_blob_path: None,
            firmware_blob_sha256: None,
            sdk_family_hint: None,
            load_vendor_runtime: false,
            connect: false,
            camera_index: 0,
            width: 512,
            height: 512,
            exposure: TimeInterval::from_seconds(0.01),
            frame_count: 1,
            pixel_format: "Mono16".into(),
            cycle_mode: "Fixed".into(),
            trigger_mode: "Internal".into(),
            sensor_cooling: false,
            temperature_control: None,
            usb_identity: None,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::fixture();
        if !device.label.is_empty() {
            configured.label = device.label.clone();
        }
        configured.vendor_id = u16_prop(device, "vendor_id")?.unwrap_or(configured.vendor_id);
        configured.product_id = u16_prop(device, "product_id")?.unwrap_or(configured.product_id);
        configured.product = string_prop(device, "product")
            .unwrap_or_else(|| AndorModel::from_pid(configured.product_id).name().into());
        configured.serial_number =
            optional_string_prop(device, "serial_number", configured.serial_number);
        configured.identity = bytes_prop(device, "identity")?.unwrap_or(configured.identity);
        configured.status_byte = optional_u8_prop(device, "status_byte", configured.status_byte)?;
        configured.sdk3_status_word =
            optional_u32_prop(device, "sdk3_status_word", configured.sdk3_status_word)?;
        configured.firmware_loaded =
            bool_prop(device, "firmware_loaded")?.unwrap_or(configured.firmware_loaded);
        configured.vendor_runtime_path = optional_string_prop(
            device,
            "vendor_runtime_path",
            configured.vendor_runtime_path,
        );
        configured.vendor_runtime_sha256 = optional_string_prop(
            device,
            "vendor_runtime_sha256",
            configured.vendor_runtime_sha256,
        );
        configured.firmware_blob_path =
            optional_string_prop(device, "firmware_blob_path", configured.firmware_blob_path);
        configured.firmware_blob_sha256 = optional_string_prop(
            device,
            "firmware_blob_sha256",
            configured.firmware_blob_sha256,
        );
        configured.sdk_family_hint = match device.driver.as_str() {
            "andor_sdk2" | "andor-sdk2" => Some(AndorSdkFamily::Sdk2),
            "andor_sdk3" | "andor-sdk3" => Some(AndorSdkFamily::Sdk3),
            _ => configured.sdk_family_hint,
        };
        configured.load_vendor_runtime =
            bool_prop(device, "load_vendor_runtime")?.unwrap_or(configured.load_vendor_runtime);
        configured.connect = bool_prop(device, "connect")?.unwrap_or(configured.connect);
        configured.camera_index =
            i32_prop(device, "camera_index")?.unwrap_or(configured.camera_index);
        configured.width = pixel_count_prop(device, "width")?.unwrap_or(configured.width);
        configured.height = pixel_count_prop(device, "height")?.unwrap_or(configured.height);
        configured.exposure =
            time_interval_prop(device, "exposure")?.unwrap_or(configured.exposure);
        configured.frame_count =
            positive_i64_prop(device, "frame_count")?.unwrap_or(configured.frame_count);
        configured.pixel_format =
            string_prop(device, "pixel_format").unwrap_or(configured.pixel_format);
        configured.cycle_mode = string_prop(device, "cycle_mode").unwrap_or(configured.cycle_mode);
        configured.trigger_mode =
            string_prop(device, "trigger_mode").unwrap_or(configured.trigger_mode);
        configured.sensor_cooling =
            bool_prop(device, "sensor_cooling")?.unwrap_or(configured.sensor_cooling);
        configured.temperature_control = optional_string_prop(
            device,
            "temperature_control",
            configured.temperature_control,
        );
        Ok(configured)
    }

    fn discovery_label(&self) -> String {
        let normalized = self.label.to_ascii_lowercase();
        let label = if normalized.contains("andor sdk2") || normalized.contains("andor sdk3") {
            self.label.clone()
        } else {
            format!("{} {}", self.sdk_family_label(), self.label)
        };
        format!("{} ({:04x}:{:04x})", label, self.vendor_id, self.product_id)
    }

    fn sdk_family(&self) -> AndorSdkFamily {
        self.sdk_family_hint
            .unwrap_or_else(|| AndorModel::from_pid(self.product_id).sdk_family())
    }

    fn sdk_family_label(&self) -> &'static str {
        match self.sdk_family() {
            AndorSdkFamily::Sdk2 => "Andor SDK2",
            AndorSdkFamily::Sdk3 => "Andor SDK3",
            AndorSdkFamily::Unknown => "Andor",
        }
    }
}

#[cfg(feature = "os-usb")]
fn active_usb_probes() -> Result<Vec<AndorCameraConfiguredProbe>> {
    let devices = nusb::list_devices().map_err(|error| {
        Error::new(
            ErrorCode::Transport,
            format!("Andor USB device listing failed: {error}"),
        )
    })?;
    Ok(devices
        .filter(|device| is_andor_usb_candidate(device.vendor_id(), device.product_id()))
        .map(|device| {
            let vendor_id = device.vendor_id();
            let product_id = device.product_id();
            let firmware_loaded = vendor_id == ANDOR_VID;
            let model = AndorModel::from_pid(product_id);
            let product = device
                .product_string()
                .map(str::to_string)
                .unwrap_or_else(|| andor_usb_product_name(vendor_id, product_id).into());
            let serial_number = device.serial_number().map(str::to_string);
            let label = format!(
                "{} {:04x}:{:04x} bus {} addr {}",
                product,
                vendor_id,
                product_id,
                device.bus_number(),
                device.device_address()
            );
            AndorCameraConfiguredProbe {
                label,
                vendor_id,
                product_id,
                product: if vendor_id == ANDOR_VID {
                    model.name().into()
                } else {
                    product.clone()
                },
                serial_number: serial_number.clone(),
                identity: Vec::new(),
                status_byte: None,
                sdk3_status_word: None,
                firmware_loaded,
                vendor_runtime_path: None,
                vendor_runtime_sha256: None,
                firmware_blob_path: None,
                firmware_blob_sha256: None,
                sdk_family_hint: match (vendor_id, product_id) {
                    (CYPRESS_VID, CYPRESS_FX2_PID) => Some(AndorSdkFamily::Sdk2),
                    (CYPRESS_VID, CYPRESS_FX3_PID) => Some(AndorSdkFamily::Sdk3),
                    _ => None,
                },
                load_vendor_runtime: false,
                connect: false,
                camera_index: 0,
                width: 512,
                height: 512,
                exposure: TimeInterval::from_seconds(0.01),
                frame_count: 1,
                pixel_format: "Mono16".into(),
                cycle_mode: "Fixed".into(),
                trigger_mode: "Internal".into(),
                sensor_cooling: false,
                temperature_control: None,
                usb_identity: Some(AndorUsbIdentity {
                    product,
                    serial: serial_number,
                    vendor_id,
                    product_id,
                    bus_number: device.bus_number(),
                    device_address: device.device_address(),
                    firmware_loaded,
                }),
            }
        })
        .collect())
}

#[cfg(feature = "os-usb")]
fn is_andor_usb_candidate(vendor_id: u16, product_id: u16) -> bool {
    let _ = product_id;
    vendor_id == ANDOR_VID
}

#[cfg(feature = "os-usb")]
fn andor_usb_product_name(vendor_id: u16, product_id: u16) -> &'static str {
    match (vendor_id, product_id) {
        (ANDOR_VID, pid) => AndorModel::from_pid(pid).name(),
        _ => "Andor USB device",
    }
}

pub struct AndorCameraDriver {
    id: DriverId,
    hub: DeviceId,
    camera: DeviceId,
    cooler: DeviceId,
    control: ResourceId,
    frame_bulk_in: ResourceId,
    vendor_runtime: ResourceId,
    configured: AndorCameraConfiguredProbe,
    next_token: u64,
    events: VecDeque<DriverEvent>,
}

impl AndorCameraDriver {
    pub fn configured(id: DriverId, configured: AndorCameraConfiguredProbe) -> Self {
        Self {
            id,
            hub: DeviceId(NodeId(id.0 * 1000 + 940)),
            camera: DeviceId(NodeId(id.0 * 1000 + 941)),
            cooler: DeviceId(NodeId(id.0 * 1000 + 942)),
            control: ResourceId(NodeId(id.0 * 1000 + 943)),
            frame_bulk_in: ResourceId(NodeId(id.0 * 1000 + 944)),
            vendor_runtime: ResourceId(NodeId(id.0 * 1000 + 946)),
            configured,
            next_token: 1,
            events: VecDeque::new(),
        }
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn initialize_hidden_firmware(&mut self) -> Result<()> {
        if !self.configured.connect || self.configured.firmware_loaded {
            return Ok(());
        }
        let digest_state = Self::package_digest_allows_use(
            self.configured.firmware_blob_path.as_deref(),
            self.configured.firmware_blob_sha256.as_deref(),
        );
        if digest_state != "verified" {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Andor firmware package is not verified: {digest_state}"),
            ));
        }
        let path = self
            .configured
            .firmware_blob_path
            .as_deref()
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    "Andor firmware package path is required for hidden firmware initialization",
                )
            })?;
        #[cfg(feature = "os-usb")]
        {
            match self.sdk_family() {
                AndorSdkFamily::Sdk2 => {
                    live_sdk2::upload_fx2_firmware(&self.configured.usb_identity, path)?
                }
                AndorSdkFamily::Sdk3 => {
                    live_sdk2::upload_fx3_firmware(&self.configured.usb_identity, path)?
                }
                AndorSdkFamily::Unknown => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Andor hidden firmware initialization requires SDK2 or SDK3 family",
                    ))
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(1500));
            if let Some(identity) =
                live_sdk2::select_loaded_andor_runtime(self.configured.product_id)?
            {
                self.configured.vendor_id = identity.vendor_id;
                self.configured.product_id = identity.product_id;
                self.configured.product = identity.product.clone();
                self.configured.serial_number = identity.serial.clone();
                self.configured.usb_identity = Some(identity);
            } else {
                return Err(Error::new(
                    ErrorCode::Transport,
                    "Andor firmware probe did not produce an Andor runtime USB device",
                ));
            }
            self.configured.firmware_loaded = true;
            if let Some(identity) = self.configured.usb_identity.as_mut() {
                identity.firmware_loaded = true;
            }
            Ok(())
        }
        #[cfg(not(feature = "os-usb"))]
        {
            let _ = path;
            Err(Error::new(
                ErrorCode::Unsupported,
                "Andor hidden firmware initialization requires numanager-drivers/os-usb",
            ))
        }
    }

    fn model(&self) -> AndorModel {
        AndorModel::from_pid(self.configured.product_id)
    }

    fn sdk_family(&self) -> AndorSdkFamily {
        self.configured
            .sdk_family_hint
            .unwrap_or_else(|| self.model().sdk_family())
    }

    fn descriptors_inner(&self) -> Vec<DeviceDescriptor> {
        vec![
            self.hub_descriptor(),
            self.camera_descriptor(),
            self.cooler_descriptor(),
        ]
    }

    fn hub_descriptor(&self) -> DeviceDescriptor {
        DeviceDescriptor {
            id: self.hub,
            driver: self.id,
            label: format!("{} hub", self.configured.label),
            vendor: Some("Andor".into()),
            model: Some(self.configured.product.clone()),
            serial: self.configured.serial_number.clone(),
            kinds: vec![
                "hub".into(),
                "usb.camera".into(),
                "camera.controller".into(),
            ],
            properties: vec![
                string_property("product", "Product"),
                string_property("serial_number", "Serial number"),
                string_property("sdk_family", "SDK family"),
                property("vendor_id", "USB vendor ID", ValueType::I64),
                property("product_id", "USB product ID", ValueType::I64),
                property("usb_identity", "USB identity", ValueType::Map),
                property("identity", "Identity block", ValueType::Bytes),
                property("status_byte", "Status byte", ValueType::I64),
                property("sdk3_status_word", "SDK3 status word", ValueType::I64),
                string_property("vendor_runtime_path", "Vendor runtime path"),
                string_property("vendor_runtime_sha256", "Vendor runtime SHA-256"),
                property(
                    "load_vendor_runtime",
                    "Load vendor runtime",
                    ValueType::Bool,
                ),
                property("camera_index", "Camera index", ValueType::I64),
                string_property("vendor_runtime_file_status", "Vendor runtime file status"),
                string_property("vendor_runtime_digest_state", "Vendor runtime digest state"),
                property(
                    "vendor_runtime_file_size",
                    "Vendor runtime file size",
                    ValueType::ByteCount,
                ),
                string_property("vendor_runtime_probe_state", "Vendor runtime probe state"),
                string_property("vendor_runtime_abi_state", "Vendor runtime ABI state"),
                string_property("package_strategy", "Package strategy"),
                string_property("vendor_runtime_state", "Vendor runtime state"),
                string_property("package_gate", "Package gate"),
                string_property("third_party_notice", "Third-party notice"),
                string_property("support_level", "Support level"),
            ],
            metadata: self.shared_metadata(),
        }
    }

    fn camera_descriptor(&self) -> DeviceDescriptor {
        let mut properties = vec![
            string_property("product", "Product"),
            string_property("serial_number", "Serial number"),
            string_property("sdk_family", "SDK family"),
        ];
        if matches!(
            self.sdk_family(),
            AndorSdkFamily::Sdk2 | AndorSdkFamily::Sdk3
        ) {
            properties.extend([
                property("width", "Width", ValueType::PixelCount),
                property("height", "Height", ValueType::PixelCount),
            ]);
            if self.sdk_family() == AndorSdkFamily::Sdk3 {
                properties.push(writable_property(
                    "pixel_format",
                    "Pixel format",
                    ValueType::String,
                ));
            } else {
                properties.push(property("pixel_format", "Pixel format", ValueType::String));
            }
        }
        if matches!(
            self.sdk_family(),
            AndorSdkFamily::Sdk2 | AndorSdkFamily::Sdk3
        ) {
            properties.push(writable_property(
                "exposure",
                "Exposure",
                ValueType::TimeInterval,
            ));
        }
        if self.sdk_family() == AndorSdkFamily::Sdk3 {
            properties.extend([
                writable_property("frame_count", "Frame count", ValueType::I64),
                writable_property("cycle_mode", "Cycle mode", ValueType::String),
                writable_property("trigger_mode", "Trigger mode", ValueType::String),
            ]);
        }
        if self.sdk_family() == AndorSdkFamily::Sdk2 {
            properties.extend([
                property("frame_endpoint", "Frame endpoint", ValueType::I64),
                property("status_endpoint", "Status endpoint", ValueType::I64),
                property("bulk_out_endpoint", "Bulk OUT endpoint", ValueType::I64),
                property(
                    "readout_alignment",
                    "Readout alignment",
                    ValueType::PixelCount,
                ),
                property(
                    "readout_bytes_per_pixel",
                    "Readout bytes per pixel",
                    ValueType::I64,
                ),
            ]);
        }
        properties.push(string_property("capture_gate", "Capture gate"));

        DeviceDescriptor {
            id: self.camera,
            driver: self.id,
            label: self.configured.label.clone(),
            vendor: Some("Andor".into()),
            model: Some(self.configured.product.clone()),
            serial: self.configured.serial_number.clone(),
            kinds: vec![
                "camera".into(),
                "camera.scientific".into(),
                "detector.mono".into(),
                match self.sdk_family() {
                    AndorSdkFamily::Sdk2 => "andor.sdk2",
                    AndorSdkFamily::Sdk3 => "andor.sdk3",
                    AndorSdkFamily::Unknown => "andor.unknown",
                }
                .into(),
            ],
            properties,
            metadata: self.shared_metadata(),
        }
    }

    fn cooler_descriptor(&self) -> DeviceDescriptor {
        DeviceDescriptor {
            id: self.cooler,
            driver: self.id,
            label: format!("{} cooler", self.configured.label),
            vendor: Some("Andor".into()),
            model: Some(self.configured.product.clone()),
            serial: self.configured.serial_number.clone(),
            kinds: vec![
                "temperature.controller".into(),
                "cooler".into(),
                "state.device".into(),
            ],
            properties: if matches!(
                self.sdk_family(),
                AndorSdkFamily::Sdk2 | AndorSdkFamily::Sdk3
            ) {
                vec![
                    writable_property("sensor_cooling", "Sensor cooling", ValueType::Bool),
                    writable_property(
                        "temperature_control",
                        "Temperature control",
                        ValueType::String,
                    ),
                    property(
                        "sensor_temperature",
                        "Sensor temperature",
                        ValueType::Temperature,
                    ),
                    string_property("temperature_status", "Temperature status"),
                    string_property("support_level", "Support level"),
                    string_property("cooler_gate", "Cooler gate"),
                ]
            } else {
                vec![
                    string_property("support_level", "Support level"),
                    string_property("cooler_gate", "Cooler gate"),
                ]
            },
            metadata: self.shared_metadata(),
        }
    }

    fn shared_metadata(&self) -> BTreeMap<String, Value> {
        let mut metadata = BTreeMap::from([
            (
                "sdk_free".into(),
                Value::Bool(!self.vendor_runtime_configured()),
            ),
            (
                "active_usb_detected".into(),
                Value::Bool(self.configured.usb_identity.is_some()),
            ),
            (
                "usb_stage".into(),
                Value::String(
                    if self.configured.firmware_loaded {
                        "runtime"
                    } else {
                        "pre_firmware"
                    }
                    .into(),
                ),
            ),
            (
                "usb_identity_confidence".into(),
                Value::String(
                    if self.configured.usb_identity.is_some() {
                        "exact"
                    } else if self.configured.connect {
                        "config_assumed"
                    } else {
                        "configured"
                    }
                    .into(),
                ),
            ),
            (
                "evidence_class".into(),
                Value::String("reverse engineered".into()),
            ),
            (
                "vendor_runtime_configured".into(),
                Value::Bool(self.vendor_runtime_configured()),
            ),
            (
                "vendor_runtime_backend_enabled".into(),
                Value::Bool(self.configured.load_vendor_runtime),
            ),
            (
                "vendor_runtime_state".into(),
                Value::String(self.vendor_runtime_state().into()),
            ),
            (
                "vendor_runtime_file_status".into(),
                Value::String(self.vendor_runtime_file_status()),
            ),
            (
                "vendor_runtime_digest_state".into(),
                Value::String(self.vendor_runtime_digest_state()),
            ),
            (
                "vendor_runtime_abi_state".into(),
                Value::String(self.vendor_runtime_abi_state()),
            ),
            ("connect".into(), Value::Bool(self.configured.connect)),
            (
                "support_level".into(),
                Value::String(self.support_level().into()),
            ),
            (
                "capture_gate".into(),
                Value::String(self.capture_gate().into()),
            ),
        ]);
        if let Some(identity) = &self.configured.usb_identity {
            metadata.insert("usb_identity".into(), identity.value());
        }
        metadata
    }

    fn vendor_runtime_configured(&self) -> bool {
        self.configured.vendor_runtime_path.is_some()
    }

    fn usb_identity_value(&self) -> Value {
        self.configured
            .usb_identity
            .as_ref()
            .map(AndorUsbIdentity::value)
            .unwrap_or(Value::Null)
    }

    fn vendor_runtime_state(&self) -> &'static str {
        match (
            self.configured.vendor_runtime_path.as_ref(),
            self.configured.vendor_runtime_sha256.as_ref(),
        ) {
            (Some(_), Some(_)) => "configured_with_digest",
            (Some(_), None) => "configured_without_digest",
            (None, Some(_)) => "digest_without_path",
            (None, None) => "not_configured",
        }
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
                format!("Andor package file is unavailable for digest verification: {error}"),
            )
        })?;
        let mut reader = BufReader::new(file);
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let bytes = reader.read(&mut buffer).map_err(|error| {
                Error::new(
                    ErrorCode::Transport,
                    format!("Andor package digest read failed: {error}"),
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

    fn package_file_size(path: Option<&str>, description: &str) -> Result<Value> {
        let Some(path) = path else {
            return Ok(Value::ByteCount(ByteCount::new(0)));
        };
        let metadata = std::fs::metadata(Path::new(path)).map_err(|error| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("Andor {description} file is unavailable: {error}"),
            )
        })?;
        if !metadata.is_file() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Andor {description} path is not a regular file"),
            ));
        }
        Ok(Value::ByteCount(ByteCount::new(metadata.len())))
    }

    fn vendor_runtime_file_status(&self) -> String {
        Self::package_file_status(self.configured.vendor_runtime_path.as_deref())
    }

    fn vendor_runtime_digest_state(&self) -> String {
        Self::package_digest_state(
            self.configured.vendor_runtime_path.as_deref(),
            self.configured.vendor_runtime_sha256.as_deref(),
        )
    }

    fn vendor_runtime_file_size(&self) -> Result<Value> {
        Self::package_file_size(
            self.configured.vendor_runtime_path.as_deref(),
            "vendor runtime",
        )
    }

    fn vendor_runtime_probe_state(&self) -> String {
        if !self.configured.load_vendor_runtime {
            return "disabled".into();
        }
        let digest_state = Self::package_digest_allows_use(
            self.configured.vendor_runtime_path.as_deref(),
            self.configured.vendor_runtime_sha256.as_deref(),
        );
        if digest_state != "verified" {
            return digest_state;
        }
        let Some(path) = self.configured.vendor_runtime_path.as_deref() else {
            return "missing_path".into();
        };
        if let Err(error) = std::fs::metadata(Path::new(path)) {
            return format!("file_unavailable:{}", error.kind());
        }

        // Loading is the explicit vendor-runtime boundary. No Andor ABI or
        // hardware operation is invoked by this read-only probe.
        match unsafe { Library::new(path) } {
            Ok(_library) => "loaded".into(),
            Err(error) => format!("load_error:{}", compact_error(&error.to_string())),
        }
    }

    fn vendor_runtime_expected_symbols(&self) -> &'static [&'static str] {
        match self.sdk_family() {
            AndorSdkFamily::Sdk2 => &[
                "Initialize",
                "ShutDown",
                "GetTemperature",
                "GetTemperatureRange",
                "SetTemperature",
                "CoolerON",
                "CoolerOFF",
                "GetDetector",
                "SetAcquisitionMode",
                "SetReadMode",
                "SetImage",
                "SetExposureTime",
                "StartAcquisition",
                "WaitForAcquisitionTimeOut",
                "GetAcquiredData16",
                "AbortAcquisition",
            ],
            AndorSdkFamily::Sdk3 => &[
                "AT_InitialiseLibrary",
                "AT_FinaliseLibrary",
                "AT_Open",
                "AT_Close",
                "AT_GetInt",
                "AT_GetFloat",
                "AT_SetInt",
                "AT_SetFloat",
                "AT_GetBool",
                "AT_SetBool",
                "AT_GetEnumIndex",
                "AT_SetEnumString",
                "AT_GetEnumStringByIndex",
                "AT_Command",
                "AT_QueueBuffer",
                "AT_WaitBuffer",
                "AT_Flush",
            ],
            AndorSdkFamily::Unknown => &[],
        }
    }

    fn vendor_runtime_abi_state(&self) -> String {
        if !self.configured.load_vendor_runtime {
            return "disabled".into();
        }
        let expected = self.vendor_runtime_expected_symbols();
        if expected.is_empty() {
            return "unknown_sdk_family".into();
        }
        let digest_state = Self::package_digest_allows_use(
            self.configured.vendor_runtime_path.as_deref(),
            self.configured.vendor_runtime_sha256.as_deref(),
        );
        if digest_state != "verified" {
            return digest_state;
        }
        let Some(path) = self.configured.vendor_runtime_path.as_deref() else {
            return "missing_path".into();
        };

        let library = match unsafe { Library::new(path) } {
            Ok(library) => library,
            Err(error) => return format!("load_error:{}", compact_error(&error.to_string())),
        };
        let missing = expected
            .iter()
            .copied()
            .filter(|symbol| unsafe { library.get::<*const ()>(symbol.as_bytes()) }.is_err())
            .collect::<Vec<_>>();
        if missing.is_empty() {
            format!("symbols_present:{}", expected.join(","))
        } else {
            format!("missing_symbols:{}", missing.join(","))
        }
    }

    fn package_strategy(&self) -> &'static str {
        "ship or load optional third-party runtime packages as explicit backends when project-owned replacements are not available; firmware packages are config-only and used during initialization"
    }

    fn package_gate(&self) -> &'static str {
        "runtime package identity and explicit probe states are exposed; firmware packages are hidden config-only initialization inputs"
    }

    fn support_level(&self) -> &'static str {
        match self.sdk_family() {
            AndorSdkFamily::Sdk2 => {
                "SDK2 Andor VID/PID USB discovery, config-gated hidden firmware initialization from ambiguous EZ-USB devices, firmware/runtime package checks, EP0 command helpers, live bulk-IN Mono16 capture behind os-usb, and vendor-runtime exposure, full-frame capture, detector readback, and cooler control"
            }
            AndorSdkFamily::Sdk3 => {
                "SDK3 Andor VID/PID USB discovery, config-gated hidden FX3 firmware initialization from ambiguous EZ-USB devices, confirmed EP0 status readbacks, firmware/runtime package checks, vendor-runtime feature control/readback, cooler control, and capture backend"
            }
            AndorSdkFamily::Unknown => {
                "Andor USB discovery and runtime-package checks; SDK family not classified"
            }
        }
    }

    fn capture_gate(&self) -> &'static str {
        match self.sdk_family() {
            AndorSdkFamily::Sdk2 => {
                "SDK2 CameraCapture uses verified vendor-runtime single-frame acquisition when enabled; otherwise it uses inferred acquisition sub-codes and configured frame dimensions"
            }
            AndorSdkFamily::Sdk3 => {
                "SDK3 CameraCapture uses the verified vendor runtime; native USB writes wait for acquisition framing"
            }
            AndorSdkFamily::Unknown => {
                "CameraCapture is not exposed because SDK family classification is absent"
            }
        }
    }

    fn sdk2_runtime_path(&self) -> Result<&str> {
        if self.sdk_family() != AndorSdkFamily::Sdk2 {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Andor SDK2 vendor-runtime features require an SDK2 camera",
            ));
        }
        if !self.configured.connect {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Andor SDK2 vendor-runtime features require configured connect=true",
            ));
        }
        if !self.configured.load_vendor_runtime {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Andor SDK2 vendor-runtime features require load_vendor_runtime=true",
            ));
        }
        let digest_state = Self::package_digest_allows_use(
            self.configured.vendor_runtime_path.as_deref(),
            self.configured.vendor_runtime_sha256.as_deref(),
        );
        if digest_state != "verified" {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Andor SDK2 vendor runtime is not verified: {digest_state}"),
            ));
        }
        self.configured
            .vendor_runtime_path
            .as_deref()
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    "Andor SDK2 vendor runtime path is required",
                )
            })
    }

    fn sdk2_runtime_ready(&self) -> bool {
        self.configured.connect
            && self.configured.load_vendor_runtime
            && Self::package_digest_allows_use(
                self.configured.vendor_runtime_path.as_deref(),
                self.configured.vendor_runtime_sha256.as_deref(),
            ) == "verified"
            && self.configured.vendor_runtime_path.is_some()
    }

    fn sdk3_runtime_path(&self) -> Result<&str> {
        if self.sdk_family() != AndorSdkFamily::Sdk3 {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Andor SDK3 vendor-runtime features require an SDK3 camera",
            ));
        }
        if !self.configured.connect {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Andor SDK3 vendor-runtime features require configured connect=true",
            ));
        }
        if !self.configured.load_vendor_runtime {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Andor SDK3 vendor-runtime features require load_vendor_runtime=true",
            ));
        }
        let digest_state = Self::package_digest_allows_use(
            self.configured.vendor_runtime_path.as_deref(),
            self.configured.vendor_runtime_sha256.as_deref(),
        );
        if digest_state != "verified" {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Andor SDK3 vendor runtime is not verified: {digest_state}"),
            ));
        }
        self.configured
            .vendor_runtime_path
            .as_deref()
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    "Andor SDK3 vendor runtime path is required",
                )
            })
    }

    fn sdk3_runtime_ready(&self) -> bool {
        self.configured.connect
            && self.configured.load_vendor_runtime
            && Self::package_digest_allows_use(
                self.configured.vendor_runtime_path.as_deref(),
                self.configured.vendor_runtime_sha256.as_deref(),
            ) == "verified"
            && self.configured.vendor_runtime_path.is_some()
    }

    fn validate_write_property(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        let sdk2 = self.sdk_family() == AndorSdkFamily::Sdk2;
        let sdk3 = self.sdk_family() == AndorSdkFamily::Sdk3;
        match (device, key, value) {
            (device, "width" | "height", Value::PixelCount(count))
                if device == self.camera && sdk3 =>
            {
                if count.pixels() == 0 {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Andor SDK3 AOI dimensions must be positive",
                    ));
                }
                Ok(())
            }
            (device, "exposure", Value::TimeInterval(interval))
                if device == self.camera && (sdk2 || sdk3) =>
            {
                let seconds = interval.seconds();
                if !seconds.is_finite() || seconds <= 0.0 {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Andor exposure must be positive",
                    ));
                }
                Ok(())
            }
            (device, "frame_count", Value::I64(value))
                if device == self.camera && sdk3 && *value > 0 =>
            {
                Ok(())
            }
            (device, "pixel_format", Value::String(value)) if device == self.camera && sdk3 => {
                validate_sdk3_enum_value(
                    "pixel_format",
                    value,
                    &["Mono12", "Mono12Packed", "Mono16", "Mono32"],
                )
            }
            (device, "cycle_mode", Value::String(value)) if device == self.camera && sdk3 => {
                validate_sdk3_enum_value("cycle_mode", value, &["Fixed", "Continuous"])
            }
            (device, "trigger_mode", Value::String(value)) if device == self.camera && sdk3 => {
                validate_sdk3_enum_value(
                    "trigger_mode",
                    value,
                    &[
                        "Internal",
                        "Software",
                        "External",
                        "ExternalStart",
                        "ExternalExposure",
                    ],
                )
            }
            (device, "sensor_cooling", Value::Bool(_))
                if device == self.cooler && (sdk2 || sdk3) =>
            {
                Ok(())
            }
            (device, "temperature_control", Value::String(value))
                if device == self.cooler && sdk3 =>
            {
                if value.trim().is_empty() {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Andor SDK3 temperature_control must not be empty",
                    ));
                }
                Ok(())
            }
            (device, "temperature_control", Value::String(value))
                if device == self.cooler && sdk2 =>
            {
                let target = value.trim().parse::<i32>().map_err(|_| {
                    Error::new(
                        ErrorCode::InvalidProperty,
                        "Andor SDK2 temperature_control must be an integer Celsius target",
                    )
                })?;
                if !(SDK2_MIN_TEMPERATURE_C..=SDK2_MAX_TEMPERATURE_C).contains(&target) {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        format!(
                            "Andor SDK2 temperature_control must be between {SDK2_MIN_TEMPERATURE_C} and {SDK2_MAX_TEMPERATURE_C} deg C"
                        ),
                    ));
                }
                Ok(())
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unsupported Andor writable property {key}"),
            )),
        }
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: Value) -> Result<Value> {
        self.validate_write_property(device, key, &value)?;
        if !self.configured.connect {
            return self.write_configured_property(device, key, value);
        }
        if self.sdk_family() == AndorSdkFamily::Sdk2 {
            let path = self.sdk2_runtime_path()?;
            return match (device, key, value) {
                (device, "exposure", Value::TimeInterval(interval)) if device == self.camera => {
                    live_sdk2_runtime::set_exposure_time(
                        path,
                        self.configured.camera_index,
                        interval,
                    )?;
                    self.configured.exposure = interval;
                    Ok(Value::TimeInterval(interval))
                }
                (device, "sensor_cooling", Value::Bool(value)) if device == self.cooler => {
                    live_sdk2_runtime::set_cooler(path, self.configured.camera_index, value)?;
                    self.configured.sensor_cooling = value;
                    Ok(Value::Bool(value))
                }
                (device, "temperature_control", Value::String(value)) if device == self.cooler => {
                    let target = value.trim().parse::<i32>().map_err(|_| {
                        Error::new(
                            ErrorCode::InvalidProperty,
                            "Andor SDK2 temperature_control must be an integer Celsius target",
                        )
                    })?;
                    live_sdk2_runtime::set_temperature(path, self.configured.camera_index, target)?;
                    self.configured.temperature_control = Some(target.to_string());
                    Ok(Value::String(target.to_string()))
                }
                _ => unreachable!("validated Andor SDK2 writable property"),
            };
        }
        let path = self.sdk3_runtime_path()?;
        match (device, key, value) {
            (device, "width", Value::PixelCount(count)) if device == self.camera => {
                live_sdk3::set_int(
                    path,
                    self.configured.camera_index,
                    "AOIWidth",
                    count.pixels() as i64,
                )?;
                self.configured.width = count.pixels();
                Ok(Value::PixelCount(count))
            }
            (device, "height", Value::PixelCount(count)) if device == self.camera => {
                live_sdk3::set_int(
                    path,
                    self.configured.camera_index,
                    "AOIHeight",
                    count.pixels() as i64,
                )?;
                self.configured.height = count.pixels();
                Ok(Value::PixelCount(count))
            }
            (device, "exposure", Value::TimeInterval(interval)) if device == self.camera => {
                live_sdk3::set_float(
                    path,
                    self.configured.camera_index,
                    "ExposureTime",
                    interval.seconds(),
                )?;
                self.configured.exposure = interval;
                Ok(Value::TimeInterval(interval))
            }
            (device, "frame_count", Value::I64(value)) if device == self.camera => {
                live_sdk3::set_int(path, self.configured.camera_index, "FrameCount", value)?;
                self.configured.frame_count = value;
                Ok(Value::I64(value))
            }
            (device, "pixel_format", Value::String(value)) if device == self.camera => {
                live_sdk3::set_enum_string(
                    path,
                    self.configured.camera_index,
                    "PixelEncoding",
                    &value,
                )?;
                self.configured.pixel_format = value.clone();
                Ok(Value::String(value))
            }
            (device, "cycle_mode", Value::String(value)) if device == self.camera => {
                live_sdk3::set_enum_string(
                    path,
                    self.configured.camera_index,
                    "CycleMode",
                    &value,
                )?;
                self.configured.cycle_mode = value.clone();
                Ok(Value::String(value))
            }
            (device, "trigger_mode", Value::String(value)) if device == self.camera => {
                live_sdk3::set_enum_string(
                    path,
                    self.configured.camera_index,
                    "TriggerMode",
                    &value,
                )?;
                self.configured.trigger_mode = value.clone();
                Ok(Value::String(value))
            }
            (device, "sensor_cooling", Value::Bool(value)) if device == self.cooler => {
                live_sdk3::set_bool(path, self.configured.camera_index, "SensorCooling", value)?;
                self.configured.sensor_cooling = value;
                Ok(Value::Bool(value))
            }
            (device, "temperature_control", Value::String(value)) if device == self.cooler => {
                live_sdk3::set_enum_string(
                    path,
                    self.configured.camera_index,
                    "TemperatureControl",
                    &value,
                )?;
                self.configured.temperature_control = Some(value.clone());
                Ok(Value::String(value))
            }
            _ => unreachable!("validated Andor SDK3 writable property"),
        }
    }

    fn write_configured_property(
        &mut self,
        device: DeviceId,
        key: &str,
        value: Value,
    ) -> Result<Value> {
        match (device, key, value) {
            (device, "exposure", Value::TimeInterval(interval)) if device == self.camera => {
                self.configured.exposure = interval;
                Ok(Value::TimeInterval(interval))
            }
            (device, "width", Value::PixelCount(count))
                if device == self.camera && self.sdk_family() == AndorSdkFamily::Sdk3 =>
            {
                self.configured.width = count.pixels();
                Ok(Value::PixelCount(count))
            }
            (device, "height", Value::PixelCount(count))
                if device == self.camera && self.sdk_family() == AndorSdkFamily::Sdk3 =>
            {
                self.configured.height = count.pixels();
                Ok(Value::PixelCount(count))
            }
            (device, "frame_count", Value::I64(value))
                if device == self.camera && self.sdk_family() == AndorSdkFamily::Sdk3 =>
            {
                self.configured.frame_count = value;
                Ok(Value::I64(value))
            }
            (device, "pixel_format", Value::String(value))
                if device == self.camera && self.sdk_family() == AndorSdkFamily::Sdk3 =>
            {
                self.configured.pixel_format = value.clone();
                Ok(Value::String(value))
            }
            (device, "cycle_mode", Value::String(value))
                if device == self.camera && self.sdk_family() == AndorSdkFamily::Sdk3 =>
            {
                self.configured.cycle_mode = value.clone();
                Ok(Value::String(value))
            }
            (device, "trigger_mode", Value::String(value))
                if device == self.camera && self.sdk_family() == AndorSdkFamily::Sdk3 =>
            {
                self.configured.trigger_mode = value.clone();
                Ok(Value::String(value))
            }
            (device, "sensor_cooling", Value::Bool(value)) if device == self.cooler => {
                self.configured.sensor_cooling = value;
                Ok(Value::Bool(value))
            }
            (device, "temperature_control", Value::String(value)) if device == self.cooler => {
                self.configured.temperature_control = Some(value.clone());
                Ok(Value::String(value))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unsupported Andor configured writable property {key}"),
            )),
        }
    }

    fn invoke_temperature_control(&mut self, request: TemperatureControlRequest) -> Result<Value> {
        let mut changed = BTreeMap::new();
        if let Some(enabled) = request.enabled {
            let value = self.write_property(self.cooler, "sensor_cooling", Value::Bool(enabled))?;
            changed.insert("sensor_cooling".into(), value);
        }
        if let Some(target) = request.target {
            let celsius = target.celsius();
            if !celsius.is_finite() {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Andor target temperature must be finite",
                ));
            }
            let value = self.write_property(
                self.cooler,
                "temperature_control",
                Value::String(format!("{celsius:.0}")),
            )?;
            changed.insert("temperature_control".into(), value);
        }
        Ok(Value::Map(changed))
    }

    fn sdk3_status_byte(&self) -> Result<u8> {
        if self.sdk_family() == AndorSdkFamily::Sdk3 && self.configured.connect {
            #[cfg(feature = "os-usb")]
            {
                return live_sdk2::read_sdk3_status_byte(
                    &self.configured.usb_identity,
                    self.configured.vendor_id,
                    self.configured.product_id,
                    self.configured.serial_number.as_deref(),
                );
            }
            #[cfg(not(feature = "os-usb"))]
            {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    "Andor SDK3 live status readback requires numanager-drivers/os-usb",
                ));
            }
        }
        Ok(self.configured.status_byte.unwrap_or(0))
    }

    fn sdk3_status_word(&self) -> Result<u32> {
        if self.sdk_family() == AndorSdkFamily::Sdk3 && self.configured.connect {
            #[cfg(feature = "os-usb")]
            {
                return live_sdk2::read_sdk3_status_word(
                    &self.configured.usb_identity,
                    self.configured.vendor_id,
                    self.configured.product_id,
                    self.configured.serial_number.as_deref(),
                );
            }
            #[cfg(not(feature = "os-usb"))]
            {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    "Andor SDK3 live status-word readback requires numanager-drivers/os-usb",
                ));
            }
        }
        Ok(self.configured.sdk3_status_word.unwrap_or(0))
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "product" => Ok(Value::String(self.configured.product.clone())),
                "serial_number" => Ok(Value::String(
                    self.configured.serial_number.clone().unwrap_or_default(),
                )),
                "sdk_family" => Ok(Value::String(self.sdk_family().as_str().into())),
                "vendor_id" => Ok(Value::I64(self.configured.vendor_id as i64)),
                "product_id" => Ok(Value::I64(self.configured.product_id as i64)),
                "usb_identity" => Ok(self.usb_identity_value()),
                "identity" => Ok(Value::Bytes(self.configured.identity.clone())),
                "status_byte" => Ok(Value::I64(self.sdk3_status_byte()? as i64)),
                "sdk3_status_word" => Ok(Value::I64(self.sdk3_status_word()? as i64)),
                "vendor_runtime_path" => Ok(Value::String(
                    self.configured
                        .vendor_runtime_path
                        .clone()
                        .unwrap_or_default(),
                )),
                "vendor_runtime_sha256" => Ok(Value::String(
                    self.configured
                        .vendor_runtime_sha256
                        .clone()
                        .unwrap_or_default(),
                )),
                "load_vendor_runtime" => Ok(Value::Bool(self.configured.load_vendor_runtime)),
                "camera_index" => Ok(Value::I64(self.configured.camera_index as i64)),
                "vendor_runtime_file_status" => {
                    Ok(Value::String(self.vendor_runtime_file_status()))
                }
                "vendor_runtime_digest_state" => {
                    Ok(Value::String(self.vendor_runtime_digest_state()))
                }
                "vendor_runtime_file_size" => self.vendor_runtime_file_size(),
                "vendor_runtime_probe_state" => {
                    Ok(Value::String(self.vendor_runtime_probe_state()))
                }
                "vendor_runtime_abi_state" => Ok(Value::String(self.vendor_runtime_abi_state())),
                "package_strategy" => Ok(Value::String(self.package_strategy().into())),
                "vendor_runtime_state" => Ok(Value::String(self.vendor_runtime_state().into())),
                "connect" => Ok(Value::Bool(self.configured.connect)),
                "package_gate" => Ok(Value::String(self.package_gate().into())),
                "third_party_notice" => Ok(Value::String(
                    "configured vendor runtime packages are third-party excluded data; firmware packages are config-only initialization inputs"
                        .into(),
                )),
                "support_level" => Ok(Value::String(self.support_level().into())),
                _ => invalid_property("unknown Andor hub property", key),
            };
        }
        if device == self.camera {
            let sdk2 = self.sdk_family() == AndorSdkFamily::Sdk2;
            let sdk3 = self.sdk_family() == AndorSdkFamily::Sdk3;
            return match key {
                "product" => Ok(Value::String(self.configured.product.clone())),
                "serial_number" => Ok(Value::String(
                    self.configured.serial_number.clone().unwrap_or_default(),
                )),
                "sdk_family" => Ok(Value::String(self.sdk_family().as_str().into())),
                "width" if sdk3 && self.sdk3_runtime_ready() => {
                    Ok(Value::PixelCount(PixelCount::new(checked_u32_value(
                        live_sdk3::get_int(
                            self.sdk3_runtime_path()?,
                            self.configured.camera_index,
                            "AOIWidth",
                        )?,
                        "AOIWidth",
                    )?)))
                }
                "width" if sdk2 && self.sdk2_runtime_ready() => {
                    let (width, _height) = live_sdk2_runtime::get_detector(
                        self.sdk2_runtime_path()?,
                        self.configured.camera_index,
                    )?;
                    Ok(Value::PixelCount(PixelCount::new(width)))
                }
                "width" if sdk2 || sdk3 => {
                    Ok(Value::PixelCount(PixelCount::new(self.configured.width)))
                }
                "height" if sdk3 && self.sdk3_runtime_ready() => {
                    Ok(Value::PixelCount(PixelCount::new(checked_u32_value(
                        live_sdk3::get_int(
                            self.sdk3_runtime_path()?,
                            self.configured.camera_index,
                            "AOIHeight",
                        )?,
                        "AOIHeight",
                    )?)))
                }
                "height" if sdk2 && self.sdk2_runtime_ready() => {
                    let (_width, height) = live_sdk2_runtime::get_detector(
                        self.sdk2_runtime_path()?,
                        self.configured.camera_index,
                    )?;
                    Ok(Value::PixelCount(PixelCount::new(height)))
                }
                "height" if sdk2 || sdk3 => {
                    Ok(Value::PixelCount(PixelCount::new(self.configured.height)))
                }
                "pixel_format" if sdk3 && self.sdk3_runtime_ready() => {
                    Ok(Value::String(live_sdk3::get_enum_string(
                        self.sdk3_runtime_path()?,
                        self.configured.camera_index,
                        "PixelEncoding",
                    )?))
                }
                "pixel_format" if sdk2 => Ok(Value::String("Mono16".into())),
                "pixel_format" if sdk3 => Ok(Value::String(self.configured.pixel_format.clone())),
                "exposure" if sdk3 && self.sdk3_runtime_ready() => Ok(Value::TimeInterval(
                    TimeInterval::from_seconds(live_sdk3::get_float(
                        self.sdk3_runtime_path()?,
                        self.configured.camera_index,
                        "ExposureTime",
                    )?),
                )),
                "exposure" if sdk2 || sdk3 => Ok(Value::TimeInterval(self.configured.exposure)),
                "frame_count" if sdk3 && self.sdk3_runtime_ready() => {
                    Ok(Value::I64(live_sdk3::get_int(
                        self.sdk3_runtime_path()?,
                        self.configured.camera_index,
                        "FrameCount",
                    )?))
                }
                "frame_count" if sdk3 => Ok(Value::I64(self.configured.frame_count)),
                "cycle_mode" if sdk3 && self.sdk3_runtime_ready() => {
                    Ok(Value::String(live_sdk3::get_enum_string(
                        self.sdk3_runtime_path()?,
                        self.configured.camera_index,
                        "CycleMode",
                    )?))
                }
                "cycle_mode" if sdk3 => Ok(Value::String(self.configured.cycle_mode.clone())),
                "trigger_mode" if sdk3 && self.sdk3_runtime_ready() => {
                    Ok(Value::String(live_sdk3::get_enum_string(
                        self.sdk3_runtime_path()?,
                        self.configured.camera_index,
                        "TriggerMode",
                    )?))
                }
                "trigger_mode" if sdk3 => Ok(Value::String(self.configured.trigger_mode.clone())),
                "frame_endpoint" if sdk2 => Ok(Value::I64(SDK2_BULK_IN_ENDPOINT as i64)),
                "status_endpoint" if sdk2 => Ok(Value::I64(SDK2_STATUS_BULK_IN_ENDPOINT as i64)),
                "bulk_out_endpoint" if sdk2 => Ok(Value::I64(SDK2_BULK_OUT_ENDPOINT as i64)),
                "readout_alignment" if sdk2 => Ok(Value::PixelCount(PixelCount::new(
                    SDK2_READOUT_ALIGNMENT_PIXELS,
                ))),
                "readout_bytes_per_pixel" if sdk2 => {
                    Ok(Value::I64(SDK2_READOUT_BYTES_PER_PIXEL as i64))
                }
                "capture_gate" => Ok(Value::String(self.capture_gate().into())),
                _ => invalid_property("unknown Andor camera property", key),
            };
        }
        if device == self.cooler {
            let sdk2 = self.sdk_family() == AndorSdkFamily::Sdk2;
            let sdk3 = self.sdk_family() == AndorSdkFamily::Sdk3;
            return match key {
                "sensor_cooling" if sdk2 && self.sdk2_runtime_ready() => {
                    let (_temperature, status) = live_sdk2_runtime::get_temperature(
                        self.sdk2_runtime_path()?,
                        self.configured.camera_index,
                    )?;
                    Ok(Value::Bool(status != "Off"))
                }
                "sensor_cooling" if sdk2 => Ok(Value::Bool(self.configured.sensor_cooling)),
                "sensor_cooling" if sdk3 && self.sdk3_runtime_ready() => Ok(Value::Bool(
                    live_sdk3::get_bool(
                        self.sdk3_runtime_path()?,
                        self.configured.camera_index,
                        "SensorCooling",
                    )?,
                )),
                "sensor_cooling" if sdk3 => Ok(Value::Bool(self.configured.sensor_cooling)),
                "temperature_control" if sdk2 => Ok(Value::String(
                    self.configured
                        .temperature_control
                        .clone()
                        .unwrap_or_default(),
                )),
                "temperature_control" if sdk3 && self.sdk3_runtime_ready() => {
                    Ok(Value::String(live_sdk3::get_enum_string(
                        self.sdk3_runtime_path()?,
                        self.configured.camera_index,
                        "TemperatureControl",
                    )?))
                }
                "temperature_control" if sdk3 => Ok(Value::String(
                    self.configured
                        .temperature_control
                        .clone()
                        .unwrap_or_default(),
                )),
                "sensor_temperature" if sdk2 && self.sdk2_runtime_ready() => {
                    let path = self.sdk2_runtime_path()?;
                    let (temperature, _status) =
                        live_sdk2_runtime::get_temperature(path, self.configured.camera_index)?;
                    Ok(Value::Temperature(Temperature::from_celsius(f64::from(
                        temperature,
                    ))))
                }
                "sensor_temperature" if sdk3 && self.sdk3_runtime_ready() => {
                    let path = self.sdk3_runtime_path()?;
                    Ok(Value::Temperature(Temperature::from_celsius(
                        live_sdk3::get_float(path, self.configured.camera_index, "SensorTemperature")?,
                    )))
                }
                "sensor_temperature" if sdk2 || sdk3 => Ok(Value::Null),
                "temperature_status" if sdk2 && self.sdk2_runtime_ready() => {
                    let path = self.sdk2_runtime_path()?;
                    let (_temperature, status) =
                        live_sdk2_runtime::get_temperature(path, self.configured.camera_index)?;
                    Ok(Value::String(status))
                }
                "temperature_status" if sdk3 && self.sdk3_runtime_ready() => {
                    let path = self.sdk3_runtime_path()?;
                    Ok(Value::String(live_sdk3::get_enum_string(
                        path,
                        self.configured.camera_index,
                        "TemperatureStatus",
                    )?))
                }
                "temperature_status" if sdk2 || sdk3 => Ok(Value::String(
                    if self.configured.connect {
                        "runtime_not_verified"
                    } else {
                        "configured"
                    }
                    .into(),
                )),
                "support_level" => Ok(Value::String(
                    if sdk3 {
                        "SDK3 temperature/cooler control uses verified vendor-runtime AT features"
                    } else if sdk2 {
                        "SDK2 temperature/cooler control uses verified vendor-runtime atmcd functions"
                    } else {
                        "temperature/cooler control requires SDK2 or SDK3 family classification"
                    }
                    .into(),
                )),
                "cooler_gate" => Ok(Value::String(
                    if sdk3 {
                        "sensor temperature telemetry requires verified vendor runtime and connect=true"
                    } else if sdk2 {
                        "SDK2 sensor temperature telemetry and cooler writes require verified vendor runtime and connect=true"
                    } else {
                        "sensor temperature and cooler controls require SDK2 or SDK3 family classification"
                    }
                    .into(),
                )),
                _ => invalid_property("unknown Andor cooler property", key),
            };
        }
        Err(Error::new(
            ErrorCode::InvalidCommand,
            "unknown Andor device",
        ))
    }

    fn capture_sdk2_frame(
        &mut self,
        token: DriverToken,
        request: CameraCaptureRequest,
    ) -> Result<Value> {
        if !self.configured.connect {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Andor SDK2 live capture requires configured connect=true",
            ));
        }
        let encoding = request.encoding.unwrap_or(ImageEncoding::Mono16);
        if !matches!(
            encoding,
            ImageEncoding::Native | ImageEncoding::Mono16 | ImageEncoding::Raw16
        ) {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Andor SDK2 capture supports Native, Mono16, or Raw16",
            ));
        }
        let width = self.configured.width;
        let height = self.configured.height;
        let pixels = width as usize * height as usize;
        if self.sdk2_runtime_ready() {
            let path = self.sdk2_runtime_path()?;
            let frame = live_sdk2_runtime::capture(
                path,
                self.configured.camera_index,
                width,
                height,
                self.configured.exposure,
            )?;
            self.configured.width = frame.width;
            self.configured.height = frame.height;
            let handle = FrameHandle {
                stream: StreamId(self.camera.0 .0),
                frame: FrameId(token.0),
            };
            let pixel_format = match encoding {
                ImageEncoding::Native | ImageEncoding::Mono16 => ImageEncoding::Mono16,
                ImageEncoding::Raw16 => ImageEncoding::Raw16,
                _ => unreachable!(),
            }
            .property_value()
            .to_string();
            self.events.push_back(DriverEvent::FrameReady(Frame {
                handle,
                device: self.camera,
                width: frame.width,
                height: frame.height,
                pixel_format: pixel_format.clone(),
                data: frame.data,
                metadata: BTreeMap::from([
                    (
                        "source".into(),
                        Value::String("andor-sdk2-vendor-runtime".into()),
                    ),
                    ("wire_byte_order".into(), Value::String("native".into())),
                    (
                        "runtime_backend".into(),
                        Value::String(
                            "SetExposureTime/SetImage/StartAcquisition/GetAcquiredData16".into(),
                        ),
                    ),
                ]),
                buffer: request.buffer.unwrap_or_default(),
            }));
            return Ok(Value::Map(BTreeMap::from([
                (
                    "width".into(),
                    Value::PixelCount(PixelCount::new(frame.width)),
                ),
                (
                    "height".into(),
                    Value::PixelCount(PixelCount::new(frame.height)),
                ),
                ("pixel_format".into(), Value::String(pixel_format)),
                ("stream".into(), Value::I64(handle.stream.0 as i64)),
                ("frame".into(), Value::I64(handle.frame.0 as i64)),
                (
                    "source".into(),
                    Value::String("andor-sdk2-vendor-runtime".into()),
                ),
            ])));
        }
        #[cfg(feature = "os-usb")]
        if !self.configured.firmware_loaded {
            let digest_state = Self::package_digest_allows_use(
                self.configured.firmware_blob_path.as_deref(),
                self.configured.firmware_blob_sha256.as_deref(),
            );
            if digest_state != "verified" {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Andor SDK2 firmware package is not verified: {digest_state}"),
                ));
            }
        }
        #[cfg(feature = "os-usb")]
        let data = live_sdk2::capture(
            &self.configured.usb_identity,
            self.configured.vendor_id,
            self.configured.product_id,
            self.configured.serial_number.as_deref(),
            self.configured.firmware_loaded,
            self.configured.firmware_blob_path.as_deref(),
            pixels,
        )?;
        #[cfg(not(feature = "os-usb"))]
        let _data = pixels;
        #[cfg(not(feature = "os-usb"))]
        let _ = token;
        #[cfg(not(feature = "os-usb"))]
        return Err(Error::new(
            ErrorCode::Unsupported,
            "Andor SDK2 live capture requires numanager-drivers/os-usb",
        ));
        #[cfg(feature = "os-usb")]
        {
            let handle = FrameHandle {
                stream: StreamId(self.camera.0 .0),
                frame: FrameId(token.0),
            };
            let pixel_format = match encoding {
                ImageEncoding::Native | ImageEncoding::Mono16 => ImageEncoding::Mono16,
                ImageEncoding::Raw16 => ImageEncoding::Raw16,
                _ => unreachable!(),
            }
            .property_value()
            .to_string();
            self.events.push_back(DriverEvent::FrameReady(Frame {
                handle,
                device: self.camera,
                width,
                height,
                pixel_format: pixel_format.clone(),
                data,
                metadata: BTreeMap::from([
                    ("source".into(), Value::String("andor-sdk2-live-usb".into())),
                    ("wire_byte_order".into(), Value::String("big_endian".into())),
                    (
                        "acquisition_subcodes".into(),
                        Value::String("inferred".into()),
                    ),
                ]),
                buffer: request.buffer.unwrap_or_default(),
            }));
            Ok(Value::Map(BTreeMap::from([
                ("width".into(), Value::PixelCount(PixelCount::new(width))),
                ("height".into(), Value::PixelCount(PixelCount::new(height))),
                ("pixel_format".into(), Value::String(pixel_format)),
                ("stream".into(), Value::I64(handle.stream.0 as i64)),
                ("frame".into(), Value::I64(handle.frame.0 as i64)),
                ("source".into(), Value::String("andor-sdk2-live-usb".into())),
            ])))
        }
    }

    fn capture_sdk3_frame(
        &mut self,
        token: DriverToken,
        request: CameraCaptureRequest,
    ) -> Result<Value> {
        if !self.configured.connect {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Andor SDK3 vendor-runtime capture requires configured connect=true",
            ));
        }
        if !self.configured.load_vendor_runtime {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Andor SDK3 vendor-runtime capture requires load_vendor_runtime=true",
            ));
        }
        let digest_state = Self::package_digest_allows_use(
            self.configured.vendor_runtime_path.as_deref(),
            self.configured.vendor_runtime_sha256.as_deref(),
        );
        if digest_state != "verified" {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Andor SDK3 vendor runtime is not verified: {digest_state}"),
            ));
        }
        let encoding = request.encoding.unwrap_or(ImageEncoding::Mono16);
        if !matches!(
            encoding,
            ImageEncoding::Native | ImageEncoding::Mono16 | ImageEncoding::Raw16
        ) {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Andor SDK3 vendor-runtime capture supports Native, Mono16, or Raw16",
            ));
        }
        let path = self
            .configured
            .vendor_runtime_path
            .as_deref()
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    "Andor SDK3 vendor runtime path is required",
                )
            })?;
        let requested_width = self.configured.width;
        let requested_height = self.configured.height;
        #[cfg(feature = "os-usb")]
        let frame = live_sdk3::capture(
            path,
            self.configured.camera_index,
            requested_width,
            requested_height,
        )?;
        #[cfg(not(feature = "os-usb"))]
        {
            let _ = (path, token, requested_width, requested_height);
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Andor SDK3 vendor-runtime capture requires numanager-drivers/os-usb",
            ));
        }
        #[cfg(feature = "os-usb")]
        {
            self.configured.width = frame.width;
            self.configured.height = frame.height;
            let handle = FrameHandle {
                stream: StreamId(self.camera.0 .0),
                frame: FrameId(token.0),
            };
            let pixel_format = match encoding {
                ImageEncoding::Native | ImageEncoding::Mono16 => ImageEncoding::Mono16,
                ImageEncoding::Raw16 => ImageEncoding::Raw16,
                _ => unreachable!(),
            }
            .property_value()
            .to_string();
            self.events.push_back(DriverEvent::FrameReady(Frame {
                handle,
                device: self.camera,
                width: frame.width,
                height: frame.height,
                pixel_format: pixel_format.clone(),
                data: frame.data,
                metadata: BTreeMap::from([
                    (
                        "source".into(),
                        Value::String("andor-sdk3-vendor-runtime".into()),
                    ),
                    (
                        "wire_byte_order".into(),
                        Value::String("little_endian".into()),
                    ),
                    (
                        "runtime_backend".into(),
                        Value::String("AT_QueueBuffer/AT_Command/AT_WaitBuffer".into()),
                    ),
                ]),
                buffer: request.buffer.unwrap_or_default(),
            }));
            Ok(Value::Map(BTreeMap::from([
                (
                    "width".into(),
                    Value::PixelCount(PixelCount::new(frame.width)),
                ),
                (
                    "height".into(),
                    Value::PixelCount(PixelCount::new(frame.height)),
                ),
                ("pixel_format".into(), Value::String(pixel_format)),
                ("stream".into(), Value::I64(handle.stream.0 as i64)),
                ("frame".into(), Value::I64(handle.frame.0 as i64)),
                (
                    "source".into(),
                    Value::String("andor-sdk3-vendor-runtime".into()),
                ),
            ])))
        }
    }
}

fn capability(
    id: u64,
    device: DeviceId,
    kind: CapabilityKind,
    request: ValueType,
    response: ValueType,
) -> CapabilityDescriptor {
    let name = kind.name().to_string();
    CapabilityDescriptor {
        id: CapabilityId(id),
        device,
        kind,
        name,
        request,
        response,
    }
}

impl Driver for AndorCameraDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        self.descriptors_inner()
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        let mut control_metadata = BTreeMap::from([
            ("endpoint".into(), Value::I64(0)),
            (
                "active_usb_detected".into(),
                Value::Bool(self.configured.usb_identity.is_some()),
            ),
            (
                "sdk_family".into(),
                Value::String(self.sdk_family().as_str().into()),
            ),
            (
                "completion_gate".into(),
                Value::String("status/readout completion uses the implemented backend path".into()),
            ),
        ]);
        if self.sdk_family() == AndorSdkFamily::Sdk2 {
            control_metadata.extend([(
                "control_surface".into(),
                Value::String("hidden driver-internal SDK2 control requests".into()),
            )]);
        }
        if let Some(identity) = &self.configured.usb_identity {
            control_metadata.insert("usb_identity".into(), identity.value());
        }

        let mut frame_metadata = BTreeMap::from([
            ("endpoint".into(), Value::I64(SDK2_BULK_IN_ENDPOINT as i64)),
            (
                "active_usb_detected".into(),
                Value::Bool(self.configured.usb_identity.is_some()),
            ),
            (
                "sdk_family".into(),
                Value::String(self.sdk_family().as_str().into()),
            ),
            (
                "completion_gate".into(),
                Value::String("status/readout completion uses the implemented backend path".into()),
            ),
        ]);
        if self.sdk_family() == AndorSdkFamily::Sdk2 {
            frame_metadata.extend([
                (
                    "status_endpoint".into(),
                    Value::I64(SDK2_STATUS_BULK_IN_ENDPOINT as i64),
                ),
                (
                    "bulk_out_endpoint".into(),
                    Value::I64(SDK2_BULK_OUT_ENDPOINT as i64),
                ),
                (
                    "readout_alignment".into(),
                    Value::PixelCount(PixelCount::new(SDK2_READOUT_ALIGNMENT_PIXELS)),
                ),
                (
                    "readout_bytes_per_pixel".into(),
                    Value::I64(SDK2_READOUT_BYTES_PER_PIXEL as i64),
                ),
                (
                    "pixel_format".into(),
                    Value::String("Mono16 big-endian on SDK2".into()),
                ),
            ]);
        }
        if let Some(identity) = &self.configured.usb_identity {
            frame_metadata.insert("usb_identity".into(), identity.value());
        }

        let vendor_runtime_metadata = BTreeMap::from([
            (
                "runtime_path".into(),
                Value::String(
                    self.configured
                        .vendor_runtime_path
                        .clone()
                        .unwrap_or_default(),
                ),
            ),
            (
                "runtime_sha256".into(),
                Value::String(
                    self.configured
                        .vendor_runtime_sha256
                        .clone()
                        .unwrap_or_default(),
                ),
            ),
            (
                "runtime_digest_state".into(),
                Value::String(self.vendor_runtime_digest_state()),
            ),
            (
                "configured".into(),
                Value::Bool(self.vendor_runtime_configured()),
            ),
            (
                "backend_enabled".into(),
                Value::Bool(self.configured.load_vendor_runtime),
            ),
            (
                "package_state".into(),
                Value::String(self.vendor_runtime_state().into()),
            ),
            (
                "runtime_abi_state".into(),
                Value::String(self.vendor_runtime_abi_state()),
            ),
            (
                "license_scope".into(),
                Value::String("third-party excluded data".into()),
            ),
            (
                "binding_gate".into(),
                Value::String(self.package_gate().into()),
            ),
        ]);

        vec![
            ResourceDescriptor {
                id: self.control,
                driver: self.id,
                label: format!("{} EP0 control", self.configured.label),
                kind: "usb.control.vendor".into(),
                metadata: control_metadata,
            },
            ResourceDescriptor {
                id: self.vendor_runtime,
                driver: self.id,
                label: format!("{} vendor runtime package", self.configured.label),
                kind: "vendor.runtime.andor".into(),
                metadata: vendor_runtime_metadata,
            },
            ResourceDescriptor {
                id: self.frame_bulk_in,
                driver: self.id,
                label: format!("{} frame bulk-in", self.configured.label),
                kind: "usb.bulk.in".into(),
                metadata: frame_metadata,
            },
        ]
    }

    fn capabilities(&self, _device: DeviceId) -> Vec<CapabilityDescriptor> {
        if _device == self.camera
            && matches!(
                self.sdk_family(),
                AndorSdkFamily::Sdk2 | AndorSdkFamily::Sdk3
            )
        {
            return vec![capability(
                1,
                self.camera,
                CapabilityKind::CameraCapture,
                ValueType::Map,
                ValueType::Map,
            )];
        }
        if _device == self.cooler
            && matches!(
                self.sdk_family(),
                AndorSdkFamily::Sdk2 | AndorSdkFamily::Sdk3
            )
        {
            return vec![capability(
                2,
                self.cooler,
                CapabilityKind::TemperatureControl,
                ValueType::Map,
                ValueType::Map,
            )];
        }
        Vec::new()
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key }
                    if [self.hub, self.camera, self.cooler].contains(device) =>
                {
                    let _ = self.read_property(*device, key)?;
                }
                Command::WriteProperty { device, .. }
                    if [self.hub, self.camera, self.cooler].contains(device) =>
                {
                    let Command::WriteProperty { key, value, .. } = command else {
                        unreachable!();
                    };
                    self.validate_write_property(*device, key, value)?;
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        if [self.hub, self.camera, self.cooler].contains(&write.device) {
                            self.validate_write_property(
                                write.device,
                                &write.property,
                                &write.value,
                            )?;
                        }
                    }
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if *device == self.camera && *capability == CapabilityId(1) => {
                    if !matches!(
                        self.sdk_family(),
                        AndorSdkFamily::Sdk2 | AndorSdkFamily::Sdk3
                    ) {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "Andor CameraCapture is not exposed because SDK family classification is absent",
                        ));
                    }
                    if !matches!(
                        request,
                        CapabilityRequest::CameraCapture(_) | CapabilityRequest::None
                    ) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "CameraCapture expects CameraCaptureRequest",
                        ));
                    }
                }
                Command::Invoke { device, .. }
                    if [self.hub, self.camera, self.cooler].contains(device) =>
                {
                    let Command::Invoke {
                        capability,
                        request,
                        ..
                    } = command
                    else {
                        unreachable!();
                    };
                    if *device == self.cooler
                        && *capability == CapabilityId(2)
                        && matches!(
                            self.sdk_family(),
                            AndorSdkFamily::Sdk2 | AndorSdkFamily::Sdk3
                        )
                    {
                        if !matches!(request, CapabilityRequest::TemperatureControl(_)) {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Andor TemperatureControl expects TemperatureControlRequest",
                            ));
                        }
                    } else {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "unsupported Andor capability",
                        ));
                    }
                }
                _ => {}
            }
        }
        Ok(PreparedBatch {
            id: batch.id,
            commands: batch.commands.clone(),
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.control),
                description: "Andor read-only property batch".into(),
                payload: Value::String(self.sdk_family().as_str().into()),
            }],
        })
    }

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.token();
        let mut result = Value::Null;
        for command in prepared.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    result = self.read_property(device, &key)?;
                }
                Command::WriteProperty { device, key, value }
                    if [self.camera, self.cooler].contains(&device) =>
                {
                    result = self.write_property(device, &key, value)?;
                }
                Command::ApplyStateSet(set) => {
                    for write in set.writes {
                        if [self.camera, self.cooler].contains(&write.device) {
                            result =
                                self.write_property(write.device, &write.property, write.value)?;
                        }
                    }
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if device == self.camera && capability == CapabilityId(1) => {
                    let capture = match request {
                        CapabilityRequest::CameraCapture(request) => request,
                        CapabilityRequest::None => CameraCaptureRequest::default_frame(),
                        _ => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "CameraCapture expects CameraCaptureRequest",
                            ))
                        }
                    };
                    result = match self.sdk_family() {
                        AndorSdkFamily::Sdk2 => self.capture_sdk2_frame(token, capture)?,
                        AndorSdkFamily::Sdk3 => self.capture_sdk3_frame(token, capture)?,
                        AndorSdkFamily::Unknown => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "Andor CameraCapture is not exposed because SDK family classification is absent",
                            ))
                        }
                    };
                }
                Command::Invoke {
                    device,
                    capability,
                    request: CapabilityRequest::TemperatureControl(request),
                } if device == self.cooler && capability == CapabilityId(2) => {
                    result = self.invoke_temperature_control(request)?;
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

fn writable_property(key: &str, display_name: &str, value_type: ValueType) -> PropertySchema {
    let mut schema = property(key, display_name, value_type);
    schema.writable = true;
    schema
}

fn string_prop(device: &DeviceConfig, key: &str) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn bool_prop(device: &DeviceConfig, key: &str) -> Result<Option<bool>> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Andor property {key} must be Bool"),
        )),
        None => Ok(None),
    }
}

fn u16_prop(device: &DeviceConfig, key: &str) -> Result<Option<u16>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if (0..=u16::MAX as i64).contains(value) => Ok(Some(*value as u16)),
        Some(Value::I64(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Andor property {key} must fit in an unsigned 16-bit integer"),
        )),
        Some(Value::String(value)) => parse_u16(value).map(Some).ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("Andor property {key} must be a decimal or 0x-prefixed u16"),
            )
        }),
        _ => Ok(None),
    }
}

fn optional_u8_prop(device: &DeviceConfig, key: &str, default: Option<u8>) -> Result<Option<u8>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if (0..=u8::MAX as i64).contains(value) => Ok(Some(*value as u8)),
        Some(Value::I64(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Andor property {key} must fit in an unsigned byte"),
        )),
        Some(Value::String(value)) => {
            let parsed = parse_u16(value).ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Andor property {key} must be a decimal or 0x-prefixed byte"),
                )
            })?;
            u8::try_from(parsed).map(Some).map_err(|_| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Andor property {key} must fit in an unsigned byte"),
                )
            })
        }
        Some(Value::Null) => Ok(None),
        _ => Ok(default),
    }
}

fn optional_u32_prop(
    device: &DeviceConfig,
    key: &str,
    default: Option<u32>,
) -> Result<Option<u32>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if (0..=u32::MAX as i64).contains(value) => Ok(Some(*value as u32)),
        Some(Value::I64(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Andor property {key} must fit in an unsigned 32-bit integer"),
        )),
        Some(Value::String(value)) => parse_u32(value).map(Some).ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("Andor property {key} must be a decimal or 0x-prefixed u32"),
            )
        }),
        Some(Value::Null) => Ok(None),
        _ => Ok(default),
    }
}

fn i32_prop(device: &DeviceConfig, key: &str) -> Result<Option<i32>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => i32::try_from(*value).map(Some).map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("Andor property {key} must fit in signed 32-bit integer"),
            )
        }),
        _ => Ok(None),
    }
}

fn positive_i64_prop(device: &DeviceConfig, key: &str) -> Result<Option<i64>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if *value > 0 => Ok(Some(*value)),
        Some(Value::I64(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Andor property {key} must be positive"),
        )),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Andor property {key} must be I64"),
        )),
        None => Ok(None),
    }
}

fn time_interval_prop(device: &DeviceConfig, key: &str) -> Result<Option<TimeInterval>> {
    match device.properties.get(key) {
        Some(Value::TimeInterval(value))
            if value.seconds().is_finite() && value.seconds() > 0.0 =>
        {
            Ok(Some(*value))
        }
        Some(Value::F64(value)) if value.is_finite() && *value > 0.0 => {
            Ok(Some(TimeInterval::from_seconds(*value)))
        }
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Andor property {key} must be a positive TimeInterval"),
        )),
        None => Ok(None),
    }
}

fn optional_string_prop(
    device: &DeviceConfig,
    key: &str,
    default: Option<String>,
) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) if value.is_empty() || value == "none" => None,
        Some(Value::String(value)) => Some(value.clone()),
        Some(Value::Null) => None,
        _ => default,
    }
}

fn bytes_prop(device: &DeviceConfig, key: &str) -> Result<Option<Vec<u8>>> {
    match device.properties.get(key) {
        Some(Value::Bytes(value)) => Ok(Some(value.clone())),
        Some(Value::List(values)) => values
            .iter()
            .map(|value| match value {
                Value::I64(value) if (0..=u8::MAX as i64).contains(value) => Ok(*value as u8),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Andor property {key} list entries must be unsigned bytes"),
                )),
            })
            .collect::<Result<Vec<_>>>()
            .map(Some),
        _ => Ok(None),
    }
}

fn pixel_count_prop(device: &DeviceConfig, key: &str) -> Result<Option<u32>> {
    match device.properties.get(key) {
        Some(Value::PixelCount(value)) => Ok(Some(value.pixels())),
        Some(Value::I64(value)) if (1..=u32::MAX as i64).contains(value) => Ok(Some(*value as u32)),
        Some(Value::I64(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Andor property {key} must fit in a positive unsigned 32-bit pixel count"),
        )),
        _ => Ok(None),
    }
}

fn parse_u16(value: &str) -> Option<u16> {
    let value = value.trim();
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u16::from_str_radix(hex, 16).ok()
    } else {
        value.parse().ok()
    }
}

fn parse_u32(value: &str) -> Option<u32> {
    let value = value.trim();
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16).ok()
    } else {
        value.parse().ok()
    }
}

fn validate_sdk3_enum_value(key: &str, value: &str, allowed: &[&str]) -> Result<()> {
    if allowed.iter().any(|candidate| *candidate == value) {
        Ok(())
    } else {
        Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Andor SDK3 {key} must be one of {}", allowed.join(", ")),
        ))
    }
}

fn checked_u32_value(value: i64, name: &str) -> Result<u32> {
    u32::try_from(value).map_err(|_| {
        Error::new(
            ErrorCode::Transport,
            format!("Andor SDK3 {name} value is outside unsigned 32-bit range"),
        )
    })
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

fn invalid_property<T>(message: &str, key: &str) -> Result<T> {
    Err(Error::new(
        ErrorCode::InvalidProperty,
        format!("{message}: {key}"),
    ))
}

mod live_sdk2_runtime {
    use super::*;
    use libloading::Symbol;
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int, c_long, c_ulong};

    const DRV_SUCCESS: u32 = 20002;
    const DRV_TEMPERATURE_OFF: u32 = 20034;
    const DRV_TEMPERATURE_NOT_STABILIZED: u32 = 20035;
    const DRV_TEMPERATURE_STABILIZED: u32 = 20036;
    const DRV_TEMPERATURE_NOT_REACHED: u32 = 20037;
    const DRV_TEMPERATURE_OUT_RANGE: u32 = 20038;
    const DRV_TEMPERATURE_NOT_SUPPORTED: u32 = 20039;
    const DRV_TEMPERATURE_DRIFT: u32 = 20040;

    type Initialize = unsafe extern "system" fn(*const c_char) -> u32;
    type ShutDown = unsafe extern "system" fn() -> u32;
    type GetCameraHandle = unsafe extern "system" fn(c_long, *mut c_long) -> u32;
    type SetCurrentCamera = unsafe extern "system" fn(c_long) -> u32;
    type SetTemperature = unsafe extern "system" fn(c_int) -> u32;
    type GetTemperature = unsafe extern "system" fn(*mut c_int) -> u32;
    type GetTemperatureRange = unsafe extern "system" fn(*mut c_int, *mut c_int) -> u32;
    type CoolerControl = unsafe extern "system" fn() -> u32;
    type GetDetector = unsafe extern "system" fn(*mut c_int, *mut c_int) -> u32;
    type SetAcquisitionMode = unsafe extern "system" fn(c_int) -> u32;
    type SetReadMode = unsafe extern "system" fn(c_int) -> u32;
    type SetImage = unsafe extern "system" fn(c_int, c_int, c_int, c_int, c_int, c_int) -> u32;
    type SetExposureTime = unsafe extern "system" fn(f32) -> u32;
    type StartAcquisition = unsafe extern "system" fn() -> u32;
    type WaitForAcquisitionTimeOut = unsafe extern "system" fn(c_int) -> u32;
    type GetAcquiredData16 = unsafe extern "system" fn(*mut u16, c_ulong) -> u32;
    type AbortAcquisition = unsafe extern "system" fn() -> u32;

    pub(super) struct Sdk2RuntimeFrame {
        pub(super) width: u32,
        pub(super) height: u32,
        pub(super) data: Vec<u8>,
    }

    struct Atmcd {
        library: Library,
        initialize: Initialize,
        shutdown: ShutDown,
        set_temperature: SetTemperature,
        get_temperature: GetTemperature,
        get_temperature_range: GetTemperatureRange,
        cooler_on: CoolerControl,
        cooler_off: CoolerControl,
        get_detector: GetDetector,
        set_acquisition_mode: SetAcquisitionMode,
        set_read_mode: SetReadMode,
        set_image: SetImage,
        set_exposure_time: SetExposureTime,
        start_acquisition: StartAcquisition,
        wait_for_acquisition_timeout: WaitForAcquisitionTimeOut,
        get_acquired_data16: GetAcquiredData16,
        abort_acquisition: AbortAcquisition,
    }

    impl Atmcd {
        fn load(path: &str) -> Result<Self> {
            let library = unsafe { Library::new(path) }.map_err(|error| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Andor SDK2 runtime load failed: {error}"),
                )
            })?;
            unsafe {
                let initialize = symbol::<Initialize>(&library, b"Initialize")?;
                let shutdown = symbol::<ShutDown>(&library, b"ShutDown")?;
                let set_temperature = symbol::<SetTemperature>(&library, b"SetTemperature")?;
                let get_temperature = symbol::<GetTemperature>(&library, b"GetTemperature")?;
                let get_temperature_range =
                    symbol::<GetTemperatureRange>(&library, b"GetTemperatureRange")?;
                let cooler_on = symbol::<CoolerControl>(&library, b"CoolerON")?;
                let cooler_off = symbol::<CoolerControl>(&library, b"CoolerOFF")?;
                let get_detector = symbol::<GetDetector>(&library, b"GetDetector")?;
                let set_acquisition_mode =
                    symbol::<SetAcquisitionMode>(&library, b"SetAcquisitionMode")?;
                let set_read_mode = symbol::<SetReadMode>(&library, b"SetReadMode")?;
                let set_image = symbol::<SetImage>(&library, b"SetImage")?;
                let set_exposure_time = symbol::<SetExposureTime>(&library, b"SetExposureTime")?;
                let start_acquisition = symbol::<StartAcquisition>(&library, b"StartAcquisition")?;
                let wait_for_acquisition_timeout =
                    symbol::<WaitForAcquisitionTimeOut>(&library, b"WaitForAcquisitionTimeOut")?;
                let get_acquired_data16 =
                    symbol::<GetAcquiredData16>(&library, b"GetAcquiredData16")?;
                let abort_acquisition = symbol::<AbortAcquisition>(&library, b"AbortAcquisition")?;
                Ok(Self {
                    library,
                    initialize,
                    shutdown,
                    set_temperature,
                    get_temperature,
                    get_temperature_range,
                    cooler_on,
                    cooler_off,
                    get_detector,
                    set_acquisition_mode,
                    set_read_mode,
                    set_image,
                    set_exposure_time,
                    start_acquisition,
                    wait_for_acquisition_timeout,
                    get_acquired_data16,
                    abort_acquisition,
                })
            }
        }

        fn check(&self, code: u32, operation: &str) -> Result<()> {
            if code == DRV_SUCCESS {
                Ok(())
            } else {
                Err(Error::new(
                    ErrorCode::Transport,
                    format!(
                        "Andor SDK2 {operation} failed with {}",
                        sdk2_status_name(code)
                    ),
                ))
            }
        }

        fn select_camera(&self, camera_index: i32) -> Result<()> {
            if camera_index == 0 {
                return Ok(());
            }
            if camera_index < 0 {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Andor SDK2 camera_index must be non-negative",
                ));
            }
            let get_camera_handle =
                unsafe { optional_symbol::<GetCameraHandle>(&self.library, b"GetCameraHandle") };
            let set_current_camera =
                unsafe { optional_symbol::<SetCurrentCamera>(&self.library, b"SetCurrentCamera") };
            let (Some(get_camera_handle), Some(set_current_camera)) =
                (get_camera_handle, set_current_camera)
            else {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Andor SDK2 camera_index selection requires GetCameraHandle and SetCurrentCamera",
                ));
            };
            let mut handle: c_long = 0;
            self.check(
                unsafe { get_camera_handle(camera_index as c_long, &mut handle) },
                "GetCameraHandle",
            )?;
            self.check(unsafe { set_current_camera(handle) }, "SetCurrentCamera")
        }

        fn temperature_range(&self) -> Result<(i32, i32)> {
            let mut min = 0;
            let mut max = 0;
            self.check(
                unsafe { (self.get_temperature_range)(&mut min, &mut max) },
                "GetTemperatureRange",
            )?;
            Ok((min, max))
        }

        fn temperature(&self) -> Result<(i32, String)> {
            let mut temperature = 0;
            let status = unsafe { (self.get_temperature)(&mut temperature) };
            match status {
                DRV_TEMPERATURE_OFF
                | DRV_TEMPERATURE_NOT_STABILIZED
                | DRV_TEMPERATURE_STABILIZED
                | DRV_TEMPERATURE_NOT_REACHED
                | DRV_TEMPERATURE_OUT_RANGE
                | DRV_TEMPERATURE_DRIFT => {
                    Ok((temperature, sdk2_temperature_status(status).to_string()))
                }
                DRV_TEMPERATURE_NOT_SUPPORTED => Err(Error::new(
                    ErrorCode::Unsupported,
                    "Andor SDK2 GetTemperature reports temperature control is not supported",
                )),
                other => Err(Error::new(
                    ErrorCode::Transport,
                    format!(
                        "Andor SDK2 GetTemperature failed with {}",
                        sdk2_status_name(other)
                    ),
                )),
            }
        }

        fn detector(&self) -> Result<(u32, u32)> {
            let mut width = 0;
            let mut height = 0;
            self.check(
                unsafe { (self.get_detector)(&mut width, &mut height) },
                "GetDetector",
            )?;
            if width <= 0 || height <= 0 {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Andor SDK2 GetDetector returned invalid geometry {width}x{height}"),
                ));
            }
            Ok((width as u32, height as u32))
        }

        fn set_exposure_time(&self, exposure: TimeInterval) -> Result<()> {
            let seconds = exposure.seconds();
            if !seconds.is_finite() || seconds <= 0.0 || seconds > f32::MAX as f64 {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Andor SDK2 exposure must be a positive finite interval",
                ));
            }
            self.check(
                unsafe { (self.set_exposure_time)(seconds as f32) },
                "SetExposureTime",
            )
        }

        fn capture(
            &self,
            configured_width: u32,
            configured_height: u32,
            exposure: TimeInterval,
        ) -> Result<Sdk2RuntimeFrame> {
            let (detector_width, detector_height) = self.detector()?;
            let width = if configured_width == 0 {
                detector_width
            } else {
                configured_width.min(detector_width)
            };
            let height = if configured_height == 0 {
                detector_height
            } else {
                configured_height.min(detector_height)
            };
            self.check(
                unsafe { (self.set_acquisition_mode)(1) },
                "SetAcquisitionMode",
            )?;
            self.check(unsafe { (self.set_read_mode)(4) }, "SetReadMode")?;
            self.set_exposure_time(exposure)?;
            self.check(
                unsafe { (self.set_image)(1, 1, 1, width as c_int, 1, height as c_int) },
                "SetImage",
            )?;
            self.check(unsafe { (self.start_acquisition)() }, "StartAcquisition")?;
            let timeout_ms = sdk2_capture_timeout_ms(exposure)?;
            let wait = unsafe { (self.wait_for_acquisition_timeout)(timeout_ms) };
            if wait != DRV_SUCCESS {
                unsafe {
                    (self.abort_acquisition)();
                }
                self.check(wait, "WaitForAcquisitionTimeOut")?;
            }
            let pixels = width as usize * height as usize;
            let mut raw = vec![0u16; pixels];
            self.check(
                unsafe { (self.get_acquired_data16)(raw.as_mut_ptr(), pixels as c_ulong) },
                "GetAcquiredData16",
            )?;
            let mut data = Vec::with_capacity(pixels * 2);
            for pixel in raw {
                data.extend_from_slice(&pixel.to_le_bytes());
            }
            Ok(Sdk2RuntimeFrame {
                width,
                height,
                data,
            })
        }
    }

    fn with_camera<T>(
        path: &str,
        camera_index: i32,
        action: impl FnOnce(&Atmcd) -> Result<T>,
    ) -> Result<T> {
        let runtime = Atmcd::load(path)?;
        let init_dir = CString::new("").expect("static init path");
        runtime.check(
            unsafe { (runtime.initialize)(init_dir.as_ptr()) },
            "Initialize",
        )?;
        let _guard = ShutdownGuard { runtime: &runtime };
        runtime.select_camera(camera_index)?;
        action(&runtime)
    }

    pub fn set_temperature(path: &str, camera_index: i32, target_celsius: i32) -> Result<()> {
        with_camera(path, camera_index, |runtime| {
            let (min, max) = runtime.temperature_range()?;
            if target_celsius < min || target_celsius > max {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!(
                        "Andor SDK2 temperature target {target_celsius} deg C is outside runtime range {min}..={max} deg C"
                    ),
                ));
            }
            runtime.check(
                unsafe { (runtime.set_temperature)(target_celsius) },
                "SetTemperature",
            )
        })
    }

    pub fn set_cooler(path: &str, camera_index: i32, enabled: bool) -> Result<()> {
        with_camera(path, camera_index, |runtime| {
            let operation = if enabled { "CoolerON" } else { "CoolerOFF" };
            let function = if enabled {
                runtime.cooler_on
            } else {
                runtime.cooler_off
            };
            runtime.check(unsafe { function() }, operation)
        })
    }

    pub fn get_temperature(path: &str, camera_index: i32) -> Result<(i32, String)> {
        with_camera(path, camera_index, |runtime| runtime.temperature())
    }

    pub fn get_detector(path: &str, camera_index: i32) -> Result<(u32, u32)> {
        with_camera(path, camera_index, |runtime| runtime.detector())
    }

    pub fn set_exposure_time(path: &str, camera_index: i32, exposure: TimeInterval) -> Result<()> {
        with_camera(path, camera_index, |runtime| {
            runtime.set_exposure_time(exposure)
        })
    }

    pub fn capture(
        path: &str,
        camera_index: i32,
        width: u32,
        height: u32,
        exposure: TimeInterval,
    ) -> Result<Sdk2RuntimeFrame> {
        with_camera(path, camera_index, |runtime| {
            runtime.capture(width, height, exposure)
        })
    }

    struct ShutdownGuard<'a> {
        runtime: &'a Atmcd,
    }

    impl Drop for ShutdownGuard<'_> {
        fn drop(&mut self) {
            unsafe {
                (self.runtime.shutdown)();
            }
        }
    }

    unsafe fn symbol<T: Copy>(library: &Library, name: &[u8]) -> Result<T> {
        let symbol: Symbol<'_, T> = library.get(name).map_err(|error| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!(
                    "Andor SDK2 runtime missing symbol {}: {error}",
                    String::from_utf8_lossy(name)
                ),
            )
        })?;
        Ok(*symbol)
    }

    unsafe fn optional_symbol<T: Copy>(library: &Library, name: &[u8]) -> Option<T> {
        library.get::<T>(name).ok().map(|symbol| *symbol)
    }

    fn sdk2_capture_timeout_ms(exposure: TimeInterval) -> Result<c_int> {
        let timeout_ms = (exposure.seconds() * 1000.0 + 10_000.0).ceil();
        if !timeout_ms.is_finite() || timeout_ms <= 0.0 || timeout_ms > c_int::MAX as f64 {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Andor SDK2 capture timeout cannot be represented",
            ));
        }
        Ok(timeout_ms as c_int)
    }

    fn sdk2_temperature_status(code: u32) -> &'static str {
        match code {
            DRV_TEMPERATURE_OFF => "Off",
            DRV_TEMPERATURE_NOT_STABILIZED => "NotStabilized",
            DRV_TEMPERATURE_STABILIZED => "Stabilized",
            DRV_TEMPERATURE_NOT_REACHED => "NotReached",
            DRV_TEMPERATURE_OUT_RANGE => "OutOfRange",
            DRV_TEMPERATURE_DRIFT => "Drift",
            _ => "Unknown",
        }
    }

    fn sdk2_status_name(code: u32) -> String {
        match code {
            DRV_SUCCESS => "DRV_SUCCESS".into(),
            DRV_TEMPERATURE_OFF => "DRV_TEMPERATURE_OFF".into(),
            DRV_TEMPERATURE_NOT_STABILIZED => "DRV_TEMPERATURE_NOT_STABILIZED".into(),
            DRV_TEMPERATURE_STABILIZED => "DRV_TEMPERATURE_STABILIZED".into(),
            DRV_TEMPERATURE_NOT_REACHED => "DRV_TEMPERATURE_NOT_REACHED".into(),
            DRV_TEMPERATURE_OUT_RANGE => "DRV_TEMPERATURE_OUT_RANGE".into(),
            DRV_TEMPERATURE_NOT_SUPPORTED => "DRV_TEMPERATURE_NOT_SUPPORTED".into(),
            DRV_TEMPERATURE_DRIFT => "DRV_TEMPERATURE_DRIFT".into(),
            other => format!("DRV_{other}"),
        }
    }
}

#[cfg(feature = "os-usb")]
mod live_sdk2 {
    use super::*;
    use futures_lite::future::block_on;
    use nusb::transfer::{ControlIn, ControlOut, ControlType, Recipient, RequestBuffer};

    pub fn capture(
        identity: &Option<AndorUsbIdentity>,
        vendor_id: u16,
        product_id: u16,
        serial_number: Option<&str>,
        firmware_loaded: bool,
        firmware_path: Option<&str>,
        pixels: usize,
    ) -> Result<Vec<u8>> {
        if !firmware_loaded {
            let path = firmware_path.ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    "Andor SDK2 firmware package path is required for pre-firmware initialization",
                )
            })?;
            upload_fx2_firmware(identity, path)?;
            std::thread::sleep(std::time::Duration::from_millis(1500));
        }
        let device = nusb::list_devices()
            .map_err(|error| usb_error(format!("Andor USB device listing failed: {error}")))?
            .find(|device| {
                if device.vendor_id() != vendor_id || device.product_id() != product_id {
                    return false;
                }
                if let Some(identity) = identity {
                    return device.bus_number() == identity.bus_number
                        && device.device_address() == identity.device_address;
                }
                match serial_number {
                    Some(expected) => device.serial_number() == Some(expected),
                    None => true,
                }
            })
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::Transport,
                    "configured Andor SDK2 USB device not found",
                )
            })?;

        let device = device
            .open()
            .map_err(|error| usb_error(format!("Andor SDK2 USB open failed: {error}")))?;
        let interface = device.detach_and_claim_interface(0).map_err(|error| {
            usb_error(format!(
                "Andor SDK2 claim interface 0 failed: {error}{}",
                crate::usb_discovery::usb_claim_hint(vendor_id, product_id, 0)
            ))
        })?;
        let _ = interface.set_alt_setting(0);

        vendor_in(&interface, SDK2_IDENTITY_REQUEST, 0, 0, 6)?;
        vendor_out(&interface, SDK2_FIFO_RESET_REQUEST, 0, 0, &[])?;
        vendor_out(
            &interface,
            SDK2_ACQUISITION_CONTROL_REQUEST,
            SDK2_ACQUISITION_CLEAR,
            0,
            &[],
        )?;
        vendor_out(
            &interface,
            SDK2_ACQUISITION_CONTROL_REQUEST,
            SDK2_ACQUISITION_START,
            0,
            &[],
        )?;
        let padded = pixels.div_ceil(SDK2_READOUT_ALIGNMENT_PIXELS as usize)
            * SDK2_READOUT_ALIGNMENT_PIXELS as usize;
        let mut data = block_on(interface.bulk_in(
            SDK2_BULK_IN_ENDPOINT,
            RequestBuffer::new(padded * SDK2_READOUT_BYTES_PER_PIXEL as usize),
        ))
        .into_result()
        .map_err(|error| usb_error(format!("Andor SDK2 bulk-IN frame read failed: {error}")))?;
        let _ = vendor_out(
            &interface,
            SDK2_ACQUISITION_CONTROL_REQUEST,
            SDK2_ACQUISITION_STOP,
            0,
            &[],
        );
        data.truncate(pixels * SDK2_READOUT_BYTES_PER_PIXEL as usize);
        Ok(data)
    }

    pub(super) fn read_sdk3_status_byte(
        identity: &Option<AndorUsbIdentity>,
        vendor_id: u16,
        product_id: u16,
        serial_number: Option<&str>,
    ) -> Result<u8> {
        let interface = open_andor_interface(identity, vendor_id, product_id, serial_number)?;
        let bytes = vendor_in(&interface, SDK3_STATUS8_REQUEST, 0, 0, 1)?;
        bytes.first().copied().ok_or_else(|| {
            Error::new(
                ErrorCode::Transport,
                "Andor SDK3 status-byte readback returned no data",
            )
        })
    }

    pub(super) fn read_sdk3_status_word(
        identity: &Option<AndorUsbIdentity>,
        vendor_id: u16,
        product_id: u16,
        serial_number: Option<&str>,
    ) -> Result<u32> {
        let interface = open_andor_interface(identity, vendor_id, product_id, serial_number)?;
        let bytes = vendor_in(&interface, SDK3_STATUS32_REQUEST, 0, 0, 4)?;
        if bytes.len() < 4 {
            return Err(Error::new(
                ErrorCode::Transport,
                "Andor SDK3 status-word readback returned fewer than four bytes",
            ));
        }
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn open_andor_interface(
        identity: &Option<AndorUsbIdentity>,
        vendor_id: u16,
        product_id: u16,
        serial_number: Option<&str>,
    ) -> Result<nusb::Interface> {
        let device = nusb::list_devices()
            .map_err(|error| usb_error(format!("Andor USB device listing failed: {error}")))?
            .find(|device| {
                if device.vendor_id() != vendor_id || device.product_id() != product_id {
                    return false;
                }
                if let Some(identity) = identity {
                    return device.bus_number() == identity.bus_number
                        && device.device_address() == identity.device_address;
                }
                match serial_number {
                    Some(expected) => device.serial_number() == Some(expected),
                    None => true,
                }
            })
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::Transport,
                    "configured Andor USB device not found",
                )
            })?;
        let device = device
            .open()
            .map_err(|error| usb_error(format!("Andor USB open failed: {error}")))?;
        let interface = device.detach_and_claim_interface(0).map_err(|error| {
            usb_error(format!(
                "Andor claim interface 0 failed: {error}{}",
                crate::usb_discovery::usb_claim_hint(vendor_id, product_id, 0)
            ))
        })?;
        let _ = interface.set_alt_setting(0);
        Ok(interface)
    }

    fn vendor_out(
        interface: &nusb::Interface,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
    ) -> Result<()> {
        block_on(interface.control_out(ControlOut {
            control_type: ControlType::Vendor,
            recipient: Recipient::Device,
            request,
            value,
            index,
            data,
        }))
        .into_result()
        .map(|_| ())
        .map_err(|error| {
            usb_error(format!(
                "Andor SDK2 control_out req=0x{request:02x} val=0x{value:04x} idx=0x{index:04x} failed: {error}"
            ))
        })
    }

    fn vendor_in(
        interface: &nusb::Interface,
        request: u8,
        value: u16,
        index: u16,
        length: u16,
    ) -> Result<Vec<u8>> {
        block_on(interface.control_in(ControlIn {
            control_type: ControlType::Vendor,
            recipient: Recipient::Device,
            request,
            value,
            index,
            length,
        }))
        .into_result()
        .map_err(|error| {
            usb_error(format!(
                "Andor SDK2 control_in req=0x{request:02x} val=0x{value:04x} idx=0x{index:04x} failed: {error}"
            ))
        })
    }

    fn usb_error(message: impl Into<String>) -> Error {
        Error::new(ErrorCode::Transport, message.into())
    }

    /// Intel-HEX text of an SDK2 firmware package: the configured path, falling
    /// back to a compiled-in image of the same name. The path stays required —
    /// this package ships four scoped images and the driver has no evidence for
    /// which one a given unit needs.
    fn firmware_package_text(path: &str) -> Result<String> {
        if let Ok(text) = std::fs::read_to_string(path) {
            return Ok(text);
        }
        crate::bundled_firmware::image_by_name(path)
            .map(str::to_string)
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!(
                        "Andor SDK2 firmware package {path} cannot be read and no image of \
                         that name is bundled"
                    ),
                )
            })
    }

    /// Budget for one anchor-download control transfer.
    const FIRMWARE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

    pub(super) fn upload_fx2_firmware(
        identity: &Option<AndorUsbIdentity>,
        path: &str,
    ) -> Result<()> {
        let text = firmware_package_text(path)?;
        let records = crate::ez_usb::parse_ihex(&text)?;
        let device = select_fx2_loader(identity)?;
        let (vendor_id, product_id) = (device.vendor_id(), device.product_id());
        let device = device.open().map_err(|error| {
            usb_error(format!("Andor FX2 firmware-loader open failed: {error}"))
        })?;
        let interface = device.detach_and_claim_interface(0).map_err(|error| {
            usb_error(format!(
                "Andor FX2 firmware-loader claim interface 0 failed: {error}{}",
                crate::usb_discovery::usb_claim_hint(vendor_id, product_id, 0)
            ))
        })?;
        // Shared EZ-USB anchor download: hold the 8051, write every record,
        // release. One transfer per Intel-HEX record rather than coalesced
        // 1 KiB blocks — that is what the FX2 programming model specifies and
        // what has been verified record-for-record against captured traffic on
        // another FX2 camera in this repository. This path has never been run
        // on Andor hardware, so it uses the code that has been.
        crate::ez_usb::hold_8051(&interface, true, FIRMWARE_TIMEOUT)?;
        for record in &records {
            crate::ez_usb::anchor_write(
                &interface,
                record.address,
                &record.data,
                FIRMWARE_TIMEOUT,
            )?;
        }
        crate::ez_usb::hold_8051(&interface, false, FIRMWARE_TIMEOUT)
    }

    pub(super) fn upload_fx3_firmware(
        identity: &Option<AndorUsbIdentity>,
        path: &str,
    ) -> Result<()> {
        const FX3_CHUNK: usize = 4096;
        let bytes = std::fs::read(path).map_err(|error| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("Andor SDK3 firmware package cannot be read: {error}"),
            )
        })?;
        let image = parse_fx3_image(&bytes)?;
        let device = select_loader(identity, CYPRESS_FX3_PID, "FX3")?;
        let (vendor_id, product_id) = (device.vendor_id(), device.product_id());
        let device = device.open().map_err(|error| {
            usb_error(format!("Andor FX3 firmware-loader open failed: {error}"))
        })?;
        let interface = device.detach_and_claim_interface(0).map_err(|error| {
            usb_error(format!(
                "Andor FX3 firmware-loader claim interface 0 failed: {error}{}",
                crate::usb_discovery::usb_claim_hint(vendor_id, product_id, 0)
            ))
        })?;
        for (base, data) in image.sections {
            for (offset, chunk) in data.chunks(FX3_CHUNK).enumerate() {
                let address = base.wrapping_add((offset * FX3_CHUNK) as u32);
                raw_vendor_out(
                    &interface,
                    0xa0,
                    (address & 0xffff) as u16,
                    (address >> 16) as u16,
                    chunk,
                )?;
            }
        }
        raw_vendor_out(
            &interface,
            0xa0,
            (image.entry & 0xffff) as u16,
            (image.entry >> 16) as u16,
            &[],
        )
    }

    fn select_fx2_loader(identity: &Option<AndorUsbIdentity>) -> Result<nusb::DeviceInfo> {
        select_loader(identity, CYPRESS_FX2_PID, "FX2")
    }

    pub(super) fn select_loaded_andor_runtime(
        configured_product_id: u16,
    ) -> Result<Option<AndorUsbIdentity>> {
        let runtime_product_is_known =
            !matches!(configured_product_id, CYPRESS_FX2_PID | CYPRESS_FX3_PID);
        let matches = nusb::list_devices()
            .map_err(|error| usb_error(format!("Andor USB device listing failed: {error}")))?
            .filter(|device| device.vendor_id() == ANDOR_VID)
            .filter(|device| {
                !runtime_product_is_known || device.product_id() == configured_product_id
            })
            .collect::<Vec<_>>();
        match matches.len() {
            1 => {
                let device = matches.into_iter().next().expect("one Andor runtime device");
                let product_id = device.product_id();
                let product = device
                    .product_string()
                    .map(str::to_string)
                    .unwrap_or_else(|| AndorModel::from_pid(product_id).name().into());
                Ok(Some(AndorUsbIdentity {
                    product,
                    serial: device.serial_number().map(str::to_string),
                    vendor_id: device.vendor_id(),
                    product_id,
                    bus_number: device.bus_number(),
                    device_address: device.device_address(),
                    firmware_loaded: true,
                }))
            }
            0 => Ok(None),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                "Andor firmware probe produced multiple matching runtime devices; configure a product_id or connect one device",
            )),
        }
    }

    fn select_loader(
        identity: &Option<AndorUsbIdentity>,
        product_id: u16,
        loader_name: &str,
    ) -> Result<nusb::DeviceInfo> {
        let matches = nusb::list_devices()
            .map_err(|error| usb_error(format!("Andor USB device listing failed: {error}")))?
            .filter(|device| device.vendor_id() == CYPRESS_VID && device.product_id() == product_id)
            .filter(|device| {
                identity
                    .as_ref()
                    .map(|identity| {
                        device.bus_number() == identity.bus_number
                            && device.device_address() == identity.device_address
                    })
                    .unwrap_or(true)
            })
            .collect::<Vec<_>>();
        match matches.len() {
            1 => Ok(matches.into_iter().next().expect("one FX2 loader candidate")),
            0 => Err(Error::new(
                ErrorCode::Transport,
                format!(
                    "no Andor {loader_name} pre-firmware device found for hidden firmware initialization"
                ),
            )),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!(
                    "multiple {loader_name} pre-firmware devices found; configure usb_identity or connect one device"
                ),
            )),
        }
    }

    /// Vendor control OUT on a **claimed interface**, not on the device.
    ///
    /// `nusb::Device::control_out` is Linux/macOS-only: WinUSB routes control
    /// transfers through an interface handle, so the device-level call does not
    /// exist on Windows. Going through the interface works on every platform.
    fn raw_vendor_out(
        interface: &nusb::Interface,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
    ) -> Result<()> {
        block_on(interface.control_out(ControlOut {
            control_type: ControlType::Vendor,
            recipient: Recipient::Device,
            request,
            value,
            index,
            data,
        }))
        .into_result()
        .map(|_| ())
        .map_err(|error| {
            usb_error(format!(
                "Andor FX2 firmware control_out req=0x{request:02x} val=0x{value:04x} idx=0x{index:04x} failed: {error}"
            ))
        })
    }

    struct Fx3Image {
        sections: Vec<(u32, Vec<u8>)>,
        entry: u32,
    }

    fn parse_fx3_image(bytes: &[u8]) -> Result<Fx3Image> {
        if bytes.len() < 4 || &bytes[0..2] != b"CY" {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Andor SDK3 firmware package is not a Cypress FX3 CY image",
            ));
        }
        let mut offset = 4;
        let mut sections = Vec::new();
        loop {
            if offset + 8 > bytes.len() {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Andor SDK3 firmware package has a truncated FX3 section header",
                ));
            }
            let length_words = u32::from_le_bytes(
                bytes[offset..offset + 4]
                    .try_into()
                    .expect("four-byte length"),
            );
            let address = u32::from_le_bytes(
                bytes[offset + 4..offset + 8]
                    .try_into()
                    .expect("four-byte address"),
            );
            offset += 8;
            if length_words == 0 {
                if sections.is_empty() {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Andor SDK3 firmware package contains no FX3 sections",
                    ));
                }
                return Ok(Fx3Image {
                    sections,
                    entry: address,
                });
            }
            let length_bytes = length_words as usize * 4;
            if offset + length_bytes > bytes.len() {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Andor SDK3 firmware package FX3 section overruns file",
                ));
            }
            sections.push((address, bytes[offset..offset + length_bytes].to_vec()));
            offset += length_bytes;
        }
    }
}

#[cfg(not(feature = "os-usb"))]
mod live_sdk3 {
    use super::*;

    fn unsupported<T>() -> Result<T> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Andor SDK3 vendor-runtime features require numanager-drivers/os-usb",
        ))
    }

    pub fn get_int(_path: &str, _camera_index: i32, _feature: &str) -> Result<i64> {
        unsupported()
    }

    pub fn get_float(_path: &str, _camera_index: i32, _feature: &str) -> Result<f64> {
        unsupported()
    }

    pub fn get_bool(_path: &str, _camera_index: i32, _feature: &str) -> Result<bool> {
        unsupported()
    }

    pub fn get_enum_string(_path: &str, _camera_index: i32, _feature: &str) -> Result<String> {
        unsupported()
    }

    pub fn set_int(_path: &str, _camera_index: i32, _feature: &str, _value: i64) -> Result<()> {
        unsupported()
    }

    pub fn set_float(_path: &str, _camera_index: i32, _feature: &str, _value: f64) -> Result<()> {
        unsupported()
    }

    pub fn set_bool(_path: &str, _camera_index: i32, _feature: &str, _value: bool) -> Result<()> {
        unsupported()
    }

    pub fn set_enum_string(
        _path: &str,
        _camera_index: i32,
        _feature: &str,
        _value: &str,
    ) -> Result<()> {
        unsupported()
    }
}

#[cfg(feature = "os-usb")]
mod live_sdk3 {
    use super::*;
    use libloading::Symbol;
    use std::ffi::c_int;
    use std::ptr;

    const AT_SUCCESS: c_int = 0;
    const AT_HANDLE_SYSTEM: c_int = 1;

    #[cfg(windows)]
    type AtWc = u16;
    #[cfg(not(windows))]
    type AtWc = i32;

    type AtInitialiseLibrary = unsafe extern "system" fn() -> c_int;
    type AtFinaliseLibrary = unsafe extern "system" fn() -> c_int;
    type AtOpen = unsafe extern "system" fn(c_int, *mut c_int) -> c_int;
    type AtClose = unsafe extern "system" fn(c_int) -> c_int;
    type AtGetInt = unsafe extern "system" fn(c_int, *const AtWc, *mut i64) -> c_int;
    type AtGetFloat = unsafe extern "system" fn(c_int, *const AtWc, *mut f64) -> c_int;
    type AtSetInt = unsafe extern "system" fn(c_int, *const AtWc, i64) -> c_int;
    type AtSetFloat = unsafe extern "system" fn(c_int, *const AtWc, f64) -> c_int;
    type AtGetBool = unsafe extern "system" fn(c_int, *const AtWc, *mut c_int) -> c_int;
    type AtSetBool = unsafe extern "system" fn(c_int, *const AtWc, c_int) -> c_int;
    type AtGetEnumIndex = unsafe extern "system" fn(c_int, *const AtWc, *mut c_int) -> c_int;
    type AtSetEnumString = unsafe extern "system" fn(c_int, *const AtWc, *const AtWc) -> c_int;
    type AtGetEnumStringByIndex =
        unsafe extern "system" fn(c_int, *const AtWc, c_int, *mut AtWc, c_int) -> c_int;
    type AtCommand = unsafe extern "system" fn(c_int, *const AtWc) -> c_int;
    type AtQueueBuffer = unsafe extern "system" fn(c_int, *mut u8, c_int) -> c_int;
    type AtWaitBuffer = unsafe extern "system" fn(c_int, *mut *mut u8, *mut c_int, u32) -> c_int;
    type AtFlush = unsafe extern "system" fn(c_int) -> c_int;

    pub struct Sdk3Frame {
        pub width: u32,
        pub height: u32,
        pub data: Vec<u8>,
    }

    struct AtCore {
        _library: Library,
        initialise_library: AtInitialiseLibrary,
        finalise_library: AtFinaliseLibrary,
        open: AtOpen,
        close: AtClose,
        get_int: AtGetInt,
        get_float: AtGetFloat,
        set_int: AtSetInt,
        set_float: AtSetFloat,
        get_bool: AtGetBool,
        set_bool: AtSetBool,
        get_enum_index: AtGetEnumIndex,
        set_enum_string: AtSetEnumString,
        get_enum_string_by_index: AtGetEnumStringByIndex,
        command: AtCommand,
        queue_buffer: AtQueueBuffer,
        wait_buffer: AtWaitBuffer,
        flush: AtFlush,
    }

    impl AtCore {
        fn load(path: &str) -> Result<Self> {
            let library = unsafe { Library::new(path) }.map_err(|error| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Andor SDK3 runtime load failed: {error}"),
                )
            })?;
            unsafe {
                let initialise_library =
                    symbol::<AtInitialiseLibrary>(&library, b"AT_InitialiseLibrary")?;
                let finalise_library =
                    symbol::<AtFinaliseLibrary>(&library, b"AT_FinaliseLibrary")?;
                let open = symbol::<AtOpen>(&library, b"AT_Open")?;
                let close = symbol::<AtClose>(&library, b"AT_Close")?;
                let get_int = symbol::<AtGetInt>(&library, b"AT_GetInt")?;
                let get_float = symbol::<AtGetFloat>(&library, b"AT_GetFloat")?;
                let set_int = symbol::<AtSetInt>(&library, b"AT_SetInt")?;
                let set_float = symbol::<AtSetFloat>(&library, b"AT_SetFloat")?;
                let get_bool = symbol::<AtGetBool>(&library, b"AT_GetBool")?;
                let set_bool = symbol::<AtSetBool>(&library, b"AT_SetBool")?;
                let get_enum_index = symbol::<AtGetEnumIndex>(&library, b"AT_GetEnumIndex")?;
                let set_enum_string = symbol::<AtSetEnumString>(&library, b"AT_SetEnumString")?;
                let get_enum_string_by_index =
                    symbol::<AtGetEnumStringByIndex>(&library, b"AT_GetEnumStringByIndex")?;
                let command = symbol::<AtCommand>(&library, b"AT_Command")?;
                let queue_buffer = symbol::<AtQueueBuffer>(&library, b"AT_QueueBuffer")?;
                let wait_buffer = symbol::<AtWaitBuffer>(&library, b"AT_WaitBuffer")?;
                let flush = symbol::<AtFlush>(&library, b"AT_Flush")?;
                Ok(Self {
                    _library: library,
                    initialise_library,
                    finalise_library,
                    open,
                    close,
                    get_int,
                    get_float,
                    set_int,
                    set_float,
                    get_bool,
                    set_bool,
                    get_enum_index,
                    set_enum_string,
                    get_enum_string_by_index,
                    command,
                    queue_buffer,
                    wait_buffer,
                    flush,
                })
            }
        }

        fn check(&self, code: c_int, operation: &str) -> Result<()> {
            if code == AT_SUCCESS {
                Ok(())
            } else {
                Err(Error::new(
                    ErrorCode::Transport,
                    format!("Andor SDK3 {operation} failed with AT error {code}"),
                ))
            }
        }

        fn get_int(&self, handle: c_int, feature: &str) -> Result<i64> {
            let mut value = 0_i64;
            let feature = wide(feature);
            self.check(
                unsafe { (self.get_int)(handle, feature.as_ptr(), &mut value) },
                &format!(
                    "AT_GetInt({feature_name})",
                    feature_name = feature_name(feature.as_slice())
                ),
            )?;
            Ok(value)
        }

        fn set_int(&self, handle: c_int, feature: &str, value: i64) -> Result<()> {
            let feature_w = wide(feature);
            self.check(
                unsafe { (self.set_int)(handle, feature_w.as_ptr(), value) },
                &format!("AT_SetInt({feature})"),
            )
        }

        fn get_float(&self, handle: c_int, feature: &str) -> Result<f64> {
            let mut value = 0.0_f64;
            let feature_w = wide(feature);
            self.check(
                unsafe { (self.get_float)(handle, feature_w.as_ptr(), &mut value) },
                &format!("AT_GetFloat({feature})"),
            )?;
            Ok(value)
        }

        fn set_float(&self, handle: c_int, feature: &str, value: f64) -> Result<()> {
            let feature_w = wide(feature);
            self.check(
                unsafe { (self.set_float)(handle, feature_w.as_ptr(), value) },
                &format!("AT_SetFloat({feature})"),
            )
        }

        fn get_bool(&self, handle: c_int, feature: &str) -> Result<bool> {
            let mut value = 0;
            let feature_w = wide(feature);
            self.check(
                unsafe { (self.get_bool)(handle, feature_w.as_ptr(), &mut value) },
                &format!("AT_GetBool({feature})"),
            )?;
            Ok(value != 0)
        }

        fn set_bool(&self, handle: c_int, feature: &str, value: bool) -> Result<()> {
            let feature_w = wide(feature);
            self.check(
                unsafe { (self.set_bool)(handle, feature_w.as_ptr(), i32::from(value)) },
                &format!("AT_SetBool({feature})"),
            )
        }

        fn get_enum_string(&self, handle: c_int, feature: &str) -> Result<String> {
            let mut index = 0;
            let feature_w = wide(feature);
            self.check(
                unsafe { (self.get_enum_index)(handle, feature_w.as_ptr(), &mut index) },
                &format!("AT_GetEnumIndex({feature})"),
            )?;
            let mut buffer = vec![0 as AtWc; 128];
            self.check(
                unsafe {
                    (self.get_enum_string_by_index)(
                        handle,
                        feature_w.as_ptr(),
                        index,
                        buffer.as_mut_ptr(),
                        buffer.len() as c_int,
                    )
                },
                &format!("AT_GetEnumStringByIndex({feature})"),
            )?;
            Ok(feature_name(&buffer))
        }

        fn set_enum_string(&self, handle: c_int, feature: &str, value: &str) -> Result<()> {
            let feature_w = wide(feature);
            let value_w = wide(value);
            self.check(
                unsafe { (self.set_enum_string)(handle, feature_w.as_ptr(), value_w.as_ptr()) },
                &format!("AT_SetEnumString({feature})"),
            )
        }

        fn command(&self, handle: c_int, feature: &str) -> Result<()> {
            let feature_w = wide(feature);
            self.check(
                unsafe { (self.command)(handle, feature_w.as_ptr()) },
                &format!("AT_Command({feature})"),
            )
        }
    }

    fn with_camera<T>(
        path: &str,
        camera_index: i32,
        action: impl FnOnce(&AtCore, c_int) -> Result<T>,
    ) -> Result<T> {
        let core = AtCore::load(path)?;
        core.check(
            unsafe { (core.initialise_library)() },
            "AT_InitialiseLibrary",
        )?;
        let _library_guard = LibraryGuard {
            core: &core,
            active: true,
        };
        let mut handle: c_int = 0;
        core.check(unsafe { (core.open)(camera_index, &mut handle) }, "AT_Open")?;
        let camera_guard = CameraGuard {
            core: &core,
            handle,
        };
        action(&core, camera_guard.handle)
    }

    pub fn get_int(path: &str, camera_index: i32, feature: &str) -> Result<i64> {
        with_camera(path, camera_index, |core, handle| {
            core.get_int(handle, feature)
        })
    }

    pub fn get_float(path: &str, camera_index: i32, feature: &str) -> Result<f64> {
        with_camera(path, camera_index, |core, handle| {
            core.get_float(handle, feature)
        })
    }

    pub fn get_bool(path: &str, camera_index: i32, feature: &str) -> Result<bool> {
        with_camera(path, camera_index, |core, handle| {
            core.get_bool(handle, feature)
        })
    }

    pub fn get_enum_string(path: &str, camera_index: i32, feature: &str) -> Result<String> {
        with_camera(path, camera_index, |core, handle| {
            core.get_enum_string(handle, feature)
        })
    }

    pub fn set_int(path: &str, camera_index: i32, feature: &str, value: i64) -> Result<()> {
        with_camera(path, camera_index, |core, handle| {
            core.set_int(handle, feature, value)
        })
    }

    pub fn set_float(path: &str, camera_index: i32, feature: &str, value: f64) -> Result<()> {
        with_camera(path, camera_index, |core, handle| {
            core.set_float(handle, feature, value)
        })
    }

    pub fn set_bool(path: &str, camera_index: i32, feature: &str, value: bool) -> Result<()> {
        with_camera(path, camera_index, |core, handle| {
            core.set_bool(handle, feature, value)
        })
    }

    pub fn set_enum_string(
        path: &str,
        camera_index: i32,
        feature: &str,
        value: &str,
    ) -> Result<()> {
        with_camera(path, camera_index, |core, handle| {
            core.set_enum_string(handle, feature, value)
        })
    }

    struct LibraryGuard<'a> {
        core: &'a AtCore,
        active: bool,
    }

    impl Drop for LibraryGuard<'_> {
        fn drop(&mut self) {
            if self.active {
                unsafe {
                    (self.core.finalise_library)();
                }
            }
        }
    }

    struct CameraGuard<'a> {
        core: &'a AtCore,
        handle: c_int,
    }

    impl Drop for CameraGuard<'_> {
        fn drop(&mut self) {
            unsafe {
                (self.core.flush)(self.handle);
                (self.core.close)(self.handle);
            }
        }
    }

    pub fn capture(
        path: &str,
        camera_index: i32,
        requested_width: u32,
        requested_height: u32,
    ) -> Result<Sdk3Frame> {
        let core = AtCore::load(path)?;
        core.check(
            unsafe { (core.initialise_library)() },
            "AT_InitialiseLibrary",
        )?;
        let _library_guard = LibraryGuard {
            core: &core,
            active: true,
        };

        let mut device_count = 0_i64;
        let device_count_feature = wide("DeviceCount");
        core.check(
            unsafe {
                (core.get_int)(
                    AT_HANDLE_SYSTEM,
                    device_count_feature.as_ptr(),
                    &mut device_count,
                )
            },
            "AT_GetInt(DeviceCount)",
        )?;
        if camera_index < 0 || i64::from(camera_index) >= device_count {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!(
                    "Andor SDK3 camera_index {camera_index} is outside runtime device count {device_count}"
                ),
            ));
        }

        let mut handle: c_int = 0;
        core.check(unsafe { (core.open)(camera_index, &mut handle) }, "AT_Open")?;
        let camera_guard = CameraGuard {
            core: &core,
            handle,
        };

        core.set_enum_string(camera_guard.handle, "PixelEncoding", "Mono16")?;
        core.set_enum_string(camera_guard.handle, "CycleMode", "Fixed")?;
        core.set_enum_string(camera_guard.handle, "TriggerMode", "Internal")?;
        core.set_int(camera_guard.handle, "FrameCount", 1)?;
        core.set_int(camera_guard.handle, "AOIWidth", i64::from(requested_width))?;
        core.set_int(
            camera_guard.handle,
            "AOIHeight",
            i64::from(requested_height),
        )?;
        core.set_float(camera_guard.handle, "ExposureTime", 0.01)?;

        let width = checked_u32(core.get_int(camera_guard.handle, "AOIWidth")?, "AOIWidth")?;
        let height = checked_u32(core.get_int(camera_guard.handle, "AOIHeight")?, "AOIHeight")?;
        let stride = checked_usize(core.get_int(camera_guard.handle, "AOIStride")?, "AOIStride")?;
        let image_size = checked_usize(
            core.get_int(camera_guard.handle, "ImageSizeBytes")?,
            "ImageSizeBytes",
        )?;
        if image_size == 0 {
            return Err(Error::new(
                ErrorCode::Transport,
                "Andor SDK3 ImageSizeBytes is zero",
            ));
        }

        let words = image_size.div_ceil(std::mem::size_of::<u64>());
        let mut backing = vec![0_u64; words];
        let buffer = backing.as_mut_ptr().cast::<u8>();
        core.check(
            unsafe { (core.queue_buffer)(camera_guard.handle, buffer, image_size as c_int) },
            "AT_QueueBuffer",
        )?;
        core.command(camera_guard.handle, "AcquisitionStart")?;
        let mut returned_ptr = ptr::null_mut::<u8>();
        let mut returned_size: c_int = 0;
        let wait = unsafe {
            (core.wait_buffer)(
                camera_guard.handle,
                &mut returned_ptr,
                &mut returned_size,
                5000,
            )
        };
        let stop_result = core.command(camera_guard.handle, "AcquisitionStop");
        unsafe {
            (core.flush)(camera_guard.handle);
        }
        core.check(wait, "AT_WaitBuffer")?;
        stop_result?;
        if returned_ptr.is_null() || returned_size <= 0 {
            return Err(Error::new(
                ErrorCode::Transport,
                "Andor SDK3 returned an empty frame buffer",
            ));
        }
        let returned = unsafe { std::slice::from_raw_parts(returned_ptr, returned_size as usize) };
        let row_bytes = width as usize * 2;
        let mut data = Vec::with_capacity(row_bytes * height as usize);
        for row in 0..height as usize {
            let start = row.checked_mul(stride).ok_or_else(|| {
                Error::new(ErrorCode::Transport, "Andor SDK3 frame stride overflow")
            })?;
            let end = start
                .checked_add(row_bytes)
                .ok_or_else(|| Error::new(ErrorCode::Transport, "Andor SDK3 frame row overflow"))?;
            if end > returned.len() {
                return Err(Error::new(
                    ErrorCode::Transport,
                    "Andor SDK3 returned frame shorter than AOI stride requires",
                ));
            }
            data.extend_from_slice(&returned[start..end]);
        }
        Ok(Sdk3Frame {
            width,
            height,
            data,
        })
    }

    unsafe fn symbol<T: Copy>(library: &Library, name: &[u8]) -> Result<T> {
        let symbol: Symbol<'_, T> = library.get(name).map_err(|error| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!(
                    "Andor SDK3 runtime missing symbol {}: {error}",
                    String::from_utf8_lossy(name)
                ),
            )
        })?;
        Ok(*symbol)
    }

    #[cfg(windows)]
    fn wide(value: &str) -> Vec<AtWc> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    #[cfg(not(windows))]
    fn wide(value: &str) -> Vec<AtWc> {
        value
            .chars()
            .map(|ch| ch as AtWc)
            .chain(std::iter::once(0))
            .collect()
    }

    fn feature_name(feature: &[AtWc]) -> String {
        feature
            .iter()
            .copied()
            .take_while(|value| *value != 0)
            .filter_map(|value| char::from_u32(value as u32))
            .collect()
    }

    fn checked_u32(value: i64, name: &str) -> Result<u32> {
        u32::try_from(value).map_err(|_| {
            Error::new(
                ErrorCode::Transport,
                format!("Andor SDK3 {name} value is outside unsigned 32-bit range"),
            )
        })
    }

    fn checked_usize(value: i64, name: &str) -> Result<usize> {
        usize::try_from(value).map_err(|_| {
            Error::new(
                ErrorCode::Transport,
                format!("Andor SDK3 {name} value is outside usize range"),
            )
        })
    }
}
