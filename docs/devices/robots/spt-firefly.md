# SPT Labtech firefly / firefly+ — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | SPT Labtech |
| Family | firefly, firefly+ |
| Robot class | Integrated genomics workstation — pipetting head **plus** positive-displacement dispensers **plus** gripper **plus** thermal/shaker modules in one enclosure |
| Evidence quality | High. SPT Labtech's public product help centre includes a specifications page, read directly. |

This is the densest single machine in the survey: it combines four device classes
that are separate instruments elsewhere. If numanager's model can express a
firefly, it can express most of the rest.

## Pipetting Head

| Item | Detail |
| --- | --- |
| Volume range | 0.5 µL – 125 µL |
| Configurations | 384 tips, 96 tips, a column of 16 (384 format), or a column of 8 (96 format) |
| Tip loading | Automatic from tip boxes under software control |
| Integrated grippers | The pipetting head itself carries grippers |

One head that can act as a 384 head, a 96 head, or an 8/16-channel column device
is a strong argument that "channel count" must be a *mode* of a head device, not
a static property.

## Dispensing System

| Item | Detail |
| --- | --- |
| Dispense heads | 3 or 6, model-dependent |
| Operation | Synchronised for throughput, or independently controlled for reagent flexibility |
| Technology | True positive-displacement disposable syringes — no air gap, no system liquid |
| Volume range | 200 nL – 3690 µL (marketing materials quote 200 nL – 4 mL for the syringe family) |
| Contact | Non-contact dispensing, liquid-class agnostic |
| Reservoirs | 6 reservoirs in a tray in the basement level |
| Reachable positions | Dispensers address plates in positions 6 and 7 on both upper and lower decks |

Note the asymmetry: the pipetting head can reach the whole deck, the dispensers
can reach only two positions per deck. Reachability is per-tool, not per-robot.

## Decks

| Level | Positions | Max labware height |
| --- | --- | --- |
| Top deck | 8 plate positions | 49.7 mm |
| Bottom deck | 8 plate positions | 93 mm (deep items such as tip sets must go here) |
| Basement | Reservoir tray; optionally a plate shaker and a plate thermal module | — |

Both decks are **movable**. A moving deck plus a moving head means the coordinate
frame of a labware position is not static — a fundamental difference from every
fixed-deck robot in this survey.

## Integrated Modules

| Module | Specification |
| --- | --- |
| Shaker (genomics models) | 200 – 3000 rpm; supports all firefly plate types including deep-well |
| Thermal module (genomics models) | −20 °C to 99 °C in 0.1 °C steps; regulation ±0.1 °C; uniformity ±0.7 °C at 4 °C across the cooling surface; 12 °C/min above ambient; **incubation only, not PCR** |
| Thermal block | Passive cooling adapter, ~1 °C per minute drift when off the active module |
| firefly+ | Adds an integrated Inheco On Deck Thermal Cycler plus storage shelves for labware and consumables |
| Magnetic blocks | Accessories the head's gripper can move labware on and off |

"Incubation only, not PCR" is a capability constraint that no temperature-range
number captures — the same hardware property (a temperature module) has
application-level limits that must be expressible.

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `spt-firefly` | `hub`, `liquid_handler.robot` |
| `spt-firefly-head` | `pipette.head`, `mode.multi_format` |
| `spt-firefly-head-gripper` | `labware.mover`, `tool.head_mounted` |
| `spt-firefly-dispenser-N` | `dispense.head.positive_displacement` |
| `spt-firefly-reservoir-N` | `fluid.source` |
| `spt-firefly-deck-top` / `-bottom` | `deck`, `motion.deck` — moving decks |
| `spt-firefly-shaker` | `module.shaker` |
| `spt-firefly-thermal` | `module.temperature` |
| `spt-firefly-thermocycler` | `module.thermocycler` (firefly+) |

Capability requirements:

| Capability | Reason |
| --- | --- |
| Head format mode selection | 384 / 96 / column-of-16 / column-of-8 on one head |
| Independent vs synchronised dispenser groups | Dispensers can be ganged or driven separately |
| Per-tool reachability map | Dispensers reach only positions 6 and 7 |
| Moving-deck coordinate handling | Labware position depends on deck state |
| Module constraint metadata | Temperature module usable for incubation but not thermocycling |
| Height-limited deck levels | 49.7 mm vs 93 mm changes what labware fits where |

## Abstraction Stress Points

1. The deck moves. Any model that assumes a static deck frame will produce wrong
   coordinates.
2. One instrument spans tip-based pipetting, positive-displacement non-contact
   dispensing, gripping, shaking, heating and (on firefly+) thermocycling.
3. Reachability differs per tool within the same robot.
4. Labware height limits are per-deck-level constraints, not per-slot.
5. Head channel count is dynamic.

## Evidence

| Evidence | Link |
| --- | --- |
| firefly specifications page: head 0.5–125 µL with 384/96/16/8 configurations, 3 or 6 dispense heads with 200 nL–3690 µL syringes, 6 basement reservoirs, dispenser access to positions 6 and 7, two movable 8-position decks with 49.7 mm and 93 mm height limits, shaker 200–3000 rpm, thermal module −20 to 99 °C ±0.1 °C, 12 °C/min, incubation only, thermal block ~1 °C/min, firefly+ Inheco thermocycler | <https://www.sptlabtech.com/product-help-center/firefly-user-guide-specifications-1.3-firefly-technology> |
| firefly key concepts (help centre) | <https://www.sptlabtech.com/product-help-center/firefly-user-guide-specifications-1.1-key-concepts> |
| firefly product page: head grippers move labware between decks and onto accessories such as magnetic blocks | <https://www.sptlabtech.com/products/firefly> |
| firefly all-in-one liquid handling overview | <https://www.sptlabtech.com/firefly-all-in-one-liquid-handling> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Interfaces | Host connection type and whether any external control API exists |
| Deck motion | Travel and axes of the moving decks, and how positions are addressed |
| Sensors | Whether tip presence, liquid level, or dispense verification sensing exists |
| Syringe lifecycle | Whether dispenser syringes are tracked per-use like the D300e dispenseheads |
| Gripper | Payload, jaw range, and whether rotation is possible |
