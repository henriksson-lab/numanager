# WOSM

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::wosm` |
| Families | Warwick Open-Source Microscope controller |
| Support level | Project-command-page-backed TCP output for `dig_out`, `dig_in`, `dac_dest`, and `stg_out_*`; legacy source-backed switch-sequence, blanking, pull-up, and raw analog-input commands behind opt-in `connect` |
| Protocol evidence | WOSM MCU command page for firmware code base `v0.900`; legacy reverse evidence for TCP login, prompt completion, CRLF framing, `P`/`N`/`R`/`E` sequences, `B`/`F` blanking, `A,<channel>` raw analog input, and `D,<pin>,<enabled>` pull-up commands |
| Transport | TCP text session, default host shown by controller LCD, driver/controller-PC Telnet port `1023` by default, configurable user Telnet port `23`, CRLF commands, `W>` prompt completion |
| Discovery | Config-backed two-stage discovery; optional TCP connection from config |
| Validation | No hardware validation |
| Runtime/evidence notes | [`../reverse/wosm-protocol.md`](../reverse/wosm-protocol.md) records the command-page audit. Analog raw-count scaling, route mapping, legacy sequence commands, blanking timing, current scaling, and safety behavior need hardware traces or firmware documentation |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `wosm-hub` | `hub`, `microscope.controller`, `tcp.text` | Owns one TCP text controller resource |
| `wosm-switch` | `digital.output`, `state.device`, `trigger.source` | Eight-bit public switch state maps to the WOSM digital line mask used for lines `s..z` |
| `wosm-shutter` | `shutter`, `light.gate`, `trigger.sink` | Shutter open/close remultiplexes through the same digital output mask as the switch |
| `wosm-xy-stage` | `axis.xy`, `stage.xy`, `motion.stage` | X/Y logical stage writes map to the WOSM `px` and `py` DAC-backed stage channels |
| `wosm-z-stage` | `axis.z`, `stage.z`, `motion.stage` | Z logical stage writes map to the WOSM `pz` DAC-backed focus channel |
| `wosm-input` | `digital.input`, `analog.input`, `state.device` | Connected read-on-demand for aggregate digital input and raw analog counts, plus input-pull-up writes; percent analog properties remain configured when ADC scaling is not evidenced |
| `wosm-light-1..4` | `light.source`, `dac.output`, `trigger.sink` | High-current light outputs map to WOSM `ps..pv` DAC lines and switch bits |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `wosm-tcp` | `tcp.text` | Shared command session for microscope controller, stage control, digital switch/shutter lines, high-current DAC destinations, connected input readback, and input-pull-up writes; resource metadata records configured `host`, `port`, `prompt_timeout`, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `StageMove` | `wosm-xy-stage` | `CapabilityRequest::StageMove` with X/Y targets | XYZ position map | `stg_out_x` / `stg_out_y` plus TCP `W>` prompt when `connect = true`; configured completion otherwise | Not sequenceable by current WOSM sequence protocol |
| `StageMove` | `wosm-z-stage` | `CapabilityRequest::StageMove` with Z target | XYZ position map | `stg_out_z` plus TCP `W>` prompt when `connect = true`; configured completion otherwise | Not sequenceable by current WOSM sequence protocol |
| `DigitalIo` | `wosm-switch` | `CapabilityRequest::DigitalIo` with whole output mask | Switch-state integer | TCP `W>` prompt when `connect = true`; configured completion otherwise | `state` is sequenceable |
| `TriggerSource` | `wosm-switch` | `CapabilityRequest::Trigger` enable/disable/pulse | Sequence-enabled bool | TCP `W>` prompt when `connect = true`; configured completion otherwise | Starts/stops the switch-state sequence path loaded by timing `Arm` |
| `TriggerSink` | `wosm-shutter` | `CapabilityRequest::Trigger` enable/disable/pulse | Open bool | TCP `W>` prompt when `connect = true`; configured completion otherwise | Not sequenceable directly; use switch `state` sequences for digital masks |
| `Dac` | `wosm-light-1..4` | `CapabilityRequest::Dac` with `Ratio` | Output ratio | `dac_dest p<s|t|u|v> <0..65535>` plus TCP `W>` prompt when `connect = true`; configured completion otherwise | Not sequenceable by current WOSM sequence protocol |
| `TriggerSink` | `wosm-light-1..4` | `CapabilityRequest::Trigger` enable/disable/pulse | Enabled bool | TCP `W>` prompt when `connect = true`; configured completion otherwise | Not sequenceable directly; use switch `state` sequences for digital masks |
| `Adc` | `wosm-input` | `CapabilityRequest::Adc` with optional public channel `1..6` | Raw analog count | TCP `W>` prompt for legacy live `A,<channel-1>` read when `connect = true`; cached raw count otherwise | No |
| `Measure` | `wosm-input` | `CapabilityRequest::Measure` | Map with digital aggregate, raw analog channel 1 count, and configured percent channel 1 | TCP `W>` prompt for live `dig_in` and legacy `A,0` reads when `connect = true`; configured completion otherwise | No |
| Timing plan | Switch/resource | `Command::Arm` / `Start` / `Stop` with switch `state` byte sequences | `Map` | `Arm` loads `P,index,value` bytes and `N,count`; `Start` sends `R`; `Stop` sends `E` | No route/timebase claims |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `product` | Hub | `String` | none | R | configured label | No | Config/probe metadata |
| `serial_number` | Hub | `String` | none | R | configured label | No | Config/probe metadata |
| `firmware_version` | Hub | `I64` | none | R | configured firmware version | No | Login/probe metadata |
| `host` | Hub | `String` | none | R | configured host | No | TCP endpoint metadata |
| `port` | Hub | `I64` | none | R | configured port | No | TCP endpoint metadata |
| `connected` | Hub | `Bool` | none | R | true when opt-in TCP session opened | No | Runtime transport state |
| `prompt_timeout` | Hub | `TimeInterval` | s | R | configured prompt timeout | No | TCP prompt wait timeout |
| `inverted_logic` | Hub | `Bool` | none | R/W | normal/inverted output mask logic | No | Digital mask inversion before write |
| `last_transaction` | Hub | `Map` | none | R | action, endpoint, encoded length, completion basis, live TCP flag, reply text when connected | No | Runtime transaction cache |
| `state` | Switch | `I64` | none | R/W | `0..255` | Yes | Digital output mask for lines `s..z` |
| `sequence_enabled` | Switch | `Bool` | none | R/W | enabled/disabled | No | Sequence run/end commands |
| `blanking_enabled` | Switch | `Bool` | none | R/W | enabled/disabled | No | Blanking enable command |
| `blank_on` | Switch | `String` | none | R/W | `Low`, `High` | No | Blanking polarity command |
| `open` | Shutter | `Bool` | none | R/W | open/closed | No | Digital output mask |
| `x`, `y` | XY stage | `Position` | um | R/W | `0..x_travel`, `0..y_travel` | No | `stg_out_x` / `stg_out_y` |
| `x_travel`, `y_travel` | XY stage | `Position` | um | R | configured travel | No | Config/probe metadata |
| `z` | Z stage | `Position` | um | R/W | `0..z_travel` | No | `stg_out_z` |
| `z_travel` | Z stage | `Position` | um | R | configured travel | No | Config/probe metadata |
| `output` | Light | `Ratio` | percent | R/W | `0..100` | No | `dac_dest p<s|t|u|v>` mapped to 16-bit destination counts |
| `enabled` | Light | `Bool` | none | R/W | enabled/disabled | No | Switch bit state |
| `line` | Light | `String` | none | R | `s`, `t`, `u`, or `v` | No | WOSM high-current line label |
| `digital_input` | Input | `I64` | none | R | `0..63` cached/configured value | No | Connected read sends `dig_in` and parses decimal or hex aggregate input |
| `input_pullups` | Input | `I64` | none | R/W | `0..63` bitmask for input pins 0..5 | No | Legacy source-backed `D,<pin>,<enabled>` for each input pin and caches the requested mask |
| `analog_input_1..6` | Input | `Ratio` | percent | R | configured values | No | Configured percent values when ADC raw-count scaling is not evidenced |
| `analog_input_1_raw..analog_input_6_raw` | Input | `I64` | none | R | cached/configured raw count | No | Connected read sends `A,<channel>` with zero-based channels and parses `A,<value>` |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "wosm"` or `"warwick_wosm"` | Yes | string | Selects the WOSM provider |
| `property.host` | No | string | Controller TCP host |
| `property.port` | No | `I64` | Controller TCP port; default `1023` for driver/controller-PC Telnet, use `23` for user Telnet sessions |
| `property.connect` | No | `Bool` | If true, opens the configured TCP endpoint during discovery and completes output commands from the `W>` prompt |
| `property.prompt_timeout_ms` | No | `I64` or `TimeInterval` | TCP prompt wait timeout; default 2000 ms |
| `property.product` | No | string | Persistent product/model label |
| `property.serial_number` | No | string | Persistent serial label |
| `property.firmware_version` | No | `I64` | Configured firmware version |
| `property.inverted_logic` | No | `Bool` | Invert digital output mask before writing |
| `property.switch_state` | No | `I64` | Initial switch state, `0..255` |
| `property.sequence_enabled` | No | `Bool` | Initial cached sequence run/end state |
| `property.blanking_enabled` | No | `Bool` | Initial cached blanking-enable state |
| `property.blank_on` | No | string | Initial cached blanking polarity, `Low` or `High` |
| `property.shutter_open` | No | `Bool` | Initial shutter state |
| `property.x`, `property.y`, `property.z` | No | `Position` | Initial configured positions |
| `property.x_travel`, `property.y_travel`, `property.z_travel` | No | `Position` | Configured travel ranges |
| `property.light_1_output..light_4_output` | No | `Ratio` | Initial light output percentages |
| `property.light_1_enabled..light_4_enabled` | No | `Bool` | Initial light enable states |
| `property.digital_input` | No | `I64` | Configured digital input value |
| `property.input_pullups` | No | `I64` | Initial input pull-up bitmask, `0..63` |
| `property.analog_input_1..analog_input_6` | No | `Ratio` | Configured analog input values |
| `property.analog_input_1_raw..analog_input_6_raw` | No | `I64` | Initial cached raw analog input counts |

## Examples

| Example | Demonstrates |
| --- | --- |
| `discover_devices` | Shows a configured WOSM controller in the two-stage discovery flow |
| `motion_stage` | Generic stage selection and typed `StageMove` works for WOSM stage devices |
| `light_source` | Generic light selection and typed `Dac`/`TriggerSink` works for WOSM light devices |
| `digital_io wosm` | Generic digital IO selection works for the WOSM switch/input devices |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Record login, prompt completion, `stg_out_*` stage writes, `dig_out` switch/shutter writes, `dac_dest` light output/readback, `dig_in` input reads, legacy `A,<channel>` input reads, `D,<pin>,<enabled>` pull-up writes, and matching public command output on a real controller |
| Input scaling | Aggregate digital and raw analog reply parsing is implemented, but analog conversion to physical units or percent is not recorded |
| Sequences/blanking | Sequence run/end, switch-state sequence loading, and blanking controls are command-backed by legacy source evidence; route mapping, non-switch sequence loading, current v0.900 command equivalence, and exact timing behavior need hardware traces or firmware documentation |
| Safety | Validate digital-line masks, light-current scaling, stage travel calibration, inversion behavior, and safe disable states |
