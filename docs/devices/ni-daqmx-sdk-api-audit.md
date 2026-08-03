# NI-DAQmx SDK API Audit

This note records the API surface currently audited for the optional
`numanager-imswitch-daqmx` live backend. It is evidence for header availability
and FFI shape only. It is not evidence that task ordering, trigger routing,
buffer behavior, or hardware completion semantics are correct on a real device.

## Package Identity

| Item | Value |
| --- | --- |
| Runtime package | NI-DAQmx |
| Runtime version observed by runtime probe | 26.3.1 |
| Runtime library observed | `/usr/lib/x86_64-linux-gnu/libnidaqmx.so.26.3.1` |
| Platform | Linux x86_64 |
| Header | `/usr/include/NIDAQmx.h` |
| Header audit `NIDAQmx.h` count | 1 |
| Header audit `NIDAQmx.h` path | `/usr/include/NIDAQmx.h` |
| Header SHA-256 | `86491926d3485439ba49efa1ac610ac1d2541dcff703b51c7f9be27c4b646164` |
| Header inventory SHA-256 | `3e99eb9d5a98fe39a0c6c54c1cac490ea4de1f5c1448e981582c8dfb1b2d5b45` |
| Header title line | `/* Title:       NIDAQmx.h                                                     */` |
| Header copyright line | `/*    Copyright (c) National Instruments 2003-2026.  All Rights Reserved.     */` |
| Literal package-version macro in header | none found by `scripts/audit-ni-daqmx-sdk-headers.sh` |
| FFI source | `https://github.com/mahogny/ni-daqmx-sys` fork regenerated from the Linux header with bindgen scripts |
| FFI source path | Git dependency `https://github.com/mahogny/ni-daqmx-sys`; audited local checkout `/home/mahogny/github/claude/ni-daqmx-sys` |
| FFI package version | `26.3.1` |
| FFI package edition | `2024` |
| FFI package license | `MIT` |
| FFI source revision | `a0b8093686acd349bfea9b984dc4e656682e0a50` |
| FFI source worktree status | dirty: local platform-boundary and bindgen-generator workflow patches not yet committed |
| Cargo bindgen dependency | `0.72` |
| Generated bindgen version | `0.72.1` |
| Generated bindings SHA-256 | `7974f4f3b0dbb49e73ad7332fa5bab1824d2ef752965395322878f2565996ccc` |
| Linux bindgen script SHA-256 | `78c71fd940a90e23ba347624744dd770c49e9bf802b446b8366047aefaf9ee7d` |
| Windows bindgen script SHA-256 | `34402d9aa5d0021238f68221ecd0375d05d03495b21643d68e0b479f637763ce` |
| Cargo bindgen fallback SHA-256 | `35b793879adddd0858b0c08b2b53820fc72a41c5445c3c1c375bdfab70802961` |
| Bindgen wrapper SHA-256 | `446b133430e6ca8830508e81fd4d4e0efacc480e76698fc7064b57a76d2fe0f5` |
| Build script SHA-256 | `2ddfcbd3c8495b73f11f743b76075276cef725d88ffd568165a5934c50f47b3e` |

Local installer/package identities for the available 2026 Q3 Linux driver
archive and Windows 26.5 online installer are recorded in
[`ni-daqmx-package-intake.md`](ni-daqmx-package-intake.md). Those identities do
not replace the installed header inventory above, and the Windows 26.5 installer
still needs an installed Windows header audit before bindings are regenerated
for that platform/version.

