use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::{FixedBinaryCodec, ScriptedSerial, SerialIo};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
#[cfg(feature = "os-serial")]
use std::time::Duration;

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const FRAME_LEN: usize = 9;
    pub const DEFAULT_BAUD: u32 = 9_600;
    pub const DEFAULT_MODULE_ADDRESS: u8 = 1;
    pub const DEFAULT_HOST_ADDRESS: u8 = 2;
    pub const STATUS_OK: u8 = 100;
    pub const STATUS_POSITION_REACHED_EVENT: u8 = 128;

    pub const CMD_MST: u8 = 3;
    pub const CMD_MVP: u8 = 4;
    pub const CMD_SAP: u8 = 5;
    pub const CMD_GAP: u8 = 6;
    pub const CMD_GET_FIRMWARE_VERSION: u8 = 136;

    pub const MVP_ABS: u8 = 0;
    pub const MVP_REL: u8 = 1;

    pub const AP_TARGET_POSITION: u8 = 0;
    pub const AP_ACTUAL_POSITION: u8 = 1;
    pub const AP_TARGET_SPEED: u8 = 2;
    pub const AP_ACTUAL_SPEED: u8 = 3;
    pub const AP_MAX_POSITIONING_SPEED: u8 = 4;
    pub const AP_MAX_ACCELERATION: u8 = 5;
    pub const AP_POSITION_REACHED: u8 = 8;
    pub const AP_HOME_SWITCH: u8 = 9;
    pub const AP_RIGHT_LIMIT_SWITCH: u8 = 10;
    pub const AP_LEFT_LIMIT_SWITCH: u8 = 11;

    #[derive(Debug, Clone, PartialEq)]
    pub struct TmclProbe {
        pub model: String,
        pub serial_number: String,
        pub firmware_version_raw: i32,
        pub module_address: u8,
        pub host_address: u8,
        pub axes: Vec<TmclAxisProbe>,
    }

    impl TmclProbe {
        pub fn configured_fixture() -> Self {
            Self {
                model: "TMCL configured model".into(),
                serial_number: "TMCL-CONFIG-0001".into(),
                firmware_version_raw: 0,
                module_address: DEFAULT_MODULE_ADDRESS,
                host_address: DEFAULT_HOST_ADDRESS,
                axes: vec![TmclAxisProbe {
                    axis_index: 0,
                    stage_axis: StageAxis::X,
                    step_size_um: 0.1,
                    travel_um: 25_000.0,
                    position_steps: 0,
                    target_steps: 0,
                    actual_speed: 0,
                    max_positioning_speed: 51_200,
                    max_acceleration: 10_000,
                    position_reached: true,
                    home_switch: false,
                    left_limit_switch: false,
                    right_limit_switch: false,
                }],
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct TmclAxisProbe {
        pub axis_index: u8,
        pub stage_axis: StageAxis,
        pub step_size_um: f64,
        pub travel_um: f64,
        pub position_steps: i32,
        pub target_steps: i32,
        pub actual_speed: i32,
        pub max_positioning_speed: i32,
        pub max_acceleration: i32,
        pub position_reached: bool,
        pub home_switch: bool,
        pub left_limit_switch: bool,
        pub right_limit_switch: bool,
    }

    impl TmclAxisProbe {
        pub fn steps_from_position(&self, position: Position) -> Result<i32> {
            let um = position.micrometers();
            if !um.is_finite() || self.step_size_um <= 0.0 {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "TMCL position or step size is invalid",
                ));
            }
            let steps = (um / self.step_size_um).round();
            if steps < i32::MIN as f64 || steps > i32::MAX as f64 {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "TMCL target position exceeds signed 32-bit microstep range",
                ));
            }
            Ok(steps as i32)
        }

        pub fn position_from_steps(&self, steps: i32) -> Position {
            Position::from_micrometers(steps as f64 * self.step_size_um)
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum TmclCommand {
        MoveAbsolute { axis: u8, position_steps: i32 },
        MoveRelative { axis: u8, delta_steps: i32 },
        Stop { axis: u8 },
        GetAxisParameter { axis: u8, parameter: u8 },
        SetAxisParameter { axis: u8, parameter: u8, value: i32 },
        GetFirmwareVersionRaw,
    }

    impl TmclCommand {
        pub fn opcode(&self) -> u8 {
            match self {
                TmclCommand::MoveAbsolute { .. } | TmclCommand::MoveRelative { .. } => CMD_MVP,
                TmclCommand::Stop { .. } => CMD_MST,
                TmclCommand::GetAxisParameter { .. } => CMD_GAP,
                TmclCommand::SetAxisParameter { .. } => CMD_SAP,
                TmclCommand::GetFirmwareVersionRaw => CMD_GET_FIRMWARE_VERSION,
            }
        }

        fn command_type(&self) -> u8 {
            match self {
                TmclCommand::MoveAbsolute { .. } => MVP_ABS,
                TmclCommand::MoveRelative { .. } => MVP_REL,
                TmclCommand::Stop { .. } => 0,
                TmclCommand::GetAxisParameter { parameter, .. }
                | TmclCommand::SetAxisParameter { parameter, .. } => *parameter,
                TmclCommand::GetFirmwareVersionRaw => 1,
            }
        }

        fn motor_bank(&self) -> u8 {
            match self {
                TmclCommand::MoveAbsolute { axis, .. }
                | TmclCommand::MoveRelative { axis, .. }
                | TmclCommand::Stop { axis }
                | TmclCommand::GetAxisParameter { axis, .. }
                | TmclCommand::SetAxisParameter { axis, .. } => *axis,
                TmclCommand::GetFirmwareVersionRaw => 0,
            }
        }

        fn value(&self) -> i32 {
            match self {
                TmclCommand::MoveAbsolute { position_steps, .. } => *position_steps,
                TmclCommand::MoveRelative { delta_steps, .. } => *delta_steps,
                TmclCommand::SetAxisParameter { value, .. } => *value,
                TmclCommand::Stop { .. }
                | TmclCommand::GetAxisParameter { .. }
                | TmclCommand::GetFirmwareVersionRaw => 0,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct TmclReply {
        pub reply_address: u8,
        pub module_address: u8,
        pub status: u8,
        pub command: u8,
        pub value: i32,
    }

    pub fn encode(module_address: u8, command: &TmclCommand) -> [u8; FRAME_LEN] {
        let mut frame = [0_u8; FRAME_LEN];
        frame[0] = module_address;
        frame[1] = command.opcode();
        frame[2] = command.command_type();
        frame[3] = command.motor_bank();
        frame[4..8].copy_from_slice(&command.value().to_be_bytes());
        frame[8] = checksum(&frame[..8]);
        frame
    }

    pub fn parse_reply(
        frame: &[u8],
        expected_host_address: u8,
        expected_module_address: u8,
        expected_command: u8,
    ) -> Result<TmclReply> {
        if frame.len() != FRAME_LEN {
            return Err(Error::new(
                ErrorCode::Transport,
                "TMCL reply must be exactly 9 bytes",
            ));
        }
        if checksum(&frame[..8]) != frame[8] {
            return Err(Error::new(
                ErrorCode::Transport,
                "TMCL reply checksum mismatch",
            ));
        }
        let reply = TmclReply {
            reply_address: frame[0],
            module_address: frame[1],
            status: frame[2],
            command: frame[3],
            value: i32::from_be_bytes(frame[4..8].try_into().expect("checked byte range")),
        };
        if reply.reply_address != expected_host_address {
            return Err(Error::new(
                ErrorCode::Transport,
                "TMCL reply address did not match configured host address",
            ));
        }
        if reply.module_address != expected_module_address {
            return Err(Error::new(
                ErrorCode::Transport,
                "TMCL reply module address did not match request",
            ));
        }
        if reply.command != expected_command {
            return Err(Error::new(
                ErrorCode::Transport,
                "TMCL reply command did not match request",
            ));
        }
        if reply.status != STATUS_OK && reply.status != STATUS_POSITION_REACHED_EVENT {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("TMCL controller returned status {}", reply.status),
            ));
        }
        Ok(reply)
    }

    pub fn reply_frame(host_address: u8, module_address: u8, command: u8, value: i32) -> Vec<u8> {
        let mut frame = [0_u8; FRAME_LEN];
        frame[0] = host_address;
        frame[1] = module_address;
        frame[2] = STATUS_OK;
        frame[3] = command;
        frame[4..8].copy_from_slice(&value.to_be_bytes());
        frame[8] = checksum(&frame[..8]);
        frame.to_vec()
    }

    pub fn checksum(bytes: &[u8]) -> u8 {
        bytes
            .iter()
            .copied()
            .fold(0_u8, |sum, byte| sum.wrapping_add(byte))
    }
}

#[derive(Debug, Clone)]
pub struct TmclConfiguredProbe {
    label: String,
    endpoint: Option<TmclSerialEndpoint>,
    connect_real_transport: bool,
    completion_poll_limit: usize,
    probe: protocol::TmclProbe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmclSerialEndpoint {
    pub port_name: String,
    pub baud_rate: u32,
    pub timeout_ms: u64,
}

pub struct TmclDiscovery {
    next_id: DriverId,
    probes: Vec<TmclConfiguredProbe>,
}

impl TmclDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![TmclConfiguredProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| {
                matches!(
                    device.driver.as_str(),
                    "trinamic_tmcl" | "trinamic-tmcl" | "tmcl"
                )
            })
            .map(TmclConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for TmclDiscovery {
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
                            "TMCL config requires serial_port when connect is true",
                        )
                    })?;
                    Box::new(TmclDriver::serial(id, configured, endpoint)?)
                } else {
                    Box::new(TmclDriver::configured(id, configured))
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

impl TmclConfiguredProbe {
    pub fn fixture() -> Self {
        Self {
            label: "Configured Trinamic TMCL fixture".into(),
            endpoint: None,
            connect_real_transport: false,
            completion_poll_limit: 50,
            probe: protocol::TmclProbe::configured_fixture(),
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::fixture();
        configured.label = if device.label.is_empty() {
            "Configured Trinamic TMCL".into()
        } else {
            device.label.clone()
        };
        configured.probe.model =
            string_prop(device, "model")?.unwrap_or_else(|| configured.probe.model.clone());
        configured.probe.serial_number = string_prop(device, "serial_number")?
            .unwrap_or_else(|| configured.probe.serial_number.clone());
        configured.probe.firmware_version_raw = i32_prop(device, "firmware_version_raw")?
            .unwrap_or(configured.probe.firmware_version_raw);
        configured.probe.module_address =
            u8_prop(device, "module_address")?.unwrap_or(configured.probe.module_address);
        configured.probe.host_address =
            u8_prop(device, "host_address")?.unwrap_or(configured.probe.host_address);
        let axes = usize_prop(device, "axes")?.unwrap_or(configured.probe.axes.len());
        if !(1..=u8::MAX as usize).contains(&axes) {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "TMCL axes must be in 1..=255",
            ));
        }
        let step_size_um = f64_prop(device, "step_size")?
            .or(f64_prop(device, "step_size_um")?)
            .unwrap_or(0.1);
        let travel_um = f64_prop(device, "travel")?
            .or(f64_prop(device, "travel_um")?)
            .unwrap_or(25_000.0);
        let max_positioning_speed = i32_prop(device, "max_positioning_speed")?.unwrap_or(51_200);
        let max_acceleration = i32_prop(device, "max_acceleration")?.unwrap_or(10_000);
        configured.probe.axes = (0..axes)
            .map(|index| protocol::TmclAxisProbe {
                axis_index: index as u8,
                stage_axis: stage_axis_for_index(index),
                step_size_um,
                travel_um,
                position_steps: 0,
                target_steps: 0,
                actual_speed: 0,
                max_positioning_speed,
                max_acceleration,
                position_reached: true,
                home_switch: false,
                left_limit_switch: false,
                right_limit_switch: false,
            })
            .collect();
        configured.completion_poll_limit = usize_prop(device, "completion_poll_limit")?
            .unwrap_or(configured.completion_poll_limit);
        if configured.completion_poll_limit == 0 {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "TMCL completion_poll_limit must be at least 1",
            ));
        }
        configured.endpoint = match string_prop(device, "serial_port")? {
            Some(port_name) => {
                let baud_rate = u32_prop(device, "baud_rate")?.unwrap_or(protocol::DEFAULT_BAUD);
                if baud_rate == 0 {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "TMCL baud_rate must be at least 1",
                    ));
                }
                Some(TmclSerialEndpoint {
                    port_name,
                    baud_rate,
                    timeout_ms: u64_prop(device, "serial_timeout_ms")?.unwrap_or(100),
                })
            }
            None => None,
        };
        configured.connect_real_transport = bool_prop(device, "connect")?.unwrap_or(false);
        Ok(configured)
    }
}

