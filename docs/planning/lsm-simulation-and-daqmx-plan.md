# LSM Simulation And NI-DAQmx Plan

This plan tracks long-term work for the ImSwitch-derived laser-scanning
microscopy APIs:

- `ConfocalImageCapture`
- `ConfocalImageStream`
- `ScanSignalStream`

The immediate examples and `lsm_gui` exercise these public APIs, but the current
ImSwitch DAQmx crate returns configured API summaries only. The long-term goal is
to move from GUI-local preview graphics to runtime-produced simulated data, then
to a validated optional NI-DAQmx backend.

## 1. Extract Shared Specimen Model

Create a shared simulation module, initially in `numanager-drivers`:

```text
crates/numanager-drivers/src/sim_sample.rs
```

The module should own specimen/world concepts only:

- deterministic seeded sample generation
- tile-based infinite XY cell generation
- cell geometry, nuclei, density, and fluorescence traits
- sample plane and tilt model
- coordinates in micrometers
- local specimen-intensity queries

It should not depend on cameras, DAQmx, GUI code, frame handles, or runtime
operations.

## 2. Refactor `sim_microscope`

Move the specimen generation pieces from `sim_microscope` into `sim_sample`.

`sim_microscope` should keep:

- camera device
- XY/Z stage
- objective turret
- lamp
- brightfield optics/exposure model
- runtime frame publication

The first refactor should preserve current `software_gui` behavior.

## 3. Add Fluorescence And Confocal Sampling

Add modality-specific confocal logic, either in `sim_sample` or a separate
module such as:

```text
crates/numanager-drivers/src/sim_lsm_model.rs
```

Model:

- fluorophore channels for nuclei, cytoplasm, and background
- excitation channel / laser gate
- emission intensity
- Gaussian or Airy-like PSF approximation
- pinhole/confocal contrast approximation
- Poisson photon noise
- analog detector noise
- saturation and clipping
- dark offset and background

Keep the distinction clear:

- `sim_sample`: what exists in the specimen
- `sim_lsm_model`: what the scanner and detector measure

## 4. Add A `sim_lsm` Driver

Create a simulator driver that exposes the LSM APIs as runtime capabilities:

- `ConfocalImageCapture`
- `ConfocalImageStream`
- `ScanSignalStream`

The driver should publish runtime data rather than local GUI drawings:

- `ConfocalImageCapture` returns a reconstructed frame handle
- `ConfocalImageStream` emits live frame updates
- `ScanSignalStream` emits first-class raw sample chunk events

## 5. Define LSM Runtime Output Shape

Before adding too much simulator detail, define the runtime output contract.

For `ConfocalImageCapture`, record:

- final `FrameReady`
- `FrameHandle`
- width, height, and pixel format
- metadata for scan geometry, sample rate, channels, dwell, and reconstruction

For `ConfocalImageStream`, record:

- stream ID
- repeated frame events with dirty-region metadata
- overwrite / mutable-frame semantics
- progress and status events

For `ScanSignalStream`, use the first-class `ScanSignalChunk` event. Chunk
metadata should include:

- channel names
- sample rate
- chunk size
- first sample index
- timestamps or timing origin

## 6. Refactor `lsm_gui` To Consume Runtime Data

The GUI should remain a public runtime client:

- buttons submit public capability requests
- preview updates come from runtime frame/stream events
- line plots update from `ScanSignalStream` chunks
- local decorative image generation is removed, except possibly for an empty
  placeholder

The GUI can keep specialized controls:

- scan width and height
- dwell/sample rate
- line/raster mode
- detector channel toggles
- laser gate
- overwrite policy
- snapshot, live image, and line-scan commands

## 7. Share One Specimen Across Camera And LSM

Support a composed simulator where brightfield camera and LSM scan the same
virtual sample and stage state.

Start with a combined simulator driver:

```text
sim_microscope_lsm
```

It should own:

- the shared specimen
- camera
- LSM hub
- XY/Z stages
- objective/turret state
- transmitted light / laser state

This is simpler than coordinating independent simulator drivers through a shared
scene service. A scene service can be revisited later if multiple independent
drivers need to share sample state.

## 8. Integrate Stage, Focus, And Optics

The LSM simulation should respond to:

- XY stage position
- Z focus offset
- objective magnification and numerical aperture
- scan field size
- sample tilt
- detector gain/noise
- laser power/gate

Snapshot, live image, line scan, and brightfield camera output should agree
spatially when they refer to the same simulated field of view.

## 9. Document Simulator Workflows

Document simulator behavior as simulator behavior, not hardware evidence.

Update:

- `docs/devices/sim-microscope.md`
- a new `docs/devices/sim-lsm.md` or a combined simulator page
- `docs/run_examples.md`
- `docs/example_outputs.md`

Respect the repository evidence policy: do not add self-confirming hardware
driver tests or protocol fixtures for hardware behavior.

## 10. Add Optional NI-DAQmx Backend

After the simulator validates the runtime contract and GUI workflow, add the real
NI-DAQmx path behind an optional backend.

Inputs needed from the user:

- NI-DAQmx SDK headers
- SDK/runtime package name and version
- platform and installation layout
- redistribution/license boundary
- relevant examples or API reference material if available

