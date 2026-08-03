# Spark Cyto

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::spark_cyto` |
| Families | Spark Cyto plate reader/imager graph/state support |
| Support level | TDCL/CAN graph/state transaction model for plate, detector, environment, imaging-head, and camera-binding workflows |
| Protocol evidence | Reverse engineered TDCL/CAN model only; authoritative TDCL/CAN source or hardware traces are still needed |
| Transport | Modeled TDCL command/data resources plus CAN gateway resource; a physical backend is not exposed because framing-to-transport binding is not evidenced |
| Discovery | Simulated two-stage discovery plus config-backed discovery |
| Validation | No hardware validation note; the active-backend boundary is missing TDCL/CAN transport, session, completion, and fault evidence |
| Runtime/evidence notes | TDCL frame helpers and command construction exist for the modeled graph; a real backend needs documented physical transport binding, endpoint/session setup, and completion/fault behavior |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `spark-mainboard` | `hub`, `plate.transport` | Offers all logical devices and owns command/data resources |
| `spark-absorbance` | `detector.absorbance` | Measurement device routed through mainboard |
| `spark-fluorescence` | `detector.fluorescence`, `light.source` | Measurement/illumination role routed through mainboard |
| `spark-luminescence` | `detector.luminescence` | Measurement device routed through mainboard |
| `spark-temperature` | `environment.temperature` | Temperature-control logical device |
| `spark-gas` | `environment.gas` | Environmental logical device with typed CO2 setpoint/readback and safety state |
| `spark-fim` | `imaging.head`, `objective.turret` | Imaging-head logical device with objective/mode and interlock/fault state |
| `spark-camera-binding` | `camera.binding` | Camera-binding logical device |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `spark-tdcl-command` | `tdcl.command` | Modeled TDCL command channel for plate, detector, environmental, gas, FIM, and camera-binding commands |
| `spark-tdcl-data` | `tdcl.data` | Modeled TDCL data channel for detector/readout payload evidence |
| `spark-can-gateway` | `can.gateway` | Modeled CAN gateway channel for lower-level module routing evidence |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `PlateMove` | Mainboard | `CapabilityRequest::PlateMove` | Well/status map | Runtime token completion | Well sequences |
| `Measure` | Absorbance/fluorescence/luminescence | `CapabilityRequest::Measure` | Measurement metadata/signal map | Runtime token completion | Wavelength and detector-enable sequences |
| `TemperatureControl` | Temperature | `CapabilityRequest::TemperatureControl` | Target/enabled state map | Runtime token completion | Target/enabled sequences |
| `GasControl` | Gas | `CapabilityRequest::GasControl` | CO2 target/readback/enabled/fault map | Runtime token completion | CO2 target/enabled sequences |
| `ImagingHead` | FIM | `CapabilityRequest::ImagingHead` | Objective/mode/interlock/fault map | Runtime token completion | Objective/mode sequences |
| `CameraBinding` | Camera binding | `CapabilityRequest::CameraBinding` | Binding/mode state map | Runtime token completion | Binding/mode sequences |
| `GenericCommand` | Hub/gateway metadata devices | `CapabilityRequest::GenericCommand` | Echoed command/parameter summary | Runtime token completion | No |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `well` | Mainboard | `String` | none | R/W | configured well labels | Yes | Plate-position TDCL parameter |
| `support_level` | Mainboard | `String` | none | R | fixed | No | Runtime-visible evidence summary |
| `wavelength` | Absorbance | `Wavelength` | nm | R/W | configured range | Yes | Detector setup TDCL parameter |
| `wavelength` | Fluorescence | `Wavelength` | nm | R/W | configured range | Yes | Detector setup TDCL parameter |
| `enabled` | Fluorescence/luminescence | `Bool` | none | R/W | none | Yes | Detector enable TDCL parameter |
| `target` | Temperature | `Temperature` | degC | R/W | configured range | Yes | Environmental setpoint TDCL parameter |
| `enabled` | Temperature | `Bool` | none | R/W | none | Yes | Environmental control TDCL parameter |
| `co2_target` | Gas | `GasConcentration` | percent | R/W | configured range | Yes | Gas setpoint TDCL parameter |
| `co2_actual` | Gas | `GasConcentration` | percent | R | configured readback | No | Gas readback TDCL parameter |
| `enabled` | Gas | `Bool` | none | R/W | none | Yes | Gas control TDCL parameter |
| `fault` | Gas | `Bool` | none | R | configured fault state | No | Gas safety state |
| `objective` | FIM | `I64` | none | R/W | 1..6 configured clamp | Yes | Objective turret TDCL parameter |
| `mode` | FIM | `String` | none | R/W | configured labels | Yes | Imaging-head mode TDCL parameter |
| `interlock_closed` | FIM | `Bool` | none | R | configured interlock state | No | Imaging-head safety state |
| `fault` | FIM | `Bool` | none | R | configured fault state | No | Imaging-head fault state |
| `bound` | Camera binding | `Bool` | none | R/W | none | Yes | Camera binding TDCL parameter |
| `imaging_mode` | Camera binding | `String` | none | R/W | configured labels | Yes | Imaging-mode TDCL parameter |

## Config Keys

For `driver = "spark_cyto"` or `driver = "spark-cyto"`, configuration can seed
the fixed Spark Cyto graph and initial state without claiming an active hardware
transport.

| Key | Type | Meaning |
| --- | --- | --- |
| `label` | `String` | Discovery label override |
| `serial_number` | `String` | Optional serial metadata propagated to descriptors/resources |
| `well` | `String` | Initial plate well |
| `absorbance_wavelength` | `Wavelength` or numeric nm | Initial absorbance wavelength |
| `fluorescence_wavelength` | `Wavelength` or numeric nm | Initial fluorescence wavelength |
| `fluorescence_enabled`, `luminescence_enabled` | `Bool` | Initial detector enable states |
| `temperature_target` | `Temperature` or numeric degC | Initial environmental target |
| `temperature_enabled` | `Bool` | Initial temperature-control enable state |
| `co2_target`, `co2_actual` | `GasConcentration` or numeric percent | Initial gas setpoint/readback |
| `gas_enabled`, `gas_fault` | `Bool` | Initial gas control/safety state |
| `fim_objective` | `I64` | Initial FIM objective index |
| `fim_mode` | `String` | Initial imaging-head mode |
| `fim_interlock_closed`, `fim_fault` | `Bool` | Initial FIM safety state |
| `camera_bound` | `Bool` | Initial camera-binding state |
| `imaging_mode` | `String` | Initial camera imaging mode |

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- spark_cyto` | Device discovery, graph topology, typed plate/measure/temperature/gas/FIM/camera-binding invocation, remultiplexed state-set submission, typed gas/FIM/environment/readout state, and acquisition timing sequences with driver-owned completion |
| `cargo run -p numanager-examples -- environment_control spark_cyto` | Generic temperature/gas workflow with typed setpoints, enabled state, safety summaries, completion waits, readback, and events |
| `cargo run -p numanager-examples -- plate_reader absorbance` | Generic plate-reader workflow with typed plate move, detector measurement, imaging-head selection, camera binding, completion waits, readback, and events |
| `cargo run -p numanager-examples -- plate_reader fluorescence` | Same workflow against the fluorescence detector, including detector enable and fluorescence imaging mode |
| `cargo run -p numanager-examples -- plate_reader luminescence` | Same workflow against the luminescence detector, including detector enable and luminescence readout path |

## Remaining Work

| Area | Gap |
| --- | --- |
| Properties | Expand gas/FIM configured properties into protocol-backed controls and richer model-specific enumerations |
| Transport | A physical TDCL/CAN backend is not exposed because transport binding, endpoint/session setup, frame boundaries, and completion/fault behavior are not evidenced |
| Protocol | Expand TDCL and CAN commands from authoritative sources or traces |
| Completion | Replace local completion with hardware status and fault events |
| Timing | Replace current software sequence endpoints with hardware-accurate plate/read/environment/gas/FIM/camera timing once authoritative protocol traces are available |
| Safety | Replace configured gas and imaging-head fault/interlock state with hardware status and fault events |
