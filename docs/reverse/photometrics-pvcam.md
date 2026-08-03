# Photometrics PVCAM Protocol Evidence Note

## Status

| Field | Value |
| --- | --- |
| Target | Photometrics/QImaging PVCAM cameras |
| Current state | `numanager_drivers::photometrics_pvcam` exposes configured discovery/evidence, explicit vendor-runtime loadability, camera-name discovery, writable exposure setting, one-shot runtime-backed capture, repeated one-shot stream support, and runtime temperature read/setpoint control |
| Better source status | Reverse engineered notes document the PVCAM C ABI/device contract and native USB/PCIe host-command facts |
| Next evidence | PVCAM runtime behavior notes, or legal-reviewed native USB/PCIe traces |
| Feasibility | PVCAM library binding is feasible as an optional SDK/library adapter; SDK-free native transport is not exposed because host-command, completion, and frame-ownership evidence is absent |

## Protocol Evidence Summary

| Area | Finding |
| --- | --- |
| PVCAM ABI | Lifecycle/discovery/open APIs include `pl_pvcam_init`, `pl_cam_get_total`, `pl_cam_get_name`, `pl_cam_open`, `pl_get_param`, `pl_set_param`, sequence setup/start/finish, EOF callbacks, and continuous acquisition |
| Parameter model | Public API requires probing availability, runtime type, access, ranges, increments, and enum choices before using each `PARAM_*` |
| Native USB | Notes record USB VID `0x1f12`, control OUT request `0xd4`, control IN request `0xd5`, and host-command class/framing constants |
| Native host command frame | Recorded shape is class `0x3f`, little-endian length, begin byte `0x26`, command code at offset 4, payload/response, and end byte `0x28` |
| Native PCIe | Notes identify `/dev/pvcam_pcie*`, GPL driver source, ioctl command/status/acquisition surfaces, shared command/response buffers, and EOF/BOF notification concepts |
| Host command map | Local command map ties many PVCAM parameters to host command codes, including temperature, temperature setpoint, readout port, gain index, exposure time, CCL upload, start sequence, and stop |

## Evidence To Collect

| Evidence | Required observations |
| --- | --- |
| PVCAM library binding | Runtime loadability, library version, init/open behavior, camera name list, parameter probing output, error handling, close/uninit behavior |
| Single capture | `pl_exp_setup_seq`, `pl_exp_start_seq`, EOF callback or wait path, frame buffer shape, `pl_exp_finish_seq`, abort behavior, and runtime frame-handle output |
| Continuous stream | Ring-buffer ownership, frame callbacks, dropped-frame behavior, timestamps, buffer recycling, and final stream status |
| Temperature/cooler | Implemented runtime `PARAM_TEMP` readback and `PARAM_TEMP_SETPOINT` read/write; fan/cooler status and stabilization/fault vocabulary remain to collect |
| Native USB/PCIe | Legal-reviewed traces that pair raw traffic with public runtime output and hardware output/readback for the same action window |

## Protocol Questions

| Area | Questions |
| --- | --- |
| SDK binding | Whether an optional `libpvcam` backend is acceptable under the project no-SDK default policy |
| Native USB | Whether the host-command map plus traces are enough to safely implement capture without proprietary code |
| Native PCIe | Whether relying on the GPL kernel driver ioctl ABI is acceptable and portable |
| Capture | Frame layout, metadata, bit depth, multi-ROI ordering, EOF completion, and abort states |
| Safety | Cooler/fan/shutter/trigger/EM-gain fault states and safe disable behavior |

## Candidate Public Surface

| Device | Capabilities | Properties |
| --- | --- | --- |
| PVCAM hub | runtime package checks and camera-name discovery through verified optional backend | `camera_name`, `product`, `serial_number`, `firmware_version`, `interface_type`, `support_level` |
| Camera | `CameraCapture`; repeated one-shot `CameraStream`; native continuous stream after continuous-acquisition evidence | `exposure`, `pixel_format`, `sensor_width`, `sensor_height`, `bit_depth`; ROI, binning, readout-port, speed, and gain require parameter evidence |
| Cooler | `TemperatureControl` for runtime temperature read/setpoint through verified PVCAM parameter APIs | `sensor_temperature`, `temperature_setpoint`; fan/status later |

Public values must use typed units such as `TimeInterval`, `PixelCount`, and
`Temperature`, and canonical pixel formats such as `Mono16`.

## Stop/Proceed Decision

| Decision | Condition |
| --- | --- |
| Current implemented support | Config-backed discovery/evidence properties plus explicit opt-in vendor-runtime loadability, camera-name discovery, writable exposure setting, one-shot capture, repeated one-shot stream support, and runtime temperature read/setpoint control |
| Proceed with optional SDK binding | Licensing/deployment policy accepts PVCAM as an optional non-default backend and behavior evidence records discovery, parameters, capture, stream, and safe shutdown |
| Proceed with native SDK-free transport | Legal review plus traces prove command framing, acquisition setup, completion, frame layout, and safety behavior |
| Block capture/control | No accepted backend policy or behavior evidence exists |

## Implementation Gate

Do not advertise native continuous `CameraStream`, broader PVCAM parameter writes, raw host
commands, CCL/SCCL upload, reset/maintenance operations, or native USB/PCIe
transport before the corresponding evidence above is recorded in the device page
and example output. Writable `exposure` only changes the timed-mode value used by
the verified vendor-runtime one-shot capture and repeated one-shot stream path. Writable
`temperature_setpoint` uses `PARAM_TEMP_SETPOINT` only after runtime
availability, access, and range checks.
