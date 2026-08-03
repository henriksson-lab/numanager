use libloading::Library;
use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};
use std::ffi::CStr;
use std::io::{BufReader, Read};
use std::os::raw::c_char;
use std::path::Path;

pub const PHOTOMETRICS_USB_VID: u16 = 0x1f12;
/// USB vendor ids this driver claims. Hosts that need raw USB access
/// (udev rules on Linux) must cover these; see
/// `usb_discovery::builtin_usb_vendor_claims`.
pub fn usb_vendor_ids() -> Vec<u16> {
    vec![PHOTOMETRICS_USB_VID]
}

pub const PVCAM_USB_CONTROL_OUT_REQUEST_TYPE: u8 = 0x40;
pub const PVCAM_USB_CONTROL_OUT_REQUEST: u8 = 0xd4;
pub const PVCAM_USB_CONTROL_IN_REQUEST_TYPE: u8 = 0xc0;
pub const PVCAM_USB_CONTROL_IN_REQUEST: u8 = 0xd5;
pub const PVCAM_HOST_COMMAND_CLASS: u8 = 0x3f;
pub const PVCAM_HOST_FRAME_BEGIN: u8 = 0x26;
pub const PVCAM_HOST_FRAME_END: u8 = 0x28;

#[derive(Debug, Clone)]
pub struct PvcamConfiguredProbe {
    label: String,
    camera_name: String,
    product: String,
    serial_number: Option<String>,
    chip_name: String,
    firmware_version: String,
    interface_type: String,
    sensor_width: u32,
    sensor_height: u32,
    bit_depth: u16,
    pixel_format: String,
    exposure: TimeInterval,
    sensor_temperature: Option<Temperature>,
    temperature_setpoint: Option<Temperature>,
    vendor_runtime_path: Option<String>,
    vendor_runtime_sha256: Option<String>,
    load_vendor_runtime: bool,
    usb_identity: Option<PvcamUsbIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PvcamUsbIdentity {
    vendor_id: u16,
    product_id: u16,
    product: Option<String>,
    serial_number: Option<String>,
    bus_number: u8,
    device_address: u8,
}

pub struct PvcamDiscovery {
    next_id: DriverId,
    probes: Vec<PvcamConfiguredProbe>,
    #[cfg(feature = "os-usb")]
    active_usb: bool,
}

impl PvcamDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![PvcamConfiguredProbe::fixture()],
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
                    "photometrics_pvcam" | "photometrics-pvcam" | "pvcam" | "photometrics"
                )
            })
            .map(PvcamConfiguredProbe::from_device_config)
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

impl DriverDiscovery for PvcamDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        #[cfg(not(feature = "os-usb"))]
        let probes = std::mem::take(&mut self.probes);
        #[cfg(feature = "os-usb")]
        let mut probes = std::mem::take(&mut self.probes);
        #[cfg(feature = "os-usb")]
        if self.active_usb {
            probes.extend(active_usb_probes()?);
        }
        probes
            .iter()
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                Ok(DriverCandidate::from_driver(
                    probe.discovery_label(),
                    Box::new(PvcamDriver::configured(id, probe.clone())),
                ))
            })
            .collect()
    }
}

impl PvcamConfiguredProbe {
    pub fn fixture() -> Self {
        Self {
            label: "Configured Photometrics PVCAM camera".into(),
            camera_name: "PVCAM-CONFIG-0".into(),
            product: "Photometrics PVCAM camera".into(),
            serial_number: Some("PVCAM-CONFIG-0001".into()),
            chip_name: "configured sensor".into(),
            firmware_version: "configured".into(),
            interface_type: "USB".into(),
            sensor_width: 2048,
            sensor_height: 2048,
            bit_depth: 16,
            pixel_format: "Mono16".into(),
            exposure: TimeInterval::from_milliseconds(10.0),
            sensor_temperature: Some(Temperature::from_celsius(-20.0)),
            temperature_setpoint: Some(Temperature::from_celsius(-20.0)),
            vendor_runtime_path: None,
            vendor_runtime_sha256: None,
            load_vendor_runtime: false,
            usb_identity: None,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::fixture();
        if !device.label.is_empty() {
            configured.label = device.label.clone();
        }
        configured.camera_name =
            string_prop(device, "camera_name").unwrap_or(configured.camera_name);
        configured.product = string_prop(device, "product").unwrap_or(configured.product);
        configured.serial_number =
            optional_string_prop(device, "serial_number", configured.serial_number);
        configured.chip_name = string_prop(device, "chip_name").unwrap_or(configured.chip_name);
        configured.firmware_version =
            string_prop(device, "firmware_version").unwrap_or(configured.firmware_version);
        configured.interface_type =
            string_prop(device, "interface_type").unwrap_or(configured.interface_type);
        configured.sensor_width =
            pixel_count_prop(device, "sensor_width")?.unwrap_or(configured.sensor_width);
        configured.sensor_height =
            pixel_count_prop(device, "sensor_height")?.unwrap_or(configured.sensor_height);
        configured.bit_depth = u16_prop(device, "bit_depth")?.unwrap_or(configured.bit_depth);
        configured.pixel_format =
            pixel_format_prop(device, "pixel_format")?.unwrap_or(configured.pixel_format);
        configured.exposure =
            time_interval_prop(device, "exposure")?.unwrap_or(configured.exposure);
        configured.sensor_temperature =
            optional_temperature_prop(device, "sensor_temperature", configured.sensor_temperature);
        configured.temperature_setpoint = optional_temperature_prop(
            device,
            "temperature_setpoint",
            configured.temperature_setpoint,
        );
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
        configured.load_vendor_runtime =
            bool_prop(device, "load_vendor_runtime").unwrap_or(configured.load_vendor_runtime);
        Ok(configured)
    }

    fn discovery_label(&self) -> String {
        format!("{} ({})", self.label, self.camera_name)
    }
}

#[cfg(feature = "os-usb")]
fn active_usb_probes() -> Result<Vec<PvcamConfiguredProbe>> {
    let devices = nusb::list_devices().map_err(|error| {
        Error::new(
            ErrorCode::Transport,
            format!("PVCAM USB device listing failed: {error}"),
        )
    })?;
    Ok(devices
        .filter(|device| device.vendor_id() == PHOTOMETRICS_USB_VID)
        .map(|device| {
            let product_id = device.product_id();
            let product = device
                .product_string()
                .map(str::to_string)
                .unwrap_or_else(|| format!("PVCAM USB camera {:04x}", product_id));
            let serial_number = device.serial_number().map(str::to_string);
            let label = format!(
                "Photometrics PVCAM USB {:04x}:{:04x} bus {} addr {}",
                PHOTOMETRICS_USB_VID,
                product_id,
                device.bus_number(),
                device.device_address()
            );
            let mut probe = PvcamConfiguredProbe::fixture();
            probe.label = label;
            probe.camera_name = serial_number
                .clone()
                .unwrap_or_else(|| format!("PVCAM-USB-{:04X}", product_id));
            probe.product = product.clone();
            probe.serial_number = serial_number.clone();
            probe.interface_type = "USB".into();
            probe.usb_identity = Some(PvcamUsbIdentity {
                vendor_id: PHOTOMETRICS_USB_VID,
                product_id,
                product: Some(product),
                serial_number,
                bus_number: device.bus_number(),
                device_address: device.device_address(),
            });
            probe
        })
        .collect())
}

pub struct PvcamDriver {
    id: DriverId,
    hub: DeviceId,
    camera: DeviceId,
    cooler: DeviceId,
    library: ResourceId,
    native_transport: ResourceId,
    configured: PvcamConfiguredProbe,
    next_token: u64,
    events: VecDeque<DriverEvent>,
}

impl PvcamDriver {
    pub fn configured(id: DriverId, configured: PvcamConfiguredProbe) -> Self {
        Self {
            id,
            hub: DeviceId(NodeId(id.0 * 1000 + 950)),
            camera: DeviceId(NodeId(id.0 * 1000 + 951)),
            cooler: DeviceId(NodeId(id.0 * 1000 + 952)),
            library: ResourceId(NodeId(id.0 * 1000 + 953)),
            native_transport: ResourceId(NodeId(id.0 * 1000 + 954)),
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

    fn runtime_path_for_behavior(&self, behavior: &str) -> Result<&str> {
        if !self.configured.load_vendor_runtime {
            return Err(Error::new(
                ErrorCode::Unsupported,
                format!("PVCAM {behavior} requires load_vendor_runtime=true"),
            ));
        }
        let digest_state = self.vendor_runtime_digest_allows_use();
        if digest_state != "verified" {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("PVCAM vendor runtime is not verified: {digest_state}"),
            ));
        }
        self.configured
            .vendor_runtime_path
            .as_deref()
            .ok_or_else(|| Error::new(ErrorCode::InvalidProperty, "PVCAM runtime path is required"))
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
            vendor: Some("Teledyne Photometrics".into()),
            model: Some(self.configured.product.clone()),
            serial: self.configured.serial_number.clone(),
            kinds: vec!["hub".into(), "camera.controller".into(), "pvcam".into()],
            properties: vec![
                string_property("camera_name", "PVCAM camera name"),
                string_property("product", "Product"),
                string_property("serial_number", "Serial number"),
                string_property("firmware_version", "Firmware version"),
                string_property("interface_type", "Interface type"),
                property("usb_vendor_id", "USB vendor ID", ValueType::I64),
                property("usb_product_id", "USB product ID", ValueType::I64),
                property("usb_identity", "USB identity", ValueType::Map),
                property("host_command_class", "Host command class", ValueType::I64),
                property("host_frame_begin", "Host frame begin", ValueType::I64),
                property("host_frame_end", "Host frame end", ValueType::I64),
                property(
                    "usb_control_out_request",
                    "USB control OUT request",
                    ValueType::I64,
                ),
                property(
                    "usb_control_out_request_type",
                    "USB control OUT request type",
                    ValueType::I64,
                ),
                property(
                    "usb_control_in_request",
                    "USB control IN request",
                    ValueType::I64,
                ),
                property(
                    "usb_control_in_request_type",
                    "USB control IN request type",
                    ValueType::I64,
                ),
                string_property("vendor_runtime_path", "Vendor runtime path"),
                string_property("vendor_runtime_sha256", "Vendor runtime SHA-256"),
                property(
                    "load_vendor_runtime",
                    "Load vendor runtime",
                    ValueType::Bool,
                ),
                string_property("vendor_runtime_state", "Vendor runtime state"),
                string_property("vendor_runtime_file_status", "Vendor runtime file status"),
                property(
                    "vendor_runtime_file_size",
                    "Vendor runtime file size",
                    ValueType::ByteCount,
                ),
                string_property("vendor_runtime_digest_state", "Vendor runtime digest state"),
                string_property("vendor_runtime_probe_state", "Vendor runtime probe state"),
                string_property("vendor_runtime_abi_state", "Vendor runtime ABI state"),
                string_property(
                    "vendor_runtime_discovery_state",
                    "Vendor runtime discovery state",
                ),
                property(
                    "vendor_runtime_camera_count",
                    "Vendor runtime camera count",
                    ValueType::I64,
                ),
                property(
                    "vendor_runtime_camera_names",
                    "Vendor runtime camera names",
                    ValueType::List,
                ),
                string_property("package_strategy", "Package strategy"),
                string_property("package_gate", "Package gate"),
                string_property("third_party_notice", "Third-party notice"),
                string_property("support_level", "Support level"),
            ],
            metadata: self.shared_metadata(),
        }
    }

