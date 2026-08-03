# Filter Cube and Spectra Data Sources

Research notes on where public excitation/dichroic/emission data for filter cubes
(Zeiss, Nikon, Olympus, Leica, Chroma, Semrock, Omega, …) can be obtained, what it
contains, and under what terms it can be redistributed. Intended to inform a
database that GUIs can query. No schema is implemented here.

All API probes below were run and verified on **2026-07-27**.

## 1. What the data model needs to hold

The OME data model already standardizes this and is worth aligning with, since
downstream tools and file formats speak it:

- **Filter** — with one `TransmittanceRange` carrying `CutIn`, `CutOut`,
  `CutInTolerance`, `CutOutTolerance` (all nm) and `Transmittance` as a
  fractional percentage.
- **Dichroic** — separate object.
- **FilterSet** — references `ExcitationFilterRef`, `DichroicRef`,
  `EmissionFilterRef`.
- **LightPath** — the same three reference kinds, for the as-configured path.

Source: <https://docs.openmicroscopy.org/ome-model/6.2.2/developers/filter-and-filterset.html>

Two things OME does *not* model well and which the real data demands:

- **Multiple filters per role.** Real cubes have multiband excitation with 2–4
  separate excitation filters. In the verified Zeiss dataset (84 configs) the
  role counts were EX=130, BS=88, EM=88 — i.e. more excitation filters than
  configs, and some configs carry two beamsplitters.
- **Full transmission curves.** `CutIn`/`CutOut` is a two-number summary. Real
  use (bleedthrough, collection efficiency) needs the sampled curve.

So plan for: nominal band summary (always present, cheap) **plus** optional
measured curve (large, license-encumbered, fetched/cached).

## 2. Source-by-source assessment

### 2.1 FPbase — best single source

Community-curated database with an open GraphQL API. Verified working:

```
POST https://www.fpbase.org/graphql/     (no auth, no key)
```

Top-level query fields: `proteins`, `protein`, `spectra`, `spectrum`, `states`,
`state`, `dyes`, `dye`, `microscopes`, `microscope`, `opticalConfigs`,
`opticalConfig`, `organisms`, `references`.

**Verified content (2026-07-27):** 7583 spectra total —

| category | count | meaning |
|---|---|---|
| `F` | 4131 | filters |
| `D` | 1920 | dyes |
| `P` | 1193 | fluorescent proteins |
| `L` | 270 | light sources |
| `C` | 69 | cameras |

Filter subtypes: `BP` 2036, `BS` 802 (beamsplitter/dichroic), `BM` 517
(multiband emission), `BX` 420 (multiband excitation), `LP` 234, `SP` 122.

**Vendor coverage** of the 4131 filter spectra, by leading name token:

| vendor | count | | vendor | count |
|---|---|---|---|---|
| Omega | 1287 | | Lumencor | 31 |
| Chroma | 1111 | | Thorlabs | 14 (+3 "ThorLabs") |
| Semrock | 739 | | Olympus | 12 |
| Alluxa | 403 | | AHF | 7 |
| Zeiss | 272 | | Nikon | 3 |
| Leica | 108 | | | |

⚠️ **Coverage gap:** Nikon and Olympus are barely represented. Zeiss and Leica are
well covered; the microscope-vendor cubes most likely to be missing are Nikon
C-FL blocks and Olympus/Evident mirror units.

**`Filter` object fields:** `id`, `manufacturer`, `part`, `url`, `name`, `slug`,
`bandcenter`, `bandwidth`, `edge`, `tavg`, `aoi`, `spectrum`, `opticalConfigs`,
`filterplacementSet`, `typ`. This is exactly the nominal-band summary layer.

**`Spectrum` object fields:** `id`, `created`, `modified`, `status`, `minWave`,
`maxWave`, `scaleFactor`, `peakWave`, `category`, `subtype`, `ph`, `solvent`,
`ownerFilter`, `ownerLight`, `ownerCamera`, `reference`, `source`, `owner`,
`color`, `data`. Note `source` and `reference` — **per-record provenance is
already carried**, which is what we would want to propagate.

**Filter cubes are modeled as `opticalConfigs` on `microscope` objects.** Curated
vendor microscopes exist, e.g. "Zeiss Filter Sets" (id
`VgeWjEPrGiSL6saRi9myA8`, **84 optical configs**) and "Semrock Filter Sets" (id
`HGtCWRnyn8joPY5WF2t3zW`). Verified query shape:

```graphql
{ microscope(id:"VgeWjEPrGiSL6saRi9myA8") {
    name
    opticalConfigs { name filters { path reflects
      spectrum { id subtype peakWave color owner{name} } } } } }
```

Sample output (verified):

