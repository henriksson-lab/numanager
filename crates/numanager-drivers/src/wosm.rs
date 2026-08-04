use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::io::{ErrorKind as IoErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const DEFAULT_PORT: u16 = 1023;
    pub const LINE_ENDING: &str = "\r\n";
    pub const PROMPT: &str = "W>";

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum WosmCommand {
        Login,
        LcdConnected,
        SetDacMode { line: char, mode: u8 },
        SetDigitalMode { line: char, mode: u8 },
        DacDestination { line: char, value: u16 },
        StageOut { axis: char, value: f64 },
        DigitalOut { value: u32, mask: u32 },
        SequenceLoad { index: u8, value: u8 },
        SequenceCount { count: u8 },
        SequenceRun,
        SequenceEnd,
        Blanking(bool),
        BlankingPolarityLow(bool),
        AnalogInput { channel: u8 },
        DigitalInput,
        PullUp { pin: u8, enabled: bool },
    }

    pub fn encode(command: WosmCommand) -> String {
        match command {
            WosmCommand::Login => "wosm".into(),
            WosmCommand::LcdConnected => {
                "kyp_lcd_screen t=5s \"\\n  numanager\\n   connected!\"".into()
            }
            WosmCommand::SetDacMode { line, mode } => format!("dac_mode p{line} {mode}"),
            WosmCommand::SetDigitalMode { line, mode } => format!("dig_mode {line} {mode}"),
            WosmCommand::DacDestination { line, value } => format!("dac_dest p{line} {value}"),
            WosmCommand::StageOut { axis, value } => format!("stg_out_{axis} {value:.3}"),
            WosmCommand::DigitalOut { value, mask } => {
                format!("dig_out 0x{value:08X} 0x{mask:08X}")
            }
            WosmCommand::SequenceLoad { index, value } => format!("P,{index},{value}"),
            WosmCommand::SequenceCount { count } => format!("N,{count}"),
            WosmCommand::SequenceRun => "R".into(),
            WosmCommand::SequenceEnd => "E".into(),
            WosmCommand::Blanking(enabled) => format!("B,{}", u8::from(enabled)),
            WosmCommand::BlankingPolarityLow(low) => format!("F,{}", u8::from(low)),
            WosmCommand::AnalogInput { channel } => format!("A,{channel}"),
            WosmCommand::DigitalInput => "dig_in".into(),
            WosmCommand::PullUp { pin, enabled } => format!("D,{pin},{}", u8::from(enabled)),
        }
    }

    pub fn encode_line(command: WosmCommand) -> Vec<u8> {
        format!("{}{}", encode(command), LINE_ENDING).into_bytes()
    }

    pub fn digital_mask(pattern: u8, inverted_logic: bool) -> u32 {
        let mut value = (pattern as u32) << 18;
        if inverted_logic {
            value = !value;
        }
        value
    }

    pub fn light_line(channel: usize) -> Result<char> {
        match channel {
            0 => Ok('s'),
            1 => Ok('t'),
            2 => Ok('u'),
            3 => Ok('v'),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                "WOSM high-current light channel must be 0..=3",
            )),
        }
    }

    pub fn dac_counts(value: Ratio) -> u16 {
        ((value.percent().clamp(0.0, 100.0) / 100.0) * f64::from(u16::MAX)).round() as u16
    }
}

#[derive(Debug, Clone)]
pub struct WosmConfiguredProbe {
    label: String,
    host: String,
    port: u16,
    product: String,
    serial_number: String,
    firmware_version: i64,
    inverted_logic: bool,
    switch_state: u8,
    sequence_enabled: bool,
    blanking_enabled: bool,
    blank_on_low: bool,
    shutter_open: bool,
    x: Position,
    y: Position,
    z: Position,
    x_travel: Position,
    y_travel: Position,
    z_travel: Position,
    light_outputs: [Ratio; 4],
    light_enabled: [bool; 4],
    analog_inputs: [Ratio; 6],
    analog_input_raw: [i64; 6],
    digital_input: u8,
    input_pullups: u8,
    connect_real_transport: bool,
    prompt_timeout_ms: u64,
}

pub struct WosmDiscovery {
    next_id: DriverId,
    probes: Vec<WosmConfiguredProbe>,
}

