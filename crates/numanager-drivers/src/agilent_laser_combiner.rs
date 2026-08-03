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

    pub const BAUD: u32 = 115_200;
    pub const MAX_LINES: usize = 8;
    pub const MAX_ANALOG_OUTPUTS: usize = 4;
    pub const CALIBRATION_COEFFICIENTS: usize = 11;
    pub const IDENTITY: &str = "My100xBoard";

    #[derive(Debug, Clone, PartialEq)]
    pub struct LineInfo {
        pub wavelength: Wavelength,
        pub min_voltage: Voltage,
        pub max_voltage: Voltage,
        pub dac_bit_depth: u8,
        pub max_power: OpticalPower,
        pub calibration: [f32; CALIBRATION_COEFFICIENTS],
    }

    impl LineInfo {
        pub fn fixture(line: usize) -> Self {
            let wavelengths = [405.0, 445.0, 488.0, 514.0, 561.0, 594.0, 640.0, 730.0];
            let max_power = [50.0, 75.0, 120.0, 80.0, 100.0, 50.0, 140.0, 40.0];
            let mut calibration = [0.0; CALIBRATION_COEFFICIENTS];
            calibration[1] = max_power[line.saturating_sub(1).min(MAX_LINES - 1)] as f32 / 5.0;
            Self {
                wavelength: Wavelength::from_nanometers(
                    wavelengths[line.saturating_sub(1).min(MAX_LINES - 1)],
                ),
                min_voltage: Voltage::from_volts(0.0),
                max_voltage: Voltage::from_volts(5.0),
                dac_bit_depth: 16,
                max_power: OpticalPower::from_milliwatts(
                    max_power[line.saturating_sub(1).min(MAX_LINES - 1)],
                ),
                calibration,
            }
        }

        pub fn max_counts(&self) -> u16 {
            if self.dac_bit_depth == 0 {
                0
            } else {
                ((1u32 << self.dac_bit_depth.min(16)) - 1) as u16
            }
        }

        pub fn counts_to_ratio(&self, counts: u16) -> Ratio {
            let max = self.max_counts().max(1) as f64;
            Ratio::from_fraction((counts as f64 / max).clamp(0.0, 1.0))
        }

        pub fn ratio_to_counts(&self, ratio: Ratio) -> u16 {
            let max = self.max_counts() as f64;
            (ratio.fraction().clamp(0.0, 1.0) * max)
                .round()
                .clamp(0.0, max) as u16
        }

        pub fn counts_to_voltage(&self, counts: u16) -> Voltage {
            let fraction = self.counts_to_ratio(counts).fraction();
            let min = self.min_voltage.volts();
            let max = self.max_voltage.volts();
            Voltage::from_volts(min + (max - min) * fraction)
        }

        pub fn counts_to_power(&self, counts: u16) -> OpticalPower {
            let volts = self.counts_to_voltage(counts).volts();
            let mw = self
                .calibration
                .iter()
                .rev()
                .fold(0.0, |acc, coeff| acc * volts + *coeff as f64)
                .clamp(0.0, self.max_power.milliwatts());
            OpticalPower::from_milliwatts(mw)
        }

        pub fn power_to_counts(&self, power: OpticalPower) -> u16 {
            let target = power.milliwatts().clamp(0.0, self.max_power.milliwatts());
            let mut low = 0u16;
            let mut high = self.max_counts();
            for _ in 0..16 {
                let mid = low + (high - low) / 2;
                if self.counts_to_power(mid).milliwatts() < target {
                    low = mid.saturating_add(1);
                } else {
                    high = mid;
                }
            }
            high
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Probe {
        pub model: String,
        pub serial_number: String,
        pub firmware_version: String,
        pub hardware_version: String,
        pub state_mask: u8,
        pub external_control_enabled: bool,
        pub blanking_enabled: bool,
        pub sync_mode: u8,
        pub shutter_open: bool,
        pub galvo_position: u8,
        pub nd_filter_state: u8,
        pub nd_filter_mapping: u8,
        pub direct_amplitude: u16,
        pub saved_direct_amplitude: u16,
        pub line_counts: Vec<u16>,
        pub analog_outputs: Vec<u16>,
        pub lines: Vec<LineInfo>,
    }

    impl Probe {
        pub fn fixture() -> Self {
            let lines = (1..=4).map(LineInfo::fixture).collect::<Vec<_>>();
            Self {
                model: "LU-N4".into(),
                serial_number: "AGILENT-CONFIG-0001".into(),
                firmware_version: "0.12".into(),
                hardware_version: "configured".into(),
                state_mask: 0,
                external_control_enabled: false,
                blanking_enabled: false,
                sync_mode: 0,
                shutter_open: false,
                galvo_position: 0,
                nd_filter_state: 0,
                nd_filter_mapping: 0,
                direct_amplitude: 0,
                saved_direct_amplitude: 0,
                line_counts: vec![0; 4],
                analog_outputs: vec![0; 4],
                lines,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum Command {
        Model,
        FirmwareVersion,
        Identify,
        SerialNumber,
        HardwareVersion,
        SetStateMask(u8),
        SetLinePowerRaw { line: u8, counts: u16 },
        SetAnalogOutputRaw { channel: u8, counts: u16 },
        SetExternalControl(bool),
        SetBlanking(bool),
        SetSyncMode(u8),
        SetShutter(bool),
        SetGalvoPosition(u8),
        SetNdFilterState(u8),
        SetNdFilterMapping(u8),
        SetDirectAmplitude { line: u8, counts: u16 },
        SaveDirectAmplitude,
        GetStateMask,
        GetLinePowerRaw(u8),
        GetAnalogOutputRaw(u8),
        GetExternalControl,
        GetBlanking,
        GetSyncMode,
        GetShutter,
        GetGalvoPosition,
        GetNdFilterState,
        GetNdFilterMapping,
        GetDirectAmplitude,
        GetSavedDirectAmplitude,
        GetLineCount,
        GetMinVoltage(u8),
        GetMaxVoltage(u8),
        GetDacBitDepth(u8),
        GetWavelength(u8),
        GetMaxPower(u8),
        GetCalibrationCoefficient { line: u8, index: u8 },
        SetNdFilterPresent(bool),
        SetGalvoPresent(bool),
        SetWavelength { line: u8, wavelength: Wavelength },
        SetMaxPower { line: u8, power: OpticalPower },
        SetCalibrationCoefficient { line: u8, index: u8, value: f32 },
        SetSerialNumber(String),
        SetRegister { register: u8, value: u8 },
        GetRegister(u8),
        SetEeprom { address: u16, value: u8 },
        GetEeprom(u16),
        SetAotfFrequency { channel: u8, frequency: u32 },
        GetAotfFrequency(u8),
        GetRuntimeCounterLow(u8),
        GetRuntimeCounterHigh(u8),
        ProgramStateSequence(Vec<u8>),
        ProgramLinePowerSequence { line: u8, counts: Vec<u16> },
        ProgramAnalogOutputSequence { channel: u8, counts: Vec<u16> },
        StartSequence(u8),
        StopSequence,
    }

    impl Command {
        pub fn opcode(&self) -> u8 {
            match self {
                Self::Model => 0x01,
                Self::FirmwareVersion => 0x02,
                Self::Identify => 0x03,
                Self::SerialNumber => 0x04,
                Self::HardwareVersion => 0x05,
                Self::SetStateMask(_) => 0x0a,
                Self::SetLinePowerRaw { .. } => 0x0b,
                Self::SetAnalogOutputRaw { .. } => 0x0c,
                Self::SetExternalControl(_) => 0x0d,
                Self::SetBlanking(_) => 0x0e,
                Self::SetSyncMode(_) => 0x0f,
                Self::SetShutter(_) => 0x10,
                Self::SetGalvoPosition(_) => 0x11,
                Self::SetNdFilterState(_) => 0x12,
                Self::SetNdFilterMapping(_) => 0x13,
                Self::SetDirectAmplitude { .. } => 0x14,
                Self::SaveDirectAmplitude => 0x15,
                Self::GetStateMask => 0x28,
                Self::GetLinePowerRaw(_) => 0x29,
                Self::GetAnalogOutputRaw(_) => 0x2a,
                Self::GetExternalControl => 0x2b,
                Self::GetBlanking => 0x2c,
                Self::GetSyncMode => 0x2d,
                Self::GetShutter => 0x2e,
                Self::GetGalvoPosition => 0x2f,
                Self::GetNdFilterState => 0x30,
                Self::GetNdFilterMapping => 0x31,
                Self::GetDirectAmplitude => 0x32,
                Self::GetSavedDirectAmplitude => 0x33,
                Self::GetLineCount => 0x36,
                Self::GetMinVoltage(_) => 0x37,
                Self::GetMaxVoltage(_) => 0x38,
                Self::GetDacBitDepth(_) => 0x39,
                Self::GetWavelength(_) => 0x3a,
                Self::GetMaxPower(_) => 0x3b,
                Self::GetCalibrationCoefficient { .. } => 0x3c,
                Self::SetNdFilterPresent(_) => 0x52,
                Self::SetGalvoPresent(_) => 0x53,
                Self::SetWavelength { .. } => 0x58,
                Self::SetMaxPower { .. } => 0x59,
                Self::SetCalibrationCoefficient { .. } => 0x5a,
                Self::SetSerialNumber(_) => 0x5b,
                Self::SetRegister { .. } => 0x5c,
                Self::GetRegister(_) => 0x5d,
                Self::SetEeprom { .. } => 0x5e,
                Self::GetEeprom(_) => 0x5f,
                Self::SetAotfFrequency { .. } => 0x60,
                Self::GetAotfFrequency(_) => 0x61,
                Self::GetRuntimeCounterLow(_) => 0x62,
                Self::GetRuntimeCounterHigh(_) => 0x63,
                Self::ProgramStateSequence(_) => 0x64,
                Self::ProgramLinePowerSequence { .. } => 0x65,
                Self::ProgramAnalogOutputSequence { .. } => 0x66,
                Self::StartSequence(_) => 0x67,
                Self::StopSequence => 0x68,
            }
        }
    }

    pub fn encode(command: &Command) -> Result<Vec<u8>> {
        let mut bytes = vec![command.opcode()];
        match command {
            Command::Model
            | Command::FirmwareVersion
            | Command::Identify
            | Command::SerialNumber
            | Command::HardwareVersion
            | Command::SaveDirectAmplitude
            | Command::GetStateMask
            | Command::GetExternalControl
            | Command::GetBlanking
            | Command::GetSyncMode
            | Command::GetShutter
            | Command::GetGalvoPosition
            | Command::GetNdFilterState
            | Command::GetNdFilterMapping
            | Command::GetDirectAmplitude
            | Command::GetSavedDirectAmplitude
            | Command::GetLineCount
            | Command::StopSequence => {}
            Command::SetStateMask(value)
            | Command::SetSyncMode(value)
            | Command::SetGalvoPosition(value)
            | Command::SetNdFilterState(value)
            | Command::SetNdFilterMapping(value)
            | Command::StartSequence(value) => bytes.push(*value),
            Command::SetExternalControl(value)
            | Command::SetBlanking(value)
            | Command::SetShutter(value)
            | Command::SetNdFilterPresent(value)
            | Command::SetGalvoPresent(value) => bytes.push(u8::from(*value)),
            Command::SetLinePowerRaw { line, counts }
            | Command::SetAnalogOutputRaw {
                channel: line,
                counts,
            }
            | Command::SetDirectAmplitude { line, counts } => {
                bytes.push(*line);
                bytes.extend_from_slice(&counts.to_be_bytes());
            }
            Command::GetLinePowerRaw(line)
            | Command::GetAnalogOutputRaw(line)
            | Command::GetMinVoltage(line)
            | Command::GetMaxVoltage(line)
            | Command::GetDacBitDepth(line)
            | Command::GetWavelength(line)
            | Command::GetMaxPower(line)
            | Command::GetRuntimeCounterLow(line)
            | Command::GetRuntimeCounterHigh(line)
            | Command::GetRegister(line) => bytes.push(*line),
            Command::GetCalibrationCoefficient { line, index } => {
                bytes.push(*line);
                bytes.push(*index);
            }
            Command::SetWavelength { line, wavelength } => {
                bytes.push(*line);
                let nm = wavelength.nanometers().round().clamp(0.0, u16::MAX as f64) as u16;
                bytes.extend_from_slice(&nm.to_be_bytes());
            }
            Command::SetMaxPower { line, power } => {
                bytes.push(*line);
                bytes.extend_from_slice(&(power.milliwatts() as f32).to_le_bytes());
            }
            Command::SetCalibrationCoefficient { line, index, value } => {
                bytes.push(*line);
                bytes.push(*index);
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            Command::SetSerialNumber(serial) => {
                let serial = serial.as_bytes();
                if serial.len() > 64 {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Agilent serial-number payload must be at most 64 bytes",
                    ));
                }
                bytes.extend_from_slice(serial);
                bytes.push(0);
            }
            Command::SetRegister { register, value } => {
                bytes.push(*register);
                bytes.push(*value);
            }
            Command::GetEeprom(address) => bytes.extend_from_slice(&address.to_be_bytes()),
            Command::SetEeprom { address, value } => {
                bytes.extend_from_slice(&address.to_be_bytes());
                bytes.push(*value);
            }
            Command::SetAotfFrequency { channel, frequency } => {
                bytes.push(*channel);
                bytes.extend_from_slice(&frequency.to_be_bytes());
            }
            Command::GetAotfFrequency(channel) => bytes.push(*channel),
            Command::ProgramStateSequence(states) => {
                if states.len() > u16::MAX as usize {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "Agilent state sequence is too long",
                    ));
                }
                bytes.extend_from_slice(&(states.len() as u16).to_be_bytes());
                bytes.extend_from_slice(states);
            }
            Command::ProgramLinePowerSequence { line, counts }
            | Command::ProgramAnalogOutputSequence {
                channel: line,
                counts,
            } => {
                if counts.len() > u16::MAX as usize {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "Agilent output sequence is too long",
                    ));
                }
                bytes.push(*line);
                bytes.extend_from_slice(&(counts.len() as u16).to_be_bytes());
                for value in counts {
                    bytes.extend_from_slice(&value.to_be_bytes());
                }
            }
        }
        Ok(bytes)
    }

    pub fn parse_u8(command: &Command, reply: &str) -> Result<u8> {
        parse_u32(command, reply).and_then(|value| {
            u8::try_from(value).map_err(|_| {
                Error::new(
                    ErrorCode::Transport,
                    format!("Agilent {:?} reply {reply:?} is outside u8 range", command),
                )
            })
        })
    }

    pub fn parse_u16(command: &Command, reply: &str) -> Result<u16> {
        parse_u32(command, reply).and_then(|value| {
            u16::try_from(value).map_err(|_| {
                Error::new(
                    ErrorCode::Transport,
                    format!("Agilent {:?} reply {reply:?} is outside u16 range", command),
                )
            })
        })
    }

    pub fn parse_u32(command: &Command, reply: &str) -> Result<u32> {
        reply.trim().parse::<u32>().map_err(|_| {
            Error::new(
                ErrorCode::Transport,
                format!(
                    "Agilent {:?} reply {reply:?} is not an unsigned integer",
                    command
                ),
            )
        })
    }

    pub fn parse_f64(command: &Command, reply: &str) -> Result<f64> {
        reply.trim().parse::<f64>().map_err(|_| {
            Error::new(
                ErrorCode::Transport,
                format!("Agilent {:?} reply {reply:?} is not a float", command),
            )
        })
    }
}

