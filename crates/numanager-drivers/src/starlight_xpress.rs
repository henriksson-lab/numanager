use numanager_core::config::{DeviceConfig, HardwareConfig};
#[cfg(feature = "os-hid")]
use numanager_core::hid::{
    enumerate_hid_devices, HidDeviceIdentity, HidReportIo, OsHidFeatureConfig, OsHidReportDevice,
};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::{FixedBinaryCodec, ScriptedSerial, SerialIo};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const BAUD: u32 = 9600;
    pub const FRAME_LEN: usize = 4;
    pub const REPLY_POLLS: usize = 20;
    pub const HEADER: u8 = 0xa5;
    pub const DATA_UNUSED: u8 = 0x20;
    pub const CMD_SELECT_FILTER: u8 = 0x01;
    pub const CMD_CURRENT_FILTER: u8 = 0x02;
    pub const CMD_FILTER_TOTAL: u8 = 0x03;
    pub const RESP_SELECT_FILTER: u8 = 0x81;
    pub const RESP_CURRENT_FILTER: u8 = 0x82;
    pub const RESP_FILTER_TOTAL: u8 = 0x83;
    pub const RESPONSE_ASCII_OFFSET: u8 = 0x30;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SxWheelCommand {
        SelectFilter { position: u8 },
        CurrentFilter,
        FilterTotal,
    }

    impl SxWheelCommand {
        pub fn response_code(self) -> u8 {
            match self {
                SxWheelCommand::SelectFilter { .. } => RESP_SELECT_FILTER,
                SxWheelCommand::CurrentFilter => RESP_CURRENT_FILTER,
                SxWheelCommand::FilterTotal => RESP_FILTER_TOTAL,
            }
        }
    }

    pub fn encode(command: SxWheelCommand) -> [u8; FRAME_LEN] {
        let mut frame = [HEADER, 0, DATA_UNUSED, 0];
        match command {
            SxWheelCommand::SelectFilter { position } => {
                frame[1] = CMD_SELECT_FILTER;
                frame[2] = position;
            }
            SxWheelCommand::CurrentFilter => {
                frame[1] = CMD_CURRENT_FILTER;
            }
            SxWheelCommand::FilterTotal => {
                frame[1] = CMD_FILTER_TOTAL;
            }
        }
        frame[3] = checksum(frame[0], frame[1], frame[2]);
        frame
    }

    pub fn decode(frame: &[u8], expected: u8) -> Result<SxWheelResponse> {
        if frame.len() != FRAME_LEN {
            return Err(Error::new(
                ErrorCode::Transport,
                "Starlight Xpress response must be a four-byte frame",
            ));
        }
        if frame[0] != HEADER {
            return Err(Error::new(
                ErrorCode::Transport,
                "Starlight Xpress response has an invalid header",
            ));
        }
        if frame[1] != expected {
            return Err(Error::new(
                ErrorCode::Transport,
                format!(
                    "Starlight Xpress response code {:02x} does not match expected {:02x}",
                    frame[1], expected
                ),
            ));
        }
        if frame[3] != checksum(frame[0], frame[1], frame[2]) {
            return Err(Error::new(
                ErrorCode::Transport,
                "Starlight Xpress response checksum failed",
            ));
        }
        match frame[1] {
            RESP_SELECT_FILTER => Ok(SxWheelResponse::Selected {
                position: frame[2],
                moving: frame[2] == 0,
            }),
            RESP_CURRENT_FILTER => Ok(SxWheelResponse::Current {
                position: decode_ascii_data(frame[2]),
                moving: frame[2] == 0,
            }),
            RESP_FILTER_TOTAL => Ok(SxWheelResponse::Total {
                positions: decode_ascii_data(frame[2]),
                moving: frame[2] == 0,
            }),
            _ => Err(Error::new(
                ErrorCode::Transport,
                "unsupported Starlight Xpress response code",
            )),
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SxWheelResponse {
        Selected { position: u8, moving: bool },
        Current { position: u8, moving: bool },
        Total { positions: u8, moving: bool },
    }

    pub fn selected_response(position: u8) -> [u8; FRAME_LEN] {
        response(RESP_SELECT_FILTER, position)
    }

    pub fn current_response(position: u8) -> [u8; FRAME_LEN] {
        response(
            RESP_CURRENT_FILTER,
            position.saturating_add(RESPONSE_ASCII_OFFSET),
        )
    }

    pub fn total_response(positions: u8) -> [u8; FRAME_LEN] {
        response(
            RESP_FILTER_TOTAL,
            positions.saturating_add(RESPONSE_ASCII_OFFSET),
        )
    }

    fn response(code: u8, data: u8) -> [u8; FRAME_LEN] {
        [HEADER, code, data, checksum(HEADER, code, data)]
    }

    fn decode_ascii_data(data: u8) -> u8 {
        data.saturating_sub(RESPONSE_ASCII_OFFSET)
    }

    fn checksum(a: u8, b: u8, c: u8) -> u8 {
        a.wrapping_add(b).wrapping_add(c)
    }
}

#[derive(Debug, Clone)]
pub struct SxFilterWheelConfiguredProbe {
    label: String,
    endpoint: Option<SxFilterWheelEndpoint>,
    product: String,
    serial_number: String,
    positions: u8,
    position: u8,
    completion_polls: u8,
}

#[derive(Debug, Clone)]
pub enum SxFilterWheelEndpoint {
    Serial(SxFilterWheelSerialEndpoint),
    Hid(SxFilterWheelHidEndpoint),
}

#[derive(Debug, Clone)]
pub struct SxFilterWheelSerialEndpoint {
    pub port_name: String,
    pub baud_rate: u32,
}

#[derive(Debug, Clone)]
pub struct SxFilterWheelHidEndpoint {
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial_number: Option<String>,
    pub report_id: u8,
    pub timeout_ms: i32,
}

pub struct SxFilterWheelDiscovery {
    next_id: DriverId,
    probes: Vec<SxFilterWheelConfiguredProbe>,
}

impl SxFilterWheelDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![SxFilterWheelConfiguredProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| {
                matches!(
                    device.driver.as_str(),
                    "starlight_xpress" | "sx_filter_wheel" | "sx-wheel"
                )
            })
            .map(SxFilterWheelConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for SxFilterWheelDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver: Box<dyn Driver> = if configured.endpoint.is_some() {
                    Box::new(SxFilterWheelDriver::connected(id, configured)?)
                } else {
                    Box::new(SxFilterWheelDriver::configured(id, configured))
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

impl SxFilterWheelConfiguredProbe {
    pub fn fixture() -> Self {
        Self {
            label: "Configured Starlight Xpress filter wheel".into(),
            endpoint: None,
            product: "SX Universal/Maxi USB Filter Wheel".into(),
            serial_number: "SXFW-CONFIG-0001".into(),
            positions: 7,
            position: 1,
            completion_polls: 20,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::fixture();
        configured.label = if device.label.is_empty() {
            configured.label
        } else {
            device.label.clone()
        };
        configured.product = string_prop(device, "product").unwrap_or(configured.product);
        configured.serial_number =
            string_prop(device, "serial_number").unwrap_or(configured.serial_number);
        configured.positions = u8_prop(device, "positions")
            .or_else(|| u8_prop(device, "filter_count"))
            .unwrap_or(configured.positions);
        configured.position = u8_prop(device, "position").unwrap_or(configured.position);
        configured.completion_polls =
            u8_prop(device, "completion_polls").unwrap_or(configured.completion_polls);
        let serial_endpoint =
            string_prop(device, "serial_port").map(|port_name| SxFilterWheelSerialEndpoint {
                port_name,
                baud_rate: u32_prop(device, "baud_rate").unwrap_or(protocol::BAUD),
            });
        let mut hid_endpoint = hid_endpoint(device)?;
        if serial_endpoint.is_some() && hid_endpoint.is_some() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Starlight Xpress config must not set both serial_port and HID endpoint fields",
            ));
        }
        let connect = bool_prop(device, "connect").unwrap_or(false);
        if connect && serial_endpoint.is_none() && hid_endpoint.is_none() && wants_usb_hid(device) {
            hid_endpoint = autodiscover_hid_endpoint(device)?;
        }
        configured.endpoint = serial_endpoint
            .map(SxFilterWheelEndpoint::Serial)
            .or_else(|| hid_endpoint.map(SxFilterWheelEndpoint::Hid));
        if connect && configured.endpoint.is_none() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Starlight Xpress real transport config requires serial_port, explicit HID endpoint fields, or a single auto-discoverable HID wheel",
            ));
        }
        if !(1..=16).contains(&configured.positions) {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Starlight Xpress filter count must be in 1..=16",
            ));
        }
        configured.position = configured.position.clamp(1, configured.positions);
        Ok(configured)
    }
}

