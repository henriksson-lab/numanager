use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::{LineEnding, ScriptedSerial, SerialIo, SerialLineCodec};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const BAUD: u32 = 19_200;
    pub const MAX_LINES: usize = 8;
    pub const MAX_TRANSMISSION: u16 = 1000;
    pub const ACK_ERROR: u8 = 0xff;

    const CMD_SHUTTER_CONTROL: u8 = 0x01;
    const CMD_SHUTTER_STATUS: u8 = 0x02;
    const CMD_CHANGE_TRANSMISSION: u8 = 0x04;
    const CMD_READ_WAVELENGTHS: u8 = 0x08;
    const CMD_TRIGGER_IN_CONFIGURE: u8 = 0x22;
    const CMD_TRIGGER_OUT_CONFIGURE: u8 = 0x23;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Lmm5Command {
        SetShutters(u8),
        ShutterStatus,
        SetTransmission {
            line: u8,
            level: u16,
        },
        ReadWavelengths,
        ConfigureTriggerIn {
            enabled: bool,
            count_before_action: u8,
            cycle_mode: bool,
        },
        ConfigureTriggerOut {
            enabled: bool,
            clock_mode: bool,
            interval_tenths_ms: u16,
        },
    }

    impl Lmm5Command {
        pub fn opcode(&self) -> u8 {
            match self {
                Lmm5Command::SetShutters(_) => CMD_SHUTTER_CONTROL,
                Lmm5Command::ShutterStatus => CMD_SHUTTER_STATUS,
                Lmm5Command::SetTransmission { .. } => CMD_CHANGE_TRANSMISSION,
                Lmm5Command::ReadWavelengths => CMD_READ_WAVELENGTHS,
                Lmm5Command::ConfigureTriggerIn { .. } => CMD_TRIGGER_IN_CONFIGURE,
                Lmm5Command::ConfigureTriggerOut { .. } => CMD_TRIGGER_OUT_CONFIGURE,
            }
        }

        pub fn bytes(&self) -> Vec<u8> {
            match self {
                Lmm5Command::SetShutters(mask) => vec![self.opcode(), *mask],
                Lmm5Command::ShutterStatus => vec![self.opcode()],
                Lmm5Command::SetTransmission { line, level } => {
                    let [high, low] = level.to_be_bytes();
                    vec![self.opcode(), line.saturating_sub(1), high, low]
                }
                Lmm5Command::ReadWavelengths => vec![self.opcode()],
                Lmm5Command::ConfigureTriggerIn {
                    enabled,
                    count_before_action,
                    cycle_mode,
                } => vec![
                    self.opcode(),
                    u8::from(*enabled),
                    *count_before_action,
                    u8::from(*cycle_mode),
                ],
                Lmm5Command::ConfigureTriggerOut {
                    enabled,
                    clock_mode,
                    interval_tenths_ms,
                } => {
                    let [high, low] = interval_tenths_ms.to_be_bytes();
                    vec![
                        self.opcode(),
                        u8::from(*enabled),
                        u8::from(*clock_mode),
                        high,
                        low,
                    ]
                }
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Lmm5Response {
        Ack(u8),
        ShutterStatus(u8),
        Wavelengths(Vec<Option<u16>>),
    }

    pub fn encode_ascii(command: &Lmm5Command) -> String {
        let mut line = String::new();
        for byte in command.bytes() {
            line.push_str(&format!("{byte:02X}"));
        }
        line
    }

    pub fn decode_response(data: &[u8], expected: u8) -> Result<Lmm5Response> {
        let bytes = parse_response_bytes(data)?;
        if bytes.is_empty() {
            return Err(Error::new(
                ErrorCode::Transport,
                "Spectral LMM5 response was empty",
            ));
        }
        if bytes[0] == ACK_ERROR {
            return Err(Error::new(
                ErrorCode::Transport,
                "Spectral LMM5 returned an error ACK",
            ));
        }
        if bytes[0] != expected {
            return Err(Error::new(
                ErrorCode::Transport,
                format!(
                    "Spectral LMM5 response opcode {:02x} does not match expected {:02x}",
                    bytes[0], expected
                ),
            ));
        }
        match expected {
            CMD_SHUTTER_STATUS if bytes.len() >= 2 => Ok(Lmm5Response::ShutterStatus(bytes[1])),
            CMD_READ_WAVELENGTHS if bytes.len() >= 1 + MAX_LINES * 2 => {
                let mut wavelengths = Vec::with_capacity(MAX_LINES);
                for chunk in bytes[1..(1 + MAX_LINES * 2)].chunks_exact(2) {
                    let value = u16::from_be_bytes([chunk[0], chunk[1]]);
                    wavelengths.push((value != 0).then_some(value));
                }
                Ok(Lmm5Response::Wavelengths(wavelengths))
            }
            _ => Ok(Lmm5Response::Ack(bytes[0])),
        }
    }

    fn parse_response_bytes(data: &[u8]) -> Result<Vec<u8>> {
        let trimmed = trim_response(data);
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        if trimmed.iter().all(|byte| byte.is_ascii_hexdigit()) && trimmed.len() % 2 == 0 {
            return (0..trimmed.len())
                .step_by(2)
                .map(|index| parse_hex_pair(trimmed[index], trimmed[index + 1]))
                .collect();
        }
        Ok(trimmed.to_vec())
    }

    fn trim_response(data: &[u8]) -> &[u8] {
        let mut start = 0;
        let mut end = data.len();
        while start < end && data[start].is_ascii_whitespace() {
            start += 1;
        }
        while end > start && data[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
        &data[start..end]
    }

    fn parse_hex_pair(high: u8, low: u8) -> Result<u8> {
        let high = hex_value(high)?;
        let low = hex_value(low)?;
        Ok((high << 4) | low)
    }

    fn hex_value(byte: u8) -> Result<u8> {
        match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'a'..=b'f' => Ok(byte - b'a' + 10),
            b'A'..=b'F' => Ok(byte - b'A' + 10),
            _ => Err(Error::new(
                ErrorCode::Transport,
                "invalid Spectral LMM5 hexadecimal response byte",
            )),
        }
    }

    pub fn ack_response(opcode: u8) -> Vec<u8> {
        format!("{opcode:02X}\r").into_bytes()
    }

    pub fn shutter_status_response(mask: u8) -> Vec<u8> {
        format!("{CMD_SHUTTER_STATUS:02X}{mask:02X}\r").into_bytes()
    }

    pub fn wavelength_response(wavelengths: &[Option<u16>]) -> Vec<u8> {
        let mut bytes = vec![CMD_READ_WAVELENGTHS];
        for index in 0..MAX_LINES {
            let value = wavelengths.get(index).copied().flatten().unwrap_or(0);
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes_to_ascii_line(&bytes)
    }

    fn bytes_to_ascii_line(bytes: &[u8]) -> Vec<u8> {
        let mut line = String::new();
        for byte in bytes {
            line.push_str(&format!("{byte:02X}"));
        }
        line.push('\r');
        line.into_bytes()
    }
}

#[derive(Debug, Clone)]
pub struct Lmm5ConfiguredProbe {
    label: String,
    serial_port: Option<String>,
    connect_real_transport: bool,
    product: String,
    serial_number: String,
    line_count: usize,
    wavelengths: Vec<Option<u16>>,
    shutter_mask: u8,
    transmissions: Vec<u16>,
    trigger_in_enabled: bool,
    trigger_in_count: u8,
    trigger_in_cycle: bool,
    trigger_out_enabled: bool,
    trigger_out_clock: bool,
    trigger_out_interval_tenths_ms: u16,
}

pub struct Lmm5Discovery {
    next_id: DriverId,
    probes: Vec<Lmm5ConfiguredProbe>,
}

impl Lmm5Discovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![Lmm5ConfiguredProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), "spectral_lmm5" | "lmm5"))
            .map(Lmm5ConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for Lmm5Discovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver: Box<dyn Driver> = if configured.connect_real_transport {
                    Box::new(Lmm5Driver::serial(id, configured)?)
                } else {
                    Box::new(Lmm5Driver::configured(id, configured))
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

impl Lmm5ConfiguredProbe {
    pub fn fixture() -> Self {
        Self {
            label: "Configured Spectral LMM5".into(),
            serial_port: None,
            connect_real_transport: false,
            product: "Laser Merge Module LMM5".into(),
            serial_number: "LMM5-CONFIG-0001".into(),
            line_count: 5,
            wavelengths: vec![
                Some(405),
                Some(440),
                Some(488),
                Some(561),
                Some(640),
                None,
                None,
                None,
            ],
            shutter_mask: 0,
            transmissions: vec![0; protocol::MAX_LINES],
            trigger_in_enabled: false,
            trigger_in_count: 1,
            trigger_in_cycle: false,
            trigger_out_enabled: false,
            trigger_out_clock: false,
            trigger_out_interval_tenths_ms: 0,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::fixture();
        if !device.label.is_empty() {
            configured.label = device.label.clone();
        }
        configured.product = string_prop(device, "product").unwrap_or(configured.product);
        configured.serial_number =
            string_prop(device, "serial_number").unwrap_or(configured.serial_number);
        configured.line_count = usize_prop(device, "line_count").unwrap_or(configured.line_count);
        if !(1..=protocol::MAX_LINES).contains(&configured.line_count) {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Spectral LMM5 line_count must be in 1..=8",
            ));
        }
        configured.wavelengths = (1..=protocol::MAX_LINES)
            .map(|line| {
                wavelength_prop(device, &format!("line_{line}_wavelength"))
                    .map(|value| value.round().clamp(0.0, u16::MAX as f64) as u16)
            })
            .collect();
        configured.shutter_mask =
            u8_prop(device, "shutter_mask").unwrap_or(configured.shutter_mask);
        configured.transmissions = (1..=protocol::MAX_LINES)
            .map(|line| {
                ratio_prop(device, &format!("line_{line}_transmission"))
                    .map(ratio_to_lmm5)
                    .unwrap_or_else(|| {
                        if configured.shutter_mask & line_mask(line) != 0 {
                            protocol::MAX_TRANSMISSION
                        } else {
                            0
                        }
                    })
            })
            .collect();
        configured.trigger_in_enabled =
            bool_prop(device, "trigger_in_enabled").unwrap_or(configured.trigger_in_enabled);
        configured.trigger_in_count =
            u8_prop(device, "trigger_in_count").unwrap_or(configured.trigger_in_count);
        configured.trigger_in_cycle =
            bool_prop(device, "trigger_in_cycle").unwrap_or(configured.trigger_in_cycle);
        configured.trigger_out_enabled =
            bool_prop(device, "trigger_out_enabled").unwrap_or(configured.trigger_out_enabled);
        configured.trigger_out_clock =
            bool_prop(device, "trigger_out_clock").unwrap_or(configured.trigger_out_clock);
        configured.trigger_out_interval_tenths_ms =
            trigger_interval_prop(device, "trigger_out_interval")
                .unwrap_or(configured.trigger_out_interval_tenths_ms);
        configured.connect_real_transport = bool_prop(device, "connect").unwrap_or(false);
        configured.serial_port = string_prop(device, "serial_port");
        Ok(configured)
    }
}

#[derive(Debug, Clone)]
struct Lmm5LineState {
    device: DeviceId,
    label: String,
    wavelength: Option<u16>,
    enabled: bool,
    transmission: u16,
}

pub struct Lmm5Driver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    product: String,
    serial_number: String,
    serial_port: Option<String>,
    connected: bool,
    lines: Vec<Lmm5LineState>,
    trigger_in_enabled: bool,
    trigger_in_count: u8,
    trigger_in_cycle: bool,
    trigger_out_enabled: bool,
    trigger_out_clock: bool,
    trigger_out_interval_tenths_ms: u16,
    serial: Box<dyn SerialIo>,
    line_codec: SerialLineCodec,
    synthesize_responses: bool,
    last_transaction: Value,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
}

impl Lmm5Driver {
    pub fn configured(id: DriverId, configured: Lmm5ConfiguredProbe) -> Self {
        let reads = vec![
            protocol::shutter_status_response(configured.shutter_mask),
            protocol::wavelength_response(&configured.wavelengths),
        ];
        let mut driver = Self::new(id, configured, Box::new(ScriptedSerial::with_reads(reads)));
        driver.synthesize_responses = true;
        driver
    }

    pub fn serial(driver_id: DriverId, configured: Lmm5ConfiguredProbe) -> Result<Self> {
        let port_name = configured.serial_port.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Spectral LMM5 real serial config requires serial_port",
            )
        })?;
        #[cfg(feature = "os-serial")]
        {
            let serial = Box::new(numanager_core::serial::OsSerialPort::open_config(
                numanager_core::serial::OsSerialConfig::new(port_name, protocol::BAUD),
            )?);
            let mut driver = Self::new(driver_id, configured, serial);
            driver.read_shutter_status()?;
            driver.read_wavelengths()?;
            Ok(driver)
        }
        #[cfg(not(feature = "os-serial"))]
        {
            let _ = driver_id;
            let _ = port_name;
            Err(Error::new(
                ErrorCode::Unsupported,
                "Spectral LMM5 real serial transport requires the os-serial feature",
            ))
        }
    }

    pub fn new(id: DriverId, configured: Lmm5ConfiguredProbe, serial: Box<dyn SerialIo>) -> Self {
        let hub = DeviceId(NodeId(id.0 * 1000 + 900));
        let lines = (0..configured.line_count)
            .map(|index| Lmm5LineState {
                device: DeviceId(NodeId(id.0 * 1000 + 901 + index as u64)),
                label: format!("spectral-lmm5-line-{}", index + 1),
                wavelength: configured.wavelengths.get(index).copied().flatten(),
                enabled: configured.shutter_mask & line_mask(index + 1) != 0,
                transmission: configured
                    .transmissions
                    .get(index)
                    .copied()
                    .unwrap_or_default()
                    .min(protocol::MAX_TRANSMISSION),
            })
            .collect();
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 899)),
            hub,
            product: configured.product,
            serial_number: configured.serial_number,
            serial_port: configured.serial_port,
            connected: configured.connect_real_transport,
            lines,
            trigger_in_enabled: configured.trigger_in_enabled,
            trigger_in_count: configured.trigger_in_count,
            trigger_in_cycle: configured.trigger_in_cycle,
            trigger_out_enabled: configured.trigger_out_enabled,
            trigger_out_clock: configured.trigger_out_clock,
            trigger_out_interval_tenths_ms: configured.trigger_out_interval_tenths_ms,
            serial,
            line_codec: SerialLineCodec::new(LineEnding::Cr, LineEnding::Cr),
            synthesize_responses: false,
            last_transaction: Value::Map(BTreeMap::new()),
            next_token: 1,
            pending: VecDeque::new(),
        }
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn send(&mut self, command: protocol::Lmm5Command) -> Result<Option<protocol::Lmm5Response>> {
        let line = protocol::encode_ascii(&command);
        let bytes = self.line_codec.encode(&line);
        self.serial.write(&bytes)?;
        match self.read_response(command.opcode()) {
            Ok(Some(response)) => Ok(Some(response)),
            Ok(None) if self.synthesize_responses => Ok(Some(self.synthetic_response(&command))),
            Ok(None) => Ok(None),
            Err(_) if self.synthesize_responses => Ok(Some(self.synthetic_response(&command))),
            Err(error) => Err(error),
        }
    }

    fn read_response(&mut self, expected: u8) -> Result<Option<protocol::Lmm5Response>> {
        let bytes = self.serial.read_available()?;
        if bytes.is_empty() {
            return Ok(None);
        }
        let lines = self.line_codec.push(&bytes);
        if let Some(line) = lines.first() {
            return protocol::decode_response(line.as_bytes(), expected).map(Some);
        }
        protocol::decode_response(&bytes, expected).map(Some)
    }

    fn synthetic_response(&self, command: &protocol::Lmm5Command) -> protocol::Lmm5Response {
        match command {
            protocol::Lmm5Command::ShutterStatus => {
                protocol::Lmm5Response::ShutterStatus(self.shutter_mask())
            }
            protocol::Lmm5Command::ReadWavelengths => protocol::Lmm5Response::Wavelengths(
                self.lines.iter().map(|line| line.wavelength).collect(),
            ),
            _ => protocol::Lmm5Response::Ack(command.opcode()),
        }
    }

    fn shutter_mask(&self) -> u8 {
        self.lines
            .iter()
            .enumerate()
            .fold(0, |mask, (index, line)| {
                if line.enabled {
                    mask | line_mask(index + 1)
                } else {
                    mask
                }
            })
    }

    fn line_index(&self, device: DeviceId) -> Option<usize> {
        self.lines.iter().position(|line| line.device == device)
    }

    fn line_index_required(&self, device: DeviceId) -> Result<usize> {
        self.line_index(device).ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidCommand,
                "unknown Spectral LMM5 laser-line device",
            )
        })
    }

    fn set_shutter_mask(&mut self, mask: u8) -> Result<Value> {
        self.send(protocol::Lmm5Command::SetShutters(mask))?;
        for (index, line) in self.lines.iter_mut().enumerate() {
            line.enabled = mask & line_mask(index + 1) != 0;
        }
        self.read_shutter_status()?;
        self.last_transaction = self.transaction("set_shutters", "ack_plus_status_readback");
        Ok(Value::I64(self.shutter_mask() as i64))
    }

    fn read_shutter_status(&mut self) -> Result<u8> {
        match self.send(protocol::Lmm5Command::ShutterStatus)? {
            Some(protocol::Lmm5Response::ShutterStatus(mask)) => {
                for (index, line) in self.lines.iter_mut().enumerate() {
                    line.enabled = mask & line_mask(index + 1) != 0;
                }
            }
            Some(_) | None => {}
        }
        self.last_transaction = self.transaction("read_shutter_status", "status_readback");
        Ok(self.shutter_mask())
    }

    fn set_line_enabled(&mut self, index: usize, enabled: bool) -> Result<Value> {
        let mut mask = self.shutter_mask();
        if enabled {
            mask |= line_mask(index + 1);
        } else {
            mask &= !line_mask(index + 1);
        }
        let value = self.set_shutter_mask(mask)?;
        self.emit_property(self.lines[index].device, "enabled", Value::Bool(enabled));
        Ok(value)
    }

    fn set_line_transmission(&mut self, index: usize, ratio: Ratio) -> Result<Value> {
        let level = ratio_to_lmm5(ratio);
        let line_number = (index + 1) as u8;
        self.send(protocol::Lmm5Command::SetTransmission {
            line: line_number,
            level,
        })?;
        self.lines[index].transmission = level;
        self.last_transaction = self.transaction("set_transmission", "ack");
        let value = Value::Ratio(lmm5_to_ratio(level));
        self.emit_property(self.lines[index].device, "transmission", value.clone());
        Ok(value)
    }

    fn read_wavelengths(&mut self) -> Result<()> {
        match self.send(protocol::Lmm5Command::ReadWavelengths)? {
            Some(protocol::Lmm5Response::Wavelengths(wavelengths)) => {
                for (line, wavelength) in self.lines.iter_mut().zip(wavelengths.into_iter()) {
                    line.wavelength = wavelength;
                }
            }
            Some(_) | None => {}
        }
        self.last_transaction = self.transaction("read_wavelengths", "serial_response");
        Ok(())
    }

    fn configure_trigger_in(&mut self) -> Result<Value> {
        self.send(protocol::Lmm5Command::ConfigureTriggerIn {
            enabled: self.trigger_in_enabled,
            count_before_action: self.trigger_in_count,
            cycle_mode: self.trigger_in_cycle,
        })?;
        self.last_transaction = self.transaction("configure_trigger_in", "ack");
        Ok(Value::Map(BTreeMap::from([
            ("enabled".into(), Value::Bool(self.trigger_in_enabled)),
            ("count".into(), Value::I64(self.trigger_in_count as i64)),
            ("cycle".into(), Value::Bool(self.trigger_in_cycle)),
        ])))
    }

    fn configure_trigger_out(&mut self) -> Result<Value> {
        self.send(protocol::Lmm5Command::ConfigureTriggerOut {
            enabled: self.trigger_out_enabled,
            clock_mode: self.trigger_out_clock,
            interval_tenths_ms: self.trigger_out_interval_tenths_ms,
        })?;
        self.last_transaction = self.transaction("configure_trigger_out", "ack");
        Ok(Value::Map(BTreeMap::from([
            ("enabled".into(), Value::Bool(self.trigger_out_enabled)),
            ("clock".into(), Value::Bool(self.trigger_out_clock)),
            (
                "interval".into(),
                Value::TimeInterval(trigger_interval_value(self.trigger_out_interval_tenths_ms)),
            ),
        ])))
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
                "Spectral LMM5 GenericCommand does not take parameters",
            ));
        }
        let _ = lmm5_generic_command_kind(&request.command)?;
        Ok(())
    }

    fn apply_generic_command(&mut self, request: GenericCommandRequest) -> Result<Value> {
        self.validate_generic_command(&request)?;
        match lmm5_generic_command_kind(&request.command)? {
            Lmm5GenericCommand::RefreshReadbacks => {
                self.read_shutter_status()?;
                let shutter_status = self.shutter_status_summary("refresh_shutter_status".into());
                self.read_wavelengths()?;
                let wavelengths = self.wavelength_summary("refresh_wavelengths".into());
                Ok(Value::Map(BTreeMap::from([
                    ("command".into(), Value::String(request.command)),
                    ("shutter_status".into(), shutter_status),
                    ("wavelengths".into(), wavelengths),
                    (
                        "completion_basis".into(),
                        Value::String(
                            "Spectral LMM5 shutter-status and wavelength readbacks".into(),
                        ),
                    ),
                ])))
            }
            Lmm5GenericCommand::RefreshShutterStatus => {
                self.read_shutter_status()?;
                Ok(self.shutter_status_summary(request.command))
            }
            Lmm5GenericCommand::RefreshWavelengths => {
                self.read_wavelengths()?;
                Ok(self.wavelength_summary(request.command))
            }
            Lmm5GenericCommand::ApplyTriggerIn => {
                let state = self.configure_trigger_in()?;
                Ok(lmm5_generic_trigger_result(
                    request.command,
                    "trigger_in",
                    state,
                ))
            }
            Lmm5GenericCommand::ApplyTriggerOut => {
                let state = self.configure_trigger_out()?;
                Ok(lmm5_generic_trigger_result(
                    request.command,
                    "trigger_out",
                    state,
                ))
            }
            Lmm5GenericCommand::ApplyTriggerProfiles => {
                let trigger_in = self.configure_trigger_in()?;
                let trigger_out = self.configure_trigger_out()?;
                Ok(Value::Map(BTreeMap::from([
                    ("command".into(), Value::String(request.command)),
                    ("trigger_in".into(), trigger_in),
                    ("trigger_out".into(), trigger_out),
                    (
                        "completion_basis".into(),
                        Value::String("Spectral LMM5 trigger configure ACKs".into()),
                    ),
                ])))
            }
        }
    }

    fn shutter_status_summary(&self, command: String) -> Value {
        Value::Map(BTreeMap::from([
            ("command".into(), Value::String(command)),
            (
                "shutter_mask".into(),
                Value::I64(self.shutter_mask() as i64),
            ),
            (
                "enabled".into(),
                Value::List(
                    self.lines
                        .iter()
                        .map(|line| Value::Bool(line.enabled))
                        .collect(),
                ),
            ),
            (
                "completion_basis".into(),
                Value::String("Spectral LMM5 shutter-status readback".into()),
            ),
        ]))
    }

    fn wavelength_summary(&self, command: String) -> Value {
        Value::Map(BTreeMap::from([
            ("command".into(), Value::String(command)),
            (
                "wavelengths".into(),
                Value::List(
                    self.lines
                        .iter()
                        .map(|line| {
                            line.wavelength
                                .map(|nm| Value::Wavelength(Wavelength::from_nanometers(nm as f64)))
                                .unwrap_or(Value::Null)
                        })
                        .collect(),
                ),
            ),
            (
                "completion_basis".into(),
                Value::String("Spectral LMM5 wavelength readback".into()),
            ),
        ]))
    }

    fn trigger_sink(&mut self, device: DeviceId, request: CapabilityRequest) -> Result<Value> {
        let request = match request {
            CapabilityRequest::Trigger(request) => request,
            CapabilityRequest::None => TriggerRequest::pulse(),
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "Spectral LMM5 trigger sink expects TriggerRequest",
                ));
            }
        };
        let index = self.line_index_required(device)?;
        match request.action {
            TriggerAction::Enable => {
                self.set_line_enabled(index, true)?;
            }
            TriggerAction::Disable => {
                self.set_line_enabled(index, false)?;
            }
            TriggerAction::Pulse => {
                self.set_line_enabled(index, true)?;
                self.set_line_enabled(index, false)?;
            }
        }
        Ok(Value::Map(BTreeMap::from([
            ("triggered".into(), Value::Bool(true)),
            ("enabled".into(), Value::Bool(self.lines[index].enabled)),
        ])))
    }

    fn dac(&mut self, device: DeviceId, request: CapabilityRequest) -> Result<Value> {
        let CapabilityRequest::Dac(request) = request else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Spectral LMM5 DAC expects DacRequest",
            ));
        };
        let Value::Ratio(ratio) = request.value else {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Spectral LMM5 DAC expects Ratio percent transmission",
            ));
        };
        let index = self.line_index_required(device)?;
        self.set_line_transmission(index, ratio)
    }

    fn read_property(&mut self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "product" => Ok(Value::String(self.product.clone())),
                "serial_number" => Ok(Value::String(self.serial_number.clone())),
                "protocol" => Ok(Value::String("Spectral LMM5 RS-232 hex protocol".into())),
                "line_count" => Ok(Value::I64(self.lines.len() as i64)),
                "shutter_mask" => Ok(Value::I64(self.read_shutter_status()? as i64)),
                "trigger_in_enabled" => Ok(Value::Bool(self.trigger_in_enabled)),
                "trigger_in_count" => Ok(Value::I64(self.trigger_in_count as i64)),
                "trigger_in_cycle" => Ok(Value::Bool(self.trigger_in_cycle)),
                "trigger_out_enabled" => Ok(Value::Bool(self.trigger_out_enabled)),
                "trigger_out_clock" => Ok(Value::Bool(self.trigger_out_clock)),
                "trigger_out_interval" => Ok(Value::TimeInterval(trigger_interval_value(
                    self.trigger_out_interval_tenths_ms,
                ))),
                "last_transaction" => Ok(self.last_transaction.clone()),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown Spectral LMM5 hub property {key}"),
                )),
            };
        }
        let index = self.line_index_required(device)?;
        if key == "wavelength" {
            self.read_wavelengths()?;
        } else if key == "enabled" {
            self.read_shutter_status()?;
        }
        let line = &self.lines[index];
        match key {
            "line" => Ok(Value::I64(index as i64 + 1)),
            "wavelength" => Ok(line
                .wavelength
                .map(|nm| Value::Wavelength(Wavelength::from_nanometers(nm as f64)))
                .unwrap_or(Value::Null)),
            "enabled" => Ok(Value::Bool(line.enabled)),
            "transmission" => Ok(Value::Ratio(lmm5_to_ratio(line.transmission))),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Spectral LMM5 line property {key}"),
            )),
        }
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        if device == self.hub {
            return match (key, value) {
                ("trigger_in_enabled" | "trigger_out_enabled", Value::Bool(_)) => Ok(()),
                ("trigger_in_cycle" | "trigger_out_clock", Value::Bool(_)) => Ok(()),
                ("trigger_in_count", Value::I64(count)) if (0..=u8::MAX as i64).contains(count) => {
                    Ok(())
                }
                ("trigger_out_interval", Value::TimeInterval(interval)) => {
                    let _ = trigger_interval_to_tenths_ms(*interval)?;
                    Ok(())
                }
                ("shutter_mask", Value::I64(mask)) if (0..=u8::MAX as i64).contains(mask) => Ok(()),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Spectral LMM5 hub property {key} is read-only or wrong type"),
                )),
            };
        }
        let _ = self.line_index_required(device)?;
        match (key, value) {
            ("enabled", Value::Bool(_)) => Ok(()),
            ("transmission", Value::Ratio(ratio)) if (0.0..=100.0).contains(&ratio.percent()) => {
                Ok(())
            }
            ("transmission", _) => Err(Error::new(
                ErrorCode::InvalidProperty,
                "Spectral LMM5 transmission must be Ratio percent in 0..=100",
            )),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Spectral LMM5 line property {key} is read-only or wrong type"),
            )),
        }
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: Value) -> Result<Value> {
        self.validate_write(device, key, &value)?;
        if device == self.hub {
            return match (key, value) {
                ("shutter_mask", Value::I64(mask)) => self.set_shutter_mask(mask as u8),
                ("trigger_in_enabled", Value::Bool(enabled)) => {
                    self.trigger_in_enabled = enabled;
                    self.configure_trigger_in()
                }
                ("trigger_in_count", Value::I64(count)) => {
                    self.trigger_in_count = count as u8;
                    self.configure_trigger_in()
                }
                ("trigger_in_cycle", Value::Bool(cycle)) => {
                    self.trigger_in_cycle = cycle;
                    self.configure_trigger_in()
                }
                ("trigger_out_enabled", Value::Bool(enabled)) => {
                    self.trigger_out_enabled = enabled;
                    self.configure_trigger_out()
                }
                ("trigger_out_clock", Value::Bool(clock)) => {
                    self.trigger_out_clock = clock;
                    self.configure_trigger_out()
                }
                ("trigger_out_interval", Value::TimeInterval(interval)) => {
                    self.trigger_out_interval_tenths_ms = trigger_interval_to_tenths_ms(interval)?;
                    self.configure_trigger_out()
                }
                _ => unreachable!("validated write"),
            };
        }
        let index = self.line_index_required(device)?;
        match (key, value) {
            ("enabled", Value::Bool(enabled)) => {
                self.set_line_enabled(index, enabled)?;
                Ok(Value::Bool(enabled))
            }
            ("transmission", Value::Ratio(ratio)) => self.set_line_transmission(index, ratio),
            _ => unreachable!("validated write"),
        }
    }

    fn transaction(&self, command: &str, completion_basis: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("command".into(), Value::String(command.into())),
            ("line_count".into(), Value::I64(self.lines.len() as i64)),
            (
                "shutter_mask".into(),
                Value::I64(self.shutter_mask() as i64),
            ),
            (
                "completion_basis".into(),
                Value::String(completion_basis.into()),
            ),
        ]))
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
            .filter(|sequence| self.line_index(sequence.device).is_some())
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequences(plan) {
            if sequence.property != "enabled" && sequence.property != "transmission" {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Spectral LMM5 timing sequences can only target enabled or transmission",
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
                "sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
            (
                "lines".into(),
                Value::List(
                    self.lines
                        .iter()
                        .enumerate()
                        .map(|(index, line)| {
                            Value::Map(BTreeMap::from([
                                ("line".into(), Value::I64(index as i64 + 1)),
                                (
                                    "participant".into(),
                                    Value::Bool(plan.participants.contains(&line.device)),
                                ),
                                ("enabled".into(), Value::Bool(line.enabled)),
                                (
                                    "transmission".into(),
                                    Value::Ratio(lmm5_to_ratio(line.transmission)),
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
            let line_index = self.line_index_required(device)?;
            let applied = self.write_property(device, &property, value)?;
            changed.insert(format!("line{}:{property}", line_index + 1), applied);
        }
        Ok(Value::Map(changed))
    }
}

impl Driver for Lmm5Driver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "spectral-lmm5-serial".into(),
            kind: "serial.ascii.hex".into(),
            metadata: BTreeMap::from([
                ("baud_rate".into(), Value::I64(protocol::BAUD as i64)),
                (
                    "serial_port".into(),
                    self.serial_port
                        .as_ref()
                        .map(|port| Value::String(port.clone()))
                        .unwrap_or(Value::Null),
                ),
                ("connected".into(), Value::Bool(self.connected)),
                ("send_terminator".into(), Value::String("CR".into())),
                (
                    "completion".into(),
                    Value::String("ACK plus status readback where documented".into()),
                ),
                (
                    "support_scope".into(),
                    Value::String(
                        "shutter/transmission/wavelength/trigger-profile command helpers".into(),
                    ),
                ),
            ]),
        }]
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        let mut descriptors = vec![DeviceDescriptor {
            id: self.hub,
            driver: self.id,
            label: "spectral-lmm5-hub".into(),
            vendor: Some("Spectral Applied Research".into()),
            model: Some(self.product.clone()),
            serial: Some(self.serial_number.clone()),
            kinds: vec![
                "hub".into(),
                "light.engine".into(),
                "serial.ascii.hex".into(),
            ],
            properties: vec![
                string_property("product", "Product", false),
                string_property("serial_number", "Serial number", false),
                string_property("protocol", "Protocol", false),
                integer_range_property("line_count", "Line count", false, 1, 8),
                integer_range_property("shutter_mask", "Shutter mask", true, 0, 255),
                bool_property("trigger_in_enabled", "Trigger in enabled", true),
                integer_range_property("trigger_in_count", "Trigger in count", true, 0, 255),
                bool_property("trigger_in_cycle", "Trigger in cycle", true),
                bool_property("trigger_out_enabled", "Trigger out enabled", true),
                bool_property("trigger_out_clock", "Trigger out clock", true),
                time_interval_property(
                    "trigger_out_interval",
                    "Trigger out interval",
                    true,
                    0.0,
                    u16::MAX as f64 / 10.0,
                ),
                map_property("last_transaction", "Last transaction", false),
            ],
            metadata: BTreeMap::from([(
                "source".into(),
                Value::String("Spectral LMM5 RS-232 software manual".into()),
            )]),
        }];
        descriptors.extend(
            self.lines
                .iter()
                .enumerate()
                .map(|(index, line)| DeviceDescriptor {
                    id: line.device,
                    driver: self.id,
                    label: line.label.clone(),
                    vendor: Some("Spectral Applied Research".into()),
                    model: Some(self.product.clone()),
                    serial: Some(format!("{}:line{}", self.serial_number, index + 1)),
                    kinds: vec![
                        "light.source".into(),
                        "laser.line".into(),
                        "shutter".into(),
                        "trigger.sink".into(),
                    ],
                    properties: vec![
                        integer_range_property("line", "Line", false, 1, 8),
                        wavelength_property("wavelength", "Wavelength", false),
                        bool_property("enabled", "Enabled", true),
                        ratio_property("transmission", "Transmission", true),
                    ],
                    metadata: BTreeMap::from([("line".into(), Value::I64(index as i64 + 1))]),
                }),
        );
        descriptors
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if self.line_index(device).is_some() {
            vec![
                capability(1, device, CapabilityKind::TriggerSink),
                capability(2, device, CapabilityKind::Dac),
            ]
        } else if device == self.hub {
            vec![capability(3, device, CapabilityKind::GenericCommand)]
        } else {
            Vec::new()
        }
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut physical_transactions = Vec::new();
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    self.validate_read(*device, key)?;
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("spectral lmm5 read {key}"),
                        Value::String(key.clone()),
                    ));
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("spectral lmm5 write {key}"),
                        value.clone(),
                    ));
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
                            "unknown Spectral LMM5 capability",
                        ));
                    };
                    if !capability.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "Spectral LMM5 {:?} expects {:?}, got {:?}",
                                capability.kind,
                                capability.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    match capability.kind {
                        CapabilityKind::TriggerSink if self.line_index(*device).is_some() => {
                            let _ = trigger_request(request)?;
                            physical_transactions.push(transaction(
                                self.resource,
                                "spectral lmm5 trigger sink",
                                Value::String(capability.kind.name().into()),
                            ));
                        }
                        CapabilityKind::Dac if self.line_index(*device).is_some() => {
                            let _ = dac_ratio_request(request)?;
                            physical_transactions.push(transaction(
                                self.resource,
                                "spectral lmm5 dac",
                                Value::String(capability.kind.name().into()),
                            ));
                        }
                        CapabilityKind::GenericCommand if *device == self.hub => {
                            let CapabilityRequest::GenericCommand(request) = request else {
                                return Err(Error::new(
                                    ErrorCode::InvalidCommand,
                                    "Spectral LMM5 GenericCommand expects a GenericCommandRequest",
                                ));
                            };
                            self.validate_generic_command(request)?;
                            physical_transactions.push(transaction(
                                self.resource,
                                "spectral lmm5 documented hub command",
                                Value::String(request.command.clone()),
                            ));
                        }
                        _ => {}
                    }
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        if write.device == self.hub || self.line_index(write.device).is_some() {
                            self.validate_write(write.device, &write.property, &write.value)?;
                        }
                    }
                    physical_transactions.push(transaction(
                        self.resource,
                        "spectral lmm5 state set",
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
                    let Some(capability) = self
                        .capabilities(device)
                        .into_iter()
                        .find(|candidate| candidate.id == capability)
                    else {
                        continue;
                    };
                    match capability.kind {
                        CapabilityKind::TriggerSink => last = self.trigger_sink(device, request)?,
                        CapabilityKind::Dac => last = self.dac(device, request)?,
                        CapabilityKind::GenericCommand if device == self.hub => {
                            let CapabilityRequest::GenericCommand(request) = request else {
                                return Err(Error::new(
                                    ErrorCode::InvalidCommand,
                                    "Spectral LMM5 GenericCommand expects a GenericCommandRequest",
                                ));
                            };
                            last = self.apply_generic_command(request)?;
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported Spectral LMM5 capability invocation",
                            ));
                        }
                    }
                }
                Command::ApplyStateSet(set) => {
                    let mut values = BTreeMap::new();
                    for write in set.writes {
                        if write.device == self.hub || self.line_index(write.device).is_some() {
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
                "spectral lmm5 timing arm summary",
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
                "spectral lmm5 timing start sequence",
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
                "spectral lmm5 timing stop sequence",
                Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "stop")),
                    ("changed".into(), changed),
                ])),
            )],
        })
    }
}

