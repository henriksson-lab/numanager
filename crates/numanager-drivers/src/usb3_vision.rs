use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

#[cfg(feature = "os-usb")]
use nusb::transfer::RequestBuffer;
#[cfg(feature = "os-usb")]
use nusb::Interface;

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod u3v {
    use numanager_core::{Error, ErrorCode, Result};

    pub const CONTROL_ENDPOINT: u8 = 0;
    pub const STREAM_ENDPOINT: u8 = 1;
    pub const READMEM_CMD: u16 = 0x0800;
    pub const READMEM_ACK: u16 = 0x0801;
    pub const WRITEMEM_CMD: u16 = 0x0802;
    pub const WRITEMEM_ACK: u16 = 0x0803;
    pub const EVENT_CMD: u16 = 0x0c00;

    pub const REG_MANIFEST_TABLE: u64 = 0x0000_0000;
    pub const REG_DEVICE_CAPABILITY: u64 = 0x0000_0100;
    pub const REG_TIMESTAMP_CONTROL: u64 = 0x0000_0930;
    pub const REG_TIMESTAMP_VALUE: u64 = 0x0000_0938;
    pub const REG_WIDTH: u64 = 0x0003_0000;
    pub const REG_HEIGHT: u64 = 0x0003_0004;
    pub const REG_PAYLOAD_SIZE: u64 = 0x0003_0008;
    pub const REG_ACQUISITION_START: u64 = 0x0003_0100;
    pub const REG_ACQUISITION_STOP: u64 = 0x0003_0104;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct U3vControlPacket {
        pub command: u16,
        pub request_id: u16,
        pub payload: Vec<u8>,
    }

    impl U3vControlPacket {
        pub fn encode(&self) -> Vec<u8> {
            let mut bytes = Vec::with_capacity(8 + self.payload.len());
            bytes.extend_from_slice(&self.command.to_le_bytes());
            bytes.extend_from_slice(&(self.payload.len() as u16).to_le_bytes());
            bytes.extend_from_slice(&self.request_id.to_le_bytes());
            bytes.extend_from_slice(&0u16.to_le_bytes());
            bytes.extend_from_slice(&self.payload);
            bytes
        }
    }

    pub fn read_memory(request_id: u16, address: u64, byte_count: u32) -> U3vControlPacket {
        let mut payload = Vec::with_capacity(12);
        payload.extend_from_slice(&address.to_le_bytes());
        payload.extend_from_slice(&byte_count.to_le_bytes());
        U3vControlPacket {
            command: READMEM_CMD,
            request_id,
            payload,
        }
    }

    pub fn write_memory(request_id: u16, address: u64, data: &[u8]) -> U3vControlPacket {
        let mut payload = Vec::with_capacity(12 + data.len());
        payload.extend_from_slice(&address.to_le_bytes());
        payload.extend_from_slice(&(data.len() as u32).to_le_bytes());
        payload.extend_from_slice(data);
        U3vControlPacket {
            command: WRITEMEM_CMD,
            request_id,
            payload,
        }
    }

    pub fn write_u32(request_id: u16, address: u64, value: u32) -> U3vControlPacket {
        write_memory(request_id, address, &value.to_le_bytes())
    }

    pub fn register_name(address: u64) -> &'static str {
        match address {
            REG_MANIFEST_TABLE => "ManifestTable",
            REG_DEVICE_CAPABILITY => "DeviceCapability",
            REG_TIMESTAMP_CONTROL => "TimestampControl",
            REG_TIMESTAMP_VALUE => "TimestampValue",
            REG_WIDTH => "Width",
            REG_HEIGHT => "Height",
            REG_PAYLOAD_SIZE => "PayloadSize",
            REG_ACQUISITION_START => "AcquisitionStart",
            REG_ACQUISITION_STOP => "AcquisitionStop",
            _ => "VendorRegister",
        }
    }

    pub fn decode_ack(bytes: &[u8], expected_command: u16, request_id: u16) -> Result<Vec<u8>> {
        if bytes.len() < 8 {
            return Err(Error::new(
                ErrorCode::Transport,
                "U3V command ACK shorter than header",
            ));
        }
        let command = u16::from_le_bytes([bytes[0], bytes[1]]);
        if command != expected_command {
            return Err(Error::new(
                ErrorCode::Transport,
                format!(
                    "U3V command ACK 0x{command:04x} did not match expected 0x{expected_command:04x}"
                ),
            ));
        }
        let payload_len = u16::from_le_bytes([bytes[2], bytes[3]]) as usize;
        let actual_request_id = u16::from_le_bytes([bytes[4], bytes[5]]);
        if actual_request_id != request_id {
            return Err(Error::new(
                ErrorCode::Transport,
                format!(
                    "U3V command ACK request id {actual_request_id} did not match {request_id}"
                ),
            ));
        }
        let end = 8usize.checked_add(payload_len).ok_or_else(|| {
            Error::new(
                ErrorCode::Transport,
                "U3V command ACK payload length overflow",
            )
        })?;
        if bytes.len() < end {
            return Err(Error::new(
                ErrorCode::Transport,
                format!(
                    "U3V command ACK payload length {payload_len} exceeds received {} bytes",
                    bytes.len()
                ),
            ));
        }
        Ok(bytes[8..end].to_vec())
    }
}

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod u3v_stream {
    use numanager_core::{Error, ErrorCode, Result};
    use std::collections::BTreeMap;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct U3vStreamLeader {
        pub block_id: u64,
        pub expected_transfers: u32,
        pub payload_size: usize,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct U3vBulkTransfer {
        pub block_id: u64,
        pub transfer_id: u32,
        pub payload: Vec<u8>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct U3vStreamTrailer {
        pub block_id: u64,
        pub status: U3vStreamStatus,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum U3vStreamStatus {
        Complete,
        Incomplete,
        Cancelled,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum U3vStreamPacket {
        Leader(U3vStreamLeader),
        Bulk(U3vBulkTransfer),
        Trailer(U3vStreamTrailer),
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct U3vBlockReassembler {
        block_id: u64,
        expected_transfers: Option<u32>,
        payload_size: Option<usize>,
        transfers: BTreeMap<u32, Vec<u8>>,
        trailer: Option<U3vStreamStatus>,
    }

    impl U3vBlockReassembler {
        pub fn new(block_id: u64) -> Self {
            Self {
                block_id,
                expected_transfers: None,
                payload_size: None,
                transfers: BTreeMap::new(),
                trailer: None,
            }
        }

        pub fn block_id(&self) -> u64 {
            self.block_id
        }

        pub fn accept(&mut self, packet: U3vStreamPacket) -> Result<()> {
            match packet {
                U3vStreamPacket::Leader(leader) => {
                    self.check_block(leader.block_id)?;
                    if leader.expected_transfers == 0 {
                        return Err(Error::new(
                            ErrorCode::Transport,
                            "U3V stream leader must expect at least one bulk transfer",
                        ));
                    }
                    self.expected_transfers = Some(leader.expected_transfers);
                    self.payload_size = Some(leader.payload_size);
                }
                U3vStreamPacket::Bulk(transfer) => {
                    self.check_block(transfer.block_id)?;
                    if transfer.transfer_id == 0 {
                        return Err(Error::new(
                            ErrorCode::Transport,
                            "U3V bulk transfer ids are one-based",
                        ));
                    }
                    if let Some(expected) = self.expected_transfers {
                        if transfer.transfer_id > expected {
                            return Err(Error::new(
                                ErrorCode::Transport,
                                format!(
                                    "U3V bulk transfer {} exceeds expected count {}",
                                    transfer.transfer_id, expected
                                ),
                            ));
                        }
                    }
                    self.transfers
                        .insert(transfer.transfer_id, transfer.payload);
                }
                U3vStreamPacket::Trailer(trailer) => {
                    self.check_block(trailer.block_id)?;
                    self.trailer = Some(trailer.status);
                }
            }
            Ok(())
        }

        pub fn missing_transfers(&self) -> Vec<u32> {
            let Some(expected) = self.expected_transfers else {
                return Vec::new();
            };
            (1..=expected)
                .filter(|transfer_id| !self.transfers.contains_key(transfer_id))
                .collect()
        }

        pub fn is_complete(&self) -> bool {
            self.trailer == Some(U3vStreamStatus::Complete) && self.missing_transfers().is_empty()
        }

        pub fn assembled_payload(&self) -> Result<Vec<u8>> {
            if !self.is_complete() {
                return Err(Error::new(
                    ErrorCode::Transport,
                    format!(
                        "U3V stream block {} is incomplete; missing transfers {:?}",
                        self.block_id,
                        self.missing_transfers()
                    ),
                ));
            }
            let mut payload = Vec::with_capacity(self.payload_size.unwrap_or_default());
            for chunk in self.transfers.values() {
                payload.extend_from_slice(chunk);
            }
            if let Some(expected) = self.payload_size {
                if payload.len() != expected {
                    return Err(Error::new(
                        ErrorCode::Transport,
                        format!(
                            "U3V stream block {} payload length {} did not match leader size {}",
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
                        "U3V stream packet for block {} cannot be applied to block {}",
                        block_id, self.block_id
                    ),
                ))
            }
        }
    }
}

#[derive(Debug, Clone)]
struct Usb3VisionUsbIdentity {
    product: String,
    serial: Option<String>,
    vendor_id: u16,
    product_id: u16,
    bus_number: u8,
    device_address: u8,
}

impl Usb3VisionUsbIdentity {
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
        ]);
        if let Some(serial) = &self.serial {
            fields.insert("serial".into(), Value::String(serial.clone()));
        }
        Value::Map(fields)
    }
}

#[derive(Debug, Clone)]
struct Usb3VisionEndpointSummary {
    bulk_in: Vec<u8>,
    bulk_out: Vec<u8>,
    interrupt_in: Vec<u8>,
    interrupt_out: Vec<u8>,
}

impl Usb3VisionEndpointSummary {
    fn value(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("bulk_in".into(), endpoint_list_value(&self.bulk_in)),
            ("bulk_out".into(), endpoint_list_value(&self.bulk_out)),
            (
                "interrupt_in".into(),
                endpoint_list_value(&self.interrupt_in),
            ),
            (
                "interrupt_out".into(),
                endpoint_list_value(&self.interrupt_out),
            ),
        ]))
    }

    fn control_boundary(&self) -> &'static str {
        if self.bulk_in.is_empty() || self.bulk_out.is_empty() {
            "descriptor endpoints do not include a bulk IN/OUT pair for U3V control bring-up"
        } else {
            "descriptor endpoints include bulk IN/OUT candidates; live U3V memory transfer requires explicit or single-candidate command endpoints"
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Usb3VisionCommandEndpoints {
    in_endpoint: u8,
    out_endpoint: u8,
}

fn endpoint_list_value(endpoints: &[u8]) -> Value {
    Value::List(
        endpoints
            .iter()
            .map(|endpoint| Value::String(format!("0x{endpoint:02x}")))
            .collect(),
    )
}

pub struct Usb3VisionDiscovery {
    next_id: DriverId,
    probes: Vec<Usb3VisionConfiguredProbe>,
}

impl Usb3VisionDiscovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![Usb3VisionConfiguredProbe::simulated()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), "usb3_vision" | "usb3-vision"))
            .map(Usb3VisionConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for Usb3VisionDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        Ok(self
            .probes
            .iter()
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                DriverCandidate::from_driver(
                    probe.discovery_label(),
                    Box::new(Usb3VisionDriver::configured(id, probe.clone())),
                )
            })
            .collect())
    }
}

