use libloading::Library;
use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};
use std::io::{BufReader, Read};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct MightexCameraProbe {
    label: String,
    product: String,
    serial_number: Option<String>,
    endpoint_hint: String,
    width: u32,
    height: u32,
    bit_depth: u16,
    pixel_format: String,
    exposure: TimeInterval,
    vendor_runtime_path: Option<String>,
    vendor_runtime_sha256: Option<String>,
    load_vendor_runtime: bool,
}

pub struct MightexCameraDiscovery {
    next_id: DriverId,
    probes: Vec<MightexCameraProbe>,
}

impl MightexCameraDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![MightexCameraProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| {
                matches!(
                    device.driver.as_str(),
                    "mightex_camera" | "mightex-camera" | "mightex-cam"
                )
            })
            .map(MightexCameraProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for MightexCameraDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .iter()
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                Ok(DriverCandidate::from_driver(
                    format!("{} ({})", probe.label, probe.product),
                    Box::new(MightexCameraDriver::configured(id, probe.clone())),
                ))
            })
            .collect()
    }
}

impl MightexCameraProbe {
    fn fixture() -> Self {
        Self {
            label: "Configured Mightex camera reverse engineered support".into(),
            product: "Mightex buffered USB camera".into(),
            serial_number: None,
            endpoint_hint: "bulk endpoint evidence exists; frame layout unknown".into(),
            width: 1280,
            height: 960,
            bit_depth: 12,
            pixel_format: "Mono16".into(),
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
        probe.endpoint_hint = string_prop(device, "endpoint_hint").unwrap_or(probe.endpoint_hint);
        probe.width = pixel_count_prop(device, "width")?.unwrap_or(probe.width);
        probe.height = pixel_count_prop(device, "height")?.unwrap_or(probe.height);
        probe.bit_depth = u16_prop(device, "bit_depth")?.unwrap_or(probe.bit_depth);
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

pub struct MightexCameraDriver {
    id: DriverId,
    camera: DeviceId,
    control: ResourceId,
    stream: ResourceId,
    probe: MightexCameraProbe,
    next_token: u64,
    events: VecDeque<DriverEvent>,
}

impl MightexCameraDriver {
    pub fn configured(id: DriverId, probe: MightexCameraProbe) -> Self {
        Self {
            id,
            camera: DeviceId(NodeId(id.0 * 1000 + 720)),
            control: ResourceId(NodeId(id.0 * 1000 + 721)),
            stream: ResourceId(NodeId(id.0 * 1000 + 722)),
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
            vendor: Some("Mightex".into()),
            model: Some(self.probe.product.clone()),
            serial: self.probe.serial_number.clone(),
            kinds: vec!["camera".into(), "reverse.engineered".into()],
            properties: vec![
                string_property("model", "Model"),
                string_property("serial_number", "Serial number"),
                string_property("support_level", "Support level"),
                string_property("endpoint_hint", "Endpoint hint"),
                writable_property("width", "Width", ValueType::PixelCount),
                writable_property("height", "Height", ValueType::PixelCount),
                writable_property("bit_depth", "Bit depth", ValueType::I64),
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
                format!("Mightex camera vendor runtime file is unavailable: {error}"),
            )
        })?;
        if !metadata.is_file() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Mightex camera vendor runtime path is not a regular file",
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
                    "Mightex camera vendor runtime file is unavailable for digest verification: {error}"
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
                    format!("Mightex camera vendor runtime digest read failed: {error}"),
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

        // Loading is the explicit vendor-runtime boundary. No Mightex camera
        // ABI or hardware operation is invoked by this read-only probe.
        match unsafe { Library::new(path) } {
            Ok(_library) => "loaded".into(),
            Err(error) => format!("load_error:{}", compact_error(&error.to_string())),
        }
    }

    fn vendor_runtime_expected_symbols(&self) -> &'static [&'static str] {
        &[
            "BUFCCDUSB_InitDevice",
            "BUFCCDUSB_UnInitDevice",
            "BUFCCDUSB_GetModuleNoSerialNo",
            "BUFCCDUSB_AddDeviceToWorkingSet",
            "BUFCCDUSB_ActiveDeviceInWorkingSet",
            "BUFCCDUSB_StartCameraEngine",
            "BUFCCDUSB_StopCameraEngine",
            "BUFCCDUSB_SetCameraWorkMode",
            "BUFCCDUSB_StartFrameGrab",
            "BUFCCDUSB_StopFrameGrab",
            "BUFCCDUSB_SetCustomizedResolution",
            "BUFCCDUSB_SetExposureTime",
            "BUFCCDUSB_InstallFrameHooker",
            "BUFCCDUSB_InstallUSBDeviceHooker",
            "BUFCCDUSB_SetSoftTrigger",
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
        "runtime package identity and explicit loadability/symbol probes are exposed; one-shot capture and repeated one-shot stream use the verified vendor runtime; native frame transport is not exposed because protocol evidence is absent"
    }

    fn read_property(&self, key: &str) -> Result<Value> {
        match key {
            "model" => Ok(Value::String(self.probe.product.clone())),
            "serial_number" => Ok(Value::String(
                self.probe.serial_number.clone().unwrap_or_default(),
            )),
            "support_level" => Ok(Value::String(
                "runtime-package evidence with file-status/digest/loadability/ABI-symbol checks, writable capture parameters, opt-in vendor-runtime capture, and repeated one-shot stream support; SDK-free/native controls and native continuous streaming is not exposed because protocol evidence is absent"
                    .into(),
            )),
            "endpoint_hint" => Ok(Value::String(self.probe.endpoint_hint.clone())),
            "width" => Ok(Value::PixelCount(PixelCount::new(self.probe.width))),
            "height" => Ok(Value::PixelCount(PixelCount::new(self.probe.height))),
            "bit_depth" => Ok(Value::I64(self.probe.bit_depth as i64)),
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
                "configured Mightex camera vendor firmware/runtime packages are third-party excluded data"
                    .into(),
            )),
            "capture_gate" => Ok(Value::String(
                "CameraCapture uses the verified vendor runtime; native frame transport is not exposed because frame/completion evidence is absent".into(),
            )),
            "stream_gate" => Ok(Value::String(
                "CameraStream uses repeated verified vendor-runtime one-shot captures; native continuous streaming waits for dropped-frame and buffer ownership evidence".into(),
            )),
            "control_gate" => Ok(Value::String(
                "runtime capture applies exposure, resolution, trigger mode, and software trigger through the verified vendor runtime; native gain/color controls and SDK-free control writes are not exposed because control-packet evidence is absent".into(),
            )),
            "feature_summary" => Ok(Value::Map(BTreeMap::from([
                ("bulk_endpoint_evidence".into(), Value::Bool(true)),
                ("frame_layout_known".into(), Value::Bool(true)),
                ("capture_supported".into(), Value::Bool(true)),
                ("stream_supported".into(), Value::Bool(true)),
            ]))),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                "unknown Mightex camera property",
            )),
        }
    }

    fn validate_write_property(&self, key: &str, value: &Value) -> Result<()> {
        match key {
            "width" | "height" => {
                let Value::PixelCount(count) = value else {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        format!("Mightex camera {key} expects PixelCount"),
                    ));
                };
                if count.pixels() == 0 {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        format!("Mightex camera {key} must be positive"),
                    ));
                }
                Ok(())
            }
            "bit_depth" => {
                let Value::I64(bits) = value else {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Mightex camera bit_depth expects I64",
                    ));
                };
                if !(9..=16).contains(bits) {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Mightex camera bit_depth must be in 9..=16 for raw >8-bit capture",
                    ));
                }
                Ok(())
            }
            "exposure" => {
                let Value::TimeInterval(exposure) = value else {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Mightex camera exposure expects TimeInterval",
                    ));
                };
                validate_exposure(*exposure)
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "Mightex camera exposes only the evidenced writable capture properties",
            )),
        }
    }

    fn write_property(&mut self, key: String, value: Value) -> Result<Value> {
        self.validate_write_property(&key, &value)?;
        match (key.as_str(), value) {
            ("width", Value::PixelCount(width)) => {
                self.probe.width = width.pixels();
                Ok(Value::PixelCount(width))
            }
            ("height", Value::PixelCount(height)) => {
                self.probe.height = height.pixels();
                Ok(Value::PixelCount(height))
            }
            ("bit_depth", Value::I64(bits)) => {
                self.probe.bit_depth = bits as u16;
                Ok(Value::I64(bits))
            }
            ("exposure", Value::TimeInterval(exposure)) => {
                self.probe.exposure = exposure;
                Ok(Value::TimeInterval(exposure))
            }
            _ => unreachable!("Mightex camera write validation constrains writable properties"),
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
                "Mightex camera capture requires load_vendor_runtime=true",
            ));
        }
        let digest_state = self.vendor_runtime_digest_allows_use();
        if digest_state != "verified" {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Mightex camera vendor runtime is not verified: {digest_state}"),
            ));
        }
        let encoding = request.encoding.unwrap_or(ImageEncoding::Native);
        let pixel_format = match encoding {
            ImageEncoding::Native | ImageEncoding::Mono16 => "Mono16",
            ImageEncoding::Raw16 => "Raw16",
            _ => {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    "Mightex camera capture supports Native, Mono16, or Raw16 through the documented raw callback mode",
                ))
            }
        };
        if self.probe.bit_depth <= 8 {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Mightex camera vendor-runtime capture currently requires the raw >8-bit callback mode",
            ));
        }
        let path = self.probe.vendor_runtime_path.as_deref().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Mightex camera runtime path is required",
            )
        })?;
        #[cfg(feature = "os-usb")]
        let frame = live_mightex_camera::capture(
            path,
            self.probe.width,
            self.probe.height,
            self.probe.bit_depth,
            self.probe.exposure,
        )?;
        #[cfg(not(feature = "os-usb"))]
        {
            let _ = (path, token, pixel_format);
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Mightex camera capture requires numanager-drivers/os-usb",
            ));
        }
        #[cfg(feature = "os-usb")]
        {
            self.probe.width = frame.width;
            self.probe.height = frame.height;
            self.probe.bit_depth = frame.bit_depth;
            self.probe.pixel_format = pixel_format.into();
            if let Some(serial) = frame.serial_number {
                self.probe.serial_number = Some(serial);
            }
            let handle = FrameHandle {
                stream: StreamId(self.camera.0 .0),
                frame: FrameId(token.0),
            };
            self.events.push_back(DriverEvent::FrameReady(Frame {
                handle,
                device: self.camera,
                width: frame.width,
                height: frame.height,
                pixel_format: pixel_format.into(),
                data: frame.data,
                metadata: BTreeMap::from([
                    (
                        "source".into(),
                        Value::String("mightex-camera-vendor-runtime".into()),
                    ),
                    (
                        "runtime_backend".into(),
                        Value::String(
                            "BUFCCDUSB_InitDevice/StartFrameGrab/SetSoftTrigger callback".into(),
                        ),
                    ),
                    ("module_number".into(), Value::String(frame.module_number)),
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
                ("pixel_format".into(), Value::String(pixel_format.into())),
                ("stream".into(), Value::I64(handle.stream.0 as i64)),
                ("frame".into(), Value::I64(handle.frame.0 as i64)),
                (
                    "source".into(),
                    Value::String("mightex-camera-vendor-runtime".into()),
                ),
            ])))
        }
    }

    fn stream_frames(&mut self, token: DriverToken, request: CameraStreamRequest) -> Result<Value> {
        let frame_count = request.frame_count.unwrap_or(8);
        if frame_count == 0 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Mightex camera CameraStream frame_count must be positive",
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

impl Driver for MightexCameraDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        vec![self.descriptor()]
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![
            ResourceDescriptor {
                id: self.control,
                driver: self.id,
                label: "Mightex camera control candidate".into(),
                kind: "usb.vendor.control".into(),
                metadata: BTreeMap::from([
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
            },
            ResourceDescriptor {
                id: self.stream,
                driver: self.id,
                label: "Mightex camera stream candidate".into(),
                kind: "usb.bulk.stream".into(),
                metadata: BTreeMap::from([
                    (
                        "endpoint_hint".into(),
                        Value::String(self.probe.endpoint_hint.clone()),
                    ),
                    (
                        "runtime_path".into(),
                        Value::String(self.probe.vendor_runtime_path.clone().unwrap_or_default()),
                    ),
                    (
                        "package_state".into(),
                        Value::String(self.vendor_runtime_state().into()),
                    ),
                    (
                        "runtime_digest_state".into(),
                        Value::String(self.vendor_runtime_digest_state()),
                    ),
                ]),
            },
        ]
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
                        "unsupported Mightex camera capability",
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
                        "unsupported Mightex camera capability",
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

fn u16_prop(device: &DeviceConfig, key: &str) -> Result<Option<u16>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if (0..=u16::MAX as i64).contains(value) => Ok(Some(*value as u16)),
        Some(Value::I64(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Mightex camera property {key} must fit in an unsigned 16-bit integer"),
        )),
        Some(Value::String(value)) => value.parse().map(Some).map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("Mightex camera property {key} must be an unsigned 16-bit integer"),
            )
        }),
        _ => Ok(None),
    }
}

