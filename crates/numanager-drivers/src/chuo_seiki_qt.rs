use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::{LineEnding, SerialIo, SerialLineCodec};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

const MOTION_READBACK_POLLS: usize = 4;

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const BAUD: u32 = 9_600;
    pub const DATA_BITS: u8 = 8;
    pub const STOP_BITS: u8 = 1;
    pub const PARITY: &str = "none";
    pub const LINE_ENDING: &str = "CRLF";

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Axis {
        A,
        B,
        C,
    }

    impl Axis {
        pub fn as_str(self) -> &'static str {
            match self {
                Axis::A => "A",
                Axis::B => "B",
                Axis::C => "C",
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum QtCommand {
        Identify,
        FeedbackOn,
        QueryBusy(Vec<Axis>),
        QueryPosition(Vec<Axis>),
        MoveAbsolute(Vec<(Axis, i64)>),
        MoveRelative(Vec<(Axis, i64)>),
        Home(Vec<Axis>),
        StopAll,
        SetOrigin(Vec<Axis>),
        SetHighSpeed { axis: Axis, pulses_per_second: i64 },
        SetLowSpeed { axis: Axis, pulses_per_second: i64 },
        SetAcceleration { axis: Axis, milliseconds: i64 },
    }

    pub fn encode(command: &QtCommand) -> String {
        match command {
            QtCommand::Identify => "?:CHUOSEIKI".into(),
            QtCommand::FeedbackOn => "X:1".into(),
            QtCommand::QueryBusy(axes) => {
                format!(
                    "Q:{}",
                    axes.iter()
                        .map(|axis| format!("{}2", axis.as_str()))
                        .collect::<String>()
                )
            }
            QtCommand::QueryPosition(axes) => {
                format!(
                    "Q:{}",
                    axes.iter()
                        .map(|axis| format!("{}0", axis.as_str()))
                        .collect::<String>()
                )
            }
            QtCommand::MoveAbsolute(values) => format!("A:{}", axis_values(values)),
            QtCommand::MoveRelative(values) => format!("M:{}", axis_values(values)),
            QtCommand::Home(axes) => {
                format!(
                    "H:{}",
                    axes.iter().map(|axis| axis.as_str()).collect::<String>()
                )
            }
            QtCommand::StopAll => "L:".into(),
            QtCommand::SetOrigin(axes) => {
                format!(
                    "R:{}",
                    axes.iter().map(|axis| axis.as_str()).collect::<String>()
                )
            }
            QtCommand::SetHighSpeed {
                axis,
                pulses_per_second,
            } => format!("D:{}S{}", axis.as_str(), pulses_per_second),
            QtCommand::SetLowSpeed {
                axis,
                pulses_per_second,
            } => format!("D:{}F{}", axis.as_str(), pulses_per_second),
            QtCommand::SetAcceleration { axis, milliseconds } => {
                format!("D:{}R{}", axis.as_str(), milliseconds)
            }
        }
    }

    pub fn steps(position: Position, step_size: Position) -> Result<i64> {
        let step_um = step_size.micrometers();
        if step_um <= 0.0 || !step_um.is_finite() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Chuo QT step_size must be positive",
            ));
        }
        Ok((position.micrometers() / step_um).round() as i64)
    }

    fn axis_values(values: &[(Axis, i64)]) -> String {
        values
            .iter()
            .map(|(axis, value)| format!("{}{}", axis.as_str(), value))
            .collect::<Vec<_>>()
            .join("")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChuoAxisState {
    Moving,
    Homing,
    Other(char),
}

impl ChuoAxisState {
    fn is_motion_state(self) -> bool {
        matches!(self, Self::Moving | Self::Homing)
    }
}

#[derive(Debug, Clone, Copy)]
struct ChuoAxisPosition {
    axis: protocol::Axis,
    position: Position,
    state: ChuoAxisState,
}

fn parse_position_reply(
    reply: &str,
    axes: &[protocol::Axis],
    step_size: Position,
) -> Vec<ChuoAxisPosition> {
    reply
        .trim()
        .split(',')
        .zip(axes.iter().copied())
        .filter_map(|(segment, axis)| parse_position_segment(segment.trim(), axis, step_size))
        .collect()
}

fn parse_position_segment(
    segment: &str,
    axis: protocol::Axis,
    step_size: Position,
) -> Option<ChuoAxisPosition> {
    if segment.len() < 10 {
        return None;
    }
    let steps = segment.get(0..9)?.parse::<i64>().ok()?;
    let state = match segment.as_bytes().get(9).copied()? as char {
        'D' => ChuoAxisState::Moving,
        'H' => ChuoAxisState::Homing,
        other => ChuoAxisState::Other(other),
    };
    Some(ChuoAxisPosition {
        axis,
        position: Position::from_micrometers(steps as f64 * step_size.micrometers()),
        state,
    })
}

#[derive(Debug, Clone)]
pub struct ChuoQtConfiguredProbe {
    label: String,
    serial_port: Option<String>,
    serial_timeout_ms: u64,
    connect_real_transport: bool,
    product: String,
    serial_number: String,
    expose_z: bool,
    z_axis: protocol::Axis,
    x: Position,
    y: Position,
    z: Position,
    x_travel: Position,
    y_travel: Position,
    z_travel: Position,
    step_size: Position,
    high_speed: i64,
    low_speed: i64,
    acceleration_time: TimeInterval,
    busy_reply: String,
    position_reply: String,
}

pub struct ChuoQtDiscovery {
    next_id: DriverId,
    probes: Vec<ChuoQtConfiguredProbe>,
}

impl ChuoQtDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![ChuoQtConfiguredProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| {
                matches!(
                    device.driver.as_str(),
                    "chuo_seiki_qt" | "chuo-seiki-qt" | "chuo_qt"
                )
            })
            .map(ChuoQtConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for ChuoQtDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver: Box<dyn Driver> = if configured.connect_real_transport {
                    Box::new(ChuoQtDriver::serial(id, configured)?)
                } else {
                    Box::new(ChuoQtDriver::configured(id, configured))
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

impl ChuoQtConfiguredProbe {
    pub fn fixture() -> Self {
        Self {
            label: "Configured Chuo Seiki QT controller".into(),
            serial_port: None,
            serial_timeout_ms: 500,
            connect_real_transport: false,
            product: "Chuo Seiki QT-series controller".into(),
            serial_number: "CHUO-QT-CONFIG-0001".into(),
            expose_z: true,
            z_axis: protocol::Axis::C,
            x: Position::from_micrometers(0.0),
            y: Position::from_micrometers(0.0),
            z: Position::from_micrometers(0.0),
            x_travel: Position::from_micrometers(100_000.0),
            y_travel: Position::from_micrometers(100_000.0),
            z_travel: Position::from_micrometers(25_000.0),
            step_size: Position::from_micrometers(1.0),
            high_speed: 2_000,
            low_speed: 500,
            acceleration_time: TimeInterval::from_milliseconds(100.0),
            busy_reply: String::new(),
            position_reply: String::new(),
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::fixture();
        if !device.label.is_empty() {
            configured.label = device.label.clone();
        }
        configured.serial_port = string_prop(device, "serial_port");
        configured.serial_timeout_ms =
            u64_prop(device, "serial_timeout_ms").unwrap_or(configured.serial_timeout_ms);
        configured.connect_real_transport =
            bool_prop(device, "connect").unwrap_or(configured.connect_real_transport);
        configured.product = string_prop(device, "product").unwrap_or(configured.product);
        configured.serial_number =
            string_prop(device, "serial_number").unwrap_or(configured.serial_number);
        configured.expose_z = bool_prop(device, "expose_z").unwrap_or(configured.expose_z);
        configured.z_axis = axis_prop(device, "z_axis").unwrap_or(configured.z_axis);
        configured.x = position_prop(device, "x").unwrap_or(configured.x);
        configured.y = position_prop(device, "y").unwrap_or(configured.y);
        configured.z = position_prop(device, "z").unwrap_or(configured.z);
        configured.x_travel = position_prop(device, "x_travel").unwrap_or(configured.x_travel);
        configured.y_travel = position_prop(device, "y_travel").unwrap_or(configured.y_travel);
        configured.z_travel = position_prop(device, "z_travel").unwrap_or(configured.z_travel);
        configured.step_size = position_prop(device, "step_size").unwrap_or(configured.step_size);
        configured.high_speed = i64_prop(device, "high_speed").unwrap_or(configured.high_speed);
        configured.low_speed = i64_prop(device, "low_speed").unwrap_or(configured.low_speed);
        configured.acceleration_time =
            time_prop(device, "acceleration_time").unwrap_or(configured.acceleration_time);
        configured.busy_reply = string_prop(device, "busy_reply").unwrap_or(configured.busy_reply);
        configured.position_reply =
            string_prop(device, "position_reply").unwrap_or(configured.position_reply);
        configured.apply_parsed_positions(parse_position_reply(
            &configured.position_reply,
            &configured.configured_axes(),
            configured.step_size,
        ));
        validate_motion_settings(
            configured.step_size,
            configured.high_speed,
            configured.low_speed,
            configured.acceleration_time,
        )?;
        Ok(configured)
    }

    fn configured_axes(&self) -> Vec<protocol::Axis> {
        let mut axes = vec![protocol::Axis::A, protocol::Axis::B];
        if self.expose_z && !axes.contains(&self.z_axis) {
            axes.push(self.z_axis);
        }
        axes
    }

    fn apply_parsed_positions(&mut self, positions: Vec<ChuoAxisPosition>) {
        for readback in positions {
            if readback.state == ChuoAxisState::Homing {
                continue;
            }
            match readback.axis {
                protocol::Axis::A => self.x = clamp_position(readback.position, self.x_travel),
                protocol::Axis::B => self.y = clamp_position(readback.position, self.y_travel),
                axis if self.expose_z && axis == self.z_axis => {
                    self.z = clamp_position(readback.position, self.z_travel)
                }
                _ => {}
            }
        }
    }
}

pub struct ChuoQtDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    xy: DeviceId,
    z: Option<DeviceId>,
    configured: ChuoQtConfiguredProbe,
    last_transaction: Value,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Option<Box<dyn SerialIo>>,
    codec: SerialLineCodec,
}

impl ChuoQtDriver {
    pub fn configured(id: DriverId, configured: ChuoQtConfiguredProbe) -> Self {
        Self::new(id, configured, None)
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: ChuoQtConfiguredProbe) -> Result<Self> {
        let port_name = configured.serial_port.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Chuo QT config requires serial_port when connect is true",
            )
        })?;
        let serial = Box::new(numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(port_name, protocol::BAUD)
                .timeout(Duration::from_millis(configured.serial_timeout_ms)),
        )?);
        let mut driver = Self::new(id, configured, Some(serial));
        let identity = driver.send(protocol::QtCommand::Identify, "identify")?;
        if !identity.trim().is_empty() {
            driver.configured.product = identity.trim().into();
        }
        driver.send(protocol::QtCommand::FeedbackOn, "feedback_on")?;
        Ok(driver)
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _configured: ChuoQtConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Chuo QT real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(
        id: DriverId,
        configured: ChuoQtConfiguredProbe,
        serial: Option<Box<dyn SerialIo>>,
    ) -> Self {
        let base = id.0 * 1000 + 960;
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
            codec: SerialLineCodec::new(LineEnding::CrLf, LineEnding::CrLf),
        }
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn owns_device(&self, device: DeviceId) -> bool {
        device == self.hub || device == self.xy || self.z == Some(device)
    }

    fn send(&mut self, command: protocol::QtCommand, action: &str) -> Result<String> {
        let line = protocol::encode(&command);
        let mut reply = String::new();
        let completion_basis = if self.serial.is_some() {
            let bytes = self.codec.encode(&line);
            self.active_serial()?.write(&bytes)?;
            reply = self.read_line_until_timeout()?;
            "serial write and line readback"
        } else {
            "configured command acceptance; live line readback disabled"
        };
        self.last_transaction = Value::Map(BTreeMap::from([
            ("action".into(), Value::String(action.into())),
            (
                "completion_basis".into(),
                Value::String(completion_basis.into()),
            ),
            (
                "encoded_length".into(),
                Value::ByteCount(ByteCount::new(line.len() as u64 + 2)),
            ),
            ("live_serial".into(), Value::Bool(self.serial.is_some())),
            ("reply".into(), Value::String(reply.clone())),
        ]));
        Ok(reply)
    }

    fn cache_action(&mut self, action: &str) {
        self.last_transaction = Value::Map(BTreeMap::from([
            ("action".into(), Value::String(action.into())),
            (
                "completion_basis".into(),
                Value::String("software configuration cache".into()),
            ),
            ("encoded_length".into(), Value::ByteCount(ByteCount::new(0))),
        ]));
    }

    fn active_serial(&mut self) -> Result<&mut (dyn SerialIo + 'static)> {
        self.serial.as_deref_mut().ok_or_else(|| {
            Error::new(
                ErrorCode::Unsupported,
                "Chuo QT active serial is not connected",
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

    fn send_settings(&mut self, action: &str) -> Result<()> {
        if self.serial.is_none() {
            self.cache_action(action);
            return Ok(());
        }
        let acceleration_ms = (self.configured.acceleration_time.seconds() * 1000.0).round() as i64;
        for axis in self.configured_axes() {
            self.send(
                protocol::QtCommand::SetHighSpeed {
                    axis,
                    pulses_per_second: self.configured.high_speed,
                },
                action,
            )?;
            self.send(
                protocol::QtCommand::SetLowSpeed {
                    axis,
                    pulses_per_second: self.configured.low_speed,
                },
                action,
            )?;
            self.send(
                protocol::QtCommand::SetAcceleration {
                    axis,
                    milliseconds: acceleration_ms,
                },
                action,
            )?;
        }
        Ok(())
    }

    fn refresh_busy(&mut self) -> Result<String> {
        let reply = self.send(
            protocol::QtCommand::QueryBusy(self.configured_axes()),
            "refresh_busy",
        )?;
        self.configured.busy_reply = reply.clone();
        self.emit_property(self.hub, "busy_reply", Value::String(reply.clone()));
        Ok(reply)
    }

    fn refresh_position(&mut self) -> Result<String> {
        let axes = self.configured_axes();
        let (reply, _) = self.refresh_position_for_axes(axes)?;
        Ok(reply)
    }

    fn refresh_readbacks(&mut self) -> Result<Value> {
        let busy = self.refresh_busy()?;
        let position = self.refresh_position()?;
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String("refresh_readbacks".into())),
            ("commands".into(), Value::I64(2)),
            ("connected".into(), Value::Bool(self.serial.is_some())),
            ("busy_reply".into(), Value::String(busy)),
            ("position_reply".into(), Value::String(position)),
            ("x".into(), Value::Position(self.configured.x)),
            ("y".into(), Value::Position(self.configured.y)),
            ("z".into(), Value::Position(self.configured.z)),
        ])))
    }

    fn refresh_position_for_axes(
        &mut self,
        axes: Vec<protocol::Axis>,
    ) -> Result<(String, Vec<ChuoAxisPosition>)> {
        let reply = self.send(
            protocol::QtCommand::QueryPosition(axes.clone()),
            "refresh_position",
        )?;
        self.configured.position_reply = reply.clone();
        let positions = parse_position_reply(&reply, &axes, self.configured.step_size);
        self.apply_parsed_positions(positions.clone());
        self.emit_property(self.hub, "position_reply", Value::String(reply.clone()));
        Ok((reply, positions))
    }

    fn refresh_raw_readback_after_motion(&mut self, axes: &[protocol::Axis]) -> Result<()> {
        if self.serial.is_some() {
            for _ in 0..MOTION_READBACK_POLLS {
                let (_, positions) = self.refresh_position_for_axes(axes.to_vec())?;
                if !positions
                    .iter()
                    .any(|position| position.state.is_motion_state())
                {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            let _ = self.refresh_busy()?;
        }
        Ok(())
    }

    fn configured_axes(&self) -> Vec<protocol::Axis> {
        self.configured.configured_axes()
    }

    fn apply_parsed_positions(&mut self, positions: Vec<ChuoAxisPosition>) {
        for readback in positions {
            if readback.state == ChuoAxisState::Homing {
                continue;
            }
            match readback.axis {
                protocol::Axis::A => {
                    self.configured.x = clamp_position(readback.position, self.configured.x_travel);
                    self.emit_property(self.xy, "x", Value::Position(self.configured.x));
                }
                protocol::Axis::B => {
                    self.configured.y = clamp_position(readback.position, self.configured.y_travel);
                    self.emit_property(self.xy, "y", Value::Position(self.configured.y));
                }
                axis if self.z.is_some() && axis == self.configured.z_axis => {
                    self.configured.z = clamp_position(readback.position, self.configured.z_travel);
                    if let Some(device) = self.z {
                        self.emit_property(device, "z", Value::Position(self.configured.z));
                    }
                }
                _ => {}
            }
        }
    }

    fn move_absolute(&mut self, x: Position, y: Position, z: Position) -> Result<Value> {
        let x = clamp_position(x, self.configured.x_travel);
        let y = clamp_position(y, self.configured.y_travel);
        let z = clamp_position(z, self.configured.z_travel);
        let command = protocol::QtCommand::MoveAbsolute(vec![
            (
                protocol::Axis::A,
                protocol::steps(x, self.configured.step_size)?,
            ),
            (
                protocol::Axis::B,
                protocol::steps(y, self.configured.step_size)?,
            ),
        ]);
        self.send(command, "move_xy_absolute")?;
        self.configured.x = x;
        self.configured.y = y;
        self.configured.z = z;
        self.emit_property(self.xy, "x", Value::Position(x));
        self.emit_property(self.xy, "y", Value::Position(y));
        if let Some(device) = self.z {
            self.emit_property(device, "z", Value::Position(z));
        }
        self.refresh_raw_readback_after_motion(&[protocol::Axis::A, protocol::Axis::B])?;
        Ok(self.position_map())
    }

    fn move_z(&mut self, z: Position) -> Result<Value> {
        let z = clamp_position(z, self.configured.z_travel);
        let command = protocol::QtCommand::MoveAbsolute(vec![(
            self.configured.z_axis,
            protocol::steps(z, self.configured.step_size)?,
        )]);
        self.send(command, "move_z_absolute")?;
        self.configured.z = z;
        if let Some(device) = self.z {
            self.emit_property(device, "z", Value::Position(z));
        }
        self.refresh_raw_readback_after_motion(&[self.configured.z_axis])?;
        Ok(Value::Position(z))
    }

    fn apply_stage_move(&mut self, device: DeviceId, request: StageMoveRequest) -> Result<Value> {
        self.validate_stage_move(device, &request)?;
        if device == self.xy {
            let mut x = self.configured.x;
            let mut y = self.configured.y;
            if let Some(target) = request.target.get(&StageAxis::X) {
                x = if request.relative {
                    Position::from_micrometers(x.micrometers() + target.micrometers())
                } else {
                    *target
                };
            }
            if let Some(target) = request.target.get(&StageAxis::Y) {
                y = if request.relative {
                    Position::from_micrometers(y.micrometers() + target.micrometers())
                } else {
                    *target
                };
            }
            return self.move_absolute(x, y, self.configured.z);
        }
        let target = request.target.get(&StageAxis::Z).ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidCommand,
                "Chuo QT Z StageMove requires a Z target",
            )
        })?;
        let z = if request.relative {
            Position::from_micrometers(self.configured.z.micrometers() + target.micrometers())
        } else {
            *target
        };
        self.move_z(z)
    }

    fn stage_stop(&mut self) -> Result<Value> {
        self.send(protocol::QtCommand::StopAll, "stop_all")?;
        self.refresh_raw_readback_after_motion(&self.configured_axes())?;
        Ok(Value::Map(BTreeMap::from([(
            "moving".into(),
            Value::Bool(false),
        )])))
    }

    fn stage_home(&mut self, device: DeviceId) -> Result<Value> {
        let axes = if device == self.xy {
            vec![protocol::Axis::A, protocol::Axis::B]
        } else {
            vec![self.configured.z_axis]
        };
        self.send(protocol::QtCommand::Home(axes), "home")?;
        if device == self.xy {
            self.configured.x = Position::from_micrometers(0.0);
            self.configured.y = Position::from_micrometers(0.0);
            self.emit_property(self.xy, "x", Value::Position(self.configured.x));
            self.emit_property(self.xy, "y", Value::Position(self.configured.y));
            self.refresh_raw_readback_after_motion(&[protocol::Axis::A, protocol::Axis::B])?;
            Ok(self.position_map())
        } else {
            self.configured.z = Position::from_micrometers(0.0);
            if let Some(device) = self.z {
                self.emit_property(device, "z", Value::Position(self.configured.z));
            }
            self.refresh_raw_readback_after_motion(&[self.configured.z_axis])?;
            Ok(Value::Position(self.configured.z))
        }
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "product" => Ok(Value::String(self.configured.product.clone())),
                "serial_number" => Ok(Value::String(self.configured.serial_number.clone())),
                "serial_port" => Ok(Value::String(
                    self.configured.serial_port.clone().unwrap_or_default(),
                )),
                "connected" => Ok(Value::Bool(self.serial.is_some())),
                "serial_timeout" => Ok(Value::TimeInterval(TimeInterval::from_milliseconds(
                    self.configured.serial_timeout_ms as f64,
                ))),
                "protocol" => Ok(Value::String("Chuo Seiki QT serial command control".into())),
                "step_size" => Ok(Value::Position(self.configured.step_size)),
                "high_speed" => Ok(Value::I64(self.configured.high_speed)),
                "low_speed" => Ok(Value::I64(self.configured.low_speed)),
                "acceleration_time" => Ok(Value::TimeInterval(self.configured.acceleration_time)),
                "busy_reply" => Ok(Value::String(self.configured.busy_reply.clone())),
                "position_reply" => Ok(Value::String(self.configured.position_reply.clone())),
                "last_transaction" => Ok(self.last_transaction.clone()),
                _ => invalid_property("unknown Chuo QT hub property", key),
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
            (device, "axis") if self.z == Some(device) => {
                Ok(Value::String(self.configured.z_axis.as_str().into()))
            }
            _ => invalid_property("unknown Chuo QT property", key),
        }
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        match (device, key, value) {
            (device, "x" | "y", Value::Position(_)) if device == self.xy => Ok(()),
            (device, "z", Value::Position(_)) if self.z == Some(device) => Ok(()),
            (device, "step_size", Value::Position(position))
                if device == self.hub && position.micrometers() > 0.0 =>
            {
                Ok(())
            }
            (device, "high_speed" | "low_speed", Value::I64(value))
                if device == self.hub && *value > 0 =>
            {
                Ok(())
            }
            (device, "acceleration_time", Value::TimeInterval(value))
                if device == self.hub && value.seconds() > 0.0 =>
            {
                Ok(())
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Chuo QT property {key} is read-only or wrong type"),
            )),
        }
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: Value) -> Result<Value> {
        self.validate_write(device, key, &value)?;
        match (device, key, value) {
            (device, "x", Value::Position(position)) if device == self.xy => {
                self.move_absolute(position, self.configured.y, self.configured.z)
            }
            (device, "y", Value::Position(position)) if device == self.xy => {
                self.move_absolute(self.configured.x, position, self.configured.z)
            }
            (device, "z", Value::Position(position)) if self.z == Some(device) => {
                self.move_z(position)
            }
            (device, "step_size", Value::Position(position)) if device == self.hub => {
                self.configured.step_size = position;
                self.cache_action("set_step_size");
                Ok(Value::Position(position))
            }
            (device, "high_speed", Value::I64(speed)) if device == self.hub => {
                self.configured.high_speed = speed;
                self.send_settings("set_high_speed")?;
                Ok(Value::I64(speed))
            }
            (device, "low_speed", Value::I64(speed)) if device == self.hub => {
                self.configured.low_speed = speed;
                self.send_settings("set_low_speed")?;
                Ok(Value::I64(speed))
            }
            (device, "acceleration_time", Value::TimeInterval(value)) if device == self.hub => {
                self.configured.acceleration_time = value;
                self.send_settings("set_acceleration_time")?;
                Ok(Value::TimeInterval(value))
            }
            _ => unreachable!("validated Chuo QT write"),
        }
    }

    fn validate_stage_move(&self, device: DeviceId, request: &StageMoveRequest) -> Result<()> {
        if device != self.xy && self.z != Some(device) {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Chuo QT StageMove requires the XY or Z device",
            ));
        }
        if request.target.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Chuo QT StageMove requires at least one target axis",
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
                            "axis {} is not available on this Chuo QT device",
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
                        "Chuo QT timing sequences can only target x, y, or z",
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
                _ => unreachable!("validated Chuo QT timing write"),
            }
        }
        if xy_changed {
            self.move_absolute(target_x, target_y, self.configured.z)?;
        }
        if z_changed {
            self.move_z(target_z)?;
        }
        Ok(Value::Map(changed))
    }
}

