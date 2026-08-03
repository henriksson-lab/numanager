use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::{LineEnding, SerialIo, SerialLineCodec};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    pub const BAUD: u32 = 9_600;
    pub const DATA_BITS: u8 = 8;
    pub const STOP_BITS: u8 = 1;
    pub const PARITY: &str = "none";
    pub const LINE_ENDING: &str = "CRLF";
    pub const CHANNELS: usize = 7;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TriggerSource {
        Internal,
        External,
    }

    impl TriggerSource {
        pub fn code(self) -> u8 {
            match self {
                TriggerSource::Internal => 0,
                TriggerSource::External => 1,
            }
        }

        pub fn name(self) -> &'static str {
            match self {
                TriggerSource::Internal => "Internal",
                TriggerSource::External => "External",
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TriggerLogic {
        ActiveLow,
        ActiveHigh,
    }

    impl TriggerLogic {
        pub fn code(self) -> u8 {
            match self {
                TriggerLogic::ActiveLow => 0,
                TriggerLogic::ActiveHigh => 1,
            }
        }

        pub fn name(self) -> &'static str {
            match self {
                TriggerLogic::ActiveLow => "ActiveLow",
                TriggerLogic::ActiveHigh => "ActiveHigh",
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TriggerResistor {
        PullDown,
        PullUp,
    }

    impl TriggerResistor {
        pub fn code(self) -> u8 {
            match self {
                TriggerResistor::PullDown => 0,
                TriggerResistor::PullUp => 1,
            }
        }

        pub fn name(self) -> &'static str {
            match self {
                TriggerResistor::PullDown => "PullDown",
                TriggerResistor::PullUp => "PullUp",
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum OutputMode {
        ConstantCurrent,
        ConstantOpticalPower,
    }

    impl OutputMode {
        pub fn code(self) -> u8 {
            match self {
                OutputMode::ConstantCurrent => 0,
                OutputMode::ConstantOpticalPower => 1,
            }
        }

        pub fn name(self) -> &'static str {
            match self {
                OutputMode::ConstantCurrent => "ConstantCurrent",
                OutputMode::ConstantOpticalPower => "ConstantOpticalPower",
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum NijiCommand {
        SetChannelEnabled {
            channel: u8,
            enabled: bool,
        },
        SetChannelIntensity {
            channel: u8,
            percent: u8,
        },
        SetTrigger {
            source: TriggerSource,
            logic: TriggerLogic,
            resistor: TriggerResistor,
        },
        SetOutputMode(OutputMode),
        QueryStatus,
        QueryTemperatures,
    }

    pub fn encode(command: &NijiCommand) -> String {
        match command {
            NijiCommand::SetChannelEnabled { channel, enabled } => {
                format!("D,{channel},{}", u8::from(*enabled))
            }
            NijiCommand::SetChannelIntensity { channel, percent } => {
                format!("d,{channel},{percent}")
            }
            NijiCommand::SetTrigger {
                source,
                logic,
                resistor,
            } => format!("TTL,{},{},{}", source.code(), logic.code(), resistor.code()),
            NijiCommand::SetOutputMode(mode) => format!("CC,{},", mode.code()),
            NijiCommand::QueryStatus => "?".into(),
            NijiCommand::QueryTemperatures => "r".into(),
        }
    }
}

#[derive(Debug, Default)]
struct NijiParsedReply {
    firmware_version: Option<String>,
    output_temperature: Option<Temperature>,
    ambient_temperature: Option<Temperature>,
    error_code: Option<i64>,
}

fn parse_known_reply_lines(reply: &str) -> NijiParsedReply {
    let mut parsed = NijiParsedReply::default();
    for line in reply.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Some(value) = line.strip_prefix("Firmware,") {
            let value = value.trim();
            if !value.is_empty() {
                parsed.firmware_version = Some(value.into());
            }
        } else if let Some(value) = parse_csv_f64(line, "R2,") {
            parsed.ambient_temperature = Some(Temperature::from_celsius(value));
        } else if let Some(value) = parse_csv_f64(line, "R,") {
            parsed.output_temperature = Some(Temperature::from_celsius(value));
        } else if let Some(value) = parse_csv_i64(line, "Status,") {
            parsed.error_code = Some(value);
        }
    }
    parsed
}

fn parse_csv_f64(line: &str, prefix: &str) -> Option<f64> {
    line.strip_prefix(prefix)?
        .split(',')
        .next()?
        .trim()
        .parse()
        .ok()
}

fn parse_csv_i64(line: &str, prefix: &str) -> Option<i64> {
    line.strip_prefix(prefix)?
        .split(',')
        .next()?
        .trim()
        .parse()
        .ok()
}

#[derive(Debug, Clone)]
pub struct NijiConfiguredProbe {
    label: String,
    serial_port: Option<String>,
    serial_timeout_ms: u64,
    connect_real_transport: bool,
    product: String,
    serial_number: String,
    firmware_version: String,
    channel_labels: Vec<String>,
    wavelengths: Vec<Wavelength>,
    global_enabled: bool,
    global_intensity: Ratio,
    channel_enabled: Vec<bool>,
    channel_intensity: Vec<Ratio>,
    trigger_source: protocol::TriggerSource,
    trigger_logic: protocol::TriggerLogic,
    trigger_resistor: protocol::TriggerResistor,
    output_mode: protocol::OutputMode,
    output_temperature: Temperature,
    ambient_temperature: Temperature,
    error_code: i64,
    status_reply: String,
    temperature_reply: String,
}

pub struct NijiDiscovery {
    next_id: DriverId,
    probes: Vec<NijiConfiguredProbe>,
}

impl NijiDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![NijiConfiguredProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), "bluebox_niji" | "niji"))
            .map(NijiConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for NijiDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver: Box<dyn Driver> = if configured.connect_real_transport {
                    Box::new(NijiDriver::serial(id, configured)?)
                } else {
                    Box::new(NijiDriver::configured(id, configured))
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

impl NijiConfiguredProbe {
    pub fn fixture() -> Self {
        let channel_labels = vec![
            "395/14".into(),
            "445/15".into(),
            "470/25".into(),
            "515/30".into(),
            "575/25".into(),
            "630/20".into(),
            "745/30".into(),
        ];
        let wavelengths = [395.0, 445.0, 470.0, 515.0, 575.0, 630.0, 745.0]
            .into_iter()
            .map(Wavelength::from_nanometers)
            .collect::<Vec<_>>();
        Self {
            label: "Configured Bluebox Optics niji".into(),
            serial_port: None,
            serial_timeout_ms: 500,
            connect_real_transport: false,
            product: "Bluebox Optics niji LED illuminator".into(),
            serial_number: "NIJI-CONFIG-0001".into(),
            firmware_version: "V2.101.000 configured".into(),
            channel_labels,
            wavelengths,
            global_enabled: false,
            global_intensity: Ratio::from_percent(100.0),
            channel_enabled: vec![false; protocol::CHANNELS],
            channel_intensity: vec![Ratio::from_percent(100.0); protocol::CHANNELS],
            trigger_source: protocol::TriggerSource::Internal,
            trigger_logic: protocol::TriggerLogic::ActiveHigh,
            trigger_resistor: protocol::TriggerResistor::PullUp,
            output_mode: protocol::OutputMode::ConstantCurrent,
            output_temperature: Temperature::from_celsius(22.5),
            ambient_temperature: Temperature::from_celsius(22.0),
            error_code: 0,
            status_reply: String::new(),
            temperature_reply: String::new(),
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::fixture();
        if !device.label.is_empty() {
            configured.label = device.label.clone();
        }
        configured.serial_port = string_prop(device, "serial_port");
        configured.serial_timeout_ms =
            u64_prop(device, "serial_timeout_ms").unwrap_or(configured.serial_timeout_ms);
        configured.connect_real_transport =
            bool_prop(device, "connect").unwrap_or(configured.connect_real_transport);
        configured.product = string_prop(device, "product").unwrap_or(configured.product);
        configured.serial_number =
            string_prop(device, "serial_number").unwrap_or(configured.serial_number);
        configured.firmware_version =
            string_prop(device, "firmware_version").unwrap_or(configured.firmware_version);
        configured.global_enabled =
            bool_prop(device, "enabled").unwrap_or(configured.global_enabled);
        configured.global_intensity =
            ratio_prop(device, "global_intensity").unwrap_or(configured.global_intensity);
        configured.trigger_source =
            trigger_source_prop(device, "trigger_source").unwrap_or(configured.trigger_source);
        configured.trigger_logic =
            trigger_logic_prop(device, "trigger_logic").unwrap_or(configured.trigger_logic);
        configured.trigger_resistor = trigger_resistor_prop(device, "trigger_resistor")
            .unwrap_or(configured.trigger_resistor);
        configured.output_mode =
            output_mode_prop(device, "output_mode").unwrap_or(configured.output_mode);
        configured.output_temperature =
            temperature_prop(device, "output_temperature").unwrap_or(configured.output_temperature);
        configured.ambient_temperature = temperature_prop(device, "ambient_temperature")
            .unwrap_or(configured.ambient_temperature);
        configured.error_code = i64_prop(device, "error_code").unwrap_or(configured.error_code);
        configured.status_reply =
            string_prop(device, "status_reply").unwrap_or(configured.status_reply);
        configured.temperature_reply =
            string_prop(device, "temperature_reply").unwrap_or(configured.temperature_reply);
        configured.apply_parsed_reply(parse_known_reply_lines(&configured.status_reply));
        configured.apply_parsed_reply(parse_known_reply_lines(&configured.temperature_reply));

        for index in 0..protocol::CHANNELS {
            let channel = index + 1;
            if let Some(value) = string_prop(device, &format!("channel_{channel}_label")) {
                configured.channel_labels[index] = value;
            }
            if let Some(value) = wavelength_prop(device, &format!("channel_{channel}_wavelength")) {
                configured.wavelengths[index] = value;
            }
            if let Some(value) = bool_prop(device, &format!("channel_{channel}_enabled")) {
                configured.channel_enabled[index] = value;
            }
            if let Some(value) = ratio_prop(device, &format!("channel_{channel}_intensity")) {
                configured.channel_intensity[index] = value;
            }
        }
        validate_ratios(&configured)?;
        Ok(configured)
    }

    fn apply_parsed_reply(&mut self, parsed: NijiParsedReply) {
        if let Some(firmware) = parsed.firmware_version {
            self.firmware_version = firmware;
        }
        if let Some(temperature) = parsed.output_temperature {
            self.output_temperature = temperature;
        }
        if let Some(temperature) = parsed.ambient_temperature {
            self.ambient_temperature = temperature;
        }
        if let Some(error_code) = parsed.error_code {
            self.error_code = error_code;
        }
    }
}

pub struct NijiDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    channels: Vec<DeviceId>,
    configured: NijiConfiguredProbe,
    last_transaction: Value,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Option<Box<dyn SerialIo>>,
    codec: SerialLineCodec,
}

impl NijiDriver {
    pub fn configured(id: DriverId, configured: NijiConfiguredProbe) -> Self {
        Self::new(id, configured, None)
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: NijiConfiguredProbe) -> Result<Self> {
        let port_name = configured.serial_port.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "niji config requires serial_port when connect is true",
            )
        })?;
        let serial = Box::new(numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(port_name, protocol::BAUD)
                .timeout(Duration::from_millis(configured.serial_timeout_ms)),
        )?);
        let mut driver = Self::new(id, configured, Some(serial));
        driver.refresh_status()?;
        driver.refresh_temperatures()?;
        Ok(driver)
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _configured: NijiConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "niji real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(
        id: DriverId,
        configured: NijiConfiguredProbe,
        serial: Option<Box<dyn SerialIo>>,
    ) -> Self {
        let base = id.0 * 1000 + 500;
        Self {
            id,
            resource: ResourceId(NodeId(base)),
            hub: DeviceId(NodeId(base + 1)),
            channels: (0..protocol::CHANNELS)
                .map(|index| DeviceId(NodeId(base + 2 + index as u64)))
                .collect(),
            configured,
            last_transaction: Value::Map(BTreeMap::new()),
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            codec: SerialLineCodec::new(LineEnding::CrLf, LineEnding::CrLf),
        }
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn channel_index(&self, device: DeviceId) -> Option<usize> {
        self.channels.iter().position(|id| *id == device)
    }

    fn owns_device(&self, device: DeviceId) -> bool {
        device == self.hub || self.channel_index(device).is_some()
    }

    fn record(&mut self, command: protocol::NijiCommand, action: &str) -> Result<String> {
        let line = protocol::encode(&command);
        let mut reply = String::new();
        let completion_basis = if self.serial.is_some() {
            let bytes = self.codec.encode(&line);
            self.active_serial()?.write(&bytes)?;
            reply = self.read_line_until_timeout()?;
            "serial write and line readback"
        } else {
            "configured command acceptance; known-prefix reply parsing only"
        };
        self.last_transaction = Value::Map(BTreeMap::from([
            ("action".into(), Value::String(action.into())),
            (
                "completion_basis".into(),
                Value::String(completion_basis.into()),
            ),
            (
                "encoded_length".into(),
                Value::ByteCount(ByteCount::new(line.len() as u64 + 2)),
            ),
            ("live_serial".into(), Value::Bool(self.serial.is_some())),
            ("reply".into(), Value::String(reply.clone())),
        ]));
        Ok(reply)
    }

    fn refresh_status(&mut self) -> Result<String> {
        let status = self.record_query(protocol::NijiCommand::QueryStatus, "refresh_status")?;
        self.configured.status_reply = status.clone();
        self.apply_parsed_refresh(parse_known_reply_lines(&status));
        self.emit_property(self.hub, "status_reply", Value::String(status.clone()));
        Ok(status)
    }

    fn refresh_temperatures(&mut self) -> Result<String> {
        let reply = self.record_query(
            protocol::NijiCommand::QueryTemperatures,
            "refresh_temperatures",
        )?;
        self.configured.temperature_reply = reply.clone();
        self.apply_parsed_refresh(parse_known_reply_lines(&reply));
        self.emit_property(self.hub, "temperature_reply", Value::String(reply.clone()));
        Ok(reply)
    }

    fn refresh_readbacks(&mut self) -> Result<Value> {
        let status = self.refresh_status()?;
        let temperatures = self.refresh_temperatures()?;
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String("refresh_readbacks".into())),
            ("commands".into(), Value::I64(2)),
            (
                "connected".into(),
                Value::Bool(self.configured.connect_real_transport && self.serial.is_some()),
            ),
            ("status_reply".into(), Value::String(status)),
            ("temperature_reply".into(), Value::String(temperatures)),
            (
                "firmware_version".into(),
                Value::String(self.configured.firmware_version.clone()),
            ),
            (
                "output_temperature".into(),
                Value::Temperature(self.configured.output_temperature),
            ),
            (
                "ambient_temperature".into(),
                Value::Temperature(self.configured.ambient_temperature),
            ),
            ("error_code".into(), Value::I64(self.configured.error_code)),
            ("fault".into(), Value::Bool(self.configured.error_code != 0)),
        ])))
    }

    fn refresh_status_after_write(&mut self) -> Result<()> {
        if self.serial.is_some() {
            let _ = self.refresh_status()?;
        }
        Ok(())
    }

    fn record_query(&mut self, command: protocol::NijiCommand, action: &str) -> Result<String> {
        let line = protocol::encode(&command);
        let mut reply = String::new();
        let completion_basis = if self.serial.is_some() {
            let bytes = self.codec.encode(&line);
            self.active_serial()?.write(&bytes)?;
            reply = self.read_lines_until_timeout()?;
            "serial write and multi-line readback"
        } else {
            "configured command acceptance; known-prefix reply parsing only"
        };
        self.last_transaction = Value::Map(BTreeMap::from([
            ("action".into(), Value::String(action.into())),
            (
                "completion_basis".into(),
                Value::String(completion_basis.into()),
            ),
            (
                "encoded_length".into(),
                Value::ByteCount(ByteCount::new(line.len() as u64 + 2)),
            ),
            ("live_serial".into(), Value::Bool(self.serial.is_some())),
            ("reply".into(), Value::String(reply.clone())),
        ]));
        Ok(reply)
    }

    fn apply_parsed_refresh(&mut self, parsed: NijiParsedReply) {
        if let Some(firmware) = parsed.firmware_version {
            self.configured.firmware_version = firmware;
            self.emit_property(
                self.hub,
                "firmware_version",
                Value::String(self.configured.firmware_version.clone()),
            );
        }
        if let Some(temperature) = parsed.output_temperature {
            self.configured.output_temperature = temperature;
            self.emit_property(
                self.hub,
                "output_temperature",
                Value::Temperature(temperature),
            );
        }
        if let Some(temperature) = parsed.ambient_temperature {
            self.configured.ambient_temperature = temperature;
            self.emit_property(
                self.hub,
                "ambient_temperature",
                Value::Temperature(temperature),
            );
        }
        if let Some(error_code) = parsed.error_code {
            self.configured.error_code = error_code;
            self.emit_property(self.hub, "error_code", Value::I64(error_code));
            self.emit_property(self.hub, "fault", Value::Bool(error_code != 0));
            self.emit_property(self.hub, "interlock_closed", Value::Bool(error_code == 0));
        }
    }

    fn active_serial(&mut self) -> Result<&mut (dyn SerialIo + 'static)> {
        self.serial.as_deref_mut().ok_or_else(|| {
            Error::new(
                ErrorCode::Unsupported,
                "niji active serial is not connected",
            )
        })
    }

    fn read_line_until_timeout(&mut self) -> Result<String> {
        let deadline = Instant::now() + Duration::from_millis(self.configured.serial_timeout_ms);
        loop {
            let bytes = self.active_serial()?.read_available()?;
            let lines = self.codec.push(&bytes);
            if let Some(line) = lines.into_iter().find(|line| !line.trim().is_empty()) {
                return Ok(line.trim().into());
            }
            if Instant::now() >= deadline {
                return Ok(String::new());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn read_lines_until_timeout(&mut self) -> Result<String> {
        let deadline = Instant::now() + Duration::from_millis(self.configured.serial_timeout_ms);
        let mut lines = Vec::new();
        loop {
            let bytes = self.active_serial()?.read_available()?;
            for line in self.codec.push(&bytes) {
                let line = line.trim();
                if !line.is_empty() {
                    lines.push(line.to_string());
                }
            }
            if Instant::now() >= deadline {
                return Ok(lines.join("\n"));
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "product" => Ok(Value::String(self.configured.product.clone())),
                "serial_number" => Ok(Value::String(self.configured.serial_number.clone())),
                "serial_port" => Ok(Value::String(
                    self.configured.serial_port.clone().unwrap_or_default(),
                )),
                "connected" => Ok(Value::Bool(self.serial.is_some())),
                "serial_timeout" => Ok(Value::TimeInterval(TimeInterval::from_milliseconds(
                    self.configured.serial_timeout_ms as f64,
                ))),
                "firmware_version" => Ok(Value::String(self.configured.firmware_version.clone())),
                "enabled" => Ok(Value::Bool(self.configured.global_enabled)),
                "global_intensity" => Ok(Value::Ratio(self.configured.global_intensity)),
                "trigger_source" => Ok(Value::String(self.configured.trigger_source.name().into())),
                "trigger_logic" => Ok(Value::String(self.configured.trigger_logic.name().into())),
                "trigger_resistor" => Ok(Value::String(
                    self.configured.trigger_resistor.name().into(),
                )),
                "output_mode" => Ok(Value::String(self.configured.output_mode.name().into())),
                "output_temperature" => Ok(Value::Temperature(self.configured.output_temperature)),
                "ambient_temperature" => {
                    Ok(Value::Temperature(self.configured.ambient_temperature))
                }
                "error_code" => Ok(Value::I64(self.configured.error_code)),
                "fault" => Ok(Value::Bool(self.configured.error_code != 0)),
                "interlock_closed" => Ok(Value::Bool(self.configured.error_code == 0)),
                "status_reply" => Ok(Value::String(self.configured.status_reply.clone())),
                "temperature_reply" => Ok(Value::String(self.configured.temperature_reply.clone())),
                "last_transaction" => Ok(self.last_transaction.clone()),
                _ => invalid_property("unknown niji hub property", key),
            };
        }
        let index = self
            .channel_index(device)
            .ok_or_else(|| Error::new(ErrorCode::InvalidProperty, "unknown niji channel device"))?;
        match key {
            "enabled" | "selected" => Ok(Value::Bool(self.configured.channel_enabled[index])),
            "intensity" => Ok(Value::Ratio(self.configured.channel_intensity[index])),
            "wavelength" => Ok(Value::Wavelength(self.configured.wavelengths[index])),
            "label" => Ok(Value::String(self.configured.channel_labels[index].clone())),
            _ => invalid_property("unknown niji channel property", key),
        }
    }

    fn validate_read(&self, device: DeviceId, key: &str) -> Result<()> {
        if device == self.hub
            && matches!(
                key,
                "product"
                    | "serial_number"
                    | "serial_port"
                    | "connected"
                    | "serial_timeout"
                    | "firmware_version"
                    | "enabled"
                    | "global_intensity"
                    | "trigger_source"
                    | "trigger_logic"
                    | "trigger_resistor"
                    | "output_mode"
                    | "output_temperature"
                    | "ambient_temperature"
                    | "error_code"
                    | "fault"
                    | "interlock_closed"
                    | "status_reply"
                    | "temperature_reply"
                    | "last_transaction"
            )
        {
            return Ok(());
        }
        if self.channel_index(device).is_some()
            && matches!(
                key,
                "enabled" | "selected" | "intensity" | "wavelength" | "label"
            )
        {
            return Ok(());
        }
        invalid_property("unknown niji property", key)
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        if device == self.hub {
            return match (key, value) {
                ("enabled", Value::Bool(_)) => Ok(()),
                ("global_intensity", Value::Ratio(value)) if percent_ok(*value) => Ok(()),
                ("trigger_source", Value::String(value)) if trigger_source(value).is_some() => {
                    Ok(())
                }
                ("trigger_logic", Value::String(value)) if trigger_logic(value).is_some() => Ok(()),
                ("trigger_resistor", Value::String(value)) if trigger_resistor(value).is_some() => {
                    Ok(())
                }
                ("output_mode", Value::String(value)) if output_mode(value).is_some() => Ok(()),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("niji hub property {key} is read-only or wrong type"),
                )),
            };
        }
        if self.channel_index(device).is_some() {
            return match (key, value) {
                ("enabled" | "selected", Value::Bool(_)) => Ok(()),
                ("intensity", Value::Ratio(value)) if percent_ok(*value) => Ok(()),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("niji channel property {key} is read-only or wrong type"),
                )),
            };
        }
        Err(Error::new(
            ErrorCode::InvalidProperty,
            "unknown niji device",
        ))
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: Value) -> Result<Value> {
        self.validate_write(device, key, &value)?;
        if device == self.hub {
            return match (key, value) {
                ("enabled", Value::Bool(enabled)) => {
                    self.configured.global_enabled = enabled;
                    self.apply_global_enabled()?;
                    Ok(Value::Bool(enabled))
                }
                ("global_intensity", Value::Ratio(value)) => {
                    self.configured.global_intensity = value;
                    self.apply_all_intensities()?;
                    Ok(Value::Ratio(value))
                }
                ("trigger_source", Value::String(value)) => {
                    self.configured.trigger_source = trigger_source(&value).expect("validated");
                    self.apply_trigger()?;
                    Ok(Value::String(self.configured.trigger_source.name().into()))
                }
                ("trigger_logic", Value::String(value)) => {
                    self.configured.trigger_logic = trigger_logic(&value).expect("validated");
                    self.apply_trigger()?;
                    Ok(Value::String(self.configured.trigger_logic.name().into()))
                }
                ("trigger_resistor", Value::String(value)) => {
                    self.configured.trigger_resistor = trigger_resistor(&value).expect("validated");
                    self.apply_trigger()?;
                    Ok(Value::String(
                        self.configured.trigger_resistor.name().into(),
                    ))
                }
                ("output_mode", Value::String(value)) => {
                    self.configured.output_mode = output_mode(&value).expect("validated");
                    self.record(
                        protocol::NijiCommand::SetOutputMode(self.configured.output_mode),
                        "set_output_mode",
                    )?;
                    self.refresh_status_after_write()?;
                    Ok(Value::String(self.configured.output_mode.name().into()))
                }
                _ => unreachable!("validated niji hub write"),
            };
        }
        let index = self.channel_index(device).expect("validated niji channel");
        match (key, value) {
            ("enabled" | "selected", Value::Bool(enabled)) => {
                self.configured.channel_enabled[index] = enabled;
                if self.configured.global_enabled {
                    self.record(
                        protocol::NijiCommand::SetChannelEnabled {
                            channel: (index + 1) as u8,
                            enabled,
                        },
                        "set_channel_enabled",
                    )?;
                }
                self.emit_property(device, "enabled", Value::Bool(enabled));
                self.emit_property(device, "selected", Value::Bool(enabled));
                self.refresh_status_after_write()?;
                Ok(Value::Bool(enabled))
            }
            ("intensity", Value::Ratio(value)) => {
                self.configured.channel_intensity[index] = value;
                let percent = effective_percent(self.configured.global_intensity, value);
                self.record(
                    protocol::NijiCommand::SetChannelIntensity {
                        channel: (index + 1) as u8,
                        percent,
                    },
                    "set_channel_intensity",
                )?;
                self.emit_property(device, "intensity", Value::Ratio(value));
                self.refresh_status_after_write()?;
                Ok(Value::Ratio(value))
            }
            _ => unreachable!("validated niji channel write"),
        }
    }

    fn apply_global_enabled(&mut self) -> Result<()> {
        let enabled = self.configured.global_enabled;
        for index in 0..self.channels.len() {
            self.record(
                protocol::NijiCommand::SetChannelEnabled {
                    channel: (index + 1) as u8,
                    enabled: enabled && self.configured.channel_enabled[index],
                },
                "set_global_enabled",
            )?;
        }
        self.emit_property(self.hub, "enabled", Value::Bool(enabled));
        self.refresh_status_after_write()?;
        Ok(())
    }

    fn apply_all_intensities(&mut self) -> Result<()> {
        for index in 0..self.channels.len() {
            let percent = effective_percent(
                self.configured.global_intensity,
                self.configured.channel_intensity[index],
            );
            self.record(
                protocol::NijiCommand::SetChannelIntensity {
                    channel: (index + 1) as u8,
                    percent,
                },
                "set_global_intensity",
            )?;
        }
        self.emit_property(
            self.hub,
            "global_intensity",
            Value::Ratio(self.configured.global_intensity),
        );
        self.refresh_status_after_write()?;
        Ok(())
    }

    fn apply_trigger(&mut self) -> Result<()> {
        self.record(
            protocol::NijiCommand::SetTrigger {
                source: self.configured.trigger_source,
                logic: self.configured.trigger_logic,
                resistor: self.configured.trigger_resistor,
            },
            "set_trigger",
        )?;
        self.refresh_status_after_write()?;
        Ok(())
    }

    fn invoke(
        &mut self,
        device: DeviceId,
        kind: CapabilityKind,
        request: CapabilityRequest,
    ) -> Result<Value> {
        match (kind, request) {
            (CapabilityKind::Dac, CapabilityRequest::Dac(request)) => {
                self.write_property(device, "intensity", request.value)
            }
            (CapabilityKind::TriggerSink, CapabilityRequest::None) => {
                self.write_property(device, "enabled", Value::Bool(true))
            }
            (CapabilityKind::TriggerSink, CapabilityRequest::Trigger(request)) => {
                let enabled = match request.action {
                    TriggerAction::Enable | TriggerAction::Pulse => true,
                    TriggerAction::Disable => false,
                };
                self.write_property(device, "enabled", Value::Bool(enabled))
            }
            (CapabilityKind::GenericCommand, CapabilityRequest::GenericCommand(request)) => {
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
                        "Niji GenericCommand refresh commands do not accept params",
                    ));
                }
                match request.command.as_str() {
                    "refresh_readbacks" => self.refresh_readbacks(),
                    "refresh_status" => Ok(Value::String(self.refresh_status()?)),
                    "refresh_temperatures" => Ok(Value::String(self.refresh_temperatures()?)),
                    _ => Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "Niji GenericCommand supports refresh_readbacks, refresh_status, and refresh_temperatures",
                    )),
                }
            }
            (CapabilityKind::GenericCommand, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Niji GenericCommand expects GenericCommandRequest",
            )),
            _ => Err(Error::new(
                ErrorCode::InvalidCommand,
                "niji capability request kind does not match",
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

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| self.owns_device(sequence.device))
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequences(plan) {
            match (sequence.device, sequence.property.as_str()) {
                (device, "enabled" | "global_intensity") if device == self.hub => {}
                (device, "enabled" | "selected" | "intensity")
                    if self.channel_index(device).is_some() => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "niji timing sequences can only target hub enabled/global_intensity or channel enabled/selected/intensity",
                    ))
                }
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
                "hub_participant".into(),
                Value::Bool(plan.participants.contains(&self.hub)),
            ),
            (
                "sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
            (
                "enabled".into(),
                Value::Bool(self.configured.global_enabled),
            ),
            (
                "global_intensity".into(),
                Value::Ratio(self.configured.global_intensity),
            ),
            (
                "channels".into(),
                Value::List(
                    self.channels
                        .iter()
                        .enumerate()
                        .map(|(index, device)| {
                            Value::Map(BTreeMap::from([
                                ("channel".into(), Value::I64(index as i64 + 1)),
                                (
                                    "participant".into(),
                                    Value::Bool(plan.participants.contains(device)),
                                ),
                                (
                                    "enabled".into(),
                                    Value::Bool(self.configured.channel_enabled[index]),
                                ),
                                (
                                    "intensity".into(),
                                    Value::Ratio(self.configured.channel_intensity[index]),
                                ),
                            ]))
                        })
                        .collect(),
                ),
            ),
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
                Some((sequence.device, sequence.property.clone(), value.clone()))
            })
            .collect::<Vec<_>>();
        let mut changed = BTreeMap::new();
        for (device, property, value) in writes {
            let applied = self.write_property(device, &property, value)?;
            let key = if device == self.hub {
                format!("hub:{property}")
            } else {
                let index = self.channel_index(device).expect("validated niji device");
                format!("channel{}:{property}", index + 1)
            };
            changed.insert(key, applied);
        }
        Ok(Value::Map(changed))
    }
}

