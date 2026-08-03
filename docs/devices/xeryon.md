# Xeryon ASCII Piezo Stages

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::xeryon` |
| Families | Xeryon XD-M, XD-C, and XD-OEM-class controllers using the documented ASCII-over-serial command interface for XLS/XVS/XRT-U/XVP-style stages |
| Support level | Configured single-axis stage control with optional real serial backend, typed absolute/relative motion, home-to-zero, stop, velocity setting, position/target/status readback, and refresh helpers |
| Protocol evidence | Xeryon controller manuals document ASCII line framing, axis prefixes, command tags, `=?` readback syntax, units, feedback tags, and status bits |
| Transport | USB virtual COM / RS232 / UART ASCII, LF terminator, default 115200 baud, 8 data bits, 1 stop bit, no parity, no handshaking |
| Discovery | Configured discovery only; optional configured startup readback for explicitly configured serial endpoints |
| Validation | No hardware validation in this repository |
| Runtime/evidence notes | Real serial requires `numanager-drivers/os-serial`; integrated XLA/XUMU CANopen devices are covered separately by [`xeryon-canopen.md`](xeryon-canopen.md) |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `xeryon-ascii-hub` | `hub`, `motion.controller`, `serial.ascii`, `xeryon.ascii` | Owns one configured serial resource |
| `xeryon-ascii-axis-*` | `axis.*`, `stage.axis`, `motion.stage`, `xeryon.ascii.axis` | One configured logical axis; multiple physical axes should be configured as separate axis entries until a shared-port multi-axis backend is validated |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `xeryon-ascii-serial` | `serial` | Sends documented LF-terminated `X:TAG=value`, `X:TAG=?`, and no-value motion commands |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `StageMove` | Axis device | `CapabilityRequest::StageMove` with the configured axis target | Motion/status map | Sends `DPOS` for absolute or `STEP` for relative motion, then refreshes `EPOS` and `STAT` for connected serial or updates configured state locally | `position` is sequenceable through software timing endpoints |
| `StageHome` | Axis device | `None` | `"homed"` | Sends `HOME`; documented as equivalent to `DPOS=0`; hardware reference behavior remains validation-pending | Not sequenceable |
| `StageStop` | Axis device | `None` | `"stopped"` | Sends `STOP` and updates/refreshes status | Not sequenceable |
| `GenericCommand` | Axis device | `refresh_readbacks`, `refresh_position`, `refresh_target`, `refresh_velocity`, `refresh_status`, or `refresh_axis_summary` | Refreshed property map | Issues selected documented `=?` readbacks only; no arbitrary ASCII command surface | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `controller_model` | Hub | `String` | none | R | configured label | No | Config metadata |
| `serial_number` | Hub | `String` | none | R | controller serial | No | `SRNO=?` |
| `software_version` | Hub | `String` | none | R | firmware/software version | No | `SOFT=?` |
| `state_summary` | Hub | `Map` | none | R | current cached axis state | No | Runtime cache/readbacks |
| `position` | Axis | `Position` | um | R/W | configured limits | Yes | `DPOS=value` for writes, `EPOS=?` for readback |
| `target` | Axis | `Position` | um | R/W | configured limits | No | cached target / `DPOS=?` |
| `velocity` | Axis | `Velocity` | um/s | R/W | `0..500000` advertised until model-specific validation exists | No | `SSPD=value`, `SSPD=?` |
| `busy` | Axis | `Bool` | none | R | derived | No | `STAT` motor/search/scan bits |
| `indexed` | Axis | `Bool` | none | R | derived | No | `STAT` encoder-valid bit |
| `position_reached` | Axis | `Bool` | none | R | derived | No | `STAT` position-reached bit |
| `fault_active` | Axis | `Bool` | none | R | derived | No | `STAT` thermal/encoder/error-limit/timeout/fail bits |
| `status_bits` | Axis | `I64` | none | R | raw status value | No | `STAT=?` |
| `status_summary` | Axis | `Map` | none | R | decoded documented flags | No | `STAT=?` |
| `low_limit`, `high_limit` | Axis | `Position` | um | R | configured/probed limits | No | config or `LLIM=?` / `HLIM=?` |
| `encoder_unit` | Axis | `Position` | um | R | configured conversion | No | inverse of configured `encoder_units_per_um` |
| `axis_summary` | Axis | `Map` | none | R | current cached axis state | No | Runtime cache/readbacks |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "xeryon"` or `"xeryon_ascii"` | Yes | string | Selects the Xeryon ASCII provider |
| `property.axis` | No | string | Controller axis letter: `X`, `Y`, `Z`, `A`, `B`, or `C`; default `X` |
| `property.stage_model` | No | string | Persistent stage model label |
| `property.controller_model` | No | string | Persistent controller model label |
| `property.encoder_units_per_um` | Yes for physical-unit correctness | `F64`/`I64` | Native encoder increments per micrometer for the configured stage |
| `property.low_limit`, `property.high_limit` | No | `Position` | Configured travel range; legacy scalar aliases `low_limit_um` and `high_limit_um` are accepted |
| `property.position`, `property.target` | No | `Position` | Initial configured position/target |
| `property.velocity` | No | `Velocity` | Initial velocity |
| `property.serial_port` | For real serial | string | OS serial port name |
| `property.baud_rate` | No | `I64` | Defaults to `115200` |
| `property.serial_timeout_ms` | No | `I64` | Defaults to `5` |
| `property.connect` | No | `Bool` | If true, opens the configured serial port behind `os-serial` |
| `property.startup_readback` | No | `Bool` | If true with `connect`, queries `SRNO`, `SOFT`, `STAT`, `EPOS`, `DPOS`, `SSPD`, `LLIM`, and `HLIM` before registration |

`RSET`, `SAVE`, `LOAD`, tuning, signal-shaping, direction, GPIO, UART
configuration, and encoder-reset commands remain hidden maintenance or
bring-up operations. They are not exposed through properties, capabilities, or
generic commands.

## Examples

| Example | Demonstrates |
| --- | --- |
| `discover_devices` | Shows configured Xeryon ASCII devices in the two-stage discovery flow |
| `motion_stage` | Generic `StageMove`, typed position properties, software timing endpoints, stop, and home through the common stage API |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Record controller model, firmware/software version, stage model, serial settings, startup readbacks, `INDX`/`HOME` behavior, `STAT` bit behavior, limit/fault behavior, and post-motion `EPOS`/`DPOS` readbacks on real hardware |
| Multi-axis sharing | The ASCII protocol supports axis prefixes, but shared-port multi-axis scheduling should be validated before one driver owns several axes |
| Unit conversion | `encoder_units_per_um` must be configured or validated per stage family; the driver does not infer physical scaling from model names |
| CANopen integrated devices | Integrated XLA/XUMU controllers use CANopen/CiA 402 and EDS files; configured transaction planning plus optional live SocketCAN/SLCAN transport are tracked in [`xeryon-canopen.md`](xeryon-canopen.md) |
