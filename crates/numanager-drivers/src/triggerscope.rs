use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::{LineEnding, SerialIo, SerialLineCodec};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const BAUD: u32 = 115_200;
    pub const LINE_ENDING: &str = "\n";
    pub const SOFTWARE_VERSION: &str = "v1.6.5, 8/16/16";

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum TriggerScopeCommand {
        Identify,
        Ttl {
            channel: u8,
            high: bool,
        },
        Cam {
            channel: u8,
            high: bool,
        },
        Dac {
            channel: u8,
            counts: u32,
        },
        Focus {
            counts: u32,
        },
        Arm,
        ClearTtl {
            channel: u8,
        },
        ProgramTtl {
            index: u16,
            channel: u8,
            value: u8,
        },
        ClearDac {
            channel: u8,
        },
        ProgramDac {
            index: u16,
            channel: u8,
            counts: u32,
        },
        ClearFocus,
        ProgramFocus {
            start: u32,
            step: u32,
            transitions: u16,
            direction: u8,
            slave: u8,
        },
    }

    pub fn encode(command: TriggerScopeCommand) -> String {
        match command {
            TriggerScopeCommand::Identify => "*".into(),
            TriggerScopeCommand::Ttl { channel, high } => {
                format!("TTL{channel},{}", u8::from(high))
            }
            TriggerScopeCommand::Cam { channel, high } => {
                format!("CAM{channel},{}", u8::from(high))
            }
            TriggerScopeCommand::Dac { channel, counts } => format!("DAC{channel},{counts}"),
            TriggerScopeCommand::Focus { counts } => format!("FOCUS,{counts}"),
            TriggerScopeCommand::Arm => "ARM".into(),
            TriggerScopeCommand::ClearTtl { channel } => format!("CLEAR_TTL,{channel}"),
            TriggerScopeCommand::ProgramTtl {
                index,
                channel,
                value,
            } => format!("PROG_TTL,{index},{channel},{value}"),
            TriggerScopeCommand::ClearDac { channel } => format!("CLEAR_DAC,{channel}"),
            TriggerScopeCommand::ProgramDac {
                index,
                channel,
                counts,
            } => format!("PROG_DAC,{index},{channel},{counts}"),
            TriggerScopeCommand::ClearFocus => "CLEAR_FOCUS".into(),
            TriggerScopeCommand::ProgramFocus {
                start,
                step,
                transitions,
                direction,
                slave,
            } => format!("PROG_FOCUS,{start},{step},{transitions},{direction},{slave}"),
        }
    }

    pub fn encode_line(command: TriggerScopeCommand) -> Vec<u8> {
        format!("{}{}", encode(command), LINE_ENDING).into_bytes()
    }

    pub fn dac_counts(voltage: Voltage, bits: u8) -> Result<u32> {
        let volts = voltage.volts();
        if !(0.0..=10.0).contains(&volts) || !volts.is_finite() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "TriggerScope DAC voltage must be in 0..=10 V",
            ));
        }
        let max = if bits >= 16 { 65_535.0 } else { 4_095.0 };
        Ok((volts * max / 10.0).round() as u32)
    }

    pub fn focus_counts(
        position: Position,
        lower: Position,
        upper: Position,
        bits: u8,
    ) -> Result<u32> {
        let lower_um = lower.micrometers();
        let upper_um = upper.micrometers();
        let pos_um = position.micrometers();
        if upper_um <= lower_um {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "TriggerScope focus upper limit must be above lower limit",
            ));
        }
        if !(lower_um..=upper_um).contains(&pos_um) || !pos_um.is_finite() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "TriggerScope focus position is outside configured limits",
            ));
        }
        let max = if bits >= 16 { 65_535.0 } else { 4_095.0 };
        Ok(((pos_um - lower_um) * max / (upper_um - lower_um)).round() as u32)
    }
}

#[derive(Debug, Clone)]
pub struct TriggerScopeConfiguredProbe {
    label: String,
    serial_port: Option<String>,
    serial_timeout_ms: u64,
    connect_real_transport: bool,
    product: String,
    serial_number: String,
    firmware_version: String,
    dac_bits: u8,
    ttl_count: usize,
    dac_count: usize,
    cam_count: usize,
    ttl_states: Vec<bool>,
    cam_states: Vec<bool>,
    dac_voltages: Vec<Voltage>,
    dac_enabled: Vec<bool>,
    focus: Position,
    focus_lower: Position,
    focus_upper: Position,
}

pub struct TriggerScopeDiscovery {
    next_id: DriverId,
    probes: Vec<TriggerScopeConfiguredProbe>,
}

