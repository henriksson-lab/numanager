use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::SerialIo;
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TransportMode {
        Rtu,
        Tcp,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum RegisterKind {
        Coil,
        DiscreteInput,
        HoldingRegister,
        InputRegister,
    }

    impl RegisterKind {
        pub fn read_function(self) -> u8 {
            match self {
                RegisterKind::Coil => 0x01,
                RegisterKind::DiscreteInput => 0x02,
                RegisterKind::HoldingRegister => 0x03,
                RegisterKind::InputRegister => 0x04,
            }
        }

        pub fn write_single_function(self) -> Option<u8> {
            match self {
                RegisterKind::Coil => Some(0x05),
                RegisterKind::HoldingRegister => Some(0x06),
                RegisterKind::DiscreteInput | RegisterKind::InputRegister => None,
            }
        }

        pub fn label(self) -> &'static str {
            match self {
                RegisterKind::Coil => "coil",
                RegisterKind::DiscreteInput => "discrete_input",
                RegisterKind::HoldingRegister => "holding_register",
                RegisterKind::InputRegister => "input_register",
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct RegisterAddress(pub u16);

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct UnitId(pub u8);

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum ModbusRequest {
        Read {
            unit: UnitId,
            kind: RegisterKind,
            address: RegisterAddress,
            quantity: u16,
        },
        WriteSingleCoil {
            unit: UnitId,
            address: RegisterAddress,
            value: bool,
        },
        WriteSingleRegister {
            unit: UnitId,
            address: RegisterAddress,
            value: u16,
        },
        WriteMultipleCoils {
            unit: UnitId,
            address: RegisterAddress,
            values: Vec<bool>,
        },
        WriteMultipleRegisters {
            unit: UnitId,
            address: RegisterAddress,
            values: Vec<u16>,
        },
    }

    impl ModbusRequest {
        pub fn unit(&self) -> UnitId {
            match self {
                ModbusRequest::Read { unit, .. }
                | ModbusRequest::WriteSingleCoil { unit, .. }
                | ModbusRequest::WriteSingleRegister { unit, .. }
                | ModbusRequest::WriteMultipleCoils { unit, .. }
                | ModbusRequest::WriteMultipleRegisters { unit, .. } => *unit,
            }
        }

        pub fn pdu(&self) -> Result<Vec<u8>> {
            match self {
                ModbusRequest::Read {
                    kind,
                    address,
                    quantity,
                    ..
                } => {
                    if *quantity == 0 {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "Modbus read quantity must be nonzero",
                        ));
                    }
                    let mut pdu = vec![kind.read_function()];
                    pdu.extend_from_slice(&address.0.to_be_bytes());
                    pdu.extend_from_slice(&quantity.to_be_bytes());
                    Ok(pdu)
                }
                ModbusRequest::WriteSingleCoil { address, value, .. } => {
                    let mut pdu = vec![0x05];
                    pdu.extend_from_slice(&address.0.to_be_bytes());
                    pdu.extend_from_slice(if *value { &[0xff, 0x00] } else { &[0x00, 0x00] });
                    Ok(pdu)
                }
                ModbusRequest::WriteSingleRegister { address, value, .. } => {
                    let mut pdu = vec![0x06];
                    pdu.extend_from_slice(&address.0.to_be_bytes());
                    pdu.extend_from_slice(&value.to_be_bytes());
                    Ok(pdu)
                }
                ModbusRequest::WriteMultipleCoils {
                    address, values, ..
                } => {
                    if values.is_empty() || values.len() > u16::MAX as usize {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "Modbus coil write count is invalid",
                        ));
                    }
                    let byte_count = values.len().div_ceil(8);
                    if byte_count > u8::MAX as usize {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "Modbus coil write byte count is too large",
                        ));
                    }
                    let mut pdu = vec![0x0f];
                    pdu.extend_from_slice(&address.0.to_be_bytes());
                    pdu.extend_from_slice(&(values.len() as u16).to_be_bytes());
                    pdu.push(byte_count as u8);
                    pdu.extend(pack_bits(values));
                    Ok(pdu)
                }
                ModbusRequest::WriteMultipleRegisters {
                    address, values, ..
                } => {
                    if values.is_empty() || values.len() > 123 {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "Modbus register write count is invalid",
                        ));
                    }
                    let byte_count = values.len() * 2;
                    let mut pdu = vec![0x10];
                    pdu.extend_from_slice(&address.0.to_be_bytes());
                    pdu.extend_from_slice(&(values.len() as u16).to_be_bytes());
                    pdu.push(byte_count as u8);
                    for value in values {
                        pdu.extend_from_slice(&value.to_be_bytes());
                    }
                    Ok(pdu)
                }
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ModbusFrame {
        pub mode: TransportMode,
        pub transaction_id: Option<u16>,
        pub unit: UnitId,
        pub pdu: Vec<u8>,
    }

    pub fn encode_rtu(request: &ModbusRequest) -> Result<Vec<u8>> {
        let mut frame = vec![request.unit().0];
        frame.extend(request.pdu()?);
        let crc = crc16_modbus(&frame);
        frame.extend_from_slice(&crc.to_le_bytes());
        Ok(frame)
    }

    pub fn encode_tcp(transaction_id: u16, request: &ModbusRequest) -> Result<Vec<u8>> {
        let pdu = request.pdu()?;
        let length = (pdu.len() + 1) as u16;
        let mut frame = Vec::with_capacity(7 + pdu.len());
        frame.extend_from_slice(&transaction_id.to_be_bytes());
        frame.extend_from_slice(&0u16.to_be_bytes());
        frame.extend_from_slice(&length.to_be_bytes());
        frame.push(request.unit().0);
        frame.extend(pdu);
        Ok(frame)
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ModbusResponse {
        pub mode: TransportMode,
        pub transaction_id: Option<u16>,
        pub unit: UnitId,
        pub pdu: Vec<u8>,
        pub raw: Vec<u8>,
    }

    pub fn drain_responses(
        mode: TransportMode,
        buffer: &mut Vec<u8>,
    ) -> Result<Vec<ModbusResponse>> {
        let mut responses = Vec::new();
        loop {
            let Some(response) = (match mode {
                TransportMode::Rtu => pop_rtu_response(buffer)?,
                TransportMode::Tcp => pop_tcp_response(buffer)?,
            }) else {
                break;
            };
            responses.push(response);
        }
        Ok(responses)
    }

    fn pop_rtu_response(buffer: &mut Vec<u8>) -> Result<Option<ModbusResponse>> {
        if buffer.len() < 5 {
            return Ok(None);
        }
        let function = buffer[1];
        let len = if function & 0x80 != 0 {
            5
        } else {
            match function {
                0x01..=0x04 => {
                    if buffer.len() < 3 {
                        return Ok(None);
                    }
                    3 + buffer[2] as usize + 2
                }
                0x05 | 0x06 | 0x0f | 0x10 => 8,
                _ => {
                    return Err(Error::new(
                        ErrorCode::Transport,
                        format!("unsupported Modbus RTU response function 0x{function:02x}"),
                    ));
                }
            }
        };
        if buffer.len() < len {
            return Ok(None);
        }
        let raw = buffer.drain(..len).collect::<Vec<_>>();
        let received = u16::from_le_bytes([raw[len - 2], raw[len - 1]]);
        let expected = crc16_modbus(&raw[..len - 2]);
        if received != expected {
            return Err(Error::new(
                ErrorCode::Transport,
                format!(
                    "Modbus RTU CRC mismatch: received 0x{received:04x}, expected 0x{expected:04x}"
                ),
            ));
        }
        Ok(Some(ModbusResponse {
            mode: TransportMode::Rtu,
            transaction_id: None,
            unit: UnitId(raw[0]),
            pdu: raw[1..len - 2].to_vec(),
            raw,
        }))
    }

    fn pop_tcp_response(buffer: &mut Vec<u8>) -> Result<Option<ModbusResponse>> {
        if buffer.len() < 7 {
            return Ok(None);
        }
        let transaction_id = u16::from_be_bytes([buffer[0], buffer[1]]);
        let protocol_id = u16::from_be_bytes([buffer[2], buffer[3]]);
        if protocol_id != 0 {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("invalid Modbus TCP protocol id {protocol_id}"),
            ));
        }
        let length = u16::from_be_bytes([buffer[4], buffer[5]]) as usize;
        if length == 0 {
            return Err(Error::new(
                ErrorCode::Transport,
                "invalid Modbus TCP length 0",
            ));
        }
        let frame_len = 6 + length;
        if buffer.len() < frame_len {
            return Ok(None);
        }
        let raw = buffer.drain(..frame_len).collect::<Vec<_>>();
        Ok(Some(ModbusResponse {
            mode: TransportMode::Tcp,
            transaction_id: Some(transaction_id),
            unit: UnitId(raw[6]),
            pdu: raw[7..].to_vec(),
            raw,
        }))
    }

    pub fn crc16_modbus(bytes: &[u8]) -> u16 {
        let mut crc = 0xffffu16;
        for byte in bytes {
            crc ^= *byte as u16;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xa001;
                } else {
                    crc >>= 1;
                }
            }
        }
        crc
    }

    pub fn pack_bits(values: &[bool]) -> Vec<u8> {
        let mut bytes = vec![0u8; values.len().div_ceil(8)];
        for (index, value) in values.iter().enumerate() {
            if *value {
                bytes[index / 8] |= 1 << (index % 8);
            }
        }
        bytes
    }

    pub fn read_coil_response(values: &[bool]) -> Vec<u8> {
        let packed = pack_bits(values);
        let mut pdu = vec![0x01, packed.len() as u8];
        pdu.extend(packed);
        pdu
    }

    pub fn read_register_response(values: &[u16]) -> Vec<u8> {
        let mut pdu = vec![0x03, (values.len() * 2) as u8];
        for value in values {
            pdu.extend_from_slice(&value.to_be_bytes());
        }
        pdu
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModbusFixtureResponseOrder {
    Fifo,
    Lifo,
}

#[derive(Debug, Clone)]
pub struct ModbusFixtureSerial {
    writes: Vec<Vec<u8>>,
    reads: VecDeque<Vec<u8>>,
    coils: BTreeMap<u16, bool>,
    registers: BTreeMap<u16, u16>,
    response_order: ModbusFixtureResponseOrder,
}

impl ModbusFixtureSerial {
    fn new(maps: &[ModbusPropertyMap]) -> Self {
        let mut serial = Self {
            writes: Vec::new(),
            reads: VecDeque::new(),
            coils: BTreeMap::from([(0, false)]),
            registers: BTreeMap::from([(1, 0), (2, 23)]),
            response_order: ModbusFixtureResponseOrder::Fifo,
        };
        for map in maps {
            match map.value_map {
                ModbusValueMap::Bool => {
                    serial.coils.entry(map.address.0).or_insert(false);
                }
                ModbusValueMap::U16 | ModbusValueMap::I16 => {
                    serial
                        .registers
                        .entry(map.address.0)
                        .or_insert(default_register(map));
                }
                ModbusValueMap::U32
                | ModbusValueMap::I32
                | ModbusValueMap::U64
                | ModbusValueMap::I64
                | ModbusValueMap::F32
                | ModbusValueMap::F64 => {
                    let registers = encode_default_registers(map);
                    for (offset, value) in registers.into_iter().enumerate() {
                        serial
                            .registers
                            .entry(map.address.0 + offset as u16)
                            .or_insert(value);
                    }
                }
            }
        }
        serial
    }

    pub fn tcp_lifo(maps: &[ModbusPropertyMap]) -> Self {
        let mut serial = Self::new(maps);
        serial.response_order = ModbusFixtureResponseOrder::Lifo;
        serial
    }

    fn enqueue_rtu_response(&mut self, unit: u8, pdu: Vec<u8>) {
        let mut frame = vec![unit];
        frame.extend(pdu);
        let crc = protocol::crc16_modbus(&frame);
        frame.extend_from_slice(&crc.to_le_bytes());
        self.enqueue_response(frame);
    }

    fn enqueue_tcp_response(&mut self, transaction_id: u16, unit: u8, pdu: Vec<u8>) {
        let length = (pdu.len() + 1) as u16;
        let mut frame = Vec::with_capacity(7 + pdu.len());
        frame.extend_from_slice(&transaction_id.to_be_bytes());
        frame.extend_from_slice(&0u16.to_be_bytes());
        frame.extend_from_slice(&length.to_be_bytes());
        frame.push(unit);
        frame.extend(pdu);
        self.enqueue_response(frame);
    }

    fn enqueue_response(&mut self, frame: Vec<u8>) {
        match self.response_order {
            ModbusFixtureResponseOrder::Fifo => self.reads.push_back(frame),
            ModbusFixtureResponseOrder::Lifo => self.reads.push_front(frame),
        }
    }

    fn handle_rtu_request(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() < 4 {
            return Ok(());
        }
        let len = bytes.len();
        let received = u16::from_le_bytes([bytes[len - 2], bytes[len - 1]]);
        let expected = protocol::crc16_modbus(&bytes[..len - 2]);
        if received != expected {
            self.enqueue_rtu_response(bytes[0], vec![bytes[1] | 0x80, 0x03]);
            return Ok(());
        }
        let unit = bytes[0];
        let function = bytes[1];
        match function {
            0x01 | 0x02 => {
                let address = u16::from_be_bytes([bytes[2], bytes[3]]);
                let quantity = u16::from_be_bytes([bytes[4], bytes[5]]);
                let values = (0..quantity)
                    .map(|offset| {
                        self.coils
                            .get(&(address + offset))
                            .copied()
                            .unwrap_or(false)
                    })
                    .collect::<Vec<_>>();
                let mut pdu = vec![function, values.len().div_ceil(8) as u8];
                pdu.extend(protocol::pack_bits(&values));
                self.enqueue_rtu_response(unit, pdu);
            }
            0x03 | 0x04 => {
                let address = u16::from_be_bytes([bytes[2], bytes[3]]);
                let quantity = u16::from_be_bytes([bytes[4], bytes[5]]);
                let mut pdu = vec![function, (quantity * 2) as u8];
                for offset in 0..quantity {
                    let value = self
                        .registers
                        .get(&(address + offset))
                        .copied()
                        .unwrap_or(0);
                    pdu.extend_from_slice(&value.to_be_bytes());
                }
                self.enqueue_rtu_response(unit, pdu);
            }
            0x05 => {
                let address = u16::from_be_bytes([bytes[2], bytes[3]]);
                let value = u16::from_be_bytes([bytes[4], bytes[5]]) == 0xff00;
                self.coils.insert(address, value);
                self.enqueue_rtu_response(unit, bytes[1..6].to_vec());
            }
            0x06 => {
                let address = u16::from_be_bytes([bytes[2], bytes[3]]);
                let value = u16::from_be_bytes([bytes[4], bytes[5]]);
                self.registers.insert(address, value);
                self.enqueue_rtu_response(unit, bytes[1..6].to_vec());
            }
            0x0f => {
                let address = u16::from_be_bytes([bytes[2], bytes[3]]);
                let quantity = u16::from_be_bytes([bytes[4], bytes[5]]);
                let byte_count = bytes[6] as usize;
                for offset in 0..quantity {
                    let byte = bytes[7 + offset as usize / 8];
                    let value = byte & (1 << (offset % 8)) != 0;
                    self.coils.insert(address + offset, value);
                }
                self.enqueue_rtu_response(unit, bytes[1..6].to_vec());
                if byte_count == 0 {
                    self.enqueue_rtu_response(unit, vec![function | 0x80, 0x03]);
                }
            }
            0x10 => {
                let address = u16::from_be_bytes([bytes[2], bytes[3]]);
                let quantity = u16::from_be_bytes([bytes[4], bytes[5]]);
                for offset in 0..quantity {
                    let base = 7 + offset as usize * 2;
                    let value = u16::from_be_bytes([bytes[base], bytes[base + 1]]);
                    self.registers.insert(address + offset, value);
                }
                self.enqueue_rtu_response(unit, bytes[1..6].to_vec());
            }
            _ => self.enqueue_rtu_response(unit, vec![function | 0x80, 0x01]),
        }
        Ok(())
    }

    fn handle_tcp_request(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() < 8 {
            return Ok(());
        }
        let transaction_id = u16::from_be_bytes([bytes[0], bytes[1]]);
        let protocol_id = u16::from_be_bytes([bytes[2], bytes[3]]);
        if protocol_id != 0 {
            return Ok(());
        }
        let length = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
        if bytes.len() < 6 + length || length == 0 {
            return Ok(());
        }
        let unit = bytes[6];
        let pdu = &bytes[7..6 + length];
        let function = pdu[0];
        match function {
            0x01 | 0x02 => {
                let address = u16::from_be_bytes([pdu[1], pdu[2]]);
                let quantity = u16::from_be_bytes([pdu[3], pdu[4]]);
                let values = (0..quantity)
                    .map(|offset| {
                        self.coils
                            .get(&(address + offset))
                            .copied()
                            .unwrap_or(false)
                    })
                    .collect::<Vec<_>>();
                let mut response = vec![function, values.len().div_ceil(8) as u8];
                response.extend(protocol::pack_bits(&values));
                self.enqueue_tcp_response(transaction_id, unit, response);
            }
            0x03 | 0x04 => {
                let address = u16::from_be_bytes([pdu[1], pdu[2]]);
                let quantity = u16::from_be_bytes([pdu[3], pdu[4]]);
                let mut response = vec![function, (quantity * 2) as u8];
                for offset in 0..quantity {
                    let value = self
                        .registers
                        .get(&(address + offset))
                        .copied()
                        .unwrap_or(0);
                    response.extend_from_slice(&value.to_be_bytes());
                }
                self.enqueue_tcp_response(transaction_id, unit, response);
            }
            0x05 => {
                let address = u16::from_be_bytes([pdu[1], pdu[2]]);
                let value = u16::from_be_bytes([pdu[3], pdu[4]]) == 0xff00;
                self.coils.insert(address, value);
                self.enqueue_tcp_response(transaction_id, unit, pdu[..5].to_vec());
            }
            0x06 => {
                let address = u16::from_be_bytes([pdu[1], pdu[2]]);
                let value = u16::from_be_bytes([pdu[3], pdu[4]]);
                self.registers.insert(address, value);
                self.enqueue_tcp_response(transaction_id, unit, pdu[..5].to_vec());
            }
            0x0f => {
                let address = u16::from_be_bytes([pdu[1], pdu[2]]);
                let quantity = u16::from_be_bytes([pdu[3], pdu[4]]);
                for offset in 0..quantity {
                    let byte = pdu[6 + offset as usize / 8];
                    let value = byte & (1 << (offset % 8)) != 0;
                    self.coils.insert(address + offset, value);
                }
                self.enqueue_tcp_response(transaction_id, unit, pdu[..5].to_vec());
            }
            0x10 => {
                let address = u16::from_be_bytes([pdu[1], pdu[2]]);
                let quantity = u16::from_be_bytes([pdu[3], pdu[4]]);
                for offset in 0..quantity {
                    let base = 6 + offset as usize * 2;
                    let value = u16::from_be_bytes([pdu[base], pdu[base + 1]]);
                    self.registers.insert(address + offset, value);
                }
                self.enqueue_tcp_response(transaction_id, unit, pdu[..5].to_vec());
            }
            _ => self.enqueue_tcp_response(transaction_id, unit, vec![function | 0x80, 0x01]),
        }
        Ok(())
    }
}

impl SerialIo for ModbusFixtureSerial {
    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.writes.push(bytes.to_vec());
        if looks_like_modbus_tcp(bytes) {
            self.handle_tcp_request(bytes)
        } else {
            self.handle_rtu_request(bytes)
        }
    }

    fn read_available(&mut self) -> Result<Vec<u8>> {
        Ok(self.reads.pop_front().unwrap_or_default())
    }
}