impl Driver for ChuoQtDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "chuo-qt-serial".into(),
            kind: "serial.ascii".into(),
            metadata: BTreeMap::from([
                ("baud_rate".into(), Value::I64(protocol::BAUD as i64)),
                (
                    "serial_port".into(),
                    self.configured
                        .serial_port
                        .as_ref()
                        .map(|port| Value::String(port.clone()))
                        .unwrap_or(Value::Null),
                ),
                (
                    "serial_timeout".into(),
                    Value::TimeInterval(TimeInterval::from_milliseconds(
                        self.configured.serial_timeout_ms as f64,
                    )),
                ),
                ("data_bits".into(), Value::I64(protocol::DATA_BITS as i64)),
                ("stop_bits".into(), Value::I64(protocol::STOP_BITS as i64)),
                ("parity".into(), Value::String(protocol::PARITY.into())),
                (
                    "line_ending".into(),
                    Value::String(protocol::LINE_ENDING.into()),
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
                label: "chuo-qt-hub".into(),
                vendor: Some("Chuo Precision Industrial".into()),
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
                    bool_property("connected", "Connected", false),
                    time_property("serial_timeout", "Serial timeout", false),
                    string_property("protocol", "Protocol", false),
                    position_property("step_size", "Step size", true, None),
                    integer_range_property("high_speed", "High speed", true, 1, 1_000_000),
                    integer_range_property("low_speed", "Low speed", true, 1, 1_000_000),
                    time_property("acceleration_time", "Acceleration time", true),
                    string_property("busy_reply", "Busy reply", false),
                    string_property("position_reply", "Position reply", false),
                    map_property("last_transaction", "Last transaction", false),
                ],
                metadata: source_metadata(),
            },
            DeviceDescriptor {
                id: self.xy,
                driver: self.id,
                label: "chuo-qt-xy-stage".into(),
                vendor: Some("Chuo Precision Industrial".into()),
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
                    ("axis_x".into(), Value::String("A".into())),
                    ("axis_y".into(), Value::String("B".into())),
                ]),
            },
        ];
        if let Some(device) = self.z {
            descriptors.push(DeviceDescriptor {
                id: device,
                driver: self.id,
                label: "chuo-qt-z-stage".into(),
                vendor: Some("Chuo Precision Industrial".into()),
                model: Some(self.configured.product.clone()),
                serial: Some(format!("{}:z", self.configured.serial_number)),
                kinds: vec!["axis.z".into(), "stage.z".into(), "motion.stage".into()],
                properties: vec![
                    position_property("z", "Z", true, Some(self.configured.z_travel)),
                    position_property("z_travel", "Z travel", false, None),
                    string_property("axis", "Axis", false),
                ],
                metadata: BTreeMap::from([(
                    "axis".into(),
                    Value::String(self.configured.z_axis.as_str().into()),
                )]),
            });
        }
        descriptors
    }

    fn graph(&self) -> DeviceGraph {
        let mut graph = DeviceGraph::default();
        let _ = graph.insert_node(GraphNode {
            id: self.resource.0,
            kind: NodeKind::Resource,
            label: "chuo-qt-serial".into(),
        });
        let _ = graph.insert_node(GraphNode {
            id: self.hub.0,
            kind: NodeKind::Hub,
            label: "chuo-qt-hub".into(),
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
                        format!("chuo qt read {key}"),
                        Value::String(key.clone()),
                    ));
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("chuo qt write {key}"),
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
                            "unknown Chuo QT capability",
                        ));
                    };
                    if !descriptor.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "Chuo QT {} request kind does not match",
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
                                "Chuo QT GenericCommand expects GenericCommandRequest",
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
                            "refresh_readbacks" | "refresh_busy" | "refresh_position"
                        ) {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Chuo QT GenericCommand supports refresh_readbacks, refresh_busy, and refresh_position",
                            ));
                        }
                        if !request.params.is_empty() {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Chuo QT GenericCommand refresh commands do not accept params",
                            ));
                        }
                    }
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("chuo qt {}", descriptor.kind.name()),
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
                        "chuo qt state set",
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
                    if device == self.hub && key == "busy_reply" {
                        let _ = self.refresh_busy()?;
                    } else if device == self.hub && key == "position_reply" {
                        let _ = self.refresh_position()?;
                    }
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
                        ) => {
                            if !request.params.is_empty() {
                                return Err(Error::new(
                                    ErrorCode::InvalidCommand,
                                    "Chuo QT GenericCommand refresh commands do not accept params",
                                ));
                            }
                            match request.command.as_str() {
                                "refresh_readbacks" => self.refresh_readbacks()?,
                                "refresh_busy" => Value::String(self.refresh_busy()?),
                                "refresh_position" => Value::String(self.refresh_position()?),
                                _ => return Err(Error::new(
                                    ErrorCode::InvalidCommand,
                                    "Chuo QT GenericCommand supports refresh_readbacks, refresh_busy, and refresh_position",
                                )),
                            }
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported Chuo QT capability invocation",
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
                "chuo qt timing arm summary",
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
                "chuo qt timing start sequence",
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
                "chuo qt timing stop sequence",
                Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "stop")),
                    ("changed".into(), changed),
                ])),
            )],
        })
    }
}

