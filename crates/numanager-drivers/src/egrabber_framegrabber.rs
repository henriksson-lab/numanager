use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};
use std::io::{BufReader, Read};
use std::path::Path;

#[derive(Debug, Clone)]
struct EGrabberFramegrabberProbe {
    label: String,
    model: String,
    serial_number: Option<String>,
    transport: String,
    sdk_root: Option<String>,
    producer_path: Option<String>,
    producer_sha256: Option<String>,
    load_sdk: bool,
}

pub(crate) struct EGrabberFramegrabberDiscovery {
    next_id: DriverId,
    probes: Vec<EGrabberFramegrabberProbe>,
}

impl EGrabberFramegrabberDiscovery {
    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| {
                matches!(
                    device.driver.as_str(),
                    "egrabber_framegrabber"
                        | "egrabber-framegrabber"
                        | "euresys_egrabber"
                        | "euresys-egrabber"
                        | "egrabber"
                )
            })
            .map(EGrabberFramegrabberProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for EGrabberFramegrabberDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .iter()
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                Ok(DriverCandidate::from_driver(
                    format!("{} ({})", probe.label, probe.model),
                    Box::new(EGrabberFramegrabberDriver::configured(id, probe.clone())),
                ))
            })
            .collect()
    }
}

impl EGrabberFramegrabberProbe {
    fn template() -> Self {
        Self {
            label: "Configured Euresys eGrabber framegrabber".into(),
            model: "Euresys eGrabber GenTL producer".into(),
            serial_number: None,
            transport: "PCI/GenTL".into(),
            sdk_root: None,
            producer_path: None,
            producer_sha256: None,
            load_sdk: false,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = Self::template();
        if !device.label.is_empty() {
            probe.label = device.label.clone();
        }
        probe.model = string_prop(device, "model").unwrap_or(probe.model);
        probe.serial_number = optional_string_prop(device, "serial_number", probe.serial_number);
        probe.transport = string_prop(device, "transport").unwrap_or(probe.transport);
        probe.sdk_root = optional_string_prop(device, "sdk_root", probe.sdk_root);
        probe.producer_path = optional_string_prop(device, "producer_path", probe.producer_path);
        probe.producer_sha256 =
            optional_string_prop(device, "producer_sha256", probe.producer_sha256);
        probe.load_sdk = bool_prop(device, "load_sdk").unwrap_or(probe.load_sdk);
        Ok(probe)
    }
}

struct EGrabberFramegrabberDriver {
    id: DriverId,
    framegrabber: DeviceId,
    camera_port: DeviceId,
    producer: ResourceId,
    probe: EGrabberFramegrabberProbe,
    next_token: u64,
    events: VecDeque<DriverEvent>,
}

impl EGrabberFramegrabberDriver {
    fn configured(id: DriverId, probe: EGrabberFramegrabberProbe) -> Self {
        Self {
            id,
            framegrabber: DeviceId(NodeId(id.0 * 1000 + 840)),
            camera_port: DeviceId(NodeId(id.0 * 1000 + 841)),
            producer: ResourceId(NodeId(id.0 * 1000 + 842)),
            probe,
            next_token: 1,
            events: VecDeque::new(),
        }
    }

