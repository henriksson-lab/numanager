# Marzhauser TANGO / L-Step

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::marzhauser` |
| Families | Marzhauser TANGO and L-Step stage controllers |
| Support level | Configured state plus configured opt-in serial stage move/home/stop control, readback, and refresh helpers behind `os-serial` |
| Protocol evidence | Public serial command behavior for identity, position, velocity, acceleration, move, calibrate/home, stop, status, error, and limit commands |
| Transport | Serial ASCII over `SerialIo` |
| Discovery | Configured discovery; `connect = true` opens the explicit serial endpoint and runs the documented configured startup-readback script |
| Validation | Configured-state path and opt-in serial backend compile; real hardware validation pending |
| Runtime/evidence notes | Real serial requires `numanager-drivers/os-serial` and configured `serial_port` |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `marzhauser-hub` | `hub`, `motion.controller`, `serial.ascii` | Owns one serial resource |
| `marzhauser-xy-stage` | `axis.xy`, `stage.xy` | X/Y share controller resource |
| `marzhauser-z-stage` | `axis.z`, `stage.z` | Z shares controller resource |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `marzhauser-serial` | `serial` | Serial command path shared by XY/Z motion, profile, limit, and controller status queries; resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `StageMove` | XY/Z | `CapabilityRequest::StageMove` | Moved-axis map | Writes documented move commands; construction-time probe and property reads ingest controller position/status replies, while final move completion still needs hardware validation | X/Y/Z position plus speed/acceleration software endpoint sequences |
| `StageHome` | XY/Z | `None` | Calibration status string | Writes documented `!cal` commands for X/Y or Z and requests mapped `?statusaxis`, position, and `?err` readbacks when available | Not sequenceable |
| `StageStop` | XY/Z | `None` | Status string plus property events | Writes documented abort command and requests mapped `?statusaxis`, position, and `?err` readbacks when available; fault/limit validation pending | Not sequenceable |
| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_identity`, `refresh_status`, `refresh_position`, `refresh_profiles`, or `refresh_limits` with no params | Map with command count and state summary | Uses only mapped query readbacks; no arbitrary serial command surface | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `version` | Hub | `String` | none | R | controller reply | No | `?ver` configured startup readback/readback |
| `configuration` | Hub | `I64` | none | R | `?det` reply | No | `?det` configured startup readback/readback |
| `busy` | Hub/XY/Z | `Bool` | none | R | status-axis reply | No | `?statusaxis` readback |
| `last_error` | Hub | `String` | none | R | cached raw reply | No | `?err` readback |
| `fault` | Hub | `Bool` | none | R | true for nonzero `?err` reply | No | `?err` readback |
| `state_summary` | Hub | `Map` | none | R | controller identity, limits, typed X/Y/Z state | No | Composite controller/device state |
| `x` | XY | `Position` | um | R/W | configured travel | Yes | `?pos` readback; `!moa`/`!mor` writes |
| `y` | XY | `Position` | um | R/W | configured travel | Yes | `?pos` readback; `!moa`/`!mor` writes |
| `speed_x` | XY | `Velocity` | um/s | R/W | controller range | Yes | `?vel x` readback; `!vel x` write |
| `speed_y` | XY | `Velocity` | um/s | R/W | controller range | Yes | `?vel y` readback; `!vel y` write |
| `accel_x` | XY | `Acceleration` | um/s^2 | R/W | controller range | Yes | `?accel x` readback; `!accel x` write |
| `accel_y` | XY | `Acceleration` | um/s^2 | R/W | controller range | Yes | `?accel y` readback; `!accel y` write |
| `limit_x` | XY | `String` | none | R | controller reply | No | `?lim x` probe/readback |
| `limit_y` | XY | `String` | none | R | controller reply | No | `?lim y` probe/readback |
| `z` | Z | `Position` | um | R/W | configured travel | Yes | `?pos z` readback; `!moa z`/`!mor z` writes |
| `speed` | Z | `Velocity` | um/s | R/W | controller range | Yes | `?vel z` readback; `!vel z` write |
| `accel` | Z | `Acceleration` | um/s^2 | R/W | controller range | Yes | `?accel z` readback; `!accel z` write |
| `limit` | Z | `String` | none | R | controller reply | No | `?lim z` probe/readback |

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- motion_stage` | Generic XY/Z `StageMove`, typed position properties, remultiplexed state set, `Runtime::wait_completed`, timing plan, stop, and homing |

## Config Keys

| Key | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "marzhauser"` | Yes | string | Selects the Marzhauser configured discovery provider |
| `property.version`, `property.controller`, `property.configuration` | No | `String` / `I64` | Configured or probe identity |
| `property.x_travel`, `property.y_travel`, `property.z_travel` | No | `Position` | Axis travel ranges |
| `property.pitch_x`, `property.pitch_y`, `property.pitch_z` | No | `Position` | Axis screw pitch/calibration distance |
| `property.steps_per_mm` | No | `F64` / `I64` | Controller step calibration scalar from the protocol/manual surface |
| `property.limit_x`, `property.limit_y`, `property.limit_z` | No | `String` | Fixture limit reply overrides |
| `property.last_error` | No | `String` | Fixture/controller error reply cache |
| `property.serial_port`, `property.baud_rate`, `property.serial_timeout_ms`, `property.connect` | No | `String` / `I64` / `Bool` | Explicit serial endpoint and opt-in real transport connection. With `connect = true`, discovery opens the port and runs the probe script before adding the driver |

Legacy scalar aliases `x_travel_um`, `y_travel_um`, `z_travel_um`,
`pitch_x_mm`, `pitch_y_mm`, and `pitch_z_mm` remain accepted for existing
configs. New configs should use the typed `Position` keys above.

Runtime property reads request and ingest the mapped query reply before
returning cached state. Home and stop paths also request mapped busy,
position, and error readbacks after command writes, while retaining configured
cached configured state if no live reply is available.

The hub `GenericCommand` capability exposes read-only refresh helpers.
Each helper issues the same mapped query commands used by runtime property reads
and updates cached controller, position, profile, limit, busy, and error state.

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate protocol variants, ingested position/status/profile reply forms, position scaling, and completion against real controllers |
| Discovery | Axis inventory and model-specific feature probing |
| Safety | Limit, joystick/manual state, and fault handling |
| Timing | Hardware-synchronized timing beyond current software position/profile endpoint application |
