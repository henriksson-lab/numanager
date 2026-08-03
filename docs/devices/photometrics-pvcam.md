# Photometrics PVCAM Cameras

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::photometrics_pvcam` |
| Families | Teledyne Photometrics / QImaging PVCAM cameras such as Prime, Prime BSI, Kinetix, Iris, Retiga |
| Support level | Configured and active USB PVCAM evidence plus runtime-package file-status/digest/loadability/ABI-symbol checks, opt-in PVCAM camera-name discovery, writable exposure setting, opt-in one-shot capture, repeated one-shot stream support, and runtime temperature read/setpoint control through a verified vendor runtime; native continuous streaming and broader parameter control are not exposed because documented ABI/native-transport evidence is absent |
| Protocol evidence | Reverse engineered notes |
| Transport | Default model covers the PVCAM vendor-library path and descriptor-only USB identity discovery. Native SDK-free USB/PCIe transport needs host-command framing, request fields, completion, and frame ownership evidence. Local notes record USB VID `0x1f12`, USB control OUT `0x40/0xd4`, control IN `0xc0/0xd5`, host command class `0x3f`, frame begin `0x26`, frame end `0x28`, and PCIe `/dev/pvcam_pcie*` ioctl/source evidence |
| Discovery | Config-backed two-stage discovery plus non-invasive `os-usb` descriptor scanning for USB VID `0x1f12`; explicit `load_vendor_runtime=true` can initialize the configured PVCAM runtime and read camera count/names after SHA-256 verification; PCIe scanning needs native transport evidence |
| Validation | No real camera validation in numanager |
| Runtime/evidence notes | `CameraCapture`, repeated one-shot `CameraStream`, and runtime temperature read/setpoint control are available only through the verified vendor-runtime backend with `load_vendor_runtime=true`. Writable `exposure` updates the timed-mode value used by the next one-shot capture. Writable `temperature_setpoint` uses PVCAM `PARAM_TEMP_SETPOINT` with availability/access/range checks; configured values remain metadata when the runtime backend is unavailable. Native continuous streaming, broader PVCAM parameter-API control, native USB/PCIe transport, CCL/SCCL upload, trigger routing, shutter timing, metadata decode, compression, centroids, EM gain, and S.M.A.R.T. streaming is not exposed without documented ABI behavior, native transport evidence, or hardware traces; reset/maintenance operations remain hidden. Required vendor firmware/runtime packages may be shipped or loaded as third-party excluded data behind an optional backend when project-owned firmware, loader, or runtime replacements are not available |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `<label> hub` | `hub`, `camera.controller`, `pvcam` | Represents the PVCAM library/native transport command surface |
| `<label>` | `camera`, `camera.scientific`, `detector.mono`, `pvcam` | Camera logical device; one-shot capture uses the verified vendor runtime |
| `<label> cooler` | `temperature.controller`, `cooler`, `state.device` | Runtime temperature read/setpoint control through verified PVCAM parameter APIs, with configured metadata |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `PVCAM library` | `vendor.library.pvcam` | Third-party vendor firmware/runtime package identity, configured file status, SHA-256 digest state, explicit opt-in loadability state, and PVCAM symbol-presence state after digest verification |
| `native transport evidence` | `reverse.usb-pcie` | Records USB/PCIe host-command evidence and descriptor-discovered USB identity when available, but is not a default SDK-free transport |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `CameraCapture` | Camera | `CapabilityRequest::CameraCapture` or `None`; optional `Native`, `Mono16`, `Raw16`, `Mono8`, or `Raw8` encoding constrained by configured bit depth | Frame handle plus width, height, pixel format, and source metadata | Opens the configured runtime camera, sets one full-frame timed sequence, starts acquisition, polls `pl_exp_check_status`, finishes the sequence, then closes/uninitializes | One-shot capture only; software-timed invocation |
| `CameraStream` | Camera | `CapabilityRequest::CameraStream` with encoding, frame count, and buffer policy | `CameraStreamStarted`-parseable map plus one `FrameReady` event per frame | Repeats the same verified runtime one-shot capture path under one stream id; does not claim native continuous acquisition, EOF events, or dropped-frame semantics | Runtime-managed frame sequence |
| `TemperatureControl` | Cooler | `CapabilityRequest::TemperatureControl` or `None`; optional target setpoint | Map containing `temperature_setpoint` and `sensor_temperature` | Checks `PARAM_TEMP_SETPOINT` availability/access/min/max, writes through `pl_set_param`, and reads current `PARAM_TEMP`/`PARAM_TEMP_SETPOINT` when the verified runtime backend is enabled | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `camera_name` | Hub | `String` | none | R | PVCAM opaque camera token | No | `pl_cam_get_name` |
| `product` | Hub/camera | `String` | none | R | configured/product identity | No | PVCAM identity/config |
| `serial_number` | Hub | `String` | none | R | configured/PVCAM serial | No | PVCAM identity parameter |
| `firmware_version` | Hub | `String` | none | R | configured/PVCAM firmware | No | `PARAM_CAM_FW_VERSION` |
| `interface_type` | Hub | `String` | none | R | `USB`, `PCIe`, `Ethernet`, etc. | No | `PARAM_CAM_INTERFACE_TYPE` |
| `usb_vendor_id` | Hub | `I64` | none | R | `0x1f12` | No | udev/runtime installer evidence |
| `usb_product_id` | Hub | `I64` or `Null` | none | R | descriptor-discovered product ID when available | No | Runtime USB descriptor |
| `usb_identity` | Hub | `Map` or `Null` | none | R | descriptor-discovered VID/PID plus product, serial, bus, and address when available | No | Runtime USB descriptor; descriptor scanning does not open devices or imply capture/control support |
| `host_command_class`, `host_frame_begin`, `host_frame_end` | Hub | `I64` | none | R | `0x3f`, `0x26`, `0x28` | No | Native host-command frame notes |
| `usb_control_out_request`, `usb_control_out_request_type`, `usb_control_in_request`, `usb_control_in_request_type` | Hub | `I64` | none | R | `0xd4`, `0x40`, `0xd5`, `0xc0` | No | Native USB notes; native transport is not exposed because host-command and completion evidence is absent |
| `vendor_runtime_path`, `vendor_runtime_sha256` | Hub | `String` | none | R | configured package identity | No | Third-party vendor firmware/runtime package |
| `load_vendor_runtime` | Hub | `Bool` | none | R | explicit opt-in runtime-load backend flag; default `false` | No | Configured backend gate |
| `vendor_runtime_state` | Hub | `String` | none | R | `not_configured`, `configured_without_digest`, `configured_with_digest`, or `digest_without_path` | No | Derived from configured runtime package identity |
| `vendor_runtime_file_status` | Hub | `String` | none | R | `not_configured`, `present`, `not_a_file`, or `unavailable:<kind>` | No | Local configured package path check |
| `vendor_runtime_file_size` | Hub | `ByteCount` | bytes | R | byte length when configured path is a regular file; `0` when not configured | No | Local configured package path check |
| `vendor_runtime_digest_state` | Hub | `String` | none | R | `not_configured`, `invalid_configured_sha256`, `digest_without_path`, `verified`, `mismatch:<actual>`, or `unavailable:<message>` | No | SHA-256 identity check for the configured runtime package |
| `vendor_runtime_probe_state` | Hub | `String` | none | R | `disabled`, `missing_sha256`, `invalid_configured_sha256`, `missing_path`, `digest_mismatch`, `digest_unavailable:<message>`, `file_unavailable:<kind>`, `loaded`, or `load_error:<message>` | No | Verifies configured SHA-256, then attempts to load the configured runtime only when `load_vendor_runtime=true`; does not call PVCAM init/open/capture APIs |
| `vendor_runtime_abi_state` | Hub | `String` | none | R | `disabled`, digest-gate states, `load_error:<message>`, `symbols_present:<list>`, or `missing_symbols:<list>` | No | After digest verification and explicit `load_vendor_runtime=true`, loads the configured runtime and checks expected PVCAM exported symbols without calling them |
| `vendor_runtime_discovery_state` | Hub | `String` | none | R | `disabled`, digest-gate states, `load_error:<message>`, `missing_symbols:<list>`, `init_failed`, `camera_count_failed`, `camera_name_failed:<index>`, or `ready:<count>` | No | After digest verification and explicit `load_vendor_runtime=true`, calls `pl_pvcam_init`, `pl_cam_get_total`, `pl_cam_get_name`, and `pl_pvcam_uninit`; does not open cameras or start acquisition |
| `vendor_runtime_camera_count` | Hub | `I64` | none | R | PVCAM runtime camera count, or `0` when not configured or unavailable | No | Count of names returned by the opt-in PVCAM camera-name discovery path |
| `vendor_runtime_camera_names` | Hub | `List` | none | R | PVCAM runtime camera names, or empty when not configured or unavailable | No | Names returned by `pl_cam_get_name` during the opt-in read-only discovery path |
| `package_strategy` | Hub | `String` | none | R | default interim package policy | No | Runtime support metadata |
| `package_gate` | Hub | `String` | none | R | optional-backend loading and behavior-evidence gate | No | Runtime support metadata |
| `third_party_notice` | Hub | `String` | none | R | license note | No | Runtime support metadata |
| `support_level` | Hub | `String` | none | R | current support gate | No | Runtime support metadata |
| `chip_name` | Camera | `String` | none | R | sensor/chip name | No | `PARAM_CHIP_NAME` |
| `sensor_width`, `sensor_height` | Camera | `PixelCount` | px | R | configured/full sensor geometry | No | `PARAM_SER_SIZE`, `PARAM_PAR_SIZE` |
| `bit_depth` | Camera | `I64` | bits | R | configured/current bit depth | No | `PARAM_BIT_DEPTH` |
| `pixel_format` | Camera | `String` | none | R | `Mono16`, `Mono8`, `Bayer16` | No | `PARAM_IMAGE_FORMAT`/bit-depth evidence |
| `exposure` | Camera | `TimeInterval` | typed | R/W | positive interval used by vendor-runtime one-shot capture | No | `pl_exp_setup_seq` timed-mode exposure |
| `capture_gate`, `cooler_gate` | Camera/cooler | `String` | none | R | control-surface explanation | No | Runtime support metadata |
| `sensor_temperature` | Cooler | `Temperature` | deg C | R | runtime readback or configured metadata; `Null` config omits the metadata property | No | `PARAM_TEMP` current value in hundredths of deg C |
| `temperature_setpoint` | Cooler | `Temperature` | deg C | R/W | runtime setpoint read/write or configured metadata; `Null` config omits the metadata property | No | `PARAM_TEMP_SETPOINT` current/min/max/write in hundredths of deg C |
## Examples

| Example | Demonstrates |
| --- | --- |
| `discover_devices` | Config-backed PVCAM candidate detection and, with `os-usb`, descriptor-only USB candidate discovery |
| `camera_acquisition`, `camera_stream` | Generic one-shot capture and repeated one-shot stream through the verified vendor-runtime backend |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "photometrics_pvcam"`, `"photometrics-pvcam"`, `"pvcam"`, or `"photometrics"` | Yes | string | Selects the configured PVCAM provider |
| `property.sensor_width`, `property.sensor_height` | No | `PixelCount` or non-negative `I64` | Configured sensor geometry; invalid pixel counts are rejected instead of silently falling back |
| `property.bit_depth` | No | `I64` or decimal string | Configured bit depth; invalid u16 values are rejected instead of silently falling back |
| `property.pixel_format` | No | string | Canonicalized to `Mono16`, `Mono8`, or `Bayer16`; unknown values are rejected instead of silently falling back |
| `property.exposure` | No | positive `TimeInterval`, positive `I64` milliseconds, or positive `F64` seconds | One-shot capture exposure for the verified vendor-runtime backend; writable at runtime as the next capture setup value |
| `property.camera_name`, `property.product`, `property.serial_number`, `property.chip_name`, `property.firmware_version`, `property.interface_type` | No | string | Persistent identity/read-only metadata for config-backed discovery |
| `property.sensor_temperature`, `property.temperature_setpoint` | No | `Temperature` or `Null` | Configured cooler metadata; runtime `temperature_setpoint` writes use the verified vendor runtime when enabled |
| `property.vendor_runtime_path`, `property.vendor_runtime_sha256` | No | string | Third-party vendor firmware/runtime package identity; empty/`none`/`Null` means not configured |
| `property.load_vendor_runtime` | No | bool | Enables the explicit vendor-runtime loadability probe after SHA-256 verification. Default `false`; ordinary discovery does not load third-party code |

