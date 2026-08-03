# Liquid Handling Robots — Per-Robot Hardware Notes

These pages record **what hardware each robot actually has**, so that the
numanager device model can be designed from the real axes of variation rather
than from one reference instrument. They are hardware inventories, not protocol
evidence: nothing here authorises driver behaviour. Protocol claims still need
manufacturer API documentation, public standards, open source, captured traffic,
or bench validation, per [`../evidence.md`](../evidence.md).

Market/vendor context lives in [`market-research.md`](market-research.md).
The only robot in this class with a numanager driver page today is the
[Opentrons OT-2](../opentrons-ot2.md).

## Index

| Robot | Vendor | Class | Evidence quality |
| --- | --- | --- | --- |
| [Microlab STAR / STARlet / STARplus](hamilton-microlab-star.md) | Hamilton | Independent-channel deck handler | Good |
| [Microlab VANTAGE](hamilton-vantage.md) | Hamilton | Multi-arm enclosed deck handler | Moderate |
| [Microlab NIMBUS](hamilton-nimbus.md) | Hamilton | Compact channel-or-head handler | Moderate |
| [Microlab Prep](hamilton-microlab-prep.md) | Hamilton | Compact benchtop with deck camera | Good |
| [Fluent 480/780/1080](tecan-fluent.md) | Tecan | Multi-arm deck handler | High |
| [Freedom EVO 100/150/200](tecan-freedom-evo.md) | Tecan | Multi-arm deck handler (legacy, huge base) | High |
| [Veya](tecan-veya.md) | Tecan | Deck handler | **Low — insufficient** |
| [D300e Digital Dispenser](tecan-d300e.md) | Tecan | Inkjet dispenser | High |
| [Bravo / AssayMAP Bravo](agilent-bravo.md) | Agilent | Head-only fixed-deck handler | High |
| [BenchCel / MiniHub workcell](agilent-benchcel-workcell.md) | Agilent | Plate handling and storage | Moderate–good |
| [Biomek i5 / i7](beckman-biomek-i-series.md) | Beckman | Pod-based deck handler with ALPs | Moderate |
| [Echo 525 / 650](beckman-echo-acoustic.md) | Beckman | Acoustic droplet ejection | Good |
| [Versette](thermo-versette.md) | Thermo Fisher | Head-only with RFID head ID | Moderate |
| [Multidrop Combi](thermo-multidrop-combi.md) | Thermo Fisher | Peristaltic bulk dispenser | Moderate–good |
| [epMotion 5070/5073/5075](eppendorf-epmotion.md) | Eppendorf | Tool-changing benchtop handler | High (5070) |
| [JANUS G3](revvity-janus-g3.md) | Revvity | Varispan arm + MDT head | Moderate |
| [Flex](opentrons-flex.md) | Opentrons | Open-deck robot, fully documented | High |
| [ASSIST PLUS](integra-assist-plus.md) | INTEGRA | Handheld-pipette positioner | Moderate–good |
| [MINI 96](integra-mini-96.md) | INTEGRA | Standalone 96-channel head | Good |
| [firefly / firefly+](spt-firefly.md) | SPT Labtech | Integrated genomics workstation | High |
| [mosquito](spt-mosquito.md) | SPT Labtech | Positive-displacement low-volume | Moderate–good |
| [dragonfly discovery](spt-dragonfly.md) | SPT Labtech | Multi-reagent syringe dispenser | Moderate–good |
| [MANTIS](formulatrix-mantis.md) | FORMULATRIX | Microfluidic chip dispenser | Good |
| [TEMPEST](formulatrix-tempest.md) | FORMULATRIX | Modular chip bulk dispenser | Good |
| [PIPETMAX 268](gilson-pipetmax.md) | Gilson | Head-cassette benchtop handler | Moderate–good |
| [GX-271 / GX-281](gilson-gx-liquid-handlers.md) | Gilson | Flow-path autosampler/handler | Moderate–good |

## The Axes Of Variation

The point of reading 26 machines is to find the dimensions along which they
actually differ. These are the ones that survived.

### 1. What carries the pipetting hardware

