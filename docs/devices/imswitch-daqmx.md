# ImSwitch DAQmx

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver crate | `numanager-imswitch-daqmx` |
| Driver selector | `imswitch_daqmx` |
| Families | National Instruments NI-DAQmx devices used in ImSwitch-style microscope timing setups |
| Support level | Separate niche crate with configured descriptor/state model, optional NI-DAQmx runtime-version probe, and internal SDK task-wrapper compilation; live NI-DAQmx task execution is intentionally not exposed yet |
| Protocol/API evidence | ImSwitch source identifies the role split for AO, DO, AI, CI, CO, APD counting, scan timing, and TTL/analog sequencing. NI-DAQmx task API evidence and bench traces are still required before live task behavior is claimed |
| Transport/runtime | Optional NI-DAQmx vendor-runtime probe behind the `ni-daqmx-sdk` feature on Linux and Windows targets; unsupported OSes keep configured API planning behavior and report `target_platform_linux_or_windows` in readiness metadata. `property.connect = true` loads the runtime and reads its version when the feature is enabled on a supported target. On Linux, configuring `inventory_helper_path` moves the runtime-version probe into a process-isolated helper. In-process Linux device inventory is disabled because `DAQmxGetSysDevNames` can abort when NI-PAL is not initialized; Linux inventory is allowed only through the helper |
| Discovery | Config-backed two-stage discovery through `ImSwitchDaqmxDiscovery::configured` |
| Validation | No hardware validation |
| Evidence gaps | NI-DAQmx license legal review for exact packages, installed 26.5 SDK header audit, device discovery, task lifecycle, channel naming, clock/trigger routing, buffered writes, reads, completion/error semantics, safe stop/clear behavior |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `<Dev>-imswitch-daqmx-hub` | `hub`, `daq`, `ni.daqmx`, `imswitch.daqmx` | Owns one future NI-DAQmx runtime resource |
| `<Dev>-aoN` | `analog.output`, `dac`, `trigger.sink` | Analog output channel for galvos, piezo, AOM/AOTF modulation, or other voltage-controlled devices |
| `<Dev>-port0-lineN` | `digital.output`, `ttl.output`, `trigger.source`, `trigger.sink` | Digital output/TTL line for laser gates, camera triggers, line clocks, frame clocks, or shutters |
| `<Dev>-aiN` | `analog.input`, `adc` | Analog input channel for monitor voltage/focus sensors |
| `<Dev>-ciN` | `counter`, `counter.input`, `digital.input.counter` | Counter input for APD photon counting |
| `<Dev>-coN` | `counter.output`, `clock.output`, `trigger.source` | Counter output for sample clocks or pulse trains |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `Dac` | AO channel | `CapabilityRequest::Dac` with `Voltage` | `Voltage` | Configured-state update only | `voltage` is sequenceable metadata only |
| `DigitalIo` | DO/TTL line | `CapabilityRequest::DigitalIo` | `Bool` | Configured-state update only | `high` is sequenceable metadata only |
| `TriggerSource` / `TriggerSink` | DO/TTL line | `CapabilityRequest::Trigger` | `Bool` | Configured-state update only | `high` is sequenceable metadata only |
| `Adc` | AI channel | `CapabilityRequest::Adc` | `Voltage` | Configured-state readback only | No live sampling |
| `Measure` | CI counter | `CapabilityRequest::Measure` | `I64` | Configured-state readback only | No live counting |
| `PulseProgram` | CO counter | `CapabilityRequest::PulseProgram` | `Map` | Configured-state update only | No live pulse generation |
| `TriggerSource` | CO counter | `CapabilityRequest::Trigger` | `Map` | Configured-state transaction only | No live pulse generation |
| `ConfocalImageCapture` | Hub | `CapabilityRequest::ConfocalImageCapture` with scan and reconstruction maps | `Map` | Configured API summary plus non-executing DAQmx task plan | Final reconstructed image API; no live hardware execution yet |
| `ConfocalImageStream` | Hub | `CapabilityRequest::ConfocalImageStream` with scan, reconstruction, update policy, and overwrite flag | `Map` | Configured API summary plus non-executing DAQmx task plan | Live reconstructed image API; intended for dirty-region/mutable-frame updates |
| `ScanSignalStream` | Hub | `CapabilityRequest::ScanSignalStream` with timing map, channel list, and chunk size | `Map` | Configured API summary plus non-executing DAQmx task plan | Raw timed detector/DAQ sample stream API; no image assumptions |

## API Surfaces

