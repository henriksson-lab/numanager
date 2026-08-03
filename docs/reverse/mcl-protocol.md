# Mad City Labs USB Protocol Specification

Covers MCL **MicroDrive** (stepper stages, MadTweezer, Motorized Micromirror
TIRF) and **NanoDrive** (piezo nanopositioners, C-Focus, waveform/sequence).

## Evidence

| Item | Value |
| --- | --- |
| Status | Static protocol-evidence specification; no hardware validation yet |
| MicroDrive evidence | Reverse engineered |
| NanoDrive evidence | Reverse engineered |
| Header cross-check | Public error-code declarations only |
| Evidence class | Reverse engineered; implementation must use only the wire-level facts recorded here |
| Independent cross-check | The wire-layer error mapping matches the vendor's published `MCL_*` error enum (see §6) |
| Validation boundary | Transport, endpoint assignment, device identity, vendor-request codes, and the encoder wire format are recorded as candidate wire facts. **Payload field semantics, units, scaling, and completion/limit behavior are not validated** and need a hardware trace. |

Function names below identify protocol roles, not public APIs.

---

## 1. Transport

MCL talks to both product families over **libusb 1.0.21**, statically linked
into the observed implementation. There is no kernel IOCTL layer and no vendor kernel driver
protocol to reverse — the wire format is plain USB.

The entire libusb surface used by MCL code is:

```text
libusb_init            libusb_get_device_list      libusb_get_device
libusb_get_device_descriptor                       libusb_open
libusb_claim_interface  libusb_release_interface   libusb_close
libusb_control_transfer libusb_bulk_transfer
libusb_free_device_list libusb_exit
```

Notably absent: `libusb_set_configuration`, `libusb_reset_device`, and the
entire async API. Device setup is therefore just *open + claim interface*, and
all traffic is synchronous.

Exactly two internal routines touch the wire:

### 1.1 `RWControlPipe` — control transfers

```c
int RWControlPipe(libusb_device_handle *h, unsigned char *const setup,
                  void *data, unsigned int len,
                  unsigned long *transferred, bool unused, unsigned int timeout);
```

`setup` is a **raw 8-byte USB setup packet**, unpacked field-by-field and
forwarded through libusb:

| Setup byte | Field | Passed to `libusb_control_transfer` as |
| --- | --- | --- |
| `setup[0]` | `bmRequestType` | arg 2 |
| `setup[1]` | `bRequest` | arg 3 |
| `setup[2..3]` | `wValue`, little-endian | arg 4 |
| `setup[4..5]` | `wIndex`, little-endian | arg 5 |
| `setup[6..7]` | `wLength`, little-endian | arg 7 |

The `len` argument is **ignored** — the transfer length comes from the setup
packet's own `wLength`. On success `*transferred` receives the byte count and
the function returns 0.

### 1.2 `RWNAxisPipe` — bulk transfers

```c
int RWNAxisPipe(libusb_device_handle *h, bool global, void *data,
                unsigned int len, unsigned long *transferred,
                int axis, bool in, unsigned int timeout);
```

Endpoint selection:

| Selector | OUT endpoint (`in == false`) | IN endpoint (`in == true`) |
| --- | --- | --- |
| `global == true` | `0x02` | `0x86` |
| `axis == 1` | `0x01` | `0x81` |
| `axis == 2` | `0x02` | `0x82` |
| `axis == 3` | `0x03` | `0x83` |
| `axis == 4` | `0x04` | `0x84` |
| `axis == 5` | `0x05` | `0x85` |
| any other axis | — | returns `MCL_INVALID_AXIS` (-7) |

So each axis owns a matched bulk endpoint pair `0x0N` / `0x8N`, and there is a
device-global pair that shares the OUT endpoint with axis 2 but reads on a
dedicated IN endpoint `0x86`.

Forwarded to `libusb_bulk_transfer(h, endpoint, data, len, &transferred, timeout)`.
Returns 0 on success, `MCL_DEV_ERROR` (-2) on any libusb failure.

