# OpenUC2 Feather Controller

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::openuc2` |
| Families | OpenUC2 Feather controller |
| Support level | Config-backed JSON-line protocol plus opt-in configured real serial startup-readback/control path with typed wavelength/readback metadata and state refresh helper |
| Protocol evidence | Open JSON-line command surface |
| Transport | LF/CR JSON lines over `SerialIo`; configured real serial uses an explicit OS serial port |
| Discovery | Simulated two-stage discovery plus config-backed discovery; configured real serial sends `/state_get` before registration |
| Validation | Configured/simulated validation and real serial backend compile; real firmware validation pending |
| Runtime requirements | Real serial requires `numanager-drivers/os-serial` and explicit `connect = true` |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `openuc2-hub` | `hub`, `microcontroller` | Owns one serial resource |
| `openuc2-xy` | `axis.xy` | X/Y motion remultiplexed through hub |
| `openuc2-z` | `axis.z` | Z motion shares hub serial resource |
| `openuc2-laser` | `light.source`, `laser`, `trigger.sink`, `analog.output` | Laser enable/power through hub |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `openuc2-serial` | `serial.json-lines` | JSON-line serial link for motion, laser, and state readback commands; configured real serial records `serial_port`, `baud_rate`, and `connected` metadata |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `StageMove` | XY/Z | `CapabilityRequest::StageMove` | Moved-axis map | Runtime token after firmware command; asynchronous `/state_get` replies update cached position properties | Position endpoint sequences through properties |
| `GenericCommand` | Hub | `refresh_state` with no params | State summary map | Runtime token after mapped `/state_get` readback | Not sequenceable |
| `TriggerSink` | Laser | `None` or `CapabilityRequest::Trigger` | Map/status | Runtime token after firmware command | Laser enable endpoint sequences |
| `Dac` | Laser | `CapabilityRequest::Dac` | Map/status | Runtime token after firmware command | Laser power endpoint sequences |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `state_summary` | Hub | `Map` | none | R | controller/motion/laser fields | No | `/state_get` parser updates hub, XY/Z, and laser readback |
| `x` | XY stage | `Position` | um | R/W | configured travel | Yes | `/motor_act` |
| `y` | XY stage | `Position` | um | R/W | configured travel | Yes | `/motor_act` |
| `z` | Z stage | `Position` | um | R/W | configured travel | Yes | `/motor_act` |
| `enabled` | Laser | `Bool` | none | R/W | none | Yes | `/laser_act` |
| `power` | Laser | `Ratio` | percent | R/W | 0..100 | Yes | `/laser_act` |
| `wavelength` | Laser | `Wavelength` | named wavelength value | R | configured laser wavelength | No | Configured laser metadata exposed as a typed property |

## Metadata

| Key | Device | Type | Meaning |
| --- | --- | --- | --- |
| `x_travel`, `y_travel` | XY stage | `Position` | Fixture travel ranges |
| `z_travel` | Z stage | `Position` | Fixture travel range |

Legacy metadata keys `x_travel_um`, `y_travel_um`, and `z_travel_um` are
retained only as explicitly labeled `legacy_*` entries for compatibility.

## Config Keys

| Key | Type | Required | Meaning |
| --- | --- | --- | --- |
| `driver = "openuc2"` or `"open-uc2"` | string | Yes | Selects OpenUC2 configured discovery |
| `controller` | string | No | Configured controller label used before live readback |
| `x_travel`, `y_travel`, `z_travel` | `Position`, `F64`, or `I64` | No | Canonical typed travel metadata; legacy aliases `x_travel_um`, `y_travel_um`, and `z_travel_um` remain accepted |
| `laser_wavelength` | `Wavelength`, `F64`, or `I64` | No | Laser wavelength metadata |
| `serial_port` | string | Required for real serial | OS serial port name |
| `baud_rate` | integer | No | Defaults to `115200` |
| `serial_timeout_ms` | integer | No | Defaults to `500` |
| `connect` | bool | No | When true, opens the configured serial port behind `numanager-drivers/os-serial`, sends `/state_get`, and ingests the startup state before registration |

The hub `GenericCommand` capability exposes the named read-only
`refresh_state` helper over `/state_get`. It does not expose arbitrary JSON
tasks, module inventory commands, or serial discovery.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- motion_stage` | Generic XY/Z `StageMove`, typed position properties, remultiplexed state set, `Runtime::wait_completed`, and timing-plan endpoint application |
| `cargo run -p numanager-examples -- light_source openuc2` | Generic light-source `Dac` and `TriggerSink`, typed ratio power property, remultiplexed state set, `Runtime::wait_completed`, timing plan, and readback |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate JSON grammar, state-ingest replies, motion completion, and laser behavior against firmware |
| Inventory | OpenUC2 module inventory commands are not exposed because protocol or trace evidence is absent |
| Timing | Hardware trigger integration beyond output gating |
| Safety | Laser interlock, emission warnings, and fault state model |
