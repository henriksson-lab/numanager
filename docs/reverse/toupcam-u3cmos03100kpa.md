# Captured Trace Note — Toupcam U3CMOS03100KPA (0547:3310)

## Target And Status

| Field | Value |
| --- | --- |
| Target | ToupTek U3CMOS03100KPA USB3 camera |
| Device page | `docs/devices/toupcam.md` |
| Reverse note | `docs/reverse/toupcam-u3cmos03100kpa.md` |
| Status | promote (identity, geometry, frame format, open sequence) / needs more trace (exposure and gain register encoding) |
| Driver behavior under consideration | Second model profile for `numanager_drivers::toupcam`: per-model geometry, frame trailer, and recorded open sequence |

## Why This Trace Exists

The existing Toupcam backend was built against one model (U3CMOS08500KPA,
`0547:13a1`, 3328x2548). Pointed at a U3CMOS03100KPA it never produced a frame:
`read_frame` waited for the bench camera's 8 479 744 bytes from a device that
sends 3 141 633 bytes per frame, so every capture ran to the 15 s timeout. The
capture below establishes what actually differs between the two models.

## Capture Identity

| Field | Value |
| --- | --- |
| Hardware identity | ToupTek U3CMOS03100KPA, USB id `0547:3310`, device-reported product string `USB3.0 Camera`, in-device serial `TP21101316273817CB7B5FF925428E7` |
| Host identity | Windows 11 26200, x64; device bound to Microsoft `winusb.sys` 10.0.26100.8875 via the device's own MS OS `MS_COMP_WINUSB` compatible id |
| Software identity | ToupView 4.12.30405 (`C:\Program Files\ToupTek\ToupView\x64\toupview.exe`, vendor SDK `toupcam.dll`) |
| Capture tool | USBPcap via `reveng-rec record --device-vidpid 0547:3310` (reveng-recorder) |
| Operator and date | Repository owner, 2026-08-04 |
| Clock alignment | reveng-recorder normalizes USB frames, input events, and screenshots to one QPC timeline; each UI click is a checkpoint whose screenshot shows the vendor UI state at that instant |
| Capture integrity | `reveng-rec verify`: 56 control SETUPs, 56 completions, 0 unpaired, 0 non-zero status, 0 out-of-order timestamps |

## Raw Evidence Storage

| Evidence item | Evidence class or package id | Retention policy |
| --- | --- | --- |
| Raw capture (`usb.pcapng`, 91 MB and 390 MB sessions) | USBPcap capture of vendor application traffic | local / lab storage, not committed |
| Curated open sequence | `crates/numanager-drivers/src/toupcam_u3cmos03100kpa_init_seq.jsonl` (48 control transfers, `reveng-rec ctrl --json --req-type vendor`) | committed as the driver's replay asset |
| Checkpoint screenshots (vendor UI state) | reveng-recorder session screenshots | local / lab storage, not committed |
| Runtime output | numanager capture run, reproduced under "Runtime Output" below | this note |

## USB Descriptor Facts

Both models present the same transport shape, so the difference is protocol and
geometry, not endpoint layout.

| Field | U3CMOS08500KPA (`0547:13a1`) | U3CMOS03100KPA (`0547:3310`) |
| --- | --- | --- |
| Configurations / interfaces | 1 / 1 | 1 / 1 |
| Interface class | vendor-specific `ff/00/00` | vendor-specific `ff/00/00` |
| Endpoints | bulk-IN `0x81` | bulk-IN `0x81`, max packet 1024 (SuperSpeed) |
| Product string | `U3CMOS08500KPA` | `USB3.0 Camera` (model name is not in the descriptor) |
| Serial string | — | absent from descriptors; the model/serial live in the request `0x20` payload |

No firmware download appears anywhere in the capture: the only OUT data in the
whole session is the 16-byte request `0x47` payload. The device enumerates ready
to stream. The `0x04b4` Cypress vendor id the driver also claims is the
pre-firmware bootloader identity, which this unit was already past.

## Vendor Request Comparison

