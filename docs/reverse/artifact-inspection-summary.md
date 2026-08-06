# Reverse Engineered Evidence Summary

One row per target whose interface facts come from reverse engineering rather
than from a manufacturer document, a public standard, or open source: what is
known, whether it is hardware validated, and what the driver may therefore
expose. Not sufficient on its own for a behavior claim — driver work uses the
linked protocol spec and fails unsupported operations explicitly.

## Evidence Matrix

| Target | Evidence | Interface facts recorded | Hardware validated | Implementation decision |
| --- | --- | --- | --- | --- |
| Okolab | Reverse engineered | Serial transport, CR framing, plain and checksum frame grammars, error vocabulary, retry behavior, command dictionary | No | Serial/configured support exists from [`okolab-protocol.md`](okolab-protocol.md). Hardware-support claims wait on traces validating ACK/status/fault replies, completion, readback, and safety behavior |
| Agilent Laser Combiner | Reverse engineered | COM port at 115200 8N1, serial identity scan, opcode table, counts→volts→mW conversion chain | No | Typed control/readback exists from [`agilent-laser-combiner-protocol.md`](agilent-laser-combiner-protocol.md). Traces are still needed for handshake, latency, and the absent interlock/fault surface |
| MCL MicroDrive | Reverse engineered | VID/PID table, control setup-packet layout, per-axis bulk endpoint map, vendor-request numbers, encoder wire format, status-word axis layout, error mapping | No | Descriptor discovery plus raw status/encoder/control reads. Typed motion is not exposed because payload field semantics, units/scaling, status-bit meaning, and move-completion evidence are absent |
| MCL NanoDrive | Reverse engineered | VID/PID table including pre-firmware IDs, shared USB transport, NanoDrive vendor-request set | No | As MicroDrive. The two families share a transport but have incompatible request spaces, so dispatch keys on PID |
| ABS camera | Reverse engineered | Camera operation and capture-mode surface only; no USB endpoint, control, or frame-transfer grammar | No | Writable one-shot exposure, explicit async software trigger, and opt-in capture through a user-configured vendor runtime. SDK-free native capture/stream is not exposed because platform-camera evidence and USB traces are absent |
| Mightex buffered camera SDK | Reverse engineered | Device/error operation surface and a dependency on a USB helper layer | No | Writable capture parameters plus opt-in one-shot capture and repeated one-shot stream through a user-configured vendor runtime. Native frame transfer/control is not exposed because traces are absent |
| Mightex USB helper | Reverse engineered | Windows USB/Cypress-style transport layer with bulk endpoints | No | Context for a future native transport only; insufficient to expose native frame acquisition or control writes without traces |

## Consequence

Mightex BLS/SLC, Agilent Laser Combiner and Okolab have enough reverse
engineered protocol evidence for SDK-free drivers. The remaining rows improve the
evidence map but do not unlock default MCL, ABS, or Mightex camera drivers.

Analysis records, proprietary materials, and raw captures live outside this
repository.
