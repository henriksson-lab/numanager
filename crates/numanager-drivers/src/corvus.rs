use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::{LineEnding, SerialIo, SerialLineCodec};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

const MOTION_STATUS_POLLS: usize = 4;

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const DATA_BITS: u8 = 8;
    pub const STOP_BITS: u8 = 1;
    pub const PARITY: &str = "none";
    pub const TX_TERMINATOR: &str = "space";
    pub const RX_TERMINATOR: &str = "CRLF";
    pub const DEFAULT_BAUD: u32 = 115_200;

    #[derive(Debug, Clone, PartialEq)]
    pub enum CorvusCommand {
        HostMode,
        Version,
        SetUnit { axis: u8, unit: u8 },
        GetError,
        Status,
        SetDim(u8),
        SetAxis { enabled: bool, axis: u8 },
        Position,
        MoveAbsolute(Vec<Position>),
        MoveRelative(Vec<Position>),
        SetPosition(Vec<Position>),
        Calibrate,
        RangeMeasure,
        GetLimit,
        Abort,
        GetVelocity,
        SetVelocity(Velocity),
        GetAcceleration,
        SetAcceleration(Acceleration),
        Joystick(bool),
    }

    pub fn encode(command: &CorvusCommand) -> String {
        match command {
            CorvusCommand::HostMode => "0 mode".into(),
            CorvusCommand::Version => "version".into(),
            CorvusCommand::SetUnit { axis, unit } => format!("{unit} {axis} setunit"),
            CorvusCommand::GetError => "ge".into(),
            CorvusCommand::Status => "st".into(),
            CorvusCommand::SetDim(dimensions) => format!("{dimensions} setdim"),
            CorvusCommand::SetAxis { enabled, axis } => {
                format!("{} {axis} setaxis", i64::from(*enabled))
            }
            CorvusCommand::Position => "p".into(),
            CorvusCommand::MoveAbsolute(values) => format!("{} move", position_list(values)),
            CorvusCommand::MoveRelative(values) => format!("{} rmove", position_list(values)),
            CorvusCommand::SetPosition(values) => format!("{} setpos", position_list(values)),
            CorvusCommand::Calibrate => "cal".into(),
            CorvusCommand::RangeMeasure => "rm".into(),
            CorvusCommand::GetLimit => "getlimit".into(),
            CorvusCommand::Abort => "abort".into(),
            CorvusCommand::GetVelocity => "getvel".into(),
            CorvusCommand::SetVelocity(velocity) => {
                format!("{:.3} setvel", velocity.micrometers_per_second())
            }
            CorvusCommand::GetAcceleration => "getaccel".into(),
            CorvusCommand::SetAcceleration(acceleration) => {
                format!("{:.6} setaccel", acceleration.meters_per_second_squared())
            }
            CorvusCommand::Joystick(enabled) => format!("{} j", i64::from(*enabled)),
        }
    }

    pub fn decode_status(reply: &str) -> Result<i64> {
        reply.trim().parse::<i64>().map_err(|_| {
            Error::new(
                ErrorCode::Transport,
                format!("invalid Corvus status reply {reply:?}"),
            )
        })
    }

    pub fn busy_from_status(status: i64) -> bool {
        status & 1 == 1
    }

    fn position_list(values: &[Position]) -> String {
        values
            .iter()
            .map(|position| format!("{:.3}", position.micrometers()))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn parse_position_reply(reply: &str) -> Vec<Position> {
    reply
        .split_whitespace()
        .filter_map(|value| value.parse::<f64>().ok())
        .map(Position::from_micrometers)
        .collect()
}

fn parse_velocity_reply(reply: &str) -> Option<Velocity> {
    reply
        .split_whitespace()
        .next()?
        .parse::<f64>()
        .ok()
        .map(Velocity::from_micrometers_per_second)
}

fn parse_acceleration_reply(reply: &str) -> Option<Acceleration> {
    reply
        .split_whitespace()
        .next()?
        .parse::<f64>()
        .ok()
        .map(Acceleration::from_meters_per_second_squared)
}

#[derive(Debug, Clone)]
pub struct CorvusConfiguredProbe {
    label: String,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connect_real_transport: bool,
    product: String,
    serial_number: String,
    version: String,
    expose_z: bool,
    x: Position,
    y: Position,
    z: Position,
    x_travel: Position,
    y_travel: Position,
    z_travel: Position,
    speed: Velocity,
    acceleration: Acceleration,
    joystick_enabled: bool,
    last_status: Option<i64>,
    status_reply: String,
    last_error: String,
    position_reply: String,
    limit_reply: String,
    speed_reply: String,
    acceleration_reply: String,
}

pub struct CorvusDiscovery {
    next_id: DriverId,
    probes: Vec<CorvusConfiguredProbe>,
}

impl CorvusDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![CorvusConfiguredProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), "corvus" | "itk_corvus"))
            .map(CorvusConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for CorvusDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver: Box<dyn Driver> = if configured.connect_real_transport {
                    Box::new(CorvusDriver::serial(id, configured)?)
                } else {
                    Box::new(CorvusDriver::configured(id, configured))
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

impl CorvusConfiguredProbe {
    pub fn fixture() -> Self {
        Self {
            label: "Configured ITK Corvus controller".into(),
            serial_port: None,
            baud_rate: protocol::DEFAULT_BAUD,
            serial_timeout_ms: 500,
            connect_real_transport: false,
            product: "Marzhauser/ITK Corvus controller".into(),
            serial_number: "CORVUS-CONFIG-0001".into(),
            version: "configured".into(),
            expose_z: true,
            x: Position::from_micrometers(0.0),
            y: Position::from_micrometers(0.0),
            z: Position::from_micrometers(0.0),
            x_travel: Position::from_micrometers(100_000.0),
            y_travel: Position::from_micrometers(100_000.0),
            z_travel: Position::from_micrometers(25_000.0),
            speed: Velocity::from_millimeters_per_second(40.0),
            acceleration: Acceleration::from_meters_per_second_squared(0.2),
            joystick_enabled: false,
            last_status: None,
            status_reply: String::new(),
            last_error: String::new(),
            position_reply: String::new(),
            limit_reply: String::new(),
            speed_reply: String::new(),
            acceleration_reply: String::new(),
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::fixture();
        if !device.label.is_empty() {
            configured.label = device.label.clone();
        }
        configured.serial_port = string_prop(device, "serial_port")?;
        configured.baud_rate = u32_prop(device, "baud_rate")?.unwrap_or(configured.baud_rate);
        configured.serial_timeout_ms =
            u64_prop(device, "serial_timeout_ms")?.unwrap_or(configured.serial_timeout_ms);
        configured.connect_real_transport =
            bool_prop(device, "connect")?.unwrap_or(configured.connect_real_transport);
        configured.product = string_prop(device, "product")?.unwrap_or(configured.product);
        configured.serial_number =
            string_prop(device, "serial_number")?.unwrap_or(configured.serial_number);
        configured.version = string_prop(device, "version")?.unwrap_or(configured.version);
        configured.expose_z = bool_prop(device, "expose_z")?.unwrap_or(configured.expose_z);
        configured.x = position_prop(device, "x")?.unwrap_or(configured.x);
        configured.y = position_prop(device, "y")?.unwrap_or(configured.y);
        configured.z = position_prop(device, "z")?.unwrap_or(configured.z);
        configured.x_travel = position_prop(device, "x_travel")?.unwrap_or(configured.x_travel);
        configured.y_travel = position_prop(device, "y_travel")?.unwrap_or(configured.y_travel);
        configured.z_travel = position_prop(device, "z_travel")?.unwrap_or(configured.z_travel);
        configured.speed = velocity_prop(device, "speed")?.unwrap_or(configured.speed);
        configured.acceleration =
            acceleration_prop(device, "acceleration")?.unwrap_or(configured.acceleration);
        configured.joystick_enabled =
            bool_prop(device, "joystick_enabled")?.unwrap_or(configured.joystick_enabled);
        configured.status_reply =
            string_prop(device, "status_reply")?.unwrap_or(configured.status_reply);
        configured.last_error = string_prop(device, "last_error")?.unwrap_or(configured.last_error);
        configured.position_reply =
            string_prop(device, "position_reply")?.unwrap_or(configured.position_reply);
        configured.limit_reply =
            string_prop(device, "limit_reply")?.unwrap_or(configured.limit_reply);
        configured.speed_reply =
            string_prop(device, "speed_reply")?.unwrap_or(configured.speed_reply);
        configured.acceleration_reply =
            string_prop(device, "acceleration_reply")?.unwrap_or(configured.acceleration_reply);
        configured.apply_parsed_positions(parse_position_reply(&configured.position_reply));
        if let Some(speed) = parse_velocity_reply(&configured.speed_reply) {
            configured.speed = speed;
        }
        if let Some(acceleration) = parse_acceleration_reply(&configured.acceleration_reply) {
            configured.acceleration = acceleration;
        }
        validate_config(&configured)?;
        Ok(configured)
    }

    fn apply_parsed_positions(&mut self, positions: Vec<Position>) {
        if let Some(position) = positions.first() {
            self.x = clamp_position(*position, self.x_travel);
        }
        if let Some(position) = positions.get(1) {
            self.y = clamp_position(*position, self.y_travel);
        }
        if self.expose_z {
            if let Some(position) = positions.get(2) {
                self.z = clamp_position(*position, self.z_travel);
            }
        }
    }
}

pub struct CorvusDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    xy: DeviceId,
    z: Option<DeviceId>,
    configured: CorvusConfiguredProbe,
    last_transaction: Value,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Option<Box<dyn SerialIo>>,
    codec: SerialLineCodec,
}

impl CorvusDriver {
    pub fn configured(id: DriverId, configured: CorvusConfiguredProbe) -> Self {
        Self::new(id, configured, None)
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: CorvusConfiguredProbe) -> Result<Self> {
        let port_name = configured.serial_port.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Corvus config requires serial_port when connect is true",
            )
        })?;
        let serial = Box::new(numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(port_name, configured.baud_rate)
                .timeout(Duration::from_millis(configured.serial_timeout_ms)),
        )?);
        let mut driver = Self::new(id, configured, Some(serial));
        driver.record(protocol::CorvusCommand::HostMode, "host_mode")?;
        let version = driver.record(protocol::CorvusCommand::Version, "version")?;
        if !version.trim().is_empty() {
            driver.configured.version = version.trim().into();
        }
        let status = driver.record(protocol::CorvusCommand::Status, "status")?;
        if !status.trim().is_empty() {
            driver.configured.last_status = Some(protocol::decode_status(&status)?);
            driver.configured.status_reply = status.trim().into();
        }
        let error = driver.record(protocol::CorvusCommand::GetError, "get_error")?;
        if !error.trim().is_empty() {
            driver.configured.last_error = error.trim().into();
        }
        Ok(driver)
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _configured: CorvusConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Corvus real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(
        id: DriverId,
        configured: CorvusConfiguredProbe,
        serial: Option<Box<dyn SerialIo>>,
    ) -> Self {
        let base = id.0 * 1000 + 490;
        Self {
            id,
            resource: ResourceId(NodeId(base)),
            hub: DeviceId(NodeId(base + 1)),
            xy: DeviceId(NodeId(base + 2)),
            z: configured.expose_z.then_some(DeviceId(NodeId(base + 3))),
            configured,
            last_transaction: Value::Map(BTreeMap::new()),
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            codec: SerialLineCodec::new(LineEnding::Cr, LineEnding::CrLf),
        }
    }

    fn refresh_status(&mut self) -> Result<String> {
        let reply = self.record(protocol::CorvusCommand::Status, "refresh_status")?;
        self.configured.status_reply = reply.clone();
        if !reply.trim().is_empty() {
            self.configured.last_status = Some(protocol::decode_status(&reply)?);
            if let Some(status) = self.configured.last_status {
                self.emit_property(self.hub, "status", Value::I64(status));
                self.emit_property(
                    self.hub,
                    "busy",
                    Value::Bool(protocol::busy_from_status(status)),
                );
            }
        }
        self.emit_property(self.hub, "status_reply", Value::String(reply.clone()));
        Ok(reply)
    }

    fn refresh_error(&mut self) -> Result<String> {
        let reply = self.record(protocol::CorvusCommand::GetError, "refresh_error")?;
        self.configured.last_error = reply.trim().into();
        self.emit_property(
            self.hub,
            "last_error",
            Value::String(self.configured.last_error.clone()),
        );
        Ok(reply)
    }

    fn refresh_motion_readback(&mut self) -> Result<()> {
        if self.serial.is_none() {
            return Ok(());
        }
        for _ in 0..MOTION_STATUS_POLLS {
            let reply = self.refresh_status()?;
            if reply.trim().is_empty() {
                break;
            }
            let Some(status) = self.configured.last_status else {
                break;
            };
            if !protocol::busy_from_status(status) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.refresh_raw_reply(
            protocol::CorvusCommand::Position,
            "refresh_position",
            "position_reply",
        )?;
        let _ = self.refresh_error()?;
        Ok(())
    }

    fn refresh_raw_reply(
        &mut self,
        command: protocol::CorvusCommand,
        action: &str,
        property: &str,
    ) -> Result<String> {
        let reply = self.record(command, action)?;
        match property {
            "position_reply" => {
                self.configured.position_reply = reply.clone();
                self.apply_parsed_positions(parse_position_reply(&reply));
            }
            "limit_reply" => self.configured.limit_reply = reply.clone(),
            "speed_reply" => {
                self.configured.speed_reply = reply.clone();
                if let Some(speed) = parse_velocity_reply(&reply) {
                    self.configured.speed = speed;
                    self.emit_property(self.hub, "speed", Value::Velocity(speed));
                }
            }
            "acceleration_reply" => {
                self.configured.acceleration_reply = reply.clone();
                if let Some(acceleration) = parse_acceleration_reply(&reply) {
                    self.configured.acceleration = acceleration;
                    self.emit_property(self.hub, "acceleration", Value::Acceleration(acceleration));
                }
            }
            _ => {}
        }
        self.emit_property(self.hub, property, Value::String(reply.clone()));
        Ok(reply)
    }

    fn refresh_readbacks(&mut self) -> Result<Value> {
        let status = self.refresh_status()?;
        let error = self.refresh_error()?;
        let position = self.refresh_raw_reply(
            protocol::CorvusCommand::Position,
            "refresh_position",
            "position_reply",
        )?;
        let limits = self.refresh_raw_reply(
            protocol::CorvusCommand::GetLimit,
            "refresh_limits",
            "limit_reply",
        )?;
        let speed = self.refresh_raw_reply(
            protocol::CorvusCommand::GetVelocity,
            "refresh_speed",
            "speed_reply",
        )?;
        let acceleration = self.refresh_raw_reply(
            protocol::CorvusCommand::GetAcceleration,
            "refresh_acceleration",
            "acceleration_reply",
        )?;
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String("refresh_readbacks".into())),
            ("commands".into(), Value::I64(6)),
            ("connected".into(), Value::Bool(self.serial.is_some())),
            ("status_reply".into(), Value::String(status)),
            ("last_error".into(), Value::String(error)),
            ("position_reply".into(), Value::String(position)),
            ("limit_reply".into(), Value::String(limits)),
            ("speed_reply".into(), Value::String(speed)),
            ("acceleration_reply".into(), Value::String(acceleration)),
            ("x".into(), Value::Position(self.configured.x)),
            ("y".into(), Value::Position(self.configured.y)),
            ("z".into(), Value::Position(self.configured.z)),
            ("speed".into(), Value::Velocity(self.configured.speed)),
            (
                "acceleration".into(),
                Value::Acceleration(self.configured.acceleration),
            ),
            (
                "busy".into(),
                Value::Bool(
                    self.configured
                        .last_status
                        .map(protocol::busy_from_status)
                        .unwrap_or(false),
                ),
            ),
        ])))
    }

    fn invoke_generic(&mut self, request: GenericCommandRequest) -> Result<Value> {
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
                "Corvus GenericCommand refresh commands do not accept params",
            ));
        }
        let reply = match request.command.as_str() {
            "refresh_readbacks" => return self.refresh_readbacks(),
            "refresh_status" => self.refresh_status()?,
            "refresh_error" => self.refresh_error()?,
            "refresh_position" => self.refresh_raw_reply(
                protocol::CorvusCommand::Position,
                "refresh_position",
                "position_reply",
            )?,
            "refresh_limits" => self.refresh_raw_reply(
                protocol::CorvusCommand::GetLimit,
                "refresh_limits",
                "limit_reply",
            )?,
            "refresh_speed" => self.refresh_raw_reply(
                protocol::CorvusCommand::GetVelocity,
                "refresh_speed",
                "speed_reply",
            )?,
            "refresh_acceleration" => self.refresh_raw_reply(
                protocol::CorvusCommand::GetAcceleration,
                "refresh_acceleration",
                "acceleration_reply",
            )?,
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "Corvus GenericCommand supports refresh_readbacks, refresh_status, refresh_error, refresh_position, refresh_limits, refresh_speed, and refresh_acceleration",
                ))
            }
        };
        Ok(Value::String(reply))
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn owns_device(&self, device: DeviceId) -> bool {
        device == self.hub || device == self.xy || self.z == Some(device)
    }

    fn record(&mut self, command: protocol::CorvusCommand, action: &str) -> Result<String> {
        let line = protocol::encode(&command);
        let mut reply = String::new();
        let completion_basis = if self.serial.is_some() {
            let mut bytes = line.as_bytes().to_vec();
            bytes.push(b' ');
            self.active_serial()?.write(&bytes)?;
            reply = self.read_line_until_timeout()?;
            "serial write and line readback"
        } else {
            "configured command acceptance; status-poll completion requires connected readback"
        };
        self.last_transaction = Value::Map(BTreeMap::from([
            ("action".into(), Value::String(action.into())),
            (
                "completion_basis".into(),
                Value::String(completion_basis.into()),
            ),
            (
                "encoded_length".into(),
                Value::ByteCount(ByteCount::new(line.len() as u64 + 1)),
            ),
            ("live_serial".into(), Value::Bool(self.serial.is_some())),
            ("reply".into(), Value::String(reply.clone())),
        ]));
        Ok(reply)
    }

    fn active_serial(&mut self) -> Result<&mut (dyn SerialIo + 'static)> {
        self.serial.as_deref_mut().ok_or_else(|| {
            Error::new(
                ErrorCode::Unsupported,
                "Corvus active serial is not connected",
            )
        })
    }

    fn read_line_until_timeout(&mut self) -> Result<String> {
        let deadline = Instant::now() + Duration::from_millis(self.configured.serial_timeout_ms);
        loop {
            let bytes = self.active_serial()?.read_available()?;
            let lines = self.codec.push(&bytes);
            if let Some(line) = lines.into_iter().find(|line| !line.trim().is_empty()) {
                return Ok(line.trim().into());
            }
            if Instant::now() >= deadline {
                return Ok(String::new());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn apply_parsed_positions(&mut self, positions: Vec<Position>) {
        if let Some(position) = positions.first() {
            self.configured.x = clamp_position(*position, self.configured.x_travel);
            self.emit_property(self.xy, "x", Value::Position(self.configured.x));
        }
        if let Some(position) = positions.get(1) {
            self.configured.y = clamp_position(*position, self.configured.y_travel);
            self.emit_property(self.xy, "y", Value::Position(self.configured.y));
        }
        if self.z.is_some() {
            if let Some(position) = positions.get(2) {
                self.configured.z = clamp_position(*position, self.configured.z_travel);
                if let Some(device) = self.z {
                    self.emit_property(device, "z", Value::Position(self.configured.z));
                }
            }
        }
    }

    fn apply_stage_move(&mut self, device: DeviceId, request: StageMoveRequest) -> Result<Value> {
        self.validate_stage_move(device, &request)?;
        if device == self.xy {
            let mut final_x = self.configured.x;
            let mut final_y = self.configured.y;
            if let Some(target) = request.target.get(&StageAxis::X) {
                final_x = if request.relative {
                    Position::from_micrometers(
                        self.configured.x.micrometers() + target.micrometers(),
                    )
                } else {
                    *target
                };
            }
            if let Some(target) = request.target.get(&StageAxis::Y) {
                final_y = if request.relative {
                    Position::from_micrometers(
                        self.configured.y.micrometers() + target.micrometers(),
                    )
                } else {
                    *target
                };
            }
            self.move_xy(final_x, final_y, request.relative)
        } else {
            let target = request.target.get(&StageAxis::Z).ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidCommand,
                    "Corvus Z StageMove requires a Z target",
                )
            })?;
            let z = if request.relative {
                Position::from_micrometers(self.configured.z.micrometers() + target.micrometers())
            } else {
                *target
            };
            self.move_z(z, request.relative)
        }
    }

    fn move_xy(&mut self, x: Position, y: Position, relative: bool) -> Result<Value> {
        let x = clamp_position(x, self.configured.x_travel);
        let y = clamp_position(y, self.configured.y_travel);
        let command = if relative {
            protocol::CorvusCommand::MoveRelative(vec![
                Position::from_micrometers(x.micrometers() - self.configured.x.micrometers()),
                Position::from_micrometers(y.micrometers() - self.configured.y.micrometers()),
            ])
        } else {
            protocol::CorvusCommand::MoveAbsolute(vec![x, y])
        };
        self.record(command, "move_xy")?;
        self.configured.x = x;
        self.configured.y = y;
        self.emit_property(self.xy, "x", Value::Position(x));
        self.emit_property(self.xy, "y", Value::Position(y));
        self.refresh_motion_readback()?;
        Ok(self.position_map())
    }

    fn move_z(&mut self, z: Position, relative: bool) -> Result<Value> {
        let z = clamp_position(z, self.configured.z_travel);
        let command = if relative {
            protocol::CorvusCommand::MoveRelative(vec![
                Position::from_micrometers(0.0),
                Position::from_micrometers(0.0),
                Position::from_micrometers(z.micrometers() - self.configured.z.micrometers()),
            ])
        } else {
            protocol::CorvusCommand::MoveAbsolute(vec![
                Position::from_micrometers(0.0),
                Position::from_micrometers(0.0),
                z,
            ])
        };
        self.record(command, "move_z")?;
        self.configured.z = z;
        if let Some(device) = self.z {
            self.emit_property(device, "z", Value::Position(z));
        }
        self.refresh_motion_readback()?;
        Ok(Value::Position(z))
    }

    fn stage_home(&mut self, device: DeviceId) -> Result<Value> {
        self.record(protocol::CorvusCommand::Calibrate, "home")?;
        if device == self.xy {
            self.configured.x = Position::from_micrometers(0.0);
            self.configured.y = Position::from_micrometers(0.0);
            self.emit_property(self.xy, "x", Value::Position(self.configured.x));
            self.emit_property(self.xy, "y", Value::Position(self.configured.y));
            self.refresh_motion_readback()?;
            Ok(self.position_map())
        } else {
            self.configured.z = Position::from_micrometers(0.0);
            if let Some(device) = self.z {
                self.emit_property(device, "z", Value::Position(self.configured.z));
            }
            self.refresh_motion_readback()?;
            Ok(Value::Position(self.configured.z))
        }
    }

    fn stage_stop(&mut self) -> Result<Value> {
        self.record(protocol::CorvusCommand::Abort, "stop")?;
        self.refresh_motion_readback()?;
        Ok(Value::Map(BTreeMap::from([(
            "moving".into(),
            Value::Bool(false),
        )])))
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "product" => Ok(Value::String(self.configured.product.clone())),
                "serial_number" => Ok(Value::String(self.configured.serial_number.clone())),
                "serial_port" => Ok(Value::String(
                    self.configured.serial_port.clone().unwrap_or_default(),
                )),
                "version" => Ok(Value::String(self.configured.version.clone())),
                "connected" => Ok(Value::Bool(self.serial.is_some())),
                "serial_timeout" => Ok(Value::TimeInterval(TimeInterval::from_milliseconds(
                    self.configured.serial_timeout_ms as f64,
                ))),
                "protocol" => Ok(Value::String("Corvus host-mode serial command set".into())),
                "speed" => Ok(Value::Velocity(self.configured.speed)),
                "acceleration" => Ok(Value::Acceleration(self.configured.acceleration)),
                "joystick_enabled" => Ok(Value::Bool(self.configured.joystick_enabled)),
                "status" => Ok(self
                    .configured
                    .last_status
                    .map(Value::I64)
                    .unwrap_or(Value::Null)),
                "busy" => Ok(Value::Bool(
                    self.configured
                        .last_status
                        .map(protocol::busy_from_status)
                        .unwrap_or(false),
                )),
                "last_error" => Ok(Value::String(self.configured.last_error.clone())),
                "status_reply" => Ok(Value::String(self.configured.status_reply.clone())),
                "position_reply" => Ok(Value::String(self.configured.position_reply.clone())),
                "limit_reply" => Ok(Value::String(self.configured.limit_reply.clone())),
                "speed_reply" => Ok(Value::String(self.configured.speed_reply.clone())),
                "acceleration_reply" => {
                    Ok(Value::String(self.configured.acceleration_reply.clone()))
                }
                "last_transaction" => Ok(self.last_transaction.clone()),
                _ => invalid_property("unknown Corvus hub property", key),
            };
        }
        match (device, key) {
            (device, "x") if device == self.xy => Ok(Value::Position(self.configured.x)),
            (device, "y") if device == self.xy => Ok(Value::Position(self.configured.y)),
            (device, "x_travel") if device == self.xy => {
                Ok(Value::Position(self.configured.x_travel))
            }
            (device, "y_travel") if device == self.xy => {
                Ok(Value::Position(self.configured.y_travel))
            }
            (device, "z") if self.z == Some(device) => Ok(Value::Position(self.configured.z)),
            (device, "z_travel") if self.z == Some(device) => {
                Ok(Value::Position(self.configured.z_travel))
            }
            _ => invalid_property("unknown Corvus property", key),
        }
    }

    fn validate_read(&self, device: DeviceId, key: &str) -> Result<()> {
        if device == self.hub
            && matches!(
                key,
                "product"
                    | "serial_number"
                    | "serial_port"
                    | "version"
                    | "connected"
                    | "serial_timeout"
                    | "protocol"
                    | "speed"
                    | "acceleration"
                    | "joystick_enabled"
                    | "status"
                    | "busy"
                    | "last_error"
                    | "status_reply"
                    | "position_reply"
                    | "limit_reply"
                    | "speed_reply"
                    | "acceleration_reply"
                    | "last_transaction"
            )
        {
            return Ok(());
        }
        if device == self.xy && matches!(key, "x" | "y" | "x_travel" | "y_travel") {
            return Ok(());
        }
        if self.z == Some(device) && matches!(key, "z" | "z_travel") {
            return Ok(());
        }
        invalid_property("unknown Corvus property", key)
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        match (device, key, value) {
            (device, "x" | "y", Value::Position(_)) if device == self.xy => Ok(()),
            (device, "z", Value::Position(_)) if self.z == Some(device) => Ok(()),
            (device, "speed", Value::Velocity(value))
                if device == self.hub && value.micrometers_per_second() > 0.0 =>
            {
                Ok(())
            }
            (device, "acceleration", Value::Acceleration(value))
                if device == self.hub && value.meters_per_second_squared() > 0.0 =>
            {
                Ok(())
            }
            (device, "joystick_enabled", Value::Bool(_)) if device == self.hub => Ok(()),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Corvus property {key} is read-only or wrong type"),
            )),
        }
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: Value) -> Result<Value> {
        self.validate_write(device, key, &value)?;
        match (device, key, value) {
            (device, "x", Value::Position(position)) if device == self.xy => {
                self.move_xy(position, self.configured.y, false)
            }
            (device, "y", Value::Position(position)) if device == self.xy => {
                self.move_xy(self.configured.x, position, false)
            }
            (device, "z", Value::Position(position)) if self.z == Some(device) => {
                self.move_z(position, false)
            }
            (device, "speed", Value::Velocity(value)) if device == self.hub => {
                self.configured.speed = value;
                self.record(protocol::CorvusCommand::SetVelocity(value), "set_speed")?;
                Ok(Value::Velocity(value))
            }
            (device, "acceleration", Value::Acceleration(value)) if device == self.hub => {
                self.configured.acceleration = value;
                self.record(
                    protocol::CorvusCommand::SetAcceleration(value),
                    "set_acceleration",
                )?;
                Ok(Value::Acceleration(value))
            }
            (device, "joystick_enabled", Value::Bool(value)) if device == self.hub => {
                self.configured.joystick_enabled = value;
                self.record(protocol::CorvusCommand::Joystick(value), "set_joystick")?;
                Ok(Value::Bool(value))
            }
            _ => unreachable!("validated Corvus write"),
        }
    }

    fn validate_stage_move(&self, device: DeviceId, request: &StageMoveRequest) -> Result<()> {
        if device != self.xy && self.z != Some(device) {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Corvus StageMove requires the XY or Z device",
            ));
        }
        if request.target.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Corvus StageMove requires at least one target axis",
            ));
        }
        for axis in request.target.keys() {
            match (device, axis) {
                (device, StageAxis::X | StageAxis::Y) if device == self.xy => {}
                (device, StageAxis::Z) if self.z == Some(device) => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        format!(
                            "axis {} is not available on this Corvus device",
                            axis.name()
                        ),
                    ))
                }
            }
        }
        Ok(())
    }

    fn position_map(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("x".into(), Value::Position(self.configured.x)),
            ("y".into(), Value::Position(self.configured.y)),
            ("z".into(), Value::Position(self.configured.z)),
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
            .filter(|sequence| sequence.device == self.xy || self.z == Some(sequence.device))
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequences(plan) {
            match (sequence.device, sequence.property.as_str()) {
                (device, "x" | "y") if device == self.xy => {}
                (device, "z") if self.z == Some(device) => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Corvus timing sequences can only target x, y, or z",
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
                Value::Bool(
                    self.z
                        .is_some_and(|device| plan.participants.contains(&device)),
                ),
            ),
            ("x".into(), Value::Position(self.configured.x)),
            ("y".into(), Value::Position(self.configured.y)),
            ("z".into(), Value::Position(self.configured.z)),
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

        let mut target_x = self.configured.x;
        let mut target_y = self.configured.y;
        let mut target_z = self.configured.z;
        let mut xy_changed = false;
        let mut z_changed = false;
        let mut changed = BTreeMap::new();
        for (device, property, value) in writes {
            self.validate_write(device, &property, &value)?;
            match (device, property.as_str(), value) {
                (device, "x", Value::Position(position)) if device == self.xy => {
                    target_x = position;
                    xy_changed = true;
                    changed.insert("x".into(), Value::Position(position));
                }
                (device, "y", Value::Position(position)) if device == self.xy => {
                    target_y = position;
                    xy_changed = true;
                    changed.insert("y".into(), Value::Position(position));
                }
                (device, "z", Value::Position(position)) if self.z == Some(device) => {
                    target_z = position;
                    z_changed = true;
                    changed.insert("z".into(), Value::Position(position));
                }
                _ => unreachable!("validated Corvus timing write"),
            }
        }
        if xy_changed {
            self.move_xy(target_x, target_y, false)?;
        }
        if z_changed {
            self.move_z(target_z, false)?;
        }
        Ok(Value::Map(changed))
    }
}

