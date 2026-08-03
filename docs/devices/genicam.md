# GenICam Node Maps

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::genicam` |
| Families | GenICam/SFNC-style node-map cameras |
| Support level | XML/register node-map execution model with maintenance-command filtering and optional local PGM/PPM frame source |
| Protocol evidence | GenICam node-map concepts, XML-derived nodes, and Netpbm PGM/PPM frame formats for local frame sources |
| Transport | GenICam node-map resource over local register transport; optional local file source feeds generated frames |
| Discovery | Hardcoded local probe and `HardwareConfig`-backed node-map probe |
| Validation | Local XML/register execution; real camera transport validation pending |
| Runtime/evidence notes | None currently |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| Probe label, default `genicam-local-camera` | `camera`, `genicam`, `genicam.node_map`, transport kind | One logical camera with properties derived from the node map |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `<camera>-node-map` | `genicam.node_map` | Parsed node map and register backing store for XML-derived properties and raw register access |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `CameraCapture` | Camera | capture request map | Frame metadata/handle with node-derived chunk/readback fields | Runtime completion plus `FrameReady` | Capture participant |
| `CameraStream` | Camera | `CapabilityRequest::CameraStream` with ring-buffer policy | `CameraStreamStarted`-parseable stream id, frame count, width, height, pixel format, and frame events with node-derived chunk/readback fields | Runtime stream events | Continuous acquisition path |
| `RawRegisterAccess` | Camera | `GenericCommand` reads by `node`, `register`, or `address`/`port`; writes require a named public node target | Register bytes plus decoded node value when a node target is used | Register backing-store accept completion | Useful for transport bring-up, outside timing plans |
| `TriggerSink` | Camera | `None` or `CapabilityRequest::Trigger` | Command-node trigger status map | `AcquisitionStart`/`AcquisitionStop` command-node completion | Trigger route endpoint when acquisition command nodes are present |
| `TriggerSource` | Camera | `None` or `CapabilityRequest::Trigger` | Command-node trigger status map | `AcquisitionStart`/`AcquisitionStop` command-node completion | Trigger route source when acquisition command nodes are present |
| `GenericCommand` | Camera | GenICam command-node request | Command status map | Register/transport accept completion | Maintenance command nodes named like reset, firmware, upload, loader, bootloader, flash, DFU, factory, default, restore, store, program, or file-access/update primitives are hidden; timing plans sequence writable acquisition nodes separately |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| XML-derived node names | Camera | Derived from node kind/access mode | Node unit if present | R/W according to GenICam access mode | Node min/max/inc/enums and dynamic constraints | Depends on node metadata; local probe marks `ExposureTime`, `Gain`, and `AcquisitionFrameRate` sequenceable | Register, masked register, enum pValue, command register, converter/formula, selector/category metadata; maintenance nodes named like reset, firmware, upload, loader, bootloader, flash, DFU, factory, default, restore, store, or program are hidden |
| `node_count` | Descriptor metadata | `I64` | count | R | parsed node count | No | Node-map metadata |
| `command_nodes` | Descriptor metadata | `List` | none | R | command-node names after maintenance filtering | No | Node-map metadata |
| `categories` / `category_tree` | Descriptor metadata | metadata values | none | R | parsed category structure | No | Node-map metadata |
| `ports` / `registers` / `node_metadata` | Descriptor metadata | metadata values | none | R | parsed register/node details; XML `PollingTime` is exposed as typed `polling_time` metadata | No | Node-map metadata |

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- camera_acquisition genicam` | Generic camera source setup, node-map-derived capture capability, frame handles, and public chunk/timestamp metadata |
| `cargo run -p numanager-examples -- camera_stream genicam` | Generic camera stream workflow with runtime frame rings and dropped-frame telemetry |
| `cargo run -p numanager-examples -- discover_devices` | Two-stage discovery including hardcoded and config-backed GenICam node-map candidates |

## Config Discovery

`GenicamDiscovery::from_config(next_id, &config)` claims devices whose
`driver` is `genicam`, `genicam_fixture`, `genicam-fixture`, or
`genicam_node_map`.

| Config key | Type | Meaning |
| --- | --- | --- |
| `label` | string | Optional public camera label; defaults to the configured device label |
| `vendor` | string | Vendor metadata |
| `model` | string | Model metadata |
| `serial_number` / `serial` | string | Serial metadata |
| `transport` | string | `fixture`, `gige_vision`, `usb3_vision`, `camera_link`, or custom kind tag; only local register backing is implemented here |
| `xml` | string | Optional inline GenICam XML; parsed before the candidate is advertised |
| `fixture_path` | string | Optional local Netpbm `P2`, `P3`, `P5`, or `P6` file for capture/stream frame payloads |

## Remaining Work

| Area | Gap |
| --- | --- |
| Transport | Connect node map to real GigE Vision/USB3 Vision register transports |
| Coverage | Expand XML node coverage where unsupported node forms remain |
| Validation | Validate against real vendor XML files and cameras |
| Streaming | Replace local chunk/readback metadata with real transport chunk extraction and stream configuration binding |
| Local frame formats | Extend local frame decoding beyond PGM/PPM if a documented format is needed |