    fn camera_descriptor(&self) -> DeviceDescriptor {
        DeviceDescriptor {
            id: self.camera,
            driver: self.id,
            label: self.configured.label.clone(),
            vendor: Some("Teledyne Photometrics".into()),
            model: Some(self.configured.product.clone()),
            serial: self.configured.serial_number.clone(),
            kinds: vec![
                "camera".into(),
                "camera.scientific".into(),
                "detector.mono".into(),
                "pvcam".into(),
            ],
            properties: vec![
                string_property("product", "Product"),
                string_property("chip_name", "Chip name"),
                property("sensor_width", "Sensor width", ValueType::PixelCount),
                property("sensor_height", "Sensor height", ValueType::PixelCount),
                property("bit_depth", "Bit depth", ValueType::I64),
                string_property("pixel_format", "Pixel format"),
                writable_property("exposure", "Exposure", ValueType::TimeInterval),
                string_property("capture_gate", "Capture gate"),
            ],
            metadata: self.shared_metadata(),
        }
    }

    fn cooler_descriptor(&self) -> DeviceDescriptor {
        let properties = vec![
            property(
                "sensor_temperature",
                "Sensor temperature",
                ValueType::Temperature,
            ),
            writable_property(
                "temperature_setpoint",
                "Temperature setpoint",
                ValueType::Temperature,
            ),
            string_property("cooler_gate", "Cooler gate"),
        ];

        DeviceDescriptor {
            id: self.cooler,
            driver: self.id,
            label: format!("{} cooler", self.configured.label),
            vendor: Some("Teledyne Photometrics".into()),
            model: Some(self.configured.product.clone()),
            serial: self.configured.serial_number.clone(),
            kinds: vec![
                "temperature.controller".into(),
                "cooler".into(),
                "state.device".into(),
            ],
            properties,
            metadata: self.shared_metadata(),
        }
    }