impl TriggerScopeDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![TriggerScopeConfiguredProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), "triggerscope" | "trigger_scope"))
            .map(TriggerScopeConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for TriggerScopeDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver: Box<dyn Driver> = if configured.connect_real_transport {
                    Box::new(TriggerScopeDriver::serial(id, configured)?)
                } else {
                    Box::new(TriggerScopeDriver::configured(id, configured))
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

impl TriggerScopeConfiguredProbe {
    pub fn fixture() -> Self {
        let ttl_count = 4;
        let dac_count = 4;
        let cam_count = 2;
        Self {
            label: "Configured TriggerScope controller".into(),
            serial_port: None,
            serial_timeout_ms: 500,
            connect_real_transport: false,
            product: "ARC TriggerScope 16".into(),
            serial_number: "TRIGGERSCOPE-CONFIG-0001".into(),
            firmware_version: "ARC TRIGGERSCOPE 16 v1.65".into(),
            dac_bits: 16,
            ttl_count,
            dac_count,
            cam_count,
            ttl_states: vec![false; ttl_count],
            cam_states: vec![false; cam_count],
            dac_voltages: vec![Voltage::from_volts(0.0); dac_count],
            dac_enabled: vec![false; dac_count],
            focus: Position::from_micrometers(0.0),
            focus_lower: Position::from_micrometers(0.0),
            focus_upper: Position::from_micrometers(1000.0),
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::fixture();
        if !device.label.is_empty() {
            configured.label = device.label.clone();
        }
        configured.serial_port = string_prop(device, "serial_port")?;
        configured.serial_timeout_ms =
            u64_prop(device, "serial_timeout_ms")?.unwrap_or(configured.serial_timeout_ms);
        configured.connect_real_transport =
            bool_prop(device, "connect")?.unwrap_or(configured.connect_real_transport);
        configured.product = string_prop(device, "product")?.unwrap_or(configured.product);
        configured.serial_number =
            string_prop(device, "serial_number")?.unwrap_or(configured.serial_number);
        configured.firmware_version =
            string_prop(device, "firmware_version")?.unwrap_or(configured.firmware_version);
        configured.dac_bits = u8_prop(device, "dac_bits")?.unwrap_or(configured.dac_bits);
        configured.ttl_count = usize_prop(device, "ttl_count")?.unwrap_or(configured.ttl_count);
        configured.dac_count = usize_prop(device, "dac_count")?.unwrap_or(configured.dac_count);
        configured.cam_count = usize_prop(device, "cam_count")?.unwrap_or(configured.cam_count);
        if !(1..=16).contains(&configured.ttl_count)
            || !(1..=16).contains(&configured.dac_count)
            || !(0..=2).contains(&configured.cam_count)
            || !matches!(configured.dac_bits, 12 | 16)
        {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "TriggerScope counts must use ttl/dac 1..=16, cam 0..=2, dac_bits 12 or 16",
            ));
        }
        configured.ttl_states.resize(configured.ttl_count, false);
        configured.cam_states.resize(configured.cam_count, false);
        configured
            .dac_voltages
            .resize(configured.dac_count, Voltage::from_volts(0.0));
        configured.dac_enabled.resize(configured.dac_count, false);
        configured.focus = position_prop(device, "focus")?.unwrap_or(configured.focus);
        configured.focus_lower =
            position_prop(device, "focus_lower")?.unwrap_or(configured.focus_lower);
        configured.focus_upper =
            position_prop(device, "focus_upper")?.unwrap_or(configured.focus_upper);
        for index in 0..configured.ttl_count {
            let key = format!("ttl_{}_high", index + 1);
            configured.ttl_states[index] = bool_prop(device, &key)?.unwrap_or(false);
        }
        for index in 0..configured.cam_count {
            let key = format!("cam_{}_high", index + 1);
            configured.cam_states[index] = bool_prop(device, &key)?.unwrap_or(false);
        }
        for index in 0..configured.dac_count {
            let key = format!("dac_{}_voltage", index + 1);
            configured.dac_voltages[index] =
                voltage_prop(device, &key)?.unwrap_or(Voltage::from_volts(0.0));
            let key = format!("dac_{}_enabled", index + 1);
            configured.dac_enabled[index] = bool_prop(device, &key)?.unwrap_or(false);
        }
        Ok(configured)
    }
}

pub struct TriggerScopeDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    focus: DeviceId,
    cams: Vec<DeviceId>,
    ttls: Vec<DeviceId>,
    dacs: Vec<DeviceId>,
    configured: TriggerScopeConfiguredProbe,
    last_transaction: Value,
    armed: bool,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Option<Box<dyn SerialIo>>,
    codec: SerialLineCodec,
}