| API | Purpose | Current implementation |
| --- | --- | --- |
| `ConfocalImageCapture` | Run a complete laser-scanning confocal acquisition and return a reconstructed final image or stack | Declared on the hub; returns a configured summary until NI-DAQmx task execution and frame-store publication are implemented |
| `ConfocalImageStream` | Run a scan while publishing live reconstructed image updates, where later samples may overwrite previous pixels in the same frame buffer | Declared on the hub; intended response path is stream plus dirty-region updates, but current crate only reports the requested policy |
| `ScanSignalStream` | Stream raw timed counter/analog/digital samples for non-raster or externally reconstructed scan cycles | Declared on the hub; current crate only validates the request shape and records requested channel names plus channel/chunk metadata |

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- lsm_confocal_capture` | Public `ConfocalImageCaptureRequest` with raster scan and reconstruction maps |
| `cargo run -p numanager-examples -- lsm_confocal_stream` | Public `ConfocalImageStreamRequest` with dirty-region updates and overwrite policy |
| `cargo run -p numanager-examples -- lsm_live_cancel imswitch` | Public continuous-stream lifecycle request; currently completes without frames because live DAQmx task execution is not exposed |
| `cargo run -p numanager-examples -- lsm_signal_stream` | Public `ScanSignalStreamRequest` for one line of raw counter/analog samples |
| `cargo run -p numanager-examples -- lsm_daqmx_bringup_plan` | Public LSM requests plus role-matched DAQmx helper commands for bench validation |
| `cargo run -p numanager-examples -- lsm_daqmx_validation_note` | Markdown bench-validation scaffold generated from public non-live task plans, including command-output rows for bench artifacts |
| `cargo run -p numanager-examples --features gui -- software_gui imswitch` | Snapshot/line-scan GUI against the configured descriptor, with backend readiness and role-channel display |
| `cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe` | Public configured discovery with `connect=true`; verifies vendor-runtime linkage and reports `backend_status` |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "imswitch_daqmx"` | Yes | string | Selects this separate niche driver crate |
| `property.device_name` | No | string | NI device name, defaults to `Dev1` |
| `property.product` | No | string | Configured NI product label |
| `property.serial_number` | No | string | Configured hardware identity |
| `property.runtime_package`, `property.runtime_version` | No | string | User-supplied NI-DAQmx package/runtime metadata |
| `property.runtime_platform` | No | string | Platform for the NI-DAQmx runtime evidence, such as Linux x86_64 or Windows x64 |
| `property.runtime_license` | No | string | License/redistribution boundary for the configured NI-DAQmx package |
| `property.sdk_header_path` | No | string | User-provided local path or package-relative path for NI-DAQmx SDK headers |
| `property.sdk_header_sha256` | No | string | SHA-256 digest for the SDK header set or archived header package |
| `property.backend_status` | No | map | Readiness map listing runtime probe state, package/header evidence flags, helper-path configuration, internal task-wrapper availability, bring-up helper availability, missing SDK evidence, feature flag, API audit, and hardware validation |
| `property.connect` | No | bool | With `ni-daqmx-sdk`, load the NI-DAQmx runtime and query its version; without the feature, construction fails closed |
| `property.live_task_execution` | No | bool | Bench-only request flag for future live DAQmx task execution; default false and does not change the support claim without recorded bench evidence |
| `property.inventory_devices` | No | bool | Request read-only DAQmx system/device inventory where safe. On Linux this requires `inventory_helper_path` so NI-PAL failures stay outside the runtime process |
| `property.inventory_helper_path` | No | string | Path to a process-isolated DAQmx helper. On Linux, `connect=true` uses this helper for runtime-version probing when configured, and `inventory_devices=true` fails closed unless this is set |
| `property.inventory_helper_timeout` | No | `TimeInterval` | Process-isolated helper timeout, default 8 s |
| `property.analog_output_count`, `digital_output_count`, `analog_input_count`, `counter_input_count`, `counter_output_count` | No | integer | Child device counts |
| `property.lsm_x_galvo`, `property.lsm_y_galvo` | No | string | Logical or physical role channels for LSM raster AO outputs; request `scan.x_galvo` and `scan.y_galvo` override these defaults |
| `property.lsm_laser_gate` | No | string | Logical or physical role channel for the LSM TTL laser gate; request `scan.laser_gate` overrides this default |
| `property.lsm_detector` | No | string | Logical or physical role channel for the default LSM detector; request `scan.detector` overrides this default |
| `property.lsm_sample_clock` | No | string | Logical or physical role channel for the optional counter-output sample clock; request `scan.sample_clock` overrides this default |
| `property.lsm_sample_clock_source` | No | string | Optional DAQmx sample-clock source route used when a request omits `scan.sample_clock_source` or `timing.sample_clock_source` |
| `property.lsm_start_trigger_source` | No | string | Optional DAQmx digital start-trigger source route used when a request omits `scan.start_trigger` or `timing.start_trigger` |
| `property.default_sample_rate` | No | `Frequency` | Default configured sample rate |
| `property.daqmx_timeout` | No | `TimeInterval` | Default DAQmx wait/stop/read/write timeout recorded in non-live task and cleanup plans |
| `property.analog_min`, `property.analog_max` | No | `Voltage` | AO/AI range metadata |
| `property.analog_output_N`, `analog_input_N` | No | `Voltage` | Initial configured channel state |
| `property.digital_output_N` | No | bool | Initial configured TTL state |
| `property.counter_input_N` | No | integer | Initial configured counter value |
| `property.counter_output_N_frequency` | No | `Frequency` | Initial configured pulse frequency |