```
Zeiss Filter Set 38 HE-equivalents, Set 49, Set 43 HE, … 84 configs
-- Zeiss Filter Set 109 HE LED
     BS  Zeiss TBS 405+493+575           sub BS  peak 505
     EM  Zeiss TBP 425/29+514/31+632/100 sub BP  peak 636
     EX  Zeiss BP 385/30 LED             sub BP  peak 384
     EX  Zeiss BP 469/38 LED             sub BP  peak 452
     EX  Zeiss BP 555/30 LED             sub BP  peak 546
```

⚠️ **Data-quality caveat:** `peakWave` is unreliable for filters. Verified
example: `Zeiss BP 450-490` reports `peakWave: 1143`, which is nonsense for a
450–490 nm bandpass (the curve presumably extends into an IR leak or a
second passband, and the max was taken globally). **Use `bandcenter`/`bandwidth`
from the `Filter` object, or the designation string, not `peakWave`.** Any
ingest should validate peak against the nominal band and flag mismatches.

**REST API** (`https://www.fpbase.org/api/`) exists in JSON and CSV but is
protein-focused and being deprecated in favor of GraphQL. Verified:
`/api/proteins/?format=csv` → HTTP 200 (~460 KB);
`/api/filters/?format=json` → **HTTP 404**. Filters are GraphQL-only.

#### FPbase licensing — verified

Three distinct things, do not conflate them:

1. **The data.** Exact wording from the FPbase terms page:

   > "The data contained in the FPbase are free of all copyright restrictions and
   > made fully and freely available for both non-commercial and commercial use.
   > Users of the data should attribute the original authors of the corresponding
   > data (referenced on the corresponding protein page)."

   With this caveat, same page:

   > "We cannot provide unrestricted permission regarding the use of the data, as
   > some data may be covered by patents or other rights."

   And: "The user assumes all responsibility for insuring that intellectual
   property claims associated with any data deposited in FPbase are honored. It
   should be understood that FPbase data do not contain any information on
   intellectual property claims."

   Source: <https://www.fpbase.org/terms/> (rendered page is JS-gated; the
   template is at
   `backend/fpbase/templates/pages/terms.html` in the source repo).

   **Reading:** this is a public-domain-style dedication *by FPbase for what FPbase
   can dedicate*, with attribution requested and an explicit disclaimer that
   FPbase cannot clear third-party rights. It is **not** a warranty that a
   vendor's measured transmission curve is freely redistributable — a Chroma
   curve deposited into FPbase does not become unencumbered because FPbase says
   its own data is.

2. **The website/source code.** The terms page states the project is licensed
   CC BY-SA 4.0. ⚠️ **However, the repository's own `LICENSE` file is
   GNU GPL v3** (verified at
   `https://raw.githubusercontent.com/tlambert03/FPbase/main/LICENSE`). These two
   statements conflict. Irrelevant if we only consume data over the API, but it
   matters if any FPbase code is ever vendored.

3. **The Python client** `fpbase` on PyPI is BSD-3-Clause (separate project).