impl ChuoQtDriver {
    fn validate_read(&self, device: DeviceId, key: &str) -> Result<()> {
        if device == self.hub
            && matches!(
                key,
                "product"
                    | "serial_number"
                    | "serial_port"
                    | "connected"
                    | "serial_timeout"
                    | "protocol"
                    | "step_size"
                    | "high_speed"
                    | "low_speed"
                    | "acceleration_time"
                    | "busy_reply"
                    | "position_reply"
                    | "last_transaction"
            )
        {
            return Ok(());
        }
        if device == self.xy && matches!(key, "x" | "y" | "x_travel" | "y_travel") {
            return Ok(());
        }
        if self.z == Some(device) && matches!(key, "z" | "z_travel" | "axis") {
            return Ok(());
        }
        invalid_property("unknown Chuo QT property", key)
    }
}

fn validate_motion_settings(
    step_size: Position,
    high_speed: i64,
    low_speed: i64,
    acceleration_time: TimeInterval,
) -> Result<()> {
    if step_size.micrometers() <= 0.0 {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "Chuo QT step_size must be positive",
        ));
    }
    if high_speed <= 0 || low_speed <= 0 || low_speed >= high_speed {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "Chuo QT speed settings require 0 < low_speed < high_speed",
        ));
    }
    if acceleration_time.seconds() <= 0.0 {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "Chuo QT acceleration_time must be positive",
        ));
    }
    Ok(())
}