impl WosmDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![WosmConfiguredProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), "wosm" | "warwick_wosm"))
            .map(WosmConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for WosmDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver: Box<dyn Driver> = if configured.connect_real_transport {
                    Box::new(WosmDriver::connect_tcp(id, configured)?)
                } else {
                    Box::new(WosmDriver::configured(id, configured))
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

impl WosmConfiguredProbe {
    pub fn fixture() -> Self {
        Self {
            label: "Configured WOSM controller".into(),
            host: "192.168.10.100".into(),
            port: protocol::DEFAULT_PORT,
            product: "Warwick Open-Source Microscope controller".into(),
            serial_number: "WOSM-CONFIG-0001".into(),
            firmware_version: 99,
            inverted_logic: false,
            switch_state: 0,
            sequence_enabled: false,
            blanking_enabled: false,
            blank_on_low: true,
            shutter_open: false,
            x: Position::from_micrometers(0.0),
            y: Position::from_micrometers(0.0),
            z: Position::from_micrometers(0.0),
            x_travel: Position::from_micrometers(100.0),
            y_travel: Position::from_micrometers(100.0),
            z_travel: Position::from_micrometers(100.0),
            light_outputs: [
                Ratio::from_percent(0.0),
                Ratio::from_percent(0.0),
                Ratio::from_percent(0.0),
                Ratio::from_percent(0.0),
            ],
            light_enabled: [false, false, false, false],
            analog_inputs: [
                Ratio::from_percent(10.0),
                Ratio::from_percent(20.0),
                Ratio::from_percent(30.0),
                Ratio::from_percent(40.0),
                Ratio::from_percent(50.0),
                Ratio::from_percent(60.0),
            ],
            analog_input_raw: [0; 6],
            digital_input: 0,
            input_pullups: 0,
            connect_real_transport: false,
            prompt_timeout_ms: 2_000,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::fixture();
        if !device.label.is_empty() {
            configured.label = device.label.clone();
        }
        configured.host = string_prop(device, "host").unwrap_or(configured.host);
        configured.port = u16_prop(device, "port").unwrap_or(configured.port);
        configured.product = string_prop(device, "product").unwrap_or(configured.product);
        configured.serial_number =
            string_prop(device, "serial_number").unwrap_or(configured.serial_number);
        configured.firmware_version =
            i64_prop(device, "firmware_version").unwrap_or(configured.firmware_version);
        configured.connect_real_transport =
            bool_prop(device, "connect").unwrap_or(configured.connect_real_transport);
        configured.prompt_timeout_ms =
            u64_prop(device, "prompt_timeout_ms").unwrap_or(configured.prompt_timeout_ms);
        configured.inverted_logic =
            bool_prop(device, "inverted_logic").unwrap_or(configured.inverted_logic);
        configured.switch_state =
            u8_prop(device, "switch_state").unwrap_or(configured.switch_state);
        configured.sequence_enabled =
            bool_prop(device, "sequence_enabled").unwrap_or(configured.sequence_enabled);
        configured.blanking_enabled =
            bool_prop(device, "blanking_enabled").unwrap_or(configured.blanking_enabled);
        configured.blank_on_low = match string_prop(device, "blank_on")
            .unwrap_or_else(|| {
                if configured.blank_on_low {
                    "Low"
                } else {
                    "High"
                }
                .into()
            })
            .as_str()
        {
            "Low" | "low" => true,
            "High" | "high" => false,
            other => {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("WOSM blank_on must be Low or High, got {other}"),
                ))
            }
        };
        configured.shutter_open =
            bool_prop(device, "shutter_open").unwrap_or(configured.shutter_open);
        configured.x = position_prop(device, "x").unwrap_or(configured.x);
        configured.y = position_prop(device, "y").unwrap_or(configured.y);
        configured.z = position_prop(device, "z").unwrap_or(configured.z);
        configured.x_travel = position_prop(device, "x_travel").unwrap_or(configured.x_travel);
        configured.y_travel = position_prop(device, "y_travel").unwrap_or(configured.y_travel);
        configured.z_travel = position_prop(device, "z_travel").unwrap_or(configured.z_travel);
        configured.digital_input =
            u8_prop(device, "digital_input").unwrap_or(configured.digital_input) & 0x3f;
        configured.input_pullups =
            u8_prop(device, "input_pullups").unwrap_or(configured.input_pullups) & 0x3f;
        for index in 0..4 {
            let output = format!("light_{}_output", index + 1);
            let enabled = format!("light_{}_enabled", index + 1);
            configured.light_outputs[index] =
                ratio_prop(device, &output).unwrap_or(configured.light_outputs[index]);
            configured.light_enabled[index] =
                bool_prop(device, &enabled).unwrap_or(configured.light_enabled[index]);
        }
        for index in 0..6 {
            let key = format!("analog_input_{}", index + 1);
            configured.analog_inputs[index] =
                ratio_prop(device, &key).unwrap_or(configured.analog_inputs[index]);
            let raw_key = format!("analog_input_{}_raw", index + 1);
            configured.analog_input_raw[index] =
                i64_prop(device, &raw_key).unwrap_or(configured.analog_input_raw[index]);
        }
        Ok(configured)
    }
}

struct WosmTcpSession {
    stream: TcpStream,
    prompt: Vec<u8>,
    timeout: Duration,
    rx: Vec<u8>,
}

impl WosmTcpSession {
    fn connect(host: &str, port: u16, timeout_ms: u64) -> Result<Self> {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let addr = (host, port)
            .to_socket_addrs()
            .map_err(map_tcp_error)?
            .next()
            .ok_or_else(|| Error::new(ErrorCode::Transport, "WOSM host did not resolve"))?;
        let stream = TcpStream::connect_timeout(&addr, timeout).map_err(map_tcp_error)?;
        stream
            .set_read_timeout(Some(Duration::from_millis(20)))
            .map_err(map_tcp_error)?;
        stream
            .set_write_timeout(Some(timeout))
            .map_err(map_tcp_error)?;
        let mut session = Self {
            stream,
            prompt: protocol::PROMPT.as_bytes().to_vec(),
            timeout,
            rx: Vec::new(),
        };
        session.send_command(protocol::WosmCommand::Login)?;
        session.send_command(protocol::WosmCommand::LcdConnected)?;
        Ok(session)
    }

    fn send_command(&mut self, command: protocol::WosmCommand) -> Result<String> {
        let bytes = protocol::encode_line(command);
        self.stream.write_all(&bytes).map_err(map_tcp_error)?;
        self.stream.flush().map_err(map_tcp_error)?;
        self.read_until_prompt()
    }

    fn read_until_prompt(&mut self) -> Result<String> {
        let start = Instant::now();
        let mut buffer = [0u8; 512];
        loop {
            if find_subslice(&self.rx, &self.prompt).is_some() {
                let bytes = std::mem::take(&mut self.rx);
                return Ok(String::from_utf8_lossy(&bytes).to_string());
            }
            if start.elapsed() > self.timeout {
                return Err(Error::new(
                    ErrorCode::Timeout,
                    "WOSM TCP command timed out waiting for prompt",
                ));
            }
            match self.stream.read(&mut buffer) {
                Ok(0) => {
                    return Err(Error::new(
                        ErrorCode::Transport,
                        "WOSM TCP connection closed",
                    ))
                }
                Ok(n) => self.rx.extend_from_slice(&buffer[..n]),
                Err(err)
                    if matches!(
                        err.kind(),
                        IoErrorKind::TimedOut | IoErrorKind::WouldBlock | IoErrorKind::Interrupted
                    ) => {}
                Err(err) => return Err(map_tcp_error(err)),
            }
        }
    }
}

pub struct WosmDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    switch: DeviceId,
    shutter: DeviceId,
    xy: DeviceId,
    z: DeviceId,
    input: DeviceId,
    lights: [DeviceId; 4],
    configured: WosmConfiguredProbe,
    tcp: Option<WosmTcpSession>,
    last_transaction: Value,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
}

