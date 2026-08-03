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

    pub const SEND_ENDING: LineEnding = LineEnding::CrLf;
    pub const RECV_ENDING: LineEnding = LineEnding::CrLf;
    pub const STATUS_RECV_ENDING: LineEnding = LineEnding::Lf;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DeviceFamily {
        Dc2xxx,
        Dc2200Scpi,
        Dc3100,
        Dc4100,
    }

    impl DeviceFamily {
        pub fn model_family(self) -> &'static str {
            match self {
                DeviceFamily::Dc2xxx => "DC2010/DC2100",
                DeviceFamily::Dc2200Scpi => "DC2200 SCPI/USBTMC",
                DeviceFamily::Dc3100 => "DC3100",
                DeviceFamily::Dc4100 => "DC4100/DC4104/LEDD4",
            }
        }

        pub fn current_setpoint_uses_amps(self) -> bool {
            matches!(self, DeviceFamily::Dc2200Scpi | DeviceFamily::Dc3100)
        }

        pub fn is_scpi(self) -> bool {
            matches!(self, DeviceFamily::Dc2200Scpi)
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum OperationMode {
        ConstantCurrent,
        Pwm,
        InternalModulation,
        Brightness,
        ExternalControl,
    }

    impl OperationMode {
        pub fn label(self) -> &'static str {
            match self {
                OperationMode::ConstantCurrent => "Constant Current",
                OperationMode::Pwm => "PWM",
                OperationMode::InternalModulation => "Internal Modulation",
                OperationMode::Brightness => "Brightness Mode",
                OperationMode::ExternalControl => "External Control",
            }
        }

        pub fn code(self) -> u8 {
            match self {
                OperationMode::ConstantCurrent => 0,
                OperationMode::Pwm
                | OperationMode::InternalModulation
                | OperationMode::Brightness => 1,
                OperationMode::ExternalControl => 2,
            }
        }

        pub fn from_code(family: DeviceFamily, code: u8) -> Option<Self> {
            match code {
                0 => Some(OperationMode::ConstantCurrent),
                1 if family == DeviceFamily::Dc3100 => Some(OperationMode::InternalModulation),
                1 if family == DeviceFamily::Dc4100 => Some(OperationMode::Brightness),
                1 => Some(OperationMode::Pwm),
                2 => Some(OperationMode::ExternalControl),
                _ => None,
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct StatusRegister(pub u32);

    impl StatusRegister {
        pub fn labels(self, family: DeviceFamily) -> Vec<&'static str> {
            let mut labels = Vec::new();
            if self.0 & 0x02 != 0 {
                labels.push("No LED");
            }
            match family {
                DeviceFamily::Dc2xxx => {
                    if self.0 & 0x08 != 0 {
                        labels.push("LED open");
                    }
                    if self.0 & 0x20 != 0 {
                        labels.push("Limit");
                    }
                }
                DeviceFamily::Dc2200Scpi => {
                    if self.0 & 0x01 != 0 {
                        labels.push("Questionable");
                    }
                    if self.0 & 0x02 != 0 {
                        labels.push("Over-current");
                    }
                    if self.0 & 0x04 != 0 {
                        labels.push("Interlock");
                    }
                }
                DeviceFamily::Dc3100 => {
                    if self.0 & 0x08 != 0 {
                        labels.push("VCC Fail");
                    }
                    if self.0 & 0x20 != 0 {
                        labels.push("OTP");
                    }
                    if self.0 & 0x80 != 0 {
                        labels.push("LED open");
                    }
                    if self.0 & 0x0200 != 0 {
                        labels.push("Limit");
                    }
                    if self.0 & 0x0800 != 0 {
                        labels.push("OTP head");
                    }
                }
                DeviceFamily::Dc4100 => {
                    if self.0 & 0x0000_0002 != 0 {
                        labels.push("VCC Fail");
                    }
                    if self.0 & 0x0000_0008 != 0 {
                        labels.push("OTP");
                    }
                    for index in 0..4 {
                        let led = index + 1;
                        if self.0 & (0x20 << (index * 2)) != 0 {
                            labels.push(match led {
                                1 => "No LED1",
                                2 => "No LED2",
                                3 => "No LED3",
                                _ => "No LED4",
                            });
                        }
                        if self.0 & (0x2000 << (index * 2)) != 0 {
                            labels.push(match led {
                                1 => "LED1 open",
                                2 => "LED2 open",
                                3 => "LED3 open",
                                _ => "LED4 open",
                            });
                        }
                        if self.0 & (0x20_0000 << (index * 2)) != 0 {
                            labels.push(match led {
                                1 => "Limit LED1",
                                2 => "Limit LED2",
                                3 => "Limit LED3",
                                _ => "Limit LED4",
                            });
                        }
                    }
                }
            }
            if labels.is_empty() {
                labels.push("No Fault");
            }
            labels
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct ThorlabsDcProbe {
        pub family: DeviceFamily,
        pub model: String,
        pub serial_number: String,
        pub firmware_revision: String,
        pub led_serial_number: String,
        pub wavelength: Option<Wavelength>,
        pub forward_bias_volts: Option<f64>,
        pub maximum_current: ElectricCurrent,
        pub maximum_frequency_hz: Option<f64>,
        pub channel_wavelengths: Vec<Wavelength>,
        pub channel_forward_bias_volts: Vec<f64>,
        pub channel_led_serial_numbers: Vec<String>,
        pub channel_maximum_currents: Vec<ElectricCurrent>,
    }

    impl ThorlabsDcProbe {
        pub fn dc2xxx_configured_fixture() -> Self {
            Self {
                family: DeviceFamily::Dc2xxx,
                model: "DC2100".into(),
                serial_number: "MDC2100-001".into(),
                firmware_revision: "1.3".into(),
                led_serial_number: "LEDHEAD-470".into(),
                wavelength: Some(Wavelength::from_nanometers(470.0)),
                forward_bias_volts: Some(3.2),
                maximum_current: ElectricCurrent::from_milliamps(2000.0),
                maximum_frequency_hz: None,
                channel_wavelengths: Vec::new(),
                channel_forward_bias_volts: Vec::new(),
                channel_led_serial_numbers: Vec::new(),
                channel_maximum_currents: Vec::new(),
            }
        }

        pub fn dc3100_configured_fixture() -> Self {
            Self {
                family: DeviceFamily::Dc3100,
                model: "DC3100".into(),
                serial_number: "MDC3100-001".into(),
                firmware_revision: "1.2".into(),
                led_serial_number: "LEDHEAD-3100".into(),
                wavelength: None,
                forward_bias_volts: None,
                maximum_current: ElectricCurrent::from_milliamps(1000.0),
                maximum_frequency_hz: Some(100.0),
                channel_wavelengths: Vec::new(),
                channel_forward_bias_volts: Vec::new(),
                channel_led_serial_numbers: Vec::new(),
                channel_maximum_currents: Vec::new(),
            }
        }

        pub fn dc2200_scpi_configured_fixture() -> Self {
            Self {
                family: DeviceFamily::Dc2200Scpi,
                model: "DC2200".into(),
                serial_number: "MDC2200-001".into(),
                firmware_revision: "SCPI-FIXTURE-1.0".into(),
                led_serial_number: "LEDHEAD-DC2200".into(),
                wavelength: Some(Wavelength::from_nanometers(530.0)),
                forward_bias_volts: Some(3.0),
                maximum_current: ElectricCurrent::from_milliamps(1200.0),
                maximum_frequency_hz: Some(10_000.0),
                channel_wavelengths: Vec::new(),
                channel_forward_bias_volts: Vec::new(),
                channel_led_serial_numbers: Vec::new(),
                channel_maximum_currents: Vec::new(),
            }
        }

        pub fn dc4100_configured_fixture() -> Self {
            Self {
                family: DeviceFamily::Dc4100,
                model: "DC4100".into(),
                serial_number: "MDC4100-001".into(),
                firmware_revision: "1.4".into(),
                led_serial_number: "n/a".into(),
                wavelength: None,
                forward_bias_volts: None,
                maximum_current: ElectricCurrent::from_milliamps(1000.0),
                maximum_frequency_hz: None,
                channel_wavelengths: vec![
                    Wavelength::from_nanometers(405.0),
                    Wavelength::from_nanometers(470.0),
                    Wavelength::from_nanometers(565.0),
                    Wavelength::from_nanometers(625.0),
                ],
                channel_forward_bias_volts: vec![3.2, 3.1, 2.4, 2.1],
                channel_led_serial_numbers: vec![
                    "DC4100-LED-1".into(),
                    "DC4100-LED-2".into(),
                    "DC4100-LED-3".into(),
                    "DC4100-LED-4".into(),
                ],
                channel_maximum_currents: vec![
                    ElectricCurrent::from_milliamps(1000.0),
                    ElectricCurrent::from_milliamps(1000.0),
                    ElectricCurrent::from_milliamps(1000.0),
                    ElectricCurrent::from_milliamps(1000.0),
                ],
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum ThorlabsDcCommand {
        DeviceName,
        SerialNumber,
        FirmwareRevision,
        LedHeadSerialNumber,
        Wavelength,
        ForwardBias,
        OutputQuery,
        SetOutput(bool),
        ChannelOutputQuery(u8),
        SetAllChannelsOutput(bool),
        SelectionModeQuery,
        SetMultiSelectionMode,
        SetChannelOutput {
            channel: u8,
            enabled: bool,
        },
        LimitCurrentQuery,
        ChannelLimitCurrentQuery(u8),
        SetLimitCurrent(ElectricCurrent),
        SetChannelLimitCurrent {
            channel: u8,
            current: ElectricCurrent,
        },
        SetLimitCurrentAmps(ElectricCurrent),
        MaximumCurrentQuery,
        MaximumFrequencyQuery,
        ConstantCurrentQuery,
        ChannelConstantCurrentQuery(u8),
        SetConstantCurrent(ElectricCurrent),
        SetChannelConstantCurrent {
            channel: u8,
            current: ElectricCurrent,
        },
        SetConstantCurrentAmps(ElectricCurrent),
        PwmCurrentQuery,
        SetPwmCurrent(ElectricCurrent),
        PwmFrequencyQuery,
        SetPwmFrequencyHz(u32),
        PwmDutyCycleQuery,
        SetPwmDutyCyclePercent(u8),
        PwmCountsQuery,
        SetPwmCounts(u32),
        ModulationCurrentQuery,
        SetModulationCurrentAmps(ElectricCurrent),
        ModulationFrequencyQuery,
        SetModulationFrequencyHz(f64),
        ModulationDepthQuery,
        SetModulationDepthPercent(u8),
        ChannelBrightnessQuery(u8),
        SetChannelBrightnessPercent {
            channel: u8,
            percent: u8,
        },
        ChannelWavelength(u8),
        ChannelForwardBias(u8),
        ChannelLedHeadSerialNumber(u8),
        ChannelMaximumCurrentQuery(u8),
        OperationModeQuery,
        SetOperationMode(OperationMode),
        StatusQuery,
        ErrorQuery,
    }

    pub fn encode(family: DeviceFamily, command: &ThorlabsDcCommand) -> String {
        if family.is_scpi() {
            return encode_scpi(command);
        }
        match command {
            ThorlabsDcCommand::DeviceName => "n?".into(),
            ThorlabsDcCommand::SerialNumber => "s?".into(),
            ThorlabsDcCommand::FirmwareRevision => "v?".into(),
            ThorlabsDcCommand::LedHeadSerialNumber => "hs?".into(),
            ThorlabsDcCommand::Wavelength => "wl?".into(),
            ThorlabsDcCommand::ForwardBias => "fb?".into(),
            ThorlabsDcCommand::OutputQuery => "o?".into(),
            ThorlabsDcCommand::SetOutput(enabled) => format!("o {}", u8::from(*enabled)),
            ThorlabsDcCommand::ChannelOutputQuery(channel) => format!("o? {}", channel),
            ThorlabsDcCommand::SetAllChannelsOutput(enabled) => {
                format!("o -1 {}", u8::from(*enabled))
            }
            ThorlabsDcCommand::SelectionModeQuery => "sm?".into(),
            ThorlabsDcCommand::SetMultiSelectionMode => "sm 0".into(),
            ThorlabsDcCommand::SetChannelOutput { channel, enabled } => {
                format!("o {} {}", channel, u8::from(*enabled))
            }
            ThorlabsDcCommand::LimitCurrentQuery => "l?".into(),
            ThorlabsDcCommand::ChannelLimitCurrentQuery(channel) => format!("l? {}", channel),
            ThorlabsDcCommand::SetLimitCurrent(current) => {
                format!("l {:.0}", current.milliamps())
            }
            ThorlabsDcCommand::SetChannelLimitCurrent { channel, current } => {
                format!("l {} {:.0}", channel, current.milliamps())
            }
            ThorlabsDcCommand::SetLimitCurrentAmps(current) => format!("l {:.6}", current.amps()),
            ThorlabsDcCommand::MaximumCurrentQuery => "ml?".into(),
            ThorlabsDcCommand::MaximumFrequencyQuery => "mf?".into(),
            ThorlabsDcCommand::ConstantCurrentQuery => "cc?".into(),
            ThorlabsDcCommand::ChannelConstantCurrentQuery(channel) => format!("cc? {}", channel),
            ThorlabsDcCommand::SetConstantCurrent(current) => {
                format!("cc {:.0}", current.milliamps())
            }
            ThorlabsDcCommand::SetChannelConstantCurrent { channel, current } => {
                format!("cc {} {:.0}", channel, current.milliamps())
            }
            ThorlabsDcCommand::SetConstantCurrentAmps(current) => {
                format!("cc {:.6}", current.amps())
            }
            ThorlabsDcCommand::PwmCurrentQuery => "pc?".into(),
            ThorlabsDcCommand::SetPwmCurrent(current) => {
                format!("pc {:.0}", current.milliamps())
            }
            ThorlabsDcCommand::PwmFrequencyQuery => "pf?".into(),
            ThorlabsDcCommand::SetPwmFrequencyHz(hz) => format!("pf {}", hz),
            ThorlabsDcCommand::PwmDutyCycleQuery => "pd?".into(),
            ThorlabsDcCommand::SetPwmDutyCyclePercent(percent) => {
                format!("pd {}", (*percent).min(100))
            }
            ThorlabsDcCommand::PwmCountsQuery => "pn?".into(),
            ThorlabsDcCommand::SetPwmCounts(counts) => format!("pn {}", counts),
            ThorlabsDcCommand::ModulationCurrentQuery => "cm?".into(),
            ThorlabsDcCommand::SetModulationCurrentAmps(current) => {
                format!("cm {:.6}", current.amps())
            }
            ThorlabsDcCommand::ModulationFrequencyQuery => "f?".into(),
            ThorlabsDcCommand::SetModulationFrequencyHz(hz) => format!("f {}", hz),
            ThorlabsDcCommand::ModulationDepthQuery => "d?".into(),
            ThorlabsDcCommand::SetModulationDepthPercent(percent) => {
                format!("d {}", (*percent).min(100))
            }
            ThorlabsDcCommand::ChannelBrightnessQuery(channel) => format!("bp? {}", channel),
            ThorlabsDcCommand::SetChannelBrightnessPercent { channel, percent } => {
                format!("bp {} {}", channel, (*percent).min(100))
            }
            ThorlabsDcCommand::ChannelWavelength(channel) => format!("wl? {}", channel),
            ThorlabsDcCommand::ChannelForwardBias(channel) => format!("fb? {}", channel),
            ThorlabsDcCommand::ChannelLedHeadSerialNumber(channel) => format!("hs? {}", channel),
            ThorlabsDcCommand::ChannelMaximumCurrentQuery(channel) => format!("ml? {}", channel),
            ThorlabsDcCommand::OperationModeQuery => "m?".into(),
            ThorlabsDcCommand::SetOperationMode(mode) => format!("m {}", mode.code()),
            ThorlabsDcCommand::StatusQuery => "r?".into(),
            ThorlabsDcCommand::ErrorQuery => "e?".into(),
        }
    }

    fn encode_scpi(command: &ThorlabsDcCommand) -> String {
        match command {
            ThorlabsDcCommand::DeviceName => "*IDN?".into(),
            ThorlabsDcCommand::SerialNumber => "SYST:SER?".into(),
            ThorlabsDcCommand::FirmwareRevision => "SYST:VERS?".into(),
            ThorlabsDcCommand::LedHeadSerialNumber => "SOUR:LED:SER?".into(),
            ThorlabsDcCommand::Wavelength => "SOUR:LED:WAV?".into(),
            ThorlabsDcCommand::ForwardBias => "SOUR:LED:VF?".into(),
            ThorlabsDcCommand::OutputQuery => "OUTP?".into(),
            ThorlabsDcCommand::SetOutput(enabled) => format!("OUTP {}", u8::from(*enabled)),
            ThorlabsDcCommand::LimitCurrentQuery => "CURR:LIM?".into(),
            ThorlabsDcCommand::SetLimitCurrent(current)
            | ThorlabsDcCommand::SetLimitCurrentAmps(current) => {
                format!("CURR:LIM {:.6}", current.amps())
            }
            ThorlabsDcCommand::MaximumCurrentQuery => "CURR:LIM:MAX?".into(),
            ThorlabsDcCommand::MaximumFrequencyQuery => "PULS:FREQ:MAX?".into(),
            ThorlabsDcCommand::ConstantCurrentQuery => "CURR?".into(),
            ThorlabsDcCommand::SetConstantCurrent(current)
            | ThorlabsDcCommand::SetConstantCurrentAmps(current) => {
                format!("CURR {:.6}", current.amps())
            }
            ThorlabsDcCommand::PwmCurrentQuery => "PULS:CURR?".into(),
            ThorlabsDcCommand::SetPwmCurrent(current) => {
                format!("PULS:CURR {:.6}", current.amps())
            }
            ThorlabsDcCommand::PwmFrequencyQuery => "PULS:FREQ?".into(),
            ThorlabsDcCommand::SetPwmFrequencyHz(hz) => format!("PULS:FREQ {}", hz),
            ThorlabsDcCommand::PwmDutyCycleQuery => "PULS:DCYC?".into(),
            ThorlabsDcCommand::SetPwmDutyCyclePercent(percent) => {
                format!("PULS:DCYC {}", (*percent).min(100))
            }
            ThorlabsDcCommand::PwmCountsQuery => "PULS:COUN?".into(),
            ThorlabsDcCommand::SetPwmCounts(counts) => format!("PULS:COUN {}", counts),
            ThorlabsDcCommand::OperationModeQuery => "SOUR:FUNC?".into(),
            ThorlabsDcCommand::SetOperationMode(OperationMode::ConstantCurrent) => {
                "SOUR:FUNC CURR".into()
            }
            ThorlabsDcCommand::SetOperationMode(OperationMode::Pwm) => "SOUR:FUNC PULS".into(),
            ThorlabsDcCommand::SetOperationMode(OperationMode::ExternalControl) => {
                "SOUR:FUNC EXT".into()
            }
            ThorlabsDcCommand::SetOperationMode(mode) => format!("SOUR:FUNC {}", mode.code()),
            ThorlabsDcCommand::StatusQuery => "STAT:QUES:COND?".into(),
            ThorlabsDcCommand::ErrorQuery => "SYST:ERR?".into(),
            _ => format!("/* unsupported DC2200 SCPI command: {command:?} */"),
        }
    }

    pub fn parse_bool(reply: &str) -> Result<bool> {
        match reply.trim().chars().next() {
            Some('0') => Ok(false),
            Some('1') => Ok(true),
            _ => Err(Error::new(
                ErrorCode::Transport,
                format!("invalid Thorlabs DC boolean reply: {reply}"),
            )),
        }
    }

    pub fn parse_current_ma(reply: &str) -> Result<ElectricCurrent> {
        let milliamps = reply
            .trim()
            .parse::<f64>()
            .map_err(|_| Error::new(ErrorCode::Transport, "invalid DC current value"))?;
        Ok(ElectricCurrent::from_milliamps(milliamps))
    }

    pub fn parse_operation_mode(family: DeviceFamily, reply: &str) -> Result<OperationMode> {
        let trimmed = reply.trim().trim_matches('"');
        if family == DeviceFamily::Dc2200Scpi {
            return match trimmed.to_ascii_uppercase().as_str() {
                "CURR" | "CURRENT" => Ok(OperationMode::ConstantCurrent),
                "PULS" | "PULSE" | "PWM" => Ok(OperationMode::Pwm),
                "EXT" | "EXTERNAL" => Ok(OperationMode::ExternalControl),
                _ => Err(Error::new(
                    ErrorCode::Transport,
                    "unknown DC operation mode",
                )),
            };
        }
        let code = trimmed
            .parse::<u8>()
            .map_err(|_| Error::new(ErrorCode::Transport, "invalid DC mode value"))?;
        OperationMode::from_code(family, code)
            .ok_or_else(|| Error::new(ErrorCode::Transport, "unknown DC operation mode"))
    }

    pub fn parse_status(reply: &str) -> Result<StatusRegister> {
        let status = reply
            .trim()
            .parse::<u32>()
            .map_err(|_| Error::new(ErrorCode::Transport, "invalid DC status register"))?;
        Ok(StatusRegister(status))
    }

    pub fn parse_last_error(reply: &str) -> Result<()> {
        let trimmed = reply.trim();
        if trimmed.is_empty() {
            return Err(Error::new(ErrorCode::Transport, "empty DC error reply"));
        }
        let mut parts = trimmed.splitn(2, ' ');
        let _prefix = parts.next();
        let rest = parts.next().unwrap_or(trimmed);
        let mut code_and_message = rest.splitn(2, ':');
        let code_text = code_and_message.next().unwrap_or(rest).trim();
        let code = code_text
            .parse::<i64>()
            .map_err(|_| Error::new(ErrorCode::Transport, "invalid DC error code"))?;
        if code == 0 {
            Ok(())
        } else {
            let message = code_and_message.next().unwrap_or("device error").trim();
            Err(Error::new(
                ErrorCode::Driver,
                format!("Thorlabs DC error {code}: {message}"),
            ))
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct ThorlabsDcChannelProbeResult {
        pub channel: u8,
        pub enabled: Option<bool>,
        pub wavelength: Option<Wavelength>,
        pub forward_bias_volts: Option<f64>,
        pub led_serial_number: Option<String>,
        pub maximum_current: Option<ElectricCurrent>,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct ThorlabsDcProbeResult {
        pub family: DeviceFamily,
        pub model: Option<String>,
        pub serial_number: Option<String>,
        pub firmware_revision: Option<String>,
        pub led_serial_number: Option<String>,
        pub wavelength: Option<Wavelength>,
        pub forward_bias_volts: Option<f64>,
        pub output_enabled: Option<bool>,
        pub operation_mode: Option<OperationMode>,
        pub maximum_current: Option<ElectricCurrent>,
        pub maximum_frequency_hz: Option<f64>,
        pub status: Option<StatusRegister>,
        pub replies: Vec<(String, String)>,
        pub channels: Vec<ThorlabsDcChannelProbeResult>,
    }

    pub fn probe_commands(family: DeviceFamily) -> Vec<ThorlabsDcCommand> {
        let mut commands = vec![
            ThorlabsDcCommand::DeviceName,
            ThorlabsDcCommand::SerialNumber,
            ThorlabsDcCommand::FirmwareRevision,
            ThorlabsDcCommand::LedHeadSerialNumber,
            ThorlabsDcCommand::Wavelength,
            ThorlabsDcCommand::ForwardBias,
            ThorlabsDcCommand::OutputQuery,
            ThorlabsDcCommand::OperationModeQuery,
            ThorlabsDcCommand::MaximumCurrentQuery,
            ThorlabsDcCommand::StatusQuery,
            ThorlabsDcCommand::ErrorQuery,
        ];
        if matches!(
            family,
            DeviceFamily::Dc2xxx | DeviceFamily::Dc2200Scpi | DeviceFamily::Dc3100
        ) {
            commands.push(ThorlabsDcCommand::MaximumFrequencyQuery);
        }
        if family == DeviceFamily::Dc4100 {
            commands.push(ThorlabsDcCommand::SelectionModeQuery);
            for channel in 0..4 {
                commands.extend([
                    ThorlabsDcCommand::ChannelOutputQuery(channel),
                    ThorlabsDcCommand::ChannelWavelength(channel),
                    ThorlabsDcCommand::ChannelForwardBias(channel),
                    ThorlabsDcCommand::ChannelLedHeadSerialNumber(channel),
                    ThorlabsDcCommand::ChannelMaximumCurrentQuery(channel),
                ]);
            }
        }
        commands
    }

    pub fn probe_script(family: DeviceFamily) -> Vec<String> {
        probe_commands(family)
            .iter()
            .map(|command| encode(family, command))
            .collect()
    }

    pub fn execute_probe_script(
        serial: &mut dyn SerialIo,
        family: DeviceFamily,
        polls_per_command: usize,
    ) -> Result<ThorlabsDcProbeResult> {
        let mut codec = SerialLineCodec::new(SEND_ENDING, RECV_ENDING);
        let mut result = ThorlabsDcProbeResult {
            family,
            model: None,
            serial_number: None,
            firmware_revision: None,
            led_serial_number: None,
            wavelength: None,
            forward_bias_volts: None,
            output_enabled: None,
            operation_mode: None,
            maximum_current: None,
            maximum_frequency_hz: None,
            status: None,
            replies: Vec::new(),
            channels: Vec::new(),
        };
        for command in probe_commands(family) {
            let encoded = encode(family, &command);
            serial.write(&codec.encode(&encoded))?;
            let reply = read_line(serial, &mut codec, polls_per_command)?;
            apply_probe_reply(&mut result, &command, &reply)?;
            result.replies.push((encoded, reply));
        }
        Ok(result)
    }

    fn apply_probe_reply(
        result: &mut ThorlabsDcProbeResult,
        command: &ThorlabsDcCommand,
        reply: &str,
    ) -> Result<()> {
        match command {
            ThorlabsDcCommand::DeviceName => result.model = Some(clean_reply(reply)),
            ThorlabsDcCommand::SerialNumber => result.serial_number = Some(clean_reply(reply)),
            ThorlabsDcCommand::FirmwareRevision => {
                result.firmware_revision = Some(clean_reply(reply))
            }
            ThorlabsDcCommand::LedHeadSerialNumber => {
                result.led_serial_number = Some(clean_reply(reply))
            }
            ThorlabsDcCommand::Wavelength => {
                result.wavelength = parse_number(reply).map(Wavelength::from_nanometers);
            }
            ThorlabsDcCommand::ForwardBias => result.forward_bias_volts = parse_number(reply),
            ThorlabsDcCommand::OutputQuery => result.output_enabled = Some(parse_bool(reply)?),
            ThorlabsDcCommand::OperationModeQuery => {
                result.operation_mode = Some(parse_operation_mode(result.family, reply)?)
            }
            ThorlabsDcCommand::MaximumCurrentQuery => {
                result.maximum_current = Some(parse_current(result.family, reply)?)
            }
            ThorlabsDcCommand::MaximumFrequencyQuery => {
                result.maximum_frequency_hz = parse_number(reply)
            }
            ThorlabsDcCommand::StatusQuery => result.status = Some(parse_status(reply)?),
            ThorlabsDcCommand::ErrorQuery => parse_last_error(reply)?,
            ThorlabsDcCommand::SelectionModeQuery => {}
            ThorlabsDcCommand::ChannelOutputQuery(channel) => {
                channel_result(result, *channel).enabled = Some(parse_bool(reply)?);
            }
            ThorlabsDcCommand::ChannelWavelength(channel) => {
                channel_result(result, *channel).wavelength =
                    parse_number(reply).map(Wavelength::from_nanometers);
            }
            ThorlabsDcCommand::ChannelForwardBias(channel) => {
                channel_result(result, *channel).forward_bias_volts = parse_number(reply);
            }
            ThorlabsDcCommand::ChannelLedHeadSerialNumber(channel) => {
                channel_result(result, *channel).led_serial_number = Some(clean_reply(reply));
            }
            ThorlabsDcCommand::ChannelMaximumCurrentQuery(channel) => {
                let family = result.family;
                channel_result(result, *channel).maximum_current =
                    Some(parse_current(family, reply)?);
            }
            _ => {}
        }
        Ok(())
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
            "timed out waiting for Thorlabs DC probe reply",
        ))
    }

    fn channel_result(
        result: &mut ThorlabsDcProbeResult,
        channel: u8,
    ) -> &mut ThorlabsDcChannelProbeResult {
        if let Some(index) = result
            .channels
            .iter()
            .position(|entry| entry.channel == channel)
        {
            return &mut result.channels[index];
        }
        result.channels.push(ThorlabsDcChannelProbeResult {
            channel,
            enabled: None,
            wavelength: None,
            forward_bias_volts: None,
            led_serial_number: None,
            maximum_current: None,
        });
        result.channels.last_mut().expect("pushed channel")
    }

    pub(crate) fn parse_current(family: DeviceFamily, reply: &str) -> Result<ElectricCurrent> {
        let value = reply
            .trim()
            .parse::<f64>()
            .map_err(|_| Error::new(ErrorCode::Transport, "invalid DC current value"))?;
        if family.current_setpoint_uses_amps() {
            Ok(ElectricCurrent::from_amps(value))
        } else {
            Ok(ElectricCurrent::from_milliamps(value))
        }
    }

    pub(crate) fn parse_number(reply: &str) -> Option<f64> {
        reply.trim().parse::<f64>().ok()
    }

    pub(crate) fn clean_reply(reply: &str) -> String {
        reply.trim().trim_matches('"').into()
    }
}

pub struct ThorlabsDcDiscovery {
    next_id: DriverId,
    probes: Vec<ThorlabsDcConfiguredProbe>,
}

impl ThorlabsDcDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![
                ThorlabsDcConfiguredProbe::fixture(
                    "Configured Thorlabs DC2010/DC2100 LED controller",
                    protocol::ThorlabsDcProbe::dc2xxx_configured_fixture(),
                ),
                ThorlabsDcConfiguredProbe::fixture(
                    "Configured Thorlabs DC3100 LED controller",
                    protocol::ThorlabsDcProbe::dc3100_configured_fixture(),
                ),
                ThorlabsDcConfiguredProbe::fixture(
                    "Configured Thorlabs DC2200 SCPI LED controller",
                    protocol::ThorlabsDcProbe::dc2200_scpi_configured_fixture(),
                ),
                ThorlabsDcConfiguredProbe::fixture(
                    "Configured Thorlabs DC4100/DC4104 LED controller",
                    protocol::ThorlabsDcProbe::dc4100_configured_fixture(),
                ),
            ],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), "thorlabs_dc" | "thorlabs_dc_led"))
            .map(ThorlabsDcConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for ThorlabsDcDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver = if configured.connect_real_transport {
                    let endpoint = configured.endpoint.clone().ok_or_else(|| {
                        Error::new(
                            ErrorCode::InvalidProperty,
                            "Thorlabs DC config requires serial_port or USBTMC endpoint fields when connect is true",
                        )
                    })?;
                    match endpoint {
                        ThorlabsDcEndpoint::Serial(endpoint) => Box::new(ThorlabsDcDriver::serial(
                            id,
                            configured.probe,
                            endpoint.port_name,
                            endpoint.baud_rate,
                            endpoint.timeout_ms,
                        )?)
                            as Box<dyn Driver>,
                        ThorlabsDcEndpoint::UsbTmc(endpoint) => {
                            Box::new(ThorlabsDcDriver::usb_tmc(id, configured.probe, endpoint)?)
                                as Box<dyn Driver>
                        }
                    }
                } else {
                    Box::new(ThorlabsDcDriver::configured(id, configured)) as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct ThorlabsDcConfiguredProbe {
    pub label: String,
    pub endpoint: Option<ThorlabsDcEndpoint>,
    pub connect_real_transport: bool,
    probe: protocol::ThorlabsDcProbe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThorlabsDcEndpoint {
    Serial(ThorlabsDcSerialEndpoint),
    UsbTmc(ThorlabsDcUsbTmcEndpoint),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThorlabsDcSerialEndpoint {
    pub port_name: String,
    pub baud_rate: u32,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThorlabsDcUsbTmcEndpoint {
    pub vendor_id: u16,
    pub product_id: u16,
    pub interface: u8,
    pub bulk_out_endpoint: u8,
    pub bulk_in_endpoint: u8,
    pub read_size: usize,
}

impl ThorlabsDcConfiguredProbe {
    fn fixture(label: impl Into<String>, probe: protocol::ThorlabsDcProbe) -> Self {
        Self {
            label: label.into(),
            endpoint: None,
            connect_real_transport: false,
            probe,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let family = string_prop(device, "family")
            .or_else(|| string_prop(device, "model"))
            .as_deref()
            .map(thorlabs_dc_family_from_label)
            .transpose()?
            .unwrap_or(protocol::DeviceFamily::Dc2xxx);
        let mut probe = match family {
            protocol::DeviceFamily::Dc2xxx => {
                protocol::ThorlabsDcProbe::dc2xxx_configured_fixture()
            }
            protocol::DeviceFamily::Dc2200Scpi => {
                protocol::ThorlabsDcProbe::dc2200_scpi_configured_fixture()
            }
            protocol::DeviceFamily::Dc3100 => {
                protocol::ThorlabsDcProbe::dc3100_configured_fixture()
            }
            protocol::DeviceFamily::Dc4100 => {
                protocol::ThorlabsDcProbe::dc4100_configured_fixture()
            }
        };

        probe.model = string_prop(device, "model").unwrap_or(probe.model);
        probe.serial_number = string_prop(device, "serial_number").unwrap_or(probe.serial_number);
        probe.firmware_revision =
            string_prop(device, "firmware").unwrap_or(probe.firmware_revision);
        probe.led_serial_number =
            string_prop(device, "led_serial").unwrap_or(probe.led_serial_number);
        probe.wavelength =
            wavelength_config(device, "wavelength", "wavelength_nm").or(probe.wavelength);
        probe.forward_bias_volts = voltage_config(device, "forward_bias", "forward_bias_v")
            .map(|value| value.volts())
            .or(probe.forward_bias_volts);
        probe.maximum_current =
            electric_current_config(device, "maximum_current", "maximum_current_ma")
                .unwrap_or(probe.maximum_current);
        probe.maximum_frequency_hz =
            frequency_config(device, "maximum_frequency", "maximum_frequency_hz")
                .map(|value| value.hertz())
                .or(probe.maximum_frequency_hz);

        if let Some(wavelengths) = wavelength_list_prop(device, "channel_wavelengths") {
            probe.channel_wavelengths = wavelengths;
        }
        if let Some(currents) = electric_current_list_prop(device, "channel_maximum_currents") {
            probe.channel_maximum_currents = currents;
        }

        let serial_endpoint =
            string_prop(device, "serial_port").map(|port_name| ThorlabsDcSerialEndpoint {
                port_name,
                baud_rate: u32_prop(device, "baud_rate").unwrap_or(115_200),
                timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(100),
            });
        let usb_endpoint = usb_tmc_endpoint(device)?;
        if serial_endpoint.is_some() && usb_endpoint.is_some() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Thorlabs DC config must not set both serial_port and USBTMC endpoint fields",
            ));
        }
        if usb_endpoint.is_some() && family != protocol::DeviceFamily::Dc2200Scpi {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Thorlabs DC USBTMC transport is supported only for the DC2200 SCPI family",
            ));
        }
        let endpoint = serial_endpoint
            .map(ThorlabsDcEndpoint::Serial)
            .or_else(|| usb_endpoint.map(ThorlabsDcEndpoint::UsbTmc));

        Ok(Self {
            label: if device.label.is_empty() {
                format!("Configured Thorlabs {}", probe.family.model_family())
            } else {
                device.label.clone()
            },
            endpoint,
            connect_real_transport: bool_prop(device, "connect").unwrap_or(false),
            probe,
        })
    }
}

pub struct ThorlabsDcDriver {
    id: DriverId,
    resource: ResourceId,
    controller: DeviceId,
    channels: [DeviceId; 4],
    probe: protocol::ThorlabsDcProbe,
    enabled: bool,
    operation_mode: protocol::OperationMode,
    limit_current: ElectricCurrent,
    constant_current: ElectricCurrent,
    pwm_current: ElectricCurrent,
    pwm_frequency_hz: u32,
    pwm_duty_cycle_percent: u8,
    pwm_counts: u32,
    modulation_current: ElectricCurrent,
    modulation_frequency_hz: f64,
    modulation_depth_percent: u8,
    channel_enabled: [bool; 4],
    channel_limit_current: [ElectricCurrent; 4],
    channel_constant_current: [ElectricCurrent; 4],
    channel_brightness_percent: [u8; 4],
    status: protocol::StatusRegister,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
    status_codec: SerialLineCodec,
    resource_label: String,
    resource_kind: String,
    endpoint_metadata: BTreeMap<String, Value>,
}

impl ThorlabsDcDriver {
    pub fn configured_fixture(id: DriverId) -> Self {
        Self::dc2xxx_configured_fixture(id)
    }

    pub fn configured(id: DriverId, configured: ThorlabsDcConfiguredProbe) -> Self {
        Self::new_configured(id, configured, Box::new(ScriptedSerial::new()), false)
    }

    pub fn dc2xxx_configured_fixture(id: DriverId) -> Self {
        let serial = ScriptedSerial::new();
        Self::new(
            id,
            protocol::ThorlabsDcProbe::dc2xxx_configured_fixture(),
            Box::new(serial),
        )
    }

    pub fn dc3100_configured_fixture(id: DriverId) -> Self {
        let serial = ScriptedSerial::new();
        Self::new(
            id,
            protocol::ThorlabsDcProbe::dc3100_configured_fixture(),
            Box::new(serial),
        )
    }

    pub fn dc2200_scpi_configured_fixture(id: DriverId) -> Self {
        let serial = ScriptedSerial::new();
        Self::new(
            id,
            protocol::ThorlabsDcProbe::dc2200_scpi_configured_fixture(),
            Box::new(serial),
        )
    }

    pub fn dc4100_configured_fixture(id: DriverId) -> Self {
        let serial = ScriptedSerial::new();
        Self::new(
            id,
            protocol::ThorlabsDcProbe::dc4100_configured_fixture(),
            Box::new(serial),
        )
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(
        id: DriverId,
        probe: protocol::ThorlabsDcProbe,
        port_name: impl Into<String>,
        baud_rate: u32,
        timeout_ms: u64,
    ) -> Result<Self> {
        let port_name = port_name.into();
        let mut serial = numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(port_name.clone(), baud_rate)
                .timeout(Duration::from_millis(timeout_ms)),
        )?;
        let probe_result = protocol::execute_probe_script(&mut serial, probe.family, 4)?;
        let mut driver = Self::new(id, probe, Box::new(serial)).with_probe_result(probe_result);
        driver.set_serial_endpoint_metadata(port_name, baud_rate, timeout_ms, true);
        Ok(driver)
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(
        _id: DriverId,
        _probe: protocol::ThorlabsDcProbe,
        _port_name: impl Into<String>,
        _baud_rate: u32,
        _timeout_ms: u64,
    ) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Thorlabs DC real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    #[cfg(feature = "os-usb")]
    pub fn usb_tmc(
        id: DriverId,
        probe: protocol::ThorlabsDcProbe,
        endpoint: ThorlabsDcUsbTmcEndpoint,
    ) -> Result<Self> {
        if probe.family != protocol::DeviceFamily::Dc2200Scpi {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Thorlabs DC USBTMC transport is supported only for the DC2200 SCPI family",
            ));
        }
        let mut serial = live_thorlabs_dc_usbtmc::LiveUsbTmc::open(&endpoint)?;
        let probe_result = protocol::execute_probe_script(&mut serial, probe.family, 4)?;
        let mut driver = Self::new_with_transport(
            id,
            probe,
            Box::new(serial),
            "thorlabs-dc2200-usbtmc",
            "usb.usbtmc",
        )
        .with_probe_result(probe_result);
        driver.set_usbtmc_endpoint_metadata(&endpoint, true);
        Ok(driver)
    }

    #[cfg(not(feature = "os-usb"))]
    pub fn usb_tmc(
        _id: DriverId,
        _probe: protocol::ThorlabsDcProbe,
        _endpoint: ThorlabsDcUsbTmcEndpoint,
    ) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Thorlabs DC USBTMC transport requires the numanager-drivers os-usb feature",
        ))
    }

    pub fn new(id: DriverId, probe: protocol::ThorlabsDcProbe, serial: Box<dyn SerialIo>) -> Self {
        Self::new_with_transport(id, probe, serial, "thorlabs-dc-serial", "serial")
    }

    fn new_with_transport(
        id: DriverId,
        probe: protocol::ThorlabsDcProbe,
        serial: Box<dyn SerialIo>,
        resource_label: impl Into<String>,
        resource_kind: impl Into<String>,
    ) -> Self {
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 1501)),
            controller: DeviceId(NodeId(id.0 * 1000 + 1510)),
            channels: [
                DeviceId(NodeId(id.0 * 1000 + 1520)),
                DeviceId(NodeId(id.0 * 1000 + 1521)),
                DeviceId(NodeId(id.0 * 1000 + 1522)),
                DeviceId(NodeId(id.0 * 1000 + 1523)),
            ],
            limit_current: probe.maximum_current,
            constant_current: ElectricCurrent::from_milliamps(0.0),
            pwm_current: ElectricCurrent::from_milliamps(0.0),
            pwm_frequency_hz: 1000,
            pwm_duty_cycle_percent: 50,
            pwm_counts: 0,
            modulation_current: ElectricCurrent::from_milliamps(0.0),
            modulation_frequency_hz: 10.0,
            modulation_depth_percent: 100,
            channel_enabled: [false; 4],
            channel_limit_current: [probe.maximum_current; 4],
            channel_constant_current: [ElectricCurrent::from_milliamps(0.0); 4],
            channel_brightness_percent: [0; 4],
            probe,
            enabled: false,
            operation_mode: protocol::OperationMode::ConstantCurrent,
            status: protocol::StatusRegister(0),
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            codec: SerialLineCodec::new(protocol::SEND_ENDING, protocol::RECV_ENDING),
            status_codec: SerialLineCodec::new(protocol::SEND_ENDING, protocol::STATUS_RECV_ENDING),
            resource_label: resource_label.into(),
            resource_kind: resource_kind.into(),
            endpoint_metadata: BTreeMap::new(),
        }
    }

    fn new_configured(
        id: DriverId,
        configured: ThorlabsDcConfiguredProbe,
        serial: Box<dyn SerialIo>,
        connected: bool,
    ) -> Self {
        let mut driver = match configured.endpoint.as_ref() {
            Some(ThorlabsDcEndpoint::UsbTmc(_)) => Self::new_with_transport(
                id,
                configured.probe,
                serial,
                "thorlabs-dc2200-usbtmc",
                "usb.usbtmc",
            ),
            _ => Self::new(id, configured.probe, serial),
        };
        if let Some(endpoint) = configured.endpoint.as_ref() {
            driver.set_endpoint_metadata(endpoint, connected);
        } else {
            driver
                .endpoint_metadata
                .insert("connected".into(), Value::Bool(connected));
        }
        driver
    }

    fn set_endpoint_metadata(&mut self, endpoint: &ThorlabsDcEndpoint, connected: bool) {
        match endpoint {
            ThorlabsDcEndpoint::Serial(endpoint) => self.set_serial_endpoint_metadata(
                endpoint.port_name.clone(),
                endpoint.baud_rate,
                endpoint.timeout_ms,
                connected,
            ),
            ThorlabsDcEndpoint::UsbTmc(endpoint) => {
                self.set_usbtmc_endpoint_metadata(endpoint, connected)
            }
        }
    }

    fn set_serial_endpoint_metadata(
        &mut self,
        port_name: String,
        baud_rate: u32,
        timeout_ms: u64,
        connected: bool,
    ) {
        self.endpoint_metadata = BTreeMap::from([
            ("baud_rate".into(), Value::I64(baud_rate as i64)),
            ("serial_port".into(), Value::String(port_name)),
            (
                "serial_timeout".into(),
                Value::TimeInterval(TimeInterval::from_milliseconds(timeout_ms as f64)),
            ),
            ("connected".into(), Value::Bool(connected)),
        ]);
    }

    fn set_usbtmc_endpoint_metadata(
        &mut self,
        endpoint: &ThorlabsDcUsbTmcEndpoint,
        connected: bool,
    ) {
        self.endpoint_metadata = BTreeMap::from([
            (
                "usb_vendor_id".into(),
                Value::I64(endpoint.vendor_id as i64),
            ),
            (
                "usb_product_id".into(),
                Value::I64(endpoint.product_id as i64),
            ),
            (
                "usb_interface".into(),
                Value::I64(endpoint.interface as i64),
            ),
            (
                "bulk_out_endpoint".into(),
                Value::I64(endpoint.bulk_out_endpoint as i64),
            ),
            (
                "bulk_in_endpoint".into(),
                Value::I64(endpoint.bulk_in_endpoint as i64),
            ),
            ("read_size".into(), Value::I64(endpoint.read_size as i64)),
            ("connected".into(), Value::Bool(connected)),
        ]);
    }

    #[cfg(any(feature = "os-serial", feature = "os-usb"))]
    fn with_probe_result(mut self, probe_result: protocol::ThorlabsDcProbeResult) -> Self {
        if let Some(model) = probe_result.model {
            self.probe.model = model;
        }
        if let Some(serial_number) = probe_result.serial_number {
            self.probe.serial_number = serial_number;
        }
        if let Some(firmware_revision) = probe_result.firmware_revision {
            self.probe.firmware_revision = firmware_revision;
        }
        if let Some(led_serial_number) = probe_result.led_serial_number {
            self.probe.led_serial_number = led_serial_number;
        }
        if let Some(wavelength) = probe_result.wavelength {
            self.probe.wavelength = Some(wavelength);
        }
        if let Some(forward_bias_volts) = probe_result.forward_bias_volts {
            self.probe.forward_bias_volts = Some(forward_bias_volts);
        }
        if let Some(maximum_current) = probe_result.maximum_current {
            self.probe.maximum_current = maximum_current;
            self.limit_current = maximum_current;
        }
        if let Some(maximum_frequency_hz) = probe_result.maximum_frequency_hz {
            self.probe.maximum_frequency_hz = Some(maximum_frequency_hz);
        }
        if let Some(output_enabled) = probe_result.output_enabled {
            self.enabled = output_enabled;
        }
        if let Some(operation_mode) = probe_result.operation_mode {
            self.operation_mode = operation_mode;
        }
        if let Some(status) = probe_result.status {
            self.status = status;
        }
        for channel in probe_result.channels {
            let index = channel.channel as usize;
            if index >= self.channel_enabled.len() {
                continue;
            }
            if let Some(enabled) = channel.enabled {
                self.channel_enabled[index] = enabled;
            }
            if let Some(wavelength) = channel.wavelength {
                if self.probe.channel_wavelengths.len() <= index {
                    let fallback = self
                        .probe
                        .wavelength
                        .unwrap_or(Wavelength::from_nanometers(0.0));
                    self.probe.channel_wavelengths.resize(index + 1, fallback);
                }
                self.probe.channel_wavelengths[index] = wavelength;
            }
            if let Some(forward_bias_volts) = channel.forward_bias_volts {
                if self.probe.channel_forward_bias_volts.len() <= index {
                    self.probe
                        .channel_forward_bias_volts
                        .resize(index + 1, self.probe.forward_bias_volts.unwrap_or(0.0));
                }
                self.probe.channel_forward_bias_volts[index] = forward_bias_volts;
            }
            if let Some(led_serial_number) = channel.led_serial_number {
                if self.probe.channel_led_serial_numbers.len() <= index {
                    self.probe
                        .channel_led_serial_numbers
                        .resize(index + 1, self.probe.led_serial_number.clone());
                }
                self.probe.channel_led_serial_numbers[index] = led_serial_number;
            }
            if let Some(maximum_current) = channel.maximum_current {
                if self.probe.channel_maximum_currents.len() <= index {
                    self.probe
                        .channel_maximum_currents
                        .resize(index + 1, self.probe.maximum_current);
                }
                self.probe.channel_maximum_currents[index] = maximum_current;
                self.channel_limit_current[index] = maximum_current;
            }
        }
        self
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn send(&mut self, command: protocol::ThorlabsDcCommand) -> Result<()> {
        let line = protocol::encode(self.probe.family, &command);
        self.serial.write(&self.codec.encode(&line))
    }

    fn send_checked(&mut self, command: protocol::ThorlabsDcCommand) -> Result<()> {
        self.send(command)?;
        self.send(protocol::ThorlabsDcCommand::ErrorQuery)?;
        if let Ok(bytes) = self.serial.read_available() {
            let mut saw_error_reply = false;
            for line in self.codec.push(&bytes) {
                if line.trim_start().starts_with("CMD") || line.trim_start().starts_with("E") {
                    protocol::parse_last_error(&line)?;
                    saw_error_reply = true;
                } else {
                    self.pending
                        .push_back(DriverEvent::Event(Event::Log(LogEvent {
                            driver: Some(self.id),
                            message: format!("thorlabs-dc serial: {line}"),
                        })));
                }
            }
            if saw_error_reply {
                return Ok(());
            }
        }
        Ok(())
    }

    fn query_for_property(
        &self,
        device: DeviceId,
        key: &str,
    ) -> Result<protocol::ThorlabsDcCommand> {
        let key = thorlabs_dc_public_key(key);
        if let Some(index) = self.channel_index(device) {
            let channel = index as u8;
            return match key {
                "enabled" => Ok(protocol::ThorlabsDcCommand::ChannelOutputQuery(channel)),
                "limit_current" => Ok(protocol::ThorlabsDcCommand::ChannelLimitCurrentQuery(
                    channel,
                )),
                "constant_current" => Ok(protocol::ThorlabsDcCommand::ChannelConstantCurrentQuery(
                    channel,
                )),
                "brightness" => Ok(protocol::ThorlabsDcCommand::ChannelBrightnessQuery(channel)),
                "maximum_current" => Ok(protocol::ThorlabsDcCommand::ChannelMaximumCurrentQuery(
                    channel,
                )),
                "wavelength" => Ok(protocol::ThorlabsDcCommand::ChannelWavelength(channel)),
                "forward_bias" => Ok(protocol::ThorlabsDcCommand::ChannelForwardBias(channel)),
                "led_serial" => Ok(protocol::ThorlabsDcCommand::ChannelLedHeadSerialNumber(
                    channel,
                )),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown Thorlabs DC channel property {key}"),
                )),
            };
        }
        if device != self.controller {
            return Err(Error::new(ErrorCode::InvalidCommand, "unknown device"));
        }
        match key {
            "enabled" => Ok(protocol::ThorlabsDcCommand::OutputQuery),
            "operation_mode" => Ok(protocol::ThorlabsDcCommand::OperationModeQuery),
            "limit_current" => Ok(protocol::ThorlabsDcCommand::LimitCurrentQuery),
            "constant_current" => Ok(protocol::ThorlabsDcCommand::ConstantCurrentQuery),
            "pwm_current" => Ok(protocol::ThorlabsDcCommand::PwmCurrentQuery),
            "pwm_frequency" => Ok(protocol::ThorlabsDcCommand::PwmFrequencyQuery),
            "pwm_duty_cycle" => Ok(protocol::ThorlabsDcCommand::PwmDutyCycleQuery),
            "pwm_counts" => Ok(protocol::ThorlabsDcCommand::PwmCountsQuery),
            "modulation_current" => Ok(protocol::ThorlabsDcCommand::ModulationCurrentQuery),
            "modulation_frequency" => Ok(protocol::ThorlabsDcCommand::ModulationFrequencyQuery),
            "modulation_depth" => Ok(protocol::ThorlabsDcCommand::ModulationDepthQuery),
            "maximum_frequency" => Ok(protocol::ThorlabsDcCommand::MaximumFrequencyQuery),
            "status" | "status_register" => Ok(protocol::ThorlabsDcCommand::StatusQuery),
            "wavelength" => Ok(protocol::ThorlabsDcCommand::Wavelength),
            "forward_bias" => Ok(protocol::ThorlabsDcCommand::ForwardBias),
            "firmware" => Ok(protocol::ThorlabsDcCommand::FirmwareRevision),
            "led_serial" => Ok(protocol::ThorlabsDcCommand::LedHeadSerialNumber),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Thorlabs DC property {key}"),
            )),
        }
    }

    fn read_query_reply(
        &mut self,
        device: DeviceId,
        command: &protocol::ThorlabsDcCommand,
    ) -> Result<()> {
        let bytes = self.serial.read_available()?;
        if bytes.is_empty() {
            return Ok(());
        }
        let lines = self.codec.push(&bytes);
        for line in lines {
            let trimmed = line.trim_start();
            if trimmed.starts_with("CMD") || trimmed.starts_with("E") {
                protocol::parse_last_error(&line)?;
                continue;
            }
            self.apply_readback_reply(device, command, &line)?;
        }
        Ok(())
    }

    fn refresh_property_readback(&mut self, device: DeviceId, key: &str) -> Result<()> {
        let query = self.query_for_property(device, key)?;
        self.send(query.clone())?;
        self.read_query_reply(device, &query)
    }

    fn refresh_keys_for(&self, device: DeviceId, command: &str) -> Result<Vec<&'static str>> {
        if self.channel_index(device).is_some() {
            return match command {
                "refresh_readbacks" => Ok(vec![
                    "enabled",
                    "limit_current",
                    "constant_current",
                    "brightness",
                    "maximum_current",
                    "wavelength",
                    "forward_bias",
                    "led_serial",
                ]),
                "refresh_output" => Ok(vec!["enabled"]),
                "refresh_setpoints" => Ok(vec!["limit_current", "constant_current", "brightness"]),
                "refresh_identity" => Ok(vec!["maximum_current", "wavelength", "forward_bias", "led_serial"]),
                other => Err(Error::new(
                    ErrorCode::Unsupported,
                    format!(
                        "Thorlabs DC channel GenericCommand supports refresh_readbacks, refresh_output, refresh_setpoints, and refresh_identity; got {other}"
                    ),
                )),
            };
        }
        if device != self.controller {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown Thorlabs DC device",
            ));
        }
        match command {
            "refresh_readbacks" => {
                let mut keys = vec![
                    "enabled",
                    "operation_mode",
                    "limit_current",
                    "constant_current",
                    "status",
                    "wavelength",
                    "forward_bias",
                    "firmware",
                    "led_serial",
                ];
                if matches!(
                    self.probe.family,
                    protocol::DeviceFamily::Dc2xxx | protocol::DeviceFamily::Dc2200Scpi
                ) {
                    keys.extend(["pwm_current", "pwm_frequency", "pwm_duty_cycle", "pwm_counts"]);
                }
                if self.probe.family == protocol::DeviceFamily::Dc3100 {
                    keys.extend([
                        "modulation_current",
                        "modulation_frequency",
                        "modulation_depth",
                        "maximum_frequency",
                    ]);
                }
                Ok(keys)
            }
            "refresh_output" => Ok(vec!["enabled", "operation_mode"]),
            "refresh_setpoints" => {
                let mut keys = vec!["limit_current", "constant_current"];
                if matches!(
                    self.probe.family,
                    protocol::DeviceFamily::Dc2xxx | protocol::DeviceFamily::Dc2200Scpi
                ) {
                    keys.extend(["pwm_current", "pwm_frequency", "pwm_duty_cycle", "pwm_counts"]);
                }
                if self.probe.family == protocol::DeviceFamily::Dc3100 {
                    keys.extend([
                        "modulation_current",
                        "modulation_frequency",
                        "modulation_depth",
                    ]);
                }
                Ok(keys)
            }
            "refresh_status" => Ok(vec!["status"]),
            "refresh_identity" => Ok(vec!["wavelength", "forward_bias", "firmware", "led_serial"]),
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "Thorlabs DC controller GenericCommand supports refresh_readbacks, refresh_output, refresh_setpoints, refresh_status, and refresh_identity; got {other}"
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
                "Thorlabs DC GenericCommand does not take parameters",
            ));
        }
        let _ = self.refresh_keys_for(device, &request.command)?;
        Ok(())
    }

    fn apply_generic_command(
        &mut self,
        device: DeviceId,
        request: GenericCommandRequest,
    ) -> Result<Value> {
        self.validate_generic_command(device, &request)?;
        let keys = self.refresh_keys_for(device, &request.command)?;
        let mut values = BTreeMap::new();
        for key in &keys {
            self.refresh_property_readback(device, key)?;
            values.insert((*key).into(), self.read_property(device, key)?);
        }
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("commands".into(), Value::I64(keys.len() as i64)),
            ("values".into(), Value::Map(values)),
            (
                "completion_basis".into(),
                Value::String("Thorlabs DC mapped query readback".into()),
            ),
        ])))
    }

    fn apply_readback_reply(
        &mut self,
        device: DeviceId,
        command: &protocol::ThorlabsDcCommand,
        reply: &str,
    ) -> Result<()> {
        match command {
            protocol::ThorlabsDcCommand::OutputQuery => {
                self.enabled = protocol::parse_bool(reply)?;
                self.emit_property(self.controller, "enabled", Value::Bool(self.enabled));
            }
            protocol::ThorlabsDcCommand::OperationModeQuery => {
                self.operation_mode = protocol::parse_operation_mode(self.probe.family, reply)?;
                self.emit_property(
                    self.controller,
                    "operation_mode",
                    Value::String(self.operation_mode.label().into()),
                );
            }
            protocol::ThorlabsDcCommand::LimitCurrentQuery => {
                self.limit_current = protocol::parse_current(self.probe.family, reply)?;
                self.emit_property(
                    self.controller,
                    "limit_current",
                    Value::ElectricCurrent(self.limit_current),
                );
            }
            protocol::ThorlabsDcCommand::ConstantCurrentQuery => {
                self.constant_current = protocol::parse_current(self.probe.family, reply)?;
                self.emit_property(
                    self.controller,
                    "constant_current",
                    Value::ElectricCurrent(self.constant_current),
                );
            }
            protocol::ThorlabsDcCommand::PwmCurrentQuery => {
                self.pwm_current = protocol::parse_current(self.probe.family, reply)?;
                self.emit_property(
                    self.controller,
                    "pwm_current",
                    Value::ElectricCurrent(self.pwm_current),
                );
            }
            protocol::ThorlabsDcCommand::PwmFrequencyQuery => {
                if let Some(hz) = protocol::parse_number(reply) {
                    self.pwm_frequency_hz = hz.round().max(0.0) as u32;
                    self.emit_property(self.controller, "pwm_frequency", frequency(hz));
                }
            }
            protocol::ThorlabsDcCommand::PwmDutyCycleQuery => {
                if let Some(percent) = protocol::parse_number(reply) {
                    self.pwm_duty_cycle_percent = percent.round().clamp(0.0, 100.0) as u8;
                    self.emit_property(
                        self.controller,
                        "pwm_duty_cycle",
                        percent_ratio(self.pwm_duty_cycle_percent),
                    );
                }
            }
            protocol::ThorlabsDcCommand::PwmCountsQuery => {
                if let Some(counts) = protocol::parse_number(reply) {
                    self.pwm_counts = counts.round().max(0.0) as u32;
                    self.emit_property(
                        self.controller,
                        "pwm_counts",
                        Value::I64(self.pwm_counts as i64),
                    );
                }
            }
            protocol::ThorlabsDcCommand::ModulationCurrentQuery => {
                self.modulation_current = protocol::parse_current(self.probe.family, reply)?;
                self.emit_property(
                    self.controller,
                    "modulation_current",
                    Value::ElectricCurrent(self.modulation_current),
                );
            }
            protocol::ThorlabsDcCommand::ModulationFrequencyQuery => {
                if let Some(hz) = protocol::parse_number(reply) {
                    self.modulation_frequency_hz = hz;
                    self.emit_property(self.controller, "modulation_frequency", frequency(hz));
                }
            }
            protocol::ThorlabsDcCommand::ModulationDepthQuery => {
                if let Some(percent) = protocol::parse_number(reply) {
                    self.modulation_depth_percent = percent.round().clamp(0.0, 100.0) as u8;
                    self.emit_property(
                        self.controller,
                        "modulation_depth",
                        percent_ratio(self.modulation_depth_percent),
                    );
                }
            }
            protocol::ThorlabsDcCommand::MaximumFrequencyQuery => {
                self.probe.maximum_frequency_hz = protocol::parse_number(reply);
                let value = self
                    .probe
                    .maximum_frequency_hz
                    .map(frequency)
                    .unwrap_or(Value::Null);
                self.emit_property(self.controller, "maximum_frequency", value);
            }
            protocol::ThorlabsDcCommand::StatusQuery => {
                self.status = protocol::parse_status(reply)?;
                self.emit_property(
                    self.controller,
                    "status",
                    Value::String(self.status.labels(self.probe.family).join(" ")),
                );
                self.emit_property(
                    self.controller,
                    "status_register",
                    Value::I64(self.status.0 as i64),
                );
            }
            protocol::ThorlabsDcCommand::Wavelength => {
                self.probe.wavelength =
                    protocol::parse_number(reply).map(Wavelength::from_nanometers);
                self.emit_property(
                    self.controller,
                    "wavelength",
                    self.probe
                        .wavelength
                        .map(Value::Wavelength)
                        .unwrap_or(Value::Null),
                );
            }
            protocol::ThorlabsDcCommand::ForwardBias => {
                self.probe.forward_bias_volts = protocol::parse_number(reply);
                self.emit_property(
                    self.controller,
                    "forward_bias",
                    self.probe
                        .forward_bias_volts
                        .map(voltage)
                        .unwrap_or(Value::Null),
                );
            }
            protocol::ThorlabsDcCommand::FirmwareRevision => {
                self.probe.firmware_revision = protocol::clean_reply(reply);
                self.emit_property(
                    self.controller,
                    "firmware",
                    Value::String(self.probe.firmware_revision.clone()),
                );
            }
            protocol::ThorlabsDcCommand::LedHeadSerialNumber => {
                self.probe.led_serial_number = protocol::clean_reply(reply);
                self.emit_property(
                    self.controller,
                    "led_serial",
                    Value::String(self.probe.led_serial_number.clone()),
                );
            }
            protocol::ThorlabsDcCommand::ChannelOutputQuery(channel) => {
                if let Some(index) = self.channel_index(device) {
                    self.channel_enabled[index] = protocol::parse_bool(reply)?;
                    self.enabled = self.channel_enabled.iter().any(|enabled| *enabled);
                    self.emit_property(device, "enabled", Value::Bool(self.channel_enabled[index]));
                    self.emit_property(self.controller, "enabled", Value::Bool(self.enabled));
                } else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        format!("channel reply {channel} has no channel device"),
                    ));
                }
            }
            protocol::ThorlabsDcCommand::ChannelLimitCurrentQuery(_) => {
                if let Some(index) = self.channel_index(device) {
                    self.channel_limit_current[index] =
                        protocol::parse_current(self.probe.family, reply)?;
                    self.emit_property(
                        device,
                        "limit_current",
                        Value::ElectricCurrent(self.channel_limit_current[index]),
                    );
                }
            }
            protocol::ThorlabsDcCommand::ChannelConstantCurrentQuery(_) => {
                if let Some(index) = self.channel_index(device) {
                    self.channel_constant_current[index] =
                        protocol::parse_current(self.probe.family, reply)?;
                    self.emit_property(
                        device,
                        "constant_current",
                        Value::ElectricCurrent(self.channel_constant_current[index]),
                    );
                }
            }
            protocol::ThorlabsDcCommand::ChannelBrightnessQuery(_) => {
                if let Some(index) = self.channel_index(device) {
                    let percent = protocol::parse_number(reply).unwrap_or(0.0);
                    self.channel_brightness_percent[index] =
                        percent.round().clamp(0.0, 100.0) as u8;
                    self.emit_property(
                        device,
                        "brightness",
                        percent_ratio(self.channel_brightness_percent[index]),
                    );
                }
            }
            protocol::ThorlabsDcCommand::ChannelMaximumCurrentQuery(_) => {
                if let Some(index) = self.channel_index(device) {
                    let current = protocol::parse_current(self.probe.family, reply)?;
                    if self.probe.channel_maximum_currents.len() <= index {
                        self.probe
                            .channel_maximum_currents
                            .resize(index + 1, self.probe.maximum_current);
                    }
                    self.probe.channel_maximum_currents[index] = current;
                    self.emit_property(device, "maximum_current", Value::ElectricCurrent(current));
                }
            }
            protocol::ThorlabsDcCommand::ChannelWavelength(_) => {
                if let Some(index) = self.channel_index(device) {
                    let wavelength = protocol::parse_number(reply).map(Wavelength::from_nanometers);
                    if let Some(wavelength) = wavelength {
                        if self.probe.channel_wavelengths.len() <= index {
                            self.probe
                                .channel_wavelengths
                                .resize(index + 1, Wavelength::from_nanometers(0.0));
                        }
                        self.probe.channel_wavelengths[index] = wavelength;
                        self.emit_property(device, "wavelength", Value::Wavelength(wavelength));
                    }
                }
            }
            protocol::ThorlabsDcCommand::ChannelForwardBias(_) => {
                if let Some(index) = self.channel_index(device) {
                    if let Some(volts) = protocol::parse_number(reply) {
                        if self.probe.channel_forward_bias_volts.len() <= index {
                            self.probe.channel_forward_bias_volts.resize(index + 1, 0.0);
                        }
                        self.probe.channel_forward_bias_volts[index] = volts;
                        self.emit_property(device, "forward_bias", voltage(volts));
                    }
                }
            }
            protocol::ThorlabsDcCommand::ChannelLedHeadSerialNumber(_) => {
                if let Some(index) = self.channel_index(device) {
                    let serial = protocol::clean_reply(reply);
                    if self.probe.channel_led_serial_numbers.len() <= index {
                        self.probe
                            .channel_led_serial_numbers
                            .resize(index + 1, String::new());
                    }
                    self.probe.channel_led_serial_numbers[index] = serial.clone();
                    self.emit_property(device, "led_serial", Value::String(serial));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn operation_modes(&self) -> &'static [protocol::OperationMode] {
        match self.probe.family {
            protocol::DeviceFamily::Dc2xxx => &[
                protocol::OperationMode::ConstantCurrent,
                protocol::OperationMode::Pwm,
                protocol::OperationMode::ExternalControl,
            ],
            protocol::DeviceFamily::Dc2200Scpi => &[
                protocol::OperationMode::ConstantCurrent,
                protocol::OperationMode::Pwm,
                protocol::OperationMode::ExternalControl,
            ],
            protocol::DeviceFamily::Dc3100 => &[
                protocol::OperationMode::ConstantCurrent,
                protocol::OperationMode::InternalModulation,
                protocol::OperationMode::ExternalControl,
            ],
            protocol::DeviceFamily::Dc4100 => &[
                protocol::OperationMode::ConstantCurrent,
                protocol::OperationMode::Brightness,
                protocol::OperationMode::ExternalControl,
            ],
        }
    }

    fn channel_index(&self, device: DeviceId) -> Option<usize> {
        self.channels
            .iter()
            .position(|candidate| *candidate == device)
    }

    fn set_limit_current_command(&self, current: ElectricCurrent) -> protocol::ThorlabsDcCommand {
        if self.probe.family.current_setpoint_uses_amps() {
            protocol::ThorlabsDcCommand::SetLimitCurrentAmps(current)
        } else {
            protocol::ThorlabsDcCommand::SetLimitCurrent(current)
        }
    }

    fn set_constant_current_command(
        &self,
        current: ElectricCurrent,
    ) -> protocol::ThorlabsDcCommand {
        if self.probe.family.current_setpoint_uses_amps() {
            protocol::ThorlabsDcCommand::SetConstantCurrentAmps(current)
        } else {
            protocol::ThorlabsDcCommand::SetConstantCurrent(current)
        }
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        let mut descriptors = vec![DeviceDescriptor {
            id: self.controller,
            driver: self.id,
            label: match self.probe.family {
                protocol::DeviceFamily::Dc2xxx => "thorlabs-dc-led".into(),
                protocol::DeviceFamily::Dc2200Scpi => "thorlabs-dc2200-led".into(),
                protocol::DeviceFamily::Dc3100 => "thorlabs-dc3100-led".into(),
                protocol::DeviceFamily::Dc4100 => "thorlabs-dc4100-hub".into(),
            },
            vendor: Some("Thorlabs".into()),
            model: Some(self.probe.model.clone()),
            serial: Some(self.probe.serial_number.clone()),
            kinds: vec![
                "led.controller".into(),
                "light.source".into(),
                "shutter".into(),
                "trigger.sink".into(),
            ],
            properties: vec![
                sequenceable_property("enabled", "LED output", ValueType::Bool, None, true, None),
                enum_property(
                    "operation_mode",
                    "Operation mode",
                    ValueType::String,
                    true,
                    self.operation_modes(),
                ),
                current_property(
                    "limit_current",
                    "Limit current",
                    true,
                    ElectricCurrent::from_milliamps(0.0),
                    self.probe.maximum_current,
                ),
                sequenceable_current_property(
                    "constant_current",
                    "Constant current",
                    true,
                    ElectricCurrent::from_milliamps(0.0),
                    self.limit_current,
                ),
                sequenceable_current_property(
                    "pwm_current",
                    "PWM current",
                    matches!(
                        self.probe.family,
                        protocol::DeviceFamily::Dc2xxx | protocol::DeviceFamily::Dc2200Scpi
                    ),
                    ElectricCurrent::from_milliamps(0.0),
                    self.limit_current,
                ),
                property(
                    "pwm_frequency",
                    "PWM frequency",
                    ValueType::Frequency,
                    Some("Hz"),
                    true,
                    Some(Range {
                        min: frequency(1.0),
                        max: frequency(10_000.0),
                    }),
                ),
                property(
                    "pwm_duty_cycle",
                    "PWM duty cycle",
                    ValueType::Ratio,
                    Some("percent"),
                    true,
                    Some(Range {
                        min: Value::Ratio(Ratio::from_percent(1.0)),
                        max: Value::Ratio(Ratio::from_percent(100.0)),
                    }),
                ),
                property(
                    "pwm_counts",
                    "PWM counts",
                    ValueType::I64,
                    None,
                    true,
                    Some(Range {
                        min: Value::I64(0),
                        max: Value::I64(100),
                    }),
                ),
                current_property(
                    "modulation_current",
                    "Modulation current",
                    self.probe.family == protocol::DeviceFamily::Dc3100,
                    ElectricCurrent::from_milliamps(0.0),
                    self.limit_current,
                ),
                property(
                    "modulation_frequency",
                    "Modulation frequency",
                    ValueType::Frequency,
                    Some("Hz"),
                    self.probe.family == protocol::DeviceFamily::Dc3100,
                    Some(Range {
                        min: frequency(0.0),
                        max: frequency(self.probe.maximum_frequency_hz.unwrap_or(100.0)),
                    }),
                ),
                property(
                    "modulation_depth",
                    "Modulation depth",
                    ValueType::Ratio,
                    Some("percent"),
                    self.probe.family == protocol::DeviceFamily::Dc3100,
                    Some(Range {
                        min: Value::Ratio(Ratio::from_percent(0.0)),
                        max: Value::Ratio(Ratio::from_percent(100.0)),
                    }),
                ),
                property(
                    "maximum_frequency",
                    "Maximum frequency",
                    ValueType::Frequency,
                    Some("Hz"),
                    false,
                    None,
                ),
                property("status", "Status", ValueType::String, None, false, None),
                property(
                    "status_register",
                    "Status register",
                    ValueType::I64,
                    None,
                    false,
                    None,
                ),
                property(
                    "wavelength",
                    "LED wavelength",
                    ValueType::Wavelength,
                    None,
                    false,
                    None,
                ),
                property(
                    "forward_bias",
                    "LED forward bias",
                    ValueType::Voltage,
                    Some("V"),
                    false,
                    None,
                ),
                property("firmware", "Firmware", ValueType::String, None, false, None),
                property(
                    "led_serial",
                    "LED serial number",
                    ValueType::String,
                    None,
                    false,
                    None,
                ),
            ],
            metadata: BTreeMap::from([
                (
                    "protocol".into(),
                    Value::String(format!(
                        "Thorlabs {} {}",
                        self.probe.family.model_family(),
                        if self.probe.family.is_scpi() {
                            "SCPI-style command set"
                        } else {
                            "serial"
                        }
                    )),
                ),
                (
                    "family".into(),
                    Value::String(self.probe.family.model_family().into()),
                ),
                (
                    "maximum_current".into(),
                    Value::ElectricCurrent(self.probe.maximum_current),
                ),
                (
                    "startup_readback_supported".into(),
                    Value::List(
                        protocol::probe_script(self.probe.family)
                            .into_iter()
                            .map(Value::String)
                            .collect(),
                    ),
                ),
            ]),
        }];

        if self.probe.family == protocol::DeviceFamily::Dc4100 {
            for index in 0..4 {
                descriptors.push(DeviceDescriptor {
                    id: self.channels[index],
                    driver: self.id,
                    label: format!("thorlabs-dc4100-led-{}", index + 1),
                    vendor: Some("Thorlabs".into()),
                    model: Some("DC4100 LED channel".into()),
                    serial: self.probe.channel_led_serial_numbers.get(index).cloned(),
                    kinds: vec![
                        "light.source".into(),
                        "led.channel".into(),
                        "trigger.sink".into(),
                    ],
                    properties: vec![
                        sequenceable_property(
                            "enabled",
                            "LED output",
                            ValueType::Bool,
                            None,
                            true,
                            None,
                        ),
                        current_property(
                            "limit_current",
                            "Limit current",
                            true,
                            ElectricCurrent::from_milliamps(0.0),
                            self.probe
                                .channel_maximum_currents
                                .get(index)
                                .copied()
                                .unwrap_or(self.probe.maximum_current),
                        ),
                        sequenceable_current_property(
                            "constant_current",
                            "Constant current",
                            true,
                            ElectricCurrent::from_milliamps(0.0),
                            self.channel_limit_current[index],
                        ),
                        sequenceable_property(
                            "brightness",
                            "Brightness",
                            ValueType::Ratio,
                            Some("percent"),
                            true,
                            Some(Range {
                                min: Value::Ratio(Ratio::from_percent(0.0)),
                                max: Value::Ratio(Ratio::from_percent(100.0)),
                            }),
                        ),
                        current_property(
                            "maximum_current",
                            "Maximum current",
                            false,
                            ElectricCurrent::from_milliamps(0.0),
                            self.probe
                                .channel_maximum_currents
                                .get(index)
                                .copied()
                                .unwrap_or(self.probe.maximum_current),
                        ),
                        property(
                            "wavelength",
                            "LED wavelength",
                            ValueType::Wavelength,
                            None,
                            false,
                            None,
                        ),
                        property(
                            "forward_bias",
                            "Forward bias",
                            ValueType::Voltage,
                            Some("V"),
                            false,
                            None,
                        ),
                        property(
                            "led_serial",
                            "LED serial number",
                            ValueType::String,
                            None,
                            false,
                            None,
                        ),
                    ],
                    metadata: BTreeMap::from([
                        ("channel_index".into(), Value::I64(index as i64)),
                        (
                            "wavelength".into(),
                            self.probe
                                .channel_wavelengths
                                .get(index)
                                .copied()
                                .map(Value::Wavelength)
                                .unwrap_or(Value::Null),
                        ),
                    ]),
                });
            }
        }

        descriptors
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        let key = thorlabs_dc_public_key(key);
        if let Some(index) = self.channel_index(device) {
            return match key {
                "enabled" => Ok(Value::Bool(self.channel_enabled[index])),
                "limit_current" => Ok(Value::ElectricCurrent(self.channel_limit_current[index])),
                "constant_current" => {
                    Ok(Value::ElectricCurrent(self.channel_constant_current[index]))
                }
                "brightness" => Ok(percent_ratio(self.channel_brightness_percent[index])),
                "maximum_current" => Ok(Value::ElectricCurrent(
                    self.probe
                        .channel_maximum_currents
                        .get(index)
                        .copied()
                        .unwrap_or(self.probe.maximum_current),
                )),
                "wavelength" => Ok(self
                    .probe
                    .channel_wavelengths
                    .get(index)
                    .copied()
                    .map(Value::Wavelength)
                    .unwrap_or(Value::Null)),
                "forward_bias" => Ok(self
                    .probe
                    .channel_forward_bias_volts
                    .get(index)
                    .copied()
                    .map(voltage)
                    .unwrap_or(Value::Null)),
                "led_serial" => Ok(Value::String(
                    self.probe
                        .channel_led_serial_numbers
                        .get(index)
                        .cloned()
                        .unwrap_or_default(),
                )),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown Thorlabs DC channel property {key}"),
                )),
            };
        }
        if device != self.controller {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown Thorlabs DC device",
            ));
        }
        match key {
            "enabled" => Ok(Value::Bool(self.enabled)),
            "operation_mode" => Ok(Value::String(self.operation_mode.label().into())),
            "limit_current" => Ok(Value::ElectricCurrent(self.limit_current)),
            "constant_current" => Ok(Value::ElectricCurrent(self.constant_current)),
            "pwm_current" => Ok(Value::ElectricCurrent(self.pwm_current)),
            "pwm_frequency" => Ok(frequency(self.pwm_frequency_hz as f64)),
            "pwm_duty_cycle" => Ok(percent_ratio(self.pwm_duty_cycle_percent)),
            "pwm_counts" => Ok(Value::I64(self.pwm_counts as i64)),
            "modulation_current" => Ok(Value::ElectricCurrent(self.modulation_current)),
            "modulation_frequency" => Ok(frequency(self.modulation_frequency_hz)),
            "modulation_depth" => Ok(percent_ratio(self.modulation_depth_percent)),
            "maximum_frequency" => Ok(self
                .probe
                .maximum_frequency_hz
                .map(frequency)
                .unwrap_or(Value::Null)),
            "status" => Ok(Value::String(
                self.status.labels(self.probe.family).join(" "),
            )),
            "status_register" => Ok(Value::I64(self.status.0 as i64)),
            "wavelength" => Ok(self
                .probe
                .wavelength
                .map(Value::Wavelength)
                .unwrap_or(Value::Null)),
            "forward_bias" => Ok(self
                .probe
                .forward_bias_volts
                .map(voltage)
                .unwrap_or(Value::Null)),
            "firmware" => Ok(Value::String(self.probe.firmware_revision.clone())),
            "led_serial" => Ok(Value::String(self.probe.led_serial_number.clone())),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Thorlabs DC property {key}"),
            )),
        }
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        let key = thorlabs_dc_public_key(key);
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
        let key = thorlabs_dc_public_key(key);
        self.validate_write(device, key, value)?;
        if let Some(index) = self.channel_index(device) {
            let channel = index as u8;
            return match (key, value) {
                ("enabled", Value::Bool(enabled)) => {
                    self.ensure_multi_selection()?;
                    self.send_checked(protocol::ThorlabsDcCommand::SetChannelOutput {
                        channel,
                        enabled: *enabled,
                    })?;
                    self.channel_enabled[index] = *enabled;
                    self.enabled = self.channel_enabled.iter().any(|channel| *channel);
                    self.refresh_property_readback(device, "enabled")?;
                    Ok(Value::Bool(*enabled))
                }
                ("limit_current", Value::ElectricCurrent(current)) => {
                    self.send_checked(protocol::ThorlabsDcCommand::SetChannelLimitCurrent {
                        channel,
                        current: *current,
                    })?;
                    self.channel_limit_current[index] = *current;
                    self.refresh_property_readback(device, "limit_current")?;
                    Ok(Value::ElectricCurrent(*current))
                }
                ("constant_current", Value::ElectricCurrent(current)) => {
                    self.send_checked(protocol::ThorlabsDcCommand::SetChannelConstantCurrent {
                        channel,
                        current: *current,
                    })?;
                    self.channel_constant_current[index] = *current;
                    self.refresh_property_readback(device, "constant_current")?;
                    Ok(Value::ElectricCurrent(*current))
                }
                ("brightness", Value::Ratio(value)) => {
                    let percent = ratio_percent_u8(*value, 0.0, 100.0);
                    self.send_checked(protocol::ThorlabsDcCommand::SetChannelBrightnessPercent {
                        channel,
                        percent,
                    })?;
                    self.channel_brightness_percent[index] = percent;
                    self.refresh_property_readback(device, "brightness")?;
                    Ok(percent_ratio(percent))
                }
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("invalid Thorlabs DC channel write {key}"),
                )),
            };
        }
        if device != self.controller {
            return Err(Error::new(ErrorCode::InvalidCommand, "unknown device"));
        }
        match (key, value) {
            ("enabled", Value::Bool(enabled)) => {
                if self.probe.family == protocol::DeviceFamily::Dc4100 {
                    self.ensure_multi_selection()?;
                    self.send_checked(protocol::ThorlabsDcCommand::SetAllChannelsOutput(*enabled))?;
                    self.channel_enabled = [*enabled; 4];
                } else {
                    self.send_checked(protocol::ThorlabsDcCommand::SetOutput(*enabled))?;
                }
                self.enabled = *enabled;
                self.refresh_property_readback(device, "enabled")?;
                Ok(Value::Bool(*enabled))
            }
            ("operation_mode", Value::String(mode)) => {
                let mode = parse_mode_label(mode)?;
                self.send_checked(protocol::ThorlabsDcCommand::SetOutput(false))?;
                self.send_checked(protocol::ThorlabsDcCommand::SetOperationMode(mode))?;
                self.enabled = false;
                self.operation_mode = mode;
                self.refresh_property_readback(device, "enabled")?;
                self.refresh_property_readback(device, "operation_mode")?;
                Ok(Value::String(mode.label().into()))
            }
            ("limit_current", Value::ElectricCurrent(current)) => {
                self.send_checked(self.set_limit_current_command(*current))?;
                self.limit_current = *current;
                self.refresh_property_readback(device, "limit_current")?;
                Ok(Value::ElectricCurrent(*current))
            }
            ("constant_current", Value::ElectricCurrent(current)) => {
                self.send_checked(self.set_constant_current_command(*current))?;
                self.constant_current = *current;
                self.refresh_property_readback(device, "constant_current")?;
                Ok(Value::ElectricCurrent(*current))
            }
            ("pwm_current", Value::ElectricCurrent(current)) => {
                self.send_checked(protocol::ThorlabsDcCommand::SetPwmCurrent(*current))?;
                self.pwm_current = *current;
                self.refresh_property_readback(device, "pwm_current")?;
                Ok(Value::ElectricCurrent(*current))
            }
            ("pwm_frequency", Value::Frequency(value)) => {
                let hz = value.hertz().round().clamp(1.0, 10_000.0) as u32;
                self.send_checked(protocol::ThorlabsDcCommand::SetPwmFrequencyHz(hz))?;
                self.pwm_frequency_hz = hz;
                self.refresh_property_readback(device, "pwm_frequency")?;
                Ok(frequency(hz as f64))
            }
            ("pwm_duty_cycle", Value::Ratio(value)) => {
                let percent = ratio_percent_u8(*value, 1.0, 100.0);
                self.send_checked(protocol::ThorlabsDcCommand::SetPwmDutyCyclePercent(percent))?;
                self.pwm_duty_cycle_percent = percent;
                self.refresh_property_readback(device, "pwm_duty_cycle")?;
                Ok(percent_ratio(percent))
            }
            ("pwm_counts", Value::I64(counts)) => {
                let counts = (*counts).clamp(0, 100) as u32;
                self.send_checked(protocol::ThorlabsDcCommand::SetPwmCounts(counts))?;
                self.pwm_counts = counts;
                self.refresh_property_readback(device, "pwm_counts")?;
                Ok(Value::I64(counts as i64))
            }
            ("modulation_current", Value::ElectricCurrent(current)) => {
                self.send_checked(protocol::ThorlabsDcCommand::SetModulationCurrentAmps(
                    *current,
                ))?;
                self.modulation_current = *current;
                self.refresh_property_readback(device, "modulation_current")?;
                Ok(Value::ElectricCurrent(*current))
            }
            ("modulation_frequency", Value::Frequency(value)) => {
                let max = self.probe.maximum_frequency_hz.unwrap_or(100.0);
                let hz = value.hertz().clamp(0.0, max);
                self.send_checked(protocol::ThorlabsDcCommand::SetModulationFrequencyHz(hz))?;
                self.modulation_frequency_hz = hz;
                self.refresh_property_readback(device, "modulation_frequency")?;
                Ok(frequency(hz))
            }
            ("modulation_depth", Value::Ratio(value)) => {
                let percent = ratio_percent_u8(*value, 0.0, 100.0);
                self.send_checked(protocol::ThorlabsDcCommand::SetModulationDepthPercent(
                    percent,
                ))?;
                self.modulation_depth_percent = percent;
                self.refresh_property_readback(device, "modulation_depth")?;
                Ok(percent_ratio(percent))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid Thorlabs DC write {key}"),
            )),
        }
    }

    fn ensure_multi_selection(&mut self) -> Result<()> {
        if self.probe.family != protocol::DeviceFamily::Dc4100 {
            return Ok(());
        }
        self.send(protocol::ThorlabsDcCommand::SelectionModeQuery)?;
        self.send_checked(protocol::ThorlabsDcCommand::SetMultiSelectionMode)
    }

    fn local_timing_participants(&self, plan: &TimingPlan) -> Vec<DeviceId> {
        plan.participants
            .iter()
            .copied()
            .filter(|device| *device == self.controller || self.channel_index(*device).is_some())
            .collect()
    }

    fn local_timing_routes(&self, plan: &TimingPlan) -> Vec<Value> {
        plan.routes
            .iter()
            .filter(|route| {
                route.from == self.controller
                    || route.to == self.controller
                    || self.channel_index(route.from).is_some()
                    || self.channel_index(route.to).is_some()
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
            .collect()
    }

    fn local_timing_sequences(&self, plan: &TimingPlan) -> Vec<Value> {
        plan.sequences
            .iter()
            .filter(|sequence| {
                sequence.device == self.controller || self.channel_index(sequence.device).is_some()
            })
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
            .filter(|sequence| {
                sequence.device == self.controller || self.channel_index(sequence.device).is_some()
            })
            .collect()
    }

    fn has_explicit_sequence(&self, plan: &TimingPlan, device: DeviceId, property: &str) -> bool {
        plan.sequences
            .iter()
            .any(|sequence| sequence.device == device && sequence.property == property)
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        let descriptors = self.descriptors_for();
        for sequence in self.local_timing_sequence_refs(plan) {
            if sequence.values.is_empty() {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "Thorlabs DC timing sequence must contain at least one value",
                ));
            }
            let property = thorlabs_dc_public_key(&sequence.property);
            match (sequence.device, property) {
                (device, "enabled" | "constant_current" | "pwm_current")
                    if device == self.controller => {}
                (device, "enabled" | "constant_current" | "brightness")
                    if self.channel_index(device).is_some() => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        format!(
                            "Thorlabs DC timing does not support {} on {:?}",
                            property, sequence.device
                        ),
                    ))
                }
            }
            let descriptor = descriptors
                .iter()
                .find(|descriptor| descriptor.id == sequence.device)
                .ok_or_else(|| {
                    Error::new(ErrorCode::InvalidCommand, "unknown Thorlabs DC device")
                })?;
            let schema = descriptor
                .properties
                .iter()
                .find(|schema| schema.key == property)
                .ok_or_else(|| {
                    Error::new(ErrorCode::InvalidProperty, "unknown Thorlabs DC property")
                })?;
            if !schema.sequenceable {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!(
                        "Thorlabs DC property {} is not sequenceable",
                        sequence.property
                    ),
                ));
            }
            for value in &sequence.values {
                schema.validate(value)?;
            }
        }
        Ok(())
    }

    fn apply_timing_sequence_step(&mut self, plan: &TimingPlan, start: bool) -> Result<Value> {
        let sequences = self
            .local_timing_sequence_refs(plan)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let mut applied = BTreeMap::new();
        for sequence in sequences {
            let value = (if start {
                sequence.values.first()
            } else {
                sequence.values.last()
            })
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidCommand,
                    "Thorlabs DC timing sequence must contain at least one value",
                )
            })?
            .clone();
            let applied_value = self.write_property(sequence.device, &sequence.property, &value)?;
            self.emit_property(sequence.device, &sequence.property, applied_value.clone());
            applied.insert(
                format!("{}:{}", sequence.device.0 .0, sequence.property),
                applied_value,
            );
        }
        Ok(Value::Map(applied))
    }

    fn timing_summary(&self, plan: &TimingPlan, action: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("action".into(), Value::String(action.into())),
            ("controller".into(), Value::I64(self.controller.0 .0 as i64)),
            (
                "model_family".into(),
                Value::String(self.probe.family.model_family().into()),
            ),
            (
                "participants".into(),
                Value::List(
                    self.local_timing_participants(plan)
                        .into_iter()
                        .map(|device| Value::I64(device.0 .0 as i64))
                        .collect(),
                ),
            ),
            ("routes".into(), Value::List(self.local_timing_routes(plan))),
            (
                "sequences".into(),
                Value::List(self.local_timing_sequences(plan)),
            ),
            (
                "operation_mode".into(),
                Value::String(self.operation_mode.label().into()),
            ),
            ("enabled".into(), Value::Bool(self.enabled)),
        ]))
    }

    fn timing_output_commands(
        &self,
        plan: &TimingPlan,
        enabled: bool,
    ) -> Vec<(DeviceId, String, protocol::ThorlabsDcCommand)> {
        let channel_participants = self
            .local_timing_participants(plan)
            .into_iter()
            .filter_map(|device| self.channel_index(device).map(|index| (device, index)))
            .collect::<Vec<_>>();
        if self.probe.family == protocol::DeviceFamily::Dc4100 && !channel_participants.is_empty() {
            channel_participants
                .into_iter()
                .filter(|(device, _)| !self.has_explicit_sequence(plan, *device, "enabled"))
                .map(|(device, index)| {
                    (
                        device,
                        "enabled".into(),
                        protocol::ThorlabsDcCommand::SetChannelOutput {
                            channel: index as u8,
                            enabled,
                        },
                    )
                })
                .collect()
        } else if self.probe.family == protocol::DeviceFamily::Dc4100
            && !self.has_explicit_sequence(plan, self.controller, "enabled")
        {
            vec![(
                self.controller,
                "enabled".into(),
                protocol::ThorlabsDcCommand::SetAllChannelsOutput(enabled),
            )]
        } else if !self.has_explicit_sequence(plan, self.controller, "enabled") {
            vec![(
                self.controller,
                "enabled".into(),
                protocol::ThorlabsDcCommand::SetOutput(enabled),
            )]
        } else {
            Vec::new()
        }
    }

    fn apply_timing_output_command(&mut self, device: DeviceId, enabled: bool) {
        if let Some(index) = self.channel_index(device) {
            self.channel_enabled[index] = enabled;
            self.enabled = self.channel_enabled.iter().any(|channel| *channel);
            self.emit_property(device, "enabled", Value::Bool(enabled));
        } else {
            self.enabled = enabled;
            if self.probe.family == protocol::DeviceFamily::Dc4100 {
                self.channel_enabled = [enabled; 4];
            }
            self.emit_property(self.controller, "enabled", Value::Bool(enabled));
        }
    }

    fn timing_transaction(
        &self,
        description: &str,
        command: &protocol::ThorlabsDcCommand,
    ) -> PhysicalTransaction {
        let line = protocol::encode(self.probe.family, command);
        PhysicalTransaction {
            resource: Some(self.resource),
            description: description.into(),
            payload: Value::Bytes(self.codec.encode(&line)),
        }
    }

    fn trigger_sink_commands(
        &self,
        device: DeviceId,
        request: &CapabilityRequest,
    ) -> Result<Vec<protocol::ThorlabsDcCommand>> {
        if device != self.controller && self.channel_index(device).is_none() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown Thorlabs DC trigger sink device",
            ));
        }
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
                    "Thorlabs DC TriggerSink expects None or CapabilityRequest::Trigger",
                ))
            }
        };
        let output_command = |enabled| {
            if let Some(index) = self.channel_index(device) {
                Ok(protocol::ThorlabsDcCommand::SetChannelOutput {
                    channel: index as u8,
                    enabled,
                })
            } else {
                Ok(self.output_command(enabled))
            }
        };
        Ok(match action {
            TriggerSinkAction::Enable => vec![output_command(true)?],
            TriggerSinkAction::Disable => vec![output_command(false)?],
            TriggerSinkAction::Pulse => vec![output_command(true)?, output_command(false)?],
        })
    }

    fn dac_commands(
        &self,
        device: DeviceId,
        request: &CapabilityRequest,
    ) -> Result<Vec<protocol::ThorlabsDcCommand>> {
        if let Some(index) = self.channel_index(device) {
            return match dac_channel_request(request)? {
                DacChannelRequest::Current(current) => Ok(vec![
                    protocol::ThorlabsDcCommand::SetChannelConstantCurrent {
                        channel: index as u8,
                        current,
                    },
                ]),
                DacChannelRequest::Brightness(percent) => Ok(vec![
                    protocol::ThorlabsDcCommand::SetChannelBrightnessPercent {
                        channel: index as u8,
                        percent,
                    },
                ]),
            };
        }
        if device != self.controller {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown Thorlabs DC Dac device",
            ));
        }
        if self.probe.family == protocol::DeviceFamily::Dc4100 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Thorlabs DC4100 Dac is exposed on LED channel devices",
            ));
        }
        Ok(match dac_controller_request(request)? {
            DacControllerRequest::ConstantCurrent(current) => {
                vec![self.set_constant_current_command(current)]
            }
        })
    }

    fn apply_dac(&mut self, device: DeviceId, request: CapabilityRequest) -> Result<Value> {
        if self.channel_index(device).is_some() {
            return match dac_channel_request(&request)? {
                DacChannelRequest::Current(current) => {
                    let value = self.write_property(
                        device,
                        "constant_current",
                        &Value::ElectricCurrent(current),
                    )?;
                    self.emit_property(device, "constant_current", value.clone());
                    Ok(Value::Map(BTreeMap::from([
                        ("constant_current".into(), value),
                        ("commands".into(), Value::I64(1)),
                    ])))
                }
                DacChannelRequest::Brightness(percent) => {
                    let value =
                        self.write_property(device, "brightness", &percent_ratio(percent))?;
                    self.emit_property(device, "brightness", value.clone());
                    Ok(Value::Map(BTreeMap::from([
                        ("brightness".into(), value),
                        ("commands".into(), Value::I64(1)),
                    ])))
                }
            };
        }
        match dac_controller_request(&request)? {
            DacControllerRequest::ConstantCurrent(current) => {
                let value = self.write_property(
                    device,
                    "constant_current",
                    &Value::ElectricCurrent(current),
                )?;
                self.emit_property(device, "constant_current", value.clone());
                Ok(Value::Map(BTreeMap::from([
                    ("constant_current".into(), value),
                    ("commands".into(), Value::I64(1)),
                ])))
            }
        }
    }

    fn output_command(&self, enabled: bool) -> protocol::ThorlabsDcCommand {
        if self.probe.family == protocol::DeviceFamily::Dc4100 {
            protocol::ThorlabsDcCommand::SetAllChannelsOutput(enabled)
        } else {
            protocol::ThorlabsDcCommand::SetOutput(enabled)
        }
    }

    fn apply_output_command_state(&mut self, enabled: bool) {
        self.enabled = enabled;
        if self.probe.family == protocol::DeviceFamily::Dc4100 {
            self.channel_enabled = [enabled; 4];
        }
        self.emit_property(self.controller, "enabled", Value::Bool(enabled));
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

impl Driver for ThorlabsDcDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        self.descriptors_for()
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        let mut metadata = BTreeMap::from([
            ("send_terminator".into(), Value::String("CRLF".into())),
            ("recv_terminator".into(), Value::String("CRLF".into())),
            ("status_recv_terminator".into(), Value::String("LF".into())),
            (
                "completion".into(),
                Value::String("write accepted plus e? hardware error query".into()),
            ),
            (
                "startup_readback_supported".into(),
                Value::List(
                    protocol::probe_script(self.probe.family)
                        .into_iter()
                        .map(Value::String)
                        .collect(),
                ),
            ),
        ]);
        metadata.extend(self.endpoint_metadata.clone());
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: self.resource_label.clone(),
            kind: self.resource_kind.clone(),
            metadata,
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.controller {
            let mut capabilities = vec![capability(1, device, CapabilityKind::TriggerSink)];
            if self.probe.family != protocol::DeviceFamily::Dc4100 {
                capabilities.push(capability(2, device, CapabilityKind::Dac));
            }
            capabilities.push(capability(3, device, CapabilityKind::GenericCommand));
            capabilities
        } else if self.channel_index(device).is_some() {
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
                        description: format!("thorlabs-dc read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("thorlabs-dc write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "thorlabs-dc remultiplexed LED state set".into(),
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
                            "unknown Thorlabs DC capability",
                        ));
                    };
                    if !capability.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "Thorlabs DC {:?} expects {:?}, got {:?}",
                                capability.kind,
                                capability.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    match capability.kind {
                        CapabilityKind::TriggerSink => {
                            for command in self.trigger_sink_commands(*device, request)? {
                                physical_transactions.push(self.timing_transaction(
                                    "thorlabs-dc trigger sink output command",
                                    &command,
                                ));
                            }
                        }
                        CapabilityKind::Dac => {
                            for command in self.dac_commands(*device, request)? {
                                physical_transactions.push(
                                    self.timing_transaction("thorlabs-dc dac setpoint", &command),
                                );
                            }
                        }
                        CapabilityKind::GenericCommand => {
                            let CapabilityRequest::GenericCommand(request) = request else {
                                return Err(Error::new(
                                    ErrorCode::InvalidCommand,
                                    "Thorlabs DC GenericCommand expects GenericCommandRequest",
                                ));
                            };
                            self.validate_generic_command(*device, request)?;
                            physical_transactions.push(PhysicalTransaction {
                                resource: Some(self.resource),
                                description: format!(
                                    "thorlabs-dc mapped refresh {}",
                                    request.command
                                ),
                                payload: Value::List(
                                    self.refresh_keys_for(*device, &request.command)?
                                        .into_iter()
                                        .map(|key| Value::String(key.into()))
                                        .collect(),
                                ),
                            });
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported Thorlabs DC invocation",
                            ))
                        }
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
                    let query = self.query_for_property(device, &key)?;
                    self.send(query.clone())?;
                    self.read_query_reply(device, &query)?;
                    last = self.read_property(device, &key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    last = self.write_property(device, &key, &value)?;
                    self.emit_property(device, &key, last.clone());
                }
                Command::ApplyStateSet(set) => {
                    let mut deferred_enable = Vec::new();
                    let mut changed = BTreeMap::new();
                    for write in set.writes {
                        if write.property == "enabled" {
                            deferred_enable.push(write);
                            continue;
                        }
                        let value =
                            self.write_property(write.device, &write.property, &write.value)?;
                        self.emit_property(write.device, &write.property, value.clone());
                        changed.insert(format!("{}:{}", (write.device.0).0, write.property), value);
                    }
                    for write in deferred_enable {
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
                            "unknown Thorlabs DC capability",
                        ));
                    };
                    if !capability.accepts_request(&request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "Thorlabs DC {:?} expects {:?}, got {:?}",
                                capability.kind,
                                capability.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    last = match capability.kind {
                        CapabilityKind::TriggerSink => {
                            let commands = self.trigger_sink_commands(device, &request)?;
                            if self.probe.family == protocol::DeviceFamily::Dc4100 {
                                self.ensure_multi_selection()?;
                            }
                            for command in &commands {
                                self.send_checked(command.clone())?;
                            }
                            let enabled = commands
                                .last()
                                .and_then(output_command_enabled)
                                .unwrap_or(self.enabled);
                            if let Some(index) = self.channel_index(device) {
                                self.channel_enabled[index] = enabled;
                                self.enabled = self.channel_enabled.iter().any(|channel| *channel);
                                self.emit_property(device, "enabled", Value::Bool(enabled));
                                self.emit_property(
                                    self.controller,
                                    "enabled",
                                    Value::Bool(self.enabled),
                                );
                            } else {
                                self.apply_output_command_state(enabled);
                            }
                            Value::Map(BTreeMap::from([
                                ("triggered".into(), Value::Bool(true)),
                                ("enabled".into(), Value::Bool(enabled)),
                                ("commands".into(), Value::I64(commands.len() as i64)),
                            ]))
                        }
                        CapabilityKind::Dac => self.apply_dac(device, request)?,
                        CapabilityKind::GenericCommand => {
                            let CapabilityRequest::GenericCommand(request) = request else {
                                return Err(Error::new(
                                    ErrorCode::InvalidCommand,
                                    "Thorlabs DC GenericCommand expects GenericCommandRequest",
                                ));
                            };
                            self.apply_generic_command(device, request)?
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported Thorlabs DC invocation",
                            ))
                        }
                    };
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
                        message: format!("thorlabs-dc serial: {line}"),
                    })));
            }
            for line in self.status_codec.push(&[]) {
                self.pending
                    .push_back(DriverEvent::Event(Event::Log(LogEvent {
                        driver: Some(self.id),
                        message: format!("thorlabs-dc status serial: {line}"),
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
                description: "thorlabs-dc timing arm summary".into(),
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
        let commands = self.timing_output_commands(&armed.plan, true);
        let mut transactions = Vec::new();
        if self.probe.family == protocol::DeviceFamily::Dc4100 {
            self.ensure_multi_selection()?;
        }
        for (device, _, command) in &commands {
            self.send_checked(command.clone())?;
            self.apply_timing_output_command(*device, true);
            transactions
                .push(self.timing_transaction("thorlabs-dc timing start output enable", command));
        }
        transactions.push(PhysicalTransaction {
            resource: Some(self.resource),
            description: "thorlabs-dc timing start summary".into(),
            payload: with_applied(self.timing_summary(&armed.plan, "start"), applied),
        });
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Start(OperationId(0))],
            physical_transactions: transactions,
        })
    }

    fn stop_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let applied = self.apply_timing_sequence_step(&armed.plan, false)?;
        let commands = self.timing_output_commands(&armed.plan, false);
        let mut transactions = Vec::new();
        if self.probe.family == protocol::DeviceFamily::Dc4100 {
            self.ensure_multi_selection()?;
        }
        for (device, _, command) in &commands {
            self.send_checked(command.clone())?;
            self.apply_timing_output_command(*device, false);
            transactions
                .push(self.timing_transaction("thorlabs-dc timing stop output disable", command));
        }
        transactions.push(PhysicalTransaction {
            resource: Some(self.resource),
            description: "thorlabs-dc timing stop summary".into(),
            payload: with_applied(self.timing_summary(&armed.plan, "stop"), applied),
        });
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions: transactions,
        })
    }
}

