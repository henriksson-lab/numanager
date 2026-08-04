# Xeryon Protocol Evidence Note

## Protocol Evidence Summary

| Item | Value |
| --- | --- |
| Target | Xeryon piezo stages/actuators on XD-M, XD-C, XD-OEM-class controllers, over the documented ASCII serial interface |
| Evidence class | Manufacturer documentation (controller manual + communication overview + CANopen introduction and published examples) |
| Transport | USB virtual COM / RS232 / UART ASCII, LF terminator, default 115200 baud, 8N1, no handshaking |
| Hardware validation | Not recorded |
| Current state | Native configured ASCII driver for one logical axis; optional real serial open/readback behind `os-serial`; typed move/home/stop/velocity/readback surface |
| Integrated devices | XLA/XUMU use CANopen/CiA 402 in `numanager_drivers::xeryon_canopen` |

## Implemented ASCII Boundary

| Role | Tags |
| --- | --- |
| Motion | `DPOS`, `STEP`, `HOME`, `STOP` |
| Velocity | `SSPD` |
| Readback | `SRNO`, `SOFT`, `STAT`, `EPOS`, `DPOS`, `SSPD`, `LLIM`, `HLIM` |
| `STAT` bits | motor/search/scan busy, position reached, encoder validity, end stops, encoder error, error limit, safety timeout, position fail |

Raw serial command entry is not exposed. `RSET`, `SAVE`, `LOAD`, `ZERO`, tuning,
amplitude/phase, direction, GPIO/UART configuration, and encoder-reset controls
stay hidden: they affect initialization, persistent state, safety behavior, or
hardware tuning.

## Implemented CANopen Boundary

`numanager_drivers::xeryon_canopen` maps public stage operations to standard CiA
402 NMT/SDO transactions. Configured mode records COB-ID and eight-byte payload
intent; with `connect = true` it opens SocketCAN (`os-can`) or SLCAN
(`os-serial`), sends NMT/SDO frames, validates SDO download acks, parses
expedited uploads, and updates cached status/position/mode readbacks.

| Role | Objects |
| --- | --- |
| State/mode | controlword `0x6040`, statusword `0x6041`, modes of operation `0x6060`, mode display `0x6061` |
| Motion | actual position `0x6064`, target position `0x607A`, profile velocity `0x6081`, profile acceleration `0x6083`, profile deceleration `0x6084` |
| Homing | homing method `0x6098` |

## Protocol Questions

| Question | Why it matters |
| --- | --- |
| Are `STAT` bits the same decimal bitfield on every firmware revision? | Bit meanings are documented; numbering and transitions are unconfirmed |
| Exact `INDX` vs `HOME` completion behavior per stage family? | `HOME` targets zero, `INDX` establishes encoder reference; needs bench confirmation |
| Can one serial backend safely remultiplex multiple axes during overlapping commands? | Axis prefixes exist, but scheduling and async feedback are unvalidated |
| Can model/type feedback infer encoder-unit conversion? | `encoder_units_per_um` must stay configured until model scaling is validated |
| How do limits, error-limit trips, thermal faults, and safety timeouts appear during motion? | Fault handling must be validated before stronger hardware-support claims |

## Hardware Validation Plan

Use [`hardware-validation-template.md`](../devices/hardware-validation-template.md)
and [`trace-capture-guide.md`](trace-capture-guide.md).

1. Record controller model, firmware version, stage model, serial number,
   transport, OS serial device, baud rate, and numanager commit.
2. With motion mechanically safe, query `SRNO=?`, `SOFT=?`, `STAT=?`, `EPOS=?`,
   `DPOS=?`, `SSPD=?`, `LLIM=?`, `HLIM=?`.
3. Run index/reference via a controlled `INDX` path; record `STAT`, `EPOS`, and
   visible behavior before/after.
4. Small absolute `DPOS` and relative `STEP` moves within travel: record command
   text, readbacks, completion timing, `STAT` transitions.
5. `STOP` during a slow move: record final `EPOS`, `DPOS`, status bits.
6. At least one safe limit or error-state observation, if hardware policy
   permits.

Do not replace these notes with scripted serial fixtures or encoder/decoder
round-trip tests.
