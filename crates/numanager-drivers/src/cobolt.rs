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

    pub const BAUD_HIGH: u32 = 115_200;
    pub const BAUD_FALLBACK: u32 = 19_200;
    pub const SEND_ENDING: LineEnding = LineEnding::Cr;
    pub const RECV_ENDING: LineEnding = LineEnding::CrLf;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ControlMode {
        ConstantPower,
        ConstantCurrent,
        Modulation,
    }

    impl ControlMode {
        pub fn label(self) -> &'static str {
            match self {
                ControlMode::ConstantPower => "Constant Power",
                ControlMode::ConstantCurrent => "Constant Current",
                ControlMode::Modulation => "Modulation",
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct CoboltProbe {
        pub model: String,
        pub serial_number: String,
        pub firmware_version: String,
        pub wavelength: Wavelength,
        pub max_power: OpticalPower,
        pub max_current: ElectricCurrent,
    }

    impl CoboltProbe {
        pub fn simulated() -> Self {
            Self {
                model: "Cobolt 06-MLD 488".into(),
                serial_number: "COBOLT-SIM-001".into(),
                firmware_version: "1.0-sim".into(),
                wavelength: Wavelength::from_nanometers(488.0),
                max_power: OpticalPower::from_milliwatts(120.0),
                max_current: ElectricCurrent::from_milliamps(500.0),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct CoboltProbeResult {
        pub probe: CoboltProbe,
        pub selected: bool,
        pub enabled: bool,
        pub power_setpoint: OpticalPower,
        pub actual_power: OpticalPower,
        pub max_power: OpticalPower,
        pub current: ElectricCurrent,
        pub max_current: ElectricCurrent,
        pub control_mode: ControlMode,
        pub interlock_closed: bool,
        pub fault: String,
        pub autostart: bool,
        pub hours: TimeInterval,
        pub replies: Vec<(String, String)>,
    }

    impl CoboltProbeResult {
        pub fn from_replies(replies: &[(impl AsRef<str>, impl AsRef<str>)]) -> Result<Self> {
            let mut probe = CoboltProbe::simulated();
            let mut selected = false;
            let mut enabled = false;
            let mut power_setpoint = OpticalPower::from_milliwatts(0.0);
            let mut actual_power = OpticalPower::from_milliwatts(0.0);
            let mut max_power = probe.max_power;
            let mut current = ElectricCurrent::from_milliamps(0.0);
            let mut max_current = probe.max_current;
            let mut control_mode = ControlMode::ConstantPower;
            let mut interlock_closed = true;
            let mut fault = "No Fault".into();
            let mut autostart = false;
            let mut hours = TimeInterval::from_hours(0.0);
            let mut stored = Vec::new();

            for (command, reply) in replies {
                let command = command.as_ref();
                let reply = reply.as_ref().trim();
                stored.push((command.to_string(), reply.to_string()));
                match command {
                    "@cob0" => {
                        parse_ok(reply)?;
                        selected = true;
                    }
                    "sn?" => probe.serial_number = reply.to_string(),
                    "glm?" => {
                        probe.model = reply.to_string();
                        if let Some(wavelength) = parse_wavelength_from_model(reply) {
                            probe.wavelength = wavelength;
                        }
                    }
                    "ver?" => probe.firmware_version = reply.to_string(),
                    "hrs?" => hours = TimeInterval::from_hours(parse_number("hrs?", reply)?),
                    "l?" => enabled = parse_bool(reply)?,
                    "p?" => power_setpoint = parse_power_watts(reply)?,
                    "pa?" => actual_power = parse_power_watts(reply)?,
                    "gmlp?" => {
                        max_power = parse_power_watts(reply)?;
                        probe.max_power = max_power;
                    }
                    "i?" => current = parse_current_milliamps(reply)?,
                    "gmlc?" => {
                        max_current = parse_current_milliamps(reply)?;
                        probe.max_current = max_current;
                    }
                    "gom?" => control_mode = parse_control_mode(reply)?,
                    "ilk?" => interlock_closed = parse_bool(reply)?,
                    "f?" => fault = parse_fault(reply),
                    "@cobas?" => autostart = parse_bool(reply)?,
                    _ => {}
                }
            }

            Ok(Self {
                probe,
                selected,
                enabled,
                power_setpoint,
                actual_power,
                max_power,
                current,
                max_current,
                control_mode,
                interlock_closed,
                fault,
                autostart,
                hours,
                replies: stored,
            })
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum CoboltCommand {
        Select,
        LaserOff,
        LaserOn,
        LaserQuery,
        SerialNumber,
        Model,
        Version,
        Hours,
        SetPower(OpticalPower),
        PowerSetpointQuery,
        ActualPowerQuery,
        MaxPowerQuery,
        SetCurrent(ElectricCurrent),
        CurrentQuery,
        MaxCurrentQuery,
        SetControlMode(ControlMode),
        OperatingModeQuery,
        InterlockQuery,
        FaultQuery,
        AutostartQuery,
        SetAutostart(bool),
    }

    pub fn encode(command: &CoboltCommand) -> String {
        match command {
            CoboltCommand::Select => "@cob0".into(),
            CoboltCommand::LaserOff => "l0".into(),
            CoboltCommand::LaserOn => "l1".into(),
            CoboltCommand::LaserQuery => "l?".into(),
            CoboltCommand::SerialNumber => "sn?".into(),
            CoboltCommand::Model => "glm?".into(),
            CoboltCommand::Version => "ver?".into(),
            CoboltCommand::Hours => "hrs?".into(),
            CoboltCommand::SetPower(power) => format!("p {:.6}", power.watts()),
            CoboltCommand::PowerSetpointQuery => "p?".into(),
            CoboltCommand::ActualPowerQuery => "pa?".into(),
            CoboltCommand::MaxPowerQuery => "gmlp?".into(),
            CoboltCommand::SetCurrent(current) => format!("slc {:.3}", current.milliamps()),
            CoboltCommand::CurrentQuery => "i?".into(),
            CoboltCommand::MaxCurrentQuery => "gmlc?".into(),
            CoboltCommand::SetControlMode(ControlMode::ConstantPower) => "cp".into(),
            CoboltCommand::SetControlMode(ControlMode::ConstantCurrent) => "ci".into(),
            CoboltCommand::SetControlMode(ControlMode::Modulation) => "em".into(),
            CoboltCommand::OperatingModeQuery => "gom?".into(),
            CoboltCommand::InterlockQuery => "ilk?".into(),
            CoboltCommand::FaultQuery => "f?".into(),
            CoboltCommand::AutostartQuery => "@cobas?".into(),
            CoboltCommand::SetAutostart(enabled) => format!("@cobas {}", u8::from(*enabled)),
        }
    }

    pub fn parse_ok(reply: &str) -> Result<()> {
        let reply = reply.trim();
        if reply == "OK" || reply == "1" || reply == "0" {
            Ok(())
        } else {
            Err(Error::new(
                ErrorCode::Transport,
                format!("unexpected Cobolt acknowledgement: {reply}"),
            ))
        }
    }

    pub fn parse_bool(reply: &str) -> Result<bool> {
        match reply.trim() {
            "1" | "ON" | "On" | "on" => Ok(true),
            "0" | "OFF" | "Off" | "off" => Ok(false),
            other => Err(Error::new(
                ErrorCode::Transport,
                format!("invalid Cobolt bool reply: {other}"),
            )),
        }
    }

    pub fn parse_power_watts(reply: &str) -> Result<OpticalPower> {
        let watts = reply
            .trim()
            .parse::<f64>()
            .map_err(|_| Error::new(ErrorCode::Transport, "invalid Cobolt power value"))?;
        Ok(OpticalPower::from_watts(watts))
    }

    pub fn parse_current_milliamps(reply: &str) -> Result<ElectricCurrent> {
        let milliamps = reply
            .trim()
            .parse::<f64>()
            .map_err(|_| Error::new(ErrorCode::Transport, "invalid Cobolt current value"))?;
        Ok(ElectricCurrent::from_milliamps(milliamps))
    }

    pub fn probe_commands() -> Vec<CoboltCommand> {
        vec![
            CoboltCommand::Select,
            CoboltCommand::SerialNumber,
            CoboltCommand::Model,
            CoboltCommand::Version,
            CoboltCommand::Hours,
            CoboltCommand::LaserQuery,
            CoboltCommand::PowerSetpointQuery,
            CoboltCommand::ActualPowerQuery,
            CoboltCommand::MaxPowerQuery,
            CoboltCommand::CurrentQuery,
            CoboltCommand::MaxCurrentQuery,
            CoboltCommand::OperatingModeQuery,
            CoboltCommand::InterlockQuery,
            CoboltCommand::FaultQuery,
            CoboltCommand::AutostartQuery,
        ]
    }

    pub fn probe_script() -> Vec<String> {
        probe_commands().iter().map(encode).collect()
    }

    pub fn execute_probe_script(
        serial: &mut dyn SerialIo,
        polls_per_command: usize,
    ) -> Result<CoboltProbeResult> {
        let mut codec = SerialLineCodec::new(SEND_ENDING, RECV_ENDING);
        let mut replies = Vec::new();
        for command in probe_commands() {
            let encoded = encode(&command);
            serial.write(&codec.encode(&encoded))?;
            replies.push((encoded, read_line(serial, &mut codec, polls_per_command)?));
        }
        CoboltProbeResult::from_replies(&replies)
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
            "timed out waiting for Cobolt probe reply",
        ))
    }

    pub(crate) fn parse_number(command: &str, reply: &str) -> Result<f64> {
        reply.trim().parse::<f64>().map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("invalid Cobolt {command} number {reply}: {error}"),
            )
        })
    }

    pub(crate) fn parse_control_mode(reply: &str) -> Result<ControlMode> {
        match reply.trim() {
            "0" | "cp" | "CP" | "Constant Power" => Ok(ControlMode::ConstantPower),
            "1" | "ci" | "CI" | "Constant Current" => Ok(ControlMode::ConstantCurrent),
            "2" | "em" | "EM" | "Modulation" => Ok(ControlMode::Modulation),
            other => Err(Error::new(
                ErrorCode::Transport,
                format!("invalid Cobolt control mode {other}"),
            )),
        }
    }

    pub(crate) fn parse_fault(reply: &str) -> String {
        match reply.trim() {
            "0" | "0.0" | "No Fault" | "none" | "NONE" => "No Fault".into(),
            other => other.into(),
        }
    }

    fn parse_wavelength_from_model(model: &str) -> Option<Wavelength> {
        model
            .split(|ch: char| !ch.is_ascii_digit() && ch != '.')
            .filter_map(|token| token.parse::<f64>().ok())
            .find(|value| (300.0..=1200.0).contains(value))
            .map(Wavelength::from_nanometers)
    }
}

pub struct CoboltDiscovery {
    next_id: DriverId,
    probes: Vec<CoboltConfiguredProbe>,
}

impl CoboltDiscovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![CoboltConfiguredProbe::simulated()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| device.driver == "cobolt")
            .map(CoboltConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for CoboltDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver = if configured.connect_real_transport {
                    Box::new(CoboltDriver::serial(id, configured)?) as Box<dyn Driver>
                } else {
                    Box::new(CoboltDriver::configured_fixture(id, configured)) as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct CoboltConfiguredProbe {
    pub label: String,
    pub probe: protocol::CoboltProbe,
    pub enabled: bool,
    pub interlock_closed: bool,
    pub fault: String,
    pub autostart: bool,
    pub control_mode: protocol::ControlMode,
    pub power_setpoint: OpticalPower,
    pub actual_power: OpticalPower,
    pub current_setpoint: ElectricCurrent,
    pub current_actual: ElectricCurrent,
    pub hours: TimeInterval,
    pub endpoint: Option<CoboltSerialEndpoint>,
    pub connect_real_transport: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoboltSerialEndpoint {
    pub port_name: String,
    pub baud_rate: u32,
    pub timeout_ms: u64,
}

impl CoboltConfiguredProbe {
    pub fn simulated() -> Self {
        let probe = protocol::CoboltProbe::simulated();
        Self {
            label: "Simulated Cobolt serial laser".into(),
            probe,
            enabled: false,
            interlock_closed: true,
            fault: "No Fault".into(),
            autostart: false,
            control_mode: protocol::ControlMode::ConstantPower,
            power_setpoint: OpticalPower::from_milliwatts(10.0),
            actual_power: OpticalPower::from_milliwatts(0.0),
            current_setpoint: ElectricCurrent::from_milliamps(0.0),
            current_actual: ElectricCurrent::from_milliamps(0.0),
            hours: TimeInterval::from_hours(0.0),
            endpoint: None,
            connect_real_transport: false,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::simulated();
        configured.label = if device.label.is_empty() {
            "Configured Cobolt serial laser".into()
        } else {
            device.label.clone()
        };
        configured.probe.model =
            string_prop(device, "model").unwrap_or_else(|| configured.probe.model.clone());
        configured.probe.serial_number = string_prop(device, "serial_number")
            .unwrap_or_else(|| configured.probe.serial_number.clone());
        configured.probe.firmware_version = string_prop(device, "firmware_version")
            .unwrap_or_else(|| configured.probe.firmware_version.clone());
        configured.probe.wavelength = wavelength_prop(device, "wavelength")
            .or_else(|| f64_prop(device, "wavelength_nm").map(Wavelength::from_nanometers))
            .unwrap_or(configured.probe.wavelength);
        configured.probe.max_power = optical_power_prop(device, "max_power")
            .or_else(|| f64_prop(device, "max_power_mw").map(OpticalPower::from_milliwatts))
            .unwrap_or(configured.probe.max_power);
        configured.probe.max_current = electric_current_prop(device, "max_current")
            .or_else(|| f64_prop(device, "max_current_ma").map(ElectricCurrent::from_milliamps))
            .unwrap_or(configured.probe.max_current);

        configured.enabled = bool_prop(device, "enabled").unwrap_or(configured.enabled);
        configured.interlock_closed =
            bool_prop(device, "interlock_closed").unwrap_or(configured.interlock_closed);
        configured.fault = string_prop(device, "fault").unwrap_or_else(|| configured.fault.clone());
        configured.autostart = bool_prop(device, "autostart").unwrap_or(configured.autostart);
        configured.control_mode = string_prop(device, "control_mode")
            .map(|mode| control_mode_from_label(&mode))
            .transpose()?
            .unwrap_or(configured.control_mode);
        configured.power_setpoint = optical_power_prop(device, "power")
            .or_else(|| f64_prop(device, "power_mw").map(OpticalPower::from_milliwatts))
            .unwrap_or(configured.power_setpoint);
        configured.actual_power = optical_power_prop(device, "actual_power")
            .or_else(|| f64_prop(device, "actual_power_mw").map(OpticalPower::from_milliwatts))
            .unwrap_or(configured.actual_power);
        configured.current_setpoint = electric_current_prop(device, "current")
            .or_else(|| f64_prop(device, "current_ma").map(ElectricCurrent::from_milliamps))
            .unwrap_or(configured.current_setpoint);
        configured.current_actual = electric_current_prop(device, "actual_current")
            .or_else(|| f64_prop(device, "actual_current_ma").map(ElectricCurrent::from_milliamps))
            .unwrap_or(configured.current_actual);
        configured.hours = f64_prop(device, "hours")
            .map(TimeInterval::from_hours)
            .or_else(|| time_interval_prop(device, "hours_interval"))
            .unwrap_or(configured.hours);

        configured.endpoint =
            string_prop(device, "serial_port").map(|port_name| CoboltSerialEndpoint {
                port_name,
                baud_rate: u32_prop(device, "baud_rate").unwrap_or(protocol::BAUD_HIGH),
                timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(100),
            });
        configured.connect_real_transport = bool_prop(device, "connect").unwrap_or(false);
        Ok(configured)
    }
}

pub struct CoboltDriver {
    id: DriverId,
    resource: ResourceId,
    laser: DeviceId,
    probe: protocol::CoboltProbe,
    enabled: bool,
    interlock_closed: bool,
    fault: String,
    autostart: bool,
    control_mode: protocol::ControlMode,
    power_setpoint: OpticalPower,
    actual_power: OpticalPower,
    current_setpoint: ElectricCurrent,
    current_actual: ElectricCurrent,
    hours: f64,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
}

impl CoboltDriver {
    pub fn simulated(id: DriverId) -> Self {
        Self::configured_fixture(id, CoboltConfiguredProbe::simulated())
    }

    pub fn configured_fixture(id: DriverId, configured: CoboltConfiguredProbe) -> Self {
        let serial = ScriptedSerial::new();
        Self::new_configured(id, configured, Box::new(serial))
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: CoboltConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Cobolt serial probe is missing serial_port metadata",
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
    pub fn serial(_id: DriverId, _configured: CoboltConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Cobolt real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(id: DriverId, probe: protocol::CoboltProbe, serial: Box<dyn SerialIo>) -> Self {
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 701)),
            laser: DeviceId(NodeId(id.0 * 1000 + 710)),
            power_setpoint: OpticalPower::from_milliwatts(10.0),
            actual_power: OpticalPower::from_milliwatts(0.0),
            current_setpoint: ElectricCurrent::from_milliamps(0.0),
            current_actual: ElectricCurrent::from_milliamps(0.0),
            probe,
            enabled: false,
            interlock_closed: true,
            fault: "No Fault".into(),
            autostart: false,
            control_mode: protocol::ControlMode::ConstantPower,
            hours: 0.0,
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            codec: SerialLineCodec::new(protocol::SEND_ENDING, protocol::RECV_ENDING),
            serial_port: None,
            baud_rate: protocol::BAUD_HIGH,
            serial_timeout_ms: 100,
            connected: false,
        }
    }

    fn new_configured(
        id: DriverId,
        configured: CoboltConfiguredProbe,
        serial: Box<dyn SerialIo>,
    ) -> Self {
        let mut driver = Self::new(id, configured.probe, serial);
        driver.enabled = configured.enabled;
        driver.interlock_closed = configured.interlock_closed;
        driver.fault = configured.fault;
        driver.autostart = configured.autostart;
        driver.control_mode = configured.control_mode;
        driver.power_setpoint = configured.power_setpoint;
        driver.actual_power = configured.actual_power;
        driver.current_setpoint = configured.current_setpoint;
        driver.current_actual = configured.current_actual;
        driver.hours = configured.hours.hours();
        driver.serial_port = configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.port_name.clone());
        driver.baud_rate = configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.baud_rate)
            .unwrap_or(protocol::BAUD_HIGH);
        driver.serial_timeout_ms = configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.timeout_ms)
            .unwrap_or(100);
        driver.connected = configured.connect_real_transport;
        driver
    }

    #[cfg(feature = "os-serial")]
    fn with_probe_result(mut self, probe_result: protocol::CoboltProbeResult) -> Self {
        self.probe = probe_result.probe;
        self.enabled = probe_result.enabled;
        self.interlock_closed = probe_result.interlock_closed;
        self.fault = probe_result.fault;
        self.autostart = probe_result.autostart;
        self.control_mode = probe_result.control_mode;
        self.power_setpoint = probe_result.power_setpoint;
        self.actual_power = probe_result.actual_power;
        self.current_setpoint = probe_result.current;
        self.current_actual = probe_result.current;
        self.hours = probe_result.hours.hours();
        self
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn send(&mut self, command: protocol::CoboltCommand) -> Result<()> {
        let line = protocol::encode(&command);
        self.serial.write(&self.codec.encode(&line))
    }

    fn query_for_property(key: &str) -> Vec<protocol::CoboltCommand> {
        match key {
            "enabled" => vec![protocol::CoboltCommand::LaserQuery],
            "power" => vec![protocol::CoboltCommand::PowerSetpointQuery],
            "actual_power" => vec![protocol::CoboltCommand::ActualPowerQuery],
            "current" | "actual_current" => vec![protocol::CoboltCommand::CurrentQuery],
            "control_mode" => vec![protocol::CoboltCommand::OperatingModeQuery],
            "autostart" => vec![protocol::CoboltCommand::AutostartQuery],
            "interlock_closed" => vec![protocol::CoboltCommand::InterlockQuery],
            "fault" => vec![protocol::CoboltCommand::FaultQuery],
            "hours" => vec![protocol::CoboltCommand::Hours],
            "telemetry_summary" => vec![
                protocol::CoboltCommand::LaserQuery,
                protocol::CoboltCommand::PowerSetpointQuery,
                protocol::CoboltCommand::ActualPowerQuery,
                protocol::CoboltCommand::CurrentQuery,
                protocol::CoboltCommand::OperatingModeQuery,
                protocol::CoboltCommand::AutostartQuery,
                protocol::CoboltCommand::InterlockQuery,
                protocol::CoboltCommand::FaultQuery,
                protocol::CoboltCommand::Hours,
            ],
            _ => Vec::new(),
        }
    }

    fn issue_read_commands(&mut self, key: &str) -> Result<Vec<protocol::CoboltCommand>> {
        let commands = Self::query_for_property(key);
        for command in &commands {
            self.send(command.clone())?;
        }
        Ok(commands)
    }

    fn generic_refresh_property(command: &str) -> Option<&'static str> {
        match command {
            "refresh_telemetry" => Some("telemetry_summary"),
            "refresh_enabled" => Some("enabled"),
            "refresh_power" => Some("power"),
            "refresh_actual_power" => Some("actual_power"),
            "refresh_current" => Some("current"),
            "refresh_control_mode" => Some("control_mode"),
            "refresh_autostart" => Some("autostart"),
            "refresh_interlock" => Some("interlock_closed"),
            "refresh_fault" => Some("fault"),
            "refresh_hours" => Some("hours"),
            _ => None,
        }
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
                "Cobolt GenericCommand refresh commands do not accept params",
            ));
        }
        let Some(property) = Self::generic_refresh_property(&request.command) else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Cobolt GenericCommand supports refresh_telemetry, refresh_enabled, refresh_power, refresh_actual_power, refresh_current, refresh_control_mode, refresh_autostart, refresh_interlock, refresh_fault, refresh_hours",
            ));
        };
        let commands = self.issue_read_commands(property)?;
        self.read_query_replies(&commands)?;
        self.read_property(self.laser, property)
    }

    fn read_query_replies(&mut self, commands: &[protocol::CoboltCommand]) -> Result<()> {
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

    fn apply_readback_reply(
        &mut self,
        command: &protocol::CoboltCommand,
        reply: &str,
    ) -> Result<()> {
        match command {
            protocol::CoboltCommand::LaserQuery => {
                self.enabled = protocol::parse_bool(reply)?;
                let value = Value::Bool(self.enabled);
                self.emit_property(self.laser, "enabled", value);
            }
            protocol::CoboltCommand::PowerSetpointQuery => {
                self.power_setpoint = protocol::parse_power_watts(reply)?;
                let value = Value::OpticalPower(self.power_setpoint);
                self.emit_property(self.laser, "power", value);
            }
            protocol::CoboltCommand::ActualPowerQuery => {
                self.actual_power = protocol::parse_power_watts(reply)?;
                let value = Value::OpticalPower(self.actual_power);
                self.emit_property(self.laser, "actual_power", value);
            }
            protocol::CoboltCommand::CurrentQuery => {
                let current = protocol::parse_current_milliamps(reply)?;
                self.current_setpoint = current;
                self.current_actual = current;
                let value = Value::ElectricCurrent(current);
                self.emit_property(self.laser, "current", value.clone());
                self.emit_property(self.laser, "actual_current", value);
            }
            protocol::CoboltCommand::OperatingModeQuery => {
                self.control_mode = protocol::parse_control_mode(reply)?;
                let value = Value::String(self.control_mode.label().into());
                self.emit_property(self.laser, "control_mode", value);
            }
            protocol::CoboltCommand::AutostartQuery => {
                self.autostart = protocol::parse_bool(reply)?;
                let value = Value::Bool(self.autostart);
                self.emit_property(self.laser, "autostart", value);
            }
            protocol::CoboltCommand::InterlockQuery => {
                self.interlock_closed = protocol::parse_bool(reply)?;
                let value = Value::Bool(self.interlock_closed);
                self.emit_property(self.laser, "interlock_closed", value);
            }
            protocol::CoboltCommand::FaultQuery => {
                self.fault = protocol::parse_fault(reply);
                let value = Value::String(self.fault.clone());
                self.emit_property(self.laser, "fault", value);
            }
            protocol::CoboltCommand::Hours => {
                self.hours = protocol::parse_number("hrs?", reply)?;
                let value = Value::TimeInterval(TimeInterval::from_hours(self.hours));
                self.emit_property(self.laser, "hours", value);
            }
            _ => {}
        }
        Ok(())
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        vec![DeviceDescriptor {
            id: self.laser,
            driver: self.id,
            label: "cobolt-laser".into(),
            vendor: Some("Hubner Photonics".into()),
            model: Some(self.probe.model.clone()),
            serial: Some(self.probe.serial_number.clone()),
            kinds: vec![
                "laser".into(),
                "light.source".into(),
                "shutter".into(),
                "trigger.sink".into(),
                "serial.ascii".into(),
            ],
            properties: vec![
                sequenceable_property("enabled", "Emission", ValueType::Bool, None, true, None),
                sequenceable_property(
                    "power",
                    "Power setpoint",
                    ValueType::OpticalPower,
                    None,
                    true,
                    Some(Range {
                        min: Value::OpticalPower(OpticalPower::from_milliwatts(0.0)),
                        max: Value::OpticalPower(self.probe.max_power),
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
                    "current",
                    "Current setpoint",
                    ValueType::ElectricCurrent,
                    None,
                    true,
                    None,
                ),
                property(
                    "actual_current",
                    "Actual current",
                    ValueType::ElectricCurrent,
                    None,
                    false,
                    None,
                ),
                property(
                    "control_mode",
                    "Control mode",
                    ValueType::String,
                    None,
                    true,
                    None,
                )
                .with_enum(&["Constant Power", "Constant Current", "Modulation"]),
                property("autostart", "Autostart", ValueType::Bool, None, true, None),
                property(
                    "interlock_closed",
                    "Interlock closed",
                    ValueType::Bool,
                    None,
                    false,
                    None,
                ),
                property("fault", "Fault", ValueType::String, None, false, None),
                property(
                    "telemetry_summary",
                    "Telemetry summary",
                    ValueType::Map,
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
                property(
                    "hours",
                    "Hours",
                    ValueType::TimeInterval,
                    Some("h"),
                    false,
                    None,
                ),
            ],
            metadata: BTreeMap::from([
                ("model".into(), Value::String(self.probe.model.clone())),
                (
                    "firmware_version".into(),
                    Value::String(self.probe.firmware_version.clone()),
                ),
                (
                    "wavelength".into(),
                    Value::Wavelength(self.probe.wavelength),
                ),
                (
                    "max_power".into(),
                    Value::OpticalPower(self.probe.max_power),
                ),
                (
                    "max_current".into(),
                    Value::ElectricCurrent(self.probe.max_current),
                ),
            ]),
        }]
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device != self.laser {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown Cobolt device",
            ));
        }
        match key {
            "enabled" => Ok(Value::Bool(self.enabled)),
            "power" => Ok(Value::OpticalPower(self.power_setpoint)),
            "actual_power" => Ok(Value::OpticalPower(self.actual_power)),
            "current" => Ok(Value::ElectricCurrent(self.current_setpoint)),
            "actual_current" => Ok(Value::ElectricCurrent(self.current_actual)),
            "control_mode" => Ok(Value::String(self.control_mode.label().into())),
            "autostart" => Ok(Value::Bool(self.autostart)),
            "interlock_closed" => Ok(Value::Bool(self.interlock_closed)),
            "fault" => Ok(Value::String(self.fault.clone())),
            "telemetry_summary" => Ok(self.telemetry_summary()),
            "wavelength" => Ok(Value::Wavelength(self.probe.wavelength)),
            "hours" => Ok(Value::TimeInterval(TimeInterval::from_hours(self.hours))),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Cobolt property {key}"),
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
        match (key, value) {
            ("enabled", Value::Bool(enabled)) => {
                if *enabled && (!self.interlock_closed || self.fault != "No Fault") {
                    return Err(Error::new(
                        ErrorCode::Driver,
                        "Cobolt emission is blocked by interlock or fault state",
                    ));
                }
                self.send(if *enabled {
                    protocol::CoboltCommand::LaserOn
                } else {
                    protocol::CoboltCommand::LaserOff
                })?;
                self.enabled = *enabled;
                self.actual_power = if *enabled {
                    self.power_setpoint
                } else {
                    OpticalPower::from_milliwatts(0.0)
                };
                Ok(Value::Bool(*enabled))
            }
            ("power", Value::OpticalPower(power)) => {
                if power.watts() > self.probe.max_power.watts() {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Cobolt power exceeds advertised maximum",
                    ));
                }
                self.send(protocol::CoboltCommand::SetPower(*power))?;
                self.power_setpoint = *power;
                if self.enabled {
                    self.actual_power = *power;
                }
                Ok(Value::OpticalPower(*power))
            }
            ("current", Value::ElectricCurrent(current)) => {
                if current.amps() > self.probe.max_current.amps() {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Cobolt current exceeds advertised maximum",
                    ));
                }
                self.send(protocol::CoboltCommand::SetCurrent(*current))?;
                self.current_setpoint = *current;
                if self.enabled {
                    self.current_actual = *current;
                }
                Ok(Value::ElectricCurrent(*current))
            }
            ("control_mode", Value::String(mode)) => {
                let mode = match mode.as_str() {
                    "Constant Power" => protocol::ControlMode::ConstantPower,
                    "Constant Current" => protocol::ControlMode::ConstantCurrent,
                    "Modulation" => protocol::ControlMode::Modulation,
                    _ => {
                        return Err(Error::new(
                            ErrorCode::InvalidProperty,
                            "invalid Cobolt control mode",
                        ))
                    }
                };
                self.send(protocol::CoboltCommand::SetControlMode(mode))?;
                self.control_mode = mode;
                Ok(Value::String(mode.label().into()))
            }
            ("autostart", Value::Bool(enabled)) => {
                self.send(protocol::CoboltCommand::SetAutostart(*enabled))?;
                self.autostart = *enabled;
                Ok(Value::Bool(*enabled))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid Cobolt write {key}"),
            )),
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

    fn apply_trigger(&mut self, device: DeviceId, action: TriggerAction) -> Result<Value> {
        let mut changed = BTreeMap::new();
        match action {
            TriggerAction::Enable => {
                let value = self.write_property(device, "enabled", &Value::Bool(true))?;
                self.emit_property(device, "enabled", value.clone());
                changed.insert("enabled".into(), value);
            }
            TriggerAction::Disable => {
                let value = self.write_property(device, "enabled", &Value::Bool(false))?;
                self.emit_property(device, "enabled", value.clone());
                changed.insert("enabled".into(), value);
            }
            TriggerAction::Pulse => {
                let value = self.write_property(device, "enabled", &Value::Bool(true))?;
                self.emit_property(device, "enabled", value.clone());
                changed.insert("enabled_before_pulse_end".into(), value);

                let value = self.write_property(device, "enabled", &Value::Bool(false))?;
                self.emit_property(device, "enabled", value.clone());
                changed.insert("enabled".into(), value);
            }
        }
        Ok(Value::Map(changed))
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
                Error::new(ErrorCode::InvalidCommand, "missing Cobolt descriptor")
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
                if let Value::OpticalPower(power) = value {
                    if power.watts() > self.probe.max_power.watts() {
                        return Err(Error::new(
                            ErrorCode::InvalidProperty,
                            "Cobolt power exceeds advertised maximum",
                        ));
                    }
                }
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
                "control_mode".into(),
                Value::String(self.control_mode.label().into()),
            ),
            ("power".into(), Value::OpticalPower(self.power_setpoint)),
            (
                "actual_power".into(),
                Value::OpticalPower(self.actual_power),
            ),
            (
                "interlock_closed".into(),
                Value::Bool(self.interlock_closed),
            ),
            ("fault".into(), Value::String(self.fault.clone())),
            ("routes".into(), Value::List(self.local_timing_routes(plan))),
            (
                "sequences".into(),
                Value::List(self.local_timing_sequences(plan)),
            ),
        ]))
    }

    fn timing_transaction(
        &self,
        description: &str,
        command: protocol::CoboltCommand,
    ) -> PhysicalTransaction {
        let line = protocol::encode(&command);
        PhysicalTransaction {
            resource: Some(self.resource),
            description: description.into(),
            payload: Value::Bytes(self.codec.encode(&line)),
        }
    }

    fn telemetry_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("device".into(), Value::I64(self.laser.0 .0 as i64)),
            ("model".into(), Value::String(self.probe.model.clone())),
            (
                "serial_number".into(),
                Value::String(self.probe.serial_number.clone()),
            ),
            (
                "firmware_version".into(),
                Value::String(self.probe.firmware_version.clone()),
            ),
            ("enabled".into(), Value::Bool(self.enabled)),
            ("power".into(), Value::OpticalPower(self.power_setpoint)),
            (
                "actual_power".into(),
                Value::OpticalPower(self.actual_power),
            ),
            (
                "current".into(),
                Value::ElectricCurrent(self.current_setpoint),
            ),
            (
                "actual_current".into(),
                Value::ElectricCurrent(self.current_actual),
            ),
            (
                "max_power".into(),
                Value::OpticalPower(self.probe.max_power),
            ),
            (
                "max_current".into(),
                Value::ElectricCurrent(self.probe.max_current),
            ),
            (
                "control_mode".into(),
                Value::String(self.control_mode.label().into()),
            ),
            ("autostart".into(), Value::Bool(self.autostart)),
            (
                "interlock_closed".into(),
                Value::Bool(self.interlock_closed),
            ),
            ("fault".into(), Value::String(self.fault.clone())),
            (
                "wavelength".into(),
                Value::Wavelength(self.probe.wavelength),
            ),
            (
                "hours".into(),
                Value::TimeInterval(TimeInterval::from_hours(self.hours)),
            ),
        ]))
    }
}