pub struct SxFilterWheelDriver {
    id: DriverId,
    resource: ResourceId,
    wheel: DeviceId,
    product: String,
    serial_number: String,
    positions: u8,
    position: u8,
    moving: bool,
    completion_polls: u8,
    serial: Box<dyn SerialIo>,
    codec: FixedBinaryCodec,
    synthesize_responses: bool,
    last_transaction: Value,
    resource_label: String,
    resource_kind: String,
    endpoint: Option<SxFilterWheelEndpoint>,
    connected: bool,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
}

impl SxFilterWheelDriver {
    pub fn configured(id: DriverId, configured: SxFilterWheelConfiguredProbe) -> Self {
        let reads = vec![
            protocol::current_response(configured.position).to_vec(),
            protocol::total_response(configured.positions).to_vec(),
        ];
        let mut driver = Self::new(
            id,
            configured,
            Box::new(ScriptedSerial::with_reads(reads)),
            false,
        );
        driver.synthesize_responses = true;
        driver
    }

    pub fn connected(
        driver_id: DriverId,
        configured: SxFilterWheelConfiguredProbe,
    ) -> Result<Self> {
        let endpoint = configured.endpoint.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Starlight Xpress real transport config requires serial_port or HID endpoint fields",
            )
        })?;
        match endpoint {
            SxFilterWheelEndpoint::Serial(endpoint) => {
                Self::serial(driver_id, configured, endpoint)
            }
            SxFilterWheelEndpoint::Hid(endpoint) => Self::hid(driver_id, configured, endpoint),
        }
    }

    pub fn serial(
        driver_id: DriverId,
        configured: SxFilterWheelConfiguredProbe,
        endpoint: SxFilterWheelSerialEndpoint,
    ) -> Result<Self> {
        #[cfg(feature = "os-serial")]
        {
            let serial = Box::new(numanager_core::serial::OsSerialPort::open_config(
                numanager_core::serial::OsSerialConfig::new(endpoint.port_name, endpoint.baud_rate),
            )?);
            let mut driver = Self::new(driver_id, configured, serial, true);
            driver.refresh_startup_state()?;
            Ok(driver)
        }
        #[cfg(not(feature = "os-serial"))]
        {
            let _ = driver_id;
            let _ = configured;
            let _ = endpoint;
            Err(Error::new(
                ErrorCode::Unsupported,
                "Starlight Xpress real serial transport requires the os-serial feature",
            ))
        }
    }

    #[cfg(feature = "os-hid")]
    pub fn hid(
        driver_id: DriverId,
        configured: SxFilterWheelConfiguredProbe,
        endpoint: SxFilterWheelHidEndpoint,
    ) -> Result<Self> {
        let mut config = OsHidFeatureConfig::new(endpoint.vendor_id, endpoint.product_id)
            .read_timeout_ms(endpoint.timeout_ms);
        if let Some(serial) = &endpoint.serial_number {
            config = config.serial_number(serial.clone());
        }
        let io = Box::new(OsHidReportDevice::open_config(config, endpoint.report_id)?);
        let mut driver = Self::new(
            driver_id,
            configured,
            Box::new(SxHidSerialAdapter::new(io)),
            true,
        )
        .with_resource(
            "Starlight Xpress filter wheel HID endpoint",
            "usb.hid.report",
        );
        driver.refresh_startup_state()?;
        Ok(driver)
    }

    #[cfg(not(feature = "os-hid"))]
    pub fn hid(
        _driver_id: DriverId,
        _configured: SxFilterWheelConfiguredProbe,
        _endpoint: SxFilterWheelHidEndpoint,
    ) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Starlight Xpress USB HID transport requires the os-hid feature",
        ))
    }

    pub fn new(
        id: DriverId,
        configured: SxFilterWheelConfiguredProbe,
        serial: Box<dyn SerialIo>,
        connected: bool,
    ) -> Self {
        let endpoint = configured.endpoint.clone();
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 880)),
            wheel: DeviceId(NodeId(id.0 * 1000 + 881)),
            product: configured.product,
            serial_number: configured.serial_number,
            positions: configured.positions,
            position: configured.position,
            moving: false,
            completion_polls: configured.completion_polls,
            serial,
            codec: FixedBinaryCodec::new(protocol::FRAME_LEN),
            synthesize_responses: false,
            last_transaction: Value::Map(BTreeMap::new()),
            resource_label: "Starlight Xpress filter wheel serial endpoint".into(),
            resource_kind: "serial.binary".into(),
            endpoint,
            connected,
            next_token: 1,
            pending: VecDeque::new(),
        }
    }

    #[cfg(feature = "os-hid")]
    fn with_resource(mut self, label: impl Into<String>, kind: impl Into<String>) -> Self {
        self.resource_label = label.into();
        self.resource_kind = kind.into();
        self
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn send(
        &mut self,
        command: protocol::SxWheelCommand,
    ) -> Result<Option<protocol::SxWheelResponse>> {
        let frame = protocol::encode(command);
        self.serial.write(&frame)?;
        match self.read_response(command.response_code()) {
            Ok(Some(response)) => Ok(Some(response)),
            Ok(None) if self.synthesize_responses => Ok(Some(self.synthetic_response(command))),
            Ok(None) => Ok(None),
            Err(_) if self.synthesize_responses => Ok(Some(self.synthetic_response(command))),
            Err(error) => Err(error),
        }
    }

    fn read_response(&mut self, expected: u8) -> Result<Option<protocol::SxWheelResponse>> {
        for _ in 0..protocol::REPLY_POLLS {
            let bytes = self.serial.read_available()?;
            let frames = self.codec.push(&bytes)?;
            if let Some(frame) = frames.first() {
                return protocol::decode(frame, expected).map(Some);
            }
        }
        Ok(None)
    }

    fn synthetic_response(&self, command: protocol::SxWheelCommand) -> protocol::SxWheelResponse {
        match command {
            protocol::SxWheelCommand::SelectFilter { position } => {
                protocol::SxWheelResponse::Selected {
                    position,
                    moving: false,
                }
            }
            protocol::SxWheelCommand::CurrentFilter => protocol::SxWheelResponse::Current {
                position: self.position,
                moving: false,
            },
            protocol::SxWheelCommand::FilterTotal => protocol::SxWheelResponse::Total {
                positions: self.positions,
                moving: false,
            },
        }
    }

    fn read_current(&mut self) -> Result<u8> {
        match self.send(protocol::SxWheelCommand::CurrentFilter)? {
            Some(protocol::SxWheelResponse::Current { position, moving }) => {
                self.moving = moving;
                if position != 0 {
                    self.position = position.clamp(1, self.positions);
                }
            }
            Some(_) => {}
            None => return Err(missing_response_error("current-filter")),
        }
        self.last_transaction = self.transaction("read_current_filter", "serial_response");
        Ok(self.position)
    }

    fn read_total(&mut self) -> Result<u8> {
        match self.send(protocol::SxWheelCommand::FilterTotal)? {
            Some(protocol::SxWheelResponse::Total { positions, moving }) => {
                self.moving = moving;
                if positions != 0 {
                    self.positions = positions;
                    self.position = self.position.clamp(1, self.positions);
                }
            }
            Some(_) => {}
            None => return Err(missing_response_error("filter-total")),
        }
        self.last_transaction = self.transaction("read_filter_total", "serial_response");
        Ok(self.positions)
    }

    #[cfg_attr(not(any(feature = "os-serial", feature = "os-hid")), allow(dead_code))]
    fn refresh_startup_state(&mut self) -> Result<()> {
        self.read_total()?;
        self.read_current()?;
        Ok(())
    }

    fn select_position(&mut self, position: u8) -> Result<Value> {
        let positions = self.read_total()?;
        if positions != self.positions {
            self.positions = positions;
        }
        if !(1..=self.positions).contains(&position) {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!(
                    "Starlight Xpress filter position must be in 1..={}",
                    self.positions
                ),
            ));
        }
        let target = position;
        self.moving = true;
        match self.send(protocol::SxWheelCommand::SelectFilter { position: target })? {
            Some(protocol::SxWheelResponse::Selected { position, moving }) => {
                self.moving = moving;
                if position != 0 {
                    self.position = position.clamp(1, self.positions);
                }
            }
            Some(_) => {}
            None => return Err(missing_response_error("select-filter")),
        }
        let _ = self.read_current()?;
        for _ in 0..self.completion_polls {
            if !self.moving {
                break;
            }
            let _ = self.read_current()?;
        }
        if self.moving {
            return Err(Error::new(
                ErrorCode::Transport,
                "Starlight Xpress filter wheel did not report completion before poll limit",
            ));
        }
        self.last_transaction = self.transaction("select_filter", "serial_readback");
        let value = Value::I64(self.position as i64);
        self.emit_property("position", value.clone());
        self.emit_property("moving", Value::Bool(self.moving));
        Ok(value)
    }

    fn read_property(&mut self, key: &str) -> Result<Value> {
        match key {
            "product" => Ok(Value::String(self.product.clone())),
            "serial_number" => Ok(Value::String(self.serial_number.clone())),
            "protocol" => Ok(Value::String(
                "Starlight Xpress serial filter-wheel protocol".into(),
            )),
            "positions" => Ok(Value::I64(self.read_total()? as i64)),
            "position" => Ok(Value::I64(self.read_current()? as i64)),
            "moving" => Ok(Value::Bool(self.moving)),
            "last_transaction" => Ok(self.last_transaction.clone()),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Starlight Xpress filter wheel property {key}"),
            )),
        }
    }

    fn refresh_generic(&mut self, request: GenericCommandRequest) -> Result<Value> {
        if request.is_hidden_maintenance() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "GenericCommand {} is a hidden maintenance operation",
                    request.command
                ),
            ));
        }
        if !request.params.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Starlight Xpress GenericCommand does not take parameters",
            ));
        }
        let commands = match request.command.as_str() {
            "refresh_position" => {
                self.read_current()?;
                1
            }
            "refresh_positions" => {
                self.read_total()?;
                1
            }
            "refresh_readbacks" => {
                self.read_total()?;
                self.read_current()?;
                2
            }
            other => {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    format!(
                        "Starlight Xpress GenericCommand supports refresh_readbacks, refresh_position, and refresh_positions; got {other}"
                    ),
                ))
            }
        };
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("commands".into(), Value::I64(commands)),
            ("position".into(), Value::I64(self.position as i64)),
            ("positions".into(), Value::I64(self.positions as i64)),
            ("moving".into(), Value::Bool(self.moving)),
            (
                "completion_basis".into(),
                Value::String("Starlight Xpress mapped filter readback".into()),
            ),
        ])))
    }

    fn validate_write(&self, key: &str, value: &Value) -> Result<()> {
        match (key, value) {
            ("position", Value::I64(position)) if (1..=16).contains(position) => Ok(()),
            ("position", _) => Err(Error::new(
                ErrorCode::InvalidProperty,
                "Starlight Xpress filter position must be in the documented range 1..=16",
            )),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Starlight Xpress property {key} is read-only or has the wrong type"),
            )),
        }
    }

    fn write_property(&mut self, key: &str, value: Value) -> Result<Value> {
        self.validate_write(key, &value)?;
        if key == "position" {
            let Value::I64(position) = value else {
                unreachable!("validated write")
            };
            return self.select_position(position as u8);
        }
        unreachable!("validated write")
    }

    fn transaction(&self, command: &str, completion_basis: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("command".into(), Value::String(command.into())),
            ("position".into(), Value::I64(self.position as i64)),
            ("positions".into(), Value::I64(self.positions as i64)),
            ("moving".into(), Value::Bool(self.moving)),
            (
                "completion_basis".into(),
                Value::String(completion_basis.into()),
            ),
        ]))
    }

    fn emit_property(&mut self, key: &str, value: Value) {
        self.pending
            .push_back(DriverEvent::Event(Event::PropertyChanged(
                PropertyChanged {
                    device: self.wheel,
                    key: key.into(),
                    value,
                },
            )));
    }
}