#[derive(Debug, Clone)]
pub struct AgilentLaserCombinerConfiguredProbe {
    label: String,
    serial_port: Option<String>,
    connect_real_transport: bool,
    probe: protocol::Probe,
}

pub struct AgilentLaserCombinerDiscovery {
    next_id: DriverId,
    probes: Vec<AgilentLaserCombinerConfiguredProbe>,
}

impl AgilentLaserCombinerDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![AgilentLaserCombinerConfiguredProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| {
                matches!(
                    device.driver.as_str(),
                    "agilent_laser_combiner" | "agilent-laser-combiner" | "agilent_combiner"
                )
            })
            .map(AgilentLaserCombinerConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for AgilentLaserCombinerDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver: Box<dyn Driver> = if configured.connect_real_transport {
                    Box::new(AgilentLaserCombinerDriver::serial(id, configured)?)
                } else {
                    Box::new(AgilentLaserCombinerDriver::configured(id, configured))
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

impl AgilentLaserCombinerConfiguredProbe {
    pub fn fixture() -> Self {
        Self {
            label: "Configured Agilent Laser Combiner".into(),
            serial_port: None,
            connect_real_transport: false,
            probe: protocol::Probe::fixture(),
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::fixture();
        if !device.label.is_empty() {
            configured.label = device.label.clone();
        }
        configured.serial_port = string_prop(device, "serial_port");
        configured.connect_real_transport = bool_prop(device, "connect").unwrap_or(false);
        configured.probe.model = string_prop(device, "model").unwrap_or(configured.probe.model);
        configured.probe.serial_number =
            string_prop(device, "serial_number").unwrap_or(configured.probe.serial_number);
        configured.probe.firmware_version =
            string_prop(device, "firmware_version").unwrap_or(configured.probe.firmware_version);
        configured.probe.hardware_version =
            string_prop(device, "hardware_version").unwrap_or(configured.probe.hardware_version);
        configured.probe.state_mask = u8_prop(device, "state_mask").unwrap_or(0);
        configured.probe.external_control_enabled =
            bool_prop(device, "external_control_enabled").unwrap_or(false);
        configured.probe.blanking_enabled = bool_prop(device, "blanking_enabled").unwrap_or(false);
        configured.probe.sync_mode = u8_prop(device, "sync_mode").unwrap_or(0);
        configured.probe.shutter_open = bool_prop(device, "shutter_open").unwrap_or(false);
        configured.probe.galvo_position = u8_prop(device, "galvo_position").unwrap_or(0);
        configured.probe.nd_filter_state = u8_prop(device, "nd_filter_state").unwrap_or(0);
        configured.probe.nd_filter_mapping = u8_prop(device, "nd_filter_mapping").unwrap_or(0);
        let line_count = usize_prop(device, "line_count").unwrap_or(configured.probe.lines.len());
        if !(1..=protocol::MAX_LINES).contains(&line_count) {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Agilent Laser Combiner line_count must be in 1..=8",
            ));
        }
        configured.probe.lines = (1..=line_count)
            .map(|line| configured_line_info(device, line))
            .collect();
        configured.probe.line_counts = (1..=line_count)
            .map(|line| {
                ratio_prop(device, &format!("line_{line}_intensity"))
                    .map(|ratio| configured.probe.lines[line - 1].ratio_to_counts(ratio))
                    .unwrap_or(0)
            })
            .collect();
        configured.probe.analog_outputs = (1..=protocol::MAX_ANALOG_OUTPUTS)
            .map(|channel| {
                u16_prop(device, &format!("analog_output_{channel}_raw_counts")).unwrap_or(0)
            })
            .collect();
        Ok(configured)
    }
}

pub struct AgilentLaserCombinerDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    lines: Vec<DeviceId>,
    analog_outputs: Vec<DeviceId>,
    probe: protocol::Probe,
    serial: Box<dyn SerialIo>,
    reader: ReplyReader,
    synthesize_replies: bool,
    last_transaction: Value,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial_port: Option<String>,
    serial_timeout_ms: u64,
    connected: bool,
}

