//! Integrity of the third-party blobs under `data/third_party/`.
//!
//! These files are compiled into the binary by `crate::bundled_firmware`, so a
//! wrong file here is a wrong file in every build that follows. The failure
//! mode this guards is specific and has happened: `lucam-fpga.lufpga` is
//! LFS-tracked, and a checkout that never ran the smudge filter leaves a
//! ~130-byte pointer in its place. Nothing about that is visible at build time
//! — sizes and digests are what tell the two apart.
//!
//! Each `manifest.toml` already records name, size and SHA-256 per file. This
//! test is what makes those records load-bearing rather than documentation.
//!
//! In CI this requires `actions/checkout` with `lfs: true`; see
//! `.github/workflows/build.yml`. It is a test rather than a `build.rs` check
//! so that a consumer building this crate as a dependency is not forced to have
//! the digests verified on every build — the compile-time assertions in
//! `bundled_firmware` cover them, and those need no manifest.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// `data/third_party/`, relative to this crate.
fn data_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/third_party")
}

/// One `[[file]]` entry: the fields this test enforces.
#[derive(Debug)]
struct Entry {
    name: String,
    sha256: String,
    size_bytes: u64,
}

/// Reads the `[[file]]` blocks out of a manifest.
///
/// Hand-rolled rather than a TOML dependency: the manifests are a fixed shape
/// (`[[file]]` blocks of `key = "value"`), and a test that guards the build
/// should not widen the dependency graph to run. Anything unparseable is a
/// failure, not a skip — a manifest this cannot read is one it cannot enforce.
fn parse_manifest(text: &str, manifest: &Path) -> Vec<Entry> {
    let mut entries = Vec::new();
    for block in text.split("[[file]]").skip(1) {
        let field = |key: &str| -> Option<String> {
            block.lines().find_map(|line| {
                let line = line.trim();
                let rest = line.strip_prefix(key)?.trim_start().strip_prefix('=')?.trim();
                Some(rest.trim_matches('"').to_string())
            })
        };
        let (Some(name), Some(sha256), Some(size)) =
            (field("name"), field("sha256"), field("size_bytes"))
        else {
            panic!("{}: a [[file]] block is missing name/sha256/size_bytes", manifest.display());
        };
        let size_bytes = size
            .parse()
            .unwrap_or_else(|_| panic!("{}: size_bytes for {name} is not a number", manifest.display()));
        entries.push(Entry { name, sha256, size_bytes });
    }
    entries
}

fn manifests() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(data_root())
        .expect("data/third_party is missing — is this a full checkout?")
        .filter_map(|e| e.ok())
        .map(|e| e.path().join("manifest.toml"))
        .filter(|p| p.exists())
        .collect();
    found.sort();
    found
}

/// Every file a manifest names exists, and is byte-for-byte what it records.
///
/// Size is asserted before the digest purely so a mismatch reads usefully: a
/// 132-byte "2.4 MB" file names the problem, where two hex digests do not.
#[test]
fn manifested_files_match_their_recorded_identity() {
    let manifests = manifests();
    assert!(
        !manifests.is_empty(),
        "no manifest.toml found under {} — this test would silently pass",
        data_root().display()
    );

    let mut checked = 0;
    for manifest in &manifests {
        let dir = manifest.parent().unwrap();
        let text = std::fs::read_to_string(manifest)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", manifest.display()));
        let entries = parse_manifest(&text, manifest);
        assert!(
            !entries.is_empty(),
            "{} lists no files — an empty manifest enforces nothing",
            manifest.display()
        );

        for entry in entries {
            let path = dir.join(&entry.name);
            let bytes = std::fs::read(&path)
                .unwrap_or_else(|e| panic!("{} is listed in {} but unreadable: {e}", path.display(), manifest.display()));

            assert_eq!(
                bytes.len() as u64,
                entry.size_bytes,
                "{} is {} bytes, manifest says {}{}",
                path.display(),
                bytes.len(),
                entry.size_bytes,
                lfs_hint(&bytes),
            );

            let digest = hex::encode(Sha256::digest(&bytes));
            assert_eq!(
                digest,
                entry.sha256,
                "{} has digest {digest}, manifest says {}{}",
                path.display(),
                entry.sha256,
                lfs_hint(&bytes),
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "no files were checked");
}

/// No file under `data/third_party/` is an unsmudged Git LFS pointer.
///
/// Broader than the manifest check on purpose: it covers files that no manifest
/// lists yet, so adding an LFS-tracked blob without a manifest entry still
/// fails loudly here instead of being baked in unnoticed.
#[test]
fn no_third_party_file_is_an_lfs_pointer() {
    const PREFIX: &[u8] = b"version https://git-lfs.github.com/spec/v1";

    let mut pointers = Vec::new();
    let mut stack = vec![data_root()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display())) {
            let path = entry.expect("unreadable dir entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            // Pointers are ~130 bytes; reading the head is enough and keeps
            // this off the 2.4 MB store.
            let Ok(bytes) = std::fs::read(&path) else { continue };
            if bytes.starts_with(PREFIX) {
                pointers.push(path);
            }
        }
    }

    assert!(
        pointers.is_empty(),
        "these files are Git LFS pointers, not their contents — run `git lfs pull`:\n  {}",
        pointers
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

/// Every file `bundled_firmware` compiles in is covered by a manifest entry.
///
/// Without this, a blob could be embedded with no recorded digest, and the
/// integrity test above would pass by not looking at it.
#[test]
fn bundled_blobs_are_covered_by_a_manifest() {
    let recorded: Vec<String> = manifests()
        .iter()
        .flat_map(|m| {
            let text = std::fs::read_to_string(m).unwrap();
            parse_manifest(&text, m).into_iter().map(|e| e.name)
        })
        .collect();

    // Kept in step with bundled_firmware.rs by hand — that module's IMAGES and
    // SEQUENCES are private, and making them public to satisfy a test would be
    // the tail wagging the dog.
    let bundled = [
        "fx2_AndorCam.hex",
        "fx2_temp_prog.hex",
        "fx2_usbcam.hex",
        "fx2_vendax.hex",
        "lumenera_fw_img00.hex",
        "lumenera_fw_img01.hex",
        "lumenera_fw_img10.hex",
        "lumenera_fw_img18.hex",
        "lumenera_init_lu130.jsonl",
        "lucam-fpga.lufpga",
    ];

    let missing: Vec<&str> = bundled
        .iter()
        .copied()
        .filter(|name| !recorded.iter().any(|r| r == name))
        .collect();
    assert!(
        missing.is_empty(),
        "compiled into the binary but recorded in no manifest.toml: {missing:?}"
    );
}

/// Appended to a size/digest mismatch when the bytes explain themselves.
fn lfs_hint(bytes: &[u8]) -> String {
    if bytes.starts_with(b"version https://git-lfs.github.com/spec/v1") {
        " — this file is a Git LFS pointer, not its contents; run `git lfs pull` \
         (CI needs actions/checkout with `lfs: true`)"
            .to_string()
    } else {
        String::new()
    }
}
