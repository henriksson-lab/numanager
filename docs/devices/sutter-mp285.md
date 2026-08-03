# Sutter MP-285

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::sutter_mp285` |
| Families | Sutter MP-285 manipulators |
| Support level | Configured opt-in serial control/readback for MP-285 status, position, velocity, move, stop, and refresh helpers |
| Protocol evidence | Public MP-285 command behavior |
| Transport | Binary serial over `SerialIo` |
| Discovery | Config-backed discovery; live serial requires configured endpoint and explicit connect |
| Validation | Configured serial startup-readback/control path is implemented; real hardware validation pending |
| Runtime/evidence notes | `numanager-drivers/os-serial` enables configured real serial ports |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `sutter-mp285-hub` | `hub`, `motion.controller`, `serial.binary` | Owns one serial resource |
| `sutter-mp285-xy` | `stage.xy`, `axis.x`, `axis.y` | X/Y targets share one XYZ controller transaction |
| `sutter-mp285-z` | `stage.z`, `axis.z` | Z shares the same XYZ controller transaction |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `sutter-mp285-serial` | `serial` | Binary serial command path for remultiplexed XYZ moves, status, and velocity commands; resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `StageMove` | XY/Z | `CapabilityRequest::StageMove`; velocity-only `MotionProfile` accepted | Moved-axis map | Optional ACK plus status/position readback when available | X/Y/Z position sequences |
| `StageStop` | XY/Z | `None` | Status string plus property events | Stop byte plus optional ACK and status/position readback when available | Not sequenceable |
| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_status`, or `refresh_position` with no params | Map with command count and status summary | Uses only MP-285 status and position readbacks; no arbitrary binary command surface | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `firmware` | Hub | `String` | none | R | firmware reply | No | Active probe/readback |
| `resolution` | Hub | `Position` | um per microstep | R | configured/status readback | No | Status/probe metadata; legacy status key preserves former `resolution_nm_per_microstep` label |
| `velocity` | Hub | `Velocity` | um/s | R/W | controller range | No | Velocity command and status readback |
| `status_summary` | Hub | `Map` | none | R | configured/status fields | No | Status readback plus current XYZ/target cache |
| `busy` | Hub/XY/Z | `Bool` | none | R | none | No | Command state plus status readback |
| `x` | XY | `Position` | um | R/W | configured travel | Yes | XYZ move/readback |
| `y` | XY | `Position` | um | R/W | configured travel | Yes | XYZ move/readback |
| `target_x` | XY | `Position` | um | R/W | configured travel | No | Target cache before XYZ move |
| `target_y` | XY | `Position` | um | R/W | configured travel | No | Target cache before XYZ move |
| `z` | Z | `Position` | um | R/W | configured travel | Yes | XYZ move/readback |
| `target_z` | Z | `Position` | um | R/W | configured travel | No | Target cache before XYZ move |

## Metadata And Config

| Key | Scope | Type | Status | Meaning |
| --- | --- | --- | --- | --- |
| `travel` | XY/Z metadata, config | `Position` | Canonical | Per-axis travel range used for clamping and property ranges |
| `microstep_size` | XY/Z metadata | `Position` | Canonical | Derived physical size of one MP-285 microstep |
| `travel_um` | Config | `F64`/`I64` micrometers | Legacy alias | Accepted for older configs |
| `legacy_travel_um` | XY/Z metadata | `Position` | Legacy marker | Compatibility label for former `travel_um` metadata |
| `legacy_microstep_size_um` | XY/Z metadata | `Position` | Legacy marker | Compatibility label for former `microstep_size_um` metadata |
| `velocity_microsteps_per_s` | Config/status summary | `I64` | Native controller value | MP-285 velocity register value in microsteps per second |
| `serial_port`, `baud_rate`, `serial_timeout_ms`, `connect` | Configured discovery/resource metadata | `String` / `I64` / `Bool` | Explicit serial endpoint and opt-in real transport connection |

When `connect = true`, discovery opens the configured serial endpoint, runs a
read-only startup probe, and seeds cached firmware, resolution, velocity, position,
and target state from controller replies before registering the driver. Reset
remains an internal protocol primitive and is not part of startup probing,
metadata command previews, or `GenericCommand`.
Runtime reads of `firmware`, `resolution`, `velocity`, and `status_summary`
send `s CR` and ingest the documented status reply. Runtime reads of `x`, `y`,
or `z` send `c CR` and update the cached XYZ position from the binary
readback. Move and stop paths consume optional controller ACK/error
bytes when present and then request status/position readbacks while retaining
cached configured state if no live reply is available.
The current-position-as-origin command remains an internal protocol primitive
and is hidden from regular and advanced command surfaces.

The hub `GenericCommand` capability exposes read-only refresh helpers for
the same status and XYZ position readback commands used by runtime property
reads.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- motion_stage` | Generic XY/Z `StageMove`, typed position properties, remultiplexed state set, `Runtime::wait_completed`, timing plan, and stop |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate binary framing, busy/completion, scaling, and hidden origin behavior against real controller |
| Motion | `StageMoveRequest::profile.velocity` maps to the documented MP-285 velocity command; `profile.acceleration` is rejected because the documented command surface does not expose a typed acceleration command |
| Safety | Limit handling, abort behavior, joystick/manual movement state |
| Timing | Hardware-accurate synchronization beyond current position-sequence hooks |
