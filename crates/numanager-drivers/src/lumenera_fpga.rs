//! Lumenera FPGA bring-up: the bitstream store and the programming handshake.
//!
//! A Lumenera camera is an EZ-USB bridge in front of an FPGA, and **neither
//! streams until the FPGA is programmed**. Bring-up is therefore two downloads,
//! not one: the 8051 firmware (see [`crate::lumenera`]) and then a bitstream
//! chosen by the camera's product and device id.
//!
//! Which bitstream is not a free choice. A model carries several, the right set
//! depends on the device id, and each one has a *program code* that must be
//! written before its data. The Lu130 alone needs one bitstream at device id
//! `0x0000`, a different one at `0x0010`, and **two in sequence** at `0x0018`.
//! Hardcoding a single captured blob works only on the revision it was captured
//! from — which is what this module replaces.
//!
//! The device id is **`bcdDevice` from the USB device descriptor**, read from
//! the *imaging-stage* device after renumeration — not the loader's. The same
//! field selects the 8051 firmware image at the loader stage, so one revision
//! number is used twice for different purposes.
//!
//! The store is a single container built by `reveng-dll/tools` `lucam_fpga
//! --pack`. Content-identical bitstreams are shared between models, so 108
//! (pid, did) references resolve to 38 distinct blobs.
//!
//! Protocol and sequence are specified in `reveng-dll/teledyne/
//! lucam-protocol-spec.md` §5. This is an independent implementation from that
//! specification.

use numanager_core::{Error, ErrorCode, Result};

fn err(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::Driver, msg)
}

/// Vendor request that reads and writes the camera register file.
pub const REQUEST_REGISTER: u8 = 0x12;

/// `FPGA_MODE` — the whole programming handshake runs through this register.
pub const REG_FPGA_MODE: u16 = 0x0008;

/// FPGA is programmed and ready.
pub const FPGA_PROGRAMMED: u32 = 0x20;
/// Programming is in progress.
pub const FPGA_BUSY: u32 = 0x40;

/// Payload chunk size. The final chunk is sent **short** — the staging buffer is
/// zeroed, but the transfer length is the remaining byte count, so the padding
/// is never on the wire.
///
/// An earlier revision of this module padded to the full 512 bytes and asserted
/// it in a test. That was wrong, and wrong in the direction that sends the
/// camera bytes the vendor never sends it.
pub const FPGA_CHUNK: usize = 512;

const MAGIC: &[u8; 8] = b"LUFPGA01";

/// Matches any product or device id in a stored key.
pub const WILDCARD_ID: u16 = 0xFFFF;

/// One bitstream to program, in the order the camera expects it.
#[derive(Debug, Clone)]
pub struct Bitstream {
    /// Written to `FPGA_MODE` before the data. Per-bitstream, not a constant.
    pub code: Option<u32>,
    pub data: Vec<u8>,
}

/// The packed bitstream store.
///
/// Blobs stay compressed until something asks for them: the whole container is
/// 2.4 MB, and a given camera needs at most a few hundred KB of it.
pub struct BitstreamStore {
    /// (pid, did, order, code, blob index), sorted so a lookup is a scan of one
    /// contiguous run.
    entries: Vec<(u16, u16, u8, Option<u32>, u32)>,
    /// (raw length, offset into `payload`, compressed length)
    blobs: Vec<(usize, usize, usize)>,
    payload: Vec<u8>,
}

fn rd_u32(b: &[u8], at: usize) -> Result<u32> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| err("Lumenera bitstream store is truncated"))
}

fn rd_u16(b: &[u8], at: usize) -> Result<u16> {
    b.get(at..at + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| err("Lumenera bitstream store is truncated"))
}

