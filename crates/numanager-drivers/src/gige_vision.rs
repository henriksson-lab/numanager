use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::net::{ToSocketAddrs, UdpSocket};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod gvcp {
    pub const CONTROL_PORT: u16 = 3956;
    pub const DISCOVERY_CMD: u16 = 0x0002;
    pub const DISCOVERY_ACK: u16 = 0x0003;
    pub const READREG_CMD: u16 = 0x0080;
    pub const READREG_ACK: u16 = 0x0081;
    pub const WRITEREG_CMD: u16 = 0x0082;
    pub const WRITEREG_ACK: u16 = 0x0083;

    pub const REG_DEVICE_MODE: u32 = 0x0000_0000;
    pub const REG_DEVICE_MAC_HIGH: u32 = 0x0000_0008;
    pub const REG_DEVICE_MAC_LOW: u32 = 0x0000_000c;
    pub const REG_TIMESTAMP_CONTROL: u32 = 0x0000_0930;
    pub const REG_TIMESTAMP_HIGH: u32 = 0x0000_0934;
    pub const REG_TIMESTAMP_LOW: u32 = 0x0000_0938;
    pub const REG_WIDTH: u32 = 0x0003_0000;
    pub const REG_HEIGHT: u32 = 0x0003_0004;
    pub const REG_PAYLOAD_SIZE: u32 = 0x0003_0008;
    pub const REG_ACQUISITION_START: u32 = 0x0003_0100;
    pub const REG_ACQUISITION_STOP: u32 = 0x0003_0104;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct GvcpPacket {
        pub command: u16,
        pub request_id: u16,
        pub payload: Vec<u8>,
    }

    impl GvcpPacket {
        pub fn encode(&self) -> Vec<u8> {
            let mut bytes = Vec::with_capacity(8 + self.payload.len());
            bytes.extend_from_slice(&0x4201u16.to_be_bytes());
            bytes.extend_from_slice(&self.command.to_be_bytes());
            bytes.extend_from_slice(&(self.payload.len() as u16).to_be_bytes());
            bytes.extend_from_slice(&self.request_id.to_be_bytes());
            bytes.extend_from_slice(&self.payload);
            bytes
        }
    }

    pub fn discovery(request_id: u16) -> GvcpPacket {
        GvcpPacket {
            command: DISCOVERY_CMD,
            request_id,
            payload: Vec::new(),
        }
    }

    pub fn read_register(request_id: u16, address: u32) -> GvcpPacket {
        GvcpPacket {
            command: READREG_CMD,
            request_id,
            payload: address.to_be_bytes().to_vec(),
        }
    }

    pub fn write_register(request_id: u16, address: u32, value: u32) -> GvcpPacket {
        let mut payload = Vec::with_capacity(8);
        payload.extend_from_slice(&address.to_be_bytes());
        payload.extend_from_slice(&value.to_be_bytes());
        GvcpPacket {
            command: WRITEREG_CMD,
            request_id,
            payload,
        }
    }

    pub fn register_name(address: u32) -> &'static str {
        match address {
            REG_DEVICE_MODE => "DeviceMode",
            REG_DEVICE_MAC_HIGH => "DeviceMacHigh",
            REG_DEVICE_MAC_LOW => "DeviceMacLow",
            REG_TIMESTAMP_CONTROL => "TimestampControl",
            REG_TIMESTAMP_HIGH => "TimestampHigh",
            REG_TIMESTAMP_LOW => "TimestampLow",
            REG_WIDTH => "Width",
            REG_HEIGHT => "Height",
            REG_PAYLOAD_SIZE => "PayloadSize",
            REG_ACQUISITION_START => "AcquisitionStart",
            REG_ACQUISITION_STOP => "AcquisitionStop",
            _ => "VendorRegister",
        }
    }
}

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod gvsp {
    use numanager_core::{Error, ErrorCode, Result};
    use std::collections::BTreeMap;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct GvspBlockLeader {
        pub block_id: u64,
        pub expected_packets: u32,
        pub payload_size: usize,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct GvspPayloadPacket {
        pub block_id: u64,
        pub packet_id: u32,
        pub payload: Vec<u8>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct GvspBlockTrailer {
        pub block_id: u64,
        pub status: GvspBlockStatus,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum GvspBlockStatus {
        Complete,
        Incomplete,
        ResendRequested,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum GvspBlockPacket {
        Leader(GvspBlockLeader),
        Payload(GvspPayloadPacket),
        Trailer(GvspBlockTrailer),
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct GvspBlockReassembler {
        block_id: u64,
        expected_packets: Option<u32>,
        payload_size: Option<usize>,
        payloads: BTreeMap<u32, Vec<u8>>,
        trailer: Option<GvspBlockStatus>,
    }

    impl GvspBlockReassembler {
        pub fn new(block_id: u64) -> Self {
            Self {
                block_id,
                expected_packets: None,
                payload_size: None,
                payloads: BTreeMap::new(),
                trailer: None,
            }
        }

        pub fn block_id(&self) -> u64 {
            self.block_id
        }

        pub fn accept(&mut self, packet: GvspBlockPacket) -> Result<()> {
            match packet {
                GvspBlockPacket::Leader(leader) => {
                    self.check_block(leader.block_id)?;
                    if leader.expected_packets == 0 {
                        return Err(Error::new(
                            ErrorCode::Transport,
                            "GVSP leader must expect at least one payload packet",
                        ));
                    }
                    self.expected_packets = Some(leader.expected_packets);
                    self.payload_size = Some(leader.payload_size);
                }
                GvspBlockPacket::Payload(payload) => {
                    self.check_block(payload.block_id)?;
                    if payload.packet_id == 0 {
                        return Err(Error::new(
                            ErrorCode::Transport,
                            "GVSP payload packet ids are one-based",
                        ));
                    }
                    if let Some(expected) = self.expected_packets {
                        if payload.packet_id > expected {
                            return Err(Error::new(
                                ErrorCode::Transport,
                                format!(
                                    "GVSP payload packet {} exceeds expected count {}",
                                    payload.packet_id, expected
                                ),
                            ));
                        }
                    }
                    self.payloads.insert(payload.packet_id, payload.payload);
                }
                GvspBlockPacket::Trailer(trailer) => {
                    self.check_block(trailer.block_id)?;
                    self.trailer = Some(trailer.status);
                }
            }
            Ok(())
        }

        pub fn missing_packets(&self) -> Vec<u32> {
            let Some(expected) = self.expected_packets else {
                return Vec::new();
            };
            (1..=expected)
                .filter(|packet_id| !self.payloads.contains_key(packet_id))
                .collect()
        }

        pub fn is_complete(&self) -> bool {
            self.trailer == Some(GvspBlockStatus::Complete) && self.missing_packets().is_empty()
        }

        pub fn assembled_payload(&self) -> Result<Vec<u8>> {
            if !self.is_complete() {
                return Err(Error::new(
                    ErrorCode::Transport,
                    format!(
                        "GVSP block {} is incomplete; missing packets {:?}",
                        self.block_id,
                        self.missing_packets()
                    ),
                ));
            }
            let mut payload = Vec::with_capacity(self.payload_size.unwrap_or_default());
            for chunk in self.payloads.values() {
                payload.extend_from_slice(chunk);
            }
            if let Some(expected) = self.payload_size {
                if payload.len() != expected {
                    return Err(Error::new(
                        ErrorCode::Transport,
                        format!(
                            "GVSP block {} payload length {} did not match leader size {}",
                            self.block_id,
                            payload.len(),
                            expected
                        ),
                    ));
                }
            }
            Ok(payload)
        }

        fn check_block(&self, block_id: u64) -> Result<()> {
            if block_id == self.block_id {
                Ok(())
            } else {
                Err(Error::new(
                    ErrorCode::Transport,
                    format!(
                        "GVSP packet for block {} cannot be applied to block {}",
                        block_id, self.block_id
                    ),
                ))
            }
        }
    }
}

pub struct GigEVisionDiscovery {
    next_id: DriverId,
    probes: Vec<GigEVisionConfiguredProbe>,
}

impl GigEVisionDiscovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![GigEVisionConfiguredProbe::simulated()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), "gige_vision" | "gige-vision"))
            .map(GigEVisionConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for GigEVisionDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        Ok(self
            .probes
            .iter()
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                DriverCandidate::from_driver(
                    probe.discovery_label(),
                    Box::new(GigEVisionDriver::configured(id, probe.clone())),
                )
            })
            .collect())
    }
}

