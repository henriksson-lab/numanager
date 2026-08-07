//! Lumenera sensor geometry, read from the camera rather than compiled in.
//!
//! The vendor driver carries no per-model geometry table — it asks the device.
//! That is why this exists: hardcoding `1392x1040 / 12-bit` works for exactly one
//! model, and querying works for all 46 in the SDK's device list without
//! carrying a byte of per-model data.
//!
//! Confidence is not uniform here, and the type says so rather than flattening
//! it:
//!
//! * **Width and height are confirmed.** Register `0x1000` reads back as two
//!   little-endian `u16`; a Gel Doc EZ returned `0x04100570` = 1392 x 1040 on
//!   2026-08-05, matching both the dimension write and the captured frame size.
//! * **Bit depth is not.** `lucam-protocol-spec.md` names `0x01A0`
//!   `TRUE_PIXEL_DEPTH`, but its encoding has not been checked against hardware,
//!   so it is reported as a raw word alongside a best-effort interpretation and
//!   never silently substituted for a known-good value.
//! * `0x0010` (`SPECIFICATION`) is a protocol version and `0x019C`
//!   (`FORMAT_COUNT`) is a stream count. Both were initially mistaken for a
//!   capability bitfield and a descriptor table respectively; they are reported
//!   raw here and interpreted by their callers.
//!
//! An earlier revision of the driver read `0x1014` as bit depth because a camera
//! returned `0x0c`. A second reading returned `0x05`; it is a device state code
//! and the match was coincidence. That is the failure mode this module is shaped
//! to avoid.

use numanager_core::{Error, ErrorCode, Result};

fn err(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::Driver, msg)
}

/// Two little-endian `u16`: width then height. **Confirmed on hardware.**
pub const REG_DIMENSIONS: u16 = 0x1000;
/// `SPECIFICATION` — the camera's **protocol version**, not a capability
/// bitfield. Read once at bring-up and clamped to the highest version the host
/// implements; it gates the still-trigger encoding among other things.
pub const REG_SPECIFICATION: u16 = 0x0010;
/// `FORMAT_COUNT` — the number of **streams**, not a descriptor table. There is
/// no format-descriptor layout behind it. Load-bearing: a camera with one bulk
/// IN endpoint can still report two streams, so stream count must be taken from
/// here rather than inferred from the endpoint count.
pub const REG_FORMAT_COUNT: u16 = 0x019C;
/// `TRUE_PIXEL_DEPTH` — encoding unverified.
pub const REG_TRUE_PIXEL_DEPTH: u16 = 0x01A0;

/// Bit depths a sensor of this family plausibly reports. Used only to decide
/// whether a raw read is credible, never to invent a value.
const PLAUSIBLE_DEPTHS: [u8; 5] = [8, 10, 12, 14, 16];

/// What the camera says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Geometry {
    pub width: u32,
    pub height: u32,
    /// Interpreted bit depth, if the raw read was credible.
    pub bit_depth: Option<u8>,
    /// Raw `TRUE_PIXEL_DEPTH`, always reported so a caller can see what the
    /// interpretation was based on.
    pub raw_pixel_depth: u32,
    /// Protocol version, unclamped. Callers clamp to what they implement.
    pub raw_specification: u32,
    /// Number of streams the camera reports.
    pub format_count: u32,
}

impl Geometry {
    /// Bytes in one frame at this geometry and binning.
    ///
    /// Falls back to 16 bits per pixel when the depth read was not credible,
    /// which matches the observed frame size on the one camera that has been
    /// measured — a 12-bit sensor still ships two bytes per pixel.
    pub fn frame_bytes(&self, x_bin: u16, y_bin: u16) -> usize {
        let w = (self.width / x_bin.max(1) as u32) as usize;
        let h = (self.height / y_bin.max(1) as u32) as usize;
        let bytes_per_pixel = match self.bit_depth {
            Some(d) if d <= 8 => 1,
            _ => 2,
        };
        w * h * bytes_per_pixel
    }
}

/// Register reads, so the query is testable without a camera.
pub trait GeometryTransport {
    fn read_reg(&mut self, index: u16) -> Result<u32>;
}

/// Interprets `TRUE_PIXEL_DEPTH`.
///
/// The low byte is the obvious candidate and is accepted when it names a depth
/// this sensor family could actually have. Anything else returns `None` rather
/// than a guess — an invented depth silently corrupts every frame size derived
/// from it.
fn interpret_depth(raw: u32) -> Option<u8> {
    let low = (raw & 0xFF) as u8;
    PLAUSIBLE_DEPTHS.contains(&low).then_some(low)
}

