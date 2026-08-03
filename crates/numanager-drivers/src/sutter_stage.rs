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

    pub const BAUD: u32 = 9_600;
    pub const SEND_ENDING: LineEnding = LineEnding::Cr;
    pub const RECV_ENDING: LineEnding = LineEnding::Lf;

    #[derive(Debug, Clone, PartialEq)]
    pub struct SutterStageProbe {
        pub version: String,
        pub inventory: String,
        pub x_travel_um: f64,
        pub y_travel_um: f64,
        pub z_travel_um: f64,
        pub step_size_um: f64,
        pub x_axis: String,
        pub y_axis: String,
        pub z_axis: String,
    }

    impl SutterStageProbe {
        pub fn simulated() -> Self {
            Self {
                version: "SutterStage simulated controller".into(),
                inventory: "1 EMOT X X axis; 2 EMOT Y Y axis; 6 EMOT Z aux axis".into(),
                x_travel_um: 100_000.0,
                y_travel_um: 75_000.0,
                z_travel_um: 20_000.0,
                step_size_um: 0.1,
                x_axis: "X".into(),
                y_axis: "Y".into(),
                z_axis: "Z".into(),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct SutterStageProbeResult {
        pub probe: SutterStageProbe,
        pub busy: bool,
        pub x_um: f64,
        pub y_um: f64,
        pub z_um: f64,
        pub transmission_delay: Option<u8>,
        pub speed_um_per_s: Option<f64>,
        pub start_speed_um_per_s: Option<f64>,
        pub acceleration: Option<i64>,
        pub replies: Vec<(String, String)>,
    }

    impl SutterStageProbeResult {
        pub fn from_replies(
            template: &SutterStageProbe,
            replies: &[(impl AsRef<str>, impl AsRef<str>)],
        ) -> Result<Self> {
            let mut probe = template.clone();
            let mut busy = false;
            let mut x_steps = 0;
            let mut y_steps = 0;
            let mut z_steps = 0;
            let mut transmission_delay = None;
            let mut speed_um_per_s = None;
            let mut start_speed_um_per_s = None;
            let mut acceleration = None;
            let mut stored = Vec::new();

            for (command, reply) in replies {
                let command = command.as_ref();
                let reply = reply.as_ref().trim();
                stored.push((command.to_string(), reply.to_string()));
                if command == "VER" {
                    probe.version = parse_text_reply(reply);
                } else if command == "Rconfig" {
                    probe.inventory = parse_text_reply(reply);
                } else if command == "Remres" {
                    parse_ack(reply)?;
                } else if command == "TRXDEL" {
                    transmission_delay = Some(parse_u8_reply("TRXDEL", reply)?);
                } else if command.starts_with("STATUS ") {
                    busy |= parse_busy_byte(first_reply_byte("STATUS", reply)?)?;
                } else if command == "WHERE X Y" {
                    let positions = parse_i64_list("WHERE X Y", reply)?;
                    if positions.len() >= 2 {
                        x_steps = positions[0];
                        y_steps = positions[1];
                    } else {
                        return Err(Error::new(
                            ErrorCode::Transport,
                            format!("Sutter WHERE X Y reply needs two coordinates: {reply}"),
                        ));
                    }
                } else if command == format!("WHERE {}", template.z_axis) {
                    z_steps = parse_i64_reply("WHERE Z", reply)?;
                } else if command == "SPEED X Y" {
                    speed_um_per_s = Some(
                        parse_i64_list("SPEED X Y", reply)?
                            .first()
                            .copied()
                            .ok_or_else(|| {
                                Error::new(ErrorCode::Transport, "empty Sutter SPEED reply")
                            })? as f64
                            * probe.step_size_um,
                    );
                } else if command == "STSPEED X Y" {
                    start_speed_um_per_s = Some(
                        parse_i64_list("STSPEED X Y", reply)?
                            .first()
                            .copied()
                            .ok_or_else(|| {
                                Error::new(ErrorCode::Transport, "empty Sutter STSPEED reply")
                            })? as f64
                            * probe.step_size_um,
                    );
                } else if command == "ACCEL X Y" {
                    acceleration = Some(
                        parse_i64_list("ACCEL X Y", reply)?
                            .first()
                            .copied()
                            .ok_or_else(|| {
                                Error::new(ErrorCode::Transport, "empty Sutter ACCEL reply")
                            })?,
                    );
                }
            }

            Ok(Self {
                x_um: um(x_steps, probe.step_size_um),
                y_um: um(y_steps, probe.step_size_um),
                z_um: um(z_steps, probe.step_size_um),
                probe,
                busy,
                transmission_delay,
                speed_um_per_s,
                start_speed_um_per_s,
                acceleration,
                replies: stored,
            })
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum SutterCommand {
        Version,
        Inventory,
        ResetModules,
        QueryTransmissionDelay,
        SetTransmissionDelay(u8),
        Status {
            axis: String,
        },
        MoveXyAbs {
            x_steps: i64,
            y_steps: i64,
        },
        MoveXyRel {
            dx_steps: i64,
            dy_steps: i64,
        },
        WhereXy,
        SetXyOrigin,
        HomeXy,
        Halt,
        QuerySpeedXy,
        SetSpeedXy {
            speed_steps_per_s: i64,
        },
        SetSpeedAxis {
            axis: String,
            speed_steps_per_s: i64,
        },
        QueryStartSpeedXy,
        SetStartSpeedXy {
            speed_steps_per_s: i64,
        },
        QueryAccelerationXy,
        SetAccelerationXy {
            acceleration: i64,
        },
        MoveAxisAbs {
            axis: String,
            steps: i64,
        },
        WhereAxis {
            axis: String,
        },
        Autofocus {
            axis: String,
            parameter: i64,
        },
    }

    pub fn encode(command: &SutterCommand) -> String {
        match command {
            SutterCommand::Version => "VER".into(),
            SutterCommand::Inventory => "Rconfig".into(),
            SutterCommand::ResetModules => "Remres".into(),
            SutterCommand::QueryTransmissionDelay => "TRXDEL".into(),
            SutterCommand::SetTransmissionDelay(delay) => format!("TRXDEL {delay}"),
            SutterCommand::Status { axis } => format!("STATUS {axis}"),
            SutterCommand::MoveXyAbs { x_steps, y_steps } => {
                format!("MOVE X={x_steps} Y={y_steps}")
            }
            SutterCommand::MoveXyRel { dx_steps, dy_steps } => {
                format!("MOVREL X={dx_steps} Y={dy_steps}")
            }
            SutterCommand::WhereXy => "WHERE X Y".into(),
            SutterCommand::SetXyOrigin => "HERE X=0 Y=0".into(),
            SutterCommand::HomeXy => "HOME X Y".into(),
            SutterCommand::Halt => "HALT".into(),
            SutterCommand::QuerySpeedXy => "SPEED X Y".into(),
            SutterCommand::SetSpeedXy { speed_steps_per_s } => {
                format!("SPEED X={speed_steps_per_s} Y={speed_steps_per_s}")
            }
            SutterCommand::SetSpeedAxis {
                axis,
                speed_steps_per_s,
            } => {
                format!("SPEED {axis}={speed_steps_per_s}")
            }
            SutterCommand::QueryStartSpeedXy => "STSPEED X Y".into(),
            SutterCommand::SetStartSpeedXy { speed_steps_per_s } => {
                format!("STSPEED X={speed_steps_per_s} Y={speed_steps_per_s}")
            }
            SutterCommand::QueryAccelerationXy => "ACCEL X Y".into(),
            SutterCommand::SetAccelerationXy { acceleration } => {
                format!("ACCEL X={acceleration} Y={acceleration}")
            }
            SutterCommand::MoveAxisAbs { axis, steps } => format!("MOVE {axis}={steps}"),
            SutterCommand::WhereAxis { axis } => format!("WHERE {axis}"),
            SutterCommand::Autofocus { axis, parameter } => format!("AF {axis}={parameter}"),
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

    pub fn speed_steps_per_s(um_per_s: f64, step_size_um: f64) -> i64 {
        (um_per_s / step_size_um).round().clamp(85.0, 276_480.0) as i64
    }

    pub fn parse_ack(reply: &str) -> Result<()> {
        let reply = reply.trim();
        if reply.starts_with(":A") {
            Ok(())
        } else if reply.starts_with(":N") {
            Err(Error::new(
                ErrorCode::Transport,
                format!("Sutter controller rejected command: {reply}"),
            ))
        } else {
            Err(Error::new(
                ErrorCode::Transport,
                format!("unexpected Sutter reply: {reply}"),
            ))
        }
    }

    pub fn parse_busy_byte(byte: u8) -> Result<bool> {
        match byte {
            b'B' => Ok(true),
            b'N' | b'I' | b'0' => Ok(false),
            other => Err(Error::new(
                ErrorCode::Transport,
                format!("invalid Sutter status byte 0x{other:02x}"),
            )),
        }
    }

    pub fn probe_commands(probe: &SutterStageProbe) -> Vec<SutterCommand> {
        vec![
            SutterCommand::Version,
            SutterCommand::Inventory,
            SutterCommand::QueryTransmissionDelay,
            SutterCommand::Status {
                axis: probe.x_axis.clone(),
            },
            SutterCommand::Status {
                axis: probe.y_axis.clone(),
            },
            SutterCommand::Status {
                axis: probe.z_axis.clone(),
            },
            SutterCommand::WhereXy,
            SutterCommand::WhereAxis {
                axis: probe.z_axis.clone(),
            },
            SutterCommand::QuerySpeedXy,
            SutterCommand::QueryStartSpeedXy,
            SutterCommand::QueryAccelerationXy,
        ]
    }

    pub fn probe_script(probe: &SutterStageProbe) -> Vec<String> {
        probe_commands(probe).iter().map(encode).collect()
    }

    pub fn execute_probe_script(
        serial: &mut dyn SerialIo,
        template: &SutterStageProbe,
        polls_per_command: usize,
    ) -> Result<SutterStageProbeResult> {
        let mut codec = SerialLineCodec::new(SEND_ENDING, RECV_ENDING);
        let mut replies = Vec::new();
        for command in probe_commands(template) {
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
                    format!("timed out waiting for SutterStage probe reply to {encoded}"),
                )
            })?;
            replies.push((encoded, reply));
        }
        SutterStageProbeResult::from_replies(template, &replies)
    }

    pub(crate) fn parse_text_reply(reply: &str) -> String {
        reply.trim().trim_start_matches(":A").trim().to_string()
    }

    pub(crate) fn first_reply_byte(command: &str, reply: &str) -> Result<u8> {
        reply.trim().as_bytes().first().copied().ok_or_else(|| {
            Error::new(
                ErrorCode::Transport,
                format!("empty Sutter {command} reply"),
            )
        })
    }

    pub(crate) fn parse_u8_reply(command: &str, reply: &str) -> Result<u8> {
        let value = parse_i64_reply(command, reply)?;
        u8::try_from(value).map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("invalid Sutter {command} u8 {value}: {error}"),
            )
        })
    }

    pub(crate) fn parse_i64_reply(command: &str, reply: &str) -> Result<i64> {
        let values = parse_i64_list(command, reply)?;
        values.first().copied().ok_or_else(|| {
            Error::new(
                ErrorCode::Transport,
                format!("empty Sutter {command} reply"),
            )
        })
    }

    pub(crate) fn parse_i64_list(command: &str, reply: &str) -> Result<Vec<i64>> {
        let mut values = Vec::new();
        for token in reply
            .trim()
            .split(|byte: char| !(byte == '-' || byte == '+' || byte.is_ascii_digit()))
            .filter(|token| !token.is_empty() && *token != "+" && *token != "-")
        {
            values.push(token.parse::<i64>().map_err(|error| {
                Error::new(
                    ErrorCode::Transport,
                    format!("invalid Sutter {command} integer {token}: {error}"),
                )
            })?);
        }
        Ok(values)
    }
}

