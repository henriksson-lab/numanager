# Beckman Coulter Echo Acoustic Liquid Handlers (525 / 650 series) — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | Beckman Coulter Life Sciences (Danaher; technology from Labcyte) |
| Family | Echo 525, Echo 650 / 650 Plus series |
| Robot class | Acoustic droplet ejection (ADE) transfer station. No tips, no nozzles, no contact. |
| Evidence quality | Good for principle and headline specs (vendor technology pages); weak for mechanics and interfaces. |

## Operating Principle

An acoustic transducer under the source plate focuses ultrasound at the fluid
surface of one well and ejects a precisely sized droplet **upward** into a
destination plate held inverted above it. Nothing enters the liquid.

| Item | Detail |
| --- | --- |
| Droplet size | 2.5 nL (both 525 and 650 families) |
| Per-well transfer | Echo 525: up to 2.5 µL from each source well |
| Rate | Echo 650 Plus: up to 700 drops/second |
| Transducer | Echo 650 Plus uses a next-generation transducer with a titanium lens |
| Transfer topology | Any source well to any destination well, including combinatorial patterns |
| Source labware | Echo Qualified microplates and reservoirs are **required**; 384- and 1536-well formats, plus 96-format sample tube arrays on the 650 Plus |

Volume is quantised: every transfer is an integer number of 2.5 nL drops. A
device model that assumes continuous volume commands will misrepresent this.

## Acoustic Sensing (Survey / Audit)

This is the most distinctive sensor in the whole survey. Before transferring, the
instrument pulses each source well and listens to the echo:

| Measurement | Derived from |
| --- | --- |
| Fluid volume / surface height in the well | Time of flight of the reflected pulse |
| Acoustic impedance of the fluid | Amplitude characteristics of the echo |
| Fluid composition | Impedance → e.g. DMSO concentration in DMSO-based samples, glycerol concentration |

Fluid classes must be declared so the instrument calibrates ejection correctly:
DMSO calibration for reagents at 70–100 % DMSO, Glycerol calibration for reagents
up to 50 % glycerol that may contain DNA or protein.

Echo Plate Audit software exists specifically to visualise, track and compare
these acoustically measured sample characteristics within and across plates.

So the Echo produces a **per-well analytical measurement** as a first-class
output, independent of any liquid actually being moved. That is a plate-reader-like
capability living inside a liquid handler.

## Motion And Plate Handling

| Item | Detail |
| --- | --- |
| Axes | A stage positions the source plate over the fixed transducer, and a destination holder inverts the target plate above it. Exact axis count and travel are not documented in the sources reviewed. |
| Plate handling | Echo systems are commonly integrated with a plate handler / access unit for hotel-fed operation; the standalone unit is manually loaded |
| Consumables | None per transfer — no tips, no cassettes, no wash |

Zero consumable per transfer, and zero cross-contamination path, is what makes
this class worth modelling separately from `pipette`.

## Control Stack

| Layer | Detail |
| --- | --- |
| Vendor software | Echo Plate Reformat, Echo Cherry Pick, Echo Plate Audit, and Echo Client-type applications |
| Automation | Echo systems are integrated into workcells with vendor access/handler units |
| Documented external API | Not identified in this pass |

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `beckman-echo` | `hub`, `dispenser.robot`, `transfer.acoustic` |
| `beckman-echo-transducer` | `dispense.head.acoustic`, `sensor.acoustic` |
| `beckman-echo-source-stage` | `stage.plate`, `labware.carrier` |
| `beckman-echo-destination-holder` | `labware.carrier.inverted` |

Capability requirements:

| Capability | Reason |
| --- | --- |
| `AcousticTransfer(source_well, dest_well, volume)` | Well-to-well, not aspirate-then-dispense; there is no intermediate state where liquid is "held" |
| Quantised volume validation | Volumes must be multiples of the drop size |
| `SurveyPlate` / `AuditPlate` | Returns per-well volume and fluid-property measurements — a measurement capability, not a motion one |
| Fluid-class declaration | Calibration depends on declared fluid chemistry |
| Qualified-labware constraint | Source labware must be from a vendor-qualified set; this is a hard precondition |

## Abstraction Stress Points

1. There is no aspirate/dispense pair and no pipette state. The atomic operation
   is a *transfer*, and it cannot be decomposed.
2. Liquid moves upward against gravity into an inverted plate.
3. The instrument measures chemistry, not just geometry, and that measurement is
   a deliverable in its own right.
4. Calibration is chemistry-dependent, so a "liquid class" is required input, not
   an optimisation hint.
5. Labware qualification is a hardware constraint enforced by physics (acoustic
   coupling), not a software whitelist.

## Evidence

| Evidence | Link |
| --- | --- |
| Echo acoustic technology overview: droplet ejection from source into inverted destination | <https://www.beckman.com/liquid-handlers/echo-acoustic/technology> |
| Echo 525: 2.5 nL droplets, up to 2.5 µL per source well | <https://www.beckman.com/liquid-handlers/echo-525> |
| Echo 650 Plus: next-generation titanium-lens transducer, up to 700 drops/s, 2.5 nL, any-well-to-any-well, 384/1536 source plates and 96-tube arrays | <https://www.beckman.com/liquid-handlers/echo-acoustic/echo-650-plus-series> |
| Echo 650 series | <https://www.beckman.com/liquid-handlers/echo-acoustic/echo-650-series> |
| Echo Plate Audit software: acoustic fluid analysis, tracking and comparison | <https://www.beckman.com/liquid-handlers/software/echo/plate-audit> |
| Survey mechanism (echo time-of-flight for volume, amplitude for acoustic impedance and DMSO/glycerol concentration), fluid-class calibration guidance, Echo Qualified labware requirement | <https://www.emeraldcloudlab.com/helpfiles/experimentacousticliquidhandling> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Mechanics | Number of axes, travel, plate-exchange mechanism, and whether the destination holder moves |
| Interfaces | Host connection (USB/Ethernet/serial) and whether any documented command API exists |
| Survey output | Whether per-well survey data is retrievable programmatically and in what form |
| Fluid classes | The complete calibration class list and its effect on achievable volumes |
| Integration | The command surface of the Echo access/plate-handler unit |
