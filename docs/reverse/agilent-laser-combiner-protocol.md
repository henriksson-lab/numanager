# Agilent Laser Combiner Wire Protocol

Interface specification for the combiner's serial command protocol. **Nothing
here has been observed against a real combiner**, and it does not authorise a
runtime driver — see [`agilent-laser-combiner.md`](agilent-laser-combiner.md).

## Evidence Identity

| Field | Value |
| --- | --- |
| Evidence class | Reconstructed host-side protocol description |
| Hardware validation | None |
| Coverage | Every wire opcode reachable from the known host command surface is listed below |

## Transport

| Parameter | Value |
| --- | --- |
| Transport | RS-232 style serial port (COM), opened as `\\.\COMn` |
| Baud | 115200 |
| Framing | 8 data bits, no parity, 1 stop bit |
| Flow control | none |
| Access | read/write, no sharing, existing port only |

No HID, WinUSB, FTDI, or proprietary kernel-driver transport is involved.
Whether the port is a physical UART or a USB CDC virtual COM port is not
determinable from the protocol — no VID/PID is ever enumerated.

## Frame Format

Requests are **binary**. Replies are **ASCII text**. This asymmetry holds for
every command.

```
host -> board :  <cmd:1 byte> [payload: 0..N raw bytes]        (no terminator)
board -> host :  <cmd echo:1 byte> <ASCII payload> CR LF
```

Transaction rules:

1. Write the 1-byte command; retry **once** on write failure.
2. If `payloadLen > 0`, write the payload bytes; retry **once** on failure.
3. For command `0x5B` (`SetSerial`), wait **400 ms** before reading.
4. Read CRLF-terminated lines, **discarding any line whose first byte is not the
   echoed command byte**, until one matches or a read error occurs.
5. The reply value is the matched line with the echoed command byte and CRLF
   stripped.

The reply-matching loop is the resynchronisation mechanism: stale or unsolicited
lines are silently dropped. All command bytes are `< 0x80`.

There is **no checksum, no length field, no address field, and no ACK/NAK**.
Reply payloads are plain decimal or float text. Reads run to the `"\r\n"`
terminator under a host timeout (a runtime port setting, not a protocol
constant); on expiry the buffer is discarded and error `10` returned.

### Numeric encodings

| Direction | Type | Encoding |
| --- | --- | --- |
| Host → board | 16-bit value | **big-endian**: `hi = v >> 8`, then `lo = v & 0xFF` |
| Host → board | `float` | raw IEEE-754 single, **little-endian** |
| Host → board | index/flag | single raw byte |
| Board → host | all | ASCII decimal / float text |

## Session Startup

1. **Port discovery**: format `"COM%d"` for **n = 1..256**, probe `\\.\COMn`,
   keep the ones that open.
2. **Port scan**: open each candidate at 115200 8N1 and send `0x03`. Accept the
   port if the reply equals the exact ASCII string **`"My100xBoard"`**. Two full
   passes over all candidates, then a 1500 ms wait, then a third pass. Nothing
   answering → error `13`.
3. **Board inventory**: `0x04` serial number, `0x01` model string, `0x36` laser
   line count (validated `0..8`, otherwise error `18`).
4. **Per-line inventory**, for each line `i`: `0x37`, `0x38`, `0x39`, `0x3A`,
   `0x3B` (payload `[i]`), then `0x3C` with payload `[i][k]` for `k = 0..10` to
   read the 11-point calibration curve.
5. **Model gate**: if the model string is exactly `"LUn8"` or `"LU-N4"`, open
   completes here and the remaining initialisation is skipped.
6. Otherwise: `0x0D` external control off, `0x28` state, `0x02` firmware version,
   then `0x32`, `0x33`, `0x30`, `0x31`, `0x2C`, `0x29`, `0x2A` to prime caches.
7. The firmware version string is compared against `"0.12"` and cached as a
   feature flag, so some behaviour is firmware-dependent.

`"LUn8"` and `"LU-N4"` are almost certainly the Nikon-badged 8-line and 4-line
laser units. Treat that as a naming observation, not a confirmed OEM claim.

## Command Table

Getter opcodes are **setter opcode + 0x1E** across the whole `0x0A`–`0x15` /
`0x28`–`0x33` block. `len` is the request payload length in bytes, excluding the
command byte.