    fn next_token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn framegrabber_descriptor(&self) -> DeviceDescriptor {
        DeviceDescriptor {
            id: self.framegrabber,
            driver: self.id,
            label: self.probe.label.clone(),
            vendor: Some("Euresys".into()),
            model: Some(self.probe.model.clone()),
            serial: self.probe.serial_number.clone(),
            kinds: vec!["framegrabber".into(), "gentl.producer".into(), "pci".into()],
            properties: vec![
                string_property("model", "Model"),
                string_property("serial_number", "Serial number"),
                string_property("transport", "Transport"),
                string_property("sdk_root", "SDK root"),
                string_property("producer_path", "GenTL producer path"),
                string_property("producer_sha256", "GenTL producer SHA-256"),
                property("load_sdk", "Load SDK", ValueType::Bool),
                string_property("sdk_state", "SDK state"),
                string_property("producer_file_status", "GenTL producer file status"),
                property(
                    "producer_file_size",
                    "GenTL producer file size",
                    ValueType::ByteCount,
                ),
                string_property("producer_digest_state", "GenTL producer digest state"),
                string_property("producer_abi_state", "GenTL producer ABI state"),
                string_property("gentl_probe_state", "GenTL probe state"),
                property(
                    "gentl_interface_count",
                    "GenTL interface count",
                    ValueType::I64,
                ),
                property("gentl_device_count", "GenTL device count", ValueType::I64),
                property("gentl_interfaces", "GenTL interfaces", ValueType::List),
                property("gentl_devices", "GenTL devices", ValueType::List),
                string_property("support_level", "Support level"),
                string_property("capture_gate", "Capture gate"),
                string_property("stream_gate", "Stream gate"),
                string_property("package_strategy", "Package strategy"),
                string_property("third_party_notice", "Third-party notice"),
                property("hardware_validated", "Hardware validated", ValueType::Bool),
            ],
            metadata: BTreeMap::from([
                ("vendor".into(), Value::String("Euresys".into())),
                ("family".into(), Value::String("eGrabber".into())),
                (
                    "transport".into(),
                    Value::String(self.probe.transport.clone()),
                ),
                ("standard".into(), Value::String("GenTL".into())),
                (
                    "evidence_class".into(),
                    Value::String("manufacturer SDK/source package".into()),
                ),
                (
                    "sdk_backend_enabled".into(),
                    Value::Bool(self.probe.load_sdk),
                ),
                ("hardware_validated".into(), Value::Bool(false)),
                (
                    "support_level".into(),
                    Value::String(self.support_level().into()),
                ),
            ]),
        }
    }

    fn camera_port_descriptor(&self) -> DeviceDescriptor {
        DeviceDescriptor {
            id: self.camera_port,
            driver: self.id,
            label: format!("{} camera link", self.probe.label),
            vendor: Some("Euresys".into()),
            model: Some("eGrabber remote-camera port".into()),
            serial: self.probe.serial_number.clone(),
            kinds: vec!["camera.binding".into(), "gentl.remote_device".into()],
            properties: vec![
                string_property("binding_state", "Binding state"),
                property("bound", "Bound", ValueType::Bool),
                property("available_devices", "Available devices", ValueType::List),
                string_property("capture_gate", "Capture gate"),
                string_property("stream_gate", "Stream gate"),
                property("hardware_validated", "Hardware validated", ValueType::Bool),
            ],
            metadata: BTreeMap::from([
                ("parent".into(), Value::String(self.probe.label.clone())),
                (
                    "binding_strategy".into(),
                    Value::String(
                        "GenTL interface/device inventory through opt-in SDK backend".into(),
                    ),
                ),
                ("hardware_validated".into(), Value::Bool(false)),
            ]),
        }
    }

    fn sdk_state(&self) -> &'static str {
        match (
            self.probe.producer_path.as_ref(),
            self.probe.producer_sha256.as_ref(),
        ) {
            (Some(_), Some(_)) => "configured_with_digest",
            (Some(_), None) => "configured_without_digest",
            (None, Some(_)) => "digest_without_path",
            (None, None) => "not_configured",
        }
    }

