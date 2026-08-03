# Agilent BenchCel / Labware MiniHub / Workcell Devices — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | Agilent Technologies (Automation Solutions) |
| Family | BenchCel Microplate Handler (R-Series, X-Series), Labware MiniHub, Direct Drive Robot, and the peripherals they feed |
| Robot class | Plate/sample handling workcell — **no liquid handling at all** |
| Evidence quality | Moderate–good. Agilent online help for the MiniHub read directly; BenchCel facts from Agilent product pages and quick-guide summaries. |

This page exists because the workcell is where numanager's device model will
break first if `liquid_handler.robot` is treated as the only robot kind. These
machines move, store, seal, label and identify labware and never touch liquid.

## BenchCel Microplate Handler

| Item | Detail |
| --- | --- |
| Configurations | 2, 4, or 6 racks (R-Series), with selectable rack capacity and style |
| Rack capacity | Up to 60 standard SBS microplates per rack |
| Robot | High-speed robot that accesses the integrated stacks **and** peripheral instruments around it |
| Gripper | Paired grippers on the interior bottom of each tab; the pair holds a microplate through loading, unloading, downstacking and upstacking |
| Core operations | Downstack (take a plate from a rack and deliver it), upstack (return a plate to a rack), load/unload |
| Labware support | Labware database covering standard microplates, filter plates, deep-well plates, tip boxes and tube racks |
| Typical integration | Sits behind a Bravo, sealer, reader, or washer and feeds them plates |

Rack occupancy is the state that matters: a stack is an ordered, LIFO-ish store
with a count, not a set of addressable slots.

## Labware MiniHub

| Item | Detail |
| --- | --- |
| Function | Rotating, random-access microplate storage for ANSI/SBS 1-2004 labware |
| Capacity | Up to 64 standard SBS microplates |
| Structure | Numbered cassettes (cassette 1 identified by a "1" on the cassette cap), each with numbered shelves (shelf 1 = bottom-most) |
| Motion | Rotation to present a cassette to the robot; a single rotational axis |
| Shelf spacing | User-configurable via stackable spacers, so different labware heights can coexist in one carousel |
| Home position | Defined as the rotation at which the automation robot can access any slot in cassette 1; `Position = 0.0` in the Controls tab |
| Teaching | Motor can be disabled so the unit is rotated by hand during teaching, then "Teach Home" saves the reference into a profile |
| Integration | Works with the BenchCel robot or the Direct Drive Robot, under VWorks or third-party scheduling software |

Unlike the BenchCel stack, the MiniHub is **random access**: an address is
(cassette, shelf). Two structurally different storage models therefore exist in
one vendor's own product line.

## Other Workcell Members

| Device | Role in the cell |
| --- | --- |
| Direct Drive Robot (DDR) | Free-standing plate-transport robot serving several instruments |
| PlateLoc-class sealer | Heat sealing of plates |
| Microplate labeler | Applies and reads barcode labels |
| Bravo | Liquid handling ([`agilent-bravo.md`](agilent-bravo.md)) |
| Readers, washers, incubators | Third-party or Agilent devices scheduled into the same cell |

## Control Stack

| Layer | Detail |
| --- | --- |
| Vendor software | VWorks Automation Control, which both drives devices and schedules the cell |
| Teaching model | Per-device profiles storing communication settings, teachpoints, and location configuration |
| Third-party control | Agilent documents controlling VWorks in the background via its API, and notes that device ActiveX controls may give tighter timing for supported devices |
| Scheduling | Cell-level scheduling is a VWorks concern, above individual device control |

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `agilent-benchcel` | `hub`, `labware.mover`, `labware.store` |
| `agilent-benchcel-rack-N` | `labware.stack` — ordered store with capacity and count |
| `agilent-minihub` | `labware.store.random_access`, `motion.rotary` |
| `agilent-minihub-cassette-N` | `labware.hotel` — addressable (cassette, shelf) |
| `agilent-ddr` | `labware.mover`, `motion.robot` |
| `agilent-sealer`, `agilent-labeler` | `module.sealer`, `module.labeler`, `barcode.printer` |

Capability requirements that do not exist in a pipetting-only model:

| Capability | Reason |
| --- | --- |
| `LabwareMove(from, to)` between *devices* | Source and destination belong to different devices, not to one deck |
| `LabwareStore` / `LabwareRetrieve` | Stack and hotel semantics differ (LIFO count vs addressed slot) |
| Storage occupancy readback | Count and per-slot occupancy are the primary state |
| Shelf-geometry configuration | Spacer-defined shelf pitch changes what labware fits |
| Cell-level reservation/scheduling | Two robots can contend for the same instrument nest |

## Abstraction Stress Points

1. A robot in this class has no pipette, no tips, and no liquid state.
2. Labware identity and location must be trackable *across* devices, which forces
   a system-level labware registry rather than per-deck occupancy.
3. Two storage topologies (ordered stack vs random-access carousel) need
   different addressing but the same move capability.
4. Teaching/calibration state lives in vendor profiles and is a precondition for
   any move being safe.
5. Scheduling and arbitration become real problems once more than one mover
   exists.

## Evidence

| Evidence | Link |
| --- | --- |
| Labware MiniHub online help: cassettes, shelf numbering, rotation, home position, motor disable, Teach Home, profiles | <https://automation.help.agilent.com/AutomationSolutionsKB14/Labware%20MiniHub%20User%20Guide/Config.05.7.html> |
| Labware MiniHub product listing: 64 SBS plates, configurable shelf spacing, integration with BenchCel/DDR | <https://www.directindustry.com/prod/agilent-technologies-life-sciences-chemical/product-32598-2012609.html> |
| Labware MiniHub user guide | <https://www.agilent.com/cs/library/usermanuals/public/G5584-90000A_MiniHub_UG_EN.pdf> |
| BenchCel product page: modular racks, high-speed robot, integration | <https://www.agilent.com/en/product/automated-liquid-handling/automated-microplate-management/benchcel-microplate-handler> |
| BenchCel R-Series quick guide: 2/4/6 racks, up to 60 plates per rack, gripper tabs, downstack/upstack | <https://www.agilent.com/Library/usermanuals/Public/G5400-90003A_BenchCelQG_S_EN.pdf> |
| BenchCel user guide | <https://www.agilent.com/cs/library/usermanuals/public/G5580-90000-PUI_revB_BenchCelUG_P_EN.pdf> |
| VWorks third-party integration (API and ActiveX) | <https://community.agilent.com/knowledge/automated-liquid-handling-portal/kmp/automated-liquid-handling-articles/kp1619.integrating-agilent-automated-liquid-handling-systems-with-third-party-systems> |

## Open Questions

| Area | Unknown |
| --- | --- |
| BenchCel axes | Number and type of robot axes, payload, and reach |
| Barcode | Whether the BenchCel robot carries a barcode reader or relies on a separate labeler/reader |
| MiniHub geometry | Number of cassettes, shelves per cassette, and rotation limits |
| Protocol | Serial/Ethernet command sets for these devices are not public; VWorks is the documented boundary |
| Arbitration | How VWorks prevents two movers colliding at a shared nest, and whether that is exposed to an external client |