fn parse_mode_label(label: &str) -> Result<protocol::OperationMode> {
    match label {
        "Constant Current" => Ok(protocol::OperationMode::ConstantCurrent),
        "PWM" => Ok(protocol::OperationMode::Pwm),
        "Internal Modulation" => Ok(protocol::OperationMode::InternalModulation),
        "Brightness Mode" => Ok(protocol::OperationMode::Brightness),
        "External Control" => Ok(protocol::OperationMode::ExternalControl),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unknown Thorlabs DC operation mode: {other}"),
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TriggerSinkAction {
    Enable,
    Disable,
    Pulse,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DacControllerRequest {
    ConstantCurrent(ElectricCurrent),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DacChannelRequest {
    Current(ElectricCurrent),
    Brightness(u8),
}

fn dac_controller_request(request: &CapabilityRequest) -> Result<DacControllerRequest> {
    match request {
        CapabilityRequest::Dac(request) => Ok(DacControllerRequest::ConstantCurrent(
            current_dac_value(&request.value)?,
        )),
        _ => Err(Error::new(
            ErrorCode::Unsupported,
            "Thorlabs DC Dac expects CapabilityRequest::Dac",
        )),
    }
}

fn dac_channel_request(request: &CapabilityRequest) -> Result<DacChannelRequest> {
    match request {
        CapabilityRequest::Dac(request) => dac_channel_value(&request.value),
        _ => Err(Error::new(
            ErrorCode::Unsupported,
            "Thorlabs DC channel Dac expects CapabilityRequest::Dac",
        )),
    }
}

fn current_dac_value(value: &Value) -> Result<ElectricCurrent> {
    match value {
        Value::ElectricCurrent(current) => Ok(*current),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            "Thorlabs DC controller Dac value must be ElectricCurrent",
        )),
    }
}

fn dac_channel_value(value: &Value) -> Result<DacChannelRequest> {
    match value {
        Value::ElectricCurrent(current) => Ok(DacChannelRequest::Current(*current)),
        Value::Ratio(value) => Ok(DacChannelRequest::Brightness(ratio_percent_u8(
            *value, 0.0, 100.0,
        ))),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            "Thorlabs DC channel Dac value must be ElectricCurrent or Ratio percent",
        )),
    }
}

