# Tecan Freedom EVO 100 / 150 / 200 — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | Tecan |
| Family | Freedom EVO 75 / 100 / 150 / 200 (worktable width in cm) |
| Robot class | Multi-arm deck liquid handler; the long-running predecessor to the Fluent |
| Market status | End of sale 31 December 2025; very large installed base remains |
| Evidence quality | High. Manufacturer specification sheet (Tecan doc 399174 V1.0) read directly. |

## Platform Variants

| Model | Width | Height | Depth | Weight (base unit) | Arms | Power |
| --- | --- | --- | --- | --- | --- | --- |
| Freedom EVO 100 | 1075 mm | 870 mm | 780 mm | 110 kg | 1–2 | 600 VA |
| Freedom EVO 150 | 1450 mm | 870 mm | 780 mm | 130 kg | 1–3 | 1200 VA |
| Freedom EVO 200 | 2050 mm | 870 mm | 780 mm | 182 kg | 1–3 | 1200 VA |

A 75 cm worktable variant (881 mm) also exists. Dual Liquid LiHa, or one Liquid
LiHa plus one Air LiHa, is possible on the 150/200.

Worktable addressing is by **grid** positions: roughly 30 grids on the EVO 100
and 69 grids on the EVO 200 per third-party configuration guides. Grid pitch is
not stated in the specification sheet reviewed.

## The Seven Arm Types

Tecan documents exactly seven interchangeable arms. This enumeration is the most
useful single artefact in this survey for arm abstraction.

| # | Arm | Purpose | Key hardware facts |
| --- | --- | --- | --- |
| 1 | RoMa (Robotic Manipulator) | Transport labware or disposable tips | Eccentric, eccentric-long, or centric fingers; max 400 g payload; gripper range 58–140 mm |
| 2 | RoMa long Z | Same as RoMa | Adds 350 mm access below the worktable |
| 3 | PnP (Pick and Place) | Transport tubes and cylindrical containers | Max 100 g; tube diameter 11–18 mm; **360° unlimited rotation** |
| 4 | Liquid LiHa | Liquid-displacement pipetting | 2, 4, or 8 channels; independent Z; 4-/8-tip arm auto Y spacing 9–38 mm; **2-tip arm variable spacing 9–418 mm**; volume 0.5–5000 µL |
| 5 | Air LiHa | Air-displacement pipetting | 4 or 8 channels; independent Z; auto Y spacing 9–38 mm; volume 0.5–1000 µL; non-contact dispense down to 0.5 µL |
| 6 | MCA 96 | 96-channel head | Volume 1–200 µL; washable fixed-tip blocks or disposable tips **interchangeable during a run**; row-, column- and quadrant-wise pipetting |
| 7 | MCA 384 | 384-channel head | Volume 0.5–125 µL in 384 format, 1–500 µL in 96 format; automatically interchangeable 384/96 head adapters; row/column/quadrant pipetting |

Movement precision: LiHa / Air LiHa ±0.4 mm on X, Y, Z. RoMa and PnP ±0.4 mm X,
±0.5 mm Y, ±0.3 mm Z. MCA 96 / MCA 384 ±0.5 mm on X, Y, Z.

Different arms have different precision on the same instrument — a single
"robot accuracy" number is not modellable.

## Liquid LiHa Fluidics

| Item | Detail |
| --- | --- |
| Syringes | 50, 250, 500, 1000, 2500, 5000 µL, mounted on the Tecan XP Smart dilutor |
| Fixed tips | Washable: standard PTFE-coated stainless steel, ceramic coated, hard PTFE with full DMSO compatibility, short/long low volume; Te-PS tips for 1536-well plates |
| DiTi sizes | 10, 50, 200, 1000, 5000 µL with/without filters; 350 µL nested without filter |
| Fast Wash | Diaphragm-pump system-liquid delivery |
| Liquid waste vigilance | Active monitoring of liquid levels in system and waste containers |

The syringe/dilutor is a separately identifiable fluidic component with its own
size. This is the "system liquid" architecture: a plumbed pump behind fixed or
disposable tips, absent entirely on air-displacement platforms.

## Sensing