    fn support_level(&self) -> &'static str {
        "configured GenTL producer checks and opt-in interface/device inventory; acquisition backend pending"
    }

    fn capture_gate(&self) -> &'static str {
        "not_exposed_until_gentl_buffer_acquisition_backend_is_implemented"
    }

    fn stream_gate(&self) -> &'static str {
        "not_exposed_until_gentl_stream_lifecycle_is_implemented"
    }

    fn package_strategy(&self) -> &'static str {
        "load configured SDK/GenTL producer on demand only when feature and config opt in"
    }

    fn third_party_notice(&self) -> &'static str {
        "vendor SDK package is third-party excluded data unless redistribution terms are recorded"
    }

    fn producer_file_status(&self) -> String {
        let Some(path) = self.probe.producer_path.as_deref() else {
            return "not_configured".into();
        };
        match std::fs::metadata(Path::new(path)) {
            Ok(metadata) if metadata.is_file() => "present".into(),
            Ok(_) => "not_a_file".into(),
            Err(error) => format!("unavailable:{}", error.kind()),
        }
    }

    fn producer_file_size(&self) -> Result<Value> {
        let Some(path) = self.probe.producer_path.as_deref() else {
            return Ok(Value::ByteCount(ByteCount::new(0)));
        };
        let metadata = std::fs::metadata(Path::new(path)).map_err(|error| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("eGrabber GenTL producer file is unavailable: {error}"),
            )
        })?;
        if !metadata.is_file() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "eGrabber GenTL producer path is not a regular file",
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

    fn producer_sha256(path: &str) -> Result<String> {
        let file = std::fs::File::open(Path::new(path)).map_err(|error| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!(
                    "eGrabber GenTL producer file is unavailable for digest verification: {error}"
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
                    format!("eGrabber GenTL producer digest read failed: {error}"),
                )
            })?;
            if bytes == 0 {
                break;
            }
            hasher.update(&buffer[..bytes]);
        }
        Ok(hex::encode(hasher.finalize()))
    }

    fn producer_digest_state(&self) -> String {
        let Some(configured_sha256) = self.probe.producer_sha256.as_deref() else {
            return "not_configured".into();
        };
        let Some(expected) = Self::normalized_sha256(configured_sha256) else {
            return "invalid_configured_sha256".into();
        };
        let Some(path) = self.probe.producer_path.as_deref() else {
            return "digest_without_path".into();
        };
        match Self::producer_sha256(path) {
            Ok(actual) if actual == expected => "verified".into(),
            Ok(actual) => format!("mismatch:{actual}"),
            Err(error) => format!("unavailable:{}", compact_error(&error.message)),
        }
    }

    fn producer_digest_allows_use(&self) -> String {
        let Some(configured_sha256) = self.probe.producer_sha256.as_deref() else {
            return "missing_sha256".into();
        };
        let Some(expected) = Self::normalized_sha256(configured_sha256) else {
            return "invalid_configured_sha256".into();
        };
        let Some(path) = self.probe.producer_path.as_deref() else {
            return "missing_path".into();
        };
        match Self::producer_sha256(path) {
            Ok(actual) if actual == expected => "verified".into(),
            Ok(_) => "digest_mismatch".into(),
            Err(error) => format!("digest_unavailable:{}", compact_error(&error.message)),
        }
    }

    fn producer_abi_state(&self) -> String {
        if !self.probe.load_sdk {
            return "backend_disabled".into();
        }
        let digest_state = self.producer_digest_allows_use();
        if digest_state != "verified" {
            return digest_state;
        }
        self.producer_abi_state_inner()
    }

    #[cfg(not(feature = "egrabber-sdk"))]
    fn producer_abi_state_inner(&self) -> String {
        "feature_disabled".into()
    }

    #[cfg(feature = "egrabber-sdk")]
    fn producer_abi_state_inner(&self) -> String {
        let Some(path) = self.probe.producer_path.as_deref() else {
            return "missing_path".into();
        };
        match gentl_backend::check_abi(path) {
            Ok(()) => "available".into(),
            Err(error) => error,
        }
    }

    fn gentl_inventory(&self) -> GenTlInventory {
        if !self.probe.load_sdk {
            return GenTlInventory::disabled("backend_disabled");
        }
        let digest_state = self.producer_digest_allows_use();
        if digest_state != "verified" {
            return GenTlInventory::disabled(digest_state);
        }
        self.gentl_inventory_inner()
    }

    #[cfg(not(feature = "egrabber-sdk"))]
    fn gentl_inventory_inner(&self) -> GenTlInventory {
        GenTlInventory::disabled("feature_disabled")
    }

    #[cfg(feature = "egrabber-sdk")]
    fn gentl_inventory_inner(&self) -> GenTlInventory {
        let Some(path) = self.probe.producer_path.as_deref() else {
            return GenTlInventory::disabled("missing_path");
        };
        match gentl_backend::inventory(path) {
            Ok(inventory) => inventory,
            Err(error) => GenTlInventory::disabled(error),
        }
    }

    fn read_framegrabber_property(&self, key: &str) -> Result<Value> {
        match key {
            "model" => Ok(Value::String(self.probe.model.clone())),
            "serial_number" => Ok(optional_value(self.probe.serial_number.as_deref())),
            "transport" => Ok(Value::String(self.probe.transport.clone())),
            "sdk_root" => Ok(optional_value(self.probe.sdk_root.as_deref())),
            "producer_path" => Ok(optional_value(self.probe.producer_path.as_deref())),
            "producer_sha256" => Ok(optional_value(self.probe.producer_sha256.as_deref())),
            "load_sdk" => Ok(Value::Bool(self.probe.load_sdk)),
            "sdk_state" => Ok(Value::String(self.sdk_state().into())),
            "producer_file_status" => Ok(Value::String(self.producer_file_status())),
            "producer_file_size" => self.producer_file_size(),
            "producer_digest_state" => Ok(Value::String(self.producer_digest_state())),
            "producer_abi_state" => Ok(Value::String(self.producer_abi_state())),
            "gentl_probe_state" => Ok(Value::String(self.gentl_inventory().state)),
            "gentl_interface_count" => {
                Ok(Value::I64(self.gentl_inventory().interfaces.len() as i64))
            }
            "gentl_device_count" => Ok(Value::I64(self.gentl_inventory().device_count() as i64)),
            "gentl_interfaces" => Ok(self.gentl_inventory().interfaces_value()),
            "gentl_devices" => Ok(self.gentl_inventory().devices_value()),
            "support_level" => Ok(Value::String(self.support_level().into())),
            "capture_gate" => Ok(Value::String(self.capture_gate().into())),
            "stream_gate" => Ok(Value::String(self.stream_gate().into())),
            "package_strategy" => Ok(Value::String(self.package_strategy().into())),
            "third_party_notice" => Ok(Value::String(self.third_party_notice().into())),
            "hardware_validated" => Ok(Value::Bool(false)),
            other => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown eGrabber framegrabber property {other}"),
            )),
        }
    }

    fn read_camera_port_property(&self, key: &str) -> Result<Value> {
        match key {
            "binding_state" => Ok(Value::String(self.gentl_inventory().state)),
            "bound" => Ok(Value::Bool(self.gentl_inventory().device_count() > 0)),
            "available_devices" => Ok(self.gentl_inventory().devices_value()),
            "capture_gate" => Ok(Value::String(self.capture_gate().into())),
            "stream_gate" => Ok(Value::String(self.stream_gate().into())),
            "hardware_validated" => Ok(Value::Bool(false)),
            other => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown eGrabber camera-port property {other}"),
            )),
        }
    }

    fn inventory_value(&self) -> Value {
        self.gentl_inventory().to_value()
    }
}

