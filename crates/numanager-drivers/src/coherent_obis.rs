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
    pub const POWER_WIRE_UNIT: &str = "W";

    #[derive(Debug, Clone, PartialEq)]
    pub struct ObisProbe {
        pub index: u8,
        pub head_id: String,
        pub wavelength: Wavelength,
        pub min_power: OpticalPower,
        pub max_power: OpticalPower,
        pub hours: f64,
    }

    impl ObisProbe {
        pub fn simulated() -> Self {
            Self {
                index: 1,
                head_id: "OBIS-SIM-488".into(),
                wavelength: Wavelength::from_nanometers(488.0),
                min_power: OpticalPower::from_milliwatts(0.0),
                max_power: OpticalPower::from_milliwatts(150.0),
                hours: 42.0,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct ObisProbeResult {
        pub probe: ObisProbe,
        pub power_setpoint: OpticalPower,
        pub enabled: bool,
        pub analog_modulation: bool,
        pub mode: ObisMode,
        pub fault: String,
        pub handshake_disabled: bool,
        pub prompt_disabled: bool,
        pub error_cleared: bool,
        pub replies: Vec<(String, String)>,
    }

    impl ObisProbeResult {
        pub fn from_replies(
            index: u8,
            replies: &[(impl AsRef<str>, impl AsRef<str>)],
        ) -> Result<Self> {
            let mut probe = ObisProbe {
                index,
                ..ObisProbe::simulated()
            };
            let mut power_setpoint = probe.min_power;
            let mut enabled = false;
            let mut analog_modulation = false;
            let mut mode = ObisMode::ContinuousWave;
            let mut fault = "No Fault".into();
            let mut handshake_disabled = false;
            let mut prompt_disabled = false;
            let mut error_cleared = false;
            let mut replies_stored = Vec::new();

            for (command, reply) in replies {
                let command = command.as_ref();
                let reply = reply.as_ref().trim();
                replies_stored.push((command.to_string(), reply.to_string()));
                let normalized = strip_index(index, command);
                match normalized.as_str() {
                    "SYST:COMM:HAND Off" => {
                        parse_ok(reply)?;
                        handshake_disabled = true;
                    }
                    "SYST:COMM:PROM Off" => {
                        parse_ok(reply)?;
                        prompt_disabled = true;
                    }
                    "SYST:ERR:CLE" => {
                        parse_ok(reply)?;
                        error_cleared = true;
                    }
                    "SYST:INF:SNUM?" => probe.head_id = reply.to_string(),
                    "SYST:DIOD:HOUR?" => {
                        probe.hours = parse_number("SYST:DIOD:HOUR?", reply)?;
                    }
                    "SYST:INF:WAV?" => probe.wavelength = parse_wavelength_nm(reply)?,
                    "SOUR:POW:LIM:LOW?" => probe.min_power = parse_power_watts(reply)?,
                    "SOUR:POW:LIM:HIGH?" => probe.max_power = parse_power_watts(reply)?,
                    "SOUR:POW:LEV:IMM:AMPL?" => power_setpoint = parse_power_watts(reply)?,
                    "SOUR:AM:STATE?" => {
                        let value = parse_bool(reply)?;
                        analog_modulation = value;
                        enabled = value;
                    }
                    "SOUR:AM:SOUR?" => mode = parse_mode_reply(reply)?,
                    "SYST:ERR?" => fault = parse_fault(reply),
                    _ => {}
                }
            }

            Ok(Self {
                probe,
                power_setpoint,
                enabled,
                analog_modulation,
                mode,
                fault,
                handshake_disabled,
                prompt_disabled,
                error_cleared,
                replies: replies_stored,
            })
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ObisMode {
        ContinuousWave,
        CdrhDelay,
    }

    impl ObisMode {
        pub fn wire(self) -> &'static str {
            match self {
                ObisMode::ContinuousWave => "CW",
                ObisMode::CdrhDelay => "CDRH",
            }
        }

        pub fn label(self) -> &'static str {
            match self {
                ObisMode::ContinuousWave => "CW",
                ObisMode::CdrhDelay => "CDRH",
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum ObisCommand {
        QueryHeadSerial,
        QueryHeadHours,
        QueryWavelength,
        QueryMinPower,
        QueryMaxPower,
        QueryPowerSetpoint,
        SetPower(OpticalPower),
        QueryAnalogState,
        SetAnalogState(bool),
        QueryMode,
        SetMode(ObisMode),
        QueryEmission,
        SetEmission(bool),
        QueryError,
        ClearError,
        CommunicationHandshake(bool),
        CommunicationPrompt(bool),
    }

    pub fn encode(index: u8, command: &ObisCommand) -> String {
        match command {
            ObisCommand::QueryHeadSerial => format!("SYST{index}:INF:SNUM?"),
            ObisCommand::QueryHeadHours => format!("SYST{index}:DIOD:HOUR?"),
            ObisCommand::QueryWavelength => format!("SYST{index}:INF:WAV?"),
            ObisCommand::QueryMinPower => format!("SOUR{index}:POW:LIM:LOW?"),
            ObisCommand::QueryMaxPower => format!("SOUR{index}:POW:LIM:HIGH?"),
            ObisCommand::QueryPowerSetpoint => format!("SOUR{index}:POW:LEV:IMM:AMPL?"),
            ObisCommand::SetPower(power) => {
                format!("SOUR{index}:POW:LEV:IMM:AMPL {:.6}", power.watts())
            }
            ObisCommand::QueryAnalogState => format!("SOUR{index}:AM:STATE?"),
            ObisCommand::SetAnalogState(enabled) => {
                format!("SOUR{index}:AM:STATE {}", on_off(*enabled))
            }
            ObisCommand::QueryMode => format!("SOUR{index}:AM:SOUR?"),
            ObisCommand::SetMode(mode) => format!("SOUR{index}:AM:SOUR {}", mode.wire()),
            ObisCommand::QueryEmission => format!("SOUR{index}:AM:STATE?"),
            ObisCommand::SetEmission(enabled) => {
                format!("SOUR{index}:AM:STATE {}", on_off(*enabled))
            }
            ObisCommand::QueryError => format!("SYST{index}:ERR?"),
            ObisCommand::ClearError => format!("SYST{index}:ERR:CLE"),
            ObisCommand::CommunicationHandshake(enabled) => {
                format!("SYST{index}:COMM:HAND {}", on_off(*enabled))
            }
            ObisCommand::CommunicationPrompt(enabled) => {
                format!("SYST{index}:COMM:PROM {}", on_off(*enabled))
            }
        }
    }

    pub fn parse_bool(reply: &str) -> Result<bool> {
        match reply.trim().to_ascii_lowercase().as_str() {
            "on" | "1" => Ok(true),
            "off" | "0" => Ok(false),
            other => Err(Error::new(
                ErrorCode::Transport,
                format!("invalid OBIS boolean reply: {other}"),
            )),
        }
    }

    pub fn parse_power_watts(reply: &str) -> Result<OpticalPower> {
        let watts = reply
            .trim()
            .parse::<f64>()
            .map_err(|_| Error::new(ErrorCode::Transport, "invalid OBIS power value"))?;
        Ok(OpticalPower::from_watts(watts))
    }

    pub fn parse_wavelength_nm(reply: &str) -> Result<Wavelength> {
        let nm = reply
            .trim()
            .parse::<f64>()
            .map_err(|_| Error::new(ErrorCode::Transport, "invalid OBIS wavelength value"))?;
        Ok(Wavelength::from_nanometers(nm))
    }

    pub fn probe_commands() -> Vec<ObisCommand> {
        vec![
            ObisCommand::CommunicationHandshake(false),
            ObisCommand::CommunicationPrompt(false),
            ObisCommand::ClearError,
            ObisCommand::QueryHeadSerial,
            ObisCommand::QueryHeadHours,
            ObisCommand::QueryWavelength,
            ObisCommand::QueryMinPower,
            ObisCommand::QueryMaxPower,
            ObisCommand::QueryPowerSetpoint,
            ObisCommand::QueryAnalogState,
            ObisCommand::QueryMode,
            ObisCommand::QueryError,
        ]
    }

    pub fn probe_script(index: u8) -> Vec<String> {
        probe_commands()
            .iter()
            .map(|command| encode(index, command))
            .collect()
    }

    pub fn execute_probe_script(
        serial: &mut dyn SerialIo,
        index: u8,
        polls_per_command: usize,
    ) -> Result<ObisProbeResult> {
        let mut codec = SerialLineCodec::new(SEND_ENDING, RECV_ENDING);
        let mut replies = Vec::new();
        for command in probe_commands() {
            let encoded = encode(index, &command);
            serial.write(&codec.encode(&encoded))?;
            replies.push((encoded, read_line(serial, &mut codec, polls_per_command)?));
        }
        ObisProbeResult::from_replies(index, &replies)
    }

    pub fn parse_ok(reply: &str) -> Result<()> {
        match reply.trim() {
            "OK" | "0" | "" => Ok(()),
            other => Err(Error::new(
                ErrorCode::Transport,
                format!("unexpected OBIS acknowledgement: {other}"),
            )),
        }
    }

    pub fn parse_mode_reply(reply: &str) -> Result<ObisMode> {
        match reply.trim().to_ascii_uppercase().as_str() {
            "CW" | "CONTINUOUSWAVE" | "CONTINUOUS_WAVE" => Ok(ObisMode::ContinuousWave),
            "CDRH" | "CDRHDELAY" | "CDRH_DELAY" => Ok(ObisMode::CdrhDelay),
            other => Err(Error::new(
                ErrorCode::Transport,
                format!("invalid OBIS mode reply: {other}"),
            )),
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
            "timed out waiting for OBIS probe reply",
        ))
    }

    pub(crate) fn parse_number(command: &str, reply: &str) -> Result<f64> {
        reply.trim().parse::<f64>().map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("invalid OBIS {command} number {reply}: {error}"),
            )
        })
    }

    pub(crate) fn parse_fault(reply: &str) -> String {
        match reply.trim() {
            "0" | "0,No error" | "0,\"No error\"" | "No error" | "No Fault" => "No Fault".into(),
            other => other.into(),
        }
    }

    fn strip_index(index: u8, command: &str) -> String {
        command
            .strip_prefix(&format!("SYST{index}"))
            .map(|rest| format!("SYST{rest}"))
            .or_else(|| {
                command
                    .strip_prefix(&format!("SOUR{index}"))
                    .map(|rest| format!("SOUR{rest}"))
            })
            .unwrap_or_else(|| command.to_string())
    }

    fn on_off(enabled: bool) -> &'static str {
        if enabled {
            "On"
        } else {
            "Off"
        }
    }
}

