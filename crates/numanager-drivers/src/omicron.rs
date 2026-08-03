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
    pub const DAC_MAX: u16 = 4095;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DeviceFamily {
        PhoxX,
        LuxX,
        BrixX,
        Unknown(u16),
    }

    impl DeviceFamily {
        pub fn from_code(code: u16) -> Self {
            match code {
                3 => DeviceFamily::PhoxX,
                4 => DeviceFamily::LuxX,
                100 => DeviceFamily::BrixX,
                other => DeviceFamily::Unknown(other),
            }
        }

        pub fn label(self) -> String {
            match self {
                DeviceFamily::PhoxX => "PhoxX diode laser".into(),
                DeviceFamily::LuxX => "LuxX diode laser".into(),
                DeviceFamily::BrixX => "BrixX diode laser".into(),
                DeviceFamily::Unknown(code) => format!("Omicron device {code}"),
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum OperatingMode {
        Standby,
        Cw,
        AnalogModulation,
        DigitalModulation,
        AnalogDigitalModulation,
    }

    impl OperatingMode {
        pub fn label(self) -> &'static str {
            match self {
                OperatingMode::Standby => "Standby",
                OperatingMode::Cw => "CW",
                OperatingMode::AnalogModulation => "Analog Modulation",
                OperatingMode::DigitalModulation => "Digital Modulation",
                OperatingMode::AnalogDigitalModulation => "Analog + Digital Modulation",
            }
        }

        pub fn apply_to_bits(self, mut bits: u16) -> u16 {
            match self {
                OperatingMode::Standby => {
                    bits &= !(1 << 3);
                    bits &= !(1 << 4);
                }
                OperatingMode::Cw => {
                    bits |= 1 << 4;
                    bits &= !(1 << 5);
                    bits &= !(1 << 7);
                }
                OperatingMode::AnalogModulation => {
                    bits |= 1 << 4;
                    bits &= !(1 << 5);
                    bits |= 1 << 7;
                }
                OperatingMode::DigitalModulation => {
                    bits |= 1 << 4;
                    bits |= 1 << 5;
                    bits &= !(1 << 7);
                }
                OperatingMode::AnalogDigitalModulation => {
                    bits |= 1 << 4;
                    bits |= 1 << 5;
                    bits |= 1 << 7;
                }
            }
            bits
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CwSubMode {
        Acc,
        Apc,
    }

    impl CwSubMode {
        pub fn label(self) -> &'static str {
            match self {
                CwSubMode::Acc => "ACC",
                CwSubMode::Apc => "APC",
            }
        }

        pub fn apply_to_bits(self, bits: u16) -> u16 {
            match self {
                CwSubMode::Acc => bits & !(1 << 8),
                CwSubMode::Apc => bits | (1 << 8),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct OmicronProbe {
        pub family: DeviceFamily,
        pub serial_number: String,
        pub wavelength: Wavelength,
        pub specified_power: OpticalPower,
        pub firmware: String,
    }

    impl OmicronProbe {
        pub fn simulated() -> Self {
            Self {
                family: DeviceFamily::LuxX,
                serial_number: "OMICRON-SIM-488".into(),
                wavelength: Wavelength::from_nanometers(488.0),
                specified_power: OpticalPower::from_milliwatts(120.0),
                firmware: "numanager-sim".into(),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct OmicronProbeResult {
        pub probe: OmicronProbe,
        pub enabled: bool,
        pub power_level: u16,
        pub power_percent: f64,
        pub actual_power: OpticalPower,
        pub operating_bits: u16,
        pub operating_mode: OperatingMode,
        pub cw_submode: CwSubMode,
        pub fault_bits: u16,
        pub fault: String,
        pub interlock_closed: bool,
        pub hours: TimeInterval,
        pub diode_temperature: Temperature,
        pub baseplate_temperature: Temperature,
        pub replies: Vec<(String, String)>,
    }

    impl OmicronProbeResult {
        pub fn from_replies(replies: &[(impl AsRef<str>, impl AsRef<str>)]) -> Result<Self> {
            let mut probe = OmicronProbe::simulated();
            let mut enabled = false;
            let mut power_level = 0;
            let mut actual_power = OpticalPower::from_milliwatts(0.0);
            let mut operating_bits = 0;
            let mut fault_bits = 0;
            let mut hours = TimeInterval::from_hours(0.0);
            let mut diode_temperature = Temperature::from_celsius(0.0);
            let mut baseplate_temperature = Temperature::from_celsius(0.0);
            let mut stored = Vec::new();

            for (command, reply) in replies {
                let command = command.as_ref();
                let reply = reply.as_ref().trim();
                stored.push((command.to_string(), reply.to_string()));
                match command {
                    "?GFw" => probe.firmware = parse_payload(reply, "!GFw").to_string(),
                    "?GSN" => probe.serial_number = parse_payload(reply, "!GSN").to_string(),
                    "?GSI" => {
                        (probe.wavelength, probe.specified_power) = parse_spec_info(reply)?;
                        probe.family = parse_family_from_spec_info(reply).unwrap_or(probe.family);
                    }
                    "?GOM" => operating_bits = parse_prefixed_hex(reply, "!GOM")?,
                    "?GFB" => fault_bits = parse_prefixed_hex(reply, "!GFB")?,
                    "?GWH" => hours = TimeInterval::from_hours(parse_number_reply("!GWH", reply)?),
                    "?GLP" => power_level = parse_prefixed_hex(reply, "!GLP")?,
                    "?MDP" => {
                        actual_power =
                            OpticalPower::from_milliwatts(parse_number_reply("!MDP", reply)?)
                    }
                    "?MTA" => {
                        baseplate_temperature =
                            Temperature::from_celsius(parse_number_reply("!MTA", reply)?)
                    }
                    "?MTD" => {
                        diode_temperature =
                            Temperature::from_celsius(parse_number_reply("!MTD", reply)?)
                    }
                    "?GAS" => enabled = parse_laser_state(reply)?,
                    _ => {}
                }
            }

            let operating_mode = operating_mode_from_bits(operating_bits);
            let cw_submode = if (operating_bits >> 8) & 1 == 1 {
                CwSubMode::Apc
            } else {
                CwSubMode::Acc
            };
            Ok(Self {
                probe,
                enabled,
                power_level,
                power_percent: percent_from_level(power_level),
                actual_power,
                operating_bits,
                operating_mode,
                cw_submode,
                fault_bits,
                fault: fault_text(fault_bits),
                interlock_closed: (fault_bits >> 9) & 1 == 0,
                hours,
                diode_temperature,
                baseplate_temperature,
                replies: stored,
            })
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum OmicronCommand {
        Firmware,
        QueryOperatingMode,
        SetOperatingModeBits(u16),
        QueryFaultBits,
        QueryHours,
        QueryPowerLevel,
        SetPowerLevel(u16),
        QueryActualPower,
        QuerySerialNumber,
        QuerySpecInfo,
        QueryBaseplateTemperature,
        QueryDiodeTemperature,
        QueryLaserState,
        LaserOn,
        LaserOff,
        Reset,
    }

    pub fn encode(command: &OmicronCommand) -> String {
        match command {
            OmicronCommand::Firmware => "?GFw".into(),
            OmicronCommand::QueryOperatingMode => "?GOM".into(),
            OmicronCommand::SetOperatingModeBits(bits) => format!("?SOM{bits:x}"),
            OmicronCommand::QueryFaultBits => "?GFB".into(),
            OmicronCommand::QueryHours => "?GWH".into(),
            OmicronCommand::QueryPowerLevel => "?GLP".into(),
            OmicronCommand::SetPowerLevel(level) => {
                format!("?SLP{:03x}", (*level).min(DAC_MAX))
            }
            OmicronCommand::QueryActualPower => "?MDP".into(),
            OmicronCommand::QuerySerialNumber => "?GSN".into(),
            OmicronCommand::QuerySpecInfo => "?GSI".into(),
            OmicronCommand::QueryBaseplateTemperature => "?MTA".into(),
            OmicronCommand::QueryDiodeTemperature => "?MTD".into(),
            OmicronCommand::QueryLaserState => "?GAS".into(),
            OmicronCommand::LaserOn => "?LOn".into(),
            OmicronCommand::LaserOff => "?LOf".into(),
            OmicronCommand::Reset => "?RsC".into(),
        }
    }

    pub fn level_from_percent(percent: f64) -> u16 {
        ((percent.clamp(0.0, 100.0) / 100.0 * DAC_MAX as f64) + 0.5) as u16
    }

    pub fn percent_from_level(level: u16) -> f64 {
        level.min(DAC_MAX) as f64 * 100.0 / DAC_MAX as f64
    }

    pub fn level_from_power(power: OpticalPower, specified_power: OpticalPower) -> u16 {
        let percent = power.milliwatts() / specified_power.milliwatts().max(f64::EPSILON) * 100.0;
        level_from_percent(percent)
    }

    pub fn power_from_level(level: u16, specified_power: OpticalPower) -> OpticalPower {
        OpticalPower::from_milliwatts(
            level.min(DAC_MAX) as f64 * specified_power.milliwatts() / DAC_MAX as f64,
        )
    }

    pub fn parse_prefixed_hex(reply: &str, prefix: &str) -> Result<u16> {
        let payload = reply
            .trim()
            .strip_prefix(prefix)
            .ok_or_else(|| Error::new(ErrorCode::Transport, format!("missing prefix {prefix}")))?;
        u16::from_str_radix(payload.trim(), 16)
            .map_err(|_| Error::new(ErrorCode::Transport, "invalid Omicron hex value"))
    }

    pub fn parse_spec_info(reply: &str) -> Result<(Wavelength, OpticalPower)> {
        let payload = reply
            .trim()
            .strip_prefix("!GSI")
            .ok_or_else(|| Error::new(ErrorCode::Transport, "missing !GSI prefix"))?;
        let mut parts = payload.split('\u{00a7}');
        let wavelength = parts
            .next()
            .ok_or_else(|| Error::new(ErrorCode::Transport, "missing Omicron wavelength"))?
            .parse::<f64>()
            .map_err(|_| Error::new(ErrorCode::Transport, "invalid Omicron wavelength"))?;
        let power = parts
            .next()
            .ok_or_else(|| Error::new(ErrorCode::Transport, "missing Omicron specified power"))?
            .parse::<f64>()
            .map_err(|_| Error::new(ErrorCode::Transport, "invalid Omicron specified power"))?;
        Ok((
            Wavelength::from_nanometers(wavelength),
            OpticalPower::from_milliwatts(power),
        ))
    }

    pub fn fault_text(bits: u16) -> String {
        let flags = [
            (0, "Interlock but no failure is pending - reset required"),
            (4, "CDRH error"),
            (5, "Internal communication error"),
            (8, "Under/over-voltage error"),
            (9, "Interlock loop is open"),
            (10, "Diode current exceeded maximum"),
            (11, "Ambient temperature out of range"),
            (12, "Diode temperature out of range"),
            (14, "Internal error"),
            (15, "Diode power exceeded maximum"),
        ];
        let active = flags
            .iter()
            .filter_map(|(bit, label)| ((bits >> bit) & 1 == 1).then_some(*label))
            .collect::<Vec<_>>();
        if active.is_empty() {
            "No Error".into()
        } else {
            active.join("; ")
        }
    }

    pub fn operating_bit_labels(bits: u16) -> String {
        let flags = [
            (3, "standby-disable"),
            (4, "laser-emission"),
            (5, "digital-modulation"),
            (7, "analog-modulation"),
            (8, "apc"),
        ];
        let active = flags
            .iter()
            .filter_map(|(bit, label)| ((bits >> bit) & 1 == 1).then_some(*label))
            .collect::<Vec<_>>();
        if active.is_empty() {
            "none".into()
        } else {
            active.join(" ")
        }
    }

    pub fn fault_bit_labels(bits: u16) -> String {
        let flags = [
            (0, "reset-required"),
            (4, "cdrh"),
            (5, "internal-communication"),
            (8, "voltage"),
            (9, "interlock-open"),
            (10, "diode-current"),
            (11, "ambient-temperature"),
            (12, "diode-temperature"),
            (14, "internal"),
            (15, "diode-power"),
        ];
        let active = flags
            .iter()
            .filter_map(|(bit, label)| ((bits >> bit) & 1 == 1).then_some(*label))
            .collect::<Vec<_>>();
        if active.is_empty() {
            "none".into()
        } else {
            active.join(" ")
        }
    }

    pub fn probe_commands() -> Vec<OmicronCommand> {
        vec![
            OmicronCommand::Firmware,
            OmicronCommand::QuerySerialNumber,
            OmicronCommand::QuerySpecInfo,
            OmicronCommand::QueryOperatingMode,
            OmicronCommand::QueryFaultBits,
            OmicronCommand::QueryHours,
            OmicronCommand::QueryPowerLevel,
            OmicronCommand::QueryActualPower,
            OmicronCommand::QueryLaserState,
            OmicronCommand::QueryBaseplateTemperature,
            OmicronCommand::QueryDiodeTemperature,
        ]
    }

    pub fn probe_script() -> Vec<String> {
        probe_commands().iter().map(encode).collect()
    }

    pub fn execute_probe_script(
        serial: &mut dyn SerialIo,
        polls_per_command: usize,
    ) -> Result<OmicronProbeResult> {
        let mut codec = SerialLineCodec::new(SEND_ENDING, RECV_ENDING);
        let mut replies = Vec::new();
        for command in probe_commands() {
            let encoded = encode(&command);
            serial.write(&codec.encode(&encoded))?;
            replies.push((encoded, read_line(serial, &mut codec, polls_per_command)?));
        }
        OmicronProbeResult::from_replies(&replies)
    }

    pub fn operating_mode_from_bits(bits: u16) -> OperatingMode {
        let emission = (bits >> 4) & 1 == 1;
        let digital = (bits >> 5) & 1 == 1;
        let analog = (bits >> 7) & 1 == 1;
        match (emission, analog, digital) {
            (false, _, _) => OperatingMode::Standby,
            (true, false, false) => OperatingMode::Cw,
            (true, true, false) => OperatingMode::AnalogModulation,
            (true, false, true) => OperatingMode::DigitalModulation,
            (true, true, true) => OperatingMode::AnalogDigitalModulation,
        }
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
            "timed out waiting for Omicron probe reply",
        ))
    }

    fn parse_payload<'a>(reply: &'a str, prefix: &str) -> &'a str {
        reply
            .trim()
            .strip_prefix(prefix)
            .unwrap_or(reply.trim())
            .trim()
    }

    pub(crate) fn parse_number_reply(prefix: &str, reply: &str) -> Result<f64> {
        parse_payload(reply, prefix)
            .parse::<f64>()
            .map_err(|error| {
                Error::new(
                    ErrorCode::Transport,
                    format!("invalid Omicron {prefix} number {reply}: {error}"),
                )
            })
    }

    pub(crate) fn parse_laser_state(reply: &str) -> Result<bool> {
        match parse_payload(reply, "!GAS").to_ascii_lowercase().as_str() {
            "1" | "on" | "laser on" => Ok(true),
            "0" | "off" | "laser off" => Ok(false),
            other => Err(Error::new(
                ErrorCode::Transport,
                format!("invalid Omicron laser state {other}"),
            )),
        }
    }

    pub(crate) fn parse_family_from_spec_info(reply: &str) -> Option<DeviceFamily> {
        reply
            .trim()
            .strip_prefix("!GSI")?
            .split('\u{00a7}')
            .nth(2)
            .and_then(|token| token.parse::<u16>().ok())
            .map(DeviceFamily::from_code)
    }
}

pub struct OmicronDiscovery {
    next_id: DriverId,
    probes: Vec<OmicronConfiguredProbe>,
}

impl OmicronDiscovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![OmicronConfiguredProbe::simulated()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| device.driver == "omicron")
            .map(OmicronConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for OmicronDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver = if configured.connect_real_transport {
                    Box::new(OmicronDriver::serial(id, configured)?) as Box<dyn Driver>
                } else {
                    Box::new(OmicronDriver::configured_fixture(id, configured)) as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct OmicronConfiguredProbe {
    pub label: String,
    pub probe: protocol::OmicronProbe,
    pub enabled: bool,
    pub power_level: u16,
    pub actual_power: OpticalPower,
    pub operating_bits: u16,
    pub operating_mode: protocol::OperatingMode,
    pub cw_submode: protocol::CwSubMode,
    pub fault_bits: u16,
    pub hours: TimeInterval,
    pub diode_temperature: Temperature,
    pub baseplate_temperature: Temperature,
    pub endpoint: Option<OmicronSerialEndpoint>,
    pub connect_real_transport: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmicronSerialEndpoint {
    pub port_name: String,
    pub baud_rate: u32,
    pub timeout_ms: u64,
}

impl OmicronConfiguredProbe {
    pub fn simulated() -> Self {
        let probe = protocol::OmicronProbe::simulated();
        let operating_mode = protocol::OperatingMode::Cw;
        let cw_submode = protocol::CwSubMode::Acc;
        Self {
            label: "Simulated Omicron serial laser".into(),
            probe,
            enabled: false,
            power_level: 0,
            actual_power: OpticalPower::from_milliwatts(0.0),
            operating_bits: cw_submode.apply_to_bits(operating_mode.apply_to_bits(0)),
            operating_mode,
            cw_submode,
            fault_bits: 0,
            hours: TimeInterval::from_hours(0.0),
            diode_temperature: Temperature::from_celsius(25.0),
            baseplate_temperature: Temperature::from_celsius(24.0),
            endpoint: None,
            connect_real_transport: false,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::simulated();
        configured.label = if device.label.is_empty() {
            "Configured Omicron serial laser".into()
        } else {
            device.label.clone()
        };
        configured.probe.family = u16_prop(device, "family_code")
            .map(protocol::DeviceFamily::from_code)
            .or_else(|| string_prop(device, "family").and_then(|value| family_from_label(&value)))
            .unwrap_or(configured.probe.family);
        configured.probe.serial_number = string_prop(device, "serial_number")
            .unwrap_or_else(|| configured.probe.serial_number.clone());
        configured.probe.firmware =
            string_prop(device, "firmware").unwrap_or_else(|| configured.probe.firmware.clone());
        configured.probe.wavelength = wavelength_prop(device, "wavelength")
            .or_else(|| f64_prop(device, "wavelength_nm").map(Wavelength::from_nanometers))
            .unwrap_or(configured.probe.wavelength);
        configured.probe.specified_power = optical_power_prop(device, "specified_power")
            .or_else(|| f64_prop(device, "specified_power_mw").map(OpticalPower::from_milliwatts))
            .unwrap_or(configured.probe.specified_power);
        configured.enabled = bool_prop(device, "enabled").unwrap_or(configured.enabled);
        configured.power_level = u16_prop(device, "power_level")
            .or_else(|| {
                ratio_prop(device, "relative_power")
                    .map(|percent| protocol::level_from_percent(percent.percent()))
            })
            .or_else(|| f64_prop(device, "power_percent").map(protocol::level_from_percent))
            .or_else(|| {
                optical_power_prop(device, "power").map(|power| {
                    protocol::level_from_power(power, configured.probe.specified_power)
                })
            })
            .or_else(|| {
                f64_prop(device, "power_mw").map(|mw| {
                    protocol::level_from_power(
                        OpticalPower::from_milliwatts(mw),
                        configured.probe.specified_power,
                    )
                })
            })
            .unwrap_or(configured.power_level);
        configured.actual_power = optical_power_prop(device, "actual_power")
            .or_else(|| f64_prop(device, "actual_power_mw").map(OpticalPower::from_milliwatts))
            .unwrap_or(configured.actual_power);
        configured.operating_mode = string_prop(device, "operating_mode")
            .map(|mode| parse_mode(&mode))
            .transpose()?
            .unwrap_or(configured.operating_mode);
        configured.cw_submode = string_prop(device, "cw_submode")
            .map(|mode| parse_submode(&mode))
            .transpose()?
            .unwrap_or(configured.cw_submode);
        configured.operating_bits = u16_prop(device, "operating_bits").unwrap_or_else(|| {
            configured
                .cw_submode
                .apply_to_bits(configured.operating_mode.apply_to_bits(0))
        });
        if let Some(enabled) = bool_prop(device, "analog_modulation_enabled") {
            configured.operating_bits = set_bit(configured.operating_bits, 7, enabled);
        }
        if let Some(enabled) = bool_prop(device, "digital_modulation_enabled") {
            configured.operating_bits = set_bit(configured.operating_bits, 5, enabled);
        }
        configured.operating_mode = protocol::operating_mode_from_bits(configured.operating_bits);
        configured.cw_submode = if (configured.operating_bits >> 8) & 1 == 1 {
            protocol::CwSubMode::Apc
        } else {
            protocol::CwSubMode::Acc
        };
        configured.fault_bits = u16_prop(device, "fault_bits").unwrap_or(configured.fault_bits);
        configured.hours = f64_prop(device, "hours")
            .map(TimeInterval::from_hours)
            .or_else(|| time_interval_prop(device, "hours_interval"))
            .unwrap_or(configured.hours);
        configured.diode_temperature = temperature_prop(device, "diode_temperature")
            .or_else(|| f64_prop(device, "diode_temperature_c").map(Temperature::from_celsius))
            .unwrap_or(configured.diode_temperature);
        configured.baseplate_temperature = temperature_prop(device, "baseplate_temperature")
            .or_else(|| f64_prop(device, "baseplate_temperature_c").map(Temperature::from_celsius))
            .unwrap_or(configured.baseplate_temperature);
        configured.endpoint =
            string_prop(device, "serial_port").map(|port_name| OmicronSerialEndpoint {
                port_name,
                baud_rate: u32_prop(device, "baud_rate").unwrap_or(500_000),
                timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(100),
            });
        configured.connect_real_transport = bool_prop(device, "connect").unwrap_or(false);
        Ok(configured)
    }
}

pub struct OmicronDriver {
    id: DriverId,
    resource: ResourceId,
    laser: DeviceId,
    probe: protocol::OmicronProbe,
    enabled: bool,
    power_level: u16,
    actual_power: OpticalPower,
    operating_bits: u16,
    operating_mode: protocol::OperatingMode,
    cw_submode: protocol::CwSubMode,
    fault_bits: u16,
    hours: f64,
    diode_temperature: Temperature,
    baseplate_temperature: Temperature,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
}

impl OmicronDriver {
    pub fn simulated(id: DriverId) -> Self {
        Self::configured_fixture(id, OmicronConfiguredProbe::simulated())
    }

    pub fn configured_fixture(id: DriverId, configured: OmicronConfiguredProbe) -> Self {
        let serial = ScriptedSerial::new();
        Self::new_configured(id, configured, Box::new(serial))
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: OmicronConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Omicron serial probe is missing serial_port metadata",
            )
        })?;
        let mut serial = numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(endpoint.port_name, endpoint.baud_rate)
                .timeout(Duration::from_millis(endpoint.timeout_ms)),
        )?;
        let probe_result = protocol::execute_probe_script(&mut serial, 4)?;
        Ok(Self::new_configured(id, configured, Box::new(serial)).with_probe_result(probe_result))
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _configured: OmicronConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Omicron real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(id: DriverId, probe: protocol::OmicronProbe, serial: Box<dyn SerialIo>) -> Self {
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 1101)),
            laser: DeviceId(NodeId(id.0 * 1000 + 1110)),
            probe,
            enabled: false,
            power_level: 0,
            actual_power: OpticalPower::from_milliwatts(0.0),
            operating_bits: protocol::OperatingMode::Cw.apply_to_bits(0),
            operating_mode: protocol::OperatingMode::Cw,
            cw_submode: protocol::CwSubMode::Acc,
            fault_bits: 0,
            hours: 0.0,
            diode_temperature: Temperature::from_celsius(25.0),
            baseplate_temperature: Temperature::from_celsius(24.0),
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            codec: SerialLineCodec::new(protocol::SEND_ENDING, protocol::RECV_ENDING),
            serial_port: None,
            baud_rate: 500_000,
            serial_timeout_ms: 100,
            connected: false,
        }
    }

    fn new_configured(
        id: DriverId,
        configured: OmicronConfiguredProbe,
        serial: Box<dyn SerialIo>,
    ) -> Self {
        let mut driver = Self::new(id, configured.probe, serial);
        driver.enabled = configured.enabled;
        driver.power_level = configured.power_level;
        driver.actual_power = configured.actual_power;
        driver.operating_bits = configured.operating_bits;
        driver.operating_mode = configured.operating_mode;
        driver.cw_submode = configured.cw_submode;
        driver.fault_bits = configured.fault_bits;
        driver.hours = configured.hours.hours();
        driver.diode_temperature = configured.diode_temperature;
        driver.baseplate_temperature = configured.baseplate_temperature;
        driver.serial_port = configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.port_name.clone());
        driver.baud_rate = configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.baud_rate)
            .unwrap_or(500_000);
        driver.serial_timeout_ms = configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.timeout_ms)
            .unwrap_or(100);
        driver.connected = configured.connect_real_transport;
        driver
    }

    #[cfg(feature = "os-serial")]
    fn with_probe_result(mut self, probe_result: protocol::OmicronProbeResult) -> Self {
        self.probe = probe_result.probe;
        self.enabled = probe_result.enabled;
        self.power_level = probe_result.power_level;
        self.actual_power = probe_result.actual_power;
        self.operating_bits = probe_result.operating_bits;
        self.operating_mode = probe_result.operating_mode;
        self.cw_submode = probe_result.cw_submode;
        self.fault_bits = probe_result.fault_bits;
        self.hours = probe_result.hours.hours();
        self.diode_temperature = probe_result.diode_temperature;
        self.baseplate_temperature = probe_result.baseplate_temperature;
        self
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn send(&mut self, command: protocol::OmicronCommand) -> Result<()> {
        let line = protocol::encode(&command);
        self.serial.write(&self.codec.encode(&line))
    }

    fn queries_for_property(
        device: DeviceId,
        laser: DeviceId,
        key: &str,
    ) -> Vec<protocol::OmicronCommand> {
        if device != laser {
            return Vec::new();
        }
        match omicron_public_key(key) {
            "enabled" => vec![protocol::OmicronCommand::QueryLaserState],
            "power" | "relative_power" => vec![protocol::OmicronCommand::QueryPowerLevel],
            "actual_power" => vec![protocol::OmicronCommand::QueryActualPower],
            "wavelength" => vec![protocol::OmicronCommand::QuerySpecInfo],
            "operating_mode"
            | "cw_submode"
            | "operating_bits"
            | "operating_flags"
            | "analog_modulation_enabled"
            | "digital_modulation_enabled"
            | "apc_enabled" => vec![protocol::OmicronCommand::QueryOperatingMode],
            "fault" | "fault_bits" | "fault_flags" | "interlock_closed" => {
                vec![protocol::OmicronCommand::QueryFaultBits]
            }
            "serial_number" => vec![protocol::OmicronCommand::QuerySerialNumber],
            "hours" => vec![protocol::OmicronCommand::QueryHours],
            "diode_temperature" => vec![protocol::OmicronCommand::QueryDiodeTemperature],
            "baseplate_temperature" => vec![protocol::OmicronCommand::QueryBaseplateTemperature],
            "telemetry_summary" => vec![
                protocol::OmicronCommand::QueryLaserState,
                protocol::OmicronCommand::QueryPowerLevel,
                protocol::OmicronCommand::QueryActualPower,
                protocol::OmicronCommand::QuerySpecInfo,
                protocol::OmicronCommand::QueryOperatingMode,
                protocol::OmicronCommand::QueryFaultBits,
                protocol::OmicronCommand::QuerySerialNumber,
                protocol::OmicronCommand::QueryHours,
                protocol::OmicronCommand::QueryDiodeTemperature,
                protocol::OmicronCommand::QueryBaseplateTemperature,
            ],
            _ => Vec::new(),
        }
    }

    fn issue_read_commands(
        &mut self,
        device: DeviceId,
        key: &str,
    ) -> Result<Vec<protocol::OmicronCommand>> {
        let commands = Self::queries_for_property(device, self.laser, omicron_public_key(key));
        for command in &commands {
            self.send(command.clone())?;
        }
        Ok(commands)
    }

    fn read_query_replies(&mut self, commands: &[protocol::OmicronCommand]) -> Result<()> {
        for command in commands {
            let bytes = self.serial.read_available()?;
            if bytes.is_empty() {
                continue;
            }
            let lines = self.codec.push(&bytes);
            for line in lines {
                self.apply_readback_reply(command, &line)?;
            }
        }
        Ok(())
    }

    fn confirm_write_readback(&mut self, commands: &[protocol::OmicronCommand]) -> Result<()> {
        let fault_before = self.fault_bits;
        for command in commands {
            self.send(command.clone())?;
        }
        self.read_query_replies(commands)?;
        if self.fault_bits != fault_before && self.fault_bits != 0 {
            return Err(Error::new(
                ErrorCode::Driver,
                format!(
                    "Omicron laser reported fault {}",
                    protocol::fault_text(self.fault_bits)
                ),
            ));
        }
        Ok(())
    }

    fn refresh_commands_for(command: &str) -> Result<Vec<protocol::OmicronCommand>> {
        match command {
            "refresh_telemetry" => Ok(vec![
                protocol::OmicronCommand::QueryLaserState,
                protocol::OmicronCommand::QueryPowerLevel,
                protocol::OmicronCommand::QueryActualPower,
                protocol::OmicronCommand::QuerySpecInfo,
                protocol::OmicronCommand::QueryOperatingMode,
                protocol::OmicronCommand::QueryFaultBits,
                protocol::OmicronCommand::QuerySerialNumber,
                protocol::OmicronCommand::QueryHours,
                protocol::OmicronCommand::QueryDiodeTemperature,
                protocol::OmicronCommand::QueryBaseplateTemperature,
            ]),
            "refresh_identity" => Ok(vec![
                protocol::OmicronCommand::Firmware,
                protocol::OmicronCommand::QuerySerialNumber,
                protocol::OmicronCommand::QuerySpecInfo,
                protocol::OmicronCommand::QueryHours,
            ]),
            "refresh_power" => Ok(vec![
                protocol::OmicronCommand::QueryPowerLevel,
                protocol::OmicronCommand::QueryActualPower,
            ]),
            "refresh_status" => Ok(vec![
                protocol::OmicronCommand::QueryLaserState,
                protocol::OmicronCommand::QueryOperatingMode,
                protocol::OmicronCommand::QueryFaultBits,
            ]),
            "refresh_temperatures" => Ok(vec![
                protocol::OmicronCommand::QueryDiodeTemperature,
                protocol::OmicronCommand::QueryBaseplateTemperature,
            ]),
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "Omicron GenericCommand supports refresh_telemetry, refresh_identity, refresh_power, refresh_status, and refresh_temperatures; got {other}"
                ),
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
                "Omicron GenericCommand does not take parameters",
            ));
        }
        let _ = Self::refresh_commands_for(&request.command)?;
        Ok(())
    }

    fn apply_generic_command(&mut self, request: GenericCommandRequest) -> Result<Value> {
        self.validate_generic_command(&request)?;
        let commands = Self::refresh_commands_for(&request.command)?;
        for command in &commands {
            self.send(command.clone())?;
        }
        self.read_query_replies(&commands)?;
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("commands".into(), Value::I64(commands.len() as i64)),
            ("telemetry".into(), self.telemetry_summary()),
            (
                "completion_basis".into(),
                Value::String("Omicron query readback".into()),
            ),
        ])))
    }

    fn apply_readback_reply(
        &mut self,
        command: &protocol::OmicronCommand,
        reply: &str,
    ) -> Result<()> {
        match command {
            protocol::OmicronCommand::QueryLaserState => {
                self.enabled = protocol::parse_laser_state(reply)?;
                self.emit_property(self.laser, "enabled", Value::Bool(self.enabled));
            }
            protocol::OmicronCommand::Firmware => {
                self.probe.firmware = reply
                    .trim()
                    .strip_prefix("!GFw")
                    .unwrap_or(reply.trim())
                    .trim()
                    .to_string();
            }
            protocol::OmicronCommand::QueryPowerLevel => {
                self.power_level = protocol::parse_prefixed_hex(reply, "!GLP")?;
                self.emit_property(
                    self.laser,
                    "power",
                    Value::OpticalPower(protocol::power_from_level(
                        self.power_level,
                        self.probe.specified_power,
                    )),
                );
                self.emit_property(
                    self.laser,
                    "relative_power",
                    Value::Ratio(Ratio::from_percent(protocol::percent_from_level(
                        self.power_level,
                    ))),
                );
            }
            protocol::OmicronCommand::QueryActualPower => {
                self.actual_power =
                    OpticalPower::from_milliwatts(protocol::parse_number_reply("!MDP", reply)?);
                self.emit_property(
                    self.laser,
                    "actual_power",
                    Value::OpticalPower(self.actual_power),
                );
            }
            protocol::OmicronCommand::QuerySpecInfo => {
                let (wavelength, specified_power) = protocol::parse_spec_info(reply)?;
                self.probe.wavelength = wavelength;
                self.probe.specified_power = specified_power;
                if let Some(family) = protocol::parse_family_from_spec_info(reply) {
                    self.probe.family = family;
                }
                self.emit_property(self.laser, "wavelength", Value::Wavelength(wavelength));
            }
            protocol::OmicronCommand::QueryOperatingMode => {
                self.operating_bits = protocol::parse_prefixed_hex(reply, "!GOM")?;
                self.operating_mode = protocol::operating_mode_from_bits(self.operating_bits);
                self.cw_submode = if (self.operating_bits >> 8) & 1 == 1 {
                    protocol::CwSubMode::Apc
                } else {
                    protocol::CwSubMode::Acc
                };
                self.emit_property(
                    self.laser,
                    "operating_mode",
                    Value::String(self.operating_mode.label().into()),
                );
                self.emit_property(
                    self.laser,
                    "cw_submode",
                    Value::String(self.cw_submode.label().into()),
                );
                self.emit_property(
                    self.laser,
                    "operating_bits",
                    Value::I64(self.operating_bits as i64),
                );
                self.emit_property(
                    self.laser,
                    "operating_flags",
                    Value::String(protocol::operating_bit_labels(self.operating_bits)),
                );
                self.emit_property(
                    self.laser,
                    "analog_modulation_enabled",
                    Value::Bool((self.operating_bits >> 7) & 1 == 1),
                );
                self.emit_property(
                    self.laser,
                    "digital_modulation_enabled",
                    Value::Bool((self.operating_bits >> 5) & 1 == 1),
                );
                self.emit_property(
                    self.laser,
                    "apc_enabled",
                    Value::Bool((self.operating_bits >> 8) & 1 == 1),
                );
            }
            protocol::OmicronCommand::QueryFaultBits => {
                self.fault_bits = protocol::parse_prefixed_hex(reply, "!GFB")?;
                self.emit_property(
                    self.laser,
                    "fault",
                    Value::String(protocol::fault_text(self.fault_bits)),
                );
                self.emit_property(self.laser, "fault_bits", Value::I64(self.fault_bits as i64));
                self.emit_property(
                    self.laser,
                    "fault_flags",
                    Value::String(protocol::fault_bit_labels(self.fault_bits)),
                );
                self.emit_property(
                    self.laser,
                    "interlock_closed",
                    Value::Bool((self.fault_bits >> 9) & 1 == 0),
                );
            }
            protocol::OmicronCommand::QuerySerialNumber => {
                self.probe.serial_number = reply
                    .trim()
                    .strip_prefix("!GSN")
                    .unwrap_or(reply.trim())
                    .trim()
                    .to_string();
                self.emit_property(
                    self.laser,
                    "serial_number",
                    Value::String(self.probe.serial_number.clone()),
                );
            }
            protocol::OmicronCommand::QueryHours => {
                self.hours = protocol::parse_number_reply("!GWH", reply)?;
                self.emit_property(
                    self.laser,
                    "hours",
                    Value::TimeInterval(TimeInterval::from_hours(self.hours)),
                );
            }
            protocol::OmicronCommand::QueryDiodeTemperature => {
                self.diode_temperature =
                    Temperature::from_celsius(protocol::parse_number_reply("!MTD", reply)?);
                self.emit_property(
                    self.laser,
                    "diode_temperature",
                    Value::Temperature(self.diode_temperature),
                );
            }
            protocol::OmicronCommand::QueryBaseplateTemperature => {
                self.baseplate_temperature =
                    Temperature::from_celsius(protocol::parse_number_reply("!MTA", reply)?);
                self.emit_property(
                    self.laser,
                    "baseplate_temperature",
                    Value::Temperature(self.baseplate_temperature),
                );
            }
            _ => {}
        }
        Ok(())
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        vec![DeviceDescriptor {
            id: self.laser,
            driver: self.id,
            label: "omicron-serial-laser".into(),
            vendor: Some("Omicron Laserage".into()),
            model: Some(self.probe.family.label()),
            serial: Some(self.probe.serial_number.clone()),
            kinds: vec![
                "laser".into(),
                "light.source".into(),
                "shutter".into(),
                "trigger.sink".into(),
                "serial.ascii".into(),
            ],
            properties: vec![
                sequenceable_property("enabled", "Enabled", ValueType::Bool, None, true, None),
                sequenceable_property(
                    "power",
                    "Power setpoint",
                    ValueType::OpticalPower,
                    None,
                    true,
                    Some(Range {
                        min: Value::OpticalPower(OpticalPower::from_milliwatts(0.0)),
                        max: Value::OpticalPower(self.probe.specified_power),
                    }),
                ),
                sequenceable_property(
                    "relative_power",
                    "Relative power",
                    ValueType::Ratio,
                    Some("percent"),
                    true,
                    Some(Range {
                        min: Value::Ratio(Ratio::from_percent(0.0)),
                        max: Value::Ratio(Ratio::from_percent(100.0)),
                    }),
                ),
                property(
                    "actual_power",
                    "Actual power",
                    ValueType::OpticalPower,
                    None,
                    false,
                    None,
                ),
                property(
                    "wavelength",
                    "Wavelength",
                    ValueType::Wavelength,
                    None,
                    false,
                    None,
                ),
                mode_property(),
                submode_property(),
                property(
                    "operating_bits",
                    "Operating mode bits",
                    ValueType::I64,
                    None,
                    false,
                    None,
                ),
                property(
                    "operating_flags",
                    "Operating flags",
                    ValueType::String,
                    None,
                    false,
                    None,
                ),
                sequenceable_property(
                    "analog_modulation_enabled",
                    "Analog modulation enabled",
                    ValueType::Bool,
                    None,
                    true,
                    None,
                ),
                sequenceable_property(
                    "digital_modulation_enabled",
                    "Digital modulation enabled",
                    ValueType::Bool,
                    None,
                    true,
                    None,
                ),
                property(
                    "apc_enabled",
                    "Auto power control enabled",
                    ValueType::Bool,
                    None,
                    false,
                    None,
                ),
                property("fault", "Fault", ValueType::String, None, false, None),
                property(
                    "fault_bits",
                    "Fault bits",
                    ValueType::I64,
                    None,
                    false,
                    None,
                ),
                property(
                    "fault_flags",
                    "Fault flags",
                    ValueType::String,
                    None,
                    false,
                    None,
                ),
                property(
                    "interlock_closed",
                    "Interlock closed",
                    ValueType::Bool,
                    None,
                    false,
                    None,
                ),
                property(
                    "serial_number",
                    "Serial number",
                    ValueType::String,
                    None,
                    false,
                    None,
                ),
                property(
                    "hours",
                    "Working hours",
                    ValueType::TimeInterval,
                    Some("h"),
                    false,
                    None,
                ),
                property(
                    "diode_temperature",
                    "Diode temperature",
                    ValueType::Temperature,
                    None,
                    false,
                    None,
                ),
                property(
                    "baseplate_temperature",
                    "Baseplate temperature",
                    ValueType::Temperature,
                    None,
                    false,
                    None,
                ),
                property(
                    "telemetry_summary",
                    "Telemetry summary",
                    ValueType::Map,
                    None,
                    false,
                    None,
                ),
            ],
            metadata: BTreeMap::from([
                (
                    "firmware".into(),
                    Value::String(self.probe.firmware.clone()),
                ),
                (
                    "specified_power".into(),
                    Value::OpticalPower(self.probe.specified_power),
                ),
                ("power_dac_max".into(), Value::I64(protocol::DAC_MAX as i64)),
            ]),
        }]
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        let key = omicron_public_key(key);
        if device != self.laser {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown Omicron device",
            ));
        }
        match key {
            "enabled" => Ok(Value::Bool(self.enabled)),
            "power" => Ok(Value::OpticalPower(protocol::power_from_level(
                self.power_level,
                self.probe.specified_power,
            ))),
            "relative_power" => Ok(Value::Ratio(Ratio::from_percent(
                protocol::percent_from_level(self.power_level),
            ))),
            "actual_power" => Ok(Value::OpticalPower(self.actual_power)),
            "wavelength" => Ok(Value::Wavelength(self.probe.wavelength)),
            "operating_mode" => Ok(Value::String(self.operating_mode.label().into())),
            "cw_submode" => Ok(Value::String(self.cw_submode.label().into())),
            "operating_bits" => Ok(Value::I64(self.operating_bits as i64)),
            "operating_flags" => Ok(Value::String(protocol::operating_bit_labels(
                self.operating_bits,
            ))),
            "analog_modulation_enabled" => Ok(Value::Bool((self.operating_bits >> 7) & 1 == 1)),
            "digital_modulation_enabled" => Ok(Value::Bool((self.operating_bits >> 5) & 1 == 1)),
            "apc_enabled" => Ok(Value::Bool((self.operating_bits >> 8) & 1 == 1)),
            "fault" => Ok(Value::String(protocol::fault_text(self.fault_bits))),
            "fault_bits" => Ok(Value::I64(self.fault_bits as i64)),
            "fault_flags" => Ok(Value::String(protocol::fault_bit_labels(self.fault_bits))),
            "interlock_closed" => Ok(Value::Bool((self.fault_bits >> 9) & 1 == 0)),
            "serial_number" => Ok(Value::String(self.probe.serial_number.clone())),
            "hours" => Ok(Value::TimeInterval(TimeInterval::from_hours(self.hours))),
            "diode_temperature" => Ok(Value::Temperature(self.diode_temperature)),
            "baseplate_temperature" => Ok(Value::Temperature(self.baseplate_temperature)),
            "telemetry_summary" => Ok(self.telemetry_summary()),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Omicron property {key}"),
            )),
        }
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        let key = omicron_public_key(key);
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
        let key = omicron_public_key(key);
        self.validate_write(device, key, value)?;
        if device != self.laser {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown Omicron device",
            ));
        }
        match (key, value) {
            ("enabled", Value::Bool(enabled)) => {
                if *enabled && self.fault_bits != 0 {
                    return Err(Error::new(
                        ErrorCode::Driver,
                        format!(
                            "refusing to enable Omicron laser with fault {}",
                            protocol::fault_text(self.fault_bits)
                        ),
                    ));
                }
                self.send(if *enabled {
                    protocol::OmicronCommand::LaserOn
                } else {
                    protocol::OmicronCommand::LaserOff
                })?;
                self.enabled = *enabled;
                self.actual_power = if *enabled {
                    protocol::power_from_level(self.power_level, self.probe.specified_power)
                } else {
                    OpticalPower::from_milliwatts(0.0)
                };
                self.confirm_write_readback(&[
                    protocol::OmicronCommand::QueryLaserState,
                    protocol::OmicronCommand::QueryFaultBits,
                ])?;
                self.finish_state();
                Ok(Value::Bool(self.enabled))
            }
            ("power", Value::OpticalPower(power)) => {
                let level = protocol::level_from_power(*power, self.probe.specified_power);
                self.send(protocol::OmicronCommand::SetPowerLevel(level))?;
                self.power_level = level;
                if self.enabled {
                    self.actual_power =
                        protocol::power_from_level(level, self.probe.specified_power);
                }
                self.confirm_write_readback(&[
                    protocol::OmicronCommand::QueryPowerLevel,
                    protocol::OmicronCommand::QueryFaultBits,
                ])?;
                Ok(Value::OpticalPower(protocol::power_from_level(
                    self.power_level,
                    self.probe.specified_power,
                )))
            }
            ("relative_power", Value::Ratio(percent)) => {
                let level = protocol::level_from_percent(percent.percent());
                self.send(protocol::OmicronCommand::SetPowerLevel(level))?;
                self.power_level = level;
                if self.enabled {
                    self.actual_power =
                        protocol::power_from_level(level, self.probe.specified_power);
                }
                self.confirm_write_readback(&[
                    protocol::OmicronCommand::QueryPowerLevel,
                    protocol::OmicronCommand::QueryFaultBits,
                ])?;
                Ok(Value::Ratio(Ratio::from_percent(
                    protocol::percent_from_level(self.power_level),
                )))
            }
            ("operating_mode", Value::String(mode)) => {
                let mode = parse_mode(mode)?;
                self.operating_bits = mode.apply_to_bits(self.operating_bits);
                self.send(protocol::OmicronCommand::SetOperatingModeBits(
                    self.operating_bits,
                ))?;
                self.operating_mode = mode;
                self.confirm_write_readback(&[
                    protocol::OmicronCommand::QueryOperatingMode,
                    protocol::OmicronCommand::QueryFaultBits,
                ])?;
                Ok(Value::String(self.operating_mode.label().into()))
            }
            ("cw_submode", Value::String(submode)) => {
                let submode = parse_submode(submode)?;
                self.operating_bits = submode.apply_to_bits(self.operating_bits);
                self.send(protocol::OmicronCommand::SetOperatingModeBits(
                    self.operating_bits,
                ))?;
                self.cw_submode = submode;
                self.confirm_write_readback(&[
                    protocol::OmicronCommand::QueryOperatingMode,
                    protocol::OmicronCommand::QueryFaultBits,
                ])?;
                Ok(Value::String(self.cw_submode.label().into()))
            }
            ("analog_modulation_enabled", Value::Bool(enabled)) => {
                self.operating_bits = set_bit(self.operating_bits, 7, *enabled);
                self.send(protocol::OmicronCommand::SetOperatingModeBits(
                    self.operating_bits,
                ))?;
                self.operating_mode = protocol::operating_mode_from_bits(self.operating_bits);
                self.confirm_write_readback(&[
                    protocol::OmicronCommand::QueryOperatingMode,
                    protocol::OmicronCommand::QueryFaultBits,
                ])?;
                Ok(Value::Bool((self.operating_bits >> 7) & 1 == 1))
            }
            ("digital_modulation_enabled", Value::Bool(enabled)) => {
                self.operating_bits = set_bit(self.operating_bits, 5, *enabled);
                self.send(protocol::OmicronCommand::SetOperatingModeBits(
                    self.operating_bits,
                ))?;
                self.operating_mode = protocol::operating_mode_from_bits(self.operating_bits);
                self.confirm_write_readback(&[
                    protocol::OmicronCommand::QueryOperatingMode,
                    protocol::OmicronCommand::QueryFaultBits,
                ])?;
                Ok(Value::Bool((self.operating_bits >> 5) & 1 == 1))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid Omicron write {key}"),
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
        Ok(Value::Map(changed))
    }

    fn finish_state(&mut self) {
        self.pending
            .push_back(DriverEvent::Event(Event::Log(LogEvent {
                driver: Some(self.id),
                message: format!("omicron laser {}", if self.enabled { "On" } else { "Off" }),
            })));
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
            .filter(|route| route.from == self.laser || route.to == self.laser)
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
            .filter(|sequence| sequence.device == self.laser)
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
            .filter(|sequence| sequence.device == self.laser)
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequence_refs(plan) {
            let descriptor = self.descriptors_for().into_iter().next().ok_or_else(|| {
                Error::new(ErrorCode::InvalidCommand, "missing Omicron descriptor")
            })?;
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
        let has_enabled_sequence = sequences
            .iter()
            .any(|sequence| sequence.property.as_str() == "enabled");
        let mut changed = BTreeMap::new();

        if !has_enabled_sequence {
            let value = self.write_property(self.laser, "enabled", &Value::Bool(start))?;
            self.emit_property(self.laser, "enabled", value.clone());
            changed.insert(format!("{}:enabled", (self.laser.0).0), value);
        }

        let writes = sequences
            .into_iter()
            .filter_map(|sequence| {
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

        for write in writes {
            let value = self.write_property(write.device, &write.property, &write.value)?;
            self.emit_property(write.device, &write.property, value.clone());
            if write.property == "power" {
                self.emit_property(
                    write.device,
                    "power",
                    Value::OpticalPower(protocol::power_from_level(
                        self.power_level,
                        self.probe.specified_power,
                    )),
                );
                self.emit_property(
                    write.device,
                    "relative_power",
                    Value::Ratio(Ratio::from_percent(protocol::percent_from_level(
                        self.power_level,
                    ))),
                );
            }
            changed.insert(format!("{}:{}", (write.device.0).0, write.property), value);
        }

        Ok(Value::Map(changed))
    }

    fn timing_summary(&self, plan: &TimingPlan, action: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("action".into(), Value::String(action.into())),
            ("device".into(), Value::I64(self.laser.0 .0 as i64)),
            ("enabled".into(), Value::Bool(self.enabled)),
            (
                "operating_mode".into(),
                Value::String(self.operating_mode.label().into()),
            ),
            (
                "cw_submode".into(),
                Value::String(self.cw_submode.label().into()),
            ),
            (
                "operating_bits".into(),
                Value::I64(self.operating_bits as i64),
            ),
            (
                "power".into(),
                Value::OpticalPower(protocol::power_from_level(
                    self.power_level,
                    self.probe.specified_power,
                )),
            ),
            (
                "actual_power".into(),
                Value::OpticalPower(self.actual_power),
            ),
            ("fault_bits".into(), Value::I64(self.fault_bits as i64)),
            (
                "interlock_closed".into(),
                Value::Bool((self.fault_bits >> 9) & 1 == 0),
            ),
            ("routes".into(), Value::List(self.local_timing_routes(plan))),
            (
                "sequences".into(),
                Value::List(self.local_timing_sequences(plan)),
            ),
        ]))
    }

    fn telemetry_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("device".into(), Value::I64(self.laser.0 .0 as i64)),
            (
                "family".into(),
                Value::String(self.probe.family.label().into()),
            ),
            (
                "serial_number".into(),
                Value::String(self.probe.serial_number.clone()),
            ),
            (
                "firmware".into(),
                Value::String(self.probe.firmware.clone()),
            ),
            ("enabled".into(), Value::Bool(self.enabled)),
            (
                "power".into(),
                Value::OpticalPower(protocol::power_from_level(
                    self.power_level,
                    self.probe.specified_power,
                )),
            ),
            (
                "relative_power".into(),
                Value::Ratio(Ratio::from_percent(protocol::percent_from_level(
                    self.power_level,
                ))),
            ),
            ("power_level".into(), Value::I64(self.power_level as i64)),
            (
                "actual_power".into(),
                Value::OpticalPower(self.actual_power),
            ),
            (
                "specified_power".into(),
                Value::OpticalPower(self.probe.specified_power),
            ),
            (
                "wavelength".into(),
                Value::Wavelength(self.probe.wavelength),
            ),
            (
                "operating_mode".into(),
                Value::String(self.operating_mode.label().into()),
            ),
            (
                "cw_submode".into(),
                Value::String(self.cw_submode.label().into()),
            ),
            (
                "operating_bits".into(),
                Value::I64(self.operating_bits as i64),
            ),
            (
                "operating_flags".into(),
                Value::String(protocol::operating_bit_labels(self.operating_bits)),
            ),
            (
                "analog_modulation_enabled".into(),
                Value::Bool((self.operating_bits >> 7) & 1 == 1),
            ),
            (
                "digital_modulation_enabled".into(),
                Value::Bool((self.operating_bits >> 5) & 1 == 1),
            ),
            (
                "apc_enabled".into(),
                Value::Bool((self.operating_bits >> 8) & 1 == 1),
            ),
            (
                "fault".into(),
                Value::String(protocol::fault_text(self.fault_bits)),
            ),
            ("fault_bits".into(), Value::I64(self.fault_bits as i64)),
            (
                "fault_flags".into(),
                Value::String(protocol::fault_bit_labels(self.fault_bits)),
            ),
            (
                "interlock_closed".into(),
                Value::Bool((self.fault_bits >> 9) & 1 == 0),
            ),
            (
                "hours".into(),
                Value::TimeInterval(TimeInterval::from_hours(self.hours)),
            ),
            (
                "diode_temperature".into(),
                Value::Temperature(self.diode_temperature),
            ),
            (
                "baseplate_temperature".into(),
                Value::Temperature(self.baseplate_temperature),
            ),
        ]))
    }

    fn timing_transaction(
        &self,
        description: &str,
        command: protocol::OmicronCommand,
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
    ) -> Result<Vec<protocol::OmicronCommand>> {
        if device != self.laser {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown Omicron device",
            ));
        }
        match kind {
            CapabilityKind::Dac => Ok(vec![protocol::OmicronCommand::SetPowerLevel(
                dac_request_level(request, self.probe.specified_power)?,
            )]),
            CapabilityKind::TriggerSink => trigger_sink_commands(request),
            CapabilityKind::GenericCommand => {
                let CapabilityRequest::GenericCommand(request) = request else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "Omicron GenericCommand expects a GenericCommandRequest",
                    ));
                };
                self.validate_generic_command(request)?;
                Self::refresh_commands_for(&request.command)
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Omicron invocation capability",
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
                let level = dac_request_level(&request, self.probe.specified_power)?;
                let power = protocol::power_from_level(level, self.probe.specified_power);
                let value = self.write_property(device, "power", &Value::OpticalPower(power))?;
                self.emit_property(device, "power", value.clone());
                self.emit_property(
                    device,
                    "relative_power",
                    Value::Ratio(Ratio::from_percent(protocol::percent_from_level(
                        self.power_level,
                    ))),
                );
                Ok(Value::Map(BTreeMap::from([
                    ("power".into(), value),
                    ("level".into(), Value::I64(self.power_level as i64)),
                    (
                        "relative_power".into(),
                        Value::Ratio(Ratio::from_percent(protocol::percent_from_level(
                            self.power_level,
                        ))),
                    ),
                    ("commands".into(), Value::I64(1)),
                ])))
            }
            CapabilityKind::TriggerSink => {
                let commands = trigger_sink_commands(&request)?;
                for command in &commands {
                    match command {
                        protocol::OmicronCommand::LaserOn => {
                            let value =
                                self.write_property(device, "enabled", &Value::Bool(true))?;
                            self.emit_property(device, "enabled", value);
                        }
                        protocol::OmicronCommand::LaserOff => {
                            let value =
                                self.write_property(device, "enabled", &Value::Bool(false))?;
                            self.emit_property(device, "enabled", value);
                        }
                        _ => self.send(command.clone())?,
                    }
                }
                Ok(Value::Map(BTreeMap::from([
                    ("triggered".into(), Value::Bool(true)),
                    ("enabled".into(), Value::Bool(self.enabled)),
                    ("commands".into(), Value::I64(commands.len() as i64)),
                ])))
            }
            CapabilityKind::GenericCommand => {
                let CapabilityRequest::GenericCommand(request) = request else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "Omicron GenericCommand expects a GenericCommandRequest",
                    ));
                };
                self.apply_generic_command(request)
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Omicron invocation capability",
            )),
        }
    }
}

