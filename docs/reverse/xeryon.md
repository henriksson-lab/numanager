# Xeryon Protocol Evidence Note

## Protocol Evidence Summary

| Item | Value |
| --- | --- |
| Plan target | Xeryon piezo stages and actuators controlled by XD-M, XD-C, and XD-OEM-class controllers over the documented ASCII serial interface |
| Current state | Native configured ASCII driver exists for one logical axis, optional real serial open/readback behind `os-serial`, typed move/home/stop/velocity/readback surface, and hidden maintenance boundary |
| Evidence quality | Manufacturer documentation |
| Better source status | Xeryon publishes controller manuals and a controller communication overview; no reverse engineering is required for the implemented ASCII command grammar |
| Transport | USB virtual COM / RS232 / UART ASCII, LF terminator, default 115200 baud, 8N1, no handshaking |
| Implementation note | Integrated XLA/XUMU devices use CANopen/CiA 402 and are implemented separately in `numanager_drivers::xeryon_canopen` with transaction planning, optional live SocketCAN/SLCAN NMT/SDO execution, and EDS object parsing |

## Sources

| Source | Evidence Used |
| --- | --- |
| Xeryon XD-M Controller manual v3.2, last updated 2026-02-20 | ASCII framing, axis prefixes, command tags, `=?` query syntax, units, feedback tags, status bits, and motion/settings command list |
| Xeryon controller communication overview | Controller-family communication interfaces and the statement that ASCII terminal commands are available through the controller manuals |
| Xeryon CANopen Introduction and `XLA-INTG_prog_examples` repository | Separate integrated-controller CANopen/CiA 402 path and EDS/example availability |

## Implemented ASCII Boundary

The driver uses only the documented public control/readback surface:

| Role | Tags |
| --- | --- |
| Motion | `DPOS`, `STEP`, `HOME`, `STOP` |
| Velocity | `SSPD` |
| Readback | `SRNO`, `SOFT`, `STAT`, `EPOS`, `DPOS`, `SSPD`, `LLIM`, `HLIM` |
| Status decoding | Documented `STAT` bits for motor/search/scan busy state, position reached, encoder validity, end stops, encoder error, error limit, safety timeout, and position fail |

The public driver does not expose raw serial command entry. `RSET`, `SAVE`,
`LOAD`, `ZERO`, tuning, amplitude/phase, direction, GPIO/UART configuration,
and encoder-reset controls remain hidden because they affect initialization,
persistent controller state, safety behavior, or hardware tuning.

## Implemented CANopen Boundary

For integrated XLA/XUMU devices, `numanager_drivers::xeryon_canopen` maps public
stage operations to standard CiA 402 NMT/SDO transactions. In configured mode it
records COB-ID and eight-byte payload intent. With `connect = true`, it can open
Linux SocketCAN behind `os-can` or serial SLCAN behind `os-serial`, send NMT/SDO
frames, validate SDO download acknowledgements, parse expedited SDO uploads, and
update cached status/position/mode readbacks.

| Role | Objects |
| --- | --- |
| State/mode | controlword `0x6040`, statusword `0x6041`, modes of operation `0x6060`, mode display `0x6061` |
| Motion | actual position `0x6064`, target position `0x607A`, profile velocity `0x6081`, profile acceleration `0x6083`, profile deceleration `0x6084` |
| Homing | homing method `0x6098` |

## Protocol Questions

| Question | Why It Matters |
| --- | --- |
| Are `STAT` bits emitted as the same decimal bitfield on every target firmware revision? | The manual documents bit meanings, but a real controller should confirm bit numbering and status transitions |
| What is the exact `INDX` and `HOME` completion behavior for each stage family? | `HOME` is documented as target zero, while `INDX` establishes encoder reference; public homing semantics need bench confirmation |
| Can a shared serial backend safely remultiplex multiple configured axes during overlapping commands? | The ASCII protocol has axis prefixes, but runtime scheduling and asynchronous feedback should be validated on real multi-axis controllers |
| Can model/type feedback reliably infer encoder-unit conversion? | The driver requires configured `encoder_units_per_um` until model-specific scaling is validated |
| How do limits, error-limit trips, thermal faults, and safety timeouts appear during connected motion? | Fault handling should be validated before advertising stronger hardware-support claims |

## Hardware Validation Plan

Use [`hardware-validation-template.md`](../devices/hardware-validation-template.md)
and [`trace-capture-guide.md`](trace-capture-guide.md) for a bench note.

Minimum useful run:

1. Record controller model, firmware/software version, stage model, serial number, transport, OS serial device, baud rate, and numanager commit.
2. With motion mechanically safe, query `SRNO=?`, `SOFT=?`, `STAT=?`, `EPOS=?`, `DPOS=?`, `SSPD=?`, `LLIM=?`, and `HLIM=?`.
3. Run index/reference through the vendor GUI or controlled `INDX` path and record `STAT`, `EPOS`, and visible stage behavior before and after.
4. Execute small absolute `DPOS` and relative `STEP` moves within travel, recording command text, readbacks, completion timing, and `STAT` transitions.
5. Execute `STOP` during a slow scan or move, recording final `EPOS`, `DPOS`, and status bits.
6. Record at least one safe limit or error-state observation if hardware policy permits it.

Do not replace these notes with scripted serial fixtures or encoder/decoder
round-trip tests.
