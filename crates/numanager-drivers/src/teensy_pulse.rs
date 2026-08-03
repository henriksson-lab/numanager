use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::{FixedBinaryCodec, ScriptedSerial, SerialIo};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const BAUD: u32 = 115_200;
    pub const CMD_VERSION: u8 = 0;
    pub const CMD_START: u8 = 1;
    pub const CMD_STOP: u8 = 2;
    pub const CMD_INTERVAL: u8 = 3;
    pub const CMD_PULSE_DURATION: u8 = 4;
    pub const CMD_WAIT_FOR_INPUT: u8 = 5;
    pub const CMD_NUMBER_OF_PULSES: u8 = 6;
    pub const CMD_ENQUIRE: u8 = 255;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PulseCommand {
        Version,
        Start,
        Stop,
        IntervalUs,
        PulseDurationUs,
        WaitForInput,
        NumberOfPulses,
    }

    impl PulseCommand {
        pub fn opcode(self) -> u8 {
            match self {
                PulseCommand::Version => CMD_VERSION,
                PulseCommand::Start => CMD_START,
                PulseCommand::Stop => CMD_STOP,
                PulseCommand::IntervalUs => CMD_INTERVAL,
                PulseCommand::PulseDurationUs => CMD_PULSE_DURATION,
                PulseCommand::WaitForInput => CMD_WAIT_FOR_INPUT,
                PulseCommand::NumberOfPulses => CMD_NUMBER_OF_PULSES,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum TeensyFrame {
        Set {
            command: PulseCommand,
            parameter: u32,
        },
        Enquire {
            command: PulseCommand,
        },
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TeensyReply {
        pub command: u8,
        pub value: u32,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PulseProgramSnapshot {
        pub interval_us: u32,
        pub duration_us: u32,
        pub wait_for_input: bool,
        pub number_of_pulses: u32,
        pub running: bool,
        pub counted_pulses: u32,
    }

    impl PulseProgramSnapshot {
        pub fn value(&self) -> Value {
            Value::Map(BTreeMap::from([
                ("interval".into(), time_interval_us(self.interval_us)),
                ("duration".into(), time_interval_us(self.duration_us)),
                ("wait_for_input".into(), Value::Bool(self.wait_for_input)),
                (
                    "number_of_pulses".into(),
                    Value::I64(self.number_of_pulses as i64),
                ),
                ("running".into(), Value::Bool(self.running)),
                (
                    "counted_pulses".into(),
                    Value::I64(self.counted_pulses as i64),
                ),
            ]))
        }

        pub fn apply_reply(&mut self, reply: TeensyReply) -> Result<()> {
            match reply.command {
                CMD_VERSION => {}
                CMD_INTERVAL => self.interval_us = reply.value,
                CMD_PULSE_DURATION => self.duration_us = reply.value,
                CMD_WAIT_FOR_INPUT => self.wait_for_input = reply.value != 0,
                CMD_NUMBER_OF_PULSES => self.number_of_pulses = reply.value,
                CMD_START => self.running = reply.value != 0,
                CMD_STOP => self.running = false,
                CMD_ENQUIRE => self.counted_pulses = reply.value,
                other => {
                    return Err(Error::new(
                        ErrorCode::Transport,
                        format!("unknown TeensyPulse reply command {other}"),
                    ))
                }
            }
            Ok(())
        }
    }

    pub fn encode(frame: &TeensyFrame) -> Vec<u8> {
        match frame {
            TeensyFrame::Set { command, parameter } => {
                let mut bytes = vec![command.opcode()];
                bytes.extend_from_slice(&parameter.to_le_bytes());
                bytes
            }
            TeensyFrame::Enquire { command } => vec![CMD_ENQUIRE, command.opcode()],
        }
    }

    pub fn decode_reply(bytes: &[u8]) -> Result<TeensyReply> {
        if bytes.len() != 5 {
            return Err(Error::new(
                ErrorCode::Transport,
                "TeensyPulse reply must be 5 bytes",
            ));
        }
        Ok(TeensyReply {
            command: bytes[0],
            value: u32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]),
        })
    }

    pub fn snapshot_from_replies(
        initial: PulseProgramSnapshot,
        replies: &[impl AsRef<[u8]>],
    ) -> Result<PulseProgramSnapshot> {
        let mut snapshot = initial;
        for reply in replies {
            snapshot.apply_reply(decode_reply(reply.as_ref())?)?;
        }
        Ok(snapshot)
    }
}

pub struct TeensyPulseDiscovery {
    next_id: DriverId,
    simulated: bool,
    configured: Vec<TeensyPulseConfiguredProbe>,
}

#[derive(Debug, Clone)]
pub struct TeensyPulseConfiguredProbe {
    label: String,
    version: u32,
    interval_us: u32,
    duration_us: u32,
    wait_for_input: bool,
    number_of_pulses: u32,
    counted_pulses: u32,
    running: bool,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connect_real_transport: bool,
}

impl TeensyPulseDiscovery {
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
                    "teensy_pulse" | "teensy-pulse" | "mm-teensy-pulse"
                )
            })
            .map(TeensyPulseConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            next_id,
            simulated: false,
            configured,
        })
    }
}

