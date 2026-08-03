# Lumenera Lu130 / Bio-Rad Gel Doc EZ Camera

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::lumenera` |
| Families | Lumenera Lu130-class USB cameras, including the Bio-Rad Gel Doc EZ OEM unit |
| Support level | USB descriptor discovery for the loader and imaging stages plus hidden EZ-USB firmware initialization when explicitly connected; imaging capture and exposure/gain control fail closed because the imaging wire protocol is not recorded |
| Evidence | Hardware trace for the Bio-Rad OEM USB IDs and firmware initialization sequence; nominal sensor geometry from device literature remains metadata only |
| Transport | USB userspace via `nusb`; EZ-USB anchor writes are internal initialization only |
| Validation | Firmware initialization was live-confirmed on 2026-08-03 for one Bio-Rad Gel Doc EZ unit; frame capture/control validation is pending |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `<label>` | `camera`, `camera.scientific`, `detector.mono`, `reverse.engineered` | One camera descriptor with discovery, firmware-stage, USB identity, and protocol-gate metadata |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `CameraCapture` | Camera | `CapabilityRequest::CameraCapture` | Unsupported error until imaging setup and frame framing are evidenced | None | None |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `model`, `serial_number`, `sensor`, `pixel_format` | Camera | `String` | none | R | `Mono16` is the conservative configured pixel-format claim | No | Descriptor/configured metadata |
| `bit_depth`, `width`, `height` | Camera | `I64` / `PixelCount` | bits / px | R | Nominal Sony ICX205 metadata; exact active ROI pending live protocol evidence | No | Configured metadata |
| `exposure`, `gain` | Camera | `TimeInterval` / `F64` | s / none | R/W schema | Writes are rejected until control tuples are recorded | No | Not exposed |
| `firmware_loaded`, `firmware_stage`, `firmware_dir`, `firmware_image` | Camera | `Bool` / `String` | none | R | Configured or discovered stage metadata | No | USB descriptor and configured package path |
| `usb_vendor_id`, `usb_product_id`, `usb_identity` | Camera | `I64` / `Map` | none | R | OEM VID `0x5354`, stock Lumenera VID `0x1724`, loader PID `0x809a`, imaging PID `0x009a` | No | USB descriptors |
| `support_level`, `protocol_status`, `capture_gate`, `control_gate` | Camera | `String` | none | R | Current evidence boundary | No | Runtime metadata |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "lumenera"` | Yes | string | Selects the Lumenera camera provider; aliases include `lumenera_camera`, `lumenera-camera`, `geldoc_ez`, and `geldoc-ez` |
| `property.vendor_id`, `property.product_id` | No | integer | Override the configured USB identity |
| `property.product`, `property.serial_number` | No | string | Configured descriptor metadata |
| `property.firmware_dir` | Required for explicit firmware initialization | string | Directory containing the third-party firmware package |
| `property.image_selector` | No | integer | Selects the firmware image by USB `bcdDevice` value; unknown values use the catch-all image |
| `property.connect` | No | bool | Enables live USB firmware initialization when `os-usb` is compiled; default `false` |

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- discover_devices` | Configured discovery metadata and support gates; no frame capture is claimed |

## Remaining Work

| Area | Gap |
| --- | --- |
| Imaging protocol | Need USB control tuples for property/register access, stream setup, frame transfer, completion, and error behavior |
| Sensor geometry | Confirm active ROI, row stride, and pixel packing against live descriptors or captured frames |
| Runtime behavior | Validate exposure/gain write semantics, capture timing, abort/timeout behavior, and final safe state |
| Documentation | Record a hardware-validation note with exact unit identity, firmware package identity, firmware digest, OS/runtime versions, and observed output |