fn looks_like_modbus_tcp(bytes: &[u8]) -> bool {
    if bytes.len() < 8 || u16::from_be_bytes([bytes[2], bytes[3]]) != 0 {
        return false;
    }
    let length = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
    length > 0 && bytes.len() == 6 + length
}

pub struct ModbusTcpIo {
    stream: TcpStream,
}

impl ModbusTcpIo {
    pub fn connect(endpoint: &ModbusTcpEndpoint) -> Result<Self> {
        let address = (endpoint.host.as_str(), endpoint.port)
            .to_socket_addrs()
            .map_err(|error| Error::new(ErrorCode::Transport, error.to_string()))?
            .next()
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::Transport,
                    format!("could not resolve Modbus TCP host {}", endpoint.host),
                )
            })?;
        let stream = TcpStream::connect_timeout(
            &address,
            Duration::from_millis(endpoint.connect_timeout_ms),
        )
        .map_err(|error| Error::new(ErrorCode::Transport, error.to_string()))?;
        stream
            .set_nonblocking(true)
            .map_err(|error| Error::new(ErrorCode::Transport, error.to_string()))?;
        stream
            .set_nodelay(true)
            .map_err(|error| Error::new(ErrorCode::Transport, error.to_string()))?;
        Ok(Self { stream })
    }
}

impl SerialIo for ModbusTcpIo {
    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.stream
            .write_all(bytes)
            .map_err(|error| Error::new(ErrorCode::Transport, error.to_string()))
    }

    fn read_available(&mut self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            match self.stream.read(&mut chunk) {
                Ok(0) => {
                    if out.is_empty() {
                        return Err(Error::new(
                            ErrorCode::Transport,
                            "Modbus TCP connection closed",
                        ));
                    }
                    break;
                }
                Ok(count) => out.extend_from_slice(&chunk[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(Error::new(ErrorCode::Transport, error.to_string())),
            }
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModbusValueMap {
    Bool,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
    F32,
    F64,
}

impl ModbusValueMap {
    fn register_count(&self) -> u16 {
        match self {
            ModbusValueMap::Bool | ModbusValueMap::U16 | ModbusValueMap::I16 => 1,
            ModbusValueMap::U32 | ModbusValueMap::I32 | ModbusValueMap::F32 => 2,
            ModbusValueMap::U64 | ModbusValueMap::I64 | ModbusValueMap::F64 => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModbusEndian {
    Big,
    LittleWord,
    ByteSwap,
    LittleWordByteSwap,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModbusPropertyMap {
    pub key: String,
    pub display_name: String,
    pub kind: protocol::RegisterKind,
    pub address: protocol::RegisterAddress,
    pub value_map: ModbusValueMap,
    pub endian: ModbusEndian,
    pub scale: f64,
    pub offset: f64,
    pub quantity: Option<ModbusQuantity>,
    pub enum_values: BTreeMap<String, i64>,
    pub bit_mask: Option<u64>,
    pub bit_shift: u8,
    pub readable: bool,
    pub writable: bool,
    pub poll_interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModbusQuantity {
    TemperatureCelsius,
    PressureKilopascals,
    GasPercent,
    FlowMicrolitersPerMinute,
    RatioPercent,
    TimeMilliseconds,
    TimeMicroseconds,
}

impl ModbusPropertyMap {
    pub fn coil(key: &str, display_name: &str, address: u16, writable: bool) -> Self {
        Self {
            key: key.into(),
            display_name: display_name.into(),
            kind: protocol::RegisterKind::Coil,
            address: protocol::RegisterAddress(address),
            value_map: ModbusValueMap::Bool,
            endian: ModbusEndian::Big,
            scale: 1.0,
            offset: 0.0,
            quantity: None,
            enum_values: BTreeMap::new(),
            bit_mask: None,
            bit_shift: 0,
            readable: true,
            writable,
            poll_interval_ms: None,
        }
    }

    pub fn holding_u16(key: &str, display_name: &str, address: u16, writable: bool) -> Self {
        Self {
            key: key.into(),
            display_name: display_name.into(),
            kind: protocol::RegisterKind::HoldingRegister,
            address: protocol::RegisterAddress(address),
            value_map: ModbusValueMap::U16,
            endian: ModbusEndian::Big,
            scale: 1.0,
            offset: 0.0,
            quantity: None,
            enum_values: BTreeMap::new(),
            bit_mask: None,
            bit_shift: 0,
            readable: true,
            writable,
            poll_interval_ms: None,
        }
    }

    pub fn input_i16(key: &str, display_name: &str, address: u16) -> Self {
        Self {
            key: key.into(),
            display_name: display_name.into(),
            kind: protocol::RegisterKind::InputRegister,
            address: protocol::RegisterAddress(address),
            value_map: ModbusValueMap::I16,
            endian: ModbusEndian::Big,
            scale: 1.0,
            offset: 0.0,
            quantity: None,
            enum_values: BTreeMap::new(),
            bit_mask: None,
            bit_shift: 0,
            readable: true,
            writable: false,
            poll_interval_ms: None,
        }
    }

    fn with_endian(mut self, endian: ModbusEndian) -> Self {
        self.endian = endian;
        self
    }

    fn with_scale(mut self, scale: f64, offset: f64) -> Self {
        self.scale = scale;
        self.offset = offset;
        self
    }

    fn with_quantity(mut self, quantity: ModbusQuantity) -> Self {
        self.quantity = Some(quantity);
        self
    }

    fn with_bit_mask(mut self, bit_mask: u64) -> Self {
        self.bit_mask = Some(bit_mask);
        self.bit_shift = bit_mask.trailing_zeros().min(u8::MAX as u32) as u8;
        self
    }

    fn with_poll_interval_ms(mut self, interval_ms: u64) -> Self {
        self.poll_interval_ms = Some(interval_ms);
        self
    }

    fn with_enum<const N: usize>(mut self, entries: [(&str, i64); N]) -> Self {
        self.enum_values = entries
            .into_iter()
            .map(|(label, value)| (label.into(), value))
            .collect();
        self
    }
}

fn map_bool(
    key: &str,
    display_name: &str,
    kind: protocol::RegisterKind,
    address: u16,
    writable: bool,
    poll_interval_ms: Option<u64>,
) -> ModbusPropertyMap {
    ModbusPropertyMap {
        key: key.into(),
        display_name: display_name.into(),
        kind,
        address: protocol::RegisterAddress(address),
        value_map: ModbusValueMap::Bool,
        endian: ModbusEndian::Big,
        scale: 1.0,
        offset: 0.0,
        quantity: None,
        enum_values: BTreeMap::new(),
        bit_mask: None,
        bit_shift: 0,
        readable: true,
        writable,
        poll_interval_ms,
    }
}

fn map_register(
    key: &str,
    display_name: &str,
    kind: protocol::RegisterKind,
    address: u16,
    value_map: ModbusValueMap,
    writable: bool,
) -> ModbusPropertyMap {
    ModbusPropertyMap {
        key: key.into(),
        display_name: display_name.into(),
        kind,
        address: protocol::RegisterAddress(address),
        value_map,
        endian: ModbusEndian::Big,
        scale: 1.0,
        offset: 0.0,
        quantity: None,
        enum_values: BTreeMap::new(),
        bit_mask: None,
        bit_shift: 0,
        readable: true,
        writable,
        poll_interval_ms: None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModbusProbe {
    pub label: String,
    pub device_id: Option<DeviceId>,
    pub device_label: String,
    pub transport: protocol::TransportMode,
    pub unit: protocol::UnitId,
    pub tcp_endpoint: Option<ModbusTcpEndpoint>,
    pub rtu_endpoint: Option<ModbusRtuEndpoint>,
    pub connect_real_transport: bool,
    pub response_timeout_ms: u64,
    pub retry_count: u8,
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub maps: Vec<ModbusPropertyMap>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModbusTcpEndpoint {
    pub host: String,
    pub port: u16,
    pub connect_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModbusRtuEndpoint {
    pub port_name: String,
    pub baud_rate: u32,
    pub timeout_ms: u64,
}

impl ModbusProbe {
    pub fn configured_fixture() -> Self {
        Self {
            label: "Configured Modbus IO fixture".into(),
            device_id: None,
            device_label: "modbus-mapped-io".into(),
            transport: protocol::TransportMode::Rtu,
            unit: protocol::UnitId(1),
            tcp_endpoint: None,
            rtu_endpoint: None,
            connect_real_transport: false,
            response_timeout_ms: 1000,
            retry_count: 0,
            vendor: Some("Modbus".into()),
            model: Some("Mapped IO fixture".into()),
            maps: vec![
                ModbusPropertyMap::coil("enabled", "Enable coil", 0, true),
                ModbusPropertyMap::holding_u16("target_register", "Target register", 1, true),
                ModbusPropertyMap::input_i16("measured_register", "Measured register", 2),
            ],
        }
    }

    pub fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        if device.driver != "modbus" {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                format!("device {} is not a modbus config entry", device.label),
            ));
        }
        let transport = match string_prop(&device.properties, "transport").as_deref() {
            Some("tcp") => protocol::TransportMode::Tcp,
            Some("rtu") | None => protocol::TransportMode::Rtu,
            Some(other) => {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unsupported Modbus transport {other}"),
                ));
            }
        };
        let unit = int_prop(&device.properties, "unit_id")
            .or_else(|| int_prop(&device.properties, "unit"))
            .unwrap_or(1);
        if !(0..=u8::MAX as i64).contains(&unit) {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Modbus unit_id must fit in u8",
            ));
        }
        let maps = property_maps_from_config(&device.properties)?;
        if maps.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Modbus config must define at least one property map",
            ));
        }
        let tcp_endpoint = if transport == protocol::TransportMode::Tcp {
            let port = int_prop(&device.properties, "tcp_port")
                .or_else(|| int_prop(&device.properties, "port"))
                .unwrap_or(502);
            if !(1..=u16::MAX as i64).contains(&port) {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Modbus tcp_port must be 1..=65535",
                ));
            }
            let connect_timeout_ms =
                int_prop(&device.properties, "connect_timeout_ms").unwrap_or(1000);
            if connect_timeout_ms < 0 {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Modbus connect_timeout_ms must be non-negative",
                ));
            }
            Some(ModbusTcpEndpoint {
                host: string_prop(&device.properties, "tcp_host")
                    .or_else(|| string_prop(&device.properties, "host"))
                    .unwrap_or_else(|| "127.0.0.1".into()),
                port: port as u16,
                connect_timeout_ms: connect_timeout_ms as u64,
            })
        } else {
            None
        };
        let rtu_endpoint = if transport == protocol::TransportMode::Rtu {
            match string_prop(&device.properties, "serial_port")
                .or_else(|| string_prop(&device.properties, "port_name"))
            {
                Some(port_name) => {
                    let baud_rate = int_prop(&device.properties, "baud_rate")
                        .or_else(|| int_prop(&device.properties, "baud"))
                        .unwrap_or(9600);
                    if !(1..=u32::MAX as i64).contains(&baud_rate) {
                        return Err(Error::new(
                            ErrorCode::InvalidProperty,
                            "Modbus baud_rate must be 1..=u32::MAX",
                        ));
                    }
                    let timeout_ms = int_prop(&device.properties, "serial_timeout_ms").unwrap_or(1);
                    if timeout_ms < 0 {
                        return Err(Error::new(
                            ErrorCode::InvalidProperty,
                            "Modbus serial_timeout_ms must be non-negative",
                        ));
                    }
                    Some(ModbusRtuEndpoint {
                        port_name,
                        baud_rate: baud_rate as u32,
                        timeout_ms: timeout_ms as u64,
                    })
                }
                None => None,
            }
        } else {
            None
        };
        let response_timeout_ms =
            int_prop(&device.properties, "response_timeout_ms").unwrap_or(1000);
        if response_timeout_ms <= 0 {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Modbus response_timeout_ms must be positive",
            ));
        }
        let retry_count = int_prop(&device.properties, "retries")
            .or_else(|| int_prop(&device.properties, "retry_count"))
            .unwrap_or(0);
        if !(0..=u8::MAX as i64).contains(&retry_count) {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Modbus retries must fit in u8",
            ));
        }
        Ok(Self {
            label: string_prop(&device.properties, "candidate_label")
                .unwrap_or_else(|| format!("Configured Modbus {}", device.label)),
            device_id: Some(device.id),
            device_label: device.label.clone(),
            transport,
            unit: protocol::UnitId(unit as u8),
            tcp_endpoint,
            rtu_endpoint,
            connect_real_transport: bool_prop(&device.properties, "connect").unwrap_or(false)
                || bool_prop(&device.properties, "real_transport").unwrap_or(false),
            response_timeout_ms: response_timeout_ms as u64,
            retry_count: retry_count as u8,
            vendor: string_prop(&device.properties, "vendor").or_else(|| Some("Modbus".into())),
            model: string_prop(&device.properties, "model"),
            maps,
        })
    }
}

