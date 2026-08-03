use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::{LineEnding, ScriptedSerial, SerialIo, SerialLineCodec};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const SEND_ENDING: LineEnding = LineEnding::Cr;
    pub const RECV_ENDING: LineEnding = LineEnding::Cr;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum CounterCommand {
        GateMs(u32),
        Stop,
        IntervalUs(u32),
        PulseQuery,
        PulseIncrement,
        PulseDecrement,
    }

    pub fn encode(command: &CounterCommand) -> String {
        match command {
            CounterCommand::GateMs(ms) => format!("g{ms:03}"),
            CounterCommand::Stop => "s".into(),
            CounterCommand::IntervalUs(us) => format!("i{us}"),
            CounterCommand::PulseQuery => "p?".into(),
            CounterCommand::PulseIncrement => "pi".into(),
            CounterCommand::PulseDecrement => "pd".into(),
        }
    }

    pub fn parse_count(reply: &str) -> Result<u64> {
        reply
            .trim()
            .parse()
            .map_err(|_| Error::new(ErrorCode::Transport, "invalid ArduinoCounter count reply"))
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct CounterSnapshot {
        pub count: u64,
        pub pulse_level: bool,
    }

    impl CounterSnapshot {
        pub fn value(self) -> Value {
            Value::Map(BTreeMap::from([
                ("count".into(), Value::I64(self.count as i64)),
                ("pulse_level".into(), Value::Bool(self.pulse_level)),
            ]))
        }
    }

    pub fn parse_snapshot(reply: &str) -> Result<CounterSnapshot> {
        let mut count = None;
        let mut pulse_level = None;
        for field in reply.trim().split(';') {
            let Some((key, value)) = field.split_once('=') else {
                continue;
            };
            match key.trim() {
                "count" => {
                    count = Some(value.trim().parse::<u64>().map_err(|error| {
                        Error::new(
                            ErrorCode::Transport,
                            format!("invalid ArduinoCounter snapshot count: {error}"),
                        )
                    })?);
                }
                "level" | "pulse_level" => {
                    pulse_level = Some(match value.trim() {
                        "1" | "true" => true,
                        "0" | "false" => false,
                        other => {
                            return Err(Error::new(
                                ErrorCode::Transport,
                                format!("invalid ArduinoCounter pulse level: {other}"),
                            ))
                        }
                    });
                }
                _ => {}
            }
        }
        Ok(CounterSnapshot {
            count: count.ok_or_else(|| {
                Error::new(
                    ErrorCode::Transport,
                    "missing ArduinoCounter snapshot count",
                )
            })?,
            pulse_level: pulse_level.ok_or_else(|| {
                Error::new(
                    ErrorCode::Transport,
                    "missing ArduinoCounter snapshot pulse level",
                )
            })?,
        })
    }
}

pub struct ArduinoCounterDiscovery {
    next_id: DriverId,
    simulated: bool,
    configured: Vec<ArduinoCounterConfiguredProbe>,
}

#[derive(Debug, Clone)]
pub struct ArduinoCounterConfiguredProbe {
    label: String,
    gate_ms: i64,
    interval_us: i64,
    count: u64,
    pulse_level: bool,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connect_real_transport: bool,
}

impl ArduinoCounterDiscovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            simulated: true,
            configured: Vec::new(),
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let configured = config
            .devices
            .iter()
            .filter(|device| {
                matches!(
                    device.driver.as_str(),
                    "arduino_counter" | "arduino-counter" | "mm-arduino-counter"
                )
            })
            .map(ArduinoCounterConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            next_id,
            simulated: false,
            configured,
        })
    }
}

