# FORMULATRIX TEMPEST — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | FORMULATRIX |
| Family | TEMPEST bulk reagent dispenser |
| Robot class | Non-contact multi-channel bulk dispenser using microdiaphragm pump chips |
| Evidence quality | Good. Vendor product page, brochure, and a public chip-specification reference. |

## Architecture

TEMPEST shares MANTIS's microdiaphragm chip technology but scales it into a
modular multi-nozzle bulk dispenser.

| Item | Detail |
| --- | --- |
| Dispense head | Modular; accepts up to **12 replaceable chips** |
| Nozzles per chip | 8 |
| Maximum nozzles | 96 (12 chips × 8) |
| Ingredient inputs | 12 |
| Valve cluster | Two microdiaphragms per cluster; chip selection fixes the pair — either 200 nL + 1 µL, or 1 µL + 5 µL |
| Cycle rate | Fill-and-dispense up to 8 times per second |
| Minimum dispense | 200 nL with CV < 5 %; no stated upper limit |
| Throughput example | 200 nL into a 1536-well plate in 11 seconds |
| Plate support | Virtually all SBS plate types |
| Dead volume | < 40 µL non-recoverable; ~100 µL when dispensing from pipette tips |
| Clogging | Positive displacement through the diaphragm is described as clog-free |
| Automation | Robot-accessible plate position; optional plate stacking; an external pump box is part of the system |

## Distinguishing Points For The Model

1. **Chips are the unit of both fluid channel and volume resolution.** Twelve
   chips × eight nozzles gives 12 reagents at 8-fold parallelism, and the chip
   choice sets the volume quanta.
2. **The instrument is a composition of a dispenser and an external pump box.**
   A device may depend on a physically separate service unit.
3. **Optional plate stacking** turns a single-position dispenser into a
   walk-away device with a labware store attached.

## Comparison Within The Dispenser Class

| Dispenser | Metering mechanism | Channels | Min volume | Source model |
| --- | --- | --- | --- | --- |
| Tecan D300e | Thermal inkjet | 8 per cassette | 11 pL | Manually loaded cassette reservoirs |
| Beckman Echo | Acoustic ejection | 1 transducer, any well | 2.5 nL | Source plate wells |
| Thermo Multidrop | Peristaltic tubing | 8 nozzles, 1 reagent | 0.5 µL | External bottle |
| SPT dragonfly | Positive-displacement syringe | 3/6/10 independent | 200 nL | Per-head syringe |
| FORMULATRIX MANTIS | Pneumatic microdiaphragm | chip-dependent | 100 nL | Tips or tubes as reservoirs |
| FORMULATRIX TEMPEST | Pneumatic microdiaphragm | up to 12 × 8 | 200 nL | 12 ingredient inputs |

Six dispensers with six metering physics and six source topologies. This table is
the argument for a `dispense.head` abstraction parameterised by
(metering mechanism, volume quantum, source binding, channel count) rather than
one derived from pipette semantics.

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `formulatrix-tempest` | `hub`, `dispenser.bulk` |
| `formulatrix-tempest-chip-N` | `dispense.head.microfluidic`, `consumable.chip` (8 nozzles each) |
| `formulatrix-tempest-input-N` | `fluid.source` |
| `formulatrix-tempest-pump-box` | `service.pressure`, `device.external` |
| `formulatrix-tempest-stacker` | `labware.store` (optional) |

Capability requirements:

| Capability | Reason |
| --- | --- |
| Nozzle-group dispense | 8 nozzles per chip, up to 12 chips, addressable per reagent |
| Chip inventory and volume quanta | Installed chips define channels and resolution |
| External service dependency | Pump box must be present and healthy |
| Optional attached store | Plate stacker changes the device's operating mode |

## Abstraction Stress Points

1. The channel count of a dispenser can be a function of how many consumables are
   installed today.
2. A dispenser can require a companion device (pump box) that is not a module in
   the usual sense.
3. Dead volume differs by source type (tubing vs pipette tip), so consumable
   accounting must be source-aware.

## Evidence

| Evidence | Link |
| --- | --- |
| TEMPEST product page: non-contact bulk dispenser, modular head accepting up to 12 chips of 8 nozzles, 12 ingredient inputs, 200 nL CV < 5 % with no upper limit, 1536-well plate in 11 s, dead volumes, plate stacking and robot accessibility | <https://formulatrix.com/liquid-handling-systems/tempest-liquid-dispensing/> |
| Chip specifications reference: two microdiaphragms per valve cluster (200 nL + 1 µL or 1 µL + 5 µL), up to 8 fill-dispense cycles per second | <https://help.formulatrix.com/tempest/3.5/Content/Chip_Specifications_Reference.htm> |
| TEMPEST brochure | <https://formulatrix.com/brochures/tempest.pdf> |
| TEMPEST with plate stacker and pump box (reseller configuration listing) | <https://www.bostonind.com/formulatrix-tempest-liquid-handler-nanoliter-multichannel-reagent-dispenser-w-microplate-stacker-formulatrix-pump-box> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Motion | Axes and travel; whether the head or the plate moves |
| Interfaces | Host connection and whether a documented control API exists |
| Pump box | What it supplies (pressure, vacuum, or both) and whether its state is readable |
| Stacker | Capacity and control interface of the optional plate stacker |
| Sensing | Whether reservoir level, dispense verification, or nozzle-fault detection exists |
