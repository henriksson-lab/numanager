# Mightex Protocol Evidence Note

## Status

| Field | Value |
| --- | --- |
| Plan target | Mightex cameras and Mightex_BLS illumination/controller family |
| Current state | BLS/SLC has a device page, Rust protocol encoder/decoder, optional HID backend, identity discovery with preserved config labels and config-backed topology overrides, enum-backed `MODE`/low-range `CURRENT` output writes with public value bounds, SLC raw `NORMAL` pair writes plus staged strobe/trigger profile setup, BLS-only `soft_start` plus raw staged pulse/follow trigger setup, volatile overdrive rule queries from `?GetImax`/`?GetODRules`, SLC-only raw `?MODE`/`?CURRENT` max/set/`?STROBE`/`?STRP`/`?TRIGGER`/`?TRIGP`/`ReadBinaryV` readbacks, a hub-only `disable_all` helper, hidden maintenance operations, command/reply telemetry with conservative outcome labels and reply kind, conservative textual error-reply failure handling, and an opt-in 1% output bring-up path that prints requested output, hold duration, completion/readback, channel disable, staged BLS/SLC trigger settings when present, optional SLC profile readbacks, and broadcast disable-all result; no hardware trace or validation note yet. Camera subset has writable capture parameters, digest-verified vendor-runtime one-shot raw capture, and repeated one-shot stream support with runtime trigger-mode/software-trigger setup; native USB capture/stream, native gain/color controls, and broader SDK-free controls are not exposed because protocol evidence is absent |
| Better source status | BLS/SLC HID framing and ASCII command construction are reverse engineered; camera SDK/public wrappers do not provide a clean frame protocol |
| Next evidence | BLS/SLC hardware HID traces; reverse engineered camera evidence plus USB frame traces |
| Camera evidence type | Reverse engineered |
| BLS evidence type | Reverse engineered HID implementation; no separate reverse engineered evidence required for the BLS/SLC command surface found in this pass |
| Feasibility | BLS/SLC can advance to spec candidate; camera one-shot capture and repeated one-shot stream can use the verified vendor runtime, while native capture/stream requires frame protocol evidence |

## Protocol Evidence Summary

| Area | Finding |
| --- | --- |
| BLS/SLC protocol evidence | Reverse engineered HID discovery, feature-report write/read framing, product-string parsing, and ASCII command construction |
| BLS/SLC repo implementation | `numanager-core::hid` defines scripted and optional OS feature-report transports; `numanager_drivers::mightex_bls` exposes HID identity devices plus reverse engineered `SL` product-token topology parsing, preserved configured discovery labels, config-backed channel-count/module-type overrides, diagnostic hub support, channel enum `mode`/`enabled`, SLC raw `NORMAL` pair writes and staged strobe/trigger profile setup, BLS-only `soft_start` and raw staged pulse/follow trigger setup, volatile overdrive rule readback, SLC-only raw current/strobe/trigger profile readback properties including reverse engineered `ECHOOFF` flush and binary load-voltage count, and low-range `CURRENT` output with runtime-validated public bounds, hub-only disable-all helper alias with maintenance operations hidden, reverse engineered reply-versus-send-only policy, command/reply telemetry, trace-note-friendly `last_transaction`, `last_outcome`, and `last_error` for obvious textual failure replies; crate-private Mightex BLS helpers encode documented HID chunks, ASCII command families, binary echo u16 parsing, and Sirius product-string classification; the generic `light_source` example has a opt-in hardware-output branch that holds 1% output long enough to observe and prints completion/readback before disabling the channel and sending broadcast disable-all |
| BLS/SLC discovery | HID product string filters are `Sirius BLS` and `Sirius SLC`; module type and channel count are parsed from the reverse engineered `SL` product token with conservative fallbacks |
| BLS/SLC transport | Feature reports use report ID 0, ASCII type 1, a payload length byte, and command bytes chunked into the report payload; SDK command constructors include LF/CR (`\n\r`) terminators; replies are drained with repeated feature reads |
| BLS/SLC commands | Evidenced command families include `MODE`, `CURRENT`, SLC `NORMAL`/`STROBE`/`STRP`/`TRIGGER`/`TRIGP`, BLS `Trigger`/`TrigP`, `SoftStart`, hidden maintenance operations, `?GetImax`, `?GetODRules`, SLC `ECHOOFF`, `?MODE`, `?CURRENT`, `?STROBE`, `?STRP`, `?TRIGGER`, `?TRIGP`, and SLC `ReadBinaryV`; the Rust driver now exposes `MODE`, `CURRENT`, SLC `NORMAL` plus strobe/trigger setup, BLS `Trigger`/`TrigP`/`SoftStart`, the two rule queries, SLC-only mode/current/strobe/trigger profile/binary voltage raw readbacks, and hub-only disable-all alias |
| BLS/SLC audited surface | The remaining audited BLS/SLC status paths are configured software status defaults: `Status = No Fault`, `Busy() = false`, and dynamic-error helpers that set no hardware error code. No reverse engineered fault query, ACK vocabulary, calibrated current scale, optical-output unit, safe operating envelope, or timing-completion frame is present beyond the commands already listed above |
| Camera evidence | Reverse engineered evidence records the buffered-camera SDK ABI for init, working-set selection, engine/grab lifecycle, raw frame callback, exposure, ROI, trigger mode, and software trigger; see [`artifact-inspection-summary.md`](artifact-inspection-summary.md) |
| Camera status | One-shot raw capture, repeated one-shot stream, and typed next-capture parameters are implemented through the verified vendor runtime. Bulk endpoint names prove native USB transport shape, but not native frame format, control requests, buffer ownership, or completion semantics |