fn output_command_enabled(command: &protocol::ThorlabsDcCommand) -> Option<bool> {
    match command {
        protocol::ThorlabsDcCommand::SetOutput(enabled)
        | protocol::ThorlabsDcCommand::SetAllChannelsOutput(enabled) => Some(*enabled),
        protocol::ThorlabsDcCommand::SetChannelOutput { enabled, .. } => Some(*enabled),
        _ => None,
    }
}

fn capability(id: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    CapabilityDescriptor::new(CapabilityId(id), device, kind, ValueType::Map)
}

fn thorlabs_dc_public_key(key: &str) -> &str {
    match key {
        "brightness_percent" => "brightness",
        "pwm_frequency_hz" => "pwm_frequency",
        "modulation_frequency_hz" => "modulation_frequency",
        "maximum_frequency_hz" => "maximum_frequency",
        other => other,
    }
}

fn thorlabs_dc_family_from_label(label: &str) -> Result<protocol::DeviceFamily> {
    match label.trim().to_ascii_lowercase().as_str() {
        "dc2010" | "dc2100" | "dc2xxx" | "dc2010/dc2100" => Ok(protocol::DeviceFamily::Dc2xxx),
        "dc2200" | "dc2200_scpi" | "dc2200 scpi" | "dc2200 scpi/usbtmc" => {
            Ok(protocol::DeviceFamily::Dc2200Scpi)
        }
        "dc3100" => Ok(protocol::DeviceFamily::Dc3100),
        "dc4100" | "dc4104" | "dc4100/dc4104" | "ledd4" => Ok(protocol::DeviceFamily::Dc4100),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unknown Thorlabs DC family {other}"),
        )),
    }
}

