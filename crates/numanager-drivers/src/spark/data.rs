//! Decoder for the TDCL2 Data-channel scalar photometric format.
//!
//! A measurement result is two parts, bound to a request by the TDCL `seq` byte:
//! * a `0x88` **header** frame whose payload is an ordered list of 1-byte
//!   [`DataType`] codes, one per scalar field, and
//! * one or more `0x83` **payload** frames: a big-endian, tightly-packed
//!   concatenation of the field values in header order.
//!
//! This covers the ABS/FLUOR/LUM photometric readers. Camera **images do not use
//! this codec** — they travel a separate native uEye→OpenCV→TIFF path.

/// `TdclDataType` wire codes. The numeric value is the byte that appears in the
/// `0x88` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DataType {
    U16Rd = 0,
    U32Rd = 1,
    U16Md = 2,
    U16Md2 = 3,
    U16Md3 = 4,
    U16Md4 = 5,
    U16Md5 = 6,
    U16Md6 = 7,
    U16Md7 = 8,
    U16Md8 = 9,
    X100U16Temp = 10,
    X10U16Rwl = 11,
    U32Time = 12,
    U32Dark = 13,
    U32Md = 14,
    U8Ratio = 15,
    U16Att = 16,
    U16Gain = 17,
    U16Mult = 18,
    U16MultH = 19,
    U32MTime = 20,
    U16RdDark = 21,
    U16MdDark = 22,
    X10U16Mwl = 23,
    U16MGain = 24,
    U8Byte = 25,
    U16ReadCount = 26,
    U16RdHor = 27,
    U16MdHor = 28,
    U16RdVer = 29,
    U16MdVer = 30,
    U8MirPos = 31,
    U16Vib = 32,
}

impl DataType {
    pub fn from_u8(v: u8) -> Option<Self> {
        use DataType::*;
        Some(match v {
            0 => U16Rd,
            1 => U32Rd,
            2 => U16Md,
            3 => U16Md2,
            4 => U16Md3,
            5 => U16Md4,
            6 => U16Md5,
            7 => U16Md6,
            8 => U16Md7,
            9 => U16Md8,
            10 => X100U16Temp,
            11 => X10U16Rwl,
            12 => U32Time,
            13 => U32Dark,
            14 => U32Md,
            15 => U8Ratio,
            16 => U16Att,
            17 => U16Gain,
            18 => U16Mult,
            19 => U16MultH,
            20 => U32MTime,
            21 => U16RdDark,
            22 => U16MdDark,
            23 => X10U16Mwl,
            24 => U16MGain,
            25 => U8Byte,
            26 => U16ReadCount,
            27 => U16RdHor,
            28 => U16MdHor,
            29 => U16RdVer,
            30 => U16MdVer,
            31 => U8MirPos,
            32 => U16Vib,
            _ => return None,
        })
    }

    /// Width in bytes consumed from the `0x83` payload (0 for markers).
    pub fn width(self) -> usize {
        use DataType::*;
        match self {
            U8Ratio | U8Byte | U8MirPos => 1,
            U32Rd | U32Time | U32Dark | U32Md | U32MTime => 4,
            U16MultH => 0, // package-level marker, never a field
            _ => 2,        // all remaining scalars are u16
        }
    }

    /// Divisor applied to the raw integer to get the physical value
    /// (Temperature = raw/100 °C, Wavelength = raw/10 nm — the only scaling in
    /// this codec). All others are raw counts (divisor 1).
    pub fn divisor(self) -> u32 {
        match self {
            DataType::X100U16Temp => 100,
            DataType::X10U16Rwl | DataType::X10U16Mwl => 10,
            _ => 1,
        }
    }
}

/// One decoded scalar field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Field {
    pub type_: DataType,
    /// Raw big-endian integer as sent on the wire.
    pub raw: u32,
    /// Physical value = `raw / type.divisor` (°C, nm, or raw counts).
    pub value: f64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DataError {
    UnknownType(u8),
    Truncated {
        need: usize,
        have: usize,
    },
    /// `U16MULT_H` (multi-header) is a package-level construct spanning several
    /// header frames; decode it at the session layer, not here.
    MultiHeaderUnsupported,
}

/// Parse a `0x88` header payload into its list of field type codes.
pub fn parse_header(payload: &[u8]) -> Result<Vec<DataType>, DataError> {
    if payload.contains(&(DataType::U16MultH as u8)) {
        return Err(DataError::MultiHeaderUnsupported);
    }
    payload
        .iter()
        .map(|&b| DataType::from_u8(b).ok_or(DataError::UnknownType(b)))
        .collect()
}

fn take(buf: &mut &[u8], n: usize) -> Result<u32, DataError> {
    if buf.len() < n {
        return Err(DataError::Truncated {
            need: n,
            have: buf.len(),
        });
    }
    let (head, tail) = buf.split_at(n);
    *buf = tail;
    Ok(head.iter().fold(0u32, |v, &b| (v << 8) + b as u32)) // big-endian
}

fn decode_one(type_: DataType, buf: &mut &[u8]) -> Result<Field, DataError> {
    let raw = take(buf, type_.width())?;
    Ok(Field {
        type_,
        raw,
        value: raw as f64 / type_.divisor() as f64,
    })
}

/// Decode the concatenated `0x83` payload against a parsed header.
///
/// Handles the in-header `U16MULT` repeat: a `U16MULT` field carries a loop
/// count, and the fields *after* it repeat that many times (kinetic/multi-read
/// blocks). `U16MULT_H` multi-headers are rejected (session-layer concern).
pub fn decode(header: &[DataType], payload: &[u8]) -> Result<Vec<Field>, DataError> {
    let mut buf = payload;
    let mut out = Vec::new();
    let mut i = 0;
    while i < header.len() {
        let t = header[i];
        if t == DataType::U16Mult {
            let count = take(&mut buf, 2)?; // loop count
            let block = &header[i + 1..];
            for _ in 0..count {
                for &bt in block {
                    out.push(decode_one(bt, &mut buf)?);
                }
            }
            break; // the repeated block consumes the remainder of the schema
        }
        out.push(decode_one(t, &mut buf)?);
        i += 1;
    }
    Ok(out)
}
