use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::{LineEnding, SerialIo, SerialLineCodec};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Sc10ConfiguredProbe {
    label: String,
    model: String,
    serial_number: String,
    firmware_version: String,
    mode: protocol::Mode,
    shutter_open: bool,
    open_time: TimeInterval,
    close_time: TimeInterval,
    trigger_mode: protocol::TriggerMode,
    repeat_count: i64,
    interlock_closed: bool,
    fault: bool,
    endpoint: Option<Sc10SerialEndpoint>,
    connect_real_transport: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sc10SerialEndpoint {
    pub port_name: String,
    pub timeout_ms: u64,
}

pub struct Sc10Discovery {
    next_id: DriverId,
    probes: Vec<Sc10ConfiguredProbe>,
}

impl Sc10Discovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![Sc10ConfiguredProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| {
                matches!(
                    device.driver.as_str(),
                    "thorlabs_sc10" | "thorlabs-sc10" | "sc10"
                )
            })
            .map(Sc10ConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for Sc10Discovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver: Box<dyn Driver> = if configured.connect_real_transport {
                    Box::new(Sc10Driver::serial(id, configured)?)
                } else {
                    Box::new(Sc10Driver::configured(id, configured))
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

impl Sc10ConfiguredProbe {
    pub fn fixture() -> Self {
        Self {
            label: "Configured Thorlabs SC10 shutter controller".into(),
            model: "SC10".into(),
            serial_number: "SC10-CONFIG-0001".into(),
            firmware_version: "configured".into(),
            mode: protocol::Mode::Manual,
            shutter_open: false,
            open_time: TimeInterval::from_milliseconds(10.0),
            close_time: TimeInterval::from_milliseconds(10.0),
            trigger_mode: protocol::TriggerMode::Internal,
            repeat_count: 1,
            interlock_closed: true,
            fault: false,
            endpoint: None,
            connect_real_transport: false,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::fixture();
        if !device.label.is_empty() {
            configured.label = device.label.clone();
        }
        configured.model = string_prop(device, "model").unwrap_or(configured.model);
        configured.serial_number =
            string_prop(device, "serial_number").unwrap_or(configured.serial_number);
        configured.firmware_version =
            string_prop(device, "firmware_version").unwrap_or(configured.firmware_version);
        configured.mode = mode_prop(device, "mode").unwrap_or(configured.mode);
        configured.shutter_open = bool_prop(device, "open")
            .or_else(|| bool_prop(device, "enabled"))
            .unwrap_or(configured.shutter_open);
        configured.open_time = time_prop(device, "open_time").unwrap_or(configured.open_time);
        configured.close_time = time_prop(device, "close_time").unwrap_or(configured.close_time);
        configured.trigger_mode =
            trigger_mode_prop(device, "trigger_mode").unwrap_or(configured.trigger_mode);
        configured.repeat_count =
            i64_prop(device, "repeat_count").unwrap_or(configured.repeat_count);
        configured.interlock_closed =
            bool_prop(device, "interlock_closed").unwrap_or(configured.interlock_closed);
        configured.fault = bool_prop(device, "fault").unwrap_or(configured.fault);
        configured.endpoint =
            string_prop(device, "serial_port").map(|port_name| Sc10SerialEndpoint {
                port_name,
                timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(500),
            });
        configured.connect_real_transport = bool_prop(device, "connect").unwrap_or(false);
        Ok(configured)
    }
}

pub struct Sc10Driver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    shutter: DeviceId,
    configured: Sc10ConfiguredProbe,
    next_token: u64,
    events: VecDeque<DriverEvent>,
    serial: Option<Box<dyn SerialIo>>,
    codec: SerialLineCodec,
    prompt: protocol::PromptParser,
}

impl Sc10Driver {
    pub fn configured(id: DriverId, configured: Sc10ConfiguredProbe) -> Self {
        Self::new(id, configured, None)
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: Sc10ConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "SC10 config requires serial_port when connect is true",
            )
        })?;
        let serial = Box::new(numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(
                endpoint.port_name,
                protocol::DEFAULT_BAUD_RATE,
            )
            .timeout(Duration::from_millis(endpoint.timeout_ms)),
        )?);
        let mut driver = Self::new(id, configured, Some(serial));
        driver.refresh_startup_state()?;
        Ok(driver)
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _configured: Sc10ConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Thorlabs SC10 real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(
        id: DriverId,
        configured: Sc10ConfiguredProbe,
        serial: Option<Box<dyn SerialIo>>,
    ) -> Self {
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 980)),
            hub: DeviceId(NodeId(id.0 * 1000 + 981)),
            shutter: DeviceId(NodeId(id.0 * 1000 + 982)),
            configured,
            next_token: 1,
            events: VecDeque::new(),
            serial,
            codec: SerialLineCodec::new(protocol::SEND_ENDING, protocol::RECV_ENDING),
            prompt: protocol::PromptParser::default(),
        }
    }

    pub fn configured_fixture(id: DriverId) -> Self {
        Self::configured(id, Sc10ConfiguredProbe::fixture())
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "model" => Ok(Value::String(self.configured.model.clone())),
                "serial_number" => Ok(Value::String(self.configured.serial_number.clone())),
                "firmware_version" => Ok(Value::String(self.configured.firmware_version.clone())),
                "serial_settings" => Ok(Value::String("9600 8N1 no-flow".into())),
                _ => invalid_property("unknown SC10 controller property", key),
            };
        }
        if device != self.shutter {
            return Err(Error::new(ErrorCode::InvalidCommand, "unknown SC10 device"));
        }
        match key {
            "open" => Ok(Value::Bool(self.configured.shutter_open)),
            "mode" => Ok(Value::String(self.configured.mode.label().into())),
            "open_time" => Ok(Value::TimeInterval(self.configured.open_time)),
            "close_time" => Ok(Value::TimeInterval(self.configured.close_time)),
            "trigger_mode" => Ok(Value::String(self.configured.trigger_mode.label().into())),
            "repeat_count" => Ok(Value::I64(self.configured.repeat_count)),
            "interlock_closed" => Ok(Value::Bool(self.configured.interlock_closed)),
            "fault" => Ok(Value::Bool(self.configured.fault)),
            "state_summary" => Ok(self.state_summary()),
            _ => invalid_property("unknown SC10 shutter property", key),
        }
    }

    fn active_serial(&mut self) -> Result<&mut (dyn SerialIo + 'static)> {
        self.serial.as_deref_mut().ok_or_else(|| {
            Error::new(
                ErrorCode::Unsupported,
                "SC10 active serial is not connected",
            )
        })
    }

    fn command_prompt(&mut self, command: protocol::Command) -> Result<protocol::PromptReply> {
        let line = protocol::encode(command);
        let bytes = self.codec.encode(&line);
        self.active_serial()?.write(&bytes)?;
        self.read_prompt()
    }

    fn query(&mut self, query: protocol::Query) -> Result<protocol::PromptReply> {
        self.command_prompt(protocol::Command::Query(query))
    }

    fn read_prompt(&mut self) -> Result<protocol::PromptReply> {
        let deadline = Instant::now() + Duration::from_millis(self.serial_timeout_ms());
        loop {
            let bytes = self.active_serial()?.read_available()?;
            if let Some(reply) = self.prompt.push(&bytes)? {
                return Ok(reply);
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        Err(Error::new(
            ErrorCode::Transport,
            "SC10 did not return the documented prompt",
        ))
    }

    fn serial_timeout_ms(&self) -> u64 {
        self.configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.timeout_ms)
            .unwrap_or(500)
    }

    #[cfg_attr(not(feature = "os-serial"), allow(dead_code))]
    fn refresh_startup_state(&mut self) -> Result<()> {
        let identity = self.query(protocol::Query::Identity)?;
        self.apply_query_reply(protocol::Query::Identity, identity)?;
        for query in protocol::Query::STATE_SUMMARY {
            let reply = self.query(query)?;
            self.apply_query_reply(query, reply)?;
        }
        Ok(())
    }

    fn read_property_active(&mut self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            if key == "model" || key == "firmware_version" {
                let reply = self.query(protocol::Query::Identity)?;
                let identity = protocol::parse_identity(reply.value_line())?;
                if self.configured.model != identity.model {
                    self.configured.model = identity.model.clone();
                    self.emit_property(self.hub, "model", Value::String(identity.model));
                }
                if self.configured.firmware_version != identity.firmware_version {
                    self.configured.firmware_version = identity.firmware_version.clone();
                    self.emit_property(
                        self.hub,
                        "firmware_version",
                        Value::String(identity.firmware_version),
                    );
                }
            }
            return self.read_property(device, key);
        }

        match key {
            "open" => {
                let value =
                    protocol::parse_bool(self.query(protocol::Query::Enabled)?.value_line())?;
                self.update_enabled(value);
            }
            "mode" => {
                let value = protocol::parse_mode(self.query(protocol::Query::Mode)?.value_line())?;
                self.update_mode(value);
            }
            "open_time" => {
                let value =
                    protocol::parse_time(self.query(protocol::Query::OpenTime)?.value_line())?;
                self.update_open_time(value);
            }
            "close_time" => {
                let value =
                    protocol::parse_time(self.query(protocol::Query::CloseTime)?.value_line())?;
                self.update_close_time(value);
            }
            "trigger_mode" => {
                let value = protocol::parse_trigger_mode(
                    self.query(protocol::Query::TriggerMode)?.value_line(),
                )?;
                self.update_trigger_mode(value);
            }
            "repeat_count" => {
                let value = protocol::parse_repeat_count(
                    self.query(protocol::Query::RepeatCount)?.value_line(),
                )?;
                self.update_repeat_count(value);
            }
            "state_summary" => {
                for query in protocol::Query::STATE_SUMMARY {
                    let reply = self.query(query)?;
                    self.apply_query_reply(query, reply)?;
                }
            }
            "interlock_closed" | "fault" => {}
            _ => return self.read_property(device, key),
        }
        self.read_property(device, key)
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: &Value) -> Result<Value> {
        self.validate_write(device, key, value)?;
        if self.serial.is_some() {
            return self.write_property_active(device, key, value);
        }
        let applied = match (key, value) {
            ("open", Value::Bool(enabled)) => {
                self.configured.shutter_open = *enabled;
                Value::Bool(self.configured.shutter_open)
            }
            ("mode", Value::String(mode)) => {
                self.configured.mode = protocol::Mode::from_label(mode).ok_or_else(|| {
                    Error::new(ErrorCode::InvalidProperty, "unknown SC10 shutter mode")
                })?;
                Value::String(self.configured.mode.label().into())
            }
            ("open_time", Value::TimeInterval(time)) => {
                validate_sc10_time("open_time", *time)?;
                self.configured.open_time = *time;
                Value::TimeInterval(self.configured.open_time)
            }
            ("close_time", Value::TimeInterval(time)) => {
                validate_sc10_time("close_time", *time)?;
                self.configured.close_time = *time;
                Value::TimeInterval(self.configured.close_time)
            }
            ("trigger_mode", Value::String(mode)) => {
                self.configured.trigger_mode =
                    protocol::TriggerMode::from_label(mode).ok_or_else(|| {
                        Error::new(ErrorCode::InvalidProperty, "unknown SC10 trigger mode")
                    })?;
                Value::String(self.configured.trigger_mode.label().into())
            }
            ("repeat_count", Value::I64(count)) => {
                if !(1..=99).contains(count) {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "SC10 repeat_count must be in 1..=99",
                    ));
                }
                self.configured.repeat_count = *count;
                Value::I64(self.configured.repeat_count)
            }
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("invalid SC10 write {key}"),
                ))
            }
        };
        self.emit_property(device, key, applied.clone());
        Ok(applied)
    }

    fn write_property_active(
        &mut self,
        device: DeviceId,
        key: &str,
        value: &Value,
    ) -> Result<Value> {
        let applied = match (key, value) {
            ("open", Value::Bool(enabled)) => {
                self.set_enabled_active(*enabled)?;
                Value::Bool(self.configured.shutter_open)
            }
            ("mode", Value::String(mode)) => {
                let mode = protocol::Mode::from_label(mode).ok_or_else(|| {
                    Error::new(ErrorCode::InvalidProperty, "unknown SC10 shutter mode")
                })?;
                self.command_prompt(protocol::Command::SetMode(mode))?;
                let readback =
                    protocol::parse_mode(self.query(protocol::Query::Mode)?.value_line())?;
                self.update_mode(readback);
                Value::String(self.configured.mode.label().into())
            }
            ("open_time", Value::TimeInterval(time)) => {
                validate_sc10_time("open_time", *time)?;
                self.command_prompt(protocol::Command::SetOpenTime(*time))?;
                let readback =
                    protocol::parse_time(self.query(protocol::Query::OpenTime)?.value_line())?;
                self.update_open_time(readback);
                Value::TimeInterval(self.configured.open_time)
            }
            ("close_time", Value::TimeInterval(time)) => {
                validate_sc10_time("close_time", *time)?;
                self.command_prompt(protocol::Command::SetCloseTime(*time))?;
                let readback =
                    protocol::parse_time(self.query(protocol::Query::CloseTime)?.value_line())?;
                self.update_close_time(readback);
                Value::TimeInterval(self.configured.close_time)
            }
            ("trigger_mode", Value::String(mode)) => {
                let mode = protocol::TriggerMode::from_label(mode).ok_or_else(|| {
                    Error::new(ErrorCode::InvalidProperty, "unknown SC10 trigger mode")
                })?;
                self.command_prompt(protocol::Command::SetTriggerMode(mode))?;
                let readback = protocol::parse_trigger_mode(
                    self.query(protocol::Query::TriggerMode)?.value_line(),
                )?;
                self.update_trigger_mode(readback);
                Value::String(self.configured.trigger_mode.label().into())
            }
            ("repeat_count", Value::I64(count)) => {
                if !(1..=99).contains(count) {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "SC10 repeat_count must be in 1..=99",
                    ));
                }
                self.command_prompt(protocol::Command::SetRepeatCount(*count))?;
                let readback = protocol::parse_repeat_count(
                    self.query(protocol::Query::RepeatCount)?.value_line(),
                )?;
                self.update_repeat_count(readback);
                Value::I64(self.configured.repeat_count)
            }
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("invalid SC10 write {key}"),
                ))
            }
        };
        self.emit_property(device, key, applied.clone());
        Ok(applied)
    }

    fn set_enabled_active(&mut self, requested: bool) -> Result<()> {
        let current = protocol::parse_bool(self.query(protocol::Query::Enabled)?.value_line())?;
        self.update_enabled(current);
        if current != requested {
            self.command_prompt(protocol::Command::ToggleEnabled)?;
        }
        let readback = protocol::parse_bool(self.query(protocol::Query::Enabled)?.value_line())?;
        self.update_enabled(readback);
        Ok(())
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        if device != self.shutter {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "SC10 writes target the shutter device",
            ));
        }
        let schema = self
            .shutter_descriptor()
            .properties
            .into_iter()
            .find(|property| property.key == key)
            .ok_or_else(|| Error::new(ErrorCode::InvalidProperty, "unknown SC10 property"))?;
        if !schema.writable {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "property is read-only",
            ));
        }
        schema.validate(value)
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

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| sequence.device == self.shutter)
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        let descriptor = self.shutter_descriptor();
        for sequence in self.local_timing_sequences(plan) {
            let schema = descriptor
                .properties
                .iter()
                .find(|property| property.key == sequence.property)
                .ok_or_else(|| {
                    Error::new(ErrorCode::InvalidProperty, "unknown SC10 timing property")
                })?;
            if !schema.sequenceable {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    format!("SC10 property {} is not sequenceable", sequence.property),
                ));
            }
            for value in &sequence.values {
                schema.validate(value)?;
            }
        }
        Ok(())
    }

    fn timing_summary(&self, plan: &TimingPlan, phase: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("phase".into(), Value::String(phase.into())),
            ("shutter".into(), Value::I64(self.shutter.0 .0 as i64)),
            (
                "shutter_participant".into(),
                Value::Bool(plan.participants.contains(&self.shutter)),
            ),
            (
                "sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
            ("state".into(), self.state_summary()),
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
        self.apply_state_set(StateSet {
            name: Some(if first {
                "sc10 timing start sequence".into()
            } else {
                "sc10 timing stop sequence".into()
            }),
            writes,
            commit: CommitMode::Immediate,
        })
    }

    fn invoke_trigger(&mut self, request: CapabilityRequest) -> Result<Value> {
        let values = trigger_values(&request)?;
        for value in values {
            if self.serial.is_some() {
                self.set_enabled_active(value)?;
            } else {
                self.configured.shutter_open = value;
            }
            self.emit_property(
                self.shutter,
                "open",
                Value::Bool(self.configured.shutter_open),
            );
        }
        Ok(Value::Bool(self.configured.shutter_open))
    }

    fn state_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("open".into(), Value::Bool(self.configured.shutter_open)),
            (
                "mode".into(),
                Value::String(self.configured.mode.label().into()),
            ),
            ("fault".into(), Value::Bool(self.configured.fault)),
            (
                "interlock_closed".into(),
                Value::Bool(self.configured.interlock_closed),
            ),
            (
                "trigger_mode".into(),
                Value::String(self.configured.trigger_mode.label().into()),
            ),
        ]))
    }

    fn apply_query_reply(
        &mut self,
        query: protocol::Query,
        reply: protocol::PromptReply,
    ) -> Result<()> {
        match query {
            protocol::Query::Identity => {
                let identity = protocol::parse_identity(reply.value_line())?;
                self.configured.model = identity.model;
                self.configured.firmware_version = identity.firmware_version;
            }
            protocol::Query::Enabled => {
                self.update_enabled(protocol::parse_bool(reply.value_line())?)
            }
            protocol::Query::Mode => self.update_mode(protocol::parse_mode(reply.value_line())?),
            protocol::Query::OpenTime => {
                self.update_open_time(protocol::parse_time(reply.value_line())?)
            }
            protocol::Query::CloseTime => {
                self.update_close_time(protocol::parse_time(reply.value_line())?)
            }
            protocol::Query::TriggerMode => {
                self.update_trigger_mode(protocol::parse_trigger_mode(reply.value_line())?)
            }
            protocol::Query::RepeatCount => {
                self.update_repeat_count(protocol::parse_repeat_count(reply.value_line())?)
            }
        }
        Ok(())
    }

    fn refresh_queries_for(command: &str) -> Result<Vec<protocol::Query>> {
        match command {
            "refresh_readbacks" => Ok(vec![
                protocol::Query::Identity,
                protocol::Query::Enabled,
                protocol::Query::Mode,
                protocol::Query::OpenTime,
                protocol::Query::CloseTime,
                protocol::Query::TriggerMode,
                protocol::Query::RepeatCount,
            ]),
            "refresh_identity" => Ok(vec![protocol::Query::Identity]),
            "refresh_status" => Ok(vec![
                protocol::Query::Enabled,
                protocol::Query::Mode,
                protocol::Query::TriggerMode,
            ]),
            "refresh_timing" => Ok(vec![
                protocol::Query::OpenTime,
                protocol::Query::CloseTime,
                protocol::Query::RepeatCount,
            ]),
            "refresh_open" => Ok(vec![protocol::Query::Enabled]),
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "SC10 GenericCommand supports refresh_readbacks, refresh_identity, refresh_status, refresh_timing, and refresh_open; got {other}"
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
                "SC10 GenericCommand does not take parameters",
            ));
        }
        let _ = Self::refresh_queries_for(&request.command)?;
        Ok(())
    }

    fn apply_generic_command(&mut self, request: GenericCommandRequest) -> Result<Value> {
        self.validate_generic_command(&request)?;
        let queries = Self::refresh_queries_for(&request.command)?;
        for query in queries.iter().copied() {
            let reply = self.query(query)?;
            self.apply_query_reply(query, reply)?;
        }
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("commands".into(), Value::I64(queries.len() as i64)),
            ("state".into(), self.state_summary()),
            (
                "completion_basis".into(),
                Value::String("SC10 mapped query readback".into()),
            ),
        ])))
    }

    fn update_enabled(&mut self, value: bool) {
        if self.configured.shutter_open != value {
            self.configured.shutter_open = value;
            self.emit_property(self.shutter, "open", Value::Bool(value));
        }
    }

    fn update_mode(&mut self, value: protocol::Mode) {
        if self.configured.mode != value {
            self.configured.mode = value;
            self.emit_property(self.shutter, "mode", Value::String(value.label().into()));
        }
    }

    fn update_open_time(&mut self, value: TimeInterval) {
        if self.configured.open_time != value {
            self.configured.open_time = value;
            self.emit_property(self.shutter, "open_time", Value::TimeInterval(value));
        }
    }

    fn update_close_time(&mut self, value: TimeInterval) {
        if self.configured.close_time != value {
            self.configured.close_time = value;
            self.emit_property(self.shutter, "close_time", Value::TimeInterval(value));
        }
    }

    fn update_trigger_mode(&mut self, value: protocol::TriggerMode) {
        if self.configured.trigger_mode != value {
            self.configured.trigger_mode = value;
            self.emit_property(
                self.shutter,
                "trigger_mode",
                Value::String(value.label().into()),
            );
        }
    }

    fn update_repeat_count(&mut self, value: i64) {
        if self.configured.repeat_count != value {
            self.configured.repeat_count = value;
            self.emit_property(self.shutter, "repeat_count", Value::I64(value));
        }
    }

    fn controller_descriptor(&self) -> DeviceDescriptor {
        DeviceDescriptor {
            id: self.hub,
            driver: self.id,
            label: "thorlabs-sc10-controller".into(),
            vendor: Some("Thorlabs".into()),
            model: Some(self.configured.model.clone()),
            serial: Some(self.configured.serial_number.clone()),
            kinds: strings(&["hub", "shutter.controller", "serial.ascii"]),
            properties: vec![
                property("model", "Model", ValueType::String, false, false, None),
                property(
                    "serial_number",
                    "Serial number",
                    ValueType::String,
                    false,
                    false,
                    None,
                ),
                property(
                    "firmware_version",
                    "Firmware version",
                    ValueType::String,
                    false,
                    false,
                    None,
                ),
                property(
                    "serial_settings",
                    "Serial settings",
                    ValueType::String,
                    false,
                    false,
                    None,
                ),
            ],
            metadata: BTreeMap::from([
                ("family".into(), Value::String("Thorlabs SC10".into())),
                (
                    "support_level".into(),
                    Value::String("configured_shutter".into()),
                ),
            ]),
        }
    }

    fn shutter_descriptor(&self) -> DeviceDescriptor {
        DeviceDescriptor {
            id: self.shutter,
            driver: self.id,
            label: "thorlabs-sc10-shutter".into(),
            vendor: Some("Thorlabs".into()),
            model: Some(self.configured.model.clone()),
            serial: Some(self.configured.serial_number.clone()),
            kinds: strings(&["shutter", "light.gate", "trigger.sink"]),
            properties: vec![
                property("open", "Open", ValueType::Bool, true, true, None),
                mode_property(),
                property(
                    "open_time",
                    "Open time",
                    ValueType::TimeInterval,
                    true,
                    true,
                    Some(sc10_time_range()),
                ),
                property(
                    "close_time",
                    "Close time",
                    ValueType::TimeInterval,
                    true,
                    true,
                    Some(sc10_time_range()),
                ),
                trigger_mode_property(),
                property(
                    "repeat_count",
                    "Repeat count",
                    ValueType::I64,
                    true,
                    true,
                    Some(Range {
                        min: Value::I64(1),
                        max: Value::I64(99),
                    }),
                ),
                property(
                    "interlock_closed",
                    "Interlock closed",
                    ValueType::Bool,
                    false,
                    false,
                    None,
                ),
                property("fault", "Fault", ValueType::Bool, false, false, None),
                property(
                    "state_summary",
                    "State summary",
                    ValueType::Map,
                    false,
                    false,
                    None,
                ),
            ],
            metadata: BTreeMap::from([(
                "support_level".into(),
                Value::String("configured_shutter".into()),
            )]),
        }
    }

    fn emit_property(&mut self, device: DeviceId, key: &str, value: Value) {
        self.events
            .push_back(DriverEvent::Event(Event::PropertyChanged(
                PropertyChanged {
                    device,
                    key: key.into(),
                    value,
                },
            )));
    }
}