fn usb_tmc_endpoint(device: &DeviceConfig) -> Result<Option<ThorlabsDcUsbTmcEndpoint>> {
    let has_usb_config = device.properties.contains_key("usb_tmc")
        || device.properties.contains_key("vendor_id")
        || device.properties.contains_key("product_id")
        || device.properties.contains_key("bulk_out_endpoint")
        || device.properties.contains_key("bulk_in_endpoint");
    if !has_usb_config {
        return Ok(None);
    }
    if matches!(device.properties.get("usb_tmc"), Some(Value::Bool(false))) {
        return Ok(None);
    }
    let vendor_id = required_u16_prop(device, "vendor_id")?;
    let product_id = required_u16_prop(device, "product_id")?;
    let interface = u8_prop(device, "interface")?.unwrap_or(0);
    let bulk_out_endpoint = required_u8_prop(device, "bulk_out_endpoint")?;
    let bulk_in_endpoint = required_u8_prop(device, "bulk_in_endpoint")?;
    let read_size = u64_prop(device, "read_size")
        .unwrap_or(4096)
        .clamp(64, 1_048_576) as usize;
    Ok(Some(ThorlabsDcUsbTmcEndpoint {
        vendor_id,
        product_id,
        interface,
        bulk_out_endpoint,
        bulk_in_endpoint,
        read_size,
    }))
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

fn wavelength_config(device: &DeviceConfig, key: &str, legacy_key: &str) -> Option<Wavelength> {
    match device.properties.get(key) {
        Some(Value::Wavelength(value)) => Some(*value),
        _ => f64_prop(device, legacy_key).map(Wavelength::from_nanometers),
    }
}

fn voltage_config(device: &DeviceConfig, key: &str, legacy_key: &str) -> Option<Voltage> {
    match device.properties.get(key) {
        Some(Value::Voltage(value)) => Some(*value),
        _ => f64_prop(device, legacy_key).map(Voltage::from_volts),
    }
}

fn electric_current_config(
    device: &DeviceConfig,
    key: &str,
    legacy_key: &str,
) -> Option<ElectricCurrent> {
    match device.properties.get(key) {
        Some(Value::ElectricCurrent(value)) => Some(*value),
        _ => f64_prop(device, legacy_key).map(ElectricCurrent::from_milliamps),
    }
}

fn frequency_config(device: &DeviceConfig, key: &str, legacy_key: &str) -> Option<Frequency> {
    match device.properties.get(key) {
        Some(Value::Frequency(value)) => Some(*value),
        _ => f64_prop(device, legacy_key).map(Frequency::from_hertz),
    }
}

fn wavelength_list_prop(device: &DeviceConfig, key: &str) -> Option<Vec<Wavelength>> {
    match device.properties.get(key) {
        Some(Value::List(values)) => {
            let mut wavelengths = Vec::new();
            for value in values {
                match value {
                    Value::Wavelength(wavelength) => wavelengths.push(*wavelength),
                    _ => return None,
                }
            }
            Some(wavelengths)
        }
        _ => None,
    }
}

fn electric_current_list_prop(device: &DeviceConfig, key: &str) -> Option<Vec<ElectricCurrent>> {
    match device.properties.get(key) {
        Some(Value::List(values)) => {
            let mut currents = Vec::new();
            for value in values {
                match value {
                    Value::ElectricCurrent(current) => currents.push(*current),
                    _ => return None,
                }
            }
            Some(currents)
        }
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

fn u32_prop(device: &DeviceConfig, key: &str) -> Option<u32> {
    u64_prop(device, key).and_then(|value| value.try_into().ok())
}

fn u16_prop(device: &DeviceConfig, key: &str) -> Result<Option<u16>> {
    match u64_prop(device, key) {
        Some(value) => value.try_into().map(Some).map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("Thorlabs DC property {key} must fit in u16"),
            )
        }),
        None => Ok(None),
    }
}

