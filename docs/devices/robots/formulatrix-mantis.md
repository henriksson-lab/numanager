# FORMULATRIX MANTIS — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | FORMULATRIX |
| Family | MANTIS microfluidic liquid dispenser |
| Robot class | Non-contact microfluidic dispenser using pneumatically actuated diaphragm chips |
| Evidence quality | Good. Vendor product page, brochure, and a public chip-specification reference. |

## Dispensing Mechanism

The MANTIS meters liquid with a **microfluidic chip**, not a plunger or a pump
head. The chip contains micro-diaphragms and valves driven pneumatically:

1. Vacuum is applied to the input valve, drawing a fixed volume into the
   microdiaphragm.
2. Pressure closes the input valve, isolating the measured volume.
3. The output valve opens and pressure is applied to the diaphragm, expelling the
   droplet through the nozzle.

| Item | Detail |
| --- | --- |
| Valve cluster | Two micro-diaphragms per cluster; volume pairing selected by chip type — either 100 nL + 500 nL, or 1 µL + 5 µL |
| Cycle rate | Fill-and-dispense up to 10 times per second |
| Minimum dispense | 100 nL with CV < 2 % |
| Range | 100 nL up to hundreds of mL by repetition |
| Continuous flow option | 5 µL to thousands of µL as a continuous stream, for deep-well blocks |
| Chip materials | Polypropylene with silicone or PFE diaphragms |
| Liquid compatibility | Diaphragm design handles aqueous and organic solutions |
| Plate formats | Any SBS plate from 6-well to 1536-well, plus programmable custom layouts |
| Dead volume | As low as 6 µL when pipette tips are used as reagent reservoirs |

## Why This Matters For The Model

The dispensed volume is **quantised by the chip's diaphragm sizes**. Choosing a
chip fixes the achievable volume increments, exactly as choosing a syringe fixes
a Tecan LiHa's resolution and as the 2.5 nL drop fixes the Echo's.

Three vendors, three mechanisms, one shared modelling requirement: a dispensing
device must publish its *volume quantum and the consumable that determines it*.

Also notable: pipette tips used as reagent reservoirs. Consumable roles are not
fixed — a tip here is a source vessel, not a pipetting interface.

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `formulatrix-mantis` | `hub`, `dispenser.robot` |
| `formulatrix-mantis-chip-N` | `dispense.head.microfluidic`, `consumable.chip` |
| `formulatrix-mantis-valve-cluster` | `fluidics.valve_cluster` (two diaphragms) |
| `formulatrix-mantis-stage` | `stage.plate` |
| `formulatrix-mantis-pneumatics` | `service.pressure`, `service.vacuum` |

Capability requirements:

| Capability | Reason |
| --- | --- |
| Chip-defined volume quanta | Available volumes come in fixed pairs per chip |
| Continuous-flow dispense mode | A second, fundamentally different dispense mode on the same hardware |
| Pneumatic service state | Pressure and vacuum supply are preconditions for any dispense |
| Reservoir-role labware | A tip rack position may be a reagent source |

## Abstraction Stress Points

1. The instrument needs external pneumatic services (pressure and vacuum) to
   function — a dependency class that no pipetting robot in this survey has.
2. Two operating modes (metered droplets vs continuous stream) with disjoint
   volume ranges live on one device.
3. Volume resolution is a property of an installed consumable chip.

## Evidence

| Evidence | Link |
| --- | --- |
| MANTIS product page: dispense from 100 nL, CV < 2 %, microfluidic chips with pneumatically controlled microdiaphragms and valves, dispensing cycle description, continuous-flow option, SBS 6–1536 plates, 6 µL dead volume with pipette-tip reservoirs | <https://formulatrix.com/liquid-handling-systems/mantis-liquid-dispenser/> |
| Chip specifications reference: valve cluster with two microdiaphragms, 100 nL/500 nL or 1 µL/5 µL, up to 10 fill-dispense cycles per second, polypropylene chips with silicone or PFE diaphragms | <https://help.formulatrix.com/mantis/5.1/Content/Chip_Specifications_Reference.htm> |
| MANTIS brochure | <https://formulatrix.com/brochures/mantis.pdf> |
| FORMULATRIX liquid handling portfolio | <https://formulatrix.com/liquid-handling-systems/> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Motion | Number of axes, whether the chip head or the plate moves, and travel |
| Chip count | How many chips a MANTIS carries simultaneously and therefore how many reagents |
| Interfaces | Host connection and whether a documented control API exists |
| Sensing | Whether dispense verification, reservoir level, or clog detection exists |
| Pneumatics | Required supply pressure/vacuum and whether the instrument reports supply faults |