impl DriverDiscovery for ArduinoCounterDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        if self.simulated {
            return Ok(vec![DriverCandidate::from_driver(
                "Simulated Micro-Manager Arduino Counter firmware",
                Box::new(ArduinoCounterDriver::simulated(self.next_id)),
            )]);
        }
        self.configured
            .iter()
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let driver: Box<dyn Driver> = if configured.connect_real_transport {
                    Box::new(ArduinoCounterDriver::serial(id, configured.clone())?)
                } else {
                    Box::new(ArduinoCounterDriver::configured(id, configured.clone()))
                };
                Ok(DriverCandidate::from_driver(
                    configured.label.clone(),
                    driver,
                ))
            })
            .collect()
    }
}

impl ArduinoCounterConfiguredProbe {
    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        Ok(Self {
            label: if device.label.is_empty() {
                "Configured Micro-Manager Arduino Counter firmware".into()
            } else {
                device.label.clone()
            },
            gate_ms: i64_prop(device, "gate_ms")
                .or_else(|| time_ms_prop(device, "gate"))
                .unwrap_or(100)
                .clamp(1, 999),
            interval_us: i64_prop(device, "interval_us")
                .or_else(|| time_us_prop(device, "interval"))
                .unwrap_or(1_000)
                .clamp(1, 10_000_000),
            count: u64_prop(device, "count").unwrap_or(0),
            pulse_level: bool_prop(device, "pulse_level").unwrap_or(false),
            serial_port: string_prop(device, "serial_port"),
            baud_rate: u32_prop(device, "baud_rate").unwrap_or(57_600),
            serial_timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(1_000),
            connect_real_transport: bool_prop(device, "connect").unwrap_or(false),
        })
    }
}

pub struct ArduinoCounterDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    counter: DeviceId,
    pulse: DeviceId,
    gate_ms: i64,
    interval_us: i64,
    count: u64,
    pulse_level: bool,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
}

impl ArduinoCounterDriver {
    pub fn configured(id: DriverId, configured: ArduinoCounterConfiguredProbe) -> Self {
        Self::new(
            id,
            configured.gate_ms,
            configured.interval_us,
            configured.count,
            configured.pulse_level,
            Box::new(ScriptedSerial::new()),
            configured.serial_port,
            configured.baud_rate,
            configured.serial_timeout_ms,
            false,
        )
    }

