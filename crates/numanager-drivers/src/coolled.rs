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
    pub const RECV_ENDING: LineEnding = LineEnding::Lf;
    pub const CHANNELS: usize = 4;
    pub const PE300_CHANNELS: usize = 3;
    pub const WAVELENGTHS_NM: [[f64; 4]; CHANNELS] = [
        [365.0, 385.0, 405.0, 435.0],
        [460.0, 470.0, 490.0, 500.0],
        [525.0, 550.0, 580.0, 595.0],
        [635.0, 660.0, 740.0, 770.0],
    ];

    #[derive(Debug, Clone, PartialEq)]
    pub struct CoolLedPe4000Probe {
        pub model: String,
        pub version: String,
        pub device_prefix: String,
        pub wavelengths: [[f64; 4]; CHANNELS],
    }

    impl CoolLedPe4000Probe {
        pub fn simulated() -> Self {
            Self {
                model: "CoolLED pE-4000".into(),
                version: "numanager-sim".into(),
                device_prefix: "coolled-pe4000".into(),
                wavelengths: WAVELENGTHS_NM,
            }
        }

        pub fn simulated_pe340() -> Self {
            Self {
                model: "CoolLED pE-340".into(),
                version: "numanager-sim-pe340".into(),
                device_prefix: "coolled-pe340".into(),
                wavelengths: WAVELENGTHS_NM,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct CoolLedPe4000ProbeResult {
        pub probe: CoolLedPe4000Probe,
        pub status: String,
        pub lamp_summary: String,
        pub channels: Vec<CoolLedChannelProbe>,
        pub global_enabled: Option<bool>,
        pub replies: Vec<(String, String)>,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct CoolLedChannelProbe {
        pub channel: Channel,
        pub selected: bool,
        pub intensity_percent: u8,
        pub wavelength: Option<Wavelength>,
        pub raw: String,
    }

    impl CoolLedPe4000ProbeResult {
        pub fn from_replies(replies: &[(impl AsRef<str>, impl AsRef<str>)]) -> Result<Self> {
            let mut probe = CoolLedPe4000Probe::simulated();
            let mut status = String::new();
            let mut lamp_summary = String::new();
            let mut channels = Vec::new();
            let mut global_enabled = None;
            let mut stored = Vec::new();

            for (command, reply) in replies {
                let command = command.as_ref();
                let reply = reply.as_ref().trim();
                stored.push((command.to_string(), reply.to_string()));
                match command {
                    "XMODEL" => {
                        probe.model = reply.to_string();
                        probe.device_prefix = if reply.to_ascii_lowercase().contains("340") {
                            "coolled-pe340".into()
                        } else {
                            "coolled-pe4000".into()
                        };
                    }
                    "XVER" => probe.version = reply.to_string(),
                    "CSS?" => {
                        status = reply.to_string();
                        global_enabled = parse_global_enabled(reply);
                    }
                    "LAMS" => {
                        lamp_summary = reply.to_string();
                        if let Some(wavelengths) = parse_lamp_summary(reply) {
                            probe.wavelengths = wavelengths;
                        }
                    }
                    "CA?" | "CB?" | "CC?" | "CD?" => {
                        let channel = match command.as_bytes()[1] as char {
                            'A' => Channel::A,
                            'B' => Channel::B,
                            'C' => Channel::C,
                            'D' => Channel::D,
                            _ => continue,
                        };
                        channels.push(parse_channel_probe(channel, reply)?);
                    }
                    _ => {}
                }
            }

            Ok(Self {
                probe,
                status,
                lamp_summary,
                channels,
                global_enabled,
                replies: stored,
            })
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct CoolLedPe300Probe {
        pub model: String,
        pub mainboard_version: String,
        pub pod_version: String,
        pub channel_labels: [String; PE300_CHANNELS],
    }

    impl CoolLedPe300Probe {
        pub fn simulated() -> Self {
            Self {
                model: "CoolLED pE-300".into(),
                mainboard_version: "numanager-mainboard".into(),
                pod_version: "numanager-pod".into(),
                channel_labels: ["A".into(), "B".into(), "C".into()],
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct CoolLedPe300ProbeResult {
        pub probe: CoolLedPe300Probe,
        pub status: String,
        pub channels: Vec<CoolLedChannelProbe>,
        pub global_enabled: Option<bool>,
        pub replies: Vec<(String, String)>,
    }

    impl CoolLedPe300ProbeResult {
        pub fn from_replies(replies: &[(impl AsRef<str>, impl AsRef<str>)]) -> Result<Self> {
            let mut probe = CoolLedPe300Probe::simulated();
            let mut status = String::new();
            let mut channels = Vec::new();
            let mut global_enabled = None;
            let mut stored = Vec::new();

            for (command, reply) in replies {
                let command = command.as_ref();
                let reply = reply.as_ref().trim();
                stored.push((command.to_string(), reply.to_string()));
                match command {
                    "XMODEL" => probe.model = reply.to_string(),
                    "XVER" => {
                        probe.mainboard_version = reply.to_string();
                        if let Some(pod) = parse_pe300_pod_version(reply) {
                            probe.pod_version = pod;
                        }
                    }
                    "CSS?" => {
                        status = reply.to_string();
                        global_enabled = parse_global_enabled(reply);
                    }
                    "CA?" | "CB?" | "CC?" => {
                        let channel = match command.as_bytes()[1] as char {
                            'A' => Channel::A,
                            'B' => Channel::B,
                            'C' => Channel::C,
                            _ => continue,
                        };
                        channels.push(parse_channel_probe(channel, reply)?);
                    }
                    _ => {}
                }
            }

            Ok(Self {
                probe,
                status,
                channels,
                global_enabled,
                replies: stored,
            })
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Channel {
        A,
        B,
        C,
        D,
    }

    impl Channel {
        pub fn index(self) -> usize {
            match self {
                Channel::A => 0,
                Channel::B => 1,
                Channel::C => 2,
                Channel::D => 3,
            }
        }

        pub fn letter(self) -> char {
            match self {
                Channel::A => 'A',
                Channel::B => 'B',
                Channel::C => 'C',
                Channel::D => 'D',
            }
        }

        pub fn from_index(index: usize) -> Option<Self> {
            match index {
                0 => Some(Channel::A),
                1 => Some(Channel::B),
                2 => Some(Channel::C),
                3 => Some(Channel::D),
                _ => None,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum CoolLedCommand {
        Model,
        Version,
        Status,
        LampSummary,
        LoadWavelength(Wavelength),
        SetChannelSelected { channel: Channel, selected: bool },
        SetIntensity { channel: Channel, percent: u8 },
        QueryChannel(Channel),
        SetGlobal(bool),
        SetPodLocked(bool),
    }

    pub fn encode(command: &CoolLedCommand) -> String {
        match command {
            CoolLedCommand::Model => "XMODEL".into(),
            CoolLedCommand::Version => "XVER".into(),
            CoolLedCommand::Status => "CSS?".into(),
            CoolLedCommand::LampSummary => "LAMS".into(),
            CoolLedCommand::LoadWavelength(wavelength) => {
                format!("LOAD:{:.0}", wavelength.nanometers())
            }
            CoolLedCommand::SetChannelSelected { channel, selected } => {
                format!("C{}{}", channel.letter(), if *selected { "S" } else { "X" })
            }
            CoolLedCommand::SetIntensity { channel, percent } => {
                format!("C{}I{}", channel.letter(), (*percent).min(100))
            }
            CoolLedCommand::QueryChannel(channel) => format!("C{}?", channel.letter()),
            CoolLedCommand::SetGlobal(enabled) => {
                if *enabled {
                    "CSN".into()
                } else {
                    "CSF".into()
                }
            }
            CoolLedCommand::SetPodLocked(locked) => {
                if *locked {
                    "PORT:P=OFF".into()
                } else {
                    "PORT:P=ON".into()
                }
            }
        }
    }

    pub fn pe4000_probe_commands() -> Vec<CoolLedCommand> {
        vec![
            CoolLedCommand::Model,
            CoolLedCommand::Version,
            CoolLedCommand::Status,
            CoolLedCommand::LampSummary,
            CoolLedCommand::QueryChannel(Channel::A),
            CoolLedCommand::QueryChannel(Channel::B),
            CoolLedCommand::QueryChannel(Channel::C),
            CoolLedCommand::QueryChannel(Channel::D),
        ]
    }

    pub fn pe300_probe_commands() -> Vec<CoolLedCommand> {
        vec![
            CoolLedCommand::Model,
            CoolLedCommand::Version,
            CoolLedCommand::Status,
            CoolLedCommand::QueryChannel(Channel::A),
            CoolLedCommand::QueryChannel(Channel::B),
            CoolLedCommand::QueryChannel(Channel::C),
        ]
    }

    pub fn pe4000_probe_script() -> Vec<String> {
        pe4000_probe_commands().iter().map(encode).collect()
    }

    pub fn pe300_probe_script() -> Vec<String> {
        pe300_probe_commands().iter().map(encode).collect()
    }

    pub fn execute_pe4000_probe_script(
        serial: &mut dyn SerialIo,
        polls_per_command: usize,
    ) -> Result<CoolLedPe4000ProbeResult> {
        let mut codec = SerialLineCodec::new(SEND_ENDING, RECV_ENDING);
        let mut replies = Vec::new();
        for command in pe4000_probe_commands() {
            let encoded = encode(&command);
            serial.write(&codec.encode(&encoded))?;
            replies.push((encoded, read_line(serial, &mut codec, polls_per_command)?));
        }
        CoolLedPe4000ProbeResult::from_replies(&replies)
    }

    pub fn execute_pe300_probe_script(
        serial: &mut dyn SerialIo,
        polls_per_command: usize,
    ) -> Result<CoolLedPe300ProbeResult> {
        let mut codec = SerialLineCodec::new(SEND_ENDING, RECV_ENDING);
        let mut replies = Vec::new();
        for command in pe300_probe_commands() {
            let encoded = encode(&command);
            serial.write(&codec.encode(&encoded))?;
            replies.push((encoded, read_line(serial, &mut codec, polls_per_command)?));
        }
        CoolLedPe300ProbeResult::from_replies(&replies)
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
            "timed out waiting for CoolLED probe reply",
        ))
    }

    pub(crate) fn parse_global_enabled(reply: &str) -> Option<bool> {
        let lower = reply.to_ascii_lowercase();
        if lower.contains("on") || lower.contains("enabled") || lower.contains("global=1") {
            Some(true)
        } else if lower.contains("off") || lower.contains("disabled") || lower.contains("global=0")
        {
            Some(false)
        } else {
            None
        }
    }

    pub(crate) fn parse_lamp_summary(reply: &str) -> Option<[[f64; 4]; CHANNELS]> {
        let values = numbers(reply);
        if values.len() < CHANNELS * 4 {
            return None;
        }
        let mut wavelengths = [[0.0; 4]; CHANNELS];
        for channel in 0..CHANNELS {
            for slot in 0..4 {
                wavelengths[channel][slot] = values[channel * 4 + slot];
            }
        }
        Some(wavelengths)
    }

    pub(crate) fn parse_channel_probe(
        channel: Channel,
        reply: &str,
    ) -> Result<CoolLedChannelProbe> {
        let values = numbers(reply);
        let lower = reply.to_ascii_lowercase();
        let selected = keyed_number(&lower, "selected")
            .or_else(|| keyed_number(&lower, "enabled"))
            .map(|value| value != 0.0)
            .unwrap_or_else(|| lower.contains(" on") || lower.ends_with("on"));
        let intensity_percent = keyed_number(&lower, "intensity")
            .or_else(|| keyed_number(&lower, "i"))
            .or_else(|| {
                values
                    .iter()
                    .copied()
                    .find(|value| (0.0..=100.0).contains(value))
            })
            .unwrap_or(0.0)
            .round()
            .clamp(0.0, 100.0) as u8;
        let wavelength = keyed_number(&lower, "wavelength")
            .or_else(|| keyed_number(&lower, "wl"))
            .or_else(|| {
                values
                    .iter()
                    .copied()
                    .find(|value| (300.0..=800.0).contains(value))
            })
            .map(Wavelength::from_nanometers);
        Ok(CoolLedChannelProbe {
            channel,
            selected,
            intensity_percent,
            wavelength,
            raw: reply.to_string(),
        })
    }

    fn numbers(reply: &str) -> Vec<f64> {
        reply
            .split(|ch: char| !(ch == '-' || ch == '+' || ch == '.' || ch.is_ascii_digit()))
            .filter(|token| !token.is_empty() && *token != "+" && *token != "-")
            .filter_map(|token| token.parse::<f64>().ok())
            .collect()
    }

    fn keyed_number(reply: &str, key: &str) -> Option<f64> {
        let start = reply.find(key)?;
        let tail = &reply[start + key.len()..];
        let tail =
            tail.trim_start_matches(|ch: char| ch == '=' || ch == ':' || ch == ' ' || ch == '\t');
        tail.split(|ch: char| !(ch == '-' || ch == '+' || ch == '.' || ch.is_ascii_digit()))
            .find(|token| !token.is_empty() && *token != "+" && *token != "-")?
            .parse()
            .ok()
    }

    pub(crate) fn parse_pe300_pod_version(reply: &str) -> Option<String> {
        let lower = reply.to_ascii_lowercase();
        let start = lower.find("pod")?;
        let tail = reply[start + 3..]
            .trim_start_matches(|ch: char| ch == '=' || ch == ':' || ch == ' ' || ch == '\t');
        let token = tail
            .split([',', ';', '\r', '\n'])
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        Some(token.to_string())
    }
}

pub struct CoolLedPe300Discovery {
    next_id: DriverId,
    probes: Vec<CoolLedPe300ConfiguredProbe>,
}

impl CoolLedPe300Discovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![CoolLedPe300ConfiguredProbe::simulated()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| device.driver == "coolled-pe300")
            .map(CoolLedPe300ConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for CoolLedPe300Discovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver = if configured.connect_real_transport {
                    Box::new(CoolLedPe300Driver::serial(id, configured)?) as Box<dyn Driver>
                } else {
                    Box::new(CoolLedPe300Driver::configured_fixture(id, configured))
                        as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

pub struct CoolLedPe4000Discovery {
    next_id: DriverId,
    probes: Vec<CoolLedPe4000ConfiguredProbe>,
}

impl CoolLedPe4000Discovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![CoolLedPe4000ConfiguredProbe::simulated_pe4000()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| device.driver == "coolled-pe4000")
            .map(|device| CoolLedPe4000ConfiguredProbe::from_device_config(device, false))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for CoolLedPe4000Discovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver = if configured.connect_real_transport {
                    Box::new(CoolLedPe4000Driver::serial(id, configured)?) as Box<dyn Driver>
                } else {
                    Box::new(CoolLedPe4000Driver::configured_fixture(id, configured))
                        as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

pub struct CoolLedPe340Discovery {
    next_id: DriverId,
    probes: Vec<CoolLedPe4000ConfiguredProbe>,
}

impl CoolLedPe340Discovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![CoolLedPe4000ConfiguredProbe::simulated_pe340()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| device.driver == "coolled-pe340")
            .map(|device| CoolLedPe4000ConfiguredProbe::from_device_config(device, true))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for CoolLedPe340Discovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver = if configured.connect_real_transport {
                    Box::new(CoolLedPe4000Driver::serial(id, configured)?) as Box<dyn Driver>
                } else {
                    Box::new(CoolLedPe4000Driver::configured_fixture(id, configured))
                        as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct CoolLedSerialEndpoint {
    pub port_name: String,
    pub baud_rate: u32,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone)]
pub struct CoolLedPe300ConfiguredProbe {
    pub label: String,
    pub probe: protocol::CoolLedPe300Probe,
    pub endpoint: Option<CoolLedSerialEndpoint>,
    pub connect_real_transport: bool,
}

impl CoolLedPe300ConfiguredProbe {
    pub fn simulated() -> Self {
        Self {
            label: "Simulated CoolLED pE-300".into(),
            probe: protocol::CoolLedPe300Probe::simulated(),
            endpoint: None,
            connect_real_transport: false,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::simulated();
        configured.label = if device.label.is_empty() {
            "Configured CoolLED pE-300".into()
        } else {
            device.label.clone()
        };
        configured.probe.model =
            string_prop(device, "model").unwrap_or_else(|| configured.probe.model.clone());
        configured.probe.mainboard_version = string_prop(device, "mainboard_version")
            .unwrap_or_else(|| configured.probe.mainboard_version.clone());
        configured.probe.pod_version = string_prop(device, "pod_version")
            .unwrap_or_else(|| configured.probe.pod_version.clone());
        if let Some(labels) = string_prop(device, "channel_labels") {
            configured.probe.channel_labels = parse_pe300_channel_labels(&labels)?;
        }
        configured.endpoint =
            string_prop(device, "serial_port").map(|port_name| CoolLedSerialEndpoint {
                port_name,
                baud_rate: u32_prop(device, "baud_rate").unwrap_or(9_600),
                timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(100),
            });
        configured.connect_real_transport = bool_prop(device, "connect").unwrap_or(false);
        Ok(configured)
    }
}

#[derive(Debug, Clone)]
pub struct CoolLedPe4000ConfiguredProbe {
    pub label: String,
    pub probe: protocol::CoolLedPe4000Probe,
    pub endpoint: Option<CoolLedSerialEndpoint>,
    pub connect_real_transport: bool,
}

impl CoolLedPe4000ConfiguredProbe {
    pub fn simulated_pe4000() -> Self {
        Self {
            label: "Simulated CoolLED pE-4000".into(),
            probe: protocol::CoolLedPe4000Probe::simulated(),
            endpoint: None,
            connect_real_transport: false,
        }
    }

    pub fn simulated_pe340() -> Self {
        Self {
            label: "Simulated CoolLED pE-340".into(),
            probe: protocol::CoolLedPe4000Probe::simulated_pe340(),
            endpoint: None,
            connect_real_transport: false,
        }
    }

    fn from_device_config(device: &DeviceConfig, pe340: bool) -> Result<Self> {
        let mut configured = if pe340 {
            Self::simulated_pe340()
        } else {
            Self::simulated_pe4000()
        };
        configured.label = if device.label.is_empty() {
            if pe340 {
                "Configured CoolLED pE-340".into()
            } else {
                "Configured CoolLED pE-4000".into()
            }
        } else {
            device.label.clone()
        };
        configured.probe.model =
            string_prop(device, "model").unwrap_or_else(|| configured.probe.model.clone());
        configured.probe.version =
            string_prop(device, "version").unwrap_or_else(|| configured.probe.version.clone());
        configured.probe.device_prefix = string_prop(device, "device_prefix")
            .unwrap_or_else(|| configured.probe.device_prefix.clone());
        if let Some(wavelengths) = string_prop(device, "wavelengths_nm") {
            configured.probe.wavelengths = parse_wavelength_table(&wavelengths)?;
        }
        configured.endpoint =
            string_prop(device, "serial_port").map(|port_name| CoolLedSerialEndpoint {
                port_name,
                baud_rate: u32_prop(device, "baud_rate").unwrap_or(9_600),
                timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(100),
            });
        configured.connect_real_transport = bool_prop(device, "connect").unwrap_or(false);
        Ok(configured)
    }
}

#[derive(Debug, Clone)]
struct ChannelState {
    selected: bool,
    intensity_percent: u8,
    wavelength: Wavelength,
    enabled: bool,
}

#[derive(Debug, Clone)]
struct Pe300ChannelState {
    selected: bool,
    intensity_percent: u8,
}

#[derive(Debug, Default, Clone)]
struct TimingExecutionState {
    armed: bool,
    running: bool,
    routed_channels: usize,
    route_count: usize,
    sequence_count: usize,
    starts: u64,
    stops: u64,
}

impl TimingExecutionState {
    fn value(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("armed".into(), Value::Bool(self.armed)),
            ("running".into(), Value::Bool(self.running)),
            (
                "routed_channels".into(),
                Value::I64(self.routed_channels as i64),
            ),
            ("route_count".into(), Value::I64(self.route_count as i64)),
            (
                "sequence_count".into(),
                Value::I64(self.sequence_count as i64),
            ),
            ("starts".into(), Value::I64(self.starts as i64)),
            ("stops".into(), Value::I64(self.stops as i64)),
        ]))
    }
}

pub struct CoolLedPe300Driver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    channels: [DeviceId; protocol::PE300_CHANNELS],
    probe: protocol::CoolLedPe300Probe,
    global_enabled: bool,
    pod_locked: bool,
    states: Vec<Pe300ChannelState>,
    timing: TimingExecutionState,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
}

impl CoolLedPe300Driver {
    pub fn simulated(id: DriverId) -> Self {
        Self::configured_fixture(id, CoolLedPe300ConfiguredProbe::simulated())
    }

    pub fn configured_fixture(id: DriverId, configured: CoolLedPe300ConfiguredProbe) -> Self {
        let serial = ScriptedSerial::with_reads(vec![b"pE-300\r\n".to_vec()]);
        Self::new_configured(id, configured, Box::new(serial), false)
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: CoolLedPe300ConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "CoolLED pE-300 serial probe is missing serial_port metadata",
            )
        })?;
        let mut serial = numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(endpoint.port_name, endpoint.baud_rate)
                .timeout(Duration::from_millis(endpoint.timeout_ms)),
        )?;
        let probe_result = protocol::execute_pe300_probe_script(&mut serial, 4)?;
        Ok(Self::new_configured(id, configured, Box::new(serial), true)
            .with_probe_result(probe_result))
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _configured: CoolLedPe300ConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "CoolLED pE-300 real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(
        id: DriverId,
        probe: protocol::CoolLedPe300Probe,
        serial: Box<dyn SerialIo>,
    ) -> Self {
        let states = (0..protocol::PE300_CHANNELS)
            .map(|_| Pe300ChannelState {
                selected: false,
                intensity_percent: 0,
            })
            .collect();
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 821)),
            hub: DeviceId(NodeId(id.0 * 1000 + 830)),
            channels: [
                DeviceId(NodeId(id.0 * 1000 + 831)),
                DeviceId(NodeId(id.0 * 1000 + 832)),
                DeviceId(NodeId(id.0 * 1000 + 833)),
            ],
            probe,
            global_enabled: false,
            pod_locked: false,
            states,
            timing: TimingExecutionState::default(),
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

    pub fn new_configured(
        id: DriverId,
        configured: CoolLedPe300ConfiguredProbe,
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
            .unwrap_or(9_600);
        driver.serial_timeout_ms = configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.timeout_ms)
            .unwrap_or(100);
        driver.connected = connected;
        driver
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    #[cfg_attr(not(feature = "os-serial"), allow(dead_code))]
    fn with_probe_result(mut self, result: protocol::CoolLedPe300ProbeResult) -> Self {
        self.probe.model = result.probe.model;
        self.probe.mainboard_version = result.probe.mainboard_version;
        self.probe.pod_version = result.probe.pod_version;
        if let Some(enabled) = result.global_enabled {
            self.global_enabled = enabled;
        }
        for channel in result.channels {
            let index = channel.channel.index();
            if index < self.states.len() {
                self.states[index].selected = channel.selected;
                self.states[index].intensity_percent = channel.intensity_percent;
            }
        }
        self
    }

    fn send(&mut self, command: protocol::CoolLedCommand) -> Result<()> {
        let line = protocol::encode(&command);
        self.serial.write(&self.codec.encode(&line))
    }

    fn invoke_transaction(
        &self,
        description: &str,
        command: protocol::CoolLedCommand,
    ) -> PhysicalTransaction {
        let line = protocol::encode(&command);
        PhysicalTransaction {
            resource: Some(self.resource),
            description: description.into(),
            payload: Value::Bytes(self.codec.encode(&line)),
        }
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        let mut descriptors = vec![DeviceDescriptor {
            id: self.hub,
            driver: self.id,
            label: "coolled-pe300-hub".into(),
            vendor: Some("CoolLED".into()),
            model: Some(self.probe.model.clone()),
            serial: None,
            kinds: vec!["hub".into(), "light.engine".into(), "shutter".into()],
            properties: vec![
                sequenceable_property("enabled", "Global state", ValueType::Bool, None, true, None),
                property(
                    "pod_locked",
                    "Pod locked",
                    ValueType::Bool,
                    None,
                    true,
                    None,
                ),
                property("model", "Model", ValueType::String, None, false, None),
                property(
                    "mainboard_version",
                    "Mainboard version",
                    ValueType::String,
                    None,
                    false,
                    None,
                ),
                property(
                    "pod_version",
                    "Pod version",
                    ValueType::String,
                    None,
                    false,
                    None,
                ),
                property(
                    "timing_state",
                    "Timing state",
                    ValueType::Map,
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
            ],
            metadata: BTreeMap::from([
                ("model".into(), Value::String(self.probe.model.clone())),
                (
                    "channel_count".into(),
                    Value::I64(protocol::PE300_CHANNELS as i64),
                ),
                (
                    "protocol".into(),
                    Value::String("CoolLED pE-300 CR/LF serial".into()),
                ),
                ("timing_state".into(), self.timing.value()),
            ]),
        }];

        for (index, device) in self.channels.iter().enumerate() {
            descriptors.push(DeviceDescriptor {
                id: *device,
                driver: self.id,
                label: format!("coolled-pe300-channel-{}", index + 1),
                vendor: Some("CoolLED".into()),
                model: Some(format!(
                    "pE-300 channel {}",
                    self.probe.channel_labels[index]
                )),
                serial: None,
                kinds: vec![
                    "light.source".into(),
                    "led.channel".into(),
                    "trigger.sink".into(),
                ],
                properties: vec![
                    sequenceable_property(
                        "enabled",
                        "Channel enabled",
                        ValueType::Bool,
                        None,
                        true,
                        None,
                    ),
                    sequenceable_property(
                        "selected",
                        "Selected",
                        ValueType::Bool,
                        None,
                        true,
                        None,
                    ),
                    sequenceable_property(
                        "intensity",
                        "Intensity",
                        ValueType::Ratio,
                        Some("percent"),
                        true,
                        Some(Range {
                            min: Value::Ratio(Ratio::from_percent(0.0)),
                            max: Value::Ratio(Ratio::from_percent(100.0)),
                        }),
                    ),
                ],
                metadata: BTreeMap::from([
                    ("channel_index".into(), Value::I64(index as i64)),
                    (
                        "channel_label".into(),
                        Value::String(self.probe.channel_labels[index].clone()),
                    ),
                ]),
            });
        }
        descriptors
    }

    fn device_index(&self, device: DeviceId) -> Option<usize> {
        self.channels
            .iter()
            .position(|candidate| *candidate == device)
    }

    fn owns_device(&self, device: DeviceId) -> bool {
        device == self.hub || self.device_index(device).is_some()
    }

    fn timing_counts(&self, plan: &TimingPlan) -> (usize, usize, usize) {
        let route_count = plan
            .routes
            .iter()
            .filter(|route| self.owns_device(route.from) || self.owns_device(route.to))
            .count();
        let routed_channels = plan
            .routes
            .iter()
            .filter(|route| self.device_index(route.to).is_some())
            .count();
        let sequence_count = plan
            .sequences
            .iter()
            .filter(|sequence| self.owns_device(sequence.device))
            .count();
        (route_count, routed_channels, sequence_count)
    }

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| self.owns_device(sequence.device))
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequences(plan) {
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
                Some(StateWrite {
                    device: sequence.device,
                    property: sequence.property.clone(),
                    value: value.clone(),
                })
            })
            .collect::<Vec<_>>();
        let mut changed = BTreeMap::new();
        for write in writes {
            let value = self.write_property(write.device, &write.property, &write.value)?;
            self.emit_property(write.device, &write.property, value.clone());
            changed.insert(format!("{}:{}", (write.device.0).0, write.property), value);
        }
        Ok(Value::Map(changed))
    }

    fn state_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("hub".into(), Value::I64(self.hub.0 .0 as i64)),
            ("model".into(), Value::String(self.probe.model.clone())),
            (
                "mainboard_version".into(),
                Value::String(self.probe.mainboard_version.clone()),
            ),
            (
                "pod_version".into(),
                Value::String(self.probe.pod_version.clone()),
            ),
            (
                "channel_count".into(),
                Value::I64(protocol::PE300_CHANNELS as i64),
            ),
            ("enabled".into(), Value::Bool(self.global_enabled)),
            ("pod_locked".into(), Value::Bool(self.pod_locked)),
            ("timing_state".into(), self.timing.value()),
            (
                "channels".into(),
                Value::List(
                    self.states
                        .iter()
                        .enumerate()
                        .map(|(index, state)| {
                            Value::Map(BTreeMap::from([
                                (
                                    "device".into(),
                                    Value::I64(self.channels[index].0 .0 as i64),
                                ),
                                ("index".into(), Value::I64(index as i64)),
                                (
                                    "label".into(),
                                    Value::String(self.probe.channel_labels[index].clone()),
                                ),
                                ("enabled".into(), Value::Bool(state.selected)),
                                ("selected".into(), Value::Bool(state.selected)),
                                (
                                    "intensity".into(),
                                    Value::Ratio(Ratio::from_percent(
                                        state.intensity_percent as f64,
                                    )),
                                ),
                            ]))
                        })
                        .collect(),
                ),
            ),
        ]))
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "enabled" => Ok(Value::Bool(self.global_enabled)),
                "pod_locked" => Ok(Value::Bool(self.pod_locked)),
                "model" => Ok(Value::String(self.probe.model.clone())),
                "mainboard_version" => Ok(Value::String(self.probe.mainboard_version.clone())),
                "pod_version" => Ok(Value::String(self.probe.pod_version.clone())),
                "timing_state" => Ok(self.timing.value()),
                "state_summary" => Ok(self.state_summary()),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown CoolLED pE-300 hub property {key}"),
                )),
            };
        }
        let index = self.device_index(device).ok_or_else(|| {
            Error::new(ErrorCode::InvalidCommand, "unknown CoolLED pE-300 device")
        })?;
        let state = &self.states[index];
        match key {
            "enabled" | "selected" => Ok(Value::Bool(state.selected)),
            "intensity" => Ok(Value::Ratio(Ratio::from_percent(
                state.intensity_percent as f64,
            ))),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown CoolLED pE-300 channel property {key}"),
            )),
        }
    }

    fn generic_readbacks_for(
        &self,
        device: DeviceId,
        command: &str,
    ) -> Result<Vec<protocol::CoolLedCommand>> {
        if device == self.hub {
            let channels = self
                .channels
                .iter()
                .enumerate()
                .filter_map(|(index, _)| protocol::Channel::from_index(index))
                .map(protocol::CoolLedCommand::QueryChannel);
            return match command {
                "refresh_readbacks" => Ok([
                    protocol::CoolLedCommand::Model,
                    protocol::CoolLedCommand::Version,
                    protocol::CoolLedCommand::Status,
                ]
                .into_iter()
                .chain(channels)
                .collect()),
                "refresh_identity" => Ok(vec![
                    protocol::CoolLedCommand::Model,
                    protocol::CoolLedCommand::Version,
                ]),
                "refresh_status" => Ok(vec![protocol::CoolLedCommand::Status]),
                "refresh_channels" => Ok(channels.collect()),
                other => Err(Error::new(
                    ErrorCode::Unsupported,
                    format!(
                        "CoolLED pE-300 hub GenericCommand supports refresh_readbacks, refresh_identity, refresh_status, and refresh_channels; got {other}"
                    ),
                )),
            };
        }
        let index = self.device_index(device).ok_or_else(|| {
            Error::new(ErrorCode::InvalidCommand, "unknown CoolLED pE-300 device")
        })?;
        let channel = protocol::Channel::from_index(index).ok_or_else(|| {
            Error::new(ErrorCode::InvalidCommand, "unknown CoolLED pE-300 channel")
        })?;
        match command {
            "refresh_readbacks" | "refresh_channel" => {
                Ok(vec![protocol::CoolLedCommand::QueryChannel(channel)])
            }
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "CoolLED pE-300 channel GenericCommand supports refresh_readbacks and refresh_channel; got {other}"
                ),
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
        if !request.params.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "CoolLED pE-300 GenericCommand does not take parameters",
            ));
        }
        let _ = self.generic_readbacks_for(device, &request.command)?;
        Ok(())
    }

    fn apply_generic_command(
        &mut self,
        device: DeviceId,
        request: GenericCommandRequest,
    ) -> Result<Value> {
        self.validate_generic_command(device, &request)?;
        let commands = self.generic_readbacks_for(device, &request.command)?;
        for command in &commands {
            self.refresh_pe300_readback(command)?;
        }
        let value = if device == self.hub {
            self.state_summary()
        } else {
            Value::Map(BTreeMap::from([
                ("enabled".into(), self.read_property(device, "enabled")?),
                ("selected".into(), self.read_property(device, "selected")?),
                ("intensity".into(), self.read_property(device, "intensity")?),
            ]))
        };
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("commands".into(), Value::I64(commands.len() as i64)),
            ("state".into(), value),
            (
                "completion_basis".into(),
                Value::String("CoolLED pE-300 mapped readback".into()),
            ),
        ])))
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
        if device == self.hub {
            return match (key, value) {
                ("enabled", Value::Bool(enabled)) => {
                    self.send(protocol::CoolLedCommand::SetGlobal(*enabled))?;
                    self.global_enabled = *enabled;
                    self.refresh_pe300_readback(&protocol::CoolLedCommand::Status)?;
                    Ok(Value::Bool(*enabled))
                }
                ("pod_locked", Value::Bool(locked)) => {
                    self.send(protocol::CoolLedCommand::SetPodLocked(*locked))?;
                    self.pod_locked = *locked;
                    Ok(Value::Bool(*locked))
                }
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("invalid CoolLED pE-300 hub write {key}"),
                )),
            };
        }

        let index = self.device_index(device).ok_or_else(|| {
            Error::new(ErrorCode::InvalidCommand, "unknown CoolLED pE-300 device")
        })?;
        let channel = protocol::Channel::from_index(index).ok_or_else(|| {
            Error::new(ErrorCode::InvalidCommand, "unknown CoolLED pE-300 channel")
        })?;
        match (key, value) {
            ("enabled", Value::Bool(enabled)) | ("selected", Value::Bool(enabled)) => {
                self.send(protocol::CoolLedCommand::SetChannelSelected {
                    channel,
                    selected: *enabled,
                })?;
                self.states[index].selected = *enabled;
                self.refresh_pe300_readback(&protocol::CoolLedCommand::QueryChannel(channel))?;
                Ok(Value::Bool(*enabled))
            }
            ("intensity", Value::Ratio(percent)) => {
                let percent = percent.percent().clamp(0.0, 100.0).round() as u8;
                self.send(protocol::CoolLedCommand::SetIntensity { channel, percent })?;
                self.states[index].intensity_percent = percent;
                self.refresh_pe300_readback(&protocol::CoolLedCommand::QueryChannel(channel))?;
                Ok(Value::Ratio(Ratio::from_percent(percent as f64)))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid CoolLED pE-300 write {key}"),
            )),
        }
    }

    fn invoke_transactions(
        &self,
        device: DeviceId,
        kind: CapabilityKind,
        request: &CapabilityRequest,
    ) -> Result<Vec<protocol::CoolLedCommand>> {
        match kind {
            CapabilityKind::Dac => {
                let index = self.device_index(device).ok_or_else(|| {
                    Error::new(
                        ErrorCode::InvalidCommand,
                        "CoolLED pE-300 Dac invocation requires a channel device",
                    )
                })?;
                let channel = protocol::Channel::from_index(index).ok_or_else(|| {
                    Error::new(ErrorCode::InvalidCommand, "unknown CoolLED pE-300 channel")
                })?;
                Ok(vec![protocol::CoolLedCommand::SetIntensity {
                    channel,
                    percent: dac_request_percent(request)?,
                }])
            }
            CapabilityKind::TriggerSink => {
                if device == self.hub {
                    return trigger_sink_commands(request, protocol::CoolLedCommand::SetGlobal);
                }
                let index = self.device_index(device).ok_or_else(|| {
                    Error::new(
                        ErrorCode::InvalidCommand,
                        "CoolLED pE-300 TriggerSink invocation requires the hub or a channel device",
                    )
                })?;
                let channel = protocol::Channel::from_index(index).ok_or_else(|| {
                    Error::new(ErrorCode::InvalidCommand, "unknown CoolLED pE-300 channel")
                })?;
                trigger_sink_commands(request, |selected| {
                    protocol::CoolLedCommand::SetChannelSelected { channel, selected }
                })
            }
            CapabilityKind::GenericCommand => {
                let CapabilityRequest::GenericCommand(request) = request else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "CoolLED pE-300 GenericCommand expects GenericCommandRequest",
                    ));
                };
                self.validate_generic_command(device, request)?;
                self.generic_readbacks_for(device, &request.command)
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported CoolLED pE-300 invocation capability",
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
            CapabilityKind::Dac => {
                let percent = dac_request_percent(&request)?;
                let value = self.write_property(
                    device,
                    "intensity",
                    &Value::Ratio(Ratio::from_percent(percent as f64)),
                )?;
                self.emit_property(device, "intensity", value.clone());
                Ok(Value::Map(BTreeMap::from([
                    ("intensity".into(), value),
                    ("commands".into(), Value::I64(1)),
                ])))
            }
            CapabilityKind::TriggerSink => {
                let commands = self.invoke_transactions(device, kind, &request)?;
                for command in &commands {
                    match command {
                        protocol::CoolLedCommand::SetGlobal(enabled) => {
                            let value =
                                self.write_property(self.hub, "enabled", &Value::Bool(*enabled))?;
                            self.emit_property(self.hub, "enabled", value);
                        }
                        protocol::CoolLedCommand::SetChannelSelected { selected, .. } => {
                            let value =
                                self.write_property(device, "enabled", &Value::Bool(*selected))?;
                            self.emit_property(device, "enabled", value.clone());
                            self.emit_property(device, "selected", value);
                        }
                        _ => self.send(command.clone())?,
                    }
                }
                let enabled = if device == self.hub {
                    self.global_enabled
                } else {
                    let index = self.device_index(device).ok_or_else(|| {
                        Error::new(ErrorCode::InvalidCommand, "unknown CoolLED pE-300 device")
                    })?;
                    self.states[index].selected
                };
                Ok(Value::Map(BTreeMap::from([
                    ("triggered".into(), Value::Bool(true)),
                    ("enabled".into(), Value::Bool(enabled)),
                    ("commands".into(), Value::I64(commands.len() as i64)),
                ])))
            }
            CapabilityKind::GenericCommand => {
                let CapabilityRequest::GenericCommand(request) = request else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "CoolLED pE-300 GenericCommand expects GenericCommandRequest",
                    ));
                };
                self.apply_generic_command(device, request)
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported CoolLED pE-300 invocation capability",
            )),
        }
    }

    fn refresh_pe300_readback(&mut self, command: &protocol::CoolLedCommand) -> Result<()> {
        self.send(command.clone())?;
        let bytes = self.serial.read_available()?;
        if bytes.is_empty() {
            return Ok(());
        }
        for line in self.codec.push(&bytes) {
            self.apply_pe300_readback(command, &line)?;
        }
        Ok(())
    }

    fn apply_pe300_readback(
        &mut self,
        command: &protocol::CoolLedCommand,
        reply: &str,
    ) -> Result<()> {
        match command {
            protocol::CoolLedCommand::Model => {
                self.probe.model = reply.trim().to_string();
                self.emit_property(self.hub, "model", Value::String(self.probe.model.clone()));
            }
            protocol::CoolLedCommand::Version => {
                self.probe.mainboard_version = reply.trim().to_string();
                if let Some(pod) = protocol::parse_pe300_pod_version(reply) {
                    self.probe.pod_version = pod;
                }
                self.emit_property(
                    self.hub,
                    "mainboard_version",
                    Value::String(self.probe.mainboard_version.clone()),
                );
                self.emit_property(
                    self.hub,
                    "pod_version",
                    Value::String(self.probe.pod_version.clone()),
                );
            }
            protocol::CoolLedCommand::Status => {
                if let Some(enabled) = protocol::parse_global_enabled(reply) {
                    self.global_enabled = enabled;
                    self.emit_property(self.hub, "enabled", Value::Bool(enabled));
                }
                self.emit_property(self.hub, "state_summary", self.state_summary());
            }
            protocol::CoolLedCommand::QueryChannel(channel) => {
                let parsed = protocol::parse_channel_probe(*channel, reply)?;
                let index = channel.index();
                let device = self.channels.get(index).copied();
                let mut values = None;
                if let Some(state) = self.states.get_mut(index) {
                    state.selected = parsed.selected;
                    state.intensity_percent = parsed.intensity_percent;
                    values = Some((state.selected, state.intensity_percent));
                }
                if let (Some(device), Some((selected, intensity))) = (device, values) {
                    self.emit_property(device, "enabled", Value::Bool(selected));
                    self.emit_property(device, "selected", Value::Bool(selected));
                    self.emit_property(
                        device,
                        "intensity",
                        Value::Ratio(Ratio::from_percent(intensity as f64)),
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn issue_read_command(&mut self, device: DeviceId, key: &str) -> Result<()> {
        if device == self.hub && key == "model" {
            self.refresh_pe300_readback(&protocol::CoolLedCommand::Model)?;
        } else if device == self.hub && (key == "mainboard_version" || key == "pod_version") {
            self.refresh_pe300_readback(&protocol::CoolLedCommand::Version)?;
        } else if device == self.hub && key == "state_summary" {
            self.refresh_pe300_readback(&protocol::CoolLedCommand::Status)?;
        } else if let Some(index) = self.device_index(device) {
            if let Some(channel) = protocol::Channel::from_index(index) {
                self.refresh_pe300_readback(&protocol::CoolLedCommand::QueryChannel(channel))?;
            }
        }
        Ok(())
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

impl Driver for CoolLedPe300Driver {
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
            label: "coolled-pe300-serial".into(),
            kind: "serial".into(),
            metadata: BTreeMap::from([
                ("send_terminator".into(), Value::String("CR".into())),
                ("recv_terminator".into(), Value::String("LF".into())),
                (
                    "completion".into(),
                    Value::String("command response line".into()),
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
                        protocol::pe300_probe_script()
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
            vec![
                capability(1, device, CapabilityKind::TriggerSink),
                capability(3, device, CapabilityKind::GenericCommand),
            ]
        } else if self.device_index(device).is_some() {
            vec![
                capability(1, device, CapabilityKind::TriggerSink),
                capability(2, device, CapabilityKind::Dac),
                capability(3, device, CapabilityKind::GenericCommand),
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
                        description: format!("coolled pe300 read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("coolled pe300 write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "coolled pe300 remultiplexed light state set".into(),
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
                            "unknown CoolLED pE-300 capability",
                        ));
                    };
                    if !capability.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "CoolLED pE-300 {:?} expects {:?}, got {:?}",
                                capability.kind,
                                capability.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    for command in self.invoke_transactions(*device, capability.kind, request)? {
                        physical_transactions.push(
                            self.invoke_transaction("coolled pe300 direct invocation", command),
                        );
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

    fn prepare_timing_plan(
        &mut self,
        plan: &TimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let (local_route_count, routed_channels, local_sequence_count) = self.timing_counts(plan);
        self.validate_timing_plan(plan)?;
        self.timing.armed = true;
        self.timing.running = false;
        self.timing.route_count = local_route_count;
        self.timing.routed_channels = routed_channels;
        self.timing.sequence_count = local_sequence_count;
        let local_routes = plan
            .routes
            .iter()
            .filter(|route| {
                route.from == self.hub
                    || route.to == self.hub
                    || self.device_index(route.from).is_some()
                    || self.device_index(route.to).is_some()
            })
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
            .collect::<Vec<_>>();
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Arm(plan.clone())],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "coolled pe300 timing arm".into(),
                payload: Value::Map(BTreeMap::from([
                    ("hub".into(), Value::I64(self.hub.0 .0 as i64)),
                    ("routes".into(), Value::List(local_routes)),
                    ("channels".into(), Value::I64(self.channels.len() as i64)),
                    ("timing_state".into(), self.timing.value()),
                ])),
            }],
        })
    }

    fn start_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let (route_count, routed_channels, sequence_count) = self.timing_counts(&armed.plan);
        self.timing.armed = true;
        self.timing.running = true;
        self.timing.routed_channels = routed_channels;
        self.timing.route_count = route_count;
        self.timing.sequence_count = sequence_count;
        self.timing.starts += 1;
        let applied = self.apply_timing_sequence_step(&armed.plan, true)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Start(OperationId(0))],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "coolled pe300 timing start".into(),
                payload: Value::Map(BTreeMap::from([
                    ("hub".into(), Value::I64(self.hub.0 .0 as i64)),
                    ("routed_channels".into(), Value::I64(routed_channels as i64)),
                    ("applied".into(), applied),
                    ("timing_state".into(), self.timing.value()),
                ])),
            }],
        })
    }

    fn stop_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let (route_count, routed_channels, sequence_count) = self.timing_counts(&armed.plan);
        self.timing.armed = false;
        self.timing.running = false;
        self.timing.route_count = route_count;
        self.timing.routed_channels = routed_channels;
        self.timing.sequence_count = sequence_count;
        self.timing.stops += 1;
        let applied = self.apply_timing_sequence_step(&armed.plan, false)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "coolled pe300 timing stop".into(),
                payload: Value::Map(BTreeMap::from([
                    ("hub".into(), Value::I64(self.hub.0 .0 as i64)),
                    ("channels".into(), Value::I64(self.channels.len() as i64)),
                    ("applied".into(), applied),
                    ("timing_state".into(), self.timing.value()),
                ])),
            }],
        })
    }

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.token();
        let mut last = Value::Null;
        for command in prepared.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    self.issue_read_command(device, &key)?;
                    last = self.read_property(device, &key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    last = self.write_property(device, &key, &value)?;
                    self.emit_property(device, &key, last.clone());
                }
                Command::ApplyStateSet(set) => {
                    let mut changed = BTreeMap::new();
                    for write in set.writes {
                        let value =
                            self.write_property(write.device, &write.property, &write.value)?;
                        self.emit_property(write.device, &write.property, value.clone());
                        changed.insert(format!("{}:{}", (write.device.0).0, write.property), value);
                    }
                    last = Value::Map(changed);
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
                            "unknown CoolLED pE-300 capability",
                        ));
                    };
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
        if let Ok(bytes) = self.serial.read_available() {
            for line in self.codec.push(&bytes) {
                self.pending
                    .push_back(DriverEvent::Event(Event::Log(LogEvent {
                        driver: Some(self.id),
                        message: format!("coolled pe300 serial: {line}"),
                    })));
            }
        }
        self.pending.drain(..).collect()
    }
}

