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

    #[derive(Debug, Clone, PartialEq)]
    pub struct MarzhauserProbe {
        pub version: String,
        pub controller: String,
        pub configuration: u16,
        pub x_travel_um: f64,
        pub y_travel_um: f64,
        pub z_travel_um: f64,
        pub pitch_x_mm: f64,
        pub pitch_y_mm: f64,
        pub pitch_z_mm: f64,
        pub steps_per_mm: f64,
    }

    impl MarzhauserProbe {
        pub fn simulated_lstep() -> Self {
            Self {
                version: "Vers:LS numanager-sim".into(),
                controller: "L-Step/TANGO-compatible".into(),
                configuration: 0x30,
                x_travel_um: 100_000.0,
                y_travel_um: 75_000.0,
                z_travel_um: 12_000.0,
                pitch_x_mm: 1.0,
                pitch_y_mm: 1.0,
                pitch_z_mm: 1.0,
                steps_per_mm: 50_000.0,
            }
        }

        pub fn step_size_x_um(&self) -> f64 {
            self.pitch_x_mm * 1000.0 / self.steps_per_mm
        }

        pub fn step_size_y_um(&self) -> f64 {
            self.pitch_y_mm * 1000.0 / self.steps_per_mm
        }

        pub fn step_size_z_um(&self) -> f64 {
            self.pitch_z_mm * 1000.0 / self.steps_per_mm
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct MarzhauserProbeResult {
        pub probe: MarzhauserProbe,
        pub x_um: f64,
        pub y_um: f64,
        pub z_um: f64,
        pub speed_x_mm_s: Option<f64>,
        pub speed_y_mm_s: Option<f64>,
        pub speed_z_mm_s: Option<f64>,
        pub accel_x_m_s2: Option<f64>,
        pub accel_y_m_s2: Option<f64>,
        pub accel_z_m_s2: Option<f64>,
        pub busy: bool,
        pub last_error: String,
        pub limit_x: Option<String>,
        pub limit_y: Option<String>,
        pub limit_z: Option<String>,
        pub replies: Vec<(String, String)>,
    }

    impl MarzhauserProbeResult {
        pub fn from_replies(replies: &[(impl AsRef<str>, impl AsRef<str>)]) -> Result<Self> {
            let mut probe = MarzhauserProbe::simulated_lstep();
            let mut x_um = 0.0;
            let mut y_um = 0.0;
            let mut z_um = 0.0;
            let mut speed_x_mm_s = None;
            let mut speed_y_mm_s = None;
            let mut speed_z_mm_s = None;
            let mut accel_x_m_s2 = None;
            let mut accel_y_m_s2 = None;
            let mut accel_z_m_s2 = None;
            let mut busy = false;
            let mut last_error = String::new();
            let mut limit_x = None;
            let mut limit_y = None;
            let mut limit_z = None;
            let mut stored = Vec::new();

            for (command, reply) in replies {
                let command = command.as_ref();
                let reply = reply.as_ref().trim();
                stored.push((command.to_string(), reply.to_string()));
                match command {
                    "?ver" => probe.version = reply.to_string(),
                    "?version" => probe.controller = reply.to_string(),
                    "!autostatus 0" => parse_error(reply)?,
                    "?det" => probe.configuration = parse_u16_reply("?det", reply)?,
                    "?pitch x" => probe.pitch_x_mm = parse_f64_reply("?pitch x", reply)?,
                    "?pitch y" => probe.pitch_y_mm = parse_f64_reply("?pitch y", reply)?,
                    "?pitch z" => probe.pitch_z_mm = parse_f64_reply("?pitch z", reply)?,
                    "?vel x" => {
                        let rev_s = parse_f64_reply("?vel x", reply)?;
                        speed_x_mm_s = Some(rev_s * probe.pitch_x_mm);
                    }
                    "?vel y" => {
                        let rev_s = parse_f64_reply("?vel y", reply)?;
                        speed_y_mm_s = Some(rev_s * probe.pitch_y_mm);
                    }
                    "?vel z" => {
                        let rev_s = parse_f64_reply("?vel z", reply)?;
                        speed_z_mm_s = Some(rev_s * probe.pitch_z_mm);
                    }
                    "?accel x" => accel_x_m_s2 = Some(parse_f64_reply("?accel x", reply)?),
                    "?accel y" => accel_y_m_s2 = Some(parse_f64_reply("?accel y", reply)?),
                    "?accel z" => accel_z_m_s2 = Some(parse_f64_reply("?accel z", reply)?),
                    "?pos" => {
                        let values = parse_f64_list("?pos", reply)?;
                        if values.len() < 2 {
                            return Err(Error::new(
                                ErrorCode::Transport,
                                format!("Marzhauser ?pos reply needs x/y values: {reply}"),
                            ));
                        }
                        x_um = values[0] * 1000.0;
                        y_um = values[1] * 1000.0;
                    }
                    "?pos z" => z_um = parse_f64_reply("?pos z", reply)? * 1000.0,
                    "?err" => {
                        last_error = reply.to_string();
                        parse_error(reply)?;
                    }
                    "?statusaxis" => busy = is_busy_status(reply),
                    "?lim x" => limit_x = Some(reply.to_string()),
                    "?lim y" => limit_y = Some(reply.to_string()),
                    "?lim z" => limit_z = Some(reply.to_string()),
                    _ => {}
                }
            }

            Ok(Self {
                probe,
                x_um,
                y_um,
                z_um,
                speed_x_mm_s,
                speed_y_mm_s,
                speed_z_mm_s,
                accel_x_m_s2,
                accel_y_m_s2,
                accel_z_m_s2,
                busy,
                last_error,
                limit_x,
                limit_y,
                limit_z,
                replies: stored,
            })
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum MarzhauserCommand {
        Version,
        TangoVersion,
        DisableAutostatus,
        DetectConfiguration,
        QueryPitch { axis: char },
        SetDimension { axis: String, dim: u8 },
        QueryVelocity { axis: char },
        SetVelocity { axis: char, rev_per_s: f64 },
        QueryAcceleration { axis: char },
        SetAcceleration { axis: char, acceleration: f64 },
        MoveAbsXy { x: f64, y: f64 },
        MoveRelXy { dx: f64, dy: f64 },
        MoveAbsZ { z: f64 },
        MoveRelZ { dz: f64 },
        QueryPosition,
        QueryPositionAxis { axis: char },
        SetOriginXy,
        SetOriginZ,
        Calibrate { axis: char },
        MoveContinuousXy { vx: f64, vy: f64 },
        MoveContinuousZ { vz: f64 },
        Abort,
        QueryError,
        QueryStatusAxis,
        QueryLimit { axis: char },
    }

    pub fn encode(command: &MarzhauserCommand) -> String {
        match command {
            MarzhauserCommand::Version => "?ver".into(),
            MarzhauserCommand::TangoVersion => "?version".into(),
            MarzhauserCommand::DisableAutostatus => "!autostatus 0".into(),
            MarzhauserCommand::DetectConfiguration => "?det".into(),
            MarzhauserCommand::QueryPitch { axis } => format!("?pitch {axis}"),
            MarzhauserCommand::SetDimension { axis, dim } => format!("!dim {axis} {dim}"),
            MarzhauserCommand::QueryVelocity { axis } => format!("?vel {axis}"),
            MarzhauserCommand::SetVelocity { axis, rev_per_s } => {
                format!("!vel {axis} {rev_per_s:.6}")
            }
            MarzhauserCommand::QueryAcceleration { axis } => format!("?accel {axis}"),
            MarzhauserCommand::SetAcceleration { axis, acceleration } => {
                format!("!accel {axis} {acceleration:.6}")
            }
            MarzhauserCommand::MoveAbsXy { x, y } => format!("!moa {x:.6} {y:.6}"),
            MarzhauserCommand::MoveRelXy { dx, dy } => format!("!mor {dx:.6} {dy:.6}"),
            MarzhauserCommand::MoveAbsZ { z } => format!("!moa z {z:.6}"),
            MarzhauserCommand::MoveRelZ { dz } => format!("!mor z {dz:.6}"),
            MarzhauserCommand::QueryPosition => "?pos".into(),
            MarzhauserCommand::QueryPositionAxis { axis } => format!("?pos {axis}"),
            MarzhauserCommand::SetOriginXy => "!pos 0 0".into(),
            MarzhauserCommand::SetOriginZ => "!pos z 0".into(),
            MarzhauserCommand::Calibrate { axis } => format!("!cal {axis}"),
            MarzhauserCommand::MoveContinuousXy { vx, vy } => {
                format!("!speed {vx:.6} {vy:.6}")
            }
            MarzhauserCommand::MoveContinuousZ { vz } => format!("!speed z {vz:.6}"),
            MarzhauserCommand::Abort => "a".into(),
            MarzhauserCommand::QueryError => "?err".into(),
            MarzhauserCommand::QueryStatusAxis => "?statusaxis".into(),
            MarzhauserCommand::QueryLimit { axis } => format!("?lim {axis}"),
        }
    }

    pub fn parse_error(reply: &str) -> Result<()> {
        if error_is_fault(reply) {
            Err(Error::new(
                ErrorCode::Transport,
                format!("Marzhauser controller error: {}", reply.trim()),
            ))
        } else {
            Ok(())
        }
    }

    pub fn error_is_fault(reply: &str) -> bool {
        !matches!(reply.trim(), "0" | "0 0" | "")
    }

    pub fn is_busy_status(reply: &str) -> bool {
        reply
            .split_whitespace()
            .any(|token| token != "0" && token != "N" && token != "n")
    }

    pub fn probe_commands() -> Vec<MarzhauserCommand> {
        vec![
            MarzhauserCommand::Version,
            MarzhauserCommand::TangoVersion,
            MarzhauserCommand::DisableAutostatus,
            MarzhauserCommand::DetectConfiguration,
            MarzhauserCommand::QueryPitch { axis: 'x' },
            MarzhauserCommand::QueryPitch { axis: 'y' },
            MarzhauserCommand::QueryPitch { axis: 'z' },
            MarzhauserCommand::QueryVelocity { axis: 'x' },
            MarzhauserCommand::QueryVelocity { axis: 'y' },
            MarzhauserCommand::QueryVelocity { axis: 'z' },
            MarzhauserCommand::QueryAcceleration { axis: 'x' },
            MarzhauserCommand::QueryAcceleration { axis: 'y' },
            MarzhauserCommand::QueryAcceleration { axis: 'z' },
            MarzhauserCommand::QueryPosition,
            MarzhauserCommand::QueryPositionAxis { axis: 'z' },
            MarzhauserCommand::QueryError,
            MarzhauserCommand::QueryStatusAxis,
            MarzhauserCommand::QueryLimit { axis: 'x' },
            MarzhauserCommand::QueryLimit { axis: 'y' },
            MarzhauserCommand::QueryLimit { axis: 'z' },
        ]
    }

    pub fn probe_script() -> Vec<String> {
        probe_commands().iter().map(encode).collect()
    }

    pub fn execute_probe_script(
        serial: &mut dyn SerialIo,
        polls_per_command: usize,
    ) -> Result<MarzhauserProbeResult> {
        let mut codec = SerialLineCodec::new(SEND_ENDING, RECV_ENDING);
        let mut replies = Vec::new();
        for command in probe_commands() {
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
                    format!("timed out waiting for Marzhauser probe reply to {encoded}"),
                )
            })?;
            replies.push((encoded, reply));
        }
        MarzhauserProbeResult::from_replies(&replies)
    }

    pub(crate) fn parse_u16_reply(command: &str, reply: &str) -> Result<u16> {
        let value = parse_f64_reply(command, reply)?;
        if value.is_finite() && value >= 0.0 && value <= u16::MAX as f64 {
            Ok(value.round() as u16)
        } else {
            Err(Error::new(
                ErrorCode::Transport,
                format!("invalid Marzhauser {command} u16 {reply}"),
            ))
        }
    }

    pub(crate) fn parse_f64_reply(command: &str, reply: &str) -> Result<f64> {
        parse_f64_list(command, reply)?
            .first()
            .copied()
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::Transport,
                    format!("empty Marzhauser {command} reply"),
                )
            })
    }

    pub(crate) fn parse_f64_list(command: &str, reply: &str) -> Result<Vec<f64>> {
        let mut values = Vec::new();
        for token in reply
            .trim()
            .split(|ch: char| {
                !(ch == '-'
                    || ch == '+'
                    || ch == '.'
                    || ch == 'e'
                    || ch == 'E'
                    || ch.is_ascii_digit())
            })
            .filter(|token| !token.is_empty() && *token != "+" && *token != "-")
        {
            values.push(token.parse::<f64>().map_err(|error| {
                Error::new(
                    ErrorCode::Transport,
                    format!("invalid Marzhauser {command} number {token}: {error}"),
                )
            })?);
        }
        Ok(values)
    }
}

