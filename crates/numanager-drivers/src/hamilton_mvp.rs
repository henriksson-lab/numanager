use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::{LineEnding, ScriptedSerial, SerialIo, SerialLineCodec};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
#[cfg(feature = "os-serial")]
use std::time::Duration;

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const BAUD: u32 = 9_600;
    pub const DATA_BITS: u8 = 7;
    pub const STOP_BITS: u8 = 1;
    pub const ACK: u8 = 0x06;
    pub const NAK: u8 = 0x15;
    pub const SEND_ENDING: LineEnding = LineEnding::Cr;
    pub const RECV_ENDING: LineEnding = LineEnding::Cr;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct HamiltonMvpProbe {
        pub model: String,
        pub serial_number: String,
        pub address: char,
        pub port_count: u8,
        pub position: u8,
        pub firmware: Option<String>,
        pub initialized: bool,
        pub valve_error: bool,
        pub busy: bool,
        pub valve_type: Option<u8>,
    }

    impl HamiltonMvpProbe {
        pub fn configured_fixture() -> Self {
            Self {
                model: "Serial MVP".into(),
                serial_number: "MVP-CONFIG-0001".into(),
                address: 'a',
                port_count: 8,
                position: 1,
                firmware: Some("MV configured".into()),
                initialized: true,
                valve_error: false,
                busy: false,
                valve_type: Some(2),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum HamiltonMvpCommand {
        Firmware,
        Done,
        ValveError,
        Status,
        Initialize,
        QueryPosition,
        QueryValveType,
        SelectPosition {
            position: u8,
            direction: ValveDirection,
        },
    }

    pub fn encode(address: char, command: &HamiltonMvpCommand) -> String {
        let body = match command {
            HamiltonMvpCommand::Firmware => "U".into(),
            HamiltonMvpCommand::Done => "F".into(),
            HamiltonMvpCommand::ValveError => "G".into(),
            HamiltonMvpCommand::Status => "E1".into(),
            HamiltonMvpCommand::Initialize => "LXR".into(),
            HamiltonMvpCommand::QueryPosition => "LQP".into(),
            HamiltonMvpCommand::QueryValveType => "LQT".into(),
            HamiltonMvpCommand::SelectPosition {
                position,
                direction,
            } => {
                format!("LP{}{}R", direction_code(*direction), position)
            }
        };
        format!("{address}{body}")
    }

    pub fn validate_address(address: char) -> Result<()> {
        if ('a'..='p').contains(&address) {
            Ok(())
        } else {
            Err(Error::new(
                ErrorCode::InvalidProperty,
                "Hamilton MVP Protocol 1/RNO+ address must be in a..p",
            ))
        }
    }

    pub fn validate_position(position: u8, port_count: u8) -> Result<()> {
        if (1..=port_count.min(8)).contains(&position) {
            Ok(())
        } else {
            Err(Error::new(
                ErrorCode::InvalidProperty,
                format!(
                    "Hamilton MVP valve position must be in 1..={}",
                    port_count.min(8)
                ),
            ))
        }
    }

    pub fn validate_port_count(port_count: u8) -> Result<()> {
        if (1..=8).contains(&port_count) {
            Ok(())
        } else {
            Err(Error::new(
                ErrorCode::InvalidProperty,
                "Hamilton MVP port_count must be in 1..=8",
            ))
        }
    }

    pub fn ack_or_response(bytes: &[u8]) -> Result<Option<String>> {
        if bytes.is_empty() {
            return Ok(None);
        }
        if bytes.contains(&NAK) {
            return Err(Error::new(
                ErrorCode::Transport,
                "Hamilton MVP returned NAK",
            ));
        }
        let Some(ack_index) = bytes.iter().position(|byte| *byte == ACK) else {
            return Err(Error::new(
                ErrorCode::Transport,
                "Hamilton MVP response did not contain ACK",
            ));
        };
        let response = bytes[ack_index + 1..]
            .iter()
            .copied()
            .filter(|byte| *byte != b'\r' && *byte != b'\n')
            .collect::<Vec<_>>();
        if response.is_empty() {
            Ok(Some(String::new()))
        } else {
            Ok(Some(String::from_utf8_lossy(&response).trim().to_string()))
        }
    }

    pub fn parse_done(response: &str) -> Result<Option<bool>> {
        parse_yn_busy(response).map(|value| value.map(|state| state == 'Y'))
    }

    pub fn parse_valve_error(response: &str) -> Result<Option<bool>> {
        parse_yn_busy(response).map(|value| value.map(|state| state == 'Y'))
    }

    pub fn parse_status(response: &str) -> Result<Option<HamiltonStatus>> {
        let Some(byte) = response.as_bytes().first().copied() else {
            return Ok(None);
        };
        Ok(Some(HamiltonStatus {
            buffer_not_empty: byte & 0x01 != 0,
            syringe_busy: byte & 0x02 != 0,
            valve_busy: byte & 0x04 != 0,
            syntax_error: byte & 0x08 != 0,
            instrument_error: byte & 0x10 != 0,
            raw: byte,
        }))
    }

    pub fn parse_position(response: &str, port_count: u8) -> Result<Option<u8>> {
        let Some(position) = parse_decimal_u8(response, "position")? else {
            return Ok(None);
        };
        validate_position(position, port_count)?;
        Ok(Some(position))
    }

    pub fn parse_valve_type(response: &str) -> Result<Option<(u8, u8)>> {
        let Some(valve_type) = parse_decimal_u8(response, "valve type")? else {
            return Ok(None);
        };
        let port_count = match valve_type {
            2 => 8,
            3 => 6,
            4 => 3,
            5 | 6 => 2,
            7 => 4,
            _ => {
                return Err(Error::new(
                    ErrorCode::Transport,
                    format!("unsupported Hamilton MVP valve type {valve_type}"),
                ))
            }
        };
        Ok(Some((valve_type, port_count)))
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct HamiltonStatus {
        pub buffer_not_empty: bool,
        pub syringe_busy: bool,
        pub valve_busy: bool,
        pub syntax_error: bool,
        pub instrument_error: bool,
        pub raw: u8,
    }

    fn parse_yn_busy(response: &str) -> Result<Option<char>> {
        match response.chars().next() {
            Some('Y' | 'N' | '*') => Ok(response.chars().next()),
            None => Ok(None),
            Some(other) => Err(Error::new(
                ErrorCode::Transport,
                format!("invalid Hamilton MVP status response {other}"),
            )),
        }
    }

    fn parse_decimal_u8(response: &str, label: &str) -> Result<Option<u8>> {
        let response = response.trim();
        if response.is_empty() {
            return Ok(None);
        }
        if !response.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("invalid Hamilton MVP {label} response {response}"),
            ));
        }
        response.parse::<u8>().map(Some).map_err(|_| {
            Error::new(
                ErrorCode::Transport,
                format!("Hamilton MVP {label} response is out of range"),
            )
        })
    }

    fn direction_code(direction: ValveDirection) -> u8 {
        match direction {
            ValveDirection::Clockwise => 0,
            ValveDirection::CounterClockwise => 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HamiltonMvpConfiguredProbe {
    label: String,
    endpoint: Option<HamiltonMvpSerialEndpoint>,
    connect_real_transport: bool,
    completion_poll_limit: usize,
    model: String,
    serial_number: String,
    firmware: Option<String>,
    valves: Vec<protocol::HamiltonMvpProbe>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HamiltonMvpSerialEndpoint {
    pub port_name: String,
    pub timeout_ms: u64,
}

pub struct HamiltonMvpDiscovery {
    next_id: DriverId,
    probes: Vec<HamiltonMvpConfiguredProbe>,
}

impl HamiltonMvpDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![HamiltonMvpConfiguredProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), "hamilton_mvp" | "hamilton-mvp"))
            .map(HamiltonMvpConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for HamiltonMvpDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver: Box<dyn Driver> = if configured.connect_real_transport {
                    let endpoint = configured.endpoint.clone().ok_or_else(|| {
                        Error::new(
                            ErrorCode::InvalidProperty,
                            "Hamilton MVP config requires serial_port when connect is true",
                        )
                    })?;
                    Box::new(HamiltonMvpDriver::serial(id, configured, endpoint)?)
                } else {
                    Box::new(HamiltonMvpDriver::configured(id, configured))
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

impl HamiltonMvpConfiguredProbe {
    pub fn fixture() -> Self {
        let probe = protocol::HamiltonMvpProbe::configured_fixture();
        Self {
            label: "Configured Hamilton Serial MVP fixture".into(),
            endpoint: None,
            connect_real_transport: false,
            completion_poll_limit: 20,
            model: probe.model.clone(),
            serial_number: probe.serial_number.clone(),
            firmware: probe.firmware.clone(),
            valves: vec![probe],
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::fixture();
        configured.label = if device.label.is_empty() {
            "Configured Hamilton Serial MVP".into()
        } else {
            device.label.clone()
        };
        configured.model = string_prop(device, "model")?.unwrap_or(configured.model);
        configured.serial_number =
            string_prop(device, "serial_number")?.unwrap_or(configured.serial_number);
        configured.firmware = string_prop(device, "firmware")?.or(configured.firmware);
        let addresses = match address_list_prop(device, "addresses")? {
            Some(addresses) => addresses,
            None => string_prop(device, "address")?
                .map(|address| vec![address])
                .unwrap_or_else(|| vec!["a".into()]),
        };
        if addresses.len() > 16 {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Hamilton MVP daisy-chain address count must be at most 16",
            ));
        }
        let port_count = u8_prop(device, "port_count")?.unwrap_or(8);
        protocol::validate_port_count(port_count)?;
        let position = u8_prop(device, "position")?.unwrap_or(1);
        protocol::validate_position(position, port_count)?;
        let mut seen_addresses = Vec::new();
        configured.valves = addresses
            .into_iter()
            .map(|address| {
                let address = parse_address(&address)?;
                if seen_addresses.contains(&address) {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Hamilton MVP daisy-chain addresses must be unique",
                    ));
                }
                seen_addresses.push(address);
                let mut probe = protocol::HamiltonMvpProbe::configured_fixture();
                probe.model = configured.model.clone();
                probe.serial_number = configured.serial_number.clone();
                probe.firmware = configured.firmware.clone();
                probe.address = address;
                probe.port_count = port_count;
                probe.position = position;
                Ok(probe)
            })
            .collect::<Result<Vec<_>>>()?;
        if configured.valves.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Hamilton MVP addresses must not be empty",
            ));
        }
        configured.completion_poll_limit = usize_prop(device, "completion_poll_limit")?
            .unwrap_or(configured.completion_poll_limit);
        if configured.completion_poll_limit == 0 {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Hamilton MVP completion_poll_limit must be at least 1",
            ));
        }
        let timeout_ms = u64_prop(device, "serial_timeout_ms")?;
        configured.endpoint = match string_prop(device, "serial_port")? {
            Some(port_name) => Some(HamiltonMvpSerialEndpoint {
                port_name,
                timeout_ms: timeout_ms.unwrap_or(100),
            }),
            None => None,
        };
        configured.connect_real_transport = bool_prop(device, "connect")?.unwrap_or(false);
        Ok(configured)
    }
}