impl Driver for NijiDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "niji-serial".into(),
            kind: "serial.ascii".into(),
            metadata: BTreeMap::from([
                (
                    "serial_port".into(),
                    self.configured
                        .serial_port
                        .as_ref()
                        .map(|port| Value::String(port.clone()))
                        .unwrap_or(Value::Null),
                ),
                ("baud_rate".into(), Value::I64(protocol::BAUD as i64)),
                ("data_bits".into(), Value::I64(protocol::DATA_BITS as i64)),
                ("stop_bits".into(), Value::I64(protocol::STOP_BITS as i64)),
                ("parity".into(), Value::String(protocol::PARITY.into())),
                (
                    "line_ending".into(),
                    Value::String(protocol::LINE_ENDING.into()),
                ),
                (
                    "completion".into(),
                    Value::String("configured acceptance or active serial line readback".into()),
                ),
                (
                    "connected".into(),
                    Value::Bool(self.configured.connect_real_transport && self.serial.is_some()),
                ),
            ]),
        }]
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        let mut descriptors = vec![DeviceDescriptor {
            id: self.hub,
            driver: self.id,
            label: "niji-hub".into(),
            vendor: Some("Bluebox Optics".into()),
            model: Some(self.configured.product.clone()),
            serial: Some(self.configured.serial_number.clone()),
            kinds: vec![
                "hub".into(),
                "light.engine".into(),
                "shutter".into(),
                "serial.ascii".into(),
            ],
            properties: vec![
                string_property("product", "Product", false),
                string_property("serial_number", "Serial number", false),
                string_property("serial_port", "Serial port", false),
                bool_property("connected", "Connected", false),
                time_property("serial_timeout", "Serial timeout", false),
                string_property("firmware_version", "Firmware version", false),
                bool_property("enabled", "Enabled", true),
                ratio_property("global_intensity", "Global intensity", true),
                enum_property(
                    "trigger_source",
                    "Trigger source",
                    true,
                    &["Internal", "External"],
                ),
                enum_property(
                    "trigger_logic",
                    "Trigger logic",
                    true,
                    &["ActiveLow", "ActiveHigh"],
                ),
                enum_property(
                    "trigger_resistor",
                    "Trigger resistor",
                    true,
                    &["PullDown", "PullUp"],
                ),
                enum_property(
                    "output_mode",
                    "Output mode",
                    true,
                    &["ConstantCurrent", "ConstantOpticalPower"],
                ),
                temperature_property("output_temperature", "Output temperature", false),
                temperature_property("ambient_temperature", "Ambient temperature", false),
                integer_property("error_code", "Error code", false),
                bool_property("fault", "Fault", false),
                bool_property("interlock_closed", "Interlock closed", false),
                string_property("status_reply", "Status reply", false),
                string_property("temperature_reply", "Temperature reply", false),
                map_property("last_transaction", "Last transaction", false),
            ],
            metadata: source_metadata(),
        }];
        for (index, device) in self.channels.iter().copied().enumerate() {
            descriptors.push(DeviceDescriptor {
                id: device,
                driver: self.id,
                label: format!("niji-channel-{}", index + 1),
                vendor: Some("Bluebox Optics".into()),
                model: Some(self.configured.product.clone()),
                serial: Some(format!("{}:{}", self.configured.serial_number, index + 1)),
                kinds: vec![
                    "light.source".into(),
                    "led.channel".into(),
                    "trigger.sink".into(),
                ],
                properties: vec![
                    bool_property("enabled", "Enabled", true),
                    bool_property("selected", "Selected", true),
                    ratio_property("intensity", "Intensity", true),
                    wavelength_property("wavelength", "Wavelength", false),
                    string_property("label", "Label", false),
                ],
                metadata: BTreeMap::from([(
                    "channel_index".into(),
                    Value::I64((index + 1) as i64),
                )]),
            });
        }
        descriptors
    }

    fn graph(&self) -> DeviceGraph {
        let mut graph = DeviceGraph::default();
        let _ = graph.insert_node(GraphNode {
            id: self.resource.0,
            kind: NodeKind::Resource,
            label: "niji-serial".into(),
        });
        let _ = graph.insert_node(GraphNode {
            id: self.hub.0,
            kind: NodeKind::Hub,
            label: "niji-hub".into(),
        });
        let _ = graph.insert_edge(GraphEdge {
            from: self.hub.0,
            to: self.resource.0,
            kind: EdgeKind::OwnsResource,
        });
        for device in &self.channels {
            let _ = graph.insert_node(GraphNode {
                id: device.0,
                kind: NodeKind::Device,
                label: format!("niji-channel-{}", device.0 .0),
            });
            let _ = graph.insert_edge(GraphEdge {
                from: self.hub.0,
                to: device.0,
                kind: EdgeKind::OffersDevice,
            });
        }
        graph
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.hub {
            return vec![
                capability(1, device, CapabilityKind::TriggerSink),
                capability(2, device, CapabilityKind::GenericCommand),
            ];
        }
        if self.channel_index(device).is_some() {
            return vec![
                capability(1, device, CapabilityKind::TriggerSink),
                capability(2, device, CapabilityKind::Dac),
            ];
        }
        Vec::new()
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut physical_transactions = Vec::new();
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    self.validate_read(*device, key)?;
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("niji read {key}"),
                        Value::String(key.clone()),
                    ));
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("niji write {key}"),
                        value.clone(),
                    ));
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    let Some(descriptor) = self
                        .capabilities(*device)
                        .into_iter()
                        .find(|candidate| candidate.id == *capability)
                    else {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "unknown niji capability",
                        ));
                    };
                    if !descriptor.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "niji {} request kind does not match",
                                descriptor.kind.name()
                            ),
                        ));
                    }
                    if descriptor.kind == CapabilityKind::GenericCommand {
                        let CapabilityRequest::GenericCommand(request) = request else {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Niji GenericCommand expects GenericCommandRequest",
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
                        if !matches!(
                            request.command.as_str(),
                            "refresh_readbacks" | "refresh_status" | "refresh_temperatures"
                        ) {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Niji GenericCommand supports refresh_readbacks, refresh_status, and refresh_temperatures",
                            ));
                        }
                        if !request.params.is_empty() {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Niji GenericCommand refresh commands do not accept params",
                            ));
                        }
                    }
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("niji {}", descriptor.kind.name()),
                        Value::String(descriptor.kind.name().into()),
                    ));
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        if self.owns_device(write.device) {
                            self.validate_write(write.device, &write.property, &write.value)?;
                        }
                    }
                    physical_transactions.push(transaction(
                        self.resource,
                        "niji state set",
                        Value::I64(set.writes.len() as i64),
                    ));
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
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    let Some(descriptor) = self
                        .capabilities(device)
                        .into_iter()
                        .find(|candidate| candidate.id == capability)
                    else {
                        continue;
                    };
                    last = self.invoke(device, descriptor.kind, request)?;
                }
                Command::ApplyStateSet(set) => {
                    let mut values = BTreeMap::new();
                    for write in set.writes {
                        if self.owns_device(write.device) {
                            values.insert(
                                write.property.clone(),
                                self.write_property(write.device, &write.property, write.value)?,
                            );
                        }
                    }
                    last = Value::Map(values);
                }
                Command::Arm(_) | Command::Start(_) | Command::Stop(_) => {}
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
            physical_transactions: vec![transaction(
                self.resource,
                "niji timing arm summary",
                self.timing_summary(plan, "arm"),
            )],
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
            physical_transactions: vec![transaction(
                self.resource,
                "niji timing start sequence",
                Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "start")),
                    ("changed".into(), changed),
                ])),
            )],
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
            physical_transactions: vec![transaction(
                self.resource,
                "niji timing stop sequence",
                Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "stop")),
                    ("changed".into(), changed),
                ])),
            )],
        })
    }
}

