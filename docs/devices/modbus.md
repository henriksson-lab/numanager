# Modbus Mapped IO

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::modbus` |
| Families | Modbus RTU/TCP mapped IO, environment controllers, chambers, pressure/flow devices, interlock IO |
| Support level | Config-backed Modbus RTU/TCP mapped IO with reusable property map profiles, configured local register model, and explicit real transport |
| Protocol evidence | Public Modbus protocol model |
| Transport | Fixture transport by default; `connect=true` or `real_transport=true` opens configured Modbus TCP or RTU serial |
| Discovery | Config-loaded candidate; no active unit scanning |
| Validation | Local fixture plus explicit RTU/TCP backend compile; real transport validation pending |
| Runtime/evidence notes | Real RTU serial requires `os-serial`; TCP uses configured host/port through the standard network stack |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| Configured device label, default `modbus-mapped-io` | `mapped.io`, `modbus` | One logical mapped device generated from config/profile entries |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `modbus-transport` | `modbus.rtu` / `modbus.tcp` | RTU ordered-frame or TCP MBAP transaction-id correlated transport for configured register/coil maps; resource metadata records configured RTU serial or TCP endpoint fields, response timeout, retry count, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `RawRegisterAccess` | Mapped device | generic register/coil read/write request: `read_coils`, `read_discrete_inputs`, `read_holding_register`, `read_holding_registers`, `read_input_registers`, `write_single_coil`, `write_single_register`, `write_multiple_coils`, `write_multiple_registers` | Raw register/coil map or write acknowledgement metadata | Runtime token after response parse/correlation | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Configured map keys | Mapped device | `Bool`, `I64`, `F64`, `Temperature`, `Pressure`, `GasConcentration`, `FlowRate`, `Ratio`, `TimeInterval`, or `String` | Profile/config-defined | R or R/W by map | Profile/config-defined raw scaling/range | No | Coil/discrete/input/holding register address and value map |
| Built-in environment profile humidity keys | Mapped device | `Ratio` | percent | R | profile-scaled register | No | `humidity` and `relative_humidity` use percent quantities without unit suffixes |
| Built-in pressure/flow profile valve keys | Mapped device | `Ratio` | percent | R/W by map | profile-scaled register | No | `valve_position` and `valve_setpoint` use percent quantities without unit suffixes |
| Built-in interlock profile timing keys | Mapped device | `TimeInterval` | us/ms | R/W by map | profile-scaled register | No | `pulse_width` and `cdrh_delay` convert units at the Modbus boundary |
| `mapping_count` | Descriptor metadata | `I64` | count | R | number of configured maps | No | Config metadata |
| `poll_intervals` | Descriptor metadata | `Map<String, TimeInterval>` | typed time | R | per-property poll intervals | No | Config metadata |
| `response_correlation` | Descriptor metadata | `String` | none | R | `ordered-rtu-frame` or `mbap-transaction-id` | No | Transport mode |

Raw Modbus requests use zero-based `address` values and optional `count` for
read commands. Coil writes use `Bool` values; register writes use `I64` values in
the `u16` range. Multiple-write commands take a nonempty `values` list.

## Metadata And Config

| Key | Required | Type | Meaning |
| --- | --- | --- | --- |
| `transport` | No | `String` | `rtu` by default or `tcp` |
| `connect` / `real_transport` | No | `Bool` | When true, use the configured real transport instead of the fixture backend |
| `serial_port` / `port_name` | RTU real transport | `String` | Serial port for Modbus RTU |
| `baud_rate` / `baud` | No | `I64` | RTU baud rate, default 9600 |
| `serial_timeout_ms` | No | `I64` | RTU read timeout, default 1 ms |
| `tcp_host` / `host` | TCP transport | `String` | TCP host, default `127.0.0.1` |
| `tcp_port` / `port` | No | `I64` | TCP port, default 502 |
| `connect_timeout_ms` | No | `I64` | TCP connect timeout, default 1000 ms |
| `unit_id` / `unit` | No | `I64` | Modbus unit id, default 1 |
| `response_timeout_ms` | No | `I64` | Request completion timeout, default 1000 ms |
| `retries` / `retry_count` | No | `I64` | Request retry count, default 0 |
| `map_profile` | No | `String` | Built-in profile name, such as `environment_controller_basic`, `pressure_flow_controller_basic`, or `laser_safety_interlock_basic` |
| `map.<name>.quantity` | No | `String` | Converts raw/scaled registers into typed values; supports `temperature_c`, `pressure_kpa`, `gas_percent`, `flow_ul_min`, `ratio_percent`, `time_ms`, and `time_us` |
| `map.<name>.poll_interval` | No | `TimeInterval` | Canonical typed polling interval for readable properties |
| `map.<name>.poll_interval_ms` | No | `I64` milliseconds | Legacy alias accepted for existing configs |

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- digital_io modbus` | Generic mapped-IO workflow shape: state-set writes, `Runtime::wait_completed`, typed property readback, and event subscription |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate RTU timing, TCP reconnect/correlation, retries, and timeout behavior against real devices |
| Profiles | Add more manufacturer-specific chamber, pressure, gas, and interlock maps |
| Safety | Promote safety-critical properties into higher-level environment/interlock capabilities |
| Discovery | Keep unit IDs configured explicitly; verify register maps against device documentation or bench traces |