impl Driver for Sc10Driver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "thorlabs-sc10-serial".into(),
            kind: "serial.ascii".into(),
            metadata: BTreeMap::from([
                (
                    "baud_rate".into(),
                    Value::I64(protocol::DEFAULT_BAUD_RATE as i64),
                ),
                ("data_bits".into(), Value::I64(8)),
                ("parity".into(), Value::String("none".into())),
                ("stop_bits".into(), Value::I64(1)),
                ("flow_control".into(), Value::String("none".into())),
                (
                    "serial_port".into(),
                    self.configured
                        .endpoint
                        .as_ref()
                        .map(|endpoint| Value::String(endpoint.port_name.clone()))
                        .unwrap_or(Value::Null),
                ),
                (
                    "serial_timeout".into(),
                    Value::TimeInterval(TimeInterval::from_milliseconds(
                        self.serial_timeout_ms() as f64
                    )),
                ),
                ("connected".into(), Value::Bool(self.serial.is_some())),
                (
                    "support_level".into(),
                    Value::String("configured_shutter".into()),
                ),
            ]),
        }]
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        vec![self.controller_descriptor(), self.shutter_descriptor()]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.shutter {
            vec![CapabilityDescriptor::new(
                CapabilityId(1),
                device,
                CapabilityKind::TriggerSink,
                ValueType::Bool,
            )]
        } else if device == self.hub {
            vec![CapabilityDescriptor::new(
                CapabilityId(1),
                device,
                CapabilityKind::GenericCommand,
                ValueType::Map,
            )]
        } else {
            Vec::new()
        }
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    self.read_property(*device, key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
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
                            Error::new(ErrorCode::Unsupported, "unknown SC10 capability")
                        })?;
                    match (&candidate.kind, request) {
                        (CapabilityKind::TriggerSink, _) => {
                            if !candidate.accepts_request(request) {
                                return Err(Error::new(
                                    ErrorCode::InvalidCommand,
                                    "SC10 TriggerSink expects None or CapabilityRequest::Trigger",
                                ));
                            }
                            trigger_values(request)?;
                        }
                        (
                            CapabilityKind::GenericCommand,
                            CapabilityRequest::GenericCommand(request),
                        ) if *device == self.hub => {
                            self.validate_generic_command(request)?;
                        }
                        (CapabilityKind::GenericCommand, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "SC10 GenericCommand expects GenericCommandRequest",
                            ));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported SC10 capability",
                            ));
                        }
                    }
                }
                Command::Arm(plan) => self.validate_timing_plan(plan)?,
                Command::Start(_) | Command::Stop(_) => {}
            }
        }
        Ok(PreparedBatch {
            id: batch.id,
            commands: batch.commands.clone(),
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "sc10 configured shutter command set".into(),
                payload: Value::Map(BTreeMap::from([
                    ("device".into(), Value::String("thorlabs-sc10".into())),
                    (
                        "completion".into(),
                        Value::String("immediate configured readback".into()),
                    ),
                ])),
            }],
        })
    }

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.token();
        let mut last = Value::Null;
        for command in prepared.commands {
            last = match command {
                Command::ReadProperty { device, key } => self.read_property_active(device, &key)?,
                Command::WriteProperty { device, key, value } => {
                    self.write_property(device, &key, &value)?
                }
                Command::ApplyStateSet(set) => self.apply_state_set(set)?,
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    let candidate = self
                        .capabilities(device)
                        .into_iter()
                        .find(|candidate| candidate.id == capability)
                        .ok_or_else(|| {
                            Error::new(ErrorCode::Unsupported, "unknown SC10 capability")
                        })?;
                    match (candidate.kind, request) {
                        (CapabilityKind::TriggerSink, request) => self.invoke_trigger(request)?,
                        (
                            CapabilityKind::GenericCommand,
                            CapabilityRequest::GenericCommand(request),
                        ) if device == self.hub => self.apply_generic_command(request)?,
                        (CapabilityKind::GenericCommand, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "SC10 GenericCommand expects GenericCommandRequest",
                            ));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported SC10 capability",
                            ));
                        }
                    }
                }
                Command::Arm(_) | Command::Start(_) | Command::Stop(_) => Value::Null,
            };
        }
        self.events
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
                description: "sc10 timing arm summary".into(),
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
                description: "sc10 timing start sequence".into(),
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
                description: "sc10 timing stop sequence".into(),
                payload: Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "stop")),
                    ("applied".into(), applied),
                ])),
            }],
        })
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        if let Some(serial) = self.serial.as_deref_mut() {
            if let Ok(bytes) = serial.read_available() {
                if !bytes.is_empty() {
                    match self.prompt.push(&bytes) {
                        Ok(Some(reply)) => {
                            self.events
                                .push_back(DriverEvent::Event(Event::Log(LogEvent {
                                    driver: Some(self.id),
                                    message: format!("sc10 serial prompt: {:?}", reply.lines),
                                })));
                        }
                        Ok(None) => {}
                        Err(error) => {
                            self.events
                                .push_back(DriverEvent::Event(Event::Log(LogEvent {
                                    driver: Some(self.id),
                                    message: format!("sc10 serial parse error: {error}"),
                                })));
                        }
                    }
                }
            }
        }
        self.events.drain(..).collect()
    }
}

