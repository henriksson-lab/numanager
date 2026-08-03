use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::{ScriptedSerial, SerialIo};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const BAUD: u32 = 115_200;
    pub const TERMINATOR: char = '$';

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum OpenStageCommand {
        MoveAbsolute { x: f64, y: f64, z: f64 },
        MoveRelative { x: f64, y: f64, z: f64 },
        ReadPosition,
        ZeroPosition,
        SetStepSize(u8),
        ReadStepSize,
        SetVelocity { x: f64, y: f64, z: f64 },
        ReadVelocity,
        SetAcceleration { x: f64, y: f64, z: f64 },
        ReadAcceleration,
        SetSpeedMode(u8),
        Information,
        Beep,
    }

    pub fn encode(command: OpenStageCommand) -> String {
        match command {
            OpenStageCommand::MoveAbsolute { x, y, z } => {
                format!(
                    "ga{},{},{}",
                    stage_number(x),
                    stage_number(y),
                    stage_number(z)
                )
            }
            OpenStageCommand::MoveRelative { x, y, z } => {
                format!(
                    "gr{},{},{}",
                    stage_number(x),
                    stage_number(y),
                    stage_number(z)
                )
            }
            OpenStageCommand::ReadPosition => "p".into(),
            OpenStageCommand::ZeroPosition => "z".into(),
            OpenStageCommand::SetStepSize(size) => format!("ss{size}"),
            OpenStageCommand::ReadStepSize => "sr".into(),
            OpenStageCommand::SetVelocity { x, y, z } => {
                format!(
                    "vs{},{},{}",
                    motion_number(x),
                    motion_number(y),
                    motion_number(z)
                )
            }
            OpenStageCommand::ReadVelocity => "vr".into(),
            OpenStageCommand::SetAcceleration { x, y, z } => {
                format!(
                    "as{},{},{}",
                    motion_number(x),
                    motion_number(y),
                    motion_number(z)
                )
            }
            OpenStageCommand::ReadAcceleration => "ar".into(),
            OpenStageCommand::SetSpeedMode(mode) => format!("m{mode}"),
            OpenStageCommand::Information => "I".into(),
            OpenStageCommand::Beep => "b".into(),
        }
    }

    pub fn requires_command_terminator(command: OpenStageCommand) -> bool {
        matches!(
            command,
            OpenStageCommand::MoveAbsolute { .. }
                | OpenStageCommand::MoveRelative { .. }
                | OpenStageCommand::SetVelocity { .. }
                | OpenStageCommand::ReadVelocity
                | OpenStageCommand::SetAcceleration { .. }
                | OpenStageCommand::ReadAcceleration
        )
    }

    pub fn decode_position(reply: &str) -> Result<(f64, f64, f64)> {
        let values = reply
            .trim()
            .trim_end_matches(TERMINATOR)
            .split(',')
            .map(|value| {
                value.parse::<f64>().map_err(|_| {
                    Error::new(
                        ErrorCode::Transport,
                        format!("invalid OpenStage position component {value}"),
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if values.len() != 3 {
            return Err(Error::new(
                ErrorCode::Transport,
                "OpenStage position reply must contain X,Y,Z",
            ));
        }
        Ok((values[0], values[1], values[2]))
    }

    pub fn decode_step_size(reply: &str) -> Result<f64> {
        reply
            .trim()
            .trim_end_matches(TERMINATOR)
            .parse::<f64>()
            .map_err(|_| Error::new(ErrorCode::Transport, "invalid OpenStage step-size reply"))
    }

    pub fn decode_triple(reply: &str, label: &str) -> Result<(f64, f64, f64)> {
        let values = reply
            .trim()
            .trim_end_matches(TERMINATOR)
            .split(',')
            .map(|value| {
                value.parse::<f64>().map_err(|_| {
                    Error::new(
                        ErrorCode::Transport,
                        format!("invalid OpenStage {label} component {value}"),
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if values.len() != 3 {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("OpenStage {label} reply must contain X,Y,Z"),
            ));
        }
        Ok((values[0], values[1], values[2]))
    }

    pub fn completion_reply() -> Vec<u8> {
        vec![TERMINATOR as u8]
    }

    pub fn position_reply(x: f64, y: f64, z: f64) -> Vec<u8> {
        format!("{x:.3},{y:.3},{z:.3}{TERMINATOR}").into_bytes()
    }

    pub fn step_size_reply(step: f64) -> Vec<u8> {
        format!("{step:.5}{TERMINATOR}").into_bytes()
    }

    fn stage_number(micrometers: f64) -> i64 {
        (micrometers * 1000.0).round() as i64
    }

    fn motion_number(value: f64) -> i64 {
        value.round() as i64
    }
}

#[derive(Debug, Clone)]
pub struct OpenStageConfiguredProbe {
    label: String,
    serial_port: Option<String>,
    connect_real_transport: bool,
    product: String,
    serial_number: String,
    controller_info: String,
    x: Position,
    y: Position,
    z: Position,
    x_travel: Position,
    y_travel: Position,
    z_travel: Position,
    step_size: Position,
    x_velocity: Velocity,
    y_velocity: Velocity,
    z_velocity: Velocity,
    x_acceleration: Acceleration,
    y_acceleration: Acceleration,
    z_acceleration: Acceleration,
    speed_mode: i64,
}

pub struct OpenStageDiscovery {
    next_id: DriverId,
    probes: Vec<OpenStageConfiguredProbe>,
}

impl OpenStageDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![OpenStageConfiguredProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), "openstage" | "open_stage"))
            .map(OpenStageConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for OpenStageDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver: Box<dyn Driver> = if configured.connect_real_transport {
                    Box::new(OpenStageDriver::serial(id, configured)?)
                } else {
                    Box::new(OpenStageDriver::configured(id, configured))
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

impl OpenStageConfiguredProbe {
    pub fn fixture() -> Self {
        Self {
            label: "Configured OpenStage controller".into(),
            serial_port: None,
            connect_real_transport: false,
            product: "OpenStage Arduino Mega controller".into(),
            serial_number: "OPENSTAGE-CONFIG-0001".into(),
            controller_info: "configured OpenStage controller".into(),
            x: Position::from_micrometers(0.0),
            y: Position::from_micrometers(0.0),
            z: Position::from_micrometers(0.0),
            x_travel: Position::from_micrometers(50_000.0),
            y_travel: Position::from_micrometers(50_000.0),
            z_travel: Position::from_micrometers(10_000.0),
            step_size: Position::from_micrometers(1.0),
            x_velocity: Velocity::from_micrometers_per_second(1_000.0),
            y_velocity: Velocity::from_micrometers_per_second(1_000.0),
            z_velocity: Velocity::from_micrometers_per_second(1_000.0),
            x_acceleration: Acceleration::from_micrometers_per_second_squared(30_000.0),
            y_acceleration: Acceleration::from_micrometers_per_second_squared(30_000.0),
            z_acceleration: Acceleration::from_micrometers_per_second_squared(30_000.0),
            speed_mode: 2,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::fixture();
        if !device.label.is_empty() {
            configured.label = device.label.clone();
        }
        configured.product = string_prop(device, "product").unwrap_or(configured.product);
        configured.serial_number =
            string_prop(device, "serial_number").unwrap_or(configured.serial_number);
        configured.controller_info =
            string_prop(device, "controller_info").unwrap_or(configured.controller_info);
        configured.x = position_prop(device, "x").unwrap_or(configured.x);
        configured.y = position_prop(device, "y").unwrap_or(configured.y);
        configured.z = position_prop(device, "z").unwrap_or(configured.z);
        configured.x_travel = position_prop(device, "x_travel").unwrap_or(configured.x_travel);
        configured.y_travel = position_prop(device, "y_travel").unwrap_or(configured.y_travel);
        configured.z_travel = position_prop(device, "z_travel").unwrap_or(configured.z_travel);
        configured.step_size = position_prop(device, "step_size").unwrap_or(configured.step_size);
        configured.x_velocity =
            velocity_prop(device, "x_velocity").unwrap_or(configured.x_velocity);
        configured.y_velocity =
            velocity_prop(device, "y_velocity").unwrap_or(configured.y_velocity);
        configured.z_velocity =
            velocity_prop(device, "z_velocity").unwrap_or(configured.z_velocity);
        configured.x_acceleration =
            acceleration_prop(device, "x_acceleration").unwrap_or(configured.x_acceleration);
        configured.y_acceleration =
            acceleration_prop(device, "y_acceleration").unwrap_or(configured.y_acceleration);
        configured.z_acceleration =
            acceleration_prop(device, "z_acceleration").unwrap_or(configured.z_acceleration);
        configured.speed_mode = i64_prop(device, "speed_mode").unwrap_or(configured.speed_mode);
        if !(1..=4).contains(&configured.speed_mode) {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "OpenStage speed_mode must be in 1..=4",
            ));
        }
        configured.connect_real_transport = bool_prop(device, "connect").unwrap_or(false);
        configured.serial_port = string_prop(device, "serial_port");
        Ok(configured)
    }
}

pub struct OpenStageDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    xy: DeviceId,
    z: DeviceId,
    product: String,
    serial_number: String,
    controller_info: String,
    serial_port: Option<String>,
    connected: bool,
    x: Position,
    y: Position,
    z_position: Position,
    x_travel: Position,
    y_travel: Position,
    z_travel: Position,
    step_size: Position,
    x_velocity: Velocity,
    y_velocity: Velocity,
    z_velocity: Velocity,
    x_acceleration: Acceleration,
    y_acceleration: Acceleration,
    z_acceleration: Acceleration,
    speed_mode: i64,
    serial: Box<dyn SerialIo>,
    recv_buffer: Vec<u8>,
    synthesize_responses: bool,
    last_transaction: Value,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
}

impl OpenStageDriver {
    pub fn configured(id: DriverId, configured: OpenStageConfiguredProbe) -> Self {
        let reads = vec![
            protocol::position_reply(
                configured.x.micrometers(),
                configured.y.micrometers(),
                configured.z.micrometers(),
            ),
            protocol::step_size_reply(configured.step_size.micrometers()),
        ];
        let mut driver = Self::new(id, configured, Box::new(ScriptedSerial::with_reads(reads)));
        driver.synthesize_responses = true;
        driver
    }

    pub fn serial(driver_id: DriverId, configured: OpenStageConfiguredProbe) -> Result<Self> {
        let port_name = configured.serial_port.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "OpenStage real serial config requires serial_port",
            )
        })?;
        #[cfg(feature = "os-serial")]
        {
            let serial = Box::new(numanager_core::serial::OsSerialPort::open_config(
                numanager_core::serial::OsSerialConfig::new(port_name, protocol::BAUD),
            )?);
            let mut driver = Self::new(driver_id, configured, serial);
            driver.read_information()?;
            driver.read_position()?;
            driver.read_step_size()?;
            driver.read_velocity()?;
            driver.read_acceleration()?;
            Ok(driver)
        }
        #[cfg(not(feature = "os-serial"))]
        {
            let _ = driver_id;
            let _ = port_name;
            Err(Error::new(
                ErrorCode::Unsupported,
                "OpenStage real serial transport requires the os-serial feature",
            ))
        }
    }

    pub fn new(
        id: DriverId,
        configured: OpenStageConfiguredProbe,
        serial: Box<dyn SerialIo>,
    ) -> Self {
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 920)),
            hub: DeviceId(NodeId(id.0 * 1000 + 921)),
            xy: DeviceId(NodeId(id.0 * 1000 + 922)),
            z: DeviceId(NodeId(id.0 * 1000 + 923)),
            product: configured.product,
            serial_number: configured.serial_number,
            controller_info: configured.controller_info,
            serial_port: configured.serial_port,
            connected: configured.connect_real_transport,
            x: configured.x,
            y: configured.y,
            z_position: configured.z,
            x_travel: configured.x_travel,
            y_travel: configured.y_travel,
            z_travel: configured.z_travel,
            step_size: configured.step_size,
            x_velocity: configured.x_velocity,
            y_velocity: configured.y_velocity,
            z_velocity: configured.z_velocity,
            x_acceleration: configured.x_acceleration,
            y_acceleration: configured.y_acceleration,
            z_acceleration: configured.z_acceleration,
            speed_mode: configured.speed_mode,
            serial,
            recv_buffer: Vec::new(),
            synthesize_responses: false,
            last_transaction: Value::Map(BTreeMap::new()),
            next_token: 1,
            pending: VecDeque::new(),
        }
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn send(&mut self, command: protocol::OpenStageCommand) -> Result<Option<String>> {
        let mut line = protocol::encode(command);
        if protocol::requires_command_terminator(command) {
            line.push(protocol::TERMINATOR);
        }
        self.serial.write(line.as_bytes())?;
        let bytes = self.serial.read_available()?;
        if bytes.is_empty() {
            return if self.synthesize_responses {
                Ok(Some(self.synthetic_response(command)))
            } else {
                Ok(None)
            };
        }
        self.recv_buffer.extend_from_slice(&bytes);
        if let Some(index) = self
            .recv_buffer
            .iter()
            .position(|byte| *byte == protocol::TERMINATOR as u8)
        {
            let line = self.recv_buffer.drain(..index).collect::<Vec<_>>();
            self.recv_buffer.drain(..1);
            return Ok(Some(String::from_utf8_lossy(&line).trim().to_string()));
        }
        Ok(Some(String::from_utf8_lossy(&bytes).trim().to_string()))
    }

    fn synthetic_response(&self, command: protocol::OpenStageCommand) -> String {
        match command {
            protocol::OpenStageCommand::ReadPosition => format!(
                "{:.3},{:.3},{:.3}",
                self.x.micrometers(),
                self.y.micrometers(),
                self.z_position.micrometers()
            ),
            protocol::OpenStageCommand::ReadStepSize => {
                format!("{:.5}", self.step_size.micrometers())
            }
            protocol::OpenStageCommand::ReadVelocity => format!(
                "{:.0},{:.0},{:.0}",
                self.x_velocity.micrometers_per_second(),
                self.y_velocity.micrometers_per_second(),
                self.z_velocity.micrometers_per_second()
            ),
            protocol::OpenStageCommand::ReadAcceleration => format!(
                "{:.0},{:.0},{:.0}",
                self.x_acceleration.micrometers_per_second_squared(),
                self.y_acceleration.micrometers_per_second_squared(),
                self.z_acceleration.micrometers_per_second_squared()
            ),
            protocol::OpenStageCommand::Information => self.controller_info.clone(),
            _ => String::new(),
        }
    }

    fn read_information(&mut self) -> Result<String> {
        if let Some(reply) = self.send(protocol::OpenStageCommand::Information)? {
            if !reply.is_empty() {
                self.controller_info = reply;
            }
        }
        self.last_transaction = self.transaction("read_information", "information_reply");
        Ok(self.controller_info.clone())
    }

    fn read_position(&mut self) -> Result<()> {
        if let Some(reply) = self.send(protocol::OpenStageCommand::ReadPosition)? {
            if !reply.is_empty() {
                let (x, y, z) = protocol::decode_position(&reply)?;
                self.x = Position::from_micrometers(x);
                self.y = Position::from_micrometers(y);
                self.z_position = Position::from_micrometers(z);
            }
        }
        self.last_transaction = self.transaction("read_position", "position_reply");
        Ok(())
    }

    fn refresh_position_after_motion(&mut self, action: &str) -> Result<()> {
        self.read_position()?;
        self.last_transaction = self.transaction(action, "terminator_plus_position_readback");
        self.emit_property(self.xy, "x", Value::Position(self.x));
        self.emit_property(self.xy, "y", Value::Position(self.y));
        self.emit_property(self.z, "z", Value::Position(self.z_position));
        Ok(())
    }

    fn read_step_size(&mut self) -> Result<Position> {
        if let Some(reply) = self.send(protocol::OpenStageCommand::ReadStepSize)? {
            if !reply.is_empty() {
                self.step_size = Position::from_micrometers(protocol::decode_step_size(&reply)?);
            }
        }
        self.last_transaction = self.transaction("read_step_size", "step_size_reply");
        Ok(self.step_size)
    }

    fn read_velocity(&mut self) -> Result<(Velocity, Velocity, Velocity)> {
        if let Some(reply) = self.send(protocol::OpenStageCommand::ReadVelocity)? {
            if !reply.is_empty() {
                let (x, y, z) = protocol::decode_triple(&reply, "velocity")?;
                self.x_velocity = Velocity::from_micrometers_per_second(x);
                self.y_velocity = Velocity::from_micrometers_per_second(y);
                self.z_velocity = Velocity::from_micrometers_per_second(z);
            }
        }
        self.last_transaction = self.transaction("read_velocity", "velocity_reply");
        Ok((self.x_velocity, self.y_velocity, self.z_velocity))
    }

    fn read_acceleration(&mut self) -> Result<(Acceleration, Acceleration, Acceleration)> {
        if let Some(reply) = self.send(protocol::OpenStageCommand::ReadAcceleration)? {
            if !reply.is_empty() {
                let (x, y, z) = protocol::decode_triple(&reply, "acceleration")?;
                self.x_acceleration = Acceleration::from_micrometers_per_second_squared(x);
                self.y_acceleration = Acceleration::from_micrometers_per_second_squared(y);
                self.z_acceleration = Acceleration::from_micrometers_per_second_squared(z);
            }
        }
        self.last_transaction = self.transaction("read_acceleration", "acceleration_reply");
        Ok((
            self.x_acceleration,
            self.y_acceleration,
            self.z_acceleration,
        ))
    }

    fn write_velocity(&mut self) -> Result<()> {
        self.send(protocol::OpenStageCommand::SetVelocity {
            x: self.x_velocity.micrometers_per_second(),
            y: self.y_velocity.micrometers_per_second(),
            z: self.z_velocity.micrometers_per_second(),
        })?;
        self.last_transaction = self.transaction("set_velocity", "command_acceptance");
        Ok(())
    }

    fn write_acceleration(&mut self) -> Result<()> {
        self.send(protocol::OpenStageCommand::SetAcceleration {
            x: self.x_acceleration.micrometers_per_second_squared(),
            y: self.y_acceleration.micrometers_per_second_squared(),
            z: self.z_acceleration.micrometers_per_second_squared(),
        })?;
        self.last_transaction = self.transaction("set_acceleration", "command_acceptance");
        Ok(())
    }

    fn move_absolute(&mut self, x: Position, y: Position, z: Position) -> Result<Value> {
        let x = clamp_position(x, self.x_travel);
        let y = clamp_position(y, self.y_travel);
        let z = clamp_position(z, self.z_travel);
        self.send(protocol::OpenStageCommand::MoveAbsolute {
            x: x.micrometers(),
            y: y.micrometers(),
            z: z.micrometers(),
        })?;
        self.x = x;
        self.y = y;
        self.z_position = z;
        self.last_transaction = self.transaction("move_absolute", "terminator_completion");
        self.refresh_position_after_motion("move_absolute")?;
        Ok(self.position_map())
    }

    fn move_relative(&mut self, dx: f64, dy: f64, dz: f64) -> Result<Value> {
        self.send(protocol::OpenStageCommand::MoveRelative {
            x: dx,
            y: dy,
            z: dz,
        })?;
        self.x = clamp_position(
            Position::from_micrometers(self.x.micrometers() + dx),
            self.x_travel,
        );
        self.y = clamp_position(
            Position::from_micrometers(self.y.micrometers() + dy),
            self.y_travel,
        );
        self.z_position = clamp_position(
            Position::from_micrometers(self.z_position.micrometers() + dz),
            self.z_travel,
        );
        self.last_transaction = self.transaction("move_relative", "terminator_completion");
        self.refresh_position_after_motion("move_relative")?;
        Ok(self.position_map())
    }

    fn apply_stage_move(&mut self, device: DeviceId, request: StageMoveRequest) -> Result<Value> {
        self.validate_stage_move(device, &request)?;
        if let Some(profile) = request.profile.as_ref() {
            if let Some(velocity) = profile.velocity {
                if device == self.xy {
                    self.x_velocity = velocity;
                    self.y_velocity = velocity;
                } else {
                    self.z_velocity = velocity;
                }
                self.write_velocity()?;
            }
            if let Some(acceleration) = profile.acceleration {
                if device == self.xy {
                    self.x_acceleration = acceleration;
                    self.y_acceleration = acceleration;
                } else {
                    self.z_acceleration = acceleration;
                }
                self.write_acceleration()?;
            }
        }
        let mut x = self.x;
        let mut y = self.y;
        let mut z = self.z_position;
        let mut dx = 0.0;
        let mut dy = 0.0;
        let mut dz = 0.0;
        if device == self.xy {
            if let Some(target) = request.target.get(&StageAxis::X) {
                dx = target.micrometers();
                x = if request.relative { self.x } else { *target };
            }
            if let Some(target) = request.target.get(&StageAxis::Y) {
                dy = target.micrometers();
                y = if request.relative { self.y } else { *target };
            }
        } else if let Some(target) = request.target.get(&StageAxis::Z) {
            dz = target.micrometers();
            z = if request.relative {
                self.z_position
            } else {
                *target
            };
        }
        if request.relative {
            return self.move_relative(dx, dy, dz);
        }
        self.move_absolute(x, y, z)
    }

    fn validate_stage_move(&self, device: DeviceId, request: &StageMoveRequest) -> Result<()> {
        if device != self.xy && device != self.z {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "OpenStage StageMove requires the XY or Z device",
            ));
        }
        if request.target.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "OpenStage StageMove requires at least one target axis",
            ));
        }
        if let Some(profile) = request.profile.as_ref() {
            if matches!(profile.velocity, Some(value) if value.micrometers_per_second() <= 0.0) {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "OpenStage StageMove velocity profile must be positive",
                ));
            }
            if matches!(profile.acceleration, Some(value) if value.micrometers_per_second_squared() <= 0.0)
            {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "OpenStage StageMove acceleration profile must be positive",
                ));
            }
        }
        for axis in request.target.keys() {
            match (device, axis) {
                (device, StageAxis::X | StageAxis::Y) if device == self.xy => {}
                (device, StageAxis::Z) if device == self.z => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        format!(
                            "axis {} is not available on this OpenStage device",
                            axis.name()
                        ),
                    ))
                }
            }
        }
        Ok(())
    }

    fn read_property(&mut self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "product" => Ok(Value::String(self.product.clone())),
                "serial_number" => Ok(Value::String(self.serial_number.clone())),
                "controller_info" => Ok(Value::String(self.read_information()?)),
                "protocol" => Ok(Value::String("OpenStage serial protocol".into())),
                "step_size" => Ok(Value::Position(self.read_step_size()?)),
                "speed_mode" => Ok(Value::I64(self.speed_mode)),
                "last_transaction" => Ok(self.last_transaction.clone()),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown OpenStage hub property {key}"),
                )),
            };
        }
        if matches!(key, "x" | "y" | "z") {
            self.read_position()?;
        }
        if matches!(key, "x_velocity" | "y_velocity" | "z_velocity") {
            self.read_velocity()?;
        }
        if matches!(key, "x_acceleration" | "y_acceleration" | "z_acceleration") {
            self.read_acceleration()?;
        }
        match (device, key) {
            (device, "x") if device == self.xy => Ok(Value::Position(self.x)),
            (device, "y") if device == self.xy => Ok(Value::Position(self.y)),
            (device, "x_velocity") if device == self.xy => Ok(Value::Velocity(self.x_velocity)),
            (device, "y_velocity") if device == self.xy => Ok(Value::Velocity(self.y_velocity)),
            (device, "x_acceleration") if device == self.xy => {
                Ok(Value::Acceleration(self.x_acceleration))
            }
            (device, "y_acceleration") if device == self.xy => {
                Ok(Value::Acceleration(self.y_acceleration))
            }
            (device, "z") if device == self.z => Ok(Value::Position(self.z_position)),
            (device, "z_velocity") if device == self.z => Ok(Value::Velocity(self.z_velocity)),
            (device, "z_acceleration") if device == self.z => {
                Ok(Value::Acceleration(self.z_acceleration))
            }
            (device, "x_travel") if device == self.xy => Ok(Value::Position(self.x_travel)),
            (device, "y_travel") if device == self.xy => Ok(Value::Position(self.y_travel)),
            (device, "z_travel") if device == self.z => Ok(Value::Position(self.z_travel)),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown OpenStage property {key}"),
            )),
        }
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        match (device, key, value) {
            (device, "x" | "y", Value::Position(_)) if device == self.xy => Ok(()),
            (device, "z", Value::Position(_)) if device == self.z => Ok(()),
            (device, "x_velocity" | "y_velocity", Value::Velocity(velocity))
                if device == self.xy && velocity.micrometers_per_second() > 0.0 =>
            {
                Ok(())
            }
            (device, "z_velocity", Value::Velocity(velocity))
                if device == self.z && velocity.micrometers_per_second() > 0.0 =>
            {
                Ok(())
            }
            (device, "x_acceleration" | "y_acceleration", Value::Acceleration(acceleration))
                if device == self.xy && acceleration.micrometers_per_second_squared() > 0.0 =>
            {
                Ok(())
            }
            (device, "z_acceleration", Value::Acceleration(acceleration))
                if device == self.z && acceleration.micrometers_per_second_squared() > 0.0 =>
            {
                Ok(())
            }
            (device, "step_size", Value::Position(position))
                if device == self.hub && position.micrometers() > 0.0 =>
            {
                Ok(())
            }
            (device, "speed_mode", Value::I64(mode))
                if device == self.hub && (1..=4).contains(mode) =>
            {
                Ok(())
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("OpenStage property {key} is read-only or wrong type"),
            )),
        }
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: Value) -> Result<Value> {
        self.validate_write(device, key, &value)?;
        match (device, key, value) {
            (device, "x", Value::Position(position)) if device == self.xy => {
                self.move_absolute(position, self.y, self.z_position)
            }
            (device, "y", Value::Position(position)) if device == self.xy => {
                self.move_absolute(self.x, position, self.z_position)
            }
            (device, "z", Value::Position(position)) if device == self.z => {
                self.move_absolute(self.x, self.y, position)
            }
            (device, "x_velocity", Value::Velocity(velocity)) if device == self.xy => {
                self.x_velocity = velocity;
                self.write_velocity()?;
                Ok(Value::Velocity(self.x_velocity))
            }
            (device, "y_velocity", Value::Velocity(velocity)) if device == self.xy => {
                self.y_velocity = velocity;
                self.write_velocity()?;
                Ok(Value::Velocity(self.y_velocity))
            }
            (device, "z_velocity", Value::Velocity(velocity)) if device == self.z => {
                self.z_velocity = velocity;
                self.write_velocity()?;
                Ok(Value::Velocity(self.z_velocity))
            }
            (device, "x_acceleration", Value::Acceleration(acceleration)) if device == self.xy => {
                self.x_acceleration = acceleration;
                self.write_acceleration()?;
                Ok(Value::Acceleration(self.x_acceleration))
            }
            (device, "y_acceleration", Value::Acceleration(acceleration)) if device == self.xy => {
                self.y_acceleration = acceleration;
                self.write_acceleration()?;
                Ok(Value::Acceleration(self.y_acceleration))
            }
            (device, "z_acceleration", Value::Acceleration(acceleration)) if device == self.z => {
                self.z_acceleration = acceleration;
                self.write_acceleration()?;
                Ok(Value::Acceleration(self.z_acceleration))
            }
            (device, "step_size", Value::Position(position)) if device == self.hub => {
                self.send(protocol::OpenStageCommand::SetStepSize(step_size_index(
                    position,
                )))?;
                self.step_size = position;
                self.last_transaction = self.transaction("set_step_size", "command_acceptance");
                Ok(Value::Position(self.step_size))
            }
            (device, "speed_mode", Value::I64(mode)) if device == self.hub => {
                self.send(protocol::OpenStageCommand::SetSpeedMode(mode as u8))?;
                self.speed_mode = mode;
                self.last_transaction = self.transaction("set_speed_mode", "command_acceptance");
                Ok(Value::I64(self.speed_mode))
            }
            _ => unreachable!("validated write"),
        }
    }

    fn invoke_hub(&mut self, request: CapabilityRequest) -> Result<Value> {
        let CapabilityRequest::GenericCommand(request) = request else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "OpenStage hub GenericCommand expects GenericCommandRequest",
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
        if !request.params.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "OpenStage hub GenericCommand does not take parameters",
            ));
        }
        match request.command.as_str() {
            "read_information" => Ok(Value::Map(BTreeMap::from([(
                "controller_info".into(),
                Value::String(self.read_information()?),
            )]))),
            "read_velocity" => {
                self.read_velocity()?;
                Ok(self.velocity_map())
            }
            "read_acceleration" => {
                self.read_acceleration()?;
                Ok(self.acceleration_map())
            }
            "beep" => {
                self.send(protocol::OpenStageCommand::Beep)?;
                self.last_transaction = self.transaction("beep", "command_acceptance");
                Ok(Value::Map(BTreeMap::from([(
                    "command".into(),
                    Value::String("beep".into()),
                )])))
            }
            other => Err(Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "OpenStage hub GenericCommand supports read_information, read_velocity, read_acceleration, and beep, got {other}"
                ),
            )),
        }
    }

    fn position_map(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("x".into(), Value::Position(self.x)),
            ("y".into(), Value::Position(self.y)),
            ("z".into(), Value::Position(self.z_position)),
        ]))
    }

    fn velocity_map(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("x_velocity".into(), Value::Velocity(self.x_velocity)),
            ("y_velocity".into(), Value::Velocity(self.y_velocity)),
            ("z_velocity".into(), Value::Velocity(self.z_velocity)),
        ]))
    }

    fn acceleration_map(&self) -> Value {
        Value::Map(BTreeMap::from([
            (
                "x_acceleration".into(),
                Value::Acceleration(self.x_acceleration),
            ),
            (
                "y_acceleration".into(),
                Value::Acceleration(self.y_acceleration),
            ),
            (
                "z_acceleration".into(),
                Value::Acceleration(self.z_acceleration),
            ),
        ]))
    }

    fn transaction(&self, command: &str, completion_basis: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("command".into(), Value::String(command.into())),
            ("x".into(), Value::Position(self.x)),
            ("y".into(), Value::Position(self.y)),
            ("z".into(), Value::Position(self.z_position)),
            (
                "completion_basis".into(),
                Value::String(completion_basis.into()),
            ),
        ]))
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

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| sequence.device == self.xy || sequence.device == self.z)
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequences(plan) {
            match (sequence.device, sequence.property.as_str()) {
                (device, "x" | "y") if device == self.xy => {}
                (device, "z") if device == self.z => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "OpenStage timing sequences can only target x, y, or z",
                    ))
                }
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
                "xy_participant".into(),
                Value::Bool(plan.participants.contains(&self.xy)),
            ),
            (
                "z_participant".into(),
                Value::Bool(plan.participants.contains(&self.z)),
            ),
            ("x".into(), Value::Position(self.x)),
            ("y".into(), Value::Position(self.y)),
            ("z".into(), Value::Position(self.z_position)),
            (
                "sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
        ]))
    }

    fn apply_timing_sequence_step(&mut self, plan: &TimingPlan, first: bool) -> Result<Value> {
        let writes = self
            .local_timing_sequences(plan)
            .into_iter()
            .filter_map(|sequence| {
                let value = if first {
                    sequence.values.first()
                } else {
                    sequence.values.last()
                }?;
                Some((sequence.device, sequence.property.clone(), value.clone()))
            })
            .collect::<Vec<_>>();
        if writes.is_empty() {
            return Ok(Value::Map(BTreeMap::new()));
        }

        let mut target_x = self.x;
        let mut target_y = self.y;
        let mut target_z = self.z_position;
        let mut changed = BTreeMap::new();
        for (device, property, value) in writes {
            self.validate_write(device, &property, &value)?;
            match (device, property.as_str(), value) {
                (device, "x", Value::Position(position)) if device == self.xy => {
                    target_x = position;
                    changed.insert("x".into(), Value::Position(position));
                }
                (device, "y", Value::Position(position)) if device == self.xy => {
                    target_y = position;
                    changed.insert("y".into(), Value::Position(position));
                }
                (device, "z", Value::Position(position)) if device == self.z => {
                    target_z = position;
                    changed.insert("z".into(), Value::Position(position));
                }
                _ => unreachable!("validated OpenStage timing write"),
            }
        }
        self.move_absolute(target_x, target_y, target_z)?;
        Ok(Value::Map(changed))
    }
}

