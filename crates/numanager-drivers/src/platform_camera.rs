use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io::Read;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformCameraBackend {
    V4l2,
    GStreamer,
    DirectShow,
    Fixture,
}

impl PlatformCameraBackend {
    pub fn name(self) -> &'static str {
        match self {
            PlatformCameraBackend::V4l2 => "v4l2",
            PlatformCameraBackend::GStreamer => "gstreamer",
            PlatformCameraBackend::DirectShow => "directshow",
            PlatformCameraBackend::Fixture => "fixture",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlatformCameraFormat {
    pub width: u32,
    pub height: u32,
    pub pixel_format: &'static str,
    pub frame_interval_s: f64,
}

impl PlatformCameraFormat {
    pub fn value(&self) -> Value {
        Value::Map(BTreeMap::from([
            (
                "width".into(),
                Value::PixelCount(PixelCount::new(self.width)),
            ),
            (
                "height".into(),
                Value::PixelCount(PixelCount::new(self.height)),
            ),
            (
                "pixel_format".into(),
                Value::String(self.pixel_format.into()),
            ),
            (
                "frame_interval".into(),
                Value::TimeInterval(TimeInterval::from_seconds(self.frame_interval_s)),
            ),
        ]))
    }
}

pub struct PlatformCameraDiscovery {
    next_id: DriverId,
    probes: Vec<PlatformCameraConfiguredProbe>,
    #[cfg(target_os = "linux")]
    active_v4l2: bool,
}

impl PlatformCameraDiscovery {
    pub fn simulated(next_id: DriverId, backend: PlatformCameraBackend) -> Self {
        Self {
            next_id,
            probes: vec![PlatformCameraConfiguredProbe::simulated(backend)],
            #[cfg(target_os = "linux")]
            active_v4l2: false,
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| {
                matches!(
                    device.driver.as_str(),
                    "platform_camera"
                        | "platform-camera"
                        | "platform_camera_fixture"
                        | "platform-camera-fixture"
                )
            })
            .map(PlatformCameraConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            next_id,
            probes,
            #[cfg(target_os = "linux")]
            active_v4l2: false,
        })
    }

    #[cfg(target_os = "linux")]
    pub fn v4l2(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: Vec::new(),
            active_v4l2: true,
        }
    }
}

impl DriverDiscovery for PlatformCameraDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        #[cfg(not(target_os = "linux"))]
        let probes = std::mem::take(&mut self.probes);
        #[cfg(target_os = "linux")]
        let mut probes = std::mem::take(&mut self.probes);
        #[cfg(target_os = "linux")]
        if self.active_v4l2 {
            probes.extend(active_v4l2_probes()?);
        }
        Ok(probes
            .iter()
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                DriverCandidate::from_driver(
                    probe.discovery_label(),
                    Box::new(PlatformCameraDriver::configured(id, probe.clone())),
                )
            })
            .collect())
    }
}

#[derive(Debug, Clone)]
pub struct PlatformCameraConfiguredProbe {
    backend: PlatformCameraBackend,
    label: String,
    device_path: Option<String>,
    device_name: Option<String>,
    width: u32,
    height: u32,
    exposure_s: f64,
    gain_percent: i64,
    pixel_format: String,
    frame_interval_s: f64,
    fixture_path: Option<String>,
    connect: bool,
    simulated: bool,
}

impl PlatformCameraConfiguredProbe {
    pub fn simulated(backend: PlatformCameraBackend) -> Self {
        Self {
            backend,
            label: format!("platform-camera-{}", backend.name()),
            device_path: None,
            device_name: None,
            width: 1280,
            height: 720,
            exposure_s: 0.02,
            gain_percent: 100,
            pixel_format: ImageEncoding::Mono8.property_value().into(),
            frame_interval_s: 1.0 / 30.0,
            fixture_path: None,
            connect: false,
            simulated: true,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let backend = string_prop(device, "backend")
            .as_deref()
            .map(parse_platform_backend)
            .transpose()?
            .unwrap_or(PlatformCameraBackend::Fixture);
        let mut probe = Self::simulated(backend);
        probe.simulated = false;
        probe.label = string_prop(device, "label").unwrap_or_else(|| device.label.clone());
        if probe.label.is_empty() {
            probe.label = format!("platform-camera-{}", backend.name());
        }
        probe.device_path = string_prop(device, "device_path");
        probe.device_name = string_prop(device, "device_name");
        if let Some(width) = pixel_count_prop(device, "width") {
            probe.width = width;
        }
        if let Some(height) = pixel_count_prop(device, "height") {
            probe.height = height;
        }
        if let Some(exposure) = time_interval_prop(device, "exposure", "exposure_s") {
            probe.exposure_s = exposure.seconds();
        }
        if let Some(gain) = ratio_prop(device, "gain", "gain_percent") {
            probe.gain_percent = gain.percent().round() as i64;
        }
        if let Some(frame_interval) =
            time_interval_prop(device, "frame_interval", "frame_interval_s")
        {
            probe.frame_interval_s = frame_interval.seconds();
        }
        if let Some(pixel_format) = string_prop(device, "pixel_format") {
            let pixel_format = canonical_platform_pixel_format(&pixel_format).ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unsupported platform camera pixel_format {pixel_format}"),
                )
            })?;
            probe.pixel_format = pixel_format.into();
        }
        probe.fixture_path = string_prop(device, "fixture_path");
        probe.connect = bool_prop(device, "connect").unwrap_or(probe.connect);
        Ok(probe)
    }

    fn discovery_label(&self) -> String {
        if self.simulated {
            format!("Simulated {} platform camera", self.backend.name())
        } else if let Some(path) = &self.device_path {
            format!("Platform camera {} ({path})", self.label)
        } else {
            format!(
                "Configured platform camera {} ({})",
                self.label,
                self.backend.name()
            )
        }
    }
}

#[cfg(target_os = "linux")]
fn active_v4l2_probes() -> Result<Vec<PlatformCameraConfiguredProbe>> {
    let entries = match fs::read_dir("/sys/class/video4linux") {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("scan V4L2 device descriptors failed: {error}"),
            ))
        }
    };
    let mut probes = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("read V4L2 descriptor entry failed: {error}"),
            )
        })?;
        let node = entry.file_name().to_string_lossy().into_owned();
        if !node.starts_with("video") {
            continue;
        }
        let device_path = format!("/dev/{node}");
        let name_path = entry.path().join("name");
        let device_name = fs::read_to_string(&name_path)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let product = device_name.clone().unwrap_or_else(|| node.clone());
        let mut probe = PlatformCameraConfiguredProbe::simulated(PlatformCameraBackend::V4l2);
        probe.simulated = false;
        probe.label = format!("V4L2 {product}");
        probe.device_path = Some(device_path);
        probe.device_name = device_name;
        probe.fixture_path = None;
        probes.push(probe);
    }
    probes.sort_by(|left, right| left.device_path.cmp(&right.device_path));
    Ok(probes)
}

