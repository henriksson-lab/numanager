use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::{ScriptedSerial, SerialIo};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
#[cfg(feature = "os-serial")]
use std::time::Duration;

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const CHANNELS: usize = 6;
    pub const INIT_GPIO_0_TO_3: [u8; 4] = [0x57, 0x02, 0xff, 0x50];
    pub const INIT_GPIO_5_TO_7: [u8; 4] = [0x57, 0x03, 0xab, 0x50];
    pub const TERMINATOR: u8 = 0x50;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum LightEngineKind {
        Aura,
        Sola,
        Spectra,
        SpectraX,
    }

    impl LightEngineKind {
        pub fn label(self) -> &'static str {
            match self {
                LightEngineKind::Aura => "Aura",
                LightEngineKind::Sola => "Sola",
                LightEngineKind::Spectra => "Spectra",
                LightEngineKind::SpectraX => "SpectraX",
            }
        }

        pub fn cia_code(self) -> u8 {
            match self {
                LightEngineKind::Aura => 1,
                LightEngineKind::Sola => 2,
                LightEngineKind::Spectra => 3,
                LightEngineKind::SpectraX => 4,
            }
        }

        pub fn from_label(label: &str) -> Option<Self> {
            match label {
                "Aura" => Some(LightEngineKind::Aura),
                "Sola" => Some(LightEngineKind::Sola),
                "Spectra" => Some(LightEngineKind::Spectra),
                "SpectraX" => Some(LightEngineKind::SpectraX),
                _ => None,
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ColorChannel {
        Red,
        Green,
        Cyan,
        Violet,
        Blue,
        Teal,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ChannelTriggerMode {
        Internal,
        Ttl,
        Analog,
        AnalogAndTtl,
    }

    impl ChannelTriggerMode {
        pub fn label(self) -> &'static str {
            match self {
                ChannelTriggerMode::Internal => "Internal",
                ChannelTriggerMode::Ttl => "TTL",
                ChannelTriggerMode::Analog => "Analog",
                ChannelTriggerMode::AnalogAndTtl => "Analog + TTL",
            }
        }

        pub fn from_label(label: &str) -> Option<Self> {
            match label {
                "Internal" => Some(ChannelTriggerMode::Internal),
                "TTL" => Some(ChannelTriggerMode::Ttl),
                "Analog" => Some(ChannelTriggerMode::Analog),
                "Analog + TTL" => Some(ChannelTriggerMode::AnalogAndTtl),
                _ => None,
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TtlPolarity {
        ActiveLow,
        ActiveHigh,
    }

    impl TtlPolarity {
        pub fn label(self) -> &'static str {
            match self {
                TtlPolarity::ActiveLow => "Active Low",
                TtlPolarity::ActiveHigh => "Active High",
            }
        }

        pub fn from_label(label: &str) -> Option<Self> {
            match label {
                "Active Low" => Some(TtlPolarity::ActiveLow),
                "Active High" => Some(TtlPolarity::ActiveHigh),
                _ => None,
            }
        }
    }

    impl ColorChannel {
        pub fn index(self) -> usize {
            match self {
                ColorChannel::Red => 0,
                ColorChannel::Green => 1,
                ColorChannel::Cyan => 2,
                ColorChannel::Violet => 3,
                ColorChannel::Blue => 4,
                ColorChannel::Teal => 5,
            }
        }

        pub fn label(self) -> &'static str {
            match self {
                ColorChannel::Red => "red",
                ColorChannel::Green => "green",
                ColorChannel::Cyan => "cyan",
                ColorChannel::Violet => "violet",
                ColorChannel::Blue => "blue",
                ColorChannel::Teal => "teal",
            }
        }

        pub fn display_name(self) -> &'static str {
            match self {
                ColorChannel::Red => "Red",
                ColorChannel::Green => "Green",
                ColorChannel::Cyan => "Cyan",
                ColorChannel::Violet => "Violet",
                ColorChannel::Blue => "Blue",
                ColorChannel::Teal => "Teal",
            }
        }

        pub fn bit(self) -> u8 {
            match self {
                ColorChannel::Red => 0,
                ColorChannel::Green => 1,
                ColorChannel::Cyan => 2,
                ColorChannel::Violet => 3,
                ColorChannel::Blue => 5,
                ColorChannel::Teal => 6,
            }
        }

        pub fn nominal_wavelength(self) -> Wavelength {
            match self {
                ColorChannel::Red => Wavelength::from_nanometers(635.0),
                ColorChannel::Green => Wavelength::from_nanometers(550.0),
                ColorChannel::Cyan => Wavelength::from_nanometers(475.0),
                ColorChannel::Violet => Wavelength::from_nanometers(395.0),
                ColorChannel::Blue => Wavelength::from_nanometers(438.0),
                ColorChannel::Teal => Wavelength::from_nanometers(510.0),
            }
        }

        pub fn dac_selector(self) -> (u8, u8) {
            match self {
                ColorChannel::Red => (0x18, 0x08),
                ColorChannel::Green => (0x18, 0x04),
                ColorChannel::Cyan => (0x18, 0x02),
                ColorChannel::Violet => (0x18, 0x01),
                ColorChannel::Blue => (0x1a, 0x01),
                ColorChannel::Teal => (0x1a, 0x02),
            }
        }

        pub fn from_index(index: usize) -> Option<Self> {
            match index {
                0 => Some(ColorChannel::Red),
                1 => Some(ColorChannel::Green),
                2 => Some(ColorChannel::Cyan),
                3 => Some(ColorChannel::Violet),
                4 => Some(ColorChannel::Blue),
                5 => Some(ColorChannel::Teal),
                _ => None,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct LumencorProbe {
        pub model: String,
        pub engine: LightEngineKind,
        pub channels: Vec<ColorChannel>,
    }

    impl LumencorProbe {
        pub fn configured_spectrax_fixture() -> Self {
            Self {
                model: "Lumencor SpectraX configured model".into(),
                engine: LightEngineKind::SpectraX,
                channels: vec![
                    ColorChannel::Red,
                    ColorChannel::Green,
                    ColorChannel::Cyan,
                    ColorChannel::Violet,
                    ColorChannel::Blue,
                    ColorChannel::Teal,
                ],
            }
        }

        pub fn initial_enable_mask(&self) -> u8 {
            match self.engine {
                LightEngineKind::Aura | LightEngineKind::Sola => 0xff,
                LightEngineKind::Spectra | LightEngineKind::SpectraX => 0xff & !(1 << 7),
            }
        }

        pub fn all_off_mask(&self) -> u8 {
            match self.engine {
                LightEngineKind::Aura | LightEngineKind::Sola => 0xff,
                LightEngineKind::Spectra | LightEngineKind::SpectraX => {
                    0xff & !((1 << 4) | (1 << 7))
                }
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum LumencorCommand {
        InitGpio0To3,
        InitGpio5To7,
        SetEnableMask(u8),
        SetLevel { channel: ColorChannel, percent: u8 },
        SetWhiteLevel(u8),
    }

    pub fn encode(command: &LumencorCommand) -> Vec<u8> {
        match command {
            LumencorCommand::InitGpio0To3 => INIT_GPIO_0_TO_3.to_vec(),
            LumencorCommand::InitGpio5To7 => INIT_GPIO_5_TO_7.to_vec(),
            LumencorCommand::SetEnableMask(mask) => vec![0x4f, *mask, TERMINATOR],
            LumencorCommand::SetLevel { channel, percent } => {
                let (event, color_bits) = channel.dac_selector();
                let value = level_to_dac(*percent);
                vec![
                    0x53,
                    event,
                    0x03,
                    color_bits,
                    ((value >> 4) as u8 & 0x0f) | 0xf0,
                    ((value << 4) as u8) & 0xf0,
                    TERMINATOR,
                ]
            }
            LumencorCommand::SetWhiteLevel(percent) => {
                let value = level_to_dac(*percent);
                vec![
                    0x53,
                    0x18,
                    0x03,
                    0x0f,
                    ((value >> 4) as u8 & 0x0f) | 0xf0,
                    ((value << 4) as u8) & 0xf0,
                    TERMINATOR,
                    0x53,
                    0x1a,
                    0x03,
                    0x03,
                    ((value >> 4) as u8 & 0x0f) | 0xf0,
                    ((value << 4) as u8) & 0xf0,
                    TERMINATOR,
                ]
            }
        }
    }

    pub fn level_to_dac(percent: u8) -> u16 {
        let percent = percent.min(100);
        match percent {
            0 => 0xff,
            100 => 0,
            other => 255_u16.saturating_sub((2.55 * other as f64).round() as u16),
        }
    }

    pub fn set_bit(mask: u8, bit: u8, enabled: bool) -> u8 {
        if enabled {
            mask & !(1 << bit)
        } else {
            mask | (1 << bit)
        }
    }

    pub fn bit_enabled(mask: u8, bit: u8) -> bool {
        mask & (1 << bit) == 0
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct LumencorChannelProbe {
        pub color: ColorChannel,
        pub enabled: bool,
        pub intensity_percent: u8,
        pub wavelength: Wavelength,
        pub trigger_mode: ChannelTriggerMode,
        pub ttl_polarity: TtlPolarity,
        pub analog_level_percent: u8,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct LumencorSpectraProbeResult {
        pub probe: LumencorProbe,
        pub startup_commands: Vec<Vec<u8>>,
        pub initial_enable_mask: u8,
        pub shuttered_startup_mask: u8,
        pub channels: Vec<LumencorChannelProbe>,
        pub replies: Vec<Vec<u8>>,
    }

    pub fn spectra_probe_commands(probe: &LumencorProbe) -> Vec<LumencorCommand> {
        vec![
            LumencorCommand::InitGpio0To3,
            LumencorCommand::InitGpio5To7,
            LumencorCommand::SetEnableMask(probe.initial_enable_mask() | probe.all_off_mask()),
        ]
    }

    pub fn spectra_probe_script(probe: &LumencorProbe) -> Vec<String> {
        spectra_probe_commands(probe)
            .iter()
            .map(|command| hex_bytes(&encode(command)))
            .collect()
    }

    pub fn execute_spectra_probe_script(
        serial: &mut dyn SerialIo,
        probe: &LumencorProbe,
    ) -> Result<LumencorSpectraProbeResult> {
        let mut startup_commands = Vec::new();
        let mut replies = Vec::new();
        for command in spectra_probe_commands(probe) {
            let bytes = encode(&command);
            serial.write(&bytes)?;
            let reply = serial.read_available()?;
            if !reply.is_empty() {
                replies.push(reply);
            }
            startup_commands.push(bytes);
        }
        let channels = probe
            .channels
            .iter()
            .map(|color| LumencorChannelProbe {
                color: *color,
                enabled: false,
                intensity_percent: 100,
                wavelength: color.nominal_wavelength(),
                trigger_mode: ChannelTriggerMode::Internal,
                ttl_polarity: TtlPolarity::ActiveHigh,
                analog_level_percent: 100,
            })
            .collect();
        Ok(LumencorSpectraProbeResult {
            probe: probe.clone(),
            startup_commands,
            initial_enable_mask: probe.initial_enable_mask(),
            shuttered_startup_mask: probe.initial_enable_mask() | probe.all_off_mask(),
            channels,
            replies,
        })
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CiaInputPolarity {
        Low,
        High,
    }

    impl CiaInputPolarity {
        pub fn label(self) -> &'static str {
            match self {
                CiaInputPolarity::Low => "Low",
                CiaInputPolarity::High => "High",
            }
        }

        pub fn code(self) -> u8 {
            match self {
                CiaInputPolarity::Low => 0,
                CiaInputPolarity::High => 1,
            }
        }

        pub fn from_label(label: &str) -> Option<Self> {
            match label {
                "Low" => Some(CiaInputPolarity::Low),
                "High" => Some(CiaInputPolarity::High),
                _ => None,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum CiaCommand {
        QueryInfo,
        SetEngine(LightEngineKind),
        SetInputPolarity {
            input1: CiaInputPolarity,
            input2: CiaInputPolarity,
        },
        WriteLevels([u8; 7]),
        WriteEvents(Vec<u8>),
        Run,
        Stop,
        Step,
        Rewind,
    }

    pub fn encode_cia(command: &CiaCommand) -> Vec<u8> {
        match command {
            CiaCommand::QueryInfo => b"#I\n".to_vec(),
            CiaCommand::SetEngine(engine) => format!("#E{}\n", engine.cia_code()).into_bytes(),
            CiaCommand::SetInputPolarity { input1, input2 } => {
                vec![b'#', b'P', input1.code(), input2.code(), b'\n']
            }
            CiaCommand::WriteLevels(levels) => {
                let mut bytes = b"#H\n".to_vec();
                bytes.extend_from_slice(levels);
                bytes.push(b'\n');
                bytes
            }
            CiaCommand::WriteEvents(events) => {
                let mut bytes = b"#D\n".to_vec();
                bytes.extend_from_slice(events);
                bytes.push(b'\n');
                bytes
            }
            CiaCommand::Run => b"#R\n".to_vec(),
            CiaCommand::Stop => b"#S\n".to_vec(),
            CiaCommand::Step => b"#T\n".to_vec(),
            CiaCommand::Rewind => b"#@\n".to_vec(),
        }
    }

    pub fn cia_level(percent: u8) -> u8 {
        let percent = percent.min(100);
        if percent == 100 {
            0
        } else if percent == 0 {
            0xff
        } else {
            ((100 - percent) as f64 * 2.55).round() as u8
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct LumencorCiaProbeResult {
        pub info: String,
        pub engine: LightEngineKind,
        pub input1: CiaInputPolarity,
        pub input2: CiaInputPolarity,
        pub setup_replies: Vec<String>,
    }

    pub fn cia_probe_commands(
        engine: LightEngineKind,
        input1: CiaInputPolarity,
        input2: CiaInputPolarity,
    ) -> Vec<CiaCommand> {
        vec![
            CiaCommand::QueryInfo,
            CiaCommand::SetEngine(engine),
            CiaCommand::SetInputPolarity { input1, input2 },
        ]
    }

    pub fn cia_probe_script(
        engine: LightEngineKind,
        input1: CiaInputPolarity,
        input2: CiaInputPolarity,
    ) -> Vec<String> {
        cia_probe_commands(engine, input1, input2)
            .iter()
            .map(|command| ascii_or_hex(&encode_cia(command)))
            .collect()
    }

    pub fn execute_cia_probe_script(
        serial: &mut dyn SerialIo,
        engine: LightEngineKind,
        input1: CiaInputPolarity,
        input2: CiaInputPolarity,
        polls_per_command: usize,
    ) -> Result<LumencorCiaProbeResult> {
        let commands = cia_probe_commands(engine, input1, input2);
        let mut setup_replies = Vec::new();
        let mut info = None;
        for command in commands {
            serial.write(&encode_cia(&command))?;
            match command {
                CiaCommand::QueryInfo => {
                    info = Some(read_lf_line(serial, polls_per_command)?);
                }
                _ => {
                    if let Some(reply) = read_optional_lf_line(serial, polls_per_command)? {
                        setup_replies.push(reply);
                    }
                }
            }
        }
        Ok(LumencorCiaProbeResult {
            info: info.unwrap_or_default(),
            engine,
            input1,
            input2,
            setup_replies,
        })
    }

    fn read_lf_line(serial: &mut dyn SerialIo, polls_per_command: usize) -> Result<String> {
        read_optional_lf_line(serial, polls_per_command)?.ok_or_else(|| {
            Error::new(
                ErrorCode::Transport,
                "timed out waiting for Lumencor CIA probe reply",
            )
        })
    }

    pub(crate) fn read_optional_lf_line(
        serial: &mut dyn SerialIo,
        polls_per_command: usize,
    ) -> Result<Option<String>> {
        let mut buffer = Vec::new();
        for _ in 0..polls_per_command.max(1) {
            buffer.extend(serial.read_available()?);
            if let Some(index) = buffer.iter().position(|byte| *byte == b'\n') {
                buffer.truncate(index);
                if buffer.ends_with(b"\r") {
                    buffer.pop();
                }
                return Ok(Some(String::from_utf8_lossy(&buffer).trim().into()));
            }
        }
        if buffer.is_empty() {
            Ok(None)
        } else {
            Ok(Some(String::from_utf8_lossy(&buffer).trim().into()))
        }
    }

    fn ascii_or_hex(bytes: &[u8]) -> String {
        if bytes
            .iter()
            .all(|byte| byte.is_ascii_graphic() || matches!(*byte, b' ' | b'\r' | b'\n' | b'\t'))
        {
            String::from_utf8_lossy(bytes).trim_end().into()
        } else {
            hex_bytes(bytes)
        }
    }

    fn hex_bytes(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

pub struct LumencorSpectraDiscovery {
    next_id: DriverId,
    probes: Vec<LumencorSpectraConfiguredProbe>,
}

impl LumencorSpectraDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![LumencorSpectraConfiguredProbe::configured_fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| device.driver == "lumencor-spectra")
            .map(LumencorSpectraConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for LumencorSpectraDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver = if configured.connect_real_transport {
                    Box::new(LumencorSpectraDriver::serial(id, configured)?) as Box<dyn Driver>
                } else {
                    Box::new(LumencorSpectraDriver::configured_from(id, configured))
                        as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

pub struct LumencorCiaDiscovery {
    next_id: DriverId,
    probes: Vec<LumencorCiaConfiguredProbe>,
}

impl LumencorCiaDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![LumencorCiaConfiguredProbe::configured_fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| device.driver == "lumencor-cia")
            .map(LumencorCiaConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for LumencorCiaDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver = if configured.connect_real_transport {
                    Box::new(LumencorCiaDriver::serial(id, configured)?) as Box<dyn Driver>
                } else {
                    Box::new(LumencorCiaDriver::configured_from(id, configured)) as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct LumencorSerialEndpoint {
    pub port_name: String,
    pub baud_rate: u32,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone)]
pub struct LumencorSpectraConfiguredProbe {
    pub label: String,
    pub probe: protocol::LumencorProbe,
    pub endpoint: Option<LumencorSerialEndpoint>,
    pub connect_real_transport: bool,
}

impl LumencorSpectraConfiguredProbe {
    pub fn configured_fixture() -> Self {
        Self {
            label: "Configured Lumencor SpectraX fixture".into(),
            probe: protocol::LumencorProbe::configured_spectrax_fixture(),
            endpoint: None,
            connect_real_transport: false,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::configured_fixture();
        configured.label = if device.label.is_empty() {
            configured.label
        } else {
            device.label.clone()
        };
        configured.probe.model =
            string_prop(device, "model").unwrap_or_else(|| configured.probe.model.clone());
        configured.probe.engine = string_prop(device, "engine")
            .and_then(|engine| protocol::LightEngineKind::from_label(&engine))
            .unwrap_or(configured.probe.engine);
        if let Some(channels) = string_prop(device, "channels") {
            configured.probe.channels = parse_color_channels(&channels)?;
        }
        configured.endpoint =
            string_prop(device, "serial_port").map(|port_name| LumencorSerialEndpoint {
                port_name,
                baud_rate: u32_prop(device, "baud_rate").unwrap_or(9_600),
                timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(100),
            });
        configured.connect_real_transport = bool_prop(device, "connect").unwrap_or(false);
        Ok(configured)
    }
}

#[derive(Debug, Clone)]
pub struct LumencorCiaConfiguredProbe {
    pub label: String,
    pub engine: protocol::LightEngineKind,
    pub input1: protocol::CiaInputPolarity,
    pub input2: protocol::CiaInputPolarity,
    pub endpoint: Option<LumencorSerialEndpoint>,
    pub connect_real_transport: bool,
}

impl LumencorCiaConfiguredProbe {
    pub fn configured_fixture() -> Self {
        Self {
            label: "Configured Lumencor CIA fixture".into(),
            engine: protocol::LightEngineKind::Spectra,
            input1: protocol::CiaInputPolarity::High,
            input2: protocol::CiaInputPolarity::High,
            endpoint: None,
            connect_real_transport: false,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::configured_fixture();
        configured.label = if device.label.is_empty() {
            configured.label
        } else {
            device.label.clone()
        };
        configured.engine = string_prop(device, "engine")
            .and_then(|engine| protocol::LightEngineKind::from_label(&engine))
            .unwrap_or(configured.engine);
        configured.input1 = string_prop(device, "input1_polarity")
            .and_then(|polarity| protocol::CiaInputPolarity::from_label(&polarity))
            .unwrap_or(configured.input1);
        configured.input2 = string_prop(device, "input2_polarity")
            .and_then(|polarity| protocol::CiaInputPolarity::from_label(&polarity))
            .unwrap_or(configured.input2);
        configured.endpoint =
            string_prop(device, "serial_port").map(|port_name| LumencorSerialEndpoint {
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
    color: protocol::ColorChannel,
    enabled: bool,
    intensity_percent: u8,
    wavelength: Wavelength,
    trigger_mode: protocol::ChannelTriggerMode,
    ttl_polarity: protocol::TtlPolarity,
    analog_level_percent: u8,
}

pub struct LumencorSpectraDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    channels: [DeviceId; protocol::CHANNELS],
    probe: protocol::LumencorProbe,
    open: bool,
    initialized: bool,
    enable_mask: u8,
    yg_filter_enabled: bool,
    states: Vec<ChannelState>,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
}

impl LumencorSpectraDriver {
    pub fn configured_fixture(id: DriverId) -> Self {
        Self::configured_from(id, LumencorSpectraConfiguredProbe::configured_fixture())
    }

    pub fn configured_from(id: DriverId, configured: LumencorSpectraConfiguredProbe) -> Self {
        Self::new_configured(id, configured, Box::new(ScriptedSerial::new()), false)
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: LumencorSpectraConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Lumencor Spectra serial probe is missing serial_port metadata",
            )
        })?;
        let mut serial = numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(endpoint.port_name, endpoint.baud_rate)
                .timeout(Duration::from_millis(endpoint.timeout_ms)),
        )?;
        let probe_result = protocol::execute_spectra_probe_script(&mut serial, &configured.probe)?;
        Ok(Self::new_configured(id, configured, Box::new(serial), true)
            .with_probe_result(probe_result))
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _configured: LumencorSpectraConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Lumencor Spectra real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(id: DriverId, probe: protocol::LumencorProbe, serial: Box<dyn SerialIo>) -> Self {
        let states = probe
            .channels
            .iter()
            .map(|color| ChannelState {
                color: *color,
                enabled: false,
                intensity_percent: 100,
                wavelength: color.nominal_wavelength(),
                trigger_mode: protocol::ChannelTriggerMode::Internal,
                ttl_polarity: protocol::TtlPolarity::ActiveHigh,
                analog_level_percent: 100,
            })
            .collect::<Vec<_>>();
        let enable_mask = probe.initial_enable_mask();
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 1701)),
            hub: DeviceId(NodeId(id.0 * 1000 + 1710)),
            channels: [
                DeviceId(NodeId(id.0 * 1000 + 1711)),
                DeviceId(NodeId(id.0 * 1000 + 1712)),
                DeviceId(NodeId(id.0 * 1000 + 1713)),
                DeviceId(NodeId(id.0 * 1000 + 1714)),
                DeviceId(NodeId(id.0 * 1000 + 1715)),
                DeviceId(NodeId(id.0 * 1000 + 1716)),
            ],
            probe,
            open: false,
            initialized: false,
            enable_mask,
            yg_filter_enabled: false,
            states,
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            serial_port: None,
            baud_rate: 9_600,
            serial_timeout_ms: 100,
            connected: false,
        }
    }

    pub fn new_configured(
        id: DriverId,
        configured: LumencorSpectraConfiguredProbe,
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
    fn with_probe_result(mut self, probe_result: protocol::LumencorSpectraProbeResult) -> Self {
        self.probe = probe_result.probe;
        self.enable_mask = probe_result.initial_enable_mask;
        self.open = false;
        self.initialized = true;
        self.states = probe_result
            .channels
            .into_iter()
            .map(|channel| ChannelState {
                color: channel.color,
                enabled: channel.enabled,
                intensity_percent: channel.intensity_percent,
                wavelength: channel.wavelength,
                trigger_mode: channel.trigger_mode,
                ttl_polarity: channel.ttl_polarity,
                analog_level_percent: channel.analog_level_percent,
            })
            .collect();
        self
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn send(&mut self, command: protocol::LumencorCommand) -> Result<()> {
        self.serial.write(&protocol::encode(&command))
    }

    fn invoke_transaction(
        &self,
        description: &str,
        command: protocol::LumencorCommand,
    ) -> PhysicalTransaction {
        PhysicalTransaction {
            resource: Some(self.resource),
            description: description.into(),
            payload: Value::Bytes(protocol::encode(&command)),
        }
    }

    fn ensure_startup(&mut self) -> Result<()> {
        if !self.initialized {
            self.send(protocol::LumencorCommand::InitGpio0To3)?;
            self.send(protocol::LumencorCommand::InitGpio5To7)?;
            self.send(protocol::LumencorCommand::SetEnableMask(
                self.shuttered_mask(),
            ))?;
            self.initialized = true;
        }
        Ok(())
    }

    fn shuttered_mask(&self) -> u8 {
        if self.open {
            self.enable_mask
        } else {
            self.enable_mask | self.probe.all_off_mask()
        }
    }

    fn channel_index(&self, device: DeviceId) -> Option<usize> {
        self.channels
            .iter()
            .position(|candidate| *candidate == device)
    }

    fn trigger_profile(&self) -> String {
        self.states
            .iter()
            .map(|state| {
                format!(
                    "{}:{}:{}:{}%",
                    state.color.label(),
                    state.trigger_mode.label(),
                    state.ttl_polarity.label(),
                    state.analog_level_percent
                )
            })
            .collect::<Vec<_>>()
            .join(";")
    }

    fn set_channel_enabled(&mut self, index: usize, enabled: bool) -> Result<()> {
        let bit = self.states[index].color.bit();
        self.enable_mask = protocol::set_bit(self.enable_mask, bit, enabled);
        self.states[index].enabled = enabled;
        if self.open {
            self.send(protocol::LumencorCommand::SetEnableMask(
                self.shuttered_mask(),
            ))?;
        }
        Ok(())
    }

    fn set_open(&mut self, open: bool) -> Result<()> {
        if self.open != open {
            self.open = open;
            self.send(protocol::LumencorCommand::SetEnableMask(
                self.shuttered_mask(),
            ))?;
        }
        Ok(())
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        let mut descriptors = vec![DeviceDescriptor {
            id: self.hub,
            driver: self.id,
            label: "lumencor-spectra-hub".into(),
            vendor: Some("Lumencor".into()),
            model: Some(self.probe.model.clone()),
            serial: None,
            kinds: vec!["hub".into(), "light.engine".into(), "shutter".into()],
            properties: vec![
                property("model", "Model", ValueType::String, None, false, None),
                sequenceable_property("open", "Open", ValueType::Bool, None, true, None),
                property(
                    "enable_mask",
                    "Enable mask",
                    ValueType::I64,
                    None,
                    false,
                    None,
                ),
                property(
                    "trigger_profile",
                    "Trigger profile",
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
                property("yg_filter", "YG filter", ValueType::Bool, None, true, None),
            ],
            metadata: BTreeMap::from([
                (
                    "startup".into(),
                    Value::String("57 02 ff 50; 57 03 ab 50; 4f <mask> 50".into()),
                ),
                (
                    "completion".into(),
                    Value::String(
                        "serial write acceptance; legacy Spectra command set has no ACK".into(),
                    ),
                ),
                (
                    "startup_readback_supported".into(),
                    Value::List(
                        protocol::spectra_probe_script(&self.probe)
                            .into_iter()
                            .map(Value::String)
                            .collect(),
                    ),
                ),
            ]),
        }];

        for (index, state) in self.states.iter().enumerate() {
            descriptors.push(DeviceDescriptor {
                id: self.channels[index],
                driver: self.id,
                label: format!("lumencor-{}", state.color.label()),
                vendor: Some("Lumencor".into()),
                model: Some(self.probe.model.clone()),
                serial: None,
                kinds: vec![
                    "light.source".into(),
                    "led.channel".into(),
                    "trigger.sink".into(),
                ],
                properties: vec![
                    sequenceable_property("enabled", "Enabled", ValueType::Bool, None, true, None),
                    sequenceable_ratio_property_range(
                        "intensity",
                        "Intensity",
                        Some("percent"),
                        true,
                        0.0,
                        100.0,
                    ),
                    enum_property(
                        "trigger_mode",
                        "Trigger mode",
                        true,
                        &["Internal", "TTL", "Analog", "Analog + TTL"],
                    ),
                    enum_property(
                        "ttl_polarity",
                        "TTL polarity",
                        true,
                        &["Active Low", "Active High"],
                    ),
                    ratio_property_range(
                        "analog_level",
                        "Analog level",
                        Some("percent"),
                        true,
                        0.0,
                        100.0,
                    ),
                    property(
                        "wavelength",
                        "Wavelength",
                        ValueType::Wavelength,
                        Some("nm"),
                        false,
                        None,
                    ),
                ],
                metadata: BTreeMap::from([
                    (
                        "color".into(),
                        Value::String(state.color.display_name().into()),
                    ),
                    ("mask_bit".into(), Value::I64(state.color.bit() as i64)),
                    ("wavelength".into(), Value::Wavelength(state.wavelength)),
                    (
                        "trigger_surface".into(),
                        Value::String(
                            "fixture-level TTL/analog mode selection; legacy Spectra serial validates state while CIA/hardware traces validate timing"
                                .into(),
                        ),
                    ),
                ]),
            });
        }
        descriptors
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "model" => Ok(Value::String(self.probe.model.clone())),
                "open" => Ok(Value::Bool(self.open)),
                "enable_mask" => Ok(Value::I64(self.enable_mask as i64)),
                "trigger_profile" => Ok(Value::String(self.trigger_profile())),
                "state_summary" => Ok(self.state_summary()),
                "yg_filter" => Ok(Value::Bool(self.yg_filter_enabled)),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "unknown Lumencor hub property",
                )),
            };
        }
        let Some(index) = self.channel_index(device) else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown Lumencor device",
            ));
        };
        match key {
            "enabled" => Ok(Value::Bool(self.states[index].enabled)),
            "intensity" => Ok(Value::Ratio(Ratio::from_percent(
                self.states[index].intensity_percent as f64,
            ))),
            "trigger_mode" => Ok(Value::String(
                self.states[index].trigger_mode.label().into(),
            )),
            "ttl_polarity" => Ok(Value::String(
                self.states[index].ttl_polarity.label().into(),
            )),
            "analog_level" => Ok(Value::Ratio(Ratio::from_percent(
                self.states[index].analog_level_percent as f64,
            ))),
            "wavelength" => Ok(Value::Wavelength(self.states[index].wavelength)),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Lumencor channel property {key}"),
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
        self.ensure_startup()?;
        if device == self.hub {
            return match (key, value) {
                ("open", Value::Bool(open)) => {
                    self.set_open(*open)?;
                    Ok(Value::Bool(self.open))
                }
                ("yg_filter", Value::Bool(enabled)) => {
                    self.yg_filter_enabled = *enabled;
                    self.enable_mask = protocol::set_bit(self.enable_mask, 4, *enabled);
                    self.send(protocol::LumencorCommand::SetEnableMask(
                        self.shuttered_mask(),
                    ))?;
                    Ok(Value::Bool(self.yg_filter_enabled))
                }
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "invalid Lumencor hub write",
                )),
            };
        }
        let index = self
            .channel_index(device)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown Lumencor channel"))?;
        match (key, value) {
            ("enabled", Value::Bool(enabled)) => {
                self.set_channel_enabled(index, *enabled)?;
                Ok(Value::Bool(self.states[index].enabled))
            }
            ("intensity", Value::Ratio(percent)) => {
                let percent = percent.percent().clamp(0.0, 100.0).round() as u8;
                self.states[index].intensity_percent = percent;
                self.send(protocol::LumencorCommand::SetLevel {
                    channel: self.states[index].color,
                    percent,
                })?;
                Ok(Value::Ratio(Ratio::from_percent(percent as f64)))
            }
            ("trigger_mode", Value::String(mode)) => {
                let mode = protocol::ChannelTriggerMode::from_label(mode).ok_or_else(|| {
                    Error::new(ErrorCode::InvalidProperty, "unknown Lumencor trigger mode")
                })?;
                self.states[index].trigger_mode = mode;
                Ok(Value::String(mode.label().into()))
            }
            ("ttl_polarity", Value::String(polarity)) => {
                let polarity = protocol::TtlPolarity::from_label(polarity).ok_or_else(|| {
                    Error::new(ErrorCode::InvalidProperty, "unknown Lumencor TTL polarity")
                })?;
                self.states[index].ttl_polarity = polarity;
                Ok(Value::String(polarity.label().into()))
            }
            ("analog_level", Value::Ratio(percent)) => {
                let percent = percent.percent().clamp(0.0, 100.0).round() as u8;
                self.states[index].analog_level_percent = percent;
                Ok(Value::Ratio(Ratio::from_percent(percent as f64)))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid Lumencor write {key}"),
            )),
        }
    }

    fn invoke_transactions(
        &self,
        device: DeviceId,
        kind: CapabilityKind,
        request: &CapabilityRequest,
    ) -> Result<Vec<protocol::LumencorCommand>> {
        match kind {
            CapabilityKind::Dac => {
                let index = self.channel_index(device).ok_or_else(|| {
                    Error::new(
                        ErrorCode::InvalidCommand,
                        "Lumencor Spectra Dac invocation requires a channel device",
                    )
                })?;
                Ok(vec![protocol::LumencorCommand::SetLevel {
                    channel: self.states[index].color,
                    percent: dac_request_percent(request)?,
                }])
            }
            CapabilityKind::TriggerSink => {
                let commands = trigger_sink_actions(request)?;
                if device == self.hub {
                    return Ok(commands
                        .into_iter()
                        .map(|open| {
                            let mask = if open {
                                self.enable_mask
                            } else {
                                self.enable_mask | self.probe.all_off_mask()
                            };
                            protocol::LumencorCommand::SetEnableMask(mask)
                        })
                        .collect());
                }
                let index = self.channel_index(device).ok_or_else(|| {
                    Error::new(
                        ErrorCode::InvalidCommand,
                        "Lumencor Spectra TriggerSink invocation requires the hub or a channel device",
                    )
                })?;
                let bit = self.states[index].color.bit();
                let mut mask = self.enable_mask;
                Ok(commands
                    .into_iter()
                    .map(|enabled| {
                        mask = protocol::set_bit(mask, bit, enabled);
                        let shuttered_mask = if self.open {
                            mask
                        } else {
                            mask | self.probe.all_off_mask()
                        };
                        protocol::LumencorCommand::SetEnableMask(shuttered_mask)
                    })
                    .collect())
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Lumencor Spectra invocation capability",
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
                let actions = trigger_sink_actions(&request)?;
                for enabled in &actions {
                    if device == self.hub {
                        let value =
                            self.write_property(self.hub, "open", &Value::Bool(*enabled))?;
                        self.emit_property(self.hub, "open", value);
                        self.emit_property(
                            self.hub,
                            "enable_mask",
                            Value::I64(self.enable_mask as i64),
                        );
                    } else {
                        let value =
                            self.write_property(device, "enabled", &Value::Bool(*enabled))?;
                        self.emit_property(device, "enabled", value);
                    }
                }
                self.emit_property(
                    self.hub,
                    "trigger_profile",
                    Value::String(self.trigger_profile()),
                );
                let enabled = if device == self.hub {
                    self.open
                } else {
                    let index = self.channel_index(device).ok_or_else(|| {
                        Error::new(ErrorCode::InvalidCommand, "unknown Lumencor channel")
                    })?;
                    self.states[index].enabled
                };
                Ok(Value::Map(BTreeMap::from([
                    ("triggered".into(), Value::Bool(true)),
                    ("enabled".into(), Value::Bool(enabled)),
                    ("commands".into(), Value::I64(actions.len() as i64)),
                ])))
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Lumencor Spectra invocation capability",
            )),
        }
    }

    fn apply_state_set(&mut self, set: StateSet) -> Result<Value> {
        let mut changed = BTreeMap::new();
        self.ensure_startup()?;
        let mut mask_dirty = false;
        let mut next_open = self.open;
        let mut intensity_writes = Vec::new();

        for write in set.writes {
            self.validate_write(write.device, &write.property, &write.value)?;
            if write.device == self.hub && write.property == "open" {
                if let Value::Bool(open) = write.value {
                    next_open = open;
                    mask_dirty = true;
                    changed.insert(format!("{}:open", (self.hub.0).0), Value::Bool(open));
                }
                continue;
            }
            if write.device == self.hub && write.property == "yg_filter" {
                if let Value::Bool(enabled) = write.value {
                    self.yg_filter_enabled = enabled;
                    self.enable_mask = protocol::set_bit(self.enable_mask, 4, enabled);
                    mask_dirty = true;
                    changed.insert(
                        format!("{}:yg_filter", (self.hub.0).0),
                        Value::Bool(enabled),
                    );
                }
                continue;
            }
            if let Some(index) = self.channel_index(write.device) {
                match (write.property.as_str(), write.value) {
                    ("enabled", Value::Bool(enabled)) => {
                        self.states[index].enabled = enabled;
                        self.enable_mask = protocol::set_bit(
                            self.enable_mask,
                            self.states[index].color.bit(),
                            enabled,
                        );
                        mask_dirty = true;
                        changed.insert(
                            format!("{}:enabled", (write.device.0).0),
                            Value::Bool(enabled),
                        );
                    }
                    ("intensity", Value::Ratio(percent)) => {
                        let percent = percent.percent().clamp(0.0, 100.0).round() as u8;
                        self.states[index].intensity_percent = percent;
                        intensity_writes.push((index, percent));
                        changed.insert(
                            format!("{}:intensity", (write.device.0).0),
                            Value::Ratio(Ratio::from_percent(percent as f64)),
                        );
                    }
                    ("trigger_mode", Value::String(mode)) => {
                        let mode =
                            protocol::ChannelTriggerMode::from_label(&mode).ok_or_else(|| {
                                Error::new(
                                    ErrorCode::InvalidProperty,
                                    "unknown Lumencor trigger mode",
                                )
                            })?;
                        self.states[index].trigger_mode = mode;
                        changed.insert(
                            format!("{}:trigger_mode", (write.device.0).0),
                            Value::String(mode.label().into()),
                        );
                    }
                    ("ttl_polarity", Value::String(polarity)) => {
                        let polarity =
                            protocol::TtlPolarity::from_label(&polarity).ok_or_else(|| {
                                Error::new(
                                    ErrorCode::InvalidProperty,
                                    "unknown Lumencor TTL polarity",
                                )
                            })?;
                        self.states[index].ttl_polarity = polarity;
                        changed.insert(
                            format!("{}:ttl_polarity", (write.device.0).0),
                            Value::String(polarity.label().into()),
                        );
                    }
                    ("analog_level", Value::Ratio(percent)) => {
                        let percent = percent.percent().clamp(0.0, 100.0).round() as u8;
                        self.states[index].analog_level_percent = percent;
                        changed.insert(
                            format!("{}:analog_level", (write.device.0).0),
                            Value::Ratio(Ratio::from_percent(percent as f64)),
                        );
                    }
                    _ => {}
                }
            }
        }

        for (index, percent) in intensity_writes {
            self.send(protocol::LumencorCommand::SetLevel {
                channel: self.states[index].color,
                percent,
            })?;
            self.emit_property(
                self.channels[index],
                "intensity",
                Value::Ratio(Ratio::from_percent(percent as f64)),
            );
        }

        if mask_dirty {
            self.open = next_open;
            self.send(protocol::LumencorCommand::SetEnableMask(
                self.shuttered_mask(),
            ))?;
            self.emit_property(self.hub, "open", Value::Bool(self.open));
            self.emit_property(self.hub, "enable_mask", Value::I64(self.enable_mask as i64));
            for index in 0..self.states.len() {
                self.emit_property(
                    self.channels[index],
                    "enabled",
                    Value::Bool(self.states[index].enabled),
                );
            }
        }
        self.emit_property(
            self.hub,
            "trigger_profile",
            Value::String(self.trigger_profile()),
        );
        Ok(Value::Map(changed))
    }

    fn timing_targets(&self, plan: &TimingPlan) -> (bool, Vec<usize>) {
        let hub = plan.participants.contains(&self.hub);
        let mut channels = plan
            .participants
            .iter()
            .filter_map(|device| self.channel_index(*device))
            .collect::<Vec<_>>();
        channels.sort_unstable();
        channels.dedup();
        (hub, channels)
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        let (hub, channels) = self.timing_targets(plan);
        if !hub && channels.is_empty() {
            return Ok(());
        }

        for sequence in &plan.sequences {
            if sequence.device == self.hub {
                if sequence.property != "open" {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Lumencor Spectra hub timing sequences can only target open",
                    ));
                }
            } else if self.channel_index(sequence.device).is_some() {
                if sequence.property != "enabled" && sequence.property != "intensity" {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Lumencor Spectra channel timing sequences can only target enabled or intensity",
                    ));
                }
            } else {
                continue;
            }
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

    fn state_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("hub".into(), Value::I64(self.hub.0 .0 as i64)),
            ("model".into(), Value::String(self.probe.model.clone())),
            ("initialized".into(), Value::Bool(self.initialized)),
            ("open".into(), Value::Bool(self.open)),
            ("enable_mask".into(), Value::I64(self.enable_mask as i64)),
            (
                "shuttered_mask".into(),
                Value::I64(self.shuttered_mask() as i64),
            ),
            (
                "all_off_mask".into(),
                Value::I64(self.probe.all_off_mask() as i64),
            ),
            ("yg_filter".into(), Value::Bool(self.yg_filter_enabled)),
            (
                "trigger_profile".into(),
                Value::String(self.trigger_profile()),
            ),
            (
                "startup_commands".into(),
                Value::List(
                    [
                        protocol::LumencorCommand::InitGpio0To3,
                        protocol::LumencorCommand::InitGpio5To7,
                        protocol::LumencorCommand::SetEnableMask(self.shuttered_mask()),
                    ]
                    .into_iter()
                    .map(|command| Value::Bytes(protocol::encode(&command)))
                    .collect(),
                ),
            ),
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
                                ("color".into(), Value::String(state.color.label().into())),
                                (
                                    "display_name".into(),
                                    Value::String(state.color.display_name().into()),
                                ),
                                ("mask_bit".into(), Value::I64(state.color.bit() as i64)),
                                ("enabled".into(), Value::Bool(state.enabled)),
                                (
                                    "intensity".into(),
                                    Value::Ratio(Ratio::from_percent(
                                        state.intensity_percent as f64,
                                    )),
                                ),
                                ("wavelength".into(), Value::Wavelength(state.wavelength)),
                                (
                                    "trigger_mode".into(),
                                    Value::String(state.trigger_mode.label().into()),
                                ),
                                (
                                    "ttl_polarity".into(),
                                    Value::String(state.ttl_polarity.label().into()),
                                ),
                                (
                                    "analog_level".into(),
                                    Value::Ratio(Ratio::from_percent(
                                        state.analog_level_percent as f64,
                                    )),
                                ),
                            ]))
                        })
                        .collect(),
                ),
            ),
        ]))
    }

    fn timing_summary(&self, plan: &TimingPlan, phase: &str) -> Value {
        let (hub, channels) = self.timing_targets(plan);
        let channel_values = channels
            .iter()
            .map(|index| {
                Value::Map(BTreeMap::from([
                    (
                        "device".into(),
                        Value::I64(self.channels[*index].0 .0 as i64),
                    ),
                    (
                        "color".into(),
                        Value::String(self.states[*index].color.label().into()),
                    ),
                    ("enabled".into(), Value::Bool(self.states[*index].enabled)),
                ]))
            })
            .collect::<Vec<_>>();

        Value::Map(BTreeMap::from([
            ("phase".into(), Value::String(phase.into())),
            ("hub".into(), Value::I64(self.hub.0 .0 as i64)),
            ("hub_participant".into(), Value::Bool(hub)),
            ("open".into(), Value::Bool(self.open)),
            ("enable_mask".into(), Value::I64(self.enable_mask as i64)),
            (
                "shuttered_mask".into(),
                Value::I64(self.shuttered_mask() as i64),
            ),
            ("channels".into(), Value::List(channel_values)),
            (
                "sequences".into(),
                Value::I64(
                    plan.sequences
                        .iter()
                        .filter(|sequence| {
                            sequence.device == self.hub
                                || self.channel_index(sequence.device).is_some()
                        })
                        .count() as i64,
                ),
            ),
        ]))
    }

    fn apply_timing_transition(&mut self, plan: &TimingPlan, start: bool) -> Result<Value> {
        self.ensure_startup()?;
        let (hub, channels) = self.timing_targets(plan);
        let sequence_writes = plan
            .sequences
            .iter()
            .filter_map(|sequence| {
                if sequence.device != self.hub && self.channel_index(sequence.device).is_none() {
                    return None;
                }
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

        if !sequence_writes.is_empty() {
            return self.apply_state_set(StateSet {
                name: Some(if start {
                    "lumencor spectra timing start".into()
                } else {
                    "lumencor spectra timing stop".into()
                }),
                writes: sequence_writes,
                commit: CommitMode::Immediate,
            });
        }

        if start {
            for index in channels {
                let value =
                    self.write_property(self.channels[index], "enabled", &Value::Bool(true))?;
                self.emit_property(self.channels[index], "enabled", value);
            }
            if hub {
                let value = self.write_property(self.hub, "open", &Value::Bool(true))?;
                self.emit_property(self.hub, "open", value);
                self.emit_property(self.hub, "enable_mask", Value::I64(self.enable_mask as i64));
            }
        } else {
            if hub {
                let value = self.write_property(self.hub, "open", &Value::Bool(false))?;
                self.emit_property(self.hub, "open", value);
                self.emit_property(self.hub, "enable_mask", Value::I64(self.enable_mask as i64));
            }
            for index in channels {
                let value =
                    self.write_property(self.channels[index], "enabled", &Value::Bool(false))?;
                self.emit_property(self.channels[index], "enabled", value);
            }
        }
        self.emit_property(
            self.hub,
            "trigger_profile",
            Value::String(self.trigger_profile()),
        );
        Ok(Value::Map(BTreeMap::new()))
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

impl Driver for LumencorSpectraDriver {
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
            label: "lumencor-spectra-serial".into(),
            kind: "serial.binary".into(),
            metadata: BTreeMap::from([
                (
                    "protocol".into(),
                    Value::String("Lumencor Spectra legacy serial".into()),
                ),
                (
                    "completion".into(),
                    Value::String(
                        "command writes have no ACK in legacy Spectra command set".into(),
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
                        protocol::spectra_probe_script(&self.probe)
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
            vec![capability(1, device, CapabilityKind::TriggerSink)]
        } else if self.channel_index(device).is_some() {
            vec![
                capability(1, device, CapabilityKind::TriggerSink),
                capability(2, device, CapabilityKind::Dac),
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
                        description: format!("lumencor read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("lumencor write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "lumencor remultiplexed light state set".into(),
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
                            "unknown Lumencor Spectra capability",
                        ));
                    };
                    if !capability.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "Lumencor Spectra {:?} expects {:?}, got {:?}",
                                capability.kind,
                                capability.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    for command in self.invoke_transactions(*device, capability.kind, request)? {
                        physical_transactions
                            .push(self.invoke_transaction("lumencor direct invocation", command));
                    }
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
                    let Some(capability) = self
                        .capabilities(device)
                        .into_iter()
                        .find(|candidate| candidate.id == capability)
                    else {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "unknown Lumencor Spectra capability",
                        ));
                    };
                    if !capability.accepts_request(&request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "Lumencor Spectra {:?} expects {:?}, got {:?}",
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
                description: "lumencor spectra timing arm summary".into(),
                payload: self.timing_summary(plan, "arm"),
            }],
        })
    }

    fn start_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let applied = self.apply_timing_transition(&armed.plan, true)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Start(OperationId(0))],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "lumencor spectra timing start gate".into(),
                payload: with_applied(self.timing_summary(&armed.plan, "start"), applied),
            }],
        })
    }

    fn stop_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let applied = self.apply_timing_transition(&armed.plan, false)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "lumencor spectra timing stop gate".into(),
                payload: with_applied(self.timing_summary(&armed.plan, "stop"), applied),
            }],
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CiaRunState {
    Uninitialized,
    Ready,
    Running,
    Stopped,
}

