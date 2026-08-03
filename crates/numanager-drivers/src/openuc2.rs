use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::{LineEnding, ScriptedSerial, SerialIo, SerialLineCodec};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
#[cfg(feature = "os-serial")]
use std::time::{Duration, Instant};

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const SEND_ENDING: LineEnding = LineEnding::Lf;
    pub const RECV_ENDING: LineEnding = LineEnding::Cr;

    #[derive(Debug, Clone, PartialEq)]
    pub struct OpenUc2Probe {
        pub controller: String,
        pub x_travel_um: f64,
        pub y_travel_um: f64,
        pub z_travel_um: f64,
        pub laser_wavelength: Wavelength,
    }

    impl OpenUc2Probe {
        pub fn simulated() -> Self {
            Self {
                controller: "UC2_Feather numanager-sim".into(),
                x_travel_um: 40_000.0,
                y_travel_um: 40_000.0,
                z_travel_um: 4_000.0,
                laser_wavelength: Wavelength::from_nanometers(488.0),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum OpenUc2Command {
        StateGet,
        LaserAct {
            laser_id: u8,
            value: u8,
        },
        Move {
            steppers: Vec<StepperMove>,
            speed: u32,
            absolute: bool,
        },
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct StepperMove {
        pub id: u8,
        pub position: f64,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct OpenUc2State {
        pub controller: String,
        pub x_um: f64,
        pub y_um: f64,
        pub z_um: f64,
        pub laser_enabled: bool,
        pub laser_power_percent: f64,
    }

    impl OpenUc2State {
        pub fn value(&self) -> Value {
            Value::Map(BTreeMap::from([
                ("controller".into(), Value::String(self.controller.clone())),
                ("x".into(), position(self.x_um)),
                ("y".into(), position(self.y_um)),
                ("z".into(), position(self.z_um)),
                ("laser_enabled".into(), Value::Bool(self.laser_enabled)),
                (
                    "laser_power".into(),
                    Value::Ratio(Ratio::from_percent(self.laser_power_percent)),
                ),
            ]))
        }
    }

    pub fn encode(command: &OpenUc2Command) -> String {
        match command {
            OpenUc2Command::StateGet => "{\"task\":\"/state_get\"}".into(),
            OpenUc2Command::LaserAct { laser_id, value } => {
                format!("{{\"task\":\"/laser_act\",\"LASERid\":{laser_id},\"LASERval\":{value}}}")
            }
            OpenUc2Command::Move {
                steppers,
                speed,
                absolute,
            } => {
                let isabs = u8::from(*absolute);
                let steppers = steppers
                    .iter()
                    .map(|stepper| {
                        format!(
                            "{{\"stepperid\":{},\"position\":{:.3},\"speed\":{},\"isabs\":{}}}",
                            stepper.id, stepper.position, speed, isabs
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!("{{\"task\":\"/motor_act\",\"motor\":{{\"steppers\":[{steppers}]}}}}")
            }
        }
    }

    pub fn parse_state(reply: &str) -> Result<OpenUc2State> {
        let controller = string_field(reply, "controller")?;
        if !controller.contains("UC2") {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("unexpected OpenUC2 controller field: {controller}"),
            ));
        }
        Ok(OpenUc2State {
            controller,
            x_um: f64_field(reply, "x")?,
            y_um: f64_field(reply, "y")?,
            z_um: f64_field(reply, "z")?,
            laser_enabled: bool_field(reply, "laser_enabled")?,
            laser_power_percent: f64_field(reply, "laser_power")?,
        })
    }

    fn string_field(reply: &str, key: &str) -> Result<String> {
        let start = value_start(reply, key)?;
        let value = reply[start..].trim_start();
        if !value.starts_with('"') {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("OpenUC2 field {key} is not a string"),
            ));
        }
        let start = start + reply[start..].find('"').unwrap() + 1;
        let end = reply[start..].find('"').ok_or_else(|| {
            Error::new(
                ErrorCode::Transport,
                format!("unterminated OpenUC2 string field {key}"),
            )
        })? + start;
        Ok(reply[start..end].to_string())
    }

    fn f64_field(reply: &str, key: &str) -> Result<f64> {
        scalar_field(reply, key)?.parse::<f64>().map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("invalid OpenUC2 numeric field {key}: {error}"),
            )
        })
    }

    fn bool_field(reply: &str, key: &str) -> Result<bool> {
        match scalar_field(reply, key)?.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            other => Err(Error::new(
                ErrorCode::Transport,
                format!("invalid OpenUC2 bool field {key}: {other}"),
            )),
        }
    }

    fn scalar_field(reply: &str, key: &str) -> Result<String> {
        let start = value_start(reply, key)?;
        let tail = &reply[start..];
        let end = tail.find(|c| c == ',' || c == '}').unwrap_or(tail.len());
        Ok(tail[..end].trim().to_string())
    }

    fn value_start(reply: &str, key: &str) -> Result<usize> {
        let needle = format!("\"{key}\"");
        let key_start = reply.find(&needle).ok_or_else(|| {
            Error::new(ErrorCode::Transport, format!("missing OpenUC2 field {key}"))
        })? + needle.len();
        let tail = &reply[key_start..];
        let colon = tail.find(':').ok_or_else(|| {
            Error::new(
                ErrorCode::Transport,
                format!("missing OpenUC2 field separator for {key}"),
            )
        })?;
        Ok(key_start + colon + 1)
    }
}