| Pattern | Examples |
| --- | --- |
| Fixed mounts on a gantry | Opentrons OT-2 (2 mounts), Flex (2 pipette mounts + 1 extension mount) |
| Typed, pluralisable arms | Tecan Fluent (1–3 arms), Tecan EVO (7 arm types), Beckman "pods", Revvity Varispan + MDT |
| One arm carrying heterogeneous tools | Hamilton STAR/VANTAGE (channels + head + gripper + camera on one arm) |
| A head mount with no channels at all | Agilent Bravo, Thermo Versette |
| A tool changer with an empty holder | Eppendorf epMotion |
| A holder for a separate handheld instrument | INTEGRA ASSIST PLUS |
| No robot at all | INTEGRA MINI 96 |

**Consequence:** `mount` (a position) and `instrument` (what occupies it) must be
separate concepts, discovered rather than configured, and an instrument may
occupy more than one mount (Flex 96-channel).

### 2. Pipetting physics

| Principle | Examples | What it changes |
| --- | --- | --- |
| Air displacement | Hamilton, Opentrons, Eppendorf, Tecan Air LiHa/FCA-air, Gilson PIPETMAX | Air gap, blow-out, liquid class matters |
| Liquid displacement with system fluid | Tecan Liquid LiHa/FCA-liquid, Gilson GX | Syringe/dilutor devices, wash stations, fixed tips |
| Positive displacement (piston in consumable) | SPT mosquito, SPT dragonfly | Liquid-class agnostic, consumable owns the volume envelope |
| Acoustic ejection | Beckman Echo | No aspiration at all; quantised volume; measures chemistry |
| Thermal inkjet | Tecan D300e | Picolitre quanta; per-nozzle fluid binding; consumable life |
| Pneumatic microdiaphragm | FORMULATRIX MANTIS, TEMPEST | Chip-defined volume quanta; needs pressure/vacuum services |
| Peristaltic tubing | Thermo Multidrop | Continuous source; prime/purge/wash are protocol steps |

**Consequence:** a single `PipetteActuate` shaped like "move a plunger" is wrong.
The model needs a *metering mechanism* discriminator, and `liquid_class` cannot
be a mandatory parameter of a generic transfer.

### 3. Channel geometry

| Pattern | Examples |
| --- | --- |
| Fixed pitch, fixed count | Opentrons 8-channel, Thermo Versette heads |
| Variable pitch, independent Y and Z per channel | Hamilton DPS, Tecan LiHa/FCA (9–38 mm; 2-tip 9–418 mm), Beckman Span-8, Revvity Varispan, INTEGRA VOYAGER |
| Rigid head, shared Z and plunger | Hamilton CO-RE 96/384, Tecan MCA, Agilent Bravo heads, Beckman MC pods |
| Head with selectable nozzle subsets | Agilent Series III (column/row/well), SPT firefly (384/96/16/8), Gilson (1–8 of 8), Tecan MCA (row/column/quadrant) |
| Head that changes format at run time | Tecan MCA 384/96 adapter exchange |

**Consequence:** channel count is a *mode*, pitch is a *commanded parameter*, and
nozzle-subset masks are a baseline requirement, not an advanced feature.

### 4. Deck topology

| Pattern | Examples |
| --- | --- |
| Flat SBS slot list | Hamilton NIMBUS/Prep, Agilent Bravo (9), Revvity JANUS Mini (12) |
| Named grid plus staging region | Opentrons Flex (A1–D3 + column 4) |
| Track and carrier space | Hamilton STAR (30/54/71 tracks; 1T and 6T carriers) |
| Dual addressing: tracks **and** SLAS positions | Hamilton VANTAGE |
| Grid worktable | Tecan EVO (~30–69 grids), Fluent |
| Multi-level deck | Thermo Versette (6 positions on two levels), SPT firefly (top/bottom/basement with 49.7 mm and 93 mm height limits) |
| **Moving** deck | SPT firefly (two movable decks) |
| Typed positions | INTEGRA ASSIST PLUS (3 work + 2 tip), Beckman ALPs (a slot may be a device) |
| One plate position | Thermo Multidrop, Tecan D300e |
| Racks of tubes, vendor codes | Gilson GX (Code 20 / 200 / 34X) |

**Consequence:** the deck must be a pluggable geometry strategy with typed
positions, optional height limits per level, and support for positions that are
themselves devices. It must also be *optional* — some dispensers have no deck.

### 5. Sensing