    fn shared_metadata(&self) -> BTreeMap<String, Value> {
        BTreeMap::from([
            ("sdk_free".into(), Value::Bool(false)),
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
                "vendor_runtime_digest_state".into(),
                Value::String(self.vendor_runtime_digest_state()),
            ),
            (
                "vendor_runtime_abi_state".into(),
                Value::String(self.vendor_runtime_abi_state()),
            ),
            (
                "support_level".into(),
                Value::String(
                    "configured and active USB PVCAM evidence plus verified vendor-runtime camera-name discovery, writable exposure setting, one-shot capture, repeated one-shot stream support, and runtime temperature read/setpoint control".into(),
                ),
            ),
            (
                "capture_gate".into(),
                Value::String(
                    "CameraCapture and repeated one-shot CameraStream use the verified vendor runtime; native continuous streaming and native transport require documented ABI/native-transport evidence".into(),
                ),
            ),
        ])
    }

    fn vendor_runtime_configured(&self) -> bool {
        self.configured.vendor_runtime_path.is_some()
    }

    fn usb_product_id(&self) -> Value {
        self.configured
            .usb_identity
            .as_ref()
            .map(|identity| Value::I64(identity.product_id as i64))
            .unwrap_or(Value::Null)
    }

    fn usb_identity_map(&self) -> Value {
        self.configured
            .usb_identity
            .as_ref()
            .map(|identity| {
                Value::Map(BTreeMap::from([
                    ("vendor_id".into(), Value::I64(identity.vendor_id as i64)),
                    ("product_id".into(), Value::I64(identity.product_id as i64)),
                    (
                        "product".into(),
                        identity
                            .product
                            .as_ref()
                            .map(|value| Value::String(value.clone()))
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "serial_number".into(),
                        identity
                            .serial_number
                            .as_ref()
                            .map(|value| Value::String(value.clone()))
                            .unwrap_or(Value::Null),
                    ),
                    ("bus_number".into(), Value::I64(identity.bus_number as i64)),
                    (
                        "device_address".into(),
                        Value::I64(identity.device_address as i64),
                    ),
                ]))
            })
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

    fn vendor_runtime_file_status(&self) -> String {
        let Some(path) = self.configured.vendor_runtime_path.as_deref() else {
            return "not_configured".into();
        };
        match std::fs::metadata(Path::new(path)) {
            Ok(metadata) if metadata.is_file() => "present".into(),
            Ok(_) => "not_a_file".into(),
            Err(error) => format!("unavailable:{}", error.kind()),
        }
    }

    fn vendor_runtime_file_size(&self) -> Result<Value> {
        let Some(path) = self.configured.vendor_runtime_path.as_deref() else {
            return Ok(Value::ByteCount(ByteCount::new(0)));
        };
        let metadata = std::fs::metadata(Path::new(path)).map_err(|error| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("PVCAM vendor runtime file is unavailable: {error}"),
            )
        })?;
        if !metadata.is_file() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "PVCAM vendor runtime path is not a regular file",
            ));
        }
        Ok(Value::ByteCount(ByteCount::new(metadata.len())))
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

    fn vendor_runtime_sha256(path: &str) -> Result<String> {
        let file = std::fs::File::open(Path::new(path)).map_err(|error| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!(
                    "PVCAM vendor runtime file is unavailable for digest verification: {error}"
                ),
            )
        })?;
        let mut reader = BufReader::new(file);
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let bytes = reader.read(&mut buffer).map_err(|error| {
                Error::new(
                    ErrorCode::Transport,
                    format!("PVCAM vendor runtime digest read failed: {error}"),
                )
            })?;
            if bytes == 0 {
                break;
            }
            hasher.update(&buffer[..bytes]);
        }
        Ok(hex::encode(hasher.finalize()))
    }

    fn vendor_runtime_digest_state(&self) -> String {
        let Some(configured_sha256) = self.configured.vendor_runtime_sha256.as_deref() else {
            return "not_configured".into();
        };
        let Some(expected) = Self::normalized_sha256(configured_sha256) else {
            return "invalid_configured_sha256".into();
        };
        let Some(path) = self.configured.vendor_runtime_path.as_deref() else {
            return "digest_without_path".into();
        };
        match Self::vendor_runtime_sha256(path) {
            Ok(actual) if actual == expected => "verified".into(),
            Ok(actual) => format!("mismatch:{actual}"),
            Err(error) => format!("unavailable:{}", compact_error(&error.message)),
        }
    }

    fn vendor_runtime_digest_allows_use(&self) -> String {
        let Some(configured_sha256) = self.configured.vendor_runtime_sha256.as_deref() else {
            return "missing_sha256".into();
        };
        let Some(expected) = Self::normalized_sha256(configured_sha256) else {
            return "invalid_configured_sha256".into();
        };
        let Some(path) = self.configured.vendor_runtime_path.as_deref() else {
            return "missing_path".into();
        };
        match Self::vendor_runtime_sha256(path) {
            Ok(actual) if actual == expected => "verified".into(),
            Ok(_) => "digest_mismatch".into(),
            Err(error) => format!("digest_unavailable:{}", compact_error(&error.message)),
        }
    }

    fn vendor_runtime_probe_state(&self) -> String {
        if !self.configured.load_vendor_runtime {
            return "disabled".into();
        }
        let digest_state = self.vendor_runtime_digest_allows_use();
        if digest_state != "verified" {
            return digest_state;
        }
        let Some(path) = self.configured.vendor_runtime_path.as_deref() else {
            return "missing_path".into();
        };
        if let Err(error) = std::fs::metadata(Path::new(path)) {
            return format!("file_unavailable:{}", error.kind());
        }

        // Loading is the explicit backend boundary. No PVCAM init/open/capture
        // calls are made here, so this proves only runtime loadability.
        match unsafe { Library::new(path) } {
            Ok(_library) => "loaded".into(),
            Err(error) => format!("load_error:{}", compact_error(&error.to_string())),
        }
    }

    fn vendor_runtime_expected_symbols(&self) -> &'static [&'static str] {
        &[
            "pl_pvcam_init",
            "pl_pvcam_uninit",
            "pl_cam_get_total",
            "pl_cam_get_name",
            "pl_cam_open",
            "pl_cam_close",
            "pl_get_param",
            "pl_set_param",
            "pl_exp_setup_seq",
            "pl_exp_start_seq",
            "pl_exp_check_status",
            "pl_exp_finish_seq",
            "pl_exp_abort",
        ]
    }

    fn vendor_runtime_abi_state(&self) -> String {
        if !self.configured.load_vendor_runtime {
            return "disabled".into();
        }
        let digest_state = self.vendor_runtime_digest_allows_use();
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
        let missing = self
            .vendor_runtime_expected_symbols()
            .iter()
            .copied()
            .filter(|symbol| unsafe { library.get::<*const ()>(symbol.as_bytes()) }.is_err())
            .collect::<Vec<_>>();
        if missing.is_empty() {
            format!(
                "symbols_present:{}",
                self.vendor_runtime_expected_symbols().join(",")
            )
        } else {
            format!("missing_symbols:{}", missing.join(","))
        }
    }

    fn vendor_runtime_camera_discovery(&self) -> (String, Vec<String>) {
        if !self.configured.load_vendor_runtime {
            return ("disabled".into(), Vec::new());
        }
        let digest_state = self.vendor_runtime_digest_allows_use();
        if digest_state != "verified" {
            return (digest_state, Vec::new());
        }
        let Some(path) = self.configured.vendor_runtime_path.as_deref() else {
            return ("missing_path".into(), Vec::new());
        };
        let library = match unsafe { Library::new(path) } {
            Ok(library) => library,
            Err(error) => {
                return (
                    format!("load_error:{}", compact_error(&error.to_string())),
                    Vec::new(),
                )
            }
        };
        let init = match unsafe { library.get::<unsafe extern "C" fn() -> i16>(b"pl_pvcam_init") } {
            Ok(symbol) => symbol,
            Err(_) => return ("missing_symbols:pl_pvcam_init".into(), Vec::new()),
        };
        let uninit =
            match unsafe { library.get::<unsafe extern "C" fn() -> i16>(b"pl_pvcam_uninit") } {
                Ok(symbol) => symbol,
                Err(_) => return ("missing_symbols:pl_pvcam_uninit".into(), Vec::new()),
            };
        let get_total = match unsafe {
            library.get::<unsafe extern "C" fn(*mut i16) -> i16>(b"pl_cam_get_total")
        } {
            Ok(symbol) => symbol,
            Err(_) => return ("missing_symbols:pl_cam_get_total".into(), Vec::new()),
        };
        let get_name = match unsafe {
            library.get::<unsafe extern "C" fn(i16, *mut c_char) -> i16>(b"pl_cam_get_name")
        } {
            Ok(symbol) => symbol,
            Err(_) => return ("missing_symbols:pl_cam_get_name".into(), Vec::new()),
        };

        let initialized = unsafe { init() } != 0;
        if !initialized {
            return ("init_failed".into(), Vec::new());
        }

        let mut total = 0_i16;
        let total_ok = unsafe { get_total(&mut total as *mut i16) } != 0;
        if !total_ok || total < 0 {
            let _ = unsafe { uninit() };
            return ("camera_count_failed".into(), Vec::new());
        }

        let mut names = Vec::new();
        for index in 0..total.min(64) {
            let mut buffer = [0 as c_char; 256];
            let name_ok = unsafe { get_name(index, buffer.as_mut_ptr()) } != 0;
            if !name_ok {
                let _ = unsafe { uninit() };
                return (format!("camera_name_failed:{index}"), names);
            }
            let name = unsafe { CStr::from_ptr(buffer.as_ptr()) }
                .to_string_lossy()
                .trim()
                .to_string();
            names.push(name);
        }
        let _ = unsafe { uninit() };
        (format!("ready:{}", names.len()), names)
    }

    fn package_strategy(&self) -> &'static str {
        "use optional third-party vendor firmware/runtime package as an explicit backend when a project-owned replacement is not available"
    }

    fn package_gate(&self) -> &'static str {
        "runtime package identity, explicit loadability/symbol probes, camera-name discovery, and one-shot capture are available only after SHA-256 verification and load_vendor_runtime=true"
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "camera_name" => Ok(Value::String(self.configured.camera_name.clone())),
                "product" => Ok(Value::String(self.configured.product.clone())),
                "serial_number" => Ok(Value::String(
                    self.configured.serial_number.clone().unwrap_or_default(),
                )),
                "firmware_version" => Ok(Value::String(self.configured.firmware_version.clone())),
                "interface_type" => Ok(Value::String(self.configured.interface_type.clone())),
                "usb_vendor_id" => Ok(Value::I64(PHOTOMETRICS_USB_VID as i64)),
                "usb_product_id" => Ok(self.usb_product_id()),
                "usb_identity" => Ok(self.usb_identity_map()),
                "host_command_class" => Ok(Value::I64(PVCAM_HOST_COMMAND_CLASS as i64)),
                "host_frame_begin" => Ok(Value::I64(PVCAM_HOST_FRAME_BEGIN as i64)),
                "host_frame_end" => Ok(Value::I64(PVCAM_HOST_FRAME_END as i64)),
                "usb_control_out_request" => Ok(Value::I64(PVCAM_USB_CONTROL_OUT_REQUEST as i64)),
                "usb_control_out_request_type" => {
                    Ok(Value::I64(PVCAM_USB_CONTROL_OUT_REQUEST_TYPE as i64))
                }
                "usb_control_in_request" => Ok(Value::I64(PVCAM_USB_CONTROL_IN_REQUEST as i64)),
                "usb_control_in_request_type" => {
                    Ok(Value::I64(PVCAM_USB_CONTROL_IN_REQUEST_TYPE as i64))
                }
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
                "vendor_runtime_state" => Ok(Value::String(self.vendor_runtime_state().into())),
                "vendor_runtime_file_status" => {
                    Ok(Value::String(self.vendor_runtime_file_status()))
                }
                "vendor_runtime_file_size" => self.vendor_runtime_file_size(),
                "vendor_runtime_digest_state" => {
                    Ok(Value::String(self.vendor_runtime_digest_state()))
                }
                "vendor_runtime_probe_state" => {
                    Ok(Value::String(self.vendor_runtime_probe_state()))
                }
                "vendor_runtime_abi_state" => Ok(Value::String(self.vendor_runtime_abi_state())),
                "vendor_runtime_discovery_state" => Ok(Value::String(
                    self.vendor_runtime_camera_discovery().0,
                )),
                "vendor_runtime_camera_count" => {
                    let (_, names) = self.vendor_runtime_camera_discovery();
                    Ok(Value::I64(names.len() as i64))
                }
                "vendor_runtime_camera_names" => {
                    let (_, names) = self.vendor_runtime_camera_discovery();
                    Ok(Value::List(names.into_iter().map(Value::String).collect()))
                }
                "package_strategy" => Ok(Value::String(self.package_strategy().into())),
                "package_gate" => Ok(Value::String(self.package_gate().into())),
                "third_party_notice" => Ok(Value::String(
                    "configured PVCAM vendor firmware/runtime packages are third-party excluded data"
                        .into(),
                )),
                "support_level" => Ok(Value::String(
                    "configured and active USB PVCAM evidence plus verified vendor-runtime camera-name discovery, writable exposure setting, one-shot capture, repeated one-shot stream support, and runtime temperature read/setpoint control".into(),
                )),
                _ => invalid_property("unknown PVCAM hub property", key),
            };
        }
        if device == self.camera {
            return match key {
                "product" => Ok(Value::String(self.configured.product.clone())),
                "chip_name" => Ok(Value::String(self.configured.chip_name.clone())),
                "sensor_width" => Ok(Value::PixelCount(PixelCount::new(
                    self.configured.sensor_width,
                ))),
                "sensor_height" => Ok(Value::PixelCount(PixelCount::new(
                    self.configured.sensor_height,
                ))),
                "bit_depth" => Ok(Value::I64(self.configured.bit_depth as i64)),
                "pixel_format" => Ok(Value::String(self.configured.pixel_format.clone())),
                "exposure" => Ok(Value::TimeInterval(self.configured.exposure)),
                "capture_gate" => Ok(Value::String(
                    "CameraCapture and repeated one-shot CameraStream use the verified vendor runtime; native continuous streaming and native transport require documented ABI/native-transport evidence".into(),
                )),
                _ => invalid_property("unknown PVCAM camera property", key),
            };
        }
        if device == self.cooler {
            return match key {
                "sensor_temperature" => self.read_runtime_temperature().or_else(|_| {
                    self.configured
                        .sensor_temperature
                        .map(Value::Temperature)
                        .ok_or_else(|| {
                            Error::new(
                                ErrorCode::Unsupported,
                                "PVCAM sensor_temperature is unavailable for this configured device",
                            )
                        })
                }),
                "temperature_setpoint" => self.read_runtime_temperature_setpoint().or_else(|_| {
                    self.configured
                        .temperature_setpoint
                        .map(Value::Temperature)
                        .ok_or_else(|| {
                            Error::new(
                                ErrorCode::Unsupported,
                                "PVCAM temperature_setpoint is unavailable for this configured device",
                            )
                        })
                }),
                "cooler_gate" => Ok(Value::String(
                    "temperature readback and setpoint use the verified vendor runtime when enabled; configured values remain metadata".into(),
                )),
                _ => invalid_property("unknown PVCAM cooler property", key),
            };
        }
        Err(Error::new(
            ErrorCode::InvalidCommand,
            "unknown PVCAM device",
        ))
    }

    fn capture_frame(
        &mut self,
        token: DriverToken,
        request: CameraCaptureRequest,
    ) -> Result<Value> {
        let path = self.runtime_path_for_behavior("capture")?;
        let encoding = request.encoding.unwrap_or(ImageEncoding::Native);
        let expected_pixel_format = if self.configured.bit_depth <= 8 {
            match encoding {
                ImageEncoding::Native | ImageEncoding::Mono8 => "Mono8",
                ImageEncoding::Raw8 => "Raw8",
                _ => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "PVCAM 8-bit capture supports Native, Mono8, or Raw8",
                    ))
                }
            }
        } else {
            match encoding {
                ImageEncoding::Native | ImageEncoding::Mono16 => "Mono16",
                ImageEncoding::Raw16 => "Raw16",
                _ => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "PVCAM 16-bit capture supports Native, Mono16, or Raw16",
                    ))
                }
            }
        };
        #[cfg(feature = "os-usb")]
        let frame = live_pvcam::capture(
            path,
            &self.configured.camera_name,
            self.configured.sensor_width,
            self.configured.sensor_height,
            self.configured.exposure,
        )?;
        #[cfg(not(feature = "os-usb"))]
        {
            let _ = (path, token, expected_pixel_format);
            return Err(Error::new(
                ErrorCode::Unsupported,
                "PVCAM capture requires numanager-drivers/os-usb",
            ));
        }
        #[cfg(feature = "os-usb")]
        {
            let handle = FrameHandle {
                stream: StreamId(self.camera.0 .0),
                frame: FrameId(token.0),
            };
            self.configured.sensor_width = frame.width;
            self.configured.sensor_height = frame.height;
            self.configured.bit_depth = frame.bit_depth;
            self.configured.pixel_format = expected_pixel_format.into();
            self.events.push_back(DriverEvent::FrameReady(Frame {
                handle,
                device: self.camera,
                width: frame.width,
                height: frame.height,
                pixel_format: expected_pixel_format.into(),
                data: frame.data,
                metadata: BTreeMap::from([
                    (
                        "source".into(),
                        Value::String("pvcam-vendor-runtime".into()),
                    ),
                    (
                        "runtime_backend".into(),
                        Value::String(
                            "pl_exp_setup_seq/pl_exp_start_seq/pl_exp_check_status".into(),
                        ),
                    ),
                    ("wire_byte_order".into(), Value::String("native".into())),
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
                (
                    "pixel_format".into(),
                    Value::String(expected_pixel_format.into()),
                ),
                ("stream".into(), Value::I64(handle.stream.0 as i64)),
                ("frame".into(), Value::I64(handle.frame.0 as i64)),
                (
                    "source".into(),
                    Value::String("pvcam-vendor-runtime".into()),
                ),
            ])))
        }
    }

    fn stream_frames(&mut self, token: DriverToken, request: CameraStreamRequest) -> Result<Value> {
        let frame_count = request.frame_count.unwrap_or(8);
        if frame_count == 0 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "PVCAM CameraStream frame_count must be positive",
            ));
        }
        let stream = StreamId(token.0);
        let mut completed_width = None;
        let mut completed_height = None;
        let mut completed_pixel_format = None;
        for index in 0..frame_count {
            let capture = CameraCaptureRequest {
                encoding: request.encoding.clone(),
                buffer: Some(request.buffer.clone()),
            };
            let value = self.capture_frame(DriverToken(token.0 + index), capture)?;
            if let Value::Map(fields) = value {
                completed_width = fields.get("width").and_then(|value| match value {
                    Value::PixelCount(count) => Some(count.pixels()),
                    _ => None,
                });
                completed_height = fields.get("height").and_then(|value| match value {
                    Value::PixelCount(count) => Some(count.pixels()),
                    _ => None,
                });
                completed_pixel_format = fields.get("pixel_format").and_then(|value| match value {
                    Value::String(value) => Some(value.clone()),
                    _ => None,
                });
            }
            if let Some(DriverEvent::FrameReady(frame)) = self.events.back_mut() {
                frame.handle = FrameHandle {
                    stream,
                    frame: FrameId(index),
                };
                frame.metadata.insert(
                    "stream_mode".into(),
                    Value::String("repeated_one_shot".into()),
                );
            }
        }
        let mut values = BTreeMap::from([
            ("stream".into(), Value::I64(stream.0 as i64)),
            ("frame_count".into(), Value::I64(frame_count as i64)),
        ]);
        if let Some(width) = completed_width {
            values.insert("width".into(), Value::PixelCount(PixelCount::new(width)));
        }
        if let Some(height) = completed_height {
            values.insert("height".into(), Value::PixelCount(PixelCount::new(height)));
        }
        if let Some(pixel_format) = completed_pixel_format {
            values.insert("pixel_format".into(), Value::String(pixel_format));
        }
        Ok(Value::Map(values))
    }
}

