# Laser-Scanning Microscope Simulation

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::sim_lsm` |
| Families | Confocal laser-scanning microscope simulation |
| Support level | Biological-model-oriented confocal capture, image stream, and scan-signal simulation over the shared seeded specimen model |
| Protocol evidence | Internal simulation model, not hardware protocol evidence |
| Transport | In-memory runtime resources |
| Discovery | Constructed directly by examples or simulator clients |
| Validation | Local examples only |
| Runtime/evidence notes | Emits simulated `FrameReady` data for `ConfocalImageCapture` and `ConfocalImageStream`, including typed scan/reconstruction/timing metadata; emits simulated scan-signal chunks as first-class `ScanSignalChunk` events |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `sim-lsm` | `hub`, `lsm`, `camera`, `simulator` | Owns one in-memory specimen resource and exposes confocal capture, image stream, and signal stream capabilities |
| `sim-lsm-sample` | resource | Procedural adherent cell culture shared with the brightfield simulator model |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `sim-lsm-sample` | `simulated.specimen` | Records the seeded specimen model used by confocal sampling |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `ConfocalImageCapture` | Hub | `CapabilityRequest::ConfocalImageCapture` with scan and reconstruction maps | Frame summary map with stream, frame, width, height, pixel format, and sample pixel size | Runtime `FrameReady` event plus token completion | Not sequenceable yet |
| `ConfocalImageStream` | Hub | `CapabilityRequest::ConfocalImageStream` with scan and reconstruction maps | Stream summary map with first/latest frame IDs, frame count, dimensions, pixel format, update policy, and overwrite flag | Multiple runtime `FrameReady` events, `OperationChanged` progress, plus token completion | Not sequenceable yet |
| `ScanSignalStream` | Hub | `CapabilityRequest::ScanSignalStream` with timing, channel list, and chunk size | Stream summary map with chunk count, channel names, channel count, line/sample geometry, and sample rate | Runtime `ScanSignalChunk` events, `OperationChanged` progress, plus token completion | Not sequenceable yet |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `model` | Hub | `String` | none | R | none | No | Simulation identity |
| `sample_seed` | Hub | `I64` | none | R | none | No | Shared specimen model seed |
| `detector_gain` | Hub | `Ratio` | percent | R/W | 0-500% | No | Scales simulated fluorescence signal before photon/readout conversion |
| `detector_noise` | Hub | `Ratio` | percent | R/W | 0-500% | No | Multiplies the simulated detector's total noise, shot noise included, leaving the mean signal unchanged; 100% is plain photon statistics |

## Image Model

The driver samples the shared procedural cell-culture model with confocal-like
fluorescence detectors. The current helper models separate cytoplasm, nuclei,
and background contributions; named synthetic detector responses; Gaussian
XY/Z response; a pinhole-style axial rejection term for confocal contrast;
deterministic Poisson photon sampling for low expected photon counts with a
normal approximation for high-count shot noise; read noise; laser power; and
saturated pixels.
The `detector_gain` and `detector_noise` properties provide public simulator
controls without changing hardware evidence claims. `detector_gain` scales the
signal before photon conversion, so raising it adds photons and improves
signal-to-noise. `detector_noise` scales the deviation from the expected photon
count and the read-noise term together, so it changes noise alone: the mean
image is unaffected while the per-pixel spread grows with the setting. Both
apply to a stream already running, from the next frame or line onward.
Standard LSM requests publish little-endian `Mono16` frames; `Mono8` remains
accepted as a lower-depth reconstruction format for simple consumers.

The scan map may provide:

- `width` / `height` as `PixelCount` or integer values
- `fast_axis` / `slow_axis` labels
- `sample_rate` as a `Frequency`
- `line_dwell` as a `TimeInterval`, or legacy `line_dwell_us` as a scalar
- `pixel_size_um` as a scalar micrometre size
- `laser_power` as a ratio or scalar fraction
- `magnification` as a scalar objective magnification
- `numerical_aperture` as a `NumericalAperture` value or scalar NA
- `stage_x`, `stage_y`, `stage_z` as positions or scalar micrometre coordinates
- `detectors` as a list of detector channel labels, or legacy `detector` as a
  single label, recorded in frame metadata and used to weight the simulated
  cytoplasm/nucleus/background response
- `laser_gate_enabled` as a boolean; false sets effective simulated laser power
  to zero for image and line-signal sampling

The reconstruction map may provide:

- `image_width` / `image_height` as reconstructed `PixelCount` values
- `pixel_format` as `Mono16` or `Mono8`
- `accumulation` as the reconstruction accumulation label
- `background_subtraction` as a boolean

`image_width` and `image_height` control the published frame dimensions. When
they differ from the scan dimensions, the simulator preserves the horizontal
scan field of view and records both scan and reconstruction dimensions in frame
metadata. The returned `sample_pixel_size` summary uses the reconstructed frame
pixel size.

When `magnification` and `numerical_aperture` are present, they tune the
simulated lateral/axial PSF and collection gain. A configurable simulator
pinhole scale then applies an axial rejection curve on top of excitation PSF
weighting, so out-of-focus structures contribute less to confocal image and
line-signal output. Standalone `sim-lsm` defaults to 20x / 0.45 NA optics.

Timing resolution is deterministic: an explicit `sample_rate` controls sample
period when present; otherwise an explicit positive `line_dwell` derives
`sample_rate = samples_per_line / line_dwell`. If neither field is present, the
simulator uses the default 100 kHz sample rate and records a line dwell derived
from the scan width.

Detector labels are simulator semantics, not hardware channel evidence.
`counter0`-style labels use the default mixed fluorescence response, `ai*`
labels use an analog-style lower-gain response, nucleus/DAPI/405-style labels
prefer nuclear signal, cytoplasm/FITC/488/green-style labels prefer cytoplasm,
and background/dark labels prefer background and dark offset. Multi-detector
image reconstruction averages the requested detector responses. `ScanSignalChunk`
sample maps render each requested channel independently and then convert `ai*`
channels to voltage values while counter-style channels remain integer counts.

Captured and streamed frames record scan geometry and reconstruction metadata in
the public frame metadata map using typed values: `scan_width`, `scan_height`,
`reconstruction_width`, `reconstruction_height`, `sample_rate`, `line_dwell`,
`sample_pixel_size`, `reconstruction_pixel_size`, stage positions, `detectors`,
`fast_axis`, `slow_axis`, `laser_gate_enabled`,
`detector_gain`, `detector_noise`, `reconstruction_accumulation`,
`background_subtraction`, and `saturated_pixels`.
`ScanSignalChunk` metadata uses the same typed scene and scan fields for
line-scan output, plus chunk-specific `chunk_size`,
`samples_per_line`, `lines`, `channels`, `detectors`, `laser_gate_enabled`,
`detector_gain`, `detector_noise`,
`line`, `chunk_index`, `first_sample`, `timing_origin`, `dropped_chunks`,
`dropped_samples`, and `overflowed`. The simulator currently reports zero
dropped chunks/samples and `overflowed=false`; live DAQ backends must replace
those fields with measured transport state.

For `ConfocalImageStream` requests with `update_policy="dirty_region"`, frame
metadata reports deterministic horizontal strip regions through `dirty_x`,
`dirty_y`, `dirty_width`, and `dirty_height`. The frame payload is still a full
image; `dirty_region_basis` is set to `horizontal_strip_full_frame_payload` so
clients know the dirty rectangle is an incremental redraw hint, not a cropped
buffer. The stream example prints the latest frame metadata summary so resized
stream output records both scan and reconstruction geometry.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- lsm_confocal_capture sim-lsm` | Public `ConfocalImageCapture` over the simulated confocal frame path |
| `cargo run -p numanager-examples -- lsm_confocal_capture_mono8 sim-lsm` | Public `ConfocalImageCapture` requesting a `Mono8` reconstructed frame |
| `cargo run -p numanager-examples -- lsm_confocal_stream sim-lsm` | Public `ConfocalImageStream` over simulated reconstructed frame updates, including scan dimensions that differ from frame dimensions |
| `cargo run -p numanager-examples -- lsm_live_cancel sim-lsm` | Public continuous `ConfocalImageStream` operation with frame events and cancellation |
| `cargo run -p numanager-examples -- lsm_signal_stream sim-lsm` | Public `ScanSignalStream` over simulated line-profile `ScanSignalChunk` events |
| `cargo run -p numanager-examples -- lsm_line_dwell_timing` | Public `ScanSignalStream` timing resolution where `line_dwell` derives sample rate |
| `cargo run -p numanager-examples -- lsm_signal_cancel sim-lsm` | Public continuous `ScanSignalStream` operation with chunk events, first-chunk timing/drop/scene metadata, and cancellation |
| `cargo run --release -p numanager-examples --features gui -- software_gui [sim-lsm\|sim-composed\|imswitch]` | Focused LSM control surface with source selection for simulator and configured ImSwitch DAQmx paths, including public simulator detector gain/noise property controls, composed-simulator shared stage/focus/lamp/objective controls when the selected source exposes them, and frame/line readouts for public scan, scene, and chunk metadata |
| `cargo run -p numanager-examples --features gui -- software_gui sim-lsm --smoke` | Headless validation of the LSM GUI snapshot, live-image, and line-signal workflows over simulator runtime output, including detector gain/noise property writes plus frame and chunk metadata consumed by the GUI |
| `cargo run -p numanager-examples --features gui -- software_gui sim-composed --smoke` | Headless validation that the LSM GUI writes shared XY/Z/lamp simulator state and selects the objective through public APIs, then receives the inherited scene and optics values in LSM frame and chunk metadata |

## Remaining Work

| Area | Gap |
| --- | --- |
| Stream API | Continuous simulator stream start/stop uses public operation submit/cancel; dirty-region metadata reports horizontal strip updates while frame payloads remain full images |
| Signal API | Simulator reports zero-drop/zero-overflow state; live DAQ backends still need measured overflow/drop reporting during hardware validation |
| Composed simulator | `sim_microscope_lsm` is available when brightfield camera and LSM APIs must share stage, focus, objective, lamp, and specimen state |
| GUI integration | Source selection covers `sim-lsm`, `sim-composed`, and configured `imswitch`; simulator sources expose detector gain/noise sliders through public property writes; the composed simulator exposes shared XY/Z/lamp sliders through public state writes plus objective selection through public `FilterSelect` or property APIs; the ImSwitch source displays public backend readiness, live-execution blocker, and resolved DAQmx role channels |
| Image depth | `Mono16` and `Mono8` reconstructed frames are implemented; additional output formats are absent until a public consumer requires them |