pub struct HamiltonMvpDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    valves: Vec<HamiltonMvpValve>,
    model: String,
    serial_number: String,
    completion_poll_limit: usize,
    last_transaction: Value,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
    fixture_mode: bool,
    serial_port: Option<String>,
    serial_timeout_ms: u64,
    connected: bool,
}

#[derive(Debug, Clone)]
struct HamiltonMvpValve {
    device: DeviceId,
    probe: protocol::HamiltonMvpProbe,
    status_raw: u8,
}

impl HamiltonMvpDriver {
    pub fn configured(id: DriverId, configured: HamiltonMvpConfiguredProbe) -> Self {
        Self::new(
            id,
            configured.model,
            configured.serial_number,
            configured.valves,
            configured.completion_poll_limit,
            Box::new(ScriptedSerial::new()),
            true,
            configured
                .endpoint
                .as_ref()
                .map(|endpoint| endpoint.port_name.clone()),
            configured
                .endpoint
                .as_ref()
                .map(|endpoint| endpoint.timeout_ms)
                .unwrap_or(100),
            false,
        )
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(
        id: DriverId,
        configured: HamiltonMvpConfiguredProbe,
        endpoint: HamiltonMvpSerialEndpoint,
    ) -> Result<Self> {
        let port_name = endpoint.port_name.clone();
        let timeout_ms = endpoint.timeout_ms;
        let serial = Box::new(numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(endpoint.port_name, protocol::BAUD)
                .timeout(Duration::from_millis(timeout_ms))
                .data_bits(serialport::DataBits::Seven)
                .parity(serialport::Parity::Odd)
                .stop_bits(serialport::StopBits::One)
                .flow_control(serialport::FlowControl::None),
        )?);
        let mut driver = Self::new(
            id,
            configured.model,
            configured.serial_number,
            configured.valves,
            configured.completion_poll_limit,
            serial,
            false,
            Some(port_name),
            timeout_ms,
            true,
        );
        for index in 0..driver.valves.len() {
            driver.read_firmware_for(index)?;
            driver.read_valve_type_for(index)?;
            driver.read_position_for(index)?;
            driver.refresh_status_for(index)?;
            driver.read_done_for(index)?;
            driver.read_valve_error_for(index)?;
        }
        Ok(driver)
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(
        _id: DriverId,
        _configured: HamiltonMvpConfiguredProbe,
        _endpoint: HamiltonMvpSerialEndpoint,
    ) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Hamilton MVP real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(
        id: DriverId,
        model: String,
        serial_number: String,
        probes: Vec<protocol::HamiltonMvpProbe>,
        completion_poll_limit: usize,
        serial: Box<dyn SerialIo>,
        fixture_mode: bool,
        serial_port: Option<String>,
        serial_timeout_ms: u64,
        connected: bool,
    ) -> Self {
        let valves = probes
            .into_iter()
            .enumerate()
            .map(|(index, probe)| HamiltonMvpValve {
                device: DeviceId(NodeId(id.0 * 1000 + 943 + index as u64)),
                probe,
                status_raw: 0,
            })
            .collect();
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 941)),
            hub: DeviceId(NodeId(id.0 * 1000 + 942)),
            valves,
            model,
            serial_number,
            completion_poll_limit,
            last_transaction: Value::Map(BTreeMap::new()),
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            codec: SerialLineCodec::new(protocol::SEND_ENDING, protocol::RECV_ENDING),
            fixture_mode,
            serial_port,
            serial_timeout_ms,
            connected,
        }
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn valve_index(&self, device: DeviceId) -> Option<usize> {
        self.valves
            .iter()
            .position(|candidate| candidate.device == device)
    }

    fn transact(
        &mut self,
        valve_index: usize,
        command: protocol::HamiltonMvpCommand,
    ) -> Result<Option<String>> {
        let address = self.valves[valve_index].probe.address;
        let encoded = protocol::encode(address, &command);
        self.serial.write(&self.codec.encode(&encoded))?;
        let reply = self.serial.read_available()?;
        if reply.is_empty() && !self.fixture_mode {
            return Err(Error::new(
                ErrorCode::Transport,
                "Hamilton MVP reply was not received",
            ));
        }
        let response = protocol::ack_or_response(&reply)?;
        self.last_transaction = Value::Map(BTreeMap::from([
            ("command".into(), Value::String(encoded)),
            ("reply_len".into(), Value::I64(reply.len() as i64)),
            (
                "response".into(),
                response.clone().map(Value::String).unwrap_or(Value::Null),
            ),
            (
                "completion_basis".into(),
                Value::String(if matches!(command, protocol::HamiltonMvpCommand::Status) {
                    "status_reply".into()
                } else {
                    "ack_then_status".into()
                }),
            ),
        ]));
        Ok(response)
    }

    fn refresh_status(&mut self) -> Result<()> {
        self.refresh_status_for(0)
    }

    fn refresh_status_for(&mut self, valve_index: usize) -> Result<()> {
        let Some(response) = self.transact(valve_index, protocol::HamiltonMvpCommand::Status)?
        else {
            return Ok(());
        };
        if let Some(status) = protocol::parse_status(&response)? {
            let valve = &mut self.valves[valve_index];
            let before_busy = valve.probe.busy;
            let before_valve_error = valve.probe.valve_error;
            let before_status_raw = valve.status_raw;
            valve.probe.busy = status.valve_busy;
            valve.probe.valve_error = status.instrument_error || status.syntax_error;
            valve.status_raw = status.raw;
            let device = valve.device;
            let busy = valve.probe.busy;
            let valve_error = valve.probe.valve_error;
            let status_raw = valve.status_raw;
            if busy != before_busy {
                self.emit_property(device, "busy", Value::Bool(busy));
            }
            if valve_error != before_valve_error {
                self.emit_property(device, "valve_error", Value::Bool(valve_error));
            }
            if status_raw != before_status_raw {
                self.emit_property(device, "status_raw", Value::I64(status_raw as i64));
            }
        }
        Ok(())
    }

    fn wait_valve_idle(&mut self, valve_index: usize) -> Result<()> {
        if self.fixture_mode {
            self.valves[valve_index].probe.busy = false;
            return Ok(());
        }
        for _ in 0..self.completion_poll_limit.max(1) {
            self.refresh_status_for(valve_index)?;
            if !self.valves[valve_index].probe.busy {
                return Ok(());
            }
        }
        Err(Error::new(
            ErrorCode::Timeout,
            "Hamilton MVP valve did not report idle before completion_poll_limit",
        ))
    }

    fn select_position(
        &mut self,
        valve_index: usize,
        request: ValveSelectRequest,
    ) -> Result<Value> {
        let port_count = self.valves[valve_index].probe.port_count;
        protocol::validate_position(request.position, port_count)?;
        let direction = request.direction.unwrap_or(ValveDirection::Clockwise);
        self.transact(
            valve_index,
            protocol::HamiltonMvpCommand::SelectPosition {
                position: request.position,
                direction,
            },
        )?;
        self.valves[valve_index].probe.busy = true;
        self.wait_valve_idle(valve_index)?;
        if self.fixture_mode {
            let valve = &mut self.valves[valve_index];
            valve.probe.position = request.position;
        } else {
            self.read_position_for(valve_index)?;
        }
        let valve = &self.valves[valve_index];
        let device = valve.device;
        let position = valve.probe.position;
        let busy = valve.probe.busy;
        self.emit_property(device, "position", Value::I64(position as i64));
        self.emit_property(device, "busy", Value::Bool(busy));
        self.state_summary_for(valve_index)
    }

    fn read_firmware_for(&mut self, valve_index: usize) -> Result<()> {
        if let Some(response) =
            self.transact(valve_index, protocol::HamiltonMvpCommand::Firmware)?
        {
            if !response.is_empty() {
                self.valves[valve_index].probe.firmware = Some(response);
            }
        }
        Ok(())
    }

    fn read_position_for(&mut self, valve_index: usize) -> Result<()> {
        let port_count = self.valves[valve_index].probe.port_count;
        if let Some(response) =
            self.transact(valve_index, protocol::HamiltonMvpCommand::QueryPosition)?
        {
            if let Some(position) = protocol::parse_position(&response, port_count)? {
                let valve = &mut self.valves[valve_index];
                let before_position = valve.probe.position;
                valve.probe.position = position;
                let device = valve.device;
                if position != before_position {
                    self.emit_property(device, "position", Value::I64(position as i64));
                }
            }
        }
        Ok(())
    }

    fn read_valve_type_for(&mut self, valve_index: usize) -> Result<()> {
        if let Some(response) =
            self.transact(valve_index, protocol::HamiltonMvpCommand::QueryValveType)?
        {
            if let Some((valve_type, port_count)) = protocol::parse_valve_type(&response)? {
                let valve = &mut self.valves[valve_index];
                let before_type = valve.probe.valve_type;
                let before_port_count = valve.probe.port_count;
                valve.probe.valve_type = Some(valve_type);
                valve.probe.port_count = port_count;
                let device = valve.device;
                if before_type != Some(valve_type) {
                    self.emit_property(device, "valve_type", Value::I64(valve_type as i64));
                }
                if port_count != before_port_count {
                    self.emit_property(device, "port_count", Value::I64(port_count as i64));
                }
            }
        }
        Ok(())
    }

    fn read_done_for(&mut self, valve_index: usize) -> Result<()> {
        if let Some(response) = self.transact(valve_index, protocol::HamiltonMvpCommand::Done)? {
            if let Some(done) = protocol::parse_done(&response)? {
                let valve = &mut self.valves[valve_index];
                let before_busy = valve.probe.busy;
                valve.probe.busy = !done;
                let device = valve.device;
                let busy = valve.probe.busy;
                if busy != before_busy {
                    self.emit_property(device, "busy", Value::Bool(busy));
                }
            }
        }
        Ok(())
    }

    fn read_valve_error_for(&mut self, valve_index: usize) -> Result<()> {
        if let Some(response) =
            self.transact(valve_index, protocol::HamiltonMvpCommand::ValveError)?
        {
            if let Some(valve_error) = protocol::parse_valve_error(&response)? {
                let valve = &mut self.valves[valve_index];
                let before_valve_error = valve.probe.valve_error;
                valve.probe.valve_error = valve_error;
                let device = valve.device;
                if valve_error != before_valve_error {
                    self.emit_property(device, "valve_error", Value::Bool(valve_error));
                }
            }
        }
        Ok(())
    }

    fn emit_property(&mut self, device: DeviceId, key: &str, value: Value) {
        self.pending
            .push_back(DriverEvent::Event(Event::PropertyChanged(
                PropertyChanged {
                    device,
                    key: key.into(),
                    value,
                },
            )));
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        match (device, key) {
            (device, "model") if device == self.hub => Ok(Value::String(self.model.clone())),
            (device, "serial_number") if device == self.hub => {
                Ok(Value::String(self.serial_number.clone()))
            }
            (device, "protocol") if device == self.hub => {
                Ok(Value::String("Hamilton Protocol 1/RNO+".into()))
            }
            (device, "address") if device == self.hub => {
                let addresses = self
                    .valves
                    .iter()
                    .map(|valve| valve.probe.address.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                Ok(Value::String(addresses))
            }
            (device, "firmware") if device == self.hub => Ok(Value::String(
                self.valves
                    .first()
                    .and_then(|valve| valve.probe.firmware.clone())
                    .or_else(|| {
                        self.valves
                            .iter()
                            .find_map(|valve| valve.probe.firmware.clone())
                    })
                    .clone()
                    .unwrap_or_else(|| "unknown".into()),
            )),
            (device, "valve_count") if device == self.hub => {
                Ok(Value::I64(self.valves.len() as i64))
            }
            (device, "valve_addresses") if device == self.hub => Ok(Value::List(
                self.valves
                    .iter()
                    .map(|valve| Value::String(valve.probe.address.to_string()))
                    .collect(),
            )),
            (device, "last_transaction") if device == self.hub => Ok(self.last_transaction.clone()),
            (device, "position") if self.valve_index(device).is_some() => {
                let valve = &self.valves[self.valve_index(device).expect("checked valve")];
                Ok(Value::I64(valve.probe.position as i64))
            }
            (device, "port_count") if self.valve_index(device).is_some() => {
                let valve = &self.valves[self.valve_index(device).expect("checked valve")];
                Ok(Value::I64(valve.probe.port_count as i64))
            }
            (device, "valve_type") if self.valve_index(device).is_some() => {
                let valve = &self.valves[self.valve_index(device).expect("checked valve")];
                Ok(valve
                    .probe
                    .valve_type
                    .map(|valve_type| Value::I64(valve_type as i64))
                    .unwrap_or(Value::Null))
            }
            (device, "address") if self.valve_index(device).is_some() => {
                let valve = &self.valves[self.valve_index(device).expect("checked valve")];
                Ok(Value::String(valve.probe.address.to_string()))
            }
            (device, "initialized") if self.valve_index(device).is_some() => {
                let valve = &self.valves[self.valve_index(device).expect("checked valve")];
                Ok(Value::Bool(valve.probe.initialized))
            }
            (device, "busy") if self.valve_index(device).is_some() => {
                let valve = &self.valves[self.valve_index(device).expect("checked valve")];
                Ok(Value::Bool(valve.probe.busy))
            }
            (device, "valve_error") if self.valve_index(device).is_some() => {
                let valve = &self.valves[self.valve_index(device).expect("checked valve")];
                Ok(Value::Bool(valve.probe.valve_error))
            }
            (device, "status_raw") if self.valve_index(device).is_some() => {
                let valve = &self.valves[self.valve_index(device).expect("checked valve")];
                Ok(Value::I64(valve.status_raw as i64))
            }
            (device, "state_summary") if self.valve_index(device).is_some() => {
                self.state_summary_for(self.valve_index(device).expect("checked valve"))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Hamilton MVP property {key}"),
            )),
        }
    }

    fn state_summary_for(&self, valve_index: usize) -> Result<Value> {
        let valve = self.valves.get(valve_index).ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidCommand,
                "Hamilton MVP valve index is out of range",
            )
        })?;
        Ok(Value::Map(BTreeMap::from([
            (
                "address".into(),
                Value::String(valve.probe.address.to_string()),
            ),
            ("position".into(), Value::I64(valve.probe.position as i64)),
            (
                "port_count".into(),
                Value::I64(valve.probe.port_count as i64),
            ),
            (
                "valve_type".into(),
                valve
                    .probe
                    .valve_type
                    .map(|valve_type| Value::I64(valve_type as i64))
                    .unwrap_or(Value::Null),
            ),
            ("initialized".into(), Value::Bool(valve.probe.initialized)),
            ("busy".into(), Value::Bool(valve.probe.busy)),
            ("valve_error".into(), Value::Bool(valve.probe.valve_error)),
            ("status_raw".into(), Value::I64(valve.status_raw as i64)),
        ])))
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        match (self.valve_index(device), key, value) {
            (Some(index), "position", Value::I64(position)) => {
                let position = u8::try_from(*position).map_err(|_| {
                    Error::new(
                        ErrorCode::InvalidProperty,
                        "Hamilton MVP valve position must fit in u8",
                    )
                })?;
                protocol::validate_position(position, self.valves[index].probe.port_count)
            }
            (Some(_), _, _) => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Hamilton MVP property {key} is read-only or has the wrong type"),
            )),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                "Hamilton MVP write targets an unknown device",
            )),
        }
    }

    fn invoke(
        &mut self,
        device: DeviceId,
        capability: CapabilityId,
        request: CapabilityRequest,
    ) -> Result<Value> {
        let descriptor = self
            .capabilities(device)
            .into_iter()
            .find(|candidate| candidate.id == capability)
            .ok_or_else(|| Error::new(ErrorCode::Unsupported, "unknown Hamilton MVP capability"))?;
        match (descriptor.kind, request) {
            (CapabilityKind::ValveSelect, CapabilityRequest::ValveSelect(request)) => {
                let valve_index = self.valve_index(device).ok_or_else(|| {
                    Error::new(
                        ErrorCode::InvalidCommand,
                        "unknown Hamilton MVP valve device",
                    )
                })?;
                self.select_position(valve_index, request)
            }
            (CapabilityKind::GenericCommand, CapabilityRequest::GenericCommand(request)) => {
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
                        "Hamilton MVP GenericCommand commands do not accept params",
                    ));
                }
                match request.command.as_str() {
                    "refresh_status" => {
                        let mut values = BTreeMap::new();
                        for index in 0..self.valves.len() {
                            self.refresh_status_for(index)?;
                            let address = self.valves[index].probe.address.to_string();
                            values.insert(address, self.state_summary_for(index)?);
                        }
                        Ok(Value::Map(values))
                    }
                    "read_done" => {
                        let mut values = BTreeMap::new();
                        for index in 0..self.valves.len() {
                            self.read_done_for(index)?;
                            let address = self.valves[index].probe.address.to_string();
                            values
                                .insert(address, self.read_property(self.valves[index].device, "busy")?);
                        }
                        Ok(Value::Map(values))
                    }
                    "read_position" => {
                        let mut values = BTreeMap::new();
                        for index in 0..self.valves.len() {
                            self.read_position_for(index)?;
                            let address = self.valves[index].probe.address.to_string();
                            values.insert(
                                address,
                                self.read_property(self.valves[index].device, "position")?,
                            );
                        }
                        Ok(Value::Map(values))
                    }
                    "read_valve_type" => {
                        let mut values = BTreeMap::new();
                        for index in 0..self.valves.len() {
                            self.read_valve_type_for(index)?;
                            let address = self.valves[index].probe.address.to_string();
                            values.insert(address, self.state_summary_for(index)?);
                        }
                        Ok(Value::Map(values))
                    }
                    "read_valve_error" => {
                        let mut values = BTreeMap::new();
                        for index in 0..self.valves.len() {
                            self.read_valve_error_for(index)?;
                            let address = self.valves[index].probe.address.to_string();
                            values.insert(
                                address,
                                self.read_property(self.valves[index].device, "valve_error")?,
                            );
                        }
                        Ok(Value::Map(values))
                    }
                    _ => Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "Hamilton MVP GenericCommand supports refresh_status, read_done, read_position, read_valve_type, and read_valve_error",
                    )),
                }
            }
            (CapabilityKind::ValveSelect, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Hamilton MVP ValveSelect expects ValveSelectRequest",
            )),
            (CapabilityKind::GenericCommand, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Hamilton MVP GenericCommand expects GenericCommandRequest",
            )),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Hamilton MVP capability",
            )),
        }
    }
}

