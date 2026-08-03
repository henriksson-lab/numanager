# NI-DAQmx SDK Evidence Template

Use this note when adding a user-provided NI-DAQmx SDK/runtime package for the
optional ImSwitch DAQmx backend.

## Package Identity

| Item | Value |
| --- | --- |
| Runtime package | |
| Runtime version | |
| Platform | |
| Installation layout | |
| License / redistribution boundary | |
| Supported SDK target | Linux or Windows; other targets require separate NI SDK/runtime evidence |
| External-gates audit command | `scripts/audit-ni-daqmx-external-gates.sh` |
| External-gates audit output | |
| Package input inventory command | `scripts/audit-ni-daqmx-package-inputs.sh <installer-file-or-directory>` |
| Package input inventory output | |
| Header root or archive | |
| Header inventory SHA-256 | |
| Header inventory NIDAQmx.h count | |
| Header inventory NIDAQmx.h path | |
| Header inventory command | `scripts/audit-ni-daqmx-sdk-headers.sh <header-file-or-directory>` |
| Installed target-platform NIDAQmx.h used for bindgen | |
| Bindgen regeneration command | |
| Target-scope audit command | `scripts/audit-ni-daqmx-target-scope.sh` |
| Target-scope audit output | |
| No-hardware helper audit command | `scripts/audit-ni-daqmx-no-hardware-helpers.sh` |
| No-hardware helper audit output | |
| Plan-validation audit command | `scripts/audit-ni-daqmx-plan-validation.sh` |
| Plan-validation audit output | |
| Live-gate audit command | `scripts/audit-ni-daqmx-live-gate.sh` |
| Live-gate audit output | |
| Runtime-probe audit command | `scripts/audit-ni-daqmx-runtime-probe.sh` |
| Runtime-probe audit output | |
| Example-output sync audit command | `scripts/audit-ni-daqmx-example-output-sync.sh` |
| Example-output sync audit output | |

## Package Input Inventory

Paste the output of:

```sh
scripts/audit-ni-daqmx-package-inputs.sh <installer-file-or-directory>
scripts/audit-ni-daqmx-sdk-headers.sh <header-file-or-directory>
scripts/audit-ni-daqmx-sys-source.sh <ni-daqmx-sys-repo>
```

These record intake and source markers only; they do not load the NI-DAQmx
runtime, create NI-DAQmx tasks, write outputs, read inputs, execute scans,
establish redistribution permission, or provide hardware evidence.

## External Gates Audit

Paste the output of:

```sh
scripts/audit-ni-daqmx-external-gates.sh
```

The audit checks that license/legal review, installed package/header review,
NI-PAL/device inventory, bench safety preconditions, runtime publication, and
live task execution remain explicit external gates rather than implied support.
It does not complete legal review, audit installed Windows headers, initialize
NI-PAL, approve bench wiring/safety, create NI-DAQmx tasks, or provide hardware
validation evidence.

Paste the output of:

```sh
scripts/audit-ni-daqmx-package-inputs.sh <installer-file-or-directory>
```

The inventory records installer/package file identity, archive entries,
Debian/RPM metadata where supported by local tooling, and embedded
package/license file identities where extractable. When `7z` is available, it
also records Windows online-installer PE and first-level payload inventory. It
does not prove legal redistribution permission, installed header contents,
runtime loading, or task behavior.

## Header Inventory

Paste the output of:

```sh
scripts/audit-ni-daqmx-sdk-headers.sh <header-file-or-directory>
```

The inventory records discovered `NIDAQmx.h` count/path, header identity,
title/copyright banner, required symbols, runtime-version property/getter
symbols, and whether a literal package-version macro exists. It does not prove
runtime behavior, and package/runtime version claims still need package-intake
and runtime-probe evidence. The audit exits non-zero when the supplied
file/directory does not contain `NIDAQmx.h`.
Before regenerated 26.5 bindings are accepted, record the exact installed
target-platform `NIDAQmx.h` path used for bindgen and the bindgen regeneration
command. The FFI-source audit must then come from that regenerated source state;
do not mix Linux-generated bindings with Windows ABI claims or installer-input
archives with installed-header claims.

## FFI Source Inventory

Paste the output of:

```sh
scripts/audit-ni-daqmx-sys-source.sh <ni-daqmx-sys-repo>
```

The inventory records fork revision, worktree state, package metadata, bindgen
inputs, generated-source hashes, platform link cfgs, and required symbol
availability, including runtime-version bindings. Fork-local runtime smoke tests
are runtime probes only; they are not evidence for numanager task ordering,
routing, cleanup, or hardware behavior. Record a separate FFI source inventory
for each target-platform header used to publish bindings. Do not infer Windows
support from Linux-generated bindings, and do not treat macOS as supported
without NI SDK/runtime evidence plus target-specific link and bindgen audits.

## Target-Scope Audit

Paste the output of:

```sh
scripts/audit-ni-daqmx-target-scope.sh
```

The audit records numanager Cargo dependency target scope, SDK-feature helper
gating, helper unsupported-target stubs, wrapper/implementation FFI boundaries,
and readiness target-support metadata. It does not prove Windows ABI
compatibility, runtime installation, NI task behavior, or hardware behavior.