## SDK Header Intake

Inventory the user-provided NI-DAQmx installer or package inputs before
recording runtime/header evidence:

```sh
scripts/audit-ni-daqmx-package-inputs.sh <installer-file-or-directory>
```

This records package identities only. It does not establish license terms,
legal redistribution permission, installed SDK header contents, binding
correctness, runtime behavior, or hardware behavior. When local package tools
are available, the audit also records Debian/RPM package metadata, embedded
license/copyright file identities, and Windows online-installer PE/payload
inventory.

When NI-DAQmx SDK headers are available, inventory them before implementing live
task execution:

```sh
scripts/audit-ni-daqmx-sdk-headers.sh <header-file-or-directory>
```

The header inventory records the discovered `NIDAQmx.h` count/path, header
identity, title/copyright banner, required symbols, runtime-version
property/getter symbols, and whether a literal package version macro exists.
The audit exits non-zero if the supplied file/directory does not contain
`NIDAQmx.h`.
The current Linux header has no literal package-version macro, so
runtime/package version claims must be paired with
`daqmx_runtime_probe` and package-input evidence.

Inventory a checkout of the `ni-daqmx-sys` fork that numanager links from
`https://github.com/mahogny/ni-daqmx-sys` after any bindgen regeneration or
platform-support change:

```sh
scripts/audit-ni-daqmx-sys-source.sh <ni-daqmx-sys-repo>
```

Record the output and API audit notes in
[`ni-daqmx-sdk-evidence-template.md`](ni-daqmx-sdk-evidence-template.md). Mirror
the package identity into `runtime_package`, `runtime_version`,
`runtime_platform`, `runtime_license`, `sdk_header_path`, and
`sdk_header_sha256`. The `ni-daqmx-sys` dependency uses the GitHub fork and is
target-scoped to Linux and Windows in numanager. macOS remains configured-only
unless NI-provided SDK and runtime evidence exists and a separate
target-platform binding audit has been recorded.
For regenerated 26.5 bindings, the evidence note must record the exact
installed target-platform `NIDAQmx.h` path used for bindgen and the bindgen
regeneration command. The FFI-source audit must then come from that regenerated
source state; a package archive alone is not an installed-header audit, and
Linux-generated bindings are not Windows ABI evidence.

The numanager-side target boundary can be checked without loading NI-DAQmx:

```sh
scripts/audit-ni-daqmx-target-scope.sh
```

That audit confirms the Cargo dependency is Linux/Windows target-scoped, helper
binaries require the SDK feature, helper entrypoints provide unsupported-target
failure stubs, and wrapper files do not reference NI-DAQmx FFI directly. It is
source-boundary evidence only, not Windows ABI, runtime, task, or hardware
evidence.

The helper command boundary can also be checked without creating NI tasks:

```sh
scripts/audit-ni-daqmx-no-hardware-helpers.sh
```

That audit builds the SDK-feature helper binaries and runs dry-run,
preflight-only, simulated-cleanup, and invalid-input paths. It verifies the
expected no-hardware markers such as `execute=false`, `created_task=false`,
`preflight_only=true`, `wrote_output=false`, and `read_input=false`; it is not
runtime, task, I/O, or hardware evidence.

The public plan-validation boundary can be checked without creating NI tasks:

```sh
scripts/audit-ni-daqmx-plan-validation.sh
```

That audit runs the public `lsm_daqmx_plan_validation` example and verifies
that valid configured raster/signal plans keep helper commands runnable,
invalid role/channel plans suppress setup/preflight helper commands, and the
execution gate remains `not_live_task_execution`.

The public live-task request gate can be checked without creating NI tasks:

```sh
scripts/audit-ni-daqmx-live-gate.sh
```

That audit sets `NUMANAGER_DAQMX_LIVE_TASK_EXECUTION=1` for configured ImSwitch
capture, stream, signal, and GUI smoke paths and verifies they still report
`live_task_execution_ready=false`, `execution=not_live_task_execution`, and no
frames or chunks. It is a public API gate audit only; it does not execute
NI-DAQmx tasks or provide hardware evidence.

The public runtime-probe readiness boundary can be checked with:

```sh
scripts/audit-ni-daqmx-runtime-probe.sh
```

That audit verifies config-only metadata reporting without vendor-runtime
loading and verifies that the process-isolated runtime-version helper path keeps
the runtime process in `runtime_probe_only` with
`live_task_execution_ready=false`, including when NI-PAL initialization fails
inside the helper process. It is not task, I/O, scan, or hardware evidence.

