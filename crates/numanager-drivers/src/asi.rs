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

    pub const BAUD: u32 = 115_200;
    pub const SEND_ENDING: LineEnding = LineEnding::Cr;
    pub const RECV_ENDING: LineEnding = LineEnding::Cr;
    pub const SERIAL_UNITS_PER_UM: f64 = 10.0;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Axis {
        X,
        Y,
        Z,
    }

    impl Axis {
        pub fn name(self) -> &'static str {
            match self {
                Axis::X => "X",
                Axis::Y => "Y",
                Axis::Z => "Z",
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct AsiMs2000Probe {
        pub firmware_version: String,
        pub build_name: String,
        pub x_travel_um: f64,
        pub y_travel_um: f64,
        pub z_travel_um: f64,
    }

    impl AsiMs2000Probe {
        pub fn simulated() -> Self {
            Self {
                firmware_version: "ASI MS-2000 simulated".into(),
                build_name: "numanager-sim".into(),
                x_travel_um: 100_000.0,
                y_travel_um: 75_000.0,
                z_travel_um: 10_000.0,
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TigerModuleKind {
        XyStage,
        ZStage,
        TtlIo,
        RingBuffer,
        CrispAutofocus,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct TigerCard {
        pub address: u8,
        pub module: TigerModuleKind,
        pub axes: Vec<Axis>,
        pub label: String,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct AsiTigerProbe {
        pub firmware_version: String,
        pub build_name: String,
        pub cards: Vec<TigerCard>,
        pub x_travel_um: f64,
        pub y_travel_um: f64,
        pub z_travel_um: f64,
    }

    impl AsiTigerProbe {
        pub fn simulated() -> Self {
            Self {
                firmware_version: "ASI Tiger simulated".into(),
                build_name: "numanager-tiger-sim".into(),
                cards: vec![
                    TigerCard {
                        address: 1,
                        module: TigerModuleKind::XyStage,
                        axes: vec![Axis::X, Axis::Y],
                        label: "XY stage card".into(),
                    },
                    TigerCard {
                        address: 2,
                        module: TigerModuleKind::ZStage,
                        axes: vec![Axis::Z],
                        label: "Z focus card".into(),
                    },
                    TigerCard {
                        address: 3,
                        module: TigerModuleKind::TtlIo,
                        axes: Vec::new(),
                        label: "TTL IO card".into(),
                    },
                    TigerCard {
                        address: 4,
                        module: TigerModuleKind::RingBuffer,
                        axes: Vec::new(),
                        label: "Ring buffer card".into(),
                    },
                    TigerCard {
                        address: 5,
                        module: TigerModuleKind::CrispAutofocus,
                        axes: vec![Axis::Z],
                        label: "CRISP autofocus card".into(),
                    },
                ],
                x_travel_um: 120_000.0,
                y_travel_um: 80_000.0,
                z_travel_um: 12_000.0,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum AsiCommand {
        Version,
        BuildName,
        Status,
        Halt,
        Where { axes: Vec<Axis> },
        MoveXyAbs { x_um: f64, y_um: f64 },
        MoveZAbs { z_um: f64 },
        MoveXyRel { dx_um: f64, dy_um: f64 },
        MoveZRel { dz_um: f64 },
        Speed { axes: Vec<(Axis, f64)> },
        Accel { axes: Vec<(Axis, f64)> },
        Home { axes: Vec<Axis> },
        Here { axis: Axis, position_um: f64 },
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum TigerCommand {
        Card { address: u8, command: AsiCommand },
        TtlOut { address: u8, line: u8, high: bool },
        RingBufferMode { address: u8, mode: String },
        RingBufferStart { address: u8 },
        RingBufferStop { address: u8 },
        CrispQueryState { address: u8 },
        CrispSetState { address: u8, state: CrispState },
        CrispQueryFocusScore { address: u8 },
        CrispQueryOffset { address: u8 },
        CrispSetOffset { address: u8, offset_um: f64 },
        CrispQueryObjectiveNa { address: u8 },
        CrispSetObjectiveNa { address: u8, na: f64 },
        CrispQueryLockRange { address: u8 },
        CrispSetLockRange { address: u8, range_mm: f64 },
        CrispQueryInFocusRange { address: u8 },
        CrispSetInFocusRange { address: u8, range_um: f64 },
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CrispState {
        Idle,
        Ready,
        Locking,
        Locked,
        Error,
    }

    impl CrispState {
        pub fn label(self) -> &'static str {
            match self {
                CrispState::Idle => "Idle",
                CrispState::Ready => "Ready",
                CrispState::Locking => "Locking",
                CrispState::Locked => "Locked",
                CrispState::Error => "Error",
            }
        }

        pub fn code(self) -> char {
            match self {
                CrispState::Idle => 'I',
                CrispState::Ready => 'R',
                CrispState::Locking => 'K',
                CrispState::Locked => 'F',
                CrispState::Error => 'E',
            }
        }

        pub fn command_value(self) -> u8 {
            match self {
                CrispState::Idle => 79,
                CrispState::Ready => 85,
                CrispState::Locking | CrispState::Locked => 83,
                CrispState::Error => 79,
            }
        }

        pub fn from_label(value: &str) -> Option<Self> {
            match value {
                "Idle" | "I" | "idle" => Some(CrispState::Idle),
                "Ready" | "R" | "ready" => Some(CrispState::Ready),
                "Locking" | "K" | "locking" => Some(CrispState::Locking),
                "Locked" | "F" | "locked" => Some(CrispState::Locked),
                "Error" | "E" | "error" => Some(CrispState::Error),
                _ => None,
            }
        }
    }

    pub fn encode(command: &AsiCommand) -> String {
        match command {
            AsiCommand::Version => "V".into(),
            AsiCommand::BuildName => "BU".into(),
            AsiCommand::Status => "/".into(),
            AsiCommand::Halt => "HALT".into(),
            AsiCommand::Where { axes } => format!(
                "W {}",
                axes.iter()
                    .map(|axis| axis.name())
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            AsiCommand::MoveXyAbs { x_um, y_um } => {
                format!("M X={} Y={}", asi_units(*x_um), asi_units(*y_um))
            }
            AsiCommand::MoveZAbs { z_um } => format!("M Z={}", asi_units(*z_um)),
            AsiCommand::MoveXyRel { dx_um, dy_um } => {
                format!("R X={} Y={}", asi_units(*dx_um), asi_units(*dy_um))
            }
            AsiCommand::MoveZRel { dz_um } => format!("R Z={}", asi_units(*dz_um)),
            AsiCommand::Speed { axes } => format!(
                "S {}",
                axes.iter()
                    .map(|(axis, speed_um_s)| {
                        format!("{}={:.6}", axis.name(), speed_um_s / 1000.0)
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            AsiCommand::Accel { axes } => format!(
                "AC {}",
                axes.iter()
                    .map(|(axis, ramp_ms)| format!("{}={:.0}", axis.name(), ramp_ms))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            AsiCommand::Home { axes } => format!(
                "HOME {}",
                axes.iter()
                    .map(|axis| axis.name())
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            AsiCommand::Here { axis, position_um } => {
                format!("HERE {}={}", axis.name(), asi_units(*position_um))
            }
        }
    }

    pub fn encode_tiger(command: &TigerCommand) -> String {
        match command {
            TigerCommand::Card { address, command } => format!("{address} {}", encode(command)),
            TigerCommand::TtlOut {
                address,
                line,
                high,
            } => format!("{address} TTL X={line} Y={}", i64::from(*high)),
            TigerCommand::RingBufferMode { address, mode } => format!("{address} RBMODE {mode}"),
            TigerCommand::RingBufferStart { address } => format!("{address} RM X=1"),
            TigerCommand::RingBufferStop { address } => format!("{address} RM X=0"),
            TigerCommand::CrispQueryState { address } => format!("{address} LK X?"),
            TigerCommand::CrispSetState { address, state } => {
                format!("{address} LK F={}", state.command_value())
            }
            TigerCommand::CrispQueryFocusScore { address } => format!("{address} LK Y?"),
            TigerCommand::CrispQueryOffset { address } => format!("{address} LK Z?"),
            TigerCommand::CrispSetOffset { address, offset_um } => {
                format!("{address} LK Z={offset_um:.0}")
            }
            TigerCommand::CrispQueryObjectiveNa { address } => format!("{address} LR Y?"),
            TigerCommand::CrispSetObjectiveNa { address, na } => format!("{address} LR Y={na:.3}"),
            TigerCommand::CrispQueryLockRange { address } => format!("{address} LR Z?"),
            TigerCommand::CrispSetLockRange { address, range_mm } => {
                format!("{address} LR Z={range_mm:.3}")
            }
            TigerCommand::CrispQueryInFocusRange { address } => format!("{address} AL Z?"),
            TigerCommand::CrispSetInFocusRange { address, range_um } => {
                format!("{address} AL Z={:.3}", range_um / 1000.0)
            }
        }
    }

    pub fn asi_units(um: f64) -> String {
        format!("{:.6}", um * SERIAL_UNITS_PER_UM)
    }

    fn status_token(reply: &str) -> &str {
        let reply = reply.trim();
        if reply.starts_with(':') {
            reply
        } else {
            reply
                .split_whitespace()
                .find(|token| token.starts_with(':'))
                .unwrap_or(reply)
        }
    }

    pub fn check_ack(reply: &str) -> Result<()> {
        let status = status_token(reply);
        if status.starts_with(":N") {
            Err(Error::new(
                ErrorCode::Transport,
                format!("ASI rejected command: {reply}"),
            ))
        } else if status.starts_with(":A") {
            Ok(())
        } else {
            Err(Error::new(
                ErrorCode::Transport,
                format!("invalid ASI reply: {reply}"),
            ))
        }
    }

    pub fn is_busy(reply: &str) -> Result<bool> {
        let status = status_token(reply);
        if status.starts_with(":A") || status == "A" {
            Ok(false)
        } else if status.starts_with(":B") || status == "B" {
            Ok(true)
        } else if status.starts_with(":N") {
            Err(Error::new(
                ErrorCode::Transport,
                format!("ASI status error: {status}"),
            ))
        } else {
            Err(Error::new(
                ErrorCode::Transport,
                format!("invalid ASI status reply: {status}"),
            ))
        }
    }

    pub fn parse_axis_position(reply: &str, axis: Axis) -> Result<f64> {
        let value = parse_positions(reply)?
            .remove(axis.name())
            .ok_or_else(|| Error::new(ErrorCode::Transport, "ASI position reply lacks axis"))?;
        Ok(value)
    }

    pub fn parse_xy(reply: &str) -> Result<(f64, f64)> {
        let mut positions = parse_positions(reply)?;
        let x = positions
            .remove("X")
            .ok_or_else(|| Error::new(ErrorCode::Transport, "ASI position reply lacks X"))?;
        let y = positions
            .remove("Y")
            .ok_or_else(|| Error::new(ErrorCode::Transport, "ASI position reply lacks Y"))?;
        Ok((x, y))
    }

    pub fn parse_positions(reply: &str) -> Result<BTreeMap<&'static str, f64>> {
        let mut positions = BTreeMap::new();
        let mut unlabeled = Vec::new();
        for token in reply.trim().split_whitespace() {
            if token == ":A" || token == "A" {
                continue;
            }
            if let Some((key, value)) = token.split_once('=') {
                let axis = match key {
                    "X" => "X",
                    "Y" => "Y",
                    "Z" => "Z",
                    _ => continue,
                };
                let value = value
                    .parse::<f64>()
                    .map_err(|_| Error::new(ErrorCode::Transport, "invalid ASI position value"))?;
                positions.insert(axis, value / SERIAL_UNITS_PER_UM);
            } else if let Ok(value) = token.parse::<f64>() {
                unlabeled.push(value / SERIAL_UNITS_PER_UM);
            }
        }
        if positions.is_empty() {
            if unlabeled.len() >= 2 {
                positions.insert("X", unlabeled[0]);
                positions.insert("Y", unlabeled[1]);
            } else if let Some(value) = unlabeled.first() {
                positions.insert("Z", *value);
            }
        }
        if positions.is_empty() {
            Err(Error::new(
                ErrorCode::Transport,
                format!("cannot parse ASI positions from {reply}"),
            ))
        } else {
            Ok(positions)
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct AsiMs2000ProbeResult {
        pub probe: AsiMs2000Probe,
        pub busy: bool,
        pub x_um: f64,
        pub y_um: f64,
        pub z_um: f64,
        pub replies: Vec<(String, String)>,
    }

    impl AsiMs2000ProbeResult {
        pub fn from_replies(replies: &[(impl AsRef<str>, impl AsRef<str>)]) -> Result<Self> {
            let mut probe = AsiMs2000Probe::simulated();
            let mut busy = false;
            let mut x_um = 0.0;
            let mut y_um = 0.0;
            let mut z_um = 0.0;
            let mut stored = Vec::new();
            for (command, reply) in replies {
                let command = command.as_ref();
                let reply = reply.as_ref().trim();
                stored.push((command.to_string(), reply.to_string()));
                match command {
                    "V" => probe.firmware_version = reply.to_string(),
                    "BU" => probe.build_name = reply.to_string(),
                    "/" => busy = is_busy(reply)?,
                    "W X Y" => {
                        (x_um, y_um) = parse_xy(reply)?;
                    }
                    "W Z" => {
                        z_um = parse_axis_position(reply, Axis::Z)?;
                    }
                    _ => {}
                }
            }
            Ok(Self {
                probe,
                busy,
                x_um,
                y_um,
                z_um,
                replies: stored,
            })
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct AsiTigerProbeResult {
        pub probe: AsiTigerProbe,
        pub busy_by_card: BTreeMap<u8, bool>,
        pub x_um: f64,
        pub y_um: f64,
        pub z_um: f64,
        pub crisp_state: Option<CrispState>,
        pub crisp_focus_score: Option<f64>,
        pub replies: Vec<(String, String)>,
    }

    impl AsiTigerProbeResult {
        pub fn from_replies(
            template: &AsiTigerProbe,
            replies: &[(impl AsRef<str>, impl AsRef<str>)],
        ) -> Result<Self> {
            let mut probe = template.clone();
            let mut busy_by_card = BTreeMap::new();
            let mut x_um = 0.0;
            let mut y_um = 0.0;
            let mut z_um = 0.0;
            let mut crisp_state = None;
            let mut crisp_focus_score = None;
            let mut stored = Vec::new();
            for (command, reply) in replies {
                let command = command.as_ref();
                let reply = reply.as_ref().trim();
                stored.push((command.to_string(), reply.to_string()));
                match command {
                    "V" => probe.firmware_version = reply.to_string(),
                    "BU" => probe.build_name = reply.to_string(),
                    _ => {
                        if let Some((address, rest)) = parse_tiger_command_prefix(command) {
                            match rest {
                                "/" => {
                                    busy_by_card.insert(address, is_busy(reply)?);
                                }
                                "W X Y" => {
                                    (x_um, y_um) = parse_xy(reply)?;
                                }
                                "W Z" => {
                                    z_um = parse_axis_position(reply, Axis::Z)?;
                                }
                                "LK X?" => {
                                    crisp_state = CrispState::from_label(parse_last_value(reply))
                                        .or_else(|| {
                                            parse_last_value(reply).chars().next().and_then(|ch| {
                                                CrispState::from_label(&ch.to_string())
                                            })
                                        });
                                }
                                "LK Y?" => {
                                    crisp_focus_score =
                                        Some(parse_reply_number("Tiger LK Y?", reply)?);
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            Ok(Self {
                probe,
                busy_by_card,
                x_um,
                y_um,
                z_um,
                crisp_state,
                crisp_focus_score,
                replies: stored,
            })
        }
    }

    pub fn ms2000_probe_commands() -> Vec<AsiCommand> {
        vec![
            AsiCommand::Version,
            AsiCommand::BuildName,
            AsiCommand::Status,
            AsiCommand::Where {
                axes: vec![Axis::X, Axis::Y],
            },
            AsiCommand::Where {
                axes: vec![Axis::Z],
            },
        ]
    }

    pub fn ms2000_probe_script() -> Vec<String> {
        ms2000_probe_commands().iter().map(encode).collect()
    }

    pub fn execute_ms2000_probe_script(
        serial: &mut dyn SerialIo,
        polls_per_command: usize,
    ) -> Result<AsiMs2000ProbeResult> {
        let mut codec = SerialLineCodec::new(SEND_ENDING, RECV_ENDING);
        let mut replies = Vec::new();
        for command in ms2000_probe_commands() {
            let encoded = encode(&command);
            serial.write(&codec.encode(&encoded))?;
            replies.push((encoded, read_line(serial, &mut codec, polls_per_command)?));
        }
        AsiMs2000ProbeResult::from_replies(&replies)
    }

    pub fn tiger_probe_commands(probe: &AsiTigerProbe) -> Vec<TigerCommand> {
        let mut commands = vec![
            TigerCommand::Card {
                address: 0,
                command: AsiCommand::Version,
            },
            TigerCommand::Card {
                address: 0,
                command: AsiCommand::BuildName,
            },
        ];
        for card in &probe.cards {
            commands.push(TigerCommand::Card {
                address: card.address,
                command: AsiCommand::Status,
            });
            if card.axes.contains(&Axis::X) && card.axes.contains(&Axis::Y) {
                commands.push(TigerCommand::Card {
                    address: card.address,
                    command: AsiCommand::Where {
                        axes: vec![Axis::X, Axis::Y],
                    },
                });
            }
            if card.axes.contains(&Axis::Z) {
                commands.push(TigerCommand::Card {
                    address: card.address,
                    command: AsiCommand::Where {
                        axes: vec![Axis::Z],
                    },
                });
            }
            if card.module == TigerModuleKind::CrispAutofocus {
                commands.push(TigerCommand::CrispQueryState {
                    address: card.address,
                });
                commands.push(TigerCommand::CrispQueryFocusScore {
                    address: card.address,
                });
            }
        }
        commands
    }

    pub fn tiger_probe_script(probe: &AsiTigerProbe) -> Vec<String> {
        tiger_probe_commands(probe)
            .into_iter()
            .map(|command| match command {
                TigerCommand::Card {
                    address: 0,
                    command,
                } => encode(&command),
                other => encode_tiger(&other),
            })
            .collect()
    }

    pub fn execute_tiger_probe_script(
        serial: &mut dyn SerialIo,
        template: &AsiTigerProbe,
        polls_per_command: usize,
    ) -> Result<AsiTigerProbeResult> {
        let mut codec = SerialLineCodec::new(SEND_ENDING, RECV_ENDING);
        let mut replies = Vec::new();
        for command in tiger_probe_commands(template) {
            let encoded = match command {
                TigerCommand::Card {
                    address: 0,
                    command,
                } => encode(&command),
                other => encode_tiger(&other),
            };
            serial.write(&codec.encode(&encoded))?;
            replies.push((encoded, read_line(serial, &mut codec, polls_per_command)?));
        }
        AsiTigerProbeResult::from_replies(template, &replies)
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
            "timed out waiting for ASI probe reply",
        ))
    }

    fn parse_tiger_command_prefix(command: &str) -> Option<(u8, &str)> {
        let (address, rest) = command.split_once(' ')?;
        Some((address.parse().ok()?, rest))
    }

    pub(crate) fn parse_last_value(reply: &str) -> &str {
        reply
            .split(|ch: char| ch.is_whitespace() || ch == '=' || ch == ':')
            .filter(|token| !token.is_empty() && *token != "A")
            .next_back()
            .unwrap_or(reply.trim())
    }

    pub(crate) fn parse_reply_number(command: &str, reply: &str) -> Result<f64> {
        parse_last_value(reply).parse::<f64>().map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("invalid ASI {command} number {reply}: {error}"),
            )
        })
    }
}

pub struct AsiMs2000Discovery {
    next_id: DriverId,
    probes: Vec<AsiMs2000ConfiguredProbe>,
}

impl AsiMs2000Discovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![AsiMs2000ConfiguredProbe::simulated()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| device.driver == "asi-ms2000")
            .map(AsiMs2000ConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for AsiMs2000Discovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = probe.label.clone();
                let driver = if probe.connect_real_transport {
                    Box::new(AsiMs2000Driver::serial(id, probe)?) as Box<dyn Driver>
                } else {
                    Box::new(AsiMs2000Driver::configured(id, probe)) as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct AsiMs2000ConfiguredProbe {
    pub label: String,
    pub probe: protocol::AsiMs2000Probe,
    pub endpoint: Option<AsiSerialEndpoint>,
    pub connect_real_transport: bool,
}

#[derive(Debug, Clone)]
pub struct AsiTigerConfiguredProbe {
    pub label: String,
    pub probe: protocol::AsiTigerProbe,
    pub endpoint: Option<AsiSerialEndpoint>,
    pub connect_real_transport: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsiSerialEndpoint {
    pub port_name: String,
    pub baud_rate: u32,
    pub timeout_ms: u64,
}

impl AsiMs2000ConfiguredProbe {
    pub fn simulated() -> Self {
        Self {
            label: "Simulated ASI MS-2000 controller".into(),
            probe: protocol::AsiMs2000Probe::simulated(),
            endpoint: None,
            connect_real_transport: false,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = protocol::AsiMs2000Probe::simulated();
        probe.firmware_version = string_prop(device, "firmware_version")
            .unwrap_or_else(|| probe.firmware_version.clone());
        probe.build_name =
            string_prop(device, "build_name").unwrap_or_else(|| probe.build_name.clone());
        probe.x_travel_um =
            position_config_um(device, "x_travel", "x_travel_um").unwrap_or(probe.x_travel_um);
        probe.y_travel_um =
            position_config_um(device, "y_travel", "y_travel_um").unwrap_or(probe.y_travel_um);
        probe.z_travel_um =
            position_config_um(device, "z_travel", "z_travel_um").unwrap_or(probe.z_travel_um);

        Ok(Self {
            label: if device.label.is_empty() {
                "Configured ASI MS-2000 controller".into()
            } else {
                device.label.clone()
            },
            probe,
            endpoint: asi_endpoint_from_config(device),
            connect_real_transport: bool_prop(device, "connect").unwrap_or(false),
        })
    }
}

pub struct AsiMs2000Driver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    xy: DeviceId,
    z: DeviceId,
    probe: protocol::AsiMs2000Probe,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
    x_um: f64,
    y_um: f64,
    z_um: f64,
    xy_speed_um_s: f64,
    z_speed_um_s: f64,
    xy_accel_ms: f64,
    z_accel_ms: f64,
    busy: bool,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
}

impl AsiMs2000Driver {
    pub fn simulated(id: DriverId) -> Self {
        Self::configured(id, AsiMs2000ConfiguredProbe::simulated())
    }

    pub fn configured(id: DriverId, configured: AsiMs2000ConfiguredProbe) -> Self {
        let serial = ScriptedSerial::with_reads(vec![
            format!("{}\r", configured.probe.firmware_version).into_bytes(),
            format!("{}\r", configured.probe.build_name).into_bytes(),
            b":A X=0 Y=0\r".to_vec(),
            b":A Z=0\r".to_vec(),
        ]);
        Self::new_with_transport_metadata(
            id,
            configured.probe,
            configured.endpoint,
            false,
            Box::new(serial),
        )
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: AsiMs2000ConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "ASI MS-2000 serial probe is missing serial_port metadata",
            )
        })?;
        let mut serial = numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(
                endpoint.port_name.clone(),
                endpoint.baud_rate,
            )
            .timeout(Duration::from_millis(endpoint.timeout_ms)),
        )?;
        let probe_result = protocol::execute_ms2000_probe_script(&mut serial, 4)?;
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
    pub fn serial(_id: DriverId, _configured: AsiMs2000ConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "ASI MS-2000 real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(id: DriverId, probe: protocol::AsiMs2000Probe, serial: Box<dyn SerialIo>) -> Self {
        Self::new_with_transport_metadata(id, probe, None, false, serial)
    }

    fn new_with_transport_metadata(
        id: DriverId,
        probe: protocol::AsiMs2000Probe,
        endpoint: Option<AsiSerialEndpoint>,
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
            resource: ResourceId(NodeId(id.0 * 1000 + 601)),
            hub: DeviceId(NodeId(id.0 * 1000 + 610)),
            xy: DeviceId(NodeId(id.0 * 1000 + 611)),
            z: DeviceId(NodeId(id.0 * 1000 + 612)),
            probe,
            serial_port,
            baud_rate,
            serial_timeout_ms,
            connected,
            x_um: 0.0,
            y_um: 0.0,
            z_um: 0.0,
            xy_speed_um_s: 5_000.0,
            z_speed_um_s: 1_000.0,
            xy_accel_ms: 100.0,
            z_accel_ms: 100.0,
            busy: false,
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            codec: SerialLineCodec::new(protocol::SEND_ENDING, protocol::RECV_ENDING),
        }
    }

    #[cfg(feature = "os-serial")]
    fn with_probe_result(mut self, probe_result: protocol::AsiMs2000ProbeResult) -> Self {
        self.probe.firmware_version = probe_result.probe.firmware_version;
        self.probe.build_name = probe_result.probe.build_name;
        self.x_um = probe_result.x_um.clamp(0.0, self.probe.x_travel_um);
        self.y_um = probe_result.y_um.clamp(0.0, self.probe.y_travel_um);
        self.z_um = probe_result.z_um.clamp(0.0, self.probe.z_travel_um);
        self.busy = probe_result.busy;
        self
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn send(&mut self, command: protocol::AsiCommand) -> Result<()> {
        let line = protocol::encode(&command);
        self.serial.write(&self.codec.encode(&line))
    }

    fn refresh_readback(&mut self, command: &protocol::AsiCommand) -> Result<()> {
        self.send(command.clone())?;
        let bytes = self.serial.read_available()?;
        for line in self.codec.push(&bytes) {
            self.apply_readback_reply(command, &line)?;
            return Ok(());
        }
        Ok(())
    }

    fn read_optional_ack(&mut self) -> Result<()> {
        let bytes = self.serial.read_available()?;
        for line in self.codec.push(&bytes) {
            protocol::check_ack(&line)?;
            break;
        }
        Ok(())
    }

    fn apply_readback_reply(&mut self, command: &protocol::AsiCommand, reply: &str) -> Result<()> {
        match command {
            protocol::AsiCommand::Version => {
                self.probe.firmware_version = reply.trim().to_string();
                self.emit_property(
                    self.hub,
                    "firmware_version",
                    Value::String(self.probe.firmware_version.clone()),
                );
            }
            protocol::AsiCommand::BuildName => {
                self.probe.build_name = reply.trim().to_string();
                self.emit_property(
                    self.hub,
                    "build_name",
                    Value::String(self.probe.build_name.clone()),
                );
            }
            protocol::AsiCommand::Status => {
                self.busy = protocol::is_busy(reply)?;
                self.emit_property(self.xy, "busy", Value::Bool(self.busy));
                self.emit_property(self.z, "busy", Value::Bool(self.busy));
            }
            protocol::AsiCommand::Where { axes } => {
                let positions = protocol::parse_positions(reply)?;
                if axes.contains(&protocol::Axis::X) {
                    if let Some(x_um) = positions.get("X") {
                        self.x_um = x_um.clamp(0.0, self.probe.x_travel_um);
                        self.emit_property(self.xy, "x", position(self.x_um));
                    }
                }
                if axes.contains(&protocol::Axis::Y) {
                    if let Some(y_um) = positions.get("Y") {
                        self.y_um = y_um.clamp(0.0, self.probe.y_travel_um);
                        self.emit_property(self.xy, "y", position(self.y_um));
                    }
                }
                if axes.contains(&protocol::Axis::Z) {
                    if let Some(z_um) = positions.get("Z") {
                        self.z_um = z_um.clamp(0.0, self.probe.z_travel_um);
                        self.emit_property(self.z, "z", position(self.z_um));
                    }
                }
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
                label: "asi-ms2000-hub".into(),
                vendor: Some("Applied Scientific Instrumentation".into()),
                model: Some("MS-2000/RM-2000".into()),
                serial: None,
                kinds: vec![
                    "hub".into(),
                    "motion.controller".into(),
                    "serial.ascii".into(),
                ],
                properties: vec![
                    property(
                        "firmware_version",
                        "Firmware",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                    property(
                        "build_name",
                        "Build name",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                ],
                metadata: BTreeMap::from([
                    (
                        "firmware_version".into(),
                        Value::String(self.probe.firmware_version.clone()),
                    ),
                    (
                        "build_name".into(),
                        Value::String(self.probe.build_name.clone()),
                    ),
                    (
                        "serial_units_per_um".into(),
                        Value::F64(protocol::SERIAL_UNITS_PER_UM),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.xy,
                driver: self.id,
                label: "asi-ms2000-xy".into(),
                vendor: Some("Applied Scientific Instrumentation".into()),
                model: Some("MS-2000 XY".into()),
                serial: None,
                kinds: vec!["axis.xy".into(), "stage.xy".into()],
                properties: vec![
                    sequenceable_position_property("x", "X position", true, self.probe.x_travel_um),
                    sequenceable_position_property("y", "Y position", true, self.probe.y_travel_um),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
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
                label: "asi-ms2000-z".into(),
                vendor: Some("Applied Scientific Instrumentation".into()),
                model: Some("MS-2000 Z".into()),
                serial: None,
                kinds: vec!["axis.z".into(), "stage.z".into()],
                properties: vec![
                    sequenceable_position_property("z", "Z position", true, self.probe.z_travel_um),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                ],
                metadata: BTreeMap::from([
                    ("z_travel".into(), position(self.probe.z_travel_um)),
                    (
                        "legacy_z_travel_um".into(),
                        position(self.probe.z_travel_um),
                    ),
                ]),
            },
        ]
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        match (device, key) {
            (device, "firmware_version") if device == self.hub => {
                Ok(Value::String(self.probe.firmware_version.clone()))
            }
            (device, "build_name") if device == self.hub => {
                Ok(Value::String(self.probe.build_name.clone()))
            }
            (device, "x") if device == self.xy => Ok(position(self.x_um)),
            (device, "y") if device == self.xy => Ok(position(self.y_um)),
            (device, "z") if device == self.z => Ok(position(self.z_um)),
            (device, "busy") if device == self.xy || device == self.z => Ok(Value::Bool(self.busy)),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown ASI property {key}"),
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
                self.x_um = position_um(value)?.clamp(0.0, self.probe.x_travel_um);
                self.send(protocol::AsiCommand::MoveXyAbs {
                    x_um: self.x_um,
                    y_um: self.y_um,
                })?;
                self.finish_motion();
                Ok(position(self.x_um))
            }
            (device, "y", value) if device == self.xy => {
                self.y_um = position_um(value)?.clamp(0.0, self.probe.y_travel_um);
                self.send(protocol::AsiCommand::MoveXyAbs {
                    x_um: self.x_um,
                    y_um: self.y_um,
                })?;
                self.finish_motion();
                Ok(position(self.y_um))
            }
            (device, "z", value) if device == self.z => {
                self.z_um = position_um(value)?.clamp(0.0, self.probe.z_travel_um);
                self.send(protocol::AsiCommand::MoveZAbs { z_um: self.z_um })?;
                self.finish_motion();
                Ok(position(self.z_um))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid ASI write {key}"),
            )),
        }
    }

    fn apply_state_set(&mut self, set: StateSet) -> Result<Value> {
        let mut next_x = self.x_um;
        let mut next_y = self.y_um;
        let mut next_z = self.z_um;
        let mut changed = BTreeMap::new();

        for write in set.writes {
            self.validate_write(write.device, &write.property, &write.value)?;
            match (write.device, write.property.as_str(), &write.value) {
                (device, "x", value) if device == self.xy => {
                    next_x = position_um(value)?.clamp(0.0, self.probe.x_travel_um);
                    changed.insert(format!("{}:x", (device.0).0), position(next_x));
                }
                (device, "y", value) if device == self.xy => {
                    next_y = position_um(value)?.clamp(0.0, self.probe.y_travel_um);
                    changed.insert(format!("{}:y", (device.0).0), position(next_y));
                }
                (device, "z", value) if device == self.z => {
                    next_z = position_um(value)?.clamp(0.0, self.probe.z_travel_um);
                    changed.insert(format!("{}:z", (device.0).0), position(next_z));
                }
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "unsupported ASI state-set write",
                    ))
                }
            }
        }

        let xy_changed = next_x != self.x_um || next_y != self.y_um;
        let z_changed = next_z != self.z_um;
        if xy_changed {
            self.send(protocol::AsiCommand::MoveXyAbs {
                x_um: next_x,
                y_um: next_y,
            })?;
            self.x_um = next_x;
            self.y_um = next_y;
            self.emit_property(self.xy, "x", position(self.x_um));
            self.emit_property(self.xy, "y", position(self.y_um));
        }
        if z_changed {
            self.send(protocol::AsiCommand::MoveZAbs { z_um: next_z })?;
            self.z_um = next_z;
            self.emit_property(self.z, "z", position(self.z_um));
        }
        if xy_changed || z_changed {
            self.finish_motion();
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
                (device, "x" | "y") if device == self.xy => {}
                (device, "z") if device == self.z => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "ASI MS-2000 timing plans only support XY/Z position sequences",
                    ))
                }
            }
            for value in &sequence.values {
                let _ = position_um(value)?;
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
                "asi ms2000 timing start sequence".into()
            } else {
                "asi ms2000 timing stop sequence".into()
            }),
            writes,
            commit: CommitMode::Immediate,
        })
    }

    fn validate_stage_move(&self, device: DeviceId, request: &StageMoveRequest) -> Result<()> {
        if request.target.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "ASI StageMove target must contain at least one axis",
            ));
        }
        if let Some(profile) = &request.profile {
            if matches!(profile.velocity, Some(value) if value.micrometers_per_second() <= 0.0) {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "ASI StageMove velocity profile must be positive",
                ));
            }
            if matches!(profile.acceleration, Some(value) if value.micrometers_per_second_squared() <= 0.0)
            {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "ASI StageMove acceleration profile must be positive",
                ));
            }
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
                        "ASI StageMove axis does not belong to the target device",
                    ))
                }
            }
        }
        Ok(())
    }

    fn profile_axes(&self, device: DeviceId, request: &StageMoveRequest) -> Vec<protocol::Axis> {
        if device == self.xy {
            let mut axes = Vec::new();
            for axis in request.target.keys() {
                match axis {
                    StageAxis::X => axes.push(protocol::Axis::X),
                    StageAxis::Y => axes.push(protocol::Axis::Y),
                    StageAxis::Custom(name) if name == "x" => axes.push(protocol::Axis::X),
                    StageAxis::Custom(name) if name == "y" => axes.push(protocol::Axis::Y),
                    _ => {}
                }
            }
            axes
        } else if device == self.z {
            vec![protocol::Axis::Z]
        } else {
            Vec::new()
        }
    }

    fn apply_motion_profile(
        &mut self,
        device: DeviceId,
        request: &StageMoveRequest,
    ) -> Result<BTreeMap<String, Value>> {
        let mut changed = BTreeMap::new();
        let Some(profile) = &request.profile else {
            return Ok(changed);
        };
        let axes = self.profile_axes(device, request);
        if axes.is_empty() {
            return Ok(changed);
        }
        let current_speed = if device == self.xy {
            self.xy_speed_um_s
        } else {
            self.z_speed_um_s
        };
        let speed_um_s = profile
            .velocity
            .map(|value| value.micrometers_per_second())
            .unwrap_or(current_speed);
        if profile.velocity.is_some() {
            self.send(protocol::AsiCommand::Speed {
                axes: axes
                    .iter()
                    .copied()
                    .map(|axis| (axis, speed_um_s))
                    .collect(),
            })?;
            if device == self.xy {
                self.xy_speed_um_s = speed_um_s;
            } else {
                self.z_speed_um_s = speed_um_s;
            }
            changed.insert("velocity".into(), velocity(speed_um_s));
        }
        if let Some(acceleration) = profile.acceleration {
            let ramp_ms = (speed_um_s / acceleration.micrometers_per_second_squared() * 1000.0)
                .clamp(7.0, 10_000.0);
            self.send(protocol::AsiCommand::Accel {
                axes: axes.iter().copied().map(|axis| (axis, ramp_ms)).collect(),
            })?;
            if device == self.xy {
                self.xy_accel_ms = ramp_ms;
            } else {
                self.z_accel_ms = ramp_ms;
            }
            changed.insert(
                "accel_ramp_time".into(),
                Value::TimeInterval(TimeInterval::from_milliseconds(ramp_ms)),
            );
        }
        Ok(changed)
    }

    fn stage_move(&mut self, device: DeviceId, request: StageMoveRequest) -> Result<Value> {
        self.validate_stage_move(device, &request)?;
        let mut result = self.apply_motion_profile(device, &request)?;
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
                x = (self.x_um + x).clamp(0.0, self.probe.x_travel_um);
                y = (self.y_um + y).clamp(0.0, self.probe.y_travel_um);
                self.send(protocol::AsiCommand::MoveXyRel {
                    dx_um: x - self.x_um,
                    dy_um: y - self.y_um,
                })?;
            } else {
                x = x.clamp(0.0, self.probe.x_travel_um);
                y = y.clamp(0.0, self.probe.y_travel_um);
                self.send(protocol::AsiCommand::MoveXyAbs { x_um: x, y_um: y })?;
            }
            self.x_um = x;
            self.y_um = y;
            self.finish_motion();
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
            let mut z = request
                .target
                .values()
                .next()
                .expect("validated one Z target")
                .micrometers();
            if request.relative {
                z = (self.z_um + z).clamp(0.0, self.probe.z_travel_um);
                self.send(protocol::AsiCommand::MoveZRel {
                    dz_um: z - self.z_um,
                })?;
            } else {
                z = z.clamp(0.0, self.probe.z_travel_um);
                self.send(protocol::AsiCommand::MoveZAbs { z_um: z })?;
            }
            self.z_um = z;
            self.finish_motion();
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
                "ASI StageMove target device must be XY or Z stage",
            ))
        }
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
            return Err(Error::new(ErrorCode::Unsupported, "unknown ASI capability"));
        };
        match (capability.kind, request) {
            (CapabilityKind::GenericCommand, CapabilityRequest::GenericCommand(request))
                if device == self.hub =>
            {
                self.apply_generic_command(device, request)
            }
            (CapabilityKind::GenericCommand, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "ASI GenericCommand expects a GenericCommandRequest",
            )),
            (CapabilityKind::StageMove, CapabilityRequest::StageMove(request))
                if device == self.xy || device == self.z =>
            {
                self.stage_move(device, request)
            }
            (CapabilityKind::StageMove, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "ASI StageMove expects a StageMoveRequest",
            )),
            (CapabilityKind::StageHome, CapabilityRequest::None) if device == self.xy => {
                self.send(protocol::AsiCommand::Home {
                    axes: vec![protocol::Axis::X, protocol::Axis::Y],
                })?;
                self.read_optional_ack()?;
                self.x_um = 0.0;
                self.y_um = 0.0;
                self.finish_motion();
                self.emit_property(self.xy, "x", position(self.x_um));
                self.emit_property(self.xy, "y", position(self.y_um));
                self.refresh_motion_readback(vec![protocol::Axis::X, protocol::Axis::Y])?;
                Ok(Value::String("xy homed".into()))
            }
            (CapabilityKind::StageHome, CapabilityRequest::None) if device == self.z => {
                self.send(protocol::AsiCommand::Home {
                    axes: vec![protocol::Axis::Z],
                })?;
                self.read_optional_ack()?;
                self.z_um = 0.0;
                self.finish_motion();
                self.emit_property(self.z, "z", position(self.z_um));
                self.refresh_motion_readback(vec![protocol::Axis::Z])?;
                Ok(Value::String("z homed".into()))
            }
            (CapabilityKind::StageStop, CapabilityRequest::None) => {
                self.send(protocol::AsiCommand::Halt)?;
                self.read_optional_ack()?;
                self.busy = false;
                self.emit_property(self.xy, "busy", Value::Bool(false));
                self.emit_property(self.z, "busy", Value::Bool(false));
                let axes = if device == self.xy {
                    vec![protocol::Axis::X, protocol::Axis::Y]
                } else {
                    vec![protocol::Axis::Z]
                };
                self.refresh_motion_readback(axes)?;
                Ok(Value::String("halted".into()))
            }
            (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "ASI home/stop capabilities take no request",
            )),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported ASI capability",
            )),
        }
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
                "ASI GenericCommand requires the hub device",
            ));
        }
        match request.command.as_str() {
            "refresh_readbacks" | "refresh_identity" | "refresh_status" | "refresh_position"
            | "refresh_positions" => command_no_params("ASI", request),
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!("unsupported ASI generic command {other}"),
            )),
        }
    }

    fn apply_generic_command(
        &mut self,
        device: DeviceId,
        request: GenericCommandRequest,
    ) -> Result<Value> {
        self.validate_generic_command(device, &request)?;
        match request.command.as_str() {
            "refresh_readbacks" => {
                self.refresh_identity()?;
                self.refresh_status()?;
                self.refresh_position()?;
                Ok(asi_refresh_result(
                    "refresh_readbacks",
                    "identity, status, and position query replies",
                ))
            }
            "refresh_identity" => {
                self.refresh_identity()?;
                Ok(asi_refresh_result(
                    "refresh_identity",
                    "version and build-name query replies",
                ))
            }
            "refresh_status" => {
                self.refresh_status()?;
                Ok(asi_refresh_result("refresh_status", "status query reply"))
            }
            "refresh_position" | "refresh_positions" => {
                self.refresh_position()?;
                Ok(asi_refresh_result(
                    request.command.as_str(),
                    "XY and Z position query replies",
                ))
            }
            _ => unreachable!("validated ASI generic command"),
        }
    }

    fn finish_motion(&mut self) {
        self.busy = true;
        self.pending
            .push_back(DriverEvent::Event(Event::Log(LogEvent {
                driver: Some(self.id),
                message: "asi status :B".into(),
            })));
        self.busy = false;
        self.pending
            .push_back(DriverEvent::Event(Event::Log(LogEvent {
                driver: Some(self.id),
                message: "asi status :A".into(),
            })));
    }

    fn refresh_motion_readback(&mut self, axes: Vec<protocol::Axis>) -> Result<()> {
        self.refresh_readback(&protocol::AsiCommand::Status)?;
        self.refresh_readback(&protocol::AsiCommand::Where { axes })
    }

    fn refresh_identity(&mut self) -> Result<()> {
        self.refresh_readback(&protocol::AsiCommand::Version)?;
        self.refresh_readback(&protocol::AsiCommand::BuildName)
    }

    fn refresh_status(&mut self) -> Result<()> {
        self.refresh_readback(&protocol::AsiCommand::Status)
    }

    fn refresh_position(&mut self) -> Result<()> {
        self.refresh_readback(&protocol::AsiCommand::Where {
            axes: vec![protocol::Axis::X, protocol::Axis::Y],
        })?;
        self.refresh_readback(&protocol::AsiCommand::Where {
            axes: vec![protocol::Axis::Z],
        })
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

impl Driver for AsiMs2000Driver {
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
            label: "asi-ms2000-serial".into(),
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
                ("terminator".into(), Value::String("CR".into())),
                (
                    "completion".into(),
                    Value::String("STATUS / idle-busy handling".into()),
                ),
                (
                    "support_scope".into(),
                    Value::String("MS-2000 motion/status command helpers".into()),
                ),
                (
                    "startup_readback_supported".into(),
                    Value::List(
                        protocol::ms2000_probe_script()
                            .into_iter()
                            .map(Value::String)
                            .collect(),
                    ),
                ),
            ]),
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.xy || device == self.z {
            vec![
                capability(1, device, CapabilityKind::StageMove),
                capability(2, device, CapabilityKind::StageHome),
                capability(3, device, CapabilityKind::StageStop),
            ]
        } else if device == self.hub {
            vec![capability(4, device, CapabilityKind::GenericCommand)]
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
                        description: format!("asi read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("asi write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "asi remultiplexed XY/Z motion state set".into(),
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
                            Error::new(ErrorCode::Unsupported, "unknown ASI capability")
                        })?;
                    match (&candidate.kind, request) {
                        (
                            CapabilityKind::GenericCommand,
                            CapabilityRequest::GenericCommand(request),
                        ) => {
                            self.validate_generic_command(*device, request)?;
                        }
                        (CapabilityKind::StageMove, CapabilityRequest::StageMove(request)) => {
                            self.validate_stage_move(*device, request)?;
                        }
                        (CapabilityKind::GenericCommand, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "ASI GenericCommand expects a GenericCommandRequest",
                            ));
                        }
                        (
                            CapabilityKind::StageHome | CapabilityKind::StageStop,
                            CapabilityRequest::None,
                        ) => {}
                        (CapabilityKind::StageMove, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "ASI StageMove expects a StageMoveRequest",
                            ));
                        }
                        (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "ASI home/stop capabilities take no request",
                            ));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported ASI capability",
                            ));
                        }
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("asi invoke {}", capability.0),
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
                    let readback = if device == self.xy && (key == "x" || key == "y") {
                        Some(protocol::AsiCommand::Where {
                            axes: vec![protocol::Axis::X, protocol::Axis::Y],
                        })
                    } else if device == self.z && key == "z" {
                        Some(protocol::AsiCommand::Where {
                            axes: vec![protocol::Axis::Z],
                        })
                    } else if key == "busy" {
                        Some(protocol::AsiCommand::Status)
                    } else if device == self.hub && key == "firmware_version" {
                        Some(protocol::AsiCommand::Version)
                    } else if device == self.hub && key == "build_name" {
                        Some(protocol::AsiCommand::BuildName)
                    } else {
                        None
                    };
                    if let Some(command) = readback {
                        self.refresh_readback(&command)?;
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
                        message: format!("asi serial: {line}"),
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
                description: "asi ms2000 timing arm summary".into(),
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
                description: "asi ms2000 timing start sequence".into(),
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
                description: "asi ms2000 timing stop sequence".into(),
                payload: Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "stop")),
                    ("changed".into(), changed),
                ])),
            }],
        })
    }
}