pub struct CoolLedPe4000Driver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    channels: [DeviceId; protocol::CHANNELS],
    probe: protocol::CoolLedPe4000Probe,
    global_enabled: bool,
    pod_locked: bool,
    states: Vec<ChannelState>,
    timing: TimingExecutionState,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
}

impl CoolLedPe4000Driver {
    pub fn simulated(id: DriverId) -> Self {
        Self::configured_fixture(id, CoolLedPe4000ConfiguredProbe::simulated_pe4000())
    }

    pub fn pe340_simulated(id: DriverId) -> Self {
        Self::configured_fixture(id, CoolLedPe4000ConfiguredProbe::simulated_pe340())
    }

    pub fn configured_fixture(id: DriverId, configured: CoolLedPe4000ConfiguredProbe) -> Self {
        let read = if configured.probe.device_prefix == "coolled-pe340" {
            b"pE-340\r\n".to_vec()
        } else {
            b"pE-4000\r\n".to_vec()
        };
        let serial = ScriptedSerial::with_reads(vec![read]);
        Self::new_configured(id, configured, Box::new(serial), false)
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: CoolLedPe4000ConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "CoolLED pE-4000/pE-340 serial probe is missing serial_port metadata",
            )
        })?;
        let mut serial = numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(endpoint.port_name, endpoint.baud_rate)
                .timeout(Duration::from_millis(endpoint.timeout_ms)),
        )?;
        let probe_result = protocol::execute_pe4000_probe_script(&mut serial, 4)?;
        Ok(Self::new_configured(id, configured, Box::new(serial), true)
            .with_probe_result(probe_result))
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _configured: CoolLedPe4000ConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "CoolLED pE-4000/pE-340 real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(
        id: DriverId,
        probe: protocol::CoolLedPe4000Probe,
        serial: Box<dyn SerialIo>,
    ) -> Self {
        let states = (0..protocol::CHANNELS)
            .map(|index| ChannelState {
                selected: false,
                intensity_percent: 0,
                wavelength: Wavelength::from_nanometers(probe.wavelengths[index][0]),
                enabled: false,
            })
            .collect();
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 801)),
            hub: DeviceId(NodeId(id.0 * 1000 + 810)),
            channels: [
                DeviceId(NodeId(id.0 * 1000 + 811)),
                DeviceId(NodeId(id.0 * 1000 + 812)),
                DeviceId(NodeId(id.0 * 1000 + 813)),
                DeviceId(NodeId(id.0 * 1000 + 814)),
            ],
            probe,
            global_enabled: false,
            pod_locked: false,
            states,
            timing: TimingExecutionState::default(),
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

    pub fn new_configured(
        id: DriverId,
        configured: CoolLedPe4000ConfiguredProbe,
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
            .unwrap_or(9_600);
        driver.serial_timeout_ms = configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.timeout_ms)
            .unwrap_or(100);
        driver.connected = connected;
        driver
    }

    #[cfg(feature = "os-serial")]
    fn with_probe_result(mut self, probe_result: protocol::CoolLedPe4000ProbeResult) -> Self {
        self.probe = probe_result.probe;
        if let Some(global_enabled) = probe_result.global_enabled {
            self.global_enabled = global_enabled;
        }
        for channel in probe_result.channels {
            let index = channel.channel.index();
            if let Some(state) = self.states.get_mut(index) {
                state.selected = channel.selected;
                state.enabled = channel.selected && self.global_enabled;
                state.intensity_percent = channel.intensity_percent;
                if let Some(wavelength) = channel.wavelength {
                    state.wavelength = wavelength;
                }
            }
        }
        self
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn send(&mut self, command: protocol::CoolLedCommand) -> Result<()> {
        let line = protocol::encode(&command);
        self.serial.write(&self.codec.encode(&line))
    }

    fn invoke_transaction(
        &self,
        description: &str,
        command: protocol::CoolLedCommand,
    ) -> PhysicalTransaction {
        let line = protocol::encode(&command);
        PhysicalTransaction {
            resource: Some(self.resource),
            description: description.into(),
            payload: Value::Bytes(self.codec.encode(&line)),
        }
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        let mut descriptors = vec![DeviceDescriptor {
            id: self.hub,
            driver: self.id,
            label: format!("{}-hub", self.probe.device_prefix),
            vendor: Some("CoolLED".into()),
            model: Some(self.probe.model.clone()),
            serial: None,
            kinds: vec!["hub".into(), "light.engine".into(), "shutter".into()],
            properties: vec![
                sequenceable_property("enabled", "Global state", ValueType::Bool, None, true, None),
                property(
                    "pod_locked",
                    "Pod locked",
                    ValueType::Bool,
                    None,
                    true,
                    None,
                ),
                property("model", "Model", ValueType::String, None, false, None),
                property("version", "Version", ValueType::String, None, false, None),
                property(
                    "timing_state",
                    "Timing state",
                    ValueType::Map,
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
            ],
            metadata: BTreeMap::from([
                ("model".into(), Value::String(self.probe.model.clone())),
                ("version".into(), Value::String(self.probe.version.clone())),
                (
                    "channel_count".into(),
                    Value::I64(protocol::CHANNELS as i64),
                ),
            ]),
        }];

        for (index, device) in self.channels.iter().enumerate() {
            let label = format!("{}-channel-{}", self.probe.device_prefix, index + 1);
            descriptors.push(DeviceDescriptor {
                id: *device,
                driver: self.id,
                label,
                vendor: Some("CoolLED".into()),
                model: Some(format!("{} channel", self.probe.model)),
                serial: None,
                kinds: vec![
                    "light.source".into(),
                    "led.channel".into(),
                    "trigger.sink".into(),
                ],
                properties: vec![
                    sequenceable_property(
                        "enabled",
                        "Channel enabled",
                        ValueType::Bool,
                        None,
                        true,
                        None,
                    ),
                    sequenceable_property(
                        "selected",
                        "Selected",
                        ValueType::Bool,
                        None,
                        true,
                        None,
                    ),
                    sequenceable_property(
                        "intensity",
                        "Intensity",
                        ValueType::Ratio,
                        Some("percent"),
                        true,
                        Some(Range {
                            min: Value::Ratio(Ratio::from_percent(0.0)),
                            max: Value::Ratio(Ratio::from_percent(100.0)),
                        }),
                    ),
                    wavelength_property(&self.probe.wavelengths[index]),
                ],
                metadata: BTreeMap::from([
                    ("channel_index".into(), Value::I64(index as i64)),
                    (
                        "wavelengths".into(),
                        Value::List(
                            self.probe.wavelengths[index]
                                .iter()
                                .map(|nm| Value::Wavelength(Wavelength::from_nanometers(*nm)))
                                .collect(),
                        ),
                    ),
                ]),
            });
        }
        descriptors
    }

    fn device_index(&self, device: DeviceId) -> Option<usize> {
        self.channels
            .iter()
            .position(|candidate| *candidate == device)
    }

    fn owns_device(&self, device: DeviceId) -> bool {
        device == self.hub || self.device_index(device).is_some()
    }

    fn timing_counts(&self, plan: &TimingPlan) -> (usize, usize, usize) {
        let route_count = plan
            .routes
            .iter()
            .filter(|route| self.owns_device(route.from) || self.owns_device(route.to))
            .count();
        let routed_channels = plan
            .routes
            .iter()
            .filter(|route| self.device_index(route.to).is_some())
            .count();
        let sequence_count = plan
            .sequences
            .iter()
            .filter(|sequence| self.owns_device(sequence.device))
            .count();
        (route_count, routed_channels, sequence_count)
    }

    fn state_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("hub".into(), Value::I64(self.hub.0 .0 as i64)),
            ("model".into(), Value::String(self.probe.model.clone())),
            ("version".into(), Value::String(self.probe.version.clone())),
            (
                "channel_count".into(),
                Value::I64(protocol::CHANNELS as i64),
            ),
            ("enabled".into(), Value::Bool(self.global_enabled)),
            ("pod_locked".into(), Value::Bool(self.pod_locked)),
            ("timing_state".into(), self.timing.value()),
            (
                "channels".into(),
                Value::List(
                    self.states
                        .iter()
                        .enumerate()
                        .map(|(index, state)| {
                            Value::Map(BTreeMap::from([
                                (
                                    "device".into(),
                                    Value::I64(self.channels[index].0 .0 as i64),
                                ),
                                ("index".into(), Value::I64(index as i64)),
                                ("enabled".into(), Value::Bool(state.enabled)),
                                ("selected".into(), Value::Bool(state.selected)),
                                (
                                    "intensity".into(),
                                    Value::Ratio(Ratio::from_percent(
                                        state.intensity_percent as f64,
                                    )),
                                ),
                                ("wavelength".into(), Value::Wavelength(state.wavelength)),
                                (
                                    "available_wavelengths".into(),
                                    Value::List(
                                        self.probe.wavelengths[index]
                                            .iter()
                                            .map(|nm| {
                                                Value::Wavelength(Wavelength::from_nanometers(*nm))
                                            })
                                            .collect(),
                                    ),
                                ),
                            ]))
                        })
                        .collect(),
                ),
            ),
        ]))
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "enabled" => Ok(Value::Bool(self.global_enabled)),
                "pod_locked" => Ok(Value::Bool(self.pod_locked)),
                "model" => Ok(Value::String(self.probe.model.clone())),
                "version" => Ok(Value::String(self.probe.version.clone())),
                "timing_state" => Ok(self.timing.value()),
                "state_summary" => Ok(self.state_summary()),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown CoolLED hub property {key}"),
                )),
            };
        }
        let index = self
            .device_index(device)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown CoolLED device"))?;
        let state = &self.states[index];
        match key {
            "enabled" => Ok(Value::Bool(state.enabled)),
            "selected" => Ok(Value::Bool(state.selected)),
            "intensity" => Ok(Value::Ratio(Ratio::from_percent(
                state.intensity_percent as f64,
            ))),
            "wavelength" => Ok(Value::Wavelength(state.wavelength)),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown CoolLED channel property {key}"),
            )),
        }
    }

    fn generic_readbacks_for(
        &self,
        device: DeviceId,
        command: &str,
    ) -> Result<Vec<protocol::CoolLedCommand>> {
        if device == self.hub {
            let channels = self
                .channels
                .iter()
                .enumerate()
                .filter_map(|(index, _)| protocol::Channel::from_index(index))
                .map(protocol::CoolLedCommand::QueryChannel);
            return match command {
                "refresh_readbacks" => Ok([
                    protocol::CoolLedCommand::Model,
                    protocol::CoolLedCommand::Version,
                    protocol::CoolLedCommand::LampSummary,
                    protocol::CoolLedCommand::Status,
                ]
                .into_iter()
                .chain(channels)
                .collect()),
                "refresh_identity" => Ok(vec![
                    protocol::CoolLedCommand::Model,
                    protocol::CoolLedCommand::Version,
                    protocol::CoolLedCommand::LampSummary,
                ]),
                "refresh_status" => Ok(vec![protocol::CoolLedCommand::Status]),
                "refresh_channels" => Ok(channels.collect()),
                other => Err(Error::new(
                    ErrorCode::Unsupported,
                    format!(
                        "CoolLED pE-4000 hub GenericCommand supports refresh_readbacks, refresh_identity, refresh_status, and refresh_channels; got {other}"
                    ),
                )),
            };
        }
        let index = self
            .device_index(device)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown CoolLED device"))?;
        let channel = protocol::Channel::from_index(index)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown CoolLED channel"))?;
        match command {
            "refresh_readbacks" | "refresh_channel" => {
                Ok(vec![protocol::CoolLedCommand::QueryChannel(channel)])
            }
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "CoolLED pE-4000 channel GenericCommand supports refresh_readbacks and refresh_channel; got {other}"
                ),
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
        if !request.params.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "CoolLED pE-4000 GenericCommand does not take parameters",
            ));
        }
        let _ = self.generic_readbacks_for(device, &request.command)?;
        Ok(())
    }

    fn apply_generic_command(
        &mut self,
        device: DeviceId,
        request: GenericCommandRequest,
    ) -> Result<Value> {
        self.validate_generic_command(device, &request)?;
        let commands = self.generic_readbacks_for(device, &request.command)?;
        for command in &commands {
            self.refresh_pe4000_readback(command)?;
        }
        let value = if device == self.hub {
            self.state_summary()
        } else {
            Value::Map(BTreeMap::from([
                ("enabled".into(), self.read_property(device, "enabled")?),
                ("selected".into(), self.read_property(device, "selected")?),
                ("intensity".into(), self.read_property(device, "intensity")?),
                (
                    "wavelength".into(),
                    self.read_property(device, "wavelength")?,
                ),
            ]))
        };
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("commands".into(), Value::I64(commands.len() as i64)),
            ("state".into(), value),
            (
                "completion_basis".into(),
                Value::String("CoolLED pE-4000 mapped readback".into()),
            ),
        ])))
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
        if device == self.hub {
            return match (key, value) {
                ("enabled", Value::Bool(enabled)) => {
                    self.send(protocol::CoolLedCommand::SetGlobal(*enabled))?;
                    self.global_enabled = *enabled;
                    self.refresh_pe4000_readback(&protocol::CoolLedCommand::Status)?;
                    Ok(Value::Bool(*enabled))
                }
                ("pod_locked", Value::Bool(locked)) => {
                    self.send(protocol::CoolLedCommand::SetPodLocked(*locked))?;
                    self.pod_locked = *locked;
                    Ok(Value::Bool(*locked))
                }
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("invalid CoolLED hub write {key}"),
                )),
            };
        }

        let index = self
            .device_index(device)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown CoolLED device"))?;
        let channel = protocol::Channel::from_index(index)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown CoolLED channel"))?;
        match (key, value) {
            ("enabled", Value::Bool(enabled)) | ("selected", Value::Bool(enabled)) => {
                self.send(protocol::CoolLedCommand::SetChannelSelected {
                    channel,
                    selected: *enabled,
                })?;
                self.states[index].selected = *enabled;
                self.states[index].enabled = *enabled;
                self.refresh_pe4000_readback(&protocol::CoolLedCommand::QueryChannel(channel))?;
                Ok(Value::Bool(*enabled))
            }
            ("intensity", Value::Ratio(percent)) => {
                let percent = percent.percent().clamp(0.0, 100.0).round() as u8;
                self.send(protocol::CoolLedCommand::SetIntensity { channel, percent })?;
                self.states[index].intensity_percent = percent;
                self.refresh_pe4000_readback(&protocol::CoolLedCommand::QueryChannel(channel))?;
                Ok(Value::Ratio(Ratio::from_percent(percent as f64)))
            }
            ("wavelength", Value::Wavelength(wavelength)) => {
                let nm = wavelength.nanometers();
                if !self.probe.wavelengths[index]
                    .iter()
                    .any(|candidate| (*candidate - nm).abs() < 0.5)
                {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "wavelength is not available for this pE-4000 channel",
                    ));
                }
                self.send(protocol::CoolLedCommand::LoadWavelength(*wavelength))?;
                self.states[index].wavelength = *wavelength;
                self.refresh_pe4000_readback(&protocol::CoolLedCommand::QueryChannel(channel))?;
                Ok(Value::Wavelength(*wavelength))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid CoolLED write {key}"),
            )),
        }
    }

    fn refresh_pe4000_readback(&mut self, command: &protocol::CoolLedCommand) -> Result<()> {
        self.send(command.clone())?;
        let bytes = self.serial.read_available()?;
        if bytes.is_empty() {
            return Ok(());
        }
        for line in self.codec.push(&bytes) {
            self.apply_pe4000_readback(command, &line)?;
        }
        Ok(())
    }

    fn apply_pe4000_readback(
        &mut self,
        command: &protocol::CoolLedCommand,
        reply: &str,
    ) -> Result<()> {
        match command {
            protocol::CoolLedCommand::Model => {
                self.probe.model = reply.trim().to_string();
                self.emit_property(self.hub, "model", Value::String(self.probe.model.clone()));
            }
            protocol::CoolLedCommand::Version => {
                self.probe.version = reply.trim().to_string();
                self.emit_property(
                    self.hub,
                    "version",
                    Value::String(self.probe.version.clone()),
                );
            }
            protocol::CoolLedCommand::Status => {
                if let Some(enabled) = protocol::parse_global_enabled(reply) {
                    self.global_enabled = enabled;
                    self.emit_property(self.hub, "enabled", Value::Bool(enabled));
                }
                self.emit_property(self.hub, "state_summary", self.state_summary());
            }
            protocol::CoolLedCommand::LampSummary => {
                if let Some(wavelengths) = protocol::parse_lamp_summary(reply) {
                    self.probe.wavelengths = wavelengths;
                }
            }
            protocol::CoolLedCommand::QueryChannel(channel) => {
                let parsed = protocol::parse_channel_probe(*channel, reply)?;
                let index = channel.index();
                let device = self.channels.get(index).copied();
                let mut values = None;
                if let Some(state) = self.states.get_mut(index) {
                    state.selected = parsed.selected;
                    state.enabled = parsed.selected;
                    state.intensity_percent = parsed.intensity_percent;
                    if let Some(wavelength) = parsed.wavelength {
                        state.wavelength = wavelength;
                    }
                    values = Some((
                        state.enabled,
                        state.selected,
                        state.intensity_percent,
                        state.wavelength,
                    ));
                }
                if let (Some(device), Some((enabled, selected, intensity, wavelength))) =
                    (device, values)
                {
                    self.emit_property(device, "enabled", Value::Bool(enabled));
                    self.emit_property(device, "selected", Value::Bool(selected));
                    self.emit_property(
                        device,
                        "intensity",
                        Value::Ratio(Ratio::from_percent(intensity as f64)),
                    );
                    self.emit_property(device, "wavelength", Value::Wavelength(wavelength));
                }
            }
            _ => {}
        }
        Ok(())
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

    fn apply_timing_sequence_step(&mut self, plan: &TimingPlan, first: bool) -> Result<Value> {
        let writes = self
            .local_timing_sequence_refs(plan)
            .into_iter()
            .filter_map(|sequence| {
                let value = if first {
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
        let mut changed = BTreeMap::new();
        for write in writes {
            let value = self.write_property(write.device, &write.property, &write.value)?;
            self.emit_property(write.device, &write.property, value.clone());
            changed.insert(format!("{}:{}", (write.device.0).0, write.property), value);
        }
        Ok(Value::Map(changed))
    }

    fn timing_summary(&self, plan: &TimingPlan, action: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("action".into(), Value::String(action.into())),
            ("hub".into(), Value::I64(self.hub.0 .0 as i64)),
            ("model".into(), Value::String(self.probe.model.clone())),
            ("enabled".into(), Value::Bool(self.global_enabled)),
            ("channels".into(), Value::I64(self.channels.len() as i64)),
            ("routes".into(), Value::List(self.local_timing_routes(plan))),
            (
                "sequences".into(),
                Value::List(self.local_timing_sequences(plan)),
            ),
            ("timing_state".into(), self.timing.value()),
        ]))
    }

    fn timing_transaction(
        &self,
        description: &str,
        command: protocol::CoolLedCommand,
    ) -> PhysicalTransaction {
        let line = protocol::encode(&command);
        PhysicalTransaction {
            resource: Some(self.resource),
            description: description.into(),
            payload: Value::Bytes(self.codec.encode(&line)),
        }
    }

    fn invoke_transactions(
        &self,
        device: DeviceId,
        kind: CapabilityKind,
        request: &CapabilityRequest,
    ) -> Result<Vec<protocol::CoolLedCommand>> {
        match kind {
            CapabilityKind::Dac => {
                let index = self.device_index(device).ok_or_else(|| {
                    Error::new(
                        ErrorCode::InvalidCommand,
                        "CoolLED Dac invocation requires a channel device",
                    )
                })?;
                let channel = protocol::Channel::from_index(index).ok_or_else(|| {
                    Error::new(ErrorCode::InvalidCommand, "unknown CoolLED channel")
                })?;
                Ok(vec![protocol::CoolLedCommand::SetIntensity {
                    channel,
                    percent: dac_request_percent(request)?,
                }])
            }
            CapabilityKind::TriggerSink => {
                if device == self.hub {
                    return trigger_sink_commands(request, protocol::CoolLedCommand::SetGlobal);
                }
                let index = self.device_index(device).ok_or_else(|| {
                    Error::new(
                        ErrorCode::InvalidCommand,
                        "CoolLED TriggerSink invocation requires the hub or a channel device",
                    )
                })?;
                let channel = protocol::Channel::from_index(index).ok_or_else(|| {
                    Error::new(ErrorCode::InvalidCommand, "unknown CoolLED channel")
                })?;
                trigger_sink_commands(request, |selected| {
                    protocol::CoolLedCommand::SetChannelSelected { channel, selected }
                })
            }
            CapabilityKind::GenericCommand => {
                let CapabilityRequest::GenericCommand(request) = request else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "CoolLED pE-4000 GenericCommand expects GenericCommandRequest",
                    ));
                };
                self.validate_generic_command(device, request)?;
                self.generic_readbacks_for(device, &request.command)
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported CoolLED invocation capability",
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
            CapabilityKind::Dac => {
                let percent = dac_request_percent(&request)?;
                let value = self.write_property(
                    device,
                    "intensity",
                    &Value::Ratio(Ratio::from_percent(percent as f64)),
                )?;
                self.emit_property(device, "intensity", value.clone());
                Ok(Value::Map(BTreeMap::from([
                    ("intensity".into(), value),
                    ("commands".into(), Value::I64(1)),
                ])))
            }
            CapabilityKind::TriggerSink => {
                let commands = self.invoke_transactions(device, kind, &request)?;
                for command in &commands {
                    match command {
                        protocol::CoolLedCommand::SetGlobal(enabled) => {
                            let value =
                                self.write_property(self.hub, "enabled", &Value::Bool(*enabled))?;
                            self.emit_property(self.hub, "enabled", value);
                        }
                        protocol::CoolLedCommand::SetChannelSelected { selected, .. } => {
                            let value =
                                self.write_property(device, "enabled", &Value::Bool(*selected))?;
                            self.emit_property(device, "enabled", value.clone());
                            self.emit_property(device, "selected", value);
                        }
                        _ => self.send(command.clone())?,
                    }
                }
                let enabled = if device == self.hub {
                    self.global_enabled
                } else {
                    let index = self.device_index(device).ok_or_else(|| {
                        Error::new(ErrorCode::InvalidCommand, "unknown CoolLED device")
                    })?;
                    self.states[index].enabled
                };
                Ok(Value::Map(BTreeMap::from([
                    ("triggered".into(), Value::Bool(true)),
                    ("enabled".into(), Value::Bool(enabled)),
                    ("commands".into(), Value::I64(commands.len() as i64)),
                ])))
            }
            CapabilityKind::GenericCommand => {
                let CapabilityRequest::GenericCommand(request) = request else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "CoolLED pE-4000 GenericCommand expects GenericCommandRequest",
                    ));
                };
                self.apply_generic_command(device, request)
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported CoolLED invocation capability",
            )),
        }
    }
}

