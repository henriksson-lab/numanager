# Captured Trace Note — Toupcam U3CMOS03100KPA (0547:3310)

## Target And Status

| Field | Value |
| --- | --- |
| Target | ToupTek U3CMOS03100KPA USB3 camera |
| Device page | `docs/devices/toupcam.md` |
| Protocol note | [`toupcam-protocol.md`](toupcam-protocol.md) |
| Status | promote (identity, geometry, frame format, open sequence, exposure/gain) |

## Capture Identity

| Field | Value |
| --- | --- |
| Hardware | ToupTek U3CMOS03100KPA, `0547:3310`, product string `USB3.0 Camera`, in-device serial `TP21101316273817CB7B5FF925428E7` |
| Host | Windows 11 26200 x64; bound to Microsoft `winusb.sys` via the device's `MS_COMP_WINUSB` compatible id |
| Capture | USBPcap via `reveng-rec record --device-vidpid 0547:3310`, one QPC timeline for USB frames, input events and screenshots |
| Date | 2026-08-04 |
| Integrity | `reveng-rec verify`: 56 control SETUPs, 56 completions, 0 unpaired, 0 non-zero status, 0 out-of-order timestamps |

Raw captures and screenshots stay in local lab storage. The curated 48-transfer
open sequence is committed as
`crates/numanager-drivers/src/toupcam_u3cmos03100kpa_init_seq.jsonl`.

## USB Descriptor Facts

Both models present the same transport shape, so the difference is protocol and
geometry, not endpoint layout.

| Field | U3CMOS08500KPA (`0547:13a1`) | U3CMOS03100KPA (`0547:3310`) |
| --- | --- | --- |
| Configurations / interfaces | 1 / 1 | 1 / 1 |
| Interface class | vendor-specific `ff/00/00` | vendor-specific `ff/00/00` |
| Endpoints | bulk-IN `0x81` | bulk-IN `0x81`, max packet 1024 (SuperSpeed) |
| Product string | `U3CMOS08500KPA` | `USB3.0 Camera` (model name not in the descriptor) |
| Serial string | — | absent from descriptors; model/serial live in the request `0x20` payload |

No firmware download appears in the capture — the only OUT data in the whole
session is the 16-byte request `0x47` payload; the device enumerates ready to
stream. The `0x04b4` Cypress vendor id the driver also claims is the
pre-firmware bootloader identity, which this unit was already past.

## Vendor Request Comparison

Opcodes differ between models, so a capture from one model does not drive the
other.

| Purpose | U3CMOS08500KPA | U3CMOS03100KPA |
| --- | --- | --- |
| Probe / handshake read | `0x16` | `0x16` |
| 16-byte host challenge (OUT) | `0x4c` | `0x47` |
| 16-byte device response (IN) | `0x7d` | `0x75` |
| Descriptor blob read | `0x23` | `0x20` |
| Register access | `0x0b` | `0x0b` |
| Stream start/stop (OUT) | `0x01` | `0x01`, `wValue=0x0003` start, `0x0000` stop, `wIndex=0x000f` |
| Open sequence length | 681 transfers | 48 transfers |

### Request `0x20` — model/calibration blob

A 4-byte read returns `0x000006ba` (1722); a 1770-byte read returns the record:

| Offset | Bytes | Meaning |
| --- | --- | --- |
| `0x000` | `ba 06 00 00` | 1722 = end offset of the compressed section |
| `0x004` | `32 a9 05 00` | unidentified (not the decompressed length) |
| `0x008` | `00` | unidentified |
| `0x009`..`0x6ba` | `BZh9…` | bzip2 stream; 1449 bytes of per-record calibration data (incrementing record index, consistent with a per-row or defect-pixel table) — not decoded |
| `0x6ba` | ASCII | in-device serial, NUL-terminated |
| after | 9 bytes | unidentified trailer |

The driver does not parse this blob; it is the only in-band source of the serial.

## Frame Format

Modal **3 141 633 bytes** per frame, median period 96.1 ms (~10.4 fps):
`2048 x 1534 = 3 141 632` RAW8 pixel bytes plus a **1-byte trailer**.

The trailer sometimes arrives inside the same bulk transfer as the last pixels
and sometimes as its own 1-byte transfer, so a reader must consume it either way
or the stream drifts by one byte per frame. Frames are delimited by a short bulk
transfer: with 512 KiB reads a full frame is five 524 288-byte transfers plus a
520 193-byte remainder. A reader that takes a fixed byte count from the
free-running stream tears frames; the driver segments on the short-transfer
delimiter and keeps the first segment holding a whole frame.

## Register Encoding — SUPERSEDED, now decoded

**This section previously concluded the register encoding was an unbreakable
per-session device key. That conclusion was wrong**, and is kept as a record of
the error.

The masking is not a device-issued nonce: the host chooses a 16-bit token, sends
it as the `wValue` of the `0x16` probe, and both sides derive the mask from it —
token 0 maps to the identity mask, so an implementation sends token 0 and uses
plaintext register numbers. See [`toupcam-protocol.md`](toupcam-protocol.md).

Two mistakes produced the wrong conclusion:

* The table below is **offset by one row** against its screenshots. Each
  mousedown screenshot shows the value in force *before* that click, so pairing
  positionally attributed every payload to the wrong exposure. That is why no
  transform fitted, and why two rows appeared to show one exposure with two
  payloads — they were different exposures.
* Differing register indices between sessions were read as evidence of a device
  secret. They were the host picking a different token each run.

The original (mis-paired) observations, retained so the error is auditable:

| Exposure shown on screen | `wValue` written to `0xb85b` |
| --- | --- |
| 842.063 ms | `0x0310` |
| 344.687 ms | `0x046e` |
| 96.06 ms | `0xb116` |
| 9.912 ms | `0x87b4` |
| 0.1 ms | `0x89ef` |
| 0.1 ms | `0x884d` |

Correctly paired and unmasked, these are writes of `COARSE_INTEGRATION_TIME`
(`0x3012`) and reproduce exactly from the exposure formula. The replay
observation was never in doubt: replaying a recorded sequence verbatim
reproduces the state it was captured at, because it replays the same token.

## Hardware Replay Verification

The committed 48-transfer sequence replayed against the device: 48 of 48
transfers status 0, then steady 3 141 633-byte frames. A decoded frame gives
`min=0 max=195 mean=15` with row-to-row mean absolute difference 9.4 —
structured image content, not noise or constant fill. Replaying only the open
prefix (without the trailing register writes) yields `min=0 max=60 mean=0`, the
dark default exposure, confirming the trailing writes carry exposure/gain state.

Through numanager's public runtime API (`ToupcamDriver::open_first_usb` →
`CameraCaptureRequest`):

```
camera: Toupcam USB3.0 Camera 0547:3310 bus 0 addr 16
captured frame: size=Some(2048)xSome(1534) format=Some("Raw8")
frame bytes: 3141632
pixel stats: min=0 max=247 mean=8
```

Verified exposure and gain sweeps are in
[`toupcam-protocol.md`](toupcam-protocol.md#hardware-validation).

## Remaining Uncertainty

| Behavior | Uncertainty | Evidence needed before support claim |
| --- | --- | --- |
| Trailer byte meaning | Position confirmed; content not interpreted | Frames under varied settings, checking for a counter or status |
| Request `0x20` blob | Layout partly mapped; records not interpreted | Comparison across units/models |
| Bayer phase | Not determined | A frame of a known colour target |
| ROI, binning, trigger, resolution switching | Not exercised | A capture driving those controls |
| Exposure range | 0.1 ms to 842 ms observed; true limits unknown | A sweep to both ends |
