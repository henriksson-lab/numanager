# Mad City Labs USB Protocol Specification

Covers MCL **MicroDrive** (stepper stages, MadTweezer, Motorized Micromirror
TIRF) and **NanoDrive** (piezo nanopositioners, C-Focus, waveform/sequence).

## Evidence

| Item | Value |
| --- | --- |
| Evidence class | Reverse-engineered protocol evidence, plus manufacturer documentation for the published `MCL_*` error enum |
| Hardware validation | **None.** No captured traffic from a physical device yet |
| Recorded as candidate wire facts | Transport, endpoint assignment, device identity, vendor-request codes, encoder wire format, error mapping |
| **Not validated** | Payload field semantics, units, scaling, completion/limit behavior. These need captured traffic from a physical device before any driver relies on them |

---

## 1. Transport

Plain USB. There is no kernel IOCTL layer and no vendor kernel-driver protocol —
both families speak USB vendor control transfers plus bulk endpoints directly.

Device setup is *open + claim interface* only: no `SET_CONFIGURATION`, no device
reset, no asynchronous transfers. All traffic is synchronous.

### 1.1 Control transfers

Standard 8-byte USB setup packet (`bmRequestType`, `bRequest`, little-endian
`wValue`/`wIndex`/`wLength`). Transfer length is `wLength`.

### 1.2 Bulk endpoints

| Selector | OUT | IN |
| --- | --- | --- |
| axis `N`, `N` = 1..5 | `0x0N` | `0x8N` |
| device-global | `0x02` | `0x86` |
| axis outside 1..5 | — | `MCL_INVALID_AXIS` (-7) |

The device-global pair shares the OUT endpoint with axis 2 but reads on a
dedicated IN endpoint `0x86`.

---

## 2. Device identity

### MicroDrive

Vendor ID **`0x1569`** (Mad City Labs) for every entry.

| Two-byte status (§4.2) | PIDs |
| --- | --- |
| yes | `0x2504`, `0x2506`, `0x2580`, `0x2581`, `0x2588` |
| no | `0x2500`, `0x2501`, `0x2503`, `0x2522`, `0x3500` |

### NanoDrive

| VID | PIDs |
| --- | --- |
| `0x1569` | `0x0001`, `0x1000`, `0x1020`, `0x1030`, `0x1230`, `0x1253`, `0x2000`, `0x2001`, `0x2003`, `0x2004`, `0x2053`, `0x2100`, `0x2201`, `0x2203`, `0x2253`, `0x2401`, `0x2601`, `0x3003` |
| `0x0547` | `0x8613` |
| `0x04B4` | `0x2235` |

The last two are Cypress EZ-USB defaults — an un-renumerated device that has not
yet loaded firmware. Such a device will not answer MCL vendor requests until
firmware load completes.

---

## 3. MicroDrive vendor requests

Control transfers. `bmRequestType` is `0xC0` (device-to-host, vendor, device)
for reads and `0x40` (host-to-device, vendor, device) for writes.
`wValue`/`wIndex` carry request arguments; `wLength` is the payload size.

| `bRequest` | Dir | `wLength` | Function |
| --- | --- | --- | --- |
| `0xC9` | IN | 1 | stop motion |
| `0xCA` | IN | 1 | reset all encoders |
| `0xCB` | IN | 1 | reset X encoder |
| `0xCC` | IN | 1 | reset Y encoder |
| `0xCD` | IN | 1 / 2 | status word (width per §4.2) |
| `0xCE` | **OUT** | — | move variable load |
| `0xCF` | IN | 1 | move status poll |
| `0xD0` | IN | 2 | three-axis move profile (also micro-steps variant) |
| `0xD1` | IN | varies | steps taken; `wLength` 41 (`0x29`) for MD6, 33 (`0x21`) for MadTweezer |
| `0xD2` | IN | 2 | move status / previous-move step count |
| `0xD3` | IN | 1 | reset Z encoder |
| `0xD4` | IN | — | MD3 start |
| `0xD5` | IN | 6 | axis assignments |
| `0xD7` | IN | 4 | wait time |
| `0xD8` | **OUT** | 8 | move parameters |
| `0xDA` | IN | 4 | temperature |
| `0xDC` | IN | 1 | set mode (M360) |
| `0xDD` | IN | 2 | get mode (M360) |
| `0xDE` | IN | 10 | rotation count (M360) |
| `0xDF` | IN | 1 | move until interrupt (M360) |
| `0xE7` | IN | 24 (`0x18`) | encoder read (§4.1) |
| `0xE8` | IN | 2 | MD8 encoder reset |
| `0xE9` | IN | 64 (`0x40`) | Motorized Micromirror TIRF get state, `wValue` low byte = 1 |
| `0xEA` | **OUT** | 4 | Motorized Micromirror TIRF set state, `wValue` low byte = 1 |

