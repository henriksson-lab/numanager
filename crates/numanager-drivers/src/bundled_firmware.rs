//! Third-party device firmware compiled into the binary, so a device reloads
//! after a power cycle without a `data/` directory to locate. A configured
//! firmware path overrides these; it is never a prerequisite.
//!
//! Lookup is by file name only — which image a unit needs is per-driver
//! evidence (Lumenera's `bcdDevice` selector, Andor's four scoped images), and
//! this module does not guess.
//!
//! These images are **not** covered by this repository's MIT or Apache-2.0
//! terms. Each `data/third_party/<vendor>/manifest.toml` records identity, size
//! and SHA-256 per file; Andor's adds "verify redistribution terms before
//! distribution", which travels with these bytes into any binary built here.

/// `(file name, Intel-HEX text)` for one image under `data/third_party/`.
macro_rules! bundled {
    ($package:literal, $name:literal) => {
        (
            $name,
            include_str!(concat!("../../../data/third_party/", $package, "/", $name)),
        )
    };
}

/// Every image compiled in, by originating package.
///
/// Keep this in step with the `manifest.toml` files. An image listed in a
/// manifest but absent here is simply not bundled — a supported state; that
/// driver then needs a configured path.
const IMAGES: &[(&str, &str)] = &[
    bundled!("andor", "fx2_AndorCam.hex"),
    bundled!("andor", "fx2_temp_prog.hex"),
    bundled!("andor", "fx2_usbcam.hex"),
    bundled!("andor", "fx2_vendax.hex"),
    #[cfg(feature = "lumenera")]
    bundled!("lumenera", "lumenera_fw_img00.hex"),
    #[cfg(feature = "lumenera")]
    bundled!("lumenera", "lumenera_fw_img01.hex"),
    #[cfg(feature = "lumenera")]
    bundled!("lumenera", "lumenera_fw_img10.hex"),
    #[cfg(feature = "lumenera")]
    bundled!("lumenera", "lumenera_fw_img18.hex"),
];

// The captured `lumenera_fpga_lu130.bin` was compiled in here. It was one
// revision's FPGA bitstream, superseded by the bitstream store in
// `data/third_party/lumenera/lucam-fpga.lufpga`, which covers every revision of
// every model with the program codes and ordering the camera needs.
//
// That store was once loaded from disk rather than compiled in — 2.4 MB, and
// vendor data. It is compiled in now, for the same reason as the images above:
// the on-disk path was `env!("CARGO_MANIFEST_DIR")`-derived, so it named a
// *build machine's* source tree. A shipped binary, or any consumer that took
// this crate as a cargo git dependency, therefore had no store at all, and
// found that out on hardware during FPGA bring-up rather than at build time.

/// The packed bitstream store, compiled in. Parsed on demand — see
/// [`crate::lumenera_fpga::BitstreamStore`]; a configured `firmware_dir`
/// overrides it, and as with [`IMAGES`] it is never a prerequisite.
#[cfg(feature = "lumenera")]
pub(crate) const BITSTREAM_STORE: &[u8] =
    include_bytes!("../../../data/third_party/lumenera/lucam-fpga.lufpga");

/// Name of the store within `data/third_party/lumenera/`, and the file a
/// configured `firmware_dir` is expected to hold.
#[cfg(feature = "lumenera")]
pub(crate) const BITSTREAM_STORE_FILE: &str = "lucam-fpga.lufpga";

/// Compile-time proof that no bundled blob is a Git LFS pointer.
///
/// `include_bytes!`/`include_str!` embed whatever is on disk. The store is
/// LFS-tracked, and an unsmudged checkout leaves a ~130-byte pointer in its
/// place — which would otherwise be baked into the binary silently and surface
/// much later, on a bench, as "bad magic". Failing the build instead puts the
/// error where the cause is, for every build rather than only in CI.
///
/// Intel-HEX images are checked for their record marker in the same pass: any
/// wrong-file substitution that still parses as UTF-8 (a pointer, an LFS error
/// page, a README) fails here rather than mid-download to an 8051.
const _: () = assert_bundled_blobs_are_real();

const fn assert_bundled_blobs_are_real() {
    let mut i = 0;
    while i < IMAGES.len() {
        let bytes = IMAGES[i].1.as_bytes();
        assert!(
            !bytes.is_empty(),
            "a bundled firmware image is empty — check data/third_party/"
        );
        assert!(
            !crate::lfs::is_pointer(bytes),
            "a bundled firmware image is a Git LFS pointer, not firmware: run \
             `git lfs pull` (cargo does not smudge LFS in git dependencies)"
        );
        assert!(
            bytes[0] == b':',
            "a bundled firmware image is not Intel HEX (no ':' record marker) — \
             the wrong file is about to be baked in"
        );
        i += 1;
    }
}

#[cfg(feature = "lumenera")]
const _: () = {
    assert!(
        !crate::lfs::is_pointer(BITSTREAM_STORE),
        "the Lumenera bitstream store is a Git LFS pointer, not the store: run \
         `git lfs pull` (cargo does not smudge LFS in git dependencies)"
    );
    assert!(
        BITSTREAM_STORE.len() > 1024,
        "the Lumenera bitstream store is implausibly small — the wrong file is \
         about to be baked in"
    );
};

#[cfg(all(feature = "os-usb", feature = "lumenera"))]
const _: () = {
    let mut i = 0;
    while i < SEQUENCES.len() {
        assert!(
            !crate::lfs::is_pointer(SEQUENCES[i].1.as_bytes()),
            "a bundled capture sequence is a Git LFS pointer, not a capture: run \
             `git lfs pull` (cargo does not smudge LFS in git dependencies)"
        );
        i += 1;
    }
};

/// Recorded control-transfer sequences, replayed verbatim during bring-up where
/// the individual transfers are not decoded. Same treatment as [`IMAGES`]:
/// third-party data, compiled in so a device works with no `data/` directory.
#[cfg(all(feature = "os-usb", feature = "lumenera"))]
const SEQUENCES: &[(&str, &str)] = &[bundled!("lumenera", "lumenera_init_lu130.jsonl")];

/// The compiled-in recorded sequence with this exact file name.
#[cfg(all(feature = "os-usb", feature = "lumenera"))]
pub(crate) fn sequence_by_name(name: &str) -> Option<&'static str> {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    SEQUENCES
        .iter()
        .find(|(seq, _)| *seq == file)
        .map(|(_, text)| *text)
}

/// The compiled-in image with this exact file name, or `None` when it is not
/// bundled. `name` may be a bare file name or a path — only the final component
/// is matched, so a driver can pass a configured path straight through.
pub(crate) fn image_by_name(name: &str) -> Option<&'static str> {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    IMAGES
        .iter()
        .find(|(image, _)| *image == file)
        .map(|(_, text)| *text)
}