pub struct OpenUc2Discovery {
    next_id: DriverId,
    simulated: bool,
    configured: Vec<OpenUc2ConfiguredProbe>,
}

#[derive(Debug, Clone)]
pub struct OpenUc2ConfiguredProbe {
    label: String,
    probe: protocol::OpenUc2Probe,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connect_real_transport: bool,
}

impl OpenUc2Discovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            simulated: true,
            configured: Vec::new(),
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let configured = config
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), "openuc2" | "open-uc2"))
            .map(OpenUc2ConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            next_id,
            simulated: false,
            configured,
        })
    }
}

impl DriverDiscovery for OpenUc2Discovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        if self.simulated {
            return Ok(vec![DriverCandidate::from_driver(
                "Simulated OpenUC2 Feather controller",
                Box::new(OpenUc2Driver::simulated(self.next_id)),
            )]);
        }
        self.configured
            .iter()
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let driver: Box<dyn Driver> = if configured.connect_real_transport {
                    Box::new(OpenUc2Driver::serial(id, configured.clone())?)
                } else {
                    Box::new(OpenUc2Driver::configured(id, configured.clone()))
                };
                Ok(DriverCandidate::from_driver(
                    configured.label.clone(),
                    driver,
                ))
            })
            .collect()
    }
}

impl OpenUc2ConfiguredProbe {
    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = protocol::OpenUc2Probe::simulated();
        probe.controller = string_prop(device, "controller").unwrap_or(probe.controller);
        probe.x_travel_um =
            position_config_um(device, "x_travel", "x_travel_um").unwrap_or(probe.x_travel_um);
        probe.y_travel_um =
            position_config_um(device, "y_travel", "y_travel_um").unwrap_or(probe.y_travel_um);
        probe.z_travel_um =
            position_config_um(device, "z_travel", "z_travel_um").unwrap_or(probe.z_travel_um);
        probe.laser_wavelength =
            wavelength_prop(device, "laser_wavelength").unwrap_or(probe.laser_wavelength);
        Ok(Self {
            label: if device.label.is_empty() {
                "Configured OpenUC2 Feather controller".into()
            } else {
                device.label.clone()
            },
            probe,
            serial_port: string_prop(device, "serial_port"),
            baud_rate: u32_prop(device, "baud_rate").unwrap_or(115_200),
            serial_timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(500),
            connect_real_transport: bool_prop(device, "connect").unwrap_or(false),
        })
    }
}

pub struct OpenUc2Driver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    xy: DeviceId,
    z: DeviceId,
    laser: DeviceId,
    probe: protocol::OpenUc2Probe,
    xy_position_um: (f64, f64),
    z_position_um: f64,
    laser_enabled: bool,
    laser_power_percent: f64,
    serial_port: Option<String>,
    baud_rate: u32,
    connected: bool,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
}

impl OpenUc2Driver {
    pub fn configured(id: DriverId, configured: OpenUc2ConfiguredProbe) -> Self {
        Self::new(
            id,
            configured.probe,
            Box::new(ScriptedSerial::new()),
            configured.serial_port,
            configured.baud_rate,
            false,
        )
    }

    pub fn simulated(id: DriverId) -> Self {
        Self::new(
            id,
            protocol::OpenUc2Probe::simulated(),
            Box::new(ScriptedSerial::new()),
            None,
            115_200,
            false,
        )
    }

