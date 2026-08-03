# Squid Hardware Interface Specification

This document is a first-pass interface specification for a clean-room
`numanager` driver for the Cephla Squid controller. It is extracted from the
current Squid repository's active controller firmware and Python host wrapper,
not from the legacy firmware sketches.

Primary source files used:

- controller protocol constants
- serial communication implementation
- command dispatch implementation
- stage command implementation
- light command implementation
- shared firmware constants
- `software/control/microcontroller.py`
- `software/control/_def.py`

## Scope

The Squid controller is a USB serial microcontroller hub. It exposes multiple
logical devices through one hardware link:

- XY stage
- Z stage
- theta axis
- W and W2 filter-wheel axes
- illumination ports and LED matrix
- camera trigger outputs and strobe timing
- onboard DAC channels
- autofocus laser pin control
- joystick button and switch status

Camera control in Squid software is separate from this controller protocol and
uses vendor-specific camera modules. A `numanager` Squid driver should model the
microcontroller as a hub and expose camera trigger synchronization capabilities,
but camera image acquisition should remain a separate camera driver unless a
specific camera protocol is implemented.

## Transport

- Physical transport: USB serial exposed by the controller board.
- Default baud rate: `2_000_000`.
- Existing host detection:
  - Linux/macOS path filters serial ports with manufacturer `Teensyduino`.
  - Windows fallback in the Squid wrapper filters manufacturer `Microsoft`.
  - Older Arduino Due support filters description `Arduino Due`.
  - Optional serial-number matching is supported by the Squid wrapper.
- Firmware serial object: `SerialUSB`.
- Firmware serial timeout: 200 ms.
- Status update cadence: firmware sends a status frame roughly every 10 ms.

Discovery for `numanager` should therefore be two-stage:

1. Detect candidate serial ports by USB metadata and optionally by config file.
2. Claim one candidate, open it, send a harmless command such as
   `TURN_OFF_ALL_PORTS`, and read a valid status frame to identify firmware
   version and protocol support.

## Framing

All multi-byte integers are big-endian.

### Host Command Frame

Length: 8 bytes.

| Byte | Meaning |
| --- | --- |
| 0 | command id, wrapping `u8` |
| 1 | command code |
| 2-5 | primary payload |
| 6 | reserved or extended payload byte for selected commands |
| 7 | CRC-8/CCITT over bytes 0-6 |

Signed 32-bit payloads use two's-complement encoding in bytes 2-5 or 3-6,
depending on command layout.

### Controller Status Frame

Length: 24 bytes.

| Byte | Meaning |
| --- | --- |
| 0 | last command id seen by controller |
| 1 | command execution status |
| 2-5 | X position, signed i32 microsteps or encoder counts |
| 6-9 | Y position, signed i32 microsteps or encoder counts |
| 10-13 | Z position, signed i32 microsteps or encoder counts |
| 14-17 | theta position, signed i32 microsteps or encoder counts |
| 18 | button/switch bitfield |
| 19-21 | reserved |
| 22 | firmware version, high nibble major and low nibble minor |
| 23 | CRC-8/CCITT over bytes 0-22 |

Older firmware may send `0` as the response CRC. The existing Squid host accepts
either a valid CRC or zero. A `numanager` driver should accept zero only in a
legacy compatibility mode and should mark that device as lower confidence.

## Status Model

Command execution status values:

| Code | Meaning |
| --- | --- |
| 0 | completed without errors |
| 1 | in progress |
| 2 | command checksum error |
| 3 | invalid command |
| 4 | command execution error |

The controller continues to emit status frames independently of host commands.
Movement commands set firmware `in_progress`; atomic commands generally complete
on the next status frame. The driver should run a dedicated read loop, match
status frames by command id, and complete `numanager` operations from hardware
status transitions.

The host-side retry policy in Squid is:

- If no matching acknowledgement arrives within 0.5 s, resend.
- Retry up to 5 times.
- Resend on checksum error.
- Fail fast on `CMD_EXECUTION_ERROR`.
- Treat repeated ack timeout/checksum failure as uncertain motor state.

For `numanager`, this should become a driver-owned operation state machine:
`Submitted -> Accepted/InProgress -> Completed | Failed | LostSync`.

## Firmware Version

The active firmware defines version 1.4. Version byte format:

```text
version = (major << 4) | (minor & 0x0f)
```

Known version meaning from firmware comments:

| Version | Meaning |
| --- | --- |
| 1.0 | multi-port illumination support |
| 1.1 | serial watchdog for illumination auto-shutoff |
| 1.2 | command execution error reporting and `MOVETO_W2` |
| 1.3 | strobe ISR latches illumination source at trigger start |
| 1.4 | W/W2 filter-wheel homing range fix |

