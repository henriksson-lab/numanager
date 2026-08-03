# NI-DAQmx Bench Validation Checklist

Use this checklist before promoting `numanager-imswitch-daqmx` beyond configured
task planning and runtime probing. It specializes
[`hardware-validation-template.md`](hardware-validation-template.md) for the
ImSwitch-style NI-DAQmx backend.

This document is not a validation result. Fill one copy per hardware setup and
link the completed note from `imswitch-daqmx.md`, `evidence.md`, and
`lsm-simulation-and-daqmx-plan.md`.

## Run Identity

| Field | Value |
| --- | --- |
| Driver crate | `numanager-imswitch-daqmx` |
| Device page | `docs/devices/imswitch-daqmx.md` |
| NI device model |  |
| NI device name | `Dev1` or configured device name |
| Serial number or asset tag |  |
| Firmware/software version |  |
| Transport | NI-DAQmx vendor runtime / PCIe, PXI, USB, Ethernet, or cDAQ chassis |
| NI-DAQmx runtime version |  |
| NI-DAQmx package / installer |  |
| Host OS and driver stack |  |
| Date | YYYY-MM-DD |
| Operator |  |
| Config file or discovery record |  |
| `lsm_x_galvo` / `lsm_y_galvo` |  |
| `lsm_laser_gate` |  |
| `lsm_detector` |  |
| `lsm_sample_clock` |  |
| `lsm_sample_clock_source` |  |
| `lsm_start_trigger_source` |  |
| `daqmx_timeout` |  |

## Evidence Sources

| Source class | Reference | Covered behavior |
| --- | --- | --- |
| Audited SDK/header | Header inventory output | Available NI-DAQmx symbols and header identity only |
| Audited FFI source | FFI source inventory output | Generated binding source, platform cfgs, and symbol availability only |
| Audited target scope | Target-scope audit output | numanager Cargo feature, target cfg, helper-wrapper, and readiness boundary only |
| Vendor package/runtime | Package input inventory and runtime probe outputs | Package identity and loaded runtime version only |
| Bench run | Command output log, inventory output, electrical readback, and runtime API output | Physical channel mapping, task behavior, I/O behavior, cleanup, and runtime publication |

## Setup And Safety

| Area | Observed or enforced behavior |
| --- | --- |
| Motion limits and homing state |  |
| Laser/light output limits and interlocks |  |
| Voltage/current/load limits |  |
| Emergency stop or safe shutdown |  |
| DAQmx safe output state after stop/clear |  |
| Fault injection or recovery tested |  |

## Required Artifacts

| Artifact | Path or value |
| --- | --- |
| External-gates audit command | `scripts/audit-ni-daqmx-external-gates.sh` |
| External-gates audit output |  |
| Package input inventory command | `scripts/audit-ni-daqmx-package-inputs.sh <installer-file-or-directory>` |
| Package input inventory output |  |
| SDK header path or archive |  |
| Header inventory command | `scripts/audit-ni-daqmx-sdk-headers.sh <header-file-or-directory>` |
| Header inventory SHA-256 |  |
| Header inventory `NIDAQmx.h` count |  |
| Header inventory `NIDAQmx.h` path |  |
| Installed target-platform `NIDAQmx.h` used for bindgen |  |
| Bindgen regeneration command |  |
| FFI source inventory command | `scripts/audit-ni-daqmx-sys-source.sh <ni-daqmx-sys-repo>` |
| FFI source inventory output |  |
| Target-scope audit command | `scripts/audit-ni-daqmx-target-scope.sh` |
| Target-scope audit output |  |
| No-hardware helper audit command | `scripts/audit-ni-daqmx-no-hardware-helpers.sh` |
| No-hardware helper audit output |  |
| Plan-validation audit command | `scripts/audit-ni-daqmx-plan-validation.sh` |
| Plan-validation audit output |  |
| Live-gate audit command | `scripts/audit-ni-daqmx-live-gate.sh` |
| Live-gate audit output |  |
| Runtime-probe audit command | `scripts/audit-ni-daqmx-runtime-probe.sh` |
| Runtime-probe audit output |  |
| Example-output sync audit command | `scripts/audit-ni-daqmx-example-output-sync.sh` |
| Example-output sync audit output |  |
| LSM bring-up plan output |  |
| LSM bring-up `backend_readiness` line | `backend_readiness: ... runtime_version=... promotion_gate_statuses=[pending=9]` |
| Backend inventory readiness table | `## Backend Inventory` |
| Bench safety preconditions table | `## Setup And Safety` |
| Helper build output |  |
| Runtime probe output |  |
| Inventory helper output |  |
| Task lifecycle helper output |  |
| Channel setup helper output |  |
| Plan setup helper output |  |
| Electrical readback or loopback log |  |
| Runtime API output for promoted operation |  |