fn axis_prop(device: &DeviceConfig, key: &str) -> Option<protocol::Axis> {
    match device.properties.get(key) {
        Some(Value::String(value)) => match value.as_str() {
            "A" | "a" => Some(protocol::Axis::A),
            "B" | "b" => Some(protocol::Axis::B),
            "C" | "c" => Some(protocol::Axis::C),
            _ => None,
        },
        _ => None,
    }
}

fn clamp_position(value: Position, travel: Position) -> Position {
    Position::from_micrometers(value.micrometers().clamp(0.0, travel.micrometers()))
}

fn source_metadata() -> BTreeMap<String, Value> {
    BTreeMap::from([
        (
            "evidence".into(),
            Value::String("reverse engineered plus manufacturer QT command-control page".into()),
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

fn invalid_property<T>(message: &str, key: &str) -> Result<T> {
    Err(Error::new(
        ErrorCode::InvalidProperty,
        format!("{message}: {key}"),
    ))
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

fn u64_prop(device: &DeviceConfig, key: &str) -> Option<u64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if *value >= 0 => Some(*value as u64),
        Some(Value::TimeInterval(value))
            if value.seconds().is_finite() && value.seconds() >= 0.0 =>
        {
            Some((value.seconds() * 1000.0).round() as u64)
        }
        _ => None,
    }
}

fn position_prop(device: &DeviceConfig, key: &str) -> Option<Position> {
    match device.properties.get(key) {
        Some(Value::Position(value)) => Some(*value),
        _ => None,
    }
}

fn time_prop(device: &DeviceConfig, key: &str) -> Option<TimeInterval> {
    match device.properties.get(key) {
        Some(Value::TimeInterval(value)) => Some(*value),
        _ => None,
    }
}
