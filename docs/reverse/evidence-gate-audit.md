# Protocol Evidence Gate Audit

This audit records protocol targets where the implemented surface is defined by
available trace, bench, manufacturer, standard, or open-source evidence.

## Acceptance State

An evidence-limited target is acceptable when the repo states the exact missing
evidence and exposes no unsupported hardware behavior as a working capability.

| Target | Required artifacts | Driver source/export check | Current disposition | Next evidence gate |
| --- | --- | --- | --- | --- |
| Okolab | `docs/reverse/okolab.md`, `docs/devices/okolab.md`, evidence register row, shipped third-party command database note | `numanager_drivers::okolab` is exported | Reverse engineered serial/configured runtime support with connected read/write; hardware-complete claims need traces | Serial frame grammar/checksum confirmation, ACK/status/fault replies, discovery/readback, and one safe setpoint trace with runtime command output/event plus hardware output/readback |
| Agilent/Keysight Laser Combiner | `docs/reverse/agilent-laser-combiner.md`, `docs/reverse/agilent-laser-combiner-protocol.md`, `docs/devices/agilent-laser-combiner.md`, evidence register row | `numanager_drivers::agilent_laser_combiner` is exported | External-evidence implementation with typed control/readback paths | Real-board handshake, laser-line discovery, disable trace with runtime command output/event plus hardware output/readback, reply-latency data, and a resolution for the protocol's absent interlock/fault status |
| Mightex Sirius BLS/SLC | `docs/reverse/mightex.md`, `docs/devices/mightex-bls.md`, evidence register row, generic `light_source` workflow output, discovery output in `docs/example_outputs.md` | `numanager_drivers::mightex_bls` is exported | Reverse engineered HID output support; unit/safety/completion validation is not recorded | HID traces for completion/error vocabulary, calibrated scaling, hardware-safe ranges, fault states, trigger timing, and observed low-output/readback/disable behavior |
| Mightex buffered USB cameras | `docs/reverse/mightex.md`, `docs/devices/mightex-camera.md`, evidence register row, third-party runtime package note | `numanager_drivers::mightex_camera` is exported | Runtime-package evidence with file-status/digest/loadability/ABI-symbol checks, writable capture parameters, opt-in vendor-runtime `Mono16`/`Raw16` capture, and repeated one-shot stream support; native frame transport, native continuous streaming, native gain/color controls, ROI/binning beyond configured frame dimensions, and broader SDK-free acquisition behavior are not exposed because native protocol evidence is absent | Hardware validation of runtime capture/stream behavior, platform-camera route proof if applicable, or USB control/frame traces covering native frame layout, completion, buffer ownership, trigger behavior, dropped-frame semantics, and matching frame-handle/stream output |
| MCL MicroDrive/NanoDrive | `docs/reverse/mcl.md`, `docs/devices/mcl.md`, evidence register row | `numanager_drivers::mcl` is exported | Active USB descriptor discovery plus opt-in MicroDrive raw encoder/status readback, fixed-length raw MicroDrive control-read/action commands, and firmware/runtime package checks; stage moves require units, status meanings, and completion behavior evidence | Live USB validation of endpoint/interface behavior, units/calibration, move payload fields, status-bit meanings, busy/fault completion evidence, and runtime command output/event plus hardware position/readback |
| ABS legacy USB cameras | `docs/reverse/abs-camera.md`, `docs/devices/abs-camera.md`, evidence register row, third-party runtime package note | `numanager_drivers::abs_camera` is exported | Runtime-package evidence with file-status/digest/loadability/ABI-symbol checks, writable exposure setting, explicit async software trigger, opt-in vendor-runtime capture, and repeated one-shot stream support; native transport, native continuous streaming, gain controls, persistent trigger modes, and broader acquisition behavior is not exposed because USB protocol evidence is absent | Exact hardware identity, platform route check, runtime capture/stream validation, USB control/frame traces, throughput, buffer ownership, abort/error behavior, and matching frame-handle/stream output |

## Current Evidence Policy

| Area | Decision |
| --- | --- |
| Evidence-backed protocol targets | Expose hardware operations only when the linked device page identifies protocol evidence for command payloads, units, completion, and fault behavior |
| Mightex BLS/SLC | The currently identified Mightex BLS/SLC HID command/readback surface has been implemented for bring-up; do not add public fault/status/completion/timing/calibrated-unit behavior from software status defaults |
| Examples | Keep examples generic; the Mightex path remains an opt-in branch of the generic `light_source` workflow and must print requested output, hold duration, runtime completion/readback, and disable result |
| Recorded output | `docs/example_outputs.md` must include the current discovery count and the public candidate blocks for newly added drivers so the visible add/detect flow can be audited without reading driver internals |
| Tests | Do not generate hardware-driver tests. Protocol assertions belong in evidence notes, trace notes, or hardware-validation records unless a specific externally evidenced validation workflow explicitly requires executable checks |
| Simulations | Do not add standalone device simulations for these targets. Simulation work is postponed unless coupled to a biological-system model |

## Verification Commands

Use these checks after editing this track:

```sh
git diff --check
bash scripts/audit-reverse-evidence-boundary.sh
CARGO_TARGET_DIR=/tmp/numanager-target cargo check -p numanager-drivers --features os-hid
CARGO_TARGET_DIR=/tmp/numanager-target cargo check -p numanager-examples --features os-hid
```

These commands verify formatting hygiene, required evidence artifact presence,
target device-page and reverse-note section coverage, central evidence-register
coverage, root README and device-index coverage, absence of reverse-evidence
driver test artifacts, and that the exported reverse-evidence implementations
plus generic examples still compile. They do not validate hardware behavior.