impl DriverDiscovery for TeensyPulseDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        if self.simulated {
            return Ok(vec![DriverCandidate::from_driver(
                "Simulated Teensy pulse generator firmware",
                Box::new(TeensyPulseDriver::simulated(self.next_id)),
            )]);
        }
        self.configured
            .iter()
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let driver: Box<dyn Driver> = if configured.connect_real_transport {
                    Box::new(TeensyPulseDriver::serial(id, configured.clone())?)
                } else {
                    Box::new(TeensyPulseDriver::configured(id, configured.clone()))
                };
                Ok(DriverCandidate::from_driver(
                    configured.label.clone(),
                    driver,
                ))
            })
            .collect()
    }
}

impl TeensyPulseConfiguredProbe {
    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        Ok(Self {
            label: if device.label.is_empty() {
                "Configured Teensy pulse generator firmware".into()
            } else {
                device.label.clone()
            },
            version: u32_prop(device, "version").unwrap_or(1),
            interval_us: u32_prop(device, "interval_us")
                .or_else(|| time_us_prop(device, "interval"))
                .unwrap_or(100_000)
                .max(1),
            duration_us: u32_prop(device, "duration_us")
                .or_else(|| time_us_prop(device, "duration"))
                .unwrap_or(1_000)
                .max(1),
            wait_for_input: bool_prop(device, "wait_for_input").unwrap_or(false),
            number_of_pulses: u32_prop(device, "number_of_pulses").unwrap_or(10),
            counted_pulses: u32_prop(device, "counted_pulses").unwrap_or(0),
            running: bool_prop(device, "running").unwrap_or(false),
            serial_port: string_prop(device, "serial_port"),
            baud_rate: u32_prop(device, "baud_rate").unwrap_or(protocol::BAUD),
            serial_timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(500),
            connect_real_transport: bool_prop(device, "connect").unwrap_or(false),
        })
    }
}

pub struct TeensyPulseDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    pulse: DeviceId,
    version: u32,
    interval_us: u32,
    duration_us: u32,
    wait_for_input: bool,
    number_of_pulses: u32,
    counted_pulses: u32,
    running: bool,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    reply_codec: FixedBinaryCodec,
}