pub struct MarzhauserDiscovery {
    next_id: DriverId,
    probes: Vec<MarzhauserConfiguredProbe>,
}

impl MarzhauserDiscovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![MarzhauserConfiguredProbe::simulated()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| device.driver == "marzhauser")
            .map(MarzhauserConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for MarzhauserDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = probe.label.clone();
                let driver = if probe.connect_real_transport {
                    Box::new(MarzhauserDriver::serial(id, probe)?) as Box<dyn Driver>
                } else {
                    Box::new(MarzhauserDriver::configured_fixture(id, probe)) as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct MarzhauserConfiguredProbe {
    pub label: String,
    pub probe: protocol::MarzhauserProbe,
    pub limit_x: Option<String>,
    pub limit_y: Option<String>,
    pub limit_z: Option<String>,
    pub last_error: String,
    pub endpoint: Option<MarzhauserSerialEndpoint>,
    pub connect_real_transport: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarzhauserSerialEndpoint {
    pub port_name: String,
    pub baud_rate: u32,
    pub timeout_ms: u64,
}

impl MarzhauserConfiguredProbe {
    pub fn simulated() -> Self {
        Self {
            label: "Simulated Marzhauser L-Step/TANGO controller".into(),
            probe: protocol::MarzhauserProbe::simulated_lstep(),
            limit_x: Some("0 100".into()),
            limit_y: Some("0 75".into()),
            limit_z: Some("0 12".into()),
            last_error: "0".into(),
            endpoint: None,
            connect_real_transport: false,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = protocol::MarzhauserProbe::simulated_lstep();
        probe.version = string_prop(device, "version").unwrap_or_else(|| probe.version.clone());
        probe.controller =
            string_prop(device, "controller").unwrap_or_else(|| probe.controller.clone());
        probe.configuration = u16_prop(device, "configuration").unwrap_or(probe.configuration);
        probe.x_travel_um =
            position_config_um(device, "x_travel", "x_travel_um").unwrap_or(probe.x_travel_um);
        probe.y_travel_um =
            position_config_um(device, "y_travel", "y_travel_um").unwrap_or(probe.y_travel_um);
        probe.z_travel_um =
            position_config_um(device, "z_travel", "z_travel_um").unwrap_or(probe.z_travel_um);
        probe.pitch_x_mm =
            position_config_mm(device, "pitch_x", "pitch_x_mm").unwrap_or(probe.pitch_x_mm);
        probe.pitch_y_mm =
            position_config_mm(device, "pitch_y", "pitch_y_mm").unwrap_or(probe.pitch_y_mm);
        probe.pitch_z_mm =
            position_config_mm(device, "pitch_z", "pitch_z_mm").unwrap_or(probe.pitch_z_mm);
        probe.steps_per_mm = f64_prop(device, "steps_per_mm").unwrap_or(probe.steps_per_mm);
        let limit_x = string_prop(device, "limit_x");
        let limit_y = string_prop(device, "limit_y");
        let limit_z = string_prop(device, "limit_z");
        let last_error = string_prop(device, "last_error").unwrap_or_else(|| "0".into());

        let endpoint =
            string_prop(device, "serial_port").map(|port_name| MarzhauserSerialEndpoint {
                port_name,
                baud_rate: u32_prop(device, "baud_rate").unwrap_or(57_600),
                timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(1),
            });

        Ok(Self {
            label: if device.label.is_empty() {
                "Configured Marzhauser L-Step/TANGO controller".into()
            } else {
                device.label.clone()
            },
            probe,
            limit_x,
            limit_y,
            limit_z,
            last_error,
            endpoint,
            connect_real_transport: bool_prop(device, "connect").unwrap_or(false),
        })
    }
}

pub struct MarzhauserDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    xy: DeviceId,
    z: DeviceId,
    probe: protocol::MarzhauserProbe,
    x_um: f64,
    y_um: f64,
    z_um: f64,
    speed_x_mm_s: f64,
    speed_y_mm_s: f64,
    speed_z_mm_s: f64,
    accel_x_m_s2: f64,
    accel_y_m_s2: f64,
    accel_z_m_s2: f64,
    limit_x: Option<String>,
    limit_y: Option<String>,
    limit_z: Option<String>,
    last_error: String,
    busy: bool,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
}

impl MarzhauserDriver {
    pub fn simulated(id: DriverId) -> Self {
        Self::configured_fixture(id, MarzhauserConfiguredProbe::simulated())
    }

    pub fn configured_fixture(id: DriverId, configured: MarzhauserConfiguredProbe) -> Self {
        let serial = ScriptedSerial::new();
        Self::new_configured(id, configured, Box::new(serial), false)
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: MarzhauserConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Marzhauser serial probe is missing serial_port metadata",
            )
        })?;
        let mut serial = numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(endpoint.port_name, endpoint.baud_rate)
                .timeout(Duration::from_millis(endpoint.timeout_ms)),
        )?;
        let probe_result = protocol::execute_probe_script(&mut serial, 4)?;
        let mut probed = configured;
        probed.probe = probe_result.probe.clone();
        probed.limit_x = probe_result.limit_x.clone().or(probed.limit_x);
        probed.limit_y = probe_result.limit_y.clone().or(probed.limit_y);
        probed.limit_z = probe_result.limit_z.clone().or(probed.limit_z);
        probed.last_error = probe_result.last_error.clone();
        Ok(
            Self::new_configured(id, probed, Box::new(serial), true)
                .with_probe_result(probe_result),
        )
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _configured: MarzhauserConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Marzhauser real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(
        id: DriverId,
        probe: protocol::MarzhauserProbe,
        limit_x: Option<String>,
        limit_y: Option<String>,
        limit_z: Option<String>,
        last_error: String,
        serial: Box<dyn SerialIo>,
    ) -> Self {
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 1401)),
            hub: DeviceId(NodeId(id.0 * 1000 + 1410)),
            xy: DeviceId(NodeId(id.0 * 1000 + 1411)),
            z: DeviceId(NodeId(id.0 * 1000 + 1412)),
            probe,
            x_um: 0.0,
            y_um: 0.0,
            z_um: 0.0,
            speed_x_mm_s: 20.0,
            speed_y_mm_s: 20.0,
            speed_z_mm_s: 5.0,
            accel_x_m_s2: 0.2,
            accel_y_m_s2: 0.2,
            accel_z_m_s2: 0.2,
            limit_x,
            limit_y,
            limit_z,
            last_error,
            busy: false,
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            codec: SerialLineCodec::new(protocol::SEND_ENDING, protocol::RECV_ENDING),
            serial_port: None,
            baud_rate: 57_600,
            serial_timeout_ms: 1,
            connected: false,
        }
    }

    pub fn new_configured(
        id: DriverId,
        configured: MarzhauserConfiguredProbe,
        serial: Box<dyn SerialIo>,
        connected: bool,
    ) -> Self {
        let mut driver = Self::new(
            id,
            configured.probe,
            configured.limit_x,
            configured.limit_y,
            configured.limit_z,
            configured.last_error,
            serial,
        );
        driver.serial_port = configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.port_name.clone());
        driver.baud_rate = configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.baud_rate)
            .unwrap_or(57_600);
        driver.serial_timeout_ms = configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.timeout_ms)
            .unwrap_or(1);
        driver.connected = connected;
        driver
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    #[cfg(feature = "os-serial")]
    fn with_probe_result(mut self, result: protocol::MarzhauserProbeResult) -> Self {
        self.x_um = result.x_um.clamp(0.0, self.probe.x_travel_um);
        self.y_um = result.y_um.clamp(0.0, self.probe.y_travel_um);
        self.z_um = result.z_um.clamp(0.0, self.probe.z_travel_um);
        if let Some(speed) = result.speed_x_mm_s {
            self.speed_x_mm_s = speed;
        }
        if let Some(speed) = result.speed_y_mm_s {
            self.speed_y_mm_s = speed;
        }
        if let Some(speed) = result.speed_z_mm_s {
            self.speed_z_mm_s = speed;
        }
        if let Some(accel) = result.accel_x_m_s2 {
            self.accel_x_m_s2 = accel;
        }
        if let Some(accel) = result.accel_y_m_s2 {
            self.accel_y_m_s2 = accel;
        }
        if let Some(accel) = result.accel_z_m_s2 {
            self.accel_z_m_s2 = accel;
        }
        self.busy = result.busy;
        self.last_error = result.last_error;
        self
    }

    fn send(&mut self, command: protocol::MarzhauserCommand) -> Result<()> {
        let line = protocol::encode(&command);
        self.serial.write(&self.codec.encode(&line))
    }

    fn query_for_property(
        &self,
        device: DeviceId,
        key: &str,
    ) -> Option<protocol::MarzhauserCommand> {
        match (device, key) {
            (device, "version") if device == self.hub => Some(protocol::MarzhauserCommand::Version),
            (device, "configuration") if device == self.hub => {
                Some(protocol::MarzhauserCommand::DetectConfiguration)
            }
            (device, "last_error") | (device, "fault") if device == self.hub => {
                Some(protocol::MarzhauserCommand::QueryError)
            }
            (device, "busy") if device == self.hub || device == self.xy || device == self.z => {
                Some(protocol::MarzhauserCommand::QueryStatusAxis)
            }
            (device, "x") | (device, "y") if device == self.xy => {
                Some(protocol::MarzhauserCommand::QueryPosition)
            }
            (device, "z") if device == self.z => {
                Some(protocol::MarzhauserCommand::QueryPositionAxis { axis: 'z' })
            }
            (device, "speed_x") if device == self.xy => {
                Some(protocol::MarzhauserCommand::QueryVelocity { axis: 'x' })
            }
            (device, "speed_y") if device == self.xy => {
                Some(protocol::MarzhauserCommand::QueryVelocity { axis: 'y' })
            }
            (device, "accel_x") if device == self.xy => {
                Some(protocol::MarzhauserCommand::QueryAcceleration { axis: 'x' })
            }
            (device, "accel_y") if device == self.xy => {
                Some(protocol::MarzhauserCommand::QueryAcceleration { axis: 'y' })
            }
            (device, "limit_x") if device == self.xy => {
                Some(protocol::MarzhauserCommand::QueryLimit { axis: 'x' })
            }
            (device, "limit_y") if device == self.xy => {
                Some(protocol::MarzhauserCommand::QueryLimit { axis: 'y' })
            }
            (device, "speed") if device == self.z => {
                Some(protocol::MarzhauserCommand::QueryVelocity { axis: 'z' })
            }
            (device, "accel") if device == self.z => {
                Some(protocol::MarzhauserCommand::QueryAcceleration { axis: 'z' })
            }
            (device, "limit") if device == self.z => {
                Some(protocol::MarzhauserCommand::QueryLimit { axis: 'z' })
            }
            _ => None,
        }
    }

    fn read_query_reply(
        &mut self,
        device: DeviceId,
        command: &protocol::MarzhauserCommand,
    ) -> Result<()> {
        let bytes = self.serial.read_available()?;
        if bytes.is_empty() {
            return Ok(());
        }
        let lines = self.codec.push(&bytes);
        for line in lines {
            self.apply_readback_reply(device, command, &line)?;
        }
        Ok(())
    }

    fn refresh_property_readback(&mut self, device: DeviceId, key: &str) -> Result<()> {
        let Some(query) = self.query_for_property(device, key) else {
            return Ok(());
        };
        self.send(query.clone())?;
        self.read_query_reply(device, &query)
    }

    fn refresh_xy_motion_readback(&mut self) -> Result<()> {
        self.refresh_property_readback(self.xy, "busy")?;
        self.refresh_property_readback(self.xy, "x")?;
        self.refresh_property_readback(self.hub, "last_error")
    }

    fn refresh_z_motion_readback(&mut self) -> Result<()> {
        self.refresh_property_readback(self.z, "busy")?;
        self.refresh_property_readback(self.z, "z")?;
        self.refresh_property_readback(self.hub, "last_error")
    }

    fn refresh_targets_for(command: &str) -> Result<Vec<(u8, &'static str)>> {
        match command {
            "refresh_readbacks" => Ok(vec![
                (0, "version"),
                (0, "configuration"),
                (0, "busy"),
                (0, "last_error"),
                (1, "x"),
                (2, "z"),
                (1, "speed_x"),
                (1, "speed_y"),
                (2, "speed"),
                (1, "accel_x"),
                (1, "accel_y"),
                (2, "accel"),
                (1, "limit_x"),
                (1, "limit_y"),
                (2, "limit"),
            ]),
            "refresh_identity" => Ok(vec![(0, "version"), (0, "configuration")]),
            "refresh_status" => Ok(vec![(0, "busy"), (0, "last_error")]),
            "refresh_position" => Ok(vec![(1, "x"), (2, "z")]),
            "refresh_profiles" => Ok(vec![
                (1, "speed_x"),
                (1, "speed_y"),
                (2, "speed"),
                (1, "accel_x"),
                (1, "accel_y"),
                (2, "accel"),
            ]),
            "refresh_limits" => Ok(vec![(1, "limit_x"), (1, "limit_y"), (2, "limit")]),
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "Marzhauser GenericCommand supports refresh_readbacks, refresh_identity, refresh_status, refresh_position, refresh_profiles, and refresh_limits; got {other}"
                ),
            )),
        }
    }

    fn actual_refresh_target(&self, target: u8, key: &'static str) -> (DeviceId, &'static str) {
        let device = match target {
            0 => self.hub,
            1 => self.xy,
            _ => self.z,
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
                "Marzhauser GenericCommand does not take parameters",
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
            self.refresh_property_readback(device, key)?;
        }
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("commands".into(), Value::I64(targets.len() as i64)),
            ("state".into(), self.state_summary()),
            (
                "completion_basis".into(),
                Value::String("Marzhauser mapped query readback".into()),
            ),
        ])))
    }

    fn apply_readback_reply(
        &mut self,
        device: DeviceId,
        command: &protocol::MarzhauserCommand,
        reply: &str,
    ) -> Result<()> {
        match command {
            protocol::MarzhauserCommand::Version => {
                self.probe.version = reply.trim().to_string();
                self.emit_property(
                    self.hub,
                    "version",
                    Value::String(self.probe.version.clone()),
                );
            }
            protocol::MarzhauserCommand::DetectConfiguration => {
                self.probe.configuration = protocol::parse_u16_reply("?det", reply)?;
                self.emit_property(
                    self.hub,
                    "configuration",
                    Value::I64(self.probe.configuration as i64),
                );
            }
            protocol::MarzhauserCommand::QueryStatusAxis => {
                self.busy = protocol::is_busy_status(reply);
                for target in [self.hub, self.xy, self.z] {
                    self.emit_property(target, "busy", Value::Bool(self.busy));
                }
            }
            protocol::MarzhauserCommand::QueryError => {
                self.last_error = reply.trim().to_string();
                self.emit_property(
                    self.hub,
                    "last_error",
                    Value::String(self.last_error.clone()),
                );
                self.emit_property(
                    self.hub,
                    "fault",
                    Value::Bool(protocol::error_is_fault(&self.last_error)),
                );
            }
            protocol::MarzhauserCommand::QueryPosition => {
                let values = protocol::parse_f64_list("?pos", reply)?;
                if values.len() < 2 {
                    return Err(Error::new(
                        ErrorCode::Transport,
                        format!("Marzhauser ?pos reply needs x/y values: {reply}"),
                    ));
                }
                self.x_um = values[0] * 1000.0;
                self.y_um = values[1] * 1000.0;
                self.emit_property(self.xy, "x", position(self.x_um));
                self.emit_property(self.xy, "y", position(self.y_um));
            }
            protocol::MarzhauserCommand::QueryPositionAxis { axis: 'z' } => {
                self.z_um = protocol::parse_f64_reply("?pos z", reply)? * 1000.0;
                self.emit_property(self.z, "z", position(self.z_um));
            }
            protocol::MarzhauserCommand::QueryVelocity { axis } => {
                let rev_s = protocol::parse_f64_reply(&format!("?vel {axis}"), reply)?;
                match axis {
                    'x' => {
                        self.speed_x_mm_s = rev_s * self.probe.pitch_x_mm;
                        self.emit_property(self.xy, "speed_x", velocity(self.speed_x_mm_s));
                    }
                    'y' => {
                        self.speed_y_mm_s = rev_s * self.probe.pitch_y_mm;
                        self.emit_property(self.xy, "speed_y", velocity(self.speed_y_mm_s));
                    }
                    'z' => {
                        self.speed_z_mm_s = rev_s * self.probe.pitch_z_mm;
                        self.emit_property(self.z, "speed", velocity(self.speed_z_mm_s));
                    }
                    _ => {}
                }
            }
            protocol::MarzhauserCommand::QueryAcceleration { axis } => {
                let value = protocol::parse_f64_reply(&format!("?accel {axis}"), reply)?;
                match axis {
                    'x' => {
                        self.accel_x_m_s2 = value;
                        self.emit_property(self.xy, "accel_x", acceleration(self.accel_x_m_s2));
                    }
                    'y' => {
                        self.accel_y_m_s2 = value;
                        self.emit_property(self.xy, "accel_y", acceleration(self.accel_y_m_s2));
                    }
                    'z' => {
                        self.accel_z_m_s2 = value;
                        self.emit_property(self.z, "accel", acceleration(self.accel_z_m_s2));
                    }
                    _ => {}
                }
            }
            protocol::MarzhauserCommand::QueryLimit { axis } => {
                let value = reply.trim().to_string();
                match axis {
                    'x' => {
                        self.limit_x = Some(value.clone());
                        self.emit_property(self.xy, "limit_x", Value::String(value));
                    }
                    'y' => {
                        self.limit_y = Some(value.clone());
                        self.emit_property(self.xy, "limit_y", Value::String(value));
                    }
                    'z' => {
                        self.limit_z = Some(value.clone());
                        self.emit_property(self.z, "limit", Value::String(value));
                    }
                    _ => {}
                }
            }
            _ => {
                let _ = device;
            }
        }
        Ok(())
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        vec![
            DeviceDescriptor {
                id: self.hub,
                driver: self.id,
                label: "marzhauser-hub".into(),
                vendor: Some("Marzhauser".into()),
                model: Some(self.probe.controller.clone()),
                serial: None,
                kinds: vec![
                    "hub".into(),
                    "motion.controller".into(),
                    "serial.ascii".into(),
                ],
                properties: vec![
                    property("version", "Version", ValueType::String, None, false, None),
                    property(
                        "configuration",
                        "Configuration",
                        ValueType::I64,
                        None,
                        false,
                        None,
                    ),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                    property(
                        "last_error",
                        "Last error",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                    property("fault", "Fault", ValueType::Bool, None, false, None),
                    property(
                        "state_summary",
                        "State summary",
                        ValueType::Map,
                        None,
                        false,
                        None,
                    ),
                ],
                metadata: BTreeMap::from([
                    ("version".into(), Value::String(self.probe.version.clone())),
                    (
                        "configuration".into(),
                        Value::I64(self.probe.configuration as i64),
                    ),
                    ("autostatus".into(), Value::String("disabled".into())),
                ]),
            },
            DeviceDescriptor {
                id: self.xy,
                driver: self.id,
                label: "marzhauser-xy-stage".into(),
                vendor: Some("Marzhauser".into()),
                model: Some("L-Step/TANGO XY".into()),
                serial: None,
                kinds: vec!["axis.xy".into(), "stage.xy".into()],
                properties: vec![
                    sequenceable_position_property("x", "X position", true, self.probe.x_travel_um),
                    sequenceable_position_property("y", "Y position", true, self.probe.y_travel_um),
                    sequenceable_velocity_property("speed_x", "X speed", true, 20.0),
                    sequenceable_velocity_property("speed_y", "Y speed", true, 20.0),
                    sequenceable_acceleration_property("accel_x", "X acceleration", true, 20.0),
                    sequenceable_acceleration_property("accel_y", "Y acceleration", true, 20.0),
                    property(
                        "limit_x",
                        "X limit reply",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                    property(
                        "limit_y",
                        "Y limit reply",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                ],
                metadata: BTreeMap::from([
                    ("pitch_x".into(), position_mm(self.probe.pitch_x_mm)),
                    ("pitch_y".into(), position_mm(self.probe.pitch_y_mm)),
                    (
                        "legacy_pitch_x_mm".into(),
                        position_mm(self.probe.pitch_x_mm),
                    ),
                    (
                        "legacy_pitch_y_mm".into(),
                        position_mm(self.probe.pitch_y_mm),
                    ),
                    ("step_size_x".into(), position(self.probe.step_size_x_um())),
                    ("step_size_y".into(), position(self.probe.step_size_y_um())),
                    (
                        "legacy_step_size_x_um".into(),
                        position(self.probe.step_size_x_um()),
                    ),
                    (
                        "legacy_step_size_y_um".into(),
                        position(self.probe.step_size_y_um()),
                    ),
                    (
                        "limit_x".into(),
                        Value::String(self.limit_x.clone().unwrap_or_else(|| "unknown".into())),
                    ),
                    (
                        "limit_y".into(),
                        Value::String(self.limit_y.clone().unwrap_or_else(|| "unknown".into())),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.z,
                driver: self.id,
                label: "marzhauser-z-stage".into(),
                vendor: Some("Marzhauser".into()),
                model: Some("L-Step/TANGO Z".into()),
                serial: None,
                kinds: vec!["axis.z".into(), "stage.z".into()],
                properties: vec![
                    sequenceable_position_property("z", "Z position", true, self.probe.z_travel_um),
                    sequenceable_velocity_property("speed", "Z speed", true, 20.0),
                    sequenceable_acceleration_property("accel", "Z acceleration", true, 20.0),
                    property(
                        "limit",
                        "Z limit reply",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                ],
                metadata: BTreeMap::from([
                    ("pitch_z".into(), position_mm(self.probe.pitch_z_mm)),
                    (
                        "legacy_pitch_z_mm".into(),
                        position_mm(self.probe.pitch_z_mm),
                    ),
                    ("step_size_z".into(), position(self.probe.step_size_z_um())),
                    (
                        "legacy_step_size_z_um".into(),
                        position(self.probe.step_size_z_um()),
                    ),
                    (
                        "limit_z".into(),
                        Value::String(self.limit_z.clone().unwrap_or_else(|| "unknown".into())),
                    ),
                ]),
            },
        ]
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        match (device, key) {
            (device, "version") if device == self.hub => {
                Ok(Value::String(self.probe.version.clone()))
            }
            (device, "configuration") if device == self.hub => {
                Ok(Value::I64(self.probe.configuration as i64))
            }
            (device, "busy") if device == self.hub || device == self.xy || device == self.z => {
                Ok(Value::Bool(self.busy))
            }
            (device, "last_error") if device == self.hub => {
                Ok(Value::String(self.last_error.clone()))
            }
            (device, "fault") if device == self.hub => {
                Ok(Value::Bool(protocol::error_is_fault(&self.last_error)))
            }
            (device, "state_summary") if device == self.hub => Ok(self.state_summary()),
            (device, "x") if device == self.xy => Ok(position(self.x_um)),
            (device, "y") if device == self.xy => Ok(position(self.y_um)),
            (device, "speed_x") if device == self.xy => Ok(velocity(self.speed_x_mm_s)),
            (device, "speed_y") if device == self.xy => Ok(velocity(self.speed_y_mm_s)),
            (device, "accel_x") if device == self.xy => Ok(acceleration(self.accel_x_m_s2)),
            (device, "accel_y") if device == self.xy => Ok(acceleration(self.accel_y_m_s2)),
            (device, "limit_x") if device == self.xy => Ok(Value::String(
                self.limit_x.clone().unwrap_or_else(|| "unknown".into()),
            )),
            (device, "limit_y") if device == self.xy => Ok(Value::String(
                self.limit_y.clone().unwrap_or_else(|| "unknown".into()),
            )),
            (device, "z") if device == self.z => Ok(position(self.z_um)),
            (device, "speed") if device == self.z => Ok(velocity(self.speed_z_mm_s)),
            (device, "accel") if device == self.z => Ok(acceleration(self.accel_z_m_s2)),
            (device, "limit") if device == self.z => Ok(Value::String(
                self.limit_z.clone().unwrap_or_else(|| "unknown".into()),
            )),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Marzhauser property {key}"),
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
            (device, "speed_x", value) if device == self.xy => {
                let speed = velocity_mm_s(value)?;
                self.send(protocol::MarzhauserCommand::SetDimension {
                    axis: "x".into(),
                    dim: 2,
                })?;
                self.send(protocol::MarzhauserCommand::SetVelocity {
                    axis: 'x',
                    rev_per_s: speed / self.probe.pitch_x_mm,
                })?;
                self.speed_x_mm_s = speed;
                Ok(velocity(self.speed_x_mm_s))
            }
            (device, "speed_y", value) if device == self.xy => {
                let speed = velocity_mm_s(value)?;
                self.send(protocol::MarzhauserCommand::SetDimension {
                    axis: "y".into(),
                    dim: 2,
                })?;
                self.send(protocol::MarzhauserCommand::SetVelocity {
                    axis: 'y',
                    rev_per_s: speed / self.probe.pitch_y_mm,
                })?;
                self.speed_y_mm_s = speed;
                Ok(velocity(self.speed_y_mm_s))
            }
            (device, "accel_x", value) if device == self.xy => {
                let accel = acceleration_m_s2(value)?;
                self.send(protocol::MarzhauserCommand::SetAcceleration {
                    axis: 'x',
                    acceleration: accel,
                })?;
                self.accel_x_m_s2 = accel;
                Ok(acceleration(self.accel_x_m_s2))
            }
            (device, "accel_y", value) if device == self.xy => {
                let accel = acceleration_m_s2(value)?;
                self.send(protocol::MarzhauserCommand::SetAcceleration {
                    axis: 'y',
                    acceleration: accel,
                })?;
                self.accel_y_m_s2 = accel;
                Ok(acceleration(self.accel_y_m_s2))
            }
            (device, "z", value) if device == self.z => {
                self.move_z(position_um(value)?.clamp(0.0, self.probe.z_travel_um))?;
                Ok(position(self.z_um))
            }
            (device, "speed", value) if device == self.z => {
                let speed = velocity_mm_s(value)?;
                self.send(protocol::MarzhauserCommand::SetDimension {
                    axis: "z".into(),
                    dim: 2,
                })?;
                self.send(protocol::MarzhauserCommand::SetVelocity {
                    axis: 'z',
                    rev_per_s: speed / self.probe.pitch_z_mm,
                })?;
                self.speed_z_mm_s = speed;
                Ok(velocity(self.speed_z_mm_s))
            }
            (device, "accel", value) if device == self.z => {
                let accel = acceleration_m_s2(value)?;
                self.send(protocol::MarzhauserCommand::SetAcceleration {
                    axis: 'z',
                    acceleration: accel,
                })?;
                self.accel_z_m_s2 = accel;
                Ok(acceleration(self.accel_z_m_s2))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid Marzhauser write {key}"),
            )),
        }
    }

    fn move_xy(&mut self, x_um: f64, y_um: f64) -> Result<()> {
        self.send(protocol::MarzhauserCommand::SetDimension {
            axis: "1".into(),
            dim: 1,
        })?;
        self.send(protocol::MarzhauserCommand::MoveAbsXy {
            x: x_um / 1000.0,
            y: y_um / 1000.0,
        })?;
        self.x_um = x_um;
        self.y_um = y_um;
        self.finish_motion("marzhauser xy ?statusaxis busy then idle");
        Ok(())
    }

    fn move_xy_relative(&mut self, dx_um: f64, dy_um: f64) -> Result<()> {
        let next_x = (self.x_um + dx_um).clamp(0.0, self.probe.x_travel_um);
        let next_y = (self.y_um + dy_um).clamp(0.0, self.probe.y_travel_um);
        self.send(protocol::MarzhauserCommand::SetDimension {
            axis: "1".into(),
            dim: 1,
        })?;
        self.send(protocol::MarzhauserCommand::MoveRelXy {
            dx: (next_x - self.x_um) / 1000.0,
            dy: (next_y - self.y_um) / 1000.0,
        })?;
        self.x_um = next_x;
        self.y_um = next_y;
        self.finish_motion("marzhauser xy relative ?statusaxis busy then idle");
        Ok(())
    }

    fn move_z(&mut self, z_um: f64) -> Result<()> {
        self.send(protocol::MarzhauserCommand::SetDimension {
            axis: "z".into(),
            dim: 1,
        })?;
        self.send(protocol::MarzhauserCommand::MoveAbsZ { z: z_um / 1000.0 })?;
        self.z_um = z_um;
        self.finish_motion("marzhauser z ?statusaxis busy then idle");
        Ok(())
    }

    fn move_z_relative(&mut self, dz_um: f64) -> Result<()> {
        let next_z = (self.z_um + dz_um).clamp(0.0, self.probe.z_travel_um);
        self.send(protocol::MarzhauserCommand::SetDimension {
            axis: "z".into(),
            dim: 1,
        })?;
        self.send(protocol::MarzhauserCommand::MoveRelZ {
            dz: (next_z - self.z_um) / 1000.0,
        })?;
        self.z_um = next_z;
        self.finish_motion("marzhauser z relative ?statusaxis busy then idle");
        Ok(())
    }

    fn validate_stage_move(&self, device: DeviceId, request: &StageMoveRequest) -> Result<()> {
        if request.target.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Marzhauser StageMove target must contain at least one axis",
            ));
        }
        for axis in request.target.keys() {
            match (device, axis) {
                (device, StageAxis::X | StageAxis::Y) if device == self.xy => {}
                (device, StageAxis::Z) if device == self.z => {}
                (device, StageAxis::Custom(name))
                    if device == self.xy && (name == "x" || name == "y") => {}
                (device, StageAxis::Custom(name)) if device == self.z && name == "z" => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "Marzhauser StageMove axis does not belong to the target device",
                    ))
                }
            }
        }
        Ok(())
    }

    fn apply_stage_move_profile(
        &mut self,
        device: DeviceId,
        request: &StageMoveRequest,
    ) -> Result<()> {
        let Some(profile) = &request.profile else {
            return Ok(());
        };
        let velocity = profile
            .velocity
            .as_ref()
            .map(|velocity| velocity.micrometers_per_second() / 1000.0);
        let acceleration = profile
            .acceleration
            .as_ref()
            .map(|accel| accel.meters_per_second_squared());

        if device == self.xy {
            let has_x = request.target.keys().any(is_x_axis);
            let has_y = request.target.keys().any(is_y_axis);
            if has_x {
                self.apply_stage_axis_profile('x', self.probe.pitch_x_mm, velocity, acceleration)?;
                if let Some(speed) = velocity {
                    self.speed_x_mm_s = speed;
                }
                if let Some(accel) = acceleration {
                    self.accel_x_m_s2 = accel;
                }
            }
            if has_y {
                self.apply_stage_axis_profile('y', self.probe.pitch_y_mm, velocity, acceleration)?;
                if let Some(speed) = velocity {
                    self.speed_y_mm_s = speed;
                }
                if let Some(accel) = acceleration {
                    self.accel_y_m_s2 = accel;
                }
            }
        } else if device == self.z {
            self.apply_stage_axis_profile('z', self.probe.pitch_z_mm, velocity, acceleration)?;
            if let Some(speed) = velocity {
                self.speed_z_mm_s = speed;
            }
            if let Some(accel) = acceleration {
                self.accel_z_m_s2 = accel;
            }
        }
        Ok(())
    }

    fn apply_stage_axis_profile(
        &mut self,
        axis: char,
        pitch_mm: f64,
        velocity_mm_s: Option<f64>,
        acceleration_m_s2: Option<f64>,
    ) -> Result<()> {
        if let Some(speed) = velocity_mm_s {
            self.send(protocol::MarzhauserCommand::SetDimension {
                axis: axis.to_string(),
                dim: 2,
            })?;
            self.send(protocol::MarzhauserCommand::SetVelocity {
                axis,
                rev_per_s: speed / pitch_mm,
            })?;
        }
        if let Some(acceleration) = acceleration_m_s2 {
            self.send(protocol::MarzhauserCommand::SetAcceleration { axis, acceleration })?;
        }
        Ok(())
    }

    fn stage_move(&mut self, device: DeviceId, request: StageMoveRequest) -> Result<Value> {
        self.validate_stage_move(device, &request)?;
        self.apply_stage_move_profile(device, &request)?;
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
                ("speed_x".into(), velocity(self.speed_x_mm_s)),
                ("speed_y".into(), velocity(self.speed_y_mm_s)),
                ("accel_x".into(), acceleration(self.accel_x_m_s2)),
                ("accel_y".into(), acceleration(self.accel_y_m_s2)),
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
                ("speed".into(), velocity(self.speed_z_mm_s)),
                ("accel".into(), acceleration(self.accel_z_m_s2)),
            ])))
        } else {
            Err(Error::new(
                ErrorCode::InvalidCommand,
                "Marzhauser StageMove target device must be XY or Z stage",
            ))
        }
    }

    fn apply_state_set(&mut self, set: StateSet) -> Result<Value> {
        let mut next_x = self.x_um;
        let mut next_y = self.y_um;
        let mut next_z = self.z_um;
        let mut xy_changed = false;
        let mut z_changed = false;
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
        for write in remaining {
            let value = self.write_property(write.device, &write.property, &write.value)?;
            self.emit_property(write.device, &write.property, value.clone());
            changed.insert(format!("{}:{}", (write.device.0).0, write.property), value);
        }
        Ok(Value::Map(changed))
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
                (device, "x" | "y" | "speed_x" | "speed_y" | "accel_x" | "accel_y")
                    if device == self.xy => {}
                (device, "z" | "speed" | "accel") if device == self.z => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Marzhauser timing sequences can only target position, speed, or acceleration endpoints",
                    ))
                }
            }
            for value in &sequence.values {
                match sequence.property.as_str() {
                    "x" | "y" | "z" => {
                        let _ = position_um(value)?;
                    }
                    "speed_x" | "speed_y" | "speed" => {
                        let _ = velocity_mm_s(value)?;
                    }
                    "accel_x" | "accel_y" | "accel" => {
                        let _ = acceleration_m_s2(value)?;
                    }
                    _ => unreachable!("validated Marzhauser timing property"),
                }
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
            ("x".into(), position(self.x_um)),
            ("y".into(), position(self.y_um)),
            ("z".into(), position(self.z_um)),
            (
                "sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
        ]))
    }

    fn axis_state_summary(
        &self,
        axis: &str,
        position_um: f64,
        travel_um: f64,
        pitch_mm: f64,
        step_size_um: f64,
        speed_mm_s: f64,
        acceleration_m_s2: f64,
        limit: &Option<String>,
    ) -> Value {
        Value::Map(BTreeMap::from([
            ("axis".into(), Value::String(axis.into())),
            ("position".into(), position(position_um)),
            ("target".into(), position(position_um)),
            ("travel".into(), position(travel_um)),
            ("pitch".into(), position_mm(pitch_mm)),
            ("step_size".into(), position(step_size_um)),
            ("speed".into(), velocity(speed_mm_s)),
            ("acceleration".into(), acceleration(acceleration_m_s2)),
            (
                "limit".into(),
                Value::String(limit.clone().unwrap_or_else(|| "unknown".into())),
            ),
        ]))
    }

    fn state_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("version".into(), Value::String(self.probe.version.clone())),
            (
                "controller".into(),
                Value::String(self.probe.controller.clone()),
            ),
            (
                "configuration".into(),
                Value::I64(self.probe.configuration as i64),
            ),
            ("busy".into(), Value::Bool(self.busy)),
            ("last_error".into(), Value::String(self.last_error.clone())),
            (
                "fault".into(),
                Value::Bool(protocol::error_is_fault(&self.last_error)),
            ),
            ("autostatus".into(), Value::String("disabled".into())),
            ("xy_device".into(), Value::I64((self.xy.0).0 as i64)),
            ("z_device".into(), Value::I64((self.z.0).0 as i64)),
            (
                "x".into(),
                self.axis_state_summary(
                    "x",
                    self.x_um,
                    self.probe.x_travel_um,
                    self.probe.pitch_x_mm,
                    self.probe.step_size_x_um(),
                    self.speed_x_mm_s,
                    self.accel_x_m_s2,
                    &self.limit_x,
                ),
            ),
            (
                "y".into(),
                self.axis_state_summary(
                    "y",
                    self.y_um,
                    self.probe.y_travel_um,
                    self.probe.pitch_y_mm,
                    self.probe.step_size_y_um(),
                    self.speed_y_mm_s,
                    self.accel_y_m_s2,
                    &self.limit_y,
                ),
            ),
            (
                "z".into(),
                self.axis_state_summary(
                    "z",
                    self.z_um,
                    self.probe.z_travel_um,
                    self.probe.pitch_z_mm,
                    self.probe.step_size_z_um(),
                    self.speed_z_mm_s,
                    self.accel_z_m_s2,
                    &self.limit_z,
                ),
            ),
        ]))
    }

    fn apply_timing_sequence_step(&mut self, plan: &TimingPlan, first: bool) -> Result<Value> {
        let mut writes = Vec::new();
        for sequence in self.local_timing_sequences(plan) {
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
                "marzhauser timing start sequence".into()
            } else {
                "marzhauser timing stop sequence".into()
            }),
            writes,
            commit: CommitMode::Immediate,
        })
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
                "unknown Marzhauser capability",
            ));
        };
        match (capability.kind, request) {
            (CapabilityKind::StageMove, CapabilityRequest::StageMove(request))
                if device == self.xy || device == self.z =>
            {
                self.stage_move(device, request)
            }
            (CapabilityKind::StageMove, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Marzhauser StageMove expects a StageMoveRequest",
            )),
            (CapabilityKind::StageHome, CapabilityRequest::None) if device == self.xy => {
                self.send(protocol::MarzhauserCommand::Calibrate { axis: 'x' })?;
                self.send(protocol::MarzhauserCommand::Calibrate { axis: 'y' })?;
                self.x_um = 0.0;
                self.y_um = 0.0;
                self.finish_motion("marzhauser xy calibration complete");
                self.emit_property(self.xy, "x", position(self.x_um));
                self.emit_property(self.xy, "y", position(self.y_um));
                self.refresh_xy_motion_readback()?;
                Ok(Value::String("xy calibrated".into()))
            }
            (CapabilityKind::StageHome, CapabilityRequest::None) if device == self.z => {
                self.send(protocol::MarzhauserCommand::Calibrate { axis: 'z' })?;
                self.z_um = 0.0;
                self.finish_motion("marzhauser z calibration complete");
                self.emit_property(self.z, "z", position(self.z_um));
                self.refresh_z_motion_readback()?;
                Ok(Value::String("z calibrated".into()))
            }
            (CapabilityKind::StageStop, CapabilityRequest::None)
                if device == self.xy || device == self.z =>
            {
                self.send(protocol::MarzhauserCommand::Abort)?;
                self.busy = false;
                if device == self.xy {
                    self.refresh_xy_motion_readback()?;
                } else {
                    self.refresh_z_motion_readback()?;
                }
                Ok(Value::String("aborted".into()))
            }
            (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Marzhauser home/stop capabilities take no request",
            )),
            (CapabilityKind::GenericCommand, CapabilityRequest::GenericCommand(request))
                if device == self.hub =>
            {
                self.apply_generic_command(request)
            }
            (CapabilityKind::GenericCommand, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Marzhauser GenericCommand expects GenericCommandRequest",
            )),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Marzhauser capability",
            )),
        }
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