#[derive(Debug, Clone)]
pub struct GigEVisionConfiguredProbe {
    label: String,
    serial: String,
    width: u32,
    height: u32,
    exposure_s: f64,
    gain_db: f64,
    pixel_format: String,
    packet_size: i64,
    inter_packet_delay_ns: i64,
    stream_channel_port: i64,
    fixture_path: Option<String>,
    camera_address: Option<String>,
    connect: bool,
    gvcp_timeout_ms: u64,
    simulated: bool,
}

impl GigEVisionConfiguredProbe {
    pub fn simulated() -> Self {
        Self {
            label: "gige-vision-camera-0".into(),
            serial: "GV-SIM-0001".into(),
            width: 1280,
            height: 720,
            exposure_s: 0.01,
            gain_db: 0.0,
            pixel_format: "Mono8".into(),
            packet_size: 1500,
            inter_packet_delay_ns: 0,
            stream_channel_port: 49152,
            fixture_path: None,
            camera_address: None,
            connect: false,
            gvcp_timeout_ms: 500,
            simulated: true,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = Self::simulated();
        probe.simulated = false;
        probe.label = if device.label.is_empty() {
            string_prop(device, "label").unwrap_or_else(|| "configured-gige-vision-camera".into())
        } else {
            device.label.clone()
        };
        if let Some(serial) = string_prop(device, "serial_number") {
            probe.serial = serial;
        }
        if let Some(width) = pixel_count_prop(device, "width") {
            probe.width = width;
        }
        if let Some(height) = pixel_count_prop(device, "height") {
            probe.height = height;
        }
        if let Some(exposure) = time_interval_prop(device, "exposure", "exposure_s") {
            probe.exposure_s = exposure.seconds();
        }
        if let Some(gain) = decibel_prop(device, "gain", "gain_db") {
            probe.gain_db = gain.db();
        }
        if let Some(pixel_format) = string_prop(device, "pixel_format") {
            validate_pixel_format(&pixel_format, "GigE Vision")?;
            probe.pixel_format = pixel_format;
        }
        if let Some(packet_size) = byte_count_prop(device, "packet_size") {
            probe.packet_size = packet_size;
        }
        if let Some(delay) =
            time_interval_prop(device, "inter_packet_delay", "inter_packet_delay_s")
        {
            probe.inter_packet_delay_ns = delay.nanoseconds().round() as i64;
        }
        if let Some(delay) = i64_prop(device, "inter_packet_delay_ns") {
            probe.inter_packet_delay_ns = delay;
        }
        if let Some(port) = i64_prop(device, "stream_channel_port") {
            probe.stream_channel_port = port;
        }
        probe.fixture_path = string_prop(device, "fixture_path");
        probe.camera_address = string_prop(device, "camera_address")
            .or_else(|| string_prop(device, "host"))
            .or_else(|| string_prop(device, "ip_address"));
        probe.connect = bool_prop(device, "connect").unwrap_or(probe.connect);
        if let Some(timeout) = time_interval_prop(device, "gvcp_timeout", "gvcp_timeout_s") {
            probe.gvcp_timeout_ms = (timeout.seconds() * 1000.0)
                .round()
                .clamp(1.0, u64::MAX as f64) as u64;
        }
        if let Some(timeout) = i64_prop(device, "gvcp_timeout_ms") {
            probe.gvcp_timeout_ms = timeout.clamp(1, i64::MAX) as u64;
        }
        Ok(probe)
    }

    fn discovery_label(&self) -> String {
        if self.simulated {
            "Simulated GigE Vision camera".into()
        } else {
            format!("Configured GigE Vision camera {}", self.label)
        }
    }
}

pub struct GigEVisionDriver {
    id: DriverId,
    camera: DeviceId,
    control: ResourceId,
    stream: ResourceId,
    width: u32,
    height: u32,
    exposure_s: f64,
    gain_db: f64,
    pixel_format: String,
    packet_size: i64,
    inter_packet_delay_ns: i64,
    stream_channel_port: i64,
    next_request_id: u16,
    next_token: u64,
    events: VecDeque<DriverEvent>,
    worker_tx: Sender<DriverEvent>,
    worker_rx: Receiver<DriverEvent>,
    label: String,
    serial: String,
    fixture_path: Option<String>,
    camera_address: Option<String>,
    connect: bool,
    gvcp_timeout_ms: u64,
}

impl GigEVisionDriver {
    pub fn simulated(id: DriverId) -> Self {
        Self::configured(id, GigEVisionConfiguredProbe::simulated())
    }

    pub fn configured(id: DriverId, probe: GigEVisionConfiguredProbe) -> Self {
        let (worker_tx, worker_rx) = mpsc::channel();
        Self {
            id,
            camera: DeviceId(NodeId(id.0 * 1000 + 901)),
            control: ResourceId(NodeId(id.0 * 1000 + 902)),
            stream: ResourceId(NodeId(id.0 * 1000 + 903)),
            width: probe.width,
            height: probe.height,
            exposure_s: probe.exposure_s,
            gain_db: probe.gain_db,
            pixel_format: probe.pixel_format,
            packet_size: probe.packet_size,
            inter_packet_delay_ns: probe.inter_packet_delay_ns,
            stream_channel_port: probe.stream_channel_port,
            next_request_id: 1,
            next_token: 1,
            events: VecDeque::new(),
            worker_tx,
            worker_rx,
            label: probe.label,
            serial: probe.serial,
            fixture_path: probe.fixture_path,
            camera_address: probe.camera_address,
            connect: probe.connect,
            gvcp_timeout_ms: probe.gvcp_timeout_ms,
        }
    }

    fn next_token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn next_request_id(&mut self) -> u16 {
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1).max(1);
        id
    }