pub struct SutterStageDiscovery {
    next_id: DriverId,
    probes: Vec<SutterStageConfiguredProbe>,
}

impl SutterStageDiscovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![SutterStageConfiguredProbe::simulated()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| device.driver == "sutter-stage")
            .map(SutterStageConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for SutterStageDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = probe.label.clone();
                let driver = if probe.connect_real_transport {
                    Box::new(SutterStageDriver::serial(id, probe)?) as Box<dyn Driver>
                } else {
                    Box::new(SutterStageDriver::configured(id, probe)) as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct SutterStageConfiguredProbe {
    pub label: String,
    pub probe: protocol::SutterStageProbe,
    pub endpoint: Option<SutterStageSerialEndpoint>,
    pub connect_real_transport: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SutterStageSerialEndpoint {
    pub port_name: String,
    pub baud_rate: u32,
    pub timeout_ms: u64,
}

impl SutterStageConfiguredProbe {
    pub fn simulated() -> Self {
        Self {
            label: "Simulated SutterStage controller".into(),
            probe: protocol::SutterStageProbe::simulated(),
            endpoint: None,
            connect_real_transport: false,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = protocol::SutterStageProbe::simulated();
        probe.version = string_prop(device, "version").unwrap_or(probe.version);
        probe.inventory = string_prop(device, "inventory").unwrap_or(probe.inventory);
        probe.x_travel_um =
            position_config_um(device, "x_travel", "x_travel_um").unwrap_or(probe.x_travel_um);
        probe.y_travel_um =
            position_config_um(device, "y_travel", "y_travel_um").unwrap_or(probe.y_travel_um);
        probe.z_travel_um =
            position_config_um(device, "z_travel", "z_travel_um").unwrap_or(probe.z_travel_um);
        probe.step_size_um =
            position_config_um(device, "step_size", "step_size_um").unwrap_or(probe.step_size_um);
        probe.x_axis = string_prop(device, "x_axis").unwrap_or(probe.x_axis);
        probe.y_axis = string_prop(device, "y_axis").unwrap_or(probe.y_axis);
        probe.z_axis = string_prop(device, "z_axis").unwrap_or(probe.z_axis);

        let endpoint =
            string_prop(device, "serial_port").map(|port_name| SutterStageSerialEndpoint {
                port_name,
                baud_rate: u32_prop(device, "baud_rate").unwrap_or(protocol::BAUD),
                timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(1),
            });

        Ok(Self {
            label: if device.label.is_empty() {
                "Configured SutterStage controller".into()
            } else {
                device.label.clone()
            },
            probe,
            endpoint,
            connect_real_transport: bool_prop(device, "connect").unwrap_or(false),
        })
    }
}

pub struct SutterStageDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    xy: DeviceId,
    z: DeviceId,
    autofocus: DeviceId,
    probe: protocol::SutterStageProbe,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
    x_um: f64,
    y_um: f64,
    z_um: f64,
    autofocus_parameter: i64,
    autofocus_mode: AutofocusMode,
    autofocus_focus_score: f64,
    speed_um_per_s: f64,
    z_speed_um_per_s: f64,
    start_speed_um_per_s: f64,
    acceleration: i64,
    transmission_delay: u8,
    busy: bool,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
}

impl SutterStageDriver {
    pub fn simulated(id: DriverId) -> Self {
        Self::configured(id, SutterStageConfiguredProbe::simulated())
    }

    pub fn configured(id: DriverId, configured: SutterStageConfiguredProbe) -> Self {
        let serial = ScriptedSerial::new();
        Self::new_with_transport_metadata(
            id,
            configured.probe,
            configured.endpoint,
            false,
            Box::new(serial),
        )
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: SutterStageConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "SutterStage serial probe is missing serial_port metadata",
            )
        })?;
        let mut serial = numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(
                endpoint.port_name.clone(),
                endpoint.baud_rate,
            )
            .timeout(Duration::from_millis(endpoint.timeout_ms)),
        )?;
        let probe_result = protocol::execute_probe_script(&mut serial, &configured.probe, 4)?;
        Ok(Self::new_with_transport_metadata(
            id,
            configured.probe,
            Some(endpoint),
            true,
            Box::new(serial),
        )
        .with_probe_result(probe_result))
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _configured: SutterStageConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "SutterStage real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(id: DriverId, probe: protocol::SutterStageProbe, serial: Box<dyn SerialIo>) -> Self {
        Self::new_with_transport_metadata(id, probe, None, false, serial)
    }

    fn new_with_transport_metadata(
        id: DriverId,
        probe: protocol::SutterStageProbe,
        endpoint: Option<SutterStageSerialEndpoint>,
        connected: bool,
        serial: Box<dyn SerialIo>,
    ) -> Self {
        let serial_port = endpoint.as_ref().map(|endpoint| endpoint.port_name.clone());
        let baud_rate = endpoint
            .as_ref()
            .map(|endpoint| endpoint.baud_rate)
            .unwrap_or(protocol::BAUD);
        let serial_timeout_ms = endpoint
            .as_ref()
            .map(|endpoint| endpoint.timeout_ms)
            .unwrap_or(1);
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 1301)),
            hub: DeviceId(NodeId(id.0 * 1000 + 1310)),
            xy: DeviceId(NodeId(id.0 * 1000 + 1311)),
            z: DeviceId(NodeId(id.0 * 1000 + 1312)),
            autofocus: DeviceId(NodeId(id.0 * 1000 + 1313)),
            probe,
            serial_port,
            baud_rate,
            serial_timeout_ms,
            connected,
            x_um: 0.0,
            y_um: 0.0,
            z_um: 0.0,
            autofocus_parameter: 0,
            autofocus_mode: AutofocusMode::Stop,
            autofocus_focus_score: 0.0,
            speed_um_per_s: 2500.0,
            z_speed_um_per_s: 1000.0,
            start_speed_um_per_s: 500.0,
            acceleration: 75,
            transmission_delay: 10,
            busy: false,
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            codec: SerialLineCodec::new(protocol::SEND_ENDING, protocol::RECV_ENDING),
        }
    }

    #[cfg(feature = "os-serial")]
    fn with_probe_result(mut self, probe_result: protocol::SutterStageProbeResult) -> Self {
        self.probe = probe_result.probe;
        self.x_um = probe_result.x_um.clamp(0.0, self.probe.x_travel_um);
        self.y_um = probe_result.y_um.clamp(0.0, self.probe.y_travel_um);
        self.z_um = probe_result.z_um.clamp(0.0, self.probe.z_travel_um);
        self.speed_um_per_s = probe_result.speed_um_per_s.unwrap_or(self.speed_um_per_s);
        self.z_speed_um_per_s = self.speed_um_per_s;
        self.start_speed_um_per_s = probe_result
            .start_speed_um_per_s
            .unwrap_or(self.start_speed_um_per_s);
        self.acceleration = probe_result.acceleration.unwrap_or(self.acceleration);
        self.transmission_delay = probe_result
            .transmission_delay
            .unwrap_or(self.transmission_delay);
        self.busy = probe_result.busy;
        self
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn send(&mut self, command: protocol::SutterCommand) -> Result<()> {
        let line = protocol::encode(&command);
        self.serial.write(&self.codec.encode(&line))
    }

    fn queries_for_property(&self, device: DeviceId, key: &str) -> Vec<protocol::SutterCommand> {
        match (device, key) {
            (device, "version") if device == self.hub => vec![protocol::SutterCommand::Version],
            (device, "inventory") if device == self.hub => vec![protocol::SutterCommand::Inventory],
            (device, "transmission_delay") if device == self.hub => {
                vec![protocol::SutterCommand::QueryTransmissionDelay]
            }
            (device, "busy") if device == self.hub => vec![protocol::SutterCommand::Status {
                axis: self.probe.x_axis.clone(),
            }],
            (device, "busy") if device == self.xy => vec![
                protocol::SutterCommand::Status {
                    axis: self.probe.x_axis.clone(),
                },
                protocol::SutterCommand::Status {
                    axis: self.probe.y_axis.clone(),
                },
            ],
            (device, "busy") if device == self.z => vec![protocol::SutterCommand::Status {
                axis: self.probe.z_axis.clone(),
            }],
            (device, "state_summary") if device == self.hub => vec![
                protocol::SutterCommand::Status {
                    axis: self.probe.x_axis.clone(),
                },
                protocol::SutterCommand::WhereXy,
                protocol::SutterCommand::WhereAxis {
                    axis: self.probe.z_axis.clone(),
                },
            ],
            (device, "x") | (device, "y") if device == self.xy => {
                vec![protocol::SutterCommand::WhereXy]
            }
            (device, "speed") if device == self.xy => vec![protocol::SutterCommand::QuerySpeedXy],
            (device, "start_speed") if device == self.xy => {
                vec![protocol::SutterCommand::QueryStartSpeedXy]
            }
            (device, "acceleration") if device == self.xy => {
                vec![protocol::SutterCommand::QueryAccelerationXy]
            }
            (device, "z") if device == self.z => vec![protocol::SutterCommand::WhereAxis {
                axis: self.probe.z_axis.clone(),
            }],
            _ => Vec::new(),
        }
    }

    fn read_query_reply(
        &mut self,
        device: DeviceId,
        command: &protocol::SutterCommand,
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

    fn refresh_property_readback(&mut self, device: DeviceId, key: &str) -> Result<()> {
        for query in self.queries_for_property(device, key) {
            self.send(query.clone())?;
            self.read_query_reply(device, &query)?;
        }
        Ok(())
    }

    fn refresh_xy_motion_readback(&mut self) -> Result<()> {
        self.refresh_property_readback(self.xy, "busy")?;
        self.refresh_property_readback(self.xy, "x")?;
        Ok(())
    }

    fn refresh_z_motion_readback(&mut self) -> Result<()> {
        self.refresh_property_readback(self.z, "busy")?;
        self.refresh_property_readback(self.z, "z")
    }

    fn refresh_targets_for(command: &str) -> Result<Vec<(u8, &'static str)>> {
        match command {
            "refresh_readbacks" => Ok(vec![
                (0, "version"),
                (0, "inventory"),
                (0, "transmission_delay"),
                (0, "state_summary"),
                (1, "x"),
                (2, "z"),
                (1, "speed"),
                (1, "start_speed"),
                (1, "acceleration"),
            ]),
            "refresh_identity" => Ok(vec![(0, "version"), (0, "inventory")]),
            "refresh_status" => Ok(vec![(0, "state_summary")]),
            "refresh_position" => Ok(vec![(1, "x"), (2, "z")]),
            "refresh_profiles" => Ok(vec![
                (0, "transmission_delay"),
                (1, "speed"),
                (1, "start_speed"),
                (1, "acceleration"),
            ]),
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "SutterStage GenericCommand supports refresh_readbacks, refresh_identity, refresh_status, refresh_position, and refresh_profiles; got {other}"
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
                "SutterStage GenericCommand does not take parameters",
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
                Value::String("SutterStage mapped query readback".into()),
            ),
        ])))
    }

    fn apply_readback_reply(
        &mut self,
        device: DeviceId,
        command: &protocol::SutterCommand,
        reply: &str,
    ) -> Result<()> {
        match command {
            protocol::SutterCommand::Version => {
                self.probe.version = protocol::parse_text_reply(reply);
                self.emit_property(
                    self.hub,
                    "version",
                    Value::String(self.probe.version.clone()),
                );
            }
            protocol::SutterCommand::Inventory => {
                self.probe.inventory = protocol::parse_text_reply(reply);
                self.emit_property(
                    self.hub,
                    "inventory",
                    Value::String(self.probe.inventory.clone()),
                );
            }
            protocol::SutterCommand::QueryTransmissionDelay => {
                self.transmission_delay = protocol::parse_u8_reply("TRXDEL", reply)?;
                self.emit_property(
                    self.hub,
                    "transmission_delay",
                    transmission_delay(self.transmission_delay),
                );
            }
            protocol::SutterCommand::Status { .. } => {
                self.busy =
                    protocol::parse_busy_byte(protocol::first_reply_byte("STATUS", reply)?)?;
                for target in [self.hub, self.xy, self.z] {
                    self.emit_property(target, "busy", Value::Bool(self.busy));
                }
                if device == self.hub {
                    self.emit_property(self.hub, "state_summary", self.state_summary());
                }
            }
            protocol::SutterCommand::WhereXy => {
                let positions = protocol::parse_i64_list("WHERE X Y", reply)?;
                if positions.len() < 2 {
                    return Err(Error::new(
                        ErrorCode::Transport,
                        format!("Sutter WHERE X Y reply needs two coordinates: {reply}"),
                    ));
                }
                self.x_um = protocol::um(positions[0], self.probe.step_size_um);
                self.y_um = protocol::um(positions[1], self.probe.step_size_um);
                self.emit_property(self.xy, "x", position(self.x_um));
                self.emit_property(self.xy, "y", position(self.y_um));
            }
            protocol::SutterCommand::WhereAxis { axis } if axis == &self.probe.z_axis => {
                self.z_um = protocol::um(
                    protocol::parse_i64_reply("WHERE Z", reply)?,
                    self.probe.step_size_um,
                );
                self.emit_property(self.z, "z", position(self.z_um));
            }
            protocol::SutterCommand::QuerySpeedXy => {
                let steps = protocol::parse_i64_reply("SPEED X Y", reply)?;
                self.speed_um_per_s = protocol::um(steps, self.probe.step_size_um);
                self.emit_property(self.xy, "speed", velocity(self.speed_um_per_s));
            }
            protocol::SutterCommand::QueryStartSpeedXy => {
                let steps = protocol::parse_i64_reply("STSPEED X Y", reply)?;
                self.start_speed_um_per_s = protocol::um(steps, self.probe.step_size_um);
                self.emit_property(self.xy, "start_speed", velocity(self.start_speed_um_per_s));
            }
            protocol::SutterCommand::QueryAccelerationXy => {
                self.acceleration = protocol::parse_i64_reply("ACCEL X Y", reply)?;
                self.emit_property(
                    self.xy,
                    "acceleration",
                    controller_scalar(self.acceleration),
                );
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
                label: "sutter-stage-hub".into(),
                vendor: Some("Sutter Instrument".into()),
                model: Some("SutterStage/Ludl-compatible controller".into()),
                serial: None,
                kinds: vec![
                    "hub".into(),
                    "motion.controller".into(),
                    "serial.ascii".into(),
                ],
                properties: vec![
                    property("version", "Version", ValueType::String, None, false, None),
                    property(
                        "inventory",
                        "Inventory",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                    property(
                        "transmission_delay",
                        "Transmission delay",
                        ValueType::TimeInterval,
                        Some("controller_tick"),
                        true,
                        Some(Range {
                            min: transmission_delay(1),
                            max: transmission_delay(255),
                        }),
                    ),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
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
                        "inventory".into(),
                        Value::String(self.probe.inventory.clone()),
                    ),
                    ("baud_rate".into(), Value::I64(protocol::BAUD as i64)),
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
            },
            DeviceDescriptor {
                id: self.xy,
                driver: self.id,
                label: "sutter-xy-stage".into(),
                vendor: Some("Sutter Instrument".into()),
                model: Some("Sutter XY".into()),
                serial: None,
                kinds: vec!["axis.xy".into(), "stage.xy".into()],
                properties: vec![
                    sequenceable_position_property("x", "X position", true, self.probe.x_travel_um),
                    sequenceable_position_property("y", "Y position", true, self.probe.y_travel_um),
                    velocity_property("speed", "Speed", true, 27_648.0),
                    velocity_property("start_speed", "Start speed", true, 27_648.0),
                    property(
                        "acceleration",
                        "Acceleration",
                        ValueType::ControllerScalar,
                        Some("controller_step"),
                        true,
                        Some(Range {
                            min: controller_scalar(1),
                            max: controller_scalar(255),
                        }),
                    ),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                ],
                metadata: BTreeMap::from([
                    ("x_axis".into(), Value::String(self.probe.x_axis.clone())),
                    ("y_axis".into(), Value::String(self.probe.y_axis.clone())),
                    ("step_size".into(), position(self.probe.step_size_um)),
                    (
                        "legacy_step_size_um".into(),
                        position(self.probe.step_size_um),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.z,
                driver: self.id,
                label: "sutter-z-stage".into(),
                vendor: Some("Sutter Instrument".into()),
                model: Some("Sutter single axis".into()),
                serial: None,
                kinds: vec!["axis.z".into(), "stage.z".into()],
                properties: vec![
                    sequenceable_position_property("z", "Z position", true, self.probe.z_travel_um),
                    velocity_property("speed", "Speed", true, 27_648.0),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                    property(
                        "autofocus_parameter",
                        "Autofocus parameter",
                        ValueType::I64,
                        None,
                        true,
                        None,
                    ),
                ],
                metadata: BTreeMap::from([
                    ("axis".into(), Value::String(self.probe.z_axis.clone())),
                    ("step_size".into(), position(self.probe.step_size_um)),
                    (
                        "legacy_step_size_um".into(),
                        position(self.probe.step_size_um),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.autofocus,
                driver: self.id,
                label: "sutter-autofocus".into(),
                vendor: Some("Sutter Instrument".into()),
                model: Some("SutterStage autofocus command provider".into()),
                serial: None,
                kinds: vec!["autofocus".into(), "sutter.af".into()],
                properties: vec![
                    sequenceable_property("enabled", "Enabled", ValueType::Bool, None, true, None),
                    sequenceable_enum_property(
                        "mode",
                        "Mode",
                        true,
                        &["single_shot", "continuous", "hold", "stop"],
                    ),
                    property("status", "Status", ValueType::String, None, false, None),
                    property(
                        "focus_score",
                        "Focus score",
                        ValueType::F64,
                        None,
                        false,
                        None,
                    ),
                    property(
                        "parameter",
                        "Controller AF parameter",
                        ValueType::I64,
                        None,
                        true,
                        None,
                    ),
                ],
                metadata: BTreeMap::from([
                    ("depends_on".into(), Value::String("sutter-z-stage".into())),
                    (
                        "protocol".into(),
                        Value::String("Sutter AF <axis>=<parameter> command".into()),
                    ),
                    ("axis".into(), Value::String(self.probe.z_axis.clone())),
                ]),
            },
        ]
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        match (device, key) {
            (device, "version") if device == self.hub => {
                Ok(Value::String(self.probe.version.clone()))
            }
            (device, "inventory") if device == self.hub => {
                Ok(Value::String(self.probe.inventory.clone()))
            }
            (device, "transmission_delay") if device == self.hub => {
                Ok(transmission_delay(self.transmission_delay))
            }
            (device, "busy") if device == self.hub || device == self.xy || device == self.z => {
                Ok(Value::Bool(self.busy))
            }
            (device, "state_summary") if device == self.hub => Ok(self.state_summary()),
            (device, "x") if device == self.xy => Ok(position(self.x_um)),
            (device, "y") if device == self.xy => Ok(position(self.y_um)),
            (device, "speed") if device == self.xy => Ok(velocity(self.speed_um_per_s)),
            (device, "start_speed") if device == self.xy => Ok(velocity(self.start_speed_um_per_s)),
            (device, "acceleration") if device == self.xy => {
                Ok(controller_scalar(self.acceleration))
            }
            (device, "z") if device == self.z => Ok(position(self.z_um)),
            (device, "speed") if device == self.z => Ok(velocity(self.z_speed_um_per_s)),
            (device, "autofocus_parameter") if device == self.z => {
                Ok(Value::I64(self.autofocus_parameter))
            }
            (device, "enabled") if device == self.autofocus => {
                Ok(Value::Bool(self.autofocus_mode != AutofocusMode::Stop))
            }
            (device, "mode") if device == self.autofocus => {
                Ok(Value::String(autofocus_mode_name(self.autofocus_mode)))
            }
            (device, "status") if device == self.autofocus => {
                Ok(Value::String(autofocus_status(self.autofocus_mode).into()))
            }
            (device, "focus_score") if device == self.autofocus => {
                Ok(Value::F64(self.autofocus_focus_score))
            }
            (device, "parameter") if device == self.autofocus => {
                Ok(Value::I64(self.autofocus_parameter))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown SutterStage property {key}"),
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
        if device == self.hub && key == "transmission_delay" {
            let delay = transmission_delay_ticks(value)?;
            let canonical = transmission_delay(delay);
            self.validate_write(device, key, &canonical)?;
            self.send(protocol::SutterCommand::SetTransmissionDelay(delay))?;
            self.transmission_delay = delay;
            self.refresh_property_readback(device, key)?;
            return Ok(canonical);
        }
        if device == self.xy && key == "acceleration" {
            let accel = controller_scalar_i64(value)?.clamp(1, 255);
            let canonical = controller_scalar(accel);
            self.validate_write(device, key, &canonical)?;
            self.send(protocol::SutterCommand::SetAccelerationXy {
                acceleration: accel,
            })?;
            self.acceleration = accel;
            self.refresh_property_readback(device, key)?;
            return Ok(canonical);
        }

        self.validate_write(device, key, value)?;
        match (device, key, value) {
            (device, "x", value) if device == self.xy => {
                self.move_xy(
                    position_um(value)?.clamp(0.0, self.probe.x_travel_um),
                    self.y_um,
                )?;
                self.refresh_property_readback(device, key)?;
                Ok(position(self.x_um))
            }
            (device, "y", value) if device == self.xy => {
                self.move_xy(
                    self.x_um,
                    position_um(value)?.clamp(0.0, self.probe.y_travel_um),
                )?;
                self.refresh_property_readback(device, key)?;
                Ok(position(self.y_um))
            }
            (device, "speed", value) if device == self.xy => {
                let steps =
                    protocol::speed_steps_per_s(velocity_um_s(value)?, self.probe.step_size_um);
                self.send(protocol::SutterCommand::SetSpeedXy {
                    speed_steps_per_s: steps,
                })?;
                self.speed_um_per_s = protocol::um(steps, self.probe.step_size_um);
                self.refresh_property_readback(device, key)?;
                Ok(velocity(self.speed_um_per_s))
            }
            (device, "start_speed", value) if device == self.xy => {
                let steps =
                    protocol::speed_steps_per_s(velocity_um_s(value)?, self.probe.step_size_um);
                self.send(protocol::SutterCommand::SetStartSpeedXy {
                    speed_steps_per_s: steps,
                })?;
                self.start_speed_um_per_s = protocol::um(steps, self.probe.step_size_um);
                self.refresh_property_readback(device, key)?;
                Ok(velocity(self.start_speed_um_per_s))
            }
            (device, "z", value) if device == self.z => {
                self.move_z(position_um(value)?.clamp(0.0, self.probe.z_travel_um))?;
                self.refresh_property_readback(device, key)?;
                Ok(position(self.z_um))
            }
            (device, "speed", value) if device == self.z => {
                let steps =
                    protocol::speed_steps_per_s(velocity_um_s(value)?, self.probe.step_size_um);
                self.send(protocol::SutterCommand::SetSpeedAxis {
                    axis: self.probe.z_axis.clone(),
                    speed_steps_per_s: steps,
                })?;
                self.z_speed_um_per_s = protocol::um(steps, self.probe.step_size_um);
                Ok(velocity(self.z_speed_um_per_s))
            }
            (device, "autofocus_parameter", Value::I64(parameter)) if device == self.z => {
                self.set_autofocus_parameter(*parameter)
            }
            (device, "enabled", Value::Bool(enabled)) if device == self.autofocus => {
                let mode = if *enabled {
                    AutofocusMode::Hold
                } else {
                    AutofocusMode::Stop
                };
                self.apply_autofocus_mode(mode)?;
                Ok(Value::Bool(*enabled))
            }
            (device, "mode", Value::String(mode)) if device == self.autofocus => {
                let mode = parse_autofocus_mode(mode)?;
                self.apply_autofocus_mode(mode)?;
                Ok(Value::String(autofocus_mode_name(mode)))
            }
            (device, "parameter", Value::I64(parameter)) if device == self.autofocus => {
                self.set_autofocus_parameter(*parameter)
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid SutterStage write {key}"),
            )),
        }
    }

    fn set_autofocus_parameter(&mut self, parameter: i64) -> Result<Value> {
        self.autofocus_parameter = parameter;
        self.send(protocol::SutterCommand::Autofocus {
            axis: self.probe.z_axis.clone(),
            parameter,
        })?;
        Ok(Value::I64(parameter))
    }

    fn apply_autofocus_mode(&mut self, mode: AutofocusMode) -> Result<Value> {
        self.autofocus_mode = mode;
        let parameter = match mode {
            AutofocusMode::SingleShot => self.autofocus_parameter.max(1),
            AutofocusMode::Continuous | AutofocusMode::Hold => self.autofocus_parameter.max(1),
            AutofocusMode::Stop => 0,
        };
        self.send(protocol::SutterCommand::Autofocus {
            axis: self.probe.z_axis.clone(),
            parameter,
        })?;
        self.autofocus_parameter = parameter;
        self.autofocus_focus_score = if mode == AutofocusMode::Stop {
            0.0
        } else {
            1.0
        };
        self.emit_property(self.z, "autofocus_parameter", Value::I64(parameter));
        self.emit_property(self.autofocus, "parameter", Value::I64(parameter));
        self.emit_property(
            self.autofocus,
            "mode",
            Value::String(autofocus_mode_name(mode)),
        );
        self.emit_property(
            self.autofocus,
            "enabled",
            Value::Bool(mode != AutofocusMode::Stop),
        );
        self.emit_property(
            self.autofocus,
            "status",
            Value::String(autofocus_status(mode).into()),
        );
        self.emit_property(
            self.autofocus,
            "focus_score",
            Value::F64(self.autofocus_focus_score),
        );
        Ok(Value::Map(BTreeMap::from([
            ("mode".into(), Value::String(autofocus_mode_name(mode))),
            ("enabled".into(), Value::Bool(mode != AutofocusMode::Stop)),
            (
                "status".into(),
                Value::String(autofocus_status(mode).into()),
            ),
            ("focus_score".into(), Value::F64(self.autofocus_focus_score)),
            ("parameter".into(), Value::I64(parameter)),
        ])))
    }

    fn move_xy(&mut self, x_um: f64, y_um: f64) -> Result<()> {
        self.x_um = x_um;
        self.y_um = y_um;
        self.send(protocol::SutterCommand::MoveXyAbs {
            x_steps: protocol::steps(x_um, self.probe.step_size_um),
            y_steps: protocol::steps(y_um, self.probe.step_size_um),
        })?;
        self.finish_motion("sutter xy STATUS X/Y BUSY then idle");
        Ok(())
    }

    fn move_xy_relative(&mut self, dx_um: f64, dy_um: f64) -> Result<()> {
        let next_x = (self.x_um + dx_um).clamp(0.0, self.probe.x_travel_um);
        let next_y = (self.y_um + dy_um).clamp(0.0, self.probe.y_travel_um);
        self.send(protocol::SutterCommand::MoveXyRel {
            dx_steps: protocol::steps(next_x - self.x_um, self.probe.step_size_um),
            dy_steps: protocol::steps(next_y - self.y_um, self.probe.step_size_um),
        })?;
        self.x_um = next_x;
        self.y_um = next_y;
        self.finish_motion("sutter xy relative STATUS X/Y BUSY then idle");
        Ok(())
    }

    fn move_z(&mut self, z_um: f64) -> Result<()> {
        self.z_um = z_um;
        self.send(protocol::SutterCommand::MoveAxisAbs {
            axis: self.probe.z_axis.clone(),
            steps: protocol::steps(z_um, self.probe.step_size_um),
        })?;
        self.finish_motion("sutter z STATUS Z BUSY then idle");
        Ok(())
    }

    fn move_z_relative(&mut self, dz_um: f64) -> Result<()> {
        let next_z = (self.z_um + dz_um).clamp(0.0, self.probe.z_travel_um);
        self.move_z(next_z)
    }

    fn validate_stage_move(&self, device: DeviceId, request: &StageMoveRequest) -> Result<()> {
        if request.target.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "SutterStage StageMove target must contain at least one axis",
            ));
        }
        if request
            .profile
            .as_ref()
            .and_then(|profile| profile.acceleration.as_ref())
            .is_some()
        {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "SutterStage StageMove MotionProfile acceleration uses typed physical units; the documented ACCEL command is a native controller scalar and needs calibration evidence before conversion",
            ));
        }
        for axis in request.target.keys() {
            match (device, axis) {
                (device, StageAxis::X | StageAxis::Y) if device == self.xy => {}
                (device, StageAxis::Z) if device == self.z => {}
                (device, StageAxis::Custom(name))
                    if device == self.xy
                        && (name == &self.probe.x_axis || name == &self.probe.y_axis) => {}
                (device, StageAxis::Custom(name))
                    if device == self.z && name == &self.probe.z_axis => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "SutterStage StageMove axis does not belong to the target device",
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
        let Some(velocity) = profile.velocity.as_ref() else {
            return Ok(());
        };
        let steps =
            protocol::speed_steps_per_s(velocity.micrometers_per_second(), self.probe.step_size_um);
        if device == self.xy {
            self.send(protocol::SutterCommand::SetSpeedXy {
                speed_steps_per_s: steps,
            })?;
            self.speed_um_per_s = protocol::um(steps, self.probe.step_size_um);
        } else if device == self.z {
            self.send(protocol::SutterCommand::SetSpeedAxis {
                axis: self.probe.z_axis.clone(),
                speed_steps_per_s: steps,
            })?;
            self.z_speed_um_per_s = protocol::um(steps, self.probe.step_size_um);
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
                    StageAxis::Custom(name) if name == &self.probe.x_axis => {
                        x = target.micrometers()
                    }
                    StageAxis::Custom(name) if name == &self.probe.y_axis => {
                        y = target.micrometers()
                    }
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
                ("speed".into(), velocity(self.speed_um_per_s)),
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
                ("speed".into(), velocity(self.z_speed_um_per_s)),
            ])))
        } else {
            Err(Error::new(
                ErrorCode::InvalidCommand,
                "SutterStage StageMove target device must be XY or Z stage",
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
            .filter(|sequence| {
                sequence.device == self.xy
                    || sequence.device == self.z
                    || sequence.device == self.autofocus
            })
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequences(plan) {
            match (sequence.device, sequence.property.as_str()) {
                (device, "x" | "y") if device == self.xy => {}
                (device, "z") if device == self.z => {}
                (device, "enabled" | "mode") if device == self.autofocus => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "SutterStage timing sequences can only target x, y, z, autofocus enabled, or autofocus mode",
                    ))
                }
            }
            for value in &sequence.values {
                if sequence.device == self.autofocus {
                    self.validate_write(sequence.device, &sequence.property, value)?;
                    if sequence.property == "mode" {
                        if let Value::String(mode) = value {
                            let _ = parse_autofocus_mode(mode)?;
                        }
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
            (
                "autofocus_participant".into(),
                Value::Bool(plan.participants.contains(&self.autofocus)),
            ),
            ("x".into(), position(self.x_um)),
            ("y".into(), position(self.y_um)),
            ("z".into(), position(self.z_um)),
            (
                "autofocus_enabled".into(),
                Value::Bool(self.autofocus_mode != AutofocusMode::Stop),
            ),
            (
                "autofocus_mode".into(),
                Value::String(autofocus_mode_name(self.autofocus_mode)),
            ),
            (
                "sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
        ]))
    }

    fn state_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("hub".into(), Value::I64(self.hub.0 .0 as i64)),
            ("xy_device".into(), Value::I64(self.xy.0 .0 as i64)),
            ("z_device".into(), Value::I64(self.z.0 .0 as i64)),
            (
                "autofocus_device".into(),
                Value::I64(self.autofocus.0 .0 as i64),
            ),
            ("version".into(), Value::String(self.probe.version.clone())),
            (
                "inventory".into(),
                Value::String(self.probe.inventory.clone()),
            ),
            ("busy".into(), Value::Bool(self.busy)),
            (
                "transmission_delay".into(),
                transmission_delay(self.transmission_delay),
            ),
            ("step_size".into(), position(self.probe.step_size_um)),
            (
                "xy".into(),
                Value::Map(BTreeMap::from([
                    ("x".into(), position(self.x_um)),
                    ("y".into(), position(self.y_um)),
                    ("speed".into(), velocity(self.speed_um_per_s)),
                    ("start_speed".into(), velocity(self.start_speed_um_per_s)),
                    ("acceleration".into(), controller_scalar(self.acceleration)),
                    ("x_axis".into(), Value::String(self.probe.x_axis.clone())),
                    ("y_axis".into(), Value::String(self.probe.y_axis.clone())),
                    ("x_travel".into(), position(self.probe.x_travel_um)),
                    ("y_travel".into(), position(self.probe.y_travel_um)),
                ])),
            ),
            (
                "z".into(),
                Value::Map(BTreeMap::from([
                    ("position".into(), position(self.z_um)),
                    ("speed".into(), velocity(self.z_speed_um_per_s)),
                    ("axis".into(), Value::String(self.probe.z_axis.clone())),
                    ("travel".into(), position(self.probe.z_travel_um)),
                    (
                        "autofocus_parameter".into(),
                        Value::I64(self.autofocus_parameter),
                    ),
                ])),
            ),
            (
                "autofocus_state".into(),
                Value::Map(BTreeMap::from([
                    (
                        "enabled".into(),
                        Value::Bool(self.autofocus_mode != AutofocusMode::Stop),
                    ),
                    (
                        "mode".into(),
                        Value::String(autofocus_mode_name(self.autofocus_mode)),
                    ),
                    (
                        "status".into(),
                        Value::String(autofocus_status(self.autofocus_mode).into()),
                    ),
                    ("focus_score".into(), Value::F64(self.autofocus_focus_score)),
                    ("parameter".into(), Value::I64(self.autofocus_parameter)),
                    ("depends_on".into(), Value::I64(self.z.0 .0 as i64)),
                ])),
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
                "sutter timing start sequence".into()
            } else {
                "sutter timing stop sequence".into()
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
                "unknown SutterStage capability",
            ));
        };
        match (capability.kind, request) {
            (CapabilityKind::StageMove, CapabilityRequest::StageMove(request))
                if device == self.xy || device == self.z =>
            {
                self.stage_move(device, request)
            }
            (CapabilityKind::Autofocus, CapabilityRequest::Autofocus(request))
                if device == self.autofocus =>
            {
                self.apply_autofocus_mode(request.mode)
            }
            (CapabilityKind::Autofocus, _) if device == self.autofocus => Err(Error::new(
                ErrorCode::InvalidCommand,
                "SutterStage autofocus expects an AutofocusRequest",
            )),
            (CapabilityKind::StageMove, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "SutterStage StageMove expects a StageMoveRequest",
            )),
            (CapabilityKind::StageHome, CapabilityRequest::None) if device == self.xy => {
                self.send(protocol::SutterCommand::HomeXy)?;
                self.x_um = 0.0;
                self.y_um = 0.0;
                self.finish_motion("sutter xy HOME complete");
                self.emit_property(self.xy, "x", position(self.x_um));
                self.emit_property(self.xy, "y", position(self.y_um));
                self.refresh_xy_motion_readback()?;
                Ok(Value::String("xy homed".into()))
            }
            (CapabilityKind::StageStop, CapabilityRequest::None)
                if device == self.xy || device == self.z =>
            {
                self.send(protocol::SutterCommand::Halt)?;
                self.busy = false;
                if device == self.xy {
                    self.refresh_xy_motion_readback()?;
                } else {
                    self.refresh_z_motion_readback()?;
                }
                Ok(Value::String("halted".into()))
            }
            (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "SutterStage home/stop capabilities take no request",
            )),
            (CapabilityKind::GenericCommand, CapabilityRequest::GenericCommand(request))
                if device == self.hub =>
            {
                self.apply_generic_command(request)
            }
            (CapabilityKind::GenericCommand, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "SutterStage GenericCommand expects GenericCommandRequest",
            )),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported SutterStage capability",
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

