# Okolab Protocol Evidence Note

## Status

| Field | Value |
| --- | --- |
| Plan target | Okolab environmental controllers |
| Current state | `numanager_drivers::okolab` implements the recovered serial/configured protocol surface. Complete static protocol spec exists in [`okolab-protocol.md`](okolab-protocol.md); no captured trace or validation note |
| Better source status | Reverse engineered evidence recovered the framing, checksum, identification handshake, error vocabulary, retry policy, and command-dictionary read algorithm |
| Next evidence | Hardware serial traces plus matching user-facing command output |
| Evidence type | Reverse engineered |
| Feasibility | Strong. The wire grammar is recovered; [`okolib.db`](../../data/third_party/okolab/okolib.db) is the shippable command dictionary. Value semantics and completion/fault behavior remain unvalidated. |

## Protocol Evidence Summary

| Area | Finding |
| --- | --- |
| Evidence inventory | Reverse engineered; see [`artifact-inspection-summary.md`](artifact-inspection-summary.md) |
| Transport evidence | Serial via `libserialport`, 8N1, no flow control, CR-terminated, baud auto-scan `115200` then `4800` |
| External note coverage | Framing, checksum, identification handshake, error vocabulary, retry policy, and `okolib.db` command lookup |
| Framing | Three-digit decimal command code, optional payload, CR. Optional checksum mode adds a `G`/`S`/`R` type byte and a `#`+16-bit signed-byte-sum trailer |
| Identification | Database-driven: probe each candidate `Product.name_code` as a read command; the reply string is matched back to `Product.name`/`AltName.alt_name` |
| Command dictionary | Shipped third-party Okolab database `Parameters` (660 rows) joined through `ProductVar` gives per-product read/write/volatile-write/min/max codes, types, units, and enums |
| Protocol spec | [`okolab-protocol.md`](okolab-protocol.md) records the byte-exact grammar, checksum algorithm, session handshake, error vocabulary, retry policy, and the `okolib.db` read algorithm |
| Missing wire evidence | Hardware traces are still missing for readback value formatting, write completion/ACK shape, stability/settling, and fault/alarm behavior |

## Evidence To Collect

| Evidence | Required observations |
| --- | --- |
| Evidence inventory | Done; still need any vendor examples and hardware package variants |
| Reverse engineered note | Done: static serial grammar and command-code database model recovered; hardware behavior is still unvalidated |
| Micro-Manager adapter calls | Properties, polling cadence, initialization/shutdown order, error handling |
| Serial trace | Open sequence, identity/module discovery, readback replies with units, safe write completion, ACK/error/fault replies |
| Hardware note | Controller model, firmware, connected temperature/gas/humidity modules |

## Protocol Questions

| Area | Questions |
| --- | --- |
| Session | Resolved statically: 8N1, baud scan, bare-CR liveness probe answered with `E3`, checksum negotiated by retrying the identity read framed. Trace confirmation still required |
| Framing | Resolved statically and byte-exact in `okolab-protocol.md`, including the checksum. Trace confirmation still required |
| Discovery | Resolved statically: `Product.name_code` probe returns the product-name string. Module inventory is a naming convention over `Parameters`, not a wire feature. No channel addressing exists — one device per port; `E10` marks a bus slave |
| Temperature | Codes recovered per product from `okolib.db`. Reply formatting, enable semantics, and stability/completion still need a trace |
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
| Proceed to spec | Done. `okolab-protocol.md` is complete for framing, handshake, errors, and the `okolib.db` command dictionary |
| Hardware trace | Required before claiming hardware-complete behavior because reply value formatting, write completion/ACK shape, and fault/alarm behavior are not hardware-validated |
| Live support | Current implementation exposes configured connected read/write; hardware serial traces and output records are still needed to validate discovery, readback, one safe write, and faults |

## Implementation Gate

`numanager_drivers::okolab` exposes the recovered configured serial surface.
Do not claim hardware-validated behavior until this note or a linked trace note
contains enough command evidence for discovery, readback, one safe write path,
and hardware-driven completion/fault behavior.