impl Driver for CorvusDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "corvus-serial".into(),
            kind: "serial.ascii".into(),
            metadata: BTreeMap::from([
                (
                    "serial_port".into(),
                    self.configured
                        .serial_port
                        .as_ref()
                        .map(|port| Value::String(port.clone()))
                        .unwrap_or(Value::Null),
                ),
                (
                    "baud_rate".into(),
                    Value::I64(self.configured.baud_rate as i64),
                ),
                ("data_bits".into(), Value::I64(protocol::DATA_BITS as i64)),
                ("stop_bits".into(), Value::I64(protocol::STOP_BITS as i64)),
                ("parity".into(), Value::String(protocol::PARITY.into())),
                (
                    "tx_terminator".into(),
                    Value::String(protocol::TX_TERMINATOR.into()),
                ),
                (
                    "rx_terminator".into(),
                    Value::String(protocol::RX_TERMINATOR.into()),
                ),
                (
                    "completion".into(),
                    Value::String("configured acceptance or active serial line readback".into()),
                ),
                (
                    "connected".into(),
                    Value::Bool(self.configured.connect_real_transport && self.serial.is_some()),
                ),
            ]),
        }]
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        let mut descriptors = vec![
            DeviceDescriptor {
                id: self.hub,
                driver: self.id,
                label: "corvus-hub".into(),
                vendor: Some("Marzhauser/ITK".into()),
                model: Some(self.configured.product.clone()),
                serial: Some(self.configured.serial_number.clone()),
                kinds: vec![
                    "hub".into(),
                    "motion.controller".into(),
                    "serial.ascii".into(),
                ],
                properties: vec![
                    string_property("product", "Product", false),
                    string_property("serial_number", "Serial number", false),
                    string_property("serial_port", "Serial port", false),
                    string_property("version", "Version", false),
                    bool_property("connected", "Connected", false),
                    time_property("serial_timeout", "Serial timeout", false),
                    string_property("protocol", "Protocol", false),
                    velocity_property("speed", "Speed", true),
                    acceleration_property("acceleration", "Acceleration", true),
                    bool_property("joystick_enabled", "Joystick enabled", true),
                    property("status", "Status", ValueType::I64, None, false, None),
                    bool_property("busy", "Busy", false),
                    string_property("last_error", "Last error", false),
                    string_property("status_reply", "Status reply", false),
                    string_property("position_reply", "Position reply", false),
                    string_property("limit_reply", "Limit reply", false),
                    string_property("speed_reply", "Speed reply", false),
                    string_property("acceleration_reply", "Acceleration reply", false),
                    map_property("last_transaction", "Last transaction", false),
                ],
                metadata: source_metadata(),
            },
            DeviceDescriptor {
                id: self.xy,
                driver: self.id,
                label: "corvus-xy-stage".into(),
                vendor: Some("Marzhauser/ITK".into()),
                model: Some(self.configured.product.clone()),
                serial: Some(format!("{}:xy", self.configured.serial_number)),
                kinds: vec!["axis.xy".into(), "stage.xy".into(), "motion.stage".into()],
                properties: vec![
                    position_property("x", "X", true, Some(self.configured.x_travel)),
                    position_property("y", "Y", true, Some(self.configured.y_travel)),
                    position_property("x_travel", "X travel", false, None),
                    position_property("y_travel", "Y travel", false, None),
                ],
                metadata: BTreeMap::from([
                    ("axis_x".into(), Value::String("1".into())),
                    ("axis_y".into(), Value::String("2".into())),
                ]),
            },
        ];
        if let Some(device) = self.z {
            descriptors.push(DeviceDescriptor {
                id: device,
                driver: self.id,
                label: "corvus-z-stage".into(),
                vendor: Some("Marzhauser/ITK".into()),
                model: Some(self.configured.product.clone()),
                serial: Some(format!("{}:z", self.configured.serial_number)),
                kinds: vec!["axis.z".into(), "stage.z".into(), "motion.stage".into()],
                properties: vec![
                    position_property("z", "Z", true, Some(self.configured.z_travel)),
                    position_property("z_travel", "Z travel", false, None),
                ],
                metadata: BTreeMap::from([("axis_z".into(), Value::String("3".into()))]),
            });
        }
        descriptors
    }

    fn graph(&self) -> DeviceGraph {
        let mut graph = DeviceGraph::default();
        let _ = graph.insert_node(GraphNode {
            id: self.resource.0,
            kind: NodeKind::Resource,
            label: "corvus-serial".into(),
        });
        let _ = graph.insert_node(GraphNode {
            id: self.hub.0,
            kind: NodeKind::Hub,
            label: "corvus-hub".into(),
        });
        let _ = graph.insert_edge(GraphEdge {
            from: self.hub.0,
            to: self.resource.0,
            kind: EdgeKind::OwnsResource,
        });
        for descriptor in self
            .descriptors()
            .into_iter()
            .filter(|descriptor| descriptor.id != self.hub)
        {
            let _ = graph.insert_node(GraphNode {
                id: descriptor.id.0,
                kind: NodeKind::Device,
                label: descriptor.label,
            });
            let _ = graph.insert_edge(GraphEdge {
                from: self.hub.0,
                to: descriptor.id.0,
                kind: EdgeKind::OffersDevice,
            });
        }
        graph
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.hub {
            return vec![capability(4, device, CapabilityKind::GenericCommand)];
        }
        if device == self.xy || self.z == Some(device) {
            return vec![
                capability(1, device, CapabilityKind::StageMove),
                capability(2, device, CapabilityKind::StageHome),
                capability(3, device, CapabilityKind::StageStop),
            ];
        }
        Vec::new()
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut physical_transactions = Vec::new();
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    self.validate_read(*device, key)?;
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("corvus read {key}"),
                        Value::String(key.clone()),
                    ));
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("corvus write {key}"),
                        value.clone(),
                    ));
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    let Some(descriptor) = self
                        .capabilities(*device)
                        .into_iter()
                        .find(|candidate| candidate.id == *capability)
                    else {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "unknown Corvus capability",
                        ));
                    };
                    if !descriptor.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "Corvus {} request kind does not match",
                                descriptor.kind.name()
                            ),
                        ));
                    }
                    if let CapabilityRequest::StageMove(request) = request {
                        self.validate_stage_move(*device, request)?;
                    }
                    if descriptor.kind == CapabilityKind::GenericCommand {
                        let CapabilityRequest::GenericCommand(request) = request else {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Corvus GenericCommand expects GenericCommandRequest",
                            ));
                        };
                        if !matches!(
                            request.command.as_str(),
                            "refresh_readbacks"
                                | "refresh_status"
                                | "refresh_error"
                                | "refresh_position"
                                | "refresh_limits"
                                | "refresh_speed"
                                | "refresh_acceleration"
                        ) {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Corvus GenericCommand supports refresh_readbacks, refresh_status, refresh_error, refresh_position, refresh_limits, refresh_speed, and refresh_acceleration",
                            ));
                        }
                        if !request.params.is_empty() {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Corvus GenericCommand refresh commands do not accept params",
                            ));
                        }
                    }
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("corvus {}", descriptor.kind.name()),
                        Value::String(descriptor.kind.name().into()),
                    ));
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        if self.owns_device(write.device) {
                            self.validate_write(write.device, &write.property, &write.value)?;
                        }
                    }
                    physical_transactions.push(transaction(
                        self.resource,
                        "corvus state set",
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
                    let Some(descriptor) = self
                        .capabilities(device)
                        .into_iter()
                        .find(|candidate| candidate.id == capability)
                    else {
                        continue;
                    };
                    last = match (descriptor.kind, request) {
                        (CapabilityKind::StageMove, CapabilityRequest::StageMove(request)) => {
                            self.apply_stage_move(device, request)?
                        }
                        (CapabilityKind::StageHome, CapabilityRequest::None) => {
                            self.stage_home(device)?
                        }
                        (CapabilityKind::StageStop, CapabilityRequest::None) => {
                            self.stage_stop()?
                        }
                        (
                            CapabilityKind::GenericCommand,
                            CapabilityRequest::GenericCommand(request),
                        ) => self.invoke_generic(request)?,
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported Corvus capability invocation",
                            ));
                        }
                    };
                }
                Command::ApplyStateSet(set) => {
                    let mut values = BTreeMap::new();
                    for write in set.writes {
                        if self.owns_device(write.device) {
                            values.insert(
                                write.property.clone(),
                                self.write_property(write.device, &write.property, write.value)?,
                            );
                        }
                    }
                    last = Value::Map(values);
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
                "corvus timing arm summary",
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
                "corvus timing start sequence",
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
                "corvus timing stop sequence",
                Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "stop")),
                    ("changed".into(), changed),
                ])),
            )],
        })
    }
}