The DAQmx scaffold documentation can be checked with:

```sh
scripts/audit-ni-daqmx-example-output-sync.sh
```

That audit runs the public bring-up plan and validation-note scaffold examples
and checks that recorded example output still contains the emitted audit
commands and required scaffold sections. It is documentation drift evidence
only, not task, I/O, scan, or hardware evidence.

With `ni-daqmx-sdk` and `connect=true` on a supported target,
`backend_status` reports `runtime_probe_only` after the vendor runtime version
query succeeds.
The same map includes `configured_runtime_version`,
`configured_runtime_version_major`, `configured_runtime_version_minor`, and
`configured_runtime_version_update` when the configured package metadata
contains a dotted version string. It also includes `detected_runtime_version`,
`detected_runtime_version_major`, `detected_runtime_version_minor`, and
`detected_runtime_version_update` when the runtime or process-isolated helper
can report those components. `runtime_version_comparison`,
`runtime_version_matches`, and `runtime_version_comparison_basis` summarize
whether the configured package version matches the runtime that was actually
loaded. `runtime_version_matches` is `Null` when there is no configured version,
no runtime probe, or insufficient parseable component data.
The `daqmx_runtime_probe` example prints compact readiness, missing-evidence,
helper-build, promotion-gate, and promotion-gate-status summary lines after the
raw `backend_status` map, so the external live-execution gates are visible
without parsing the full debug map.
`inventory_helper_configured` records whether a helper path was supplied, and
`inventory_helper_timeout` records the configured helper supervision timeout.
On Linux, `connect=true` with a helper path uses
`numanager-daqmx-inventory-helper --version-only` for a process-isolated
runtime-version probe even when `inventory_devices=false`; this keeps NI-PAL
abort failures out of the runtime process.
The `daqmx_runtime_probe` example defaults to `Dev1`, `NI-DAQmx`, the current
host platform, a user-provided third-party license boundary, and
`/usr/include/NIDAQmx.h`; bench runs can override those fields with
`NUMANAGER_DAQMX_DEVICE_NAME`, `NIDAQMX_RUNTIME_PACKAGE`,
`NUMANAGER_DAQMX_RUNTIME_VERSION`, `NIDAQMX_RUNTIME_PLATFORM`,
`NIDAQMX_RUNTIME_LICENSE`, `NIDAQMX_HEADER_PATH`, and
`NIDAQMX_HEADER_SHA256`. The legacy `NIDAQMX_RUNTIME_VERSION` variable is still
accepted for runtime-version metadata, but the numanager-prefixed variable is
preferred for new bench scripts. Set `NUMANAGER_DAQMX_LIVE_TASK_EXECUTION=1`
only for a bench run that intentionally requests the future live-task path; this
records `live_task_execution_requested=true` but does not bypass the
support-evidence boundary. Set `NUMANAGER_DAQMX_CONFIG_ONLY=1` to print the effective probe
configuration and no-runtime `backend_status` with `connect=false`, without
loading the vendor runtime. Set
`NUMANAGER_DAQMX_RUNTIME_HELPER=target/debug/numanager-daqmx-inventory-helper`
to use the isolated version-only helper path without requesting inventory.
`NUMANAGER_DAQMX_HELPER_TIMEOUT_SECONDS` overrides the process-isolated helper
timeout for runtime probe, inventory probe, bring-up plan, and validation-note
examples. When it is set, generated bring-up and validation-note command lists
prefix the supervised runtime helper probe commands with the same value so bench
logs preserve the helper supervision policy. Generated runtime-probe commands
also include shell-safe prefixes for other configured probe metadata variables,
including package/runtime/header identity, so saved bench notes do not rely on
hidden shell state.
Clients should use `feature_requested`, `target_supported`, `feature_enabled`,
`metadata_configured`, `package_identity_recorded`, `sdk_header_recorded`,
`live_task_execution_requested`, and `live_task_execution_ready` to separate the
requested Cargo feature, target-platform support, compiled SDK backend,
packaging and SDK-header readiness, configured-vs-detected runtime-version
comparison, bench-run intent, and the live task execution gate.
`feature_enabled` means the NI-DAQmx SDK backend is compiled for the current
target, not merely that the Cargo feature was requested.
If a configured runtime version is present, live task execution remains blocked
unless the detected runtime version compares as a confirmed match; an explicit
mismatch is reported as `runtime_version_mismatch`, and partial or unknown
detection is reported as `runtime_version_unverified`.
If no configured runtime-version mismatch is present and package/header
metadata, process-isolated runtime probing, and
`NUMANAGER_DAQMX_LIVE_TASK_EXECUTION=1` are all present, readiness advances only
to `pending_hardware_validation`; it still does not create tasks, publish
hardware frames/chunks, or enable live scans.
`configured` remains false and `missing` includes
`api_audit_and_hardware_validation` until bench validation has been recorded.
`external_promotion_gates` lists the remaining non-code gates for live
execution promotion: legal review, installed Windows package/license review,
installed Linux/Windows 26.5 header audits, NI-PAL/device inventory, bench
safety preconditions, task ordering/routing/completion/cleanup bench
validation, runtime publication hardware validation, and a hardware validation
note.
`external_promotion_gate_statuses` mirrors those gate names as structured
backend-status entries with `status=pending`, `support_claim=not_validated`, and
the required evidence text that must be satisfied before promotion.
The same structured status map is mirrored in
`daqmx_task_plan.live_task_execution_readiness`, so clients can verify a
capture, stream, or signal plan agrees with backend readiness without joining
separate free-text fields.
The `lsm_daqmx_validation_note` example expands `external_promotion_gates` into per-gate evidence rows
so bench notes can track the required legal, installed header, NI-PAL,
bench-safety, task-behavior, runtime-publication, and hardware-note inputs
without inferring them from the compact backend-status list.
The `software_gui imswitch` source summary also displays the compact
`promotion_gate_statuses=[pending=9]` count from the same backend metadata.
`bringup_helpers_compiled` reports whether the inventory, task-lifecycle,
channel-setup, plan-setup, and I/O smoke helper binaries are available in the
current SDK-feature build.
The configured LSM API result maps also include
`live_task_execution_requested`, `live_task_execution_ready=false`, and
`live_task_execution_blocker`, plus a structured
`live_task_execution_readiness` map with feature, target, package/header,
runtime, runtime-version-comparison, missing-evidence,
external-promotion-gate, and hardware-validation status fields. Clients can
show the same gate reason next to snapshot, live-image, and line-scan results.
Local installer/package identities, Linux package license-file identities,
Windows online-installer PE/payload inventory, and unresolved legal
review/header boundaries are recorded in
[`ni-daqmx-package-intake.md`](ni-daqmx-package-intake.md).
Raster LSM task plans include a `role_channels` map that resolves configured or
request-supplied role channels into physical NI strings for bench bring-up.
Resource and device metadata also include `lsm_role_channels` for the configured
defaults discovered before a scan is submitted, plus `lsm_routing` for optional
descriptor-level `sample_clock_source` and `start_trigger_source` route
defaults. Per-request route fields override these descriptor defaults. Raster
plans derive `/Device/CtrNInternalOutput` from the configured counter-output
sample-clock channel when no explicit sample-clock source is supplied, pass that
route to helper commands, and keep route acceptance pending hardware validation.
Plans
also include `scan_buffer_plan`, `signal_buffer_plan`, and per-task
`buffer_plan` maps that describe intended write/read/generate direction,
sample/channel dimensions, transfer API, and candidate DAQmx layout while still
marking buffer evidence as pending hardware validation. A structured
`runtime_sequence` records the intended finite setup, buffered-write, start,
read, counter-output wait, stop, and clear phases, and `completion_plan` records
the finite sample count and configured timeout. A structured `execution_contract`
records the top-level write/read/wait contract that the future live executor
must preserve, including buffered-before-start writes, `auto_start=false`,
finite expected-sample reads, candidate layout, timeout, and the rule that
hardware-derived events are published only after validated read/reconstruction.
This remains `contract_evidence_status=pending_hardware_validation`. A
structured `live_executor_plan` records the future SDK-backed executor status,
Linux/Windows optional target scope, readiness gate, execution phases, DAQmx API
surface, and required validation gates while remaining
`not_enabled_pending_hardware_validation`. A structured raster
`reconstruction_plan` records the intended sample-to-pixel mapping, input
tasks, scan/reconstruction dimensions, pixel format, accumulation,
background-subtraction, saturation, and publish-after-reconstruction gate before
hardware-derived frame events are enabled. A structured `publication_plan`
records the intended public runtime event (`FrameReady` for raster capture or
streaming and `ScanSignalChunk` for signal streams), expected frame/chunk
dimensions, required metadata fields, and
`publication_evidence_status=pending_hardware_validation`. A structured
`cleanup_plan` records stop/clear order, configured `daqmx_timeout`, partial
setup cleanup intent, expected failure-cleanup modes, started-task
stop-before-clear strategy, and the pending safe-output validation boundary. Raster
and signal task plans also include a structured `cancel_plan` for future public
cancel handling; it records request-stop strategy, stop/clear order, timeout,
safe-output uncertainty, and `cancel_evidence_status=pending_hardware_validation`.
Raster AO/DO tasks also include candidate `waveform_plan` metadata for x-fast/y-slow
scan output and laser-gate timing; these plans contain no generated voltage or
TTL samples, and no hardware evidence has been recorded for them.
The plan-level `execution_gate`, `live_task_execution_requested`,
`live_task_execution_ready=false`, and `live_task_execution_blocker` fields,
plus structured `live_task_execution_readiness` fields, make the non-live task
boundary inspectable even when clients only display `daqmx_task_plan`.
`plan_validation` records whether all requested roles/channels map to planned
tasks and whether the generated helper commands are complete enough to run.
Raster validation reports role/channel type mismatches such as an X galvo mapped
to an AI channel; signal validation reports unrecognized requested signal
channels. Invalid or partial plans keep the task-plan metadata for inspection
but set `plan_preflight_helper_command` and `plan_setup_helper_command` to null.
Valid plans are still non-live configured summaries unless bench evidence has
been recorded.
The `lsm_daqmx_plan_validation` example prints a valid raster and signal
baseline with runnable helper commands, then intentionally invalid raster and
signal requests that expose the validation status, recognized task count,
unrecognized channel count, invalid role count, and null helper-command fields.
The `imswitch` LSM examples use the same public configured descriptor path and
accept bench-channel overrides through `NUMANAGER_DAQMX_DEVICE_NAME`,
`NUMANAGER_DAQMX_LSM_X_GALVO`, `NUMANAGER_DAQMX_LSM_Y_GALVO`,
`NUMANAGER_DAQMX_LSM_LASER_GATE`, `NUMANAGER_DAQMX_LSM_DETECTOR`,
`NUMANAGER_DAQMX_LSM_SAMPLE_CLOCK`,
`NUMANAGER_DAQMX_LSM_SAMPLE_CLOCK_SOURCE`,
`NUMANAGER_DAQMX_LSM_START_TRIGGER_SOURCE`, `NUMANAGER_DAQMX_SIGNAL_AI`, and
comma-separated `NUMANAGER_DAQMX_SIGNAL_CHANNELS`. `NUMANAGER_DAQMX_TIMEOUT_SECONDS`
overrides the configured `daqmx_timeout` used in cleanup plans and generated
helper `--timeout` arguments. `NUMANAGER_DAQMX_LIVE_TASK_EXECUTION=1` sets the
configured live-task request flag for readiness display and LSM result metadata
only. These override descriptor and request planning metadata only; the
generated helper commands remain non-live unless an individual helper is
explicitly run, and `--execute` remains bench-only.
`routing_plan` records intended sample-clock producer/consumer tasks,
configured or omitted route source strings, start-trigger consumers, and routing
evidence status. The `lsm_daqmx_bringup_plan` example turns public task-plan
roles into compact backend-readiness and promotion-gate status output, including
configured-vs-detected runtime-version comparison,
package/header/FFI-source and external-gates evidence commands, an SDK-feature
`cargo build -p numanager-imswitch-daqmx --features ni-daqmx-sdk --bins`
command, plus `numanager-daqmx-inventory-helper`,
`numanager-daqmx-task-lifecycle-helper`, and
`numanager-daqmx-channel-setup-helper` commands for bench validation. It also
prints a runtime-probe command that delegates inventory to the process-isolated
helper, the task plan's `plan_preflight_helper_command`,
`plan_setup_helper_command`, plus dry-run and conservative
`numanager-daqmx-io-smoke-helper --execute --bench-safety-reviewed` commands for later bench I/O
validation. It prints lifecycle dry-run and cleanup-log simulation commands plus
invalid numeric/range/transfer/raster-consistency helper-input guard commands before real
setup commands so bench logs can prove the helpers reject `NaN`/`inf`
sample-rate, timeout, frequency, reversed analog range, setpoint, invalid duty-cycle,
empty or whitespace-padded routes, empty plan channels/task labels,
leading/trailing whitespace in helper identifiers, duplicate physical channels,
duplicate active task labels,
single-channel empty channel input, empty explicit task names,
incomplete raster dimensions, raster dimension overflow, raster frame-product
overflow, oversized sample-count, transfer-element overflow, and raster-mismatch
input before any DAQmx call.
Runtime-probe, inventory, lifecycle, guard, channel-setup, and I/O
smoke commands are generated from shared example code used by both the bring-up
plan and validation note scaffold. The dry-run
smoke commands print planned calls without DAQmx task creation; the execute
variants are bench-only. The preflight command validates
and prints the configured
task/transfer plan, setup/start/read/stop/clear order, route producer/consumer
rows, per-task sample-clock/implicit timing rows, raster and signal timing
preview rows, AO/DO waveform-intent rows, compact first/middle/final AO/DO
waveform preview rows, finite runtime sequence/completion rows,
`planned_live_executor` rows for the future SDK task-wrapper phase order and
validation gates,
`planned_reconstruction` rows for raster sample-to-pixel reconstruction intent,
`planned_publication` rows for raster `FrameReady` and signal
`ScanSignalChunk` intent using the same public metadata names as
`daqmx_task_plan.publication_plan` (`frame_handle`, `stream`, `line_index`,
`chunk_index`, `first_sample_index`, `sample_count`, and `sample_values`),
cleanup policy, and `planned_cleanup` rows with
failure modes, stop-before-clear strategy, stop/clear order, timeout, and
pending safe-output-state evidence without NI-DAQmx task calls.
Generated helper
commands pass runtime task names through `--ao-task`,
`--do-task`, `--ai-task`, `--ci-task`, and `--co-task` so preflight task, order,
route, waveform, and transfer rows can be compared directly with
`daqmx_task_plan`. The
setup command creates the planned task set, configures channels/timing/triggers,
passes configured AO voltage bounds and timeout, prints and flushes preflight
planned-task/order/route/timing/waveform/preview/transfer rows before DAQmx task creation, and
clears tasks without starting, writing, or reading. If setup fails after
creating a task, the helper prints
`cleared_partial_task` for the failed task when cleanup succeeds, then clears
any earlier tasks and prints `cleanup_after_setup_error`. Use preflight only to
check the configured plan shape, not as hardware setup evidence. With
`--preflight-only --simulate-setup-error-after N`, the same helper emits
no-DAQmx partial-setup cleanup rows so bench-log capture can be rehearsed before
running real setup commands. Helper CLIs
reject non-finite timing, reversed analog ranges, setpoint, frequency, duty-cycle,
timeout, empty channel inputs, and empty explicit task names before printing
dry-run output or calling NI-DAQmx. Use `--unnamed` for a null DAQmx task name.
The plan-setup helper also rejects empty or whitespace-padded route sources,
empty physical channels/task labels, leading/trailing whitespace in helper identifiers,
duplicate physical channels, duplicate active task labels, incomplete raster
dimensions, raster dimension overflow, oversized sample counts, raster
frame-product overflow, overflowing per-task transfer element counts, and raster
dimension products that do not match `--samples`.
The `lsm_daqmx_validation_note` example emits a markdown scaffold from the same
public task-plan fields, including the expected preflight task/order/route,
waveform, and transfer rows. It also prints run-identity, evidence-source,
setup/safety, and required-artifact tables that preserve configured device,
role-channel, route, signal-channel, timeout, host, transport, firmware/software,
package/runtime, source, and safety placeholders for the bench note. The note
also reads public `backend_status` and prints a `Backend Readiness` table so the
per-plan `live_task_execution_readiness` rows can be compared against the
backend blocker, missing-evidence list, and runtime-version comparison fields
before promotion.
Physical channel mapping and output/input validation tables copy the resolved
public plan channels while leaving inventory, runtime-output, and hardware
readback columns blank for bench evidence. The generated note also mirrors the
LSM task-execution promotion gate with finite-task, routing, buffered I/O,
runtime publication, cancel, and failure-cleanup rows left `Unknown` until
hardware evidence exists.
The command-output log table records exit status and stdout/stderr artifact
paths, while each hardware-evidence row stays `Unknown` until a bench run fills
it in.
Task execution remains not live until the API audit, implementation, and
hardware validation are complete. Use
[`ni-daqmx-bench-validation-checklist.md`](ni-daqmx-bench-validation-checklist.md)
to record the required bench sequence before changing that support claim. Runtime
publication evidence is split by public API: `ConfocalImageCapture` must record
hardware-backed final `FrameReady` frame handles and metadata,
`ConfocalImageStream` must record repeated hardware-backed `FrameReady` updates
with dirty-region/update and progress/status metadata, and `ScanSignalStream`
must record hardware-backed `ScanSignalChunk` channel, timing, sample,
dropped-count, overflow, and progress/status metadata rather than simulator
events.

