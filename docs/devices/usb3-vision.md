# USB3 Vision

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::usb3_vision` |
| Families | USB3 Vision cameras using GenICam transport concepts |
| Support level | U3V command/stream/event model with optional local PGM/PPM frame source plus opt-in USB identity/open/endpoint-catalog and command-endpoint ReadMem/WriteMem path |
| Protocol evidence | USB3 Vision standard concepts and Netpbm PGM/PPM frame formats for local frame sources |
| Transport | U3V control, stream, and event resources; optional local file source feeds generated frames; `property.connect = true` with configured USB VID/PID opens the configured USB device, records descriptor endpoint candidates, claims the configured interface behind `os-usb`, and can issue U3V ReadMem/WriteMem over explicit or single-candidate bulk command endpoints |
| Discovery | Simulated/configured discovery |
| Validation | Configured/local fixture validation plus compiled USB open/descriptor/claim path; real USB3 camera validation pending |
| Runtime/evidence notes | None currently |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `usb3-vision-camera-0` | `camera`, `usb3.vision`, `genicam.transport`, `trigger.sink`, `trigger.source` | One logical camera with control, stream, and event resources |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `usb3-vision-control` | `usb.u3v.control` | U3V control endpoint for memory/register, trigger, and acquisition commands; metadata records configured USB VID/PID/interface, optional serial, descriptor endpoint candidates, selected command endpoints, open/claim state, live identity, and local-vs-configured transport state |
| `usb3-vision-stream` | `usb.u3v.stream` | U3V bulk stream endpoint feeding runtime frame rings and chunk/timestamp metadata |
| `usb3-vision-event` | `usb.u3v.event` | U3V event endpoint for event-channel metadata |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `CameraCapture` | Camera | `CapabilityRequest::CameraCapture` | `CapturedFrame`-parseable frame handle | Runtime completion plus `FrameReady` | Capture participant with acquisition-setting endpoints |
| `CameraStream` | Camera | `CapabilityRequest::CameraStream` with ring-buffer policy | `CameraStreamStarted`-parseable stream id, frame count, pixel format, and frame events | Runtime stream events | Continuous acquisition path with ring buffer |
| `TriggerSink` | Camera | `None` or `CapabilityRequest::Trigger` | U3V trigger status map plus telemetry | Runtime token completion after local U3V memory write | Trigger route endpoint using `AcquisitionStart`/`AcquisitionStop` local writes |
| `TriggerSource` | Camera | `None` or `CapabilityRequest::Trigger` | U3V trigger status map plus telemetry | Runtime token completion after local U3V memory write | Trigger route source using `AcquisitionStart`/`AcquisitionStop` local writes |
| `RawRegisterAccess` | Camera | `GenericCommandRequest` reads by `address` or standard node `node`; writes require a named public node target | Register value/status map with resolved address and node when provided | Runtime token completion | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `width` | Camera | `PixelCount` | px | R/W | 64..8192 | Yes | GenICam width register model |
| `height` | Camera | `PixelCount` | px | R/W | 64..8192 | Yes | GenICam height register model |
| `exposure` | Camera | `TimeInterval` | s | R/W | 10 us..60 s | Yes | GenICam exposure register model |
| `gain` | Camera | `Decibel` | dB | R/W | 0..48 | Yes | GenICam gain register model |
| `pixel_format` | Camera | `String` | none | R/W | `Mono8`, `Mono16`, `BayerRG8`, `Rgb8` | Yes | GenICam pixel-format register model |
| `transfer_size` | Camera | `ByteCount` | bytes | R/W | 16,384..16,777,216 | No | U3V stream transfer setting; scalar bytes accepted as legacy config/write values |
| `transfer_queue_depth` | Camera | `I64` | none | R/W | 1..256 | No | U3V stream queue setting |
| `stream_endpoint` | Camera | `I64` | none | R | 1..15 | No | U3V endpoint metadata |
| `hardware_timestamp` | Camera | `Timestamp` | controller_tick | R | local timestamp | No | U3V/chunk metadata |

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- camera_acquisition usb3` | Generic camera source setup, typed acquisition properties, capture completion, frame handles, and public U3V chunk/timestamp metadata |
| `cargo run -p numanager-examples -- camera_stream usb3` | Generic camera stream workflow with fixed-size frame rings and dropped-frame telemetry |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "usb3_vision"` | Yes | string | Selects config-backed USB3 Vision discovery |
| `property.serial_number` | No | string | Persistent camera serial metadata |
| `property.width` / `property.height` | No | `PixelCount` | Initial local-frame dimensions |
| `property.exposure` | No | `TimeInterval` | Initial exposure; legacy scalar alias `exposure_s` |
| `property.gain` | No | `Decibel` | Initial gain; legacy scalar alias `gain_db` |
| `property.pixel_format` | No | string enum | `Mono8`, `Mono16`, `BayerRG8`, or `Rgb8` |
| `property.transfer_size` | No | `ByteCount` | U3V bulk transfer-size metadata; legacy scalar bytes accepted |
| `property.transfer_queue_depth` | No | integer | U3V fixture queue depth |
| `property.stream_endpoint` | No | integer | U3V stream endpoint metadata |
| `property.fixture_path` | No | string | Optional local Netpbm `P2`, `P3`, `P5`, or `P6` file for capture/stream frame payloads |
| `property.connect` | No | bool | Opens and claims the configured USB device/interface when `os-usb` is enabled |
| `property.usb_vendor_id` / `property.usb_product_id` | Required for active USB open | integer or hex string | USB device identity used when `connect = true` |
| `property.usb_interface` | No | integer | USB interface to claim; defaults to `0` |
| `property.command_in_endpoint` / `property.command_out_endpoint` | No | integer or hex string | Explicit U3V command bulk endpoints; when absent, live command I/O is used only if descriptor discovery finds exactly one bulk IN and one bulk OUT endpoint on the claimed interface |
| `property.command_ack_size` | No | integer bytes | Read buffer for one U3V command ACK; defaults to `4096` |
| `property.command_timeout_ms` | No | integer milliseconds | Command timeout metadata for endpoint bring-up; currently recorded for diagnostics while transfers use the underlying USB backend behavior |
| `property.usb_serial_number` | No | string | Optional serial filter for active USB open |

## Raw Register Bring-Up

`RawRegisterAccess` accepts `GenericCommandRequest` commands `read`,
`ReadRegister`, `read_register`, `ReadMem`, `read_memory`, `write`,
`WriteRegister`, `write_register`, `WriteMem`, and `write_memory`. Reads may
target `address` or `node`; writes require a named public `node` target.
Supported node names are `ManifestTable`,
`DeviceCapability`, `Width`, `Height`, `PayloadSize`, `TimestampControl`,
`TimestampValue`, `AcquisitionStart`, and `AcquisitionStop`. Node targeting is
a local GenICam/SFNC bridge for bring-up and still uses the U3V memory
register path internally.
When `property.connect = true` and configured USB VID/PID are present, the
driver opens the matching USB device, records descriptor endpoint candidates,
and claims the configured interface so the resource metadata can report
descriptor identity, endpoint shape, and claim state. If explicit command
endpoints are configured, or descriptor discovery finds exactly one bulk IN and
one bulk OUT candidate, mapped property writes, trigger writes, mapped timing
writes, and `RawRegisterAccess` use live U3V WriteMem/ReadMem packets and
reject mismatched ACK command ids, request ids, or truncated ACK payloads.
Without a selected command endpoint pair, the same APIs continue to use the
local register model.

## Remaining Work

| Area | Gap |
| --- | --- |
| Transport | Active discovery beyond configured USB identity, stream/event endpoint receive, and broader U3V command-status semantics beyond ACK header validation |
| Local frame formats | Extend local frame decoding beyond PGM/PPM if a documented format is needed |
| GenICam | Connect transport to parsed model-specific XML; current bridge resolves selected standard node names for raw-register bring-up |
| Timing | Trigger-mode validation and hardware timestamp/event behavior |
| Performance | Transfer queue tuning and zero-copy USB receive path |

## Standards And Bring-Up

Standard sources (A3 USB3 Vision, EMVA GenCP/GenApi/SFNC/PFNC), a survey of
reference implementations, the gap-to-document mapping, and a step-by-step
hardware bring-up checklist are recorded in
[`../reverse/usb3-vision-genicam.md`](../reverse/usb3-vision-genicam.md). The
control-channel protocol used here is GenCP, whose specification is freely
available from EMVA; only stream framing and the bootstrap register map require
the gated A3 document.
