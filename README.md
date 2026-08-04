# nu-manager

`nu-manager` aims to be the next generation of [Micro-manager](https://github.com/micro-manager).

## Design philosophy

* Pure Rust implementation. This gives the speed of C++ but without the gun
* Just a driver collection - a single GUI cannot cover all use cases well, so it's out of scope
* No "core". It's hard to make one-fits-all model of all hardware, so this is delegated to downstream
* A full DAG (direct acyclic graph) of drivers, enabling implementation of metadrivers (autofocus etc)
* All kinds of lab hardware is in scope, as at some point you might want to connect the microscope to a robot or other hardware
* All operation are asynchronous
* Native protocol backends whenever feasible

**Under development: APIs and device coverage are still changing. Real hardware
validation is still needed for most drivers. Device pages state what has and
has not been tested.**

## Testing devices on your hardware

Use the software-test GUI or the generic workflow examples first. If behavior
differs from the device page, capture the device model, firmware version,
configuration, command output, and any hardware log or trace that can anchor a
fix.

## Run Examples

Example commands and recorded outputs are listed in
[`docs/run_examples.md`](docs/run_examples.md).

## Core Model

`nu-manager` treats each instrument as a set of typed devices and operations. A
controller can expose several logical devices, such as a camera, stage, light
source, filter wheel, or autofocus provider, while the driver keeps the hardware
protocol details behind the API.

Applications normally read or set typed properties, invoke capabilities such as
capture or stage movement, and listen for completion/events from the runtime.
The detailed API vocabulary is in [`docs/core_model.md`](docs/core_model.md).

## Device Index

Detailed support, provenance, capability, and property tables live under
[`docs/devices/`](docs/devices/README.md). The cross-driver evidence and
validation audit is tracked in [`docs/devices/evidence.md`](docs/devices/evidence.md).

The hardware table below is the implementation checklist. Each row states the
support currently implemented from available protocol evidence; the hardware
marker only records whether that implementation has also been validated on a
real device. `✓` means validated on hardware, and `-` means not yet validated.
Unknown hardware operations fail explicitly when the device page does not
record enough protocol evidence for an implementation.

### Hardware devices

| Driver family | Implemented support | Tested on hardware |
| --- | --- | --- |
| [ABS legacy USB cameras](docs/devices/abs-camera.md) | Runtime-package evidence, writable exposure, explicit software trigger, opt-in vendor-runtime capture, and repeated-capture stream | - |
| [Agilent/Keysight Laser Combiner](docs/devices/agilent-laser-combiner.md) | Laser control and readback | - |
| [Andor SDK2 cameras](docs/devices/andor-sdk2.md) | USB discovery, firmware/runtime package checks, EP0 control helpers, opt-in live Mono16 capture, and vendor-runtime exposure/detector/cooler control | - |
| [Andor SDK3 cameras](docs/devices/andor-sdk3.md) | USB discovery, hidden FX3 firmware init, EP0 status readback, runtime package checks, vendor-runtime feature control, and Mono16 capture | - |
| [Arduino controller](docs/devices/arduino.md) | Firmware protocol control plus opt-in real serial read/write | - |
| [Arduino Counter](docs/devices/arduino-counter.md) | Counter/pulse protocol control plus opt-in real serial readback | - |
| [ASI MS-2000/Tiger](docs/devices/asi.md) | Serial stage, Tiger TTL/ring-buffer, and CRISP autofocus control/readback | - |
| [Bluebox Optics niji](docs/devices/bluebox-niji.md) | Serial light output and status | - |
| [Cephla Squid/Octopi](docs/devices/squid.md) | Serial controller motion, illumination, trigger, autofocus, and status | - |
| [3Z Optics IRIS](docs/devices/3z-optics.md) | Source-backed Modbus-style serial light-source control and readback | - |
| [Chuo Seiki QT stages](docs/devices/chuo-seiki-qt.md) | Serial stage startup, control, and status | - |
| [ITK Corvus stages](docs/devices/corvus.md) | Serial stage control and status | - |
| [Cobolt/Hubner lasers](docs/devices/cobolt.md) | Serial laser control and telemetry | - |
| [Coherent OBIS lasers](docs/devices/coherent-obis.md) | Serial laser control and telemetry | - |
| [CoolLED pE series](docs/devices/coolled.md) | Serial illumination control and readback | - |
| [ESP32 controller](docs/devices/esp32.md) | Serial GPIO, PWM/shutter, ADC, and XY/Z stage control/readback | - |
| [Euresys eGrabber frame grabbers](docs/devices/egrabber-framegrabber.md) | Configured GenTL producer checks plus default-off SDK interface/device inventory | - |
| [Evident/Olympus IX85](docs/devices/evident-ix85.md) | Serial focus, state-device, shutter, timing endpoints, and body readback/control | - |
| [GenICam node maps](docs/devices/genicam.md) | Node-map execution model with maintenance filtering and local frame source | - |
| [GigE Vision cameras](docs/devices/gige-vision.md) | GVCP/GVSP model plus opt-in UDP GVCP mapped-property and raw-register access | - |
| [Hamilton Serial MVP valves](docs/devices/hamilton-mvp.md) | Serial valve control and readback | - |
| [Lumencor Spectra/SpectraX/CIA](docs/devices/lumencor.md) | Serial illumination control and readback | - |
| [Lumenera Lu130 / Bio-Rad Gel Doc EZ cameras](docs/devices/lumenera.md) | USB discovery and hidden firmware initialization; imaging capture/control awaits wire-protocol evidence | - |
| [Marzhauser TANGO/L-Step](docs/devices/marzhauser.md) | Serial stage control and status | - |
| [Mad City Labs MicroDrive/NanoDrive](docs/devices/mcl.md) | USB descriptor discovery, MicroDrive raw encoder/status readback, fixed-length raw control read/actions, and firmware/runtime package checks | - |
| [Modbus mapped IO](docs/devices/modbus.md) | Modbus RTU/TCP mapped IO with explicit real transport | - |
| [Mightex buffered USB cameras](docs/devices/mightex-camera.md) | Runtime-package evidence, writable capture settings, opt-in vendor-runtime Mono16/Raw16 capture, and repeated-capture stream | - |
| [Mightex Sirius BLS/SLC](docs/devices/mightex-bls.md) | HID light output, trigger/strobe setup, and rule/readback helpers | - |
| [Okolab environmental controllers](docs/devices/okolab.md) | Serial environmental control and readback | - |
| [Omicron serial lasers](docs/devices/omicron.md) | Serial laser control and telemetry | - |
| [Opentrons OT-2](docs/devices/opentrons-ot2.md) | HTTP health, inventory/readback, run actions, gantry home/move, temperature-module control, and camera snapshot | - |
| [OpenStage](docs/devices/openstage.md) | Serial XYZ motion, settings, and readback | - |
| [OpenUC2 Feather](docs/devices/openuc2.md) | JSON-line motion/light control plus opt-in real serial | - |
| [OS/platform cameras](docs/devices/platform-camera.md) | Descriptor-only V4L2 discovery plus explicit V4L2 read capture and local frame source | - |
| [Photometrics/QImaging PVCAM cameras](docs/devices/photometrics-pvcam.md) | USB discovery, verified PVCAM runtime discovery, one-shot capture, repeated-capture stream, and temperature setpoint control | - |
| [PI GCS/GCS2](docs/devices/pi-gcs.md) | Serial stage motion/home/stop, servo/profile/reference/status readback, typed velocity/acceleration settings, and timing endpoint hooks | - |
| [Prior ProScan/OptiScan](docs/devices/prior.md) | Serial stage, NanoScan Z, filter, shutter, TTL, Lumen, native speed/acceleration, and readback helpers | - |
| [Spark Cyto](docs/devices/spark-cyto.md) | TDCL/CAN graph and transaction model for plate, detector, environment, imaging-head, and camera-binding workflows | - |
| [Spectral LMM5](docs/devices/spectral-lmm5.md) | Serial light-source control and readback | - |
| [Standa 8SMC4](docs/devices/standa.md) | Serial single-axis motion, status, and settings readback | - |
| [Starlight Xpress filter wheels](docs/devices/starlight-xpress.md) | Spec-backed serial and explicit/autodiscovered USB HID control | - |
| [Sutter/Ludl-compatible stages](docs/devices/sutter-stage.md) | Serial stage control and readback | - |
| [Sutter MP-285](docs/devices/sutter-mp285.md) | Serial stage control and readback | - |
| [Teensy pulse generator](docs/devices/teensy-pulse.md) | Binary pulse control plus opt-in real serial readback | - |
| [Thorlabs APT motors](docs/devices/thorlabs-apt.md) | Serial APT motion/home/stop, status, position, identity, velocity profile, and keep-alive helpers | - |
| [Thorlabs DC LED controllers](docs/devices/thorlabs-dc.md) | Serial/USBTMC LED control and readback | - |
| [Thorlabs KURIOS](docs/devices/thorlabs-kurios.md) | Serial filter control and readback | - |
| [Thorlabs SC10](docs/devices/thorlabs-sc10.md) | Serial shutter control and readback | - |
| [TriggerScope](docs/devices/triggerscope.md) | Serial TTL/camera trigger, DAC, focus, and timing-program control | - |
| [Trinamic TMCL stages](docs/devices/trinamic-tmcl.md) | Serial motion and readback | - |
| [Toupcam/AmScope cameras](docs/devices/toupcam.md) | Config-backed geometry plus live userspace USB camera backend with per-model profiles and local frame source | ✓ |
| [USB3 Vision cameras](docs/devices/usb3-vision.md) | U3V command/stream model plus opt-in USB open, endpoint catalog, and live command ReadMem/WriteMem path | - |
| [Velleman K8055/VM110 and K8061/VM140 IO boards](docs/devices/velleman.md) | USB analog, digital, PWM, and counter IO | - |
| [Warwick Open-Source Microscope](docs/devices/wosm.md) | v0.900 command-page-backed TCP stage, switch/shutter, light, and digital input plus legacy switch-sequence, blanking, pull-up, and raw analog readback | - |
| [Xeryon ASCII piezo stages](docs/devices/xeryon.md) | ASCII serial stage motion, velocity, status, and readback | - |
| [Xeryon integrated CANopen stages](docs/devices/xeryon-canopen.md) | CiA 402 transaction planning, optional live SocketCAN/SLCAN NMT/SDO execution, and EDS object parsing | - |
| [Zaber ASCII stages](docs/devices/zaber.md) | ASCII motion and readback | - |

### Metadevices

| Device family | Scope |
| --- | --- |
| [Autofocus providers](docs/planning/device_implementation_plan.md) | Provider-neutral autofocus capability implemented by Squid/Octopi, ASI Tiger CRISP, and SutterStage, with composed simulation listed below |

### Simulators

| Device family | Scope |
| --- | --- |
| [Biological system simulation](docs/devices/sim.md) | Biological-model-oriented system simulation |
| [Laser-scanning microscope simulation](docs/devices/sim-lsm.md) | Confocal capture, image stream, and signal stream output over the shared procedural cell-culture model |
| [Composed microscope and LSM simulation](docs/devices/sim-microscope-lsm.md) | Brightfield camera and LSM APIs in one simulator driver with shared stage, focus, objective, lamp, and specimen state |
| [Brightfield microscope simulation](docs/devices/sim-microscope.md) | One composed microscope over a shared procedural cell-culture model: camera, XY/Z motion, three-position objective turret, transmitted-light lamp, and a published optical calibration chain |

## License

MIT if nothing else mentioned

Exceptions under `data/third_party/` are third-party data and are not
covered by this repository's common license terms.

Note that the code is largely AI-generated. Thus please review code in any scenario
when the license is especially important (e.g., if you copy parts of it).
