# ABS Camera

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::abs_camera` |
| Families | ABS legacy USB cameras using CamUSB API |
| Support level | Runtime-package evidence with file-status/digest/loadability/ABI-symbol checks, writable exposure setting, opt-in vendor-runtime capture with explicit async software trigger, and repeated one-shot stream support; native transport, native continuous streaming, gain controls, persistent trigger modes, and broader acquisition behavior is not exposed because USB protocol evidence is absent |
| Evidence | Reverse engineered |
| Transport | Vendor runtime `CamUSB_GetImage` capture path is implemented; native USB control/frame protocol is not recorded |
| Validation | No numanager hardware validation note |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `abs-camera` | `camera`, `reverse.engineered` | One camera plus transport resource; one-shot capture is routed through the verified vendor runtime |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `abs-camera-transport` | `vendor.runtime.camera` | Optional runtime package identity, configured file status, explicit loadability state, and CamUSB symbol-presence state are exposed; runtime capture uses documented CamUSB ABI calls |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `CameraCapture` | Camera | `Native` or a matching returned encoding (`Mono8`, `Mono16`, `Raw8`, `Raw16`, `Rgb8`, or `Bgr8`) with optional buffer label | Captured frame handle map and `FrameReady` event | Requires `load_vendor_runtime=true`, verified `vendor_runtime_sha256`, and configured runtime path; uses hidden `CamUSB_InitCameraExS`, `MODE_ASYNC_TRIGGER`, `CamUSB_SetExposureTime`, `CamUSB_TriggerImage`, `CamUSB_GetImage`, and `CamUSB_ReleaseImage` | Not sequenceable |
| `CameraStream` | Camera | `CapabilityRequest::CameraStream` with encoding, frame count, and buffer policy | `CameraStreamStarted`-parseable map plus one `FrameReady` event per frame | Uses repeated `CameraCapture` transactions through the verified runtime path; does not claim native continuous CamUSB streaming or dropped-frame semantics | Runtime-managed frame sequence |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `exposure` | Camera | `TimeInterval` | s | R/W | Positive interval; runtime capture converts to microseconds | No | `CamUSB_SetExposureTime` during capture |
| `pixel_format` | Camera | `String` | none | R/config | `Native` before capture, then returned canonical `Mono8`, `Mono16`, `Rgb8`, or `Bgr8` when mapped from `S_IMAGE_HEADER.dwPixel_type` | No | `S_IMAGE_HEADER.dwPixel_type` |
| `width` | Camera | `PixelCount` | px | R/config | `0` before capture if unknown, then returned image width | No | `S_IMAGE_HEADER.dwSize_x` |
| `height` | Camera | `PixelCount` | px | R/config | `0` before capture if unknown, then returned image height | No | `S_IMAGE_HEADER.dwSize_y` |
| `vendor_runtime_path`, `vendor_runtime_sha256` | Camera | `String` | none | R | configured package identity | No | Third-party camera runtime package |
| `load_vendor_runtime` | Camera | `Bool` | none | R | explicit opt-in runtime-load backend flag; default `false` | No | Configured backend gate |
| `vendor_runtime_state` | Camera | `String` | none | R | `not_configured`, `configured_without_digest`, `configured_with_digest`, or `digest_without_path` | No | Derived from configured runtime package identity |
| `vendor_runtime_file_status` | Camera | `String` | none | R | `not_configured`, `present`, `not_a_file`, or `unavailable:<kind>` | No | Local configured package path check |
| `vendor_runtime_file_size` | Camera | `ByteCount` | bytes | R | byte length when configured path is a regular file; `0` when not configured | No | Local configured package path check |
| `vendor_runtime_digest_state` | Camera | `String` | none | R | `not_configured`, `invalid_configured_sha256`, `digest_without_path`, `verified`, `mismatch:<actual>`, or `unavailable:<message>` | No | SHA-256 identity check for the configured runtime package |
| `vendor_runtime_probe_state` | Camera | `String` | none | R | `disabled`, `missing_sha256`, `invalid_configured_sha256`, `missing_path`, `digest_mismatch`, `digest_unavailable:<message>`, `file_unavailable:<kind>`, `loaded`, or `load_error:<message>` | No | Verifies configured SHA-256, then attempts to load the configured runtime only when `load_vendor_runtime=true`; does not call ABS camera ABI or hardware APIs |
| `vendor_runtime_abi_state` | Camera | `String` | none | R | `disabled`, digest-gate states, `load_error:<message>`, `symbols_present:<list>`, or `missing_symbols:<list>` | No | After digest verification and explicit `load_vendor_runtime=true`, loads the configured runtime and checks expected CamUSB exported symbols without calling them |
| `package_strategy`, `package_gate`, `third_party_notice` | Camera | `String` | none | R | package-policy metadata | No | Runtime support metadata |

## Evidence Gate

| Claim | Current evidence | Default driver decision |
| --- | --- | --- |
| Camera identity | Reverse engineered evidence identifies the CamUSB SDK family, but no exact hardware VID/PID/model/firmware is recorded | Configured identity plus runtime package state |
| Platform-camera route | No device-specific platform route has been recorded | Prefer UVC/DirectShow/platform support if hardware proves compatible |
| Frame transfer | SDK exposes SDK-managed trigger/acquire/release image buffers and `S_IMAGE_HEADER` dimensions/pixel type | Advertise opt-in `CameraCapture` only when a verified vendor runtime is configured; native USB transport needs packet and completion evidence |
| Exposure/gain/control | SDK exposes exposure get/set in microseconds plus broader functions | Writable one-shot `exposure` is exposed because the vendor-runtime capture path applies it through `CamUSB_SetExposureTime`; one-shot capture applies async software trigger through the runtime. Gain controls, persistent trigger modes, and broader SDK-free writes are not exposed because control behavior is not evidenced |
| Streaming/backpressure | SDK acquisition modes include continuous, timed, async, event, and triggers | Public `CameraStream` uses repeated evidenced one-shot captures. Native continuous streaming remains unimplemented because dropped-frame and ring-buffer semantics are not known |

## Examples

| Example | Demonstrates |
| --- | --- |
| `camera_acquisition` | Generic one-shot capture path when configured with `load_vendor_runtime=true` and verified runtime identity |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware identity | Need VID/PID, model, firmware, sensor, descriptors |
| Transport | Determine UVC/platform route versus vendor USB protocol |
| Runtime package | Hardware validation of digest-verified runtime capture, timeout behavior, cleanup, and error reporting |
| Frame protocol | Native USB capture initialization, snap/stream, buffer transfer, release, abort, and errors |
| Driver | Expand beyond runtime-backed one-shot capture/repeated-capture stream only after native transport or additional SDK behavior is evidenced |

## Unblock Trace Checklist

Use the USB vendor/bulk and camera-frame sections of
[`../reverse/trace-capture-guide.md`](../reverse/trace-capture-guide.md) when
collecting these observations.

| Trace item | Must record |
| --- | --- |
| Hardware identity | Camera model, firmware/driver/SDK version, sensor mode, USB descriptors, and whether the OS also exposes a UVC/DirectShow/platform-camera device |
| Platform route | If UVC/platform-compatible, backend name, negotiated format, frame dimensions, timestamps, exposure/gain readback, trigger support, and the matching generic camera example output |
| Vendor transport | USB endpoints/control requests for open/init, exposure/gain/ROI/pixel-format setup, snap, stream start, abort, close, and the runtime output for the same action windows |
| Frame layout | Exact frame payload layout, pixel format, dimensions, metadata/header/footer if any, and how SDK buffer ownership maps to frame-ready completion plus printed frame-handle metadata |
| Throughput | Continuous acquisition trace with ring-buffer capacity, dropped/late frame behavior, abort behavior, frame completion/error reporting, and matching stream status output |
| Fault path | Timeout, disconnect, invalid exposure/gain, or abort/error status plus failed-operation output sufficient to map runtime operation failures |
