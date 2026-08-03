# Mightex Buffered USB Camera

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::mightex_camera` |
| Families | Mightex C/Cam buffered USB cameras |
| Support level | Runtime-package evidence with file-status/digest/loadability/ABI-symbol checks, writable capture parameters, opt-in vendor-runtime `Mono16`/`Raw16` capture, and repeated one-shot stream support; native transport, native continuous streaming, native gain/color controls, ROI/binning beyond configured frame dimensions, and broader SDK-free acquisition behavior is not exposed because native protocol evidence is absent |
| Evidence | Reverse engineered |
| Transport | Vendor runtime callback capture path is implemented; native USB control/frame protocol is not recorded |
| Validation | No numanager hardware validation note |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `mightex-camera` | `camera`, `reverse.engineered` | One camera plus control/stream resources; one-shot capture is routed through the verified vendor runtime |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `mightex-camera-control` | `usb.vendor.control` | Optional runtime package identity, configured file status, explicit loadability state, and documented SDK symbol-presence state are exposed for backend bring-up; runtime capture uses documented ABI calls |
| `mightex-camera-stream` | `usb.bulk.stream` | Native frame transfer path is not recorded; optional runtime package state mirrors the control resource |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `CameraCapture` | Camera | `Native`, `Mono16`, or `Raw16` encoding with optional buffer label | Captured frame handle map and `FrameReady` event | Requires `load_vendor_runtime=true`, verified `vendor_runtime_sha256`, configured runtime path, and raw >8-bit callback mode; uses `BUFCCDUSB_SetSoftTrigger` and waits for the SDK frame callback | Not sequenceable |
| `CameraStream` | Camera | `CapabilityRequest::CameraStream` with encoding, frame count, and buffer policy | `CameraStreamStarted`-parseable map plus one `FrameReady` event per frame | Repeats the same verified runtime one-shot capture path under one stream id; does not claim native continuous buffering or dropped-frame semantics | Runtime-managed frame sequence |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `exposure` | Camera | `TimeInterval` | s | R/W | Non-negative interval; runtime capture converts to 50 us SDK ticks | No | `BUFCCDUSB_SetExposureTime` during capture |
| `pixel_format` | Camera | `String` | none | R/config | `Native`, `Mono16`, or `Raw16` capture path; other SDK formats are not exposed because frame layout evidence is absent | No | Raw callback frame type 0 |
| `width` | Camera | `PixelCount` | px | R/W | Positive frame width used by the next one-shot capture | No | `BUFCCDUSB_SetCustomizedResolution` during capture |
| `height` | Camera | `PixelCount` | px | R/W | Positive frame height used by the next one-shot capture | No | `BUFCCDUSB_SetCustomizedResolution` during capture |
| `bit_depth` | Camera | `I64` | bits | R/W | `9..=16`, raw >8-bit callback mode; default 12 | No | `BUFCCDUSB_StartCameraEngine` bit option and raw callback |
| `vendor_runtime_path`, `vendor_runtime_sha256` | Camera | `String` | none | R | configured package identity | No | Third-party camera runtime package |
| `load_vendor_runtime` | Camera | `Bool` | none | R | explicit opt-in runtime-load backend flag; default `false` | No | Configured backend gate |
| `vendor_runtime_state` | Camera | `String` | none | R | `not_configured`, `configured_without_digest`, `configured_with_digest`, or `digest_without_path` | No | Derived from configured runtime package identity |
| `vendor_runtime_file_status` | Camera | `String` | none | R | `not_configured`, `present`, `not_a_file`, or `unavailable:<kind>` | No | Local configured package path check |
| `vendor_runtime_file_size` | Camera | `ByteCount` | bytes | R | byte length when configured path is a regular file; `0` when not configured | No | Local configured package path check |
| `vendor_runtime_digest_state` | Camera | `String` | none | R | `not_configured`, `invalid_configured_sha256`, `digest_without_path`, `verified`, `mismatch:<actual>`, or `unavailable:<message>` | No | SHA-256 identity check for the configured runtime package |
| `vendor_runtime_probe_state` | Camera | `String` | none | R | `disabled`, `missing_sha256`, `invalid_configured_sha256`, `missing_path`, `digest_mismatch`, `digest_unavailable:<message>`, `file_unavailable:<kind>`, `loaded`, or `load_error:<message>` | No | Verifies configured SHA-256, then attempts to load the configured runtime only when `load_vendor_runtime=true`; does not call Mightex camera ABI or hardware APIs |
| `vendor_runtime_abi_state` | Camera | `String` | none | R | `disabled`, digest-gate states, `load_error:<message>`, `symbols_present:<list>`, or `missing_symbols:<list>` | No | After digest verification and explicit `load_vendor_runtime=true`, loads the configured runtime and checks expected Mightex SDK exports without calling them |
| `package_strategy`, `package_gate`, `third_party_notice` | Camera | `String` | none | R | package-policy metadata | No | Runtime support metadata |

## Evidence Gate

| Claim | Current evidence | Default driver decision |
| --- | --- | --- |
| USB stream transport | External notes record bulk endpoint style helper operations | Bulk shape is visible, but no native frame protocol is implementable yet |
| Control path | Camera SDK and adapter expose exposure, ROI, trigger, capture, and buffer APIs | The capture parameters already applied by the verified runtime path (`exposure`, `width`, `height`, and `bit_depth`) are exposed; one-shot capture applies SDK trigger mode and software trigger through the runtime. Gain/color controls and broader SDK-free writes are not exposed because control packets/readbacks are not known |
| Capture completion | SDK-managed raw frame callback and software-trigger calls are visible | Advertise opt-in `CameraCapture` only when a verified vendor runtime is configured; hardware validation remains required |
| Streaming/ring buffer | Bulk endpoints imply high-throughput transfer | Public `CameraStream` uses repeated evidenced one-shot captures. Native continuous streaming remains unimplemented because dropped-frame and backpressure behavior are not known |
| Platform-camera route | No UVC/DirectShow/GenICam compatibility evidence is recorded | Prefer generic camera backend if a real device proves compatible |

## Examples

| Example | Demonstrates |
| --- | --- |
| `camera_acquisition`, `camera_stream` | Generic one-shot capture and repeated one-shot stream paths when configured with `load_vendor_runtime=true` and verified runtime identity |

## Remaining Work

| Area | Gap |
| --- | --- |
| Frame protocol | Need USB bulk packet/frame layout, metadata, timestamps, and ownership rules |
| Control protocol | Need gain/color/ROI/binning command and readback evidence plus SDK-free native control writes |
| Runtime package | Hardware validation of digest-verified runtime capture, timeout behavior, cleanup, and error reporting |
| Throughput | Need ring-buffer size, overflow, dropped-frame, and backpressure behavior |
| Driver | Expand beyond runtime-backed one-shot capture/repeated-capture stream only after native frame transfer or additional SDK behavior is evidenced |

## Unblock Trace Checklist

Use the USB vendor/bulk and camera-frame sections of
[`../reverse/trace-capture-guide.md`](../reverse/trace-capture-guide.md) when
collecting these observations.

| Trace item | Must record |
| --- | --- |
| Hardware identity | Camera model, firmware, SDK/helper runtime version, USB descriptors, endpoint layout, and whether the OS exposes UVC/DirectShow/GenICam/platform-camera compatibility |
| Control path | Raw traffic for open/init, exposure, gain, ROI/binning, pixel format, trigger mode, corresponding readbacks, and matching runtime property output |
| Snap capture | Raw traffic for one triggered or software snap, including start command, frame payload, completion/status, timeout behavior, SDK-visible frame dimensions/format, and matching frame-handle output |
| Streaming | Continuous acquisition traffic with frame IDs or inferred ordering, dropped-frame behavior, abort/stop, backpressure/ring-buffer behavior, and matching stream status output |
| Frame layout | Pixel encoding, row stride, metadata/header/footer, timestamp source, buffer ownership/release semantics, and printed frame metadata keys |
| Fault path | Timeout, disconnect, invalid setting, transfer error, or abort status plus failed-operation output sufficient to map runtime operation failures |