impl Driver for EGrabberFramegrabberDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        vec![
            self.framegrabber_descriptor(),
            self.camera_port_descriptor(),
        ]
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        let mut metadata = BTreeMap::from([
            ("backend".into(), Value::String("GenTL producer".into())),
            ("load_sdk".into(), Value::Bool(self.probe.load_sdk)),
            ("sdk_state".into(), Value::String(self.sdk_state().into())),
            (
                "producer_digest_state".into(),
                Value::String(self.producer_digest_state()),
            ),
        ]);
        if let Some(path) = &self.probe.producer_path {
            metadata.insert("producer_path".into(), Value::String(path.clone()));
        }
        if let Some(root) = &self.probe.sdk_root {
            metadata.insert("sdk_root".into(), Value::String(root.clone()));
        }
        vec![ResourceDescriptor {
            id: self.producer,
            driver: self.id,
            label: format!("{} GenTL producer", self.probe.label),
            kind: "gentl_producer".into(),
            metadata,
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.framegrabber {
            return vec![CapabilityDescriptor::with_name(
                CapabilityId(1),
                self.framegrabber,
                CapabilityKind::Custom("GenTLInventory".into()),
                "GenTLInventory",
                ValueType::Map,
            )];
        }
        Vec::new()
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } if *device == self.framegrabber => {
                    let _ = self.read_framegrabber_property(key)?;
                }
                Command::ReadProperty { device, key } if *device == self.camera_port => {
                    let _ = self.read_camera_port_property(key)?;
                }
                Command::WriteProperty { device, .. }
                    if *device == self.framegrabber || *device == self.camera_port =>
                {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "eGrabber framegrabber properties are read-only",
                    ));
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if *device == self.framegrabber && *capability == CapabilityId(1) => {
                    if let CapabilityRequest::GenericCommand(request) = request {
                        if request.is_hidden_maintenance() {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "hidden maintenance command",
                            ));
                        }
                    }
                    if !matches!(
                        request,
                        CapabilityRequest::GenericCommand(_) | CapabilityRequest::None
                    ) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "GenTLInventory expects GenericCommandRequest or no request",
                        ));
                    }
                }
                Command::Invoke { device, .. }
                    if *device == self.framegrabber || *device == self.camera_port =>
                {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "unsupported eGrabber framegrabber capability",
                    ));
                }
                _ => {}
            }
        }
        Ok(PreparedBatch {
            id: batch.id,
            commands: batch.commands.clone(),
            physical_transactions: Vec::new(),
        })
    }

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.next_token();
        let mut result = Value::Null;
        for command in prepared.commands {
            match command {
                Command::ReadProperty { device, key } if device == self.framegrabber => {
                    result = self.read_framegrabber_property(&key)?;
                }
                Command::ReadProperty { device, key } if device == self.camera_port => {
                    result = self.read_camera_port_property(&key)?;
                }
                Command::Invoke {
                    device, capability, ..
                } if device == self.framegrabber && capability == CapabilityId(1) => {
                    result = self.inventory_value();
                }
                Command::Invoke { device, .. }
                    if device == self.framegrabber || device == self.camera_port =>
                {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "unsupported eGrabber framegrabber capability",
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

#[derive(Debug, Clone)]
struct GenTlInventory {
    state: String,
    interfaces: Vec<GenTlInterface>,
}

#[derive(Debug, Clone)]
struct GenTlInterface {
    id: String,
    devices: Vec<String>,
}

impl GenTlInventory {
    fn disabled(state: impl Into<String>) -> Self {
        Self {
            state: state.into(),
            interfaces: Vec::new(),
        }
    }

    fn device_count(&self) -> usize {
        self.interfaces
            .iter()
            .map(|interface| interface.devices.len())
            .sum()
    }

    fn interfaces_value(&self) -> Value {
        Value::List(
            self.interfaces
                .iter()
                .map(|interface| Value::String(interface.id.clone()))
                .collect(),
        )
    }

    fn devices_value(&self) -> Value {
        Value::List(
            self.interfaces
                .iter()
                .flat_map(|interface| {
                    interface.devices.iter().map(|device| {
                        Value::Map(BTreeMap::from([
                            ("interface".into(), Value::String(interface.id.clone())),
                            ("device".into(), Value::String(device.clone())),
                        ]))
                    })
                })
                .collect(),
        )
    }

    fn to_value(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("state".into(), Value::String(self.state.clone())),
            (
                "interface_count".into(),
                Value::I64(self.interfaces.len() as i64),
            ),
            (
                "device_count".into(),
                Value::I64(self.device_count() as i64),
            ),
            ("interfaces".into(), self.interfaces_value()),
            ("devices".into(), self.devices_value()),
        ]))
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
        _ => fallback,
    }
}