impl TriggerScopeDriver {
    pub fn configured(id: DriverId, configured: TriggerScopeConfiguredProbe) -> Self {
        Self::new(id, configured, None)
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: TriggerScopeConfiguredProbe) -> Result<Self> {
        let port_name = configured.serial_port.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "TriggerScope config requires serial_port when connect is true",
            )
        })?;
        let serial = Box::new(numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(port_name, protocol::BAUD)
                .timeout(Duration::from_millis(configured.serial_timeout_ms)),
        )?);
        let mut driver = Self::new(id, configured, Some(serial));
        let reply = driver.send(protocol::TriggerScopeCommand::Identify, "identify")?;
        if !reply.trim().is_empty() {
            driver.configured.firmware_version = reply.trim().into();
        }
        Ok(driver)
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _configured: TriggerScopeConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "TriggerScope real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(
        id: DriverId,
        configured: TriggerScopeConfiguredProbe,
        serial: Option<Box<dyn SerialIo>>,
    ) -> Self {
        let base = id.0 * 1000 + 950;
        Self {
            id,
            resource: ResourceId(NodeId(base)),
            hub: DeviceId(NodeId(base + 1)),
            focus: DeviceId(NodeId(base + 2)),
            cams: (0..configured.cam_count)
                .map(|index| DeviceId(NodeId(base + 10 + index as u64)))
                .collect(),
            ttls: (0..configured.ttl_count)
                .map(|index| DeviceId(NodeId(base + 100 + index as u64)))
                .collect(),
            dacs: (0..configured.dac_count)
                .map(|index| DeviceId(NodeId(base + 200 + index as u64)))
                .collect(),
            configured,
            last_transaction: Value::Map(BTreeMap::new()),
            armed: false,
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            codec: SerialLineCodec::new(LineEnding::Lf, LineEnding::Lf),
        }
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn send(&mut self, command: protocol::TriggerScopeCommand, action: &str) -> Result<String> {
        let bytes = protocol::encode_line(command);
        let mut reply = String::new();
        let completion_basis = if self.serial.is_some() {
            self.active_serial()?.write(&bytes)?;
            reply = self.read_line_until_timeout()?;
            "serial write and line readback"
        } else {
            "configured update; no live serial connection"
        };
        let mut transaction = BTreeMap::from([
            ("action".into(), Value::String(action.into())),
            (
                "completion_basis".into(),
                Value::String(completion_basis.into()),
            ),
            (
                "encoded_length".into(),
                Value::ByteCount(ByteCount::new(bytes.len() as u64)),
            ),
        ]);
        if self.serial.is_some() {
            transaction.insert("live_serial".into(), Value::Bool(true));
            transaction.insert("reply".into(), Value::String(reply.clone()));
        }
        self.last_transaction = Value::Map(transaction);
        Ok(reply)
    }

    fn active_serial(&mut self) -> Result<&mut (dyn SerialIo + 'static)> {
        self.serial.as_deref_mut().ok_or_else(|| {
            Error::new(
                ErrorCode::Unsupported,
                "TriggerScope active serial is not connected",
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

    fn cam_index(&self, device: DeviceId) -> Option<usize> {
        self.cams.iter().position(|id| *id == device)
    }

    fn ttl_index(&self, device: DeviceId) -> Option<usize> {
        self.ttls.iter().position(|id| *id == device)
    }

    fn dac_index(&self, device: DeviceId) -> Option<usize> {
        self.dacs.iter().position(|id| *id == device)
    }

    fn owns_device(&self, device: DeviceId) -> bool {
        device == self.hub
            || device == self.focus
            || self.cam_index(device).is_some()
            || self.ttl_index(device).is_some()
            || self.dac_index(device).is_some()
    }

    fn channel(index: usize) -> u8 {
        index as u8 + 1
    }

    fn write_ttl(&mut self, index: usize, high: bool) -> Result<Value> {
        self.send(
            protocol::TriggerScopeCommand::Ttl {
                channel: Self::channel(index),
                high,
            },
            "set_ttl",
        )?;
        self.configured.ttl_states[index] = high;
        self.emit_property(self.ttls[index], "high", Value::Bool(high));
        Ok(Value::Bool(high))
    }

    fn write_cam(&mut self, index: usize, high: bool) -> Result<Value> {
        self.send(
            protocol::TriggerScopeCommand::Cam {
                channel: Self::channel(index),
                high,
            },
            "set_camera_trigger",
        )?;
        self.configured.cam_states[index] = high;
        self.emit_property(self.cams[index], "high", Value::Bool(high));
        Ok(Value::Bool(high))
    }

    fn write_dac(&mut self, index: usize, voltage: Voltage) -> Result<Value> {
        let counts = protocol::dac_counts(voltage, self.configured.dac_bits)?;
        self.send(
            protocol::TriggerScopeCommand::Dac {
                channel: Self::channel(index),
                counts,
            },
            "set_dac",
        )?;
        self.configured.dac_voltages[index] = voltage;
        self.configured.dac_enabled[index] = voltage.volts() > 0.0;
        self.emit_property(self.dacs[index], "voltage", Value::Voltage(voltage));
        self.emit_property(
            self.dacs[index],
            "enabled",
            Value::Bool(self.configured.dac_enabled[index]),
        );
        Ok(Value::Voltage(voltage))
    }

    fn set_dac_enabled(&mut self, index: usize, enabled: bool) -> Result<Value> {
        let output = if enabled {
            self.configured.dac_voltages[index]
        } else {
            Voltage::from_volts(0.0)
        };
        let counts = protocol::dac_counts(output, self.configured.dac_bits)?;
        self.send(
            protocol::TriggerScopeCommand::Dac {
                channel: Self::channel(index),
                counts,
            },
            "set_dac_enabled",
        )?;
        self.configured.dac_enabled[index] = enabled;
        self.emit_property(self.dacs[index], "enabled", Value::Bool(enabled));
        Ok(Value::Bool(enabled))
    }

    fn write_focus(&mut self, position: Position) -> Result<Value> {
        let counts = protocol::focus_counts(
            position,
            self.configured.focus_lower,
            self.configured.focus_upper,
            self.configured.dac_bits,
        )?;
        self.send(protocol::TriggerScopeCommand::Focus { counts }, "set_focus")?;
        self.configured.focus = position;
        self.emit_property(self.focus, "z", Value::Position(position));
        Ok(Value::Position(position))
    }

    fn apply_stage_move(&mut self, request: StageMoveRequest) -> Result<Value> {
        let Some(target) = request.target.get(&StageAxis::Z) else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "TriggerScope focus StageMove requires a Z target",
            ));
        };
        let position = if request.relative {
            Position::from_micrometers(self.configured.focus.micrometers() + target.micrometers())
        } else {
            *target
        };
        self.write_focus(position)
    }

    fn read_property(&mut self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "product" => Ok(Value::String(self.configured.product.clone())),
                "serial_number" => Ok(Value::String(self.configured.serial_number.clone())),
                "firmware_version" => Ok(Value::String(self.configured.firmware_version.clone())),
                "software_version" => Ok(Value::String(protocol::SOFTWARE_VERSION.into())),
                "dac_bits" => Ok(Value::I64(self.configured.dac_bits as i64)),
                "serial_port" => Ok(Value::String(
                    self.configured.serial_port.clone().unwrap_or_default(),
                )),
                "connected" => Ok(Value::Bool(self.serial.is_some())),
                "serial_timeout" => Ok(Value::TimeInterval(TimeInterval::from_milliseconds(
                    self.configured.serial_timeout_ms as f64,
                ))),
                "armed" => Ok(Value::Bool(self.armed)),
                "last_transaction" => Ok(self.last_transaction.clone()),
                _ => invalid_property("unknown TriggerScope hub property", key),
            };
        }
        if device == self.focus {
            return match key {
                "z" => Ok(Value::Position(self.configured.focus)),
                "z_lower" => Ok(Value::Position(self.configured.focus_lower)),
                "z_upper" => Ok(Value::Position(self.configured.focus_upper)),
                _ => invalid_property("unknown TriggerScope focus property", key),
            };
        }
        if let Some(index) = self.ttl_index(device) {
            return match key {
                "high" => Ok(Value::Bool(self.configured.ttl_states[index])),
                "channel" => Ok(Value::I64(Self::channel(index) as i64)),
                _ => invalid_property("unknown TriggerScope TTL property", key),
            };
        }
        if let Some(index) = self.cam_index(device) {
            return match key {
                "high" => Ok(Value::Bool(self.configured.cam_states[index])),
                "channel" => Ok(Value::I64(Self::channel(index) as i64)),
                _ => invalid_property("unknown TriggerScope camera-trigger property", key),
            };
        }
        if let Some(index) = self.dac_index(device) {
            return match key {
                "voltage" => Ok(Value::Voltage(self.configured.dac_voltages[index])),
                "enabled" => Ok(Value::Bool(self.configured.dac_enabled[index])),
                "channel" => Ok(Value::I64(Self::channel(index) as i64)),
                _ => invalid_property("unknown TriggerScope DAC property", key),
            };
        }
        invalid_property("unknown TriggerScope device property", key)
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        match (device, key, value) {
            (device, "z", Value::Position(position)) if device == self.focus => {
                protocol::focus_counts(
                    *position,
                    self.configured.focus_lower,
                    self.configured.focus_upper,
                    self.configured.dac_bits,
                )
                .map(|_| ())
            }
            (device, "high", Value::Bool(_)) if self.ttl_index(device).is_some() => Ok(()),
            (device, "high", Value::Bool(_)) if self.cam_index(device).is_some() => Ok(()),
            (device, "voltage", Value::Voltage(voltage)) if self.dac_index(device).is_some() => {
                protocol::dac_counts(*voltage, self.configured.dac_bits).map(|_| ())
            }
            (device, "enabled", Value::Bool(_)) if self.dac_index(device).is_some() => Ok(()),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("TriggerScope property {key} is read-only or wrong type"),
            )),
        }
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: Value) -> Result<Value> {
        self.validate_write(device, key, &value)?;
        match (device, key, value) {
            (device, "z", Value::Position(position)) if device == self.focus => {
                self.write_focus(position)
            }
            (device, "high", Value::Bool(high)) => {
                if let Some(index) = self.ttl_index(device) {
                    self.write_ttl(index, high)
                } else {
                    let index = self.cam_index(device).expect("validated cam");
                    self.write_cam(index, high)
                }
            }
            (device, "voltage", Value::Voltage(voltage)) => {
                let index = self.dac_index(device).expect("validated dac");
                self.write_dac(index, voltage)
            }
            (device, "enabled", Value::Bool(enabled)) => {
                let index = self.dac_index(device).expect("validated dac");
                self.set_dac_enabled(index, enabled)
            }
            _ => unreachable!("validated TriggerScope write"),
        }
    }
}

