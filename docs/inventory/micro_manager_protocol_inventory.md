# Micro-Manager DeviceAdapters Protocol Inventory

This is a first-pass clean-room planning inventory. It answers one practical
question: which devices look implementable in `numanager` without linking a
vendor SDK?

## Classification Rules

`direct`: enough serial/text/binary command framing is known to begin a
clean-room driver from reverse engineered evidence plus public hardware docs.

`likely`: the device is serial/socket/HID shaped, but needs either more reverse
engineering or external public protocol documentation before implementation.

`runtime-backed`: adapter depends on a vendor SDK, C library, camera SDK, or
opaque binary API. Implement the device only through a verified optional
runtime/package backend or after independent protocol evidence is recorded.

## Direct Or Near-Direct Candidates

| Adapter | Hardware | Transport | Status |
| --- | --- | --- | --- |
| `Cephla` | Octopi/Squid controller family | USB serial binary | direct |
| `TeensyPulseGenerator` | Teensy TTL pulse generator | serial binary | direct |
| `Arduino` | Arduino digital IO, shutter, ADC, DAC | serial binary commands | direct |
| `Arduino32bitBoards` | Arduino-compatible MCU IO, DAC, PWM, ADC | serial binary commands | direct |
| `ArduinoCounter` | Arduino pulse counter | serial text | direct |
| `ESP32` | ESP32 hub, switch, shutter, PWM, ADC, XY/Z stage | serial text CSV | direct |
| `OpenUC2` | UC2 Feather hub, XY/Z, laser/LED shutter | serial JSON lines | direct |
| `ASIStage` | ASI MS-2000-style stages, CRISP, LED, turret | serial ASCII | direct |
| `ASITiger` | ASI Tiger hub/peripherals | serial ASCII with card addressing | direct |
| `Cobolt` | Cobolt lasers | serial ASCII | direct |
| `CoherentOBIS` | Coherent OBIS lasers | serial SCPI-like ASCII | direct |
| `CoolLEDpE4000` | CoolLED pE-4000 | serial ASCII | direct |
| `Prior` | Prior stages, shutters, wheels, Lumen, TTL | serial ASCII | direct |
| `LumencorSpectra` | Lumencor light engines | serial ASCII | direct |
| `HamiltonMVP` | Hamilton MVP valve positioners | serial ASCII | direct |
| `K8055` / `K8061` | Velleman USB IO boards | USB packets | direct/likely |
| `3Z_Optics` | 3Z Optics IRIS LED light sources | USB serial / Modbus RTU-style frames | direct, new upstream target |
| `kdv` | Trinamic/TMCL-style motion controller | serial binary | direct |
| `StarlightXpress` | filter wheel | HID/serial small binary packets | direct |
| `SpectralLMM5` | Spectral laser/illumination module | HID or RS-232 | direct/likely |
| `SutterStage` | Sutter Lambda/MPC/MAC stage family | serial ASCII | direct |
| `MP285` | Sutter MP-285 manipulator | serial binary | direct |
| `Marzhauser-LStep` | Marzhauser stages | serial ASCII | direct |
| `Corvus` | ITK/Corvus stage controllers | serial ASCII | likely |
| `ChuoSeiki_QT` | Chuo Seiki stages | serial ASCII | likely |
| `WOSM` | project-defined microscope controller | TCP text | likely |
| `ThorlabsElliptecSlider` / `Thorlabs_ELL14` | Elliptec sliders/rotators | serial ASCII | likely |
| `ThorlabsSC10` | SC10 shutter | serial ASCII | likely |
| `ThorlabsAPTStage` and related APT devices | Thorlabs stages/controllers | APT binary protocol over USB/serial | likely |
| `NewportCONEX` / `NewportSMC` | Newport motion controllers | serial ASCII | likely |
| `PI_GCS` | Physik Instrumente GCS controllers | serial/socket command protocol | likely |
| Standa/8SMC adapters | Standa motor controllers | `libximc` / documented controller protocol | likely |
| `EvidentIX85` | Evident/Olympus IX85 microscope body | serial tags; configured opt-in focus/state/shutter/readback `numanager_drivers::evident_ix85` support exists | likely |
| `Ludl` / `LudlLow` | Ludl stages/filter wheels/shutters | serial ASCII | likely |
| `SutterLambda` | Sutter Lambda filter wheels/shutters | serial/binary | likely |
| `Oxxius` / `OxxiusCombiner` | Oxxius lasers/combiner | serial and some USB/HID references | likely |
| `LumencorCIA` | newer Lumencor control interface | serial or vendor-specific interface | likely |
| `Toptica_iBeamSmartCW` | Toptica laser | serial ASCII | likely |
| `Omicron`, `Vortran`, `Sapphire`, `MPBLaser`, `LaserQuantumLaser` | lasers | serial ASCII | likely |
| `Zaber` | Zaber stages/turrets/wheels | Zaber Motion SDK in current adapter; public ASCII protocol in numanager | implemented through public ASCII motion/readback support with configured/probed axes |

