# Rendering Wavelengths as RGB

Research notes for a GUI helper that turns a `Wavelength` into a display color.
No implementation is proposed here; this records the candidate algorithms, their
provenance, and their licensing.

## 1. Two different jobs, often conflated

Before picking an algorithm, decide which question the GUI is asking. They have
different correct answers.

**(a) "What color is this light?"** — a monochromatic source at 532 nm, an
illumination port, a laser line, a filter passband. The answer is a colorimetric
one: map the spectral power distribution through the human visual response.
Physically motivated, and users expect 532 nm to look green.

**(b) "What color should this channel be drawn in?"** — a display LUT for an
acquired image. Here the physically correct answer is often the wrong UI answer:

- A DAPI channel at 461 nm emission renders as a very dark blue-violet, which is
  nearly invisible on screen. Microscopy convention draws it as bright blue.
- Red/green pairs are the widespread convention but are the worst choice for
  red-green colorblind viewers; the accepted fix is red → magenta, or
  perceptually uniform LUTs (viridis, magma, inferno) for single channels.
- The display color assigned to a channel via its LUT is conventionally
  independent of the actual emission wavelength.

So: use a spectral mapping for **(a)** — spectrum bars, filter/laser/LED
swatches, illumination port indicators — and a curated LUT palette for **(b)**.
A single "wavelength → color" helper that gets used for both will produce unusable
image displays. If one helper serves both, it needs a mode/brightness parameter,
with the image-LUT mode normalizing to full brightness/saturation.

## 2. Candidate algorithms for the spectral mapping

### 2.1 Bruton piecewise-linear approximation

The most-copied approach in scientific GUIs. Piecewise-linear RGB ramps over the
visible range with empirical breakpoints at 380, 440, 490, 510, 580, 645, 780 nm,
followed by an intensity falloff near the vision limits and a gamma of 0.8:

```
if   λ > 700:  s = 0.3 + 0.7 * (780 - λ) / (780 - 700)
elif λ < 420:  s = 0.3 + 0.7 * (λ - 380) / (420 - 380)
else:          s = 1
R,G,B = (s*R)^γ, (s*G)^γ, (s*B)^γ      with γ = 0.8
```

- **Pro:** trivial, no color-space machinery, no gamut handling, output is always
  in [0,1], and its output *looks* like the rainbow people expect.
- **Con:** not colorimetric. It is a hand-tuned ramp, not a perceptual model.
  Hue boundaries are visibly wrong in places against a real spectrum.
- **Provenance:** Dan Bruton (Stephen F. Austin State University), original
  FORTRAN, published at
  <https://www.physics.sfasu.edu/astro/color/spectra.html>.
- **Licensing:** ⚠️ **unresolved.** The page is a personal academic page; no
  explicit license grant was found. It is redistributed widely without
  attribution, but that is not evidence of a grant. The *algorithm* (breakpoints
  and linear interpolation) is not copyrightable; a verbatim transcription of the
  code is. If used, re-derive from the described breakpoints rather than copying
  a transcription, and cite Bruton.

### 2.2 CIE 1931 color matching functions → XYZ → sRGB

The colorimetrically defensible route. For a monochromatic wavelength λ the
tristimulus values are just the CMF values at λ:

```
X = x̄(λ),  Y = ȳ(λ),  Z = z̄(λ)
```

then XYZ → linear sRGB via the standard matrix, then the sRGB transfer function.

Using tabulated CIE CMFs at 1 nm requires shipping ~400 rows × 3. The analytic
fits below avoid that entirely.

#### Wyman, Sloan & Shirley (2013) analytic fits

**Multi-lobe piecewise Gaussian fit** (their Equation 4) — the accurate one.
Form, with a selector `S(x, y, z) = (x < 0) ? y : z`:

```
c̄(λ) = Σᵢ αᵢ · exp( -½ · [ (λ - βᵢ) · S(λ - βᵢ, γᵢ, δᵢ) ]² )
```

x̄ uses 3 lobes, ȳ and z̄ use 2 lobes each. Coefficients (their Table 1):

