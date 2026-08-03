# Beckman Coulter Biomek i-Series (i5 / i7) — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | Beckman Coulter Life Sciences (Danaher) |
| Family | Biomek i5, Biomek i7, Biomek i7 Hybrid; NGeniuS and assay-specific derivatives |
| Robot class | Single- or dual-**pod** deck liquid handler with active deck positions |
| Evidence quality | Moderate. Vendor product pages and reseller configuration listings; no Beckman service manual reviewed. |

## The Pod Model

Beckman's unit of configuration is the **pod** — a mountable pipetting assembly on
the gantry. A Biomek carries one or two pods, and the pod type determines the
whole pipetting personality of that arm.

| Pod type | Hardware |
| --- | --- |
| Multichannel (MC) pod | 96-channel or 384-channel head, or a pin tool |
| Span-8 pod | 8 independently positionable probes with independent Z and variable spacing |
| Hybrid configuration | One MC pod **and** one Span-8 pod on the same instrument (Biomek i7 Hybrid) |

| Model | Pods | Deck positions |
| --- | --- | --- |
| Biomek i5 | Single pod (MC or Span-8) | ~25 (configuration-dependent) |
| Biomek i7 | Single or dual pod | up to 45 with enclosure |
| Biomek i7 Hybrid | MC + Span-8 | up to 45 with enclosure |

Base unit without enclosure: approximately 67 in W × 32 in D × 41 in H.

"Pod" is the abstraction Beckman chose for the same problem Tecan solved with
"arm" and Hamilton with "arm + tools". All three agree that the pipetting unit is
mountable, typed, and pluralisable.

## Pipetting Hardware

| Item | Detail |
| --- | --- |
| Platform volume envelope | 0.5 µL to 5000 µL across configurations |
| Span-8 probes | 8 independent probes; transfers up to 1000 µL; independent probe calibration |
| Multichannel head | 96-channel head handling volumes up to ~1070 µL; 384-channel head available |
| Tips | Both disposable and fixed Biomek tips, including septum-piercing tips |
| Labware range | Tubes through 1536-well microplates |
| Intra-well pipetting | Supported (positioning within a single well) |

Septum-piercing tips are worth noting: tip type changes what the pipette may
physically do to labware, which is a safety-relevant property, not cosmetics.

## Sensing

| Feature | Detail |
| --- | --- |
| Liquid level sensing | 8-channel liquid-level sensing on the Span-8 pod |
| Probe calibration | Independent per-probe calibration |
| Enclosure | Optional enclosure protecting samples from airborne particles; temperature-controlled variants quoted at 10–30 °C |

No pressure-monitoring/clot-detection equivalent to Hamilton TADM or Tecan PMP is
documented in the sources reviewed. Treat that as unknown rather than absent.

## Grippers And Labware Movement

| Item | Detail |
| --- | --- |
| Gripper | Gripper integrated with a pod; dual-gripper configurations allow simultaneous plate transfers across the deck |
| Reach | Grippers serve on-deck positions and hand off to integrated off-deck devices |

## Deck: ALPs

The Biomek deck is built from **Automated Labware Positioners (ALPs)** — deck
modules that occupy positions and may be passive (a plate holder) or active
(heating, cooling, shaking, washing, tip loading, waste, and similar).

This is the cleanest vendor formulation of an idea numanager needs: *a deck
position is a typed slot that may itself be a device.* The same physical
coordinate can be an inert platepad on one instrument and a peltier module on the
next.

## Control Stack

| Layer | Detail |
| --- | --- |
| Vendor software | Biomek Software; SAMI EX for scheduling multi-device systems |
| Device drivers | Beckman advertises a large maintained catalogue of drivers for integrated third-party devices |
| Documented external API | None identified in this pass |
| Protocol | Not public from the sources reviewed |

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `beckman-biomek` | `hub`, `liquid_handler.robot` |
| `beckman-biomek-pod-N` | `motion.arm`, `pipette.pod` — typed MC or Span-8 |
| `beckman-biomek-span8-probe-N` | `pipette.channel` |
| `beckman-biomek-mc-head` | `pipette.head` |
| `beckman-biomek-gripper-N` | `labware.mover` |
| `beckman-biomek-deck` | `deck`, `labware.host` |
| `beckman-biomek-alp-<pos>` | `deck.position` plus the ALP's own kind (`module.temperature`, `module.shaker`, `module.wash`, …) |

Capability requirements:

| Capability | Reason |
| --- | --- |
| Pod-typed device discovery | An instrument's capabilities depend on which pods are fitted |
| Per-probe calibration state | Span-8 probes are individually calibrated |
| Tip-type semantics including piercing | Physical interaction with labware differs by tip |
| Deck position as device | ALPs make deck slots addressable devices |
| Dual-gripper concurrency | Two grippers can move plates simultaneously |

## Abstraction Stress Points

1. Deck positions are not passive coordinates — they can be active hardware with
   their own state and commands.
2. One instrument can hold two structurally different pipetting units at once.
3. Volume envelope (0.5 µL – 5 mL) spans two orders of magnitude across
   configurations, so a per-robot volume range is meaningless; it must be
   per-pod/per-tip.
4. Vendor stack is closed: integration is likely to be through Beckman software,
   which pushes numanager toward a method/step boundary rather than actuators.

## Evidence

| Evidence | Link |
| --- | --- |
| Biomek i-Series overview: single/dual pipetting head models combining multichannel (96 or 384) and Span-8, ALPs, 0.5 µL – 5000 µL, disposable and fixed tips including septum-piercing, 8-channel liquid-level sensing, tubes to 1536-well | <https://www.beckman.com/liquid-handlers/biomek-i-series-automated-workstations> |
| Biomek i7: up to 45 deck positions, dual grippers, enclosure | <https://www.beckman.com/liquid-handlers/biomek-i7> |
| Biomek i5 | <https://www.beckman.com/liquid-handlers/biomek-i-series-automated-workstations/biomek-i5> |
| i7 Hybrid (MC + Span-8) configuration and dimensions | <https://www.beckman.com/liquid-handlers/biomek-i-series-automated-workstations/biomek-i7/b87585> |
| MC 96-channel head up to ~1070 µL, Span-8 up to 1000 µL (reseller listing) | <https://www.bostonind.com/beckman-biomek-i7-hybrid-liquid-handler-w-span8,-multi-96-and-dual-grippers> |

## Open Questions

| Area | Unknown |
| --- | --- |
| ALP catalogue | The full list of active ALPs and whether each has an independent control interface |
| Sensing | Whether pressure/clot monitoring exists on Span-8 or MC pods |
| External control | Whether Biomek Software or SAMI exposes any documented API, and at what granularity |
| Protocol | No public wire protocol; would require vendor documentation or captured traffic |
| Pod exchange | Whether pods are user-swappable in the field and how the software discovers the fitted configuration |
