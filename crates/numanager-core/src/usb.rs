use crate::{Error, ErrorCode, Result};
use std::collections::VecDeque;

pub trait UsbPacketIo: Send {
    fn write_packet(&mut self, bytes: &[u8]) -> Result<()>;
    fn read_packet(&mut self, len: usize) -> Result<Vec<u8>>;
}

#[derive(Debug, Clone)]
pub struct UsbDeviceIdentity {
    pub vendor_id: u16,
    pub product_id: u16,
    pub product_string: Option<String>,
    pub serial_number: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ScriptedUsbPacket {
    writes: Vec<Vec<u8>>,
    reads: VecDeque<Vec<u8>>,
}

impl ScriptedUsbPacket {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_reads(reads: impl IntoIterator<Item = Vec<u8>>) -> Self {
        Self {
            writes: Vec::new(),
            reads: reads.into_iter().collect(),
        }
    }

    pub fn push_read(&mut self, packet: impl Into<Vec<u8>>) {
        self.reads.push_back(packet.into());
    }

    pub fn writes(&self) -> &[Vec<u8>] {
        &self.writes
    }
}

impl UsbPacketIo for ScriptedUsbPacket {
    fn write_packet(&mut self, bytes: &[u8]) -> Result<()> {
        self.writes.push(bytes.to_vec());
        Ok(())
    }

    fn read_packet(&mut self, len: usize) -> Result<Vec<u8>> {
        if len == 0 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "USB packet length must be nonzero",
            ));
        }
        let mut packet = self.reads.pop_front().unwrap_or_default();
        packet.resize(len, 0);
        Ok(packet)
    }
}
