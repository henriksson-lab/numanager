# INTEGRA ASSIST PLUS (and ASSIST) — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | INTEGRA Biosciences |
| Family | ASSIST, ASSIST PLUS |
| Robot class | Pipette *positioner* — the robot holds and actuates a handheld electronic pipette |
| Evidence quality | Moderate–good. Vendor product page and user guide summaries. |

## The Distinguishing Idea

The ASSIST PLUS has no pipetting mechanism of its own. It is a three-axis
positioner with a pipette holder into which a **standard INTEGRA handheld
electronic pipette** (VIAFLO or VOYAGER, or the D-ONE single-channel module) is
mounted. The robot moves the pipette and triggers it; the pipette does the
fluidics.

Consequence: the "pipette" is a separate, independently usable instrument with
its own firmware, its own volume range, and its own channel count, temporarily
docked into a robot.

## Pipetting

| Item | Detail |
| --- | --- |
| Channels | 1 to 16, depending on the mounted pipette |
| Volume envelope | 0.5 – 1250 µL across the pipette range |
| VOYAGER pipettes | Adjustable tip spacing — the pipette itself changes its channel pitch |
| D-ONE module | Single-channel pipetting module for the same holder |
| Tips | INTEGRA GRIPTIPS |
| Controlled parameters | Constant pipetting angle, consistent tip immersion depth, controlled pipetting speed, accurate well targeting |

## Deck

| Item | Detail |
| --- | --- |
| Work positions | 3 positions for reservoirs, tube racks and plates |
| Tip positions | 2 dedicated positions for automatic tip loading and tip ejection |
| Labware range | Tubes through 384-well plates, in landscape or portrait orientation |

Five positions total, of which two are functionally reserved. Portrait/landscape
plate orientation as a deck property is worth noting: plate rotation changes well
addressing without any gripper being involved.

## Control Stack

| Layer | Detail |
| --- | --- |
| Operation | Select a protocol, load labware and tips, press RUN |
| Programming | Protocols authored in INTEGRA's software (VIALAB) and transferred to the instrument/pipette |
| External API | Not identified in this pass |

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `integra-assist-plus` | `hub`, `pipette.positioner` |
| `integra-assist-plus-holder` | `mount`, `tool.host` |
| `integra-viaflo` / `integra-voyager` / `integra-d-one` | `pipette` or `pipette.head` — a device that exists independently of the robot |
| `integra-assist-plus-deck` | `deck`, `labware.host` (3 work + 2 tip positions) |

Capability requirements:

| Capability | Reason |
| --- | --- |
| Composed device identity | The robot's pipetting capability is entirely delegated to a docked instrument |
| Pipette-side pitch control | VOYAGER changes tip spacing itself, so pitch is a property of the pipette, not the arm |
| Plate orientation | Landscape/portrait is a labware placement property affecting well mapping |

## Abstraction Stress Points

1. A liquid handler may be a **composition of two separately purchasable
   instruments**, one of which is a handheld device. numanager's device graph
   must allow a child device that can also exist standalone.
2. Variable tip spacing can live on either side of the robot/pipette boundary.
3. Very small decks with functionally reserved positions argue for typed deck
   positions (work / tip-load / tip-eject) rather than uniform slots.

## Evidence

| Evidence | Link |
| --- | --- |
| ASSIST PLUS product page: mount a VIAFLO or VOYAGER pipette, 1–16 channels, 0.5–1250 µL, 3 work positions plus 2 tip positions, tubes to 384-well in landscape or portrait, D-ONE module | <https://www.integra-biosciences.com/global/en/pipetting-robots/assist-plus> |
| ASSIST (predecessor) product page | <https://www.integra-biosciences.com/united-states/en/pipetting-robots/assist> |
| ASSIST PLUS user guide | <https://manuals.plus/integra/assist-plus-pipetting-robot-manual> |
| INTEGRA pipetting robots overview | <https://www.integra-biosciences.com/united-states/en/pipetting-robots> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Axes | Whether the positioner is X/Y/Z or a reduced-axis mechanism, and its travel |
| Pipette link | How the robot commands the docked pipette (electrical contacts, Bluetooth, or mechanical plunger actuation) |
| Protocol transfer | Whether protocols live on the robot, the pipette, or the PC |
| External control | Whether any documented remote interface exists |
| Tip handling | Whether tip pickup/eject is sensed or open-loop |