|   | x̄₀ | x̄₁ | x̄₂ | ȳ₀ | ȳ₁ | z̄₀ | z̄₁ |
|---|------|------|------|------|------|------|------|
| α | 0.362 | 1.056 | −0.065 | 0.821 | 0.286 | 1.217 | 0.681 |
| β | 442.0 | 599.8 | 501.1 | 568.8 | 530.9 | 437.0 | 459.0 |
| γ | 0.0624 | 0.0264 | 0.0490 | 0.0213 | 0.0613 | 0.0845 | 0.0385 |
| δ | 0.0374 | 0.0323 | 0.0382 | 0.0247 | 0.0322 | 0.0278 | 0.0725 |

(γ applies below the mean, δ above — that is what the selector picks.)

Accuracy: max squared error 2.0e-4 / 6.4e-5 / 4.9e-4 for x̄/ȳ/z̄ against 1 nm
sampled CIE 1931 curves — **below the measured between-subject variance** in the
data the CIE standard was built from, and comparable to interpolating 10 nm
samples. Roughly ten lines of arithmetic.

**Single-lobe fit** (their Equation 2) — simpler, still better than prior
published analytic fits:

```
x̄₃₁(λ) = 1.065·exp(-½((λ-595.8)/33.33)²) + 0.366·exp(-½((λ-446.8)/19.44)²)
ȳ₃₁(λ) = 1.014·exp(-½((ln λ - ln 556.3)/0.075)²)
z̄₃₁(λ) = 1.839·exp(-½((ln λ - ln 449.8)/0.051)²)
```

RMS error below 0.015 and max absolute error below 0.046 for x̄ and ȳ; z̄ is
harder to fit and carries roughly three times that error.

A 1964 10° observer fit also exists (their Equation 3) if the wider-field
observer is ever wanted; for GUI swatches the 1931 2° observer is the
conventional choice.

- **Provenance:** Chris Wyman, Peter-Pike Sloan, Peter Shirley (NVIDIA), *Simple
  Analytic Approximations to the CIE XYZ Color Matching Functions*, Journal of
  Computer Graphics Techniques 2(2):1–11, 2013.
  <https://jcgt.org/published/0002/02/01/>