impl Driver for TriggerScopeDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "triggerscope-serial".into(),
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
                ("line_ending".into(), Value::String("LF".into())),
                (
                    "completion".into(),
                    Value::String("configured-state update or active serial line readback".into()),
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
                label: "triggerscope-hub".into(),
                vendor: Some("Advanced Research Consulting".into()),
                model: Some(self.configured.product.clone()),
                serial: Some(self.configured.serial_number.clone()),
                kinds: vec![
                    "hub".into(),
                    "trigger.controller".into(),
                    "serial.ascii".into(),
                ],
                properties: vec![
                    string_property("product", "Product", false),
                    string_property("serial_number", "Serial number", false),
                    string_property("firmware_version", "Firmware version", false),
                    string_property("software_version", "Software version", false),
                    integer_property("dac_bits", "DAC bits", false),
                    string_property("serial_port", "Serial port", false),
                    bool_property("connected", "Connected", false),
                    time_property("serial_timeout", "Serial timeout", false),
                    bool_property("armed", "Armed", false),
                    map_property("last_transaction", "Last transaction", false),
                ],
                metadata: source_metadata(),
            },
            DeviceDescriptor {
                id: self.focus,
                driver: self.id,
                label: "triggerscope-focus".into(),
                vendor: Some("Advanced Research Consulting".into()),
                model: Some(self.configured.product.clone()),
                serial: Some(format!("{}:focus", self.configured.serial_number)),
                kinds: vec!["axis.z".into(), "stage.z".into(), "motion.stage".into()],
                properties: vec![
                    position_range_property(
                        "z",
                        "Z",
                        true,
                        self.configured.focus_lower,
                        self.configured.focus_upper,
                    ),
                    position_property("z_lower", "Z lower", false),
                    position_property("z_upper", "Z upper", false),
                ],
                metadata: BTreeMap::from([
                    (
                        "z_lower".into(),
                        Value::Position(self.configured.focus_lower),
                    ),
                    (
                        "z_upper".into(),
                        Value::Position(self.configured.focus_upper),
                    ),
                ]),
            },
        ];
        for (index, device) in self.cams.iter().enumerate() {
            descriptors.push(DeviceDescriptor {
                id: *device,
                driver: self.id,
                label: format!("triggerscope-cam-{}", index + 1),
                vendor: Some("Advanced Research Consulting".into()),
                model: Some(self.configured.product.clone()),
                serial: Some(format!(
                    "{}:cam-{}",
                    self.configured.serial_number,
                    index + 1
                )),
                kinds: vec![
                    "camera.trigger".into(),
                    "trigger.source".into(),
                    "state.device".into(),
                ],
                properties: vec![
                    non_sequenceable_bool_property("high", "High", true),
                    integer_property("channel", "Channel", false),
                ],
                metadata: BTreeMap::new(),
            });
        }
        for (index, device) in self.ttls.iter().enumerate() {
            descriptors.push(DeviceDescriptor {
                id: *device,
                driver: self.id,
                label: format!("triggerscope-ttl-{}", index + 1),
                vendor: Some("Advanced Research Consulting".into()),
                model: Some(self.configured.product.clone()),
                serial: Some(format!(
                    "{}:ttl-{}",
                    self.configured.serial_number,
                    index + 1
                )),
                kinds: vec![
                    "digital.output".into(),
                    "ttl.output".into(),
                    "trigger.source".into(),
                    "trigger.sink".into(),
                ],
                properties: vec![
                    bool_property("high", "High", true),
                    integer_property("channel", "Channel", false),
                ],
                metadata: BTreeMap::new(),
            });
        }
        for (index, device) in self.dacs.iter().enumerate() {
            descriptors.push(DeviceDescriptor {
                id: *device,
                driver: self.id,
                label: format!("triggerscope-dac-{}", index + 1),
                vendor: Some("Advanced Research Consulting".into()),
                model: Some(self.configured.product.clone()),
                serial: Some(format!(
                    "{}:dac-{}",
                    self.configured.serial_number,
                    index + 1
                )),
                kinds: vec![
                    "analog.output".into(),
                    "dac.output".into(),
                    "trigger.sink".into(),
                ],
                properties: vec![
                    voltage_property("voltage", "Voltage", true),
                    bool_property("enabled", "Enabled", true),
                    integer_property("channel", "Channel", false),
                ],
                metadata: BTreeMap::new(),
            });
        }
        descriptors
    }

    fn graph(&self) -> DeviceGraph {
        let mut graph = DeviceGraph::default();
        let _ = graph.insert_node(GraphNode {
            id: self.resource.0,
            kind: NodeKind::Resource,
            label: "triggerscope-serial".into(),
        });
        let _ = graph.insert_node(GraphNode {
            id: self.hub.0,
            kind: NodeKind::Hub,
            label: "triggerscope-hub".into(),
        });
        let _ = graph.insert_edge(GraphEdge {
            from: self.hub.0,
            to: self.resource.0,
            kind: EdgeKind::OwnsResource,
        });
        for device in self
            .descriptors()
            .into_iter()
            .filter(|descriptor| descriptor.id != self.hub)
        {
            let _ = graph.insert_node(GraphNode {
                id: device.id.0,
                kind: NodeKind::Device,
                label: device.label,
            });
            let _ = graph.insert_edge(GraphEdge {
                from: self.hub.0,
                to: device.id.0,
                kind: EdgeKind::OffersDevice,
            });
        }
        graph
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.hub {
            return vec![capability(
                8,
                device,
                CapabilityKind::GenericCommand,
                ValueType::Map,
            )];
        }
        if device == self.focus {
            return vec![capability(
                1,
                device,
                CapabilityKind::StageMove,
                ValueType::Position,
            )];
        }
        if self.ttl_index(device).is_some() {
            return vec![
                capability(2, device, CapabilityKind::DigitalIo, ValueType::Bool),
                capability(3, device, CapabilityKind::TriggerSink, ValueType::Bool),
                capability(4, device, CapabilityKind::TriggerSource, ValueType::Bool),
            ];
        }
        if self.cam_index(device).is_some() {
            return vec![capability(
                5,
                device,
                CapabilityKind::TriggerSource,
                ValueType::Bool,
            )];
        }
        if self.dac_index(device).is_some() {
            return vec![
                capability(6, device, CapabilityKind::Dac, ValueType::Voltage),
                capability(7, device, CapabilityKind::TriggerSink, ValueType::Bool),
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
                        format!("triggerscope read {key}"),
                        Value::String(key.clone()),
                    ));
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("triggerscope write {key}"),
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
                            "unknown TriggerScope capability",
                        ));
                    };
                    if !descriptor.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "TriggerScope {} request kind does not match",
                                descriptor.kind.name()
                            ),
                        ));
                    }
                    if let CapabilityRequest::StageMove(request) = request {
                        self.validate_stage_move(*device, request)?;
                    }
                    if let CapabilityRequest::GenericCommand(request) = request {
                        self.validate_generic_command(*device, request)?;
                    }
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("triggerscope {}", descriptor.kind.name()),
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
                        "triggerscope state set",
                        Value::I64(set.writes.len() as i64),
                    ));
                }
                Command::Arm(plan) => {
                    self.validate_timing_plan(plan)?;
                    physical_transactions.push(transaction(
                        self.resource,
                        "triggerscope timing arm",
                        self.timing_summary(plan, "arm"),
                    ));
                }
                Command::Start(_) => {
                    physical_transactions.push(transaction(
                        self.resource,
                        "triggerscope timing start",
                        Value::String("ARM".into()),
                    ));
                }
                Command::Stop(_) => {
                    physical_transactions.push(transaction(
                        self.resource,
                        "triggerscope timing stop",
                        Value::String("configured armed-state clear".into()),
                    ));
                }
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
                            self.apply_stage_move(request)?
                        }
                        (CapabilityKind::Dac, CapabilityRequest::Dac(request)) => {
                            let Value::Voltage(voltage) = request.value else {
                                return Err(Error::new(
                                    ErrorCode::InvalidCommand,
                                    "TriggerScope Dac requires Voltage value",
                                ));
                            };
                            let index = self.dac_index(device).expect("capability on dac");
                            self.write_dac(index, voltage)?
                        }
                        (CapabilityKind::DigitalIo, CapabilityRequest::DigitalIo(request)) => {
                            let Some(index) = self.ttl_index(device) else {
                                return Err(Error::new(
                                    ErrorCode::InvalidCommand,
                                    "TriggerScope DigitalIo requires a TTL device",
                                ));
                            };
                            self.write_ttl(index, request.mask & 1 != 0)?
                        }
                        (
                            CapabilityKind::TriggerSink | CapabilityKind::TriggerSource,
                            CapabilityRequest::Trigger(request),
                        ) => {
                            let high = !matches!(request.action, TriggerAction::Disable);
                            if let Some(index) = self.ttl_index(device) {
                                self.write_ttl(index, high)?
                            } else if let Some(index) = self.cam_index(device) {
                                self.write_cam(index, high)?
                            } else {
                                let index = self.dac_index(device).expect("capability on dac");
                                self.set_dac_enabled(index, high)?
                            }
                        }
                        (
                            CapabilityKind::GenericCommand,
                            CapabilityRequest::GenericCommand(request),
                        ) => self.apply_generic_command(device, request)?,
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported TriggerScope capability invocation",
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
                Command::Arm(plan) => {
                    last = self.program_timing_plan(&plan)?;
                }
                Command::Start(_) => {
                    self.send(protocol::TriggerScopeCommand::Arm, "timing_start")?;
                    self.armed = true;
                    self.emit_property(self.hub, "armed", Value::Bool(true));
                    last = Value::Map(BTreeMap::from([
                        ("action".into(), Value::String("start".into())),
                        ("armed".into(), Value::Bool(true)),
                    ]));
                }
                Command::Stop(_) => {
                    self.armed = false;
                    self.emit_property(self.hub, "armed", Value::Bool(false));
                    last = Value::Map(BTreeMap::from([
                        ("action".into(), Value::String("stop".into())),
                        ("armed".into(), Value::Bool(false)),
                    ]));
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
                "triggerscope timing arm",
                self.timing_summary(plan, "arm"),
            )],
        })
    }

    fn start_timing_plan(
        &mut self,
        _armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Start(OperationId(0))],
            physical_transactions: vec![transaction(
                self.resource,
                "triggerscope timing start",
                Value::String("ARM".into()),
            )],
        })
    }

    fn stop_timing_plan(
        &mut self,
        _armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions: vec![transaction(
                self.resource,
                "triggerscope timing stop",
                Value::String("configured armed-state clear".into()),
            )],
        })
    }
}

