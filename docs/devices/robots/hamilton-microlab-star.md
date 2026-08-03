# Hamilton Microlab STAR / STARlet / STARplus — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | Hamilton Company |
| Family | Microlab STARlet, STAR, STARplus; STAR V is the current generation |
| Robot class | Independent-channel deck liquid handler with optional multi-probe head |
| Evidence quality | Good for mechanical inventory: an OEM-redistributed reference guide (Illumina ML STAR guide) and Hamilton's own brochure specification page. Weak for control protocol. |
| numanager driver | None. Related market note: [`market-research.md`](market-research.md) |

## Platform Variants

Same mechanics, different deck length. Track count is the primary variant axis.

| Model | Deck tracks | Width without multi-probe head | Width with multi-probe head | Notes |
| --- | --- | --- | --- | --- |
| STARlet | 30 | 1124 mm | 1664 mm | Field-upgradable to STARplus with a deck extension module |
| STAR | 54 | 1387 mm | 1927 mm | Reference guide: up to 54 single-track carriers or 9 six-track carriers |
| STARplus | 71 (full deck extension) | ~2160 mm total length | — | Third-party spec listing; treat width figure as lower confidence |

Common to all: height 903 mm, depth 781 mm manual-load or 1011 mm with Autoload,
weight 135–160 kg depending on head, max labware height 140 mm above the deck,
stated x/y/z positional accuracy 0.1 mm, 100/115/230 VAC 50/60 Hz, 600 VA
(STARlet) to 1000 VA (STAR).

## Motion Structure

This is the single most important difference from an OT-2-shaped model.

| Element | Behaviour |
| --- | --- |
| Arm | One pipetting arm traverses X across the deck. Optional second arm/tool rail on larger configurations. |
| Channel Y | Each pipetting channel moves independently on Y via the Dynamic Positioning System (DPS), so channel pitch is variable, not fixed at 9 mm. |
| Channel Z | Each channel moves independently on Z. |
| Channel plunger | Each channel has its own air-displacement plunger drive. |
| Multi-probe head | CO-RE 96 or CO-RE 384 head is a separate rigid body on the arm with one shared Z and one shared plunger drive across all nozzles. |
| iSWAP | Optional dedicated plate-handling arm with wrist rotation, distinct from the channel arm. |

Net: an axis model of "gantry XYZ + per-mount plunger" is wrong here. The correct
decomposition is *arm X* → *per-channel (Y, Z, plunger)*, plus a separately
addressable head body whose nozzles are not independently positionable.

## Pipetting Hardware

| Item | Detail |
| --- | --- |
| Base configuration | 8 independent air-displacement channels working in parallel |
| Maximum channels | Up to 16 independent 1000 µL channels on one arm, or up to 8 independent 5 mL channels; mixed 1 mL + 5 mL on the same arm is offered |
| MagPip channels | Tubular-linear-drive channel variant marketed for ultra-fast pipetting down to ~350 nL; typically 8 per arm |
| Brochure volume span | 10 µL to 5000 µL quoted at platform level; per-channel usable range depends on channel type and mounted tip size, so treat the platform figure as an envelope, not a channel range |
| Pipetting principle | Air displacement with plunger, barrel, disposable tip, and an air gap between plunger and liquid |
| Tip attachment | CO-RE (Compression-induced O-Ring Expansion): tips are picked up by radial O-ring expansion, not by vertical press-fit force |
| Tip constraint | Only Hamilton CO-RE tips mount correctly; filtered tips required for biohazard work |
| Tip racks | Barcode-labelled per rack; a tip carrier holds up to 5 racks |
| CO-RE 96 head | 96 nozzles actuated simultaneously, 1000 µL class channels, every nozzle moves the same volume |
| CO-RE 384 head | 384-nozzle multi-probe head for 384-well plates |

## Sensing And Process Control

