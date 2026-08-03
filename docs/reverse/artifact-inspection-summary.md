# Reverse Engineered Evidence Summary

This file records coarse evidence status, promoted wire-level facts, and
implementation decisions for targets with reverse engineered evidence. Raw
analysis reports, proprietary materials, exact artifact identities, hashes, and
tooling live outside this repository.

This table is not sufficient by itself for complete behavior claims. Driver
work should use the linked clean protocol spec, implement only evidenced
behavior, and fail unsupported operations explicitly.

## Evidence Matrix

| Target | Evidence | Promoted wire-level facts | Implementation decision |
| --- | --- | --- | --- |
| Okolab | Reverse engineered | Serial transport, CR framing, plain/checksum frames, error vocabulary, retry behavior, and shipped command dictionary. | Serial/configured runtime support exists from `okolab-protocol.md`; hardware-support claims wait for real hardware traces that validate ACK/status/fault replies, completion, output/readback, and safety behavior. |
| Agilent Laser Combiner | Reverse engineered | COM-port transport at 115200 8N1, serial identity scan, opcode table, and counts-to-volts-to-mW conversion chain. | Documented in `agilent-laser-combiner-protocol.md`; typed control/readback support exists, while hardware traces are still needed for handshake, latency, and absent interlock/fault behavior. |
| MCL MicroDrive | Reverse engineered | VID/PID table, control setup-packet layout, per-axis bulk endpoint map, vendor-request numbers, encoder wire format, status-word axis layout, and error mapping. | Descriptor discovery and raw MicroDrive status/encoder/control-read support exist. Typed motion is not exposed because payload field semantics, units/scaling, status-bit meaning, and move completion evidence is absent. |
| MCL NanoDrive | Reverse engineered | VID/PID table including pre-firmware IDs, shared USB transport, and NanoDrive vendor-request set. | Same as MicroDrive. The two families share transport but have incompatible request spaces, so dispatch must key on PID. |
| ABS camera | Reverse engineered | Camera API surface exists, but no USB endpoint/control/frame-transfer grammar is recorded. | Runtime-package metadata, writable one-shot exposure, explicit async software trigger, and opt-in vendor-runtime one-shot capture exist; SDK-free native capture/stream is not exposed because UVC/platform-camera evidence or USB traces are absent. |
| Mightex buffered camera SDK | Reverse engineered | SDK-level device/error functions and dependency on a USB helper runtime. | Runtime-package metadata, writable capture parameters, opt-in vendor-runtime one-shot capture, and repeated one-shot stream support exist; native frame transfer/control is not exposed because traces are absent. |
| Mightex USB helper | Reverse engineered | Windows USB/Cypress-style helper and bulk endpoint hints. | Useful as native transport context, but not enough to expose native frame acquisition or control writes without traces. |

## Consequence

Mightex BLS/SLC has enough reverse engineered protocol evidence for a default
SDK-free driver support. Agilent Laser Combiner and Okolab have
reverse-engineered-evidence implementations. The remaining rows above improve
the evidence map but do not unlock MCL, ABS, or Mightex camera default drivers.