impl TriggerScopeDriver {
    fn validate_read(&self, device: DeviceId, key: &str) -> Result<()> {
        if device == self.hub
            && matches!(
                key,
                "product"
                    | "serial_number"
                    | "firmware_version"
                    | "software_version"
                    | "dac_bits"
                    | "serial_port"
                    | "connected"
                    | "serial_timeout"
                    | "armed"
                    | "last_transaction"
            )
        {
            return Ok(());
        }
        if device == self.focus && matches!(key, "z" | "z_lower" | "z_upper") {
            return Ok(());
        }
        if (self.ttl_index(device).is_some() || self.cam_index(device).is_some())
            && matches!(key, "high" | "channel")
        {
            return Ok(());
        }
        if self.dac_index(device).is_some() && matches!(key, "voltage" | "enabled" | "channel") {
            return Ok(());
        }
        Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unknown TriggerScope property {key}"),
        ))
    }

    fn validate_stage_move(&self, device: DeviceId, request: &StageMoveRequest) -> Result<()> {
        if device != self.focus {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "TriggerScope StageMove requires the focus device",
            ));
        }
        if request.target.keys().any(|axis| *axis != StageAxis::Z) {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "TriggerScope focus only accepts the Z axis",
            ));
        }
        let Some(target) = request.target.get(&StageAxis::Z) else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "TriggerScope focus StageMove requires a Z target",
            ));
        };
        let position = if request.relative {
            Position::from_micrometers(self.configured.focus.micrometers() + target.micrometers())
        } else {
            *target
        };
        protocol::focus_counts(
            position,
            self.configured.focus_lower,
            self.configured.focus_upper,
            self.configured.dac_bits,
        )
        .map(|_| ())
    }

    fn validate_generic_command(
        &self,
        device: DeviceId,
        request: &GenericCommandRequest,
    ) -> Result<()> {
        if request.is_hidden_maintenance() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "GenericCommand {} is a hidden maintenance operation",
                    request.command
                ),
            ));
        }
        if device != self.hub {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "TriggerScope GenericCommand requires the hub device",
            ));
        }
        match request.command.as_str() {
            "clear_ttl" => {
                let channel = command_channel(&request.params, self.configured.ttl_count)?;
                let _ = channel;
            }
            "program_ttl" => {
                let channel = command_channel(&request.params, self.configured.ttl_count)?;
                let index = command_u16(&request.params, "index")?;
                let value = command_ttl_value(&request.params)?;
                let _ = (channel, index, value);
            }
            "clear_dac" => {
                let channel = command_channel(&request.params, self.configured.dac_count)?;
                let _ = channel;
            }
            "program_dac" => {
                let channel = command_channel(&request.params, self.configured.dac_count)?;
                let index = command_u16(&request.params, "index")?;
                let counts = command_dac_counts(&request.params, self.configured.dac_bits)?;
                let _ = (channel, index, counts);
            }
            "clear_focus" => {}
            "program_focus" => {
                let start = command_counts(&request.params, "start", self.configured.dac_bits)?;
                let step = command_counts(&request.params, "step", self.configured.dac_bits)?;
                let transitions = command_u16(&request.params, "transitions")?;
                let direction = command_u8(&request.params, "direction")?;
                let slave = command_u8(&request.params, "slave")?;
                let _ = (start, step, transitions, direction, slave);
            }
            "arm" => {}
            other => {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    format!("unsupported TriggerScope command {other}"),
                ));
            }
        }
        Ok(())
    }

    fn apply_generic_command(
        &mut self,
        device: DeviceId,
        request: GenericCommandRequest,
    ) -> Result<Value> {
        self.validate_generic_command(device, &request)?;
        let command = request.command.clone();
        match request.command.as_str() {
            "clear_ttl" => {
                let channel = command_channel(&request.params, self.configured.ttl_count)?;
                self.send(
                    protocol::TriggerScopeCommand::ClearTtl { channel },
                    "clear_ttl_program",
                )?;
                Ok(command_result(
                    command,
                    [("channel", Value::I64(channel as i64))],
                ))
            }
            "program_ttl" => {
                let channel = command_channel(&request.params, self.configured.ttl_count)?;
                let index = command_u16(&request.params, "index")?;
                let value = command_ttl_value(&request.params)?;
                self.send(
                    protocol::TriggerScopeCommand::ProgramTtl {
                        index,
                        channel,
                        value,
                    },
                    "program_ttl",
                )?;
                Ok(command_result(
                    command,
                    [
                        ("channel", Value::I64(channel as i64)),
                        ("index", Value::I64(index as i64)),
                        ("value", Value::I64(value as i64)),
                    ],
                ))
            }
            "clear_dac" => {
                let channel = command_channel(&request.params, self.configured.dac_count)?;
                self.send(
                    protocol::TriggerScopeCommand::ClearDac { channel },
                    "clear_dac_program",
                )?;
                Ok(command_result(
                    command,
                    [("channel", Value::I64(channel as i64))],
                ))
            }
            "program_dac" => {
                let channel = command_channel(&request.params, self.configured.dac_count)?;
                let index = command_u16(&request.params, "index")?;
                let counts = command_dac_counts(&request.params, self.configured.dac_bits)?;
                self.send(
                    protocol::TriggerScopeCommand::ProgramDac {
                        index,
                        channel,
                        counts,
                    },
                    "program_dac",
                )?;
                Ok(command_result(
                    command,
                    [
                        ("channel", Value::I64(channel as i64)),
                        ("index", Value::I64(index as i64)),
                        ("counts", Value::I64(counts as i64)),
                    ],
                ))
            }
            "clear_focus" => {
                self.send(
                    protocol::TriggerScopeCommand::ClearFocus,
                    "clear_focus_program",
                )?;
                Ok(command_result(command, []))
            }
            "program_focus" => {
                let start = command_counts(&request.params, "start", self.configured.dac_bits)?;
                let step = command_counts(&request.params, "step", self.configured.dac_bits)?;
                let transitions = command_u16(&request.params, "transitions")?;
                let direction = command_u8(&request.params, "direction")?;
                let slave = command_u8(&request.params, "slave")?;
                self.send(
                    protocol::TriggerScopeCommand::ProgramFocus {
                        start,
                        step,
                        transitions,
                        direction,
                        slave,
                    },
                    "program_focus",
                )?;
                Ok(command_result(
                    command,
                    [
                        ("start", Value::I64(start as i64)),
                        ("step", Value::I64(step as i64)),
                        ("transitions", Value::I64(transitions as i64)),
                        ("direction", Value::I64(direction as i64)),
                        ("slave", Value::I64(slave as i64)),
                    ],
                ))
            }
            "arm" => {
                self.send(protocol::TriggerScopeCommand::Arm, "arm")?;
                self.armed = true;
                self.emit_property(self.hub, "armed", Value::Bool(true));
                Ok(command_result(command, [("armed", Value::Bool(true))]))
            }
            _ => unreachable!("validated TriggerScope generic command"),
        }
    }

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| self.owns_device(sequence.device))
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        if plan
            .routes
            .iter()
            .any(|route| self.owns_device(route.from) || self.owns_device(route.to))
        {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "TriggerScope timing routes have no evidenced route opcode",
            ));
        }
        match &plan.start {
            StartCondition::Software => {}
            StartCondition::ExternalTrigger(device) if !self.owns_device(*device) => {}
            StartCondition::ExternalTrigger(_) | StartCondition::At(_) => {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    "TriggerScope local external/absolute timing starts have no evidenced start opcode",
                ));
            }
        }
        for sequence in self.local_timing_sequences(plan) {
            self.validate_timing_sequence(sequence)?;
        }
        Ok(())
    }

    fn validate_timing_sequence(&self, sequence: &DeviceSequence) -> Result<()> {
        if sequence.values.len() > u16::MAX as usize + 1 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "TriggerScope timing sequence is too long for u16 array indices",
            ));
        }
        if self.ttl_index(sequence.device).is_some() && sequence.property == "high" {
            for value in &sequence.values {
                let Value::Bool(_) = value else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "TriggerScope TTL timing values must be Bool",
                    ));
                };
            }
            return Ok(());
        }
        if self.dac_index(sequence.device).is_some() && sequence.property == "voltage" {
            for value in &sequence.values {
                let Value::Voltage(voltage) = value else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "TriggerScope DAC timing values must be Voltage",
                    ));
                };
                protocol::dac_counts(*voltage, self.configured.dac_bits)?;
            }
            return Ok(());
        }
        if sequence.device == self.focus && sequence.property == "z" {
            if sequence.values.len() < 2 {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "TriggerScope focus timing requires at least two positions",
                ));
            }
            let mut counts = Vec::new();
            for value in &sequence.values {
                let Value::Position(position) = value else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "TriggerScope focus timing values must be Position",
                    ));
                };
                counts.push(protocol::focus_counts(
                    *position,
                    self.configured.focus_lower,
                    self.configured.focus_upper,
                    self.configured.dac_bits,
                )?);
            }
            let first_delta = counts[1] as i64 - counts[0] as i64;
            if first_delta == 0 {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "TriggerScope focus timing requires a non-zero step",
                ));
            }
            if counts
                .windows(2)
                .any(|pair| pair[1] as i64 - pair[0] as i64 != first_delta)
            {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "TriggerScope focus timing requires an evenly stepped sequence",
                ));
            }
            return Ok(());
        }
        Err(Error::new(
            ErrorCode::Unsupported,
            format!(
                "TriggerScope timing does not support property {} on device {}",
                sequence.property,
                (sequence.device.0).0
            ),
        ))
    }

    fn program_timing_plan(&mut self, plan: &TimingPlan) -> Result<Value> {
        self.validate_timing_plan(plan)?;
        let mut programmed = Vec::new();
        let sequences = self
            .local_timing_sequences(plan)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        for sequence in sequences {
            programmed.push(self.program_timing_sequence(&sequence)?);
        }
        self.armed = false;
        self.emit_property(self.hub, "armed", Value::Bool(false));
        Ok(Value::Map(BTreeMap::from([
            ("action".into(), Value::String("arm".into())),
            ("armed".into(), Value::Bool(false)),
            ("programmed".into(), Value::List(programmed)),
            (
                "sequence_count".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
        ])))
    }

    fn program_timing_sequence(&mut self, sequence: &DeviceSequence) -> Result<Value> {
        if let Some(index) = self.ttl_index(sequence.device) {
            let channel = Self::channel(index);
            self.send(
                protocol::TriggerScopeCommand::ClearTtl { channel },
                "timing_clear_ttl",
            )?;
            for (step, value) in sequence.values.iter().enumerate() {
                let Value::Bool(high) = value else {
                    unreachable!("validated TriggerScope TTL timing value")
                };
                self.send(
                    protocol::TriggerScopeCommand::ProgramTtl {
                        index: step as u16,
                        channel,
                        value: u8::from(*high),
                    },
                    "timing_program_ttl",
                )?;
            }
            return Ok(Value::Map(BTreeMap::from([
                ("kind".into(), Value::String("ttl".into())),
                ("channel".into(), Value::I64(channel as i64)),
                ("steps".into(), Value::I64(sequence.values.len() as i64)),
            ])));
        }
        if let Some(index) = self.dac_index(sequence.device) {
            let channel = Self::channel(index);
            self.send(
                protocol::TriggerScopeCommand::ClearDac { channel },
                "timing_clear_dac",
            )?;
            for (step, value) in sequence.values.iter().enumerate() {
                let Value::Voltage(voltage) = value else {
                    unreachable!("validated TriggerScope DAC timing value")
                };
                let counts = protocol::dac_counts(*voltage, self.configured.dac_bits)?;
                self.send(
                    protocol::TriggerScopeCommand::ProgramDac {
                        index: step as u16,
                        channel,
                        counts,
                    },
                    "timing_program_dac",
                )?;
            }
            return Ok(Value::Map(BTreeMap::from([
                ("kind".into(), Value::String("dac".into())),
                ("channel".into(), Value::I64(channel as i64)),
                ("steps".into(), Value::I64(sequence.values.len() as i64)),
            ])));
        }
        if sequence.device == self.focus {
            let counts = sequence
                .values
                .iter()
                .map(|value| {
                    let Value::Position(position) = value else {
                        unreachable!("validated TriggerScope focus timing value")
                    };
                    protocol::focus_counts(
                        *position,
                        self.configured.focus_lower,
                        self.configured.focus_upper,
                        self.configured.dac_bits,
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            let delta = counts[1] as i64 - counts[0] as i64;
            self.send(
                protocol::TriggerScopeCommand::ClearFocus,
                "timing_clear_focus",
            )?;
            self.send(
                protocol::TriggerScopeCommand::ProgramFocus {
                    start: counts[0],
                    step: delta.unsigned_abs() as u32,
                    transitions: (sequence.values.len() - 1) as u16,
                    direction: u8::from(delta > 0),
                    slave: 0,
                },
                "timing_program_focus",
            )?;
            return Ok(Value::Map(BTreeMap::from([
                ("kind".into(), Value::String("focus".into())),
                ("steps".into(), Value::I64(sequence.values.len() as i64)),
                ("start".into(), Value::I64(counts[0] as i64)),
                ("step".into(), Value::I64(delta.unsigned_abs() as i64)),
            ])));
        }
        unreachable!("validated TriggerScope timing sequence")
    }

    fn timing_summary(&self, plan: &TimingPlan, action: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("action".into(), Value::String(action.into())),
            (
                "local_sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
            ("routes".into(), Value::I64(plan.routes.len() as i64)),
            (
                "start".into(),
                Value::String(
                    match &plan.start {
                        StartCondition::Software => "software",
                        StartCondition::ExternalTrigger(_) => "external_trigger",
                        StartCondition::At(_) => "at",
                    }
                    .into(),
                ),
            ),
            (
                "stop".into(),
                Value::String(
                    match &plan.stop {
                        StopCondition::Manual => "manual",
                        StopCondition::Count(_) => "count",
                        StopCondition::Duration(_) => "duration",
                    }
                    .into(),
                ),
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

fn capability(
    id: u64,
    device: DeviceId,
    kind: CapabilityKind,
    response_type: ValueType,
) -> CapabilityDescriptor {
    CapabilityDescriptor::new(CapabilityId(id), device, kind, response_type)
}

fn source_metadata() -> BTreeMap<String, Value> {
    BTreeMap::from([
        (
            "evidence".into(),
            Value::String("reverse engineered serial command evidence".into()),
        ),
        (
            "support_scope".into(),
            Value::String("opt-in serial direct-control plus sequence-programming helpers".into()),
        ),
    ])
}

fn invalid_property<T>(prefix: &str, key: &str) -> Result<T> {
    Err(Error::new(
        ErrorCode::InvalidProperty,
        format!("{prefix} {key}"),
    ))
}

fn command_result<const N: usize>(command: String, fields: [(&str, Value); N]) -> Value {
    let mut map = BTreeMap::from([("command".into(), Value::String(command))]);
    for (key, value) in fields {
        map.insert(key.into(), value);
    }
    Value::Map(map)
}

fn command_channel(params: &BTreeMap<String, Value>, max_count: usize) -> Result<u8> {
    let channel = command_u8(params, "channel")?;
    if channel == 0 || channel as usize > max_count {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("TriggerScope channel must be in 1..={max_count}"),
        ));
    }
    Ok(channel)
}

fn command_ttl_value(params: &BTreeMap<String, Value>) -> Result<u8> {
    match params.get("value") {
        Some(Value::Bool(value)) => Ok(u8::from(*value)),
        Some(Value::I64(value)) if matches!(*value, 0 | 1) => Ok(*value as u8),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidCommand,
            "TriggerScope value must be Bool or 0/1 I64",
        )),
        None => Err(Error::new(
            ErrorCode::InvalidCommand,
            "TriggerScope command requires value",
        )),
    }
}

fn command_dac_counts(params: &BTreeMap<String, Value>, bits: u8) -> Result<u32> {
    if let Some(Value::Voltage(voltage)) = params.get("voltage") {
        return protocol::dac_counts(*voltage, bits);
    }
    command_counts(params, "counts", bits)
}

fn command_counts(params: &BTreeMap<String, Value>, key: &str, bits: u8) -> Result<u32> {
    let value = command_u32(params, key)?;
    let max = if bits >= 16 { 65_535 } else { 4_095 };
    if value > max {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("TriggerScope {key} must be in 0..={max}"),
        ));
    }
    Ok(value)
}

fn command_u8(params: &BTreeMap<String, Value>, key: &str) -> Result<u8> {
    u8::try_from(command_i64(params, key)?).map_err(|_| {
        Error::new(
            ErrorCode::InvalidCommand,
            format!("TriggerScope {key} must fit in an unsigned byte"),
        )
    })
}

fn command_u16(params: &BTreeMap<String, Value>, key: &str) -> Result<u16> {
    u16::try_from(command_i64(params, key)?).map_err(|_| {
        Error::new(
            ErrorCode::InvalidCommand,
            format!("TriggerScope {key} must fit in an unsigned 16-bit integer"),
        )
    })
}

fn command_u32(params: &BTreeMap<String, Value>, key: &str) -> Result<u32> {
    u32::try_from(command_i64(params, key)?).map_err(|_| {
        Error::new(
            ErrorCode::InvalidCommand,
            format!("TriggerScope {key} must fit in an unsigned 32-bit integer"),
        )
    })
}

fn command_i64(params: &BTreeMap<String, Value>, key: &str) -> Result<i64> {
    match params.get(key) {
        Some(Value::I64(value)) => Ok(*value),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("TriggerScope {key} must be I64"),
        )),
        None => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("TriggerScope command requires {key}"),
        )),
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
        sequenceable: matches!(key, "z" | "high" | "voltage" | "enabled"),
        hardware_address: None,
    }
}

