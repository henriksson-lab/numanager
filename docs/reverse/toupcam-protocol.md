# Reverse Note — ToupTek USB camera protocol

## Target And Status

| Field | Value |
| --- | --- |
| Target | ToupTek USB cameras, vendor id `0x0547` |
| Device page | `docs/devices/toupcam.md` |
| Related notes | [`toupcam-u3cmos03100kpa.md`](toupcam-u3cmos03100kpa.md) (captured traffic), [`toupcam-model-registry.md`](toupcam-model-registry.md) (catalogue contents) |
| Status | promote — the driver programs the sensor directly for models with a specified register map |

## Source

The authoritative document is the **ToupTek USB camera interface
specification**, an independent interface specification maintained outside this
repository and written so the protocol can be implemented from it alone. It
contains no vendor code. Section numbers below refer to it. Claims marked as
trace-backed come from captured traffic from a physical device.

Companion data: a camera catalogue (1337 variants), 45 sensor bring-up tables,
and decoded traffic for `0547:3310`.

## Interface facts

| Area | Spec | Fact / driver behaviour |
| --- | --- | --- |
| Device shape | §1 | Vendor interface `FF/00/00`, bulk IN `0x81` |
| Session token | §4 | The host chooses a 16-bit token and announces it in the `0x16` probe; both sides derive the register mask from it. Token 0 selects the identity mask, so register operands are plaintext. The driver sends `SESSION_TOKEN = 0` and carries **no masking arithmetic at all** |
| Probe | §4.4 | `IN 0x16`, first byte must be `0x08`, retried for up to 2 s |
| Register access | §8 form A | `IN 0x0B`, `wValue = value`, `wIndex = register`, `wLength 1`; the returned byte is status. A register write is an IN transfer |
| Bring-up | §13 | Table 45, as typed `InitStep::Reg` / `InitStep::DelayMs` steps |
| Window and timing | §9 | Programmed from `SensorProfile`; `X 134..2181`, `Y 6..1539` reproduce 2048x1534 |
| Exposure | §10 | `coarse_integration_time()`; writes `LINE_LENGTH_PCK` only when a long exposure stretches the row period |
| Gain | §11 | `analog_gain_code()` step ladder to `ANALOG_GAIN`, capped at `0x1E` above 289 % |
| Streaming | §12 | `OUT 0x01`, `wValue 0x0003` start / `0x0000` stop, `wIndex 0x000F` |
| Framing | §12 | Segment on short bulk transfers; consume the 1-byte trailer |
| Catalogue | §14 | `toupcam_models.tsv`, 1337 variants, looked up by product id |

Deliberately not implemented: the link self-test (§5 — gates nothing), the flat
address space (§6 — the driver needs neither the serial nor the calibration
blob), register-record arrays (§7), and form B register access (§8), for which
no device was available.

Because token 0 removes masking entirely, there is no key schedule, no
per-session state, and nothing to carry between transfers. This is the single
fact that turns the protocol from "replay a recorded capture" into "program the
sensor".

## Validation

| Claim | Evidence |
| --- | --- |
| Exposure formula | Reproduces all five worked values in §10 exactly (842.063 ms → 35879, 344.687 ms → 14687, 96.06 ms → 4093, 9.912 ms → 422, 0.1 ms → 4), asserted by the bring-up harness |
| Gain ladder | 180 % → `0x12`, matching the `ANALOG_GAIN` value in captured traffic at that setting |
| Window geometry | `2181-134+1 = 2048`, `1539-6+1 = 1534`, matching the catalogue and the measured 3 141 633-byte frame |
| Bring-up table | Every entry present, in order, in captured traffic for this device |

### Hardware validation

U3CMOS03100KPA (`0547:3310`), 2026-08-04, through numanager's public runtime API
with no other host software attached. Probe with token 0, bring-up table, window
and timing, computed exposure and gain, stream start — produced whole 2048x1534
frames of 3 141 632 pixel bytes, and both controls demonstrably reached the
sensor:

| Exposure | frame max | | Gain at 1500 ms | frame mean |
| --- | --- | --- | --- | --- |
| 1 ms | 3 | | 100 % | 5 |
| 10 ms | 3 | | 200 % | 12 |
| 100 ms | 19 | | 400 % | 18 |
| 500 ms | 97 | | 800 % | 18 |
| 1500 ms | 249 | | | |

Exposure is monotonic and close to linear where unsaturated (100 ms → 19,
500 ms → 97: a 5x change for 5x exposure). Gain saturating between 400 % and
800 % is specified behaviour, not a defect — the ladder caps at `0x1E`, so both
settings write the same code.

One correction to the specification was required. Its bring-up table ends with
`RESET_REGISTER = 0x10D8` (streaming), but the sensor produces **no frames at
all** if the window is reprogrammed while streaming. Hardware requires `0x10D0`
(standby) at that point, returning to streaming only after the window, row
period and PLL are set, under the two grouped-hold variants. The driver follows
the hardware-verified behaviour.

## Correction to earlier notes in this repository

An earlier note in this repository concluded that exposure and gain were
protected by an unbreakable per-session device key. That was wrong on two counts
— a host-chosen token was mistaken for a device secret, and an exposure table
was mis-paired against its screenshots by one row. Both are recorded in
[`toupcam-u3cmos03100kpa.md`](toupcam-u3cmos03100kpa.md) rather than deleted, so
the error stays auditable.