fn trigger_values(request: &CapabilityRequest) -> Result<Vec<bool>> {
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
                "SC10 TriggerSink expects None or CapabilityRequest::Trigger",
            ))
        }
    };
    Ok(match action {
        TriggerSinkAction::Enable => vec![true],
        TriggerSinkAction::Disable => vec![false],
        TriggerSinkAction::Pulse => vec![true, false],
    })
}

fn property(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    writable: bool,
    sequenceable: bool,
    range: Option<Range>,
) -> PropertySchema {
    PropertySchema {
        key: key.into(),
        display_name: display_name.into(),
        value_type,
        unit: None,
        range,
        increment: None,
        enum_values: Vec::new(),
        readable: true,
        writable,
        volatile: false,
        sequenceable,
        hardware_address: None,
    }
}

fn mode_property() -> PropertySchema {
    let mut schema = property("mode", "Mode", ValueType::String, true, true, None);
    schema.enum_values = protocol::Mode::ALL
        .iter()
        .map(|mode| EnumValue {
            value: Value::String(mode.label().into()),
            label: mode.label().into(),
        })
        .collect();
    schema
}

fn trigger_mode_property() -> PropertySchema {
    let mut schema = property(
        "trigger_mode",
        "Trigger mode",
        ValueType::String,
        true,
        true,
        None,
    );
    schema.enum_values = protocol::TriggerMode::ALL
        .iter()
        .map(|mode| EnumValue {
            value: Value::String(mode.label().into()),
            label: mode.label().into(),
        })
        .collect();
    schema
}

