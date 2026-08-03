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

    pub const SEND_ENDING: LineEnding = LineEnding::Lf;
    pub const RECV_ENDING: LineEnding = LineEnding::Lf;
    pub const MOVING_STATUS_BYTE: u8 = 5;

    #[derive(Debug, Clone, PartialEq)]
    pub struct PiGcsAxis {
        pub name: String,
        pub travel_um: f64,
        pub referenced: bool,
        pub servo: bool,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct PiGcsProbe {
        pub controller_id: String,
        pub syntax_version: Option<f64>,
        pub x_axis: PiGcsAxis,
        pub y_axis: PiGcsAxis,
        pub z_axis: PiGcsAxis,
        pub um_to_default_unit: f64,
        pub has_servo: bool,
        pub has_reference: bool,
        pub has_velocity: bool,
        pub has_acceleration: bool,
        pub has_halt: bool,
        pub has_moving_status_byte: bool,
    }

    impl PiGcsProbe {
        pub fn configured_fixture() -> Self {
            Self {
                controller_id: "PI GCS configured model".into(),
                syntax_version: Some(2.0),
                x_axis: PiGcsAxis {
                    name: "X".into(),
                    travel_um: 100_000.0,
                    referenced: true,
                    servo: true,
                },
                y_axis: PiGcsAxis {
                    name: "Y".into(),
                    travel_um: 75_000.0,
                    referenced: true,
                    servo: true,
                },
                z_axis: PiGcsAxis {
                    name: "Z".into(),
                    travel_um: 20_000.0,
                    referenced: true,
                    servo: true,
                },
                um_to_default_unit: 0.001,
                has_servo: true,
                has_reference: true,
                has_velocity: true,
                has_acceleration: true,
                has_halt: true,
                has_moving_status_byte: true,
            }
        }

        pub fn default_units(&self, um: f64) -> f64 {
            um * self.um_to_default_unit
        }

        pub fn micrometers(&self, default_units: f64) -> f64 {
            default_units / self.um_to_default_unit
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct PiGcsProbeResult {
        pub probe: PiGcsProbe,
        pub axes: Vec<String>,
        pub x_um: f64,
        pub y_um: f64,
        pub z_um: f64,
        pub speed_x_um_s: Option<f64>,
        pub speed_y_um_s: Option<f64>,
        pub speed_z_um_s: Option<f64>,
        pub acceleration_x_um_s2: Option<f64>,
        pub acceleration_y_um_s2: Option<f64>,
        pub acceleration_z_um_s2: Option<f64>,
        pub on_target: Option<bool>,
        pub moving_status_busy: Option<bool>,
        pub replies: Vec<(String, String)>,
    }

    impl PiGcsProbeResult {
        pub fn from_replies(
            template: &PiGcsProbe,
            replies: &[(impl AsRef<str>, impl AsRef<str>)],
        ) -> Result<Self> {
            let mut probe = template.clone();
            let mut axes = Vec::new();
            let mut x_um = 0.0;
            let mut y_um = 0.0;
            let mut z_um = 0.0;
            let mut speed_x_um_s = None;
            let mut speed_y_um_s = None;
            let mut speed_z_um_s = None;
            let mut acceleration_x_um_s2 = None;
            let mut acceleration_y_um_s2 = None;
            let mut acceleration_z_um_s2 = None;
            let mut on_target = None;
            let mut moving_status_busy = None;
            let mut stored = Vec::new();

            for (command, reply) in replies {
                let command = command.as_ref();
                let reply = reply.as_ref().trim();
                stored.push((command.to_string(), reply.to_string()));
                if command == "*IDN?" {
                    probe.controller_id = reply.to_string();
                } else if command == "CSV?" {
                    probe.syntax_version = Some(parse_f64_reply("CSV?", reply)?);
                } else if command == "SAI?" {
                    axes = reply.split_whitespace().map(str::to_string).collect();
                    if axes.len() >= 3 {
                        probe.x_axis.name = axes[0].clone();
                        probe.y_axis.name = axes[1].clone();
                        probe.z_axis.name = axes[2].clone();
                    }
                } else if command == format!("SVO? {}", probe.x_axis.name) {
                    probe.x_axis.servo = parse_boolish_reply(reply)?;
                } else if command == format!("SVO? {}", probe.y_axis.name) {
                    probe.y_axis.servo = parse_boolish_reply(reply)?;
                } else if command == format!("SVO? {}", probe.z_axis.name) {
                    probe.z_axis.servo = parse_boolish_reply(reply)?;
                } else if command == format!("POS? {}", probe.x_axis.name) {
                    x_um = probe.micrometers(parse_axis_value(reply)?);
                } else if command == format!("POS? {}", probe.y_axis.name) {
                    y_um = probe.micrometers(parse_axis_value(reply)?);
                } else if command == format!("POS? {}", probe.z_axis.name) {
                    z_um = probe.micrometers(parse_axis_value(reply)?);
                } else if command == format!("VEL? {}", probe.x_axis.name) {
                    speed_x_um_s = Some(probe.micrometers(parse_axis_value(reply)?));
                } else if command == format!("VEL? {}", probe.y_axis.name) {
                    speed_y_um_s = Some(probe.micrometers(parse_axis_value(reply)?));
                } else if command == format!("VEL? {}", probe.z_axis.name) {
                    speed_z_um_s = Some(probe.micrometers(parse_axis_value(reply)?));
                } else if command == format!("ACC? {}", probe.x_axis.name) {
                    acceleration_x_um_s2 = Some(probe.micrometers(parse_axis_value(reply)?));
                } else if command == format!("ACC? {}", probe.y_axis.name) {
                    acceleration_y_um_s2 = Some(probe.micrometers(parse_axis_value(reply)?));
                } else if command == format!("ACC? {}", probe.z_axis.name) {
                    acceleration_z_um_s2 = Some(probe.micrometers(parse_axis_value(reply)?));
                } else if command.starts_with("ONT? ") {
                    on_target = Some(parse_all_boolish(reply)?);
                } else if command == "ERR?" {
                    parse_error(reply)?;
                } else if command == "MOVING_STATUS_BYTE" {
                    moving_status_busy = Some(moving_status_is_busy(reply)?);
                }
            }

            Ok(Self {
                probe,
                axes,
                x_um,
                y_um,
                z_um,
                speed_x_um_s,
                speed_y_um_s,
                speed_z_um_s,
                acceleration_x_um_s2,
                acceleration_y_um_s2,
                acceleration_z_um_s2,
                on_target,
                moving_status_busy,
                replies: stored,
            })
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum PiGcsCommand {
        Identify,
        QuerySyntaxVersion,
        QueryAxes,
        Servo { axis: String, enabled: bool },
        QueryServo { axis: String },
        Reference { axes: Vec<String> },
        MoveAbs { targets: Vec<(String, f64)> },
        MoveRel { deltas: Vec<(String, f64)> },
        QueryPosition { axes: Vec<String> },
        SetVelocity { axis: String, velocity: f64 },
        QueryVelocity { axis: String },
        SetAcceleration { axis: String, acceleration: f64 },
        QueryAcceleration { axis: String },
        Halt { axes: Vec<String> },
        StopAll,
        QueryError,
        QueryOnTarget { axes: Vec<String> },
        QueryMovingStatusByte,
    }

    pub fn encode(command: &PiGcsCommand) -> Vec<u8> {
        match command {
            PiGcsCommand::QueryMovingStatusByte => vec![MOVING_STATUS_BYTE],
            other => text(other).into_bytes(),
        }
    }

    pub fn text(command: &PiGcsCommand) -> String {
        match command {
            PiGcsCommand::Identify => "*IDN?".into(),
            PiGcsCommand::QuerySyntaxVersion => "CSV?".into(),
            PiGcsCommand::QueryAxes => "SAI?".into(),
            PiGcsCommand::Servo { axis, enabled } => {
                format!("SVO {axis} {}", if *enabled { 1 } else { 0 })
            }
            PiGcsCommand::QueryServo { axis } => format!("SVO? {axis}"),
            PiGcsCommand::Reference { axes } => format!("FRF {}", axes.join(" ")),
            PiGcsCommand::MoveAbs { targets } => format!("MOV {}", axis_values(targets)),
            PiGcsCommand::MoveRel { deltas } => format!("MVR {}", axis_values(deltas)),
            PiGcsCommand::QueryPosition { axes } => format!("POS? {}", axes.join(" ")),
            PiGcsCommand::SetVelocity { axis, velocity } => {
                format!("VEL {axis} {velocity:.6}")
            }
            PiGcsCommand::QueryVelocity { axis } => format!("VEL? {axis}"),
            PiGcsCommand::SetAcceleration { axis, acceleration } => {
                format!("ACC {axis} {acceleration:.6}")
            }
            PiGcsCommand::QueryAcceleration { axis } => format!("ACC? {axis}"),
            PiGcsCommand::Halt { axes } => format!("HLT {}", axes.join(" ")),
            PiGcsCommand::StopAll => "STP".into(),
            PiGcsCommand::QueryError => "ERR?".into(),
            PiGcsCommand::QueryOnTarget { axes } => format!("ONT? {}", axes.join(" ")),
            PiGcsCommand::QueryMovingStatusByte => {
                String::from_utf8_lossy(&[MOVING_STATUS_BYTE]).into()
            }
        }
    }

    pub fn axis_values(values: &[(String, f64)]) -> String {
        values
            .iter()
            .map(|(axis, value)| format!("{axis} {value:.6}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn parse_error(reply: &str) -> Result<()> {
        match reply.trim() {
            "0" | "" => Ok(()),
            other => Err(Error::new(
                ErrorCode::Transport,
                format!("PI GCS controller error: {other}"),
            )),
        }
    }

    pub fn parse_bool(reply: &str) -> Result<bool> {
        let token = reply
            .split_whitespace()
            .last()
            .ok_or_else(|| Error::new(ErrorCode::Transport, "empty PI GCS boolean reply"))?;
        match token {
            "0" => Ok(false),
            "1" => Ok(true),
            other => Err(Error::new(
                ErrorCode::Transport,
                format!("invalid PI GCS boolean value {other}"),
            )),
        }
    }

    pub fn parse_position_lines(reply: &str) -> Result<BTreeMap<String, f64>> {
        let mut positions = BTreeMap::new();
        for line in reply.lines().filter(|line| !line.trim().is_empty()) {
            let mut parts = line.split_whitespace();
            let axis = parts
                .next()
                .ok_or_else(|| Error::new(ErrorCode::Transport, "missing PI GCS axis"))?;
            let value = parts
                .next()
                .ok_or_else(|| Error::new(ErrorCode::Transport, "missing PI GCS value"))?
                .parse::<f64>()
                .map_err(|_| Error::new(ErrorCode::Transport, "invalid PI GCS numeric value"))?;
            positions.insert(axis.to_string(), value);
        }
        Ok(positions)
    }

    pub fn moving_status_is_busy(reply: &str) -> Result<bool> {
        let value = reply
            .trim()
            .parse::<i64>()
            .map_err(|_| Error::new(ErrorCode::Transport, "invalid PI GCS moving status"))?;
        Ok(value != 0)
    }

    pub fn probe_commands(probe: &PiGcsProbe) -> Vec<(String, PiGcsCommand, ProbeReply)> {
        let axes = vec![
            probe.x_axis.name.clone(),
            probe.y_axis.name.clone(),
            probe.z_axis.name.clone(),
        ];
        vec![
            ("*IDN?".into(), PiGcsCommand::Identify, ProbeReply::Line),
            (
                "CSV?".into(),
                PiGcsCommand::QuerySyntaxVersion,
                ProbeReply::Line,
            ),
            ("SAI?".into(), PiGcsCommand::QueryAxes, ProbeReply::Line),
            (
                format!("SVO? {}", probe.x_axis.name),
                PiGcsCommand::QueryServo {
                    axis: probe.x_axis.name.clone(),
                },
                ProbeReply::Line,
            ),
            (
                format!("SVO? {}", probe.y_axis.name),
                PiGcsCommand::QueryServo {
                    axis: probe.y_axis.name.clone(),
                },
                ProbeReply::Line,
            ),
            (
                format!("SVO? {}", probe.z_axis.name),
                PiGcsCommand::QueryServo {
                    axis: probe.z_axis.name.clone(),
                },
                ProbeReply::Line,
            ),
            (
                format!("POS? {}", probe.x_axis.name),
                PiGcsCommand::QueryPosition {
                    axes: vec![probe.x_axis.name.clone()],
                },
                ProbeReply::Line,
            ),
            (
                format!("POS? {}", probe.y_axis.name),
                PiGcsCommand::QueryPosition {
                    axes: vec![probe.y_axis.name.clone()],
                },
                ProbeReply::Line,
            ),
            (
                format!("POS? {}", probe.z_axis.name),
                PiGcsCommand::QueryPosition {
                    axes: vec![probe.z_axis.name.clone()],
                },
                ProbeReply::Line,
            ),
            (
                format!("VEL? {}", probe.x_axis.name),
                PiGcsCommand::QueryVelocity {
                    axis: probe.x_axis.name.clone(),
                },
                ProbeReply::Line,
            ),
            (
                format!("VEL? {}", probe.y_axis.name),
                PiGcsCommand::QueryVelocity {
                    axis: probe.y_axis.name.clone(),
                },
                ProbeReply::Line,
            ),
            (
                format!("VEL? {}", probe.z_axis.name),
                PiGcsCommand::QueryVelocity {
                    axis: probe.z_axis.name.clone(),
                },
                ProbeReply::Line,
            ),
            (
                format!("ACC? {}", probe.x_axis.name),
                PiGcsCommand::QueryAcceleration {
                    axis: probe.x_axis.name.clone(),
                },
                ProbeReply::Line,
            ),
            (
                format!("ACC? {}", probe.y_axis.name),
                PiGcsCommand::QueryAcceleration {
                    axis: probe.y_axis.name.clone(),
                },
                ProbeReply::Line,
            ),
            (
                format!("ACC? {}", probe.z_axis.name),
                PiGcsCommand::QueryAcceleration {
                    axis: probe.z_axis.name.clone(),
                },
                ProbeReply::Line,
            ),
            (
                format!("ONT? {}", axes.join(" ")),
                PiGcsCommand::QueryOnTarget { axes },
                ProbeReply::Line,
            ),
            ("ERR?".into(), PiGcsCommand::QueryError, ProbeReply::Line),
            (
                "MOVING_STATUS_BYTE".into(),
                PiGcsCommand::QueryMovingStatusByte,
                ProbeReply::Byte,
            ),
        ]
    }

    pub fn probe_script(probe: &PiGcsProbe) -> Vec<String> {
        probe_commands(probe)
            .into_iter()
            .map(|(label, command, reply)| match reply {
                ProbeReply::Line => text(&command),
                ProbeReply::Byte => label,
            })
            .collect()
    }

    pub fn execute_probe_script(
        serial: &mut dyn SerialIo,
        template: &PiGcsProbe,
        polls_per_command: usize,
    ) -> Result<PiGcsProbeResult> {
        let mut codec = SerialLineCodec::new(SEND_ENDING, RECV_ENDING);
        let mut replies = Vec::new();
        for (label, command, reply_kind) in probe_commands(template) {
            match command {
                PiGcsCommand::QueryMovingStatusByte => serial.write(&encode(&command))?,
                _ => serial.write(&codec.encode(&text(&command)))?,
            }
            let reply = match reply_kind {
                ProbeReply::Line => read_line(serial, &mut codec, polls_per_command)?,
                ProbeReply::Byte => read_byte(serial, polls_per_command)?,
            };
            replies.push((label, reply));
        }
        PiGcsProbeResult::from_replies(template, &replies)
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ProbeReply {
        Line,
        Byte,
    }

    fn read_line(
        serial: &mut dyn SerialIo,
        codec: &mut SerialLineCodec,
        polls_per_command: usize,
    ) -> Result<String> {
        for _ in 0..polls_per_command.max(1) {
            let bytes = serial.read_available()?;
            for line in codec.push(&bytes) {
                return Ok(line);
            }
        }
        Err(Error::new(
            ErrorCode::Transport,
            "timed out waiting for PI GCS probe line",
        ))
    }

    fn read_byte(serial: &mut dyn SerialIo, polls_per_command: usize) -> Result<String> {
        for _ in 0..polls_per_command.max(1) {
            if let Some(byte) = serial.read_available()?.first().copied() {
                return Ok(byte.to_string());
            }
        }
        Err(Error::new(
            ErrorCode::Transport,
            "timed out waiting for PI GCS moving status byte",
        ))
    }

    pub(crate) fn parse_f64_reply(command: &str, reply: &str) -> Result<f64> {
        parse_axis_value(reply).map_err(|error| {
            Error::new(
                error.code,
                format!("invalid PI GCS {command} number {reply}: {}", error.message),
            )
        })
    }

    pub(crate) fn parse_axis_value(reply: &str) -> Result<f64> {
        reply
            .split(|ch: char| {
                !(ch == '-'
                    || ch == '+'
                    || ch == '.'
                    || ch == 'e'
                    || ch == 'E'
                    || ch.is_ascii_digit())
            })
            .filter(|token| !token.is_empty() && *token != "+" && *token != "-")
            .next_back()
            .ok_or_else(|| Error::new(ErrorCode::Transport, "missing PI GCS numeric value"))?
            .parse::<f64>()
            .map_err(|error| Error::new(ErrorCode::Transport, error.to_string()))
    }

    pub(crate) fn parse_boolish_reply(reply: &str) -> Result<bool> {
        let value = parse_axis_value(reply)?;
        Ok(value != 0.0)
    }

    pub(crate) fn parse_all_boolish(reply: &str) -> Result<bool> {
        let mut found = false;
        for line in reply.lines().filter(|line| !line.trim().is_empty()) {
            found = true;
            if !parse_boolish_reply(line)? {
                return Ok(false);
            }
        }
        if found {
            Ok(true)
        } else {
            parse_boolish_reply(reply)
        }
    }
}

pub struct PiGcsDiscovery {
    next_id: DriverId,
    probes: Vec<PiGcsConfiguredProbe>,
}

impl PiGcsDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![PiGcsConfiguredProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| device.driver == "pi-gcs")
            .map(PiGcsConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for PiGcsDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = probe.label.clone();
                let driver = if probe.connect_real_transport {
                    Box::new(PiGcsDriver::serial(id, probe)?) as Box<dyn Driver>
                } else {
                    Box::new(PiGcsDriver::configured(id, probe)) as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct PiGcsConfiguredProbe {
    pub label: String,
    pub probe: protocol::PiGcsProbe,
    pub endpoint: Option<PiGcsSerialEndpoint>,
    pub connect_real_transport: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiGcsSerialEndpoint {
    pub port_name: String,
    pub baud_rate: u32,
    pub timeout_ms: u64,
}

impl PiGcsConfiguredProbe {
    pub fn fixture() -> Self {
        Self {
            label: "Configured PI GCS controller fixture".into(),
            probe: protocol::PiGcsProbe::configured_fixture(),
            endpoint: None,
            connect_real_transport: false,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = protocol::PiGcsProbe::configured_fixture();
        probe.controller_id =
            string_prop(device, "controller_id").unwrap_or_else(|| probe.controller_id.clone());
        probe.syntax_version = f64_prop(device, "syntax_version").or(probe.syntax_version);
        probe.x_axis = axis_from_config(device, "x", probe.x_axis);
        probe.y_axis = axis_from_config(device, "y", probe.y_axis);
        probe.z_axis = axis_from_config(device, "z", probe.z_axis);
        probe.um_to_default_unit =
            f64_prop(device, "um_to_default_unit").unwrap_or(probe.um_to_default_unit);
        probe.has_servo = bool_prop(device, "has_servo").unwrap_or(probe.has_servo);
        probe.has_reference = bool_prop(device, "has_reference").unwrap_or(probe.has_reference);
        probe.has_velocity = bool_prop(device, "has_velocity").unwrap_or(probe.has_velocity);
        probe.has_acceleration =
            bool_prop(device, "has_acceleration").unwrap_or(probe.has_acceleration);
        probe.has_halt = bool_prop(device, "has_halt").unwrap_or(probe.has_halt);
        probe.has_moving_status_byte =
            bool_prop(device, "has_moving_status_byte").unwrap_or(probe.has_moving_status_byte);

        let endpoint = string_prop(device, "serial_port").map(|port_name| PiGcsSerialEndpoint {
            port_name,
            baud_rate: u32_prop(device, "baud_rate").unwrap_or(115_200),
            timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(1),
        });

        Ok(Self {
            label: if device.label.is_empty() {
                "Configured PI GCS controller".into()
            } else {
                device.label.clone()
            },
            probe,
            endpoint,
            connect_real_transport: bool_prop(device, "connect").unwrap_or(false),
        })
    }
}

pub struct PiGcsDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    xy: DeviceId,
    z: DeviceId,
    probe: protocol::PiGcsProbe,
    x_um: f64,
    y_um: f64,
    z_um: f64,
    speed_x_um_s: f64,
    speed_y_um_s: f64,
    speed_z_um_s: f64,
    acceleration_x_um_s2: f64,
    acceleration_y_um_s2: f64,
    acceleration_z_um_s2: f64,
    busy: bool,
    last_error: String,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
}

impl PiGcsDriver {
    pub fn configured_fixture(id: DriverId) -> Self {
        Self::configured(id, PiGcsConfiguredProbe::fixture())
    }

    pub fn configured(id: DriverId, configured: PiGcsConfiguredProbe) -> Self {
        let serial = ScriptedSerial::new();
        Self::new_configured(id, configured, Box::new(serial), false)
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: PiGcsConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "PI GCS serial probe is missing serial_port metadata",
            )
        })?;
        let mut serial = numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(endpoint.port_name, endpoint.baud_rate)
                .timeout(Duration::from_millis(endpoint.timeout_ms)),
        )?;
        let probe_result = protocol::execute_probe_script(&mut serial, &configured.probe, 4)?;
        Ok(Self::new_configured(id, configured, Box::new(serial), true)
            .with_probe_result(probe_result))
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _configured: PiGcsConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "PI GCS real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(id: DriverId, probe: protocol::PiGcsProbe, serial: Box<dyn SerialIo>) -> Self {
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 1501)),
            hub: DeviceId(NodeId(id.0 * 1000 + 1510)),
            xy: DeviceId(NodeId(id.0 * 1000 + 1511)),
            z: DeviceId(NodeId(id.0 * 1000 + 1512)),
            probe,
            x_um: 0.0,
            y_um: 0.0,
            z_um: 0.0,
            speed_x_um_s: 5_000.0,
            speed_y_um_s: 5_000.0,
            speed_z_um_s: 1_000.0,
            acceleration_x_um_s2: 50_000.0,
            acceleration_y_um_s2: 50_000.0,
            acceleration_z_um_s2: 10_000.0,
            busy: false,
            last_error: "0".into(),
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            codec: SerialLineCodec::new(protocol::SEND_ENDING, protocol::RECV_ENDING),
            serial_port: None,
            baud_rate: 115_200,
            serial_timeout_ms: 1,
            connected: false,
        }
    }

    pub fn new_configured(
        id: DriverId,
        configured: PiGcsConfiguredProbe,
        serial: Box<dyn SerialIo>,
        connected: bool,
    ) -> Self {
        let mut driver = Self::new(id, configured.probe, serial);
        driver.serial_port = configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.port_name.clone());
        driver.baud_rate = configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.baud_rate)
            .unwrap_or(115_200);
        driver.serial_timeout_ms = configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.timeout_ms)
            .unwrap_or(1);
        driver.connected = connected;
        driver
    }

    #[cfg(feature = "os-serial")]
    fn with_probe_result(mut self, probe_result: protocol::PiGcsProbeResult) -> Self {
        self.probe = probe_result.probe;
        self.x_um = probe_result.x_um.clamp(0.0, self.probe.x_axis.travel_um);
        self.y_um = probe_result.y_um.clamp(0.0, self.probe.y_axis.travel_um);
        self.z_um = probe_result.z_um.clamp(0.0, self.probe.z_axis.travel_um);
        self.speed_x_um_s = probe_result.speed_x_um_s.unwrap_or(self.speed_x_um_s);
        self.speed_y_um_s = probe_result.speed_y_um_s.unwrap_or(self.speed_y_um_s);
        self.speed_z_um_s = probe_result.speed_z_um_s.unwrap_or(self.speed_z_um_s);
        self.acceleration_x_um_s2 = probe_result
            .acceleration_x_um_s2
            .unwrap_or(self.acceleration_x_um_s2);
        self.acceleration_y_um_s2 = probe_result
            .acceleration_y_um_s2
            .unwrap_or(self.acceleration_y_um_s2);
        self.acceleration_z_um_s2 = probe_result
            .acceleration_z_um_s2
            .unwrap_or(self.acceleration_z_um_s2);
        self.busy = probe_result
            .moving_status_busy
            .or_else(|| probe_result.on_target.map(|on_target| !on_target))
            .unwrap_or(self.busy);
        self
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn send(&mut self, command: protocol::PiGcsCommand) -> Result<()> {
        match command {
            protocol::PiGcsCommand::QueryMovingStatusByte => {
                self.serial.write(&protocol::encode(&command))
            }
            _ => {
                let line = protocol::text(&command);
                self.serial.write(&self.codec.encode(&line))
            }
        }
    }

    fn query_for_property(&self, device: DeviceId, key: &str) -> Option<protocol::PiGcsCommand> {
        match (device, key) {
            (device, "controller_id") if device == self.hub => {
                Some(protocol::PiGcsCommand::Identify)
            }
            (device, "syntax_version") if device == self.hub => {
                Some(protocol::PiGcsCommand::QuerySyntaxVersion)
            }
            (device, "last_error") | (device, "fault") if device == self.hub => {
                Some(protocol::PiGcsCommand::QueryError)
            }
            (device, "busy") | (device, "state_summary")
                if device == self.hub || device == self.xy || device == self.z =>
            {
                if self.probe.has_moving_status_byte {
                    Some(protocol::PiGcsCommand::QueryMovingStatusByte)
                } else {
                    Some(protocol::PiGcsCommand::QueryOnTarget {
                        axes: vec![self.axis_x(), self.axis_y(), self.axis_z()],
                    })
                }
            }
            (device, "x") | (device, "y") if device == self.xy => {
                Some(protocol::PiGcsCommand::QueryPosition {
                    axes: vec![self.axis_x(), self.axis_y()],
                })
            }
            (device, "z") if device == self.z => Some(protocol::PiGcsCommand::QueryPosition {
                axes: vec![self.axis_z()],
            }),
            (device, "speed_x") if device == self.xy => {
                Some(protocol::PiGcsCommand::QueryVelocity {
                    axis: self.axis_x(),
                })
            }
            (device, "speed_y") if device == self.xy => {
                Some(protocol::PiGcsCommand::QueryVelocity {
                    axis: self.axis_y(),
                })
            }
            (device, "speed") if device == self.z => Some(protocol::PiGcsCommand::QueryVelocity {
                axis: self.axis_z(),
            }),
            (device, "acceleration_x") if device == self.xy => {
                Some(protocol::PiGcsCommand::QueryAcceleration {
                    axis: self.axis_x(),
                })
            }
            (device, "acceleration_y") if device == self.xy => {
                Some(protocol::PiGcsCommand::QueryAcceleration {
                    axis: self.axis_y(),
                })
            }
            (device, "acceleration") if device == self.z => {
                Some(protocol::PiGcsCommand::QueryAcceleration {
                    axis: self.axis_z(),
                })
            }
            (device, "servo_x") if device == self.xy => Some(protocol::PiGcsCommand::QueryServo {
                axis: self.axis_x(),
            }),
            (device, "servo_y") if device == self.xy => Some(protocol::PiGcsCommand::QueryServo {
                axis: self.axis_y(),
            }),
            (device, "servo") if device == self.z => Some(protocol::PiGcsCommand::QueryServo {
                axis: self.axis_z(),
            }),
            _ => None,
        }
    }

    fn read_query_reply(
        &mut self,
        device: DeviceId,
        command: &protocol::PiGcsCommand,
    ) -> Result<()> {
        let bytes = self.serial.read_available()?;
        if bytes.is_empty() {
            return Ok(());
        }
        if matches!(command, protocol::PiGcsCommand::QueryMovingStatusByte) {
            if let Some(byte) = bytes.first() {
                self.apply_readback_reply(device, command, &byte.to_string())?;
            }
            return Ok(());
        }
        for line in self.codec.push(&bytes) {
            self.apply_readback_reply(device, command, &line)?;
        }
        Ok(())
    }

    fn apply_readback_reply(
        &mut self,
        device: DeviceId,
        command: &protocol::PiGcsCommand,
        reply: &str,
    ) -> Result<()> {
        match command {
            protocol::PiGcsCommand::Identify => {
                self.probe.controller_id = reply.trim().to_string();
                self.emit_property(
                    self.hub,
                    "controller_id",
                    Value::String(self.probe.controller_id.clone()),
                );
            }
            protocol::PiGcsCommand::QuerySyntaxVersion => {
                self.probe.syntax_version = Some(protocol::parse_f64_reply("CSV?", reply)?);
                self.emit_property(
                    self.hub,
                    "syntax_version",
                    Value::F64(self.probe.syntax_version.unwrap_or_default()),
                );
            }
            protocol::PiGcsCommand::QueryMovingStatusByte => {
                self.busy = protocol::moving_status_is_busy(reply)?;
                for target in [self.hub, self.xy, self.z] {
                    self.emit_property(target, "busy", Value::Bool(self.busy));
                }
                if device == self.hub {
                    self.emit_property(self.hub, "state_summary", self.state_summary());
                }
            }
            protocol::PiGcsCommand::QueryOnTarget { .. } => {
                self.busy = !protocol::parse_all_boolish(reply)?;
                for target in [self.hub, self.xy, self.z] {
                    self.emit_property(target, "busy", Value::Bool(self.busy));
                }
                if device == self.hub {
                    self.emit_property(self.hub, "state_summary", self.state_summary());
                }
            }
            protocol::PiGcsCommand::QueryPosition { axes } => {
                self.apply_axis_position_reply(axes, reply)?;
            }
            protocol::PiGcsCommand::QueryVelocity { axis } => {
                let value_um_s = self.probe.micrometers(protocol::parse_axis_value(reply)?);
                self.apply_axis_velocity(axis, value_um_s);
            }
            protocol::PiGcsCommand::QueryAcceleration { axis } => {
                let value_um_s2 = self.probe.micrometers(protocol::parse_axis_value(reply)?);
                self.apply_axis_acceleration(axis, value_um_s2);
            }
            protocol::PiGcsCommand::QueryServo { axis } => {
                let value = protocol::parse_boolish_reply(reply)?;
                if axis == &self.probe.x_axis.name {
                    self.probe.x_axis.servo = value;
                    self.emit_property(self.xy, "servo_x", Value::Bool(value));
                } else if axis == &self.probe.y_axis.name {
                    self.probe.y_axis.servo = value;
                    self.emit_property(self.xy, "servo_y", Value::Bool(value));
                } else if axis == &self.probe.z_axis.name {
                    self.probe.z_axis.servo = value;
                    self.emit_property(self.z, "servo", Value::Bool(value));
                }
            }
            protocol::PiGcsCommand::QueryError => {
                self.last_error = reply.trim().to_string();
                self.emit_property(
                    self.hub,
                    "last_error",
                    Value::String(self.last_error.clone()),
                );
                self.emit_property(self.hub, "fault", Value::Bool(self.error_is_fault()));
                self.emit_property(self.hub, "state_summary", self.state_summary());
            }
            _ => {}
        }
        Ok(())
    }

    fn error_is_fault(&self) -> bool {
        !matches!(self.last_error.trim(), "" | "0" | "0.0")
    }

    fn refresh_error_readback(&mut self, fail_on_fault: bool) -> Result<()> {
        let command = protocol::PiGcsCommand::QueryError;
        self.send(command.clone())?;
        self.read_query_reply(self.hub, &command)?;
        if fail_on_fault && self.error_is_fault() {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("PI GCS controller error: {}", self.last_error),
            ));
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
        self.refresh_error_readback(true)
    }

    fn refresh_z_motion_readback(&mut self) -> Result<()> {
        self.refresh_property_readback(self.z, "busy")?;
        self.refresh_property_readback(self.z, "z")?;
        self.refresh_error_readback(true)
    }

    fn refresh_targets_for(command: &str) -> Result<Vec<(u8, &'static str)>> {
        match command {
            "refresh_readbacks" => Ok(vec![
                (0, "controller_id"),
                (0, "syntax_version"),
                (0, "busy"),
                (0, "last_error"),
                (1, "x"),
                (2, "z"),
                (1, "speed_x"),
                (1, "speed_y"),
                (2, "speed"),
                (1, "acceleration_x"),
                (1, "acceleration_y"),
                (2, "acceleration"),
                (1, "servo_x"),
                (1, "servo_y"),
                (2, "servo"),
            ]),
            "refresh_identity" => Ok(vec![(0, "controller_id"), (0, "syntax_version")]),
            "refresh_status" => Ok(vec![(0, "busy"), (0, "last_error")]),
            "refresh_position" => Ok(vec![(1, "x"), (2, "z")]),
            "refresh_profiles" => Ok(vec![
                (1, "speed_x"),
                (1, "speed_y"),
                (2, "speed"),
                (1, "acceleration_x"),
                (1, "acceleration_y"),
                (2, "acceleration"),
            ]),
            "refresh_servo" => Ok(vec![(1, "servo_x"), (1, "servo_y"), (2, "servo")]),
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "PI GCS GenericCommand supports refresh_readbacks, refresh_identity, refresh_status, refresh_position, refresh_profiles, and refresh_servo; got {other}"
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
                "PI GCS GenericCommand does not take parameters",
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
                Value::String("PI GCS mapped query readback".into()),
            ),
        ])))
    }

    fn axis_x(&self) -> String {
        self.probe.x_axis.name.clone()
    }

    fn axis_y(&self) -> String {
        self.probe.y_axis.name.clone()
    }

    fn axis_z(&self) -> String {
        self.probe.z_axis.name.clone()
    }

    fn axis_value_from_reply(&self, axes: &[String], reply: &str) -> Result<Option<(String, f64)>> {
        if axes.len() == 1 {
            return Ok(Some((
                axes[0].clone(),
                self.probe.micrometers(protocol::parse_axis_value(reply)?),
            )));
        }
        let values = protocol::parse_position_lines(reply)?;
        for axis in axes {
            if let Some(value) = values.get(axis) {
                return Ok(Some((axis.clone(), self.probe.micrometers(*value))));
            }
        }
        Ok(None)
    }

    fn apply_axis_position_reply(&mut self, axes: &[String], reply: &str) -> Result<()> {
        let Some((axis, value_um)) = self.axis_value_from_reply(axes, reply)? else {
            return Ok(());
        };
        if axis == self.probe.x_axis.name {
            self.x_um = value_um;
            self.emit_property(self.xy, "x", position(self.x_um));
        } else if axis == self.probe.y_axis.name {
            self.y_um = value_um;
            self.emit_property(self.xy, "y", position(self.y_um));
        } else if axis == self.probe.z_axis.name {
            self.z_um = value_um;
            self.emit_property(self.z, "z", position(self.z_um));
        }
        Ok(())
    }

    fn apply_axis_velocity(&mut self, axis: &str, value_um_s: f64) {
        if axis == self.probe.x_axis.name {
            self.speed_x_um_s = value_um_s;
            self.emit_property(self.xy, "speed_x", velocity(self.speed_x_um_s));
        } else if axis == self.probe.y_axis.name {
            self.speed_y_um_s = value_um_s;
            self.emit_property(self.xy, "speed_y", velocity(self.speed_y_um_s));
        } else if axis == self.probe.z_axis.name {
            self.speed_z_um_s = value_um_s;
            self.emit_property(self.z, "speed", velocity(self.speed_z_um_s));
        }
    }

    fn apply_axis_acceleration(&mut self, axis: &str, value_um_s2: f64) {
        if axis == self.probe.x_axis.name {
            self.acceleration_x_um_s2 = value_um_s2;
            self.emit_property(
                self.xy,
                "acceleration_x",
                acceleration_value(self.acceleration_x_um_s2),
            );
        } else if axis == self.probe.y_axis.name {
            self.acceleration_y_um_s2 = value_um_s2;
            self.emit_property(
                self.xy,
                "acceleration_y",
                acceleration_value(self.acceleration_y_um_s2),
            );
        } else if axis == self.probe.z_axis.name {
            self.acceleration_z_um_s2 = value_um_s2;
            self.emit_property(
                self.z,
                "acceleration",
                acceleration_value(self.acceleration_z_um_s2),
            );
        }
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        vec![
            DeviceDescriptor {
                id: self.hub,
                driver: self.id,
                label: "pi-gcs-hub".into(),
                vendor: Some("Physik Instrumente".into()),
                model: Some(self.probe.controller_id.clone()),
                serial: None,
                kinds: vec![
                    "hub".into(),
                    "motion.controller".into(),
                    "serial.ascii".into(),
                ],
                properties: vec![
                    property(
                        "controller_id",
                        "Controller ID",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                    property(
                        "syntax_version",
                        "GCS syntax version",
                        ValueType::F64,
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
                metadata: BTreeMap::from([(
                    "completion".into(),
                    Value::String("hardware status byte, ONT?, and ERR? define completion".into()),
                )]),
            },
            DeviceDescriptor {
                id: self.xy,
                driver: self.id,
                label: "pi-gcs-xy-stage".into(),
                vendor: Some("Physik Instrumente".into()),
                model: Some("GCS XY".into()),
                serial: None,
                kinds: vec!["axis.xy".into(), "stage.xy".into()],
                properties: vec![
                    sequenceable_position_property(
                        "x",
                        "X position",
                        true,
                        self.probe.x_axis.travel_um,
                    ),
                    sequenceable_position_property(
                        "y",
                        "Y position",
                        true,
                        self.probe.y_axis.travel_um,
                    ),
                    velocity_property("speed_x", "X speed", true, 100_000.0),
                    velocity_property("speed_y", "Y speed", true, 100_000.0),
                    acceleration_property("acceleration_x", "X acceleration", true, 1_000_000.0),
                    acceleration_property("acceleration_y", "Y acceleration", true, 1_000_000.0),
                    sequenceable_property("servo_x", "X servo", ValueType::Bool, None, true, None),
                    sequenceable_property("servo_y", "Y servo", ValueType::Bool, None, true, None),
                    property(
                        "referenced_x",
                        "X referenced",
                        ValueType::Bool,
                        None,
                        false,
                        None,
                    ),
                    property(
                        "referenced_y",
                        "Y referenced",
                        ValueType::Bool,
                        None,
                        false,
                        None,
                    ),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                ],
                metadata: BTreeMap::from([
                    (
                        "axis_x".into(),
                        Value::String(self.probe.x_axis.name.clone()),
                    ),
                    (
                        "axis_y".into(),
                        Value::String(self.probe.y_axis.name.clone()),
                    ),
                    (
                        "referenced_x".into(),
                        Value::Bool(self.probe.x_axis.referenced),
                    ),
                    (
                        "referenced_y".into(),
                        Value::Bool(self.probe.y_axis.referenced),
                    ),
                    ("x_travel".into(), position(self.probe.x_axis.travel_um)),
                    ("y_travel".into(), position(self.probe.y_axis.travel_um)),
                    (
                        "legacy_travel_x_um".into(),
                        position(self.probe.x_axis.travel_um),
                    ),
                    (
                        "legacy_travel_y_um".into(),
                        position(self.probe.y_axis.travel_um),
                    ),
                    (
                        "default_unit_size".into(),
                        position(1.0 / self.probe.um_to_default_unit),
                    ),
                    (
                        "legacy_default_unit_size_um".into(),
                        position(1.0 / self.probe.um_to_default_unit),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.z,
                driver: self.id,
                label: "pi-gcs-z-stage".into(),
                vendor: Some("Physik Instrumente".into()),
                model: Some("GCS Z".into()),
                serial: None,
                kinds: vec!["axis.z".into(), "stage.z".into()],
                properties: vec![
                    sequenceable_position_property(
                        "z",
                        "Z position",
                        true,
                        self.probe.z_axis.travel_um,
                    ),
                    velocity_property("speed", "Z speed", true, 100_000.0),
                    acceleration_property("acceleration", "Z acceleration", true, 1_000_000.0),
                    sequenceable_property("servo", "Z servo", ValueType::Bool, None, true, None),
                    property(
                        "referenced",
                        "Z referenced",
                        ValueType::Bool,
                        None,
                        false,
                        None,
                    ),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                ],
                metadata: BTreeMap::from([
                    (
                        "axis_z".into(),
                        Value::String(self.probe.z_axis.name.clone()),
                    ),
                    (
                        "referenced_z".into(),
                        Value::Bool(self.probe.z_axis.referenced),
                    ),
                    ("z_travel".into(), position(self.probe.z_axis.travel_um)),
                    (
                        "legacy_travel_z_um".into(),
                        position(self.probe.z_axis.travel_um),
                    ),
                    (
                        "default_unit_size".into(),
                        position(1.0 / self.probe.um_to_default_unit),
                    ),
                    (
                        "legacy_default_unit_size_um".into(),
                        position(1.0 / self.probe.um_to_default_unit),
                    ),
                ]),
            },
        ]
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        match (device, key) {
            (device, "controller_id") if device == self.hub => {
                Ok(Value::String(self.probe.controller_id.clone()))
            }
            (device, "syntax_version") if device == self.hub => {
                Ok(Value::F64(self.probe.syntax_version.unwrap_or_default()))
            }
            (device, "busy") if device == self.hub || device == self.xy || device == self.z => {
                Ok(Value::Bool(self.busy))
            }
            (device, "last_error") if device == self.hub => {
                Ok(Value::String(self.last_error.clone()))
            }
            (device, "fault") if device == self.hub => Ok(Value::Bool(self.error_is_fault())),
            (device, "state_summary") if device == self.hub => Ok(self.state_summary()),
            (device, "x") if device == self.xy => Ok(position(self.x_um)),
            (device, "y") if device == self.xy => Ok(position(self.y_um)),
            (device, "speed_x") if device == self.xy => Ok(velocity(self.speed_x_um_s)),
            (device, "speed_y") if device == self.xy => Ok(velocity(self.speed_y_um_s)),
            (device, "acceleration_x") if device == self.xy => {
                Ok(acceleration_value(self.acceleration_x_um_s2))
            }
            (device, "acceleration_y") if device == self.xy => {
                Ok(acceleration_value(self.acceleration_y_um_s2))
            }
            (device, "servo_x") if device == self.xy => Ok(Value::Bool(self.probe.x_axis.servo)),
            (device, "servo_y") if device == self.xy => Ok(Value::Bool(self.probe.y_axis.servo)),
            (device, "referenced_x") if device == self.xy => {
                Ok(Value::Bool(self.probe.x_axis.referenced))
            }
            (device, "referenced_y") if device == self.xy => {
                Ok(Value::Bool(self.probe.y_axis.referenced))
            }
            (device, "z") if device == self.z => Ok(position(self.z_um)),
            (device, "speed") if device == self.z => Ok(velocity(self.speed_z_um_s)),
            (device, "acceleration") if device == self.z => {
                Ok(acceleration_value(self.acceleration_z_um_s2))
            }
            (device, "servo") if device == self.z => Ok(Value::Bool(self.probe.z_axis.servo)),
            (device, "referenced") if device == self.z => {
                Ok(Value::Bool(self.probe.z_axis.referenced))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown PI GCS property {key}"),
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
                    position_um(value)?.clamp(0.0, self.probe.x_axis.travel_um),
                    self.y_um,
                )?;
                Ok(position(self.x_um))
            }
            (device, "y", value) if device == self.xy => {
                self.move_xy(
                    self.x_um,
                    position_um(value)?.clamp(0.0, self.probe.y_axis.travel_um),
                )?;
                Ok(position(self.y_um))
            }
            (device, "z", value) if device == self.z => {
                self.move_z(position_um(value)?.clamp(0.0, self.probe.z_axis.travel_um))?;
                Ok(position(self.z_um))
            }
            (device, "speed_x", value) if device == self.xy => {
                let speed = velocity_um_s(value)?;
                self.set_velocity(self.axis_x(), speed)?;
                self.speed_x_um_s = speed;
                Ok(velocity(self.speed_x_um_s))
            }
            (device, "speed_y", value) if device == self.xy => {
                let speed = velocity_um_s(value)?;
                self.set_velocity(self.axis_y(), speed)?;
                self.speed_y_um_s = speed;
                Ok(velocity(self.speed_y_um_s))
            }
            (device, "speed", value) if device == self.z => {
                let speed = velocity_um_s(value)?;
                self.set_velocity(self.axis_z(), speed)?;
                self.speed_z_um_s = speed;
                Ok(velocity(self.speed_z_um_s))
            }
            (device, "acceleration_x", value) if device == self.xy => {
                let acceleration = acceleration_um_s2(value)?;
                self.set_acceleration(self.axis_x(), acceleration)?;
                self.acceleration_x_um_s2 = acceleration;
                Ok(acceleration_value(self.acceleration_x_um_s2))
            }
            (device, "acceleration_y", value) if device == self.xy => {
                let acceleration = acceleration_um_s2(value)?;
                self.set_acceleration(self.axis_y(), acceleration)?;
                self.acceleration_y_um_s2 = acceleration;
                Ok(acceleration_value(self.acceleration_y_um_s2))
            }
            (device, "acceleration", value) if device == self.z => {
                let acceleration = acceleration_um_s2(value)?;
                self.set_acceleration(self.axis_z(), acceleration)?;
                self.acceleration_z_um_s2 = acceleration;
                Ok(acceleration_value(self.acceleration_z_um_s2))
            }
            (device, "servo_x", Value::Bool(enabled)) if device == self.xy => {
                self.set_servo(self.axis_x(), *enabled)?;
                self.probe.x_axis.servo = *enabled;
                Ok(Value::Bool(*enabled))
            }
            (device, "servo_y", Value::Bool(enabled)) if device == self.xy => {
                self.set_servo(self.axis_y(), *enabled)?;
                self.probe.y_axis.servo = *enabled;
                Ok(Value::Bool(*enabled))
            }
            (device, "servo", Value::Bool(enabled)) if device == self.z => {
                self.set_servo(self.axis_z(), *enabled)?;
                self.probe.z_axis.servo = *enabled;
                Ok(Value::Bool(*enabled))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid PI GCS write {key}"),
            )),
        }
    }

    fn set_servo(&mut self, axis: String, enabled: bool) -> Result<()> {
        if !self.probe.has_servo {
            return Err(Error::new(ErrorCode::Unsupported, "PI GCS SVO unsupported"));
        }
        self.send(protocol::PiGcsCommand::Servo { axis, enabled })?;
        self.refresh_error_readback(true)
    }

    fn set_velocity(&mut self, axis: String, velocity_um_s: f64) -> Result<()> {
        if !self.probe.has_velocity {
            return Err(Error::new(ErrorCode::Unsupported, "PI GCS VEL unsupported"));
        }
        let velocity = self.probe.default_units(velocity_um_s);
        self.send(protocol::PiGcsCommand::SetVelocity { axis, velocity })?;
        self.refresh_error_readback(true)
    }

    fn set_acceleration(&mut self, axis: String, acceleration_um_s2: f64) -> Result<()> {
        if !self.probe.has_acceleration {
            return Err(Error::new(ErrorCode::Unsupported, "PI GCS ACC unsupported"));
        }
        let acceleration = self.probe.default_units(acceleration_um_s2);
        self.send(protocol::PiGcsCommand::SetAcceleration { axis, acceleration })?;
        self.refresh_error_readback(true)
    }

    fn move_xy(&mut self, x_um: f64, y_um: f64) -> Result<()> {
        self.send(protocol::PiGcsCommand::MoveAbs {
            targets: vec![
                (self.axis_x(), self.probe.default_units(x_um)),
                (self.axis_y(), self.probe.default_units(y_um)),
            ],
        })?;
        self.refresh_error_readback(true)?;
        self.finish_motion("pi gcs xy hardware moving status reached idle");
        self.x_um = x_um;
        self.y_um = y_um;
        self.refresh_xy_motion_readback()?;
        Ok(())
    }

    fn move_xy_relative(&mut self, dx_um: f64, dy_um: f64) -> Result<()> {
        let next_x = (self.x_um + dx_um).clamp(0.0, self.probe.x_axis.travel_um);
        let next_y = (self.y_um + dy_um).clamp(0.0, self.probe.y_axis.travel_um);
        self.send(protocol::PiGcsCommand::MoveRel {
            deltas: vec![
                (self.axis_x(), self.probe.default_units(next_x - self.x_um)),
                (self.axis_y(), self.probe.default_units(next_y - self.y_um)),
            ],
        })?;
        self.refresh_error_readback(true)?;
        self.finish_motion("pi gcs xy relative hardware moving status reached idle");
        self.x_um = next_x;
        self.y_um = next_y;
        self.refresh_xy_motion_readback()?;
        Ok(())
    }

    fn move_z(&mut self, z_um: f64) -> Result<()> {
        self.send(protocol::PiGcsCommand::MoveAbs {
            targets: vec![(self.axis_z(), self.probe.default_units(z_um))],
        })?;
        self.refresh_error_readback(true)?;
        self.finish_motion("pi gcs z hardware moving status reached idle");
        self.z_um = z_um;
        self.refresh_z_motion_readback()?;
        Ok(())
    }

    fn move_z_relative(&mut self, dz_um: f64) -> Result<()> {
        let next_z = (self.z_um + dz_um).clamp(0.0, self.probe.z_axis.travel_um);
        self.send(protocol::PiGcsCommand::MoveRel {
            deltas: vec![(self.axis_z(), self.probe.default_units(next_z - self.z_um))],
        })?;
        self.refresh_error_readback(true)?;
        self.finish_motion("pi gcs z relative hardware moving status reached idle");
        self.z_um = next_z;
        self.refresh_z_motion_readback()?;
        Ok(())
    }

    fn validate_stage_move(&self, device: DeviceId, request: &StageMoveRequest) -> Result<()> {
        if request.target.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "PI GCS StageMove target must contain at least one axis",
            ));
        }
        if request.profile.as_ref().is_some_and(|profile| {
            profile
                .velocity
                .is_some_and(|velocity| velocity.micrometers_per_second() <= 0.0)
        }) {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "PI GCS StageMove velocity must be positive",
            ));
        }
        if let Some(acceleration) = request
            .profile
            .as_ref()
            .and_then(|profile| profile.acceleration.as_ref())
        {
            if acceleration.micrometers_per_second_squared() <= 0.0 {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "PI GCS StageMove acceleration must be positive",
                ));
            }
            if !self.probe.has_acceleration {
                return Err(Error::new(ErrorCode::Unsupported, "PI GCS ACC unsupported"));
            }
        }
        for axis in request.target.keys() {
            match (device, axis) {
                (device, StageAxis::X | StageAxis::Y) if device == self.xy => {}
                (device, StageAxis::Z) if device == self.z => {}
                (device, StageAxis::Custom(name))
                    if device == self.xy
                        && (name == &self.probe.x_axis.name || name == &self.probe.y_axis.name) => {
                }
                (device, StageAxis::Custom(name))
                    if device == self.z && name == &self.probe.z_axis.name => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "PI GCS StageMove axis does not belong to the target device",
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
    ) -> Result<BTreeMap<String, Value>> {
        let mut changed = BTreeMap::new();
        let Some(profile) = &request.profile else {
            return Ok(changed);
        };
        let velocity_um_s = profile
            .velocity
            .as_ref()
            .map(|velocity| velocity.micrometers_per_second());
        let acceleration_um_s2 = profile
            .acceleration
            .as_ref()
            .map(|acceleration| acceleration.micrometers_per_second_squared());
        if device == self.xy {
            if request.target.contains_key(&StageAxis::X)
                || request
                    .target
                    .keys()
                    .any(|axis| matches!(axis, StageAxis::Custom(name) if name == &self.probe.x_axis.name))
            {
                if let Some(velocity_um_s) = velocity_um_s {
                    self.set_velocity(self.axis_x(), velocity_um_s)?;
                    self.speed_x_um_s = velocity_um_s;
                    changed.insert("speed_x".into(), velocity(self.speed_x_um_s));
                }
                if let Some(acceleration_um_s2) = acceleration_um_s2 {
                    self.set_acceleration(self.axis_x(), acceleration_um_s2)?;
                    self.acceleration_x_um_s2 = acceleration_um_s2;
                    changed.insert(
                        "acceleration_x".into(),
                        acceleration_value(self.acceleration_x_um_s2),
                    );
                }
            }
            if request.target.contains_key(&StageAxis::Y)
                || request
                    .target
                    .keys()
                    .any(|axis| matches!(axis, StageAxis::Custom(name) if name == &self.probe.y_axis.name))
            {
                if let Some(velocity_um_s) = velocity_um_s {
                    self.set_velocity(self.axis_y(), velocity_um_s)?;
                    self.speed_y_um_s = velocity_um_s;
                    changed.insert("speed_y".into(), velocity(self.speed_y_um_s));
                }
                if let Some(acceleration_um_s2) = acceleration_um_s2 {
                    self.set_acceleration(self.axis_y(), acceleration_um_s2)?;
                    self.acceleration_y_um_s2 = acceleration_um_s2;
                    changed.insert(
                        "acceleration_y".into(),
                        acceleration_value(self.acceleration_y_um_s2),
                    );
                }
            }
        } else if device == self.z {
            if let Some(velocity_um_s) = velocity_um_s {
                self.set_velocity(self.axis_z(), velocity_um_s)?;
                self.speed_z_um_s = velocity_um_s;
                changed.insert("speed".into(), velocity(self.speed_z_um_s));
            }
            if let Some(acceleration_um_s2) = acceleration_um_s2 {
                self.set_acceleration(self.axis_z(), acceleration_um_s2)?;
                self.acceleration_z_um_s2 = acceleration_um_s2;
                changed.insert(
                    "acceleration".into(),
                    acceleration_value(self.acceleration_z_um_s2),
                );
            }
        }
        Ok(changed)
    }

    fn stage_move(&mut self, device: DeviceId, request: StageMoveRequest) -> Result<Value> {
        self.validate_stage_move(device, &request)?;
        let mut result = self.apply_stage_move_profile(device, &request)?;
        if device == self.xy {
            let mut x = self.x_um;
            let mut y = self.y_um;
            for (axis, target) in &request.target {
                match axis {
                    StageAxis::X => x = target.micrometers(),
                    StageAxis::Y => y = target.micrometers(),
                    StageAxis::Custom(name) if name == &self.probe.x_axis.name => {
                        x = target.micrometers()
                    }
                    StageAxis::Custom(name) if name == &self.probe.y_axis.name => {
                        y = target.micrometers()
                    }
                    _ => {}
                }
            }
            if request.relative {
                self.move_xy_relative(x, y)?;
            } else {
                self.move_xy(
                    x.clamp(0.0, self.probe.x_axis.travel_um),
                    y.clamp(0.0, self.probe.y_axis.travel_um),
                )?;
            }
            self.emit_property(self.xy, "x", position(self.x_um));
            self.emit_property(self.xy, "y", position(self.y_um));
            result.insert(
                "mode".into(),
                Value::String(if request.relative {
                    "relative".into()
                } else {
                    "absolute".into()
                }),
            );
            result.insert("x".into(), position(self.x_um));
            result.insert("y".into(), position(self.y_um));
            Ok(Value::Map(result))
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
                self.move_z(z.clamp(0.0, self.probe.z_axis.travel_um))?;
            }
            self.emit_property(self.z, "z", position(self.z_um));
            result.insert(
                "mode".into(),
                Value::String(if request.relative {
                    "relative".into()
                } else {
                    "absolute".into()
                }),
            );
            result.insert("z".into(), position(self.z_um));
            Ok(Value::Map(result))
        } else {
            Err(Error::new(
                ErrorCode::InvalidCommand,
                "PI GCS StageMove target device must be XY or Z stage",
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
                    next_x = position_um(value)?.clamp(0.0, self.probe.x_axis.travel_um);
                    xy_changed = true;
                }
                (device, "y", value) if device == self.xy => {
                    next_y = position_um(value)?.clamp(0.0, self.probe.y_axis.travel_um);
                    xy_changed = true;
                }
                (device, "z", value) if device == self.z => {
                    next_z = position_um(value)?.clamp(0.0, self.probe.z_axis.travel_um);
                    z_changed = true;
                }
                _ => remaining.push(write),
            }
        }

        let mut changed = BTreeMap::new();
        for write in remaining {
            let value = self.write_property(write.device, &write.property, &write.value)?;
            self.emit_property(write.device, &write.property, value.clone());
            changed.insert(format!("{}:{}", (write.device.0).0, write.property), value);
        }
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
                (device, "x" | "y" | "servo_x" | "servo_y") if device == self.xy => {}
                (device, "z" | "servo") if device == self.z => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "PI GCS timing plans only support XY/Z position and servo sequences",
                    ))
                }
            }
            for value in &sequence.values {
                if matches!(sequence.property.as_str(), "servo_x" | "servo_y" | "servo") {
                    if !matches!(value, Value::Bool(_)) {
                        return Err(Error::new(
                            ErrorCode::InvalidProperty,
                            "PI GCS servo timing sequences require Bool values",
                        ));
                    }
                } else {
                    let _ = position_um(value)?;
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
        axis: &protocol::PiGcsAxis,
        position_um: f64,
        speed_um_s: f64,
        acceleration_um_s2: f64,
    ) -> Value {
        Value::Map(BTreeMap::from([
            ("axis".into(), Value::String(axis.name.clone())),
            ("position".into(), position(position_um)),
            ("target".into(), position(position_um)),
            ("travel".into(), position(axis.travel_um)),
            ("speed".into(), velocity(speed_um_s)),
            (
                "acceleration".into(),
                acceleration_value(acceleration_um_s2),
            ),
            ("servo".into(), Value::Bool(axis.servo)),
            ("referenced".into(), Value::Bool(axis.referenced)),
        ]))
    }

    fn state_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            (
                "controller_id".into(),
                Value::String(self.probe.controller_id.clone()),
            ),
            (
                "syntax_version".into(),
                Value::F64(self.probe.syntax_version.unwrap_or_default()),
            ),
            (
                "default_unit_size".into(),
                position(1.0 / self.probe.um_to_default_unit),
            ),
            ("busy".into(), Value::Bool(self.busy)),
            ("last_error".into(), Value::String(self.last_error.clone())),
            ("fault".into(), Value::Bool(self.error_is_fault())),
            ("has_servo".into(), Value::Bool(self.probe.has_servo)),
            (
                "has_reference".into(),
                Value::Bool(self.probe.has_reference),
            ),
            ("has_velocity".into(), Value::Bool(self.probe.has_velocity)),
            (
                "has_acceleration".into(),
                Value::Bool(self.probe.has_acceleration),
            ),
            ("has_halt".into(), Value::Bool(self.probe.has_halt)),
            (
                "has_moving_status_byte".into(),
                Value::Bool(self.probe.has_moving_status_byte),
            ),
            ("xy_device".into(), Value::I64((self.xy.0).0 as i64)),
            ("z_device".into(), Value::I64((self.z.0).0 as i64)),
            (
                "x".into(),
                self.axis_state_summary(
                    &self.probe.x_axis,
                    self.x_um,
                    self.speed_x_um_s,
                    self.acceleration_x_um_s2,
                ),
            ),
            (
                "y".into(),
                self.axis_state_summary(
                    &self.probe.y_axis,
                    self.y_um,
                    self.speed_y_um_s,
                    self.acceleration_y_um_s2,
                ),
            ),
            (
                "z".into(),
                self.axis_state_summary(
                    &self.probe.z_axis,
                    self.z_um,
                    self.speed_z_um_s,
                    self.acceleration_z_um_s2,
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
                "pi gcs timing start sequence".into()
            } else {
                "pi gcs timing stop sequence".into()
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
                "unknown PI GCS capability",
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
                "PI GCS StageMove expects a StageMoveRequest",
            )),
            (CapabilityKind::StageHome, CapabilityRequest::None) if device == self.xy => {
                if !self.probe.has_reference {
                    return Err(Error::new(ErrorCode::Unsupported, "PI GCS FRF unsupported"));
                }
                self.send(protocol::PiGcsCommand::Reference {
                    axes: vec![self.axis_x(), self.axis_y()],
                })?;
                self.refresh_error_readback(true)?;
                self.finish_motion("pi gcs xy reference move complete");
                self.x_um = 0.0;
                self.y_um = 0.0;
                self.probe.x_axis.referenced = true;
                self.probe.y_axis.referenced = true;
                self.emit_property(self.xy, "x", position(self.x_um));
                self.emit_property(self.xy, "y", position(self.y_um));
                self.emit_property(self.xy, "referenced_x", Value::Bool(true));
                self.emit_property(self.xy, "referenced_y", Value::Bool(true));
                self.refresh_xy_motion_readback()?;
                Ok(Value::String("xy referenced".into()))
            }
            (CapabilityKind::StageHome, CapabilityRequest::None) if device == self.z => {
                if !self.probe.has_reference {
                    return Err(Error::new(ErrorCode::Unsupported, "PI GCS FRF unsupported"));
                }
                self.send(protocol::PiGcsCommand::Reference {
                    axes: vec![self.axis_z()],
                })?;
                self.refresh_error_readback(true)?;
                self.finish_motion("pi gcs z reference move complete");
                self.z_um = 0.0;
                self.probe.z_axis.referenced = true;
                self.emit_property(self.z, "z", position(self.z_um));
                self.emit_property(self.z, "referenced", Value::Bool(true));
                self.refresh_z_motion_readback()?;
                Ok(Value::String("z referenced".into()))
            }
            (CapabilityKind::StageStop, CapabilityRequest::None) if device == self.xy => {
                self.stop_axes(vec![self.axis_x(), self.axis_y()])?;
                self.refresh_xy_motion_readback()?;
                Ok(Value::String("xy halted".into()))
            }
            (CapabilityKind::StageStop, CapabilityRequest::None) if device == self.z => {
                self.stop_axes(vec![self.axis_z()])?;
                self.refresh_z_motion_readback()?;
                Ok(Value::String("z halted".into()))
            }
            (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "PI GCS home/stop capabilities take no request",
            )),
            (CapabilityKind::GenericCommand, CapabilityRequest::GenericCommand(request))
                if device == self.hub =>
            {
                self.apply_generic_command(request)
            }
            (CapabilityKind::GenericCommand, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "PI GCS GenericCommand expects GenericCommandRequest",
            )),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported PI GCS capability",
            )),
        }
    }

    fn stop_axes(&mut self, axes: Vec<String>) -> Result<()> {
        if self.probe.has_halt {
            self.send(protocol::PiGcsCommand::Halt { axes })?;
        } else {
            self.send(protocol::PiGcsCommand::StopAll)?;
        }
        self.refresh_error_readback(true)?;
        self.busy = false;
        Ok(())
    }

    fn finish_motion(&mut self, message: &str) {
        self.busy = true;
        if self.probe.has_moving_status_byte {
            let _ = self.send(protocol::PiGcsCommand::QueryMovingStatusByte);
        } else {
            let _ = self.send(protocol::PiGcsCommand::QueryOnTarget {
                axes: vec![self.axis_x(), self.axis_y(), self.axis_z()],
            });
        }
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

impl Driver for PiGcsDriver {
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
            label: "pi-gcs-serial".into(),
            kind: "serial".into(),
            metadata: BTreeMap::from([
                ("send_terminator".into(), Value::String("LF".into())),
                ("recv_terminator".into(), Value::String("LF".into())),
                (
                    "completion".into(),
                    Value::String(
                        "moving status byte or ONT? plus ERR? report hardware state".into(),
                    ),
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
                        protocol::probe_script(&self.probe)
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
                        description: format!("pi gcs read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("pi gcs write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "pi gcs remultiplexed XY/Z state set".into(),
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
                            Error::new(ErrorCode::Unsupported, "unknown PI GCS capability")
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
                                "PI GCS StageMove expects a StageMoveRequest",
                            ));
                        }
                        (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "PI GCS home/stop capabilities take no request",
                            ));
                        }
                        (CapabilityKind::GenericCommand, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "PI GCS GenericCommand expects GenericCommandRequest",
                            ));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported PI GCS capability",
                            ));
                        }
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("pi gcs invoke {}", capability.0),
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
                        message: format!("pi gcs serial: {line}"),
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
                description: "pi gcs timing arm summary".into(),
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
                description: "pi gcs timing start sequence".into(),
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
                description: "pi gcs timing stop sequence".into(),
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
    let mut schema = position_property(key, display_name, writable, max_um);
    schema.sequenceable = true;
    schema
}