Current SDK/runtime intake notes:

- Linux headers are installed at `/usr/include/NIDAQmx.h`; the local runtime
  reports NI-DAQmx 26.3.1 through the major/minor/update runtime-version
  getters and provides `libnidaqmx.so.26.3.1`.
  The Linux header audit records the header digest, 2003-2026 copyright banner,
  runtime-version property IDs/getter symbols, and no literal package-version
  macro, and now explicitly reports `NIDAQmx.h count = 1` plus the audited
  `/usr/include/NIDAQmx.h` path, so installed package-version claims are
  intentionally tied to `daqmx_runtime_probe` and package-intake evidence
  instead of inferred from the header alone. The header audit exits non-zero if
  `NIDAQmx.h` is absent from the supplied file/directory.
- The local `ni-daqmx-sys` fork at
  `/home/mahogny/github/claude/ni-daqmx-sys` has been regenerated with the
  bindgen scripts from the Linux header and compiles/tests on Linux. The scripts
  prefer an installed bindgen CLI and fall back to the fork-local Cargo
  generator when the CLI is not installed. The source
  audit records package metadata, bindgen dependency/version, generated-source
  hashes, platform link cfgs, required symbols, and the fork-local runtime smoke
  test boundary. The same audit now prints a platform-boundary verdict. The
  current local dirty fork worktree has the required DAQmx symbols,
  Windows/Linux link paths, Linux-specific non-Windows cfg, explicit macOS
  unsupported guard, explicit rejection for other non-Linux/non-Windows targets,
  and 32-bit non-Windows rejection; commit the fork patch and update the recorded
  revision before treating that exact source state as pinned.
- numanager can load the Linux NI-DAQmx runtime through the optional
  `ni-daqmx-sdk` feature and report the runtime version. Linux device inventory
  is disabled in-process because `DAQmxGetSysDevNames` can
  abort when NI-PAL is not initialized. The runtime can delegate Linux inventory
  to a configured process-isolated `numanager-daqmx-inventory-helper` path and
  report helper failure as `device_inventory_error`. A standalone SDK-feature
  helper build command is recorded for bench runs, and the
  `numanager-daqmx-task-lifecycle-helper` binary also exists for empty-task
  create/clear bring-up, but on the current Linux host it aborts in
  `libnipalu.so` because NI-PAL is not initialized. If a lifecycle call fails
  after a task has started, the helper now attempts stop-before-clear cleanup and
  prints `cleanup_after_lifecycle_error` / `stopped_task_after_error` rows for
  bench logs. A dry-run-only lifecycle cleanup-log simulation command emits the
  same row names without DAQmx task calls so bench-log capture can be checked. A standalone
  `numanager-daqmx-inventory-helper --version-only` mode exists so Linux
  runtime-version probing can be delegated to a child process without also
  requesting device inventory; helper aborts are reported in `backend_status`
  instead of killing the runtime process. The helper process timeout is exposed
  as typed `inventory_helper_timeout` metadata and can be overridden by bench
  examples with `NUMANAGER_DAQMX_HELPER_TIMEOUT_SECONDS`; generated bring-up and
  validation-note command lists carry that override into supervised runtime
  helper probe commands. A standalone
  `numanager-daqmx-channel-setup-helper` binary exists for AO/DO/AI/CI/CO
  channel creation/clear bring-up without starting tasks, writing outputs, or
  reading inputs. A standalone `numanager-daqmx-io-smoke-helper` binary exists
  for dry-run single-channel I/O call plans and explicitly gated AO/DO writes,
  AI/CI reads, and finite CO pulse smoke checks with
  `--execute --bench-safety-reviewed`. Its dry-run
  output records intended safe final states for AO, DO, and CO; AO execute paths
  require a range containing 0 V and write 0 V before clear, DO execute paths
  write low before clear, and CO execute paths report the configured idle state
  expected after stop. The runtime
  helper CLIs reject non-finite or non-positive timing, non-finite or reversed
  channel-setup analog bounds, setpoint, and I/O smoke non-finite or reversed
  analog bounds, non-finite or non-positive frequency, non-finite or out-of-range
  duty-cycle, sample-count, sample-range, non-positive timeout, bare
  `--execute` without `--bench-safety-reviewed`, and explicitly empty or
  whitespace-padded route-source inputs before printing dry-run plans or
  calling NI-DAQmx. AO I/O smoke helpers also reject analog ranges that
  exclude 0 V so the bench-only execute path can always issue its final
  safe-state write.
  The I/O smoke helper also rejects cleanup-simulation requests for output-only
  AO/DO kinds, and the lifecycle helper rejects cleanup-simulation mode unless
  both the dry-run and started-task preconditions are present. The plan-setup
  helper also rejects missing channel lists, non-finite analog bounds, reversed
  analog ranges, non-positive or oversized sample counts, overflowing per-task
  transfer element counts, empty physical channel or task-label inputs,
  leading/trailing whitespace in helper identifiers, duplicate physical channels,
  duplicate active task labels, incomplete raster dimensions,
  non-positive raster dimensions, raster dimension overflow,
  raster frame-product overflow, and raster dimension products that do not match
  `--samples`. The single-channel channel-setup and I/O smoke helpers
  also reject single-channel helper empty channel inputs before printing dry-run
  output or calling NI-DAQmx. Lifecycle, channel-setup, and I/O smoke helpers
  reject empty explicit task names (`--name ''`) and keep `--unnamed` as the
  supported null task-name path.
  The runtime
  driver still does not expose live task execution until a safe
  isolation/readiness strategy is validated.