| Cmd | len | Request payload | Reply | Meaning |
| --- | --- | --- | --- | --- |
| `0x01` | 0 | — | text | Model string (`"LUn8"`, `"LU-N4"`, …) |
| `0x02` | 0 | — | text | Firmware version |
| `0x03` | 0 | — | text | Identify; expected `"My100xBoard"` |
| `0x04` | 0 | — | text | Serial number |
| `0x05` | 0 | — | text | Hardware version |
| `0x0A` | 1 | `[stateMask]` | — | Set laser on/off bitmask, bit *i* = line *i* |
| `0x0B` | 3 | `[line][hi][lo]` | — | Set line power, 16-bit raw DAC counts |
| `0x0C` | 3 | `[chan][hi][lo]` | — | Set analog output, 16-bit raw DAC counts |
| `0x0D` | 1 | `[onoff]` | — | External (hardware) control enable |
| `0x0E` | 1 | `[onoff]` | — | Blanking enable |
| `0x0F` | 1 | `[value]` | — | Sync / trigger configuration |
| `0x10` | 1 | `[onoff]` | — | Shutter |
| `0x11` | 1 | `[value]` | — | Galvo position |
| `0x12` | 1 | `[state]` | — | ND filter state |
| `0x13` | 1 | `[mapping]` | — | ND filter mapping |
| `0x14` | 3 | `[line][hi][lo]` | — | Set direct laser amplitude |
| `0x15` | 0 | — | — | Save direct laser amplitude (persist) |
| `0x28` | 0 | — | int | Get laser on/off bitmask |
| `0x29` | 1 | `[line]` | text | Get line power |
| `0x2A` | 1 | `[chan]` | text | Get analog output |
| `0x2B` | 0 | — | int | Get external control |
| `0x2C` | 0 | — | int | Get blanking |
| `0x2D` | 0 | — | int | Get sync |
| `0x2E` | 0 | — | int | Get shutter |
| `0x2F` | 0 | — | int | Get galvo |
| `0x30` | 0 | — | int | Get ND state |
| `0x31` | 0 | — | int | Get ND mapping |
| `0x32` | 0 | — | int | Get direct laser amplitude |
| `0x33` | 0 | — | int | Get saved direct laser amplitude |
| `0x36` | 0 | — | int | Number of laser lines (0..8) |
| `0x37` | 1 | `[line]` | float | Line **minimum** output voltage |
| `0x38` | 1 | `[line]` | float | Line **maximum** output voltage |
| `0x39` | 1 | `[line]` | uint | Line DAC bit depth (validated 0..33) |
| `0x3A` | 1 | `[line]` | uint | Line **wavelength in nm** |
| `0x3B` | 1 | `[line]` | float | Line **maximum power in mW** |
| `0x3C` | 2 | `[line][k]` | float | Calibration coefficient *k* (k = 0..10) |
| `0x52` | 1 | `[present]` | — | Set ND-filter-present flag |
| `0x53` | 1 | `[present]` | — | Set galvo-present flag |
| `0x58` | 3 | `[line][nmHi][nmLo]` | — | Set line wavelength (nm) |
| `0x59` | 5 | `[line][f32 LE]` | — | Set line max power (mW) |
| `0x5A` | 6 | `[line][k][f32 LE]` | — | Set calibration coefficient *k* |
| `0x5B` | var | NUL-terminated serial string, capped at 64 bytes | — | Set serial number; host waits 400 ms after |
| `0x5C` | 2 | `[reg][value]` | — | Write hardware register |
| `0x5D` | 1 | `[reg]` | int | Read hardware register |
| `0x5E` | 3 | `[addrHi][addrLo][value]` | — | Write EEPROM byte |
| `0x5F` | 2 | `[addrHi][addrLo]` | int | Read EEPROM byte |
| `0x60` | 5 | `[chan][value31..24][value23..16][value15..8][value7..0]` | — | Write AOTF register (32-bit value, big-endian) |
| `0x61` | 1 | `[chan]` | uint | Read AOTF register |
| `0x62` | 1 | `[line]` | uint | Laser runtime counter (part 1) |
| `0x63` | 1 | `[line]` | uint | Laser runtime counter (part 2) |
| `0x64` | var | `[countHi][countLo][state0]…[stateN-1]` | — | Load state sequence |
| `0x65` | var | `[line][countHi][countLo][counts0Hi][counts0Lo]…` | — | Load power sequence |
| `0x66` | var | `[chan][countHi][countLo][counts0Hi][counts0Lo]…` | — | Load analog-output sequence |
| `0x67` | 1 | `[value]` | — | Start sequence |
| `0x68` | 0 | — | uint | Stop sequence |

