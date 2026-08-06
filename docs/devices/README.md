# Device Support Matrix

This directory holds per-driver device pages. Each page uses the same table
shape so capabilities and properties can be scanned without reading the driver
source.

The cross-driver audit view is [`evidence.md`](evidence.md). Use that register
before adding protocol behavior, tests, or claimed hardware support.

Use [`hardware-validation-template.md`](hardware-validation-template.md) for
bench notes or captured-trace summaries that move a feature toward real
hardware support.
For protocol-evidence targets waiting on serial, HID, USB, or frame traffic,
use [`../reverse/trace-capture-guide.md`](../reverse/trace-capture-guide.md)
before promoting a command into a driver.

Whenever firmware, a loader, or a vendor runtime is required, the default
interim solution is to ship or load the original vendor package as
third-party excluded data behind an explicit optional backend when a
project-owned firmware or open replacement is not available. Treat this as the default
implementation path for every firmware-dependent device, not as a
device-specific exception. That package can support backend bring-up, but it
does not by itself prove command behavior, reply semantics, state transitions,
or unit conversions.

Firmware upload, bootloader entry, reset, factory/default restore, flash/DFU,
and similar maintenance operations are not normal driver commands. If a backend
must perform one, keep it hidden behind verified initialization or another
driver-internal path. Do not expose these operations through regular or advanced
command browsers. Read-only firmware identity/readback properties are still
ordinary diagnostic metadata.

## Page Format

| Section | Purpose |
| --- | --- |
| Status/provenance | Support level, protocol evidence, transport, discovery, validation, runtime requirements, and evidence gaps |
| Logical devices | Advertised devices, kind tags, graph/dependency role |
| Resources | Driver-owned command/data resources and remultiplexing paths |
| Capabilities | Capability kind, device, request, response, completion, timing behavior |
| Properties | Property key, device, value type, unit, access, range/enums/increment, sequenceable, wire mapping |
| Examples | Example binaries and demonstrated workflows |
| Remaining work | Hardware validation, protocol gaps, safety gaps, model-specific limitations |

Static property ranges and increments are part of `PropertySchema` and are
validated by the runtime in canonical units before a command reaches a driver.
Dynamic hardware constraints, such as GenICam node references whose value can
change at runtime, must still be validated inside the driver.

Devices that expose safety-relevant properties should prefer the common keys
`enabled`, `interlock_closed`, `emission_permitted`, `fault_active`, `fault`,
and specific fault flags such as `interlock_fault`, `overtemperature_fault`,
`overpressure_fault`, or `gas_fault`. `LocalRuntime::safety_summary()` reads
these advertised properties and returns a normalized `SafetySummary` while
preserving the raw readback values.

Camera stream capabilities should feed frames through the runtime frame store.
`Runtime::stream_status()` provides the common pull-style view of retained
frame handles, ring depth, capacity, overflow policy, and dropped-frame count;
frame events and telemetry remain the push path.

## Supported Drivers

Rows described as reverse engineered support expose only the hardware operations
that the linked page records as evidenced. Unsupported operations fail closed
rather than being advertised as public capabilities.