#[derive(Debug, Default)]
struct PartialMapConfig {
    display_name: Option<String>,
    kind: Option<protocol::RegisterKind>,
    address: Option<protocol::RegisterAddress>,
    value_map: Option<ModbusValueMap>,
    endian: Option<ModbusEndian>,
    scale: Option<f64>,
    offset: Option<f64>,
    quantity: Option<ModbusQuantity>,
    enum_values: BTreeMap<String, i64>,
    bit_mask: Option<u64>,
    bit_shift: Option<u8>,
    readable: Option<bool>,
    writable: Option<bool>,
    poll_interval_ms: Option<u64>,
}

fn property_maps_from_config(
    properties: &BTreeMap<String, Value>,
) -> Result<Vec<ModbusPropertyMap>> {
    let mut maps = match string_prop(properties, "map_profile").as_deref() {
        Some(profile) => builtin_modbus_map_profile(profile)?,
        None => Vec::new(),
    };
    let mut partials: BTreeMap<String, PartialMapConfig> = BTreeMap::new();
    for (key, value) in properties {
        let Some(rest) = key.strip_prefix("map.") else {
            continue;
        };
        let Some((name, field)) = rest.rsplit_once('.') else {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid Modbus map key {key}"),
            ));
        };
        let (name, enum_label) = if let Some((name, label)) = name.split_once(".enum.") {
            (name, Some(label))
        } else {
            (name, None)
        };
        let partial = partials.entry(name.into()).or_default();
        if let Some(label) = enum_label {
            if field != "value" {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Modbus enum key {key} must end with .value"),
                ));
            }
            partial
                .enum_values
                .insert(label.into(), value_as_i64(value, key)?);
            continue;
        }
        match field {
            "display_name" => partial.display_name = Some(value_as_string(value, key)?),
            "kind" => partial.kind = Some(parse_register_kind(&value_as_string(value, key)?)?),
            "address" => {
                partial.address = Some(protocol::RegisterAddress(value_as_u16(value, key)?))
            }
            "value" | "value_map" => {
                partial.value_map = Some(parse_value_map(&value_as_string(value, key)?)?)
            }
            "endian" => partial.endian = Some(parse_endian(&value_as_string(value, key)?)?),
            "scale" => partial.scale = Some(value_as_f64(value, key)?),
            "offset" => partial.offset = Some(value_as_f64(value, key)?),
            "quantity" => partial.quantity = Some(parse_quantity(&value_as_string(value, key)?)?),
            "bit_mask" => partial.bit_mask = Some(value_as_u64(value, key)?),
            "bit_shift" => partial.bit_shift = Some(value_as_u8(value, key)?),
            "readable" => partial.readable = Some(value_as_bool(value, key)?),
            "writable" => partial.writable = Some(value_as_bool(value, key)?),
            "poll_interval" => {
                partial.poll_interval_ms = Some(value_as_time_interval_ms(value, key)?)
            }
            "poll_interval_ms" => partial.poll_interval_ms = Some(value_as_u64(value, key)?),
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unsupported Modbus map field {field} in {key}"),
                ));
            }
        }
    }

    for map in partials.into_iter().map(|(key, partial)| {
        let kind = partial.kind.ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("Modbus map {key} is missing kind"),
            )
        })?;
        let address = partial.address.ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("Modbus map {key} is missing address"),
            )
        })?;
        let value_map = partial.value_map.unwrap_or(match kind {
            protocol::RegisterKind::Coil | protocol::RegisterKind::DiscreteInput => {
                ModbusValueMap::Bool
            }
            protocol::RegisterKind::HoldingRegister | protocol::RegisterKind::InputRegister => {
                ModbusValueMap::U16
            }
        });
        if matches!(
            kind,
            protocol::RegisterKind::Coil | protocol::RegisterKind::DiscreteInput
        ) && value_map != ModbusValueMap::Bool
        {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Modbus bit map {key} must use bool value mapping"),
            ));
        }
        let bit_shift = partial.bit_shift.unwrap_or_else(|| {
            partial
                .bit_mask
                .map(|mask| mask.trailing_zeros().min(u8::MAX as u32) as u8)
                .unwrap_or(0)
        });
        if let Some(mask) = partial.bit_mask {
            if mask == 0 {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Modbus bitfield map {key} has zero bit_mask"),
                ));
            }
            if value_map.register_count() != 1 {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Modbus bitfield map {key} must use a one-register value map"),
                ));
            }
            if mask > u16::MAX as u64 {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Modbus bitfield map {key} mask exceeds 16 bits"),
                ));
            }
            if partial.writable == Some(true) && kind != protocol::RegisterKind::HoldingRegister {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Modbus bitfield map {key} can only be writable on holding registers"),
                ));
            }
            let field_max = (mask >> bit_shift) as i64;
            for (label, value) in &partial.enum_values {
                if *value < 0 || *value > field_max {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        format!(
                            "Modbus enum value {label}={value} does not fit bitfield map {key}"
                        ),
                    ));
                }
            }
        }
        if partial.poll_interval_ms.unwrap_or(0) > 0 && partial.readable == Some(false) {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Modbus map {key} cannot poll a non-readable property"),
            ));
        }
        if partial.quantity.is_some() {
            if !partial.enum_values.is_empty() || partial.bit_mask.is_some() {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Modbus map {key} cannot combine quantity with enum or bitfield"),
                ));
            }
            if matches!(value_map, ModbusValueMap::Bool) {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Modbus map {key} cannot use bool value mapping with quantity"),
                ));
            }
        }
        Ok(ModbusPropertyMap {
            display_name: partial.display_name.unwrap_or_else(|| key.clone()),
            key,
            kind,
            address,
            value_map,
            endian: partial.endian.unwrap_or(ModbusEndian::Big),
            scale: partial.scale.unwrap_or(1.0),
            offset: partial.offset.unwrap_or(0.0),
            quantity: partial.quantity,
            enum_values: partial.enum_values,
            bit_mask: partial.bit_mask,
            bit_shift,
            readable: partial.readable.unwrap_or(true),
            writable: partial.writable.unwrap_or(
                partial.bit_mask.is_none()
                    && matches!(
                        kind,
                        protocol::RegisterKind::Coil | protocol::RegisterKind::HoldingRegister
                    ),
            ),
            poll_interval_ms: partial.poll_interval_ms,
        })
    }) {
        merge_property_map(&mut maps, map?);
    }
    Ok(maps)
}

fn merge_property_map(maps: &mut Vec<ModbusPropertyMap>, map: ModbusPropertyMap) {
    if let Some(existing) = maps.iter_mut().find(|existing| existing.key == map.key) {
        *existing = map;
    } else {
        maps.push(map);
    }
}

fn builtin_modbus_map_profile(profile: &str) -> Result<Vec<ModbusPropertyMap>> {
    match profile {
        "mapped_io_fixture" => Ok(mapped_io_fixture_profile()),
        "environment_controller_basic" => Ok(environment_controller_basic_profile()),
        "incubator_environment_basic" => Ok(incubator_environment_basic_profile()),
        "live_cell_chamber_basic" => Ok(live_cell_chamber_basic_profile()),
        "stage_top_incubation_chamber_basic" => Ok(stage_top_incubation_chamber_basic_profile()),
        "pressure_flow_controller_basic" => Ok(pressure_flow_controller_basic_profile()),
        "shutter_safety_io_basic" => Ok(shutter_safety_io_basic_profile()),
        "laser_safety_interlock_basic" => Ok(laser_safety_interlock_basic_profile()),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unknown Modbus map profile {other}"),
        )),
    }
}

fn mapped_io_fixture_profile() -> Vec<ModbusPropertyMap> {
    vec![
        map_bool(
            "enabled",
            "Enable coil",
            protocol::RegisterKind::Coil,
            0,
            true,
            None,
        ),
        map_register(
            "target_register",
            "Target register",
            protocol::RegisterKind::HoldingRegister,
            1,
            ModbusValueMap::U16,
            true,
        ),
        map_register(
            "target_u32",
            "Target u32 register",
            protocol::RegisterKind::HoldingRegister,
            4,
            ModbusValueMap::U32,
            true,
        ),
        map_register(
            "target_float",
            "Target float register",
            protocol::RegisterKind::HoldingRegister,
            8,
            ModbusValueMap::F32,
            true,
        )
        .with_endian(ModbusEndian::LittleWord),
        map_register(
            "target_u64",
            "Target u64 register",
            protocol::RegisterKind::HoldingRegister,
            20,
            ModbusValueMap::U64,
            true,
        )
        .with_endian(ModbusEndian::LittleWordByteSwap),
        map_register(
            "target_double",
            "Target double register",
            protocol::RegisterKind::HoldingRegister,
            24,
            ModbusValueMap::F64,
            true,
        )
        .with_endian(ModbusEndian::ByteSwap),
        map_register(
            "scaled_target",
            "Scaled target",
            protocol::RegisterKind::HoldingRegister,
            12,
            ModbusValueMap::U16,
            true,
        )
        .with_scale(0.1, -40.0),
        map_register(
            "mode",
            "Mode",
            protocol::RegisterKind::HoldingRegister,
            14,
            ModbusValueMap::U16,
            true,
        )
        .with_enum([("off", 0), ("manual", 1), ("auto", 2)]),
        map_register(
            "alarm_active",
            "Alarm active",
            protocol::RegisterKind::InputRegister,
            16,
            ModbusValueMap::U16,
            false,
        )
        .with_bit_mask(4)
        .with_poll_interval_ms(50),
        map_register(
            "cooling_enabled",
            "Cooling enabled",
            protocol::RegisterKind::HoldingRegister,
            18,
            ModbusValueMap::U16,
            true,
        )
        .with_bit_mask(8),
        map_register(
            "fan_mode",
            "Fan mode",
            protocol::RegisterKind::HoldingRegister,
            18,
            ModbusValueMap::U16,
            true,
        )
        .with_bit_mask(48)
        .with_enum([("off", 0), ("low", 1), ("boost", 2)]),
        map_register(
            "measured_register",
            "Measured register",
            protocol::RegisterKind::InputRegister,
            2,
            ModbusValueMap::I16,
            false,
        ),
    ]
}

fn environment_controller_basic_profile() -> Vec<ModbusPropertyMap> {
    vec![
        map_register(
            "temperature",
            "Temperature",
            protocol::RegisterKind::InputRegister,
            0,
            ModbusValueMap::I16,
            false,
        )
        .with_scale(0.1, 0.0)
        .with_quantity(ModbusQuantity::TemperatureCelsius)
        .with_poll_interval_ms(1000),
        map_register(
            "temperature_setpoint",
            "Temperature setpoint",
            protocol::RegisterKind::HoldingRegister,
            10,
            ModbusValueMap::I16,
            true,
        )
        .with_scale(0.1, 0.0)
        .with_quantity(ModbusQuantity::TemperatureCelsius),
        map_register(
            "co2",
            "CO2",
            protocol::RegisterKind::InputRegister,
            1,
            ModbusValueMap::U16,
            false,
        )
        .with_scale(0.01, 0.0)
        .with_quantity(ModbusQuantity::GasPercent)
        .with_poll_interval_ms(1000),
        map_register(
            "humidity",
            "Humidity",
            protocol::RegisterKind::InputRegister,
            2,
            ModbusValueMap::U16,
            false,
        )
        .with_scale(0.1, 0.0)
        .with_quantity(ModbusQuantity::RatioPercent)
        .with_poll_interval_ms(2000),
        map_register(
            "status",
            "Status",
            protocol::RegisterKind::InputRegister,
            20,
            ModbusValueMap::U16,
            false,
        )
        .with_poll_interval_ms(500),
        map_register(
            "alarm_active",
            "Alarm active",
            protocol::RegisterKind::InputRegister,
            20,
            ModbusValueMap::U16,
            false,
        )
        .with_bit_mask(1)
        .with_poll_interval_ms(500),
        map_register(
            "control_mode",
            "Control mode",
            protocol::RegisterKind::HoldingRegister,
            21,
            ModbusValueMap::U16,
            true,
        )
        .with_bit_mask(6)
        .with_enum([("off", 0), ("manual", 1), ("auto", 2)]),
        map_bool(
            "enabled",
            "Enabled",
            protocol::RegisterKind::Coil,
            0,
            true,
            None,
        ),
    ]
}

fn incubator_environment_basic_profile() -> Vec<ModbusPropertyMap> {
    vec![
        map_register(
            "chamber_temperature",
            "Chamber temperature",
            protocol::RegisterKind::InputRegister,
            0,
            ModbusValueMap::I16,
            false,
        )
        .with_scale(0.1, 0.0)
        .with_quantity(ModbusQuantity::TemperatureCelsius)
        .with_poll_interval_ms(1000),
        map_register(
            "chamber_temperature_setpoint",
            "Chamber temperature setpoint",
            protocol::RegisterKind::HoldingRegister,
            10,
            ModbusValueMap::I16,
            true,
        )
        .with_scale(0.1, 0.0)
        .with_quantity(ModbusQuantity::TemperatureCelsius),
        map_register(
            "co2",
            "CO2",
            protocol::RegisterKind::InputRegister,
            1,
            ModbusValueMap::U16,
            false,
        )
        .with_scale(0.01, 0.0)
        .with_quantity(ModbusQuantity::GasPercent)
        .with_poll_interval_ms(1000),
        map_register(
            "co2_setpoint",
            "CO2 setpoint",
            protocol::RegisterKind::HoldingRegister,
            11,
            ModbusValueMap::U16,
            true,
        )
        .with_scale(0.01, 0.0)
        .with_quantity(ModbusQuantity::GasPercent),
        map_register(
            "o2",
            "O2",
            protocol::RegisterKind::InputRegister,
            2,
            ModbusValueMap::U16,
            false,
        )
        .with_scale(0.01, 0.0)
        .with_quantity(ModbusQuantity::GasPercent)
        .with_poll_interval_ms(1000),
        map_register(
            "o2_setpoint",
            "O2 setpoint",
            protocol::RegisterKind::HoldingRegister,
            12,
            ModbusValueMap::U16,
            true,
        )
        .with_scale(0.01, 0.0)
        .with_quantity(ModbusQuantity::GasPercent),
        map_register(
            "relative_humidity",
            "Relative humidity",
            protocol::RegisterKind::InputRegister,
            3,
            ModbusValueMap::U16,
            false,
        )
        .with_scale(0.1, 0.0)
        .with_quantity(ModbusQuantity::RatioPercent)
        .with_poll_interval_ms(2000),
        map_bool(
            "heater_enabled",
            "Heater enabled",
            protocol::RegisterKind::Coil,
            0,
            true,
            None,
        ),
        map_bool(
            "gas_control_enabled",
            "Gas control enabled",
            protocol::RegisterKind::Coil,
            1,
            true,
            None,
        ),
        map_register(
            "status",
            "Status",
            protocol::RegisterKind::InputRegister,
            20,
            ModbusValueMap::U16,
            false,
        )
        .with_poll_interval_ms(500),
        map_register(
            "alarm_active",
            "Alarm active",
            protocol::RegisterKind::InputRegister,
            20,
            ModbusValueMap::U16,
            false,
        )
        .with_bit_mask(1)
        .with_poll_interval_ms(500),
        map_register(
            "door_open",
            "Door open",
            protocol::RegisterKind::InputRegister,
            20,
            ModbusValueMap::U16,
            false,
        )
        .with_bit_mask(2)
        .with_poll_interval_ms(500),
    ]
}

