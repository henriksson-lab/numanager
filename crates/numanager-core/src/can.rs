use crate::serial::{LineEnding, SerialIo, SerialLineCodec};
use crate::{Error, ErrorCode, Result};
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanFrame {
    pub id: u32,
    pub data: Vec<u8>,
    pub extended: bool,
}

impl CanFrame {
    pub fn standard(id: u32, data: impl Into<Vec<u8>>) -> Result<Self> {
        if id > 0x7FF {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                format!("standard CAN id out of range: 0x{id:X}"),
            ));
        }
        let data = data.into();
        if data.len() > 8 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "classic CAN frame payload must be at most 8 bytes",
            ));
        }
        Ok(Self {
            id,
            data,
            extended: false,
        })
    }
}

pub trait CanIo: Send {
    fn write_frame(&mut self, frame: &CanFrame) -> Result<()>;
    fn read_available(&mut self) -> Result<Vec<CanFrame>>;
}

#[derive(Debug, Clone, Default)]
pub struct ScriptedCan {
    writes: Vec<CanFrame>,
    reads: VecDeque<CanFrame>,
}

impl ScriptedCan {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_reads(reads: impl IntoIterator<Item = CanFrame>) -> Self {
        Self {
            writes: Vec::new(),
            reads: reads.into_iter().collect(),
        }
    }

    pub fn writes(&self) -> &[CanFrame] {
        &self.writes
    }
}

impl CanIo for ScriptedCan {
    fn write_frame(&mut self, frame: &CanFrame) -> Result<()> {
        self.writes.push(frame.clone());
        Ok(())
    }

    fn read_available(&mut self) -> Result<Vec<CanFrame>> {
        Ok(self.reads.drain(..).collect())
    }
}

pub struct SlcanIo {
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
}

impl SlcanIo {
    pub fn new(serial: Box<dyn SerialIo>) -> Self {
        Self {
            serial,
            codec: SerialLineCodec::new(LineEnding::Cr, LineEnding::Cr),
        }
    }

    pub fn with_setup(
        serial: Box<dyn SerialIo>,
        bitrate_code: Option<char>,
        open_channel: bool,
    ) -> Result<Self> {
        let mut io = Self::new(serial);
        if let Some(code) = bitrate_code {
            io.set_bitrate_code(code)?;
        }
        if open_channel {
            io.open_channel()?;
        }
        Ok(io)
    }

    pub fn set_bitrate_code(&mut self, code: char) -> Result<()> {
        if !matches!(code, '0'..='8') {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("SLCAN bitrate code must be S0..S8, got S{code}"),
            ));
        }
        self.serial.write(&self.codec.encode(&format!("S{code}")))?;
        let _ = self.serial.read_available()?;
        Ok(())
    }

    pub fn open_channel(&mut self) -> Result<()> {
        self.serial.write(&self.codec.encode("O"))?;
        let _ = self.serial.read_available()?;
        Ok(())
    }
}

impl CanIo for SlcanIo {
    fn write_frame(&mut self, frame: &CanFrame) -> Result<()> {
        if frame.extended {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "SLCAN backend supports standard 11-bit CAN frames only",
            ));
        }
        if frame.data.len() > 8 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "SLCAN classic frame payload must be at most 8 bytes",
            ));
        }
        let mut line = format!("t{:03X}{}", frame.id, frame.data.len());
        for byte in &frame.data {
            line.push_str(&format!("{byte:02X}"));
        }
        self.serial.write(&self.codec.encode(&line))
    }

    fn read_available(&mut self) -> Result<Vec<CanFrame>> {
        let bytes = self.serial.read_available()?;
        let mut frames = Vec::new();
        for line in self.codec.push(&bytes) {
            if line.is_empty() || line == "\u{7}" || line == "\r" {
                continue;
            }
            frames.push(parse_slcan_frame(&line)?);
        }
        Ok(frames)
    }
}

fn parse_slcan_frame(line: &str) -> Result<CanFrame> {
    let line = line.trim();
    let mut chars = line.chars();
    let kind = chars
        .next()
        .ok_or_else(|| Error::new(ErrorCode::Transport, "empty SLCAN line"))?;
    if kind != 't' {
        return Err(Error::new(
            ErrorCode::Transport,
            format!("unsupported SLCAN frame prefix {kind}"),
        ));
    }
    if line.len() < 5 {
        return Err(Error::new(
            ErrorCode::Transport,
            format!("short SLCAN frame {line}"),
        ));
    }
    let id = u32::from_str_radix(&line[1..4], 16).map_err(|error| {
        Error::new(
            ErrorCode::Transport,
            format!("invalid SLCAN standard id in {line}: {error}"),
        )
    })?;
    let len = usize::from_str_radix(&line[4..5], 16).map_err(|error| {
        Error::new(
            ErrorCode::Transport,
            format!("invalid SLCAN dlc in {line}: {error}"),
        )
    })?;
    let expected = 5 + len * 2;
    if line.len() < expected {
        return Err(Error::new(
            ErrorCode::Transport,
            format!("SLCAN frame payload is shorter than DLC: {line}"),
        ));
    }
    let mut data = Vec::with_capacity(len);
    for offset in (5..expected).step_by(2) {
        data.push(
            u8::from_str_radix(&line[offset..offset + 2], 16).map_err(|error| {
                Error::new(
                    ErrorCode::Transport,
                    format!("invalid SLCAN data byte in {line}: {error}"),
                )
            })?,
        );
    }
    CanFrame::standard(id, data)
}