---

## 2. Device identity

Each family records a flat device table of 4-byte entries:

```c
struct DeviceTable { uint16_t vid; uint16_t pid; };   // 4 bytes
```

`DeviceTableValidPid(pid)` scans from `supportedTable + 2` in stride 4 — i.e.
the **second** `uint16` of each entry — confirming the field order above.

### MicroDrive — `DeviceTableSize() == 10`

Vendor ID **`0x1569`** (Mad City Labs) for every entry.

| PID | Two-byte status? (§4.2) |
| --- | --- |
| `0x2500` | no |
| `0x2501` | no |
| `0x2503` | no |
| `0x2504` | **yes** |
| `0x2506` | **yes** |
| `0x2522` | no |
| `0x2580` | **yes** |
| `0x2581` | **yes** |
| `0x2588` | **yes** |
| `0x3500` | no |

### NanoDrive — `DeviceTableSize() == 20`

| VID | PIDs |
| --- | --- |
| `0x1569` | `0x0001`, `0x1000`, `0x1020`, `0x1030`, `0x1230`, `0x1253`, `0x2000`, `0x2001`, `0x2003`, `0x2004`, `0x2053`, `0x2100`, `0x2201`, `0x2203`, `0x2253`, `0x2401`, `0x2601`, `0x3003` |
| `0x0547` | `0x8613` |
| `0x04B4` | `0x2235` |

The last two are Cypress EZ-USB defaults — an un-renumerated device that has not
yet loaded firmware. A driver that enumerates them must expect a device which
does not answer MCL vendor requests until firmware load completes.

---

## 3. MicroDrive vendor requests

All are control transfers. `bmRequestType` is `0xC0` (device-to-host, vendor,
device) for reads and `0x40` (host-to-device, vendor, device) for writes.
`wValue`/`wIndex` carry the request arguments; `wLength` is the payload size.

Recovered from internal request wrappers, each of which builds the 8-byte setup
packet inline.

| `bRequest` | Dir | `wLength` | Wrapper | Public API it backs |
| --- | --- | --- | --- | --- |
| `0xC9` | IN | 1 | `VR_MDCommand` | `MCL_MicroDriveStop`, `MCL_MDStop` |
| `0xCA` | IN | 1 | `VR_MDCommand` | `MCL_MicroDriveResetEncoders` |
| `0xCB` | IN | 1 | `VR_MDCommand` | `MCL_MicroDriveResetXEncoder` |
| `0xCC` | IN | 1 | `VR_MDCommand` | `MCL_MicroDriveResetYEncoder`, `MCL_MD1ResetEncoder` |
| `0xCD` | IN | 1 / 2 | `VR_MDCommand`, `VR_MicroDriveCommand` | `MCL_MicroDriveStatus`, `MCL_MDStatus` |
| `0xCE` | **OUT** | — | `VR_MicroDriveMoveVariables` | move variable load |
| `0xCF` | IN | 1 | `VR_MicroDrive8MoveStatus` | `CheckMovementStatus`, MoveProfile poll |
| `0xD0` | IN | 2 | `VR_MicroDriveCommand` | `MCL_MicroDriveMoveProfileXYZ`, `MCL_MicroDriveMoveProfileXYZ_MicroSteps`, `MDMoveThreeAxes` |
| `0xD1` | IN | varies | `VR_MicroDriveStepsTaken` | steps taken; `wLength` 41 (`0x29`) for MD6, 33 (`0x21`) for MadTweezer |
| `0xD2` | IN | 2 | `VR_MicroDriveCommand` | `MCL_MicroDriveMoveStatus`, `CountPreviousMoveProfileSteps` |
| `0xD3` | IN | 1 | `VR_MDCommand` | `MCL_MicroDriveResetZEncoder` |
| `0xD4` | IN | — | `VR_MD3Start`, `VR_MD3StartPic32` | MD3 start |
| `0xD5` | IN | 6 | `VR_MicroDriveGetAssignments` | axis assignments |
| `0xD7` | IN | 4 | `VR_GetWaitTime` | wait time |
| `0xD8` | **OUT** | 8 | `VR_MicroDriveMoveParams` | move parameters |
| `0xDA` | IN | 4 | `VR_GetTemperature` | temperature |
| `0xDC` | IN | 1 | `VR_SetMode` | `SetMode` (M360) |
| `0xDD` | IN | 2 | `VR_GetMode` | `VR_GetMode` (M360) |
| `0xDE` | IN | 10 | `VR_GetRotations` | rotation count (M360) |
| `0xDF` | IN | 1 | — | `MoveUntilInterrupt` (M360) |
| `0xE7` | IN | 24 (`0x18`) | `VendorRequestEncoderRead` | encoder read (§4.1) |
| `0xE8` | IN | 2 | `VR_MD8ResetEncoder` | MD8 encoder reset |
| `0xE9` | IN | 64 (`0x40`) | `VR_MMTGetState` | Motorized Micromirror TIRF state, `wValue` low byte = 1 |
| `0xEA` | **OUT** | 4 | `VR_MMTSetState` | Motorized Micromirror TIRF set state, `wValue` low byte = 1 |