## SDK-Backed Or Opaque Families

Camera adapters are mostly SDK-backed and should be treated separately from
microcontroller/stage/light protocols:

- `Hamamatsu`, `PICAM`, `PCO_Generic`,
  `Basler`, `AlliedVisionCamera`, `DahengGalaxy`, `Hikrobot`, `IDS_uEye`,
  `IDSPeak`, `TUCam`, `FLICamera`, `Lumenera`, `Pixelink`.
- `Andor`/`AndorSDK3` and `PVCAM` have since been split out of the blanket
  runtime-backed category for reverse engineered supports: Andor has a userspace USB/readout
  audit plus runtime-package file-status/digest/loadability/ABI-symbol
  surface, and PVCAM has C ABI/native transport notes plus a
  runtime-package file-status/digest/loadability/ABI-symbol checks,
  camera-name discovery, writable exposure setting, runtime-backed one-shot
  capture, and repeated one-shot stream support. Broader control and native
  continuous streaming remain gated until the
  documented evidence pages are satisfied.
- `Aravis` and `GigECamera` are not proprietary SDK in the same sense, but they
  rely on GigE Vision/GenICam stacks. Treat as a separate transport project, not
  a quick adapter rewrite.

The retry pass found an important camera distinction:

- GenICam/GigE Vision/USB3 Vision/CoaXPress/Camera Link devices may be
  implementable through a standards-first camera transport plus device XML,
  even when the Micro-Manager adapter uses a vendor SDK. This applies especially
  to Basler/pylon, FLIR/Spinnaker, IDS peak, Hikrobot, and Allied Vision-class
  industrial cameras.
- Scientific camera families such as Hamamatsu DCAM, PICAM, and ZWO/Player One
  astronomy cameras still look SDK-first from public information. Andor and
  Photometrics PVCAM now have evidence tracks and runtime-backed capture paths,
  but broader writable controls and streaming still require the device-page
  evidence gates.
- A generic GenICam path is not a quick driver rewrite. It needs its own
  transport layer, stream buffer model, node-map parser, event model, and source
  hygiene/legal review for any non-public interface-standard material.

Other likely SDK/library-backed adapters from pattern scan:

- `Zaber` current adapter includes Zaber Motion headers, while numanager uses the
  public ASCII protocol path for the implemented driver.
- `PI_GCS_2`, Thorlabs APT, and Standa/8SMC adapters may use vendor
  libraries in Micro-Manager, but public command documentation or open library
  source makes SDK-free or audited-source implementations plausible.
- `FLICamera` uses an SDK, but the vendor describes the SDK as open source.
  Treat it as a possible audited vendoring/protocol-extraction target rather
  than a black-box dependency.
- `Mightex_BLS` has since been promoted to an HID output driver plus protocol
  serialization layer for the recorded command/readback surface. Hardware
  validation covers completion, error, unit, and fault behavior; the Mightex
  camera subset now exposes runtime-package checks, writable capture parameters,
  opt-in verified-runtime capture, and repeated one-shot streaming while native
  USB transport remains evidence-gated.

SDK use is only a heuristic. When a Micro-Manager adapter uses an SDK, assume
the public wire protocol is usually unavailable or incomplete, but still check
the manufacturer and standards landscape before ruling it out. `Zaber` is the
important counterexample here: the current adapter uses a vendor library, while
Zaber publishes a complete ASCII protocol that is a better basis for a clean
driver than reverse engineered behavior.

## Source Priority

For each candidate driver, prefer sources in this order:

1. Original manufacturer command manual, protocol manual, or official firmware.
2. Public standards docs for the transport/protocol, for example Modbus RTU or
   TMCL, plus manufacturer register/command assignments.
3. Open project firmware when the device is community/project-defined.
4. Reverse engineered implementation evidence: defaults, command order, quirks,
   timeout behavior, and device variants.
5. Hardware traces when no suitable public documentation is available, and for
   validation before claiming hardware support.

This matters because `numanager` should not merely clone the Micro-Manager
surface. Manufacturer manuals can expose queueing, triggers, ring buffers,
streaming modes, safety state, IO routing, scan modules, and synchronization
features that Micro-Manager never modeled.

## Protocol Specs

The following specs are intentionally concise and implementation-oriented. They
capture reverse engineered command grammar, not complete vendor manual coverage.

### Arduino

Source: Micro-Manager Arduino adapter.

Transport:

- Serial, default detection config sets baud `57600`, 1 stop bit, no
  handshaking.
- Adapter sleeps about 2 seconds after opening the port to avoid the Arduino
  bootloader/update window.
- Responses to text-style version queries are terminated by `\r\n`.
- Several commands are raw single-byte/binary requests.

Detection and version:

| Command | Frame | Reply | Meaning |
| --- | --- | --- | --- |
| controller id | one byte `30` | string starting `MM-Ard...` | identify Micro-Manager Arduino firmware |
| version | one byte `31` | integer text | firmware protocol version |
| pattern count | one byte `32` | `[32, hi, lo]` | max number of patterns, firmware >= 3 |
| DAC channel count | one byte `34` | `[34, count]` | firmware >= 5 |
| digital pin count | one byte `35` | `[35, count]` | firmware >= 5 |

Implementation notes:

- `numanager_drivers::arduino` implements the recorded identification,
  digital-output, shutter, DAC, sequence, timed-output, blanking,
  digital-input, ADC, and input-pull-up opcodes behind configured discovery and
  an explicit real serial backend.
- Further Arduino-family behavior should be added only from firmware source,
  project documentation, captured traffic, or bench logs.

### ESP32

Source: Micro-Manager ESP32 adapter.

Transport:

- Serial text commands.
- Host sends commands with `\r\n`.
- Host reads answers terminated by `\r\n`.

Detection:

| Command | Reply | Meaning |
| --- | --- | --- |
| `V` | `MM-ESP32,<version>` | firmware id/version |
| `U,0` | `U,<travel>` | X travel/range |
| `U,1` | `U,<travel>` | Y travel/range |
| `U,2` | `U,<travel>` | Z travel/range |
| `A,<channel>` | `A,<value>` | analog input count |

Device exposure:

- Hub.
- Digital switch/shutter.
- PWM channels 0-4.
- ADC input through `A,<channel>` readback.
- Optional XY stage if X and Y travel are nonzero.
- Optional Z stage if Z travel is nonzero.

Implementation notes:

- `numanager_drivers::esp32` implements the recorded firmware-version,
  travel-range, GPIO, PWM/shutter, XY/Z motion, position-state, and ADC
  readback paths behind configured discovery and an explicit real serial
  backend.
- Further ESP32-family behavior should be added only from firmware source,
  project documentation, captured traffic, or bench logs.

### OpenUC2

Source: Micro-Manager OpenUC2 adapter.

Transport:

- Serial JSON commands.
- Host sends one JSON object followed by `\n`.
- Host reads a response terminated by `\r`.
- Hub serial access is protected by an IO mutex.

Detection:

| Command | Expected content |
| --- | --- |
| `{"task":"/state_get"}` | reply containing `UC2_Feather` |

Commands visible in adapter:

| Function | JSON command |
| --- | --- |
| laser/LED shutter | `{"task":"/laser_act","LASERid":1,"LASERval":<0 or 255>}` |
| XY absolute move | `{"task":"/motor_act","motor":{"steppers":[{"stepperid":1,"position":<x>,"speed":5000,"isabs":1},{"stepperid":2,"position":<y>,"speed":5000,"isabs":1}]}}` |

Implementation notes:

- The adapter caches XY/Z state in places; a robust driver should prefer
  `/state_get` or a motor-state query if firmware supports it.
- This maps naturally to our hub/device model because one JSON command can set
  multiple steppers.