fn validate_sc10_time(key: &str, value: TimeInterval) -> Result<()> {
    let milliseconds = value.seconds() * 1_000.0;
    if !(1.0..=999_999.0).contains(&milliseconds) {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("{key} must be in 1..=999999 milliseconds"),
        ));
    }
    Ok(())
}

fn sc10_time_range() -> Range {
    Range {
        min: Value::TimeInterval(TimeInterval::from_milliseconds(1.0)),
        max: Value::TimeInterval(TimeInterval::from_milliseconds(999_999.0)),
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

fn i64_prop(device: &DeviceConfig, key: &str) -> Option<i64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => Some(*value),
        _ => None,
    }
}

fn u64_prop(device: &DeviceConfig, key: &str) -> Option<u64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if *value >= 0 => Some(*value as u64),
        _ => None,
    }
}

fn time_prop(device: &DeviceConfig, key: &str) -> Option<TimeInterval> {
    match device.properties.get(key) {
        Some(Value::TimeInterval(value)) => Some(*value),
        _ => None,
    }
}

fn mode_prop(device: &DeviceConfig, key: &str) -> Option<protocol::Mode> {
    match device.properties.get(key) {
        Some(Value::String(value)) => protocol::Mode::from_label(value),
        _ => None,
    }
}

fn trigger_mode_prop(device: &DeviceConfig, key: &str) -> Option<protocol::TriggerMode> {
    match device.properties.get(key) {
        Some(Value::String(value)) => protocol::TriggerMode::from_label(value),
        _ => None,
    }
}

