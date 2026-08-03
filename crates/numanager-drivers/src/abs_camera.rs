use libloading::Library;
use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};
use std::io::{BufReader, Read};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct AbsCameraProbe {
    label: String,
    product: String,
    serial_number: Option<String>,
    transport_hint: String,
    width: u32,
    height: u32,
    pixel_format: String,
    exposure: TimeInterval,
    vendor_runtime_path: Option<String>,
    vendor_runtime_sha256: Option<String>,
    load_vendor_runtime: bool,
}

pub struct AbsCameraDiscovery {
    next_id: DriverId,
    probes: Vec<AbsCameraProbe>,
}

impl AbsCameraDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![AbsCameraProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), "abs_camera" | "abs-camera"))
            .map(AbsCameraProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for AbsCameraDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .iter()
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                Ok(DriverCandidate::from_driver(
                    format!("{} ({})", probe.label, probe.product),
                    Box::new(AbsCameraDriver::configured(id, probe.clone())),
                ))
            })
            .collect()
    }
}

impl AbsCameraProbe {
    fn fixture() -> Self {
        Self {
            label: "Configured ABS camera reverse engineered support".into(),
            product: "ABS CamUSB camera".into(),
            serial_number: None,
            transport_hint: "unknown; platform or vendor USB route required".into(),
            width: 0,
            height: 0,
            pixel_format: ImageEncoding::Native.property_value().into(),
            exposure: TimeInterval::from_milliseconds(10.0),
            vendor_runtime_path: None,
            vendor_runtime_sha256: None,
            load_vendor_runtime: false,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = Self::fixture();
        if !device.label.is_empty() {
            probe.label = device.label.clone();
        }
        probe.product = string_prop(device, "product").unwrap_or(probe.product);
        probe.serial_number = optional_string_prop(device, "serial_number", probe.serial_number);
        probe.transport_hint =
            string_prop(device, "transport_hint").unwrap_or(probe.transport_hint);
        probe.width = pixel_count_prop(device, "width")?.unwrap_or(probe.width);
        probe.height = pixel_count_prop(device, "height")?.unwrap_or(probe.height);
        probe.pixel_format =
            pixel_format_prop(device, "pixel_format")?.unwrap_or(probe.pixel_format);
        probe.exposure = time_interval_prop(device, "exposure")?.unwrap_or(probe.exposure);
        probe.vendor_runtime_path =
            optional_string_prop(device, "vendor_runtime_path", probe.vendor_runtime_path);
        probe.vendor_runtime_sha256 =
            optional_string_prop(device, "vendor_runtime_sha256", probe.vendor_runtime_sha256);
        probe.load_vendor_runtime =
            bool_prop(device, "load_vendor_runtime").unwrap_or(probe.load_vendor_runtime);
        Ok(probe)
    }
}

pub struct AbsCameraDriver {
    id: DriverId,
    camera: DeviceId,
    transport: ResourceId,
    probe: AbsCameraProbe,
    next_token: u64,
    events: VecDeque<DriverEvent>,
}

impl AbsCameraDriver {
    pub fn configured(id: DriverId, probe: AbsCameraProbe) -> Self {
        Self {
            id,
            camera: DeviceId(NodeId(id.0 * 1000 + 710)),
            transport: ResourceId(NodeId(id.0 * 1000 + 711)),
            probe,
            next_token: 1,
            events: VecDeque::new(),
        }
    }