    fn descriptor(&self) -> DeviceDescriptor {
        DeviceDescriptor {
            id: self.camera,
            driver: self.id,
            label: self.label.clone(),
            vendor: Some("GigE Vision".into()),
            model: Some("GVCP/GVSP local model".into()),
            serial: Some(self.serial.clone()),
            kinds: vec![
                "camera".into(),
                "gige.vision".into(),
                "genicam.transport".into(),
                "trigger.sink".into(),
                "trigger.source".into(),
            ],
            properties: vec![
                property_range(
                    "width",
                    "Width",
                    ValueType::PixelCount,
                    Some("px"),
                    true,
                    Value::PixelCount(PixelCount::new(64)),
                    Value::PixelCount(PixelCount::new(8192)),
                    true,
                ),
                property_range(
                    "height",
                    "Height",
                    ValueType::PixelCount,
                    Some("px"),
                    true,
                    Value::PixelCount(PixelCount::new(64)),
                    Value::PixelCount(PixelCount::new(8192)),
                    true,
                ),
                property_range(
                    "exposure",
                    "Exposure",
                    ValueType::TimeInterval,
                    Some("s"),
                    true,
                    Value::TimeInterval(TimeInterval::from_microseconds(10.0)),
                    Value::TimeInterval(TimeInterval::from_seconds(60.0)),
                    true,
                ),
                property_range(
                    "gain",
                    "Gain",
                    ValueType::Decibel,
                    Some("dB"),
                    true,
                    Value::Decibel(Decibel::new(0.0)),
                    Value::Decibel(Decibel::new(48.0)),
                    true,
                ),
                property_enum(
                    "pixel_format",
                    "Pixel format",
                    true,
                    true,
                    ["Mono8", "Mono16", "BayerRG8", "Rgb8"],
                ),
                property_range(
                    "packet_size",
                    "GVSP packet size",
                    ValueType::ByteCount,
                    Some("bytes"),
                    true,
                    byte_count(576),
                    byte_count(9000),
                    false,
                ),
                property_range(
                    "inter_packet_delay",
                    "Inter-packet delay",
                    ValueType::TimeInterval,
                    Some("s"),
                    true,
                    Value::TimeInterval(TimeInterval::from_nanoseconds(0.0)),
                    Value::TimeInterval(TimeInterval::from_milliseconds(1.0)),
                    false,
                ),
                property_range(
                    "stream_channel_port",
                    "Stream channel port",
                    ValueType::I64,
                    None,
                    true,
                    Value::I64(1024),
                    Value::I64(65535),
                    false,
                ),
                property(
                    "hardware_timestamp",
                    "Hardware timestamp",
                    ValueType::Timestamp,
                    Some("controller_tick"),
                    false,
                ),
            ],
            metadata: {
                let mut metadata = BTreeMap::from([
                    ("standard".into(), Value::String("GigE Vision".into())),
                    ("control_protocol".into(), Value::String("GVCP".into())),
                    ("stream_protocol".into(), Value::String("GVSP".into())),
                    ("control_port".into(), Value::I64(gvcp::CONTROL_PORT as i64)),
                    ("sdk_free".into(), Value::Bool(true)),
                    (
                        "transport_strategy".into(),
                        Value::String(
                            "GVCP/GVSP model plus opt-in UDP GVCP raw-register control".into(),
                        ),
                    ),
                    ("chunk_metadata".into(), Value::Bool(true)),
                    ("hardware_timestamps".into(), Value::Bool(true)),
                    ("connected".into(), Value::Bool(self.active_gvcp())),
                    (
                        "gvcp_timeout".into(),
                        Value::TimeInterval(TimeInterval::from_milliseconds(
                            self.gvcp_timeout_ms as f64,
                        )),
                    ),
                ]);
                if let Some(address) = &self.camera_address {
                    metadata.insert("camera_address".into(), Value::String(address.clone()));
                }
                if let Some(path) = &self.fixture_path {
                    metadata.insert("fixture_path".into(), Value::String(path.clone()));
                }
                metadata
            },
        }
    }

    fn active_gvcp(&self) -> bool {
        self.connect && self.camera_address.is_some()
    }

    fn control_metadata(&self) -> BTreeMap<String, Value> {
        let mut metadata = BTreeMap::from([
            ("port".into(), Value::I64(gvcp::CONTROL_PORT as i64)),
            (
                "discovery".into(),
                Value::Bytes(gvcp::discovery(1).encode()),
            ),
            ("connected".into(), Value::Bool(self.active_gvcp())),
            (
                "transport".into(),
                Value::String(
                    if self.active_gvcp() {
                        "udp.gvcp.configured"
                    } else {
                        "fixture.gvcp"
                    }
                    .into(),
                ),
            ),
            (
                "gvcp_timeout".into(),
                Value::TimeInterval(TimeInterval::from_milliseconds(self.gvcp_timeout_ms as f64)),
            ),
        ]);
        if let Some(address) = &self.camera_address {
            metadata.insert("camera_address".into(), Value::String(address.clone()));
        }
        metadata
    }