impl Driver for MarzhauserDriver {
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
            label: "marzhauser-serial".into(),
            kind: "serial".into(),
            metadata: BTreeMap::from([
                ("send_terminator".into(), Value::String("CR".into())),
                ("recv_terminator".into(), Value::String("CR".into())),
                (
                    "completion".into(),
                    Value::String("?statusaxis and ?err report hardware state".into()),
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
                (
                    "startup_readback_supported".into(),
                    Value::List(
                        protocol::probe_script()
                            .into_iter()
                            .map(Value::String)
                            .collect(),
                    ),
                ),
            ]),
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.hub {
            vec![capability(1, device, CapabilityKind::GenericCommand)]
        } else if device == self.xy || device == self.z {
            vec![
                capability(1, device, CapabilityKind::StageMove),
                capability(2, device, CapabilityKind::StageHome),
                capability(3, device, CapabilityKind::StageStop),
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
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("marzhauser read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("marzhauser write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "marzhauser remultiplexed XY/Z state set".into(),
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
                            Error::new(ErrorCode::Unsupported, "unknown Marzhauser capability")
                        })?;
                    match (&candidate.kind, request) {
                        (CapabilityKind::StageMove, CapabilityRequest::StageMove(request)) => {
                            self.validate_stage_move(*device, request)?;
                        }
                        (
                            CapabilityKind::StageHome | CapabilityKind::StageStop,
                            CapabilityRequest::None,
                        ) => {}
                        (
                            CapabilityKind::GenericCommand,
                            CapabilityRequest::GenericCommand(request),
                        ) if *device == self.hub => {
                            self.validate_generic_command(request)?;
                        }
                        (CapabilityKind::StageMove, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Marzhauser StageMove expects a StageMoveRequest",
                            ));
                        }
                        (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Marzhauser home/stop capabilities take no request",
                            ));
                        }
                        (CapabilityKind::GenericCommand, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Marzhauser GenericCommand expects GenericCommandRequest",
                            ));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported Marzhauser capability",
                            ));
                        }
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("marzhauser invoke {}", capability.0),
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
                        description: "marzhauser timing arm summary".into(),
                        payload: self.timing_summary(plan, "arm"),
                    });
                }
                Command::Start(_) | Command::Stop(_) => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "Marzhauser direct timing transitions are runtime-owned",
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
                    self.refresh_property_readback(device, &key)?;
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
                        message: format!("marzhauser serial: {line}"),
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
                description: "marzhauser timing arm summary".into(),
                payload: self.timing_summary(plan, "arm"),
            }],
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
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "marzhauser timing start sequence".into(),
                payload: Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "start")),
                    ("changed".into(), changed),
                ])),
            }],
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
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "marzhauser timing stop sequence".into(),
                payload: Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "stop")),
                    ("changed".into(), changed),
                ])),
            }],
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
    schema.sequenceable = true;
    schema
}