pub struct PlatformCameraDriver {
    id: DriverId,
    camera: DeviceId,
    resource: ResourceId,
    backend: PlatformCameraBackend,
    label: String,
    device_path: Option<String>,
    device_name: Option<String>,
    simulated: bool,
    width: u32,
    height: u32,
    exposure_s: f64,
    gain_percent: i64,
    pixel_format: String,
    frame_interval_s: f64,
    next_token: u64,
    events: VecDeque<DriverEvent>,
    worker_tx: Sender<DriverEvent>,
    worker_rx: Receiver<DriverEvent>,
    fixture_path: Option<String>,
    connect: bool,
}

impl PlatformCameraDriver {
    pub fn simulated(id: DriverId, backend: PlatformCameraBackend) -> Self {
        Self::configured(id, PlatformCameraConfiguredProbe::simulated(backend))
    }

    pub fn configured(id: DriverId, probe: PlatformCameraConfiguredProbe) -> Self {
        let (worker_tx, worker_rx) = mpsc::channel();
        Self {
            id,
            camera: DeviceId(NodeId(700)),
            resource: ResourceId(NodeId(701)),
            backend: probe.backend,
            label: probe.label,
            device_path: probe.device_path,
            device_name: probe.device_name,
            simulated: probe.simulated,
            width: probe.width,
            height: probe.height,
            exposure_s: probe.exposure_s,
            gain_percent: probe.gain_percent,
            pixel_format: probe.pixel_format,
            frame_interval_s: probe.frame_interval_s,
            next_token: 1,
            events: VecDeque::new(),
            worker_tx,
            worker_rx,
            fixture_path: probe.fixture_path,
            connect: probe.connect,
        }
    }

    fn next_token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn descriptor(&self) -> DeviceDescriptor {
        let mut kinds = vec!["camera".into(), "platform.camera".into()];
        if self.can_simulate_triggers() {
            kinds.extend(["trigger.sink".into(), "trigger.source".into()]);
        }
        DeviceDescriptor {
            id: self.camera,
            driver: self.id,
            label: self.label.clone(),
            vendor: Some("Platform".into()),
            model: Some(format!("{} camera backend", self.backend.name())),
            serial: None,
            kinds,
            properties: vec![
                property_range(
                    "exposure",
                    "Exposure",
                    ValueType::TimeInterval,
                    Some("s"),
                    true,
                    Value::TimeInterval(TimeInterval::from_milliseconds(0.1)),
                    Value::TimeInterval(TimeInterval::from_seconds(10.0)),
                ),
                property_range(
                    "gain",
                    "Gain",
                    ValueType::Ratio,
                    Some("percent"),
                    true,
                    Value::Ratio(Ratio::from_percent(0.0)),
                    Value::Ratio(Ratio::from_percent(800.0)),
                ),
                property_enum(
                    "pixel_format",
                    "Pixel format",
                    ValueType::String,
                    None,
                    true,
                    ["Native", "Mono8", "Mono16", "Rgb8", "Bgr8", "Yuyv", "Mjpeg"],
                ),
                property_range(
                    "frame_interval",
                    "Frame interval",
                    ValueType::TimeInterval,
                    Some("s"),
                    true,
                    Value::TimeInterval(TimeInterval::from_milliseconds(1.0)),
                    Value::TimeInterval(TimeInterval::from_seconds(60.0)),
                ),
                property_range(
                    "width",
                    "Width",
                    ValueType::PixelCount,
                    Some("px"),
                    false,
                    Value::PixelCount(PixelCount::new(1)),
                    Value::PixelCount(PixelCount::new(8192)),
                ),
                property_range(
                    "height",
                    "Height",
                    ValueType::PixelCount,
                    Some("px"),
                    false,
                    Value::PixelCount(PixelCount::new(1)),
                    Value::PixelCount(PixelCount::new(8192)),
                ),
                property(
                    "active_format",
                    "Active format",
                    ValueType::Map,
                    None,
                    false,
                ),
                property(
                    "supported_formats",
                    "Supported formats",
                    ValueType::List,
                    None,
                    false,
                ),
                property("backend", "Backend", ValueType::String, None, false),
                property("device_path", "Device path", ValueType::String, None, false),
                property("device_name", "Device name", ValueType::String, None, false),
                property("connect", "Connect", ValueType::Bool, None, false),
                property(
                    "capture_gate",
                    "Capture gate",
                    ValueType::String,
                    None,
                    false,
                ),
            ],
            metadata: self.device_metadata(),
        }
    }

    fn device_metadata(&self) -> BTreeMap<String, Value> {
        let mut metadata = BTreeMap::from([
            ("backend".into(), Value::String(self.backend.name().into())),
            ("active_format".into(), self.active_format().value()),
            (
                "supported_formats".into(),
                Value::List(
                    self.supported_formats()
                        .iter()
                        .map(PlatformCameraFormat::value)
                        .collect(),
                ),
            ),
            ("sdk_free".into(), Value::Bool(true)),
            (
                "transport_strategy".into(),
                Value::String(
                    "platform camera stack fixture plus descriptor-only OS discovery; real V4L2/GStreamer/DirectShow frame bindings are explicit"
                        .into(),
                ),
            ),
            (
                "capture_gate".into(),
                Value::String(self.capture_gate().into()),
            ),
            (
                "frame_source".into(),
                Value::String(self.frame_source_label().into()),
            ),
            ("ring_buffer".into(), Value::Bool(self.can_generate_frames())),
            ("connect".into(), Value::Bool(self.connect)),
        ]);
        if let Some(path) = &self.fixture_path {
            metadata.insert("fixture_path".into(), Value::String(path.clone()));
        }
        if let Some(path) = &self.device_path {
            metadata.insert("device_path".into(), Value::String(path.clone()));
        }
        if let Some(name) = &self.device_name {
            metadata.insert("device_name".into(), Value::String(name.clone()));
        }
        metadata
    }

    fn can_generate_frames(&self) -> bool {
        self.simulated
            || self.backend == PlatformCameraBackend::Fixture
            || self.fixture_path.is_some()
            || (self.backend == PlatformCameraBackend::V4l2
                && self.connect
                && self.device_path.is_some())
    }

