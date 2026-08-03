# Hamilton Microlab VANTAGE — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | Hamilton Company |
| Family | Microlab VANTAGE 1.3 and VANTAGE 2.0 (deck length in metres) |
| Robot class | Enclosed, modular, multi-arm independent-channel deck liquid handler |
| Evidence quality | Moderate. Vendor product page plus dealer/reseller configuration listings. No manufacturer service manual reviewed. |
| Relationship to STAR | Same CO-RE pipetting lineage as [`hamilton-microlab-star.md`](hamilton-microlab-star.md), with CO-RE II tip attachment and a redesigned enclosed frame |

## Platform Variants

| Model | Deck length | Tracks | ANSI/SLAS positions |
| --- | --- | --- | --- |
| VANTAGE 1.3 | 1.3 m | 54 | 35 |
| VANTAGE 2.0 | 2.0 m | 80 | 60 |

Unlike the STAR line, the VANTAGE deck is described in *both* tracks and
ANSI/SLAS plate positions. That dual addressing is itself a modelling signal: the
carrier/track space and the plate-position space coexist on one deck.

## Motion Structure

| Element | Behaviour |
| --- | --- |
| Arms | Modular; the platform is sold as "multiple arms, channels, and transport options". A single VANTAGE 1.3 pipetting arm can carry up to 16 × 1 mL channels, a plate gripper, and a 96-probe head. |
| Arm payload | The pipetting arm holds pipetting channels, transport devices and camera hardware — i.e. an arm is a *carrier of tools*, not a fixed pipette mount |
| Channel motion | Per-channel independent Y and Z as on the STAR line |
| Fluid Motion | Hamilton's slim-profile channel/head geometry lets X1 channels or the 96 multi-probe head reach up to 5 additional deck tracks compared with the previous generation |

## Pipetting Hardware

| Option | Count per arm | Notes |
| --- | --- | --- |
| 1 mL independent channels | up to 16 | Standard high-density channel option |
| 5 mL independent channels | up to 8 | Large-volume channel option |
| MagPip channels | 8 | Tubular linear drive, marketed for fast low-volume work |
| CO-RE 96 Probe Head | 1 | 96 nozzles, shared Z and plunger |
| 384 Multi-Probe Head | 1 | 384 nozzles |

Tip attachment is CO-RE II (second-generation compressed O-ring expansion).
Pipetting is air displacement.

## Labware Handling

| Tool | Notes |
| --- | --- |
| Plate gripper on the pipetting arm | Channel-mounted gripper, as on the STAR |
| iSWAP | Available as a plate-transport arm on configured systems |
| Autoload | Carrier loading with barcode identification on configured systems |
| Camera on arm | Arm-mounted camera technology for deck/labware verification |

## Control Stack And Interfaces

| Layer | Detail |
| --- | --- |
| Vendor software | VENUS |
| Documented API | Hamilton VENUS Web API (devices, deck layout, method execution, status, notifications); credentialed developer portal |
| Firmware boundary | Not publicly documented. PyLabRobot's Hamilton backend targets STAR/STARlet, not VANTAGE, so raw-firmware reachability on VANTAGE is unproven. |

## Device-Model Implications

The VANTAGE is the strongest argument in this survey for making *arm* a
first-class device rather than an implementation detail:

- one robot can have several arms;
- an arm's tool complement is configurable (channels + head + gripper + camera);
- the same arm can hold heterogeneous tools simultaneously.

| Proposed device | Kind tags |
| --- | --- |
| `hamilton-vantage` | `hub`, `liquid_handler.robot` |
| `hamilton-vantage-arm-N` | `motion.arm`, `tool.host` |
| `hamilton-vantage-channel-N` | `pipette.channel` |
| `hamilton-vantage-head-96` / `-384` | `pipette.head` |
| `hamilton-vantage-gripper` | `labware.mover`, `tool.arm_mounted` |
| `hamilton-vantage-iswap` | `labware.mover`, `motion.arm` |
| `hamilton-vantage-camera` | `camera.snapshot`, `inspection.camera` |
| `hamilton-vantage-deck` | `deck`, `labware.host` |

Required capability shapes beyond the OT-2 set: arm-scoped tool inventory,
nozzle-subset head actuation, and a deck that resolves both track ranges and
SLAS positions.

## Abstraction Stress Points

1. Arm count is a configuration variable, so device topology must be discovered,
   not hard-coded.
2. Deck addressing is dual (tracks *and* SLAS positions).
3. Reachability depends on which tool is mounted — Fluid Motion explicitly
   changes how many tracks a channel or head can reach.

## Evidence

| Evidence | Link |
| --- | --- |
| VANTAGE product page: modular arms/channels/transport, CO-RE II, arm holds channels/transport/camera | <https://www.hamiltoncompany.com/Microlab-VANTAGE> |
| Deck sizes: 1.3 m = 54 tracks / 35 SLAS, 2.0 m = 80 tracks / 60 SLAS | <https://www.hamiltoncompany.com/Microlab-VANTAGE> |
| Fluid Motion reach (5 additional tracks for X1 channels or 96 MPH) | <https://www.hamiltoncompany.com/technologies/fluid-motion> |
| VANTAGE 1.3 arm capacity: 16 × 1 mL channels, 8 × 5 mL channels, plate gripper, 96-probe head | <https://www.htslabs.de/en/offer/hamilton-vantage-1-3> |
| VANTAGE brochure (PDF; text layer not extractable in this pass) | <https://analityk.com/wp-content/uploads/2024/04/Microlab-VANTAGE-Brochure.pdf> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Arm count | Maximum simultaneous arms on VANTAGE 1.3 vs 2.0 |
| Physical envelope | Dimensions, weight, power, host interface (USB vs Ethernet) not confirmed from a manufacturer datasheet |
| Deck geometry | Track pitch, SLAS position origin, and how the two addressing schemes relate |
| Sensors | Whether cLLD/pLLD/TADM/MAD coverage matches the STAR line exactly, including head-nozzle sensor placement |
| Camera | Resolution, framing, and whether images are retrievable over the API |
