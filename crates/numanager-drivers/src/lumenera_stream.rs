// SPDX-License-Identifier: GPL-2.0-only
//! Lumenera streaming: the acquire / start / stop / release state machine.
//!
//! Three things here are not guessable from watching traffic, and getting any of
//! them wrong produces a camera that enumerates, answers every control transfer,
//! and delivers no frames:
//!
//! 1. **Every transfer is submitted before the camera is enabled.** It starts
//!    emitting the moment `VIDEO_EN` is written, so enable-then-submit loses the
//!    opening frames.
//! 2. **`VIDEO_EN` is not a boolean.** Enable is `0xFFFFFFFF`; a real teardown
//!    writes `0x40` first — asking for zero-length packets so the transfer ends
//!    cleanly — and only then `0x00`.
//! 3. **The enable read-back is transport-specific.** The shipped USB 2 path
//!    writes the enable and assumes success. USB 3 endpoint paths read it back,
//!    and the legacy USB 3 endpoint path additionally treats high bits as a
//!    recovery condition.
//!
//! This state machine is derived from Teledyne's GPLv2 Linux SDK driver
//! (`lucam.c`, USB 2 plus labelled legacy USB3/U3V branches), with
//! `reveng-dll/teledyne/lucam-protocol-spec.md` kept as an audit notebook.
//! This file is therefore annotated `GPL-2.0-only`.
//!
//! The transport is abstracted so the ordering can be tested without a camera,
//! which is the only way any of this is checkable before hardware time.

use crate::lumenera_fpga::AltSetting;
use numanager_core::{Error, ErrorCode, Result};

fn err(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::Driver, msg)
}

/// Streaming enable/disable register (video mode).
pub const REG_VIDEO_EN: u16 = 0x0214;
/// Trigger control — the enable register in **still** mode.
pub const REG_TRIGGER_CTRL: u16 = 0x0218;
/// Still-mode values written to [`REG_TRIGGER_CTRL`].
///
/// Still capture is **two operations**: enable, then trigger. Enabling alone
/// arms the camera and it never exposes — a host waiting for that frame waits
/// forever. Which trigger value applies depends on the protocol version the
/// camera reports and on whether it is in hardware- or software-trigger mode.
pub const STILL_ENABLE: u32 = 0x04;
pub const STILL_DISABLE: u32 = 0x00;
/// Written after a successful enable, hardware-trigger mode only.
pub const STILL_ARM_HARDWARE: u32 = 0x05;
/// Software trigger, protocol version 0.
pub const STILL_TRIGGER_V0: u32 = 0x03;
/// Software trigger, protocol version >= 1, software-trigger mode.
pub const STILL_TRIGGER_SOFTWARE: u32 = 0x06;
/// Software trigger, protocol version >= 1, hardware-trigger mode.
pub const STILL_TRIGGER_HARDWARE: u32 = 0x07;
/// Extended-command subcode that recovers a wedged stream.
pub const EXT_STREAM_RECOVERY: u8 = 0x21;

/// Written to enable. Not `1`.
pub const VIDEO_ENABLE: u32 = 0xFFFF_FFFF;
/// Written to disable.
pub const VIDEO_DISABLE: u32 = 0x0000_0000;
/// Written *before* the disable on a real teardown: requests zero-length packets
/// so the outstanding transfer terminates instead of being left partial.
pub const VIDEO_REQUEST_ZLP: u32 = 0x0000_0040;

/// Any bit set above the low byte in the `VIDEO_EN` read-back means the camera
/// is unhealthy and needs recovery.
const VIDEO_EN_FAULT_MASK: u32 = 0xFFFF_FF00;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// No resources held.
    Stopped,
    /// Resources held, camera idle.
    Paused,
    /// Transfers are being submitted/enabled; cleanup must still run on failure.
    Starting,
    /// Camera emitting.
    Running,
}

/// Why the stream is being stopped, which decides whether the camera is asked
/// for zero-length packets on the way down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopKind {
    /// Between frames; the stream will start again.
    Pause,
    /// Going away for good.
    Teardown,
}

