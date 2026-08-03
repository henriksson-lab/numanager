# Revvity (PerkinElmer) JANUS G3 — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | Revvity (formerly PerkinElmer) |
| Family | JANUS G3 Mini / Standard / Expanded / Integrator; BioTx Pro and Pro Plus application variants |
| Robot class | Configurable deck liquid handler combining a variable-span tip arm with a modular dispense head |
| Evidence quality | Moderate. Revvity product pages and configuration listings; no manufacturer manual reviewed. |

## Configuration Axes

JANUS is sold as a matrix of (deck size × arm complement), and the product codes
reflect it directly:

| Axis | Options |
| --- | --- |
| Deck size | Mini, Standard, Expanded, Integrator |
| Pipetting arm | 4-tip Varispan, 8-tip Varispan, or none |
| Dispense head | 96-channel MDT, 384-channel MDT, or none |
| Gripper | Present or absent |
| Enclosure | Present or absent |

Real catalogue configurations include "8-tip + MDT, Expanded", "MDT, Expanded,
Gripper", and "Mini, 4-tip", i.e. arms and heads are independently selectable and
either can be omitted.

## Varispan Pipetting Arm

| Item | Detail |
| --- | --- |
| Tips | 4 or 8 |
| Span | **Variable tip spacing** ("Varispan") — the arm changes its own tip pitch to match source and destination labware |
| Volume | 1 µL to 5000 µL on the 8-tip arm |

Variable span is the defining feature. Tecan (9–38 mm), Hamilton (DPS) and
Revvity (Varispan) all implement it; three of the major vendors treat channel
pitch as a commanded degree of freedom.

## MDT Dispense Head

| Item | Detail |
| --- | --- |
| Formats | 96-channel or 384-channel |
| Name | Modular Dispense Technology — the head is a module, exchangeable within the family |

## Deck And Physical

| Item | Detail |
| --- | --- |
| Deck (Mini) | Up to 12 SBS labware positions |
| Dimensions (Expanded with enclosure) | 1170 × 1780 × 890 mm |
| Gripper | Optional plate gripper for on-deck labware movement |
| Integrator variant | Designed to be embedded in a larger automated system |

## Control Stack

| Layer | Detail |
| --- | --- |
| Vendor software | WinPREP, with an Application Assistant layer for monitoring, sample tracking, and integrating third-party devices |
| External API | Not identified in this pass |
| Protocol | Not public from the sources reviewed |

WinPREP explicitly integrates *other* devices, so JANUS often plays the role of
the cell controller rather than a leaf device — a topology numanager should be
able to represent (a robot that is also a scheduler for its peripherals).

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `revvity-janus` | `hub`, `liquid_handler.robot` |
| `revvity-janus-varispan` | `motion.arm`, `pipette.arm`, `pitch.variable` |
| `revvity-janus-varispan-tip-N` | `pipette.channel` |
| `revvity-janus-mdt` | `pipette.head` or `dispense.head` |
| `revvity-janus-gripper` | `labware.mover` |
| `revvity-janus-deck` | `deck`, `labware.host` |

Capability requirements: nothing structurally new beyond what the Tecan and
Hamilton notes already demand (typed arms, variable pitch, head-format
selection, optional gripper). JANUS is therefore useful as *confirmation* that
the arm/head/gripper decomposition generalises across vendors rather than being
an artefact of one design.

## Abstraction Stress Points

1. Configuration is a product matrix; a JANUS may have an arm, a head, both, or
   (with a gripper-only build) neither pipetting unit in the usual sense.
2. Deck size changes the physical envelope by nearly a metre, so geometry must be
   per-instance.
3. The vendor software is also an integration platform, blurring "device" and
   "cell controller".

## Evidence

| Evidence | Link |
| --- | --- |
| JANUS workstation family page | <https://www.revvity.com/category/janus-workstations> |
| JANUS G3 Expanded, 8-tip + MDT: 8-tip Varispan 1–5000 µL, 96/384 MDT head, WinPREP | <https://www.revvity.com/product/janus-g3-expanded-8-tip-mdt-ruo-yjl8m01> |
| JANUS G3 Mini 8-tip: deck fitting up to 12 SBS labware | <https://www.revvity.com/product/janus-g3-mini-8-tip-ruo-yjs8001> |
| JANUS G3 Mini 4-tip | <https://www.perkinelmer.com/product/janus-g3-mini-4-tip-ruo-yjs4001> |
| JANUS G3 MDT Expanded with gripper (arm-less configuration) | <https://www.revvity.com/product/janus-g3-expanded-mdt-grip-ruo-yjlmg01> |
| Expanded-with-enclosure dimensions 1170 × 1780 × 890 mm | <https://www.revvity.com/product/janus-g3-expanded-mdt-ruo-yjlm001> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Varispan range | Minimum and maximum tip pitch in mm |
| MDT volume ranges | Per-format volume envelopes for the 96 and 384 MDT heads |
| Sensors | Whether liquid-level detection, pressure monitoring, or tip-presence sensing exists |
| Deck geometry | Position counts for Standard / Expanded / Integrator decks and the addressing scheme |
| Interfaces | Host connection and whether WinPREP exposes an external control interface |
