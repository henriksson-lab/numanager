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

    pub const SEND_ENDING: LineEnding = LineEnding::Cr;
    pub const RECV_ENDING: LineEnding = LineEnding::Cr;
    pub const PROBE_COMMANDS: [PriorCommand; 9] = [
        PriorCommand::StandardMode,
        PriorCommand::Date,
        PriorCommand::Status,
        PriorCommand::QueryX,
        PriorCommand::QueryY,
        PriorCommand::QueryZ,
        PriorCommand::QueryZResolution,
        PriorCommand::QueryShutter { shutter: 1 },
        PriorCommand::QueryTtl { line: 0 },
    ];

    #[derive(Debug, Clone, PartialEq)]
    pub struct PriorProbe {
        pub model: String,
        pub firmware_date: String,
        pub x_travel_um: f64,
        pub y_travel_um: f64,
        pub z_travel_um: f64,
        pub step_size_xy_um: f64,
        pub step_size_z_um: f64,
        pub wheel_positions: u8,
    }

    impl PriorProbe {
        pub fn simulated() -> Self {
            Self {
                model: "Prior ProScan III".into(),
                firmware_date: "numanager-sim".into(),
                x_travel_um: 110_000.0,
                y_travel_um: 75_000.0,
                z_travel_um: 12_000.0,
                step_size_xy_um: 1.0,
                step_size_z_um: 0.1,
                wheel_positions: 10,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct PriorProbeResult {
        pub probe: PriorProbe,
        pub busy: bool,
        pub x_um: f64,
        pub y_um: f64,
        pub z_um: f64,
        pub z_resolution_um: f64,
        pub shutter_open: Option<bool>,
        pub ttl_high: Option<bool>,
        pub replies: Vec<(String, String)>,
    }

    impl PriorProbeResult {
        pub fn from_replies(replies: &[(impl AsRef<str>, impl AsRef<str>)]) -> Result<Self> {
            let mut probe = PriorProbe::simulated();
            let mut busy = false;
            let mut x_um = 0.0;
            let mut y_um = 0.0;
            let mut z_steps = 0;
            let mut z_resolution_um = probe.step_size_z_um;
            let mut shutter_open = None;
            let mut ttl_high = None;
            let mut stored = Vec::new();
            for (command, reply) in replies {
                let command = command.as_ref();
                let reply = reply.as_ref().trim();
                stored.push((command.to_string(), reply.to_string()));
                match command {
                    "COMP 0" => check_ack(reply)?,
                    "DATE" => probe.firmware_date = parse_text_reply(reply),
                    "$" => busy = is_busy_status(reply)?,
                    "PX" => x_um = parse_i64_reply("PX", reply)? as f64 * probe.step_size_xy_um,
                    "PY" => y_um = parse_i64_reply("PY", reply)? as f64 * probe.step_size_xy_um,
                    "PZ" => z_steps = parse_i64_reply("PZ", reply)?,
                    "RES,Z" => {
                        z_resolution_um = parse_f64_reply("RES,Z", reply)?;
                        if z_resolution_um.is_finite() && z_resolution_um > 0.0 {
                            probe.step_size_z_um = z_resolution_um;
                        }
                    }
                    "8,1" => shutter_open = Some(parse_boolish_reply(reply)?),
                    "TTL,0,?" => ttl_high = Some(parse_boolish_reply(reply)?),
                    _ => {}
                }
            }
            Ok(Self {
                z_um: z_steps as f64 * probe.step_size_z_um,
                probe,
                busy,
                x_um,
                y_um,
                z_resolution_um,
                shutter_open,
                ttl_high,
                replies: stored,
            })
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum PriorCommand {
        StandardMode,
        Status,
        Date,
        MoveXyAbs { x_steps: i64, y_steps: i64 },
        MoveXyRel { dx_steps: i64, dy_steps: i64 },
        SetXyOrigin,
        QueryX,
        QueryY,
        HomeXy,
        Halt,
        QueryXySpeed,
        SetXySpeed(u8),
        QueryXyAcceleration,
        SetXyAcceleration(u8),
        QueryZ,
        MoveZRel { dz_steps: i64 },
        SetZOrigin,
        QueryZResolution,
        SetWheel { wheel: u8, position: u8 },
        HomeWheel { wheel: u8 },
        SetShutter { shutter: u8, open: bool },
        QueryShutter { shutter: u8 },
        SetTtl { line: u8, high: bool },
        QueryTtl { line: u8 },
        MoveNanoZAbs { position_um: f64 },
        QueryNanoZ,
        SetLumenIntensity(u8),
        SetLumenOpen { intensity: u8, open: bool },
    }

    pub fn encode(command: &PriorCommand) -> String {
        match command {
            PriorCommand::StandardMode => "COMP 0".into(),
            PriorCommand::Status => "$".into(),
            PriorCommand::Date => "DATE".into(),
            PriorCommand::MoveXyAbs { x_steps, y_steps } => format!("G,{x_steps},{y_steps}"),
            PriorCommand::MoveXyRel { dx_steps, dy_steps } => format!("GR,{dx_steps},{dy_steps}"),
            PriorCommand::SetXyOrigin => "PS,0,0".into(),
            PriorCommand::QueryX => "PX".into(),
            PriorCommand::QueryY => "PY".into(),
            PriorCommand::HomeXy => "SIS".into(),
            PriorCommand::Halt => "K".into(),
            PriorCommand::QueryXySpeed => "SMS".into(),
            PriorCommand::SetXySpeed(speed) => format!("SMS,{}", (*speed).min(100)),
            PriorCommand::QueryXyAcceleration => "SAS".into(),
            PriorCommand::SetXyAcceleration(accel) => format!("SAS,{}", (*accel).min(100)),
            PriorCommand::QueryZ => "PZ".into(),
            PriorCommand::MoveZRel { dz_steps } if *dz_steps >= 0 => format!("U,{dz_steps}"),
            PriorCommand::MoveZRel { dz_steps } => format!("D,{}", dz_steps.abs()),
            PriorCommand::SetZOrigin => "PZ,0".into(),
            PriorCommand::QueryZResolution => "RES,Z".into(),
            PriorCommand::SetWheel { wheel, position } => format!("7,{wheel},{position}"),
            PriorCommand::HomeWheel { wheel } => format!("7,{wheel},h"),
            PriorCommand::SetShutter { shutter, open } => {
                format!("8,{shutter},{}", if *open { 0 } else { 1 })
            }
            PriorCommand::QueryShutter { shutter } => format!("8,{shutter}"),
            PriorCommand::SetTtl { line, high } => format!("TTL,{line},{}", u8::from(*high)),
            PriorCommand::QueryTtl { line } => format!("TTL,{line},?"),
            PriorCommand::MoveNanoZAbs { position_um } => format!("V {position_um:.3}"),
            PriorCommand::QueryNanoZ => "PZ".into(),
            PriorCommand::SetLumenIntensity(intensity) => {
                format!("Light,{}", (*intensity).min(100))
            }
            PriorCommand::SetLumenOpen { intensity, open } => {
                format!("Light,{}", if *open { (*intensity).min(100) } else { 0 })
            }
        }
    }

    pub fn steps(um: f64, step_size_um: f64) -> i64 {
        if um >= 0.0 {
            (um / step_size_um + 0.5) as i64
        } else {
            (um / step_size_um - 0.5) as i64
        }
    }

    pub fn um(steps: i64, step_size_um: f64) -> f64 {
        steps as f64 * step_size_um
    }

    pub fn is_busy_status(reply: &str) -> Result<bool> {
        let status = reply
            .trim()
            .chars()
            .next()
            .ok_or_else(|| Error::new(ErrorCode::Transport, "empty Prior status reply"))?
            .to_digit(10)
            .ok_or_else(|| Error::new(ErrorCode::Transport, "invalid Prior status reply"))?;
        Ok((status & 0b111) != 0)
    }

    pub fn check_ack(reply: &str) -> Result<()> {
        let reply = reply.trim();
        if reply.starts_with('R') || reply == "0" {
            Ok(())
        } else if reply.starts_with('E') {
            Err(Error::new(
                ErrorCode::Transport,
                format!("Prior controller error: {reply}"),
            ))
        } else {
            Err(Error::new(
                ErrorCode::Transport,
                format!("unexpected Prior acknowledgement: {reply}"),
            ))
        }
    }

    pub fn probe_script() -> Vec<String> {
        PROBE_COMMANDS.iter().map(encode).collect()
    }

    pub fn execute_probe_script(
        serial: &mut dyn SerialIo,
        polls_per_command: usize,
    ) -> Result<PriorProbeResult> {
        let mut codec = SerialLineCodec::new(SEND_ENDING, RECV_ENDING);
        let mut replies = Vec::new();
        for command in PROBE_COMMANDS {
            let encoded = encode(&command);
            serial.write(&codec.encode(&encoded))?;
            let mut reply = None;
            for _ in 0..polls_per_command.max(1) {
                let bytes = serial.read_available()?;
                for line in codec.push(&bytes) {
                    reply = Some(line);
                    break;
                }
                if reply.is_some() {
                    break;
                }
            }
            let reply = reply.ok_or_else(|| {
                Error::new(
                    ErrorCode::Transport,
                    format!("timed out waiting for Prior probe reply to {encoded}"),
                )
            })?;
            replies.push((encoded, reply));
        }
        PriorProbeResult::from_replies(&replies)
    }

    pub(crate) fn parse_text_reply(reply: &str) -> String {
        reply
            .trim()
            .trim_start_matches('R')
            .trim_start_matches(',')
            .trim()
            .to_string()
    }

    pub(crate) fn parse_i64_reply(command: &str, reply: &str) -> Result<i64> {
        let value = parse_text_reply(reply);
        value.parse::<i64>().map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("invalid Prior {command} integer {value}: {error}"),
            )
        })
    }

    pub(crate) fn parse_f64_reply(command: &str, reply: &str) -> Result<f64> {
        let value = parse_text_reply(reply);
        value.parse::<f64>().map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("invalid Prior {command} number {value}: {error}"),
            )
        })
    }

    pub(crate) fn parse_boolish_reply(reply: &str) -> Result<bool> {
        let value = parse_text_reply(reply);
        match value.as_str() {
            "1" | "OPEN" | "Open" | "open" | "HIGH" | "High" | "high" => Ok(true),
            "0" | "CLOSED" | "Closed" | "closed" | "LOW" | "Low" | "low" => Ok(false),
            other => Err(Error::new(
                ErrorCode::Transport,
                format!("invalid Prior boolean reply {other}"),
            )),
        }
    }
}

