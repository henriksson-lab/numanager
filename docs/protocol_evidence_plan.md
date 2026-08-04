# Protocol Evidence Plan

This repository may contain hardware protocol specifications, but they must be
written as clean hardware-interface documents. Reverse engineered working notes,
tool output, implementation-internal details, and proprietary binary internals belong
outside this repository.

## Source Ladder

Prefer evidence in this order when sources conflict or when deciding how much
confidence to assign. Lower-ranked sources are still valid implementation
inputs when their provenance and uncertainty are recorded.

1. Manufacturer protocol manuals, application notes, and command references.
2. Public standards such as Modbus, GenICam, USB class specs, SCPI, or HID
   report descriptors.
3. Open firmware, open SDKs, public headers, or audited open-source adapters.
4. Audited open adapter source such as Micro-Manager device adapters, with
   revision and source URL recorded.
5. Vendor SDK/API documentation or headers.
6. Reverse notes that identify observable wire/API behavior.
7. Captured traffic from real hardware with matching runtime output.
8. Documented bench runs on real hardware.

## Clean-Room Spec Criteria

A protocol spec in this repository is acceptable when it:

- Describes observable wire behavior: transport settings, request and reply
  framing, command IDs, fields, units, status values, timing, and errors.
- Tags each claim with an evidence class from the source ladder.
- Can be implemented from the spec alone without consulting proprietary
  binaries or external tool output.
- Avoids copied or proprietary implementation structure, private function names, addresses,
  call graphs, local variable layouts, and binary tooling details.
- Marks unvalidated behavior as not hardware validated instead of presenting it
  as bench-tested.
- Separates public API design from protocol helpers; examples must use public
  runtime/device/capability/property APIs.

## Driver Gate

Driver code may be implemented from any recorded source type as long as the
source, confidence, and validation state are explicit. Default-supported driver
code requires enough external evidence to make device behavior auditable without
proprietary binary internals:

- discovery identity and transport setup;
- command and reply framing;
- readback for every advertised readable value;
- completion and fault behavior for actions;
- units, scaling, and limits for physical quantities;
- safe disable/stop behavior for motion, laser, light, temperature, pressure,
  and fluidic output devices;
- a validation state recorded in the device page or evidence register.

If those criteria are not met, implementation attempts are still allowed, but
the driver must expose unsupported/unknown behavior explicitly and must not
claim hardware validation or complete protocol coverage.

Hardware validation notes following
`docs/devices/hardware-validation-template.md` promote source-backed support to
bench-validated support. They are not required before writing the implementation
when another recorded source describes the behavior.
A hardware validation note is the promotion record, not the implementation
permission slip.

The general interim solution whenever firmware, a loader, or a vendor runtime
is required is to ship the original vendor package as third-party excluded data
when redistribution terms permit it, or load a user-configured local copy when
they do not, behind an optional backend until a project-owned firmware, loader,
or open runtime replacement exists. Treat this as the default path for every
firmware-dependent device, not a device-specific exception. The driver should
document the package identity, redistribution status, license boundary, and
remaining replacement work. Drivers should read or load those packages only on
demand through explicit configuration. The package requirement should not block
driver implementation.

Firmware upload, bootloader entry, reset, factory/default restore, flash/DFU,
and similar maintenance operations must not be exposed as regular
`GenericCommand` aliases or advanced UI commands. Initialization-only firmware
paths may run behind digest verification and explicit configuration. Read-only
firmware identity queries are allowed as diagnostic metadata. Runtime validation
rejects maintenance-looking `GenericCommand` names before driver dispatch; any
future intentional exposure must be outside the regular and advanced command
browser surfaces.

Audit anchors: default path for every firmware-dependent device; only on demand
through explicit configuration; maintenance operations must not be exposed as
regular GenericCommand aliases or advanced UI commands; rejects
maintenance-looking `GenericCommand` names before driver dispatch.
Audit exact: only on demand through explicit configuration.

## External Notes

When reverse engineered evidence was used historically, record only:

- artifact family and stable hash;
- the wire-level facts promoted into the clean spec;
- remaining validation gaps;
- the external note location or identifier when needed for audit continuity.

Do not commit analysis tools, raw dumps, implementation listings, or proprietary
binary artifacts.