impl BitstreamStore {
    /// Parses a container. Only the index is read here; blobs stay packed.
    pub fn parse(raw: Vec<u8>) -> Result<Self> {
        if raw.len() < 12 || &raw[..8] != MAGIC {
            return Err(err(
                "not a Lumenera bitstream store (bad magic); rebuild it with `lucam_fpga --pack`",
            ));
        }
        let n_entries = rd_u32(&raw, 8)? as usize;
        let mut at = 12;
        let mut entries = Vec::with_capacity(n_entries);
        for _ in 0..n_entries {
            let pid = rd_u16(&raw, at)?;
            let did = rd_u16(&raw, at + 2)?;
            let order = *raw
                .get(at + 4)
                .ok_or_else(|| err("Lumenera bitstream store is truncated"))?;
            let has_code = *raw
                .get(at + 5)
                .ok_or_else(|| err("Lumenera bitstream store is truncated"))?;
            let code = rd_u32(&raw, at + 6)?;
            let blob = rd_u32(&raw, at + 10)?;
            entries.push((
                pid,
                did,
                order,
                if has_code != 0 { Some(code) } else { None },
                blob,
            ));
            at += 14;
        }

        let n_blobs = rd_u32(&raw, at)? as usize;
        at += 4;
        let mut sizes = Vec::with_capacity(n_blobs);
        for _ in 0..n_blobs {
            sizes.push((rd_u32(&raw, at)? as usize, rd_u32(&raw, at + 4)? as usize));
            at += 8;
        }
        let mut blobs = Vec::with_capacity(n_blobs);
        let mut off = 0usize;
        for (rawlen, complen) in sizes {
            blobs.push((rawlen, off, complen));
            off += complen;
        }
        if at + off > raw.len() {
            return Err(err(
                "Lumenera bitstream store is truncated (payload shorter than its index)",
            ));
        }
        let payload = raw[at..].to_vec();

        entries.sort_unstable_by_key(|(pid, did, order, _, _)| (*pid, *did, *order));
        Ok(Self {
            entries,
            blobs,
            payload,
        })
    }

    pub fn load(path: &std::path::Path) -> Result<Self> {
        let raw = std::fs::read(path).map_err(|e| {
            err(format!(
                "cannot read the Lumenera bitstream store at {}: {e}",
                path.display()
            ))
        })?;
        Self::parse(raw)
    }

    /// Bitstreams for a camera, in programming order.
    ///
    /// `0xFFFF` in either key position of a stored entry is a **wildcard**: a
    /// catch-all covering any product or device id. Matching exactly would miss
    /// those entries entirely, and a camera that should have been covered would
    /// look unsupported.
    ///
    /// An empty result is not an error at this level: an entry may legitimately
    /// carry no bitstreams, and some models have no FPGA at all.
    pub fn bitstreams_for(&self, pid: u16, did: u16) -> Result<Vec<Bitstream>> {
        let matches = |stored: u16, want: u16| stored == want || stored == WILDCARD_ID;
        let mut out = Vec::new();
        for (p, d, _, code, blob) in &self.entries {
            if !matches(*p, pid) || !matches(*d, did) {
                continue;
            }
            let (rawlen, off, complen) = *self
                .blobs
                .get(*blob as usize)
                .ok_or_else(|| err("Lumenera bitstream store index is out of range"))?;
            let packed = self
                .payload
                .get(off..off + complen)
                .ok_or_else(|| err("Lumenera bitstream store payload is truncated"))?;
            let data = zstd::decode_all(packed)
                .map_err(|e| err(format!("cannot decompress a Lumenera bitstream: {e}")))?;
            if data.len() != rawlen {
                return Err(err(format!(
                    "Lumenera bitstream decompressed to {} bytes, expected {rawlen}",
                    data.len()
                )));
            }
            out.push(Bitstream { code: *code, data });
        }
        Ok(out)
    }

    /// Device ids this product id is known to carry bitstreams for. Used to say
    /// something useful when a camera reports one the store does not cover.
    pub fn known_device_ids(&self, pid: u16) -> Vec<u16> {
        let mut v: Vec<u16> = self
            .entries
            .iter()
            .filter(|(p, ..)| *p == pid)
            .map(|(_, d, ..)| *d)
            .collect();
        v.dedup();
        v
    }
}

/// Splits a bitstream into the chunks the camera is fed. The last one is short
/// when the length is not a multiple of [`FPGA_CHUNK`].
pub fn chunks(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    data.chunks(FPGA_CHUNK)
}