- Latest installer inputs found in
  `/home/mahogny/github/claude/reveng-dll/nidaq/` are
  `NILinux2026Q3DeviceDrivers.zip` and `ni-daqmx_26.5_online.exe`; file
  identities and archive contents are recorded in
  `docs/devices/ni-daqmx-package-intake.md` using
  `scripts/audit-ni-daqmx-package-inputs.sh`; that script now records Debian/RPM
  package metadata when local tooling is available, embedded Linux license-file
  identities from Debian/RPM payloads, and Windows online-installer PE/payload
  inventory when `7z` is available. Legal review, installed Windows
  package/license review, and installed Windows header audit remain open.
- Linux or Windows 26.5 support should use the same evidence path before
  publishing a 26.5 binding update: audit the installed target-platform
  `NIDAQmx.h`, record the exact header path and bindgen regeneration command,
  regenerate the local fork with that platform's bindgen script, and re-run the
  FFI source audit from that regenerated source state. Do not infer Windows ABI
  support from Linux-generated bindings, and do not infer installed header
  version solely from the 26.5 installer input.
- numanager now treats the optional NI-DAQmx SDK backend as a Linux/Windows
  target only. The `ni-daqmx-sys` dependency is target-scoped to Linux and
  Windows, helper binaries compile Linux/Windows implementations behind small
  platform wrappers, and unsupported OSes keep the configured descriptor/API
  planning path. Readiness metadata now separates `feature_requested`,
  `target_supported`, and `feature_enabled`, with
  `target_platform_linux_or_windows` reported as the blocker on unsupported
  targets.
  macOS support would require NI-provided SDK/runtime evidence and a separate
  target-platform binding audit before linking any vendor library. Downstream
  target-scoping protects numanager builds, while the fork's own build-script
  platform cfgs now reject macOS and other unsupported non-Linux/non-Windows
  targets in the local dirty worktree.
- The ImSwitch LSM examples accept environment overrides for the DAQmx device
  name, LSM role channels, route sources, signal channel list, and DAQmx timeout
  so `lsm_daqmx_bringup_plan` can generate helper commands for a bench-specific
  channel map and timeout policy without editing code.
- The generated DAQmx validation note derives representative invalid numeric
  helper-input guard commands from the resolved AO/CI/CO task channels, so
  custom bench channel maps are preserved in the command list and output-log
  table instead of falling back to fixed `Dev1/...` defaults.
- Generated DAQmx bring-up and validation-note runtime-probe commands include
  shell-safe prefixes for currently set probe metadata environment variables,
  including package/runtime/header identity and helper supervision settings, so
  saved bench notes are reproducible without relying on hidden shell state.
- The DAQmx bring-up plan prints baseline config-only and direct runtime probe
  commands before helper build/inventory commands so the checklist's readiness
  sequence is visible from the public bring-up example.
- The DAQmx bring-up plan now prints invalid numeric, range, transfer-overflow,
  and raster-consistency helper-input guard commands immediately after lifecycle
  dry-run commands, before real task or channel setup, so bench runs can record
  that helper CLIs reject `NaN`/`inf`, oversized sample counts, transfer
  element overflow, and inconsistent raster dimensions before any DAQmx calls.
- The runtime-probe, inventory, lifecycle, invalid numeric guard,
  channel-setup, and I/O smoke command generators are shared between
  `lsm_daqmx_bringup_plan` and `lsm_daqmx_validation_note`, reducing drift
  between the public bench command sequence and generated bench-note scaffold.
- The DAQmx plan-setup helper now prints compact first/middle/final AO/DO raster
  `waveform_preview` rows in preflight output, derived from the configured
  dimensions and voltage bounds. These rows make scan and laser-gate intent
  inspectable in bench logs before any DAQmx task calls and remain
  `pending_hardware_validation`.
- The same preflight output now prints a `raster_timing_preview` row with pixel,
  line, frame, and total planned durations derived from sample rate and raster
  dimensions, so bench logs have a timing basis for the waveform preview before
  any DAQmx task calls.
- Signal plan preflight commands now include `--signal-lines` and `--chunk-size`
  metadata and print a `signal_timing_preview` row with sample, line, chunk, and
  total durations, so signal-stream timing intent is visible in no-hardware
  bench logs before any reads are enabled.
- DAQmx plan preflight output now also prints per-task `planned_timing` rows
  for finite sample-clock consumers and implicit finite counter-output pulse
  generation, making the task-level timing configuration auditable before task
  setup, writes, or reads.
- Raster DAQmx plans now derive a candidate `/Device/CtrNInternalOutput`
  sample-clock source from the configured counter-output sample-clock channel
  when no explicit source is configured, pass that route to helper commands, and
  expose the source origin in preflight output while keeping route acceptance
  pending hardware validation.
