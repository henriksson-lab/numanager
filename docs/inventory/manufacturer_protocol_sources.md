# Manufacturer Protocol Source Map

When an original manufacturer publishes a command manual or protocol reference,
we should treat that as the primary source for `numanager` drivers. The
Micro-Manager adapter is still useful as implementation evidence: defaults,
device variants, command ordering, quirks, and what features have worked in
practice.

This document maps likely SDK-free Micro-Manager adapters to public
manufacturer or manufacturer-adjacent protocol sources.

## Primary Sources Available

| Family | Micro-Manager adapter(s) | Primary evidence status | Use this before Micro-Manager? | Notes |
| --- | --- | --- | --- | --- |
| Cephla Squid/Octopi | `Cephla` | Open project command-set and firmware/source | yes | This is project-defined hardware. Prefer Cephla/Squid command-set docs and firmware over reconstructing behavior from Micro-Manager alone. |
| ASI MS-2000/RM-2000/Tiger | `ASIStage`, `ASITiger` | Excellent official online command docs | yes | ASI publishes serial command pages and RS-232 communication notes. This likely exposes more than Micro-Manager uses: ring buffer, TTL, scan modules, synchronized encoder reporting, array module, SPIM/Tiger card features. |
| Zaber X-Series | `Zaber` | Excellent official ASCII protocol manual | yes | Current Micro-Manager adapter uses Zaber Motion SDK headers, but a clean driver should use Zaber ASCII protocol directly. Public docs include command/reply grammar, alerts, settings, streams, triggers, IO, and product-specific protocol pages. |
| Xeryon XD-M/XD-C/XD-OEM piezo stages and integrated XLA/XUMU controllers | none in Micro-Manager tree | Manufacturer controller manuals document ASCII-over-serial commands; integrated-controller materials identify CANopen/CiA 402 and EDS/example paths | yes | Xeryon publishes controller manuals with LF-terminated `X:TAG=value` / `X:TAG=?` framing, motion tags, feedback tags, units, and status bits. Integrated XLA/XUMU support is modeled separately with CiA 402 transaction planning, optional live SocketCAN/SLCAN NMT/SDO execution, and EDS object parsing. |
| GenICam/GigE Vision/USB3 Vision cameras | `Aravis`, `GigECamera`, `Basler`, `Spinnaker`, some Allied Vision/industrial cameras | Public GenICam docs; interface standards and open Aravis implementation exist | yes, with licensing review | Do not treat every SDK camera as opaque. Cameras that expose GenICam XML over GigE Vision, USB3 Vision, CoaXPress, or Camera Link can be modeled from standards plus device XML. This can expose events, chunk data, sequencer, action commands, trigger routing, stream buffers, and vendor extension nodes beyond Micro-Manager. |
| Basler pylon cameras | `Basler` | SDK adapter, but Basler documents GenICam/SFNC and transport standards | yes for standard transports | pylon is SDK-based, but Basler cameras commonly expose GenICam node maps over GigE Vision/USB3 Vision/CoaXPress. A generic GenICam driver may cover many features without using pylon. |
| FLIR/Teledyne Spinnaker, IDS peak, Hikrobot, Allied Vision cameras | `Spinnaker`, `IDSPeak`, `IDS_uEye`, `Hikrobot`, `AlliedVisionCamera` | SDK adapters, but many models are GenICam/GigE Vision/USB3 Vision devices | yes for standard transports | The SDK docs often expose GenICam-style node maps. A standards-first path is plausible for compliant cameras, separate from vendor-only USB modes and old FireWire/legacy APIs. |
| Hamamatsu scientific cameras | `Hamamatsu` | DCAM API/SDK is official; public wire protocol not found | no; runtime-backed only | Hamamatsu describes DCAM-API as the standardized control API across their digital cameras. Treat as SDK-first unless a model-specific public protocol or Linux kernel/USB evidence is found. |
| Andor/Oxford Instruments cameras | `Andor`, `AndorSDK3` | No manufacturer wire-protocol manual found; reverse engineered evidence records a USB transport/readout audit; vendor firmware/runtime packages may be used as third-party excluded data where required | mixed | Treat SDK2 discovery/identity/status/acquisition/readout as implementable where reverse engineered evidence records confirmed USB facts or where an optional vendor-runtime backend is available. SDK2 vendor-runtime exposure, detector readback, full-frame capture, and temperature/cooler control are implemented behind the verified runtime gate. SDK3 vendor-runtime feature getters/setters are implemented for the documented standard feature surface; SDK2 SDK-free exposure/register-window mapping and SDK3 native feature-register mappings remain unsupported until traces, firmware/runtime behavior, ABI evidence, or deeper register-map evidence exist. |
| Teledyne Photometrics/QImaging PVCAM/PICAM cameras | `PVCAM`, `PICAM`, `QCam` | Local `extra_Photometrix` notes include a PVCAM C ABI hardware spec plus native USB/PCIe protocol notes | mixed | Current `numanager_drivers::photometrics_pvcam` support includes configured/USB descriptor evidence, runtime package checks, opt-in runtime camera-name discovery, writable exposure setting, opt-in runtime one-shot capture, repeated one-shot stream support, and runtime temperature read/setpoint control. Treat native continuous streaming, broader PVCAM parameter control, and native transport as later targets after parameter probing, traces, and hardware output are recorded. |
| ZWO / Player One astronomy cameras | `ZWO`, `PlayerOneCamera` | SDK downloads and headers public; public wire protocol not found | no for true SDK-free, maybe for wrapper | These may be acceptable only as optional SDK wrappers. Without USB protocol docs, do not treat them as clean SDK-free drivers. |
| FLI / Finger Lakes Instrumentation | `FLICamera` | Vendor says SDK is open source | maybe | This is not SDK-free in the strict sense, but an open-source SDK may allow audited vendoring or protocol extraction without black-box binaries. |
| Prior ProScan/OptiScan | `Prior`, `PriorLegacy` | Good public ProScan III command manual | yes | Manual covers standard/compatibility mode, stage, Z, wheels, shutters, Lumen, patterns, mapping, TTL, encoder and trigger-board commands. Micro-Manager uses only a subset. |
| CoolLED pE series | `CoolLEDpE300`, `CoolLEDpE4000`, `XCiteLed` where applicable | Good official/product-linked command manual | yes | CoolLED product pages list a commands manual for pE-4000/pE-300/pE-340. Likely exposes individual channel irradiance, TTL/analogue behavior, function generator features beyond Micro-Manager. |
| Lumencor light engines | `LumencorSpectra`, `LumencorCIA` | Public command references available from vendor downloads | yes for serial/standard modes | Good light-source target. Possible feature expansion includes Ethernet/LAN control, channel calibration, trigger profiles, and richer per-channel telemetry. |
| Sutter MP-285/MP-285A | `MP285` | Good manufacturer support downloads and quick reference | yes | Sutter publishes MP-285 software/examples and MP-285 external-control quick reference. Use that for binary framing, byte order, status, resolution, and velocity details. |
| Sutter MPC/MAC/Ludl-compatible stages | `SutterStage`, `Ludl` | Partial public docs; Micro-Manager wiki states Sutter emulates Ludl protocol | mixed | Use Sutter/Ludl manuals if available; Micro-Manager is a practical command map for `MOVE`, `WHERE`, `STATUS`, `Rconfig`, etc. |
| Märzhäuser TANGO/L-Step | `Marzhauser-LStep`, `Marzhauser` | Excellent official instruction-set downloads | yes | Manufacturer publishes TANGO native instruction sets by firmware version and Venus instruction-set docs. Prefer those for feature completeness. |
| Physik Instrumente GCS controllers | `PI_GCS`, `PI_GCS_2` | Public PI docs describe GCS text command concept; detailed manuals in PI documentation portal | yes | Strong SDK-backed counterexample. GCS is command-based over USB/RS-232/TCP/IP. Feature expansion includes wave generators, data recorders, triggers, and capability probing. |
| Thorlabs APT-compatible motion | `ThorlabsAPTStage` and related Thorlabs motion adapters | Public APT communication protocol references exist | yes | Avoid proprietary APT/Kinesis runtime dependencies where possible. The protocol path can expose full status packets, homing/limit parameters, channels, bays, and keepalive/status streaming. |
| Thorlabs SC10 shutter controller | `ThorlabsSC10` | SC10/SH05 operating manual documents RS-232 command-line interface; Micro-Manager page corroborates serial defaults | yes | The current `numanager_drivers::thorlabs_sc10` support implements the documented RS-232 identity, shutter enable/readback, mode, open/close timing, trigger-source, repeat-count, and mapped refresh-helper surface. Interlock/alarm context remains read-only/configured until a stable CLI query or hardware trace identifies reply semantics. |
| Standa 8SMC controllers | Standa/8SMC adapters | Open `libximc` source and programming documentation | maybe | Best path may be audited vendoring or clean extraction from open source rather than a black-box SDK. Feature expansion includes diagnostics, encoder details, and coordinated motion profiles. |
| Evident/Olympus IX85 | `EvidentIX85`, `EvidentIX85Win` | One BSD-licensed Micro-Manager path uses direct serial tags; Windows path uses vendor software | mixed | `numanager_drivers::evident_ix85` now exposes configured inventory plus opt-in serial focus motion/stop, state-device selection, shutter control, body readback, and mapped refresh helpers. ZDC/autofocus actions await `AF` parameter semantics from protocol docs, traces, or bench validation. |
| Hamilton MVP | `HamiltonMVP` | Product page confirms RS-232 serial ASCII and manual availability; Hamilton Protocol 1/RNO+ evidence documents address framing, ACK/NAK, valve positioning, current-position query, valve-type query, status, and firmware requests | yes | Public product page says up to 16 valves can be daisy-chained and controlled by simple ASCII commands. The current driver uses the documented Protocol 1/RNO+ valve subset; DIN/BDZ+ semantics are not recorded, and daisy-chain behavior remains a hardware-validation item. |
| Velleman K8055/K8061 | `K8055`, `K8061` | Manufacturer pages describe IO surface; Linux `vmk80xx` open driver documents packet/register behavior | mixed | `numanager_drivers::velleman` covers K8055/VM110 and K8061/VM140 analog/digital/PWM/counter IO and can use an explicit-config or endpoint-autodiscovered `os-usb` packet backend. K8061 debounce, reset safety, and hardware validation remain separate evidence gates. |
| Trinamic/TMCL controllers | `kdv` and similar motion adapters | Public TMCL protocol manuals plus controller manuals | yes | Treat TMCL as the primary wire-protocol source, then use adapter code to map device-specific axes and defaults. The current support implements startup/runtime-refresh direct-mode serial control with `MVP`, `MST`, `SAP`, `GAP`, and raw binary firmware-version readback for configured stage controllers. |
| 3Z Optics | `3Z_Optics` | Modbus RTU framing is public; device register map needs vendor/project confirmation | mixed | Use Modbus for framing and CRC. Do not rely on generic Modbus alone for semantic safety; register meanings need a primary map. |
| Bluebox Optics niji | `BlueboxOptics_niji` | Reverse engineered evidence records a compact serial command surface; manufacturer command manual revision is not pinned | mixed | Current `numanager_drivers::bluebox_niji` supports opt-in serial startup query, output control, status/temperature/readback refresh helpers, and known-prefix readback parsing after writes. Pin a manufacturer manual or hardware traces before expanding broader reply/error parsing, lockout/fault semantics, or safety claims. |
| Starlight Xpress filter wheels | `StarlightXpress` | Public wheel handbooks document HID report and serial frame protocols | yes for serial/HID support | Standard and Maxi product pages state they use the same protocol as the smaller SX wheel. The current driver implements the documented four-byte serial protocol plus explicit-config or single-match autodiscovered USB HID input/output-report control behind `os-hid`; product-specific VID/PID cataloging is not complete, and hardware validation is tracked separately. |
| Spectral LMM5 | `SpectralLMM5` | Public LMM5 user/software manual documents RS-232 hexadecimal command protocol | yes for serial support | The current driver implements the documented serial shutter, transmission, wavelength, and documented trigger-configuration surface. USB/HID transport framing is not recorded. |
| Corvus / ITK stages | `Corvus` | Reverse engineered evidence records serial settings, DIP-switch baud rates, and a compact text command surface; exact manufacturer manual revision is not pinned | mixed | Current `numanager_drivers::corvus` supports opt-in serial startup readback, stage move/home/stop writes, refresh helpers, and known numeric position/speed/acceleration readback parsing. Pin the exact Corvus manual/command-list revision before expanding status bits, host-mode variants, range/limit behavior, or safety claims. |
| Chuo Seiki QT stages | `ChuoSeiki_QT` | Manufacturer page confirms QT command control over RS-232/USB; reverse engineered evidence records controller families, serial settings, axis naming, and command surface | mixed | The current `numanager_drivers::chuo_seiki_qt` support implements opt-in serial startup identification, stage writes, busy/position/readback refresh helpers, and known-format typed position-state readback. Pin the exact downloadable QT controller manual/command-list revision before expanding completion/error semantics, limit behavior, or safety claims. |
| Cobolt / Hübner lasers | `Cobolt`, `CoboltOfficial` | Public serial command behavior | yes, if manual obtained | The current `numanager_drivers::cobolt` support implements opt-in serial startup readback, emission/power/current/mode/autostart control, interlock/fault/usage telemetry, and query-backed refresh helpers. Manufacturer protocol or hardware traces are still needed before expanding warmup, CDRH delay, interlock reset, fault recovery, modulation, and hardware trigger behavior. |
| Coherent OBIS | `CoherentOBIS` | Public SCPI-style command behavior | yes, if manual obtained | The current `numanager_drivers::coherent_obis` support implements opt-in serial startup readback, emission/power/modulation/mode/CDRH-delay control, fault/head/usage telemetry, and query-backed refresh helpers. Manufacturer protocol or hardware traces are still needed before expanding fault reset, interlock behavior, and hardware trigger/modulation behavior. |
| ArduinoCounter | `ArduinoCounter` | Project firmware and Micro-Manager wiki | in-tree is primary | This is Micro-Manager-defined hardware, so the adapter firmware is the protocol reference. |
| TeensyPulseGenerator | `TeensyPulseGenerator` | In-tree firmware primary; related public Teensy pulse docs exist | in-tree is primary | Treat as project-defined firmware unless targeting a separate published hardware project. |
| Arduino / Arduino32bitBoards / ESP32 / OpenUC2 | `Arduino`, `Arduino32bitBoards`, `ESP32`, `OpenUC2` | Firmware/source-defined protocols | in-tree/project docs primary | These are open firmware protocols, not proprietary manufacturer controllers. Use firmware plus project docs rather than proprietary binary internals. |
| OpenStage | none in Micro-Manager tree | Peer-reviewed open hardware paper publishes serial command tables | yes | This is not a Micro-Manager adapter target, but it is microscope hardware with a public serial control protocol and fits the spec-backed driver track. |
| ASI-compatible `WOSM`, `TriggerScope`, custom MCU devices | several | usually project-defined | in-tree/project docs primary | Use source/firmware where the hardware protocol was created by the adapter authors. The current WOSM support implements opt-in prompt-based TCP output commands plus aggregate digital-input and raw analog-input reads; analog scaling is not recorded. The current TriggerScope support implements opt-in serial direct-control, constrained TTL/DAC/focus sequence programming, and public timing-plan mapping; route mapping, camera-trigger sequence mapping, response/error vocabulary, and exact timebase semantics are not recorded. |