    fn read_property(&self, key: &str) -> Result<Value> {
        let key = public_camera_key(key);
        match key {
            "width" => Ok(Value::PixelCount(PixelCount::new(self.width))),
            "height" => Ok(Value::PixelCount(PixelCount::new(self.height))),
            "exposure" => Ok(time_interval(self.exposure_s)),
            "gain" => Ok(Value::Decibel(Decibel::new(self.gain_db))),
            "pixel_format" => Ok(Value::String(self.pixel_format.clone())),
            "packet_size" => Ok(byte_count(self.packet_size)),
            "inter_packet_delay" | "inter_packet_delay_ns" => Ok(Value::TimeInterval(
                TimeInterval::from_nanoseconds(self.inter_packet_delay_ns as f64),
            )),
            "stream_channel_port" => Ok(Value::I64(self.stream_channel_port)),
            "hardware_timestamp" => Ok(timestamp(1_000_000)),
            other => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown GigE Vision property {other}"),
            )),
        }
    }

    fn validate_write(&self, key: &str, value: &Value) -> Result<()> {
        let key = public_camera_key(key);
        if key == "packet_size" && !matches!(value, Value::ByteCount(_)) {
            let packet_size = byte_count_i64(value)?.clamp(576, 9000);
            return self.validate_write("packet_size", &byte_count(packet_size));
        }
        let descriptor = self.descriptor();
        let schema = descriptor
            .properties
            .iter()
            .find(|schema| schema.key == key)
            .ok_or_else(|| {
                Error::new(ErrorCode::InvalidProperty, "unknown GigE Vision property")
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
                if matches!(value.as_str(), "Mono8" | "Mono16" | "BayerRG8" | "Rgb8") =>
            {
                Ok(())
            }
            ("pixel_format", Value::String(_)) => Err(Error::new(
                ErrorCode::InvalidProperty,
                "unsupported GigE Vision pixel_format",
            )),
            _ => Ok(()),
        }
    }

    fn apply_write(&mut self, key: &str, value: &Value) -> Result<()> {
        if public_camera_key(key) == "packet_size" {
            let packet_size = byte_count_i64(value)?.clamp(576, 9000);
            let canonical = byte_count(packet_size);
            self.validate_write("packet_size", &canonical)?;
            self.packet_size = packet_size;
            return Ok(());
        }

        self.validate_write(key, value)?;
        let key = public_camera_key(key);
        match (key, value) {
            ("width", Value::PixelCount(value)) => self.width = value.pixels().clamp(64, 8192),
            ("height", Value::PixelCount(value)) => self.height = value.pixels().clamp(64, 8192),
            ("exposure", value) => self.exposure_s = seconds(value)?,
            ("gain", Value::Decibel(value)) => self.gain_db = value.db(),
            ("pixel_format", Value::String(value)) => self.pixel_format = value.clone(),
            ("inter_packet_delay", value) | ("inter_packet_delay_ns", value) => {
                self.inter_packet_delay_ns = time_nanoseconds(value)?;
            }
            ("stream_channel_port", Value::I64(value)) => self.stream_channel_port = *value,
            _ => {}
        }
        Ok(())
    }

    fn gvcp_write_transaction(&mut self, key: &str, value: &Value) -> Option<PhysicalTransaction> {
        let (address, _, packet) = self.gvcp_write_packet(key, value)?;
        Some(PhysicalTransaction {
            resource: Some(self.control),
            description: format!(
                "GVCP WriteReg {} for {}",
                gvcp::register_name(address),
                public_camera_key(key)
            ),
            payload: Value::Bytes(packet),
        })
    }

    fn gvcp_write_packet(&mut self, key: &str, value: &Value) -> Option<(u32, u16, Vec<u8>)> {
        let key = public_camera_key(key);
        let (address, raw) = match (key, value) {
            ("width", Value::PixelCount(value)) => (gvcp::REG_WIDTH, value.pixels()),
            ("height", Value::PixelCount(value)) => (gvcp::REG_HEIGHT, value.pixels()),
            ("packet_size", value) => (0x000d_0404, byte_count_i64(value).ok()? as u32),
            ("stream_channel_port", Value::I64(value)) => (0x000d_0018, *value as u32),
            _ => return None,
        };
        let request_id = self.next_request_id();
        let packet = gvcp::write_register(request_id, address, raw).encode();
        Some((address, request_id, packet))
    }

    fn write_live_gvcp_property_if_mapped(
        &mut self,
        key: &str,
        value: &Value,
    ) -> Result<Option<GvcpAck>> {
        if !self.active_gvcp() {
            return Ok(None);
        }
        let Some((_address, request_id, packet)) = self.gvcp_write_packet(key, value) else {
            return Ok(None);
        };
        self.send_gvcp_packet(&packet, gvcp::WRITEREG_ACK, request_id)
            .map(Some)
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
                    "GigE Vision timing sequence must contain at least one value",
                ));
            }
            let schema = descriptor
                .properties
                .iter()
                .find(|schema| schema.key == sequence.property)
                .ok_or_else(|| {
                    Error::new(ErrorCode::InvalidProperty, "unknown GigE Vision property")
                })?;
            if !schema.sequenceable {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!(
                        "GigE Vision property {} is not sequenceable",
                        sequence.property
                    ),
                ));
            }
            for value in &sequence.values {
                self.validate_write(&sequence.property, value)?;
            }
        }
        Ok(())
    }

    fn timing_summary(&self, plan: &TimingPlan, phase: &str, applied: Value) -> Value {
        Value::Map(BTreeMap::from([
            ("phase".into(), Value::String(phase.into())),
            ("camera".into(), Value::I64(self.camera.0 .0 as i64)),
            (
                "width".into(),
                Value::PixelCount(PixelCount::new(self.width)),
            ),
            (
                "height".into(),
                Value::PixelCount(PixelCount::new(self.height)),
            ),
            ("exposure".into(), time_interval(self.exposure_s)),
            ("gain".into(), Value::Decibel(Decibel::new(self.gain_db))),
            (
                "pixel_format".into(),
                Value::String(self.pixel_format.clone()),
            ),
            (
                "sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
            ("applied".into(), applied),
        ]))
    }

    fn apply_timing_sequence_step(
        &mut self,
        plan: &TimingPlan,
        start: bool,
    ) -> Result<(Value, Vec<PhysicalTransaction>)> {
        let sequences = self
            .local_timing_sequences(plan)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let mut applied = BTreeMap::new();
        let mut transactions = Vec::new();
        for sequence in sequences {
            let value = (if start {
                sequence.values.first()
            } else {
                sequence.values.last()
            })
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidCommand,
                    "GigE Vision timing sequence must contain at least one value",
                )
            })?
            .clone();
            self.apply_write(&sequence.property, &value)?;
            let applied_value = self.read_property(&sequence.property)?;
            self.events
                .push_back(DriverEvent::Event(Event::PropertyChanged(
                    PropertyChanged {
                        device: sequence.device,
                        key: sequence.property.clone(),
                        value: applied_value.clone(),
                    },
                )));
            if let Some(transaction) = self.gvcp_write_transaction(&sequence.property, &value) {
                transactions.push(transaction);
            }
            applied.insert(
                format!("{}:{}", sequence.device.0 .0, sequence.property),
                applied_value,
            );
        }
        Ok((Value::Map(applied), transactions))
    }

    fn trigger_transaction(
        &mut self,
        kind: CapabilityKind,
        action: VisionTriggerAction,
    ) -> PhysicalTransaction {
        let request_id = self.next_request_id();
        let (address, value) = action.gvcp_register();
        let packet = gvcp::write_register(request_id, address, value).encode();
        PhysicalTransaction {
            resource: Some(self.control),
            description: format!("GVCP {} {}", kind.name(), gvcp::register_name(address)),
            payload: Value::Bytes(packet),
        }
    }

    fn invoke_trigger(&mut self, kind: CapabilityKind, action: VisionTriggerAction) -> Value {
        let request_id = self.next_request_id();
        let (address, value) = action.gvcp_register();
        let packet = gvcp::write_register(request_id, address, value).encode();
        let result = Value::Map(BTreeMap::from([
            ("protocol".into(), Value::String("GVCP".into())),
            ("capability".into(), Value::String(kind.name().into())),
            ("action".into(), Value::String(action.name().into())),
            (
                "register".into(),
                Value::String(gvcp::register_name(address).into()),
            ),
            ("address".into(), Value::String(format!("0x{address:08x}"))),
            ("value".into(), Value::I64(value as i64)),
            ("request_id".into(), Value::I64(request_id as i64)),
            ("packet".into(), Value::Bytes(packet)),
        ]));
        self.events
            .push_back(DriverEvent::Event(Event::Telemetry(TelemetryEvent {
                device: self.camera,
                values: BTreeMap::from([
                    ("protocol".into(), Value::String("GVCP".into())),
                    ("capability".into(), Value::String(kind.name().into())),
                    ("action".into(), Value::String(action.name().into())),
                    (
                        "register".into(),
                        Value::String(gvcp::register_name(address).into()),
                    ),
                    ("request_id".into(), Value::I64(request_id as i64)),
                ]),
            })));
        result
    }

    fn frame_metadata(
        frame_index: u64,
        hardware_timestamp: i64,
        packet_size: i64,
        inter_packet_delay_ns: i64,
        stream_channel_port: i64,
        exposure_s: f64,
        gain_db: f64,
    ) -> BTreeMap<String, Value> {
        BTreeMap::from([
            ("chunk_frame_id".into(), Value::I64(frame_index as i64)),
            ("hardware_timestamp".into(), timestamp(hardware_timestamp)),
            ("packet_size".into(), byte_count(packet_size)),
            (
                "inter_packet_delay".into(),
                Value::TimeInterval(TimeInterval::from_nanoseconds(inter_packet_delay_ns as f64)),
            ),
            (
                "stream_channel_port".into(),
                Value::I64(stream_channel_port),
            ),
            ("exposure".into(), time_interval(exposure_s)),
            ("gain".into(), Value::Decibel(Decibel::new(gain_db))),
            ("gvsp_status".into(), Value::String("complete".into())),
            ("chunk_metadata".into(), Value::Bool(true)),
        ])
    }
}

