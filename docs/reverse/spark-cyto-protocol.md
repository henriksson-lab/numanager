# Tecan Spark Cyto — USB control protocol

## Evidence

| Item | Value |
| --- | --- |
| Status | External-evidence protocol specification; **no hardware validation** — nothing in this note has been seen on an instrument |
| Runtime evidence | Reverse engineered from the vendor Windows stack: managed assemblies, the shipped reference-firmware simulator, and the configuration XMLs |
| Capture evidence | **None yet.** No usbmon/USBPcap capture of a live session exists; §9 lists what a first capture must settle |
| Note coverage | TDCL 2.0 framing and checksum, frame-type table, the ASCII command grammar, the data-package field codes, module topology, the endpoint→channel map, and the host-side photometric math |
| Validation boundary | The framing, type bytes and field codes are recorded as candidate wire facts. Command *spellings* per subsystem, the numeric error-code table, per-axis unit scales, and both devices' VID/PID are **not** established. Rows marked "to confirm" are hypotheses. |
| Provenance | Recovered in the [`brunnim`](https://github.com/henriksson-lab/brunnim) project, which owns the artifacts this was read from; imported here because the driver in `numanager_drivers::spark` is written from it. Micro-Manager has no Spark adapter, so none was consulted. |

A description of the control interface of the Tecan Spark Cyto, sufficient to implement
a driver: the TDCL 2.0 binary framing, the ASCII command language it carries, the
data-channel encoding, the module topology and the photometry. Tecan does not publish
it; their own **SparkControl** drives it. Everything below is stated in terms of what
crosses the wire: keywords, frames, endpoints, encodings.

---

## 1. Big picture — the control stack

```
 SparkControl
        │  high-level instrument functions
        ▼
 Instrument driver ─ builds ASCII command lines:  "ABSOLUTE Z=1234 MODULE=3"
        │
        ▼
 TDCL 2.0 protocol ─ wraps each command/response in a binary frame + XOR checksum
        │  three logical channels per module: Command / Data / Debug
        ▼
 Named pipe ─ a Windows named pipe on the vendor stack
        │  ASCII RPC: "USB,SCH,…"/"USB,DCH,…" open a channel → returns a per-channel pipe
        ▼
 UsbCommunicationService  (NATIVE, *not shipped here*)  ← the RE boundary
        │  does the real USB: VID/PID open, control transfers, BULK/INTERRUPT I/O
        ▼
 USB  ──►  MTP main board  ──(USB↔CAN bridge)──►  CAN peripherals
                                                   (INJ, GCM, COOLING, STACKER…)
```

Two independent USB devices hang off the PC:

1. **The reader mainboard (MTP)** — speaks TDCL over USB; it is also a **USB↔CAN
   gateway** that relays TDCL frames to the CAN-bus modules.
2. **The imaging camera** — a **standard IDS uEye** USB camera. The vendor stack drives it
   directly through the uEye SDK, but that is not the only route: the reader firmware also
   answers `CAMERA …` commands and **uploads the acquired image on its own TDCL Data
   channel** (§12), so a host that speaks TDCL can acquire without touching the camera's own
   USB device at all.

**Key boundary:** the managed code contains *zero* raw USB. The lowest layer we can
see is the ASCII named-pipe RPC. Actual VID/PID and USB control/bulk transfers are
inside the native `UsbCommunicationService` behind the pipe → those require a live
**usbmon / USBPcap capture** to nail down.

---

## 2. USB endpoint & channel model

Endpoint types, as reported in the descriptor:

| value | type      | dir |
|-------|-----------|-----|
| 0     | BULK      | IN  |
| 1     | BULK      | OUT |
| 2     | INTERRUPT | IN  |
| 3     | ISO       | IN  |
| 4     | ISO       | OUT |

Each TDCL module exposes **three logical channels** mapped onto endpoints, taken in
ascending order of address:

| channel | endpoints | pipe RPC | purpose |
|---------|-----------|----------|---------|
| **Command** | INTERRUPT-IN #0 (dev→host) + BULK-OUT #0 (host→dev), same interface | `USB,DCH` (duplex) | send commands / receive replies |
| **Data**    | BULK-IN #0 | `USB,SCH` (simplex) | bulk streaming: images, measurement blobs |
| **Debug**   | INTERRUPT-IN #1 (the *second* interrupt-in) | `USB,SCH` (simplex) | firmware log output |

So a reader interface has ≥ 1×BULK-IN, 1×BULK-OUT, 2×INTERRUPT-IN. (ISO is defined
but no channel requests it.)

USB descriptors are **not read locally** — the native controller returns them as XML
in the `ScanModules` reply. `StringDescriptor3` (USB string index 3) is a
`|`-separated `Key=Value` blob: `S`=instrument serial, `N`=module number,
`P`=protocol, `M`=mode, `F`=instrument family, `U`=USB/module serial, `E`=is-extern.
`ProductId` (an int) selects the module type.

---

## 3. Named-pipe RPC grammar (the vendor stack on Windows)

ASCII, comma-delimited fields, `;`-terminated. Replies start with `ACK`/`NACK`/`Error`.

```
Enumerate modules:   USB,ScanModules;          →  ACK,<xml-blob>;
Open simplex chan:   USB,SCH,{inst},{ser}:{if}:{ep};
                                               →  ACK,{inst},{ser},{if},{ep},{pipeName};   (6 tokens)
Open duplex  chan:   USB,DCH,{inst},{ser}:{if}:{epIN},{ser}:{if}:{epOUT};
                                               →  ACK,{inst},{serIN},{ifIN},{epIN},{serOUT},{ifOUT},{epOUT},{pipeName};  (9 tokens)
Close:               Close,{pipeName};          →  ACK,Close,{pipeName};   /  NACK,Close,…;
Reject:              …                          →  NACK,{message};
Async (unsolicited): ASYNC,UsbCam,ModuleArrival   |   ASYNC,UsbCam,ModuleRemoved,{a},{b};
```

The reply's `{pipeName}` is a `\\server\pipe\Name` path; the managed side opens a
`NamedPipeClientStream` to it (`PipeOptions.Asynchronous|WriteThrough`, 1 s connect
timeout) and then runs **TDCL framing** over that per-channel pipe. That per-channel
pipe is a byte-for-byte proxy of the USB endpoint.

The `ScanModules` XML schema:
```xml
<Module ProductId="<int>" StringDescriptor3="S=..|N=..|P=..|M=..|F=..|U=..|E=..">
  <Interface InterfaceNumber="<int>">
    <Endpoint address="<int>" maxPacketSize="<int>"
              type="BULK|INTERRUPT|ISO" direction="IN|OUT"/>
  </Interface>
</Module>
```

---

## 4. TDCL 2.0 wire framing

Every frame, both directions:

```
+--------+--------+---------+---------+= … =+----------+
| type   | seq    | len_hi  | len_lo  | payload | checksum |
| 1 byte | 1 byte |  2 bytes (BIG-endian)  | len B   | 1 byte   |
+--------+--------+---------+---------+= … =+----------+
 \_________ 4-byte header ___________/         XOR of all
                                               preceding bytes
```

- **len** = big-endian length of `payload` only (header + checksum excluded).
- **Integers on the wire are big-endian, fixed width** (`CreateInt`/`ReadInt`):
  len=2 B, error/msg number=2 B, timestamp=4 B, terminate time=4 B, busy time=2 B.
- **Checksum = 8-bit XOR fold** over the 4 header bytes + all payload bytes
  (`c=0; for b: c ^= b`). Verified but not itself folded. Mismatch →
  `"Checksum mismatch in received command"`.
- **seq** = per-request byte; a response echoes the request's seq.
- **type** high bit (0x80) set ⇒ frame is device→host.

Frame types:

| type | dir | name | payload |
|------|-----|------|---------|
| 0x01 | →dev | Ascii command | ASCII command string |
| 0x02 | →dev | Terminate | 4-byte time |
| 0x03 | →dev | Binary | raw bytes |
| 0x81 | ←dev | Ready (success/data) | ASCII (may be empty) |
| 0x82 | ←dev | Terminate ack | 4-byte time |
| 0x83 | ←dev | Binary payload | raw chunk (≤65530 B/frame) |
| 0x84 | ←dev | Busy | 2-byte time |
| 0x85 | ←dev | Message | 2-byte msgNo + ASCII |
| 0x86 | ←dev | Error | 4-byte ticks + 2-byte errNo + ASCII |
| 0x87 | ←dev | Log | ASCII |
| 0x88 | ←dev | Data header | raw header bytes |
| 0x89 | ←dev | AsyncError | 4-byte ticks + 2-byte errNo + ASCII |

Worked examples (seq=0x05):
- Ready(""):          `81 05 00 00 84`
- Command "IDN?":     `01 05 00 04 49 44 4E 3F 7C`
- Busy(time=0x0102):  `84 05 00 02 01 02 80`

A bulk transfer = one `0x88` header frame + one or more `0x83` payload frames
(chunked at 65530 = 65535 − 4 header − 1 checksum).

---

## 5. Instrument command language (payload of a 0x01 frame)

ASCII, human-readable, keyword=value lines:

```
[<prefix>]KEYWORD [SUBKEY] [KEY=VALUE …] [MODULE=<n> [NUMBER=<n>] [SUB=<n>]]
```

Prefix selects the operation:
- (none) = **set / execute**
- `#`     = **get definition / allowed range / list**
- `?`     = **get current value / state**

Responses: `KEY=VALUE` pairs on a **Ready**; ranges as `from~to[unit]`; errors as
`ERR###:text` / `ASYNC_ERR###:text` (3-digit code + text). Example triad:
`#BEAM {mod}` (list) · `BEAM {mod}={val}` (set) · `?BEAM {mod}` (get).

Command groups (one subsystem each):
motion/axes (`ABSOLUTE`, `INIT`, `MOVE`), plate transport + lid + stacker, filter
slides (`DEFINE FILTER`), mirrors/dichroics (`DEFINE MIRROR`), beam/objective/
polarisation, light sources (`LIGHTING`, `LASER POWER`, `LED`), detectors
(`GAIN`, `TIME`, `READ`), camera + autofocus (`CAMERA …`), temperature/gas/cooling
(`GASCONTROL`, `TEMPERATURE DEVICE=…`), injectors (`INJECTOR …`), measurement
sequencer (`MEASUREMENT START/END`, `MODE`, `SCAN`), shaking, barcode, sensors/
counters, info/module/config (`?INFO`, `#MODULE`), firmware up/download, reset/service.

**Focus is motion, not camera.** The objective's height is an ordinary axis: `ABSOLUTE
Z={steps} MODULE={n}` on the imaging module moves it, `?ABSOLUTE` reads it back, so a
viewer can drive focus by hand. `CAMERA AUTOFOCUS …` runs a sweep and reports its
result (`?CAMERA AUTOFOCUSDETAIL IMAGE={n}` → `MAXVALUE=… STDDEV=…`), and an imaging
acquisition additionally carries a focus offset in µm applied on top of the height
autofocus found. Z accuracy is ≈0.04–0.2 µm depending on objective; autofocus sweeps
in 100/50/20 µm steps for 2×/4×/10×.

Firmware/EEPROM read: `UPLOAD SECTION NAME=EEPROM …`; identity: `?INFO SAP_SERIAL_INSTRUMENT`,
`?INFO INSTRUMENT_TYPE`, `?INFO HARDWARE_VERSION`.

---

## 6. Module topology

Family `SYMBIO`, protocol `TDCL2.0`. Instrument variants **Spark 10M / 20M**.

**USB-attached:** `MTP` (main board / plate transport, always ModuleNumber 0),
`ABS` (absorbance), `FLUOR` (fluorescence intensity), `LUM` (luminescence),
`USBCAM`/`USBCAM2` (cameras, `IsExtern=true`).

**CAN-attached (via MTP bridge):** `FIM` (fluorescence imaging head), `CELL`
(brightfield cell imaging), `INJ` (injector), `GCM` (gas CO2/O2), `COOLING`,
`BARCODE`, `STACKER`, `PODI` (power distribution).

---

## 7. Camera (IDS uEye)

Standard IDS uEye USB camera via the vendor stack (classic `is_*` API). Monochrome
(`IS_CM_MONO8`). Model/sensor queried at runtime (not hardcoded). Reader
hardware-triggers it (`SetExternalTriggerMode`) and the uEye flash/strobe output gates
the illumination LEDs (`SetFlashMode`, `GetGlobalFlashParams`). Calibration/identity in
camera EEPROM; firmware uploaded at `InitCamera`. Hot-plug surfaces as
`ASYNC,UsbCam,ModuleArrival` on the pipe. **Public IDS VID = 0x1409 (confirm live).**

Two routes exist, and this repository takes the second: drive the camera directly with a uEye
driver, **or** let the reader do it and take the pixels off the TDCL Data channel
(`CAMERA INIT` → `EXPOSURETIME`/`AOI`/`TRIGGERMODE` → `CAMERA TAKEIMAGE` → `0x88`+`0x83`).
The second needs no vendor SDK and no second USB device, at the cost of going through the
reader's own acquisition path.

---

## 8. Open-source driver — recommended path

**Two viable strategies:**

- **(a) Ride the native controller (Windows-only, fastest):** connect to
  `the controller pipe`, speak the §3 grammar, then §4 TDCL + §5 commands.
  Lets you skip raw USB entirely, but keeps the closed native service.

- **(b) Full open stack (Linux/cross-platform, the real goal):** replace the native
  service with a libusb transport. You already have everything above the pipe (§4/§5/
  §6). What's still needed from **live hardware**:
  1. Reader mainboard **VID/PID** + USB descriptor (interfaces, endpoint addresses,
     maxPacketSize) — `lsusb -v`.
  2. A **usbmon/USBPcap capture** of a real session to confirm: which endpoints carry
     command vs data vs debug, the enumeration/handshake before `ScanModules`, and any
     control transfers used to fetch `StringDescriptor3`.
  3. Confirm the camera VID/PID + IDS model for USBCAM vs USBCAM2.
  Then: open the device with libusb, map the three channels to the endpoints per §2,
  implement TDCL framing per §4, and drive it with the §5 command set. The camera is a
  separate libueye device (§7).

**Suggested first milestones on real hardware:**
- `lsusb -v` for both devices → fill in the VID/PID/endpoint gaps.
- usbmon capture of app startup → observe the exact `ScanModules` XML and the first
  `?INFO`/`#MODULE` exchange; validate the §4 frame layout against real bytes.
- Send a read-only `?INFO SAP_SERIAL_INSTRUMENT` over BULK-OUT, read INTERRUPT-IN,
  verify the 0x81 Ready frame + checksum.

---

## 9. Remaining unknowns (need live hardware)

1. Reader mainboard **VID/PID** and USB descriptor (endpoint addresses/sizes).
2. Camera **VID/PID** and exact IDS model (USBCAM vs USBCAM2).
3. Any **USB control-transfer handshake** the native service does before/around
   `ScanModules` (device open, string-descriptor fetch, reset).
4. Firmware **numeric error-code table** (`ERR###` → meaning) — lives in device
   firmware, not in these managed DLLs (only the textual error stack is read back).
5. Command **unit scales** (steps vs µm, ms vs µs, nL) — partially in the config XML
   range definitions; confirm per subsystem.
6. Whether ISO endpoints are ever used (enum supports them; no channel requests them).

---
---

# Part II — Deep-dive findings (round 2)

Part II covers the reference firmware's behaviour, the measurement and image data
codecs, the full command dictionary, the automation-level API and the configuration
XMLs. Corrections to Part I are flagged **[CORRECTION]**.

## 10. Command dispatch & connection handshake (from the simulator)

Tecan ships a simulator that is a complete
attribute-driven **reference firmware** (~90 command handlers). Behaviour a driver
can rely on:

- A command line `[prefix]KEYWORD [KEY=VAL] [MODULE=n]` → the prefix selects the
  operation (`#`=List, `?`=Query, none=Action); the keyword resolves
  KEYWORD(+module) to a handler whose `OnExecute{Query,List,Set}Command` returns the
  string that becomes the **Ready (0x81)** payload.
- **Every command emits `Busy (0x84, time=5000)` first, then the `Ready`** (or an
  `Error 0x86`). A driver must expect a Busy before the terminal response.
- Handshake: the CAP pipe; a module scan returns
  the same `<TecanModules>` XML as the real `CapConnection.CreateModulesTag`; per
  module the controller opens `…Command_/…Data_{instr}_{mod}` channel pipes; arrivals
  announced via `ASYNC,<id>,ModuleArrival`.
- Representative request→response pairs (verbatim formats):
  `?INFO INSTRUMENT_TYPE` → `INSTRUMENT_TYPE=SPARK 10M`;
  `?INSTRUMENT STATE` → `STATE=STANDBY|READY`;
  `?…EXPECTED_USB` → `…MTP:0|USBCAM:1`; `?CAMERA ISINITIALIZED` → `ISINITIALIZED=TRUE`;
  `?CAMERA AOI` → `X=.. Y=.. WIDTH=.. HEIGHT=..`; gas → `CONCENTRATION O2 = {v} {unit}`.
- Error payload confirms Part I: `Error 0x86` body = `[ticks:4][errNo:2][ASCII text]`.
  (Simulator error numbers e.g. 123/1234/1240-1243/8001/9999 are sim-invented; real
  firmware codes will differ.)

## 11. Command dictionary & units  (`command_dictionary.md`)

- the firmware-name table is **not a switch** — it reads a
  `[FirmwareName("…")]` attribute on each enum field (fallback = C# name). ~30 enums
  recovered in full (`MeasurementMode` = ABS/CUV/LUM/ALPHA/FITOP/FIBOTTOM/FP/CELL/
  INJ/WELL_TEMP/BARCODE/FIM; `FilterType`, `MirrorType`, carriers, objectives,
  `LightingName`, counters …), and all 32 module command literals.
- **[CORRECTION] Units are device-driven and taken literally from the wire.**
  `ValueWithUnit` does `Enum.Parse(Unit, <bracket text>)`, so the `[unit]` token in a
  range *is* the `Unit` enum member name. Notable scales from the 24-member `Unit`
  enum: **`ang` = ångström** (wavelength on the command/range side — *not* nm),
  `c100` = 0.01 °C, `ulPerS` = µl/s, `step` = raw motor steps, plus `mHz/dHz/hz10/
  fps/ppm`. Range wire format: `{from}~{to}%{step} [unit]` (count) or `:{step}` (delta).
  (Note the Data-channel wavelength field is separately scaled nm=raw/10, §13.)

## 12. [SUPERSEDED] Images and the Data channel

> **Corrected against the primary notes.** This section originally claimed camera pixels bypass
> the TDCL data channel entirely. The command catalog and the reference-firmware notes say
> otherwise: `CAMERA TAKEIMAGE` (or `PREPARETAKEIMAGE` + `FETCHIMAGE`) **uploads the image on
> the Data channel**, as one `0x88` header frame plus `0x83` payload frames — the same framing a
> measurement package uses, with the payload being the raster itself (stride =
> `Width * BitsPerPixel / 8`). The simulator takes a WCF shortcut for its own images, which is
> what this section described; on real hardware the pixels ride TDCL.
>
> What remains true below: the *application* stores rasters as TIFF and keeps only segmented
> object lists in HDF5, and the scalar `TdclDataType` codec is photometry-only — an image
> payload is raw pixels, not typed fields. The consequence drawn below — that an open imaging
> path requires driving the uEye camera directly — does **not** follow, and this driver
> implements `CameraCapture` over TDCL instead.

### Original text

Part I §2 implied images arrive as Data-channel binary. In fact the `TdclDataType`
codec (§13) is **scalar-photometric only** (ABS/FLUOR/LUM). **Camera pixels bypass
`0x88`/`0x83` entirely**: native IDS uEye → OpenCV `cv::Mat` → **TIFF** files
(grayscale 8/12/16-bit or RGB 24/32-bit; `ImageDataType` in the workspace SQLite DB
carries Width/Height/PixelSizeInNm). HDF5 (v1.12.0) stores only **segmented object
lists** (`BlobDescriptor`), not rasters. So an open imaging path = drive the uEye
camera directly (libueye) and read pixels natively; only the photometric readers use
TDCL binary framing.

## 13. Data-channel scalar format  (`data_formats.md`) — implemented in `src/driver/data.rs`

- `0x88` header payload = an **ordered list of 1-byte type codes** (no well/id/
  dimension metadata). `0x83` payload = **big-endian packed scalars** decoded in
  header order, each consuming its width. Result binds to a well/label out-of-band
  via the TDCL **`seq`** byte (host pre-registers a waiting-order for the seq it uses).
- Full **33-entry `TdclDataType` table** recovered (widths + meaning). Only in-codec
  scaling: **Temperature = raw/100 °C**, **Wavelength = raw/10 nm**; everything else
  is raw counts. `U16MULT` (code 18) = a loop-count that repeats the trailing field
  block (kinetics/multi-read); `U16MULT_H` (19) = multi-header marker.
- **counts → OD/RFU/RLU is NOT in this codec** — done downstream in
  the detection families (§13b). Now recovered and implemented in
  `src/driver/photometry.rs`.

## 13b. Photometric math (counts → OD/RFU/RLU/mP)  (`photometric_math.md`)

All of it runs host-side, not in firmware. Inputs are raw
unsigned counts + gain/attenuation/time that ride alongside them in the TDCL2 package.

```
OD  = −log10( max( I_sample/I0 , 10^(−maxOd) ) ), rounded 4dp
        I  = mean_flash[(M−D_M)/(R−D_R)]  (dark-subtracted meas/ref ratio, lamp-referenced)
        I0 = same on the air/blank "prepare" read;  maxOd = ValidationConfig.MaxOpticalDensity
        (gain cancels in the ratio — no gain term). pathlength=(OD_test−OD_ref)/plc; OD_1cm=OD/pathlength
RFU = (S−D_S)/(R_meas−D_R)·K,   K=(R_bright−D_R)/(65536−G_ref)·65536   (NaN/Inf→0; signal gain kept)
RLU = calFactor·(cpsLin_meas − cpsLin_dark),  cpsLin=(cnt/t)/(1−(cnt/t)·τ)
        then ·10^(att/10000) attenuation restore; ·t if OutputDataAs==34 (→counts); over-range if >13e6
FP  : I‖=RFU‖−blank, I⊥=RFU⊥−blank;  G=sqrt((I_‖⊥·I_⊥⊥)/(I_‖‖·I_⊥‖))
      mP=1000·(G·I‖−I⊥)/(G·I‖+I⊥);  r=(G·I‖−I⊥)/(G·I‖+2I⊥);  Itot=G·I‖+2I⊥
```

Validation bitmask: 0x1 lumi over-range, 0x4 flash-dropout, 0x20 OD NaN, 0x800 bad PLC,
0x1000 unknown attenuation. Open: integration/dead-time unit codes and per-filter OD values
(config-dependent) — confirm from a capture.

## 14. Automation / higher-level API

Above the firmware protocol there is a **documented public automation API** — a
legitimate control path worth mirroring for semantics:
- the vendor stack — in-process .NET facade (`AutomationInterfaceFactory` →
  `IInstrumentManagement`, `IInstrument.Acquire/Release/PlateIn/PlateOut`,
  `IMethodExecution.CheckMethod/ExecuteMethod(instrument, methodXml, name, StackerData)
  /GetResults`). Transport = **WCF over a local named pipe**
  to a host service (started with the native USB service by
  a local agent process).
- WCF contracts: method mgmt, execution control (Execute/Pause/
  Continue/Cancel), result subscription (by well-index × label-index), streamed
  **LiveViewer** camera frames, workspace (SQLite) access, and a **direct hardware
  path** `IInstrumentControlContract.ExecuteHardwareCommand[WithParameters]` keyed by
  ~35 hardware command keys (PlateIn/Out, filterslides, mirrors, injectors,
  temperature, gas, barcode, stacker).
- Measurement model: a **Method = tree of "Strips"** (Plate→Detection→Action→Kinetic
  →DataAnalysis→Export), serialized as **XAML** (`schemas.tecan.com/at/dragonfly/
  operations/xaml`) stored as a blob in the workspace SQLite DB. Detection strips
  per mode (Absorbance/FI/LUM/CellImaging/FIM/NanoQuant); addressing via
  `AbsolutePosition` (intra-well grid) + `MultipleReadsPerWell`.

## 15. Concrete hardware parameters

From the simulator config XMLs (real numbers, but a Spark Cyto's DB may differ):
- **Objectives** 2X/4X/10X: magnifications 0.99 / 2.005 / 5.005; per-objective AF
  offset/times/stepsize (100/50/20 µm) and Z ranges; XY envelope ≈130×90 mm.
- **FI optics**: excitation mono 234–894, emission mono 300–890 (2-step), 20-unit
  bandwidth, beam Ø 5.4/2.6; dichroics 510/560/625 + Automatic + 50/50; six ex/em
  filter pairs (e.g. Em 535/25, Ex 485/20). *(Wire unit is ångström, §11.)*
- **Environment**: Temp 24.00–42.00 °C (0.10 step); CO₂ 0.04–10%; O₂ 0.1–21%.
  Shaking Linear/Orbital/Double, 11 amplitude presets 0.8–6 mm. Injectors A/B:
  1000 µl syringe, 100–300 µl/s, 100–1000 µl.
- **Camera**: classic IDS uEye, 8-bit mono default; model/sensor/resolution all
  runtime-queried (nothing hardcoded). Trigger = hardware external; flash mode
  **`IO_FLASH_MODE_TRIGGER_HI_ACTIVE`** — the camera's HI-active flash output gates
  the illumination LEDs. Instrument serial (e.g. `PT12345678`) stored in camera
  EEPROM; firmware uploaded at `InitCamera`. 10M→USBCAM, imaging 20M→USBCAM2 (same
  uEye code path; differ only as module-type codes).

## 16. Where this is implemented

In this repository the wire layer is `numanager_drivers::spark` (`tdcl`, `commands`, `data`,
`parse`, `catalog`, `session`, `backend`) and the device graph is
`numanager_drivers::spark_cyto`; see [`../devices/spark-cyto.md`](../devices/spark-cyto.md)
for what each capability is backed by and what is still modeled. Per this repository's rules
those modules carry no inline tests — the codec vectors in §4 are asserted from the
`brunnim` side, which owns the artifacts.

The list below is the originating project's own implementation record, kept because it
states which recovered spec each piece was written against:
- `tdcl.rs` — TDCL 2.0 frame codec (encode/decode, XOR checksum, streaming, response
  parsing), validated against the §4 byte vectors.
- `commands.rs` — the command-line builder (prefixes,
  `KEY=VALUE`, `MODULE/NUMBER/SUB`, space-quoting).
- `data.rs` — Data-channel scalar decoder (the §13 33-entry table, big-endian
  consumption, temp/wavelength scaling, `U16MULT` repeats).
- `catalog.rs` — ~278 typed enum variants for the command vocabulary (`firmware_name`
  round-trips) + the 32 module command literals.
- `photometry.rs` — OD/RFU/RLU/mP math (§13b).
- `model.rs` — ScanModules XML + `StringDescriptor3` parsing + endpoint→channel map.
- `parse.rs` — response `KEY=VALUE`/range + command-line parsing.
- `transport.rs` — the `Transport` trait + in-process `Loopback`.
- `sim.rs` — a reference-firmware `Simulator` (Busy→Ready, data frames) — a subset,
  extend as commands are validated against captures.
- `session.rs` — `Session`: seq allocation + Busy→Ready/Error state machine.
- `bin/tdcl_decode.rs` — `tdcl-decode`: decodes a captured endpoint stream into JSONL.
- `tests/loopback.rs` — **end-to-end**: a `Session` drives the `Simulator` through the
  whole stack (command → TDCL → dispatch → data decode → OD). No hardware.

- `usb.rs` — **real hardware transport** (feature `usb`): libusb via `rusb` (vendored),
  cross-platform (Linux/macOS/Windows). Enumerates the device, derives the
  command/data endpoints from its descriptors per §2, reassembles TDCL frames, and
  implements the same `Transport` trait — so `Session`/`engine` run on real hardware
  unchanged. `bin/usb-scan` lists USB devices to find the reader's VID/PID.

TODO (need live hardware): the reader's **VID/PID** (set in `usb::UsbConfig`; get it
from `usb-scan`/a capture) + validating the endpoint layout against a real descriptor;
optionally a the controller pipe named-pipe client for Windows-with-vendor-service. The
loopback + the compiled libusb transport prove everything else already fits together.
