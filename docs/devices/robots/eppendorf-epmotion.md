# Eppendorf epMotion 5070 / 5073 / 5075 (and epMotion 96) — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | Eppendorf |
| Family | epMotion 5070, 5073, 5075 (with t / v / f / l suffixes for ThermoMixer, vacuum, cleanbench and large-deck variants); epMotion 96 and 96 Flex are separate manual-load 96-channel instruments |
| Robot class | Compact benchtop tool-changing liquid handler |
| Evidence quality | High for the 5070 (Eppendorf operating manual read directly); moderate for 5073/5075 variants |

## Core Architecture: A Tool Changer

The epMotion has **no permanently attached pipette**. It has a carrier with an
empty tool holder that picks up whichever dispensing tool the method needs.

| Component | Description (from the operating manual) |
| --- | --- |
| Carrier | Moves in X, Y and Z |
| Tool holder | Holds dispensing tools — the tool is acquired and released during a run |
| Optical sensor | On the carrier; detects levels, tips and labware |
| Worktable | Work surface for **tools and labware** — tools are parked on the deck like labware |
| Front hood | Safety device protecting from moving parts and contamination |
| EasyCon | Touchscreen control panel |

This is a third distinct answer to "what is the pipetting unit": Hamilton mounts
channels permanently and picks up gripper *tools*; Tecan and Beckman bolt on
typed arms/pods; Eppendorf makes the *whole pipette* a swappable tool that lives
on the deck.

## Dispensing Tools

| Tool | Channels | Volume range |
| --- | --- | --- |
| TS 50 | 1 | 1–50 µL |
| TS 300 | 1 | 20–300 µL |
| TS 1000 | 1 | 40–1000 µL |
| TM 10-8 / TM 50-8 / TM 300-8 | 8 | tool-specific, sub-1000 µL |
| Gripper | — | Labware transport tool, parked in a parking block |

Platform-level dispense range on the 5070 is 1 µL – 1000 µL, using
ep.T.I.P.S. Motion pipette tips. Dispensing tools work on the **piston-stroke
principle** and are autoclavable.

Consequence for the model: volume envelope, channel count and even the existence
of a pipette are functions of the currently mounted tool, and the mounted tool
changes mid-protocol.

## Optical Sensing

The carrier-mounted optical sensor checks three distinct things:

| Check | Meaning |
| --- | --- |
| Type and location of labware | Deck verification |
| Quantity and position of pipette tips in the rack | Consumable inventory |
| Filling level of vessels | Liquid volume estimation |

Together with the Hamilton Prep camera and Tecan DeckCheck, this is the third
independent vendor implementation of *the robot looks at its own deck*. Three
different sensors (camera, camera, optical level sensor) serving one abstract
capability: deck/consumable verification with a discrepancy result.

epBlue additionally compares the physical worktable against the software
worktable model, which is the software half of the same idea.

## Variants And Deck

| Variant | Notes |
| --- | --- |
| 5070 | Base instrument; PCR-oriented packages |
| 5070f | Identical function but must be operated inside a cleanbench; adds a **light barrier** with reflectors on the inside of the cleanbench front screen |
| 5073 | Mid-size; 5073t NGS package adds ThermoMixer, 3 dispensing tools and a gripper |
| 5075 | Larger deck; 5075t adds ThermoMixer, 5075v adds a vacuum system and gripper |
| 5075l | Quoted at 15 worktable positions |
| Accessories | Thermoadapter (heat-conductive plate holder), Thermoblock, Thermorack, reservoir racks, SafeRack (partitioned tip rack for tip reuse), waste box; UV lamp option on some configurations |

Note the thermal-accessory taxonomy: an adapter is passive-but-heat-conductive, a
block is bonded to specific labware, a rack is temperable. Deck accessories carry
thermal semantics without necessarily being independently controllable devices.

## Interfaces

| Port | Purpose |
| --- | --- |
| Ethernet (instrument) | Cable to the EasyCon control panel — **not** a general lab network port |
| USB (instrument) | USB storage medium for firmware updates only |
| EasyCon Ethernet | Connection to the epMotion |
| EasyCon USB ×4 | Mouse and USB storage |
| Power | 100–240 V, 50/60 Hz |

The control panel is a separate networked computer that owns the instrument link.
A third-party controller has no documented port to talk to: the only Ethernet
socket is already claimed by EasyCon.

