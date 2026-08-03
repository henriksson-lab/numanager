# Chuo Seiki QT

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::chuo_seiki_qt` |
| Families | Chuo Seiki QT-series 1/2/3-axis stage controllers |
| Support level | Opt-in serial startup identification, stage writes, runtime timing endpoints, busy/position/readback refresh helpers, and known-format typed position-state readback |
| Protocol evidence | Chuo manufacturer page confirming QT controller RS-232/USB command control plus reverse engineered command evidence |
| Transport | ASCII serial, 9600-8N1, CRLF line ending; live construction sends identification and feedback-mode commands |
| Discovery | Config-backed two-stage discovery; optional serial connection from config |
| Validation | No hardware validation |
| Evidence gaps | Broader reply/error parsing, timeout tuning, limit handling, and physical speed calibration need hardware traces or documentation |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `chuo-qt-hub` | `hub`, `motion.controller`, `serial.ascii` | Owns one serial controller resource |
| `chuo-qt-xy-stage` | `axis.xy`, `stage.xy`, `motion.stage` | Axes A/B share the controller resource and move through a remultiplexed XY command |
| `chuo-qt-z-stage` | `axis.z`, `stage.z`, `motion.stage` | Optional one-axis stage using configured axis A/B/C, normally C for 3-axis controllers |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `chuo-qt-serial` | `serial.ascii` | Shared text-command session for QT stage moves, home, stop, position, and motion-parameter commands; resource metadata records configured `serial_port`, fixed `baud_rate`, `serial_timeout`, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `StageMove` | XY stage | `CapabilityRequest::StageMove` with X/Y targets | Map with X/Y/Z positions | Serial write plus position-state polling while known `D`/`H` motion states are reported when `connect = true`; configured acceptance otherwise | `x`, `y` are sequenceable; runtime timing-plan start/stop applies first/last endpoints through the same XY move path |
| `StageMove` | Z stage | `CapabilityRequest::StageMove` with Z target | `Position` | Serial write plus position-state polling while known `D`/`H` motion states are reported when `connect = true`; configured acceptance otherwise | `z` is sequenceable; runtime timing-plan start/stop applies first/last endpoints through the same Z move path |
| `StageHome` | XY/Z stage | `None` | Position map or `Position` | Serial write plus position-state polling while known `D`/`H` motion states are reported when `connect = true`; configured acceptance otherwise | No |
| `StageStop` | XY/Z stage | `None` | Map with `moving=false` | Serial write plus position-state polling and busy refresh when `connect = true`; configured acceptance otherwise | No |
| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_busy`, or `refresh_position` with no params | raw reply string or readback map | Sends only the documented busy and position queries for configured axes, caches raw replies, and updates typed positions when the known signed-step/state segment grammar is recognized; configured acceptance otherwise | No |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `product` | Hub | `String` | none | R | configured product label, or non-empty startup identification banner when connected | No | Config/probe metadata |
| `serial_number` | Hub | `String` | none | R | configured serial label | No | Config/probe metadata |
| `serial_port` | Hub | `String` | none | R | configured serial port label | No | Config metadata |
| `connected` | Hub | `Bool` | none | R | true when the opt-in serial port is open | No | Runtime transport state |
| `serial_timeout` | Hub | `TimeInterval` | ms | R | configured serial read window | No | Config metadata |
| `protocol` | Hub | `String` | none | R | fixed provenance label | No | Runtime metadata |
| `step_size` | Hub | `Position` | um | R/W | positive | No | Step-to-position conversion |
| `high_speed` | Hub | `I64` | controller pulses/s | R/W | positive, must be greater than `low_speed` in config | No | Native high-speed setting; writes all configured axes when connected |
| `low_speed` | Hub | `I64` | controller pulses/s | R/W | positive | No | Native low-speed setting; writes all configured axes when connected |
| `acceleration_time` | Hub | `TimeInterval` | ms | R/W | positive | No | Native acceleration-time setting; writes all configured axes when connected |
| `busy_reply` | Hub | `String` | none | R | cached raw reply | No | Busy query for configured axes, refreshed on read |
| `position_reply` | Hub | `String` | none | R | cached raw reply | No | Position query for configured axes, refreshed on read; known signed-step/state segments update typed positions |
| `last_transaction` | Hub | `Map` | none | R | action, encoded length, completion basis, live serial flag, reply text | No | Runtime transaction cache |
| `x`, `y` | XY stage | `Position` | um | R/W | `0..x_travel`, `0..y_travel` | Yes | Axis A/B target positions through step conversion |
| `x_travel`, `y_travel` | XY stage | `Position` | um | R | configured travel range | No | Config/probe metadata |
| `z` | Z stage | `Position` | um | R/W | `0..z_travel` | Yes | Configured axis target through step conversion |
| `z_travel` | Z stage | `Position` | um | R | configured travel range | No | Config/probe metadata |
| `axis` | Z stage | `String` | none | R | `A`, `B`, or `C` | No | Configured controller channel |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "chuo_seiki_qt"` | Yes | string | Selects the Chuo QT provider |
| `property.serial_port` | Required when `connect = true` | string | Serial port path/name |
| `property.connect` | No | `Bool` | Open the serial port and send commands through the live transport |
| `property.serial_timeout_ms` | No | `I64` or `TimeInterval` | Serial read window after each command; default 500 ms |
| `property.product` | No | string | Persistent product/model label |
| `property.serial_number` | No | string | Persistent serial label |
| `property.expose_z` | No | `Bool` | Expose the optional one-axis Z device |
| `property.z_axis` | No | `String` | Controller axis for the Z device, `A`, `B`, or `C` |
| `property.x`, `property.y`, `property.z` | No | `Position` | Initial configured positions |
| `property.x_travel`, `property.y_travel`, `property.z_travel` | No | `Position` | Configured travel limits |
| `property.step_size` | No | `Position` | Controller step size used for position-to-step conversion |
| `property.high_speed`, `property.low_speed` | No | `I64` | Native controller pulses-per-second settings |
| `property.acceleration_time` | No | `TimeInterval` | Native acceleration time |
| `property.busy_reply`, `property.position_reply` | No | string | Configured cached raw replies when not connected; known position-reply segments seed typed positions |

Runtime reads of `busy_reply` and `position_reply` issue the mapped query
before returning cached raw text. Motion, home, and stop paths request
position readbacks for the addressed axes until the known `D`/`H` moving/homing
state characters clear or the configured poll count is exhausted, then refresh the
busy reply when the serial transport is connected. Known position replies parse
signed controller steps plus one state character per queried axis into typed
public positions through `step_size`. Broader error handling and hardware-tuned
completion timing need hardware traces or documentation.

## Examples

| Example | Demonstrates |
| --- | --- |
| `discover_devices` | Shows a configured Chuo QT controller in the two-stage discovery flow |
| `motion_stage` | Generic stage selection can use the XY/Z stage devices, including software timing-plan endpoint application |

## Remaining Work

| Area | Gap |
| --- | --- |
| Primary command manual | Pin the exact downloadable QT controller manual/command-list revision used for active writes |
| Configured serial | Current live path requires configured `serial_port`, caches a non-empty startup identification banner, and can repeat busy/position queries through hub refresh commands |
| Completion | Hardware-tune position-state polling, validate settled-position behavior, and add error-code handling |
| Timing | Runtime timing plans apply first/last X/Y/Z endpoints through software moves; synchronized or hardware-triggered timing remains unvalidated |
| Motion safety | Validate limits, homing behavior, stop behavior, controller alarms, and post-move position readback on real hardware |
| Speed calibration | Keep `high_speed` and `low_speed` as native controller pulses/s when physical velocity conversion is not validated for a stage/controller pair |
