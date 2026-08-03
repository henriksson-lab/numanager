# Hamilton Microlab Prep — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | Hamilton Company |
| Family | Microlab Prep (entry-level benchtop) |
| Robot class | Compact enclosed benchtop liquid handler with an on-board touchscreen and vision-based labware recognition |
| Evidence quality | Good for physical setup facts (vendor welcome packet), moderate for pipetting internals |

## Physical And Deck

| Item | Value |
| --- | --- |
| Deck | 8 ANSI/SLAS sites; site numbering places 4 and 8 at the front |
| Base plates | Per-site base plates are removable to accommodate 1000 µL tips, tube pedestals, or semi-skirted PCR plates |
| Width (with touchscreen) | 572 mm |
| Depth (with touchscreen) | 636 mm |
| Height (door closed / open) | 604 mm / 809 mm |
| Weight | 41.6 kg |
| Power | 100–240 VAC, 50–60 Hz |
| Environment | 15–35 °C, 15–85 % RH non-condensing |
| Lighting requirement | Must be consistently well-lit without shadows, because labware recognition is camera-based |

That last row is unusual and worth carrying into the model: this robot has an
*environmental precondition for its perception system*, which is a real failure
mode a driver may have to surface.

## Pipetting Hardware

| Configuration | Hardware |
| --- | --- |
| Standard | 2 independent pipetting channels |
| Head | 8-probe high-speed multi-probe head |
| Combined | 2 independent channels **plus** the 8-probe head on the same instrument |

| Item | Value |
| --- | --- |
| Volume range | 0.5–1000 µL |
| Tip attachment | CO-RE II |
| Consumable wear tracking | The instrument counts tip-eject cycles per pipetting unit and recommends CO-RE II O-ring replacement at 40 000 ejects |

The tip-eject counter is a genuinely useful modelling detail: it is a
maintenance-state property owned by each pipetting unit, not by the robot.

## Perception And Labware Handling

| Item | Detail |
| --- | --- |
| Deck camera | Top-mounted camera detects and identifies labware on the deck; used both to speed protocol authoring and to verify labware type and placement at run time |
| Barcode reader | Handheld USB barcode scanner accessory, connected after setup |
| On-deck transport | Optional CO-RE paddles let the channels move sample vessels around the deck and to optional peripherals |

## Control Stack And Interfaces

| Layer | Detail |
| --- | --- |
| Local UI | Integrated touchscreen with graphical protocol software |
| Host ports | USB ports on the lower left side of the instrument |
| Ethernet | Not documented in the welcome packet reviewed |
| Vendor API | No published external control API found for Prep in this pass |

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `hamilton-prep` | `hub`, `liquid_handler.robot` |
| `hamilton-prep-channel-1/2` | `pipette.channel` |
| `hamilton-prep-head-8` | `pipette.head` |
| `hamilton-prep-camera` | `camera.snapshot`, `inspection.camera`, `labware.identifier` |
| `hamilton-prep-barcode` | `barcode.reader` |
| `hamilton-prep-deck` | `deck`, `labware.host` |

Notable model requirements this instrument introduces:

| Requirement | Reason |
| --- | --- |
| An 8-nozzle head kind | Between "single pipette" and "96 head"; a binary single-vs-96 taxonomy fails |
| Channels and a head coexisting | The combined configuration has both on one small instrument |
| Vision-derived labware identity | Deck occupancy can be *observed*, not only declared |
| Consumable/maintenance counters | Tip-eject cycles per pipetting unit with a vendor-defined threshold |

## Abstraction Stress Points

1. A liquid handler may know what labware is on its deck by looking at it. Deck
   occupancy is a readback, not just configuration.
2. Perception has environmental preconditions that can fail independently of
   motion or fluidics.
3. Self-reported maintenance state (eject counts, O-ring life) belongs in the
   device property surface.

## Evidence

| Evidence | Link |
| --- | --- |
| Welcome packet: 8 deck sites with front sites 4 and 8, removable base plates, dimensions, 41.6 kg, 100–240 VAC, USB ports, lighting requirement | <https://info.hamiltoncompany.com/view/414061399> |
| Product page and press release: 2 independent channels, 8-probe head, both together, 0.5–1000 µL, CO-RE, deck camera, CO-RE paddles, 8 SLAS positions | <https://www.hamiltoncompany.com/microlab-prep> |
| Press release with deck capacity and pipetting range | <https://www.hamiltoncompany.com/press-releases/new-microlab-prep-automated-liquid-handler-provides-high-end-performance-for-any-budget> |
| Tip-eject cycle threshold (40 000) and camera labware recognition | <https://www.hamiltoncompany.com/knowledge-base/automated-liquid-handling/microlab-prep> |
| Handheld barcode scanner accessory | <https://www.hamiltoncompany.com/other-robotics/6603432-01> |

## Open Questions

| Area | Unknown |
| --- | --- |
| External control | Whether any documented external API, USB protocol, or network interface exists for third-party control |
| Sensors | Whether cLLD / pressure monitoring is present on Prep channels |
| Head geometry | Whether the 8-probe head has fixed 9 mm pitch or variable spacing |
| Camera access | Whether camera frames or labware-recognition results are exposed outside the vendor UI |
