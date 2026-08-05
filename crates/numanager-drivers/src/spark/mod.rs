//! Tecan Spark Cyto wire protocol: TDCL 2.0 framing, the ASCII command vocabulary, and the
//! data-package decoder.
//!
//! # Provenance
//!
//! Reverse engineered from the vendor Windows stack — managed assemblies, the shipped
//! reference-firmware simulator and the configuration XMLs; there is no vendor documentation
//! for any of it, and no capture of a live session either. The frame layout, the type bytes
//! and the data-package field codes are recorded in `docs/reverse/spark-cyto-protocol.md`,
//! which is the evidence this implementation is written from, and
//! `docs/reverse/spark-cyto.md` records what is still unknown and what a first capture has
//! to settle. Behaviour marked "to confirm" has not been seen on hardware and should be
//! treated as a hypothesis.
//!
//! This is the only codec for the instrument. An earlier sketch in [`crate::spark_cyto`] —
//! a different header order, a little-endian length, no checksum, five invented type bytes —
//! was removed rather than kept beside this one, because two codecs for one instrument is an
//! invitation to bind a transport to the wrong half.
//!
//! Per this repository's rules the modules here carry no inline tests. They are exercised
//! from `brunnim`'s integration suite, which owns the artifacts they were derived from.

pub mod backend;
pub mod catalog;
pub mod commands;
pub mod data;
pub mod parse;
pub mod session;
pub mod tdcl;
pub mod usb;