impl Driver for PvcamDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        self.descriptors_inner()
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![
            ResourceDescriptor {
                id: self.library,
                driver: self.id,
                label: format!("{} PVCAM library", self.configured.label),
                kind: "vendor.library.pvcam".into(),
                metadata: BTreeMap::from([
                    ("sdk_free".into(), Value::Bool(false)),
                    (
                        "required_runtime".into(),
                        Value::String("libpvcam.so or platform PVCAM runtime".into()),
                    ),
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
                        "configured".into(),
                        Value::Bool(self.vendor_runtime_configured()),
                    ),
                    (
                        "package_state".into(),
                        Value::String(self.vendor_runtime_state().into()),
                    ),
                    (
                        "runtime_digest_state".into(),
                        Value::String(self.vendor_runtime_digest_state()),
                    ),
                    (
                        "runtime_abi_state".into(),
                        Value::String(self.vendor_runtime_abi_state()),
                    ),
                    (
                        "backend_enabled".into(),
                        Value::Bool(self.configured.load_vendor_runtime),
                    ),
                    (
                        "license_scope".into(),
                        Value::String("third-party excluded data".into()),
                    ),
                    (
                        "binding_gate".into(),
                        Value::String(self.package_gate().into()),
                    ),
                ]),
            },
            ResourceDescriptor {
                id: self.native_transport,
                driver: self.id,
                label: format!("{} native transport evidence", self.configured.label),
                kind: "reverse.usb-pcie".into(),
                metadata: BTreeMap::from([
                    (
                        "usb_vendor_id".into(),
                        Value::I64(PHOTOMETRICS_USB_VID as i64),
                    ),
                    ("usb_product_id".into(), self.usb_product_id()),
                    ("usb_identity".into(), self.usb_identity_map()),
                    ("usb_control_out".into(), Value::String("0x40/0xd4".into())),
                    ("usb_control_in".into(), Value::String("0xc0/0xd5".into())),
                    (
                        "host_frame".into(),
                        Value::String("0x3f len_le16 0x26 code ... 0x28".into()),
                    ),
                    (
                        "default_support".into(),
                        Value::String(
                            "native backend is not exposed because host-command framing, request fields, completion, and frame ownership evidence is absent; use a vendor runtime backend when available".into(),
                        ),
                    ),
                ]),
            },
        ]
    }

    fn capabilities(&self, _device: DeviceId) -> Vec<CapabilityDescriptor> {
        if _device == self.camera {
            return vec![
                capability(
                    1,
                    self.camera,
                    CapabilityKind::CameraCapture,
                    ValueType::Map,
                    ValueType::Map,
                ),
                capability(
                    3,
                    self.camera,
                    CapabilityKind::CameraStream,
                    ValueType::Map,
                    ValueType::Map,
                ),
            ];
        }
        if _device == self.cooler {
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
                Command::WriteProperty { device, key, value }
                    if [self.hub, self.camera, self.cooler].contains(device) =>
                {
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
                        request,
                        CapabilityRequest::CameraCapture(_) | CapabilityRequest::None
                    ) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "CameraCapture expects CameraCaptureRequest",
                        ));
                    }
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if *device == self.camera && *capability == CapabilityId(3) => {
                    if !matches!(request, CapabilityRequest::CameraStream(_)) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "CameraStream expects CameraStreamRequest",
                        ));
                    }
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if *device == self.cooler && *capability == CapabilityId(2) => {
                    if !matches!(
                        request,
                        CapabilityRequest::TemperatureControl(_) | CapabilityRequest::None
                    ) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "TemperatureControl expects TemperatureControlRequest",
                        ));
                    }
                }
                Command::Invoke { device, .. }
                    if [self.hub, self.camera, self.cooler].contains(device) =>
                {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "unsupported PVCAM capability",
                    ));
                }
                _ => {}
            }
        }
        Ok(PreparedBatch {
            id: batch.id,
            commands: batch.commands.clone(),
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.library),
                description: "PVCAM property/capture batch".into(),
                payload: Value::String(self.configured.camera_name.clone()),
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
                Command::WriteProperty { device, key, value } => {
                    result = self.write_property(device, &key, value)?;
                }
                Command::ApplyStateSet(set) => {
                    let mut values = BTreeMap::new();
                    for write in set.writes {
                        let applied =
                            self.write_property(write.device, &write.property, write.value)?;
                        values.insert(write.property, applied);
                    }
                    result = Value::Map(values);
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
                    result = self.capture_frame(token, capture)?;
                }
                Command::Invoke {
                    device,
                    capability,
                    request: CapabilityRequest::CameraStream(request),
                } if device == self.camera && capability == CapabilityId(3) => {
                    result = self.stream_frames(token, request)?;
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if device == self.cooler && capability == CapabilityId(2) => {
                    let request = match request {
                        CapabilityRequest::TemperatureControl(request) => request,
                        CapabilityRequest::None => TemperatureControlRequest {
                            target: None,
                            enabled: None,
                        },
                        _ => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "TemperatureControl expects TemperatureControlRequest",
                            ))
                        }
                    };
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

