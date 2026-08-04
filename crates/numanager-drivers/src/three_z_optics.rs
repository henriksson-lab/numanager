use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::{ScriptedSerial, SerialIo};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const BAUD: u32 = 115_200;
    pub const DATA_BITS: u8 = 8;
    pub const STOP_BITS: u8 = 1;
    pub const PARITY: &str = "none";
    pub const SLAVE_ADDRESS: u8 = 0x01;
    pub const MODEL_ADDR: u16 = 0x01;
    pub const MODE_ADDR: u16 = 0x20;
    pub const DIRTY_BIT_ADDR: u16 = 0x21;
    pub const GLOBAL_INTENSITY_ADDR: u16 = 0x30;
    pub const GLOBAL_SWITCH_ADDR: u16 = 0x30;
    pub const CH1_INTENSITY_ADDR: u16 = 0x31;
    pub const CH1_SWITCH_ADDR: u16 = 0x31;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Mode {
        Global,
        Independent,
        Ttl,
    }

    impl Mode {
        pub fn code(self) -> u16 {
            match self {
                Mode::Global => 1,
                Mode::Independent => 2,
                Mode::Ttl => 3,
            }
        }

        pub fn name(self) -> &'static str {
            match self {
                Mode::Global => "Global",
                Mode::Independent => "Independent",
                Mode::Ttl => "TTL",
            }
        }

        pub fn from_code(code: u16) -> Option<Self> {
            match code {
                1 => Some(Mode::Global),
                2 => Some(Mode::Independent),
                3 => Some(Mode::Ttl),
                _ => None,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Command {
        ReadInputRegister { address: u16 },
        ReadHoldingRegister { address: u16 },
        ReadHoldingRegisters { start: u16, count: u16 },
        WriteHoldingRegister { address: u16, value: u16 },
        ReadCoil { address: u16 },
        ReadCoils { start: u16, count: u16 },
        WriteCoil { address: u16, enabled: bool },
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum Reply {
        Register(u16),
        Registers(Vec<u16>),
        Coil(bool),
        Coils(Vec<bool>),
        WriteAccepted,
    }

    pub fn encode(command: &Command) -> Vec<u8> {
        let mut frame = Vec::new();
        frame.push(SLAVE_ADDRESS);
        match *command {
            Command::ReadInputRegister { address } => {
                frame.push(0x04);
                push_u16(&mut frame, address);
                push_u16(&mut frame, 1);
            }
            Command::ReadHoldingRegister { address } => {
                frame.push(0x03);
                push_u16(&mut frame, address);
                push_u16(&mut frame, 1);
            }
            Command::ReadHoldingRegisters { start, count } => {
                frame.push(0x03);
                push_u16(&mut frame, start);
                push_u16(&mut frame, count);
            }
            Command::WriteHoldingRegister { address, value } => {
                frame.push(0x06);
                push_u16(&mut frame, address);
                push_u16(&mut frame, value);
            }
            Command::ReadCoil { address } => {
                frame.push(0x01);
                push_u16(&mut frame, address);
                push_u16(&mut frame, 1);
            }
            Command::ReadCoils { start, count } => {
                frame.push(0x01);
                push_u16(&mut frame, start);
                push_u16(&mut frame, count);
            }
            Command::WriteCoil { address, enabled } => {
                frame.push(0x05);
                push_u16(&mut frame, address);
                if enabled {
                    frame.extend_from_slice(&[0xff, 0x00]);
                } else {
                    frame.extend_from_slice(&[0x00, 0x00]);
                }
            }
        }
        push_crc(&mut frame);
        frame
    }

    pub fn expected_response_len(command: &Command) -> usize {
        match command {
            Command::ReadInputRegister { .. }
            | Command::ReadHoldingRegister { .. }
            | Command::ReadCoil { .. } => 7,
            Command::ReadHoldingRegisters { count, .. } => 5 + usize::from(*count) * 2,
            Command::ReadCoils { count, .. } => 5 + usize::from((*count + 7) / 8),
            Command::WriteHoldingRegister { .. } | Command::WriteCoil { .. } => 8,
        }
    }

    pub fn parse(command: &Command, response: &[u8]) -> Result<Reply> {
        if response.len() < 5 {
            return Err(Error::new(
                ErrorCode::Transport,
                "3Z Modbus response is too short",
            ));
        }
        if response[0] != SLAVE_ADDRESS {
            return Err(Error::new(
                ErrorCode::Transport,
                "3Z Modbus response has unexpected slave address",
            ));
        }
        validate_crc(response)?;
        if response[1] & 0x80 != 0 {
            return Err(Error::new(
                ErrorCode::Transport,
                format!(
                    "3Z Modbus exception response function 0x{:02x}",
                    response[1]
                ),
            ));
        }
        match command {
            Command::ReadInputRegister { .. } | Command::ReadHoldingRegister { .. } => {
                if response.len() != 7 || response[2] != 2 {
                    return Err(Error::new(
                        ErrorCode::Transport,
                        "invalid 3Z single-register response",
                    ));
                }
                Ok(Reply::Register(read_u16(response, 3)?))
            }
            Command::ReadHoldingRegisters { count, .. } => {
                let expected_bytes = (*count as u8).checked_mul(2).ok_or_else(|| {
                    Error::new(ErrorCode::InvalidCommand, "3Z register count too large")
                })?;
                if response[2] != expected_bytes {
                    return Err(Error::new(
                        ErrorCode::Transport,
                        "invalid 3Z multi-register byte count",
                    ));
                }
                let mut values = Vec::new();
                for index in 0..usize::from(*count) {
                    values.push(read_u16(response, 3 + index * 2)?);
                }
                Ok(Reply::Registers(values))
            }
            Command::ReadCoil { .. } => {
                if response.len() != 6 || response[2] != 1 {
                    return Err(Error::new(
                        ErrorCode::Transport,
                        "invalid 3Z single-coil response",
                    ));
                }
                Ok(Reply::Coil(response[3] != 0))
            }
            Command::ReadCoils { count, .. } => {
                let byte_count = ((*count + 7) / 8) as usize;
                if usize::from(response[2]) != byte_count {
                    return Err(Error::new(
                        ErrorCode::Transport,
                        "invalid 3Z multi-coil byte count",
                    ));
                }
                let mut values = Vec::new();
                for index in 0..usize::from(*count) {
                    let byte = response[3 + index / 8];
                    values.push(((byte >> (index % 8)) & 0x01) != 0);
                }
                Ok(Reply::Coils(values))
            }
            Command::WriteHoldingRegister { .. } | Command::WriteCoil { .. } => {
                if response.len() != 8 {
                    return Err(Error::new(
                        ErrorCode::Transport,
                        "invalid 3Z write acknowledgement length",
                    ));
                }
                Ok(Reply::WriteAccepted)
            }
        }
    }

    pub fn crc16_modbus(data: &[u8]) -> u16 {
        let mut crc = 0xffff;
        for byte in data {
            crc ^= u16::from(*byte);
            for _ in 0..8 {
                if crc & 0x0001 != 0 {
                    crc >>= 1;
                    crc ^= 0xa001;
                } else {
                    crc >>= 1;
                }
            }
        }
        crc
    }

    fn push_u16(frame: &mut Vec<u8>, value: u16) {
        frame.push((value >> 8) as u8);
        frame.push((value & 0xff) as u8);
    }

    fn push_crc(frame: &mut Vec<u8>) {
        let crc = crc16_modbus(frame);
        frame.push((crc & 0xff) as u8);
        frame.push((crc >> 8) as u8);
    }

    fn validate_crc(frame: &[u8]) -> Result<()> {
        let Some((&crc_hi, rest)) = frame.split_last() else {
            return Err(Error::new(ErrorCode::Transport, "empty 3Z Modbus frame"));
        };
        let Some((&crc_lo, data)) = rest.split_last() else {
            return Err(Error::new(ErrorCode::Transport, "short 3Z Modbus frame"));
        };
        let expected = crc16_modbus(data);
        let actual = u16::from(crc_lo) | (u16::from(crc_hi) << 8);
        if actual == expected {
            Ok(())
        } else {
            Err(Error::new(
                ErrorCode::Transport,
                "3Z Modbus response CRC mismatch",
            ))
        }
    }

    fn read_u16(bytes: &[u8], index: usize) -> Result<u16> {
        let high = *bytes
            .get(index)
            .ok_or_else(|| Error::new(ErrorCode::Transport, "short 3Z register value"))?;
        let low = *bytes
            .get(index + 1)
            .ok_or_else(|| Error::new(ErrorCode::Transport, "short 3Z register value"))?;
        Ok((u16::from(high) << 8) | u16::from(low))
    }
}

#[derive(Debug, Clone)]
pub struct ThreeZConfiguredProbe {
    label: String,
    product: String,
    serial_number: String,
    serial_port: Option<String>,
    serial_timeout_ms: u64,
    connect_real_transport: bool,
    model_id: i64,
    mode: protocol::Mode,
    brightness_min: i64,
    brightness_max: i64,
    global_enabled: bool,
    global_intensity: Ratio,
    channel_labels: Vec<String>,
    wavelengths: Vec<Wavelength>,
    channel_enabled: Vec<bool>,
    channel_intensity: Vec<Ratio>,
    dirty_bit: bool,
}

pub struct ThreeZOpticsDiscovery {
    next_id: DriverId,
    probes: Vec<ThreeZConfiguredProbe>,
}

impl ThreeZOpticsDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![ThreeZConfiguredProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), "3z_optics" | "3z" | "3Z_Optics"))
            .map(ThreeZConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for ThreeZOpticsDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver: Box<dyn Driver> = if configured.connect_real_transport {
                    Box::new(ThreeZOpticsDriver::serial(id, configured)?)
                } else {
                    Box::new(ThreeZOpticsDriver::configured(id, configured))
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

impl ThreeZConfiguredProbe {
    fn fixture() -> Self {
        let labels = ["365", "470", "525", "635"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let wavelengths = [365.0, 470.0, 525.0, 635.0]
            .into_iter()
            .map(Wavelength::from_nanometers)
            .collect::<Vec<_>>();
        Self {
            label: "Configured 3Z Optics IRIS light source".into(),
            product: "3Z Optics IRIS LED light source".into(),
            serial_number: "3Z-CONFIG-0001".into(),
            serial_port: None,
            serial_timeout_ms: 500,
            connect_real_transport: false,
            model_id: 0,
            mode: protocol::Mode::Global,
            brightness_min: 0,
            brightness_max: 100,
            global_enabled: false,
            global_intensity: Ratio::from_percent(100.0),
            channel_enabled: vec![false; 4],
            channel_intensity: vec![Ratio::from_percent(100.0); 4],
            channel_labels: labels,
            wavelengths,
            dirty_bit: false,
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
        configured.serial_port = string_prop(device, "serial_port");
        configured.serial_timeout_ms =
            u64_prop(device, "serial_timeout_ms").unwrap_or(configured.serial_timeout_ms);
        configured.connect_real_transport =
            bool_prop(device, "connect").unwrap_or(configured.connect_real_transport);
        configured.model_id = i64_prop(device, "model_id").unwrap_or(configured.model_id);
        configured.mode = mode_prop(device, "mode").unwrap_or(configured.mode);
        configured.brightness_min =
            i64_prop(device, "brightness_min").unwrap_or(configured.brightness_min);
        configured.brightness_max =
            i64_prop(device, "brightness_max").unwrap_or(configured.brightness_max);
        if configured.brightness_max < configured.brightness_min {
            std::mem::swap(
                &mut configured.brightness_min,
                &mut configured.brightness_max,
            );
        }
        configured.global_enabled =
            bool_prop(device, "enabled").unwrap_or(configured.global_enabled);
        configured.global_intensity =
            ratio_prop(device, "global_intensity").unwrap_or(configured.global_intensity);

        let channel_count = usize_prop(device, "channel_count")
            .unwrap_or_else(|| configured.channel_labels.len())
            .clamp(1, 16);
        configured.channel_labels = (0..channel_count)
            .map(|index| {
                string_prop(device, &format!("channel_{}_label", index + 1))
                    .unwrap_or_else(|| format!("CH{}", index + 1))
            })
            .collect();
        configured.wavelengths = (0..channel_count)
            .map(|index| {
                wavelength_prop(device, &format!("channel_{}_wavelength", index + 1))
                    .unwrap_or_else(|| Wavelength::from_nanometers(0.0))
            })
            .collect();
        configured.channel_enabled = (0..channel_count)
            .map(|index| {
                bool_prop(device, &format!("channel_{}_enabled", index + 1)).unwrap_or(false)
            })
            .collect();
        configured.channel_intensity = (0..channel_count)
            .map(|index| {
                ratio_prop(device, &format!("channel_{}_intensity", index + 1))
                    .unwrap_or_else(|| Ratio::from_percent(configured.brightness_max as f64))
            })
            .collect();
        validate_config(&configured)?;
        Ok(configured)
    }

    fn brightness_percent(&self, value: u16) -> Ratio {
        Ratio::from_percent(f64::from(value))
    }

    fn brightness_scalar(&self, value: Ratio) -> u16 {
        value
            .percent()
            .round()
            .clamp(self.brightness_min as f64, self.brightness_max as f64) as u16
    }
}

pub struct ThreeZOpticsDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    channels: Vec<DeviceId>,
    configured: ThreeZConfiguredProbe,
    last_transaction: Value,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    connected: bool,
}

impl ThreeZOpticsDriver {
    pub fn configured(id: DriverId, configured: ThreeZConfiguredProbe) -> Self {
        Self::new(id, configured, Box::new(ScriptedSerial::new()), false)
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: ThreeZConfiguredProbe) -> Result<Self> {
        let port_name = configured.serial_port.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "3Z Optics config requires serial_port when connect is true",
            )
        })?;
        let serial = Box::new(numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(port_name, protocol::BAUD)
                .timeout(Duration::from_millis(1)),
        )?);
        let mut driver = Self::new(id, configured, serial, true);
        driver.refresh_identity()?;
        driver.refresh_readbacks()?;
        Ok(driver)
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _configured: ThreeZConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "3Z Optics real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(
        id: DriverId,
        configured: ThreeZConfiguredProbe,
        serial: Box<dyn SerialIo>,
        connected: bool,
    ) -> Self {
        let base = id.0 * 1000 + 690;
        let channel_count = configured.channel_labels.len();
        Self {
            id,
            resource: ResourceId(NodeId(base)),
            hub: DeviceId(NodeId(base + 1)),
            channels: (0..channel_count)
                .map(|index| DeviceId(NodeId(base + 2 + index as u64)))
                .collect(),
            configured,
            last_transaction: Value::Map(BTreeMap::new()),
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            connected,
        }
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn owns_device(&self, device: DeviceId) -> bool {
        device == self.hub || self.channel_index(device).is_some()
    }

    fn channel_index(&self, device: DeviceId) -> Option<usize> {
        self.channels.iter().position(|id| *id == device)
    }

    fn transact(&mut self, command: protocol::Command, action: &str) -> Result<protocol::Reply> {
        let request = protocol::encode(&command);
        let expected = protocol::expected_response_len(&command);
        let reply = if self.connected {
            self.serial.write(&request)?;
            let response = self.read_exact_response(expected)?;
            protocol::parse(&command, &response)?
        } else {
            self.configured_reply(&command)?
        };
        self.last_transaction = Value::Map(BTreeMap::from([
            ("action".into(), Value::String(action.into())),
            ("live_serial".into(), Value::Bool(self.connected)),
            (
                "request_bytes".into(),
                Value::ByteCount(ByteCount::new(request.len() as u64)),
            ),
            (
                "expected_response_bytes".into(),
                Value::ByteCount(ByteCount::new(expected as u64)),
            ),
            ("function".into(), Value::I64(i64::from(request[1]))),
        ]));
        Ok(reply)
    }

    fn configured_reply(&mut self, command: &protocol::Command) -> Result<protocol::Reply> {
        match *command {
            protocol::Command::ReadInputRegister {
                address: protocol::MODEL_ADDR,
            } => Ok(protocol::Reply::Register(self.configured.model_id as u16)),
            protocol::Command::ReadHoldingRegister {
                address: protocol::MODE_ADDR,
            } => Ok(protocol::Reply::Register(self.configured.mode.code())),
            protocol::Command::ReadHoldingRegister {
                address: protocol::GLOBAL_INTENSITY_ADDR,
            } => Ok(protocol::Reply::Register(
                self.configured
                    .brightness_scalar(self.configured.global_intensity),
            )),
            protocol::Command::ReadHoldingRegisters { start, count }
                if start == protocol::CH1_INTENSITY_ADDR =>
            {
                Ok(protocol::Reply::Registers(
                    self.configured
                        .channel_intensity
                        .iter()
                        .take(usize::from(count))
                        .map(|value| self.configured.brightness_scalar(*value))
                        .collect(),
                ))
            }
            protocol::Command::ReadCoil {
                address: protocol::DIRTY_BIT_ADDR,
            } => Ok(protocol::Reply::Coil(self.configured.dirty_bit)),
            protocol::Command::ReadCoil {
                address: protocol::GLOBAL_SWITCH_ADDR,
            } => Ok(protocol::Reply::Coil(self.configured.global_enabled)),
            protocol::Command::ReadCoils { start, count } if start == protocol::CH1_SWITCH_ADDR => {
                Ok(protocol::Reply::Coils(
                    self.configured
                        .channel_enabled
                        .iter()
                        .take(usize::from(count))
                        .copied()
                        .collect(),
                ))
            }
            protocol::Command::WriteHoldingRegister {
                address: protocol::MODE_ADDR,
                value,
            } => {
                if let Some(mode) = protocol::Mode::from_code(value) {
                    self.configured.mode = mode;
                }
                Ok(protocol::Reply::WriteAccepted)
            }
            protocol::Command::WriteHoldingRegister {
                address: protocol::GLOBAL_INTENSITY_ADDR,
                value,
            } => {
                self.configured.global_intensity = self.configured.brightness_percent(value);
                Ok(protocol::Reply::WriteAccepted)
            }
            protocol::Command::WriteHoldingRegister { address, value }
                if address >= protocol::CH1_INTENSITY_ADDR =>
            {
                let index = usize::from(address - protocol::CH1_INTENSITY_ADDR);
                if let Some(slot) = self.configured.channel_intensity.get_mut(index) {
                    *slot = Ratio::from_percent(f64::from(value));
                }
                Ok(protocol::Reply::WriteAccepted)
            }
            protocol::Command::WriteCoil {
                address: protocol::GLOBAL_SWITCH_ADDR,
                enabled,
            } => {
                self.configured.global_enabled = enabled;
                Ok(protocol::Reply::WriteAccepted)
            }
            protocol::Command::WriteCoil { address, enabled }
                if address >= protocol::CH1_SWITCH_ADDR =>
            {
                let index = usize::from(address - protocol::CH1_SWITCH_ADDR);
                if let Some(slot) = self.configured.channel_enabled.get_mut(index) {
                    *slot = enabled;
                }
                Ok(protocol::Reply::WriteAccepted)
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "3Z configured local model does not support command",
            )),
        }
    }

    fn read_exact_response(&mut self, expected: usize) -> Result<Vec<u8>> {
        let deadline = Instant::now() + Duration::from_millis(self.configured.serial_timeout_ms);
        let mut response = Vec::new();
        while response.len() < expected {
            response.extend(self.serial.read_available()?);
            if Instant::now() >= deadline {
                break;
            }
            if response.len() < expected {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        if response.len() < expected {
            return Err(Error::new(
                ErrorCode::Transport,
                "timed out waiting for 3Z Modbus response",
            ));
        }
        response.truncate(expected);
        Ok(response)
    }

    fn refresh_identity(&mut self) -> Result<Value> {
        let reply = self.transact(
            protocol::Command::ReadInputRegister {
                address: protocol::MODEL_ADDR,
            },
            "refresh_identity",
        )?;
        let protocol::Reply::Register(model) = reply else {
            return Err(Error::new(ErrorCode::Transport, "invalid 3Z model reply"));
        };
        self.configured.model_id = i64::from(model);
        self.emit_property(self.hub, "model_id", Value::I64(self.configured.model_id));
        Ok(Value::I64(self.configured.model_id))
    }

    fn refresh_readbacks(&mut self) -> Result<Value> {
        let mode = self.read_mode()?;
        let dirty = self.read_dirty_bit()?;
        match mode {
            protocol::Mode::Global => {
                let global = self.read_global_switch()?;
                let intensity = self.read_global_intensity()?;
                self.emit_property(self.hub, "enabled", Value::Bool(global));
                self.emit_property(self.hub, "global_intensity", Value::Ratio(intensity));
            }
            protocol::Mode::Independent | protocol::Mode::Ttl => {
                self.read_channel_switches()?;
                self.read_channel_intensities()?;
            }
        }
        self.emit_property(self.hub, "mode", Value::String(mode.name().into()));
        self.emit_property(self.hub, "dirty", Value::Bool(dirty));
        Ok(self.summary("refresh_readbacks"))
    }

    fn poll_dirty(&mut self) -> Result<Value> {
        if self.read_dirty_bit()? {
            self.refresh_readbacks()
        } else {
            Ok(self.summary("poll_dirty"))
        }
    }

    fn read_mode(&mut self) -> Result<protocol::Mode> {
        let reply = self.transact(
            protocol::Command::ReadHoldingRegister {
                address: protocol::MODE_ADDR,
            },
            "read_mode",
        )?;
        let protocol::Reply::Register(value) = reply else {
            return Err(Error::new(ErrorCode::Transport, "invalid 3Z mode reply"));
        };
        let mode = protocol::Mode::from_code(value)
            .ok_or_else(|| Error::new(ErrorCode::Transport, "unknown 3Z mode value"))?;
        self.configured.mode = mode;
        Ok(mode)
    }

    fn read_dirty_bit(&mut self) -> Result<bool> {
        let reply = self.transact(
            protocol::Command::ReadCoil {
                address: protocol::DIRTY_BIT_ADDR,
            },
            "read_dirty_bit",
        )?;
        let protocol::Reply::Coil(value) = reply else {
            return Err(Error::new(
                ErrorCode::Transport,
                "invalid 3Z dirty-bit reply",
            ));
        };
        self.configured.dirty_bit = value;
        Ok(value)
    }

    fn read_global_switch(&mut self) -> Result<bool> {
        let reply = self.transact(
            protocol::Command::ReadCoil {
                address: protocol::GLOBAL_SWITCH_ADDR,
            },
            "read_global_switch",
        )?;
        let protocol::Reply::Coil(value) = reply else {
            return Err(Error::new(ErrorCode::Transport, "invalid 3Z switch reply"));
        };
        self.configured.global_enabled = value;
        Ok(value)
    }

    fn read_global_intensity(&mut self) -> Result<Ratio> {
        let reply = self.transact(
            protocol::Command::ReadHoldingRegister {
                address: protocol::GLOBAL_INTENSITY_ADDR,
            },
            "read_global_intensity",
        )?;
        let protocol::Reply::Register(value) = reply else {
            return Err(Error::new(
                ErrorCode::Transport,
                "invalid 3Z intensity reply",
            ));
        };
        let ratio = self.configured.brightness_percent(value);
        self.configured.global_intensity = ratio;
        Ok(ratio)
    }

    fn read_channel_switches(&mut self) -> Result<()> {
        let reply = self.transact(
            protocol::Command::ReadCoils {
                start: protocol::CH1_SWITCH_ADDR,
                count: self.channels.len() as u16,
            },
            "read_channel_switches",
        )?;
        let protocol::Reply::Coils(values) = reply else {
            return Err(Error::new(ErrorCode::Transport, "invalid 3Z switch reply"));
        };
        for (index, value) in values.into_iter().enumerate() {
            if index < self.configured.channel_enabled.len() {
                self.configured.channel_enabled[index] = value;
                self.emit_property(self.channels[index], "enabled", Value::Bool(value));
                self.emit_property(self.channels[index], "selected", Value::Bool(value));
            }
        }
        Ok(())
    }

    fn read_channel_intensities(&mut self) -> Result<()> {
        let reply = self.transact(
            protocol::Command::ReadHoldingRegisters {
                start: protocol::CH1_INTENSITY_ADDR,
                count: self.channels.len() as u16,
            },
            "read_channel_intensities",
        )?;
        let protocol::Reply::Registers(values) = reply else {
            return Err(Error::new(
                ErrorCode::Transport,
                "invalid 3Z intensity reply",
            ));
        };
        for (index, value) in values.into_iter().enumerate() {
            if index < self.configured.channel_intensity.len() {
                let intensity = Ratio::from_percent(f64::from(value));
                self.configured.channel_intensity[index] = intensity;
                self.emit_property(self.channels[index], "intensity", Value::Ratio(intensity));
            }
        }
        Ok(())
    }

    fn set_mode(&mut self, mode: protocol::Mode) -> Result<()> {
        self.transact(
            protocol::Command::WriteHoldingRegister {
                address: protocol::MODE_ADDR,
                value: mode.code(),
            },
            "set_mode",
        )?;
        self.configured.mode = mode;
        self.emit_property(self.hub, "mode", Value::String(mode.name().into()));
        if self.connected {
            let _ = self.refresh_readbacks()?;
        }
        Ok(())
    }

    fn set_global_enabled(&mut self, enabled: bool) -> Result<()> {
        self.configured.global_enabled = enabled;
        if self.configured.mode == protocol::Mode::Global {
            self.transact(
                protocol::Command::WriteCoil {
                    address: protocol::GLOBAL_SWITCH_ADDR,
                    enabled,
                },
                "set_global_enabled",
            )?;
        } else {
            self.apply_channel_switches()?;
        }
        self.emit_property(self.hub, "enabled", Value::Bool(enabled));
        Ok(())
    }

    fn set_global_intensity(&mut self, value: Ratio) -> Result<()> {
        self.configured.global_intensity = value;
        if self.configured.mode == protocol::Mode::Global {
            let scalar = self.configured.brightness_scalar(value);
            self.transact(
                protocol::Command::WriteHoldingRegister {
                    address: protocol::GLOBAL_INTENSITY_ADDR,
                    value: scalar,
                },
                "set_global_intensity",
            )?;
        }
        self.emit_property(self.hub, "global_intensity", Value::Ratio(value));
        Ok(())
    }

    fn set_channel_enabled(&mut self, index: usize, enabled: bool) -> Result<()> {
        self.configured.channel_enabled[index] = enabled;
        let output_enabled = enabled && self.configured.global_enabled;
        self.transact(
            protocol::Command::WriteCoil {
                address: protocol::CH1_SWITCH_ADDR + index as u16,
                enabled: output_enabled,
            },
            "set_channel_enabled",
        )?;
        let device = self.channels[index];
        self.emit_property(device, "enabled", Value::Bool(enabled));
        self.emit_property(device, "selected", Value::Bool(enabled));
        Ok(())
    }

    fn set_channel_intensity(&mut self, index: usize, value: Ratio) -> Result<()> {
        self.configured.channel_intensity[index] = value;
        let scalar = self.configured.brightness_scalar(value);
        self.transact(
            protocol::Command::WriteHoldingRegister {
                address: protocol::CH1_INTENSITY_ADDR + index as u16,
                value: scalar,
            },
            "set_channel_intensity",
        )?;
        self.emit_property(self.channels[index], "intensity", Value::Ratio(value));
        Ok(())
    }

    fn apply_channel_switches(&mut self) -> Result<()> {
        for index in 0..self.channels.len() {
            let enabled = self.configured.global_enabled && self.configured.channel_enabled[index];
            self.transact(
                protocol::Command::WriteCoil {
                    address: protocol::CH1_SWITCH_ADDR + index as u16,
                    enabled,
                },
                "apply_channel_switches",
            )?;
        }
        Ok(())
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "product" => Ok(Value::String(self.configured.product.clone())),
                "serial_number" => Ok(Value::String(self.configured.serial_number.clone())),
                "serial_port" => Ok(Value::String(
                    self.configured.serial_port.clone().unwrap_or_default(),
                )),
                "connected" => Ok(Value::Bool(self.connected)),
                "serial_timeout" => Ok(Value::TimeInterval(TimeInterval::from_milliseconds(
                    self.configured.serial_timeout_ms as f64,
                ))),
                "model_id" => Ok(Value::I64(self.configured.model_id)),
                "mode" => Ok(Value::String(self.configured.mode.name().into())),
                "brightness_min" => Ok(Value::I64(self.configured.brightness_min)),
                "brightness_max" => Ok(Value::I64(self.configured.brightness_max)),
                "enabled" => Ok(Value::Bool(self.configured.global_enabled)),
                "global_intensity" => Ok(Value::Ratio(self.configured.global_intensity)),
                "dirty" => Ok(Value::Bool(self.configured.dirty_bit)),
                "last_transaction" => Ok(self.last_transaction.clone()),
                _ => invalid_property("unknown 3Z hub property", key),
            };
        }
        let index = self
            .channel_index(device)
            .ok_or_else(|| Error::new(ErrorCode::InvalidProperty, "unknown 3Z device"))?;
        match key {
            "enabled" | "selected" => Ok(Value::Bool(self.configured.channel_enabled[index])),
            "intensity" => Ok(Value::Ratio(self.configured.channel_intensity[index])),
            "wavelength" => Ok(Value::Wavelength(self.configured.wavelengths[index])),
            "label" => Ok(Value::String(self.configured.channel_labels[index].clone())),
            _ => invalid_property("unknown 3Z channel property", key),
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
                    | "model_id"
                    | "mode"
                    | "brightness_min"
                    | "brightness_max"
                    | "enabled"
                    | "global_intensity"
                    | "dirty"
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
        invalid_property("unknown 3Z property", key)
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        if device == self.hub {
            return match (key, value) {
                ("enabled", Value::Bool(_)) => Ok(()),
                ("global_intensity", Value::Ratio(value)) if self.ratio_ok(*value) => Ok(()),
                ("mode", Value::String(value)) if mode(value).is_some() => Ok(()),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("3Z hub property {key} is read-only or wrong type"),
                )),
            };
        }
        if self.channel_index(device).is_some() {
            return match (key, value) {
                ("enabled" | "selected", Value::Bool(_)) => Ok(()),
                ("intensity", Value::Ratio(value)) if self.ratio_ok(*value) => Ok(()),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("3Z channel property {key} is read-only or wrong type"),
                )),
            };
        }
        Err(Error::new(ErrorCode::InvalidProperty, "unknown 3Z device"))
    }

    fn ratio_ok(&self, value: Ratio) -> bool {
        let percent = value.percent();
        percent >= self.configured.brightness_min as f64
            && percent <= self.configured.brightness_max as f64
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: Value) -> Result<Value> {
        self.validate_write(device, key, &value)?;
        if device == self.hub {
            return match (key, value) {
                ("enabled", Value::Bool(enabled)) => {
                    self.set_global_enabled(enabled)?;
                    Ok(Value::Bool(enabled))
                }
                ("global_intensity", Value::Ratio(value)) => {
                    self.set_global_intensity(value)?;
                    Ok(Value::Ratio(value))
                }
                ("mode", Value::String(value)) => {
                    let mode = mode(&value).expect("validated 3Z mode");
                    self.set_mode(mode)?;
                    Ok(Value::String(mode.name().into()))
                }
                _ => unreachable!("validated 3Z hub write"),
            };
        }
        let index = self.channel_index(device).expect("validated 3Z channel");
        match (key, value) {
            ("enabled" | "selected", Value::Bool(enabled)) => {
                self.set_channel_enabled(index, enabled)?;
                Ok(Value::Bool(enabled))
            }
            ("intensity", Value::Ratio(value)) => {
                self.set_channel_intensity(index, value)?;
                Ok(Value::Ratio(value))
            }
            _ => unreachable!("validated 3Z channel write"),
        }
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
                        "3Z GenericCommand refresh commands do not accept params",
                    ));
                }
                match request.command.as_str() {
                    "refresh_identity" => self.refresh_identity(),
                    "refresh_readbacks" => self.refresh_readbacks(),
                    "poll_dirty" => self.poll_dirty(),
                    _ => Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "3Z GenericCommand supports refresh_identity, refresh_readbacks, and poll_dirty",
                    )),
                }
            }
            (CapabilityKind::GenericCommand, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "3Z GenericCommand expects GenericCommandRequest",
            )),
            _ => Err(Error::new(
                ErrorCode::InvalidCommand,
                "3Z capability request kind does not match",
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
                        "3Z timing sequences can only target hub enabled/global_intensity or channel enabled/selected/intensity",
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
                "sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
            (
                "mode".into(),
                Value::String(self.configured.mode.name().into()),
            ),
            (
                "enabled".into(),
                Value::Bool(self.configured.global_enabled),
            ),
            (
                "global_intensity".into(),
                Value::Ratio(self.configured.global_intensity),
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
            changed.insert(format!("{}:{property}", device.0 .0), applied);
        }
        Ok(Value::Map(changed))
    }

    fn summary(&self, command: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("command".into(), Value::String(command.into())),
            ("model_id".into(), Value::I64(self.configured.model_id)),
            (
                "mode".into(),
                Value::String(self.configured.mode.name().into()),
            ),
            ("dirty".into(), Value::Bool(self.configured.dirty_bit)),
            (
                "enabled".into(),
                Value::Bool(self.configured.global_enabled),
            ),
            (
                "global_intensity".into(),
                Value::Ratio(self.configured.global_intensity),
            ),
        ]))
    }
}

