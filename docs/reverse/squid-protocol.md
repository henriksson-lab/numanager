# Squid Hardware Interface Specification

Interface specification for a clean-room `numanager` driver for the Cephla Squid
controller.

| Item | Value |
| --- | --- |
| Evidence class | Open-source controller firmware and its public Python host wrapper (current generation, not the legacy sketches) |
| Hardware validation | **None recorded.** No captured traffic or bench run from a physical controller yet |
| Firmware covered | 1.4 |

## Scope

The Squid controller is a USB-serial microcontroller hub exposing multiple
logical devices over one link: XY stage, Z stage, theta axis, W and W2
filter-wheel axes, illumination ports and LED matrix, camera trigger outputs and
strobe timing, onboard DAC channels, autofocus laser pin control, and
joystick/switch status.

Camera image acquisition is **not** part of this protocol. A driver should model
the microcontroller as a hub exposing camera trigger synchronization, and leave
image acquisition to a separate camera driver.

## Transport

| Item | Value |
| --- | --- |
| Physical | USB serial exposed by the controller board |
| Baud rate | `2_000_000` |
| Serial timeout (firmware) | 200 ms |
| Status cadence | firmware emits a status frame roughly every 10 ms |
| Port identification | USB manufacturer string `Teensyduino` (Linux/macOS); `Microsoft` on the Windows fallback path; description `Arduino Due` for older boards. Optional serial-number matching |

Discovery should be two-stage:

1. Detect candidate serial ports by USB metadata and optionally by config file.
2. Claim a candidate, open it, send a harmless command such as
   `TURN_OFF_ALL_PORTS`, and read a valid status frame to identify firmware
   version and protocol support.

## Framing

All multi-byte integers are big-endian.

### Host Command Frame — 8 bytes

| Byte | Meaning |
| --- | --- |
| 0 | command id, wrapping `u8` |
| 1 | command code |
| 2-5 | primary payload |
| 6 | reserved, or extended payload byte for selected commands |
| 7 | CRC-8/CCITT over bytes 0-6 |

Signed 32-bit payloads use two's-complement in bytes 2-5 or 3-6, depending on
command layout.

### Controller Status Frame — 24 bytes

| Byte | Meaning |
| --- | --- |
| 0 | last command id seen by controller |
| 1 | command execution status |
| 2-5 | X position, signed i32 microsteps or encoder counts |
| 6-9 | Y position, signed i32 |
| 10-13 | Z position, signed i32 |
| 14-17 | theta position, signed i32 |
| 18 | button/switch bitfield |
| 19-21 | reserved |
| 22 | firmware version, high nibble major, low nibble minor |
| 23 | CRC-8/CCITT over bytes 0-22 |

Older firmware may send `0` as the response CRC. A driver should accept zero only
in a legacy compatibility mode and mark that device as lower confidence.

## Status Model

| Code | Meaning |
| --- | --- |
| 0 | completed without errors |
| 1 | in progress |
| 2 | command checksum error |
| 3 | invalid command |
| 4 | command execution error |

Status frames are emitted independently of host commands. Movement commands set
`in_progress`; atomic commands generally complete on the next status frame. The
driver should run a dedicated read loop, match status frames by command id, and
complete operations from hardware status transitions
(`Submitted -> Accepted/InProgress -> Completed | Failed | LostSync`).

Recommended retry policy: resend if no matching acknowledgement arrives within
0.5 s, up to 5 attempts; resend on checksum error; fail fast on command
execution error; treat repeated ack timeout or checksum failure as uncertain
motor state.

## Firmware Version

```text
version = (major << 4) | (minor & 0x0f)
```

| Version | Meaning |
| --- | --- |
| 1.0 | multi-port illumination support |
| 1.1 | serial watchdog for illumination auto-shutoff |
| 1.2 | command execution error reporting and `MOVETO_W2` |
| 1.3 | strobe ISR latches illumination source at trigger start |
| 1.4 | W/W2 filter-wheel homing range fix |

## Axis Identifiers

Protocol axis identifiers differ from firmware-internal array indices; use the
protocol ids on the wire.

| Axis | Protocol id |
| --- | --- |
| X | 0 |
| Y | 1 |
| Z | 2 |
| Theta | 3 |
| XY aggregate | 4 |
| W filter wheel | 5 |
| W2 filter wheel | 6 |

The status frame reports X, Y, Z, and theta only. W/W2 positions are tracked by
command completion, not reported.

## Homing And Zeroing