impl Driver for SutterStageDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        self.descriptors_for()
    }

    fn graph(&self) -> DeviceGraph {
        let mut graph = DeviceGraph::default();
        for resource in self.resources() {
            let _ = graph.insert_node(GraphNode {
                id: resource.id.0,
                kind: NodeKind::Resource,
                label: resource.label,
            });
        }
        for device in self.descriptors_for() {
            let _ = graph.insert_node(GraphNode {
                id: device.id.0,
                kind: NodeKind::Device,
                label: device.label.clone(),
            });
            if device.id != self.hub {
                let _ = graph.insert_edge(GraphEdge {
                    from: self.hub.0,
                    to: device.id.0,
                    kind: EdgeKind::OffersDevice,
                });
            }
        }
        let _ = graph.insert_device_dependency(self.z.0, self.autofocus.0, Role::ZStage);
        graph
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "sutter-stage-serial".into(),
            kind: "serial".into(),
            metadata: BTreeMap::from([
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
                ("send_terminator".into(), Value::String("CR".into())),
                ("recv_terminator".into(), Value::String("LF".into())),
                (
                    "completion".into(),
                    Value::String("STATUS <axis> returns B while moving".into()),
                ),
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
        } else if device == self.autofocus {
            vec![capability(4, device, CapabilityKind::Autofocus)]
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
                        description: format!("sutter read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("sutter write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "sutter remultiplexed XY/Z state set".into(),
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
                            Error::new(ErrorCode::Unsupported, "unknown SutterStage capability")
                        })?;
                    match (&candidate.kind, request) {
                        (CapabilityKind::StageMove, CapabilityRequest::StageMove(request)) => {
                            self.validate_stage_move(*device, request)?;
                        }
                        (
                            CapabilityKind::StageHome | CapabilityKind::StageStop,
                            CapabilityRequest::None,
                        ) => {}
                        (CapabilityKind::Autofocus, CapabilityRequest::Autofocus(_)) => {}
                        (
                            CapabilityKind::GenericCommand,
                            CapabilityRequest::GenericCommand(request),
                        ) if *device == self.hub => {
                            self.validate_generic_command(request)?;
                        }
                        (CapabilityKind::StageMove, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "SutterStage StageMove expects a StageMoveRequest",
                            ));
                        }
                        (CapabilityKind::Autofocus, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "SutterStage autofocus expects an AutofocusRequest",
                            ));
                        }
                        (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "SutterStage home/stop capabilities take no request",
                            ));
                        }
                        (CapabilityKind::GenericCommand, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "SutterStage GenericCommand expects GenericCommandRequest",
                            ));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported SutterStage capability",
                            ));
                        }
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("sutter invoke {}", capability.0),
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
                            CapabilityRequest::Autofocus(request) => Value::Map(BTreeMap::from([
                                (
                                    "mode".into(),
                                    Value::String(autofocus_mode_name(request.mode)),
                                ),
                                ("has_range".into(), Value::Bool(request.range.is_some())),
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
                        description: "sutter timing arm summary".into(),
                        payload: self.timing_summary(plan, "arm"),
                    });
                }
                Command::Start(_) | Command::Stop(_) => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "SutterStage direct timing transitions are runtime-owned",
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
                    for query in self.queries_for_property(device, &key) {
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
                        message: format!("sutter serial: {line}"),
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
                description: "sutter timing arm summary".into(),
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
                description: "sutter timing start sequence".into(),
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
                description: "sutter timing stop sequence".into(),
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

fn autofocus_mode_name(mode: AutofocusMode) -> String {
    match mode {
        AutofocusMode::SingleShot => "single_shot",
        AutofocusMode::Continuous => "continuous",
        AutofocusMode::Hold => "hold",
        AutofocusMode::Stop => "stop",
    }
    .into()
}

fn parse_autofocus_mode(mode: &str) -> Result<AutofocusMode> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "single_shot" | "single-shot" | "single" => Ok(AutofocusMode::SingleShot),
        "continuous" => Ok(AutofocusMode::Continuous),
        "hold" => Ok(AutofocusMode::Hold),
        "stop" | "off" => Ok(AutofocusMode::Stop),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            "autofocus mode must be single_shot, continuous, hold, or stop",
        )),
    }
}