`VR_MDCommand` / `VR_MicroDriveCommand` / `VR_MDCommandMD6ExtendedResponse` are
generic wrappers whose `bRequest` is a runtime argument; the table above lists
the constants their callers actually pass. The generic form is:

```text
bmRequestType = 0xC0
bRequest      = <command byte>
wValue        = 0
wIndex        = 0
wLength       = 1  (VR_MDCommand)  |  2  (VR_MDCommandMD6ExtendedResponse)
```

### Timeouts

Two values appear throughout, chosen per request:

| Timeout | Used for |
| --- | --- |
| **3000 ms** (`0xBB8`) | Actions and state changes: stop, encoder reset, status, move start, move variables/params, mode set, MMT set |
| **250 ms** (`0xFA`) | Fast informational reads: steps taken, assignments, wait time, temperature, get mode, rotations, encoder read, MMT get state |

---

## 4. MicroDrive data formats

### 4.1 Encoder values

`BufferToEncoderVals(MicroDrive*, unsigned char *buf, int len, EncoderValues *out)`
requires `len >= 24` and decodes **8 signed 24-bit little-endian counters**,
packed 3 bytes each with no padding:

```text
value[k] = (int32_t)( ((int8_t)buf[3k+2] << 16) | (buf[3k+1] << 8) | buf[3k] )
           for k = 0 .. 7            // 8 * 3 = 24 bytes
```

The high byte is sign-extended (`movsx`), the lower two are zero-extended
(`movzx`), so each counter is a signed 24-bit quantity in a 32-bit result.

Two paths deliver the same 24-byte payload:

| Path | Static-library selection | Mechanism |
| --- | --- | --- |
| `VendorRequestEncoderRead` | PID `0x2588` | control IN, `bRequest 0xE7`, `wLength 0x18`, 250 ms |
| `BulkEndpointEncoderRead` | all other MicroDrive PIDs | bulk IN on the global endpoint, into a **512-byte** buffer |

`MicroDriveReadEncoders` compares the device PID at object offset `+0x1C` with
`0x2588` and calls `VendorRequestEncoderRead` only on equality; the not-equal
branch calls `BulkEndpointEncoderRead`.

### 4.2 Status word

`HasTwoByteStatus(pid)` returns true for PIDs `0x2504`, `0x2506`, `0x2580`,
`0x2581`, `0x2588`, and false for the rest. This selects a 1-byte versus 2-byte
status response — i.e. `wLength` on `bRequest 0xCD` and the `VR_MDCommand` vs
`VR_MicroDriveCommand` wrapper.

`RemoveInvalidAxesFromStatus(MicroDrive*, uint16_t status)` masks the word with
**two bits per axis**:

| Axis | Status bits | Mask applied when axis absent |
| --- | --- | --- |
| 1 | 0–1 | `0xFFFC` |
| 2 | 2–3 | `0xFFF3` |
| 3 | 4–5 | `0xFFCF` |
| 4 | 6–7 | `0xFF3F` |
| 5 | 8–9 | `0xFCFF` |

Which axes exist is read from a per-device bitmask (bits 1, 2, 4, 8 tested), and
the whole routine dispatches through a jump table on PID (`pid - 0x2501`, range
check `> 0x87`), so the axis complement is per-model.

**The meaning of the two bits per axis is not recovered.** Forward/reverse limit
switch is the obvious reading, but that is an inference and must be confirmed on
hardware before any driver treats a bit as a limit.

### 4.3 Driver-state layout

Observed driver-state fields used by the wire layer:

| Offset | Contents |
| --- | --- |
| `+0x08` | `libusb_device_handle *` |
| `+0x1C` | `uint16_t` USB PID |
| `+0x10C` | flags byte; bit `0x10` tested in `BulkEndpointEncoderRead` |
| `+0x3D2` | per-axis presence bitmask |

---

## 5. NanoDrive vendor requests

Same transport. Recovered from internal request wrappers across movement,
waveform, sequence, focus, clock, encoder, information, and temperature paths.

| `bRequest` | Dir | Wrapper / API |
| --- | --- | --- |
| `0xB1` | OUT | `VR_ChangeClock` |
| `0xBA` | IN | `NanoDrive::VR_GetFWVersion` |
| `0xC2` | IN | `VR_GetClockFreq` |
| `0xC3` | OUT | `VR_SetWFFreq` (waveform frequency) |
| `0xC5` | IN | `VR_MeasureTemp` |
| `0xC9` | IN | `EncoderReadBuffer` |
| `0xCD` | IN | `VR_CFocusStatus` |
| `0xCE` | OUT | `VR_CFocusStep` |
| `0xCF` | IN | `VR_CFocusIsFocusLocked` |
| `0xD0` | OUT | `IssBindClockToAxis` |
| `0xD1` | OUT | `IssConfigurePolarity` |
| `0xD2` | OUT | `IssSetClock` |
| `0xD3` | OUT | `IssResetDefaults` |
| `0xD4` | IN | `AdcReadAxis` (position read) |
| `0xD5` | IN | `VR_CFocusSetFocus` |
| `0xD6` | OUT | `MCL_DdsScanIncrement` |
| `0xD7` | OUT | `VR_WFMA_Setup` (multi-axis waveform) |
| `0xD8` | IN | `VR_WFMA_Trigger` |
| `0xD9` | IN | `VR_WFMA_Status` |
| `0xDA` | IN | `VR_WFMA_Stop` |
| `0xDB` | IN | `VR_WFMA_DebugStatus` |
| `0xDC` | IN | `VR_GetDacPosition` |
| `0xDD` | IN | `VR_SequenceSetup` |
| `0xDE` | IN | `VR_SequenceStart` |
| `0xDF` | IN | `VR_SequenceStop` |
| `0xE0` | OUT | `MCL_SequenceClear` |
| `0xE1` | IN | `NanoDrive::VR_GetMaxSequence` |
| `0xE2` | IN | `VR_WFMA_Trigger_UserInt` |

> The MicroDrive and NanoDrive request spaces **overlap numerically but are not
> compatible** — `0xD4` is "MD3 start" on MicroDrive and "ADC read axis" on
> NanoDrive. Dispatch must be keyed on PID first.

Bulk (`RWNAxisPipe`) is used by movement, encoder, sequence, and waveform paths
-- i.e. per-axis position
streaming, waveform upload, and sequence data ride the axis bulk endpoints while
configuration rides control transfers. `DAC()` (position write) and `ADC()`
(position read) are the primary movement primitives.

---

## 6. Error mapping

`RWControlPipe` translates libusb return codes:

| libusb result | MCL result |
| --- | --- |
| `>= 0` | `MCL_SUCCESS` (0), `*transferred` set |
| `LIBUSB_ERROR_PIPE` (-9), `LIBUSB_ERROR_TIMEOUT` (-7) | `MCL_USAGE_ERROR` (-4) |
| `LIBUSB_ERROR_NO_DEVICE` (-4), `LIBUSB_ERROR_IO` (-1) | `MCL_DEV_NOT_ATTACHED` (-3) |
| anything else | `MCL_DEV_ERROR` (-2) |

`RWNAxisPipe` returns `MCL_SUCCESS` (0) on success, `MCL_DEV_ERROR` (-2) on any
libusb failure, `MCL_INVALID_AXIS` (-7) for an axis outside 1–5.

These land exactly on the vendor's published error enum:

```text
MCL_SUCCESS 0   MCL_GENERAL_ERROR -1   MCL_DEV_ERROR -2   MCL_DEV_NOT_ATTACHED -3
MCL_USAGE_ERROR -4   MCL_DEV_NOT_READY -5   MCL_ARGUMENT_ERROR -6
MCL_INVALID_AXIS -7  MCL_INVALID_HANDLE -8  MCL_INVALID_DRIVER -9
MCL_SEQ_NOT_VALID -10   MCL_BLOCKED_BY_TIRFLOCK -11
```

Note that a **timeout is reported as `MCL_USAGE_ERROR`**, not as a distinct
timeout code — a driver cannot distinguish "device busy" from "bad request" by
return code alone.

---

## 7. Session sequence

From the recorded startup path and libusb surface:

1. `libusb_init`
2. `libusb_get_device_list`
3. For each device: `libusb_get_device_descriptor`, match `idVendor`/`idProduct`
   against `supportedTable` (§2)
4. `libusb_open`
5. `libusb_claim_interface`
6. Device string descriptors are read for serial/product identification
7. Traffic per §1
8. Teardown: `libusb_release_interface`, `libusb_close`,
   `libusb_free_device_list`, `libusb_exit`

No `SET_CONFIGURATION` and no device reset are issued, so the device is expected
to be in its default configuration on open.

---

## 8. Implementation checklist for an SDK-free driver

Recovered and safe to implement from this document:

- Full USB transport: control setup-packet layout, per-axis bulk endpoint map,
  timeouts, and the libusb-to-`MCL_*` error mapping.
- Device identification: exact VID/PID tables for both families, including the
  Cypress pre-firmware IDs.
- Vendor request numbers and directions for both families, tied to the public
  API each one backs.
- Encoder wire format (8 × signed 24-bit LE) and status-word axis bit layout.
- Open/claim/close sequence.

Needs a hardware trace before typed motion/control status
(`docs/reverse/trace-capture-guide.md`):

| Gap | What the trace must show |
| --- | --- |
| Status bit meaning | What the two bits per axis actually encode. Limit switches is an inference, not evidence. Do not gate motion on them until confirmed. |
| Move payload fields | `wValue`/`wIndex` field packing for `0xCE` move variables and the 8-byte `0xD8` move parameters, and the units of each field |
| Completion | Whether `0xCF` / `0xD2` polling is level- or edge-triggered, the poll cadence, and what a completed vs interrupted move looks like |
| Encoder counter mapping | What the 8 counters map to on a given axis count and device model |
| Encoder scaling | Counts-to-microns per model; none of the scaling constants have been located yet |
| Homing / limits | `FindHome` behavior and what the device reports at a hard limit |
| NanoDrive DAC/ADC | Sample encoding on the axis bulk endpoints, and calibration/range readback |
| Firmware load | Whether a device enumerating as `0547:8613` / `04B4:2235` needs a host-side firmware download before it answers vendor requests |

Until those exist, expose only the readback/action surface whose wire fields
are recorded above. Motion hardware can damage itself and its surroundings on a
bad command, so typed motion/control needs the limit and completion facts above.
