# Prior ProScan / OptiScan

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::prior` |
| Families | Prior ProScan/OptiScan controllers and attached peripherals |
| Support level | Configured opt-in serial control/readback for Prior motion, filter, shutter, TTL, Lumen, and refresh helpers |
| Protocol evidence | Public serial command concepts |
| Transport | Serial ASCII over `SerialIo` |
| Discovery | Config-backed discovery plus opt-in configured serial startup readback |
| Validation | Configured serial startup-readback/control path is implemented; real hardware validation pending |
| Runtime/evidence notes | `numanager-drivers/os-serial` enables configured real serial ports |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `prior-proscan-hub` | `hub`, `motion.controller`, `serial.ascii` | Owns one serial resource |
| `prior-xy-stage` | `axis.xy`, `stage.xy` | X/Y commands share controller resource |
| `prior-z-stage` | `axis.z`, `stage.z` | Z shares controller resource |
| `prior-nanoscan-z` | `axis.z`, `stage.z`, `piezo.z` | Piezo Z shares controller resource |
| `prior-filter-wheel-1` | `filter.wheel`, `state.device` | Filter wheel state through controller |
| `prior-shutter-1` | `shutter`, `light.gate`, `trigger.sink` | Shutter output through controller |
| `prior-ttl-0` | `trigger.source`, `digital.output` | TTL output through controller |
| `prior-lumen-200pro` | `light.source`, `shutter`, `trigger.sink` | Lumen output through controller |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `prior-proscan-serial` | `serial` | Serial command path shared by XY, Z, NanoScan Z, filter, shutter, TTL, and Lumen devices; resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `StageMove` | XY/Z/NanoScan Z | `CapabilityRequest::StageMove` without `MotionProfile` | Moved-axis map | Immediate ACK/error handling plus configured busy/status completion model | X/Y/Z and NanoScanZ position sequences |
| `StageHome` | XY | `None` | Home status map | Consumes ACK/error when present, then refreshes `$`, `PX`, and `PY`; cached configured state records the home position only when no reply is available | Not sequenceable |
| `StageStop` | XY/Z/NanoScan Z | `None` | Stop status map | Consumes ACK/error when present, then refreshes `$` and the addressed stage position; cached configured state clears busy only when no reply is available | Not sequenceable |
| `TriggerSource` | TTL | `None` or `CapabilityRequest::Trigger` | Level/status map | Runtime token completion | `high` sequences apply first/last values; route/participant-only timing drives high on start and low on stop |
| `TriggerSink` | Shutter/Lumen | `None` or `CapabilityRequest::Trigger` | Open/status map | Runtime token completion | `open` sequences apply first/last values; route/participant-only timing opens on start and closes on stop |
| `FilterSelect` | Filter wheel | `CapabilityRequest::FilterSelect` | Final position | Same controller path as writable `position`; fixture completes after busy/status update | Position state writes remain available for dynamic state sets |
| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_identity`, `refresh_status`, `refresh_position`, `refresh_profiles`, or `refresh_outputs` with no params | Map with command count and state summary | Uses only mapped Prior query readbacks; no arbitrary serial command, filter movement, or output-write surface | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `model` | Hub | `String` | none | R | model reply | No | Active probe/readback |
| `firmware_date` | Hub | `String` | none | R | firmware/date reply | No | `DATE` probe/readback |
| `last_ack` | Hub | `String` | none | R | last controller acknowledgement/error line | No | Immediate write acknowledgement when present |
| `fault` | Hub | `Bool` | none | R | derived from `last_ack` | No | Immediate write acknowledgement when present |
| `state_summary` | Hub | `Map` | none | R | configured state fields plus last acknowledgement/fault | No | `$` status readback updates summary |
| `busy` | Hub/motion/filter | `Bool` | none | R | none | No | `$` busy/status readback |
| `x` | XY | `Position` | um | R/W | configured travel | Yes | Stage move/readback |
| `y` | XY | `Position` | um | R/W | configured travel | Yes | Stage move/readback |
| `speed` | XY | `Ratio` | percent | R/W | 1..100 | No | `SMS` native percentage speed; not a typed physical velocity |
| `acceleration` | XY | `Ratio` | percent | R/W | fixture range | No | `SAS` native percentage acceleration; not a typed physical acceleration |
| `z` | Z/NanoScan Z | `Position` | um | R/W | configured travel | Yes | Z move/readback |
| `position_steps` | NanoScan Z | `StepCount` | steps | R | fixture range | No | `PZ` readback |
| `position` | Filter wheel | `I64` | position | R/W | fixture positions | No | Filter wheel command |
| `open` | Shutter/Lumen | `Bool` | none | R/W | none | Yes | Shutter readback uses `8,<id>`; Lumen is cached command state |
| `high` | TTL | `Bool` | none | R/W | none | Yes | `TTL,<line>,?` readback |
| `intensity` | Lumen | `Ratio` | percent | R/W | 0..100 | No | Lumen intensity command |
| `delay` | Lumen | `TimeInterval` | ms | R/W | 0..1000 ms | No | Fixture timing delay |

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- motion_stage` | Generic XY/Z `StageMove`, typed position properties, remultiplexed state set, `Runtime::wait_completed`, timing plan, stop, and XY homing where available |
| `cargo run -p numanager-examples -- filters prior` | Generic filter-wheel `FilterSelect`, position state-set write, completion waits, readback, and events |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate command grammar, module detection, status, and peripheral behavior against controllers |
| Discovery | Explicit configured endpoints are supported; module inventory reconciliation requires manufacturer documentation or captured controller replies |
| Motion | `StageMoveRequest::profile` is rejected because core `MotionProfile` uses typed physical velocity/acceleration, while Prior `SMS`/`SAS` are native percentage settings; need controller/model calibration evidence before converting between them |
| Timing | Hardware-trigger routes beyond current software position-sequence and output-gating hooks; TTL/shutter/Lumen boolean sequencing is software endpoint application only |
| Safety | Lamp/shutter/TTL polarity, travel limits, and fault handling |

## Config Keys

| Key | Type | Purpose |
| --- | --- | --- |
| `model`, `firmware_date` | `String` | Fixture/probe identity |
| `x_travel`, `y_travel`, `z_travel` | `Position` | Travel ranges for advertised stage properties |
| `step_size_xy`, `step_size_z` | `Position` | Step-to-micrometer conversion metadata |
| `wheel_positions` | `I64` | Filter wheel position count |
| `x`, `y`, `z`, `nano_z` | `Position` | Initial fixture positions |
| `xy_speed`, `xy_acceleration` | `I64` | Initial native percentage motion settings |
| `wheel_position` | `I64` | Initial filter wheel position |
| `shutter_open`, `ttl_high`, `lumen_open`, `busy` | `Bool` | Initial digital/status state |
| `lumen_intensity`, `lumen_delay` | `I64` / `TimeInterval` | Initial Lumen output level and delay |
| `serial_port`, `baud_rate`, `serial_timeout_ms`, `connect` | mixed | Real serial endpoint metadata; `connect = true` requires `numanager-drivers/os-serial` |

Legacy scalar aliases `x_travel_um`, `y_travel_um`, `z_travel_um`,
`step_size_xy_um`, `step_size_z_um`, `x_um`, `y_um`, `z_um`, `nano_z_um`, and
`lumen_delay_ms` are still accepted in config for existing fixtures. New config
should use the typed keys above.

When `connect = true`, discovery opens the configured serial endpoint, runs the
configured startup-readback script, and seeds cached identity, position, Z resolution,
shutter/TTL state, and busy state from controller replies before registering
the driver.
Writable motion, filter-wheel, shutter, TTL, Lumen, speed, acceleration, home,
and stop paths consume an immediate controller acknowledgement when one is
available. `R`/`0` acknowledgements are cached as `last_ack`; `E...` replies are
cached, set `fault`, and fail the operation.
Home and stop invocations additionally issue mapped status and position queries
after a live acknowledgement. If no acknowledgement is available, they retain
the cached configured behavior and do not issue follow-up queries that could
consume a delayed acknowledgement as query data.

The hub `GenericCommand` capability exposes read-only refresh helpers.
Each helper issues the same mapped query commands used by runtime property
reads for firmware date, busy/status, XY/Z/NanoScan Z position, XY native
profile percentages, shutter state, and TTL state. It does not expose filter
movement, Lumen writes, raw command strings, or cached-only acknowledgement
state as refresh operations.