## Evidence To Collect

| Evidence | Required observations |
| --- | --- |
| Evidence inventory | Camera evidence exists; BLS/SLC source path identified |
| Strings | USB VID/PID, HID report labels, command names, error text, model names |
| Micro-Manager adapter calls | Distinguish camera properties from BLS/illumination properties |
| HID/USB trace | Confirm BLS/SLC report framing, open sequence, channel output commands, status/fault readback |
| Camera trace | Native acquisition start/stop, gain/color, ROI/binning, frame buffer transfer, and trigger semantics beyond the runtime one-shot path |
| Hardware note | Exact camera or BLS model, firmware, endpoint/report descriptors |

## Protocol Questions

| Area | Questions |
| --- | --- |
| BLS transport | HID report layout, addressing, command ACK/status, channel count |
| BLS output | Enable, intensity/current/power, wavelength/channel identity, trigger mode |
| Camera compatibility | Whether any models expose UVC, DirectShow, GenICam, or platform-camera paths |
| Camera acquisition | Buffer ownership, frame metadata, timestamps, dropped-frame reporting |
| Camera properties | Exposure units, gain units (`Decibel` or `Ratio`), ROI/binning, pixel formats |
| Safety | Output fault, thermal fault, trigger lockout, camera acquisition errors |

## Candidate Public Surface

| Device | Capabilities | Properties |
| --- | --- | --- |
| BLS hub | safety summary | `model`, `firmware`, `fault`, `channel_summary` |
| BLS channel | `Dac`, `TriggerSink` | `enabled`, `intensity` or `current`, `wavelength`, `trigger_mode`, `fault` |
| Camera | `CameraCapture`, `CameraStream`, `TriggerSink` | `exposure`, `gain`, `pixel_format`, `width`, `height`, `roi_*` if evidenced |

Use typed values: `Ratio`, `ElectricCurrent`, `OpticalPower`, `Wavelength`,
`TimeInterval`, `Decibel`, `PixelCount`, and canonical pixel-format strings.

## Stop/Proceed Decision

| Decision | Condition |
| --- | --- |
| Proceed with BLS spec | Done for command/framing spec, evidence gate, and output driver; see [`../devices/mightex-bls.md`](../devices/mightex-bls.md). The reverse engineered command/readback surface currently identified has been implemented for bring-up. Hardware traces are the next required evidence before adding fault properties, calibrated units, hardware-safe ranges, or timing-plan behavior |
| Proceed with camera spec | Vendor-runtime one-shot capture and repeated one-shot stream are available; native frame transfer and completion semantics require traces or another clean source |
| Route to platform camera | Hardware exposes UVC/DirectShow/GStreamer-compatible capture |
| Block SDK-free camera | Only opaque camera SDK API is visible and no trace/hardware evidence exists |

## Implementation Gate

A Mightex camera driver may use the verified vendor runtime for one-shot raw
capture, but native frame-buffer capture/stream operations are not exposed
when frame-buffer semantics are not documented. Do not claim more
default Mightex BLS/SLC hardware behavior from reverse engineered evidence
alone: the remaining visible surfaces are software-side status/error defaults, not
hardware protocol.
BLS/SLC output has no hardware-trace claim for full command completion/error
vocabulary, calibrated units, hardware-safe ranges, fault states, or timing
behavior.
