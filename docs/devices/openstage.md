# OpenStage

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::openstage` |
| Families | OpenStage Arduino Mega microscope stage controller |
| Support level | XYZ move plus post-motion position readback, controller-info, velocity/acceleration settings, beep, and runtime timing-endpoint serial control with opt-in startup readback behind `os-serial`; coordinate-zeroing remains hidden from regular and advanced command surfaces |
| Protocol evidence | The OpenStage paper publishes the PC serial control interface for absolute/relative XYZ moves, position readback, zeroing, step-size read/write, velocity read/write, acceleration read/write, speed-mode selection, beep, and controller information |
| Transport | Plain ASCII serial, `$` terminator for move commands and replies, default 115200 baud |
| Discovery | Config-backed two-stage discovery only; configured real serial construction reads controller information, position, and step size before registration |
| Validation | No hardware validation |
| Runtime/evidence notes | Real serial requires `numanager-drivers/os-serial`; fault/error behavior and safe stop/disable behavior need hardware traces or documentation because the published command table does not define stop/disable commands |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `openstage-hub` | `hub`, `motion.controller`, `serial.ascii` | One controller resource owns all three axes |
| `openstage-xy` | `axis.xy`, `stage.xy`, `motion.stage` | X/Y logical stage remultiplexed into shared XYZ absolute/relative commands |
| `openstage-z` | `axis.z`, `stage.z`, `motion.stage` | Z logical stage remultiplexed into shared XYZ absolute/relative commands |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `openstage-serial` | `serial.ascii` | Sends the startup/move/readback/settings commands used by this driver and reads `$`-terminated completion/readback replies |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `StageMove` | `openstage-xy` | `CapabilityRequest::StageMove` with X/Y targets | XYZ position map | `$` terminator plus `p` position readback for real serial or configured completion; no busy/status polling is implemented | `x` and `y` are sequenceable; runtime timing-plan start/stop applies first/last endpoints through the same absolute XYZ move |
| `StageMove` | `openstage-z` | `CapabilityRequest::StageMove` with Z target | XYZ position map | `$` terminator plus `p` position readback for real serial or configured completion; no busy/status polling is implemented | `z` is sequenceable; runtime timing-plan start/stop applies first/last endpoints through the same absolute XYZ move |
| `GenericCommand` | `openstage-hub` | `read_information`, `read_velocity`, `read_acceleration`, or `beep` | Controller-info, velocity, acceleration, or beep status map | Information/settings reply or command acceptance | Not sequenceable; coordinate-zeroing remains hidden from regular and advanced command surfaces |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `product` | Hub | `String` | none | R | configured product label | No | Config/probe metadata |
| `serial_number` | Hub | `String` | none | R | configured serial label | No | Config/probe metadata |
| `controller_info` | Hub | `String` | none | R | controller information string | No | `I` command |
| `protocol` | Hub | `String` | none | R | OpenStage serial protocol | No | Protocol metadata |
| `step_size` | Hub | `Position` | um | R/W | positive; maps to the documented command shape; not validated against all firmware modes | No | `ss` / `sr` commands |
| `speed_mode` | Hub | `I64` | mode | R/W | `1..4` | No | `m` command |
| `last_transaction` | Hub | `Map` | none | R | command, XYZ positions, completion basis | No | Runtime transaction cache |
| `x` | XY stage | `Position` | um | R/W | `0..x_travel` | Yes | absolute/relative go-to plus position readback |
| `y` | XY stage | `Position` | um | R/W | `0..y_travel` | Yes | absolute/relative go-to plus position readback |
| `x_velocity`, `y_velocity` | XY stage | `Velocity` | um/s | R/W | positive; advertised max 100000 um/s when hardware limits are not validated | No | `vs` / `vr` commands |
| `x_acceleration`, `y_acceleration` | XY stage | `Acceleration` | um/s^2 | R/W | positive; advertised max 1000000 um/s^2 when hardware limits are not validated | No | `as` / `ar` commands |
| `x_travel` | XY stage | `Position` | um | R | configured travel | No | Config/probe metadata |
| `y_travel` | XY stage | `Position` | um | R | configured travel | No | Config/probe metadata |
| `z` | Z stage | `Position` | um | R/W | `0..z_travel` | Yes | absolute/relative go-to plus position readback |
| `z_velocity` | Z stage | `Velocity` | um/s | R/W | positive; advertised max 100000 um/s when hardware limits are not validated | No | `vs` / `vr` commands |
| `z_acceleration` | Z stage | `Acceleration` | um/s^2 | R/W | positive; advertised max 1000000 um/s^2 when hardware limits are not validated | No | `as` / `ar` commands |
| `z_travel` | Z stage | `Position` | um | R | configured travel | No | Config/probe metadata |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "openstage"` or `"open_stage"` | Yes | string | Selects the OpenStage provider |
| `property.product` | No | string | Persistent product/model label |
| `property.serial_number` | No | string | Persistent serial label |
| `property.controller_info` | No | string | Initial cached controller information for fixture/configured mode |
| `property.x`, `property.y`, `property.z` | No | `Position` | Initial configured positions |
| `property.x_travel`, `property.y_travel`, `property.z_travel` | No | `Position` | Configured travel ranges |
| `property.step_size` | No | `Position` | Initial step-size setting |
| `property.x_velocity`, `property.y_velocity`, `property.z_velocity` | No | `Velocity` | Initial per-axis velocity settings |
| `property.x_acceleration`, `property.y_acceleration`, `property.z_acceleration` | No | `Acceleration` | Initial per-axis acceleration settings |
| `property.speed_mode` | No | `I64` | Initial controller speed mode, `1..4` |
| `property.serial_port` | For real serial | string | OS serial port name; also recorded in resource metadata |
| `property.connect` | No | `Bool` | If true, opens the configured serial port behind `os-serial` and reads `I` controller information, `p` position, `sr` step size, `vr` velocity, and `ar` acceleration before registration; otherwise uses the configured state model |

Runtime position reads issue the `p` readback command before returning cached
XYZ values. Velocity and acceleration reads issue `vr` and `ar` before
returning cached axis settings. Absolute and relative moves consume their
normal command terminator and then request `p` position readback before
returning completion. StageMove profile velocity and acceleration are applied
through `vs` and `as` before the move. Coordinate-zeroing remains private
protocol evidence and is not exposed through regular or advanced command
surfaces. Busy/status polling, skipped-step detection, safe stop/disable
behavior, and fault/error parsing need hardware traces.

## Examples

| Example | Demonstrates |
| --- | --- |
| `discover_devices` | Shows a configured OpenStage controller in the two-stage discovery flow |
| `motion_stage` | Generic stage selection, typed `StageMove`, typed position properties, runtime-owned completion, and software timing-plan endpoint application |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Record construction-time controller-info/position/step-size/velocity/acceleration readback, command stdout/stderr, motion completion terminators, post-motion position readback, velocity/acceleration setting behavior, and beep on a real controller |
| Completion | Validate whether motion completion terminator and post-motion position readback are always emitted and how timeouts/skipped steps surface |
| Timing | Runtime timing plans apply first/last X/Y/Z endpoints through software absolute moves; hardware-timed or synchronized motion remains unvalidated |
| Protocol expansion | The published serial command table is implemented for the public motion, readback, step/velocity/acceleration setting, speed-mode, information, and beep surfaces. Coordinate-zeroing is recorded as protocol evidence only and remains hidden. Error/fault behavior is not exposed without firmware-specific documentation or hardware traces |
| Safety | Validate travel limits, safe stop/disable behavior if firmware-specific commands exist, and what happens on malformed commands |