impl Driver for GigEVisionDriver {
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
                label: "gige-vision-gvcp".into(),
                kind: "udp.gvcp".into(),
                metadata: self.control_metadata(),
            },
            ResourceDescriptor {
                id: self.stream,
                driver: self.id,
                label: "gige-vision-gvsp".into(),
                kind: "udp.gvsp".into(),
                metadata: BTreeMap::from([
                    (
                        "stream_channel_port".into(),
                        Value::I64(self.stream_channel_port),
                    ),
                    ("packet_size".into(), byte_count(self.packet_size)),
                ]),
            },
        ]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device != self.camera {
            return Vec::new();
        }
        vec![
            capability(1, device, CapabilityKind::CameraCapture),
            capability(2, device, CapabilityKind::CameraStream),
            capability(3, device, CapabilityKind::TriggerSink),
            capability(4, device, CapabilityKind::TriggerSource),
            capability(5, device, CapabilityKind::RawRegisterAccess),
        ]
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut physical_transactions = Vec::new();
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } if *device == self.camera => {
                    let _ = self.read_property(key)?;
                    let address = match key.as_str() {
                        "width" => gvcp::REG_WIDTH,
                        "height" => gvcp::REG_HEIGHT,
                        "hardware_timestamp" => gvcp::REG_TIMESTAMP_LOW,
                        _ => gvcp::REG_DEVICE_MODE,
                    };
                    let packet = gvcp::read_register(self.next_request_id(), address);
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.control),
                        description: format!("GVCP ReadReg {}", gvcp::register_name(address)),
                        payload: Value::Bytes(packet.encode()),
                    });
                }
                Command::WriteProperty { device, key, value } if *device == self.camera => {
                    self.validate_write(key, value)?;
                    if let Some(transaction) = self.gvcp_write_transaction(key, value) {
                        physical_transactions.push(transaction);
                    }
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        if write.device == self.camera {
                            self.validate_write(&write.property, &write.value)?;
                        }
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.control),
                        description: "coalesced GigE Vision camera state set".into(),
                        payload: Value::List(
                            set.writes
                                .iter()
                                .filter_map(|write| {
                                    (write.device == self.camera).then(|| {
                                        Value::Map(BTreeMap::from([
                                            (
                                                "property".into(),
                                                Value::String(write.property.clone()),
                                            ),
                                            ("value".into(), write.value.clone()),
                                        ]))
                                    })
                                })
                                .collect(),
                        ),
                    });
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
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.control),
                        description: "GVCP AcquisitionStart for single capture".into(),
                        payload: Value::Bytes(
                            gvcp::write_register(
                                self.next_request_id(),
                                gvcp::REG_ACQUISITION_START,
                                1,
                            )
                            .encode(),
                        ),
                    });
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
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.control),
                        description: "GVCP AcquisitionStart for stream".into(),
                        payload: Value::Bytes(
                            gvcp::write_register(
                                self.next_request_id(),
                                gvcp::REG_ACQUISITION_START,
                                1,
                            )
                            .encode(),
                        ),
                    });
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if *device == self.camera
                    && matches!(*capability, CapabilityId(3) | CapabilityId(4)) =>
                {
                    let kind = if *capability == CapabilityId(3) {
                        CapabilityKind::TriggerSink
                    } else {
                        CapabilityKind::TriggerSource
                    };
                    let action = parse_vision_trigger_action(request, &kind)?;
                    physical_transactions.push(self.trigger_transaction(kind, action));
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if *device == self.camera && *capability == CapabilityId(5) => {
                    let raw = parse_raw_register_request(request)?;
                    physical_transactions.push(self.raw_register_transaction(&raw)?);
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
                Command::ReadProperty { device, key } if device == self.camera => {
                    result = self.read_property(&key)?;
                }
                Command::WriteProperty { device, key, value } if device == self.camera => {
                    self.validate_write(&key, &value)?;
                    self.write_live_gvcp_property_if_mapped(&key, &value)?;
                    self.apply_write(&key, &value)?;
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
                            self.validate_write(&write.property, &write.value)?;
                            self.write_live_gvcp_property_if_mapped(&write.property, &write.value)?;
                            self.apply_write(&write.property, &write.value)?;
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
                    let capture = match request {
                        CapabilityRequest::CameraCapture(request) => request,
                        CapabilityRequest::None => CameraCaptureRequest::default_frame(),
                        _ => unreachable!(),
                    };
                    spawn_frames(FrameJob {
                        tx: self.worker_tx.clone(),
                        token,
                        device,
                        width: self.width,
                        height: self.height,
                        exposure_s: self.exposure_s,
                        gain_db: self.gain_db,
                        pixel_format: requested_pixel_format(capture.encoding, &self.pixel_format),
                        packet_size: self.packet_size,
                        inter_packet_delay_ns: self.inter_packet_delay_ns,
                        stream_channel_port: self.stream_channel_port,
                        frame_count: 1,
                        buffer: capture.buffer.unwrap_or_default(),
                        fixture_path: self.fixture_path.clone(),
                    });
                    return Ok(token);
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if device == self.camera && capability == CapabilityId(2) => {
                    let request = match request {
                        CapabilityRequest::CameraStream(request) => request,
                        _ => unreachable!(),
                    };
                    spawn_frames(FrameJob {
                        tx: self.worker_tx.clone(),
                        token,
                        device,
                        width: self.width,
                        height: self.height,
                        exposure_s: self.exposure_s,
                        gain_db: self.gain_db,
                        pixel_format: requested_pixel_format(request.encoding, &self.pixel_format),
                        packet_size: self.packet_size,
                        inter_packet_delay_ns: self.inter_packet_delay_ns,
                        stream_channel_port: self.stream_channel_port,
                        frame_count: request.frame_count.unwrap_or(8),
                        buffer: request.buffer,
                        fixture_path: self.fixture_path.clone(),
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
                    let kind = if capability == CapabilityId(3) {
                        CapabilityKind::TriggerSink
                    } else {
                        CapabilityKind::TriggerSource
                    };
                    let action = parse_owned_vision_trigger_action(request, &kind)?;
                    result = self.invoke_trigger(kind, action);
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if device == self.camera && capability == CapabilityId(5) => {
                    let raw = parse_owned_raw_register_request(request)?;
                    result = self.invoke_raw_register(raw)?;
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
                resource: Some(self.control),
                description: "GigE Vision timing arm summary".into(),
                payload: self.timing_summary(plan, "arm", Value::Null),
            }],
        })
    }

    fn start_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let (applied, mut transactions) = self.apply_timing_sequence_step(&armed.plan, true)?;
        transactions.push(PhysicalTransaction {
            resource: Some(self.control),
            description: "GigE Vision timing start summary".into(),
            payload: self.timing_summary(&armed.plan, "start", applied),
        });
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Start(OperationId(0))],
            physical_transactions: transactions,
        })
    }

    fn stop_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let (applied, mut transactions) = self.apply_timing_sequence_step(&armed.plan, false)?;
        transactions.push(PhysicalTransaction {
            resource: Some(self.control),
            description: "GigE Vision timing stop summary".into(),
            payload: self.timing_summary(&armed.plan, "stop", applied),
        });
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions: transactions,
        })
    }
}

