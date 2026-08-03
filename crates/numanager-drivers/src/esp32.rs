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

    pub const SEND_ENDING: LineEnding = LineEnding::CrLf;
    pub const RECV_ENDING: LineEnding = LineEnding::CrLf;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Axis {
        X,
        Y,
        Z,
    }

    impl Axis {
        pub fn index(self) -> u8 {
            match self {
                Axis::X => 0,
                Axis::Y => 1,
                Axis::Z => 2,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Esp32Probe {
        pub firmware: String,
        pub x_travel_um: f64,
        pub y_travel_um: f64,
        pub z_travel_um: f64,
        pub pwm_channels: u8,
    }

    impl Esp32Probe {
        pub fn simulated() -> Self {
            Self {
                firmware: "MM-ESP32,5".into(),
                x_travel_um: 75_000.0,
                y_travel_um: 50_000.0,
                z_travel_um: 8_000.0,
                pwm_channels: 5,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum Esp32Command {
        Version,
        Travel(Axis),
        Digital { channel: u8, high: bool },
        Pwm { channel: u8, duty_percent: f64 },
        ReadAnalog { channel: u8 },
        MoveXyAbs { x_um: f64, y_um: f64 },
        MoveZAbs { z_um: f64 },
        QueryPosition,
    }

    pub fn encode(command: &Esp32Command) -> String {
        match command {
            Esp32Command::Version => "V".into(),
            Esp32Command::Travel(axis) => format!("U,{}", axis.index()),
            Esp32Command::Digital { channel, high } => {
                format!("D,{channel},{}", u8::from(*high))
            }
            Esp32Command::Pwm {
                channel,
                duty_percent,
            } => {
                format!("P,{channel},{duty_percent:.3}")
            }
            Esp32Command::ReadAnalog { channel } => format!("A,{channel}"),
            Esp32Command::MoveXyAbs { x_um, y_um } => format!("M,0,{x_um:.3},1,{y_um:.3}"),
            Esp32Command::MoveZAbs { z_um } => format!("M,2,{z_um:.3}"),
            Esp32Command::QueryPosition => "W".into(),
        }
    }

    pub fn parse_version(reply: &str) -> Result<String> {
        if reply.starts_with("MM-ESP32,") {
            Ok(reply.to_string())
        } else {
            Err(Error::new(
                ErrorCode::Transport,
                format!("unexpected ESP32 version reply: {reply}"),
            ))
        }
    }

    pub fn parse_travel(reply: &str) -> Result<f64> {
        let (_, value) = reply
            .split_once(',')
            .ok_or_else(|| Error::new(ErrorCode::Transport, "invalid ESP32 travel reply"))?;
        value
            .parse()
            .map_err(|_| Error::new(ErrorCode::Transport, "invalid ESP32 travel value"))
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Esp32Position {
        pub x_um: f64,
        pub y_um: f64,
        pub z_um: f64,
    }

    impl Esp32Position {
        pub fn value(&self) -> Value {
            Value::Map(BTreeMap::from([
                ("x".into(), position(self.x_um)),
                ("y".into(), position(self.y_um)),
                ("z".into(), position(self.z_um)),
            ]))
        }
    }

    pub fn parse_position(reply: &str) -> Result<Esp32Position> {
        let fields = reply.trim().split(',').collect::<Vec<_>>();
        if fields.len() != 4 || fields[0] != "W" {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("invalid ESP32 position reply: {reply}"),
            ));
        }
        Ok(Esp32Position {
            x_um: parse_axis_position("x", fields[1])?,
            y_um: parse_axis_position("y", fields[2])?,
            z_um: parse_axis_position("z", fields[3])?,
        })
    }

    fn parse_axis_position(axis: &str, value: &str) -> Result<f64> {
        value.parse::<f64>().map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("invalid ESP32 {axis} position: {error}"),
            )
        })
    }

    pub fn parse_analog(reply: &str) -> Result<i64> {
        let fields = reply.trim().split(',').collect::<Vec<_>>();
        if fields.len() != 2 || fields[0] != "A" {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("invalid ESP32 analog reply: {reply}"),
            ));
        }
        let value = fields[1].parse::<i64>().map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("invalid ESP32 analog value: {error}"),
            )
        })?;
        Ok(value.clamp(0, 4095))
    }
}

pub struct Esp32Discovery {
    next_id: DriverId,
    simulated: bool,
    configured: Vec<Esp32ConfiguredProbe>,
}

#[derive(Debug, Clone)]
pub struct Esp32ConfiguredProbe {
    label: String,
    probe: protocol::Esp32Probe,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connect_real_transport: bool,
}

impl Esp32Discovery {
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
            .filter(|device| matches!(device.driver.as_str(), "esp32" | "mm-esp32"))
            .map(Esp32ConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            next_id,
            simulated: false,
            configured,
        })
    }
}