fn effective_percent(global: Ratio, channel: Ratio) -> u8 {
    (global.percent() * channel.percent() / 100.0)
        .round()
        .clamp(0.0, 100.0) as u8
}

fn percent_ok(value: Ratio) -> bool {
    (0.0..=100.0).contains(&value.percent())
}

fn validate_ratios(configured: &NijiConfiguredProbe) -> Result<()> {
    if !percent_ok(configured.global_intensity) {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "niji global_intensity must be in 0..=100 percent",
        ));
    }
    if configured
        .channel_intensity
        .iter()
        .any(|ratio| !percent_ok(*ratio))
    {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "niji channel intensity must be in 0..=100 percent",
        ));
    }
    Ok(())
}

fn source_metadata() -> BTreeMap<String, Value> {
    BTreeMap::from([
        (
            "evidence".into(),
            Value::String("reverse engineered serial command evidence".into()),
        ),
        (
            "support_level".into(),
            Value::String("opt-in serial control and readback".into()),
        ),
        (
            "hardware_validation".into(),
            Value::String("not_recorded".into()),
        ),
    ])
}

fn transaction(
    resource: ResourceId,
    description: impl Into<String>,
    payload: Value,
) -> PhysicalTransaction {
    PhysicalTransaction {
        resource: Some(resource),
        description: description.into(),
        payload,
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
        sequenceable: matches!(
            key,
            "enabled" | "selected" | "intensity" | "global_intensity"
        ),
        hardware_address: None,
    }
}

