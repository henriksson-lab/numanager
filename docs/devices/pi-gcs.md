# Physik Instrumente GCS/GCS2

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::pi_gcs` |
| Families | PI GCS/GCS2 controllers |
| Support level | Configured opt-in serial control/readback for GCS motion, servo/profile/reference/status queries, and refresh helpers |
| Protocol evidence | Public GCS command concepts |
| Transport | LF-terminated ASCII over `SerialIo` |
| Discovery | Config-backed discovery; live serial requires configured endpoint and explicit connect |
| Validation | Configured serial startup-readback/control path is implemented; real hardware validation pending |
| Runtime/evidence notes | `numanager-drivers/os-serial` for configured real serial ports |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `pi-gcs-hub` | `hub`, `motion.controller`, `serial.ascii` | Owns one serial resource |
| `pi-gcs-xy-stage` | `axis.xy`, `stage.xy` | X/Y writes coalesce into one `MOV X ... Y ...` transaction |
| `pi-gcs-z-stage` | `axis.z`, `stage.z` | Shares serial resource with XY |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `pi-gcs-serial` | `serial` | LF-terminated GCS command path shared by XY/Z motion, servo/profile, reference, status, and error queries; resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `StageMove` | XY and Z stage | `CapabilityRequest::StageMove` | Map with moved axes/profile metadata | Moving-status byte or `ONT?`, `POS?`, and `ERR?` readback when available | X/Y/Z position sequences; servo endpoints are software-sequenceable through `SVO` |
| `StageHome` | XY and Z stage | `None` | Reference status string plus property events | Moving-status byte or `ONT?`, `POS?`, and `ERR?` readback when available | Not sequenceable |
| `StageStop` | XY and Z stage | `None` | Halt status string plus property events | `HLT` or `STP` followed by moving-status/`ONT?`, `POS?`, and `ERR?` readback when available | Not sequenceable |
| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_identity`, `refresh_status`, `refresh_position`, `refresh_profiles`, or `refresh_servo` with no params | Map with command count and state summary | Uses only mapped GCS query readbacks; no arbitrary serial command surface | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `controller_id` | Hub | `String` | none | R | none | No | `*IDN?` / configured probe |
| `syntax_version` | Hub | `F64` | none | R | none | No | `CSV?` / configured probe |
| `busy` | Hub/stages | `Bool` | none | R | none | No | Moving-status byte or `ONT?` |
| `last_error` | Hub | `String` | none | R | raw controller code | No | `ERR?` readback |
| `fault` | Hub | `Bool` | none | R | derived from `ERR?` | No | `ERR?` readback |
| `state_summary` | Hub | `Map` | none | R | controller features, last error, fault, and all X/Y/Z state | No | Composite controller/device state; busy/error query updates before summary read |
| `x` | XY stage | `Position` | um | R/W | `x_travel` | Yes | `POS? X`, `MOV X`, `MVR X` |
| `y` | XY stage | `Position` | um | R/W | `y_travel` | Yes | `POS? Y`, `MOV Y`, `MVR Y` |
| `z` | Z stage | `Position` | um | R/W | `z_travel` | Yes | `POS? Z`, `MOV Z`, `MVR Z` |
| `speed_x` | XY stage | `Velocity` | um/s | R/W | configured range | No | `VEL? X`, `VEL X` |
| `speed_y` | XY stage | `Velocity` | um/s | R/W | configured range | No | `VEL? Y`, `VEL Y` |
| `speed` | Z stage | `Velocity` | um/s | R/W | configured range | No | `VEL? Z`, `VEL Z` |
| `acceleration_x` | XY stage | `Acceleration` | um/s^2 | R/W if supported | configured range | No | `ACC? X`, `ACC X` |
| `acceleration_y` | XY stage | `Acceleration` | um/s^2 | R/W if supported | configured range | No | `ACC? Y`, `ACC Y` |
| `acceleration` | Z stage | `Acceleration` | um/s^2 | R/W if supported | configured range | No | `ACC? Z`, `ACC Z` |
| `servo_x` | XY stage | `Bool` | none | R/W if supported | none | Yes | `SVO? X`, `SVO X` |
| `servo_y` | XY stage | `Bool` | none | R/W if supported | none | Yes | `SVO? Y`, `SVO Y` |
| `servo` | Z stage | `Bool` | none | R/W if supported | none | Yes | `SVO? Z`, `SVO Z` |
| `referenced_x` | XY stage | `Bool` | none | R | none | No | Active probe/config state; set true after `FRF X` |
| `referenced_y` | XY stage | `Bool` | none | R | none | No | Active probe/config state; set true after `FRF Y` |
| `referenced` | Z stage | `Bool` | none | R | none | No | Active probe/config state; set true after `FRF Z` |

## Metadata And Config

| Key | Scope | Type | Status | Meaning |
| --- | --- | --- | --- | --- |
| `x_travel` | XY metadata, config | `Position` | Canonical | X travel range used for clamping and property ranges |
| `y_travel` | XY metadata, config | `Position` | Canonical | Y travel range used for clamping and property ranges |
| `z_travel` | Z metadata, config | `Position` | Canonical | Z travel range used for clamping and property ranges |
| `default_unit_size` | XY/Z metadata, state summary | `Position` | Canonical | Physical size of one PI default controller unit |
| `x_travel_um`, `y_travel_um`, `z_travel_um` | Config | `F64`/`I64` micrometers | Legacy alias | Accepted for older configs |
| `legacy_travel_x_um`, `legacy_travel_y_um`, `legacy_travel_z_um` | XY/Z metadata | `Position` | Legacy marker | Compatibility label for former travel metadata names |
| `legacy_default_unit_size_um` | XY/Z metadata | `Position` | Legacy marker | Compatibility label for former default-unit metadata name |
| `um_to_default_unit` | Config | `F64` | Native calibration | Controller conversion factor used only at the wire boundary |

When `connect = true`, discovery opens the configured serial endpoint, runs the
configured startup-readback script, and seeds cached identity, syntax, axes, servo state,
position, velocity, acceleration, and busy state from controller replies before
registering the driver.
Runtime property reads request and ingest the mapped query reply before
returning cached state. Writable motion, reference, and stop paths request
mapped busy, position, and `ERR?` readbacks after command writes while retaining
cached configured state when no live reply is available. Servo, velocity, and
acceleration writes consume `ERR?` immediately after the command and fail the
operation when the controller reports a nonzero error code.

The hub `GenericCommand` capability exposes read-only refresh helpers.
Each helper issues the same mapped GCS query commands used by runtime property
reads and updates cached controller, position, profile, servo, busy, and error
state.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- motion_stage` | Generic XY/Z `StageMove`, typed position properties, remultiplexed state set, `Runtime::wait_completed`, timing plan, stop, and homing |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate configured startup readback, motion completion, `ERR?` code meanings, `ONT?`, and moving-status byte against controllers |
| Discovery | Configured stage assignment, axis referencing state, and limit queries |
| Transport | USB/TCP transports beyond configured serial |
| Timing | Wave generators, data recorders, trigger IO, and hardware-accurate timing beyond current software position/servo sequence hooks |
| Compatibility | Controller-default-unit conversion and model-specific feature flags |