impl WosmDriver {
    pub fn configured(id: DriverId, configured: WosmConfiguredProbe) -> Self {
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 930)),
            hub: DeviceId(NodeId(id.0 * 1000 + 931)),
            switch: DeviceId(NodeId(id.0 * 1000 + 932)),
            shutter: DeviceId(NodeId(id.0 * 1000 + 933)),
            xy: DeviceId(NodeId(id.0 * 1000 + 934)),
            z: DeviceId(NodeId(id.0 * 1000 + 935)),
            input: DeviceId(NodeId(id.0 * 1000 + 936)),
            lights: [
                DeviceId(NodeId(id.0 * 1000 + 937)),
                DeviceId(NodeId(id.0 * 1000 + 938)),
                DeviceId(NodeId(id.0 * 1000 + 939)),
                DeviceId(NodeId(id.0 * 1000 + 940)),
            ],
            configured,
            tcp: None,
            last_transaction: Value::Map(BTreeMap::new()),
            next_token: 1,
            pending: VecDeque::new(),
        }
    }

    pub fn connect_tcp(id: DriverId, configured: WosmConfiguredProbe) -> Result<Self> {
        let mut driver = Self::configured(id, configured);
        let session = WosmTcpSession::connect(
            &driver.configured.host,
            driver.configured.port,
            driver.configured.prompt_timeout_ms,
        )?;
        driver.tcp = Some(session);
        Ok(driver)
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn query(&mut self, command: protocol::WosmCommand, action: &str) -> Result<Option<String>> {
        let encoded_length = protocol::encode_line(command).len();
        let mut completion_basis = "configured state".to_string();
        let mut reply = Value::Null;
        let mut text_reply = None;
        if let Some(tcp) = self.tcp.as_mut() {
            let text = tcp.send_command(command)?;
            completion_basis = "controller prompt".into();
            reply = Value::String(text.clone());
            text_reply = Some(text);
        }
        self.last_transaction = Value::Map(BTreeMap::from([
            ("action".into(), Value::String(action.into())),
            ("completion_basis".into(), Value::String(completion_basis)),
            (
                "resource".into(),
                Value::String(format!("{}:{}", self.configured.host, self.configured.port)),
            ),
            (
                "encoded_length".into(),
                Value::ByteCount(ByteCount::new(encoded_length as u64)),
            ),
            ("live_tcp".into(), Value::Bool(self.tcp.is_some())),
            ("reply".into(), reply),
        ]));
        Ok(text_reply)
    }

    fn send(&mut self, command: protocol::WosmCommand, action: &str) -> Result<()> {
        self.query(command, action).map(|_| ())
    }

    fn refresh_digital_input(&mut self) -> Result<u8> {
        let Some(reply) = self.query(protocol::WosmCommand::DigitalInput, "read_digital_input")?
        else {
            return Ok(self.configured.digital_input);
        };
        if let Some(value) = parse_wosm_reply_integer(&reply, "dig_in") {
            self.configured.digital_input = (value as u8) & 0x3f;
            self.emit_property(
                self.input,
                "digital_input",
                Value::I64(self.configured.digital_input as i64),
            );
        }
        Ok(self.configured.digital_input)
    }

    fn refresh_analog_input_raw(&mut self, index: usize) -> Result<i64> {
        let channel = u8::try_from(index)
            .map_err(|_| Error::new(ErrorCode::InvalidProperty, "invalid WOSM analog channel"))?;
        let Some(reply) = self.query(
            protocol::WosmCommand::AnalogInput { channel },
            "read_analog_input_raw",
        )?
        else {
            return Ok(self.configured.analog_input_raw[index]);
        };
        if let Some(value) = parse_wosm_reply_integer(&reply, "A") {
            self.configured.analog_input_raw[index] = value;
            self.emit_property(
                self.input,
                &format!("analog_input_{}_raw", index + 1),
                Value::I64(value),
            );
        }
        Ok(self.configured.analog_input_raw[index])
    }

    fn write_input_pullups(&mut self, mask: u8) -> Result<Value> {
        let mask = mask & 0x3f;
        for pin in 0..6u8 {
            self.send(
                protocol::WosmCommand::PullUp {
                    pin,
                    enabled: mask & (1 << pin) != 0,
                },
                "set_input_pullup",
            )?;
        }
        self.configured.input_pullups = mask;
        self.emit_property(self.input, "input_pullups", Value::I64(mask as i64));
        Ok(Value::I64(mask as i64))
    }

    fn analog_input_index(key: &str, raw: bool) -> Option<usize> {
        let prefix = "analog_input_";
        let suffix = "_raw";
        if !key.starts_with(prefix) {
            return None;
        }
        let number = if raw {
            key.strip_suffix(suffix)?.trim_start_matches(prefix)
        } else if key.ends_with(suffix) {
            return None;
        } else {
            key.trim_start_matches(prefix)
        };
        number
            .parse::<usize>()
            .ok()
            .and_then(|value| value.checked_sub(1))
            .filter(|index| *index < 6)
    }

    fn adc_request_index(request: &AdcRequest) -> Result<usize> {
        let Some(channel) = request.channel.as_deref() else {
            return Ok(0);
        };
        let normalized = channel
            .trim()
            .trim_start_matches("analog_input_")
            .trim_start_matches("channel_")
            .trim_start_matches("channel")
            .trim_start_matches("input_")
            .trim_start_matches("input")
            .trim_end_matches("_raw");
        normalized
            .parse::<usize>()
            .ok()
            .and_then(|value| value.checked_sub(1))
            .filter(|index| *index < 6)
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidCommand,
                    "WOSM Adc channel must be 1..6, channel_1..channel_6, or analog_input_1..analog_input_6",
                )
            })
    }

    fn analog_input_ratio(&self, index: usize) -> Result<Value> {
        self.configured
            .analog_inputs
            .get(index)
            .copied()
            .map(Value::Ratio)
            .ok_or_else(|| Error::new(ErrorCode::InvalidProperty, "unknown WOSM analog input"))
    }

    fn light_index(&self, device: DeviceId) -> Option<usize> {
        self.lights.iter().position(|id| *id == device)
    }

    fn move_absolute(&mut self, x: Position, y: Position, z: Position) -> Result<Value> {
        let x = clamp_position(x, self.configured.x_travel);
        let y = clamp_position(y, self.configured.y_travel);
        let z = clamp_position(z, self.configured.z_travel);
        if x != self.configured.x {
            self.send(
                protocol::WosmCommand::StageOut {
                    axis: 'x',
                    value: x.micrometers(),
                },
                "move_x",
            )?;
            self.configured.x = x;
            self.emit_property(self.xy, "x", Value::Position(x));
        }
        if y != self.configured.y {
            self.send(
                protocol::WosmCommand::StageOut {
                    axis: 'y',
                    value: y.micrometers(),
                },
                "move_y",
            )?;
            self.configured.y = y;
            self.emit_property(self.xy, "y", Value::Position(y));
        }
        if z != self.configured.z {
            self.send(
                protocol::WosmCommand::StageOut {
                    axis: 'z',
                    value: z.micrometers(),
                },
                "move_z",
            )?;
            self.configured.z = z;
            self.emit_property(self.z, "z", Value::Position(z));
        }
        Ok(self.position_map())
    }

    fn apply_stage_move(&mut self, device: DeviceId, request: StageMoveRequest) -> Result<Value> {
        self.validate_stage_move(device, &request)?;
        let mut x = self.configured.x;
        let mut y = self.configured.y;
        let mut z = self.configured.z;
        if device == self.xy {
            if let Some(target) = request.target.get(&StageAxis::X) {
                x = if request.relative {
                    Position::from_micrometers(
                        self.configured.x.micrometers() + target.micrometers(),
                    )
                } else {
                    *target
                };
            }
            if let Some(target) = request.target.get(&StageAxis::Y) {
                y = if request.relative {
                    Position::from_micrometers(
                        self.configured.y.micrometers() + target.micrometers(),
                    )
                } else {
                    *target
                };
            }
        } else if let Some(target) = request.target.get(&StageAxis::Z) {
            z = if request.relative {
                Position::from_micrometers(self.configured.z.micrometers() + target.micrometers())
            } else {
                *target
            };
        }
        self.move_absolute(x, y, z)
    }

    fn write_light_output(&mut self, index: usize, ratio: Ratio) -> Result<Value> {
        validate_ratio(ratio, "WOSM light output")?;
        let line = protocol::light_line(index)?;
        self.send(
            protocol::WosmCommand::DacDestination {
                line,
                value: protocol::dac_counts(ratio),
            },
            "set_light_output",
        )?;
        self.configured.light_outputs[index] = ratio;
        self.emit_property(self.lights[index], "output", Value::Ratio(ratio));
        Ok(Value::Ratio(ratio))
    }

    fn write_light_enabled(&mut self, index: usize, enabled: bool) -> Result<Value> {
        self.configured.light_enabled[index] = enabled;
        let mut pattern = self.configured.switch_state;
        if enabled {
            pattern |= 1 << index;
        } else {
            pattern &= !(1 << index);
        }
        self.write_switch_state(pattern)
    }

    fn write_switch_state(&mut self, pattern: u8) -> Result<Value> {
        let pattern = pattern & 0xff;
        self.send(
            protocol::WosmCommand::DigitalOut {
                value: protocol::digital_mask(pattern, self.configured.inverted_logic),
                mask: 0x03fc_0000,
            },
            "set_switch_state",
        )?;
        self.configured.switch_state = pattern;
        for index in 0..4 {
            let enabled = pattern & (1 << index) != 0;
            if self.configured.light_enabled[index] != enabled {
                self.configured.light_enabled[index] = enabled;
                self.emit_property(self.lights[index], "enabled", Value::Bool(enabled));
            }
        }
        self.emit_property(self.switch, "state", Value::I64(pattern as i64));
        Ok(Value::I64(pattern as i64))
    }

    fn write_sequence_enabled(&mut self, enabled: bool) -> Result<Value> {
        let command = if enabled {
            protocol::WosmCommand::SequenceRun
        } else {
            protocol::WosmCommand::SequenceEnd
        };
        self.send(command, "set_sequence_enabled")?;
        self.configured.sequence_enabled = enabled;
        self.emit_property(self.switch, "sequence_enabled", Value::Bool(enabled));
        Ok(Value::Bool(enabled))
    }

    fn write_blanking_enabled(&mut self, enabled: bool) -> Result<Value> {
        self.send(
            protocol::WosmCommand::Blanking(enabled),
            "set_blanking_enabled",
        )?;
        self.configured.blanking_enabled = enabled;
        self.emit_property(self.switch, "blanking_enabled", Value::Bool(enabled));
        Ok(Value::Bool(enabled))
    }

    fn write_blank_on(&mut self, edge: &str) -> Result<Value> {
        let low = match edge {
            "Low" | "low" => true,
            "High" | "high" => false,
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "WOSM blank_on must be Low or High",
                ))
            }
        };
        self.send(
            protocol::WosmCommand::BlankingPolarityLow(low),
            "set_blank_on",
        )?;
        self.configured.blank_on_low = low;
        let value = Value::String(if low { "Low" } else { "High" }.into());
        self.emit_property(self.switch, "blank_on", value.clone());
        Ok(value)
    }

    fn write_shutter(&mut self, open: bool) -> Result<Value> {
        let pattern = if open {
            self.configured.switch_state
        } else {
            0
        };
        self.send(
            protocol::WosmCommand::DigitalOut {
                value: protocol::digital_mask(pattern, self.configured.inverted_logic),
                mask: 0x03fc_0000,
            },
            "set_shutter",
        )?;
        self.configured.shutter_open = open;
        self.emit_property(self.shutter, "open", Value::Bool(open));
        Ok(Value::Bool(open))
    }

    fn read_property(&mut self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "product" => Ok(Value::String(self.configured.product.clone())),
                "serial_number" => Ok(Value::String(self.configured.serial_number.clone())),
                "firmware_version" => Ok(Value::I64(self.configured.firmware_version)),
                "host" => Ok(Value::String(self.configured.host.clone())),
                "port" => Ok(Value::I64(self.configured.port as i64)),
                "connected" => Ok(Value::Bool(self.tcp.is_some())),
                "prompt_timeout" => Ok(Value::TimeInterval(TimeInterval::from_milliseconds(
                    self.configured.prompt_timeout_ms as f64,
                ))),
                "inverted_logic" => Ok(Value::Bool(self.configured.inverted_logic)),
                "last_transaction" => Ok(self.last_transaction.clone()),
                _ => invalid_property("unknown WOSM hub property", key),
            };
        }
        if device == self.switch {
            return match key {
                "state" => Ok(Value::I64(self.configured.switch_state as i64)),
                "sequence_enabled" => Ok(Value::Bool(self.configured.sequence_enabled)),
                "blanking_enabled" => Ok(Value::Bool(self.configured.blanking_enabled)),
                "blank_on" => Ok(Value::String(
                    if self.configured.blank_on_low {
                        "Low"
                    } else {
                        "High"
                    }
                    .into(),
                )),
                _ => invalid_property("unknown WOSM switch property", key),
            };
        }
        if device == self.shutter {
            return match key {
                "open" => Ok(Value::Bool(self.configured.shutter_open)),
                _ => invalid_property("unknown WOSM shutter property", key),
            };
        }
        if device == self.xy {
            return match key {
                "x" => Ok(Value::Position(self.configured.x)),
                "y" => Ok(Value::Position(self.configured.y)),
                "x_travel" => Ok(Value::Position(self.configured.x_travel)),
                "y_travel" => Ok(Value::Position(self.configured.y_travel)),
                _ => invalid_property("unknown WOSM XY property", key),
            };
        }
        if device == self.z {
            return match key {
                "z" => Ok(Value::Position(self.configured.z)),
                "z_travel" => Ok(Value::Position(self.configured.z_travel)),
                _ => invalid_property("unknown WOSM Z property", key),
            };
        }
        if device == self.input {
            return match key {
                "digital_input" => {
                    let value = self.refresh_digital_input()?;
                    Ok(Value::I64(value as i64))
                }
                "input_pullups" => Ok(Value::I64(self.configured.input_pullups as i64)),
                key if Self::analog_input_index(key, true).is_some() => {
                    let index =
                        Self::analog_input_index(key, true).expect("index checked by match guard");
                    self.refresh_analog_input_raw(index).map(Value::I64)
                }
                key if Self::analog_input_index(key, false).is_some() => {
                    let index =
                        Self::analog_input_index(key, false).expect("index checked by match guard");
                    self.analog_input_ratio(index)
                }
                _ => invalid_property("unknown WOSM input property", key),
            };
        }
        if let Some(index) = self.light_index(device) {
            return match key {
                "output" => Ok(Value::Ratio(self.configured.light_outputs[index])),
                "enabled" => Ok(Value::Bool(self.configured.light_enabled[index])),
                "line" => Ok(Value::String(protocol::light_line(index)?.to_string())),
                _ => invalid_property("unknown WOSM light property", key),
            };
        }
        invalid_property("unknown WOSM device property", key)
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        match (device, key, value) {
            (device, "inverted_logic", Value::Bool(_)) if device == self.hub => Ok(()),
            (device, "state", Value::I64(value))
                if device == self.switch && (0..=255).contains(value) =>
            {
                Ok(())
            }
            (device, "sequence_enabled" | "blanking_enabled", Value::Bool(_))
                if device == self.switch =>
            {
                Ok(())
            }
            (device, "blank_on", Value::String(value))
                if device == self.switch
                    && matches!(value.as_str(), "Low" | "low" | "High" | "high") =>
            {
                Ok(())
            }
            (device, "open", Value::Bool(_)) if device == self.shutter => Ok(()),
            (device, "x" | "y", Value::Position(_)) if device == self.xy => Ok(()),
            (device, "z", Value::Position(_)) if device == self.z => Ok(()),
            (device, "output", Value::Ratio(value)) if self.light_index(device).is_some() => {
                validate_ratio(*value, "WOSM light output")
            }
            (device, "enabled", Value::Bool(_)) if self.light_index(device).is_some() => Ok(()),
            (device, "input_pullups", Value::I64(value))
                if device == self.input && (0..=63).contains(value) =>
            {
                Ok(())
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("WOSM property {key} is read-only or wrong type"),
            )),
        }
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: Value) -> Result<Value> {
        self.validate_write(device, key, &value)?;
        match (device, key, value) {
            (device, "inverted_logic", Value::Bool(value)) if device == self.hub => {
                self.configured.inverted_logic = value;
                Ok(Value::Bool(value))
            }
            (device, "state", Value::I64(value)) if device == self.switch => {
                self.write_switch_state(value as u8)
            }
            (device, "sequence_enabled", Value::Bool(enabled)) if device == self.switch => {
                self.write_sequence_enabled(enabled)
            }
            (device, "blanking_enabled", Value::Bool(enabled)) if device == self.switch => {
                self.write_blanking_enabled(enabled)
            }
            (device, "blank_on", Value::String(edge)) if device == self.switch => {
                self.write_blank_on(&edge)
            }
            (device, "open", Value::Bool(value)) if device == self.shutter => {
                self.write_shutter(value)
            }
            (device, "x", Value::Position(position)) if device == self.xy => {
                self.move_absolute(position, self.configured.y, self.configured.z)
            }
            (device, "y", Value::Position(position)) if device == self.xy => {
                self.move_absolute(self.configured.x, position, self.configured.z)
            }
            (device, "z", Value::Position(position)) if device == self.z => {
                self.move_absolute(self.configured.x, self.configured.y, position)
            }
            (device, "output", Value::Ratio(ratio)) => {
                let index = self.light_index(device).expect("validated light output");
                self.write_light_output(index, ratio)
            }
            (device, "enabled", Value::Bool(enabled)) => {
                let index = self.light_index(device).expect("validated light enabled");
                self.write_light_enabled(index, enabled)
            }
            (device, "input_pullups", Value::I64(mask)) if device == self.input => {
                self.write_input_pullups(mask as u8)
            }
            _ => unreachable!("validated WOSM write"),
        }
    }

    fn validate_stage_move(&self, device: DeviceId, request: &StageMoveRequest) -> Result<()> {
        if device != self.xy && device != self.z {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "WOSM StageMove requires the XY or Z device",
            ));
        }
        if request.target.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "WOSM StageMove requires at least one target axis",
            ));
        }
        for axis in request.target.keys() {
            match (device, axis) {
                (device, StageAxis::X | StageAxis::Y) if device == self.xy => {}
                (device, StageAxis::Z) if device == self.z => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        format!("axis {} is not available on this WOSM device", axis.name()),
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
}

impl Driver for WosmDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "wosm-tcp".into(),
            kind: "tcp.text".into(),
            metadata: BTreeMap::from([
                ("host".into(), Value::String(self.configured.host.clone())),
                ("port".into(), Value::I64(self.configured.port as i64)),
                (
                    "prompt_timeout".into(),
                    Value::TimeInterval(TimeInterval::from_milliseconds(
                        self.configured.prompt_timeout_ms as f64,
                    )),
                ),
                ("connected".into(), Value::Bool(self.tcp.is_some())),
                ("prompt".into(), Value::String(protocol::PROMPT.into())),
                ("line_ending".into(), Value::String("CRLF".into())),
                (
                    "support_level".into(),
                    Value::String("prompt_tcp_when_connect_true".into()),
                ),
            ]),
        }]
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        let mut devices = vec![
            DeviceDescriptor {
                id: self.hub,
                driver: self.id,
                label: "wosm-hub".into(),
                vendor: Some("University of Warwick".into()),
                model: Some(self.configured.product.clone()),
                serial: Some(self.configured.serial_number.clone()),
                kinds: vec![
                    "hub".into(),
                    "microscope.controller".into(),
                    "tcp.text".into(),
                ],
                properties: vec![
                    string_property("product", "Product", false),
                    string_property("serial_number", "Serial number", false),
                    integer_property("firmware_version", "Firmware version", false),
                    string_property("host", "Host", false),
                    integer_property("port", "Port", false),
                    bool_property("connected", "Connected", false),
                    time_property("prompt_timeout", "Prompt timeout", false),
                    bool_property("inverted_logic", "Inverted logic", true),
                    map_property("last_transaction", "Last transaction", false),
                ],
                metadata: source_metadata(),
            },
            DeviceDescriptor {
                id: self.switch,
                driver: self.id,
                label: "wosm-switch".into(),
                vendor: Some("University of Warwick".into()),
                model: Some(self.configured.product.clone()),
                serial: Some(format!("{}:switch", self.configured.serial_number)),
                kinds: vec![
                    "digital.output".into(),
                    "state.device".into(),
                    "trigger.source".into(),
                ],
                properties: vec![
                    integer_range_property("state", "State", true, 0, 255),
                    bool_property("sequence_enabled", "Sequence enabled", true),
                    bool_property("blanking_enabled", "Blanking enabled", true),
                    string_property("blank_on", "Blank on", true),
                ],
                metadata: BTreeMap::from([("mask".into(), Value::String("s_to_z".into()))]),
            },
            DeviceDescriptor {
                id: self.shutter,
                driver: self.id,
                label: "wosm-shutter".into(),
                vendor: Some("University of Warwick".into()),
                model: Some(self.configured.product.clone()),
                serial: Some(format!("{}:shutter", self.configured.serial_number)),
                kinds: vec!["shutter".into(), "light.gate".into(), "trigger.sink".into()],
                properties: vec![bool_property("open", "Open", true)],
                metadata: BTreeMap::new(),
            },
            DeviceDescriptor {
                id: self.xy,
                driver: self.id,
                label: "wosm-xy-stage".into(),
                vendor: Some("University of Warwick".into()),
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
                    ("x_travel".into(), Value::Position(self.configured.x_travel)),
                    ("y_travel".into(), Value::Position(self.configured.y_travel)),
                ]),
            },
            DeviceDescriptor {
                id: self.z,
                driver: self.id,
                label: "wosm-z-stage".into(),
                vendor: Some("University of Warwick".into()),
                model: Some(self.configured.product.clone()),
                serial: Some(format!("{}:z", self.configured.serial_number)),
                kinds: vec!["axis.z".into(), "stage.z".into(), "motion.stage".into()],
                properties: vec![
                    position_property("z", "Z", true, Some(self.configured.z_travel)),
                    position_property("z_travel", "Z travel", false, None),
                ],
                metadata: BTreeMap::from([(
                    "z_travel".into(),
                    Value::Position(self.configured.z_travel),
                )]),
            },
            DeviceDescriptor {
                id: self.input,
                driver: self.id,
                label: "wosm-input".into(),
                vendor: Some("University of Warwick".into()),
                model: Some(self.configured.product.clone()),
                serial: Some(format!("{}:input", self.configured.serial_number)),
                kinds: vec![
                    "digital.input".into(),
                    "analog.input".into(),
                    "state.device".into(),
                ],
                properties: input_properties(),
                metadata: BTreeMap::from([(
                    "support_level".into(),
                    Value::String("live_digital_and_raw_analog_readback".into()),
                )]),
            },
        ];
        for index in 0..4 {
            devices.push(DeviceDescriptor {
                id: self.lights[index],
                driver: self.id,
                label: format!("wosm-light-{}", index + 1),
                vendor: Some("University of Warwick".into()),
                model: Some(self.configured.product.clone()),
                serial: Some(format!(
                    "{}:light-{}",
                    self.configured.serial_number,
                    index + 1
                )),
                kinds: vec![
                    "light.source".into(),
                    "dac.output".into(),
                    "trigger.sink".into(),
                ],
                properties: vec![
                    ratio_property("output", "Output", true),
                    bool_property("enabled", "Enabled", true),
                    string_property("line", "Line", false),
                ],
                metadata: BTreeMap::from([(
                    "line".into(),
                    Value::String(protocol::light_line(index).unwrap().to_string()),
                )]),
            });
        }
        devices
    }

    fn graph(&self) -> DeviceGraph {
        let mut graph = DeviceGraph::default();
        let _ = graph.insert_node(GraphNode {
            id: self.resource.0,
            kind: NodeKind::Resource,
            label: "wosm-tcp".into(),
        });
        let _ = graph.insert_node(GraphNode {
            id: self.hub.0,
            kind: NodeKind::Hub,
            label: "wosm-hub".into(),
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
        if device == self.xy || device == self.z {
            return vec![capability(
                1,
                device,
                CapabilityKind::StageMove,
                ValueType::Map,
            )];
        }
        if device == self.switch {
            return vec![
                capability(2, device, CapabilityKind::DigitalIo, ValueType::Map),
                capability(3, device, CapabilityKind::TriggerSource, ValueType::Bool),
            ];
        }
        if device == self.shutter {
            return vec![capability(
                4,
                device,
                CapabilityKind::TriggerSink,
                ValueType::Bool,
            )];
        }
        if self.light_index(device).is_some() {
            return vec![
                capability(5, device, CapabilityKind::Dac, ValueType::Ratio),
                capability(6, device, CapabilityKind::TriggerSink, ValueType::Bool),
            ];
        }
        if device == self.input {
            return vec![
                capability(7, device, CapabilityKind::DigitalIo, ValueType::Map),
                capability(8, device, CapabilityKind::Adc, ValueType::I64),
                capability(9, device, CapabilityKind::Measure, ValueType::Map),
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
                        format!("wosm read {key}"),
                        Value::String(key.clone()),
                    ));
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("wosm write {key}"),
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
                            "unknown WOSM capability",
                        ));
                    };
                    if !descriptor.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "WOSM {} request kind does not match",
                                descriptor.kind.name()
                            ),
                        ));
                    }
                    if let CapabilityRequest::StageMove(request) = request {
                        self.validate_stage_move(*device, request)?;
                    }
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("wosm {}", descriptor.kind.name()),
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
                        "wosm state set",
                        Value::I64(set.writes.len() as i64),
                    ));
                }
                Command::Arm(plan) => {
                    self.validate_timing_plan(plan)?;
                    physical_transactions.push(transaction(
                        self.resource,
                        "wosm timing arm",
                        self.timing_summary(plan, "arm"),
                    ));
                }
                Command::Start(_) => {
                    physical_transactions.push(transaction(
                        self.resource,
                        "wosm timing start",
                        Value::String("R".into()),
                    ));
                }
                Command::Stop(_) => {
                    physical_transactions.push(transaction(
                        self.resource,
                        "wosm timing stop",
                        Value::String("E".into()),
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
                            self.apply_stage_move(device, request)?
                        }
                        (CapabilityKind::Dac, CapabilityRequest::Dac(request)) => {
                            let Value::Ratio(ratio) = request.value else {
                                return Err(Error::new(
                                    ErrorCode::InvalidCommand,
                                    "WOSM Dac requires Ratio value",
                                ));
                            };
                            let index = self.light_index(device).expect("capability on light");
                            self.write_light_output(index, ratio)?
                        }
                        (CapabilityKind::TriggerSource, CapabilityRequest::Trigger(request))
                            if device == self.switch =>
                        {
                            match request.action {
                                TriggerAction::Enable | TriggerAction::Pulse => {
                                    self.write_sequence_enabled(true)?
                                }
                                TriggerAction::Disable => self.write_sequence_enabled(false)?,
                            }
                        }
                        (CapabilityKind::TriggerSink, CapabilityRequest::Trigger(request)) => {
                            match request.action {
                                TriggerAction::Enable | TriggerAction::Pulse => {
                                    if device == self.shutter {
                                        self.write_shutter(true)?
                                    } else {
                                        let index =
                                            self.light_index(device).expect("capability on light");
                                        self.write_light_enabled(index, true)?
                                    }
                                }
                                TriggerAction::Disable => {
                                    if device == self.shutter {
                                        self.write_shutter(false)?
                                    } else {
                                        let index =
                                            self.light_index(device).expect("capability on light");
                                        self.write_light_enabled(index, false)?
                                    }
                                }
                            }
                        }
                        (CapabilityKind::DigitalIo, CapabilityRequest::DigitalIo(request)) => {
                            if device == self.switch {
                                self.write_switch_state((request.mask & 0xff) as u8)?
                            } else {
                                Value::I64(self.refresh_digital_input()? as i64)
                            }
                        }
                        (CapabilityKind::Adc, CapabilityRequest::Adc(request)) => {
                            let index = Self::adc_request_index(&request)?;
                            Value::I64(self.refresh_analog_input_raw(index)?)
                        }
                        (CapabilityKind::Measure, CapabilityRequest::Measure(_)) => {
                            let digital = self.refresh_digital_input()? as i64;
                            let analog_raw = self.refresh_analog_input_raw(0)?;
                            Value::Map(BTreeMap::from([
                                ("digital_input".into(), Value::I64(digital)),
                                ("analog_input_1_raw".into(), Value::I64(analog_raw)),
                                (
                                    "analog_input_1".into(),
                                    Value::Ratio(self.configured.analog_inputs[0]),
                                ),
                            ]))
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported WOSM capability invocation",
                            ));
                        }
                    };
                }
                Command::ApplyStateSet(set) => {
                    last = self.apply_state_set(set)?;
                }
                Command::Arm(plan) => {
                    last = self.program_timing_plan(&plan)?;
                }
                Command::Start(_) => {
                    last = self.write_sequence_enabled(true)?;
                }
                Command::Stop(_) => {
                    last = self.write_sequence_enabled(false)?;
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
                "wosm timing arm",
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
                "wosm timing start",
                Value::String("R".into()),
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
                "wosm timing stop",
                Value::String("E".into()),
            )],
        })
    }
}

