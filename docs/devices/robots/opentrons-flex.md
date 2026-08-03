# Opentrons Flex — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | Opentrons |
| Family | Opentrons Flex |
| Robot class | Enclosed compact open-deck robot; successor architecture to the OT-2 |
| Evidence quality | High. Opentrons publishes an open system-description manual and the full stack is open source. |
| Related | [`../opentrons-ot2.md`](../opentrons-ot2.md) is the existing numanager driver page for the OT-2 |

The Flex is the most documentable robot in this survey — Opentrons publishes
mechanical details, an HTTP API, and the entire control stack as source. It is a
good bring-up target *and* a good reference for what a well-specified device
model looks like.

## Deck

| Item | Detail |
| --- | --- |
| Working slots | 12 ANSI/SLAS slots addressed as a coordinate grid, A1 through D3 (rows A–D, columns 1–3) |
| Staging area | An additional column 4 on the right side, outside the working area, used for storage |
| Slot interchangeability | Slots are interchangeable within a column but **not** across columns |
| Fixtures | Deck positions can be reconfigured to hold labware, modules, and consumables |
| Modules | Heater-Shaker, Temperature Module, Thermocycler, Magnetic Block |

Named-grid addressing (A1…D3) with a distinct staging region is a cleaner deck
model than either an integer slot list or a track/carrier space, and it maps
directly onto a `(row, column, region)` addressing scheme.

## Gantry And Mounts

| Item | Detail |
| --- | --- |
| Axes | X, Y, Z gantry, precise to 0.1 mm |
| Motors | 36 VDC hybrid bipolar stepper motors |
| Mounts | Left pipette mount, right pipette mount, and a **separate extension mount** for the gripper |
| Concurrency | The gripper is on its own mount, so it can be used with any pipette configuration |

Three mounts of two different kinds on one gantry is the key structural fact: the
Flex separates "tool position" from "tool type" explicitly in hardware.

## Pipettes

| Pipette | Channels | Documented capacity | Mounts occupied |
| --- | --- | --- | --- |
| Flex 1-Channel 50 µL | 1 | 1–50 µL | 1 |
| Flex 1-Channel 1000 µL | 1 | 5–1000 µL | 1 |
| Flex 8-Channel 50 µL | 8 | 1–50 µL | 1 |
| Flex 8-Channel 1000 µL | 8 | 5–1000 µL | 1 |
| Flex 96-Channel | 96 | documented as 1–200 µL and 5–1000 µL depending on tips | **both** pipette mounts |

Tip pickup on the 96-channel pipette is mechanically inverted: the pipette lowers
onto mounting pins and **lifts the adapter and tip rack** to pull tips on, rather
than pressing down onto them, to get the required leverage without warping the
deck.

That is a concrete example of why "pick up tip" cannot be a single generic motion
primitive — the same logical operation is implemented by opposite forces on
different hardware.

## Gripper

| Item | Detail |
| --- | --- |
| Mount | Extension mount, independent of pipette mounts |
| Mechanism | Two parallel paddles driven by a 36 VDC brushed motor through a rack-and-pinion |
| Motion | Uses the gantry: lift on Z, move laterally, lower to place |
| Labware | Certain fully skirted well plates, lids, and tip racks |
| Calibration | Magnetic calibration pin used during setup to measure precise positioning |
| Rotation | Not documented |

## Perception, UI And Status

| Item | Detail |
| --- | --- |
| Camera | Mounted on the interior frame in the upper corner of the enclosure near the front door |
| Touchscreen | 7-inch LCD on the front right |
| Status light | LED strip giving visual status |
| Enclosure | Front door; the robot is fully enclosed |

## Control Stack

| Layer | Detail |
| --- | --- |
| Primary boundary | robot-server HTTP/JSON API, as on the OT-2 |
| Internal | Opentrons' own motion/hardware controller stack (open source), CAN-based on Flex rather than the OT-2's Smoothie UART |
| Local UI | On-robot touchscreen |
| Openness | Full source available, including hardware controllers and labware definitions |

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `opentrons-flex` | `hub`, `liquid_handler.robot`, `network.http` |
| `opentrons-flex-gantry` | `motion.xyz` |
| `opentrons-flex-mount-left` / `-right` / `-extension` | `mount` — a position that may be empty or occupied |
| `opentrons-flex-pipette-*` | `pipette` or `pipette.head` (96-channel) |
| `opentrons-flex-gripper` | `labware.mover` |
| `opentrons-flex-deck` | `deck`, `labware.host` (A1–D3 plus staging column 4) |
| `opentrons-flex-module-*` | `module.temperature`, `module.thermocycler`, `module.magnetic`, `module.heater_shaker` |
| `opentrons-flex-camera` | `camera.snapshot` |

Capability requirements:

| Capability | Reason |
| --- | --- |
| Mount inventory with occupancy | A mount is a first-class addressable position, separate from what occupies it |
| Multi-mount instruments | The 96-channel pipette consumes two mounts |
| Grid-plus-staging deck addressing | (row, column) with a non-working region |
| Tip-pickup strategy metadata | Push versus lift-adapter pickup is hardware-specific |
| Gripper calibration state | Placement accuracy depends on a calibration artefact |

## Abstraction Stress Points

1. An instrument can occupy more than one mount, so mounts and instruments are
   not 1:1.
2. Deck slots are not fungible — column membership constrains what can go where.
3. Some deck area is explicitly non-working (staging), so "has labware" and "is
   reachable for pipetting" are different properties.
4. Tip acquisition mechanics differ per instrument in ways that affect motion
   planning and failure modes.

## Evidence

| Evidence | Link |
| --- | --- |
| Flex robot system description: 12 A1–D3 slots, staging column 4, gantry X/Y/Z to 0.1 mm, 36 VDC steppers, mounts plus extension mount, camera location, 7-inch touchscreen, status LED, supported modules | <https://docs.opentrons.com/flex/system-description/robot/> |
| Flex pipettes: models, capacities, mount occupancy, 96-channel lift-adapter tip pickup | <https://docs.opentrons.com/flex/system-description/pipettes/> |
| Flex gripper: extension mount, parallel paddles, 36 VDC brushed motor with rack and pinion, gantry-driven motion, labware compatibility, magnetic calibration pin | <https://docs.opentrons.com/flex/system-description/gripper/> |
| Opentrons HTTP API reference | <https://docs.opentrons.com/http/api_reference.html> |
| Opentrons open-source stack | <https://github.com/Opentrons/opentrons> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Connectivity | Ethernet / USB / Wi-Fi port inventory is not in the robot system-description page reviewed |
| Gripper force | Jaw force, travel, payload, and whether force feedback is readable |
| Internal bus | The CAN/hardware-controller layer needs source review before being treated as an integration option |
| Camera access | Whether Flex exposes a snapshot endpoint equivalent to the OT-2's |
| Deck fixtures | Full fixture catalogue (waste chute, trash bin, module cutouts) and their addressing |
