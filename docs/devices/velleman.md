# Velleman K8055/K8061

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::velleman` |
| Families | Velleman K8055/VM110 and K8061/VM140 USB experiment interface boards |
| Support level | Descriptor-discovered or explicit-config USB packet backend with analog/digital/PWM/counter IO |
| Protocol evidence | Velleman product pages document channel counts, analog resolutions, address counts, PWM frequency, and response/execution timing; Linux `vmk80xx` COMEDI driver and module aliases document USB VID/PID tables, packet lengths, endpoint style, K8055 packet registers, K8055 counter/debounce command bytes, K8061 command bytes, K8061 64-byte packet registers, and digital/analog/PWM/counter readback commands |
| Transport | Configured packet-USB abstraction, descriptor-only `os-usb` candidate discovery, explicit `os-usb` endpoint binding, or `connect=true` endpoint autodiscovery for a single known Velleman board |
| Discovery | Config-backed two-stage discovery with optional live USB connection plus non-invasive `os-usb` descriptor scanning for `10cf:5500`..`10cf:5503` and `10cf:8061`..`10cf:8068` |
| Validation | No hardware validation |
| Runtime/evidence notes | K8061 counter debounce, counter-reset safety, and bench output/readback notes need hardware traces or documentation |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `velleman-k8055-hub` / `velleman-k8061-hub` | `hub`, `usb.io`, `velleman.k8055` or `velleman.k8061` | Owns one configured board address |
| `velleman-*-digital-input` | `digital.input`, `state.device` | Digital input mask readback from the shared USB packet |
| `velleman-*-digital-output` | `digital.output`, `state.device` | Digital output mask on the shared packet endpoint |
| `velleman-k8055-analog-input-1..2` | `analog.input`, `adc` | 8-bit analog readback exposed as `Ratio` |
| `velleman-k8055-analog-output-1..2` | `analog.output`, `dac` | 8-bit analog output exposed as `Ratio`; shares the K8055 analog/digital output packet |
| `velleman-k8055-counter-1..2` | `counter`, `digital.input.counter` | 16-bit counter readback from the K8055 input packet; K8055 debounce writes are exposed, while reset commands remain hidden from regular and advanced command surfaces |
| `velleman-k8061-analog-input-1..8` | `analog.input`, `adc` | 10-bit analog readback exposed as `Ratio` |
| `velleman-k8061-analog-output-1..8` | `analog.output`, `dac` | 8-bit analog output exposed as `Ratio`; K8061 readback command is used after writes |
| `velleman-k8061-counter-1..2` | `counter`, `digital.input.counter` | 16-bit counter readback through `RD_CNT`; all-counter reset remains hidden from regular and advanced command surfaces; debounce is not exposed by the audited open-driver path |
| `velleman-k8061-pwm-output` | `pwm.output`, `dac` | 10-bit PWM duty exposed as `Ratio`; fixed 15.6 kHz frequency property |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| Velleman USB packet endpoint | `usb.packet` | Sends 8-byte K8055 or 64-byte K8061 packets through the shared packet abstraction; resource metadata records packet style, backend, configured or descriptor-discovered USB VID/PID, optional USB identity, interface, IN/OUT endpoints, transfer kind, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `DigitalIo` | Digital output | `CapabilityRequest::DigitalIo` | Map with output mask and completion basis | K8055 completes on packet write; K8061 emits active per-bit `SET_DO`/`CLR_DO` packets for changed bits, then follows with the documented digital-output readback command | State-set sequencing only |
| `Measure` | Digital input | `CapabilityRequest::Measure` | Map with input mask and input count | Reads a K8055 input packet or sends K8061 `RD_DI` and reads the reply packet | No |
| `Measure` | Counter channels | `CapabilityRequest::Measure` | Map with channel, count, max count, and K8055 debounce when available | K8055 reads the shared input packet; K8061 sends `RD_CNT` and reads the counter bytes | No |
| `Adc` | Analog input channels | `CapabilityRequest::Adc` | `Ratio` | K8055 reads input packet bytes 2/3; K8061 sends `RD_AI` with channel and reads two-byte 10-bit reply | No |
| `Dac` | Analog output channels | `CapabilityRequest::Dac` | Map with value and completion basis | K8055 writes the combined output packet; K8061 sends `SET_AO` then reads `RD_AO` for completion/readback | State-set sequencing plus runtime timing-plan first/last `value` endpoint application |
| `Dac` | K8061 PWM output | `CapabilityRequest::Dac` | Map with value and completion basis | Sends `OUT_PWM` then reads `RD_PWM` for completion/readback | State-set sequencing plus runtime timing-plan first/last `value` endpoint application |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `model` | Hub | `String` | none | R | `K8055/VM110` or `K8061/VM140` | No | Config/probe metadata |
| `serial_number` | Hub | `String` | none | R | configured serial label | No | Config/probe metadata |
| `board_address` | Hub | `I64` | address | R | K8055 `0..3`; K8061 `0..7` | No | Product address/USB ID family |
| `protocol` | Hub | `String` | none | R | model-specific Velleman packet protocol | No | Protocol metadata |
| `packet_len` | Hub | `I64` | bytes | R | K8055 `8`; K8061 `64` | No | Linux `vmk80xx` packet-length evidence |
| `usb_endpoint_style` | Hub | `String` | none | R | `interrupt` for K8055, `bulk` for K8061 | No | Linux `vmk80xx` endpoint-style evidence |
| `packet_backend` | Hub | `String` | none | R | configured `ScriptedUsbPacket` or `nusb` live packet backend | No | Runtime support metadata |
| `connected` | Hub | `Bool` | none | R | active USB transport state | No | Runtime transport state |
| `usb_endpoint` | Hub | `String` | none | R | configured VID/PID/interface/endpoints when live USB is connected | No | Runtime endpoint metadata |
| `usb_identity` | Hub | `Map` | none | R | descriptor-discovered or configured VID/PID plus product, serial, bus, and address when available | No | Linux `vmk80xx` module alias VID/PID evidence plus runtime descriptors |
| `command_summary` | Hub | `String` | none | R | model-specific public command-byte summary with reset operations omitted | No | Linux `vmk80xx` command evidence |
| `last_transaction` | Hub | `Map` | none | R | command, output/readback values, completion basis | No | Runtime transaction cache |
| `mask` | Digital input | `I64` | bit mask | R | K8055 `0..31`; K8061 `0..255` | No | K8055 input byte with Linux bit permutation; K8061 byte 1 after `RD_DI` |
| `input_count` | Digital input | `I64` | count | R | K8055 `5`; K8061 `8` | No | Product specification |
| `mask` | Digital output | `I64` | bit mask | R/W | `0..255` | No | K8055 output byte 1; K8061 changed bits through `SET_DO`/`CLR_DO` byte 1 with `RD_DO` readback |
| `output_count` | Digital output | `I64` | count | R | `8` | No | Product specification |
| `count` | Counter | `I64` | count | R | `0..65535` | No | K8055 packet bytes 4/5 or 6/7; K8061 `RD_CNT` reply bytes |
| `max_count` | Counter | `I64` | count | R | `65535` | No | Linux `vmk80xx` counter maxdata / 16-bit packet readback |
| `debounce` | K8055 counter | `TimeInterval` | ms | R/W | `1..7450 ms` | No | K8055 debounce command value derived from the Linux `vmk80xx` formula |
| `value` | K8055 analog input 1/2 | `Ratio` | percent | R | `0..100 percent` | No | Input packet bytes 2/3, 8-bit ADC count |
| `value` | K8061 analog input 1..8 | `Ratio` | percent | R | `0..100 percent` | No | `RD_AI`, channel byte 1, reply bytes 2/3, 10-bit ADC count |
| `value` | Analog outputs | `Ratio` | percent | R/W | `0..100 percent` | Yes | K8055 output bytes 2/3; K8061 `SET_AO`, channel byte 1, value byte 2, `RD_AO` readback |
| `value` | K8061 PWM output | `Ratio` | percent | R/W | `0..100 percent` | Yes | `OUT_PWM` low two bits in byte 1, upper bits in byte 2; `RD_PWM` readback |
| `resolution` | Analog/PWM devices | `I64` | bits | R | K8055 AI/AO `8`; K8061 AI/PWM `10`; K8061 AO `8` | No | Product specification and Linux maxdata |
| `frequency` | K8061 PWM output | `Frequency` | Hz | R | 15.6 kHz | No | Product specification |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "velleman"`, `"k8055"`, `"vm110"`, `"k8061"`, or `"vm140"` | Yes | string | Selects the Velleman discovery provider and model; `driver = "velleman"` defaults to K8055 unless `property.model` selects K8061 |
| `property.model` | No | string | Optional explicit model: `k8055`, `vm110`, `k8061`, or `vm140` |
| `property.serial_number` | No | string | Persistent label for the configured board |
| `property.board_address` | No | `I64` | K8055 card address `0..3`; K8061 card address `0..7`; invalid byte-sized values are rejected instead of ignored |
| `property.digital_output_mask` | No | `I64` | Initial digital output mask, `0..255`; invalid byte-sized values are rejected instead of ignored |
| `property.digital_input_mask` | No | `I64` | Configured packet-model digital input mask; invalid byte-sized values are rejected instead of ignored |
| `property.analog_output_1` ... | No | `Ratio` | Initial analog output values; 2 K8055 channels or 8 K8061 channels |
| `property.analog_input_1` ... | No | `Ratio` | Configured packet-model analog input values; 2 K8055 channels or 8 K8061 channels |
| `property.counter_1`, `property.counter_2` | No | `I64` or u16 string | Configured packet-model counter values |
| `property.counter_1_debounce`, `property.counter_2_debounce` | No | `TimeInterval` or `I64` milliseconds | Initial K8055 debounce intervals, clamped by config validation to `1..7450 ms` |
| `property.pwm_output` | No | `Ratio` | Initial K8061 PWM duty |
| `property.connect` | No | `Bool` | Opens live USB when true; requires `os-usb` plus explicit endpoint metadata or auto-discovery from a single known Velleman USB device |
| `property.vendor_id`, `property.product_id` | Optional with endpoint autodiscovery; required with explicit endpoints | `I64` or decimal/`0x` string | USB VID/PID to open; absent values default to Velleman VID plus the product ID implied by model and board address |
| `property.interface` | No | `I64` | USB interface to claim; defaults to `0` |
| `property.out_endpoint`, `property.in_endpoint` | Optional with endpoint autodiscovery; required with explicit endpoints | `I64` | USB OUT and IN endpoint addresses; `in_endpoint` must include the IN direction bit |
| `property.transfer_kind` | No | string | `interrupt` for K8055 or `bulk` for K8061 by default; can be set explicitly |

