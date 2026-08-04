# 3Z Optics IRIS Modbus-Style Protocol

This is a source-backed protocol specification for the Micro-Manager
`3Z_Optics` adapter. It is not a hardware validation note. Official 3Z product
pages confirm IRIS light-source serial control and device-level specifications,
but no ungated official register map was found during the August 2026 source
audit.

## Evidence Identity

| Field | Value |
| --- | --- |
| Primary implementation evidence | Micro-Manager `DeviceAdapters/3Z_Optics` source |
| Official product evidence | 3Z Optics IRIS-400, IRIS-400HP/P, IRIS-600HP/P product pages and download page |
| Evidence class | Audited open adapter source plus official product-level serial-control claims |
| Hardware validation | Pending |

Sources:

- <https://github.com/micro-manager/mmCoreAndDevices/tree/main/DeviceAdapters/3Z_Optics>
- <https://3zoptics.com/products/index2.html>
- <https://3zoptics.com/products/index12.html>
- <https://3zoptics.com/products/index3.html>
- <https://3zoptics.com/service/download.html>

## Transport

The Micro-Manager adapter uses a user-selected serial port carrying
Modbus-RTU-style request and response frames.

| Parameter | Value |
| --- | --- |
| Transport | USB serial / serial port |
| Slave address | `0x01` |
| Retry behavior | up to 3 command attempts in the adapter |
| Inter-attempt wait | 50 ms in the adapter |
| Post-write wait | 100 ms in the adapter |
| Checksum | CRC-16/MODBUS, polynomial `0xA001`, initial `0xffff`, CRC low byte first |
| Serial line settings | Not set by the adapter; configure the runtime serial endpoint for the attached controller |

## Frame Format

Requests and replies use standard Modbus RTU byte order for fields: register
addresses, counts, and values are big-endian; CRC bytes are little-endian.

```
read request  : 01 <func> <addr_hi> <addr_lo> <count_hi> <count_lo> <crc_lo> <crc_hi>
write request : 01 <func> <addr_hi> <addr_lo> <value_hi> <value_lo> <crc_lo> <crc_hi>
read reply    : 01 <func> <byte_count> <payload...> <crc_lo> <crc_hi>
write reply   : 01 <func> <addr_hi> <addr_lo> <value_hi> <value_lo> <crc_lo> <crc_hi>
```

The adapter clears the serial port before sending each command and reads the
exact expected response length.

## Function Codes

| Function | Meaning in adapter | Request payload | Reply payload |
| --- | --- | --- | --- |
| `0x01` | Read coil(s) | start address, count | packed bits |
| `0x03` | Read holding register(s) | start address, count | big-endian `u16` values |
| `0x04` | Read input register | address, count `1` | one big-endian `u16` |
| `0x05` | Write single coil | address, `0xff00` or `0x0000` | echoed write request |
| `0x06` | Write single holding register | address, big-endian `u16` value | echoed write request |

## Register And Coil Map

| Address | Function(s) | Meaning |
| --- | --- | --- |
| `0x01` | `0x04` read input register | Device model id |
| `0x20` | `0x03` read / `0x06` write holding register | Mode: `1 = Global`, `2 = Independent`, `3 = TTL` |
| `0x21` | `0x01` read coil | Dirty/status-change bit polled by the adapter |
| `0x30` | `0x01` / `0x05` coil | Global switch |
| `0x30` | `0x03` / `0x06` holding register | Global intensity scalar |
| `0x31 + n` | `0x01` / `0x05` coil | Channel `n + 1` switch |
| `0x31 + n` | `0x03` / `0x06` holding register | Channel `n + 1` intensity scalar |

The Micro-Manager adapter indexes channels from `0` internally and presents
channel names from model metadata. The first wire channel is therefore
`0x31`.

## State Model

| Mode | Readback behavior | Write behavior |
| --- | --- | --- |
| `Global` | Read global switch coil `0x30` and global intensity register `0x30` | Global enable writes coil `0x30`; global intensity writes register `0x30` |
| `Independent` | Batch-read channel coils and registers starting at `0x31` | Per-channel enable writes coil `0x31 + n`; per-channel intensity writes register `0x31 + n` |
| `TTL` | Same channel readback path as independent mode in the adapter | Per-channel state/intensity writes use the same channel addresses |

The adapter keeps a software shutter state. Closing the shutter writes every
channel switch coil off, and in global mode also writes the global switch off.
Opening the shutter in global mode writes the global switch on and then applies
each channel switch as `channel_enabled && shutter_open`.

## Model Metadata

The adapter reads model id from input register `0x01`, then loads local JSON
metadata named `models.json` from several possible locations. Each model entry
contains:

| Field | Meaning |
| --- | --- |
| `name` | Display/product name |
| `channels` | Channel display labels |
| `BrightnessMin` or `brightnessMin` | Minimum brightness scalar |
| `BrightnessMax` or `brightnessMax` | Maximum brightness scalar |

If no metadata is available, the adapter falls back to an unknown model, generic
channel names, and a `0..100` brightness range.

## Numanager Mapping

`numanager_drivers::three_z_optics` implements the mapped command surface from
this note. The public properties use typed runtime values:

| Runtime property | Wire mapping |
| --- | --- |
| Hub `model_id` | input register `0x01` |
| Hub `mode` | holding register `0x20` |
| Hub `enabled` | global switch coil `0x30` in global mode; otherwise software shutter plus channel writes |
| Hub `global_intensity` | holding register `0x30` in global mode |
| Hub `dirty` | coil `0x21` |
| Channel `enabled` / `selected` | coil `0x31 + n` |
| Channel `intensity` | holding register `0x31 + n` |

The implementation is source-backed, not bench-validated. Hardware validation
should record the exact IRIS model, firmware/software version, serial settings,
model id, model metadata, command completion/fault behavior, and visible output
or optical readback for enable/intensity changes.
