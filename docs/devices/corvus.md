# ITK Corvus

## Status

| Field | Value |
| --- | --- |
| Driver module | `numanager_drivers::corvus` |
| Families | ITK/Marzhauser Corvus stage controllers exposing controller, XY stage, and optional Z axis |
| Support level | Opt-in serial startup readback, stage move/home/stop writes, runtime timing endpoints, refresh helpers, and known numeric position/speed/acceleration readback |
| Protocol evidence | Reverse engineered serial command evidence |
| Transport | Serial ASCII host-mode command session; default configured provider records 115200 baud, 8 data bits, no parity, 1 stop bit |
| Discovery | Configured discovery; optional serial connection from config with startup host-mode, version, status, and error queries |
| Validation | Configured-state path and opt-in serial backend compile; real controller validation pending |
| Evidence gaps | Exact manual revision, broader status/error parsing, hardware-tuned completion, range-measure limits, and axis-orientation validation need hardware traces or documentation |

## Logical Devices

| Device | Kind tags | Role |
| --- | --- | --- |
| `corvus-hub` | `hub`, `motion.controller`, `serial.ascii` | Owns the serial command session and shared motion settings |
| `corvus-xy-stage` | `axis.xy`, `stage.xy`, `motion.stage` | Axes 1/2 share one controller resource and are remultiplexed into XY moves |
| `corvus-z-stage` | `axis.z`, `stage.z`, `motion.stage` | Optional axis 3 logical Z device |

## Resources

| Resource | Kind | Notes |
| --- | --- | --- |
| `corvus-serial` | `serial.ascii` | Host-mode text commands with space transmit terminator and CRLF receive terminator in the audited adapter; resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing |
| --- | --- | --- | --- | --- | --- |
| `StageMove` | XY stage | `CapabilityRequest::StageMove` with X/Y targets | Position map | Serial write plus `st` busy-bit polling, then `p` position and `ge` error readbacks when `connect = true`; relative moves encode the clamped delta while cached readback stores the final position; configured acceptance otherwise | `x`, `y` are sequenceable; runtime timing-plan start/stop applies first/last endpoints through the same XY absolute move path |
| `StageMove` | Z stage | `CapabilityRequest::StageMove` with Z target | `Position` | Serial write plus `st` busy-bit polling, then `p` position and `ge` error readbacks when `connect = true`; relative moves encode the clamped delta while cached readback stores the final position; configured acceptance otherwise | `z` is sequenceable; runtime timing-plan start/stop applies first/last endpoints through the same Z absolute move path |
| `StageHome` | XY/Z stage | `None` | Position map or `Position` | Serial `cal` write plus `st` busy-bit polling, then `p` position and `ge` error readbacks when `connect = true`; configured position resets to zero otherwise | No |
| `StageStop` | XY/Z stage | `None` | Map with `moving=false` | Serial write plus `st` busy-bit polling, then `p` position and `ge` error readbacks when `connect = true`; configured acceptance otherwise | No |
| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_status`, `refresh_error`, `refresh_position`, `refresh_limits`, `refresh_speed`, or `refresh_acceleration` with no params | raw reply string or readback map | Sends only mapped status, error, position, limit, speed, and acceleration queries and caches replies; known numeric position, speed, and acceleration replies update typed readbacks; configured acceptance otherwise | No |

## Properties

| Property | Device | Type | Unit | Access | Range/enums | Sequenceable | Mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `product` | Hub | `String` | none | R | configured model string | No | Controller family |
| `serial_number` | Hub | `String` | none | R | configured identity | No | Discovery-lock identity |
| `serial_port` | Hub | `String` | none | R | configured port or empty | No | Serial resource label |
| `version` | Hub | `String` | none | R | configured or non-empty active version reply | No | Startup version query |
| `connected` | Hub | `Bool` | none | R | true when the opt-in serial port is open | No | Runtime transport state |
| `serial_timeout` | Hub | `TimeInterval` | ms | R | configured serial read window | No | Config metadata |
| `protocol` | Hub | `String` | none | R | fixed description | No | Host-mode serial command surface |
| `speed` | Hub | `Velocity` | um/s | R/W | positive | No | Controller velocity read/write in um/s |
| `acceleration` | Hub | `Acceleration` | m/s^2 | R/W | positive | No | Adapter exposes acceleration in m/s^2 |
| `joystick_enabled` | Hub | `Bool` | none | R/W | `true`/`false` | No | Joystick enable command |
| `status` | Hub | `I64` or `Null` | none | R | startup status reply when connected | No | Startup status query |
| `busy` | Hub | `Bool` | none | R | derived from startup status bit when available | No | Startup status query |
| `last_error` | Hub | `String` | none | R | non-empty startup error query reply when connected | No | Startup error query |
| `status_reply` | Hub | `String` | none | R | cached raw reply | No | Status query |
| `position_reply` | Hub | `String` | none | R | cached raw reply | No | Position query; known whitespace-separated micrometer values update typed positions |
| `limit_reply` | Hub | `String` | none | R | cached raw reply | No | Limit query |
| `speed_reply` | Hub | `String` | none | R | cached raw reply | No | Velocity query; known numeric replies update `speed` |
| `acceleration_reply` | Hub | `String` | none | R | cached raw reply | No | Acceleration query; known numeric replies update `acceleration` |
| `last_transaction` | Hub | `Map` | none | R | action, completion basis, encoded length | No | Diagnostic transaction summary without exposing command text |
| `x` | XY stage | `Position` | um | R/W | `0..x_travel` | Yes | Axis 1 position |
| `y` | XY stage | `Position` | um | R/W | `0..y_travel` | Yes | Axis 2 position |
| `x_travel` | XY stage | `Position` | um | R | configured travel | No | Configured software travel bound; raw `getlimit` text is exposed through `limit_reply`/`refresh_limits` |
| `y_travel` | XY stage | `Position` | um | R | configured travel | No | Configured software travel bound; raw `getlimit` text is exposed through `limit_reply`/`refresh_limits` |
| `z` | Z stage | `Position` | um | R/W | `0..z_travel` | Yes | Axis 3 position |
| `z_travel` | Z stage | `Position` | um | R | configured travel | No | Z limit validation is not recorded |

## Config

| Key | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "corvus"` | Yes | string | Selects the Corvus configured provider |
| `serial_port` | Required when `connect = true` | string | Serial port path/name |
| `connect` | No | `Bool` | Open the serial port and send commands through the live transport |
| `serial_timeout_ms` | No | `I64` or `TimeInterval` | Serial read window after each command; default 500 ms |
| `baud_rate` | No | `I64` | Controller baud; Micro-Manager page records DIP-switch-dependent values |
| `product`, `serial_number`, `version` | No | string | Discovery-lock identity and descriptive metadata |
| `expose_z` | No | `Bool` | Adds the logical Z device when true |
| `property.x`, `property.y`, `property.z` | No | `Position` | Initial configured positions |
| `property.x_travel`, `property.y_travel`, `property.z_travel` | No | `Position` | Configured software travel bounds |
| `property.speed` | No | `Velocity` | Shared configured motion speed |
| `property.acceleration` | No | `Acceleration` | Shared configured acceleration |
| `property.joystick_enabled` | No | `Bool` | Configured joystick state |
| `property.status_reply`, `property.last_error`, `property.position_reply`, `property.limit_reply`, `property.speed_reply`, `property.acceleration_reply` | No | string | Configured cached raw replies when not connected; known position/speed/acceleration replies seed typed readbacks |