Both models use vendor requests on EP0, but the opcodes differ, so a capture
from one model does not drive the other.

| Purpose | U3CMOS08500KPA | U3CMOS03100KPA |
| --- | --- | --- |
| Probe / handshake read | `0x16` | `0x16` |
| 16-byte host challenge (OUT) | `0x4c` | `0x47` |
| 16-byte device response (IN) | `0x7d` | `0x75` |
| Descriptor blob read | `0x23` | `0x20` |
| Register access | `0x0b` | `0x0b` |
| Stream start/stop (OUT) | `0x01` | `0x01`, `wValue=0x0003` starts, `wValue=0x0000` stops, `wIndex=0x000f` |
| Open sequence length | 681 transfers | 48 transfers |

### Request `0x20` — model/calibration blob

A 4-byte read returns `0x000006ba` (1722); a 1770-byte read returns the whole
record:

| Offset | Bytes | Meaning |
| --- | --- | --- |
| `0x000` | `ba 06 00 00` | 1722 = end offset of the compressed section |
| `0x004` | `32 a9 05 00` | unidentified (not the decompressed length) |
| `0x008` | `00` | unidentified |
| `0x009`..`0x6ba` | `BZh9…` | bzip2 stream; decompresses to 1449 bytes of per-record calibration data (records carry an incrementing index, consistent with a per-row or defect-pixel table) — content not decoded |
| `0x6ba` | ASCII | in-device serial `TP21101316273817CB7B5FF925428E7`, NUL-terminated |
| after | 9 bytes | unidentified trailer |

The driver does not parse this blob; it is replayed as a read and discarded.
Recorded here because it is the only in-band source of the model serial.

## Frame Format

`reveng-rec frame-guess` on the bulk endpoint, time-gap segmentation:
modal **3 141 633 bytes** per frame, median period 96.1 ms (~10.4 fps).

The checkpoint screenshot taken at the same instant shows the vendor UI
reporting `Live: 2048 × 1534` for camera `U3CMOS03100KPA`. That pairs the
on-the-wire byte count with the on-screen geometry:

```
2048 x 1534      = 3 141 632 bytes RAW8
observed frame   = 3 141 633 bytes
difference       =         1 byte trailer
```

The trailer is a single byte the device appends after the pixel plane. It
sometimes arrives inside the same bulk transfer as the last pixels and sometimes
as its own 1-byte transfer, so a reader must consume it either way or the stream
drifts by one byte per frame.

Frames are delimited by a short bulk transfer. With 512 KiB reads a full frame
is five 524 288-byte transfers plus a 520 193-byte remainder.

## Register Encoding — SUPERSEDED, now decoded

**This section previously concluded the register encoding was an unbreakable
session key. That conclusion was wrong.** It is kept only as a record of the
error, because the reasoning failed in an instructive way.

The masking is not a device-issued nonce. The host chooses a 16-bit token, sends
it as the `wValue` of the `0x16` probe, and both sides derive the mask from it —
and the derivation maps token 0 to mask 0. An implementation sends token 0 and
uses plaintext register numbers. See `toupcam-protocol.md`.

Two mistakes produced the wrong conclusion:

* The exposure/`wValue` table below is **offset by one row** against its
  screenshots. Each mousedown screenshot shows the value in force *before* that
  click, so pairing them positionally attributed every payload to the wrong
  exposure. That is why no transform fitted, and why two rows appeared to show
  one exposure with two payloads — they were different exposures.
* Differing register indices between sessions were read as evidence of a device
  secret. They were the host picking a different token each run.

The original (mis-paired) observations, retained so the error is auditable:

| Exposure shown in vendor UI | `wValue` written to `0xb85b` |
| --- | --- |
| 842.063 ms | `0x0310` |
| 344.687 ms | `0x046e` |
| 96.06 ms | `0xb116` |
| 9.912 ms | `0x87b4` |
| 0.1 ms | `0x89ef` |
| 0.1 ms | `0x884d` |

Correctly paired and unmasked, these are writes of `COARSE_INTEGRATION_TIME`
(`0x3012`) and reproduce exactly from the exposure formula.