impl DriverDiscovery for Esp32Discovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        if self.simulated {
            return Ok(vec![DriverCandidate::from_driver(
                "Simulated Micro-Manager ESP32 firmware",
                Box::new(Esp32Driver::simulated(self.next_id)),
            )]);
        }
        self.configured
            .iter()
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let driver: Box<dyn Driver> = if configured.connect_real_transport {
                    Box::new(Esp32Driver::serial(id, configured.clone())?)
                } else {
                    Box::new(Esp32Driver::configured(id, configured.clone()))
                };
                Ok(DriverCandidate::from_driver(
                    configured.label.clone(),
                    driver,
                ))
            })
            .collect()
    }
}

impl Esp32ConfiguredProbe {
    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = protocol::Esp32Probe::simulated();
        probe.firmware = string_prop(device, "firmware").unwrap_or(probe.firmware);
        probe.x_travel_um =
            position_config_um(device, "x_travel", "x_travel_um").unwrap_or(probe.x_travel_um);
        probe.y_travel_um =
            position_config_um(device, "y_travel", "y_travel_um").unwrap_or(probe.y_travel_um);
        probe.z_travel_um =
            position_config_um(device, "z_travel", "z_travel_um").unwrap_or(probe.z_travel_um);
        probe.pwm_channels = u8_prop(device, "pwm_channels").unwrap_or(probe.pwm_channels);
        Ok(Self {
            label: if device.label.is_empty() {
                "Configured Micro-Manager ESP32 firmware".into()
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

pub struct Esp32Driver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    digital: DeviceId,
    shutter: DeviceId,
    pwm: DeviceId,
    adc: DeviceId,
    xy: DeviceId,
    z: DeviceId,
    probe: protocol::Esp32Probe,
    digital_mask: u64,
    shutter_open: bool,
    pwm_values: Vec<f64>,
    adc_value: i64,
    xy_position_um: (f64, f64),
    z_position_um: f64,
    serial_port: Option<String>,
    baud_rate: u32,
    connected: bool,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
}

impl Esp32Driver {
    pub fn configured(id: DriverId, configured: Esp32ConfiguredProbe) -> Self {
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
            protocol::Esp32Probe::simulated(),
            Box::new(ScriptedSerial::new()),
            None,
            115_200,
            false,
        )
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: Esp32ConfiguredProbe) -> Result<Self> {
        let port_name = configured.serial_port.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "ESP32 real serial config requires serial_port",
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
        driver.refresh_startup_probe(configured.serial_timeout_ms)?;
        Ok(driver)
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, configured: Esp32ConfiguredProbe) -> Result<Self> {
        let _ = configured.serial_port.as_ref();
        let _ = configured.baud_rate;
        let _ = configured.serial_timeout_ms;
        Err(Error::new(
            ErrorCode::Unsupported,
            "ESP32 real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    fn new(
        id: DriverId,
        probe: protocol::Esp32Probe,
        serial: Box<dyn SerialIo>,
        serial_port: Option<String>,
        baud_rate: u32,
        connected: bool,
    ) -> Self {
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 101)),
            hub: DeviceId(NodeId(id.0 * 1000 + 110)),
            digital: DeviceId(NodeId(id.0 * 1000 + 111)),
            shutter: DeviceId(NodeId(id.0 * 1000 + 112)),
            pwm: DeviceId(NodeId(id.0 * 1000 + 113)),
            adc: DeviceId(NodeId(id.0 * 1000 + 114)),
            xy: DeviceId(NodeId(id.0 * 1000 + 115)),
            z: DeviceId(NodeId(id.0 * 1000 + 116)),
            pwm_values: vec![0.0; probe.pwm_channels as usize],
            probe,
            digital_mask: 0,
            shutter_open: false,
            adc_value: 2048,
            xy_position_um: (0.0, 0.0),
            z_position_um: 0.0,
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
    fn refresh_startup_probe(&mut self, timeout_ms: u64) -> Result<()> {
        let firmware = self.query_line(protocol::Esp32Command::Version, timeout_ms)?;
        self.probe.firmware = protocol::parse_version(&firmware)?;
        self.probe.x_travel_um = self.query_travel(protocol::Axis::X, timeout_ms)?;
        self.probe.y_travel_um = self.query_travel(protocol::Axis::Y, timeout_ms)?;
        self.probe.z_travel_um = self.query_travel(protocol::Axis::Z, timeout_ms)?;
        self.write_line(protocol::Esp32Command::QueryPosition)?;
        self.drain_position_replies_until(timeout_ms)?;
        Ok(())
    }

    #[cfg(feature = "os-serial")]
    fn query_travel(&mut self, axis: protocol::Axis, timeout_ms: u64) -> Result<f64> {
        let reply = self.query_line(protocol::Esp32Command::Travel(axis), timeout_ms)?;
        protocol::parse_travel(&reply)
    }

    #[cfg(feature = "os-serial")]
    fn query_line(&mut self, command: protocol::Esp32Command, timeout_ms: u64) -> Result<String> {
        self.write_line(command)?;
        self.read_line_until(timeout_ms)
    }

    #[cfg(feature = "os-serial")]
    fn read_line_until(&mut self, timeout_ms: u64) -> Result<String> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            let bytes = self.serial.read_available()?;
            for line in self.codec.push(&bytes) {
                let line = line.trim().to_string();
                if !line.is_empty() {
                    return Ok(line);
                }
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        Err(Error::new(
            ErrorCode::Transport,
            "ESP32 did not return a startup probe reply",
        ))
    }

    #[cfg(feature = "os-serial")]
    fn drain_position_replies_until(&mut self, timeout_ms: u64) -> Result<()> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            if self.drain_position_replies()? {
                return Ok(());
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        Err(Error::new(
            ErrorCode::Transport,
            "ESP32 did not return a position reply",
        ))
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        let mut descriptors = vec![
            DeviceDescriptor {
                id: self.hub,
                driver: self.id,
                label: "esp32-hub".into(),
                vendor: Some("Micro-Manager".into()),
                model: Some("ESP32 firmware".into()),
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
                        "firmware".into(),
                        Value::String(self.probe.firmware.clone()),
                    ),
                    ("state_summary".into(), self.state_summary()),
                ]),
            },
            DeviceDescriptor {
                id: self.digital,
                driver: self.id,
                label: "esp32-digital-out".into(),
                vendor: Some("Micro-Manager".into()),
                model: Some("ESP32 digital IO".into()),
                serial: None,
                kinds: vec!["digital.io".into(), "trigger.source".into()],
                properties: vec![property(
                    "mask",
                    "Digital output mask",
                    ValueType::I64,
                    None,
                    true,
                    None,
                )],
                metadata: BTreeMap::new(),
            },
            DeviceDescriptor {
                id: self.shutter,
                driver: self.id,
                label: "esp32-shutter".into(),
                vendor: Some("Micro-Manager".into()),
                model: Some("ESP32 shutter".into()),
                serial: None,
                kinds: vec!["shutter".into(), "trigger.sink".into()],
                properties: vec![sequenceable_property(
                    "open",
                    "Open",
                    ValueType::Bool,
                    None,
                    true,
                    None,
                )],
                metadata: BTreeMap::new(),
            },
            DeviceDescriptor {
                id: self.pwm,
                driver: self.id,
                label: "esp32-pwm".into(),
                vendor: Some("Micro-Manager".into()),
                model: Some("ESP32 PWM".into()),
                serial: None,
                kinds: vec!["analog.output".into(), "pwm".into()],
                properties: vec![sequenceable_property(
                    "channel_0",
                    "PWM channel 0",
                    ValueType::Ratio,
                    Some("percent"),
                    true,
                    Some(Range {
                        min: Value::Ratio(Ratio::from_percent(0.0)),
                        max: Value::Ratio(Ratio::from_percent(100.0)),
                    }),
                )],
                metadata: BTreeMap::from([(
                    "channel_count".into(),
                    Value::I64(self.probe.pwm_channels as i64),
                )]),
            },
            DeviceDescriptor {
                id: self.adc,
                driver: self.id,
                label: "esp32-adc".into(),
                vendor: Some("Micro-Manager".into()),
                model: Some("ESP32 ADC".into()),
                serial: None,
                kinds: vec!["analog.input".into(), "adc".into()],
                properties: vec![property(
                    "channel_0",
                    "ADC channel 0",
                    ValueType::I64,
                    Some("count"),
                    false,
                    Some(Range {
                        min: Value::I64(0),
                        max: Value::I64(4095),
                    }),
                )],
                metadata: BTreeMap::new(),
            },
        ];

        if self.probe.x_travel_um > 0.0 && self.probe.y_travel_um > 0.0 {
            descriptors.push(DeviceDescriptor {
                id: self.xy,
                driver: self.id,
                label: "esp32-xy".into(),
                vendor: Some("Micro-Manager".into()),
                model: Some("ESP32 XY stage".into()),
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
            });
        }
        if self.probe.z_travel_um > 0.0 {
            descriptors.push(DeviceDescriptor {
                id: self.z,
                driver: self.id,
                label: "esp32-z".into(),
                vendor: Some("Micro-Manager".into()),
                model: Some("ESP32 Z stage".into()),
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
            });
        }

        descriptors
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        match (device, key) {
            (device, "mask") if device == self.digital => Ok(Value::I64(self.digital_mask as i64)),
            (device, "open") if device == self.shutter => Ok(Value::Bool(self.shutter_open)),
            (device, "channel_0") if device == self.pwm => Ok(Value::Ratio(Ratio::from_percent(
                self.pwm_values.first().copied().unwrap_or(0.0),
            ))),
            (device, "channel_0") if device == self.adc => Ok(Value::I64(self.adc_value)),
            (device, "state_summary") if device == self.hub => Ok(self.state_summary()),
            (device, "x") if device == self.xy => Ok(position(self.xy_position_um.0)),
            (device, "y") if device == self.xy => Ok(position(self.xy_position_um.1)),
            (device, "z") if device == self.z => Ok(position(self.z_position_um)),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown ESP32 property {key}"),
            )),
        }
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: &Value) -> Result<Value> {
        match (device, key, value) {
            (device, "mask", Value::I64(mask)) if device == self.digital => {
                self.digital_mask = *mask as u64;
                for channel in 0..8 {
                    let high = (self.digital_mask & (1u64 << channel)) != 0;
                    self.write_line(protocol::Esp32Command::Digital { channel, high })?;
                }
                Ok(Value::I64(*mask))
            }
            (device, "open", Value::Bool(open)) if device == self.shutter => {
                self.shutter_open = *open;
                self.write_line(protocol::Esp32Command::Digital {
                    channel: 0,
                    high: *open,
                })?;
                Ok(Value::Bool(*open))
            }
            (device, "channel_0", Value::Ratio(percent)) if device == self.pwm => {
                let percent = percent.percent().clamp(0.0, 100.0);
                if let Some(slot) = self.pwm_values.first_mut() {
                    *slot = percent;
                }
                self.write_line(protocol::Esp32Command::Pwm {
                    channel: 0,
                    duty_percent: percent,
                })?;
                Ok(Value::Ratio(Ratio::from_percent(percent)))
            }
            (device, "x", value) if device == self.xy => {
                let x = position_um(value)?.clamp(0.0, self.probe.x_travel_um);
                self.xy_position_um.0 = x;
                self.write_line(protocol::Esp32Command::MoveXyAbs {
                    x_um: self.xy_position_um.0,
                    y_um: self.xy_position_um.1,
                })?;
                Ok(position(x))
            }
            (device, "y", value) if device == self.xy => {
                let y = position_um(value)?.clamp(0.0, self.probe.y_travel_um);
                self.xy_position_um.1 = y;
                self.write_line(protocol::Esp32Command::MoveXyAbs {
                    x_um: self.xy_position_um.0,
                    y_um: self.xy_position_um.1,
                })?;
                Ok(position(y))
            }
            (device, "z", value) if device == self.z => {
                let z = position_um(value)?.clamp(0.0, self.probe.z_travel_um);
                self.z_position_um = z;
                self.write_line(protocol::Esp32Command::MoveZAbs { z_um: z })?;
                Ok(position(z))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid ESP32 write {key}"),
            )),
        }
    }

    fn write_line(&mut self, command: protocol::Esp32Command) -> Result<()> {
        let line = protocol::encode(&command);
        self.serial.write(&self.codec.encode(&line))
    }

    fn drain_position_replies(&mut self) -> Result<bool> {
        let bytes = self.serial.read_available()?;
        let mut parsed_position = false;
        for line in self.codec.push(&bytes) {
            if let Ok(position) = protocol::parse_position(&line) {
                self.apply_hardware_position(position);
                parsed_position = true;
            } else if let Ok(value) = protocol::parse_analog(&line) {
                self.apply_hardware_adc(value);
            } else {
                self.pending
                    .push_back(DriverEvent::Event(Event::Log(LogEvent {
                        driver: Some(self.id),
                        message: format!("esp32 serial: {line}"),
                    })));
            }
        }
        Ok(parsed_position)
    }

    fn drain_analog_replies(&mut self) -> Result<bool> {
        let bytes = self.serial.read_available()?;
        let mut parsed_analog = false;
        for line in self.codec.push(&bytes) {
            match protocol::parse_analog(&line) {
                Ok(value) => {
                    self.apply_hardware_adc(value);
                    parsed_analog = true;
                }
                Err(_) => {
                    if let Ok(position) = protocol::parse_position(&line) {
                        self.apply_hardware_position(position);
                    } else {
                        self.pending
                            .push_back(DriverEvent::Event(Event::Log(LogEvent {
                                driver: Some(self.id),
                                message: format!("esp32 serial: {line}"),
                            })));
                    }
                }
            }
        }
        Ok(parsed_analog)
    }

    fn apply_hardware_position(&mut self, reply: protocol::Esp32Position) {
        let old_summary = self.state_summary();

        let x = reply.x_um.clamp(0.0, self.probe.x_travel_um);
        if self.xy_position_um.0 != x {
            self.xy_position_um.0 = x;
            self.emit_property(self.xy, "x", position(self.xy_position_um.0));
        }

        let y = reply.y_um.clamp(0.0, self.probe.y_travel_um);
        if self.xy_position_um.1 != y {
            self.xy_position_um.1 = y;
            self.emit_property(self.xy, "y", position(self.xy_position_um.1));
        }

        let z = reply.z_um.clamp(0.0, self.probe.z_travel_um);
        if self.z_position_um != z {
            self.z_position_um = z;
            self.emit_property(self.z, "z", position(self.z_position_um));
        }

        let new_summary = self.state_summary();
        if old_summary != new_summary {
            self.emit_property(self.hub, "state_summary", new_summary);
        }
    }

    fn apply_hardware_adc(&mut self, value: i64) {
        let value = value.clamp(0, 4095);
        if self.adc_value != value {
            self.adc_value = value;
            self.emit_property(self.adc, "channel_0", Value::I64(value));
            self.emit_property(self.hub, "state_summary", self.state_summary());
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

    fn position_snapshot(&self) -> protocol::Esp32Position {
        protocol::Esp32Position {
            x_um: self.xy_position_um.0,
            y_um: self.xy_position_um.1,
            z_um: self.z_position_um,
        }
    }

    fn state_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            (
                "firmware".into(),
                Value::String(self.probe.firmware.clone()),
            ),
            ("position".into(), self.position_snapshot().value()),
            ("digital_mask".into(), Value::I64(self.digital_mask as i64)),
            ("shutter_open".into(), Value::Bool(self.shutter_open)),
            (
                "pwm".into(),
                Value::List(
                    self.pwm_values
                        .iter()
                        .map(|value| Value::Ratio(Ratio::from_percent(*value)))
                        .collect(),
                ),
            ),
            ("adc_channel_0".into(), Value::I64(self.adc_value)),
        ]))
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
        if device != self.hub && device != self.xy && device != self.z && device != self.adc {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "ESP32 GenericCommand is available on the hub, XY stage, Z stage, or ADC device",
            ));
        }
        if !request.params.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "ESP32 GenericCommand does not take parameters",
            ));
        }
        match (device, request.command.as_str()) {
            (device, "refresh_adc") if device == self.adc => Ok(()),
            (device, "refresh_position" | "refresh_state")
                if device == self.hub || device == self.xy || device == self.z =>
            {
                Ok(())
            }
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "ESP32 GenericCommand supports refresh_position/refresh_state on hub/stages and refresh_adc on ADC; got {}",
                    other.1
                ),
            )),
        }
    }

    fn apply_generic_command(
        &mut self,
        device: DeviceId,
        request: GenericCommandRequest,
    ) -> Result<Value> {
        self.validate_generic_command(device, &request)?;
        if device == self.adc {
            self.write_line(protocol::Esp32Command::ReadAnalog { channel: 0 })?;
            let _ = self.drain_analog_replies()?;
        } else {
            self.write_line(protocol::Esp32Command::QueryPosition)?;
            let _ = self.drain_position_replies()?;
        }
        let state = if device == self.adc {
            Value::Map(BTreeMap::from([(
                "channel_0".into(),
                Value::I64(self.adc_value),
            )]))
        } else if device == self.xy {
            Value::Map(BTreeMap::from([
                ("x".into(), position(self.xy_position_um.0)),
                ("y".into(), position(self.xy_position_um.1)),
            ]))
        } else if device == self.z {
            Value::Map(BTreeMap::from([("z".into(), position(self.z_position_um))]))
        } else {
            self.state_summary()
        };
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("commands".into(), Value::I64(1)),
            ("state".into(), state),
            (
                "completion_basis".into(),
                Value::String(if device == self.adc {
                    "ESP32 mapped A analog readback".into()
                } else {
                    "ESP32 mapped W position readback".into()
                }),
            ),
        ])))
    }

    fn owns_device(&self, device: DeviceId) -> bool {
        device == self.hub
            || device == self.digital
            || device == self.shutter
            || device == self.pwm
            || device == self.adc
            || device == self.xy
            || device == self.z
    }

    fn has_timed_shutter(&self, plan: &TimingPlan) -> bool {
        plan.participants.contains(&self.shutter)
            || plan
                .routes
                .iter()
                .any(|route| route.from == self.shutter || route.to == self.shutter)
            || plan
                .sequences
                .iter()
                .any(|sequence| sequence.device == self.shutter)
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
        let has_shutter_sequence = sequences
            .iter()
            .any(|sequence| sequence.device == self.shutter && sequence.property == "open");
        let mut changed = BTreeMap::new();
        let mut xy_changed = false;
        let mut z_changed = false;
        let mut shutter_changed = false;
        let mut pwm_changed = false;

        if self.has_timed_shutter(plan) && !has_shutter_sequence {
            self.shutter_open = start;
            self.emit_property(self.shutter, "open", Value::Bool(start));
            changed.insert(format!("{}:open", (self.shutter.0).0), Value::Bool(start));
            shutter_changed = true;
        }

        for sequence in sequences {
            let Some(value) = (if start {
                sequence.values.first()
            } else {
                sequence.values.last()
            }) else {
                continue;
            };
            match (sequence.device, sequence.property.as_str(), value) {
                (device, "open", Value::Bool(open)) if device == self.shutter => {
                    self.shutter_open = *open;
                    self.emit_property(self.shutter, "open", Value::Bool(*open));
                    changed.insert(format!("{}:open", (self.shutter.0).0), Value::Bool(*open));
                    shutter_changed = true;
                }
                (device, "channel_0", Value::Ratio(percent)) if device == self.pwm => {
                    let percent = percent.percent().clamp(0.0, 100.0);
                    if let Some(slot) = self.pwm_values.first_mut() {
                        *slot = percent;
                    }
                    let value = Value::Ratio(Ratio::from_percent(percent));
                    self.emit_property(self.pwm, "channel_0", value.clone());
                    changed.insert(format!("{}:channel_0", (self.pwm.0).0), value);
                    pwm_changed = true;
                }
                (device, "x", value) if device == self.xy => {
                    self.xy_position_um.0 = position_um(value)?.clamp(0.0, self.probe.x_travel_um);
                    let value = position(self.xy_position_um.0);
                    self.emit_property(self.xy, "x", value.clone());
                    changed.insert(format!("{}:x", (self.xy.0).0), value);
                    xy_changed = true;
                }
                (device, "y", value) if device == self.xy => {
                    self.xy_position_um.1 = position_um(value)?.clamp(0.0, self.probe.y_travel_um);
                    let value = position(self.xy_position_um.1);
                    self.emit_property(self.xy, "y", value.clone());
                    changed.insert(format!("{}:y", (self.xy.0).0), value);
                    xy_changed = true;
                }
                (device, "z", value) if device == self.z => {
                    self.z_position_um = position_um(value)?.clamp(0.0, self.probe.z_travel_um);
                    let value = position(self.z_position_um);
                    self.emit_property(self.z, "z", value.clone());
                    changed.insert(format!("{}:z", (self.z.0).0), value);
                    z_changed = true;
                }
                _ => {}
            }
        }

        if xy_changed {
            self.write_line(protocol::Esp32Command::MoveXyAbs {
                x_um: self.xy_position_um.0,
                y_um: self.xy_position_um.1,
            })?;
        }
        if z_changed {
            self.write_line(protocol::Esp32Command::MoveZAbs {
                z_um: self.z_position_um,
            })?;
        }
        if pwm_changed {
            self.write_line(protocol::Esp32Command::Pwm {
                channel: 0,
                duty_percent: self.pwm_values.first().copied().unwrap_or(0.0),
            })?;
        }
        if shutter_changed {
            self.write_line(protocol::Esp32Command::Digital {
                channel: 0,
                high: self.shutter_open,
            })?;
        }

        Ok(Value::Map(changed))
    }

    fn timing_summary(&self, plan: &TimingPlan, action: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("action".into(), Value::String(action.into())),
            ("hub".into(), Value::I64(self.hub.0 .0 as i64)),
            ("digital".into(), Value::I64(self.digital.0 .0 as i64)),
            ("shutter".into(), Value::I64(self.shutter.0 .0 as i64)),
            (
                "timed_shutter".into(),
                Value::Bool(self.has_timed_shutter(plan)),
            ),
            ("shutter_open".into(), Value::Bool(self.shutter_open)),
            ("digital_mask".into(), Value::I64(self.digital_mask as i64)),
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
        command: protocol::Esp32Command,
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
                "ESP32 StageMove requires the XY or Z stage device",
            ));
        }
        for axis in request.target.keys() {
            match (device, axis) {
                (device, StageAxis::X | StageAxis::Y) if device == self.xy => {}
                (device, StageAxis::Z) if device == self.z => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        format!("axis {} is not available on this ESP32 device", axis.name()),
                    ))
                }
            }
        }
        if request.target.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "ESP32 StageMove requires at least one target axis",
            ));
        }
        Ok(())
    }

    fn stage_move_commands(
        &self,
        device: DeviceId,
        request: &StageMoveRequest,
    ) -> Result<Vec<protocol::Esp32Command>> {
        self.validate_stage_move(device, request)?;
        let mut commands = Vec::new();
        if device == self.xy {
            let mut x = self.xy_position_um.0;
            let mut y = self.xy_position_um.1;
            if let Some(target) = request.target.get(&StageAxis::X) {
                x = if request.relative {
                    x + target.micrometers()
                } else {
                    target.micrometers()
                }
                .clamp(0.0, self.probe.x_travel_um);
            }
            if let Some(target) = request.target.get(&StageAxis::Y) {
                y = if request.relative {
                    y + target.micrometers()
                } else {
                    target.micrometers()
                }
                .clamp(0.0, self.probe.y_travel_um);
            }
            commands.push(protocol::Esp32Command::MoveXyAbs { x_um: x, y_um: y });
        } else if let Some(target) = request.target.get(&StageAxis::Z) {
            let z = if request.relative {
                self.z_position_um + target.micrometers()
            } else {
                target.micrometers()
            }
            .clamp(0.0, self.probe.z_travel_um);
            commands.push(protocol::Esp32Command::MoveZAbs { z_um: z });
        }
        Ok(commands)
    }

    fn apply_stage_move(&mut self, device: DeviceId, request: StageMoveRequest) -> Result<Value> {
        let commands = self.stage_move_commands(device, &request)?;
        if device == self.xy {
            if let Some(target) = request.target.get(&StageAxis::X) {
                self.xy_position_um.0 = if request.relative {
                    self.xy_position_um.0 + target.micrometers()
                } else {
                    target.micrometers()
                }
                .clamp(0.0, self.probe.x_travel_um);
                self.emit_property(self.xy, "x", position(self.xy_position_um.0));
            }
            if let Some(target) = request.target.get(&StageAxis::Y) {
                self.xy_position_um.1 = if request.relative {
                    self.xy_position_um.1 + target.micrometers()
                } else {
                    target.micrometers()
                }
                .clamp(0.0, self.probe.y_travel_um);
                self.emit_property(self.xy, "y", position(self.xy_position_um.1));
            }
        } else if let Some(target) = request.target.get(&StageAxis::Z) {
            self.z_position_um = if request.relative {
                self.z_position_um + target.micrometers()
            } else {
                target.micrometers()
            }
            .clamp(0.0, self.probe.z_travel_um);
            self.emit_property(self.z, "z", position(self.z_position_um));
        }
        for command in commands {
            self.write_line(command)?;
        }
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
    ) -> Result<Vec<protocol::Esp32Command>> {
        match (kind, request) {
            (CapabilityKind::StageMove, CapabilityRequest::StageMove(request)) => {
                self.stage_move_commands(device, request)
            }
            (CapabilityKind::StageMove, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "ESP32 StageMove expects a StageMoveRequest",
            )),
            (CapabilityKind::DigitalIo, CapabilityRequest::DigitalIo(request))
                if device == self.digital =>
            {
                Ok((0..8)
                    .map(|channel| protocol::Esp32Command::Digital {
                        channel,
                        high: (request.mask & (1u64 << channel)) != 0,
                    })
                    .collect())
            }
            (CapabilityKind::TriggerSource, request) if device == self.digital => {
                Ok(trigger_sink_actions(request)?
                    .into_iter()
                    .map(|high| protocol::Esp32Command::Digital { channel: 0, high })
                    .collect())
            }
            (CapabilityKind::Dac, request) if device == self.pwm => {
                Ok(vec![protocol::Esp32Command::Pwm {
                    channel: 0,
                    duty_percent: dac_request_percent(request)?,
                }])
            }
            (CapabilityKind::TriggerSink, request) if device == self.shutter => {
                Ok(trigger_sink_actions(request)?
                    .into_iter()
                    .map(|high| protocol::Esp32Command::Digital { channel: 0, high })
                    .collect())
            }
            (CapabilityKind::GenericCommand, CapabilityRequest::GenericCommand(request))
                if device == self.hub
                    || device == self.xy
                    || device == self.z
                    || device == self.adc =>
            {
                self.validate_generic_command(device, request)?;
                if device == self.adc {
                    Ok(vec![protocol::Esp32Command::ReadAnalog { channel: 0 }])
                } else {
                    Ok(vec![protocol::Esp32Command::QueryPosition])
                }
            }
            (CapabilityKind::GenericCommand, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "ESP32 GenericCommand expects GenericCommandRequest",
            )),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported ESP32 invocation capability",
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
                    "ESP32 StageMove expects a StageMoveRequest",
                )),
            },
            CapabilityKind::DigitalIo if device == self.digital => match request {
                CapabilityRequest::DigitalIo(request) => {
                    let value =
                        self.write_property(device, "mask", &Value::I64(request.mask as i64))?;
                    self.emit_property(device, "mask", value.clone());
                    Ok(Value::Map(BTreeMap::from([
                        ("mask".into(), value),
                        ("commands".into(), Value::I64(1)),
                    ])))
                }
                _ => Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "ESP32 DigitalIo expects a DigitalIoRequest",
                )),
            },
            CapabilityKind::TriggerSource if device == self.digital => {
                let actions = trigger_sink_actions(&request)?;
                for high in &actions {
                    let value = self.write_property(
                        device,
                        "mask",
                        &Value::I64(if *high { 1 } else { 0 }),
                    )?;
                    self.emit_property(device, "mask", value);
                }
                Ok(Value::Map(BTreeMap::from([
                    ("triggered".into(), Value::Bool(true)),
                    ("mask".into(), Value::I64(self.digital_mask as i64)),
                    ("commands".into(), Value::I64(actions.len() as i64)),
                ])))
            }
            CapabilityKind::Dac if device == self.pwm => {
                let percent = dac_request_percent(&request)?;
                let value = self.write_property(
                    device,
                    "channel_0",
                    &Value::Ratio(Ratio::from_percent(percent)),
                )?;
                self.emit_property(device, "channel_0", value.clone());
                Ok(Value::Map(BTreeMap::from([
                    ("channel_0".into(), value),
                    ("commands".into(), Value::I64(1)),
                ])))
            }
            CapabilityKind::TriggerSink if device == self.shutter => {
                let actions = trigger_sink_actions(&request)?;
                for open in &actions {
                    let value = self.write_property(device, "open", &Value::Bool(*open))?;
                    self.emit_property(device, "open", value);
                }
                Ok(Value::Map(BTreeMap::from([
                    ("triggered".into(), Value::Bool(true)),
                    ("open".into(), Value::Bool(self.shutter_open)),
                    ("commands".into(), Value::I64(actions.len() as i64)),
                ])))
            }
            CapabilityKind::GenericCommand
                if device == self.hub
                    || device == self.xy
                    || device == self.z
                    || device == self.adc =>
            {
                let CapabilityRequest::GenericCommand(request) = request else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "ESP32 GenericCommand expects GenericCommandRequest",
                    ));
                };
                self.apply_generic_command(device, request)
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported ESP32 invocation capability",
            )),
        }
    }
}

