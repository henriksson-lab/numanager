# Tecan Fluent 480 / 780 / 1080 — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | Tecan |
| Family | Fluent Automation Workstation: Fluent 480, 780, 1080 |
| Robot class | Multi-arm deck liquid handler with interchangeable arm types |
| Evidence quality | High. Manufacturer specification sheet (Tecan doc 398328 V2.5) read directly. |

## Platform Variants

| Model | Robotic arms | Width | Depth | Height (standard Z / long Z) | Plate/tip-box capacity | DeckCheck cameras |
| --- | --- | --- | --- | --- | --- | --- |
| Fluent 480 | 1–2 | 1150 mm | 785 mm | 1236 mm / 2301 mm | 30 | 1 |
| Fluent 780 | 1–3 (dual FCA possible) | 1650 mm | 785 mm | 1236 mm / 2301 mm | 48 | 3 |
| Fluent 1080 | 1–3 (dual FCA possible) | 2150 mm | 785 mm | 1236 mm / 2301 mm | 72 | 3 |

Arm precision: ±0.1 mm on X, Y and Z. Power 100–240 VAC 50/60 Hz. Noise < 60 dBA.
Operating range 15–32 °C, 30–80 % RH.

The "long Z" height variant is a hardware option that changes the vertical
envelope of the whole instrument — reach below the worktable is a configuration
property, not a constant.

## Arm Types

An arm is the unit of configuration. Any Fluent carries 1–3 arms chosen from:

### FCA — Flexible Channel Arm

| Property | Value |
| --- | --- |
| Channels | 8 pipetting channels |
| Z motion | Independent Z per channel |
| Y motion | Automatic tip spacing from 9 mm to 38 mm |
| Pipetting systems | Liquid displacement **or** air displacement (choose at configuration time) |
| Disposable tips (DiTi) | 10, 50, 200, 1000, 5000 µL with or without filters; 10 and 350 µL nested without filters |
| Fixed tips | Standard, low-volume 384-well, Te-PS, and piercing tips (liquid-displacement system only) |
| Rapid Wash | Diaphragm-pump wash delivery (liquid-displacement system only) |
| FCA gripper | A gripper tool **picked up by the disposable-tip channels**; max lift 0.4 kg |
| Tip ejection | Contained ejection to prevent aerosols; also used for tip re-racking |

### MCA — Multiple Channel Arm

| Variant | Volume range | Tip formats |
| --- | --- | --- |
| MCA 96 | 1–1000 µL free dispense | 50, 100, 150, 200, 500 µL in 96-well format, with and without filters |
| MCA 384/96 | 250 nL – 500 µL | 15, 50, 125 µL in 384 format; automatic exchange between 384- and 96-tip formats; disposable- or fixed-tip adapters |

MCA 96 has **disposable tips with individual liquid detection** and an optional
plate gripper with a Finger Exchange System.

### RGA — Robotic Gripper Arm

| Property | Value |
| --- | --- |
| Z axes | Standard (335 mm vertical range) or long (645 mm) |
| Gripper heads | Regular, or automatic Finger Exchange System head |
| Finger types | Eccentric, long-eccentric, centric, tube |
| Gripper range | 74–135 mm (plate fingers); 8–60 mm (tube fingers) |
| Access below worktable | Standard Z: eccentric 80 mm, centric 137 mm. Long Z: eccentric 385 mm, centric 438 mm |
| Offset eccentric → centric | 53 mm horizontal, 152 mm vertical |
| Max transportable weight | 0.45 kg (eccentric fingers) |
| Barcode | Optional barcode reader on the arm |

The RGA is the clearest case in this survey of a *gripper whose reachability and
payload depend on which fingers are installed*. Finger type is a swappable
sub-component with its own geometry.

## Sensing And Process Control

| Feature | Detail |
| --- | --- |
| Liquid level detection | FCA detects down to 2 µL aqueous, 3 µL deionized water, or 10 µL ethanol in a 96-well skirted PCR plate with DiTi 10; determines presence of sufficient liquid; performs liquid-arrival check |
| Aspiration supervision | FCA real-time aspiration monitoring, tip-diving prevention, tip-occlusion detection |
| Tip state | Detection of disposable tip pickup and ejection |
| MCA 96 LLD | Individual liquid detection per disposable tip |
| Fluent ID | High-capacity barcode scanning of tubes; plate scanning |
| DeckCheck | Camera system that assesses the actual deck layout at run time, compares it to the expected layout, and highlights discrepancies. 1 camera on Fluent 480, 3 on 780/1080 |
| Safety | Door sensors on the safety screen support user-activated Active Stop and Resume; optional door locks |