impl Driver for ThreeZOpticsDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "3z-optics-serial".into(),
            kind: "serial.modbus_rtu".into(),
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
                    "slave_address".into(),
                    Value::I64(i64::from(protocol::SLAVE_ADDRESS)),
                ),
                ("connected".into(), Value::Bool(self.connected)),
                (
                    "evidence".into(),
                    Value::String("Micro-Manager 3Z_Optics adapter source".into()),
                ),
            ]),
        }]
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        let mut descriptors = vec![DeviceDescriptor {
            id: self.hub,
            driver: self.id,
            label: "3z-optics-hub".into(),
            vendor: Some("3Z Optics".into()),
            model: Some(self.configured.product.clone()),
            serial: Some(self.configured.serial_number.clone()),
            kinds: vec![
                "hub".into(),
                "light.engine".into(),
                "shutter".into(),
                "serial.modbus_rtu".into(),
            ],
            properties: vec![
                string_property("product", "Product", false),
                string_property("serial_number", "Serial number", false),
                string_property("serial_port", "Serial port", false),
                bool_property("connected", "Connected", false),
                time_property("serial_timeout", "Serial timeout", false),
                integer_property("model_id", "Model ID", false),
                enum_property("mode", "Mode", true, &["Global", "Independent", "TTL"]),
                integer_property("brightness_min", "Brightness min", false),
                integer_property("brightness_max", "Brightness max", false),
                bool_property("enabled", "Enabled", true),
                ratio_property(
                    "global_intensity",
                    "Global intensity",
                    true,
                    self.configured.brightness_min,
                    self.configured.brightness_max,
                ),
                bool_property("dirty", "Dirty", false),
                map_property("last_transaction", "Last transaction", false),
            ],
            metadata: source_metadata(),
        }];
        for (index, device) in self.channels.iter().copied().enumerate() {
            descriptors.push(DeviceDescriptor {
                id: device,
                driver: self.id,
                label: format!("3z-channel-{}", index + 1),
                vendor: Some("3Z Optics".into()),
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
                    ratio_property(
                        "intensity",
                        "Intensity",
                        true,
                        self.configured.brightness_min,
                        self.configured.brightness_max,
                    ),
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
            label: "3z-optics-serial".into(),
        });
        let _ = graph.insert_node(GraphNode {
            id: self.hub.0,
            kind: NodeKind::Hub,
            label: "3z-optics-hub".into(),
        });
        let _ = graph.insert_edge(GraphEdge {
            from: self.hub.0,
            to: self.resource.0,
            kind: EdgeKind::OwnsResource,
        });
        for (index, device) in self.channels.iter().enumerate() {
            let _ = graph.insert_node(GraphNode {
                id: device.0,
                kind: NodeKind::Device,
                label: format!("3z-channel-{}", index + 1),
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
                        format!("3Z read {key}"),
                        Value::String(key.clone()),
                    ));
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("3Z write {key}"),
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
                        return Err(Error::new(ErrorCode::Unsupported, "unknown 3Z capability"));
                    };
                    if !descriptor.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!("3Z {} request kind does not match", descriptor.kind.name()),
                        ));
                    }
                    if descriptor.kind == CapabilityKind::GenericCommand {
                        let CapabilityRequest::GenericCommand(request) = request else {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "3Z GenericCommand expects GenericCommandRequest",
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
                            "refresh_identity" | "refresh_readbacks" | "poll_dirty"
                        ) {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "3Z GenericCommand supports refresh_identity, refresh_readbacks, and poll_dirty",
                            ));
                        }
                        if !request.params.is_empty() {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "3Z GenericCommand refresh commands do not accept params",
                            ));
                        }
                    }
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("3Z {}", descriptor.kind.name()),
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
                        "3Z state set",
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
                "3Z timing arm summary",
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
                "3Z timing start sequence",
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
                "3Z timing stop sequence",
                Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "stop")),
                    ("changed".into(), changed),
                ])),
            )],
        })
    }
}