- DAQmx plan preflight output now prints `planned_runtime_sequence` and
  `planned_completion` rows for the intended finite acquisition lifecycle,
  covering buffered output writes before start, start/read/wait/stop/clear
  ordering, completion timeout, and the pending hardware-validation boundary.
- Configured DAQmx task-plan maps now also expose the same lifecycle intent as
  structured `runtime_sequence` and `completion_plan` metadata, so public API
  results and GUI summaries can display the finite setup/write/start/read/wait/
  stop/clear plan without parsing helper output or enabling live task
  execution.
- Configured DAQmx task-plan maps now also expose structured `execution_contract` metadata for the future live executor, including
  buffered-before-start writes, `auto_start=false`, finite expected-sample
  reads, candidate layout, wait order, timeout, and
  `contract_evidence_status=pending_hardware_validation`.
- Configured DAQmx task-plan maps now also expose
  `live_task_execution_blocker` and structured
  `live_task_execution_readiness`, matching the public backend readiness blocker
  and missing-evidence list for the current feature/target/package/header/
  runtime/live-intent state. Backend status and task-plan readiness maps also
  expose `external_promotion_gates`, listing legal review, installed package and
  header audits, NI-PAL/device inventory, bench safety preconditions, task
  behavior validation, runtime publication validation, and hardware-note gates
  as structured API data. This keeps GUI summaries, example output, and
  bench-note current plans from having to infer the live-execution gate from
  surrounding result fields.
- Backend status now also exposes `external_promotion_gate_statuses`, a
  machine-readable pending status map with required-evidence text and
  `support_claim=not_validated` for every external promotion gate. The same map
  is mirrored into `daqmx_task_plan.live_task_execution_readiness`, allowing
  validation notes and clients to check backend/task-plan agreement on the
  full gate-status structure.
- Backend and task-plan readiness now also mirror
  configured-vs-detected runtime-version comparison fields. If a configured
  runtime version is present, live task execution remains blocked unless the
  detected runtime version compares as a confirmed match; explicit mismatches
  report `runtime_version_mismatch`, and partial or unknown detection reports
  `runtime_version_unverified`.
- The ImSwitch DAQmx GUI source summary now surfaces the compact
  `promotion_gate_statuses=[pending=9]` count from backend metadata alongside
  the live-execution blocker and role-channel summary, so snapshot and line-scan
  operators see the remaining promotion gate state in the GUI path.
- The generated DAQmx validation-note scaffold now expands
  `external_promotion_gates` into per-gate evidence rows, making the legal,
  installed-header, NI-PAL/device-inventory, task-behavior,
  runtime-publication, and hardware-note gates auditable from the note before
  live task execution is promoted.
- Configured DAQmx task-plan maps now expose structured `cancel_plan` metadata
  for the future public cancel path, including request-stop strategy, reverse
  stop order, clear order, timeout, safe-output uncertainty, and
  `cancel_evidence_status=pending_hardware_validation`.
- Configured DAQmx `cleanup_plan` metadata now records the expected
  failure-cleanup modes for partial setup, post-start, buffered-write, finite
  read, and counter-output wait-timeout failures, plus the intended
  stop-started-tasks-before-clear strategy and pending safe-output validation
  boundary.
- Configured DAQmx task-plan maps now include structured `publication_plan`
  metadata for the intended public runtime output: `FrameReady` frame
  dimensions, reconstruction dimensions, pixel format, and required metadata for
  raster capture/streaming, and `ScanSignalChunk` channel/chunk/timing metadata
  for signal streams. These remain publication intent only and are marked
  `pending_hardware_validation` until live DAQmx execution can produce
  hardware-backed events.
- Configured raster DAQmx task-plan maps now include structured
  `reconstruction_plan` metadata for the future sample-to-pixel reconstruction
  path, including input tasks, scan/reconstruction dimensions, pixel format,
  row-major one-sample-per-pixel mapping, accumulation, background-subtraction,
  saturation, and publish-after-reconstruction intent. This remains
  `pending_hardware_validation`.
- DAQmx plan preflight output now also prints `planned_reconstruction` rows for
  raster sample-to-pixel reconstruction intent derived from the helper's
  configured scan shape, so bench logs capture reconstruction intent before live
  task execution or frame publication is enabled.
- DAQmx plan preflight output now also prints `planned_publication` rows for
  raster `FrameReady` and signal `ScanSignalChunk` intent derived from the
  helper's configured scan/signal shape, so bench logs capture the intended
  runtime-output contract before live task execution is enabled.
- `planned_publication` and `daqmx_task_plan.publication_plan` now use the same
  public metadata vocabulary for future hardware-backed events, including
  raster `frame_handle` / `stream` fields and signal `line_index`,
  `chunk_index`, `first_sample_index`, `sample_count`, and `sample_values`
  fields.
- The generated validation note now prints each
  `publication_plan.required_metadata` list in the preflight publication
  targets, so bench notes compare the exact public `FrameReady` /
  `ScanSignalChunk` metadata vocabulary before live task execution.
- DAQmx plan preflight output now also prints `planned_cleanup` rows for
  expected failure-cleanup modes, stop-before-clear strategy, stop/clear order,
  timeout, and safe-output-state evidence status before live task execution is
  enabled.
