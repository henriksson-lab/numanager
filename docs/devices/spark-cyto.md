# Spark Cyto

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::spark_cyto` |
| Families | Spark Cyto plate reader/imager graph/state support |
| Support level | TDCL/CAN graph and transaction model for plate, detector, environment, motion, optics-carrier, injector, barcode, imaging-head, and camera-binding workflows |
| Protocol evidence | Reverse engineered from the vendor Windows stack; recorded in [`../reverse/spark-cyto-protocol.md`](../reverse/spark-cyto-protocol.md) with the open questions in [`../reverse/spark-cyto.md`](../reverse/spark-cyto.md) |
| Transport | TDCL over USB (`spark::usb`, `os-usb`): commands out on BULK-OUT #0, replies in on INTERRUPT-IN #0, measurement data on BULK-IN #0. The reader's VID/PID is **not** evidenced and has no default — it comes from configuration, and `connect()` fails naming that when it is absent |
| Discovery | Simulated two-stage discovery plus config-backed discovery |
| Validation | No hardware validation note. Nothing here has met an instrument: command spellings taken from the recovered command dictionary are marked `// dictionary` in `spark::backend`, and the rest are inferred |
| Runtime/evidence notes | With a transport attached, a request the instrument has no established command for fails explicitly rather than being answered from the model. Without one, the modeled path answers everything, which is what the examples exercise |

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
| `spark-camera-binding` | `camera.binding` | Reader-side binding state for the imaging camera |
| `spark-camera` | `camera` | The imaging camera as the reader presents it; frames arrive on the TDCL data channel |
| `spark-stage-xy` | `stage.xy`, `axis.xy` | Imaging-module X/Y axes |
| `spark-stage-z` | `stage.z`, `axis.z` | Imaging-module focus axis — focus is motion on this instrument, not a camera setting |
| `spark-filter-excitation` | `filter.wheel` | Excitation filter slide (`FILTER_EX`) |
| `spark-mirror` | `mirror.turret` | Dichroic/mirror carrier (`MIRROR1`) |
| `spark-injector-a`, `spark-injector-b` | `injector` | The two injector pumps (`PUMP=A`, `PUMP=B`) |
| `spark-barcode` | `barcode.reader` | Plate barcode reader |
| `spark-shaker` | `shaker` | Plate shaker |
| `spark-lid` | `lid` | Lid lifter; driven by a property write rather than a capability |
| `spark-autofocus` | `autofocus` | The camera's own autofocus sweep |

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
| `StageMove`, `StageHome` | XY/Z stages | `CapabilityRequest::StageMove` / no-request home | Position map | Runtime token completion | Position sequences |
| `FilterSelect` | Filter slide, mirror carrier | `CapabilityRequest::FilterSelect` | Position map | Runtime token completion | Position sequences |
| `Inject` | Injector A/B | `CapabilityRequest::Inject` | Pump/action/volume map | Runtime token completion | No |
| `Barcode` | Barcode reader | `CapabilityRequest::Barcode` | Decoded text, or `Null` for an unlabelled plate | Runtime token completion | No |
| `CameraCapture` | Camera | `CapabilityRequest::CameraCapture` | Geometry/format map, plus a `FrameReady` frame | Runtime token completion | No |
| `Shake` | Shaker | `CapabilityRequest::Shake` | Mode/amplitude/frequency map | Runtime token completion | Mode/amplitude/frequency sequences |
| `Autofocus` | Autofocus | `CapabilityRequest::Autofocus` (single-shot only) | `max_value`/`std_dev` of the peak found | Runtime token completion | No |
| `GenericCommand` | Hub/gateway metadata devices | `CapabilityRequest::GenericCommand` | Echoed command/parameter summary | Runtime token completion | No |

### Where camera frames come from

The imaging camera is a stock IDS uEye on its own USB connection, but the reader firmware
drives it and **uploads the raster on the TDCL data channel** — `CAMERA TAKEIMAGE` answers with
one `0x88` header frame plus `0x83` payload frames, rows of `width * bits_per_pixel / 8` bytes.
So this driver serves `CameraCapture` itself, with no vendor SDK and without opening the
camera's own USB device.