fn u8_prop(device: &DeviceConfig, key: &str) -> Result<Option<u8>> {
    match u64_prop(device, key) {
        Some(value) => value.try_into().map(Some).map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("Thorlabs DC property {key} must fit in u8"),
            )
        }),
        None => Ok(None),
    }
}

fn required_u16_prop(device: &DeviceConfig, key: &str) -> Result<u16> {
    u16_prop(device, key)?.ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidProperty,
            format!("Thorlabs DC USBTMC config requires property.{key}"),
        )
    })
}

fn required_u8_prop(device: &DeviceConfig, key: &str) -> Result<u8> {
    u8_prop(device, key)?.ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidProperty,
            format!("Thorlabs DC USBTMC config requires property.{key}"),
        )
    })
}

fn u64_prop(device: &DeviceConfig, key: &str) -> Option<u64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => (*value >= 0).then_some(*value as u64),
        Some(Value::F64(value)) if value.is_finite() && *value >= 0.0 => Some(*value as u64),
        _ => None,
    }
}

fn percent_ratio(percent: u8) -> Value {
    Value::Ratio(Ratio::from_percent(percent as f64))
}

fn ratio_percent_u8(value: Ratio, min: f64, max: f64) -> u8 {
    value.percent().round().clamp(min, max) as u8
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
    let mut property = property(key, display_name, value_type, unit, writable, range);
    property.sequenceable = true;
    property
}