impl Driver for CoolLedPe4000Driver {
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
            label: format!("{}-serial", self.probe.device_prefix),
            kind: "serial".into(),
            metadata: BTreeMap::from([
                ("send_terminator".into(), Value::String("CR".into())),
                ("recv_terminator".into(), Value::String("LF".into())),
                (
                    "completion".into(),
                    Value::String("command response line".into()),
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
            vec![
                capability(1, device, CapabilityKind::TriggerSink),
                capability(3, device, CapabilityKind::GenericCommand),
            ]
        } else if self.device_index(device).is_some() {
            vec![
                capability(1, device, CapabilityKind::TriggerSink),
                capability(2, device, CapabilityKind::Dac),
                capability(3, device, CapabilityKind::GenericCommand),
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
                        description: format!("coolled read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("coolled write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "coolled remultiplexed light state set".into(),
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
                            "unknown CoolLED capability",
                        ));
                    };
                    if !capability.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "CoolLED {:?} expects {:?}, got {:?}",
                                capability.kind,
                                capability.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    for command in self.invoke_transactions(*device, capability.kind, request)? {
                        physical_transactions
                            .push(self.invoke_transaction("coolled direct invocation", command));
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
                    last = self.read_property(device, &key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    last = self.write_property(device, &key, &value)?;
                    self.emit_property(device, &key, last.clone());
                }
                Command::ApplyStateSet(set) => {
                    let mut changed = BTreeMap::new();
                    for write in set.writes {
                        let value =
                            self.write_property(write.device, &write.property, &write.value)?;
                        self.emit_property(write.device, &write.property, value.clone());
                        changed.insert(format!("{}:{}", (write.device.0).0, write.property), value);
                    }
                    last = Value::Map(changed);
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
                            "unknown CoolLED capability",
                        ));
                    };
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
        if let Ok(bytes) = self.serial.read_available() {
            for line in self.codec.push(&bytes) {
                self.pending
                    .push_back(DriverEvent::Event(Event::Log(LogEvent {
                        driver: Some(self.id),
                        message: format!("coolled serial: {line}"),
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
        let (route_count, routed_channels, sequence_count) = self.timing_counts(plan);
        self.timing.armed = true;
        self.timing.running = false;
        self.timing.route_count = route_count;
        self.timing.routed_channels = routed_channels;
        self.timing.sequence_count = sequence_count;
        self.emit_property(self.hub, "timing_state", self.timing.value());
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Arm(plan.clone())],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "coolled timing arm summary".into(),
                payload: self.timing_summary(plan, "arm"),
            }],
        })
    }

    fn start_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let (route_count, routed_channels, sequence_count) = self.timing_counts(&armed.plan);
        let value = self.write_property(self.hub, "enabled", &Value::Bool(true))?;
        self.emit_property(self.hub, "enabled", value);
        let applied = self.apply_timing_sequence_step(&armed.plan, true)?;
        self.timing.armed = true;
        self.timing.running = true;
        self.timing.route_count = route_count;
        self.timing.routed_channels = routed_channels;
        self.timing.sequence_count = sequence_count;
        self.timing.starts += 1;
        self.emit_property(self.hub, "timing_state", self.timing.value());
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Start(OperationId(0))],
            physical_transactions: vec![
                self.timing_transaction(
                    "coolled timing start global output enable",
                    protocol::CoolLedCommand::SetGlobal(true),
                ),
                PhysicalTransaction {
                    resource: Some(self.resource),
                    description: "coolled timing start summary".into(),
                    payload: with_applied(self.timing_summary(&armed.plan, "start"), applied),
                },
            ],
        })
    }

    fn stop_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let (route_count, routed_channels, sequence_count) = self.timing_counts(&armed.plan);
        let value = self.write_property(self.hub, "enabled", &Value::Bool(false))?;
        self.emit_property(self.hub, "enabled", value);
        let applied = self.apply_timing_sequence_step(&armed.plan, false)?;
        self.timing.armed = false;
        self.timing.running = false;
        self.timing.route_count = route_count;
        self.timing.routed_channels = routed_channels;
        self.timing.sequence_count = sequence_count;
        self.timing.stops += 1;
        self.emit_property(self.hub, "timing_state", self.timing.value());
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions: vec![
                self.timing_transaction(
                    "coolled timing stop global output disable",
                    protocol::CoolLedCommand::SetGlobal(false),
                ),
                PhysicalTransaction {
                    resource: Some(self.resource),
                    description: "coolled timing stop summary".into(),
                    payload: with_applied(self.timing_summary(&armed.plan, "stop"), applied),
                },
            ],
        })
    }
}