The `daqmx_runtime_probe` example can mirror bench-host metadata through
environment variables before constructing the configured public descriptor:
`NUMANAGER_DAQMX_DEVICE_NAME`, `NIDAQMX_RUNTIME_PACKAGE`,
`NUMANAGER_DAQMX_RUNTIME_VERSION`, `NIDAQMX_RUNTIME_PLATFORM`,
`NIDAQMX_RUNTIME_LICENSE`, `NIDAQMX_HEADER_PATH`, and
`NIDAQMX_HEADER_SHA256`. The legacy `NIDAQMX_RUNTIME_VERSION` name remains
accepted for runtime-version metadata, but new bench scripts should prefer the
numanager-prefixed variable. `NUMANAGER_DAQMX_LIVE_TASK_EXECUTION=1` records
bench-run intent for the future live-task path. These values affect readiness
metadata only; the probe still performs no task creation, channel setup, reads,
writes, or LSM scan execution. Set `NUMANAGER_DAQMX_CONFIG_ONLY=1` to construct
the configured descriptor with `connect=false`, print the effective metadata,
read the no-runtime `backend_status`, and avoid loading the vendor runtime. On
Linux, set
`NUMANAGER_DAQMX_RUNTIME_HELPER=target/debug/numanager-daqmx-inventory-helper`
to run the runtime-version probe through the process-isolated helper's
`--version-only` mode without requesting device inventory.
`NUMANAGER_DAQMX_HELPER_TIMEOUT_SECONDS` overrides the helper process timeout
for runtime probe and validation examples.

The local fork worktree was dirty when these source identities were recorded
because the platform-boundary and bindgen-generator workflow patches had not
been committed. Commit the fork change and update this revision/status row when
the `numanager-imswitch-daqmx` dependency or feature behavior is pinned to that
source state.

Reproduce the aggregate local evidence-input and FFI source inventories with:

```sh
scripts/audit-ni-daqmx-evidence-inputs.sh
```

That aggregate audit runs the package-input, installed-header, and FFI-source
inventory scripts against configured local paths and checks stable markers
without loading the runtime or making task/hardware claims. Override
`NUMANAGER_DAQMX_PACKAGE_INPUTS`, `NUMANAGER_DAQMX_HEADER_ROOT`, and
`NUMANAGER_DAQMX_SYS_REPO` when the inputs or fork checkout live elsewhere.

```sh
scripts/audit-ni-daqmx-sys-source.sh /home/mahogny/github/claude/ni-daqmx-sys
```

That audit now checks the source revision, worktree cleanliness, package
version/edition/license, Cargo bindgen dependency, generated bindgen version,
generated bindings hash, Linux and Windows bindgen scripts, Cargo fallback
generator, signed macro constant generation, wrapper include,
crate-root generated-bindings include,
Cargo `links = "nidaqmx"` metadata, build-script link paths for Windows and
Linux x86_64, whether build-script cfgs are Linux-specific, whether macOS is
explicitly rejected, whether unsupported non-Linux/non-Windows targets are
explicitly rejected, the non-Windows 32-bit compile guard, fork-local runtime
smoke test presence and ignored-by-default marker, and generated binding
availability for the DAQmx lifecycle, AO/DO/AI/CI/CO, timing/trigger, error,
runtime-version, alias, and constant
symbols used by the backend. It is still an FFI-source audit only: Windows
support requires the same header audit and bindgen script procedure against an
installed Windows NI header before publishing a Windows binding update. macOS
support remains configured-only in numanager unless NI-provided SDK/runtime
evidence and separate target-platform binding/link audits exist; treating every
non-Windows target as Linux is not sufficient. The fork-local smoke test is
ignored by default, treated only as a runtime probe, and is not numanager
evidence for task behavior.

The current fork audit reports the required DAQmx symbols present and the
platform-boundary verdict clean for the local dirty worktree: Windows and Linux
x86_64 link paths are recorded, the Linux path is guarded by a Linux-specific
cfg, macOS is explicitly unsupported, and 32-bit non-Windows targets are
rejected. Other non-Linux/non-Windows targets are also explicitly unsupported so
they cannot inherit Linux linker behavior. The fork still needs an installed
Windows header audit and Windows bindgen regeneration before publishing Windows
bindings.

The numanager-side target boundary is audited separately with:

```sh
scripts/audit-ni-daqmx-target-scope.sh
```

That audit checks that `crates/numanager-imswitch-daqmx/Cargo.toml` keeps
`ni-daqmx-sys` target-scoped to Linux/Windows, that all DAQmx helper binaries
require the `ni-daqmx-sdk` feature, that helper wrappers load implementation
files only behind Linux/Windows cfgs, that unsupported targets get explicit
failure stubs, and that wrappers do not reference NI-DAQmx FFI directly. It is
source-boundary evidence only; it does not establish Windows ABI compatibility,
runtime installation, task ordering, completion, cleanup, or hardware behavior.

The helper no-hardware boundary is audited with:

```sh
scripts/audit-ni-daqmx-no-hardware-helpers.sh
```

That audit builds the SDK-feature helper binaries and runs only dry-run,
preflight-only, simulated-cleanup, and invalid-input guard paths. It confirms
the helper outputs preserve no-hardware markers such as `execute=false`,
`created_task=false`, `preflight_only=true`, `wrote_output=false`, and
`read_input=false`; it does not execute NI-DAQmx tasks or provide hardware
evidence.

The public plan-validation boundary is audited with:

```sh
scripts/audit-ni-daqmx-plan-validation.sh
```

That audit runs the public `lsm_daqmx_plan_validation` example and checks valid
configured helper-command readiness, invalid-plan helper-command suppression,
and the non-live execution gate. It is not runtime, task, I/O, scan, or
hardware evidence.

The public live-task request gate is audited with:

```sh
scripts/audit-ni-daqmx-live-gate.sh
```

That audit sets `NUMANAGER_DAQMX_LIVE_TASK_EXECUTION=1` and verifies configured
ImSwitch capture, stream, signal, and GUI smoke paths record live-task intent
while keeping `live_task_execution_ready=false`,
`execution=not_live_task_execution`, and no frame/chunk output. It is not
runtime, task, I/O, or hardware evidence.

The public runtime-probe readiness boundary is audited with:

```sh
scripts/audit-ni-daqmx-runtime-probe.sh
```

That audit verifies config-only metadata without vendor-runtime loading and
process-isolated runtime-version probing with `execution_status` remaining
`runtime_probe_only` and `live_task_execution_ready=false`. It is not task, I/O,
scan, or hardware evidence.

The DAQmx scaffold documentation boundary is audited with:

```sh
scripts/audit-ni-daqmx-example-output-sync.sh
```

That audit runs the public bring-up plan and validation-note scaffold examples
and checks that recorded example output still contains their emitted audit
commands and required scaffold sections. It is documentation consistency
evidence only.

## Header Inventory

`scripts/audit-ni-daqmx-sdk-headers.sh /usr/include/NIDAQmx.h` reports
`NIDAQmx.h count = 1`, the audited path `/usr/include/NIDAQmx.h`, and all
expected symbols present for the first live backend implementation pass:

| Area | Symbols present |
| --- | --- |
| Task lifecycle | `DAQmxCreateTask`, `DAQmxStartTask`, `DAQmxStopTask`, `DAQmxClearTask`, `DAQmxWaitUntilTaskDone` |
| AO voltage output | `DAQmxCreateAOVoltageChan`, `DAQmxWriteAnalogF64` |
| DO line output | `DAQmxCreateDOChan`, `DAQmxWriteDigitalLines` |
| AI voltage input | `DAQmxCreateAIVoltageChan`, `DAQmxReadAnalogF64` |
| CI counter input | `DAQmxCreateCICountEdgesChan`, `DAQmxReadCounterU32`, `DAQmxReadCounterF64` |
| CO pulse output | `DAQmxCreateCOPulseChanFreq`, `DAQmxCreateCOPulseChanTicks` |
| Timing and triggers | `DAQmxCfgSampClkTiming`, `DAQmxCfgImplicitTiming`, `DAQmxCfgDigEdgeStartTrig`, `DAQmxCfgDigEdgeRefTrig` |
| Error text | `DAQmxGetErrorString`, `DAQmxGetExtendedErrorInfo` |
| Runtime version | `DAQmxGetSysNIDAQMajorVersion`, `DAQmxGetSysNIDAQMinorVersion`, `DAQmxGetSysNIDAQUpdateVersion` |