pub struct AsiTigerDiscovery {
    next_id: DriverId,
    probes: Vec<AsiTigerConfiguredProbe>,
}

impl AsiTigerDiscovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![AsiTigerConfiguredProbe::simulated()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| device.driver == "asi-tiger")
            .map(AsiTigerConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for AsiTigerDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = probe.label.clone();
                let driver = if probe.connect_real_transport {
                    Box::new(AsiTigerDriver::serial(id, probe)?) as Box<dyn Driver>
                } else {
                    Box::new(AsiTigerDriver::configured(id, probe)) as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

impl AsiTigerConfiguredProbe {
    pub fn simulated() -> Self {
        Self {
            label: "Simulated ASI Tiger controller".into(),
            probe: protocol::AsiTigerProbe::simulated(),
            endpoint: None,
            connect_real_transport: false,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = protocol::AsiTigerProbe::simulated();
        probe.firmware_version = string_prop(device, "firmware_version")
            .unwrap_or_else(|| probe.firmware_version.clone());
        probe.build_name =
            string_prop(device, "build_name").unwrap_or_else(|| probe.build_name.clone());
        probe.x_travel_um =
            position_config_um(device, "x_travel", "x_travel_um").unwrap_or(probe.x_travel_um);
        probe.y_travel_um =
            position_config_um(device, "y_travel", "y_travel_um").unwrap_or(probe.y_travel_um);
        probe.z_travel_um =
            position_config_um(device, "z_travel", "z_travel_um").unwrap_or(probe.z_travel_um);

        Ok(Self {
            label: if device.label.is_empty() {
                "Configured ASI Tiger controller".into()
            } else {
                device.label.clone()
            },
            probe,
            endpoint: asi_endpoint_from_config(device),
            connect_real_transport: bool_prop(device, "connect").unwrap_or(false),
        })
    }
}

pub struct AsiTigerDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    xy: DeviceId,
    z: DeviceId,
    ttl: DeviceId,
    ring: DeviceId,
    crisp: DeviceId,
    probe: protocol::AsiTigerProbe,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
    x_um: f64,
    y_um: f64,
    z_um: f64,
    ttl0: bool,
    ring_mode: String,
    ring_size: i64,
    ring_running: bool,
    crisp_state: protocol::CrispState,
    crisp_focus_score: f64,
    crisp_offset_um: f64,
    crisp_objective_na: f64,
    crisp_lock_range_mm: f64,
    crisp_in_focus_range_um: f64,
    xy_speed_um_s: f64,
    z_speed_um_s: f64,
    xy_accel_ms: f64,
    z_accel_ms: f64,
    busy: bool,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
}

impl AsiTigerDriver {
    pub fn simulated(id: DriverId) -> Self {
        Self::configured(id, AsiTigerConfiguredProbe::simulated())
    }

    pub fn configured(id: DriverId, configured: AsiTigerConfiguredProbe) -> Self {
        let serial = ScriptedSerial::with_reads(vec![
            format!("{}\r", configured.probe.firmware_version).into_bytes(),
            format!("{}\r", configured.probe.build_name).into_bytes(),
            b":A X=0 Y=0\r".to_vec(),
            b":A Z=0\r".to_vec(),
        ]);
        Self::new_with_transport_metadata(
            id,
            configured.probe,
            configured.endpoint,
            false,
            Box::new(serial),
        )
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: AsiTigerConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "ASI Tiger serial probe is missing serial_port metadata",
            )
        })?;
        let mut serial = numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(
                endpoint.port_name.clone(),
                endpoint.baud_rate,
            )
            .timeout(Duration::from_millis(endpoint.timeout_ms)),
        )?;
        let probe_result = protocol::execute_tiger_probe_script(&mut serial, &configured.probe, 4)?;
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
    pub fn serial(_id: DriverId, _configured: AsiTigerConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "ASI Tiger real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(id: DriverId, probe: protocol::AsiTigerProbe, serial: Box<dyn SerialIo>) -> Self {
        Self::new_with_transport_metadata(id, probe, None, false, serial)
    }

    fn new_with_transport_metadata(
        id: DriverId,
        probe: protocol::AsiTigerProbe,
        endpoint: Option<AsiSerialEndpoint>,
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
            resource: ResourceId(NodeId(id.0 * 1000 + 631)),
            hub: DeviceId(NodeId(id.0 * 1000 + 640)),
            xy: DeviceId(NodeId(id.0 * 1000 + 641)),
            z: DeviceId(NodeId(id.0 * 1000 + 642)),
            ttl: DeviceId(NodeId(id.0 * 1000 + 643)),
            ring: DeviceId(NodeId(id.0 * 1000 + 644)),
            crisp: DeviceId(NodeId(id.0 * 1000 + 645)),
            probe,
            serial_port,
            baud_rate,
            serial_timeout_ms,
            connected,
            x_um: 0.0,
            y_um: 0.0,
            z_um: 0.0,
            ttl0: false,
            ring_mode: "off".into(),
            ring_size: 0,
            ring_running: false,
            crisp_state: protocol::CrispState::Ready,
            crisp_focus_score: 0.0,
            crisp_offset_um: 0.0,
            crisp_objective_na: 0.75,
            crisp_lock_range_mm: 0.5,
            crisp_in_focus_range_um: 0.1,
            xy_speed_um_s: 5_000.0,
            z_speed_um_s: 1_000.0,
            xy_accel_ms: 100.0,
            z_accel_ms: 100.0,
            busy: false,
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            codec: SerialLineCodec::new(protocol::SEND_ENDING, protocol::RECV_ENDING),
        }
    }

    #[cfg(feature = "os-serial")]
    fn with_probe_result(mut self, probe_result: protocol::AsiTigerProbeResult) -> Self {
        self.probe.firmware_version = probe_result.probe.firmware_version;
        self.probe.build_name = probe_result.probe.build_name;
        self.x_um = probe_result.x_um.clamp(0.0, self.probe.x_travel_um);
        self.y_um = probe_result.y_um.clamp(0.0, self.probe.y_travel_um);
        self.z_um = probe_result.z_um.clamp(0.0, self.probe.z_travel_um);
        self.busy = probe_result.busy_by_card.values().any(|busy| *busy);
        if let Some(crisp_state) = probe_result.crisp_state {
            self.crisp_state = crisp_state;
        }
        if let Some(crisp_focus_score) = probe_result.crisp_focus_score {
            self.crisp_focus_score = crisp_focus_score;
        }
        self
    }

    pub fn graph(&self) -> DeviceGraph {
        let mut graph = DeviceGraph::default();
        let _ = graph.insert_node(GraphNode {
            id: self.resource.0,
            kind: NodeKind::Resource,
            label: "asi-tiger-serial".into(),
        });
        let _ = graph.insert_node(GraphNode {
            id: self.hub.0,
            kind: NodeKind::Hub,
            label: "asi-tiger-hub".into(),
        });
        let _ = graph.insert_edge(GraphEdge {
            from: self.resource.0,
            to: self.hub.0,
            kind: EdgeKind::OwnsResource,
        });
        for device in [self.xy, self.z, self.ttl, self.ring, self.crisp] {
            let label = self
                .descriptors_for()
                .into_iter()
                .find(|descriptor| descriptor.id == device)
                .map(|descriptor| descriptor.label)
                .unwrap_or_else(|| format!("asi-tiger-node-{}", device.0 .0));
            let _ = graph.insert_node(GraphNode {
                id: device.0,
                kind: NodeKind::Device,
                label,
            });
            let _ = graph.insert_edge(GraphEdge {
                from: self.hub.0,
                to: device.0,
                kind: EdgeKind::OffersDevice,
            });
        }
        let _ = graph.insert_device_dependency(self.z.0, self.crisp.0, Role::ZStage);
        graph
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn card(&self, module: protocol::TigerModuleKind) -> Result<u8> {
        self.probe
            .cards
            .iter()
            .find(|card| card.module == module)
            .map(|card| card.address)
            .ok_or_else(|| Error::new(ErrorCode::Unsupported, "ASI Tiger module is not present"))
    }

    fn send(&mut self, command: protocol::TigerCommand) -> Result<()> {
        let line = protocol::encode_tiger(&command);
        self.serial.write(&self.codec.encode(&line))
    }

    fn refresh_readback(&mut self, command: &protocol::TigerCommand) -> Result<()> {
        self.send(command.clone())?;
        let bytes = self.serial.read_available()?;
        for line in self.codec.push(&bytes) {
            self.apply_readback_reply(command, &line)?;
            return Ok(());
        }
        Ok(())
    }

    fn read_optional_ack(&mut self) -> Result<()> {
        let bytes = self.serial.read_available()?;
        for line in self.codec.push(&bytes) {
            protocol::check_ack(&line)?;
            break;
        }
        Ok(())
    }

    fn apply_readback_reply(
        &mut self,
        command: &protocol::TigerCommand,
        reply: &str,
    ) -> Result<()> {
        match command {
            protocol::TigerCommand::Card {
                command: protocol::AsiCommand::Version,
                ..
            } => {
                self.probe.firmware_version = reply.trim().to_string();
                self.emit_property(
                    self.hub,
                    "firmware_version",
                    Value::String(self.probe.firmware_version.clone()),
                );
            }
            protocol::TigerCommand::Card {
                command: protocol::AsiCommand::BuildName,
                ..
            } => {
                self.probe.build_name = reply.trim().to_string();
                self.emit_property(
                    self.hub,
                    "build_name",
                    Value::String(self.probe.build_name.clone()),
                );
            }
            protocol::TigerCommand::Card {
                command: protocol::AsiCommand::Status,
                ..
            } => {
                self.busy = protocol::is_busy(reply)?;
                self.emit_property(self.xy, "busy", Value::Bool(self.busy));
                self.emit_property(self.z, "busy", Value::Bool(self.busy));
            }
            protocol::TigerCommand::Card {
                command: protocol::AsiCommand::Where { axes },
                ..
            } => {
                let positions = protocol::parse_positions(reply)?;
                if axes.contains(&protocol::Axis::X) {
                    if let Some(x_um) = positions.get("X") {
                        self.x_um = x_um.clamp(0.0, self.probe.x_travel_um);
                        self.emit_property(self.xy, "x", position(self.x_um));
                    }
                }
                if axes.contains(&protocol::Axis::Y) {
                    if let Some(y_um) = positions.get("Y") {
                        self.y_um = y_um.clamp(0.0, self.probe.y_travel_um);
                        self.emit_property(self.xy, "y", position(self.y_um));
                    }
                }
                if axes.contains(&protocol::Axis::Z) {
                    if let Some(z_um) = positions.get("Z") {
                        self.z_um = z_um.clamp(0.0, self.probe.z_travel_um);
                        self.emit_property(self.z, "z", position(self.z_um));
                    }
                }
            }
            protocol::TigerCommand::CrispQueryState { .. } => {
                if let Some(state) = protocol::CrispState::from_label(protocol::parse_last_value(
                    reply,
                ))
                .or_else(|| {
                    protocol::parse_last_value(reply)
                        .chars()
                        .next()
                        .and_then(|ch| protocol::CrispState::from_label(&ch.to_string()))
                }) {
                    self.crisp_state = state;
                    self.emit_crisp_status();
                }
            }
            protocol::TigerCommand::CrispQueryFocusScore { .. } => {
                self.crisp_focus_score = protocol::parse_reply_number("Tiger LK Y?", reply)?;
                self.emit_property(
                    self.crisp,
                    "focus_score",
                    Value::F64(self.crisp_focus_score),
                );
            }
            protocol::TigerCommand::CrispQueryOffset { .. } => {
                self.crisp_offset_um = protocol::parse_reply_number("Tiger LK Z?", reply)?;
                self.emit_property(self.crisp, "offset", position(self.crisp_offset_um));
            }
            protocol::TigerCommand::CrispQueryObjectiveNa { .. } => {
                self.crisp_objective_na = protocol::parse_reply_number("Tiger LR Y?", reply)?;
                self.emit_property(
                    self.crisp,
                    "objective_na",
                    Value::NumericalAperture(NumericalAperture::new(self.crisp_objective_na)),
                );
            }
            protocol::TigerCommand::CrispQueryLockRange { .. } => {
                self.crisp_lock_range_mm = protocol::parse_reply_number("Tiger LR Z?", reply)?;
                self.emit_property(
                    self.crisp,
                    "lock_range",
                    position(self.crisp_lock_range_mm * 1000.0),
                );
            }
            protocol::TigerCommand::CrispQueryInFocusRange { .. } => {
                self.crisp_in_focus_range_um = protocol::parse_reply_number("Tiger AL Z?", reply)?;
                self.emit_property(
                    self.crisp,
                    "in_focus_range",
                    position(self.crisp_in_focus_range_um),
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
                label: "asi-tiger-hub".into(),
                vendor: Some("Applied Scientific Instrumentation".into()),
                model: Some("Tiger".into()),
                serial: None,
                kinds: vec![
                    "hub".into(),
                    "motion.controller".into(),
                    "serial.ascii".into(),
                    "asi.tiger".into(),
                ],
                properties: vec![
                    property(
                        "firmware_version",
                        "Firmware",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                    property(
                        "build_name",
                        "Build name",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                    property("cards", "Cards", ValueType::List, None, false, None),
                ],
                metadata: BTreeMap::from([
                    (
                        "firmware_version".into(),
                        Value::String(self.probe.firmware_version.clone()),
                    ),
                    (
                        "build_name".into(),
                        Value::String(self.probe.build_name.clone()),
                    ),
                    ("cards".into(), self.card_metadata()),
                ]),
            },
            DeviceDescriptor {
                id: self.xy,
                driver: self.id,
                label: "asi-tiger-xy".into(),
                vendor: Some("Applied Scientific Instrumentation".into()),
                model: Some("Tiger XY card".into()),
                serial: None,
                kinds: vec!["axis.xy".into(), "stage.xy".into(), "asi.tiger.card".into()],
                properties: vec![
                    sequenceable_position_property("x", "X position", true, self.probe.x_travel_um),
                    sequenceable_position_property("y", "Y position", true, self.probe.y_travel_um),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                ],
                metadata: BTreeMap::from([
                    (
                        "card_address".into(),
                        Value::I64(
                            self.card(protocol::TigerModuleKind::XyStage).unwrap_or(0) as i64
                        ),
                    ),
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
                label: "asi-tiger-z".into(),
                vendor: Some("Applied Scientific Instrumentation".into()),
                model: Some("Tiger Z card".into()),
                serial: None,
                kinds: vec!["axis.z".into(), "stage.z".into(), "asi.tiger.card".into()],
                properties: vec![
                    sequenceable_position_property("z", "Z position", true, self.probe.z_travel_um),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                ],
                metadata: BTreeMap::from([
                    (
                        "card_address".into(),
                        Value::I64(self.card(protocol::TigerModuleKind::ZStage).unwrap_or(0) as i64),
                    ),
                    ("z_travel".into(), position(self.probe.z_travel_um)),
                    (
                        "legacy_z_travel_um".into(),
                        position(self.probe.z_travel_um),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.ttl,
                driver: self.id,
                label: "asi-tiger-ttl".into(),
                vendor: Some("Applied Scientific Instrumentation".into()),
                model: Some("Tiger TTL IO card".into()),
                serial: None,
                kinds: vec![
                    "digital.output".into(),
                    "trigger.source".into(),
                    "asi.tiger.card".into(),
                ],
                properties: vec![sequenceable_property(
                    "ttl0",
                    "TTL 0",
                    ValueType::Bool,
                    None,
                    true,
                    None,
                )],
                metadata: BTreeMap::from([(
                    "card_address".into(),
                    Value::I64(self.card(protocol::TigerModuleKind::TtlIo).unwrap_or(0) as i64),
                )]),
            },
            DeviceDescriptor {
                id: self.ring,
                driver: self.id,
                label: "asi-tiger-ring-buffer".into(),
                vendor: Some("Applied Scientific Instrumentation".into()),
                model: Some("Tiger ring buffer".into()),
                serial: None,
                kinds: vec![
                    "motion.program".into(),
                    "ring.buffer".into(),
                    "asi.tiger.card".into(),
                ],
                properties: vec![
                    property("mode", "Mode", ValueType::String, None, true, None),
                    property("size", "Size", ValueType::I64, None, true, None),
                    sequenceable_property("running", "Running", ValueType::Bool, None, true, None),
                ],
                metadata: BTreeMap::from([(
                    "card_address".into(),
                    Value::I64(
                        self.card(protocol::TigerModuleKind::RingBuffer)
                            .unwrap_or(0) as i64,
                    ),
                )]),
            },
            DeviceDescriptor {
                id: self.crisp,
                driver: self.id,
                label: "asi-tiger-crisp-autofocus".into(),
                vendor: Some("Applied Scientific Instrumentation".into()),
                model: Some("Tiger CRISP autofocus".into()),
                serial: None,
                kinds: vec![
                    "autofocus".into(),
                    "continuous.focus".into(),
                    "asi.crisp".into(),
                    "asi.tiger.card".into(),
                ],
                properties: vec![
                    crisp_state_property(),
                    property(
                        "continuous",
                        "Continuous",
                        ValueType::Bool,
                        None,
                        true,
                        None,
                    ),
                    property("locked", "Locked", ValueType::Bool, None, false, None),
                    property(
                        "focus_score",
                        "Focus score",
                        ValueType::F64,
                        None,
                        false,
                        None,
                    ),
                    position_property_range("offset", "Lock offset", true, -10_000.0, 10_000.0),
                    numerical_aperture_property_range(
                        "objective_na",
                        "Objective NA",
                        true,
                        0.0,
                        2.0,
                    ),
                    position_property_range("lock_range", "Lock range", true, 0.0, 10_000.0),
                    position_property_range("in_focus_range", "In focus range", true, 0.0, 100.0),
                ],
                metadata: BTreeMap::from([
                    (
                        "card_address".into(),
                        Value::I64(
                            self.card(protocol::TigerModuleKind::CrispAutofocus)
                                .unwrap_or(0) as i64,
                        ),
                    ),
                    ("depends_on".into(), Value::String("asi-tiger-z".into())),
                    (
                        "protocol".into(),
                        Value::String("ASI CRISP LK/LR/AL command families".into()),
                    ),
                ]),
            },
        ]
    }

    fn card_metadata(&self) -> Value {
        Value::List(
            self.probe
                .cards
                .iter()
                .map(|card| {
                    Value::Map(BTreeMap::from([
                        ("address".into(), Value::I64(card.address as i64)),
                        ("label".into(), Value::String(card.label.clone())),
                        ("module".into(), Value::String(format!("{:?}", card.module))),
                        (
                            "axes".into(),
                            Value::List(
                                card.axes
                                    .iter()
                                    .map(|axis| Value::String(axis.name().into()))
                                    .collect(),
                            ),
                        ),
                    ]))
                })
                .collect(),
        )
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        match (device, key) {
            (device, "firmware_version") if device == self.hub => {
                Ok(Value::String(self.probe.firmware_version.clone()))
            }
            (device, "build_name") if device == self.hub => {
                Ok(Value::String(self.probe.build_name.clone()))
            }
            (device, "cards") if device == self.hub => Ok(self.card_metadata()),
            (device, "x") if device == self.xy => Ok(position(self.x_um)),
            (device, "y") if device == self.xy => Ok(position(self.y_um)),
            (device, "z") if device == self.z => Ok(position(self.z_um)),
            (device, "busy") if device == self.xy || device == self.z => Ok(Value::Bool(self.busy)),
            (device, "ttl0") if device == self.ttl => Ok(Value::Bool(self.ttl0)),
            (device, "mode") if device == self.ring => Ok(Value::String(self.ring_mode.clone())),
            (device, "size") if device == self.ring => Ok(Value::I64(self.ring_size)),
            (device, "running") if device == self.ring => Ok(Value::Bool(self.ring_running)),
            (device, "state") if device == self.crisp => {
                Ok(Value::String(self.crisp_state.label().into()))
            }
            (device, "continuous") if device == self.crisp => Ok(Value::Bool(matches!(
                self.crisp_state,
                protocol::CrispState::Locking | protocol::CrispState::Locked
            ))),
            (device, "locked") if device == self.crisp => Ok(Value::Bool(
                self.crisp_state == protocol::CrispState::Locked,
            )),
            (device, "focus_score") if device == self.crisp => {
                Ok(Value::F64(self.crisp_focus_score))
            }
            (device, "offset") if device == self.crisp => Ok(position(self.crisp_offset_um)),
            (device, "objective_na") if device == self.crisp => Ok(Value::NumericalAperture(
                NumericalAperture::new(self.crisp_objective_na),
            )),
            (device, "lock_range") if device == self.crisp => {
                Ok(position(self.crisp_lock_range_mm * 1000.0))
            }
            (device, "in_focus_range") if device == self.crisp => {
                Ok(position(self.crisp_in_focus_range_um))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown ASI Tiger property {key}"),
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
                self.x_um = position_um(value)?.clamp(0.0, self.probe.x_travel_um);
                self.send(protocol::TigerCommand::Card {
                    address: self.card(protocol::TigerModuleKind::XyStage)?,
                    command: protocol::AsiCommand::MoveXyAbs {
                        x_um: self.x_um,
                        y_um: self.y_um,
                    },
                })?;
                self.finish_motion();
                Ok(position(self.x_um))
            }
            (device, "y", value) if device == self.xy => {
                self.y_um = position_um(value)?.clamp(0.0, self.probe.y_travel_um);
                self.send(protocol::TigerCommand::Card {
                    address: self.card(protocol::TigerModuleKind::XyStage)?,
                    command: protocol::AsiCommand::MoveXyAbs {
                        x_um: self.x_um,
                        y_um: self.y_um,
                    },
                })?;
                self.finish_motion();
                Ok(position(self.y_um))
            }
            (device, "z", value) if device == self.z => {
                self.z_um = position_um(value)?.clamp(0.0, self.probe.z_travel_um);
                self.send(protocol::TigerCommand::Card {
                    address: self.card(protocol::TigerModuleKind::ZStage)?,
                    command: protocol::AsiCommand::MoveZAbs { z_um: self.z_um },
                })?;
                self.finish_motion();
                Ok(position(self.z_um))
            }
            (device, "ttl0", Value::Bool(high)) if device == self.ttl => {
                self.ttl0 = *high;
                self.send(protocol::TigerCommand::TtlOut {
                    address: self.card(protocol::TigerModuleKind::TtlIo)?,
                    line: 0,
                    high: *high,
                })?;
                Ok(Value::Bool(self.ttl0))
            }
            (device, "mode", Value::String(mode)) if device == self.ring => {
                self.ring_mode = mode.clone();
                self.send(protocol::TigerCommand::RingBufferMode {
                    address: self.card(protocol::TigerModuleKind::RingBuffer)?,
                    mode: mode.clone(),
                })?;
                Ok(Value::String(self.ring_mode.clone()))
            }
            (device, "size", Value::I64(size)) if device == self.ring => {
                self.ring_size = (*size).max(0);
                Ok(Value::I64(self.ring_size))
            }
            (device, "running", Value::Bool(running)) if device == self.ring => {
                self.ring_running = *running;
                let address = self.card(protocol::TigerModuleKind::RingBuffer)?;
                if *running {
                    self.send(protocol::TigerCommand::RingBufferStart { address })?;
                } else {
                    self.send(protocol::TigerCommand::RingBufferStop { address })?;
                }
                Ok(Value::Bool(self.ring_running))
            }
            (device, "state", Value::String(state)) if device == self.crisp => {
                let state = protocol::CrispState::from_label(state).ok_or_else(|| {
                    Error::new(ErrorCode::InvalidProperty, "unknown ASI CRISP state")
                })?;
                self.set_crisp_state(state)?;
                Ok(Value::String(self.crisp_state.label().into()))
            }
            (device, "continuous", Value::Bool(enabled)) if device == self.crisp => {
                if *enabled {
                    self.set_crisp_state(protocol::CrispState::Locking)?;
                    self.crisp_state = protocol::CrispState::Locked;
                } else {
                    self.set_crisp_state(protocol::CrispState::Ready)?;
                }
                Ok(Value::Bool(*enabled))
            }
            (device, "offset", value) if device == self.crisp => {
                self.crisp_offset_um = position_um(value)?;
                self.send(protocol::TigerCommand::CrispSetOffset {
                    address: self.card(protocol::TigerModuleKind::CrispAutofocus)?,
                    offset_um: self.crisp_offset_um,
                })?;
                Ok(position(self.crisp_offset_um))
            }
            (device, "objective_na", Value::NumericalAperture(na)) if device == self.crisp => {
                self.crisp_objective_na = na.value();
                self.send(protocol::TigerCommand::CrispSetObjectiveNa {
                    address: self.card(protocol::TigerModuleKind::CrispAutofocus)?,
                    na: na.value(),
                })?;
                Ok(Value::NumericalAperture(NumericalAperture::new(
                    self.crisp_objective_na,
                )))
            }
            (device, "lock_range", value) if device == self.crisp => {
                self.crisp_lock_range_mm = position_um(value)? / 1000.0;
                self.send(protocol::TigerCommand::CrispSetLockRange {
                    address: self.card(protocol::TigerModuleKind::CrispAutofocus)?,
                    range_mm: self.crisp_lock_range_mm,
                })?;
                Ok(position(self.crisp_lock_range_mm * 1000.0))
            }
            (device, "in_focus_range", value) if device == self.crisp => {
                self.crisp_in_focus_range_um = position_um(value)?;
                self.send(protocol::TigerCommand::CrispSetInFocusRange {
                    address: self.card(protocol::TigerModuleKind::CrispAutofocus)?,
                    range_um: self.crisp_in_focus_range_um,
                })?;
                Ok(position(self.crisp_in_focus_range_um))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid ASI Tiger write {key}"),
            )),
        }
    }

    fn set_crisp_state(&mut self, state: protocol::CrispState) -> Result<()> {
        self.send(protocol::TigerCommand::CrispSetState {
            address: self.card(protocol::TigerModuleKind::CrispAutofocus)?,
            state,
        })?;
        self.crisp_state = match state {
            protocol::CrispState::Locking => protocol::CrispState::Locking,
            other => other,
        };
        self.pending
            .push_back(DriverEvent::Event(Event::Log(LogEvent {
                driver: Some(self.id),
                message: format!("asi crisp state command {}", self.crisp_state.code()),
            })));
        Ok(())
    }

    fn apply_state_set(&mut self, set: StateSet) -> Result<Value> {
        for write in &set.writes {
            self.validate_write(write.device, &write.property, &write.value)?;
        }

        let mut next_x = self.x_um;
        let mut next_y = self.y_um;
        let mut next_z = self.z_um;
        let mut next_ttl0 = self.ttl0;
        let mut next_ring_mode = self.ring_mode.clone();
        let mut next_ring_size = self.ring_size;
        let mut next_ring_running = self.ring_running;
        let mut next_crisp_state = self.crisp_state;
        let mut next_crisp_offset_um = self.crisp_offset_um;
        let mut next_crisp_objective_na = self.crisp_objective_na;
        let mut next_crisp_lock_range_mm = self.crisp_lock_range_mm;
        let mut next_crisp_in_focus_range_um = self.crisp_in_focus_range_um;
        let mut changed = BTreeMap::new();

        for write in &set.writes {
            match (write.device, write.property.as_str(), &write.value) {
                (device, "x", value) if device == self.xy => {
                    next_x = position_um(value)?.clamp(0.0, self.probe.x_travel_um);
                }
                (device, "y", value) if device == self.xy => {
                    next_y = position_um(value)?.clamp(0.0, self.probe.y_travel_um);
                }
                (device, "z", value) if device == self.z => {
                    next_z = position_um(value)?.clamp(0.0, self.probe.z_travel_um);
                }
                (device, "ttl0", Value::Bool(high)) if device == self.ttl => next_ttl0 = *high,
                (device, "mode", Value::String(mode)) if device == self.ring => {
                    next_ring_mode = mode.clone()
                }
                (device, "size", Value::I64(size)) if device == self.ring => {
                    next_ring_size = (*size).max(0)
                }
                (device, "running", Value::Bool(running)) if device == self.ring => {
                    next_ring_running = *running
                }
                (device, "state", Value::String(state)) if device == self.crisp => {
                    next_crisp_state =
                        protocol::CrispState::from_label(state).ok_or_else(|| {
                            Error::new(ErrorCode::InvalidProperty, "unknown ASI CRISP state")
                        })?;
                }
                (device, "continuous", Value::Bool(enabled)) if device == self.crisp => {
                    next_crisp_state = if *enabled {
                        protocol::CrispState::Locked
                    } else {
                        protocol::CrispState::Ready
                    };
                }
                (device, "offset", value) if device == self.crisp => {
                    next_crisp_offset_um = position_um(value)?
                }
                (device, "objective_na", Value::NumericalAperture(na)) if device == self.crisp => {
                    next_crisp_objective_na = na.value()
                }
                (device, "lock_range", value) if device == self.crisp => {
                    next_crisp_lock_range_mm = position_um(value)? / 1000.0
                }
                (device, "in_focus_range", value) if device == self.crisp => {
                    next_crisp_in_focus_range_um = position_um(value)?
                }
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "unsupported ASI Tiger state-set write",
                    ))
                }
            }
        }

        if next_ring_mode != self.ring_mode {
            self.ring_mode = next_ring_mode;
            self.send(protocol::TigerCommand::RingBufferMode {
                address: self.card(protocol::TigerModuleKind::RingBuffer)?,
                mode: self.ring_mode.clone(),
            })?;
            changed.insert("ring:mode".into(), Value::String(self.ring_mode.clone()));
            self.emit_property(self.ring, "mode", Value::String(self.ring_mode.clone()));
        }
        if next_ring_size != self.ring_size {
            self.ring_size = next_ring_size;
            changed.insert("ring:size".into(), Value::I64(self.ring_size));
            self.emit_property(self.ring, "size", Value::I64(self.ring_size));
        }

        let xy_changed = next_x != self.x_um || next_y != self.y_um;
        if xy_changed {
            self.send(protocol::TigerCommand::Card {
                address: self.card(protocol::TigerModuleKind::XyStage)?,
                command: protocol::AsiCommand::MoveXyAbs {
                    x_um: next_x,
                    y_um: next_y,
                },
            })?;
            self.x_um = next_x;
            self.y_um = next_y;
            changed.insert("xy:x".into(), position(self.x_um));
            changed.insert("xy:y".into(), position(self.y_um));
            self.emit_property(self.xy, "x", position(self.x_um));
            self.emit_property(self.xy, "y", position(self.y_um));
        }

        if next_z != self.z_um {
            self.send(protocol::TigerCommand::Card {
                address: self.card(protocol::TigerModuleKind::ZStage)?,
                command: protocol::AsiCommand::MoveZAbs { z_um: next_z },
            })?;
            self.z_um = next_z;
            changed.insert("z:z".into(), position(self.z_um));
            self.emit_property(self.z, "z", position(self.z_um));
        }

        if next_ttl0 != self.ttl0 {
            self.ttl0 = next_ttl0;
            self.send(protocol::TigerCommand::TtlOut {
                address: self.card(protocol::TigerModuleKind::TtlIo)?,
                line: 0,
                high: self.ttl0,
            })?;
            changed.insert("ttl:ttl0".into(), Value::Bool(self.ttl0));
            self.emit_property(self.ttl, "ttl0", Value::Bool(self.ttl0));
        }

        if next_ring_running != self.ring_running {
            self.ring_running = next_ring_running;
            let address = self.card(protocol::TigerModuleKind::RingBuffer)?;
            if self.ring_running {
                self.send(protocol::TigerCommand::RingBufferStart { address })?;
            } else {
                self.send(protocol::TigerCommand::RingBufferStop { address })?;
            }
            changed.insert("ring:running".into(), Value::Bool(self.ring_running));
            self.emit_property(self.ring, "running", Value::Bool(self.ring_running));
        }

        let crisp_address = self.card(protocol::TigerModuleKind::CrispAutofocus)?;
        if next_crisp_objective_na != self.crisp_objective_na {
            self.crisp_objective_na = next_crisp_objective_na;
            self.send(protocol::TigerCommand::CrispSetObjectiveNa {
                address: crisp_address,
                na: self.crisp_objective_na,
            })?;
            changed.insert(
                "crisp:objective_na".into(),
                Value::NumericalAperture(NumericalAperture::new(self.crisp_objective_na)),
            );
            self.emit_property(
                self.crisp,
                "objective_na",
                Value::NumericalAperture(NumericalAperture::new(self.crisp_objective_na)),
            );
        }
        if next_crisp_lock_range_mm != self.crisp_lock_range_mm {
            self.crisp_lock_range_mm = next_crisp_lock_range_mm;
            self.send(protocol::TigerCommand::CrispSetLockRange {
                address: crisp_address,
                range_mm: self.crisp_lock_range_mm,
            })?;
            changed.insert(
                "crisp:lock_range".into(),
                position(self.crisp_lock_range_mm * 1000.0),
            );
            self.emit_property(
                self.crisp,
                "lock_range",
                position(self.crisp_lock_range_mm * 1000.0),
            );
        }
        if next_crisp_in_focus_range_um != self.crisp_in_focus_range_um {
            self.crisp_in_focus_range_um = next_crisp_in_focus_range_um;
            self.send(protocol::TigerCommand::CrispSetInFocusRange {
                address: crisp_address,
                range_um: self.crisp_in_focus_range_um,
            })?;
            changed.insert(
                "crisp:in_focus_range".into(),
                position(self.crisp_in_focus_range_um),
            );
            self.emit_property(
                self.crisp,
                "in_focus_range",
                position(self.crisp_in_focus_range_um),
            );
        }
        if next_crisp_offset_um != self.crisp_offset_um {
            self.crisp_offset_um = next_crisp_offset_um;
            self.send(protocol::TigerCommand::CrispSetOffset {
                address: crisp_address,
                offset_um: self.crisp_offset_um,
            })?;
            changed.insert("crisp:offset".into(), position(self.crisp_offset_um));
            self.emit_property(self.crisp, "offset", position(self.crisp_offset_um));
        }
        if next_crisp_state != self.crisp_state {
            self.set_crisp_state(next_crisp_state)?;
            if next_crisp_state == protocol::CrispState::Locked {
                self.crisp_focus_score = 1.0;
            }
            changed.insert(
                "crisp:state".into(),
                Value::String(self.crisp_state.label().into()),
            );
            changed.insert(
                "crisp:continuous".into(),
                Value::Bool(matches!(
                    self.crisp_state,
                    protocol::CrispState::Locking | protocol::CrispState::Locked
                )),
            );
            self.emit_property(
                self.crisp,
                "state",
                Value::String(self.crisp_state.label().into()),
            );
            self.emit_property(
                self.crisp,
                "continuous",
                Value::Bool(matches!(
                    self.crisp_state,
                    protocol::CrispState::Locking | protocol::CrispState::Locked
                )),
            );
            self.emit_property(
                self.crisp,
                "locked",
                Value::Bool(self.crisp_state == protocol::CrispState::Locked),
            );
        }

        if xy_changed || changed.contains_key("z:z") {
            self.finish_motion();
        }

        Ok(Value::Map(changed))
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
                "ASI Tiger GenericCommand requires the hub device",
            ));
        }
        match request.command.as_str() {
            "refresh_readbacks" | "refresh_identity" | "refresh_status" | "refresh_position"
            | "refresh_positions" => command_no_params("ASI Tiger", request),
            "refresh_crisp" => {
                command_no_params("ASI Tiger", request)?;
                self.card(protocol::TigerModuleKind::CrispAutofocus)
                    .map(|_| ())
            }
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!("unsupported ASI Tiger generic command {other}"),
            )),
        }
    }

    fn apply_generic_command(
        &mut self,
        device: DeviceId,
        request: GenericCommandRequest,
    ) -> Result<Value> {
        self.validate_generic_command(device, &request)?;
        match request.command.as_str() {
            "refresh_readbacks" => {
                self.refresh_identity()?;
                self.refresh_status()?;
                self.refresh_position()?;
                if self
                    .probe
                    .cards
                    .iter()
                    .any(|card| card.module == protocol::TigerModuleKind::CrispAutofocus)
                {
                    self.refresh_crisp()?;
                }
                Ok(asi_refresh_result(
                    "refresh_readbacks",
                    "identity, card status, position, and available CRISP query replies",
                ))
            }
            "refresh_identity" => {
                self.refresh_identity()?;
                Ok(asi_refresh_result(
                    "refresh_identity",
                    "version and build-name query replies",
                ))
            }
            "refresh_status" => {
                self.refresh_status()?;
                Ok(asi_refresh_result(
                    "refresh_status",
                    "configured card status query replies",
                ))
            }
            "refresh_position" | "refresh_positions" => {
                self.refresh_position()?;
                Ok(asi_refresh_result(
                    request.command.as_str(),
                    "XY and Z card position query replies",
                ))
            }
            "refresh_crisp" => {
                self.refresh_crisp()?;
                Ok(asi_refresh_result(
                    "refresh_crisp",
                    "CRISP state, score, offset, objective, and range query replies",
                ))
            }
            _ => unreachable!("validated ASI Tiger generic command"),
        }
    }

    fn validate_stage_move(&self, device: DeviceId, request: &StageMoveRequest) -> Result<()> {
        if request.target.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "ASI Tiger StageMove target must contain at least one axis",
            ));
        }
        if let Some(profile) = &request.profile {
            if matches!(profile.velocity, Some(value) if value.micrometers_per_second() <= 0.0) {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "ASI Tiger StageMove velocity profile must be positive",
                ));
            }
            if matches!(profile.acceleration, Some(value) if value.micrometers_per_second_squared() <= 0.0)
            {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "ASI Tiger StageMove acceleration profile must be positive",
                ));
            }
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
                        "ASI Tiger StageMove axis does not belong to the target device",
                    ))
                }
            }
        }
        Ok(())
    }

    fn profile_axes(&self, device: DeviceId, request: &StageMoveRequest) -> Vec<protocol::Axis> {
        if device == self.xy {
            let mut axes = Vec::new();
            for axis in request.target.keys() {
                match axis {
                    StageAxis::X => axes.push(protocol::Axis::X),
                    StageAxis::Y => axes.push(protocol::Axis::Y),
                    StageAxis::Custom(name) if name == "x" => axes.push(protocol::Axis::X),
                    StageAxis::Custom(name) if name == "y" => axes.push(protocol::Axis::Y),
                    _ => {}
                }
            }
            axes
        } else if device == self.z {
            vec![protocol::Axis::Z]
        } else {
            Vec::new()
        }
    }

    fn apply_motion_profile(
        &mut self,
        device: DeviceId,
        request: &StageMoveRequest,
    ) -> Result<BTreeMap<String, Value>> {
        let mut changed = BTreeMap::new();
        let Some(profile) = &request.profile else {
            return Ok(changed);
        };
        let axes = self.profile_axes(device, request);
        if axes.is_empty() {
            return Ok(changed);
        }
        let (address, current_speed) = if device == self.xy {
            (
                self.card(protocol::TigerModuleKind::XyStage)?,
                self.xy_speed_um_s,
            )
        } else {
            (
                self.card(protocol::TigerModuleKind::ZStage)?,
                self.z_speed_um_s,
            )
        };
        let speed_um_s = profile
            .velocity
            .map(|value| value.micrometers_per_second())
            .unwrap_or(current_speed);
        if profile.velocity.is_some() {
            self.send(protocol::TigerCommand::Card {
                address,
                command: protocol::AsiCommand::Speed {
                    axes: axes
                        .iter()
                        .copied()
                        .map(|axis| (axis, speed_um_s))
                        .collect(),
                },
            })?;
            if device == self.xy {
                self.xy_speed_um_s = speed_um_s;
            } else {
                self.z_speed_um_s = speed_um_s;
            }
            changed.insert("velocity".into(), velocity(speed_um_s));
        }
        if let Some(acceleration) = profile.acceleration {
            let ramp_ms = (speed_um_s / acceleration.micrometers_per_second_squared() * 1000.0)
                .clamp(7.0, 10_000.0);
            self.send(protocol::TigerCommand::Card {
                address,
                command: protocol::AsiCommand::Accel {
                    axes: axes.iter().copied().map(|axis| (axis, ramp_ms)).collect(),
                },
            })?;
            if device == self.xy {
                self.xy_accel_ms = ramp_ms;
            } else {
                self.z_accel_ms = ramp_ms;
            }
            changed.insert(
                "accel_ramp_time".into(),
                Value::TimeInterval(TimeInterval::from_milliseconds(ramp_ms)),
            );
        }
        Ok(changed)
    }

    fn stage_move(&mut self, device: DeviceId, request: StageMoveRequest) -> Result<Value> {
        self.validate_stage_move(device, &request)?;
        let mut result = self.apply_motion_profile(device, &request)?;
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
                x = (self.x_um + x).clamp(0.0, self.probe.x_travel_um);
                y = (self.y_um + y).clamp(0.0, self.probe.y_travel_um);
                self.send(protocol::TigerCommand::Card {
                    address: self.card(protocol::TigerModuleKind::XyStage)?,
                    command: protocol::AsiCommand::MoveXyRel {
                        dx_um: x - self.x_um,
                        dy_um: y - self.y_um,
                    },
                })?;
            } else {
                x = x.clamp(0.0, self.probe.x_travel_um);
                y = y.clamp(0.0, self.probe.y_travel_um);
                self.send(protocol::TigerCommand::Card {
                    address: self.card(protocol::TigerModuleKind::XyStage)?,
                    command: protocol::AsiCommand::MoveXyAbs { x_um: x, y_um: y },
                })?;
            }
            self.x_um = x;
            self.y_um = y;
            self.finish_motion();
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
            let mut z = request
                .target
                .values()
                .next()
                .expect("validated one Z target")
                .micrometers();
            if request.relative {
                z = (self.z_um + z).clamp(0.0, self.probe.z_travel_um);
                self.send(protocol::TigerCommand::Card {
                    address: self.card(protocol::TigerModuleKind::ZStage)?,
                    command: protocol::AsiCommand::MoveZRel {
                        dz_um: z - self.z_um,
                    },
                })?;
            } else {
                z = z.clamp(0.0, self.probe.z_travel_um);
                self.send(protocol::TigerCommand::Card {
                    address: self.card(protocol::TigerModuleKind::ZStage)?,
                    command: protocol::AsiCommand::MoveZAbs { z_um: z },
                })?;
            }
            self.z_um = z;
            self.finish_motion();
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
                "ASI Tiger StageMove target device must be XY or Z stage",
            ))
        }
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
                "unknown ASI Tiger capability",
            ));
        };
        match (capability.kind, request) {
            (CapabilityKind::GenericCommand, CapabilityRequest::GenericCommand(request))
                if device == self.hub =>
            {
                self.apply_generic_command(device, request)
            }
            (CapabilityKind::GenericCommand, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "ASI Tiger GenericCommand expects a GenericCommandRequest",
            )),
            (CapabilityKind::StageMove, CapabilityRequest::StageMove(request))
                if device == self.xy || device == self.z =>
            {
                self.stage_move(device, request)
            }
            (CapabilityKind::StageMove, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "ASI Tiger StageMove expects a StageMoveRequest",
            )),
            (CapabilityKind::StageHome, CapabilityRequest::None) if device == self.xy => {
                let address = self.card(protocol::TigerModuleKind::XyStage)?;
                self.send(protocol::TigerCommand::Card {
                    address,
                    command: protocol::AsiCommand::Home {
                        axes: vec![protocol::Axis::X, protocol::Axis::Y],
                    },
                })?;
                self.read_optional_ack()?;
                self.x_um = 0.0;
                self.y_um = 0.0;
                self.finish_motion();
                self.emit_property(self.xy, "x", position(self.x_um));
                self.emit_property(self.xy, "y", position(self.y_um));
                self.refresh_motion_readback(address, vec![protocol::Axis::X, protocol::Axis::Y])?;
                Ok(Value::String("xy homed".into()))
            }
            (CapabilityKind::StageHome, CapabilityRequest::None) if device == self.z => {
                let address = self.card(protocol::TigerModuleKind::ZStage)?;
                self.send(protocol::TigerCommand::Card {
                    address,
                    command: protocol::AsiCommand::Home {
                        axes: vec![protocol::Axis::Z],
                    },
                })?;
                self.read_optional_ack()?;
                self.z_um = 0.0;
                self.finish_motion();
                self.emit_property(self.z, "z", position(self.z_um));
                self.refresh_motion_readback(address, vec![protocol::Axis::Z])?;
                Ok(Value::String("z homed".into()))
            }
            (CapabilityKind::StageStop, CapabilityRequest::None)
                if device == self.xy || device == self.z =>
            {
                let (address, axes) = if device == self.xy {
                    (
                        self.card(protocol::TigerModuleKind::XyStage)?,
                        vec![protocol::Axis::X, protocol::Axis::Y],
                    )
                } else {
                    (
                        self.card(protocol::TigerModuleKind::ZStage)?,
                        vec![protocol::Axis::Z],
                    )
                };
                self.send(protocol::TigerCommand::Card {
                    address,
                    command: protocol::AsiCommand::Halt,
                })?;
                self.read_optional_ack()?;
                self.busy = false;
                self.emit_property(self.xy, "busy", Value::Bool(false));
                self.emit_property(self.z, "busy", Value::Bool(false));
                self.refresh_motion_readback(address, axes)?;
                Ok(Value::String("halted".into()))
            }
            (CapabilityKind::TriggerSource, CapabilityRequest::None) if device == self.ttl => {
                self.ttl0 = true;
                self.send(protocol::TigerCommand::TtlOut {
                    address: self.card(protocol::TigerModuleKind::TtlIo)?,
                    line: 0,
                    high: true,
                })?;
                self.emit_property(self.ttl, "ttl0", Value::Bool(self.ttl0));
                Ok(Value::String("ttl pulse asserted".into()))
            }
            (CapabilityKind::TriggerSource, CapabilityRequest::Trigger(request))
                if device == self.ttl =>
            {
                self.invoke_ttl_trigger(request)
            }
            (CapabilityKind::PulseProgram, CapabilityRequest::None) if device == self.ring => self
                .invoke_ring_program(PulseProgramRequest {
                    interval: None,
                    duration: None,
                    count: None,
                    wait_for_input: None,
                }),
            (CapabilityKind::PulseProgram, CapabilityRequest::PulseProgram(request))
                if device == self.ring =>
            {
                self.invoke_ring_program(request)
            }
            (CapabilityKind::Autofocus, CapabilityRequest::None) if device == self.crisp => {
                Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "ASI CRISP autofocus requires an AutofocusRequest",
                ))
            }
            (_, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "ASI Tiger capability received an unsupported request",
            )),
        }
    }

    fn invoke_ttl_trigger(&mut self, request: TriggerRequest) -> Result<Value> {
        let address = self.card(protocol::TigerModuleKind::TtlIo)?;
        let mut commands = 0i64;
        let mut send_level = |driver: &mut Self, high: bool| -> Result<()> {
            driver.ttl0 = high;
            driver.send(protocol::TigerCommand::TtlOut {
                address,
                line: 0,
                high,
            })?;
            driver.emit_property(driver.ttl, "ttl0", Value::Bool(driver.ttl0));
            commands += 1;
            Ok(())
        };
        match request.action {
            TriggerAction::Enable => send_level(self, true)?,
            TriggerAction::Disable => send_level(self, false)?,
            TriggerAction::Pulse => {
                send_level(self, true)?;
                send_level(self, false)?;
            }
        }
        Ok(Value::Map(BTreeMap::from([
            (
                "action".into(),
                Value::String(match request.action {
                    TriggerAction::Enable => "enable".into(),
                    TriggerAction::Disable => "disable".into(),
                    TriggerAction::Pulse => "pulse".into(),
                }),
            ),
            ("ttl0".into(), Value::Bool(self.ttl0)),
            ("commands".into(), Value::I64(commands)),
        ])))
    }

    fn invoke_ring_program(&mut self, request: PulseProgramRequest) -> Result<Value> {
        if request.interval.is_some() || request.duration.is_some() {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "ASI Tiger ring-buffer direct PulseProgram does not expose interval/duration timing",
            ));
        }
        let address = self.card(protocol::TigerModuleKind::RingBuffer)?;
        if let Some(count) = request.count {
            self.ring_size = count.min(i64::MAX as u64) as i64;
        }
        if let Some(wait) = request.wait_for_input {
            self.ring_mode = if wait {
                "ttl".into()
            } else {
                "software".into()
            };
            self.send(protocol::TigerCommand::RingBufferMode {
                address,
                mode: self.ring_mode.clone(),
            })?;
        }
        self.ring_running = true;
        self.send(protocol::TigerCommand::RingBufferStart { address })?;
        self.emit_property(self.ring, "running", Value::Bool(self.ring_running));
        Ok(Value::Map(BTreeMap::from([
            ("mode".into(), Value::String(self.ring_mode.clone())),
            ("size".into(), Value::I64(self.ring_size)),
            ("running".into(), Value::Bool(self.ring_running)),
        ])))
    }

    fn invoke_crisp_autofocus(&mut self, request: AutofocusRequest) -> Result<Value> {
        match request.mode {
            AutofocusMode::SingleShot => {
                self.set_crisp_state(protocol::CrispState::Locking)?;
                self.crisp_state = protocol::CrispState::Locked;
                self.crisp_focus_score = 1.0;
                self.emit_crisp_status();
                self.set_crisp_state(protocol::CrispState::Ready)?;
                Ok(Value::Map(BTreeMap::from([
                    ("mode".into(), Value::String("single_shot".into())),
                    ("locked".into(), Value::Bool(true)),
                    ("focus_score".into(), Value::F64(self.crisp_focus_score)),
                ])))
            }
            AutofocusMode::Continuous | AutofocusMode::Hold => {
                self.set_crisp_state(protocol::CrispState::Locking)?;
                self.crisp_state = protocol::CrispState::Locked;
                self.crisp_focus_score = 1.0;
                self.emit_crisp_status();
                Ok(Value::Map(BTreeMap::from([
                    ("mode".into(), Value::String("continuous".into())),
                    ("locked".into(), Value::Bool(true)),
                    ("focus_score".into(), Value::F64(self.crisp_focus_score)),
                ])))
            }
            AutofocusMode::Stop => {
                self.set_crisp_state(protocol::CrispState::Ready)?;
                self.emit_crisp_status();
                Ok(Value::Map(BTreeMap::from([
                    ("mode".into(), Value::String("stop".into())),
                    ("locked".into(), Value::Bool(false)),
                ])))
            }
        }
    }

    fn emit_crisp_status(&mut self) {
        self.emit_property(
            self.crisp,
            "state",
            Value::String(self.crisp_state.label().into()),
        );
        self.emit_property(
            self.crisp,
            "continuous",
            Value::Bool(matches!(
                self.crisp_state,
                protocol::CrispState::Locking | protocol::CrispState::Locked
            )),
        );
        self.emit_property(
            self.crisp,
            "locked",
            Value::Bool(self.crisp_state == protocol::CrispState::Locked),
        );
        self.emit_property(
            self.crisp,
            "focus_score",
            Value::F64(self.crisp_focus_score),
        );
    }

    fn finish_motion(&mut self) {
        self.busy = true;
        self.pending
            .push_back(DriverEvent::Event(Event::Log(LogEvent {
                driver: Some(self.id),
                message: "asi tiger status :B".into(),
            })));
        self.busy = false;
        self.pending
            .push_back(DriverEvent::Event(Event::Log(LogEvent {
                driver: Some(self.id),
                message: "asi tiger status :A".into(),
            })));
    }

    fn refresh_motion_readback(&mut self, address: u8, axes: Vec<protocol::Axis>) -> Result<()> {
        self.refresh_readback(&protocol::TigerCommand::Card {
            address,
            command: protocol::AsiCommand::Status,
        })?;
        self.refresh_readback(&protocol::TigerCommand::Card {
            address,
            command: protocol::AsiCommand::Where { axes },
        })
    }

    fn refresh_identity(&mut self) -> Result<()> {
        let address = self.card(protocol::TigerModuleKind::XyStage)?;
        self.refresh_readback(&protocol::TigerCommand::Card {
            address,
            command: protocol::AsiCommand::Version,
        })?;
        self.refresh_readback(&protocol::TigerCommand::Card {
            address,
            command: protocol::AsiCommand::BuildName,
        })
    }

    fn refresh_status(&mut self) -> Result<()> {
        let addresses: Vec<u8> = self.probe.cards.iter().map(|card| card.address).collect();
        for address in addresses {
            self.refresh_readback(&protocol::TigerCommand::Card {
                address,
                command: protocol::AsiCommand::Status,
            })?;
        }
        Ok(())
    }

    fn refresh_position(&mut self) -> Result<()> {
        let xy = self.card(protocol::TigerModuleKind::XyStage)?;
        self.refresh_readback(&protocol::TigerCommand::Card {
            address: xy,
            command: protocol::AsiCommand::Where {
                axes: vec![protocol::Axis::X, protocol::Axis::Y],
            },
        })?;
        let z = self.card(protocol::TigerModuleKind::ZStage)?;
        self.refresh_readback(&protocol::TigerCommand::Card {
            address: z,
            command: protocol::AsiCommand::Where {
                axes: vec![protocol::Axis::Z],
            },
        })
    }

    fn refresh_crisp(&mut self) -> Result<()> {
        let address = self.card(protocol::TigerModuleKind::CrispAutofocus)?;
        self.refresh_readback(&protocol::TigerCommand::CrispQueryState { address })?;
        self.refresh_readback(&protocol::TigerCommand::CrispQueryFocusScore { address })?;
        self.refresh_readback(&protocol::TigerCommand::CrispQueryOffset { address })?;
        self.refresh_readback(&protocol::TigerCommand::CrispQueryObjectiveNa { address })?;
        self.refresh_readback(&protocol::TigerCommand::CrispQueryLockRange { address })?;
        self.refresh_readback(&protocol::TigerCommand::CrispQueryInFocusRange { address })
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

    fn tiger_timing_targets(&self, plan: &TimingPlan) -> (bool, bool, bool, bool, bool) {
        (
            plan.participants.contains(&self.xy),
            plan.participants.contains(&self.z),
            plan.participants.contains(&self.ttl),
            plan.participants.contains(&self.ring),
            plan.participants.contains(&self.crisp),
        )
    }

    fn validate_tiger_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        let (xy, z, ttl, ring, crisp) = self.tiger_timing_targets(plan);
        if !xy && !z && !ttl && !ring && !crisp {
            return Ok(());
        }

        for sequence in &plan.sequences {
            if sequence.device == self.xy {
                if !matches!(sequence.property.as_str(), "x" | "y") {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "ASI Tiger XY timing sequences can only target x or y",
                    ));
                }
            } else if sequence.device == self.z {
                if sequence.property != "z" {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "ASI Tiger Z timing sequences can only target z",
                    ));
                }
            } else if sequence.device == self.ttl {
                if sequence.property != "ttl0" {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "ASI Tiger TTL timing sequences can only target ttl0",
                    ));
                }
            } else if sequence.device == self.ring {
                if sequence.property != "running" {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "ASI Tiger ring-buffer timing sequences can only target running",
                    ));
                }
            } else if sequence.device == self.crisp {
                if sequence.property != "state" {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "ASI Tiger CRISP timing sequences can only target state",
                    ));
                }
            } else {
                continue;
            }

            for value in &sequence.values {
                if sequence.device == self.xy || sequence.device == self.z {
                    let _ = position_um(value)?;
                } else if sequence.device == self.crisp {
                    self.validate_write(sequence.device, &sequence.property, value)?;
                } else if !matches!(value, Value::Bool(_)) {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "ASI Tiger timing sequences require bool values",
                    ));
                }
            }
        }
        Ok(())
    }

    fn tiger_timing_summary(&self, plan: &TimingPlan, phase: &str) -> Value {
        let (xy, z, ttl, ring, crisp) = self.tiger_timing_targets(plan);
        Value::Map(BTreeMap::from([
            ("phase".into(), Value::String(phase.into())),
            ("xy_participant".into(), Value::Bool(xy)),
            ("z_participant".into(), Value::Bool(z)),
            ("ttl_participant".into(), Value::Bool(ttl)),
            ("ring_participant".into(), Value::Bool(ring)),
            ("crisp_participant".into(), Value::Bool(crisp)),
            ("x".into(), position(self.x_um)),
            ("y".into(), position(self.y_um)),
            ("z".into(), position(self.z_um)),
            ("ttl0".into(), Value::Bool(self.ttl0)),
            ("ring_mode".into(), Value::String(self.ring_mode.clone())),
            ("ring_size".into(), Value::I64(self.ring_size)),
            ("ring_running".into(), Value::Bool(self.ring_running)),
            (
                "crisp_state".into(),
                Value::String(self.crisp_state.label().into()),
            ),
            (
                "crisp_continuous".into(),
                Value::Bool(matches!(
                    self.crisp_state,
                    protocol::CrispState::Locking | protocol::CrispState::Locked
                )),
            ),
            (
                "sequences".into(),
                Value::I64(
                    plan.sequences
                        .iter()
                        .filter(|sequence| {
                            sequence.device == self.xy
                                || sequence.device == self.z
                                || sequence.device == self.ttl
                                || sequence.device == self.ring
                                || sequence.device == self.crisp
                        })
                        .count() as i64,
                ),
            ),
        ]))
    }

    fn timing_sequence_value(sequence: &DeviceSequence, first: bool) -> Option<Value> {
        if first {
            sequence.values.first()
        } else {
            sequence.values.last()
        }
        .cloned()
    }

    fn apply_tiger_timing_transition(&mut self, plan: &TimingPlan, start: bool) -> Result<Value> {
        let (_, _, ttl, ring, _) = self.tiger_timing_targets(plan);
        let mut writes = Vec::new();
        let mut ttl_sequence = false;
        let mut ring_sequence = false;

        for sequence in &plan.sequences {
            if sequence.device == self.xy && matches!(sequence.property.as_str(), "x" | "y") {
                if let Some(value) = Self::timing_sequence_value(sequence, start) {
                    writes.push(StateWrite {
                        device: self.xy,
                        property: sequence.property.clone(),
                        value,
                    });
                }
            } else if sequence.device == self.z && sequence.property == "z" {
                if let Some(value) = Self::timing_sequence_value(sequence, start) {
                    writes.push(StateWrite {
                        device: self.z,
                        property: "z".into(),
                        value,
                    });
                }
            } else if sequence.device == self.ttl && sequence.property == "ttl0" {
                ttl_sequence = true;
                if let Some(value) = Self::timing_sequence_value(sequence, start) {
                    writes.push(StateWrite {
                        device: self.ttl,
                        property: "ttl0".into(),
                        value,
                    });
                }
            } else if sequence.device == self.ring && sequence.property == "running" {
                ring_sequence = true;
                if let Some(value) = Self::timing_sequence_value(sequence, start) {
                    writes.push(StateWrite {
                        device: self.ring,
                        property: "running".into(),
                        value,
                    });
                }
            } else if sequence.device == self.crisp && sequence.property == "state" {
                if let Some(value) = Self::timing_sequence_value(sequence, start) {
                    writes.push(StateWrite {
                        device: self.crisp,
                        property: "state".into(),
                        value,
                    });
                }
            }
        }

        if ttl && !ttl_sequence {
            writes.push(StateWrite {
                device: self.ttl,
                property: "ttl0".into(),
                value: Value::Bool(start),
            });
        }
        if ring && !ring_sequence {
            writes.push(StateWrite {
                device: self.ring,
                property: "running".into(),
                value: Value::Bool(start),
            });
        }
        if writes.is_empty() {
            return Ok(Value::Map(BTreeMap::new()));
        }
        self.apply_state_set(StateSet {
            name: Some(if start {
                "asi tiger timing start".into()
            } else {
                "asi tiger timing stop".into()
            }),
            writes,
            commit: CommitMode::Immediate,
        })
    }
}