## Families Where Manufacturer Sources May Expand Feature Coverage

## Reverse-Engineered Fallback Inventory

Some Micro-Manager adapters depend on proprietary packages. These are
only fallback evidence signals; they are not our preferred protocol source.

| Adapter | Fallback evidence | Better source to try before fallback evidence | Current disposition |
| --- | --- | --- | --- |
| `Andor` / `AndorSDK3` | Reverse engineered | Local clean-room source and notes, plus any model-specific standard transport docs | SDK2 USB discovery, hidden firmware initialization, EP0 identity/status/FIFO/acquisition helpers, opt-in live bulk-IN `Mono16` capture, and verified SDK2 runtime exposure/detector/cooler control are implemented; SDK3 USB discovery, hidden FX3 firmware initialization, EP0 status readbacks, verified runtime feature control/readback, cooler control, and opt-in `Mono16` capture are implemented. SDK2 SDK-free exposure/register-window mapping and SDK3 native feature-register mappings remain unsupported until register evidence exists. |
| `AmScope` / Toupcam family | Reverse engineered | Toupcam headers/bindings, DirectShow/TWAIN/UVC paths where exposed, existing `opengel` protocol-derived path | Live userspace USB camera backend, config-backed geometry, and local frame-source support are implemented; broader property control remains SDK/protocol-specific until additional protocol evidence is recorded. |
| `BaumerOptronic` | Reverse engineered | GenICam/GigE Vision/USB3 Vision path for modern Baumer models; legacy GAPI/FxLib docs for Leica DFC class | low for legacy Leica DFC/FxLib, better for modern GenICam models |
| `PCO_Generic` | Reverse engineered | PCO SDK samples/API docs, and GenICam only for models that expose it | low for SC2/CameraLink path, medium for model-specific GenICam |
| `TISCam` | Reverse engineered | open `tiscamera`, GStreamer/V4L2, Aravis/GigE, GenTL/DirectShow depending platform | high for supported cameras through open/platform stacks |
| `ScionCam` | Reverse engineered | legacy camera documentation or open driver evidence | low priority; likely legacy SDK-only |
| `ABS` | Reverse engineered | vendor docs or USB protocol evidence | Runtime package checks, writable exposure, explicit software trigger, opt-in verified-runtime capture, and repeated one-shot streaming are implemented; native USB transport remains unsupported until protocol evidence is recorded. |
| `AgilentLaserCombiner` | Reverse engineered | public Micro-Manager docs only; external DA/TTL control may cover basic laser gating | Reverse engineered serial request/reply support, typed line control/readback, analog-output diagnostics, and mapped refresh helpers are implemented; hardware sequence and persistence behavior remain evidence-gated. |
| `Omicron` | Reverse engineered | serial settings from Micro-Manager docs, public LuxX command examples, manufacturer command-list references | medium-high for serial-supported LuxX/PhoxX/BrixX/LightHUB devices |
| `KuriosLCTF` | Reverse engineered | Thorlabs KURIOS user guide CLI: `Keyword=argument(CR)` and `Keyword?(CR)` command table | high; use documented CLI rather than vendor software |
| `ThorlabsDC40` | Reverse engineered | DC40 user guide and TLDC VXIpnp/header docs | low-medium; no low-level command grammar pinned yet |
| `ThorlabsDCxxxx` | Reverse engineered | serial commands in adapter for DC2010/2100/3100/4100; SCPI/USBTMC/VISA evidence for DC2200 | high for DC2010/2100/3100/4100; DC2200 needs hardware/manual confirmation |
| `ThorlabsSC10` | none required | SC10/SH05 operating manual command-line interface plus Micro-Manager serial defaults | high; use documented CLI rather than vendor software |
| `Mightex_C_Cam` | Reverse engineered | Mightex SDK docs and public wrappers | Runtime package checks, writable capture parameters, opt-in verified-runtime `Mono16`/`Raw16` capture, and repeated one-shot streaming are implemented; native transport and broader SDK-free acquisition remain unsupported until protocol evidence is recorded. |
| `MCL_MicroDrive` / `MCL_NanoDrive` | Reverse engineered | public headers/examples expose API only, not USB protocol | Active USB descriptor discovery, MicroDrive raw encoder/status readback, fixed-length raw control-read/action commands, and firmware/runtime package checks are implemented; typed motion remains unsupported until units, status meanings, and completion behavior are evidenced. |
| `Okolab` | Reverse engineered | OkoLib docs/header confirm COM-port devices; reverse engineered evidence recovers serial framing, checksum mode, discovery, and property command codes | Opt-in serial/configured runtime support is implemented with database-backed product identity, temperature/CO2 target/readback, humidity readback/control where command rows exist, and named parameter read/write helpers; module inventory and fault/completion semantics remain evidence-gated. |
| `Modbus` | none needed | Modbus RTU/TCP standards and open Rust Modbus stack | high; use standard/open library, no proprietary artifact analysis |
| `ParallelPort` / `AOTF` | Reverse engineered | OS-specific GPIO/parallel-port access APIs | platform utility only; do not analyze proprietary internals |
| `OpenCVgrabber` | OpenCV 2.4 runtime libraries | OpenCV crate/system package | ignore for driver protocols |