| Sensor / feature | Detail | Model consequence |
| --- | --- | --- |
| cLLD (capacitive liquid level detection) | On every independent channel. On the CO-RE 96 head, only the A1 and H12 nozzles carry cLLD sensors. | Liquid-level sensing is per-channel on the channel arm but sparse on the head. A "head has LLD" boolean is wrong. |
| pLLD (pressure-based LLD) | Offered as dual LLD together with cLLD for non-conductive liquids | Sensing mode is a selectable per-aspirate parameter |
| TADM | Total Aspiration and Dispense Monitoring: records the pressure curve inside each channel during aspirate/transport/dispense; used as an audit trail | Produces a per-command time-series result, not a scalar. Needs a trace/waveform return type. |
| MAD | Monitored Air Displacement: real-time clot / empty-well / volatile-solvent detection | Per-command status flags on top of the pressure trace |
| Tip presence | Tip pickup/ejection confirmation is part of normal operation | Per-channel `has_tip` state |
| Autoload barcode reader | Class 2 laser scanner on the loading unit; reads carrier and tube barcodes during load | Identity/occupancy is populated by a scan pass, not by static config |
| Front cover interlock | Opening the cover stops the run | First-class safety property |

## Labware Handling

| Tool | Detail |
| --- | --- |
| CO-RE gripper | Two gripper jaws parked on the waste block; **two pipetting channels pick them up** as tools. Grips plates in landscape or portrait but cannot rotate a plate. |
| iSWAP | Separate plate-handling arm; can rotate/reorient plates and reach hotels and integrated devices |
| Autoload | Motorised carrier loading unit that pulls carriers in from the loading tray, with barcode scanning |
| CO-RE paddles | Push/pull tools also picked up by channels, used for moving vessels on deck |

The CO-RE gripper is the key modelling oddity: the same physical actuators are
either pipetting channels or, after a tool-pickup step, half of a gripper. Tool
state is therefore a mode of the channel device, not a separate always-present
device.

## Deck And Labware Geometry

| Item | Detail |
| --- | --- |
| Deck primitive | Equal-width tracks that mechanically guide carriers into fixed positions |
| Carrier types | 1-track (1T) tube carriers holding 24 or 32 tubes; 6-track (6T) carriers holding 5 tip racks or 5 plate positions |
| Position numbering | Carrier positions numbered back-to-front and left-to-right; barcodes face right |
| Addressing | A labware location is (carrier type, track range, site index), not a flat SBS slot number |
| Height limit | 140 mm labware height above deck surface |

An SBS-slot-only deck model cannot express this. The deck needs a
*track/carrier/site* hierarchy where a carrier occupies a contiguous track range
and exposes N sites.

## Integrated Modules And Peripherals

| Module | Hardware facts |
| --- | --- |
| Hamilton Heater Shaker (HHS) | Heats and shakes plates on deck; max 105 °C; two temperature sensors for monitoring and control; a default threshold protects samples; plate is locked during shaking and unlocked for automatic removal |
| Plate adapters | HHS adapters are plate-type specific; only labware matching HHS dimensions is allowed |
| Others commonly integrated | Tip waste chutes/containers, wash stations, temperature-controlled carriers, magnetic bead modules, on-deck readers and thermal cyclers via iSWAP |

## Control Stack And Interfaces

| Layer | Detail |
| --- | --- |
| Host link | USB to a Windows control PC; the safety label limits cable length to 5 m |
| Vendor software | VENUS (Windows) drives methods; a Hamilton App Launcher wraps deployed methods |
| Run control primitives exposed by vendor UI | Start/Run, Pause (completes the current pipetting step first), Single Step, Abort, Control Panel for initialisation and manual mechanical moves |
| Documented API boundary | Hamilton publishes VENUS Web API material (devices, deck layout, method execution, status, notifications); access may need developer-portal credentials |
| Firmware boundary | The instrument speaks a documented-to-partners firmware command language over USB. PyLabRobot implements a STAR/STARlet backend that talks to this firmware directly over USB with PyUSB, deriving commands from VENUS-generated traffic and Hamilton reference manuals. |

Two viable integration boundaries therefore exist for a STAR: the VENUS/Web API
service layer, and the raw USB firmware layer that PyLabRobot has already shown
to be reachable without VENUS. They imply very different device models — the
first is method/run oriented, the second is channel/axis oriented.

## Device-Model Implications