fn string_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::String, None, writable, None)
}

fn bool_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::Bool, None, writable, None)
}

fn map_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::Map, None, writable, None)
}

fn time_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::TimeInterval,
        Some("ms"),
        writable,
        None,
    )
}

fn integer_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::I64, None, writable, None)
}

fn ratio_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Ratio,
        Some("percent"),
        writable,
        Some(Range {
            min: Value::Ratio(Ratio::from_percent(0.0)),
            max: Value::Ratio(Ratio::from_percent(100.0)),
        }),
    )
}

fn wavelength_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Wavelength,
        Some("nm"),
        writable,
        None,
    )
}

fn temperature_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Temperature,
        Some("C"),
        writable,
        None,
    )
}

fn enum_property(key: &str, display_name: &str, writable: bool, values: &[&str]) -> PropertySchema {
    let mut schema = string_property(key, display_name, writable);
    schema.enum_values = values
        .iter()
        .map(|value| EnumValue {
            value: Value::String((*value).into()),
            label: (*value).into(),
        })
        .collect();
    schema
}

fn invalid_property<T>(message: &str, key: &str) -> Result<T> {
    Err(Error::new(
        ErrorCode::InvalidProperty,
        format!("{message}: {key}"),
    ))
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

fn i64_prop(device: &DeviceConfig, key: &str) -> Option<i64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => Some(*value),
        _ => None,
    }
}