The crate also provides standalone helper binaries for manual bring-up:

```sh
cargo build -p numanager-imswitch-daqmx --features ni-daqmx-sdk --bin numanager-daqmx-inventory-helper
target/debug/numanager-daqmx-inventory-helper --device Dev1 --version-only
target/debug/numanager-daqmx-inventory-helper --device Dev1

cargo build -p numanager-imswitch-daqmx --features ni-daqmx-sdk --bin numanager-daqmx-task-lifecycle-helper
target/debug/numanager-daqmx-task-lifecycle-helper --dry-run
target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --start --wait-seconds 0.250000
target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --start --wait-seconds 0.250000 --simulate-error-after-start
target/debug/numanager-daqmx-task-lifecycle-helper

cargo build -p numanager-imswitch-daqmx --features ni-daqmx-sdk --bin numanager-daqmx-channel-setup-helper
target/debug/numanager-daqmx-channel-setup-helper --kind ao --channel Dev1/ao0 --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind ao --channel Dev1/ao0

cargo build -p numanager-imswitch-daqmx --features ni-daqmx-sdk --bin numanager-daqmx-plan-setup-helper
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 262144 --width 512 --height 512 --frames 1 --ao-task ao_scan --do-task do_laser_gate --ci-task ci_detector --co-task co_sample_clock --ao Dev1/ao0 --ao Dev1/ao1 --do Dev1/port0/line0 --ci Dev1/ctr0 --co Dev1/ctr2 --min-volts -10.000000 --max-volts 10.000000 --sample-clock-source /Dev1/Ctr2InternalOutput --timeout 10.000000

cargo build -p numanager-imswitch-daqmx --features ni-daqmx-sdk --bin numanager-daqmx-io-smoke-helper
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --volts 0
target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1 --simulate-error-after-start
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --volts 0 --execute --bench-safety-reviewed
```

