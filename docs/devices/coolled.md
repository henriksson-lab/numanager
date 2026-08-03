# CoolLED pE Series

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::coolled` |
| Families | CoolLED pE-300, pE-340, and pE-4000 light engines |
| Support level | pE-300/pE-4000/pE-340 configured opt-in serial control/readback and refresh helpers |
| Protocol evidence | Public CoolLED serial command behavior |
| Transport | Serial ASCII over `SerialIo` |
| Discovery | Config-backed pE-300, pE-340, and pE-4000 discovery; live serial requires configured endpoints and explicit connect |
| Validation | Configured serial startup-readback/control paths are implemented; real hardware validation pending |
| Runtime/evidence notes | `numanager-drivers/os-serial` enables configured real serial ports |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `coolled-pe300-hub` / `<prefix>-hub` | `hub`, `light.engine`, `shutter` | Owns one serial resource and global output state |
| `coolled-pe300-channel-*` / `<prefix>-channel-*` | `light.source`, `led.channel`, `trigger.sink` | Per-channel logical lights remultiplexed through hub |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `<prefix>-serial` | `serial` | Serial command path shared by hub output state, channel selection, intensity, wavelength, and status/readback queries; resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_identity`, `refresh_status`, or `refresh_channels` with no params | State summary map | Uses only mapped model/version/status/channel readback commands; pE-4000-family identity refresh also includes the lamp-summary wavelength inventory readback | Not sequenceable |
| `GenericCommand` | Channels | `refresh_readbacks` or `refresh_channel` with no params | Channel state map | Uses only the mapped channel query readback command | Not sequenceable |
| `TriggerSink` | Hub/channels | `None` or `CapabilityRequest::Trigger` | Output/selection map | Runtime token after serial command | Global/channel output gating and endpoint sequences |
| `Dac` | Channels | `CapabilityRequest::Dac` | Intensity map | Runtime token after serial command | Channel intensity endpoint sequences |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `enabled` | Hub | `Bool` | none | R/W | none | Yes | Global output command |
| `timing_state` | Hub | `Map` | none | R | arm/start/stop summary | No | Runtime timing hook state |
| `state_summary` | Hub | `Map` | none | R | model/version, pod lock, global output, timing state, all channels | No | Composite status/readback |
| `model` | Hub | `String` | none | R | model reply | No | Active probe/readback |
| `version` | Hub | `String` | none | R | version reply where supported | No | Active probe/readback |
| `channel_count` | Hub | `I64` | count | R | configured/probed count | No | Descriptor metadata |
| `enabled` | Channel | `Bool` | none | R/W | none | Yes | Channel selected/output command |
| `selected` | Channel | `Bool` | none | R/W | none | Yes | Channel selection command |
| `intensity` | Channel | `Ratio` | percent | R/W | 0..100 | Yes | Channel intensity command |
| `wavelength` | Channel | `Wavelength` | named wavelength value | R/W on pE-4000, R on fixed channels | available channel wavelengths | No | `LOAD:<nm>` or descriptor metadata |

When `connect = true`, discovery opens the configured serial endpoint and runs
the model-specific configured startup-readback script before registering the driver. pE-300
seeds model, version, global output, channel selection, and intensity state.
pE-4000-family devices also seed channel wavelength state from controller
replies.

Property reads request the model-specific status or channel query before
returning cached state. Writable global, channel selection, intensity, and
pE-4000 wavelength paths issue the command and then ingest status/channel
readbacks when the controller returns them.

The hub and channel `GenericCommand` capabilities expose named read-only
refresh helpers over the same mapped readback commands used by property reads.
They do not expose raw serial commands or setter commands.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- light_source` | Generic pE-300 light-source `Dac` and `TriggerSink`, typed intensity/enable properties, remultiplexed state set, `Runtime::wait_completed`, timing plan, and readback |
| `cargo run -p numanager-examples -- light_source pe4000` | Generic pE-4000 selector with configurable LED wavelengths, typed intensity, `TriggerSink`, timing-plan transitions, and readback |
| `cargo run -p numanager-examples -- light_source pe340` | Generic pE-340 selector using the pE-4000-family channel/wavelength surface with typed intensity, `TriggerSink`, timing-plan transitions, and readback |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate serial replies, channel inventories, output gating, and timing behavior against real engines |
| Safety | Interlock, pod lock, shutter polarity, and fault state coverage |
| Timing | Hardware trigger profiles beyond current global/channel gating and intensity endpoint sequencing |
