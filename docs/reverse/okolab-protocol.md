# Okolab Serial Protocol Specification

## Evidence

| Item | Value |
| --- | --- |
| Status | Interface specification from external evidence; no hardware validation yet |
| Evidence classes | Manufacturer documentation (public interface declarations); an independent interface specification maintained outside this repository; a manufacturer-supplied command database shipped as third-party data in this repository and excluded from its license |
| Consistency check | Independent copies of the interface specification agree on the command grammar, baud table, command-code format, float formats, error strings, and the identification/discovery lookups; the command-database content matches across copies. |
| Validation boundary | Framing, checksum, retry, error vocabulary, product identification, and the dictionary lookup are recorded as candidate wire facts. What the values *mean* on real hardware — units in practice, settling/completion behavior, alarm semantics, safe write ranges — is **not** validated and still needs a hardware trace. |

## 1. Transport

| Field | Value |
| --- | --- |
| Transport | Serial (USB-serial bridges in practice, §3.1) |
| Line settings | **8 data bits, no parity, 1 stop bit, no flow control**; CTS/DSR ignored, DTR off, RTS off, XON/XOFF disabled |
| Baud rates tried | `115200`, then `4800` |
| Terminator | Carriage return `0x0D` in both directions. **No LF.** |
| Write timeout | 200 ms per write, followed by a drain |
| Reply timeout | 200 ms for a bare-CR probe; **500 ms** once a command code has been written |
| Read granularity | One byte at a time with a 199 ms per-byte deadline, looped until CR or overall deadline |
| Max reply | 199 payload bytes plus a terminating NUL in a 200-byte buffer |
| Max request payload | 201 bytes, truncated beyond that |
| Concurrency | One transaction at a time per port; concurrent callers block until the in-flight transaction clears |
| Write failure | Flush, close, re-open with the same settings, retry the identical write **once**; a second failure is a communication error. The controller therefore tolerates a mid-session port re-open with no re-handshake. |

## 2. Frame grammar

A command is identified by a **numeric command code** in decimal, zero-padded to
a minimum of three digits. The command database contains 4-digit codes (for
example `544`, `1051`), which emit 4 characters, so a parser must treat the code
field as *variable length* even though the recorded reply validation assumes 3
characters (§2.3).

### 2.1 Plain Frame Grammar

Used when checksum support is disabled or not present.

| Operation | Bytes on the wire |
| --- | --- |
| Read | `DDD` `\r` |
| Write | `DDD` `<payload>` `\r` |
| RPC | `DDD` `\r` (indistinguishable from a read) |
| Status probe | `\r` (bare CR, no code) |
| Success reply | `DDD` `<payload>` `\r` |

A write is emitted as three separate write+drain operations: code, payload,
terminator. The command type (`G`/`S`/`R`) is **not** transmitted in plain mode —
the device distinguishes a read from a write purely by whether a payload precedes
the CR. On reply, the first three bytes must echo the code sent; everything from
offset 3 up to (not including) the CR is the value.

### 2.2 Checksum Frame Grammar

Request and reply share one shape, where `<type>` is `G` (`0x47`, get/read),
`S` (`0x53`, set/write), or `R` (`0x52`, RPC/execute); any other type value is
rejected locally with internal code 15.

```text
<type> <DDD> <payload> '#' <sum_hi> <sum_lo> '\r'
```

The status probe in checksum mode is `'#' 0x00 0x00 '\r'` — an empty body with a
zero checksum.

The checksum is a 16-bit value sent as two raw bytes, high byte first: the sum of
the **signed 8-bit** values of everything after the type character and before the
`#`.

```text
sum = Σ (int8_t) c   for c in DDD ++ payload
sum_hi = (sum >> 8) & 0xFF
sum_lo =  sum       & 0xFF
```

The type character is excluded on both send and receive. On receive the two bytes
after `#` are read **unsigned** and combined as `hi*256 + lo`; a mismatch yields
internal code 16.

> Because the checksum bytes are raw binary, a checksum-mode frame is **not**
> ASCII-safe: `sum_hi`/`sum_lo` can be `0x00`, `0x0D`, or `0x23`. An embedded CR
> is handled by treating CR as end-of-frame only once both checksum bytes have
> been consumed, or when the frame starts with `E`.