| Feature | Detail |
| --- | --- |
| LLD | Capacitive (conductive liquids) or pressure-based (non-conductive); works down to 50 µL in a round-bottom 96-well plate on standard carriers with cLLD |
| ILID | Integrated liquid detection, which includes tip-occlusion detection |
| PMP | Pressure Monitored Pipetting: real-time quality control of the transfer; detects clots and air aspiration |
| DiTi sensing | Confirmation of tip pickup and ejection |
| PosID | Fully automated barcode scanner for tubes, plates, reagents and carriers |
| Safety screens | User-activated interlocked screens prevent access to the work area or an unintentional system halt |
| Access control | Three password levels: operator, application specialist, administrator |

## Control Stack And Interfaces

| Layer | Detail |
| --- | --- |
| Vendor software | Freedom EVOware; EVOware Plus adds process scheduling |
| Host OS | Windows 7 / 64-bit era stack |
| Instrument link | 1 USB **or** RS-232 for instrument control; 1 USB for the software hardlock |
| Third-party control | The EVO is the most publicly reimplemented Tecan platform; open projects (e.g. PyLabRobot's EVO backend) target the firmware layer directly |

RS-232 as an alternative instrument link is significant: this platform predates
USB-only control and its firmware protocol is line-oriented.

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `tecan-evo` | `hub`, `liquid_handler.robot` |
| `tecan-evo-liha` / `-air-liha` | `motion.arm`, `pipette.arm` |
| `tecan-evo-liha-channel-N` | `pipette.channel` |
| `tecan-evo-dilutor-N` | `pump.syringe`, `fluidics.system_liquid` |
| `tecan-evo-mca96` / `-mca384` | `pipette.head` |
| `tecan-evo-roma` | `labware.mover`, `motion.arm` |
| `tecan-evo-pnp` | `labware.mover`, `tube.handler` (rotation-capable) |
| `tecan-evo-posid` | `barcode.reader` |
| `tecan-evo-deck` | `deck`, `labware.host` (grid-addressed) |

New capability requirements versus a simple pipette model:

| Capability | Reason |
| --- | --- |
| `SyringePumpControl` | System-liquid dilutors are addressable hardware |
| `WashStation` / `FastWash` | Fixed-tip platforms need wash as a first-class operation |
| Tube-handling move with rotation | PnP rotates 360°, RoMa does not |
| Run-time tip-block exchange | MCA 96 swaps fixed-tip blocks and disposables mid-run |
| Per-arm precision metadata | Accuracy differs per arm type |

## Abstraction Stress Points

1. Two pipetting paradigms (liquid displacement with system liquid vs air
   displacement) can coexist on one robot.
2. Tip spacing range differs by arm variant — a 2-tip LiHa spans 9–418 mm, far
   beyond plate pitch.
3. Labware movers are plural and non-equivalent (RoMa vs PnP: payload, geometry,
   rotation).
4. Consumable state includes fixed-tip wash state, not just tip presence.

## Evidence

| Evidence | Link |
| --- | --- |
| Freedom EVO specification sheet 399174 V1.0: dimensions, weights, arm counts, all seven arm types, volume ranges, syringes, tips, LLD/ILID/PMP, PosID, precision, PC/interface requirements | <https://www.richmondscientific.com/wp-content/uploads/2023/06/Product-specifications-Tecan-Freedom-EVO-2-150-Base-2111000042.pdf> |
| Freedom EVO platform page and end-of-sale note | <https://lifesciences.tecan.com/freedom-evo-platform> |
| Worktable grid counts (100 = 30 grids, 200 = 69 grids) — third-party configuration guide | <https://www.bostonind.com/tecan-freedom-evo-configuration-guide-100-150-200> |
| Freedom EVO brochure | <https://www.tecan.com/doc/freedom-evo-brochure-pdf-392956> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Grid geometry | Grid pitch in mm, origin, and how carriers map onto grids |
| Firmware protocol | Command framing over RS-232/USB, addressing of arms and channels, error vocabulary |
| EVOware API | Whether EVOware exposes a documented external control interface comparable to FluentControl's |
| Dilutor addressing | How many dilutors exist per channel and whether they are individually commandable |
| Wash station | Physical wash-station variants and their control primitives |