| Driver module | Device family | Support level | Device page | Public workflow example |
| --- | --- | --- | --- | --- |
| `numanager_drivers::abs_camera` | ABS legacy USB cameras | Runtime-package evidence with file-status/digest/loadability/ABI-symbol checks, writable exposure setting, explicit async software trigger, opt-in vendor-runtime capture, and repeated one-shot stream support; native transport, native continuous streaming, gain controls, persistent trigger modes, and broader acquisition behavior is not exposed because USB protocol evidence is absent | [abs-camera.md](abs-camera.md) | `discover_devices` |
| `numanager_drivers::agilent_laser_combiner` | Agilent/Keysight Laser Combiner | Implemented from external protocol evidence with typed control paths and mapped readback helpers | [agilent-laser-combiner.md](agilent-laser-combiner.md) | `discover_devices`, `light_source` |
| `numanager_drivers::andor_camera` | Andor/Oxford Instruments SDK2 cameras | USB discovery, hidden firmware initialization, firmware/runtime package checks, EP0 identity/status/FIFO/acquisition helpers, opt-in live bulk-IN `Mono16` capture, and vendor-runtime exposure, full-frame capture, detector readback, and temperature/cooler control; native SDK-free exposure/register-window controls are not exposed because register mappings are absent | [andor-sdk2.md](andor-sdk2.md) | `discover_devices`, `environment_control` |
| `numanager_drivers::andor_camera` | Andor/Oxford Instruments SDK3 cameras | USB discovery, hidden FX3 firmware initialization, confirmed EP0 status readbacks, firmware/runtime package checks, vendor-runtime feature control/readback, cooler control, and opt-in `Mono16` capture | [andor-sdk3.md](andor-sdk3.md) | `discover_devices`, `environment_control` |
| `numanager_drivers::asi` | ASI MS-2000/RM-2000 and ASI Tiger | Configured opt-in serial stage control/readback, Tiger TTL/ring-buffer control, CRISP autofocus control/readback, and hidden coordinate-reference maintenance command | [asi.md](asi.md) | `motion_stage`, `autofocus` |
| `numanager_drivers::bluebox_niji` | Bluebox Optics niji LED illuminator | Opt-in serial startup query, output control, status/temperature/readback refresh helpers, and connected write-path status readback | [bluebox-niji.md](bluebox-niji.md) | `discover_devices`, `light_source` |
| `numanager_drivers::three_z_optics` | 3Z Optics IRIS LED light sources | Source-backed configured discovery plus opt-in Modbus-style serial control/readback for mode, global output, global intensity, channel output, channel intensity, model id, and dirty-bit refresh | [3z-optics.md](3z-optics.md) | `discover_devices`, `light_source` |
| `numanager_drivers::pi_gcs` | Physik Instrumente GCS/GCS2 controllers | Configured opt-in serial motion/home/stop, servo/profile/reference/status readback, typed velocity/acceleration settings, timing endpoint hooks, and refresh helpers | [pi-gcs.md](pi-gcs.md) | `motion_stage` |
| `numanager_drivers::squid` | Cephla Squid/Octopi controller | Protocol-backed control plus opt-in configured real serial backend and documented generic command aliases | [squid.md](squid.md) | `squid`, `autofocus` |
| `numanager_drivers::chuo_seiki_qt` | Chuo Seiki QT stages | Opt-in serial startup identification, stage writes, busy/position/readback refresh helpers, and known-format typed position-state readback | [chuo-seiki-qt.md](chuo-seiki-qt.md) | `discover_devices`, `motion_stage` |
| `numanager_drivers::corvus` | ITK/Marzhauser Corvus stages | Opt-in serial startup readback, stage move/home/stop writes, refresh helpers, and known numeric position/speed/acceleration readback parsing | [corvus.md](corvus.md) | `discover_devices`, `motion_stage` |
| `numanager_drivers::egrabber_framegrabber` | Euresys eGrabber frame grabbers | Configured GenTL producer file/digest/ABI checks plus default-off SDK interface/device inventory; capture and stream acquisition are not exposed yet | [egrabber-framegrabber.md](egrabber-framegrabber.md) | `discover_devices` |
| `numanager_drivers::thorlabs_kurios` | Thorlabs KURIOS LCTF | Configured opt-in serial control/readback and refresh helpers | [thorlabs-kurios.md](thorlabs-kurios.md) | `filters` |
| `numanager_drivers::toupcam` | Toupcam/AmScope-like USB cameras | Config-backed geometry/identity plus live userspace USB backend behind `os-usb` with retained USB identity metadata, per-model profiles (U3CMOS08500KPA, U3CMOS03100KPA), and local frame source | [toupcam.md](toupcam.md) | `discover_devices`, `camera_acquisition`, `camera_stream` |
| `numanager_drivers::platform_camera` | OS camera backends | Descriptor-only Linux V4L2 discovery, explicit configured V4L2 `read()` capture/stream for fixed-size raw frames, and local PGM/PPM frame source | [platform-camera.md](platform-camera.md) | `camera_acquisition`, `camera_stream`, `discover_devices` |
| `numanager_drivers::gige_vision` | GigE Vision cameras | GVCP/GVSP command/frame model with optional local PGM/PPM frame source plus opt-in UDP GVCP mapped-property and raw-register control | [gige-vision.md](gige-vision.md) | `camera_acquisition`, `camera_stream`, `discover_devices` |
| `numanager-imswitch-daqmx` | NI-DAQmx devices used by ImSwitch-style microscope setups | Separate niche crate with configured descriptor/state model, package-intake notes, and optional NI-DAQmx runtime-version probe; live task execution remains gated on API audit and hardware validation | [imswitch-daqmx.md](imswitch-daqmx.md) | `lsm_confocal_capture`, `lsm_confocal_stream`, `lsm_signal_stream`, `daqmx_runtime_probe` |
| `numanager_drivers::hamilton_mvp` | Hamilton Serial MVP valve positioners | Protocol 1/RNO+ configured serial startup/readback with configured daisy-chain aggregation | [hamilton-mvp.md](hamilton-mvp.md) | `discover_devices`, `fluidics` |
| `numanager_drivers::usb3_vision` | USB3 Vision cameras | U3V control/stream/event model with optional local PGM/PPM frame source plus opt-in USB identity/open/endpoint-catalog and command-endpoint ReadMem/WriteMem path | [usb3-vision.md](usb3-vision.md) | `camera_acquisition`, `camera_stream`, `discover_devices` |
| `numanager_drivers::spark_cyto` | Spark Cyto | TDCL/CAN graph and transaction model for plate, detector, environment, imaging-head, and camera-binding workflows; physical backend is not exposed because transport binding is not evidenced | [spark-cyto.md](spark-cyto.md) | `spark_cyto`, `environment_control`, `plate_reader` |
| `numanager_drivers::arduino` | Micro-Manager Arduino controller | Firmware protocol control plus opt-in configured real serial startup readback, output/control writes, ADC/digital input readback, and refresh helpers | [arduino.md](arduino.md) | `discover_devices`, `digital_io` |
| `numanager_drivers::arduino_counter` | Arduino Counter | Counter/pulse protocol control plus opt-in configured real serial snapshot/count readback and refresh helper | [arduino-counter.md](arduino-counter.md) | `discover_devices`, `digital_io` |
| `numanager_drivers::esp32` | Micro-Manager ESP32 controller | Firmware protocol control plus opt-in configured real serial startup readback, GPIO/PWM/shutter/motion writes, ADC readback, and position refresh helpers | [esp32.md](esp32.md) | `discover_devices`, `motion_stage`, `digital_io`, `shutter` |
| `numanager_drivers::evident_ix85` | Evident/Olympus IX85 microscope body | Configured opt-in serial focus motion/stop, state-device selection, shutter control, software timing endpoints, body readback, and hub refresh commands; ZDC status readback is exposed, while autofocus actions are not exposed because `AF` parameter semantics are absent | [evident-ix85.md](evident-ix85.md) | `discover_devices`, `motion_stage`, `filters`, `shutter` |
| `numanager_drivers::openuc2` | OpenUC2 Feather controller | JSON-line motion/light control plus opt-in configured real serial startup readback, typed wavelength metadata, and state refresh helper | [openuc2.md](openuc2.md) | `discover_devices`, `motion_stage`, `light_source` |
| `numanager_drivers::openstage` | OpenStage | Published serial command-table support for XYZ motion, settings, beep, and optional startup readback; coordinate-zeroing remains hidden from regular and advanced command surfaces | [openstage.md](openstage.md) | `discover_devices`, `motion_stage` |
| `numanager_drivers::wosm` | Warwick Open-Source Microscope | v0.900 command-page-backed TCP stage, digital output/input, and DAC destination light control plus legacy source-backed switch-sequence, blanking, pull-up, and raw analog-input support | [wosm.md](wosm.md) | `discover_devices`, `motion_stage`, `light_source`, `digital_io` |
| `numanager_drivers::xeryon` | Xeryon ASCII piezo stages | Manufacturer-documented ASCII serial motion/readback with configured axes, optional serial backend, status-bit decoding, and selected refresh helpers | [xeryon.md](xeryon.md) | `discover_devices`, `motion_stage` |
| `numanager_drivers::xeryon_canopen` | Xeryon integrated CANopen stages | CiA 402 transaction planning plus optional live SocketCAN/SLCAN NMT/SDO execution and EDS object parsing for integrated XLA/XUMU motion | [xeryon-canopen.md](xeryon-canopen.md) | `discover_devices`, `motion_stage` |
| `numanager_drivers::sutter_mp285` | Sutter MP-285 | Configured opt-in serial control/readback and refresh helpers | [sutter-mp285.md](sutter-mp285.md) | `motion_stage` |
| `numanager_drivers::sutter_stage` | Sutter/Ludl-compatible controller | Configured opt-in serial move/home/stop control, readback, and refresh helpers | [sutter-stage.md](sutter-stage.md) | `motion_stage`, `autofocus` |
| `numanager_drivers::prior` | Prior ProScan/OptiScan | Configured opt-in serial stage, NanoScan Z, filter, shutter, TTL, Lumen, native speed/acceleration, readback, and refresh helpers | [prior.md](prior.md) | `motion_stage`; `filters` |
| `numanager_drivers::marzhauser` | Marzhauser TANGO/L-Step | Configured opt-in serial stage move/home/stop control, readback, and refresh helpers | [marzhauser.md](marzhauser.md) | `motion_stage` |
| `numanager_drivers::mcl` | Mad City Labs MicroDrive/NanoDrive | Active USB descriptor discovery plus opt-in MicroDrive raw encoder/status readback, fixed-length raw MicroDrive control-read/action commands, and firmware/runtime package checks; typed stage motion is not exposed because units, status, and completion evidence are absent | [mcl.md](mcl.md) | `discover_devices` |
| `numanager_drivers::zaber` | Zaber ASCII stages | ASCII motion/readback with configured/probed axes and selected refresh helpers, optional serial feature | [zaber.md](zaber.md) | `motion_stage` |
| `numanager_drivers::standa` | Standa 8SMC4 | Single-axis serial motion/readback with startup status plus movement, engine, brake, and home settings refresh helpers | [standa.md](standa.md) | `discover_devices`, `motion_stage` |
| `numanager_drivers::starlight_xpress` | Starlight Xpress filter wheels | Spec-backed serial and explicit/autodiscovered USB HID report control with startup/runtime readback helpers | [starlight-xpress.md](starlight-xpress.md) | `discover_devices`, `filters` |
| `numanager_drivers::thorlabs_apt` | Thorlabs APT motors | Configured opt-in serial APT motion/home/stop, status, position, identity, velocity-profile, keep-alive, readback, and refresh helpers | [thorlabs-apt.md](thorlabs-apt.md) | `motion_stage` |
| `numanager_drivers::trinamic_tmcl` | Trinamic/ADI TMCL stages | Startup/runtime-refresh direct-mode serial control with raw firmware-version readback and GAP helpers, optional configured serial feature | [trinamic-tmcl.md](trinamic-tmcl.md) | `discover_devices`, `motion_stage` |
| `numanager_drivers::cobolt` | Cobolt/Hubner lasers | Configured opt-in serial laser control/readback and refresh helpers | [cobolt.md](cobolt.md) | `laser`, `light_source` |
| `numanager_drivers::coherent_obis` | Coherent OBIS lasers | Configured opt-in serial laser control/readback and refresh helpers | [coherent-obis.md](coherent-obis.md) | `laser`, `light_source` |
| `numanager_drivers::omicron` | Omicron serial lasers | Configured opt-in serial control/readback and refresh helpers | [omicron.md](omicron.md) | `laser`, `light_source` |
| `numanager_drivers::coolled` | CoolLED pE series | pE-300/pE-4000/pE-340 configured opt-in serial control/readback and refresh helpers | [coolled.md](coolled.md) | `light_source` |
| `numanager_drivers::lumencor` | Lumencor Spectra/SpectraX/CIA | Configured opt-in serial startup/setup readback plus CIA info readback and CIA command helpers | [lumencor.md](lumencor.md) | `light_source` |
| `numanager_drivers::lumenera` | Lumenera Lu130 / Bio-Rad Gel Doc EZ cameras | USB descriptor discovery for both stages, hidden EZ-USB firmware initialization when explicitly connected, and live `CameraCapture` with writable `exposure` from captured-traffic evidence; `gain` fails closed because its register mapping is unevidenced | [lumenera.md](lumenera.md) | `gel_doc` |
| `numanager_drivers::spectral_lmm5` | Spectral LMM5 | Startup-readback shutter/transmission/wavelength/trigger-profile RS-232 control with hub refresh/apply helpers, optional serial feature | [spectral-lmm5.md](spectral-lmm5.md) | `discover_devices`, `light_source` |
| `numanager_drivers::teensy_pulse` | Teensy pulse generator | Binary pulse control plus opt-in configured real serial startup/program readback path and enquiry refresh helpers | [teensy-pulse.md](teensy-pulse.md) | `discover_devices`, `digital_io` |
| `numanager_drivers::thorlabs_dc` | Thorlabs LED controllers | Opt-in serial and explicit-config DC2200 USBTMC control/readback with refresh helpers | [thorlabs-dc.md](thorlabs-dc.md) | `light_source` |
| `numanager_drivers::thorlabs_sc10` | Thorlabs SC10 shutter controller | Configured manufacturer-spec shutter control plus configured opt-in serial startup readback and refresh helpers | [thorlabs-sc10.md](thorlabs-sc10.md) | `discover_devices`, `shutter` |
| `numanager_drivers::triggerscope` | ARC TriggerScope | Opt-in serial startup identification, direct TTL/camera-trigger/DAC/focus control, and constrained timing-program commands | [triggerscope.md](triggerscope.md) | `discover_devices`, `motion_stage`, `digital_io` |
| `numanager_drivers::modbus` | Modbus RTU/TCP mapped IO | Mapped Modbus RTU/TCP IO with configured local register model and explicit real transport | [modbus.md](modbus.md) | `digital_io` |
| `numanager_drivers::velleman` | Velleman K8055/VM110 and K8061/VM140 USB IO boards | Descriptor-discovered or explicit-config USB packet backend with analog, digital, PWM, and counter IO | [velleman.md](velleman.md) | `discover_devices`, `digital_io` |
| `numanager_drivers::mightex_bls` | Mightex Sirius BLS/SLC HID light controllers | HID output driver with typed light control, trigger/strobe setup, and disable-all helper | [mightex-bls.md](mightex-bls.md) | `light_source` |
| `numanager_drivers::mightex_camera` | Mightex buffered USB cameras | Runtime-package evidence with file-status/digest/loadability/ABI-symbol checks, writable capture parameters, opt-in vendor-runtime `Mono16`/`Raw16` capture, and repeated one-shot stream support; native frame transport, native continuous streaming, native gain/color controls, ROI/binning beyond configured frame dimensions, and broader SDK-free acquisition behavior is not exposed because native protocol evidence is absent | [mightex-camera.md](mightex-camera.md) | `discover_devices` |
| `numanager_drivers::genicam` | GenICam node maps | XML/register node-map execution model with maintenance-command filtering and optional local PGM/PPM frame source | [genicam.md](genicam.md) | `camera_acquisition`, `camera_stream`, `discover_devices` |
| `numanager_drivers::okolab` | Okolab environmental controllers | Reverse engineered serial/configured runtime support with opt-in connected read/write and refresh helpers | [okolab.md](okolab.md) | `discover_devices`, `environment_control` |
| `numanager_drivers::opentrons_ot2` | Opentrons OT-2 liquid handling robot | Active HTTP health/inventory/module readback/current-run refresh, constrained run actions, gantry home/absolute move, v2 temperature-module control, and camera snapshot capture | [opentrons-ot2.md](opentrons-ot2.md) | `discover_devices`, `robot_inventory` |
| `numanager_drivers::sim` | Composed autofocus and biological focus-plane model | Biological-model-oriented system simulation | [sim.md](sim.md) | `autofocus`, `biology_simulation` |
| `numanager_drivers::sim_lsm` | Laser-scanning microscope simulation | Confocal capture, image stream, and signal stream simulator over the shared procedural cell-culture model | [sim-lsm.md](sim-lsm.md) | `lsm_confocal_capture`, `lsm_confocal_stream`, `lsm_signal_stream`, `software_gui` |
| `numanager_drivers::sim_microscope_lsm` | Composed brightfield and laser-scanning microscope simulation | Brightfield camera, XY/Z motion, objective, lamp, and LSM APIs in one simulator driver with shared scene state | [sim-microscope-lsm.md](sim-microscope-lsm.md) | `lsm_composed_workflow` |
| `numanager_drivers::sim_microscope` | Composed brightfield microscope simulation | Biological-model-oriented system simulation of one composed microscope: camera, XY/Z motion, objective turret, and lamp sharing a single procedural cell-culture model, publishing the pixel-pitch/binning/magnification calibration chain | [sim-microscope.md](sim-microscope.md) | `software_gui` |
| `numanager_drivers::photometrics_pvcam` | Photometrics/QImaging PVCAM cameras | Configured and active USB evidence, verified runtime camera-name discovery, package checks, writable exposure setting, opt-in one-shot capture, repeated one-shot stream support, and runtime temperature read/setpoint control; native continuous streaming and broader parameter control are not exposed because documented ABI/native-transport evidence is absent | [photometrics-pvcam.md](photometrics-pvcam.md) | `discover_devices` |
