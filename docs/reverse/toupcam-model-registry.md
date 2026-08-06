# Reverse Note — ToupTek camera catalogue

## Target And Status

| Field | Value |
| --- | --- |
| Target | ToupTek camera catalogue — identity and geometry per model variant |
| Device page | `docs/devices/toupcam.md` |
| Related notes | [`toupcam-protocol.md`](toupcam-protocol.md), [`toupcam-u3cmos03100kpa.md`](toupcam-u3cmos03100kpa.md) |
| Artifact | `crates/numanager-drivers/src/toupcam_models.tsv` |
| Source | The ToupTek USB camera interface specification, §14 (maintained outside this repository) |
| Status | promote (identity + geometry) / needs more work (per-model open sequences) |

## Contents

1337 variants, vendor id `0x0547` throughout. Columns: model name, vendor id,
product id, full-frame width and height, pixel pitch in µm, and the preview
resolution list as `WxH@fps` entries.

```
U3CMOS03100KPA           0x0547 0x3310 2048 1534 2.2  2048x1534@28;1024x770@60
U3CMOS03100KPA(USB2.0)   0x0547 0x4310 2048 1534 2.2  2048x1534@28;1024x770@60
```

This makes identification independent of having the physical model: a camera in
the catalogue but without a supported open path fails at open with its model
name and geometry instead of hanging until the frame timeout.

## Validation

Two entries are independently verifiable against captured traffic from physical
devices, and both match exactly:

| Model | Catalogue says | Independent evidence |
| --- | --- | --- |
| U3CMOS03100KPA | pid `0x3310`, 2048 x 1534, 2.2 µm | captured 3 141 633-byte frames = 2048 x 1534 + 1-byte trailer |
| U3CMOS08500KPA | pid `0x13a1`, 3328 x 2548, 1.55 µm | pre-existing driver constants from a separate bench capture |

## Known Gaps

| Gap | Detail |
| --- | --- |
| Missing geometry | 111 rows carry a name and product id but no width/height/pitch. |
| Duplicate names | 992 distinct names across 1337 rows: one name can cover several hardware revisions with different product ids and pixel pitches (`U3CMOS08500KPA` is `0x13a1` @1.55 µm and `0x3850` @1.67 µm). **Look up by product id, not by name.** |
| Still resolutions | Only the preview list is carried. |
| Feature flags | Not carried. |
| Open sequences | Not carried. Streaming still needs a per-model register map or a recorded open sequence; the catalogue supplies identity and geometry only. |
