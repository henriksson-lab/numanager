# Zaber ASCII Stages

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::zaber` |
| Families | Zaber ASCII single-axis and multi-axis controllers |
| Support level | ASCII motion/readback for configured/probed axes, selected refresh helpers, and optional serial transport |
| Protocol evidence | Public Zaber ASCII command model |
| Transport | ASCII serial over `SerialIo` |
| Discovery | Configured discovery plus optional configured startup readback for explicitly configured serial endpoints |
| Validation | Configured-state path and opt-in serial backend compile; real hardware validation pending |
| Runtime/evidence notes | `numanager-drivers/os-serial` for configured real serial ports |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `zaber-ascii-hub` | `hub`, `motion.controller`, `serial.ascii`, `zaber.ascii` | Owns one serial resource for single-axis fixture |
| `zaber-ascii-axis-*` | `axis.*`, `stage.1d`, `zaber.ascii.axis` | Axis logical device; multi-axis fixtures share one serial resource |
| `zaber-ascii-multi-axis-hub` | `hub`, `motion.controller`, `serial.ascii`, `zaber.ascii`, `multi.axis` | Owns one serial resource for multiple logical axes |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `zaber-ascii-serial` | `serial` | ASCII serial command path for the single-axis command set |
| `zaber-ascii-multi-axis-serial` | `serial` | ASCII serial command path shared by configured/probed logical axes |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `StageMove` | Axis devices | `CapabilityRequest::StageMove` or `None` | Move/status map | Sends `move abs`/`move rel`, ingests command status/warning when present, then refreshes `get pos`; cached configured state records the target position only when no reply is available | Position sequences for single and multi-axis |
| `StageHome` | Axis devices | `None` | Home status map | Sends `home`, ingests command status/warning when present, then refreshes `get pos`; cached configured state records the home position only when no reply is available | Not sequenceable |
| `StageStop` | Axis devices | `None` | Stop status map | Sends `stop`, ingests command status/warning when present, then refreshes `get pos`; cached configured state clears busy only when no reply is available | Not sequenceable |
| `GenericCommand` | Axis devices | `refresh_readbacks`, `refresh_position`, `refresh_velocity`, `refresh_acceleration`, `refresh_status`, `refresh_warning`, or `refresh_axis_summary` with no params | Refreshed property map | Sends selected Zaber ASCII `get` readback through the existing property path; no arbitrary ASCII command/settings surface | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `device_id` | Hub | `String` | none | R | controller id | No | Probe metadata |
| `serial_number` | Hub | `String` | none | R | controller serial | No | Probe metadata |
| `axis_count` | Hub | `I64` | count | R | configured/probed count | No | Probe metadata |
| `state_summary` | Hub | `Map` | none | R | configured/probed state fields | No | `get pos` reply updates status-bearing axis state before summary |
| `position` | Axis | `Position` | um | R/W | configured travel | Yes | `move abs`, `get pos` |
| `target` | Axis | `Position` | um | R/W | configured travel | No | Target cache only; motion uses `position`/`StageMove` |
| `velocity` | Axis | `Velocity` | um/s | R/W | configured range | No | `set maxspeed`, `get maxspeed` readback |
| `acceleration` | Axis | `Acceleration` | um/s^2 | R/W | configured range | No | `set accel`, `get accel` readback |
| `busy` | Axis | `Bool` | none | R | none | No | Status field from `get pos` reply |
| `status` | Axis | `String` | none | R | status labels | No | Status field from readback reply |
| `warning` | Axis | `String` | none | R | warning flags | No | Warning field from readback reply |
| `peripheral_id` | Axis | `String` | none | R | peripheral id | No | Probe/config metadata from `peripheral.id` |
| `travel` | Axis | `Position` | um | R | axis travel range | No | Probe/config metadata from `limit.max` scaled by `resolution` |
| `microstep_size` | Axis | `Position` | um | R | native unit conversion | No | Probe/config metadata from `resolution` |
| `warning_summary` | Axis | `Map` | none | R | decoded warning category/severity | No | Warning classifier |
| `axis_summary` | Axis | `Map` | none | R | configured/probed state fields | No | Axis readback parser |

## Metadata And Config

| Key | Applies to | Type | Meaning |
| --- | --- | --- | --- |
| `travel` | Axis/config | `Position` | Axis travel range, also exposed as an axis property |
| `microstep_size` | Axis/config | `Position` | Native unit conversion, also exposed as an axis property |
| `position` / `probed_position` | Axis/config or metadata | `Position` | Initial/probed position |
| `velocity` | Axis/config or metadata | `Velocity` | Initial/probed velocity |
| `acceleration` | Axis/config or metadata | `Acceleration` | Initial/probed acceleration |
| `address`, `axis`, `device_id`, `peripheral_id`, `serial_number` | Config/probe metadata | `I64` / `String` | Zaber chain identity |
| `serial_port`, `baud_rate`, `serial_timeout_ms`, `connect`, `startup_readback` | Configured discovery/resource metadata | `String` / `I64` / `Bool` | Explicit serial endpoint, opt-in real transport, and configured startup readback |

Configured discovery accepts typed values for the physical quantities above.
Legacy scalar aliases `travel_um`, `microstep_size_um`, `position_um`,
`velocity_um_s`, and `acceleration_um_s2` remain accepted for existing configs.
The deprecated compatibility key `active_probe` is accepted as an alias for
`startup_readback`; it is not serial autodiscovery.
Descriptor and discovery metadata keep old names only as explicitly labeled
`legacy_*` entries.

Move, home, and stop invocations use the same status/warning parser as property
readback for command replies, then issue `get pos` to update typed position and
status-bearing axis state when the serial transport returns a reply.

Axis `GenericCommand` refresh helpers issue only selected `get pos`, `get
maxspeed`, or `get accel` requests through the existing property readback path.
`refresh_status`, `refresh_warning`, and `refresh_axis_summary` use `get pos`
because status and warning fields come from the readback reply.
`refresh_readbacks` combines position, velocity, acceleration, and axis-summary
readbacks. They do not expose arbitrary ASCII commands or settings.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- motion_stage` | Generic XY/Z `StageMove`, typed position properties, remultiplexed state set, `Runtime::wait_completed`, timing plan, stop, and homing |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate reply parsing, home/stop command status, warning/status handling, scaling, and multi-axis behavior against real devices |
| Discovery | Device chain inventory and axis label persistence require configured endpoints, manufacturer database evidence, or captured controller replies |
| Timing | Hardware trigger support if available for target models |
| Protocol expansion | Current command coverage includes selected readback `get` requests, velocity/acceleration `set` requests, `move abs`, `move rel`, `home`, `stop`, and mapped refresh helpers. Broader Zaber settings, device database behavior, streaming, and trigger behavior is not exposed without manufacturer documentation, public protocol evidence, or hardware traces |