impl Lmm5Driver {
    fn validate_read(&self, device: DeviceId, key: &str) -> Result<()> {
        if device == self.hub {
            if matches!(
                key,
                "product"
                    | "serial_number"
                    | "protocol"
                    | "line_count"
                    | "shutter_mask"
                    | "trigger_in_enabled"
                    | "trigger_in_count"
                    | "trigger_in_cycle"
                    | "trigger_out_enabled"
                    | "trigger_out_clock"
                    | "trigger_out_interval"
                    | "last_transaction"
            ) {
                return Ok(());
            }
        } else if self.line_index(device).is_some()
            && matches!(key, "line" | "wavelength" | "enabled" | "transmission")
        {
            return Ok(());
        }
        Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unknown Spectral LMM5 property {key}"),
        ))
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lmm5GenericCommand {
    RefreshReadbacks,
    RefreshShutterStatus,
    RefreshWavelengths,
    ApplyTriggerIn,
    ApplyTriggerOut,
    ApplyTriggerProfiles,
}

fn lmm5_generic_command_kind(command: &str) -> Result<Lmm5GenericCommand> {
    match command {
        "refresh_readbacks" => Ok(Lmm5GenericCommand::RefreshReadbacks),
        "refresh_shutter_status" => Ok(Lmm5GenericCommand::RefreshShutterStatus),
        "refresh_wavelengths" => Ok(Lmm5GenericCommand::RefreshWavelengths),
        "apply_trigger_in" => Ok(Lmm5GenericCommand::ApplyTriggerIn),
        "apply_trigger_out" => Ok(Lmm5GenericCommand::ApplyTriggerOut),
        "apply_trigger_profiles" => Ok(Lmm5GenericCommand::ApplyTriggerProfiles),
        other => Err(Error::new(
            ErrorCode::Unsupported,
            format!(
                "Spectral LMM5 GenericCommand supports refresh_readbacks, refresh_shutter_status, refresh_wavelengths, apply_trigger_in, apply_trigger_out, and apply_trigger_profiles; got {other}"
            ),
        )),
    }
}