Generic short-command form: `bmRequestType 0xC0`, `wValue 0`, `wIndex 0`,
`wLength 1` (or 2 for the extended-response variant).

### Timeouts

| Timeout | Used for |
| --- | --- |
| **3000 ms** (`0xBB8`) | Actions and state changes: stop, encoder reset, status, move start, move variables/params, mode set, MMT set |
| **250 ms** (`0xFA`) | Fast informational reads: steps taken, assignments, wait time, temperature, get mode, rotations, encoder read, MMT get state |

---

## 4. MicroDrive data formats

### 4.1 Encoder values

The encoder payload is **24 bytes = 8 signed 24-bit little-endian counters**,
packed 3 bytes each with no padding:

```text
value[k] = (int32_t)( ((int8_t)buf[3k+2] << 16) | (buf[3k+1] << 8) | buf[3k] )
           for k = 0 .. 7            // 8 * 3 = 24 bytes
```

The high byte is sign-extended; each counter is a signed 24-bit quantity.

Two paths deliver the same 24-byte payload, selected by PID:

| PID | Mechanism |
| --- | --- |
| `0x2588` | control IN, `bRequest 0xE7`, `wLength 0x18`, 250 ms |
| all other MicroDrive PIDs | bulk IN on the global endpoint, into a **512-byte** buffer |

### 4.2 Status word

PIDs `0x2504`, `0x2506`, `0x2580`, `0x2581`, `0x2588` return a **2-byte** status
word; the remaining MicroDrive PIDs return **1 byte**. This selects `wLength` on
`bRequest 0xCD`.

The word carries **two bits per axis**: axis `N` occupies bits `2N-2` and
`2N-1`, so axes 1..5 occupy bits 0–9. Bits for axes the device does not have are
masked off (axis 1 → `0xFFFC`, 2 → `0xFFF3`, 3 → `0xFFCF`, 4 → `0xFF3F`,
5 → `0xFCFF`). The axis complement is per-model, driven by a per-device
axis-presence bitmask.

**The meaning of the two bits per axis is not validated.** Forward/reverse limit
switch is the obvious reading, but that is an inference and must be confirmed on
hardware before any driver treats a bit as a limit.

---

## 5. NanoDrive vendor requests

Same transport.

| `bRequest` | Dir | Function |
| --- | --- | --- |
| `0xB1` | OUT | change clock |
| `0xBA` | IN | get firmware version |
| `0xC2` | IN | get clock frequency |
| `0xC3` | OUT | set waveform frequency |
| `0xC5` | IN | measure temperature |
| `0xC9` | IN | encoder read buffer |
| `0xCD` | IN | C-Focus status |
| `0xCE` | OUT | C-Focus step |
| `0xCF` | IN | C-Focus lock state |
| `0xD0` | OUT | bind clock to axis |
| `0xD1` | OUT | configure polarity |
| `0xD2` | OUT | set clock |
| `0xD3` | OUT | reset defaults |
| `0xD4` | IN | ADC read axis (position read) |
| `0xD5` | IN | C-Focus set focus |
| `0xD6` | OUT | DDS scan increment |
| `0xD7` | OUT | multi-axis waveform setup |
| `0xD8` | IN | multi-axis waveform trigger |
| `0xD9` | IN | multi-axis waveform status |
| `0xDA` | IN | multi-axis waveform stop |
| `0xDB` | IN | multi-axis waveform debug status |
| `0xDC` | IN | get DAC position |
| `0xDD` | IN | sequence setup |
| `0xDE` | IN | sequence start |
| `0xDF` | IN | sequence stop |
| `0xE0` | OUT | sequence clear |
| `0xE1` | IN | get max sequence length |
| `0xE2` | IN | waveform trigger with user interrupt |

