# Hardware Follow-up

This list tracks concrete hardware models that appear likely to matter for
near-term bring-up but are not yet model-validated support claims. Keep entries
as implementation targets until evidence is recorded in device pages, reverse
notes, hardware validation notes, captured traces, or bench logs.

## Squid Control Camera Targets

Source context: `aicell-lab/squid-control` main branch at commit
`5772b44d281130d59fc411f5f3596c72880a460d` includes Squid+ configurations
with `camera_type = Toupcam`, `main_camera_model = ITR3CMOS26000KMA`,
`camera_sensor = IMX571`, `focus_camera_model = MER2-630-60U3M`, and
`focus_camera_type = Default`. In that codebase, `Toupcam` routes through the
Toupcam SDK wrapper, while `Default` routes through Daheng/GxiPy.

| Model | Role | Likely backend path | Current numanager status | Follow-up |
| --- | --- | --- | --- | --- |
| ToupTek/Toupcam `ITR3CMOS26000KMA` with Sony `IMX571` | Main Squid+ camera | Extend `numanager_drivers::toupcam`; consider optional Toupcam SDK/runtime backend if required for cooled-camera features | Family-level Toupcam live USB support exists, but this model is not explicitly listed, hardware-validated, or covered for the Squid Control feature surface | Record device identity and vendor package/license boundary; validate live open, geometry, RAW8/Mono8 and 16-bit modes, exposure, gain, binning, ROI, trigger mode, frame callbacks/streaming behavior, temperature readback, TEC setpoint, fan control, and black level against real hardware or captured traffic |
| Daheng MERCURY2 `MER2-630-60U3M` | Laser autofocus/focus camera | Add Daheng Galaxy/GxiPy-compatible optional vendor-runtime backend, or validate a standards-first `usb3_vision`/`genicam` path if the camera exposes enough USB3 Vision/GenICam behavior | Generic USB3 Vision and GenICam bring-up paths exist, but no Daheng Galaxy runtime backend, exact model support entry, or real MER2 validation exists | Record USB identity, serial/model matching, vendor package/license boundary, and Galaxy SDK/API evidence; validate open by model/serial, Mono8/Mono12/Mono16 or actual supported pixel formats, exposure, gain, ROI/offsets, continuous/software/hardware trigger, Line2/Line3 trigger/strobe behavior, frame ID/timestamp behavior, and focus-camera timing requirements |

## ImSwitch DAQmx Targets

Source context: `ImSwitch/ImSwitch` default branch at commit
`37bf1df6de0ee4746b05bef9684c782e4995b2e8` includes a Python
`NidaqManager` that imports `nidaqmx` and creates NI-DAQmx AO, DO, AI, CI, and
CO tasks for ImSwitch microscope timing, scan, APD, laser, and positioner roles.

| Target | Role | Likely backend path | Current numanager status | Follow-up |
| --- | --- | --- | --- | --- |
| `imswitch_daqmx` hub | ImSwitch-style NI-DAQmx device container | Separate `numanager-imswitch-daqmx` crate with optional NI-DAQmx runtime probe backend | Configured descriptor/state model exists; with `ni-daqmx-sdk`, `connect=true` loads the vendor runtime and reports the detected NI-DAQmx version. Header/API audit is recorded in `docs/devices/ni-daqmx-sdk-api-audit.md`; Linux device inventory is guarded because NI-PAL can abort the process | Bench-validate task lifecycle, routing, completion/error semantics, and safe stop/clear behavior; then implement real task execution |
| `imswitch_daqmx` AO child devices | Galvo, piezo, AOM/AOTF analog control | NI-DAQmx AO voltage channels | Descriptor/state model only | Validate physical channel naming, voltage range, finite buffered writes, sample-clock source, trigger behavior, idle output, and scaling |
| `imswitch_daqmx` DO/TTL child devices | Laser gates, camera triggers, line clocks, frame clocks, shutters | NI-DAQmx digital output lines | Descriptor/state model only | Validate line naming, TTL levels, finite buffered writes, start trigger, idle/safe state, and camera/laser timing |
| `imswitch_daqmx` AI child devices | Monitor/focus analog input | NI-DAQmx AI voltage channels | Descriptor/state model only | Validate range, terminal configuration, sample clock, finite/continuous read behavior, and timeout/error behavior |
| `imswitch_daqmx` CI child devices | APD photon counting | NI-DAQmx counter input tasks | Descriptor/state model only | Validate terminal routing, edge selection, finite sample-clocked reads, count rollover, DMA/read timeout behavior, and APD timing |
| `imswitch_daqmx` CO child devices | Sample clock and pulse train generation | NI-DAQmx counter output tasks | Descriptor/state model only | Validate frequency accuracy, finite pulse count, arm/start trigger behavior, stop behavior, and routing into CI/AO/DO tasks |
| `ConfocalImageCapture` API | Final reconstructed laser-scanning confocal image/stack | `imswitch_daqmx` hub service over AO/DO/CI/CO children | First-class capability/request type exists; current crate returns configured API summary only | Bind scan geometry, reconstruction parameters, DAQ task execution, sample-to-pixel binning, and runtime frame-store output after NI task behavior is evidenced |
| `ConfocalImageStream` API | Live reconstructed image updates during scan | `imswitch_daqmx` hub service with mutable frame/dirty-region updates | First-class capability/request type exists; current crate returns configured API summary only | Validate overwrite/dirty-region semantics, update cadence, frame handle lifecycle, and behavior for slow versus fast scans |
| `ScanSignalStream` API | Raw timed detector/DAQ sample stream for non-standard scan cycles | `imswitch_daqmx` hub service over CI/AI/DI timing | First-class capability/request type exists; current crate returns configured API summary only | Define stream chunk metadata, timebase, channel identifiers, trigger/task provenance, overflow/drop behavior, and offline reconstruction handoff |
