# GigE Vision

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::gige_vision` |
| Families | GigE Vision cameras using GenICam transport concepts |
| Support level | GVCP/GVSP command/frame model with optional local PGM/PPM frame source plus opt-in UDP GVCP mapped-property and raw-register control |
| Protocol evidence | GigE Vision GVCP/GVSP standards concepts and Netpbm PGM/PPM frame formats for local frame sources |
| Transport | GVCP control resource and GVSP stream resource; optional local file source feeds generated frames; `property.connect = true` with configured `property.camera_address` enables UDP GVCP for mapped `width`, `height`, `packet_size`, and `stream_channel_port` writes plus raw-register reads/writes |
| Discovery | Simulated/configured discovery |
| Validation | Configured/local fixture validation plus compiled UDP GVCP control path; real NIC/camera validation pending |
| Runtime/evidence notes | None currently |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `gige-vision-camera-0` | `camera`, `gige.vision`, `genicam.transport`, `trigger.sink`, `trigger.source` | One logical camera with separate control and stream resources |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `gige-vision-gvcp` | `udp.gvcp` | GVCP control path for register, trigger, discovery, and acquisition commands; metadata records configured `camera_address`, `connected`, `gvcp_timeout`, and local-vs-UDP transport state |
| `gige-vision-gvsp` | `udp.gvsp` | GVSP image stream path feeding runtime frame rings and chunk/timestamp metadata |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `CameraCapture` | Camera | `CapabilityRequest::CameraCapture` | `CapturedFrame`-parseable frame handle | Runtime completion plus `FrameReady` | Capture participant with acquisition-setting endpoints |
| `CameraStream` | Camera | `CapabilityRequest::CameraStream` with ring-buffer policy | `CameraStreamStarted`-parseable stream id, frame count, pixel format, and frame events | Runtime stream events | Continuous acquisition path with ring buffer |
| `TriggerSink` | Camera | `None` or `CapabilityRequest::Trigger` | GVCP trigger status map plus telemetry | Runtime token completion after local GVCP write | Trigger route endpoint using `AcquisitionStart`/`AcquisitionStop` local writes |
| `TriggerSource` | Camera | `None` or `CapabilityRequest::Trigger` | GVCP trigger status map plus telemetry | Runtime token completion after local GVCP write | Trigger route source using `AcquisitionStart`/`AcquisitionStop` local writes |
| `RawRegisterAccess` | Camera | `GenericCommandRequest` reads by `address` or standard node `node`; writes require a named public node target | Register value/status map with resolved address and node when provided; configured UDP mode also returns ACK command/status/payload metadata | Local runtime token completion or UDP GVCP ACK validation when `connect = true` and `camera_address` is configured | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `width` | Camera | `PixelCount` | px | R/W | 64..8192 | Yes | GenICam width register model |
| `height` | Camera | `PixelCount` | px | R/W | 64..8192 | Yes | GenICam height register model |
| `exposure` | Camera | `TimeInterval` | s | R/W | 10 us..60 s | Yes | GenICam exposure register model |
| `gain` | Camera | `Decibel` | dB | R/W | 0..48 | Yes | GenICam gain register model |
| `pixel_format` | Camera | `String` | none | R/W | `Mono8`, `Mono16`, `BayerRG8`, `Rgb8` | Yes | GenICam pixel-format register model |
| `packet_size` | Camera | `ByteCount` | bytes | R/W | 576..9000 | No | GVSP packet-size setting; scalar bytes accepted as legacy config/write values |
| `inter_packet_delay` | Camera | `TimeInterval` | s | R/W | 0..1 ms | No | GVSP pacing setting |
| `stream_channel_port` | Camera | `I64` | none | R/W | 1024..65535 | No | GVSP stream channel port |
| `hardware_timestamp` | Camera | `Timestamp` | controller_tick | R | local timestamp | No | GVSP/chunk metadata |

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- camera_acquisition gige` | Generic camera source setup, typed acquisition properties, capture completion, frame handles, and public GVSP chunk/timestamp metadata |
| `cargo run -p numanager-examples -- camera_stream gige` | Generic camera stream workflow with runtime frame rings and dropped-frame telemetry |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "gige_vision"` | Yes | string | Selects config-backed GigE Vision discovery |
| `property.serial_number` | No | string | Persistent camera serial metadata |
| `property.width` / `property.height` | No | `PixelCount` | Initial local-frame dimensions |
| `property.exposure` | No | `TimeInterval` | Initial exposure; legacy scalar alias `exposure_s` |
| `property.gain` | No | `Decibel` | Initial gain; legacy scalar alias `gain_db` |
| `property.pixel_format` | No | string enum | `Mono8`, `Mono16`, `BayerRG8`, or `Rgb8` |
| `property.packet_size` | No | `ByteCount` | GVSP packet-size setting; legacy scalar bytes accepted |
| `property.inter_packet_delay` | No | `TimeInterval` | GVSP pacing; legacy scalar alias `inter_packet_delay_ns` remains accepted |
| `property.stream_channel_port` | No | integer | GVSP stream channel port |
| `property.fixture_path` | No | string | Optional local Netpbm `P2`, `P3`, `P5`, or `P6` file for capture/stream frame payloads |
| `property.camera_address` | Required for active GVCP | string | GigE Vision camera host/IP for opt-in UDP GVCP raw-register reads/writes on port 3956 |
| `property.connect` | No | bool | Enables the configured UDP GVCP raw-register path when `camera_address` is present |
| `property.gvcp_timeout` | No | `TimeInterval` | UDP GVCP ACK timeout; legacy scalar alias `gvcp_timeout_ms` remains accepted |

## Raw Register Bring-Up

`RawRegisterAccess` accepts `GenericCommandRequest` commands `read`,
`ReadRegister`, `read_register`, `write`, `WriteRegister`, and
`write_register`. Reads may target `address` or `node`; writes require a named public `node` target.
Supported node names
are `DeviceMode`, `Width`, `Height`, `PayloadSize`, `TimestampControl`,
`TimestampHigh`, `TimestampLow`, `TimestampValue`, `AcquisitionStart`, and
`AcquisitionStop`. Node targeting is a local GenICam/SFNC bridge for
bring-up and still uses the GVCP register path internally.
When `property.connect = true` and `property.camera_address` is configured,
mapped `width`, `height`, `packet_size`, and `stream_channel_port` writes plus
raw-register requests send GVCP WriteReg/ReadReg packets over UDP port 3956.
ACK parsing validates the command, status, request ID, and read-payload length
before returning a value or updating local state. Camera capture and stream
capabilities remain local-frame backed; live GVSP packet reception needs
transport evidence.

## Remaining Work

| Area | Gap |
| --- | --- |
| Transport | Broadcast discovery, typed camera-control properties over live GVCP, and GVSP packet receive/reassembly |
| Local frame formats | Extend local frame decoding beyond PGM/PPM if a documented format is needed |
| GenICam | Connect transport to parsed model-specific XML; current bridge resolves standard node names for raw-register bring-up |
| Timing | Trigger-mode validation and PTP/hardware timestamp behavior |
| Performance | NIC tuning, jumbo frames, packet resend, and zero-copy receive path |

## Standards And Bring-Up

Standard sources, a survey of reference implementations, the gap-to-document
mapping, and a hardware bring-up checklist shared with the USB3 Vision target are
recorded in [`../reverse/usb3-vision-genicam.md`](../reverse/usb3-vision-genicam.md).
Note that `GvspBlockReassembler` in this module already implements leader/payload/
trailer reassembly and is not yet constructed anywhere; the remaining GVSP work is
the socket receive path that feeds it, not the reassembly itself.