impl Driver for OpenStageDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "openstage-serial".into(),
            kind: "serial.ascii".into(),
            metadata: BTreeMap::from([
                ("baud_rate".into(), Value::I64(protocol::BAUD as i64)),
                (
                    "serial_port".into(),
                    self.serial_port
                        .as_ref()
                        .map(|port| Value::String(port.clone()))
                        .unwrap_or(Value::Null),
                ),
                ("connected".into(), Value::Bool(self.connected)),
                (
                    "terminator".into(),
                    Value::String(protocol::TERMINATOR.to_string()),
                ),
                (
                    "completion".into(),
                    Value::String("move/readback terminator; no busy polling".into()),
                ),
            ]),
        }]
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        vec![
            DeviceDescriptor {
                id: self.hub,
                driver: self.id,
                label: "openstage-hub".into(),
                vendor: Some("OpenStage".into()),
                model: Some(self.product.clone()),
                serial: Some(self.serial_number.clone()),
                kinds: vec![
                    "hub".into(),
                    "motion.controller".into(),
                    "serial.ascii".into(),
                ],
                properties: vec![
                    string_property("product", "Product", false),
                    string_property("serial_number", "Serial number", false),
                    string_property("controller_info", "Controller info", false),
                    string_property("protocol", "Protocol", false),
                    position_property("step_size", "Step size", true, None),
                    integer_range_property("speed_mode", "Speed mode", true, 1, 4),
                    map_property("last_transaction", "Last transaction", false),
                ],
                metadata: BTreeMap::from([
                    (
                        "source".into(),
                        Value::String("OpenStage paper serial protocol tables".into()),
                    ),
                    (
                        "support_scope".into(),
                        Value::String("XYZ move/readback command helpers".into()),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.xy,
                driver: self.id,
                label: "openstage-xy".into(),
                vendor: Some("OpenStage".into()),
                model: Some(self.product.clone()),
                serial: Some(format!("{}:xy", self.serial_number)),
                kinds: vec!["axis.xy".into(), "stage.xy".into(), "motion.stage".into()],
                properties: vec![
                    position_property("x", "X", true, Some(self.x_travel)),
                    position_property("y", "Y", true, Some(self.y_travel)),
                    velocity_property("x_velocity", "X velocity", true),
                    velocity_property("y_velocity", "Y velocity", true),
                    acceleration_property("x_acceleration", "X acceleration", true),
                    acceleration_property("y_acceleration", "Y acceleration", true),
                    position_property("x_travel", "X travel", false, None),
                    position_property("y_travel", "Y travel", false, None),
                ],
                metadata: BTreeMap::from([
                    ("geometry".into(), Value::String("Stage2D".into())),
                    ("x_travel".into(), Value::Position(self.x_travel)),
                    ("y_travel".into(), Value::Position(self.y_travel)),
                ]),
            },
            DeviceDescriptor {
                id: self.z,
                driver: self.id,
                label: "openstage-z".into(),
                vendor: Some("OpenStage".into()),
                model: Some(self.product.clone()),
                serial: Some(format!("{}:z", self.serial_number)),
                kinds: vec!["axis.z".into(), "stage.z".into(), "motion.stage".into()],
                properties: vec![
                    position_property("z", "Z", true, Some(self.z_travel)),
                    velocity_property("z_velocity", "Z velocity", true),
                    acceleration_property("z_acceleration", "Z acceleration", true),
                    position_property("z_travel", "Z travel", false, None),
                ],
                metadata: BTreeMap::from([
                    ("geometry".into(), Value::String("Stage1D".into())),
                    ("z_travel".into(), Value::Position(self.z_travel)),
                ]),
            },
        ]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.hub {
            vec![capability(2, device, CapabilityKind::GenericCommand)]
        } else if device == self.xy || device == self.z {
            vec![capability(1, device, CapabilityKind::StageMove)]
        } else {
            Vec::new()
        }
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut physical_transactions = Vec::new();
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    self.validate_read(*device, key)?;
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("openstage read {key}"),
                        Value::String(key.clone()),
                    ));
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("openstage write {key}"),
                        value.clone(),
                    ));
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    let Some(capability) = self
                        .capabilities(*device)
                        .into_iter()
                        .find(|candidate| candidate.id == *capability)
                    else {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "unknown OpenStage capability",
                        ));
                    };
                    if !capability.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "OpenStage capability received an invalid request type",
                        ));
                    }
                    if capability.kind == CapabilityKind::GenericCommand {
                        validate_hub_generic_command(request)?;
                    } else if let CapabilityRequest::StageMove(request) = request {
                        self.validate_stage_move(*device, request)?;
                    }
                    physical_transactions.push(transaction(
                        self.resource,
                        "openstage stage move",
                        Value::String(capability.kind.name().into()),
                    ));
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        if write.device == self.hub
                            || write.device == self.xy
                            || write.device == self.z
                        {
                            self.validate_write(write.device, &write.property, &write.value)?;
                        }
                    }
                    physical_transactions.push(transaction(
                        self.resource,
                        "openstage state set",
                        Value::I64(set.writes.len() as i64),
                    ));
                }
                Command::Arm(plan) => self.validate_timing_plan(plan)?,
                Command::Start(_) | Command::Stop(_) => {}
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
                    last = self.read_property(device, &key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    last = self.write_property(device, &key, value)?;
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    let Some(capability) = self
                        .capabilities(device)
                        .into_iter()
                        .find(|candidate| candidate.id == capability)
                    else {
                        continue;
                    };
                    if capability.kind == CapabilityKind::StageMove {
                        let CapabilityRequest::StageMove(request) = request else {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "OpenStage StageMove expects a StageMoveRequest",
                            ));
                        };
                        last = self.apply_stage_move(device, request)?;
                    } else if capability.kind == CapabilityKind::GenericCommand {
                        last = self.invoke_hub(request)?;
                    }
                }
                Command::ApplyStateSet(set) => {
                    let mut target_x = self.x;
                    let mut target_y = self.y;
                    let mut target_z = self.z_position;
                    let mut values = BTreeMap::new();
                    for write in set.writes {
                        match (write.device, write.property.as_str(), write.value) {
                            (device, "x", Value::Position(position)) if device == self.xy => {
                                target_x = position;
                                values.insert("x".into(), Value::Position(position));
                            }
                            (device, "y", Value::Position(position)) if device == self.xy => {
                                target_y = position;
                                values.insert("y".into(), Value::Position(position));
                            }
                            (device, "z", Value::Position(position)) if device == self.z => {
                                target_z = position;
                                values.insert("z".into(), Value::Position(position));
                            }
                            (device, property, value) if device == self.xy || device == self.z => {
                                values.insert(
                                    property.into(),
                                    self.write_property(device, property, value)?,
                                );
                            }
                            (device, property, value) if device == self.hub => {
                                values.insert(
                                    property.into(),
                                    self.write_property(device, property, value)?,
                                );
                            }
                            _ => {}
                        }
                    }
                    if values.contains_key("x")
                        || values.contains_key("y")
                        || values.contains_key("z")
                    {
                        last = self.move_absolute(target_x, target_y, target_z)?;
                    } else {
                        last = Value::Map(values);
                    }
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
                "openstage timing arm summary",
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
                "openstage timing start sequence",
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
                "openstage timing stop sequence",
                Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "stop")),
                    ("changed".into(), changed),
                ])),
            )],
        })
    }
}