### ASI MS-2000 / ASIStage

Source: Micro-Manager ASIStage adapter.

Transport:

- Serial ASCII commands through `QueryCommand`.
- `ASIBase` clears serial buffers and reads responses up to a configured serial
  terminator.

Core commands visible:

| Command | Meaning |
| --- | --- |
| `/` | busy/status query |
| `V` | firmware version |
| `BU` | build number |
| `CD` | firmware date |
| `M X=<x> Y=<y>` | absolute XY move |
| `R X=<dx> Y=<dy>` | relative XY move |
| `W X Y` | position query |
| `HERE X=0 Y=0` or `HERE Z=0` variants | define current position |
| `HOME X Y` / `HOME Z` variants | home axes |
| `HALT` | stop motion |
| `TTL Y?`, `TTL Y=<value>` | LED/TTL-style control |
| `MTUR X=<pos>` | turret position |
| `LK`, `LR`, `EXTRA` families | CRISP autofocus state/config |

Units:

- Adapter comments state ASI serial units are tenths of microns for XY/stage
  move commands in this adapter path.

Implementation notes:

- ASI is a high-value target because the protocol is text, multi-device, and
  already hub-shaped.
- Need model shared serial command queue and status query fanout.
- CRISP autofocus should be a second pass after XY/Z and LED/turret.

### ASI Tiger

Source: Micro-Manager ASITiger adapter.

Transport:

- Serial ASCII commands with optional card address prefix.
- Responses commonly verify `:A` for acknowledged commands.

Core commands visible:

| Command | Meaning |
| --- | --- |
| `0 V` | controller firmware version |
| `0 CD` | firmware date |
| `0 BU` | build info |
| `VB F=0` | verbose/output config |
| `W <axis>` | position query |
| `M <axis>=<pos>` | absolute move/state position |
| `J <axis>?`, `J <axis>=<value>` | joystick or wheel parameter |
| `RS <axis>?`, `RS <axis>` | scanner/clocked state |
| `SS` | save settings |
| `LK`, `RA`, `WRDAC`, `E` | PMT/LED/DAC controls |
| `MP`, `MP<n>` | filter wheel position/query |
| `FW<n>` | select filter wheel |
| `HO` | home filter wheel |
| `VR`, `SV`, `LM`, `OF` | filter wheel speed/mode/offset controls |

Implementation notes:

- Treat Tiger as a hub with card-addressed children.
- Discovery should parse build/axis info and instantiate devices from detected
  cards.

### Cobolt

Source: Micro-Manager Cobolt adapter.

Transport:

- Serial ASCII.
- Send terminator: `\r`.
- Receive terminator: `\r\n`.

Core commands visible:

| Command | Meaning |
| --- | --- |
| `@cob0` | select/identify Cobolt device |
| `sn?` | serial number |
| `glm?` | model/type |
| `ver?` | firmware version |
| `l0`, `l1`, `l?` | laser off/on/query |
| `p?`, `pa?`, `p <watts>` | power read/actual/set |
| `hrs?` | operating hours |
| `i?` | current |
| `cp`, `ci`, `em` | operating/control modes |
| `gmlp?`, `gmlc?` | power/current limits |
| `slc <value>` | set laser current |
| `ilk?` | interlock query |
| `gom?` | operating mode query |
| `f?` | fault query |
| `@cobas?`, `@cobas 0/1` | autostart query/set |

Implementation notes:

- Good first laser driver.
- Needs safety model: interlock/fault must be read-only telemetry that gates
  enabling output.
- Current CoboltOfficial upstream also carries a manufacturer-authored
  adapter with 2025-2026 updates for 05/Gen5 lasers, 12 V MLD/DPL variants,
  and 5 V shutter-command handling. That code exposes command families not
  covered by `numanager_drivers::cobolt`, including `laser:*`,
  `system:input:*`, `autostart:*`, `fault:clear`, `gfv?`, `gkses?`,
  `state?`, `gam?`, `gartn?`, `sartn`, Skyra line-addressed commands, and
  modulation current/power setpoints. Treat these as candidate gaps pending a
  Cobolt/Huebner manual revision or hardware traces for the specific laser
  generation.

### 3Z_Optics

Source: Micro-Manager `3Z_Optics` adapter added upstream in June 2026.

Transport:

- USB serial port carrying Modbus RTU-style request/response frames.
- Slave address `0x01`.
- CRC-16/MODBUS over request and response frames.
- Function codes used by the adapter: `0x01` read coils, `0x03` read holding
  registers, `0x04` read input registers, `0x05` write single coil, and `0x06`
  write single holding register.

Core addresses visible in the adapter:

| Address | Access | Meaning inferred from adapter |
| --- | --- | --- |
| `0x01` | input register | device model id |
| `0x20` | holding register | mode: `1` global, `2` independent, `3` TTL |
| `0x21` | coil | dirty/status-change bit used by polling |
| `0x30` | coil/register | global switch and global intensity |
| `0x31 + channel` | coil/register | channel switch and channel intensity |

Adapter properties:

| Property shape | Meaning |
| --- | --- |
| `<channel> Switch` | per-channel on/off state |
| `<channel> Intensity` | per-channel brightness scalar |
| `Global Switch` | global output state |
| `Global Intensity` | global brightness scalar |
| `Mode` | `Global`, `Independent`, or `TTL` |
| `Refresh` | manual readback trigger |

Implementation notes:

- The adapter loads model-specific display names, channel labels, and
  brightness limits from a local `models.json`, falling back to eight generic
  channels and a `0..100` brightness range.
- 3Z product pages confirm IRIS light-source families with controller, TTL
  trigger, and serial communication modes; public product specs list IRIS-400,
  IRIS-400HP/P, and IRIS-600HP/P channel counts, wavelength ranges, TTL timing,
  and USB serial protocol availability.
- I did not find an ungated vendor protocol/register-map manual. The
  `numanager_drivers::three_z_optics` implementation therefore records
  Micro-Manager as the command/register source and marks the behavior
  source-backed rather than bench-validated. Hardware validation or an official
  register-map manual should still be used to confirm serial settings, model
  ids, brightness limits, dirty-bit behavior, fault behavior, and optical
  output.

### Coherent OBIS

Source: Micro-Manager Coherent OBIS adapter.

Transport:

- Serial SCPI-like ASCII.
- Send terminator: `\r`.
- Receive terminator: `\n`.
- Adapter supports indexed command prefixes such as `SYST<n>` and `SOUR<n>`.

Core tokens visible:

| Token | Meaning |
| --- | --- |
| `SYST<n>` | system prefix for indexed device |
| `SOUR<n>` | source prefix for indexed device |
| `SOUR<n>:INF:WAV` | wavelength information |
| `SOUR<n>:POW:LEV:IMM:AMPL` | power set/read |
| `SOUR<n>:POW:LIM:HIGH` / `LOW` | power limits |
| `SOUR<n>:AM:STATE` | analog modulation state |
| `CDRH` | CDRH delay mode |
| `CW` | continuous-wave mode token |

Implementation notes:

- Use typed wavelength/power units.
- Treat CDRH as safety-relevant state because enabling output may include a
  hardware delay.

### CoolLED pE-4000

Source: Micro-Manager CoolLED pE-4000 adapter.

Transport:

- Serial ASCII.
- Send terminator: `\r`.
- Receive terminator: `\n`.

Commands visible:

| Command | Meaning |
| --- | --- |
| `XMODEL` | model |
| `XVER` | firmware/version |
| `CSS?` | channel/status summary query |
| `LAMS` | lamp/channel list |
| `LOAD:<wavelength>` | load/select wavelength channel |
| `CSN` / `CSF` | channel set on/off |
| `PORT:P=ON` / `PORT:P=OFF` | pod/port lock control |

Implementation notes:

- `numanager_drivers::coolled` implements pE-300 and pE-4000/pE-340
  configured serial control/readback for model/version/status, global output,
  channel selection, channel intensity, pE-4000-family wavelength selection,
  and mapped refresh helpers.
- Further CoolLED behavior should be added only from manufacturer protocol
  documentation, public source, captured traffic, or bench logs.

### Prior

Source: Micro-Manager Prior adapter.

Transport:

- Serial ASCII.
- Send terminator: `\r`.
- Receive terminator: `\r`.
- Adapter configures `COMP 0` during initialization.

Commands visible:

| Command | Meaning |
| --- | --- |
| `COMP 0` | controller compatibility/config mode |
| `$` | controller status/info |
| `DATE` | model/date string |
| `G,<x>,<y>` | XY absolute move |
| `GR,<dx>,<dy>` | XY relative move |
| `PS,0,0` | set XY origin |
| `P<x/y>` | axis position query |
| `SIS` | home/init stage |
| `K` | halt movement |
| `SMS`, `SMS,<v>` | XY speed query/set |
| `SAS`, `SAS,<a>` | XY acceleration query/set |
| `SCS`, `SCS,<v>` | XY S-curve query/set |
| `U,<dz>` / `D,<dz>` | Z relative up/down |
| `PZ` | Z position query |
| `RES,Z` | Z resolution |
| `SMZ`, `SAZ`, `SCZ` | Z speed/acceleration/S-curve |
| `V <pos>` | wheel or focus style position set, depending device |
| `7,<wheel>,h` | filter wheel home |
| `7,<wheel>,<pos>` | filter wheel position |
| `8,<id>,<0/1>` | shutter control |

Implementation notes:

- High-value stage driver.
- Need controller-model feature flags because command availability differs
  between Prior controllers.

### SutterStage

Source: Micro-Manager SutterStage adapter.

Transport:

- Serial ASCII.
- Send terminator: `\r`.
- Receive terminator: generally `\n`, with inventory/config parsing also using
  `:` separators.

Commands visible:

| Command | Meaning |
| --- | --- |
| `VER` | version |
| `Rconfig` | peripheral inventory/config |
| `Remres` | encoder/motor resolution |
| `TRXDEL` / `TRXDEL <value>` | transmission delay query/set |
| `STATUS <axis>` | busy/status |
| `MOVE X=<x> Y=<y>` | XY absolute move |
| `MOVREL X=<dx> Y=<dy>` | XY relative move |
| `WHERE X Y` | XY position query |
| `HERE X=0 Y=0` | define XY origin |
| `HOME X Y` | home XY |
| `HALT` | stop |
| `SPEED X Y`, `SPEED X=<v> Y=<v>` | speed query/set |
| `STSPEED X Y`, `STSPEED X=<v> Y=<v>` | start speed query/set |
| `ACCEL X Y`, `ACCEL X=<a> Y=<a>` | acceleration query/set |
| `MOVE <axis>=<pos>` | single-axis absolute move |
| `WHERE <axis>` | single-axis position query |
| `AF Z=<param>` | autofocus/focus parameter |

Implementation notes:

- Good stage target with explicit busy/status commands.
- The inventory parser should discover axis identifiers and module types before
  registering logical devices.

### Sutter MP-285

Source: Micro-Manager MP-285 adapter.

Transport:

- Serial binary protocol.
- Transmit and receive terminator byte: `0x0d`.
- Adapter writes command bytes one at a time.
- Multi-byte positions are written by casting host `long` values into the
  command buffer; a Rust implementation must explicitly choose the observed
  little-endian wire format and test against hardware/manual.

Commands visible:

| Command byte | Meaning |
| --- | --- |
| `0x73` (`s`) | status query |
| `0x63` (`c`) | current position query |
| `0x6d` (`m`) | move to X/Y/Z, followed by three 32-bit position values and `0x0d` |
| `0x6f` (`o`) | set origin |
| `0x56` (`V`) | set velocity, followed by two bytes and `0x0d` |
| `0x61` (`a`) | absolute motion mode |
| `0x62` (`b`) | relative motion mode |
| `0x03` | stop/interruption command |

Error/status bytes visible:

| Byte | Meaning |
| --- | --- |
| `0x30` | serial overrun |
| `0x31` | frame error |
| `0x32` | buffer overrun |
| `0x34` | bad command |
| `0x38` | move interrupted |
| `0x0d` or `0x00` | success/no error in adapter checks |

Implementation notes:

- This is implementable without an SDK but should be treated more carefully
  than ASCII adapters because of binary endianness and timing.
- Driver completion should come from status polling rather than sleeps.

## Open Questions

- For each hardware family, confirm public protocol manuals and licensing
  posture before shipping a non-simulator driver.
- For binary or USB/HID adapters, capture traces from real hardware or compare
  against official command manuals.
- For lasers, define a common safety capability: interlock, emission delay,
  fault reset, output enable, and power/current setpoints must not be naked
  dynamic values.