impl Driver for SxFilterWheelDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        let mut metadata = BTreeMap::from([
            ("frame_len".into(), Value::I64(protocol::FRAME_LEN as i64)),
            (
                "protocol".into(),
                Value::String("Starlight Xpress filter-wheel protocol".into()),
            ),
            ("connected".into(), Value::Bool(self.connected)),
        ]);
        match &self.endpoint {
            Some(SxFilterWheelEndpoint::Serial(endpoint)) => {
                metadata.insert("baud_rate".into(), Value::I64(endpoint.baud_rate as i64));
                metadata.insert(
                    "serial_port".into(),
                    Value::String(endpoint.port_name.clone()),
                );
            }
            Some(SxFilterWheelEndpoint::Hid(endpoint)) => {
                metadata.insert(
                    "usb_vendor_id".into(),
                    Value::I64(endpoint.vendor_id as i64),
                );
                metadata.insert(
                    "usb_product_id".into(),
                    Value::I64(endpoint.product_id as i64),
                );
                metadata.insert(
                    "hid_serial_number".into(),
                    endpoint
                        .serial_number
                        .as_ref()
                        .map(|serial| Value::String(serial.clone()))
                        .unwrap_or(Value::Null),
                );
                metadata.insert(
                    "hid_report_id".into(),
                    Value::I64(endpoint.report_id as i64),
                );
                metadata.insert(
                    "hid_timeout".into(),
                    Value::TimeInterval(TimeInterval::from_milliseconds(
                        endpoint.timeout_ms as f64,
                    )),
                );
            }
            None => {
                metadata.insert("baud_rate".into(), Value::I64(protocol::BAUD as i64));
                metadata.insert("serial_port".into(), Value::Null);
            }
        }
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: self.resource_label.clone(),
            kind: self.resource_kind.clone(),
            metadata,
        }]
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        vec![DeviceDescriptor {
            id: self.wheel,
            driver: self.id,
            label: "starlight-xpress-filter-wheel".into(),
            vendor: Some("Starlight Xpress".into()),
            model: Some(self.product.clone()),
            serial: Some(self.serial_number.clone()),
            kinds: vec!["filter.wheel".into(), "state.device".into()],
            properties: vec![
                string_property("product", "Product", false),
                string_property("serial_number", "Serial number", false),
                string_property("protocol", "Protocol", false),
                integer_range_property("positions", "Positions", false, 1, 16),
                integer_range_property("position", "Position", true, 1, self.positions as i64),
                bool_property("moving", "Moving", false),
                map_property("last_transaction", "Last transaction", false),
            ],
            metadata: BTreeMap::from([(
                "source".into(),
                Value::String("Starlight Xpress filter wheel manuals".into()),
            )]),
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.wheel {
            vec![
                CapabilityDescriptor::new(
                    CapabilityId(1),
                    device,
                    CapabilityKind::FilterSelect,
                    ValueType::I64,
                ),
                CapabilityDescriptor::new(
                    CapabilityId(2),
                    device,
                    CapabilityKind::GenericCommand,
                    ValueType::Map,
                ),
            ]
        } else {
            Vec::new()
        }
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut physical_transactions = Vec::new();
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } if *device == self.wheel => {
                    if !matches!(
                        key.as_str(),
                        "product"
                            | "serial_number"
                            | "protocol"
                            | "positions"
                            | "position"
                            | "moving"
                            | "last_transaction"
                    ) {
                        return Err(Error::new(
                            ErrorCode::InvalidProperty,
                            format!("unknown Starlight Xpress filter wheel property {key}"),
                        ));
                    }
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("starlight xpress read {key}"),
                        Value::String(key.clone()),
                    ));
                }
                Command::WriteProperty { device, key, value } if *device == self.wheel => {
                    self.validate_write(key, value)?;
                    physical_transactions.push(transaction(
                        self.resource,
                        "starlight xpress select filter",
                        value.clone(),
                    ));
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        if write.device == self.wheel {
                            self.validate_write(&write.property, &write.value)?;
                        }
                    }
                    physical_transactions.push(transaction(
                        self.resource,
                        "starlight xpress state set",
                        Value::List(
                            set.writes
                                .iter()
                                .filter(|write| write.device == self.wheel)
                                .map(|write| Value::String(write.property.clone()))
                                .collect(),
                        ),
                    ));
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if *device == self.wheel => {
                    let descriptor = self
                        .capabilities(*device)
                        .into_iter()
                        .find(|candidate| candidate.id == *capability)
                        .ok_or_else(|| {
                            Error::new(
                                ErrorCode::Unsupported,
                                "unknown Starlight Xpress filter wheel capability",
                            )
                        })?;
                    match (&descriptor.kind, request) {
                        (
                            CapabilityKind::FilterSelect,
                            CapabilityRequest::FilterSelect(request),
                        ) if (1..=16).contains(&request.position) => {}
                        (
                            CapabilityKind::GenericCommand,
                            CapabilityRequest::GenericCommand(request),
                        ) => {
                            if !request.params.is_empty() {
                                return Err(Error::new(
                                    ErrorCode::InvalidCommand,
                                    "Starlight Xpress GenericCommand does not take parameters",
                                ));
                            }
                            match request.command.as_str() {
                                "refresh_readbacks" | "refresh_position" | "refresh_positions" => {
                                }
                                other => {
                                    return Err(Error::new(
                                        ErrorCode::Unsupported,
                                        format!(
                                            "Starlight Xpress GenericCommand supports refresh_readbacks, refresh_position, and refresh_positions; got {other}"
                                        ),
                                    ))
                                }
                            }
                        }
                        (CapabilityKind::FilterSelect, CapabilityRequest::FilterSelect(_)) => {
                            return Err(Error::new(
                                ErrorCode::InvalidProperty,
                                "Starlight Xpress filter position must be in the documented range 1..=16",
                            ));
                        }
                        (CapabilityKind::FilterSelect, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Starlight Xpress FilterSelect expects FilterSelectRequest",
                            ));
                        }
                        (CapabilityKind::GenericCommand, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Starlight Xpress GenericCommand expects GenericCommandRequest",
                            ));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported Starlight Xpress filter wheel capability",
                            ));
                        }
                    }
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("starlight xpress invoke {}", descriptor.kind.name()),
                        match request {
                            CapabilityRequest::FilterSelect(request) => {
                                Value::I64(request.position as i64)
                            }
                            CapabilityRequest::GenericCommand(request) => {
                                Value::String(request.command.clone())
                            }
                            _ => Value::Null,
                        },
                    ));
                }
                Command::ReadProperty { .. } | Command::WriteProperty { .. } => {}
                Command::Invoke { .. } => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "unsupported Starlight Xpress filter wheel capability invocation",
                    ));
                }
                Command::Arm(_) | Command::Start(_) | Command::Stop(_) => {}
            }
        }
        Ok(PreparedBatch {
            id: batch.id,
            commands: batch.commands.clone(),
            physical_transactions,
        })
    }

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.token();
        let mut last = Value::Null;
        for command in prepared.commands {
            match command {
                Command::ReadProperty { device, key } if device == self.wheel => {
                    last = self.read_property(&key)?;
                }
                Command::WriteProperty { device, key, value } if device == self.wheel => {
                    last = self.write_property(&key, value)?;
                }
                Command::ApplyStateSet(set) => {
                    let mut map = BTreeMap::new();
                    for write in set.writes {
                        if write.device == self.wheel {
                            let value = self.write_property(&write.property, write.value)?;
                            map.insert(write.property, value);
                        }
                    }
                    last = Value::Map(map);
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if device == self.wheel => {
                    let descriptor = self
                        .capabilities(device)
                        .into_iter()
                        .find(|candidate| candidate.id == capability)
                        .ok_or_else(|| {
                            Error::new(
                                ErrorCode::Unsupported,
                                "unknown Starlight Xpress filter wheel capability",
                            )
                        })?;
                    match (descriptor.kind, request) {
                        (
                            CapabilityKind::FilterSelect,
                            CapabilityRequest::FilterSelect(request),
                        ) => {
                            last = self.select_position(request.position)?;
                        }
                        (
                            CapabilityKind::GenericCommand,
                            CapabilityRequest::GenericCommand(request),
                        ) => {
                            last = self.refresh_generic(request)?;
                        }
                        (CapabilityKind::FilterSelect, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Starlight Xpress FilterSelect expects FilterSelectRequest",
                            ));
                        }
                        (CapabilityKind::GenericCommand, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Starlight Xpress GenericCommand expects GenericCommandRequest",
                            ));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported Starlight Xpress filter wheel capability",
                            ));
                        }
                    }
                }
                Command::ReadProperty { .. }
                | Command::WriteProperty { .. }
                | Command::Arm(_)
                | Command::Start(_)
                | Command::Stop(_) => {}
                Command::Invoke { .. } => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "unsupported Starlight Xpress filter wheel capability invocation",
                    ));
                }
            }
        }
        self.pending
            .push_back(DriverEvent::TokenCompleted { token, value: last });
        Ok(token)
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        self.pending.drain(..).collect()
    }
}

