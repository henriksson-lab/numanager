# TriggerScope

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::triggerscope` |
| Families | ARC TriggerScope / TriggerScope 16 timing controller |
| Support level | Opt-in serial startup-identification, direct-control commands, constrained sequence-programming commands, and timing-plan mapping |
| Protocol evidence | Reverse engineered newline-terminated serial commands for identification, TTL output, camera-trigger output, DAC output, focus output, array clearing/programming, and arming |
| Transport | Config-backed ASCII serial; direct writes open `property.serial_port` when `property.connect = true` and `os-serial` is enabled; live construction sends the identification command and caches a non-empty banner as `firmware_version` |
| Discovery | Config-backed two-stage discovery with optional serial connection |
| Validation | No hardware validation |
| Evidence gaps | Hardware timing validation, response/error parsing, exact timebase semantics, and camera-trigger sequence mapping need additional protocol evidence |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `triggerscope-hub` | `hub`, `trigger.controller`, `serial.ascii` | Owns one serial controller resource |
| `triggerscope-focus` | `axis.z`, `stage.z`, `motion.stage` | Focus position maps to the controller focus DAC scale |
| `triggerscope-cam-1..2` | `camera.trigger`, `trigger.source`, `state.device` | Camera trigger state devices |
| `triggerscope-ttl-*` | `digital.output`, `ttl.output`, `trigger.source`, `trigger.sink` | TTL lines share the same controller resource |
| `triggerscope-dac-*` | `analog.output`, `dac.output`, `trigger.sink` | DAC outputs share the same controller resource |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `triggerscope-serial` | `serial.ascii` | Runtime resource for encoded TTL, camera trigger, DAC, and focus commands; resource metadata records configured `serial_port`, fixed `baud_rate`, `serial_timeout`, and `connected` state; live serial is opened only when `property.connect = true` |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `StageMove` | `triggerscope-focus` | `CapabilityRequest::StageMove` with Z target | `Position` | Serial write plus configured line-read window when connected; configured-state update otherwise | `z` is sequenceable |
| `DigitalIo` | TTL devices | `CapabilityRequest::DigitalIo` with bit 0 as output state | `Bool` | Serial write plus configured line-read window when connected; configured-state update otherwise | `high` is sequenceable |
| `TriggerSink` / `TriggerSource` | TTL devices | `CapabilityRequest::Trigger` enable/disable/pulse | `Bool` | Serial write plus configured line-read window when connected; configured-state update otherwise | `high` is sequenceable |
| `TriggerSource` | Camera-trigger devices | `CapabilityRequest::Trigger` enable/disable/pulse | `Bool` | Serial write plus configured line-read window when connected; configured-state update otherwise | Not sequenceable; current evidence covers direct state writes only |
| `Dac` | DAC devices | `CapabilityRequest::Dac` with `Voltage` | `Voltage` | Serial write plus configured line-read window when connected; configured-state update otherwise | `voltage` is sequenceable |
| `TriggerSink` | DAC devices | `CapabilityRequest::Trigger` enable/disable/pulse | `Bool`/voltage-backed state | Serial write plus configured line-read window when connected; disable writes 0 V in runtime state | `enabled` is sequenceable |
| `GenericCommand` | Hub | `clear_ttl`, `program_ttl`, `clear_dac`, `program_dac`, `clear_focus`, `program_focus`, or `arm` | `Map` | Serial write plus configured line-read window when connected; configured-state update otherwise | Bring-up surface for evidenced sequence commands |
| Timing plan | Hub/resource | `Command::Arm` / `Start` / `Stop` with TTL `high`, DAC `voltage`, or evenly stepped focus `z` sequences | `Map` | `Arm` clears/programs arrays with constrained sequence commands; `Start` sends `ARM`; `Stop` clears cached armed state | No route/timebase claims |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `product` | Hub | `String` | none | R | configured product label | No | Config/probe metadata |
| `serial_number` | Hub | `String` | none | R | configured serial label | No | Config/probe metadata |
| `firmware_version` | Hub | `String` | none | R | configured banner | No | Identification banner |
| `software_version` | Hub | `String` | none | R | adapter software version | No | Source metadata |
| `dac_bits` | Hub | `I64` | bits | R | `12` or `16` | No | Firmware/model metadata |
| `serial_port` | Hub | `String` | none | R | configured serial port label | No | Config metadata |
| `connected` | Hub | `Bool` | none | R | configured serial transport state | No | Runtime transport state |
| `serial_timeout` | Hub | `TimeInterval` | ms | R | configured serial read window | No | Config metadata |
| `armed` | Hub | `Bool` | none | R | last runtime arm command state | No | Runtime state after `ARM` command |
| `last_transaction` | Hub | `Map` | none | R | action, encoded length, completion basis; connected writes also record live serial and reply text | No | Runtime transaction cache |
| `z` | Focus | `Position` | um | R/W | `z_lower..z_upper` | Yes | Focus DAC count |
| `z_lower`, `z_upper` | Focus | `Position` | um | R | configured travel range | No | Config/probe metadata |
| `high` | TTL | `Bool` | none | R/W | high/low | Yes | TTL state command and `PROG_TTL` sequence command |
| `high` | Camera trigger | `Bool` | none | R/W | high/low | No | Camera trigger state command |
| `channel` | TTL/camera trigger | `I64` | none | R | one-based channel | No | Channel metadata |
| `voltage` | DAC | `Voltage` | V | R/W | `0..10 V` | Yes | DAC count scaled by `dac_bits` |
| `enabled` | DAC | `Bool` | none | R/W | disable writes 0 V; enable restores configured voltage | Yes | DAC output command |
| `channel` | DAC | `I64` | none | R | one-based channel | No | Channel metadata |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "triggerscope"` or `"trigger_scope"` | Yes | string | Selects the TriggerScope provider |
| `property.serial_port` | Required when `property.connect = true` | string | Serial port path/name for active transport |
| `property.connect` | No | `Bool` | Open real serial transport when true; requires `os-serial` |
| `property.serial_timeout_ms` | No | `I64` or `TimeInterval` | Line-read window after each direct-control write |
| `property.product` | No | string | Persistent product/model label |
| `property.serial_number` | No | string | Persistent serial label |
| `property.firmware_version` | No | string | Configured identification banner |
| `property.dac_bits` | No | `I64` | DAC resolution, `12` or `16` |
| `property.ttl_count` | No | `I64` | Number of TTL devices to expose, `1..16` |
| `property.dac_count` | No | `I64` | Number of DAC devices to expose, `1..16` |
| `property.cam_count` | No | `I64` | Number of camera-trigger devices to expose, `0..2` |
| `property.focus`, `property.focus_lower`, `property.focus_upper` | No | `Position` | Focus position and range |
| `property.ttl_1_high..ttl_16_high` | No | `Bool` | Initial TTL states |
| `property.cam_1_high..cam_2_high` | No | `Bool` | Initial camera trigger states |
| `property.dac_1_voltage..dac_16_voltage` | No | `Voltage` | Initial DAC voltages |
| `property.dac_1_enabled..dac_16_enabled` | No | `Bool` | Initial DAC output-gate states |