#[cfg(all(feature = "os-can", target_os = "linux"))]
mod socketcan_linux {
    use super::*;
    use std::ffi::CString;
    use std::io;
    use std::mem::MaybeUninit;
    use std::os::fd::RawFd;
    use std::os::raw::{c_char, c_int, c_uint, c_void};

    const AF_CAN: c_int = 29;
    const PF_CAN: c_int = AF_CAN;
    const SOCK_RAW: c_int = 3;
    const CAN_RAW: c_int = 1;
    const F_GETFL: c_int = 3;
    const F_SETFL: c_int = 4;
    const O_NONBLOCK: c_int = 0x800;

    #[repr(C)]
    struct SockAddrCan {
        can_family: u16,
        can_ifindex: c_int,
        addr: [u8; 8],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct RawCanFrame {
        can_id: u32,
        can_dlc: u8,
        __pad: u8,
        __res0: u8,
        __res1: u8,
        data: [u8; 8],
    }

    extern "C" {
        fn socket(domain: c_int, ty: c_int, protocol: c_int) -> c_int;
        fn bind(fd: c_int, addr: *const SockAddrCan, len: u32) -> c_int;
        fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
        fn write(fd: c_int, buf: *const c_void, count: usize) -> isize;
        fn close(fd: c_int) -> c_int;
        fn fcntl(fd: c_int, cmd: c_int, arg: c_int) -> c_int;
        fn if_nametoindex(ifname: *const c_char) -> c_uint;
    }

    pub struct SocketCanIo {
        fd: RawFd,
        interface: String,
    }

    impl SocketCanIo {
        pub fn open(interface: impl Into<String>) -> Result<Self> {
            let interface = interface.into();
            let c_interface = CString::new(interface.as_str()).map_err(|error| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("invalid CAN interface name {interface}: {error}"),
                )
            })?;
            let ifindex = unsafe { if_nametoindex(c_interface.as_ptr()) };
            if ifindex == 0 {
                return Err(Error::new(
                    ErrorCode::Transport,
                    format!("CAN interface {interface} was not found"),
                ));
            }
            let fd = unsafe { socket(PF_CAN, SOCK_RAW, CAN_RAW) };
            if fd < 0 {
                return Err(map_io_error(io::Error::last_os_error()));
            }
            let addr = SockAddrCan {
                can_family: AF_CAN as u16,
                can_ifindex: ifindex as c_int,
                addr: [0; 8],
            };
            let rc = unsafe {
                bind(
                    fd,
                    &addr,
                    std::mem::size_of::<SockAddrCan>()
                        .try_into()
                        .expect("sockaddr size fits u32"),
                )
            };
            if rc < 0 {
                let error = io::Error::last_os_error();
                unsafe {
                    close(fd);
                }
                return Err(map_io_error(error));
            }
            let flags = unsafe { fcntl(fd, F_GETFL, 0) };
            if flags >= 0 {
                unsafe {
                    let _ = fcntl(fd, F_SETFL, flags | O_NONBLOCK);
                }
            }
            Ok(Self { fd, interface })
        }

        pub fn interface(&self) -> &str {
            &self.interface
        }
    }

    impl Drop for SocketCanIo {
        fn drop(&mut self) {
            unsafe {
                let _ = close(self.fd);
            }
        }
    }

    impl CanIo for SocketCanIo {
        fn write_frame(&mut self, frame: &CanFrame) -> Result<()> {
            if frame.extended {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    "SocketCAN backend supports standard 11-bit CAN frames only",
                ));
            }
            if frame.data.len() > 8 {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "classic CAN frame payload must be at most 8 bytes",
                ));
            }
            let mut raw = RawCanFrame {
                can_id: frame.id,
                can_dlc: frame.data.len() as u8,
                __pad: 0,
                __res0: 0,
                __res1: 0,
                data: [0; 8],
            };
            raw.data[..frame.data.len()].copy_from_slice(&frame.data);
            let n = unsafe {
                write(
                    self.fd,
                    (&raw as *const RawCanFrame).cast::<c_void>(),
                    std::mem::size_of::<RawCanFrame>(),
                )
            };
            if n < 0 {
                return Err(map_io_error(io::Error::last_os_error()));
            }
            Ok(())
        }

        fn read_available(&mut self) -> Result<Vec<CanFrame>> {
            let mut frames = Vec::new();
            loop {
                let mut raw = MaybeUninit::<RawCanFrame>::uninit();
                let n = unsafe {
                    read(
                        self.fd,
                        raw.as_mut_ptr().cast::<c_void>(),
                        std::mem::size_of::<RawCanFrame>(),
                    )
                };
                if n < 0 {
                    let error = io::Error::last_os_error();
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock
                            | io::ErrorKind::TimedOut
                            | io::ErrorKind::Interrupted
                    ) {
                        break;
                    }
                    return Err(map_io_error(error));
                }
                if n == 0 {
                    break;
                }
                if n as usize != std::mem::size_of::<RawCanFrame>() {
                    return Err(Error::new(
                        ErrorCode::Transport,
                        format!("short SocketCAN frame read: {n} bytes"),
                    ));
                }
                let raw = unsafe { raw.assume_init() };
                let dlc = raw.can_dlc.min(8) as usize;
                frames.push(CanFrame {
                    id: raw.can_id & 0x1FFF_FFFF,
                    data: raw.data[..dlc].to_vec(),
                    extended: raw.can_id & 0x8000_0000 != 0,
                });
            }
            Ok(frames)
        }
    }

    fn map_io_error(error: io::Error) -> Error {
        Error::new(ErrorCode::Transport, error.to_string())
    }
}

#[cfg(all(feature = "os-can", target_os = "linux"))]
pub use socketcan_linux::SocketCanIo;