Barcode support: Code 128 (recommended), Code 39, Codabar, Interleaved 2 of 5.
Tube barcodes ≥ 6.6 mil density, ≥ 8 mm height, ≤ 80 mm length, ≤ 64 digits;
plate barcodes ≥ 3 mil, ≥ 5 mm height, ≤ 74 digits. Fluent ID is a Class 2 laser
product where fitted.

DeckCheck is important for the abstraction: deck state can be *measured* and can
disagree with the declared layout, so a deck device needs both an expected model
and an observed model plus a discrepancy report.

## Control Stack And Interfaces

| Layer | Detail |
| --- | --- |
| Control PC | Windows 10 Enterprise LTSC 2019; i7-class CPU, ≥32 GB RAM, 512 GB SSD; **NVIDIA GPU with ≥5 GB RAM and CUDA support required** (DeckCheck vision); two GigE NICs (one external network, one for DeckCheck); one USB port for the instrument |
| Vendor software | FluentControl; Fluent Gx Assurance Software for regulated environments; Introspect dashboards |
| Instrument link | USB from the control PC |
| Third-party API | Tecan publishes a Fluent SiLA2 connector on GitLab that exposes the FluentControl API to non-.NET clients |

The GPU/second-NIC requirement is a real deployment constraint: the vision
subsystem is a networked camera pipeline living beside the instrument link.

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `tecan-fluent` | `hub`, `liquid_handler.robot` |
| `tecan-fluent-fca` | `motion.arm`, `pipette.arm` |
| `tecan-fluent-fca-channel-N` | `pipette.channel` |
| `tecan-fluent-mca96` / `-mca384` | `pipette.head` |
| `tecan-fluent-rga` | `labware.mover`, `motion.arm` |
| `tecan-fluent-rga-fingers` | `tool.gripper_fingers` (geometry-bearing sub-component) |
| `tecan-fluent-deckcheck` | `camera.inspection`, `deck.verifier` |
| `tecan-fluent-id` | `barcode.reader` |
| `tecan-fluent-deck` | `deck`, `labware.host` |

Capability requirements this platform adds:

| Capability | Reason |
| --- | --- |
| Pipetting-system mode as a static property | Liquid displacement vs air displacement changes available tips, wash behaviour, and volume envelope |
| Tip-spacing control | 9–38 mm automatic spread is a commanded parameter |
| Format-switching head | MCA 384/96 changes its own nozzle format at run time |
| Gripper finger inventory | Reach, grip range, payload, and below-deck access depend on installed fingers |
| Deck verification | DeckCheck compare/report is a distinct operation from reading occupancy |

## Abstraction Stress Points

1. Up to three heterogeneous arms per robot, chosen freely — topology is
   configuration.
2. A head can change its own nozzle format (384 ↔ 96) during a run.
3. Gripper geometry is a property of a removable finger set.
4. The instrument's vision system needs its own network path and GPU.
5. Reach below the worktable is a hardware variant (standard vs long Z).

## Evidence

| Evidence | Link |
| --- | --- |
| Fluent specification sheet 398328 V2.5: arm counts, FCA/MCA/RGA detail, tip sizes, LLD limits, DeckCheck, barcodes, PC requirements, dimensions | <https://www.triolab.no/media/mrbfucx2/ss_fluent-specification-sheet_398328-v2-4.pdf> |
| Same specification sheet, alternate host | <https://www.insideprecisionmedicine.com/wp-content/uploads/2023/08/BR_Fluent-Specification-Sheet_398328.pdf> |
| Fluent product page | <https://www.tecan.com/fluent-automated-workstation> |
| Fluent Reference Manual 399937 (not yet mined in this pass) | <https://www.tecan.com/hubfs/399937_en_v1_7.pdf> |
| Fluent SiLA2 connector | <https://gitlab.com/tecan/fluent-sila2-connector> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Deck geometry | Carrier/grid pitch and coordinate origin are not in the specification sheet; the Fluent Reference Manual should have the worktable grid definition |
| Command surface | Which FluentControl API / SiLA2 features expose per-channel actuation versus script execution only |
| Sensor payloads | Whether aspiration-supervision traces are retrievable, or only pass/fail flags |
| DeckCheck output | Whether images and discrepancy reports are exposed outside FluentControl |
| Arm collision model | How multiple arms are arbitrated and whether that arbitration is visible to an external client |