The practical ordering from this fallback-evidence pass is:

1. Implement or integrate standard/open backends: Modbus, GenICam/Aravis,
   `tiscamera`/V4L2/GStreamer where appropriate, and modern OpenCV capture only
   as a generic webcam fallback.
2. Maintain and expand documented command devices: KURIOS CLI, Omicron serial
   lasers, Thorlabs DC2010/DC2100/DC3100/DC4100 serial commands, and explicit
   DC2200 USBTMC/SCPI support already have opt-in control/readback drivers.
3. Keep Toupcam/AmScope as a special target because this repository already has
   a prior userspace USB backend; treat full property coverage as unresolved.
4. Maintain the implemented Andor SDK2/SDK3 userspace USB and verified-runtime
   tracks, while keeping SDK2 SDK-free writable controls and SDK3 native
   feature-register control unsupported until register-map evidence exists.
5. Maintain the implemented PVCAM evidence/discovery/runtime surface; keep
   native transport and broader parameter control gated by native traces and
   PVCAM parameter probing evidence.
6. Treat remaining proprietary-runtime-only scientific or controller SDKs as
   optional verified-runtime backends unless protocol evidence exists. For MCL,
   ABS, Mightex camera, Okolab, and Agilent Laser Combiner, keep the implemented
   runtime or SDK-free subset and expand only when the missing protocol,
   completion, or safety evidence is recorded.
