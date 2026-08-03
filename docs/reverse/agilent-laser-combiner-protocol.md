# Agilent Laser Combiner Wire Protocol

Recorded from reverse engineered evidence and cross-checked
against the Micro-Manager adapter's public call sites.

This page documents the **host-side protocol grammar**. It is not a hardware
validation note. No command here has been observed against a real combiner, and
nothing in it authorises adding a runtime driver — see
[`agilent-laser-combiner.md`](agilent-laser-combiner.md) for the unblock gate.

## Evidence Identity

| Field | Value |
| --- | --- |
| Evidence | Reverse engineered |
| Type | Reverse engineered |
| Build timestamp | 2012-07-14 18:06:48 UTC |
| Reported SDK version | `"0.3 64Bit"` (`LaserBoardDriverVersion`) |
| Exports | 61 `LaserBoard*` functions |
| Command-call coverage | All wire commands listed below are present in the external analysis note |

The checked transaction calls cover every wire command listed below. The
remaining exported functions are local/runtime wrappers, cached metadata
accessors, or unit-conversion helpers that do not introduce another wire opcode.

## Transport

The vendor implementation uses the Win32 serial API. There is no evidence of
HID, WinUSB, FTDI, or vendor kernel-driver transport in the recorded command
path.

| Parameter | Value | Evidence class |
| --- | --- | --- |
| Transport | RS-232 style serial port (COM), opened as `\\.\COMn` | reverse engineered |
| Baud | 115200 | reverse engineered |
| Data bits | 8 | reverse engineered |
| Parity | none | reverse engineered |
| Stop bits | 1 | reverse engineered |
| Flow control | none set beyond platform defaults | reverse engineered |
| Access | read/write, no sharing, existing port only | reverse engineered |

Whether the port is a physical UART or a USB CDC virtual COM port is not
determinable from the vendor implementation — it never enumerates VID/PID. That distinction has
to come from hardware.

## Frame Format

Requests are **binary**. Replies are **ASCII text**. This asymmetry is
consistent across every command.

```
host -> board :  <cmd:1 byte> [payload: 0..N raw bytes]        (no terminator)
board -> host :  <cmd echo:1 byte> <ASCII payload> CR LF
```

The common transaction path does this:

1. If the port handle is null, return error `15`.
2. Write the 1-byte command. On write failure, retry **once**.
3. If `payloadLen > 0`, write the payload bytes. On failure, retry **once**.
4. If the command byte is `0x5B` (`SetSerial`), `Sleep(400 ms)` before reading.
5. Read CRLF-terminated lines into a 64-byte buffer, **discarding any line whose
   first byte is not the echoed command byte**, until one matches or a read
   error occurs.
6. Return `line.substr(1)` — the reply with the echoed command byte stripped and
   CRLF already removed.

The reply-matching loop is the resynchronisation mechanism: stale or unsolicited
lines are silently dropped. All command bytes are `< 0x80`, which matters
because the echoed command is compared as a signed byte in the vendor path.

There is **no checksum, no length field, no address field, and no ACK/NAK**.
Reply payloads are parsed with `std::istringstream` and `operator>>` into
`int`, `unsigned`, or `float` depending on the command, so they are plain
decimal (or float) text.

### Reading

The serial reader consumes **one byte at a time**, appending until the buffer
contains the terminator `"\r\n"`, then truncates at it. A software timer bounds
the loop; on expiry the buffer is emptied and error `10` is returned. The
timeout value is configured in the port object and is not recorded as a
compile-time protocol constant.

### Numeric encodings

| Direction | Type | Encoding |
| --- | --- | --- |
| Host → board | 16-bit value | **big-endian**: `hi = v >> 8`, then `lo = v & 0xFF` |
| Host → board | `float` | raw IEEE-754 single, **little-endian** |
| Host → board | index/flag | single raw byte |
| Board → host | all | ASCII decimal / float text |

## Session Startup

`LaserBoardOpen` runs this sequence:

1. **Port discovery**: build a candidate list by formatting `"COM%d"` for
   **n = 1..256** and probing `\\.\COMn`; keep the ones that open.
2. **Port scan**: for each candidate, open at 115200 8N1 and send
   command `0x03`. Accept the port if the reply equals the exact ASCII string
   **`"My100xBoard"`**. Two full passes over all candidate ports, then `Sleep(1500 ms)`, then a third
   pass. If nothing answers, `LaserBoardOpen` returns `13`.
3. **Board inventory**: `0x04` → serial number string,
   `0x01` → model string, `0x36` → laser line count (validated `0..8`,
   otherwise error `18`).
4. **Per-line inventory**, for each line `i`: `0x37`, `0x38`, `0x39`, `0x3A`,
   `0x3B` (each with payload `[i]`), then `0x3C` with payload `[i][k]` for
   `k = 0..10` to read the 11-point calibration curve.
5. **Model gate**: if the model string is exactly `"LUn8"` or `"LU-N4"`, open
   returns success here and the remaining initialisation is skipped. Any other
   model continues to step 6.
6. `0x0D` external control off, `0x28` state, `0x02` firmware version,
   then `0x32`, `0x33`, `0x30`, `0x31`, `0x2C`, `0x29`, `0x2A` to prime caches.
