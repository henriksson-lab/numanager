# Okolab Environmental Controllers

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::okolab` |
| Families | Okolab temperature, gas, humidity, and related environmental modules |
| Support level | Reverse engineered serial/configured runtime model with opt-in live read/write path |
| Evidence | Reverse engineered evidence, shipped third-party command database, and [`../reverse/okolab-protocol.md`](../reverse/okolab-protocol.md) |
| Transport | Configured cached mode by default; optional Serial/COM transport with CR-terminated plain/checksum frames behind `os-serial`; connected construction reads the configured `name_code` and matches the reply through the shipped database before loading the parameter dictionary; connected reads issue configured or dictionary-backed named read commands and parse numeric replies for temperature, CO2/O2, and database parameter values where type metadata is available |
| Validation | No numanager hardware validation note |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `okolab-hub` | `hub`, `environment.controller`, `serial.device` | Owns one controller serial resource |
| `okolab-temperature-*` | `environment.temperature`, `measure` | Temperature module/channel if detected |
| `okolab-gas-*` | `environment.gas`, `measure` | CO2/O2 module/channel if detected |
| `okolab-humidity-*` | `environment.humidity`, `measure` | Humidity module/channel if detected |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `okolab-serial` | `serial.ascii` plus optional binary checksum trailer | Plain and checksum frame serializer; resource metadata records configured `serial_port`, primary/fallback baud rates, checksum mode, and opt-in live-transport state |
| `okolab-command-database` | `third_party.database.json` | Command dictionary the driver embeds at compile time, `data/third_party/okolab/okolib.json`, extracted from the shipped `okolib.db` by `scripts/extract-okolab-db.sh`; both excluded from the repository license |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `TemperatureControl` | Temperature module | `CapabilityRequest::TemperatureControl` | Target/enabled/actual map plus completion basis | Target writes use the configured command code; `enabled` is cached only in configured mode because the recorded module abstraction lists no temperature paused property | Not sequenceable |
| `GasControl` | Gas module | `CapabilityRequest::GasControl` | CO2/O2 target/enabled/actual map plus completion basis | CO2 target writes use the configured command code; O2 target read/write is exposed when the selected product dictionary has `O2`/`O2 setpoint` or the configured product is O2-capable; `enabled` uses the database-backed `Gas control paused` parameter when available and stores the inverted cached state | Not sequenceable |
| `Measure` | Temperature, gas, humidity modules | `CapabilityRequest::Measure` | Measurement map | Cached configured value by default; connected temperature/CO2/O2 reads use configured or named database read command codes, and humidity reads use a percent-valued database humidity read code when available | Readback only |
| `GenericCommand` | Hub | `refresh_temperature_actual`, `refresh_temperature_target`, `refresh_temperature_status`, `refresh_co2_actual`, `refresh_co2_target`, `refresh_co2_status`, `refresh_o2_actual`, `refresh_o2_target`, `refresh_humidity`, or `refresh_humidity_enabled` with no params; `refresh_parameter` with string `parameter`/`name`; `write_parameter` with string `parameter`/`name`, `value`, and optional bool `volatile` | Refreshed typed property or named database-parameter result plus completion basis | Named readback/write helpers only; no arbitrary numeric command surface | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `model` | Hub | `String` | none | R | Configured product name | No | Configured identity/name-code evidence |
| `serial_number` | Hub | `String` | none | R | Configured serial | No | Configured identity |
| `firmware` | Hub | `String` | none | R | Configured firmware | No | Configured identity |
| `support_level` | Hub | `String` | none | R | Reverse engineered serial runtime model | No | Runtime metadata |
| `database_path` | Hub | `String` | none | R | Shipped repo-relative path | No | Runtime metadata |
| `database_status` | Hub | `String` | none | R | Database load status for configured product | No | Runtime metadata |
| `database_parameter_count` | Hub | `I64` | count | R | Number of loaded parameter rows for configured product | No | Runtime metadata |
| `name_code` | Hub | `I64` | native code | R | Configured/read from command database | No | Product identity command code |
| `checksum_enabled` | Hub | `Bool` | none | R | Configured | No | Frame-mode setting |
| `connected` | Hub | `Bool` | none | R | Configured live-transport flag | No | Runtime transport state |
| `fault_active` | Hub | `Bool` | none | R | Configured fault string not `none` | No | Hardware fault semantics are not exposed because fault-code evidence is absent |
| `fault` | Hub | `String` | none | R | Configured; default unknown | No | Hardware fault semantics are not exposed because fault-code evidence is absent |
| `module_summary` | Hub | `Map` | none | R | Configured module flags | No | Runtime metadata |
| `parameter_summary` | Hub | `Map` | none | R | Main database parameters for configured product, including names, type ids, units, and declared command codes | No | Runtime metadata |
| `target` | Temperature module | `Temperature` | degC/K | R/W | Configured and writable; range unchecked | No | Configured command code, volatile write |
| `actual` | Temperature module | `Temperature` | degC/K | R | Configured readback value | No | Configured command code |
| `enabled` | Temperature module | `Bool` | none | R/W in configured mode; live serial rejects writes | Configured cached state only | No | The recorded module abstraction lists no temperature paused property |
| `enabled` | Gas module | `Bool` | none | R/W | Configured cached state or inverted database-backed `Gas control paused` state | No | Uses the named `Gas control paused` database parameter when available |
| `status` | Temperature/gas module | `String` | none | R | Configured; default `unvalidated`; connected reads store `raw:<reply>` | No | Raw status readback only; hardware status semantics are not exposed because status-code evidence is absent |
| `status_read_code` | Temperature module | `I64` | native code | R | Configured command code; default `128` | No | Command database temperature status read code |
| `read_code` | Temperature module | `I64` | native code | R | Configured command code | No | Command database |
| `write_code` | Temperature module | `I64` | native code | R | Configured command code | No | Command database |
| `co2_target` | Gas module | `GasConcentration` | percent/ppm | R/W | Configured and writable; range unchecked | No | Configured command code, volatile write |
| `co2_actual` | Gas module | `GasConcentration` | percent/ppm | R | Configured readback value | No | Configured command code |
| `co2_status_read_code` | Gas module | `I64` | native code | R | Configured command code; default `129` | No | Command database CO2 status read code |
| `co2_read_code` | Gas module | `I64` | native code | R | Configured command code | No | Command database |
| `co2_write_code` | Gas module | `I64` | native code | R | Configured command code | No | Command database |
| `o2_target` | Gas module | `GasConcentration` | percent/ppm | R/W when available | Configured and writable; range unchecked | No | Selected product database `O2 setpoint`, or configured O2 command codes |
| `o2_actual` | Gas module | `GasConcentration` | percent/ppm | R when available | Configured readback value | No | Selected product database `O2`, or configured O2 command code |
| `o2_read_code` | Gas module | `I64` | native code | R when available | Selected database or configured code | No | Command database |
| `o2_write_code` | Gas module | `I64` | native code | R when available | Selected database or configured code | No | Command database |
| `relative_humidity` | Humidity module | `Ratio` | percent | R | Configured optional value or database-backed live readback | No | Uses the first available percent humidity read parameter from the shipped database, preferring `Humidity`, `Input gas Humidity`, then `Sensing cell sensor humidity` |
| `enabled` | Humidity module | `Bool` | none | R/W | Configured optional state or database-backed humidity control/activation when available | No | Uses explicit humidity enable parameters from the shipped database, preferring `Humidity control`, then `HM activation status` |
| `read_code` | Humidity module | `I64` | native code | R | Selected database humidity read code, or `0` if configured-only | No | Command database |
| `enabled_read_code` | Humidity module | `I64` | native code | R | Selected database humidity enable read code, or `0` if configured-only | No | Command database |
| `enabled_write_code` | Humidity module | `I64` | native code | R | Selected database humidity enable write code, or `0` if configured-only | No | Command database |

## Evidence Gate

| Claim | Current evidence | Default driver decision |
| --- | --- | --- |
| Serial transport | Reverse engineered notes record serial transport, CR-terminated plain frames, and optional checksum frames | Implemented as an opt-in connected transport; checksum mode uses the recorded `#` marker plus signed 16-bit trailer and frame reads tolerate raw checksum bytes; hardware validation is not recorded |
| Module inventory | The shipped SQLite database maps products to parameters and numeric command codes | Configured endpoint identity readback can update the product and parameter dictionary; module DAG remains configured until module-inventory behavior is captured from hardware; hub exposes database load status and parameter summary |
| Readback/measurements | Static grammar and read command codes are recovered | Connected temperature/CO2 reads issue configured read frames; O2 reads use the selected product dictionary or configured O2 read codes when available; gas enable readback uses the inverted named `Gas control paused` database parameter when available; humidity reads use the shipped database when a percent humidity read parameter is available; humidity enable readback uses explicit database humidity control/activation parameters when available; `refresh_parameter` issues named database-backed read frames; connected status reads expose raw replies; hardware validation still needed for units, status meanings, and faults |
| Writes/control | Static grammar and normal/volatile write command codes are recovered | Connected target writes issue configured volatile write frames for temperature, CO2, and O2 when O2 is available; gas enable writes use the inverted named `Gas control paused` database parameter when available; temperature enable writes are rejected on live serial because the recorded module abstraction lists no temperature paused property; humidity enable writes use explicit database humidity control/activation write codes when available; `write_parameter` issues named database-backed write frames when the database declares a write code; safety, ACK/error, and settling claims need hardware traces |
| Diagnostic commands | Numeric command codes are available as configured metadata and in the shipped database | Runtime `GenericCommand` is constrained to typed refresh helpers and named database parameters; arbitrary numeric command reads are not exposed as a public invocation surface |
| Completion/faults | Reverse engineered evidence records timeout, checksum mismatch, and `E1`..`E5` error replies | Runtime completion cannot be hardware-owned until status/fault frames are mapped to public errors |

