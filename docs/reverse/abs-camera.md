# ABS Camera Protocol Evidence Note

## Status

| Field | Value |
| --- | --- |
| Plan target | ABS legacy USB camera |
| Evidence class | Reverse engineered device/capability facts. No manufacturer protocol document and no captured traffic |
| Current state | Device page, configured driver surface, writable exposure setting, opt-in one-shot capture with an explicit async software trigger, and repeated one-shot stream support. All capture runs through an optional vendor runtime that is loaded only when the user configures it |
| Hardware validation | **None.** No bench run and no captured traffic from a physical device |
| Next evidence | A bench run of the runtime capture path, plus USB traces if a native SDK-free transport is wanted |
| Feasibility | One-shot capture over the optional runtime is workable today. Native SDK-free capture cannot be implemented from the current evidence |

The camera hardware family and its native USB protocol are **not identified**.

## Protocol Evidence Summary

| Area | Finding |
| --- | --- |
| Device capability surface | Camera enumeration, open/close, capability and value get/set, image acquire/release/abort, software trigger, capture-mode selection, device and event notification, exposure, gain, pixel type, frame rate, resolution, IO ports, temperature, stored profiles, and sensor-register/firmware access |
| Capture modes | Triggered software, triggered hardware, continuous, event-sync/event, timed, and async trigger |
| Exposure | Applied in microseconds |
| Optional runtime | The driver may load a user-configured vendor runtime. It verifies file status and a digest before loading, then probes loadability and the expected entry points. Nothing is loaded unless the user explicitly enables it |
| Implemented path | `numanager_drivers::abs_camera` performs hidden initialization, sets async-trigger capture mode, applies the typed `exposure` value, issues a software trigger, copies the runtime-owned image buffer using its image header, releases the buffer, and returns supported canonical frame encodings |
| **Missing wire evidence** | No UVC/platform-camera compatibility determination, no USB control requests, no endpoint or frame protocol, no buffer descriptor layout, and no dropped-frame or error-frame semantics |

## Evidence To Collect

| Evidence | Required observations |
| --- | --- |
| Hardware identity | Vendor/product IDs, model, firmware, sensor, endpoint descriptors |
| USB trace | Initialization, property writes, snap and stream start/stop, frame transfer |
| Platform route | Whether the camera exposes UVC/DirectShow or another OS-camera path |
| Property mapping | Exact property/range mapping, obtainable from an audited open adapter source once hardware is available |
| Strings | Camera model names, USB commands, pixel formats, error text |

## Protocol Questions

| Area | Questions |
| --- | --- |
| Transport | UVC/DirectShow-compatible, USB vendor control/bulk, or opaque runtime only |
| Acquisition | Snap versus stream, buffer ownership, frame completion, timeout/error behavior |
| Image format | Pixel formats, bit depth, stride, ROI/binning, metadata |
| Controls | Exposure, gain, trigger mode, frame interval, black level if present |
| Throughput | Ring-buffer needs, dropped-frame reporting, backpressure |

## Candidate Public Surface

| Device | Capabilities | Properties |
| --- | --- | --- |
| ABS camera | `CameraCapture`, `CameraStream`, `TriggerSink` once evidenced | `exposure`, `gain`, `pixel_format`, `width`, `height`, `frame_interval`, `trigger_mode` |

Use typed values: `TimeInterval`, `Decibel` or `Ratio` for gain depending on
evidence, `PixelCount`, and canonical pixel-format strings.

## Stop/Proceed Decision

| Decision | Condition |
| --- | --- |
| Route to platform camera | Device turns out to be UVC/DirectShow-compatible |
| Proceed to ABS spec | USB control/frame protocol becomes recoverable from traces |
| Evidence policy | A user-configured, digest-verified vendor runtime may provide one-shot capture. SDK-free native capture/stream operations fail closed while no platform or USB path is known |

## Implementation Gate

`numanager_drivers::abs_camera` may use the optional user-configured runtime for
one-shot capture. Native USB capture/stream operations are not exposed unless a
platform-camera route, an open-source implementation, a hardware trace, or an
explicit vendor binding contract supplies exact frame acquisition and completion
behavior.
