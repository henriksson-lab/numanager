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
//! 3. **The enable is read back as a health check.** A failed read, or any bit
//!    set above the low byte, means the camera needs a stream recovery. Recovery
//!    is issued during *stop*, and start retries exactly once — never an
//!    in-place fix.
//!
//!    **This third one is deliberate hardening, not replication.** The read-back,
//!    the recovery command and the retry all belong to the vendor's USB 3
//!    transport; its USB 2 transport writes the enable and assumes success. We
//!    do the check on both because a silent enable failure is otherwise
//!    indistinguishable from a camera that simply never sends a frame — which is
//!    the symptom this driver already had. If it proves to misbehave on USB 2
//!    hardware, this is the thing to switch off first.
//!
//! Specified in `reveng-dll/teledyne/lucam-protocol-spec.md` §4.3–4.4. This is an
//! independent implementation from that specification.
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
            Mode::Still { hardware_trigger: true, .. } => Some(STILL_ARM_HARDWARE),
            _ => None,
        }
    }
    /// The software-trigger value, or `None` where triggering does not apply.
    fn software_trigger(self) -> Option<u32> {
        match self {
            Mode::Video => None,
            Mode::Still { spec_version: 0, .. } => Some(STILL_TRIGGER_V0),
            Mode::Still { hardware_trigger: false, .. } => Some(STILL_TRIGGER_SOFTWARE),
            Mode::Still { hardware_trigger: true, .. } => Some(STILL_TRIGGER_HARDWARE),
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

/// What streaming needs from a transport. The driver supplies the USB one.
pub trait StreamTransport {
    fn read_reg(&mut self, index: u16) -> Result<u32>;
    fn write_reg(&mut self, index: u16, value: u32) -> Result<()>;
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
}

impl Stream {
    pub const DEFAULT_POOL: usize = 15;

    /// A video stream. Use [`Stream::new_still`] for single-frame capture.
    pub fn new(pool: usize) -> Self {
        Self::with_mode(pool, Mode::Video)
    }

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
        }
    }

    pub fn state(&self) -> State {
        self.state
    }
    pub fn mode(&self) -> Mode {
        self.mode
    }
    pub fn frames(&self) -> u64 {
        self.frames
    }
    pub fn recovery_pending(&self) -> bool {
        self.recovery_outstanding
    }

    /// Record a completed transfer so teardown knows where the ring is.
    pub fn note_completed(&mut self, slot: usize) {
        self.last_completed = slot;
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
            State::Running => return Ok(()),
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
                    if self.recovery_outstanding && !retried {
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
        // Seeded one before the start so the first teardown sweep begins at
        // slot 0 rather than slot 1.
        self.last_completed = self.pool.saturating_sub(1);

        // Everything in flight *before* the camera is told to go.
        for slot in 0..self.pool {
            io.submit(slot)?;
        }

        io.write_reg(self.mode.reg(), self.mode.enable())?;

        // The read-back is a health check, not a formality.
        match io.read_reg(self.mode.reg()) {
            Err(_) => {
                self.recovery_outstanding = true;
                return Err(err(
                    "Lumenera VIDEO_EN read-back failed after enabling; stream recovery required",
                ));
            }
            Ok(v) if v & VIDEO_EN_FAULT_MASK != 0 => {
                self.recovery_outstanding = true;
                return Err(err(format!(
                    "Lumenera VIDEO_EN read back as {v:#010x} after enabling; stream recovery required"
                )));
            }
            Ok(_) => {}
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
        if self.state != State::Running {
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
        if self.mode.requests_zlp() {
            let _ = io.write_reg(self.mode.reg(), VIDEO_REQUEST_ZLP);
        }
        let _ = io.write_reg(self.mode.reg(), self.mode.disable());
        let _ = kind;

        // Walk the ring from after the last completion, not from zero.
        let from = (self.last_completed + 1) % self.pool.max(1);
        let _ = io.kill(from, self.pool);

        // Unconditional on USB 2. The error-guarded form belongs to the USB 3
        // transport, where clearing a halt that does not exist misbehaves.
        if io.bus_speed() != BusSpeed::Super || self.frame_error {
            let _ = io.clear_halt();
        }

        // Recovery is issued here, not where it was detected. USB 3 behaviour
        // adopted on all transports; see the module header.
        if self.recovery_outstanding {
            io.ext_cmd(EXT_STREAM_RECOVERY)?;
            self.recovery_outstanding = false;
        }

        self.frame_error = false;
        io.reset_frames();
        Ok(())
    }

    /// Give the resources back. Paused -> Stopped.
    pub fn release<T: StreamTransport>(&mut self, io: &mut T) -> Result<()> {
        if self.state == State::Running {
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

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;

    #[derive(Default)]
    pub(crate) struct Fake {
        pub(crate) log: Vec<String>,
        /// values the enable register reads back, popped in order
        pub(crate) readback: Vec<Option<u32>>,
        pub(crate) speed_super: bool,
        pub(crate) reg_writes: Vec<(u16, u32)>,
    }

    impl Fake {
        pub(crate) fn new() -> Self {
            Self::default()
        }
    }

    impl StreamTransport for Fake {
        fn read_reg(&mut self, index: u16) -> Result<u32> {
            self.log.push(format!("read:{index:#06x}"));
            match self.readback.pop() {
                Some(Some(v)) => Ok(v),
                Some(None) => Err(err("simulated read failure")),
                None => Ok(1),
            }
        }
        fn write_reg(&mut self, index: u16, value: u32) -> Result<()> {
            self.reg_writes.push((index, value));
            self.log.push(format!("write:{value:#x}"));
            Ok(())
        }
        fn ext_cmd(&mut self, sub: u8) -> Result<()> {
            self.log.push(format!("ext:{sub:#x}"));
            Ok(())
        }
        fn set_alt_setting(&mut self, alt: AltSetting) -> Result<()> {
            self.log.push(format!("alt:{alt:?}"));
            Ok(())
        }
        fn submit(&mut self, slot: usize) -> Result<()> {
            self.log.push(format!("submit:{slot}"));
            Ok(())
        }
        fn kill(&mut self, from: usize, count: usize) -> Result<()> {
            self.log.push(format!("kill:{from}+{count}"));
            Ok(())
        }
        fn clear_halt(&mut self) -> Result<()> {
            self.log.push("clear_halt".into());
            Ok(())
        }
        fn reset_frames(&mut self) {
            self.log.push("reset_frames".into());
        }
        fn bus_speed(&self) -> BusSpeed {
            if self.speed_super {
                BusSpeed::Super
            } else {
                BusSpeed::High
            }
        }
    }

    fn started(pool: usize) -> (Stream, Fake) {
        let mut s = Stream::new(pool);
        let mut io = Fake::new();
        s.acquire(&mut io).unwrap();
        s.start(&mut io).unwrap();
        (s, io)
    }

    /// The ordering fact the whole module exists for.
    #[test]
    fn every_transfer_is_submitted_before_the_camera_is_enabled() {
        let (_, io) = started(15);
        let enable = io
            .log
            .iter()
            .position(|l| l == &format!("write:{VIDEO_ENABLE:#x}"))
            .expect("enable must happen");
        let submits: Vec<usize> = io
            .log
            .iter()
            .enumerate()
            .filter(|(_, l)| l.starts_with("submit:"))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(submits.len(), 15, "the whole pool must be in flight");
        assert!(
            submits.iter().all(|&i| i < enable),
            "a transfer was submitted after enable: {:?}",
            io.log
        );
    }

    #[test]
    fn enable_is_not_a_boolean() {
        let (_, io) = started(2);
        assert!(io.log.contains(&"write:0xffffffff".to_string()), "{:?}", io.log);
        assert!(!io.log.contains(&"write:0x1".to_string()));
    }

    /// Zero-length packets are requested before the disable on **every** stop,
    /// pause included. An earlier revision restricted this to teardown.
    #[test]
    fn zero_length_packets_are_requested_before_every_disable() {
        for kind in [StopKind::Teardown, StopKind::Pause] {
            let (mut s, mut io) = started(2);
            io.log.clear();
            s.stop(&mut io, kind).unwrap();
            let zlp = io.log.iter().position(|l| l == "write:0x40")
                .unwrap_or_else(|| panic!("{kind:?} must request ZLP: {:?}", io.log));
            let off = io.log.iter().position(|l| l == "write:0x0").expect("disable");
            assert!(zlp < off, "{kind:?}: 0x40 must precede 0x00: {:?}", io.log);
        }
    }

    /// Camera first, transfers second.
    #[test]
    fn stop_disables_the_camera_before_killing_transfers() {
        let (mut s, mut io) = started(4);
        io.log.clear();
        s.stop(&mut io, StopKind::Pause).unwrap();
        let off = io.log.iter().position(|l| l == "write:0x0").unwrap();
        let kill = io.log.iter().position(|l| l.starts_with("kill:")).unwrap();
        assert!(off < kill, "{:?}", io.log);
    }

    /// The ring is walked from after the last completion.
    #[test]
    fn kill_starts_after_the_last_completed_slot() {
        let (mut s, mut io) = started(8);
        s.note_completed(5);
        io.log.clear();
        s.stop(&mut io, StopKind::Pause).unwrap();
        assert!(io.log.contains(&"kill:6+8".to_string()), "{:?}", io.log);
    }

    /// With nothing completed yet the sweep starts at slot 0, which is why the
    /// ring is seeded one before the start rather than at zero.
    #[test]
    fn first_sweep_starts_at_slot_zero() {
        let (mut s, mut io) = started(8);
        io.log.clear();
        s.stop(&mut io, StopKind::Pause).unwrap();
        assert!(io.log.contains(&"kill:0+8".to_string()), "{:?}", io.log);
    }

    /// A read-back with high bits set means recovery, and enable has failed.
    #[test]
    fn bad_readback_schedules_recovery_and_fails_enable() {
        let mut s = Stream::new(2);
        let mut io = Fake::new();
        // Both the first attempt and the retry read back faulted.
        io.readback = vec![Some(0x0000_0100), Some(0x0000_0100)];
        s.acquire(&mut io).unwrap();
        let e = s.start(&mut io).unwrap_err();
        assert!(format!("{e}").contains("recovery"), "{e}");
        assert_eq!(s.state(), State::Paused);
    }

    #[test]
    fn failed_readback_read_also_schedules_recovery() {
        let mut s = Stream::new(2);
        let mut io = Fake::new();
        io.readback = vec![None, None];
        s.acquire(&mut io).unwrap();
        assert!(s.start(&mut io).is_err());
    }

    /// Recovery is a stop-then-start, issued during stop, retried exactly once.
    #[test]
    fn recovery_is_issued_during_stop_and_start_retries_once() {
        let mut s = Stream::new(2);
        let mut io = Fake::new();
        // First enable faults; the retry reads back clean.
        io.readback = vec![Some(1), Some(0x0000_0200)];
        s.acquire(&mut io).unwrap();
        s.start(&mut io).expect("the single retry should succeed");

        assert_eq!(
            io.log.iter().filter(|l| *l == &format!("ext:{EXT_STREAM_RECOVERY:#x}")).count(),
            1,
            "recovery sent exactly once: {:?}",
            io.log
        );
        assert_eq!(
            io.log.iter().filter(|l| l.as_str() == "write:0xffffffff").count(),
            2,
            "enabled twice: once failing, once on the retry"
        );
        assert!(!s.recovery_pending(), "flag must be cleared once sent");
        assert_eq!(s.state(), State::Running);
    }

    /// Two failures propagate; it does not retry forever.
    #[test]
    fn a_second_failure_propagates() {
        let mut s = Stream::new(2);
        let mut io = Fake::new();
        io.readback = vec![Some(0x0000_0300), Some(0x0000_0400)];
        s.acquire(&mut io).unwrap();
        assert!(s.start(&mut io).is_err());
        assert_eq!(
            io.log.iter().filter(|l| l.as_str() == "write:0xffffffff").count(),
            2,
            "exactly one retry"
        );
    }

    /// On USB 3 the halt is only cleared when a frame error actually happened.
    #[test]
    fn halt_clearing_is_conditional_on_bus_and_error() {
        let mut s = Stream::new(2);
        let mut io = Fake::new();
        io.speed_super = true;
        s.acquire(&mut io).unwrap();
        s.start(&mut io).unwrap();
        io.log.clear();
        s.stop(&mut io, StopKind::Pause).unwrap();
        assert!(!io.log.contains(&"clear_halt".to_string()), "no error, USB3: {:?}", io.log);
        // ...but USB 2 clears unconditionally, which is the shipped transport.

        s.start(&mut io).unwrap();
        s.note_frame_error(false);
        io.log.clear();
        s.stop(&mut io, StopKind::Pause).unwrap();
        assert!(io.log.contains(&"clear_halt".to_string()), "error, USB3: {:?}", io.log);

        // USB 2 clears unconditionally.
        let (mut s2, mut io2) = started(2);
        io2.log.clear();
        s2.stop(&mut io2, StopKind::Pause).unwrap();
        assert!(io2.log.contains(&"clear_halt".to_string()));
    }

    #[test]
    fn lifecycle_is_idempotent_where_it_should_be() {
        let mut s = Stream::new(3);
        let mut io = Fake::new();
        assert!(s.start(&mut io).is_err(), "cannot start before acquire");
        s.acquire(&mut io).unwrap();
        s.acquire(&mut io).unwrap(); // second acquire is a no-op
        s.start(&mut io).unwrap();
        s.start(&mut io).unwrap(); // already running
        assert_eq!(io.log.iter().filter(|l| l.starts_with("submit:")).count(), 3);
        s.stop(&mut io, StopKind::Pause).unwrap();
        s.stop(&mut io, StopKind::Pause).unwrap(); // already paused
        s.release(&mut io).unwrap();
        assert_eq!(s.state(), State::Stopped);
        assert_eq!(io.log.last().unwrap(), "alt:Idle");
    }

    /// Release from Running must take the camera down properly on the way.
    #[test]
    fn release_while_running_tears_down_first() {
        let (mut s, mut io) = started(2);
        io.log.clear();
        s.release(&mut io).unwrap();
        assert!(io.log.contains(&"write:0x40".to_string()), "teardown: {:?}", io.log);
        assert_eq!(io.log.last().unwrap(), "alt:Idle");
    }
}

#[cfg(test)]
mod mode_tests {
    use super::tests_support::Fake;
    use super::*;

    /// Still mode drives a different register with different values — not the
    /// same register with a different payload.
    #[test]
    fn still_mode_uses_trigger_ctrl() {
        let mut s = Stream::new_still(2, 1, false);
        let mut io = Fake::new();
        s.acquire(&mut io).unwrap();
        s.start(&mut io).unwrap();
        assert!(io.reg_writes.contains(&(REG_TRIGGER_CTRL, STILL_ENABLE)), "{:?}", io.reg_writes);
        assert!(
            !io.reg_writes.iter().any(|(r, _)| *r == REG_VIDEO_EN),
            "still capture must not touch VIDEO_EN: {:?}",
            io.reg_writes
        );
    }

    /// Enable then trigger. Without the trigger the camera arms and never
    /// exposes, which presents as a camera that simply never sends a frame.
    #[test]
    fn still_capture_enables_then_triggers() {
        let mut s = Stream::new_still(2, 1, false);
        let mut io = Fake::new();
        s.acquire(&mut io).unwrap();
        s.start(&mut io).unwrap();
        s.trigger(&mut io).unwrap();
        assert_eq!(
            io.reg_writes,
            vec![
                (REG_TRIGGER_CTRL, STILL_ENABLE),
                (REG_TRIGGER_CTRL, STILL_TRIGGER_SOFTWARE),
            ],
            "0x04 then 0x06"
        );
    }

    /// The trigger encoding depends on protocol version and trigger mode.
    #[test]
    fn trigger_encoding_follows_version_and_mode() {
        let cases = [
            ((0, false), STILL_TRIGGER_V0),
            ((1, false), STILL_TRIGGER_SOFTWARE),
            ((2, false), STILL_TRIGGER_SOFTWARE),
            ((1, true), STILL_TRIGGER_HARDWARE),
        ];
        for ((ver, hw), want) in cases {
            let mut s = Stream::new_still(1, ver, hw);
            let mut io = Fake::new();
            s.acquire(&mut io).unwrap();
            s.start(&mut io).unwrap();
            io.reg_writes.clear();
            s.trigger(&mut io).unwrap();
            assert_eq!(io.reg_writes, vec![(REG_TRIGGER_CTRL, want)], "v{ver} hw={hw}");
        }
    }

    /// Hardware-trigger mode takes an extra write straight after the enable.
    #[test]
    fn hardware_trigger_mode_arms_after_enable() {
        let mut s = Stream::new_still(1, 1, true);
        let mut io = Fake::new();
        s.acquire(&mut io).unwrap();
        s.start(&mut io).unwrap();
        assert_eq!(
            io.reg_writes,
            vec![
                (REG_TRIGGER_CTRL, STILL_ENABLE),
                (REG_TRIGGER_CTRL, STILL_ARM_HARDWARE),
            ]
        );
    }

    /// The camera refuses a trigger unless the stream is running.
    #[test]
    fn trigger_before_start_is_refused() {
        let mut s = Stream::new_still(1, 1, false);
        let mut io = Fake::new();
        s.acquire(&mut io).unwrap();
        assert!(s.trigger(&mut io).is_err());
    }

    /// Video mode has no trigger step.
    #[test]
    fn video_mode_trigger_is_a_no_op() {
        let mut s = Stream::new(1);
        let mut io = Fake::new();
        s.acquire(&mut io).unwrap();
        s.start(&mut io).unwrap();
        io.reg_writes.clear();
        s.trigger(&mut io).unwrap();
        assert!(io.reg_writes.is_empty());
    }

    /// The zero-length-packet request is video-only.
    #[test]
    fn still_mode_does_not_request_zero_length_packets() {
        let mut s = Stream::new_still(2, 1, false);
        let mut io = Fake::new();
        s.acquire(&mut io).unwrap();
        s.start(&mut io).unwrap();
        io.reg_writes.clear();
        s.stop(&mut io, StopKind::Teardown).unwrap();
        assert_eq!(io.reg_writes, vec![(REG_TRIGGER_CTRL, STILL_DISABLE)]);
    }

    /// Ordering discipline is the same in both modes.
    #[test]
    fn still_mode_still_submits_before_enabling() {
        let mut s = Stream::new_still(4, 1, false);
        let mut io = Fake::new();
        s.acquire(&mut io).unwrap();
        s.start(&mut io).unwrap();
        let enable = io.log.iter().position(|l| l.starts_with("write:")).unwrap();
        let last_submit = io.log.iter().rposition(|l| l.starts_with("submit:")).unwrap();
        assert!(last_submit < enable, "{:?}", io.log);
    }
}