/// Which capture mode the stream drives. The specification makes the enable
/// step mode-dependent, and the two use *different registers* — not different
/// values in one register.
///
/// This matters for provenance here: the sequence captured from vendor traffic
/// for this camera writes `0x04`/`0x00` to `TRIGGER_CTRL`, which is **still**
/// mode, not video. So a driver that only ever grabs single frames has been
/// driving still capture, and switching it to `VIDEO_EN` would be a behavioural
/// change, not a fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Continuous video: `VIDEO_EN`, enable `0xFFFFFFFF`.
    Video,
    /// Single frames: `TRIGGER_CTRL`, enable then trigger.
    Still {
        /// `SPECIFICATION` (`0x0010`) — the camera's **protocol version**, not a
        /// capability bitfield. It selects the trigger encoding.
        spec_version: u32,
        /// Whether the camera is in hardware-trigger mode. The two trigger modes
        /// are distinct camera states; enabling is refused unless the camera is
        /// already in the matching one.
        hardware_trigger: bool,
    },
}

impl Mode {
    fn reg(self) -> u16 {
        match self {
            Mode::Video => REG_VIDEO_EN,
            Mode::Still { .. } => REG_TRIGGER_CTRL,
        }
    }
    fn enable(self) -> u32 {
        match self {
            Mode::Video => VIDEO_ENABLE,
            Mode::Still { .. } => STILL_ENABLE,
        }
    }
    fn disable(self) -> u32 {
        match self {
            Mode::Video => VIDEO_DISABLE,
            Mode::Still { .. } => STILL_DISABLE,
        }
    }
    /// The zero-length-packet request is a video-mode concept.
    fn requests_zlp(self) -> bool {
        self == Mode::Video
    }
    /// Value written straight after a successful enable, where the mode needs
    /// one. Hardware-trigger still capture does; nothing else does.
    fn post_enable(self) -> Option<u32> {
        match self {
            Mode::Still {
                hardware_trigger: true,
                ..
            } => Some(STILL_ARM_HARDWARE),
            _ => None,
        }
    }
    /// The software-trigger value, or `None` where triggering does not apply.
    fn software_trigger(self) -> Option<u32> {
        match self {
            Mode::Video => None,
            Mode::Still {
                spec_version: 0, ..
            } => Some(STILL_TRIGGER_V0),
            Mode::Still {
                hardware_trigger: false,
                ..
            } => Some(STILL_TRIGGER_SOFTWARE),
            Mode::Still {
                hardware_trigger: true,
                ..
            } => Some(STILL_TRIGGER_HARDWARE),
        }
    }
}

/// Bus speed, because clearing the endpoint halt is conditional on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusSpeed {
    /// USB 2.0 and below — halt is cleared on every stop.
    High,
    /// USB 3 — clearing halt when nothing is halted misbehaves, so it is only
    /// done when a frame error actually occurred.
    Super,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum EnableReadback {
    /// USB 2 vendor-control path: no enable read-back.
    None,
    /// U3V-compatible path: read failure is an enable failure, no recovery mask.
    ReadOnly,
    /// Legacy USB3 endpoint path: read failure or high bits schedules recovery.
    LegacyUsb3,
}

/// What streaming needs from a transport. The driver supplies the USB one.
pub trait StreamTransport {
    fn read_reg(&mut self, index: u16) -> Result<u32>;
    fn write_reg(&mut self, index: u16, value: u32) -> Result<()>;
    fn before_enable(&mut self, _mode: Mode) -> Result<()> {
        Ok(())
    }
    /// Host-to-device extended command, no payload.
    fn ext_cmd(&mut self, sub: u8) -> Result<()>;
    fn set_alt_setting(&mut self, alt: AltSetting) -> Result<()>;
    /// Submit the transfer in ring slot `slot`.
    fn submit(&mut self, slot: usize) -> Result<()>;
    /// Cancel outstanding transfers, walking the ring from `from` for `count`.
    fn kill(&mut self, from: usize, count: usize) -> Result<()>;
    fn clear_halt(&mut self) -> Result<()>;
    /// Drop queued frames and reset the buffer state.
    fn reset_frames(&mut self);
    fn bus_speed(&self) -> BusSpeed;
    fn enable_readback(&self) -> EnableReadback {
        EnableReadback::None
    }
    fn stream_recovery_supported(&self) -> bool {
        false
    }
}