fn u64_prop(device: &DeviceConfig, key: &str) -> Option<u64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if *value >= 0 => Some(*value as u64),
        Some(Value::TimeInterval(value))
            if value.seconds().is_finite() && value.seconds() >= 0.0 =>
        {
            Some((value.seconds() * 1000.0).round() as u64)
        }
        _ => None,
    }
}

fn ratio_prop(device: &DeviceConfig, key: &str) -> Option<Ratio> {
    match device.properties.get(key) {
        Some(Value::Ratio(value)) => Some(*value),
        _ => None,
    }
}

fn wavelength_prop(device: &DeviceConfig, key: &str) -> Option<Wavelength> {
    match device.properties.get(key) {
        Some(Value::Wavelength(value)) => Some(*value),
        _ => None,
    }
}

fn temperature_prop(device: &DeviceConfig, key: &str) -> Option<Temperature> {
    match device.properties.get(key) {
        Some(Value::Temperature(value)) => Some(*value),
        _ => None,
    }
}

fn trigger_source_prop(device: &DeviceConfig, key: &str) -> Option<protocol::TriggerSource> {
    string_prop(device, key).and_then(|value| trigger_source(&value))
}

fn trigger_logic_prop(device: &DeviceConfig, key: &str) -> Option<protocol::TriggerLogic> {
    string_prop(device, key).and_then(|value| trigger_logic(&value))
}

