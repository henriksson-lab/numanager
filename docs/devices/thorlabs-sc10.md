# Thorlabs SC10

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::thorlabs_sc10` |
| Families | Thorlabs SC10 shutter controller with compatible shutter heads |
| Support level | Configured shutter model plus configured opt-in serial startup-readback, typed command/query path, and read-only refresh helpers |
| Protocol evidence | Thorlabs SC10 software/support page, SC10/SH05 operating manual command-line interface, and Micro-Manager SC10 serial-setting notes |
| Transport | RS-232 ASCII, 9600 baud, 8 data bits, no parity, 1 stop bit, no flow control, CR command terminator, `>` prompt completion |
| Discovery | Config-backed fixture and `discover_devices` two-stage candidate; configured real serial construction runs identity and shutter-state readbacks before registration |
| Validation | Configured-state path and opt-in serial backend compile; real SC10 hardware validation pending |
| Runtime/evidence notes | Configured state model plus configured serial construction through `numanager-drivers/os-serial` |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `thorlabs-sc10-controller` | `hub`, `shutter.controller`, `serial.ascii` | Owns the serial command path and controller identity/readback |
| `thorlabs-sc10-shutter` | `shutter`, `light.gate`, `trigger.sink` | Logical shutter endpoint remultiplexed through the controller serial resource |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `thorlabs-sc10-serial` | `serial.ascii` | SC10 controller command/readback path; configured state completes from cached values, configured serial startup and commands complete from prompt/readback; resource metadata records configured `serial_port`, fixed `baud_rate`, `serial_timeout`, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `GenericCommand` | `thorlabs-sc10-controller` | `refresh_readbacks`, `refresh_identity`, `refresh_status`, `refresh_timing`, or `refresh_open` with no params | Map with command count and state summary | Uses only mapped SC10 query readbacks; no arbitrary prompt command, save command, or state-changing toggle surface | Not sequenceable |
| `TriggerSink` | `thorlabs-sc10-shutter` | `None` or `CapabilityRequest::Trigger(TriggerRequest)` | `Bool` final open state | Configured path completes after local readback update; configured serial queries `ens?`, sends `ens` only when the readback differs, then completes after `>` prompt and final `ens?` readback | Runtime timing plans apply first/last sequence values for documented shutter endpoints through the same property write/readback paths |

The controller `GenericCommand` capability exposes read-only refresh helpers over the mapped query set. It does not expose a raw SC10 prompt command path.

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `model` | Controller | `String` | none | R | configured or `*idn?` readback | No | `*idn?` model field |
| `serial_number` | Controller | `String` | none | R | configured identity | No | configured inventory; real unit identity handling is not recorded |
| `firmware_version` | Controller | `String` | none | R | configured or `*idn?` readback | No | `*idn?` firmware field |
| `serial_settings` | Controller | `String` | none | R | `9600 8N1 no-flow` | No | RS-232 setup from manual/Micro-Manager notes |
| `open` | Shutter | `Bool` | none | R/W | `0`/`1` readback | Yes | `ens?`; writes query first, issue `ens` toggle only if needed, then query `ens?` again |
| `mode` | Shutter | `String` | none | R/W | `Manual`, `Auto`, `Single`, `Repeat`, `ExternalGate` | Yes | `mode?`; `mode=1..5` |
| `open_time` | Shutter | `TimeInterval` | named time value | R/W | `1..=999999 ms` | Yes | `open?`; `open=<milliseconds>` |
| `close_time` | Shutter | `TimeInterval` | named time value | R/W | `1..=999999 ms` | Yes | `shut?`; `shut=<milliseconds>` |
| `trigger_mode` | Shutter | `String` | none | R/W | `Internal`, `External` | Yes | `trig?`; `trig=0` internal, `trig=1` external |
| `repeat_count` | Shutter | `I64` | count | R/W | `1..=99` | Yes | `rep?`; `rep=<count>` |
| `interlock_closed` | Shutter | `Bool` | none | R | configured/readback only | No | Manual documents interlock display/safety behavior but no stable CLI query is exposed here; driver does not synthesize changes |
| `fault` | Shutter | `Bool` | none | R | configured/readback only | No | Manual documents alarm display/safety behavior but no stable CLI query is exposed here; driver does not synthesize changes |
| `state_summary` | Shutter | `Map` | none | R | current public shutter state | No | Active path refreshes `ens?`, `mode?`, `open?`, `shut?`, `trig?`, and `rep?`; safety fields are cached configured/readback values only |

## Config Keys

| Key | Type | Status | Meaning |
| --- | --- | --- | --- |
| `driver = "thorlabs_sc10"` | string | Canonical | Selects config-backed SC10 discovery |
| `driver = "thorlabs-sc10"` or `driver = "sc10"` | string | Alias | Accepted discovery aliases |
| `model` | string | Canonical | Configured model label |
| `serial_number` | string | Canonical | Configured controller serial number |
| `firmware_version` | string | Canonical | Configured firmware string |
| `mode` | string enum | Canonical | Initial shutter mode |
| `open` | bool | Canonical | Initial shutter open/closed readback |
| `enabled` | bool | Alias | Legacy alias for initial shutter open/closed readback |
| `open_time`, `close_time` | `TimeInterval` | Canonical | Initial opening and closing timing values |
| `trigger_mode` | string enum | Canonical | Initial internal/external trigger source |
| `repeat_count` | integer | Canonical | Initial repeat count, expected `1..=99` |
| `interlock_closed`, `fault` | bool | Canonical | Configured safety readbacks |
| `serial_port` | string | Optional | OS serial port used when `connect = true` |
| `serial_timeout_ms` | integer | Optional | OS serial read timeout, default `500` |
| `connect` | bool | Optional | When true, construct an active RS-232 transport behind `numanager-drivers/os-serial` and read `*idn?`, `ens?`, `mode?`, `open?`, `shut?`, `trig?`, and `rep?` before registration |

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- discover_devices` | Two-stage discovery candidate and add flow for the configured SC10 controller |
| `cargo run -p numanager-examples -- shutter sc10` | Generic shutter workflow: typed properties, setup state set, `TriggerSink` open/pulse/close, `Runtime::wait_completed`, individual readback, final `state_summary` readback, and events |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Record construction-time `*idn?`/state readbacks, query/write/readback cycles, prompt timing, and observable shutter state on a real controller |
| Safety | Keep interlock and alarm as reported/readback-only when no documented CLI source or hardware capture identifies stable query replies |
| Timing | Validate hardware-synchronized timing, open/close timing units, repeat behavior, external-gate behavior, and observable runtime timing-plan transitions on a real controller |
| Discovery | Keep RS-232 setup configured by explicit port; document any identity readback observed during hardware validation |