Reply validation order: length ≤ 1 → malformed (internal 11); first byte `'E'` →
error string (§4); length ≤ 3 → malformed; checksum mismatch → internal 16;
leading `<type><DDD>` does not echo what was sent → malformed; otherwise payload
is bytes 4 .. len-3 (total minus the 4-byte header, `#`, and two checksum bytes).

### 2.3 Known quirks worth reproducing or deliberately not reproducing

- The reply header comparison is fixed at **3 bytes** plain / **4 bytes**
  checksum, so for a 4-digit code only a prefix is checked and a fixed 3 (or 4)
  bytes stripped, leaving the trailing digit inside the value. Send side, same
  cause: a checksum-mode request for a 4-digit code transmits the fourth digit
  but omits it from the outgoing checksum, while the receive-side validator sums
  every byte after the type until `#`. Treat 4-digit codes as unvalidated.
- In plain mode a read and an RPC produce byte-identical traffic.
- A bare-CR probe in plain mode compares the reply against an all-zero buffer, so
  a well-formed data reply to a bare CR reads as malformed; the bare CR is only
  ever a liveness probe.

## 3. Session and identification sequence

### 3.1 Port selection

Reject a port already open; run the filtered connect sequence (§3.2–3.4) against
a product-name filter; discover properties (§5); detect modules (§7). When
USB-only filtering is enabled, restrict to USB-transport ports whose VID/PID
appears in the database's USB table for products matching the filter:

| vid | pid | Product lines |
| --- | --- | --- |
| 1027 (`0x0403`, FTDI) | 24577 | Bold Line, CO2-UNIT-XL, H401-T, UNO |
| 1003 (`0x03EB`, Atmel) | 9220 (`0x2404`) | TC-7D, H402-T, UNO-CONTROLLER |
| 0 | 0 | H402-T, UNO-CONTROLLER, H401-T, UNO |

The table also carries a nominal baud (`4800` for Bold Line and CO2-UNIT-XL,
`115200` elsewhere), but it is informational and unused — baud is discovered by
the scan in §3.2.

### 3.2 Baud discovery / liveness probe

The baud list is walked **twice** (a two-pass retry). For each baud: flush and
re-open the port at that baud, send a **bare CR**, read the reply. The port is
accepted on internal `12` (success) or internal `2`, which is the error reply
`E3`. An idle Okolab controller answers a bare CR with `E3\r`; that is the
liveness signature, and the "device alive" flag is set only on exactly `E3`.

```text
host -> (no code):  0D
dev  -> "E3" 0D                 ; device present and idle
```

### 3.3 Product identification

Identification is a **database-driven probe**, not a fixed identity command.
Candidates are the distinct non-zero `name_code` and `code_alt` values of
`Product` rows whose `name` matches the caller's product-name substring, ordered
by product line; an empty filter matches every product. Each candidate is an
**(identity command code, product line)** pair. For each, in order:

1. Send a **read** of the identity command code.
2. On failure, move to the next candidate.
3. On success the reply string is the product name; match it back to the
   catalogue against `Product.name` and `AltName.alt_name` for that identity
   code, appending every matching `Product.id` to the device's product-id list.
4. After the first match, stop probing *other* product lines and continue only
   within the matched line.
5. If any probe returns internal `8` (reply `E10`), abort the whole scan — the
   device is a bus slave and must not be enumerated on this port.

Identity command codes in the shipped command database:

| `name_code` | Parameter that carries it | Product lines |
| --- | --- | --- |
| `1` | `Product code` (id 351) | TC-7D, OKO-TOUCH |
| `8` | `Product code` (id 319) | UNO, UNO-CONTROLLER, LEO |
| `17` | `Product code` (id 279) / `Product code (COSC)` (id 605) | H401-T, H402-T, LEO |
| `31` | `Gas product code` (id 25) / `Product code (TK-Sens)` (id 631) | Bold Line, CO2-UNIT-XL, LEO |
| `58` | `Gas2 product code` (id 39) | Bold Line |
| `64` | `Temperature product code` (id 45) / `Product code (PT-Sens)` (id 630) | Bold Line, LEO |
| `362` | `Humidity product code` (id 255) | Bold Line |

A Bold Line temperature unit is identified by `064\r` → `064H301 T Unit-BL\r`, a
Bold Line gas unit by `031\r` → `031CO2 Unit-BL\r`.

