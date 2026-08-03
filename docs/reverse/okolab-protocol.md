# Okolab Serial Protocol Specification

## Evidence

| Item | Value |
| --- | --- |
| Status | External-evidence protocol specification; no hardware validation yet |
| Runtime evidence | Reverse engineered |
| Header evidence | Public declarations |
| Command dictionary | Shipped Okolab command database: [`../../data/third_party/okolab/okolib.db`](../../data/third_party/okolab/okolib.db) |
| Adapter evidence | Micro-Manager adapter behavior |
| Note coverage | Frame grammar, checksum control bytes, raw checksum trailer, CR terminator, serial settings, retry gates, DB-driven identity probe, property-table loading, write-code selection, and RPC reachability |
| Consistency check | Checked copies carry the same command grammar and database content. The shipped database is third-party data and is excluded from this repository's license. |
| Cross-build check | The checked alternate build carries the same baud table, command-code format, float formats, error strings, database filename, and identification/discovery queries. |
| Validation boundary | Framing, checksum, retry, error vocabulary, product identification, and the database read algorithm are recorded as candidate wire facts. What the values *mean* on real hardware — units in practice, settling/completion behavior, alarm semantics, safe write ranges — is **not** validated and still needs a hardware trace. |

---

## 1. Transport

| Field | Recovered value | Evidence class |
| --- | --- | --- |
| API transport | Serial | reverse engineered |
| Line settings | **8 data bits, no parity, 1 stop bit, no flow control**; CTS ignore, DSR ignore, DTR off, RTS off, XON/XOFF disabled | reverse engineered |
| Open mode | `sp_open(port, 3)` = `SP_MODE_READ_WRITE`, then `sp_flush(BOTH)` | reverse engineered |
| Baud rates tried | `115200`, then `4800` | reverse engineered |
| Terminator | Carriage return `0x0D` on both directions. **No LF.** | reverse engineered |
| Write timeout | 200 ms per `sp_blocking_write`, followed by `sp_drain` | literal `0xc8` at every write site |
| Reply timeout | 200 ms for a bare-CR probe; **500 ms** once a command code has been written | reverse engineered |
| Read granularity | One byte per `sp_blocking_read(port, buf, 1, 199)`, looped until CR or deadline | `0xc7` = 199 ms |
| Max reply | 199 bytes of payload + NUL into a 200-byte channel buffer | index guard `cmp edx,0xc7` |
| Max request payload | 201 bytes (`0xc9`), truncated by `strncpy`/`snprintf` | reverse engineered |
| Concurrency | One transaction at a time per port: `EnterCriticalSection`, then spin `Sleep(1000)` while an in-flight flag is set | reverse engineered |

### Auto-reconnect on write failure

Every write site has the same recovery path: if `sp_blocking_write` returns
negative, the vendor implementation does `sp_flush(BOTH)` → `sp_close` → `sp_free_port` →
`sp_free_config` → `_SerialConnect()` (re-open with the same settings) and
**retries the identical write once**. A second failure returns comm error.
This means a driver that reimplements the protocol will see the controller
tolerate a port re-open mid-session without any re-handshake.

---

## 2. Frame grammar

A command is always identified by a **numeric command code**, rendered as
exactly three decimal digits with `snprintf("%03d")`. Codes are `unsigned int`
and the shipped database contains codes up to 4 digits (for example `544`,
`1051`) — `%03d` does not truncate, it only zero-pads, so a 4-digit code emits
4 characters. Any parser must therefore treat the code field as *variable
length* in the general case even though the vendor implementation's own reply validation assumes
3 characters (see §2.3 quirk).

### 2.1 Plain Frame Grammar

This path is used when checksum support is disabled or not present.

| Operation | Bytes on the wire |
| --- | --- |
| Read | `DDD` `\r` |
| Write | `DDD` `<payload>` `\r` |
| RPC | `DDD` `\r` (indistinguishable from a read in plain mode) |
| Status probe | `\r` (bare CR, no code) |