    fn descriptor(&self) -> DeviceDescriptor {
        DeviceDescriptor {
            id: self.camera,
            driver: self.id,
            label: self.probe.label.clone(),
            vendor: Some("ABS".into()),
            model: Some(self.probe.product.clone()),
            serial: self.probe.serial_number.clone(),
            kinds: vec!["camera".into(), "reverse.engineered".into()],
            properties: vec![
                string_property("model", "Model"),
                string_property("serial_number", "Serial number"),
                string_property("support_level", "Support level"),
                string_property("transport_hint", "Transport hint"),
                property("width", "Width", ValueType::PixelCount),
                property("height", "Height", ValueType::PixelCount),
                string_property("pixel_format", "Pixel format"),
                writable_property("exposure", "Exposure", ValueType::TimeInterval),
                string_property("vendor_runtime_path", "Vendor runtime path"),
                string_property("vendor_runtime_sha256", "Vendor runtime SHA-256"),
                property(
                    "load_vendor_runtime",
                    "Load vendor runtime",
                    ValueType::Bool,
                ),
                string_property("vendor_runtime_state", "Vendor runtime state"),
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
                string_property("package_gate", "Package gate"),
                string_property("third_party_notice", "Third-party notice"),
                string_property("capture_gate", "Capture gate"),
                string_property("stream_gate", "Stream gate"),
                string_property("control_gate", "Control gate"),
                property("feature_summary", "Feature summary", ValueType::Map),
            ],
            metadata: BTreeMap::from([
                (
                    "support_level".into(),
                    Value::String(
                        "runtime-package evidence with file-status/digest/loadability/ABI-symbol checks and opt-in one-shot vendor-runtime capture".into(),
                    ),
                ),
                (
                    "vendor_runtime_backend_enabled".into(),
                    Value::Bool(self.probe.load_vendor_runtime),
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
                ("hardware_validated".into(), Value::Bool(false)),
            ]),
        }
    }

    fn vendor_runtime_configured(&self) -> bool {
        self.probe.vendor_runtime_path.is_some()
    }

    fn vendor_runtime_state(&self) -> &'static str {
        match (
            self.probe.vendor_runtime_path.as_ref(),
            self.probe.vendor_runtime_sha256.as_ref(),
        ) {
            (Some(_), Some(_)) => "configured_with_digest",
            (Some(_), None) => "configured_without_digest",
            (None, Some(_)) => "digest_without_path",
            (None, None) => "not_configured",
        }
    }

    fn vendor_runtime_file_status(&self) -> String {
        let Some(path) = self.probe.vendor_runtime_path.as_deref() else {
            return "not_configured".into();
        };
        match std::fs::metadata(Path::new(path)) {
            Ok(metadata) if metadata.is_file() => "present".into(),
            Ok(_) => "not_a_file".into(),
            Err(error) => format!("unavailable:{}", error.kind()),
        }
    }

