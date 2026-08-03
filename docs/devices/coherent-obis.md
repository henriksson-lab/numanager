# Coherent OBIS Lasers

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::coherent_obis` |
| Families | Coherent OBIS serial lasers |
| Support level | Configured opt-in serial laser control/readback and refresh helpers |
| Protocol evidence | Public OBIS SCPI-style command behavior |
| Transport | Serial ASCII over `SerialIo` |
| Discovery | Config-backed discovery; live serial requires configured endpoint and explicit connect |
| Validation | Configured serial startup-readback/control path is implemented; real hardware validation pending |
| Runtime/evidence notes | `numanager-drivers/os-serial` for configured real serial ports |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `coherent-obis-laser` | `laser`, `light.source`, `trigger.sink` | One logical laser on one serial resource |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `coherent-obis-serial` | `serial` | CR/LF SCPI-like serial command path using `SYST<n>` and `SOUR<n>` prefixes plus configured-startup readback metadata for configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `Dac` | Laser | `CapabilityRequest::Dac` | Power/status map | Runtime token after serial command; query replies refresh cached state | Optical-power endpoint sequences |
| `TriggerSink` | Laser | `None` or `CapabilityRequest::Trigger` | Emission status map | Runtime token after serial command; emission-state replies refresh cached state | Emission endpoint sequences |
| `GenericCommand` | Laser | `refresh_telemetry`, `refresh_identity`, `refresh_power`, `refresh_status`, or `refresh_limits` with no params | Refreshed telemetry map | Uses only mapped OBIS query readbacks; no arbitrary serial command, communication setup, or error-clear surface | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `enabled` | Laser | `Bool` | none | R/W | none | Yes | `SOUR<n>:AM:STATE?` readback; `SOUR<n>:AM:STATE <On/Off>` write |
| `power` | Laser | `OpticalPower` | named power value | R/W | advertised min..max | Yes | `SOUR<n>:POW:LEV:IMM:AMPL?` readback; `SOUR<n>:POW:LEV:IMM:AMPL <W>` write |
| `actual_power` | Laser | `OpticalPower` | named power value | R | telemetry | No | `SOUR<n>:POW:LEV:IMM:AMPL?` fallback when a model-specific measured-power query is not validated |
| `wavelength` | Laser | `Wavelength` | named wavelength value | R | probe wavelength | No | `SYST<n>:INF:WAV?` readback |
| `analog_modulation` | Laser | `Bool` | none | R/W | none | No | `SOUR<n>:AM:STATE?` readback; `SOUR<n>:AM:STATE <On/Off>` write |
| `cdrh_delay` | Laser | `Bool` | none | R/W | none | No | `SOUR<n>:AM:SOUR?` readback; `SOUR<n>:AM:SOUR CDRH/CW` write |
| `mode` | Laser | `String` | none | R/W | fixture modes | No | `SOUR<n>:AM:SOUR?` readback; `SOUR<n>:AM:SOUR CDRH/CW` write |
| `fault` | Laser | `String` | none | R | fault labels | No | `SYST<n>:ERR?` readback |
| `head_id` | Laser | `String` | none | R | head identity | No | `SYST<n>:INF:SNUM?` readback |
| `telemetry_summary` | Laser | `Map` | none | R | head, typed power and wavelength, emission, modulation, mode, fault, usage hours | No | Composite query readback over `AM:STATE?`, `POW:LEV:IMM:AMPL?`, `INF:WAV?`, `AM:SOUR?`, `ERR?`, `INF:SNUM?`, and `DIOD:HOUR?` |
| `head_hours` | Laser | `TimeInterval` | h | R | head usage telemetry | No | `SYST<n>:DIOD:HOUR?` readback |

## Config Keys

| Key | Type | Status | Meaning |
| --- | --- | --- | --- |
| `wavelength` | `Wavelength` | Canonical | Laser head wavelength |
| `min_power` | `OpticalPower` | Canonical | Advertised minimum output power |
| `max_power` | `OpticalPower` | Canonical | Advertised maximum output power |
| `power` | `OpticalPower` | Canonical | Initial power setpoint |
| `actual_power` | `OpticalPower` | Canonical | Initial configured telemetry value |
| `wavelength_nm`, `min_power_mw`, `max_power_mw`, `power_mw`, `actual_power_mw` | Scalar | Legacy aliases | Accepted for older configs |
| `head_hours` | `TimeInterval` | Canonical | Initial configured head-usage telemetry value |
| `head_hours` as scalar or `head_hours_h` | scalar hours | Legacy alias | Accepted for older configs |

When `connect = true`, discovery opens the configured serial endpoint, runs the
configured startup-readback script, and seeds cached head identity, wavelength, power limits,
power setpoint, emission/modulation mode, fault, and usage-hour state from
laser replies before registering the driver.
Writable emission, power, analog-modulation, mode, and CDRH-delay paths request
the corresponding query plus `SYST<n>:ERR?` after the write and ingest replies
when available. Newly ingested nonzero fault replies fail the operation.
Laser `GenericCommand` refresh helpers issue only the mapped readback queries
already used by property reads and telemetry summaries. They do not expose raw
serial commands, communication setup, or error-clear commands.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- laser obis` | Generic laser `Dac` and `TriggerSink`, typed optical-power output, emission enable/disable, safety summary, and public readback |
| `cargo run -p numanager-examples -- light_source obis` | Generic light-source `Dac` and `TriggerSink`, typed optical-power/enable properties, remultiplexed state set, `Runtime::wait_completed`, timing plan, and readback |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate SCPI command variants, power units, ingested status/fault reply forms, and emission timing against real lasers |
| Safety | Complete CDRH/interlock/fault reset model |
| Timing | Hardware modulation and trigger-mode behavior |