> **Matching caution.** The recorded catalogue match applies the identity-code
> filter to the alternative-name branch only, so a reply equal to a product name
> from a *different* identity code can still match. Apply it to both branches.

### 3.4 Sub-product probe

For each `SubProduct` row whose `productId` is in the found list and whose
`subProdId` is not already found, send a read of that row's `sp_code` and add
`subProdId` **iff the reply is exactly `"1"`**. The shipped database has one such
rule: `sp_code = 234` detects `HM-ACTIVE-SUB` (an active humidity module) on
`CO2 Unit-BL` and the three `CO2-O2 Unit-BL` variants.

```text
host -> "234" 0D
dev  -> "234" "1" 0D            ; humidity sub-module present
```

### 3.5 Checksum negotiation

Once the product list is non-empty and the caller has asked for checksums
(default off), the identity code is re-read framed with `G`/`#`/checksum. Success
keeps checksum mode on; otherwise the session falls back to plain framing.
Checksum framing is used only when **presence AND usage** are both true.

## 4. Error vocabulary and result mapping

### 4.1 Wire error replies

Any reply whose first byte is `'E'` is an error string. There is no `E11`-`E14`
handler.

| Wire reply | Internal code | Meaning |
| --- | --- | --- |
| `E1` | 0 | Not supported by this unit; retried twice, then abandoned when invalid-command checking is armed |
| `E2` | 1 | Communication error |
| `E3` | 2 | Communication error — the canonical "I am here but that was not a command" reply; accepted as proof of liveness (§3.2) |
| `E4` | 3 | Not supported |
| `E5` | 4 | Bad argument (out of range / bad format) |
| `E6` | 5 | Not supported |
| `E7` | 6 | Communication error |
| `E8` | 11 | Communication error |
| `E9` | 7 | Device present but not running |
| `E10` | 8 | Device is a bus slave; do not enumerate it on this port |
| `E15` | 13 | Communication error |
| `E16` | 14 | Communication error |
| `E17` | 15 | Communication error |
| `E18` | 16 | Communication error |
| any other `E…` | 11 | Communication error |

### 4.2 Internal (non-wire) codes

| Internal | Meaning |
| --- | --- |
| 9 | Reply timeout (deadline expired with no CR) |
| 10 | Serial write failed after one reconnect+retry |
| 11 | Malformed reply: too short, or header did not echo the sent code |
| 12 | **Success** |
| 15 | Bad command type (local programming error) |
| 16 | Checksum mismatch |

### 4.3 Retry policy

If the "device alive" flag is clear, the liveness check runs first and a failure
is returned immediately. Otherwise up to **20 attempts** are made: return
immediately on internal `3,4,5,6,7,8,12` (and on `2` when the command code is 0);
on `0,1,2,9,10,11,13,14,15,16` re-run the liveness check and either return its
error or retry. Internal `0` (`E1`) after the second attempt, with
invalid-command checking armed, returns `E1` with no further retries. The
liveness check itself retries the bare-CR probe **10 times** if the device was
previously known-alive and **2 times** otherwise, clearing the alive flag when it
gives up.

Net effect: one property read can legitimately generate up to ~20 command frames
interleaved with up to ~10 bare-CR probes each. A reimplementation should choose
a much tighter budget and document it.

## 5. Command dictionary

The manufacturer-supplied command database shipped as third-party data in this
repository is the command dictionary; it is read-only in practice.

### 5.1 Schema roles

| Table | Rows | Role |
| --- | --- | --- |
| `ProdLine` | 10 | Product families (`Bold Line`, `H401-T`, `H402-T`, `UNO`, `TC-7D`, `LEO`, …) |
| `Product` | 44 | One row per marketed unit: `name`, `name_code`, `code_alt`, `prodLineID` |
| `AltName` | 15 | Alternative reply strings mapping to the same `Product` |
| `SubProduct` | 4 | `sp_code` probe → `subProdId` to add when the reply is `"1"` |
| `UsbInfo` | 12 | Per-product-line USB VID/PID and nominal baud |
| `VarType` | 5 | `0 Undefined, 1 String, 2 Integer, 3 Floating, 4 Enumerator` |
| `Parameters` | 660 | The command dictionary — see §5.2 |
| `ProductVar` | 2116 | Which parameters each product exposes (many-to-many) |
| `EnumType` / `EnumValues` | 40 / 164 | Named value sets for `var_type = 4` |

