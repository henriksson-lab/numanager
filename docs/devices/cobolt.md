# Cobolt / Hubner Lasers

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::cobolt` |
| Families | Cobolt and Hubner serial lasers |
| Support level | Configured opt-in serial laser control/readback with telemetry refresh commands |
| Protocol evidence | Public Cobolt serial commands |
| Transport | Serial ASCII over `SerialIo` |
| Discovery | Config-backed discovery; live serial requires configured endpoint and explicit connect |
| Validation | Configured serial startup-readback/control path is implemented; real hardware validation pending |
| Runtime/evidence notes | `numanager-drivers/os-serial` for configured real serial ports |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `cobolt-laser` | `laser`, `light.source`, `trigger.sink` | One logical laser on one serial resource |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `cobolt-serial` | `serial` | Cobolt/Hubner serial ASCII command path with primary/fallback baud metadata, configured-startup readback metadata, and resource metadata for configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `Dac` | Laser | `CapabilityRequest::Dac` with `Value::OpticalPower` | Power setpoint | Runtime token after serial command; `power` property event | Power endpoint sequences through typed `power` property |
| `TriggerSink` | Laser | `None` or `CapabilityRequest::Trigger` enable/disable/pulse | Emission status map | Runtime token after serial command; query replies refresh cached state | Emission endpoint sequences |
| `GenericCommand` | Laser | `refresh_telemetry`, `refresh_enabled`, `refresh_power`, `refresh_actual_power`, `refresh_current`, `refresh_control_mode`, `refresh_autostart`, `refresh_interlock`, `refresh_fault`, or `refresh_hours` with no params | Refreshed property value or telemetry map | Sends only mapped documented query commands, ingests replies, and emits property changes where values update; no raw serial command surface | Readback/bring-up only |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `enabled` | Laser | `Bool` | none | R/W | none | Yes | `l?` readback; `l1`/`l0` write |
| `power` | Laser | `OpticalPower` | named power value | R/W | 0..advertised max | Yes | `p?` readback; `p <W>` write |
| `actual_power` | Laser | `OpticalPower` | named power value | R | telemetry | No | `pa?` readback |
| `current` | Laser | `ElectricCurrent` | named current value | R/W | 0..advertised max | No | `i?` readback; `slc <mA>` write |
| `actual_current` | Laser | `ElectricCurrent` | named current value | R | telemetry | No | `i?` readback |
| `control_mode` | Laser | `String` | none | R/W | configured laser modes | No | `gom?` readback; `cp`/`ci`/`em` write |
| `autostart` | Laser | `Bool` | none | R/W | none | No | `@cobas?` readback; `@cobas <0/1>` write |
| `interlock_closed` | Laser | `Bool` | none | R | safety state | No | `ilk?` readback |
| `fault` | Laser | `String` | none | R | fault labels | No | `f?` readback |
| `telemetry_summary` | Laser | `Map` | none | R | model, firmware, typed setpoints, telemetry, safety, limits, usage hours | No | Composite query readback over `l?`, `p?`, `pa?`, `i?`, `gom?`, `@cobas?`, `ilk?`, `f?`, `hrs?` |
| `wavelength` | Laser | `Wavelength` | named wavelength value | R | model/probe value | No | Model/probe metadata |
| `hours` | Laser | `TimeInterval` | h | R | usage telemetry | No | `hrs?` readback |

## Config Keys

| Key | Type | Status | Meaning |
| --- | --- | --- | --- |
| `wavelength` | `Wavelength` | Canonical | Laser wavelength |
| `max_power` | `OpticalPower` | Canonical | Advertised maximum output power |
| `max_current` | `ElectricCurrent` | Canonical | Advertised maximum diode/current-loop current |
| `power` | `OpticalPower` | Canonical | Initial power setpoint |
| `actual_power` | `OpticalPower` | Canonical | Initial configured telemetry value |
| `current` | `ElectricCurrent` | Canonical | Initial current setpoint |
| `actual_current` | `ElectricCurrent` | Canonical | Initial configured current telemetry value |
| `hours_interval` | `TimeInterval` | Canonical | Initial configured usage-hours value |
| `wavelength_nm`, `max_power_mw`, `max_current_ma`, `power_mw`, `actual_power_mw`, `current_ma`, `actual_current_ma` | Scalar | Legacy aliases | Accepted for older configs |

When `connect = true`, discovery opens the configured serial endpoint, runs the
configured startup-readback script, and seeds cached identity, limit, emission, power/current,
operating-mode, interlock, fault, autostart, and usage-hour state from laser
replies before registering the driver. Runtime `GenericCommand` is constrained
to the documented query-backed refresh commands above with no params; it is not
a raw serial escape.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- laser [cobolt]` | Generic laser `Dac` and `TriggerSink`, typed optical-power output, emission enable/disable, safety summary, and public readback |
| `cargo run -p numanager-examples -- light_source` | Generic light-source `Dac` and `TriggerSink`, typed intensity/enable properties, remultiplexed state set, `Runtime::wait_completed`, timing plan, and readback |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate command grammar, ingested query reply forms, interlock/fault behavior, and emission timing against real lasers |
| Safety | Complete warmup, CDRH delay, interlock reset, and fault recovery model |
| Timing | Hardware trigger mode and modulation behavior beyond output gating |
| Newer CoboltOfficial coverage | Micro-Manager CoboltOfficial has 2025-2026 manufacturer-authored updates for 05/Gen5 lasers, 12 V MLD/DPL variants, 5 V shutter-command handling, and Skyra line-addressed control. Candidate gaps include `laser:*`, `system:input:*`, `autostart:*`, `fault:clear`, firmware/key-switch/state queries, modulation input voltage/impedance, modulation current/power setpoints, and Skyra per-line commands. Implement source-backed coverage as far as the recorded Micro-Manager source identifies command semantics; use a Cobolt/Huebner manual revision, captured traffic, or a bench note to promote generation-specific command semantics and safety behavior to bench-validated support. |
