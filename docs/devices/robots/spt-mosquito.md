# SPT Labtech mosquito — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | SPT Labtech |
| Family | mosquito LV, mosquito HV, mosquito LV genomics, mosquito HV genomics, mosquito Gen3, crystallisation variants |
| Robot class | Low-volume positive-displacement pipetting robot |
| Evidence quality | Moderate–good. Vendor product pages; no service manual reviewed. |

## The Distinguishing Technology

mosquito uses **true positive displacement**: each disposable micropipette has
its own individual piston. There is no air gap and no system liquid, so the
dispensed volume does not depend on liquid viscosity, vapour pressure, or
surface tension, and there is no cross-contamination path between samples.

This is the third fluidic paradigm in the survey, alongside air displacement
(Hamilton, Opentrons, Eppendorf) and liquid displacement with system fluid
(Tecan Liquid LiHa). A device model that assumes "aspirate = move a plunger in an
air column" cannot describe it.

| Variant | Volume range |
| --- | --- |
| mosquito LV genomics | 25 nL – 1.2 µL |
| mosquito HV genomics | 500 nL – 5 µL |

## Configuration

| Item | Detail |
| --- | --- |
| Channels | Multi-channel head using disposable micropipettes (commonly 8- or 16-tip configurations) |
| Tips | Disposable, pre-sterilised pipettes; each one contains its own piston |
| Deck | mosquito HV genomics: 5 deck positions |
| Cross-contamination | Eliminated by design because tips are discarded and no fluid path is shared |

## Consumable Model

The consumable is not a tip — it is a complete single-use pipette. Aspiration
capacity, accuracy and dead volume are properties of the *consumable*, and the
instrument supplies only motion and plunger actuation.

That inverts the usual ownership: on an air-displacement robot the channel owns
the volume envelope and the tip constrains it; here the consumable owns the
volume envelope entirely.

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `spt-mosquito` | `hub`, `liquid_handler.robot` |
| `spt-mosquito-head` | `pipette.head.positive_displacement` |
| `spt-mosquito-deck` | `deck`, `labware.host` |

Capability requirements:

| Capability | Reason |
| --- | --- |
| Pipetting-principle metadata | positive displacement / air displacement / liquid displacement changes what parameters are meaningful |
| Consumable-defined volume envelope | The mounted disposable pipette defines the range, not the channel |
| No liquid-class parameter | Positive displacement is liquid-class agnostic; a mandatory liquid-class field would be meaningless here |

## Abstraction Stress Points

1. "Liquid class" is a required concept on Tecan and Hamilton, a calibration
   input on the Echo, and meaningless on mosquito. It cannot be a mandatory
   field of a generic aspirate command.
2. Volume envelopes attach to consumables, not to channels.
3. Sub-microlitre working ranges make volume representation and rounding a real
   correctness concern (25 nL resolution alongside 5 mL channels elsewhere).

## Evidence

| Evidence | Link |
| --- | --- |
| mosquito HV genomics: 500 nL – 5 µL, positive displacement, 5 deck positions, disposable pre-sterilised pipettes | <https://www.sptlabtech.com/products/mosquito/mosquito-hv-genomics> |
| mosquito LV genomics: 25 nL – 1.2 µL | <https://www.sptlabtech.com/products/mosquito/mosquito-lv-genomics> |
| Each disposable micropipette has its own individual piston — no air gap or system liquid | <https://www.sptlabtech.com/products/mosquito/mosquito-hv> |
| mosquito product range | <https://www.sptlabtech.com/products/mosquito> |
| mosquito Gen3 launch | <https://www.sptlabtech.com/news/mosquito-gen3-launch> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Channel counts | Exact tip counts per model and whether spacing is adjustable |
| Axes | Motion architecture and travel |
| Deck | Position counts for LV/HV and crystallisation variants, and labware constraints |
| Interfaces | Host connection and whether external control is documented |
| Sensors | Whether any tip-presence or dispense verification sensing exists |
