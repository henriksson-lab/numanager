# Trinamic TMCL

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::trinamic_tmcl` |
| Families | Trinamic/ADI TMCL direct-mode stepper controllers, including configured serial/USB direct-mode modules such as TMCM-3212-TMCL |
| Support level | Spec-backed startup/runtime-refresh over configured serial with configured serial resource metadata |
| Protocol evidence | ADI TMCM-3212 product page documents TMCL firmware and USB/RS485 interfaces; official TMCM-3212 TMCL firmware manual documents direct-mode binary frames, checksum, replies, `MVP`, `MST`, `SAP`, `GAP`, axis parameters, and control command 136 type 1 for raw binary firmware-version readback |
| Transport | Configured serial-style direct mode, default 9600 baud, 8 data bits, no parity, 1 stop bit; module-specific baud is config-backed |
| Discovery | Config-backed two-stage discovery; optional configured real serial construction behind `os-serial` refreshes documented raw firmware-version and axis parameters before registration |
| Validation | No hardware validation |
| Runtime/evidence notes | `os-serial` for real configured serial ports |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `trinamic-tmcl-hub` | `hub`, `motion.controller`, `trinamic.tmcl` | Owns one configured TMCL direct-mode controller address |
| `trinamic-tmcl-<axis>-stage` | `stage.1d`, `motion.stage`, `state.device`, `trinamic.tmcl.axis` | One logical stage per configured TMCL axis; all axes remultiplex through the same controller resource |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| Controller direct-mode channel | `serial` | Sends 9-byte TMCL direct-mode frames and receives 9-byte replies; resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `StageMove` | Axis stage | `CapabilityRequest::StageMove` | Axis state map | Sends `MVP ABS` or `MVP REL`, then polls `GAP` axis parameters for actual position, target position, actual speed, and position-reached flag until the controller reports reached plus zero speed | `position` and `target` properties are sequenceable; runtime timing-plan start/stop applies first/last endpoints through the same `MVP`/`GAP` path; hardware-timed synchronized motion needs controller-specific traces |
| `StageStop` | Axis stage | `None` | Axis state map | Sends `MST`, then refreshes position/speed/reached state from `GAP` | Not sequenceable |
| `GenericCommand` | Axis stage | `refresh_readbacks`, `refresh_motion`, `refresh_profile`, or `refresh_switches` with no params | Axis state map | Runtime token after mapped `GAP` readbacks | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `model` | Hub | `String` | none | R | configured model | No | Config/probe metadata |
| `serial_number` | Hub | `String` | none | R | configured serial | No | Config/probe metadata until identity probing is model-specific |
| `firmware_version_raw` | Hub | `I64` | native reply value | R | configured or connected readback | No | TMCL control command 136, type 1; raw binary value only |
| `protocol` | Hub | `String` | none | R | `TMCL direct-mode binary` | No | Protocol metadata |
| `module_address` | Hub | `I64` | address | R | `0..255` | No | Frame byte 0 |
| `host_address` | Hub | `I64` | address | R | `0..255` | No | Reply byte 0 expectation |
| `baud_rate` | Hub | `I64` | baud | R | configured positive integer | No | Configured serial endpoint |
| `last_transaction` | Hub | `Map` | none | R | opcode, type, axis, value, status, reply value, completion basis | No | Runtime transaction cache for trace notes |
| `axis` | Axis | `String` | none | R | `x`, `y`, `z`, or configured custom label | No | Configured axis map |
| `axis_index` | Axis | `I64` | index | R | `0..255` | No | TMCL motor/bank byte |
| `position` | Axis | `Position` | configured physical unit | R/W | `0..travel` from config | Yes | `GAP 1` read, `MVP ABS` write |
| `target` | Axis | `Position` | configured physical unit | R/W | `0..travel` from config | Yes | `GAP 0` read, `MVP ABS` write |
| `actual_steps` | Axis | `StepCount` | steps | R | signed 32-bit controller microsteps | No | `GAP 1` |
| `target_steps` | Axis | `StepCount` | steps | R | signed 32-bit controller microsteps | No | `GAP 0` |
| `step_size` | Axis | `Position` | physical distance per microstep | R | configured | No | Configured conversion boundary |
| `travel` | Axis | `Position` | physical distance | R | configured | No | Configured safety range |
| `actual_speed` | Axis | `ControllerScalar` | controller steps | R | controller-specific `pps` scalar | No | `GAP 3` |
| `max_positioning_speed` | Axis | `ControllerScalar` | controller steps | R/W | `0..7999774` | Yes | `GAP/SAP 4` |
| `max_acceleration` | Axis | `ControllerScalar` | controller steps | R/W | `117..7629278` | Yes | `GAP/SAP 5` |
| `busy` | Axis | `Bool` | none | R | none | No | Derived from `GAP 8` position reached and `GAP 3` actual speed |
| `position_reached` | Axis | `Bool` | none | R | none | No | `GAP 8` |
| `home_switch` | Axis | `Bool` | none | R | none | No | `GAP 9` |
| `left_limit_switch` | Axis | `Bool` | none | R | none | No | `GAP 11` |
| `right_limit_switch` | Axis | `Bool` | none | R | none | No | `GAP 10` |
| `state_summary` | Axis | `Map` | none | R | position, target, steps, speed, busy, switches | No | Runtime status cache plus `GAP` refresh when read |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "trinamic_tmcl"`, `"trinamic-tmcl"`, or `"tmcl"` | Yes | string | Selects the TMCL discovery provider |
| `property.model` | No | string | Controller model label |
| `property.serial_number` | No | string | Persistent controller serial label |
| `property.firmware_version_raw` | No | `I64` | Configured raw binary firmware-version value before live readback |
| `property.module_address` | No | `I64` | TMCL module address; default `1` |
| `property.host_address` | No | `I64` | Expected reply host address; default `2` |
| `property.axes` | No | `I64` | Number of logical axes; default `1` |
| `property.step_size` | No | `Position` | Physical distance per controller microstep; legacy scalar alias `step_size_um` |
| `property.travel` | No | `Position` | Logical travel range; legacy scalar alias `travel_um` |
| `property.max_positioning_speed` | No | `I64` | Initial controller scalar for `SAP/GAP 4`; default `51200` |
| `property.max_acceleration` | No | `I64` | Initial controller scalar for `SAP/GAP 5`; default `10000` |
| `property.serial_port` | No | string | Opt-in real serial port; without `connect=true`, the configured state model remains active |
| `property.connect` | No | `Bool` | If true with `serial_port`, open a real serial transport and refresh documented `GAP` axis parameters before registration |
| `property.baud_rate` | No | `I64` | Serial baud rate; default `9600` |
| `property.serial_timeout_ms` | No | integer | Serial read timeout for configured real ports |
| `property.completion_poll_limit` | No | integer | Maximum reply/status polls for moves and reads |

