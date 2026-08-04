//! TDCL 2.0 wire-framing codec for the Tecan Spark Cyto reader.
//!
//! See `../../PROTOCOL.md` §4 for the full frame-layout specification.
//!
//! Frame layout (both directions):
//! ```text
//! +--------+--------+---------+---------+=... =+----------+
//! | type | seq | len_hi | len_lo | payload | checksum |
//! | 1 byte | 1 byte | 2 bytes (big-endian) | len B | 1 byte |
//! +--------+--------+---------+---------+=... =+----------+
//! ```
//! * `len` = big-endian length of `payload` only (header + checksum excluded)
//! * `checksum` = 8-bit XOR fold of every byte before it (header + payload)
//! * `type` high bit (0x80) set => frame is device -> host

/// A 16-bit length field caps the payload of a single frame.
pub const MAX_PAYLOAD: usize = 65535;
/// `SendDataPayload` chunks binary data at 65530 = 65535 − 4 header − 1 checksum.
pub const MAX_BINARY_CHUNK: usize = 65530;

/// TDCL frame type byte. High bit set => device → host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameType {
    // host -> device (requests); ReadTdclFrame switches on (type-1) for 0,1,2.
    AsciiCommand = 0x01,
    TerminateReq = 0x02,
    BinaryReq = 0x03,
    // device -> host (responses), high bit set.
    Ready = 0x81,
    Terminate = 0x82,
    Binary = 0x83,
    Busy = 0x84,
    Message = 0x85,
    Error = 0x86,
    Log = 0x87,
    DataHeader = 0x88,
    AsyncError = 0x89,
}

impl FrameType {
    pub fn from_u8(v: u8) -> Option<Self> {
        use FrameType::*;
        Some(match v {
            0x01 => AsciiCommand,
            0x02 => TerminateReq,
            0x03 => BinaryReq,
            0x81 => Ready,
            0x82 => Terminate,
            0x83 => Binary,
            0x84 => Busy,
            0x85 => Message,
            0x86 => Error,
            0x87 => Log,
            0x88 => DataHeader,
            0x89 => AsyncError,
            _ => return None,
        })
    }

    /// True if the high bit is set (device → host).
    pub fn from_device(self) -> bool {
        (self as u8) & 0x80 != 0
    }
}

/// 8-bit XOR fold of every byte in the frame.
pub fn xor_checksum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |c, &b| c ^ b)
}

/// Big-endian fixed-width int (MSB first).
///
/// The firmware silently truncates to the low `count` bytes; we mirror that.
/// `count` is 1..=4 in practice (len=2, timestamp=4, etc.).
pub fn int_to_bytes(count: usize, value: u32) -> Vec<u8> {
    (0..count)
        .rev()
        .map(|i| ((value >> (8 * i as u32)) & 0xFF) as u8)
        .collect()
}

/// Big-endian decode (`result = (result<<8)+byte`).
pub fn read_int(data: &[u8]) -> u32 {
    data.iter().fold(0u32, |v, &b| (v << 8) + b as u32)
}

/// Errors from decoding a frame off the wire.
#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Not enough bytes yet for a whole frame; need at least `needed` total.
    Short { needed: usize },
    /// Trailing checksum byte mismatch ("Checksum mismatch in received command").
    Checksum { got: u8, want: u8 },
}

