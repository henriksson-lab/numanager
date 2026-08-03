# Thorlabs KURIOS

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::thorlabs_kurios` |
| Families | Thorlabs KURIOS liquid crystal tunable filters |
| Support level | Configured opt-in serial control/readback for KURIOS wavelength, bandwidth, output, trigger/status queries, and refresh helpers |
| Protocol evidence | Public KURIOS keyword/argument CLI behavior |
| Transport | CR-terminated ASCII over `SerialIo` |
| Discovery | Config-backed discovery; live serial requires configured endpoint and explicit connect |
| Validation | Configured serial startup-readback/control path is implemented; real hardware validation pending |
| Runtime/evidence notes | `numanager-drivers/os-serial` for explicitly constructed real serial ports |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `thorlabs-kurios-lctf` | `filter.tunable`, `lctf`, `light.filter`, `serial.ascii` | One logical tunable filter owning one serial resource |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `thorlabs-kurios-serial` | `serial` | CR-terminated KURIOS CLI command path with configured-startup readback metadata plus configured `serial_port`, `serial_timeout`, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `TriggerSink` | Tunable filter | `None` or `CapabilityRequest::Trigger` | Map with `output_enabled` and step count | Runtime token after serial writes; query replies update cached readback | Filter/output endpoint sequences |
| `GenericCommand` | Tunable filter | `refresh_telemetry`, `refresh_identity`, `refresh_wavelength`, `refresh_bandwidth`, `refresh_output`, or `refresh_status` with no params | Map with command count and telemetry summary | Uses only mapped KURIOS query readbacks; no arbitrary serial command surface | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `wavelength` | Tunable filter | `Wavelength` | named wavelength value | R/W | fixture probe min/max | Yes | `WL?` readback ingestion, `WL=<nm>` write |
| `bandwidth` | Tunable filter | `Wavelength` | named wavelength value | R/W | fixture probe min/max bandwidth | Yes | `BW?` readback ingestion, `BW=<nm>` write |
| `output_enabled` | Tunable filter | `Bool` | none | R/W | none | Yes | `OUTPUT?` readback ingestion, `OUTPUT=0/1` write |
| `trigger_mode` | Tunable filter | `String` | none | R/W | `Internal`, `External` | No | `TRIG?` readback ingestion, `TRIG=0/1` write |
| `status` | Tunable filter | `String` | none | R | device status reply | No | `STATUS?` readback ingestion |
| `firmware` | Tunable filter | `String` | none | R | firmware string | No | `VERSION?` readback ingestion |

## Config Keys

| Key | Type | Status | Meaning |
| --- | --- | --- | --- |
| `driver = "thorlabs_kurios"` | string | Canonical | Selects config-backed KURIOS discovery |
| `model` | string | Canonical | Configured model label for the probe |
| `serial_number` | string | Canonical | Configured controller serial number |
| `firmware` | string | Canonical | Configured firmware string |
| `min_wavelength`, `max_wavelength` | `Wavelength` | Canonical | Public wavelength range |
| `min_bandwidth`, `max_bandwidth` | `Wavelength` | Canonical | Public bandwidth range |
| `min_wavelength_nm`, `max_wavelength_nm`, `min_bandwidth_nm`, `max_bandwidth_nm` | scalar nanometers | Legacy aliases | Accepted for older configs |
| `serial_port` | string | Real transport | Serial device path; required when `connect = true` |
| `serial_timeout_ms` | integer milliseconds | Real transport | Serial timeout, default `100` |
| `connect` | bool | Real transport | Opens `serial_port` with `os-serial`; false keeps the configured state model |

When `connect = true`, discovery opens the configured serial endpoint, runs the
configured startup-readback script, and seeds cached model, serial, firmware, status,
wavelength, bandwidth, output, and trigger-mode state from controller replies
before registering the driver.

Runtime property reads issue the mapped query before returning cached state.
Writable wavelength, bandwidth, output, and trigger-mode properties send the
setter and then request the corresponding query readback when the controller
returns it.

The tunable-filter `GenericCommand` capability exposes read-only refresh
helpers. Each helper issues the same mapped KURIOS query commands used by
runtime property reads and updates the cached state from replies.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- filters kurios` | Generic tunable-filter workflow with typed wavelength/bandwidth properties, output enable/disable, `Runtime::wait_completed`, timing plan, and readback |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate query names, ingested reply forms, range handling, and status/busy semantics against real KURIOS units |
| Discovery/config | Explicit configured endpoints are supported; model/range reconciliation requires manufacturer documentation or captured startup readback |
| Timing | Validate software sequence behavior against hardware-accurate trigger/output behavior if the controller supports it |
| Compatibility | Capture model-specific bandwidth/range behavior and command differences |
