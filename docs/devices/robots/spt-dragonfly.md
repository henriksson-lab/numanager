# SPT Labtech dragonfly discovery — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | SPT Labtech |
| Family | dragonfly discovery |
| Robot class | Multi-reagent non-contact positive-displacement dispenser |
| Evidence quality | Moderate–good. Vendor product and store pages, plus the public dragonfly technology help page. |

## Architecture

| Item | Detail |
| --- | --- |
| Dispense heads | 3, 6, or 10 **independent** positive-displacement heads |
| Syringes | Disposable, non-contact, positive-displacement syringes; polypropylene bodies with HDPE plungers |
| Volume range | 200 nL – 4 mL, into 96- through 1536-well microplates |
| Single fill | Up to 4 mL in one syringe fill |
| Aspiration per head | Each head aspirates between 0.3 mL and 4 mL |
| Accuracy | Quoted as < 5 % error at 1 µL (Artel multichannel verification) |
| Liquid handling | Suitable across a wide range of liquid viscosities because displacement is positive |

Each head is an independent reagent channel: N heads means N simultaneously
available reagents, each with its own syringe, its own fill state, and its own
source.

## What Makes It A Distinct Device Class

Unlike the Tecan D300e (fixed cassette of up to 8 fluids, picolitre drops) and
unlike the Thermo Multidrop (8 nozzles sharing one reagent), dragonfly is
"N independent reagents × wide volume range × non-contact".

| Dispenser | Fluid channels | Volume span | Source model |
| --- | --- | --- | --- |
| Tecan D300e | up to 8 per cassette | 11 pL – 10 µL | Manually loaded reservoirs on a consumable |
| Thermo Multidrop Combi | 8 nozzles, 1 reagent | 0.5 – 2500 µL | External bottle via peristaltic tubing |
| SPT dragonfly | 3 / 6 / 10 independent | 200 nL – 4 mL | Per-head disposable syringe drawing from a reservoir |

Three dispensers, three completely different source topologies. `dispense.head`
must therefore carry an explicit source binding model rather than assuming one.

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `spt-dragonfly` | `hub`, `dispenser.robot` |
| `spt-dragonfly-head-N` | `dispense.head.positive_displacement`, `fluid.channel` |
| `spt-dragonfly-syringe-N` | `consumable.syringe` with fill state and capacity |
| `spt-dragonfly-plate-stage` | `stage.plate`, `labware.carrier` |

Capability requirements:

| Capability | Reason |
| --- | --- |
| Per-head reagent binding | Each head carries a distinct reagent |
| Explicit fill / refill operations | A head must aspirate 0.3–4 mL before dispensing |
| Non-contact dispense with height control | Dispense height matters and there is no tip touching |
| Head-count-dependent parallelism | 3, 6, or 10 heads changes achievable protocols |

## Abstraction Stress Points

1. A dispenser can have a *stateful reservoir per channel* that must be filled,
   tracked, and emptied — halfway between a pipette (transient volume) and a
   Multidrop (continuous supply).
2. Volume spans four orders of magnitude on one instrument (200 nL – 4 mL).
3. Viscosity independence removes the liquid-class concept again, as on mosquito.

## Evidence

| Evidence | Link |
| --- | --- |
| dragonfly discovery product page: 3, 6 or 10 independent positive-displacement dispense heads | <https://www.sptlabtech.com/products/dragonfly/dragonfly-discovery> |
| Syringes: disposable non-contact positive displacement, 200 nL – 4 mL, 96–1536 well plates, polypropylene body / HDPE plunger | <https://store.sptlabtech.com/shop/4150-07200-dragonfly-r-discovery-syringes-pack-100-syringes-plungers-121> |
| Per-head aspiration 0.3–4 mL, up to 4 mL per syringe fill, accuracy claim | <https://www.medicalexpo.com/prod/spt-labtech/product-128127-940914.html> |
| dragonfly discovery technology (help centre) | <https://www.sptlabtech.com/product-help-center/dragonfly-discovery-user-manual-dragonfly-discovery-technology> |
| dragonfly product range | <https://www.sptlabtech.com/products/dragonfly> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Motion | Axes, plate stage travel, and whether heads move or the plate moves |
| Reservoirs | Physical reservoir format and how a head is bound to a source |
| Interfaces | Host connection and whether an external control API exists |
| Sensors | Whether syringe fill level or dispense verification is sensed |
| Integration | Whether dragonfly is offered in an automation-ready variant for workcells |
