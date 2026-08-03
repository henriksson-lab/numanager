use crate::{Error, ErrorCode, Result};
use std::collections::VecDeque;

#[cfg(feature = "os-serial")]
use std::io::{ErrorKind as IoErrorKind, Read, Write};
#[cfg(feature = "os-serial")]
use std::time::Duration;

pub trait SerialIo: Send {
    fn write(&mut self, bytes: &[u8]) -> Result<()>;
    fn read_available(&mut self) -> Result<Vec<u8>>;
}

#[cfg(feature = "os-serial")]
#[derive(Debug, Clone)]
pub struct OsSerialConfig {
    pub port_name: String,
    pub baud_rate: u32,
    pub timeout: Duration,
    pub data_bits: serialport::DataBits,
    pub flow_control: serialport::FlowControl,
    pub parity: serialport::Parity,
    pub stop_bits: serialport::StopBits,
}

#[cfg(feature = "os-serial")]
impl OsSerialConfig {
    pub fn new(port_name: impl Into<String>, baud_rate: u32) -> Self {
        Self {
            port_name: port_name.into(),
            baud_rate,
            timeout: Duration::from_millis(1),
            data_bits: serialport::DataBits::Eight,
            flow_control: serialport::FlowControl::None,
            parity: serialport::Parity::None,
            stop_bits: serialport::StopBits::One,
        }
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn data_bits(mut self, data_bits: serialport::DataBits) -> Self {
        self.data_bits = data_bits;
        self
    }

    pub fn flow_control(mut self, flow_control: serialport::FlowControl) -> Self {
        self.flow_control = flow_control;
        self
    }

    pub fn parity(mut self, parity: serialport::Parity) -> Self {
        self.parity = parity;
        self
    }

    pub fn stop_bits(mut self, stop_bits: serialport::StopBits) -> Self {
        self.stop_bits = stop_bits;
        self
    }
}

#[cfg(feature = "os-serial")]
pub struct OsSerialPort {
    port: Box<dyn serialport::SerialPort>,
}

#[cfg(feature = "os-serial")]
impl OsSerialPort {
    pub fn open(port_name: impl Into<String>, baud_rate: u32) -> Result<Self> {
        Self::open_config(OsSerialConfig::new(port_name, baud_rate))
    }

    pub fn open_config(config: OsSerialConfig) -> Result<Self> {
        let port = serialport::new(&config.port_name, config.baud_rate)
            .timeout(config.timeout)
            .data_bits(config.data_bits)
            .flow_control(config.flow_control)
            .parity(config.parity)
            .stop_bits(config.stop_bits)
            .open()
            .map_err(map_serial_error)?;
        Ok(Self { port })
    }
}

#[cfg(feature = "os-serial")]
impl SerialIo for OsSerialPort {
    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.port.write_all(bytes).map_err(map_io_error)
    }

    fn read_available(&mut self) -> Result<Vec<u8>> {
        let pending = match self.port.bytes_to_read() {
            Ok(0) => return Ok(Vec::new()),
            Ok(n) => n as usize,
            Err(_) => 4096,
        };
        let mut buffer = vec![0; pending.max(1)];
        match self.port.read(buffer.as_mut_slice()) {
            Ok(n) => {
                buffer.truncate(n);
                Ok(buffer)
            }
            Err(err)
                if matches!(
                    err.kind(),
                    IoErrorKind::TimedOut | IoErrorKind::WouldBlock | IoErrorKind::Interrupted
                ) =>
            {
                Ok(Vec::new())
            }
            Err(err) => Err(map_io_error(err)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    Cr,
    CrLf,
}

impl LineEnding {
    pub fn bytes(&self) -> &'static [u8] {
        match self {
            LineEnding::Lf => b"\n",
            LineEnding::Cr => b"\r",
            LineEnding::CrLf => b"\r\n",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SerialLineCodec {
    send_ending: LineEnding,
    recv_ending: LineEnding,
    buffer: Vec<u8>,
}

impl SerialLineCodec {
    pub fn new(send_ending: LineEnding, recv_ending: LineEnding) -> Self {
        Self {
            send_ending,
            recv_ending,
            buffer: Vec::new(),
        }
    }

    pub fn encode(&self, line: &str) -> Vec<u8> {
        let mut bytes = line.as_bytes().to_vec();
        bytes.extend_from_slice(self.send_ending.bytes());
        bytes
    }

    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(bytes);
        let mut lines = Vec::new();
        let ending = self.recv_ending.bytes();
        while let Some(index) = find_subslice(&self.buffer, ending) {
            let raw = self.buffer.drain(..index).collect::<Vec<_>>();
            self.buffer.drain(..ending.len());
            lines.push(String::from_utf8_lossy(&raw).to_string());
        }
        lines
    }
}

#[derive(Debug, Clone, Default)]
pub struct FixedBinaryCodec {
    frame_len: usize,
    buffer: Vec<u8>,
}

impl FixedBinaryCodec {
    pub fn new(frame_len: usize) -> Self {
        Self {
            frame_len,
            buffer: Vec::new(),
        }
    }

    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
        if self.frame_len == 0 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "binary frame length must be nonzero",
            ));
        }
        self.buffer.extend_from_slice(bytes);
        let mut frames = Vec::new();
        while self.buffer.len() >= self.frame_len {
            frames.push(self.buffer.drain(..self.frame_len).collect());
        }
        Ok(frames)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScriptedSerial {
    writes: Vec<Vec<u8>>,
    reads: VecDeque<Vec<u8>>,
}

impl ScriptedSerial {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_reads(reads: impl IntoIterator<Item = Vec<u8>>) -> Self {
        Self {
            writes: Vec::new(),
            reads: reads.into_iter().collect(),
        }
    }

    pub fn push_read(&mut self, bytes: impl Into<Vec<u8>>) {
        self.reads.push_back(bytes.into());
    }

    pub fn writes(&self) -> &[Vec<u8>] {
        &self.writes
    }
}

impl SerialIo for ScriptedSerial {
    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.writes.push(bytes.to_vec());
        Ok(())
    }

    fn read_available(&mut self) -> Result<Vec<u8>> {
        Ok(self.reads.pop_front().unwrap_or_default())
    }
}

#[cfg(feature = "os-serial")]
fn map_serial_error(error: serialport::Error) -> Error {
    Error::new(ErrorCode::Transport, error.to_string())
}

#[cfg(feature = "os-serial")]
fn map_io_error(error: std::io::Error) -> Error {
    Error::new(ErrorCode::Transport, error.to_string())
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