pub struct TmclDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    stages: Vec<DeviceId>,
    probe: protocol::TmclProbe,
    completion_poll_limit: usize,
    last_transaction: Value,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: FixedBinaryCodec,
    fixture_mode: bool,
    baud_rate: u32,
    serial_port: Option<String>,
    serial_timeout_ms: u64,
    connected: bool,
}

impl TmclDriver {
    pub fn configured(id: DriverId, configured: TmclConfiguredProbe) -> Self {
        Self::new(
            id,
            configured.probe,
            configured.completion_poll_limit,
            Box::new(ScriptedSerial::new()),
            true,
            configured
                .endpoint
                .as_ref()
                .map(|endpoint| endpoint.baud_rate)
                .unwrap_or(protocol::DEFAULT_BAUD),
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
        configured: TmclConfiguredProbe,
        endpoint: TmclSerialEndpoint,
    ) -> Result<Self> {
        let port_name = endpoint.port_name.clone();
        let baud_rate = endpoint.baud_rate;
        let timeout_ms = endpoint.timeout_ms;
        let serial = Box::new(numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(endpoint.port_name, endpoint.baud_rate)
                .timeout(Duration::from_millis(timeout_ms))
                .data_bits(serialport::DataBits::Eight)
                .parity(serialport::Parity::None)
                .stop_bits(serialport::StopBits::One)
                .flow_control(serialport::FlowControl::None),
        )?);
        let mut driver = Self::new(
            id,
            configured.probe,
            configured.completion_poll_limit,
            serial,
            false,
            baud_rate,
            Some(port_name),
            timeout_ms,
            true,
        );
        driver.refresh_startup_axes()?;
        Ok(driver)
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(
        _id: DriverId,
        _configured: TmclConfiguredProbe,
        _endpoint: TmclSerialEndpoint,
    ) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "TMCL real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(
        id: DriverId,
        probe: protocol::TmclProbe,
        completion_poll_limit: usize,
        serial: Box<dyn SerialIo>,
        fixture_mode: bool,
        baud_rate: u32,
        serial_port: Option<String>,
        serial_timeout_ms: u64,
        connected: bool,
    ) -> Self {
        let stages = (0..probe.axes.len())
            .map(|index| DeviceId(NodeId(id.0 * 1000 + 962 + index as u64)))
            .collect();
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 960)),
            hub: DeviceId(NodeId(id.0 * 1000 + 961)),
            stages,
            probe,
            completion_poll_limit,
            last_transaction: Value::Map(BTreeMap::new()),
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            codec: FixedBinaryCodec::new(protocol::FRAME_LEN),
            fixture_mode,
            baud_rate,
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

    fn axis_index_for_device(&self, device: DeviceId) -> Result<usize> {
        self.stages
            .iter()
            .position(|candidate| *candidate == device)
            .ok_or_else(|| Error::new(ErrorCode::InvalidProperty, "unknown TMCL stage device"))
    }

    fn axis_for_device(&self, device: DeviceId) -> Result<&protocol::TmclAxisProbe> {
        let index = self.axis_index_for_device(device)?;
        Ok(&self.probe.axes[index])
    }

    fn transact(&mut self, command: protocol::TmclCommand) -> Result<protocol::TmclReply> {
        let request = protocol::encode(self.probe.module_address, &command);
        if self.fixture_mode {
            let value = self.fixture_reply_value(&command)?;
            let reply = protocol::parse_reply(
                &protocol::reply_frame(
                    self.probe.host_address,
                    self.probe.module_address,
                    command.opcode(),
                    value,
                ),
                self.probe.host_address,
                self.probe.module_address,
                command.opcode(),
            )?;
            self.last_transaction = transaction_value(&request, Some(&reply), "fixture_reply");
            return Ok(reply);
        }

        self.serial.write(&request)?;
        for _ in 0..self.completion_poll_limit.max(1) {
            let bytes = self.serial.read_available()?;
            if bytes.is_empty() {
                continue;
            }
            for frame in self.codec.push(&bytes)? {
                let reply = protocol::parse_reply(
                    &frame,
                    self.probe.host_address,
                    self.probe.module_address,
                    command.opcode(),
                )?;
                self.last_transaction = transaction_value(&request, Some(&reply), "reply");
                return Ok(reply);
            }
        }
        self.last_transaction = transaction_value(&request, None, "timeout");
        Err(Error::new(
            ErrorCode::Timeout,
            "TMCL reply was not received before completion_poll_limit",
        ))
    }

    fn fixture_reply_value(&mut self, command: &protocol::TmclCommand) -> Result<i32> {
        match command {
            protocol::TmclCommand::MoveAbsolute {
                axis,
                position_steps,
            } => {
                let axis = self.axis_mut_by_wire(*axis)?;
                axis.target_steps = *position_steps;
                axis.position_steps = *position_steps;
                axis.actual_speed = 0;
                axis.position_reached = true;
                Ok(0)
            }
            protocol::TmclCommand::MoveRelative { axis, delta_steps } => {
                let axis = self.axis_mut_by_wire(*axis)?;
                axis.target_steps = axis.position_steps.saturating_add(*delta_steps);
                axis.position_steps = axis.target_steps;
                axis.actual_speed = 0;
                axis.position_reached = true;
                Ok(0)
            }
            protocol::TmclCommand::Stop { axis } => {
                let axis = self.axis_mut_by_wire(*axis)?;
                axis.target_steps = axis.position_steps;
                axis.actual_speed = 0;
                axis.position_reached = true;
                Ok(0)
            }
            protocol::TmclCommand::GetAxisParameter { axis, parameter } => {
                let axis = self.axis_by_wire(*axis)?;
                Ok(match *parameter {
                    protocol::AP_TARGET_POSITION => axis.target_steps,
                    protocol::AP_ACTUAL_POSITION => axis.position_steps,
                    protocol::AP_TARGET_SPEED => 0,
                    protocol::AP_ACTUAL_SPEED => axis.actual_speed,
                    protocol::AP_MAX_POSITIONING_SPEED => axis.max_positioning_speed,
                    protocol::AP_MAX_ACCELERATION => axis.max_acceleration,
                    protocol::AP_POSITION_REACHED => i32::from(axis.position_reached),
                    protocol::AP_HOME_SWITCH => i32::from(axis.home_switch),
                    protocol::AP_RIGHT_LIMIT_SWITCH => i32::from(axis.right_limit_switch),
                    protocol::AP_LEFT_LIMIT_SWITCH => i32::from(axis.left_limit_switch),
                    _ => {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "TMCL axis parameter is not supported in fixture mode",
                        ))
                    }
                })
            }
            protocol::TmclCommand::SetAxisParameter {
                axis,
                parameter,
                value,
            } => {
                let axis = self.axis_mut_by_wire(*axis)?;
                match *parameter {
                    protocol::AP_TARGET_POSITION => axis.target_steps = *value,
                    protocol::AP_ACTUAL_POSITION => axis.position_steps = *value,
                    protocol::AP_MAX_POSITIONING_SPEED => axis.max_positioning_speed = *value,
                    protocol::AP_MAX_ACCELERATION => axis.max_acceleration = *value,
                    _ => {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "TMCL axis parameter write is not supported",
                        ))
                    }
                }
                Ok(0)
            }
            protocol::TmclCommand::GetFirmwareVersionRaw => Ok(self.probe.firmware_version_raw),
        }
    }

    fn axis_by_wire(&self, axis: u8) -> Result<&protocol::TmclAxisProbe> {
        self.probe
            .axes
            .iter()
            .find(|candidate| candidate.axis_index == axis)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown TMCL axis index"))
    }

    fn axis_mut_by_wire(&mut self, axis: u8) -> Result<&mut protocol::TmclAxisProbe> {
        self.probe
            .axes
            .iter_mut()
            .find(|candidate| candidate.axis_index == axis)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown TMCL axis index"))
    }

    fn refresh_axis_parameter(&mut self, axis_index: usize, parameter: u8) -> Result<i32> {
        let axis = self.probe.axes[axis_index].axis_index;
        let reply = self.transact(protocol::TmclCommand::GetAxisParameter { axis, parameter })?;
        let axis_state = &mut self.probe.axes[axis_index];
        match parameter {
            protocol::AP_TARGET_POSITION => axis_state.target_steps = reply.value,
            protocol::AP_ACTUAL_POSITION => axis_state.position_steps = reply.value,
            protocol::AP_ACTUAL_SPEED => axis_state.actual_speed = reply.value,
            protocol::AP_MAX_POSITIONING_SPEED => axis_state.max_positioning_speed = reply.value,
            protocol::AP_MAX_ACCELERATION => axis_state.max_acceleration = reply.value,
            protocol::AP_POSITION_REACHED => axis_state.position_reached = reply.value != 0,
            protocol::AP_HOME_SWITCH => axis_state.home_switch = reply.value != 0,
            protocol::AP_RIGHT_LIMIT_SWITCH => axis_state.right_limit_switch = reply.value != 0,
            protocol::AP_LEFT_LIMIT_SWITCH => axis_state.left_limit_switch = reply.value != 0,
            _ => {}
        }
        Ok(reply.value)
    }

    fn refresh_motion_status(&mut self, axis_index: usize) -> Result<()> {
        self.refresh_axis_parameter(axis_index, protocol::AP_ACTUAL_POSITION)?;
        self.refresh_axis_parameter(axis_index, protocol::AP_TARGET_POSITION)?;
        self.refresh_axis_parameter(axis_index, protocol::AP_ACTUAL_SPEED)?;
        self.refresh_axis_parameter(axis_index, protocol::AP_POSITION_REACHED)?;
        Ok(())
    }

    fn refresh_firmware_version_raw(&mut self) -> Result<i32> {
        let reply = self.transact(protocol::TmclCommand::GetFirmwareVersionRaw)?;
        self.probe.firmware_version_raw = reply.value;
        self.pending
            .push_back(DriverEvent::Event(Event::PropertyChanged(
                PropertyChanged {
                    device: self.hub,
                    key: "firmware_version_raw".into(),
                    value: Value::I64(reply.value as i64),
                },
            )));
        Ok(reply.value)
    }

    #[cfg_attr(not(feature = "os-serial"), allow(dead_code))]
    fn refresh_startup_axes(&mut self) -> Result<()> {
        self.refresh_firmware_version_raw()?;
        for axis_index in 0..self.probe.axes.len() {
            self.refresh_axis_parameter(axis_index, protocol::AP_ACTUAL_POSITION)?;
            self.refresh_axis_parameter(axis_index, protocol::AP_TARGET_POSITION)?;
            self.refresh_axis_parameter(axis_index, protocol::AP_ACTUAL_SPEED)?;
            self.refresh_axis_parameter(axis_index, protocol::AP_MAX_POSITIONING_SPEED)?;
            self.refresh_axis_parameter(axis_index, protocol::AP_MAX_ACCELERATION)?;
            self.refresh_axis_parameter(axis_index, protocol::AP_POSITION_REACHED)?;
            self.refresh_axis_parameter(axis_index, protocol::AP_HOME_SWITCH)?;
            self.refresh_axis_parameter(axis_index, protocol::AP_RIGHT_LIMIT_SWITCH)?;
            self.refresh_axis_parameter(axis_index, protocol::AP_LEFT_LIMIT_SWITCH)?;
        }
        Ok(())
    }

    fn wait_axis_idle(&mut self, axis_index: usize) -> Result<()> {
        for _ in 0..self.completion_poll_limit.max(1) {
            self.refresh_motion_status(axis_index)?;
            let axis = &self.probe.axes[axis_index];
            if axis.position_reached && axis.actual_speed == 0 {
                return Ok(());
            }
        }
        Err(Error::new(
            ErrorCode::Timeout,
            "TMCL axis did not report position_reached and zero actual_speed before completion_poll_limit",
        ))
    }

    fn stage_move(&mut self, device: DeviceId, request: StageMoveRequest) -> Result<Value> {
        let axis_index = self.axis_index_for_device(device)?;
        if request.target.len() != 1 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "TMCL StageMove expects one axis target for a logical 1D stage",
            ));
        }
        let (requested_axis, position) = request.target.iter().next().expect("len checked");
        let configured_axis = self.probe.axes[axis_index].stage_axis.clone();
        if requested_axis != &configured_axis {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "TMCL StageMove axis does not match the configured stage axis",
            ));
        }
        if request.profile.is_some() {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "TMCL motion profile writes need module-specific conversion evidence before implementation",
            ));
        }
        let axis = self.probe.axes[axis_index].axis_index;
        let steps = self.probe.axes[axis_index].steps_from_position(*position)?;
        let command = if request.relative {
            protocol::TmclCommand::MoveRelative {
                axis,
                delta_steps: steps,
            }
        } else {
            protocol::TmclCommand::MoveAbsolute {
                axis,
                position_steps: steps,
            }
        };
        self.probe.axes[axis_index].position_reached = false;
        self.transact(command)?;
        self.wait_axis_idle(axis_index)?;
        self.emit_stage_property(axis_index, "position", self.axis_position_value(axis_index));
        self.emit_stage_property(axis_index, "position_reached", Value::Bool(true));
        Ok(self.axis_state_summary(axis_index))
    }

    fn stage_stop(&mut self, device: DeviceId) -> Result<Value> {
        let axis_index = self.axis_index_for_device(device)?;
        let axis = self.probe.axes[axis_index].axis_index;
        self.transact(protocol::TmclCommand::Stop { axis })?;
        self.refresh_motion_status(axis_index)?;
        self.emit_stage_property(axis_index, "busy", Value::Bool(false));
        Ok(self.axis_state_summary(axis_index))
    }

    fn refresh_generic_axis(
        &mut self,
        device: DeviceId,
        request: GenericCommandRequest,
    ) -> Result<Value> {
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
                "TMCL GenericCommand does not take parameters",
            ));
        }
        let axis_index = self.axis_index_for_device(device)?;
        let commands = match request.command.as_str() {
            "refresh_motion" => {
                self.refresh_motion_status(axis_index)?;
                4
            }
            "refresh_profile" => {
                self.refresh_axis_parameter(axis_index, protocol::AP_MAX_POSITIONING_SPEED)?;
                self.refresh_axis_parameter(axis_index, protocol::AP_MAX_ACCELERATION)?;
                2
            }
            "refresh_switches" => {
                self.refresh_axis_parameter(axis_index, protocol::AP_HOME_SWITCH)?;
                self.refresh_axis_parameter(axis_index, protocol::AP_RIGHT_LIMIT_SWITCH)?;
                self.refresh_axis_parameter(axis_index, protocol::AP_LEFT_LIMIT_SWITCH)?;
                3
            }
            "refresh_readbacks" => {
                self.refresh_startup_axes_for(axis_index)?;
                9
            }
            other => {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    format!(
                        "TMCL GenericCommand supports refresh_readbacks, refresh_motion, refresh_profile, and refresh_switches; got {other}"
                    ),
                ))
            }
        };
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("commands".into(), Value::I64(commands)),
            ("state".into(), self.axis_state_summary(axis_index)),
            (
                "completion_basis".into(),
                Value::String("TMCL mapped GAP readback".into()),
            ),
        ])))
    }

    fn refresh_startup_axes_for(&mut self, axis_index: usize) -> Result<()> {
        self.refresh_axis_parameter(axis_index, protocol::AP_ACTUAL_POSITION)?;
        self.refresh_axis_parameter(axis_index, protocol::AP_TARGET_POSITION)?;
        self.refresh_axis_parameter(axis_index, protocol::AP_ACTUAL_SPEED)?;
        self.refresh_axis_parameter(axis_index, protocol::AP_MAX_POSITIONING_SPEED)?;
        self.refresh_axis_parameter(axis_index, protocol::AP_MAX_ACCELERATION)?;
        self.refresh_axis_parameter(axis_index, protocol::AP_POSITION_REACHED)?;
        self.refresh_axis_parameter(axis_index, protocol::AP_HOME_SWITCH)?;
        self.refresh_axis_parameter(axis_index, protocol::AP_RIGHT_LIMIT_SWITCH)?;
        self.refresh_axis_parameter(axis_index, protocol::AP_LEFT_LIMIT_SWITCH)?;
        Ok(())
    }

    fn emit_stage_property(&mut self, axis_index: usize, key: &str, value: Value) {
        self.pending
            .push_back(DriverEvent::Event(Event::PropertyChanged(
                PropertyChanged {
                    device: self.stages[axis_index],
                    key: key.into(),
                    value,
                },
            )));
    }

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| self.stages.contains(&sequence.device))
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequences(plan) {
            if sequence.property != "position" && sequence.property != "target" {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "TMCL timing sequences can only target position or target",
                ));
            }
            for value in &sequence.values {
                self.validate_write(sequence.device, &sequence.property, value)?;
            }
        }
        Ok(())
    }

    fn timing_summary(&self, plan: &TimingPlan, phase: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("phase".into(), Value::String(phase.into())),
            (
                "participants".into(),
                Value::List(
                    self.stages
                        .iter()
                        .map(|device| Value::Bool(plan.participants.contains(device)))
                        .collect(),
                ),
            ),
            (
                "sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
            (
                "axes".into(),
                Value::List(
                    self.probe
                        .axes
                        .iter()
                        .enumerate()
                        .map(|(index, axis)| {
                            Value::Map(BTreeMap::from([
                                ("device".into(), Value::I64((self.stages[index].0).0 as i64)),
                                ("axis".into(), Value::String(axis.stage_axis.name().into())),
                                ("position".into(), self.axis_position_value(index)),
                                (
                                    "target".into(),
                                    Value::Position(axis.position_from_steps(axis.target_steps)),
                                ),
                            ]))
                        })
                        .collect(),
                ),
            ),
        ]))
    }

    fn apply_timing_sequence_step(&mut self, plan: &TimingPlan, first: bool) -> Result<Value> {
        let mut changed = BTreeMap::new();
        for sequence in self.local_timing_sequences(plan) {
            let value = if first {
                sequence.values.first()
            } else {
                sequence.values.last()
            };
            if let Some(value) = value {
                let applied =
                    self.write_property(sequence.device, &sequence.property, value.clone())?;
                changed.insert(
                    format!("{}:{}", (sequence.device.0).0, sequence.property),
                    applied,
                );
            }
        }
        Ok(Value::Map(changed))
    }

    fn axis_position_value(&self, axis_index: usize) -> Value {
        let axis = &self.probe.axes[axis_index];
        Value::Position(axis.position_from_steps(axis.position_steps))
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "model" => Ok(Value::String(self.probe.model.clone())),
                "serial_number" => Ok(Value::String(self.probe.serial_number.clone())),
                "firmware_version_raw" => Ok(Value::I64(self.probe.firmware_version_raw as i64)),
                "protocol" => Ok(Value::String("TMCL direct-mode binary".into())),
                "module_address" => Ok(Value::I64(self.probe.module_address as i64)),
                "host_address" => Ok(Value::I64(self.probe.host_address as i64)),
                "baud_rate" => Ok(Value::I64(self.baud_rate as i64)),
                "last_transaction" => Ok(self.last_transaction.clone()),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown TMCL hub property {key}"),
                )),
            };
        }
        let axis_index = self.axis_index_for_device(device)?;
        let axis = &self.probe.axes[axis_index];
        match key {
            "axis" => Ok(Value::String(axis.stage_axis.name().into())),
            "axis_index" => Ok(Value::I64(axis.axis_index as i64)),
            "position" => Ok(self.axis_position_value(axis_index)),
            "target" => Ok(Value::Position(axis.position_from_steps(axis.target_steps))),
            "actual_steps" => Ok(Value::StepCount(StepCount::new(axis.position_steps as i64))),
            "target_steps" => Ok(Value::StepCount(StepCount::new(axis.target_steps as i64))),
            "step_size" => Ok(Value::Position(Position::from_micrometers(
                axis.step_size_um,
            ))),
            "travel" => Ok(Value::Position(Position::from_micrometers(axis.travel_um))),
            "actual_speed" => Ok(Value::ControllerScalar(ControllerScalar::new(
                axis.actual_speed as i64,
            ))),
            "max_positioning_speed" => Ok(Value::ControllerScalar(ControllerScalar::new(
                axis.max_positioning_speed as i64,
            ))),
            "max_acceleration" => Ok(Value::ControllerScalar(ControllerScalar::new(
                axis.max_acceleration as i64,
            ))),
            "busy" => Ok(Value::Bool(
                !axis.position_reached || axis.actual_speed != 0,
            )),
            "position_reached" => Ok(Value::Bool(axis.position_reached)),
            "home_switch" => Ok(Value::Bool(axis.home_switch)),
            "left_limit_switch" => Ok(Value::Bool(axis.left_limit_switch)),
            "right_limit_switch" => Ok(Value::Bool(axis.right_limit_switch)),
            "state_summary" => Ok(self.axis_state_summary(axis_index)),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown TMCL stage property {key}"),
            )),
        }
    }

    fn axis_state_summary(&self, axis_index: usize) -> Value {
        let axis = &self.probe.axes[axis_index];
        Value::Map(BTreeMap::from([
            ("axis".into(), Value::String(axis.stage_axis.name().into())),
            ("axis_index".into(), Value::I64(axis.axis_index as i64)),
            ("position".into(), self.axis_position_value(axis_index)),
            (
                "target".into(),
                Value::Position(axis.position_from_steps(axis.target_steps)),
            ),
            (
                "actual_steps".into(),
                Value::StepCount(StepCount::new(axis.position_steps as i64)),
            ),
            (
                "target_steps".into(),
                Value::StepCount(StepCount::new(axis.target_steps as i64)),
            ),
            (
                "actual_speed".into(),
                Value::ControllerScalar(ControllerScalar::new(axis.actual_speed as i64)),
            ),
            (
                "busy".into(),
                Value::Bool(!axis.position_reached || axis.actual_speed != 0),
            ),
            (
                "position_reached".into(),
                Value::Bool(axis.position_reached),
            ),
            ("home_switch".into(), Value::Bool(axis.home_switch)),
            (
                "left_limit_switch".into(),
                Value::Bool(axis.left_limit_switch),
            ),
            (
                "right_limit_switch".into(),
                Value::Bool(axis.right_limit_switch),
            ),
        ]))
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        let axis = self.axis_for_device(device)?;
        match (key, value) {
            ("position", Value::Position(position)) | ("target", Value::Position(position)) => {
                let um = position.micrometers();
                if um < 0.0 || um > axis.travel_um {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "TMCL position is outside configured travel",
                    ));
                }
                axis.steps_from_position(*position).map(|_| ())
            }
            ("max_positioning_speed", Value::ControllerScalar(value))
                if value.value() >= 0 && value.value() <= 7_999_774 =>
            {
                Ok(())
            }
            ("max_acceleration", Value::ControllerScalar(value))
                if value.value() >= 117 && value.value() <= 7_629_278 =>
            {
                Ok(())
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("TMCL property {key} is read-only or has the wrong type"),
            )),
        }
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: Value) -> Result<Value> {
        self.validate_write(device, key, &value)?;
        let axis_index = self.axis_index_for_device(device)?;
        match (key, value) {
            ("position", Value::Position(position)) | ("target", Value::Position(position)) => {
                let request = StageMoveRequest::absolute([(
                    self.probe.axes[axis_index].stage_axis.clone(),
                    position,
                )]);
                self.stage_move(device, request)
            }
            ("max_positioning_speed", Value::ControllerScalar(value)) => {
                let axis = self.probe.axes[axis_index].axis_index;
                self.transact(protocol::TmclCommand::SetAxisParameter {
                    axis,
                    parameter: protocol::AP_MAX_POSITIONING_SPEED,
                    value: value.value() as i32,
                })?;
                self.probe.axes[axis_index].max_positioning_speed = value.value() as i32;
                Ok(self.read_property(device, "max_positioning_speed")?)
            }
            ("max_acceleration", Value::ControllerScalar(value)) => {
                let axis = self.probe.axes[axis_index].axis_index;
                self.transact(protocol::TmclCommand::SetAxisParameter {
                    axis,
                    parameter: protocol::AP_MAX_ACCELERATION,
                    value: value.value() as i32,
                })?;
                self.probe.axes[axis_index].max_acceleration = value.value() as i32;
                Ok(self.read_property(device, "max_acceleration")?)
            }
            _ => unreachable!("validated write"),
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
            .ok_or_else(|| Error::new(ErrorCode::Unsupported, "unknown TMCL capability"))?;
        match (descriptor.kind, request) {
            (CapabilityKind::StageMove, CapabilityRequest::StageMove(request)) => {
                self.stage_move(device, request)
            }
            (CapabilityKind::StageStop, CapabilityRequest::None) => self.stage_stop(device),
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
                self.refresh_generic_axis(device, request)
            }
            (CapabilityKind::StageMove, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "TMCL StageMove expects a StageMoveRequest",
            )),
            (CapabilityKind::StageStop, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "TMCL StageStop takes no request",
            )),
            (CapabilityKind::GenericCommand, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "TMCL GenericCommand expects GenericCommandRequest",
            )),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported TMCL capability",
            )),
        }
    }
}