7. Use proprietary-runtime fallback evidence only after this source ladder fails and only for
   small, high-value controllers where legal/licensing review is acceptable.

ASI:

- Primary docs cover `MOVE`, `WHERE`, `STATUS`, `HALT`, `HOME`, `HERE`,
  `JOYSTICK`, `TTL`, `RBMODE`, ring buffers, scan modules, array module,
  encoder reporting, and Tiger card addressing.
- Micro-Manager mostly focuses on stage, CRISP, LED, turret, filter wheel, PMT,
  scanner, DAC, and SPIM-adjacent features. A `numanager` driver can expose
  richer synchronized motion and trigger sequences directly.

Zaber:

- Zaber ASCII protocol supports command/reply framing, faults, warnings, alerts,
  `home`, `move abs`, `move rel`, `move max`, settings via `get`/`set`,
  streams, triggers, and IO commands.
- This is a better source than the current Micro-Manager adapter because the
  adapter calls Zaber Motion Library rather than spelling out the whole wire
  protocol.

Prior:

- The ProScan III manual includes standard mode queueing, end-of-move `R`
  replies, compatibility mode, `$` status bitfields, `?` peripheral inventory,
  stage/Z/filter/shutter/Lumen/TTL/trigger commands, and OEM axes.
- Micro-Manager's `Prior` adapter uses common stage, Z, wheel, shutter and TTL
  commands, but the manual should let us support queueing and richer TTL/trigger
  features.