impl Driver for HamiltonMvpDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "Hamilton MVP serial transport".into(),
            kind: "serial".into(),
            metadata: BTreeMap::from([
                ("baud_rate".into(), Value::I64(protocol::BAUD as i64)),
                (
                    "serial_port".into(),
                    self.serial_port
                        .as_ref()
                        .map(|port| Value::String(port.clone()))
                        .unwrap_or(Value::Null),
                ),
                (
                    "serial_timeout".into(),
                    Value::TimeInterval(TimeInterval::from_milliseconds(
                        self.serial_timeout_ms as f64,
                    )),
                ),
                ("connected".into(), Value::Bool(self.connected)),
                ("data_bits".into(), Value::I64(protocol::DATA_BITS as i64)),
                ("stop_bits".into(), Value::I64(protocol::STOP_BITS as i64)),
                ("parity".into(), Value::String("odd".into())),
                (
                    "protocol".into(),
                    Value::String("Hamilton Protocol 1/RNO+".into()),
                ),
            ]),
        }]
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        let mut descriptors = vec![DeviceDescriptor {
            id: self.hub,
            driver: self.id,
            label: "hamilton-mvp-hub".into(),
            vendor: Some("Hamilton".into()),
            model: Some(self.model.clone()),
            serial: Some(self.serial_number.clone()),
            kinds: vec![
                "hub".into(),
                "fluidics.controller".into(),
                "hamilton.mvp".into(),
            ],
            properties: vec![
                string_property("model", "Model", false),
                string_property("serial_number", "Serial number", false),
                string_property("protocol", "Protocol", false),
                string_property("address", "Addresses", false),
                string_property("firmware", "Firmware", false),
                integer_range_property("valve_count", "Valve count", false, 1, 16),
                property("valve_addresses", "Valve addresses", ValueType::List, false),
                map_property("last_transaction", "Last transaction", false),
            ],
            metadata: BTreeMap::from([
                (
                    "source".into(),
                    Value::String("Hamilton Protocol 1/RNO+".into()),
                ),
                (
                    "daisy_chain_addresses".into(),
                    Value::List(
                        self.valves
                            .iter()
                            .map(|valve| Value::String(valve.probe.address.to_string()))
                            .collect(),
                    ),
                ),
            ]),
        }];
        descriptors.extend(self.valves.iter().map(|valve| DeviceDescriptor {
            id: valve.device,
            driver: self.id,
            label: format!("hamilton-mvp-valve-{}", valve.probe.address),
            vendor: Some("Hamilton".into()),
            model: Some(self.model.clone()),
            serial: Some(self.serial_number.clone()),
            kinds: vec![
                "fluidics.valve".into(),
                "state.device".into(),
                "hamilton.mvp.valve".into(),
            ],
            properties: vec![
                integer_range_property(
                    "position",
                    "Position",
                    true,
                    1,
                    valve.probe.port_count.min(8) as i64,
                ),
                integer_range_property("port_count", "Port count", false, 1, 8),
                integer_range_property("valve_type", "Valve type", false, 2, 7),
                string_property("address", "Address", false),
                bool_property("initialized", "Initialized", false),
                bool_property("busy", "Busy", false),
                bool_property("valve_error", "Valve error", false),
                integer_range_property("status_raw", "Raw status byte", false, 0, 127),
                map_property("state_summary", "State summary", false),
            ],
            metadata: BTreeMap::from([
                (
                    "address".into(),
                    Value::String(valve.probe.address.to_string()),
                ),
                (
                    "port_count".into(),
                    Value::I64(valve.probe.port_count as i64),
                ),
            ]),
        }));
        descriptors
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if self.valve_index(device).is_some() {
            vec![capability(1, device, CapabilityKind::ValveSelect)]
        } else if device == self.hub {
            vec![capability(2, device, CapabilityKind::GenericCommand)]
        } else {
            Vec::new()
        }
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut physical_transactions = Vec::new();
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    let _ = self.read_property(*device, key)?;
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("hamilton read {key}"),
                        Value::String(key.clone()),
                    ));
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("hamilton write {key}"),
                        value.clone(),
                    ));
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(transaction(
                        self.resource,
                        "hamilton remultiplexed valve state set",
                        Value::List(
                            set.writes
                                .iter()
                                .map(|write| Value::String(write.property.clone()))
                                .collect(),
                        ),
                    ));
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    let descriptor = self
                        .capabilities(*device)
                        .into_iter()
                        .find(|candidate| candidate.id == *capability)
                        .ok_or_else(|| {
                            Error::new(ErrorCode::Unsupported, "unknown Hamilton MVP capability")
                        })?;
                    if !descriptor.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "Hamilton MVP capability request type does not match descriptor",
                        ));
                    }
                    if descriptor.kind == CapabilityKind::GenericCommand {
                        let CapabilityRequest::GenericCommand(request) = request else {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Hamilton MVP GenericCommand expects GenericCommandRequest",
                            ));
                        };
                        if request.is_hidden_maintenance() {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                format!(
                                    "GenericCommand {} is a hidden maintenance operation",
                                    request.command
                                ),
                            ));
                        }
                        if !matches!(
                            request.command.as_str(),
                            "refresh_status"
                                | "read_done"
                                | "read_position"
                                | "read_valve_type"
                                | "read_valve_error"
                        ) {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Hamilton MVP GenericCommand supports refresh_status, read_done, read_position, read_valve_type, and read_valve_error",
                            ));
                        }
                        if !request.params.is_empty() {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Hamilton MVP GenericCommand commands do not accept params",
                            ));
                        }
                    }
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("hamilton invoke {}", descriptor.kind.name()),
                        Value::String(descriptor.kind.name().into()),
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
                Command::ReadProperty { device, key } => {
                    if device == self.hub && key == "firmware" {
                        for index in 0..self.valves.len() {
                            self.read_firmware_for(index)?;
                        }
                    } else if let Some(index) = self.valve_index(device) {
                        if key == "position" {
                            self.read_position_for(index)?;
                        } else if key == "port_count" || key == "valve_type" {
                            self.read_valve_type_for(index)?;
                        } else if key == "busy" {
                            self.read_done_for(index)?;
                        } else if key == "valve_error" {
                            self.read_valve_error_for(index)?;
                        } else if key == "status_raw" || key == "state_summary" {
                            self.refresh_status_for(index)?;
                        }
                    } else if device == self.hub && (key == "status_raw" || key == "state_summary")
                    {
                        self.refresh_status()?;
                    }
                    last = self.read_property(device, &key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(device, &key, &value)?;
                    if key == "position" {
                        let valve_index = self.valve_index(device).ok_or_else(|| {
                            Error::new(
                                ErrorCode::InvalidCommand,
                                "unknown Hamilton MVP valve device",
                            )
                        })?;
                        let position = match value {
                            Value::I64(position) => position as u8,
                            _ => unreachable!("validated write"),
                        };
                        last = self
                            .select_position(valve_index, ValveSelectRequest::position(position))?;
                    }
                }
                Command::ApplyStateSet(set) => {
                    let mut map = BTreeMap::new();
                    for write in set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                        if write.property == "position" {
                            let valve_index = self.valve_index(write.device).ok_or_else(|| {
                                Error::new(
                                    ErrorCode::InvalidCommand,
                                    "unknown Hamilton MVP valve device",
                                )
                            })?;
                            let Value::I64(position) = write.value else {
                                unreachable!("validated write")
                            };
                            let value = self.select_position(
                                valve_index,
                                ValveSelectRequest::position(position as u8),
                            )?;
                            map.insert(self.valves[valve_index].probe.address.to_string(), value);
                        }
                    }
                    last = Value::Map(map);
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    last = self.invoke(device, capability, request)?;
                }
                Command::Arm(_) | Command::Start(_) | Command::Stop(_) => {}
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