fn live_cell_chamber_basic_profile() -> Vec<ModbusPropertyMap> {
    vec![
        map_register(
            "sample_temperature",
            "Sample temperature",
            protocol::RegisterKind::InputRegister,
            0,
            ModbusValueMap::I16,
            false,
        )
        .with_scale(0.1, 0.0)
        .with_quantity(ModbusQuantity::TemperatureCelsius)
        .with_poll_interval_ms(1000),
        map_register(
            "sample_temperature_setpoint",
            "Sample temperature setpoint",
            protocol::RegisterKind::HoldingRegister,
            10,
            ModbusValueMap::I16,
            true,
        )
        .with_scale(0.1, 0.0)
        .with_quantity(ModbusQuantity::TemperatureCelsius),
        map_register(
            "lid_temperature",
            "Lid temperature",
            protocol::RegisterKind::InputRegister,
            1,
            ModbusValueMap::I16,
            false,
        )
        .with_scale(0.1, 0.0)
        .with_quantity(ModbusQuantity::TemperatureCelsius)
        .with_poll_interval_ms(1000),
        map_register(
            "lid_temperature_setpoint",
            "Lid temperature setpoint",
            protocol::RegisterKind::HoldingRegister,
            11,
            ModbusValueMap::I16,
            true,
        )
        .with_scale(0.1, 0.0)
        .with_quantity(ModbusQuantity::TemperatureCelsius),
        map_register(
            "co2",
            "CO2",
            protocol::RegisterKind::InputRegister,
            2,
            ModbusValueMap::U16,
            false,
        )
        .with_scale(0.01, 0.0)
        .with_quantity(ModbusQuantity::GasPercent)
        .with_poll_interval_ms(1000),
        map_register(
            "co2_setpoint",
            "CO2 setpoint",
            protocol::RegisterKind::HoldingRegister,
            12,
            ModbusValueMap::U16,
            true,
        )
        .with_scale(0.01, 0.0)
        .with_quantity(ModbusQuantity::GasPercent),
        map_register(
            "o2",
            "O2",
            protocol::RegisterKind::InputRegister,
            3,
            ModbusValueMap::U16,
            false,
        )
        .with_scale(0.01, 0.0)
        .with_quantity(ModbusQuantity::GasPercent)
        .with_poll_interval_ms(1000),
        map_register(
            "o2_setpoint",
            "O2 setpoint",
            protocol::RegisterKind::HoldingRegister,
            13,
            ModbusValueMap::U16,
            true,
        )
        .with_scale(0.01, 0.0)
        .with_quantity(ModbusQuantity::GasPercent),
        map_register(
            "humidity",
            "Humidity",
            protocol::RegisterKind::InputRegister,
            4,
            ModbusValueMap::U16,
            false,
        )
        .with_scale(0.1, 0.0)
        .with_quantity(ModbusQuantity::RatioPercent)
        .with_poll_interval_ms(2000),
        map_bool(
            "heater_enabled",
            "Heater enabled",
            protocol::RegisterKind::Coil,
            0,
            true,
            None,
        ),
        map_bool(
            "gas_enabled",
            "Gas enabled",
            protocol::RegisterKind::Coil,
            1,
            true,
            None,
        ),
        map_bool(
            "humidifier_enabled",
            "Humidifier enabled",
            protocol::RegisterKind::Coil,
            2,
            true,
            None,
        ),
        map_register(
            "control_mode",
            "Control mode",
            protocol::RegisterKind::HoldingRegister,
            20,
            ModbusValueMap::U16,
            true,
        )
        .with_bit_mask(3)
        .with_enum([("off", 0), ("manual", 1), ("closed_loop", 2)]),
        map_register(
            "status",
            "Status",
            protocol::RegisterKind::InputRegister,
            30,
            ModbusValueMap::U16,
            false,
        )
        .with_poll_interval_ms(500),
        map_register(
            "door_open",
            "Door open",
            protocol::RegisterKind::InputRegister,
            30,
            ModbusValueMap::U16,
            false,
        )
        .with_bit_mask(1)
        .with_poll_interval_ms(500),
        map_register(
            "condensation_alarm",
            "Condensation alarm",
            protocol::RegisterKind::InputRegister,
            30,
            ModbusValueMap::U16,
            false,
        )
        .with_bit_mask(2)
        .with_poll_interval_ms(500),
        map_register(
            "gas_fault",
            "Gas fault",
            protocol::RegisterKind::InputRegister,
            30,
            ModbusValueMap::U16,
            false,
        )
        .with_bit_mask(4)
        .with_poll_interval_ms(500),
    ]
}

fn stage_top_incubation_chamber_basic_profile() -> Vec<ModbusPropertyMap> {
    vec![
        map_register(
            "sample_temperature",
            "Sample temperature",
            protocol::RegisterKind::InputRegister,
            100,
            ModbusValueMap::I16,
            false,
        )
        .with_scale(0.01, 0.0)
        .with_quantity(ModbusQuantity::TemperatureCelsius)
        .with_poll_interval_ms(500),
        map_register(
            "sample_temperature_setpoint",
            "Sample temperature setpoint",
            protocol::RegisterKind::HoldingRegister,
            100,
            ModbusValueMap::I16,
            true,
        )
        .with_scale(0.01, 0.0)
        .with_quantity(ModbusQuantity::TemperatureCelsius),
        map_register(
            "lid_temperature",
            "Lid temperature",
            protocol::RegisterKind::InputRegister,
            101,
            ModbusValueMap::I16,
            false,
        )
        .with_scale(0.01, 0.0)
        .with_quantity(ModbusQuantity::TemperatureCelsius)
        .with_poll_interval_ms(500),
        map_register(
            "lid_temperature_setpoint",
            "Lid temperature setpoint",
            protocol::RegisterKind::HoldingRegister,
            101,
            ModbusValueMap::I16,
            true,
        )
        .with_scale(0.01, 0.0)
        .with_quantity(ModbusQuantity::TemperatureCelsius),
        map_register(
            "co2",
            "CO2",
            protocol::RegisterKind::InputRegister,
            102,
            ModbusValueMap::U16,
            false,
        )
        .with_scale(0.01, 0.0)
        .with_quantity(ModbusQuantity::GasPercent)
        .with_poll_interval_ms(1000),
        map_register(
            "co2_setpoint",
            "CO2 setpoint",
            protocol::RegisterKind::HoldingRegister,
            102,
            ModbusValueMap::U16,
            true,
        )
        .with_scale(0.01, 0.0)
        .with_quantity(ModbusQuantity::GasPercent),
        map_register(
            "relative_humidity",
            "Relative humidity",
            protocol::RegisterKind::InputRegister,
            103,
            ModbusValueMap::U16,
            false,
        )
        .with_scale(0.1, 0.0)
        .with_quantity(ModbusQuantity::RatioPercent)
        .with_poll_interval_ms(2000),
        map_register(
            "perfusion_flow",
            "Perfusion flow",
            protocol::RegisterKind::InputRegister,
            104,
            ModbusValueMap::U16,
            false,
        )
        .with_scale(0.1, 0.0)
        .with_quantity(ModbusQuantity::FlowMicrolitersPerMinute)
        .with_poll_interval_ms(250),
        map_register(
            "perfusion_flow_setpoint",
            "Perfusion flow setpoint",
            protocol::RegisterKind::HoldingRegister,
            104,
            ModbusValueMap::U16,
            true,
        )
        .with_scale(0.1, 0.0)
        .with_quantity(ModbusQuantity::FlowMicrolitersPerMinute),
        map_bool(
            "heater_enabled",
            "Heater enabled",
            protocol::RegisterKind::Coil,
            20,
            true,
            None,
        ),
        map_bool(
            "gas_enabled",
            "Gas enabled",
            protocol::RegisterKind::Coil,
            21,
            true,
            None,
        ),
        map_bool(
            "perfusion_enabled",
            "Perfusion enabled",
            protocol::RegisterKind::Coil,
            22,
            true,
            None,
        ),
        map_bool(
            "lid_closed",
            "Lid closed",
            protocol::RegisterKind::DiscreteInput,
            20,
            false,
            Some(100),
        ),
        map_bool(
            "reservoir_present",
            "Reservoir present",
            protocol::RegisterKind::DiscreteInput,
            21,
            false,
            Some(500),
        ),
        map_register(
            "control_mode",
            "Control mode",
            protocol::RegisterKind::HoldingRegister,
            120,
            ModbusValueMap::U16,
            true,
        )
        .with_bit_mask(7)
        .with_enum([
            ("off", 0),
            ("temperature", 1),
            ("gas", 2),
            ("closed_loop", 3),
            ("service", 4),
        ]),
        map_register(
            "status_word",
            "Status word",
            protocol::RegisterKind::InputRegister,
            130,
            ModbusValueMap::U16,
            false,
        )
        .with_poll_interval_ms(250),
        map_register(
            "temperature_alarm",
            "Temperature alarm",
            protocol::RegisterKind::InputRegister,
            130,
            ModbusValueMap::U16,
            false,
        )
        .with_bit_mask(1)
        .with_poll_interval_ms(250),
        map_register(
            "gas_alarm",
            "Gas alarm",
            protocol::RegisterKind::InputRegister,
            130,
            ModbusValueMap::U16,
            false,
        )
        .with_bit_mask(2)
        .with_poll_interval_ms(250),
        map_register(
            "perfusion_alarm",
            "Perfusion alarm",
            protocol::RegisterKind::InputRegister,
            130,
            ModbusValueMap::U16,
            false,
        )
        .with_bit_mask(4)
        .with_poll_interval_ms(250),
    ]
}

fn pressure_flow_controller_basic_profile() -> Vec<ModbusPropertyMap> {
    vec![
        map_register(
            "pressure",
            "Pressure",
            protocol::RegisterKind::InputRegister,
            0,
            ModbusValueMap::I16,
            false,
        )
        .with_scale(0.01, 0.0)
        .with_quantity(ModbusQuantity::PressureKilopascals)
        .with_poll_interval_ms(100),
        map_register(
            "pressure_setpoint",
            "Pressure setpoint",
            protocol::RegisterKind::HoldingRegister,
            10,
            ModbusValueMap::I16,
            true,
        )
        .with_scale(0.01, 0.0)
        .with_quantity(ModbusQuantity::PressureKilopascals),
        map_register(
            "flow",
            "Flow",
            protocol::RegisterKind::InputRegister,
            1,
            ModbusValueMap::U16,
            false,
        )
        .with_scale(0.1, 0.0)
        .with_quantity(ModbusQuantity::FlowMicrolitersPerMinute)
        .with_poll_interval_ms(100),
        map_register(
            "flow_setpoint",
            "Flow setpoint",
            protocol::RegisterKind::HoldingRegister,
            11,
            ModbusValueMap::U16,
            true,
        )
        .with_scale(0.1, 0.0)
        .with_quantity(ModbusQuantity::FlowMicrolitersPerMinute),
        map_register(
            "valve_position",
            "Valve position",
            protocol::RegisterKind::InputRegister,
            2,
            ModbusValueMap::U16,
            false,
        )
        .with_scale(0.1, 0.0)
        .with_quantity(ModbusQuantity::RatioPercent)
        .with_poll_interval_ms(250),
        map_register(
            "valve_setpoint",
            "Valve setpoint",
            protocol::RegisterKind::HoldingRegister,
            12,
            ModbusValueMap::U16,
            true,
        )
        .with_scale(0.1, 0.0)
        .with_quantity(ModbusQuantity::RatioPercent),
        map_bool(
            "pump_enabled",
            "Pump enabled",
            protocol::RegisterKind::Coil,
            0,
            true,
            None,
        ),
        map_bool(
            "valve_enabled",
            "Valve enabled",
            protocol::RegisterKind::Coil,
            1,
            true,
            None,
        ),
        map_register(
            "control_mode",
            "Control mode",
            protocol::RegisterKind::HoldingRegister,
            20,
            ModbusValueMap::U16,
            true,
        )
        .with_bit_mask(3)
        .with_enum([("off", 0), ("pressure", 1), ("flow", 2), ("manual", 3)]),
        map_register(
            "overpressure_fault",
            "Overpressure fault",
            protocol::RegisterKind::InputRegister,
            30,
            ModbusValueMap::U16,
            false,
        )
        .with_bit_mask(1)
        .with_poll_interval_ms(250),
    ]
}

fn shutter_safety_io_basic_profile() -> Vec<ModbusPropertyMap> {
    vec![
        map_bool(
            "shutter_open",
            "Shutter open",
            protocol::RegisterKind::Coil,
            0,
            true,
            None,
        ),
        map_bool(
            "ttl_output_enabled",
            "TTL output enabled",
            protocol::RegisterKind::Coil,
            1,
            true,
            None,
        ),
        map_bool(
            "interlock_closed",
            "Interlock closed",
            protocol::RegisterKind::DiscreteInput,
            0,
            false,
            Some(100),
        ),
        map_bool(
            "emergency_stop",
            "Emergency stop",
            protocol::RegisterKind::DiscreteInput,
            1,
            false,
            Some(100),
        ),
        map_bool(
            "shutter_open_feedback",
            "Shutter open feedback",
            protocol::RegisterKind::DiscreteInput,
            2,
            false,
            Some(100),
        ),
        map_register(
            "pulse_width",
            "Pulse width",
            protocol::RegisterKind::HoldingRegister,
            10,
            ModbusValueMap::U32,
            true,
        )
        .with_quantity(ModbusQuantity::TimeMicroseconds),
        map_register(
            "trigger_mode",
            "Trigger mode",
            protocol::RegisterKind::HoldingRegister,
            12,
            ModbusValueMap::U16,
            true,
        )
        .with_enum([
            ("software", 0),
            ("rising_edge", 1),
            ("falling_edge", 2),
            ("level", 3),
        ]),
        map_register(
            "fault_code",
            "Fault code",
            protocol::RegisterKind::InputRegister,
            20,
            ModbusValueMap::U16,
            false,
        )
        .with_poll_interval_ms(250),
        map_register(
            "fault_active",
            "Fault active",
            protocol::RegisterKind::InputRegister,
            20,
            ModbusValueMap::U16,
            false,
        )
        .with_bit_mask(1)
        .with_poll_interval_ms(250),
    ]
}

fn laser_safety_interlock_basic_profile() -> Vec<ModbusPropertyMap> {
    vec![
        map_bool(
            "emission_request",
            "Emission request",
            protocol::RegisterKind::Coil,
            0,
            true,
            None,
        ),
        map_bool(
            "shutter_open_request",
            "Shutter open request",
            protocol::RegisterKind::Coil,
            1,
            true,
            None,
        ),
        map_bool(
            "remote_enable",
            "Remote enable",
            protocol::RegisterKind::Coil,
            3,
            true,
            None,
        ),
        map_bool(
            "interlock_closed",
            "Interlock closed",
            protocol::RegisterKind::DiscreteInput,
            0,
            false,
            Some(50),
        ),
        map_bool(
            "key_switch_on",
            "Key switch on",
            protocol::RegisterKind::DiscreteInput,
            1,
            false,
            Some(50),
        ),
        map_bool(
            "shutter_open_feedback",
            "Shutter open feedback",
            protocol::RegisterKind::DiscreteInput,
            2,
            false,
            Some(50),
        ),
        map_bool(
            "emission_permitted",
            "Emission permitted",
            protocol::RegisterKind::DiscreteInput,
            3,
            false,
            Some(50),
        ),
        map_register(
            "status_word",
            "Status word",
            protocol::RegisterKind::InputRegister,
            20,
            ModbusValueMap::U16,
            false,
        )
        .with_poll_interval_ms(100),
        map_register(
            "fault_active",
            "Fault active",
            protocol::RegisterKind::InputRegister,
            20,
            ModbusValueMap::U16,
            false,
        )
        .with_bit_mask(1)
        .with_poll_interval_ms(100),
        map_register(
            "interlock_fault",
            "Interlock fault",
            protocol::RegisterKind::InputRegister,
            20,
            ModbusValueMap::U16,
            false,
        )
        .with_bit_mask(2)
        .with_poll_interval_ms(100),
        map_register(
            "overtemperature_fault",
            "Overtemperature fault",
            protocol::RegisterKind::InputRegister,
            20,
            ModbusValueMap::U16,
            false,
        )
        .with_bit_mask(4)
        .with_poll_interval_ms(100),
        map_register(
            "operation_mode",
            "Operation mode",
            protocol::RegisterKind::HoldingRegister,
            30,
            ModbusValueMap::U16,
            true,
        )
        .with_enum([("standby", 0), ("armed", 1), ("service", 2)]),
        map_register(
            "cdrh_delay",
            "CDRH delay",
            protocol::RegisterKind::HoldingRegister,
            31,
            ModbusValueMap::U32,
            true,
        )
        .with_quantity(ModbusQuantity::TimeMilliseconds),
    ]
}

fn parse_register_kind(value: &str) -> Result<protocol::RegisterKind> {
    match value {
        "coil" => Ok(protocol::RegisterKind::Coil),
        "discrete_input" => Ok(protocol::RegisterKind::DiscreteInput),
        "holding_register" => Ok(protocol::RegisterKind::HoldingRegister),
        "input_register" => Ok(protocol::RegisterKind::InputRegister),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unsupported Modbus register kind {other}"),
        )),
    }
}