fn velocity_property(
    key: &str,
    display_name: &str,
    writable: bool,
    max_mm_s: f64,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Velocity,
        Some("mm/s"),
        writable,
        Some(Range {
            min: velocity(0.0),
            max: velocity(max_mm_s),
        }),
    )
}

fn sequenceable_velocity_property(
    key: &str,
    display_name: &str,
    writable: bool,
    max_mm_s: f64,
) -> PropertySchema {
    let mut schema = velocity_property(key, display_name, writable, max_mm_s);
    schema.sequenceable = writable;
    schema
}

fn acceleration_property(
    key: &str,
    display_name: &str,
    writable: bool,
    max_m_s2: f64,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Acceleration,
        Some("m/s^2"),
        writable,
        Some(Range {
            min: acceleration(0.0),
            max: acceleration(max_m_s2),
        }),
    )
}

fn sequenceable_acceleration_property(
    key: &str,
    display_name: &str,
    writable: bool,
    max_m_s2: f64,
) -> PropertySchema {
    let mut schema = acceleration_property(key, display_name, writable, max_m_s2);
    schema.sequenceable = writable;
    schema
}

fn position(value_um: f64) -> Value {
    Value::Position(Position::from_micrometers(value_um))
}

fn position_mm(value_mm: f64) -> Value {
    Value::Position(Position::from_millimeters(value_mm))
}

