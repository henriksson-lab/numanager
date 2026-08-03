# Liquid Handling Robot Market Research

## Scope

This note is a market and device-model survey for automated liquid handling
robots. It is not protocol evidence for driver behavior. Protocol claims still
need manufacturer API documentation, public standards, open SDK/source, captured
traffic, or bench evidence before implementation.

Per-robot hardware inventories for the platforms listed here live in
[`robots/`](robots/README.md). That directory also carries the cross-robot
synthesis of hardware axes of variation used to design the device model.

The market does not publish a reliable public install-base ranking by robot
model. The "common platform" list below is therefore an engineering shortlist
based on repeated appearance in 2025-2026 market reports, current vendor
portfolios, long-running installed platforms, and relevance to direct-control
integration.

## Common Vendors And Platform Families

| Vendor | Common platform families | Market position signal | Direct-control notes |
| --- | --- | --- | --- |
| Hamilton | Microlab STAR, STAR V, VANTAGE, NIMBUS, Microlab Prep | Repeatedly listed among top automated liquid handling vendors; Hamilton describes its platforms as market-leading | Strong candidate for service/API integration. Hamilton publishes VENUS Web API material for devices, deck layout, method execution, status, and notifications, but access may require developer portal credentials. |
| Tecan | Fluent, Veya, Freedom EVO, EVOlyzer, D300e | Repeatedly listed among top vendors; Freedom EVO has a large legacy installed base and ended sale on December 31, 2025 | Likely service/API integration through FluentControl/EVOware. Public Fluent SiLA2 connector exists for FluentControl API access from non-.NET clients; verify supported versions and licensing. |
| Agilent | Bravo, AssayMAP Bravo, VPrep, BenchCel/Labware MiniHub integrated workcells | Repeatedly listed among top vendors; Bravo is a common compact liquid handler | Direct integration is likely through VWorks API or ActiveX controls on Windows. Agilent states VWorks API can control VWorks in the background and ActiveX may give tighter timing for supported devices. |
| Beckman Coulter Life Sciences / Danaher | Biomek i-Series, Biomek i3/i5/i7, Echo acoustic liquid handlers | Repeatedly listed through Danaher/Beckman; Biomek has decades of installed-base history | Treat as a platform family with both tip-based and acoustic dispensing. Public product pages emphasize software and many maintained device drivers, but raw control protocol is not public from the sources reviewed. |
| Thermo Fisher Scientific | Versette, Multidrop dispensers, KingFisher-adjacent automation workflows | Repeatedly listed as a top vendor | Versette appears as a compact 96/384-channel workstation with ControlMate software. Direct protocol/API evidence needs a deeper source pass. Multidrop is a dispenser class rather than a full deck robot. |
| Eppendorf | epMotion 5070/5073/5075, epMotion 96/96 Flex | Repeatedly listed as a top vendor; epMotion is a common bench liquid handler family | Likely software-mediated through epBlue/MultiCon. Public pages establish device shape, tools, volumes, accessories, and sensors, but not a raw external protocol. |
| Revvity / PerkinElmer | JANUS G3 Mini/Standard/Expanded/Integrator, MDT, Varispan | Repeatedly listed as PerkinElmer/Revvity | Large configurable workstation family. Public pages emphasize arm/head/deck configurability; direct-control API/protocol requires further evidence. |
| Opentrons | OT-2, Flex | Emerging/lower-cost platform in market reports; common in academic and small-lab automation | Direct-control path is robot-server HTTP command/status API. No protocol upload or Jupyter integration for numanager. Raw Smoothie G-code should remain diagnostic only. |
| INTEGRA Biosciences | ASSIST, ASSIST PLUS, MINI 96 | Appears in market/company lists; common in compact benchtop pipetting | Often built around electronic pipette modules and simple workflow automation. Determine whether direct control is exposed over a supported API before driver work. |
| SPT Labtech | firefly/firefly+, mosquito, dragonfly, apricot | Important genomics and dispensing specialist; appears as an emerging/specialist vendor | Product architecture often combines pipetting, dispensing, gripper, shaker, thermal modules, and cloud/protocol software. Direct-control API needs further evidence. |
| FORMULATRIX | MANTIS, TEMPEST, F.A.S.T., FLO i8 PD | Specialist vendor for low-dead-volume/non-contact dispensing and positive-displacement handling | Useful for modeling non-contact dispensing separately from pipette-axis devices. Direct-control API/protocol needs further evidence. |
| Gilson | PIPETMAX 268/278, GX-271, GX-281 | Long-standing liquid handling/robotics supplier; appears in market lists | Good representative of compact disposable-tip and larger probe/tube workflows. Direct-control API/protocol needs further evidence. |