impl Driver for OmicronDriver {
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
            label: "omicron-serial".into(),
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
                ("recv_terminator".into(), Value::String("CR".into())),
                (
                    "completion".into(),
                    Value::String("command response line and status readback".into()),
                ),
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
        if device == self.laser {
            vec![
                capability(1, device, CapabilityKind::Dac),
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
                        description: format!("omicron read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("omicron write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "omicron laser state set".into(),
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
                            "unknown Omicron capability",
                        ));
                    };
                    if !capability.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "Omicron {:?} expects {:?}, got {:?}",
                                capability.kind,
                                capability.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    for command in self.invoke_transactions(*device, capability.kind, request)? {
                        physical_transactions.push(
                            self.timing_transaction(
                                "omicron direct capability invocation",
                                command,
                            ),
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

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.token();
        let mut last = Value::Null;
        for command in prepared.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    let commands = self.issue_read_commands(device, &key)?;
                    self.read_query_replies(&commands)?;
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
                            "unknown Omicron capability",
                        ));
                    };
                    if !capability.accepts_request(&request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "Omicron {:?} expects {:?}, got {:?}",
                                capability.kind,
                                capability.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    last = self.apply_invoke(device, capability.kind, request)?;
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
                        message: format!("omicron serial: {line}"),
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
                description: "omicron timing arm summary".into(),
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
            physical_transactions: vec![
                self.timing_transaction(
                    "omicron timing start emission enable",
                    protocol::OmicronCommand::LaserOn,
                ),
                PhysicalTransaction {
                    resource: Some(self.resource),
                    description: "omicron timing start summary".into(),
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
        let applied = self.apply_timing_sequence_step(&armed.plan, false)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions: vec![
                self.timing_transaction(
                    "omicron timing stop emission disable",
                    protocol::OmicronCommand::LaserOff,
                ),
                PhysicalTransaction {
                    resource: Some(self.resource),
                    description: "omicron timing stop summary".into(),
                    payload: with_applied(self.timing_summary(&armed.plan, "stop"), applied),
                },
            ],
        })
    }
}

