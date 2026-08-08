# Lumenera Lu130 (Bio-Rad Gel Doc EZ) — hardware validation note, 2026-08-05

Follows [`hardware-validation-template.md`](hardware-validation-template.md).

**Superseded in part by the 2026-08-06 run recorded at the end of this note:
`CameraCapture` is now hardware-validated.** The 2026-08-05 session below failed
— zero image bytes — and is kept because the failure is what identified the two
missing stages. Read it as the diagnosis and the 2026-08-06 section as the
result.

The 2026-08-05 run promotes **firmware initialization** and the
**capability-register readback**.

Evidence is stated by class. Raw captures and analysis records are kept outside
this repository, per [`../reverse/README.md`](../reverse/README.md).

## Run Identity

| Field | Value |
| --- | --- |
| Driver module | `numanager_drivers::lumenera` |
| Device page | [`lumenera.md`](lumenera.md) |
| Hardware model | Bio-Rad Gel Doc EZ imager, Lumenera Lu130 camera (Sony ICX205, 12-bit mono) |
| Serial number or asset tag | USB serial `19020090` (imaging stage) |
| Firmware/software version | EZ-USB image `lumenera_fw_img01.hex`, selected by `bcdDevice = 0x0001` |
| Transport | USB userspace via `nusb`; imaging stage bound to WinUSB |
| Host OS and relevant driver stack | Windows 11 Home 10.0.26200. Imaging node `USB\VID_5354&PID_009A\19020090` bound to WinUSB. The loader node was bound to a pre-existing third-party kernel driver on the port used, which performed the firmware download itself; numanager's own initialization path was therefore not exercised in this run |
| Date | 2026-08-05 |
| Operator | Johan Henriksson |
| Config file or discovery record | `LumeneraDiscovery::os_usb(DriverId(4200)).with_firmware_initialization(None)`, via `numanager-examples --features os-usb gel_doc capture 100` |

## Evidence Sources

| Source class | Reference | Covered behavior |
| --- | --- | --- |
| Captured traffic from a physical device | Cold-power-on trace, external capture store, 2026-08-05 | Loader enumeration at `5354:809A` with `bcdDevice = 0x0001`, the complete EZ-USB anchor download, renumeration, and the imaging stage's configuration descriptor |
| Captured traffic from a physical device | Capture of this bench run, external capture store, 2026-08-05 | numanager's control stream and image-endpoint behavior |
| Captured traffic from a physical device | Reference acquisition trace, external capture store, 2026-08-05 | A working acquisition on the same unit, used as the comparison baseline |
| Documented bench run | This note; runtime output quoted below | `gel_doc capture 100` result as reported to the caller |

## Setup And Safety

| Area | Observed or enforced behavior |
| --- | --- |
| Motion limits and homing state | Not applicable — no motion axes on this device |
| Laser/light output limits and interlocks | The driver issues no illumination commands. The cabinet lamp is a separate USB device this driver never opens |
| Temperature, pressure, gas, or voltage limits | Not applicable |
| Emergency stop or safe shutdown | Capture teardown runs on the failure path: acquisition stop, the post-stop FPGA write, tap-register restore, and return to alternate setting 0. Confirmed present in the trace after the failed read |
| Fault injection or recovery tested | Read timeout exercised for real (the capture produced no data). Driver returned a typed `Transport` error and left the interface idle rather than hanging or leaving the stream armed |

## Commands And Properties