fn transaction(
    resource: ResourceId,
    description: impl Into<String>,
    payload: Value,
) -> PhysicalTransaction {
    PhysicalTransaction {
        resource: Some(resource),
        description: description.into(),
        payload,
    }
}

#[cfg(feature = "os-hid")]
struct SxHidSerialAdapter {
    io: Box<dyn HidReportIo>,
    expected_response: Option<u8>,
}

#[cfg(feature = "os-hid")]
impl SxHidSerialAdapter {
    fn new(io: Box<dyn HidReportIo>) -> Self {
        Self {
            io,
            expected_response: None,
        }
    }

    fn serial_response(&self, response_code: u8, data: u8) -> Vec<u8> {
        vec![
            protocol::HEADER,
            response_code,
            data,
            protocol::HEADER
                .wrapping_add(response_code)
                .wrapping_add(data),
        ]
    }
}

#[cfg(feature = "os-hid")]
impl SerialIo for SxHidSerialAdapter {
    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() != protocol::FRAME_LEN || bytes[0] != protocol::HEADER {
            return Err(Error::new(
                ErrorCode::Transport,
                "Starlight Xpress HID adapter expected a documented four-byte serial frame",
            ));
        }
        let (report, expected) = match bytes[1] {
            protocol::CMD_SELECT_FILTER => ([bytes[2], 0], protocol::RESP_SELECT_FILTER),
            protocol::CMD_CURRENT_FILTER => ([0, 0], protocol::RESP_CURRENT_FILTER),
            protocol::CMD_FILTER_TOTAL => ([0, 1], protocol::RESP_FILTER_TOTAL),
            _ => {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    "unsupported Starlight Xpress HID command",
                ))
            }
        };
        self.io.write_report(&report)?;
        self.expected_response = Some(expected);
        Ok(())
    }

    fn read_available(&mut self) -> Result<Vec<u8>> {
        let Some(expected) = self.expected_response else {
            return Ok(Vec::new());
        };
        let report = self.io.read_report(2)?;
        if report.len() < 2 {
            return Err(Error::new(
                ErrorCode::Transport,
                "Starlight Xpress HID input report must be two bytes",
            ));
        }
        let first = report[0];
        let second = report[1];
        let data = match expected {
            protocol::RESP_SELECT_FILTER => first,
            protocol::RESP_CURRENT_FILTER => {
                if first == 0 {
                    0
                } else {
                    first.saturating_add(protocol::RESPONSE_ASCII_OFFSET)
                }
            }
            protocol::RESP_FILTER_TOTAL => {
                let total = if second != 0 { second } else { first };
                if total == 0 {
                    0
                } else {
                    total.saturating_add(protocol::RESPONSE_ASCII_OFFSET)
                }
            }
            _ => 0,
        };
        self.expected_response = None;
        Ok(self.serial_response(expected, data))
    }
}

