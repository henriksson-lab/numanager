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

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TriggerMode {
        Internal,
        External,
    }

    impl TriggerMode {
        pub fn label(self) -> &'static str {
            match self {
                TriggerMode::Internal => "Internal",
                TriggerMode::External => "External",
            }
        }

        pub fn cli(self) -> &'static str {
            match self {
                TriggerMode::Internal => "0",
                TriggerMode::External => "1",
            }
        }

        pub fn from_label(value: &str) -> Option<Self> {
            match value {
                "Internal" | "internal" | "0" => Some(TriggerMode::Internal),
                "External" | "external" | "1" => Some(TriggerMode::External),
                _ => None,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct KuriosProbe {
        pub model: String,
        pub serial_number: String,
        pub firmware: String,
        pub min_wavelength: Wavelength,
        pub max_wavelength: Wavelength,
        pub min_bandwidth_nm: f64,
        pub max_bandwidth_nm: f64,
    }

    impl KuriosProbe {
        pub fn configured_fixture() -> Self {
            Self {
                model: "KURIOS-WB1".into(),
                serial_number: "KURIOS-FIXTURE-001".into(),
                firmware: "1.2".into(),
                min_wavelength: Wavelength::from_nanometers(420.0),
                max_wavelength: Wavelength::from_nanometers(730.0),
                min_bandwidth_nm: 10.0,
                max_bandwidth_nm: 40.0,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum KuriosCommand {
        QueryModel,
        QuerySerial,
        QueryFirmware,
        QueryStatus,
        QueryWavelength,
        SetWavelength(Wavelength),
        QueryBandwidth,
        SetBandwidth(f64),
        QueryOutput,
        SetOutput(bool),
        QueryTriggerMode,
        SetTriggerMode(TriggerMode),
    }

    pub fn encode(command: &KuriosCommand) -> String {
        match command {
            KuriosCommand::QueryModel => "MODEL?".into(),
            KuriosCommand::QuerySerial => "SERIAL?".into(),
            KuriosCommand::QueryFirmware => "VERSION?".into(),
            KuriosCommand::QueryStatus => "STATUS?".into(),
            KuriosCommand::QueryWavelength => "WL?".into(),
            KuriosCommand::SetWavelength(wavelength) => {
                format!("WL={:.0}", wavelength.nanometers())
            }
            KuriosCommand::QueryBandwidth => "BW?".into(),
            KuriosCommand::SetBandwidth(nm) => format!("BW={nm:.0}"),
            KuriosCommand::QueryOutput => "OUTPUT?".into(),
            KuriosCommand::SetOutput(enabled) => format!("OUTPUT={}", i64::from(*enabled)),
            KuriosCommand::QueryTriggerMode => "TRIG?".into(),
            KuriosCommand::SetTriggerMode(mode) => format!("TRIG={}", mode.cli()),
        }
    }

    pub fn parse_bool(reply: &str) -> Result<bool> {
        match reply.trim() {
            "1" | "ON" | "On" | "on" => Ok(true),
            "0" | "OFF" | "Off" | "off" => Ok(false),
            other => Err(Error::new(
                ErrorCode::Transport,
                format!("invalid KURIOS boolean reply {other}"),
            )),
        }
    }

    pub fn parse_wavelength(reply: &str) -> Result<Wavelength> {
        let nm = reply.trim().parse::<f64>().map_err(|_| {
            Error::new(
                ErrorCode::Transport,
                format!("invalid KURIOS wavelength reply {reply}"),
            )
        })?;
        Ok(Wavelength::from_nanometers(nm))
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct KuriosProbeResult {
        pub model: Option<String>,
        pub serial_number: Option<String>,
        pub firmware: Option<String>,
        pub status: Option<String>,
        pub wavelength: Option<Wavelength>,
        pub bandwidth_nm: Option<f64>,
        pub output_enabled: Option<bool>,
        pub trigger_mode: Option<TriggerMode>,
        pub replies: Vec<(String, String)>,
    }

    pub fn probe_commands() -> Vec<KuriosCommand> {
        vec![
            KuriosCommand::QueryModel,
            KuriosCommand::QuerySerial,
            KuriosCommand::QueryFirmware,
            KuriosCommand::QueryStatus,
            KuriosCommand::QueryWavelength,
            KuriosCommand::QueryBandwidth,
            KuriosCommand::QueryOutput,
            KuriosCommand::QueryTriggerMode,
        ]
    }

    pub fn probe_script() -> Vec<String> {
        probe_commands().iter().map(encode).collect()
    }

    pub fn execute_probe_script(
        serial: &mut dyn SerialIo,
        polls_per_command: usize,
    ) -> Result<KuriosProbeResult> {
        let mut codec = SerialLineCodec::new(SEND_ENDING, RECV_ENDING);
        let mut result = KuriosProbeResult {
            model: None,
            serial_number: None,
            firmware: None,
            status: None,
            wavelength: None,
            bandwidth_nm: None,
            output_enabled: None,
            trigger_mode: None,
            replies: Vec::new(),
        };
        for command in probe_commands() {
            let encoded = encode(&command);
            serial.write(&codec.encode(&encoded))?;
            let reply = read_line(serial, &mut codec, polls_per_command)?;
            apply_probe_reply(&mut result, &command, &reply)?;
            result.replies.push((encoded, reply));
        }
        Ok(result)
    }

    fn apply_probe_reply(
        result: &mut KuriosProbeResult,
        command: &KuriosCommand,
        reply: &str,
    ) -> Result<()> {
        match command {
            KuriosCommand::QueryModel => result.model = Some(clean_reply(reply)),
            KuriosCommand::QuerySerial => result.serial_number = Some(clean_reply(reply)),
            KuriosCommand::QueryFirmware => result.firmware = Some(clean_reply(reply)),
            KuriosCommand::QueryStatus => result.status = Some(clean_reply(reply)),
            KuriosCommand::QueryWavelength => result.wavelength = Some(parse_wavelength(reply)?),
            KuriosCommand::QueryBandwidth => {
                result.bandwidth_nm = Some(reply.trim().parse::<f64>().map_err(|_| {
                    Error::new(ErrorCode::Transport, "invalid KURIOS bandwidth reply")
                })?)
            }
            KuriosCommand::QueryOutput => result.output_enabled = Some(parse_bool(reply)?),
            KuriosCommand::QueryTriggerMode => {
                result.trigger_mode =
                    Some(TriggerMode::from_label(reply.trim()).ok_or_else(|| {
                        Error::new(ErrorCode::Transport, "invalid KURIOS trigger mode reply")
                    })?)
            }
            KuriosCommand::SetWavelength(_)
            | KuriosCommand::SetBandwidth(_)
            | KuriosCommand::SetOutput(_)
            | KuriosCommand::SetTriggerMode(_) => {}
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
            "timed out waiting for KURIOS probe reply",
        ))
    }

    fn clean_reply(reply: &str) -> String {
        reply.trim().trim_matches('"').into()
    }
}

pub struct KuriosDiscovery {
    next_id: DriverId,
    probes: Vec<KuriosConfiguredProbe>,
}

impl KuriosDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![KuriosConfiguredProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), "thorlabs_kurios" | "kurios"))
            .map(KuriosConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for KuriosDiscovery {
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
                            "KURIOS config requires serial_port when connect is true",
                        )
                    })?;
                    Box::new(KuriosDriver::serial(
                        id,
                        configured.probe,
                        endpoint.port_name,
                        endpoint.timeout_ms,
                    )?) as Box<dyn Driver>
                } else {
                    Box::new(KuriosDriver::configured(id, configured)) as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct KuriosConfiguredProbe {
    pub label: String,
    pub endpoint: Option<KuriosSerialEndpoint>,
    pub connect_real_transport: bool,
    probe: protocol::KuriosProbe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KuriosSerialEndpoint {
    pub port_name: String,
    pub timeout_ms: u64,
}

impl KuriosConfiguredProbe {
    pub fn fixture() -> Self {
        Self {
            label: "Configured Thorlabs KURIOS LCTF fixture".into(),
            endpoint: None,
            connect_real_transport: false,
            probe: protocol::KuriosProbe::configured_fixture(),
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::fixture();
        configured.label = if device.label.is_empty() {
            "Configured Thorlabs KURIOS LCTF".into()
        } else {
            device.label.clone()
        };
        configured.probe.model =
            string_prop(device, "model").unwrap_or_else(|| configured.probe.model.clone());
        configured.probe.serial_number = string_prop(device, "serial_number")
            .unwrap_or_else(|| configured.probe.serial_number.clone());
        configured.probe.firmware =
            string_prop(device, "firmware").unwrap_or_else(|| configured.probe.firmware.clone());
        configured.probe.min_wavelength =
            wavelength_config(device, "min_wavelength", "min_wavelength_nm")
                .unwrap_or(configured.probe.min_wavelength);
        configured.probe.max_wavelength =
            wavelength_config(device, "max_wavelength", "max_wavelength_nm")
                .unwrap_or(configured.probe.max_wavelength);
        if configured.probe.min_wavelength.nanometers()
            > configured.probe.max_wavelength.nanometers()
        {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "KURIOS min_wavelength must be <= max_wavelength",
            ));
        }
        configured.probe.min_bandwidth_nm =
            wavelength_config(device, "min_bandwidth", "min_bandwidth_nm")
                .map(|value| value.nanometers())
                .unwrap_or(configured.probe.min_bandwidth_nm);
        configured.probe.max_bandwidth_nm =
            wavelength_config(device, "max_bandwidth", "max_bandwidth_nm")
                .map(|value| value.nanometers())
                .unwrap_or(configured.probe.max_bandwidth_nm);
        if configured.probe.min_bandwidth_nm > configured.probe.max_bandwidth_nm {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "KURIOS min_bandwidth must be <= max_bandwidth",
            ));
        }
        configured.endpoint =
            string_prop(device, "serial_port").map(|port_name| KuriosSerialEndpoint {
                port_name,
                timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(100),
            });
        configured.connect_real_transport = bool_prop(device, "connect").unwrap_or(false);
        Ok(configured)
    }
}