#[derive(Debug, Clone)]
enum RawRegisterRequest {
    Read {
        address: u32,
        node: Option<String>,
    },
    Write {
        address: u32,
        node: Option<String>,
        value: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisionTriggerAction {
    Enable,
    Disable,
    Pulse,
}

impl VisionTriggerAction {
    fn name(self) -> &'static str {
        match self {
            VisionTriggerAction::Enable => "enable",
            VisionTriggerAction::Disable => "disable",
            VisionTriggerAction::Pulse => "pulse",
        }
    }

    fn gvcp_register(self) -> (u32, u32) {
        match self {
            VisionTriggerAction::Enable | VisionTriggerAction::Pulse => {
                (gvcp::REG_ACQUISITION_START, 1)
            }
            VisionTriggerAction::Disable => (gvcp::REG_ACQUISITION_STOP, 1),
        }
    }
}

impl GigEVisionDriver {
    fn raw_register_transaction(
        &mut self,
        request: &RawRegisterRequest,
    ) -> Result<PhysicalTransaction> {
        let packet = match request {
            RawRegisterRequest::Read { address, .. } => {
                gvcp::read_register(self.next_request_id(), *address)
            }
            RawRegisterRequest::Write { address, value, .. } => {
                gvcp::write_register(self.next_request_id(), *address, *value)
            }
        };
        Ok(PhysicalTransaction {
            resource: Some(self.control),
            description: match request {
                RawRegisterRequest::Read { address, node } => {
                    format!(
                        "GVCP RawRegisterAccess read {}",
                        raw_register_label(*address, node.as_deref())
                    )
                }
                RawRegisterRequest::Write { address, node, .. } => {
                    format!(
                        "GVCP RawRegisterAccess write {}",
                        raw_register_label(*address, node.as_deref())
                    )
                }
            },
            payload: Value::Bytes(packet.encode()),
        })
    }

    fn invoke_raw_register(&mut self, request: RawRegisterRequest) -> Result<Value> {
        let request_id = self.next_request_id();
        let (operation, address, node, value, packet) = match request.clone() {
            RawRegisterRequest::Read { address, node } => {
                let packet = gvcp::read_register(request_id, address).encode();
                if self.active_gvcp() {
                    return self.invoke_live_raw_register(
                        "read".into(),
                        address,
                        node,
                        None,
                        request_id,
                        packet,
                    );
                }
                let value = self.raw_register_value(address);
                ("read", address, node, value, packet)
            }
            RawRegisterRequest::Write {
                address,
                node,
                value,
            } => {
                let packet = gvcp::write_register(request_id, address, value).encode();
                if self.active_gvcp() {
                    return self.invoke_live_raw_register(
                        "write".into(),
                        address,
                        node,
                        Some(value),
                        request_id,
                        packet,
                    );
                }
                self.apply_raw_register_write(address, value);
                ("write", address, node, value, packet)
            }
        };
        let mut values = BTreeMap::from([
            ("protocol".into(), Value::String("GVCP".into())),
            ("operation".into(), Value::String(operation.into())),
            ("address".into(), Value::String(format!("0x{address:08x}"))),
            (
                "register".into(),
                Value::String(gvcp::register_name(address).into()),
            ),
            ("request_id".into(), Value::I64(request_id as i64)),
            ("value".into(), Value::I64(value as i64)),
            ("packet".into(), Value::Bytes(packet)),
        ]);
        if let Some(node) = node {
            values.insert("node".into(), Value::String(node));
        }
        Ok(Value::Map(values))
    }

    fn invoke_live_raw_register(
        &mut self,
        operation: String,
        address: u32,
        node: Option<String>,
        write_value: Option<u32>,
        request_id: u16,
        packet: Vec<u8>,
    ) -> Result<Value> {
        let expected_ack = if write_value.is_some() {
            gvcp::WRITEREG_ACK
        } else {
            gvcp::READREG_ACK
        };
        let ack = self.send_gvcp_packet(&packet, expected_ack, request_id)?;
        let value = if let Some(value) = write_value {
            self.apply_raw_register_write(address, value);
            value
        } else {
            ack.read_value.ok_or_else(|| {
                Error::new(
                    ErrorCode::Transport,
                    "GVCP ReadReg ACK did not contain a register value",
                )
            })?
        };
        let mut values = BTreeMap::from([
            ("protocol".into(), Value::String("GVCP".into())),
            (
                "transport".into(),
                Value::String("udp.gvcp.configured".into()),
            ),
            ("operation".into(), Value::String(operation)),
            ("address".into(), Value::String(format!("0x{address:08x}"))),
            (
                "register".into(),
                Value::String(gvcp::register_name(address).into()),
            ),
            ("request_id".into(), Value::I64(request_id as i64)),
            ("value".into(), Value::I64(value as i64)),
            ("packet".into(), Value::Bytes(packet)),
            (
                "ack_command".into(),
                Value::String(format!("0x{:04x}", ack.command)),
            ),
            ("ack_status".into(), Value::I64(ack.status as i64)),
            (
                "ack_payload_length".into(),
                Value::I64(ack.payload.len() as i64),
            ),
            ("peer".into(), Value::String(ack.peer)),
        ]);
        if let Some(node) = node {
            values.insert("node".into(), Value::String(node));
        }
        Ok(Value::Map(values))
    }