fn property(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    writable: bool,
) -> PropertySchema {
    PropertySchema {
        key: key.into(),
        display_name: display_name.into(),
        value_type,
        unit: None,
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

fn missing_response_error(command: &str) -> Error {
    Error::new(
        ErrorCode::Transport,
        format!("Starlight Xpress {command} command did not return a documented response"),
    )
}

fn string_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::String, writable)
}

fn bool_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::Bool, writable)
}

fn map_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::Map, writable)
}

fn integer_range_property(
    key: &str,
    display_name: &str,
    writable: bool,
    min: i64,
    max: i64,
) -> PropertySchema {
    let mut schema = property(key, display_name, ValueType::I64, writable);
    schema.range = Some(Range {
        min: Value::I64(min),
        max: Value::I64(max),
    });
    schema
}

fn string_prop(device: &DeviceConfig, key: &str) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn hid_endpoint(device: &DeviceConfig) -> Result<Option<SxFilterWheelHidEndpoint>> {
    let has_vid_pid =
        device.properties.contains_key("vendor_id") || device.properties.contains_key("product_id");
    if !has_vid_pid {
        return Ok(None);
    }
    if matches!(device.properties.get("usb_hid"), Some(Value::Bool(false))) {
        return Ok(None);
    }
    Ok(Some(SxFilterWheelHidEndpoint {
        vendor_id: required_u16_prop(device, "vendor_id")?,
        product_id: required_u16_prop(device, "product_id")?,
        serial_number: string_prop(device, "hid_serial_number")
            .or_else(|| string_prop(device, "serial_number")),
        report_id: u8_prop(device, "report_id").unwrap_or(0),
        timeout_ms: u32_prop(device, "hid_timeout_ms").unwrap_or(100) as i32,
    }))
}