/// What programming needs from a transport, so the sequence can be exercised
/// without a camera. The driver supplies the USB implementation.
pub trait FpgaTransport {
    /// Read a 32-bit camera register (`bRequest 0x12`, `wIndex` = index).
    fn read_reg(&mut self, index: u16) -> Result<u32>;
    /// Write a 32-bit camera register.
    fn write_reg(&mut self, index: u16, value: u32) -> Result<()>;
    /// Send one full-size chunk on the FPGA bulk OUT pipe.
    fn send_chunk(&mut self, chunk: &[u8]) -> Result<()>;
    /// Select the alternate interface setting used for FPGA download / idle.
    fn set_alt_setting(&mut self, alt: AltSetting) -> Result<()>;
    /// Pause. Separated out so tests do not actually sleep.
    fn delay(&mut self, d: std::time::Duration);
}

/// The interface settings programming moves between. Their numeric values are
/// discovered during endpoint enumeration, not constants, so they are named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AltSetting {
    Idle,
    Fpga,
    /// Streaming. Like `Fpga`, its numeric value is discovered during endpoint
    /// enumeration rather than being a constant.
    Data,
}

/// Programming outcome, so a caller can tell "already done" from "just done".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Programmed {
    /// Bit 5 was already set — the camera has not been power-cycled.
    AlreadyDone,
    /// Bitstreams were sent.
    Completed { bitstreams: usize },
    /// The model carries no FPGA; nothing to do.
    NotApplicable,
}

/// Poll attempts while waiting for one bitstream to finish programming.
const PROGRAM_POLLS: usize = 10;
/// Settle time after writing a program code. Present because some hosts are
/// otherwise faster than the camera.
const CODE_SETTLE: std::time::Duration = std::time::Duration::from_millis(2);
/// Pause between payload chunks.
const CHUNK_GAP: std::time::Duration = std::time::Duration::from_millis(1);
/// Poll interval while waiting for the busy bit to clear.
const POLL_GAP: std::time::Duration = std::time::Duration::from_millis(2);