## Examples

| Example | Demonstrates |
| --- | --- |
| `discover_devices` | Shows configured Velleman K8055 and K8061 hubs, digital IO devices, counters, analog channels, and K8061 PWM in the two-stage discovery flow |
| `digital_io velleman` | Generic digital/analog IO workflow with output completion, input readback, and advertised hub `last_transaction` completion-basis readback; not Velleman-specific |

## Remaining Work

| Area | Gap |
| --- | --- |
| Endpoint discovery | `os-usb` descriptor scanning identifies known VID/PID candidates and board addresses without opening them; `connect=true` can open one matching configured board and select the single active-interface IN/OUT endpoint pair for the expected interrupt or bulk transfer style |
| Hardware validation | Record command stdout/stderr, requested digital/analog/PWM outputs, physical LED/output/readback behavior, input readback, and disable/reset behavior |
| Counters | K8055/K8061 counter readback and K8055 debounce are implemented from open driver evidence; K8055/K8061 counter-reset operations remain hidden from regular and advanced command surfaces; K8061 debounce is not exposed by the audited open-driver path; hardware validation note pending |
| Reset/safe state | K8055 reset and K8061 final disable/reset behavior need bench validation before automatic safety transitions are added |
| Timing | Runtime timing plans apply analog/PWM output `value` endpoints through existing write/readback paths; K8055 20 ms conversion time and K8061 4 ms/21 ms/48 ms hardware timing figures need validation before hardware-accurate timing promises are added |