CoolLED:

- Manufacturer command manual should be primary for channel assignment,
  irradiance, global/channel on/off, TTL override behavior, analog inputs, and
  pE-4000 function-generator features.
- Micro-Manager shows key commands (`XMODEL`, `XVER`, `CSS?`, `LAMS`,
  `LOAD:<wavelength>`, `CSN`, `CSF`, `PORT:P=...`) but not the full surface.

Sutter:

- MP-285 external-control docs should settle byte order, resolution modes,
  velocity scaling, status/error bytes, and USB-vs-RS232 details.
- For `SutterStage`, the Micro-Manager wiki says the Sutter XY stage emulates
  the Ludl communication protocol. Prefer an official Sutter/Ludl protocol
  manual if we can obtain one; otherwise treat reverse engineered behavior as a
  named subset.

Märzhäuser:

- Official instruction-set PDFs are versioned by firmware. Use them to build a
  protocol module that can negotiate controller firmware and expose supported
  features rather than only the Micro-Manager `?ver`, `!moa`, `!mor`, `?pos`,
  `?err` subset.

GenICam cameras:

- A standards-first camera stack should be separate from vendor camera drivers.
  The minimal architecture is discovery, device XML retrieval, a typed GenApi
  node model, event channels, stream-buffer negotiation, and transport plugins
  for GigE Vision and USB3 Vision.
