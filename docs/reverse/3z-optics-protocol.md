# 3Z Optics IRIS Modbus-Style Protocol

Interface specification for the IRIS light-source serial control protocol. Not a
hardware validation note.

## Evidence Identity

| Field | Value |
| --- | --- |
| Evidence class | Audited open-source host implementation plus official product-level serial-control claims |
| Official product evidence | 3Z Optics IRIS-400, IRIS-400HP/P, IRIS-600HP/P product and download pages |
| Register map | No ungated official register map found (August 2026 audit) |
| Hardware validation | Pending |

Sources: <https://3zoptics.com/products/index2.html>,
<https://3zoptics.com/products/index12.html>,
<https://3zoptics.com/products/index3.html>,
<https://3zoptics.com/service/download.html>

## Transport

Modbus-RTU-style request/response frames over a serial port.

| Parameter | Value |
| --- | --- |
| Transport | USB serial / serial port |
| Slave address | `0x01` |
| Retries | up to 3 command attempts, 50 ms between attempts |
| Post-write wait | 100 ms |
| Checksum | CRC-16/MODBUS, polynomial `0xA001`, initial `0xffff`, CRC low byte first |
| Line settings | Not fixed by the protocol; configure the runtime endpoint for the attached controller |

## Frame Format

Standard Modbus RTU byte order: addresses, counts, and values big-endian; CRC
little-endian. The host clears the port before each command and reads the exact
expected response length.

```
read request  : 01 <func> <addr_hi> <addr_lo> <count_hi> <count_lo> <crc_lo> <crc_hi>
write request : 01 <func> <addr_hi> <addr_lo> <value_hi> <value_lo> <crc_lo> <crc_hi>
read reply    : 01 <func> <byte_count> <payload...> <crc_lo> <crc_hi>
write reply   : 01 <func> <addr_hi> <addr_lo> <value_hi> <value_lo> <crc_lo> <crc_hi>
```

## Function Codes

| Function | Meaning | Request payload | Reply payload |
| --- | --- | --- | --- |
| `0x01` | Read coil(s) | start address, count | packed bits |
| `0x03` | Read holding register(s) | start address, count | big-endian `u16` values |
| `0x04` | Read input register | address, count `1` | one big-endian `u16` |
| `0x05` | Write single coil | address, `0xff00` or `0x0000` | echoed request |
| `0x06` | Write single holding register | address, big-endian `u16` | echoed request |

## Register And Coil Map

Channels are indexed from `0` host-side, so the first wire channel is `0x31`.

| Address | Function(s) | Meaning | numanager property |
| --- | --- | --- | --- |
| `0x01` | `0x04` input register | Device model id | hub `model_id` |
| `0x20` | `0x03` / `0x06` holding register | Mode: `1 = Global`, `2 = Independent`, `3 = TTL` | hub `mode` |
| `0x21` | `0x01` coil | Dirty/status-change bit, polled by the host | hub `dirty` |
| `0x30` | `0x01` / `0x05` coil | Global switch | hub `enabled` (global mode) |
| `0x30` | `0x03` / `0x06` holding register | Global intensity scalar | hub `global_intensity` |
| `0x31 + n` | `0x01` / `0x05` coil | Channel `n + 1` switch | channel `enabled` / `selected` |
| `0x31 + n` | `0x03` / `0x06` holding register | Channel `n + 1` intensity scalar | channel `intensity` |

## State Model

| Mode | Readback | Write |
| --- | --- | --- |
| `Global` | Coil `0x30`, register `0x30` | Enable → coil `0x30`; intensity → register `0x30` |
| `Independent` | Batch-read channel coils/registers from `0x31` | Enable → coil `0x31 + n`; intensity → register `0x31 + n` |
| `TTL` | Same as independent | Same as independent |

The shutter is host-side software state. Closing writes every channel switch
coil off, plus the global switch off in global mode. Opening in global mode
writes the global switch on, then each channel switch as
`channel_enabled && shutter_open`.

## Model Metadata

Model id from input register `0x01` is resolved against a local model metadata
file providing `name`, `channels` (display labels), and
`BrightnessMin`/`BrightnessMax` scalars (either capitalisation). With no
metadata: unknown model, generic channel names, `0..100` brightness range.

## Validation Status

`numanager_drivers::three_z_optics` implements the map above and is **not
bench-validated**. Hardware validation should record the exact IRIS model,
firmware version, serial settings, model id, model metadata, command
completion/fault behavior, and visible or optical readback for enable/intensity
changes.