#[derive(Debug, Clone)]
pub struct Usb3VisionConfiguredProbe {
    label: String,
    serial: String,
    width: u32,
    height: u32,
    exposure_s: f64,
    gain_db: f64,
    pixel_format: String,
    transfer_size: i64,
    transfer_queue_depth: i64,
    stream_endpoint: i64,
    fixture_path: Option<String>,
    connect: bool,
    usb_vendor_id: Option<u16>,
    usb_product_id: Option<u16>,
    usb_interface: u8,
    command_in_endpoint: Option<u8>,
    command_out_endpoint: Option<u8>,
    command_ack_size: usize,
    command_timeout_ms: u64,
    usb_serial_number: Option<String>,
    simulated: bool,
}

impl Usb3VisionConfiguredProbe {
    pub fn simulated() -> Self {
        Self {
            label: "usb3-vision-camera-0".into(),
            serial: "U3V-SIM-0001".into(),
            width: 1920,
            height: 1080,
            exposure_s: 0.01,
            gain_db: 0.0,
            pixel_format: "Mono8".into(),
            transfer_size: 1_048_576,
            transfer_queue_depth: 16,
            stream_endpoint: u3v::STREAM_ENDPOINT as i64,
            fixture_path: None,
            connect: false,
            usb_vendor_id: None,
            usb_product_id: None,
            usb_interface: 0,
            command_in_endpoint: None,
            command_out_endpoint: None,
            command_ack_size: 4096,
            command_timeout_ms: 500,
            usb_serial_number: None,
            simulated: true,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = Self::simulated();
        probe.simulated = false;
        probe.label = if device.label.is_empty() {
            string_prop(device, "label").unwrap_or_else(|| "configured-usb3-vision-camera".into())
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
            validate_pixel_format(&pixel_format, "USB3 Vision")?;
            probe.pixel_format = pixel_format;
        }
        if let Some(transfer_size) = byte_count_prop(device, "transfer_size") {
            probe.transfer_size = transfer_size;
        }
        if let Some(queue_depth) = i64_prop(device, "transfer_queue_depth") {
            probe.transfer_queue_depth = queue_depth;
        }
        if let Some(endpoint) = i64_prop(device, "stream_endpoint") {
            probe.stream_endpoint = endpoint;
        }
        probe.fixture_path = string_prop(device, "fixture_path");
        probe.connect = bool_prop(device, "connect").unwrap_or(probe.connect);
        probe.usb_vendor_id = u16_prop(device, "usb_vendor_id").or(probe.usb_vendor_id);
        probe.usb_product_id = u16_prop(device, "usb_product_id").or(probe.usb_product_id);
        probe.usb_interface = u16_prop(device, "usb_interface")
            .map(|value| value.min(u8::MAX as u16) as u8)
            .unwrap_or(probe.usb_interface);
        probe.command_in_endpoint = u16_prop(device, "command_in_endpoint")
            .map(|value| value.min(u8::MAX as u16) as u8)
            .or(probe.command_in_endpoint);
        probe.command_out_endpoint = u16_prop(device, "command_out_endpoint")
            .map(|value| value.min(u8::MAX as u16) as u8)
            .or(probe.command_out_endpoint);
        probe.command_ack_size = u64_prop(device, "command_ack_size")
            .map(|value| value.clamp(8, 1_048_576) as usize)
            .unwrap_or(probe.command_ack_size);
        probe.command_timeout_ms =
            u64_prop(device, "command_timeout_ms").unwrap_or(probe.command_timeout_ms);
        probe.usb_serial_number = string_prop(device, "usb_serial_number");
        Ok(probe)
    }

    fn discovery_label(&self) -> String {
        if self.simulated {
            "Simulated USB3 Vision camera".into()
        } else {
            format!("Configured USB3 Vision camera {}", self.label)
        }
    }
}

#[cfg(feature = "os-usb")]
fn open_usb3_vision_interface(
    probe: &Usb3VisionConfiguredProbe,
) -> (
    Option<Usb3VisionUsbIdentity>,
    String,
    Option<Interface>,
    Option<Usb3VisionEndpointSummary>,
) {
    if !probe.connect {
        return (None, "not_requested".into(), None, None);
    }
    let Some(vendor_id) = probe.usb_vendor_id else {
        return (
            None,
            "usb_vendor_id is required when connect=true".into(),
            None,
            None,
        );
    };
    let Some(product_id) = probe.usb_product_id else {
        return (
            None,
            "usb_product_id is required when connect=true".into(),
            None,
            None,
        );
    };
    let devices = match nusb::list_devices() {
        Ok(devices) => devices,
        Err(error) => {
            return (
                None,
                format!("USB3 Vision device listing failed: {error}"),
                None,
                None,
            )
        }
    };
    let Some(device_info) = devices
        .filter(|device| device.vendor_id() == vendor_id && device.product_id() == product_id)
        .find(|device| {
            probe
                .usb_serial_number
                .as_deref()
                .map(|serial| device.serial_number() == Some(serial))
                .unwrap_or(true)
        })
    else {
        return (
            None,
            format!("configured USB3 Vision device {vendor_id:04x}:{product_id:04x} not found"),
            None,
            None,
        );
    };
    let identity = Usb3VisionUsbIdentity {
        product: device_info
            .product_string()
            .unwrap_or("USB3 Vision camera")
            .into(),
        serial: device_info.serial_number().map(str::to_string),
        vendor_id: device_info.vendor_id(),
        product_id: device_info.product_id(),
        bus_number: device_info.bus_number(),
        device_address: device_info.device_address(),
    };
    let device = match device_info.open() {
        Ok(device) => device,
        Err(error) => {
            return (
                Some(identity),
                format!("USB3 Vision device open failed: {error}"),
                None,
                None,
            )
        }
    };
    let endpoint_summary = usb3_vision_endpoint_summary(&device, probe.usb_interface);
    let interface = match device.detach_and_claim_interface(probe.usb_interface) {
        Ok(interface) => interface,
        Err(error) => {
            let hint = crate::usb_discovery::usb_claim_hint(
                identity.vendor_id,
                identity.product_id,
                probe.usb_interface,
            );
            return (
                Some(identity),
                format!(
                    "USB3 Vision interface {} claim failed: {error}{hint}",
                    probe.usb_interface
                ),
                None,
                endpoint_summary,
            );
        }
    };
    (
        Some(identity),
        format!("claimed configured USB interface {}", probe.usb_interface),
        Some(interface),
        endpoint_summary,
    )
}

#[cfg(feature = "os-usb")]
fn usb3_vision_endpoint_summary(
    device: &nusb::Device,
    interface_number: u8,
) -> Option<Usb3VisionEndpointSummary> {
    let configuration = device.active_configuration().ok()?;
    let mut summary = Usb3VisionEndpointSummary {
        bulk_in: Vec::new(),
        bulk_out: Vec::new(),
        interrupt_in: Vec::new(),
        interrupt_out: Vec::new(),
    };
    for interface in configuration.interface_alt_settings() {
        if interface.interface_number() != interface_number || interface.alternate_setting() != 0 {
            continue;
        }
        for endpoint in interface.endpoints() {
            match (endpoint.transfer_type(), endpoint.direction()) {
                (nusb::transfer::EndpointType::Bulk, nusb::transfer::Direction::In) => {
                    summary.bulk_in.push(endpoint.address())
                }
                (nusb::transfer::EndpointType::Bulk, nusb::transfer::Direction::Out) => {
                    summary.bulk_out.push(endpoint.address())
                }
                (nusb::transfer::EndpointType::Interrupt, nusb::transfer::Direction::In) => {
                    summary.interrupt_in.push(endpoint.address())
                }
                (nusb::transfer::EndpointType::Interrupt, nusb::transfer::Direction::Out) => {
                    summary.interrupt_out.push(endpoint.address())
                }
                _ => {}
            }
        }
    }
    Some(summary)
}

pub struct Usb3VisionDriver {
    id: DriverId,
    camera: DeviceId,
    control: ResourceId,
    stream: ResourceId,
    event: ResourceId,
    width: u32,
    height: u32,
    exposure_s: f64,
    gain_db: f64,
    pixel_format: String,
    transfer_size: i64,
    transfer_queue_depth: i64,
    stream_endpoint: i64,
    next_request_id: u16,
    next_token: u64,
    events: VecDeque<DriverEvent>,
    worker_tx: Sender<DriverEvent>,
    worker_rx: Receiver<DriverEvent>,
    label: String,
    serial: String,
    fixture_path: Option<String>,
    connect: bool,
    usb_vendor_id: Option<u16>,
    usb_product_id: Option<u16>,
    usb_interface: u8,
    usb_serial_number: Option<String>,
    command_in_endpoint: Option<u8>,
    command_out_endpoint: Option<u8>,
    command_ack_size: usize,
    command_timeout_ms: u64,
    live_identity: Option<Usb3VisionUsbIdentity>,
    live_state: String,
    live_endpoints: Option<Usb3VisionEndpointSummary>,
    #[cfg(feature = "os-usb")]
    live_interface: Option<Interface>,
}

impl Usb3VisionDriver {
    pub fn simulated(id: DriverId) -> Self {
        Self::configured(id, Usb3VisionConfiguredProbe::simulated())
    }

