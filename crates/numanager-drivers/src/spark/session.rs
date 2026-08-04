//! Sequence allocation and the Busy → Ready/Error state machine, without blocking.
//!
//! The instrument answers a command with zero or more `Busy` frames and then a terminal
//! `Ready` (with optional `KEY=VALUE` text) or `Error`, all echoing the sequence number the
//! command carried. Read commands additionally stream a data header plus payload on the
//! data channel.
//!
//! # Why this does not block
//!
//! The obvious implementation sends a command and loops until the terminal frame arrives.
//! That is what brunnim's `Session` did, and it is wrong here: a [`Driver`] shares one
//! thread with every other device in the graph, so a plate move that takes ten seconds
//! would freeze the stage, the camera and the incubator with it. Instead a command is
//! *submitted*, and [`SparkSession::poll`] drains whatever bytes have arrived and reports
//! only the transactions that actually finished.
//!
//! One command is in flight at a time. The instrument echoes the sequence number, so
//! several could be tracked at once, but nothing has established that it accepts them —
//! pipelining on an assumption is how a reader ends up attributing one well's counts to
//! another.

use super::tdcl::{ascii_command, Frame, FrameStream, FrameType, Response};
use numanager_core::{Error, ErrorCode, Result, Transport};
use std::collections::VecDeque;

/// A command that has been sent and not yet answered.
#[derive(Debug, Clone)]
struct Pending<K> {
    seq: u8,
    /// Whatever the caller needs to make sense of the reply. The session does not
    /// interpret it.
    key: K,
    /// Data-channel frames seen since the command was sent.
    data: Vec<Frame>,
}

/// What a completed transaction produced.
#[derive(Debug, Clone)]
pub struct Outcome<K> {
    pub key: K,
    pub response: Response,
    /// Data-channel frames that arrived with it: a header (`0x88`) and a payload (`0x83`)
    /// for a measurement, empty for a command that only acknowledges.
    pub data: Vec<Frame>,
}

/// A transaction that failed.
#[derive(Debug, Clone)]
pub struct Failure<K> {
    pub key: K,
    pub number: Option<u32>,
    pub text: String,
}

/// The result of one [`SparkSession::poll`].
#[derive(Debug, Clone)]
pub enum Progress<K> {
    Completed(Outcome<K>),
    Failed(Failure<K>),
    /// The instrument reported it is still working, with the time it expects to need.
    Busy { key: K, ticks: Option<u32> },
    /// A fault arrived that no outstanding command asked for.
    Asynchronous { number: Option<u32>, text: String },
}

/// Non-blocking session over one instrument's command and data channels.
///
/// `K` is caller-chosen bookkeeping — a driver token, say — carried through to the reply
/// so the session never has to know what a command was for.
pub struct SparkSession<T: Transport, K> {
    transport: T,
    stream: FrameStream,
    seq: u8,
    inflight: Option<Pending<K>>,
    queued: VecDeque<(K, String)>,
}