impl PvcamDriver {
    fn validate_write_property(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        match (device, key, value) {
            (device, "exposure", Value::TimeInterval(interval)) if device == self.camera => {
                validate_positive_exposure(*interval)
            }
            (device, "temperature_setpoint", Value::Temperature(temperature))
                if device == self.cooler =>
            {
                if temperature.celsius().is_finite() {
                    Ok(())
                } else {
                    Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "PVCAM temperature setpoint must be finite",
                    ))
                }
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "PVCAM exposes only the evidenced writable exposure and temperature_setpoint properties",
            )),
        }
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: Value) -> Result<Value> {
        self.validate_write_property(device, key, &value)?;
        match (device, key, value) {
            (device, "exposure", Value::TimeInterval(interval)) if device == self.camera => {
                self.configured.exposure = interval;
                Ok(Value::TimeInterval(interval))
            }
            (device, "temperature_setpoint", Value::Temperature(temperature))
                if device == self.cooler =>
            {
                if let Ok(path) = self.runtime_path_for_behavior("temperature control") {
                    #[cfg(feature = "os-usb")]
                    let applied = live_pvcam::set_temperature_setpoint(
                        path,
                        &self.configured.camera_name,
                        temperature,
                    )?;
                    #[cfg(not(feature = "os-usb"))]
                    {
                        let _ = path;
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "PVCAM temperature control requires numanager-drivers/os-usb",
                        ));
                    }
                    #[cfg(feature = "os-usb")]
                    {
                        self.configured.temperature_setpoint = Some(applied);
                        return Ok(Value::Temperature(applied));
                    }
                }
                self.configured.temperature_setpoint = Some(temperature);
                Ok(Value::Temperature(temperature))
            }
            _ => unreachable!("validated PVCAM writable property"),
        }
    }

    fn invoke_temperature_control(&mut self, request: TemperatureControlRequest) -> Result<Value> {
        let mut changed = BTreeMap::new();
        if let Some(enabled) = request.enabled {
            changed.insert(
                "enabled".into(),
                Value::String(
                    if enabled {
                        "setpoint_control"
                    } else {
                        "unsupported_disable"
                    }
                    .into(),
                ),
            );
            if !enabled {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    "PVCAM temperature control exposes setpoint writes, not cooler disable",
                ));
            }
        }
        if let Some(target) = request.target {
            let value = self.write_property(
                self.cooler,
                "temperature_setpoint",
                Value::Temperature(target),
            )?;
            changed.insert("temperature_setpoint".into(), value);
        }
        if changed.is_empty() {
            changed.insert(
                "temperature_setpoint".into(),
                self.read_property(self.cooler, "temperature_setpoint")?,
            );
            changed.insert(
                "sensor_temperature".into(),
                self.read_property(self.cooler, "sensor_temperature")?,
            );
        }
        Ok(Value::Map(changed))
    }

    fn read_runtime_temperature(&self) -> Result<Value> {
        let path = self.runtime_path_for_behavior("temperature readback")?;
        #[cfg(feature = "os-usb")]
        {
            live_pvcam::read_temperature(path, &self.configured.camera_name).map(Value::Temperature)
        }
        #[cfg(not(feature = "os-usb"))]
        {
            let _ = path;
            Err(Error::new(
                ErrorCode::Unsupported,
                "PVCAM temperature readback requires numanager-drivers/os-usb",
            ))
        }
    }

    fn read_runtime_temperature_setpoint(&self) -> Result<Value> {
        let path = self.runtime_path_for_behavior("temperature setpoint readback")?;
        #[cfg(feature = "os-usb")]
        {
            live_pvcam::read_temperature_setpoint(path, &self.configured.camera_name)
                .map(Value::Temperature)
        }
        #[cfg(not(feature = "os-usb"))]
        {
            let _ = path;
            Err(Error::new(
                ErrorCode::Unsupported,
                "PVCAM temperature setpoint readback requires numanager-drivers/os-usb",
            ))
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

fn writable_property(key: &str, display_name: &str, value_type: ValueType) -> PropertySchema {
    let mut schema = property(key, display_name, value_type);
    schema.writable = true;
    schema
}

fn string_property(key: &str, display_name: &str) -> PropertySchema {
    property(key, display_name, ValueType::String)
}

fn string_prop(device: &DeviceConfig, key: &str) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn u16_prop(device: &DeviceConfig, key: &str) -> Result<Option<u16>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if (0..=u16::MAX as i64).contains(value) => Ok(Some(*value as u16)),
        Some(Value::I64(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("PVCAM property {key} must fit in an unsigned 16-bit integer"),
        )),
        Some(Value::String(value)) => value.parse().map(Some).map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("PVCAM property {key} must be an unsigned 16-bit integer"),
            )
        }),
        _ => Ok(None),
    }
}

