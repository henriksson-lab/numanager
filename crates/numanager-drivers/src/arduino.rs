use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::{FixedBinaryCodec, ScriptedSerial, SerialIo, SerialLineCodec};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const BAUD: u32 = 57_600;
    pub const CMD_CONTROLLER_ID: u8 = 30;
    pub const CMD_VERSION: u8 = 31;
    pub const CMD_PATTERN_COUNT: u8 = 32;
    pub const CMD_UPLOAD_SEQUENCE_FAST: u8 = 33;
    pub const CMD_DAC_CHANNEL_COUNT: u8 = 34;
    pub const CMD_DIGITAL_PIN_COUNT: u8 = 35;
    pub const CMD_SET_DIGITAL: u8 = 1;
    pub const CMD_GET_DIGITAL: u8 = 2;
    pub const CMD_SET_DAC: u8 = 3;
    pub const CMD_SET_SEQUENCE_PATTERN: u8 = 5;
    pub const CMD_SET_SEQUENCE_LENGTH: u8 = 6;
    pub const CMD_START_SEQUENCE: u8 = 8;
    pub const CMD_STOP_SEQUENCE: u8 = 9;
    pub const CMD_SET_TIMED_PATTERN_DELAY: u8 = 10;
    pub const CMD_SET_TIMED_PATTERN_REPEAT: u8 = 11;
    pub const CMD_START_TIMED_OUTPUT: u8 = 12;
    pub const CMD_START_BLANKING: u8 = 20;
    pub const CMD_STOP_BLANKING: u8 = 21;
    pub const CMD_SET_BLANK_ON: u8 = 22;
    pub const CMD_READ_DIGITAL_INPUTS: u8 = 40;
    pub const CMD_READ_ANALOG_INPUT: u8 = 41;
    pub const CMD_SET_INPUT_PULLUP: u8 = 42;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum ArduinoProbeCommand {
        ControllerId,
        Version,
        PatternCount,
        DacChannelCount,
        DigitalPinCount,
    }

    impl ArduinoProbeCommand {
        pub fn opcode(&self) -> u8 {
            match self {
                ArduinoProbeCommand::ControllerId => CMD_CONTROLLER_ID,
                ArduinoProbeCommand::Version => CMD_VERSION,
                ArduinoProbeCommand::PatternCount => CMD_PATTERN_COUNT,
                ArduinoProbeCommand::DacChannelCount => CMD_DAC_CHANNEL_COUNT,
                ArduinoProbeCommand::DigitalPinCount => CMD_DIGITAL_PIN_COUNT,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ArduinoProbe {
        pub controller_id: String,
        pub version: u16,
        pub extended_version: i64,
        pub pattern_count: u16,
        pub dac_channels: u8,
        pub digital_pins: u8,
    }

    impl ArduinoProbe {
        pub fn simulated() -> Self {
            Self {
                controller_id: "MM-Ard numanager-sim".into(),
                version: 5,
                extended_version: 0,
                pattern_count: 64,
                dac_channels: 2,
                digital_pins: 8,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum ArduinoCommand {
        SetDigitalMask { mask: u8 },
        SetDac { channel: u8, value: u16 },
        SetSequencePattern { index: u8, mask: u8 },
        SetSequenceLength { len: u8 },
        UploadSequenceFast { masks: Vec<u8> },
        StartSequence,
        StopSequence,
        SetTimedPatternDelay { index: u8, delay_ms: u16 },
        SetTimedPatternRepeat { count: u8 },
        StartTimedOutput,
        StartBlanking,
        StopBlanking,
        SetBlankOn { high: bool },
        ReadDigitalInputs,
        ReadAdc { channel: u8 },
        SetInputPullUp { pin: u8, high: bool },
    }

    pub fn encode_probe(command: ArduinoProbeCommand) -> Vec<u8> {
        vec![command.opcode()]
    }

    pub fn encode_command(command: &ArduinoCommand) -> Vec<u8> {
        match command {
            ArduinoCommand::SetDigitalMask { mask } => vec![CMD_SET_DIGITAL, *mask],
            ArduinoCommand::SetDac { channel, value } => {
                vec![CMD_SET_DAC, *channel, (value >> 8) as u8, *value as u8]
            }
            ArduinoCommand::SetSequencePattern { index, mask } => {
                vec![CMD_SET_SEQUENCE_PATTERN, *index, *mask]
            }
            ArduinoCommand::SetSequenceLength { len } => vec![CMD_SET_SEQUENCE_LENGTH, *len],
            ArduinoCommand::UploadSequenceFast { masks } => {
                let len = masks.len().min(u16::MAX as usize) as u16;
                let mut bytes = vec![CMD_UPLOAD_SEQUENCE_FAST, (len >> 8) as u8, len as u8];
                bytes.extend_from_slice(&masks[..len as usize]);
                bytes
            }
            ArduinoCommand::StartSequence => vec![CMD_START_SEQUENCE],
            ArduinoCommand::StopSequence => vec![CMD_STOP_SEQUENCE],
            ArduinoCommand::SetTimedPatternDelay { index, delay_ms } => vec![
                CMD_SET_TIMED_PATTERN_DELAY,
                *index,
                (delay_ms >> 8) as u8,
                *delay_ms as u8,
            ],
            ArduinoCommand::SetTimedPatternRepeat { count } => {
                vec![CMD_SET_TIMED_PATTERN_REPEAT, *count]
            }
            ArduinoCommand::StartTimedOutput => vec![CMD_START_TIMED_OUTPUT],
            ArduinoCommand::StartBlanking => vec![CMD_START_BLANKING],
            ArduinoCommand::StopBlanking => vec![CMD_STOP_BLANKING],
            ArduinoCommand::SetBlankOn { high } => vec![CMD_SET_BLANK_ON, u8::from(!*high)],
            ArduinoCommand::ReadDigitalInputs => vec![CMD_READ_DIGITAL_INPUTS],
            ArduinoCommand::ReadAdc { channel } => vec![CMD_READ_ANALOG_INPUT, *channel],
            ArduinoCommand::SetInputPullUp { pin, high } => {
                vec![CMD_SET_INPUT_PULLUP, *pin, u8::from(*high)]
            }
        }
    }

    pub fn decode_count_reply(expected_opcode: u8, bytes: &[u8]) -> Result<u16> {
        if bytes.len() != 3 || bytes[0] != expected_opcode {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("invalid Arduino count reply for opcode {expected_opcode}"),
            ));
        }
        Ok(u16::from_be_bytes([bytes[1], bytes[2]]))
    }

    pub fn decode_u8_reply(expected_opcode: u8, bytes: &[u8]) -> Result<u8> {
        if bytes.len() != 2 || bytes[0] != expected_opcode {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("invalid Arduino u8 reply for opcode {expected_opcode}"),
            ));
        }
        Ok(bytes[1])
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ArduinoInputSnapshot {
        pub digital_mask: u8,
        pub adc_channel: u8,
        pub adc_count: u16,
    }

    impl ArduinoInputSnapshot {
        pub fn decode(digital_reply: &[u8], adc_reply: &[u8], adc_channel: u8) -> Result<Self> {
            Ok(Self {
                digital_mask: decode_u8_reply(CMD_READ_DIGITAL_INPUTS, digital_reply)?,
                adc_channel,
                adc_count: decode_count_reply(CMD_READ_ANALOG_INPUT, adc_reply)?,
            })
        }

        pub fn value(&self) -> Value {
            Value::Map(BTreeMap::from([
                (
                    "digital_inputs".into(),
                    Value::I64(self.digital_mask as i64),
                ),
                ("adc_channel".into(), Value::I64(self.adc_channel as i64)),
                ("adc_count".into(), Value::I64(self.adc_count as i64)),
            ]))
        }
    }
}

pub struct ArduinoDiscovery {
    next_id: DriverId,
    simulated: bool,
    configured: Vec<ArduinoConfiguredProbe>,
}

#[derive(Debug, Clone)]
pub struct ArduinoConfiguredProbe {
    label: String,
    probe: protocol::ArduinoProbe,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connect_real_transport: bool,
}

impl ArduinoDiscovery {
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
            .filter(|device| matches!(device.driver.as_str(), "arduino" | "mm-arduino"))
            .map(ArduinoConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            next_id,
            simulated: false,
            configured,
        })
    }
}

