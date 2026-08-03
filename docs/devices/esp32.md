# ESP32 Controller

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::esp32` |
| Families | Micro-Manager ESP32 firmware-compatible controllers |
| Support level | Config-backed firmware protocol plus opt-in configured real serial startup readback, control path, ADC readback, and position refresh helpers |
| Protocol evidence | Firmware-style CRLF text command surface; adapter/firmware source records `A,<channel>` analog readback |
| Transport | CRLF-terminated serial text over `SerialIo`; configured real serial uses an explicit OS serial port |
| Discovery | Simulated two-stage discovery plus config-backed discovery; configured real serial reads firmware version, X/Y/Z travel, and current position before registration |
| Validation | Configured/simulated validation and real serial backend compile; real firmware validation pending |
| Runtime/evidence notes | Real serial requires `numanager-drivers/os-serial` and explicit `connect = true` |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `esp32-hub` | `hub`, `microcontroller` | Owns one serial resource |
| `esp32-digital-out` | `digital.io`, `trigger.source` | Digital output through hub |
| `esp32-shutter` | `shutter`, `trigger.sink` | Shutter maps to digital channel 0 for timing |
| `esp32-pwm` | `analog.output`, `pwm` | PWM output through hub |
| `esp32-adc` | `analog.input`, `adc` | ADC channel 0 count readback through hub |
| `esp32-xy` | `axis.xy` | X/Y motion remultiplexed through hub |
| `esp32-z` | `axis.z` | Z motion shares hub serial resource |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `esp32-serial` | `serial` | CRLF-terminated firmware serial link for digital, PWM, and remultiplexed motion commands; configured real serial records port and connection metadata |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `DigitalIo` | Digital output | `CapabilityRequest::DigitalIo` | Map/status | Runtime token after firmware command | Digital output endpoint |
| `TriggerSource` | Digital output | `None` or `CapabilityRequest::Trigger` | Map/status | Runtime token after firmware command | Trigger source endpoint |
| `TriggerSink` | Shutter | `None` or `CapabilityRequest::Trigger` | Map/status | Runtime token after firmware command | Shutter endpoint sequences |
| `Dac` | PWM | `CapabilityRequest::Dac` | Map/status | Runtime token after firmware command | PWM endpoint sequences |
| `StageMove` | XY/Z | `CapabilityRequest::StageMove` | Moved-axis map | Runtime token after firmware command; asynchronous `W,<x>,<y>,<z>` replies update cached position properties | Position endpoint sequences through properties |
| `GenericCommand` | Hub/XY/Z | `refresh_position` or `refresh_state` with no params | State or position map | Uses only the mapped `W` position/state readback command; no raw command or setter surface | Not sequenceable |
| `GenericCommand` | ADC | `refresh_adc` with no params | ADC count map | Uses only mapped `A,0` analog readback; no raw command or setter surface | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `state_summary` | Hub | `Map` | none | R | configured state fields | No | `W,<x>,<y>,<z>` parser updates hub and XY/Z readback |
| `state` | Digital output | `Bool` | none | R/W | none | Yes | Digital command |
| `open` | Shutter | `Bool` | none | R/W | none | Yes | Digital channel 0 |
| `channel_0` | PWM | `Ratio` | percent | R/W | 0..100 | Yes | PWM command |
| `channel_0` | ADC | `I64` | count | R | `0..4095` cached/configured value | No | `A,0` readback reply `A,<count>` when connected |
| `x` | XY stage | `Position` | um | R/W | configured travel | Yes | `U`/motion command path |
| `y` | XY stage | `Position` | um | R/W | configured travel | Yes | `U`/motion command path |
| `z` | Z stage | `Position` | um | R/W | configured travel | Yes | `U`/motion command path |

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
| `driver = "esp32"` or `"mm-esp32"` | string | Yes | Selects ESP32 configured discovery |
| `firmware` | string | No | Configured firmware label used before live readback |
| `x_travel`, `y_travel`, `z_travel` | `Position`, `F64`, or `I64` | No | Canonical typed travel metadata; legacy aliases `x_travel_um`, `y_travel_um`, and `z_travel_um` remain accepted |
| `pwm_channels` | integer | No | Configured PWM channel count |
| `serial_port` | string | Required for real serial | OS serial port name |
| `baud_rate` | integer | No | Defaults to `115200` |
| `serial_timeout_ms` | integer | No | Defaults to `500` |
| `connect` | bool | No | When true, opens the configured serial port behind `numanager-drivers/os-serial`, reads `V`, `U,0`, `U,1`, `U,2`, and `W` replies before registration, uses `W` replies for connected position/state reads, and uses `A,0` for connected ADC reads |

The hub and stage `GenericCommand` capabilities expose named read-only
refresh helpers over the mapped `W` position/state readback. The ADC
`GenericCommand` exposes only `refresh_adc`, mapped to `A,0`. They do not expose
raw firmware commands, setter commands, or port discovery.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- motion_stage` | Generic XY/Z `StageMove`, typed position properties, remultiplexed state set, `Runtime::wait_completed`, and timing-plan endpoint application |
| `cargo run -p numanager-examples -- digital_io esp32` | Generic `DigitalIo`, `TriggerSource`, `TriggerSink`, and `Dac` workflow with `Runtime::wait_completed`, typed property readback, and events |
| `cargo run -p numanager-examples -- shutter esp32` | Generic shutter workflow for the ESP32 shutter/trigger-sink device |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate firmware command grammar, position parsing, and completion against real devices |
| Motion | Add hardware status/limit handling and real hardware validation of motion sequence timing |
| ADC | Validate ADC count range, pin mapping, and pull-up/digital-input interactions on hardware |
| Safety | Electrical output limits, PWM/shutter polarity, and fault states |