impl AgilentLaserCombinerDriver {
    pub fn configured_fixture(id: DriverId) -> Self {
        Self::configured(id, AgilentLaserCombinerConfiguredProbe::fixture())
    }

    pub fn configured(id: DriverId, configured: AgilentLaserCombinerConfiguredProbe) -> Self {
        let mut driver =
            Self::new_configured(id, configured, Box::new(ScriptedSerial::new()), false);
        driver.synthesize_replies = true;
        driver
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: AgilentLaserCombinerConfiguredProbe) -> Result<Self> {
        let port_name = configured.serial_port.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Agilent Laser Combiner real serial config requires serial_port",
            )
        })?;
        let serial = Box::new(numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(port_name, protocol::BAUD)
                .timeout(Duration::from_millis(1)),
        )?);
        let mut driver = Self::new_configured(id, configured, serial, true);
        driver.refresh_identity_from_hardware()?;
        Ok(driver)
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _configured: AgilentLaserCombinerConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Agilent Laser Combiner real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(id: DriverId, probe: protocol::Probe, serial: Box<dyn SerialIo>) -> Self {
        let line_count = probe.lines.len();
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 4700)),
            hub: DeviceId(NodeId(id.0 * 1000 + 4701)),
            lines: (0..line_count)
                .map(|index| DeviceId(NodeId(id.0 * 1000 + 4710 + index as u64)))
                .collect(),
            analog_outputs: (0..protocol::MAX_ANALOG_OUTPUTS)
                .map(|index| DeviceId(NodeId(id.0 * 1000 + 4720 + index as u64)))
                .collect(),
            probe,
            serial,
            reader: ReplyReader::default(),
            synthesize_replies: false,
            last_transaction: Value::Map(BTreeMap::new()),
            next_token: 1,
            pending: VecDeque::new(),
            serial_port: None,
            serial_timeout_ms: 1,
            connected: false,
        }
    }

    pub fn new_configured(
        id: DriverId,
        configured: AgilentLaserCombinerConfiguredProbe,
        serial: Box<dyn SerialIo>,
        connected: bool,
    ) -> Self {
        let mut driver = Self::new(id, configured.probe, serial);
        driver.serial_port = configured.serial_port;
        driver.connected = connected;
        driver
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    #[cfg(feature = "os-serial")]
    fn refresh_identity_from_hardware(&mut self) -> Result<()> {
        let identity = self.send(protocol::Command::Identify)?;
        if identity != protocol::IDENTITY {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("Agilent Laser Combiner identity mismatch: {identity:?}"),
            ));
        }
        self.probe.model = self.send(protocol::Command::Model)?;
        self.probe.serial_number = self.send(protocol::Command::SerialNumber)?;
        self.probe.firmware_version = self.send(protocol::Command::FirmwareVersion)?;
        self.probe.hardware_version = self.send(protocol::Command::HardwareVersion)?;
        let line_count = protocol::parse_u8(
            &protocol::Command::GetLineCount,
            &self.send(protocol::Command::GetLineCount)?,
        )?
        .clamp(1, protocol::MAX_LINES as u8) as usize;
        self.lines = (0..line_count)
            .map(|index| DeviceId(NodeId(self.id.0 * 1000 + 4710 + index as u64)))
            .collect();
        self.analog_outputs = (0..protocol::MAX_ANALOG_OUTPUTS)
            .map(|index| DeviceId(NodeId(self.id.0 * 1000 + 4720 + index as u64)))
            .collect();
        self.probe.lines.clear();
        self.probe.line_counts.clear();
        self.probe.analog_outputs = vec![0; protocol::MAX_ANALOG_OUTPUTS];
        for line in 1..=line_count {
            let line_u8 = line as u8;
            let min_voltage = protocol::parse_f64(
                &protocol::Command::GetMinVoltage(line_u8),
                &self.send(protocol::Command::GetMinVoltage(line_u8))?,
            )?;
            let max_voltage = protocol::parse_f64(
                &protocol::Command::GetMaxVoltage(line_u8),
                &self.send(protocol::Command::GetMaxVoltage(line_u8))?,
            )?;
            let bit_depth = protocol::parse_u8(
                &protocol::Command::GetDacBitDepth(line_u8),
                &self.send(protocol::Command::GetDacBitDepth(line_u8))?,
            )?;
            let wavelength = protocol::parse_u16(
                &protocol::Command::GetWavelength(line_u8),
                &self.send(protocol::Command::GetWavelength(line_u8))?,
            )?;
            let max_power = protocol::parse_f64(
                &protocol::Command::GetMaxPower(line_u8),
                &self.send(protocol::Command::GetMaxPower(line_u8))?,
            )?;
            let mut calibration = [0.0f32; protocol::CALIBRATION_COEFFICIENTS];
            for (index, coefficient) in calibration.iter_mut().enumerate() {
                *coefficient = protocol::parse_f64(
                    &protocol::Command::GetCalibrationCoefficient {
                        line: line_u8,
                        index: index as u8,
                    },
                    &self.send(protocol::Command::GetCalibrationCoefficient {
                        line: line_u8,
                        index: index as u8,
                    })?,
                )? as f32;
            }
            let info = protocol::LineInfo {
                wavelength: Wavelength::from_nanometers(wavelength as f64),
                min_voltage: Voltage::from_volts(min_voltage),
                max_voltage: Voltage::from_volts(max_voltage),
                dac_bit_depth: bit_depth.min(16),
                max_power: OpticalPower::from_milliwatts(max_power),
                calibration,
            };
            let counts = protocol::parse_u16(
                &protocol::Command::GetLinePowerRaw(line_u8),
                &self.send(protocol::Command::GetLinePowerRaw(line_u8))?,
            )?;
            self.probe.lines.push(info);
            self.probe.line_counts.push(counts);
        }
        self.probe.state_mask = protocol::parse_u8(
            &protocol::Command::GetStateMask,
            &self.send(protocol::Command::GetStateMask)?,
        )?;
        self.probe.external_control_enabled = protocol::parse_u8(
            &protocol::Command::GetExternalControl,
            &self.send(protocol::Command::GetExternalControl)?,
        )? != 0;
        self.probe.blanking_enabled = protocol::parse_u8(
            &protocol::Command::GetBlanking,
            &self.send(protocol::Command::GetBlanking)?,
        )? != 0;
        self.probe.sync_mode = protocol::parse_u8(
            &protocol::Command::GetSyncMode,
            &self.send(protocol::Command::GetSyncMode)?,
        )?;
        self.probe.shutter_open = protocol::parse_u8(
            &protocol::Command::GetShutter,
            &self.send(protocol::Command::GetShutter)?,
        )? != 0;
        self.probe.nd_filter_state = protocol::parse_u8(
            &protocol::Command::GetNdFilterState,
            &self.send(protocol::Command::GetNdFilterState)?,
        )?;
        self.probe.nd_filter_mapping = protocol::parse_u8(
            &protocol::Command::GetNdFilterMapping,
            &self.send(protocol::Command::GetNdFilterMapping)?,
        )?;
        self.last_transaction = self.transaction("hardware_probe", "request_reply");
        Ok(())
    }

    fn send(&mut self, command: protocol::Command) -> Result<String> {
        let bytes = protocol::encode(&command)?;
        self.serial.write(&bytes)?;
        #[cfg(feature = "os-serial")]
        if matches!(command, protocol::Command::SetSerialNumber(_)) {
            std::thread::sleep(Duration::from_millis(400));
        }
        if self.synthesize_replies {
            return Ok(self.synthetic_reply(&command));
        }
        self.read_reply(command.opcode())
    }

    fn read_reply(&mut self, expected: u8) -> Result<String> {
        for _ in 0..100 {
            let bytes = self.serial.read_available()?;
            for line in self.reader.push(&bytes) {
                if line.first().copied() == Some(expected) {
                    return String::from_utf8(line[1..].to_vec()).map_err(|error| {
                        Error::new(
                            ErrorCode::Transport,
                            format!("Agilent reply is not UTF-8: {error}"),
                        )
                    });
                }
            }
            #[cfg(feature = "os-serial")]
            std::thread::sleep(Duration::from_millis(1));
        }
        Err(Error::new(
            ErrorCode::Timeout,
            format!("Agilent command 0x{expected:02x} did not return an echoed CRLF reply"),
        ))
    }

    fn synthetic_reply(&self, command: &protocol::Command) -> String {
        match command {
            protocol::Command::Model => self.probe.model.clone(),
            protocol::Command::FirmwareVersion => self.probe.firmware_version.clone(),
            protocol::Command::Identify => protocol::IDENTITY.into(),
            protocol::Command::SerialNumber => self.probe.serial_number.clone(),
            protocol::Command::HardwareVersion => self.probe.hardware_version.clone(),
            protocol::Command::GetStateMask => self.probe.state_mask.to_string(),
            protocol::Command::GetExternalControl => {
                u8::from(self.probe.external_control_enabled).to_string()
            }
            protocol::Command::GetBlanking => u8::from(self.probe.blanking_enabled).to_string(),
            protocol::Command::GetSyncMode => self.probe.sync_mode.to_string(),
            protocol::Command::GetShutter => u8::from(self.probe.shutter_open).to_string(),
            protocol::Command::GetGalvoPosition => self.probe.galvo_position.to_string(),
            protocol::Command::GetNdFilterState => self.probe.nd_filter_state.to_string(),
            protocol::Command::GetNdFilterMapping => self.probe.nd_filter_mapping.to_string(),
            protocol::Command::GetDirectAmplitude => self.probe.direct_amplitude.to_string(),
            protocol::Command::GetSavedDirectAmplitude => {
                self.probe.saved_direct_amplitude.to_string()
            }
            protocol::Command::GetLineCount => self.probe.lines.len().to_string(),
            protocol::Command::GetLinePowerRaw(line) => self
                .probe
                .line_counts
                .get((*line as usize).saturating_sub(1))
                .copied()
                .unwrap_or(0)
                .to_string(),
            protocol::Command::GetAnalogOutputRaw(channel) => self
                .probe
                .analog_outputs
                .get((*channel as usize).saturating_sub(1))
                .copied()
                .unwrap_or(0)
                .to_string(),
            protocol::Command::GetMinVoltage(line) => self
                .line_info_by_number(*line)
                .map(|line| line.min_voltage.volts())
                .unwrap_or(0.0)
                .to_string(),
            protocol::Command::GetMaxVoltage(line) => self
                .line_info_by_number(*line)
                .map(|line| line.max_voltage.volts())
                .unwrap_or(0.0)
                .to_string(),
            protocol::Command::GetDacBitDepth(line) => self
                .line_info_by_number(*line)
                .map(|line| line.dac_bit_depth)
                .unwrap_or(0)
                .to_string(),
            protocol::Command::GetWavelength(line) => self
                .line_info_by_number(*line)
                .map(|line| line.wavelength.nanometers().round() as u16)
                .unwrap_or(0)
                .to_string(),
            protocol::Command::GetMaxPower(line) => self
                .line_info_by_number(*line)
                .map(|line| line.max_power.milliwatts())
                .unwrap_or(0.0)
                .to_string(),
            protocol::Command::GetCalibrationCoefficient { line, index } => self
                .line_info_by_number(*line)
                .and_then(|line| line.calibration.get(*index as usize).copied())
                .unwrap_or(0.0)
                .to_string(),
            _ => "0".into(),
        }
    }

    fn line_info_by_number(&self, line: u8) -> Option<&protocol::LineInfo> {
        self.probe.lines.get((line as usize).checked_sub(1)?)
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        let mut descriptors = vec![DeviceDescriptor {
            id: self.hub,
            driver: self.id,
            label: "agilent-combiner-hub".into(),
            vendor: Some("Agilent/Keysight".into()),
            model: Some(self.probe.model.clone()),
            serial: Some(self.probe.serial_number.clone()),
            kinds: vec!["hub".into(), "light.engine".into()],
            properties: vec![
                property("model", "Model", ValueType::String, None, false, None),
                property(
                    "firmware_version",
                    "Firmware version",
                    ValueType::String,
                    None,
                    false,
                    None,
                ),
                property(
                    "hardware_version",
                    "Hardware version",
                    ValueType::String,
                    None,
                    false,
                    None,
                ),
                property(
                    "serial_number",
                    "Serial number",
                    ValueType::String,
                    None,
                    true,
                    None,
                ),
                property(
                    "line_count",
                    "Line count",
                    ValueType::I64,
                    None,
                    false,
                    None,
                ),
                property(
                    "state_mask",
                    "State mask",
                    ValueType::I64,
                    None,
                    false,
                    None,
                ),
                property(
                    "shutter_open",
                    "Shutter open",
                    ValueType::Bool,
                    None,
                    true,
                    None,
                ),
                property(
                    "external_control_enabled",
                    "External control",
                    ValueType::Bool,
                    None,
                    true,
                    None,
                ),
                property(
                    "blanking_enabled",
                    "Blanking",
                    ValueType::Bool,
                    None,
                    true,
                    None,
                ),
                property(
                    "sync_mode",
                    "Sync mode",
                    ValueType::I64,
                    None,
                    true,
                    Some(i64_range(0, 255)),
                ),
                property(
                    "galvo_position",
                    "Galvo position",
                    ValueType::I64,
                    None,
                    true,
                    Some(i64_range(0, 255)),
                ),
                property(
                    "nd_filter_state",
                    "ND filter state",
                    ValueType::I64,
                    None,
                    true,
                    Some(i64_range(0, 255)),
                ),
                property(
                    "nd_filter_mapping",
                    "ND filter mapping",
                    ValueType::I64,
                    None,
                    true,
                    Some(i64_range(0, 255)),
                ),
                property(
                    "direct_amplitude",
                    "Direct amplitude",
                    ValueType::Ratio,
                    Some("percent"),
                    true,
                    Some(ratio_range()),
                ),
                property(
                    "saved_direct_amplitude",
                    "Saved direct amplitude",
                    ValueType::Ratio,
                    Some("percent"),
                    false,
                    Some(ratio_range()),
                ),
                property(
                    "last_transaction",
                    "Last transaction",
                    ValueType::Map,
                    None,
                    false,
                    None,
                ),
            ],
            metadata: BTreeMap::from([
                (
                    "protocol".into(),
                    Value::String(
                        "Agilent Laser Combiner external-evidence serial protocol".into(),
                    ),
                ),
                (
                    "transport".into(),
                    Value::String("115200 8N1 binary request / echoed ASCII CRLF reply".into()),
                ),
                (
                    "completion".into(),
                    Value::String(
                        "command echo reply observed by the controller transaction layer".into(),
                    ),
                ),
            ]),
        }];
        for (index, info) in self.probe.lines.iter().enumerate() {
            let line_number = index + 1;
            descriptors.push(DeviceDescriptor {
                id: self.lines[index],
                driver: self.id,
                label: format!("agilent-laser-line-{line_number}"),
                vendor: Some("Agilent/Keysight".into()),
                model: Some(self.probe.model.clone()),
                serial: Some(self.probe.serial_number.clone()),
                kinds: vec!["light.source".into(), "laser".into(), "trigger.sink".into()],
                properties: vec![
                    property("line", "Line", ValueType::I64, None, false, None),
                    property(
                        "wavelength",
                        "Wavelength",
                        ValueType::Wavelength,
                        Some("nm"),
                        true,
                        None,
                    ),
                    sequenceable_property("enabled", "Enabled", ValueType::Bool, None, true, None),
                    sequenceable_property(
                        "intensity",
                        "Intensity",
                        ValueType::Ratio,
                        Some("percent"),
                        true,
                        Some(ratio_range()),
                    ),
                    sequenceable_property(
                        "power",
                        "Power",
                        ValueType::OpticalPower,
                        Some("mW"),
                        true,
                        Some(Range {
                            min: Value::OpticalPower(OpticalPower::from_milliwatts(0.0)),
                            max: Value::OpticalPower(info.max_power),
                        }),
                    ),
                    property(
                        "min_voltage",
                        "Minimum voltage",
                        ValueType::Voltage,
                        Some("V"),
                        false,
                        None,
                    ),
                    property(
                        "max_voltage",
                        "Maximum voltage",
                        ValueType::Voltage,
                        Some("V"),
                        false,
                        None,
                    ),
                    property(
                        "dac_bit_depth",
                        "DAC bit depth",
                        ValueType::I64,
                        None,
                        false,
                        Some(i64_range(0, 16)),
                    ),
                    property(
                        "max_power",
                        "Maximum power",
                        ValueType::OpticalPower,
                        Some("mW"),
                        false,
                        None,
                    ),
                    property(
                        "calibration",
                        "Calibration",
                        ValueType::List,
                        None,
                        false,
                        None,
                    ),
                ],
                metadata: BTreeMap::from([
                    ("line".into(), Value::I64(line_number as i64)),
                    ("wavelength".into(), Value::Wavelength(info.wavelength)),
                    ("max_power".into(), Value::OpticalPower(info.max_power)),
                    (
                        "remux".into(),
                        Value::String(
                            "enabled remultiplexes through hub state_mask command 0x0a".into(),
                        ),
                    ),
                ]),
            });
        }
        for (index, _device) in self.analog_outputs.iter().enumerate() {
            let channel = index + 1;
            descriptors.push(DeviceDescriptor {
                id: self.analog_outputs[index],
                driver: self.id,
                label: format!("agilent-analog-output-{channel}"),
                vendor: Some("Agilent/Keysight".into()),
                model: Some(self.probe.model.clone()),
                serial: Some(self.probe.serial_number.clone()),
                kinds: vec!["analog.output".into(), "diagnostic.raw".into()],
                properties: vec![property(
                    "raw_counts",
                    "Raw counts",
                    ValueType::I64,
                    Some("counts"),
                    true,
                    Some(i64_range(0, u16::MAX as i64)),
                )],
                metadata: BTreeMap::from([
                    ("channel".into(), Value::I64(channel as i64)),
                    (
                        "wire_command".into(),
                        Value::String("SetAnalogOutputRaw/GetAnalogOutputRaw".into()),
                    ),
                ]),
            });
        }
        descriptors
    }

    fn line_index(&self, device: DeviceId) -> Option<usize> {
        self.lines.iter().position(|candidate| *candidate == device)
    }

    fn analog_output_index(&self, device: DeviceId) -> Option<usize> {
        self.analog_outputs
            .iter()
            .position(|candidate| *candidate == device)
    }

    fn line_index_required(&self, device: DeviceId) -> Result<usize> {
        self.line_index(device).ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidCommand,
                "unknown Agilent Laser Combiner line device",
            )
        })
    }

    fn read_property(&mut self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "model" => Ok(Value::String(self.send(protocol::Command::Model)?)),
                "firmware_version" => Ok(Value::String(
                    self.send(protocol::Command::FirmwareVersion)?,
                )),
                "hardware_version" => Ok(Value::String(
                    self.send(protocol::Command::HardwareVersion)?,
                )),
                "serial_number" => Ok(Value::String(self.send(protocol::Command::SerialNumber)?)),
                "line_count" => Ok(Value::I64(self.probe.lines.len() as i64)),
                "state_mask" => {
                    self.probe.state_mask = protocol::parse_u8(
                        &protocol::Command::GetStateMask,
                        &self.send(protocol::Command::GetStateMask)?,
                    )?;
                    Ok(Value::I64(self.probe.state_mask as i64))
                }
                "shutter_open" => {
                    self.probe.shutter_open = protocol::parse_u8(
                        &protocol::Command::GetShutter,
                        &self.send(protocol::Command::GetShutter)?,
                    )? != 0;
                    Ok(Value::Bool(self.probe.shutter_open))
                }
                "external_control_enabled" => Ok(Value::Bool(self.probe.external_control_enabled)),
                "blanking_enabled" => Ok(Value::Bool(self.probe.blanking_enabled)),
                "sync_mode" => Ok(Value::I64(self.probe.sync_mode as i64)),
                "galvo_position" => Ok(Value::I64(self.probe.galvo_position as i64)),
                "nd_filter_state" => Ok(Value::I64(self.probe.nd_filter_state as i64)),
                "nd_filter_mapping" => Ok(Value::I64(self.probe.nd_filter_mapping as i64)),
                "direct_amplitude" => Ok(Value::Ratio(
                    self.probe
                        .lines
                        .first()
                        .map(|info| info.counts_to_ratio(self.probe.direct_amplitude))
                        .unwrap_or_else(|| Ratio::from_percent(0.0)),
                )),
                "saved_direct_amplitude" => Ok(Value::Ratio(
                    self.probe
                        .lines
                        .first()
                        .map(|info| info.counts_to_ratio(self.probe.saved_direct_amplitude))
                        .unwrap_or_else(|| Ratio::from_percent(0.0)),
                )),
                "last_transaction" => Ok(self.last_transaction.clone()),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown Agilent hub property {key}"),
                )),
            };
        }
        if let Some(index) = self.analog_output_index(device) {
            let channel = index as u8 + 1;
            if key == "raw_counts" {
                self.probe.analog_outputs[index] = protocol::parse_u16(
                    &protocol::Command::GetAnalogOutputRaw(channel),
                    &self.send(protocol::Command::GetAnalogOutputRaw(channel))?,
                )?;
                return Ok(Value::I64(self.probe.analog_outputs[index] as i64));
            }
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Agilent analog-output property {key}"),
            ));
        }
        let index = self.line_index_required(device)?;
        let line_number = index as u8 + 1;
        if key == "enabled" {
            self.probe.state_mask = protocol::parse_u8(
                &protocol::Command::GetStateMask,
                &self.send(protocol::Command::GetStateMask)?,
            )?;
        } else if matches!(key, "intensity" | "power") {
            self.probe.line_counts[index] = protocol::parse_u16(
                &protocol::Command::GetLinePowerRaw(line_number),
                &self.send(protocol::Command::GetLinePowerRaw(line_number))?,
            )?;
        }
        let info = &self.probe.lines[index];
        match key {
            "line" => Ok(Value::I64(line_number as i64)),
            "wavelength" => Ok(Value::Wavelength(info.wavelength)),
            "enabled" => Ok(Value::Bool(self.probe.state_mask & line_mask(index) != 0)),
            "intensity" => Ok(Value::Ratio(
                info.counts_to_ratio(self.probe.line_counts[index]),
            )),
            "power" => Ok(Value::OpticalPower(
                info.counts_to_power(self.probe.line_counts[index]),
            )),
            "min_voltage" => Ok(Value::Voltage(info.min_voltage)),
            "max_voltage" => Ok(Value::Voltage(info.max_voltage)),
            "dac_bit_depth" => Ok(Value::I64(info.dac_bit_depth as i64)),
            "max_power" => Ok(Value::OpticalPower(info.max_power)),
            "calibration" => Ok(Value::List(
                info.calibration
                    .iter()
                    .map(|value| Value::F64(*value as f64))
                    .collect(),
            )),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Agilent line property {key}"),
            )),
        }
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        let descriptor = self
            .descriptors_for()
            .into_iter()
            .find(|descriptor| descriptor.id == device)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown Agilent device"))?;
        let schema = descriptor
            .properties
            .iter()
            .find(|property| property.key == key)
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown Agilent property {key}"),
                )
            })?;
        if !schema.writable {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Agilent property {key} is read-only"),
            ));
        }
        schema.validate(value)
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: Value) -> Result<Value> {
        self.validate_write(device, key, &value)?;
        if device == self.hub {
            return self.write_hub_property(key, value);
        }
        if let Some(index) = self.analog_output_index(device) {
            return self.write_analog_output_property(index, key, value);
        }
        let index = self.line_index_required(device)?;
        self.write_line_property(index, key, value)
    }

    fn write_hub_property(&mut self, key: &str, value: Value) -> Result<Value> {
        match (key, value) {
            ("serial_number", Value::String(serial)) => {
                self.send(protocol::Command::SetSerialNumber(serial.clone()))?;
                self.probe.serial_number = serial.clone();
                self.last_transaction = self.transaction("set_serial_number", "request_reply");
                Ok(Value::String(serial))
            }
            ("shutter_open", Value::Bool(open)) => {
                self.send(protocol::Command::SetShutter(open))?;
                self.probe.shutter_open = open;
                self.last_transaction = self.transaction("set_shutter", "request_reply");
                Ok(Value::Bool(open))
            }
            ("external_control_enabled", Value::Bool(enabled)) => {
                self.send(protocol::Command::SetExternalControl(enabled))?;
                self.probe.external_control_enabled = enabled;
                self.last_transaction = self.transaction("set_external_control", "request_reply");
                Ok(Value::Bool(enabled))
            }
            ("blanking_enabled", Value::Bool(enabled)) => {
                self.send(protocol::Command::SetBlanking(enabled))?;
                self.probe.blanking_enabled = enabled;
                self.last_transaction = self.transaction("set_blanking", "request_reply");
                Ok(Value::Bool(enabled))
            }
            ("sync_mode", Value::I64(mode)) => {
                let mode = mode as u8;
                self.send(protocol::Command::SetSyncMode(mode))?;
                self.probe.sync_mode = mode;
                self.last_transaction = self.transaction("set_sync_mode", "request_reply");
                Ok(Value::I64(mode as i64))
            }
            ("galvo_position", Value::I64(position)) => {
                let position = position as u8;
                self.send(protocol::Command::SetGalvoPosition(position))?;
                self.probe.galvo_position = position;
                self.last_transaction = self.transaction("set_galvo_position", "request_reply");
                Ok(Value::I64(position as i64))
            }
            ("nd_filter_state", Value::I64(state)) => {
                let state = state as u8;
                self.send(protocol::Command::SetNdFilterState(state))?;
                self.probe.nd_filter_state = state;
                self.last_transaction = self.transaction("set_nd_filter_state", "request_reply");
                Ok(Value::I64(state as i64))
            }
            ("nd_filter_mapping", Value::I64(mapping)) => {
                let mapping = mapping as u8;
                self.send(protocol::Command::SetNdFilterMapping(mapping))?;
                self.probe.nd_filter_mapping = mapping;
                self.last_transaction = self.transaction("set_nd_filter_mapping", "request_reply");
                Ok(Value::I64(mapping as i64))
            }
            ("direct_amplitude", Value::Ratio(ratio)) => {
                let info = self.probe.lines.first().ok_or_else(|| {
                    Error::new(
                        ErrorCode::InvalidCommand,
                        "Agilent combiner has no laser lines",
                    )
                })?;
                let counts = info.ratio_to_counts(ratio);
                let readback = info.counts_to_ratio(counts);
                self.send(protocol::Command::SetDirectAmplitude { line: 1, counts })?;
                self.probe.direct_amplitude = counts;
                self.last_transaction = self.transaction("set_direct_amplitude", "request_reply");
                Ok(Value::Ratio(readback))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                "invalid Agilent hub write",
            )),
        }
    }

    fn write_line_property(&mut self, index: usize, key: &str, value: Value) -> Result<Value> {
        let line = index as u8 + 1;
        match (key, value) {
            ("wavelength", Value::Wavelength(wavelength)) => {
                self.send(protocol::Command::SetWavelength { line, wavelength })?;
                self.probe.lines[index].wavelength = Wavelength::from_nanometers(
                    wavelength.nanometers().round().clamp(0.0, u16::MAX as f64),
                );
                self.last_transaction = self.transaction("set_wavelength", "request_reply");
                Ok(Value::Wavelength(self.probe.lines[index].wavelength))
            }
            ("enabled", Value::Bool(enabled)) => {
                self.set_line_enabled(index, enabled)?;
                Ok(Value::Bool(enabled))
            }
            ("intensity", Value::Ratio(ratio)) => {
                self.set_line_counts(index, self.probe.lines[index].ratio_to_counts(ratio))
            }
            ("power", Value::OpticalPower(power)) => {
                self.set_line_counts(index, self.probe.lines[index].power_to_counts(power))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                "invalid Agilent line write",
            )),
        }
    }

    fn write_analog_output_property(
        &mut self,
        index: usize,
        key: &str,
        value: Value,
    ) -> Result<Value> {
        let channel = index as u8 + 1;
        match (key, value) {
            ("raw_counts", Value::I64(counts)) => {
                let counts = u16::try_from(counts).map_err(|_| {
                    Error::new(
                        ErrorCode::InvalidProperty,
                        "Agilent analog raw_counts is out of u16 range",
                    )
                })?;
                self.send(protocol::Command::SetAnalogOutputRaw { channel, counts })?;
                self.probe.analog_outputs[index] = counts;
                self.last_transaction = self.transaction("set_analog_output_raw", "request_reply");
                Ok(Value::I64(counts as i64))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                "invalid Agilent analog-output write",
            )),
        }
    }

    fn set_line_enabled(&mut self, index: usize, enabled: bool) -> Result<()> {
        self.probe.state_mask = set_mask_bit(self.probe.state_mask, index, enabled);
        self.send(protocol::Command::SetStateMask(self.probe.state_mask))?;
        self.last_transaction = self.transaction("set_state_mask", "request_reply");
        Ok(())
    }

    fn set_line_counts(&mut self, index: usize, counts: u16) -> Result<Value> {
        let line = index as u8 + 1;
        self.send(protocol::Command::SetLinePowerRaw { line, counts })?;
        self.probe.line_counts[index] = counts;
        self.last_transaction = self.transaction("set_line_power_raw", "request_reply");
        Ok(Value::Ratio(
            self.probe.lines[index].counts_to_ratio(counts),
        ))
    }

    fn apply_state_set(&mut self, set: StateSet) -> Result<Value> {
        let mut changed = BTreeMap::new();
        let mut next_mask = self.probe.state_mask;
        let mut mask_dirty = false;
        let mut power_writes = Vec::new();
        for write in set.writes {
            self.validate_write(write.device, &write.property, &write.value)?;
            if let Some(index) = self.line_index(write.device) {
                match (write.property.as_str(), write.value) {
                    ("enabled", Value::Bool(enabled)) => {
                        next_mask = set_mask_bit(next_mask, index, enabled);
                        mask_dirty = true;
                        changed.insert(
                            format!("{}:enabled", (write.device.0).0),
                            Value::Bool(enabled),
                        );
                    }
                    ("intensity", Value::Ratio(ratio)) => {
                        power_writes.push((index, self.probe.lines[index].ratio_to_counts(ratio)));
                    }
                    ("power", Value::OpticalPower(power)) => {
                        power_writes.push((index, self.probe.lines[index].power_to_counts(power)));
                    }
                    ("wavelength", value) => {
                        let value = self.write_line_property(index, "wavelength", value)?;
                        changed.insert(format!("{}:wavelength", (write.device.0).0), value);
                    }
                    _ => {}
                }
            } else if write.device == self.hub {
                let key = write.property.clone();
                let value = self.write_hub_property(&key, write.value)?;
                changed.insert(format!("{}:{key}", (self.hub.0).0), value);
            } else if let Some(index) = self.analog_output_index(write.device) {
                let key = write.property.clone();
                let value = self.write_analog_output_property(index, &key, write.value)?;
                changed.insert(format!("{}:{key}", (write.device.0).0), value);
            }
        }
        for (index, counts) in power_writes {
            let value = self.set_line_counts(index, counts)?;
            changed.insert(
                format!("{}:intensity", (self.lines[index].0).0),
                value.clone(),
            );
            self.emit_property(self.lines[index], "intensity", value.clone());
            self.emit_property(
                self.lines[index],
                "power",
                Value::OpticalPower(self.probe.lines[index].counts_to_power(counts)),
            );
        }
        if mask_dirty {
            self.probe.state_mask = next_mask;
            self.send(protocol::Command::SetStateMask(next_mask))?;
            self.last_transaction = self.transaction("set_state_mask", "request_reply");
            self.emit_property(self.hub, "state_mask", Value::I64(next_mask as i64));
            for (index, line) in self.lines.clone().into_iter().enumerate() {
                self.emit_property(
                    line,
                    "enabled",
                    Value::Bool(next_mask & line_mask(index) != 0),
                );
            }
        }
        Ok(Value::Map(changed))
    }

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| self.line_index(sequence.device).is_some())
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequences(plan) {
            if !matches!(
                sequence.property.as_str(),
                "enabled" | "intensity" | "power"
            ) {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Agilent timing sequences can only target enabled, intensity, or power",
                ));
            }
            for value in &sequence.values {
                self.validate_write(sequence.device, &sequence.property, value)?;
            }
        }
        Ok(())
    }

    fn timing_summary(&self, plan: &TimingPlan, phase: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("phase".into(), Value::String(phase.into())),
            ("hub".into(), Value::I64(self.hub.0 .0 as i64)),
            ("line_count".into(), Value::I64(self.lines.len() as i64)),
            (
                "sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
            (
                "state_mask".into(),
                Value::I64(self.probe.state_mask as i64),
            ),
            ("last_transaction".into(), self.last_transaction.clone()),
        ]))
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
        if writes.is_empty() {
            return Ok(Value::Map(BTreeMap::new()));
        }
        let applied = self.apply_state_set(StateSet {
            name: Some(if first {
                "agilent timing start sequence".into()
            } else {
                "agilent timing stop sequence".into()
            }),
            writes,
            commit: CommitMode::Immediate,
        })?;
        Ok(Value::Map(BTreeMap::from([
            ("applied".into(), applied),
            (
                "completion_basis".into(),
                Value::String("command echo reply".into()),
            ),
        ])))
    }

    fn invoke(
        &mut self,
        device: DeviceId,
        kind: CapabilityKind,
        request: CapabilityRequest,
    ) -> Result<Value> {
        match kind {
            CapabilityKind::Dac => {
                let CapabilityRequest::Dac(request) = request else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "Agilent Dac expects DacRequest",
                    ));
                };
                match request.value {
                    Value::Ratio(ratio) => {
                        self.write_property(device, "intensity", Value::Ratio(ratio))
                    }
                    Value::OpticalPower(power) => {
                        self.write_property(device, "power", Value::OpticalPower(power))
                    }
                    _ => Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Agilent Dac value must be Ratio or OpticalPower",
                    )),
                }
            }
            CapabilityKind::TriggerSink => {
                let action = match request {
                    CapabilityRequest::None => TriggerAction::Pulse,
                    CapabilityRequest::Trigger(request) => request.action,
                    _ => {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "Agilent TriggerSink expects TriggerRequest",
                        ))
                    }
                };
                let actions = match action {
                    TriggerAction::Enable => vec![true],
                    TriggerAction::Disable => vec![false],
                    TriggerAction::Pulse => vec![true, false],
                };
                for enabled in &actions {
                    if device == self.hub {
                        let value =
                            self.write_hub_property("shutter_open", Value::Bool(*enabled))?;
                        self.emit_property(self.hub, "shutter_open", value);
                    } else {
                        let index = self.line_index_required(device)?;
                        self.set_line_enabled(index, *enabled)?;
                        self.emit_property(device, "enabled", Value::Bool(*enabled));
                    }
                }
                Ok(Value::Map(BTreeMap::from([
                    ("triggered".into(), Value::Bool(true)),
                    ("commands".into(), Value::I64(actions.len() as i64)),
                ])))
            }
            CapabilityKind::GenericCommand if device == self.hub => {
                let CapabilityRequest::GenericCommand(request) = request else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "Agilent GenericCommand expects a GenericCommandRequest",
                    ));
                };
                self.apply_generic_command(request)
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Agilent capability",
            )),
        }
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
                "Agilent GenericCommand does not take parameters",
            ));
        }
        let _ = agilent_generic_command_kind(&request.command)?;
        Ok(())
    }

    fn apply_generic_command(&mut self, request: GenericCommandRequest) -> Result<Value> {
        self.validate_generic_command(&request)?;
        match agilent_generic_command_kind(&request.command)? {
            AgilentGenericCommand::RefreshIdentity => {
                self.refresh_identity_readback()?;
                Ok(self.identity_summary(request.command))
            }
            AgilentGenericCommand::RefreshControlState => {
                self.refresh_control_state_readback()?;
                Ok(self.control_state_summary(request.command))
            }
            AgilentGenericCommand::RefreshLineOutputs => {
                self.refresh_line_output_readback()?;
                Ok(self.line_output_summary(request.command))
            }
            AgilentGenericCommand::RefreshLineMetadata => {
                self.refresh_line_metadata_readback()?;
                Ok(self.line_metadata_summary(request.command))
            }
        }
    }

    fn refresh_identity_readback(&mut self) -> Result<()> {
        self.probe.model = self.send(protocol::Command::Model)?;
        self.probe.firmware_version = self.send(protocol::Command::FirmwareVersion)?;
        self.probe.serial_number = self.send(protocol::Command::SerialNumber)?;
        self.probe.hardware_version = self.send(protocol::Command::HardwareVersion)?;
        self.last_transaction = self.transaction("refresh_identity", "request_reply");
        Ok(())
    }

    fn refresh_control_state_readback(&mut self) -> Result<()> {
        self.probe.state_mask = protocol::parse_u8(
            &protocol::Command::GetStateMask,
            &self.send(protocol::Command::GetStateMask)?,
        )?;
        self.probe.external_control_enabled = protocol::parse_u8(
            &protocol::Command::GetExternalControl,
            &self.send(protocol::Command::GetExternalControl)?,
        )? != 0;
        self.probe.blanking_enabled = protocol::parse_u8(
            &protocol::Command::GetBlanking,
            &self.send(protocol::Command::GetBlanking)?,
        )? != 0;
        self.probe.sync_mode = protocol::parse_u8(
            &protocol::Command::GetSyncMode,
            &self.send(protocol::Command::GetSyncMode)?,
        )?;
        self.probe.shutter_open = protocol::parse_u8(
            &protocol::Command::GetShutter,
            &self.send(protocol::Command::GetShutter)?,
        )? != 0;
        self.probe.galvo_position = protocol::parse_u8(
            &protocol::Command::GetGalvoPosition,
            &self.send(protocol::Command::GetGalvoPosition)?,
        )?;
        self.probe.nd_filter_state = protocol::parse_u8(
            &protocol::Command::GetNdFilterState,
            &self.send(protocol::Command::GetNdFilterState)?,
        )?;
        self.probe.nd_filter_mapping = protocol::parse_u8(
            &protocol::Command::GetNdFilterMapping,
            &self.send(protocol::Command::GetNdFilterMapping)?,
        )?;
        self.probe.direct_amplitude = protocol::parse_u16(
            &protocol::Command::GetDirectAmplitude,
            &self.send(protocol::Command::GetDirectAmplitude)?,
        )?;
        self.probe.saved_direct_amplitude = protocol::parse_u16(
            &protocol::Command::GetSavedDirectAmplitude,
            &self.send(protocol::Command::GetSavedDirectAmplitude)?,
        )?;
        self.last_transaction = self.transaction("refresh_control_state", "request_reply");
        Ok(())
    }

    fn refresh_line_output_readback(&mut self) -> Result<()> {
        self.probe.state_mask = protocol::parse_u8(
            &protocol::Command::GetStateMask,
            &self.send(protocol::Command::GetStateMask)?,
        )?;
        for index in 0..self.probe.lines.len() {
            let line = index as u8 + 1;
            self.probe.line_counts[index] = protocol::parse_u16(
                &protocol::Command::GetLinePowerRaw(line),
                &self.send(protocol::Command::GetLinePowerRaw(line))?,
            )?;
        }
        self.last_transaction = self.transaction("refresh_line_outputs", "request_reply");
        Ok(())
    }

    fn refresh_line_metadata_readback(&mut self) -> Result<()> {
        for index in 0..self.probe.lines.len() {
            let line = index as u8 + 1;
            let wavelength = protocol::parse_u16(
                &protocol::Command::GetWavelength(line),
                &self.send(protocol::Command::GetWavelength(line))?,
            )?;
            let min_voltage = protocol::parse_f64(
                &protocol::Command::GetMinVoltage(line),
                &self.send(protocol::Command::GetMinVoltage(line))?,
            )?;
            let max_voltage = protocol::parse_f64(
                &protocol::Command::GetMaxVoltage(line),
                &self.send(protocol::Command::GetMaxVoltage(line))?,
            )?;
            let bit_depth = protocol::parse_u8(
                &protocol::Command::GetDacBitDepth(line),
                &self.send(protocol::Command::GetDacBitDepth(line))?,
            )?;
            let max_power = protocol::parse_f64(
                &protocol::Command::GetMaxPower(line),
                &self.send(protocol::Command::GetMaxPower(line))?,
            )?;
            let mut calibration = [0.0; protocol::CALIBRATION_COEFFICIENTS];
            for coefficient in 0..protocol::CALIBRATION_COEFFICIENTS {
                calibration[coefficient] = protocol::parse_f64(
                    &protocol::Command::GetCalibrationCoefficient {
                        line,
                        index: coefficient as u8,
                    },
                    &self.send(protocol::Command::GetCalibrationCoefficient {
                        line,
                        index: coefficient as u8,
                    })?,
                )? as f32;
            }
            self.probe.lines[index] = protocol::LineInfo {
                wavelength: Wavelength::from_nanometers(wavelength as f64),
                min_voltage: Voltage::from_volts(min_voltage),
                max_voltage: Voltage::from_volts(max_voltage),
                dac_bit_depth: bit_depth.min(16),
                max_power: OpticalPower::from_milliwatts(max_power),
                calibration,
            };
        }
        self.last_transaction = self.transaction("refresh_line_metadata", "request_reply");
        Ok(())
    }

    fn identity_summary(&self, command: String) -> Value {
        Value::Map(BTreeMap::from([
            ("command".into(), Value::String(command)),
            ("model".into(), Value::String(self.probe.model.clone())),
            (
                "firmware_version".into(),
                Value::String(self.probe.firmware_version.clone()),
            ),
            (
                "hardware_version".into(),
                Value::String(self.probe.hardware_version.clone()),
            ),
            (
                "serial_number".into(),
                Value::String(self.probe.serial_number.clone()),
            ),
            (
                "completion_basis".into(),
                Value::String("Agilent request/reply readback".into()),
            ),
        ]))
    }

    fn control_state_summary(&self, command: String) -> Value {
        Value::Map(BTreeMap::from([
            ("command".into(), Value::String(command)),
            (
                "state_mask".into(),
                Value::I64(self.probe.state_mask as i64),
            ),
            (
                "external_control_enabled".into(),
                Value::Bool(self.probe.external_control_enabled),
            ),
            (
                "blanking_enabled".into(),
                Value::Bool(self.probe.blanking_enabled),
            ),
            ("sync_mode".into(), Value::I64(self.probe.sync_mode as i64)),
            ("shutter_open".into(), Value::Bool(self.probe.shutter_open)),
            (
                "galvo_position".into(),
                Value::I64(self.probe.galvo_position as i64),
            ),
            (
                "nd_filter_state".into(),
                Value::I64(self.probe.nd_filter_state as i64),
            ),
            (
                "nd_filter_mapping".into(),
                Value::I64(self.probe.nd_filter_mapping as i64),
            ),
            (
                "completion_basis".into(),
                Value::String("Agilent request/reply readback".into()),
            ),
        ]))
    }

    fn line_output_summary(&self, command: String) -> Value {
        Value::Map(BTreeMap::from([
            ("command".into(), Value::String(command)),
            (
                "state_mask".into(),
                Value::I64(self.probe.state_mask as i64),
            ),
            (
                "line_counts".into(),
                Value::List(
                    self.probe
                        .line_counts
                        .iter()
                        .map(|counts| Value::I64(*counts as i64))
                        .collect(),
                ),
            ),
            (
                "completion_basis".into(),
                Value::String("Agilent request/reply readback".into()),
            ),
        ]))
    }

    fn line_metadata_summary(&self, command: String) -> Value {
        Value::Map(BTreeMap::from([
            ("command".into(), Value::String(command)),
            (
                "lines".into(),
                Value::List(
                    self.probe
                        .lines
                        .iter()
                        .enumerate()
                        .map(|(index, line)| {
                            Value::Map(BTreeMap::from([
                                ("line".into(), Value::I64(index as i64 + 1)),
                                ("wavelength".into(), Value::Wavelength(line.wavelength)),
                                ("max_power".into(), Value::OpticalPower(line.max_power)),
                                (
                                    "dac_bit_depth".into(),
                                    Value::I64(line.dac_bit_depth as i64),
                                ),
                            ]))
                        })
                        .collect(),
                ),
            ),
            (
                "completion_basis".into(),
                Value::String("Agilent request/reply readback".into()),
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

    fn transaction(&self, command: &str, completion_basis: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("command".into(), Value::String(command.into())),
            (
                "state_mask".into(),
                Value::I64(self.probe.state_mask as i64),
            ),
            (
                "line_count".into(),
                Value::I64(self.probe.lines.len() as i64),
            ),
            (
                "completion_basis".into(),
                Value::String(completion_basis.into()),
            ),
        ]))
    }
}

impl Driver for AgilentLaserCombinerDriver {
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
            label: "agilent-combiner-transport".into(),
            kind: "serial.binary_ascii".into(),
            metadata: BTreeMap::from([
                ("baud_rate".into(), Value::I64(protocol::BAUD as i64)),
                ("data_bits".into(), Value::I64(8)),
                ("parity".into(), Value::String("none".into())),
                ("stop_bits".into(), Value::I64(1)),
                ("flow_control".into(), Value::String("none".into())),
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
                    "reply".into(),
                    Value::String("command echo byte plus ASCII payload plus CRLF".into()),
                ),
            ]),
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.hub {
            vec![
                capability(1, device, CapabilityKind::TriggerSink),
                capability(2, device, CapabilityKind::GenericCommand),
            ]
        } else if self.line_index(device).is_some() {
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
                    let _ = self
                        .descriptors_for()
                        .into_iter()
                        .find(|d| d.id == *device)
                        .ok_or_else(|| {
                            Error::new(ErrorCode::InvalidCommand, "unknown Agilent device")
                        })?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("agilent read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("agilent write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "agilent remultiplexed light state set".into(),
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
                            "unknown Agilent capability",
                        ));
                    };
                    if !capability.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "Agilent {:?} expects {:?}, got {:?}",
                                capability.kind,
                                capability.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    if capability.kind == CapabilityKind::GenericCommand {
                        let CapabilityRequest::GenericCommand(request) = request else {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Agilent GenericCommand expects a GenericCommandRequest",
                            ));
                        };
                        self.validate_generic_command(request)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("agilent invoke {}", capability.kind.name()),
                        payload: Value::String(capability.kind.name().into()),
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
                    last = self.read_property(device, &key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    last = self.write_property(device, &key, value)?;
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
                            "unknown Agilent capability",
                        ));
                    };
                    if !capability.accepts_request(&request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "Agilent capability request has wrong type",
                        ));
                    }
                    last = self.invoke(device, capability.kind, request)?;
                }
                Command::Arm(plan) => {
                    self.validate_timing_plan(&plan)?;
                    last = self.timing_summary(&plan, "arm");
                }
                Command::Start(_) | Command::Stop(_) => {}
            }
        }
        self.pending
            .push_back(DriverEvent::TokenCompleted { token, value: last });
        Ok(token)
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
                description: "agilent timing arm summary".into(),
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
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Start(OperationId(0))],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "agilent timing start sequence".into(),
                payload: Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "start")),
                    ("applied".into(), applied),
                ])),
            }],
        })
    }

    fn stop_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let applied = self.apply_timing_sequence_step(&armed.plan, false)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "agilent timing stop sequence".into(),
                payload: Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "stop")),
                    ("applied".into(), applied),
                ])),
            }],
        })
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        self.pending.drain(..).collect()
    }
}