fn parse_value_map(value: &str) -> Result<ModbusValueMap> {
    match value {
        "bool" => Ok(ModbusValueMap::Bool),
        "u16" => Ok(ModbusValueMap::U16),
        "i16" => Ok(ModbusValueMap::I16),
        "u32" => Ok(ModbusValueMap::U32),
        "i32" => Ok(ModbusValueMap::I32),
        "u64" => Ok(ModbusValueMap::U64),
        "i64" => Ok(ModbusValueMap::I64),
        "f32" => Ok(ModbusValueMap::F32),
        "f64" => Ok(ModbusValueMap::F64),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unsupported Modbus value map {other}"),
        )),
    }
}

fn parse_endian(value: &str) -> Result<ModbusEndian> {
    match value {
        "big" | "be" | "big_word" => Ok(ModbusEndian::Big),
        "little_word" | "word_swap" => Ok(ModbusEndian::LittleWord),
        "byte_swap" | "big_byte_swap" => Ok(ModbusEndian::ByteSwap),
        "little" | "le" | "little_byte" | "little_word_byte_swap" | "word_byte_swap" => {
            Ok(ModbusEndian::LittleWordByteSwap)
        }
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unsupported Modbus endian {other}"),
        )),
    }
}

fn parse_quantity(value: &str) -> Result<ModbusQuantity> {
    match value {
        "temperature_c" | "temperature_celsius" | "celsius" => {
            Ok(ModbusQuantity::TemperatureCelsius)
        }
        "pressure_kpa" | "kilopascals" | "kpa" => Ok(ModbusQuantity::PressureKilopascals),
        "gas_percent" | "concentration_percent" | "percent" => Ok(ModbusQuantity::GasPercent),
        "flow_ul_min" | "microliters_per_minute" | "ul_min" => {
            Ok(ModbusQuantity::FlowMicrolitersPerMinute)
        }
        "ratio_percent" | "relative_percent" | "duty_percent" => Ok(ModbusQuantity::RatioPercent),
        "time_ms" | "milliseconds" | "ms" => Ok(ModbusQuantity::TimeMilliseconds),
        "time_us" | "microseconds" | "us" => Ok(ModbusQuantity::TimeMicroseconds),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unsupported Modbus quantity {other}"),
        )),
    }
}

fn string_prop(properties: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    properties.get(key).and_then(|value| match value {
        Value::String(value) => Some(value.clone()),
        _ => None,
    })
}

fn int_prop(properties: &BTreeMap<String, Value>, key: &str) -> Option<i64> {
    properties.get(key).and_then(|value| match value {
        Value::I64(value) => Some(*value),
        _ => None,
    })
}

fn bool_prop(properties: &BTreeMap<String, Value>, key: &str) -> Option<bool> {
    properties.get(key).and_then(|value| match value {
        Value::Bool(value) => Some(*value),
        _ => None,
    })
}

fn value_as_string(value: &Value, key: &str) -> Result<String> {
    match value {
        Value::String(value) => Ok(value.clone()),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Modbus config key {key} must be a string"),
        )),
    }
}

fn value_as_bool(value: &Value, key: &str) -> Result<bool> {
    match value {
        Value::Bool(value) => Ok(*value),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Modbus config key {key} must be a bool"),
        )),
    }
}

fn value_as_i64(value: &Value, key: &str) -> Result<i64> {
    match value {
        Value::I64(value) => Ok(*value),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Modbus config key {key} must be an integer"),
        )),
    }
}

fn value_as_u64(value: &Value, key: &str) -> Result<u64> {
    match value {
        Value::I64(value) if *value >= 0 => Ok(*value as u64),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Modbus config key {key} must be a non-negative integer"),
        )),
    }
}

fn value_as_u8(value: &Value, key: &str) -> Result<u8> {
    match value {
        Value::I64(value) if (0..=u8::MAX as i64).contains(value) => Ok(*value as u8),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Modbus config key {key} must be a u8 integer"),
        )),
    }
}

fn value_as_f64(value: &Value, key: &str) -> Result<f64> {
    match value {
        Value::F64(value) if value.is_finite() => Ok(*value),
        Value::I64(value) => Ok(*value as f64),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Modbus config key {key} must be a finite number"),
        )),
    }
}

fn value_as_u16(value: &Value, key: &str) -> Result<u16> {
    match value {
        Value::I64(value) if (0..=u16::MAX as i64).contains(value) => Ok(*value as u16),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Modbus config key {key} must be a u16 integer"),
        )),
    }
}

fn value_as_time_interval_ms(value: &Value, key: &str) -> Result<u64> {
    match value {
        Value::TimeInterval(value) => {
            let microseconds = value.microseconds();
            if microseconds.is_finite() && microseconds >= 0.0 {
                Ok((microseconds * 1e-3).round() as u64)
            } else {
                Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Modbus config key {key} must be a non-negative TimeInterval"),
                ))
            }
        }
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Modbus config key {key} must be a non-negative TimeInterval"),
        )),
    }
}

pub struct ModbusDiscovery {
    next_id: DriverId,
    probes: Vec<ModbusProbe>,
}

impl ModbusDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![ModbusProbe::configured_fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| device.driver == "modbus")
            .map(ModbusProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for ModbusDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = probe.label.clone();
                let driver = if probe.connect_real_transport {
                    match probe.transport {
                        protocol::TransportMode::Rtu => {
                            Box::new(ModbusDriver::rtu_serial(id, probe)?) as Box<dyn Driver>
                        }
                        protocol::TransportMode::Tcp => {
                            Box::new(ModbusDriver::tcp(id, probe)?) as Box<dyn Driver>
                        }
                    }
                } else {
                    Box::new(ModbusDriver::fixture(id, probe)) as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

pub struct ModbusDriver {
    id: DriverId,
    resource: ResourceId,
    device: DeviceId,
    probe: ModbusProbe,
    values: BTreeMap<String, Value>,
    serial: Box<dyn SerialIo>,
    next_token: u64,
    next_transaction_id: u16,
    pending: VecDeque<DriverEvent>,
    rx_buffer: Vec<u8>,
    in_flight: VecDeque<PendingOperation>,
    poll_due: BTreeMap<String, Instant>,
}

#[derive(Debug, Clone)]
struct PendingOperation {
    token: DriverToken,
    actions: VecDeque<PendingAction>,
    last: Value,
    background: bool,
    transaction_id: Option<u16>,
    sent_at: Instant,
    retries_remaining: u8,
}

#[derive(Debug, Clone)]
struct PendingAction {
    request: protocol::ModbusRequest,
    kind: PendingActionKind,
}

#[derive(Debug, Clone)]
enum PendingActionKind {
    ReadProperty {
        key: String,
        map: ModbusPropertyMap,
    },
    WriteProperty {
        key: String,
        value: Value,
        aggregate: bool,
    },
    WriteBitfield {
        key: String,
        value: Value,
        map: ModbusPropertyMap,
        aggregate: bool,
        phase: BitfieldWritePhase,
    },
    Raw,
    PollProperty {
        key: String,
        map: ModbusPropertyMap,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BitfieldWritePhase {
    Read,
    Write,
}

impl ModbusDriver {
    pub fn fixture(id: DriverId, probe: ModbusProbe) -> Self {
        let values = fixture_values(&probe.maps);
        let serial = Box::new(ModbusFixtureSerial::new(&probe.maps));
        Self::new(id, probe, serial, values)
    }

    pub fn tcp(id: DriverId, probe: ModbusProbe) -> Result<Self> {
        if probe.transport != protocol::TransportMode::Tcp {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Modbus TCP driver requires a TCP probe",
            ));
        }
        let endpoint = probe.tcp_endpoint.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Modbus TCP probe is missing endpoint metadata",
            )
        })?;
        let values = fixture_values(&probe.maps);
        let serial = Box::new(ModbusTcpIo::connect(&endpoint)?);
        Ok(Self::new(id, probe, serial, values))
    }

    #[cfg(feature = "os-serial")]
    pub fn rtu_serial(id: DriverId, probe: ModbusProbe) -> Result<Self> {
        if probe.transport != protocol::TransportMode::Rtu {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Modbus RTU serial driver requires an RTU probe",
            ));
        }
        let endpoint = probe.rtu_endpoint.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Modbus RTU probe is missing serial_port metadata",
            )
        })?;
        let values = fixture_values(&probe.maps);
        let serial = Box::new(numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(endpoint.port_name, endpoint.baud_rate)
                .timeout(Duration::from_millis(endpoint.timeout_ms)),
        )?);
        Ok(Self::new(id, probe, serial, values))
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn rtu_serial(_id: DriverId, _probe: ModbusProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Modbus RTU real serial requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(
        id: DriverId,
        probe: ModbusProbe,
        serial: Box<dyn SerialIo>,
        values: BTreeMap<String, Value>,
    ) -> Self {
        let now = Instant::now();
        let poll_due = probe
            .maps
            .iter()
            .filter(|map| map.readable && map.poll_interval_ms.unwrap_or(0) > 0)
            .map(|map| (map.key.clone(), now))
            .collect();
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 901)),
            device: probe
                .device_id
                .unwrap_or(DeviceId(NodeId(id.0 * 1000 + 910))),
            probe,
            values,
            serial,
            next_token: 1,
            next_transaction_id: 1,
            pending: VecDeque::new(),
            rx_buffer: Vec::new(),
            in_flight: VecDeque::new(),
            poll_due,
        }
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn next_transaction_id(&mut self) -> u16 {
        let id = self.next_transaction_id;
        self.next_transaction_id = self.next_transaction_id.wrapping_add(1).max(1);
        id
    }

    fn pending_operation(
        &self,
        token: DriverToken,
        actions: VecDeque<PendingAction>,
        background: bool,
        transaction_id: Option<u16>,
    ) -> PendingOperation {
        PendingOperation {
            token,
            actions,
            last: Value::Null,
            background,
            transaction_id,
            sent_at: Instant::now(),
            retries_remaining: self.probe.retry_count,
        }
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        let mut metadata = BTreeMap::from([
            ("unit_id".into(), Value::I64(self.probe.unit.0 as i64)),
            (
                "transport".into(),
                Value::String(match self.probe.transport {
                    protocol::TransportMode::Rtu => "rtu".into(),
                    protocol::TransportMode::Tcp => "tcp".into(),
                }),
            ),
            (
                "mapping_count".into(),
                Value::I64(self.probe.maps.len() as i64),
            ),
            (
                "poll_intervals".into(),
                poll_interval_metadata(&self.probe.maps),
            ),
            (
                "real_transport".into(),
                Value::Bool(self.probe.connect_real_transport),
            ),
            (
                "connected".into(),
                Value::Bool(self.probe.connect_real_transport),
            ),
            (
                "response_timeout_ms".into(),
                Value::I64(self.probe.response_timeout_ms as i64),
            ),
            (
                "response_correlation".into(),
                Value::String(match self.probe.transport {
                    protocol::TransportMode::Rtu => "ordered-rtu-frame".into(),
                    protocol::TransportMode::Tcp => "mbap-transaction-id".into(),
                }),
            ),
            ("retries".into(), Value::I64(self.probe.retry_count as i64)),
        ]);
        if let Some(endpoint) = &self.probe.tcp_endpoint {
            metadata.insert("tcp_host".into(), Value::String(endpoint.host.clone()));
            metadata.insert("tcp_port".into(), Value::I64(endpoint.port as i64));
        }
        if let Some(endpoint) = &self.probe.rtu_endpoint {
            metadata.insert(
                "serial_port".into(),
                Value::String(endpoint.port_name.clone()),
            );
            metadata.insert("baud_rate".into(), Value::I64(endpoint.baud_rate as i64));
        }
        vec![DeviceDescriptor {
            id: self.device,
            driver: self.id,
            label: self.probe.device_label.clone(),
            vendor: self.probe.vendor.clone(),
            model: self.probe.model.clone(),
            serial: None,
            kinds: vec!["mapped.io".into(), "modbus".into()],
            properties: self.probe.maps.iter().map(property_schema).collect(),
            metadata,
        }]
    }

    fn map_for(&self, device: DeviceId, key: &str) -> Result<&ModbusPropertyMap> {
        if device != self.device {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown Modbus device",
            ));
        }
        self.probe
            .maps
            .iter()
            .find(|map| map.key == key)
            .ok_or_else(|| Error::new(ErrorCode::InvalidProperty, "unknown Modbus property"))
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        let map = self.map_for(device, key)?;
        if !map.writable {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Modbus property is read-only",
            ));
        }
        let expected = property_value_type(map);
        let actual = match value {
            Value::Bool(_) => ValueType::Bool,
            Value::I64(_) => ValueType::I64,
            Value::F64(_) => ValueType::F64,
            Value::Temperature(_) => ValueType::Temperature,
            Value::Pressure(_) => ValueType::Pressure,
            Value::GasConcentration(_) => ValueType::GasConcentration,
            Value::FlowRate(_) => ValueType::FlowRate,
            Value::Ratio(_) => ValueType::Ratio,
            Value::TimeInterval(_) => ValueType::TimeInterval,
            Value::String(_) => ValueType::String,
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Modbus mapped property value type is not supported by this map",
                ));
            }
        };
        if actual != expected {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Modbus property {key} expected {expected:?}"),
            ));
        }
        let _ = raw_value_for_property(map, value)?;
        Ok(())
    }

    fn request_for_read(&self, map: &ModbusPropertyMap) -> protocol::ModbusRequest {
        protocol::ModbusRequest::Read {
            unit: self.probe.unit,
            kind: map.kind,
            address: map.address,
            quantity: map.value_map.register_count(),
        }
    }

    fn request_for_write(
        &self,
        map: &ModbusPropertyMap,
        value: &Value,
    ) -> Result<protocol::ModbusRequest> {
        match map.kind {
            protocol::RegisterKind::Coil => {
                let Value::Bool(v) = value else {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Modbus coil writes require bool values",
                    ));
                };
                Ok(protocol::ModbusRequest::WriteSingleCoil {
                    unit: self.probe.unit,
                    address: map.address,
                    value: *v,
                })
            }
            protocol::RegisterKind::HoldingRegister => {
                let registers = encode_register_value(map, value)?;
                if registers.len() == 1 {
                    Ok(protocol::ModbusRequest::WriteSingleRegister {
                        unit: self.probe.unit,
                        address: map.address,
                        value: registers[0],
                    })
                } else {
                    Ok(protocol::ModbusRequest::WriteMultipleRegisters {
                        unit: self.probe.unit,
                        address: map.address,
                        values: registers,
                    })
                }
            }
            protocol::RegisterKind::DiscreteInput | protocol::RegisterKind::InputRegister => {
                Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Modbus input/discrete-input mappings are read-only",
                ))
            }
        }
    }

    fn send_request(&mut self, request: &protocol::ModbusRequest) -> Result<Option<u16>> {
        let transaction_id = match self.probe.transport {
            protocol::TransportMode::Rtu => None,
            protocol::TransportMode::Tcp => Some(self.next_transaction_id()),
        };
        let bytes = match self.probe.transport {
            protocol::TransportMode::Rtu => protocol::encode_rtu(request)?,
            protocol::TransportMode::Tcp => {
                protocol::encode_tcp(transaction_id.expect("tcp transaction id exists"), request)?
            }
        };
        self.serial.write(&bytes)?;
        Ok(transaction_id)
    }

    fn resend_current_request(&mut self, operation: &mut PendingOperation) -> Result<()> {
        let request = operation
            .actions
            .front()
            .ok_or_else(|| Error::new(ErrorCode::Driver, "Modbus operation has no current action"))?
            .request
            .clone();
        operation.transaction_id = self.send_request(&request)?;
        operation.sent_at = Instant::now();
        Ok(())
    }

    fn action_for_write(
        &self,
        device: DeviceId,
        key: &str,
        value: &Value,
        aggregate: bool,
    ) -> Result<PendingAction> {
        self.validate_write(device, key, value)?;
        let map = self.map_for(device, key)?.clone();
        if map.bit_mask.is_some() {
            if map.kind != protocol::RegisterKind::HoldingRegister {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Modbus writable bitfields require a holding register",
                ));
            }
            let request = self.request_for_read(&map);
            return Ok(PendingAction {
                request,
                kind: PendingActionKind::WriteBitfield {
                    key: key.into(),
                    value: value.clone(),
                    map,
                    aggregate,
                    phase: BitfieldWritePhase::Read,
                },
            });
        }
        let request = self.request_for_write(&map, value)?;
        Ok(PendingAction {
            request,
            kind: PendingActionKind::WriteProperty {
                key: key.into(),
                value: value.clone(),
                aggregate,
            },
        })
    }

    fn emit_property(&mut self, key: &str, value: Value) {
        self.pending
            .push_back(DriverEvent::Event(Event::PropertyChanged(
                PropertyChanged {
                    device: self.device,
                    key: key.into(),
                    value,
                },
            )));
    }

    fn validate_timing_sequences(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in plan
            .sequences
            .iter()
            .filter(|sequence| sequence.device == self.device)
        {
            let map = self.map_for(sequence.device, &sequence.property)?;
            if map.kind != protocol::RegisterKind::Coil
                || property_value_type(map) != ValueType::Bool
            {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    "Modbus timing sequences currently require writable bool coil properties",
                ));
            }
            if !map.writable {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Modbus timing sequence targets a read-only property",
                ));
            }
            if sequence.values.is_empty() {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "Modbus timing sequence must contain at least one value",
                ));
            }
            if sequence
                .values
                .iter()
                .any(|value| !matches!(value, Value::Bool(_)))
            {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Modbus timing sequence values must be bools",
                ));
            }
        }
        Ok(())
    }

    fn local_timing_sequences(&self, plan: &TimingPlan) -> Vec<Value> {
        plan.sequences
            .iter()
            .filter(|sequence| sequence.device == self.device)
            .map(|sequence| {
                Value::Map(BTreeMap::from([
                    ("device".into(), Value::I64(sequence.device.0 .0 as i64)),
                    ("property".into(), Value::String(sequence.property.clone())),
                    ("values".into(), Value::List(sequence.values.clone())),
                ]))
            })
            .collect()
    }

    fn local_timing_routes(&self, plan: &TimingPlan) -> Vec<Value> {
        plan.routes
            .iter()
            .filter(|route| route.from == self.device || route.to == self.device)
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

    fn timing_writes(&self, plan: &TimingPlan, start: bool) -> Result<Vec<(String, Value)>> {
        plan.sequences
            .iter()
            .filter(|sequence| sequence.device == self.device)
            .map(|sequence| {
                let value = if start {
                    sequence.values.first()
                } else {
                    sequence.values.last()
                }
                .cloned()
                .ok_or_else(|| {
                    Error::new(
                        ErrorCode::InvalidCommand,
                        "Modbus timing sequence must contain at least one value",
                    )
                })?;
                self.validate_write(sequence.device, &sequence.property, &value)?;
                Ok((sequence.property.clone(), value))
            })
            .collect()
    }

    fn timing_summary(&self, plan: &TimingPlan, action: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("action".into(), Value::String(action.into())),
            ("device".into(), Value::I64(self.device.0 .0 as i64)),
            (
                "transport".into(),
                Value::String(match self.probe.transport {
                    protocol::TransportMode::Rtu => "rtu".into(),
                    protocol::TransportMode::Tcp => "tcp".into(),
                }),
            ),
            (
                "sequences".into(),
                Value::List(self.local_timing_sequences(plan)),
            ),
            ("routes".into(), Value::List(self.local_timing_routes(plan))),
        ]))
    }

    fn queue_timing_writes(
        &mut self,
        writes: Vec<(String, Value)>,
        physical_transactions: &mut Vec<PhysicalTransaction>,
        description_prefix: &str,
    ) -> Result<()> {
        let mut actions = VecDeque::new();
        for (key, value) in writes {
            let action = self.action_for_write(self.device, &key, &value, true)?;
            physical_transactions.push(PhysicalTransaction {
                resource: Some(self.resource),
                description: format!("{description_prefix} {key}"),
                payload: Value::Map(BTreeMap::from([
                    ("property".into(), Value::String(key.clone())),
                    ("value".into(), value.clone()),
                    (
                        "request".into(),
                        Value::String(format!("{:?}", action.request)),
                    ),
                ])),
            });
            self.values.insert(key.clone(), value.clone());
            self.emit_property(&key, value);
            actions.push_back(action);
        }
        if let Some(first) = actions.front().map(|action| action.request.clone()) {
            let transaction_id = self.send_request(&first)?;
            self.in_flight.push_back(self.pending_operation(
                DriverToken(0),
                actions,
                true,
                transaction_id,
            ));
        }
        Ok(())
    }

    fn schedule_background_poll(&mut self) {
        let now = Instant::now();
        let Some((key, _)) = self
            .poll_due
            .iter()
            .filter(|(_, due)| **due <= now)
            .min_by_key(|(_, due)| **due)
            .map(|(key, due)| (key.clone(), *due))
        else {
            return;
        };
        let Ok(map) = self.map_for(self.device, &key).cloned() else {
            self.poll_due.remove(&key);
            return;
        };
        let Some(interval_ms) = map.poll_interval_ms.filter(|interval| *interval > 0) else {
            self.poll_due.remove(&key);
            return;
        };
        self.poll_due
            .insert(key.clone(), now + Duration::from_millis(interval_ms.max(1)));
        let request = self.request_for_read(&map);
        let transaction_id = match self.send_request(&request) {
            Ok(transaction_id) => transaction_id,
            Err(error) => {
                self.pending
                    .push_back(DriverEvent::Event(Event::Fault(FaultEvent {
                        device: Some(self.device),
                        report: error.into(),
                    })));
                return;
            }
        };
        self.in_flight.push_back(self.pending_operation(
            DriverToken(0),
            VecDeque::from([PendingAction {
                request,
                kind: PendingActionKind::PollProperty { key, map },
            }]),
            true,
            transaction_id,
        ));
    }
}

