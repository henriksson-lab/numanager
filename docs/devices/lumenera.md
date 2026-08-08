# Lumenera Lu130 / Bio-Rad Gel Doc EZ Camera

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::lumenera` |
| Families | Lumenera Lu130-class USB cameras, including the Bio-Rad Gel Doc EZ OEM unit |
| Support level | USB descriptor discovery for both stages, EZ-USB firmware initialization, sensor-pipeline configuration, register load, and **live single-frame `CameraCapture`** at 1392x1040 `Raw16` with writable `exposure`; `gain` fails closed because its register mapping is unevidenced |
| Evidence | Hardware trace for the Bio-Rad OEM USB IDs and firmware initialization; captured hardware traffic (2026-08-05, 2026-08-06) for the bring-up chain, acquisition sequence, exposure encoding and frame layout; documented bench runs (2026-08-05, 2026-08-06) |
| Transport | USB userspace via `nusb`; EZ-USB anchor writes and the pipeline image are internal initialization only |
| Validation | **Capture is hardware-validated (2026-08-06):** six complete frames of 2 895 360 bytes each on endpoint `0x86`, at 100 ms, 200 ms and 500 ms exposures, decoding to a correctly aligned 1392x1040 image with no shear or tearing. Firmware initialization live-confirmed 2026-08-03. Full bench record in [`lumenera-hardware-validation-2026-08-05.md`](lumenera-hardware-validation-2026-08-05.md) |
| Feature gate | Default-on Cargo feature `lumenera`; disable with `--no-default-features` when the large Lumenera firmware/trace payloads and GPL-marked SDK-derived code should not be built |
| Source license scope | Lumenera driver source files are `GPL-2.0-only`; the repository default license applies only to files without a more specific SPDX header |

## Bring-Up Chain

A cold camera needs four stages before it will produce a frame. Each was
recorded from captured traffic and each is required — skipping the pipeline
configuration leaves a camera that accepts every control transfer and delivers
zero image bytes, which is indistinguishable from a wiring fault without a
trace.

| Stage | What happens | Notes |
| --- | --- | --- |
| 1. Firmware | EZ-USB anchor download (`0xA0`), `CPUCS` held then released; the device renumerates from the loader id to the imaging id | Selected by `bcdDevice`. On a host where a third-party loader already owns the loader node, that driver does this and numanager must not |
| 2. Pipeline configuration | A 98 KB image streamed to bulk endpoint `0x08` under alternate setting 1, bracketed by arm (`0xFFFFFFFF`) and finish (`0`) writes on `wIndex 0x0008` | `0x0008` reports `0x80` ready, `0x40` busy, `0x00` done, `0xA0` already configured. An arm is accepted **only** from `0x80`; a configured device refuses another and must be power-cycled to return to `0x80` |
| 3. Register load | 510 recorded transfers, mostly 8-bit writes on `wIndex 0x0006` addressed by `wValue` | Replayed as recorded. Layout is understood (reset pulse, ascending sweep, four per-channel blocks of 30 at stride 38); individual register meanings are not |
| 4. Acquisition | Configure geometry/exposure, select alternate setting 2, arm, start, drain the frame, stop, restore alternate setting 0 | Per capture |

Stages 2 and 3 run once per session, at open. Stage 2 is skipped when the
device reports it is already configured.

### Reads must be sized to the remainder

A frame is not a whole number of transfer chunks, and its final piece is an
exact multiple of the endpoint's packet size — so it carries no short packet to
terminate an over-long request. A reader that always asks for a full chunk gets
every chunk but the last and then blocks forever. Each read is therefore sized
to `min(chunk, remaining)`.

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `<label>` | `camera`, `camera.scientific`, `detector.mono`, `reverse.engineered` | One camera descriptor with discovery, firmware-stage, USB identity, and protocol-gate metadata |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `CameraCapture` | Camera | `CapabilityRequest::CameraCapture` | `Map` of width/height/pixel_format/stream/frame plus a `FrameReady` event carrying 1392x1040 `Raw16` bytes; `Unsupported` without a live session | `FrameReady` | None |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `model`, `serial_number`, `sensor`, `pixel_format` | Camera | `String` | none | R | `Mono16` is the conservative configured pixel-format claim | No | Descriptor/configured metadata |
| `bit_depth`, `width`, `height` | Camera | `I64` / `PixelCount` | bits / px | R | 12-bit, 1392 x 1040 — read off the dimension write and confirmed by frame size | No | `0x12` `wIndex 0x400c`, two LE u16 |
| `exposure` | Camera | `TimeInterval` | s | R/W | Applied at the next acquisition, not on write | No | `0x12` `wIndex 0x0540`, `[u32 0x80000000][u32 microseconds]` |
| `gain` | Camera | `F64` | none | R/W schema | Writes rejected: per-tap register mapping to a canonical unit is unevidenced | No | Not exposed |
| `firmware_loaded`, `firmware_stage`, `firmware_dir`, `firmware_image` | Camera | `Bool` / `String` | none | R | Configured or discovered stage metadata | No | USB descriptor and configured package path |
| `connect` | Camera | `Bool` | none | R | Whether this probe is authorized to push firmware; default `false` | No | Configured or discovery-level opt-in |
| `usb_vendor_id`, `usb_product_id`, `usb_identity` | Camera | `I64` / `Map` | none | R | OEM VID `0x5354`, stock Lumenera VID `0x1724`, loader PID `0x809a`, imaging PID `0x009a` | No | USB descriptors |
| `support_level`, `protocol_status`, `capture_gate`, `control_gate` | Camera | `String` | none | R | Current evidence boundary | No | Runtime metadata |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "lumenera"` | Yes | string | Selects the Lumenera camera provider; aliases include `lumenera_camera`, `lumenera-camera`, `geldoc_ez`, and `geldoc-ez` |
| `property.vendor_id`, `property.product_id` | No | integer | Override the configured USB identity |
| `property.product`, `property.serial_number` | No | string | Configured descriptor metadata |
| `property.firmware_dir` | No | string | Firmware package directory, overriding the compiled-in images |
| `property.image_selector` | No | integer | Selects the firmware image by USB `bcdDevice` value; supported SDK selectors are `0x0000`, `0x0001`, `0x0010`, and `0x0018`; unknown values fail closed |
| `property.connect` | No | bool | Enables live USB firmware initialization when `os-usb` is compiled; default `false` |