The frame's geometry is read from the instrument (`?CAMERA AOI`, `?CAMERA BITSPERPIXEL`), never
assumed: the vendor stack queries the sensor at runtime too, and nothing in the evidence fixes
a size. A payload whose length disagrees with that geometry **fails the operation** rather than
being cropped or padded — a reshaped raster is a picture of something that was not measured.

Without a transport there are no pixels and the capture fails saying so; this driver has no
scene to render, and a simulator driver is the right tool for a modeled camera.

`spark-camera-binding` remains the reader-side binding state. A camera from a *different*
driver can also be attached to it with
`LocalRuntime::bind_device(camera, spark_camera_binding, Role::Camera)`, which is how a host
records which camera belongs to which instrument when more than one is present.

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `well` | Mainboard | `String` | none | R/W | configured well labels | Yes | Plate-position TDCL parameter |
| `support_level` | Mainboard | `String` | none | R | fixed | No | Runtime-visible evidence summary |
| `instrument_type` | Mainboard | `String` | none | R | e.g. `SPARK 10M` | No | `?INFO INSTRUMENT_TYPE`; `Null` until answered |
| `hardware_version` | Mainboard | `String` | none | R | instrument readback | No | `?INFO HARDWARE_VERSION`; `Null` until answered |
| `state` | Mainboard | `String` | none | R | `STANDBY`/`READY` | No | `?INSTRUMENT STATE`; `Null` until answered |
| `modules` | Mainboard | `String` | none | R | `NAME:NUMBER` pairs | No | `#MODULE EXPECTED_USB`/`EXPECTED_CAN`; `unknown` until answered |
| `wavelength` | Absorbance | `Wavelength` | nm | R/W | configured range | Yes | Detector setup TDCL parameter |
| `wavelength` | Fluorescence | `Wavelength` | nm | R/W | configured range | Yes | Detector setup TDCL parameter |
| `enabled` | Fluorescence/luminescence | `Bool` | none | R/W | none | Yes | Detector enable TDCL parameter |
| `target` | Temperature | `Temperature` | degC | R/W | configured range | Yes | `TEMPERATURE DEVICE=AMBIENTCONTROL TARGET=` (degC x 100) |
| `actual` | Temperature | `Temperature` | degC | R | instrument readback | No | `?SENSORVALUE TEMPERATURE AMBIENTCONTROL`. **`Null` until the instrument answers** — a setpoint reported as a reading hides an incubator that is not heating |
| `enabled` | Temperature | `Bool` | none | R/W | none | Yes | Environmental control TDCL parameter |
| `co2_target` | Gas | `GasConcentration` | percent | R/W | configured range | Yes | Gas setpoint TDCL parameter |
| `co2_actual` | Gas | `GasConcentration` | percent | R | instrument readback | No | `?GASCONTROL ACTUAL_CONCENTRATION GAS=CO2`; `Null` until answered |
| `o2_target` | Gas | `GasConcentration` | percent | R/W | 0.1-21 % | Yes | `GASCONTROL GAS=O2 RATED_CONCENTRATION=` (percent x 10000) |
| `o2_actual` | Gas | `GasConcentration` | percent | R | instrument readback | No | `?GASCONTROL ACTUAL_CONCENTRATION GAS=O2`; `Null` until answered |
| `enabled` | Gas | `Bool` | none | R/W | none | Yes | Gas control TDCL parameter |
| `fault` | Gas | `Bool` | none | R | configured fault state | No | Gas safety state |
| `objective` | FIM | `I64` | none | R/W | 1..6 configured clamp | Yes | Objective turret TDCL parameter |
| `mode` | FIM | `String` | none | R/W | configured labels | Yes | Imaging-head mode TDCL parameter |
| `interlock_closed` | FIM | `Bool` | none | R | configured interlock state | No | Imaging-head safety state |
| `fault` | FIM | `Bool` | none | R | configured fault state | No | Imaging-head fault state |
| `bound` | Camera binding | `Bool` | none | R/W | none | Yes | Camera binding TDCL parameter |
| `imaging_mode` | Camera binding | `String` | none | R/W | configured labels | Yes | Imaging-mode TDCL parameter |
| `x`, `y` | XY stage | `Position` | um | R | instrument readback | No | `?ABSOLUTE`; `Null` until answered, and on an axis that counts motor steps |
| `z` | Z stage | `Position` | um | R | instrument readback | No | `?ABSOLUTE`; same. Focus is the imaging module's `Z_OBJECTIVE`, not the main board's `Z` — they are different axes on different modules |
| `unit` | XY/Z stage | `String` | none | R | `um` or `step` | No | Read from the axis's own `#ABSOLUTE` range reply. See "Axis units" below |
| `position` | Filter slide, mirror | `I64` | none | R/W | positions the fitted slide has | Yes | `MOVE CARRIER=FILTER_EX\|MIRROR1 POSITION=` |
| `slots` | Filter slide, mirror | `I64` | none | R | instrument readback | No | `#EXCITATION`/`#MIRROR CARRIER=`; `Null` until answered |
| `fitted` | Filter slide, mirror | `String` | none | R | instrument readback | No | What the carrier reported is fitted, verbatim |
| `pump` | Injector | `String` | none | R | `A` or `B` | No | `PUMP=` token |
| `barcode` | Barcode reader | `String` | none | R | decoded text | No | `BARCODE READ`; `Null` for an unlabelled plate |
| `exposure` | Camera | `TimeInterval` | s | R/W | camera range | Yes | `CAMERA EXPOSURETIME=` (microseconds) |
| `width`, `height` | Camera | `PixelCount` | none | R | instrument readback | No | `?CAMERA AOI`; `Null` until answered |
| `pixel_format` | Camera | `String` | none | R | `Mono8`/`Mono10`/`Mono12` | No | `?CAMERA BITSPERPIXEL`; `Null` until answered |
| `mode` | Shaker | `String` | none | R/W | `LINEAR`/`ORBITAL`/`DOUBLE` | Yes | `MODE SHAKING=` |
| `amplitude` | Shaker | `Position` | um | R/W | device range | Yes | `SHAKING AMPLITUDE=` (micrometres) |
| `frequency` | Shaker | `Frequency` | Hz | R/W | device range | Yes | `SHAKING FREQUENCY=` (tenths of a hertz) |
| `state` | Lid | `String` | none | R/W | device lid states | Yes | `LIDLIFT STATE=` |
| `max_value`, `std_dev` | Autofocus | `F64` | none | R | sweep result | No | `?CAMERA AUTOFOCUSDETAIL IMAGE=`; `Null` until a sweep has run |