Present Corvus config keys with the wrong typed value are rejected instead of
silently falling back to configured defaults. This includes `expose_z`, because it
controls whether the logical Z device is advertised.

Runtime reads of `status_reply`, `last_error`, `position_reply`, `limit_reply`,
`speed_reply`, and `acceleration_reply` issue the mapped query before returning
cached raw text. Move and stop paths request `st` status
readbacks while the documented busy bit is set, then request `p` position and
`ge` error readbacks when the serial transport is connected. Broader
status/error vocabulary, timeout tuning, and limit semantics need hardware
traces or documentation.

## Examples

| Example | Coverage |
| --- | --- |
| `discover_devices` | Shows a configured ITK Corvus controller in the two-stage discovery flow |
| `motion_stage corvus` | Runs the generic XY/Z stage workflow with typed properties, waits, homing, stop, timing plan, and event output |

## Remaining Work

| Area | Needed evidence |
| --- | --- |
| Primary specification | Pin exact Corvus controller manual/command-list revision before expanding stage write behavior |
| Configured serial | Current live path requires configured `serial_port`; startup readback caches version, status/busy, and error-query replies; hub refresh commands can repeat status, error, position, limit, speed, and acceleration queries |
| Completion | Hardware-tune busy-bit polling interval/timeouts, confirm settled-position behavior, and add broader status/error handling |
| Timing | Runtime timing plans apply first/last X/Y/Z endpoints through software absolute moves; synchronized or hardware-triggered timing remains unvalidated |
| Coordinates | Validate axis orientation, unit mode, adapter-origin semantics, and Z-axis availability probing |
| Limits | Map raw limit replies to typed travel bounds after the reply fields and axis orientation are documented |
| Hardware validation | Record runtime output next to serial trace/readback for move, read position, stop, speed/acceleration writes, joystick toggle, and error states |