/// Programs the FPGA, per `lucam-protocol-spec.md` §5.2.
///
/// Returns without touching the device if it is already programmed, which is the
/// common path for a camera that has not been power-cycled.
pub fn program<T: FpgaTransport>(io: &mut T, bitstreams: &[Bitstream]) -> Result<Programmed> {
    if bitstreams.is_empty() {
        return Ok(Programmed::NotApplicable);
    }
    if io.read_reg(REG_FPGA_MODE)? & FPGA_PROGRAMMED != 0 {
        return Ok(Programmed::AlreadyDone);
    }

    io.set_alt_setting(AltSetting::Fpga)?;

    // Anything after this point must restore the idle setting, including on the
    // error paths, or the interface is left where streaming cannot use it.
    let outcome = (|| -> Result<()> {
        for (n, bs) in bitstreams.iter().enumerate() {
            if let Some(code) = bs.code {
                io.write_reg(REG_FPGA_MODE, code)?;
                io.delay(CODE_SETTLE);
            }
            for chunk in chunks(&bs.data) {
                io.send_chunk(chunk)?;
                io.delay(CHUNK_GAP);
            }

            let mut done = false;
            for _ in 0..PROGRAM_POLLS {
                io.delay(POLL_GAP);
                if io.read_reg(REG_FPGA_MODE)? & FPGA_BUSY == 0 {
                    done = true;
                    break;
                }
            }
            if !done {
                return Err(err(format!(
                    "Lumenera FPGA bitstream {} of {} did not finish programming after \
                     {PROGRAM_POLLS} polls",
                    n + 1,
                    bitstreams.len()
                )));
            }
        }

        // Back to normal mode, then confirm it actually took.
        io.write_reg(REG_FPGA_MODE, 0)?;
        if io.read_reg(REG_FPGA_MODE)? & FPGA_PROGRAMMED == 0 {
            return Err(err(
                "Lumenera FPGA reports not programmed after the whole bitstream set was sent",
            ));
        }
        Ok(())
    })();

    let restored = io.set_alt_setting(AltSetting::Idle);
    outcome?;
    restored?;
    Ok(Programmed::Completed {
        bitstreams: bitstreams.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a container the same way the packer does, so the reader is tested
    /// against the format rather than against itself.
    fn pack(entries: &[(u16, u16, u8, Option<u32>, u32)], blobs: &[Vec<u8>]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        for (pid, did, order, code, blob) in entries {
            b.extend_from_slice(&pid.to_le_bytes());
            b.extend_from_slice(&did.to_le_bytes());
            b.push(*order);
            b.push(code.is_some() as u8);
            b.extend_from_slice(&code.unwrap_or(0).to_le_bytes());
            b.extend_from_slice(&blob.to_le_bytes());
        }
        let packed: Vec<Vec<u8>> = blobs
            .iter()
            .map(|x| zstd::encode_all(x.as_slice(), 1).unwrap())
            .collect();
        b.extend_from_slice(&(blobs.len() as u32).to_le_bytes());
        for (r, c) in blobs.iter().zip(packed.iter()) {
            b.extend_from_slice(&(r.len() as u32).to_le_bytes());
            b.extend_from_slice(&(c.len() as u32).to_le_bytes());
        }
        for c in &packed {
            b.extend_from_slice(c);
        }
        b
    }

    #[test]
    fn resolves_a_revision_to_its_bitstreams_in_order() {
        let blobs = vec![vec![0xAAu8; 100], vec![0xBBu8; 200]];
        // The Lu130 shape: one bitstream at did 0x0000, two at did 0x0018.
        let store = BitstreamStore::parse(pack(
            &[
                (0x009a, 0x0000, 0, Some(0xff), 0),
                (0x009a, 0x0018, 0, Some(0x01), 0),
                (0x009a, 0x0018, 1, Some(0x02), 1),
            ],
            &blobs,
        ))
        .expect("parse");

        let one = store.bitstreams_for(0x009a, 0x0000).unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].code, Some(0xff));
        assert_eq!(one[0].data.len(), 100);

        let two = store.bitstreams_for(0x009a, 0x0018).unwrap();
        assert_eq!(two.len(), 2, "this revision needs two bitstreams");
        assert_eq!(
            (two[0].code, two[1].code),
            (Some(0x01), Some(0x02)),
            "program codes must arrive in order"
        );
        assert_eq!(two[1].data.len(), 200);
    }

    /// A camera reporting an unknown revision must not be silently given the
    /// wrong bitstream — the caller gets nothing and can say so.
    #[test]
    fn unknown_revision_yields_nothing() {
        let store = BitstreamStore::parse(pack(
            &[(0x009a, 0x0000, 0, Some(0xff), 0)],
            &[vec![1, 2, 3]],
        ))
        .unwrap();
        assert!(store.bitstreams_for(0x009a, 0xbeef).unwrap().is_empty());
        assert_eq!(store.known_device_ids(0x009a), vec![0x0000]);
    }

    #[test]
    fn rejects_a_container_it_does_not_understand() {
        assert!(BitstreamStore::parse(b"not a store at all".to_vec()).is_err());
        let mut good = pack(&[(1, 2, 0, None, 0)], &[vec![7; 10]]);
        good.truncate(good.len() - 4);
        assert!(BitstreamStore::parse(good).is_err(), "truncation must fail");
    }

    /// The final chunk is **short**, not padded. An earlier revision asserted
    /// padding; the vendor zeroes its staging buffer but transfers only the
    /// remaining byte count, so the padding never reaches the camera.
    #[test]
    fn final_chunk_is_short_not_padded() {
        let data = vec![0xEEu8; FPGA_CHUNK + 3];
        let c: Vec<&[u8]> = chunks(&data).collect();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].len(), FPGA_CHUNK);
        assert_eq!(c[1].len(), 3, "the tail is sent as 3 bytes, not 512");
    }

    #[test]
    fn exact_multiple_needs_no_extra_chunk() {
        assert_eq!(chunks(&vec![1u8; FPGA_CHUNK * 2]).count(), 2);
        assert_eq!(chunks(&[]).count(), 0);
    }

    /// A wildcard entry must be found, or catch-all coverage silently vanishes.
    #[test]
    fn wildcard_entries_match_any_id() {
        let store = BitstreamStore::parse(pack(
            &[(0x009a, WILDCARD_ID, 0, Some(0xff), 0)],
            &[vec![9u8; 32]],
        ))
        .unwrap();
        assert_eq!(store.bitstreams_for(0x009a, 0x1234).unwrap().len(), 1);
        assert_eq!(store.bitstreams_for(0x009a, 0x0000).unwrap().len(), 1);
        assert!(store.bitstreams_for(0x00ff, 0x0000).unwrap().is_empty());
    }
}