fn current_property(
    key: &str,
    display_name: &str,
    writable: bool,
    min: ElectricCurrent,
    max: ElectricCurrent,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::ElectricCurrent,
        None,
        writable,
        Some(Range {
            min: Value::ElectricCurrent(min),
            max: Value::ElectricCurrent(max),
        }),
    )
}

fn sequenceable_current_property(
    key: &str,
    display_name: &str,
    writable: bool,
    min: ElectricCurrent,
    max: ElectricCurrent,
) -> PropertySchema {
    let mut property = current_property(key, display_name, writable, min, max);
    property.sequenceable = true;
    property
}

fn with_applied(mut summary: Value, applied: Value) -> Value {
    if let Value::Map(map) = &mut summary {
        map.insert("applied".into(), applied);
    }
    summary
}

fn voltage(volts: f64) -> Value {
    Value::Voltage(Voltage::from_volts(volts))
}

fn frequency(hertz: f64) -> Value {
    Value::Frequency(Frequency::from_hertz(hertz))
}

fn enum_property(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    writable: bool,
    modes: &[protocol::OperationMode],
) -> PropertySchema {
    let mut schema = property(key, display_name, value_type, None, writable, None);
    schema.enum_values = modes
        .iter()
        .map(|mode| EnumValue {
            value: Value::String(mode.label().into()),
            label: mode.label().into(),
        })
        .collect();
    schema
}