- This is a better long-term answer for Basler, FLIR/Spinnaker, Allied Vision,
  IDS peak, Hikrobot, and similar industrial cameras than cloning per-vendor
  Micro-Manager adapter behavior. It also aligns with high-throughput
  acquisition needs because the transport owns stream buffers and frame/event
  delivery.
- Legal/source hygiene needs attention. Open projects such as Aravis explicitly
  warn contributors not to base open-source implementation work on restricted
  A3 specification documents. Use freely available GenICam material, device XML,
  vendor public docs, clean-room testing, and licensing review before
  implementing transport details.

## Source Links

- Micro-Manager DeviceAdapters tree:
  <https://github.com/micro-manager/mmCoreAndDevices/tree/main/DeviceAdapters>
- ASI serial command reference:
  <https://www.asiimaging.com/docs/products/serial_commands>
- ASI MS-2000/RM-2000 docs:
  <https://asiimaging.com/docs/products/ms2000>
- ASI Tiger docs:
  <https://asiimaging.com/docs/products/tiger>
- ASI RS-232 communication note:
  <https://asiimaging.com/docs/tech_note_rs232_comm>
- Zaber ASCII protocol overview:
  <https://www.zaber.com/articles/ascii-protocol>
- Zaber ASCII protocol manual:
  <https://www.zaber.com/protocol-manual?protocol=ASCII>
- Xeryon controller manuals and guides:
  <https://xeryon.com/help-center/manuals-guides/>
- Xeryon integrated-controller CANopen examples:
  <https://github.com/Xeryon-Precision/XLA-INTG_prog_examples>
- EMVA GenICam introduction:
  <https://www.emva.org/standards-technology/genicam/introduction-new/>
- EMVA GenICam downloads archive:
  <https://www.emva.org/standards-technology/genicam/genicam-downloads-archive/>
- Aravis open GenICam/GigE Vision/USB3 Vision implementation:
  <https://github.com/AravisProject/aravis>
- Aravis API documentation:
  <https://aravisproject.github.io/aravis/aravis-stable/index.html>
- Basler pylon software suite:
  <https://docs.baslerweb.com/pylon-software-suite>
- Basler pylon C programming guide:
  <https://docs.baslerweb.com/pylonapi/c/programmingguide>
- FLIR/Teledyne Spinnaker SDK docs:
  <https://softwareservices.flir.com/spinnaker/latest/index.html>
- FLIR/Teledyne Spinnaker programmer guide:
  <https://softwareservices.flir.com/spinnaker/latest/_programmer_guide.html>
- A3 GigE Vision standard download page:
  <https://www.automate.org/vision/vision-standards/download-the-gige-vision-standard>
- Hamamatsu DCAM driver/software page:
  <https://www.hamamatsu.com/jp/en/product/cameras/software/driver-software.html>
- Teledyne Photometrics PVCAM SDK page:
  <https://www.teledynevisionsolutions.com/products/pvcam-sdk-amp-driver/GetResourcesSupportDownloads/>
- ZWO product SDK page:
  <https://www.zwoastro.com/software/product-sdk/>
- Player One SDK page:
  <https://www.player-one-astronomy.com/service/software/>
- Debian Player One header package source:
  <https://sources.debian.org/src/libplayerone/3.1.0%2B20221218103507-1/PlayerOneCamera.h>
- FLI support page:
  <https://www.flicamera.com/support>
- Micro-Manager Andor page:
  <https://micro-manager.org/Andor>
- Micro-Manager Andor SDK3 page:
  <https://micro-manager.org/Andor_SDK3>
- Andor SDK2 Python wrapper docs:
  <https://pythonhosted.org/andor/>
- Andor SDK3 open ctypes wrapper:
  <https://gitlab.com/ptapping/andor3/-/blob/main/andor3/andor3.py>
- Micro-Manager AmScope page:
  <https://micro-manager.org/AmScope>
- AmScope MU camera support:
  <https://amscope.com/pages/camera-support-mu-series>
- Toupcam Rust bindings:
  <https://docs.rs/crate/toupcam-sys/latest>
- CToupcam wrapper:
  <https://github.com/tirfil/CToupcam>
