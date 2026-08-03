# Micro-Manager Arduino Controller

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::arduino` |
| Families | Micro-Manager Arduino firmware-compatible controllers |
| Support level | Config-backed firmware protocol plus opt-in configured real serial startup readback, control, readback, input pull-up path, and input refresh helper |
| Protocol evidence | Open firmware command opcodes and Micro-Manager behavior as secondary evidence |
| Transport | Serial binary/text command frames over `SerialIo`; configured real serial uses an explicit OS serial port |
| Discovery | Simulated two-stage firmware-identification discovery plus config-backed discovery; configured real serial reads controller ID, version, pattern count, DAC channel count, and digital pin count before registration |
| Validation | Configured/simulated validation and real serial backend compile; real firmware validation pending |
| Runtime/evidence notes | Real serial requires `numanager-drivers/os-serial` and explicit `connect = true` |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `arduino-hub` | `hub`, `microcontroller` | Owns one serial resource |
| `arduino-digital-out` | `digital.io`, `trigger.source` | Digital output and sequence state remultiplexed through hub |
| `arduino-shutter` | `shutter`, `trigger.sink` | Shutter open/close maps to digital output path |
| `arduino-adc` | `analog.input` | ADC readback through hub |
| `arduino-dac` | `analog.output` | DAC output through hub |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `arduino-serial` | `serial` | Firmware serial link for binary/text command frames and readback snapshots; configured real serial records port and connection metadata |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `DigitalIo` | Digital output | `CapabilityRequest::DigitalIo` | Map/status | Runtime token after firmware command | Digital mask and sequence start/stop endpoints |
| `TriggerSource` | Digital output | `None` or `CapabilityRequest::Trigger` | Map/status | Runtime token after firmware command | Sequence output source |
| `TriggerSink` | Shutter | `None` or `CapabilityRequest::Trigger` | Map/status | Runtime token after firmware command | Shutter open endpoint |
| `Adc` | ADC | `CapabilityRequest::Adc` | Snapshot/count map | Runtime token after read path | Not sequenceable |
| `GenericCommand` | ADC | `refresh_inputs`, `refresh_digital_inputs`, or `refresh_channel_0` with no params | Snapshot/count map | Runtime token after mapped readback | Not sequenceable |
| `Dac` | DAC | `CapabilityRequest::Dac` | Map/status | Runtime token after firmware command | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `logic` | Hub | `String` | none | R/W | firmware logic modes | No | Logic/inversion setup |
| `version` | Hub | `I64` | none | R | firmware version | No | Version opcode |
| `digital_inputs` | ADC | `I64` | none | R | digital input mask | No | Digital input read opcode |
| `input_pullups` | ADC | `I64` | none | R/W | digital input pull-up mask | No | Input pull-up opcode, one command per pin |
| `input_summary` | ADC | `Map` | none | R | ADC/digital snapshot | No | Input snapshot opcodes |
| `mask` | Digital output | `I64` | none | R/W | digital output bitmask | Yes | Digital write opcode |
| `sequence` | Digital output | `String` | none | R/W | `On`/`Off` style configured states | Yes | Digital sequence start/stop |
| `sequence_values` | Digital output | `List` | none | R/W | bitmask list | No | Sequence upload opcode |
| `timed_delays` | Digital output | `List<TimeInterval>` | ms wire conversion | R/W | fixture range | No | Timed-pattern delay opcode; legacy alias `timed_delays_ms` |
| `timed_repeat` | Digital output | `I64` | count | R/W | positive fixture count | No | Timed-pattern repeat opcode |
| `timed_output` | Digital output | `String` | none | R/W | `On`/`Off` | Yes | Timed-pattern start endpoint |
| `blanking` | Digital output | `String` | none | R/W | `On`/`Off` | No | Blanking start/stop opcode |
| `blank_on` | Digital output | `String` | none | R/W | fixture blanking modes | No | Blanking mode opcode |
| `open` | Shutter | `Bool` | none | R/W | none | Yes | Shutter/digital output opcode |
| `channel_N` | DAC | `I64` | count | R/W | 0-4095 fixture range | No | DAC output opcode |

## Config Keys

| Key | Type | Required | Meaning |
| --- | --- | --- | --- |
| `driver = "arduino"` or `"mm-arduino"` | string | Yes | Selects Arduino configured discovery |
| `controller_id` | string | No | Configured controller label used before live readback |
| `version`, `extended_version`, `pattern_count`, `dac_channels`, `digital_pins` | integer | No | Configured firmware identity and capacity metadata |
| `serial_port` | string | Required for real serial | OS serial port name |
| `baud_rate` | integer | No | Defaults to `57600` |
| `serial_timeout_ms` | integer | No | Defaults to `500` |
| `connect` | bool | No | When true, opens the configured serial port behind `numanager-drivers/os-serial`, reads startup firmware identity/capacity replies, and uses connected replies for ADC/digital-input reads |

The ADC `GenericCommand` surface exposes read-only helpers over mapped input
readbacks. It does not expose raw firmware commands, output/setup writes, or port
discovery.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- digital_io` | Generic `DigitalIo`, `Dac`, `Adc`, `TriggerSource`, and `TriggerSink` setup, invocation, `Runtime::wait_completed`, timing plan, output/readback, and event subscription |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate opcode coverage and input parsing against real firmware |
| Protocol | Current evidenced firmware-identification, digital output, DAC, sequence, timed output, blanking, digital-input, ADC, and pull-up opcodes are implemented; further opcodes are not exposed without firmware source, project documentation, captured traces, or bench logs |
| Safety | Document electrical limits, polarity, blanking defaults, and fault behavior per board |