fn validate_config(configured: &CorvusConfiguredProbe) -> Result<()> {
    if configured.baud_rate == 0 {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "Corvus baud_rate must be positive",
        ));
    }
    if configured.speed.micrometers_per_second() <= 0.0 {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "Corvus speed must be positive",
        ));
    }
    if configured.acceleration.meters_per_second_squared() <= 0.0 {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "Corvus acceleration must be positive",
        ));
    }
    Ok(())
}

fn clamp_position(value: Position, travel: Position) -> Position {
    Position::from_micrometers(value.micrometers().clamp(0.0, travel.micrometers()))
}

fn source_metadata() -> BTreeMap<String, Value> {
    BTreeMap::from([
        (
            "evidence".into(),
            Value::String("reverse engineered serial command evidence".into()),
        ),
        (
            "support_level".into(),
            Value::String("opt-in serial stage control and readback".into()),
        ),
        (
            "hardware_validation".into(),
            Value::String("not_recorded".into()),
        ),
    ])
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

fn capability(id: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    CapabilityDescriptor::new(CapabilityId(id), device, kind, ValueType::Map)
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

fn bool_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::Bool, None, writable, None)
}

fn map_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::Map, None, writable, None)
}

fn time_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::TimeInterval,
        Some("ms"),
        writable,
        None,
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
        None,
    )
}

