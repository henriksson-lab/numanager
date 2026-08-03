# Hamilton Serial MVP

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::hamilton_mvp` |
| Families | Hamilton Serial Modular Valve Positioner, Protocol 1/RNO+ mode |
| Support level | Spec-backed Protocol 1/RNO+ configured serial startup/readback with explicit position/type/status/done/error refresh commands, configured serial resource metadata, and configured daisy-chain aggregation |
| Protocol evidence | Hamilton MVP product page and Serial MVP manual identify RS-232 ASCII operation, 16-device daisy chaining, and Protocol 1/RNO+/DIN protocol options; Hamilton Protocol 1/RNO+ evidence documents address/CR framing, ACK/NAK, valve positioning, current-position query, valve-type query, status, and firmware requests |
| Transport | Configured serial, 9600 baud, 7 data bits, odd parity, 1 stop bit, CR termination |
| Discovery | Config-backed two-stage discovery; optional configured real serial construction behind `os-serial` reads firmware, valve type, current position, status, done, and valve-error state for every configured address before registration |
| Validation | No hardware validation |
| Runtime/evidence notes | `os-serial` for real configured serial ports |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `hamilton-mvp-hub` | `hub`, `fluidics.controller`, `hamilton.mvp` | Owns one configured Protocol 1/RNO+ serial resource and the configured address list |
| `hamilton-mvp-valve-<address>` | `fluidics.valve`, `state.device`, `hamilton.mvp.valve` | One logical valve per configured address, up to the documented 16-address daisy-chain limit |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| Controller serial port | `serial` | Shared Protocol 1/RNO+ command/reply channel for the configured address list; resource metadata records configured `serial_port`, fixed `baud_rate`, `serial_timeout`, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `ValveSelect` | Valve | `CapabilityRequest::ValveSelect` | Valve state map | Sends addressed `LPdppR`, requires ACK, polls addressed `E1` status until the valve-busy bit clears on real serial, then refreshes `LQP` current position; cached configured state records completion when no live reply is available | Position property can be used in state sets; hardware-timed valve sequencing is not exposed because timing evidence is absent |
| `GenericCommand` | Hub | `refresh_status`, `read_done`, `read_position`, `read_valve_type`, or `read_valve_error` with no params | Address-keyed map of state, busy, position, valve type, or error values | Repeats only the mapped readback command for every configured address; status, done, position, valve-type, and valve-error commands complete from hardware reply where real serial is configured | Diagnostic/readback only; firmware identity is exposed through properties, and reset-like initialization remains hidden from regular and advanced command surfaces |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `model` | Hub | `String` | none | R | configured model | No | Config/probe metadata |
| `serial_number` | Hub | `String` | none | R | configured serial | No | Config/probe metadata |
| `protocol` | Hub | `String` | none | R | `Hamilton Protocol 1/RNO+` | No | Protocol metadata |
| `address` | Hub | `String` | none | R | comma-separated `a..p` list | No | Configured Protocol 1/RNO+ addresses |
| `firmware` | Hub | `String` | none | R | first known product identifier/version string | No | `U` firmware request when read on real serial |
| `valve_count` | Hub | `I64` | count | R | `1..16` | No | Configured daisy-chain topology |
| `valve_addresses` | Hub | `List` | none | R | `a..p` entries | No | Configured daisy-chain topology |
| `last_transaction` | Hub | `Map` | none | R | command, reply length, response, completion basis | No | Runtime transaction cache for trace notes |
| `address` | Valve | `String` | none | R | `a..p` | No | Protocol address for this valve device |
| `position` | Valve | `I64` | ordinal | R/W | `1..port_count`, max 8 | No | `LPdppR` write; `LQP` one-based current-position query when read on real serial |
| `port_count` | Valve | `I64` | count | R | `1..8` | No | Configured valve topology or `LQT` valve-type query mapped to position count |
| `valve_type` | Valve | `I64` | native code | R | `2..7` | No | `LQT` valve-type query; codes 2/3/4/5/6/7 map to 8/6/3/2/2/4 positions |
| `initialized` | Valve | `Bool` | none | R | none | No | Hidden `LXR` initialization command state |
| `busy` | Valve | `Bool` | none | R | none | No | `F` done request or `E1` status byte; emits property change on refresh when value changes |
| `valve_error` | Valve | `Bool` | none | R | none | No | `G` valve-error request or `E1` error bits; emits property change on refresh when value changes |
| `status_raw` | Valve | `I64` | byte | R | `0..127` | No | `E1` status byte |
| `state_summary` | Valve | `Map` | none | R | position, port count, valve type, initialized, busy, error, raw status | No | Runtime cache plus mapped readbacks |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "hamilton_mvp"` or `"hamilton-mvp"` | Yes | string | Selects the Hamilton MVP discovery provider |
| `property.model` | No | string | Controller model label |
| `property.serial_number` | No | string | Persistent controller serial |
| `property.address` | No | string | Single Protocol 1/RNO+ address, `a..p`; default `a`; retained as the one-valve alias |
| `property.addresses` | No | comma-separated string, or `Value::List` in programmatic config | Protocol 1/RNO+ daisy-chain address list, unique `a..p` entries, maximum 16 |
| `property.port_count` | No | `I64` | Valve positions, `1..8`; invalid configured topologies are rejected instead of clamped; default `8` |
| `property.position` | No | `I64` | Initial configured position; default `1` |
| `property.firmware` | No | string | Configured firmware label before active readback |
| `property.serial_port` | No | string | Opt-in real serial port; without `connect=true`, the configured state model remains active |
| `property.connect` | No | `Bool` | If true with `serial_port`, open a real serial transport and read firmware/status/done/error state before registration |
| `property.serial_timeout_ms` | No | integer | Serial read timeout for configured real ports |
| `property.completion_poll_limit` | No | integer | Maximum `E1` status polls after motion commands |

Present Hamilton MVP config keys with the wrong type or invalid count/address
range are rejected instead of silently falling back to configured defaults.

## Examples

| Example | Demonstrates |
| --- | --- |
| `discover_devices` | Shows configured Hamilton Serial MVP in the two-stage discovery flow |
| `fluidics` | Runs the generic valve workflow with `ValveSelect`, a position state set, completion waits, typed valve readback, controller `last_transaction` completion-basis readback, and events |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate 7O1 serial framing, startup `U`/`LQT`/`LQP`/`E1`/`F`/`G` readback, hardware echo handling, ACK/NAK, `LPdppR`, hidden `LXR`, and idle completion against real Serial MVP hardware |
| DIN/BDZ+ | DIN protocol is not exposed without audited protocol documentation or traces for a configured device |
| Daisy chain | Validate multi-address startup, command ordering, and cross-valve state sets on real daisy-chained hardware |
| Safety | Record valve stall/encoder error behavior and recovery from hardware traces |