Writes are emitted as three separate `sp_blocking_write` + `sp_drain` calls:
the code, then the payload if non-empty, then the terminator. The command type
(`G`/`S`/`R`) is **not** transmitted in plain mode — the device distinguishes a
read from a write purely by whether a payload precedes the CR.

Success reply: `DDD` `<payload>` `\r`. the vendor implementation validates that the first three
received bytes equal the three code characters it sent, then copies everything
from offset 3 up to (not including) the CR as the value.

### 2.2 Checksum Frame Grammar

External notes record the checksum-mode frame builder and parser.

`eCommandType` maps to a leading type character:

| `eCommandType` | Char | Meaning |
| --- | --- | --- |
| `0` | `G` (`0x47`) | Get / read |
| `1` | `S` (`0x53`) | Set / write |
| `2` | `R` (`0x52`) | RPC / execute |
| other | — | rejected locally with internal code 15 |

Request:

```text
<type> <DDD> <payload> '#' <sum_hi> <sum_lo> '\r'
```

Status probe in checksum mode is `'#' 0x00 0x00 '\r'` — i.e. an empty body with
a zero checksum.

Reply:

```text
<type> <DDD> <payload> '#' <sum_hi> <sum_lo> '\r'
```

#### Checksum algorithm

The checksum is a 16-bit value transmitted as two raw bytes, high byte first.
It is the sum of the **signed 8-bit** values of the frame body, where the body
is *everything after the type character and before the `#`* for normal
three-digit command codes:

```text
sum = Σ (int8_t) c   for c in DDD ++ payload
sum_hi = (sum >> 8) & 0xFF
sum_lo =  sum       & 0xFF
```

Send side: external note sign-extends `buf[1]`, `buf[2]`, `buf[3]` (the three
digits) and adds them; external note adds the payload bytes with `movsx` /
`pcmpgtb` sign extension. The type character at `buf[0]` is deliberately
skipped.

Receive side: external note accumulates `movsx` (signed) bytes into the running
sum starting at received index 1 — index 0, the echoed type character, is
stored but not summed — and stops at `#`. The two bytes after `#` are read
**unsigned** and combined as `hi*256 + lo` (external note: contribution =
`b + b*255*k` with `k = 1` then `k = 0`). Mismatch yields internal code 16.

> Because the checksum bytes are raw binary, a checksum-mode frame is **not**
> ASCII-safe: `sum_hi`/`sum_lo` can be `0x00`, `0x0D`, or `0x23`. the vendor implementation's
> receive loop handles an embedded CR by only treating CR as end-of-frame once
> both checksum bytes have been consumed (`cmp ebp,-2`) or when
> the frame starts with `E`.

#### Reply validation in checksum mode

1. length ≤ 1 → malformed (internal 11)
2. first byte is `'E'` → parse as an error string (§4)
3. length ≤ 3 → malformed
4. computed sum ≠ received sum → internal 16
5. `strncmp(reply, "<type><DDD>", 4) != 0` → malformed
6. otherwise payload = `reply[4 .. len-3]`, i.e. total length minus the
   4-byte header minus `#` and the two checksum bytes

### 2.3 Known vendor implementation quirks worth reproducing or deliberately not reproducing

- The reply header comparison is hard-coded to **3 bytes** in plain mode and
  **4 bytes** in checksum mode. For a 4-digit command code the vendor implementation therefore
  only checks a prefix and then strips a fixed 3 (or 4) bytes, which would
  leave the trailing code digit inside the returned value. Either the affected
  4-digit codes are never used on a real reply path, or the vendor tolerates it.
  Treat 4-digit codes as unvalidated until a hardware trace exists.
- The checksum request builder has a related 4-digit-code quirk: it writes all
  code digits produced by `snprintf("%03d")`, but the send-side checksum seed
  only adds `buf[1]`, `buf[2]`, and `buf[3]` before adding payload bytes. A
  checksum-mode request for a 4-digit code would therefore transmit the fourth
  code digit but not include it in the outgoing checksum. The receive-side
  checksum validator does sum every byte after the type until `#`, so replies
  with 4-digit echoed codes are especially unvalidated.
