# Sutter/Ludl-Compatible Stage

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::sutter_stage` |
| Families | SutterStage/Ludl-compatible serial stage controllers |
| Support level | Configured opt-in serial move/home/stop control, readback, Sutter/Ludl autofocus state, and refresh helpers |
| Protocol evidence | Public serial command behavior |
| Transport | Serial ASCII over `SerialIo` |
| Discovery | Config-backed discovery; live serial requires configured endpoint and explicit connect |
| Validation | Configured serial startup-readback/control path is implemented; real hardware validation pending |
| Runtime/evidence notes | `numanager-drivers/os-serial` enables configured real serial ports |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `sutter-stage-hub` | `hub`, `motion.controller`, `serial.ascii` | Owns one serial resource |
| `sutter-xy-stage` | `axis.xy`, `stage.xy` | X/Y commands share controller resource |
| `sutter-z-stage` | `axis.z`, `stage.z` | Z shares controller resource |
| `sutter-autofocus` | `autofocus`, `sutter.af` | General autofocus provider with Z-stage dependency |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `sutter-stage-serial` | `serial` | Serial command path shared by XY/Z motion and autofocus command/readback state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `StageMove` | XY/Z | `CapabilityRequest::StageMove`; velocity-only `MotionProfile` accepted | Moved-axis map | Cached configured state plus mapped property readback when available | X/Y/Z position sequences |
| `StageHome` | XY | `None` | Status string plus property events | Sends documented `HOME X Y`, resets cached XY position to zero, and requests mapped `STATUS`/`WHERE` readback when available | Not sequenceable |
| `StageStop` | XY/Z | `None` | Status string plus property events | Cached configured state plus mapped `STATUS`/`WHERE` readback when available | Not sequenceable |
| `Autofocus` | Autofocus device | `CapabilityRequest::Autofocus` | Provider-neutral autofocus state map | Runtime token completion | Enable/mode sequences |
| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_identity`, `refresh_status`, `refresh_position`, or `refresh_profiles` with no params | Map with command count and state summary | Uses only mapped Sutter/Ludl query readbacks; no arbitrary serial command or autofocus action surface | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `version` | Hub | `String` | none | R | controller reply | No | Active probe/readback |
| `inventory` | Hub | `String` | none | R | module inventory reply | No | `Rconfig` probe/readback |
| `transmission_delay` | Hub | `TimeInterval` | controller_tick | R/W | 1..255 | No | `TRXDEL` command/readback; scalar tick values accepted as legacy writes |
| `busy` | Hub/XY/Z | `Bool` | none | R | none | No | `STATUS <axis>` readback |
| `state_summary` | Hub | `Map` | none | R | version, inventory, typed XY/Z state, speeds, autofocus state/dependency | No | Composite status/readback with status and position query ingestion |
| `x` | XY | `Position` | um | R/W | configured travel | Yes | Axis move/readback |
| `y` | XY | `Position` | um | R/W | configured travel | Yes | Axis move/readback |
| `speed` | XY/Z | `Velocity` | um/s | R/W | controller range | No | XY readback uses `SPEED X Y`; Z is cached command state |
| `start_speed` | XY | `Velocity` | um/s | R/W | controller range | No | `STSPEED X Y` readback |
| `acceleration` | XY | `ControllerScalar` | controller_step | R/W | 1..255 | No | `ACCEL X Y` readback; scalar controller steps accepted as legacy writes |
| `z` | Z | `Position` | um | R/W | configured travel | Yes | Axis move/readback |
| `autofocus_parameter` | Z | `I64` | controller scalar | R/W | controller dependent | No | `AF <axis>=<parameter>` |
| `enabled` | Autofocus | `Bool` | none | R/W | none | Yes | `AF` command path |
| `mode` | Autofocus | `String` | none | R/W | provider modes | Yes | `AF` command path |
| `status` | Autofocus | `String` | none | R | provider status | No | Local/provider state |
| `focus_score` | Autofocus | `F64` | none | R | none | No | Local/provider state |
| `parameter` | Autofocus | `I64` | controller scalar | R/W | controller dependent | No | `AF <axis>=<parameter>` |

## Metadata And Config

| Key | Applies to | Type | Meaning |
| --- | --- | --- | --- |
| `x_travel`, `y_travel` | XY stage/config | `Position` | Axis travel ranges |
| `z_travel` | Z stage/config | `Position` | Axis travel range |
| `step_size` | XY/Z stage/config | `Position` | Controller step size |
| `x_axis`, `y_axis`, `z_axis` | Config/device metadata | `String` | Controller axis labels |
| `serial_port`, `baud_rate`, `serial_timeout_ms`, `connect` | Configured discovery/resource metadata | `String` / `I64` / `Bool` | Explicit serial endpoint and opt-in real transport connection |

Legacy scalar aliases `x_travel_um`, `y_travel_um`, `z_travel_um`, and
`step_size_um` remain accepted for existing configs. Descriptor metadata keeps
old names only as explicitly labeled `legacy_*` entries.

When `connect = true`, discovery opens the configured serial endpoint, runs a
read-only startup probe, and seeds cached version, inventory, position,
transmission-delay, speed, start-speed, acceleration, and busy state from
controller replies before registering the driver. Module reset remains an
internal protocol primitive and is not part of startup probing, metadata command
previews, or `GenericCommand`.

Runtime property reads request and ingest the mapped query reply before
returning cached state. Writable transmission delay, XY position, XY speed,
XY start speed, XY acceleration, and Z position paths request the corresponding
query readback when replies are available. Stop paths also request mapped
status/position readbacks after command writes, while retaining the configured
cached configured state if no live reply is available. The XY origin command remains hidden
from regular and advanced command surfaces.

The hub `GenericCommand` capability exposes read-only refresh helpers.
Each helper issues the same mapped query commands used by runtime property
reads for identity, busy/state summary, XY/Z position, transmission delay, and
XY speed/start-speed/acceleration.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- motion_stage` | Generic XY/Z `StageMove`, typed position properties, remultiplexed state set, `Runtime::wait_completed`, timing plan, stop, and XY homing where available |
| `cargo run -p numanager-examples -- autofocus` | Provider-neutral autofocus selection including SutterStage |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate command grammar, status/busy behavior, and autofocus semantics against hardware |
| Motion | `StageMoveRequest::profile.velocity` maps to typed speed commands; `profile.acceleration` is rejected because the documented `ACCEL` command is a native controller scalar and needs calibration evidence before conversion |
| Autofocus | Real focus metric validation and hardware-triggered autofocus timing beyond current enable/mode sequence hooks |