fn acceleration_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Acceleration,
        Some("m/s^2"),
        writable,
        None,
    )
}

fn invalid_property<T>(message: &str, key: &str) -> Result<T> {
    Err(Error::new(
        ErrorCode::InvalidProperty,
        format!("{message}: {key}"),
    ))
}

fn string_prop(device: &DeviceConfig, key: &str) -> Result<Option<String>> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Corvus property {key} must be String"),
        )),
        None => Ok(None),
    }
}

fn bool_prop(device: &DeviceConfig, key: &str) -> Result<Option<bool>> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Corvus property {key} must be Bool"),
        )),
        None => Ok(None),
    }
}

fn u32_prop(device: &DeviceConfig, key: &str) -> Result<Option<u32>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if *value > 0 && *value <= u32::MAX as i64 => {
            Ok(Some(*value as u32))
        }
        Some(Value::I64(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Corvus property {key} must fit in a positive unsigned 32-bit integer"),
        )),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Corvus property {key} must be I64"),
        )),
        None => Ok(None),
    }
}

fn u64_prop(device: &DeviceConfig, key: &str) -> Result<Option<u64>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if *value >= 0 => Ok(Some(*value as u64)),
        Some(Value::TimeInterval(value))
            if value.seconds().is_finite() && value.seconds() >= 0.0 =>
        {
            Ok(Some((value.seconds() * 1000.0).round() as u64))
        }
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Corvus property {key} must be nonnegative I64 or TimeInterval"),
        )),
        None => Ok(None),
    }
}

fn position_prop(device: &DeviceConfig, key: &str) -> Result<Option<Position>> {
    match device.properties.get(key) {
        Some(Value::Position(value)) => Ok(Some(*value)),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Corvus property {key} must be Position"),
        )),
        None => Ok(None),
    }
}

fn velocity_prop(device: &DeviceConfig, key: &str) -> Result<Option<Velocity>> {
    match device.properties.get(key) {
        Some(Value::Velocity(value)) => Ok(Some(*value)),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Corvus property {key} must be Velocity"),
        )),
        None => Ok(None),
    }
}

fn acceleration_prop(device: &DeviceConfig, key: &str) -> Result<Option<Acceleration>> {
    match device.properties.get(key) {
        Some(Value::Acceleration(value)) => Ok(Some(*value)),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Corvus property {key} must be Acceleration"),
        )),
        None => Ok(None),
    }
}