Run these helpers outside the runtime process during bring-up. The inventory
helper's `--version-only` mode calls only the runtime-version getters and exits
before device inventory. The task
lifecycle helper's `--dry-run` mode prints planned lifecycle calls without
creating a DAQmx task, including optional start/wait/stop rows when requested.
Its `--simulate-error-after-start` mode is dry-run-only and emits no-DAQmx
cleanup-log rows for lifecycle failure capture.
The channel setup helper's `--dry-run` mode prints planned channel creation
calls without creating a DAQmx task. The I/O smoke
helper performs no DAQmx task calls unless `--execute --bench-safety-reviewed`
is present; the executing
path is for bench setups where the physical channel, load, loopback, and safe
output state have already been reviewed. AO execute commands reject analog
ranges that exclude 0 V, write the requested setpoint, then write 0 V before
clear. DO execute commands write the requested line state, then write low before
clear. CO execute commands report the configured idle state expected after stop.
If a lifecycle helper path fails after
starting a task, it attempts explicit stop before clear and prints
`cleanup_after_lifecycle_error` / `stopped_task_after_error` rows. If an
executing I/O smoke path fails after starting a task, the helper attempts an
explicit stop before clear and prints `cleanup_after_io_error` /
`stopped_task_after_error` rows for the bench log. Without `--execute`,
`--simulate-error-after-start` emits simulated cleanup rows without DAQmx calls
so bench-log capture can be checked safely. The runtime can
also delegate Linux runtime-version probing and inventory to
`numanager-daqmx-inventory-helper` when `inventory_helper_path` is configured,
but helper crashes or non-zero exits are reported as `device_inventory_error`;
if the helper aborts before it can print structured runtime-version lines,
`detected_runtime_version` is reported as `unknown`, the numeric version
component fields remain null, and the failure stays outside the runtime
process. On the current Linux host, `DAQmxGetSysDevNames`
and even empty-task creation can abort when NI-PAL is not initialized, so the
runtime still does not call Linux inventory in-process and does not expose live
task execution. The public `daqmx_runtime_probe` example prints a compact
`inventory:` summary for requested inventory state, helper isolation, detected
device count, configured-device detection, configured-device identity, and any
contained helper or configured-device error.