    fn vendor_runtime_file_size(&self) -> Result<Value> {
        let Some(path) = self.probe.vendor_runtime_path.as_deref() else {
            return Ok(Value::ByteCount(ByteCount::new(0)));
        };
        let metadata = std::fs::metadata(Path::new(path)).map_err(|error| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("ABS camera vendor runtime file is unavailable: {error}"),
            )
        })?;
        if !metadata.is_file() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "ABS camera vendor runtime path is not a regular file",
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
                    "ABS camera vendor runtime file is unavailable for digest verification: {error}"
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
                    format!("ABS camera vendor runtime digest read failed: {error}"),
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
        let Some(configured_sha256) = self.probe.vendor_runtime_sha256.as_deref() else {
            return "not_configured".into();
        };
        let Some(expected) = Self::normalized_sha256(configured_sha256) else {
            return "invalid_configured_sha256".into();
        };
        let Some(path) = self.probe.vendor_runtime_path.as_deref() else {
            return "digest_without_path".into();
        };
        match Self::vendor_runtime_sha256(path) {
            Ok(actual) if actual == expected => "verified".into(),
            Ok(actual) => format!("mismatch:{actual}"),
            Err(error) => format!("unavailable:{}", compact_error(&error.message)),
        }
    }

    fn vendor_runtime_digest_allows_use(&self) -> String {
        let Some(configured_sha256) = self.probe.vendor_runtime_sha256.as_deref() else {
            return "missing_sha256".into();
        };
        let Some(expected) = Self::normalized_sha256(configured_sha256) else {
            return "invalid_configured_sha256".into();
        };
        let Some(path) = self.probe.vendor_runtime_path.as_deref() else {
            return "missing_path".into();
        };
        match Self::vendor_runtime_sha256(path) {
            Ok(actual) if actual == expected => "verified".into(),
            Ok(_) => "digest_mismatch".into(),
            Err(error) => format!("digest_unavailable:{}", compact_error(&error.message)),
        }
    }

    fn vendor_runtime_probe_state(&self) -> String {
        if !self.probe.load_vendor_runtime {
            return "disabled".into();
        }
        let digest_state = self.vendor_runtime_digest_allows_use();
        if digest_state != "verified" {
            return digest_state;
        }
        let Some(path) = self.probe.vendor_runtime_path.as_deref() else {
            return "missing_path".into();
        };
        if let Err(error) = std::fs::metadata(Path::new(path)) {
            return format!("file_unavailable:{}", error.kind());
        }

        // Loading is the explicit vendor-runtime boundary. No ABS camera ABI or
        // hardware operation is invoked by this read-only probe.
        match unsafe { Library::new(path) } {
            Ok(_library) => "loaded".into(),
            Err(error) => format!("load_error:{}", compact_error(&error.to_string())),
        }
    }

    fn vendor_runtime_expected_symbols(&self) -> &'static [&'static str] {
        &[
            "CamUSB_InitCameraExS",
            "CamUSB_FreeCamera",
            "CamUSB_GetImage",
            "CamUSB_ReleaseImage",
            "CamUSB_AbortGetImage",
            "CamUSB_TriggerImage",
            "CamUSB_SetCaptureMode",
            "CamUSB_SetExposureTime",
            "CamUSB_GetLastError",
        ]
    }

    fn vendor_runtime_abi_state(&self) -> String {
        if !self.probe.load_vendor_runtime {
            return "disabled".into();
        }
        let digest_state = self.vendor_runtime_digest_allows_use();
        if digest_state != "verified" {
            return digest_state;
        }
        let Some(path) = self.probe.vendor_runtime_path.as_deref() else {
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

    fn package_strategy(&self) -> &'static str {
        "use optional third-party vendor firmware/runtime package as an explicit backend when a project-owned replacement is not available"
    }

    fn package_gate(&self) -> &'static str {
        "runtime package identity and explicit loadability/symbol probes are exposed; one-shot capture uses the verified vendor runtime; native transport is not exposed because protocol evidence is absent"
    }

    fn read_property(&self, key: &str) -> Result<Value> {
        match key {
            "model" => Ok(Value::String(self.probe.product.clone())),
            "serial_number" => Ok(Value::String(
                self.probe.serial_number.clone().unwrap_or_default(),
            )),
            "support_level" => Ok(Value::String(
                "runtime-package evidence with file-status/digest/loadability/ABI-symbol checks and opt-in one-shot vendor-runtime capture; native stream/control is not exposed because protocol evidence is absent".into(),
            )),
            "transport_hint" => Ok(Value::String(self.probe.transport_hint.clone())),
            "width" => Ok(Value::PixelCount(PixelCount::new(self.probe.width))),
            "height" => Ok(Value::PixelCount(PixelCount::new(self.probe.height))),
            "pixel_format" => Ok(Value::String(self.probe.pixel_format.clone())),
            "exposure" => Ok(Value::TimeInterval(self.probe.exposure)),
            "vendor_runtime_path" => Ok(Value::String(
                self.probe.vendor_runtime_path.clone().unwrap_or_default(),
            )),
            "vendor_runtime_sha256" => Ok(Value::String(
                self.probe.vendor_runtime_sha256.clone().unwrap_or_default(),
            )),
            "load_vendor_runtime" => Ok(Value::Bool(self.probe.load_vendor_runtime)),
            "vendor_runtime_state" => Ok(Value::String(self.vendor_runtime_state().into())),
            "vendor_runtime_file_status" => Ok(Value::String(self.vendor_runtime_file_status())),
            "vendor_runtime_file_size" => self.vendor_runtime_file_size(),
            "vendor_runtime_digest_state" => Ok(Value::String(self.vendor_runtime_digest_state())),
            "vendor_runtime_probe_state" => Ok(Value::String(self.vendor_runtime_probe_state())),
            "vendor_runtime_abi_state" => Ok(Value::String(self.vendor_runtime_abi_state())),
            "package_strategy" => Ok(Value::String(self.package_strategy().into())),
            "package_gate" => Ok(Value::String(self.package_gate().into())),
            "third_party_notice" => Ok(Value::String(
                "configured ABS camera vendor firmware/runtime packages are third-party excluded data"
                    .into(),
            )),
            "capture_gate" => Ok(Value::String(
                "CameraCapture uses the verified vendor runtime; native frame transport is not exposed because frame/completion evidence is absent".into(),
            )),
            "stream_gate" => Ok(Value::String(
                "CameraStream uses repeated verified vendor-runtime one-shot captures; native continuous streaming waits for dropped-frame and ring-buffer evidence".into(),
            )),
            "control_gate" => Ok(Value::String(
                "runtime one-shot capture applies async software trigger; gain, persistent trigger modes, and broader SDK-free writes are not exposed because command mapping evidence is absent".into(),
            )),
            "feature_summary" => Ok(Value::Map(BTreeMap::from([
                ("camera_identity".into(), Value::Bool(true)),
                ("platform_route_known".into(), Value::Bool(false)),
                ("native_frame_protocol_known".into(), Value::Bool(false)),
                ("runtime_frame_layout_known".into(), Value::Bool(true)),
                ("capture_supported".into(), Value::Bool(true)),
                ("stream_supported".into(), Value::Bool(true)),
            ]))),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                "unknown ABS camera property",
            )),
        }
    }

    fn validate_write_property(&self, key: &str, value: &Value) -> Result<()> {
        match key {
            "exposure" => {
                let Value::TimeInterval(exposure) = value else {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "ABS camera exposure expects TimeInterval",
                    ));
                };
                validate_positive_exposure(*exposure)
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "ABS camera exposes only the evidenced writable capture properties",
            )),
        }
    }

    fn write_property(&mut self, key: String, value: Value) -> Result<Value> {
        self.validate_write_property(&key, &value)?;
        match (key.as_str(), value) {
            ("exposure", Value::TimeInterval(exposure)) => {
                self.probe.exposure = exposure;
                Ok(Value::TimeInterval(exposure))
            }
            _ => unreachable!("ABS camera write validation constrains writable properties"),
        }
    }

    fn next_token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn capture_frame(
        &mut self,
        token: DriverToken,
        request: CameraCaptureRequest,
    ) -> Result<Value> {
        if !self.probe.load_vendor_runtime {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "ABS camera capture requires load_vendor_runtime=true",
            ));
        }
        let digest_state = self.vendor_runtime_digest_allows_use();
        if digest_state != "verified" {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("ABS camera vendor runtime is not verified: {digest_state}"),
            ));
        }
        let path = self.probe.vendor_runtime_path.as_deref().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "ABS camera runtime path is required",
            )
        })?;
        #[cfg(feature = "os-usb")]
        let frame = live_abs_camera::capture(path, self.probe.exposure)?;
        #[cfg(not(feature = "os-usb"))]
        {
            let _ = (path, token, request);
            return Err(Error::new(
                ErrorCode::Unsupported,
                "ABS camera capture requires numanager-drivers/os-usb",
            ));
        }
        #[cfg(feature = "os-usb")]
        {
            let requested = request.encoding.unwrap_or(ImageEncoding::Native);
            if !encoding_matches(&requested, &frame.pixel_format) {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    format!(
                        "ABS camera runtime returned {}, which does not satisfy requested {}",
                        frame.pixel_format,
                        requested.property_value()
                    ),
                ));
            }
            self.probe.width = frame.width;
            self.probe.height = frame.height;
            self.probe.pixel_format = frame.pixel_format.clone();
            let handle = FrameHandle {
                stream: StreamId(self.camera.0 .0),
                frame: FrameId(token.0),
            };
            self.events.push_back(DriverEvent::FrameReady(Frame {
                handle,
                device: self.camera,
                width: frame.width,
                height: frame.height,
                pixel_format: frame.pixel_format.clone(),
                data: frame.data,
                metadata: BTreeMap::from([
                    (
                        "source".into(),
                        Value::String("abs-camera-vendor-runtime".into()),
                    ),
                    (
                        "runtime_backend".into(),
                        Value::String(
                            "CamUSB_InitCameraExS/SetCaptureMode/TriggerImage/GetImage/ReleaseImage".into(),
                        ),
                    ),
                    (
                        "native_pixel_type".into(),
                        Value::String(format!("0x{:08x}", frame.native_pixel_type)),
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
                ("pixel_format".into(), Value::String(frame.pixel_format)),
                ("stream".into(), Value::I64(handle.stream.0 as i64)),
                ("frame".into(), Value::I64(handle.frame.0 as i64)),
                (
                    "source".into(),
                    Value::String("abs-camera-vendor-runtime".into()),
                ),
            ])))
        }
    }

    fn stream_frames(&mut self, token: DriverToken, request: CameraStreamRequest) -> Result<Value> {
        let frame_count = request.frame_count.unwrap_or(8);
        if frame_count == 0 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "ABS CameraStream frame_count must be positive",
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

impl Driver for AbsCameraDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        vec![self.descriptor()]
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.transport,
            driver: self.id,
            label: "ABS camera transport candidate".into(),
            kind: "unknown.camera.transport".into(),
            metadata: BTreeMap::from([
                (
                    "transport_hint".into(),
                    Value::String(self.probe.transport_hint.clone()),
                ),
                (
                    "runtime_path".into(),
                    Value::String(self.probe.vendor_runtime_path.clone().unwrap_or_default()),
                ),
                (
                    "runtime_sha256".into(),
                    Value::String(self.probe.vendor_runtime_sha256.clone().unwrap_or_default()),
                ),
                (
                    "configured".into(),
                    Value::Bool(self.vendor_runtime_configured()),
                ),
                (
                    "backend_enabled".into(),
                    Value::Bool(self.probe.load_vendor_runtime),
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
                    "license_scope".into(),
                    Value::String("third-party excluded data".into()),
                ),
                (
                    "binding_gate".into(),
                    Value::String(self.package_gate().into()),
                ),
            ]),
        }]
    }

    fn capabilities(&self, _device: DeviceId) -> Vec<CapabilityDescriptor> {
        if _device == self.camera {
            return vec![
                capability(1, self.camera, CapabilityKind::CameraCapture),
                capability(2, self.camera, CapabilityKind::CameraStream),
            ];
        }
        Vec::new()
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } if *device == self.camera => {
                    let _ = self.read_property(key)?;
                }
                Command::WriteProperty { device, key, value } if *device == self.camera => {
                    self.validate_write_property(key, value)?;
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
                } if *device == self.camera && *capability == CapabilityId(2) => {
                    if !matches!(request, CapabilityRequest::CameraStream(_)) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "CameraStream expects CameraStreamRequest",
                        ));
                    }
                }
                Command::Invoke { device, .. } if *device == self.camera => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "unsupported ABS camera capability",
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
                Command::ReadProperty { device, key } if device == self.camera => {
                    result = self.read_property(&key)?;
                }
                Command::WriteProperty { device, key, value } if device == self.camera => {
                    result = self.write_property(key, value)?;
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
                } if device == self.camera && capability == CapabilityId(2) => {
                    result = self.stream_frames(token, request)?;
                }
                Command::Invoke { device, .. } if device == self.camera => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "unsupported ABS camera capability",
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
        _ => fallback,
    }
}