    fn new(
        id: DriverId,
        probe: protocol::OpenUc2Probe,
        serial: Box<dyn SerialIo>,
        serial_port: Option<String>,
        baud_rate: u32,
        connected: bool,
    ) -> Self {
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 201)),
            hub: DeviceId(NodeId(id.0 * 1000 + 210)),
            xy: DeviceId(NodeId(id.0 * 1000 + 211)),
            z: DeviceId(NodeId(id.0 * 1000 + 212)),
            laser: DeviceId(NodeId(id.0 * 1000 + 213)),
            probe,
            xy_position_um: (0.0, 0.0),
            z_position_um: 0.0,
            laser_enabled: false,
            laser_power_percent: 0.0,
            serial_port,
            baud_rate,
            connected,
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            codec: SerialLineCodec::new(protocol::SEND_ENDING, protocol::RECV_ENDING),
        }
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: OpenUc2ConfiguredProbe) -> Result<Self> {
        let port_name = configured.serial_port.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "OpenUC2 real serial config requires serial_port",
            )
        })?;
        let serial = Box::new(numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(port_name, configured.baud_rate)
                .timeout(Duration::from_millis(configured.serial_timeout_ms)),
        )?);
        let mut driver = Self::new(
            id,
            configured.probe,
            serial,
            configured.serial_port,
            configured.baud_rate,
            true,
        );
        driver.refresh_startup_state(configured.serial_timeout_ms)?;
        Ok(driver)
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, configured: OpenUc2ConfiguredProbe) -> Result<Self> {
        let _ = configured.serial_port.as_ref();
        let _ = configured.baud_rate;
        let _ = configured.serial_timeout_ms;
        Err(Error::new(
            ErrorCode::Unsupported,
            "OpenUC2 real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    #[cfg(feature = "os-serial")]
    fn refresh_startup_state(&mut self, timeout_ms: u64) -> Result<()> {
        self.write_json(protocol::OpenUc2Command::StateGet)?;
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            if self.drain_serial_state()? {
                return Ok(());
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        Err(Error::new(
            ErrorCode::Transport,
            "OpenUC2 did not return a startup state reply",
        ))
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        vec![
            DeviceDescriptor {
                id: self.hub,
                driver: self.id,
                label: "openuc2-hub".into(),
                vendor: Some("OpenUC2".into()),
                model: Some("UC2 Feather".into()),
                serial: None,
                kinds: vec!["hub".into(), "microcontroller".into()],
                properties: vec![property(
                    "state_summary",
                    "State summary",
                    ValueType::Map,
                    None,
                    false,
                    None,
                )],
                metadata: BTreeMap::from([
                    (
                        "controller".into(),
                        Value::String(self.probe.controller.clone()),
                    ),
                    ("state_summary".into(), self.state_summary()),
                ]),
            },
            DeviceDescriptor {
                id: self.xy,
                driver: self.id,
                label: "openuc2-xy".into(),
                vendor: Some("OpenUC2".into()),
                model: Some("UC2 XY stage".into()),
                serial: None,
                kinds: vec!["axis.xy".into()],
                properties: vec![
                    sequenceable_position_property("x", "X position", true, self.probe.x_travel_um),
                    sequenceable_position_property("y", "Y position", true, self.probe.y_travel_um),
                ],
                metadata: BTreeMap::from([
                    ("x_travel".into(), position(self.probe.x_travel_um)),
                    ("y_travel".into(), position(self.probe.y_travel_um)),
                    (
                        "legacy_x_travel_um".into(),
                        position(self.probe.x_travel_um),
                    ),
                    (
                        "legacy_y_travel_um".into(),
                        position(self.probe.y_travel_um),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.z,
                driver: self.id,
                label: "openuc2-z".into(),
                vendor: Some("OpenUC2".into()),
                model: Some("UC2 Z stage".into()),
                serial: None,
                kinds: vec!["axis.z".into()],
                properties: vec![sequenceable_position_property(
                    "z",
                    "Z position",
                    true,
                    self.probe.z_travel_um,
                )],
                metadata: BTreeMap::from([
                    ("z_travel".into(), position(self.probe.z_travel_um)),
                    (
                        "legacy_z_travel_um".into(),
                        position(self.probe.z_travel_um),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.laser,
                driver: self.id,
                label: "openuc2-laser".into(),
                vendor: Some("OpenUC2".into()),
                model: Some("UC2 laser output".into()),
                serial: None,
                kinds: vec![
                    "light.source".into(),
                    "shutter".into(),
                    "trigger.sink".into(),
                ],
                properties: vec![
                    sequenceable_property("enabled", "Enabled", ValueType::Bool, None, true, None),
                    sequenceable_property(
                        "power",
                        "Power",
                        ValueType::Ratio,
                        Some("percent"),
                        true,
                        Some(Range {
                            min: Value::Ratio(Ratio::from_percent(0.0)),
                            max: Value::Ratio(Ratio::from_percent(100.0)),
                        }),
                    ),
                    property(
                        "wavelength",
                        "Wavelength",
                        ValueType::Wavelength,
                        None,
                        false,
                        None,
                    ),
                ],
                metadata: BTreeMap::from([(
                    "wavelength".into(),
                    Value::Wavelength(self.probe.laser_wavelength),
                )]),
            },
        ]
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        match (device, key) {
            (device, "x") if device == self.xy => Ok(position(self.xy_position_um.0)),
            (device, "y") if device == self.xy => Ok(position(self.xy_position_um.1)),
            (device, "z") if device == self.z => Ok(position(self.z_position_um)),
            (device, "enabled") if device == self.laser => Ok(Value::Bool(self.laser_enabled)),
            (device, "power") if device == self.laser => {
                Ok(Value::Ratio(Ratio::from_percent(self.laser_power_percent)))
            }
            (device, "wavelength") if device == self.laser => {
                Ok(Value::Wavelength(self.probe.laser_wavelength))
            }
            (device, "state_summary") if device == self.hub => Ok(self.state_summary()),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown OpenUC2 property {key}"),
            )),
        }
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        let descriptor = self
            .descriptors_for()
            .into_iter()
            .find(|descriptor| descriptor.id == device)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown device"))?;
        let schema = descriptor
            .properties
            .iter()
            .find(|property| property.key == key)
            .ok_or_else(|| Error::new(ErrorCode::InvalidProperty, "unknown property"))?;
        if !schema.writable {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "property is read-only",
            ));
        }
        schema.validate(value)
    }

    fn apply_write(&mut self, device: DeviceId, key: &str, value: &Value) -> Result<Value> {
        match (device, key, value) {
            (device, "x", value) if device == self.xy => {
                self.xy_position_um.0 = position_um(value)?.clamp(0.0, self.probe.x_travel_um);
                Ok(position(self.xy_position_um.0))
            }
            (device, "y", value) if device == self.xy => {
                self.xy_position_um.1 = position_um(value)?.clamp(0.0, self.probe.y_travel_um);
                Ok(position(self.xy_position_um.1))
            }
            (device, "z", value) if device == self.z => {
                self.z_position_um = position_um(value)?.clamp(0.0, self.probe.z_travel_um);
                Ok(position(self.z_position_um))
            }
            (device, "enabled", Value::Bool(enabled)) if device == self.laser => {
                self.laser_enabled = *enabled;
                Ok(Value::Bool(*enabled))
            }
            (device, "power", Value::Ratio(power)) if device == self.laser => {
                self.laser_power_percent = power.percent().clamp(0.0, 100.0);
                Ok(Value::Ratio(Ratio::from_percent(self.laser_power_percent)))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid OpenUC2 write {key}"),
            )),
        }
    }

    fn flush_physical_state(&mut self) -> Result<()> {
        self.write_json(protocol::OpenUc2Command::Move {
            steppers: vec![
                protocol::StepperMove {
                    id: 1,
                    position: self.xy_position_um.0,
                },
                protocol::StepperMove {
                    id: 2,
                    position: self.xy_position_um.1,
                },
                protocol::StepperMove {
                    id: 3,
                    position: self.z_position_um,
                },
            ],
            speed: 5000,
            absolute: true,
        })?;
        let value = if self.laser_enabled {
            (self.laser_power_percent * 2.55).round().clamp(1.0, 255.0) as u8
        } else {
            0
        };
        self.write_json(protocol::OpenUc2Command::LaserAct { laser_id: 1, value })
    }

    fn laser_wire_value(&self) -> u8 {
        if self.laser_enabled {
            (self.laser_power_percent * 2.55).round().clamp(1.0, 255.0) as u8
        } else {
            0
        }
    }

    fn flush_laser_state(&mut self) -> Result<()> {
        self.write_json(protocol::OpenUc2Command::LaserAct {
            laser_id: 1,
            value: self.laser_wire_value(),
        })
    }

    fn refresh_state(&mut self) -> Result<bool> {
        self.write_json(protocol::OpenUc2Command::StateGet)?;
        self.drain_serial_state()
    }

    fn state_summary(&self) -> Value {
        protocol::OpenUc2State {
            controller: self.probe.controller.clone(),
            x_um: self.xy_position_um.0,
            y_um: self.xy_position_um.1,
            z_um: self.z_position_um,
            laser_enabled: self.laser_enabled,
            laser_power_percent: self.laser_power_percent,
        }
        .value()
    }

    fn write_json(&mut self, command: protocol::OpenUc2Command) -> Result<()> {
        let line = protocol::encode(&command);
        self.serial.write(&self.codec.encode(&line))
    }

    fn drain_serial_state(&mut self) -> Result<bool> {
        let bytes = self.serial.read_available()?;
        let mut parsed_state = false;
        for line in self.codec.push(&bytes) {
            if let Ok(state) = protocol::parse_state(&line) {
                self.apply_hardware_state(state);
                parsed_state = true;
            } else {
                self.pending
                    .push_back(DriverEvent::Event(Event::Log(LogEvent {
                        driver: Some(self.id),
                        message: format!("openuc2 serial: {line}"),
                    })));
            }
        }
        Ok(parsed_state)
    }

    fn apply_hardware_state(&mut self, state: protocol::OpenUc2State) {
        self.probe.controller = state.controller;
        let old_summary = self.state_summary();

        if self.xy_position_um.0 != state.x_um {
            self.xy_position_um.0 = state.x_um.clamp(0.0, self.probe.x_travel_um);
            self.emit_property(self.xy, "x", position(self.xy_position_um.0));
        }
        if self.xy_position_um.1 != state.y_um {
            self.xy_position_um.1 = state.y_um.clamp(0.0, self.probe.y_travel_um);
            self.emit_property(self.xy, "y", position(self.xy_position_um.1));
        }
        if self.z_position_um != state.z_um {
            self.z_position_um = state.z_um.clamp(0.0, self.probe.z_travel_um);
            self.emit_property(self.z, "z", position(self.z_position_um));
        }
        if self.laser_enabled != state.laser_enabled {
            self.laser_enabled = state.laser_enabled;
            self.emit_property(self.laser, "enabled", Value::Bool(self.laser_enabled));
        }
        if self.laser_power_percent != state.laser_power_percent {
            self.laser_power_percent = state.laser_power_percent.clamp(0.0, 100.0);
            self.emit_property(
                self.laser,
                "power",
                Value::Ratio(Ratio::from_percent(self.laser_power_percent)),
            );
        }

        let new_summary = self.state_summary();
        if old_summary != new_summary {
            self.emit_property(self.hub, "state_summary", new_summary);
        }
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

    fn owns_device(&self, device: DeviceId) -> bool {
        device == self.hub || device == self.xy || device == self.z || device == self.laser
    }

    fn has_timed_laser(&self, plan: &TimingPlan) -> bool {
        plan.participants.contains(&self.laser)
            || plan
                .routes
                .iter()
                .any(|route| route.from == self.laser || route.to == self.laser)
            || plan
                .sequences
                .iter()
                .any(|sequence| sequence.device == self.laser)
    }

    fn local_timing_routes(&self, plan: &TimingPlan) -> Vec<Value> {
        plan.routes
            .iter()
            .filter(|route| self.owns_device(route.from) || self.owns_device(route.to))
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

    fn local_timing_sequences(&self, plan: &TimingPlan) -> Vec<Value> {
        plan.sequences
            .iter()
            .filter(|sequence| self.owns_device(sequence.device))
            .map(|sequence| {
                Value::Map(BTreeMap::from([
                    ("device".into(), Value::I64(sequence.device.0 .0 as i64)),
                    ("property".into(), Value::String(sequence.property.clone())),
                    ("values".into(), Value::List(sequence.values.clone())),
                ]))
            })
            .collect()
    }

    fn local_timing_sequence_refs<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| self.owns_device(sequence.device))
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequence_refs(plan) {
            let descriptor = self
                .descriptors_for()
                .into_iter()
                .find(|descriptor| descriptor.id == sequence.device)
                .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown device"))?;
            let schema = descriptor
                .properties
                .iter()
                .find(|property| property.key == sequence.property)
                .ok_or_else(|| Error::new(ErrorCode::InvalidProperty, "unknown property"))?;
            if !schema.sequenceable {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    format!("property {} is not sequenceable", sequence.property),
                ));
            }
            for value in &sequence.values {
                schema.validate(value)?;
            }
        }
        Ok(())
    }

    fn apply_timing_sequence_step(&mut self, plan: &TimingPlan, start: bool) -> Result<Value> {
        let sequences = self.local_timing_sequence_refs(plan);
        let has_laser_enabled_sequence = sequences.iter().any(|sequence| {
            sequence.device == self.laser && sequence.property.as_str() == "enabled"
        });
        let mut changed = BTreeMap::new();

        if self.has_timed_laser(plan) && !has_laser_enabled_sequence {
            let value = self.apply_write(self.laser, "enabled", &Value::Bool(start))?;
            self.emit_property(self.laser, "enabled", value.clone());
            changed.insert(format!("{}:enabled", (self.laser.0).0), value);
        }

        let writes = sequences
            .into_iter()
            .filter_map(|sequence| {
                let value = if start {
                    sequence.values.first()
                } else {
                    sequence.values.last()
                }?;
                Some(StateWrite {
                    device: sequence.device,
                    property: sequence.property.clone(),
                    value: value.clone(),
                })
            })
            .collect::<Vec<_>>();

        for write in writes {
            let value = self.apply_write(write.device, &write.property, &write.value)?;
            self.emit_property(write.device, &write.property, value.clone());
            changed.insert(format!("{}:{}", (write.device.0).0, write.property), value);
        }

        if !changed.is_empty() {
            self.flush_physical_state()?;
        }

        Ok(Value::Map(changed))
    }

    fn timing_summary(&self, plan: &TimingPlan, action: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("action".into(), Value::String(action.into())),
            ("hub".into(), Value::I64(self.hub.0 .0 as i64)),
            ("laser".into(), Value::I64(self.laser.0 .0 as i64)),
            (
                "timed_laser".into(),
                Value::Bool(self.has_timed_laser(plan)),
            ),
            ("laser_enabled".into(), Value::Bool(self.laser_enabled)),
            (
                "laser_power".into(),
                Value::Ratio(Ratio::from_percent(self.laser_power_percent)),
            ),
            (
                "laser_wire_value".into(),
                Value::I64(self.laser_wire_value() as i64),
            ),
            ("routes".into(), Value::List(self.local_timing_routes(plan))),
            (
                "sequences".into(),
                Value::List(self.local_timing_sequences(plan)),
            ),
        ]))
    }

    fn timing_transaction(
        &self,
        description: &str,
        command: protocol::OpenUc2Command,
    ) -> PhysicalTransaction {
        let line = protocol::encode(&command);
        PhysicalTransaction {
            resource: Some(self.resource),
            description: description.into(),
            payload: Value::Bytes(self.codec.encode(&line)),
        }
    }

    fn validate_stage_move(&self, device: DeviceId, request: &StageMoveRequest) -> Result<()> {
        if device != self.xy && device != self.z {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "OpenUC2 StageMove requires the XY or Z stage device",
            ));
        }
        for axis in request.target.keys() {
            match (device, axis) {
                (device, StageAxis::X | StageAxis::Y) if device == self.xy => {}
                (device, StageAxis::Z) if device == self.z => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        format!(
                            "axis {} is not available on this OpenUC2 device",
                            axis.name()
                        ),
                    ))
                }
            }
        }
        Ok(())
    }

    fn stage_move_command(
        &self,
        device: DeviceId,
        request: &StageMoveRequest,
    ) -> Result<protocol::OpenUc2Command> {
        self.validate_stage_move(device, request)?;
        let mut steppers = Vec::new();
        if device == self.xy {
            if let Some(target) = request.target.get(&StageAxis::X) {
                let mut value = target.micrometers();
                if request.relative {
                    value += self.xy_position_um.0;
                }
                steppers.push(protocol::StepperMove {
                    id: 1,
                    position: value.clamp(0.0, self.probe.x_travel_um),
                });
            }
            if let Some(target) = request.target.get(&StageAxis::Y) {
                let mut value = target.micrometers();
                if request.relative {
                    value += self.xy_position_um.1;
                }
                steppers.push(protocol::StepperMove {
                    id: 2,
                    position: value.clamp(0.0, self.probe.y_travel_um),
                });
            }
        } else if let Some(target) = request.target.get(&StageAxis::Z) {
            let mut value = target.micrometers();
            if request.relative {
                value += self.z_position_um;
            }
            steppers.push(protocol::StepperMove {
                id: 3,
                position: value.clamp(0.0, self.probe.z_travel_um),
            });
        }
        if steppers.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "OpenUC2 StageMove requires at least one target axis",
            ));
        }
        Ok(protocol::OpenUc2Command::Move {
            steppers,
            speed: stage_move_speed(request),
            absolute: true,
        })
    }

    fn apply_stage_move(&mut self, device: DeviceId, request: StageMoveRequest) -> Result<Value> {
        let command = self.stage_move_command(device, &request)?;
        if device == self.xy {
            if let Some(target) = request.target.get(&StageAxis::X) {
                let mut value = target.micrometers();
                if request.relative {
                    value += self.xy_position_um.0;
                }
                self.xy_position_um.0 = value.clamp(0.0, self.probe.x_travel_um);
                self.emit_property(self.xy, "x", position(self.xy_position_um.0));
            }
            if let Some(target) = request.target.get(&StageAxis::Y) {
                let mut value = target.micrometers();
                if request.relative {
                    value += self.xy_position_um.1;
                }
                self.xy_position_um.1 = value.clamp(0.0, self.probe.y_travel_um);
                self.emit_property(self.xy, "y", position(self.xy_position_um.1));
            }
        } else if let Some(target) = request.target.get(&StageAxis::Z) {
            let mut value = target.micrometers();
            if request.relative {
                value += self.z_position_um;
            }
            self.z_position_um = value.clamp(0.0, self.probe.z_travel_um);
            self.emit_property(self.z, "z", position(self.z_position_um));
        }
        self.write_json(command)?;
        Ok(Value::Map(BTreeMap::from([
            ("x".into(), position(self.xy_position_um.0)),
            ("y".into(), position(self.xy_position_um.1)),
            ("z".into(), position(self.z_position_um)),
        ])))
    }

    fn invoke_transactions(
        &self,
        device: DeviceId,
        kind: CapabilityKind,
        request: &CapabilityRequest,
    ) -> Result<Vec<protocol::OpenUc2Command>> {
        match (kind, request) {
            (CapabilityKind::StageMove, CapabilityRequest::StageMove(request)) => {
                Ok(vec![self.stage_move_command(device, request)?])
            }
            (CapabilityKind::StageMove, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "OpenUC2 StageMove expects a StageMoveRequest",
            )),
            (CapabilityKind::Dac, request) if device == self.laser => {
                let power = dac_request_percent(request)?;
                let value = if self.laser_enabled {
                    laser_wire_value(true, power)
                } else {
                    0
                };
                Ok(vec![protocol::OpenUc2Command::LaserAct {
                    laser_id: 1,
                    value,
                }])
            }
            (CapabilityKind::TriggerSink, request) if device == self.laser => {
                let actions = trigger_sink_actions(request)?;
                Ok(actions
                    .into_iter()
                    .map(|enabled| protocol::OpenUc2Command::LaserAct {
                        laser_id: 1,
                        value: laser_wire_value(enabled, self.laser_power_percent),
                    })
                    .collect())
            }
            (CapabilityKind::GenericCommand, CapabilityRequest::GenericCommand(request))
                if device == self.hub =>
            {
                validate_generic_command(request)?;
                Ok(vec![protocol::OpenUc2Command::StateGet])
            }
            (CapabilityKind::GenericCommand, _) if device == self.hub => Err(Error::new(
                ErrorCode::InvalidCommand,
                "OpenUC2 GenericCommand expects GenericCommandRequest",
            )),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported OpenUC2 invocation capability",
            )),
        }
    }

    fn apply_invoke(
        &mut self,
        device: DeviceId,
        kind: CapabilityKind,
        request: CapabilityRequest,
    ) -> Result<Value> {
        match kind {
            CapabilityKind::StageMove => match request {
                CapabilityRequest::StageMove(request) => self.apply_stage_move(device, request),
                _ => Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "OpenUC2 StageMove expects a StageMoveRequest",
                )),
            },
            CapabilityKind::Dac if device == self.laser => {
                let power = dac_request_percent(&request)?;
                let value =
                    self.apply_write(device, "power", &Value::Ratio(Ratio::from_percent(power)))?;
                self.flush_laser_state()?;
                self.emit_property(device, "power", value.clone());
                Ok(Value::Map(BTreeMap::from([
                    ("power".into(), value),
                    (
                        "wire_value".into(),
                        Value::I64(self.laser_wire_value() as i64),
                    ),
                    ("commands".into(), Value::I64(1)),
                ])))
            }
            CapabilityKind::TriggerSink if device == self.laser => {
                let actions = trigger_sink_actions(&request)?;
                for enabled in &actions {
                    let value = self.apply_write(device, "enabled", &Value::Bool(*enabled))?;
                    self.flush_laser_state()?;
                    self.emit_property(device, "enabled", value);
                }
                Ok(Value::Map(BTreeMap::from([
                    ("triggered".into(), Value::Bool(true)),
                    ("enabled".into(), Value::Bool(self.laser_enabled)),
                    (
                        "wire_value".into(),
                        Value::I64(self.laser_wire_value() as i64),
                    ),
                    ("commands".into(), Value::I64(actions.len() as i64)),
                ])))
            }
            CapabilityKind::GenericCommand if device == self.hub => {
                let CapabilityRequest::GenericCommand(request) = request else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "OpenUC2 GenericCommand expects GenericCommandRequest",
                    ));
                };
                validate_generic_command(&request)?;
                let updated = self.refresh_state()?;
                Ok(Value::Map(BTreeMap::from([
                    ("command".into(), Value::String(request.command)),
                    ("commands".into(), Value::I64(1)),
                    ("state_updated".into(), Value::Bool(updated)),
                    ("state".into(), self.state_summary()),
                    (
                        "completion_basis".into(),
                        Value::String("OpenUC2 mapped /state_get readback".into()),
                    ),
                ])))
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported OpenUC2 invocation capability",
            )),
        }
    }
}