fn parse_mode(mode: &str) -> Result<protocol::OperatingMode> {
    match mode.trim() {
        "Standby" => Ok(protocol::OperatingMode::Standby),
        "CW" => Ok(protocol::OperatingMode::Cw),
        "Analog Modulation" => Ok(protocol::OperatingMode::AnalogModulation),
        "Digital Modulation" => Ok(protocol::OperatingMode::DigitalModulation),
        "Analog + Digital Modulation" => Ok(protocol::OperatingMode::AnalogDigitalModulation),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unknown Omicron operating mode {other}"),
        )),
    }
}

fn parse_submode(mode: &str) -> Result<protocol::CwSubMode> {
    match mode.trim() {
        "ACC" | "ACC (auto current control)" => Ok(protocol::CwSubMode::Acc),
        "APC" | "APC (auto power control)" => Ok(protocol::CwSubMode::Apc),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unknown Omicron CW submode {other}"),
        )),
    }
}

fn dac_request_level(request: &CapabilityRequest, specified_power: OpticalPower) -> Result<u16> {
    match request {
        CapabilityRequest::Dac(request) => dac_value_level(&request.value, specified_power),
        _ => Err(Error::new(
            ErrorCode::Unsupported,
            "Omicron Dac expects CapabilityRequest::Dac",
        )),
    }
}