Present TMCL config keys with malformed types or out-of-range topology,
address, baud-rate, or completion-poll values are rejected instead of silently
falling back to configured defaults.

Axis `GenericCommand` accepts only the named read-only refresh helpers listed
above. It does not expose raw TMCL opcodes, parameter numbers, direct frame
payloads, homing/reference-search commands, or serial discovery.

## Examples

| Example | Demonstrates |
| --- | --- |
| `discover_devices` | Shows configured Trinamic TMCL controller and demultiplexed axis stages in the two-stage discovery flow |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate configured serial settings, startup `GAP` refresh, 9-byte frames, checksums, status codes, `MVP`, `MST`, `GAP`, `SAP`, and completion polling against real TMCL hardware |
| Generic motion workflow | `cargo run -p numanager-examples -- motion_stage trinamic-tmcl` covers the public motion workflow; hardware-validation notes should record target, completion/readback, stop, and limit/fault state |
| Identity probing | Raw binary firmware-version readback is implemented with control command 136 type 1; model/serial probing and string-format firmware parsing still need module-specific evidence |
| Homing/reference search | `RFS` is documented but module-specific switch-mode and safe-behavior evidence is needed before exposing `StageHome` |
| Motion profiles | Physical velocity/acceleration conversion needs module and microstep timing validation; current `ControllerScalar` properties avoid pretending the scalar is a universal SI unit |
| Multi-axis coordination | Batch/remux and runtime start/stop endpoint handling apply mapped per-axis writes through the same `MVP`/`GAP` paths; synchronized controller-side multi-axis moves need hardware-backed queue/timing evidence |
| Output/readback | Bench notes must include command stdout/stderr, requested target, controller completion/readback, final position, and any limit/fault state |
