# Spectral LMM5

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::spectral_lmm5` |
| Families | Spectral Applied Research Laser Merge Module LMM5 |
| Support level | Startup-readback shutter/transmission/wavelength/trigger-profile commands, hub refresh/apply helpers, and runtime timing-endpoint RS-232 control; real serial opt-in behind `os-serial` |
| Protocol evidence | Public LMM5 user/software manual documents the RS-232 hexadecimal command protocol, serial settings, shutter control/status, per-line transmission, wavelength readback, and trigger configuration commands |
| Transport | RS-232 hexadecimal ASCII commands, carriage-return terminated, 19200 baud, 8 data bits, no parity, 1 stop bit |
| Discovery | Config-backed two-stage discovery; real serial construction reads shutter status and wavelengths before registering the driver |
| Validation | No hardware validation |
| Runtime/evidence notes | Real serial requires `numanager-drivers/os-serial`; USB/HID control, trigger timing validation, interlocks, and fault/status coverage need hardware traces or documentation |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `spectral-lmm5-hub` | `hub`, `light.engine`, `serial.ascii.hex` | One controller resource owns all laser-line shutters and trigger configuration |
| `spectral-lmm5-line-N` | `light.source`, `laser.line`, `shutter`, `trigger.sink` | Per-line logical light source remultiplexed through the shared shutter mask and per-line transmission command |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `spectral-lmm5-serial` | `serial.ascii.hex` | Sends the hex command families used by this driver and reads command ACK/status replies |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `Dac` | Laser line | `CapabilityRequest::Dac(DacRequest { value: Ratio })` | Final `Ratio` | Command ACK for per-line transmission setpoint; optical calibration and fault readback need hardware traces or documentation | `transmission` is sequenceable; runtime timing-plan start/stop applies first/last endpoints through the same per-line transmission path |
| `TriggerSink` | Laser line | `CapabilityRequest::Trigger` or `None` | State map | Shutter-mask command ACK plus shutter-status readback | `enabled` is sequenceable; runtime timing-plan start/stop applies first/last endpoints through the same shutter-mask path |
| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_shutter_status`, `refresh_wavelengths`, `apply_trigger_in`, `apply_trigger_out`, or `apply_trigger_profiles` with no params | Status, wavelength, or trigger-profile map | Uses only documented LMM5 shutter-status, wavelength-readback, and trigger-configure command paths; no arbitrary hex command surface | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `product` | Hub | `String` | none | R | configured product label | No | Config/probe metadata |
| `serial_number` | Hub | `String` | none | R | configured serial label | No | Config/probe metadata |
| `protocol` | Hub | `String` | none | R | Spectral LMM5 RS-232 hex protocol | No | Protocol metadata |
| `line_count` | Hub | `I64` | count | R | `1..8` | No | Config/probe metadata |
| `shutter_mask` | Hub | `I64` | bit mask | R/W | `0..255` | No | Shutter control/status command |
| `trigger_in_enabled` | Hub | `Bool` | none | R/W | true/false | No | Trigger-in configure command enable byte |
| `trigger_in_count` | Hub | `I64` | count | R/W | `0..255` | No | Trigger-in configure count-before-action byte |
| `trigger_in_cycle` | Hub | `Bool` | none | R/W | true/false | No | Trigger-in configure cycle-mode byte |
| `trigger_out_enabled` | Hub | `Bool` | none | R/W | true/false | No | Trigger-out configure enable byte |
| `trigger_out_clock` | Hub | `Bool` | none | R/W | true/false | No | Trigger-out configure clock-mode byte |
| `trigger_out_interval` | Hub | `TimeInterval` | ms | R/W | `0..6553.5 ms`, encoded in 0.1 ms increments | No | Trigger-out configure interval field |
| `last_transaction` | Hub | `Map` | none | R | command, line count, shutter mask, completion basis | No | Runtime transaction cache |
| `line` | Laser line | `I64` | ordinal | R | `1..8` | No | Logical line index |
| `wavelength` | Laser line | `Wavelength` | nm | R | configured/readback wavelength, or `Null` when absent | No | Wavelength readback command |
| `enabled` | Laser line | `Bool` | none | R/W | true/false | Yes | Shared shutter mask |
| `transmission` | Laser line | `Ratio` | percent | R/W | `0..100 percent` | Yes | Per-line transmission command, 0..1000 device scale |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "spectral_lmm5"` or `"lmm5"` | Yes | string | Selects the Spectral LMM5 provider |
| `property.product` | No | string | Persistent product/model label |
| `property.serial_number` | No | string | Persistent serial label |
| `property.line_count` | No | `I64` | Number of laser lines to expose, `1..8` |
| `property.line_N_wavelength` | No | `Wavelength` | Wavelength metadata for line `N` |
| `property.line_N_transmission` | No | `Ratio` | Initial transmission for line `N` |
| `property.shutter_mask` | No | `I64` | Initial shutter bit mask |
| `property.trigger_in_enabled` | No | `Bool` | Initial trigger-in configuration state |
| `property.trigger_in_count` | No | `I64` | Initial trigger-in count-before-action byte |
| `property.trigger_in_cycle` | No | `Bool` | Initial trigger-in cycle-mode state |
| `property.trigger_out_enabled` | No | `Bool` | Initial trigger-out configuration state |
| `property.trigger_out_clock` | No | `Bool` | Initial trigger-out clock-mode state |
| `property.trigger_out_interval` | No | `TimeInterval` | Initial trigger-out interval, encoded in 0.1 ms increments |
| `property.serial_port` | For real serial | string | OS serial port name; also recorded in resource metadata |
| `property.connect` | No | `Bool` | If true, opens the configured serial port behind `os-serial`, reads startup shutter status and wavelengths, and otherwise uses the configured state model |

## Examples

| Example | Demonstrates |
| --- | --- |
| `discover_devices` | Shows a configured Spectral LMM5 in the two-stage discovery flow |
| `light_source` | Generic light-source selection, typed `Dac` percent output, trigger/shutter control, timing-plan endpoint application, and safety summary |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Record startup shutter-status and wavelength replies, command stdout/stderr, ACK/status replies, requested low-output level, observable output, shutter disable result, and fault/error behavior |
| Trigger timing | Validate trigger profile timing semantics, edge behavior, and interaction with shutter/output state on real hardware |
| Runtime timing | Runtime timing plans apply first/last `enabled` and `transmission` endpoints through software writes; hardware trigger timing remains unvalidated |
| USB/HID | A real USB/HID backend is not exposed without a documented transport abstraction and trace evidence |
| Protocol expansion | Current RS-232 command coverage includes shutter mask, shutter-status readback, per-line transmission, wavelength readback, trigger profile configuration, and hub refresh/apply helpers. USB/HID, interlock/fault/status, and additional command families are not exposed without manufacturer documentation, public protocol evidence, or hardware traces |
| Safety | Validate shutter-status readback, error ACK behavior, interlock/fault availability, and safe disable behavior on real hardware |
