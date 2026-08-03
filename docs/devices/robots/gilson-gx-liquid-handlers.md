# Gilson GX-271 / GX-281 (VERITY) Liquid Handlers — Hardware Note

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | Gilson |
| Family | GX-271, GX-281; also sold under the VERITY name |
| Robot class | X/Y/Z single-probe liquid handler and autosampler for chromatography workflows |
| Evidence quality | Moderate–good. Gilson product pages plus a public GX-271 user guide (not mined page-by-page in this pass). |

## Why This Machine Is In The Survey

The GX series is a **tube-and-flow-path** liquid handler, not a plate-and-tip
one. It exists to inject samples into an HPLC and to collect fractions from it.
It shows what the abstraction must accommodate at the boundary between liquid
handling and analytical chemistry.

| Item | GX-271 | GX-281 |
| --- | --- | --- |
| Capacity | Small footprint, medium capacity | Large capacity |
| Dimensions | 59.1 × 54.2 × 61.0 cm | larger |
| Positional accuracy | better than 0.2 mm | — |
| Role | Injection, fraction collection, liquid handling | Injection, fraction collection, re-injection in semi-prep/prep HPLC |

## Fluidic Architecture

The probe is a needle on an X/Y/Z arm, permanently plumbed to a pump and a valve
network. There is no tip, no tip waste, and no aspirate-into-a-disposable step.

| Component | Detail |
| --- | --- |
| Pumping | GX Solvent System (syringeless) **or** a 402 Syringe Pump |
| GX Solvent System range | 2 µL – 100 mL, flow rates 1 µL/min – 25 mL/min |
| Platform volume/flow envelope | 50 µL to hundreds of mL; flow rates up to 50 mL/min |
| Direct Injection Module | Injection port integrated into the valve; continuous-flow path supporting flow rates up to 200 mL/min |
| Fraction collection valve | Directs column effluent to collection vessels |
| Racks | Modular; Code 20, 200, or 34X series racks |

## Device-Model Implications

| Proposed device | Kind tags |
| --- | --- |
| `gilson-gx` | `hub`, `liquid_handler.robot`, `autosampler` |
| `gilson-gx-arm` | `motion.xyz` |
| `gilson-gx-probe` | `probe.needle`, `fluidics.port` |
| `gilson-gx-pump` | `pump.syringe` or `pump.solvent` |
| `gilson-gx-injection-valve` | `valve.injection` |
| `gilson-gx-fraction-valve` | `valve.diverter` |
| `gilson-gx-rack-N` | `labware.rack` (vendor rack codes) |

Capability requirements:

| Capability | Reason |
| --- | --- |
| `ValveSwitch` with named ports | Injection and fraction-collection valves are the core actuators |
| `PumpFlow` with rate and volume | Flow-rate control, not just volume transfer |
| Probe wash / needle rinse | Carry-over management replaces tip disposal |
| Rack-type labware model | Tube racks with vendor-defined codes, not SBS plates |
| Chromatography-system context | The robot is one node in a flow path shared with pumps, columns and detectors |

numanager already has a `fluidics` example and a Hamilton MVP valve driver, so
valve and pump abstractions exist in the codebase; the GX series is where those
meet the liquid-handling model.

## Abstraction Stress Points

1. Liquid handling can be **flow-based**: volume and flow rate matter more than
   plunger position, and the fluid path is shared with other instruments.
2. Carry-over is managed by washing a permanent needle, not by discarding tips.
3. Labware is racks of tubes and vials, addressed by vendor rack codes.
4. The instrument's job may be to hand liquid to another instrument (an HPLC),
   so its "destination" is not always labware.

## Evidence

| Evidence | Link |
| --- | --- |
| GX-271: X/Y/Z instrument, positional accuracy better than 0.2 mm, 59.1 × 54.2 × 61.0 cm, Code 20/200/34X racks, configurable with GX Solvent System or 402 Syringe Pump, Direct Injection Module, fraction collection valve, 50 µL to hundreds of mL and up to 50 mL/min | <https://www.gilson.com/default/gx-271-liquid-handler.html> |
| GX-281: large-capacity platform for injection, fraction collection and re-injection in semi-prep/prep HPLC | <https://www.gilson.com/default/gx-281-liquid-handler.html> |
| GX Solvent System 2 µL – 100 mL, 1 µL/min – 25 mL/min; Direct Injection Module up to 200 mL/min | <https://www.gilson.com/system-verity-gx-281-liquid-handler.html> |
| GX-271 user guide | <https://mantech-inc.com/wp-content/uploads/2020/08/MAN-SD-H-0211-01-AM122-User-Guide.pdf> |
| Gilson sample handling automation portfolio | <https://www.gilson.com/default/sample-handling> |

## Open Questions

| Area | Unknown |
| --- | --- |
| Control protocol | Gilson instruments historically use the GSIOC serial bus; this needs confirmation and a command-set reference before any driver work |
| Rack geometry | Coordinates and tube counts for Code 20 / 200 / 34X racks |
| Sensors | Whether liquid level detection, pressure, or leak detection is present |
| Software boundary | Whether TRILUTION or a documented device protocol is the right integration layer |
| Z-arm | Probe travel, needle types, and septum-piercing capability |