fn trigger_resistor_prop(device: &DeviceConfig, key: &str) -> Option<protocol::TriggerResistor> {
    string_prop(device, key).and_then(|value| trigger_resistor(&value))
}

fn output_mode_prop(device: &DeviceConfig, key: &str) -> Option<protocol::OutputMode> {
    string_prop(device, key).and_then(|value| output_mode(&value))
}

fn trigger_source(value: &str) -> Option<protocol::TriggerSource> {
    match value {
        "Internal" | "internal" => Some(protocol::TriggerSource::Internal),
        "External" | "external" => Some(protocol::TriggerSource::External),
        _ => None,
    }
}

fn trigger_logic(value: &str) -> Option<protocol::TriggerLogic> {
    match value {
        "ActiveLow" | "active_low" | "Active Low" => Some(protocol::TriggerLogic::ActiveLow),
        "ActiveHigh" | "active_high" | "Active High" => Some(protocol::TriggerLogic::ActiveHigh),
        _ => None,
    }
}

fn trigger_resistor(value: &str) -> Option<protocol::TriggerResistor> {
    match value {
        "PullDown" | "pull_down" | "Pull Down" => Some(protocol::TriggerResistor::PullDown),
        "PullUp" | "pull_up" | "Pull Up" => Some(protocol::TriggerResistor::PullUp),
        _ => None,
    }
}

fn output_mode(value: &str) -> Option<protocol::OutputMode> {
    match value {
        "ConstantCurrent" | "constant_current" | "Constant Current" => {
            Some(protocol::OutputMode::ConstantCurrent)
        }
        "ConstantOpticalPower" | "constant_optical_power" | "Constant Optical Power" => {
            Some(protocol::OutputMode::ConstantOpticalPower)
        }
        _ => None,
    }
}