fn lmm5_generic_trigger_result(command: String, profile: &'static str, state: Value) -> Value {
    Value::Map(BTreeMap::from([
        ("command".into(), Value::String(command)),
        ("profile".into(), Value::String(profile.into())),
        ("state".into(), state),
        (
            "completion_basis".into(),
            Value::String("Spectral LMM5 trigger configure ACK".into()),
        ),
    ]))
}

fn trigger_request(request: &CapabilityRequest) -> Result<()> {
    match request {
        CapabilityRequest::Trigger(_) | CapabilityRequest::None => Ok(()),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            "Spectral LMM5 trigger sink expects TriggerRequest",
        )),
    }
}

fn dac_ratio_request(request: &CapabilityRequest) -> Result<Ratio> {
    match request {
        CapabilityRequest::Dac(request) => match &request.value {
            Value::Ratio(ratio) if (0.0..=100.0).contains(&ratio.percent()) => Ok(*ratio),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                "Spectral LMM5 DAC expects Ratio percent in 0..=100",
            )),
        },
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            "Spectral LMM5 DAC expects DacRequest",
        )),
    }
}

fn ratio_to_lmm5(ratio: Ratio) -> u16 {
    ((ratio.percent().clamp(0.0, 100.0) / 100.0) * protocol::MAX_TRANSMISSION as f64)
        .round()
        .clamp(0.0, protocol::MAX_TRANSMISSION as f64) as u16
}