7. The firmware version string is compared against `"0.12"` and the result
   cached as a feature flag, so some behaviour is firmware-dependent.

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
| `0x5B` | var | NUL-terminated serial string, capped at 64 bytes | — | Set serial number; host sleeps 400 ms after |
| `0x5C` | 2 | `[reg][value]` | — | Write hardware register |
| `0x5D` | 1 | `[reg]` | int | Read hardware register |
| `0x5E` | 3 | `[addrHi][addrLo][value]` | — | Write EEPROM byte |
| `0x5F` | 2 | `[addrHi][addrLo]` | int | Read EEPROM byte |
| `0x60` | 5 | `[chan][value31..24][value23..16][value15..8][value7..0]` | — | Write AOTF register |
| `0x61` | 1 | `[chan]` | uint | Read AOTF register |
| `0x62` | 1 | `[line]` | uint | Laser runtime counter (part 1) |
| `0x63` | 1 | `[line]` | uint | Laser runtime counter (part 2) |
| `0x64` | var | `[countHi][countLo][state0]…[stateN-1]` | — | Load state sequence |
| `0x65` | var | `[line][countHi][countLo][counts0Hi][counts0Lo]…` | — | Load power sequence |
| `0x66` | var | `[chan][countHi][countLo][counts0Hi][counts0Lo]…` | — | Load analog-output sequence |
| `0x67` | 1 | `[value]` | — | Start sequence |
| `0x68` | 0 | — | uint | Stop sequence |

The fixed-width command payloads above match the external transaction evidence.
`0x60` stores the 32-bit AOTF register value big-endian. `0x5B` computes
`strlen(serial) + 1`, caps the result at 64 bytes, and therefore sends the
trailing NUL when the input fits.

The `0x64`–`0x66` sequence commands build variable-length heap payloads. `0x64`
stores the count as big-endian `u16` followed by caller-supplied state bytes.
`0x65` and `0x66` prepend the line/channel and big-endian `u16` count, convert
each caller-supplied float through the same host-side voltage/power conversion
used by the live setters, then store each resulting 16-bit DAC count big-endian.

## Unit Semantics

This is what the reverse note previously listed as the open question — "whether
values are optical power, relative percent, DAC counts, or analog level". The
answer is **raw DAC counts on the wire**, with the SDK doing the conversion
host-side.

**Counts → volts**:

```
maxCounts = (1 << bitDepth) - 1                  # bitDepth from cmd 0x39
volts     = counts / maxCounts * (maxV - minV) + minV
```

where `minV` comes from `0x37` and `maxV` from `0x38`. Line voltages are
range-checked to `0.0 .. 10.0` V on ingest.

**Volts → milliwatts**, using the 11 coefficients from `0x3C`:

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

So the 11 coefficients are a **piecewise-linear voltage→power calibration
curve** sampled at 11 evenly spaced voltages from `minV` to `maxV`, and `maxMW`
comes from `0x3B`.

`LaserBoardGetAOInfo` exposes a separate analog-output table of at most **4
channels** (`minV`, `maxV`, `bitDepth` each); channel index ≥ 4 returns error
`17`. Note that `LaserBoardSetPower` validates its line index against
`nrLines + 4`, so laser lines and analog channels share one index space in that
call.

## Error Codes

Recovered from the SDK's return paths. These are the values the Micro-Manager
adapter propagates as MM error codes.

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

## What This Does Not Recover

Stated explicitly so the gate is not mistakenly treated as satisfied:

- **No safety surface.** There is no interlock, emission-permitted, key-switch,
  over-temperature, or fault command anywhere in the 61-export surface. Either
  the board does not expose one over this protocol, or the SDK does not use it.
  A driver must not synthesise a safety state it cannot read.
- **No completion semantics.** Every command is a blocking request/reply with no
  busy flag, no motion-complete, and no asynchronous event channel. Whether the
  board finishes acting before it replies is unknown.
- **No timing evidence.** Inter-command spacing, worst-case reply latency, and
  the actual read timeout are not fixed constants in the analysed path. The only
  hard-coded delays are the 400 ms after `0x5B` and the 1500 ms in the port scan.
- **No device identity.** No VID/PID, USB descriptor, or serial-adapter identity,
  so discovery cannot be made deterministic from this artifact alone.
- **No hardware confirmation of low-level side effects.** External evidence records the
  host-side byte order and payload shape for register, EEPROM, AOTF, serial, and
  sequence commands, but not the board-side meaning of registers, accepted
  sequence limits, persistence behaviour, or safety consequences.
- **No hardware confirmation of anything above.** Everything here is the host's
  intent, not the board's observed behaviour.

## Suggested Bring-Up Order

If a board becomes available, the cheapest sequence that converts this into
hardware evidence:

1. Open at 115200 8N1, send `0x03`, confirm the reply is `03 "My100xBoard" CRLF`.
   This alone validates framing, echo, and terminator in one shot.
2. `0x01`, `0x04`, `0x02`, `0x05`, `0x36` — read-only identity, no emission risk.
3. `0x3A`, `0x38`, `0x39`, `0x3B` per line — confirms the unit table.
4. `0x28` state readback with all lines off, before any write.
5. Only then, with interlocks verified out-of-band, a single `0x0A` write.

Record each step against
[`../devices/hardware-validation-template.md`](../devices/hardware-validation-template.md)
and capture both the serial trace and the runtime output for the same window.