/// The streaming state machine.
pub struct Stream {
    state: State,
    mode: Mode,
    /// Number of overlapping bulk transfers. The vendor design uses 15; too few
    /// starves the pipe at frame rate.
    pool: usize,
    /// Ring index of the last completed transfer; teardown walks from after it
    /// rather than from zero.
    last_completed: usize,
    /// Set by a fatal frame error or a bad `VIDEO_EN` read-back. Cleared when the
    /// recovery command is actually sent, during stop.
    recovery_outstanding: bool,
    frame_error: bool,
    frames: u64,
    camera_enabled: bool,
}

impl Stream {
    pub const DEFAULT_POOL: usize = 15;

    pub fn new_still(pool: usize, spec_version: u32, hardware_trigger: bool) -> Self {
        Self::with_mode(
            pool,
            Mode::Still {
                spec_version,
                hardware_trigger,
            },
        )
    }

    pub fn with_mode(pool: usize, mode: Mode) -> Self {
        Self {
            mode,
            state: State::Stopped,
            pool,
            last_completed: 0,
            recovery_outstanding: false,
            frame_error: false,
            frames: 0,
            camera_enabled: false,
        }
    }

    /// Record a frame error. `fatal` means the stream cannot continue and has to
    /// be rebuilt — that is what schedules a recovery.
    pub fn note_frame_error(&mut self, fatal: bool) {
        self.frame_error = true;
        if fatal {
            self.recovery_outstanding = true;
        }
    }

    /// Take the resources streaming needs. Stopped -> Paused.
    pub fn acquire<T: StreamTransport>(&mut self, io: &mut T) -> Result<()> {
        if self.state != State::Stopped {
            return Ok(());
        }
        if self.pool == 0 {
            return Err(err("Lumenera stream needs a non-empty transfer pool"));
        }
        io.set_alt_setting(AltSetting::Data)?;
        self.state = State::Paused;
        Ok(())
    }

