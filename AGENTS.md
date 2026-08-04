# numanager Agent Rules

This repository is hardware-control software. Driver code must be treated as
engineering against real devices, not as a place to generate self-confirming
fixtures.

## Driver Tests

- Do not generate driver tests whose only evidence is code written in this
  repository.
- Do not add tests that prove an encoder matches a decoder, a scripted serial
  fixture returns scripted bytes, or a parser accepts examples invented for the
  test.
- Do not generate driver tests for hardware drivers. Driver behavior should be
  audited against sources and validated against real hardware, not maintained as
  self-confirming code.
- `scripts/audit-reverse-evidence-boundary.sh` enforces this for the consolidated
  `numanager-drivers` crate by rejecting inline test modules and driver-crate
  test files. If that audit fails, do not work around it by moving the same
  self-confirming checks elsewhere.
- Evidence for a command, reply, state transition, or unit conversion must be
  recorded in device pages, reverse notes, hardware validation notes, captured
  traces, bench logs, or source-audit notes and traced to a named source.
  Acceptable sources include, but are not limited to:
  - manufacturer documentation;
  - a public standard;
  - open firmware or audited open SDK/header source;
  - audited open adapter source, including Micro-Manager adapters, with adapter
    revision/source URL recorded;
  - vendor SDK/API documentation or headers;
  - reverse notes that identify observable wire/API behavior;
  - captured traffic from real hardware;
  - a documented bench run on real hardware.
- Implementation may proceed from any recorded source type. Hardware testing is
  a separate validation step: untested behavior must be labeled as implemented
  from source evidence and not hardware validated.
- If no recorded source exists for a behavior, do not write a fake test or
  fixture. Mark the behavior as unknown or source-evidence missing.
- Hardware validation notes should follow
  `docs/devices/hardware-validation-template.md` and include the hardware
  identity, firmware/software version, transport, observed completion/fault
  behavior, and remaining uncertainty.
- Micro-Manager source is acceptable implementation evidence when recorded with
  provenance and uncertainty. It is not, by itself, hardware validation or proof
  that behavior works on every device variant.
- The general interim solution whenever firmware, a loader, or a vendor runtime
  is required is to ship the original vendor package as third-party excluded
  data when redistribution terms permit it, or load a user-configured local copy
  when they do not, behind an optional backend until a project-owned firmware,
  loader, or open runtime replacement exists. Treat this as the default
  implementation path for every firmware-dependent device, not as a
  device-specific exception.
  Record the license boundary, redistribution status, file identity, upstream
  package/version, SHA-256 digest, and platform. Drivers should load or read the
  package only on demand through an explicit configured backend, and package
  presence alone must not imply behavior support. Do not block implementation
  solely because replacement firmware is not ready.

## Examples

- `crates/numanager-examples` is user-facing API documentation.
- Examples must use public runtime, device, property, capability, discovery, and
  timing APIs.
- Do not expose raw serial commands, packet construction, protocol modules,
  scripted serial replies, parser demos, or conformance fixtures in examples.

## Driver API Surface

- Low-level protocol modules are implementation details. Do not document them as
  user-facing APIs or use them from examples.
- If a protocol helper must remain reachable for driver construction or
  hardware bring-up, keep it hidden from generated docs and prefer adding a
  typed runtime capability/property before exposing a new protocol entry point.

## Naming

- Public property keys use `snake_case`.
- Do not encode units in public property keys when the value type or schema
  carries the unit. Use `exposure`, `frame_interval`, `sensor_temperature`,
  `wavelength`, `position`, and `power`, not `exposure_s`,
  `frame_interval_s`, `sensor_temperature_c`, `wavelength_nm`, `position_um`,
  or `power_mw`.
- Unit suffixes are acceptable only for diagnostic/protocol metadata that
  exposes a native wire format or configuration scalar.
- Public physical quantities use typed `Value` variants instead of naked
  integers/floats whenever the unit matters: `TimeInterval` for time,
  `Frequency` for frequency, `Decibel` for dB gain, `PixelCount` for image
  dimensions, and `Ratio` for fractions/percentages. Convert to protocol
  scalars only at the driver hardware boundary.
- Public enum/string choices use canonical Rust-style names such as `Mono8`,
  `Mono16`, `Raw8`, `Raw16`, `Rgb8`, `Bgr8`, and `Native`. Native spellings
  from standards or hardware protocols may be recorded in metadata or accepted
  as aliases, but should not be advertised as the normal user-facing choice.