| Sensor | Examples | Result shape |
| --- | --- | --- |
| Capacitive LLD | Hamilton (per channel; only A1/H12 on the 96 head), Tecan, Beckman Span-8 | Boolean/height |
| Pressure LLD and monitoring | Hamilton pLLD/TADM/MAD, Tecan PMP/ILID | **Pressure curve per command** |
| Optical level/tip/labware sensing | Eppendorf epMotion | Level, tip count, labware identity |
| Deck vision | Tecan DeckCheck (1–3 cameras), Hamilton Prep camera, Flex camera | Expected-vs-actual discrepancy report |
| Tool self-identification | Thermo Versette RFID heads | Identity readback |
| Barcode | Hamilton Autoload, Tecan PosID/Fluent ID, workcell labelers | Identity per carrier/plate/tube |
| Acoustic survey | Beckman Echo | Per-well volume **and fluid composition** |

**Consequence:** measurement results are not all scalars. The model needs
waveform/trace results, per-well measurement arrays, and a first-class
"observed state disagrees with declared state" outcome.

### 6. Labware movement

| Pattern | Examples |
| --- | --- |
| Gripper is a tool picked up by pipetting channels | Hamilton CO-RE gripper (2 channels), Tecan FCA gripper |
| Gripper on its own mount | Opentrons Flex extension mount |
| Gripper integrated in a head | SPT firefly |
| Dedicated plate arm with rotation | Hamilton iSWAP, Tecan RoMa, Tecan PnP (360°) |
| Gripper geometry set by swappable fingers | Tecan RGA (eccentric / long-eccentric / centric / tube) |
| Gripper doubles as a mechanism actuator | Agilent AssayMAP (actuates the head stripper plate) |
| Separate transport robot | Agilent BenchCel, Direct Drive Robot |
| Storage devices | BenchCel racks (ordered stacks, 60 plates each), Labware MiniHub (random access, cassette+shelf, 64 plates) |

**Consequence:** `labware.mover` must carry payload, reach, rotation capability,
and an installed-finger geometry; and labware identity must be tracked across
devices, not per deck.

### 7. Where the integration boundary is

| Boundary | Examples |
| --- | --- |
| Documented HTTP API + open source | Opentrons OT-2, Flex |
| Vendor API over a service | Hamilton VENUS Web API, Tecan FluentControl (+ SiLA2 connector), Agilent VWorks API/ActiveX |
| Vendor software only, no public API | Beckman, Eppendorf, Revvity, Thermo, INTEGRA, SPT, FORMULATRIX (per this pass) |
| Reachable firmware | Hamilton STAR/STARlet over USB (PyLabRobot), Tecan EVO, Agilent Bravo RS-232/Ethernet (protocol undocumented) |
| Physically blocked | Eppendorf epMotion — its only Ethernet port is consumed by its own control panel |

**Consequence:** the same abstract capability will be implemented at very
different altitudes per vendor. The device model has to work when the driver can
only submit method-level steps, not just when it can drive actuators.

## Revised Kind-Tag Proposal

Refines the table in the market research note using what the hardware showed.

| Kind tag | Meaning | Justified by |
| --- | --- | --- |
| `liquid_handler.robot` | Physical liquid-handling platform | all |
| `dispenser.robot` | Dispensing platform with no pipette semantics | D300e, Echo, dragonfly, MANTIS, TEMPEST, Multidrop |
| `mount` | An addressable position that may hold an instrument | Flex, epMotion, ASSIST PLUS, Bravo |
| `motion.arm` | A movable carrier of tools; pluralisable | Fluent, EVO, Biomek pods, VANTAGE |
| `pipette.channel` | Independently positioned channel with its own plunger | STAR, LiHa, Span-8, Varispan |
| `pipette.head` | Rigid multi-nozzle head, shared drive, subset-addressable | CO-RE 96/384, MCA, Bravo heads, firefly, MINI 96 |
| `dispense.head` | Metered dispense channel with a source binding | D300e, dragonfly, MANTIS, TEMPEST |
| `pump.syringe` / `pump.peristaltic` | Explicit fluidic pump | EVO dilutors, Multidrop, Gilson |
| `valve.*` | Injection / diverter / cluster valves | Gilson GX, MANTIS |
| `deck` | Coordinate and occupancy space; **optional** | most |
| `deck.position` | Typed position that may itself be a device | Beckman ALPs, ASSIST PLUS tip positions |
| `labware.host` | Owns labware definitions, offsets, identity | all deck-bearing robots |
| `labware.mover` | Gripper or plate mover, with geometry and payload | iSWAP, RoMa, RGA, Flex gripper, BenchCel |
| `labware.store` | Stack or hotel with occupancy | BenchCel racks, MiniHub, TEMPEST stacker |
| `barcode.reader` | Identity scanning | Autoload, PosID, Fluent ID |
| `camera.inspection` | Deck/labware verification | DeckCheck, Prep camera, Flex camera |
| `consumable.*` | Cassette, chip, syringe, tip rack with lifecycle state | D300e, MANTIS, TEMPEST, dragonfly |
| `module.temperature` / `.thermocycler` / `.magnetic` / `.heater_shaker` / `.shaker` / `.vacuum` | On-deck modules | firefly, epMotion, Opentrons, Hamilton HHS |
| `safety.interlock` | Door, cover, light curtain, pendant circuit | Bravo pendant, STAR cover, Fluent doors, epMotion hood |
| `liquid_handler` | Later meta-device composing the above | — |

