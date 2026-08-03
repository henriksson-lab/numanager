# Teensy Pulse Generator

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::teensy_pulse` |
| Families | Teensy pulse-generator firmware |
| Support level | Config-backed binary protocol plus opt-in configured real serial startup/program readback path and enquiry refresh helpers |
| Protocol evidence | Firmware-style binary command surface |
| Transport | Fixed binary frames over `SerialIo`; configured real serial uses an explicit OS serial port |
| Discovery | Simulated two-stage discovery plus config-backed discovery; configured real serial enquires version, pulse program fields, and running/count state before registration |
| Validation | Configured/simulated validation and real serial backend compile; real firmware validation pending |
| Runtime/evidence notes | Real serial requires `numanager-drivers/os-serial` and explicit `connect = true` |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `teensy-pulse-hub` | `hub`, `microcontroller` | Owns one serial resource |
| `teensy-pulse-generator` | `pulse.generator`, `trigger.source`, `timing.source` | Pulse program and output state through serial resource |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `teensy-pulse-serial` | `serial.binary` | Binary firmware serial link for fixed-frame program, start/stop, and enquiry commands; configured real serial records port and connection metadata |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `GenericCommand` | Pulse generator | `refresh_readbacks`, `refresh_program`, `refresh_running`, or `refresh_counted_pulses` with no params | Program summary map | Uses only mapped firmware enquiry frames and decoded 5-byte replies | Not sequenceable |
| `PulseProgram` | Pulse generator | `CapabilityRequest::PulseProgram` interval/duration/wait/count setup | Program summary map | Runtime token after firmware command; binary replies update cached program fields | Program state can be applied as timing endpoints |
| `TriggerSource` | Pulse generator | `None` or `CapabilityRequest::Trigger` | Running/status map | Runtime token after firmware command; binary running/count replies update cached state | Timing-plan `running` endpoint or default start/stop |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `interval` | Pulse generator | `TimeInterval` | s/us wire conversion | R/W | fixture range | Yes | Interval command; legacy alias `interval_us` |
| `duration` | Pulse generator | `TimeInterval` | s/us wire conversion | R/W | fixture range | Yes | Duration command; legacy alias `duration_us` |
| `wait_for_input` | Pulse generator | `Bool` | none | R/W | none | Yes | Wait-for-input command |
| `number_of_pulses` | Pulse generator | `I64` | count | R/W | non-negative | Yes | Number-of-pulses command |
| `running` | Pulse generator | `Bool` | none | R/W | none | Yes | Start/stop commands |
| `program_summary` | Pulse generator | `Map` | none | R | decoded program fields | No | Enquiry/readback frames update interval, duration, wait, count, running, and counted-pulse readback |

## Config Keys

| Key | Type | Required | Meaning |
| --- | --- | --- | --- |
| `driver = "teensy_pulse"`, `"teensy-pulse"`, or `"mm-teensy-pulse"` | string | Yes | Selects Teensy Pulse configured discovery |
| `version`, `number_of_pulses`, `counted_pulses` | integer | No | Configured firmware/program state before live readback |
| `interval` or `interval_us`, `duration` or `duration_us` | `TimeInterval` or integer | No | Configured/default pulse timing |
| `wait_for_input`, `running` | bool | No | Configured/default program state |
| `serial_port` | string | Required for real serial | OS serial port name |
| `baud_rate` | integer | No | Defaults to `115200` |
| `serial_timeout_ms` | integer | No | Defaults to `500` |
| `connect` | bool | No | When true, opens the configured serial port behind `numanager-drivers/os-serial`, enquires startup state, waits for one fixed 5-byte reply per connected set/enquiry, and updates cached program fields from decoded replies |

The pulse-generator `GenericCommand` capability exposes named read-only
refresh helpers over the mapped firmware enquiry frames. It does not expose raw
binary firmware commands.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- digital_io` | Generic `PulseProgram` and `TriggerSource` workflow shape for pulse generators, including typed interval/duration properties, `Runtime::wait_completed`, timing plans, and event subscription |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate binary framing, little-endian values, ingested enquiry replies, and start/stop behavior against firmware |
| Timing | Hardware latency characterization and external-input edge behavior |
| Safety | Output electrical limits and fault/interlock model |