impl Driver for CoboltDriver {
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
            label: "cobolt-serial".into(),
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
                (
                    "baud_primary".into(),
                    Value::I64(protocol::BAUD_HIGH as i64),
                ),
                (
                    "baud_fallback".into(),
                    Value::I64(protocol::BAUD_FALLBACK as i64),
                ),
                ("send_terminator".into(), Value::String("CR".into())),
                ("recv_terminator".into(), Value::String("CRLF".into())),
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
                capability(1, device, CapabilityKind::TriggerSink),
                capability(2, device, CapabilityKind::GenericCommand),
                capability(3, device, CapabilityKind::Dac),
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
                        description: format!("cobolt read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("cobolt write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "cobolt remultiplexed laser state set".into(),
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
                            "unknown Cobolt capability",
                        ));
                    };
                    if !capability.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "Cobolt {:?} expects {:?}, got {:?}",
                                capability.kind,
                                capability.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("cobolt invoke {}", capability.id.0),
                        payload: match request {
                            CapabilityRequest::None => Value::String("trigger pulse".into()),
                            CapabilityRequest::GenericCommand(request) => {
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
                                        "Cobolt GenericCommand refresh commands do not accept params",
                                    ));
                                }
                                if Self::generic_refresh_property(&request.command).is_none() {
                                    return Err(Error::new(
                                        ErrorCode::InvalidCommand,
                                        "Cobolt GenericCommand supports refresh_telemetry, refresh_enabled, refresh_power, refresh_actual_power, refresh_current, refresh_control_mode, refresh_autostart, refresh_interlock, refresh_fault, refresh_hours",
                                    ));
                                }
                                Value::String(request.command.clone())
                            }
                            CapabilityRequest::Dac(request) => {
                                if !matches!(request.value, Value::OpticalPower(_)) {
                                    return Err(Error::new(
                                        ErrorCode::InvalidCommand,
                                        "Cobolt Dac value must be OpticalPower",
                                    ));
                                }
                                request.value.clone()
                            }
                            CapabilityRequest::Trigger(request) => {
                                Value::String(format!("{:?}", request.action))
                            }
                            _ => {
                                return Err(Error::new(
                                    ErrorCode::Unsupported,
                                    "unsupported Cobolt request",
                                ))
                            }
                        },
                    });
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
                    let commands = self.issue_read_commands(&key)?;
                    self.read_query_replies(&commands)?;
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
                            "unknown Cobolt capability",
                        ));
                    };
                    if !capability.accepts_request(&request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "Cobolt {:?} expects {:?}, got {:?}",
                                capability.kind,
                                capability.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    last = match request {
                        CapabilityRequest::None => {
                            self.apply_trigger(device, TriggerAction::Pulse)?
                        }
                        CapabilityRequest::GenericCommand(request) => {
                            self.invoke_generic(request)?
                        }
                        CapabilityRequest::Dac(request) => {
                            let value = self.write_property(device, "power", &request.value)?;
                            self.emit_property(device, "power", value.clone());
                            value
                        }
                        CapabilityRequest::Trigger(request) => {
                            self.apply_trigger(device, request.action)?
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported Cobolt request",
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
                        message: format!("cobolt serial: {line}"),
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
                description: "cobolt timing arm summary".into(),
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
                    "cobolt timing start emission enable",
                    protocol::CoboltCommand::LaserOn,
                ),
                PhysicalTransaction {
                    resource: Some(self.resource),
                    description: "cobolt timing start summary".into(),
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
                    "cobolt timing stop emission disable",
                    protocol::CoboltCommand::LaserOff,
                ),
                PhysicalTransaction {
                    resource: Some(self.resource),
                    description: "cobolt timing stop summary".into(),
                    payload: with_applied(self.timing_summary(&armed.plan, "stop"), applied),
                },
            ],
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

trait PropertySchemaExt {
    fn with_enum(self, values: &[&str]) -> Self;
}

impl PropertySchemaExt for PropertySchema {
    fn with_enum(mut self, values: &[&str]) -> Self {
        self.enum_values = values
            .iter()
            .map(|value| EnumValue {
                value: Value::String((*value).into()),
                label: (*value).into(),
            })
            .collect();
        self
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

fn electric_current_prop(device: &DeviceConfig, key: &str) -> Option<ElectricCurrent> {
    match device.properties.get(key) {
        Some(Value::ElectricCurrent(value)) => Some(*value),
        _ => None,
    }
}

fn time_interval_prop(device: &DeviceConfig, key: &str) -> Option<TimeInterval> {
    match device.properties.get(key) {
        Some(Value::TimeInterval(value)) => Some(*value),
        _ => None,
    }
}

fn control_mode_from_label(label: &str) -> Result<protocol::ControlMode> {
    match label {
        "Constant Power" | "constant_power" | "power" => Ok(protocol::ControlMode::ConstantPower),
        "Constant Current" | "constant_current" | "current" => {
            Ok(protocol::ControlMode::ConstantCurrent)
        }
        "Modulation" | "modulation" => Ok(protocol::ControlMode::Modulation),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("invalid Cobolt control mode: {other}"),
        )),
    }
}