- In plain mode a read and an RPC produce byte-identical traffic.
- On a bare-CR status probe in plain mode, the vendor implementation compares the reply against
  an all-zero 4-byte buffer, so a well-formed data reply to a bare CR would be
  reported as malformed. The bare CR is only ever used as a liveness probe.

---

## 3. Session and identification sequence

This is the full open sequence recorded in the external notes.

### 3.1 Port selection

The vendor implementation follows this sequence:

1. Reject if the port is already open by this library (`"Port already used"`).
2. Create the device object and run `ConnectFiltered(name_filter)` (§3.2–3.4).
3. Run `PropertiesDiscover` (§5).
4. If USB-only filtering is enabled (`oko_LibSetSuggestedUSBOnly`), query

   ```sql
   SELECT DISTINCT vid, pid FROM UsbInfo
     INNER JOIN Product ON UsbInfo.prodLineID = Product.prodLineID
    WHERE Product.name LIKE '%<name_filter>%'
   ```

   then `sp_get_port_transport(port)` and accept only
   `SP_TRANSPORT_USB` (`1`) ports whose VID/PID is in that set.
5. `PropertiesGetNumber()`, then `ModulesDetect()` (§6).

The shipped `UsbInfo` table:

| vid | pid | speed | product line |
| --- | --- | --- | --- |
| 1027 (`0x0403` FTDI) | 24577 | 4800 | Bold Line |
| 1027 | 24577 | 115200 | Bold Line |
| 1027 | 24577 | 4800 | CO2-UNIT-XL |
| 1027 | 24577 | 115200 | H401-T |
| 1027 | 24577 | 115200 | UNO |
| 1003 (`0x03EB` Atmel) | 9220 (`0x2404`) | 115200 | TC-7D |
| 1003 | 9220 | 115200 | H402-T |
| 1003 | 9220 | 115200 | UNO-CONTROLLER |
| 0 | 0 | 115200 | H402-T, UNO-CONTROLLER, H401-T, UNO |

`speed` is informational — the vendor implementation does not read it. Baud is discovered by the
scan in §3.2.

### 3.2 Baud discovery / liveness probe

The vendor implementation walks the baud list **twice** (a two-pass retry;
the retry index starts at 2 and is reset to 1 for a second full sweep). For each
baud:

1. `sp_flush(BOTH)`, `sp_close`, `sp_free_port`, `sp_free_config`
2. `_SerialConnect()` with the candidate baud
3. send a **bare CR** and read the reply

The port is accepted when the probe returns internal `12` (success) **or**
internal `2`, which is the error reply `E3`. In practice an idle Okolab
controller answers a bare CR with `E3\r`; that is the liveness signature.

The vendor implementation confirms this: it sends a bare CR and only
sets the "device alive" flag when the reply is exactly `E3`.

```text
host -> DDD-less:  0D
dev  -> "E3" 0D                 ; device present and idle
```

### 3.3 Product identification

Identification is a **database-driven probe**, not a fixed identity command.

The vendor implementation opens `okolib.db` and runs:

```sql
SELECT DISTINCT t.nc, t.pl FROM (
    SELECT DISTINCT name_code AS nc, prodLineID AS pl FROM Product WHERE name LIKE '%<filter>%'
    UNION
    SELECT DISTINCT code_alt  AS nc, prodLineID AS pl FROM Product WHERE name LIKE '%<filter>%'
) AS t
WHERE t.nc IS NOT NULL AND t.nc > 0
ORDER BY pl
```

`filter` is the caller's product-name substring; `oko_DevicesDetect` /
`oko_DeviceOpen` pass an empty string, which makes `LIKE '%%'` match every
product. This yields a candidate list of **(identity command code, product
line)** pairs, grouped by product line.

For each candidate row, in order:

1. Send a **read** of `nc` — the identity command code:
   `Receive(nc, &reply)`.