The header also defines the `DAQmx_Sys_NIDAQMajorVersion`,
`DAQmx_Sys_NIDAQMinorVersion`, and `DAQmx_Sys_NIDAQUpdateVersion` runtime
property IDs. The audited Linux header does not define a literal package-version
macro, so package/version claims must come from package-intake evidence and
runtime probes rather than from the header digest alone. `backend_status`
reports `configured_runtime_version` plus
`configured_runtime_version_major`, `configured_runtime_version_minor`, and
`configured_runtime_version_update` when configured package metadata is
provided. It separately reports the string `detected_runtime_version` plus
`detected_runtime_version_major`, `detected_runtime_version_minor`, and
`detected_runtime_version_update` when the runtime or process-isolated helper
provides those components. The same status map reports
`runtime_version_comparison`, `runtime_version_matches`, and
`runtime_version_comparison_basis` so bench output can distinguish missing
configured metadata, missing runtime probes, exact/major-minor matches,
mismatches, and partial/unparseable versions without changing the live-task
readiness gate. `inventory_helper_configured` records whether the helper path
was supplied for process-isolated Linux probing, and
`inventory_helper_timeout` records the configured helper supervision timeout.
The header audit exits non-zero when a supplied file/directory does not contain
`NIDAQmx.h`, so a passing installed-header audit also proves the target-platform
bindgen input path was explicitly discovered.

The regenerated Linux bindings also confirm ABI-important aliases:

| C typedef | Rust binding on Linux x86_64 |
| --- | --- |
| `int32` | `std::os::raw::c_int` |
| `uInt32` | `std::os::raw::c_uint` |
| `TaskHandle` | `*mut std::os::raw::c_void` |

## Constants Needed

The Linux header defines these constants needed for typed runtime mapping:

| Purpose | Constant |
| --- | --- |
| Voltage units | `DAQmx_Val_Volts` |
| Sample modes | `DAQmx_Val_FiniteSamps`, `DAQmx_Val_ContSamps` |
| Edges | `DAQmx_Val_Rising`, `DAQmx_Val_Falling` |
| DO line grouping | `DAQmx_Val_ChanForAllLines` |
| Buffer layout | `DAQmx_Val_GroupByChannel`, `DAQmx_Val_GroupByScanNumber` |
| Counter direction | `DAQmx_Val_CountUp`, `DAQmx_Val_CountDown` |
| CO idle state | `DAQmx_Val_Low`, `DAQmx_Val_High` |

These should be wrapped behind local Rust constants or typed enums in the live
backend rather than leaked into public examples or public device properties.

## Proposed Task Mapping

This is the implementation target once hardware validation is available.

| numanager role | NI-DAQmx task shape | Initial task APIs |
| --- | --- | --- |
| Galvo/piezo analog scan output | One AO task containing configured AO channels | `DAQmxCreateTask`, `DAQmxCreateAOVoltageChan`, `DAQmxCfgSampClkTiming`, `DAQmxWriteAnalogF64` |
| Laser/shutter TTL output | One DO task containing configured lines | `DAQmxCreateTask`, `DAQmxCreateDOChan`, `DAQmxCfgSampClkTiming`, `DAQmxWriteDigitalLines` |
| Analog detector/monitor input | One AI task containing configured AI channels | `DAQmxCreateTask`, `DAQmxCreateAIVoltageChan`, `DAQmxCfgSampClkTiming`, `DAQmxReadAnalogF64` |
| APD photon counter input | One CI task per counter or grouped where validated | `DAQmxCreateTask`, `DAQmxCreateCICountEdgesChan`, `DAQmxCfgSampClkTiming`, `DAQmxReadCounterU32` or `DAQmxReadCounterF64` |
| Sample clock / pulse train | One CO task where the device uses counter-generated clocks | `DAQmxCreateTask`, `DAQmxCreateCOPulseChanFreq` or `DAQmxCreateCOPulseChanTicks`, `DAQmxCfgImplicitTiming` |
| Shared start trigger | Digital edge trigger on dependent tasks | `DAQmxCfgDigEdgeStartTrig` |
| Completion and cleanup | Wait, stop, clear every created task | `DAQmxWaitUntilTaskDone`, `DAQmxStopTask`, `DAQmxClearTask` |

