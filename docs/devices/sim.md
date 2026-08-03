# Integrated Simulation

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::sim` |
| Families | Composed autofocus biological-scene simulation |
| Support level | Biological-model-oriented autofocus simulation |
| Protocol evidence | Internal simulation model, not hardware protocol evidence |
| Transport | In-memory runtime resources |
| Discovery | Simulated two-stage discovery |
| Validation | Local examples only |
| Runtime/evidence notes | None currently |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `sim-af-camera` | `camera`, `simulator` | Camera observing shared biological focus-plane model |
| `sim-af-z` | `axis.z`, `simulator` | Z stage coupled to focus score |
| `sim-af-light` | `light.source`, `shutter`, `simulator` | Illumination coupled to focus operation |
| `sim-composed-autofocus` | `autofocus`, `service`, `simulator` | Autofocus service depending on camera, Z, and light |
| `sim-composed-autofocus-scene` | resource | In-memory biological focus-plane scene |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `StageMove` | AF Z | `CapabilityRequest::StageMove` | Moved-axis map plus focus-score update | Runtime token completion | Z position sequences coupled to focus model |
| `CameraCapture` | AF camera | `CapabilityRequest::CameraCapture` | `CapturedFrame`-parseable runtime frame handle plus focus-scene metadata | Runtime completion plus `FrameReady` frame-store insertion | Capture participant |
| `Autofocus` | Composed autofocus | `CapabilityRequest::Autofocus` | Provider-neutral autofocus state map | Runtime token completion | Camera/Z/light/autofocus property sequences |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `z` | AF Z | `Position` | um | R/W | configured travel | Yes | In-memory focus-plane model |
| `exposure` | AF camera | `TimeInterval` | s | R/W | 0.1 ms..10 s | Yes | Simulated camera exposure |
| `enabled` | AF light | `Bool` | none | R/W | none | Yes | Simulated light state |
| `power` | AF light | `Ratio` | percent | R/W | simulation range | Yes | Simulated light intensity |
| `enabled` | Autofocus | `Bool` | none | R/W | none | Yes | Service state |
| `mode` | Autofocus | `String` | none | R/W | `single_shot`, `continuous`, `hold`, `stop` | Yes | Service mode |
| `status` | Autofocus | `String` | none | R | service status | No | Service state |
| `focus_score` | Autofocus | `F64` | none | R | model-derived | No | Biological focus-plane model |

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- autofocus` | Provider-neutral autofocus selection plus composed camera/Z/light/autofocus timing over a shared biological focus-plane model |
| `cargo run -p numanager-examples -- biology_simulation` | Whole-system biological focus-plane workflow: camera frame capture through runtime frame handles, off-focus Z motion, autofocus lock, light/exposure coupling, and timing-plan transitions |

## Remaining Work

| Area | Gap |
| --- | --- |
| Simulation policy | Keep simulation work coupled to biological system models; use hardware-driver simulation only for generic device workflows |
| Biological models | Add coupled brightfield, fluorescence, bleaching, drift, noise, and acquisition timing models when simulation resumes |
| Validation | Define numeric expectations for focus score, blur, photobleaching, and frame timing |
