# Platform Camera

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::platform_camera` |
| Families | OS camera backends such as V4L2, GStreamer, DirectShow, and local fixture capture |
| Support level | Runtime frame capture/stream fixture with optional local PGM/PPM file source, descriptor-only Linux V4L2 discovery, and explicit configured V4L2 `read()` capture/stream for fixed-size raw frames |
| Protocol evidence | Platform backend concepts, Linux V4L2 sysfs device descriptors and read-based device API, and Netpbm PGM/PPM file formats for local frame sources |
| Transport | Fixture/local file source feeding runtime frame rings; descriptor-only V4L2 discovery; explicit configured V4L2 `read()` frame source; GStreamer/DirectShow capture paths need backend-specific evidence |
| Discovery | Simulated discovery, config-backed/platform backend discovery, and non-invasive Linux V4L2 descriptor scanning from `/sys/class/video4linux` |
| Validation | Configured/local fixture validation plus explicit V4L2 read-path compile checks; real OS backend validation pending |
| Runtime/evidence notes | Descriptor-only OS cameras with no fixture source do not advertise capture/stream/trigger capabilities unless `backend = "v4l2"`, `device_path`, and `connect = true` are explicitly configured |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `platform-camera-*` | `camera`, `platform.camera`, optional `trigger.sink`, optional `trigger.source` | One logical camera device per backend source; descriptor-only OS cameras advertise no capture/stream/trigger capabilities unless an explicit V4L2 read source or fixture source is configured |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `<camera>-stream` | `camera.<backend>` | Backend stream resource feeding the shared frame store and runtime frame rings |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `CameraCapture` | Fixture/simulated/local-PNM camera or explicit V4L2 read camera | `CapabilityRequest::CameraCapture` | `CapturedFrame`-parseable frame handle | Runtime completion plus `FrameReady` | Capture participant |
| `CameraStream` | Fixture/simulated/local-PNM camera or explicit V4L2 read camera | `CapabilityRequest::CameraStream` with ring-buffer policy | `CameraStreamStarted`-parseable stream id, frame count, pixel format, and frame events | Runtime stream events | Continuous acquisition path |
| `TriggerSink` | Fixture/simulated/local-PNM camera | `None` or `CapabilityRequest::Trigger` | Trigger status map plus telemetry | Runtime token completion after fixture backend ack | Trigger route endpoint and software pulse fixture |
| `TriggerSource` | Fixture/simulated/local-PNM camera | `None` or `CapabilityRequest::Trigger` | Trigger status map plus telemetry | Runtime token completion after fixture backend ack | Trigger route source and exposure-output fixture |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `exposure` | Camera | `TimeInterval` | s | R/W | 0.1 ms..10 s | Yes | Backend exposure control |
| `gain` | Camera | `Ratio` | percent | R/W | 0..800 | Yes | Backend gain control |
| `pixel_format` | Camera | `String` | none | R/W | `Native`, `Mono8`, `Mono16`, `Rgb8`, `Bgr8`, `Yuyv`, `Mjpeg` | Yes | Backend format negotiation |
| `frame_interval` | Camera | `TimeInterval` | s | R/W | 1 ms..60 s | Yes | Backend frame-rate control |
| `width` | Camera | `PixelCount` | px | R | 1..8192 | No | Active format metadata |
| `height` | Camera | `PixelCount` | px | R | 1..8192 | No | Active format metadata |
| `active_format` | Camera | `Map` | none | R | backend-specific | No | Descriptor/readback metadata |
| `supported_formats` | Camera | `List` | none | R | backend-specific | No | Descriptor/readback metadata |
| `backend` | Camera | `String` | none | R | `fixture`, `v4l2`, `gstreamer`, or `directshow` | No | Configured or descriptor-discovered backend |
| `device_path` | Camera | `String` | none | R | descriptor-discovered/configured OS path such as `/dev/video0` | No | Runtime descriptor metadata |
| `device_name` | Camera | `String` | none | R | descriptor-discovered/configured OS device name when available | No | Runtime descriptor metadata |
| `connect` | Camera | `Bool` | none | R | explicit configured live-I/O gate | No | V4L2 read capture requires `true` |
| `capture_gate` | Camera | `String` | none | R | fixture frame source availability or descriptor-only OS backend gate | No | Runtime support metadata |

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- camera_acquisition` | Generic camera source setup, typed acquisition properties, capture completion, and frame handles |
| `cargo run -p numanager-examples -- camera_stream` | Generic camera stream workflow with Fixed-capacity frame rings and dropped-frame telemetry |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "platform_camera"` | Yes | string | Selects config-backed platform-camera discovery |
| `property.backend` | No | string | `fixture`, `v4l2`, `gstreamer`, or `directshow`; defaults to `fixture` |
| `property.width` / `property.height` | No | `PixelCount` | Configured active fixture dimensions |
| `property.exposure` | No | `TimeInterval` | Initial exposure; legacy scalar alias `exposure_s` |
| `property.gain` | No | `Ratio` | Initial gain as percent/fraction; legacy scalar alias `gain_percent` |
| `property.pixel_format` | No | string enum | Initial format; accepts the advertised platform pixel-format names |
| `property.frame_interval` | No | `TimeInterval` | Initial frame interval; legacy scalar alias `frame_interval_s` |
| `property.fixture_path` | No | string | Optional local Netpbm `P2`, `P3`, `P5`, or `P6` fixture file; when absent, fixture/simulated frames come from the biological gel-scene generator |
| `property.device_path` | No | string | Optional OS camera device path recorded as metadata; for `backend = "v4l2"` plus `connect = true`, the driver reads fixed-size raw frames from this path using the V4L2 read API |
| `property.device_name` | No | string | Optional OS camera display name recorded as metadata |
| `property.connect` | No | bool | Explicit live-I/O gate for V4L2 read capture; default `false` |

## Remaining Work

| Area | Gap |
| --- | --- |
| Backends | Linux V4L2 descriptor discovery is non-invasive, and explicit configured V4L2 `read()` capture supports fixed-size raw `Mono8`, `Mono16`, `Yuyv`, `Rgb8`, or `Bgr8` frames; mmap/streaming ioctls, format negotiation, GStreamer, DirectShow, and variable-length MJPEG capture need backend-specific evidence |
| Fixture formats | Extend local fixture decoding beyond PGM/PPM if a documented format is needed |
| Discovery | Expand descriptor discovery beyond Linux V4L2 and add backend-native format enumeration |
| Timing | Validate trigger input/output semantics per backend |
| Streaming | Hardware timestamps and backend-native buffer ownership |