fn velocity_property(
    key: &str,
    display_name: &str,
    writable: bool,
    max_um_s: f64,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Velocity,
        Some("um/s"),
        writable,
        Some(Range {
            min: velocity(0.0),
            max: velocity(max_um_s),
        }),
    )
}

fn acceleration_property(
    key: &str,
    display_name: &str,
    writable: bool,
    max_um_s2: f64,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Acceleration,
        Some("um/s^2"),
        writable,
        Some(Range {
            min: acceleration_value(0.0),
            max: acceleration_value(max_um_s2),
        }),
    )
}

fn position(value_um: f64) -> Value {
    Value::Position(Position::from_micrometers(value_um))
}

fn velocity(value_um_s: f64) -> Value {
    Value::Velocity(Velocity::from_micrometers_per_second(value_um_s))
}

fn acceleration_value(value_um_s2: f64) -> Value {
    Value::Acceleration(Acceleration::from_micrometers_per_second_squared(
        value_um_s2,
    ))
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

fn velocity_um_s(value: &Value) -> Result<f64> {
    match value {
        Value::Velocity(velocity) => Ok(velocity.micrometers_per_second()),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            "expected typed velocity value",
        )),
    }
}

fn acceleration_um_s2(value: &Value) -> Result<f64> {
    match value {
        Value::Acceleration(acceleration) => Ok(acceleration.micrometers_per_second_squared()),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            "expected typed acceleration value",
        )),
    }
}

fn axis_from_config(
    device: &DeviceConfig,
    prefix: &str,
    mut axis: protocol::PiGcsAxis,
) -> protocol::PiGcsAxis {
    if let Some(name) = string_prop(device, &format!("{prefix}_axis")) {
        axis.name = name;
    }
    if let Some(travel) = position_config_um(device, &format!("{prefix}_travel"))
        .or_else(|| f64_prop(device, &format!("{prefix}_travel_um")))
    {
        axis.travel_um = travel;
    }
    if let Some(referenced) = bool_prop(device, &format!("{prefix}_referenced")) {
        axis.referenced = referenced;
    }
    if let Some(servo) = bool_prop(device, &format!("{prefix}_servo")) {
        axis.servo = servo;
    }
    axis
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

fn position_config_um(device: &DeviceConfig, key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::Position(position)) => Some(position.micrometers()),
        _ => None,
    }
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