impl OpenStageDriver {
    fn validate_read(&self, device: DeviceId, key: &str) -> Result<()> {
        if device == self.hub
            && matches!(
                key,
                "product"
                    | "serial_number"
                    | "controller_info"
                    | "protocol"
                    | "step_size"
                    | "speed_mode"
                    | "last_transaction"
            )
        {
            return Ok(());
        }
        if device == self.xy
            && matches!(
                key,
                "x" | "y"
                    | "x_velocity"
                    | "y_velocity"
                    | "x_acceleration"
                    | "y_acceleration"
                    | "x_travel"
                    | "y_travel"
            )
        {
            return Ok(());
        }
        if device == self.z && matches!(key, "z" | "z_velocity" | "z_acceleration" | "z_travel") {
            return Ok(());
        }
        Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unknown OpenStage property {key}"),
        ))
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

fn validate_hub_generic_command(request: &CapabilityRequest) -> Result<()> {
    let CapabilityRequest::GenericCommand(request) = request else {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            "OpenStage hub GenericCommand expects GenericCommandRequest",
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
    if !request.params.is_empty() {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            "OpenStage hub GenericCommand does not take parameters",
        ));
    }
    match request.command.as_str() {
        "read_information" | "read_velocity" | "read_acceleration" | "beep" => Ok(()),
        other => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!(
                "OpenStage hub GenericCommand supports read_information, read_velocity, read_acceleration, and beep, got {other}"
            ),
        )),
    }
}

