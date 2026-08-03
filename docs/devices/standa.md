# Standa 8SMC

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::standa` |
| Families | Standa 8SMC4-USB stages |
| Support level | Single-axis serial control with explicit opt-in startup identity/position/status/movement-settings readback, read-only engine/brake/home settings refresh, velocity/acceleration writes, refresh helpers, and runtime position endpoint hooks |
| Protocol evidence | Official Standa 8SMC4-USB Communication protocol specification v18.3: <https://doc.xisupport.com/en/8smc4-usb/8SMCn-USB/Programming/Communication_protocol_specification.html> |
| Transport | Configured serial, 115200 baud, 8 data bits, 2 stop bits, no parity, no flow control |
| Discovery | Simulated/config-backed two-stage discovery; configured `serial_port` records endpoint intent unless `connect = true`; optional configured real serial construction behind `os-serial` reads `gser`, `gpos`, `gets`, `gmov`, `geng`, `gbrk`, and `ghom` before registration |
| Validation | No hardware validation |
| Runtime requirements | `os-serial` for real configured serial ports; engine/brake/home setting writes, multi-axis coordination, and broad status/error decoding require specific protocol evidence or traces before implementation |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `standa-8smc4-hub` | `hub`, `motion.controller`, `standa.8smc4` | Owns one configured 8SMC4 serial controller |
| `standa-8smc4-<axis>` | `axis.<axis>`, `stage.1d`, `standa.8smc4.axis` | One configured logical axis; multi-axis/controller aggregation needs model-specific traces |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| Controller serial port | `serial` | Official protocol describes fixed 115200 8N2/no-flow-control serial settings; configured real serial opens only with explicit `connect = true` and uses the command families listed below |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `StageMove` | Axis | `CapabilityRequest::StageMove` | Move/status map | Optional typed motion profile writes use `smov` before `move`/`movr`; motion command waits for command echo/ACK, polls documented `gets` status until the moving flag clears or an estimated motion timeout expires, then issues `gpos` position readback | `position` is sequenceable; runtime timing-plan start/stop applies first/last endpoints through the same `move` path |
| `StageHome` | Axis | `None` | Home status map | Sends `home`, waits for command echo/ACK, polls documented `gets` status until the moving flag clears or an estimated home timeout expires, then issues `gpos` position readback; cached configured state sets position to 0 | Not sequenceable |
| `StageStop` | Axis | `None` | Stop status map | Sends `stop`, waits for command echo/ACK, then issues one `gets` status readback and one `gpos` position readback | Not sequenceable |
| `GenericCommand` | Axis | `refresh_readbacks`, `refresh_position`, `refresh_status`, `refresh_move_settings`, `refresh_engine_settings`, `refresh_brake_settings`, `refresh_home_settings`, or `refresh_static_settings` with no params | Status summary plus movement/static setting readbacks | Uses only mapped `gpos`, `gets`, `gmov`, `geng`, `gbrk`, and `ghom` readback commands; no raw command or setter surface | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `controller` | Hub | `String` | none | R | controller name | No | Config/probe metadata |
| `serial_number` | Hub | `String` | none | R | serial string | No | `gser` during real serial construction; otherwise config |
| `protocol` | Hub | `String` | none | R | `8SMC4 v18.3` | No | Protocol metadata |
| `position` | Axis | `Position` | um | R/W | `0..travel` from config | Yes | `gpos` readback, `move` write |
| `target` | Axis | `Position` | um | R/W | `0..travel` from config | No | local target plus `move` write |
| `velocity` | Axis | `Velocity` | um/s | R/W | converted from documented `Speed`/`uSpeed` fields and configured step size | No | `gmov` readback, `smov` write preserving unexposed movement settings |
| `acceleration` | Axis | `Acceleration` | um/s^2 | R/W | converted from documented `Accel`/`Decel` fields and configured step size | No | `gmov` readback, `smov` write preserving unexposed movement settings |
| `deceleration` | Axis | `Acceleration` | um/s^2 | R | documented `Decel` field converted through configured step size | No | `gmov` readback |
| `antiplay_velocity` | Axis | `Velocity` | um/s | R | documented `AntiplaySpeed`/`uAntiplaySpeed` fields converted through configured step size | No | `gmov` readback |
| `busy` | Axis | `Bool` | none | R | none | No | `gets` move-command state when probed/read |
| `homed` | Axis | `Bool` | none | R | none | No | `gets` status flag when probed/read |
| `left_limit` | Axis | `Bool` | none | R | none | No | `gets` `GPIOFlags` `STATE_LEFT_EDGE`, plus configured metadata |
| `right_limit` | Axis | `Bool` | none | R | none | No | `gets` `GPIOFlags` `STATE_RIGHT_EDGE`, plus configured metadata |
| `motor_enabled` | Axis | `Bool` | none | R | none | No | `gets` `PWRSts` powered states, plus configured metadata |
| `encoder_present` | Axis | `Bool` | none | R | none | No | `gets` `EncSts`, plus configured metadata |
| `alarm` | Axis | `Bool` | none | R | none | No | `gets` state flags |
| `security_flags` | Axis | `I64` | none | R | raw protocol bit mask | No | `gets` state flags |
| `power_state` | Axis | `I64` | none | R | raw protocol byte | No | `gets` `PWRSts` |
| `encoder_state` | Axis | `I64` | none | R | raw protocol byte | No | `gets` `EncSts` |
| `move_state` | Axis | `I64` | none | R | raw protocol byte | No | `gets` `MoveSts` |
| `move_command_state` | Axis | `I64` | none | R | raw protocol byte | No | `gets` `MvCmdSts` |
| `gpio_flags` | Axis | `I64` | none | R | raw protocol bit mask | No | `gets` `GPIOFlags` |
| `raw_flags` | Axis | `I64` | none | R | raw protocol bit mask | No | `gets` state flags |
| `status_summary` | Axis | `Map` | none | R | parsed/local status fields | No | `gets` plus runtime status cache |
| `engine_settings` | Axis | `Map` | mixed native fields | R | `known`, nominal voltage/current/speed, engine flags, antiplay steps, microstep mode, and steps per revolution | No | `geng` readback, refreshed by named helper |
| `brake_settings` | Axis | `Map` | mixed native fields | R | `known`, `t1`..`t4`, brake flags, enabled, and motor-power behavior | No | `gbrk` readback, refreshed by named helper |
| `home_settings` | Axis | `Map` | mixed native fields | R | `known`, fast/slow home speeds, delta position fields, and home flags | No | `ghom` readback, refreshed by named helper |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "standa-8smc"` or `"standa"` | Yes | string | Selects the Standa 8SMC4 discovery provider |
| `property.controller` | No | string | Controller model label |
| `property.serial_number` | No | string | Persistent controller serial |
| `property.axis` | No | string | Logical axis name, for example `x`, `y`, `z`, or `theta` |
| `property.travel` | No | `Position` | Axis travel range; legacy scalar alias `travel_um` |
| `property.step_size` | No | `Position` | Encoder or microstep size; legacy scalar alias `step_size_um` |
| `property.velocity` | No | `Velocity` | Initial move velocity; legacy scalar alias `velocity_um_s` |
| `property.acceleration` | No | `Acceleration` | Initial move acceleration; legacy scalar alias `acceleration_um_s2` |
| `property.serial_port` | No | string | Serial port recorded in resource metadata; `connect=true` opens the real transport |
| `property.connect` | No | `Bool` | Explicit live-open gate; only `true` opens the configured port and queries `gser`, `gpos`, `gets`, `gmov`, `geng`, `gbrk`, and `ghom` before registration |
| `property.startup_readback` | No | `Bool` | Configured startup readback flag; currently ignored for live-open gating |
| `property.active_probe` | No | `Bool` | Deprecated compatibility alias for `startup_readback`; ignored for live-open gating |
| `property.baud_rate` | No | integer | Defaults to official 115200 baud; also recorded in resource metadata |
| `property.timeout_ms` | No | integer | Serial read timeout for configured real ports; recorded as `serial_timeout` resource metadata |
| `property.left_limit_active` / `property.right_limit_active` | Evidence-backed | `Bool` | Add only for externally evidenced config or hardware-validation replay |
| `property.motor_enabled` / `property.encoder_present` | Evidence-backed | `Bool` | Add only for externally evidenced config or hardware-validation replay |