fn bool_prop(device: &DeviceConfig, key: &str) -> Option<bool> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn optional_value(value: Option<&str>) -> Value {
    value
        .map(|value| Value::String(value.into()))
        .unwrap_or(Value::Null)
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

#[cfg(feature = "egrabber-sdk")]
mod gentl_backend {
    use super::{compact_error, GenTlInterface, GenTlInventory};
    use libloading::Library;
    use std::ffi::CStr;
    use std::os::raw::{c_char, c_void};
    use std::ptr;

    type GcError = i32;
    type Handle = *mut c_void;
    type GcInitLib = unsafe extern "C" fn() -> GcError;
    type GcCloseLib = unsafe extern "C" fn() -> GcError;
    type TlOpen = unsafe extern "C" fn(*mut Handle) -> GcError;
    type TlClose = unsafe extern "C" fn(Handle) -> GcError;
    type TlGetNumInterfaces = unsafe extern "C" fn(Handle, *mut u32) -> GcError;
    type TlGetInterfaceId = unsafe extern "C" fn(Handle, u32, *mut c_char, *mut usize) -> GcError;
    type TlOpenInterface = unsafe extern "C" fn(Handle, *const c_char, *mut Handle) -> GcError;
    type IfClose = unsafe extern "C" fn(Handle) -> GcError;
    type IfGetNumDevices = unsafe extern "C" fn(Handle, *mut u32) -> GcError;
    type IfGetDeviceId = unsafe extern "C" fn(Handle, u32, *mut c_char, *mut usize) -> GcError;

    const GC_ERR_SUCCESS: GcError = 0;
    const GC_ERR_BUFFER_TOO_SMALL: GcError = -1016;

    pub fn check_abi(path: &str) -> Result<(), String> {
        let library = unsafe { Library::new(path) }
            .map_err(|error| format!("load_error:{}", compact_error(&error.to_string())))?;
        for symbol in [
            b"GCInitLib\0".as_slice(),
            b"GCCloseLib\0".as_slice(),
            b"TLOpen\0".as_slice(),
            b"TLClose\0".as_slice(),
            b"TLGetNumInterfaces\0".as_slice(),
            b"TLGetInterfaceID\0".as_slice(),
            b"TLOpenInterface\0".as_slice(),
            b"IFClose\0".as_slice(),
            b"IFGetNumDevices\0".as_slice(),
            b"IFGetDeviceID\0".as_slice(),
        ] {
            unsafe {
                library.get::<*mut c_void>(symbol).map_err(|error| {
                    format!(
                        "missing_symbol:{}:{}",
                        String::from_utf8_lossy(&symbol[..symbol.len() - 1]),
                        compact_error(&error.to_string())
                    )
                })?;
            }
        }
        Ok(())
    }

    pub fn inventory(path: &str) -> Result<GenTlInventory, String> {
        let library = unsafe { Library::new(path) }
            .map_err(|error| format!("load_error:{}", compact_error(&error.to_string())))?;
        let gc_init = unsafe {
            *library
                .get::<GcInitLib>(b"GCInitLib\0")
                .map_err(load_error)?
        };
        let gc_close = unsafe {
            *library
                .get::<GcCloseLib>(b"GCCloseLib\0")
                .map_err(load_error)?
        };
        let tl_open = unsafe { *library.get::<TlOpen>(b"TLOpen\0").map_err(load_error)? };
        let tl_close = unsafe { *library.get::<TlClose>(b"TLClose\0").map_err(load_error)? };
        let tl_get_num_interfaces = unsafe {
            *library
                .get::<TlGetNumInterfaces>(b"TLGetNumInterfaces\0")
                .map_err(load_error)?
        };
        let tl_get_interface_id = unsafe {
            *library
                .get::<TlGetInterfaceId>(b"TLGetInterfaceID\0")
                .map_err(load_error)?
        };
        let tl_open_interface = unsafe {
            *library
                .get::<TlOpenInterface>(b"TLOpenInterface\0")
                .map_err(load_error)?
        };
        let if_close = unsafe { *library.get::<IfClose>(b"IFClose\0").map_err(load_error)? };
        let if_get_num_devices = unsafe {
            *library
                .get::<IfGetNumDevices>(b"IFGetNumDevices\0")
                .map_err(load_error)?
        };
        let if_get_device_id = unsafe {
            *library
                .get::<IfGetDeviceId>(b"IFGetDeviceID\0")
                .map_err(load_error)?
        };

        call0("GCInitLib", unsafe { gc_init() })?;
        let mut tl: Handle = ptr::null_mut();
        if let Err(error) = call0("TLOpen", unsafe { tl_open(&mut tl) }) {
            let _ = unsafe { gc_close() };
            return Err(error);
        }

        let result = enumerate(
            tl,
            tl_get_num_interfaces,
            tl_get_interface_id,
            tl_open_interface,
            if_close,
            if_get_num_devices,
            if_get_device_id,
        );
        let close_result = call0("TLClose", unsafe { tl_close(tl) });
        let gc_close_result = call0("GCCloseLib", unsafe { gc_close() });

        let interfaces = result?;
        close_result?;
        gc_close_result?;
        Ok(GenTlInventory {
            state: "available".into(),
            interfaces,
        })
    }

    fn enumerate(
        tl: Handle,
        tl_get_num_interfaces: TlGetNumInterfaces,
        tl_get_interface_id: TlGetInterfaceId,
        tl_open_interface: TlOpenInterface,
        if_close: IfClose,
        if_get_num_devices: IfGetNumDevices,
        if_get_device_id: IfGetDeviceId,
    ) -> Result<Vec<GenTlInterface>, String> {
        let mut interface_count = 0_u32;
        call0("TLGetNumInterfaces", unsafe {
            tl_get_num_interfaces(tl, &mut interface_count)
        })?;
        let mut interfaces = Vec::new();
        for index in 0..interface_count {
            let id = read_string("TLGetInterfaceID", |buffer, size| unsafe {
                tl_get_interface_id(tl, index, buffer, size)
            })?;
            let mut interface_handle: Handle = ptr::null_mut();
            call0("TLOpenInterface", unsafe {
                tl_open_interface(
                    tl,
                    nul_terminated(&id).as_ptr().cast(),
                    &mut interface_handle,
                )
            })?;

            let devices = enumerate_devices(interface_handle, if_get_num_devices, if_get_device_id);
            let close_result = call0("IFClose", unsafe { if_close(interface_handle) });
            let devices = devices?;
            close_result?;
            interfaces.push(GenTlInterface { id, devices });
        }
        Ok(interfaces)
    }

    fn enumerate_devices(
        interface_handle: Handle,
        if_get_num_devices: IfGetNumDevices,
        if_get_device_id: IfGetDeviceId,
    ) -> Result<Vec<String>, String> {
        let mut device_count = 0_u32;
        call0("IFGetNumDevices", unsafe {
            if_get_num_devices(interface_handle, &mut device_count)
        })?;
        let mut devices = Vec::new();
        for index in 0..device_count {
            devices.push(read_string("IFGetDeviceID", |buffer, size| unsafe {
                if_get_device_id(interface_handle, index, buffer, size)
            })?);
        }
        Ok(devices)
    }

    fn read_string<F>(name: &str, mut call: F) -> Result<String, String>
    where
        F: FnMut(*mut c_char, *mut usize) -> GcError,
    {
        let mut size = 512_usize;
        let mut buffer = vec![0_u8; size];
        let mut status = call(buffer.as_mut_ptr().cast(), &mut size);
        if status == GC_ERR_BUFFER_TOO_SMALL && size > buffer.len() {
            buffer.resize(size, 0);
            status = call(buffer.as_mut_ptr().cast(), &mut size);
        }
        call0(name, status)?;
        Ok(c_string_from_buffer(&buffer))
    }

    fn c_string_from_buffer(buffer: &[u8]) -> String {
        match CStr::from_bytes_until_nul(buffer) {
            Ok(value) => value.to_string_lossy().into_owned(),
            Err(_) => String::from_utf8_lossy(buffer)
                .trim_end_matches('\0')
                .into(),
        }
    }

    fn nul_terminated(value: &str) -> Vec<u8> {
        let mut bytes = value
            .as_bytes()
            .iter()
            .copied()
            .filter(|byte| *byte != 0)
            .collect::<Vec<_>>();
        bytes.push(0);
        bytes
    }

    fn call0(name: &str, status: GcError) -> Result<(), String> {
        if status == GC_ERR_SUCCESS {
            Ok(())
        } else {
            Err(format!("{name}_error:{status}"))
        }
    }

    fn load_error(error: libloading::Error) -> String {
        format!("symbol_error:{}", compact_error(&error.to_string()))
    }
}