    pub fn configured(id: DriverId, probe: Usb3VisionConfiguredProbe) -> Self {
        let (worker_tx, worker_rx) = mpsc::channel();
        #[cfg(feature = "os-usb")]
        let (live_identity, live_state, live_interface, live_endpoints) =
            open_usb3_vision_interface(&probe);
        #[cfg(not(feature = "os-usb"))]
        let (live_identity, live_state, live_endpoints) = if probe.connect {
            (
                None,
                "os-usb feature is required for configured USB3 Vision device opening".into(),
                None,
            )
        } else {
            (None, "not_requested".into(), None)
        };
        Self {
            id,
            camera: DeviceId(NodeId(id.0 * 1000 + 921)),
            control: ResourceId(NodeId(id.0 * 1000 + 922)),
            stream: ResourceId(NodeId(id.0 * 1000 + 923)),
            event: ResourceId(NodeId(id.0 * 1000 + 924)),
            width: probe.width,
            height: probe.height,
            exposure_s: probe.exposure_s,
            gain_db: probe.gain_db,
            pixel_format: probe.pixel_format,
            transfer_size: probe.transfer_size,
            transfer_queue_depth: probe.transfer_queue_depth,
            stream_endpoint: probe.stream_endpoint,
            next_request_id: 1,
            next_token: 1,
            events: VecDeque::new(),
            worker_tx,
            worker_rx,
            label: probe.label,
            serial: probe.serial,
            fixture_path: probe.fixture_path,
            connect: probe.connect,
            usb_vendor_id: probe.usb_vendor_id,
            usb_product_id: probe.usb_product_id,
            usb_interface: probe.usb_interface,
            usb_serial_number: probe.usb_serial_number,
            command_in_endpoint: probe.command_in_endpoint,
            command_out_endpoint: probe.command_out_endpoint,
            command_ack_size: probe.command_ack_size,
            command_timeout_ms: probe.command_timeout_ms,
            live_identity,
            live_state,
            live_endpoints,
            #[cfg(feature = "os-usb")]
            live_interface,
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
            vendor: Some("USB3 Vision".into()),
            model: Some("U3V fixture".into()),
            serial: self
                .live_identity
                .as_ref()
                .and_then(|identity| identity.serial.clone())
                .or_else(|| Some(self.serial.clone())),
            kinds: vec![
                "camera".into(),
                "usb3.vision".into(),
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
                    "transfer_size",
                    "USB transfer size",
                    ValueType::ByteCount,
                    Some("bytes"),
                    true,
                    byte_count(16_384),
                    byte_count(16_777_216),
                    false,
                ),
                property_range(
                    "transfer_queue_depth",
                    "Transfer queue depth",
                    ValueType::I64,
                    None,
                    true,
                    Value::I64(1),
                    Value::I64(256),
                    false,
                ),
                property_range(
                    "stream_endpoint",
                    "Stream endpoint",
                    ValueType::I64,
                    None,
                    false,
                    Value::I64(1),
                    Value::I64(15),
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
                    ("standard".into(), Value::String("USB3 Vision".into())),
                    (
                        "control_protocol".into(),
                        Value::String("U3V Control".into()),
                    ),
                    ("stream_protocol".into(), Value::String("U3V Stream".into())),
                    (
                        "control_endpoint".into(),
                        Value::I64(u3v::CONTROL_ENDPOINT as i64),
                    ),
                    (
                        "stream_endpoint".into(),
                        Value::I64(u3v::STREAM_ENDPOINT as i64),
                    ),
                    ("sdk_free".into(), Value::Bool(true)),
                    (
                        "transport_strategy".into(),
                        Value::String(
                            "U3V control/stream model plus opt-in USB identity/open path".into(),
                        ),
                    ),
                    ("chunk_metadata".into(), Value::Bool(true)),
                    ("hardware_timestamps".into(), Value::Bool(true)),
                    ("connect_requested".into(), Value::Bool(self.connect)),
                    (
                        "connected".into(),
                        Value::Bool(self.live_identity.is_some()),
                    ),
                    ("live_state".into(), Value::String(self.live_state.clone())),
                ]);
                if let Some(identity) = &self.live_identity {
                    metadata.insert("usb_identity".into(), identity.value());
                }
                if let Some(vid) = self.usb_vendor_id {
                    metadata.insert("usb_vendor_id".into(), Value::I64(vid as i64));
                }
                if let Some(pid) = self.usb_product_id {
                    metadata.insert("usb_product_id".into(), Value::I64(pid as i64));
                }
                if let Some(serial) = &self.usb_serial_number {
                    metadata.insert("usb_serial_number".into(), Value::String(serial.clone()));
                }
                metadata.insert(
                    "usb_interface".into(),
                    Value::I64(self.usb_interface as i64),
                );
                if let Some(endpoint) = self.command_in_endpoint {
                    metadata.insert(
                        "command_in_endpoint".into(),
                        Value::String(format!("0x{endpoint:02x}")),
                    );
                }
                if let Some(endpoint) = self.command_out_endpoint {
                    metadata.insert(
                        "command_out_endpoint".into(),
                        Value::String(format!("0x{endpoint:02x}")),
                    );
                }
                if let Some(path) = &self.fixture_path {
                    metadata.insert("fixture_path".into(), Value::String(path.clone()));
                }
                metadata
            },
        }
    }

    fn control_metadata(&self) -> BTreeMap<String, Value> {
        let mut metadata = BTreeMap::from([
            ("endpoint".into(), Value::I64(u3v::CONTROL_ENDPOINT as i64)),
            (
                "manifest_read".into(),
                Value::Bytes(u3v::read_memory(1, u3v::REG_MANIFEST_TABLE, 512).encode()),
            ),
            ("connect_requested".into(), Value::Bool(self.connect)),
            (
                "connected".into(),
                Value::Bool(self.live_identity.is_some()),
            ),
            ("usb_claimed".into(), Value::Bool(self.live_usb_claimed())),
            ("live_state".into(), Value::String(self.live_state.clone())),
            (
                "usb_interface".into(),
                Value::I64(self.usb_interface as i64),
            ),
            (
                "command_ack_size".into(),
                Value::I64(self.command_ack_size as i64),
            ),
            (
                "command_timeout".into(),
                Value::TimeInterval(TimeInterval::from_milliseconds(
                    self.command_timeout_ms as f64,
                )),
            ),
            (
                "transport".into(),
                Value::String(
                    if self.live_usb_claimed() {
                        "usb.u3v.configured"
                    } else {
                        "fixture.u3v"
                    }
                    .into(),
                ),
            ),
        ]);
        if let Some(identity) = &self.live_identity {
            metadata.insert("usb_identity".into(), identity.value());
        }
        if let Some(endpoints) = &self.live_endpoints {
            metadata.insert("descriptor_endpoints".into(), endpoints.value());
            metadata.insert(
                "control_boundary".into(),
                Value::String(endpoints.control_boundary().into()),
            );
        }
        if let Some(endpoints) = self.command_endpoints() {
            metadata.insert(
                "command_in_endpoint".into(),
                Value::String(format!("0x{:02x}", endpoints.in_endpoint)),
            );
            metadata.insert(
                "command_out_endpoint".into(),
                Value::String(format!("0x{:02x}", endpoints.out_endpoint)),
            );
            metadata.insert("live_u3v_memory".into(), Value::Bool(true));
        } else {
            metadata.insert("live_u3v_memory".into(), Value::Bool(false));
        }
        if let Some(vid) = self.usb_vendor_id {
            metadata.insert("usb_vendor_id".into(), Value::I64(vid as i64));
        }
        if let Some(pid) = self.usb_product_id {
            metadata.insert("usb_product_id".into(), Value::I64(pid as i64));
        }
        if let Some(serial) = &self.usb_serial_number {
            metadata.insert("usb_serial_number".into(), Value::String(serial.clone()));
        }
        metadata
    }

    fn live_usb_claimed(&self) -> bool {
        #[cfg(feature = "os-usb")]
        {
            self.live_interface.is_some()
        }
        #[cfg(not(feature = "os-usb"))]
        {
            false
        }
    }

    fn command_endpoints(&self) -> Option<Usb3VisionCommandEndpoints> {
        match (self.command_in_endpoint, self.command_out_endpoint) {
            (Some(in_endpoint), Some(out_endpoint)) => Some(Usb3VisionCommandEndpoints {
                in_endpoint,
                out_endpoint,
            }),
            _ => {
                let endpoints = self.live_endpoints.as_ref()?;
                if endpoints.bulk_in.len() == 1 && endpoints.bulk_out.len() == 1 {
                    Some(Usb3VisionCommandEndpoints {
                        in_endpoint: endpoints.bulk_in[0],
                        out_endpoint: endpoints.bulk_out[0],
                    })
                } else {
                    None
                }
            }
        }
    }

    fn read_property(&self, key: &str) -> Result<Value> {
        match key {
            "width" => Ok(Value::PixelCount(PixelCount::new(self.width))),
            "height" => Ok(Value::PixelCount(PixelCount::new(self.height))),
            "exposure" => Ok(time_interval(self.exposure_s)),
            "gain" => Ok(Value::Decibel(Decibel::new(self.gain_db))),
            "pixel_format" => Ok(Value::String(self.pixel_format.clone())),
            "transfer_size" => Ok(byte_count(self.transfer_size)),
            "transfer_queue_depth" => Ok(Value::I64(self.transfer_queue_depth)),
            "stream_endpoint" => Ok(Value::I64(self.stream_endpoint)),
            "hardware_timestamp" => Ok(timestamp(2_000_000)),
            other => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown USB3 Vision property {other}"),
            )),
        }
    }

    fn validate_write(&self, key: &str, value: &Value) -> Result<()> {
        let descriptor = self.descriptor();
        let schema = descriptor
            .properties
            .iter()
            .find(|schema| schema.key == key)
            .ok_or_else(|| {
                Error::new(ErrorCode::InvalidProperty, "unknown USB3 Vision property")
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
                "unsupported USB3 Vision pixel_format",
            )),
            _ => Ok(()),
        }
    }

    fn u3v_write_transaction(&mut self, key: &str, value: &Value) -> Option<PhysicalTransaction> {
        let (address, raw) = match (key, value) {
            ("width", Value::PixelCount(value)) => (u3v::REG_WIDTH, value.pixels()),
            ("height", Value::PixelCount(value)) => (u3v::REG_HEIGHT, value.pixels()),
            ("transfer_size", value) => (0x000d_0000, byte_count_i64(value).ok()? as u32),
            ("transfer_queue_depth", Value::I64(value)) => (0x000d_0004, *value as u32),
            _ => return None,
        };
        let packet = u3v::write_u32(self.next_request_id(), address, raw);
        Some(PhysicalTransaction {
            resource: Some(self.control),
            description: format!("U3V WriteMem {} for {key}", u3v::register_name(address)),
            payload: Value::Bytes(packet.encode()),
        })
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
                    "USB3 Vision timing sequence must contain at least one value",
                ));
            }
            let schema = descriptor
                .properties
                .iter()
                .find(|schema| schema.key == sequence.property)
                .ok_or_else(|| {
                    Error::new(ErrorCode::InvalidProperty, "unknown USB3 Vision property")
                })?;
            if !schema.sequenceable {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!(
                        "USB3 Vision property {} is not sequenceable",
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
                    "USB3 Vision timing sequence must contain at least one value",
                )
            })?
            .clone();
            let completion = self.write_property_live_or_local(&sequence.property, &value)?;
            let applied_value = self.read_property(&sequence.property)?;
            self.events
                .push_back(DriverEvent::Event(Event::PropertyChanged(
                    PropertyChanged {
                        device: sequence.device,
                        key: sequence.property.clone(),
                        value: applied_value.clone(),
                    },
                )));
            transactions.push(PhysicalTransaction {
                resource: Some(self.control),
                description: format!("USB3 Vision timing write {}", sequence.property),
                payload: completion,
            });
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
        let (address, value) = action.u3v_register();
        let packet = u3v::write_u32(request_id, address, value).encode();
        PhysicalTransaction {
            resource: Some(self.control),
            description: format!("U3V {} {}", kind.name(), u3v::register_name(address)),
            payload: Value::Bytes(packet),
        }
    }

    fn invoke_trigger(
        &mut self,
        kind: CapabilityKind,
        action: VisionTriggerAction,
    ) -> Result<Value> {
        let request_id = self.next_request_id();
        let (address, value) = action.u3v_register();
        let packet = u3v::write_u32(request_id, address, value);
        let packet_bytes = packet.encode();
        let ack = self.send_u3v_packet(packet, u3v::WRITEMEM_ACK)?;
        let mut result = BTreeMap::from([
            ("protocol".into(), Value::String("U3V".into())),
            ("capability".into(), Value::String(kind.name().into())),
            ("action".into(), Value::String(action.name().into())),
            (
                "register".into(),
                Value::String(u3v::register_name(address).into()),
            ),
            ("address".into(), Value::String(format!("0x{address:016x}"))),
            ("value".into(), Value::I64(value as i64)),
            ("request_id".into(), Value::I64(request_id as i64)),
            ("packet".into(), Value::Bytes(packet_bytes)),
            ("live".into(), Value::Bool(ack.is_some())),
        ]);
        if let Some(ack) = ack {
            result.insert("ack".into(), Value::Bytes(ack));
        }
        self.events
            .push_back(DriverEvent::Event(Event::Telemetry(TelemetryEvent {
                device: self.camera,
                values: BTreeMap::from([
                    ("protocol".into(), Value::String("U3V".into())),
                    ("capability".into(), Value::String(kind.name().into())),
                    ("action".into(), Value::String(action.name().into())),
                    (
                        "register".into(),
                        Value::String(u3v::register_name(address).into()),
                    ),
                    ("request_id".into(), Value::I64(request_id as i64)),
                ]),
            })));
        Ok(Value::Map(result))
    }

    fn frame_metadata(
        frame_index: u64,
        hardware_timestamp: i64,
        transfer_size: i64,
        transfer_queue_depth: i64,
        stream_endpoint: i64,
        exposure_s: f64,
        gain_db: f64,
    ) -> BTreeMap<String, Value> {
        BTreeMap::from([
            ("chunk_frame_id".into(), Value::I64(frame_index as i64)),
            ("hardware_timestamp".into(), timestamp(hardware_timestamp)),
            ("transfer_size".into(), byte_count(transfer_size)),
            (
                "transfer_queue_depth".into(),
                Value::I64(transfer_queue_depth),
            ),
            ("stream_endpoint".into(), Value::I64(stream_endpoint)),
            ("exposure".into(), time_interval(exposure_s)),
            ("gain".into(), Value::Decibel(Decibel::new(gain_db))),
            ("u3v_stream_status".into(), Value::String("complete".into())),
            ("chunk_metadata".into(), Value::Bool(true)),
        ])
    }
}