The replay observation in this note stands and was never in doubt: replaying a
recorded sequence verbatim reproduces the state it was captured at, because it
replays the same token.

## Action Timeline

| Step | Action | Trace window | Observed result |
| --- | --- | --- | --- |
| 1 | Launch ToupView, click camera in list | t≈42.3 s | 48 vendor control transfers, ending with `0x01 wValue=0x0003` |
| 2 | Stream runs | t≈43.2 s onward | bulk-IN `0x81`, 3 141 633-byte frames at ~10.4 fps |
| 3 | Uncheck Auto Exposure, click exposure slider x5 | t≈42.8–56.4 s | one `0x0b` write per click to a single register index |
| 4 | Click gain slider x4 | t≈64.1–78.8 s | one `0x0b` write per click to a different register index |
| 5 | Close camera | t≈87.9 s | `0x01 wValue=0x0000` stop, then `0x17` read |

## Hardware Replay Verification

The committed 48-transfer sequence was replayed against the device with the
vendor application closed:

- 48 of 48 control transfers completed with status 0, 0 failures.
- After the stream start the device delivered steady 3 141 633-byte frames
  (`frame 1 = 3141633`, `frame 2 = 3141633`, `frame 3 = 3141633`).
- Pixel statistics on a decoded frame: `min=0 max=195 mean=15`, row-to-row mean
  absolute difference 9.4 — structured image content, matching the scene the
  vendor UI was showing, not noise or a constant fill.
- Replaying only the open prefix (without the trailing register writes) yields
  `min=0 max=60 mean=0`, i.e. the camera's dark default exposure, confirming the
  trailing writes are the exposure/gain state and that they replay.

## Runtime Output

Captured through numanager's public runtime API (`ToupcamDriver::open_first_usb`
→ `CameraCaptureRequest`), vendor application closed:

```
known models:
  U3CMOS08500KPA pid=0x13a1 3328x2548 trailer=0 frame_bytes=8479744 tunable=true
  U3CMOS03100KPA pid=0x3310 2048x1534 trailer=1 frame_bytes=3141633 tunable=false
added driver with 1 device(s)
camera: Toupcam USB3.0 Camera 0547:3310 bus 0 addr 16
captured frame: size=Some(2048)xSome(1534) format=Some("Raw8")
frame store: 2048x1534 Raw8
frame bytes: 3141632
pixel stats: min=0 max=247 mean=8
```

Setting exposure through the same API reports, as intended:

```
Error: Error { code: Unsupported, message: "Toupcam U3CMOS03100KPA: exposure over
USB is not decoded for this model; its 0x0b register writes are obfuscated with a
session-dependent scramble. The camera stays at the state its recorded open
sequence reproduces." }
```

## Frame Synchronization Defect Found

The first hardware capture through the driver returned a torn frame (a black
band where two partial frames met). `read_frame` took a fixed byte count from
wherever the free-running stream happened to be. Fixed by segmenting on the
device's short-transfer frame delimiter and keeping the first segment that holds
a whole frame. This affected both models, not only the new one; the bench
camera's `read_frame` had the same unsynchronized read.

## Remaining Uncertainty

| Behavior | Uncertainty | Evidence needed before support claim |
| --- | --- | --- |
| Exposure/gain register encoding | Session-keyed, not a fixed mapping; not synthesizable | Key schedule from vendor `toupcam.dll`, or a sweep large enough to model per-write state |
| Trailer byte meaning | Position confirmed (1 byte after the pixel plane); content not interpreted | Frames captured under varied settings, checking whether the byte tracks a counter or status |
| Request `0x20` blob contents | Layout partly mapped; decompressed records not interpreted | Comparison across units/models |
| Bayer phase | Not determined for this model | A frame of a known colour target |
| ROI, binning, trigger, resolution switching | Not exercised in this capture | A capture driving those vendor UI controls |
| Exposure range | Vendor UI reached 0.1 ms to 842 ms in this session; true limits unknown | A capture sweeping to both slider ends |