    pub fn simulated(id: DriverId) -> Self {
        Self::new(
            id,
            100,
            1_000,
            0,
            false,
            Box::new(ScriptedSerial::new()),
            None,
            57_600,
            1_000,
            false,
        )
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: ArduinoCounterConfiguredProbe) -> Result<Self> {
        let port_name = configured.serial_port.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Arduino Counter real serial config requires serial_port",
            )
        })?;
        let serial = Box::new(numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(port_name, configured.baud_rate)
                .timeout(Duration::from_millis(configured.serial_timeout_ms)),
        )?);
        let mut driver = Self::new(
            id,
            configured.gate_ms,
            configured.interval_us,
            configured.count,
            configured.pulse_level,
            serial,
            configured.serial_port,
            configured.baud_rate,
            configured.serial_timeout_ms,
            true,
        );
        driver.refresh_snapshot()?;
        Ok(driver)
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, configured: ArduinoCounterConfiguredProbe) -> Result<Self> {
        let _ = configured.serial_port.as_ref();
        let _ = configured.baud_rate;
        let _ = configured.serial_timeout_ms;
        Err(Error::new(
            ErrorCode::Unsupported,
            "Arduino Counter real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    fn new(
        id: DriverId,
        gate_ms: i64,
        interval_us: i64,
        count: u64,
        pulse_level: bool,
        serial: Box<dyn SerialIo>,
        serial_port: Option<String>,
        baud_rate: u32,
        serial_timeout_ms: u64,
        connected: bool,
    ) -> Self {
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 301)),
            hub: DeviceId(NodeId(id.0 * 1000 + 310)),
            counter: DeviceId(NodeId(id.0 * 1000 + 311)),
            pulse: DeviceId(NodeId(id.0 * 1000 + 312)),
            gate_ms,
            interval_us,
            count,
            pulse_level,
            serial_port,
            baud_rate,
            serial_timeout_ms,
            connected,
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            codec: SerialLineCodec::new(protocol::SEND_ENDING, protocol::RECV_ENDING),
        }
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        vec![
            DeviceDescriptor {
                id: self.hub,
                driver: self.id,
                label: "arduino-counter-hub".into(),
                vendor: Some("Micro-Manager".into()),
                model: Some("Arduino Counter firmware".into()),
                serial: None,
                kinds: vec!["hub".into(), "microcontroller".into()],
                properties: Vec::new(),
                metadata: BTreeMap::from([(
                    "protocol".into(),
                    Value::String("gNNN/s/i/p?/pi/pd".into()),
                )]),
            },
            DeviceDescriptor {
                id: self.counter,
                driver: self.id,
                label: "arduino-counter".into(),
                vendor: Some("Micro-Manager".into()),
                model: Some("Pulse counter".into()),
                serial: None,
                kinds: vec!["counter".into(), "timing.source".into()],
                properties: vec![
                    sequenceable_property(
                        "gate",
                        "Gate time",
                        ValueType::TimeInterval,
                        Some("ms"),
                        true,
                        Some(Range {
                            min: time_interval_ms(1),
                            max: time_interval_ms(999),
                        }),
                    ),
                    property("count", "Count", ValueType::I64, Some("count"), false, None),
                    property(
                        "counter_summary",
                        "Counter summary",
                        ValueType::Map,
                        None,
                        false,
                        None,
                    ),
                    sequenceable_property(
                        "interval",
                        "Interval",
                        ValueType::TimeInterval,
                        Some("us"),
                        true,
                        Some(Range {
                            min: time_interval_us(1),
                            max: time_interval_us(10_000_000),
                        }),
                    ),
                ],
                metadata: BTreeMap::from([("counter_summary".into(), self.counter_summary())]),
            },
            DeviceDescriptor {
                id: self.pulse,
                driver: self.id,
                label: "arduino-counter-pulse".into(),
                vendor: Some("Micro-Manager".into()),
                model: Some("Pulse output".into()),
                serial: None,
                kinds: vec!["trigger.source".into(), "pulse.generator".into()],
                properties: vec![sequenceable_property(
                    "level",
                    "Pulse level",
                    ValueType::Bool,
                    None,
                    true,
                    None,
                )],
                metadata: BTreeMap::new(),
            },
        ]
    }

    fn public_key(key: &str) -> &str {
        match key {
            "gate_ms" => "gate",
            "interval_us" => "interval",
            _ => key,
        }
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        let key = Self::public_key(key);
        match (device, key) {
            (device, "gate") if device == self.counter => Ok(time_interval_ms(self.gate_ms)),
            (device, "count") if device == self.counter => Ok(Value::I64(self.count as i64)),
            (device, "counter_summary") if device == self.counter => Ok(self.counter_summary()),
            (device, "interval") if device == self.counter => {
                Ok(time_interval_us(self.interval_us))
            }
            (device, "level") if device == self.pulse => Ok(Value::Bool(self.pulse_level)),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown ArduinoCounter property {key}"),
            )),
        }
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        let descriptor = self
            .descriptors_for()
            .into_iter()
            .find(|descriptor| descriptor.id == device)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown device"))?;
        let key = Self::public_key(key);
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
        let key = Self::public_key(key);
        match (device, key, value) {
            (device, "gate", value) if device == self.counter => {
                let ms = time_ms(value)?.clamp(1, 999);
                self.gate_ms = ms;
                self.write_line(protocol::CounterCommand::GateMs(ms as u32))?;
                if self.connected {
                    let reply = self.read_line_until(self.serial_timeout_ms)?;
                    self.count = protocol::parse_count(&reply)?;
                } else {
                    self.count = self.count.saturating_add(ms as u64 * 10);
                }
                Ok(time_interval_ms(ms))
            }
            (device, "interval", value) if device == self.counter => {
                let us = time_us(value)?.clamp(1, 10_000_000);
                self.interval_us = us;
                self.write_line(protocol::CounterCommand::IntervalUs(us as u32))?;
                Ok(time_interval_us(us))
            }
            (device, "level", Value::Bool(level)) if device == self.pulse => {
                self.pulse_level = *level;
                self.write_line(if *level {
                    protocol::CounterCommand::PulseIncrement
                } else {
                    protocol::CounterCommand::PulseDecrement
                })?;
                if self.connected {
                    self.refresh_snapshot()?;
                }
                Ok(Value::Bool(*level))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid ArduinoCounter write {key}"),
            )),
        }
    }

    fn write_line(&mut self, command: protocol::CounterCommand) -> Result<()> {
        let line = protocol::encode(&command);
        self.serial.write(&self.codec.encode(&line))
    }

    fn read_line_until(&mut self, timeout_ms: u64) -> Result<String> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            let bytes = self.serial.read_available()?;
            for line in self.codec.push(&bytes) {
                let line = line.trim().to_string();
                if !line.is_empty() {
                    return Ok(line);
                }
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        Err(Error::new(
            ErrorCode::Transport,
            "Arduino Counter did not return a reply",
        ))
    }

    fn refresh_snapshot(&mut self) -> Result<()> {
        self.write_line(protocol::CounterCommand::PulseQuery)?;
        let reply = self.read_line_until(self.serial_timeout_ms)?;
        let snapshot = protocol::parse_snapshot(&reply)?;
        self.apply_snapshot(snapshot);
        Ok(())
    }

    fn apply_snapshot(&mut self, snapshot: protocol::CounterSnapshot) {
        let old_summary = self.counter_summary();
        if self.count != snapshot.count {
            self.count = snapshot.count;
            self.emit_property(self.counter, "count", Value::I64(self.count as i64));
        }
        if self.pulse_level != snapshot.pulse_level {
            self.pulse_level = snapshot.pulse_level;
            self.emit_property(self.pulse, "level", Value::Bool(self.pulse_level));
        }
        let new_summary = self.counter_summary();
        if old_summary != new_summary {
            self.emit_property(self.counter, "counter_summary", new_summary);
        }
    }

    fn counter_summary(&self) -> Value {
        protocol::CounterSnapshot {
            count: self.count,
            pulse_level: self.pulse_level,
        }
        .value()
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
        if device != self.hub && device != self.counter {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "ArduinoCounter GenericCommand is only available on the hub and counter",
            ));
        }
        if !request.params.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "ArduinoCounter GenericCommand does not take parameters",
            ));
        }
        if request.command != "refresh_snapshot" {
            return Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "ArduinoCounter GenericCommand supports refresh_snapshot; got {}",
                    request.command
                ),
            ));
        }
        Ok(())
    }

    fn apply_generic_command(
        &mut self,
        device: DeviceId,
        request: GenericCommandRequest,
    ) -> Result<Value> {
        self.validate_generic_command(device, &request)?;
        if self.connected {
            self.refresh_snapshot()?;
        } else {
            self.write_line(protocol::CounterCommand::PulseQuery)?;
        }
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("commands".into(), Value::I64(1)),
            ("counter_summary".into(), self.counter_summary()),
            (
                "completion_basis".into(),
                Value::String("ArduinoCounter mapped snapshot readback".into()),
            ),
        ])))
    }

    fn owns_device(&self, device: DeviceId) -> bool {
        device == self.hub || device == self.counter || device == self.pulse
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

    fn has_timed_pulse(&self, plan: &TimingPlan) -> bool {
        plan.participants.contains(&self.pulse)
            || plan
                .routes
                .iter()
                .any(|route| route.from == self.pulse || route.to == self.pulse)
            || plan
                .sequences
                .iter()
                .any(|sequence| sequence.device == self.pulse)
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
                    "ArduinoCounter timing sequence must contain at least one value",
                ));
            }
            let property = Self::public_key(&sequence.property);
            match (sequence.device, property) {
                (device, "gate" | "interval") if device == self.counter => {}
                (device, "level") if device == self.pulse => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        format!(
                            "ArduinoCounter timing does not support {} on {:?}",
                            sequence.property, sequence.device
                        ),
                    ))
                }
            }
            let descriptor = descriptors
                .iter()
                .find(|descriptor| descriptor.id == sequence.device)
                .ok_or_else(|| {
                    Error::new(ErrorCode::InvalidCommand, "unknown ArduinoCounter device")
                })?;
            let schema = descriptor
                .properties
                .iter()
                .find(|schema| schema.key == property)
                .ok_or_else(|| {
                    Error::new(
                        ErrorCode::InvalidProperty,
                        "unknown ArduinoCounter property",
                    )
                })?;
            if !schema.sequenceable {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!(
                        "ArduinoCounter property {} is not sequenceable",
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
            let property = Self::public_key(&sequence.property);
            let value = (if start {
                sequence.values.first()
            } else {
                sequence.values.last()
            })
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidCommand,
                    "ArduinoCounter timing sequence must contain at least one value",
                )
            })?
            .clone();
            let applied_value = self.write_property(sequence.device, property, &value)?;
            self.emit_property(sequence.device, property, applied_value.clone());
            if sequence.device == self.counter && property == "gate" {
                self.emit_property(self.counter, "count", Value::I64(self.count as i64));
                self.emit_property(self.counter, "counter_summary", self.counter_summary());
            }
            applied.insert(
                format!("{}:{}", sequence.device.0 .0, property),
                applied_value,
            );
        }
        Ok(Value::Map(applied))
    }

    fn timing_summary(&self, plan: &TimingPlan, action: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("action".into(), Value::String(action.into())),
            ("counter".into(), Value::I64(self.counter.0 .0 as i64)),
            ("pulse".into(), Value::I64(self.pulse.0 .0 as i64)),
            ("gate".into(), time_interval_ms(self.gate_ms)),
            ("interval".into(), time_interval_us(self.interval_us)),
            ("count".into(), Value::I64(self.count as i64)),
            ("pulse_level".into(), Value::Bool(self.pulse_level)),
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
        command: protocol::CounterCommand,
    ) -> PhysicalTransaction {
        let line = protocol::encode(&command);
        PhysicalTransaction {
            resource: Some(self.resource),
            description: description.into(),
            payload: Value::Bytes(self.codec.encode(&line)),
        }
    }

    fn measure_command(&self, request: &CapabilityRequest) -> Result<protocol::CounterCommand> {
        let gate_ms = match request {
            CapabilityRequest::None => self.gate_ms,
            CapabilityRequest::Measure(request) => request
                .integration_time
                .map(|interval| interval.seconds() * 1_000.0)
                .unwrap_or(self.gate_ms as f64)
                as i64,
            _ => {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    "ArduinoCounter Measure expects None or Measure",
                ))
            }
        };
        Ok(protocol::CounterCommand::GateMs(
            gate_ms.clamp(1, 999) as u32
        ))
    }

    fn pulse_program_commands(
        &self,
        request: &CapabilityRequest,
    ) -> Result<Vec<protocol::CounterCommand>> {
        let CapabilityRequest::PulseProgram(request) = request else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "ArduinoCounter PulseProgram expects PulseProgram",
            ));
        };
        let interval_us = request
            .interval
            .map(|interval| interval.microseconds().round() as i64)
            .unwrap_or(self.interval_us as i64)
            .clamp(1, 10_000_000);
        Ok(vec![protocol::CounterCommand::IntervalUs(
            interval_us as u32,
        )])
    }

    fn trigger_source_commands(
        &self,
        request: &CapabilityRequest,
    ) -> Result<Vec<protocol::CounterCommand>> {
        let action = match request {
            CapabilityRequest::None => TriggerAction::Pulse,
            CapabilityRequest::Trigger(request) => match request.action {
                numanager_core::TriggerAction::Enable => TriggerAction::Start,
                numanager_core::TriggerAction::Disable => TriggerAction::Stop,
                numanager_core::TriggerAction::Pulse => TriggerAction::Pulse,
            },
            _ => {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    "ArduinoCounter TriggerSource expects None or Trigger",
                ))
            }
        };
        Ok(match action {
            TriggerAction::Start => vec![protocol::CounterCommand::PulseIncrement],
            TriggerAction::Stop => vec![protocol::CounterCommand::PulseDecrement],
            TriggerAction::Pulse => vec![
                protocol::CounterCommand::PulseIncrement,
                protocol::CounterCommand::PulseDecrement,
            ],
        })
    }

    fn invoke_transactions(
        &self,
        device: DeviceId,
        kind: CapabilityKind,
        request: &CapabilityRequest,
    ) -> Result<Vec<protocol::CounterCommand>> {
        match kind {
            CapabilityKind::Measure if device == self.counter => {
                Ok(vec![self.measure_command(request)?])
            }
            CapabilityKind::PulseProgram if device == self.counter => {
                self.pulse_program_commands(request)
            }
            CapabilityKind::TriggerSource if device == self.pulse => {
                self.trigger_source_commands(request)
            }
            CapabilityKind::GenericCommand if device == self.hub || device == self.counter => {
                let CapabilityRequest::GenericCommand(request) = request else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "ArduinoCounter GenericCommand expects GenericCommandRequest",
                    ));
                };
                self.validate_generic_command(device, request)?;
                Ok(vec![protocol::CounterCommand::PulseQuery])
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported ArduinoCounter invocation capability",
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
            CapabilityKind::Measure if device == self.counter => {
                let command = self.measure_command(&request)?;
                let protocol::CounterCommand::GateMs(ms) = command else {
                    unreachable!()
                };
                let value = self.write_property(
                    self.counter,
                    "gate",
                    &Value::TimeInterval(TimeInterval::from_milliseconds(ms as f64)),
                )?;
                self.emit_property(self.counter, "gate", value.clone());
                self.emit_property(self.counter, "count", Value::I64(self.count as i64));
                self.emit_property(self.counter, "counter_summary", self.counter_summary());
                Ok(Value::Map(BTreeMap::from([
                    ("gate".into(), value),
                    ("count".into(), Value::I64(self.count as i64)),
                    ("counter_summary".into(), self.counter_summary()),
                    ("commands".into(), Value::I64(1)),
                ])))
            }
            CapabilityKind::PulseProgram if device == self.counter => {
                let commands = self.pulse_program_commands(&request)?;
                for command in &commands {
                    if let protocol::CounterCommand::IntervalUs(us) = command {
                        let value = self.write_property(
                            self.counter,
                            "interval",
                            &Value::TimeInterval(TimeInterval::from_microseconds(*us as f64)),
                        )?;
                        self.emit_property(self.counter, "interval", value);
                    } else {
                        self.write_line(command.clone())?;
                    }
                }
                self.emit_property(self.counter, "counter_summary", self.counter_summary());
                Ok(Value::Map(BTreeMap::from([
                    ("interval".into(), time_interval_us(self.interval_us)),
                    ("counter_summary".into(), self.counter_summary()),
                    ("commands".into(), Value::I64(commands.len() as i64)),
                ])))
            }
            CapabilityKind::TriggerSource if device == self.pulse => {
                let commands = self.trigger_source_commands(&request)?;
                for command in &commands {
                    match command {
                        protocol::CounterCommand::PulseIncrement => {
                            let value =
                                self.write_property(self.pulse, "level", &Value::Bool(true))?;
                            self.emit_property(self.pulse, "level", value);
                        }
                        protocol::CounterCommand::PulseDecrement => {
                            let value =
                                self.write_property(self.pulse, "level", &Value::Bool(false))?;
                            self.emit_property(self.pulse, "level", value);
                        }
                        _ => self.write_line(command.clone())?,
                    }
                }
                Ok(Value::Map(BTreeMap::from([
                    ("triggered".into(), Value::Bool(true)),
                    ("level".into(), Value::Bool(self.pulse_level)),
                    ("commands".into(), Value::I64(commands.len() as i64)),
                ])))
            }
            CapabilityKind::GenericCommand if device == self.hub || device == self.counter => {
                let CapabilityRequest::GenericCommand(request) = request else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "ArduinoCounter GenericCommand expects GenericCommandRequest",
                    ));
                };
                self.apply_generic_command(device, request)
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported ArduinoCounter invocation capability",
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
}