`HOME_OR_ZERO` modes: `0` home positive, `1` home negative, `2` zero current
position. For XY homing, byte 3 carries X direction and byte 4 carries Y
direction.

## Command Table

### Motion

| Code | Name | Payload |
| --- | --- | --- |
| 0 | `MOVE_X` | bytes 2-5: signed i32 relative microsteps |
| 1 | `MOVE_Y` | bytes 2-5: signed i32 relative microsteps |
| 2 | `MOVE_Z` | bytes 2-5: signed i32 relative microsteps |
| 3 | `MOVE_THETA` | bytes 2-5: signed i32 relative microsteps |
| 4 | `MOVE_W` | bytes 2-5: signed i32 relative microsteps |
| 6 | `MOVETO_X` | bytes 2-5: signed i32 absolute microsteps |
| 7 | `MOVETO_Y` | bytes 2-5: signed i32 absolute microsteps |
| 8 | `MOVETO_Z` | bytes 2-5: signed i32 absolute microsteps |
| 18 | `MOVETO_W` | bytes 2-5: signed i32 absolute microsteps |
| 19 | `MOVE_W2` | bytes 2-5: signed i32 relative microsteps |
| 43 | `MOVETO_W2` | bytes 2-5: signed i32 absolute microsteps |

### Homing, Limits, And Axis Configuration

| Code | Name | Payload |
| --- | --- | --- |
| 5 | `HOME_OR_ZERO` | byte 2 axis, byte 3 mode, byte 4 optional Y mode for XY |
| 9 | `SET_LIM` | byte 2 limit code, bytes 3-6 signed i32 microstep limit |
| 20 | `SET_LIM_SWITCH_POLARITY` | byte 2 axis, byte 3 polarity |
| 21 | `CONFIGURE_STEPPER_DRIVER` | byte 2 axis, byte 3 microstepping, bytes 4-5 RMS current mA, byte 6 hold current scaled 0-255 |
| 22 | `SET_MAX_VELOCITY_ACCELERATION` | byte 2 axis, bytes 3-4 velocity mm/s ×100, bytes 5-6 acceleration mm/s² ×10 |
| 23 | `SET_LEAD_SCREW_PITCH` | byte 2 axis, bytes 3-4 pitch mm ×1000 |
| 24 | `SET_OFFSET_VELOCITY` | byte 2 axis, bytes 3-6 signed i32 velocity mm/s ×1,000,000 |
| 25 | `CONFIGURE_STAGE_PID` | byte 2 axis, byte 3 flip direction, bytes 4-5 transitions per revolution |
| 26 | `ENABLE_STAGE_PID` | byte 2 axis |
| 27 | `DISABLE_STAGE_PID` | byte 2 axis |
| 28 | `SET_HOME_SAFETY_MERGIN` | byte 2 axis, bytes 3-4 margin micrometers |
| 29 | `SET_PID_ARGUMENTS` | byte 2 axis, bytes 3-4 P, byte 5 I, byte 6 D |
| 32 | `SET_AXIS_DISABLE_ENABLE` | byte 2 axis, byte 3 status |
| 252 | `INITFILTERWHEEL_W2` | no payload |
| 253 | `INITFILTERWHEEL` | no payload |

Limit codes: X+ `0`, X- `1`, Y+ `2`, Y- `3`, Z+ `4`, Z- `5`.
Limit switch polarity: active low `0`, active high `1`, disabled `2`.

### Illumination

| Code | Name | Payload |
| --- | --- | --- |
| 10 | `TURN_ON_ILLUMINATION` | legacy current source on |
| 11 | `TURN_OFF_ILLUMINATION` | all legacy illumination off |
| 12 | `SET_ILLUMINATION` | byte 2 legacy source, bytes 3-4 intensity percent mapped to 0-65535 |
| 13 | `SET_ILLUMINATION_LED_MATRIX` | byte 2 pattern, byte 3 green, byte 4 red, byte 5 blue, each 0-255 |
| 17 | `SET_ILLUMINATION_INTENSITY_FACTOR` | byte 2 percent factor, clamped 0-100 |
| 34 | `SET_PORT_INTENSITY` | byte 2 port index, bytes 3-4 intensity percent mapped to 0-65535 |
| 35 | `TURN_ON_PORT` | byte 2 port index |
| 36 | `TURN_OFF_PORT` | byte 2 port index |
| 37 | `SET_PORT_ILLUMINATION` | byte 2 port index, bytes 3-4 intensity, byte 5 on flag |
| 38 | `SET_MULTI_PORT_MASK` | bytes 2-3 port mask, bytes 4-5 on mask |
| 39 | `TURN_OFF_ALL_PORTS` | no payload |
| 40 | `SET_WATCHDOG_TIMEOUT` | bytes 2-5 unsigned timeout ms, 0 = firmware default |
| 42 | `HEARTBEAT` | no payload |

