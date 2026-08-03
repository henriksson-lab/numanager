# INTEGRA MINI 96 — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | INTEGRA Biosciences |
| Family | MINI 96 |
| Robot class | Portable 96-channel electronic pipette — a benchtop head with **no gantry and no deck** |
| Evidence quality | Good for specifications (vendor specification page); weak for any control interface |

## What It Is

A handheld/benchtop 96-channel pipetting head. The operator positions it over a
plate; a servo motor assists all movement; the head aspirates and dispenses all
96 channels together. It fills 96- and 384-well plates and can do partial-plate
filling by individual columns.

| Item | Value |
| --- | --- |
| Channels | 96 |
| Volume variants | 0.5–12.5 µL, 5–125 µL, 10–300 µL, 50–1250 µL |
| Dimensions | 160 × 260 × 440 mm (W × D × H) |
| Weight | 9.4 kg |
| Tips | INTEGRA GRIPTIPS |
| Actuation | Servo-motor-assisted pipetting |
| Plate formats | 96- and 384-well |

## Why It Is In This Survey

It is the reduced limit case of `pipette.head`: a 96-nozzle actuator with a
volume range, a tip state, and a column-subset selection — and nothing else. No
axes, no deck, no labware model, no gripper.

If numanager's `pipette.head` abstraction cannot describe a MINI 96 on its own,
the abstraction is entangled with motion and deck concerns that belong elsewhere.
That makes it a useful design test rather than an integration target.

Its relationship to the [`integra-assist-plus.md`](integra-assist-plus.md) is
also instructive: INTEGRA's model is that pipetting heads are standalone
instruments, and robots are optional positioners for them.

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `integra-mini-96` | `pipette.head`, `instrument.standalone` |

Capability requirements:

| Capability | Reason |
| --- | --- |
| `PipetteHeadActuate` without motion | Aspirate/dispense with no gantry to coordinate |
| Column-subset selection | Partial-plate filling by column |
| Volume-variant identity | The same model name spans four disjoint volume ranges |

## Abstraction Stress Points

1. A pipetting head can be a complete instrument with no robot around it.
2. Manual positioning means well targeting is outside the device's knowledge —
   the device cannot report where it dispensed.

## Evidence

| Evidence | Link |
| --- | --- |
| MINI 96 specification page: 96 channels, four volume ranges, GRIPTIPS, servo-assisted movement, 96/384 plate filling, partial-plate columns | <https://www.integra-biosciences.com/global/en/electronic-pipettes/mini-96/mini-96-pipette-specifications> |
| MINI 96 product page | <https://www.integra-biosciences.com/global/en/electronic-pipettes/mini96> |
| Dimensions 160 × 260 × 440 mm, 9.4 kg | <https://pdf.directindustry.com/pdf/ibs-integra-biosciences/mini-96-pipette-specifications/39224-1008444.html> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Control interface | Whether any USB/Bluetooth/serial interface exists for external control or logging |
| Sensing | Whether tip presence or liquid level is sensed |
| Protocol storage | Whether programmed protocols are readable/writable externally |