## Axis Identifiers

Protocol axis identifiers are not the same as firmware internal array indices.
The driver must use protocol identifiers on the wire and keep internal mapping
separate.

| Axis | Protocol id |
| --- | --- |
| X | 0 |
| Y | 1 |
| Z | 2 |
| Theta | 3 |
| XY aggregate | 4 |
| W filter wheel | 5 |
| W2 filter wheel | 6 |

Positions reported by the firmware include X, Y, Z, and theta only. W/W2
positions are tracked by command completion but are not included in the 24-byte
status frame.

## Homing And Zeroing

`HOME_OR_ZERO` command modes:

| Mode | Code |
| --- | --- |
| home positive | 0 |
| home negative | 1 |
| zero current position | 2 |

For XY homing, byte 3 carries X direction and byte 4 carries Y direction.

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
| 22 | `SET_MAX_VELOCITY_ACCELERATION` | byte 2 axis, bytes 3-4 velocity mm/s times 100, bytes 5-6 acceleration mm/s^2 times 10 |
| 23 | `SET_LEAD_SCREW_PITCH` | byte 2 axis, bytes 3-4 pitch mm times 1000 |
| 24 | `SET_OFFSET_VELOCITY` | byte 2 axis, bytes 3-6 signed i32 velocity mm/s times 1,000,000 |
| 25 | `CONFIGURE_STAGE_PID` | byte 2 axis, byte 3 flip direction, bytes 4-5 transitions per revolution |
| 26 | `ENABLE_STAGE_PID` | byte 2 axis |
| 27 | `DISABLE_STAGE_PID` | byte 2 axis |
| 28 | `SET_HOME_SAFETY_MERGIN` | byte 2 axis, bytes 3-4 margin micrometers |
| 29 | `SET_PID_ARGUMENTS` | byte 2 axis, bytes 3-4 P, byte 5 I, byte 6 D |
| 32 | `SET_AXIS_DISABLE_ENABLE` | byte 2 axis, byte 3 status |
| 252 | `INITFILTERWHEEL_W2` | no payload |
| 253 | `INITFILTERWHEEL` | no payload |

Limit codes:

| Limit | Code |
| --- | --- |
| X positive | 0 |
| X negative | 1 |
| Y positive | 2 |
| Y negative | 3 |
| Z positive | 4 |
| Z negative | 5 |

Limit switch polarity:

| Polarity | Code |
| --- | --- |
| active low | 0 |
| active high | 1 |
| disabled | 2 |

### Illumination

| Code | Name | Payload |
| --- | --- | --- |
| 10 | `TURN_ON_ILLUMINATION` | legacy current source on |
| 11 | `TURN_OFF_ILLUMINATION` | all legacy illumination off |
| 12 | `SET_ILLUMINATION` | byte 2 legacy source, bytes 3-4 intensity percent mapped to 0-65535 |
| 13 | `SET_ILLUMINATION_LED_MATRIX` | byte 2 pattern, byte 3 green, byte 4 red, byte 5 blue, each 0-255 |
| 17 | `SET_ILLUMINATION_INTENSITY_FACTOR` | byte 2 percent factor, clamped to 0-100 |
| 34 | `SET_PORT_INTENSITY` | byte 2 port index, bytes 3-4 intensity percent mapped to 0-65535 |
| 35 | `TURN_ON_PORT` | byte 2 port index |
| 36 | `TURN_OFF_PORT` | byte 2 port index |
| 37 | `SET_PORT_ILLUMINATION` | byte 2 port index, bytes 3-4 intensity, byte 5 on flag |
| 38 | `SET_MULTI_PORT_MASK` | bytes 2-3 port mask, bytes 4-5 on mask |
| 39 | `TURN_OFF_ALL_PORTS` | no payload |
| 40 | `SET_WATCHDOG_TIMEOUT` | bytes 2-5 unsigned timeout milliseconds, 0 means firmware default |
| 42 | `HEARTBEAT` | no payload |

Multi-port illumination supports port indices 0-15. Current named Squid ports
are D1-D5:

| Port | Index | Legacy source code |
| --- | --- | --- |
| D1 | 0 | 11 |
| D2 | 1 | 12 |
| D3 | 2 | 14 |
| D4 | 3 | 13 |
| D5 | 4 | 15 |

Legacy wavelength names in Squid map to these D ports, but the actual wavelength
is a software configuration concern. A `numanager` driver should expose
illumination ports as named devices or channels with typed wavelength metadata
only when a config file declares it.

LED matrix pattern codes:

| Pattern | Code |
| --- | --- |
| full array | 0 |
| left half | 1 |
| right half | 2 |
| left blue/right red | 3 |
| low NA | 4 |
| left dot | 5 |
| right dot | 6 |
| top half | 7 |
| bottom half | 8 |
| external FET | 20 |