fn source_metadata() -> BTreeMap<String, Value> {
    BTreeMap::from([
        (
            "evidence".into(),
            Value::String("Micro-Manager 3Z_Optics adapter source; official product pages for serial-capable IRIS hardware".into()),
        ),
        (
            "support_level".into(),
            Value::String("source-backed opt-in Modbus RTU light-source control/readback".into()),
        ),
        (
            "hardware_validation".into(),
            Value::String("not_recorded".into()),
        ),
    ])
}

fn validate_config(configured: &ThreeZConfiguredProbe) -> Result<()> {
    if configured.brightness_min < 0 || configured.brightness_max > i64::from(u16::MAX) {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "3Z brightness range must fit a nonnegative u16 register",
        ));
    }
    if !configured
        .channel_intensity
        .iter()
        .chain(std::iter::once(&configured.global_intensity))
        .all(|ratio| {
            (configured.brightness_min as f64..=configured.brightness_max as f64)
                .contains(&ratio.percent())
        })
    {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "3Z intensity values must fit configured brightness_min..brightness_max",
        ));
    }
    Ok(())
}

fn mode(value: &str) -> Option<protocol::Mode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "global" => Some(protocol::Mode::Global),
        "independent" => Some(protocol::Mode::Independent),
        "ttl" => Some(protocol::Mode::Ttl),
        _ => None,
    }
}

fn mode_prop(device: &DeviceConfig, key: &str) -> Option<protocol::Mode> {
    string_prop(device, key).and_then(|value| mode(&value))
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

fn integer_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::I64, None, writable, None)
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

fn ratio_property(
    key: &str,
    display_name: &str,
    writable: bool,
    min: i64,
    max: i64,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Ratio,
        Some("percent"),
        writable,
        Some(Range {
            min: Value::Ratio(Ratio::from_percent(min as f64)),
            max: Value::Ratio(Ratio::from_percent(max as f64)),
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

fn usize_prop(device: &DeviceConfig, key: &str) -> Option<usize> {
    i64_prop(device, key).and_then(|value| usize::try_from(value).ok())
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