pub struct ObisDiscovery {
    next_id: DriverId,
    probes: Vec<ObisConfiguredProbe>,
}

impl ObisDiscovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![ObisConfiguredProbe::simulated()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| device.driver == "coherent-obis")
            .map(ObisConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for ObisDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver = if configured.connect_real_transport {
                    Box::new(ObisDriver::serial(id, configured)?) as Box<dyn Driver>
                } else {
                    Box::new(ObisDriver::configured_fixture(id, configured)) as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct ObisConfiguredProbe {
    pub label: String,
    pub probe: protocol::ObisProbe,
    pub enabled: bool,
    pub analog_modulation: bool,
    pub mode: protocol::ObisMode,
    pub power_setpoint: OpticalPower,
    pub actual_power: OpticalPower,
    pub fault: String,
    pub endpoint: Option<ObisSerialEndpoint>,
    pub connect_real_transport: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObisSerialEndpoint {
    pub port_name: String,
    pub baud_rate: u32,
    pub timeout_ms: u64,
}

impl ObisConfiguredProbe {
    pub fn simulated() -> Self {
        let probe = protocol::ObisProbe::simulated();
        Self {
            label: "Simulated Coherent OBIS laser".into(),
            power_setpoint: probe.min_power,
            probe,
            enabled: false,
            analog_modulation: false,
            mode: protocol::ObisMode::ContinuousWave,
            actual_power: OpticalPower::from_milliwatts(0.0),
            fault: "No Fault".into(),
            endpoint: None,
            connect_real_transport: false,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::simulated();
        configured.label = if device.label.is_empty() {
            "Configured Coherent OBIS laser".into()
        } else {
            device.label.clone()
        };
        configured.probe.index = u8_prop(device, "index").unwrap_or(configured.probe.index);
        configured.probe.head_id =
            string_prop(device, "head_id").unwrap_or_else(|| configured.probe.head_id.clone());
        configured.probe.wavelength = wavelength_prop(device, "wavelength")
            .or_else(|| f64_prop(device, "wavelength_nm").map(Wavelength::from_nanometers))
            .unwrap_or(configured.probe.wavelength);
        configured.probe.min_power = optical_power_prop(device, "min_power")
            .or_else(|| f64_prop(device, "min_power_mw").map(OpticalPower::from_milliwatts))
            .unwrap_or(configured.probe.min_power);
        configured.probe.max_power = optical_power_prop(device, "max_power")
            .or_else(|| f64_prop(device, "max_power_mw").map(OpticalPower::from_milliwatts))
            .unwrap_or(configured.probe.max_power);
        configured.probe.hours = time_interval_prop(device, "head_hours")
            .map(TimeInterval::hours)
            .or_else(|| f64_prop(device, "head_hours"))
            .or_else(|| f64_prop(device, "head_hours_h"))
            .unwrap_or(configured.probe.hours);
        configured.power_setpoint = optical_power_prop(device, "power")
            .or_else(|| f64_prop(device, "power_mw").map(OpticalPower::from_milliwatts))
            .unwrap_or(configured.power_setpoint);
        configured.actual_power = optical_power_prop(device, "actual_power")
            .or_else(|| f64_prop(device, "actual_power_mw").map(OpticalPower::from_milliwatts))
            .unwrap_or(configured.actual_power);
        configured.enabled = bool_prop(device, "enabled").unwrap_or(configured.enabled);
        configured.analog_modulation =
            bool_prop(device, "analog_modulation").unwrap_or(configured.analog_modulation);
        configured.mode = string_prop(device, "mode")
            .map(|mode| parse_mode(&mode))
            .transpose()?
            .unwrap_or(configured.mode);
        configured.fault = string_prop(device, "fault").unwrap_or_else(|| configured.fault.clone());
        configured.endpoint =
            string_prop(device, "serial_port").map(|port_name| ObisSerialEndpoint {
                port_name,
                baud_rate: u32_prop(device, "baud_rate").unwrap_or(115_200),
                timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(100),
            });
        configured.connect_real_transport = bool_prop(device, "connect").unwrap_or(false);
        Ok(configured)
    }
}

pub struct ObisDriver {
    id: DriverId,
    resource: ResourceId,
    laser: DeviceId,
    probe: protocol::ObisProbe,
    enabled: bool,
    analog_modulation: bool,
    mode: protocol::ObisMode,
    power_setpoint: OpticalPower,
    actual_power: OpticalPower,
    fault: String,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
}

impl ObisDriver {
    pub fn simulated(id: DriverId) -> Self {
        Self::configured_fixture(id, ObisConfiguredProbe::simulated())
    }

    pub fn configured_fixture(id: DriverId, configured: ObisConfiguredProbe) -> Self {
        let serial = ScriptedSerial::new();
        Self::new_configured(id, configured, Box::new(serial))
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: ObisConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Coherent OBIS serial probe is missing serial_port metadata",
            )
        })?;
        let mut serial = numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(endpoint.port_name, endpoint.baud_rate)
                .timeout(Duration::from_millis(endpoint.timeout_ms)),
        )?;
        let probe_result = protocol::execute_probe_script(&mut serial, configured.probe.index, 4)?;
        Ok(Self::new_configured(id, configured, Box::new(serial)).with_probe_result(probe_result))
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _configured: ObisConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Coherent OBIS real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(id: DriverId, probe: protocol::ObisProbe, serial: Box<dyn SerialIo>) -> Self {
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 1001)),
            laser: DeviceId(NodeId(id.0 * 1000 + 1010)),
            power_setpoint: probe.min_power,
            actual_power: OpticalPower::from_milliwatts(0.0),
            probe,
            enabled: false,
            analog_modulation: false,
            mode: protocol::ObisMode::ContinuousWave,
            fault: "No Fault".into(),
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            codec: SerialLineCodec::new(protocol::SEND_ENDING, protocol::RECV_ENDING),
            serial_port: None,
            baud_rate: 115_200,
            serial_timeout_ms: 100,
            connected: false,
        }
    }

    fn new_configured(
        id: DriverId,
        configured: ObisConfiguredProbe,
        serial: Box<dyn SerialIo>,
    ) -> Self {
        let mut driver = Self::new(id, configured.probe, serial);
        driver.enabled = configured.enabled;
        driver.analog_modulation = configured.analog_modulation;
        driver.mode = configured.mode;
        driver.power_setpoint = configured.power_setpoint;
        driver.actual_power = configured.actual_power;
        driver.fault = configured.fault;
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
            .unwrap_or(100);
        driver.connected = configured.connect_real_transport;
        driver
    }

    #[cfg(feature = "os-serial")]
    fn with_probe_result(mut self, probe_result: protocol::ObisProbeResult) -> Self {
        self.probe = probe_result.probe;
        self.enabled = probe_result.enabled;
        self.analog_modulation = probe_result.analog_modulation;
        self.mode = probe_result.mode;
        self.power_setpoint = probe_result.power_setpoint;
        self.fault = probe_result.fault;
        self
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn send(&mut self, command: protocol::ObisCommand) -> Result<()> {
        let line = protocol::encode(self.probe.index, &command);
        self.serial.write(&self.codec.encode(&line))
    }

    fn queries_for_property(
        device: DeviceId,
        laser: DeviceId,
        key: &str,
    ) -> Vec<protocol::ObisCommand> {
        if device != laser {
            return Vec::new();
        }
        match key {
            "enabled" => vec![protocol::ObisCommand::QueryEmission],
            "power" | "actual_power" => vec![protocol::ObisCommand::QueryPowerSetpoint],
            "wavelength" => vec![protocol::ObisCommand::QueryWavelength],
            "analog_modulation" => vec![protocol::ObisCommand::QueryAnalogState],
            "mode" | "cdrh_delay" => vec![protocol::ObisCommand::QueryMode],
            "fault" => vec![protocol::ObisCommand::QueryError],
            "head_id" => vec![protocol::ObisCommand::QueryHeadSerial],
            "head_hours" => vec![protocol::ObisCommand::QueryHeadHours],
            "telemetry_summary" => vec![
                protocol::ObisCommand::QueryEmission,
                protocol::ObisCommand::QueryPowerSetpoint,
                protocol::ObisCommand::QueryWavelength,
                protocol::ObisCommand::QueryAnalogState,
                protocol::ObisCommand::QueryMode,
                protocol::ObisCommand::QueryError,
                protocol::ObisCommand::QueryHeadSerial,
                protocol::ObisCommand::QueryHeadHours,
            ],
            _ => Vec::new(),
        }
    }

    fn issue_read_commands(
        &mut self,
        device: DeviceId,
        key: &str,
    ) -> Result<Vec<protocol::ObisCommand>> {
        let commands = Self::queries_for_property(device, self.laser, key);
        for command in &commands {
            self.send(command.clone())?;
        }
        Ok(commands)
    }

    fn read_query_replies(&mut self, commands: &[protocol::ObisCommand]) -> Result<()> {
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

    fn confirm_write_readback(&mut self, commands: &[protocol::ObisCommand]) -> Result<()> {
        let fault_before = self.fault.clone();
        for command in commands {
            self.send(command.clone())?;
        }
        self.read_query_replies(commands)?;
        if self.fault != fault_before && self.fault != "No Fault" {
            return Err(Error::new(
                ErrorCode::Driver,
                format!("OBIS reported fault {}", self.fault),
            ));
        }
        Ok(())
    }

    fn refresh_commands_for(command: &str) -> Result<Vec<protocol::ObisCommand>> {
        match command {
            "refresh_telemetry" => Ok(vec![
                protocol::ObisCommand::QueryEmission,
                protocol::ObisCommand::QueryPowerSetpoint,
                protocol::ObisCommand::QueryWavelength,
                protocol::ObisCommand::QueryAnalogState,
                protocol::ObisCommand::QueryMode,
                protocol::ObisCommand::QueryError,
                protocol::ObisCommand::QueryHeadSerial,
                protocol::ObisCommand::QueryHeadHours,
            ]),
            "refresh_identity" => Ok(vec![
                protocol::ObisCommand::QueryHeadSerial,
                protocol::ObisCommand::QueryHeadHours,
                protocol::ObisCommand::QueryWavelength,
            ]),
            "refresh_power" => Ok(vec![protocol::ObisCommand::QueryPowerSetpoint]),
            "refresh_status" => Ok(vec![
                protocol::ObisCommand::QueryEmission,
                protocol::ObisCommand::QueryAnalogState,
                protocol::ObisCommand::QueryMode,
                protocol::ObisCommand::QueryError,
            ]),
            "refresh_limits" => Ok(vec![
                protocol::ObisCommand::QueryMinPower,
                protocol::ObisCommand::QueryMaxPower,
            ]),
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "Coherent OBIS GenericCommand supports refresh_telemetry, refresh_identity, refresh_power, refresh_status, and refresh_limits; got {other}"
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
                "Coherent OBIS GenericCommand does not take parameters",
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
                Value::String("Coherent OBIS query readback".into()),
            ),
        ])))
    }

    fn apply_readback_reply(&mut self, command: &protocol::ObisCommand, reply: &str) -> Result<()> {
        match command {
            protocol::ObisCommand::QueryEmission | protocol::ObisCommand::QueryAnalogState => {
                let enabled = protocol::parse_bool(reply)?;
                self.enabled = enabled;
                self.analog_modulation = enabled;
                let value = Value::Bool(enabled);
                self.emit_property(self.laser, "enabled", value.clone());
                self.emit_property(self.laser, "analog_modulation", value);
            }
            protocol::ObisCommand::QueryPowerSetpoint => {
                self.power_setpoint = protocol::parse_power_watts(reply)?;
                self.actual_power = self.power_setpoint;
                let value = Value::OpticalPower(self.power_setpoint);
                self.emit_property(self.laser, "power", value.clone());
                self.emit_property(self.laser, "actual_power", value);
            }
            protocol::ObisCommand::QueryMinPower => {
                self.probe.min_power = protocol::parse_power_watts(reply)?;
            }
            protocol::ObisCommand::QueryMaxPower => {
                self.probe.max_power = protocol::parse_power_watts(reply)?;
            }
            protocol::ObisCommand::QueryWavelength => {
                self.probe.wavelength = protocol::parse_wavelength_nm(reply)?;
                let value = Value::Wavelength(self.probe.wavelength);
                self.emit_property(self.laser, "wavelength", value);
            }
            protocol::ObisCommand::QueryMode => {
                self.mode = protocol::parse_mode_reply(reply)?;
                let mode = Value::String(self.mode.label().into());
                self.emit_property(self.laser, "mode", mode);
                self.emit_property(
                    self.laser,
                    "cdrh_delay",
                    Value::Bool(self.mode == protocol::ObisMode::CdrhDelay),
                );
            }
            protocol::ObisCommand::QueryError => {
                self.fault = protocol::parse_fault(reply);
                let value = Value::String(self.fault.clone());
                self.emit_property(self.laser, "fault", value);
            }
            protocol::ObisCommand::QueryHeadSerial => {
                self.probe.head_id = reply.trim().to_string();
                let value = Value::String(self.probe.head_id.clone());
                self.emit_property(self.laser, "head_id", value);
            }
            protocol::ObisCommand::QueryHeadHours => {
                self.probe.hours = protocol::parse_number("SYST:DIOD:HOUR?", reply)?;
                let value = Value::TimeInterval(TimeInterval::from_hours(self.probe.hours));
                self.emit_property(self.laser, "head_hours", value);
            }
            _ => {}
        }
        Ok(())
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        vec![DeviceDescriptor {
            id: self.laser,
            driver: self.id,
            label: "coherent-obis-laser".into(),
            vendor: Some("Coherent".into()),
            model: Some("OBIS".into()),
            serial: Some(self.probe.head_id.clone()),
            kinds: vec![
                "laser".into(),
                "light.source".into(),
                "shutter".into(),
                "trigger.sink".into(),
                "serial.scpi".into(),
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
                        min: Value::OpticalPower(self.probe.min_power),
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
                    "wavelength",
                    "Wavelength",
                    ValueType::Wavelength,
                    None,
                    false,
                    None,
                ),
                property(
                    "analog_modulation",
                    "Analog modulation",
                    ValueType::Bool,
                    None,
                    true,
                    None,
                ),
                mode_property(),
                property(
                    "cdrh_delay",
                    "CDRH delay",
                    ValueType::Bool,
                    None,
                    true,
                    None,
                ),
                property("fault", "Fault", ValueType::String, None, false, None),
                property("head_id", "Head ID", ValueType::String, None, false, None),
                property(
                    "telemetry_summary",
                    "Telemetry summary",
                    ValueType::Map,
                    None,
                    false,
                    None,
                ),
                property(
                    "head_hours",
                    "Head usage hours",
                    ValueType::TimeInterval,
                    Some("h"),
                    false,
                    None,
                ),
            ],
            metadata: BTreeMap::from([
                ("device_index".into(), Value::I64(self.probe.index as i64)),
                (
                    "power_wire_unit".into(),
                    Value::String(protocol::POWER_WIRE_UNIT.into()),
                ),
                (
                    "completion".into(),
                    Value::String("emission state is complete after OBIS state reply".into()),
                ),
            ]),
        }]
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device != self.laser {
            return Err(Error::new(ErrorCode::InvalidCommand, "unknown OBIS device"));
        }
        match key {
            "enabled" => Ok(Value::Bool(self.enabled)),
            "power" => Ok(Value::OpticalPower(self.power_setpoint)),
            "actual_power" => Ok(Value::OpticalPower(self.actual_power)),
            "wavelength" => Ok(Value::Wavelength(self.probe.wavelength)),
            "analog_modulation" => Ok(Value::Bool(self.analog_modulation)),
            "mode" => Ok(Value::String(self.mode.label().into())),
            "cdrh_delay" => Ok(Value::Bool(self.mode == protocol::ObisMode::CdrhDelay)),
            "fault" => Ok(Value::String(self.fault.clone())),
            "head_id" => Ok(Value::String(self.probe.head_id.clone())),
            "telemetry_summary" => Ok(self.telemetry_summary()),
            "head_hours" => Ok(Value::TimeInterval(TimeInterval::from_hours(
                self.probe.hours,
            ))),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown OBIS property {key}"),
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
        if device != self.laser {
            return Err(Error::new(ErrorCode::InvalidCommand, "unknown OBIS device"));
        }
        match (key, value) {
            ("enabled", Value::Bool(enabled)) => {
                if *enabled && self.fault != "No Fault" {
                    return Err(Error::new(
                        ErrorCode::Driver,
                        format!("refusing to enable OBIS with fault {}", self.fault),
                    ));
                }
                self.send(protocol::ObisCommand::SetEmission(*enabled))?;
                self.enabled = *enabled;
                self.actual_power = if *enabled {
                    self.power_setpoint
                } else {
                    OpticalPower::from_milliwatts(0.0)
                };
                self.confirm_write_readback(&[
                    protocol::ObisCommand::QueryEmission,
                    protocol::ObisCommand::QueryError,
                ])?;
                self.finish_emission();
                Ok(Value::Bool(self.enabled))
            }
            ("power", Value::OpticalPower(power)) => {
                self.send(protocol::ObisCommand::SetPower(*power))?;
                self.power_setpoint = *power;
                if self.enabled {
                    self.actual_power = *power;
                }
                self.confirm_write_readback(&[
                    protocol::ObisCommand::QueryPowerSetpoint,
                    protocol::ObisCommand::QueryError,
                ])?;
                Ok(Value::OpticalPower(self.power_setpoint))
            }
            ("analog_modulation", Value::Bool(enabled)) => {
                self.send(protocol::ObisCommand::SetAnalogState(*enabled))?;
                self.analog_modulation = *enabled;
                self.confirm_write_readback(&[
                    protocol::ObisCommand::QueryAnalogState,
                    protocol::ObisCommand::QueryError,
                ])?;
                Ok(Value::Bool(self.analog_modulation))
            }
            ("mode", Value::String(mode)) => {
                let mode = parse_mode(mode)?;
                self.send(protocol::ObisCommand::SetMode(mode))?;
                self.mode = mode;
                self.confirm_write_readback(&[
                    protocol::ObisCommand::QueryMode,
                    protocol::ObisCommand::QueryError,
                ])?;
                Ok(Value::String(self.mode.label().into()))
            }
            ("cdrh_delay", Value::Bool(enabled)) => {
                let mode = if *enabled {
                    protocol::ObisMode::CdrhDelay
                } else {
                    protocol::ObisMode::ContinuousWave
                };
                self.send(protocol::ObisCommand::SetMode(mode))?;
                self.mode = mode;
                self.confirm_write_readback(&[
                    protocol::ObisCommand::QueryMode,
                    protocol::ObisCommand::QueryError,
                ])?;
                Ok(Value::Bool(self.mode == protocol::ObisMode::CdrhDelay))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid OBIS write {key}"),
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

    fn finish_emission(&mut self) {
        if self.mode == protocol::ObisMode::CdrhDelay && self.enabled {
            self.pending
                .push_back(DriverEvent::Event(Event::Log(LogEvent {
                    driver: Some(self.id),
                    message: "obis CDRH delay active; completion follows hardware state reply"
                        .into(),
                })));
        }
        self.pending
            .push_back(DriverEvent::Event(Event::Log(LogEvent {
                driver: Some(self.id),
                message: format!("obis emission {}", if self.enabled { "On" } else { "Off" }),
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
            let descriptor =
                self.descriptors_for().into_iter().next().ok_or_else(|| {
                    Error::new(ErrorCode::InvalidCommand, "missing OBIS descriptor")
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
            changed.insert(format!("{}:{}", (write.device.0).0, write.property), value);
        }

        Ok(Value::Map(changed))
    }

    fn timing_summary(&self, plan: &TimingPlan, action: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("action".into(), Value::String(action.into())),
            ("device".into(), Value::I64(self.laser.0 .0 as i64)),
            ("head_id".into(), Value::String(self.probe.head_id.clone())),
            ("enabled".into(), Value::Bool(self.enabled)),
            (
                "analog_modulation".into(),
                Value::Bool(self.analog_modulation),
            ),
            ("mode".into(), Value::String(self.mode.label().into())),
            ("power".into(), Value::OpticalPower(self.power_setpoint)),
            (
                "actual_power".into(),
                Value::OpticalPower(self.actual_power),
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
        command: protocol::ObisCommand,
    ) -> PhysicalTransaction {
        let line = protocol::encode(self.probe.index, &command);
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
    ) -> Result<Vec<protocol::ObisCommand>> {
        if device != self.laser {
            return Err(Error::new(ErrorCode::InvalidCommand, "unknown OBIS device"));
        }
        match kind {
            CapabilityKind::Dac => Ok(vec![protocol::ObisCommand::SetPower(dac_request_power(
                request,
            )?)]),
            CapabilityKind::TriggerSink => trigger_sink_commands(request),
            CapabilityKind::GenericCommand => {
                let CapabilityRequest::GenericCommand(request) = request else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "Coherent OBIS GenericCommand expects a GenericCommandRequest",
                    ));
                };
                self.validate_generic_command(request)?;
                Self::refresh_commands_for(&request.command)
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported OBIS invocation capability",
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
                let power = dac_request_power(&request)?;
                let value = self.write_property(device, "power", &Value::OpticalPower(power))?;
                self.emit_property(device, "power", value.clone());
                Ok(Value::Map(BTreeMap::from([
                    ("power".into(), value),
                    ("commands".into(), Value::I64(1)),
                ])))
            }
            CapabilityKind::TriggerSink => {
                let commands = trigger_sink_commands(&request)?;
                for command in &commands {
                    match command {
                        protocol::ObisCommand::SetEmission(enabled) => {
                            let value =
                                self.write_property(device, "enabled", &Value::Bool(*enabled))?;
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
                        "Coherent OBIS GenericCommand expects a GenericCommandRequest",
                    ));
                };
                self.apply_generic_command(request)
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported OBIS invocation capability",
            )),
        }
    }

    fn telemetry_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("device".into(), Value::I64(self.laser.0 .0 as i64)),
            ("head_id".into(), Value::String(self.probe.head_id.clone())),
            ("device_index".into(), Value::I64(self.probe.index as i64)),
            ("enabled".into(), Value::Bool(self.enabled)),
            (
                "analog_modulation".into(),
                Value::Bool(self.analog_modulation),
            ),
            ("mode".into(), Value::String(self.mode.label().into())),
            (
                "cdrh_delay".into(),
                Value::Bool(self.mode == protocol::ObisMode::CdrhDelay),
            ),
            ("power".into(), Value::OpticalPower(self.power_setpoint)),
            (
                "actual_power".into(),
                Value::OpticalPower(self.actual_power),
            ),
            (
                "min_power".into(),
                Value::OpticalPower(self.probe.min_power),
            ),
            (
                "max_power".into(),
                Value::OpticalPower(self.probe.max_power),
            ),
            (
                "wavelength".into(),
                Value::Wavelength(self.probe.wavelength),
            ),
            ("fault".into(), Value::String(self.fault.clone())),
            (
                "head_hours".into(),
                Value::TimeInterval(TimeInterval::from_hours(self.probe.hours)),
            ),
        ]))
    }
}

