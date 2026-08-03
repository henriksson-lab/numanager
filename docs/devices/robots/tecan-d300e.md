# Tecan D300e Digital Dispenser — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | Tecan (technology originally from HP) |
| Family | D300e Digital Dispenser; Duo Digital Dispenser is a related product |
| Robot class | Non-contact inkjet dispenser. **Not** a pipetting robot. |
| Evidence quality | High for mechanics and workflow: Tecan D300e Operating Manual read directly. |

This instrument is included because it is the cleanest example of a dispenser
that shares almost nothing with a pipette-and-tip liquid handler, yet must live
in the same device model.

## Hardware Components

From the operating manual's labelled hardware diagram:

| # | Component | Notes |
| --- | --- | --- |
| 1 | Dispensehead cassette | The consumable containing the actual dispense heads and fluid reservoirs |
| 2 | Pogo block | Spring-pin electrical interface that contacts the cassette; its LEDs flash during initialisation |
| 3 | Source plate holder | Holds a source plate for reference/tracking |
| 4 | Motorised stage and destination plate holder | The **plate** moves, not the dispense head |
| 5 | Destination plate | Clamped automatically when it leaves the load position |
| 6 | Deck | Fixed deck with a cutout that receives the cassette |
| 7 | Power switch and indicator light | Front |
| 8 | Power and USB connections | Rear |
| 9 | Control PC | Runs the dispenser software |

Bench space required: 47 cm wide × 40 cm deep × 23 cm high (excluding the PC).
Power 100–240 VAC, draws under 2 A.

## Dispensing Hardware

| Item | Detail |
| --- | --- |
| Principle | Thermal-inkjet-derived drop-on-demand dispensing (HP printhead lineage) |
| Cassette types | T8+ (8 dispenseheads) and D4+ (4 dispenseheads) |
| Fluids per cassette | Up to 8, one per dispensehead |
| Volume range | 11 pL to 10 µL |
| Reservoir loading | Manual: the operator pipettes each fluid into the indicated dispensehead reservoir; a blue LED illuminates the head being loaded |
| Dead volume | Each dispensehead retains fluid; required load volume exceeds dispense volume, and the excess depends on dispense volume and fluid class |
| Per-head state | Each dispensehead is `available` / `used`, and independently `locked` / `unlocked`; a locked head dispenses nothing |
| Head consumption | Heads used in a previous run are marked unavailable when the cassette is reloaded |
| Cassette-to-plate height | Adjustable during the run |

## Motion And Plate Handling

| Item | Detail |
| --- | --- |
| Axes | One motorised stage carrying the destination plate under a fixed dispense array |
| Plate clamp | Automatic clamp engages when the plate leaves the load position |
| Plate orientation | A1 must be at the top-left of the destination plate holder |
| Plate change | Multi-plate protocols pause and prompt for a plate change; plates are not necessarily requested in order and may be requested more than once |
| Show Plate | Moves the plate to an inspectable position, then returns it and resumes |

There is no gripper, no tip, no aspiration. Fluid never enters the instrument's
own plumbing — it lives entirely in the disposable cassette.

## Identity And Tracking

| Item | Detail |
| --- | --- |
| Plate ID | Optional per-plate ID, entered manually or scanned with a **USB barcode reader attached to the PC**, not to the instrument |
| Cassette ID | Optional per-cassette ID, scanned or typed |
| Validation | A run setting can require and re-validate the plate ID on every reload |
| Fluid identity | Fluids are colour- and number-coded and associated with specific cassette reservoirs, with stock concentration and required load volume |

Fluid identity with **stock concentration** is a first-class run-time concept
here. The instrument plans dispense volumes from a target concentration, which no
tip-based platform in this survey does natively.

## Control Stack And Interfaces

| Layer | Detail |
| --- | --- |
| Instrument link | USB from the control PC |
| Vendor software | D300e Dispensing Software (Windows) |
| Run control | Pause/Abort with Resume; **losing the USB connection makes the protocol unresumable**; aqueous-fluid protocols cannot be paused at all |
| Automation | The D300e is offered in an automation-friendly variant for integration into workcells; that interface is not documented here |

The "USB drop kills the run" and "aqueous protocols cannot pause" facts are
exactly the sort of hard constraint a device model must be able to express as
capability preconditions rather than discovering at runtime.

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `tecan-d300e` | `hub`, `dispenser.robot` |
| `tecan-d300e-cassette` | `consumable.cassette`, `fluid.host` |
| `tecan-d300e-head-N` | `dispense.head`, `nozzle.single_fluid` |
| `tecan-d300e-stage` | `stage.linear`, `labware.carrier` |
| `tecan-d300e-deck` | `deck` (fixed: one source holder, one destination holder, one cassette slot) |

Capability requirements:

| Capability | Reason |
| --- | --- |
| `DispenseHead` with per-head fluid binding | Each nozzle is bound to one fluid for the life of the cassette |
| Concentration-aware dispense | Targets are expressed as concentration given a stock |
| Consumable lifecycle state | available / used / locked per head; used state persists across cassette reloads |
| Operator-blocking steps | Manual fluid loading is part of the protocol, not a setup step |
| Non-resumable-run declaration | Some runs cannot be paused or resumed at all |

## Abstraction Stress Points

1. The plate moves and the dispensing hardware is stationary — the inverse of
   every gantry robot in this survey.
2. The "pipette" is a disposable with finite, per-nozzle life.
3. There is a mandatory human-in-the-loop step mid-protocol.
4. Barcode identity comes from a reader attached to the PC, not to the robot.
5. Dispense planning is chemical (concentration) rather than volumetric.

## Evidence

| Evidence | Link |
| --- | --- |
| D300e Digital Dispenser Operating Manual: hardware component list, bench space, power, USB, cassette loading, dispensehead lock/used states, blue LED, dead volume, plate clamp and A1 orientation, pause/abort semantics, plate/cassette IDs | <https://www.tecan.com/hubfs/Knowledgebase/Manuals/D300e/D300e%20Digital%20Dispenser%20Operating%20Manual.pdf> |
| D300e product page: T8+/D4+ cassettes, up to 8 fluids, 11 pL – 10 µL | <https://lifesciences.tecan.com/products/liquid_handling_and_automation/tecan_d300e_digital_dispenser> |
| D300e specifications brochure (doc 399178) | <https://www.tecan.com/doc/tecan-d300e-digital-dispenser-specifications-brochure-pdf-399178> |
| Dispensing Software User Guide | <https://www.tecan.com/hubfs/Knowledgebase/Manuals/D300e/Dispensing%20Software%20User%20Guide.pdf> |
| Duo Digital Dispenser (related product) | <https://lifesciences.tecan.com/products/liquid_handling_and_automation/duo-digital-dispenser> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Plate formats | Which destination plate formats and densities are supported, and stage travel |
| Automation interface | Whether an automation/OEM variant exposes a documented external command API |
| USB protocol | Framing and command vocabulary over the USB link |
| Fluid classes | The defined fluid classes and how they change dead volume and dispense calibration |
| Per-head metrology | Whether drop-volume calibration or verification data is readable per dispensehead |