## First Backend Slice

The first implementation slice is a low-level, feature-gated internal task
wrapper, not a public protocol API:

- own a `TaskHandle` in an RAII type: implemented in the internal
  `daqmx_task` module;
- call `DAQmxClearTask` on drop when a task was created: implemented;
- convert negative DAQmx status codes into an internal `DaqmxError`:
  implemented, with conversion to public runtime errors left to the task
  execution layer;
- collect `DAQmxGetExtendedErrorInfo` and fall back to `DAQmxGetErrorString`:
  implemented;
- create AO, DO, AI, CI, and CO tasks from configured physical channel names:
  internal wrapper methods compile behind `ni-daqmx-sdk`;
- expose no raw DAQmx symbols to examples or generated public docs: maintained;
- keep `ConfocalImageCapture`, `ConfocalImageStream`, and `ScanSignalStream`
  returning configured/API summaries until at least one bench run validates
  task order, trigger route, and stop/clear behavior.

`backend_status.feature_requested` reports whether the `ni-daqmx-sdk` Cargo
feature was requested. `backend_status.target_supported` reports whether the
current target is Linux or Windows. `backend_status.feature_enabled` reports
whether the NI-DAQmx SDK backend is actually compiled for the current target.
`backend_status.task_wrapper_compiled` reports whether this internal wrapper is
present in the current build. `backend_status.bringup_helpers_compiled` reports
whether the inventory, task-lifecycle, channel-setup, plan-setup, and I/O smoke
helper binaries are available in the current SDK-feature build. These fields
are not claims that live task execution is enabled. The helper binaries use
small platform wrappers: Linux and Windows targets include the NI-DAQmx helper
implementations, while unsupported targets compile stubs that exit with a clear
unsupported-target message instead of linking `ni-daqmx-sys`.

Bench notes should record this SDK-feature build before any
`target/debug/numanager-daqmx-*` helper command is run:

```sh
cargo build -p numanager-imswitch-daqmx --features ni-daqmx-sdk --bins
```

## Task Lifecycle Bring-Up Helper

The crate provides a standalone, feature-gated helper binary for manual DAQmx
task lifecycle bring-up:

```sh
cargo build -p numanager-imswitch-daqmx --features ni-daqmx-sdk --bin numanager-daqmx-task-lifecycle-helper
target/debug/numanager-daqmx-task-lifecycle-helper --dry-run
target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --start --wait-seconds 0.250000
target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --start --wait-seconds 0.250000 --simulate-error-after-start
target/debug/numanager-daqmx-task-lifecycle-helper
```

`--dry-run` prints the planned task lifecycle calls and explicit
`created_task=false` / `cleared_task=false` rows without creating a DAQmx task.
The default helper path calls only `DAQmxCreateTask` for an empty task and then
`DAQmxClearTask`; it does not configure channels, write outputs, start a task,
or read samples. `--start` and `--wait-seconds` exist for bench use only after a
specific setup is judged safe; with `--dry-run`, they only affect the planned
API row. `--simulate-error-after-start` requires `--dry-run --start` and prints
zero-exit no-DAQmx lifecycle cleanup-log rows so log capture can be checked
before a task is created. If a lifecycle call fails after a task has started,
the helper attempts an explicit stop before clear and prints
`cleanup_after_lifecycle_error` and `stopped_task_after_error` rows for bench
logs. The helper rejects non-finite or negative `--wait-seconds` values before
printing a plan or calling NI-DAQmx.