    fn can_simulate_triggers(&self) -> bool {
        self.simulated
            || self.backend == PlatformCameraBackend::Fixture
            || self.fixture_path.is_some()
    }

    fn frame_source_label(&self) -> &'static str {
        if self.backend == PlatformCameraBackend::V4l2 && self.connect && self.device_path.is_some()
        {
            "v4l2-read"
        } else if self.can_generate_frames() {
            "fixture"
        } else {
            "descriptor_only"
        }
    }

    fn capture_gate(&self) -> &'static str {
        if self.backend == PlatformCameraBackend::V4l2 && self.connect && self.device_path.is_some()
        {
            "explicit V4L2 read() frame source available"
        } else if self.can_generate_frames() {
            "fixture frame source available"
        } else {
            "OS backend descriptor discovered; capture is not exposed because an explicit frame source is not configured"
        }
    }

    fn validate_property(&self, key: &str, value: &Value) -> Result<()> {
        let key = public_camera_key(key);
        let descriptor = self.descriptor();
        let schema = descriptor
            .properties
            .iter()
            .find(|property| property.key == key)
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    "unknown platform camera property",
                )
            })?;
        if !schema.writable {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "property is read-only",
            ));
        }
        schema.validate(value)?;
        match (key, value) {
            ("pixel_format", Value::String(value))
                if canonical_platform_pixel_format(value).is_some() =>
            {
                Ok(())
            }
            ("pixel_format", Value::String(_)) => Err(Error::new(
                ErrorCode::InvalidProperty,
                "unsupported platform camera pixel_format",
            )),
            _ => Ok(()),
        }
    }

    fn apply_property(&mut self, key: &str, value: &Value) -> Result<()> {
        self.validate_property(key, value)?;
        let key = public_camera_key(key);
        match (key, value) {
            ("exposure", value) => self.exposure_s = seconds(value)?,
            ("gain", Value::Ratio(value)) => self.gain_percent = value.percent().round() as i64,
            ("pixel_format", Value::String(value)) => {
                self.pixel_format = canonical_platform_pixel_format(value)
                    .unwrap_or(ImageEncoding::Mono8.property_value())
                    .into()
            }
            ("frame_interval", value) => self.frame_interval_s = seconds(value)?,
            _ => {}
        }
        Ok(())
    }

    fn active_format(&self) -> PlatformCameraFormat {
        let pixel_format = if self.pixel_format == "Native" {
            self.supported_formats()
                .into_iter()
                .next()
                .map(|format| format.pixel_format)
                .unwrap_or(ImageEncoding::Mono8.property_value())
        } else {
            self.pixel_format.as_str()
        };
        PlatformCameraFormat {
            width: self.width,
            height: self.height,
            pixel_format: canonical_platform_pixel_format(pixel_format)
                .unwrap_or(ImageEncoding::Mono8.property_value()),
            frame_interval_s: self.frame_interval_s,
        }
    }

    fn supported_formats(&self) -> Vec<PlatformCameraFormat> {
        supported_formats_for(self.backend)
    }

    fn supports_pixel_format(&self, pixel_format: &str) -> bool {
        let Some(pixel_format) = canonical_platform_pixel_format(pixel_format) else {
            return false;
        };
        self.supported_formats()
            .iter()
            .any(|format| format.pixel_format == pixel_format)
    }

    fn negotiated_pixel_format(&self, encoding: Option<ImageEncoding>) -> Result<String> {
        let requested = requested_pixel_format(encoding, &self.pixel_format);
        let requested = if requested == "Native" {
            self.active_format().pixel_format.into()
        } else {
            requested
        };
        if self.supports_pixel_format(&requested) {
            Ok(requested)
        } else {
            Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "{} backend does not advertise pixel format {}",
                    self.backend.name(),
                    requested
                ),
            ))
        }
    }

    fn local_timing_routes(&self, plan: &TimingPlan) -> Vec<Value> {
        plan.routes
            .iter()
            .filter(|route| route.from == self.camera || route.to == self.camera)
            .map(|route| {
                Value::Map(BTreeMap::from([
                    ("from".into(), Value::I64(route.from.0 .0 as i64)),
                    ("to".into(), Value::I64(route.to.0 .0 as i64)),
                    (
                        "signal".into(),
                        Value::String(format!("{:?}", route.signal)),
                    ),
                    ("edge".into(), Value::String(format!("{:?}", route.edge))),
                    (
                        "delay".into(),
                        Value::TimeInterval(TimeInterval::from_seconds(route.delay.as_secs_f64())),
                    ),
                ]))
            })
            .collect()
    }

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| sequence.device == self.camera)
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        let descriptor = self.descriptor();
        for sequence in self.local_timing_sequences(plan) {
            if sequence.values.is_empty() {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "platform camera timing sequence must contain at least one value",
                ));
            }
            let schema = descriptor
                .properties
                .iter()
                .find(|property| property.key == sequence.property)
                .ok_or_else(|| {
                    Error::new(
                        ErrorCode::InvalidProperty,
                        "unknown platform camera property",
                    )
                })?;
            if !schema.sequenceable {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!(
                        "platform camera property {} is not sequenceable",
                        sequence.property
                    ),
                ));
            }
            for value in &sequence.values {
                self.validate_property(&sequence.property, value)?;
            }
        }
        Ok(())
    }

    fn timing_sequence_summary(&self, plan: &TimingPlan) -> Vec<Value> {
        self.local_timing_sequences(plan)
            .into_iter()
            .map(|sequence| {
                Value::Map(BTreeMap::from([
                    ("property".into(), Value::String(sequence.property.clone())),
                    ("count".into(), Value::I64(sequence.values.len() as i64)),
                ]))
            })
            .collect()
    }

    fn timing_summary(&self, plan: &TimingPlan, phase: &str, applied: Value) -> Value {
        Value::Map(BTreeMap::from([
            ("backend".into(), Value::String(self.backend.name().into())),
            ("camera".into(), Value::I64(self.camera.0 .0 as i64)),
            ("phase".into(), Value::String(phase.into())),
            ("routes".into(), Value::List(self.local_timing_routes(plan))),
            (
                "sequences".into(),
                Value::List(self.timing_sequence_summary(plan)),
            ),
            ("exposure".into(), time_interval(self.exposure_s)),
            (
                "gain".into(),
                Value::Ratio(Ratio::from_percent(self.gain_percent as f64)),
            ),
            (
                "pixel_format".into(),
                Value::String(self.pixel_format.clone()),
            ),
            (
                "frame_interval".into(),
                time_interval(self.frame_interval_s),
            ),
            ("applied".into(), applied),
        ]))
    }

    fn apply_timing_sequence_step(&mut self, plan: &TimingPlan, start: bool) -> Result<Value> {
        let sequences = self
            .local_timing_sequences(plan)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let mut applied = BTreeMap::new();
        for sequence in sequences {
            let value = (if start {
                sequence.values.first()
            } else {
                sequence.values.last()
            })
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidCommand,
                    "platform camera timing sequence must contain at least one value",
                )
            })?
            .clone();
            self.apply_property(&sequence.property, &value)?;
            let key = public_camera_key(&sequence.property);
            let applied_value = match key {
                "exposure" => time_interval(self.exposure_s),
                "gain" => Value::Ratio(Ratio::from_percent(self.gain_percent as f64)),
                "pixel_format" => Value::String(self.pixel_format.clone()),
                "frame_interval" => time_interval(self.frame_interval_s),
                _ => value,
            };
            self.events
                .push_back(DriverEvent::Event(Event::PropertyChanged(
                    PropertyChanged {
                        device: sequence.device,
                        key: key.into(),
                        value: applied_value.clone(),
                    },
                )));
            applied.insert(format!("{}:{}", sequence.device.0 .0, key), applied_value);
        }
        Ok(Value::Map(applied))
    }

    fn trigger_transaction(
        &self,
        kind: CapabilityKind,
        action: PlatformTriggerAction,
    ) -> PhysicalTransaction {
        PhysicalTransaction {
            resource: Some(self.resource),
            description: format!("platform camera {}", kind.name()),
            payload: Value::Map(BTreeMap::from([
                ("backend".into(), Value::String(self.backend.name().into())),
                ("camera".into(), Value::I64(self.camera.0 .0 as i64)),
                ("capability".into(), Value::String(kind.name().into())),
                ("action".into(), Value::String(action.name().into())),
                (
                    "completion".into(),
                    Value::String("fixture backend trigger ack".into()),
                ),
            ])),
        }
    }

    fn invoke_trigger(&mut self, kind: CapabilityKind, action: PlatformTriggerAction) -> Value {
        self.events
            .push_back(DriverEvent::Event(Event::Telemetry(TelemetryEvent {
                device: self.camera,
                values: BTreeMap::from([
                    ("backend".into(), Value::String(self.backend.name().into())),
                    ("capability".into(), Value::String(kind.name().into())),
                    ("action".into(), Value::String(action.name().into())),
                    (
                        "triggered".into(),
                        Value::Bool(matches!(action, PlatformTriggerAction::Pulse)),
                    ),
                    (
                        "completion".into(),
                        Value::String("fixture backend trigger ack".into()),
                    ),
                ]),
            })));
        Value::Map(BTreeMap::from([
            ("backend".into(), Value::String(self.backend.name().into())),
            ("capability".into(), Value::String(kind.name().into())),
            ("action".into(), Value::String(action.name().into())),
            (
                "triggered".into(),
                Value::Bool(matches!(action, PlatformTriggerAction::Pulse)),
            ),
        ]))
    }
}