impl DriverDiscovery for ArduinoDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        if self.simulated {
            return Ok(vec![DriverCandidate::from_driver(
                "Simulated Micro-Manager Arduino firmware",
                Box::new(ArduinoDriver::simulated(self.next_id)),
            )]);
        }
        self.configured
            .iter()
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let driver: Box<dyn Driver> = if configured.connect_real_transport {
                    Box::new(ArduinoDriver::serial(id, configured.clone())?)
                } else {
                    Box::new(ArduinoDriver::configured(id, configured.clone()))
                };
                Ok(DriverCandidate::from_driver(
                    configured.label.clone(),
                    driver,
                ))
            })
            .collect()
    }
}

impl ArduinoConfiguredProbe {
    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = protocol::ArduinoProbe::simulated();
        probe.controller_id = string_prop(device, "controller_id").unwrap_or(probe.controller_id);
        probe.version = u16_prop(device, "version").unwrap_or(probe.version);
        probe.extended_version =
            i64_prop(device, "extended_version").unwrap_or(probe.extended_version);
        probe.pattern_count = u16_prop(device, "pattern_count").unwrap_or(probe.pattern_count);
        probe.dac_channels = u8_prop(device, "dac_channels").unwrap_or(probe.dac_channels);
        probe.digital_pins = u8_prop(device, "digital_pins").unwrap_or(probe.digital_pins);
        Ok(Self {
            label: if device.label.is_empty() {
                "Configured Micro-Manager Arduino firmware".into()
            } else {
                device.label.clone()
            },
            probe,
            serial_port: string_prop(device, "serial_port"),
            baud_rate: u32_prop(device, "baud_rate").unwrap_or(protocol::BAUD),
            serial_timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(500),
            connect_real_transport: bool_prop(device, "connect").unwrap_or(false),
        })
    }
}

pub struct ArduinoDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    digital: DeviceId,
    shutter: DeviceId,
    adc: DeviceId,
    dac: DeviceId,
    probe: protocol::ArduinoProbe,
    digital_mask: u64,
    digital_input_mask: u8,
    input_pullup_mask: u8,
    shutter_open: bool,
    inverted_logic: bool,
    sequence_enabled: bool,
    sequence: Vec<u8>,
    timed_delays_ms: Vec<u16>,
    timed_repeat: u8,
    timed_output_running: bool,
    blanking_enabled: bool,
    blank_on_high: bool,
    dac_values: Vec<u16>,
    adc_values: Vec<u16>,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    line_codec: SerialLineCodec,
    count_codec: FixedBinaryCodec,
}

impl ArduinoDriver {
    pub fn configured(id: DriverId, configured: ArduinoConfiguredProbe) -> Self {
        Self::new(
            id,
            configured.probe,
            Box::new(ScriptedSerial::new()),
            configured.serial_port,
            configured.baud_rate,
            configured.serial_timeout_ms,
            false,
        )
    }