## I/O Smoke Bring-Up Helper

The crate also provides a standalone, feature-gated helper for minimal
single-channel I/O validation:

```sh
cargo build -p numanager-imswitch-daqmx --features ni-daqmx-sdk --bin numanager-daqmx-io-smoke-helper
target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --volts 0
target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1
target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1 --simulate-error-after-start
```

Without `--execute`, this helper prints the NI-DAQmx call plan and reports that
no task was created, no output was written, no input was read, and no pulse was
generated. With `--execute`, it creates one single-channel task, performs the
requested AO/DO write, AI/CI read, or finite CO pulse operation, and clears the
task. If an executing AI/CI/CO path fails after task start, the helper attempts
an explicit stop before clear and prints `cleanup_after_io_error` plus
`stopped_task_after_error` rows for the bench log. Use the executing path only
on a bench setup where the physical channel, load, loopback, safe voltage/TTL
state, and cleanup behavior have already been reviewed.
Without `--execute`, `--simulate-error-after-start` performs no DAQmx calls and
prints zero-exit simulated cleanup rows so log capture can be checked before
bench use. Numeric range, setpoint, frequency, duty-cycle, and timeout arguments
must be finite; invalid values fail before dry-run output or NI-DAQmx calls.

On the current Linux host, the default task-lifecycle helper process aborts
before returning a DAQmx status:

```text
libnipalu.so failed to initialize
Verify that nipalk.ko is built and loaded.
timeout: the monitored command dumped core
```

This is a runtime/driver readiness finding, not hardware validation. Runtime
driver construction therefore continues to avoid Linux task and inventory calls
unless explicitly gated and validated in a process-isolated bring-up path.

## Channel Setup Bring-Up Helper

The crate also provides a standalone helper for validating physical channel
names and channel-creation errors without starting tasks, writing outputs, or
reading inputs:

```sh
cargo build -p numanager-imswitch-daqmx --features ni-daqmx-sdk --bin numanager-daqmx-channel-setup-helper
target/debug/numanager-daqmx-channel-setup-helper --kind ao --channel Dev1/ao0 --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind ci --channel Dev1/ctr0 --dry-run
target/debug/numanager-daqmx-channel-setup-helper --kind ao --channel Dev1/ao0
target/debug/numanager-daqmx-channel-setup-helper --kind do --channel Dev1/port0/line0
target/debug/numanager-daqmx-channel-setup-helper --kind ai --channel Dev1/ai0
target/debug/numanager-daqmx-channel-setup-helper --kind ci --channel Dev1/ctr0
target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2
```

With `--dry-run`, the helper prints the planned selected channel creation call
and explicit `created_task=false` / `configured_channel=false` rows without
creating a DAQmx task. Without `--dry-run`, the helper only calls
`DAQmxCreateTask`, the selected channel creation function, and `DAQmxClearTask`.
It is intended for bench logs that establish physical channel names,
terminal/range configuration, and DAQmx error text before runtime task execution
is enabled. Numeric range, frequency, and duty-cycle arguments must be finite;
invalid values fail before dry-run output or NI-DAQmx calls.

## Plan Setup Bring-Up Helper

The crate also provides a standalone helper for validating planned multi-task
setup without starting tasks, writing outputs, or reading inputs:

```sh
cargo build -p numanager-imswitch-daqmx --features ni-daqmx-sdk --bin numanager-daqmx-plan-setup-helper
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 262144 --width 512 --height 512 --frames 1 --ao-task ao_scan --do-task do_laser_gate --ci-task ci_detector --co-task co_sample_clock --ao Dev1/ao0 --ao Dev1/ao1 --do Dev1/port0/line0 --ci Dev1/ctr0 --co Dev1/ctr2 --min-volts -10.000000 --max-volts 10.000000 --timeout 10.000000
target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1024 --ci Dev1/ctr0 --ai Dev1/ai0 --timeout 10.000000
```