impl Driver for ArduinoCounterDriver {
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
            label: "arduino-counter-serial".into(),
            kind: "serial.text".into(),
            metadata: BTreeMap::from([
                ("send_ending".into(), Value::String("cr".into())),
                ("recv_ending".into(), Value::String("cr".into())),
                ("baud_rate".into(), Value::I64(self.baud_rate as i64)),
                ("connected".into(), Value::Bool(self.connected)),
                (
                    "serial_port".into(),
                    self.serial_port
                        .as_ref()
                        .map(|port| Value::String(port.clone()))
                        .unwrap_or(Value::Null),
                ),
            ]),
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        match device {
            device if device == self.hub => {
                vec![capability(1, device, CapabilityKind::GenericCommand)]
            }
            device if device == self.counter => vec![
                capability(1, device, CapabilityKind::Measure),
                capability(2, device, CapabilityKind::PulseProgram),
                capability(3, device, CapabilityKind::GenericCommand),
            ],
            device if device == self.pulse => {
                vec![capability(3, device, CapabilityKind::TriggerSource)]
            }
            _ => Vec::new(),
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
                        description: format!("arduino-counter read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("arduino-counter write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "arduino-counter remultiplexed pulse/counter state set".into(),
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
                            "unknown ArduinoCounter capability",
                        ));
                    };
                    if !capability.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "{} expects {:?}, got {:?}",
                                capability.kind.name(),
                                capability.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    for command in self.invoke_transactions(*device, capability.kind, request)? {
                        physical_transactions.push(
                            self.timing_transaction("arduino-counter direct invocation", command),
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
                    let public_key = Self::public_key(&key);
                    if device == self.counter
                        && (public_key == "count" || public_key == "counter_summary")
                    {
                        if self.connected {
                            self.refresh_snapshot()?;
                        } else {
                            self.write_line(protocol::CounterCommand::PulseQuery)?;
                        }
                    }
                    last = self.read_property(device, public_key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    let public_key = Self::public_key(&key);
                    last = self.write_property(device, public_key, &value)?;
                    self.emit_property(device, public_key, last.clone());
                    if device == self.counter && public_key == "gate" {
                        self.emit_property(device, "count", Value::I64(self.count as i64));
                        self.emit_property(device, "counter_summary", self.counter_summary());
                    }
                }
                Command::ApplyStateSet(set) => {
                    let mut result = BTreeMap::new();
                    for write in set.writes {
                        let property = Self::public_key(&write.property);
                        let value = self.write_property(write.device, property, &write.value)?;
                        self.emit_property(write.device, property, value.clone());
                        if write.device == self.counter && property == "gate" {
                            self.emit_property(
                                write.device,
                                "count",
                                Value::I64(self.count as i64),
                            );
                            self.emit_property(
                                write.device,
                                "counter_summary",
                                self.counter_summary(),
                            );
                        }
                        result.insert(format!("{}:{}", (write.device.0).0, write.property), value);
                    }
                    last = Value::Map(result);
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
                            "unknown ArduinoCounter capability",
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
                        message: format!("arduino-counter serial: {line}"),
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
                description: "arduino-counter timing arm summary".into(),
                payload: self.timing_summary(plan, "arm"),
            }],
        })
    }

    fn start_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let mut physical_transactions = Vec::new();
        let applied = self.apply_timing_sequence_step(&armed.plan, true)?;
        if self.has_timed_pulse(&armed.plan)
            && !self.has_explicit_sequence(&armed.plan, self.pulse, "level")
        {
            let value = self.write_property(self.pulse, "level", &Value::Bool(true))?;
            self.emit_property(self.pulse, "level", value);
            physical_transactions.push(self.timing_transaction(
                "arduino-counter timing start pulse high",
                protocol::CounterCommand::PulseIncrement,
            ));
        }
        physical_transactions.push(PhysicalTransaction {
            resource: Some(self.resource),
            description: "arduino-counter timing start summary".into(),
            payload: with_applied(self.timing_summary(&armed.plan, "start"), applied),
        });
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Start(OperationId(0))],
            physical_transactions,
        })
    }

    fn stop_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let mut physical_transactions = Vec::new();
        let applied = self.apply_timing_sequence_step(&armed.plan, false)?;
        if self.has_timed_pulse(&armed.plan)
            && !self.has_explicit_sequence(&armed.plan, self.pulse, "level")
        {
            let value = self.write_property(self.pulse, "level", &Value::Bool(false))?;
            self.emit_property(self.pulse, "level", value);
            physical_transactions.push(self.timing_transaction(
                "arduino-counter timing stop pulse low",
                protocol::CounterCommand::PulseDecrement,
            ));
        }
        physical_transactions.push(PhysicalTransaction {
            resource: Some(self.resource),
            description: "arduino-counter timing stop summary".into(),
            payload: with_applied(self.timing_summary(&armed.plan, "stop"), applied),
        });
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions,
        })
    }
}