impl TeensyPulseDriver {
    pub fn configured(id: DriverId, configured: TeensyPulseConfiguredProbe) -> Self {
        Self::new(
            id,
            configured.version,
            configured.interval_us,
            configured.duration_us,
            configured.wait_for_input,
            configured.number_of_pulses,
            configured.counted_pulses,
            configured.running,
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
            1,
            100_000,
            1_000,
            false,
            10,
            0,
            false,
            Box::new(ScriptedSerial::new()),
            None,
            protocol::BAUD,
            500,
            false,
        )
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: TeensyPulseConfiguredProbe) -> Result<Self> {
        let port_name = configured.serial_port.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Teensy Pulse real serial config requires serial_port",
            )
        })?;
        let serial = Box::new(numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(port_name, configured.baud_rate)
                .timeout(Duration::from_millis(configured.serial_timeout_ms)),
        )?);
        let mut driver = Self::new(
            id,
            configured.version,
            configured.interval_us,
            configured.duration_us,
            configured.wait_for_input,
            configured.number_of_pulses,
            configured.counted_pulses,
            configured.running,
            serial,
            configured.serial_port,
            configured.baud_rate,
            configured.serial_timeout_ms,
            true,
        );
        driver.refresh_startup_state()?;
        Ok(driver)
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, configured: TeensyPulseConfiguredProbe) -> Result<Self> {
        let _ = configured.serial_port.as_ref();
        let _ = configured.baud_rate;
        let _ = configured.serial_timeout_ms;
        Err(Error::new(
            ErrorCode::Unsupported,
            "Teensy Pulse real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        id: DriverId,
        version: u32,
        interval_us: u32,
        duration_us: u32,
        wait_for_input: bool,
        number_of_pulses: u32,
        counted_pulses: u32,
        running: bool,
        serial: Box<dyn SerialIo>,
        serial_port: Option<String>,
        baud_rate: u32,
        serial_timeout_ms: u64,
        connected: bool,
    ) -> Self {
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 401)),
            hub: DeviceId(NodeId(id.0 * 1000 + 410)),
            pulse: DeviceId(NodeId(id.0 * 1000 + 411)),
            version,
            interval_us,
            duration_us,
            wait_for_input,
            number_of_pulses,
            counted_pulses,
            running,
            serial_port,
            baud_rate,
            serial_timeout_ms,
            connected,
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            reply_codec: FixedBinaryCodec::new(5),
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
                label: "teensy-pulse-hub".into(),
                vendor: Some("Micro-Manager".into()),
                model: Some("Teensy pulse generator firmware".into()),
                serial: None,
                kinds: vec!["hub".into(), "microcontroller".into()],
                properties: Vec::new(),
                metadata: BTreeMap::from([
                    ("firmware_version".into(), Value::I64(self.version as i64)),
                    ("wire_integer_endian".into(), Value::String("little".into())),
                ]),
            },
            DeviceDescriptor {
                id: self.pulse,
                driver: self.id,
                label: "teensy-pulse-generator".into(),
                vendor: Some("Micro-Manager".into()),
                model: Some("TTL pulse generator".into()),
                serial: None,
                kinds: vec![
                    "trigger.source".into(),
                    "pulse.generator".into(),
                    "timing.source".into(),
                ],
                properties: vec![
                    sequenceable_property(
                        "interval",
                        "Interval",
                        ValueType::TimeInterval,
                        Some("us"),
                        true,
                        Some(Range {
                            min: time_interval_us(1),
                            max: time_interval_us(u32::MAX),
                        }),
                    ),
                    sequenceable_property(
                        "duration",
                        "Pulse duration",
                        ValueType::TimeInterval,
                        Some("us"),
                        true,
                        Some(Range {
                            min: time_interval_us(1),
                            max: time_interval_us(u32::MAX),
                        }),
                    ),
                    sequenceable_property(
                        "wait_for_input",
                        "Wait for input",
                        ValueType::Bool,
                        None,
                        true,
                        None,
                    ),
                    sequenceable_property(
                        "number_of_pulses",
                        "Number of pulses",
                        ValueType::I64,
                        Some("count"),
                        true,
                        Some(Range {
                            min: Value::I64(0),
                            max: Value::I64(u32::MAX as i64),
                        }),
                    ),
                    sequenceable_property("running", "Running", ValueType::Bool, None, true, None),
                    property(
                        "counted_pulses",
                        "Counted pulses",
                        ValueType::I64,
                        Some("count"),
                        false,
                        None,
                    ),
                    property(
                        "program_summary",
                        "Program summary",
                        ValueType::Map,
                        None,
                        false,
                        None,
                    ),
                ],
                metadata: BTreeMap::from([("program_summary".into(), self.program_summary())]),
            },
        ]
    }

    fn public_key(key: &str) -> &str {
        match key {
            "interval_us" => "interval",
            "duration_us" => "duration",
            _ => key,
        }
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device != self.pulse {
            return Err(Error::new(ErrorCode::InvalidCommand, "unknown device"));
        }
        let key = Self::public_key(key);
        match key {
            "interval" => Ok(time_interval_us(self.interval_us)),
            "duration" => Ok(time_interval_us(self.duration_us)),
            "wait_for_input" => Ok(Value::Bool(self.wait_for_input)),
            "number_of_pulses" => Ok(Value::I64(self.number_of_pulses as i64)),
            "running" => Ok(Value::Bool(self.running)),
            "counted_pulses" => Ok(Value::I64(self.counted_pulses as i64)),
            "program_summary" => Ok(self.program_summary()),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown TeensyPulse property {key}"),
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
        if device != self.pulse {
            return Err(Error::new(ErrorCode::InvalidCommand, "unknown device"));
        }
        let key = Self::public_key(key);
        match (key, value) {
            ("interval", value) => {
                self.interval_us = time_us(value)?.clamp(1, u32::MAX);
                self.send_set(protocol::PulseCommand::IntervalUs, self.interval_us)?;
                Ok(time_interval_us(self.interval_us))
            }
            ("duration", value) => {
                self.duration_us = time_us(value)?.clamp(1, u32::MAX);
                self.send_set(protocol::PulseCommand::PulseDurationUs, self.duration_us)?;
                Ok(time_interval_us(self.duration_us))
            }
            ("wait_for_input", Value::Bool(wait)) => {
                self.wait_for_input = *wait;
                self.send_set(protocol::PulseCommand::WaitForInput, u32::from(*wait))?;
                Ok(Value::Bool(*wait))
            }
            ("number_of_pulses", Value::I64(count)) => {
                self.number_of_pulses = clamp_u32(*count, 0);
                self.send_set(
                    protocol::PulseCommand::NumberOfPulses,
                    self.number_of_pulses,
                )?;
                Ok(Value::I64(self.number_of_pulses as i64))
            }
            ("running", Value::Bool(running)) => {
                self.running = *running;
                if *running {
                    self.counted_pulses = 0;
                    self.send_set(protocol::PulseCommand::Start, 0)?;
                } else {
                    self.counted_pulses = if self.number_of_pulses == 0 {
                        self.counted_pulses.saturating_add(1)
                    } else {
                        self.number_of_pulses
                    };
                    self.send_set(protocol::PulseCommand::Stop, 0)?;
                }
                Ok(Value::Bool(*running))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid TeensyPulse write {key}"),
            )),
        }
    }

    fn send_set(&mut self, command: protocol::PulseCommand, parameter: u32) -> Result<()> {
        self.serial
            .write(&protocol::encode(&protocol::TeensyFrame::Set {
                command,
                parameter,
            }))?;
        if self.connected {
            self.read_reply_until()?;
        }
        Ok(())
    }

    fn send_enquire(&mut self, command: protocol::PulseCommand) -> Result<()> {
        self.serial
            .write(&protocol::encode(&protocol::TeensyFrame::Enquire {
                command,
            }))
    }

    #[cfg(feature = "os-serial")]
    fn refresh_startup_state(&mut self) -> Result<()> {
        self.send_enquire(protocol::PulseCommand::Version)?;
        self.read_reply_until()?;
        self.send_enquire(protocol::PulseCommand::IntervalUs)?;
        self.read_reply_until()?;
        self.send_enquire(protocol::PulseCommand::PulseDurationUs)?;
        self.read_reply_until()?;
        self.send_enquire(protocol::PulseCommand::WaitForInput)?;
        self.read_reply_until()?;
        self.send_enquire(protocol::PulseCommand::NumberOfPulses)?;
        self.read_reply_until()?;
        self.send_enquire(protocol::PulseCommand::Start)?;
        self.read_reply_until()?;
        Ok(())
    }

    fn read_reply_until(&mut self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_millis(self.serial_timeout_ms.max(1));
        loop {
            if self.drain_replies()? > 0 {
                return Ok(());
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        Err(Error::new(
            ErrorCode::Transport,
            "Teensy Pulse did not return a binary reply",
        ))
    }

    fn program_snapshot(&self) -> protocol::PulseProgramSnapshot {
        protocol::PulseProgramSnapshot {
            interval_us: self.interval_us,
            duration_us: self.duration_us,
            wait_for_input: self.wait_for_input,
            number_of_pulses: self.number_of_pulses,
            running: self.running,
            counted_pulses: self.counted_pulses,
        }
    }

    fn program_summary(&self) -> Value {
        self.program_snapshot().value()
    }

    fn generic_enquiries_for(command: &str) -> Result<Vec<protocol::PulseCommand>> {
        match command {
            "refresh_readbacks" => Ok(vec![
                protocol::PulseCommand::IntervalUs,
                protocol::PulseCommand::PulseDurationUs,
                protocol::PulseCommand::WaitForInput,
                protocol::PulseCommand::NumberOfPulses,
                protocol::PulseCommand::Start,
            ]),
            "refresh_program" => Ok(vec![
                protocol::PulseCommand::IntervalUs,
                protocol::PulseCommand::PulseDurationUs,
                protocol::PulseCommand::WaitForInput,
                protocol::PulseCommand::NumberOfPulses,
            ]),
            "refresh_running" | "refresh_counted_pulses" => Ok(vec![protocol::PulseCommand::Start]),
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "TeensyPulse GenericCommand supports refresh_readbacks, refresh_program, refresh_running, and refresh_counted_pulses; got {other}"
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
                "TeensyPulse GenericCommand does not take parameters",
            ));
        }
        let _ = Self::generic_enquiries_for(&request.command)?;
        Ok(())
    }

    fn apply_generic_command(&mut self, request: GenericCommandRequest) -> Result<Value> {
        self.validate_generic_command(&request)?;
        let enquiries = Self::generic_enquiries_for(&request.command)?;
        for command in &enquiries {
            self.send_enquire(*command)?;
        }
        if self.connected {
            for _ in 0..enquiries.len() {
                self.read_reply_until()?;
            }
        } else {
            let _ = self.drain_replies()?;
        }
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("commands".into(), Value::I64(enquiries.len() as i64)),
            ("program_summary".into(), self.program_summary()),
            (
                "completion_basis".into(),
                Value::String("TeensyPulse mapped enquiry readback".into()),
            ),
        ])))
    }

    fn drain_replies(&mut self) -> Result<usize> {
        let bytes = self.serial.read_available()?;
        let frames = self.reply_codec.push(&bytes)?;
        let parsed = frames.len();
        for frame in frames {
            match protocol::decode_reply(&frame) {
                Ok(reply) => {
                    self.apply_hardware_reply(reply)?;
                    self.pending
                        .push_back(DriverEvent::Event(Event::Telemetry(TelemetryEvent {
                            device: self.pulse,
                            values: BTreeMap::from([
                                ("command".into(), Value::I64(reply.command as i64)),
                                ("value".into(), Value::I64(reply.value as i64)),
                            ]),
                        })));
                }
                Err(error) => {
                    self.pending
                        .push_back(DriverEvent::Event(Event::Fault(FaultEvent {
                            device: Some(self.pulse),
                            report: error.into(),
                        })))
                }
            }
        }
        Ok(parsed)
    }

    fn apply_hardware_reply(&mut self, reply: protocol::TeensyReply) -> Result<()> {
        let old_summary = self.program_summary();
        match reply.command {
            protocol::CMD_VERSION => {
                self.version = reply.value;
            }
            protocol::CMD_INTERVAL => {
                self.interval_us = reply.value.max(1);
                self.emit_property(self.pulse, "interval", time_interval_us(self.interval_us));
            }
            protocol::CMD_PULSE_DURATION => {
                self.duration_us = reply.value.max(1);
                self.emit_property(self.pulse, "duration", time_interval_us(self.duration_us));
            }
            protocol::CMD_WAIT_FOR_INPUT => {
                self.wait_for_input = reply.value != 0;
                self.emit_property(
                    self.pulse,
                    "wait_for_input",
                    Value::Bool(self.wait_for_input),
                );
            }
            protocol::CMD_NUMBER_OF_PULSES => {
                self.number_of_pulses = reply.value;
                self.emit_property(
                    self.pulse,
                    "number_of_pulses",
                    Value::I64(self.number_of_pulses as i64),
                );
            }
            protocol::CMD_START => {
                self.running = reply.value != 0;
                self.emit_property(self.pulse, "running", Value::Bool(self.running));
            }
            protocol::CMD_STOP => {
                self.running = false;
                self.emit_property(self.pulse, "running", Value::Bool(false));
            }
            protocol::CMD_ENQUIRE => {
                self.counted_pulses = reply.value;
                self.emit_property(
                    self.pulse,
                    "counted_pulses",
                    Value::I64(self.counted_pulses as i64),
                );
            }
            other => {
                return Err(Error::new(
                    ErrorCode::Transport,
                    format!("unknown TeensyPulse reply command {other}"),
                ))
            }
        }

        let new_summary = self.program_summary();
        if old_summary != new_summary {
            self.emit_property(self.pulse, "program_summary", new_summary);
        }
        Ok(())
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

    fn owns_device(&self, device: DeviceId) -> bool {
        device == self.hub || device == self.pulse
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
                    (
                        "property".into(),
                        Value::String(Self::public_key(&sequence.property).into()),
                    ),
                    ("values".into(), Value::List(sequence.values.clone())),
                ]))
            })
            .collect()
    }

    fn local_timing_sequence_refs<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| sequence.device == self.pulse)
            .collect()
    }

    fn has_explicit_sequence(&self, plan: &TimingPlan, property: &str) -> bool {
        plan.sequences.iter().any(|sequence| {
            sequence.device == self.pulse && Self::public_key(&sequence.property) == property
        })
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        let descriptor = self
            .descriptors_for()
            .into_iter()
            .find(|descriptor| descriptor.id == self.pulse)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "missing Teensy pulse device"))?;
        for sequence in self.local_timing_sequence_refs(plan) {
            if sequence.values.is_empty() {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "TeensyPulse timing sequence must contain at least one value",
                ));
            }
            let property = Self::public_key(&sequence.property);
            match property {
                "interval" | "duration" | "wait_for_input" | "number_of_pulses" | "running" => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        format!("TeensyPulse timing does not support {}", sequence.property),
                    ))
                }
            }
            let schema = descriptor
                .properties
                .iter()
                .find(|schema| schema.key == property)
                .ok_or_else(|| {
                    Error::new(ErrorCode::InvalidProperty, "unknown TeensyPulse property")
                })?;
            if !schema.sequenceable {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!(
                        "TeensyPulse property {} is not sequenceable",
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
                    "TeensyPulse timing sequence must contain at least one value",
                )
            })?
            .clone();
            let applied_value = self.write_property(sequence.device, property, &value)?;
            self.emit_property(sequence.device, property, applied_value.clone());
            if property == "running" && applied_value == Value::Bool(false) {
                self.emit_property(
                    self.pulse,
                    "counted_pulses",
                    Value::I64(self.counted_pulses as i64),
                );
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
            ("hub".into(), Value::I64(self.hub.0 .0 as i64)),
            ("pulse".into(), Value::I64(self.pulse.0 .0 as i64)),
            (
                "timed_pulse".into(),
                Value::Bool(self.has_timed_pulse(plan)),
            ),
            ("interval".into(), time_interval_us(self.interval_us)),
            ("duration".into(), time_interval_us(self.duration_us)),
            ("wait_for_input".into(), Value::Bool(self.wait_for_input)),
            (
                "number_of_pulses".into(),
                Value::I64(self.number_of_pulses as i64),
            ),
            ("running".into(), Value::Bool(self.running)),
            (
                "counted_pulses".into(),
                Value::I64(self.counted_pulses as i64),
            ),
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
        command: protocol::PulseCommand,
    ) -> PhysicalTransaction {
        PhysicalTransaction {
            resource: Some(self.resource),
            description: description.into(),
            payload: Value::Bytes(protocol::encode(&protocol::TeensyFrame::Set {
                command,
                parameter: 0,
            })),
        }
    }

    fn set_transaction(
        &self,
        description: &str,
        command: protocol::PulseCommand,
        parameter: u32,
    ) -> PhysicalTransaction {
        PhysicalTransaction {
            resource: Some(self.resource),
            description: description.into(),
            payload: Value::Bytes(protocol::encode(&protocol::TeensyFrame::Set {
                command,
                parameter,
            })),
        }
    }

    fn enquire_transaction(
        &self,
        description: &str,
        command: protocol::PulseCommand,
    ) -> PhysicalTransaction {
        PhysicalTransaction {
            resource: Some(self.resource),
            description: description.into(),
            payload: Value::Bytes(protocol::encode(&protocol::TeensyFrame::Enquire {
                command,
            })),
        }
    }

    fn pulse_program_transactions(
        &self,
        request: &CapabilityRequest,
    ) -> Result<Vec<(protocol::PulseCommand, u32)>> {
        let CapabilityRequest::PulseProgram(request) = request else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "TeensyPulse PulseProgram expects CapabilityRequest::PulseProgram",
            ));
        };
        let mut commands = Vec::new();
        if let Some(interval) = request.interval {
            commands.push((
                protocol::PulseCommand::IntervalUs,
                interval_us(interval)?.max(1),
            ));
        }
        if let Some(duration) = request.duration {
            commands.push((
                protocol::PulseCommand::PulseDurationUs,
                interval_us(duration)?.max(1),
            ));
        }
        if let Some(wait) = request.wait_for_input {
            commands.push((protocol::PulseCommand::WaitForInput, u32::from(wait)));
        }
        if let Some(count) = request.count {
            let count = count.min(u32::MAX as u64) as u32;
            commands.push((protocol::PulseCommand::NumberOfPulses, count));
        }
        if commands.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "TeensyPulse PulseProgram request did not include any program fields",
            ));
        }
        Ok(commands)
    }

    fn trigger_source_transactions(
        &self,
        request: &CapabilityRequest,
    ) -> Result<Vec<(protocol::PulseCommand, u32)>> {
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
                    "TeensyPulse TriggerSource expects None or CapabilityRequest::Trigger",
                ))
            }
        };
        Ok(match action {
            TriggerAction::Start => vec![(protocol::PulseCommand::Start, 0)],
            TriggerAction::Stop => vec![(protocol::PulseCommand::Stop, 0)],
            TriggerAction::Pulse => vec![
                (protocol::PulseCommand::Start, 0),
                (protocol::PulseCommand::Stop, 0),
            ],
        })
    }

    fn invoke_transactions(
        &self,
        device: DeviceId,
        kind: CapabilityKind,
        request: &CapabilityRequest,
    ) -> Result<Vec<(protocol::PulseCommand, u32)>> {
        if device != self.pulse {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown TeensyPulse device",
            ));
        }
        match kind {
            CapabilityKind::PulseProgram => self.pulse_program_transactions(request),
            CapabilityKind::TriggerSource => self.trigger_source_transactions(request),
            CapabilityKind::GenericCommand => {
                let CapabilityRequest::GenericCommand(request) = request else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "TeensyPulse GenericCommand expects GenericCommandRequest",
                    ));
                };
                self.validate_generic_command(request)?;
                Ok(Self::generic_enquiries_for(&request.command)?
                    .into_iter()
                    .map(|command| (command, 0))
                    .collect())
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported TeensyPulse invocation capability",
            )),
        }
    }

    fn apply_invoke(
        &mut self,
        device: DeviceId,
        kind: CapabilityKind,
        request: CapabilityRequest,
    ) -> Result<Value> {
        if device != self.pulse {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown TeensyPulse device",
            ));
        }
        match kind {
            CapabilityKind::PulseProgram => {
                let commands = self.pulse_program_transactions(&request)?;
                for (command, parameter) in &commands {
                    match command {
                        protocol::PulseCommand::IntervalUs => {
                            self.interval_us = (*parameter).max(1);
                            self.send_set(*command, self.interval_us)?;
                            self.emit_property(
                                self.pulse,
                                "interval",
                                time_interval_us(self.interval_us),
                            );
                        }
                        protocol::PulseCommand::PulseDurationUs => {
                            self.duration_us = (*parameter).max(1);
                            self.send_set(*command, self.duration_us)?;
                            self.emit_property(
                                self.pulse,
                                "duration",
                                time_interval_us(self.duration_us),
                            );
                        }
                        protocol::PulseCommand::WaitForInput => {
                            self.wait_for_input = *parameter != 0;
                            self.send_set(*command, u32::from(self.wait_for_input))?;
                            self.emit_property(
                                self.pulse,
                                "wait_for_input",
                                Value::Bool(self.wait_for_input),
                            );
                        }
                        protocol::PulseCommand::NumberOfPulses => {
                            self.number_of_pulses = *parameter;
                            self.send_set(*command, self.number_of_pulses)?;
                            self.emit_property(
                                self.pulse,
                                "number_of_pulses",
                                Value::I64(self.number_of_pulses as i64),
                            );
                        }
                        _ => self.send_set(*command, *parameter)?,
                    }
                }
                self.emit_property(self.pulse, "program_summary", self.program_summary());
                Ok(Value::Map(BTreeMap::from([
                    ("program_summary".into(), self.program_summary()),
                    ("commands".into(), Value::I64(commands.len() as i64)),
                ])))
            }
            CapabilityKind::TriggerSource => {
                let commands = self.trigger_source_transactions(&request)?;
                for (command, parameter) in &commands {
                    match command {
                        protocol::PulseCommand::Start => {
                            let value =
                                self.write_property(self.pulse, "running", &Value::Bool(true))?;
                            self.emit_property(self.pulse, "running", value);
                        }
                        protocol::PulseCommand::Stop => {
                            let value =
                                self.write_property(self.pulse, "running", &Value::Bool(false))?;
                            self.emit_property(self.pulse, "running", value);
                            self.emit_property(
                                self.pulse,
                                "counted_pulses",
                                Value::I64(self.counted_pulses as i64),
                            );
                        }
                        _ => self.send_set(*command, *parameter)?,
                    }
                }
                Ok(Value::Map(BTreeMap::from([
                    ("triggered".into(), Value::Bool(true)),
                    ("running".into(), Value::Bool(self.running)),
                    (
                        "counted_pulses".into(),
                        Value::I64(self.counted_pulses as i64),
                    ),
                    ("commands".into(), Value::I64(commands.len() as i64)),
                ])))
            }
            CapabilityKind::GenericCommand => {
                let CapabilityRequest::GenericCommand(request) = request else {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "TeensyPulse GenericCommand expects GenericCommandRequest",
                    ));
                };
                self.apply_generic_command(request)
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported TeensyPulse invocation capability",
            )),
        }
    }
}