impl CoolLedPe4000Driver {
    fn issue_read_command(&mut self, device: DeviceId, key: &str) -> Result<()> {
        if device == self.hub && (key == "model" || key == "version") {
            self.refresh_pe4000_readback(&if key == "model" {
                protocol::CoolLedCommand::Model
            } else {
                protocol::CoolLedCommand::Version
            })?;
        } else if device == self.hub && key == "state_summary" {
            self.refresh_pe4000_readback(&protocol::CoolLedCommand::Status)?;
        } else if let Some(index) = self.device_index(device) {
            if let Some(channel) = protocol::Channel::from_index(index) {
                self.refresh_pe4000_readback(&protocol::CoolLedCommand::QueryChannel(channel))?;
            }
        }
        Ok(())
    }
}

fn dac_request_percent(request: &CapabilityRequest) -> Result<u8> {
    let percent = match request {
        CapabilityRequest::Dac(request) => percent_value(&request.value)?,
        _ => {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "CoolLED Dac expects CapabilityRequest::Dac",
            ));
        }
    };
    Ok(percent.clamp(0.0, 100.0).round() as u8)
}

fn percent_value(value: &Value) -> Result<f64> {
    match value {
        Value::Ratio(percent) => Ok(percent.percent()),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            "CoolLED percent value must be Ratio",
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TriggerSinkAction {
    Enable,
    Disable,
    Pulse,
}

fn trigger_sink_commands(
    request: &CapabilityRequest,
    command: impl Fn(bool) -> protocol::CoolLedCommand,
) -> Result<Vec<protocol::CoolLedCommand>> {
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
                "CoolLED TriggerSink expects None or CapabilityRequest::Trigger",
            ));
        }
    };
    Ok(match action {
        TriggerSinkAction::Enable => vec![command(true)],
        TriggerSinkAction::Disable => vec![command(false)],
        TriggerSinkAction::Pulse => vec![command(true), command(false)],
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

fn with_applied(summary: Value, applied: Value) -> Value {
    match summary {
        Value::Map(mut map) => {
            map.insert("applied".into(), applied);
            Value::Map(map)
        }
        other => other,
    }
}

fn wavelength_property(wavelengths: &[f64; 4]) -> PropertySchema {
    let mut schema = property(
        "wavelength",
        "Wavelength",
        ValueType::Wavelength,
        None,
        true,
        None,
    );
    schema.enum_values = wavelengths
        .iter()
        .map(|nm| EnumValue {
            value: Value::Wavelength(Wavelength::from_nanometers(*nm)),
            label: format!("{nm:.0} nm"),
        })
        .collect();
    schema
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
    u64_prop(device, key).and_then(|value| value.try_into().ok())
}

fn u64_prop(device: &DeviceConfig, key: &str) -> Option<u64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => (*value >= 0).then_some(*value as u64),
        Some(Value::F64(value)) if value.is_finite() && *value >= 0.0 => Some(*value as u64),
        _ => None,
    }
}

fn parse_pe300_channel_labels(labels: &str) -> Result<[String; protocol::PE300_CHANNELS]> {
    let labels = labels
        .split(',')
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    labels.try_into().map_err(|labels: Vec<String>| {
        Error::new(
            ErrorCode::InvalidProperty,
            format!(
                "CoolLED pE-300 channel_labels must contain exactly {} labels, got {}",
                protocol::PE300_CHANNELS,
                labels.len()
            ),
        )
    })
}

fn parse_wavelength_table(wavelengths: &str) -> Result<[[f64; 4]; protocol::CHANNELS]> {
    let values = wavelengths
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value.parse::<f64>().map_err(|error| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("invalid CoolLED wavelength value {value}: {error}"),
                )
            })
        })
        .collect::<Result<Vec<_>>>()?;
    if values.len() != protocol::CHANNELS * 4 {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            format!(
                "CoolLED wavelengths_nm must contain {} values, got {}",
                protocol::CHANNELS * 4,
                values.len()
            ),
        ));
    }
    let mut table = [[0.0; 4]; protocol::CHANNELS];
    for channel in 0..protocol::CHANNELS {
        for slot in 0..4 {
            table[channel][slot] = values[channel * 4 + slot];
        }
    }
    Ok(table)
}