- DAQmx plan preflight output now also prints `planned_execution_contract`
  rows for raster and signal plans, mirroring the public
  `daqmx_task_plan.execution_contract` write/read/wait, timeout, layout, and
  publish-after-validated-read contract before any DAQmx task calls.
- DAQmx plan preflight output now also prints `planned_live_executor` rows
  with the future SDK task-wrapper backend, disabled executor status, phase
  order, DAQmx API surface, and required validation gates before any task setup
  or live execution is enabled.
- Configured DAQmx task-plan maps now also expose `live_executor_plan`
  metadata for the future SDK task-wrapper backend, including the disabled
  executor status, Linux/Windows target scope, readiness gate, phase order,
  DAQmx API surface, required validation gates, and pending hardware-validation
  evidence status.
- `docs/example_outputs.md` now records a current
  `scripts/audit-ni-daqmx-sys-source.sh` excerpt for the local `ni-daqmx-sys`
  fork, including worktree status, bindgen inputs, platform-boundary verdict,
  and required generated symbols, while keeping it explicitly scoped to
  FFI-source evidence rather than runtime or hardware behavior.
- The package-input audit now prints standalone Windows payload
  license/EULA/copyright file identities when present instead of silently
  dropping that positive case; the current 26.5 online-installer payload still
  reports no standalone license, EULA, or copyright files at the inspected
  first-level payload.
- `scripts/audit-ni-daqmx-evidence-inputs.sh` now runs package-input,
  installed-header, and FFI-source inventory scripts over configured local
  paths and checks stable markers without loading the NI runtime or making
  task, I/O, scan, redistribution, or hardware claims.
- `scripts/audit-ni-daqmx-external-gates.sh` now checks that legal review,
  installed Windows package/license review, installed Linux/Windows 26.5 header
  audit, NI-PAL/device inventory, bench safety preconditions, runtime
  publication, and live task execution remain explicit external gates rather
  than implied support.
- The SDK-header audit now exits non-zero if `NIDAQmx.h` is absent and records
  the discovered `NIDAQmx.h` count/path when present, making the future
  Linux/Windows 26.5 installed-header gate explicit before any regenerated
  binding source is accepted. The evidence template and generated validation
  note now also require the installed target-platform `NIDAQmx.h` used for
  bindgen and the bindgen regeneration command, so the later FFI-source audit is
  tied to the same target-platform header instead of only to a package archive.
- Generated DAQmx bench scaffolds now include required-artifact rows for the
  audited `NIDAQmx.h` count and path, so saved notes capture the installed
  header gate rather than only a combined header inventory digest.
- `scripts/audit-lsm-simulator-workflows.sh` now runs the non-hardware LSM
  simulator smoke set through public runtime examples and checks capture,
  resized `Mono8` reconstruction, image streaming, signal chunks, line-dwell
  timing, cancellation, composed simulator state sharing, and GUI smoke markers
  without making NI-DAQmx or hardware claims. The signal-stream checks include
  public `ScanSignalChunk` timing/drop/overflow metadata, and the GUI smoke
  check verifies the same first-chunk metadata after GUI consumption, so the
  runtime output contract stays visible in the gate.
- `scripts/audit-ni-daqmx-target-scope.sh` now checks the numanager-side
  optional SDK boundary: `ni-daqmx-sys` is Linux/Windows target-scoped, helper
  binaries require `ni-daqmx-sdk`, helper wrappers provide unsupported-target
  failure stubs, wrappers do not reference NI-DAQmx FFI directly, and readiness
  metadata exposes the Linux/Windows target-support blocker without making ABI,
  runtime, task, or hardware claims.
- `scripts/audit-ni-daqmx-no-hardware-helpers.sh` now builds the SDK-feature
  helper binaries and exercises only dry-run, preflight-only,
  simulated-cleanup, and invalid-input guard paths, checking no-hardware markers
  such as `execute=false`, `created_task=false`, `preflight_only=true`,
  `wrote_output=false`, and `read_input=false`. It now also rehearses
  plan-setup partial-failure cleanup with
  `--preflight-only --simulate-setup-error-after 1`, recording
  `cleared_partial_task` and `cleanup_after_setup_error` rows without DAQmx task
  calls.
- `scripts/audit-ni-daqmx-plan-validation.sh` now runs the public
  `lsm_daqmx_plan_validation` example and verifies that valid configured
  raster/signal plans keep helper commands runnable, invalid role/channel plans
  suppress setup/preflight helper commands, and the execution gate remains
  non-live.
- `scripts/audit-ni-daqmx-live-gate.sh` now sets
  `NUMANAGER_DAQMX_LIVE_TASK_EXECUTION=1` and verifies public configured
  ImSwitch capture, stream, signal, and GUI smoke paths record live-task intent
  while remaining `live_task_execution_ready=false`,
  `execution=not_live_task_execution`, and frame/chunk-free. The GUI smoke path
  also checks that simulator-only scene, objective, and detector control
  writeback markers are absent for the configured ImSwitch source.