    pub fn simulated(id: DriverId) -> Self {
        let probe = protocol::ArduinoProbe::simulated();
        let mut serial = ScriptedSerial::new();
        serial.push_read(b"MM-Ard numanager-sim\r\n".to_vec());
        Self::new(
            id,
            probe,
            Box::new(serial),
            None,
            protocol::BAUD,
            500,
            false,
        )
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: ArduinoConfiguredProbe) -> Result<Self> {
        let port_name = configured.serial_port.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Arduino real serial config requires serial_port",
            )
        })?;
        let serial = Box::new(numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(port_name, configured.baud_rate)
                .timeout(Duration::from_millis(configured.serial_timeout_ms)),
        )?);
        let mut driver = Self::new(
            id,
            configured.probe,
            serial,
            configured.serial_port,
            configured.baud_rate,
            configured.serial_timeout_ms,
            true,
        );
        driver.refresh_startup_probe(configured.serial_timeout_ms)?;
        Ok(driver)
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, configured: ArduinoConfiguredProbe) -> Result<Self> {
        let _ = configured.serial_port.as_ref();
        let _ = configured.baud_rate;
        let _ = configured.serial_timeout_ms;
        Err(Error::new(
            ErrorCode::Unsupported,
            "Arduino real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    fn new(
        id: DriverId,
        probe: protocol::ArduinoProbe,
        serial: Box<dyn SerialIo>,
        serial_port: Option<String>,
        baud_rate: u32,
        serial_timeout_ms: u64,
        connected: bool,
    ) -> Self {
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 1)),
            hub: DeviceId(NodeId(id.0 * 1000 + 10)),
            digital: DeviceId(NodeId(id.0 * 1000 + 11)),
            shutter: DeviceId(NodeId(id.0 * 1000 + 12)),
            adc: DeviceId(NodeId(id.0 * 1000 + 13)),
            dac: DeviceId(NodeId(id.0 * 1000 + 14)),
            dac_values: vec![0; probe.dac_channels as usize],
            adc_values: vec![2048; 6],
            probe,
            digital_mask: 0,
            digital_input_mask: 0,
            input_pullup_mask: 0,
            shutter_open: false,
            inverted_logic: false,
            sequence_enabled: false,
            sequence: Vec::new(),
            timed_delays_ms: Vec::new(),
            timed_repeat: 1,
            timed_output_running: false,
            blanking_enabled: false,
            blank_on_high: false,
            serial_port,
            baud_rate,
            serial_timeout_ms,
            connected,
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            line_codec: SerialLineCodec::new(
                numanager_core::serial::LineEnding::CrLf,
                numanager_core::serial::LineEnding::CrLf,
            ),
            count_codec: FixedBinaryCodec::new(3),
        }
    }

    #[cfg(feature = "os-serial")]
    fn refresh_startup_probe(&mut self, timeout_ms: u64) -> Result<()> {
        self.serial.write(&protocol::encode_probe(
            protocol::ArduinoProbeCommand::ControllerId,
        ))?;
        let controller_id = self.read_probe_line(timeout_ms)?;

        let version = self.read_probe_count(protocol::ArduinoProbeCommand::Version, timeout_ms)?;
        let pattern_count =
            self.read_probe_count(protocol::ArduinoProbeCommand::PatternCount, timeout_ms)?;
        let dac_channels =
            self.read_probe_u8(protocol::ArduinoProbeCommand::DacChannelCount, timeout_ms)?;
        let digital_pins =
            self.read_probe_u8(protocol::ArduinoProbeCommand::DigitalPinCount, timeout_ms)?;

        self.probe.controller_id = controller_id;
        self.probe.version = version;
        self.probe.pattern_count = pattern_count;
        self.probe.dac_channels = dac_channels;
        self.probe.digital_pins = digital_pins;
        self.dac_values.resize(dac_channels as usize, 0);
        Ok(())
    }

    #[cfg(feature = "os-serial")]
    fn read_probe_line(&mut self, timeout_ms: u64) -> Result<String> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            let bytes = self.serial.read_available()?;
            for line in self.line_codec.push(&bytes) {
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
            "Arduino did not return a controller ID reply",
        ))
    }

    #[cfg(feature = "os-serial")]
    fn read_probe_count(
        &mut self,
        command: protocol::ArduinoProbeCommand,
        timeout_ms: u64,
    ) -> Result<u16> {
        let expected_opcode = command.opcode();
        self.serial.write(&protocol::encode_probe(command))?;
        let frame = self.read_probe_frame(3, timeout_ms)?;
        protocol::decode_count_reply(expected_opcode, &frame)
    }

    #[cfg(feature = "os-serial")]
    fn read_probe_u8(
        &mut self,
        command: protocol::ArduinoProbeCommand,
        timeout_ms: u64,
    ) -> Result<u8> {
        let expected_opcode = command.opcode();
        self.serial.write(&protocol::encode_probe(command))?;
        let frame = self.read_probe_frame(2, timeout_ms)?;
        protocol::decode_u8_reply(expected_opcode, &frame)
    }

    fn read_probe_frame(&mut self, len: usize, timeout_ms: u64) -> Result<Vec<u8>> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        let mut bytes = Vec::new();
        loop {
            bytes.extend_from_slice(&self.serial.read_available()?);
            if bytes.len() >= len {
                return Ok(bytes.drain(..len).collect());
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        Err(Error::new(
            ErrorCode::Transport,
            "Arduino did not return a complete startup probe reply",
        ))
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
                label: "arduino-hub".into(),
                vendor: Some("Micro-Manager".into()),
                model: Some("Arduino firmware".into()),
                serial: None,
                kinds: vec!["hub".into(), "microcontroller".into()],
                properties: vec![
                    property("logic", "Logic", ValueType::String, None, true, None)
                        .with_enum(&["Normal", "Inverted"]),
                    property("version", "Version", ValueType::I64, None, false, None),
                    property(
                        "extended_version",
                        "Extended version",
                        ValueType::I64,
                        None,
                        false,
                        None,
                    ),
                ],
                metadata: BTreeMap::from([
                    (
                        "controller_id".into(),
                        Value::String(self.probe.controller_id.clone()),
                    ),
                    (
                        "firmware_version".into(),
                        Value::I64(self.probe.version as i64),
                    ),
                    (
                        "extended_version".into(),
                        Value::I64(self.probe.extended_version),
                    ),
                    (
                        "pattern_count".into(),
                        Value::I64(self.probe.pattern_count as i64),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.digital,
                driver: self.id,
                label: "arduino-digital-out".into(),
                vendor: Some("Micro-Manager".into()),
                model: Some("Arduino digital pins".into()),
                serial: None,
                kinds: vec!["digital.io".into(), "trigger.source".into()],
                properties: vec![
                    sequenceable_property(
                        "mask",
                        "Digital output mask",
                        ValueType::I64,
                        None,
                        true,
                        Some(Range {
                            min: Value::I64(0),
                            max: Value::I64((1i64 << self.probe.digital_pins.min(8)) - 1),
                        }),
                    ),
                    sequenceable_property(
                        "sequence",
                        "Sequence",
                        ValueType::String,
                        None,
                        true,
                        None,
                    )
                    .with_enum(&["On", "Off"]),
                    property(
                        "sequence_values",
                        "Sequence values",
                        ValueType::List,
                        None,
                        true,
                        None,
                    ),
                    property(
                        "timed_delays",
                        "Timed pattern delays",
                        ValueType::List,
                        Some("ms"),
                        true,
                        None,
                    ),
                    property(
                        "timed_repeat",
                        "Timed pattern repeat",
                        ValueType::I64,
                        None,
                        true,
                        Some(Range {
                            min: Value::I64(0),
                            max: Value::I64(255),
                        }),
                    ),
                    sequenceable_property(
                        "timed_output",
                        "Timed output",
                        ValueType::String,
                        None,
                        true,
                        None,
                    )
                    .with_enum(&["On", "Off"]),
                    property(
                        "blanking",
                        "Blanking mode",
                        ValueType::String,
                        None,
                        true,
                        None,
                    )
                    .with_enum(&["On", "Off"]),
                    property("blank_on", "Blank on", ValueType::String, None, true, None)
                        .with_enum(&["Low", "High"]),
                ],
                metadata: BTreeMap::from([
                    (
                        "pin_count".into(),
                        Value::I64(self.probe.digital_pins as i64),
                    ),
                    (
                        "timed_pattern_capacity".into(),
                        Value::I64(self.probe.pattern_count as i64),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.shutter,
                driver: self.id,
                label: "arduino-shutter".into(),
                vendor: Some("Micro-Manager".into()),
                model: Some("Arduino shutter".into()),
                serial: None,
                kinds: vec!["shutter".into(), "trigger.sink".into()],
                properties: vec![sequenceable_property(
                    "open",
                    "Open",
                    ValueType::Bool,
                    None,
                    true,
                    None,
                )],
                metadata: BTreeMap::new(),
            },
            DeviceDescriptor {
                id: self.adc,
                driver: self.id,
                label: "arduino-adc".into(),
                vendor: Some("Micro-Manager".into()),
                model: Some("Arduino analog input".into()),
                serial: None,
                kinds: vec!["analog.input".into()],
                properties: adc_properties(self.adc_values.len(), self.probe.digital_pins),
                metadata: BTreeMap::new(),
            },
            DeviceDescriptor {
                id: self.dac,
                driver: self.id,
                label: "arduino-dac".into(),
                vendor: Some("Micro-Manager".into()),
                model: Some("Arduino DAC".into()),
                serial: None,
                kinds: vec!["analog.output".into()],
                properties: dac_properties(self.dac_values.len()),
                metadata: BTreeMap::from([(
                    "channel_count".into(),
                    Value::I64(self.probe.dac_channels as i64),
                )]),
            },
        ]
    }

    fn public_key(key: &str) -> &str {
        match key {
            "timed_delays_ms" => "timed_delays",
            _ => key,
        }
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        let key = Self::public_key(key);
        match (device, key) {
            (device, "logic") if device == self.hub => Ok(Value::String(
                if self.inverted_logic {
                    "Inverted"
                } else {
                    "Normal"
                }
                .into(),
            )),
            (device, "version") if device == self.hub => Ok(Value::I64(self.probe.version as i64)),
            (device, "extended_version") if device == self.hub => {
                Ok(Value::I64(self.probe.extended_version))
            }
            (device, "mask") if device == self.digital => Ok(Value::I64(self.digital_mask as i64)),
            (device, "sequence") if device == self.digital => Ok(Value::String(
                if self.sequence_enabled { "On" } else { "Off" }.into(),
            )),
            (device, "sequence_values") if device == self.digital => Ok(Value::List(
                self.sequence
                    .iter()
                    .map(|mask| Value::I64(*mask as i64))
                    .collect(),
            )),
            (device, "timed_delays") if device == self.digital => Ok(Value::List(
                self.timed_delays_ms
                    .iter()
                    .map(|delay| time_interval_ms(*delay))
                    .collect(),
            )),
            (device, "timed_repeat") if device == self.digital => {
                Ok(Value::I64(self.timed_repeat as i64))
            }
            (device, "timed_output") if device == self.digital => Ok(Value::String(
                if self.timed_output_running {
                    "On"
                } else {
                    "Off"
                }
                .into(),
            )),
            (device, "blanking") if device == self.digital => Ok(Value::String(
                if self.blanking_enabled { "On" } else { "Off" }.into(),
            )),
            (device, "blank_on") if device == self.digital => Ok(Value::String(
                if self.blank_on_high { "High" } else { "Low" }.into(),
            )),
            (device, "open") if device == self.shutter => Ok(Value::Bool(self.shutter_open)),
            (device, "digital_inputs") if device == self.adc => {
                Ok(Value::I64(self.digital_input_mask as i64))
            }
            (device, "input_pullups") if device == self.adc => {
                Ok(Value::I64(self.input_pullup_mask as i64))
            }
            (device, "input_summary") if device == self.adc => Ok(self.input_summary()),
            (device, key) if device == self.adc && key.starts_with("channel_") => {
                let channel = parse_channel_key(key)?;
                Ok(Value::I64(
                    self.adc_values.get(channel).copied().unwrap_or(0) as i64,
                ))
            }
            (device, key) if device == self.dac && key.starts_with("channel_") => {
                let channel = parse_channel_key(key)?;
                Ok(Value::I64(
                    self.dac_values.get(channel).copied().unwrap_or(0) as i64,
                ))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Arduino property {key}"),
            )),
        }
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: &Value) -> Result<Value> {
        let key = Self::public_key(key);
        match (device, key, value) {
            (device, "logic", Value::String(logic)) if device == self.hub => {
                self.inverted_logic = match logic.as_str() {
                    "Normal" => false,
                    "Inverted" => true,
                    _ => {
                        return Err(Error::new(
                            ErrorCode::InvalidProperty,
                            "logic must be Normal or Inverted",
                        ))
                    }
                };
                if self.shutter_open {
                    self.write_digital_output(self.digital_mask)?;
                }
                Ok(Value::String(logic.clone()))
            }
            (device, "mask", Value::I64(mask)) if device == self.digital => {
                let max = (1u64 << self.probe.digital_pins.min(8)) - 1;
                self.digital_mask = (*mask).clamp(0, max as i64) as u64;
                if self.shutter_open {
                    self.write_digital_output(self.digital_mask)?;
                }
                Ok(Value::I64(self.digital_mask as i64))
            }
            (device, "sequence", Value::String(state)) if device == self.digital => {
                match state.as_str() {
                    "On" => {
                        self.serial.write(&protocol::encode_command(
                            &protocol::ArduinoCommand::StartSequence,
                        ))?;
                        self.sequence_enabled = true;
                    }
                    "Off" => {
                        self.serial.write(&protocol::encode_command(
                            &protocol::ArduinoCommand::StopSequence,
                        ))?;
                        self.sequence_enabled = false;
                    }
                    _ => {
                        return Err(Error::new(
                            ErrorCode::InvalidProperty,
                            "sequence must be On or Off",
                        ))
                    }
                }
                Ok(Value::String(state.clone()))
            }
            (device, "sequence_values", Value::List(values)) if device == self.digital => {
                if values.len() > self.probe.pattern_count as usize {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "sequence exceeds Arduino pattern capacity",
                    ));
                }
                let max = digital_max_mask(self.probe.digital_pins);
                let masks = values
                    .iter()
                    .map(|value| match value {
                        Value::I64(mask) => Ok((*mask).clamp(0, max as i64) as u8),
                        _ => Err(Error::new(
                            ErrorCode::InvalidProperty,
                            "sequence values must be integer masks",
                        )),
                    })
                    .collect::<Result<Vec<_>>>()?;
                self.serial.write(&protocol::encode_command(
                    &protocol::ArduinoCommand::UploadSequenceFast {
                        masks: masks.clone(),
                    },
                ))?;
                self.sequence = masks.clone();
                Ok(Value::List(
                    masks
                        .into_iter()
                        .map(|mask| Value::I64(mask as i64))
                        .collect(),
                ))
            }
            (device, "timed_delays", Value::List(values)) if device == self.digital => {
                if values.len() > self.probe.pattern_count as usize {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "timed delays exceed Arduino pattern capacity",
                    ));
                }
                let delays = values
                    .iter()
                    .map(timed_delay_ms)
                    .collect::<Result<Vec<_>>>()?;
                for (index, delay_ms) in delays.iter().enumerate() {
                    self.serial.write(&protocol::encode_command(
                        &protocol::ArduinoCommand::SetTimedPatternDelay {
                            index: index as u8,
                            delay_ms: *delay_ms,
                        },
                    ))?;
                }
                self.timed_delays_ms = delays.clone();
                Ok(Value::List(
                    delays.into_iter().map(time_interval_ms).collect(),
                ))
            }
            (device, "timed_repeat", Value::I64(count)) if device == self.digital => {
                self.timed_repeat = (*count).clamp(0, 255) as u8;
                self.serial.write(&protocol::encode_command(
                    &protocol::ArduinoCommand::SetTimedPatternRepeat {
                        count: self.timed_repeat,
                    },
                ))?;
                Ok(Value::I64(self.timed_repeat as i64))
            }
            (device, "timed_output", Value::String(state)) if device == self.digital => {
                match state.as_str() {
                    "On" => {
                        self.serial.write(&protocol::encode_command(
                            &protocol::ArduinoCommand::StartTimedOutput,
                        ))?;
                        self.timed_output_running = true;
                    }
                    "Off" => {
                        self.timed_output_running = false;
                    }
                    _ => {
                        return Err(Error::new(
                            ErrorCode::InvalidProperty,
                            "timed_output must be On or Off",
                        ))
                    }
                }
                Ok(Value::String(state.clone()))
            }
            (device, "blanking", Value::String(state)) if device == self.digital => {
                match state.as_str() {
                    "On" => {
                        self.serial.write(&protocol::encode_command(
                            &protocol::ArduinoCommand::StartBlanking,
                        ))?;
                        self.blanking_enabled = true;
                    }
                    "Off" => {
                        self.serial.write(&protocol::encode_command(
                            &protocol::ArduinoCommand::StopBlanking,
                        ))?;
                        self.blanking_enabled = false;
                    }
                    _ => {
                        return Err(Error::new(
                            ErrorCode::InvalidProperty,
                            "blanking must be On or Off",
                        ))
                    }
                }
                Ok(Value::String(state.clone()))
            }
            (device, "blank_on", Value::String(edge)) if device == self.digital => {
                self.blank_on_high = match edge.as_str() {
                    "High" => true,
                    "Low" => false,
                    _ => {
                        return Err(Error::new(
                            ErrorCode::InvalidProperty,
                            "blank_on must be High or Low",
                        ))
                    }
                };
                self.serial.write(&protocol::encode_command(
                    &protocol::ArduinoCommand::SetBlankOn {
                        high: self.blank_on_high,
                    },
                ))?;
                Ok(Value::String(edge.clone()))
            }
            (device, "open", Value::Bool(open)) if device == self.shutter => {
                self.shutter_open = *open;
                let mask = if *open { self.digital_mask } else { 0 };
                self.write_digital_output(mask)?;
                Ok(Value::Bool(*open))
            }
            (device, "input_pullups", Value::I64(mask)) if device == self.adc => {
                let mask = (*mask).clamp(0, digital_max_mask(self.probe.digital_pins) as i64) as u8;
                for pin in 0..self.probe.digital_pins.min(8) {
                    let high = (mask & (1 << pin)) != 0;
                    self.serial.write(&protocol::encode_command(
                        &protocol::ArduinoCommand::SetInputPullUp { pin, high },
                    ))?;
                }
                self.input_pullup_mask = mask;
                Ok(Value::I64(mask as i64))
            }
            (device, key, Value::I64(count))
                if device == self.dac && key.starts_with("channel_") =>
            {
                let channel = parse_channel_key(key)?;
                let count = (*count).clamp(0, 4095) as u16;
                let Some(slot) = self.dac_values.get_mut(channel) else {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "unknown DAC channel",
                    ));
                };
                *slot = count;
                self.serial.write(&protocol::encode_command(
                    &protocol::ArduinoCommand::SetDac {
                        channel: channel as u8,
                        value: count,
                    },
                ))?;
                Ok(Value::I64(count as i64))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid Arduino write {key}"),
            )),
        }
    }

    fn write_digital_output(&mut self, mask: u64) -> Result<()> {
        let max = digital_max_mask(self.probe.digital_pins) as u64;
        let mut value = (mask & max) as u8;
        if self.inverted_logic {
            value = !value & digital_max_mask(self.probe.digital_pins);
        }
        self.serial.write(&protocol::encode_command(
            &protocol::ArduinoCommand::SetDigitalMask { mask: value },
        ))
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

    fn input_summary(&self) -> Value {
        let channel = 0usize;
        let mut value = protocol::ArduinoInputSnapshot {
            digital_mask: self.digital_input_mask,
            adc_channel: channel as u8,
            adc_count: self.adc_values.get(channel).copied().unwrap_or(0),
        }
        .value();
        if let Value::Map(map) = &mut value {
            map.insert(
                "input_pullups".into(),
                Value::I64(self.input_pullup_mask as i64),
            );
        }
        value
    }

    fn owns_device(&self, device: DeviceId) -> bool {
        device == self.hub
            || device == self.digital
            || device == self.shutter
            || device == self.adc
            || device == self.dac
    }

    fn has_timed_shutter(&self, plan: &TimingPlan) -> bool {
        plan.participants.contains(&self.shutter)
            || plan
                .routes
                .iter()
                .any(|route| route.from == self.shutter || route.to == self.shutter)
            || plan
                .sequences
                .iter()
                .any(|sequence| sequence.device == self.shutter)
    }

    fn has_timed_digital_sequence(&self, plan: &TimingPlan) -> bool {
        plan.participants.contains(&self.digital)
            || plan
                .routes
                .iter()
                .any(|route| route.from == self.digital || route.to == self.digital)
            || plan
                .sequences
                .iter()
                .any(|sequence| sequence.device == self.digital)
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
            .filter(|sequence| self.owns_device(sequence.device))
            .collect()
    }

    fn has_explicit_sequence(&self, plan: &TimingPlan, device: DeviceId, property: &str) -> bool {
        plan.sequences.iter().any(|sequence| {
            sequence.device == device && Self::public_key(&sequence.property) == property
        })
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        let descriptors = self.descriptors_for();
        for sequence in self.local_timing_sequence_refs(plan) {
            if sequence.values.is_empty() {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "Arduino timing sequence must contain at least one value",
                ));
            }
            let property = Self::public_key(&sequence.property);
            match (sequence.device, property) {
                (device, "mask" | "sequence" | "timed_output") if device == self.digital => {}
                (device, "open") if device == self.shutter => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        format!(
                            "Arduino timing does not support {} on {:?}",
                            sequence.property, sequence.device
                        ),
                    ))
                }
            }
            let descriptor = descriptors
                .iter()
                .find(|descriptor| descriptor.id == sequence.device)
                .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown Arduino device"))?;
            let schema = descriptor
                .properties
                .iter()
                .find(|schema| schema.key == property)
                .ok_or_else(|| {
                    Error::new(ErrorCode::InvalidProperty, "unknown Arduino property")
                })?;
            if !schema.sequenceable {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Arduino property {} is not sequenceable", sequence.property),
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
                    "Arduino timing sequence must contain at least one value",
                )
            })?
            .clone();
            let applied_value = self.write_property(sequence.device, property, &value)?;
            self.emit_property(sequence.device, property, applied_value.clone());
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
            ("digital".into(), Value::I64(self.digital.0 .0 as i64)),
            ("shutter".into(), Value::I64(self.shutter.0 .0 as i64)),
            (
                "timed_digital_sequence".into(),
                Value::Bool(self.has_timed_digital_sequence(plan)),
            ),
            (
                "timed_shutter".into(),
                Value::Bool(self.has_timed_shutter(plan)),
            ),
            ("digital_mask".into(), Value::I64(self.digital_mask as i64)),
            ("shutter_open".into(), Value::Bool(self.shutter_open)),
            (
                "sequence_enabled".into(),
                Value::Bool(self.sequence_enabled),
            ),
            (
                "sequence_values".into(),
                Value::List(
                    self.sequence
                        .iter()
                        .map(|mask| Value::I64(*mask as i64))
                        .collect(),
                ),
            ),
            ("timed_repeat".into(), Value::I64(self.timed_repeat as i64)),
            (
                "timed_delays".into(),
                Value::List(
                    self.timed_delays_ms
                        .iter()
                        .map(|delay| time_interval_ms(*delay))
                        .collect(),
                ),
            ),
            ("routes".into(), Value::List(self.local_timing_routes(plan))),
            (
                "sequences".into(),
                Value::List(self.local_timing_sequences(plan)),
            ),
            (
                "applied".into(),
                Value::List(
                    self.local_timing_sequence_refs(plan)
                        .into_iter()
                        .map(|sequence| {
                            Value::String(format!(
                                "{}:{}",
                                sequence.device.0 .0,
                                Self::public_key(&sequence.property)
                            ))
                        })
                        .collect(),
                ),
            ),
        ]))
    }

    fn timing_transaction(
        &self,
        description: &str,
        command: protocol::ArduinoCommand,
    ) -> PhysicalTransaction {
        PhysicalTransaction {
            resource: Some(self.resource),
            description: description.into(),
            payload: Value::Bytes(protocol::encode_command(&command)),
        }
    }
}