pub struct PriorDiscovery {
    next_id: DriverId,
    probes: Vec<PriorConfiguredProbe>,
}

impl PriorDiscovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![PriorConfiguredProbe::simulated()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), "prior" | "prior-proscan"))
            .map(PriorConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for PriorDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver = if configured.connect_real_transport {
                    Box::new(PriorDriver::serial(id, configured)?) as Box<dyn Driver>
                } else {
                    Box::new(PriorDriver::configured_fixture(id, configured)) as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct PriorSerialEndpoint {
    pub port_name: String,
    pub baud_rate: u32,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone)]
pub struct PriorConfiguredProbe {
    pub label: String,
    pub probe: protocol::PriorProbe,
    pub endpoint: Option<PriorSerialEndpoint>,
    pub connect_real_transport: bool,
    pub x_um: f64,
    pub y_um: f64,
    pub z_um: f64,
    pub nano_z_um: f64,
    pub xy_speed: u8,
    pub xy_acceleration: u8,
    pub wheel_position: u8,
    pub shutter_open: bool,
    pub ttl_high: bool,
    pub lumen_open: bool,
    pub lumen_intensity: u8,
    pub lumen_delay_ms: f64,
    pub busy: bool,
}

impl PriorConfiguredProbe {
    pub fn simulated() -> Self {
        Self {
            label: "Simulated Prior ProScan controller".into(),
            probe: protocol::PriorProbe::simulated(),
            endpoint: None,
            connect_real_transport: false,
            x_um: 0.0,
            y_um: 0.0,
            z_um: 0.0,
            nano_z_um: 0.0,
            xy_speed: 20,
            xy_acceleration: 20,
            wheel_position: 1,
            shutter_open: false,
            ttl_high: false,
            lumen_open: false,
            lumen_intensity: 100,
            lumen_delay_ms: 0.0,
            busy: false,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::simulated();
        configured.label = if device.label.is_empty() {
            "Configured Prior ProScan controller".into()
        } else {
            device.label.clone()
        };
        configured.probe.model =
            string_prop(device, "model").unwrap_or_else(|| configured.probe.model.clone());
        configured.probe.firmware_date = string_prop(device, "firmware_date")
            .unwrap_or_else(|| configured.probe.firmware_date.clone());
        configured.probe.x_travel_um = position_config_um(device, "x_travel", "x_travel_um")
            .unwrap_or(configured.probe.x_travel_um);
        configured.probe.y_travel_um = position_config_um(device, "y_travel", "y_travel_um")
            .unwrap_or(configured.probe.y_travel_um);
        configured.probe.z_travel_um = position_config_um(device, "z_travel", "z_travel_um")
            .unwrap_or(configured.probe.z_travel_um);
        configured.probe.step_size_xy_um =
            position_config_um(device, "step_size_xy", "step_size_xy_um")
                .unwrap_or(configured.probe.step_size_xy_um);
        configured.probe.step_size_z_um =
            position_config_um(device, "step_size_z", "step_size_z_um")
                .unwrap_or(configured.probe.step_size_z_um);
        configured.probe.wheel_positions =
            u8_prop(device, "wheel_positions").unwrap_or(configured.probe.wheel_positions);
        configured.endpoint =
            string_prop(device, "serial_port").map(|port_name| PriorSerialEndpoint {
                port_name,
                baud_rate: u32_prop(device, "baud_rate").unwrap_or(9_600),
                timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(100),
            });
        configured.connect_real_transport = bool_prop(device, "connect").unwrap_or(false);
        configured.x_um = position_config_um(device, "x", "x_um")
            .unwrap_or(configured.x_um)
            .clamp(0.0, configured.probe.x_travel_um);
        configured.y_um = position_config_um(device, "y", "y_um")
            .unwrap_or(configured.y_um)
            .clamp(0.0, configured.probe.y_travel_um);
        configured.z_um = position_config_um(device, "z", "z_um")
            .unwrap_or(configured.z_um)
            .clamp(0.0, configured.probe.z_travel_um);
        configured.nano_z_um = position_config_um(device, "nano_z", "nano_z_um")
            .unwrap_or(configured.nano_z_um)
            .clamp(0.0, configured.probe.z_travel_um);
        configured.xy_speed = u8_prop(device, "xy_speed")
            .unwrap_or(configured.xy_speed)
            .clamp(1, 100);
        configured.xy_acceleration = u8_prop(device, "xy_acceleration")
            .unwrap_or(configured.xy_acceleration)
            .clamp(1, 100);
        configured.wheel_position = u8_prop(device, "wheel_position")
            .unwrap_or(configured.wheel_position)
            .clamp(1, configured.probe.wheel_positions.max(1));
        configured.shutter_open =
            bool_prop(device, "shutter_open").unwrap_or(configured.shutter_open);
        configured.ttl_high = bool_prop(device, "ttl_high").unwrap_or(configured.ttl_high);
        configured.lumen_open = bool_prop(device, "lumen_open").unwrap_or(configured.lumen_open);
        configured.lumen_intensity = u8_prop(device, "lumen_intensity")
            .unwrap_or(configured.lumen_intensity)
            .clamp(0, 100);
        configured.lumen_delay_ms = time_config_ms(device, "lumen_delay", "lumen_delay_ms")
            .unwrap_or(configured.lumen_delay_ms)
            .clamp(0.0, 1000.0);
        configured.busy = bool_prop(device, "busy").unwrap_or(configured.busy);
        Ok(configured)
    }
}

pub struct PriorDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    xy: DeviceId,
    z: DeviceId,
    wheel: DeviceId,
    shutter: DeviceId,
    ttl: DeviceId,
    nano_z: DeviceId,
    lumen: DeviceId,
    probe: protocol::PriorProbe,
    x_um: f64,
    y_um: f64,
    z_um: f64,
    nano_z_um: f64,
    xy_speed: u8,
    xy_acceleration: u8,
    wheel_position: u8,
    shutter_open: bool,
    ttl_high: bool,
    lumen_open: bool,
    lumen_intensity: u8,
    lumen_delay_ms: f64,
    busy: bool,
    last_ack: String,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
}

impl PriorDriver {
    pub fn simulated(id: DriverId) -> Self {
        Self::configured_fixture(id, PriorConfiguredProbe::simulated())
    }

    pub fn configured_fixture(id: DriverId, configured: PriorConfiguredProbe) -> Self {
        let serial = ScriptedSerial::new();
        Self::new_configured(id, configured, Box::new(serial))
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: PriorConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Prior serial probe is missing serial_port metadata",
            )
        })?;
        let mut serial = numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(endpoint.port_name, endpoint.baud_rate)
                .timeout(Duration::from_millis(endpoint.timeout_ms)),
        )?;
        let probe_result = protocol::execute_probe_script(&mut serial, 4)?;
        let mut driver = Self::new_configured(id, configured, Box::new(serial));
        driver.connected = true;
        Ok(driver.with_probe_result(probe_result))
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _configured: PriorConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Prior real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new_configured(
        id: DriverId,
        configured: PriorConfiguredProbe,
        serial: Box<dyn SerialIo>,
    ) -> Self {
        let mut driver = Self::new(id, configured.probe, serial);
        driver.x_um = configured.x_um;
        driver.y_um = configured.y_um;
        driver.z_um = configured.z_um;
        driver.nano_z_um = configured.nano_z_um;
        driver.xy_speed = configured.xy_speed;
        driver.xy_acceleration = configured.xy_acceleration;
        driver.wheel_position = configured.wheel_position;
        driver.shutter_open = configured.shutter_open;
        driver.ttl_high = configured.ttl_high;
        driver.lumen_open = configured.lumen_open;
        driver.lumen_intensity = configured.lumen_intensity;
        driver.lumen_delay_ms = configured.lumen_delay_ms;
        driver.busy = configured.busy;
        driver.serial_port = configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.port_name.clone());
        driver.baud_rate = configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.baud_rate)
            .unwrap_or(9_600);
        driver.serial_timeout_ms = configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.timeout_ms)
            .unwrap_or(100);
        driver.connected = false;
        driver
    }

    #[cfg(feature = "os-serial")]
    fn with_probe_result(mut self, probe_result: protocol::PriorProbeResult) -> Self {
        self.probe = probe_result.probe;
        self.x_um = probe_result.x_um.clamp(0.0, self.probe.x_travel_um);
        self.y_um = probe_result.y_um.clamp(0.0, self.probe.y_travel_um);
        self.z_um = probe_result.z_um.clamp(0.0, self.probe.z_travel_um);
        self.nano_z_um = self.z_um;
        self.shutter_open = probe_result.shutter_open.unwrap_or(self.shutter_open);
        self.ttl_high = probe_result.ttl_high.unwrap_or(self.ttl_high);
        self.busy = probe_result.busy;
        self
    }

    pub fn new(id: DriverId, probe: protocol::PriorProbe, serial: Box<dyn SerialIo>) -> Self {
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 1201)),
            hub: DeviceId(NodeId(id.0 * 1000 + 1210)),
            xy: DeviceId(NodeId(id.0 * 1000 + 1211)),
            z: DeviceId(NodeId(id.0 * 1000 + 1212)),
            wheel: DeviceId(NodeId(id.0 * 1000 + 1213)),
            shutter: DeviceId(NodeId(id.0 * 1000 + 1214)),
            ttl: DeviceId(NodeId(id.0 * 1000 + 1215)),
            nano_z: DeviceId(NodeId(id.0 * 1000 + 1216)),
            lumen: DeviceId(NodeId(id.0 * 1000 + 1217)),
            probe,
            x_um: 0.0,
            y_um: 0.0,
            z_um: 0.0,
            nano_z_um: 0.0,
            xy_speed: 20,
            xy_acceleration: 20,
            wheel_position: 1,
            shutter_open: false,
            ttl_high: false,
            lumen_open: false,
            lumen_intensity: 100,
            lumen_delay_ms: 0.0,
            busy: false,
            last_ack: "0".into(),
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            codec: SerialLineCodec::new(protocol::SEND_ENDING, protocol::RECV_ENDING),
            serial_port: None,
            baud_rate: 9_600,
            serial_timeout_ms: 100,
            connected: false,
        }
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn send(&mut self, command: protocol::PriorCommand) -> Result<()> {
        let line = protocol::encode(&command);
        self.serial.write(&self.codec.encode(&line))
    }

    fn read_optional_ack(&mut self) -> Result<bool> {
        let bytes = self.serial.read_available()?;
        if bytes.is_empty() {
            return Ok(false);
        }
        let mut saw_reply = false;
        for line in self.codec.push(&bytes) {
            self.last_ack = line.trim().to_string();
            self.emit_property(self.hub, "last_ack", Value::String(self.last_ack.clone()));
            self.emit_property(self.hub, "fault", Value::Bool(self.ack_is_fault()));
            protocol::check_ack(&line)?;
            saw_reply = true;
        }
        Ok(saw_reply)
    }

    fn ack_is_fault(&self) -> bool {
        self.last_ack.trim().starts_with('E')
    }

    fn query_for_property(&self, device: DeviceId, key: &str) -> Option<protocol::PriorCommand> {
        match (device, key) {
            (device, "firmware_date") if device == self.hub => Some(protocol::PriorCommand::Date),
            (device, "last_ack") | (device, "fault") if device == self.hub => None,
            (device, "state_summary") if device == self.hub => Some(protocol::PriorCommand::Status),
            (device, "busy")
                if device == self.hub
                    || device == self.xy
                    || device == self.z
                    || device == self.wheel
                    || device == self.nano_z =>
            {
                Some(protocol::PriorCommand::Status)
            }
            (device, "x") if device == self.xy => Some(protocol::PriorCommand::QueryX),
            (device, "y") if device == self.xy => Some(protocol::PriorCommand::QueryY),
            (device, "speed") if device == self.xy => Some(protocol::PriorCommand::QueryXySpeed),
            (device, "acceleration") if device == self.xy => {
                Some(protocol::PriorCommand::QueryXyAcceleration)
            }
            (device, "z") if device == self.z => Some(protocol::PriorCommand::QueryZ),
            (device, "z") | (device, "position_steps") if device == self.nano_z => {
                Some(protocol::PriorCommand::QueryNanoZ)
            }
            (device, "open") if device == self.shutter => {
                Some(protocol::PriorCommand::QueryShutter { shutter: 1 })
            }
            (device, "high") if device == self.ttl => {
                Some(protocol::PriorCommand::QueryTtl { line: 0 })
            }
            _ => None,
        }
    }

    fn read_query_reply(
        &mut self,
        device: DeviceId,
        command: &protocol::PriorCommand,
    ) -> Result<()> {
        let bytes = self.serial.read_available()?;
        if bytes.is_empty() {
            return Ok(());
        }
        for line in self.codec.push(&bytes) {
            self.apply_readback_reply(device, command, &line)?;
        }
        Ok(())
    }

    fn refresh_command_readback(
        &mut self,
        device: DeviceId,
        commands: &[protocol::PriorCommand],
    ) -> Result<()> {
        for command in commands {
            self.send(command.clone())?;
            self.read_query_reply(device, command)?;
        }
        Ok(())
    }

    fn refresh_stage_readback(&mut self, device: DeviceId) -> Result<()> {
        if device == self.xy {
            self.refresh_command_readback(
                device,
                &[
                    protocol::PriorCommand::Status,
                    protocol::PriorCommand::QueryX,
                    protocol::PriorCommand::QueryY,
                ],
            )
        } else if device == self.z {
            self.refresh_command_readback(
                device,
                &[
                    protocol::PriorCommand::Status,
                    protocol::PriorCommand::QueryZ,
                ],
            )
        } else if device == self.nano_z {
            self.refresh_command_readback(
                device,
                &[
                    protocol::PriorCommand::Status,
                    protocol::PriorCommand::QueryNanoZ,
                ],
            )
        } else {
            Ok(())
        }
    }

    fn refresh_targets_for(command: &str) -> Result<Vec<(u8, &'static str)>> {
        match command {
            "refresh_readbacks" => Ok(vec![
                (0, "firmware_date"),
                (0, "state_summary"),
                (1, "x"),
                (1, "y"),
                (2, "z"),
                (3, "z"),
                (1, "speed"),
                (1, "acceleration"),
                (4, "open"),
                (5, "high"),
            ]),
            "refresh_identity" => Ok(vec![(0, "firmware_date")]),
            "refresh_status" => Ok(vec![(0, "state_summary")]),
            "refresh_position" => Ok(vec![(1, "x"), (1, "y"), (2, "z"), (3, "z")]),
            "refresh_profiles" => Ok(vec![(1, "speed"), (1, "acceleration")]),
            "refresh_outputs" => Ok(vec![(4, "open"), (5, "high")]),
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "Prior GenericCommand supports refresh_readbacks, refresh_identity, refresh_status, refresh_position, refresh_profiles, and refresh_outputs; got {other}"
                ),
            )),
        }
    }

    fn actual_refresh_target(&self, target: u8, key: &'static str) -> (DeviceId, &'static str) {
        let device = match target {
            0 => self.hub,
            1 => self.xy,
            2 => self.z,
            3 => self.nano_z,
            4 => self.shutter,
            _ => self.ttl,
        };
        (device, key)
    }

    fn validate_generic_command(&self, request: &GenericCommandRequest) -> Result<()> {
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
                "Prior GenericCommand does not take parameters",
            ));
        }
        let _ = Self::refresh_targets_for(&request.command)?;
        Ok(())
    }

    fn apply_generic_command(&mut self, request: GenericCommandRequest) -> Result<Value> {
        self.validate_generic_command(&request)?;
        let targets = Self::refresh_targets_for(&request.command)?;
        for (device, key) in &targets {
            let (device, key) = self.actual_refresh_target(*device, key);
            if let Some(query) = self.query_for_property(device, key) {
                self.send(query.clone())?;
                self.read_query_reply(device, &query)?;
            }
        }
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("commands".into(), Value::I64(targets.len() as i64)),
            ("state".into(), self.state_summary()),
            (
                "completion_basis".into(),
                Value::String("Prior mapped query readback".into()),
            ),
        ])))
    }

    fn apply_readback_reply(
        &mut self,
        device: DeviceId,
        command: &protocol::PriorCommand,
        reply: &str,
    ) -> Result<()> {
        match command {
            protocol::PriorCommand::Date => {
                self.probe.firmware_date = protocol::parse_text_reply(reply);
                self.emit_property(
                    self.hub,
                    "firmware_date",
                    Value::String(self.probe.firmware_date.clone()),
                );
            }
            protocol::PriorCommand::Status => {
                self.busy = protocol::is_busy_status(reply)?;
                for target in [self.hub, self.xy, self.z, self.wheel, self.nano_z] {
                    self.emit_property(target, "busy", Value::Bool(self.busy));
                }
                if device == self.hub {
                    self.emit_property(self.hub, "state_summary", self.state_summary());
                }
            }
            protocol::PriorCommand::QueryX => {
                self.x_um = protocol::um(
                    protocol::parse_i64_reply("PX", reply)?,
                    self.probe.step_size_xy_um,
                );
                self.emit_property(self.xy, "x", position(self.x_um));
            }
            protocol::PriorCommand::QueryY => {
                self.y_um = protocol::um(
                    protocol::parse_i64_reply("PY", reply)?,
                    self.probe.step_size_xy_um,
                );
                self.emit_property(self.xy, "y", position(self.y_um));
            }
            protocol::PriorCommand::QueryXySpeed => {
                let speed = protocol::parse_i64_reply("SMS", reply)?.clamp(1, 100) as u8;
                self.xy_speed = speed;
                self.emit_property(self.xy, "speed", percent_ratio(speed));
            }
            protocol::PriorCommand::QueryXyAcceleration => {
                let acceleration = protocol::parse_i64_reply("SAS", reply)?.clamp(1, 100) as u8;
                self.xy_acceleration = acceleration;
                self.emit_property(self.xy, "acceleration", percent_ratio(acceleration));
            }
            protocol::PriorCommand::QueryZ if device == self.z => {
                self.z_um = protocol::um(
                    protocol::parse_i64_reply("PZ", reply)?,
                    self.probe.step_size_z_um,
                );
                self.emit_property(self.z, "z", position(self.z_um));
            }
            protocol::PriorCommand::QueryNanoZ if device == self.nano_z => {
                let steps = protocol::parse_i64_reply("PZ", reply)?;
                self.nano_z_um = protocol::um(steps, 0.001);
                self.emit_property(self.nano_z, "z", position(self.nano_z_um));
                self.emit_property(self.nano_z, "position_steps", step_count(steps));
            }
            protocol::PriorCommand::QueryShutter { .. } => {
                self.shutter_open = protocol::parse_boolish_reply(reply)?;
                self.emit_property(self.shutter, "open", Value::Bool(self.shutter_open));
            }
            protocol::PriorCommand::QueryTtl { .. } => {
                self.ttl_high = protocol::parse_boolish_reply(reply)?;
                self.emit_property(self.ttl, "high", Value::Bool(self.ttl_high));
            }
            _ => {}
        }
        Ok(())
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        vec![
            DeviceDescriptor {
                id: self.hub,
                driver: self.id,
                label: "prior-proscan-hub".into(),
                vendor: Some("Prior Scientific".into()),
                model: Some(self.probe.model.clone()),
                serial: None,
                kinds: vec![
                    "hub".into(),
                    "motion.controller".into(),
                    "serial.ascii".into(),
                ],
                properties: vec![
                    property("model", "Model", ValueType::String, None, false, None),
                    property(
                        "firmware_date",
                        "Firmware date",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                    property(
                        "state_summary",
                        "State summary",
                        ValueType::Map,
                        None,
                        false,
                        None,
                    ),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                    property(
                        "last_ack",
                        "Last acknowledgement",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                    property("fault", "Fault", ValueType::Bool, None, false, None),
                ],
                metadata: BTreeMap::from([
                    ("model".into(), Value::String(self.probe.model.clone())),
                    (
                        "firmware_date".into(),
                        Value::String(self.probe.firmware_date.clone()),
                    ),
                    ("compatibility_mode".into(), Value::String("COMP 0".into())),
                ]),
            },
            DeviceDescriptor {
                id: self.xy,
                driver: self.id,
                label: "prior-xy-stage".into(),
                vendor: Some("Prior Scientific".into()),
                model: Some("ProScan XY".into()),
                serial: None,
                kinds: vec!["axis.xy".into(), "stage.xy".into()],
                properties: vec![
                    sequenceable_position_property("x", "X position", true, self.probe.x_travel_um),
                    sequenceable_position_property("y", "Y position", true, self.probe.y_travel_um),
                    ratio_property_range("speed", "XY speed", Some("percent"), true, 1.0, 100.0),
                    ratio_property_range(
                        "acceleration",
                        "XY acceleration",
                        Some("percent"),
                        true,
                        1.0,
                        100.0,
                    ),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                ],
                metadata: BTreeMap::from([
                    ("x_travel".into(), position(self.probe.x_travel_um)),
                    ("y_travel".into(), position(self.probe.y_travel_um)),
                    ("step_size_xy".into(), position(self.probe.step_size_xy_um)),
                    (
                        "legacy_x_travel_um".into(),
                        position(self.probe.x_travel_um),
                    ),
                    (
                        "legacy_y_travel_um".into(),
                        position(self.probe.y_travel_um),
                    ),
                    (
                        "legacy_step_size_xy_um".into(),
                        position(self.probe.step_size_xy_um),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.z,
                driver: self.id,
                label: "prior-z-stage".into(),
                vendor: Some("Prior Scientific".into()),
                model: Some("ProScan Z".into()),
                serial: None,
                kinds: vec!["axis.z".into(), "stage.z".into()],
                properties: vec![
                    sequenceable_position_property("z", "Z position", true, self.probe.z_travel_um),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                ],
                metadata: BTreeMap::from([
                    ("z_travel".into(), position(self.probe.z_travel_um)),
                    ("step_size_z".into(), position(self.probe.step_size_z_um)),
                    (
                        "legacy_z_travel_um".into(),
                        position(self.probe.z_travel_um),
                    ),
                    (
                        "legacy_step_size_z_um".into(),
                        position(self.probe.step_size_z_um),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.wheel,
                driver: self.id,
                label: "prior-filter-wheel-1".into(),
                vendor: Some("Prior Scientific".into()),
                model: Some("ProScan filter wheel".into()),
                serial: None,
                kinds: vec!["filter.wheel".into(), "state.device".into()],
                properties: vec![
                    property_i64_range(
                        "position",
                        "Position",
                        None,
                        true,
                        1,
                        self.probe.wheel_positions as i64,
                    ),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                ],
                metadata: BTreeMap::from([(
                    "positions".into(),
                    Value::I64(self.probe.wheel_positions as i64),
                )]),
            },
            DeviceDescriptor {
                id: self.shutter,
                driver: self.id,
                label: "prior-shutter-1".into(),
                vendor: Some("Prior Scientific".into()),
                model: Some("ProScan shutter".into()),
                serial: None,
                kinds: vec!["shutter".into(), "light.gate".into(), "trigger.sink".into()],
                properties: vec![sequenceable_property(
                    "open",
                    "Open",
                    ValueType::Bool,
                    None,
                    true,
                    None,
                )],
                metadata: BTreeMap::from([("shutter_id".into(), Value::I64(1))]),
            },
            DeviceDescriptor {
                id: self.ttl,
                driver: self.id,
                label: "prior-ttl-0".into(),
                vendor: Some("Prior Scientific".into()),
                model: Some("ProScan TTL".into()),
                serial: None,
                kinds: vec!["trigger.source".into(), "digital.output".into()],
                properties: vec![sequenceable_property(
                    "high",
                    "High",
                    ValueType::Bool,
                    None,
                    true,
                    None,
                )],
                metadata: BTreeMap::from([("ttl_line".into(), Value::I64(0))]),
            },
            DeviceDescriptor {
                id: self.nano_z,
                driver: self.id,
                label: "prior-nanoscan-z".into(),
                vendor: Some("Prior Scientific".into()),
                model: Some("NanoScanZ".into()),
                serial: None,
                kinds: vec!["axis.z".into(), "stage.z".into(), "piezo.z".into()],
                properties: vec![
                    sequenceable_position_property("z", "Z position", true, self.probe.z_travel_um),
                    property(
                        "position_steps",
                        "Position steps",
                        ValueType::StepCount,
                        Some("steps"),
                        false,
                        Some(Range {
                            min: step_count(0),
                            max: step_count((self.probe.z_travel_um / 0.001).round() as i64),
                        }),
                    ),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                ],
                metadata: BTreeMap::from([
                    ("step_size".into(), position(0.001)),
                    ("legacy_step_size_um".into(), position(0.001)),
                    (
                        "protocol".into(),
                        Value::String("NanoScanZ V <um> and PZ commands".into()),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.lumen,
                driver: self.id,
                label: "prior-lumen-200pro".into(),
                vendor: Some("Prior Scientific".into()),
                model: Some("Lumen 200Pro".into()),
                serial: None,
                kinds: vec![
                    "light.source".into(),
                    "shutter".into(),
                    "lamp".into(),
                    "trigger.sink".into(),
                ],
                properties: vec![
                    sequenceable_property("open", "Open", ValueType::Bool, None, true, None),
                    ratio_property_range(
                        "intensity",
                        "Intensity",
                        Some("percent"),
                        true,
                        0.0,
                        100.0,
                    ),
                    time_interval_range("delay", "Delay", Some("ms"), true, 0.0, 1000.0),
                ],
                metadata: BTreeMap::from([(
                    "protocol".into(),
                    Value::String("Lumen 200Pro Light,<intensity> command".into()),
                )]),
            },
        ]
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        match (device, key) {
            (device, "model") if device == self.hub => Ok(Value::String(self.probe.model.clone())),
            (device, "firmware_date") if device == self.hub => {
                Ok(Value::String(self.probe.firmware_date.clone()))
            }
            (device, "state_summary") if device == self.hub => Ok(self.state_summary()),
            (device, "busy")
                if device == self.hub
                    || device == self.xy
                    || device == self.z
                    || device == self.wheel
                    || device == self.nano_z =>
            {
                Ok(Value::Bool(self.busy))
            }
            (device, "last_ack") if device == self.hub => Ok(Value::String(self.last_ack.clone())),
            (device, "fault") if device == self.hub => Ok(Value::Bool(self.ack_is_fault())),
            (device, "x") if device == self.xy => Ok(position(self.x_um)),
            (device, "y") if device == self.xy => Ok(position(self.y_um)),
            (device, "speed") if device == self.xy => Ok(percent_ratio(self.xy_speed)),
            (device, "acceleration") if device == self.xy => {
                Ok(percent_ratio(self.xy_acceleration))
            }
            (device, "z") if device == self.z => Ok(position(self.z_um)),
            (device, "position") if device == self.wheel => {
                Ok(Value::I64(self.wheel_position as i64))
            }
            (device, "open") if device == self.shutter => Ok(Value::Bool(self.shutter_open)),
            (device, "high") if device == self.ttl => Ok(Value::Bool(self.ttl_high)),
            (device, "z") if device == self.nano_z => Ok(position(self.nano_z_um)),
            (device, "position_steps") if device == self.nano_z => {
                Ok(step_count((self.nano_z_um / 0.001).round() as i64))
            }
            (device, "open") if device == self.lumen => Ok(Value::Bool(self.lumen_open)),
            (device, "intensity") if device == self.lumen => {
                Ok(percent_ratio(self.lumen_intensity))
            }
            (device, "delay") if device == self.lumen => Ok(time_interval_ms(self.lumen_delay_ms)),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Prior property {key}"),
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

    fn write_property(&mut self, device: DeviceId, key: &str, value: &Value) -> Result<Value> {
        self.validate_write(device, key, value)?;
        match (device, key, value) {
            (device, "x", value) if device == self.xy => {
                self.move_xy(
                    position_um(value)?.clamp(0.0, self.probe.x_travel_um),
                    self.y_um,
                )?;
                Ok(position(self.x_um))
            }
            (device, "y", value) if device == self.xy => {
                self.move_xy(
                    self.x_um,
                    position_um(value)?.clamp(0.0, self.probe.y_travel_um),
                )?;
                Ok(position(self.y_um))
            }
            (device, "speed", Value::Ratio(speed)) if device == self.xy => {
                let speed = speed.percent().clamp(1.0, 100.0).round() as u8;
                self.send(protocol::PriorCommand::SetXySpeed(speed))?;
                self.read_optional_ack()?;
                self.xy_speed = speed;
                Ok(percent_ratio(speed))
            }
            (device, "acceleration", Value::Ratio(accel)) if device == self.xy => {
                let accel = accel.percent().clamp(1.0, 100.0).round() as u8;
                self.send(protocol::PriorCommand::SetXyAcceleration(accel))?;
                self.read_optional_ack()?;
                self.xy_acceleration = accel;
                Ok(percent_ratio(accel))
            }
            (device, "z", value) if device == self.z => {
                self.move_z(position_um(value)?.clamp(0.0, self.probe.z_travel_um))?;
                Ok(position(self.z_um))
            }
            (device, "position", Value::I64(pos)) if device == self.wheel => {
                let pos = *pos as u8;
                self.send(protocol::PriorCommand::SetWheel {
                    wheel: 1,
                    position: pos,
                })?;
                self.read_optional_ack()?;
                self.wheel_position = pos;
                self.finish_motion("prior filter wheel R");
                Ok(Value::I64(pos as i64))
            }
            (device, "open", Value::Bool(open)) if device == self.shutter => {
                self.send(protocol::PriorCommand::SetShutter {
                    shutter: 1,
                    open: *open,
                })?;
                self.read_optional_ack()?;
                self.shutter_open = *open;
                Ok(Value::Bool(*open))
            }
            (device, "high", Value::Bool(high)) if device == self.ttl => {
                self.send(protocol::PriorCommand::SetTtl {
                    line: 0,
                    high: *high,
                })?;
                self.read_optional_ack()?;
                self.ttl_high = *high;
                Ok(Value::Bool(*high))
            }
            (device, "z", value) if device == self.nano_z => {
                self.move_nano_z(position_um(value)?.clamp(0.0, self.probe.z_travel_um))?;
                Ok(position(self.nano_z_um))
            }
            (device, "open", Value::Bool(open)) if device == self.lumen => {
                self.send(protocol::PriorCommand::SetLumenOpen {
                    intensity: self.lumen_intensity,
                    open: *open,
                })?;
                self.read_optional_ack()?;
                self.lumen_open = *open;
                Ok(Value::Bool(*open))
            }
            (device, "intensity", Value::Ratio(intensity)) if device == self.lumen => {
                let intensity = intensity.percent().clamp(0.0, 100.0).round() as u8;
                if self.lumen_open {
                    self.send(protocol::PriorCommand::SetLumenIntensity(intensity))?;
                    self.read_optional_ack()?;
                }
                self.lumen_intensity = intensity;
                Ok(percent_ratio(intensity))
            }
            (device, "delay", value) if device == self.lumen => {
                self.lumen_delay_ms = time_ms(value)?.clamp(0.0, 1000.0);
                Ok(time_interval_ms(self.lumen_delay_ms))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid Prior write {key}"),
            )),
        }
    }

    fn owns_device(&self, device: DeviceId) -> bool {
        device == self.hub
            || device == self.xy
            || device == self.z
            || device == self.wheel
            || device == self.shutter
            || device == self.ttl
            || device == self.nano_z
            || device == self.lumen
    }

    fn has_timed_ttl(&self, plan: &TimingPlan) -> bool {
        plan.participants.contains(&self.ttl)
            || plan
                .routes
                .iter()
                .any(|route| route.from == self.ttl || route.to == self.ttl)
            || plan
                .sequences
                .iter()
                .any(|sequence| sequence.device == self.ttl)
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

    fn has_timed_lumen(&self, plan: &TimingPlan) -> bool {
        plan.participants.contains(&self.lumen)
            || plan
                .routes
                .iter()
                .any(|route| route.from == self.lumen || route.to == self.lumen)
            || plan
                .sequences
                .iter()
                .any(|sequence| sequence.device == self.lumen)
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

    fn local_stage_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| {
                sequence.device == self.xy
                    || sequence.device == self.z
                    || sequence.device == self.nano_z
            })
            .collect()
    }

    fn local_output_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| {
                sequence.device == self.ttl
                    || sequence.device == self.shutter
                    || sequence.device == self.lumen
            })
            .collect()
    }

    fn has_sequence_for(&self, plan: &TimingPlan, device: DeviceId, property: &str) -> bool {
        plan.sequences
            .iter()
            .any(|sequence| sequence.device == device && sequence.property == property)
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_stage_timing_sequences(plan) {
            match (sequence.device, sequence.property.as_str()) {
                (device, "x" | "y") if device == self.xy => {}
                (device, "z") if device == self.z || device == self.nano_z => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Prior timing stage sequences can only target XY x/y or Z z",
                    ))
                }
            }
            for value in &sequence.values {
                let _ = position_um(value)?;
            }
        }
        for sequence in self.local_output_timing_sequences(plan) {
            match (sequence.device, sequence.property.as_str()) {
                (device, "high") if device == self.ttl => {}
                (device, "open") if device == self.shutter || device == self.lumen => {}
                _ => return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Prior timing output sequences can only target TTL high or shutter/Lumen open",
                )),
            }
            for value in &sequence.values {
                if !matches!(value, Value::Bool(_)) {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Prior timing output sequences require Bool values",
                    ));
                }
            }
        }
        Ok(())
    }

    fn timing_summary(&self, plan: &TimingPlan, action: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("action".into(), Value::String(action.into())),
            ("hub".into(), Value::I64(self.hub.0 .0 as i64)),
            ("ttl".into(), Value::I64(self.ttl.0 .0 as i64)),
            ("shutter".into(), Value::I64(self.shutter.0 .0 as i64)),
            ("lumen".into(), Value::I64(self.lumen.0 .0 as i64)),
            ("timed_ttl".into(), Value::Bool(self.has_timed_ttl(plan))),
            (
                "timed_shutter".into(),
                Value::Bool(self.has_timed_shutter(plan)),
            ),
            (
                "timed_lumen".into(),
                Value::Bool(self.has_timed_lumen(plan)),
            ),
            ("ttl_high".into(), Value::Bool(self.ttl_high)),
            ("shutter_open".into(), Value::Bool(self.shutter_open)),
            ("lumen_open".into(), Value::Bool(self.lumen_open)),
            ("x".into(), position(self.x_um)),
            ("y".into(), position(self.y_um)),
            ("z".into(), position(self.z_um)),
            ("nano_z".into(), position(self.nano_z_um)),
            (
                "lumen_intensity".into(),
                Value::I64(self.lumen_intensity as i64),
            ),
            ("lumen_delay".into(), time_interval_ms(self.lumen_delay_ms)),
            ("routes".into(), Value::List(self.local_timing_routes(plan))),
            (
                "sequences".into(),
                Value::List(self.local_timing_sequences(plan)),
            ),
        ]))
    }

    fn apply_timing_sequence_step(&mut self, plan: &TimingPlan, first: bool) -> Result<Value> {
        let mut writes = Vec::new();
        for sequence in self.local_stage_timing_sequences(plan) {
            let value = if first {
                sequence.values.first()
            } else {
                sequence.values.last()
            };
            if let Some(value) = value {
                writes.push(StateWrite {
                    device: sequence.device,
                    property: sequence.property.clone(),
                    value: value.clone(),
                });
            }
        }
        for sequence in self.local_output_timing_sequences(plan) {
            let value = if first {
                sequence.values.first()
            } else {
                sequence.values.last()
            };
            if let Some(value) = value {
                writes.push(StateWrite {
                    device: sequence.device,
                    property: sequence.property.clone(),
                    value: value.clone(),
                });
            }
        }
        if writes.is_empty() {
            return Ok(Value::Map(BTreeMap::new()));
        }
        self.apply_state_set(StateSet {
            name: Some(if first {
                "prior timing start stage sequence".into()
            } else {
                "prior timing stop stage sequence".into()
            }),
            writes,
            commit: CommitMode::Immediate,
        })
    }

    fn timing_transaction(
        &self,
        description: &str,
        command: protocol::PriorCommand,
    ) -> PhysicalTransaction {
        let line = protocol::encode(&command);
        PhysicalTransaction {
            resource: Some(self.resource),
            description: description.into(),
            payload: Value::Bytes(self.codec.encode(&line)),
        }
    }

    fn move_xy(&mut self, x_um: f64, y_um: f64) -> Result<()> {
        self.x_um = x_um;
        self.y_um = y_um;
        self.send(protocol::PriorCommand::MoveXyAbs {
            x_steps: protocol::steps(x_um, self.probe.step_size_xy_um),
            y_steps: protocol::steps(y_um, self.probe.step_size_xy_um),
        })?;
        self.read_optional_ack()?;
        self.finish_motion("prior xy status 3 then 0");
        Ok(())
    }

    fn move_xy_relative(&mut self, dx_um: f64, dy_um: f64) -> Result<()> {
        let next_x = (self.x_um + dx_um).clamp(0.0, self.probe.x_travel_um);
        let next_y = (self.y_um + dy_um).clamp(0.0, self.probe.y_travel_um);
        self.send(protocol::PriorCommand::MoveXyRel {
            dx_steps: protocol::steps(next_x - self.x_um, self.probe.step_size_xy_um),
            dy_steps: protocol::steps(next_y - self.y_um, self.probe.step_size_xy_um),
        })?;
        self.read_optional_ack()?;
        self.x_um = next_x;
        self.y_um = next_y;
        self.finish_motion("prior xy relative status 3 then 0");
        Ok(())
    }

    fn move_z(&mut self, z_um: f64) -> Result<()> {
        let next_steps = protocol::steps(z_um, self.probe.step_size_z_um);
        let current_steps = protocol::steps(self.z_um, self.probe.step_size_z_um);
        self.send(protocol::PriorCommand::MoveZRel {
            dz_steps: next_steps - current_steps,
        })?;
        self.read_optional_ack()?;
        self.z_um = z_um;
        self.finish_motion("prior z status 4 then 0");
        Ok(())
    }

    fn move_z_relative(&mut self, dz_um: f64) -> Result<()> {
        let next_z = (self.z_um + dz_um).clamp(0.0, self.probe.z_travel_um);
        self.move_z(next_z)
    }

    fn move_nano_z(&mut self, z_um: f64) -> Result<()> {
        self.send(protocol::PriorCommand::MoveNanoZAbs { position_um: z_um })?;
        self.read_optional_ack()?;
        self.nano_z_um = z_um;
        self.finish_motion("prior nanoscan z R");
        Ok(())
    }

    fn move_nano_z_relative(&mut self, dz_um: f64) -> Result<()> {
        let next_z = (self.nano_z_um + dz_um).clamp(0.0, self.probe.z_travel_um);
        self.move_nano_z(next_z)
    }

    fn validate_stage_move(&self, device: DeviceId, request: &StageMoveRequest) -> Result<()> {
        if request.target.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Prior StageMove target must contain at least one axis",
            ));
        }
        if request.profile.is_some() {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Prior StageMove MotionProfile uses typed velocity/acceleration; Prior SMS/SAS native percentage speed settings need calibration evidence before conversion",
            ));
        }
        for axis in request.target.keys() {
            match (device, axis) {
                (device, StageAxis::X | StageAxis::Y) if device == self.xy => {}
                (device, StageAxis::Z) if device == self.z || device == self.nano_z => {}
                (device, StageAxis::Custom(name))
                    if device == self.xy && (name == "x" || name == "y") => {}
                (device, StageAxis::Custom(name))
                    if (device == self.z || device == self.nano_z) && name == "z" => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "Prior StageMove axis does not belong to the target device",
                    ))
                }
            }
        }
        Ok(())
    }

    fn stage_move(&mut self, device: DeviceId, request: StageMoveRequest) -> Result<Value> {
        self.validate_stage_move(device, &request)?;
        if device == self.xy {
            let mut x = self.x_um;
            let mut y = self.y_um;
            for (axis, target) in &request.target {
                match axis {
                    StageAxis::X => x = target.micrometers(),
                    StageAxis::Y => y = target.micrometers(),
                    StageAxis::Custom(name) if name == "x" => x = target.micrometers(),
                    StageAxis::Custom(name) if name == "y" => y = target.micrometers(),
                    _ => {}
                }
            }
            if request.relative {
                self.move_xy_relative(x, y)?;
            } else {
                self.move_xy(
                    x.clamp(0.0, self.probe.x_travel_um),
                    y.clamp(0.0, self.probe.y_travel_um),
                )?;
            }
            self.emit_property(self.xy, "x", position(self.x_um));
            self.emit_property(self.xy, "y", position(self.y_um));
            Ok(Value::Map(BTreeMap::from([
                (
                    "mode".into(),
                    Value::String(if request.relative {
                        "relative".into()
                    } else {
                        "absolute".into()
                    }),
                ),
                ("x".into(), position(self.x_um)),
                ("y".into(), position(self.y_um)),
            ])))
        } else if device == self.z {
            let z = request
                .target
                .values()
                .next()
                .expect("validated one Z target")
                .micrometers();
            if request.relative {
                self.move_z_relative(z)?;
            } else {
                self.move_z(z.clamp(0.0, self.probe.z_travel_um))?;
            }
            self.emit_property(self.z, "z", position(self.z_um));
            Ok(Value::Map(BTreeMap::from([
                (
                    "mode".into(),
                    Value::String(if request.relative {
                        "relative".into()
                    } else {
                        "absolute".into()
                    }),
                ),
                ("z".into(), position(self.z_um)),
            ])))
        } else if device == self.nano_z {
            let z = request
                .target
                .values()
                .next()
                .expect("validated one NanoZ target")
                .micrometers();
            if request.relative {
                self.move_nano_z_relative(z)?;
            } else {
                self.move_nano_z(z.clamp(0.0, self.probe.z_travel_um))?;
            }
            self.emit_property(self.nano_z, "z", position(self.nano_z_um));
            self.emit_property(
                self.nano_z,
                "position_steps",
                step_count((self.nano_z_um / 0.001).round() as i64),
            );
            Ok(Value::Map(BTreeMap::from([
                (
                    "mode".into(),
                    Value::String(if request.relative {
                        "relative".into()
                    } else {
                        "absolute".into()
                    }),
                ),
                ("z".into(), position(self.nano_z_um)),
                (
                    "position_steps".into(),
                    step_count((self.nano_z_um / 0.001).round() as i64),
                ),
            ])))
        } else {
            Err(Error::new(
                ErrorCode::InvalidCommand,
                "Prior StageMove target device must be XY, Z, or NanoZ stage",
            ))
        }
    }

    fn apply_state_set(&mut self, set: StateSet) -> Result<Value> {
        let mut next_x = self.x_um;
        let mut next_y = self.y_um;
        let mut next_z = self.z_um;
        let mut next_nano_z = self.nano_z_um;
        let mut xy_changed = false;
        let mut z_changed = false;
        let mut nano_z_changed = false;
        let mut remaining = Vec::new();

        for write in set.writes {
            self.validate_write(write.device, &write.property, &write.value)?;
            match (write.device, write.property.as_str(), &write.value) {
                (device, "x", value) if device == self.xy => {
                    next_x = position_um(value)?.clamp(0.0, self.probe.x_travel_um);
                    xy_changed = true;
                }
                (device, "y", value) if device == self.xy => {
                    next_y = position_um(value)?.clamp(0.0, self.probe.y_travel_um);
                    xy_changed = true;
                }
                (device, "z", value) if device == self.z => {
                    next_z = position_um(value)?.clamp(0.0, self.probe.z_travel_um);
                    z_changed = true;
                }
                (device, "z", value) if device == self.nano_z => {
                    next_nano_z = position_um(value)?.clamp(0.0, self.probe.z_travel_um);
                    nano_z_changed = true;
                }
                _ => remaining.push(write),
            }
        }

        let mut changed = BTreeMap::new();
        if xy_changed {
            self.move_xy(next_x, next_y)?;
            changed.insert(format!("{}:x", (self.xy.0).0), position(self.x_um));
            changed.insert(format!("{}:y", (self.xy.0).0), position(self.y_um));
            self.emit_property(self.xy, "x", position(self.x_um));
            self.emit_property(self.xy, "y", position(self.y_um));
        }
        if z_changed {
            self.move_z(next_z)?;
            changed.insert(format!("{}:z", (self.z.0).0), position(self.z_um));
            self.emit_property(self.z, "z", position(self.z_um));
        }
        if nano_z_changed {
            self.move_nano_z(next_nano_z)?;
            changed.insert(format!("{}:z", (self.nano_z.0).0), position(self.nano_z_um));
            self.emit_property(self.nano_z, "z", position(self.nano_z_um));
            self.emit_property(
                self.nano_z,
                "position_steps",
                step_count((self.nano_z_um / 0.001).round() as i64),
            );
        }
        for write in remaining {
            let value = self.write_property(write.device, &write.property, &write.value)?;
            self.emit_property(write.device, &write.property, value.clone());
            changed.insert(format!("{}:{}", (write.device.0).0, write.property), value);
        }
        Ok(Value::Map(changed))
    }

    fn invoke(
        &mut self,
        device: DeviceId,
        capability: CapabilityId,
        request: CapabilityRequest,
    ) -> Result<Value> {
        let Some(capability) = self
            .capabilities(device)
            .into_iter()
            .find(|candidate| candidate.id == capability)
        else {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "unknown Prior capability",
            ));
        };
        match (capability.kind, request) {
            (CapabilityKind::StageMove, CapabilityRequest::StageMove(request))
                if device == self.xy || device == self.z || device == self.nano_z =>
            {
                self.stage_move(device, request)
            }
            (CapabilityKind::StageMove, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Prior StageMove expects a StageMoveRequest",
            )),
            (CapabilityKind::StageHome, CapabilityRequest::None) if device == self.xy => {
                self.send(protocol::PriorCommand::HomeXy)?;
                if self.read_optional_ack()? {
                    self.refresh_stage_readback(self.xy)?;
                } else {
                    self.x_um = 0.0;
                    self.y_um = 0.0;
                    self.finish_motion("prior xy home R");
                    self.emit_property(self.xy, "x", position(self.x_um));
                    self.emit_property(self.xy, "y", position(self.y_um));
                }
                Ok(Value::String("xy homed".into()))
            }
            (CapabilityKind::StageStop, CapabilityRequest::None)
                if device == self.xy || device == self.z || device == self.nano_z =>
            {
                self.send(protocol::PriorCommand::Halt)?;
                if self.read_optional_ack()? {
                    self.refresh_stage_readback(device)?;
                } else {
                    self.busy = false;
                    self.emit_property(device, "busy", Value::Bool(false));
                }
                Ok(Value::String("halted".into()))
            }
            (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Prior home/stop capabilities take no request",
            )),
            (CapabilityKind::TriggerSource, request) if device == self.ttl => {
                self.apply_trigger_source(request)
            }
            (CapabilityKind::TriggerSink, request)
                if device == self.shutter || device == self.lumen =>
            {
                self.apply_trigger_sink(device, request)
            }
            (CapabilityKind::FilterSelect, CapabilityRequest::FilterSelect(request))
                if device == self.wheel =>
            {
                let value = self.write_property(
                    self.wheel,
                    "position",
                    &Value::I64(request.position as i64),
                )?;
                self.emit_property(self.wheel, "position", value.clone());
                Ok(value)
            }
            (CapabilityKind::FilterSelect, _) if device == self.wheel => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Prior FilterSelect expects FilterSelectRequest",
            )),
            (CapabilityKind::GenericCommand, CapabilityRequest::GenericCommand(request))
                if device == self.hub =>
            {
                self.apply_generic_command(request)
            }
            (CapabilityKind::GenericCommand, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Prior GenericCommand expects GenericCommandRequest",
            )),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Prior capability",
            )),
        }
    }

    fn apply_trigger_source(&mut self, request: CapabilityRequest) -> Result<Value> {
        let actions = trigger_actions(&request)?;
        for high in &actions {
            let value = self.write_property(self.ttl, "high", &Value::Bool(*high))?;
            self.emit_property(self.ttl, "high", value);
        }
        Ok(Value::Map(BTreeMap::from([
            ("device".into(), Value::I64((self.ttl.0).0 as i64)),
            ("high".into(), Value::Bool(self.ttl_high)),
            ("triggered".into(), Value::Bool(actions.len() > 1)),
            ("commands".into(), Value::I64(actions.len() as i64)),
        ])))
    }

    fn apply_trigger_sink(
        &mut self,
        device: DeviceId,
        request: CapabilityRequest,
    ) -> Result<Value> {
        let actions = trigger_actions(&request)?;
        for open in &actions {
            let value = self.write_property(device, "open", &Value::Bool(*open))?;
            self.emit_property(device, "open", value);
        }
        let open = if device == self.shutter {
            self.shutter_open
        } else {
            self.lumen_open
        };
        Ok(Value::Map(BTreeMap::from([
            ("device".into(), Value::I64((device.0).0 as i64)),
            ("open".into(), Value::Bool(open)),
            ("triggered".into(), Value::Bool(actions.len() > 1)),
            ("commands".into(), Value::I64(actions.len() as i64)),
        ])))
    }

    fn state_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("model".into(), Value::String(self.probe.model.clone())),
            (
                "firmware_date".into(),
                Value::String(self.probe.firmware_date.clone()),
            ),
            ("busy".into(), Value::Bool(self.busy)),
            ("last_ack".into(), Value::String(self.last_ack.clone())),
            ("fault".into(), Value::Bool(self.ack_is_fault())),
            ("x".into(), position(self.x_um)),
            ("y".into(), position(self.y_um)),
            ("z".into(), position(self.z_um)),
            ("nano_z".into(), position(self.nano_z_um)),
            (
                "nano_z_position_steps".into(),
                step_count((self.nano_z_um / 0.001).round() as i64),
            ),
            ("xy_speed".into(), percent_ratio(self.xy_speed)),
            (
                "xy_acceleration".into(),
                percent_ratio(self.xy_acceleration),
            ),
            (
                "wheel_position".into(),
                Value::I64(self.wheel_position as i64),
            ),
            ("shutter_open".into(), Value::Bool(self.shutter_open)),
            ("ttl_high".into(), Value::Bool(self.ttl_high)),
            ("lumen_open".into(), Value::Bool(self.lumen_open)),
            (
                "lumen_intensity".into(),
                percent_ratio(self.lumen_intensity),
            ),
            ("lumen_delay".into(), time_interval_ms(self.lumen_delay_ms)),
        ]))
    }

    fn finish_motion(&mut self, message: &str) {
        self.busy = true;
        self.pending
            .push_back(DriverEvent::Event(Event::Log(LogEvent {
                driver: Some(self.id),
                message: message.into(),
            })));
        self.busy = false;
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

impl Driver for PriorDriver {
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
            label: "prior-proscan-serial".into(),
            kind: "serial".into(),
            metadata: BTreeMap::from([
                ("send_terminator".into(), Value::String("CR".into())),
                ("recv_terminator".into(), Value::String("CR".into())),
                (
                    "startup_readback_supported".into(),
                    Value::List(
                        protocol::probe_script()
                            .into_iter()
                            .map(Value::String)
                            .collect(),
                    ),
                ),
                (
                    "completion".into(),
                    Value::String("$ status returns idle when XY/Z bits clear".into()),
                ),
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
            ]),
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.hub {
            vec![capability(1, device, CapabilityKind::GenericCommand)]
        } else if device == self.xy {
            vec![
                capability(1, device, CapabilityKind::StageMove),
                capability(2, device, CapabilityKind::StageHome),
                capability(3, device, CapabilityKind::StageStop),
            ]
        } else if device == self.z {
            vec![
                capability(1, device, CapabilityKind::StageMove),
                capability(3, device, CapabilityKind::StageStop),
            ]
        } else if device == self.nano_z {
            vec![
                capability(1, device, CapabilityKind::StageMove),
                capability(3, device, CapabilityKind::StageStop),
            ]
        } else if device == self.ttl {
            vec![capability(4, device, CapabilityKind::TriggerSource)]
        } else if device == self.shutter || device == self.lumen {
            vec![capability(5, device, CapabilityKind::TriggerSink)]
        } else if device == self.wheel {
            vec![capability(6, device, CapabilityKind::FilterSelect)]
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
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("prior read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("prior write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "prior remultiplexed controller state set".into(),
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
                    let candidate = self
                        .capabilities(*device)
                        .into_iter()
                        .find(|candidate| candidate.id == *capability)
                        .ok_or_else(|| {
                            Error::new(ErrorCode::Unsupported, "unknown Prior capability")
                        })?;
                    match (&candidate.kind, request) {
                        (CapabilityKind::StageMove, CapabilityRequest::StageMove(request)) => {
                            self.validate_stage_move(*device, request)?;
                        }
                        (
                            CapabilityKind::StageHome | CapabilityKind::StageStop,
                            CapabilityRequest::None,
                        ) => {}
                        (CapabilityKind::TriggerSource, request) if *device == self.ttl => {
                            let _ = trigger_actions(request)?;
                        }
                        (CapabilityKind::TriggerSink, request)
                            if *device == self.shutter || *device == self.lumen =>
                        {
                            let _ = trigger_actions(request)?;
                        }
                        (
                            CapabilityKind::FilterSelect,
                            CapabilityRequest::FilterSelect(request),
                        ) if *device == self.wheel => {
                            self.validate_write(
                                *device,
                                "position",
                                &Value::I64(request.position as i64),
                            )?;
                        }
                        (
                            CapabilityKind::GenericCommand,
                            CapabilityRequest::GenericCommand(request),
                        ) if *device == self.hub => {
                            self.validate_generic_command(request)?;
                        }
                        (CapabilityKind::StageMove, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Prior StageMove expects a StageMoveRequest",
                            ));
                        }
                        (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Prior home/stop capabilities take no request",
                            ));
                        }
                        (CapabilityKind::TriggerSource | CapabilityKind::TriggerSink, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Prior trigger capabilities expect None or CapabilityRequest::Trigger",
                            ));
                        }
                        (CapabilityKind::FilterSelect, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Prior FilterSelect expects FilterSelectRequest",
                            ));
                        }
                        (CapabilityKind::GenericCommand, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Prior GenericCommand expects GenericCommandRequest",
                            ));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported Prior capability",
                            ));
                        }
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("prior invoke {}", capability.0),
                        payload: match request {
                            CapabilityRequest::StageMove(request) => Value::Map(BTreeMap::from([
                                ("relative".into(), Value::Bool(request.relative)),
                                (
                                    "axes".into(),
                                    Value::List(
                                        request
                                            .target
                                            .keys()
                                            .map(|axis| Value::String(axis.name().into()))
                                            .collect(),
                                    ),
                                ),
                            ])),
                            CapabilityRequest::Trigger(request) => Value::Map(BTreeMap::from([(
                                "action".into(),
                                Value::String(match request.action {
                                    TriggerAction::Enable => "enable".into(),
                                    TriggerAction::Disable => "disable".into(),
                                    TriggerAction::Pulse => "pulse".into(),
                                }),
                            )])),
                            CapabilityRequest::FilterSelect(request) => {
                                Value::I64(request.position as i64)
                            }
                            CapabilityRequest::GenericCommand(request) => Value::List(
                                Self::refresh_targets_for(&request.command)?
                                    .into_iter()
                                    .map(|(_, key)| Value::String(key.into()))
                                    .collect(),
                            ),
                            _ => Value::Null,
                        },
                    });
                }
                Command::Arm(plan) => {
                    self.validate_timing_plan(plan)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "prior timing arm summary".into(),
                        payload: self.timing_summary(plan, "arm"),
                    });
                }
                Command::Start(_) | Command::Stop(_) => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "Prior direct timing transitions are runtime-owned",
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
                    if let Some(query) = self.query_for_property(device, &key) {
                        self.send(query.clone())?;
                        self.read_query_reply(device, &query)?;
                    }
                    last = self.read_property(device, &key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    last = self.write_property(device, &key, &value)?;
                    self.emit_property(device, &key, last.clone());
                }
                Command::ApplyStateSet(set) => {
                    last = self.apply_state_set(set)?;
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    last = self.invoke(device, capability, request)?;
                }
                Command::Arm(_) | Command::Start(_) | Command::Stop(_) => unreachable!(),
            }
        }
        self.pending
            .push_back(DriverEvent::TokenCompleted { token, value: last });
        Ok(token)
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        if let Ok(bytes) = self.serial.read_available() {
            for line in self.codec.push(&bytes) {
                self.pending
                    .push_back(DriverEvent::Event(Event::Log(LogEvent {
                        driver: Some(self.id),
                        message: format!("prior serial: {line}"),
                    })));
            }
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
                description: "prior timing arm summary".into(),
                payload: self.timing_summary(plan, "arm"),
            }],
        })
    }

    fn start_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let mut physical_transactions = Vec::new();
        let sequence_value = self.apply_timing_sequence_step(&armed.plan, true)?;
        if !matches!(&sequence_value, Value::Map(map) if map.is_empty()) {
            physical_transactions.push(PhysicalTransaction {
                resource: Some(self.resource),
                description: "prior timing start stage sequence".into(),
                payload: sequence_value,
            });
        }
        if self.has_timed_ttl(&armed.plan) && !self.has_sequence_for(&armed.plan, self.ttl, "high")
        {
            let value = self.write_property(self.ttl, "high", &Value::Bool(true))?;
            self.emit_property(self.ttl, "high", value);
            physical_transactions.push(self.timing_transaction(
                "prior timing start ttl high",
                protocol::PriorCommand::SetTtl {
                    line: 0,
                    high: true,
                },
            ));
        }
        if self.has_timed_shutter(&armed.plan)
            && !self.has_sequence_for(&armed.plan, self.shutter, "open")
        {
            let value = self.write_property(self.shutter, "open", &Value::Bool(true))?;
            self.emit_property(self.shutter, "open", value);
            physical_transactions.push(self.timing_transaction(
                "prior timing start shutter open",
                protocol::PriorCommand::SetShutter {
                    shutter: 1,
                    open: true,
                },
            ));
        }
        if self.has_timed_lumen(&armed.plan)
            && !self.has_sequence_for(&armed.plan, self.lumen, "open")
        {
            let value = self.write_property(self.lumen, "open", &Value::Bool(true))?;
            self.emit_property(self.lumen, "open", value);
            physical_transactions.push(self.timing_transaction(
                "prior timing start lumen open",
                protocol::PriorCommand::SetLumenOpen {
                    intensity: self.lumen_intensity,
                    open: true,
                },
            ));
        }
        physical_transactions.push(PhysicalTransaction {
            resource: Some(self.resource),
            description: "prior timing start summary".into(),
            payload: self.timing_summary(&armed.plan, "start"),
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
        let mut physical_transactions = Vec::new();
        let sequence_value = self.apply_timing_sequence_step(&armed.plan, false)?;
        if !matches!(&sequence_value, Value::Map(map) if map.is_empty()) {
            physical_transactions.push(PhysicalTransaction {
                resource: Some(self.resource),
                description: "prior timing stop stage sequence".into(),
                payload: sequence_value,
            });
        }
        if self.has_timed_lumen(&armed.plan)
            && !self.has_sequence_for(&armed.plan, self.lumen, "open")
        {
            let value = self.write_property(self.lumen, "open", &Value::Bool(false))?;
            self.emit_property(self.lumen, "open", value);
            physical_transactions.push(self.timing_transaction(
                "prior timing stop lumen close",
                protocol::PriorCommand::SetLumenOpen {
                    intensity: self.lumen_intensity,
                    open: false,
                },
            ));
        }
        if self.has_timed_shutter(&armed.plan)
            && !self.has_sequence_for(&armed.plan, self.shutter, "open")
        {
            let value = self.write_property(self.shutter, "open", &Value::Bool(false))?;
            self.emit_property(self.shutter, "open", value);
            physical_transactions.push(self.timing_transaction(
                "prior timing stop shutter close",
                protocol::PriorCommand::SetShutter {
                    shutter: 1,
                    open: false,
                },
            ));
        }
        if self.has_timed_ttl(&armed.plan) && !self.has_sequence_for(&armed.plan, self.ttl, "high")
        {
            let value = self.write_property(self.ttl, "high", &Value::Bool(false))?;
            self.emit_property(self.ttl, "high", value);
            physical_transactions.push(self.timing_transaction(
                "prior timing stop ttl low",
                protocol::PriorCommand::SetTtl {
                    line: 0,
                    high: false,
                },
            ));
        }
        physical_transactions.push(PhysicalTransaction {
            resource: Some(self.resource),
            description: "prior timing stop summary".into(),
            payload: self.timing_summary(&armed.plan, "stop"),
        });
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions,
        })
    }
}