fn capability(id: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    CapabilityDescriptor::new(CapabilityId(id), device, kind, ValueType::Map)
}

fn clamp_position(value: Position, travel: Position) -> Position {
    Position::from_micrometers(value.micrometers().clamp(0.0, travel.micrometers()))
}

fn step_size_index(step_size: Position) -> u8 {
    match step_size.micrometers() {
        value if value >= 1.0 => 1,
        value if value >= 0.5 => 2,
        value if value >= 0.25 => 3,
        value if value >= 0.125 => 4,
        _ => 5,
    }
}

fn property(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    unit: Option<&str>,
    writable: bool,
    range: Option<Range>,
) -> PropertySchema {
    PropertySchema {
        key: key.into(),
        display_name: display_name.into(),
        value_type,
        unit: unit.map(|unit| Unit(unit.into())),
        range,
        increment: None,
        enum_values: Vec::new(),
        readable: true,
        writable,
        volatile: false,
        sequenceable: matches!(key, "x" | "y" | "z"),
        hardware_address: None,
    }
}

fn string_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::String, None, writable, None)
}

fn map_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::Map, None, writable, None)
}

fn integer_range_property(
    key: &str,
    display_name: &str,
    writable: bool,
    min: i64,
    max: i64,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::I64,
        None,
        writable,
        Some(Range {
            min: Value::I64(min),
            max: Value::I64(max),
        }),
    )
}