impl Driver for ArduinoDriver {
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
            label: "arduino-serial".into(),
            kind: "serial".into(),
            metadata: BTreeMap::from([
                ("baud_rate".into(), Value::I64(self.baud_rate as i64)),
                ("stop_bits".into(), Value::I64(1)),
                ("handshake".into(), Value::String("none".into())),
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
            device if device == self.digital => vec![
                capability(
                    1,
                    device,
                    CapabilityKind::DigitalIo,
                    ValueType::Map,
                    ValueType::Map,
                ),
                capability(
                    2,
                    device,
                    CapabilityKind::TriggerSource,
                    ValueType::Map,
                    ValueType::Map,
                ),
            ],
            device if device == self.shutter => vec![capability(
                3,
                device,
                CapabilityKind::TriggerSink,
                ValueType::Map,
                ValueType::Map,
            )],
            device if device == self.adc => vec![
                capability(
                    4,
                    device,
                    CapabilityKind::Adc,
                    ValueType::Map,
                    ValueType::Map,
                ),
                capability(
                    6,
                    device,
                    CapabilityKind::GenericCommand,
                    ValueType::Map,
                    ValueType::Map,
                ),
            ],
            device if device == self.dac => vec![capability(
                5,
                device,
                CapabilityKind::Dac,
                ValueType::Map,
                ValueType::Map,
            )],
            _ => Vec::new(),
        }
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut physical_transactions = Vec::new();
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    let public_key = Self::public_key(key);
                    let _ = self.read_property(*device, public_key)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("arduino read {public_key}"),
                        payload: Value::String(public_key.into()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    let public_key = Self::public_key(key);
                    let descriptor = self
                        .descriptors_for()
                        .into_iter()
                        .find(|descriptor| descriptor.id == *device)
                        .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown device"))?;
                    let schema = descriptor
                        .properties
                        .iter()
                        .find(|property| property.key == public_key)
                        .ok_or_else(|| {
                            Error::new(ErrorCode::InvalidProperty, "unknown property")
                        })?;
                    schema.validate(value)?;
                    if !schema.writable {
                        return Err(Error::new(
                            ErrorCode::InvalidProperty,
                            "property is read-only",
                        ));
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("arduino write {public_key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    let mut writes = Vec::new();
                    for write in &set.writes {
                        writes.push(Value::Map(BTreeMap::from([
                            ("device".into(), Value::I64((write.device.0).0 as i64)),
                            (
                                "property".into(),
                                Value::String(Self::public_key(&write.property).into()),
                            ),
                            ("value".into(), write.value.clone()),
                        ])));
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "arduino remultiplexed state set".into(),
                        payload: Value::List(writes),
                    });
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    let Some(descriptor) = self
                        .capabilities(*device)
                        .into_iter()
                        .find(|cap| cap.id == *capability)
                    else {
                        return Err(Error::new(ErrorCode::Unsupported, "unknown capability"));
                    };
                    if !descriptor.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "{} expects {:?}, got {:?}",
                                descriptor.kind.name(),
                                descriptor.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("arduino invoke {}", capability.0),
                        payload: match request {
                            CapabilityRequest::None => Value::Null,
                            CapabilityRequest::DigitalIo(request) => {
                                Value::Map(BTreeMap::from([(
                                    "mask".into(),
                                    Value::I64(request.mask as i64),
                                )]))
                            }
                            CapabilityRequest::Dac(request) => Value::Map(BTreeMap::from([(
                                "value".into(),
                                request.value.clone(),
                            )])),
                            CapabilityRequest::Adc(request) => Value::Map(BTreeMap::from([
                                (
                                    "channel".into(),
                                    request
                                        .channel
                                        .as_ref()
                                        .map(|channel| Value::String(channel.clone()))
                                        .unwrap_or(Value::Null),
                                ),
                                (
                                    "integration_time".into(),
                                    request
                                        .integration_time
                                        .map(Value::TimeInterval)
                                        .unwrap_or(Value::Null),
                                ),
                            ])),
                            CapabilityRequest::Trigger(request) => Value::Map(BTreeMap::from([
                                (
                                    "action".into(),
                                    Value::String(format!("{:?}", request.action)),
                                ),
                                (
                                    "duration".into(),
                                    request
                                        .duration
                                        .map(Value::TimeInterval)
                                        .unwrap_or(Value::Null),
                                ),
                            ])),
                            CapabilityRequest::GenericCommand(request) => {
                                self.validate_generic_command(*device, request)?;
                                Value::Map(BTreeMap::from([(
                                    "command".into(),
                                    Value::String(request.command.clone()),
                                )]))
                            }
                            _ => {
                                return Err(Error::new(
                                    ErrorCode::Unsupported,
                                    "unsupported Arduino request",
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
                    let public_key = Self::public_key(&key);
                    self.issue_read_command(device, public_key)?;
                    last = self.read_property(device, public_key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    let public_key = Self::public_key(&key);
                    last = self.write_property(device, public_key, &value)?;
                    self.emit_property(device, public_key, last.clone());
                }
                Command::ApplyStateSet(set) => {
                    let mut changed = BTreeMap::new();
                    for write in set.writes {
                        let property = Self::public_key(&write.property);
                        let value = self.write_property(write.device, property, &write.value)?;
                        self.emit_property(write.device, property, value.clone());
                        changed.insert(format!("{}:{}", (write.device.0).0, property), value);
                    }
                    last = Value::Map(changed);
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
                        return Err(Error::new(ErrorCode::Unsupported, "unknown capability"));
                    };
                    last = self.apply_invoke(device, descriptor.kind, request)?;
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
            for line in self.line_codec.push(&bytes) {
                self.pending
                    .push_back(DriverEvent::Event(Event::Log(LogEvent {
                        driver: Some(self.id),
                        message: format!("arduino serial: {line}"),
                    })));
            }
            let _ = self.count_codec.push(&[]);
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
                description: "arduino timing arm summary".into(),
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
        if self.has_timed_digital_sequence(&armed.plan)
            && !self.has_explicit_sequence(&armed.plan, self.digital, "sequence")
        {
            let value =
                self.write_property(self.digital, "sequence", &Value::String("On".into()))?;
            self.emit_property(self.digital, "sequence", value);
            physical_transactions.push(self.timing_transaction(
                "arduino timing start digital sequence",
                protocol::ArduinoCommand::StartSequence,
            ));
        }
        if self.has_timed_shutter(&armed.plan)
            && !self.has_explicit_sequence(&armed.plan, self.shutter, "open")
        {
            let value = self.write_property(self.shutter, "open", &Value::Bool(true))?;
            self.emit_property(self.shutter, "open", value);
            physical_transactions.push(self.timing_transaction(
                "arduino timing start shutter open",
                protocol::ArduinoCommand::SetDigitalMask {
                    mask: self.digital_mask as u8,
                },
            ));
        }
        physical_transactions.push(PhysicalTransaction {
            resource: Some(self.resource),
            description: "arduino timing start summary".into(),
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
        if self.has_timed_digital_sequence(&armed.plan)
            && !self.has_explicit_sequence(&armed.plan, self.digital, "sequence")
        {
            let value =
                self.write_property(self.digital, "sequence", &Value::String("Off".into()))?;
            self.emit_property(self.digital, "sequence", value);
            physical_transactions.push(self.timing_transaction(
                "arduino timing stop digital sequence",
                protocol::ArduinoCommand::StopSequence,
            ));
        }
        if self.has_timed_shutter(&armed.plan)
            && !self.has_explicit_sequence(&armed.plan, self.shutter, "open")
        {
            let value = self.write_property(self.shutter, "open", &Value::Bool(false))?;
            self.emit_property(self.shutter, "open", value);
            physical_transactions.push(self.timing_transaction(
                "arduino timing stop shutter close",
                protocol::ArduinoCommand::SetDigitalMask { mask: 0 },
            ));
        }
        physical_transactions.push(PhysicalTransaction {
            resource: Some(self.resource),
            description: "arduino timing stop summary".into(),
            payload: with_applied(self.timing_summary(&armed.plan, "stop"), applied),
        });
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions,
        })
    }
}

impl ArduinoDriver {
    fn apply_invoke(
        &mut self,
        device: DeviceId,
        kind: CapabilityKind,
        request: CapabilityRequest,
    ) -> Result<Value> {
        match kind {
            CapabilityKind::DigitalIo if device == self.digital => {
                let mask = digital_mask_request(&request, self.digital_mask)?;
                let value = self.write_property(device, "mask", &Value::I64(mask as i64))?;
                self.emit_property(device, "mask", value.clone());
                Ok(Value::Map(BTreeMap::from([
                    ("mask".into(), value),
                    ("commands".into(), Value::I64(1)),
                ])))
            }
            CapabilityKind::TriggerSource if device == self.digital => {
                let action = trigger_action_request(&request)?;
                let sequence = match action {
                    TriggerAction::Start => "On",
                    TriggerAction::Stop => "Off",
                    TriggerAction::Pulse => "On",
                };
                let value =
                    self.write_property(device, "sequence", &Value::String(sequence.into()))?;
                self.emit_property(device, "sequence", value.clone());
                if matches!(action, TriggerAction::Pulse) {
                    let stop =
                        self.write_property(device, "sequence", &Value::String("Off".into()))?;
                    self.emit_property(device, "sequence", stop);
                }
                Ok(Value::Map(BTreeMap::from([
                    ("triggered".into(), Value::Bool(true)),
                    ("sequence".into(), self.read_property(device, "sequence")?),
                ])))
            }
            CapabilityKind::TriggerSink if device == self.shutter => {
                let action = trigger_action_request(&request)?;
                for open in match action {
                    TriggerAction::Start => vec![true],
                    TriggerAction::Stop => vec![false],
                    TriggerAction::Pulse => vec![true, false],
                } {
                    let value = self.write_property(device, "open", &Value::Bool(open))?;
                    self.emit_property(device, "open", value);
                }
                Ok(Value::Map(BTreeMap::from([
                    ("triggered".into(), Value::Bool(true)),
                    ("open".into(), self.read_property(device, "open")?),
                ])))
            }
            CapabilityKind::Adc if device == self.adc => {
                let key = adc_request_key(&request)?;
                self.issue_read_command(device, &key)?;
                self.read_property(device, &key)
            }
            CapabilityKind::GenericCommand if device == self.adc => {
                let CapabilityRequest::GenericCommand(request) = request else {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "Arduino GenericCommand expects GenericCommand",
                    ));
                };
                self.apply_generic_command(device, request)
            }
            CapabilityKind::Dac if device == self.dac => {
                let (key, value) = dac_request(&request)?;
                let value = self.write_property(device, &key, &Value::I64(value))?;
                self.emit_property(device, &key, value.clone());
                Ok(Value::Map(BTreeMap::from([
                    ("property".into(), Value::String(key)),
                    ("value".into(), value),
                    ("commands".into(), Value::I64(1)),
                ])))
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Arduino invocation capability",
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
        if device != self.adc {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Arduino GenericCommand is available on the ADC device",
            ));
        }
        if !request.params.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Arduino GenericCommand does not take parameters",
            ));
        }
        match request.command.as_str() {
            "refresh_inputs" | "refresh_digital_inputs" | "refresh_channel_0" => Ok(()),
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "Arduino GenericCommand supports refresh_inputs, refresh_digital_inputs, and refresh_channel_0; got {other}"
                ),
            )),
        }
    }

    fn apply_generic_command(
        &mut self,
        device: DeviceId,
        request: GenericCommandRequest,
    ) -> Result<Value> {
        self.validate_generic_command(device, &request)?;
        let (key, commands) = match request.command.as_str() {
            "refresh_inputs" => ("input_summary", 2),
            "refresh_digital_inputs" => ("digital_inputs", 1),
            "refresh_channel_0" => ("channel_0", 1),
            _ => unreachable!(),
        };
        self.issue_read_command(device, key)?;
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("commands".into(), Value::I64(commands)),
            ("state".into(), self.read_property(device, key)?),
            (
                "completion_basis".into(),
                Value::String("Arduino mapped input readback".into()),
            ),
        ])))
    }

    fn issue_read_command(&mut self, device: DeviceId, key: &str) -> Result<()> {
        if device == self.adc && key == "digital_inputs" {
            self.serial.write(&protocol::encode_command(
                &protocol::ArduinoCommand::ReadDigitalInputs,
            ))?;
            if self.connected {
                let frame = self.read_probe_frame(2, self.serial_timeout_ms)?;
                self.digital_input_mask =
                    protocol::decode_u8_reply(protocol::CMD_READ_DIGITAL_INPUTS, &frame)?;
            }
        } else if device == self.adc && key == "input_summary" {
            self.serial.write(&protocol::encode_command(
                &protocol::ArduinoCommand::ReadDigitalInputs,
            ))?;
            if self.connected {
                let frame = self.read_probe_frame(2, self.serial_timeout_ms)?;
                self.digital_input_mask =
                    protocol::decode_u8_reply(protocol::CMD_READ_DIGITAL_INPUTS, &frame)?;
            }
            self.serial.write(&protocol::encode_command(
                &protocol::ArduinoCommand::ReadAdc { channel: 0 },
            ))?;
            if self.connected {
                let frame = self.read_probe_frame(3, self.serial_timeout_ms)?;
                let adc_count =
                    protocol::decode_count_reply(protocol::CMD_READ_ANALOG_INPUT, &frame)?;
                if let Some(slot) = self.adc_values.get_mut(0) {
                    *slot = adc_count;
                }
            }
        } else if device == self.adc && key.starts_with("channel_") {
            let channel = parse_channel_key(key)?;
            self.serial.write(&protocol::encode_command(
                &protocol::ArduinoCommand::ReadAdc {
                    channel: channel as u8,
                },
            ))?;
            if self.connected {
                let frame = self.read_probe_frame(3, self.serial_timeout_ms)?;
                let adc_count =
                    protocol::decode_count_reply(protocol::CMD_READ_ANALOG_INPUT, &frame)?;
                if let Some(slot) = self.adc_values.get_mut(channel) {
                    *slot = adc_count;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TriggerAction {
    Start,
    Stop,
    Pulse,
}

fn digital_mask_request(request: &CapabilityRequest, current: u64) -> Result<u64> {
    match request {
        CapabilityRequest::None => Ok(current),
        CapabilityRequest::DigitalIo(request) => Ok(request.mask),
        _ => Err(Error::new(
            ErrorCode::Unsupported,
            "Arduino DigitalIo expects DigitalIo",
        )),
    }
}

fn trigger_action_request(request: &CapabilityRequest) -> Result<TriggerAction> {
    match request {
        CapabilityRequest::None => Ok(TriggerAction::Pulse),
        CapabilityRequest::Trigger(request) => Ok(match request.action {
            numanager_core::TriggerAction::Enable => TriggerAction::Start,
            numanager_core::TriggerAction::Disable => TriggerAction::Stop,
            numanager_core::TriggerAction::Pulse => TriggerAction::Pulse,
        }),
        _ => Err(Error::new(
            ErrorCode::Unsupported,
            "Arduino trigger expects None or Trigger",
        )),
    }
}

fn dac_request(request: &CapabilityRequest) -> Result<(String, i64)> {
    match request {
        CapabilityRequest::Dac(request) => Ok(("channel_0".into(), i64_value(&request.value)?)),
        _ => Err(Error::new(
            ErrorCode::Unsupported,
            "Arduino Dac expects Dac",
        )),
    }
}

fn adc_request_key(request: &CapabilityRequest) -> Result<String> {
    match request {
        CapabilityRequest::None => Ok("input_summary".into()),
        CapabilityRequest::Adc(request) => Ok(request
            .channel
            .as_deref()
            .map(adc_channel_key)
            .unwrap_or_else(|| "input_summary".into())),
        _ => Err(Error::new(
            ErrorCode::Unsupported,
            "Arduino Adc expects None or Adc",
        )),
    }
}

fn adc_channel_key(channel: &str) -> String {
    match channel {
        "digital_inputs" | "input_summary" => channel.into(),
        value if value.starts_with("channel_") => value.into(),
        value => format!("channel_{value}"),
    }
}

fn i64_value(value: &Value) -> Result<i64> {
    match value {
        Value::I64(value) => Ok(*value),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            "expected integer value",
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

fn time_interval_ms(ms: u16) -> Value {
    Value::TimeInterval(TimeInterval::from_milliseconds(ms as f64))
}

fn timed_delay_ms(value: &Value) -> Result<u16> {
    match value {
        Value::TimeInterval(interval) => {
            let ms = (interval.seconds() * 1_000.0).round();
            if !ms.is_finite() || ms < 0.0 || ms > u16::MAX as f64 {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "timed delay is outside Arduino u16 millisecond range",
                ));
            }
            Ok(ms as u16)
        }
        Value::I64(delay) => Ok((*delay).clamp(0, u16::MAX as i64) as u16),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            "timed delays must contain typed time intervals",
        )),
    }
}

fn capability(
    id: u64,
    device: DeviceId,
    kind: CapabilityKind,
    request: ValueType,
    response: ValueType,
) -> CapabilityDescriptor {
    CapabilityDescriptor {
        id: CapabilityId(id),
        device,
        name: kind.name().to_string(),
        kind,
        request,
        response,
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

fn adc_properties(channels: usize, digital_pins: u8) -> Vec<PropertySchema> {
    let max_mask = digital_max_mask(digital_pins) as i64;
    let mut properties = vec![
        property(
            "digital_inputs",
            "Digital input mask",
            ValueType::I64,
            None,
            false,
            Some(Range {
                min: Value::I64(0),
                max: Value::I64(max_mask),
            }),
        ),
        property(
            "input_pullups",
            "Input pull-up mask",
            ValueType::I64,
            None,
            true,
            Some(Range {
                min: Value::I64(0),
                max: Value::I64(max_mask),
            }),
        ),
        property(
            "input_summary",
            "Input summary",
            ValueType::Map,
            None,
            false,
            None,
        ),
    ];
    properties.extend((0..channels).map(|channel| {
        property(
            &format!("channel_{channel}"),
            &format!("ADC channel {channel}"),
            ValueType::I64,
            Some("count"),
            false,
            Some(Range {
                min: Value::I64(0),
                max: Value::I64(1023),
            }),
        )
    }));
    properties
}

fn dac_properties(channels: usize) -> Vec<PropertySchema> {
    (0..channels)
        .map(|channel| {
            property(
                &format!("channel_{channel}"),
                &format!("DAC channel {channel}"),
                ValueType::I64,
                Some("count"),
                true,
                Some(Range {
                    min: Value::I64(0),
                    max: Value::I64(4095),
                }),
            )
        })
        .collect()
}

fn parse_channel_key(key: &str) -> Result<usize> {
    key.strip_prefix("channel_")
        .ok_or_else(|| Error::new(ErrorCode::InvalidProperty, "invalid channel property"))?
        .parse::<usize>()
        .map_err(|_| Error::new(ErrorCode::InvalidProperty, "invalid channel index"))
}

fn digital_max_mask(pins: u8) -> u8 {
    let pins = pins.min(8);
    if pins == 8 {
        u8::MAX
    } else {
        ((1u16 << pins) - 1) as u8
    }
}