impl Driver for TmclDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "TMCL serial/USB direct-mode transport".into(),
            kind: "serial".into(),
            metadata: BTreeMap::from([
                ("baud_rate".into(), Value::I64(self.baud_rate as i64)),
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
                ("data_bits".into(), Value::I64(8)),
                ("stop_bits".into(), Value::I64(1)),
                ("parity".into(), Value::String("none".into())),
                ("frame_len".into(), Value::I64(protocol::FRAME_LEN as i64)),
                (
                    "protocol".into(),
                    Value::String("TMCL direct-mode binary".into()),
                ),
            ]),
        }]
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        let mut descriptors = vec![DeviceDescriptor {
            id: self.hub,
            driver: self.id,
            label: "trinamic-tmcl-hub".into(),
            vendor: Some("Trinamic / Analog Devices".into()),
            model: Some(self.probe.model.clone()),
            serial: Some(self.probe.serial_number.clone()),
            kinds: vec![
                "hub".into(),
                "motion.controller".into(),
                "trinamic.tmcl".into(),
            ],
            properties: vec![
                string_property("model", "Model", false),
                string_property("serial_number", "Serial number", false),
                integer_range_property(
                    "firmware_version_raw",
                    "Firmware version raw",
                    false,
                    i32::MIN as i64,
                    i32::MAX as i64,
                ),
                string_property("protocol", "Protocol", false),
                integer_range_property("module_address", "Module address", false, 0, 255),
                integer_range_property("host_address", "Host address", false, 0, 255),
                integer_range_property("baud_rate", "Baud rate", false, 1, i64::MAX),
                map_property("last_transaction", "Last transaction", false),
            ],
            metadata: BTreeMap::from([(
                "source".into(),
                Value::String("ADI Trinamic TMCL firmware manuals".into()),
            )]),
        }];

        for (index, axis) in self.probe.axes.iter().enumerate() {
            descriptors.push(DeviceDescriptor {
                id: self.stages[index],
                driver: self.id,
                label: format!("trinamic-tmcl-{}-stage", axis.stage_axis.name()),
                vendor: Some("Trinamic / Analog Devices".into()),
                model: Some(self.probe.model.clone()),
                serial: Some(self.probe.serial_number.clone()),
                kinds: vec![
                    "stage.1d".into(),
                    "motion.stage".into(),
                    "state.device".into(),
                    "trinamic.tmcl.axis".into(),
                ],
                properties: vec![
                    string_property("axis", "Axis", false),
                    integer_range_property("axis_index", "Axis index", false, 0, 255),
                    typed_property("position", "Position", ValueType::Position, true, true),
                    typed_property("target", "Target", ValueType::Position, true, true),
                    typed_property(
                        "actual_steps",
                        "Actual steps",
                        ValueType::StepCount,
                        false,
                        false,
                    ),
                    typed_property(
                        "target_steps",
                        "Target steps",
                        ValueType::StepCount,
                        false,
                        false,
                    ),
                    typed_property("step_size", "Step size", ValueType::Position, false, false),
                    typed_property("travel", "Travel", ValueType::Position, false, false),
                    typed_property(
                        "actual_speed",
                        "Actual speed",
                        ValueType::ControllerScalar,
                        false,
                        false,
                    ),
                    typed_property(
                        "max_positioning_speed",
                        "Maximum positioning speed",
                        ValueType::ControllerScalar,
                        true,
                        true,
                    ),
                    typed_property(
                        "max_acceleration",
                        "Maximum acceleration",
                        ValueType::ControllerScalar,
                        true,
                        true,
                    ),
                    bool_property("busy", "Busy", false),
                    bool_property("position_reached", "Position reached", false),
                    bool_property("home_switch", "Home switch", false),
                    bool_property("left_limit_switch", "Left limit switch", false),
                    bool_property("right_limit_switch", "Right limit switch", false),
                    map_property("state_summary", "State summary", false),
                ],
                metadata: BTreeMap::from([
                    ("axis".into(), Value::String(axis.stage_axis.name().into())),
                    ("axis_index".into(), Value::I64(axis.axis_index as i64)),
                    (
                        "wire_position_unit".into(),
                        Value::String("microsteps".into()),
                    ),
                ]),
            });
        }
        descriptors
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if self.stages.contains(&device) {
            vec![
                capability(1, device, CapabilityKind::StageMove),
                capability(2, device, CapabilityKind::StageStop),
                capability(3, device, CapabilityKind::GenericCommand),
            ]
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
                        format!("tmcl read {key}"),
                        Value::String(key.clone()),
                    ));
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("tmcl write {key}"),
                        value.clone(),
                    ));
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(transaction(
                        self.resource,
                        "tmcl remultiplexed stage state set",
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
                            Error::new(ErrorCode::Unsupported, "unknown TMCL capability")
                        })?;
                    if !descriptor.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "TMCL capability request type does not match descriptor",
                        ));
                    }
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("tmcl invoke {}", descriptor.kind.name()),
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
                    if device == self.hub {
                        if key == "firmware_version_raw" {
                            self.refresh_firmware_version_raw()?;
                        }
                    } else if self.stages.contains(&device) {
                        let axis_index = self.axis_index_for_device(device)?;
                        match key.as_str() {
                            "position" | "actual_steps" | "busy" | "state_summary" => {
                                self.refresh_motion_status(axis_index)?;
                            }
                            "target" | "target_steps" => {
                                self.refresh_axis_parameter(
                                    axis_index,
                                    protocol::AP_TARGET_POSITION,
                                )?;
                            }
                            "actual_speed" => {
                                self.refresh_axis_parameter(axis_index, protocol::AP_ACTUAL_SPEED)?;
                            }
                            "position_reached" => {
                                self.refresh_axis_parameter(
                                    axis_index,
                                    protocol::AP_POSITION_REACHED,
                                )?;
                            }
                            "home_switch" => {
                                self.refresh_axis_parameter(axis_index, protocol::AP_HOME_SWITCH)?;
                            }
                            "left_limit_switch" => {
                                self.refresh_axis_parameter(
                                    axis_index,
                                    protocol::AP_LEFT_LIMIT_SWITCH,
                                )?;
                            }
                            "right_limit_switch" => {
                                self.refresh_axis_parameter(
                                    axis_index,
                                    protocol::AP_RIGHT_LIMIT_SWITCH,
                                )?;
                            }
                            "max_positioning_speed" => {
                                self.refresh_axis_parameter(
                                    axis_index,
                                    protocol::AP_MAX_POSITIONING_SPEED,
                                )?;
                            }
                            "max_acceleration" => {
                                self.refresh_axis_parameter(
                                    axis_index,
                                    protocol::AP_MAX_ACCELERATION,
                                )?;
                            }
                            _ => {}
                        }
                    }
                    last = self.read_property(device, &key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    last = self.write_property(device, &key, value)?;
                }
                Command::ApplyStateSet(set) => {
                    let mut map = BTreeMap::new();
                    for write in set.writes {
                        let value =
                            self.write_property(write.device, &write.property, write.value)?;
                        map.insert(write.property, value);
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

    fn prepare_timing_plan(
        &mut self,
        plan: &TimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        self.validate_timing_plan(plan)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Arm(plan.clone())],
            physical_transactions: vec![transaction(
                self.resource,
                "tmcl timing arm summary",
                self.timing_summary(plan, "arm"),
            )],
        })
    }

    fn start_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let changed = self.apply_timing_sequence_step(&armed.plan, true)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Start(OperationId(0))],
            physical_transactions: vec![transaction(
                self.resource,
                "tmcl timing start sequence",
                Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "start")),
                    ("changed".into(), changed),
                ])),
            )],
        })
    }

    fn stop_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let changed = self.apply_timing_sequence_step(&armed.plan, false)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions: vec![transaction(
                self.resource,
                "tmcl timing stop sequence",
                Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "stop")),
                    ("changed".into(), changed),
                ])),
            )],
        })
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