- **Licensing:** the **paper** is CC BY-ND 3.0 (authors retain copyright;
  reuse of images/text permitted for scholarly summary with citation). The
  numeric coefficients are measurements/facts and are not themselves subject to
  copyright — reproducing the table with citation is fine. The authors' **C++
  supplemental code** lives in a separate repo
  (<https://github.com/JournalOfComputerGraphicsTechniques/TEST-0002-02-01-Wyman-Sloan-Shirley>);
  ⚠️ **its license was not verified** — check before copying any of it verbatim.
  Implementing the published formulas independently avoids the question.
- **Underlying CIE data:** the tabulated 1931 standard observer is a CIE
  publication; CIE asserts copyright on its publications, though the numeric
  tables are redistributed extremely widely (e.g. by RIT's Munsell Color Science
  Lab, which is where Wyman et al. sourced theirs, and by `colour-science` under
  BSD-3-Clause). Using the analytic fit sidesteps redistributing CIE tables at all
  — a real practical advantage beyond the memory saving.

### 2.3 Comparison

| | Bruton | Wyman single-lobe | Wyman multi-lobe |
|---|---|---|---|
| Colorimetric | no | yes | yes |
| Extra machinery | none | XYZ→sRGB + gamut handling | same |
| Data to ship | 7 breakpoints | 3 formulas | 14 coefficients |
| License risk | unresolved | none (formulas + cite) | none (formulas + cite) |
| Looks "right" | yes, by construction | yes, after gamut mapping | yes, after gamut mapping |

## 3. Problems the colorimetric route must solve

These are the reasons naive CIE implementations look worse than Bruton, and are
where the actual engineering effort goes.

**Out-of-gamut.** Every monochromatic wavelength lies on the spectral locus,
which is entirely **outside** the sRGB gamut. Direct XYZ→sRGB yields negative
components at essentially every λ. Options:

- clip negatives to 0 — cheap, hue-shifts the saturated colors;
- desaturate toward the white point until in gamut — preserves hue, what most
  spectrum renderers do;
- scale to preserve luminance ratios.

Desaturation toward D65 is the usual recommendation for spectrum bars.

**Normalization.** ȳ(λ) is the luminous efficiency function, so a physically
scaled 450 nm is dim and 555 nm is bright. Correct for a spectrum-under-a-lamp
rendering, wrong for a row of equally-weighted UI swatches. For swatches,
normalize each color to a constant luminance or to max component = 1.

**Range limits.** Both methods are defined roughly 380–780 nm. Instrument
wavelengths routinely fall outside:

- **UV** (365 nm mercury line, 385 nm LEDs — both appear in real Zeiss sets):
  conventionally rendered as deep violet/purple, fading to a neutral dark.
- **NIR** (730, 780, 850 nm illumination; Cy7 emission): conventionally deep red
  fading to dark, or a designated "invisible" swatch (grey/hatched) so the user
  is not misled into thinking it is visible light.

Decide these conventions explicitly rather than letting the formula run off its
domain — a clamp at 380/780 silently renders 850 nm identically to 780 nm.

## 4. Reference implementation to compare against: FPbase

FPbase computes and exposes a display color per spectrum. This is directly
queryable and makes a good cross-check for whatever we implement:

```
POST https://www.fpbase.org/graphql/
{ spectra { id category subtype owner{name} } }
{ microscope(id:"…"){ opticalConfigs{ name filters{ path spectrum{ peakWave color } } } } }
```

Verified sample values (live API, 2026-07-27):

| Filter | peak (nm) | FPbase color |
|---|---|---|
| Zeiss BP 365/12 | 362 | `#0c0026` |
| Zeiss BP 420/40 | 430 | `#2500e1` |
| Zeiss BP 423/44 | 408 | `#55009f` |
| Zeiss BS 465 | 471 | `#009eff` |
| Zeiss BP 511/44 | 498 | `#00ff99` |
| Zeiss BP 555/30 LED | 546 | `#ffe700` |
| Zeiss LP 615 | 649 | `#c00000` |

The strong falloff toward UV (`#0c0026` at 362 nm) and toward red (`#c00000` at
649 nm) matches a Bruton-style intensity ramp with gamma, not a pure colorimetric
mapping.

## 5. Recommendation

1. Use the **Wyman multi-lobe fit → XYZ → sRGB**, with desaturation-toward-D65
   gamut mapping and per-swatch luminance normalization, for the "color of this
   light" case. It is 14 constants, has no licensing encumbrance if implemented
   from the published formulas with citation, and avoids shipping CIE tables.
2. Define explicit UV (<380 nm) and NIR (>780 nm) conventions rather than
   clamping.
3. Keep the image-channel LUT palette **separate** and curated
   (colorblind-safe defaults: blue / green / magenta / grey rather than
   blue / green / red), seeded by wavelength but not dictated by it.
4. Cross-check output against the FPbase `color` values above.

## 6. Sources

- Wyman, Sloan & Shirley, *Simple Analytic Approximations to the CIE XYZ Color
  Matching Functions*, JCGT 2(2), 2013 — <https://jcgt.org/published/0002/02/01/>
  (paper CC BY-ND 3.0; supplemental code repo license unverified)
- Supplemental code repo —
  <https://github.com/JournalOfComputerGraphicsTechniques/TEST-0002-02-01-Wyman-Sloan-Shirley>
- Bruton, *Approximate RGB values for Visible Wavelengths* —
  <https://www.physics.sfasu.edu/astro/color/spectra.html> (license unresolved)
- FPbase GraphQL API — <https://www.fpbase.org/graphql/> (see
  [`filter_spectra_databases.md`](filter_spectra_databases.md) for FPbase data terms)
- Bankhead, *Analyzing fluorescence microscopy images with ImageJ*, "Channels &
  colors" — LUT conventions and colorblind-safe channel choices
- ImageJ visualization docs — <https://imagej.net/imaging/visualization>
