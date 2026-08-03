# Bluebox Optics niji

## Status

| Field | Value |
| --- | --- |
| Driver module | `numanager_drivers::bluebox_niji` |
| Families | Bluebox Optics niji LED illuminator, firmware version `V2.101.000` or newer according to reverse engineered evidence |
| Support level | Opt-in serial startup query, output control, runtime timing endpoints, status/temperature/readback refresh helpers, and known-prefix readback parsing after writes |
| Protocol evidence | Reverse engineered serial command evidence |
| Transport | Serial ASCII, 9600 baud, 8 data bits, no parity, 1 stop bit, CRLF line ending |
| Discovery | Configured discovery; optional serial connection from config with startup status and temperature queries |
| Validation | Configured-state path and opt-in serial backend compile; real controller validation pending |
| Evidence gaps | Broader command reply/error parsing, output safety, hardware temperature validation, and secondary-port state synchronization need hardware traces or documentation |

## Logical Devices

| Device | Kind tags | Role |
| --- | --- | --- |
| `niji-hub` | `hub`, `light.engine`, `shutter`, `serial.ascii` | Owns the serial session, global shutter, global intensity, trigger mode, output mode, status, and temperature readback |
| `niji-channel-1..7` | `light.source`, `led.channel`, `trigger.sink` | Individual LED channels with state, selection, intensity, wavelength, and label |

## Resources

| Resource | Kind | Notes |
| --- | --- | --- |
| `niji-serial` | `serial.ascii` | Shared CRLF-terminated command session for channel state/intensity, TTL configuration, output mode, status, and temperature readback; resource metadata records configured `serial_port`, fixed `baud_rate`, `serial_timeout`, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing |
| --- | --- | --- | --- | --- | --- |
| `TriggerSink` | Hub | `CapabilityRequest::Trigger` or `None` | `Bool` or state map | Serial write plus configured line-read window and `?` status refresh when `connect = true`; configured acceptance otherwise | `enabled` and `global_intensity` are sequenceable; runtime timing-plan start/stop applies first/last endpoints through the same global write paths |
| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_status`, or `refresh_temperatures` with no params | raw reply string or readback map | Sends only the documented status and temperature queries, caches raw replies, and updates known-prefix firmware/status/temperature readbacks when recognized; configured acceptance otherwise | No |
| `TriggerSink` | Channel | `CapabilityRequest::Trigger` or `None` | `Bool` | Serial write plus configured line-read window and `?` status refresh when `connect = true`; configured acceptance otherwise | `enabled` and `selected` are sequenceable; runtime timing-plan start/stop applies first/last endpoints through the same channel state path |
| `Dac` | Channel | `CapabilityRequest::Dac` with `Ratio` | `Ratio` | Serial write plus configured line-read window and `?` status refresh when `connect = true`; configured acceptance otherwise | `intensity` is sequenceable; runtime timing-plan start/stop applies first/last endpoints through the same channel intensity path |

## Properties

| Property | Device | Type | Unit | Access | Range/enums | Sequenceable | Mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `product` | Hub | `String` | none | R | configured product string | No | Discovery-lock identity |
| `serial_number` | Hub | `String` | none | R | configured identity | No | Discovery-lock identity |
| `serial_port` | Hub | `String` | none | R | configured port or empty | No | Serial resource label |
| `connected` | Hub | `Bool` | none | R | true when the opt-in serial port is open | No | Runtime transport state |
| `serial_timeout` | Hub | `TimeInterval` | ms | R | configured serial read window | No | Config metadata |
| `firmware_version` | Hub | `String` | none | R | configured value, or a non-empty `Firmware,` reply line after refresh/startup when connected | No | Startup/status query |
| `enabled` | Hub | `Bool` | none | R/W | `true`/`false` | Yes | Global shutter state |
| `global_intensity` | Hub | `Ratio` | percent | R/W | `0..=100` | Yes | Master percentage multiplier applied to all channel intensity commands |
| `trigger_source` | Hub | `String` | none | R/W | `Internal`, `External` | No | TTL source |
| `trigger_logic` | Hub | `String` | none | R/W | `ActiveLow`, `ActiveHigh` | No | TTL polarity |
| `trigger_resistor` | Hub | `String` | none | R/W | `PullDown`, `PullUp` | No | TTL resistor mode |
| `output_mode` | Hub | `String` | none | R/W | `ConstantCurrent`, `ConstantOpticalPower` | No | Output regulation mode |
| `output_temperature` | Hub | `Temperature` | C | R | configured value or parsed `R,` reply first scalar | No | Output temperature reply |
| `ambient_temperature` | Hub | `Temperature` | C | R | configured value or parsed `R2,` reply first scalar | No | Ambient temperature reply |
| `error_code` | Hub | `I64` | none | R | configured value or parsed `Status,` reply first integer | No | Raw status code when full error vocabulary is not hardware-validated |
| `fault` | Hub | `Bool` | none | R | true when `error_code != 0` | No | Shared safety summary input |
| `interlock_closed` | Hub | `Bool` | none | R | false when `error_code != 0` | No | Shared safety summary input |
| `status_reply` | Hub | `String` | none | R | cached raw reply | No | `?` status query |
| `temperature_reply` | Hub | `String` | none | R | cached raw reply | No | `r` temperature query |
| `last_transaction` | Hub | `Map` | none | R | action, completion basis, encoded length, live serial flag, reply text | No | Diagnostic transaction summary without exposing command text |
| `enabled` | Channel | `Bool` | none | R/W | `true`/`false` | Yes | Channel output state |
| `selected` | Channel | `Bool` | none | R/W | `true`/`false` | Yes | Channel selection alias |
| `intensity` | Channel | `Ratio` | percent | R/W | `0..=100` | Yes | Per-channel percentage before global multiplier |
| `wavelength` | Channel | `Wavelength` | nm | R | configured channel wavelength | No | Adapter hard-coded channel labels |
| `label` | Channel | `String` | none | R | configured channel label | No | Human-readable emission/filter label |

## Config

| Key | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "bluebox_niji"` | Yes | string | Selects the niji configured provider |
| `serial_port` | Required when `connect = true` | string | Serial port path/name |
| `connect` | No | `Bool` | Open the serial port and send commands through the live transport |
| `serial_timeout_ms` | No | `I64` or `TimeInterval` | Serial read window after each command; default 500 ms |
| `product`, `serial_number`, `firmware_version` | No | string | Discovery-lock identity and descriptive metadata |
| `enabled` | No | `Bool` | Initial global shutter state |
| `global_intensity` | No | `Ratio` | Initial master intensity |
| `trigger_source`, `trigger_logic`, `trigger_resistor`, `output_mode` | No | string enum | Initial trigger/output configuration |
| `status_reply`, `temperature_reply` | No | string | Configured cached raw replies when not connected; known prefixes seed typed firmware/status/temperature readbacks |
| `channel_1_enabled..channel_7_enabled` | No | `Bool` | Initial channel states |
| `channel_1_intensity..channel_7_intensity` | No | `Ratio` | Initial channel intensities |
| `channel_1_wavelength..channel_7_wavelength` | No | `Wavelength` | Configured wavelengths |
| `channel_1_label..channel_7_label` | No | string | Configured labels |