**Command codes are not globally unique.** `read_code = 1` is `Product code` for
TC-7D but `Temperature 1` for another line; a code is only meaningful for a
specific `Product.id`. Resolution is always `Product → ProductVar → Parameters`.

### 5.2 The `Parameters` row

| Column | Wire role |
| --- | --- |
| `name` | Property key |
| `unit` | Display unit (`°C`, `%`, `bar`, `ml/min`, `mbar`, `rpm`, `l/min`, `V`, `W`, `s`, `min`, `day`, …) |
| `description` | Human text |
| `var_type` | Value encoding (see `VarType`) |
| `main` / `advanced` | Summary-view / advanced-view flags |
| `oneshot` | Identity-like values (product code, serial number, software version) that need reading only once and can be cached |
| `read_code` | Read command code (`G`); `0` = write-only property |
| `write_code` | Persistent (EEPROM) write command code (`S`); `0` = read-only property |
| `write_code_ram` | Volatile (RAM) write command code (`S`) |
| `min_code` / `max_code` | Command codes whose **read** returns the minimum / maximum settable value |
| `enum_type_id` | `EnumType.id` when `var_type = 4` |

Derived flags:

```text
read_only  = (write_code == 0)
write_only = (read_code  == 0)
has_limits = (min_code != 0 && max_code != 0)
write_type = (write_code != 0 ? EEPROM : 0) | (write_code_ram != 0 ? VOLATILE : 0)
```

`read_only` deliberately ignores `write_code_ram`: a property with only a
volatile write reports read-only yet is still writable through the volatile path.

### 5.3 Property discovery

Select the `Parameters` rows joined through `ProductVar` to the product-id list
built in §3.3–3.4, so a device matching both a base product and a sub-product
gets the union of both parameter sets; for each `var_type = 4` row load its value
set from `EnumValues`. Parameter names prefixed `[DBG]` or `[PROD]` are
debug/production-only entries and are meant to be filtered out of the normal set.

### 5.4 A worked example

Product `H301 T Unit-BL` (`Product.id = 5`, `Bold Line`, `name_code = 64`)
exposes 70 parameters. Selected rows:

| name | unit | var_type | read | write | write_ram | min | max |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `Temperature` | `°C` | 3 float | 48 | – | – | – | – |
| `Temperature setpoint` | `°C` | 3 float | 82 | 142 | 141 | 144 | 143 |
| `Temperature status` | — | 4 enum (`Status`) | 128 | – | – | – | – |
| `Temperature product code` | — | 1 string | 64 | – | – | – | – |
| `Temperature serial number` | — | 1 string | 67 | – | – | – | – |
| `Objective Heater temperature` | `°C` | 3 float | 189 | – | – | – | – |

Resulting traffic, plain mode. **The command codes and their order are recorded
evidence; the reply payload values are illustrative, not captured bytes** — no
hardware trace exists yet, so the exact numeric formatting is unknown.

```text
host -> "064" 0D            dev -> "064H301 T Unit-BL" 0D  ; identify
host -> "048" 0D            dev -> "048" "37.02" 0D        ; current temperature
host -> "082" 0D            dev -> "082" "37.00" 0D        ; setpoint
host -> "144" 0D            dev -> "144" "25.00" 0D        ; min_code
host -> "143" 0D            dev -> "143" "50.00" 0D        ; max_code
host -> "141" "37.50" 0D    dev -> "141" … 0D              ; volatile (RAM) write
host -> "142" "37.50" 0D    dev -> "142" … 0D              ; persistent (EEPROM) write
host -> "128" 0D            dev -> "128" "0" 0D            ; Status: 0=OK 1=Transient 2=Alarm 3=Error 4=Disabled
```

The setpoint read in checksum mode is byte-exact — `"082"` is `0x30 0x38 0x32`,
so `sum = 0x9A`:

```text
host -> 'G' "082" '#' 0x00 0x9A 0D
dev  -> 'G' "082" "37.00" '#' <hi> <lo> 0D
```

## 6. Value encoding