The helper creates the planned AO, DO, AI, CI, and CO tasks, configures channels,
sample clocks, optional start triggers, configured AO voltage bounds, optional
CO implicit timing, and then clears all created tasks in reverse order. Before
the first DAQmx task call it prints and flushes `preflight_plan`, sample rate,
sample count, optional route sources, analog range, cleanup timeout,
`planned_task`, `planned_setup_order`, `planned_start_order`,
`planned_read_order`, `planned_stop_order`, `planned_clear_order`,
`planned_sample_clock_route`, `planned_start_trigger_route`, `cleanup_policy`,
`planned_waveform`, and `planned_transfer` rows. The route rows identify
configured source strings, candidate clock producer, consumers, and edge without
validating route availability. Runtime-generated commands pass task labels with
`--ao-task`, `--do-task`, `--ai-task`, `--ci-task`, and `--co-task` so helper
task, order, route, waveform, and transfer rows use the same names as
`daqmx_task_plan`. The waveform rows
record AO and DO scan intent, raster dimensions when supplied, channel count,
analog voltage bounds, and a pending evidence marker without generating
samples. The transfer rows include direction, element type, channel count,
sample count, layout, and configured timeout, while keeping those transfers
non-executing. Sample-rate, analog range, and timeout arguments must be finite.
`--samples` must be positive and inside the helper's conservative i32 sample
count range; per-task transfer element counts are checked for overflow and the
same conservative range; when width, height, and frame count are all supplied,
their product must match `--samples`. Invalid values fail before preflight
output or NI-DAQmx calls. Created tasks also clear on drop so a partial setup
failure still
attempts cleanup. The helper now also prints `cleared_partial_task` when a task
created inside the failing setup step is explicitly cleared, then prints
`cleanup_after_setup_error` after clearing any earlier created tasks. It does
not start tasks, write AO/DO buffers, read AI/CI samples, or claim that routing
is valid. Use
`lsm_daqmx_bringup_plan` to print commands derived from the public runtime task
plans for the configured role channels.

Each runtime task plan also includes `plan_preflight_helper_command`, which is
the same helper invocation with `--preflight-only` appended. Use that command to
validate argument parsing and emit the flushed planned-task, planned-order,
planned-route, planned-waveform, and planned-transfer rows without calling
NI-DAQmx task APIs. This is useful on hosts where the runtime library loads but
NI-PAL aborts task creation, but it is not hardware setup evidence.

## Candidate Execution Order

The candidate finite scan order to validate on hardware is:

1. Create all needed tasks.
2. Add physical channels.
3. Configure sample clock and sample count on buffered AO/DO/AI/CI tasks.
4. Configure CO implicit timing if a counter-generated clock is used.
5. Configure shared digital start triggers on dependent tasks.
6. Write AO/DO buffers with `autoStart=false`.
7. Start input tasks.
8. Start output tasks.
9. Start the clock/master task last if one exists.
10. Read AI/CI samples until the expected count is reached or timeout occurs.
11. Wait for finite output completion.
12. Stop and clear every created task in reverse dependency order.

This order is a hypothesis from API shape and common DAQ task construction. It
must be checked against the specific NI device, routing, and ImSwitch setup.

## Configured Task Planning

The `imswitch_daqmx` hub now includes a configured task-plan summary in the
three LSM API responses:

| API | Planned roles |
| --- | --- |
| `ConfocalImageCapture` | AO scan output, DO laser gate, CI or AI detector input, optional CO sample clock |
| `ConfocalImageStream` | Same raster roles as capture, with streaming marked in the plan |
| `ScanSignalStream` | CI and/or AI input tasks based on requested channel names |