The axis `GenericCommand` capability exposes named read-only refresh helpers
over mapped position, status, movement-settings, engine-settings, brake-settings,
and home-settings readbacks. `refresh_readbacks` refreshes all of those mapped
readback groups. It does not expose raw Standa commands or setter commands.

## Examples

| Example | Demonstrates |
| --- | --- |
| `discover_devices` | Shows configured Standa 8SMC4 in the two-stage discovery flow |
| `motion_stage` | Generic stage workflow for `StageMove`, `StageHome`, `StageStop`, and runtime position endpoint application; not Standa-specific |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate startup `gser`/`gpos`/`gets`/`gmov`/`geng`/`gbrk`/`ghom`, serial framing, command ACKs, `gpos`, `gets`, `smov`, motion completion, stop, and homing against real controllers |
| Transport | Keep serial connection configured by explicit port |
| Status | Extend `gets` decoding beyond the current motion, homed, edge, power, encoder, alarm, security, move-state, and GPIO fields once docs/traces justify more runtime properties |
| Motion/static settings | `gmov`/`smov` velocity and acceleration are mapped through configured step size; read-only `gmov` deceleration and antiplay velocity are exposed; read-only `geng`, `gbrk`, and `ghom` maps expose documented native fields; additional setting writes require field-specific evidence and a safe typed property |
| Multi-axis | Model multiple configured controllers/axes and any shared resource/remultiplexing behavior |
| Runtime timing | Current timing-plan hooks validate and apply software `position` endpoints only; synchronized timing behavior requires hardware validation |
| Protocol expansion | Current command coverage includes `gser`, `gpos`, `gets`, `gmov`, `geng`, `gbrk`, `ghom`, `smov`, `move`, `movr`, `home`, and `stop`; further 8SMC4 command families need field-specific mapping, safe typed properties, or hardware traces before exposure, with firmware/reset/flash-style operations kept hidden |
| Safety | Limit/fault/interlock semantics and abort behavior |