| `var_type` | Request payload | Reply parsing |
| --- | --- | --- |
| `1` String | Raw bytes, max 201 | Raw bytes between header and terminator |
| `2` Integer | Decimal integer text | Decimal integer text |
| `3` Floating | Fixed-point decimal with a caller-selected precision of 3, 2, or 1 fractional digits, or default float formatting otherwise | Decimal float text |
| `4` Enumerator | The requested enum *name* is matched against `EnumValues.enum_name` and the numeric `enum_value` is sent as an integer | Integer, resolved back to a name through `EnumValues` |

A volatile write uses `write_code_ram`, a persistent write uses `write_code`.
Limits are plain reads of `min_code` then `max_code`, parsed as floats — there is
no dedicated limit frame type.

**RPC framing is unresolved.** The recorded command-execute path issues a plain
read of the property's `read_code` and never emits the `R` type, which is
reachable only from a lower-level path nothing in the ordinary device layer uses.
Parameters whose name ends in `(RPC)` (for example id 580 `Pid-Tuning Start
(RPC)`, `read_code = 0`, `write_code = 580`) therefore appear to need a write
rather than an `R` frame. Unresolved until a hardware trace shows which framing
the controller accepts.

## 7. Module abstraction

`Module` is a fixed four-entry grouping and is purely a naming convention over
named properties — it adds **no new wire commands**, so a reimplementation can
skip it and work from `Parameters` directly.

| Module | Value property | Setpoint property | "paused"/status properties | Default limits |
| --- | --- | --- | --- | --- |
| Temperature (0) | `Temperature` | `Temperature setpoint` | *(none)* | 25.0 … 50.0 |
| CO2 (1) | `CO2` | `CO2 setpoint` | `Gas control paused` | 0.0 … 20.0 |
| O2 (2) | `O2` | `O2 setpoint` | `Air mode status`, `Gas control paused` | 0.0 … 20.0 |
| Humidity (3) | `Humidity` | `Humidity setpoint` | `Gas control paused` | 50.0 … 100.0 |

A module exists when its value property is in the set loaded in §5.3; its
dimension unit is that property's `unit`. A candidate "paused" property is kept
only if it reads successfully, and a module with no surviving paused property
cannot be disabled. Enabled state is that property read as a boolean and
inverted. Limits fall back to the table above when the underlying property has no
`min_code`/`max_code`.

## 8. Background polling

There is no push/streaming path — all telemetry is polled. Polling is
per-property with a per-property due time, shares the same serialization as
foreground traffic, drains queued writes first, and every polled read is a full
request/reply transaction under the retry policy of §4.3.

## 9. Implementation checklist for an SDK-free driver

Recorded and safe to implement from this document: serial settings, baud scan,
bare-CR liveness probe and CR framing; plain and checksum request/reply grammar
including the exact checksum; database-driven identification (`name_code` probe →
name match) and sub-product probe; `Parameters`/`ProductVar` resolution into a
per-device command dictionary with read/write/volatile-write/min/max codes and
enum resolution; and the error vocabulary with its mapping to typed errors.

Needs a hardware trace before claiming hardware-complete status
(`docs/reverse/trace-capture-guide.md`):

| Gap | What the trace must show |
| --- | --- |
| Reply formats | Actual numeric formatting, decimal separator, sign, and unit handling for float properties on at least one temperature and one gas unit |
| Write completion | Whether the reply to a write echoes the value, is empty, or is an ACK; and how long a setpoint takes to be reflected in the readback |
| Stability / settling | What `Temperature status` (enum `Status`: OK / Transient / Alarm / Error / Disabled) does across a setpoint change — the only visible completion signal |
| Faults and alarms | Real `E…` replies from a live unit, sensor disconnect, over-range, gas alarm, interlock |
| 4-digit codes | Whether any product actually uses a >3-digit code on a reply path, given the fixed-width header check in §2.3 |
| RPC framing | Whether `(RPC)` parameters are triggered by an `S` write or an `R` frame |
| Checksum mode | That a real controller accepts `G`/`S` framing and the byte-exact checksum above |
| Safe ranges | That `min_code`/`max_code` reads return usable bounds, and what the controller does with an out-of-range write (expected `E5`) |

Until those exist, the Okolab driver exposes the configured/read-write protocol
surface recorded in `docs/reverse/okolab.md` without claiming validated settling,
alarm, or safe-range behavior.
