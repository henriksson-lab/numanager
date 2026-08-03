# Mad City Labs MicroDrive / NanoDrive

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::mcl` |
| Families | MCL MicroDrive and NanoDrive motion controllers |
| Support level | Active USB descriptor discovery plus opt-in MicroDrive USB raw encoder/status readback, fixed-length raw MicroDrive control-read/action commands, and vendor firmware/runtime package identity/file-status/digest-state/probe surface; typed motion is not exposed because units, status meanings, and completion behavior evidence is absent |
| Evidence | Reverse engineered |
| Transport | USB/libusb transport shape, endpoint map, selected request IDs, fixed-length MicroDrive control reads/actions, raw encoder format, status-word bit packing, MicroDrive/NanoDrive VID/PID tables, and pre-firmware USB IDs are known; `os-usb` can enumerate descriptor-only candidates for the evidenced IDs, and `connect=true` can read MicroDrive raw status, encoders, and fixed-length control replies; vendor firmware/runtime packages may be configured as third-party excluded data for explicit on-demand probes; move payload units and completion semantics are not recorded |
| Validation | No numanager hardware validation note |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `mcl-hub` | `hub`, `motion.controller`, `usb.device` | Owns one configured MCL controller reverse engineered support |
| `mcl-x`, `mcl-y`, `mcl-z`, `mcl-axis-4`, `mcl-axis-5` | `stage.axis`, `stage.x/y/z/axis-*`, `reverse.engineered` | Present according to configured `axis_count`; exposes raw encoder counts and two-bit raw status fields only |
| `mcl-xy-stage` | `stage.xy` if evidenced | Not currently exposed; multi-axis logical stage needs units, limits, and completion traces |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `mcl-usb` | `usb.vendor` | Candidate or connected MCL USB transport; resource metadata records configured or descriptor-discovered USB VID/PID, optional descriptor identity, interface, IN endpoint, active-discovery state, and `connected` state plus the protocol note for raw readback request IDs; hub `GenericCommand` exposes raw readback helpers and fixed-length raw MicroDrive control-read/action helpers |
| `mcl-motion-status` | `motion.status` | Not currently exposed because the raw two-bit axis status meaning is not validated |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_status`, `refresh_encoders`, `refresh_8bit_movement_status`, `refresh_move_status`, `refresh_assignments`, `refresh_wait_time`, `refresh_temperature`, `refresh_mode`, `refresh_rotations`, `refresh_mmt_state`, or `stop` with no params | Map containing command, connection state, request ID, raw `wValue`, raw reply bytes when applicable, raw status, and/or encoder summary | Raw readback/control refresh; connected transport issues only documented fixed-length MicroDrive readback request IDs and the stop request; configured mode returns cached/no-reply state | No |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `model` | Hub | `String` | none | R | Header/API exposes product info | No | Configured/vendor-runtime identity metadata |
| `serial_number` | Hub | `String` | none | R | Header/API exposes serial info | No | Configured/vendor-runtime identity metadata |
| `axis_count` | Hub | `I64` | count | R | Configured axis inventory | No | Configured inventory metadata; live axis discovery is not exposed because enumeration evidence is absent |
| `vendor_id`, `product_id` | Hub | `I64` | none | R | Configured or descriptor-discovered USB identity | No | Device selection metadata |
| `usb_identity` | Hub | `Map` | none | R | product, serial when present, family, VID/PID, bus, and address when discovered through `os-usb` | No | USB descriptor metadata only |
| `connected` | Hub | `Bool` | none | R | true when `connect=true` opened the USB device | No | Runtime transport state |
| `raw_status` | Hub | `I64` | none | R | Configured raw status word | No | MicroDrive status request `0xCD` raw word shape |
| `encoder_summary` | Hub | `Map` | none | R | eight raw encoder counters | No | MicroDrive encoder payload, 8 × signed 24-bit little-endian counters |
| `vendor_runtime_path`, `vendor_runtime_sha256` | Hub | `String` | none | R | configured package identity | No | Third-party vendor runtime package |
| `load_vendor_runtime` | Hub | `Bool` | none | R | explicit opt-in runtime-load backend flag; default `false` | No | Configured backend gate |
| `vendor_runtime_file_status` | Hub | `String` | none | R | `not_configured`, `present`, `not_a_file`, or `unavailable:<kind>` | No | Local configured package path check |
| `vendor_runtime_digest_state` | Hub | `String` | none | R | `not_configured`, `invalid_configured_sha256`, `digest_without_path`, `verified`, `mismatch:<actual>`, or `unavailable:<message>` | No | SHA-256 identity check for the configured runtime package |
| `vendor_runtime_file_size` | Hub | `ByteCount` | bytes | R | byte length when configured path is a regular file; `0` when not configured | No | Local configured package path check |
| `vendor_runtime_probe_state` | Hub | `String` | none | R | `disabled`, `missing_sha256`, `invalid_configured_sha256`, `missing_path`, `digest_mismatch`, `digest_unavailable:<message>`, `file_unavailable:<kind>`, `loaded`, or `load_error:<message>` | No | Verifies configured SHA-256, then attempts to load the configured runtime only when `load_vendor_runtime=true`; does not call MCL ABI or hardware APIs |
| `firmware_blob_path`, `firmware_blob_sha256` | Hub | `String` | none | R | configured package identity | No | Third-party firmware package for pre-firmware IDs |
| `read_firmware_blob` | Hub | `Bool` | none | R | explicit opt-in firmware-package read flag; default `false` | No | Configured backend gate |
| `firmware_blob_file_status` | Hub | `String` | none | R | `not_configured`, `present`, `not_a_file`, or `unavailable:<kind>` | No | Local configured package path check |
| `firmware_blob_digest_state` | Hub | `String` | none | R | `not_configured`, `invalid_configured_sha256`, `digest_without_path`, `verified`, `mismatch:<actual>`, or `unavailable:<message>` | No | SHA-256 identity check for the configured firmware package |
| `firmware_blob_file_size` | Hub | `ByteCount` | bytes | R | byte length when configured path is a regular file; `0` when not configured | No | Local configured package path check |
| `firmware_blob_probe_state` | Hub | `String` | none | R | `disabled`, `missing_sha256`, `invalid_configured_sha256`, `missing_path`, `digest_mismatch`, `digest_unavailable:<message>`, `readable:<bytes>`, or `read_error:<message>` | No | Verifies configured SHA-256, then reads at most the first 4096 bytes only when `read_firmware_blob=true`; does not upload firmware |
| `package_strategy` | Hub | `String` | none | R | interim package policy | No | Runtime support metadata |
| `vendor_runtime_state` | Hub | `String` | none | R | `not_configured`, `configured_without_digest`, `configured_with_digest`, or `digest_without_path` | No | Derived from configured package identity |
| `firmware_package_state` | Hub | `String` | none | R | `not_required_for_configured_pid`, `not_configured`, `configured_without_digest`, `configured_with_digest`, or `digest_without_path` | No | Derived from configured firmware package identity and USB PID |
| `raw_encoder_count` | Axis | `ControllerScalar` | controller_step | R | configured raw counter | No | Raw encoder counter selected by configured axis index |
| `status_bits` | Axis | `I64` | none | R | `0..3` raw two-bit field | No | Raw status word, two bits per axis |
| `position_gate` | Axis | `String` | none | R | evidence-gate explanation | No | Encoder-to-position scaling not evidenced |
| `motion_gate` | Axis | `String` | none | R | evidence-gate explanation | No | Move payloads, units, limits, and completion not evidenced |