fn bool_prop(device: &DeviceConfig, key: &str) -> Option<bool> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn pixel_count_prop(device: &DeviceConfig, key: &str) -> Result<Option<u32>> {
    match device.properties.get(key) {
        Some(Value::PixelCount(value)) => Ok(Some(value.pixels())),
        Some(Value::I64(value)) if (0..=u32::MAX as i64).contains(value) => Ok(Some(*value as u32)),
        Some(Value::I64(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("ABS camera property {key} must fit in an unsigned pixel count"),
        )),
        _ => Ok(None),
    }
}

fn time_interval_prop(device: &DeviceConfig, key: &str) -> Result<Option<TimeInterval>> {
    match device.properties.get(key) {
        Some(Value::TimeInterval(value)) => Ok(Some(*value)),
        Some(Value::I64(value)) if *value >= 0 => {
            Ok(Some(TimeInterval::from_milliseconds(*value as f64)))
        }
        Some(Value::F64(value)) if *value >= 0.0 => Ok(Some(TimeInterval::from_seconds(*value))),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("ABS camera property {key} must be a non-negative TimeInterval"),
        )),
        None => Ok(None),
    }
}

fn pixel_format_prop(device: &DeviceConfig, key: &str) -> Result<Option<String>> {
    match device.properties.get(key) {
        Some(Value::String(value)) => match value.as_str() {
            "Native" | "Mono8" | "Mono16" | "Raw8" | "Raw16" | "Rgb8" | "Bgr8" => {
                Ok(Some(value.clone()))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!(
                    "ABS camera property {key} must be Native, Mono8, Mono16, Raw8, Raw16, Rgb8, or Bgr8"
                ),
            )),
        },
        _ => Ok(None),
    }
}

