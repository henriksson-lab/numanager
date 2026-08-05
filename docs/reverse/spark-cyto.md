# Tecan Spark Cyto — Reverse Evidence Note

## Status

Reverse engineered protocol evidence exists and is recorded in
[`spark-cyto-protocol.md`](spark-cyto-protocol.md): TDCL 2.0 framing with its checksum, the
frame-type table, the ASCII command grammar, the data-package field codes, the module
topology and the endpoint→channel map. A driver is written from it
(`numanager_drivers::spark_cyto` plus the `spark` wire modules) and reaches an instrument
through a configured USB transport.

**No part of it has met hardware.** There is no capture of a live session, so every command
spelling is a hypothesis: the ones taken from the recovered command dictionary are likely
right, and the ones inferred around them are likely wrong. The driver states which is which
in its device page rather than presenting them as equivalent.

**Camera pixels come through the reader.** The imaging camera is a stock IDS uEye on its own
USB connection, but the reader firmware also drives it: `CAMERA TAKEIMAGE` acquires and
**uploads the raster on the TDCL Data channel**, as one `0x88` header frame plus `0x83`
payload frames, rows of `width * bits_per_pixel / 8` bytes. So `CameraCapture` is served
without a vendor SDK and without opening the camera's own USB device. (An earlier summary of
this evidence claimed the opposite; the command catalog and the reference-firmware notes
settle it — see protocol note §12.)

One thing is deliberately *not* implemented here: **photometric conversion**. Counts cross the
capability boundary; OD/RFU/RLU are the application's arithmetic, from the settings that travel
with the counts.

## Protocol Evidence Summary

| Area | Recovered | Class |
| --- | --- | --- |
| Frame layout | `type, seq, len(BE u16), payload, XOR checksum`; `0x80` bit marks device→host | reverse engineered |
| Frame types | 12 types, including `Busy 0x84`, `Ready 0x81`, `Error 0x86`, `DataHeader 0x88`, `Binary 0x83` | reverse engineered |
| Response discipline | Every command answers `Busy` first, then `Ready` or `Error` | reference-firmware simulator |
| Command grammar | `[#\|?]KEYWORD [SUBKEY] [KEY=VALUE …] [MODULE=n]`; ranges as `{from}~{to}%{step} [unit]` | reverse engineered |
| Units | Device-driven and literal: `ang` (ångström) for wavelength, `c100` (0.01 °C), `step`, `um`, `ulPerS` | reverse engineered |
| Data package | 33-entry field-code table, big-endian packed scalars, header announces layout, `seq` binds result to request | reverse engineered |
| Module topology | USB: `MTP`, `ABS`, `FLUOR`, `LUM`, `USBCAM`; CAN via MTP bridge: `FIM`, `CELL`, `INJ`, `GCM`, `COOLING`, `BARCODE`, `STACKER`, `PODI` | reverse engineered |
| Endpoint map | Command = INTERRUPT-IN #0 + BULK-OUT #0; Data = BULK-IN #0; Debug = INTERRUPT-IN #1 | reverse engineered |
| Environment ranges | Temp 24.00–42.00 °C, CO₂ 0.04–10 %, O₂ 0.1–21 % | vendor configuration XML |
| Objectives | 2X/4X/10X, magnifications 0.99/2.005/5.005, autofocus steps 100/50/20 µm | vendor configuration XML |

## Evidence To Collect

Ordered by how much each unblocks.

1. **`lsusb -v` on both USB devices.** VID/PID and the full descriptor for the reader
   mainboard, and the same for the camera. The mainboard's VID/PID is the one value that
   stops the transport from being usable at all; the driver takes it from configuration
   because no default can be invented.
2. **A usbmon/USBPcap capture of a SparkControl session, from plug-in through one
   measurement.** Settles: the §4 frame layout against real bytes, which endpoint carries
   which channel, the enumeration/handshake before `ScanModules`, and — most valuable — the
   real command spellings for plate positioning, the measurement envelope, monochromator
   tuning and the environmental readbacks.
3. **A `#`-prefixed range sweep per axis and per subsystem** (`#ABSOLUTE`, `#TEMPERATURE`,
   `#GASCONTROL`, …). The reply carries the unit token, which is what resolves the
   steps-versus-micrometres question for motion without anyone guessing a calibration.