- `scripts/audit-ni-daqmx-runtime-probe.sh` now checks the public runtime-probe
  readiness boundary: config-only package/header metadata avoids vendor-runtime
  loading, configured package-version metadata parses without probing the
  runtime, and process-isolated runtime-version probing remains
  `runtime_probe_only` with `live_task_execution_ready=false`, including when a
  helper reports a contained NI-PAL initialization failure. Configured runtime
  versions that cannot be confirmed against the detected runtime now remain
  blocked as `runtime_version_unverified` or `runtime_version_mismatch`. The
  audit also checks the positive no-hardware readiness path: when package/header
  metadata, a process-isolated runtime probe, and live-task intent are all
  present with no configured runtime-version mismatch, the blocker advances only
  to `pending_hardware_validation` and live execution still remains disabled.
  The
  public
  `daqmx_runtime_probe` output now also prints compact `promotion_gates` summary
  lines derived from `external_promotion_gates` plus
  `promotion_gate_statuses` count summaries from the structured status map, so
  non-code live-execution gates are visible without parsing the debug
  `backend_status` map. It also prints a compact `inventory` summary for
  requested inventory state, helper isolation, detected-device count,
  configured-device identity, and contained inventory/configured-device errors;
  the audit now checks explicit process-isolated inventory probing remains
  evidence-only and keeps live task execution disabled.
- `scripts/audit-ni-daqmx-example-output-sync.sh` now runs the public DAQmx
  bring-up plan and validation-note scaffold examples and checks that recorded
  example output contains the emitted audit commands and required scaffold
  sections, including the installed target-platform `NIDAQmx.h` used for
  bindgen, bindgen regeneration command, and same-header FFI-source audit
  markers. The bring-up plan now starts its command scaffold with compact
  `backend_readiness`, configured-vs-detected runtime-version comparison, and
  `promotion_gate_statuses=[pending=9]` output from the public backend-status
  property, so bench logs capture the current live-task gate state before helper
  commands.
- `scripts/audit-lsm-daqmx-plan-nonhardware.sh` now aggregates the simulator
  workflow audit, repository reverse-evidence boundary audit, and DAQmx
  non-hardware evidence-input, external-gates, target-scope, helper,
  plan-validation, live-gate, runtime-probe, and docs-sync audits. It is the
  current plan-level implementation gate for non-hardware work and still leaves
  live NI-DAQmx task execution behind bench validation.
- The DAQmx validation note also prefixes its generated `lsm_daqmx_bringup_plan`
  command with the current LSM mapping, route, signal-channel, timeout,
  helper-timeout, and live-task-intent variables so the public task-plan command
  can be rerun from the note.
- The DAQmx bench checklist and generated validation-note scaffold now require
  the `lsm_daqmx_bringup_plan` `backend_readiness` line, including
  `promotion_gate_statuses=[pending=9]`, as an explicit artifact/evidence row
  before helper build, inventory, setup, or I/O smoke output is treated as
  bench evidence.
- The DAQmx validation note's command list now follows the bench checklist's
  runnable safe sequence: public bring-up plan, SDK-feature helper build,
  isolated helper probes, plan preflight, dry-run guards, empty/channel setup,
  full plan setup, and gated I/O smoke checks. Its evidence rows distinguish
  dry-run lifecycle/channel checks from DAQmx task/channel setup evidence.
- The DAQmx plan-validation example now prints valid configured raster/signal
  baselines with runnable helper-command fields and intentionally invalid
  raster/signal requests with explicit validation status, recognized task count,
  unrecognized channel count, invalid role count, and null helper-command
  fields.
- The generated DAQmx validation note now includes run-identity and
  required-artifact tables derived from the public task plans and current
  environment, so saved bench notes capture configured device, role-channel,
  route, signal-channel, timeout, host, and package/runtime placeholders before
  command output is pasted in.
- The generated DAQmx validation note now also includes hardware-template
  evidence-source and setup/safety sections, plus firmware/software and
  transport identity rows, so later bench notes have explicit places for
  source-class coverage, safety limits, safe shutdown, and fault-recovery
  evidence.
- The generated validation note now also mirrors the bench checklist's physical
  channel mapping and output/input validation tables, prefilled only with
  resolved public plan channels and leaving inventory, runtime-output, and
  hardware-readback cells blank until a real bench run supplies evidence.
- The generated validation note now also mirrors the bench checklist's LSM task
  execution gate, leaving finite task order, routing, buffered write/read,
  runtime publication, cancel, and failure-cleanup rows `Unknown` until hardware
  evidence exists.
- The runtime-publication validation rows are split by public LSM API:
  `ConfocalImageCapture` final `FrameReady`, `ConfocalImageStream` repeated
  `FrameReady` updates with dirty-region/progress metadata, and
  `ScanSignalStream` `ScanSignalChunk` output with channel/timing/sample/drop/
  overflow/progress metadata. This keeps the later hardware note from using one
  raster or signal log as evidence for all three APIs.
- The generated validation note now also expands its preflight evidence targets
  from public task-plan metadata, including task timing, runtime sequence,
  completion, publication, cancel, and cleanup intent rows. This keeps the
  generated bench note aligned with the static checklist before
  hardware-derived `FrameReady` or `ScanSignalChunk` events are enabled.
- The generated validation note now includes a task-plan live-readiness evidence
  checklist row, so bench notes must explicitly record the per-plan blocker,
  missing evidence, backend-status agreement, and pending hardware-validation
  state before live task execution can be promoted.