fn pixel_count_prop(device: &DeviceConfig, key: &str) -> Result<Option<u32>> {
    match device.properties.get(key) {
        Some(Value::PixelCount(value)) => Ok(Some(value.pixels())),
        Some(Value::I64(value)) if (0..=u32::MAX as i64).contains(value) => Ok(Some(*value as u32)),
        Some(Value::I64(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("PVCAM property {key} must fit in an unsigned 32-bit pixel count"),
        )),
        _ => Ok(None),
    }
}

fn optional_string_prop(
    device: &DeviceConfig,
    key: &str,
    default: Option<String>,
) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) if value == "none" => None,
        Some(Value::String(value)) => Some(value.clone()),
        Some(Value::Null) => None,
        _ => default,
    }
}

fn optional_temperature_prop(
    device: &DeviceConfig,
    key: &str,
    default: Option<Temperature>,
) -> Option<Temperature> {
    match device.properties.get(key) {
        Some(Value::Temperature(value)) => Some(*value),
        Some(Value::Null) => None,
        _ => default,
    }
}

fn time_interval_prop(device: &DeviceConfig, key: &str) -> Result<Option<TimeInterval>> {
    match device.properties.get(key) {
        Some(Value::TimeInterval(value)) => {
            validate_positive_exposure(*value)?;
            Ok(Some(*value))
        }
        Some(Value::I64(value)) if *value > 0 => {
            Ok(Some(TimeInterval::from_milliseconds(*value as f64)))
        }
        Some(Value::F64(value)) if value.is_finite() && *value > 0.0 => {
            Ok(Some(TimeInterval::from_seconds(*value)))
        }
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("PVCAM property {key} must be a positive TimeInterval"),
        )),
        None => Ok(None),
    }
}

fn validate_positive_exposure(exposure: TimeInterval) -> Result<()> {
    let seconds = exposure.seconds();
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "PVCAM exposure must be positive",
        ));
    }
    Ok(())
}

fn bool_prop(device: &DeviceConfig, key: &str) -> Option<bool> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
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

fn canonical_pixel_format(value: &str) -> Option<&'static str> {
    match value {
        "Mono16" | "mono16" | "MONO16" => Some("Mono16"),
        "Mono8" | "mono8" | "MONO8" => Some("Mono8"),
        "Bayer16" | "bayer16" | "BAYER16" => Some("Bayer16"),
        _ => None,
    }
}

fn pixel_format_prop(device: &DeviceConfig, key: &str) -> Result<Option<String>> {
    match device.properties.get(key) {
        Some(Value::String(value)) => canonical_pixel_format(value)
            .map(|value| Some(value.into()))
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("PVCAM property {key} must be Mono16, Mono8, or Bayer16"),
                )
            }),
        _ => Ok(None),
    }
}

fn invalid_property<T>(message: &str, key: &str) -> Result<T> {
    Err(Error::new(
        ErrorCode::InvalidProperty,
        format!("{message}: {key}"),
    ))
}

#[cfg(feature = "os-usb")]
mod live_pvcam {
    use super::*;
    use libloading::Library;
    use std::ffi::CString;
    use std::os::raw::c_void;
    use std::time::{Duration, Instant};

