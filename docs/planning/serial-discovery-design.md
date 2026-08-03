# Serial Discovery Research

Serial autoprobing is out of scope. Passive OS inventory can help a user choose
a port, and explicit configured endpoints remain supported, but numanager should
not send serial bytes as discovery probes.

## Current Code State

- Core serial support can open a named port and read/write bytes.
- No driver/core code enumerates serial ports with `serialport::available_ports`.
- No code scans `/dev/tty*`, `COM1` ranges, baud matrices, or multiple driver
  probes against unknown ports.
- Existing deprecated `active_probe` config keys and
  `startup_readback_supported` metadata mean configured live startup readback
  during connection initialization, not discovery.
- USB descriptor scanning exists for some devices, but that is passive USB
  metadata and does not send serial bytes.

## Unsafe Scope

Do not implement:

- Default scans of every serial port.
- Broad baud-rate, parity, stop-bit, or line-ending matrices.
- Per-driver races where multiple drivers open and probe the same port.
- Binary protocol probes on unknown ports.
- Recovery-byte strategies such as CR, LF, ESC, Ctrl-C, or close/reopen unless
  the candidate protocol family is already known.
- Startup commands that reset, enable, clear, configure, move, output, arm, or
  write state as discovery probes.
- Configured-port active confirmation probes as a discovery feature.

## Rationale

Serial discovery differs from USB/HID descriptor discovery because sending bytes
is often required before identity is known. That first probe is a hardware
command.

The main risks are:

- Devices disagree on command framing: LF, CR, CRLF, prompt replies, fixed
  binary frames, mixed binary/ASCII frames, and checksum trailers.
- A command sent with the wrong terminator can leave another parser holding an
  incomplete command. A later terminator may complete an unintended command.
- Recovery bytes are protocol-specific; a harmless CR for one device may be a
  command for another.
- Baud rate is part of protocol identity. A low baud rate is not inherently
  safer; wrong baud produces garbage that can still be buffered or
  misinterpreted.
- Binary protocols have no universal empty command, terminator, or recovery
  sequence.
- Several existing configured startup paths are not discovery-safe because they
  reset modules, enable channels, clear errors, change communication/prompt
  mode, disable autostatus, or write velocity/profile state.
- Multi-device buses require one owner for the port; independent probes can
  race or duplicate-open the same resource.

## Binary Serial Protocols In Scope

| Device/family | Protocol shape |
| --- | --- |
| Standa 8SMC4 | Binary command/reply frames with CRC16 |
| Thorlabs APT | APT binary message frames |
| Sutter MP-285 | Binary serial command/reply protocol |
| Trinamic TMCL | Fixed 9-byte direct-mode frames with checksum |
| Modbus RTU | Binary RTU frames with CRC16 |
| Teensy Pulse | Fixed binary firmware frames |
| Lumencor Spectra | Legacy binary serial frames |
| Starlight Xpress filter wheel | Four-byte binary frames with checksum |
| Squid | Fixed binary status frames on serial |
| Agilent laser combiner | Binary command byte/payload with ASCII reply text |

These devices should not be probed on unknown ports.

## Implementation Consequences

1. Passive serial inventory may be implemented as a helper for user selection.
2. Live serial I/O should require explicit configured opt-in, preferably
   `connect = true`.
3. Drivers that open live serial merely because `serial_port` is present should
   be fixed.
4. Line settings such as stop bits must be applied exactly, not only recorded in
   metadata.
5. Deprecated `active_probe` config wording should be replaced with
   `startup_readback` where compatibility does not require the old alias.
6. Multi-device serial buses should be represented by one resource owner.
7. Active discovery probes should not be added.