## Safe Bring-Up Sequence

Run these in order. Do not continue to output-writing or input-reading work until
the earlier rows pass and the setup safety constraints are recorded.
`cargo run -p numanager-examples -- lsm_daqmx_validation_note` generates a
matching command-output log table for recording exit status and stdout/stderr
artifact paths from the bench host.
The current local FFI-source inventory excerpt is recorded in
[`example_outputs.md`](../example_outputs.md#ni-daqmx-ffi-source-inventory) as
source-boundary evidence only. A dirty fork worktree must be committed and the
recorded revision updated before treating that exact source state as pinned.

| Step | Command or action | Expected evidence | Result |
| --- | --- | --- | --- |
| External-gates audit | `scripts/audit-ni-daqmx-external-gates.sh` | Confirms license/legal review, installed package/header review, NI-PAL/device inventory, bench safety preconditions, runtime publication, and live task execution remain explicit external gates; no runtime loading, task calls, I/O, legal conclusion, safety approval, or hardware claims | Pass/Fail/Unknown |
| Package input inventory | `scripts/audit-ni-daqmx-package-inputs.sh <installer-file-or-directory>` | Installer/package file identity, byte counts, file types, and archive entries where applicable; no header/runtime/task claims | Pass/Fail/Unknown |
| Header inventory | `scripts/audit-ni-daqmx-sdk-headers.sh <header-file-or-directory>` | Non-zero exit if `NIDAQmx.h` is absent; otherwise `NIDAQmx.h` count/path, header identity, digest, title/copyright banner, required symbols, runtime-version property/getter symbols, and literal package-version macro status; no runtime/task claims | Pass/Fail/Unknown |
| FFI source inventory | `scripts/audit-ni-daqmx-sys-source.sh <ni-daqmx-sys-repo>` | Fork revision, worktree state, bindgen inputs, generated-source hashes, platform link cfgs, required symbols including runtime-version bindings, and source-evidence boundary; no runtime/task claims | Pass/Fail/Unknown |
| Target-scope audit | `scripts/audit-ni-daqmx-target-scope.sh` | numanager Cargo dependency target scope, SDK-feature helper gating, helper unsupported-target stubs, and wrapper/implementation FFI boundary; no ABI/runtime/task claims | Pass/Fail/Unknown |
| No-hardware helper audit | `scripts/audit-ni-daqmx-no-hardware-helpers.sh` | SDK-feature helper build plus dry-run, preflight-only, simulated-cleanup, and invalid-input guard markers; no task creation, output writes, input reads, or hardware claims | Pass/Fail/Unknown |
| Plan-validation audit | `scripts/audit-ni-daqmx-plan-validation.sh` | Public `lsm_daqmx_plan_validation` output keeps valid raster/signal helper commands runnable, suppresses helpers for invalid plans, and keeps the non-live execution gate | Pass/Fail/Unknown |
| Live-gate audit | `scripts/audit-ni-daqmx-live-gate.sh` | Public configured ImSwitch capture, stream, signal, and GUI smoke paths record `live_task_execution_requested=true` but keep `live_task_execution_ready=false`, `execution=not_live_task_execution`, and no frames/chunks | Pass/Fail/Unknown |
| Task-plan live readiness | Public `daqmx_task_plan.live_task_execution_readiness` and `backend_status.missing` fields | The per-plan readiness map and backend status agree on `live_task_execution_ready=false`, the current blocker, missing package/header/runtime/feature evidence, configured-vs-detected runtime-version comparison, and `hardware_validation_status=pending` before live execution is enabled | Pass/Fail/Unknown |
| Runtime-probe audit | `scripts/audit-ni-daqmx-runtime-probe.sh` | Public config-only metadata path avoids runtime loading; process-isolated runtime-version probe remains `runtime_probe_only`, keeps `live_task_execution_ready=false`, contains helper runtime failures, blocks live execution when configured runtime-version metadata cannot be confirmed, and advances to `pending_hardware_validation` only when package/header metadata, runtime probing, and live-task intent are present | Pass/Fail/Unknown |
| Example-output sync audit | `scripts/audit-ni-daqmx-example-output-sync.sh` | Public DAQmx bring-up plan and validation-note scaffold emit the documented audit commands, and recorded example output contains the required scaffold sections | Pass/Fail/Unknown |
| Runtime probe config-only | `NUMANAGER_DAQMX_CONFIG_ONLY=1 cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe` with bench metadata overrides set as needed | Effective `probe_config`, `connected: Bool(false)`, no-runtime `backend_status` with `connect_requested=false`, metadata/header/package readiness, live-task request state, and no vendor-runtime loading | Pass/Fail/Unknown |
| Runtime probe | `cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe` with `NUMANAGER_DAQMX_DEVICE_NAME`, `NIDAQMX_RUNTIME_PACKAGE`, `NUMANAGER_DAQMX_RUNTIME_VERSION`, `NIDAQMX_RUNTIME_PLATFORM`, `NIDAQMX_RUNTIME_LICENSE`, `NIDAQMX_HEADER_PATH`, and `NIDAQMX_HEADER_SHA256` set when the bench host differs from the defaults | Effective `probe_config`, `connected: Bool(true)`, detected runtime version, configured/detected runtime-version comparison when configured metadata is supplied, `runtime_version_unverified` or `runtime_version_mismatch` blocker for unconfirmed configured versions, no task creation | Pass/Fail/Unknown |
| Backend inventory readiness | Generated validation-note `## Backend Inventory` table and public `daqmx_runtime_probe` `inventory:` summary | Requested inventory state, process-isolated helper state, helper timeout, detected-device count/list, configured-device detection/identity, and contained helper/configured-device errors before live execution is enabled | Pass/Fail/Unknown |
| LSM bring-up plan | `cargo run -p numanager-examples -- lsm_daqmx_bringup_plan` with `NUMANAGER_DAQMX_DEVICE_NAME`, `NIDAQMX_RUNTIME_PACKAGE`, `NUMANAGER_DAQMX_RUNTIME_VERSION`, `NIDAQMX_RUNTIME_PLATFORM`, `NIDAQMX_RUNTIME_LICENSE`, `NIDAQMX_HEADER_PATH`, `NIDAQMX_HEADER_SHA256`, `NUMANAGER_DAQMX_LSM_X_GALVO`, `NUMANAGER_DAQMX_LSM_Y_GALVO`, `NUMANAGER_DAQMX_LSM_LASER_GATE`, `NUMANAGER_DAQMX_LSM_DETECTOR`, `NUMANAGER_DAQMX_LSM_SAMPLE_CLOCK`, `NUMANAGER_DAQMX_LSM_SAMPLE_CLOCK_SOURCE`, `NUMANAGER_DAQMX_LSM_START_TRIGGER_SOURCE`, `NUMANAGER_DAQMX_SIGNAL_AI`, `NUMANAGER_DAQMX_SIGNAL_CHANNELS`, `NUMANAGER_DAQMX_TIMEOUT_SECONDS`, and `NUMANAGER_DAQMX_HELPER_TIMEOUT_SECONDS` set as needed for the bench mapping and timeout policy | `backend_readiness` line with live-task blocker, runtime-version comparison, and `promotion_gate_statuses`, followed by non-live capture/signal task plans plus role-matched helper commands for the configured device, channels, routes, DAQmx timeout, and helper timeout | Pass/Fail/Unknown |
| Bench safety preconditions | Completed generated validation-note `## Setup And Safety` table plus reviewed wiring, load, safe output state, interlocks, emergency stop, and cleanup constraints | Safety rows are filled before any helper command containing `--execute` is run | Pass/Fail/Unknown |
| Helper build | `cargo build -p numanager-imswitch-daqmx --features ni-daqmx-sdk --bins` | Feature-gated DAQmx inventory, task-lifecycle, channel-setup, plan-setup, and I/O smoke helper binaries built before any `target/debug/numanager-daqmx-*` command is run | Pass/Fail/Unknown |
| Isolated Linux runtime probe | `NUMANAGER_DAQMX_RUNTIME_HELPER=target/debug/numanager-daqmx-inventory-helper cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe` | Runtime process survives helper abort/non-zero exit, reports detected runtime version or contained helper error, no device inventory, no task creation | Pass/Fail/Unknown |
| Device inventory | `NUMANAGER_DAQMX_INVENTORY=1 NUMANAGER_DAQMX_INVENTORY_HELPER=<helper> cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe` | Device list, configured device identity, or contained helper error | Pass/Fail/Unknown |
| Raster plan preflight | `target/debug/numanager-daqmx-plan-setup-helper ... --preflight-only` from `lsm_daqmx_bringup_plan` | Flushed planned AO/DO/CI/CO tasks, runtime task labels, setup/start/read/stop/clear order, route producer/consumer rows, per-task timing rows, raster timing preview rows, task-labeled AO/DO waveform intent, compact first/middle/final AO/DO waveform preview rows, `planned_execution_contract` raster intent, `planned_live_executor` phase intent, `planned_reconstruction` sample-to-pixel intent, `planned_publication` `FrameReady` intent, Preflight `planned_cleanup` rows for failure modes and stop/clear order, cleanup policy, and task-labeled transfers; no DAQmx task calls | Pass/Fail/Unknown |
| Signal plan preflight | `target/debug/numanager-daqmx-plan-setup-helper ... --preflight-only` from `lsm_daqmx_bringup_plan` | Flushed planned CI/AI tasks, runtime task labels, setup/start/read/stop/clear order, route producer/consumer rows, per-task timing rows, signal timing preview rows, `planned_execution_contract` signal intent, `planned_live_executor` phase intent, `planned_publication` `ScanSignalChunk` intent, Preflight `planned_cleanup` rows for failure modes and stop/clear order, cleanup policy, and task-labeled transfers; no DAQmx task calls | Pass/Fail/Unknown |
| Task lifecycle dry run | `target/debug/numanager-daqmx-task-lifecycle-helper --dry-run` and `target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --start --wait-seconds 0.250000` | Planned lifecycle calls, optional wait call, and explicit `created_task=false`; no DAQmx task calls | Pass/Fail/Unknown |
| Task lifecycle cleanup-log simulation | `target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --start --wait-seconds 0.250000 --simulate-error-after-start` from `lsm_daqmx_bringup_plan` | Zero-exit simulated `cleanup_after_lifecycle_error` and `stopped_task_after_error=simulated_no_task` rows; no DAQmx task calls | Pass/Fail/Unknown |
| Plan setup cleanup-log simulation | `target/debug/numanager-daqmx-plan-setup-helper ... --preflight-only --simulate-setup-error-after 1` from `lsm_daqmx_bringup_plan` | Flushed preflight rows followed by zero-exit simulated `cleared_partial_task` and `cleanup_after_setup_error` rows, with `started_tasks=false`, `wrote_output=false`, and `read_input=false`; no DAQmx task calls | Pass/Fail/Unknown |
| Helper invalid numeric/range/transfer/raster/signal input guard | Representative helper commands with non-finite or non-positive values such as `--sample-rate NaN`, `--sample-rate 0`, `--timeout NaN`, `--timeout 0`, `--frequency inf`, `--frequency 0`, or `--wait-seconds NaN`; non-finite or out-of-range counter-output duty cycle such as `--duty-cycle NaN` and `--duty-cycle 1.5`; explicitly empty or whitespace-padded route sources such as `--sample-clock-source ''`, `--sample-clock-source ' /Dev1/Ctr0InternalOutput '`, `--start-trigger ''`, and `--start-trigger ' /Dev1/PFI0 '`; explicitly empty plan physical channels or task labels such as `--ci ''` and `--ci-task ''`; leading/trailing whitespace in helper identifiers such as `--ci ' Dev1/ctr0 '`, `--ci-task ' signal '`, and `--name ' lifecycle '`; duplicate physical channels such as `--ci Dev1/ctr0 --co Dev1/ctr0`; duplicate active task labels such as `--ci-task signal --ai-task signal`; invalid signal metadata such as `--signal-lines 0`, non-divisible `--samples`/`--signal-lines`, `--chunk-size` without `--signal-lines`, or a chunk larger than `--samples`; single-channel helper empty channel inputs such as `target/debug/numanager-daqmx-channel-setup-helper --kind co --channel '' --dry-run` and `target/debug/numanager-daqmx-io-smoke-helper --kind co --channel '' --samples 1`; empty explicit task names such as `--name ''`, where `--unnamed` is the supported null task-name path; incomplete raster dimensions such as `--width 1` without `--height` and `--frames`; raster dimension overflow such as `--width 18446744073709551615 --height 2 --frames 1`; raster frame-product overflow such as `--width 2 --height 2 --frames 4611686018427387904`; reversed analog range; AO smoke ranges that exclude the 0 V final write; oversized `--samples`; transfer element overflow; raster `--width * --height * --frames` mismatches; and I/O smoke `--execute` without `--bench-safety-reviewed` | Non-zero exit before dry-run output or DAQmx calls, with an input-validation error rather than a DAQmx runtime error | Pass/Fail/Unknown |
| Empty task lifecycle | `target/debug/numanager-daqmx-task-lifecycle-helper` and a controlled `--start --wait-seconds ...` lifecycle run where bench-safe | Created and cleared task, or DAQmx error text; if a lifecycle call fails after start, `cleanup_after_lifecycle_error` and `stopped_task_after_error` rows appear before clear | Pass/Fail/Unknown |
| Channel setup dry run | `target/debug/numanager-daqmx-channel-setup-helper --kind <ao|do|ai|ci|co> --channel <channel> --dry-run` from `lsm_daqmx_bringup_plan` | Planned single-channel setup calls and explicit `created_task=false`; no DAQmx task calls | Pass/Fail/Unknown |
| AO channel setup | `target/debug/numanager-daqmx-channel-setup-helper --kind ao --channel <Dev>/ao0` | AO channel creation and clear; no start/write | Pass/Fail/Unknown |
| DO channel setup | `target/debug/numanager-daqmx-channel-setup-helper --kind do --channel <Dev>/port0/line0` | DO line creation and clear; no start/write | Pass/Fail/Unknown |
| AI channel setup | `target/debug/numanager-daqmx-channel-setup-helper --kind ai --channel <Dev>/ai0` | AI channel creation and clear; no start/read | Pass/Fail/Unknown |
| CI channel setup | `target/debug/numanager-daqmx-channel-setup-helper --kind ci --channel <Dev>/ctr0` | CI channel creation and clear; no start/read | Pass/Fail/Unknown |
| CO channel setup | `target/debug/numanager-daqmx-channel-setup-helper --kind co --channel <Dev>/ctr2` | CO channel creation and clear; no start/output | Pass/Fail/Unknown |
| Raster plan setup | `target/debug/numanager-daqmx-plan-setup-helper ...` from `lsm_daqmx_bringup_plan` | Flushed preflight rows for AO/DO/CI/CO tasks, order, routes, waveform intent, cleanup policy, Preflight `planned_cleanup` rows, and transfers, then channels, timing, optional triggers, reverse clear if DAQmx setup succeeds, and `cleared_partial_task` / `cleanup_after_setup_error` rows if setup fails after task creation; no start/write/read | Pass/Fail/Unknown |
| Signal plan setup | `target/debug/numanager-daqmx-plan-setup-helper ...` from `lsm_daqmx_bringup_plan` | Flushed preflight rows for CI/AI tasks, order, routes, cleanup policy, Preflight `planned_cleanup` rows, and transfers, then channels, timing, optional triggers, reverse clear if DAQmx setup succeeds, and `cleared_partial_task` / `cleanup_after_setup_error` rows if setup fails after task creation; no start/read | Pass/Fail/Unknown |
| I/O smoke dry run | `target/debug/numanager-daqmx-io-smoke-helper --kind <ao|do|ai|ci|co> --channel <channel>` | Planned single-channel NI-DAQmx I/O calls and explicit `created_task=false`; no DAQmx task calls | Pass/Fail/Unknown |
| I/O smoke cleanup-log simulation | `target/debug/numanager-daqmx-io-smoke-helper --kind <ai|ci|co> --channel <channel> ... --simulate-error-after-start` from `lsm_daqmx_bringup_plan` | Zero-exit simulated `cleanup_after_io_error` and `stopped_task_after_error=simulated_no_task` rows; no DAQmx task calls | Pass/Fail/Unknown |
| I/O smoke execute | `target/debug/numanager-daqmx-io-smoke-helper --kind <ao|do|ai|ci|co> --channel <channel> ... --execute --bench-safety-reviewed` from `lsm_daqmx_bringup_plan` | One reviewed safe AO/DO write followed by AO 0 V or DO low before clear, AI/CI read, or finite CO pulse operation with configured idle state after stop; task is cleared afterward | Pass/Fail/Unknown |

## Physical Channel Mapping

| Role | Configured channel | Inventory channel | Bench note |
| --- | --- | --- | --- |
| X galvo / piezo AO | `lsm_x_galvo` |  |  |
| Y galvo / piezo AO | `lsm_y_galvo` |  |  |
| Laser gate DO | `lsm_laser_gate` |  |  |
| Frame or line trigger DO |  |  |  |
| Analog detector AI | `lsm_detector` when AI-backed |  |  |
| APD counter CI | `lsm_detector` when CI-backed |  |  |
| Sample clock CO | `lsm_sample_clock` |  |  |

## Output And Input Validation

Output-writing and input-reading validation requires completed channel setup
evidence and recorded hardware safety constraints before any channel is driven.

| Capability | Request or setpoint | Runtime output | Hardware readback | Result | Notes |
| --- | --- | --- | --- | --- | --- |
| AO voltage | Low safe voltage |  | Meter or loopback voltage | Pass/Fail/Unknown |  |
| DO TTL | Low/high transition |  | Scope, meter, or loopback | Pass/Fail/Unknown |  |
| AI voltage | Known source or AO loopback |  | Reported voltage vs source | Pass/Fail/Unknown |  |
| CI count | Known pulse source or CO loopback |  | Count rate/count total | Pass/Fail/Unknown |  |
| CO pulse | Safe frequency and count |  | Scope or CI loopback | Pass/Fail/Unknown |  |

## LSM Task Execution Gate

Do not expose live `ConfocalImageCapture`, `ConfocalImageStream`, or
`ScanSignalStream` until these rows have evidence.

| Behavior | Evidence required | Result |
| --- | --- | --- |
| Finite task creation order | Bench log for AO/DO/AI/CI/CO tasks | Pass/Fail/Unknown |
| Routing plan topology | `routing_plan` clock producer/consumers and trigger consumers match the bench wiring | Pass/Fail/Unknown |
| Sample-clock routing | Confirmed source and dependent-task route names | Pass/Fail/Unknown |
| Derived sample-clock source | If no explicit sample-clock source is configured, the derived `/Device/CtrNInternalOutput` route for the counter-output sample clock is accepted by DAQmx for all AO/DO/AI/CI consumers | Pass/Fail/Unknown |
| Start-trigger routing | Confirmed digital edge route and start order | Pass/Fail/Unknown |
| Planned buffer dimensions | `scan_buffer_plan`, `signal_buffer_plan`, and task `buffer_plan` dimensions match the bench request | Pass/Fail/Unknown |
| Task timing intent | Preflight `planned_timing` rows match configured sample-clock and implicit finite counter-output timing before setup or reads/writes are enabled | Pass/Fail/Unknown |
| Finite runtime sequence | Preflight `planned_runtime_sequence` and `planned_completion` rows match expected buffered-write, start, read, wait, stop, and clear ordering before live execution is enabled | Pass/Fail/Unknown |
| Execution contract intent | Public `daqmx_task_plan.execution_contract` and Preflight `planned_execution_contract` rows for raster and signal plans match the intended buffered-before-start write policy, `auto_start=false`, finite read order, wait order, timeout, layout, and publish-after-validated-read policy | Pass/Fail/Unknown |
| Live executor intent | Public `daqmx_task_plan.live_executor_plan` and preflight `planned_live_executor` rows match the intended SDK task-wrapper backend, readiness gate, phase order, DAQmx API surface, and required validation gates while `executor_status=not_enabled_pending_hardware_validation` | Pass/Fail/Unknown |
| Reconstruction intent | Public raster `daqmx_task_plan.reconstruction_plan` and Preflight `planned_reconstruction` rows match the intended sample-to-pixel mapping, dimensions, accumulation, saturation, and publish-after-reconstruction gate before hardware-derived frames are enabled | Pass/Fail/Unknown |
| Runtime publication intent | Preflight `planned_publication` rows match the configured raster `FrameReady` or signal `ScanSignalChunk` output contract before hardware-derived runtime events are enabled, using public metadata names such as `frame_handle`, `stream`, `line_index`, `chunk_index`, `first_sample_index`, `sample_count`, and `sample_values` | Pass/Fail/Unknown |
| Raster timing intent | Preflight `raster_timing_preview` rows match configured sample rate, pixel period, line period, frame period, and total duration before any live writes are enabled | Pass/Fail/Unknown |
| Signal timing intent | Preflight `signal_timing_preview` rows match configured sample rate, samples_per_line, lines, chunk size, chunk period, line period, and total duration before reads are enabled | Pass/Fail/Unknown |
| Waveform intent | Raster AO/DO `waveform_plan` and preflight `waveform_preview` rows match expected scan and laser-gate timing before any live writes are enabled | Pass/Fail/Unknown |
| Cleanup plan | `cleanup_plan` and Preflight `planned_cleanup` rows for failure modes, stop/clear order, configured `daqmx_timeout`, and safe-output-state evidence match the bench run | Pass/Fail/Unknown |
| Buffered AO/DO writes | Written sample counts and idle/safe final state | Pass/Fail/Unknown |
| AI/CI reads | Expected sample count, timeout behavior, data layout | Pass/Fail/Unknown |
| Runtime capture frame publication | `ConfocalImageCapture` `FrameReady` output from numanager with frame handle, final-frame width/height, pixel format, scan/reconstruction dimensions, reconstructed pixel size, sample rate, line dwell, detector metadata, and saturated-pixel status | Pass/Fail/Unknown |
| Runtime live frame stream publication | `ConfocalImageStream` `FrameReady` output from numanager with stream id, repeated frame handles, dirty-region/update metadata, frame dimensions, pixel format, scan/reconstruction dimensions, reconstructed pixel size, timing metadata, detector metadata, and progress/status events | Pass/Fail/Unknown |
| Runtime signal chunk publication | `ScanSignalStream` `ScanSignalChunk` output with stream id, channel names, timing origin, line/chunk/first-sample indices, sample count, sample rate, sample period, sample values, dropped sample/chunk counters, overflow status, and progress/status events | Pass/Fail/Unknown |
| User stop/cancel | Observed stop, clear, and safe output state | Pass/Fail/Unknown |
| Failure cleanup | Partial setup/start/wait/read failure clears all created tasks; lifecycle-helper failures after task start should capture `cleanup_after_lifecycle_error` and `stopped_task_after_error` rows, setup-helper failures should capture `cleared_partial_task` and `cleanup_after_setup_error` rows when applicable, and I/O-smoke failures after task start should capture `cleanup_after_io_error` and `stopped_task_after_error` rows | Pass/Fail/Unknown |

## Remaining Uncertainty

| Behavior | Uncertainty | Evidence needed before support claim |
| --- | --- | --- |
| Package/license boundary | Local installer identities, Linux package license-file identities, and Windows online-installer PE/payload metadata are recorded, but legal review has not established redistribution permission and the installed Windows package/license boundary has not been audited | Completed package-intake note with legal review for exact Linux and Windows inputs |
| Installed 26.5 headers | The 26.5 Linux package input and Windows online installer are identified, but no installed 26.5 `NIDAQmx.h` tree has been audited for either target platform | Passing header inventory from an installed Linux or Windows 26.5 SDK/header tree, plus recorded bindgen regeneration command and bindgen-source audit from that same target-platform header before publishing regenerated 26.5 bindings |
| Linux NI-PAL readiness | On the current Linux host, NI-PAL can abort the process during inventory or empty-task creation | Bench host log showing runtime probe, process-isolated inventory, and empty task create/clear without process abort |
| Physical channel mapping | Configured `Dev1/...` role channels are plan inputs, not proof that those channels exist or are safely wired | Inventory output plus bench mapping for AO/DO/AI/CI/CO role channels |
| Routing semantics | `routing_plan` records candidate clock/trigger topology, but route source strings and start order are not validated on hardware | Plan-setup and bench logs showing accepted timing/trigger configuration and the observed task order |
| Output safety | AO/DO/CO helper commands are gated, but safe voltage, TTL state, load, final idle state, and pulse count are not proven | Meter/scope/loopback evidence for reviewed safe setpoints and cleanup behavior |
| Input semantics | AI/CI reads are planned, but sample layout, counts, timeout behavior, and APD/count scaling are not proven | Known-source or loopback readback logs for AI/CI, including sample count and timeout observations |
| Runtime publication | Simulator publishes `ConfocalImageCapture` `FrameReady`, `ConfocalImageStream` `FrameReady` updates, and `ScanSignalStream` `ScanSignalChunk` output with the public metadata contract; the DAQmx backend does not yet publish hardware-derived frames/chunks | Hardware-backed runtime output logs showing capture `FrameReady` final-frame metadata, live-stream `FrameReady` update/dirty-region/progress metadata, and `ScanSignalChunk` channel/timing/sample/drop/overflow/progress metadata after task execution behavior is validated |
| Failure cleanup | Helper cleanup paths are implemented for lifecycle errors after start, partial setup, and post-start I/O failure, but real DAQmx failure modes are not characterized | Bench logs capturing cleanup rows after controlled start/wait/setup/read/write failures |