fn dac_value_level(value: &Value, specified_power: OpticalPower) -> Result<u16> {
    match value {
        Value::OpticalPower(power) => Ok(protocol::level_from_power(*power, specified_power)),
        Value::Ratio(percent) => Ok(protocol::level_from_percent(percent.percent())),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            "Omicron Dac value must be OpticalPower or Ratio percent",
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TriggerSinkAction {
    Enable,
    Disable,
    Pulse,
}

fn trigger_sink_commands(request: &CapabilityRequest) -> Result<Vec<protocol::OmicronCommand>> {
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
                "Omicron TriggerSink expects None or CapabilityRequest::Trigger",
            ))
        }
    };
    Ok(match action {
        TriggerSinkAction::Enable => vec![protocol::OmicronCommand::LaserOn],
        TriggerSinkAction::Disable => vec![protocol::OmicronCommand::LaserOff],
        TriggerSinkAction::Pulse => vec![
            protocol::OmicronCommand::LaserOn,
            protocol::OmicronCommand::LaserOff,
        ],
    })
}

fn capability(id: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    CapabilityDescriptor::new(CapabilityId(id), device, kind, ValueType::Map)
}

fn omicron_public_key(key: &str) -> &str {
    match key {
        "power_percent" => "relative_power",
        other => other,
    }
}