#[derive(Debug, Clone, Default)]
struct ReplyReader {
    buffer: Vec<u8>,
}

impl ReplyReader {
    fn push(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.buffer.extend_from_slice(bytes);
        let mut replies = Vec::new();
        while let Some(index) = self.buffer.windows(2).position(|window| window == b"\r\n") {
            let reply = self.buffer.drain(..index).collect::<Vec<_>>();
            self.buffer.drain(..2);
            replies.push(reply);
        }
        replies
    }
}

fn configured_line_info(device: &DeviceConfig, line: usize) -> protocol::LineInfo {
    let mut info = protocol::LineInfo::fixture(line);
    if let Some(value) = wavelength_prop(device, &format!("line_{line}_wavelength")) {
        info.wavelength = Wavelength::from_nanometers(value);
    }
    if let Some(value) = f64_prop(device, &format!("line_{line}_min_voltage")) {
        info.min_voltage = Voltage::from_volts(value);
    }
    if let Some(value) = f64_prop(device, &format!("line_{line}_max_voltage")) {
        info.max_voltage = Voltage::from_volts(value);
    }
    if let Some(value) = u8_prop(device, &format!("line_{line}_dac_bit_depth")) {
        info.dac_bit_depth = value.min(16);
    }
    if let Some(value) = optical_power_prop(device, &format!("line_{line}_max_power")) {
        info.max_power = value;
    }
    for index in 0..protocol::CALIBRATION_COEFFICIENTS {
        if let Some(value) = f64_prop(device, &format!("line_{line}_calibration_{index}")) {
            info.calibration[index] = value as f32;
        }
    }
    info
}