- The same generated note now reads public `backend_status` and prints a
  `Backend Readiness` table with live-execution blocker, missing evidence,
  external promotion gates, and task-plan readiness agreement for capture and
  signal plans, including the configured-vs-detected runtime-version comparison
  fields, so the checklist row has current scaffold data instead of only a
  placeholder.
- The generated note now also prints a `Backend Inventory` table derived from
  public `backend_status`, including requested inventory state, helper
  isolation, helper timeout, detected-device count/list, configured-device
  detection/identity, and contained helper/configured-device errors. The static
  checklist mirrors this as a required backend-inventory readiness row before
  live task execution can be promoted.
- The generated note and static bench checklist now require the
  `## Setup And Safety` table as a bench-safety precondition artifact before
  any helper command containing `--execute` is run. This keeps live I/O smoke
  checks gated on recorded wiring, load, safe output state, interlock,
  emergency-stop, cleanup, and fault-recovery evidence.
- Generated I/O smoke execute commands now include
  `--execute --bench-safety-reviewed`, and the helper rejects bare `--execute`
  before task creation. The no-hardware helper audit records this guard as an
  invalid-input case rather than as DAQmx or hardware evidence.
- Backend and task-plan readiness now include `bench_safety_preconditions` as
  a structured external promotion gate, so the public
  `promotion_gate_statuses=[pending=9]` summaries include safety preconditions
  rather than leaving them as checklist-only prose.

Implementation work:

- record SDK package identity and license boundary
- audit headers/API docs for task creation, channel setup, timing, triggers,
  reads, writes, wait/completion, stop/clear, and error handling
- add an optional NI-DAQmx backend behind a feature flag; the first increment
  is a runtime-version probe only
- map scan requests to AO/DO/AI/CI/CO tasks
- validate task ordering and trigger routing
- implement safe stop and cleanup paths
- record hardware validation notes from a real device
  using `docs/devices/ni-daqmx-bench-validation-checklist.md` before exposing
  live task execution

The real backend should conform to the API/output contract proven first by the
simulator.

## Milestones

1. Extract `sim_sample` with no behavior change.
2. Refactor `sim_microscope` onto `sim_sample`.
3. Add fluorescence/confocal sampling helpers.
4. Add `sim_lsm` with `ConfocalImageCapture`.
5. Add synthetic `ScanSignalStream` chunk output.
6. Refactor `lsm_gui` to consume runtime output.
7. Add combined camera+LSM simulator over one specimen.
8. Validate GUI workflows against simulator output.
9. Document simulator workflows and recorded example outputs.
10. Use provided NI-DAQmx SDK headers to implement and validate the optional
    NI-DAQmx backend.

## Current Implementation Status

