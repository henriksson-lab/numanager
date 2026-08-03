# Lumencor Spectra / SpectraX / CIA

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::lumencor` |
| Families | Lumencor Spectra/SpectraX-style light engines and CIA trigger controller; CIA engine selection also names Aura and Sola |
| Support level | Configured opt-in serial startup/setup readback plus CIA info readback and CIA command helpers |
| Protocol evidence | Public Lumencor serial command behavior |
| Transport | Serial ASCII over `SerialIo` |
| Discovery | Config-backed discovery plus opt-in Spectra startup and CIA setup readbacks |
| Validation | Construction-time serial startup/setup probe paths and CIA info refresh are implemented; real hardware validation pending |
| Runtime/evidence notes | `numanager-drivers/os-serial` for configured real serial ports |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `lumencor-spectra-hub` | `hub`, `light.engine`, `shutter` | Owns one Spectra/SpectraX-style serial resource and global shutter |
| `lumencor-*` channels | `light.source`, `led.channel`, `trigger.sink` | Per-color logical channels remultiplexed into channel masks/intensity commands |
| `lumencor-cia` | `trigger.controller`, `pulse.program`, `trigger.sink` | Trigger-event program controller |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `lumencor-spectra-serial` | `serial.binary` | Legacy Spectra binary serial command path for startup GPIO, channel masks, intensity, shutter, and trigger-profile state; resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state |
| `lumencor-cia-serial` | `serial.ascii` | CIA newline command path for info, engine/polarity setup, typed event-program loading, run, stop, and timing control; resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state; CIA `GenericCommand` accepts `run`, `stop`, `step`, `rewind`, and `info` with no params |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `TriggerSink` | Spectra hub/channels | `None` or `CapabilityRequest::Trigger` | Open/channel map | Runtime token after serial command | Hub shutter/channel enable endpoint sequences |
| `Dac` | Spectra channels | `CapabilityRequest::Dac` | Intensity map | Runtime token after serial command | Channel intensity endpoint sequences |
| `PulseProgram` | CIA | `CapabilityRequest::PulseProgram` with all fields `None`, or `None`; loads configured `levels`/`events` properties through the typed program path | Program status map | Runtime token after serial command | Runtime arm/start/stop binding |
| `TriggerSink` | CIA | `None` or `CapabilityRequest::Trigger` | Run-state map | Runtime token after serial command | Run/stop/pulse control for prepared CIA program |
| `GenericCommand` | CIA | `run`, `stop`, `step`, `rewind`, or `info` with no params | Status map containing command, info, and run state | Runtime token after the documented CIA command sequence | CIA bring-up and info refresh only; program loading is exposed through `PulseProgram`, not a generic command |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `model` | Spectra hub | `String` | none | R | model reply | No | Active probe/readback |
| `open` | Spectra hub | `Bool` | none | R/W | none | Yes | Global shutter command |
| `enable_mask` | Spectra hub | `I64` | bitmask | R/W | channel mask | No | Enable-mask command |
| `trigger_profile` | Spectra hub | `String` | none | R | composed channel trigger profile | No | Local/channel state |
| `state_summary` | Spectra hub | `Map` | none | R | model, masks, shutter/YG state, trigger profile, startup commands, all channels | No | Composite local/readback state |
| `yg_filter` | Spectra hub | `Bool` | none | R/W | none | No | YG filter command |
| `enabled` | Spectra channel | `Bool` | none | R/W | none | Yes | Channel enable command |
| `intensity` | Spectra channel | `Ratio` | percent | R/W | 0..100 | Yes | Channel intensity command |
| `trigger_mode` | Spectra channel | `String` | none | R/W | channel trigger modes | No | Trigger-mode command |
| `color` | Spectra channel | `String` | none | R | color enum | No | Descriptor metadata |
| `wavelength` | Spectra channel | `Wavelength` | named wavelength value | R | nominal wavelength | No | Descriptor metadata |
| `engine` | CIA | `String` | none | R/W | configured engines | No | CIA engine command |
| `input1_polarity` / `input2_polarity` | CIA | `String` | none | R/W | `Low`, `High` | No | Input polarity commands |
| `info` | CIA | `String` | none | R | `#I` reply | No | CIA info query |
| `levels` | CIA | `Bytes` | none | R/W | raw color levels | No | CIA event program |
| `events` | CIA | `Bytes` | none | R/W | raw event masks | No | CIA event program |
| `event_count` | CIA | `I64` | count | R | programmed events | No | CIA readback/local state |
| `run_state` | CIA | `String` | none | R | run-state labels | No | Run/stop state |

When `connect = true`, Spectra/SpectraX discovery opens the configured serial
endpoint, runs the startup probe script, and seeds initialized startup state,
channel metadata, enable mask, and shuttered state before registering the
driver. CIA discovery runs `#I` plus engine/polarity setup and seeds engine,
input polarity, raw info, and ready run state. Runtime reads of `info` issue
`#I`, engine/polarity writes refresh the raw info reply when available, and CIA
`GenericCommand` accepts only the named `run`, `stop`, `step`, `rewind`, and
`info` command names with no params. Program loading uses `PulseProgram` and
runtime arm/start paths so it is not exposed as a generic advanced command.
Spectra/SpectraX commands remain send-only under the current evidence.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- light_source` | Generic light-source `Dac` and `TriggerSink`, typed intensity/enable properties, Lumencor CIA `PulseProgram`/`TriggerSink` invocation, remultiplexed state set, `Runtime::wait_completed`, timing plan, and readback |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate serial command coverage, analog behavior, channel masks, CIA timing, and readbacks on real hardware |
| Timing | Hardware-accurate trigger event scheduling and latency characterization |
| Safety | Shutter polarity, source fault/interlock, and warmup states |