impl WosmDriver {
    fn owns_device(&self, device: DeviceId) -> bool {
        device == self.hub
            || device == self.switch
            || device == self.shutter
            || device == self.xy
            || device == self.z
            || device == self.input
            || self.light_index(device).is_some()
    }

    fn validate_read(&self, device: DeviceId, key: &str) -> Result<()> {
        if device == self.hub
            && matches!(
                key,
                "product"
                    | "serial_number"
                    | "firmware_version"
                    | "host"
                    | "port"
                    | "connected"
                    | "prompt_timeout"
                    | "inverted_logic"
                    | "last_transaction"
            )
        {
            return Ok(());
        }
        if device == self.switch
            && matches!(
                key,
                "state" | "sequence_enabled" | "blanking_enabled" | "blank_on"
            )
        {
            return Ok(());
        }
        if device == self.shutter && key == "open" {
            return Ok(());
        }
        if device == self.xy && matches!(key, "x" | "y" | "x_travel" | "y_travel") {
            return Ok(());
        }
        if device == self.z && matches!(key, "z" | "z_travel") {
            return Ok(());
        }
        if device == self.input
            && (key == "digital_input"
                || key == "input_pullups"
                || Self::analog_input_index(key, false).is_some()
                || Self::analog_input_index(key, true).is_some())
        {
            return Ok(());
        }
        if self.light_index(device).is_some() && matches!(key, "output" | "enabled" | "line") {
            return Ok(());
        }
        Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unknown WOSM property {key}"),
        ))
    }

    fn apply_state_set(&mut self, set: StateSet) -> Result<Value> {
        let mut x = self.configured.x;
        let mut y = self.configured.y;
        let mut z = self.configured.z;
        let mut stage_changed = false;
        let mut values = BTreeMap::new();
        for write in set.writes {
            match (write.device, write.property.as_str(), write.value) {
                (device, "x", Value::Position(position)) if device == self.xy => {
                    x = position;
                    stage_changed = true;
                }
                (device, "y", Value::Position(position)) if device == self.xy => {
                    y = position;
                    stage_changed = true;
                }
                (device, "z", Value::Position(position)) if device == self.z => {
                    z = position;
                    stage_changed = true;
                }
                (device, property, value) if self.owns_device(device) => {
                    values.insert(
                        property.into(),
                        self.write_property(device, property, value)?,
                    );
                }
                _ => {}
            }
        }
        if stage_changed {
            values.insert("stage".into(), self.move_absolute(x, y, z)?);
        }
        Ok(Value::Map(values))
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
                "WOSM timing routes have no evidenced route opcode",
            ));
        }
        match &plan.start {
            StartCondition::Software => {}
            StartCondition::ExternalTrigger(device) if !self.owns_device(*device) => {}
            StartCondition::ExternalTrigger(_) | StartCondition::At(_) => {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    "WOSM local external/absolute timing starts have no evidenced start opcode",
                ));
            }
        }
        for sequence in self.local_timing_sequences(plan) {
            self.validate_timing_sequence(sequence)?;
        }
        Ok(())
    }

    fn validate_timing_sequence(&self, sequence: &DeviceSequence) -> Result<()> {
        if sequence.device != self.switch || sequence.property != "state" {
            return Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "WOSM timing supports only switch state sequences, not {} on device {}",
                    sequence.property,
                    (sequence.device.0).0
                ),
            ));
        }
        if sequence.values.len() > u8::MAX as usize + 1 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "WOSM switch timing sequence is too long for u8 indices",
            ));
        }
        for value in &sequence.values {
            let Value::I64(pattern) = value else {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "WOSM switch timing values must be I64",
                ));
            };
            if !(0..=255).contains(pattern) {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "WOSM switch timing values must be in 0..=255",
                ));
            }
        }
        Ok(())
    }

    fn program_timing_plan(&mut self, plan: &TimingPlan) -> Result<Value> {
        self.validate_timing_plan(plan)?;
        let sequences = self
            .local_timing_sequences(plan)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let mut programmed = Vec::new();
        for sequence in sequences {
            programmed.push(self.program_timing_sequence(&sequence)?);
        }
        Ok(Value::Map(BTreeMap::from([
            ("action".into(), Value::String("arm".into())),
            ("sequence_enabled".into(), Value::Bool(false)),
            ("programmed".into(), Value::List(programmed)),
            (
                "sequence_count".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
        ])))
    }

    fn program_timing_sequence(&mut self, sequence: &DeviceSequence) -> Result<Value> {
        self.validate_timing_sequence(sequence)?;
        for (index, value) in sequence.values.iter().enumerate() {
            let Value::I64(pattern) = value else {
                unreachable!("validated WOSM switch timing value")
            };
            self.send(
                protocol::WosmCommand::SequenceLoad {
                    index: index as u8,
                    value: *pattern as u8,
                },
                "timing_sequence_load",
            )?;
        }
        self.send(
            protocol::WosmCommand::SequenceCount {
                count: sequence.values.len() as u8,
            },
            "timing_sequence_count",
        )?;
        self.configured.sequence_enabled = false;
        self.emit_property(self.switch, "sequence_enabled", Value::Bool(false));
        Ok(Value::Map(BTreeMap::from([
            ("kind".into(), Value::String("switch_state".into())),
            ("steps".into(), Value::I64(sequence.values.len() as i64)),
        ])))
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

fn clamp_position(value: Position, travel: Position) -> Position {
    Position::from_micrometers(value.micrometers().clamp(0.0, travel.micrometers()))
}

fn validate_ratio(value: Ratio, label: &str) -> Result<()> {
    if (0.0..=100.0).contains(&value.percent()) && value.percent().is_finite() {
        Ok(())
    } else {
        Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("{label} must be in 0..=100 percent"),
        ))
    }
}