#[cfg(feature = "os-usb")]
fn encoding_matches(requested: &ImageEncoding, actual: &str) -> bool {
    match requested {
        ImageEncoding::Native => true,
        ImageEncoding::Raw8 => actual == ImageEncoding::Mono8.property_value(),
        ImageEncoding::Raw16 => actual == ImageEncoding::Mono16.property_value(),
        _ => actual == requested.property_value(),
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

fn validate_positive_exposure(exposure: TimeInterval) -> Result<()> {
    let micros = exposure.seconds() * 1_000_000.0;
    if !micros.is_finite() || micros <= 0.0 || micros > u32::MAX as f64 {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "ABS camera exposure must be a finite positive interval",
        ));
    }
    Ok(())
}

fn capability(id: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    let name = kind.name().to_string();
    CapabilityDescriptor {
        id: CapabilityId(id),
        device,
        kind,
        name,
        request: ValueType::Map,
        response: ValueType::Map,
    }
}

#[cfg(feature = "os-usb")]
mod live_abs_camera {
    use super::*;
    use libloading::Library;
    use std::ptr;

    const DEVICE_NUMBER: u8 = 0;
    const NO_SERIAL_NUMBER: u32 = 0xffff_ffff;
    const CPID_NONE: u32 = 0;
    const FWOPT_AUTOMATIC: u32 = 0;
    const MODE_ASYNC_TRIGGER: u8 = 0x80;
    const ASYNC_TRIGGER_DEVICE_COUNT: u8 = 1;
    const PIX_OCCUPY_MASK: u32 = 0x00ff_0000;
    const PIX_OCCUPY8BIT: u32 = 0x0008_0000;
    const PIX_OCCUPY16BIT: u32 = 0x0010_0000;
    const PIX_OCCUPY24BIT: u32 = 0x0018_0000;
    const PIX_OCCUPY32BIT: u32 = 0x0020_0000;
    const PIX_OCCUPY48BIT: u32 = 0x0030_0000;
    const PIX_OCCUPY64BIT: u32 = 0x0040_0000;
    const PIX_RGB: u32 = 0x0200_0000;
    const PIX_MONO: u32 = 0x0100_0000;
    const PIX_MONO8: u32 = PIX_MONO | PIX_OCCUPY8BIT | 0x0001;
    const PIX_MONO10: u32 = PIX_MONO | PIX_OCCUPY16BIT | 0x0003;
    const PIX_MONO12: u32 = PIX_MONO | PIX_OCCUPY16BIT | 0x0005;
    const PIX_MONO14: u32 = PIX_MONO | PIX_OCCUPY16BIT | 0x0220;
    const PIX_MONO16: u32 = PIX_MONO | PIX_OCCUPY16BIT | 0x0007;
    const PIX_RGB8_PACKED: u32 = PIX_RGB | PIX_OCCUPY24BIT | 0x0014;
    const PIX_BGR8_PACKED: u32 = PIX_RGB | PIX_OCCUPY24BIT | 0x0015;