impl Driver for ObisDriver {
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
            label: "coherent-obis-serial".into(),
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
                    "prefixes".into(),
                    Value::String("SYST<n> and SOUR<n>".into()),
                ),
                (
                    "startup_readback_supported".into(),
                    Value::List(
                        protocol::probe_script(self.probe.index)
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
                        description: format!("obis read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("obis write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "obis laser state set".into(),
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
                            "unknown OBIS capability",
                        ));
                    };
                    if !capability.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "OBIS {:?} expects {:?}, got {:?}",
                                capability.kind,
                                capability.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    for command in self.invoke_transactions(*device, capability.kind, request)? {
                        physical_transactions.push(
                            self.timing_transaction("obis direct capability invocation", command),
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
                            "unknown OBIS capability",
                        ));
                    };
                    if !capability.accepts_request(&request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "OBIS {:?} expects {:?}, got {:?}",
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
                        message: format!("obis serial: {line}"),
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
                description: "obis timing arm summary".into(),
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
                    "obis timing start emission enable",
                    protocol::ObisCommand::SetEmission(true),
                ),
                PhysicalTransaction {
                    resource: Some(self.resource),
                    description: "obis timing start summary".into(),
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
                    "obis timing stop emission disable",
                    protocol::ObisCommand::SetEmission(false),
                ),
                PhysicalTransaction {
                    resource: Some(self.resource),
                    description: "obis timing stop summary".into(),
                    payload: with_applied(self.timing_summary(&armed.plan, "stop"), applied),
                },
            ],
        })
    }
}