| Capability/property | Request or setpoint | Evidence expectation | Runtime command output/event | Hardware output/readback | Result | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| Firmware initialization | Anchor download of `img01` | Loader renumerates to the imaging PID; uploaded bytes match the shipped image | Camera present as `5354:009A`, `Lu130`, serial `19020090` | `0xA0 wValue=0xE600 data=01`, **618** `0xA0` record writes, `0xA0 wValue=0xE600 data=00`. All 618 records identical in content and order to `lumenera_fw_img01.hex` | **Pass** | Download performed by the pre-existing third-party driver on this port. numanager's upload path uses the same encoding but was not itself on the wire in this session |
| Firmware image selection | `bcdDevice` -> SDK image table | Selector 1 picks `img01` | `firmware_image = "lumenera_fw_img01.hex"` | Device descriptor `bcdDevice = 0x0001` | **Pass** | The SDK table contains selectors `0x0000`, `0x0001`, `0x0010`, and `0x0018`; unknown values fail closed |
| Capability readback | 11 read-only `bRequest 0x12` IN transfers at open | Device reports its own geometry | Values reported in the capture error string | `0x1000 = 0x04100570` (1392 x 1040), `0x100c` same, `0x1014 = 0x0c`, `0x101c = 0x04`, `0x1040 = 0x53290299`, `0x0004 = 0x019c = 1`, `0x0280 = 0x0284 = 0x1008 = 0`, `0x1004 = 0x00080004` | **Pass** | Geometry confirmed from the device and agrees with `width`/`height`. `0x1000` is the dimension register; `0x1004` is not. **Correction:** this run read `0x1014 = 0x0c` and recorded it as "12 bpp"; the same camera later read `0x05`, so `0x1014` is a device state code and the bit-depth reading was a coincidence |
| `exposure` (write) | 100 ms | Programmed before acquisition | `exposure set to TimeInterval { value: 100.0, unit: Milliseconds }` | `0x12 wIndex 0x0540 data=00000080a0860100` = `0x000186a0` = 100 000 µs | **Pass** | Encoding confirmed; the exposure's *effect* is unvalidated because no frame was produced |
| `CameraCapture` | One frame, 1392 x 1040 `Raw16` | 2 895 360 bytes on endpoint `0x86` | `Error { code: Transport, message: "Lumenera frame read timed out (0 of 2895360 bytes); camera reports ..." }` | Endpoint `0x86`: 1 URB submitted at `ACQ_START`, completed empty at teardown. **0 bytes** | **Fail** | Not promoted. See Remaining Uncertainty |
| `gain` (write) | any | Refused | `Unsupported` | none | **Pass** (fails closed as designed) | Register mapping still unevidenced |

## Completion And Events

| Operation | Hardware completion condition | Runtime completion/event | Hardware output/readback | Timeout or fault behavior | Result |
| --- | --- | --- | --- | --- | --- |
| Firmware initialization | Renumeration `809A` → `009A` | Device enumerates and opens | Confirmed in the power-on trace and by USB enumeration | n/a | **Pass** |
| `CameraCapture` | Full frame on `0x86` | `FrameReady` event | Never emitted | Read deadline `exposure + 5 s` expired; typed `Transport` error; teardown ran; interface returned to alternate setting 0 | **Fail** (timeout path itself behaved correctly) |

## Camera Or Stream Validation

| Field | Observation |
| --- | --- |
| Pixel format and color encoding | `Raw16`, 12-bit right-aligned little-endian — from the reference acquisition trace, not reproduced here |
| Frame dimensions and stride | 1392 x 1040, stride 2784 B — **confirmed from the device** (`0x1000`) and from the reference trace's frame size |
| Exposure/gain/binning/ROI | Exposure programmed and acknowledged; gain refused; binning fixed 1x1; ROI full frame |
| Transport mode | Bulk IN `0x86`, alternate setting 2. The configuration descriptor read off the wire confirms alt 2 exposes `0x82` IN bulk 512, `0x86` IN bulk 512, `0x81` IN interrupt 1 |
| Frames captured and target rate | **0** captured |
| Ring capacity and overflow policy | 4 queued URBs of 512 KiB; not exercised |
| Dropped frame counters | Not applicable |
| Frame metadata keys | Not exercised |
| Trigger/timestamp behavior | Not exercised |

## Remaining Uncertainty

| Behavior | Uncertainty | Evidence needed before support claim |
| --- | --- | --- |
| `CameraCapture` | The camera accepts every control transfer, reports correct geometry and bit depth, runs verified firmware, and delivers no bulk data. The acquisition command stream is byte-identical to the reference acquisition trace apart from the exposure value and that trace's additional acquisitions | A captured trace covering the interval between device open and first frame on a working stack, showing stream setup and state transitions in order |
| `0x0218` bit 0 | The register behaves as a bitmask; the low bit has never been observed set in any trace | A trace that exercises it, or a deliberate probe |
| `exposure` range and linearity | Two points only (90 ms, 10 s), from the reference trace | A sweep once frames are obtainable |
| `gain` | Per-tap registers `0x0276`–`0x027b` written every acquisition; no mapping to a canonical unit | Single-variable sweep against observed image response |
| Opaque configuration steps | `wIndex` `0x4008`, `0x4010`, `0x0550`, `0x05a0`, `0x0610`, `0x0670` and the post-stop FPGA write at `0x0544` replayed verbatim | Meaning unrecorded; not required for a support claim, but should stay labelled |