- Micro-Manager BaumerOptronic page:
  <https://micro-manager.org/BaumerOptronic>
- Baumer GAPI guide mirror:
  <https://www.yumpu.com/en/document/view/49743105/baumer-gapi-sdk-v17-programmers-guide-site-ftp-elvitec>
- Micro-Manager PCO Camera page:
  <https://micro-manager.org/PCO_Camera>
- PCO SDK samples:
  <https://github.com/Excelitas-PCO/pco.sdk-samples>
- pylablib PCO SC2 notes:
  <https://pylablib.readthedocs.io/en/latest/devices/PCO_SC2.html>
- Micro-Manager TISCam page:
  <https://micro-manager.org/TIScam>
- The Imaging Source tiscamera docs:
  <https://www.theimagingsource.com/en-us/documentation/tiscamera/>
- The Imaging Source downloads:
  <https://www.theimagingsource.com/en-us/support/download/>
- The Imaging Source GStreamer/tiscamera page:
  <https://www.theimagingsource.com/en-us/product/software/gstreamer/>
- Micro-Manager ScionCam page:
  <https://micro-manager.org/ScionCam>
- Micro-Manager IIDC page:
  <https://micro-manager.org/IIDC>
- Micro-Manager ABSCamera page:
  <https://micro-manager.org/ABSCamera>
- Micro-Manager OpenCVgrabber page:
  <https://micro-manager.org/OpenCVgrabber>
- OpenCV video I/O overview:
  <https://docs.opencv.org/3.4.3/d0/da7/videoio_overview.html>
- OpenCV video I/O backend flags:
  <https://docs.opencv.org/3.4.3/d4/d15/group__videoio__flags__base.html>
- Spectral LMM5 user manual mirror:
  <https://paperzz.com/doc/7683188/lmm5-user-manual>
- OpenStage paper:
  <https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0088977>
- Prior ProScan III manual mirror:
  <https://manualzz.com/doc/60415779/prior-scientific-proscan-iii-manual>
- Lumencor downloads:
  <https://lumencor.com/customer-center/downloads?category=control-software>
- PI communication concept:
  <https://www.physikinstrumente.com/en/products/software-suite/communication-concept-interfaces>
- PI product documentation portal:
  <https://www.physikinstrumente.com/en/knowledge-center/downloads/product-documentation/>
- Thorlabs APT protocol Python implementation docs:
  <https://thorlabs-apt-device.readthedocs.io/en/latest/>
- Thorlabs APT communications protocol PDF:
  <https://www.thorlabs.com/Software/Motion%20Control/APT_Communications_Protocol.pdf>
- ADI TMCM-3212 product page:
  <https://www.analog.com/en/products/tmcm-3212.html>
- ADI TMCM-3212 TMCL firmware manual:
  <https://www.analog.com/media/en/dsp-documentation/software-manuals/TMCM-3212-TMCL_firmware_manual_fw1.13_rev1.10.pdf>
- Standa 8SMC5 product/programming documentation page:
  <https://www.standa.lt/products/catalog/motorised_positioners?item=525&print=1>
- Standa open libximc source:
  <https://github.com/Standa-Optomechanics/libximc>
- Micro-Manager Evident IX85 page:
  <https://micro-manager.org/EvidentIX85>
- Micro-Manager Prior adapter page:
  <https://micro-manager.org/Prior>
- libmodbus source:
  <https://github.com/stephane/libmodbus>
- tokio-modbus crate docs:
  <https://docs.rs/tokio-modbus/>
- Velleman K8055 product page:
  <https://www.velleman.eu/products/view/?country=ch&id=351346&lang=en>
- Linux `vmk80xx` COMEDI driver:
  <https://codebrowser.dev/linux/linux/drivers/comedi/drivers/vmk80xx.c.html>
- Microsoft parallel-port driver docs:
  <https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/parallel/ns-parallel-_parallel_port_information>
- Micro-Manager ParallelPort page:
  <https://micro-manager.org/ParallelPort>
- Micro-Manager ThorlabsDCxxxx page:
  <https://micro-manager.org/ThorlabsDCxxxx>
- MeasurementControl Thorlabs DC2200 notes:
  <https://deckers.iffgit.fz-juelich.de/measurementcontrol/instruments/thorlabs_dc2200.html>
- IVI Foundation SCPI page:
  <https://www.ivifoundation.org/About-IVI/scpi.html>
- PyVISA-py docs:
  <https://www.pyvisa.org/docs/pyvisa-py>
- Thorlabs DC40 page:
  <https://www.thorlabs.de/newgrouppage9.cfm?objectgroup_id=16692&pn=DC40>
- Thorlabs software downloads:
  <https://www.thorlabs.us/navigation.cfm?Guide_ID=2191>
- Micro-Manager AgilentLaserCombiner page:
  <https://micro-manager.org/AgilentLaserCombiner>
- Micro-Manager Omicron page:
  <https://micro-manager.org/Omicron>
