# Toupcam-Compatible Cameras

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::toupcam` |
| Families | Toupcam/ToupTek and AmScope-like USB cameras |
| Support level | Config-backed geometry/identity plus live userspace USB backend behind `os-usb` with retained USB identity metadata and local frame source |
| Protocol evidence | OpenGEL clean-room Toupcam backend, captured init sequence promoted as a runtime asset, public USB identity/register notes, and existing camera-control behavior as secondary evidence |
| Transport | Runtime frame-ring path plus fixture USB-control/raw-register surface; optional `nusb` control and bulk-IN transport for live devices |
| Discovery | Simulated two-stage discovery; config-backed discovery for model geometry/identity; optional live USB discovery through Toupcam/ToupTek/Cypress vendor IDs; live descriptors retain product, serial, VID/PID, bus, and address metadata |
| Validation | OpenGEL bench path recorded live U3CMOS08500KPA RAW8 frame capture; numanager hardware validation note has not been added |
| Runtime/evidence notes | `numanager-drivers/os-usb` for live USB discovery, init replay, exposure/gain control, and RAW8/Mono8 frame capture/stream sized from configured or bench-camera geometry. RGB/BGR output is software debayering and requires configured `bayer_phase`; black-level and white-balance controls are runtime image processing, not USB register writes |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `toupcam-0`, configured label, or live USB label | `camera`, `trigger.sink`, `raw.register` | One logical camera device with control and streaming resources; configured labels come from config, live labels come from product, VID/PID, bus, address, and serial descriptors |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `toupcam-control` | `usb.control` | USB-style control endpoint fixture or live `nusb` vendor-control endpoint for register/property commands; metadata records `connected` and `usb_identity` |
| `toupcam-bulk-stream` | `usb.bulk-in` | Fixture frame source or live bulk-IN endpoint `0x81` feeding runtime frame rings; metadata records `connected`, `usb_identity`, `bulk_chunk`, and configured `frame_bytes` |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `CameraCapture` | Camera | `CapabilityRequest::CameraCapture` | `CapturedFrame`-parseable frame handle through runtime frame store | Fixture runtime completion or live queued bulk frame read plus `FrameReady` event; live RGB/BGR requests require configured `bayer_phase` | Capture can join triggered acquisition plans after acquisition-setting sequences |
| `CameraStream` | Camera | `CapabilityRequest::CameraStream` with ring-buffer policy | `CameraStreamStarted`-parseable stream id, typed width/height, frame count, pixel format, and frame events | Fixture runtime events or live repeated queued bulk frame reads; live RGB/BGR requests require configured `bayer_phase` | Continuous acquisition path |
| `TriggerSink` | Camera | `None` or `CapabilityRequest::Trigger` | Trigger status map plus telemetry/property events | Runtime token completion after fixture control ack | Trigger route endpoint and software pulse fixture |
| `RawRegisterAccess` | Camera | `GenericCommandRequest` read only | Register value/status map | Read returns the cached register map because arbitrary register-read semantics are not evidenced; raw numeric writes are hidden without a named safe control surface | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `exposure` | Camera | `TimeInterval` | s | R/W | 37.983 us..2.489215905 s | Yes | Exposure register sequence; live `0x0b` register writes behind `os-usb`; range comes from the evidenced 37.983 us line time and 16-bit line-count field |
| `gain` | Camera | `Ratio` | percent | R/W | 100..1600 | Yes | Gain register sequence; live `0x0b` register writes behind `os-usb` |
| `pixel_format` | Camera | `String` | none | R/W | `Native`, `Raw8`, `Mono8`, `Rgb8`, `Bgr8` | Yes | Capture/stream encoding state; live source is 8-bit RAW/Bayer |
| `bayer_phase` | Camera | `String` | none | R/W | `Unknown`, `Rggb`, `Grbg`, `Gbrg`, `Bggr` | No | Required for live RGB/BGR software debayering |
| `trigger_mode` | Camera | `String` | none | R/W | `software`, `external`, `bulb` | No | Trigger-mode register state |
| `roi_width` | Camera | `PixelCount` | px | R/W | 64..sensor width | No | ROI state/register fixture |
| `roi_height` | Camera | `PixelCount` | px | R/W | 64..sensor height | No | ROI state/register fixture |
| `binning` | Camera | `I64` | none | R/W | `1`, `2`, `4` | No | Capture geometry state |
| `black_level` | Camera | `I64` | none | R/W | 0..255 | No | Runtime image processing for `Mono8`, `Rgb8`, and `Bgr8`; `Native`/`Raw8` bytes remain unmodified |
| `white_balance_red` | Camera | `Ratio` | percent | R/W | 50..200 | No | Runtime red-channel scaling for `Rgb8`/`Bgr8`; `Native`/`Raw8`/`Mono8` bytes remain unmodified |
| `white_balance_blue` | Camera | `Ratio` | percent | R/W | 50..200 | No | Runtime blue-channel scaling for `Rgb8`/`Bgr8`; `Native`/`Raw8`/`Mono8` bytes remain unmodified |
| `sensor_temperature` | Camera | `Temperature` | named temperature value | R | fixture telemetry | No | Fixture readback |
| `usb_identity` | Camera | `Map` | none | R | configured or fixture USB identity with vendor IDs, image endpoint, and typed `sensor_width`/`sensor_height` pixel counts; live identity adds product, serial when available, VID/PID, bus, and address | No | Clean-room USB descriptor/readback fixture plus live USB descriptor metadata |
| `supported_pixel_formats` | Camera | `List` | none | R | `Native`, `Raw8`, `Mono8`, `Rgb8`, `Bgr8` | No | Capability/readback fixture; `Rgb8`/`Bgr8` are software conversions |
| `feature_summary` | Camera | `Map` | none | R | implemented feature flags and provenance | No | Composite feature readback |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "toupcam"` | Yes | string | Selects the Toupcam-compatible camera provider |
| `property.connect` | No | bool | With `os-usb`, opens the configured Toupcam USB device and uses configured geometry for live RAW8 capture/stream; default `false` uses fixture/configured mode |
| `property.usb_index` | No | `I64` | Zero-based live USB candidate index used when `connect = true`; default `0` |
| `property.sensor_width`, `property.sensor_height` | No | `PixelCount` | Configured sensor/full-frame geometry; defaults to the OpenGEL bench camera geometry |
| `property.roi_width`, `property.roi_height` | No | `PixelCount` | Configured output geometry before binning; defaults to sensor width/height |
| `property.exposure`, `property.gain` | No | `TimeInterval`, `Ratio` | Initial live/control settings applied during configured USB open and later writable through public properties |
| `property.pixel_format`, `property.bayer_phase`, `property.trigger_mode` | No | string | Initial pixel-format, software-debayer phase, and trigger-mode state |

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- discover_devices` | Simulated Toupcam discovery plus config-backed Toupcam geometry/identity alongside other configured driver candidates |
| `cargo run -p numanager-examples -- camera_acquisition` | Typed camera setup, optional source selection, direct `TriggerSink` mode/pulse invocation, timing-plan exposure/gain/pixel-format endpoints, `CameraCapture`, operation-filtered listeners, frame handles |
| `cargo run -p numanager-examples -- camera_stream` | Fixed-capacity frame rings, overflow policies, dropped-frame telemetry, fault reporting |
| `cargo run -p numanager-examples --features os-usb -- camera_acquisition toupcam-live` | Live Toupcam USB open, init replay, exposure/gain control, queued RAW8 capture |
| `cargo run -p numanager-examples --features os-usb -- camera_stream toupcam-live` | Live repeated Toupcam USB bulk frame reads through the runtime frame store |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Add a numanager validation note for live USB control requests, streaming endpoint behavior, pixel formats, and trigger behavior |
| Discovery | Model-specific automatic geometry probing beyond configured geometry and the bench-camera constants |
| Streaming | Zero-copy backend path, cancellation/backpressure, and hardware timestamp support |
| Pixel formats | Confirm the bench camera's Bayer phase on hardware before defaulting RGB/BGR output |
| Protocol | Expand register coverage only from clean sources or traces |