impl Driver for PlatformCameraDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        vec![self.descriptor()]
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: format!("{}-stream", self.label),
            kind: format!("camera.{}", self.backend.name()),
            metadata: BTreeMap::from([
                ("sdk_free".into(), Value::Bool(true)),
                (
                    "frame_completion".into(),
                    Value::String("frame-ready event".into()),
                ),
            ]),
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device != self.camera || !self.can_generate_frames() {
            return Vec::new();
        }
        let mut capabilities = vec![
            capability(1, device, CapabilityKind::CameraCapture),
            capability(2, device, CapabilityKind::CameraStream),
        ];
        if self.can_simulate_triggers() {
            capabilities.extend([
                capability(3, device, CapabilityKind::TriggerSink),
                capability(4, device, CapabilityKind::TriggerSource),
            ]);
        }
        capabilities
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        for command in &batch.commands {
            match command {
                Command::WriteProperty { device, key, value } if *device == self.camera => {
                    self.validate_property(key, value)?;
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        if write.device == self.camera {
                            self.validate_property(&write.property, &write.value)?;
                        }
                    }
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if *device == self.camera && *capability == CapabilityId(1) => {
                    if !self.can_generate_frames() {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "platform camera capture needs a configured frame source or supported OS backend",
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
                    let encoding = match request {
                        CapabilityRequest::CameraCapture(request) => request.encoding.clone(),
                        CapabilityRequest::None => None,
                        _ => unreachable!(),
                    };
                    let _ = self.negotiated_pixel_format(encoding)?;
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if *device == self.camera && *capability == CapabilityId(2) => {
                    if !self.can_generate_frames() {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "platform camera stream needs a configured frame source or supported OS backend",
                        ));
                    }
                    if !matches!(request, CapabilityRequest::CameraStream(_)) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "CameraStream expects CameraStreamRequest",
                        ));
                    }
                    let CapabilityRequest::CameraStream(request) = request else {
                        unreachable!();
                    };
                    let _ = self.negotiated_pixel_format(request.encoding.clone())?;
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if *device == self.camera
                    && matches!(*capability, CapabilityId(3) | CapabilityId(4)) =>
                {
                    if !self.can_simulate_triggers() {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "platform camera trigger capabilities need a configured frame source or supported OS backend",
                        ));
                    }
                    let kind = if *capability == CapabilityId(3) {
                        CapabilityKind::TriggerSink
                    } else {
                        CapabilityKind::TriggerSource
                    };
                    let _ = parse_platform_trigger_action(request, &kind)?;
                }
                _ => {}
            }
        }
        let mut physical_transactions = vec![PhysicalTransaction {
            resource: Some(self.resource),
            description: "platform camera command batch".into(),
            payload: Value::String(self.backend.name().into()),
        }];
        for command in &batch.commands {
            if let Command::Invoke {
                device,
                capability,
                request,
            } = command
            {
                if *device == self.camera
                    && matches!(*capability, CapabilityId(3) | CapabilityId(4))
                {
                    let kind = if *capability == CapabilityId(3) {
                        CapabilityKind::TriggerSink
                    } else {
                        CapabilityKind::TriggerSource
                    };
                    let action = parse_platform_trigger_action(request, &kind)?;
                    physical_transactions.push(self.trigger_transaction(kind, action));
                }
            }
        }
        Ok(PreparedBatch {
            id: batch.id,
            commands: batch.commands.clone(),
            physical_transactions,
        })
    }

    fn prepare_timing_plan(
        &mut self,
        plan: &TimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        self.validate_timing_plan(plan)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Arm(plan.clone())],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "platform camera timing arm".into(),
                payload: self.timing_summary(plan, "arm", Value::Null),
            }],
        })
    }

    fn start_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let applied = self.apply_timing_sequence_step(&armed.plan, true)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Start(OperationId(0))],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "platform camera timing start".into(),
                payload: self.timing_summary(&armed.plan, "start", applied),
            }],
        })
    }

    fn stop_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let applied = self.apply_timing_sequence_step(&armed.plan, false)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "platform camera timing stop".into(),
                payload: self.timing_summary(&armed.plan, "stop", applied),
            }],
        })
    }

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.next_token();
        let mut result = Value::Null;
        for command in prepared.commands {
            match command {
                Command::ReadProperty { device, key } if device == self.camera => {
                    result = match public_camera_key(&key) {
                        "exposure" => time_interval(self.exposure_s),
                        "gain" => Value::Ratio(Ratio::from_percent(self.gain_percent as f64)),
                        "pixel_format" => Value::String(self.pixel_format.clone()),
                        "frame_interval" => time_interval(self.frame_interval_s),
                        "width" => Value::PixelCount(PixelCount::new(self.width)),
                        "height" => Value::PixelCount(PixelCount::new(self.height)),
                        "active_format" => self.active_format().value(),
                        "supported_formats" => Value::List(
                            self.supported_formats()
                                .iter()
                                .map(PlatformCameraFormat::value)
                                .collect(),
                        ),
                        "backend" => Value::String(self.backend.name().into()),
                        "device_path" => {
                            Value::String(self.device_path.clone().unwrap_or_default())
                        }
                        "device_name" => {
                            Value::String(self.device_name.clone().unwrap_or_default())
                        }
                        "connect" => Value::Bool(self.connect),
                        "capture_gate" => Value::String(self.capture_gate().into()),
                        _ => Value::Null,
                    };
                }
                Command::WriteProperty { device, key, value } if device == self.camera => {
                    self.apply_property(&key, &value)?;
                    self.events
                        .push_back(DriverEvent::Event(Event::PropertyChanged(
                            PropertyChanged { device, key, value },
                        )));
                    result = Value::Bool(true);
                }
                Command::ApplyStateSet(set) => {
                    let mut values = BTreeMap::new();
                    for write in set.writes {
                        if write.device == self.camera {
                            self.apply_property(&write.property, &write.value)?;
                            values.insert(write.property.clone(), write.value.clone());
                            self.events
                                .push_back(DriverEvent::Event(Event::PropertyChanged(
                                    PropertyChanged {
                                        device: write.device,
                                        key: write.property,
                                        value: write.value,
                                    },
                                )));
                        }
                    }
                    result = Value::Map(values);
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if device == self.camera && capability == CapabilityId(1) => {
                    if !self.can_generate_frames() {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "platform camera capture needs a configured frame source or supported OS backend",
                        ));
                    }
                    let capture = match request {
                        CapabilityRequest::CameraCapture(request) => request,
                        CapabilityRequest::None => CameraCaptureRequest::default_frame(),
                        _ => unreachable!(),
                    };
                    spawn_frames(FrameJob {
                        tx: self.worker_tx.clone(),
                        token,
                        device,
                        backend: self.backend,
                        width: self.width,
                        height: self.height,
                        exposure_s: self.exposure_s,
                        gain_percent: self.gain_percent,
                        frame_interval_s: self.frame_interval_s,
                        pixel_format: self.negotiated_pixel_format(capture.encoding)?,
                        buffer: capture.buffer.unwrap_or_default(),
                        frame_count: 1,
                        fixture_path: self.fixture_path.clone(),
                        device_path: self.device_path.clone(),
                        connect: self.connect,
                    });
                    return Ok(token);
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if device == self.camera && capability == CapabilityId(2) => {
                    if !self.can_generate_frames() {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "platform camera stream needs a configured frame source or supported OS backend",
                        ));
                    }
                    let request = match request {
                        CapabilityRequest::CameraStream(request) => request,
                        _ => unreachable!(),
                    };
                    spawn_frames(FrameJob {
                        tx: self.worker_tx.clone(),
                        token,
                        device,
                        backend: self.backend,
                        width: self.width,
                        height: self.height,
                        exposure_s: self.exposure_s,
                        gain_percent: self.gain_percent,
                        frame_interval_s: self.frame_interval_s,
                        pixel_format: self.negotiated_pixel_format(request.encoding)?,
                        buffer: request.buffer,
                        frame_count: request.frame_count.unwrap_or(8),
                        fixture_path: self.fixture_path.clone(),
                        device_path: self.device_path.clone(),
                        connect: self.connect,
                    });
                    return Ok(token);
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if device == self.camera
                    && matches!(capability, CapabilityId(3) | CapabilityId(4)) =>
                {
                    if !self.can_simulate_triggers() {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "platform camera trigger capabilities need a configured frame source or supported OS backend",
                        ));
                    }
                    let kind = if capability == CapabilityId(3) {
                        CapabilityKind::TriggerSink
                    } else {
                        CapabilityKind::TriggerSource
                    };
                    let action = parse_owned_platform_trigger_action(request, &kind)?;
                    result = self.invoke_trigger(kind, action);
                }
                Command::Invoke { .. } => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "unsupported platform camera capability invocation",
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
        while let Ok(event) = self.worker_rx.try_recv() {
            self.events.push_back(event);
        }
        self.events.drain(..).collect()
    }
}