The plan includes physical channel strings, configured LSM role-channel mapping,
sample rate, planned sample counts, configured sample-clock/start-trigger source
fields, `scan_buffer_plan`, `signal_buffer_plan`, per-task `buffer_plan` maps,
candidate AO/DO `waveform_plan` maps, candidate start/read/stop and clear order,
cleanup policy, structured `execution_contract`, structured `cleanup_plan`,
routing evidence status, a
structured `plan_validation`, structured `routing_plan`,
`plan_preflight_helper_command`, `plan_setup_helper_command`, and the audited
DAQmx calls that would be used. Buffer plans record intended transfer direction,
sample/channel dimensions, transfer API, candidate layout, configured
`daqmx_timeout`, and a pending evidence marker. Waveform plans record raster and
laser-gate intent only; they do not contain real scan voltage, TTL,
photon-count, or analog-input samples. The routing plan records intended
sample-clock producer/consumer tasks and start-trigger consumers without
claiming route validity. The execution contract records the top-level
buffered-before-start write policy, `auto_start=false`, finite read and wait
order, timeout, candidate layout, and publish-after-validated-read policy while
remaining `contract_evidence_status=pending_hardware_validation`.
Publication plans and preflight `planned_publication` rows use the public event
metadata vocabulary, including raster `frame_handle` / `stream` fields and
signal `line_index`, `chunk_index`, `first_sample_index`, `sample_count`, and
`sample_values` fields, while remaining pending hardware validation. The cleanup
plan records wait/stop timeout, failure cleanup intent, and a pending
safe-output state marker. Raster plans resolve
`lsm_x_galvo`,
`lsm_y_galvo`,
`lsm_laser_gate`, `lsm_detector`, and `lsm_sample_clock` defaults unless the
request supplies the corresponding `scan.*` role field. Plans also resolve
descriptor-level `lsm_sample_clock_source` and `lsm_start_trigger_source`
defaults unless the request supplies `scan.*` or `timing.*` route fields. It is
a configured mapping only: no task is created, no buffer is written, no route is
validated, and no input samples are read.

## Known Linux Runtime Boundary

The safe runtime probe currently calls only NI-DAQmx version functions and
returns `detected_runtime_version = "26.3.1"` with major `26`, minor `3`, and
update `1` on this host.

`DAQmxGetSysDevNames` is not called from the Linux runtime driver because the
current host can abort in `libnipalu.so` when NI-PAL is not initialized. The
standalone `numanager-daqmx-inventory-helper` binary exists for bring-up, but
runtime-integrated inventory remains disabled until a safe isolation/readiness
strategy is validated.

## Hardware Validation Required

Before changing support status beyond runtime probing, record bench notes for:

- DAQ device identity and physical channel inventory;
- successful create/start/stop/clear for one no-output task where safe;
- successful AO/DO/AI/CI/CO channel setup and clear with the standalone channel
  helper, before any task is started;
- AO voltage output read back with an external meter or loopback using the
  gated I/O smoke helper;
- DO TTL output observed electrically or through loopback using the gated I/O
  smoke helper;
- AI voltage readback from a known source or loopback using the gated I/O smoke
  helper;
- CI counter readback from a known pulse source using the gated I/O smoke
  helper;
- CO pulse train frequency/count observed electrically or through counter input
  using the gated I/O smoke helper;
- finite buffered scan ordering and trigger routing;
- timeout and error text behavior;
- safe cleanup after partial setup failure, start failure, read timeout,
  I/O-smoke execution failure, and user stop. Lifecycle helper failures after a
  successful start should record `cleanup_after_lifecycle_error` and
  `stopped_task_after_error` rows before clear.

Without those notes, live task execution is not implemented.
Use
[`ni-daqmx-bench-validation-checklist.md`](ni-daqmx-bench-validation-checklist.md)
as the DAQmx-specific validation note skeleton.
