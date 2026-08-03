# Agilent Laser Combiner

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::agilent_laser_combiner` |
| Families | Agilent/Keysight laser combiner controllers |
| Support level | Implemented from external protocol evidence with typed control paths and mapped readback helpers |
| Evidence | Reverse engineered |
| Transport | Serial (COM) at 115200 8N1; see [`../reverse/agilent-laser-combiner-protocol.md`](../reverse/agilent-laser-combiner-protocol.md) |
| Protocol | Binary request / ASCII CRLF reply, command byte echoed in reply; full opcode table recovered |
| Validation | No numanager hardware validation note yet |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `agilent-combiner-hub` | `hub`, `light.engine` | Owns one physical combiner controller |
| `agilent-laser-line-*` | `light.source`, `laser`, `trigger.sink` | Per-line logical devices remultiplexed through the combiner controller |
| `agilent-analog-output-1..4` | `analog.output`, `diagnostic.raw` | Diagnostic raw analog-output channels remultiplexed through the combiner controller |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `agilent-combiner-transport` | `serial` | Single COM port at 115200 8N1; all logical lines remultiplex onto it; resource metadata records configured `serial_port`, fixed `baud_rate`, fixed `serial_timeout`, and `connected` state |
| Safety surface | none | No interlock/fault command exists in the recovered protocol; do not synthesize one |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `Dac` | Laser line | `CapabilityRequest::Dac` with `Ratio` or `OpticalPower` | Applied output setpoint | Command echo reply | Runtime timing plans apply first/last `intensity` or `power` endpoints through the same line-power request/reply path; hardware sequence opcodes are not exposed as public timing support |
| `TriggerSink` | Laser line or hub | `CapabilityRequest::Trigger` or `None` | Enabled line state or hub shutter state | Command echo reply | Runtime timing plans apply first/last line `enabled` endpoints through the same state-mask request/reply path; hub shutter and hardware sequence timing are not exposed as public timing support |
| `GenericCommand` | Hub | `refresh_identity`, `refresh_control_state`, `refresh_line_outputs`, or `refresh_line_metadata` with no params | Refreshed state map | Uses only typed request/reply getter paths already represented as properties; no register, EEPROM, AOTF, or hardware sequence command surface | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `model` | Hub | `String` | none | R | `"LUn8"`, `"LU-N4"`, others | No | Cmd `0x01`, ASCII reply |
| `firmware_version` | Hub | `String` | none | R | Compared against `"0.12"` by the SDK | No | Cmd `0x02`, ASCII reply |
| `hardware_version` | Hub | `String` | none | R | Board-reported text | No | Cmd `0x05`, ASCII reply |
| `serial_number` | Hub | `String` | none | R/W | NUL-terminated, max 64 bytes | No | Read cmd `0x04`; write cmd `0x5B`, 400 ms post-write delay in SDK |
| `line_count` | Hub | `I64` | none | R | `1..=8` in driver | No | Cmd `0x36`, ASCII integer |
| `state_mask` | Hub | `I64` | none | R | Board-wide enabled-line bitmask | No | Cmd `0x28`, ASCII integer |
| `shutter_open` | Hub | `Bool` | none | R/W | `0`/`1` | No | Read cmd `0x2E`; write cmd `0x10` |
| `external_control_enabled` | Hub | `Bool` | none | R/W | `0`/`1` | No | Read cmd `0x2B`; write cmd `0x0D` |
| `blanking_enabled` | Hub | `Bool` | none | R/W | `0`/`1` | No | Read cmd `0x2C`; write cmd `0x0E` |
| `sync_mode` | Hub | `I64` | none | R/W | `0..=255` | No | Read cmd `0x2D`; write cmd `0x0F` |
| `galvo_position` | Hub | `I64` | none | R/W | `0..=255` | No | Read cmd `0x2F`; write cmd `0x11` |
| `nd_filter_state` | Hub | `I64` | none | R/W | `0..=255` | No | Read cmd `0x30`; write cmd `0x12` |
| `nd_filter_mapping` | Hub | `I64` | none | R/W | `0..=255` | No | Read cmd `0x31`; write cmd `0x13` |
| `direct_amplitude` | Hub | `Ratio` | percent | R/W | `0..=100` via first line DAC range | No | Read cmd `0x32`; write cmd `0x14` |
| `saved_direct_amplitude` | Hub | `Ratio` | percent | R | `0..=100` via first line DAC range | No | Cmd `0x33`, ASCII integer |
| `last_transaction` | Hub | `Map` | none | R | Runtime transaction summary | No | Runtime-maintained |
| `interlock_closed` | Hub | `Bool` | none | R | No such command exists in the protocol | No | Not exposed; do not advertise |
| `fault` | Hub/line | `String` | none | R | No such command exists in the protocol | No | Not exposed; do not advertise |
| `wavelength` | Laser line | `Wavelength` | nm | R/W | Per-line, integer nm | No | Read cmd `0x3A`; write cmd `0x58` as 16-bit big-endian |
| `enabled` | Laser line | `Bool` | none | R/W | Bit *i* of a board-wide mask | Yes | Read cmd `0x28`, write cmd `0x0A` (whole mask) |
| `intensity` | Laser line | `Ratio` | percent | R/W | `counts / ((1 << bit_depth) - 1)` | Yes | Read cmd `0x29`; write cmd `0x0B` as 16-bit big-endian DAC counts |
| `power` | Laser line | `OpticalPower` | mW | R/W | Per-line max power and 11-point calibration curve | Yes | Derived host-side from counts → volts → mW; no direct mW command |
| `min_voltage` | Laser line | `Voltage` | V | R | Per line | No | Cmd `0x37`, ASCII float |
| `max_voltage` | Laser line | `Voltage` | V | R | Per line | No | Cmd `0x38`, ASCII float |
| `dac_bit_depth` | Laser line | `I64` | none | R | `0..=16` in driver | No | Cmd `0x39`, ASCII integer |
| `max_power` | Laser line | `OpticalPower` | mW | R | Per line | No | Cmd `0x3B`, ASCII float |
| `calibration` | Laser line | `List<F64>` | none | R | 11 coefficients | No | Cmd `0x3C` for coefficient indexes `0..10` |
| `raw_counts` | Analog output channels | `I64` | counts | R/W | `0..=65535` | No | Read cmd `0x2A`; write cmd `0x0C` |

## Evidence Gate

| Claim | Current evidence | Default driver decision |
| --- | --- | --- |
| Controller/channel identity | Probe command `0x03` with expected reply `"My100xBoard"`; model/serial/firmware via `0x01`/`0x04`/`0x02`; line count via `0x36` | Serializable, but unconfirmed on hardware; no VID/PID so discovery cannot be deterministic |
| Output enable/shutter/control | Full opcode table recovered for state, blanking, shutter, external control, sync, sequence, analog output, and power | Implement typed properties, `TriggerSink`/`Dac`, diagnostic raw analog-output counts, and mapped readback helpers; hardware validation remains required before claiming support on a physical setup |
| Power/intensity units | Wire values are raw DAC counts; SDK converts counts → volts → mW using bit depth, min/max volts, and an 11-point calibration curve | Use `Ratio` and `OpticalPower`; convert to raw DAC counts only at the wire boundary |
| Safety/interlocks/faults | **No interlock, emission-permitted, or fault command exists anywhere in the 61-export surface** | Do not advertise synthetic `interlock_closed` or `fault`; safety must be external to this protocol |
| Completion | Every command is a blocking request/reply with no busy flag or event channel | Runtime completion is the echoed command reply; no extra user wait/readback step is required |

Analog-output channels expose diagnostic raw DAC counts only. The recovered
protocol identifies the channel/count payload and readback command, but this
driver does not claim a calibrated voltage range for those outputs.

## Examples

| Example | Demonstrates |
| --- | --- |
| Generic light-source workflows | Use line devices through `Dac`, `TriggerSink`, `enabled`, `intensity`, and `power` properties |

## Remaining Work

| Area | Gap |
| --- | --- |
| Transport | Recovered (serial 115200 8N1); still need a hardware trace confirming framing and reply latency |
| Safety | Protocol exposes no interlock/fault channel; safety must come from wiring or a different surface before any output write |
| Units | Counts/volts/mW conversion recovered; needs one hardware cross-check of a known optical power against the calibration curve |
| Discovery | No VID/PID in the artifact; external evidence includes a COM-range identity scan, but numanager requires a configured serial endpoint |
| Low-level side-effect gaps | External evidence records payload byte order and sequence layout, but register/EEPROM/AOTF meanings, board-side side effects, accepted sequence limits, persistence behavior, and safety behavior remain unvalidated |
| Driver | Keep serial connection explicitly configured; add hardware-validation notes once a real board is available |
| Timing | Runtime timing plans apply software first/last line `enabled`, `intensity`, and `power` endpoints; hardware sequence opcodes, accepted sequence limits, and trigger/sync timing are not exposed because their behavior is not defined by current protocol evidence |

## Unblock Trace Checklist

Use the HID or USB vendor/bulk sections of
[`../reverse/trace-capture-guide.md`](../reverse/trace-capture-guide.md) when
collecting these observations.

| Trace item | Must record |
| --- | --- |
| Hardware identity | Controller model, firmware, vendor SDK version, attached laser-line wavelengths, interlock wiring/state, and OS USB descriptor identity |
| Transport classification | USB descriptors, endpoint/report layout, or serial settings that prove HID, vendor USB, bulk, or serial-over-USB transport |
| Discovery | Raw traffic for controller identity, laser-line count, wavelength/channel metadata, safety/interlock status, and the matching discovered-device output |
| Safe output | A minimum safe enable/intensity or power write, observed requested level, hold duration, readback/status reply, explicit disable/shutter-close result, and the matching command-completion output |
| Units | Side-by-side SDK/API value, observed protocol payload, and printed runtime value for one relative-intensity or optical-power command so `Ratio` versus `OpticalPower` is not guessed |
| Fault path | Interlock-open or emission-blocked status traffic, plus the runtime-visible failed operation output that should fail or block output completion |