pub struct KuriosDriver {
    id: DriverId,
    resource: ResourceId,
    device: DeviceId,
    probe: protocol::KuriosProbe,
    serial_port: Option<String>,
    serial_timeout_ms: u64,
    connected: bool,
    wavelength: Wavelength,
    bandwidth_nm: f64,
    output_enabled: bool,
    trigger_mode: protocol::TriggerMode,
    status: String,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
}

impl KuriosDriver {
    pub fn configured_fixture(id: DriverId) -> Self {
        Self::configured(id, KuriosConfiguredProbe::fixture())
    }

    pub fn configured(id: DriverId, configured: KuriosConfiguredProbe) -> Self {
        Self::new_with_transport_metadata(
            id,
            configured.probe,
            configured.endpoint,
            false,
            Box::new(ScriptedSerial::new()),
        )
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(
        id: DriverId,
        probe: protocol::KuriosProbe,
        port_name: impl Into<String>,
        timeout_ms: u64,
    ) -> Result<Self> {
        let port_name = port_name.into();
        let mut serial = numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(port_name.clone(), protocol::BAUD)
                .timeout(Duration::from_millis(timeout_ms)),
        )?;
        let probe_result = protocol::execute_probe_script(&mut serial, 4)?;
        Ok(Self::new_with_transport_metadata(
            id,
            probe,
            Some(KuriosSerialEndpoint {
                port_name,
                timeout_ms,
            }),
            true,
            Box::new(serial),
        )
        .with_probe_result(probe_result))
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(
        _id: DriverId,
        _probe: protocol::KuriosProbe,
        _port_name: impl Into<String>,
        _timeout_ms: u64,
    ) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Thorlabs KURIOS real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(id: DriverId, probe: protocol::KuriosProbe, serial: Box<dyn SerialIo>) -> Self {
        Self::new_with_transport_metadata(id, probe, None, false, serial)
    }

    fn new_with_transport_metadata(
        id: DriverId,
        probe: protocol::KuriosProbe,
        endpoint: Option<KuriosSerialEndpoint>,
        connected: bool,
        serial: Box<dyn SerialIo>,
    ) -> Self {
        let serial_port = endpoint.as_ref().map(|endpoint| endpoint.port_name.clone());
        let serial_timeout_ms = endpoint
            .as_ref()
            .map(|endpoint| endpoint.timeout_ms)
            .unwrap_or(100);
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 981)),
            device: DeviceId(NodeId(id.0 * 1000 + 990)),
            serial_port,
            serial_timeout_ms,
            connected,
            wavelength: probe.min_wavelength,
            bandwidth_nm: probe.min_bandwidth_nm,
            output_enabled: false,
            trigger_mode: protocol::TriggerMode::Internal,
            status: "ready".into(),
            probe,
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            codec: SerialLineCodec::new(protocol::SEND_ENDING, protocol::RECV_ENDING),
        }
    }

    #[cfg(feature = "os-serial")]
    fn with_probe_result(mut self, probe_result: protocol::KuriosProbeResult) -> Self {
        if let Some(model) = probe_result.model {
            self.probe.model = model;
        }
        if let Some(serial_number) = probe_result.serial_number {
            self.probe.serial_number = serial_number;
        }
        if let Some(firmware) = probe_result.firmware {
            self.probe.firmware = firmware;
        }
        if let Some(wavelength) = probe_result.wavelength {
            self.wavelength = wavelength;
        }
        if let Some(bandwidth_nm) = probe_result.bandwidth_nm {
            self.bandwidth_nm = bandwidth_nm;
        }
        if let Some(output_enabled) = probe_result.output_enabled {
            self.output_enabled = output_enabled;
        }
        if let Some(trigger_mode) = probe_result.trigger_mode {
            self.trigger_mode = trigger_mode;
        }
        if let Some(status) = probe_result.status {
            self.status = status;
        }
        self
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn send(&mut self, command: protocol::KuriosCommand) -> Result<()> {
        let line = protocol::encode(&command);
        self.serial.write(&self.codec.encode(&line))
    }

    fn query_for_property(key: &str) -> Option<protocol::KuriosCommand> {
        match key {
            "wavelength" => Some(protocol::KuriosCommand::QueryWavelength),
            "bandwidth" => Some(protocol::KuriosCommand::QueryBandwidth),
            "output_enabled" => Some(protocol::KuriosCommand::QueryOutput),
            "trigger_mode" => Some(protocol::KuriosCommand::QueryTriggerMode),
            "status" => Some(protocol::KuriosCommand::QueryStatus),
            "firmware" => Some(protocol::KuriosCommand::QueryFirmware),
            _ => None,
        }
    }

    fn read_query_reply(&mut self, query: &protocol::KuriosCommand) -> Result<()> {
        let bytes = self.serial.read_available()?;
        for line in self.codec.push(&bytes) {
            self.apply_readback_reply(query, &line)?;
            return Ok(());
        }
        Ok(())
    }

    fn refresh_property_readback(&mut self, key: &str) -> Result<()> {
        if let Some(query) = Self::query_for_property(key) {
            self.send(query.clone())?;
            self.read_query_reply(&query)?;
        }
        Ok(())
    }

    fn refresh_commands_for(command: &str) -> Result<Vec<protocol::KuriosCommand>> {
        match command {
            "refresh_telemetry" => Ok(vec![
                protocol::KuriosCommand::QueryStatus,
                protocol::KuriosCommand::QueryWavelength,
                protocol::KuriosCommand::QueryBandwidth,
                protocol::KuriosCommand::QueryOutput,
                protocol::KuriosCommand::QueryTriggerMode,
                protocol::KuriosCommand::QueryFirmware,
            ]),
            "refresh_identity" => Ok(vec![
                protocol::KuriosCommand::QueryModel,
                protocol::KuriosCommand::QuerySerial,
                protocol::KuriosCommand::QueryFirmware,
            ]),
            "refresh_wavelength" => Ok(vec![protocol::KuriosCommand::QueryWavelength]),
            "refresh_bandwidth" => Ok(vec![protocol::KuriosCommand::QueryBandwidth]),
            "refresh_output" => Ok(vec![protocol::KuriosCommand::QueryOutput]),
            "refresh_status" => Ok(vec![
                protocol::KuriosCommand::QueryStatus,
                protocol::KuriosCommand::QueryOutput,
                protocol::KuriosCommand::QueryTriggerMode,
            ]),
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "KURIOS GenericCommand supports refresh_telemetry, refresh_identity, refresh_wavelength, refresh_bandwidth, refresh_output, and refresh_status; got {other}"
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
                "KURIOS GenericCommand does not take parameters",
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
            self.read_query_reply(command)?;
        }
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("commands".into(), Value::I64(commands.len() as i64)),
            ("telemetry".into(), self.telemetry_summary()),
            (
                "completion_basis".into(),
                Value::String("KURIOS query readback".into()),
            ),
        ])))
    }

    fn apply_readback_reply(&mut self, query: &protocol::KuriosCommand, reply: &str) -> Result<()> {
        match query {
            protocol::KuriosCommand::QueryWavelength => {
                let value = protocol::parse_wavelength(reply)?;
                if self.wavelength != value {
                    self.wavelength = value;
                    self.emit_property(self.device, "wavelength", Value::Wavelength(value));
                }
            }
            protocol::KuriosCommand::QueryBandwidth => {
                let value = reply.trim().parse::<f64>().map_err(|_| {
                    Error::new(ErrorCode::Transport, "invalid KURIOS bandwidth reply")
                })?;
                if self.bandwidth_nm != value {
                    self.bandwidth_nm = value;
                    self.emit_property(
                        self.device,
                        "bandwidth",
                        Value::Wavelength(Wavelength::from_nanometers(value)),
                    );
                }
            }
            protocol::KuriosCommand::QueryOutput => {
                let value = protocol::parse_bool(reply)?;
                if self.output_enabled != value {
                    self.output_enabled = value;
                    self.emit_property(self.device, "output_enabled", Value::Bool(value));
                }
            }
            protocol::KuriosCommand::QueryTriggerMode => {
                let value = protocol::TriggerMode::from_label(reply.trim()).ok_or_else(|| {
                    Error::new(ErrorCode::Transport, "invalid KURIOS trigger mode reply")
                })?;
                if self.trigger_mode != value {
                    self.trigger_mode = value;
                    self.emit_property(
                        self.device,
                        "trigger_mode",
                        Value::String(value.label().into()),
                    );
                }
            }
            protocol::KuriosCommand::QueryStatus => {
                let value = reply.trim().trim_matches('"').to_string();
                if self.status != value {
                    self.status = value.clone();
                    self.emit_property(self.device, "status", Value::String(value));
                }
            }
            protocol::KuriosCommand::QueryFirmware => {
                let value = reply.trim().trim_matches('"').to_string();
                if self.probe.firmware != value {
                    self.probe.firmware = value.clone();
                    self.emit_property(self.device, "firmware", Value::String(value));
                }
            }
            protocol::KuriosCommand::QueryModel => {
                let value = reply.trim().trim_matches('"').to_string();
                if self.probe.model != value {
                    self.probe.model = value;
                }
            }
            protocol::KuriosCommand::QuerySerial => {
                let value = reply.trim().trim_matches('"').to_string();
                if self.probe.serial_number != value {
                    self.probe.serial_number = value;
                }
            }
            protocol::KuriosCommand::SetWavelength(_)
            | protocol::KuriosCommand::SetBandwidth(_)
            | protocol::KuriosCommand::SetOutput(_)
            | protocol::KuriosCommand::SetTriggerMode(_) => {}
        }
        Ok(())
    }

    fn telemetry_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("device".into(), Value::I64(self.device.0 .0 as i64)),
            ("model".into(), Value::String(self.probe.model.clone())),
            (
                "serial_number".into(),
                Value::String(self.probe.serial_number.clone()),
            ),
            (
                "firmware".into(),
                Value::String(self.probe.firmware.clone()),
            ),
            ("status".into(), Value::String(self.status.clone())),
            ("wavelength".into(), Value::Wavelength(self.wavelength)),
            (
                "bandwidth".into(),
                Value::Wavelength(Wavelength::from_nanometers(self.bandwidth_nm)),
            ),
            ("output_enabled".into(), Value::Bool(self.output_enabled)),
            (
                "trigger_mode".into(),
                Value::String(self.trigger_mode.label().into()),
            ),
        ]))
    }

    fn invoke_transactions(
        &self,
        device: DeviceId,
        kind: CapabilityKind,
        request: &CapabilityRequest,
    ) -> Result<Value> {
        if device != self.device {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown KURIOS device",
            ));
        }
        match kind {
            CapabilityKind::TriggerSink => Ok(Value::List(
                self.trigger_sink_values(request)?
                    .into_iter()
                    .map(Value::Bool)
                    .collect(),
            )),
            CapabilityKind::GenericCommand => {
                let CapabilityRequest::GenericCommand(request) = request else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "KURIOS GenericCommand expects a GenericCommandRequest",
                    ));
                };
                self.validate_generic_command(request)?;
                Ok(Value::List(
                    Self::refresh_commands_for(&request.command)?
                        .into_iter()
                        .map(|command| Value::String(protocol::encode(&command)))
                        .collect(),
                ))
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported KURIOS capability",
            )),
        }
    }

    fn apply_invoke(
        &mut self,
        device: DeviceId,
        kind: CapabilityKind,
        request: CapabilityRequest,
    ) -> Result<Value> {
        if device != self.device {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown KURIOS device",
            ));
        }
        match kind {
            CapabilityKind::TriggerSink => {
                let values = self.trigger_sink_values(&request)?;
                for enabled in &values {
                    let _ =
                        self.write_property(device, "output_enabled", &Value::Bool(*enabled))?;
                }
                Ok(Value::Map(BTreeMap::from([
                    ("output_enabled".into(), Value::Bool(self.output_enabled)),
                    ("steps".into(), Value::I64(values.len() as i64)),
                ])))
            }
            CapabilityKind::GenericCommand => {
                let CapabilityRequest::GenericCommand(request) = request else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "KURIOS GenericCommand expects a GenericCommandRequest",
                    ));
                };
                self.apply_generic_command(request)
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported KURIOS capability",
            )),
        }
    }

    fn descriptor(&self) -> DeviceDescriptor {
        DeviceDescriptor {
            id: self.device,
            driver: self.id,
            label: "thorlabs-kurios-lctf".into(),
            vendor: Some("Thorlabs".into()),
            model: Some(self.probe.model.clone()),
            serial: Some(self.probe.serial_number.clone()),
            kinds: vec![
                "filter.tunable".into(),
                "lctf".into(),
                "light.filter".into(),
                "serial.ascii".into(),
            ],
            properties: vec![
                wavelength_property(&self.probe),
                bandwidth_property(&self.probe),
                property(
                    "output_enabled",
                    "Output",
                    ValueType::Bool,
                    None,
                    true,
                    None,
                ),
                trigger_mode_property(),
                property("status", "Status", ValueType::String, None, false, None),
                property("firmware", "Firmware", ValueType::String, None, false, None),
            ],
            metadata: BTreeMap::from([
                (
                    "protocol".into(),
                    Value::String("KURIOS Keyword=argument / Keyword? CLI".into()),
                ),
                (
                    "min_wavelength".into(),
                    Value::Wavelength(self.probe.min_wavelength),
                ),
                (
                    "max_wavelength".into(),
                    Value::Wavelength(self.probe.max_wavelength),
                ),
                (
                    "firmware".into(),
                    Value::String(self.probe.firmware.clone()),
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
        }
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device != self.device {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown KURIOS device",
            ));
        }
        match key {
            "wavelength" => Ok(Value::Wavelength(self.wavelength)),
            "bandwidth" => Ok(Value::Wavelength(Wavelength::from_nanometers(
                self.bandwidth_nm,
            ))),
            "output_enabled" => Ok(Value::Bool(self.output_enabled)),
            "trigger_mode" => Ok(Value::String(self.trigger_mode.label().into())),
            "status" => Ok(Value::String(self.status.clone())),
            "firmware" => Ok(Value::String(self.probe.firmware.clone())),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown KURIOS property {key}"),
            )),
        }
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        let schema = self
            .descriptor()
            .properties
            .into_iter()
            .find(|property| property.key == key)
            .ok_or_else(|| Error::new(ErrorCode::InvalidProperty, "unknown property"))?;
        if device != self.device {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown KURIOS device",
            ));
        }
        if !schema.writable {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "property is read-only",
            ));
        }
        schema.validate(value)?;
        match (key, value) {
            ("wavelength", Value::Wavelength(wavelength)) => {
                let nm = wavelength.nanometers();
                let min = self.probe.min_wavelength.nanometers();
                let max = self.probe.max_wavelength.nanometers();
                if nm < min || nm > max {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "KURIOS wavelength is outside the configured range",
                    ));
                }
            }
            ("bandwidth", Value::Wavelength(bandwidth)) => {
                let nm = bandwidth.nanometers();
                if nm < self.probe.min_bandwidth_nm || nm > self.probe.max_bandwidth_nm {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "KURIOS bandwidth is outside the configured range",
                    ));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: &Value) -> Result<Value> {
        self.validate_write(device, key, value)?;
        let value = match (key, value) {
            ("wavelength", Value::Wavelength(wavelength)) => {
                self.send(protocol::KuriosCommand::SetWavelength(*wavelength))?;
                self.wavelength = *wavelength;
                self.refresh_property_readback(key)?;
                Value::Wavelength(self.wavelength)
            }
            ("bandwidth", Value::Wavelength(bandwidth)) => {
                self.send(protocol::KuriosCommand::SetBandwidth(
                    bandwidth.nanometers(),
                ))?;
                self.bandwidth_nm = bandwidth.nanometers();
                self.refresh_property_readback(key)?;
                Value::Wavelength(Wavelength::from_nanometers(self.bandwidth_nm))
            }
            ("output_enabled", Value::Bool(enabled)) => {
                self.send(protocol::KuriosCommand::SetOutput(*enabled))?;
                self.output_enabled = *enabled;
                self.refresh_property_readback(key)?;
                Value::Bool(self.output_enabled)
            }
            ("trigger_mode", Value::String(label)) => {
                let mode = protocol::TriggerMode::from_label(label).ok_or_else(|| {
                    Error::new(ErrorCode::InvalidProperty, "unknown KURIOS trigger mode")
                })?;
                self.send(protocol::KuriosCommand::SetTriggerMode(mode))?;
                self.trigger_mode = mode;
                self.refresh_property_readback(key)?;
                Value::String(self.trigger_mode.label().into())
            }
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("invalid KURIOS write {key}"),
                ))
            }
        };
        self.emit_property(device, key, value.clone());
        Ok(value)
    }

    fn apply_state_set(&mut self, set: StateSet) -> Result<Value> {
        for write in &set.writes {
            self.validate_write(write.device, &write.property, &write.value)?;
        }

        let mut changed = BTreeMap::new();
        for write in set.writes {
            let value = self.write_property(write.device, &write.property, &write.value)?;
            changed.insert(write.property, value);
        }
        Ok(Value::Map(changed))
    }

    fn trigger_sink_values(&self, request: &CapabilityRequest) -> Result<Vec<bool>> {
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
                    "KURIOS TriggerSink expects None or CapabilityRequest::Trigger",
                ))
            }
        };
        Ok(match action {
            TriggerSinkAction::Enable => vec![true],
            TriggerSinkAction::Disable => vec![false],
            TriggerSinkAction::Pulse => vec![true, false],
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
                "unknown KURIOS capability",
            ));
        };
        if !capability.accepts_request(&request) {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "KURIOS {:?} expects {:?}, got {:?}",
                    capability.kind,
                    capability.preferred_request_kind(),
                    request.request_kind()
                ),
            ));
        }
        self.apply_invoke(device, capability.kind, request)
    }

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| sequence.device == self.device)
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequences(plan) {
            if sequence.values.is_empty() {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "KURIOS timing sequence must contain at least one value",
                ));
            }
            let descriptor = self.descriptor();
            let Some(schema) = descriptor
                .properties
                .iter()
                .find(|property| property.key == sequence.property)
            else {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("KURIOS timing does not support {}", sequence.property),
                ));
            };
            if !schema.sequenceable {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("KURIOS property {} is not sequenceable", sequence.property),
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
            (
                "device_participant".into(),
                Value::Bool(plan.participants.contains(&self.device)),
            ),
            ("output_enabled".into(), Value::Bool(self.output_enabled)),
            (
                "trigger_mode".into(),
                Value::String(self.trigger_mode.label().into()),
            ),
            (
                "sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
        ]))
    }

    fn apply_timing_sequence_step(&mut self, plan: &TimingPlan, first: bool) -> Result<Value> {
        let mut changed = BTreeMap::new();
        let sequences = self
            .local_timing_sequences(plan)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        for sequence in sequences {
            let value = (if first {
                sequence.values.first()
            } else {
                sequence.values.last()
            })
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidCommand,
                    "KURIOS timing sequence must contain at least one value",
                )
            })?
            .clone();
            let value = self.write_property(sequence.device, &sequence.property, &value)?;
            self.emit_property(sequence.device, &sequence.property, value.clone());
            changed.insert(
                format!("{}:{}", (sequence.device.0).0, sequence.property),
                value,
            );
        }
        Ok(Value::Map(changed))
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