fn set_bit(bits: u16, bit: u8, enabled: bool) -> u16 {
    if enabled {
        bits | (1 << bit)
    } else {
        bits & !(1 << bit)
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

fn with_applied(summary: Value, applied: Value) -> Value {
    match summary {
        Value::Map(mut map) => {
            map.insert("applied".into(), applied);
            Value::Map(map)
        }
        other => other,
    }
}

fn mode_property() -> PropertySchema {
    let mut schema = property(
        "operating_mode",
        "Operating mode",
        ValueType::String,
        None,
        true,
        None,
    );
    schema.enum_values = [
        protocol::OperatingMode::Standby,
        protocol::OperatingMode::Cw,
        protocol::OperatingMode::AnalogModulation,
        protocol::OperatingMode::DigitalModulation,
        protocol::OperatingMode::AnalogDigitalModulation,
    ]
    .into_iter()
    .map(|mode| EnumValue {
        value: Value::String(mode.label().into()),
        label: mode.label().into(),
    })
    .collect();
    schema
}

fn submode_property() -> PropertySchema {
    let mut schema = property(
        "cw_submode",
        "CW sub operating mode",
        ValueType::String,
        None,
        true,
        None,
    );
    schema.enum_values = [protocol::CwSubMode::Acc, protocol::CwSubMode::Apc]
        .into_iter()
        .map(|mode| EnumValue {
            value: Value::String(mode.label().into()),
            label: mode.label().into(),
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

fn f64_prop(device: &DeviceConfig, key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
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

fn wavelength_prop(device: &DeviceConfig, key: &str) -> Option<Wavelength> {
    match device.properties.get(key) {
        Some(Value::Wavelength(value)) => Some(*value),
        _ => None,
    }
}

fn optical_power_prop(device: &DeviceConfig, key: &str) -> Option<OpticalPower> {
    match device.properties.get(key) {
        Some(Value::OpticalPower(value)) => Some(*value),
        _ => None,
    }
}

fn temperature_prop(device: &DeviceConfig, key: &str) -> Option<Temperature> {
    match device.properties.get(key) {
        Some(Value::Temperature(value)) => Some(*value),
        _ => None,
    }
}

fn ratio_prop(device: &DeviceConfig, key: &str) -> Option<Ratio> {
    match device.properties.get(key) {
        Some(Value::Ratio(value)) => Some(*value),
        _ => None,
    }
}

fn time_interval_prop(device: &DeviceConfig, key: &str) -> Option<TimeInterval> {
    match device.properties.get(key) {
        Some(Value::TimeInterval(value)) => Some(*value),
        _ => None,
    }
}

fn family_from_label(label: &str) -> Option<protocol::DeviceFamily> {
    match label {
        "PhoxX" | "phoxx" => Some(protocol::DeviceFamily::PhoxX),
        "LuxX" | "luxx" => Some(protocol::DeviceFamily::LuxX),
        "BrixX" | "brixx" => Some(protocol::DeviceFamily::BrixX),
        _ => None,
    }
}