    /// Start emitting. Paused -> Running.
    pub fn start<T: StreamTransport>(&mut self, io: &mut T) -> Result<()> {
        match self.state {
            State::Starting | State::Running => return Ok(()),
            State::Stopped => {
                return Err(err("Lumenera stream cannot start before it is acquired"))
            }
            State::Paused => {}
        }
        // One retry only, and only when a recovery was outstanding and has since
        // been issued by the stop path.
        let mut retried = false;
        loop {
            match self.try_start(io) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    if self.recovery_outstanding && io.stream_recovery_supported() && !retried {
                        retried = true;
                        // The stop sequence is what issues the recovery.
                        self.state = State::Running;
                        self.stop_inner(io, StopKind::Pause)?;
                        continue;
                    }
                    let _ = self.stop_inner(io, StopKind::Pause);
                    return Err(e);
                }
            }
        }
    }

    fn try_start<T: StreamTransport>(&mut self, io: &mut T) -> Result<()> {
        self.frames = 0;
        self.camera_enabled = false;
        self.state = State::Starting;
        // Seeded one before the start so the first teardown sweep begins at
        // slot 0 rather than slot 1.
        self.last_completed = self.pool.saturating_sub(1);

        // Everything in flight *before* the camera is told to go.
        for slot in 0..self.pool {
            io.submit(slot)?;
        }

        io.before_enable(self.mode)?;
        io.write_reg(self.mode.reg(), self.mode.enable())?;
        self.camera_enabled = true;

        match io.enable_readback() {
            EnableReadback::None => {}
            EnableReadback::ReadOnly => {
                io.read_reg(self.mode.reg())
                    .map_err(|_| err("Lumenera enable read-back failed after enabling"))?;
            }
            EnableReadback::LegacyUsb3 => match io.read_reg(self.mode.reg()) {
                Err(_) => {
                    self.recovery_outstanding = true;
                    return Err(err(
                        "Lumenera enable read-back failed after enabling; stream recovery required",
                    ));
                }
                Ok(v) if v & VIDEO_EN_FAULT_MASK != 0 => {
                    self.recovery_outstanding = true;
                    return Err(err(format!(
                        "Lumenera enable read back as {v:#010x} after enabling; stream recovery required"
                    )));
                }
                Ok(_) => {}
            },
        }

        if let Some(v) = self.mode.post_enable() {
            io.write_reg(self.mode.reg(), v)?;
        }

        self.state = State::Running;
        Ok(())
    }

    /// Fire a software trigger. Still mode only; a no-op for video.
    ///
    /// **Still capture does not expose without this.** Enabling arms the camera;
    /// the trigger is what makes it take a frame. The stream must be running —
    /// the camera refuses a trigger otherwise, which is why this is separate
    /// from `start` rather than folded into it.
    pub fn trigger<T: StreamTransport>(&mut self, io: &mut T) -> Result<()> {
        let Some(value) = self.mode.software_trigger() else {
            return Ok(());
        };
        if self.state != State::Running {
            return Err(err(
                "Lumenera still trigger requires a running stream; the camera refuses it otherwise",
            ));
        }
        io.write_reg(self.mode.reg(), value)
    }

    /// Stop emitting. Running -> Paused.
    pub fn stop<T: StreamTransport>(&mut self, io: &mut T, kind: StopKind) -> Result<()> {
        self.stop_inner(io, kind)
    }

    fn stop_inner<T: StreamTransport>(&mut self, io: &mut T, kind: StopKind) -> Result<()> {
        if !matches!(self.state, State::Starting | State::Running) {
            return Ok(());
        }
        self.state = State::Paused;

        // Tell the camera first, before touching the transfers: a camera still
        // emitting into cancelled transfers is how a stream wedges.
        //
        // The zero-length-packet request precedes the disable on **any** stop,
        // not just a teardown. The vendor condition is "wanted state at or below
        // paused", and pause is inside that. An earlier revision restricted it
        // to teardown, which left a pause without the clean transfer
        // termination it is there to provide.
        if self.camera_enabled {
            if self.mode.requests_zlp() {
                let _ = io.write_reg(self.mode.reg(), VIDEO_REQUEST_ZLP);
            }
            let _ = io.write_reg(self.mode.reg(), self.mode.disable());
            self.camera_enabled = false;
        }
        let _ = kind;

        // Walk the ring from after the last completion, not from zero.
        let from = (self.last_completed + 1) % self.pool.max(1);
        let _ = io.kill(from, self.pool);

        // Unconditional on USB 2. The error-guarded form belongs to the USB 3
        // transport, where clearing a halt that does not exist misbehaves.
        if io.bus_speed() != BusSpeed::Super || self.frame_error {
            let _ = io.clear_halt();
        }

        // Recovery is issued here, not where it was detected, and only on the
        // legacy USB3 endpoint path that actually defines command 0x21 for this.
        if self.recovery_outstanding && io.stream_recovery_supported() {
            io.ext_cmd(EXT_STREAM_RECOVERY)?;
            self.recovery_outstanding = false;
        }

        self.frame_error = false;
        io.reset_frames();
        Ok(())
    }

    /// Give the resources back. Paused -> Stopped.
    pub fn release<T: StreamTransport>(&mut self, io: &mut T) -> Result<()> {
        if matches!(self.state, State::Starting | State::Running) {
            self.stop_inner(io, StopKind::Teardown)?;
        }
        if self.state == State::Stopped {
            return Ok(());
        }
        io.set_alt_setting(AltSetting::Idle)?;
        self.state = State::Stopped;
        Ok(())
    }
}