    const PVCAM_OK: u16 = 1;
    const OPEN_EXCLUSIVE: i16 = 0;
    const TIMED_MODE: i16 = 0;
    const READOUT_COMPLETE: i16 = 3;
    const READOUT_FAILED: i16 = 4;
    const CCS_HALT: i16 = 1;
    const MAX_CAM_NAME: usize = 256;
    const CLASS2: u32 = 2;
    const TYPE_INT16: u32 = 1;
    const PARAM_TEMP: u32 = (CLASS2 << 16) + (TYPE_INT16 << 24) + 525;
    const PARAM_TEMP_SETPOINT: u32 = (CLASS2 << 16) + (TYPE_INT16 << 24) + 526;
    const ATTR_CURRENT: i16 = 0;
    const ATTR_MIN: i16 = 3;
    const ATTR_MAX: i16 = 4;
    const ATTR_ACCESS: i16 = 7;
    const ATTR_AVAIL: i16 = 8;
    const ACC_READ_ONLY: u16 = 1;
    const ACC_READ_WRITE: u16 = 2;
    const ACC_WRITE_ONLY: u16 = 4;

    #[repr(C)]
    struct RgnType {
        s1: u16,
        s2: u16,
        sbin: u16,
        p1: u16,
        p2: u16,
        pbin: u16,
    }

    pub(super) struct PvcamFrame {
        pub width: u32,
        pub height: u32,
        pub bit_depth: u16,
        pub data: Vec<u8>,
    }

    struct Api {
        _library: Library,
        init: unsafe extern "C" fn() -> u16,
        uninit: unsafe extern "C" fn() -> u16,
        get_total: unsafe extern "C" fn(*mut i16) -> u16,
        get_name: unsafe extern "C" fn(i16, *mut c_char) -> u16,
        open: unsafe extern "C" fn(*mut c_char, *mut i16, i16) -> u16,
        close: unsafe extern "C" fn(i16) -> u16,
        setup_seq: unsafe extern "C" fn(i16, u16, u16, *mut RgnType, i16, u32, *mut u32) -> u16,
        start_seq: unsafe extern "C" fn(i16, *mut c_void) -> u16,
        check_status: unsafe extern "C" fn(i16, *mut i16, *mut u32) -> u16,
        finish_seq: unsafe extern "C" fn(i16, *mut c_void, i16) -> u16,
        abort: unsafe extern "C" fn(i16, i16) -> u16,
        get_param: unsafe extern "C" fn(i16, u32, i16, *mut c_void) -> u16,
        set_param: unsafe extern "C" fn(i16, u32, *mut c_void) -> u16,
    }