| Proposed device | Kind tags | Notes |
| --- | --- | --- |
| `hamilton-star` | `hub`, `liquid_handler.robot` | Owns the transport (USB firmware session or VENUS API session) |
| `hamilton-star-arm` | `motion.arm`, `axis.x` | Arm X traverse represented as a single-axis arm device |
| `hamilton-star-channel-N` | `pipette.channel` | Per-channel Y, Z, plunger, cLLD/pLLD, TADM trace, tip state |
| `hamilton-star-head-96` / `-384` | `pipette.head` | Shared Z and plunger; sparse LLD nodes; nozzle-subset selection |
| `hamilton-star-gripper` | `labware.mover`, `tool.channel_mounted` | Present only while channels hold the CO-RE gripper jaws |
| `hamilton-star-iswap` | `labware.mover`, `motion.arm` | Independent plate arm with rotation |
| `hamilton-star-autoload` | `labware.host`, `barcode.reader` | Carrier load + barcode identity |
| `hamilton-star-deck` | `deck`, `labware.host` | Track/carrier/site geometry |
| `hamilton-star-hhs-*` | `module.heater_shaker` | Temperature, speed, latch state |

Capabilities this platform demands that the OT-2 model does not:

| Capability | Why |
| --- | --- |
| `PipetteChannelActuate` with per-channel Y | Variable channel pitch is a first-class motion parameter |
| `PipetteHeadActuate` with nozzle-subset mask | 96/384 head sub-selection (row/column/quadrant) |
| Pressure-trace readback | TADM returns a curve per aspirate/dispense |
| `ToolPickup` / `ToolDrop` | Channels acquire grippers and paddles as physical tools |
| `LabwareMove` on two different movers | CO-RE gripper (no rotation) vs iSWAP (rotation) have different reachability and constraints |
| Carrier-level `BarcodeScan` | Deck identity comes from an autoload scan pass |

## Abstraction Stress Points

1. Channel pitch is variable, so a pipette is not defined by a fixed nozzle
   geometry.
2. A "gripper" may be an operating mode of two pipette channels.
3. Sensors are unevenly populated across nozzles of the same head.
4. The deck is a track/carrier space, not a slot grid.
5. Aspiration produces telemetry (pressure curve) that must survive as a result
   payload, not just a status code.

## Evidence

| Evidence | Link |
| --- | --- |
| ML STAR reference guide: 8 channels, DPS independent Y/Z, 54 1T or 9 6T carriers, cLLD per channel and 96-head A1/H12 only, CO-RE gripper picked up by 2 channels, HHS 105 °C with 2 sensors, USB ≤5 m, cover interlock, carrier/barcode conventions | <https://emea.support.illumina.com/content/dam/illumina-support/documents/documentation/system_documentation/mlstar/hamilton-ml-star-reference-guide-15070074-a.pdf> |
| Hamilton Microlab STAR brochure specification page: dimensions, weight, tracks, 140 mm labware height, 0.1 mm accuracy, power | <https://info.hamiltoncompany.com/view/449349329/36-37/> |
| Hamilton Microlab STAR product page: channel counts, CO-RE 96/384 heads, iSWAP, autoload | <https://www.hamiltoncompany.com/microlab-star> |
| STARlet/STAR/STARplus track counts and upgrade path | <https://www.bostonind.com/hamilton-microlab-star-line-configuration-guide-starlet-star-starplus> |
| TADM / MAD / dual LLD / MagPip descriptions | <https://info.hamiltoncompany.com/view/449349329/12-13/> |
| PyLabRobot STAR backend: direct USB firmware control, commands derived from VENUS traffic and Hamilton manuals | <https://github.com/PyLabRobot/pylabrobot> |
| PyLabRobot paper (hardware-agnostic liquid handling interface) | <https://www.cell.com/device/fulltext/S2666-9986(23)00170-9> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Track pitch | Exact deck track pitch in mm and the deck origin convention are not pinned by the sources above |
| Channel volume ranges | Per-channel-type minimum/maximum volumes (1 mL vs 5 mL vs MagPip) need a Hamilton datasheet |
| Firmware grammar | Command framing, addressing of individual channels, and error vocabulary need PyLabRobot source review or captured traffic |
| VENUS Web API | Whether the published API exposes channel-level actuation or only method/run-level control |
| STAR V | What changed mechanically in the current-generation STAR V versus the STAR documented here |