    fn send_gvcp_packet(
        &self,
        packet: &[u8],
        expected_ack: u16,
        request_id: u16,
    ) -> Result<GvcpAck> {
        let address = self.camera_address.as_deref().ok_or_else(|| {
            Error::new(
                ErrorCode::Transport,
                "GigE Vision GVCP camera_address is not configured",
            )
        })?;
        let target = format!("{address}:{}", gvcp::CONTROL_PORT)
            .to_socket_addrs()
            .map_err(|error| {
                Error::new(
                    ErrorCode::Transport,
                    format!("resolve GigE Vision camera_address {address}: {error}"),
                )
            })?
            .next()
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::Transport,
                    format!("resolve GigE Vision camera_address {address}: no socket address"),
                )
            })?;
        let socket = UdpSocket::bind("0.0.0.0:0").map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("bind local GVCP UDP socket: {error}"),
            )
        })?;
        socket
            .set_read_timeout(Some(Duration::from_millis(self.gvcp_timeout_ms)))
            .map_err(|error| {
                Error::new(ErrorCode::Transport, format!("set GVCP timeout: {error}"))
            })?;
        socket.send_to(packet, target).map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("send GVCP packet to {target}: {error}"),
            )
        })?;
        let mut buf = [0u8; 1024];
        let (len, peer) = socket.recv_from(&mut buf).map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("receive GVCP ACK from {target}: {error}"),
            )
        })?;
        parse_gvcp_ack(&buf[..len], expected_ack, request_id, peer.to_string())
    }

    fn raw_register_value(&self, address: u32) -> u32 {
        match address {
            gvcp::REG_WIDTH => self.width,
            gvcp::REG_HEIGHT => self.height,
            gvcp::REG_PAYLOAD_SIZE => self.width.saturating_mul(self.height),
            gvcp::REG_TIMESTAMP_LOW => 1_000_000,
            gvcp::REG_TIMESTAMP_HIGH => 0,
            0x000d_0404 => self.packet_size as u32,
            0x000d_0018 => self.stream_channel_port as u32,
            _ => 0,
        }
    }

    fn apply_raw_register_write(&mut self, address: u32, value: u32) {
        match address {
            gvcp::REG_WIDTH => self.width = value.clamp(64, 8192),
            gvcp::REG_HEIGHT => self.height = value.clamp(64, 8192),
            0x000d_0404 => self.packet_size = i64::from(value),
            0x000d_0018 => self.stream_channel_port = i64::from(value.clamp(1024, 65535)),
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
struct GvcpAck {
    status: u16,
    command: u16,
    payload: Vec<u8>,
    read_value: Option<u32>,
    peer: String,
}

fn parse_gvcp_ack(
    bytes: &[u8],
    expected_ack: u16,
    request_id: u16,
    peer: String,
) -> Result<GvcpAck> {
    if bytes.len() < 8 {
        return Err(Error::new(
            ErrorCode::Transport,
            format!("short GVCP ACK from {peer}: {} bytes", bytes.len()),
        ));
    }
    let status = u16::from_be_bytes([bytes[0], bytes[1]]);
    let command = u16::from_be_bytes([bytes[2], bytes[3]]);
    let length = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
    let ack_request_id = u16::from_be_bytes([bytes[6], bytes[7]]);
    if command != expected_ack {
        return Err(Error::new(
            ErrorCode::Transport,
            format!("GVCP ACK command 0x{command:04x} did not match expected 0x{expected_ack:04x}"),
        ));
    }
    if ack_request_id != request_id {
        return Err(Error::new(
            ErrorCode::Transport,
            format!("GVCP ACK request id {ack_request_id} did not match {request_id}"),
        ));
    }
    if bytes.len() < 8 + length {
        return Err(Error::new(
            ErrorCode::Transport,
            format!(
                "short GVCP ACK payload from {peer}: header length {length}, packet {} bytes",
                bytes.len()
            ),
        ));
    }
    if status != 0 {
        return Err(Error::new(
            ErrorCode::Transport,
            format!("GVCP ACK status 0x{status:04x} from {peer}"),
        ));
    }
    let payload = bytes[8..8 + length].to_vec();
    let read_value = if command == gvcp::READREG_ACK && payload.len() >= 4 {
        Some(u32::from_be_bytes([
            payload[0], payload[1], payload[2], payload[3],
        ]))
    } else {
        None
    };
    Ok(GvcpAck {
        status,
        command,
        payload,
        read_value,
        peer,
    })
}

struct FrameJob {
    tx: Sender<DriverEvent>,
    token: DriverToken,
    device: DeviceId,
    width: u32,
    height: u32,
    exposure_s: f64,
    gain_db: f64,
    pixel_format: String,
    packet_size: i64,
    inter_packet_delay_ns: i64,
    stream_channel_port: i64,
    frame_count: u64,
    buffer: FrameBufferSpec,
    fixture_path: Option<String>,
}

fn spawn_frames(job: FrameJob) {
    thread::spawn(move || {
        let stream = StreamId(job.token.0);
        let mut completed_width = job.width;
        let mut completed_height = job.height;
        for index in 0..job.frame_count {
            let scene = match load_fixture_frame(&job) {
                Ok(scene) => scene,
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
            completed_width = scene.width;
            completed_height = scene.height;
            let handle = FrameHandle {
                stream,
                frame: FrameId(index),
            };
            let timestamp = 1_000_000 + (index as i64 * 10_000);
            let mut metadata = GigEVisionDriver::frame_metadata(
                index,
                timestamp,
                job.packet_size,
                job.inter_packet_delay_ns,
                job.stream_channel_port,
                job.exposure_s,
                job.gain_db,
            );
            if let Some(path) = &job.fixture_path {
                metadata.insert("source".into(), Value::String("fixture_path".into()));
                metadata.insert("fixture_path".into(), Value::String(path.clone()));
            }
            let _ = job.tx.send(DriverEvent::FrameReady(Frame {
                handle,
                device: job.device,
                width: scene.width,
                height: scene.height,
                pixel_format: job.pixel_format.clone(),
                data: scene.pixels,
                metadata,
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
                ("transport".into(), Value::String("GVSP".into())),
            ])),
        });
    });
}

fn load_fixture_frame(job: &FrameJob) -> Result<crate::platform_camera::PlatformFixtureFrame> {
    let Some(path) = job.fixture_path.as_deref() else {
        let scene = crate::sim::gel_scene(job.width, job.height, job.exposure_s);
        return Ok(crate::platform_camera::PlatformFixtureFrame {
            width: scene.width,
            height: scene.height,
            pixels: scene.pixels,
        });
    };
    let bytes = fs::read(path).map_err(|error| {
        Error::new(
            ErrorCode::Transport,
            format!("read GigE Vision fixture {path}: {error}"),
        )
    })?;
    crate::platform_camera::decode_portable_pixmap(&bytes, &job.pixel_format)
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
    sequenceable: bool,
) -> PropertySchema {
    let mut schema = property(key, display_name, value_type, unit, writable);
    schema.range = Some(Range { min, max });
    schema.sequenceable = sequenceable;
    schema
}

fn property_enum<const N: usize>(
    key: &str,
    display_name: &str,
    writable: bool,
    sequenceable: bool,
    values: [&str; N],
) -> PropertySchema {
    let mut schema = property(key, display_name, ValueType::String, None, writable);
    schema.enum_values = values
        .into_iter()
        .map(|value| EnumValue {
            value: Value::String(value.into()),
            label: value.into(),
        })
        .collect();
    schema.sequenceable = sequenceable;
    schema
}

fn requested_pixel_format(encoding: Option<ImageEncoding>, configured: &str) -> String {
    match encoding.unwrap_or(ImageEncoding::Native) {
        ImageEncoding::Native => configured,
        ImageEncoding::Mono8 => "Mono8",
        ImageEncoding::Mono16 => "Mono16",
        ImageEncoding::Rgb8 => "Rgb8",
        ImageEncoding::Bgr8 => "BGR8",
        ImageEncoding::Raw8 => "Mono8",
        ImageEncoding::Raw16 => "Mono16",
    }
    .into()
}

fn validate_pixel_format(pixel_format: &str, family: &str) -> Result<()> {
    if matches!(pixel_format, "Mono8" | "Mono16" | "BayerRG8" | "Rgb8") {
        Ok(())
    } else {
        Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unsupported {family} pixel_format {pixel_format}"),
        ))
    }
}

fn string_prop(device: &DeviceConfig, key: &str) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
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

fn byte_count_prop(device: &DeviceConfig, key: &str) -> Option<i64> {
    match device.properties.get(key) {
        Some(Value::ByteCount(value)) => i64::try_from(value.bytes()).ok(),
        Some(Value::I64(value)) if *value >= 0 => Some(*value),
        Some(Value::F64(value)) if value.is_finite() && *value >= 0.0 => Some(*value as i64),
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

fn decibel_prop(device: &DeviceConfig, key: &str, legacy_db_key: &str) -> Option<Decibel> {
    match device.properties.get(key) {
        Some(Value::Decibel(value)) => Some(*value),
        Some(Value::F64(value)) => Some(Decibel::new(*value)),
        Some(Value::I64(value)) => Some(Decibel::new(*value as f64)),
        _ => match device.properties.get(legacy_db_key) {
            Some(Value::Decibel(value)) => Some(*value),
            Some(Value::F64(value)) => Some(Decibel::new(*value)),
            Some(Value::I64(value)) => Some(Decibel::new(*value as f64)),
            _ => None,
        },
    }
}

fn i64_prop(device: &DeviceConfig, key: &str) -> Option<i64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => Some(*value),
        _ => None,
    }
}