fn pixel_count_prop(device: &DeviceConfig, key: &str) -> Result<Option<u32>> {
    match device.properties.get(key) {
        Some(Value::PixelCount(value)) => Ok(Some(value.pixels())),
        Some(Value::I64(value)) if (1..=u32::MAX as i64).contains(value) => Ok(Some(*value as u32)),
        Some(Value::I64(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Mightex camera property {key} must fit in a positive pixel count"),
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
            format!("Mightex camera property {key} must be a non-negative TimeInterval"),
        )),
        None => Ok(None),
    }
}

fn pixel_format_prop(device: &DeviceConfig, key: &str) -> Result<Option<String>> {
    match device.properties.get(key) {
        Some(Value::String(value)) => match value.as_str() {
            "Native" | "Mono8" | "Mono16" | "Raw8" | "Raw16" => Ok(Some(value.clone())),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!(
                    "Mightex camera property {key} must be Native, Mono8, Mono16, Raw8, or Raw16"
                ),
            )),
        },
        _ => Ok(None),
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

fn writable_property(key: &str, display_name: &str, value_type: ValueType) -> PropertySchema {
    let mut schema = property(key, display_name, value_type);
    schema.writable = true;
    schema
}

fn validate_exposure(exposure: TimeInterval) -> Result<()> {
    let ticks = exposure.seconds() * 1_000_000.0 / 50.0;
    if !ticks.is_finite() || ticks < 0.0 || ticks > i32::MAX as f64 {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "Mightex camera exposure must be a finite non-negative interval",
        ));
    }
    Ok(())
}

