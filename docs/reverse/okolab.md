# Okolab Protocol Evidence Note

## Status

| Field | Value |
| --- | --- |
| Target | Okolab environmental controllers |
| Evidence class | An independent interface specification maintained outside this repository, plus a manufacturer-supplied command database shipped as third-party data |
| Current state | `numanager_drivers::okolab` implements the recorded serial/configured protocol surface. Full interface specification in [`okolab-protocol.md`](okolab-protocol.md); no captured trace or hardware-validation note |
| Next evidence | Hardware serial traces plus matching user-facing command output |
| Feasibility | Strong. The wire grammar and the command dictionary are both recorded. Value semantics and completion/fault behavior remain unvalidated. |

## Protocol Evidence Summary

| Area | Finding |
| --- | --- |
| Transport | Serial, 8N1, no flow control, CR-terminated, baud auto-scan `115200` then `4800` |
| Framing | Decimal command code (3 digits minimum), optional payload, CR. Optional checksum mode adds a `G`/`S`/`R` type byte and a `#` + 16-bit signed-byte-sum trailer |
| Identification | Database-driven: probe each candidate `Product.name_code` as a read command; the reply string is matched back to `Product.name`/`AltName.alt_name` |
| Command dictionary | `Parameters` (660 rows) joined through `ProductVar` gives per-product read/write/volatile-write/min/max codes, types, units, and enums |
| Errors and retries | `E1`–`E18` wire vocabulary with `E3` as the liveness signature and `E10` as bus-slave; retry policy recorded |
| Specification | [`okolab-protocol.md`](okolab-protocol.md) records the byte-exact grammar, checksum algorithm, session handshake, error vocabulary, retry policy, and the dictionary lookup |
| Missing wire evidence | Hardware traces are still missing for readback value formatting, write completion/ACK shape, stability/settling, and fault/alarm behavior |

## Evidence To Collect

| Evidence | Required observations |
| --- | --- |
| Serial trace | Open sequence, identity/module discovery, readback replies with units, safe write completion, ACK/error/fault replies |
| Hardware note | Controller model, firmware, connected temperature/gas/humidity modules |
| Runtime output | Matching user-facing command output for discovery, readback, and one safe write |

## Protocol Questions

| Area | Questions |
| --- | --- |
| Session | Resolved on paper: 8N1, baud scan, bare-CR liveness probe answered with `E3`, checksum negotiated by retrying the identity read framed. Trace confirmation still required |
| Framing | Resolved and byte-exact in `okolab-protocol.md`, including the checksum. Trace confirmation still required |
| Discovery | Resolved: `Product.name_code` probe returns the product-name string. Module inventory is a naming convention over `Parameters`, not a wire feature. No channel addressing exists — one device per port; `E10` marks a bus slave |
| Temperature | Codes recorded per product. Reply formatting, enable semantics, and stability/completion still need a trace |
| Gas | CO2/O2 setpoint/readback, enable, range, flow/pressure coupling |
| Humidity/flow/pressure | Whether exposed as direct modules or derived telemetry |
| Safety | Faults, sensor disconnects, gas alarms, thermal overrange, interlocks |

## Candidate Public Surface

| Device | Capabilities | Properties |
| --- | --- | --- |
| Okolab hub | discovery, safety summary | `model`, `firmware`, `fault`, `module_summary` |
| Temperature module | `TemperatureControl`, `Measure` | `target`, `actual`, `enabled`, `stable`, `fault` |
| Gas module | `GasControl`, `Measure` | `co2_target`, `co2_actual`, `o2_target`, `o2_actual`, `enabled`, `fault` |
| Humidity/flow/pressure module | `Measure` or environment capability after command evidence | `relative_humidity`, `pressure`, `flow_rate`, `fault` |

Use typed values: `Temperature`, `GasConcentration`, `Ratio`, `Pressure`,
`FlowRate`, `Bool`, and `String`.

## Stop/Proceed Decision

| Decision | Condition |
| --- | --- |
| Proceed to spec | Done. `okolab-protocol.md` is complete for framing, handshake, errors, and the command dictionary |
| Hardware trace | Required before claiming hardware-complete behavior because reply value formatting, write completion/ACK shape, and fault/alarm behavior are not hardware-validated |
| Live support | Current implementation exposes configured connected read/write; hardware serial traces and output records are still needed to validate discovery, readback, one safe write, and faults |

## Implementation Gate

`numanager_drivers::okolab` exposes the recorded configured serial surface.
Do not claim hardware-validated behavior until this note or a linked trace note
contains enough command evidence for discovery, readback, one safe write path,
and hardware-driven completion/fault behavior.