fn velocity(value_mm_s: f64) -> Value {
    Value::Velocity(Velocity::from_millimeters_per_second(value_mm_s))
}

fn acceleration(value_m_s2: f64) -> Value {
    Value::Acceleration(Acceleration::from_meters_per_second_squared(value_m_s2))
}

fn is_x_axis(axis: &StageAxis) -> bool {
    match axis {
        StageAxis::X => true,
        StageAxis::Custom(name) => name == "x",
        _ => false,
    }
}

fn is_y_axis(axis: &StageAxis) -> bool {
    match axis {
        StageAxis::Y => true,
        StageAxis::Custom(name) => name == "y",
        _ => false,
    }
}

fn position_um(value: &Value) -> Result<f64> {
    match value {
        Value::Position(position) => Ok(position.micrometers()),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("expected position value, got {other:?}"),
        )),
    }
}

fn velocity_mm_s(value: &Value) -> Result<f64> {
    match value {
        Value::Velocity(velocity) => Ok(velocity.micrometers_per_second() / 1000.0),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("expected velocity value, got {other:?}"),
        )),
    }
}

fn acceleration_m_s2(value: &Value) -> Result<f64> {
    match value {
        Value::Acceleration(acceleration) => Ok(acceleration.meters_per_second_squared()),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("expected acceleration value, got {other:?}"),
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

fn f64_prop(device: &DeviceConfig, key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
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

fn position_config_mm(device: &DeviceConfig, key: &str, legacy_key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::Position(value)) => Some(value.meters() * 1_000.0),
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => f64_prop(device, legacy_key),
    }
}

fn u16_prop(device: &DeviceConfig, key: &str) -> Option<u16> {
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