fn string_property(key: &str, display_name: &str) -> PropertySchema {
    property(key, display_name, ValueType::String)
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
mod live_mightex_camera {
    use super::*;
    use libloading::Library;
    use std::ffi::CStr;
    use std::os::raw::{c_char, c_void};
    use std::sync::mpsc::{self, Sender};
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    const DEVICE_ID: i32 = 1;
    const GRAB_FRAME_FOREVER: i32 = 0x8888;
    const TRIGGER_MODE: i32 = 1;
    const CAMERA_BIT_OPTION_12: i32 = 12;
    const MAX_NAME: usize = 32;

    #[repr(C, packed)]
    struct ProcessedDataProperty {
        camera_id: i32,
        row: i32,
        column: i32,
        bin: i32,
        x_start: i32,
        y_start: i32,
        exposure_time: i32,
        red_gain: i32,
        green_gain: i32,
        blue_gain: i32,
        time_stamp: i32,
        trigger_occurred: i32,
        trigger_event_count: i32,
        user_mark: i32,
        frame_time: i32,
        ccd_frequency: i32,
        frame_process_type: i32,
        t_filter_accept_for_file: i32,
    }

    pub(super) struct RuntimeFrame {
        pub width: u32,
        pub height: u32,
        pub bit_depth: u16,
        pub module_number: String,
        pub serial_number: Option<String>,
        pub data: Vec<u8>,
    }

    struct CallbackFrame {
        width: u32,
        height: u32,
        data: Vec<u8>,
    }

    static FRAME_SENDER: OnceLock<Mutex<Option<Sender<CallbackFrame>>>> = OnceLock::new();

    extern "system" fn frame_callback(attributes: *mut ProcessedDataProperty, byte_ptr: *mut u8) {
        if attributes.is_null() || byte_ptr.is_null() {
            return;
        }
        let column = unsafe { std::ptr::addr_of!((*attributes).column).read_unaligned() };
        let row = unsafe { std::ptr::addr_of!((*attributes).row).read_unaligned() };
        let width = column.max(0) as u32;
        let height = row.max(0) as u32;
        if width == 0 || height == 0 {
            return;
        }
        let bytes_per_pixel = 2;
        let Some(byte_len) = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(bytes_per_pixel))
        else {
            return;
        };
        let data = unsafe { std::slice::from_raw_parts(byte_ptr, byte_len) }.to_vec();
        let frame = CallbackFrame {
            width,
            height,
            data,
        };
        if let Some(lock) = FRAME_SENDER.get() {
            if let Ok(guard) = lock.lock() {
                if let Some(sender) = guard.as_ref() {
                    let _ = sender.send(frame);
                }
            }
        }
    }

    type FrameCallback = extern "system" fn(*mut ProcessedDataProperty, *mut u8);
    type FaultCallback = extern "system" fn(i32);

    struct Api {
        _library: Library,
        init: unsafe extern "system" fn() -> i32,
        uninit: unsafe extern "system" fn(),
        get_module_serial: unsafe extern "system" fn(i32, *mut c_char, *mut c_char) -> i32,
        add_working: unsafe extern "system" fn(i32) -> i32,
        active_working: unsafe extern "system" fn(i32, i32) -> i32,
        start_engine: unsafe extern "system" fn(*mut c_void, i32) -> i32,
        stop_engine: unsafe extern "system" fn() -> i32,
        set_work_mode: unsafe extern "system" fn(i32, i32) -> i32,
        start_grab: unsafe extern "system" fn(i32) -> i32,
        stop_grab: unsafe extern "system" fn() -> i32,
        set_resolution: unsafe extern "system" fn(i32, i32, i32, i32, i32) -> i32,
        set_exposure: unsafe extern "system" fn(i32, i32) -> i32,
        install_frame_hooker: unsafe extern "system" fn(i32, FrameCallback) -> i32,
        install_fault_hooker: unsafe extern "system" fn(FaultCallback),
        set_soft_trigger: unsafe extern "system" fn(i32) -> i32,
    }

    impl Api {
        fn load(path: &str) -> Result<Self> {
            let library = unsafe { Library::new(path) }.map_err(|error| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Mightex camera runtime load failed: {error}"),
                )
            })?;
            Ok(Self {
                init: symbol(&library, "BUFCCDUSB_InitDevice")?,
                uninit: symbol(&library, "BUFCCDUSB_UnInitDevice")?,
                get_module_serial: symbol(&library, "BUFCCDUSB_GetModuleNoSerialNo")?,
                add_working: symbol(&library, "BUFCCDUSB_AddDeviceToWorkingSet")?,
                active_working: symbol(&library, "BUFCCDUSB_ActiveDeviceInWorkingSet")?,
                start_engine: symbol(&library, "BUFCCDUSB_StartCameraEngine")?,
                stop_engine: symbol(&library, "BUFCCDUSB_StopCameraEngine")?,
                set_work_mode: symbol(&library, "BUFCCDUSB_SetCameraWorkMode")?,
                start_grab: symbol(&library, "BUFCCDUSB_StartFrameGrab")?,
                stop_grab: symbol(&library, "BUFCCDUSB_StopFrameGrab")?,
                set_resolution: symbol(&library, "BUFCCDUSB_SetCustomizedResolution")?,
                set_exposure: symbol(&library, "BUFCCDUSB_SetExposureTime")?,
                install_frame_hooker: symbol(&library, "BUFCCDUSB_InstallFrameHooker")?,
                install_fault_hooker: symbol(&library, "BUFCCDUSB_InstallUSBDeviceHooker")?,
                set_soft_trigger: symbol(&library, "BUFCCDUSB_SetSoftTrigger")?,
                _library: library,
            })
        }
    }

    pub(super) fn capture(
        path: &str,
        width: u32,
        height: u32,
        bit_depth: u16,
        exposure: TimeInterval,
    ) -> Result<RuntimeFrame> {
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Mightex camera capture requires configured width/height in 1..=i32::MAX",
            ));
        }
        let api = Api::load(path)?;
        let count = unsafe { (api.init)() };
        if count <= 0 {
            return Err(Error::new(
                ErrorCode::Transport,
                "Mightex camera runtime reported no devices",
            ));
        }
        let _init_guard = InitGuard { api: &api };

        let (module_number, serial_number) = read_identity(&api);
        check_positive(
            unsafe { (api.add_working)(DEVICE_ID) },
            "add device to working set",
        )?;
        check_positive(
            unsafe { (api.active_working)(DEVICE_ID, 1) },
            "activate device in working set",
        )?;
        check_positive(
            unsafe { (api.start_engine)(std::ptr::null_mut(), CAMERA_BIT_OPTION_12) },
            "start camera engine",
        )?;
        let _engine_guard = EngineGuard { api: &api };

        check_positive(
            unsafe { (api.install_frame_hooker)(0, frame_callback) },
            "install frame callback",
        )?;
        unsafe { (api.install_fault_hooker)(fault_callback) };
        check_positive(
            unsafe { (api.set_work_mode)(DEVICE_ID, TRIGGER_MODE) },
            "set trigger mode",
        )?;
        check_positive(
            unsafe {
                (api.set_resolution)(
                    DEVICE_ID,
                    width as i32,
                    height as i32,
                    0,
                    camera_buffer_count(width, height),
                )
            },
            "set resolution",
        )?;
        check_positive(
            unsafe { (api.set_exposure)(DEVICE_ID, exposure_ticks(exposure)?) },
            "set exposure",
        )?;

        let (sender, receiver) = mpsc::channel();
        let lock = FRAME_SENDER.get_or_init(|| Mutex::new(None));
        {
            let mut guard = lock.lock().map_err(|_| {
                Error::new(ErrorCode::Transport, "Mightex camera callback lock failed")
            })?;
            *guard = Some(sender);
        }
        let _callback_guard = CallbackGuard { lock };

        check_positive(
            unsafe { (api.start_grab)(GRAB_FRAME_FOREVER) },
            "start frame grab",
        )?;
        let _grab_guard = GrabGuard { api: &api };
        check_positive(
            unsafe { (api.set_soft_trigger)(DEVICE_ID) },
            "software trigger",
        )?;

        let timeout = Duration::from_millis(exposure.seconds().mul_add(1000.0, 5000.0) as u64);
        let frame = receiver.recv_timeout(timeout).map_err(|_| {
            Error::new(
                ErrorCode::Timeout,
                "Mightex camera capture timed out waiting for frame callback",
            )
        })?;
        Ok(RuntimeFrame {
            width: frame.width,
            height: frame.height,
            bit_depth,
            module_number,
            serial_number,
            data: frame.data,
        })
    }

    extern "system" fn fault_callback(_device_type: i32) {}

    fn check_positive(value: i32, operation: &str) -> Result<()> {
        if value > 0 {
            Ok(())
        } else {
            Err(Error::new(
                ErrorCode::Transport,
                format!("Mightex camera {operation} failed"),
            ))
        }
    }

    fn read_identity(api: &Api) -> (String, Option<String>) {
        let mut module = [0 as c_char; MAX_NAME];
        let mut serial = [0 as c_char; MAX_NAME];
        if unsafe { (api.get_module_serial)(DEVICE_ID, module.as_mut_ptr(), serial.as_mut_ptr()) }
            < 0
        {
            return (String::new(), None);
        }
        let module = unsafe { CStr::from_ptr(module.as_ptr()) }
            .to_string_lossy()
            .trim()
            .to_string();
        let serial = unsafe { CStr::from_ptr(serial.as_ptr()) }
            .to_string_lossy()
            .trim()
            .to_string();
        (
            module,
            if serial.is_empty() {
                None
            } else {
                Some(serial)
            },
        )
    }

    fn exposure_ticks(exposure: TimeInterval) -> Result<i32> {
        let ticks = exposure.seconds() * 1_000_000.0 / 50.0;
        if !ticks.is_finite() || ticks < 0.0 || ticks > i32::MAX as f64 {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Mightex camera exposure must be a finite non-negative interval",
            ));
        }
        Ok(ticks.round().max(1.0) as i32)
    }

    fn camera_buffer_count(width: u32, height: u32) -> i32 {
        let pixels = width.saturating_mul(height);
        match pixels {
            0..=76_800 => 16,
            76_801..=307_200 => 8,
            307_201..=614_400 => 4,
            _ => 2,
        }
    }

    fn symbol<T: Copy>(library: &Library, name: &str) -> Result<T> {
        unsafe { library.get::<T>(name.as_bytes()) }
            .map(|symbol| *symbol)
            .map_err(|_| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Mightex camera runtime is missing required symbol {name}"),
                )
            })
    }

    struct InitGuard<'a> {
        api: &'a Api,
    }

    impl Drop for InitGuard<'_> {
        fn drop(&mut self) {
            unsafe { (self.api.uninit)() };
        }
    }

    struct EngineGuard<'a> {
        api: &'a Api,
    }

    impl Drop for EngineGuard<'_> {
        fn drop(&mut self) {
            let _ = unsafe { (self.api.stop_engine)() };
        }
    }

    struct GrabGuard<'a> {
        api: &'a Api,
    }

    impl Drop for GrabGuard<'_> {
        fn drop(&mut self) {
            let _ = unsafe { (self.api.stop_grab)() };
        }
    }

    struct CallbackGuard<'a> {
        lock: &'a Mutex<Option<Sender<CallbackFrame>>>,
    }

    impl Drop for CallbackGuard<'_> {
        fn drop(&mut self) {
            if let Ok(mut guard) = self.lock.lock() {
                *guard = None;
            }
        }
    }
}
