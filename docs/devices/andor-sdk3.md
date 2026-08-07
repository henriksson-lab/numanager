# Andor SDK3 Cameras

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::andor_camera` |
| Families | Andor/Oxford Instruments SDK3 sCMOS cameras |
| Support level | Andor VID/PID USB discovery, digest-verified hidden FX3 firmware initialization, confirmed EP0 status readbacks, runtime package checks, vendor-runtime SDK3 feature control/readback, cooler control, and opt-in `Mono16` capture |
| Protocol evidence | Reverse engineered Andor behavior plus public Cypress loader evidence where applicable |
| Transport | USB userspace evidence: EP0 vendor control including SDK3 status reads, bulk-IN `0x82` frame readout, FX3 firmware loading by Cypress CY-image RAM writes; no bulk-OUT data pipe for normal frame data |
| Discovery | Config-backed candidates plus passive `os-usb` descriptor scanning for Andor SDK3 PIDs. Generic Cypress loader IDs are reported by EZ-USB loader discovery as ambiguous, not as Andor, unless config-gated firmware initialization observes an Andor runtime VID/PID |
| Validation | Hardware validation note pending |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `<label> hub` | `hub`, `usb.camera`, `camera.controller` | Owns EP0 control, runtime package metadata, hidden firmware package initialization, and frame bulk-IN resources |
| `<label>` | `camera`, `camera.scientific`, `detector.mono`, `andor.sdk3` | SDK3 camera device; capture and standard feature control use the verified vendor runtime while native USB write/acquisition framing is not exposed because request/ACK evidence is absent |
| `<label> cooler` | `temperature.controller`, `cooler`, `state.device` | SDK3 cooler feature device backed by verified vendor-runtime temperature features |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `CameraCapture` | Camera | `CapabilityRequest::CameraCapture` or `None`; `Native`, `Mono16`, and `Raw16` encodings only | Frame handle plus width/height/pixel-format/source metadata | With `os-usb`, `connect=true`, `load_vendor_runtime=true`, and verified `vendor_runtime_sha256`, the driver loads the configured SDK3 runtime, opens `camera_index`, configures one internal-trigger `Mono16` frame, queues an aligned buffer, starts acquisition, waits for one buffer, stops, flushes, and closes | Runtime-managed completion; hardware timing validation pending |
| `TemperatureControl` | Cooler | `CapabilityRequest::TemperatureControl` with optional `enabled` and `target` | Changed `sensor_cooling` / `temperature_control` values | With `connect=false`, updates configured cooler state; with `connect=true`, `load_vendor_runtime=true`, and verified `vendor_runtime_sha256`, uses verified vendor-runtime `SensorCooling` and `TemperatureControl` feature setters | Runtime-managed completion; hardware timing validation pending |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `product`, `serial_number`, `sdk_family` | Hub/camera | `String` | none | R | `Sdk3` family for known SDK3 PIDs | No | USB descriptor/PID table |
| `vendor_id`, `product_id`, `usb_identity` | Hub | `I64` / `Map` | none | R | Andor VID/PID plus bus/address/serial when discovered | No | USB descriptors |
| `status_byte`, `sdk3_status_word` | Hub | `I64` | none | R | configured metadata, or live EP0 readback when `connect=true` and `os-usb` is enabled | No | Confirmed SDK3 vendor-control IN requests `0xFA` and `0xFD` |
| `vendor_runtime_*`, `firmware_blob_*`, `package_*`, `third_party_notice` | Hub | typed metadata | none/bytes | R/config | Runtime package status, firmware package status, digest, loadability, and ABI-symbol state | No | Optional third-party package metadata; firmware upload is hidden and initialization-only |
| `camera_index` | Hub | `I64` | none | R/config | SDK3 runtime camera index; default `0` | No | `AT_Open(camera_index)` |
| `width`, `height` | Camera | `PixelCount` | px | R/W config | Positive AOI dimensions; read/write maps to `AOIWidth`/`AOIHeight` when the verified runtime is enabled | Yes | SDK3 runtime feature API |
| `exposure` | Camera | `TimeInterval` | s | R/W config | Positive exposure interval; maps to `ExposureTime` | Yes | SDK3 runtime feature API |
| `frame_count` | Camera | `I64` | frames | R/W config | Positive frame count; maps to `FrameCount` | Yes | SDK3 runtime feature API |
| `pixel_format` | Camera | `String` | none | R/W config | `Mono12`, `Mono12Packed`, `Mono16`, or `Mono32`; maps to `PixelEncoding` | Yes | SDK3 runtime feature API |
| `cycle_mode` | Camera | `String` | none | R/W config | `Fixed` or `Continuous`; maps to `CycleMode` | Yes | SDK3 runtime feature API |
| `trigger_mode` | Camera | `String` | none | R/W config | `Internal`, `Software`, `External`, `ExternalStart`, or `ExternalExposure`; maps to `TriggerMode` | Yes | SDK3 runtime feature API |
| `sensor_cooling` | Cooler | `Bool` | none | R/W config | On/off cooler request; maps to `SensorCooling` | Yes | SDK3 runtime feature API |
| `temperature_control` | Cooler | `String` | none | R/W config | Vendor runtime temperature-control enum/string; `TemperatureControl` target writes round Celsius to the nearest integer string | Yes | SDK3 runtime feature API |
| `sensor_temperature` | Cooler | `Temperature` | deg C | R | `Null` for configured-only operation; current sensor temperature when the verified runtime is enabled | No | SDK3 runtime `SensorTemperature` |
| `temperature_status` | Cooler | `String` | none | R | `configured` for configured-only operation; otherwise vendor runtime temperature status string | No | SDK3 runtime `TemperatureStatus` |
| `capture_gate`, `cooler_gate`, `support_level` | Camera/cooler/hub | `String` | none | R | Current evidence summary | No | Runtime metadata |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "andor_sdk3"` | Yes | string | Selects the SDK3 Andor provider |
| `property.status_byte`, `property.sdk3_status_word` | No | `I64` or decimal/`0x` string | Configured EP0 status metadata used when live USB is not connected |

## Remaining Work

| Area | Gap |
| --- | --- |
| Native write/acquisition framing | Native USB feature setters and acquisition commands require the EP0/bulk command framing; the implemented control path uses the documented vendor runtime ABI instead |
| Feature-register map | Native feature-register addresses, types, and scaling are not recorded; standard feature access is available through the verified vendor runtime |
| Firmware package | With `connect=true`, `firmware_loaded=false`, `firmware_blob_path`, and verified `firmware_blob_sha256`, configured discovery parses the Cypress FX3 `CY` image, writes sections with vendor request `0xA0` in 4096-byte chunks, and jumps to the entry address as a hidden initialization step. The driver must observe an Andor runtime VID/PID after renumeration or fail. This is not exposed as a public or advanced command. |
| Hardware validation | Record model, firmware/driver package, USB descriptors, register/status readback, vendor-runtime capture behavior, abort behavior, and final safe state |