    #[repr(C, packed)]
    struct CameraInit {
        reserved0: [u8; 3],
        device_number: u8,
        serial_number: u32,
        platform_id: u32,
        firmware_options: u32,
        firmware: *mut u8,
        firmware_size: u32,
        reserved1: [u32; 5],
    }

    #[repr(C, packed)]
    struct ImageHeader {
        status: u16,
        block_id: u16,
        packet_format: u8,
        packet_id_high: u8,
        packet_id_low: u16,
        payload_ext: u16,
        payload_type: u16,
        timestamp_high: u32,
        timestamp_low: u32,
        pixel_type: u32,
        size_x: u32,
        size_y: u32,
        offset_x: i32,
        offset_y: i32,
    }

    #[repr(C, packed)]
    struct AsyncTriggerDevice {
        device: u32,
        last_error: i32,
    }

    pub(super) struct RuntimeFrame {
        pub width: u32,
        pub height: u32,
        pub pixel_format: String,
        pub native_pixel_type: u32,
        pub data: Vec<u8>,
    }

    struct Api {
        _library: Library,
        init_camera: unsafe extern "C" fn(*mut CameraInit) -> i32,
        free_camera: unsafe extern "C" fn(u8) -> i32,
        get_image:
            unsafe extern "C" fn(*mut *mut u8, *mut *mut ImageHeader, u32, u8, u32, u32) -> i32,
        release_image: unsafe extern "C" fn(*mut u8, *mut ImageHeader, u8) -> i32,
        abort_get_image: unsafe extern "C" fn(u8) -> i32,
        trigger_image: unsafe extern "C" fn(*mut AsyncTriggerDevice, u8, u32) -> i32,
        set_capture_mode: unsafe extern "C" fn(u8, u8, u8, u16, *mut u8) -> i32,
        set_exposure: unsafe extern "C" fn(*mut u32, u8) -> i32,
        get_last_error: unsafe extern "C" fn(u8) -> u32,
    }