/// One decoded/encodable TDCL frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub type_: u8,
    pub seq: u8,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(type_: u8, seq: u8, payload: impl Into<Vec<u8>>) -> Self {
        Frame {
            type_,
            seq,
            payload: payload.into(),
        }
    }

    /// Serialize this frame including its trailing XOR checksum.
    pub fn encode(&self) -> Vec<u8> {
        assert!(
            self.payload.len() <= MAX_PAYLOAD,
            "payload too long; chunk it"
        );
        let mut frame = Vec::with_capacity(4 + self.payload.len() + 1);
        frame.push(self.type_);
        frame.push(self.seq);
        frame.extend_from_slice(&int_to_bytes(2, self.payload.len() as u32));
        frame.extend_from_slice(&self.payload);
        let ck = xor_checksum(&frame);
        frame.push(ck);
        frame
    }

    /// Decode the first frame in `buf`; returns `(frame, bytes_consumed)`.
    pub fn decode(buf: &[u8]) -> Result<(Frame, usize), DecodeError> {
        if buf.len() < 4 {
            return Err(DecodeError::Short { needed: 4 });
        }
        let type_ = buf[0];
        let seq = buf[1];
        let length = read_int(&buf[2..4]) as usize;
        let total = 4 + length + 1;
        if buf.len() < total {
            return Err(DecodeError::Short { needed: total });
        }
        let payload = buf[4..4 + length].to_vec();
        let got = buf[4 + length];
        let want = xor_checksum(&buf[..4 + length]);
        if got != want {
            return Err(DecodeError::Checksum { got, want });
        }
        Ok((
            Frame {
                type_,
                seq,
                payload,
            },
            total,
        ))
    }
}

// ---- host -> device request builders --------------------------------------

/// 0x01 Ascii command frame carrying a command line (see `commands`).
pub fn ascii_command(seq: u8, command: &str) -> Vec<u8> {
    Frame::new(
        FrameType::AsciiCommand as u8,
        seq,
        command.as_bytes().to_vec(),
    )
    .encode()
}

/// 0x02 Terminate — abort the running command. Payload = 4-byte BE time.
pub fn terminate_request(seq: u8, time: u32) -> Vec<u8> {
    Frame::new(FrameType::TerminateReq as u8, seq, int_to_bytes(4, time)).encode()
}

// ---- device -> host response interpretation -------------------------------

/// A device → host frame with its payload interpreted per PROTOCOL.md §4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub type_: FrameType,
    pub seq: u8,
    pub text: String,
    pub number: Option<u32>,
    pub ticks: Option<u32>,
    pub data: Vec<u8>,
}

impl Response {
    fn base(type_: FrameType, seq: u8) -> Self {
        Response {
            type_,
            seq,
            text: String::new(),
            number: None,
            ticks: None,
            data: Vec::new(),
        }
    }
}

/// Interpret a device → host frame's payload. Returns `None` for an unknown
/// type byte (caller can inspect the raw `Frame`).
pub fn parse_response(frame: &Frame) -> Option<Response> {
    let t = FrameType::from_u8(frame.type_)?;
    let p = &frame.payload;
    let mut r = Response::base(t, frame.seq);
    let ascii = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    match t {
        FrameType::Ready | FrameType::Log => r.text = ascii(p),
        FrameType::Message => {
            r.number = Some(read_int(&p[..2.min(p.len())]));
            r.text = ascii(p.get(2..).unwrap_or(&[]));
        }
        FrameType::Busy => r.number = Some(read_int(&p[..2.min(p.len())])),
        FrameType::Terminate => r.number = Some(read_int(&p[..4.min(p.len())])),
        FrameType::Error | FrameType::AsyncError => {
            r.ticks = Some(read_int(p.get(..4).unwrap_or(&[])));
            r.number = Some(read_int(p.get(4..6).unwrap_or(&[])));
            r.text = ascii(p.get(6..).unwrap_or(&[]));
        }
        FrameType::Binary | FrameType::DataHeader => r.data = p.clone(),
        _ => r.data = p.clone(),
    }
    Some(r)
}

/// Incremental frame decoder over a byte stream (pipe / bulk endpoint).
#[derive(Default)]
pub struct FrameStream {
    buf: Vec<u8>,
}

impl FrameStream {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append bytes and drain all complete frames. On a checksum error the
    /// offending frame's bytes are consumed and the error is returned so the
    /// caller can decide whether to resync or abort.
    pub fn feed(&mut self, data: &[u8]) -> Result<Vec<Frame>, DecodeError> {
        self.buf.extend_from_slice(data);
        let mut out = Vec::new();
        loop {
            match Frame::decode(&self.buf) {
                Ok((f, n)) => {
                    self.buf.drain(..n);
                    out.push(f);
                }
                Err(DecodeError::Short { .. }) => return Ok(out),
                Err(e) => return Err(e),
            }
        }
    }
}