fn parse_mode(mode: &str) -> Result<protocol::ObisMode> {
    match mode.trim().to_ascii_uppercase().as_str() {
        "CW" | "CONTINUOUSWAVE" | "CONTINUOUS_WAVE" => Ok(protocol::ObisMode::ContinuousWave),
        "CDRH" | "CDRHDELAY" | "CDRH_DELAY" => Ok(protocol::ObisMode::CdrhDelay),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unknown OBIS mode {other}"),
        )),
    }
}

fn dac_request_power(request: &CapabilityRequest) -> Result<OpticalPower> {
    match request {
        CapabilityRequest::Dac(request) => dac_value_power(&request.value),
        _ => Err(Error::new(
            ErrorCode::Unsupported,
            "OBIS Dac expects CapabilityRequest::Dac",
        )),
    }
}

fn dac_value_power(value: &Value) -> Result<OpticalPower> {
    match value {
        Value::OpticalPower(power) => Ok(*power),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            "OBIS Dac value must be OpticalPower",
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TriggerSinkAction {
    Enable,
    Disable,
    Pulse,
}

fn trigger_sink_commands(request: &CapabilityRequest) -> Result<Vec<protocol::ObisCommand>> {
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
                "OBIS TriggerSink expects None or CapabilityRequest::Trigger",
            ))
        }
    };
    Ok(match action {
        TriggerSinkAction::Enable => vec![protocol::ObisCommand::SetEmission(true)],
        TriggerSinkAction::Disable => vec![protocol::ObisCommand::SetEmission(false)],
        TriggerSinkAction::Pulse => vec![
            protocol::ObisCommand::SetEmission(true),
            protocol::ObisCommand::SetEmission(false),
        ],
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

fn mode_property() -> PropertySchema {
    let mut schema = property("mode", "Mode", ValueType::String, None, true, None);
    schema.enum_values = [
        protocol::ObisMode::ContinuousWave,
        protocol::ObisMode::CdrhDelay,
    ]
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

fn time_interval_prop(device: &DeviceConfig, key: &str) -> Option<TimeInterval> {
    match device.properties.get(key) {
        Some(Value::TimeInterval(value)) => Some(*value),
        _ => None,
    }
}

fn u8_prop(device: &DeviceConfig, key: &str) -> Option<u8> {
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