impl Driver for OpenUc2Driver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        self.descriptors_for()
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "openuc2-serial".into(),
            kind: "serial.json-lines".into(),
            metadata: BTreeMap::from([
                ("send_ending".into(), Value::String("lf".into())),
                ("recv_ending".into(), Value::String("cr".into())),
                ("baud_rate".into(), Value::I64(self.baud_rate as i64)),
                ("connected".into(), Value::Bool(self.connected)),
                (
                    "serial_port".into(),
                    self.serial_port
                        .as_ref()
                        .map(|port| Value::String(port.clone()))
                        .unwrap_or(Value::Null),
                ),
                (
                    "detection_command".into(),
                    Value::String(protocol::encode(&protocol::OpenUc2Command::StateGet)),
                ),
            ]),
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        match device {
            device if device == self.hub => {
                vec![capability(4, device, CapabilityKind::GenericCommand)]
            }
            device if device == self.xy || device == self.z => {
                vec![capability(1, device, CapabilityKind::StageMove)]
            }
            device if device == self.laser => vec![
                capability(2, device, CapabilityKind::TriggerSink),
                capability(3, device, CapabilityKind::Dac),
            ],
            _ => Vec::new(),
        }
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut physical_transactions = Vec::new();
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    let _ = self.read_property(*device, key)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("openuc2 read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("openuc2 write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "openuc2 remultiplexed motor/laser state set".into(),
                        payload: Value::List(
                            set.writes
                                .iter()
                                .map(|write| {
                                    Value::Map(BTreeMap::from([
                                        ("device".into(), Value::I64((write.device.0).0 as i64)),
                                        ("property".into(), Value::String(write.property.clone())),
                                        ("value".into(), write.value.clone()),
                                    ]))
                                })
                                .collect(),
                        ),
                    });
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
                            "unknown OpenUC2 capability",
                        ));
                    };
                    if !capability.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "OpenUC2 {:?} expects {:?}, got {:?}",
                                capability.kind,
                                capability.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    for command in self.invoke_transactions(*device, capability.kind, request)? {
                        physical_transactions
                            .push(self.timing_transaction("openuc2 direct invocation", command));
                    }
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
                    if device == self.hub && key == "state_summary" {
                        self.refresh_state()?;
                    }
                    last = self.read_property(device, &key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    last = self.apply_write(device, &key, &value)?;
                    self.flush_physical_state()?;
                    self.emit_property(device, &key, last.clone());
                }
                Command::ApplyStateSet(set) => {
                    let mut result = BTreeMap::new();
                    for write in set.writes {
                        let value =
                            self.apply_write(write.device, &write.property, &write.value)?;
                        self.emit_property(write.device, &write.property, value.clone());
                        result.insert(format!("{}:{}", (write.device.0).0, write.property), value);
                    }
                    self.flush_physical_state()?;
                    last = Value::Map(result);
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
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "unknown OpenUC2 capability",
                        ));
                    };
                    if !capability.accepts_request(&request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "OpenUC2 {:?} expects {:?}, got {:?}",
                                capability.kind,
                                capability.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    last = self.apply_invoke(device, capability.kind, request)?;
                }
                Command::Arm(_) | Command::Start(_) | Command::Stop(_) => {
                    unreachable!()
                }
            }
        }
        self.pending
            .push_back(DriverEvent::TokenCompleted { token, value: last });
        Ok(token)
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        if let Err(error) = self.drain_serial_state() {
            self.pending
                .push_back(DriverEvent::Event(Event::Fault(FaultEvent {
                    device: Some(self.hub),
                    report: error.into(),
                })));
        }
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
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "openuc2 timing arm summary".into(),
                payload: self.timing_summary(plan, "arm"),
            }],
        })
    }

    fn start_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let applied = self.apply_timing_sequence_step(&armed.plan, true)?;
        let mut physical_transactions = Vec::new();
        physical_transactions.push(PhysicalTransaction {
            resource: Some(self.resource),
            description: "openuc2 timing start remultiplexed state flush".into(),
            payload: self.state_summary(),
        });
        physical_transactions.push(PhysicalTransaction {
            resource: Some(self.resource),
            description: "openuc2 timing start summary".into(),
            payload: with_applied(self.timing_summary(&armed.plan, "start"), applied),
        });
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Start(OperationId(0))],
            physical_transactions,
        })
    }

    fn stop_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let applied = self.apply_timing_sequence_step(&armed.plan, false)?;
        let mut physical_transactions = Vec::new();
        physical_transactions.push(PhysicalTransaction {
            resource: Some(self.resource),
            description: "openuc2 timing stop remultiplexed state flush".into(),
            payload: self.state_summary(),
        });
        physical_transactions.push(PhysicalTransaction {
            resource: Some(self.resource),
            description: "openuc2 timing stop summary".into(),
            payload: with_applied(self.timing_summary(&armed.plan, "stop"), applied),
        });
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions,
        })
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
        sequenceable: false,
        hardware_address: None,
    }
}