impl Driver for Esp32Driver {
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
            label: "esp32-serial".into(),
            kind: "serial".into(),
            metadata: BTreeMap::from([
                ("send_ending".into(), Value::String("crlf".into())),
                ("recv_ending".into(), Value::String("crlf".into())),
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
                    "detection_version_command".into(),
                    Value::String("V".into()),
                ),
            ]),
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        match device {
            device if device == self.digital => vec![
                capability(1, device, CapabilityKind::DigitalIo),
                capability(2, device, CapabilityKind::TriggerSource),
            ],
            device if device == self.shutter => {
                vec![capability(3, device, CapabilityKind::TriggerSink)]
            }
            device if device == self.pwm => vec![capability(4, device, CapabilityKind::Dac)],
            device if device == self.adc => {
                vec![capability(7, device, CapabilityKind::GenericCommand)]
            }
            device if device == self.xy || device == self.z => {
                vec![
                    capability(6, device, CapabilityKind::StageMove),
                    capability(7, device, CapabilityKind::GenericCommand),
                ]
            }
            device if device == self.hub => {
                vec![capability(7, device, CapabilityKind::GenericCommand)]
            }
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
                        description: format!("esp32 read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("esp32 write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "esp32 remultiplexed state set".into(),
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
                            "unknown ESP32 capability",
                        ));
                    };
                    if !capability.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "ESP32 {:?} expects {:?}, got {:?}",
                                capability.kind,
                                capability.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    for command in self.invoke_transactions(*device, capability.kind, request)? {
                        physical_transactions
                            .push(self.timing_transaction("esp32 direct invocation", command));
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
                    self.issue_read_command(device, &key)?;
                    if (device == self.hub && key == "state_summary")
                        || (device == self.xy && (key == "x" || key == "y"))
                        || (device == self.z && key == "z")
                    {
                        let _ = self.drain_position_replies()?;
                    } else if device == self.adc && key == "channel_0" {
                        let _ = self.drain_analog_replies()?;
                    }
                    last = self.read_property(device, &key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    last = self.write_property(device, &key, &value)?;
                    self.emit_property(device, &key, last.clone());
                }
                Command::ApplyStateSet(set) => {
                    let mut result = BTreeMap::new();
                    for write in set.writes {
                        let value =
                            self.write_property(write.device, &write.property, &write.value)?;
                        self.emit_property(write.device, &write.property, value.clone());
                        result.insert(format!("{}:{}", (write.device.0).0, write.property), value);
                    }
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
                            "unknown ESP32 capability",
                        ));
                    };
                    if !capability.accepts_request(&request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "ESP32 {:?} expects {:?}, got {:?}",
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
        if let Err(error) = self.drain_position_replies() {
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
                description: "esp32 timing arm summary".into(),
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
            description: "esp32 timing start remultiplexed state flush".into(),
            payload: self.state_summary(),
        });
        physical_transactions.push(PhysicalTransaction {
            resource: Some(self.resource),
            description: "esp32 timing start summary".into(),
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
            description: "esp32 timing stop remultiplexed state flush".into(),
            payload: self.state_summary(),
        });
        physical_transactions.push(PhysicalTransaction {
            resource: Some(self.resource),
            description: "esp32 timing stop summary".into(),
            payload: with_applied(self.timing_summary(&armed.plan, "stop"), applied),
        });
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions,
        })
    }
}