## Likely Common Robot Classes

| Robot class | Representative platforms | Device-model implication |
| --- | --- | --- |
| Independent-channel deck liquid handler | Hamilton STAR/VANTAGE, Tecan Fluent, Beckman Biomek i-Series | Needs raw pipette/channel actuator devices plus a later deck-aware liquid-handler meta-device |
| Fixed-head 96/384 pipettor | Thermo Versette, Eppendorf epMotion 96, Revvity JANUS MDT, Tecan MCA, SPT firefly pipetting core | Needs a multi-channel head device, not just individual pipette channels |
| Compact open-deck benchtop robot | Opentrons OT-2/Flex, Agilent Bravo, Eppendorf epMotion 507x, INTEGRA ASSIST PLUS, Gilson PIPETMAX | Needs deck/labware model, pipette/head model, gripper optional, and module slots |
| Acoustic or non-contact dispenser | Beckman Echo, FORMULATRIX MANTIS/TEMPEST, SPT dragonfly | Needs dispenser/source/nozzle capabilities distinct from tip-based pipette actuation |
| Integrated genomics workstation | SPT firefly/firefly+, Beckman Biomek NGeniuS, Eppendorf NGS epMotion configs, Agilent AssayMAP Bravo | Needs liquid handling plus heater, shaker, magnet, thermocycler, gripper, and workflow-state composition |
| Plate/sample handling workcell | Agilent BenchCel/MiniHub workcells, Beckman Access workstations, Hamilton integrated devices | Needs separate labware mover, stacker/hotel, barcode, centrifuge, sealer, reader, and scheduler concepts |

## Device Types To Avoid Overfitting To OT-2

The OT-2 has two pipette mounts and a fixed deck, but many common systems have
multiple arms, 8-channel independent arms, 96/384 heads, grippers, integrated
modules, non-contact dispensers, or external scheduling software. The core model
should therefore separate these concepts:

| Proposed kind tag | Meaning | Examples |
| --- | --- | --- |
| `liquid_handler.robot` | Physical liquid-handling platform/hub | OT-2, Bravo, STAR, Fluent, Biomek, epMotion |
| `pipette` | Single pipette or raw pipette actuator | OT-2 left/right pipette, single-channel tools |
| `pipette.channel` | Independent channel on a multi-channel arm | Hamilton/Tecan independent channel arms |
| `pipette.head` | Fixed multi-channel head | 96/384 heads on JANUS MDT, Versette, firefly |
| `dispense.head` | Bulk/non-contact/acoustic dispenser head | Echo, MANTIS, TEMPEST, dragonfly |
| `deck` | Coordinate and occupancy space | SBS deck slots, carriers, moving decks |
| `labware.host` | Owns labware definitions, offsets, barcode identities, and occupancy | Deck manager, worktable software |
| `labware.mover` | Gripper or plate mover | Fluent/RoMa-like arms, firefly gripper, BenchCel |
| `module.temperature` | Temperature module or thermoblock | OT-2 Temperature Module, deck thermal modules |
| `module.thermocycler` | Thermocycler module | OT-2 Thermocycler, firefly+ thermocycler |
| `module.magnetic` | Magnetic bead module | OT-2 Magnetic Module, magnetic plates/accessories |
| `module.heater_shaker` | Combined heater-shaker module | OT-2 Heater-Shaker, integrated plate shaker/heater |
| `module.shaker` | Shaker without thermal control or as sub-function | Plate shaker accessories |
| `liquid_handler` | Candidate meta-device coordinating motion, pipette, deck, labware, tips, and modules | Transfer, mix, pick-up-tip, drop-tip, touch-tip |

## Capability Gaps

| Capability | Needed for | Notes |
| --- | --- | --- |
| `PipetteActuate` | Raw aspirate, dispense, blow-out, pressure/liquid-sense readback | Lower-level than transfer or tip pickup; should not require deck orchestration; reset-like maintenance actions stay outside the public command surface |
| `PipetteHeadActuate` | 8/96/384-channel head operations | Must represent selected channels/nozzles, head type, and volume constraints |
| `DispenseHead` | Non-contact/acoustic/bulk dispensing | Different from pipette aspiration because source reservoirs/nozzles may be fixed |
| `MagneticControl` | Engage/disengage magnet and height | New first-class module capability |
| `HeaterShakerControl` | Set shake speed, latch state, heater state, deactivate | New first-class module capability; safety interlocks matter |
| `LabwareMove` | Grippers, stackers, hotels, plate movers | Needed for larger workcells and integrated genomics systems |
| `LiquidHandlingPlan` or meta-device commands | Pick up tip, drop tip, transfer, mix, touch tip | Belongs above raw devices because it uses deck geometry, consumables, calibration, collision handling, and recovery |

