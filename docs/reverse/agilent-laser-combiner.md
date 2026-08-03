# Agilent Laser Combiner Protocol Evidence Note

## Status

| Field | Value |
| --- | --- |
| Plan target | Agilent/Keysight Laser Combiner |
| Current state | Full command table and transport grammar recorded from reverse engineered evidence; driver exists, but no captured hardware trace or validation note exists |
| Better source status | Reverse engineered evidence is now the primary source; no manufacturer manual recorded |
| Next evidence | Hardware traces against a real combiner |
| Evidence type | Reverse engineered |
| Feasibility | Transport is plain serial 115200 8N1; a driver is straightforward once one board confirms the handshake |
| Protocol spec | [`agilent-laser-combiner-protocol.md`](agilent-laser-combiner-protocol.md) |

## Protocol Evidence Summary

| Area | Finding |
| --- | --- |
| Evidence inventory | Reverse engineered; see [`artifact-inspection-summary.md`](artifact-inspection-summary.md) |
| Exported SDK surface | External notes record 61 `LaserBoard*` entry points covering open/close/is-open, driver/firmware/hardware version, serial/model identity, laser line count/info, analog output info/get/set, power get/set, state get/set, blanking, external control, synchronization, shutter, galvo, neutral-density, sequence start/stop/set, register, and EEPROM operations |
| Adapter evidence | The Micro-Manager adapter exposes logical shutter/safety-shutter/DA-style devices and uses SDK calls for serial/firmware/driver version, per-line wavelength/power/state, blanking, external trigger/control, sequence, output, ND, and analog volts/channel behavior |
| Transport evidence | Reverse engineered notes record serial transport at 115200 8N1. Requests are `<cmd byte><binary payload>`; replies are `<echoed cmd byte><ASCII text>CRLF`. numanager does not implement serial port scanning; live control requires an explicitly configured endpoint |
| Command evidence | All 61 exports mapped to opcodes; setter/getter pairs are offset by `0x1E`. Unit chain recovered: raw DAC counts → volts (bit depth, min/max V) → mW (11-point calibration curve). See [`agilent-laser-combiner-protocol.md`](agilent-laser-combiner-protocol.md) |
| Missing wire evidence | No command has been observed against real hardware. No VID/PID, no reply-latency data, and **no interlock/fault command exists in the protocol at all** |

## Evidence To Collect

| Evidence | Required observations |
| --- | --- |
| Evidence inventory | Done; still need any vendor examples, headers, and package variants |
| Strings | USB VID/PID, endpoint names, channel names, laser line names, fault text; current pass exposes SDK naming but not wire grammar |
| Micro-Manager adapter calls | Done at API-surface level; still need exact initialization/polling/fault sequencing curated into the eventual spec |
| USB/HID trace | Enumeration, identity query, channel enable, intensity/power write, status readback |
| Hardware note | Controller model, firmware, attached laser lines, interlock wiring |

## Protocol Questions

| Area | Questions |
| --- | --- |
| Transport | **Answered**: serial COM, 115200 8N1, binary request / ASCII CRLF reply with command echo |
| Discovery | **Answered at protocol level**: `0x36` gives line count, `0x3A` wavelength per line. numanager requires a configured serial endpoint rather than active serial discovery |
| Output | **Answered at protocol level**: `0x0A` state mask, `0x0B` power, `0x10` shutter, `0x0E` blanking, `0x0D` external control. Open question: observed hardware effect |
| Intensity/power | **Answered**: raw DAC counts on the wire; mW only via the host-side 11-point calibration curve |
| Timing | Open: no busy flag or event channel exists; reply latency and safe inter-command spacing are unmeasured |
| Safety | Open and concerning: no interlock, fault, over-temperature, or key-switch command exists in the protocol |

## Candidate Public Surface

| Device | Capabilities | Properties |
| --- | --- | --- |
| Combiner hub | safety summary, possible `TriggerSink` | `model`, `firmware`, `interlock_closed`, `fault` |
| Laser channel | `Dac`, `TriggerSink` | `enabled`, `power` or `intensity`, `wavelength`, `fault`, `external_control` |

Use typed values: `OpticalPower` when calibrated power is evidenced, otherwise
`Ratio` for relative intensity, plus `Wavelength`, `Bool`, and `String`.

## Stop/Proceed Decision

| Decision | Condition | Status |
| --- | --- | --- |
| Proceed to spec | Transport and channel commands recovered | **Done** — see [`agilent-laser-combiner-protocol.md`](agilent-laser-combiner-protocol.md) |
| Hardware-trace required | Spec exists but nothing observed on a real board | **Current state** |
| Hardware validation absent | External protocol evidence is enough for a guarded implementation, but no hardware has confirmed the handshake and the protocol exposes no safety readback | **Current state** |

## Implementation Gate

Do not expand `numanager_drivers::agilent_laser_combiner` beyond the recorded
external-evidence command table until channel identity, at least one safe
output/readback path, and safety/fault semantics are documented from hardware
traces or stronger manufacturer sources.

reverse engineered evidence now covers channel identity and the command grammar, so the
remaining gate is hardware:

1. A configured real board must answer `0x03` with `"My100xBoard"`, with the serial trace
   and runtime output captured for the same window.
2. Read-only identity and line-info commands must return parseable values that
   match the recovered unit chain.
3. Safety must be resolved. The protocol has no interlock or fault readback, so
   either another surface provides it or the driver must not advertise
   `safety.interlock` and must not own emission enable.
