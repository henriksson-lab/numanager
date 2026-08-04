# Mightex Protocol Evidence Note

Two unrelated interfaces under one manufacturer: the Sirius BLS/SLC
illumination controllers (HID) and the buffered USB cameras.

## Status

| Field | Value |
| --- | --- |
| Plan target | Mightex buffered cameras and the Mightex_BLS illumination/controller family |
| BLS/SLC evidence class | Reverse engineered — HID transport framing and ASCII command construction |
| Camera evidence class | Reverse engineered device/capability facts; no frame-protocol evidence |
| BLS/SLC state | Device page plus a Rust protocol encoder/decoder, optional HID backend, identity discovery, topology overrides from config, enum `MODE` and low-range `CURRENT` output writes with runtime-validated public bounds, staged strobe/trigger/pulse profile setup, overdrive-rule and profile readbacks, command/reply telemetry, and an opt-in 1% output bring-up path. See [`../devices/mightex-bls.md`](../devices/mightex-bls.md) |
| Camera state | Writable next-capture parameters, one-shot raw capture, and repeated one-shot stream with trigger-mode/software-trigger setup, all through an optional vendor runtime loaded only by explicit user configuration. Native USB capture/stream, native gain/colour controls, and broader SDK-free controls are not exposed because protocol evidence is absent |
| Hardware validation | **None** for either half. No captured traffic from a physical device and no validation note |
| Next evidence | BLS/SLC HID traces from real hardware; camera USB frame traces |
| Feasibility | BLS/SLC can advance to a spec candidate. Camera one-shot capture and repeated one-shot stream can use the optional vendor runtime; native capture/stream needs frame-protocol evidence |

## Protocol Evidence Summary

### BLS/SLC illumination controllers

| Area | Finding |
| --- | --- |
| Transport | USB HID feature reports. Report ID 0, ASCII type byte `1`, a payload length byte, then command bytes chunked into the report payload. Commands are terminated with LF/CR (`\n\r`). Replies are drained with repeated feature reads |
| Discovery | HID product-string filters `Sirius BLS` and `Sirius SLC`. Module type and channel count are parsed from the `SL` product token, with conservative defaults when the token does not parse. Config can override channel count and module type and can preserve discovery labels |
| Command families | `MODE`, `CURRENT`; SLC `NORMAL`, `STROBE`, `STRP`, `TRIGGER`, `TRIGP`; BLS `Trigger`, `TrigP`, `SoftStart`; queries `?GetImax`, `?GetODRules`; SLC `ECHOOFF` flush, `?MODE`, `?CURRENT`, `?STROBE`, `?STRP`, `?TRIGGER`, `?TRIGP`; SLC `ReadBinaryV` returning a binary little-endian u16 load-voltage count |
| Reply policy | Each command family is classified reply-bearing or send-only. Textual error replies are treated conservatively as failures and surfaced through `last_transaction`, `last_outcome`, and `last_error` |
| Implemented surface | `numanager-core::hid` provides scripted and optional OS feature-report transports. `numanager_drivers::mightex_bls` exposes HID identity devices, `SL`-token topology, channel `mode`/`enabled`, SLC `NORMAL` pair writes plus staged strobe/trigger profile setup, BLS-only `soft_start` and staged pulse/follow trigger setup, overdrive-rule queries, SLC-only current/strobe/trigger/binary-voltage readbacks, low-range `CURRENT` output, and hidden maintenance operations |
| **Missing wire evidence** | No fault query, no ACK vocabulary, no calibrated current scale, no optical-output unit, no safe operating envelope, and no timing-completion frame. The remaining status paths (`Status = No Fault`, `Busy() = false`, dynamic-error helpers) are software-side defaults, not hardware protocol |

### Buffered cameras

| Area | Finding |
| --- | --- |
| Capability facts | Device init, working-set selection, engine/grab lifecycle, raw frame delivery by callback, exposure, ROI, trigger mode, and software trigger are known to exist as camera operations |
| Optional runtime | One-shot raw capture, repeated one-shot stream, and typed next-capture parameters are implemented over a vendor runtime that is loaded only when the user configures it |
| Native transport | Bulk endpoints establish the transport shape. Frame format, control requests, buffer ownership, and completion semantics are **not** recorded |

## Evidence To Collect

| Evidence | Required observations |
| --- | --- |
| HID/USB trace (BLS/SLC) | Report framing on the wire, open sequence, channel output commands, status/fault readback, command completion |
| Camera trace | Native acquisition start/stop, gain/colour, ROI/binning, frame buffer transfer, and trigger semantics beyond the one-shot runtime path |
| Strings and identity | USB VID/PID, HID report labels, command names, error text, model names |
| Property mapping | Which controls belong to the camera and which to the illumination controller — obtainable from an audited open adapter source |
| Hardware note | Exact camera or BLS model, firmware, endpoint/report descriptors |

## Protocol Questions

| Area | Questions |
| --- | --- |
| BLS transport | Addressing beyond the report framing above, command ACK/status vocabulary, true channel count per model |
| BLS output | Current-to-optical-power calibration, wavelength/channel identity, safe operating envelope, trigger lockout |
| Camera compatibility | Whether any model exposes UVC, DirectShow, GenICam, or a platform-camera path |
| Camera acquisition | Buffer ownership, frame metadata, timestamps, dropped-frame reporting |
| Camera properties | Exposure units, gain units (`Decibel` or `Ratio`), ROI/binning, pixel formats |
| Safety | Output fault, thermal fault, camera acquisition errors |

## Candidate Public Surface

| Device | Capabilities | Properties |
| --- | --- | --- |
| BLS hub | safety summary | `model`, `firmware`, `fault`, `channel_summary` |
| BLS channel | `Dac`, `TriggerSink` | `enabled`, `intensity` or `current`, `wavelength`, `trigger_mode`, `fault` |
| Camera | `CameraCapture`, `CameraStream`, `TriggerSink` | `exposure`, `gain`, `pixel_format`, `width`, `height`, `roi_*` once evidenced |

Use typed values: `Ratio`, `ElectricCurrent`, `OpticalPower`, `Wavelength`,
`TimeInterval`, `Decibel`, `PixelCount`, and canonical pixel-format strings.

## Stop/Proceed Decision

| Decision | Condition |
| --- | --- |
| Proceed with BLS spec | **Done** for command/framing spec, evidence gate, and output driver. The identified command/readback surface is implemented for bring-up. Hardware traces are the next required evidence before fault properties, calibrated units, hardware-safe ranges, or timing-plan behavior |
| Proceed with camera spec | Optional-runtime one-shot capture and repeated one-shot stream are available. Native frame transfer and completion semantics need traces or another clean source |
| Route to platform camera | If hardware turns out to expose a UVC/DirectShow/GStreamer-compatible capture path |
| Block SDK-free camera | While only an opaque camera API is visible and no trace or hardware evidence exists |

## Implementation Gate

A Mightex camera driver may use the optional vendor runtime for one-shot raw
capture, but native frame-buffer capture/stream operations are not exposed while
frame-buffer semantics are undocumented.

No further default BLS/SLC hardware behavior may be claimed from the current
evidence: the remaining visible surfaces are software-side status/error defaults,
not hardware protocol. BLS/SLC output has no hardware-trace claim for command
completion or error vocabulary, calibrated units, hardware-safe ranges, fault
states, or timing behavior.
