# Arduino Counter

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::arduino_counter` |
| Families | Arduino Counter firmware-compatible pulse/counter devices |
| Support level | Config-backed counter protocol plus opt-in configured real serial snapshot/count readback and snapshot refresh helper |
| Protocol evidence | Firmware-style CR text command surface |
| Transport | CR-terminated serial text over `SerialIo`; configured real serial uses an explicit OS serial port |
| Discovery | Simulated two-stage discovery plus config-backed discovery; configured real serial reads a `p?` snapshot before registration |
| Validation | Configured/simulated validation and real serial backend compile; real firmware validation pending |
| Runtime/evidence notes | Real serial requires `numanager-drivers/os-serial` and explicit `connect = true` |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `arduino-counter-hub` | `hub`, `microcontroller` | Owns one serial resource |
| `arduino-counter` | `counter`, `timing.source` | Counter and pulse-program setup path |
| `arduino-counter-pulse` | `trigger.source`, `pulse.generator` | Pulse-level output path |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `arduino-counter-serial` | `serial.text` | CR-terminated firmware serial link for counter snapshots and pulse commands; configured real serial records port and connection metadata |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `GenericCommand` | Hub/counter | `refresh_snapshot` with no params | Snapshot map with count and pulse level | Uses only the mapped `p?` snapshot readback path | Not sequenceable |
| `Measure` | Counter | `CapabilityRequest::Measure` | Count/snapshot map | Runtime token after snapshot parse | Gate time can be applied as timing endpoint |
| `PulseProgram` | Counter | `CapabilityRequest::PulseProgram` | Program map | Runtime token after firmware command | Pulse interval can be applied as timing endpoint |
| `TriggerSource` | Pulse output | `None` or `CapabilityRequest::Trigger` | Level/status map | Runtime token after firmware command | `level` endpoint or default pulse high/low |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `gate` | Counter | `TimeInterval` | ms | R/W | fixture range | Yes | `gNNN`; legacy alias `gate_ms` |
| `count` | Counter | `I64` | count | R | non-negative | No | Count snapshot reply |
| `interval` | Counter | `TimeInterval` | us | R/W | fixture range | Yes | `pi` interval setup; legacy alias `interval_us` |
| `counter_summary` | Counter | `Map` | none | R | snapshot fields | No | `count=<n>;level=<0|1>` parser |
| `level` | Pulse output | `Bool` | none | R/W | none | Yes | `pi`/`pd` pulse level commands |

## Config Keys

| Key | Type | Required | Meaning |
| --- | --- | --- | --- |
| `driver = "arduino_counter"`, `"arduino-counter"`, or `"mm-arduino-counter"` | string | Yes | Selects Arduino Counter configured discovery |
| `gate` or `gate_ms` | `TimeInterval` or integer | No | Configured/default gate time |
| `interval` or `interval_us` | `TimeInterval` or integer | No | Configured/default pulse interval |
| `count` | integer | No | Configured cached count before live readback |
| `pulse_level` | bool | No | Configured cached pulse level before live readback |
| `serial_port` | string | Required for real serial | OS serial port name |
| `baud_rate` | integer | No | Defaults to `57600` |
| `serial_timeout_ms` | integer | No | Defaults to `1000` |
| `connect` | bool | No | When true, opens the configured serial port behind `numanager-drivers/os-serial`, reads a `p?` snapshot before registration, parses connected `gNNN` count replies, and uses `p?` snapshots for connected count/summary reads |

The hub/counter `GenericCommand` capability exposes the named
`refresh_snapshot` helper over the mapped `p?` readback. It does not expose raw
firmware commands.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- digital_io` | Generic `Measure`, `PulseProgram`, and `TriggerSource` setup, invocation, `Runtime::wait_completed`, timing plan, output/readback, and event subscription |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate CR command timing, count snapshots, and level semantics against real firmware |
| Timing | Define exact trigger edge/counting behavior in acquisition plans |
| Safety | Electrical limits and polarity/fault reporting |
