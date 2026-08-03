# ABS Camera Protocol Evidence Note

## Status

| Field | Value |
| --- | --- |
| Plan target | ABS legacy USB camera |
| Current state | Device page, configured driver surface, third-party runtime package metadata, digest-verified loadability and CamUSB symbol-presence probes, writable exposure setting, opt-in vendor-runtime one-shot capture with explicit async software trigger, and repeated one-shot stream support exist; native USB transport, native continuous streaming, and broader controls are not recorded as public protocol evidence |
| Better source status | CamUSB SDK headers document the runtime ABI; hardware family and native USB protocol are not identified |
| Next evidence | Runtime capture hardware validation plus USB traces if SDK-free native transport is needed |
| Evidence type | Reverse engineered |
| Feasibility | Vendor-runtime one-shot capture can use the verified CamUSB runtime; do not implement SDK-free capture from this evidence alone |

## Protocol Evidence Summary

| Area | Finding |
| --- | --- |
| Evidence inventory | Reverse engineered; see [`artifact-inspection-summary.md`](artifact-inspection-summary.md) |
| Header/API evidence | Public declarations expose camera list/init/free, get/set function values and capabilities, image acquire/release/abort, trigger image, capture mode, device notification/event notification, exposure, gain, pixel type, frame rate, resolution, IO ports, temperature, profiles, sensor register/firmware access, and related status APIs |
| Capture-mode evidence | Public constants name triggered software, triggered hardware, continuous, event sync/event, timed, and async trigger acquisition modes |
| Adapter evidence | The Micro-Manager adapter uses `CamUSB_InitCameraExS`, automatic flash-firmware initialization, `CamUSB_GetImage`, `CamUSB_ReleaseImage`, `CamUSB_SetCaptureMode`, `CamUSB_TriggerImage`, property get/set calls, and SDK-managed image buffers |
| Driver status | `numanager_drivers::abs_camera` uses verified third-party runtime packages only when explicitly enabled, lets the typed `exposure` property update the next capture exposure, performs hidden initialization through `CamUSB_InitCameraExS`, sets async-trigger capture mode, applies exposure in microseconds, issues `CamUSB_TriggerImage`, copies the SDK-managed image buffer using `S_IMAGE_HEADER`, releases it with `CamUSB_ReleaseImage`, and returns supported canonical frame encodings through the runtime frame store |
| Transport evidence | Current static inspection has not recovered UVC compatibility, USB control requests, endpoint/frame protocol, buffer descriptor layout, or dropped-frame/error frame semantics |
| Missing wire evidence | Need exact camera hardware identity plus USB traces or public low-level protocol information before a default SDK-free camera driver can own acquisition safely |

## Evidence To Collect

| Evidence | Required observations |
| --- | --- |
| Hardware identity | Vendor/product IDs, model, firmware, sensor, endpoint descriptors |
| Evidence inventory | Done; still need matching package variants |
| Strings | Camera model names, USB commands, pixel formats, error text; current pass exposes SDK API names but not frame protocol |
| Micro-Manager adapter calls | Done at API-surface level; still need exact property/range mapping curated if hardware becomes available |
| USB trace | Initialization, property writes, snap/stream start/stop, frame transfer |
| Platform route | Whether the camera exposes UVC/DirectShow or other OS-camera compatibility |

## Protocol Questions

| Area | Questions |
| --- | --- |
| Transport | UVC/DirectShow-compatible, USB vendor control/bulk, or opaque SDK |
| Acquisition | Snap versus stream, buffer ownership, frame completion, timeout/error behavior |
| Image format | Pixel formats, bit depth, stride, ROI/binning, metadata |
| Controls | Exposure, gain, trigger mode, frame interval, black level if present |
| Throughput | Ring-buffer needs, dropped-frame reporting, backpressure |

## Candidate Public Surface

| Device | Capabilities | Properties |
| --- | --- | --- |
| ABS camera | `CameraCapture`, `CameraStream`, `TriggerSink` if evidenced | `exposure`, `gain`, `pixel_format`, `width`, `height`, `frame_interval`, `trigger_mode` |

Use typed values: `TimeInterval`, `Decibel` or `Ratio` for gain depending
evidence, `PixelCount`, and canonical pixel-format strings.

## Stop/Proceed Decision

| Decision | Condition |
| --- | --- |
| Route to platform camera | Device is UVC/DirectShow-compatible |
| Proceed to ABS spec | USB control/frame protocol is recoverable |
| Evidence policy | Verified CamUSB runtime packages may provide one-shot capture; SDK-free native capture/stream operations fail closed when no platform or USB path is known |

## Implementation Gate

`numanager_drivers::abs_camera` may use the verified CamUSB runtime for
one-shot capture, but native USB capture/stream operations are not exposed
unless a platform-camera route, open source implementation,
hardware trace, or explicit SDK binding evidence provides exact frame
acquisition and completion behavior.