2. If the read fails, move to the next row.
3. If the read succeeds, the reply string is the device's product name. Match
   it back to the catalogue:

   ```sql
   SELECT DISTINCT(Product.id) AS pid
     FROM Product LEFT JOIN AltName ON Product.id = AltName.productId
    WHERE Product.name = '<reply>' OR AltName.alt_name = '<reply>' AND name_code = <nc>
   ```

   Every matching `Product.id` is appended to the device's product-id list.
4. Once at least one row has matched, the scan stops probing *other* product
   lines and only continues within the matched line
   (`cmp eax,r13d` / `test dil,dil`).
5. If any probe returns internal `8` (reply `E10`, mapped to
   `OKO_ERR_DEV_SLAVE`), the whole scan aborts — the device is a slave on a bus
   and must not be enumerated on this port.

Identity command codes present in the shipped database:

| `name_code` | Parameter that carries it | Product lines |
| --- | --- | --- |
| `1` | `Product code` (id 351) | TC-7D, OKO-TOUCH |
| `8` | `Product code` (id 319) | UNO, UNO-CONTROLLER, LEO |
| `17` | `Product code` (id 279) / `Product code (COSC)` (id 605) | H401-T, H402-T, LEO |
| `31` | `Gas product code` (id 25) / `Product code (TK-Sens)` (id 631) | Bold Line, CO2-UNIT-XL, LEO |
| `58` | `Gas2 product code` (id 39) | Bold Line |
| `64` | `Temperature product code` (id 45) / `Product code (PT-Sens)` (id 630) | Bold Line, LEO |
| `362` | `Humidity product code` (id 255) | Bold Line |

So a Bold Line temperature unit is identified by `064\r` → `064H301 T Unit-BL\r`,
a Bold Line gas unit by `031\r` → `031CO2 Unit-BL\r`, and so on.

> **SQL precedence quirk.** In the match query, `AND` binds tighter than `OR`,
> so the `name_code` filter applies only to the `AltName` branch. A reply that
> happens to equal a `Product.name` from a *different* identity code will still
> match. A reimplementation should apply `name_code = nc` to both branches.

### 3.4 Sub-product probe

The vendor implementation then runs:

```sql
SELECT DISTINCT sp_code, subProdId FROM SubProduct
 WHERE subprodId NOT IN (0, <already-found ids>)
   AND productId IN (<found ids>);
```

For each row it sends a read of `sp_code` and adds `subProdId` to the product
list **iff the reply string is exactly `"1"`**. The shipped database has one
such rule: `sp_code = 234` detects `HM-ACTIVE-SUB` (an active humidity module)
attached to `CO2 Unit-BL` and the three `CO2-O2 Unit-BL` variants.

```text
host -> "234" 0D
dev  -> "234" "1" 0D            ; humidity sub-module present
```

### 3.5 Checksum negotiation

After the product list is non-empty:

1. `GetChecksumUsage()` — the caller's preference, set via
   `oko_DeviceSetChecksumUsage`. Default is off.
2. If the caller wants checksums, `SetChecksumPresence(true)` and re-read the
   identity code `nc` — this time framed with `G`/`#`/checksum.
3. If that read returns success, checksum mode stays on. Otherwise
   `SetChecksumPresence(false)` and the session falls back to plain framing.

Checksum framing is used only when **presence AND usage** are both true
using the two checksum flag bytes in driver state.

---

## 4. Error vocabulary and result mapping

### 4.1 Wire error replies

Any reply whose first byte is `'E'` is an error string. External notes record
this decode table:

| Wire reply | Internal code | Mapped `oko_res_type` |
| --- | --- | --- |
| `E1` | 0 | `OKO_ERR_NOTSUPP` (-4) |
| `E2` | 1 | `OKO_ERR_COMM` (-13) |
| `E3` | 2 | `OKO_ERR_COMM` (-13) — but treated as *device alive* by the probes |
| `E4` | 3 | `OKO_ERR_NOTSUPP` (-4) |
| `E5` | 4 | `OKO_ERR_ARG` (-2) |
| `E6` | 5 | `OKO_ERR_NOTSUPP` (-4) |
| `E7` | 6 | `OKO_ERR_COMM` (-13) |
| `E8` | 11 | `OKO_ERR_COMM` (-13) |
| `E9` | 7 | `OKO_ERR_DEV_NOTRUNNING` (-17) |
| `E10` | 8 | `OKO_ERR_DEV_SLAVE` (-16) |
| `E15` | 13 | `OKO_ERR_COMM` (-13) |
| `E16` | 14 | `OKO_ERR_COMM` (-13) |
| `E17` | 15 | `OKO_ERR_COMM` (-13) |
| `E18` | 16 | `OKO_ERR_COMM` (-13) |
| any other `E…` | 11 | `OKO_ERR_COMM` (-13) |

