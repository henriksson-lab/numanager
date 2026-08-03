# Core Model

`numanager` presents hardware as a graph of devices and capabilities. A single
physical controller can expose several logical devices, such as a hub, XY stage,
Z stage, light source, filter wheel, or autofocus provider. Applications should
work through typed properties and capabilities instead of protocol packets.

## Main Concepts

| Concept | Role |
| --- | --- |
| `DeviceGraph` | Resources, hubs, logical devices, services, and dependency edges |
| `DeviceDescriptor` | Device labels, kind tags, metadata, and typed properties |
| `CapabilityDescriptor` | Runtime/driver-advertised operations, request kind, and response shape |
| `Value` | Physical quantities such as position, temperature, wavelength, current, pressure, and time |
| `CapabilityKind` | Typed operations such as capture, stream, stage move, trigger, DAC, ADC, autofocus, confocal image capture/stream, raw scan-signal stream, valve/filter selection, gas control, and imaging-head control |
| `TimingPlan` | Checked timing-plan representation for routed, sequenced, and triggered operations |
| `Command` | Property reads/writes, capability invocation, timing-plan control, and multi-device state sets |
| `LocalRuntime` | Driver workers, validation, request execution, operation handles, completion, and event delivery |
| `DriverCandidate` | Two-stage discovery result that a UI or config can claim before adding a driver |
| `CapabilityProvider` | Query result for devices that expose a capability and their graph dependencies by role |

## API Rules

Drivers own protocol translation and can coalesce logical device requests into
shared physical transactions. Applications should use typed properties, state
sets, and typed capability requests rather than protocol packets.

Public property keys are canonical `snake_case` names without unit suffixes when
the value type carries the unit, such as `exposure`, `frame_interval`,
`sensor_temperature`, `wavelength`, and `position`. Public string/enum values
use canonical Rust-style names such as `Mono8`, `Raw8`, `Rgb8`, and `Native`;
native protocol spellings may be accepted as aliases or recorded in metadata.

Public values should use typed quantities instead of naked scalars when units or
domain semantics matter, including `TimeInterval`, `Frequency`, `Decibel`,
`PixelCount`, `Ratio`, and `NumericalAperture`.

For ordinary typed operations, applications can submit the request itself with
`submit_request()` or `execute_request()` and let the runtime infer the
capability kind. Explicit `CapabilityKind` submission remains available for
no-request operations such as homing/stopping and for ambiguous trigger or
diagnostic bring-up commands. Opaque capability IDs remain runtime handles, not
user-facing API choices.

Diagnostic raw/generic command surfaces may exist for bring-up, but they are not
ordinary application workflows and are hidden from generic examples. Firmware
upload/init, bootloader, reset, factory/default restore, flash/DFU, FPGA or
bitstream loading, EEPROM/nonvolatile-memory writes, persistent user-set/config
saves, origin-zeroing, and similar maintenance operations must not appear in
regular or advanced command lists. Runtime metadata filters hidden maintenance
capabilities, and `GenericCommandRequest::is_hidden_maintenance()` exposes the
same classifier for UI-side filtering before submission.
`CapabilityDescriptor::exposure()` marks regular user capabilities,
driver-validated advanced diagnostic aliases, and hidden maintenance
capabilities. Advanced UIs may render `AdvancedDiagnostic` aliases after their
driver-specific allowlist is inspected, but must never render
`HiddenMaintenance`. Diagnostic `GenericCommand`, raw-register, and custom
command capabilities should be rendered as driver-validated aliases only, not as
free-form protocol consoles;
`CapabilityDescriptor::requires_driver_validated_command_aliases()` marks those
surfaces.

Discovery is two-stage, drivers can be added/removed at runtime, completion is
reported by the driver or hardware path, and high-throughput cameras publish
frame handles backed by the runtime frame store/ring buffer.

Laser-scanning/confocal devices should not pretend that counter-based
acquisition is a normal camera exposure. Use `ConfocalImageCapture` for a final
reconstructed image or stack, `ConfocalImageStream` for live reconstructed
dirty-region or mutable-frame updates, and `ScanSignalStream` for timed raw
sample chunks when the application owns reconstruction. Runtime producers should
publish dirty-region image updates as `FrameReady` events whose stored frame
metadata carries `update_policy`, `overwrite_previous_pixels`, and optional
`dirty_x`, `dirty_y`, `dirty_width`, and `dirty_height` pixel counts. Raw signal
chunks should be published as `ScanSignalChunk` events with stream, channel,
timing-origin, sample-index, chunk size, sample-rate, sample-period, and
channel-sample payloads. Long-running stream operations may report status through
`OperationChanged` events with `OperationStatus::Running { progress }`; finite
streams use `completed/total`, while open-ended streams use `total = 0` and
increment `completed` as a frame or chunk counter.

Composed services such as autofocus can be discovered with
`capability_providers()` and inspected by dependency role instead of by raw graph
node IDs.
