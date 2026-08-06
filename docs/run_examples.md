# Run Examples

Examples are user-facing runtime workflows. They intentionally avoid raw
protocol commands, scripted serial replies, parser fixtures, and driver
conformance demos. Protocol work belongs in evidence-backed driver
documentation and hardware validation notes, not in user examples.

| Area | Command |
| --- | --- |
| Camera acquisition | `cargo run -p numanager-examples -- camera_acquisition [toupcam\|toupcam-live\|platform\|gige\|usb3\|genicam]` ([output](example_outputs.md#camera-acquisition)) |
| High-throughput camera stream | `cargo run -p numanager-examples -- camera_stream [toupcam\|toupcam-live\|platform\|gige\|usb3\|genicam]` ([output](example_outputs.md#camera-stream)) |
| Timing plan | `cargo run -p numanager-examples -- timing_plan` ([output](example_outputs.md#timing-plan)) |
| LSM confocal capture API | `cargo run -p numanager-examples -- lsm_confocal_capture [imswitch\|sim-lsm\|sim-composed]` ([output](example_outputs.md#lsm-confocal-capture-api)) |
| LSM Mono8 confocal capture | `cargo run -p numanager-examples -- lsm_confocal_capture_mono8 [sim-lsm\|sim-composed]` ([output](example_outputs.md#lsm-mono8-confocal-capture)) |
| LSM confocal stream API | `cargo run -p numanager-examples -- lsm_confocal_stream [imswitch\|sim-lsm\|sim-composed]` ([output](example_outputs.md#lsm-confocal-stream-api)) |
| LSM live stream cancellation | `cargo run -p numanager-examples -- lsm_live_cancel [sim-lsm\|sim-composed\|imswitch]` ([output](example_outputs.md#lsm-live-stream-cancellation)) |
| LSM scan-signal stream API | `cargo run -p numanager-examples -- lsm_signal_stream [imswitch\|sim-lsm\|sim-composed]` ([output](example_outputs.md#lsm-scan-signal-stream-api)) |
| LSM line-dwell timing | `cargo run -p numanager-examples -- lsm_line_dwell_timing` ([output](example_outputs.md#lsm-line-dwell-timing)) |
| LSM signal stream cancellation | `cargo run -p numanager-examples -- lsm_signal_cancel [sim-lsm\|sim-composed]` ([output](example_outputs.md#lsm-signal-stream-cancellation)) |
| LSM simulator workflow audit | `scripts/audit-lsm-simulator-workflows.sh` ([output](example_outputs.md#lsm-simulator-workflow-audit)) |
| NI-DAQmx external-gates audit | `scripts/audit-ni-daqmx-external-gates.sh` ([output](example_outputs.md#ni-daqmx-external-gates-audit)) |
| NI-DAQmx target-scope audit | `scripts/audit-ni-daqmx-target-scope.sh` ([output](example_outputs.md#ni-daqmx-target-scope-audit)) |
| NI-DAQmx no-hardware helper audit | `scripts/audit-ni-daqmx-no-hardware-helpers.sh` ([output](example_outputs.md#ni-daqmx-no-hardware-helper-audit)) |
| NI-DAQmx plan-validation audit | `scripts/audit-ni-daqmx-plan-validation.sh` ([output](example_outputs.md#ni-daqmx-plan-validation-audit)) |
| NI-DAQmx live-gate audit | `scripts/audit-ni-daqmx-live-gate.sh` ([output](example_outputs.md#ni-daqmx-live-gate-audit)) |
| NI-DAQmx runtime-probe audit | `scripts/audit-ni-daqmx-runtime-probe.sh` ([output](example_outputs.md#ni-daqmx-runtime-probe-audit)) |
| NI-DAQmx example-output sync audit | `scripts/audit-ni-daqmx-example-output-sync.sh` ([output](example_outputs.md#ni-daqmx-example-output-sync-audit)) |
| LSM DAQmx plan validation | `cargo run -p numanager-examples -- lsm_daqmx_plan_validation` ([output](example_outputs.md#lsm-daqmx-plan-validation)) |
| LSM DAQmx validation note scaffold | `cargo run -p numanager-examples -- lsm_daqmx_validation_note` ([output](example_outputs.md#lsm-daqmx-validation-note-scaffold)) |
| NI-DAQmx runtime probe | `cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe` ([output](example_outputs.md#ni-daqmx-runtime-probe)) |
| Motion stage | `cargo run -p numanager-examples -- motion_stage [asi\|chuo\|corvus\|esp32\|marzhauser\|openstage\|openuc2\|pi-gcs\|prior\|standa\|sutter-mp285\|sutter-stage\|thorlabs-apt\|trinamic-tmcl\|triggerscope\|wosm\|zaber]` ([output](example_outputs.md#motion-stage)) |
| Light source | `cargo run -p numanager-examples -- light_source [coolled\|pe4000\|pe340\|agilent\|obis\|omicron\|lumencor\|lmm5\|thorlabs-dc\|dc2200\|dc3100\|dc4100\|niji\|openuc2\|wosm]` ([output](example_outputs.md#light-source)) |
| LSM composed simulator workflow | `cargo run -p numanager-examples -- lsm_composed_workflow` ([output](example_outputs.md#lsm-composed-simulator-workflow)) |
| Laser | `cargo run -p numanager-examples -- laser [cobolt\|obis\|omicron]` ([output](example_outputs.md#laser)) |
| Shutter | `cargo run -p numanager-examples -- shutter [sc10\|esp32\|ix85]` ([output](example_outputs.md#shutter)) |
| Light source with opt-in Mightex HID output | `NUMANAGER_MIGHTEX_OUTPUT=1 cargo run -p numanager-examples --features os-hid -- light_source` ([output](example_outputs.md#mightex-hid-output-bring-up)) |
| Digital IO | `cargo run -p numanager-examples -- digital_io` for the Arduino/Arduino Counter/ASI Tiger/Teensy workflow, or `cargo run -p numanager-examples -- digital_io [arduino\|arduino_counter\|esp32\|teensy_pulse\|triggerscope\|wosm\|modbus\|velleman]` for a configured source ([output](example_outputs.md#digital-io)) |
| Environment control | `cargo run -p numanager-examples -- environment_control [andor_sdk2\|andor_sdk3\|spark_cyto\|okolab]` ([output](example_outputs.md#environment-control)) |
| Plate reader | `cargo run -p numanager-examples -- plate_reader [absorbance\|fluorescence\|luminescence]` for Spark Cyto detector mode selection ([output](example_outputs.md#plate-reader)) |
| Fluidics | `cargo run -p numanager-examples -- fluidics` ([output](example_outputs.md#fluidics)) |
| Robot inventory | `cargo run -p numanager-examples -- robot_inventory [opentrons]` ([output](example_outputs.md#robot-inventory)) |
| Filters | `cargo run -p numanager-examples -- filters [starlight\|prior\|ix85\|kurios]` ([output](example_outputs.md#filters)) |
| Gel Doc EZ bring-up | `cargo run -p numanager-examples -- gel_doc [configured\|live\|initialize-firmware\|capture]`; every mode but `configured` needs `--features os-usb`, and `initialize-firmware`/`capture` drive real hardware ([output](example_outputs.md#gel-doc-ez-bring-up)) |
| USB host access | `cargo run -p numanager-examples -- usb_access [claims\|show \<vid:pid\>\|bind \<vid:pid\> --approve]`; `bind` displaces the node's current driver and needs elevation ([output](example_outputs.md#usb-host-access)) |
| Discovery flow | `cargo run -p numanager-examples -- discover_devices` ([output](example_outputs.md#discovery-flow)) |
| Discovery with HID devices | `cargo run -p numanager-examples --features os-hid -- discover_devices` ([output](example_outputs.md#discovery-with-hid-devices)) |
| Autofocus | `cargo run -p numanager-examples -- autofocus` ([output](example_outputs.md#autofocus)) |
| Biological simulation | `cargo run -p numanager-examples -- biology_simulation` ([output](example_outputs.md#biological-simulation)) |
| Runtime config round-trip | `cargo run -p numanager-examples -- config_roundtrip` ([output](example_outputs.md#runtime-config-round-trip)) |

The `imswitch` LSM examples print a configured `daqmx_plan` summary with
candidate buffer dimensions, transfer direction, AO/DO waveform intent,
plan-validation status, sample-clock/trigger routing topology, start/read/clear
order, and cleanup timeout/policy. That output is a public API planning surface
only; it does not create NI tasks, generate output samples, or validate hardware
routing.
`lsm_daqmx_plan_validation` prints both valid configured raster/signal plans
with `helper_command_runnable=true` and intentionally invalid plans where
unrecognized channels or role/channel mismatches force setup/preflight helper
commands to `null`.
`lsm_daqmx_bringup_plan` submits the public capture and signal requests against
the configured ImSwitch DAQmx descriptor and prints the SDK-feature helper build
command, package/header/FFI-source and external-gates evidence commands,
compact backend readiness and promotion-gate status summaries,
process-isolated inventory commands, non-live preflight/setup commands that
match the returned task plans, lifecycle dry-run and cleanup-log simulation
commands, invalid numeric/range/transfer/raster-consistency helper-input guard commands,
plus gated I/O smoke commands for later bench validation.
The compact `backend_readiness` line includes the configured-vs-detected
runtime-version comparison summary alongside the live-task blocker.
The FFI-source audit output is recorded in
[`example_outputs.md`](example_outputs.md#ni-daqmx-ffi-source-inventory) as a
source-boundary inventory only; it must stay separate from runtime or hardware
behavior evidence.
The package-input, SDK-header, and FFI-source inventory scripts are run
individually against explicit paths — `scripts/audit-ni-daqmx-package-inputs.sh`,
`scripts/audit-ni-daqmx-sdk-headers.sh`, and
`scripts/audit-ni-daqmx-sys-source.sh` — each of which records intake or source
identity without loading the NI runtime or claiming task behavior.
`scripts/audit-ni-daqmx-external-gates.sh` checks that license/legal review,
installed 26.5 header audit, NI-PAL/device inventory, bench safety
preconditions, runtime publication, and live task execution remain explicit
external gates rather than implied support.
`scripts/audit-ni-daqmx-target-scope.sh` checks the numanager-side Cargo and
helper-wrapper boundary for the optional NI-DAQmx backend: the `ni-daqmx-sys`
dependency stays target-scoped to Linux/Windows, helper binaries require the
SDK feature, unsupported targets use failure stubs, and wrappers do not
reference NI-DAQmx FFI directly. It does not prove Windows ABI compatibility or
any DAQmx runtime/task behavior.
`scripts/audit-ni-daqmx-no-hardware-helpers.sh` builds the SDK-feature helper
binaries and runs only dry-run, preflight-only, simulated-cleanup, and
invalid-input paths. It checks for markers such as `execute=false`,
`created_task=false`, `preflight_only=true`, `wrote_output=false`, and
`read_input=false`, so it validates the no-hardware helper boundary without
creating NI tasks or touching I/O.
`scripts/audit-ni-daqmx-plan-validation.sh` runs the public
`lsm_daqmx_plan_validation` example and checks that valid raster/signal plans
keep setup/preflight helper commands available, while invalid role/channel
plans null those commands and keep `execution_gate: not_live_task_execution`.
`scripts/audit-ni-daqmx-live-gate.sh` sets
`NUMANAGER_DAQMX_LIVE_TASK_EXECUTION=1` and verifies that the public configured
ImSwitch capture, stream, signal, and GUI smoke paths record live-task intent
while still reporting `live_task_execution_ready=false` and
`execution=not_live_task_execution`.
`scripts/audit-ni-daqmx-runtime-probe.sh` checks the public DAQmx runtime-probe
readiness boundary: config-only metadata paths avoid vendor-runtime loading,
and process-isolated runtime-version probing keeps the runtime process in
`runtime_probe_only` with `live_task_execution_ready=false` even when NI-PAL
initialization fails inside the helper process. It also checks the compact
`inventory:` summary for requested inventory state, helper isolation, detected
device count, configured-device detection, and contained helper errors.
When package/header metadata, process-isolated runtime probing, and
`NUMANAGER_DAQMX_LIVE_TASK_EXECUTION=1` are all present, the same audit verifies
that the blocker advances only to `pending_hardware_validation`; it still does
not create NI-DAQmx tasks or expose live scans.
`scripts/audit-ni-daqmx-example-output-sync.sh` runs the public DAQmx bring-up
plan and validation-note scaffold examples and checks that the recorded example
docs still contain the emitted audit commands and required scaffold sections.
The plan-setup guard commands cover non-finite timeout input, oversized sample
counts, and raster dimension/sample-count mismatches before any DAQmx calls.
The runtime-probe, inventory, lifecycle, invalid
numeric/range/raster-consistency guard, channel-setup, and I/O smoke commands
are generated by shared example code used by both `lsm_daqmx_bringup_plan` and
`lsm_daqmx_validation_note`, keeping the bench command list and note scaffold
aligned.
Bench hosts can override the configured DAQmx device and role channels used by
the ImSwitch examples with `NUMANAGER_DAQMX_DEVICE_NAME`,
`NUMANAGER_DAQMX_LSM_X_GALVO`, `NUMANAGER_DAQMX_LSM_Y_GALVO`,
`NUMANAGER_DAQMX_LSM_LASER_GATE`, `NUMANAGER_DAQMX_LSM_DETECTOR`,
`NUMANAGER_DAQMX_LSM_SAMPLE_CLOCK`,
`NUMANAGER_DAQMX_LSM_SAMPLE_CLOCK_SOURCE`,
`NUMANAGER_DAQMX_LSM_START_TRIGGER_SOURCE`, `NUMANAGER_DAQMX_SIGNAL_AI`, and
`NUMANAGER_DAQMX_SIGNAL_CHANNELS` as a comma-separated signal channel list.
`NUMANAGER_DAQMX_TIMEOUT_SECONDS` overrides the configured DAQmx timeout used in
cleanup plans and generated helper command `--timeout` arguments.
`NUMANAGER_DAQMX_HELPER_TIMEOUT_SECONDS` overrides the process-isolated DAQmx
helper timeout used by runtime probe and validation-note examples; generated
bring-up and validation command lists include the same environment prefix on
supervised helper probe commands when the override is set. When probe metadata
environment variables such as `NIDAQMX_RUNTIME_PLATFORM`,
`NIDAQMX_RUNTIME_LICENSE`, or `NIDAQMX_HEADER_SHA256` are set, those generated
runtime-probe commands also include shell-safe environment prefixes so saved
bench notes remain reproducible.
`NUMANAGER_DAQMX_LIVE_TASK_EXECUTION=1` records bench-run intent in readiness
metadata and configured LSM result maps, but it does not bypass the
support-evidence boundary.
`lsm_daqmx_bringup_plan` prints baseline config-only and direct runtime probe
commands before helper build/inventory commands, then prints backend readiness,
promotion-gate status, non-live public LSM task plans, and role-matched helper
commands.
`lsm_daqmx_validation_note` prints a markdown bench-note scaffold from the same
public task-plan data, including run-identity fields, evidence-source and
setup/safety placeholders, required-artifact placeholders, expected preflight
task/order/route/waveform and transfer rows, package/header/FFI-source audit
commands, physical-channel mapping rows,
output/input validation rows, LSM task-execution gate rows, and a command-output
log table, with evidence rows left as `Unknown`. Its invalid numeric/range/transfer/raster-consistency helper-input guard commands
use representative AO/CI/CO channels from the resolved public DAQmx task plan,
so custom bench channel maps are preserved in the generated note. The note also
prefixes its generated
`lsm_daqmx_bringup_plan` command with the currently set LSM mapping, route,
signal-channel, timeout, helper-timeout, and live-task-intent variables.
`software_gui imswitch --smoke` prints the same plan-aware result summaries that
the interactive GUI displays after snapshot, live, and line requests.
Simulator LSM signal examples print a first-chunk sample preview; analog-style
`ai*` detector labels are reported as voltage values, while counter-style labels
remain integer counts.
`scripts/audit-lsm-simulator-workflows.sh` runs the non-hardware LSM simulator
smoke set through public runtime examples, including capture, resized `Mono8`
capture, live-image streaming, raw signal chunks, line-dwell timing,
cancellation, composed simulator state sharing, and GUI smoke output. It checks
for public runtime markers only and is not NI-DAQmx or hardware evidence.
The DAQmx non-hardware audits are run individually rather than through an
aggregate wrapper: `scripts/audit-ni-daqmx-external-gates.sh`,
`scripts/audit-ni-daqmx-target-scope.sh`,
`scripts/audit-ni-daqmx-no-hardware-helpers.sh`,
`scripts/audit-ni-daqmx-plan-validation.sh`,
`scripts/audit-ni-daqmx-live-gate.sh`,
`scripts/audit-ni-daqmx-runtime-probe.sh`, and
`scripts/audit-ni-daqmx-example-output-sync.sh`. They are plan-implementation
boundary checks only; live NI-DAQmx task execution still requires the bench
checklist evidence. The helper and runtime-probe audits need a Linux or Windows
target.

The `daqmx_runtime_probe` example loads the user-installed NI-DAQmx runtime only
when built with `--features ni-daqmx-sdk`. It includes the audited local
`/usr/include/NIDAQmx.h` digest by default. Bench hosts can override
`NUMANAGER_DAQMX_DEVICE_NAME`, `NIDAQMX_RUNTIME_PACKAGE`,
`NUMANAGER_DAQMX_RUNTIME_VERSION`, `NIDAQMX_RUNTIME_PLATFORM`,
`NIDAQMX_RUNTIME_LICENSE`, `NIDAQMX_HEADER_PATH`, and
`NIDAQMX_HEADER_SHA256` so the probe metadata matches the package/header
evidence being recorded. `backend_status` reports configured/detected runtime
version comparison metadata when both sides are available. Set
`NUMANAGER_DAQMX_LIVE_TASK_EXECUTION=1` only for bench runs that intentionally
request the future live-task path; `backend_status` still reports
`live_task_execution_ready=false` without recorded bench evidence.
Set `NUMANAGER_DAQMX_CONFIG_ONLY=1` to print those effective metadata fields and
the no-runtime `backend_status` with `connect=false`, without loading the vendor
runtime. On Linux,
`NUMANAGER_DAQMX_RUNTIME_HELPER=target/debug/numanager-daqmx-inventory-helper`
uses the helper's version-only mode so runtime probe failures stay outside the
runtime process. `NUMANAGER_DAQMX_HELPER_TIMEOUT_SECONDS` adjusts the helper
process timeout for bench hosts where NI-PAL initialization or inventory is
slow. The full probe reports readiness metadata and does not create tasks, write
outputs, read inputs, or execute LSM scans.
The standalone `numanager-daqmx-task-lifecycle-helper --dry-run`,
`numanager-daqmx-channel-setup-helper --dry-run`, and
`numanager-daqmx-io-smoke-helper` dry-run paths print planned calls without
creating DAQmx tasks. The I/O helper requires `--execute` before it performs a
single-channel write/read/pulse operation.
DAQmx readiness output separates the requested Cargo feature, target support,
and compiled SDK backend as `feature_requested`, `target_supported`, and
`feature_enabled`; unsupported OS targets remain configured-only.

Hardware-specific public workflows are kept only where the device has topology
or acquisition behavior not covered by a generic workflow:

| Area | Command |
| --- | --- |
| Squid controller graph | `cargo run -p numanager-examples -- squid` ([output](example_outputs.md#squid-controller-graph)) |
| Spark Cyto plate-reader graph | `cargo run -p numanager-examples -- spark_cyto` ([output](example_outputs.md#spark-cyto-plate-reader-graph)) |

## Start GUI

The Slint software-test GUI is gated behind a default-off `gui` feature:

The GUI is a client of whatever devices the runtime holds. It selects them by
kind tag, capability kind, and graph dependency role, never by driver name, so
another instrument can be substituted at one point: `load_drivers` in
`software_gui.rs`, chosen by a positional argument (`software_gui [source]`,
default `sim-microscope`). It exercises camera source selection, single capture,
open-ended streaming, live frame display with optional saturation marking, a
continuous intensity histogram, focus nudging, and mouse panning.

The default source is
[`numanager_drivers::sim_microscope`](devices/sim-microscope.md): one composed
brightfield microscope whose camera, XY stage, Z stage, three-position objective
turret, and lamp share a single procedurally generated cell-culture model.
Because that device publishes its sensor pixel pitch, binning, and objective
magnification, the GUI derives micrometres per image pixel
(`pixel_pitch * binning / magnification`), moves the stage one image pixel per
dragged screen pixel, and draws a scale bar. It finds the objective through the
camera's `objective` dependency role in the device graph. When a device
publishes none of that — a real camera with no advertised pixel pitch, say — the
GUI falls back to the stage's advertised step size and then to a fixed step, and
says in the readout that the scale is not calibrated.

Streaming submits a `CameraStreamRequest` with `frame_count: None`; the same
button then stops it by cancelling the operation. While it runs, the frame is
outlined and a live indicator is shown, and single capture is disabled. The
button is disabled entirely for a camera that does not advertise `CameraStream`.

Every property editor is chosen from the `PropertySchema` rather than from the
key name: `ValueType::Bool` gets a checkbox, a schema with `enum_values` gets a
drop-down, and a numeric schema with a `range` gets a slider next to a box
holding the exact value — logarithmic when the range spans two decades or more
(exposure, stage speed), linear otherwise (gain, stage coordinates). Rows are
labelled with the schema's display name while writes still address the raw key,
and a control that produces continuous values rounds onto the schema's
`increment` before writing, because the runtime validates writes against it.

Render cost is a few milliseconds per frame in a release build and roughly
twenty times that in a debug build, so run the window with `--release`:

```sh
cargo run --release -p numanager-examples --features gui -- software_gui
```

For terminal validation without opening a window:

```sh
cargo run -p numanager-examples --features gui -- software_gui --smoke
```

Recorded smoke output is in [`example_outputs.md`](example_outputs.md#software-test-gui).

The LSM GUI focuses on snapshot capture, live reconstructed image updates, and
line-signal scans. It consumes runtime `FrameReady`, `ScanSignalChunk`, and
`OperationChanged` progress events, and its scan controls submit typed sample
rate, line-dwell, detector-channel, laser-gate, chunk-size, and overwrite values
through public requests. Detector gain/noise controls write the public simulator
properties when the selected source exposes them. Shared stage/focus/lamp
controls write public state and objective controls use the public turret API
when the composed brightfield+LSM simulator is selected, and the resulting LSM
frame/chunk metadata shows the inherited scene and optics.
The interactive frame and line readouts include the same public scan, scene,
and first-chunk metadata summaries that the smoke path records.
Its source selector can switch between the standalone simulator, the composed
brightfield+LSM simulator, and the configured ImSwitch DAQmx descriptor path.
The ImSwitch source displays public backend readiness and configured DAQmx
role-channel mapping, while leaving live task execution gated:

```sh
cargo run --release -p numanager-examples --features gui -- software_gui [sim-lsm|sim-composed|imswitch]
```

For terminal validation without opening a window:

```sh
cargo run -p numanager-examples --features gui -- software_gui [sim-lsm|sim-composed|imswitch] --smoke
```

Recorded smoke output is in [`example_outputs.md`](example_outputs.md#lsm-gui-smoke).