fn parse_wosm_reply_integer(reply: &str, tag: &str) -> Option<i64> {
    reply
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with(tag) {
                return None;
            }
            trimmed
                .split(|ch: char| ch == ',' || ch.is_whitespace())
                .rev()
                .find_map(parse_integer_token)
        })
        .last()
}

fn parse_integer_token(token: &str) -> Option<i64> {
    let token = token.trim_matches(|ch: char| ch == '\0' || ch == ',' || ch == ';');
    token
        .strip_prefix("0x")
        .or_else(|| token.strip_prefix("0X"))
        .and_then(|hex| i64::from_str_radix(hex, 16).ok())
        .or_else(|| token.parse::<i64>().ok())
}

fn invalid_property<T>(prefix: &str, key: &str) -> Result<T> {
    Err(Error::new(
        ErrorCode::InvalidProperty,
        format!("{prefix} {key}"),
    ))
}

fn source_metadata() -> BTreeMap<String, Value> {
    BTreeMap::from([("source".into(), Value::String("reverse engineered".into()))])
}

fn input_properties() -> Vec<PropertySchema> {
    let mut properties = vec![integer_range_property(
        "digital_input",
        "Digital input",
        false,
        0,
        63,
    )];
    properties.push(integer_range_property(
        "input_pullups",
        "Input pull-ups",
        true,
        0,
        63,
    ));
    for index in 1..=6 {
        properties.push(ratio_property(
            &format!("analog_input_{index}"),
            &format!("Analog input {index}"),
            false,
        ));
        properties.push(property(
            &format!("analog_input_{index}_raw"),
            &format!("Analog input {index} raw"),
            ValueType::I64,
            None,
            false,
            None,
        ));
    }
    properties
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
        sequenceable: key == "state",
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

fn time_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::TimeInterval,
        None,
        writable,
        None,
    )
}