fn wants_usb_hid(device: &DeviceConfig) -> bool {
    matches!(device.properties.get("usb_hid"), Some(Value::Bool(true)))
}

#[cfg(feature = "os-hid")]
fn autodiscover_hid_endpoint(device: &DeviceConfig) -> Result<Option<SxFilterWheelHidEndpoint>> {
    let serial_filter = explicit_string_prop(device, "hid_serial_number")
        .or_else(|| explicit_string_prop(device, "serial_number"));
    let matches = enumerate_hid_devices()?
        .into_iter()
        .filter(is_sx_filter_wheel_identity)
        .filter(|identity| {
            serial_filter
                .as_ref()
                .is_none_or(|serial| identity.serial_number.as_ref() == Some(serial))
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [identity] => Ok(Some(SxFilterWheelHidEndpoint {
            vendor_id: identity.vendor_id,
            product_id: identity.product_id,
            serial_number: identity.serial_number.clone(),
            report_id: u8_prop(device, "report_id").unwrap_or(0),
            timeout_ms: u32_prop(device, "hid_timeout_ms").unwrap_or(100) as i32,
        })),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            "multiple Starlight Xpress HID filter-wheel candidates found; set vendor_id/product_id or hid_serial_number",
        )),
    }
}

#[cfg(not(feature = "os-hid"))]
fn autodiscover_hid_endpoint(_device: &DeviceConfig) -> Result<Option<SxFilterWheelHidEndpoint>> {
    Err(Error::new(
        ErrorCode::Unsupported,
        "Starlight Xpress HID autodiscovery requires numanager-drivers/os-hid",
    ))
}

