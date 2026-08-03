# Agilent Bravo (and AssayMAP Bravo) — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | Agilent Technologies (Automation Solutions, ex-Velocity11) |
| Family | Bravo Automated Liquid Handling Platform; AssayMAP Bravo is a Bravo with a specialised head and consumables |
| Robot class | Compact fixed-deck head-only liquid handler; frequently a component inside a larger workcell |
| Evidence quality | High. Agilent Bravo Platform User Guide read directly, plus the Agilent online help. |

## Core Architecture

The Bravo has **no independent pipetting channels at all**. It is a head-only
machine: one interchangeable head is carried over a nine-position deck.

| Element | Behaviour |
| --- | --- |
| Head mount | Moves along the X and Y axes; the head itself provides Z and the pipetting drive |
| Head | Interchangeable fixed-tip or disposable-tip pipette head, or a pin tool |
| Gripper | Optional; extends from the head mount to below the pipette-head tips, and picks and places labware at specified deck locations |
| Deck | Fixed aluminium deck with nine deck locations numbered 1–3, 4–6, 7–9 left to right, each holding a platepad or accessory |
| Manual movement | The head mount can be physically moved by hand while the instrument is powered off |

Dimensions 64.8 × 43.8 × 69.7 cm; weight 52.16 kg.

## Pipette Heads And Pin Tools

Heads are field-interchangeable, and the mounted head defines nearly all of the
liquid-handling capability.

### Disposable-tip heads

| Head | Max volume | Dispense into |
| --- | --- | --- |
| 8LT | 250 µL | 96-, 384-well; single column (8 wells) |
| 16ST | 70 µL | 384-, 1536-well; single column (16 wells) |
| 96LT | 250 µL | 96-, 384-well; single column (8) or row (12) |
| 96ST | 70 µL | 96-, 384-, 1536-well; single column (8) or row (12) |
| 384ST | 70 µL | 384-, 1536-well; single column (16) or row (24) |

Per-tip working ranges quoted by Agilent: 10 µL head 0.3–10 µL, 30 µL head
0.5–30 µL, 70 µL head 0.75–70 µL, 96LT 250 µL head 2.0–250 µL.

### Fixed-tip heads

| Head | Max volume | Dispense into |
| --- | --- | --- |
| 96F50 | 50 µL | 96-, 384-well |
| 384F50 | 50 µL | 384-, 1536-well |

Fixed-tip heads dispense into an entire microplate only — they **cannot** address
a single column or row.

### Head generations

- Series II heads dispense to all wells simultaneously only.
- Series III heads can additionally dispense into a single column, single row, or
  single well, which is what makes on-deck serial dilution possible without a
  head change.
- If a gripper is attached, Series II heads have restricted tip compatibility.

Two consequences for the model: (a) plate-format compatibility and max volume are
*tip*-dependent, not head-dependent alone; (b) addressable-subset granularity is a
head property that changes by hardware generation.

### AssayMAP Bravo

| Item | Detail |
| --- | --- |
| Head | Bravo 96AM Head — 96 syringes rather than air-displacement barrels |
| Consumable | AssayMAP cartridges (packed-bed micro-chromatography columns) instead of tips |
| Seating station | 96AM Cartridge and Tip Seating Station occupies a deck location (documented at deck location 2) |
| Stripper plate | Spring-loaded mechanism on the head, **actuated by the Bravo gripper assembly**, that lets the head remove cartridges while still holding liquid in the syringes |

The stripper-plate detail is a strong abstraction signal: the gripper is not only
a labware mover, it is also an actuator for a mechanism on the head. Devices on
this platform are physically coupled.

## Control Surface (per vendor software)

Bravo Diagnostics exposes hardware-level operations that map well onto raw device
capabilities:

| Operation | Notes |
| --- | --- |
| Home and jog | Incremental head-mount movement and homing |
| Teachpoints | Named taught positions that tell heads where to move for a task |
| Deck-location configuration | Declares what accessory occupies each of the nine locations |
| Individual tasks | Tips On, Tips Off, Aspirate, Dispense can be run individually outside a protocol |
| Gripper adjustment | Separate gripper teachpoints |
| Head parameters | Volume calibration, pipette speed, tip touching, dynamic tip extension/retraction |

"Run individual tasks" is exactly the raw-actuator boundary numanager wants —
Tips On / Tips Off / Aspirate / Dispense as discrete commands, separate from
protocol execution.

## Interfaces And Safety