There is no `E11`-`E14` handler.

Behavioural reading of the codes, from how the vendor implementation reacts to them:

- `E1` — command not supported by this unit. the vendor implementation will retry it twice and
  then, if invalid-command checking is armed, give up permanently.
- `E3` — the canonical "I am here but that was not a command" reply. It is what
  a bare CR gets, and it is accepted as proof of liveness.
- `E5` — argument rejected (out of range / bad format).
- `E9` — device present but not running.
- `E10` — device is a slave; do not enumerate it on this port.

### 4.2 Internal (non-wire) codes

| Internal | Meaning | Mapped `oko_res_type` |
| --- | --- | --- |
| 9 | Reply timeout (deadline expired with no CR) | `OKO_ERR_TIMEOUT` (-19) |
| 10 | Serial write failed after one reconnect+retry | `OKO_ERR_PORT_NOTVALID` (-9) |
| 11 | Malformed reply: too short, or header did not echo the sent code | `OKO_ERR_COMM` (-13) |
| 12 | **Success** | `OKO_OK` (0) |
| 15 | Bad `eCommandType` (local programming error) | — |
| 16 | Checksum mismatch | `OKO_ERR_COMM` (-13) |

libserialport failures map as follows:
`SP_ERR_SUPP → OKO_ERR_NOTSUPP`, `SP_ERR_MEM → OKO_ERR_MEMORY`,
`SP_ERR_FAIL → OKO_ERR_FAIL`, `SP_ERR_ARG → OKO_ERR_PORT_NOTVALID`,
anything else → `OKO_ERR_UNDEF` (-999). `sp_open` returning
`SP_ERR_ARG`-class error code 5 is reported as `OKO_ERR_PORT_BUSY` (-7).

### 4.3 Retry policy

The vendor implementation:

1. If the "device alive" flag is clear, run `_CheckWorkingStatus()` first; if
   that does not return success, fail immediately.
2. Loop up to **20 attempts**:
   - issue one plain or checksum transaction;
   - **return immediately** on internal `3,4,5,6,7,8,12` (and on `2` when the
     command code is 0);
   - otherwise (`0,1,2,9,10,11,13,14,15,16`) run `_CheckWorkingStatus()`; if
     that fails, return its error, else retry.
3. Special case: internal `0` (`E1`) after the second attempt, with
   invalid-command checking armed, returns `E1` without further retries.

`_CheckWorkingStatus` itself retries the bare-CR probe **10 times** if the
device was previously known-alive, **2 times** otherwise, and clears the alive
flag when it gives up.

Net effect: a single property read can legitimately generate up to ~20
command frames interleaved with up to ~10 bare-CR probes each. Any
reimplementation should choose a much tighter budget and document it.

---

## 5. Reading `okolib.db`

The database is the command dictionary. It is opened by name **`okolib.db`**
relative to the process working directory (the vendor implementation,
global string), with `sqlite3_open_v2`. It is read-only in
practice — the vendor implementation never writes to it.

### 5.1 Schema roles

