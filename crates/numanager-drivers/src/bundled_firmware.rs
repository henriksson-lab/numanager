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
    bundled!("lumenera", "lumenera_fw_img00.hex"),
    bundled!("lumenera", "lumenera_fw_img01.hex"),
    bundled!("lumenera", "lumenera_fw_img16.hex"),
];

/// Binary (non-Intel-HEX) images compiled in, by originating package. Kept
/// separate from [`IMAGES`] because these are raw byte streams pushed to an
/// endpoint rather than address/record text.
#[cfg(feature = "os-usb")]
const BLOBS: &[(&str, &[u8])] = &[(
    "lumenera_fpga_lu130.bin",
    include_bytes!("../../../data/third_party/lumenera/lumenera_fpga_lu130.bin"),
)];

/// Recorded control-transfer sequences, replayed verbatim during bring-up where
/// the individual transfers are not decoded. Same treatment as [`IMAGES`]:
/// third-party data, compiled in so a device works with no `data/` directory.
#[cfg(feature = "os-usb")]
const SEQUENCES: &[(&str, &str)] = &[bundled!("lumenera", "lumenera_init_lu130.jsonl")];

/// The compiled-in recorded sequence with this exact file name.
#[cfg(feature = "os-usb")]
pub(crate) fn sequence_by_name(name: &str) -> Option<&'static str> {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    SEQUENCES
        .iter()
        .find(|(seq, _)| *seq == file)
        .map(|(_, text)| *text)
}

/// The compiled-in binary image with this exact file name.
#[cfg(feature = "os-usb")]
pub(crate) fn blob_by_name(name: &str) -> Option<&'static [u8]> {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    BLOBS
        .iter()
        .find(|(blob, _)| *blob == file)
        .map(|(_, bytes)| *bytes)
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
