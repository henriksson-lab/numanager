# Thermo Scientific Versette — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | Thermo Fisher Scientific |
| Family | Versette Automated Liquid Handler |
| Robot class | Compact fixed-head-per-run 96/384-channel pipettor over a small moving stage |
| Evidence quality | Moderate. Thermo brochure read directly; deeper mechanical/interface detail not located. |

## Architecture

The Versette is a head-plus-stage machine, not a gantry:

| Element | Detail |
| --- | --- |
| Pipetting head | Interchangeable 96- or 384-channel head; user-changeable **without tools**, guided step by step by ControlMate software |
| Head identification | Every pipetting head carries an **RFID tag for self-identification** |
| Stage | Six-position stage with a dual-level structure to keep the footprint small |
| Volume envelope | 0.5 – 300 µL total across the head range |
| Consumables | Thermo D.A.R.T.s (Disposable Automation Research Tips) with a surface-seal design intended to seal evenly across all channels |
| Local UI | On-board graphical display for simple pipetting procedures |
| Host software | ControlMate for complex protocols and advanced editing |

Typical uses given by Thermo: 96/384 plate replication, plate stamping, serial
dilution, and high-throughput mass spectrometric immunoassay.

## RFID Head Identification

This is the single most directly reusable idea on this instrument. The head
announces its own identity to the controller, so the software does not depend on
a configuration file to know the channel count and volume class of the mounted
head.

For numanager that argues for a general pattern: **a mountable tool may be
self-identifying, and the driver should prefer measured identity over configured
identity when both exist.** The Hamilton Prep camera (labware identity), Tecan
DeckCheck (deck identity) and Versette RFID (tool identity) are three instances
of the same principle at different levels.

## Dual-Level Stage

Six positions arranged on two levels is a genuinely different deck topology from
a flat SBS grid: position addressing must carry a level as well as a planar
coordinate, and reachability may differ between levels.

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `thermo-versette` | `hub`, `liquid_handler.robot` |
| `thermo-versette-head` | `pipette.head`, `tool.self_identifying` |
| `thermo-versette-stage` | `deck`, `labware.host`, `stage.multi_level` |

Capability requirements:

| Capability | Reason |
| --- | --- |
| `HeadIdentity` readback | RFID-reported head model, channel count, volume class |
| `PipetteHeadActuate` | 96 or 384 nozzles actuated together |
| Multi-level deck addressing | Six positions on two levels |
| Tool-change workflow | Head change is a guided, user-performed procedure with software state |

## Abstraction Stress Points

1. Mounted hardware can identify itself electronically, so device inventory can
   be discovered rather than configured.
2. Deck positions are not coplanar.
3. There are no independent channels and no gripper — another "head-only"
   machine, like the Agilent Bravo.

## Evidence

| Evidence | Link |
| --- | --- |
| Thermo Versette brochure: 96/384-channel heads, 0.5–300 µL, RFID self-identification, D.A.R.T.s tips, six-position dual-level stage, on-board display, ControlMate | <https://documents.thermofisher.com/TFS-Assets/LCD/brochures/HAL_LH_Special_ENG_Nordic_2018_LR_Versette.pdf> |
| Versette brochure (alternate host) | <https://static.fishersci.eu/content/dam/fishersci/en_US/documents/programs/scientific/brochures-and-catalogs/brochures/thermo-scientific-verstte-brochure.pdf> |
| Versette FAQ / product page | <https://www.thermofisher.com/order/catalog/product/650-INSTR/faqs> |
| Versette 384-channel head 1–100 µL FAQ | <https://www.thermofisher.com/order/catalog/product/650-06-384100/faqs> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Head catalogue | Full list of heads with channel counts and per-head volume ranges |
| Motion | Which axes exist, stage travel, and whether the head has independent Z |
| Interfaces | Host connection type and whether ControlMate exposes any API |
| Sensors | Whether any liquid-level or pressure sensing exists |
| Accessories | Whether the stage supports active modules (shaking, thermal) at any position |

## Related

The Thermo **Multidrop** family (Combi, Combi nL, Micro) is a separate dispenser
class — peristaltic/tube-cassette bulk reagent dispensers with no aspiration and
no tips. They should get their own note before being modelled; they are closer to
the FORMULATRIX and SPT dispensers than to the Versette.