#[cfg(feature = "os-hid")]
fn is_sx_filter_wheel_identity(identity: &HidDeviceIdentity) -> bool {
    let Some(product) = identity.product_string.as_ref() else {
        return false;
    };
    let normalized = product
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect::<String>();
    (normalized.contains("starlightxpress") || normalized.contains("sx"))
        && (normalized.contains("filterwheel") || normalized.contains("wheel"))
}

#[cfg(feature = "os-hid")]
fn explicit_string_prop(device: &DeviceConfig, key: &str) -> Option<String> {
    device
        .properties
        .contains_key(key)
        .then(|| string_prop(device, key))?
}

fn bool_prop(device: &DeviceConfig, key: &str) -> Option<bool> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn u8_prop(device: &DeviceConfig, key: &str) -> Option<u8> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => (*value).try_into().ok(),
        _ => None,
    }
}

fn required_u16_prop(device: &DeviceConfig, key: &str) -> Result<u16> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => (*value).try_into().map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("Starlight Xpress property {key} must fit in u16"),
            )
        }),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Starlight Xpress USB HID config requires property.{key}"),
        )),
    }
}

fn u32_prop(device: &DeviceConfig, key: &str) -> Option<u32> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => (*value).try_into().ok(),
        _ => None,
    }
}