fn string_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::String, None, writable, None)
}

fn map_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::Map, None, writable, None)
}

fn bool_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::Bool, None, writable, None)
}

fn non_sequenceable_bool_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    let mut schema = bool_property(key, display_name, writable);
    schema.sequenceable = false;
    schema
}

fn integer_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::I64, None, writable, None)
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

fn voltage_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Voltage,
        Some("V"),
        writable,
        Some(Range {
            min: Value::Voltage(Voltage::from_volts(0.0)),
            max: Value::Voltage(Voltage::from_volts(10.0)),
        }),
    )
}

fn position_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Position,
        Some("um"),
        writable,
        None,
    )
}

fn position_range_property(
    key: &str,
    display_name: &str,
    writable: bool,
    lower: Position,
    upper: Position,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Position,
        Some("um"),
        writable,
        Some(Range {
            min: Value::Position(lower),
            max: Value::Position(upper),
        }),
    )
}

fn string_prop(device: &DeviceConfig, key: &str) -> Result<Option<String>> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("TriggerScope property {key} must be String"),
        )),
        None => Ok(None),
    }
}

fn bool_prop(device: &DeviceConfig, key: &str) -> Result<Option<bool>> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("TriggerScope property {key} must be Bool"),
        )),
        None => Ok(None),
    }
}