## Control Stack

| Layer | Detail |
| --- | --- |
| Local software | epBlue running on EasyCon (or MultiCon PC on PC-based variants) |
| Method model | Select source and destination vessels, define the procedure, define the transfer pattern |
| External API | None identified in this pass |

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `eppendorf-epmotion` | `hub`, `liquid_handler.robot` |
| `eppendorf-epmotion-carrier` | `motion.xyz`, `tool.host` |
| `eppendorf-epmotion-tool-<slot>` | `pipette` or `pipette.head` or `labware.mover`, **present only while mounted** |
| `eppendorf-epmotion-optical-sensor` | `sensor.level`, `sensor.labware`, `sensor.tip_inventory` |
| `eppendorf-epmotion-deck` | `deck`, `labware.host`, also hosts tool parking positions |
| `eppendorf-epmotion-thermomixer` | `module.heater_shaker` |
| `eppendorf-epmotion-vacuum` | `module.vacuum` |

Capability requirements:

| Capability | Reason |
| --- | --- |
| `ToolPickup` / `ToolDrop` as protocol steps | The pipette itself is mounted and dismounted mid-run |
| Dynamic capability set | The hub's advertised pipetting capability changes with the mounted tool |
| `TipInventoryScan` | Optical counting of remaining tips in a rack |
| `VesselLevelSense` | Optical fill-level measurement, distinct from capacitive/pressure LLD |
| `DeckVerify` | Compare physical worktable to declared worktable |
| Enclosure/light-barrier interlock | 5070f safety state depends on the surrounding cleanbench |

## Abstraction Stress Points

1. The device tree changes shape during a run as tools are mounted and parked.
2. Level sensing here is **optical**, not capacitive or pressure-based — the
   abstract capability must not assume an electrode or a pressure transducer.
3. Tip inventory is measured, not merely tracked in software.
4. The instrument's only network port is consumed by its own control panel, which
   makes third-party integration a hardware problem, not just a protocol one.
5. Safety state can depend on equipment the robot does not own (the cleanbench).

## Evidence

| Evidence | Link |
| --- | --- |
| epMotion 5070 operating manual: carrier X/Y/Z, tool holder, optical sensor checks (labware type/location, tip quantity/position, vessel filling level), worktable holds tools and labware, front hood, EasyCon, Ethernet/USB interface roles, 1–1000 µL, piston-stroke dispensing tools, cleanbench light barrier, glossary (SafeRack, Thermoadapter, Thermoblock, Thermorack) | <https://bpb-us-w2.wpmucdn.com/sites.uwm.edu/dist/d/40/files/2016/05/epMotion5070_hardware-20tj0fm.pdf> |
| epMotion family overview (5070/5073/5075 packages, ThermoMixer, gripper, vacuum, NGS configurations) | <https://www.eppendorf.com/product-media/doc/en/8407472/Automated-Liquid-Handling_Overview_epMotion-5070_5073_5075_Our-epMotion-Family.pdf> |
| epMotion dispensing tools (TS 50 / TS 300 / TS 1000, TM 8-channel variants) | <https://www.eppendorf.com/fj-en/Products/Liquid-Handling/Automation-Accessories/epMotion-Dispensing-tools-p-PF-633899> |
| Dispensing tools instructions for use | <https://www.eppendorf.com/product-media/doc/en/160651_Using-Instructions/Eppendorf_Automated-Liquid-Handling_Instructions-use_epMotion-Dispensing-Tools.pdf> |
| epMotion 5075 product page | <https://www.eppendorf.com/us-en/Products/Liquid-Handling/Automated-Pipetting/epMotion5075-p-PF-8384702> |
| epMotion 5073 product page | <https://www.eppendorf.com/us-en/Products/Liquid-Handling/Automated-Pipetting/epMotion5073-p-PF-8384670> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Worktable geometry | Position counts and layout for 5070/5073/5075 and the addressing scheme |
| epMotion 96 | The 96-channel bench instruments are a different architecture and need their own note |
| External control | Whether any documented remote interface exists, or whether EasyCon is the only client |
| Optical sensor output | Whether measured levels/tip counts are readable, and their accuracy |
| Tool identification | How the instrument recognises which tool it has picked up (RFID, mechanical coding, or software declaration) |