fn autofocus_status(mode: AutofocusMode) -> &'static str {
    match mode {
        AutofocusMode::SingleShot => "completed",
        AutofocusMode::Continuous | AutofocusMode::Hold => "locked",
        AutofocusMode::Stop => "idle",
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
        sequenceable: false,
        hardware_address: None,
    }
}

fn enum_property(key: &str, display_name: &str, writable: bool, values: &[&str]) -> PropertySchema {
    let mut schema = property(key, display_name, ValueType::String, None, writable, None);
    schema.enum_values = values
        .iter()
        .map(|value| EnumValue {
            value: Value::String((*value).into()),
            label: (*value).into(),
        })
        .collect();
    schema
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
    schema.sequenceable = true;
    schema
}

fn sequenceable_enum_property(
    key: &str,
    display_name: &str,
    writable: bool,
    values: &[&str],
) -> PropertySchema {
    let mut schema = enum_property(key, display_name, writable, values);
    schema.sequenceable = true;
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

fn position(value_um: f64) -> Value {
    Value::Position(Position::from_micrometers(value_um))
}

fn velocity(value_um_s: f64) -> Value {
    Value::Velocity(Velocity::from_micrometers_per_second(value_um_s))
}

fn transmission_delay(ticks: u8) -> Value {
    Value::TimeInterval(TimeInterval::from_controller_ticks(ticks as f64))
}

fn controller_scalar(value: i64) -> Value {
    Value::ControllerScalar(ControllerScalar::new(value))
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

fn transmission_delay_ticks(value: &Value) -> Result<u8> {
    let ticks = match value {
        Value::TimeInterval(interval) if interval.unit == TimeIntervalUnit::ControllerTicks => {
            interval.value
        }
        Value::I64(value) => *value as f64,
        Value::F64(value) => *value,
        _ => {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "expected Sutter transmission_delay as typed controller-tick interval",
            ));
        }
    };
    if !ticks.is_finite() {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "Sutter transmission_delay must be finite",
        ));
    }
    Ok(ticks.round().clamp(1.0, 255.0) as u8)
}

fn controller_scalar_i64(value: &Value) -> Result<i64> {
    match value {
        Value::ControllerScalar(value) => Ok(value.value()),
        Value::I64(value) => Ok(*value),
        Value::F64(value) if value.is_finite() => Ok(value.round() as i64),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            "expected typed controller scalar value",
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