impl CiaRunState {
    fn label(self) -> &'static str {
        match self {
            CiaRunState::Uninitialized => "Uninitialized",
            CiaRunState::Ready => "Ready",
            CiaRunState::Running => "Running",
            CiaRunState::Stopped => "Stopped",
        }
    }
}

pub struct LumencorCiaDriver {
    id: DriverId,
    resource: ResourceId,
    cia: DeviceId,
    engine: protocol::LightEngineKind,
    input1: protocol::CiaInputPolarity,
    input2: protocol::CiaInputPolarity,
    info: String,
    levels: [u8; 7],
    events: Vec<u8>,
    run_state: CiaRunState,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
}

impl LumencorCiaDriver {
    pub fn configured_fixture(id: DriverId) -> Self {
        Self::configured_from(id, LumencorCiaConfiguredProbe::configured_fixture())
    }

    pub fn configured_from(id: DriverId, configured: LumencorCiaConfiguredProbe) -> Self {
        Self::new_configured(id, configured, Box::new(ScriptedSerial::new()), false)
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: LumencorCiaConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Lumencor CIA serial probe is missing serial_port metadata",
            )
        })?;
        let mut serial = numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(endpoint.port_name, endpoint.baud_rate)
                .timeout(Duration::from_millis(endpoint.timeout_ms)),
        )?;
        let probe_result = protocol::execute_cia_probe_script(
            &mut serial,
            configured.engine,
            configured.input1,
            configured.input2,
            4,
        )?;
        Ok(Self::new_configured(id, configured, Box::new(serial), true)
            .with_probe_result(probe_result))
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _configured: LumencorCiaConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Lumencor CIA real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(id: DriverId, serial: Box<dyn SerialIo>) -> Self {
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 1721)),
            cia: DeviceId(NodeId(id.0 * 1000 + 1722)),
            engine: protocol::LightEngineKind::Spectra,
            input1: protocol::CiaInputPolarity::High,
            input2: protocol::CiaInputPolarity::High,
            info: String::new(),
            levels: [0; 7],
            events: Vec::new(),
            run_state: CiaRunState::Uninitialized,
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            serial_port: None,
            baud_rate: 9_600,
            serial_timeout_ms: 100,
            connected: false,
        }
    }

    pub fn new_configured(
        id: DriverId,
        configured: LumencorCiaConfiguredProbe,
        serial: Box<dyn SerialIo>,
        connected: bool,
    ) -> Self {
        let mut driver = Self::new(id, serial);
        driver.engine = configured.engine;
        driver.input1 = configured.input1;
        driver.input2 = configured.input2;
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
    fn with_probe_result(mut self, probe_result: protocol::LumencorCiaProbeResult) -> Self {
        self.engine = probe_result.engine;
        self.input1 = probe_result.input1;
        self.input2 = probe_result.input2;
        self.info = probe_result.info;
        self.run_state = CiaRunState::Ready;
        self
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn send(&mut self, command: protocol::CiaCommand) -> Result<()> {
        self.serial.write(&protocol::encode_cia(&command))
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        vec![DeviceDescriptor {
            id: self.cia,
            driver: self.id,
            label: "lumencor-cia".into(),
            vendor: Some("Lumencor".into()),
            model: Some("Camera Interface Adapter".into()),
            serial: None,
            kinds: vec![
                "trigger.controller".into(),
                "pulse.program".into(),
                "light.engine.adapter".into(),
            ],
            properties: vec![
                enum_property(
                    "engine",
                    "Light engine",
                    true,
                    &["Aura", "Sola", "Spectra", "SpectraX"],
                ),
                enum_property("input1_polarity", "Input 1 polarity", true, &["Low", "High"]),
                enum_property("input2_polarity", "Input 2 polarity", true, &["Low", "High"]),
                property("info", "Info", ValueType::String, None, false, None),
                property("levels", "Color levels", ValueType::Bytes, None, true, None),
                property("events", "Event masks", ValueType::Bytes, None, true, None),
                property("event_count", "Event count", ValueType::I64, None, false, None),
                property("run_state", "Run state", ValueType::String, None, false, None),
            ],
            metadata: BTreeMap::from([
                (
                    "color_order".into(),
                    Value::String("Violet,Cyan,Green,Red,Blue,Teal,Yellow".into()),
                ),
                (
                    "protocol".into(),
                    Value::String("Lumencor CIA #H/#D/#E/#P/#R/#S/#T/#@/#I newline commands".into()),
                ),
                (
                    "completion".into(),
                    Value::String("CIA command response prefix for configured hardware; fixture completes on write".into()),
                ),
                (
                    "startup_readback_supported".into(),
                    Value::List(
                        protocol::cia_probe_script(self.engine, self.input1, self.input2)
                            .into_iter()
                            .map(Value::String)
                            .collect(),
                    ),
                ),
            ]),
        }]
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device != self.cia {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown Lumencor CIA device",
            ));
        }
        match key {
            "engine" => Ok(Value::String(self.engine.label().into())),
            "input1_polarity" => Ok(Value::String(self.input1.label().into())),
            "input2_polarity" => Ok(Value::String(self.input2.label().into())),
            "info" => Ok(Value::String(self.info.clone())),
            "levels" => Ok(Value::Bytes(self.levels.to_vec())),
            "events" => Ok(Value::Bytes(self.events.clone())),
            "event_count" => Ok(Value::I64(self.events.len() as i64)),
            "run_state" => Ok(Value::String(self.run_state.label().into())),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Lumencor CIA property {key}"),
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
        if device != self.cia {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown Lumencor CIA device",
            ));
        }
        match (key, value) {
            ("engine", Value::String(engine)) => {
                let engine = protocol::LightEngineKind::from_label(engine).ok_or_else(|| {
                    Error::new(ErrorCode::InvalidProperty, "unknown Lumencor engine")
                })?;
                self.engine = engine;
                self.send(protocol::CiaCommand::SetEngine(engine))?;
                self.refresh_cia_info()?;
                Ok(Value::String(engine.label().into()))
            }
            ("input1_polarity", Value::String(label)) => {
                self.input1 = protocol::CiaInputPolarity::from_label(label).ok_or_else(|| {
                    Error::new(ErrorCode::InvalidProperty, "unknown input polarity")
                })?;
                self.send(protocol::CiaCommand::SetInputPolarity {
                    input1: self.input1,
                    input2: self.input2,
                })?;
                self.refresh_cia_info()?;
                Ok(Value::String(self.input1.label().into()))
            }
            ("input2_polarity", Value::String(label)) => {
                self.input2 = protocol::CiaInputPolarity::from_label(label).ok_or_else(|| {
                    Error::new(ErrorCode::InvalidProperty, "unknown input polarity")
                })?;
                self.send(protocol::CiaCommand::SetInputPolarity {
                    input1: self.input1,
                    input2: self.input2,
                })?;
                self.refresh_cia_info()?;
                Ok(Value::String(self.input2.label().into()))
            }
            ("levels", Value::Bytes(levels)) => {
                if levels.len() != 7 {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Lumencor CIA levels must contain 7 bytes",
                    ));
                }
                self.levels.copy_from_slice(levels);
                self.send(protocol::CiaCommand::WriteLevels(self.levels))?;
                Ok(Value::Bytes(self.levels.to_vec()))
            }
            ("events", Value::Bytes(events)) => {
                if events.len() > 255 {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Lumencor CIA fixture limits events to 255 masks",
                    ));
                }
                self.events = events.clone();
                self.send(protocol::CiaCommand::WriteEvents(self.events.clone()))?;
                self.run_state = CiaRunState::Ready;
                Ok(Value::Bytes(self.events.clone()))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid Lumencor CIA write {key}"),
            )),
        }
    }

    fn apply_state_set(&mut self, set: StateSet) -> Result<Value> {
        let mut changed = BTreeMap::new();
        for write in set.writes {
            let value = self.write_property(write.device, &write.property, &write.value)?;
            self.emit_property(write.device, &write.property, value.clone());
            changed.insert(format!("{}:{}", (write.device.0).0, write.property), value);
        }
        self.emit_property(
            self.cia,
            "event_count",
            Value::I64(self.events.len() as i64),
        );
        self.emit_property(
            self.cia,
            "run_state",
            Value::String(self.run_state.label().into()),
        );
        Ok(Value::Map(changed))
    }

    fn invoke_generic(&mut self, request: GenericCommandRequest) -> Result<Value> {
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
                "Lumencor CIA GenericCommand commands do not accept params",
            ));
        }
        match request.command.as_str() {
            "run" => {
                self.send(protocol::CiaCommand::Run)?;
                self.run_state = CiaRunState::Running;
            }
            "stop" => {
                self.send(protocol::CiaCommand::Stop)?;
                self.run_state = CiaRunState::Stopped;
            }
            "step" => {
                self.send(protocol::CiaCommand::Step)?;
                self.run_state = CiaRunState::Ready;
            }
            "rewind" => {
                self.send(protocol::CiaCommand::Rewind)?;
                self.run_state = CiaRunState::Ready;
            }
            "info" => {
                self.refresh_cia_info()?;
            }
            other => {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    format!("unsupported Lumencor CIA command {other}"),
                ))
            }
        }
        self.emit_property(
            self.cia,
            "run_state",
            Value::String(self.run_state.label().into()),
        );
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("info".into(), Value::String(self.info.clone())),
            (
                "run_state".into(),
                Value::String(self.run_state.label().into()),
            ),
        ])))
    }

    fn cia_invoke_commands(
        &self,
        kind: CapabilityKind,
        request: &CapabilityRequest,
    ) -> Result<Vec<protocol::CiaCommand>> {
        match kind {
            CapabilityKind::PulseProgram => match request {
                CapabilityRequest::PulseProgram(request) => {
                    validate_configured_cia_program_request(request)?;
                    Ok(self.configured_cia_program_commands())
                }
                CapabilityRequest::None => Ok(self.configured_cia_program_commands()),
                _ => Err(Error::new(
                    ErrorCode::Unsupported,
                    "Lumencor CIA PulseProgram uses the configured levels/events properties",
                )),
            },
            CapabilityKind::TriggerSink => Ok(trigger_sink_actions(request)?
                .into_iter()
                .map(|enabled| {
                    if enabled {
                        protocol::CiaCommand::Run
                    } else {
                        protocol::CiaCommand::Stop
                    }
                })
                .collect()),
            CapabilityKind::GenericCommand => {
                let CapabilityRequest::GenericCommand(request) = request else {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "Lumencor CIA GenericCommand expects GenericCommand",
                    ));
                };
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
                        "Lumencor CIA GenericCommand commands do not accept params",
                    ));
                }
                Ok(match request.command.as_str() {
                    "run" => vec![protocol::CiaCommand::Run],
                    "stop" => vec![protocol::CiaCommand::Stop],
                    "step" => vec![protocol::CiaCommand::Step],
                    "rewind" => vec![protocol::CiaCommand::Rewind],
                    "info" => vec![protocol::CiaCommand::QueryInfo],
                    other => {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            format!("unsupported Lumencor CIA command {other}"),
                        ))
                    }
                })
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Lumencor CIA capability",
            )),
        }
    }

    fn configured_cia_program_commands(&self) -> Vec<protocol::CiaCommand> {
        vec![
            protocol::CiaCommand::WriteLevels(self.levels),
            protocol::CiaCommand::WriteEvents(self.events.clone()),
            protocol::CiaCommand::SetEngine(self.engine),
            protocol::CiaCommand::SetInputPolarity {
                input1: self.input1,
                input2: self.input2,
            },
        ]
    }

    fn refresh_cia_info(&mut self) -> Result<()> {
        self.send(protocol::CiaCommand::QueryInfo)?;
        if let Some(info) = protocol::read_optional_lf_line(&mut *self.serial, 4)? {
            self.info = info;
            self.emit_property(self.cia, "info", Value::String(self.info.clone()));
        }
        Ok(())
    }

    fn issue_cia_read_command(&mut self, device: DeviceId, key: &str) -> Result<()> {
        if device == self.cia && key == "info" {
            self.refresh_cia_info()?;
        }
        Ok(())
    }

    fn apply_cia_invoke(
        &mut self,
        kind: CapabilityKind,
        request: CapabilityRequest,
    ) -> Result<Value> {
        match kind {
            CapabilityKind::PulseProgram => {
                for command in self.cia_invoke_commands(kind, &request)? {
                    self.send(command)?;
                }
                self.run_state = CiaRunState::Ready;
                self.emit_property(
                    self.cia,
                    "event_count",
                    Value::I64(self.events.len() as i64),
                );
                self.emit_property(
                    self.cia,
                    "run_state",
                    Value::String(self.run_state.label().into()),
                );
                Ok(Value::Map(BTreeMap::from([
                    ("event_count".into(), Value::I64(self.events.len() as i64)),
                    (
                        "run_state".into(),
                        Value::String(self.run_state.label().into()),
                    ),
                ])))
            }
            CapabilityKind::TriggerSink => {
                for command in self.cia_invoke_commands(kind, &request)? {
                    match command {
                        protocol::CiaCommand::Run => self.run_state = CiaRunState::Running,
                        protocol::CiaCommand::Stop => self.run_state = CiaRunState::Stopped,
                        _ => {}
                    }
                    self.send(command)?;
                }
                self.emit_property(
                    self.cia,
                    "run_state",
                    Value::String(self.run_state.label().into()),
                );
                Ok(Value::Map(BTreeMap::from([(
                    "run_state".into(),
                    Value::String(self.run_state.label().into()),
                )])))
            }
            CapabilityKind::GenericCommand => {
                let CapabilityRequest::GenericCommand(request) = request else {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "Lumencor CIA GenericCommand expects GenericCommand",
                    ));
                };
                self.invoke_generic(request)
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Lumencor CIA capability",
            )),
        }
    }

    fn local_timing_routes(&self, plan: &TimingPlan) -> Vec<Value> {
        plan.routes
            .iter()
            .filter(|route| route.from == self.cia || route.to == self.cia)
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

    fn cia_transaction(
        &self,
        description: &str,
        command: protocol::CiaCommand,
    ) -> PhysicalTransaction {
        PhysicalTransaction {
            resource: Some(self.resource),
            description: description.into(),
            payload: Value::Bytes(protocol::encode_cia(&command)),
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
}

impl Driver for LumencorCiaDriver {
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
            label: "lumencor-cia-serial".into(),
            kind: "serial.ascii".into(),
            metadata: BTreeMap::from([
                ("terminator".into(), Value::String("LF".into())),
                (
                    "protocol".into(),
                    Value::String("Lumencor Camera Interface Adapter".into()),
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
                        protocol::cia_probe_script(self.engine, self.input1, self.input2)
                            .into_iter()
                            .map(Value::String)
                            .collect(),
                    ),
                ),
            ]),
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.cia {
            vec![
                capability(1, device, CapabilityKind::PulseProgram),
                capability(2, device, CapabilityKind::TriggerSink),
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
                        description: format!("lumencor cia read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("lumencor cia write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "lumencor cia program state set".into(),
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
                    if *device != self.cia
                        || self
                            .capabilities(*device)
                            .iter()
                            .all(|candidate| candidate.id != *capability)
                    {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "unknown Lumencor CIA capability",
                        ));
                    }
                    let Some(capability) = self
                        .capabilities(*device)
                        .into_iter()
                        .find(|candidate| candidate.id == *capability)
                    else {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "unknown Lumencor CIA capability",
                        ));
                    };
                    if !capability.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "Lumencor CIA {:?} expects {:?}, got {:?}",
                                capability.kind,
                                capability.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    for command in self.cia_invoke_commands(capability.kind, request)? {
                        physical_transactions
                            .push(self.cia_transaction("lumencor cia direct invocation", command));
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
        self.send(protocol::CiaCommand::WriteLevels(self.levels))?;
        self.send(protocol::CiaCommand::WriteEvents(self.events.clone()))?;
        self.send(protocol::CiaCommand::SetEngine(self.engine))?;
        self.send(protocol::CiaCommand::SetInputPolarity {
            input1: self.input1,
            input2: self.input2,
        })?;
        self.run_state = CiaRunState::Ready;
        self.emit_property(
            self.cia,
            "run_state",
            Value::String(self.run_state.label().into()),
        );
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Arm(plan.clone())],
            physical_transactions: vec![
                self.cia_transaction(
                    "lumencor cia timing arm levels",
                    protocol::CiaCommand::WriteLevels(self.levels),
                ),
                self.cia_transaction(
                    "lumencor cia timing arm events",
                    protocol::CiaCommand::WriteEvents(self.events.clone()),
                ),
                self.cia_transaction(
                    "lumencor cia timing arm engine",
                    protocol::CiaCommand::SetEngine(self.engine),
                ),
                self.cia_transaction(
                    "lumencor cia timing arm input polarity",
                    protocol::CiaCommand::SetInputPolarity {
                        input1: self.input1,
                        input2: self.input2,
                    },
                ),
                PhysicalTransaction {
                    resource: Some(self.resource),
                    description: "lumencor cia timing arm summary".into(),
                    payload: Value::Map(BTreeMap::from([
                        ("device".into(), Value::I64(self.cia.0 .0 as i64)),
                        ("events".into(), Value::I64(self.events.len() as i64)),
                        ("routes".into(), Value::List(self.local_timing_routes(plan))),
                        (
                            "run_state".into(),
                            Value::String(self.run_state.label().into()),
                        ),
                    ])),
                },
            ],
        })
    }

    fn start_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        self.send(protocol::CiaCommand::Run)?;
        self.run_state = CiaRunState::Running;
        self.emit_property(
            self.cia,
            "run_state",
            Value::String(self.run_state.label().into()),
        );
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Start(OperationId(0))],
            physical_transactions: vec![
                self.cia_transaction("lumencor cia timing start run", protocol::CiaCommand::Run),
                PhysicalTransaction {
                    resource: Some(self.resource),
                    description: "lumencor cia timing start summary".into(),
                    payload: Value::Map(BTreeMap::from([
                        ("device".into(), Value::I64(self.cia.0 .0 as i64)),
                        (
                            "routes".into(),
                            Value::List(self.local_timing_routes(&armed.plan)),
                        ),
                        (
                            "run_state".into(),
                            Value::String(self.run_state.label().into()),
                        ),
                    ])),
                },
            ],
        })
    }

    fn stop_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        self.send(protocol::CiaCommand::Stop)?;
        self.run_state = CiaRunState::Stopped;
        self.emit_property(
            self.cia,
            "run_state",
            Value::String(self.run_state.label().into()),
        );
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions: vec![
                self.cia_transaction("lumencor cia timing stop", protocol::CiaCommand::Stop),
                PhysicalTransaction {
                    resource: Some(self.resource),
                    description: "lumencor cia timing stop summary".into(),
                    payload: Value::Map(BTreeMap::from([
                        ("device".into(), Value::I64(self.cia.0 .0 as i64)),
                        (
                            "routes".into(),
                            Value::List(self.local_timing_routes(&armed.plan)),
                        ),
                        (
                            "run_state".into(),
                            Value::String(self.run_state.label().into()),
                        ),
                    ])),
                },
            ],
        })
    }

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.token();
        let mut last = Value::Null;
        for command in prepared.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    self.issue_cia_read_command(device, &key)?;
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
                    let Some(capability) = self
                        .capabilities(device)
                        .into_iter()
                        .find(|candidate| candidate.id == capability)
                    else {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "unknown Lumencor CIA capability",
                        ));
                    };
                    if !capability.accepts_request(&request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "Lumencor CIA {:?} expects {:?}, got {:?}",
                                capability.kind,
                                capability.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    last = self.apply_cia_invoke(capability.kind, request)?;
                }
                Command::Arm(_) | Command::Start(_) | Command::Stop(_) => unreachable!(),
            }
        }
        self.pending
            .push_back(DriverEvent::TokenCompleted { token, value: last });
        Ok(token)
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        self.pending.drain(..).collect()
    }
}