/// Asks the camera for its geometry.
pub fn query<T: GeometryTransport>(io: &mut T) -> Result<Geometry> {
    let dims = io.read_reg(REG_DIMENSIONS)?;
    let width = dims & 0xFFFF;
    let height = dims >> 16;
    if width == 0 || height == 0 {
        return Err(err(format!(
            "Lumenera reported an impossible sensor size {width}x{height} (register {REG_DIMENSIONS:#06x} = {dims:#010x})"
        )));
    }

    // Diagnostics: a failure here must not stop a camera that reports a good
    // size, so these degrade to zero rather than propagating.
    let raw_pixel_depth = io.read_reg(REG_TRUE_PIXEL_DEPTH).unwrap_or(0);
    let raw_specification = io.read_reg(REG_SPECIFICATION).unwrap_or(0);
    let format_count = io.read_reg(REG_FORMAT_COUNT).unwrap_or(0);

    Ok(Geometry {
        width,
        height,
        bit_depth: interpret_depth(raw_pixel_depth),
        raw_pixel_depth,
        raw_specification,
        format_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Fake {
        regs: BTreeMap<u16, Result<u32>>,
    }

    impl Fake {
        fn with(mut self, i: u16, v: u32) -> Self {
            self.regs.insert(i, Ok(v));
            self
        }
        fn failing(mut self, i: u16) -> Self {
            self.regs.insert(i, Err(err("simulated read failure")));
            self
        }
    }

    impl GeometryTransport for Fake {
        fn read_reg(&mut self, index: u16) -> Result<u32> {
            match self.regs.get(&index) {
                Some(Ok(v)) => Ok(*v),
                Some(Err(e)) => Err(err(format!("{e}"))),
                None => Ok(0),
            }
        }
    }

    /// The exact word a Gel Doc EZ returned on 2026-08-05.
    #[test]
    fn decodes_the_hardware_confirmed_dimension_word() {
        let mut io = Fake::default()
            .with(REG_DIMENSIONS, 0x0410_0570)
            .with(REG_TRUE_PIXEL_DEPTH, 12);
        let g = query(&mut io).unwrap();
        assert_eq!((g.width, g.height), (1392, 1040));
        assert_eq!(g.bit_depth, Some(12));
    }

    /// The whole point: geometry comes from the device, so a different camera
    /// yields different numbers with no per-model data.
    #[test]
    fn a_different_camera_reports_different_geometry() {
        let mut io = Fake::default().with(REG_DIMENSIONS, (1024u32 << 16) | 1280);
        let g = query(&mut io).unwrap();
        assert_eq!((g.width, g.height), (1280, 1024));
    }

    /// An implausible depth must not be passed off as real. This is the trap the
    /// previous `0x1014` reading fell into.
    #[test]
    fn implausible_depth_is_reported_as_unknown_not_guessed() {
        let mut io = Fake::default()
            .with(REG_DIMENSIONS, 0x0410_0570)
            .with(REG_TRUE_PIXEL_DEPTH, 0x0000_000c_u32 ^ 0x0c ^ 0x07); // 7 bits
        let g = query(&mut io).unwrap();
        assert_eq!(g.bit_depth, None, "7 is not a depth this family reports");
        assert_eq!(g.raw_pixel_depth, 7, "but the raw value is still visible");
    }

    /// Diagnostics failing must not sink a camera that reports a good size.
    #[test]
    fn diagnostic_read_failures_do_not_fail_the_query() {
        let mut io = Fake::default()
            .with(REG_DIMENSIONS, 0x0410_0570)
            .failing(REG_SPECIFICATION)
            .failing(REG_FORMAT_COUNT);
        let g = query(&mut io).unwrap();
        assert_eq!((g.width, g.height), (1392, 1040));
        assert_eq!(g.raw_specification, 0);
    }

    /// A zero size is a failed read dressed up as data; refuse it.
    #[test]
    fn impossible_geometry_is_refused() {
        let mut io = Fake::default().with(REG_DIMENSIONS, 0);
        let e = query(&mut io).unwrap_err();
        assert!(format!("{e}").contains("impossible"), "{e}");
    }

    #[test]
    fn frame_bytes_tracks_binning_and_depth() {
        let g = Geometry {
            width: 1392,
            height: 1040,
            bit_depth: Some(12),
            raw_pixel_depth: 12,
            raw_specification: 0,
            format_count: 0,
        };
        // 12-bit still ships two bytes per pixel, matching the measured frame.
        assert_eq!(g.frame_bytes(1, 1), 1392 * 1040 * 2);
        assert_eq!(g.frame_bytes(2, 2), 696 * 520 * 2);

        let eight = Geometry { bit_depth: Some(8), ..g.clone() };
        assert_eq!(eight.frame_bytes(1, 1), 1392 * 1040);

        // Unknown depth falls back to two bytes, not to nothing.
        let unknown = Geometry { bit_depth: None, ..g };
        assert_eq!(unknown.frame_bytes(1, 1), 1392 * 1040 * 2);
    }
}