### Trigger, DAC, GPIO, And System

| Code | Name | Payload |
| --- | --- | --- |
| 14 | `ACK_JOYSTICK_BUTTON_PRESSED` | no payload |
| 15 | `ANALOG_WRITE_ONBOARD_DAC` | byte 2 DAC channel, bytes 3-4 unsigned value |
| 16 | `SET_DAC80508_REFDIV_GAIN` | byte 2 div, byte 3 gains |
| 30 | `SEND_HARDWARE_TRIGGER` | byte 2 control-strobe flag in bit 7 plus camera channel in low nibble, bytes 3-6 illumination on-time us |
| 31 | `SET_STROBE_DELAY` | byte 2 camera channel, bytes 3-6 delay us |
| 33 | `SET_TRIGGER_MODE` | byte 2 mode |
| 41 | `SET_PIN_LEVEL` | byte 2 MCU pin, byte 3 level |

Maintenance opcodes exist in this range but are intentionally omitted from the
public command surface.

Known controller pins from the active firmware:

| Function | Pin |
| --- | --- |
| Illumination D1 | 5 |
| Illumination D2 | 4 |
| Illumination D3 | 22 |
| Illumination D4 | 3 |
| Illumination D5 | 23 |
| Illumination interlock | 2 |
| Autofocus laser | 15 |
| Camera trigger outputs | 29, 30, 31, 32 |
| DAC80508 chip select | 33 |

## Safety

The firmware has a serial watchdog for illumination. When enabled by
`SET_WATCHDOG_TIMEOUT`, every valid serial message resets the watchdog timer. If
the timeout expires, firmware turns off all illumination and disables the
watchdog until re-enabled.

Driver requirements:

- Enable the watchdog when controlling illumination-capable devices.
- Send `HEARTBEAT` at less than half the watchdog interval during active
  illumination sessions.
- Stop heartbeat and turn off all ports on driver shutdown.
- Surface watchdog support based on firmware version.
- Refuse or require explicit unsafe mode for direct `SET_PIN_LEVEL` access.

## Suggested numanager Model

The Squid microcontroller should be one hub driver that remultiplexes logical
device commands onto a single serial command queue.

Suggested devices:

- `squid.controller`: hub, firmware version, raw status stream, watchdog.
- `squid.xy_stage`: X/Y logical stage using X and Y movement commands.
- `squid.z_stage`: Z logical stage.
- `squid.theta`: optional rotational axis.
- `squid.filter_wheel.w`: W filter wheel.
- `squid.filter_wheel.w2`: W2 filter wheel.
- `squid.illumination.port.N`: one logical illumination device per configured
  port.
- `squid.led_matrix`: optional LED matrix source.
- `squid.trigger.N`: camera trigger output and strobe timing.
- `squid.dac.N`: low-level DAC channel, exposed as a diagnostic/raw capability.
- `squid.autofocus`: provider for the core `CapabilityKind::Autofocus`,   by config. This is one Squid-backed implementation of the general autofocus
  device model, not the definition of autofocus in `numanager`. The current
  Squid implementation drives firmware pin 15 internally, but the public device
  is an autofocus endpoint rather than a raw pin, light gate, or Squid-specific
  device subtype. Public state should use provider-neutral properties such as
  `enabled`, `mode`, `status`, and `focus_score`; pin-oriented fields belong in
  metadata or compatibility diagnostics.

Properties should use typed quantities and explicit units in the type, not in
string keys:

- stage positions: microsteps internally, micrometers or millimeters in public
  properties once stage geometry is configured.
- velocity: typed length/time quantity or a domain enum wrapping mm/s.
- acceleration: typed length/time^2 quantity or a domain enum wrapping mm/s^2.
- wavelength: `Value::Wavelength`, provided by config for illumination ports.
- exposure/trigger timing: typed duration.

The first implementation should include:

1. Serial port discovery provider.
2. Frame encoder/decoder with CRC-8/CCITT.
3. Dedicated reader thread that publishes status frames and completes
   operations.
4. Controller hub plus XY, Z, illumination-port, trigger, and autofocus
   devices.
5. Config-driven port-to-wavelength and stage calibration metadata.
6. Simulator backed by the same frame encoder/decoder.

Open questions before implementing hardware control:

- Confirm USB VID/PID values for the production controller variants.
- Confirm whether all deployed controllers now emit response CRC, or if zero
  CRC compatibility is still required.
- Confirm trigger mode byte values in current software configuration.
- Confirm stage calibration source of truth for each Squid hardware variant.
- Decide whether W/W2 positions need an explicit query/telemetry extension or
  should be tracked as driver-estimated state only.