`0x5B` sends `strlen(serial) + 1` bytes capped at 64, so the trailing NUL is
included when the input fits. In `0x64`–`0x66` the element count is a big-endian
`u16`; `0x65`/`0x66` convert host-side floats through the same voltage/power
conversion used by the live setters and store each resulting 16-bit DAC count
big-endian.

## Unit Semantics

The wire carries **raw DAC counts**; all engineering-unit conversion is
host-side.

**Counts → volts** (`bitDepth` from `0x39`, `minV` from `0x37`, `maxV` from
`0x38`):

```
maxCounts = (1 << bitDepth) - 1
volts     = counts / maxCounts * (maxV - minV) + minV
```

Line voltages are range-checked to `0.0 .. 10.0` V on ingest.

**Volts → milliwatts**, using the 11 coefficients from `0x3C` and `maxMW` from
`0x3B`:

```
if calibrated:
    step = (maxV - minV) / 10.0
    find i such that volts falls in the i-th of 10 evenly spaced intervals
    i == 0   -> 0 mW
    i == 11  -> maxMW
    else     -> linear interpolation between coeff[i-1] and coeff[i]
else:
    mW = volts / maxV * maxMW
```

So the 11 coefficients are a **piecewise-linear voltage→power calibration curve**
sampled at 11 evenly spaced voltages from `minV` to `maxV`.

Analog output is a separate table of at most **4 channels** (`minV`, `maxV`,
`bitDepth` each); channel index ≥ 4 is error `17`. The power setter validates its
line index against `nrLines + 4`, so laser lines and analog channels share one
index space there.

## Error Codes

Host-side status codes for this protocol layer (not values returned by the
board).

| Code | Meaning |
| --- | --- |
| `0` | Success |
| `6` | Port open failed |
| `7` | Serial parameter or timeout configuration failed |
| `8` | Serial write failed |
| `9` | Read called with zero-size buffer |
| `10` | Read timed out / read error |
| `13` | Board not found during open |
| `14` | Board not open |
| `15` | Port handle null inside transaction |
| `16` | Invalid laser line index |
| `17` | Invalid analog channel index (>= 4) |
| `18` | Board returned an out-of-range value |
| `19` | Invalid calibration coefficient index |
| `21` | Feature not present (ND / galvo) |

## What This Does Not Establish

- **No safety surface.** No interlock, emission-permitted, key-switch,
  over-temperature, or fault command exists in this protocol. A driver must not
  synthesise a safety state it cannot read.
- **No completion semantics.** Blocking request/reply only: no busy flag, no
  motion-complete, no asynchronous event channel. Whether the board finishes
  acting before it replies is unknown.
- **No timing data.** Inter-command spacing, reply latency, and the read timeout
  are unknown; the only fixed delays are 400 ms after `0x5B` and 1500 ms in the
  port scan.
- **No device identity.** No VID/PID or USB descriptor, so discovery cannot be
  made deterministic.
- **No board-side meaning** for register, EEPROM, AOTF, serial, and sequence
  commands: payload shape is known; accepted limits, persistence, and safety
  consequences are not.
- **No hardware confirmation of anything above** — host intent, not observed
  board behaviour.

## Suggested Bring-Up Order

1. Open at 115200 8N1, send `0x03`, confirm the reply is `03 "My100xBoard" CRLF`
   — validates framing, echo, and terminator in one shot.
2. `0x01`, `0x04`, `0x02`, `0x05`, `0x36` — read-only identity, no emission risk.
3. `0x3A`, `0x38`, `0x39`, `0x3B` per line — confirms the unit table.
4. `0x28` state readback with all lines off, before any write.
5. Only then, with interlocks verified out-of-band, a single `0x0A` write.

Record each step against
[`../devices/hardware-validation-template.md`](../devices/hardware-validation-template.md),
capturing the serial trace and the runtime output for the same window.