fn trigger_actions(request: &CapabilityRequest) -> Result<Vec<bool>> {
    let action = match request {
        CapabilityRequest::None => TriggerAction::Pulse,
        CapabilityRequest::Trigger(request) => request.action,
        _ => {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Prior trigger capabilities expect None or CapabilityRequest::Trigger",
            ))
        }
    };
    Ok(match action {
        TriggerAction::Enable => vec![true],
        TriggerAction::Disable => vec![false],
        TriggerAction::Pulse => vec![true, false],
    })
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

fn ratio_property_range(
    key: &str,
    display_name: &str,
    unit: Option<&str>,
    writable: bool,
    min: f64,
    max: f64,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Ratio,
        unit,
        writable,
        Some(Range {
            min: Value::Ratio(Ratio::from_percent(min)),
            max: Value::Ratio(Ratio::from_percent(max)),
        }),
    )
}

fn percent_ratio(percent: u8) -> Value {
    Value::Ratio(Ratio::from_percent(percent as f64))
}

fn time_interval_range(
    key: &str,
    display_name: &str,
    unit: Option<&str>,
    writable: bool,
    min_ms: f64,
    max_ms: f64,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::TimeInterval,
        unit,
        writable,
        Some(Range {
            min: time_interval_ms(min_ms),
            max: time_interval_ms(max_ms),
        }),
    )
}

