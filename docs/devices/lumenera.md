# Lumenera Lu130 / Bio-Rad Gel Doc EZ Camera

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::lumenera` |
| Families | Lumenera Lu130-class USB cameras, including the Bio-Rad Gel Doc EZ OEM unit |
| Support level | USB descriptor discovery for both stages, hidden EZ-USB firmware initialization when explicitly connected, and live `CameraCapture` with writable `exposure`; `gain` fails closed because its register mapping is unevidenced |
| Evidence | Hardware trace for the Bio-Rad OEM USB IDs and firmware initialization sequence; captured hardware traffic (2026-08-05) for the acquisition sequence, geometry, exposure encoding and frame layout; a documented bench run (2026-08-05) for the read-only capability registers, the reported geometry and bit depth, and the firmware-image wire comparison |
| Transport | USB userspace via `nusb`; EZ-USB anchor writes are internal initialization only |
| Validation | Firmware initialization live-confirmed 2026-08-03. The acquisition sequence and frame layout were read from captured traffic on 2026-08-05 and decode to a correct image. The driver implementation of that sequence WAS run against hardware on 2026-08-05 and returned no frame: every control write was accepted and endpoint `0x86` delivered 0 bytes. Full bench record in [`lumenera-hardware-validation-2026-08-05.md`](lumenera-hardware-validation-2026-08-05.md), which promotes firmware initialization and the capability readback and leaves capture unvalidated. `hardware_validated` stays false for capture |

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
| `property.image_selector` | No | integer | Selects the firmware image by USB `bcdDevice` value; unknown values use the catch-all image |
| `property.connect` | No | bool | Enables live USB firmware initialization when `os-usb` is compiled; default `false` |

Live USB discovery has no config file to carry `property.connect`, so a caller
opts in through `LumeneraDiscovery::with_firmware_initialization()` instead.
Both gates default to off: `detect()` runs against whatever is plugged in, and
passive enumeration must never write to a device the user has not claimed.

The EZ-USB images are compiled into the binary, so a power-cycled camera reloads
with no `data/` directory to locate; `firmware_dir` overrides them. They are
third-party data under their own license terms, not this repository's, and ship
inside every binary built from this crate — identity, sizes and SHA-256 digests
in `data/third_party/lumenera/manifest.toml`.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- gel_doc` | Configured loader-stage topology, the full property/gate readout, and `CameraCapture` failing closed without a live session |
| `cargo run -p numanager-examples --features os-usb -- gel_doc live` | Read-only enumeration of real units and their firmware stage |
| `cargo run -p numanager-examples --features os-usb -- gel_doc initialize-firmware` | Two-stage firmware download against real hardware |
| `cargo run -p numanager-examples --features os-usb -- gel_doc capture [exposure_ms]` | Sets `exposure` and takes one frame off a live camera |

## Remaining Work

| Area | Gap |
| --- | --- |
| Capability registers | `0x1000` (and `0x100c`) read back `0x04100570` = 1392 x 1040, and `0x1014` reads `0x0c` = 12 bpp, both live-confirmed 2026-08-05 — the camera reports exactly the geometry the driver programs. `0x0004`, `0x019c`, `0x0280`, `0x0284`, `0x1004`, `0x1008`, `0x101c`, `0x1040` are read but unidentified |
| Bench validation | The implemented sequence ran on 2026-08-05 and produced no frame. The wire trace is byte-identical to the reference acquisition trace, so the acquisition sequence itself is not the gap; the open candidates are image-endpoint pipe state and device state carried over from a prior session. The interface is now returned to the idle alternate setting on open and the image endpoint's halt is cleared before reads are queued, and a failed capture reports the camera's capability registers. None of that is hardware validated |
| Gain | Registers `0x0276`-`0x027b` are written on every acquisition (four equal values, then two others — consistent with per-tap gain/offset on a dual-tap sensor), but no mapping to a canonical unit is recorded. Needs a single-variable sweep |
| Opaque configuration steps | `wIndex` `0x4008`, `0x4010`, `0x0550`, `0x05a0`, `0x0610`, `0x0670` and the post-stop FPGA write at `0x0544` are replayed verbatim with unrecorded meaning |
| Binning and ROI | The wire encoding is known (`wIndex 0x4018`/`0x400c`), but only 1x1 at full frame has been observed, so neither is exposed as a property |
| Exposure range | Two points (90 ms and 10 s) fit `elapsed ≈ exposure + ~85 ms`. Min/max and linearity across the range are unconfirmed |
| Streaming | Only single-frame capture is implemented; `CameraStream` is not offered |
| Documentation | Record a hardware-validation note with exact unit identity, firmware package identity, firmware digest, OS/runtime versions, and observed output |