| Table | Rows | Role |
| --- | --- | --- |
| `ProdLine` | 10 | Product families (`Bold Line`, `H401-T`, `H402-T`, `UNO`, `TC-7D`, `LEO`, …) |
| `Product` | 44 | One row per marketed unit: `name`, `name_code`, `code_alt`, `prodLineID` |
| `AltName` | 15 | Alternative reply strings that map to the same `Product` |
| `SubProduct` | 4 | `sp_code` probe → `subProdId` to add when the reply is `"1"` |
| `UsbInfo` | 12 | Per-product-line USB VID/PID and nominal baud |
| `VarType` | 5 | `0 Undefined, 1 String, 2 Integer, 3 Floating, 4 Enumerator` |
| `Parameters` | 660 | The command dictionary — see §5.2 |
| `ProductVar` | 2116 | Which parameters each product exposes (many-to-many) |
| `EnumType` / `EnumValues` | 40 / 164 | Named value sets for `var_type = 4` |

**Command codes are not globally unique.** `read_code = 1` is `Product code`
for TC-7D but `Temperature 1` for another line. A code is only meaningful in
the context of a specific `Product.id`. Resolution is always
`Product → ProductVar → Parameters`.

### 5.2 The `Parameters` row

| Column | Wire role |
| --- | --- |
| `name` | Property key used by the SDK API |
| `unit` | Display unit (`°C`, `%`, `bar`, `ml/min`, `mbar`, `rpm`, `l/min`, `V`, `W`, `s`, `min`, `day`, …) |
| `description` | Human text |
| `var_type` | Value encoding (see `VarType`) |
| `main` | Show in a summary/primary view |
| `advanced` | Hide behind an "advanced" flag |
| `oneshot` | Marks identity-like values (product code, serial number, software version). **the vendor implementation does not read this column** — see §5.4. Useful for us: it identifies values that need reading only once. |
| `read_code` | Command code for a read (`G`). `0` = not readable → property is write-only. |
| `write_code` | Command code for a persistent (EEPROM) write (`S`). `0` = not writable → property is read-only. |
| `write_code_ram` | Command code for a volatile (RAM) write (`S`). Used by the `oko_PropertyWriteVolatile*` entry points. |
| `min_code` | Command code whose **read** returns the minimum settable value |
| `max_code` | Command code whose **read** returns the maximum settable value |
| `enum_type_id` | `EnumType.id` when `var_type = 4` |

Derived flags, exactly as `PropertiesDiscover` computes them
:

```text
read_only  = (write_code == 0)
write_only = (read_code  == 0)
has_limits = (min_code != 0 && max_code != 0)
write_type = (write_code != 0 ? EEPROM : 0) | (write_code_ram != 0 ? VOLATILE : 0)
```

Note `read_only` deliberately ignores `write_code_ram`: a property with only a
volatile write is reported read-only by `oko_PropertyGetReadOnly` but is still
writable through `oko_PropertyWriteVolatile*`.

### 5.3 The property-discovery query

The vendor implementation issues exactly:

```sql
SELECT DISTINCT V.id, V.name, V.unit, V.description, V.advanced, V.oneshot,
                V.main, V.read_code, V.write_code, V.write_code_ram,
                V.min_code, V.max_code, V.var_type, V.enum_type_id
  FROM Parameters V
 INNER JOIN ProductVar ON ProductVar.variablesId = V.id
 WHERE ProductVar.productId IN (-1, <id>, <id>, …)
   -- appended only when debug properties are disabled:
   AND (V.name NOT LIKE '[DBG]%' OR V.name NOT LIKE '[PROD]%')
```

and, for each row with `var_type = 4`:

```sql
SELECT E.id, E.enum_value, E.enum_name FROM EnumValues E WHERE E.enum_type_id = <id>
```

The `IN (-1, …)` list is the product-id list built in §3.3–3.4, so a device
that matched both a base product and a sub-product gets the union of both
parameter sets.

> **Filter quirk.** `A NOT LIKE x OR A NOT LIKE y` is a tautology for any name
> that is not both `[DBG]…` and `[PROD]…` — which is impossible. The debug
> filter therefore never removes anything. If we want it to work the operator
> must be `AND`. (SQLite `LIKE` does not treat `[` as a wildcard, so the
> bracket prefixes match literally.)

### 5.4 Post-processing the row