fn set_mask_bit(mask: u8, index: usize, enabled: bool) -> u8 {
    if enabled {
        mask | line_mask(index)
    } else {
        mask & !line_mask(index)
    }
}

fn line_mask(index: usize) -> u8 {
    if index < 8 {
        1 << index
    } else {
        0
    }
}

fn capability(id: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    CapabilityDescriptor::new(CapabilityId(id), device, kind, ValueType::Map)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgilentGenericCommand {
    RefreshIdentity,
    RefreshControlState,
    RefreshLineOutputs,
    RefreshLineMetadata,
}

fn agilent_generic_command_kind(command: &str) -> Result<AgilentGenericCommand> {
    match command {
        "refresh_identity" => Ok(AgilentGenericCommand::RefreshIdentity),
        "refresh_control_state" => Ok(AgilentGenericCommand::RefreshControlState),
        "refresh_line_outputs" => Ok(AgilentGenericCommand::RefreshLineOutputs),
        "refresh_line_metadata" => Ok(AgilentGenericCommand::RefreshLineMetadata),
        other => Err(Error::new(
            ErrorCode::Unsupported,
            format!(
                "Agilent GenericCommand supports refresh_identity, refresh_control_state, refresh_line_outputs, and refresh_line_metadata; got {other}"
            ),
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

fn i64_range(min: i64, max: i64) -> Range {
    Range {
        min: Value::I64(min),
        max: Value::I64(max),
    }
}

fn ratio_range() -> Range {
    Range {
        min: Value::Ratio(Ratio::from_percent(0.0)),
        max: Value::Ratio(Ratio::from_percent(100.0)),
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

fn u8_prop(device: &DeviceConfig, key: &str) -> Option<u8> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => u8::try_from(*value).ok(),
        _ => None,
    }
}

fn u16_prop(device: &DeviceConfig, key: &str) -> Option<u16> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => u16::try_from(*value).ok(),
        _ => None,
    }
}

fn usize_prop(device: &DeviceConfig, key: &str) -> Option<usize> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => usize::try_from(*value).ok(),
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

fn ratio_prop(device: &DeviceConfig, key: &str) -> Option<Ratio> {
    match device.properties.get(key) {
        Some(Value::Ratio(value)) => Some(*value),
        _ => None,
    }
}

fn wavelength_prop(device: &DeviceConfig, key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::Wavelength(value)) => Some(value.nanometers()),
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn optical_power_prop(device: &DeviceConfig, key: &str) -> Option<OpticalPower> {
    match device.properties.get(key) {
        Some(Value::OpticalPower(value)) => Some(*value),
        Some(Value::F64(value)) => Some(OpticalPower::from_milliwatts(*value)),
        Some(Value::I64(value)) => Some(OpticalPower::from_milliwatts(*value as f64)),
        _ => None,
    }
}