Live USB discovery has no config file to carry `property.connect`, so a caller
opts in through `LumeneraDiscovery::with_firmware_initialization()` instead.
Both gates default to off: `detect()` runs against whatever is plugged in, and
passive enumeration must never write to a device the user has not claimed.

The EZ-USB images are compiled into the binary, so a power-cycled camera reloads
with no `data/` directory to locate; `firmware_dir` overrides them. They are
third-party data under their own license terms, not this repository's, and ship
inside binaries built with the default-on `numanager-drivers/lumenera` feature —
identity, sizes and SHA-256 digests in `data/third_party/lumenera/manifest.toml`.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- gel_doc` | Configured loader-stage topology, the full property/gate readout, and `CameraCapture` failing closed without a live session |
| `cargo run -p numanager-examples --features os-usb -- gel_doc live` | Read-only enumeration of real units and their firmware stage |
| `cargo run -p numanager-examples --features os-usb -- gel_doc initialize-firmware` | Two-stage firmware download against real hardware |
| `cargo run -p numanager-examples --features os-usb -- gel_doc capture [exposure_ms]` | Diagnostic acquisition against a live camera; validated complete 1392x1040 `Raw16` frames on 2026-08-06 |

## Remaining Work

| Area | Gap |
| --- | --- |
| Register semantics | The 510-transfer load is replayed, not understood. Its layout is decoded — a reset pulse on register `0x17`, an ascending sweep over `0x0000`-`0x0286` in contiguous runs, four per-channel blocks of 30 registers at stride 38, then a re-write tail — but no individual register has a recorded meaning. Now testable: with a live image, a single-variable sweep can be observed rather than guessed |
| Capability registers | After FPGA setup and the recorded register load, `0x0010` (`SPECIFICATION`), `0x0280` (`LUCAM_FLAGS`) and `0x000c` (`FIRMFPGA_VERSION`) are mandatory SDK reads; video/still enables are then cleared. `0x019c` is SDK `FORMAT_COUNT`. The best-effort SDK refresh also reads FPGA mode, message support and FO position/size/color/subsampling/tap registers (`0x0008`, `0x4ff8`, `0x1008`, `0x100c`, `0x1010`, `0x1018`, `0x101c`, `0x1020`, `0x1024`, `0x1068`); when the FO values are available they are copied into the corresponding still registers, matching SDK init. When flags bit `0x00000002` is set, or `SPECIFICATION >= 2`, still format setup is validated through `LUCAM_STILL_VALIDATE` (`0x4060`) and SDK readback of still color id/tap configuration/position (`0x4010`/`0x4068`/`0x4008`). When flags bit `0x00004000` is set the driver writes `0x00010000` to `LUCAM_STILL_TRANSFER_SIZE` before still enable. `0x1000` (and `0x100c`) read back `0x04100570` = 1392 x 1040. `0x1014` is a device state code, not bit depth. `0x0004` and `0x1004` are read but otherwise unidentified |
| Illumination | The camera is one of two USB devices in the enclosure; the lamp belongs to the other and this driver never touches it. Every frame captured so far is therefore a dark frame. A lit image needs the enclosure driven in parallel |
| Gain | Registers `0x0276`-`0x027b` are written on every acquisition (four equal values, then two others — consistent with per-tap gain/offset on a dual-tap sensor), but no mapping to a canonical unit is recorded. Needs a single-variable sweep |
| SDK-named but unexposed parameters | `0x0550` (`LUCAM_STILL_GAIN`), `0x05a0` (`LUCAM_STILL_STROBE_DELAY`), `0x0610` (`LUCAM_STILL_EXPOSURE_DELAY`) and `0x0670` (`LUCAM_SNAPSHOT_SETTING`) are SDK-named parameter registers. They are initialized/captured on the wire but not exposed as public numanager properties because their user-facing value semantics are still unvalidated. The post-stop FPGA write at `0x0544` remains captured teardown with unrecorded meaning |
| Binning and ROI | The wire encoding is known (`wIndex 0x4018`/`0x400c`), but only 1x1 at full frame has been observed, so neither is exposed as a property |
| Exposure range | Captures at 100 ms, 200 ms and 500 ms all returned full frames, and the vendor trace fits `elapsed ≈ exposure + ~85 ms` at 90 ms and 10 s. Min/max and linearity across the range are still unconfirmed, and the *radiometric* effect of `exposure` is unverified because every frame so far is dark |
| Streaming | Only single-frame capture is implemented; `CameraStream` is not offered. The transport supports it — the device streams continuously once started — but frame-boundary handling across repeated frames is untested |