- `unit` has every byte equal to `0xC3` stripped. In the shipped
  database `°C` is stored as UTF-8 `C2 B0 43`, which contains no `0xC3`, so
  this is a no-op here — it looks like defensive handling of a mojibake
  (`C3 82 C2 B0`) seen in some database revision. Reproduce it only if a
  hardware trace shows a database that needs it.
- `oneshot` (column index 5) is fetched by the query but **never read** from
  the statement — `PropertiesDiscover` reads columns 1,2,3,4,6,7,8,9,10,11,12,13
  and skips 5. the vendor implementation therefore re-polls identity strings like `Product code`
  on every update cycle. We should honour `oneshot` and cache instead.
- `id` (column 0) is also fetched and unused.

### 5.5 A worked example

Product `H301 T Unit-BL` (`Product.id = 5`, product line `Bold Line`,
`name_code = 64`) exposes 70 parameters. Selected rows:

| name | unit | var_type | read | write | write_ram | min | max |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `Temperature` | `°C` | 3 float | 48 | – | – | – | – |
| `Temperature setpoint` | `°C` | 3 float | 82 | 142 | 141 | 144 | 143 |
| `Temperature status` | — | 4 enum (`Status`) | 128 | – | – | – | – |
| `Temperature product code` | — | 1 string | 64 | – | – | – | – |
| `Temperature serial number` | — | 1 string | 67 | – | – | – | – |
| `Objective Heater temperature` | `°C` | 3 float | 189 | – | – | – | – |

Resulting traffic, plain mode. **The command codes and their order are
recovered evidence; the reply payload values below are illustrative examples,
not captured bytes** — no hardware trace exists yet, so the exact
numeric formatting is unknown:

```text
; identify
host -> "064" 0D
dev  -> "064H301 T Unit-BL" 0D

; read current temperature
host -> "048" 0D
dev  -> "048" "37.02" 0D

; read setpoint, and its allowed range
host -> "082" 0D            dev -> "082" "37.00" 0D
host -> "144" 0D            dev -> "144" "25.00" 0D     ; min_code
host -> "143" 0D            dev -> "143" "50.00" 0D     ; max_code

; volatile setpoint write (RAM), then persistent write (EEPROM)
host -> "141" "37.50" 0D    dev -> "141" … 0D
host -> "142" "37.50" 0D    dev -> "142" … 0D

; read the status enum -> integer, resolved via EnumValues(Status)
host -> "128" 0D            dev -> "128" "0" 0D          ; 0=OK 1=Transient 2=Alarm 3=Error 4=Disabled
```

The same sequence in checksum mode, for the setpoint read. The request line is
byte-exact — `"082"` is `0x30 0x38 0x32`, so
`sum = 0x30 + 0x38 + 0x32 = 0x9A`:

```text
host -> 'G' "082" '#' 0x00 0x9A 0D
dev  -> 'G' "082" "37.00" '#' <hi> <lo> 0D
```

---

## 6. Value encoding

| `var_type` | Request payload | Reply parsing |
| --- | --- | --- |
| `1` String | Raw bytes, `strncpy` into a 201-byte buffer | Raw bytes between header and terminator |
| `2` Integer | `snprintf("%d")` | `atoi`-equivalent |
| `3` Floating | `snprintf` with a caller-selected precision: `%.03f`, `%.02f`, `%.01f`, or `%0f` for anything else | `atof` (the vendor implementation uses `atof` directly) |
| `4` Enumerator | The requested enum *name* is matched against `EnumValues.enum_name` and the numeric `enum_value` is sent as an integer | Integer, resolved back to a name through `EnumValues` |

The vendor implementation selects the format; precision comes from the property
write path.

Write-code selection:

```text
volatile requested ? write_code_ram (+0x74) : write_code (+0x70)
```

Limit reads: read `min_code`, then
`max_code`, `atof` each. Both are plain reads — there is no dedicated limit
frame type.

`oko_CommandExecute` does **not**
use the `R` type. It issues a plain `Receive(read_code)` on the named property.
The `R` / RPC type is reachable only through the lower-level external-evidence
path, which nothing in the ordinary device layer calls. Parameters whose name ends in `(RPC)`
(for example id 580 `Pid-Tuning Start (RPC)`, `read_code = 0`,
`write_code = 580`) therefore cannot be triggered through `oko_CommandExecute`
as shipped — they need a write. Flag this as unresolved until a hardware trace
shows which framing the controller actually accepts.