fn bool_prop(device: &DeviceConfig, key: &str) -> Option<bool> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn parse_raw_register_request(request: &CapabilityRequest) -> Result<RawRegisterRequest> {
    let CapabilityRequest::GenericCommand(request) = request else {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            "RawRegisterAccess expects GenericCommand request",
        ));
    };
    raw_register_request_from_generic(request)
}

fn parse_owned_raw_register_request(request: CapabilityRequest) -> Result<RawRegisterRequest> {
    let CapabilityRequest::GenericCommand(request) = request else {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            "RawRegisterAccess expects GenericCommand request",
        ));
    };
    raw_register_request_from_generic(&request)
}

fn parse_vision_trigger_action(
    request: &CapabilityRequest,
    kind: &CapabilityKind,
) -> Result<VisionTriggerAction> {
    match request {
        CapabilityRequest::None => Ok(VisionTriggerAction::Pulse),
        CapabilityRequest::Trigger(request) => match request.action {
            TriggerAction::Enable => Ok(VisionTriggerAction::Enable),
            TriggerAction::Disable => Ok(VisionTriggerAction::Disable),
            TriggerAction::Pulse => Ok(VisionTriggerAction::Pulse),
        },
        _ => Err(Error::new(
            ErrorCode::Unsupported,
            format!("{} expects None or CapabilityRequest::Trigger", kind.name()),
        )),
    }
}

fn parse_owned_vision_trigger_action(
    request: CapabilityRequest,
    kind: &CapabilityKind,
) -> Result<VisionTriggerAction> {
    parse_vision_trigger_action(&request, kind)
}

fn raw_register_request_from_generic(
    request: &GenericCommandRequest,
) -> Result<RawRegisterRequest> {
    if request.is_hidden_maintenance() {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            format!(
                "GenericCommand {} is a hidden maintenance operation",
                request.command
            ),
        ));
    }
    let target = raw_register_target(request)?;
    match request.command.as_str() {
        "read" | "ReadRegister" | "read_register" => Ok(RawRegisterRequest::Read {
            address: target.address,
            node: target.node,
        }),
        "write" | "WriteRegister" | "write_register" => {
            if target.node.is_none() {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "RawRegisterAccess writes require a named public node target",
                ));
            }
            let value = request
                .params
                .get("value")
                .ok_or_else(|| {
                    Error::new(
                        ErrorCode::InvalidCommand,
                        "RawRegisterAccess write missing value",
                    )
                })
                .and_then(value_u32)?;
            Ok(RawRegisterRequest::Write {
                address: target.address,
                node: target.node,
                value,
            })
        }
        other => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("unsupported RawRegisterAccess command {other}"),
        )),
    }
}

#[derive(Debug, Clone)]
struct RawRegisterTarget {
    address: u32,
    node: Option<String>,
}

fn raw_register_target(request: &GenericCommandRequest) -> Result<RawRegisterTarget> {
    if let Some(address) = request.params.get("address") {
        return Ok(RawRegisterTarget {
            address: value_u32(address)?,
            node: None,
        });
    }
    let node = request
        .params
        .get("node")
        .or_else(|| request.params.get("genicam_node"))
        .ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidCommand,
                "RawRegisterAccess missing address or node",
            )
        })
        .and_then(value_string)?;
    let address = gige_genicam_node_address(&node).ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidCommand,
            format!("unsupported GigE Vision GenICam node {node}"),
        )
    })?;
    Ok(RawRegisterTarget {
        address,
        node: Some(node),
    })
}

fn gige_genicam_node_address(node: &str) -> Option<u32> {
    match normalized_node_name(node).as_str() {
        "devicemode" => Some(gvcp::REG_DEVICE_MODE),
        "width" => Some(gvcp::REG_WIDTH),
        "height" => Some(gvcp::REG_HEIGHT),
        "payloadsize" => Some(gvcp::REG_PAYLOAD_SIZE),
        "timestampcontrol" => Some(gvcp::REG_TIMESTAMP_CONTROL),
        "timestamphigh" => Some(gvcp::REG_TIMESTAMP_HIGH),
        "timestamplow" | "timestampvalue" => Some(gvcp::REG_TIMESTAMP_LOW),
        "acquisitionstart" => Some(gvcp::REG_ACQUISITION_START),
        "acquisitionstop" => Some(gvcp::REG_ACQUISITION_STOP),
        _ => None,
    }
}

fn normalized_node_name(node: &str) -> String {
    node.chars()
        .filter(|ch| *ch != '_' && *ch != '-' && !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn raw_register_label(address: u32, node: Option<&str>) -> String {
    match node {
        Some(node) => format!("{node} ({})", gvcp::register_name(address)),
        None => gvcp::register_name(address).into(),
    }
}

fn value_string(value: &Value) -> Result<String> {
    match value {
        Value::String(value) => Ok(value.clone()),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("expected GenICam node name string, got {value:?}"),
        )),
    }
}

fn value_u32(value: &Value) -> Result<u32> {
    match value {
        Value::I64(value) if *value >= 0 && *value <= u32::MAX as i64 => Ok(*value as u32),
        Value::String(value) => parse_u32_address(value),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("expected u32 raw-register value, got {value:?}"),
        )),
    }
}

fn parse_u32_address(value: &str) -> Result<u32> {
    let trimmed = value.trim();
    let parsed = if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16)
    } else {
        trimmed.parse::<u32>()
    };
    parsed.map_err(|_| {
        Error::new(
            ErrorCode::InvalidCommand,
            format!("invalid u32 raw-register value {value}"),
        )
    })
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

fn time_nanoseconds(value: &Value) -> Result<i64> {
    match value {
        Value::TimeInterval(value) => Ok(value.nanoseconds().round() as i64),
        Value::I64(value) => Ok(*value),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("expected time interval, got {other:?}"),
        )),
    }
}

fn time_interval(seconds: f64) -> Value {
    Value::TimeInterval(TimeInterval::from_seconds(seconds))
}

fn byte_count(bytes: i64) -> Value {
    Value::ByteCount(ByteCount::new(bytes.max(0) as u64))
}

fn byte_count_i64(value: &Value) -> Result<i64> {
    match value {
        Value::ByteCount(value) => i64::try_from(value.bytes()).map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                "byte count exceeds supported GigE Vision register range",
            )
        }),
        Value::I64(value) if *value >= 0 => Ok(*value),
        Value::F64(value) if value.is_finite() && *value >= 0.0 => Ok(*value as i64),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("expected byte count, got {other:?}"),
        )),
    }
}

fn timestamp(ticks: i64) -> Value {
    Value::Timestamp(Timestamp::from_controller_ticks(ticks))
}

fn public_camera_key(key: &str) -> &str {
    match key {
        "inter_packet_delay_ns" => "inter_packet_delay",
        _ => key,
    }
}
