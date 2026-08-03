# Hamilton Microlab NIMBUS — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | Hamilton Company |
| Family | Microlab NIMBUS (also sold as NIMBUS96, NIMBUS4, and assay-ready variants) |
| Robot class | Compact enclosed benchtop liquid handler; either independent channels **or** a 96 head |
| Evidence quality | Moderate. Vendor product page plus vendor brochure PDF (text not extractable in this pass) and equipment-directory listings. |

## Platform Configuration

The NIMBUS is configured as one of a small number of fixed shapes rather than a
freely composable arm:

| Configuration | Pipetting hardware |
| --- | --- |
| NIMBUS4 | 4 independent CO-RE pipetting channels |
| NIMBUS (8-channel) | 8 independent CO-RE pipetting channels |
| NIMBUS96 | One 96-channel CO-RE head |

The 96-channel CO-RE head is quoted with a dynamic range of 1.0 µL to 1000 µL.

## Physical And Deck

| Item | Value |
| --- | --- |
| Footprint | ~91 × 61 × 61 cm (W × D × H) |
| Deck | 11, 12, or 20 ANSI/SLAS positions depending on enclosure/configuration |
| Positional precision | ±0.1 mm on all axes |
| Deck contents | Plates, reagents, consumables, and on-deck devices occupy SLAS positions |

Unlike the STAR/VANTAGE lines, the NIMBUS deck is quoted purely in ANSI/SLAS
positions, with no track/carrier layer. That makes it much closer to an OT-2-style
slot deck.

## Motion Structure

| Element | Behaviour |
| --- | --- |
| Arm | Single gantry arm over a fixed deck |
| Channels | Independent Z per channel with CO-RE tip attachment; variable Y spacing on the multi-channel variants |
| 96 head | Single rigid head; shared Z and plunger |
| Plate movement | CO-RE paddles / gripper tooling picked up by channels on channel-equipped variants |

## Sensing

CO-RE tip attachment, air-displacement pipetting, and Hamilton's monitoring
technologies (cLLD, and TADM/MAD where fitted) follow the STAR-line pattern.
The specific sensor complement per NIMBUS configuration is not pinned by the
sources reviewed and should be confirmed before modelling.

## Control Stack

| Layer | Detail |
| --- | --- |
| Vendor software | VENUS (same software family as STAR/VANTAGE), plus assay-ready packaged methods |
| Host | Windows control PC |
| Documented API | Hamilton VENUS Web API where licensed |

## Device-Model Implications

The NIMBUS is the useful "small end" reference point: it proves that the *same*
vendor stack spans a slot-deck 4-channel box and a 71-track multi-arm platform.
A device model that only fits one of them is wrong.

| Proposed device | Kind tags |
| --- | --- |
| `hamilton-nimbus` | `hub`, `liquid_handler.robot` |
| `hamilton-nimbus-channel-N` | `pipette.channel` (channel variants only) |
| `hamilton-nimbus-head-96` | `pipette.head` (NIMBUS96 only) |
| `hamilton-nimbus-deck` | `deck`, `labware.host` |

Key consequence: pipetting hardware is *mutually exclusive per configuration*.
Child-device inventory must be discovered or configured, never assumed.

## Abstraction Stress Points

1. Same vendor and software, radically different deck model (SLAS positions only,
   no carriers) — deck geometry must be a pluggable strategy.
2. Channel count is 4, 8, or 96-as-one-head; the "number of pipettes" is not a
   meaningful single scalar.

## Evidence

| Evidence | Link |
| --- | --- |
| NIMBUS product page: 4 or 8 independent channels or 96 Probe Head, 11/12/20 SLAS deck positions, 96-head 1–1000 µL, ±0.1 mm | <https://www.hamiltoncompany.com/microlab-nimbus> |
| Footprint 91 × 61 × 61 cm | <https://www.hamiltoncompany.com/microlab-nimbus> |
| NIMBUS brochure (PDF; text layer not extractable in this pass) | <https://genexpress.cl/wp-content/uploads/2022/12/Microlab-NIMBUS_BR001_201704_v3.1_LR.pdf> |
| NIMBUS manuals/specifications directory entry | <https://www.labwrench.com/equipment/7379/hamilton-robotics-microlab-nimbus> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Sensors | Which of cLLD / pLLD / TADM / MAD are present per configuration, and nozzle coverage on the 96 head |
| Channel volumes | Per-channel volume range for the 4- and 8-channel variants |
| Plate handling | Whether an internal gripper/plate transport exists or whether all plate moves are external |
| Host interface | USB vs Ethernet, and whether the VENUS Web API is offered on NIMBUS |
| Deck | Whether the 11/12/20 position counts correspond to distinct enclosures or deck inserts |