4. **The firmware numeric error table.** `ERR###` codes live in device firmware, not in the
   managed assemblies; only the textual stack is readable today.
5. **Whether this instrument has an absorbance monochromator that scans.** The recovered
   vocabulary carries `MEAS_ABSSCAN`/`MEAS_FISCAN`/`MEAS_LUMSCAN`, but they are the SPARK
   family's, and nothing establishes that a Cyto answers them.
6. **What the image's `0x88` header carries.** The scalar codec's header is a list of field
   codes; an image payload is a raw raster instead. This driver shapes it from the geometry
   the camera reports (`?CAMERA AOI`, `?CAMERA BITSPERPIXEL`) and rejects a payload whose
   length disagrees — a capture confirms whether the header also states the geometry, and
   whether `PREPARETAKEIMAGE`+`FETCHIMAGE` differs from the single-command form.
7. **The camera's own identity** — VID/PID and model, needed only for the alternative route
   of driving it directly. A U3V-capable model would already be served by
   `numanager_drivers::usb3_vision`.

## Protocol Questions

- Does `PLATEPOS` address wells at all, or only carrier positions (`PLATE_IN`, `OUT_LEFT`,
  `OUT_RIGHT`)? The evidence records only the latter, which is why this driver treats well
  addressing as a measurement-time concept rather than a transport move.
- What closes a measurement window if a command inside it fails — does `MEASUREMENT END`
  still have to be sent, and does the instrument reject the next `MEASUREMENT START` if it
  is not?
- How is the monochromator tuned? A wavelength-setting keyword is not in the recovered
  dictionary; only that wavelengths cross the wire in ångström.
- Is enable/disable on the environmental subsystems spelled `MODE=` (as the action table
  records for gas) or `CONTROL=`?
- Which module number carries the imaging axes, and are X, Y and Z one module or several?
- Does the reader report an actual chamber temperature, and under which keyword —
  `?TEMPERATURE DEVICE=…`, a sensor keyword, or a measurement mode (`WELL_TEMP`)?

## Candidate Public Surface

Backed by the recovered command dictionary, and implemented:

- `PlateMove` over carrier positions, `Measure` over the `MEASUREMENT START` … `SCAN` …
  `MEASUREMENT END` envelope returning raw counts plus their settings,
  `TemperatureControl`, `GasControl` including O₂, `StageMove`/`StageHome` over
  `ABSOLUTE`/`INIT` with the unit read from the instrument, `FilterSelect` for the
  excitation slide and the mirror carrier, `ImagingHead` for the objective, injector
  actions, and barcode reads.

- `CameraCapture` over `CAMERA TAKEIMAGE`, with the raster taken off the Data channel and
  the frame's geometry from `?CAMERA AOI` / `?CAMERA BITSPERPIXEL` rather than assumed.

Not exposed, and why:

- **A wavelength-scan capability** — see Evidence To Collect §5. A host that wants a
  spectrum writes `wavelength` and submits `Measure` repeatedly, which works on any reader.
- **Firmware upload, EEPROM writes and service commands** — hidden maintenance surface per
  the device-docs policy.

## Stop/Proceed Decision

**Proceed**, with the boundary stated in the driver rather than in a comment: commands
derived from the recovered dictionary are sent and their failures surface with the
instrument's own error number and text; behaviour with no evidence is refused explicitly
instead of being answered from the model. A driver that fails visibly on first contact is
the intended outcome of writing from notes — it is what turns the capture in Evidence To
Collect into a short list of corrections rather than a rewrite.

## Implementation Gate

- Any capability whose command spelling is not in the recovered dictionary must be marked
  "to confirm" in [`../devices/spark-cyto.md`](../devices/spark-cyto.md) and must fail
  visibly at the instrument rather than being answered locally when a transport is attached.
- Hardware-support claims wait for: a descriptor dump, a session capture, and per-subsystem
  runtime output/readback traces recorded against
  [`../devices/hardware-validation-template.md`](../devices/hardware-validation-template.md).
- A camera frame is published only when the payload length matches the geometry the camera
  reported. A raster reshaped to fit would be a picture of something that was not measured.