## Examples

| Example | Demonstrates |
| --- | --- |
| `discover_devices` | Configured Okolab controller, temperature, and gas devices in the discovery graph |
| `environment_control okolab` | Public temperature and gas-control requests through the configured Okolab fixture |

## Remaining Work

| Area | Gap |
| --- | --- |
| Serial grammar | Static grammar implemented from [`../reverse/okolab-protocol.md`](../reverse/okolab-protocol.md); need hardware confirmation and checksum negotiation trace |
| Discovery | Configured-endpoint product identity readback is implemented from `name_code` plus database matching; module inventory packet evidence is still needed |
| Safety | Need gas/thermal/sensor fault behavior before writes |
| Driver | Extend beyond configured module topology and named database read/write only after discovery, status, safety, and completion/fault behavior are evidenced |

## Unblock Trace Checklist

Use the serial section of [`../reverse/trace-capture-guide.md`](../reverse/trace-capture-guide.md)
when collecting these observations.

| Trace item | Must record |
| --- | --- |
| Hardware identity | Controller model, firmware/library version, connected module list, OS, serial port name, and adapter/config settings |
| Serial session | Baud rate, parity, stop bits, timeout, open/reset sequence, whether checksum mode is enabled, and the console/runtime output for the same session window |
| Discovery | Raw request/reply frames for controller identity and module inventory, including addressing/checksum bytes and the matching discovered-device output |
| Readback | Raw request/reply frames for at least one temperature readback and one gas or humidity readback, with typed units from the hardware UI or SDK call result and the matching runtime value output |
| Safe write | A low-risk setpoint write, ACK/error reply, later readback, stable/busy state needed for driver-owned completion, and the matching command-completion output |
| Fault path | At least one documented or observed sensor/module disconnect, alarm, or invalid-command reply plus the runtime-visible failed operation output so errors are not inferred from SDK return codes alone |
