# Hardware Validation Note Template

Use this template when moving a driver feature from fixture/configured support
to hardware-validated support. A validation note may live in a device page, a
linked local lab note, or a captured-trace record, but it must contain enough
detail for another developer to audit the claim without guessing.

For raw protocol captures, first create or link a curated trace note using
[`../reverse/trace-note-template.md`](../reverse/trace-note-template.md). The
validation note should then point to the trace note and record the hardware
claim being promoted.

Every promoted operation needs its user-facing output captured as evidence:
stdout/stderr, runtime events, or API-call result logs. Without that output, the
trace may show hardware traffic, but it does not prove what numanager reported
to the caller.

Do not use this template to justify self-authored fixtures. If the observation
does not come from real hardware, captured traffic, manufacturer documentation,
a public standard, open firmware, or audited open SDK/header source, keep the
behavior marked as unknown or pending validation.

## Run Identity

| Field | Value |
| --- | --- |
| Driver module |  |
| Device page |  |
| Hardware model |  |
| Serial number or asset tag |  |
| Firmware/software version |  |
| Transport | USB/serial/TCP/CAN/other |
| Host OS and relevant driver stack |  |
| Date | YYYY-MM-DD |
| Operator |  |
| Config file or discovery record |  |

## Evidence Sources

| Source class | Reference | Covered behavior |
| --- | --- | --- |
| Manufacturer protocol / public standard / open firmware / audited SDK/header / hardware trace / bench run | URL, document revision, commit, local trace path, or lab-note path | Commands, properties, completion rules, safety states, frame metadata, or timing behavior |

## Setup And Safety

| Area | Observed or enforced behavior |
| --- | --- |
| Motion limits and homing state |  |
| Laser/light output limits and interlocks |  |
| Temperature, pressure, gas, or voltage limits |  |
| Emergency stop or safe shutdown |  |
| Fault injection or recovery tested |  |

## Commands And Properties

For output, motion, environmental control, and acquisition operations, include
both software/runtime output and hardware output/readback. A runtime completion
message alone does not validate the physical behavior, and a bench observation
alone does not validate the user-facing API result.

| Capability/property | Request or setpoint | Evidence expectation | Runtime command output/event | Hardware output/readback | Result | Notes |
| --- | --- | --- | --- | --- | --- | --- |
|  |  |  |  |  | Pass/Fail/Unknown |  |

## Completion And Events

| Operation | Hardware completion condition | Runtime completion/event | Hardware output/readback | Timeout or fault behavior | Result |
| --- | --- | --- | --- | --- | --- |
|  |  |  |  |  | Pass/Fail/Unknown |

## Camera Or Stream Validation

Use this section only for acquisition devices.

| Field | Observation |
| --- | --- |
| Pixel format and color encoding |  |
| Frame dimensions and stride |  |
| Exposure/gain/binning/ROI |  |
| Transport mode |  |
| Frames captured and target rate |  |
| Ring capacity and overflow policy |  |
| Dropped frame counters |  |
| Frame metadata keys |  |
| Trigger/timestamp behavior |  |

## Remaining Uncertainty

| Behavior | Uncertainty | Evidence needed before support claim |
| --- | --- | --- |
|  |  |  |

## Update Checklist

| Evidence item | Required update |
| --- | --- |
| Device page | Update validation status and affected capability/property rows |
| Evidence register | Update evidence basis, expansion status, and missing evidence |
| Implementation plan | Remove or narrow the corresponding remaining-work item |
| Trace/log storage | Link the capture or lab note according to repository policy |
| Tests | Do not generate hardware-driver tests; link only explicitly requested validation checks |