#[cfg(feature = "os-usb")]
mod live_thorlabs_dc_usbtmc {
    use super::*;
    use futures_lite::future::block_on;
    use nusb::transfer::RequestBuffer;
    use nusb::Interface;

    const MSG_DEV_DEP_OUT: u8 = 1;
    const MSG_REQUEST_DEV_DEP_IN: u8 = 2;
    const MSG_DEV_DEP_IN: u8 = 2;
    const ATTR_EOM: u8 = 0x01;
    const HEADER_LEN: usize = 12;

    pub struct LiveUsbTmc {
        iface: Interface,
        endpoint: ThorlabsDcUsbTmcEndpoint,
        tag: u8,
    }

    impl LiveUsbTmc {
        pub fn open(endpoint: &ThorlabsDcUsbTmcEndpoint) -> Result<Self> {
            let device = nusb::list_devices()
                .map_err(|error| usb_error(error.to_string()))?
                .find(|device| {
                    device.vendor_id() == endpoint.vendor_id
                        && device.product_id() == endpoint.product_id
                })
                .ok_or_else(|| {
                    usb_error(format!(
                        "no Thorlabs DC USBTMC device found for {:04x}:{:04x}",
                        endpoint.vendor_id, endpoint.product_id
                    ))
                })?;
            let device = device.open().map_err(|error| {
                usb_error(format!(
                    "open Thorlabs DC USBTMC {:04x}:{:04x} failed: {error}",
                    endpoint.vendor_id, endpoint.product_id
                ))
            })?;
            let iface = device
                .detach_and_claim_interface(endpoint.interface)
                .map_err(|error| {
                    usb_error(format!(
                        "claim Thorlabs DC USBTMC interface {} failed: {error}",
                        endpoint.interface
                    ))
                })?;
            Ok(Self {
                iface,
                endpoint: endpoint.clone(),
                tag: 1,
            })
        }