impl Driver for TeensyPulseDriver {
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
            label: "teensy-pulse-serial".into(),
            kind: "serial.binary".into(),
            metadata: BTreeMap::from([
                ("baud_rate".into(), Value::I64(self.baud_rate as i64)),
                ("frame_len".into(), Value::I64(5)),
                ("connected".into(), Value::Bool(self.connected)),
                (
                    "serial_port".into(),
                    self.serial_port
                        .as_ref()
                        .map(|port| Value::String(port.clone()))
                        .unwrap_or(Value::Null),
                ),
                (
                    "enquire_opcode".into(),
                    Value::I64(protocol::CMD_ENQUIRE as i64),
                ),
            ]),
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.pulse {
            vec![
                capability(1, device, CapabilityKind::PulseProgram),
                capability(2, device, CapabilityKind::TriggerSource),
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
                        description: format!("teensy-pulse read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("teensy-pulse write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "teensy-pulse remultiplexed program state set".into(),
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
                            "unknown TeensyPulse capability",
                        ));
                    };
                    if !capability.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "TeensyPulse {:?} expects {:?}, got {:?}",
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
                                "TeensyPulse GenericCommand expects GenericCommandRequest",
                            ));
                        };
                        self.validate_generic_command(request)?;
                        for command in Self::generic_enquiries_for(&request.command)? {
                            physical_transactions.push(
                                self.enquire_transaction("teensy-pulse mapped enquiry", command),
                            );
                        }
                    } else {
                        for (command, parameter) in
                            self.invoke_transactions(*device, capability.kind, request)?
                        {
                            physical_transactions.push(self.set_transaction(
                                "teensy-pulse direct invocation",
                                command,
                                parameter,
                            ));
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
                    let public_key = Self::public_key(&key);
                    self.issue_read_command(device, public_key)?;
                    if device == self.pulse {
                        if self.connected {
                            let replies = if public_key == "program_summary" {
                                4
                            } else {
                                1
                            };
                            for _ in 0..replies {
                                self.read_reply_until()?;
                            }
                        } else {
                            let _ = self.drain_replies()?;
                        }
                    }
                    last = self.read_property(device, public_key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    let public_key = Self::public_key(&key);
                    last = self.write_property(device, public_key, &value)?;
                    self.emit_property(device, public_key, last.clone());
                    if device == self.pulse
                        && public_key == "running"
                        && value == Value::Bool(false)
                    {
                        self.emit_property(
                            device,
                            "counted_pulses",
                            Value::I64(self.counted_pulses as i64),
                        );
                    }
                }
                Command::ApplyStateSet(set) => {
                    let mut result = BTreeMap::new();
                    for write in set.writes {
                        let property = Self::public_key(&write.property);
                        let value = self.write_property(write.device, property, &write.value)?;
                        self.emit_property(write.device, property, value.clone());
                        if write.device == self.pulse
                            && property == "running"
                            && write.value == Value::Bool(false)
                        {
                            self.emit_property(
                                write.device,
                                "counted_pulses",
                                Value::I64(self.counted_pulses as i64),
                            );
                        }
                        result.insert(format!("{}:{}", (write.device.0).0, property), value);
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
                            "unknown TeensyPulse capability",
                        ));
                    };
                    if !capability.accepts_request(&request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "TeensyPulse {:?} expects {:?}, got {:?}",
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
        if let Err(error) = self.drain_replies() {
            self.pending
                .push_back(DriverEvent::Event(Event::Fault(FaultEvent {
                    device: Some(self.pulse),
                    report: error.into(),
                })));
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
                description: "teensy-pulse timing arm summary".into(),
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
        if self.has_timed_pulse(&armed.plan) && !self.has_explicit_sequence(&armed.plan, "running")
        {
            let value = self.write_property(self.pulse, "running", &Value::Bool(true))?;
            self.emit_property(self.pulse, "running", value);
            physical_transactions.push(
                self.timing_transaction("teensy-pulse timing start", protocol::PulseCommand::Start),
            );
        }
        physical_transactions.push(PhysicalTransaction {
            resource: Some(self.resource),
            description: "teensy-pulse timing start summary".into(),
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
        if self.has_timed_pulse(&armed.plan) && !self.has_explicit_sequence(&armed.plan, "running")
        {
            let value = self.write_property(self.pulse, "running", &Value::Bool(false))?;
            self.emit_property(self.pulse, "running", value);
            self.emit_property(
                self.pulse,
                "counted_pulses",
                Value::I64(self.counted_pulses as i64),
            );
            physical_transactions.push(
                self.timing_transaction("teensy-pulse timing stop", protocol::PulseCommand::Stop),
            );
        }
        physical_transactions.push(PhysicalTransaction {
            resource: Some(self.resource),
            description: "teensy-pulse timing stop summary".into(),
            payload: with_applied(self.timing_summary(&armed.plan, "stop"), applied),
        });
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions,
        })
    }
}

impl TeensyPulseDriver {
    fn issue_read_command(&mut self, device: DeviceId, key: &str) -> Result<()> {
        if device != self.pulse {
            return Ok(());
        }
        let key = Self::public_key(key);
        match key {
            "interval" => self.send_enquire(protocol::PulseCommand::IntervalUs)?,
            "duration" => self.send_enquire(protocol::PulseCommand::PulseDurationUs)?,
            "wait_for_input" => self.send_enquire(protocol::PulseCommand::WaitForInput)?,
            "number_of_pulses" => self.send_enquire(protocol::PulseCommand::NumberOfPulses)?,
            "running" | "counted_pulses" => self.send_enquire(protocol::PulseCommand::Start)?,
            "program_summary" => {
                self.send_enquire(protocol::PulseCommand::IntervalUs)?;
                self.send_enquire(protocol::PulseCommand::PulseDurationUs)?;
                self.send_enquire(protocol::PulseCommand::WaitForInput)?;
                self.send_enquire(protocol::PulseCommand::NumberOfPulses)?;
            }
            _ => {}
        }
        Ok(())
    }
}

fn capability(id: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    CapabilityDescriptor::new(CapabilityId(id), device, kind, ValueType::Map)
}

fn time_interval_us(us: u32) -> Value {
    Value::TimeInterval(TimeInterval::from_microseconds(us as f64))
}

fn time_us(value: &Value) -> Result<u32> {
    let Value::TimeInterval(interval) = value else {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "expected typed time interval",
        ));
    };
    interval_us(*interval)
}

fn interval_us(interval: TimeInterval) -> Result<u32> {
    let us = interval.microseconds().round();
    if !us.is_finite() || us < 0.0 || us > u32::MAX as f64 {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "time interval is outside TeensyPulse u32 microsecond range",
        ));
    }
    Ok(us as u32)
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

fn time_us_prop(device: &DeviceConfig, key: &str) -> Option<u32> {
    match device.properties.get(key) {
        Some(Value::TimeInterval(value)) => {
            let us = value.microseconds().round();
            if us.is_finite() && us >= 0.0 && us <= u32::MAX as f64 {
                Some(us as u32)
            } else {
                None
            }
        }
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

fn clamp_u32(value: i64, min: u32) -> u32 {
    value.clamp(min as i64, u32::MAX as i64) as u32
}