struct FrameJob {
    tx: Sender<DriverEvent>,
    token: DriverToken,
    device: DeviceId,
    backend: PlatformCameraBackend,
    width: u32,
    height: u32,
    exposure_s: f64,
    gain_percent: i64,
    frame_interval_s: f64,
    pixel_format: String,
    buffer: FrameBufferSpec,
    frame_count: u64,
    fixture_path: Option<String>,
    device_path: Option<String>,
    connect: bool,
}

fn spawn_frames(job: FrameJob) {
    thread::spawn(move || {
        let stream = StreamId(job.token.0);
        let mut completed_width = job.width;
        let mut completed_height = job.height;
        for index in 0..job.frame_count {
            let frame = match load_platform_fixture_frame(&job) {
                Ok(frame) => frame,
                Err(error) => {
                    let report: ErrorReport = error.into();
                    let _ = job.tx.send(DriverEvent::Event(Event::Fault(FaultEvent {
                        device: Some(job.device),
                        report: report.clone(),
                    })));
                    let _ = job.tx.send(DriverEvent::TokenFailed {
                        token: job.token,
                        report,
                    });
                    return;
                }
            };
            completed_width = frame.width;
            completed_height = frame.height;
            let handle = FrameHandle {
                stream,
                frame: FrameId(index),
            };
            let _ = job.tx.send(DriverEvent::FrameReady(Frame {
                handle,
                device: job.device,
                width: frame.width,
                height: frame.height,
                pixel_format: job.pixel_format.clone(),
                data: frame.pixels,
                metadata: BTreeMap::from([
                    ("backend".into(), Value::String(job.backend.name().into())),
                    ("exposure".into(), time_interval(job.exposure_s)),
                    (
                        "gain".into(),
                        Value::Ratio(Ratio::from_percent(job.gain_percent as f64)),
                    ),
                    (
                        "pixel_format".into(),
                        Value::String(job.pixel_format.clone()),
                    ),
                    ("frame_interval".into(), time_interval(job.frame_interval_s)),
                    ("index".into(), Value::I64(index as i64)),
                ]),
                buffer: job.buffer.clone(),
            }));
        }
        let _ = job.tx.send(DriverEvent::TokenCompleted {
            token: job.token,
            value: Value::Map(BTreeMap::from([
                ("stream".into(), Value::I64(stream.0 as i64)),
                ("frame".into(), Value::I64(0)),
                (
                    "width".into(),
                    Value::PixelCount(PixelCount::new(completed_width)),
                ),
                (
                    "height".into(),
                    Value::PixelCount(PixelCount::new(completed_height)),
                ),
                ("frames".into(), Value::I64(job.frame_count as i64)),
                ("pixel_format".into(), Value::String(job.pixel_format)),
            ])),
        });
    });
}

