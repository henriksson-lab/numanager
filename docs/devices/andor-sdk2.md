# Andor SDK2 Cameras

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::andor_camera` |
| Families | Andor/Oxford Instruments SDK2 CCD/ICCD/EMCCD cameras |
| Support level | Andor VID/PID USB discovery, runtime package checks, config-gated hidden firmware initialization from ambiguous EZ-USB loaders, EP0 identity/status/FIFO/acquisition helpers, opt-in live bulk-IN `Mono16` capture, and vendor-runtime exposure, full-frame capture, detector readback, and temperature/cooler control; native SDK-free exposure/register-window controls are not exposed because register mappings are absent |
| Protocol evidence | Reverse engineered Andor behavior plus public Cypress EZ-USB loader evidence |
| Transport | USB userspace via `nusb`; EP0 vendor control, bulk-IN `0x82` for image readout, bulk-OUT `0x01` for firmware/FPGA upload only |
| Discovery | Config-backed candidates plus passive `os-usb` descriptor scanning for Andor VID/PID. Generic Cypress FX2 loader ID `04b4:8613` is reported by the EZ-USB loader discovery as ambiguous, not as Andor, unless config-gated firmware initialization observes an Andor runtime VID/PID |
| Validation | Hardware validation note pending |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `<label> hub` | `hub`, `usb.camera`, `camera.controller` | Owns EP0 control, runtime package metadata, hidden firmware initialization, and frame bulk-IN resources |
| `<label>` | `camera`, `camera.scientific`, `detector.mono`, `andor.sdk2` | SDK2 camera device; advertises `CameraCapture` when the PID maps to SDK2; verified runtime backend supplies exposure setup and detector readback when enabled |
| `<label> cooler` | `temperature.controller`, `cooler`, `state.device` | SDK2 cooler device backed by verified vendor-runtime temperature functions; native register mapping is not used |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `CameraCapture` | Camera | `CapabilityRequest::CameraCapture` or `None`; `Native`, `Mono16`, and `Raw16` encodings only | Frame handle plus width/height/pixel-format/source metadata | With `connect=true`, `load_vendor_runtime=true`, and verified `vendor_runtime_sha256`, uses SDK2 `SetAcquisitionMode(1)`, `SetReadMode(4)`, `SetExposureTime`, full-frame `SetImage`, `StartAcquisition`, `WaitForAcquisitionTimeOut`, and `GetAcquiredData16`; otherwise, with `os-usb` and `connect=true`, configured discovery loads verified firmware automatically if the device is in FX2 pre-firmware state, reads identity, resets FIFOs, clears/starts acquisition through `0xC6`, reads padded big-endian 16-bit pixels from bulk-IN `0x82`, then sends stop | Runtime-managed completion; hardware timing validation pending |
| `TemperatureControl` | Cooler | `CapabilityRequest::TemperatureControl` with optional `enabled` and `target` | Changed `sensor_cooling` / `temperature_control` values | With `connect=false`, updates configured cooler state; with `connect=true`, `load_vendor_runtime=true`, and verified `vendor_runtime_sha256`, uses SDK2 `CoolerON`/`CoolerOFF` and `SetTemperature`; `GetTemperature` supplies live temperature/status readback | Runtime-managed completion; hardware timing validation pending |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `product`, `serial_number`, `sdk_family` | Hub/camera | `String` | none | R | `Sdk2` family for known SDK2 PIDs | No | USB descriptor/PID table |
| `vendor_id`, `product_id`, `usb_identity` | Hub | `I64` / `Map` | none | R | Andor VID/PID plus bus/address/serial when discovered | No | USB descriptors |
| `connect` | Hub | `Bool` | none | R | `false` by default; live capture requires `true` | No | Configured live-I/O gate |
| `width`, `height` | Camera | `PixelCount` | px | R | Runtime `GetDetector` geometry when verified runtime is enabled; otherwise configured frame dimensions, default `512 x 512` | No | SDK2 runtime `GetDetector` or configured bulk read sizing |
| `pixel_format` | Camera | `String` | none | R | `Mono16` | No | SDK2 bulk-IN big-endian 16-bit pixels |
| `exposure` | Camera | `TimeInterval` | s | R/W | Positive interval; runtime write requires `connect=true`, `load_vendor_runtime=true`, and verified `vendor_runtime_sha256` | No | SDK2 runtime `SetExposureTime`; native register-window encoding is not exposed |
| `identity`, `status_byte` | Hub | `Bytes` / `I64` | none | R | Configured/readback metadata | No | EP0 `0xB7` and `0xC7` |
| `frame_endpoint`, `status_endpoint`, `bulk_out_endpoint` | Camera | `I64` | none | R | `0x82`, `0x86`, `0x01` | No | SDK2 USB endpoints |
| `capture_gate` | Camera | `String` | none | R | capture availability summary | No | Firmware upload, FIFO reset, and acquisition-control requests are hidden driver-internal steps |
| `readout_alignment`, `readout_bytes_per_pixel` | Camera | `PixelCount` / `I64` | px / bytes | R | 512 pixels, 2 bytes | No | Bulk read sizing |
| `sensor_cooling` | Cooler | `Bool` | none | R/W config | On/off cooler request; runtime read maps `GetTemperature` status `Off` to `false` | Yes | SDK2 runtime `CoolerON`/`CoolerOFF` and `GetTemperature` |
| `temperature_control` | Cooler | `String` | deg C | R/W config | Integer Celsius target, conservatively prechecked as `-120..=30` and range-checked by `GetTemperatureRange` at runtime | Yes | SDK2 runtime `SetTemperature` |
| `sensor_temperature` | Cooler | `Temperature` | deg C | R | `Null` for configured-only operation; current sensor temperature when the verified runtime is enabled | No | SDK2 runtime `GetTemperature` |
| `temperature_status` | Cooler | `String` | none | R | `configured` for configured-only operation; otherwise `Off`, `NotStabilized`, `Stabilized`, `NotReached`, `OutOfRange`, or `Drift` from live readback | No | SDK2 runtime `GetTemperature` return code |
| `vendor_runtime_*`, `firmware_blob_*`, `package_*`, `third_party_notice` | Hub | typed metadata | none/bytes | R/config | Runtime package status, firmware package status, digest, loadability, and ABI-symbol state | No | Optional third-party package metadata; firmware upload is hidden and initialization-only |
| `capture_gate`, `cooler_gate`, `support_level` | Camera/cooler/hub | `String` | none | R | Current evidence summary | No | Runtime metadata |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "andor_sdk2"` | Yes | string | Selects the SDK2 Andor provider; `driver = "andor"` and `driver = "andor_camera"` remain accepted aliases |
| `property.vendor_id`, `property.product_id` | No | `I64` or decimal/`0x` string | USB IDs; invalid u16 values are rejected instead of silently falling back |
| `property.width`, `property.height` | No | `PixelCount` or positive `I64` | Configured frame dimensions used for live bulk read sizing |
| `property.connect` | No | bool | Enables live USB capture when `os-usb` is compiled; default `false` |
| `property.vendor_runtime_path`, `property.vendor_runtime_sha256` | Required for runtime-backed exposure/capture/detector/cooler control | string | Third-party vendor runtime package identity |
| `property.load_vendor_runtime` | Required for runtime-backed exposure/capture/detector/cooler control | bool | Enables verified SDK2 runtime calls; default `false` |
| `property.sensor_cooling`, `property.temperature_control` | No | bool / integer string | Initial cached cooler request state and target |
| `property.firmware_blob_path`, `property.firmware_blob_sha256` | Required only for config-gated pre-firmware FX2 initialization | string | Config-only firmware package identity; not exposed as public properties or commands. The packaged SDK2 default candidate is `data/third_party/andor/fx2_AndorCam.hex` with SHA-256 `08430b0259a6cd9f73ece020e42a140c6a7f615e03510e999952d3da0e47ac23`; alternate packaged helper images are recorded in `data/third_party/andor/manifest.toml` |

## Remaining Work

| Area | Gap |
| --- | --- |
| Acquisition sub-codes | `0xC6` start/stop/clear sub-values are inferred from the reverse engineered SDK2 crate and should be confirmed on hardware |
| Detector/register windows | Native SDK-free exposure, ROI/detector control, and capability readback need register-window or head-EEPROM mapping evidence before SDK-free public writable properties are advertised |
| Firmware upload | Cypress FX2 RAM loading is evidenced by the public EZ-USB TRM and recorded in `docs/reverse/ez-usb-renumeration.md`. Upload is an internal initialization step after configured SHA-256 verification; the driver must observe an Andor runtime VID/PID after renumeration or fail. Stage-2 FPGA upload and model-specific package selection still need evidence |
| Hardware validation | Record model, firmware/driver package, USB descriptors, one capture, completion/status behavior, abort behavior, and final safe state |