impl Esp32Driver {
    fn issue_read_command(&mut self, device: DeviceId, key: &str) -> Result<()> {
        if (device == self.hub && key == "state_summary")
            || (device == self.xy && (key == "x" || key == "y"))
            || (device == self.z && key == "z")
        {
            self.write_line(protocol::Esp32Command::QueryPosition)?;
        } else if device == self.adc && key == "channel_0" {
            self.write_line(protocol::Esp32Command::ReadAnalog { channel: 0 })?;
        }
        Ok(())
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

fn dac_request_percent(request: &CapabilityRequest) -> Result<f64> {
    match request {
        CapabilityRequest::Dac(request) => percent_value(&request.value),
        _ => Err(Error::new(
            ErrorCode::Unsupported,
            "ESP32 Dac expects CapabilityRequest::Dac",
        )),
    }
}

fn percent_value(value: &Value) -> Result<f64> {
    match value {
        Value::Ratio(percent) => Ok(percent.percent().clamp(0.0, 100.0)),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            "ESP32 percent value must be Ratio",
        )),
    }
}

fn string_prop(device: &DeviceConfig, key: &str) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
        _ => None,
    }
}

fn bool_prop(device: &DeviceConfig, key: &str) -> Option<bool> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn u8_prop(device: &DeviceConfig, key: &str) -> Option<u8> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => u8::try_from(*value).ok(),
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
                "ESP32 TriggerSink expects None or CapabilityRequest::Trigger",
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
