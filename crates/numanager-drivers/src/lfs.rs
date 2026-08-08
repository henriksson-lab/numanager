//! Recognising Git LFS pointer files where payload bytes were expected.
//!
//! Some third-party blobs under `data/` are LFS-tracked. A checkout that never
//! ran the LFS smudge filter leaves a ~130-byte text pointer in the file's
//! place, and everything downstream then reads that pointer as if it were the
//! file. The failure is silent and late: the build succeeds, the bytes are
//! wrong, and the first symptom is a device refusing to come up.
//!
//! **Cargo is the common way to land in that state.** It checks out git
//! dependencies with libgit2, which does not run filter drivers, so a consumer
//! depending on this crate via `git = "..."` gets pointers no matter how its
//! own checkout is configured — `actions/checkout` with `lfs: true` covers the
//! consumer's repository, never cargo's dependency checkouts.
//!
//! Detection lives here, in an always-compiled module, because it is needed
//! both at runtime (parsing a configured file) and in `const` context (see
//! `crate::bundled_firmware`, which refuses to bake a pointer into the binary).

use numanager_core::{Error, ErrorCode};

/// First line of a Git LFS pointer file, per the v1 spec.
pub(crate) const POINTER_PREFIX: &[u8] = b"version https://git-lfs.github.com/spec/v1";

/// Whether `raw` is a Git LFS pointer standing in for the file it names.
///
/// `const` so it can gate `include_bytes!` at compile time.
pub(crate) const fn is_pointer(raw: &[u8]) -> bool {
    if raw.len() < POINTER_PREFIX.len() {
        return false;
    }
    let mut i = 0;
    while i < POINTER_PREFIX.len() {
        if raw[i] != POINTER_PREFIX[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// The `size` field of a pointer: how large the real file is.
pub(crate) fn pointer_size(raw: &[u8]) -> Option<u64> {
    std::str::from_utf8(raw)
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("size "))?
        .trim()
        .parse()
        .ok()
}

/// The diagnosis for a pointer found where real bytes were expected.
///
/// `origin` names the copy at fault ("the bitstream store at /path"), so a
/// configured path and the bundled blob do not produce the same message.
pub(crate) fn pointer_error(raw: &[u8], origin: &str) -> Error {
    let stands_for = match pointer_size(raw) {
        Some(n) => format!("{n} bytes"),
        None => "the real file".to_string(),
    };
    Error::new(
        ErrorCode::Driver,
        format!(
            "{origin} is a Git LFS pointer ({} bytes standing in for {stands_for}), \
             not the file itself: the LFS object was never fetched. In a git clone, \
             run `git lfs pull`. Cargo does not run LFS filters when it checks out a \
             git dependency, so a build consuming this crate that way must fetch the \
             object into the checkout \
             (`git -C <checkout> -c lfs.url=<repo>/info/lfs lfs pull`) or point the \
             driver's `firmware_dir` at a directory holding the real file.",
            raw.len(),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL_POINTER: &[u8] = b"version https://git-lfs.github.com/spec/v1\n\
        oid sha256:5a27aaefc56f08f1f36c4e59ea8fe76130bac43a638ef987b12dbf1f1c6b3135\n\
        size 2400605\n";

    #[test]
    fn recognises_a_pointer_and_reads_the_size_it_stands_for() {
        assert!(is_pointer(REAL_POINTER));
        assert_eq!(pointer_size(REAL_POINTER), Some(2_400_605));
    }

    #[test]
    fn real_payloads_are_not_pointers() {
        assert!(!is_pointer(b"LUFPGA01\x6c\x00\x00\x00"));
        assert!(!is_pointer(b":101DDD0001"));
        assert!(!is_pointer(b""));
        // Shorter than the prefix, and a prefix of it: must not over-match.
        assert!(!is_pointer(b"version https://git-lfs"));
    }

    #[test]
    fn the_error_says_what_to_do_about_it() {
        let msg = pointer_error(REAL_POINTER, "the bitstream store at /tmp/x").message;
        assert!(msg.contains("the bitstream store at /tmp/x"));
        assert!(msg.contains("2400605 bytes"));
        assert!(msg.contains("git lfs pull"));
        // The cargo-git-dependency case is the one that bit us; keep it named.
        assert!(msg.contains("git dependency"));
    }

    #[test]
    fn a_pointer_without_a_size_still_diagnoses() {
        let msg = pointer_error(POINTER_PREFIX, "the store").message;
        assert!(msg.contains("the real file"));
    }
}