#[cfg(test)]
mod real_store_tests {
    use super::*;

    fn store() -> Option<BitstreamStore> {
        let p = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/third_party/lumenera/lucam-fpga.lufpga"
        ));
        p.is_file()
            .then(|| BitstreamStore::load(p).expect("real store must parse"))
    }

    /// The shipped container, not a synthetic one. Skipped where the
    /// non-redistributable blobs are absent.
    #[test]
    fn lu130_resolves_every_revision_it_ships() {
        let Some(s) = store() else { return };

        // did 0x0000: one bitstream, 98023 bytes — the blob the previous
        // implementation hardcoded as a captured "98 KB pipeline image".
        let a = s.bitstreams_for(0x009a, 0x0000).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].data.len(), 98023);
        assert_eq!(a[0].code, Some(0xff));

        // did 0x0018: two, in order, with distinct program codes. A driver that
        // programs only the first leaves the FPGA half-configured.
        let c = s.bitstreams_for(0x009a, 0x0018).unwrap();
        assert_eq!(c.len(), 2, "this revision needs two bitstreams");
        assert_eq!(c[0].code, Some(0x01));
        assert_eq!(c[1].code, Some(0x02));
        assert_eq!((c[0].data.len(), c[1].data.len()), (158224, 169216));

        assert_eq!(s.known_device_ids(0x009a), vec![0x0000, 0x0010, 0x0018]);
    }

    /// Every entry in the shipped store must decompress to its recorded length.
    #[test]
    fn every_shipped_bitstream_round_trips() {
        let Some(s) = store() else { return };
        let mut seen = 0usize;
        for pid in s
            .entries
            .iter()
            .map(|(p, ..)| *p)
            .collect::<std::collections::BTreeSet<_>>()
        {
            for did in s.known_device_ids(pid) {
                for b in s.bitstreams_for(pid, did).unwrap() {
                    assert!(!b.data.is_empty());
                    seen += 1;
                }
            }
        }
        assert!(seen >= 100, "expected the full reference set, saw {seen}");
    }
}

#[cfg(test)]
mod program_tests {
    use super::*;
    use std::time::Duration;

    #[derive(Default)]
    struct Fake {
        mode: u32,
        /// busy countdown: reads of FPGA_MODE return busy this many more times
        busy_for: u32,
        /// set once enough chunks have arrived to count as programmed
        program_on_finish: bool,
        log: Vec<String>,
        chunks: usize,
        last_chunk_len: usize,
        fail_chunk_at: Option<usize>,
    }

    impl FpgaTransport for Fake {
        fn read_reg(&mut self, index: u16) -> Result<u32> {
            assert_eq!(index, REG_FPGA_MODE);
            if self.busy_for > 0 {
                self.busy_for -= 1;
                return Ok(FPGA_BUSY);
            }
            Ok(self.mode)
        }
        fn write_reg(&mut self, index: u16, value: u32) -> Result<()> {
            assert_eq!(index, REG_FPGA_MODE);
            self.log.push(format!("code:{value:#x}"));
            if value == 0 && self.program_on_finish {
                self.mode = FPGA_PROGRAMMED;
            }
            Ok(())
        }
        fn send_chunk(&mut self, chunk: &[u8]) -> Result<()> {
            assert!(chunk.len() <= FPGA_CHUNK);
            self.last_chunk_len = chunk.len();
            self.chunks += 1;
            if Some(self.chunks) == self.fail_chunk_at {
                return Err(err("simulated bulk failure"));
            }
            Ok(())
        }
        fn set_alt_setting(&mut self, alt: AltSetting) -> Result<()> {
            self.log.push(format!("alt:{alt:?}"));
            Ok(())
        }
        fn delay(&mut self, _d: Duration) {}
    }

