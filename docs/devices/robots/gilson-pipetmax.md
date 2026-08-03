# Gilson PIPETMAX 268 — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | Gilson |
| Family | PIPETMAX 268 (standard cover and cover-cutout variants) |
| Robot class | Compact entry-level benchtop liquid handler using swappable pipette-head cassettes |
| Evidence quality | Moderate–good. Gilson product pages plus a public user guide and IQ/OQ document (not yet mined page-by-page). |

## Architecture

PIPETMAX is head-cassette-based: the gantry carries one or two **pipette heads**
derived from Gilson's PIPETMAN manual-pipette lineage, and the head defines the
channel count and volume range.

| Item | Detail |
| --- | --- |
| Head catalogue | 1-, 4-, and 8-channel pipette heads |
| Example heads | 8×20 (8 channels, 20 µL class), 8×200 (available in 4- and 8-channel options) |
| Platform volume range | 1 – 1000 µL across heads |
| Tip management | 8-channel cassettes can be commanded to use **1 to 8 tips**, so a head can behave as a narrower device |
| Lineage | Built on PIPETMAN air-displacement pipetting technology |
| Enclosure | Standard cover, or a cover with cutouts for integration/manual access |
| Software | TRILUTION micro |

The "use 1–8 tips of an 8-channel head" behaviour is the same nozzle-subset
requirement seen on Agilent Series III heads and the SPT firefly, appearing again
at the low end of the market. It is not an exotic feature.

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `gilson-pipetmax` | `hub`, `liquid_handler.robot` |
| `gilson-pipetmax-head-N` | `pipette.head`, subset-addressable |
| `gilson-pipetmax-deck` | `deck`, `labware.host` |

Capability requirements:

| Capability | Reason |
| --- | --- |
| Head identity and volume envelope | Range depends on the installed cassette |
| Active-tip-count selection | 1–8 of 8 channels used per operation |
| Multi-head instrument | Configurations carry more than one head |

## Abstraction Stress Points

1. Nozzle-subset selection is a baseline expectation, not a high-end feature —
   the abstraction must support it from the start.
2. Head cassettes derived from manual pipettes mean the volume classes follow
   the manual-pipette catalogue (P20, P200, P1000 style), not round numbers.

## Evidence

| Evidence | Link |
| --- | --- |
| PIPETMAX pipette heads: 1-, 4-, 8-channel options | <https://www.gilson.com/default/pipetmax-pipette-heads.html> |
| PIPETMAX 8×200 head, 4- and 8-channel options | <https://www.gilson.com/default/pipetmax-8x200-pipette-head.html> |
| PIPETMAX 8×20 head | <https://www.gilson.com/default/pipetmax-8x20-pipette-head.html> |
| PIPETMAX 268 platform page: 1–1000 µL, PIPETMAN lineage, walk-away benchtop automation, intelligent tip management using 1–8 tips | <https://www.gilson.com/default/system-pipetmax.html?d=553> |
| PIPETMAX 268 with standard cover / with cover cutouts | <https://www.gilson.com/default/pipetmax-with-standard-cover.html> |
| PIPETMAX 268 user guide | <https://www.gilson.com/pub/media/docs/PIPETMAX_UG_LT255519-08.pdf> |
| PIPETMAX 268 IQ/OQ procedures | <https://www.gilson.com/pub/media/docs/PIPETMAX_IQOQ_LT255520-05.pdf> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Deck | Number and layout of deck positions, and labware constraints |
| Heads per instrument | Whether one or two heads can be mounted simultaneously |
| Motion | Axes, travel, and positional accuracy |
| Interfaces | Host connection type and whether TRILUTION exposes an external API |
| Sensors | Whether tip presence or liquid level sensing exists |
| Accessories | Whether thermal, shaking, or magnetic deck accessories are offered |