fn u8_prop(device: &DeviceConfig, key: &str) -> Result<Option<u8>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => u8::try_from(*value).map(Some).map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("TriggerScope property {key} must fit in an unsigned byte"),
            )
        }),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("TriggerScope property {key} must be I64"),
        )),
        None => Ok(None),
    }
}

fn u64_prop(device: &DeviceConfig, key: &str) -> Result<Option<u64>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => u64::try_from(*value).map(Some).map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("TriggerScope property {key} must be a non-negative integer"),
            )
        }),
        Some(Value::TimeInterval(value))
            if value.seconds().is_finite() && value.seconds() >= 0.0 =>
        {
            Ok(Some((value.seconds() * 1000.0).round() as u64))
        }
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("TriggerScope property {key} must be non-negative I64 or TimeInterval"),
        )),
        None => Ok(None),
    }
}

fn usize_prop(device: &DeviceConfig, key: &str) -> Result<Option<usize>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => usize::try_from(*value).map(Some).map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("TriggerScope property {key} must be a non-negative count"),
            )
        }),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("TriggerScope property {key} must be I64"),
        )),
        None => Ok(None),
    }
}

fn position_prop(device: &DeviceConfig, key: &str) -> Result<Option<Position>> {
    match device.properties.get(key) {
        Some(Value::Position(value)) => Ok(Some(*value)),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("TriggerScope property {key} must be Position"),
        )),
        None => Ok(None),
    }
}

fn voltage_prop(device: &DeviceConfig, key: &str) -> Result<Option<Voltage>> {
    match device.properties.get(key) {
        Some(Value::Voltage(value)) => Ok(Some(*value)),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("TriggerScope property {key} must be Voltage"),
        )),
        None => Ok(None),
    }
}