fn sequenceable_property(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    unit: Option<&str>,
    writable: bool,
    range: Option<Range>,
) -> PropertySchema {
    let mut schema = property(key, display_name, value_type, unit, writable, range);
    schema.sequenceable = writable;
    schema
}

fn sequenceable_position_property(
    key: &str,
    display_name: &str,
    writable: bool,
    max_um: f64,
) -> PropertySchema {
    sequenceable_property(
        key,
        display_name,
        ValueType::Position,
        Some("um"),
        writable,
        Some(Range {
            min: position(0.0),
            max: position(max_um),
        }),
    )
}

fn with_applied(summary: Value, applied: Value) -> Value {
    match summary {
        Value::Map(mut map) => {
            map.insert("applied".into(), applied);
            Value::Map(map)
        }
        other => other,
    }
}

fn validate_generic_command(request: &GenericCommandRequest) -> Result<()> {
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
            "OpenUC2 GenericCommand does not take parameters",
        ));
    }
    match request.command.as_str() {
        "refresh_state" => Ok(()),
        other => Err(Error::new(
            ErrorCode::Unsupported,
            format!("OpenUC2 GenericCommand supports refresh_state; got {other}"),
        )),
    }
}

fn position(value_um: f64) -> Value {
    Value::Position(Position::from_micrometers(value_um))
}