fn lmm5_to_ratio(level: u16) -> Ratio {
    Ratio::from_percent(
        level.min(protocol::MAX_TRANSMISSION) as f64 * 100.0 / protocol::MAX_TRANSMISSION as f64,
    )
}

fn line_mask(line: usize) -> u8 {
    if (1..=protocol::MAX_LINES).contains(&line) {
        1 << (line - 1)
    } else {
        0
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
        sequenceable: matches!(key, "enabled" | "transmission"),
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

fn integer_range_property(
    key: &str,
    display_name: &str,
    writable: bool,
    min: i64,
    max: i64,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::I64,
        None,
        writable,
        Some(Range {
            min: Value::I64(min),
            max: Value::I64(max),
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

fn time_interval_property(
    key: &str,
    display_name: &str,
    writable: bool,
    min_ms: f64,
    max_ms: f64,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::TimeInterval,
        Some("ms"),
        writable,
        Some(Range {
            min: Value::TimeInterval(TimeInterval::from_milliseconds(min_ms)),
            max: Value::TimeInterval(TimeInterval::from_milliseconds(max_ms)),
        }),
    )
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

fn u8_prop(device: &DeviceConfig, key: &str) -> Option<u8> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => (*value).try_into().ok(),
        _ => None,
    }
}

fn usize_prop(device: &DeviceConfig, key: &str) -> Option<usize> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => (*value).try_into().ok(),
        _ => None,
    }
}

fn wavelength_prop(device: &DeviceConfig, key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::Wavelength(value)) => Some(value.nanometers()),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn ratio_prop(device: &DeviceConfig, key: &str) -> Option<Ratio> {
    match device.properties.get(key) {
        Some(Value::Ratio(value)) => Some(*value),
        _ => None,
    }
}

fn trigger_interval_prop(device: &DeviceConfig, key: &str) -> Option<u16> {
    match device.properties.get(key) {
        Some(Value::TimeInterval(interval)) => trigger_interval_to_tenths_ms(*interval).ok(),
        _ => None,
    }
}

fn trigger_interval_value(tenths_ms: u16) -> TimeInterval {
    TimeInterval::from_milliseconds(tenths_ms as f64 / 10.0)
}

fn trigger_interval_to_tenths_ms(interval: TimeInterval) -> Result<u16> {
    let tenths_ms = (interval.seconds() * 10_000.0).round();
    if tenths_ms.is_finite() && (0.0..=u16::MAX as f64).contains(&tenths_ms) {
        Ok(tenths_ms as u16)
    } else {
        Err(Error::new(
            ErrorCode::InvalidProperty,
            "Spectral LMM5 trigger_out_interval must be in 0..=6553.5 ms",
        ))
    }
}