| Milestone | Status | Evidence |
| --- | --- | --- |
| 1. Shared specimen model | Done | `crates/numanager-drivers/src/sim_sample.rs` owns seeded specimen geometry and sampling helpers |
| 2. `sim_microscope` refactor | Done | `sim_microscope` imports the shared sample model and preserves brightfield runtime publication |
| 3. Confocal sampling | Done | `crates/numanager-drivers/src/sim_lsm_model.rs` renders confocal raster frames and line profiles from the shared specimen, including separate cytoplasm/nucleus/background contributions, named synthetic detector responses, Gaussian excitation PSF, pinhole-style axial rejection for confocal contrast, deterministic low-count Poisson photon sampling, high-count shot-noise approximation, read noise, saturation, clipping, and public `detector_gain` / `detector_noise` simulator properties |
| 4. `sim_lsm` driver | Done for the three public LSM APIs | `crates/numanager-drivers/src/sim_lsm.rs` exposes `ConfocalImageCapture`, `ConfocalImageStream`, and `ScanSignalStream` |
| 5. Runtime output shape | Done for current core events | Capture/stream use `FrameReady` with simulated `Mono16` confocal frames by default and optional `Mono8` reconstruction, honor requested reconstruction dimensions for frame payloads while preserving scan dimensions and reconstructed pixel size in metadata, include typed scan/reconstruction/timing metadata, detector gain/noise metadata, horizontal-strip dirty-region metadata over full-frame payloads, and stream progress through `OperationChanged`; signal completion maps include channel names and counts; signal chunks use first-class `ScanSignalChunk` events with stream id, channels, timing origin, line/chunk/sample indices, chunk size, sample rate, sample period, per-channel simulated sample data, metadata, and operation progress; chunk metadata also repeats channels, line, chunk index, first sample, timing origin, typed scene/scan fields including detectors, laser-gate state, detector gain/noise, and simulated `dropped_chunks`, `dropped_samples`, and `overflowed` fields for clients that consume metadata maps; simulator `ScanSignalStream` supports a continuous mode via `lines <= 0` and public runtime cancellation, with the public cancellation example reporting first-chunk timing/drop/scene metadata before cancellation |
| 6. `lsm_gui` runtime consumption | Done for simulator and configured DAQmx descriptor sources | `lsm_gui` submits public runtime requests with typed width, height, sample-rate, line-dwell, detector, laser-gate, chunk-size, and overwrite controls; has source selection for `sim-lsm`, `sim-composed`, and configured `imswitch`; writes public simulator `detector_gain` and `detector_noise` properties when the selected source exposes them; writes composed-simulator XY/Z/lamp state through public `StateSet` APIs and selects the objective through public `FilterSelect` or property APIs when those devices are present; displays public source metadata including DAQmx backend readiness, live-execution blocker, and resolved role channels; uses public submit/cancel for continuous simulator live streams; converts runtime `Mono8`/`Mono16` frames, `ScanSignalChunk` events, and `OperationChanged` progress into preview images, line plots, and progress readouts; interactive frame and line summaries include public scan, scene, dirty-region, and first-chunk metadata; and its smoke paths record detector gain/noise write/readback, composed shared-scene/objective write/readback, latest frame metadata summaries, and first chunk timing/channel metadata consumed for snapshot/live/line views |
| 7. Shared camera+LSM simulator | Done for one composed driver | `crates/numanager-drivers/src/sim_microscope_lsm.rs` exposes brightfield camera, XY/Z stages, objective, lamp, and LSM APIs in one driver lane, with the LSM simulator constructed from the microscope sample configuration |
| 8. Stage/focus/optics integration | Done for simulator path | Composed LSM requests inherit stage position, focus, sample pixel size, lamp power, lamp enabled state as the simulated laser gate, magnification, and numerical aperture; confocal capture frame metadata, confocal stream frame metadata, and `ScanSignalChunk` metadata all expose the inherited scene values plus detector gain/noise values; standalone and composed LSM requests honor typed sample-rate, line-dwell-derived timing, laser-gate, and detector-list fields, while public `detector_gain` and `detector_noise` hub properties control simulator gain/noise; `lsm_gui sim-composed --smoke` now writes shared XY/Z/lamp state, selects the 60x/0.90 NA objective through public APIs, and verifies those values in LSM frame/chunk scene metadata; NA/magnification tune the LSM PSF and collection gain; `lsm_gui --smoke` and `scripts/audit-lsm-simulator-workflows.sh` validate snapshot, live-image, line-signal, cancellation, timing, and composed-state workflows against simulator runtime output without opening a window |
| 9. Simulator workflow docs | Done for current simulator workflows | `sim-microscope`, `sim-lsm`, `sim-microscope-lsm`, run examples, recorded outputs, and `scripts/audit-lsm-simulator-workflows.sh` cover current simulator paths, including `Mono8` resized capture, resized confocal stream dirty-region output, cancellation with first-chunk timing/drop/scene metadata, line-dwell timing, composed camera+LSM state sharing, `lsm_gui sim-lsm --smoke`, and `lsm_gui sim-composed --smoke`; NI-DAQmx hardware workflow docs remain tied to backend validation |
| 10. Optional NI-DAQmx backend | No-hardware backend scaffold implemented; task execution awaiting hardware validation | Runtime probing with configured-vs-detected version comparison blockers, Linux/Windows target scoping, package/header/FFI audits, configured task planning with derived counter-output sample-clock routes, structured runtime-sequence/completion/execution-contract/live-executor/reconstruction/publication/cancel plan metadata, per-task timing, finite preflight runtime sequence/completion/live-executor/reconstruction/publication rows, raster/signal timing, and AO/DO waveform preview rows, helper binaries, dry-run/preflight/simulated-cleanup paths including plan-setup partial-failure cleanup rehearsal, I/O smoke safe-final-state planning, invalid numeric/range/transfer/raster/signal guards, bring-up examples, validation-note scaffold, task-plan live-readiness metadata, structured external-promotion-gate metadata including bench safety preconditions, live-gate metadata, GUI readiness display, split runtime-publication evidence rows for capture, live-image streaming, and signal streaming, and bench checklist are implemented. Live task execution and LSM scans remain configured/API summaries until legal review, installed Windows package/header review, installed 26.5 header audits, confirmed runtime-version match when a version is configured, NI-PAL/device inventory, bench safety preconditions, task ordering, routing, reconstruction, completion, cleanup, per-API runtime publication, and hardware validation notes are recorded. |

Current non-hardware plan gate:

- `scripts/audit-lsm-daqmx-plan-nonhardware.sh` is the aggregate status check
  for milestones 1-10 before real DAQmx task execution. It runs the LSM
  simulator workflow audit, repository reverse-evidence boundary audit,
  NI-DAQmx evidence-input audit, external-gates audit, target-scope audit,
  no-hardware helper audit, plan-validation audit, live-gate audit,
  runtime-probe audit, and DAQmx example-output sync audit.
- Passing this aggregate audit means the simulator/API/documentation and
  no-hardware DAQmx gates are aligned. It does not complete legal review,
  installed Windows package/header review, NI-PAL readiness, device inventory,
  bench safety approval, live task execution, or hardware validation.

Milestone 10 also includes the public live-gate audit in
`scripts/audit-ni-daqmx-live-gate.sh`; the validation checklist and generated
bench-note scaffold require that audit before any live NI-DAQmx task execution
is promoted.
It also includes the public runtime-probe audit in
`scripts/audit-ni-daqmx-runtime-probe.sh`, which verifies config-only metadata
and process-isolated runtime-version probing without making task, I/O, scan, or
hardware claims.
`scripts/audit-ni-daqmx-example-output-sync.sh` keeps the DAQmx bring-up and
validation-note recorded outputs aligned with their current public example
commands.