pub(crate) struct PlatformFixtureFrame {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
}

fn load_platform_fixture_frame(job: &FrameJob) -> Result<PlatformFixtureFrame> {
    if job.backend == PlatformCameraBackend::V4l2 && job.connect && job.fixture_path.is_none() {
        return read_v4l2_frame(job);
    }
    let Some(path) = job.fixture_path.as_deref() else {
        let frame = crate::sim::gel_scene(job.width, job.height, job.exposure_s);
        return Ok(PlatformFixtureFrame {
            width: frame.width,
            height: frame.height,
            pixels: frame.pixels,
        });
    };
    let bytes = fs::read(path).map_err(|error| {
        Error::new(
            ErrorCode::Transport,
            format!("read platform camera fixture {path}: {error}"),
        )
    })?;
    decode_portable_pixmap(&bytes, &job.pixel_format)
}

fn read_v4l2_frame(job: &FrameJob) -> Result<PlatformFixtureFrame> {
    let Some(path) = job.device_path.as_deref() else {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "platform V4L2 capture requires configured device_path",
        ));
    };
    let bytes_per_pixel = v4l2_read_bytes_per_pixel(&job.pixel_format)?;
    let expected = job.width as usize * job.height as usize * bytes_per_pixel;
    let mut file = fs::File::open(path).map_err(|error| {
        Error::new(
            ErrorCode::Transport,
            format!("open V4L2 device {path}: {error}"),
        )
    })?;
    let mut pixels = vec![0_u8; expected];
    file.read_exact(&mut pixels).map_err(|error| {
        Error::new(
            ErrorCode::Transport,
            format!(
                "read V4L2 frame from {path} as {} {}x{} bytes: {error}",
                job.pixel_format, job.width, job.height
            ),
        )
    })?;
    Ok(PlatformFixtureFrame {
        width: job.width,
        height: job.height,
        pixels,
    })
}

fn v4l2_read_bytes_per_pixel(pixel_format: &str) -> Result<usize> {
    match canonical_platform_pixel_format(pixel_format) {
        Some("Mono8") => Ok(1),
        Some("Mono16") | Some("Yuyv") => Ok(2),
        Some("Rgb8") | Some("Bgr8") => Ok(3),
        Some("Mjpeg") => Err(Error::new(
            ErrorCode::Unsupported,
            "V4L2 read() capture does not support variable-length MJPEG frames yet",
        )),
        Some(other) => Err(Error::new(
            ErrorCode::Unsupported,
            format!("V4L2 read() capture does not support pixel format {other}"),
        )),
        None => Err(Error::new(
            ErrorCode::Unsupported,
            format!("V4L2 read() capture does not support pixel format {pixel_format}"),
        )),
    }
}

pub(crate) fn decode_portable_pixmap(
    bytes: &[u8],
    requested_pixel_format: &str,
) -> Result<PlatformFixtureFrame> {
    let mut cursor = PnmCursor::new(bytes);
    let magic = cursor.token()?;
    let width = cursor
        .token()?
        .parse::<u32>()
        .map_err(|_| Error::new(ErrorCode::Transport, "invalid platform fixture PNM width"))?;
    let height = cursor
        .token()?
        .parse::<u32>()
        .map_err(|_| Error::new(ErrorCode::Transport, "invalid platform fixture PNM height"))?;
    let max_value = cursor.token()?.parse::<u32>().map_err(|_| {
        Error::new(
            ErrorCode::Transport,
            "invalid platform fixture PNM max value",
        )
    })?;
    if width == 0 || height == 0 || max_value == 0 || max_value > 65_535 {
        return Err(Error::new(
            ErrorCode::Transport,
            "invalid platform fixture PNM dimensions or max value",
        ));
    }
    cursor.skip_ascii_ws_and_comments();
    let sample_count = width as usize * height as usize;
    let pixels = match magic.as_str() {
        "P5" => decode_binary_gray(cursor.remaining(), sample_count, max_value)?,
        "P6" => decode_binary_rgb(cursor.remaining(), sample_count, max_value)?,
        "P2" => decode_ascii_gray(&mut cursor, sample_count, max_value)?,
        "P3" => decode_ascii_rgb(&mut cursor, sample_count, max_value)?,
        _ => {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "platform camera fixture_path supports PGM/PPM P2/P3/P5/P6 files",
            ))
        }
    };
    Ok(PlatformFixtureFrame {
        width,
        height,
        pixels: convert_platform_fixture_pixels(pixels, requested_pixel_format),
    })
}

enum FixturePixels {
    Mono8(Vec<u8>),
    Rgb8(Vec<u8>),
}

fn decode_binary_gray(bytes: &[u8], sample_count: usize, max_value: u32) -> Result<FixturePixels> {
    if max_value <= 255 {
        if bytes.len() < sample_count {
            return Err(Error::new(
                ErrorCode::Transport,
                "truncated platform PGM fixture",
            ));
        }
        Ok(FixturePixels::Mono8(bytes[..sample_count].to_vec()))
    } else {
        if bytes.len() < sample_count * 2 {
            return Err(Error::new(
                ErrorCode::Transport,
                "truncated platform PGM fixture",
            ));
        }
        Ok(FixturePixels::Mono8(
            bytes[..sample_count * 2]
                .chunks_exact(2)
                .map(|sample| {
                    let value = u16::from_be_bytes([sample[0], sample[1]]) as u32;
                    scale_sample_to_u8(value, max_value)
                })
                .collect(),
        ))
    }
}

