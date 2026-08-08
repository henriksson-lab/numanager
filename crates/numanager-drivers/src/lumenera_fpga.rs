// SPDX-License-Identifier: GPL-2.0-only
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
//! The programming sequence is derived from Teledyne's GPLv2 Linux SDK driver
//! (`lucam.c`, `lucam_fpga_setup`, USB 2 path). This file is therefore
//! annotated `GPL-2.0-only`.

// A pointer where the store was expected is not corruption but an unfetched LFS
// object — `crate::lfs` owns that detection and its wording.
use crate::lfs;
use numanager_core::{Error, ErrorCode, Result};

fn err(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::Driver, msg)
}

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
    pub code: u32,
    pub data: Vec<u8>,
}

/// The packed bitstream store.
///
/// Blobs stay compressed until something asks for them: the whole container is
/// 2.4 MB, and a given camera needs at most a few hundred KB of it.
pub struct BitstreamStore {
    /// (pid, did, order, code, blob index), sorted so a lookup is a scan of one
    /// contiguous run.
    entries: Vec<(u16, u16, u8, u32, u32)>,
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
        Self::parse_from(raw, "the Lumenera bitstream store")
    }

    /// [`parse`](Self::parse), naming where the bytes came from so an error can
    /// say which copy is wrong — the bundled one or a configured path.
    pub fn parse_from(raw: Vec<u8>, origin: &str) -> Result<Self> {
        // Checked before the magic: a pointer *is* a bad magic, but reporting it
        // as one sends the reader after a corrupt download instead of a missing
        // LFS fetch.
        if lfs::is_pointer(&raw) {
            return Err(lfs::pointer_error(&raw, origin));
        }
        if raw.len() < 12 || &raw[..8] != MAGIC {
            return Err(err(format!(
                "{origin} is not a Lumenera bitstream store (bad magic); rebuild it \
                 with `lucam_fpga --pack`"
            )));
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
            if has_code == 0 {
                return Err(err(format!(
                    "Lumenera bitstream store entry pid={pid:#06x} did={did:#06x} order={order} has no ProgramCode"
                )));
            }
            entries.push((pid, did, order, code, blob));
            at += 14;
        }
        validate_entry_grouping(&entries)?;

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
        Self::parse_from(raw, &format!("the bitstream store at {}", path.display()))
    }

    /// Bitstreams for a camera, in programming order.
    ///
    /// `0xFFFF` in either key position of a stored entry is a **wildcard**: a
    /// catch-all covering any product or device id. Matching exactly would miss
    /// those entries entirely, and a camera that should have been covered would
    /// look unsupported.
    ///
    /// Only the first matching table row is selected. Later rows may be broader
    /// catch-alls, and appending them would program a camera with bitstreams the
    /// vendor table did not select for that revision.
    pub fn bitstreams_for(&self, pid: u16, did: u16) -> Result<Vec<Bitstream>> {
        let matches = |stored: u16, want: u16| stored == want || stored == WILDCARD_ID;
        let mut selected_key = None;
        let mut out = Vec::new();
        for (p, d, _, code, blob) in &self.entries {
            match selected_key {
                Some(key) if key != (*p, *d) => break,
                Some(_) => {}
                None => {
                    if !matches(*p, pid) || !matches(*d, did) {
                        continue;
                    }
                    selected_key = Some((*p, *d));
                }
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
        v.sort_unstable();
        v.dedup();
        v
    }
}

/// Checks the property [`BitstreamStore::bitstreams_for`] actually relies on:
/// each *run* of equal keys is a complete programming set, ordered from 0.
///
/// A key may open more than one run. The real vendor table does exactly that —
/// pids `0x8b`, `0x8d`, `0x92`, `0x97` and `0x9e` each appear in two or three
/// separate runs — and the lookup is built for it: it selects the first run
/// that matches and `break`s at the key change, so later runs are unreachable
/// for that key and cannot contribute bitstreams.
///
/// An earlier revision rejected a repeated key as "non-contiguous entries".
/// That was a stricter invariant than the lookup needs, and it was not one the
/// vendor's table satisfies: it rejected the shipped store outright, so *every*
/// Lumenera camera failed FPGA bring-up before a single transfer was made.
fn validate_entry_grouping(entries: &[(u16, u16, u8, u32, u32)]) -> Result<()> {
    let mut current: Option<(u16, u16)> = None;
    let mut expected_order = 0u8;
    for (pid, did, order, ..) in entries {
        let key = (*pid, *did);
        if current != Some(key) {
            current = Some(key);
            expected_order = 0;
        }
        if *order != expected_order {
            return Err(err(format!(
                "Lumenera bitstream store has pid={:#06x} did={:#06x} at order {order}, \
                 expected {expected_order} — a programming set must run 0, 1, 2, …",
                key.0, key.1
            )));
        }
        expected_order = expected_order.saturating_add(1);
    }
    Ok(())
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

/// Programs the FPGA, following the GPL SDK's `lucam_fpga_setup` USB 2 path.
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

    // SDK detail: this countdown is initialized once before the bitstream loop,
    // not once per bitstream. Failures inside the loop return immediately and
    // leave the FPGA alt-setting selected; only the success/final-verify tail
    // returns the interface to Idle.
    let mut polls_remaining = PROGRAM_POLLS;
    for (n, bs) in bitstreams.iter().enumerate() {
        io.write_reg(REG_FPGA_MODE, bs.code)?;
        io.delay(CODE_SETTLE);
        for chunk in chunks(&bs.data) {
            io.send_chunk(chunk)?;
            io.delay(CHUNK_GAP);
        }

        if polls_remaining > 0 {
            loop {
                io.delay(POLL_GAP);
                polls_remaining -= 1;
                if polls_remaining == 0 {
                    break;
                }
                if io.read_reg(REG_FPGA_MODE)? & FPGA_BUSY == 0 {
                    break;
                }
            }
        }
        if io.read_reg(REG_FPGA_MODE)? & FPGA_BUSY != 0 {
            return Err(err(format!(
                "Lumenera FPGA bitstream {} of {} did not finish programming before the shared \
                 SDK countdown expired",
                n + 1,
                bitstreams.len()
            )));
        }
    }

    // Back to normal mode, then confirm it actually took. The SDK attempts the
    // Idle transition after this final verification branch, even when the final
    // programmed-bit check fails.
    io.write_reg(REG_FPGA_MODE, 0)?;
    let programmed = io.read_reg(REG_FPGA_MODE)? & FPGA_PROGRAMMED != 0;
    let restored = io.set_alt_setting(AltSetting::Idle);
    if !programmed {
        restored?;
        return Err(err(
            "Lumenera FPGA reports not programmed after the whole bitstream set was sent",
        ));
    }
    restored?;
    Ok(Programmed::Completed {
        bitstreams: bitstreams.len(),
    })
}

#[cfg(test)]
mod store_tests {
    use super::*;

    const POINTER: &[u8] = b"version https://git-lfs.github.com/spec/v1\n\
        oid sha256:5a27aaefc56f08f1f36c4e59ea8fe76130bac43a638ef987b12dbf1f1c6b3135\n\
        size 2400605\n";

    /// `Result::unwrap_err` wants `BitstreamStore: Debug`, and a 2.4 MB store
    /// has no business having one just to satisfy a test.
    fn parse_err(raw: &[u8], origin: &str) -> Error {
        match BitstreamStore::parse_from(raw.to_vec(), origin) {
            Ok(_) => panic!("{origin}: expected a parse error, got a store"),
            Err(e) => e,
        }
    }

    /// The bundled store is the one the binary actually programs from, so a
    /// build that got past `bundled_firmware`'s assertions must still parse.
    #[test]
    fn the_bundled_store_parses() {
        let store = BitstreamStore::parse(crate::bundled_firmware::BITSTREAM_STORE.to_vec())
            .expect("the compiled-in bitstream store must parse");
        // The Lu130 at device id 0x0018 needs two bitstreams in sequence — a
        // property of the real store, not of any container that happens to
        // carry the magic.
        assert!(!store.entries.is_empty());
    }

    /// A configured `firmware_dir` can hold a pointer even when the bundled
    /// copy is fine: the operator's checkout is not this build's checkout.
    #[test]
    fn a_pointer_is_diagnosed_rather_than_called_bad_magic() {
        let e = parse_err(POINTER, "the store at /x");
        assert!(e.message.contains("Git LFS pointer"), "{}", e.message);
        assert!(e.message.contains("git lfs pull"), "{}", e.message);
        // The old wording sent readers after a corrupt pack. It must not appear.
        assert!(!e.message.contains("bad magic"), "{}", e.message);
    }

    /// Genuinely wrong bytes still report as bad magic — the LFS branch must
    /// not swallow every parse failure.
    #[test]
    fn non_pointer_garbage_still_reports_bad_magic() {
        let e = parse_err(b"not a store at all", "the store");
        assert!(e.message.contains("bad magic"), "{}", e.message);
    }

    /// The origin is named, so an error distinguishes the bundled copy from a
    /// configured path.
    #[test]
    fn errors_name_which_copy_is_wrong() {
        let e = parse_err(POINTER, "the store at /tmp/fw");
        assert!(e.message.contains("/tmp/fw"), "{}", e.message);
    }

    /// The vendor table opens several runs for the same key. Rejecting that
    /// rejected the shipped store, so it is asserted on the real bytes.
    #[test]
    fn the_bundled_store_has_keys_in_more_than_one_run() {
        let store = BitstreamStore::parse(crate::bundled_firmware::BITSTREAM_STORE.to_vec())
            .expect("the compiled-in bitstream store must parse");
        let mut runs = Vec::new();
        let mut current = None;
        for (pid, did, ..) in &store.entries {
            if current != Some((*pid, *did)) {
                current = Some((*pid, *did));
                runs.push((*pid, *did));
            }
        }
        let mut repeated: Vec<_> = runs
            .iter()
            .filter(|k| runs.iter().filter(|o| o == k).count() > 1)
            .collect();
        repeated.dedup();
        assert!(
            !repeated.is_empty(),
            "expected the vendor table to reopen keys; if it no longer does, the \
             relaxed grouping rule in validate_entry_grouping is untested"
        );
    }

    /// End-to-end on the camera this repository is actually driving: the Gel
    /// Doc EZ's imaging-stage pid. Parsing proves the index; this proves the
    /// blob offsets and the zstd payload, which is what bring-up sends.
    #[test]
    fn the_bundled_store_yields_bitstreams_for_the_gel_doc_ez() {
        let store = BitstreamStore::parse(crate::bundled_firmware::BITSTREAM_STORE.to_vec())
            .expect("the compiled-in bitstream store must parse");
        let imaging_pid = crate::lumenera::IMAGING_PID;

        let dids = store.known_device_ids(imaging_pid);
        assert!(
            !dids.is_empty(),
            "the store covers no device id for pid {imaging_pid:#06x}"
        );

        for did in dids {
            let sets = store
                .bitstreams_for(imaging_pid, did)
                .unwrap_or_else(|e| panic!("pid={imaging_pid:#06x} did={did:#06x}: {e}"));
            assert!(
                !sets.is_empty(),
                "pid={imaging_pid:#06x} did={did:#06x} resolved to no bitstreams"
            );
            for (n, bitstream) in sets.iter().enumerate() {
                assert!(
                    !bitstream.data.is_empty(),
                    "pid={imaging_pid:#06x} did={did:#06x} bitstream {n} decompressed to nothing"
                );
            }
        }
    }

    /// Relaxing the run rule must not relax the ordering rule: a set that does
    /// not start at 0 is still a broken set.
    #[test]
    fn a_run_that_does_not_start_at_zero_is_rejected() {
        // pid, did, order, code, blob
        let entries = [(0x92u16, 0u16, 1u8, 0u32, 0u32)];
        let e = validate_entry_grouping(&entries).unwrap_err();
        assert!(e.message.contains("expected 0"), "{}", e.message);
    }

    /// A gap inside one run is a missing bitstream, and the camera would be
    /// programmed with an incomplete set.
    #[test]
    fn a_gap_inside_a_run_is_rejected() {
        let entries = [
            (0x92u16, 0u16, 0u8, 0u32, 0u32),
            (0x92, 0, 2, 0, 1), // order 1 missing
        ];
        let e = validate_entry_grouping(&entries).unwrap_err();
        assert!(e.message.contains("expected 1"), "{}", e.message);
    }

    /// The shape the old rule forbade: the same key opening a second complete
    /// run later in the table.
    #[test]
    fn a_key_may_open_a_later_run() {
        let entries = [
            (0x92u16, 0u16, 0u8, 0u32, 0u32),
            (0x92, 0, 1, 0, 1),
            (0x97, 0, 0, 0, 2),
            (0x92, 0, 0, 0, 3), // reopened — legal
            (0x92, 0, 1, 0, 4),
        ];
        assert!(validate_entry_grouping(&entries).is_ok());
    }
}