fn transaction_value(
    request: &[u8; protocol::FRAME_LEN],
    reply: Option<&protocol::TmclReply>,
    completion_basis: &str,
) -> Value {
    let mut map = BTreeMap::from([
        ("request_opcode".into(), Value::I64(request[1] as i64)),
        ("request_type".into(), Value::I64(request[2] as i64)),
        ("request_axis".into(), Value::I64(request[3] as i64)),
        (
            "request_value".into(),
            Value::I64(
                i32::from_be_bytes(request[4..8].try_into().expect("checked byte range")) as i64,
            ),
        ),
        (
            "completion_basis".into(),
            Value::String(completion_basis.into()),
        ),
    ]);
    if let Some(reply) = reply {
        map.insert("reply_status".into(), Value::I64(reply.status as i64));
        map.insert("reply_value".into(), Value::I64(reply.value as i64));
    }
    Value::Map(map)
}

fn property(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    writable: bool,
    sequenceable: bool,
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
        sequenceable,
        hardware_address: None,
    }
}

fn typed_property(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    writable: bool,
    sequenceable: bool,
) -> PropertySchema {
    property(key, display_name, value_type, writable, sequenceable)
}

fn string_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::String, writable, false)
}

fn bool_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::Bool, writable, false)
}

fn map_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::Map, writable, false)
}