impl Driver for KuriosDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        vec![self.descriptor()]
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "thorlabs-kurios-serial".into(),
            kind: "serial".into(),
            metadata: BTreeMap::from([
                ("baud_rate".into(), Value::I64(protocol::BAUD as i64)),
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
                    Value::String("CLI command acceptance followed by status/readback".into()),
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
        if device == self.device {
            vec![
                capability(1, device, CapabilityKind::TriggerSink),
                capability(2, device, CapabilityKind::GenericCommand),
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
                        description: format!("kurios query {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("kurios write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "kurios remultiplexed filter state set".into(),
                        payload: Value::List(
                            set.writes
                                .iter()
                                .map(|write| Value::String(write.property.clone()))
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
                            Error::new(ErrorCode::Unsupported, "unknown KURIOS capability")
                        })?;
                    if !candidate.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "KURIOS {:?} expects {:?}, got {:?}",
                                candidate.kind,
                                candidate.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("kurios invoke {:?}", candidate.kind),
                        payload: self.invoke_transactions(*device, candidate.kind, request)?,
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
                    if let Some(query) = Self::query_for_property(&key) {
                        self.send(query.clone())?;
                        self.read_query_reply(&query)?;
                    }
                    last = self.read_property(device, &key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    last = self.write_property(device, &key, &value)?;
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
                _ => unreachable!(),
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
                        message: format!("kurios serial: {line}"),
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
                description: "kurios timing arm summary".into(),
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
                description: "kurios timing start output".into(),
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
                description: "kurios timing stop output".into(),
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

fn wavelength_property(probe: &protocol::KuriosProbe) -> PropertySchema {
    property(
        "wavelength",
        "Center wavelength",
        ValueType::Wavelength,
        None,
        true,
        Some(Range {
            min: Value::Wavelength(probe.min_wavelength),
            max: Value::Wavelength(probe.max_wavelength),
        }),
    )
}

fn bandwidth_property(probe: &protocol::KuriosProbe) -> PropertySchema {
    property(
        "bandwidth",
        "Bandwidth",
        ValueType::Wavelength,
        None,
        true,
        Some(Range {
            min: Value::Wavelength(Wavelength::from_nanometers(probe.min_bandwidth_nm)),
            max: Value::Wavelength(Wavelength::from_nanometers(probe.max_bandwidth_nm)),
        }),
    )
}

fn trigger_mode_property() -> PropertySchema {
    let mut schema = property(
        "trigger_mode",
        "Trigger mode",
        ValueType::String,
        None,
        true,
        None,
    );
    schema.enum_values = [
        protocol::TriggerMode::Internal,
        protocol::TriggerMode::External,
    ]
    .into_iter()
    .map(|mode| EnumValue {
        value: Value::String(mode.label().into()),
        label: mode.label().into(),
    })
    .collect();
    schema
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
        sequenceable: key == "wavelength" || key == "bandwidth" || key == "output_enabled",
        hardware_address: None,
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

fn wavelength_config(device: &DeviceConfig, key: &str, legacy_key: &str) -> Option<Wavelength> {
    match device.properties.get(key) {
        Some(Value::Wavelength(value)) => Some(*value),
        _ => f64_prop(device, legacy_key).map(Wavelength::from_nanometers),
    }
}

fn f64_prop(device: &DeviceConfig, key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn u64_prop(device: &DeviceConfig, key: &str) -> Option<u64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => (*value >= 0).then_some(*value as u64),
        Some(Value::F64(value)) if value.is_finite() && *value >= 0.0 => Some(*value as u64),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TriggerSinkAction {
    Enable,
    Disable,
    Pulse,
}
