//! Tecan Spark Cyto wire protocol: TDCL 2.0 framing, the ASCII command vocabulary, and the
//! data-package decoder.
//!
//! # Provenance
//!
//! Reverse engineered from captured traffic and firmware strings; there is no vendor
//! documentation for any of it. The frame layout, the type bytes and the data-package field
//! codes are recorded in `docs/reverse/spark-cyto-tdcl.md`, which is the evidence this
//! implementation is written from. Behaviour marked "to confirm" there has not been seen on
//! hardware and should be treated as a hypothesis.
//!
//! This **replaces** the placeholder codec that previously lived inline in
//! [`crate::spark_cyto`]. That sketch used a different header order, a little-endian length,
//! no checksum and five invented type bytes; it was never reachable — no transport was ever
//! constructed for it — but it disagreed with the traces on every field, and leaving it
//! beside a real one would invite someone to bind a transport to the wrong half.
//!
//! Per this repository's rules the modules here carry no inline tests. They are exercised
//! from `brunnim`'s integration suite, which owns the captures they were derived from.

pub mod backend;
pub mod catalog;
pub mod commands;
pub mod data;
pub mod parse;
pub mod session;
pub mod tdcl;