fn position_property(
    key: &str,
    display_name: &str,
    writable: bool,
    travel: Option<Position>,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Position,
        Some("um"),
        writable,
        travel.map(|travel| Range {
            min: Value::Position(Position::from_micrometers(0.0)),
            max: Value::Position(travel),
        }),
    )
}

fn velocity_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Velocity,
        Some("um/s"),
        writable,
        Some(Range {
            min: Value::Velocity(Velocity::from_micrometers_per_second(0.0)),
            max: Value::Velocity(Velocity::from_micrometers_per_second(100_000.0)),
        }),
    )
}

fn acceleration_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Acceleration,
        Some("um/s^2"),
        writable,
        Some(Range {
            min: Value::Acceleration(Acceleration::from_micrometers_per_second_squared(0.0)),
            max: Value::Acceleration(Acceleration::from_micrometers_per_second_squared(
                1_000_000.0,
            )),
        }),
    )
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

fn i64_prop(device: &DeviceConfig, key: &str) -> Option<i64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => Some(*value),
        _ => None,
    }
}

fn position_prop(device: &DeviceConfig, key: &str) -> Option<Position> {
    match device.properties.get(key) {
        Some(Value::Position(value)) => Some(*value),
        _ => None,
    }
}

fn velocity_prop(device: &DeviceConfig, key: &str) -> Option<Velocity> {
    match device.properties.get(key) {
        Some(Value::Velocity(value)) => Some(*value),
        _ => None,
    }
}

fn acceleration_prop(device: &DeviceConfig, key: &str) -> Option<Acceleration> {
    match device.properties.get(key) {
        Some(Value::Acceleration(value)) => Some(*value),
        _ => None,
    }
}