    impl Api {
        fn load(path: &str) -> Result<Self> {
            let library = unsafe { Library::new(path) }.map_err(|error| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("ABS camera runtime load failed: {error}"),
                )
            })?;
            Ok(Self {
                init_camera: symbol(&library, "CamUSB_InitCameraExS")?,
                free_camera: symbol(&library, "CamUSB_FreeCamera")?,
                get_image: symbol(&library, "CamUSB_GetImage")?,
                release_image: symbol(&library, "CamUSB_ReleaseImage")?,
                abort_get_image: symbol(&library, "CamUSB_AbortGetImage")?,
                trigger_image: symbol(&library, "CamUSB_TriggerImage")?,
                set_capture_mode: symbol(&library, "CamUSB_SetCaptureMode")?,
                set_exposure: symbol(&library, "CamUSB_SetExposureTime")?,
                get_last_error: symbol(&library, "CamUSB_GetLastError")?,
                _library: library,
            })
        }
    }

    pub(super) fn capture(path: &str, exposure: TimeInterval) -> Result<RuntimeFrame> {
        let api = Api::load(path)?;
        let mut init = CameraInit {
            reserved0: [0; 3],
            device_number: DEVICE_NUMBER,
            serial_number: NO_SERIAL_NUMBER,
            platform_id: CPID_NONE,
            firmware_options: FWOPT_AUTOMATIC,
            firmware: ptr::null_mut(),
            firmware_size: 0,
            reserved1: [0; 5],
        };
        check_bool(
            &api,
            unsafe { (api.init_camera)(&mut init) },
            init.device_number,
            "initialize camera",
        )?;
        let device_number = init.device_number;
        let _camera_guard = CameraGuard {
            api: &api,
            device_number,
        };

        check_bool(
            &api,
            unsafe {
                (api.set_capture_mode)(
                    MODE_ASYNC_TRIGGER,
                    ASYNC_TRIGGER_DEVICE_COUNT,
                    device_number,
                    0,
                    ptr::null_mut(),
                )
            },
            device_number,
            "set triggered single-image mode",
        )?;
        let mut exposure_us = exposure_us(exposure)?;
        check_bool(
            &api,
            unsafe { (api.set_exposure)(&mut exposure_us, device_number) },
            device_number,
            "set exposure",
        )?;

        let timeout_ms = capture_timeout_ms(exposure);
        let mut trigger_device = AsyncTriggerDevice {
            device: device_number as u32,
            last_error: 0,
        };
        check_bool(
            &api,
            unsafe {
                (api.trigger_image)(&mut trigger_device, ASYNC_TRIGGER_DEVICE_COUNT, timeout_ms)
            },
            device_number,
            "trigger image",
        )?;

        let mut image_ptr: *mut u8 = ptr::null_mut();
        let mut header_ptr: *mut ImageHeader = ptr::null_mut();
        check_bool(
            &api,
            unsafe {
                (api.get_image)(
                    &mut image_ptr,
                    &mut header_ptr,
                    0,
                    device_number,
                    timeout_ms,
                    0,
                )
            },
            device_number,
            "get image",
        )
        .map_err(|error| {
            let _ = unsafe { (api.abort_get_image)(device_number) };
            error
        })?;
        let _image_guard = ImageGuard {
            api: &api,
            device_number,
            image_ptr,
            header_ptr,
        };
        if image_ptr.is_null() || header_ptr.is_null() {
            return Err(Error::new(
                ErrorCode::Transport,
                "ABS camera runtime returned a null image pointer",
            ));
        }
        let width = unsafe { ptr::addr_of!((*header_ptr).size_x).read_unaligned() };
        let height = unsafe { ptr::addr_of!((*header_ptr).size_y).read_unaligned() };
        let pixel_type = unsafe { ptr::addr_of!((*header_ptr).pixel_type).read_unaligned() };
        if width == 0 || height == 0 {
            return Err(Error::new(
                ErrorCode::Transport,
                "ABS camera runtime returned empty frame dimensions",
            ));
        }
        let pixel_format = pixel_format(pixel_type)?;
        let bytes_per_pixel = bytes_per_pixel(pixel_type)?;
        let byte_len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(bytes_per_pixel))
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::Transport,
                    "ABS camera runtime returned an oversized image",
                )
            })?;
        let data = unsafe { std::slice::from_raw_parts(image_ptr, byte_len) }.to_vec();
        Ok(RuntimeFrame {
            width,
            height,
            pixel_format,
            native_pixel_type: pixel_type,
            data,
        })
    }

    fn pixel_format(pixel_type: u32) -> Result<String> {
        match pixel_type {
            PIX_MONO8 => Ok(ImageEncoding::Mono8.property_value().into()),
            PIX_MONO10 | PIX_MONO12 | PIX_MONO14 | PIX_MONO16 => {
                Ok(ImageEncoding::Mono16.property_value().into())
            }
            PIX_RGB8_PACKED => Ok(ImageEncoding::Rgb8.property_value().into()),
            PIX_BGR8_PACKED => Ok(ImageEncoding::Bgr8.property_value().into()),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                format!("ABS camera returned unsupported pixel type 0x{pixel_type:08x}"),
            )),
        }
    }

    fn bytes_per_pixel(pixel_type: u32) -> Result<usize> {
        match pixel_type & PIX_OCCUPY_MASK {
            PIX_OCCUPY8BIT => Ok(1),
            PIX_OCCUPY16BIT => Ok(2),
            PIX_OCCUPY24BIT => Ok(3),
            PIX_OCCUPY32BIT => Ok(4),
            PIX_OCCUPY48BIT => Ok(6),
            PIX_OCCUPY64BIT => Ok(8),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                format!("ABS camera returned unsupported packed pixel size 0x{pixel_type:08x}"),
            )),
        }
    }

    fn exposure_us(exposure: TimeInterval) -> Result<u32> {
        let micros = exposure.seconds() * 1_000_000.0;
        if !micros.is_finite() || micros <= 0.0 || micros > u32::MAX as f64 {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "ABS camera exposure must be a finite positive interval",
            ));
        }
        Ok(micros.round() as u32)
    }

    fn capture_timeout_ms(exposure: TimeInterval) -> u32 {
        let millis = exposure.seconds() * 1000.0;
        if millis.is_finite() && millis > 0.0 {
            millis
                .mul_add(2.0, 2500.0)
                .round()
                .clamp(2500.0, u32::MAX as f64) as u32
        } else {
            2500
        }
    }

    fn check_bool(api: &Api, value: i32, device_number: u8, operation: &str) -> Result<()> {
        if value != 0 {
            Ok(())
        } else {
            let error = unsafe { (api.get_last_error)(device_number) };
            Err(Error::new(
                ErrorCode::Transport,
                format!("ABS camera {operation} failed with CamUSB error {error}"),
            ))
        }
    }

    fn symbol<T: Copy>(library: &Library, name: &str) -> Result<T> {
        unsafe { library.get::<T>(name.as_bytes()) }
            .map(|symbol| *symbol)
            .map_err(|_| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("ABS camera runtime is missing required symbol {name}"),
                )
            })
    }

    struct CameraGuard<'a> {
        api: &'a Api,
        device_number: u8,
    }

    impl Drop for CameraGuard<'_> {
        fn drop(&mut self) {
            let _ = unsafe { (self.api.free_camera)(self.device_number) };
        }
    }

    struct ImageGuard<'a> {
        api: &'a Api,
        device_number: u8,
        image_ptr: *mut u8,
        header_ptr: *mut ImageHeader,
    }

    impl Drop for ImageGuard<'_> {
        fn drop(&mut self) {
            if !self.image_ptr.is_null() && !self.header_ptr.is_null() {
                let _ = unsafe {
                    (self.api.release_image)(self.image_ptr, self.header_ptr, self.device_number)
                };
            }
        }
    }
}