fn integer_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::I64, None, writable, None)
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

fn ratio_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Ratio,
        Some("percent"),
        writable,
        Some(Range {
            min: Value::Ratio(Ratio::from_percent(0.0)),
            max: Value::Ratio(Ratio::from_percent(100.0)),
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

fn u64_prop(device: &DeviceConfig, key: &str) -> Option<u64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => (*value).try_into().ok(),
        Some(Value::TimeInterval(value)) => {
            let ms = value.seconds() * 1_000.0;
            if ms.is_finite() && ms >= 0.0 && ms <= u64::MAX as f64 {
                Some(ms.round() as u64)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn map_tcp_error(error: std::io::Error) -> Error {
    Error::new(ErrorCode::Transport, error.to_string())
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn i64_prop(device: &DeviceConfig, key: &str) -> Option<i64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => Some(*value),
        _ => None,
    }
}

fn u16_prop(device: &DeviceConfig, key: &str) -> Option<u16> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => u16::try_from(*value).ok(),
        _ => None,
    }
}

fn u8_prop(device: &DeviceConfig, key: &str) -> Option<u8> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => u8::try_from(*value).ok(),
        _ => None,
    }
}

fn position_prop(device: &DeviceConfig, key: &str) -> Option<Position> {
    match device.properties.get(key) {
        Some(Value::Position(value)) => Some(*value),
        _ => None,
    }
}

fn ratio_prop(device: &DeviceConfig, key: &str) -> Option<Ratio> {
    match device.properties.get(key) {
        Some(Value::Ratio(value)) => Some(*value),
        _ => None,
    }
}