impl Driver for ModbusDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        self.descriptors_for()
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        let mut metadata = BTreeMap::from([
            ("unit_id".into(), Value::I64(self.probe.unit.0 as i64)),
            (
                "completion".into(),
                Value::String("matching Modbus response frame or exception response".into()),
            ),
            (
                "real_transport".into(),
                Value::Bool(self.probe.connect_real_transport),
            ),
            (
                "response_timeout_ms".into(),
                Value::I64(self.probe.response_timeout_ms as i64),
            ),
            (
                "response_correlation".into(),
                Value::String(match self.probe.transport {
                    protocol::TransportMode::Rtu => "ordered-rtu-frame".into(),
                    protocol::TransportMode::Tcp => "mbap-transaction-id".into(),
                }),
            ),
            ("retries".into(), Value::I64(self.probe.retry_count as i64)),
        ]);
        if let Some(endpoint) = &self.probe.tcp_endpoint {
            metadata.insert("tcp_host".into(), Value::String(endpoint.host.clone()));
            metadata.insert("tcp_port".into(), Value::I64(endpoint.port as i64));
            metadata.insert(
                "connect_timeout_ms".into(),
                Value::I64(endpoint.connect_timeout_ms as i64),
            );
        }
        if let Some(endpoint) = &self.probe.rtu_endpoint {
            metadata.insert(
                "serial_port".into(),
                Value::String(endpoint.port_name.clone()),
            );
            metadata.insert("baud_rate".into(), Value::I64(endpoint.baud_rate as i64));
            metadata.insert(
                "serial_timeout_ms".into(),
                Value::I64(endpoint.timeout_ms as i64),
            );
        }
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "modbus-transport".into(),
            kind: match self.probe.transport {
                protocol::TransportMode::Rtu => "modbus.rtu".into(),
                protocol::TransportMode::Tcp => "modbus.tcp".into(),
            },
            metadata,
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.device {
            vec![capability(1, device, CapabilityKind::RawRegisterAccess)]
        } else {
            Vec::new()
        }
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut physical_transactions = Vec::new();
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    let map = self.map_for(*device, key)?;
                    if !map.readable {
                        return Err(Error::new(
                            ErrorCode::InvalidProperty,
                            "Modbus property is not readable",
                        ));
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("modbus read {} {}", map.kind.label(), map.address.0),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    let map = self.map_for(*device, key)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("modbus write {} {}", map.kind.label(), map.address.0),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "modbus mapped state set".into(),
                        payload: Value::List(
                            set.writes
                                .iter()
                                .map(|write| {
                                    Value::Map(BTreeMap::from([
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
                    if *device != self.device
                        || *capability != CapabilityId(1)
                        || !matches!(request, CapabilityRequest::GenericCommand(_))
                    {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "Modbus only supports RawRegisterAccess via GenericCommand",
                        ));
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "modbus raw register access".into(),
                        payload: Value::Map(BTreeMap::new()),
                    });
                }
                Command::Arm(plan) => {
                    self.validate_timing_sequences(plan)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "modbus timing arm summary".into(),
                        payload: self.timing_summary(plan, "arm"),
                    });
                }
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
        let mut actions = VecDeque::new();
        for command in prepared.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    let map = self.map_for(device, &key)?.clone();
                    let request = self.request_for_read(&map);
                    actions.push_back(PendingAction {
                        request,
                        kind: PendingActionKind::ReadProperty { key, map },
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    actions.push_back(self.action_for_write(device, &key, &value, false)?);
                }
                Command::ApplyStateSet(set) => {
                    for write in set.writes {
                        actions.push_back(self.action_for_write(
                            write.device,
                            &write.property,
                            &write.value,
                            true,
                        )?);
                    }
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    actions.push_back(self.action_for_raw(device, capability, request)?);
                }
                Command::Arm(_) | Command::Start(_) | Command::Stop(_) => unreachable!(),
            }
        }
        if actions.is_empty() {
            self.pending.push_back(DriverEvent::TokenCompleted {
                token,
                value: Value::Null,
            });
        } else {
            let first = actions
                .front()
                .expect("nonempty actions must have first action")
                .request
                .clone();
            let transaction_id = self.send_request(&first)?;
            self.in_flight
                .push_back(self.pending_operation(token, actions, false, transaction_id));
        }
        Ok(token)
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        if let Ok(bytes) = self.serial.read_available() {
            if !bytes.is_empty() {
                self.rx_buffer.extend(bytes);
                self.pending
                    .push_back(DriverEvent::Event(Event::Log(LogEvent {
                        driver: Some(self.id),
                        message: format!(
                            "modbus received {} buffered byte(s)",
                            self.rx_buffer.len()
                        ),
                    })));
            }
        }
        match protocol::drain_responses(self.probe.transport, &mut self.rx_buffer) {
            Ok(responses) => {
                for response in responses {
                    self.handle_response(response);
                }
            }
            Err(error) => {
                if let Some(operation) = self.in_flight.pop_front() {
                    self.pending.push_back(DriverEvent::TokenFailed {
                        token: operation.token,
                        report: error.into(),
                    });
                } else {
                    self.pending
                        .push_back(DriverEvent::Event(Event::Fault(FaultEvent {
                            device: Some(self.device),
                            report: error.into(),
                        })));
                }
            }
        }
        self.handle_timeouts();
        if self.in_flight.is_empty() {
            self.schedule_background_poll();
        }
        self.pending.drain(..).collect()
    }

    fn prepare_timing_plan(
        &mut self,
        plan: &TimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        self.validate_timing_sequences(plan)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Arm(plan.clone())],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "modbus timing arm summary".into(),
                payload: self.timing_summary(plan, "arm"),
            }],
        })
    }

    fn start_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        self.validate_timing_sequences(&armed.plan)?;
        let mut physical_transactions = Vec::new();
        let writes = self.timing_writes(&armed.plan, true)?;
        self.queue_timing_writes(
            writes,
            &mut physical_transactions,
            "modbus timing start write",
        )?;
        physical_transactions.push(PhysicalTransaction {
            resource: Some(self.resource),
            description: "modbus timing start summary".into(),
            payload: self.timing_summary(&armed.plan, "start"),
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
        self.validate_timing_sequences(&armed.plan)?;
        let mut physical_transactions = Vec::new();
        let writes = self.timing_writes(&armed.plan, false)?;
        self.queue_timing_writes(
            writes,
            &mut physical_transactions,
            "modbus timing stop write",
        )?;
        physical_transactions.push(PhysicalTransaction {
            resource: Some(self.resource),
            description: "modbus timing stop summary".into(),
            payload: self.timing_summary(&armed.plan, "stop"),
        });
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions,
        })
    }
}

impl ModbusDriver {
    fn handle_timeouts(&mut self) {
        let Some(index) = self.next_timed_out_operation() else {
            return;
        };
        let Some(mut operation) = self.in_flight.remove(index) else {
            return;
        };
        if operation.retries_remaining > 0 {
            operation.retries_remaining -= 1;
            let retries_remaining = operation.retries_remaining;
            match self.resend_current_request(&mut operation) {
                Ok(()) => {
                    self.pending.push_back(DriverEvent::Event(Event::Log(LogEvent {
                        driver: Some(self.id),
                        message: format!(
                            "modbus response timed out; retrying request, {retries_remaining} retries remaining"
                        ),
                    })));
                    self.in_flight.push_back(operation);
                }
                Err(error) => self.fail_or_fault(operation, error),
            }
            return;
        }

        self.fail_or_fault(
            operation,
            Error::new(
                ErrorCode::Timeout,
                format!(
                    "Modbus response timed out after {} ms",
                    self.probe.response_timeout_ms
                ),
            ),
        );
    }

    fn next_timed_out_operation(&self) -> Option<usize> {
        let timeout = Duration::from_millis(self.probe.response_timeout_ms);
        match self.probe.transport {
            protocol::TransportMode::Rtu => self
                .in_flight
                .front()
                .is_some_and(|operation| operation.sent_at.elapsed() >= timeout)
                .then_some(0),
            protocol::TransportMode::Tcp => self
                .in_flight
                .iter()
                .position(|operation| operation.sent_at.elapsed() >= timeout),
        }
    }

    fn fail_or_fault(&mut self, operation: PendingOperation, error: Error) {
        if operation.background {
            self.pending
                .push_back(DriverEvent::Event(Event::Fault(FaultEvent {
                    device: Some(self.device),
                    report: error.into(),
                })));
        } else {
            self.pending.push_back(DriverEvent::TokenFailed {
                token: operation.token,
                report: error.into(),
            });
        }
    }