        fn next_tag(&mut self) -> u8 {
            let tag = self.tag;
            self.tag = self.tag.wrapping_add(1);
            if self.tag == 0 {
                self.tag = 1;
            }
            tag
        }

        fn bulk_out(&mut self, bytes: Vec<u8>) -> Result<()> {
            block_on(self.iface.bulk_out(self.endpoint.bulk_out_endpoint, bytes))
                .into_result()
                .map(|_| ())
                .map_err(|error| usb_error(format!("Thorlabs DC USBTMC write failed: {error}")))
        }

        fn bulk_in(&mut self, len: usize) -> Result<Vec<u8>> {
            block_on(
                self.iface
                    .bulk_in(self.endpoint.bulk_in_endpoint, RequestBuffer::new(len)),
            )
            .into_result()
            .map_err(|error| usb_error(format!("Thorlabs DC USBTMC read failed: {error}")))
        }
    }

    impl SerialIo for LiveUsbTmc {
        fn write(&mut self, bytes: &[u8]) -> Result<()> {
            let tag = self.next_tag();
            let mut message = Vec::with_capacity(HEADER_LEN + bytes.len() + 3);
            message.extend([
                MSG_DEV_DEP_OUT,
                tag,
                !tag,
                0,
                (bytes.len() & 0xff) as u8,
                ((bytes.len() >> 8) & 0xff) as u8,
                ((bytes.len() >> 16) & 0xff) as u8,
                ((bytes.len() >> 24) & 0xff) as u8,
                ATTR_EOM,
                0,
                0,
                0,
            ]);
            message.extend(bytes);
            while message.len() % 4 != 0 {
                message.push(0);
            }
            self.bulk_out(message)
        }

        fn read_available(&mut self) -> Result<Vec<u8>> {
            let tag = self.next_tag();
            let request_len = self.endpoint.read_size as u32;
            self.bulk_out(vec![
                MSG_REQUEST_DEV_DEP_IN,
                tag,
                !tag,
                0,
                (request_len & 0xff) as u8,
                ((request_len >> 8) & 0xff) as u8,
                ((request_len >> 16) & 0xff) as u8,
                ((request_len >> 24) & 0xff) as u8,
                0,
                0,
                0,
                0,
            ])?;
            let mut packet = self.bulk_in(HEADER_LEN + self.endpoint.read_size + 3)?;
            if packet.len() < HEADER_LEN {
                return Err(usb_error("short Thorlabs DC USBTMC response header"));
            }
            if packet[0] != MSG_DEV_DEP_IN {
                return Err(usb_error(format!(
                    "unexpected Thorlabs DC USBTMC response MsgID {}",
                    packet[0]
                )));
            }
            if packet[1] != tag || packet[2] != !tag {
                return Err(usb_error("Thorlabs DC USBTMC response tag mismatch"));
            }
            let declared =
                u32::from_le_bytes([packet[4], packet[5], packet[6], packet[7]]) as usize;
            packet.drain(..HEADER_LEN);
            packet.truncate(declared.min(packet.len()));
            Ok(packet)
        }
    }

    fn usb_error(message: impl Into<String>) -> Error {
        Error::new(ErrorCode::Transport, message.into())
    }
}