> The MicroDrive and NanoDrive request spaces **overlap numerically but are not
> compatible** — `0xD4` is "MD3 start" on MicroDrive and "ADC read axis" on
> NanoDrive. Dispatch must be keyed on PID first.

Bulk endpoints carry the movement, encoder, sequence, and waveform data paths —
per-axis position streaming, waveform upload, and sequence data ride the axis
bulk endpoints while configuration rides control transfers. DAC (position write)
and ADC (position read) are the primary movement primitives.

---

## 6. Error mapping

USB transfer outcomes map to the `MCL_*` result codes as follows:

| USB outcome | MCL result |
| --- | --- |
| success | `MCL_SUCCESS` (0), transferred byte count set |
| pipe/stall or timeout | `MCL_USAGE_ERROR` (-4) |
| no device, or I/O error | `MCL_DEV_NOT_ATTACHED` (-3) |
| anything else | `MCL_DEV_ERROR` (-2) |

Bulk transfers return `MCL_SUCCESS` (0), `MCL_DEV_ERROR` (-2) on any failure, or
`MCL_INVALID_AXIS` (-7) for an axis outside 1–5.

These land exactly on the manufacturer-published error enum:

```text
MCL_SUCCESS 0   MCL_GENERAL_ERROR -1   MCL_DEV_ERROR -2   MCL_DEV_NOT_ATTACHED -3
MCL_USAGE_ERROR -4   MCL_DEV_NOT_READY -5   MCL_ARGUMENT_ERROR -6
MCL_INVALID_AXIS -7  MCL_INVALID_HANDLE -8  MCL_INVALID_DRIVER -9
MCL_SEQ_NOT_VALID -10   MCL_BLOCKED_BY_TIRFLOCK -11
```

A **timeout is reported as `MCL_USAGE_ERROR`**, not as a distinct timeout code —
a driver cannot distinguish "device busy" from "bad request" by return code alone.

---

## 7. Session sequence

Enumerate → match `idVendor`/`idProduct` against §2 → open → claim interface →
read string descriptors for serial/product identity → traffic per §1 → release
interface and close. No `SET_CONFIGURATION` and no device reset are issued, so
the device is expected to be in its default configuration on open.

---

## 8. Implementation checklist for an SDK-free driver

Everything in §1–§7 is safe to implement from this document.

**Untested — needs captured traffic from a physical device**
(`docs/reverse/trace-capture-guide.md`) before typed motion/control status:

| Gap | What the trace must show |
| --- | --- |
| Status bit meaning | What the two bits per axis actually encode. Limit switches is an inference, not evidence. Do not gate motion on them until confirmed. |
| Move payload fields | `wValue`/`wIndex` field packing for `0xCE` move variables and the 8-byte `0xD8` move parameters, and the units of each field |
| Completion | Whether `0xCF` / `0xD2` polling is level- or edge-triggered, the poll cadence, and what a completed vs interrupted move looks like |
| Encoder counter mapping | What the 8 counters map to on a given axis count and device model |
| Encoder scaling | Counts-to-microns per model; no scaling constants are known |
| Homing / limits | Homing behavior and what the device reports at a hard limit |
| NanoDrive DAC/ADC | Sample encoding on the axis bulk endpoints, and calibration/range readback |
| Firmware load | Whether a device enumerating as `0547:8613` / `04B4:2235` needs a host-side firmware download before it answers vendor requests |

Until those exist, expose only the readback/action surface whose wire fields are
recorded above. Motion hardware can damage itself and its surroundings on a bad
command, so typed motion/control needs the limit and completion facts above.