fn capability(id: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    CapabilityDescriptor::new(CapabilityId(id), device, kind, ValueType::Map)
}

fn time_interval_ms(ms: i64) -> Value {
    Value::TimeInterval(TimeInterval::from_milliseconds(ms as f64))
}

fn time_interval_us(us: i64) -> Value {
    Value::TimeInterval(TimeInterval::from_microseconds(us as f64))
}

fn time_ms(value: &Value) -> Result<i64> {
    match value {
        Value::TimeInterval(interval) => Ok(interval.seconds().mul_add(1000.0, 0.0).round() as i64),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            "gate expects typed time interval",
        )),
    }
}

fn time_us(value: &Value) -> Result<i64> {
    match value {
        Value::TimeInterval(interval) => Ok(interval.microseconds().round() as i64),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            "interval expects typed time interval",
        )),
    }
}

fn string_prop(device: &DeviceConfig, key: &str) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
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

fn u32_prop(device: &DeviceConfig, key: &str) -> Option<u32> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => u32::try_from(*value).ok(),
        _ => None,
    }
}

fn u64_prop(device: &DeviceConfig, key: &str) -> Option<u64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => u64::try_from(*value).ok(),
        _ => None,
    }
}

fn time_ms_prop(device: &DeviceConfig, key: &str) -> Option<i64> {
    match device.properties.get(key) {
        Some(Value::TimeInterval(value)) => Some((value.seconds() * 1_000.0).round() as i64),
        _ => None,
    }
}

fn time_us_prop(device: &DeviceConfig, key: &str) -> Option<i64> {
    match device.properties.get(key) {
        Some(Value::TimeInterval(value)) => Some(value.microseconds().round() as i64),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TriggerAction {
    Start,
    Stop,
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

fn with_applied(mut summary: Value, applied: Value) -> Value {
    if let Value::Map(map) = &mut summary {
        map.insert("applied".into(), applied);
    }
    summary
}