Port indices 0-15 are supported. Named ports:

| Port | Index | Legacy source code |
| --- | --- | --- |
| D1 | 0 | 11 |
| D2 | 1 | 12 |
| D3 | 2 | 14 |
| D4 | 3 | 13 |
| D5 | 4 | 15 |

Wavelength is a software configuration concern, not a protocol fact — expose
wavelength metadata only when a config file declares it.

LED matrix patterns: full array `0`, left half `1`, right half `2`, left
blue/right red `3`, low NA `4`, left dot `5`, right dot `6`, top half `7`,
bottom half `8`, external FET `20`.

### Trigger, DAC, GPIO, And System

| Code | Name | Payload |
| --- | --- | --- |
| 14 | `ACK_JOYSTICK_BUTTON_PRESSED` | no payload |
| 15 | `ANALOG_WRITE_ONBOARD_DAC` | byte 2 DAC channel, bytes 3-4 unsigned value |
| 16 | `SET_DAC80508_REFDIV_GAIN` | byte 2 div, byte 3 gains |
| 30 | `SEND_HARDWARE_TRIGGER` | byte 2 control-strobe flag in bit 7 plus camera channel in low nibble, bytes 3-6 illumination on-time µs |
| 31 | `SET_STROBE_DELAY` | byte 2 camera channel, bytes 3-6 delay µs |
| 33 | `SET_TRIGGER_MODE` | byte 2 mode |
| 41 | `SET_PIN_LEVEL` | byte 2 MCU pin, byte 3 level |

Maintenance opcodes exist in this range but are intentionally omitted from the
public command surface.

Controller pins:

| Function | Pin |
| --- | --- |
| Illumination D1-D5 | 5, 4, 22, 3, 23 |
| Illumination interlock | 2 |
| Autofocus laser | 15 |
| Camera trigger outputs | 29, 30, 31, 32 |
| DAC80508 chip select | 33 |

## Safety

The firmware has a serial watchdog for illumination. When enabled by
`SET_WATCHDOG_TIMEOUT`, every valid serial message resets the timer; on expiry
the firmware turns off all illumination and disables the watchdog until
re-enabled.

Driver requirements:

- Enable the watchdog when controlling illumination-capable devices.
- Send `HEARTBEAT` at less than half the watchdog interval during active
  illumination.
- Stop heartbeat and turn off all ports on shutdown.
- Surface watchdog support based on firmware version.
- Refuse `SET_PIN_LEVEL` outside an explicit unsafe mode.

## Suggested numanager Model

One hub driver remultiplexing logical device commands onto a single serial
command queue:

| Device | Role |
| --- | --- |
| `squid.controller` | hub, firmware version, raw status stream, watchdog |
| `squid.xy_stage` / `squid.z_stage` / `squid.theta` | logical stages |
| `squid.filter_wheel.w` / `.w2` | filter wheels |
| `squid.illumination.port.N` | one logical device per configured port |
| `squid.led_matrix` | optional LED matrix source |
| `squid.trigger.N` | camera trigger output and strobe timing |
| `squid.dac.N` | DAC channel, diagnostic/raw capability |
| `squid.autofocus` | provider for core `CapabilityKind::Autofocus` |

`squid.autofocus` is one Squid-backed implementation of the general autofocus
device model, not the definition of autofocus in `numanager`. It drives a
firmware pin internally, but the public device exposes provider-neutral
properties (`enabled`, `mode`, `status`, `focus_score`); pin-oriented fields
belong in metadata.

Units belong in the type, not in string keys: stage positions (microsteps
internally, µm/mm publicly once geometry is configured), typed velocity and
acceleration, `Value::Wavelength` from config, typed durations for
exposure/trigger timing.

## Untested / Open Before Hardware Control

- USB VID/PID values for the production controller variants.
- Whether all deployed controllers now emit a response CRC, or zero-CRC
  compatibility is still required.
- Trigger mode byte values.
- Stage calibration source of truth per hardware variant.
- Whether W/W2 positions need an explicit query/telemetry extension or should be
  tracked as driver-estimated state only.
