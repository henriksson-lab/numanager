# Brightfield Microscope Simulation

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::sim_microscope` |
| Families | Composed brightfield transmitted-light microscope simulation |
| Support level | Biological-model-oriented system simulation |
| Protocol evidence | Internal simulation model, not hardware protocol evidence |
| Transport | In-memory runtime resources |
| Discovery | Not registered; constructed directly by the software GUI |
| Validation | Local examples only |
| Runtime/evidence notes | One composed microscope whose devices share a single sample model; it is not a set of independent per-device simulations |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `sim-microscope` | `hub`, `simulator` | Hub owning the sample resource and offering the five devices |
| `sim-microscope-camera` | `camera`, `simulator` | Camera observing the shared cell-culture model |
| `sim-microscope-xy` | `stage.xy`, `axis.xy`, `simulator` | Sample-plane motion into the shared model |
| `sim-microscope-z` | `stage.z`, `axis.z`, `simulator` | Focus motion driving defocus blur in the shared model |
| `sim-microscope-objective` | `objective.turret`, `state.device`, `simulator` | Three-position turret; reaches the camera through the `objective` dependency role |
| `sim-microscope-lamp` | `light.source`, `shutter`, `simulator` | Transmitted illumination coupled to image brightness |
| `sim-microscope-sample` | resource | In-memory procedural adherent cell culture |

The graph declares `XYStage`, `ZStage` and `LightSource` dependency roles from
those devices to the camera, plus `Role::Custom("objective")` from the turret.
`Role` has no objective variant, so that string is part of this driver's
published contract: a client reads it with
`CapabilityProvider::dependency_device(&Role::Custom("objective".into()))`, and
`numanager_drivers::sim_microscope::OBJECTIVE_ROLE` exports the exact spelling.

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `CameraCapture` | Camera | `CapabilityRequest::CameraCapture` | `CapturedFrame`-parseable map plus the sample pixel size | Runtime frame-store insertion and token completion | Capture participant |
| `CameraStream` | Camera | `CapabilityRequest::CameraStream` | `CameraStreamStarted`-parseable map | Frame-ready events until the frame count is reached or the operation is cancelled | Stream participant |
| `StageMove` | XY, Z | `CapabilityRequest::StageMove` | Reached axis positions and modeled travel time | Token completes when the modeled travel finishes | Position sequences |
| `StageHome` | XY, Z | no request payload | Homed axis positions | Token completes when the modeled travel finishes | Not sequenced |
| `StageStop` | XY, Z | no request payload | Stopped-state map | Immediate; in-flight moves complete where they stopped | Not sequenced |
| `FilterSelect` | Objective turret | `CapabilityRequest::FilterSelect` | Selected position, magnification, numerical aperture, sample pixel size | Token completes when the rotation finishes | Not sequenced |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `model` | Hub | `String` | none | R | none | No | Simulation identity |
| `sample_seed` | Hub | `I64` | none | R | none | No | Cell-culture model seed |
| `exposure` | Camera | `TimeInterval` | s | R/W | 0.1 ms..10 s | Yes | Simulated integration time |
| `gain` | Camera | `Ratio` | percent | R/W | 10..1000 % | Yes | Simulated analogue gain |
| `frame_interval` | Camera | `TimeInterval` | s | R/W | 1 ms..10 s | No | Stream pacing |
| `binning` | Camera | `String` | none | R/W | `1x1`, `2x2`, `4x4` | No | Simulated readout binning |
| `pixel_pitch` | Camera | `Position` | um | R | sensor geometry | No | Simulated sensor |
| `sensor_width` | Camera | `PixelCount` | px | R | sensor geometry | No | Simulated sensor |
| `sensor_height` | Camera | `PixelCount` | px | R | sensor geometry | No | Simulated sensor |
| `sample_pixel_size` | Camera | `Position` | um | R, volatile | derived | No | `pixel_pitch * binning / magnification` |
| `pixel_format` | Camera | `String` | none | R | `Mono8` | No | Simulated readout |
| `x`, `y` | XY | `Position` | um | R/W | advertised travel, 0.1 um increment | Yes | Sample-plane position |
| `z` | Z | `Position` | um | R/W | advertised travel, 0.05 um increment | Yes | Focus height |
| `speed` | XY, Z | `Velocity` | um/s | R/W | 1..20000 um/s | No | Modeled travel rate |
| `busy` | XY, Z, turret | `Bool` | none | R, volatile | none | No | Modeled motion state |
| `position` | Objective turret | `I64` | none | R/W, volatile | 1..3 with one enum entry per objective | No | Turret slot |
| `magnification` | Objective turret | `F64` | x | R, volatile | per selected objective | No | Selected objective |
| `numerical_aperture` | Objective turret | `NumericalAperture` | none | R, volatile | per selected objective | No | Selected objective |
| `enabled` | Lamp | `Bool` | none | R/W | none | Yes | Illumination state |
| `power` | Lamp | `Ratio` | percent | R/W | 0..100 % | Yes | Illumination intensity |
| `interlock_closed` | Lamp | `Bool` | none | R | none | No | Emission-safety readback |
| `fault` | Lamp | `String` | none | R | none | No | Emission-safety readback |

`position` is deliberately **not** sequenceable: the modeled rotation takes
longer than a frame, so a timing plan could not honour one value per step.
`binning` and `pixel_format` are not sequenceable either, because changing them
mid-sequence would change the frame geometry a client has already sized buffers
for, and `frame_interval` paces a sequence rather than living inside one.

## Optical Calibration

The camera and the turret together publish everything a client needs to convert
image pixels to micrometres, with no hidden constants:

```text
sample_pixel_size = pixel_pitch * binning / magnification
field_of_view     = sample_pixel_size * sensor_width / binning
```

The camera also publishes the result as a read-only volatile
`sample_pixel_size`, because the inputs live on two different devices, and
carries it in every frame's metadata so a stored frame keeps its own
calibration. `PropertyChanged` is emitted for `magnification`,
`numerical_aperture` and `sample_pixel_size` whenever the turret lands or the
binning changes.

## Motion And Completion

Property writes to `x`, `y` and `z` apply at once: a client that drags a stage
around writes coordinates continuously, and making those writes wait on travel
time would be worse than arriving early. A write to an axis that is moving
supersedes the move, whose operation then completes with `superseded`.

`StageMove`, `StageHome` and `FilterSelect` are modeled instead. They record a
deadline, return an operation that stays in progress, interpolate the position
on each driver poll so a live stream shows the field sweeping, and complete on
arrival. `Runtime::cancel` freezes a move where it is. While the turret rotates,
the light path is blocked and frames render dark, and `magnification`,
`numerical_aperture` and `sample_pixel_size` keep reporting the outgoing
objective. A `position` property write therefore reports success before the
optics have finished changing, while `FilterSelect` completes only on arrival.

## Sample And Image Model

The culture is generated from a hash of the tile and cell index, so it is
endless in XY, needs no storage, and is identical for a given seed. Cells
carry a radius, an area-preserving elongation, an orientation, an offset
nucleus, and a height spread about a tilted culture surface, so no single focus
height brings the whole field into focus.

Defocus blur is derived from the objective:
`sigma = 0.42 * hypot(|dz| * NA, 0.61 * lambda / NA)`. Depth of field therefore
follows the numerical aperture instead of being a separate constant — the 4x
objective holds focus across tens of micrometres while the 60x objective does
not. Blur is applied by widening each cell's profile and lowering its peak so
total absorbance is conserved.

Transmitted intensity is `illumination * vignette * exp(-absorbance)`, scaled by
exposure and gain into electrons, with shot and read noise from a per-pixel hash
of the frame index. The final clamp to the 8-bit range is the saturation model,
and the number of clipped pixels is reported in frame metadata.

## Frame Metadata

Every frame carries `stage_x`, `stage_y`, `stage_z`, `objective_position`,
`magnification`, `numerical_aperture`, `pixel_pitch`, `binning`,
`sample_pixel_size`, `exposure`, `gain`, `illumination_enabled`,
`illumination_power`, `focus_offset`, `frame_index` and `saturated_pixels`, so
the acquisition state and the physical scale can be reconstructed from a stored
frame alone. `focus_offset` is the signed distance from best focus at the field
centre.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run --release -p numanager-examples --features gui -- software_gui` | Device-agnostic acquisition GUI over one composed microscope: capture and streaming, panning derived from the published optical scale, focus, objective selection, and lamp safety state |
| `cargo run -p numanager-examples --features gui -- software_gui --smoke` | Terminal validation of the same workflow, including the derived micrometres-per-image-pixel chain across an objective change |

## Remaining Work

| Area | Gap |
| --- | --- |
| Simulation policy | Keep simulation work coupled to one shared biological model per microscope rather than to independent per-device simulations |
| Biological models | Couple fluorescence channels, photobleaching, motion blur, and sample drift over time into the same model |
| Acquisition | Advertise `Mono16` once clients render more than eight bits per pixel |
| Validation | Define numeric expectations for defocus blur radius, brightfield contrast, noise, and frame timing |