| Item | Detail |
| --- | --- |
| Connection panel | RS-232 serial port, Ethernet port, pendant port, fuse holder, AC power entry |
| Serial vs Ethernet | Either one, not both — connecting via serial means Ethernet is not used |
| Pump I/O port | RJ-45 (Cat-5/6 cable) for attaching a peristaltic pump; **not** an Ethernet port, only for Agilent accessories |
| Pendant | Handheld unit with a red robot-disable button and a silver Go button; the disable button interrupts the safety interlock circuit |
| Safety interlock | Must be closed for the platform to operate; can be closed by a jumper, but EU machinery directives require a real guard, light curtain, or enclosure |
| Light curtain | Optional Agilent product wired into the interlock circuit; cuts power when the boundary is breached |
| Indicator lights | Two front light panels: solid blue = standby, flashing green = protocol running, flashing orange = initialised and Diagnostics open, flashing red = protocol error |
| Recovery dialogue | On robot disable, the software offers Abort / Retry / Ignore for the current command or task |

The Abort/Retry/Ignore triple is a per-command recovery vocabulary, not a
run-level one — worth mirroring in any command result type.

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `agilent-bravo` | `hub`, `liquid_handler.robot` |
| `agilent-bravo-head` | `pipette.head` (or `tool.pin_tool`) — single mounted head, type discovered from the profile |
| `agilent-bravo-gripper` | `labware.mover`, `actuator.head_mechanism` |
| `agilent-bravo-deck` | `deck`, `labware.host` — exactly nine locations with accessory bindings |
| `agilent-bravo-accessory-N` | `module.*` per accessory installed at a deck location |
| `agilent-bravo-pendant` | `safety.interlock` |

Capability requirements:

| Capability | Reason |
| --- | --- |
| `PipetteHeadActuate` with column/row/well subsets | Series III heads address subsets; Series II and fixed-tip heads do not |
| `ToolChange` / head identity | The head is field-swappable and defines the volume envelope and format compatibility |
| Teachpoint-based motion | Positions are named taught points, not free coordinates |
| Deck-location accessory binding | A deck location is typed (platepad, MicroWash station, shaking station, …) |
| Per-command recovery verbs | Abort / Retry / Ignore |
| Interlock state | First-class safety property with a real hardware circuit |

## Abstraction Stress Points

1. A liquid handler can have **zero** independent channels — the head is the
   whole pipetting subsystem.
2. Head capability is a product of (head generation × head type × mounted tip).
3. The gripper doubles as a mechanical actuator for a head mechanism.
4. Motion is expressed in taught points stored in a vendor profile, so "move to
   XYZ" is not the native primitive.
5. Accessory ports exist that look like Ethernet but are not (pump I/O).

## Evidence

| Evidence | Link |
| --- | --- |
| Bravo Platform User Guide: nine deck locations and numbering, head mount X/Y, gripper below tips, pendant, interlock, connection panel, indicator lights, head tables, serial dilution, Diagnostics operations, 52.163 kg | <https://med.stanford.edu/content/dam/sm/htbc/documents/eq/G5409-90006_BravoUG_EN.pdf> |
| Agilent online help — Bravo liquid-handling heads (96LT/96ST/384ST/96AM) | <https://automation.help.agilent.com/AutomationSolutionsKB14/Bravo%20User%20Guide/Introduction.03.6.html> |
| Bravo datasheet: dimensions, nine pipettable deck positions, tip sizes 10/30/70/250 µL | <https://www.agilent.com/Library/datasheets/Public/5990-3480EN.pdf> |
| Bravo product page | <https://www.agilent.com/en/product/automated-liquid-handling/automated-liquid-handling-platforms/bravo-automated-liquid-handling-platform> |
| AssayMAP Bravo getting started / error recovery: 96AM head, cartridge seating station at deck 2, gripper-actuated stripper plate | <https://www.agilent.com/cs/library/usermanuals/public/D0000352_AssayMAPBravo_GettingStarted.pdf> |
| AssayMAP labware reference guide | <https://www.agilent.com/cs/library/usermanuals/public/G5496-90018B_AssayMAP_Labware_Guide_S_EN.pdf> |
| VWorks third-party integration (API and ActiveX) | <https://community.agilent.com/knowledge/automated-liquid-handling-portal/kmp/automated-liquid-handling-articles/kp1619.integrating-agilent-automated-liquid-handling-systems-with-third-party-systems> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Serial/Ethernet protocol | The wire protocol on the RS-232 and Ethernet ports is undocumented publicly; VWorks is the documented boundary |
| VWorks API scope | Whether the VWorks API/ActiveX exposes Bravo Diagnostics-level tasks (Tips On, Aspirate) or only protocol execution |
| Accessory catalogue | Full list of deck accessories (heating/cooling, shaking, magnet, vacuum, MicroWash) and their individual control interfaces |
| Sensors | Whether the Bravo has any liquid-level or pressure sensing at all — none is described in the user guide reviewed |
| Teachpoint model | How teachpoints and profiles are stored and whether they are readable externally |