## Evidence Gate

| Claim | Current evidence | Default driver decision |
| --- | --- | --- |
| Transport | Reverse engineered protocol note records libusb, USB setup layout, global/per-axis bulk endpoints, VID/PID table, and selected request IDs | `os-usb` descriptor discovery lists evidenced MicroDrive/NanoDrive/pre-firmware IDs without opening them; `connect=true` opens a matching MicroDrive for raw readback only |
| Identity/axis discovery | Headers expose product, serial, firmware, calibration, axis info, and handle APIs | Configured descriptors only when probe/calibration packets are not known |
| Raw encoder/status/control reads | Protocol note records MicroDrive encoder payload as 8 signed 24-bit little-endian counters, status as two bits per axis, and fixed-length request IDs for movement status, assignments, wait time, temperature, mode, rotations, MMT state, stop, and encoder reset actions | Expose configured counters/status; when connected, refresh MicroDrive raw status and encoder counts on read or through hub commands; expose fixed-length replies as raw bytes and refresh raw readbacks after explicit stop only; keep encoder reset hidden from regular and advanced command surfaces; do not convert to position, temperature, wait duration, rotation semantics, or limit/fault state |
| Firmware/runtime package | Protocol note records two Cypress pre-firmware IDs; no project-owned firmware exists yet | Expose configured vendor firmware/runtime package identity, local file status, SHA-256 digest state, and explicit opt-in probe state after digest verification only; do not upload firmware before the correct image, target PID transition, and loader sequence are evidenced |
| Motion commands | Headers expose absolute/relative/single-step/three-axis moves, wait, status, and sequence APIs | Stage move/home APIs are not exposed because move payloads, units, and status polling are not evidenced; the fixed-length raw stop request documented above is exposed |
| Units/calibration | Headers expose calibration and encoder/current-position APIs | Do not expose writable motion scaling without counts-to-position evidence |
| Completion/faults | Headers expose move status/wait/status APIs | Runtime completion cannot be hardware-owned when busy/error/status bit layout is not known |

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- discover_devices` | Configured MCL reverse-engineered support discovery, plus `os-usb` descriptor discovery when matching hardware is present, with raw status/encoder metadata, raw command metadata, and vendor firmware/runtime package boundary; no stage motion capability is advertised |

## Configuration

| Field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `property.product`, `property.serial_number`, `property.family` | No | string | Configured descriptor metadata |
| `property.axis_count` | No | integer | `0..=5`; creates that many axis descriptors |
| `property.connect` | No | bool | Open a real USB device for MicroDrive raw readback when `os-usb` is enabled |
| `property.vendor_id`, `property.product_id`, `property.interface`, `property.in_endpoint` | No | integer | USB selection and endpoint metadata; active descriptor discovery fills VID/PID from matched hardware, while live readback currently accepts MicroDrive PIDs only |
| `property.raw_status` | No | integer | Seed raw status word |
| `property.encoder_count_1..8` | No | integer | Seed raw encoder counters |
| `property.vendor_runtime_path`, `property.vendor_runtime_sha256` | No | string | Third-party vendor runtime package identity; empty or `none` means not configured |
| `property.firmware_blob_path`, `property.firmware_blob_sha256` | No | string | Third-party firmware package identity for pre-firmware devices; empty or `none` means not configured |
| `property.load_vendor_runtime` | No | bool | Enables the explicit vendor-runtime loadability probe after SHA-256 verification. Default `false`; ordinary discovery does not load third-party code |
| `property.read_firmware_blob` | No | bool | Enables the explicit firmware-package readability probe after SHA-256 verification. Default `false`; ordinary discovery does not read firmware files |

## Remaining Work

| Area | Gap |
| --- | --- |
| USB protocol | Active descriptor discovery is implemented for evidenced VID/PID tables; need live endpoint/interface validation plus motion payload structures, axis addressing, and status-bit meanings |
| Raw USB readback | Validate the opt-in raw readback path against hardware and record interface/endpoint behavior |
| Units | Need calibration mapping for counts/um and per-axis ranges |
| Completion | Need busy/status/error polling evidence |
| Driver | Typed motion/home capabilities are not exposed because move, stop, status, units, and completion behavior evidence is absent |

## Unblock Trace Checklist

Use the USB vendor/bulk section of
[`../reverse/trace-capture-guide.md`](../reverse/trace-capture-guide.md) when
collecting these observations.

| Trace item | Must record |
| --- | --- |
| Hardware identity | MicroDrive/NanoDrive model, firmware/library version, serial number, axis count, calibration values, limits, and OS USB descriptor identity |
| Transport layout | USB endpoints, control/bulk transfer directions, request IDs, payload lengths, and any kernel/libusb driver binding details |
| Discovery/calibration | Raw traffic for handle/open, identity, axis info, calibration, current position readback, and the matching discovered-device/runtime property output |
| Motion | Raw traffic for a small absolute or relative move on one axis, including requested position in typed units, encoded payload values, and the matching move-completion output |
| Completion | Busy/status polling or completion frames before, during, and after the move, including stop behavior if available and the runtime event sequence for the same operation |
| Fault path | Limit, invalid-axis, stop, disconnect, or documented error status traffic plus failed-operation output sufficient to map runtime `fault` and failed operation reports |
