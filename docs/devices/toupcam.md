# Toupcam-Compatible Cameras

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::toupcam` |
| Families | Toupcam/ToupTek and AmScope-like USB cameras |
| Support level | Config-backed geometry/identity plus live userspace USB backend behind `os-usb` with retained USB identity metadata and local frame source |
| Protocol evidence | OpenGEL clean-room Toupcam backend, per-model captured init sequences promoted as runtime assets, public USB identity/register notes, and existing camera-control behavior as secondary evidence. U3CMOS03100KPA facts come from a USBPcap capture of the vendor application recorded in [`../reverse/toupcam-u3cmos03100kpa.md`](../reverse/toupcam-u3cmos03100kpa.md) |
| Transport | Runtime frame-ring path plus fixture USB-control/raw-register surface; optional `nusb` control and bulk-IN transport for live devices |
| Discovery | Simulated two-stage discovery; config-backed discovery for model geometry/identity; optional live USB discovery through Toupcam/ToupTek/Cypress vendor IDs; live descriptors retain product, serial, VID/PID, bus, and address metadata. A live open resolves a model profile from the USB product id and fails explicitly for product ids with no recorded open sequence |
| Validation | OpenGEL bench path recorded live U3CMOS08500KPA RAW8 frame capture. U3CMOS03100KPA: live open, streaming, and a full 2048x1534 RAW8 frame captured through the numanager runtime on 2026-08-04. The specification-driven open path (probe with token 0, bring-up table, window/timing, computed exposure/gain) replaced the recorded replay and was validated on hardware the same day: whole frames at 1/10/100/500/1500 ms with monotonic response, and gain 100/200/400/800 % reaching the sensor. See [`../reverse/toupcam-protocol.md`](../reverse/toupcam-protocol.md) |
| Runtime/evidence notes | `numanager-drivers/os-usb` for live USB discovery, init replay, exposure/gain control, and RAW8/Mono8 frame capture/stream sized from configured or bench-camera geometry. RGB/BGR output is software debayering and requires configured `bayer_phase`; black-level and white-balance controls are runtime image processing, not USB register writes |

## Model Profiles

Geometry and bring-up are model-specific. Models whose sensor register map is
specified are programmed directly from the interface specification (see
[`../reverse/toupcam-protocol.md`](../reverse/toupcam-protocol.md)); the rest
fall back to replaying a recorded vendor open sequence, which reproduces the
state it was captured at and nothing else. A live open matches the USB product
id against this table and fails explicitly when there is no entry.

| Model | USB id | Full frame | Frame trailer | Bulk frame bytes | Exposure/gain over USB | Init sequence asset |
| --- | --- | --- | --- | --- | --- | --- |
| U3CMOS08500KPA | `0547:13a1` | 3328 x 2548 RAW8 | none | 8 479 744 | Not available — sensor register map not specified | `toupcam_init_seq.jsonl` (681 transfers), replayed verbatim |
| U3CMOS03100KPA | `0547:3310` | 2048 x 1534 RAW8 | 1 byte | 3 141 633 | **Implemented** — computed from the sensor register map | Programmed from the interface specification (no replay) |

Beyond those two, the driver carries the **vendor camera catalogue** of 1337
variants (name, USB product id, full-frame geometry, pixel pitch, preview
resolutions) from the interface specification. A catalogue model with no profile
above cannot be streamed, but it fails at open naming the model and its geometry
instead of hanging until the frame timeout. Look models up by product id, not by
name: one name covers revisions with different ids and pixel pitches.

Frames are delimited by a short bulk transfer; the reader segments on that
delimiter and discards partial segments, so a capture cannot return a frame torn
across two exposures. Any model trailer bytes are consumed and dropped so the
stream does not drift.

Register access carries its operands in the setup packet, masked with a value
derived from a session token the **host** chooses and announces in the probe.
The derivation maps token 0 to mask 0, so this driver always announces 0 and
writes plaintext register numbers and values — there is no masking arithmetic in
the driver at all. Exposure becomes `COARSE_INTEGRATION_TIME` (with
`LINE_LENGTH_PCK` stretched for long exposures) and gain a step ladder to
`ANALOG_GAIN`.

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
| `exposure` | Camera | `TimeInterval` | s | R/W | 37.983 us..2.489215905 s | Yes | Exposure register sequence; live `0x0b` register writes behind `os-usb`; range comes from the evidenced 37.983 us line time and 16-bit line-count field. Live writes compute `COARSE_INTEGRATION_TIME` (0x3012), stretching `LINE_LENGTH_PCK` (0x300C) for long exposures; rejected with `Unsupported` on models with no specified sensor register map (see Model Profiles) |
| `gain` | Camera | `Ratio` | percent | R/W | 100..1600 | Yes | Gain register sequence; live `0x0b` register writes behind `os-usb`. Live writes map the gain through a step ladder to `ANALOG_GAIN` (0x3060); rejected with `Unsupported` on models with no specified sensor register map (see Model Profiles) |
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
| `property.sensor_width`, `property.sensor_height` | No | `PixelCount` | Configured sensor/full-frame geometry. On a live open it defaults to the geometry of the model profile matched from the USB product id; in fixture/configured mode it defaults to the OpenGEL bench camera geometry |
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
| Hardware validation | Add a numanager validation note for live USB control requests, streaming endpoint behavior, pixel formats, and trigger behavior. U3CMOS03100KPA open/stream/capture is validated; its exposure, gain, ROI, binning, and trigger behavior are not |
| Protocol — U3CMOS03100KPA exposure/gain | The `0x0b` register encoding is session-keyed and not decoded, so exposure and gain cannot be set on this model. Needs the key schedule from the vendor `toupcam.dll`, or a sweep large enough to model per-write state. A shippable interim alternative is a set of recorded per-exposure sequences selectable as presets, since replay reproduces the state it was captured at |
| Discovery | Model profiles cover two recorded product ids; other Toupcam product ids fail the open explicitly. Automatic geometry probing (for example from the request `0x20` blob) would remove the need for a per-model capture |
| Streaming | Zero-copy backend path, cancellation/backpressure, and hardware timestamp support |
| Pixel formats | Confirm each model's Bayer phase on hardware before defaulting RGB/BGR output |
| Protocol | Expand register coverage only from clean sources or traces; the request `0x20` calibration blob is only partly mapped |