    fn bs(code: u32, len: usize) -> Bitstream {
        Bitstream {
            code: Some(code),
            data: vec![0xA5; len],
        }
    }

    /// The common path: a camera that is already programmed must not be
    /// reprogrammed, and must not have its interface setting disturbed.
    #[test]
    fn already_programmed_is_a_no_op() {
        let mut io = Fake {
            mode: FPGA_PROGRAMMED,
            ..Default::default()
        };
        assert_eq!(
            program(&mut io, &[bs(0xff, 10)]).unwrap(),
            Programmed::AlreadyDone
        );
        assert!(io.log.is_empty(), "must not touch the device: {:?}", io.log);
        assert_eq!(io.chunks, 0);
    }

    #[test]
    fn no_bitstreams_means_no_fpga() {
        let mut io = Fake::default();
        assert_eq!(program(&mut io, &[]).unwrap(), Programmed::NotApplicable);
        assert!(io.log.is_empty());
    }

    /// Two bitstreams, each with its own code, in order — the Lu130 did 0x0018
    /// shape. Order and codes are the thing a hardcoded blob gets wrong.
    #[test]
    fn programs_every_bitstream_in_order_with_its_own_code() {
        let mut io = Fake {
            program_on_finish: true,
            ..Default::default()
        };
        let out = program(&mut io, &[bs(0x01, FPGA_CHUNK), bs(0x02, FPGA_CHUNK * 2)]).unwrap();
        assert_eq!(out, Programmed::Completed { bitstreams: 2 });
        assert_eq!(io.chunks, 3, "1 + 2 full chunks");
        assert_eq!(
            io.log,
            vec!["alt:Fpga", "code:0x1", "code:0x2", "code:0x0", "alt:Idle"],
            "codes in order, normal mode at the end, idle restored"
        );
    }

    /// A short bitstream is padded, never sent short — asserted in send_chunk.
    /// A sub-chunk bitstream goes out as one short transfer.
    #[test]
    fn short_bitstream_is_sent_short() {
        let mut io = Fake {
            program_on_finish: true,
            ..Default::default()
        };
        program(&mut io, &[bs(0xff, 3)]).unwrap();
        assert_eq!(io.chunks, 1);
        assert_eq!(io.last_chunk_len, 3, "must not be padded to 512");
    }

    /// Busy must be waited out, not raced.
    #[test]
    fn waits_for_the_busy_bit_to_clear() {
        let mut io = Fake {
            program_on_finish: true,
            busy_for: 5,
            ..Default::default()
        };
        assert!(program(&mut io, &[bs(0xff, 8)]).is_ok());
    }

    /// If the camera never reports programmed, that is a failure, not success.
    #[test]
    fn silent_programming_failure_is_reported() {
        let mut io = Fake {
            program_on_finish: false,
            ..Default::default()
        };
        let e = program(&mut io, &[bs(0xff, 8)]).unwrap_err();
        assert!(format!("{e}").contains("not programmed"), "{e}");
        assert_eq!(
            io.log.last().unwrap(),
            "alt:Idle",
            "idle must still be restored"
        );
    }

    /// A mid-transfer failure must still put the interface back, or streaming
    /// cannot claim it afterwards.
    #[test]
    fn restores_idle_setting_even_when_the_transfer_fails() {
        let mut io = Fake {
            fail_chunk_at: Some(2),
            ..Default::default()
        };
        assert!(program(&mut io, &[bs(0xff, FPGA_CHUNK * 4)]).is_err());
        assert_eq!(io.log.last().unwrap(), "alt:Idle");
    }
}