fn position_um(value: &Value) -> Result<f64> {
    match value {
        Value::Position(position) => Ok(position.micrometers()),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            "expected typed position value",
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

fn u32_prop(device: &DeviceConfig, key: &str) -> Option<u32> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => u32::try_from(*value).ok(),
        _ => None,
    }
}

fn u64_prop(device: &DeviceConfig, key: &str) -> Option<u64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => u64::try_from(*value).ok(),
        _ => None,
    }
}

fn f64_prop(device: &DeviceConfig, key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        Some(Value::Position(value)) => Some(value.micrometers()),
        _ => None,
    }
}

fn position_config_um(device: &DeviceConfig, key: &str, legacy_key: &str) -> Option<f64> {
    f64_prop(device, key).or_else(|| f64_prop(device, legacy_key))
}

fn wavelength_prop(device: &DeviceConfig, key: &str) -> Option<Wavelength> {
    match device.properties.get(key) {
        Some(Value::Wavelength(value)) => Some(*value),
        Some(Value::F64(value)) => Some(Wavelength::from_nanometers(*value)),
        Some(Value::I64(value)) => Some(Wavelength::from_nanometers(*value as f64)),
        _ => None,
    }
}

fn stage_move_speed(request: &StageMoveRequest) -> u32 {
    request
        .profile
        .as_ref()
        .and_then(|profile| profile.velocity)
        .map(|velocity| velocity.micrometers_per_second().round().max(1.0) as u32)
        .unwrap_or(5000)
}

