# Omicron Serial Lasers

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::omicron` |
| Families | Omicron xX/LuxX/PhoxX serial lasers |
| Support level | Configured opt-in serial control/readback for Omicron query/write commands and refresh helpers |
| Protocol evidence | Public serial command examples |
| Transport | Serial ASCII over `SerialIo` |
| Discovery | Config-backed discovery; live serial requires configured endpoint and explicit connect |
| Validation | Configured serial startup-readback/control path is implemented; real hardware validation pending |
| Runtime/evidence notes | `numanager-drivers/os-serial` for configured real serial ports |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `omicron-serial-laser` | `laser`, `light.source`, `trigger.sink` | One logical laser on one serial resource |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `omicron-serial` | `serial` | Serial command path for legacy Omicron query/write commands, configured-startup readback metadata, and resource metadata for configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `Dac` | Laser | `CapabilityRequest::Dac` | Power/level map | Runtime token after serial command; query replies refresh cached state | Typed optical-power endpoint sequences |
| `TriggerSink` | Laser | `None` or `CapabilityRequest::Trigger` | Emission status map | Runtime token after serial command; laser-state replies refresh cached state | Emission endpoint sequences |
| `GenericCommand` | Laser | `refresh_telemetry`, `refresh_identity`, `refresh_power`, `refresh_status`, or `refresh_temperatures` with no params | Refreshed telemetry map | Uses only mapped Omicron query readbacks; no arbitrary serial command surface; fault reset remains hidden from regular and advanced command surfaces | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `enabled` | Laser | `Bool` | none | R/W | none | Yes | `?GAS` readback; `?LOn`/`?LOf` write |
| `power` | Laser | `OpticalPower` | named power value | R/W | 0..specified power | Yes | `?GLP` readback with DAC-to-power conversion; `?SLP<hex>` write |
| `relative_power` | Laser | `Ratio` | percent | R/W | 0..100 | Yes | `?GLP` readback with DAC-to-percent conversion; `?SLP<hex>` write; `power_percent` accepted as legacy alias |
| `actual_power` | Laser | `OpticalPower` | named power value | R | telemetry | No | `?MDP` readback |
| `wavelength` | Laser | `Wavelength` | named wavelength value | R | probe wavelength | No | `?GSI` readback |
| `operating_mode` | Laser | `String` | none | R/W | operating modes | No | `?GOM` readback; `?SOM<hex>` write |
| `cw_submode` | Laser | `String` | none | R/W | `ACC`, `APC` variants | No | `?GOM` readback; `?SOM<hex>` write |
| `operating_bits` | Laser | `I64` | none | R | raw bitfield | No | `?GOM` readback |
| `operating_flags` | Laser | `String` | none | R | decoded bit labels | No | `?GOM` readback |
| `analog_modulation_enabled` | Laser | `Bool` | none | R/W | decoded bit | Yes | `?GOM` readback; `?SOM<hex>` write preserving other operating bits |
| `digital_modulation_enabled` | Laser | `Bool` | none | R/W | decoded bit | Yes | `?GOM` readback; `?SOM<hex>` write preserving other operating bits |
| `apc_enabled` | Laser | `Bool` | none | R | decoded bit | No | `?GOM` readback |
| `interlock_closed` | Laser | `Bool` | none | R | safety state | No | `?GFB` readback |
| `fault` | Laser | `String` | none | R | fault labels | No | `?GFB` readback |
| `fault_bits` | Laser | `I64` | none | R | raw fault bitfield | No | `?GFB` readback |
| `fault_flags` | Laser | `String` | none | R | decoded fault labels | No | `?GFB` readback |
| `serial_number` | Laser | `String` | none | R | device identity | No | `?GSN` readback |
| `hours` | Laser | `TimeInterval` | h | R | usage telemetry | No | `?GWH` readback |
| `diode_temperature` / `baseplate_temperature` | Laser | `Temperature` | named temperature value | R | telemetry | No | `?MTD` / `?MTA` readback |
| `telemetry_summary` | Laser | `Map` | none | R | identity, typed power/temperature/usage, operating bits, fault/interlock state | No | Composite query readback over `?GAS`, `?GLP`, `?MDP`, `?GSI`, `?GOM`, `?GFB`, `?GSN`, `?GWH`, `?MTD`, and `?MTA` |

## Config Keys

| Key | Type | Status | Meaning |
| --- | --- | --- | --- |
| `wavelength` | `Wavelength` | Canonical | Laser wavelength |
| `specified_power` | `OpticalPower` | Canonical | Nameplate power used for DAC-to-power conversion |
| `power` | `OpticalPower` | Canonical | Initial power setpoint |
| `relative_power` | `Ratio` | Canonical | Initial relative-power setpoint |
| `actual_power` | `OpticalPower` | Canonical | Initial configured telemetry value |
| `hours_interval` | `TimeInterval` | Canonical | Initial configured usage-hours value |
| `diode_temperature`, `baseplate_temperature` | `Temperature` | Canonical | Initial configured temperature telemetry values |
| `wavelength_nm`, `specified_power_mw`, `power_mw`, `power_percent`, `actual_power_mw`, `diode_temperature_c`, `baseplate_temperature_c` | Scalar | Legacy aliases | Accepted for older configs |
| `power_level` | `I64` | Native controller value | Raw 12-bit Omicron DAC level for protocol bring-up |

When `connect = true`, discovery opens the configured serial endpoint, runs the
configured startup-readback script, and seeds cached identity, specification, emission,
power-level, measured-power, operating-mode, fault, usage-hour, and temperature
state from laser replies before registering the driver.
Writable emission, power, relative-power, operating-mode, and modulation paths
request the corresponding query plus fault readback after the write and ingest
replies when available. Newly ingested nonzero fault bits fail the operation.
Laser `GenericCommand` refresh helpers issue only the mapped readback queries
already used by property reads and telemetry summaries. They do not expose raw
serial commands or the fault-reset side-effect command.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- laser omicron` | Generic laser `Dac` and `TriggerSink`, typed optical-power output, emission enable/disable, safety summary, and public readback |
| `cargo run -p numanager-examples -- light_source omicron` | Generic light-source `Dac` and `TriggerSink`, typed optical-power/enable properties, remultiplexed state set, `Runtime::wait_completed`, timing plan, and readback |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate legacy command variants, DAC scaling, ingested actual-power/status reply forms, and emission timing |
| Safety | Complete all documented fault bits, interlock reset, and key-switch behavior |
| Timing | Hardware validation of modulation-mode switching and external trigger timing beyond software output gating |