- Omicron LuxX communication package:
  <https://pypi.org/project/luxx_communication/>
- FreiCtrl Omicron communication source:
  <https://arturoptophys.github.io/FreiCtrl_Laser/_modules/FreiCtrl_laser/luxx_communication.html>
- Thorlabs KURIOS user guide mirror:
  <https://device.report/m/f00051352e01b606d139116d44ed32aac8b5c43ccd3349aa19c3f508e92afc0a>
- Thorlabs SC10 software/support page:
  <https://www.thorlabs.de/software_pages/ViewSoftwarePage.cfm?Code=SC10>
- Thorlabs SC10/SH05 operating manual mirror:
  <https://manualzz.com/doc/23799818/thorlabs-sc10--sh05-shutter-controller-operating-manual>
- Micro-Manager ThorlabsSC10 page:
  <https://micro-manager.org/ThorlabsSC10>
- OkoLib docs:
  <https://www.oko-lab.com/public/okolib/doc/>
- OkoLib header docs:
  <https://www.oko-lab.com/public/okolib/doc/okolib_8h_source.html>
- Micro-Manager Okolab page:
  <https://micro-manager.org/Okolab>
- Micro-Manager MCL MicroDrive page:
  <https://micro-manager.org/MCL_MicroDrive>
- Micro-Manager MCL NanoDrive page:
  <https://micro-manager.org/MCL_NanoDrive>
- ScopeFoundry MCL stage notes:
  <https://scopefoundry.org/docs/301_existing-hardware-components/hw_mcl_stage-scopefoundry/>
- Mightex camera SDK page:
  <https://w.mightexbio.com/product_catalogue/cameras/camera_sdk.shtml>
- pylablib Mightex notes:
  <https://pylablib.readthedocs.io/en/latest/devices/Mightex.html>
- Sutter product support:
  <https://www.sutter.com/product-support>
- MP-285 external-control quick reference mirror:
  <https://manuals.plus/m/fb6fa936f2d2b3c58e6c159dba9bda40cf6fc617d542743be124a282127c6c91>
- Micro-Manager SutterStage page:
  <https://micro-manager.org/SutterStage>
- Märzhäuser instruction sets:
  <https://www.marzhauser.com/en/downloads/controllers/tango/drivers-firmware-documentation/instruction-sets>
- Hamilton MVP product page:
  <https://www.hamiltoncompany.com/valves-fittings-tubing/automated-valves/mvp-valve-positioner>
- CoolLED pE-4000 product/manual page:
  <https://www.coolled.com/products/pe-4000/>
- CoolLED commands manual mirror:
  <https://manualzz.com/doc/60403052/coolled-pe-300ultra--pe-300white--pe-340fura-command-manual>
- Micro-Manager Cobolt page:
  <https://micro-manager.org/Cobolt>
- Velleman K8061 product page:
  <https://www.velleman.eu/products/view/extended-usb-interface-board-k8061/?id=364910&lang=en>
- Linux COMEDI Velleman low-level driver:
  <https://gbmc.googlesource.com/linux/+/1f66f63c7312ee085dc989b3c5fa4b3d09fe9d52/drivers/comedi/drivers/vmk80xx.c>
- Starlight Xpress Standard USB Filter Wheel product page:
  <https://www.sxccd.com/product/standard-filter-wheel/>
- Starlight Xpress Maxi USB Filter Wheel product page:
  <https://www.sxccd.com/product/maxi-filter-wheel/>
- Starlight Xpress Maxi USB Filter Wheel manual mirror:
  <https://manualzz.com/doc/76668367/starlight-xpress-120-002n-maxi-usb-filter-wheel-manual>
- Starlight Xpress Universal Filter Wheel manual mirror:
  <https://www.manualsdir.com/manuals/594350/starlight-xpress-sx-universal-filter-wheel.html?page=4>
- Arduino Counter Micro-Manager page:
  <https://micro-manager.org/Arduino_Counter>

## Policy For Driver Specs

For each driver spec we should record:

1. Primary source class: manufacturer manual, open firmware, standards document,
   or reverse engineered compatibility evidence.
2. Secondary implementation evidence: known working command defaults and
   compatibility behavior.
3. Feature delta: capabilities present in the primary source but missing from
   Micro-Manager.
4. Safety model: commands that enable energy, motion, pressure, voltage, or
   persistent controller settings.
5. Completion model: explicit busy/status query, asynchronous status frames,
   end-of-move response, or fixed-response command.

SDK-backed adapters should stay in the inventory, but their default status is
`runtime-backed only` until one of these is true:

- the manufacturer publishes a command/protocol manual independent of the SDK;
- the device uses a public standard protocol such as GenICam/GigE Vision,
  Modbus, SCPI, HID reports with documented report layout, or TMCL;
- open firmware or vendor sample code documents the wire protocol clearly
  enough to implement without linking the SDK.