fn integer_range_property(
    key: &str,
    display_name: &str,
    writable: bool,
    min: i64,
    max: i64,
) -> PropertySchema {
    let mut schema = property(key, display_name, ValueType::I64, writable, false);
    schema.range = Some(Range {
        min: Value::I64(min),
        max: Value::I64(max),
    });
    schema
}

fn stage_axis_for_index(index: usize) -> StageAxis {
    match index {
        0 => StageAxis::X,
        1 => StageAxis::Y,
        2 => StageAxis::Z,
        other => StageAxis::Custom(format!("axis_{other}")),
    }
}

fn string_prop(device: &DeviceConfig, key: &str) -> Result<Option<String>> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("TMCL property {key} must be String"),
        )),
        None => Ok(None),
    }
}

fn bool_prop(device: &DeviceConfig, key: &str) -> Result<Option<bool>> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("TMCL property {key} must be Bool"),
        )),
        None => Ok(None),
    }
}

fn u8_prop(device: &DeviceConfig, key: &str) -> Result<Option<u8>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => u8::try_from(*value).map(Some).map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("TMCL property {key} must fit in an unsigned byte"),
            )
        }),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("TMCL property {key} must be I64"),
        )),
        None => Ok(None),
    }
}

fn u32_prop(device: &DeviceConfig, key: &str) -> Result<Option<u32>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => u32::try_from(*value).map(Some).map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("TMCL property {key} must fit in an unsigned 32-bit integer"),
            )
        }),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("TMCL property {key} must be I64"),
        )),
        None => Ok(None),
    }
}