    fn action_for_raw(
        &mut self,
        device: DeviceId,
        capability: CapabilityId,
        request: CapabilityRequest,
    ) -> Result<PendingAction> {
        if device != self.device || capability != CapabilityId(1) {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "unknown Modbus raw capability",
            ));
        }
        let CapabilityRequest::GenericCommand(req) = request else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Modbus raw capability expects GenericCommand",
            ));
        };
        if req.is_hidden_maintenance() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "GenericCommand {} is a hidden maintenance operation",
                    req.command
                ),
            ));
        }
        let raw = |request| {
            Ok(PendingAction {
                request,
                kind: PendingActionKind::Raw,
            })
        };
        match req.command.as_str() {
            "read_coils" => {
                let address = map_u16(&req.params, "address")?;
                let count = optional_map_u16(&req.params, "count")?.unwrap_or(1);
                raw(protocol::ModbusRequest::Read {
                    unit: self.probe.unit,
                    kind: protocol::RegisterKind::Coil,
                    address: protocol::RegisterAddress(address),
                    quantity: count,
                })
            }
            "read_discrete_inputs" => {
                let address = map_u16(&req.params, "address")?;
                let count = optional_map_u16(&req.params, "count")?.unwrap_or(1);
                raw(protocol::ModbusRequest::Read {
                    unit: self.probe.unit,
                    kind: protocol::RegisterKind::DiscreteInput,
                    address: protocol::RegisterAddress(address),
                    quantity: count,
                })
            }
            "read_holding_register" | "read_holding_registers" => {
                let address = map_u16(&req.params, "address")?;
                let count = optional_map_u16(&req.params, "count")?.unwrap_or(1);
                raw(protocol::ModbusRequest::Read {
                    unit: self.probe.unit,
                    kind: protocol::RegisterKind::HoldingRegister,
                    address: protocol::RegisterAddress(address),
                    quantity: count,
                })
            }
            "read_input_registers" => {
                let address = map_u16(&req.params, "address")?;
                let count = optional_map_u16(&req.params, "count")?.unwrap_or(1);
                raw(protocol::ModbusRequest::Read {
                    unit: self.probe.unit,
                    kind: protocol::RegisterKind::InputRegister,
                    address: protocol::RegisterAddress(address),
                    quantity: count,
                })
            }
            "write_single_coil" => {
                let address = map_u16(&req.params, "address")?;
                let value = map_bool_param(&req.params, "value")?;
                raw(protocol::ModbusRequest::WriteSingleCoil {
                    unit: self.probe.unit,
                    address: protocol::RegisterAddress(address),
                    value,
                })
            }
            "write_single_register" => {
                let address = map_u16(&req.params, "address")?;
                let value = map_u16(&req.params, "value")?;
                raw(protocol::ModbusRequest::WriteSingleRegister {
                    unit: self.probe.unit,
                    address: protocol::RegisterAddress(address),
                    value,
                })
            }
            "write_multiple_coils" => {
                let address = map_u16(&req.params, "address")?;
                let values = map_bool_list(&req.params, "values")?;
                raw(protocol::ModbusRequest::WriteMultipleCoils {
                    unit: self.probe.unit,
                    address: protocol::RegisterAddress(address),
                    values,
                })
            }
            "write_multiple_registers" => {
                let address = map_u16(&req.params, "address")?;
                let values = map_u16_list(&req.params, "values")?;
                raw(protocol::ModbusRequest::WriteMultipleRegisters {
                    unit: self.probe.unit,
                    address: protocol::RegisterAddress(address),
                    values,
                })
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                format!("unsupported Modbus raw command {}", req.command),
            )),
        }
    }

    fn handle_response(&mut self, response: protocol::ModbusResponse) {
        let Some(mut operation) = self.take_operation_for_response(&response) else {
            self.pending
                .push_back(DriverEvent::Event(Event::Fault(FaultEvent {
                    device: Some(self.device),
                    report: Error::new(
                        ErrorCode::Transport,
                        format!(
                            "unexpected Modbus response with transaction {:?}",
                            response.transaction_id
                        ),
                    )
                    .into(),
                })));
            return;
        };
        let Some(action) = operation.actions.pop_front() else {
            let error = Error::new(ErrorCode::Driver, "Modbus operation has no pending action");
            if operation.background {
                self.pending
                    .push_back(DriverEvent::Event(Event::Fault(FaultEvent {
                        device: Some(self.device),
                        report: error.into(),
                    })));
            } else {
                self.pending.push_back(DriverEvent::TokenFailed {
                    token: operation.token,
                    report: error.into(),
                });
            }
            return;
        };
        match self.complete_action(&action, &response) {
            Ok(ActionCompletion::Complete(value)) => {
                match &action.kind {
                    PendingActionKind::WriteProperty {
                        key,
                        aggregate: true,
                        ..
                    } => {
                        let mut changed = match operation.last {
                            Value::Map(map) => map,
                            _ => BTreeMap::new(),
                        };
                        changed.insert(key.clone(), value);
                        operation.last = Value::Map(changed);
                    }
                    PendingActionKind::WriteBitfield {
                        key,
                        aggregate: true,
                        ..
                    } => {
                        let mut changed = match operation.last {
                            Value::Map(map) => map,
                            _ => BTreeMap::new(),
                        };
                        changed.insert(key.clone(), value);
                        operation.last = Value::Map(changed);
                    }
                    _ => operation.last = value,
                }
                if operation.actions.is_empty() {
                    if !operation.background {
                        self.pending.push_back(DriverEvent::TokenCompleted {
                            token: operation.token,
                            value: operation.last,
                        });
                    }
                } else {
                    let next = operation
                        .actions
                        .front()
                        .expect("nonempty actions must have next action")
                        .request
                        .clone();
                    operation.transaction_id = match self.send_request(&next) {
                        Ok(transaction_id) => transaction_id,
                        Err(error) => {
                            self.pending.push_back(DriverEvent::TokenFailed {
                                token: operation.token,
                                report: error.into(),
                            });
                            return;
                        }
                    };
                    operation.sent_at = Instant::now();
                    operation.retries_remaining = self.probe.retry_count;
                    self.in_flight.push_back(operation);
                }
            }
            Ok(ActionCompletion::Continue(next_action)) => {
                let send_result = self.send_request(&next_action.request);
                operation.actions.push_front(next_action);
                match send_result {
                    Ok(transaction_id) => {
                        operation.transaction_id = transaction_id;
                        operation.sent_at = Instant::now();
                        operation.retries_remaining = self.probe.retry_count;
                        self.in_flight.push_back(operation);
                    }
                    Err(error) => {
                        self.pending.push_back(DriverEvent::TokenFailed {
                            token: operation.token,
                            report: error.into(),
                        });
                    }
                }
            }
            Err(error) => {
                if operation.background {
                    self.pending
                        .push_back(DriverEvent::Event(Event::Fault(FaultEvent {
                            device: Some(self.device),
                            report: error.into(),
                        })));
                } else {
                    self.pending.push_back(DriverEvent::TokenFailed {
                        token: operation.token,
                        report: error.into(),
                    });
                }
            }
        }
    }

    fn take_operation_for_response(
        &mut self,
        response: &protocol::ModbusResponse,
    ) -> Option<PendingOperation> {
        match self.probe.transport {
            protocol::TransportMode::Rtu => self.in_flight.pop_front(),
            protocol::TransportMode::Tcp => {
                let transaction_id = response.transaction_id?;
                let index = self
                    .in_flight
                    .iter()
                    .position(|operation| operation.transaction_id == Some(transaction_id))?;
                self.in_flight.remove(index)
            }
        }
    }

    fn complete_action(
        &mut self,
        action: &PendingAction,
        response: &protocol::ModbusResponse,
    ) -> Result<ActionCompletion> {
        if response.unit != action.request.unit() {
            return Err(Error::new(
                ErrorCode::Transport,
                format!(
                    "Modbus response unit {} did not match request unit {}",
                    response.unit.0,
                    action.request.unit().0
                ),
            ));
        }
        if response.pdu.len() >= 2 && response.pdu[0] & 0x80 != 0 {
            return Err(Error::new(
                ErrorCode::Driver,
                format!(
                    "Modbus exception for function 0x{:02x}: code {}",
                    response.pdu[0] & 0x7f,
                    response.pdu[1]
                ),
            ));
        }
        match &action.kind {
            PendingActionKind::ReadProperty { key, map } => {
                let value = decode_read_value(map, &response.pdu)?;
                self.values.insert(key.clone(), value.clone());
                Ok(ActionCompletion::Complete(value))
            }
            PendingActionKind::PollProperty { key, map } => {
                let value = decode_read_value(map, &response.pdu)?;
                let changed = self.values.get(key) != Some(&value);
                self.values.insert(key.clone(), value.clone());
                if changed {
                    self.emit_property(key, value.clone());
                }
                Ok(ActionCompletion::Complete(value))
            }
            PendingActionKind::WriteProperty { key, value, .. } => {
                validate_write_ack(&action.request, &response.pdu)?;
                self.values.insert(key.clone(), value.clone());
                self.emit_property(key, value.clone());
                Ok(ActionCompletion::Complete(value.clone()))
            }
            PendingActionKind::WriteBitfield {
                key,
                value,
                map,
                aggregate,
                phase,
            } => match phase {
                BitfieldWritePhase::Read => {
                    let raw = decode_raw_register_value(map, &response.pdu)?;
                    let field = raw_value_for_property(map, value)?;
                    let merged = merge_bitfield(raw, map, field)?;
                    Ok(ActionCompletion::Continue(PendingAction {
                        request: protocol::ModbusRequest::WriteSingleRegister {
                            unit: self.probe.unit,
                            address: map.address,
                            value: merged as u16,
                        },
                        kind: PendingActionKind::WriteBitfield {
                            key: key.clone(),
                            value: value.clone(),
                            map: map.clone(),
                            aggregate: *aggregate,
                            phase: BitfieldWritePhase::Write,
                        },
                    }))
                }
                BitfieldWritePhase::Write => {
                    validate_write_ack(&action.request, &response.pdu)?;
                    self.values.insert(key.clone(), value.clone());
                    self.emit_property(key, value.clone());
                    Ok(ActionCompletion::Complete(value.clone()))
                }
            },
            PendingActionKind::Raw => Ok(ActionCompletion::Complete(Value::Bytes(
                response.raw.clone(),
            ))),
        }
    }
}

enum ActionCompletion {
    Complete(Value),
    Continue(PendingAction),
}

fn fixture_values(maps: &[ModbusPropertyMap]) -> BTreeMap<String, Value> {
    maps.iter()
        .map(|map| {
            let raw_value = match map.value_map {
                ModbusValueMap::Bool => Value::Bool(false),
                ModbusValueMap::U16 => Value::I64(0),
                ModbusValueMap::I16 => Value::I64(23),
                ModbusValueMap::U32 => Value::I64(0),
                ModbusValueMap::I32 => Value::I64(0),
                ModbusValueMap::U64 => Value::I64(0),
                ModbusValueMap::I64 => Value::I64(0),
                ModbusValueMap::F32 => Value::F64(0.0),
                ModbusValueMap::F64 => Value::F64(0.0),
            };
            let value = if let Some(quantity) = map.quantity {
                let raw = match raw_value {
                    Value::I64(value) => value as f64,
                    Value::F64(value) => value,
                    Value::Bool(value) => {
                        if value {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    _ => 0.0,
                };
                quantity_value_for_read(quantity, raw * map.scale + map.offset)
            } else {
                raw_value
            };
            (map.key.clone(), value)
        })
        .collect()
}

fn property_value_type(map: &ModbusPropertyMap) -> ValueType {
    if let Some(quantity) = map.quantity {
        return quantity_value_type(quantity);
    }
    if !map.enum_values.is_empty() {
        return ValueType::String;
    }
    if is_bool_bitfield(map) {
        return ValueType::Bool;
    }
    if is_scaled(map) {
        return ValueType::F64;
    }
    value_type(&map.value_map)
}

fn quantity_value_type(quantity: ModbusQuantity) -> ValueType {
    match quantity {
        ModbusQuantity::TemperatureCelsius => ValueType::Temperature,
        ModbusQuantity::PressureKilopascals => ValueType::Pressure,
        ModbusQuantity::GasPercent => ValueType::GasConcentration,
        ModbusQuantity::FlowMicrolitersPerMinute => ValueType::FlowRate,
        ModbusQuantity::RatioPercent => ValueType::Ratio,
        ModbusQuantity::TimeMilliseconds | ModbusQuantity::TimeMicroseconds => {
            ValueType::TimeInterval
        }
    }
}

fn value_type(value_map: &ModbusValueMap) -> ValueType {
    match value_map {
        ModbusValueMap::Bool => ValueType::Bool,
        ModbusValueMap::U16
        | ModbusValueMap::I16
        | ModbusValueMap::U32
        | ModbusValueMap::I32
        | ModbusValueMap::U64
        | ModbusValueMap::I64 => ValueType::I64,
        ModbusValueMap::F32 | ModbusValueMap::F64 => ValueType::F64,
    }
}

fn property_schema(map: &ModbusPropertyMap) -> PropertySchema {
    PropertySchema {
        key: map.key.clone(),
        display_name: map.display_name.clone(),
        value_type: property_value_type(map),
        unit: map.quantity.map(quantity_unit),
        range: if is_scaled(map) || !map.enum_values.is_empty() {
            None
        } else {
            match map.value_map {
                ModbusValueMap::Bool => None,
                ModbusValueMap::U16 => Some(Range {
                    min: Value::I64(0),
                    max: Value::I64(u16::MAX as i64),
                }),
                ModbusValueMap::I16 => Some(Range {
                    min: Value::I64(i16::MIN as i64),
                    max: Value::I64(i16::MAX as i64),
                }),
                ModbusValueMap::U32 => Some(Range {
                    min: Value::I64(0),
                    max: Value::I64(u32::MAX as i64),
                }),
                ModbusValueMap::I32 => Some(Range {
                    min: Value::I64(i32::MIN as i64),
                    max: Value::I64(i32::MAX as i64),
                }),
                ModbusValueMap::U64 => Some(Range {
                    min: Value::I64(0),
                    max: Value::I64(i64::MAX),
                }),
                ModbusValueMap::I64 => Some(Range {
                    min: Value::I64(i64::MIN),
                    max: Value::I64(i64::MAX),
                }),
                ModbusValueMap::F32 | ModbusValueMap::F64 => None,
            }
        },
        increment: None,
        enum_values: map
            .enum_values
            .iter()
            .map(|(label, _)| EnumValue {
                value: Value::String(label.clone()),
                label: label.clone(),
            })
            .collect(),
        readable: map.readable,
        writable: map.writable,
        volatile: true,
        sequenceable: map.writable,
        hardware_address: Some(format!("{}:{}", map.kind.label(), map.address.0)),
    }
}

fn quantity_unit(quantity: ModbusQuantity) -> Unit {
    Unit(
        match quantity {
            ModbusQuantity::TemperatureCelsius => "degC",
            ModbusQuantity::PressureKilopascals => "kPa",
            ModbusQuantity::GasPercent => "percent",
            ModbusQuantity::FlowMicrolitersPerMinute => "uL/min",
            ModbusQuantity::RatioPercent => "percent",
            ModbusQuantity::TimeMilliseconds => "ms",
            ModbusQuantity::TimeMicroseconds => "us",
        }
        .into(),
    )
}

fn poll_interval_metadata(maps: &[ModbusPropertyMap]) -> Value {
    Value::Map(
        maps.iter()
            .filter_map(|map| {
                map.poll_interval_ms
                    .filter(|interval| *interval > 0)
                    .map(|interval| {
                        (
                            map.key.clone(),
                            Value::TimeInterval(TimeInterval::from_milliseconds(interval as f64)),
                        )
                    })
            })
            .collect(),
    )
}

fn is_scaled(map: &ModbusPropertyMap) -> bool {
    (map.scale - 1.0).abs() > f64::EPSILON || map.offset.abs() > f64::EPSILON
}

fn is_bool_bitfield(map: &ModbusPropertyMap) -> bool {
    map.bit_mask
        .map(|mask| mask.count_ones() == 1 && map.enum_values.is_empty() && !is_scaled(map))
        .unwrap_or(false)
}

fn default_register(map: &ModbusPropertyMap) -> u16 {
    if let Some(mask) = map.bit_mask {
        return mask as u16;
    }
    match map.value_map {
        ModbusValueMap::I16 if map.kind == protocol::RegisterKind::InputRegister => 23u16,
        _ => 0,
    }
}

fn encode_default_registers(map: &ModbusPropertyMap) -> Vec<u16> {
    match map.value_map {
        ModbusValueMap::F32 => encode_registers(0f32.to_bits() as u64, 2, map.endian),
        ModbusValueMap::F64 => encode_registers(0f64.to_bits(), 4, map.endian),
        _ => encode_registers(0, map.value_map.register_count(), map.endian),
    }
}

fn encode_register_value(map: &ModbusPropertyMap, value: &Value) -> Result<Vec<u16>> {
    if matches!(map.value_map, ModbusValueMap::F32)
        && !is_scaled(map)
        && map.enum_values.is_empty()
        && map.quantity.is_none()
    {
        let Value::F64(value) = value else {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Modbus f32 property expects F64",
            ));
        };
        if !value.is_finite() || *value < f32::MIN as f64 || *value > f32::MAX as f64 {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Modbus f32 register write is out of range",
            ));
        }
        return Ok(encode_registers(
            (*value as f32).to_bits() as u64,
            2,
            map.endian,
        ));
    }
    if matches!(map.value_map, ModbusValueMap::F64)
        && !is_scaled(map)
        && map.enum_values.is_empty()
        && map.quantity.is_none()
    {
        let Value::F64(value) = value else {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Modbus f64 property expects F64",
            ));
        };
        if !value.is_finite() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Modbus f64 register write must be finite",
            ));
        }
        return Ok(encode_registers(value.to_bits(), 4, map.endian));
    }
    let raw = raw_value_for_property(map, value)?;
    match map.value_map {
        ModbusValueMap::U16 | ModbusValueMap::I16 => Ok(vec![raw as u16]),
        ModbusValueMap::U32 | ModbusValueMap::I32 => {
            Ok(encode_registers(raw as u32 as u64, 2, map.endian))
        }
        ModbusValueMap::U64 | ModbusValueMap::I64 => {
            Ok(encode_registers(raw as u64, 4, map.endian))
        }
        ModbusValueMap::F32 => Ok(encode_registers(
            (raw as f32).to_bits() as u64,
            2,
            map.endian,
        )),
        ModbusValueMap::F64 => Ok(encode_registers((raw as f64).to_bits(), 4, map.endian)),
        ModbusValueMap::Bool => Err(Error::new(
            ErrorCode::InvalidProperty,
            "bool Modbus values are encoded as coils, not registers",
        )),
    }
}