impl<T: Transport, K: Clone> SparkSession<T, K> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            stream: FrameStream::new(),
            seq: 0,
            inflight: None,
            queued: VecDeque::new(),
        }
    }

    /// Is a command waiting for its terminal frame?
    pub fn busy(&self) -> bool {
        self.inflight.is_some()
    }

    pub fn queued(&self) -> usize {
        self.queued.len()
    }

    /// Send a command line, or queue it if one is already in flight.
    pub fn submit(&mut self, key: K, line: impl Into<String>) -> Result<()> {
        let line = line.into();
        if self.inflight.is_some() {
            self.queued.push_back((key, line));
            return Ok(());
        }
        self.send_now(key, &line)
    }

    fn send_now(&mut self, key: K, line: &str) -> Result<()> {
        let seq = self.seq;
        self.seq = self.seq.wrapping_add(1);
        self.transport.send(&ascii_command(seq, line))?;
        self.inflight = Some(Pending {
            seq,
            key,
            data: Vec::new(),
        });
        Ok(())
    }

    /// Read whatever has arrived and report anything that finished.
    ///
    /// Returns as many events as the bytes justified — usually none, since most polls
    /// happen while the instrument is still working.
    pub fn poll(&mut self) -> Result<Vec<Progress<K>>> {
        let mut events = Vec::new();
        while let Some(bytes) = self.transport.poll_recv()? {
            for frame in self.stream.feed(&bytes).map_err(decode_error)? {
                if let Some(event) = self.accept(frame) {
                    events.push(event);
                }
            }
        }
        // A finished transaction frees the line for the next one.
        if self.inflight.is_none() {
            if let Some((key, line)) = self.queued.pop_front() {
                self.send_now(key, &line)?;
            }
        }
        Ok(events)
    }

    fn accept(&mut self, frame: Frame) -> Option<Progress<K>> {
        let kind = FrameType::from_u8(frame.type_);
        match kind {
            // Data arrives before the Ready that closes the transaction, so it is
            // accumulated rather than reported.
            Some(FrameType::DataHeader) | Some(FrameType::Binary) => {
                if let Some(pending) = self.inflight.as_mut() {
                    pending.data.push(frame);
                }
                None
            }
            Some(FrameType::Busy) => {
                let pending = self.inflight.as_ref()?;
                if frame.seq != pending.seq {
                    return None;
                }
                let ticks = super::tdcl::parse_response(&frame).and_then(|response| response.ticks);
                Some(Progress::Busy {
                    key: pending.key.clone(),
                    ticks,
                })
            }
            Some(FrameType::Ready) => {
                let pending = self.inflight.as_ref()?;
                if frame.seq != pending.seq {
                    // A reply to a sequence nobody is waiting for. Dropping it is safer
                    // than attributing it to the command that happens to be in flight.
                    return None;
                }
                let response = super::tdcl::parse_response(&frame)?;
                let pending = self.inflight.take()?;
                Some(Progress::Completed(Outcome {
                    key: pending.key,
                    response,
                    data: pending.data,
                }))
            }
            Some(FrameType::Error) => {
                let response = super::tdcl::parse_response(&frame)?;
                match self.inflight.take() {
                    Some(pending) if frame.seq == pending.seq => Some(Progress::Failed(Failure {
                        key: pending.key,
                        number: response.number,
                        text: response.text,
                    })),
                    // Put it back: the error was not for this command.
                    other => {
                        self.inflight = other;
                        Some(Progress::Asynchronous {
                            number: response.number,
                            text: response.text,
                        })
                    }
                }
            }
            Some(FrameType::AsyncError) => {
                let response = super::tdcl::parse_response(&frame)?;
                Some(Progress::Asynchronous {
                    number: response.number,
                    text: response.text,
                })
            }
            // Log and message frames are chatter; terminate is a transport concern.
            _ => None,
        }
    }
}

/// A transport chosen at runtime.
///
/// The driver does not know at compile time whether it is talking to USB, to a pipe, or to
/// a loopback in a test, so it holds a boxed transport. `Transport` cannot be implemented
/// for `Box<dyn Transport>` directly — both are foreign to this crate — so this wrapper
/// carries the implementation.
pub struct BoxedTransport(pub Box<dyn Transport>);

impl BoxedTransport {
    pub fn new(transport: impl Transport + 'static) -> Self {
        Self(Box::new(transport))
    }
}

impl Transport for BoxedTransport {
    fn send(&mut self, bytes: &[u8]) -> Result<()> {
        self.0.send(bytes)
    }

    fn poll_recv(&mut self) -> Result<Option<Vec<u8>>> {
        self.0.poll_recv()
    }
}

fn decode_error(error: super::tdcl::DecodeError) -> Error {
    Error::new(ErrorCode::Transport, format!("TDCL decode failed: {error:?}"))
}
