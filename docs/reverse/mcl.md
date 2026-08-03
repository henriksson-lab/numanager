# MCL MicroDrive/NanoDrive Protocol Evidence Note

## Status

| Field | Value |
| --- | --- |
| Plan target | Mad City Labs MicroDrive and NanoDrive |
| Current state | Driver exposes configured raw encoder/status descriptors, opt-in MicroDrive USB raw readback, and vendor firmware/runtime package identity/file-status metadata. Full static protocol spec exists in [`mcl-protocol.md`](mcl-protocol.md); no captured trace or validation note |
| Better source status | Reverse engineered evidence recovered the USB transport, endpoint map, VID/PID tables, vendor-request codes for both families, encoder wire format, and error mapping |
| Next evidence | USB traces from real hardware for payload field semantics, units, and completion/limit behavior |
| Evidence type | Reverse engineered |
| Feasibility | Strong for transport and command identity; typed motion needs motion-safety semantics (status bits, completion, scaling) |

## Protocol Evidence Summary

| Area | Finding |
| --- | --- |
| Evidence inventory | Reverse engineered; see [`artifact-inspection-summary.md`](artifact-inspection-summary.md) |
| MicroDrive API evidence | Public declarations expose library init, handle acquisition, move status/wait/status, stop, three-axis moves, single-axis moves, single-step moves, encoder reset/read/current position, controller information, axis info, home/mode, serial/product/firmware identity, and close/release calls |
| NanoDrive API evidence | Public declarations expose library init, handle acquisition, single-axis read/write, calibration, serial/product identity, device-attached status, commanded position, sequence load/start/stop, and sequence capacity calls |
| Evidence type | Reverse engineered |
| Transport | libusb 1.0.21, statically linked. No kernel IOCTL layer. Two wire functions total: `RWControlPipe` (raw 8-byte USB setup packet) and `RWNAxisPipe` (bulk, per-axis endpoints `0x0N`/`0x8N`, global pair `0x02`/`0x86`) |
| Identity | VID `0x1569`; 10 MicroDrive PIDs, 18 NanoDrive PIDs plus two Cypress pre-firmware IDs (`0547:8613`, `04B4:2235`) |
| Command identity | Vendor-request numbers recovered for both families and tied to the public API each backs; see [`mcl-protocol.md`](mcl-protocol.md) §3 and §5. The two families' request spaces overlap numerically but are incompatible |
| Data formats | Encoder payload is 8 × signed 24-bit little-endian counters (24 bytes); status word carries two bits per axis with a per-model axis-presence mask |
| Missing wire evidence | Status-bit meaning, move payload field packing/units, completion semantics, encoder scaling constants, homing/limit behavior, NanoDrive DAC/ADC sample encoding, and the exact pre-firmware loader sequence |

## Evidence To Collect

| Evidence | Required observations |
| --- | --- |
| Evidence inventory | Done; still need package variants and sample programs |
| API surface | Done at header/API level for initialization, handles, identity, calibration, move/stop/status, encoder/readback, and sequence calls |
| Reverse engineered note | Done. VID/PID tables, endpoint map, vendor-request numbers, encoder format, status layout, and error mapping are recovered in `mcl-protocol.md` |
| USB trace | Still required: status-bit meaning, move payload fields/units, completion polling, encoder scaling, homing/limits, NanoDrive DAC/ADC encoding |
| Hardware note | Model, firmware, axis count, calibration, limits, controller mode |

## Protocol Questions

| Area | Questions |
| --- | --- |
| Transport | Resolved: USB vendor control transfers plus per-axis bulk endpoints, over libusb. No kernel IOCTL layer |
| Units | Open. Encoder counts are recovered as signed 24-bit values, but no counts-to-micron scaling constants have been located |
| Motion | Request numbers resolved (`0xD0` move profile, `0xC9` stop, `0xCE`/`0xD8` move variables/params). Payload field packing still open |
| Completion | Open. `0xCF`/`0xD2` are the poll requests; level-vs-edge, cadence, and interrupted-move reporting are untraced |
| Safety | Open. Status carries two bits per axis whose meaning is inferred, not evidenced |
| Families | Resolved: same transport and same two wire functions, but **incompatible request spaces** — dispatch must key on PID |

## Candidate Public Surface

| Device | Capabilities | Properties |
| --- | --- | --- |
| MCL hub | configured discovery/evidence summary plus opt-in raw USB readback | `model`, `serial_number`, `family`, `axis_count`, `vendor_id`, `product_id`, `connected`, `raw_status`, `encoder_summary`, `vendor_runtime_state`, `firmware_package_state`, package path/status/size metadata |
| Axis/stage | none currently advertised | `raw_encoder_count`, `status_bits`, `position_gate`, `motion_gate`; position/motion properties require hardware traces |

Use typed values: `Position`, `Velocity`, `Acceleration`, `Bool`, and `String`.
Only expose homing/origin behavior if it is documented or traced.

## Stop/Proceed Decision

| Decision | Condition |
| --- | --- |
| Proceed to SDK-free spec | Done. Transport, identity, and command identity are recovered in `mcl-protocol.md` |
| Optional SDK package | Not needed for the current reverse engineered reverse engineered support |
| Evidence policy | USB traces are still needed for payload fields, status-bit meaning, units/scaling, and completion. Motion/control paths fail closed when those facts are not known |

## Implementation Gate

`numanager_drivers::mcl` has an explicitly descriptor/read-only support
for configured raw encoder/status evidence, with opt-in MicroDrive USB refresh
when `connect=true` and `os-usb` are enabled. Move, stop, position units,
status meanings, completion behavior, and firmware loading are not exposed
when they are not evidenced. Vendor firmware/runtime packages may be
configured as third-party excluded data for later on-demand use, but package
presence alone does not imply loader support.