    impl Api {
        fn load(path: &str) -> Result<Self> {
            let library = unsafe { Library::new(path) }.map_err(|error| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("PVCAM runtime load failed: {error}"),
                )
            })?;
            Ok(Self {
                init: symbol(&library, "pl_pvcam_init")?,
                uninit: symbol(&library, "pl_pvcam_uninit")?,
                get_total: symbol(&library, "pl_cam_get_total")?,
                get_name: symbol(&library, "pl_cam_get_name")?,
                open: symbol(&library, "pl_cam_open")?,
                close: symbol(&library, "pl_cam_close")?,
                setup_seq: symbol(&library, "pl_exp_setup_seq")?,
                start_seq: symbol(&library, "pl_exp_start_seq")?,
                check_status: symbol(&library, "pl_exp_check_status")?,
                finish_seq: symbol(&library, "pl_exp_finish_seq")?,
                abort: symbol(&library, "pl_exp_abort")?,
                get_param: symbol(&library, "pl_get_param")?,
                set_param: symbol(&library, "pl_set_param")?,
                _library: library,
            })
        }

        fn check(ok: u16, operation: &str) -> Result<()> {
            if ok == PVCAM_OK {
                Ok(())
            } else {
                Err(Error::new(
                    ErrorCode::Transport,
                    format!("PVCAM {operation} failed"),
                ))
            }
        }

        fn get_u16_attr(&self, hcam: i16, param: u32, attr: i16, operation: &str) -> Result<u16> {
            let mut value = 0_u16;
            Self::check(
                unsafe { (self.get_param)(hcam, param, attr, (&mut value as *mut u16).cast()) },
                operation,
            )?;
            Ok(value)
        }

        fn get_i16_attr(&self, hcam: i16, param: u32, attr: i16, operation: &str) -> Result<i16> {
            let mut value = 0_i16;
            Self::check(
                unsafe { (self.get_param)(hcam, param, attr, (&mut value as *mut i16).cast()) },
                operation,
            )?;
            Ok(value)
        }

        fn param_available(&self, hcam: i16, param: u32, operation: &str) -> Result<()> {
            let mut available = 0_u8;
            Self::check(
                unsafe {
                    (self.get_param)(hcam, param, ATTR_AVAIL, (&mut available as *mut u8).cast())
                },
                operation,
            )?;
            if available != 0 {
                Ok(())
            } else {
                Err(Error::new(
                    ErrorCode::Unsupported,
                    format!("PVCAM parameter unavailable for {operation}"),
                ))
            }
        }

        fn ensure_param_readable(&self, hcam: i16, param: u32, operation: &str) -> Result<()> {
            self.param_available(hcam, param, operation)?;
            let access = self.get_u16_attr(hcam, param, ATTR_ACCESS, operation)?;
            if matches!(access, ACC_READ_ONLY | ACC_READ_WRITE) {
                Ok(())
            } else {
                Err(Error::new(
                    ErrorCode::Unsupported,
                    format!("PVCAM parameter is not readable for {operation}"),
                ))
            }
        }

        fn ensure_param_writable(&self, hcam: i16, param: u32, operation: &str) -> Result<()> {
            self.param_available(hcam, param, operation)?;
            let access = self.get_u16_attr(hcam, param, ATTR_ACCESS, operation)?;
            if matches!(access, ACC_READ_WRITE | ACC_WRITE_ONLY) {
                Ok(())
            } else {
                Err(Error::new(
                    ErrorCode::Unsupported,
                    format!("PVCAM parameter is not writable for {operation}"),
                ))
            }
        }
    }

    pub(super) fn read_temperature(path: &str, camera_name: &str) -> Result<Temperature> {
        with_open_camera(path, camera_name, |api, hcam| {
            api.ensure_param_readable(hcam, PARAM_TEMP, "temperature readback")?;
            read_temperature_hundredths(api, hcam, PARAM_TEMP, "temperature readback")
        })
    }

    pub(super) fn read_temperature_setpoint(path: &str, camera_name: &str) -> Result<Temperature> {
        with_open_camera(path, camera_name, |api, hcam| {
            api.ensure_param_readable(hcam, PARAM_TEMP_SETPOINT, "temperature setpoint readback")?;
            read_temperature_hundredths(
                api,
                hcam,
                PARAM_TEMP_SETPOINT,
                "temperature setpoint readback",
            )
        })
    }

    pub(super) fn set_temperature_setpoint(
        path: &str,
        camera_name: &str,
        temperature: Temperature,
    ) -> Result<Temperature> {
        with_open_camera(path, camera_name, |api, hcam| {
            api.ensure_param_writable(hcam, PARAM_TEMP_SETPOINT, "temperature setpoint write")?;
            let raw = temperature_hundredths(temperature)?;
            let min =
                api.get_i16_attr(hcam, PARAM_TEMP_SETPOINT, ATTR_MIN, "temperature minimum")?;
            let max =
                api.get_i16_attr(hcam, PARAM_TEMP_SETPOINT, ATTR_MAX, "temperature maximum")?;
            if raw < min || raw > max {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!(
                        "PVCAM temperature setpoint must be in {:.2}..={:.2} deg C",
                        min as f64 / 100.0,
                        max as f64 / 100.0
                    ),
                ));
            }
            let mut raw_setpoint = raw;
            Api::check(
                unsafe {
                    (api.set_param)(
                        hcam,
                        PARAM_TEMP_SETPOINT,
                        (&mut raw_setpoint as *mut i16).cast(),
                    )
                },
                "temperature setpoint write",
            )?;
            read_temperature_hundredths(
                api,
                hcam,
                PARAM_TEMP_SETPOINT,
                "temperature setpoint readback",
            )
            .or_else(|_| Ok(Temperature::from_celsius(raw as f64 / 100.0)))
        })
    }

    fn with_open_camera<T>(
        path: &str,
        camera_name: &str,
        action: impl FnOnce(&Api, i16) -> Result<T>,
    ) -> Result<T> {
        let api = Api::load(path)?;
        Api::check(unsafe { (api.init)() }, "init")?;
        let _init_guard = InitGuard { api: &api };
        let mut selected_name = selected_camera_name(&api, camera_name)?;
        let mut hcam = 0_i16;
        Api::check(
            unsafe { (api.open)(selected_name.as_mut_ptr(), &mut hcam, OPEN_EXCLUSIVE) },
            "open",
        )?;
        let _camera_guard = CameraGuard {
            api: &api,
            hcam,
            armed: false,
        };
        action(&api, hcam)
    }

    fn read_temperature_hundredths(
        api: &Api,
        hcam: i16,
        param: u32,
        operation: &str,
    ) -> Result<Temperature> {
        let raw = api.get_i16_attr(hcam, param, ATTR_CURRENT, operation)?;
        Ok(Temperature::from_celsius(raw as f64 / 100.0))
    }

    fn temperature_hundredths(temperature: Temperature) -> Result<i16> {
        let hundredths = temperature.celsius() * 100.0;
        if !hundredths.is_finite() || hundredths < i16::MIN as f64 || hundredths > i16::MAX as f64 {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "PVCAM temperature setpoint must fit in signed 16-bit hundredths of deg C",
            ));
        }
        Ok(hundredths.round() as i16)
    }

    pub(super) fn capture(
        path: &str,
        camera_name: &str,
        width: u32,
        height: u32,
        exposure: TimeInterval,
    ) -> Result<PvcamFrame> {
        if width == 0 || height == 0 || width > u16::MAX as u32 || height > u16::MAX as u32 {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "PVCAM capture requires configured width/height in 1..=65535",
            ));
        }
        let exposure_ms = exposure_ms(exposure)?;
        let api = Api::load(path)?;
        Api::check(unsafe { (api.init)() }, "init")?;
        let _init_guard = InitGuard { api: &api };
        let mut selected_name = selected_camera_name(&api, camera_name)?;
        let mut hcam = 0_i16;
        Api::check(
            unsafe { (api.open)(selected_name.as_mut_ptr(), &mut hcam, OPEN_EXCLUSIVE) },
            "open",
        )?;
        let camera_guard = CameraGuard {
            api: &api,
            hcam,
            armed: false,
        };

        let mut region = RgnType {
            s1: 0,
            s2: (width - 1) as u16,
            sbin: 1,
            p1: 0,
            p2: (height - 1) as u16,
            pbin: 1,
        };
        let mut expected_bytes = 0_u32;
        Api::check(
            unsafe {
                (api.setup_seq)(
                    hcam,
                    1,
                    1,
                    &mut region,
                    TIMED_MODE,
                    exposure_ms,
                    &mut expected_bytes,
                )
            },
            "setup sequence",
        )?;
        if expected_bytes == 0 {
            return Err(Error::new(
                ErrorCode::Transport,
                "PVCAM setup sequence returned an empty frame buffer",
            ));
        }
        let mut data = vec![0_u8; expected_bytes as usize];
        let mut camera_guard = camera_guard;
        camera_guard.armed = true;
        Api::check(
            unsafe { (api.start_seq)(hcam, data.as_mut_ptr().cast::<c_void>()) },
            "start sequence",
        )?;
        wait_complete(&api, hcam, exposure_ms)?;
        camera_guard.armed = false;
        Api::check(
            unsafe { (api.finish_seq)(hcam, data.as_mut_ptr().cast::<c_void>(), 0) },
            "finish sequence",
        )?;

        let pixels = width as usize * height as usize;
        let bit_depth = match data.len().checked_div(pixels) {
            Some(1) => 8,
            Some(2) => 16,
            _ => 16,
        };
        Ok(PvcamFrame {
            width,
            height,
            bit_depth,
            data,
        })
    }

    fn selected_camera_name(api: &Api, configured_name: &str) -> Result<Vec<c_char>> {
        let configured_name = configured_name.trim();
        if !configured_name.is_empty() {
            let name = CString::new(configured_name).map_err(|_| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    "PVCAM camera_name must not contain interior NUL bytes",
                )
            })?;
            return Ok(name
                .into_bytes_with_nul()
                .into_iter()
                .map(|byte| byte as c_char)
                .collect());
        }
        let mut total = 0_i16;
        Api::check(
            unsafe { (api.get_total)(&mut total as *mut i16) },
            "camera count",
        )?;
        if total <= 0 {
            return Err(Error::new(
                ErrorCode::Transport,
                "PVCAM runtime reported no cameras",
            ));
        }
        let mut buffer = [0 as c_char; MAX_CAM_NAME];
        Api::check(
            unsafe { (api.get_name)(0, buffer.as_mut_ptr()) },
            "first camera name",
        )?;
        let name = unsafe { CStr::from_ptr(buffer.as_ptr()) };
        Ok(name
            .to_bytes_with_nul()
            .iter()
            .copied()
            .map(|byte| byte as c_char)
            .collect())
    }

    fn exposure_ms(exposure: TimeInterval) -> Result<u32> {
        let milliseconds = exposure.seconds() * 1000.0;
        if !milliseconds.is_finite() || milliseconds < 0.0 {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "PVCAM exposure must be a finite non-negative interval",
            ));
        }
        Ok(milliseconds.round().clamp(1.0, u32::MAX as f64) as u32)
    }

    fn wait_complete(api: &Api, hcam: i16, exposure_ms: u32) -> Result<()> {
        let timeout =
            Duration::from_millis(exposure_ms as u64).saturating_add(Duration::from_secs(5));
        let started = Instant::now();
        loop {
            let mut status = 0_i16;
            let mut bytes_arrived = 0_u32;
            Api::check(
                unsafe { (api.check_status)(hcam, &mut status, &mut bytes_arrived) },
                "check status",
            )?;
            match status {
                READOUT_COMPLETE => return Ok(()),
                READOUT_FAILED => {
                    return Err(Error::new(ErrorCode::Transport, "PVCAM readout failed"))
                }
                _ if started.elapsed() >= timeout => {
                    return Err(Error::new(
                        ErrorCode::Timeout,
                        "PVCAM capture timed out waiting for readout completion",
                    ))
                }
                _ => std::thread::sleep(Duration::from_millis(10)),
            }
        }
    }

    fn symbol<T: Copy>(library: &Library, name: &str) -> Result<T> {
        unsafe { library.get::<T>(name.as_bytes()) }
            .map(|symbol| *symbol)
            .map_err(|_| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("PVCAM runtime is missing required symbol {name}"),
                )
            })
    }

    struct InitGuard<'a> {
        api: &'a Api,
    }

    impl Drop for InitGuard<'_> {
        fn drop(&mut self) {
            let _ = unsafe { (self.api.uninit)() };
        }
    }

    struct CameraGuard<'a> {
        api: &'a Api,
        hcam: i16,
        armed: bool,
    }

    impl Drop for CameraGuard<'_> {
        fn drop(&mut self) {
            if self.armed {
                let _ = unsafe { (self.api.abort)(self.hcam, CCS_HALT) };
            }
            let _ = unsafe { (self.api.close)(self.hcam) };
        }
    }
}