## No-Hardware Helper Audit

Paste the output of:

```sh
scripts/audit-ni-daqmx-no-hardware-helpers.sh
```

The audit builds the SDK-feature helper binaries and runs only helper dry-run,
preflight-only, simulated-cleanup, and invalid-input guard paths. It checks that
those outputs keep `execute=false`, `created_task=false`,
`preflight_only=true`, `wrote_output=false`, and `read_input=false` where
applicable. It does not execute NI-DAQmx tasks, write outputs, read inputs, or
provide hardware evidence.

## Plan-Validation Audit

Paste the output of:

```sh
scripts/audit-ni-daqmx-plan-validation.sh
```

The audit runs the public `lsm_daqmx_plan_validation` example and checks that
valid configured raster/signal plans keep setup/preflight helper commands
runnable, while invalid role/channel plans suppress those helpers. It keeps
`execution_gate: not_live_task_execution` and does not create NI-DAQmx tasks,
write outputs, read inputs, execute scans, or provide hardware evidence.

## Live-Gate Audit

Paste the output of:

```sh
scripts/audit-ni-daqmx-live-gate.sh
```

The audit sets `NUMANAGER_DAQMX_LIVE_TASK_EXECUTION=1` for public configured
ImSwitch capture, stream, signal, and GUI smoke paths. It verifies that those
paths record live-task intent but remain `live_task_execution_ready=false`,
`execution=not_live_task_execution`, and frame/chunk-free. It does not execute
NI-DAQmx tasks, write outputs, read inputs, publish hardware frames, or provide
hardware evidence.

## Runtime-Probe Audit

Paste the output of:

```sh
scripts/audit-ni-daqmx-runtime-probe.sh
```

The audit runs public `daqmx_runtime_probe` workflows through the optional
SDK feature. Config-only paths must avoid vendor-runtime loading while still
reporting package/header metadata readiness, and the process-isolated helper
path must keep the runtime process in `runtime_probe_only` with
`live_task_execution_ready=false`, even when the helper reports a contained
runtime-version failure. It does not create NI-DAQmx tasks, write outputs, read
inputs, execute scans, or provide hardware evidence.

## Example-Output Sync Audit

Paste the output of:

```sh
scripts/audit-ni-daqmx-example-output-sync.sh
```

The audit runs public DAQmx bring-up and validation-note scaffold examples and
checks that recorded example output still contains their emitted audit commands
and required scaffold sections. It does not create NI-DAQmx tasks, write
outputs, read inputs, execute scans, or provide hardware evidence.

## API Audit Checklist

| Area | Header/API evidence | Notes |
| --- | --- | --- |
| Task lifecycle | | `CreateTask`, start, wait, stop, clear, lifetime ownership |
| AO voltage channels | | Channel creation, range units, buffered writes |
| DO lines | | Line grouping, boolean packing, buffered writes |
| AI voltage channels | | Channel creation, range units, sample reads |
| CI counting | | Edge source, count type, sample reads |
| CO pulse/sample clock | | Frequency/tick modes, finite/continuous timing |
| Sample-clock timing | | Source, rate, edge, finite sample count |
| Start/reference triggers | | Digital trigger source syntax and edge handling |
| Runtime version metadata | | Header property IDs, getter symbols, and runtime-probe output |
| Error handling | | Error-code convention, extended error text, timeout behavior |
| Safe stop/cleanup | | Stop/clear behavior after partial setup or runtime failure |

## Runtime Mapping Plan

| numanager API | NI-DAQmx tasks | Required validation |
| --- | --- | --- |
| `ConfocalImageCapture` | AO/DO output tasks plus CI/AI input tasks | Finite task ordering, trigger routing, final frame publication |
| `ConfocalImageStream` | AO/DO output tasks plus CI/AI input tasks | Incremental frame/region publication and stop behavior |
| `ScanSignalStream` | CI/AI input tasks with timing source | Chunk size, first sample index, sample rate, timeout behavior |

Keep live task execution disabled until API audit and hardware validation are
complete. `property.connect = true` may be used only for runtime probing behind
the `ni-daqmx-sdk` feature.

## Hardware Validation

Record bench results using
[`hardware-validation-template.md`](hardware-validation-template.md). Include:

- device identity and serial number;
- NI-DAQmx runtime version and platform;
- physical channel mapping;
- observed task start/stop/clear behavior;
- trigger routing and sample-clock source;
- AO/DO output validation;
- AI/CI input validation;
- error and timeout behavior;
- safe stop/cleanup behavior.

## Config Fields

Once the package identity is recorded, mirror it into the configured
`imswitch_daqmx` hub:

```toml
property.runtime_package = "<package name>"
property.runtime_version = "<package version>"
property.runtime_platform = "<platform>"
property.runtime_license = "<license boundary>"
property.sdk_header_path = "<header path or archive>"
property.sdk_header_sha256 = "<header inventory or package digest>"
property.connect = false
```

Keep `property.connect = false` until API audit and hardware validation are
complete and the live backend implementation is ready.