impl Driver for Usb3VisionDriver {
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
                label: "usb3-vision-control".into(),
                kind: "usb.u3v.control".into(),
                metadata: self.control_metadata(),
            },
            ResourceDescriptor {
                id: self.stream,
                driver: self.id,
                label: "usb3-vision-stream".into(),
                kind: "usb.u3v.stream".into(),
                metadata: BTreeMap::from([
                    ("endpoint".into(), Value::I64(self.stream_endpoint)),
                    ("transfer_size".into(), byte_count(self.transfer_size)),
                    (
                        "transfer_queue_depth".into(),
                        Value::I64(self.transfer_queue_depth),
                    ),
                ]),
            },
            ResourceDescriptor {
                id: self.event,
                driver: self.id,
                label: "usb3-vision-event".into(),
                kind: "usb.u3v.event".into(),
                metadata: BTreeMap::from([(
                    "event_command".into(),
                    Value::I64(u3v::EVENT_CMD as i64),
                )]),
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
                        "width" => u3v::REG_WIDTH,
                        "height" => u3v::REG_HEIGHT,
                        "hardware_timestamp" => u3v::REG_TIMESTAMP_VALUE,
                        _ => u3v::REG_DEVICE_CAPABILITY,
                    };
                    let packet = u3v::read_memory(self.next_request_id(), address, 4);
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.control),
                        description: format!("U3V ReadMem {}", u3v::register_name(address)),
                        payload: Value::Bytes(packet.encode()),
                    });
                }
                Command::WriteProperty { device, key, value } if *device == self.camera => {
                    self.validate_write(key, value)?;
                    if let Some(transaction) = self.u3v_write_transaction(key, value) {
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
                        description: "coalesced USB3 Vision camera state set".into(),
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
                        description: "U3V AcquisitionStart for single capture".into(),
                        payload: Value::Bytes(
                            u3v::write_u32(self.next_request_id(), u3v::REG_ACQUISITION_START, 1)
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
                        description: "U3V AcquisitionStart for stream".into(),
                        payload: Value::Bytes(
                            u3v::write_u32(self.next_request_id(), u3v::REG_ACQUISITION_START, 1)
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
                    result = self.write_property_live_or_local(&key, &value)?;
                    let value = self.read_property(&key)?;
                    self.events
                        .push_back(DriverEvent::Event(Event::PropertyChanged(
                            PropertyChanged { device, key, value },
                        )));
                }
                Command::ApplyStateSet(set) => {
                    let mut values = BTreeMap::new();
                    for write in set.writes {
                        if write.device == self.camera {
                            let completion =
                                self.write_property_live_or_local(&write.property, &write.value)?;
                            values.insert(write.property.clone(), completion);
                            let value = self.read_property(&write.property)?;
                            self.events
                                .push_back(DriverEvent::Event(Event::PropertyChanged(
                                    PropertyChanged {
                                        device: write.device,
                                        key: write.property,
                                        value,
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
                        transfer_size: self.transfer_size,
                        transfer_queue_depth: self.transfer_queue_depth,
                        stream_endpoint: self.stream_endpoint,
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
                        transfer_size: self.transfer_size,
                        transfer_queue_depth: self.transfer_queue_depth,
                        stream_endpoint: self.stream_endpoint,
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
                    result = self.invoke_trigger(kind, action)?;
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
                description: "USB3 Vision timing arm summary".into(),
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
            description: "USB3 Vision timing start summary".into(),
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
            description: "USB3 Vision timing stop summary".into(),
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
        address: u64,
        node: Option<String>,
        byte_count: u32,
    },
    Write {
        address: u64,
        node: Option<String>,
        bytes: Vec<u8>,
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

    fn u3v_register(self) -> (u64, u32) {
        match self {
            VisionTriggerAction::Enable | VisionTriggerAction::Pulse => {
                (u3v::REG_ACQUISITION_START, 1)
            }
            VisionTriggerAction::Disable => (u3v::REG_ACQUISITION_STOP, 1),
        }
    }
}

impl Usb3VisionDriver {
    fn send_u3v_packet(
        &mut self,
        packet: u3v::U3vControlPacket,
        expected_ack: u16,
    ) -> Result<Option<Vec<u8>>> {
        let request_id = packet.request_id;
        #[cfg(feature = "os-usb")]
        {
            let Some(endpoints) = self.command_endpoints() else {
                return Ok(None);
            };
            let Some(interface) = self.live_interface.as_mut() else {
                return Ok(None);
            };
            let bytes = packet.encode();
            live_u3v_command(
                interface,
                endpoints,
                bytes,
                expected_ack,
                request_id,
                self.command_ack_size,
                self.command_timeout_ms,
            )
            .map(Some)
        }
        #[cfg(not(feature = "os-usb"))]
        {
            let _ = (packet, expected_ack, request_id);
            Ok(None)
        }
    }

    fn write_u3v_u32_live_or_local(
        &mut self,
        address: u64,
        value: u32,
        apply_local: impl FnOnce(&mut Self),
    ) -> Result<Value> {
        let request_id = self.next_request_id();
        let packet = u3v::write_u32(request_id, address, value);
        let packet_bytes = packet.encode();
        let ack = self.send_u3v_packet(packet, u3v::WRITEMEM_ACK)?;
        apply_local(self);
        let mut values = BTreeMap::from([
            ("protocol".into(), Value::String("U3V".into())),
            (
                "register".into(),
                Value::String(u3v::register_name(address).into()),
            ),
            ("address".into(), Value::String(format!("0x{address:016x}"))),
            ("value".into(), Value::I64(value as i64)),
            ("request_id".into(), Value::I64(request_id as i64)),
            ("packet".into(), Value::Bytes(packet_bytes)),
            ("live".into(), Value::Bool(ack.is_some())),
        ]);
        if let Some(ack) = ack {
            values.insert("ack".into(), Value::Bytes(ack));
        }
        Ok(Value::Map(values))
    }

    fn write_property_live_or_local(&mut self, key: &str, value: &Value) -> Result<Value> {
        if key == "transfer_size" {
            let transfer_size = byte_count_i64(value)?.clamp(16_384, 16_777_216);
            let canonical = byte_count(transfer_size);
            self.validate_write("transfer_size", &canonical)?;
            return self.write_u3v_u32_live_or_local(0x000d_0000, transfer_size as u32, |driver| {
                driver.transfer_size = transfer_size;
            });
        }

        self.validate_write(key, value)?;
        match (key, value) {
            ("width", Value::PixelCount(value)) => {
                let width = value.pixels().clamp(64, 8192);
                self.write_u3v_u32_live_or_local(u3v::REG_WIDTH, width, |driver| {
                    driver.width = width;
                })
            }
            ("height", Value::PixelCount(value)) => {
                let height = value.pixels().clamp(64, 8192);
                self.write_u3v_u32_live_or_local(u3v::REG_HEIGHT, height, |driver| {
                    driver.height = height;
                })
            }
            ("transfer_queue_depth", Value::I64(value)) => {
                let depth = (*value).clamp(1, 256) as u32;
                self.write_u3v_u32_live_or_local(0x000d_0004, depth, |driver| {
                    driver.transfer_queue_depth = i64::from(depth);
                })
            }
            ("exposure", value) => {
                self.exposure_s = seconds(value)?;
                Ok(Value::Bool(true))
            }
            ("gain", Value::Decibel(value)) => {
                self.gain_db = value.db();
                Ok(Value::Bool(true))
            }
            ("pixel_format", Value::String(value)) => {
                self.pixel_format = value.clone();
                Ok(Value::Bool(true))
            }
            _ => Ok(Value::Bool(true)),
        }
    }

    fn raw_register_transaction(
        &mut self,
        request: &RawRegisterRequest,
    ) -> Result<PhysicalTransaction> {
        let packet = match request {
            RawRegisterRequest::Read {
                address,
                byte_count,
                ..
            } => u3v::read_memory(self.next_request_id(), *address, *byte_count),
            RawRegisterRequest::Write { address, bytes, .. } => {
                u3v::write_memory(self.next_request_id(), *address, bytes)
            }
        };
        Ok(PhysicalTransaction {
            resource: Some(self.control),
            description: match request {
                RawRegisterRequest::Read { address, node, .. } => {
                    format!(
                        "U3V RawRegisterAccess read {}",
                        raw_register_label(*address, node.as_deref())
                    )
                }
                RawRegisterRequest::Write { address, node, .. } => {
                    format!(
                        "U3V RawRegisterAccess write {}",
                        raw_register_label(*address, node.as_deref())
                    )
                }
            },
            payload: Value::Bytes(packet.encode()),
        })
    }

    fn invoke_raw_register(&mut self, request: RawRegisterRequest) -> Result<Value> {
        let request_id = self.next_request_id();
        let (operation, address, node, bytes, packet, live, ack) = match request {
            RawRegisterRequest::Read {
                address,
                node,
                byte_count,
            } => {
                let packet = u3v::read_memory(request_id, address, byte_count);
                let packet_bytes = packet.encode();
                let ack = self.send_u3v_packet(packet, u3v::READMEM_ACK)?;
                let live = ack.is_some();
                let bytes = ack
                    .clone()
                    .unwrap_or_else(|| self.raw_register_bytes(address, byte_count as usize));
                ("read", address, node, bytes, packet_bytes, live, ack)
            }
            RawRegisterRequest::Write {
                address,
                node,
                bytes,
            } => {
                let packet = u3v::write_memory(request_id, address, &bytes);
                let packet_bytes = packet.encode();
                let ack = self.send_u3v_packet(packet, u3v::WRITEMEM_ACK)?;
                let live = ack.is_some();
                self.apply_raw_register_write(address, &bytes);
                (
                    "write",
                    address,
                    node,
                    bytes.clone(),
                    packet_bytes,
                    live,
                    ack,
                )
            }
        };
        let value = if bytes.len() == 4 {
            let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            Value::I64(raw as i64)
        } else {
            Value::Bytes(bytes.clone())
        };
        let mut values = BTreeMap::from([
            ("protocol".into(), Value::String("U3V".into())),
            ("operation".into(), Value::String(operation.into())),
            ("address".into(), Value::String(format!("0x{address:016x}"))),
            (
                "register".into(),
                Value::String(u3v::register_name(address).into()),
            ),
            ("request_id".into(), Value::I64(request_id as i64)),
            ("value".into(), value),
            ("bytes".into(), Value::Bytes(bytes)),
            ("packet".into(), Value::Bytes(packet)),
            ("live".into(), Value::Bool(live)),
        ]);
        if let Some(ack) = ack {
            values.insert("ack".into(), Value::Bytes(ack));
        }
        if let Some(node) = node {
            values.insert("node".into(), Value::String(node));
        }
        Ok(Value::Map(values))
    }

    fn raw_register_bytes(&self, address: u64, byte_count: usize) -> Vec<u8> {
        let value = match address {
            u3v::REG_WIDTH => self.width,
            u3v::REG_HEIGHT => self.height,
            u3v::REG_PAYLOAD_SIZE => self.width.saturating_mul(self.height),
            u3v::REG_TIMESTAMP_VALUE => 2_000_000,
            0x000d_0000 => self.transfer_size as u32,
            0x000d_0004 => self.transfer_queue_depth as u32,
            _ => 0,
        };
        let mut bytes = value.to_le_bytes().to_vec();
        bytes.resize(byte_count, 0);
        bytes.truncate(byte_count);
        bytes
    }

    fn apply_raw_register_write(&mut self, address: u64, bytes: &[u8]) {
        if bytes.len() < 4 {
            return;
        }
        let value = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        match address {
            u3v::REG_WIDTH => self.width = value.clamp(64, 8192),
            u3v::REG_HEIGHT => self.height = value.clamp(64, 8192),
            0x000d_0000 => self.transfer_size = i64::from(value),
            0x000d_0004 => self.transfer_queue_depth = i64::from(value.clamp(1, 256)),
            _ => {}
        }
    }
}

#[cfg(feature = "os-usb")]
fn live_u3v_command(
    interface: &mut Interface,
    endpoints: Usb3VisionCommandEndpoints,
    bytes: Vec<u8>,
    expected_ack: u16,
    request_id: u16,
    ack_size: usize,
    _timeout_ms: u64,
) -> Result<Vec<u8>> {
    use futures_lite::future::block_on;

    block_on(interface.bulk_out(endpoints.out_endpoint, bytes))
        .into_result()
        .map(|_| ())
        .map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!(
                    "USB3 Vision U3V bulk OUT endpoint 0x{:02x} failed: {error}",
                    endpoints.out_endpoint
                ),
            )
        })?;
    let ack =
        block_on(interface.bulk_in(endpoints.in_endpoint, RequestBuffer::new(ack_size.max(8))))
            .into_result()
            .map_err(|error| {
                Error::new(
                    ErrorCode::Transport,
                    format!(
                        "USB3 Vision U3V bulk IN endpoint 0x{:02x} ACK failed: {error}",
                        endpoints.in_endpoint
                    ),
                )
            })?;
    u3v::decode_ack(&ack, expected_ack, request_id)
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
    transfer_size: i64,
    transfer_queue_depth: i64,
    stream_endpoint: i64,
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
            let timestamp = 2_000_000 + (index as i64 * 10_000);
            let mut metadata = Usb3VisionDriver::frame_metadata(
                index,
                timestamp,
                job.transfer_size,
                job.transfer_queue_depth,
                job.stream_endpoint,
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
                ("transport".into(), Value::String("U3V".into())),
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
            format!("read USB3 Vision fixture {path}: {error}"),
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

fn u64_prop(device: &DeviceConfig, key: &str) -> Option<u64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if *value >= 0 => Some(*value as u64),
        _ => None,
    }
}

fn bool_prop(device: &DeviceConfig, key: &str) -> Option<bool> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn u16_prop(device: &DeviceConfig, key: &str) -> Option<u16> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if *value >= 0 && *value <= u16::MAX as i64 => Some(*value as u16),
        Some(Value::String(value)) => parse_u16(value).ok(),
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
        "read" | "ReadRegister" | "read_register" | "ReadMem" | "read_memory" => {
            let byte_count = request
                .params
                .get("byte_count")
                .or_else(|| request.params.get("length"))
                .map(value_u32)
                .transpose()?
                .unwrap_or(4);
            if byte_count == 0 {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "RawRegisterAccess byte_count must be positive",
                ));
            }
            Ok(RawRegisterRequest::Read {
                address: target.address,
                node: target.node,
                byte_count,
            })
        }
        "write" | "WriteRegister" | "write_register" | "WriteMem" | "write_memory" => {
            if target.node.is_none() {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "RawRegisterAccess writes require a named public node target",
                ));
            }
            let bytes = if let Some(bytes) = request.params.get("bytes") {
                value_bytes(bytes)?
            } else {
                let value = request
                    .params
                    .get("value")
                    .ok_or_else(|| {
                        Error::new(
                            ErrorCode::InvalidCommand,
                            "RawRegisterAccess write missing value or bytes",
                        )
                    })
                    .and_then(value_u32)?;
                value.to_le_bytes().to_vec()
            };
            if bytes.is_empty() {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "RawRegisterAccess write bytes must not be empty",
                ));
            }
            Ok(RawRegisterRequest::Write {
                address: target.address,
                node: target.node,
                bytes,
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
    address: u64,
    node: Option<String>,
}

fn raw_register_target(request: &GenericCommandRequest) -> Result<RawRegisterTarget> {
    if let Some(address) = request.params.get("address") {
        return Ok(RawRegisterTarget {
            address: value_u64(address)?,
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
    let address = usb3_genicam_node_address(&node).ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidCommand,
            format!("unsupported USB3 Vision GenICam node {node}"),
        )
    })?;
    Ok(RawRegisterTarget {
        address,
        node: Some(node),
    })
}

fn usb3_genicam_node_address(node: &str) -> Option<u64> {
    match normalized_node_name(node).as_str() {
        "manifesttable" => Some(u3v::REG_MANIFEST_TABLE),
        "devicecapability" => Some(u3v::REG_DEVICE_CAPABILITY),
        "width" => Some(u3v::REG_WIDTH),
        "height" => Some(u3v::REG_HEIGHT),
        "payloadsize" => Some(u3v::REG_PAYLOAD_SIZE),
        "timestampcontrol" => Some(u3v::REG_TIMESTAMP_CONTROL),
        "timestampvalue" => Some(u3v::REG_TIMESTAMP_VALUE),
        "acquisitionstart" => Some(u3v::REG_ACQUISITION_START),
        "acquisitionstop" => Some(u3v::REG_ACQUISITION_STOP),
        _ => None,
    }
}

fn normalized_node_name(node: &str) -> String {
    node.chars()
        .filter(|ch| *ch != '_' && *ch != '-' && !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn raw_register_label(address: u64, node: Option<&str>) -> String {
    match node {
        Some(node) => format!("{node} ({})", u3v::register_name(address)),
        None => u3v::register_name(address).into(),
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

fn value_u64(value: &Value) -> Result<u64> {
    match value {
        Value::I64(value) if *value >= 0 => Ok(*value as u64),
        Value::String(value) => parse_u64_address(value),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("expected u64 raw-register address, got {value:?}"),
        )),
    }
}

fn value_u32(value: &Value) -> Result<u32> {
    match value {
        Value::I64(value) if *value >= 0 && *value <= u32::MAX as i64 => Ok(*value as u32),
        Value::String(value) => parse_u32_value(value),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("expected u32 raw-register value, got {value:?}"),
        )),
    }
}

fn value_bytes(value: &Value) -> Result<Vec<u8>> {
    match value {
        Value::Bytes(bytes) => Ok(bytes.clone()),
        Value::List(values) => values.iter().map(value_u8).collect(),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("expected byte list for RawRegisterAccess, got {value:?}"),
        )),
    }
}

fn value_u8(value: &Value) -> Result<u8> {
    match value {
        Value::I64(value) if *value >= 0 && *value <= u8::MAX as i64 => Ok(*value as u8),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("expected byte value, got {value:?}"),
        )),
    }
}

fn parse_u64_address(value: &str) -> Result<u64> {
    let trimmed = value.trim();
    let parsed = if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16)
    } else {
        trimmed.parse::<u64>()
    };
    parsed.map_err(|_| {
        Error::new(
            ErrorCode::InvalidCommand,
            format!("invalid u64 raw-register address {value}"),
        )
    })
}

fn parse_u16(value: &str) -> Result<u16> {
    let raw = parse_u64_address(value)?;
    u16::try_from(raw).map_err(|_| {
        Error::new(
            ErrorCode::InvalidProperty,
            format!("USB3 Vision config value {value} exceeds u16"),
        )
    })
}

fn parse_u32_value(value: &str) -> Result<u32> {
    let raw = parse_u64_address(value)?;
    u32::try_from(raw).map_err(|_| {
        Error::new(
            ErrorCode::InvalidCommand,
            format!("raw-register value {value} exceeds u32"),
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
                "byte count exceeds supported USB3 Vision register range",
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
