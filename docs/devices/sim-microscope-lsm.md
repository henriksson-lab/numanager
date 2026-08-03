# Composed Microscope And LSM Simulation

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::sim_microscope_lsm` |
| Families | Brightfield camera plus laser-scanning microscope simulation |
| Support level | Composed simulator over one seeded specimen and one shared microscope state |
| Protocol evidence | Internal simulation model, not hardware protocol evidence |
| Transport | In-memory runtime resources |
| Discovery | Constructed directly by examples or simulator clients |
| Validation | Local examples only |
| Runtime/evidence notes | Brightfield camera and LSM APIs run in one driver lane; the LSM simulator is constructed from the microscope sample configuration and LSM scan requests inherit the current XY stage, Z focus, objective-derived sample pixel size, lamp power, and lamp enabled state as the simulated LSM gate |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `sim-microscope` | `hub`, `simulator` | Brightfield microscope hub |
| `sim-microscope-camera` | `camera`, `simulator` | Brightfield camera over the shared sample |
| `sim-microscope-xy` | `stage.xy`, `axis.xy`, `simulator` | Shared sample-plane XY stage |
| `sim-microscope-z` | `stage.z`, `axis.z`, `simulator` | Shared focus drive |
| `sim-microscope-objective` | `objective.turret`, `state.device`, `simulator` | Objective state used by brightfield and LSM sampling |
| `sim-microscope-lamp` | `light.source`, `shutter`, `simulator` | Illumination state used as simulator laser-power input |
| `sim-lsm` | `hub`, `lsm`, `camera`, `simulator` | LSM API hub sampling the same scene state |

## Capabilities

| Capability | Device | Request | Response | Completion |
| --- | --- | --- | --- | --- |
| `CameraCapture` | Brightfield camera | `CapabilityRequest::CameraCapture` | Frame summary map | Runtime `FrameReady` plus token completion |
| `CameraStream` | Brightfield camera | `CapabilityRequest::CameraStream` | Stream summary map | Runtime frame events |
| `StageMove`, `StageHome`, `StageStop` | XY/Z stages | Stage requests or `None` | Motion/state summary | Modeled travel completion |
| `FilterSelect` | Objective turret | `CapabilityRequest::FilterSelect` | Objective and sample-pixel-size summary | Modeled turret completion |
| `ConfocalImageCapture` | LSM hub | `CapabilityRequest::ConfocalImageCapture` | Frame summary map | Runtime `FrameReady` plus token completion |
| `ConfocalImageStream` | LSM hub | `CapabilityRequest::ConfocalImageStream` | Stream summary map | Runtime `FrameReady` updates plus token completion |
| `ScanSignalStream` | LSM hub | `CapabilityRequest::ScanSignalStream` | Signal-stream summary map | Runtime `ScanSignalChunk` events plus token completion |

## Shared State Mapping

The composed driver constructs the LSM simulator from the brightfield
microscope's sample configuration, so custom seeds, focal plane, sample tilt,
and cell density are shared. Before dispatching an LSM request, it injects the
current brightfield simulator state into the LSM scan or timing map:

- `stage_x`, `stage_y`, and `stage_z`
- `pixel_size_um`
- `laser_power`
- `laser_gate_enabled`
- `magnification`
- `numerical_aperture`

The LSM simulator consumes stage position, focus, pixel size, laser power, lamp
enabled state as the simulated laser gate, magnification, and numerical
aperture. Confocal frame metadata and `ScanSignalChunk` metadata both include
the inherited scene fields so clients can confirm that snapshot, stream, and
line-scan outputs came from the same simulated microscope state. NA and
magnification tune the simulated lateral/axial PSF and detector collection gain;
this is simulator optics behavior, not hardware evidence.
The LSM hub also exposes `detector_gain` and `detector_noise` as public
simulator properties. The `lsm_composed_workflow` example writes them through
the public runtime property API, and the resulting confocal frame and
`ScanSignalChunk` metadata show the adjusted values. This is simulator behavior
only.
The `lsm_composed_workflow` example prints the inherited scene summary for
confocal capture, confocal stream, and scan-signal output, making spatial
agreement visible through public runtime events.
The `software_gui sim-composed --smoke` path writes XY, Z focus, lamp power, and lamp
enabled state through public state APIs, then selects the objective through the
public turret API before submitting LSM snapshot, live, and line-scan requests.
It prints the inherited scene and optics metadata consumed from the resulting
frame and chunk events.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- lsm_composed_workflow` | Move shared stage/focus, capture brightfield, then run confocal capture, image stream, and signal stream from the same composed simulator |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware backend | Optional NI-DAQmx execution still requires SDK/header evidence and hardware validation |