## Integration Boundary Pattern

Most commercial liquid handlers should be integrated at the vendor-supported
software/API layer, not by analyzing motor buses:

| Vendor | Likely first integration boundary |
| --- | --- |
| Hamilton | VENUS Web API or Hamilton API |
| Tecan | FluentControl/EVOware API; SiLA2 connector where supported |
| Agilent | VWorks API or device ActiveX controls |
| Opentrons | robot-server HTTP command/status API |
| Beckman | Vendor software/API if available; protocol source not yet identified |
| Eppendorf | epBlue/MultiCon software boundary if an API exists |
| Revvity | JANUS software/API boundary if available |
| Thermo Fisher | ControlMate or instrument software boundary if API exists |

For numanager, this suggests a common architecture:

1. A vendor hub/resource owns the vendor service connection.
2. Raw child devices expose typed actuator/status capabilities.
3. A composed workflow layer coordinates child devices for liquid-handling workflows.
4. Protocol uploads and vendor method execution are optional vendor-workflow
   features, not the core numanager path.

## Evidence Links

| Evidence | Link |
| --- | --- |
| Market report company list: Thermo Fisher, Hamilton, PerkinElmer, Tecan, Agilent plus broader company list | <https://www.mordorintelligence.com/industry-reports/automated-liquid-handling-system-market/companies> |
| 2026 market report naming Agilent, Beckman/Danaher, Eppendorf, Hamilton, Revvity/PerkinElmer, Tecan, Thermo Fisher and others | <https://www.researchandmarkets.com/report/automated-liquid-handler> |
| Grand View 2026-2033 report naming major liquid-handling technology companies | <https://www.grandviewresearch.com/industry-analysis/automated-liquid-handling-alh-technology-market> |
| Hamilton automated liquid handling platforms | <https://www.hamiltoncompany.com/automated-liquid-handling> |
| Hamilton VENUS software and REST API | <https://www.hamiltoncompany.com/venus> |
| Hamilton VENUS Web API device/deck endpoints | <https://developer.hamiltoncompany.com/products/venus/api/openapi/devices> |
| Tecan Fluent platform | <https://www.tecan.com/fluent-automated-workstation> |
| Tecan Freedom EVO legacy platform and end-of-sale note | <https://lifesciences.tecan.com/freedom-evo-platform> |
| Tecan Fluent SiLA2 connector | <https://gitlab.com/tecan/fluent-sila2-connector> |
| Agilent automated liquid handling portfolio | <https://www.agilent.com/en/product/automated-liquid-handling> |
| Agilent Bravo platform | <https://www.agilent.com/en/product/automated-liquid-handling/automated-liquid-handling-platforms/bravo-automated-liquid-handling-platform%3Fsrsltid%3DAfmBOoqHpjILQXDoM4zPaS7hLtk3x0v6A0jMHCteanjjeWWHsZh0BBi_> |
| Agilent VWorks third-party integration article | <https://community.agilent.com/knowledge/automated-liquid-handling-portal/kmp/automated-liquid-handling-articles/kp1619.integrating-agilent-automated-liquid-handling-systems-with-third-party-systems> |
| Beckman Biomek i-Series | <https://www.beckman.com/liquid-handlers/biomek-i-series-automated-workstations> |
| Beckman liquid handlers and Echo acoustic handlers | <https://www.beckman.com/liquid-handlers> |
| Thermo Fisher liquid handling portfolio | <https://www.thermofisher.com/de/en/home/life-science/lab-equipment/liquid-handling.html> |
| Thermo Fisher Versette FAQ | <https://www.thermofisher.com/order/catalog/product/650-INSTR/faqs> |
| Eppendorf automated liquid handling | <https://www.eppendorf.com/en/own-your-solution/automated/> |
| Eppendorf epMotion 5073 | <https://www.eppendorf.com/us-en/Products/Liquid-Handling/Automated-Pipetting/epMotion5073-p-PF-8384670> |
| Revvity JANUS workstations | <https://www.revvity.com/gb-en/category/janus-workstations> |
| INTEGRA pipetting robots | <https://www.integra-biosciences.com/united-states/en/pipetting-robots> |
| SPT Labtech firefly | <https://www.sptlabtech.com/products/firefly> |
| FORMULATRIX liquid handling systems | <https://formulatrix.com/liquid-handling-systems/> |
| Gilson sample handling automation | <https://www.gilson.com/default/sample-handling> |