---

## 7. Module abstraction

`oko_module_type` is a fixed four-entry enum, and each module is a hard-coded
view over named properties:

| Module | Value property | Setpoint property | "paused"/status properties | Default limits |
| --- | --- | --- | --- | --- |
| `OKO_MODULE_TEMP` (0) | `Temperature` | `Temperature setpoint` | *(none)* | 25.0 … 50.0 |
| `OKO_MODULE_CO2` (1) | `CO2` | `CO2 setpoint` | `Gas control paused` | 0.0 … 20.0 |
| `OKO_MODULE_O2` (2) | `O2` | `O2 setpoint` | `Air mode status`, `Gas control paused` | 0.0 … 20.0 |
| `OKO_MODULE_HMD` (3) | `Humidity` | `Humidity setpoint` | `Gas control paused` | 50.0 … 100.0 |

The vendor implementation:

1. For each of the four types, construct the `Module` and test
   `PropertyIsValid(<value property>)` — i.e. is that name in the set loaded in
   §5.3. If not, the module is absent.
2. Read the value property's `unit` from the database and store it as the
   module's dimension unit.
3. For each "paused" property name, attempt `PropertyUpdate(name)`; if the read
   fails, drop that name. A module with no surviving paused property reports
   `CanBeDisabled() == false`.

`ModuleGetEnabled` reads the paused property as a boolean and inverts it.
Module limits fall back to the table above when the underlying property has no
`min_code`/`max_code`.

Note the module layer adds no new wire commands — it is purely a naming
convention over the property table. A reimplementation can skip it entirely and
work from `Parameters` directly.

---

## 8. Background polling

The vendor implementation arms a
per-property poll. It runs a thread that, under
the same critical section as foreground traffic, drains a write queue
(`PropertyWrite`) and then re-reads any property whose next-due `clock()` tick
has passed (`PropertyUpdate`). Every polled read is a full request/reply
transaction with the retry policy of §4.3.

There is no push/streaming path. All telemetry is polled.

---

## 9. Implementation checklist for an SDK-free driver

Recovered and safe to implement from this document:

- Serial settings, baud scan, bare-CR liveness probe, CR framing.
- Plain and checksum request/reply grammar, including the exact checksum.
- Database-driven identification (`name_code` probe → name match) and
  sub-product probe.
- `Parameters`/`ProductVar` resolution into a per-device command dictionary,
  including read/write/volatile-write/min/max codes and enum resolution.
- Error string vocabulary and its mapping to typed errors.

Needs a hardware trace before claiming hardware-complete status
(`docs/reverse/trace-capture-guide.md`):

| Gap | What the trace must show |
| --- | --- |
| Reply formats | Actual numeric formatting, decimal separator, sign, and unit handling for float properties on at least one temperature and one gas unit |
| Write completion | Whether the reply to a write echoes the value, is empty, or is an ACK; and how long a setpoint takes to be reflected in the readback |
| Stability / settling | What `Temperature status` (enum `Status`: OK / Transient / Alarm / Error / Disabled) does across a setpoint change — this is the only visible completion signal |
| Faults and alarms | Real `E…` replies from a live unit, sensor disconnect, over-range, gas alarm, interlock |
| 4-digit codes | Whether any product actually uses a >3-digit code on a reply path, given the vendor implementation's fixed-width header check |
| RPC framing | Whether `(RPC)` parameters are triggered by an `S` write or an `R` frame |
| Checksum mode | That a real controller accepts `G`/`S` framing and the byte-exact checksum above |
| Safe ranges | That `min_code`/`max_code` reads return usable bounds, and what the controller does with an out-of-range write (expected `E5`) |

Until those exist, the Okolab driver exposes the configured/read-write protocol
surface recorded in `docs/reverse/okolab.md` without claiming validated
settling, alarm, or safe-range behavior.