fn position_property(key: &str, display_name: &str, writable: bool, max_um: f64) -> PropertySchema {
    property(
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

fn sequenceable_position_property(
    key: &str,
    display_name: &str,
    writable: bool,
    max_um: f64,
) -> PropertySchema {
    let mut schema = position_property(key, display_name, writable, max_um);
    schema.sequenceable = writable;
    schema
}

fn position(value_um: f64) -> Value {
    Value::Position(Position::from_micrometers(value_um))
}

fn step_count(steps: i64) -> Value {
    Value::StepCount(StepCount::new(steps))
}

fn time_interval_ms(ms: f64) -> Value {
    Value::TimeInterval(TimeInterval::from_milliseconds(ms))
}

fn time_ms(value: &Value) -> Result<f64> {
    match value {
        Value::TimeInterval(interval) => Ok(interval.microseconds() * 1e-3),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            "expected typed time interval value",
        )),
    }
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

fn position_config_um(device: &DeviceConfig, key: &str, legacy_key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::Position(value)) => Some(value.micrometers()),
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => f64_prop(device, legacy_key),
    }
}

fn time_config_ms(device: &DeviceConfig, key: &str, legacy_key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::TimeInterval(value)) => Some(value.microseconds() * 1e-3),
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => f64_prop(device, legacy_key),
    }
}

fn property_i64_range(
    key: &str,
    display_name: &str,
    unit: Option<&str>,
    writable: bool,
    min: i64,
    max: i64,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::I64,
        unit,
        writable,
        Some(Range {
            min: Value::I64(min),
            max: Value::I64(max),
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

fn f64_prop(device: &DeviceConfig, key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn u8_prop(device: &DeviceConfig, key: &str) -> Option<u8> {
    u64_prop(device, key).and_then(|value| value.try_into().ok())
}

fn u32_prop(device: &DeviceConfig, key: &str) -> Option<u32> {
    u64_prop(device, key).and_then(|value| value.try_into().ok())
}

fn u64_prop(device: &DeviceConfig, key: &str) -> Option<u64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => (*value >= 0).then_some(*value as u64),
        Some(Value::F64(value)) if value.is_finite() && *value >= 0.0 => Some(*value as u64),
        _ => None,
    }
}