## Update Checklist

| Evidence item | Required update |
| --- | --- |
| Device page | Done — validation, evidence and capability-register rows updated in [`lumenera.md`](lumenera.md) |
| Evidence register | Done — [`evidence.md`](evidence.md) cites this note for firmware initialization and capability readback; capture remains unvalidated |
| Implementation plan | Remaining-work rows narrowed: firmware and geometry no longer open; bench validation of capture still open |
| Trace/log storage | Raw captures and analysis records are kept outside this repository |
| Tests | None added — driver tests are prohibited by `AGENTS.md` |

---

# 2026-08-06 — `CameraCapture` validated

Same unit (serial `19020090`), same host, after the two missing bring-up stages
were implemented. Evidence class: captured traffic from a physical device, plus
this documented bench run.

## What changed since the failing run

| Stage | Status on 2026-08-05 | Status now |
| --- | --- | --- |
| Sensor-pipeline configuration | absent | implemented — 98 KB image to bulk endpoint `0x08`, device accepts it (`0x80` -> `0x40` -> `0x00`) |
| Register load | absent | implemented — 510 recorded transfers replayed after configuration |
| Image reads | fixed-size chunks | sized to `min(chunk, remaining)` |

The read sizing was the last fault and the subtlest: a frame's final piece is an
exact multiple of the endpoint packet size, so it carries no short packet to
terminate an over-long request. Fixed-size reads collected every chunk but the
last and then blocked. The symptom was a capture stalling at exactly
2 621 440 of 2 895 360 bytes, reproducibly.

## Result

| Capability | Request | Runtime output | Hardware output | Result |
| --- | --- | --- | --- | --- |
| `CameraCapture` | one frame, 100 ms | `capture: map(frame, height, pixel_format, source, stream, width)`; `frame: 2895360 bytes -> gel_doc_frame.raw (38.7% non-zero)` | 2 895 360 B on `0x86` | **Pass** |
| `CameraCapture` | one frame, 200 ms | same | 2 895 360 B | **Pass** |
| `CameraCapture` | one frame, 500 ms | same | 2 895 360 B | **Pass** |
| Pipeline configuration | 98 023 B image | no error | `0x0008`: `0x80` -> arm -> 98 023 B on `0x08` -> `0x40` -> `0x00` | **Pass** |

Capture-level total on the image endpoint for the session: **17 372 160 bytes =
exactly 6 x 2 895 360**, i.e. six whole frames and no partial ones.

## Frame validation

| Field | Observation |
| --- | --- |
| Pixel format | `Raw16` little-endian, 12-bit right-aligned |
| Dimensions and stride | 1392 x 1040, stride 2784 B — matches the device's own `0x1000` readback |
| Decoded image | Uniform read noise, **no shear, no tearing, no row offset** — confirms framing, stride and pixel format |
| Content | Dark frame at 38.7% non-zero. Expected: the lamp belongs to the enclosure, a separate USB device this driver never touches |
| Frames captured | 6 of 6 attempted after the fix; 0 of 2 before it |

## Remaining uncertainty

| Behavior | Uncertainty |
| --- | --- |
| `exposure` radiometric effect | Encoding and acceptance confirmed at 100/200/500 ms, but every frame is dark, so the effect on image brightness is unverified |
| Register load semantics | Replayed, not understood. Layout decoded; individual meanings unrecorded. Now testable against a live image |
| `gain` | Still unevidenced; writes refused |
| Repeat configuration | An already-configured device reports `0xA0` and refuses another image. Returning to the `0x80` ready state required a power cycle in every attempt; no host-side reset was found |
| Streaming | Single-frame only; repeated-frame boundary handling untested |