fn decode_binary_rgb(bytes: &[u8], sample_count: usize, max_value: u32) -> Result<FixturePixels> {
    let channels = sample_count * 3;
    if max_value <= 255 {
        if bytes.len() < channels {
            return Err(Error::new(
                ErrorCode::Transport,
                "truncated platform PPM fixture",
            ));
        }
        Ok(FixturePixels::Rgb8(bytes[..channels].to_vec()))
    } else {
        if bytes.len() < channels * 2 {
            return Err(Error::new(
                ErrorCode::Transport,
                "truncated platform PPM fixture",
            ));
        }
        Ok(FixturePixels::Rgb8(
            bytes[..channels * 2]
                .chunks_exact(2)
                .map(|sample| {
                    let value = u16::from_be_bytes([sample[0], sample[1]]) as u32;
                    scale_sample_to_u8(value, max_value)
                })
                .collect(),
        ))
    }
}

fn decode_ascii_gray(
    cursor: &mut PnmCursor<'_>,
    sample_count: usize,
    max_value: u32,
) -> Result<FixturePixels> {
    let mut pixels = Vec::with_capacity(sample_count);
    for _ in 0..sample_count {
        pixels.push(scale_sample_to_u8(cursor.sample()?, max_value));
    }
    Ok(FixturePixels::Mono8(pixels))
}

fn decode_ascii_rgb(
    cursor: &mut PnmCursor<'_>,
    sample_count: usize,
    max_value: u32,
) -> Result<FixturePixels> {
    let mut pixels = Vec::with_capacity(sample_count * 3);
    for _ in 0..sample_count * 3 {
        pixels.push(scale_sample_to_u8(cursor.sample()?, max_value));
    }
    Ok(FixturePixels::Rgb8(pixels))
}

fn convert_platform_fixture_pixels(pixels: FixturePixels, requested_pixel_format: &str) -> Vec<u8> {
    match (pixels, requested_pixel_format) {
        (FixturePixels::Mono8(pixels), "Rgb8") => pixels
            .into_iter()
            .flat_map(|sample| [sample, sample, sample])
            .collect(),
        (FixturePixels::Mono8(pixels), "Bgr8" | "BGR8") => pixels
            .into_iter()
            .flat_map(|sample| [sample, sample, sample])
            .collect(),
        (FixturePixels::Mono8(pixels), _) => pixels,
        (FixturePixels::Rgb8(pixels), "Bgr8" | "BGR8") => pixels
            .chunks_exact(3)
            .flat_map(|rgb| [rgb[2], rgb[1], rgb[0]])
            .collect(),
        (FixturePixels::Rgb8(pixels), "Mono8") | (FixturePixels::Rgb8(pixels), "Native") => pixels
            .chunks_exact(3)
            .map(|rgb| ((rgb[0] as u16 + rgb[1] as u16 + rgb[2] as u16) / 3) as u8)
            .collect(),
        (FixturePixels::Rgb8(pixels), _) => pixels,
    }
}

fn scale_sample_to_u8(value: u32, max_value: u32) -> u8 {
    ((value.min(max_value) * 255 + (max_value / 2)) / max_value) as u8
}

struct PnmCursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> PnmCursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn remaining(&self) -> &'a [u8] {
        &self.bytes[self.pos..]
    }

    fn token(&mut self) -> Result<String> {
        self.skip_ascii_ws_and_comments();
        let start = self.pos;
        while self.pos < self.bytes.len() && !self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
        if self.pos == start {
            return Err(Error::new(
                ErrorCode::Transport,
                "truncated platform camera PNM fixture header",
            ));
        }
        std::str::from_utf8(&self.bytes[start..self.pos])
            .map(|token| token.to_string())
            .map_err(|_| Error::new(ErrorCode::Transport, "invalid platform camera PNM token"))
    }

    fn sample(&mut self) -> Result<u32> {
        self.token()?
            .parse::<u32>()
            .map_err(|_| Error::new(ErrorCode::Transport, "invalid platform camera PNM sample"))
    }

    fn skip_ascii_ws_and_comments(&mut self) {
        loop {
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
                self.pos += 1;
            }
            if self.pos < self.bytes.len() && self.bytes[self.pos] == b'#' {
                while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
                    self.pos += 1;
                }
                continue;
            }
            break;
        }
    }
}

fn requested_pixel_format(encoding: Option<ImageEncoding>, configured: &str) -> String {
    match encoding.unwrap_or(ImageEncoding::Native) {
        ImageEncoding::Native => configured,
        ImageEncoding::Mono8 => ImageEncoding::Mono8.property_value(),
        ImageEncoding::Mono16 => ImageEncoding::Mono16.property_value(),
        ImageEncoding::Rgb8 => ImageEncoding::Rgb8.property_value(),
        ImageEncoding::Bgr8 => ImageEncoding::Bgr8.property_value(),
        ImageEncoding::Raw8 => ImageEncoding::Mono8.property_value(),
        ImageEncoding::Raw16 => ImageEncoding::Mono16.property_value(),
    }
    .into()
}

fn supported_formats_for(backend: PlatformCameraBackend) -> Vec<PlatformCameraFormat> {
    match backend {
        PlatformCameraBackend::V4l2 => vec![
            platform_format(1280, 720, "Mono8", 1.0 / 30.0),
            platform_format(1920, 1080, "Yuyv", 1.0 / 30.0),
            platform_format(1920, 1080, "Mjpeg", 1.0 / 60.0),
        ],
        PlatformCameraBackend::GStreamer => vec![
            platform_format(1280, 720, "Rgb8", 1.0 / 30.0),
            platform_format(1920, 1080, "Bgr8", 1.0 / 30.0),
            platform_format(1920, 1080, "Mjpeg", 1.0 / 60.0),
        ],
        PlatformCameraBackend::DirectShow => vec![
            platform_format(1280, 720, "Bgr8", 1.0 / 30.0),
            platform_format(1920, 1080, "Yuyv", 1.0 / 30.0),
            platform_format(1920, 1080, "Mjpeg", 1.0 / 60.0),
        ],
        PlatformCameraBackend::Fixture => vec![
            platform_format(1280, 720, "Mono8", 1.0 / 30.0),
            platform_format(1280, 720, "Rgb8", 1.0 / 30.0),
        ],
    }
}