## Remaining Work

| Area | Gap |
| --- | --- |
| SDK binding | Configured package file presence/size, SHA-256 digest state, explicit loadability probe, expected PVCAM symbol-presence probe, read-only `pl_cam_get_total`/`pl_cam_get_name` discovery, writable one-shot exposure setting, one-shot capture, and temperature read/setpoint control after digest verification are exposed; richer parameter probing and control ABI calls require documented PVCAM parameter behavior |
| USB discovery | `os-usb` descriptor scanning identifies USB VID `0x1f12` candidates without opening devices; product-specific capability classification needs descriptor-to-model evidence |
| Native transport | Expose SDK-free USB/PCIe host-command transport only after host-command framing, request fields, completion, frame ownership, and command output/readback are documented |
| Capture | Validate the implemented `pl_exp_setup_seq`, `pl_exp_start_seq`, `pl_exp_check_status`, `pl_exp_finish_seq`, timeout abort path, and one frame output on real hardware |
| Streaming | Public `CameraStream` uses repeated evidenced one-shot captures. Native continuous acquisition, ring depth, dropped-frame semantics, EOF events, and native runtime frame-store behavior require more evidence |
| Properties | Writable ROI, binning, readout-port, speed, gain, fan, metadata, shutter, trigger, and additional cooler properties are not exposed because PVCAM `ATTR_AVAIL`, `ATTR_TYPE`, `ATTR_ACCESS`, ranges/enums, readback, and hardware behavior evidence is absent |
| Safety | Cooler disable, fan, shutter, trigger, EM gain, and fault/status control surfaces are not exposed because safe behavior and recovery path evidence is absent |