## Remaining Work

| Area | Gap |
| --- | --- |
| Vendor runtime | Local 2026 Q3 Linux and 26.5 Windows installer file identities are recorded with `scripts/audit-ni-daqmx-package-inputs.sh` in [`ni-daqmx-package-intake.md`](ni-daqmx-package-intake.md); Linux package license files and Windows online-installer PE/payload metadata are identified there but still need legal review before redistribution, installed 26.5 headers must be audited before regenerating bindings for that version, and `ni-daqmx-sdk` remains limited to runtime linkage/version probing |
| Device inventory | Runtime Linux inventory is process-isolated through `inventory_helper_path`; bench validation still needs a host with initialized NI-PAL and a real/configured device before inventory can become evidence |
| Task lifecycle | Standalone empty-task create/clear helper target exists for bring-up. On this Linux host it aborts in `libnipalu.so` because NI-PAL is not initialized; bench validation must record successful create/clear before runtime task execution is exposed |
| Channel setup | Standalone AO/DO/AI/CI/CO channel setup helper exists for bench validation of physical names and DAQmx error text without starting tasks, writing outputs, or reading inputs |
| API evidence | Header/API audit, internal task-wrapper status, configured task plans, and standalone helper status are recorded in [`ni-daqmx-sdk-api-audit.md`](ni-daqmx-sdk-api-audit.md); behavior still needs bench validation before live task execution is exposed |
| Hardware validation | Bench validate physical channel mapping, AO voltage, DO TTL state, AI voltage, CI APD counts, CO frequency/pulse count, task completion, timeout, and safe stop behavior |
| Timing plans | Map `TimingPlan` to finite buffered AO/DO plus counter clock tasks only after NI task ordering and trigger routing are evidenced |
| Streams/images | Bind `ConfocalImageCapture`, `ConfocalImageStream`, and `ScanSignalStream` to runtime frame/stream handles after DAQ task behavior, sample-to-pixel reconstruction, frame metadata, signal chunk timing/sample metadata, dropped-count reporting, and overflow reporting are validated |