fn capability(id: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    CapabilityDescriptor::new(CapabilityId(id), device, kind, ValueType::Map)
}

fn dac_request_percent(request: &CapabilityRequest) -> Result<u8> {
    let percent = match request {
        CapabilityRequest::Dac(request) => percent_value(&request.value)?,
        _ => {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Lumencor Dac expects CapabilityRequest::Dac",
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
            "Lumencor percent value must be Ratio",
        )),
    }
}

fn trigger_sink_actions(request: &CapabilityRequest) -> Result<Vec<bool>> {
    let action = match request {
        CapabilityRequest::None => TriggerSinkAction::Pulse,
        CapabilityRequest::Trigger(request) => match request.action {
            TriggerAction::Enable => TriggerSinkAction::Enable,
            TriggerAction::Disable => TriggerSinkAction::Disable,
            TriggerAction::Pulse => TriggerSinkAction::Pulse,
        },
        _ => {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Lumencor TriggerSink expects None or CapabilityRequest::Trigger",
            ));
        }
    };
    Ok(match action {
        TriggerSinkAction::Enable => vec![true],
        TriggerSinkAction::Disable => vec![false],
        TriggerSinkAction::Pulse => vec![true, false],
    })
}

fn validate_configured_cia_program_request(request: &PulseProgramRequest) -> Result<()> {
    if request.interval.is_none()
        && request.duration.is_none()
        && request.count.is_none()
        && request.wait_for_input.is_none()
    {
        return Ok(());
    }
    Err(Error::new(
        ErrorCode::Unsupported,
        "Lumencor CIA PulseProgram loads configured levels/events; generic interval, duration, count, and wait_for_input fields are not mapped",
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TriggerSinkAction {
    Enable,
    Disable,
    Pulse,
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

fn sequenceable_ratio_property_range(
    key: &str,
    display_name: &str,
    unit: Option<&str>,
    writable: bool,
    min: f64,
    max: f64,
) -> PropertySchema {
    sequenceable_property(
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

fn with_applied(summary: Value, applied: Value) -> Value {
    match summary {
        Value::Map(mut map) => {
            map.insert("applied".into(), applied);
            Value::Map(map)
        }
        other => other,
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

fn parse_color_channels(channels: &str) -> Result<Vec<protocol::ColorChannel>> {
    let mut parsed = Vec::new();
    for token in channels
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let channel = match token {
            "red" | "Red" => protocol::ColorChannel::Red,
            "green" | "Green" => protocol::ColorChannel::Green,
            "cyan" | "Cyan" => protocol::ColorChannel::Cyan,
            "violet" | "Violet" => protocol::ColorChannel::Violet,
            "blue" | "Blue" => protocol::ColorChannel::Blue,
            "teal" | "Teal" => protocol::ColorChannel::Teal,
            other => {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown Lumencor color channel: {other}"),
                ))
            }
        };
        parsed.push(channel);
    }
    if parsed.is_empty() {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "Lumencor channels config must name at least one channel",
        ));
    }
    Ok(parsed)
}