## Capability Requirements Implied

| Capability | Notes |
| --- | --- |
| `PipetteChannelActuate` | Per-channel aspirate/dispense/blow-out, plus commanded Y pitch where supported |
| `PipetteHeadActuate` | With a nozzle mask (full / row / column / quadrant / single) and a head format mode |
| `DispenseHead` | Parameterised by metering mechanism, volume quantum, and source binding |
| `ToolPickup` / `ToolDrop` | Channels acquire grippers and paddles; epMotion acquires whole pipettes |
| `LabwareMove` | Between devices, not only within a deck; mover-specific reach and payload |
| `LabwareStore` / `LabwareRetrieve` | Ordered stacks vs addressed hotels |
| `BarcodeScan` | Carrier/plate/tube identity as a scan pass |
| `DeckVerify` | Compare observed deck to declared deck; returns discrepancies |
| `SurveyWells` | Per-well measurement array (Echo volume/composition; epMotion levels) |
| `PressureTraceReadback` | TADM/PMP curves as command results |
| `PumpFlow`, `ValveSwitch`, `Prime`, `Purge`, `Wash` | Fluid-path management for system-liquid and bulk dispensers |
| `MagneticControl`, `HeaterShakerControl`, `ThermocyclerControl` | Module actuation |
| Safety state | Interlock, door, pendant, light curtain, emergency stop |
| Per-command recovery verbs | Agilent's Abort / Retry / Ignore is a good model |

## Design Conclusions

1. **Discover topology; do not assume it.** Arm count, pod type, mounted head,
   installed chips and fitted fingers all change what a robot can do. Device
   inventory has to be a runtime property.
2. **Separate position from occupant.** Mounts, deck positions and tool holders
   are addressable things that may be empty.
3. **Make the pipetting mechanism explicit.** Air / liquid / positive
   displacement / acoustic / inkjet / diaphragm / peristaltic change which
   parameters are meaningful and which are nonsense.
4. **Treat dispensers as a peer class of pipettes, not a subtype.** Six
   dispensers in this survey have no aspirate step at all.
5. **Deck geometry is a strategy, not a table.** Slots, named grids, tracks and
   carriers, multi-level and moving decks all occur; some robots have no deck.
6. **Consumables have lifecycle state.** Tips, cassettes, chips, syringes and
   O-rings have counts, wear thresholds and used/locked flags that belong on the
   device.
7. **Sensing produces data, not just booleans.** Plan for traces, per-well
   arrays, images and discrepancy reports as command results.
8. **The liquid-handling meta-device stays above all of this.** Tip pickup,
   transfer and mix need deck geometry, calibration, collision handling and
   recovery; the raw device layer should expose typed actuators and readbacks.

## Remaining Work

| Area | Gap |
| --- | --- |
| Tecan Veya | No specification sheet found; no driver page yet |
| Deck geometry | Track pitch, grid pitch and coordinate origins are missing for Hamilton, Tecan and Beckman |
| Protocols | Only Opentrons has a documented public control API; everything else needs vendor documentation, SDK terms, or captured traffic before driver work |
| PyLabRobot | Its resource/backend model already solves several of these problems for STAR, EVO and Opentrons and should be reviewed as prior art |
| Missing robots | Hamilton STAR V, Eppendorf epMotion 96/96 Flex, Beckman Echo access units, SPT apricot, FORMULATRIX F.A.S.T. and FLO i8 PD, Agilent Direct Drive Robot |