### A measurement is a sequence, in this order

`MODE MEASUREMENT=` → `PREPARE MODE= REFERENCE= LABEL=` → optics (wavelength, integration
time) → `MEASUREMENT START` → `SCAN LABEL=` → `MEASUREMENT END`.

The mode and the optics are set *before* the window opens, not inside it, and `PREPARE` runs
the reference read whose counts the absorbance ratio divides by — it emits its own data
package, which the completion carries as `reference_read`. Luminescence has no reference
channel and asks for none.

### Bring-up is two phases

Attaching a transport asks what the instrument is before asking it to do anything:
`?INFO SAP_SERIAL_INSTRUMENT`, `?INFO INSTRUMENT_TYPE`, `?INFO HARDWARE_VERSION`,
`?INSTRUMENT STATE`, then `#MODULE EXPECTED_USB` and `#MODULE EXPECTED_CAN`. The enumeration
answers `NAME:NUMBER|NAME:NUMBER`, which is what fills in the module numbers.

Only when the last enumeration lands does the rest of the bring-up run — the chamber sensors,
the axis units, the axis positions and the camera geometry. It has to be that way round:
those commands name a module, and a module number that has not been read yet cannot be sent.

Which imaging module is fitted is discovered the same way. `CELL` (brightfield) and `FIM`
(fluorescence) take the same commands except that the brightfield module's `TAKEIMAGE` needs a
`TYPE=`; an instrument carrying both is driven as `FIM`.