Runtime reads of `status_reply` and `temperature_reply` issue the mapped query
before returning cached raw text. Writable global output, global intensity,
trigger setup, output mode, channel enable/selection, and channel intensity
paths request the `?` status query after connected writes, updating known
`Firmware,` and `Status,` readbacks when those lines are returned. Broader
reply/error parsing and safety behavior need hardware traces.

## Examples

| Example | Coverage |
| --- | --- |
| `discover_devices` | Shows a configured Bluebox Optics niji controller in the two-stage discovery flow |
| `light_source niji` | Runs the generic light-source workflow against the configured niji devices, including typed state writes, DAC percentage, trigger-disable, timing-plan endpoint application, completion waits, readback, and events |

## Remaining Work

| Area | Needed evidence |
| --- | --- |
| Primary source | Pin a manufacturer command manual or hardware trace before expanding output control |
| Configured serial | Current live path requires configured `serial_port`; startup status and temperature queries are issued before adding the driver, write paths request status readback, and refreshes can be repeated through hub commands |
| Startup parsing | Known `Firmware,`, `Status,`, `R,`, and `R2,` reply prefixes update typed readbacks; broader status text, lockout vocabulary, calibration meaning, and reply variants require hardware traces |
| Output safety | Record low-output command output plus observed light/power readback and disable result before claiming real output support |
| Runtime timing | Runtime timing plans apply first/last hub/channel endpoints through software writes; hardware-accurate timing and secondary-port synchronization remain unvalidated |
| Status/faults | Validate lockout/error codes, status messages, and how the hardware disables output under interlock faults |
| Synchronization | Confirm secondary-port/background update behavior and channel readback semantics |