fn dac_request_percent(request: &CapabilityRequest) -> Result<f64> {
    match request {
        CapabilityRequest::Dac(request) => percent_value(&request.value),
        _ => Err(Error::new(
            ErrorCode::Unsupported,
            "OpenUC2 Dac expects CapabilityRequest::Dac",
        )),
    }
}

fn percent_value(value: &Value) -> Result<f64> {
    match value {
        Value::Ratio(percent) => Ok(percent.percent().clamp(0.0, 100.0)),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            "OpenUC2 percent value must be Ratio",
        )),
    }
}

fn laser_wire_value(enabled: bool, power_percent: f64) -> u8 {
    if enabled {
        (power_percent.clamp(0.0, 100.0) * 2.55)
            .round()
            .clamp(1.0, 255.0) as u8
    } else {
        0
    }
}

fn trigger_sink_actions(request: &CapabilityRequest) -> Result<Vec<bool>> {
    let action = match request {
        CapabilityRequest::None => TriggerSinkAction::Pulse,
        CapabilityRequest::Trigger(request) => match request.action {
            numanager_core::TriggerAction::Enable => TriggerSinkAction::Enable,
            numanager_core::TriggerAction::Disable => TriggerSinkAction::Disable,
            numanager_core::TriggerAction::Pulse => TriggerSinkAction::Pulse,
        },
        _ => {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "OpenUC2 TriggerSink expects None or CapabilityRequest::Trigger",
            ))
        }
    };
    Ok(match action {
        TriggerSinkAction::Enable => vec![true],
        TriggerSinkAction::Disable => vec![false],
        TriggerSinkAction::Pulse => vec![true, false],
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TriggerSinkAction {
    Enable,
    Disable,
    Pulse,
}
