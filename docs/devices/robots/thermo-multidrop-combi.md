# Thermo Scientific Multidrop Combi — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | Thermo Fisher Scientific |
| Family | Multidrop Combi, Combi+, Combi SMART+, Combi nL, Multidrop Micro |
| Robot class | Bulk reagent dispenser. No aspiration, no tips, no deck. |
| Evidence quality | Moderate–good. Thermo user manual and product listings; manual not mined page-by-page in this pass. |

## Architecture

The Multidrop is the minimal dispenser: a plate carrier moves under a fixed row
of nozzles fed by peristaltic pumps.

| Element | Detail |
| --- | --- |
| Dispensing cassette | 8-channel, detachable and autoclavable; standard across the Multidrop range |
| Fluid path | Each nozzle is fed by its own tube; a peristaltic pump drives the tubes |
| Reservoir | External bottle/reservoir feeding the cassette tubing |
| Plate transport | Motorised carrier moves the plate beneath the nozzle row |
| Volume range | 0.5 – 2500 µL depending on cassette (e.g. a standard tube cassette covers 5–2500 µL) |
| Plate formats | 96-, 384-, and 1536-well plates and strips |

Because the fluid path is a swappable cassette, the same instrument has different
volume ranges and different dead volumes depending on which cassette is fitted —
the same pattern as the Tecan D300e cassettes and the Agilent Bravo heads.

## What This Class Does Not Have

| Absent | Consequence |
| --- | --- |
| Aspiration | There is no "pick up liquid" state; the source is a continuously connected reservoir |
| Tips | No tip pickup, no tip waste, no tip inventory |
| Deck | One plate position, not a labware space |
| Per-well source selection | All 8 nozzles dispense the same reagent from the same cassette |

Modelling this as a degenerate pipette would be wrong in a way that matters:
priming, purging and back-flushing the tubing are the real operations, and they
have no analogue in a tip-based device.

## Operations That Do Matter

| Operation | Why it is first-class |
| --- | --- |
| Prime | Fill the tubing and nozzles before dispensing; consumes reagent |
| Empty / purge / back-prime | Recover reagent and clear the line |
| Wash | Flush the cassette between reagents |
| Dispense volume per well | The core operation, per column or per plate |
| Dispense height / speed | Affects splash and cell viability in cell-based assays |

## Control Stack

| Layer | Detail |
| --- | --- |
| Local UI | On-board keypad/display; SMART+ variants add a richer interface |
| Host | Serial/USB control is used when integrated into workcells; Thermo and third parties supply drivers |
| Integration | Commonly integrated into screening cells behind a plate handler |

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `thermo-multidrop` | `hub`, `dispenser.bulk` |
| `thermo-multidrop-cassette` | `consumable.cassette`, `fluidics.tubing` |
| `thermo-multidrop-pump` | `pump.peristaltic` |
| `thermo-multidrop-carrier` | `stage.plate` |

Capability requirements:

| Capability | Reason |
| --- | --- |
| `BulkDispense(volume, wells)` | Column- or plate-wise dispensing from a shared reservoir |
| `Prime` / `Purge` / `Wash` | Fluid-path management is the dominant workflow |
| Cassette identity and volume envelope | Range and dead volume are cassette properties |
| Dispense height and speed | Physical parameters with biological consequences |

## Abstraction Stress Points

1. A dispenser can have a *continuous* source rather than a discrete one, which
   breaks any model where liquid must first be aspirated.
2. Fluid-path maintenance operations are protocol steps, not maintenance chores.
3. The instrument has one plate position and no labware graph at all — the deck
   abstraction must be optional.

## Evidence

| Evidence | Link |
| --- | --- |
| Multidrop Combi user manual | <https://tools.thermofisher.com/content/sfs/manuals/Multidrop%20Combi%20User%20Manual.pdf> |
| Multidrop Combi brochure: 8-channel autoclavable cassettes, 0.5–2500 µL, 96/384/1536 formats | <https://alfamed.rs/wp-content/uploads/2023/07/Multidrop-Combi.pdf> |
| Multidrop Combi+ product page | <https://www.thermofisher.com/order/catalog/product/5840330> |
| Multidrop Combi SMART+ product page | <https://www.thermofisher.com/order/catalog/product/5840340> |
| Standard tube dispensing cassette 5–2500 µL | <https://labscoop.com/us/en/product/spe/spectrum-chemical/187-10046-ea-thermo-scientific-r-multidrop-r-combi-384-and-dw-reagent-dispensing-cassettes-standard-tube-dispensing-cassette-for-volumes-5-2500-ul-1-ea> |
| Per-nozzle tubing driven by a peristaltic pump | <https://iccb.med.harvard.edu/thermo-combi> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Host protocol | Serial/USB command set and whether it is documented |
| Cassette catalogue | Full cassette list with volume ranges, dead volumes, and nozzle geometry |
| Variants | How Combi nL (nanolitre) and Micro differ mechanically |
| Sensors | Whether reservoir level, flow, or dispense verification sensing exists |
| Plate carrier | Travel, supported plate heights, and whether stacking/handling accessories exist |