impl Driver for AsiTigerDriver {
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
            label: "asi-tiger-serial".into(),
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
                ("terminator".into(), Value::String("CR".into())),
                (
                    "completion".into(),
                    Value::String("card-addressed idle-busy handling".into()),
                ),
                (
                    "support_scope".into(),
                    Value::String("Tiger motion/TTL/ring-buffer/CRISP command helpers".into()),
                ),
                (
                    "startup_readback_supported".into(),
                    Value::List(
                        protocol::tiger_probe_script(&self.probe)
                            .into_iter()
                            .map(Value::String)
                            .collect(),
                    ),
                ),
                ("cards".into(), self.card_metadata()),
            ]),
        }]
    }

    fn graph(&self) -> DeviceGraph {
        AsiTigerDriver::graph(self)
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.xy || device == self.z {
            vec![
                capability(1, device, CapabilityKind::StageMove),
                capability(2, device, CapabilityKind::StageHome),
                capability(3, device, CapabilityKind::StageStop),
            ]
        } else if device == self.hub {
            vec![capability(7, device, CapabilityKind::GenericCommand)]
        } else if device == self.ttl {
            vec![capability(4, device, CapabilityKind::TriggerSource)]
        } else if device == self.ring {
            vec![capability(6, device, CapabilityKind::PulseProgram)]
        } else if device == self.crisp {
            vec![capability(5, device, CapabilityKind::Autofocus)]
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
                        description: format!("asi tiger read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("asi tiger write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "asi tiger remultiplexed card state set".into(),
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
                    let requested = self
                        .capabilities(*device)
                        .into_iter()
                        .find(|candidate| candidate.id == *capability)
                        .ok_or_else(|| {
                            Error::new(ErrorCode::Unsupported, "unknown ASI Tiger capability")
                        })?;
                    if requested.kind == CapabilityKind::Autofocus {
                        if !matches!(request, CapabilityRequest::Autofocus(_)) {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "ASI CRISP autofocus expects an AutofocusRequest",
                            ));
                        }
                    } else if requested.kind == CapabilityKind::GenericCommand {
                        if let CapabilityRequest::GenericCommand(request) = request {
                            self.validate_generic_command(*device, request)?;
                        } else {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "ASI Tiger GenericCommand expects a GenericCommandRequest",
                            ));
                        }
                    } else if requested.kind == CapabilityKind::StageMove {
                        if let CapabilityRequest::StageMove(request) = request {
                            self.validate_stage_move(*device, request)?;
                        } else {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "ASI Tiger StageMove expects a StageMoveRequest",
                            ));
                        }
                    } else if requested.kind == CapabilityKind::TriggerSource {
                        if !matches!(
                            request,
                            CapabilityRequest::None | CapabilityRequest::Trigger(_)
                        ) {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "ASI Tiger TriggerSource expects None or TriggerRequest",
                            ));
                        }
                    } else if requested.kind == CapabilityKind::PulseProgram {
                        match request {
                            CapabilityRequest::None => {}
                            CapabilityRequest::PulseProgram(request) => {
                                if request.interval.is_some() || request.duration.is_some() {
                                    return Err(Error::new(
                                        ErrorCode::Unsupported,
                                        "ASI Tiger ring-buffer PulseProgram does not expose interval/duration timing",
                                    ));
                                }
                            }
                            _ => {
                                return Err(Error::new(
                                    ErrorCode::Unsupported,
                                    "ASI Tiger PulseProgram expects None or PulseProgramRequest",
                                ));
                            }
                        }
                    } else if !matches!(request, CapabilityRequest::None) {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "ASI Tiger fixture capabilities take no request",
                        ));
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("asi tiger invoke {}", requested.kind.name()),
                        payload: Value::Null,
                    });
                }
                Command::Arm(plan) => self.validate_tiger_timing_plan(plan)?,
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
                    let readback = if device == self.xy && (key == "x" || key == "y") {
                        Some(protocol::TigerCommand::Card {
                            address: self.card(protocol::TigerModuleKind::XyStage)?,
                            command: protocol::AsiCommand::Where {
                                axes: vec![protocol::Axis::X, protocol::Axis::Y],
                            },
                        })
                    } else if device == self.z && key == "z" {
                        Some(protocol::TigerCommand::Card {
                            address: self.card(protocol::TigerModuleKind::ZStage)?,
                            command: protocol::AsiCommand::Where {
                                axes: vec![protocol::Axis::Z],
                            },
                        })
                    } else if key == "busy" {
                        Some(protocol::TigerCommand::Card {
                            address: self.card(protocol::TigerModuleKind::XyStage)?,
                            command: protocol::AsiCommand::Status,
                        })
                    } else if device == self.hub && key == "firmware_version" {
                        Some(protocol::TigerCommand::Card {
                            address: self.card(protocol::TigerModuleKind::XyStage)?,
                            command: protocol::AsiCommand::Version,
                        })
                    } else if device == self.hub && key == "build_name" {
                        Some(protocol::TigerCommand::Card {
                            address: self.card(protocol::TigerModuleKind::XyStage)?,
                            command: protocol::AsiCommand::BuildName,
                        })
                    } else if device == self.crisp {
                        let address = self.card(protocol::TigerModuleKind::CrispAutofocus)?;
                        match key.as_str() {
                            "state" | "continuous" | "locked" => {
                                Some(protocol::TigerCommand::CrispQueryState { address })
                            }
                            "focus_score" => {
                                Some(protocol::TigerCommand::CrispQueryFocusScore { address })
                            }
                            "offset" => Some(protocol::TigerCommand::CrispQueryOffset { address }),
                            "objective_na" => {
                                Some(protocol::TigerCommand::CrispQueryObjectiveNa { address })
                            }
                            "lock_range" => {
                                Some(protocol::TigerCommand::CrispQueryLockRange { address })
                            }
                            "in_focus_range" => {
                                Some(protocol::TigerCommand::CrispQueryInFocusRange { address })
                            }
                            _ => None,
                        }
                    } else {
                        None
                    };
                    if let Some(command) = readback {
                        self.refresh_readback(&command)?;
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
                    request: CapabilityRequest::None,
                } => {
                    last = self.invoke(device, capability, CapabilityRequest::None)?;
                }
                Command::Invoke {
                    device,
                    capability,
                    request: CapabilityRequest::StageMove(request),
                } => {
                    last =
                        self.invoke(device, capability, CapabilityRequest::StageMove(request))?;
                }
                Command::Invoke {
                    device,
                    capability,
                    request: CapabilityRequest::Autofocus(request),
                } if device == self.crisp => {
                    if self
                        .capabilities(device)
                        .iter()
                        .all(|candidate| candidate.id != capability)
                    {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "unknown ASI CRISP autofocus capability",
                        ));
                    }
                    last = self.invoke_crisp_autofocus(request)?;
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
                        message: format!("asi tiger serial: {line}"),
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
        self.validate_tiger_timing_plan(plan)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Arm(plan.clone())],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "asi tiger timing arm summary".into(),
                payload: self.tiger_timing_summary(plan, "arm"),
            }],
        })
    }

    fn start_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let changed = self.apply_tiger_timing_transition(&armed.plan, true)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Start(OperationId(0))],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "asi tiger timing start".into(),
                payload: Value::Map(BTreeMap::from([
                    (
                        "summary".into(),
                        self.tiger_timing_summary(&armed.plan, "start"),
                    ),
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
        let changed = self.apply_tiger_timing_transition(&armed.plan, false)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "asi tiger timing stop".into(),
                payload: Value::Map(BTreeMap::from([
                    (
                        "summary".into(),
                        self.tiger_timing_summary(&armed.plan, "stop"),
                    ),
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

fn numerical_aperture_property_range(
    key: &str,
    display_name: &str,
    writable: bool,
    min: f64,
    max: f64,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::NumericalAperture,
        None,
        writable,
        Some(Range {
            min: Value::NumericalAperture(NumericalAperture::new(min)),
            max: Value::NumericalAperture(NumericalAperture::new(max)),
        }),
    )
}

fn position_property(key: &str, display_name: &str, writable: bool, max_um: f64) -> PropertySchema {
    position_property_range(key, display_name, writable, 0.0, max_um)
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

fn position_property_range(
    key: &str,
    display_name: &str,
    writable: bool,
    min_um: f64,
    max_um: f64,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Position,
        Some("um"),
        writable,
        Some(Range {
            min: position(min_um),
            max: position(max_um),
        }),
    )
}

fn position(value_um: f64) -> Value {
    Value::Position(Position::from_micrometers(value_um))
}

fn velocity(value_um_s: f64) -> Value {
    Value::Velocity(Velocity::from_micrometers_per_second(value_um_s))
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

fn asi_refresh_result(command: &str, completion_basis: &str) -> Value {
    Value::Map(BTreeMap::from([
        ("command".into(), Value::String(command.into())),
        (
            "completion_basis".into(),
            Value::String(completion_basis.into()),
        ),
    ]))
}

fn command_no_params(prefix: &str, request: &GenericCommandRequest) -> Result<()> {
    if !request.params.is_empty() {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            format!(
                "{prefix} {} command does not accept params",
                request.command
            ),
        ));
    }
    Ok(())
}

fn asi_endpoint_from_config(device: &DeviceConfig) -> Option<AsiSerialEndpoint> {
    string_prop(device, "serial_port").map(|port_name| AsiSerialEndpoint {
        port_name,
        baud_rate: u32_prop(device, "baud_rate").unwrap_or(protocol::BAUD),
        timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(1),
    })
}

fn string_prop(device: &DeviceConfig, key: &str) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
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

fn bool_prop(device: &DeviceConfig, key: &str) -> Option<bool> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn u32_prop(device: &DeviceConfig, key: &str) -> Option<u32> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => (*value).try_into().ok(),
        _ => None,
    }
}

fn u64_prop(device: &DeviceConfig, key: &str) -> Option<u64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => (*value).try_into().ok(),
        _ => None,
    }
}

fn crisp_state_property() -> PropertySchema {
    let mut schema = property("state", "CRISP state", ValueType::String, None, true, None);
    schema.sequenceable = true;
    schema.enum_values = [
        protocol::CrispState::Idle,
        protocol::CrispState::Ready,
        protocol::CrispState::Locking,
        protocol::CrispState::Locked,
        protocol::CrispState::Error,
    ]
    .into_iter()
    .map(|state| EnumValue {
        value: Value::String(state.label().into()),
        label: state.label().into(),
    })
    .collect();
    schema
}