Present TriggerScope config keys with the wrong type are rejected instead of
silently falling back to configured defaults.

## Examples

| Example | Demonstrates |
| --- | --- |
| `discover_devices` | Shows a configured TriggerScope controller in the two-stage discovery flow |
| `motion_stage` | Generic stage selection can use the focus Z device |
| `digital_io triggerscope` | Generic digital IO can use TTL, camera-trigger, DAC devices, and advertised hub `last_transaction` completion-basis readback |

## Remaining Work

| Area | Gap |
| --- | --- |
| Configured serial | Current live path requires configured `property.serial_port`; startup identification is cached when the controller returns a non-empty line |
| Timing programs | Array clearing, TTL/DAC/focus sequence programming, and `ARM` are available through constrained hub commands and public timing-plan APIs for TTL `high`, DAC `voltage`, and evenly stepped focus `z` sequences; route objects are rejected because no route opcode is evidenced, and camera-trigger timing remains direct-state only without a sequence opcode |
| Hardware validation | Record command stdout/stderr, line responses, TTL output state, DAC voltage/readback, focus movement/readback, camera-trigger behavior, and safe disable output |
| Protocol expansion | Direct-control writes and constrained sequence/arm commands have an opt-in serial path; public timing plans map only to the evidenced TTL/DAC/focus sequence commands. Camera-trigger sequence mapping, response/error vocabulary, and exact timing semantics are not exposed without protocol documentation or hardware traces |
| Safety | Validate DAC scaling, output disable behavior, focus limits, TTL idle state, and response/error vocabulary on real hardware |
