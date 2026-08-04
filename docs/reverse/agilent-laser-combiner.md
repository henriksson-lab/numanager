# Agilent Laser Combiner Protocol Evidence Note

## Status

| Field | Value |
| --- | --- |
| Target | Agilent/Keysight Laser Combiner |
| Evidence class | Reconstructed host-side protocol description; no manufacturer manual recorded |
| Current state | Full command table and transport grammar recorded; driver exists, but no hardware trace or validation note exists |
| Next evidence | Traces against a real combiner |
| Feasibility | Plain serial 115200 8N1; a driver is straightforward once one board confirms the handshake |
| Protocol spec | [`agilent-laser-combiner-protocol.md`](agilent-laser-combiner-protocol.md) |

## Protocol Evidence Summary

| Area | Finding |
| --- | --- |
| Command surface | Full opcode table recovered; setter/getter pairs offset by `0x1E`. Covers identity/version, laser line count and per-line info, state, power, analog output, blanking, external control, sync, shutter, galvo, ND filter, sequences, register and EEPROM access |
| Transport | Serial, 115200 8N1. Requests are `<cmd byte><binary payload>`; replies are `<echoed cmd byte><ASCII text>CRLF`. numanager does not scan serial ports; live control requires an explicitly configured endpoint |
| Units | Raw DAC counts on the wire → volts (bit depth, min/max V) → mW (11-point calibration curve), all host-side |
| Missing | No command observed against real hardware. No VID/PID, no reply-latency data, and **no interlock/fault command exists in the protocol at all** |

## Evidence To Collect

| Evidence | Required observations |
| --- | --- |
| Serial trace | Identity query, line inventory, channel enable, power write, status readback, with runtime output for the same window |
| Timing | Reply latency, safe inter-command spacing, behaviour under back-to-back commands |
| Device identity | USB VID/PID or serial-adapter identity for deterministic discovery |
| Hardware note | Controller model, firmware, attached laser lines, interlock wiring |

## Protocol Questions

| Area | Status |
| --- | --- |
| Transport | **Answered**: serial COM, 115200 8N1, binary request / ASCII CRLF reply with command echo |
| Discovery | **Answered at protocol level**: `0x36` line count, `0x3A` wavelength per line. numanager requires a configured endpoint rather than active serial discovery |
| Output | **Answered at protocol level**: `0x0A` state mask, `0x0B` power, `0x10` shutter, `0x0E` blanking, `0x0D` external control. Open: observed hardware effect |
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
| Proceed to spec | Transport and channel commands recovered | **Done** |
| Hardware-trace required | Spec exists but nothing observed on a real board | **Current state** |
| Hardware validation absent | Command table is enough for a guarded implementation, but no hardware has confirmed the handshake and the protocol exposes no safety readback | **Current state** |

## Implementation Gate

Do not expand `numanager_drivers::agilent_laser_combiner` beyond the recorded
command table until hardware traces (or stronger manufacturer sources) document
channel identity, at least one safe output/readback path, and safety/fault
semantics.

The command grammar and channel identity are covered, so the remaining gate is
hardware:

1. A configured real board must answer `0x03` with `"My100xBoard"`, with the
   serial trace and runtime output captured for the same window.
2. Read-only identity and line-info commands must return parseable values
   matching the recovered unit chain.
3. Safety must be resolved. The protocol has no interlock or fault readback, so
   either another surface provides it or the driver must not advertise
   `safety.interlock` and must not own emission enable.