fn invalid_property<T>(message: &str, key: &str) -> Result<T> {
    Err(Error::new(
        ErrorCode::InvalidProperty,
        format!("{message}: {key}"),
    ))
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TriggerSinkAction {
    Enable,
    Disable,
    Pulse,
}

#[allow(dead_code)]
mod protocol {
    use super::*;

    pub const DEFAULT_BAUD_RATE: u32 = 9_600;
    pub const PROMPT: &str = ">";
    pub const SEND_ENDING: LineEnding = LineEnding::Cr;
    pub const RECV_ENDING: LineEnding = LineEnding::Cr;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Mode {
        Manual,
        Auto,
        Single,
        Repeat,
        ExternalGate,
    }

    impl Mode {
        pub const ALL: [Self; 5] = [
            Self::Manual,
            Self::Auto,
            Self::Single,
            Self::Repeat,
            Self::ExternalGate,
        ];

        pub fn label(self) -> &'static str {
            match self {
                Self::Manual => "Manual",
                Self::Auto => "Auto",
                Self::Single => "Single",
                Self::Repeat => "Repeat",
                Self::ExternalGate => "ExternalGate",
            }
        }

        pub fn from_label(label: &str) -> Option<Self> {
            match label {
                "Manual" | "manual" | "1" => Some(Self::Manual),
                "Auto" | "auto" | "2" => Some(Self::Auto),
                "Single" | "single" | "3" => Some(Self::Single),
                "Repeat" | "repeat" | "4" => Some(Self::Repeat),
                "ExternalGate" | "external_gate" | "XGate" | "xgate" | "5" => {
                    Some(Self::ExternalGate)
                }
                _ => None,
            }
        }

        fn wire_code(self) -> u8 {
            match self {
                Self::Manual => 1,
                Self::Auto => 2,
                Self::Single => 3,
                Self::Repeat => 4,
                Self::ExternalGate => 5,
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TriggerMode {
        Internal,
        External,
    }

    impl TriggerMode {
        pub const ALL: [Self; 2] = [Self::Internal, Self::External];

        pub fn label(self) -> &'static str {
            match self {
                Self::Internal => "Internal",
                Self::External => "External",
            }
        }

        pub fn from_label(label: &str) -> Option<Self> {
            match label {
                "Internal" | "internal" | "0" => Some(Self::Internal),
                "External" | "external" | "1" => Some(Self::External),
                _ => None,
            }
        }

        fn wire_code(self) -> u8 {
            match self {
                Self::Internal => 0,
                Self::External => 1,
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Query {
        Identity,
        Enabled,
        Mode,
        OpenTime,
        CloseTime,
        TriggerMode,
        RepeatCount,
    }

    impl Query {
        pub const STATE_SUMMARY: [Self; 6] = [
            Self::Enabled,
            Self::Mode,
            Self::OpenTime,
            Self::CloseTime,
            Self::TriggerMode,
            Self::RepeatCount,
        ];
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum Command {
        Query(Query),
        ToggleEnabled,
        SetMode(Mode),
        SetOpenTime(TimeInterval),
        SetCloseTime(TimeInterval),
        SetTriggerMode(TriggerMode),
        SetRepeatCount(i64),
    }

    pub fn encode(command: Command) -> String {
        match command {
            Command::Query(query) => encode_query(query).into(),
            Command::ToggleEnabled => "ens".into(),
            Command::SetMode(mode) => format!("mode={}", mode.wire_code()),
            Command::SetOpenTime(time) => format!("open={}", time_millis(time)),
            Command::SetCloseTime(time) => format!("shut={}", time_millis(time)),
            Command::SetTriggerMode(mode) => format!("trig={}", mode.wire_code()),
            Command::SetRepeatCount(count) => format!("rep={count}"),
        }
    }

    fn encode_query(query: Query) -> &'static str {
        match query {
            Query::Identity => "*idn?",
            Query::Enabled => "ens?",
            Query::Mode => "mode?",
            Query::OpenTime => "open?",
            Query::CloseTime => "shut?",
            Query::TriggerMode => "trig?",
            Query::RepeatCount => "rep?",
        }
    }

    fn time_millis(time: TimeInterval) -> i64 {
        (time.seconds() * 1_000.0).round() as i64
    }

    pub fn probe_script() -> Vec<String> {
        [
            Command::Query(Query::Identity),
            Command::Query(Query::Enabled),
        ]
        .iter()
        .map(|command| format!("{}\r", encode(*command)))
        .collect()
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Identity {
        pub model: String,
        pub firmware_version: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PromptReply {
        pub lines: Vec<String>,
    }

    impl PromptReply {
        pub fn value_line(&self) -> &str {
            self.lines
                .iter()
                .rev()
                .find(|line| !line.trim().is_empty())
                .map(String::as_str)
                .unwrap_or("")
        }
    }

    #[derive(Debug, Clone, Default)]
    pub struct PromptParser {
        buffer: Vec<u8>,
    }

    impl PromptParser {
        pub fn push(&mut self, bytes: &[u8]) -> Result<Option<PromptReply>> {
            self.buffer.extend_from_slice(bytes);
            let Some(index) = self.buffer.iter().position(|byte| *byte == b'>') else {
                return Ok(None);
            };
            let raw = self.buffer.drain(..index).collect::<Vec<_>>();
            self.buffer.drain(..1);
            let text = String::from_utf8(raw)
                .map_err(|_| Error::new(ErrorCode::Transport, "SC10 reply was not UTF-8 text"))?;
            let lines = text
                .split(['\r', '\n'])
                .map(str::trim)
                .filter(|line| !line.is_empty() && *line != PROMPT)
                .map(ToOwned::to_owned)
                .collect();
            Ok(Some(PromptReply { lines }))
        }
    }

    pub fn parse_identity(reply: &str) -> Result<Identity> {
        let trimmed = reply.trim();
        if trimmed.is_empty() {
            return Err(Error::new(
                ErrorCode::Transport,
                "empty SC10 identity reply",
            ));
        }
        let mut model = "SC10".to_string();
        let mut firmware_version = trimmed.to_string();
        for token in trimmed.split([',', ' ']).filter(|token| !token.is_empty()) {
            if token.eq_ignore_ascii_case("SC10") {
                model = "SC10".into();
            } else if token.starts_with('V')
                || token.chars().next().is_some_and(|c| c.is_ascii_digit())
            {
                firmware_version = token.trim_start_matches('V').to_string();
            }
        }
        Ok(Identity {
            model,
            firmware_version,
        })
    }

    pub fn parse_bool(reply: &str) -> Result<bool> {
        match reply.trim().trim_matches('\'').trim_matches('"') {
            "0" => Ok(false),
            "1" => Ok(true),
            other => Err(Error::new(
                ErrorCode::Transport,
                format!("invalid SC10 shutter state reply {other}"),
            )),
        }
    }

    pub fn parse_mode(reply: &str) -> Result<Mode> {
        Mode::from_label(reply.trim()).ok_or_else(|| {
            Error::new(
                ErrorCode::Transport,
                format!("invalid SC10 mode reply {reply}"),
            )
        })
    }

    pub fn parse_trigger_mode(reply: &str) -> Result<TriggerMode> {
        TriggerMode::from_label(reply.trim()).ok_or_else(|| {
            Error::new(
                ErrorCode::Transport,
                format!("invalid SC10 trigger mode reply {reply}"),
            )
        })
    }

    pub fn parse_time(reply: &str) -> Result<TimeInterval> {
        let milliseconds = reply.trim().parse::<f64>().map_err(|_| {
            Error::new(
                ErrorCode::Transport,
                format!("invalid SC10 time reply {reply}"),
            )
        })?;
        Ok(TimeInterval::from_milliseconds(milliseconds))
    }

    pub fn parse_repeat_count(reply: &str) -> Result<i64> {
        let count = reply.trim().parse::<i64>().map_err(|_| {
            Error::new(
                ErrorCode::Transport,
                format!("invalid SC10 repeat count reply {reply}"),
            )
        })?;
        if (1..=99).contains(&count) {
            Ok(count)
        } else {
            Err(Error::new(
                ErrorCode::Transport,
                format!("SC10 repeat count readback out of range {count}"),
            ))
        }
    }
}