**Citation** (from <https://www.fpbase.org/cite/>):

> Lambert, TJ (2019) FPbase: a community-editable fluorescent protein database.
> *Nature Methods* 16, 277–278. doi:10.1038/s41592-019-0352-8

**Data Availability statement** in that paper, verbatim:

> "Although no primary data are presented here, all data collated at
> https://www.fpbase.org are available for download or upon request."

Note what this does and does not establish. It is an *availability* statement —
it commits to the data being obtainable, and is the journal-level record that
FPbase is not a closed resource. It says nothing about **reuse terms**: no
license is named, and "or upon request" implies some portion may not be behind a
plain download. So it supports the open-access character of the resource but does
not supersede or extend the terms page, which remains the authoritative statement
on what may be redistributed. No separate Zenodo or otherwise deposited FPbase
dataset was located; the site itself is the deposit.

### 2.2 Chroma — direct per-part ASCII, verified working

Stable URL pattern, no auth:

```
https://www.chroma.com/files/part_spectra/<PART>-ascii.txt
```

Verified `5270-ascii.txt` → HTTP 200. Format: two tab-separated columns,
wavelength (nm) and transmission as a fraction, **0.5 nm steps starting at
300 nm**:

```
300.00	9.5000001465451E-7
300.50	9.5000001465451E-7
301.00	9.5000001465451E-7
```

Note scientific-notation floats and that out-of-band values are ~1e-6, not 0.

Also: catalog/downloads page at <https://www.chroma.com/products/catalog-downloads/>,
and a spectra viewer exporting CSV/ASCII/PNG/XLSX.

⚠️ **Licensing: not granted.** Chroma publishes a Terms of Use; no open license
or redistribution grant was found. Treat measured Chroma curves as
**fetch-and-cache-locally, do not vendor into the repo or redistribute**.

### 2.3 Semrock / IDEX Health & Science

SearchLight tool: <https://searchlight.idex-hs.com/> — 800+ fluorophores, 150+
light sources, a dozen-plus detectors, and the full Semrock filter library.
Per-filter ASCII download is linked beside each filter's graph on the product
page; format is two columns, wavelength in nm and transmission. Most filters
carry **actual measured** data, a few are theoretical — a distinction worth
recording per record.

⚠️ **Licensing: not granted.** Same treatment as Chroma. No stable bulk-download
URL pattern was confirmed.

### 2.4 Zeiss — authoritative vendor source, but not machine-readable

The Zeiss Filter Assistant lists every filter set with its components:

```
https://www.micro-shop.zeiss.com/en/us/shop/filterAssistant/filtersets/
https://www.micro-shop.zeiss.com/en/us/shop/filterAssistant/filtersets/489038-9901-000   (Set 38 HE)
https://www.micro-shop.zeiss.com/en/us/shop/filterAssistant/filtersets/489043-9901-000   (Set 43 HE)
https://www.micro-shop.zeiss.com/en/us/shop/filterAssistant/filtersets/488049-9901-000   (Set 49)
```

The part number (e.g. `489038-9901-000`) is the catalog ID and a good primary key.

⚠️ **Verified obstacle:** these pages are JS-rendered; an automated fetch of the
Set 38 HE page returned only footer/legal content, no specifications. Scraping
would need a headless browser. Individual institutions mirror the per-set PDFs
(e.g. Stanford, University of Copenhagen host `gfp_set38he.pdf`,
`dapi_set49.pdf`, `Zeiss-FilterSet-38HE.pdf`), which are convenient but are
redistributions, not primary sources.

⚠️ **Licensing:** vendor catalog content, no reuse grant.

#### Zeiss designation grammar — derivable without any spectra

This is the most useful finding for a low-cost database. Zeiss part designations
encode the nominal bands directly, so **excitation/emission bands can be parsed
from the name alone**, with no spectral data and no licensing exposure (a part
designation is a factual identifier).

Prefix counts observed across the 235 distinct Zeiss filters in the verified
84-config dataset:

| prefix | n | meaning |
|---|---|---|
| `BP` | 111 | bandpass |
| `FT` | 38 | *Farbteiler* — dichroic beamsplitter |
| `DBP` | 23 | dual bandpass |
| `TBP` | 18 | triple bandpass |
| `DFT` | 10 | dual dichroic |
| `TFT` | 10 | triple dichroic |
| `LP` | 9 | longpass |
| `QBP` | 3 | quad bandpass |
| `TBS` | 3 | triple beamsplitter |
| `DBS` | 3 | dual beamsplitter |
| `QBS` | 2 | quad beamsplitter |
| `PBP`, `PBS` | 1 each | penta bandpass / beamsplitter |
| `BS` | 1 | beamsplitter |
| `QFT` | 1 | quad dichroic |
| `G` | 1 | glass filter (e.g. `G 365`) |

Band syntax, all observed in the data:

- `BP 470/40` — center/full-width → 450–490 nm
- `BP 390-420` — explicit range
- `DBP 425/29+514/31` — multiband, `+`-joined
- `DBP 480/22+LP 530` — mixed bandpass + longpass in one part
- `DBP 518-25+625-30` — inconsistent separator (`-` used where `/` is meant);
  a parser must tolerate this
- `FT 440/505` — a dichroic with two edges, `/`-separated (**different meaning
  from `/` in `BP`**)
- suffixes: `(HE)` or `HE` (High Efficiency — steeper edges, higher
  transmission), `LED`, `DMR 25`

⚠️ The `/` overload between `BP 470/40` (center/width) and `FT 440/505`
(dual edge) means the parser must branch on the prefix. Do not write one
regex for both.

### 2.5 Other sources

- **Leica FluoScout** — online cube selection tool; no API found.
- **Thorlabs** — publishes filter and cube spectra
  (<https://www.thorlabs.com/spectral-filters>,
  `https://www.thorlabs.com/microscope-filter-cubes`); only 14–17 entries in
  FPbase, so the vendor site is the better source.
- **AHF Analysentechnik** — present in FPbase (7 entries, e.g.
  `AHF F48-572 …`, `AHF HC 483/32 F37-483`); AHF part numbers (`F##-###`) are
  another useful key.
- **Omega Optical** — largest FPbase contributor (1287 spectra) despite low
  profile.
- **Alluxa** (403) — mostly ultra-narrow laser-line filters, not microscopy cubes.
- **Edmund Optics** — sells vendor-compatible cube sets (Nikon/Olympus/Zeiss
  mounts) with published specs; a possible way to fill the Nikon/Olympus gap.

## 3. Acquisition cost — measured

All figures below were measured against the live API on 2026-07-27, not estimated.

### 3.1 There is no bulk endpoint

`spectra(category:"F")` accepts only `category` and `subtype` args and returns
`SpectrumOwnerInfo` (fields: `name`, `slug`, `url`, `id`) — **no curve data and no
manufacturer/part**. Curve data and filter metadata are only reachable via
`spectrum(id:)`, one id at a time.

The workaround is **GraphQL aliases**: 50 `spectrum(id:)` calls batched into one
POST works fine. That reduces 4131 fetches to 83 requests.

### 3.2 Measured sizes

**Nominal layer** (id, subtype, minWave, maxWave, manufacturer, part, name,
bandcenter, bandwidth, edge, aoi — no curves), all 4131 filters:

| metric | value |
|---|---|
| records | 4131 / 4131, zero failures |
| raw JSON | **941 KiB** |
| gzip transfer | **97 KiB** |
| wall time | 120 s (83 batches of 50, 0.12 s politeness delay) |

**Full curves**, extrapolated from a verified 20-spectrum batch (351,742 bytes
raw / 98,017 bytes gzipped):

| representation | all 4131 filters |
|---|---|
| raw JSON | ~79 MB |
| gzip on the wire | ~20 MB |
| dense `u16` on a 1 nm grid | **6.6 MB** |
| trimmed `u16` (leading/trailing zeros dropped) | **4.3 MB** |

All 7583 spectra (adding dyes, proteins, light sources, cameras) roughly doubles
that: ~139 MB raw JSON, ~37 MB gzipped.

### 3.3 Curve shape

Verified across a 20-spectrum sample: **step is uniformly 1.0 nm**, but the range
varies enormously — `minWave` from 200 to 900, `maxWave` from 800 to 1500, point
counts from 300 to 1300. So a fixed global grid wastes space; store
`(min_wave, step, Vec<u16>)` per curve.

The data is also **very sparse**. Bandpass filters are mostly zeros — observed
non-zero counts as low as 6, 13, and 21 points out of ~1000. Trimming to the
non-zero span alone cuts storage by ~35%.

Out-of-band values are small non-zero floats (~1e-6 to 1e-8), not exact zeros, so
a sparsity threshold is needed rather than `== 0.0`.

### 3.4 Gotchas for any fetcher

⚠️ **`tavg` returns `nan`, which is not a valid GraphQL Float.** Requesting it
fails the *entire batch* with
`"Float cannot represent non numeric value: nan"` — 50 records lost to one bad
value. **Do not request `tavg`.** This is a live server-side bug, verified.
Any batched fetcher should also degrade gracefully: on a batch error, retry the
ids individually rather than dropping them.

⚠️ **`bandcenter`/`bandwidth` are only ~⅓ populated.** Measured across all 4131
filter records:

| field | populated |
|---|---|
| `name` | 4131 (100%) |
| `manufacturer` | 3694 (89.4%) |
| `part` | 3673 (88.9%) |
| `bandcenter` | 1415 (34.3%) |
| `bandwidth` | 1359 (32.9%) |
| `edge` | 4 (0.1%) |
| `aoi` | 2 (0.0%) |

This materially changes the plan: **the nominal band layer cannot be taken from
FPbase's structured fields.** For ~two-thirds of filters it must be parsed from
the designation string (§2.4) or derived from the curve. `edge` and `aoi` are
effectively absent and should not be relied on at all.

⚠️ **Worse: where `bandcenter` *is* populated, it is often wrong.** Of 1222
populated values checked against the designation, **604 are not plausible
wavelengths at all**. FPbase derives the field with its own parser, which
mis-reads vendor series codes:

| filter | upstream `bandcenter` | actual |
|---|---|---|
| `Semrock FF01-900/32` | 1 | 900 (the `01` of `FF01`) |
| `Semrock FF02-809/81` | 2 | 809 (the `02` of `FF02`) |
| `Chroma AT350/50x` | 35 | 350 |
| `Chroma ET365/10BP` | 36 | 365 |
| `Chroma ZET1064/10x` | 1 | 1064 |

So `bandcenter` is not merely sparse, it is **actively unreliable for Semrock and
Chroma**. It should not be used as a fallback, and it is a poor validation
oracle: restricted to the 618 plausible values, our own parser disagrees with it
just **twice**, and in both of those cases upstream is the wrong one.

Manufacturer values are clean and closed-vocabulary where present: Omega 1272,
Chroma 979, Semrock 711, Alluxa 403, Zeiss 238, Leica 59, Lumencor 29,
Raspberry Pi 3.

### 3.5 Adaptive sampling — measured on 359 real curves

Filter curves are flat-baseline / steep-edge / flat-passband, so polyline
simplification (Douglas–Peucker) should compress them well. It does, but **the
error metric matters far more than the sampling strategy**, and getting it wrong
silently destroys the most important part of the data.

Measured on a stratified sample of 360 filter curves (60 each of BP, BS, LP, SP,
BM, BX), extrapolated to all 4131. Average 700 dense samples/curve.

**Baselines:**

| representation | size (4131 filters) |
|---|---|
| dense `u16` linear, raw | 5.79 MB |
| dense `u16` linear, gzip (whole blob) | 2.62 MB |
| dense `u16` **log-quantized**, gzip | 3.26 MB |
| dense `u16` linear, **per-curve** gzip (random-access) | 2.77 MB |

**Linear-domain Douglas–Peucker** — compresses beautifully and is *wrong*:

| tolerance | verts/curve | raw | max abs err | **max relative err** |
|---|---|---|---|---|
| 1e-2 | 63.6 | 1.05 MB | 1.0e-2 | **756,000 %** |
| 1e-3 | 176.7 | 2.92 MB | 1.0e-3 | **87,000 %** |

⚠️ **This is the trap.** A tolerance of 1e-2 in linear transmission treats
1e-6 and 0 as identical — but that is the OD6 blocking region, and the ratio
between them is exactly what determines bleedthrough and crosstalk. Max log₁₀
error reached **~10 decades**. The compressed curve plots identically and is
useless for the one calculation that needs it.

Note the same trap applies to storage: **plain `u16` linear quantization has a
resolution of 1.5e-5 and also flattens OD5–OD6 blocking to zero.** Store
log-quantized (`u16` over log₁₀ ∈ [−12, 0] gives 1.8e-4 decade resolution).

**Log-domain DP** fixes blocking but breaks the passband — log-linear
interpolation across a steep edge is badly wrong in the middle:

| tolerance (decades) | verts | raw | **max abs err** | max rel err |
|---|---|---|---|---|
| 0.30 | 43.7 | 0.72 MB | **0.473** | 99.5 % |
| 0.05 | 96.6 | 1.60 MB | **0.342** | 12.2 % |

**Hybrid criterion** — split while *either* absolute or relative error exceeds
tolerance, i.e. `score = max(|Δy|/abs_tol, |Δy|/max(y,1e-6)/rel_tol)`. This is
the one that works, giving a guaranteed bound on both:

| abs_tol | rel_tol | verts | raw | +gzip | max abs | max rel |
|---|---|---|---|---|---|---|
| 0.02 | 20 % | 134.8 | 2.23 MB | **1.88 MB** | 2.0e-2 | 20.0 % |
| 0.01 | 10 % | 181.4 | 3.00 MB | **2.57 MB** | 1.0e-2 | 10.0 % |
| 0.005 | 5 % | 231.6 | 3.83 MB | 3.33 MB | 5.0e-3 | 5.0 % |
| 0.001 | 2 % | 328.7 | 5.43 MB | 4.75 MB | 1.0e-3 | 2.0 % |

**Verdict.** Against the honest baseline — dense log-quantized `u16` + gzip,
3.26 MB, which is the cheapest representation that preserves everything —
hybrid adaptive sampling gives **1.88 MB at abs 0.02 / rel 20 %**, about a
**1.7× gain**, or 2.57 MB (1.3×) at the tighter tolerance.

So adaptive sampling is worth doing, but the honest gain is ~1.3–1.7×, not the
40× the raw-JSON-to-polyline comparison suggests. The stronger arguments for it
are qualitative:

- curves are **already on inconsistent grids** (200–1500 nm, 300–1300 points), so
  a resolution-independent representation is a better fit than any fixed grid;
- evaluation touches ~135 vertices instead of 700 samples;
- interpolation at arbitrary wavelengths is native, not an afterthought;
- random access needs no decompression step, unlike a gzipped blob.

Since neither variant is anywhere near a size problem, **pick the tolerance on
fidelity grounds, not size**: abs 0.01 / rel 10 % costs 0.7 MB more than the
loosest setting and halves the worst-case error.

⚠️ **Nulls in curve data.** 1 of 360 sampled curves (id 6954, a `BS`) had
**every one of its 500 samples null**. Roughly 0.3 % of curves may be entirely
null, and partial nulls should be assumed possible. An ingest must filter nulls
before any arithmetic and drop curves left with too few points.

### 3.6 Storage format: Polars/Parquet vs hand-packed blobs

Measured with Polars 1.39.3 on the same 359-curve sample, extrapolated to 4131.
Values log-quantized to `u16` unless stated. **`compression_level` was varied —
the default (zstd 3) leaves ~14 % on the table.**

**Layout matters more than codec.** Two candidate schemas:

- **long** — one row per sample: `spectrum_id`, `wavelength_nm`, `value`
  (251,451 rows in the sample; ~2.9 M for all filters)
- **wide** — one row per spectrum: `spectrum_id`, `min_nm`, `values: List<u16>`

| layout | value type | codec | size @4131 |
|---|---|---|---|
| long | f32 | parquet uncompressed | 15.82 MB |
| long | f32 | parquet snappy | 10.19 MB |
| long | f32 | parquet zstd (default) | 7.01 MB |
| long | f32 | arrow-ipc zstd | 6.58 MB |
| long | u16 | parquet zstd (default) | 5.57 MB |
| long | u16 | parquet zstd-22 | 4.96 MB |
| **wide** | u16 | parquet zstd (default) | 4.07 MB |
| **wide** | u16 | parquet zstd-9 | 3.59 MB |
| **wide** | u16 | **parquet zstd-22** | **3.50 MB** |
| wide | u16 | arrow-ipc zstd | 4.13 MB |

Long format costs ~40 % more than wide even with RLE on `spectrum_id` — it
repeats the id and wavelength for every sample, and 2.9 M rows of bookkeeping is
not free. `f32` → log-quantized `u16` saves a further ~20 %.

**Combined with adaptive sampling** (hybrid abs 0.01 / rel 10 %, §3.5, 181
vertices/curve), stored wide as two `List` columns (`wl`, `val`):

| representation | size @4131 |
|---|---|
| parquet adaptive-wide zstd-3 | 2.42 MB |
| **parquet adaptive-wide zstd-22** | **1.97 MB** |
| arrow-ipc adaptive zstd | 2.40 MB |
| parquet adaptive-**long** zstd-22 | 2.50 MB |

**Against hand-packed blobs** (no schema, no index, offsets must be stored
separately):

| blob | size @4131 |
|---|---|
| dense `u16` + gzip-9 | 3.26 MB |
| dense `u16` + zstd-22 | 3.10 MB |
| adaptive interleaved `(x,v)` + gzip-9 | 2.41 MB |
| adaptive interleaved `(x,v)` + zstd-22 | 2.30 MB |

**Parquet wins once the data is adaptive: 1.97 MB vs 2.30 MB for the equivalent
hand-packed blob** — while also carrying a schema, per-spectrum random access,
and a native Rust reader.

The reason is instructive: the hand-packed blob interleaves `(wavelength, value)`
pairs, which defeats compression. Parquet stores them as **separate columns**, so
the wavelength column — monotonically increasing small integers — delta/RLE
compresses almost to nothing. Columnar separation is exactly the right structure
for this data. (For *dense* curves Parquet is slightly behind, 3.50 vs 3.10 MB,
because a dense blob needs no wavelength column at all — the index is implicit.
That advantage disappears the moment sampling is non-uniform.)

**Should Parquet also be compressed externally? No.**

| test | result |
|---|---|
| gzip on top of zstd parquet | 0.484 → 0.478 MB (**98.7 %** — saves 1.3 %) |

Parquet already compresses per column chunk. Wrapping it gains nothing and costs
the things Parquet is chosen for: `mmap`, random access, predicate pushdown, and
lazy scanning. Use Parquet's internal codec and leave the file alone.

**Recommendation:** adaptive-sampled, **wide layout**, log-quantized `u16`,
**Parquet with zstd level 22**, no external compression → **~2 MB for every
filter curve in FPbase**, readable directly by the Rust `polars` crate. Write
with a high compression level (it is a build-time cost paid once); reads are
unaffected by the level.

⚠️ **Clamp before quantizing.** Verified in the sample: 4/359 curves peak
**above 1.0** (max 1.0043) and **54/359 (15 %) contain negative values** (down to
−0.0046). These are raw measured data with baseline noise, not idealized curves.
Feeding them to `log10` without clamping to `[1e-12, 1.0]` produces NaN or
`u16` overflow — the latter was hit during this benchmark. Wavelengths in the
sample also span **191–1800 nm**, wider than the 300–1000 nm typically assumed,
though still within `u16`.

### 3.7 Cube-level data is cheap

Top-level `opticalConfigs` (no args) returns **11053 optical configs** across all
public microscopes — 38 KiB gzipped for names alone. The curated vendor
microscopes (§2.1) are the subset worth ingesting; the rest are user-built rigs.

## 4. Licensing summary

| Source | Data reuse | Notes |
|---|---|---|
| **FPbase** | ✅ "free of all copyright restrictions… non-commercial and commercial use", attribution requested | Explicit disclaimer that third-party/patent rights are not cleared |
| FPbase source repo | ⚠️ GPLv3 in `LICENSE`, CC BY-SA 4.0 claimed on terms page — conflicting | Only matters if vendoring code |
| **Chroma** | ❌ no grant found | Fetch/cache only, do not redistribute |
| **Semrock/IDEX** | ❌ no grant found | Fetch/cache only |
| **Zeiss / Nikon / Olympus / Leica** | ❌ no grant found | Catalog content |
| **Part designations & nominal bands** | ✅ facts, not copyrightable | Safe to ship |
| **OME data model** | ✅ open specification | Safe to align with |

The practical consequence: **ship the nominal layer, cache the measured layer.**
Part numbers, designations, parsed band centers/widths, and cube→filter
relationships are facts and can live in the repo. Measured transmission curves
from vendors should be fetched at runtime into a local cache with the source URL
and retrieval date recorded, never committed.

## 5. Provenance fields every record should carry

Following FPbase's own precedent (`Spectrum.source`, `Spectrum.reference`) and
the licensing situation above, each record wants:

- `source` — enum: `fpbase` | `chroma` | `semrock` | `zeiss_catalog` |
  `derived_from_designation` | `user`
- `source_url` — exact URL fetched
- `source_id` — upstream primary key (FPbase spectrum id, Chroma part number,
  Zeiss catalog number like `489038-9901-000`)
- `retrieved` — date (data is revised upstream; FPbase exposes `modified`)
- `license` — recorded verbatim per source, not inferred
- `measurement_kind` — `measured` | `theoretical` | `nominal_from_designation`
  (Semrock explicitly mixes measured and theoretical; designation-derived bands
  are neither)
- `redistributable` — boolean, driving whether the record may be bundled

## 6. Recommendation

**Size is not the deciding factor.** Even the full curve set packs to 4.3 MB,
which is bundleable. Licensing and freshness are what actually decide the shape.

1. **Primary ingest: FPbase GraphQL.** It is the only source with an open data
   policy, a working keyless API, per-record provenance, and cube-level modeling
   (84 Zeiss configs, plus Semrock/Chroma/Omega/Leica).
2. **Fetch offline, commit the result — do not fetch at runtime.** A dev-time
   fetcher generates a dataset; the GUI reads it. A GUI that calls FPbase on
   startup breaks on an air-gapped microscope PC, which is the normal deployment.
3. **Bundle the nominal layer** — 941 KiB raw, ~100 KiB compressed, trivially
   small. Cube name, catalog number, role assignments, manufacturer/part, and
   bands. But note §3.4: bands must be **parsed from designations** for ~⅔ of
   filters, because `bandcenter`/`bandwidth` are only 34% populated.
4. **Treat measured curves as an opt-in cache**, keyed by source + part. Not
   because of size (4.3 MB packed) but because FPbase cannot clear third-party
   rights on vendor-deposited curves, and vendor sites grant nothing.
5. **Write designation parsers per vendor**, starting with the Zeiss grammar in
   §2.4 — it covers 235 filters and 84 cubes from strings alone, and is the only
   route to bands for the two-thirds of records lacking structured values.
6. **Validate on ingest**: reject/flag records where `peakWave` falls outside the
   nominal band (the `BP 450-490` → `peakWave 1143` case is real and in the data).
7. **Fill the Nikon/Olympus gap separately** — FPbase has 3 and 12 entries; these
   need vendor or Edmund Optics sources.

## 7. Implementation

Implemented as `crates/numanager-spectra`.

Three tables, all produced by one binary:

```sh
# curves — 4130 filter spectra, ~6 min
cargo run -p numanager-spectra --features fetch --bin fetch-spectra -- \
    --kind spectra --out data/spectra-filters.parquet

# cube composition — one request
cargo run -p numanager-spectra --features fetch --bin fetch-spectra -- \
    --kind cubes --out data/filter-cubes.parquet

# nominal bands — offline, parsed from the cube table
cargo run -p numanager-spectra --features fetch --bin fetch-spectra -- \
    --kind bands --in data/filter-cubes.parquet --out data/filter-bands.parquet
```

| table | rows | size | redistributable |
|---|---|---|---|
| `spectra-filters.parquet` | 4130 curves | 1.5 MB | no |
| `filter-cubes.parquet` | 29063 placements / 10398 cubes | 0.47 MB | yes |
| `filter-bands.parquet` | 2996 bands / 2218 filters | 0.06 MB | yes |

`cubes.spectrum_id` joins to `spectra.source_id`; `bands.filter_id` joins to
`cubes.filter_id`.

Other options: `--category` (`F`/`D`/`P`/`L`/`C`), `--limit`, `--abs-tol`,
`--rel-tol`.

**Feature layout** keeps the cost opt-in, matching the workspace's
minimal-dependency style:

| feature | pulls in | for |
|---|---|---|
| *(default)* | nothing | curve types, adaptive sampling, quantization, interpolation |
| `store` | polars | Parquet read/write |
| `fetch` | + ureq, serde_json | downloading (implies `store`) |

A GUI consuming the data needs `store` only; nothing depends on the fetcher at
runtime. The fetcher is a build-time tool by design — microscope machines are
routinely offline, so the GUI reads a generated file rather than the network.

**What the crate encodes from this research:**

- `simplify()` — Douglas–Peucker with the hybrid abs/rel criterion from §3.5,
  defaults `DEFAULT_ABS_TOL = 0.01` and `DEFAULT_REL_TOL = 0.10`, relative error
  floored at `BLOCKING_FLOOR = 1e-6`.
- `quantize()`/`dequantize()` — log-scale `u16` over 12 decades, so OD6 blocking
  survives (a linear `u16` resolves 1.5e-5 and would flatten it).
- `clamp_transmission()` — applied before any logarithm, because upstream data
  contains negatives and values above 1.0 (§3.6).
- `Curve::value_at()` — linear interpolation in the *linear* domain, matching the
  error model `simplify()` guarantees; returns 0.0 outside the sampled range
  rather than extrapolating.
- `fpbase::curve_query()` — batches via GraphQL aliases and **never requests
  `tavg`**; a failed batch retries id-by-id so one bad value costs one record.
- `record_from_node()` — drops null samples, skips curves with fewer than two
  surviving points.
- `Provenance` — written per record, with `redistributable` defaulting to
  **false** for FPbase-sourced curves, since FPbase cannot clear third-party
  rights (§2.1). Opting a record in is a deliberate act.

**Designation parsing** (`designation.rs`) turns part names into nominal bands,
which is the only route to bands for the ~⅔ of filters where `bandcenter` is
null and the only *correct* route for the Semrock/Chroma records where it is
wrong. Rather than six vendor-specific parsers it uses one rule set:

- a number in 180-2500 nm is a centre or an edge; anything outside is ignored,
  which is what stops Chroma catalogue numbers (`51005m`) becoming 51005 nm bands;
- a number *smaller than the centre it follows*, separated by `/` or `-`, is that
  centre's width — so `855/210` is one 210 nm-wide band while `484/561` is two
  bands;
- `-` between two centres is a range only while the first has no width yet,
  which distinguishes `BP 390-420` from Leica's `Ex 391/32-479/33`;
- `/` between two centres lists separate bands, never a range;
- role comes from keyword scan (dichroic markers first, since `lpxr` and `dclp`
  both contain `lp`), and decides what a lone number means.

Measured over all 2454 distinct filters in the cube table:

| | |
|---|---|
| parsed | **2218 (90.4%)** |
| bands produced | 2996 (1505 bandpass, 735 dichroic edges, 582 lines, 154 longpass, 20 shortpass) |
| checked against a plausible upstream `bandcenter` | 618 |
| disagreements | **2**, both cases where upstream is wrong |

Per vendor, Zeiss parses 236/237. The 236 unparsed records are overwhelmingly
Chroma set numbers (`51005m`, `59002m`) and Olympus cube codes (`U-MGFPHQ`) that
contain no wavelength at all — correctly rejected rather than guessed at.

**Verified end to end** against the live API. Output schema:

```
source_id, name, manufacturer, part, category, subtype,
wavelengths_nm: List(UInt16), values: List(UInt16),
source, source_url, retrieved, license, measurement_kind, redistributable
```

Per-column compression confirms the §3.6 prediction — the wavelength column
delta-compresses 4.3× (26,067 → 6,103 bytes) against 2.5× for values, which is
exactly why the columnar layout beats an interleaved blob.

## 8. Sources

- FPbase GraphQL API — <https://www.fpbase.org/graphql/>
- FPbase terms/license — <https://www.fpbase.org/terms/>
- FPbase citation — <https://www.fpbase.org/cite/>; Lambert TJ (2019),
  *Nature Methods* 16:277–278, doi:10.1038/s41592-019-0352-8
- FPbase source — <https://github.com/tlambert03/FPbase> (LICENSE: GPLv3)
- FPbase Zeiss filter sets —
  <https://www.fpbase.org/microscope/VgeWjEPrGiSL6saRi9myA8/>
- FPbase Semrock filter sets —
  <https://www.fpbase.org/microscope/HGtCWRnyn8joPY5WF2t3zW/>
- OME Filter and FilterSet —
  <https://docs.openmicroscopy.org/ome-model/6.2.2/developers/filter-and-filterset.html>
- Chroma part spectra —
  <https://www.chroma.com/files/part_spectra/5270-ascii.txt>,
  <https://www.chroma.com/products/catalog-downloads/>
- Semrock SearchLight — <https://searchlight.idex-hs.com/>,
  <https://www.idex-hs.com/semrock/searchlight>
- Semrock optical filter FAQ (ASCII data availability) —
  <https://www.idex-hs.com/contact/contact-us/faqs/optical-filters-faqs>
- Zeiss Filter Assistant —
  <https://www.micro-shop.zeiss.com/en/us/shop/filterAssistant/filtersets/>
- Thorlabs microscope filter cubes —
  <https://www.thorlabs.com/microscope-filter-cubes>