fn raw_value_for_property(map: &ModbusPropertyMap, value: &Value) -> Result<i64> {
    if is_bool_bitfield(map) {
        let Value::Bool(value) = value else {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Modbus bool bitfield property expects Bool",
            ));
        };
        return Ok(i64::from(*value));
    }
    if !map.enum_values.is_empty() {
        let Value::String(label) = value else {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Modbus enum property expects a string label",
            ));
        };
        return map.enum_values.get(label).copied().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Modbus enum label {label}"),
            )
        });
    }

    let raw = if is_scaled(map) || map.quantity.is_some() {
        let value = native_numeric_for_write(map, value)?;
        if map.scale == 0.0 {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Modbus scaled property cannot use scale 0",
            ));
        }
        ((value - map.offset) / map.scale).round() as i64
    } else {
        match (&map.value_map, value) {
            (ModbusValueMap::U16, Value::I64(value))
            | (ModbusValueMap::I16, Value::I64(value))
            | (ModbusValueMap::U32, Value::I64(value))
            | (ModbusValueMap::I32, Value::I64(value))
            | (ModbusValueMap::U64, Value::I64(value))
            | (ModbusValueMap::I64, Value::I64(value)) => *value,
            (ModbusValueMap::F32, Value::F64(value)) => {
                if !value.is_finite() || *value < f32::MIN as f64 || *value > f32::MAX as f64 {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Modbus f32 register write is out of range",
                    ));
                }
                return Ok(0);
            }
            (ModbusValueMap::F64, Value::F64(value)) => {
                if !value.is_finite() {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Modbus f64 register write must be finite",
                    ));
                }
                return Ok(0);
            }
            (ModbusValueMap::Bool, Value::Bool(value)) => return Ok(i64::from(*value)),
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Modbus value type does not match mapping",
                ));
            }
        }
    };
    validate_raw_range(map, raw)?;
    Ok(raw)
}

fn native_numeric_for_write(map: &ModbusPropertyMap, value: &Value) -> Result<f64> {
    match map.quantity {
        Some(ModbusQuantity::TemperatureCelsius) => match value {
            Value::Temperature(value) => Ok(value.celsius()),
            _ => Err(quantity_write_error("Temperature")),
        },
        Some(ModbusQuantity::PressureKilopascals) => match value {
            Value::Pressure(value) => Ok(value.pascals() / 1_000.0),
            _ => Err(quantity_write_error("Pressure")),
        },
        Some(ModbusQuantity::GasPercent) => match value {
            Value::GasConcentration(value) => Ok(value.percent()),
            _ => Err(quantity_write_error("GasConcentration")),
        },
        Some(ModbusQuantity::FlowMicrolitersPerMinute) => match value {
            Value::FlowRate(value) => Ok(value.microliters_per_minute()),
            _ => Err(quantity_write_error("FlowRate")),
        },
        Some(ModbusQuantity::RatioPercent) => match value {
            Value::Ratio(value) => Ok(value.percent()),
            _ => Err(quantity_write_error("Ratio")),
        },
        Some(ModbusQuantity::TimeMilliseconds) => match value {
            Value::TimeInterval(value) => Ok(value.microseconds() * 1e-3),
            _ => Err(quantity_write_error("TimeInterval")),
        },
        Some(ModbusQuantity::TimeMicroseconds) => match value {
            Value::TimeInterval(value) => Ok(value.microseconds()),
            _ => Err(quantity_write_error("TimeInterval")),
        },
        None => match value {
            Value::F64(value) if value.is_finite() => Ok(*value),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                "Modbus scaled property expects F64",
            )),
        },
    }
}

fn quantity_write_error(expected: &str) -> Error {
    Error::new(
        ErrorCode::InvalidProperty,
        format!("Modbus typed property expects Value::{expected}"),
    )
}

fn quantity_value_for_read(quantity: ModbusQuantity, native: f64) -> Value {
    match quantity {
        ModbusQuantity::TemperatureCelsius => Value::Temperature(Temperature::from_celsius(native)),
        ModbusQuantity::PressureKilopascals => Value::Pressure(Pressure::from_kilopascals(native)),
        ModbusQuantity::GasPercent => {
            Value::GasConcentration(GasConcentration::from_percent(native))
        }
        ModbusQuantity::FlowMicrolitersPerMinute => {
            Value::FlowRate(FlowRate::from_microliters_per_minute(native))
        }
        ModbusQuantity::RatioPercent => Value::Ratio(Ratio::from_percent(native)),
        ModbusQuantity::TimeMilliseconds => {
            Value::TimeInterval(TimeInterval::from_milliseconds(native))
        }
        ModbusQuantity::TimeMicroseconds => {
            Value::TimeInterval(TimeInterval::from_microseconds(native))
        }
    }
}

fn validate_raw_range(map: &ModbusPropertyMap, raw: i64) -> Result<()> {
    let ok = if let Some(mask) = map.bit_mask {
        raw >= 0 && (raw as u64) <= (mask >> map.bit_shift)
    } else {
        match map.value_map {
            ModbusValueMap::Bool => raw == 0 || raw == 1,
            ModbusValueMap::U16 => (0..=u16::MAX as i64).contains(&raw),
            ModbusValueMap::I16 => ((i16::MIN as i64)..=(i16::MAX as i64)).contains(&raw),
            ModbusValueMap::U32 => (0..=u32::MAX as i64).contains(&raw),
            ModbusValueMap::I32 => ((i32::MIN as i64)..=(i32::MAX as i64)).contains(&raw),
            ModbusValueMap::U64 => raw >= 0,
            ModbusValueMap::I64 => true,
            ModbusValueMap::F32 | ModbusValueMap::F64 => true,
        }
    };
    if ok {
        Ok(())
    } else {
        Err(Error::new(
            ErrorCode::InvalidProperty,
            format!(
                "Modbus raw value {raw} is out of range for {:?}",
                map.value_map
            ),
        ))
    }
}

fn merge_bitfield(raw_register: i64, map: &ModbusPropertyMap, field_value: i64) -> Result<u64> {
    let mask = map.bit_mask.ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidProperty,
            "Modbus bitfield merge requires bit_mask",
        )
    })?;
    validate_raw_range(map, field_value)?;
    let shifted = (field_value as u64) << map.bit_shift;
    Ok((raw_register as u64 & !mask) | (shifted & mask))
}

fn encode_registers(bits: u64, register_count: u16, endian: ModbusEndian) -> Vec<u16> {
    let mut registers = (0..register_count)
        .map(|index| {
            let shift = (register_count - 1 - index) as u32 * 16;
            ((bits >> shift) & 0xffff) as u16
        })
        .collect::<Vec<_>>();
    if matches!(
        endian,
        ModbusEndian::LittleWord | ModbusEndian::LittleWordByteSwap
    ) {
        registers.reverse();
    }
    if matches!(
        endian,
        ModbusEndian::ByteSwap | ModbusEndian::LittleWordByteSwap
    ) {
        for register in &mut registers {
            *register = register.swap_bytes();
        }
    }
    registers
}

fn decode_registers(registers: &[u16], register_count: u16, endian: ModbusEndian) -> Result<u64> {
    if registers.len() < register_count as usize {
        return Err(Error::new(
            ErrorCode::Transport,
            format!("Modbus {register_count}-register response has too few registers"),
        ));
    }
    let mut ordered = registers[..register_count as usize].to_vec();
    if matches!(
        endian,
        ModbusEndian::ByteSwap | ModbusEndian::LittleWordByteSwap
    ) {
        for register in &mut ordered {
            *register = register.swap_bytes();
        }
    }
    if matches!(
        endian,
        ModbusEndian::LittleWord | ModbusEndian::LittleWordByteSwap
    ) {
        ordered.reverse();
    }
    Ok(ordered
        .into_iter()
        .fold(0u64, |acc, register| (acc << 16) | register as u64))
}

fn decode_read_value(map: &ModbusPropertyMap, pdu: &[u8]) -> Result<Value> {
    if pdu.len() < 3 {
        return Err(Error::new(
            ErrorCode::Transport,
            "Modbus read response is too short",
        ));
    }
    if pdu[0] != map.kind.read_function() {
        return Err(Error::new(
            ErrorCode::Transport,
            format!(
                "Modbus read response function 0x{:02x} did not match expected 0x{:02x}",
                pdu[0],
                map.kind.read_function()
            ),
        ));
    }
    let raw_value = match map.value_map {
        ModbusValueMap::Bool => {
            if pdu[1] == 0 {
                return Err(Error::new(
                    ErrorCode::Transport,
                    "Modbus coil response has zero byte count",
                ));
            }
            return Ok(Value::Bool(pdu[2] & 1 != 0));
        }
        ModbusValueMap::U16 => {
            if pdu[1] < 2 || pdu.len() < 4 {
                return Err(Error::new(
                    ErrorCode::Transport,
                    "Modbus register response has too few bytes",
                ));
            }
            u16::from_be_bytes([pdu[2], pdu[3]]) as i64
        }
        ModbusValueMap::I16 => {
            if pdu[1] < 2 || pdu.len() < 4 {
                return Err(Error::new(
                    ErrorCode::Transport,
                    "Modbus register response has too few bytes",
                ));
            }
            i16::from_be_bytes([pdu[2], pdu[3]]) as i64
        }
        ModbusValueMap::U32
        | ModbusValueMap::I32
        | ModbusValueMap::U64
        | ModbusValueMap::I64
        | ModbusValueMap::F32
        | ModbusValueMap::F64 => {
            let byte_count = map.value_map.register_count() as usize * 2;
            if pdu[1] as usize != byte_count || pdu.len() < 2 + byte_count {
                return Err(Error::new(
                    ErrorCode::Transport,
                    format!(
                        "Modbus {}-register response has wrong byte count",
                        map.value_map.register_count()
                    ),
                ));
            }
            let registers = pdu[2..2 + byte_count]
                .chunks_exact(2)
                .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
                .collect::<Vec<_>>();
            let bits = decode_registers(&registers, map.value_map.register_count(), map.endian)?;
            if matches!(map.value_map, ModbusValueMap::F32)
                && !is_scaled(map)
                && map.enum_values.is_empty()
                && map.quantity.is_none()
            {
                return Ok(Value::F64(f32::from_bits(bits as u32) as f64));
            }
            if matches!(map.value_map, ModbusValueMap::F64)
                && !is_scaled(map)
                && map.enum_values.is_empty()
                && map.quantity.is_none()
            {
                return Ok(Value::F64(f64::from_bits(bits)));
            }
            match map.value_map {
                ModbusValueMap::U32 | ModbusValueMap::F32 => bits as i64,
                ModbusValueMap::I32 => (bits as u32 as i32) as i64,
                ModbusValueMap::U64 | ModbusValueMap::F64 => {
                    if bits > i64::MAX as u64 {
                        return Err(Error::new(
                            ErrorCode::InvalidProperty,
                            "Modbus u64 register value exceeds runtime I64 range",
                        ));
                    }
                    bits as i64
                }
                ModbusValueMap::I64 => bits as i64,
                _ => unreachable!(),
            }
        }
    };
    let raw_value = if let Some(mask) = map.bit_mask {
        ((raw_value as u64 & mask) >> map.bit_shift) as i64
    } else {
        raw_value
    };
    if is_bool_bitfield(map) {
        return Ok(Value::Bool(raw_value != 0));
    }
    if !map.enum_values.is_empty() {
        if let Some((label, _)) = map
            .enum_values
            .iter()
            .find(|(_, candidate)| **candidate == raw_value)
        {
            return Ok(Value::String(label.clone()));
        }
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Modbus enum raw value {raw_value} has no label"),
        ));
    }
    if is_scaled(map) || map.quantity.is_some() {
        let native = raw_value as f64 * map.scale + map.offset;
        if let Some(quantity) = map.quantity {
            return Ok(quantity_value_for_read(quantity, native));
        }
        return Ok(Value::F64(native));
    }
    Ok(Value::I64(raw_value))
}

fn decode_raw_register_value(map: &ModbusPropertyMap, pdu: &[u8]) -> Result<i64> {
    if pdu.len() < 4 {
        return Err(Error::new(
            ErrorCode::Transport,
            "Modbus register response is too short for read-modify-write",
        ));
    }
    if pdu[0] != map.kind.read_function() {
        return Err(Error::new(
            ErrorCode::Transport,
            format!(
                "Modbus RMW read function 0x{:02x} did not match expected 0x{:02x}",
                pdu[0],
                map.kind.read_function()
            ),
        ));
    }
    if pdu[1] < 2 {
        return Err(Error::new(
            ErrorCode::Transport,
            "Modbus RMW read returned too few bytes",
        ));
    }
    Ok(u16::from_be_bytes([pdu[2], pdu[3]]) as i64)
}

fn validate_write_ack(request: &protocol::ModbusRequest, pdu: &[u8]) -> Result<()> {
    let expected = request.pdu()?;
    let expected_prefix_len = match request {
        protocol::ModbusRequest::WriteSingleCoil { .. }
        | protocol::ModbusRequest::WriteSingleRegister { .. } => expected.len(),
        protocol::ModbusRequest::WriteMultipleCoils { .. }
        | protocol::ModbusRequest::WriteMultipleRegisters { .. } => 5,
        protocol::ModbusRequest::Read { .. } => {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "read request cannot have a write acknowledgement",
            ));
        }
    };
    if pdu.len() < expected_prefix_len
        || pdu[..expected_prefix_len] != expected[..expected_prefix_len]
    {
        return Err(Error::new(
            ErrorCode::Transport,
            "Modbus write acknowledgement did not match request",
        ));
    }
    Ok(())
}

fn capability(id: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    CapabilityDescriptor::new(CapabilityId(id), device, kind, ValueType::Map)
}

fn map_u16(params: &BTreeMap<String, Value>, key: &str) -> Result<u16> {
    let Some(value) = params.get(key) else {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("missing Modbus parameter {key}"),
        ));
    };
    let Value::I64(value) = value else {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("Modbus parameter {key} must be I64"),
        ));
    };
    if !(0..=u16::MAX as i64).contains(value) {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("Modbus parameter {key} is out of u16 range"),
        ));
    }
    Ok(*value as u16)
}

fn optional_map_u16(params: &BTreeMap<String, Value>, key: &str) -> Result<Option<u16>> {
    if params.contains_key(key) {
        map_u16(params, key).map(Some)
    } else {
        Ok(None)
    }
}

fn map_bool_param(params: &BTreeMap<String, Value>, key: &str) -> Result<bool> {
    let Some(value) = params.get(key) else {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("missing Modbus parameter {key}"),
        ));
    };
    let Value::Bool(value) = value else {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("Modbus parameter {key} must be Bool"),
        ));
    };
    Ok(*value)
}

fn map_u16_list(params: &BTreeMap<String, Value>, key: &str) -> Result<Vec<u16>> {
    let values = map_list(params, key)?;
    let mapped: Result<Vec<_>> = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let Value::I64(value) = value else {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    format!("Modbus parameter {key}[{index}] must be I64"),
                ));
            };
            if !(0..=u16::MAX as i64).contains(value) {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    format!("Modbus parameter {key}[{index}] is out of u16 range"),
                ));
            }
            Ok(*value as u16)
        })
        .collect();
    require_nonempty_list(mapped?, key)
}

fn map_bool_list(params: &BTreeMap<String, Value>, key: &str) -> Result<Vec<bool>> {
    let values = map_list(params, key)?;
    let mapped: Result<Vec<_>> = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let Value::Bool(value) = value else {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    format!("Modbus parameter {key}[{index}] must be Bool"),
                ));
            };
            Ok(*value)
        })
        .collect();
    require_nonempty_list(mapped?, key)
}

fn map_list<'a>(params: &'a BTreeMap<String, Value>, key: &str) -> Result<&'a [Value]> {
    let Some(value) = params.get(key) else {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("missing Modbus parameter {key}"),
        ));
    };
    let Value::List(values) = value else {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("Modbus parameter {key} must be List"),
        ));
    };
    Ok(values)
}

fn require_nonempty_list<T>(values: Vec<T>, key: &str) -> Result<Vec<T>> {
    if values.is_empty() {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("Modbus parameter {key} must be nonempty"),
        ));
    }
    Ok(values)
}