fn platform_format(
    width: u32,
    height: u32,
    pixel_format: &'static str,
    frame_interval_s: f64,
) -> PlatformCameraFormat {
    PlatformCameraFormat {
        width,
        height,
        pixel_format,
        frame_interval_s,
    }
}

fn public_camera_key(key: &str) -> &str {
    match key {
        "exposure_s" => "exposure",
        "gain_percent" => "gain",
        "frame_interval_s" => "frame_interval",
        _ => key,
    }
}

fn canonical_platform_pixel_format(pixel_format: &str) -> Option<&'static str> {
    match pixel_format {
        "Yuyv" | "YUYV" => Some("Yuyv"),
        "Mjpeg" | "MJPEG" => Some("Mjpeg"),
        _ => canonical_image_encoding_name(pixel_format),
    }
}

fn parse_platform_backend(value: &str) -> Result<PlatformCameraBackend> {
    match value.trim().to_ascii_lowercase().as_str() {
        "v4l2" | "video4linux" | "video4linux2" => Ok(PlatformCameraBackend::V4l2),
        "gstreamer" | "gst" => Ok(PlatformCameraBackend::GStreamer),
        "directshow" | "dshow" => Ok(PlatformCameraBackend::DirectShow),
        "fixture" | "local" | "simulated" => Ok(PlatformCameraBackend::Fixture),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unsupported platform camera backend {value}"),
        )),
    }
}

fn string_prop(device: &DeviceConfig, key: &str) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn bool_prop(device: &DeviceConfig, key: &str) -> Option<bool> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn pixel_count_prop(device: &DeviceConfig, key: &str) -> Option<u32> {
    match device.properties.get(key) {
        Some(Value::PixelCount(value)) => Some(value.pixels()),
        Some(Value::I64(value)) if (1..=u32::MAX as i64).contains(value) => Some(*value as u32),
        _ => None,
    }
}

fn time_interval_prop(
    device: &DeviceConfig,
    key: &str,
    legacy_seconds_key: &str,
) -> Option<TimeInterval> {
    match device.properties.get(key) {
        Some(Value::TimeInterval(value)) => Some(*value),
        Some(Value::F64(value)) => Some(TimeInterval::from_seconds(*value)),
        _ => match device.properties.get(legacy_seconds_key) {
            Some(Value::TimeInterval(value)) => Some(*value),
            Some(Value::F64(value)) => Some(TimeInterval::from_seconds(*value)),
            _ => None,
        },
    }
}

fn ratio_prop(device: &DeviceConfig, key: &str, legacy_percent_key: &str) -> Option<Ratio> {
    match device.properties.get(key) {
        Some(Value::Ratio(value)) => Some(*value),
        Some(Value::F64(value)) => Some(Ratio::from_percent(*value)),
        Some(Value::I64(value)) => Some(Ratio::from_percent(*value as f64)),
        _ => match device.properties.get(legacy_percent_key) {
            Some(Value::Ratio(value)) => Some(*value),
            Some(Value::F64(value)) => Some(Ratio::from_percent(*value)),
            Some(Value::I64(value)) => Some(Ratio::from_percent(*value as f64)),
            _ => None,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlatformTriggerAction {
    Enable,
    Disable,
    Pulse,
}

impl PlatformTriggerAction {
    fn name(self) -> &'static str {
        match self {
            PlatformTriggerAction::Enable => "enable",
            PlatformTriggerAction::Disable => "disable",
            PlatformTriggerAction::Pulse => "pulse",
        }
    }
}

fn parse_platform_trigger_action(
    request: &CapabilityRequest,
    kind: &CapabilityKind,
) -> Result<PlatformTriggerAction> {
    match request {
        CapabilityRequest::None => Ok(PlatformTriggerAction::Pulse),
        CapabilityRequest::Trigger(request) => match request.action {
            TriggerAction::Enable => Ok(PlatformTriggerAction::Enable),
            TriggerAction::Disable => Ok(PlatformTriggerAction::Disable),
            TriggerAction::Pulse => Ok(PlatformTriggerAction::Pulse),
        },
        _ => Err(Error::new(
            ErrorCode::Unsupported,
            format!("{} expects None or CapabilityRequest::Trigger", kind.name()),
        )),
    }
}

fn parse_owned_platform_trigger_action(
    request: CapabilityRequest,
    kind: &CapabilityKind,
) -> Result<PlatformTriggerAction> {
    parse_platform_trigger_action(&request, kind)
}

fn capability(id: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    CapabilityDescriptor::new(CapabilityId(id), device, kind, ValueType::Map)
}

fn property(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    unit: Option<&str>,
    writable: bool,
) -> PropertySchema {
    PropertySchema {
        key: key.into(),
        display_name: display_name.into(),
        value_type,
        unit: unit.map(|unit| Unit(unit.into())),
        range: None,
        increment: None,
        enum_values: Vec::new(),
        readable: true,
        writable,
        volatile: false,
        sequenceable: false,
        hardware_address: None,
    }
}

fn property_range(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    unit: Option<&str>,
    writable: bool,
    min: Value,
    max: Value,
) -> PropertySchema {
    let mut schema = PropertySchema {
        key: key.into(),
        display_name: display_name.into(),
        value_type,
        unit: unit.map(|unit| Unit(unit.into())),
        range: Some(Range { min, max }),
        increment: None,
        enum_values: Vec::new(),
        readable: true,
        writable,
        volatile: false,
        sequenceable: false,
        hardware_address: None,
    };
    schema.sequenceable = matches!(key, "exposure" | "gain" | "frame_interval");
    schema
}

fn property_enum<const N: usize>(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    unit: Option<&str>,
    writable: bool,
    values: [&str; N],
) -> PropertySchema {
    let mut schema = PropertySchema {
        key: key.into(),
        display_name: display_name.into(),
        value_type,
        unit: unit.map(|unit| Unit(unit.into())),
        range: None,
        increment: None,
        enum_values: values
            .into_iter()
            .map(|value| EnumValue {
                value: Value::String(value.into()),
                label: value.into(),
            })
            .collect(),
        readable: true,
        writable,
        volatile: false,
        sequenceable: false,
        hardware_address: None,
    };
    schema.sequenceable = key == "pixel_format";
    schema
}

fn seconds(value: &Value) -> Result<f64> {
    match value {
        Value::TimeInterval(value) => Ok(value.seconds()),
        Value::F64(value) => Ok(*value),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("expected time interval, got {other:?}"),
        )),
    }
}

fn time_interval(seconds: f64) -> Value {
    Value::TimeInterval(TimeInterval::from_seconds(seconds))
}