### A position the slide does not have is refused

Firmware may clamp an out-of-range carrier position into one holding different glass and report
success, which turns a failed run into a wrong one. On connect each carrier is asked what is
fitted to it (`#EXCITATION CARRIER=`, `#MIRROR CARRIER=`), and the `|`-separated reply says how
many positions exist. A position beyond that is refused before anything is sent.

The check only applies once the carrier has answered. A slide that has not reported is
`slots: Null`, not zero — refusing against a guessed count would be its own kind of wrong, and
an unanswered carrier is not an empty one.

### Property writes reach the instrument

A write to a property the instrument has a command for is sent when it is made, rather than
being held until some later capability request carries the value along: `target`, `enabled`,
`co2_target`, `o2_target`, `objective`, `mode`, carrier `position`, and the lid's `state` all
go to the wire. Writes with no command behind them — the well a measurement will address, a
detector's tuned wavelength, which camera is bound, the camera exposure — stay driver-side and
ride with the operation that uses them.

### Axis units

The instrument declares what each axis counts in: a range reply is `{from}~{to}%{step} [unit]`
and the unit token is either `um` or `step`. This driver asks (`#ABSOLUTE X\|Y\|Z`) when a
transport is attached and caches the answer.

A move is refused, with a sentence saying why, when the axis has not answered yet, and when it
counts in steps — because how many steps make a micrometre is a property of the mechanism and
is not recorded anywhere. `x`/`y`/`z` read `Null` in the same two cases. The alternative would
be a plausible wrong number in every archived position.

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
| `o2_target`, `o2_actual` | `GasConcentration` or numeric percent | Initial oxygen setpoint/readback |
| `vendor_id`, `product_id` | `I64` or `"0x…"` string | The reader's USB identity. **No default**: it is not in the recovered evidence, so `connect()` fails until a bench records it |
| `imaging_module`, `injector_module`, `gas_module`, `barcode_module` | `I64` | Module numbers, as an override. Normally **discovered** from `#MODULE` on connect; omitted from a command when neither configured nor discovered, rather than guessed |

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- spark_cyto` | Device discovery, graph topology, typed plate/measure/temperature/gas/FIM/camera-binding invocation, focus motion, excitation-filter selection, an injector dispense, a barcode read, remultiplexed state-set submission, typed gas/FIM/environment/readout state, and acquisition timing sequences with driver-owned completion |
| `cargo run -p numanager-examples -- environment_control spark_cyto` | Generic temperature/gas workflow with typed setpoints, enabled state, safety summaries, completion waits, readback, and events |
| `cargo run -p numanager-examples -- plate_reader absorbance` | Generic plate-reader workflow with typed plate move, detector measurement, imaging-head selection, camera binding, completion waits, readback, and events |
| `cargo run -p numanager-examples -- plate_reader fluorescence` | Same workflow against the fluorescence detector, including detector enable and fluorescence imaging mode |
| `cargo run -p numanager-examples -- plate_reader luminescence` | Same workflow against the luminescence detector, including detector enable and luminescence readout path |

## Remaining Work

| Area | Gap |
| --- | --- |
| Properties | Expand gas/FIM configured properties into protocol-backed controls and richer model-specific enumerations |
| Transport | The reader's VID/PID and endpoint layout need an `lsusb -v`; the CAN module numbers need a `#MODULE` sweep |
| Protocol | Confirm the inferred command spellings against a capture — monochromator tuning, carrier selection, objective selection, and the environmental enable form are the least established |
| Camera | Confirm what the image's `0x88` header carries, whether `PREPARETAKEIMAGE`+`FETCHIMAGE` differs from the single-command form, and whether the brightfield module needs its `TYPE=` beyond what is sent |
| Completion | Replace local completion with hardware status and fault events |
| Timing | Replace current software sequence endpoints with hardware-accurate plate/read/environment/gas/FIM/camera timing once authoritative protocol traces are available |
| Safety | Replace configured gas and imaging-head fault/interlock state with hardware status and fault events |
