# MCL MicroDrive/NanoDrive Protocol Evidence Note

## Status

| Field | Value |
| --- | --- |
| Target | Mad City Labs MicroDrive and NanoDrive |
| Evidence class | Reverse-engineered protocol evidence, plus manufacturer documentation for the published `MCL_*` error enum |
| Recovered | USB transport, endpoint map, VID/PID tables, vendor-request codes for both families, encoder wire format, error mapping — full spec in [`mcl-protocol.md`](mcl-protocol.md) |
| Hardware validation | **None.** No captured traffic from a physical device |
| Next evidence needed | Traces from real hardware for payload field semantics, units/scaling, and completion/limit behavior |
| Feasibility | Strong for transport and command identity; typed motion blocked on motion-safety semantics |

## Protocol Evidence Summary

| Area | Finding |
| --- | --- |
| Transport | Plain USB — vendor control transfers (raw 8-byte setup packet) plus bulk endpoints. No kernel IOCTL layer. Per-axis endpoints `0x0N`/`0x8N`; device-global pair `0x02`/`0x86` |
| Identity | VID `0x1569`; 10 MicroDrive PIDs, 18 NanoDrive PIDs, plus two Cypress pre-firmware IDs (`0547:8613`, `04B4:2235`) |
| Command identity | Vendor-request numbers recovered for both families — see [`mcl-protocol.md`](mcl-protocol.md) §3 and §5. The two request spaces overlap numerically but are **incompatible**; dispatch must key on PID |
| Data formats | Encoder payload is 8 × signed 24-bit little-endian counters (24 bytes). Status word carries two bits per axis with a per-model axis-presence mask |
| Device capability surface | Library init, handle acquisition, identity (serial/product/firmware), axis info, move/stop/status/wait, single- and three-axis moves, single-step moves, encoder reset/read/position, home/mode; NanoDrive adds single-axis read/write, calibration, commanded position, and sequence load/start/stop/capacity |
| **Missing wire evidence** | Status-bit meaning, move payload field packing and units, completion semantics, encoder scaling constants, homing/limit behavior, NanoDrive DAC/ADC sample encoding, pre-firmware loader sequence |

## Evidence To Collect

| Item | Why |
| --- | --- |
| Captured traffic from a physical device for a move, a stop, and a completion poll | Fixes payload field packing, completion semantics, and interrupted-move reporting |
| A bench run correlating encoder counts with measured displacement | Supplies the counts-to-micron scaling |
| Status word observed across known axis states | Confirms the two-bits-per-axis meaning, which is currently inferred |
| Pre-firmware enumeration captured from a cold plug | Supplies the loader sequence for the Cypress IDs |

## Protocol Questions

| Area | State |
| --- | --- |
| Transport | **Resolved** — USB vendor control transfers plus per-axis bulk endpoints |
| Families | **Resolved** — same transport, incompatible request spaces |
| Motion | Request numbers resolved (`0xD0` move profile, `0xC9` stop, `0xCE`/`0xD8` move variables/params). Payload field packing **open** |
| Units | **Open** — encoder counts are signed 24-bit; no counts-to-micron scaling known |
| Completion | **Open** — `0xCF`/`0xD2` are the poll requests; level-vs-edge, cadence, and interrupted-move reporting untraced |
| Safety | **Open** — the two status bits per axis are inferred, not evidenced |

## Candidate Public Surface

| Device | Capabilities | Properties |
| --- | --- | --- |
| MCL hub | discovery/evidence summary, opt-in raw USB readback | `model`, `serial_number`, `family`, `axis_count`, `vendor_id`, `product_id`, `connected`, `raw_status`, `encoder_summary` |
| Axis/stage | none currently advertised | `raw_encoder_count`, `status_bits`, `position_gate`, `motion_gate` — position/motion properties require hardware traces |

Use typed values: `Position`, `Velocity`, `Acceleration`, `Bool`, `String`.
Expose homing/origin behavior only once it is documented or traced.

## Stop/Proceed Decision

**Proceed** for descriptor, identity and read-only encoder/status surface: the
transport, endpoint map, identity tables and command identity are established.

**Stop** for motion and any typed position or velocity control until hardware
traces supply payload field packing, units, completion semantics and status-bit
meaning. Motion carries a physical-safety cost if any of those are guessed, so
inference is not an acceptable substitute here.

## Implementation Gate

`numanager_drivers::mcl` is descriptor/read-only: configured raw encoder/status
evidence, with opt-in MicroDrive USB refresh when `connect=true` and `os-usb` are
enabled. Move, stop, position units, status meanings, completion behavior, and
firmware loading are **not exposed** because they are not evidenced. Motion and
control paths fail closed until hardware traces supply those facts.