fn u64_prop(device: &DeviceConfig, key: &str) -> Result<Option<u64>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => u64::try_from(*value).map(Some).map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("TMCL property {key} must fit in an unsigned 64-bit integer"),
            )
        }),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("TMCL property {key} must be I64"),
        )),
        None => Ok(None),
    }
}

fn usize_prop(device: &DeviceConfig, key: &str) -> Result<Option<usize>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => usize::try_from(*value).map(Some).map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("TMCL property {key} must be a non-negative count"),
            )
        }),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("TMCL property {key} must be I64"),
        )),
        None => Ok(None),
    }
}

fn i32_prop(device: &DeviceConfig, key: &str) -> Result<Option<i32>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => i32::try_from(*value).map(Some).map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("TMCL property {key} must fit in a signed 32-bit integer"),
            )
        }),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("TMCL property {key} must be I64"),
        )),
        None => Ok(None),
    }
}

fn f64_prop(device: &DeviceConfig, key: &str) -> Result<Option<f64>> {
    match device.properties.get(key) {
        Some(Value::F64(value)) if value.is_finite() => Ok(Some(*value)),
        Some(Value::Position(value)) if value.micrometers().is_finite() => {
            Ok(Some(value.micrometers()))
        }
        Some(Value::F64(_) | Value::Position(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("TMCL property {key} must be finite"),
        )),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("TMCL property {key} must be F64 or Position"),
        )),
        None => Ok(None),
    }
}
