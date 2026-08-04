# Reverse Note — ToupTek Camera Registry from `toupcam.dll`

## Target And Status

| Field | Value |
| --- | --- |
| Target | ToupTek vendor runtime `toupcam.dll` — supported-camera table |
| Device page | `docs/devices/toupcam.md` |
| Related note | [`toupcam-u3cmos03100kpa.md`](toupcam-u3cmos03100kpa.md) |
| Status | promote (identity + geometry for 665 camera variants) / needs more work (per-model open sequences, register encoding) |
| Artifact | `crates/numanager-drivers/src/toupcam_models.tsv` |

## Why

Model support in this driver was previously one hardcoded geometry. A second
camera failed because its frame size differed, and the only way to learn a
model's geometry was to capture that physical model. The vendor runtime already
contains the full catalogue, so recovering it removes the per-model capture from
the *identification* problem (it remains for *streaming*).

## Package Identity

| Field | Value |
| --- | --- |
| File | `C:\Program Files\ToupTek\ToupView\x64\toupcam.dll` |
| SHA-256 | `488b61d108f6238e206de5c43a5131db6e0a691ce0a7070779e35eaa2a236ec6` |
| Size / build | 29 144 576 bytes, PE32+ x64, linker 14.40, timestamp 2025-12-28 |
| SDK version | `59.30405.20251228` (from `Toupcam_Version`) |
| Shipped with | ToupView 4.12.30405 |
| License boundary | The DLL is **not** redistributed or linked. Only factual interface data (model name, USB id, sensor geometry, pixel pitch, frame rates) was extracted for interoperability. No vendor code is copied into this repository. |

## Method

The DLL exports the documented public SDK (`Toupcam_EnumV2`,
`Toupcam_put_ExpoTime`, …). `Toupcam_EnumV2` returns a `ToupcamDeviceV2` whose
`model` field points at a `ToupcamModelV2` record, but it only reports
*connected* cameras, so it cannot enumerate the catalogue.

The catalogue is not a static array either: scanning for 8-byte pointers to the
model-name strings in `.rdata` finds **zero** hits, because the records are
constructed at runtime and the names are loaded with RIP-relative `lea`.

What does exist is one builder routine per model. Each fills a slot in a global
array of `0x248`-byte records:

```
lea  rax,[r9+19FA658h]          ; &record[i].name
add  rax,rdx                    ; rdx = i * 248h
lea  rcx,[rel 18176D3D8h]       ; L"U3CMOS03100KPA"
mov  [rax],rcx                  ; record.name
mov  dword [rdx+0B8h],3310h     ; USB product id
mov  dword [rdx+2Ch],400CCCCDh  ; xpixsz = 2.2f
mov  dword [rdx+30h],400CCCCDh  ; ypixsz = 2.2f
lea  rax,[rel 18176E1D8h]       ; resolution array
mov  [rdx+0C8h],rax
```

The record is `ToupcamModelV2` shifted by an 8-byte header, so the public header
layout applies: `name` at `+0x08`, `flag` `+0x10`, `maxspeed` `+0x18`, `preview`
`+0x1c`, `still` `+0x20`, `xpixsz` `+0x2c`, `ypixsz` `+0x30`. The USB product id
at `+0xb8` and the resolution pointer at `+0xc8` are beyond the public struct.

Resolutions are a separate array of 5 x `u32` per entry — confirmed by the
indexing code `lea rcx,[r9+r9*4]` / `mov r10d,[rax+rcx*4]` — of which the first
three fields are width, height, and max frame rate:

```
00 08 00 00  fe 05 00 00  1c 00 00 00 ...   -> 2048 x 1534 @ 28 fps
00 04 00 00  02 03 00 00  3c 00 00 00 ...   -> 1024 x  770 @ 60 fps
```

Extraction is therefore: linear-sweep disassemble `.text` (4 099 086
instructions), track simple register constants, split into blocks at each
`lea` of a model-name string, bound each block at its `ret`, and read the
immediates stored at the known record offsets. Displacements are encoded as
disp32 with the image base supplied by a register at runtime, so a store may
appear either as `[rdx+0B8h]` or as `[rdx+r9+19FA708h]`; both normalize to the
same record offset.

## Result

665 camera variants, each with model name, USB product id, full-frame geometry,
pixel pitch, and the full preview-resolution list. Vendor id is `0x0547`
throughout.

Recorded in `crates/numanager-drivers/src/toupcam_models.tsv`.

## Validation

Two entries are independently verifiable against evidence gathered without the
DLL, and both match exactly:

| Model | Registry says | Independent evidence |
| --- | --- | --- |
| U3CMOS03100KPA | pid `0x3310`, 2048 x 1534, 2.2 um | USBPcap capture measured 3 141 633-byte frames = 2048 x 1534 + 1; the vendor UI reported `Live: 2048 x 1534` at the same checkpoint |
| U3CMOS08500KPA | pid `0x13a1`, 3328 x 2548, 1.55 um | The pre-existing driver constants `PID_BENCH_CAM = 0x13a1`, `WIDTH = 3328`, `HEIGHT = 2548`, derived from a separate bench capture |

Neither anchor was used to guide the extraction, so they are genuine checks
rather than fitted results.

## Known Gaps

| Gap | Detail |
| --- | --- |
| Recall | Bounding each builder at its `ret` favours precision over completeness. Variants whose builder does not re-reference a name string are missed — for example `0x4310`, the USB2 twin of our `0x3310`, is absent. An unbounded pass finds more product ids but lets stores bleed between adjacent builders and produced demonstrably wrong ids, so the precise table is the one shipped. |
| Duplicate names | 481 distinct names across 665 rows: a model name can cover several hardware revisions with different product ids and pixel pitches (`U3CMOS08500KPA` appears as `0x13a1` @1.55 um and `0x3850` @1.67 um). Look up by product id, not by name. |
| Still resolutions | Only the preview list is decoded; the still-resolution array was not traced. |
| `flag` bits | Captured but not interpreted against the public `TOUPCAM_FLAG_*` constants. |
| Open sequences | **Not** recovered. Streaming still requires a per-model recorded open sequence; the registry only supplies identity and geometry. |
| Register encoding | **Not** recovered. See the U3CMOS03100KPA note: exposure/gain writes are obfuscated with a session-dependent key. The vendor transport uses raw `DeviceIoControl` IOCTLs rather than the `WinUsb_*` API, so the scrambler sits several layers below the exported API and was not traced. |

## What This Buys The Driver

A camera that is in the registry but has no recorded open sequence now fails at
open with its model name and geometry, instead of silently hanging until the
15 s frame timeout. That is the difference between "this camera is unsupported,
here is exactly which one it is" and the original failure mode.

## Next Steps

| Goal | Approach |
| --- | --- |
| Exposure/gain on U3CMOS03100KPA | Drive `Toupcam_put_ExpoTime` over a chosen value set while capturing USB, then read the exposure to wire-byte mapping directly. Because the driver replays a fixed `0x47` handshake, the session key is fixed, so a table captured that way stays valid. This needs the camera attached and is far cheaper than breaking the key schedule. |
| General register encoding | Trace from the `DeviceIoControl` call sites in the WinUSB transport back to where `wValue`/`wIndex` are computed, and recover the key schedule seeded by the `0x47`/`0x75` exchange. |
| More streamable models | One capture of the vendor application opening each model; the registry already supplies the geometry to validate the resulting frame size against. |