fn capability(id: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    CapabilityDescriptor::new(CapabilityId(id), device, kind, ValueType::Map)
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

fn string_prop(device: &DeviceConfig, key: &str) -> Result<Option<String>> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Hamilton MVP property {key} must be String"),
        )),
        None => Ok(None),
    }
}

fn address_list_prop(device: &DeviceConfig, key: &str) -> Result<Option<Vec<String>>> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Ok(Some(
            value
                .split(',')
                .map(str::trim)
                .filter(|address| !address.is_empty())
                .map(str::to_string)
                .collect(),
        )),
        Some(Value::List(values)) => values
            .iter()
            .map(|value| match value {
                Value::String(address) => Ok(address.clone()),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Hamilton MVP addresses entries must be String",
                )),
            })
            .collect::<Result<Vec<_>>>()
            .map(Some),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            "Hamilton MVP addresses must be String or List<String>",
        )),
        None => Ok(None),
    }
}

fn parse_address(value: &str) -> Result<char> {
    let mut chars = value.chars();
    let address = chars.next().ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidProperty,
            "Hamilton MVP address must not be empty",
        )
    })?;
    if chars.next().is_some() {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "Hamilton MVP address must be a single character in a..p",
        ));
    }
    protocol::validate_address(address)?;
    Ok(address)
}

fn bool_prop(device: &DeviceConfig, key: &str) -> Result<Option<bool>> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Hamilton MVP property {key} must be Bool"),
        )),
        None => Ok(None),
    }
}

fn u8_prop(device: &DeviceConfig, key: &str) -> Result<Option<u8>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => u8::try_from(*value).map(Some).map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("Hamilton MVP property {key} must fit in an unsigned byte"),
            )
        }),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Hamilton MVP property {key} must be I64"),
        )),
        None => Ok(None),
    }
}

fn u64_prop(device: &DeviceConfig, key: &str) -> Result<Option<u64>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => u64::try_from(*value).map(Some).map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("Hamilton MVP property {key} must fit in an unsigned 64-bit integer"),
            )
        }),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Hamilton MVP property {key} must be I64"),
        )),
        None => Ok(None),
    }
}

fn usize_prop(device: &DeviceConfig, key: &str) -> Result<Option<usize>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => usize::try_from(*value).map(Some).map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("Hamilton MVP property {key} must be a non-negative count"),
            )
        }),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Hamilton MVP property {key} must be I64"),
        )),
        None => Ok(None),
    }
}
