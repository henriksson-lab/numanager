# Photometrics PVCAM Protocol Evidence Note

## Status

| Field | Value |
| --- | --- |
| Target | Photometrics/QImaging PVCAM cameras |
| Evidence class | Reverse engineered notes for the native USB host-command layer; open GPL kernel-driver source for the PCIe ioctl surface |
| Current state | `numanager_drivers::photometrics_pvcam` exposes configured discovery/evidence, opt-in vendor-runtime loadability, camera-name discovery, writable exposure, one-shot capture, repeated one-shot stream, and temperature read/setpoint |
| Hardware validation | **None.** No bench run and no captured traffic from a physical device |
| Next evidence | A documented bench run of the runtime path, or legal-reviewed native USB/PCIe traces |
| Feasibility | Strong as an optional, user-configured library-backed adapter. SDK-free native transport stays closed while host-command, completion, and frame-ownership evidence is absent |

## Protocol Evidence Summary

| Area | Finding |
| --- | --- |
| Device model | Parameter-driven: availability, runtime type, access mode, range, increment, and enum choices are probed per parameter before use |
| Operation surface | Init/uninit, camera enumeration and open/close, parameter get/set, sequence setup/start/finish, end-of-frame notification, continuous acquisition |
| Native USB identity | VID `0x1f12`; control OUT request `0xd4`, control IN request `0xd5` |
| Native USB frame | Class `0x3f`, little-endian length, begin `0x26`, command code at offset 4, payload/response, end `0x28` |
| Native PCIe | Character devices `/dev/pvcam_pcie*`; ioctl command, status and acquisition surfaces; shared command/response buffers; begin/end-of-frame notification |
| Host command map | Command codes known for temperature, temperature setpoint, readout port, gain index, exposure time, CCL upload, sequence start, and stop |
| **Missing wire evidence** | Completion semantics, native frame layout and ownership, fault/safety vocabulary |

## Evidence To Collect

| Evidence | Required observations |
| --- | --- |
| Runtime binding | Loadability, version, init/open behavior, camera-name list, parameter probe output, error handling, close/uninit |
| Single capture | Sequence setup/start, end-of-frame or wait path, frame buffer shape, sequence finish, abort, returned frame handle |
| Continuous stream | Ring-buffer ownership, callbacks, dropped frames, timestamps, buffer recycling, final stream status |
| Cooler | Fan/cooler status plus the stabilization and fault vocabulary. Temperature readback and setpoint are already implemented |
| Native USB/PCIe | Legal-reviewed traces pairing raw traffic with runtime output and hardware readback over the same action window |

## Protocol Questions

| Area | Questions |
| --- | --- |
| Runtime binding | Whether an optional user-configured PVCAM library backend is acceptable under the no-SDK default policy |
| Native USB | Whether the host-command map plus traces suffice to implement capture safely without proprietary code |
| Native PCIe | Whether relying on the GPL kernel driver's ioctl ABI is acceptable and portable |
| Capture | Frame layout, metadata, bit depth, multi-ROI ordering, end-of-frame completion, abort states |
| Safety | Cooler/fan/shutter/trigger/EM-gain fault states and safe disable behavior |

## Candidate Public Surface

| Device | Capabilities | Properties |
| --- | --- | --- |
| PVCAM hub | runtime checks, camera-name discovery via the optional backend | `camera_name`, `product`, `serial_number`, `firmware_version`, `interface_type`, `support_level` |
| Camera | `CameraCapture`; repeated one-shot `CameraStream`; native continuous stream once evidenced | `exposure`, `pixel_format`, `sensor_width`, `sensor_height`, `bit_depth`; ROI, binning, readout port, speed and gain need parameter evidence |
| Cooler | `TemperatureControl` via the optional backend | `sensor_temperature`, `temperature_setpoint`; fan/status later |

Use typed units — `TimeInterval`, `PixelCount`, `Temperature` — and canonical
pixel formats such as `Mono16`.

## Stop/Proceed Decision

| Decision | Condition |
| --- | --- |
| Proceed with optional library binding | Licensing/deployment policy accepts PVCAM as an optional non-default backend, and a bench run records discovery, parameters, capture, stream, and safe shutdown |
| Proceed with native SDK-free transport | Legal review plus traces prove command framing, acquisition setup, completion, frame layout, and safety behavior |
| Block capture/control | Neither an accepted backend policy nor behavior evidence exists |

## Implementation Gate

Native continuous `CameraStream`, broader parameter writes, raw host commands,
CCL/SCCL upload, reset/maintenance operations, and native USB/PCIe transport stay
unadvertised until the evidence above is recorded in the device page and example
output. Writable `exposure` changes only the timed-mode value used by the
optional-runtime one-shot capture and repeated one-shot stream path; writable
`temperature_setpoint` is applied only after availability, access, and range
checks pass.
