//! Spectral data for filters, dichroics, light sources, and detectors.
//!
//! Curves are stored adaptively sampled and log-quantized, which preserves
//! out-of-band blocking (OD5-OD6) that a linear-domain representation discards.
//! See `docs/reference/filter_spectra_databases.md` for the measurements behind the
//! defaults chosen here.

use std::fmt;

pub mod designation;
#[cfg(feature = "fetch")]
pub mod fpbase;
#[cfg(feature = "store")]
pub mod store;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
}

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    Transport,
    Protocol,
    InvalidData,
    Storage,
}

/// Values at or below this are treated as zero. Quantization maps this to 0.
pub const TRANSMISSION_FLOOR: f64 = 1e-12;

/// Number of decades spanned by the quantized representation.
pub const QUANT_DECADES: f64 = 12.0;

/// Below OD6 the upstream data is measurement noise, so relative error is not
/// meaningful there. Used as the denominator floor in [`simplify`].
pub const BLOCKING_FLOOR: f64 = 1e-6;

/// Default absolute transmission tolerance for adaptive sampling.
pub const DEFAULT_ABS_TOL: f64 = 0.01;

/// Default relative tolerance for adaptive sampling.
pub const DEFAULT_REL_TOL: f64 = 0.10;

/// Clamp a raw transmission sample into a physically meaningful range.
///
/// Upstream data is measured, not idealized: it contains negative values from
/// baseline noise and values slightly above unity. Both must be removed before
/// any logarithm is taken.
pub fn clamp_transmission(value: f64) -> f64 {
    if !value.is_finite() {
        return TRANSMISSION_FLOOR;
    }
    value.clamp(TRANSMISSION_FLOOR, 1.0)
}

/// Quantize a transmission value to `u16` on a log scale.
///
/// A linear `u16` resolves 1.5e-5, which would flatten OD5-OD6 blocking to
/// zero. Log quantization over 12 decades resolves 1.8e-4 decades instead.
pub fn quantize(value: f64) -> u16 {
    let clamped = clamp_transmission(value);
    let decades = (clamped.log10() + QUANT_DECADES) / QUANT_DECADES;
    let scaled = (decades * u16::MAX as f64).round();
    scaled.clamp(0.0, u16::MAX as f64) as u16
}

/// Inverse of [`quantize`].
pub fn dequantize(value: u16) -> f64 {
    let decades = value as f64 / u16::MAX as f64 * QUANT_DECADES - QUANT_DECADES;
    10f64.powf(decades)
}

/// Adaptively sample a curve, returning the indices to keep.
///
/// Douglas-Peucker with a hybrid error criterion: a point is kept while
/// *either* the absolute or the relative error of the linear interpolation
/// through it exceeds tolerance. Absolute error alone silently destroys the
/// blocking region; relative error alone distorts steep passband edges.
///
/// Returns at least the first and last index. `wavelengths` must be sorted.
pub fn simplify(wavelengths: &[u16], values: &[f64], abs_tol: f64, rel_tol: f64) -> Vec<usize> {
    if wavelengths.len() != values.len() {
        return Vec::new();
    }
    if wavelengths.len() < 3 {
        return (0..wavelengths.len()).collect();
    }

    let mut keep = vec![false; wavelengths.len()];
    keep[0] = true;
    keep[wavelengths.len() - 1] = true;

    let mut stack = vec![(0usize, wavelengths.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let x0 = wavelengths[a] as f64;
        let dx = wavelengths[b] as f64 - x0;
        let (y0, y1) = (values[a], values[b]);

        let mut worst = -1.0;
        let mut worst_index = 0usize;
        for i in (a + 1)..b {
            let interpolated = if dx == 0.0 {
                y0
            } else {
                y0 + (y1 - y0) * ((wavelengths[i] as f64 - x0) / dx)
            };
            let error = (values[i] - interpolated).abs();
            // Normalize both criteria to 1.0 so the larger one drives the split.
            let score = (error / abs_tol).max(error / values[i].max(BLOCKING_FLOOR) / rel_tol);
            if score > worst {
                worst = score;
                worst_index = i;
            }
        }

        if worst > 1.0 {
            keep[worst_index] = true;
            stack.push((a, worst_index));
            stack.push((worst_index, b));
        }
    }

    keep.iter()
        .enumerate()
        .filter_map(|(i, &k)| if k { Some(i) } else { None })
        .collect()
}

/// An adaptively sampled, log-quantized spectral curve.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Curve {
    pub wavelengths_nm: Vec<u16>,
    pub values: Vec<u16>,
}

impl Curve {
    /// Build from raw samples: clamp, simplify, then quantize.
    ///
    /// Samples that are non-finite in wavelength are dropped. Returns an empty
    /// curve if fewer than two samples survive.
    pub fn from_samples(samples: &[(f64, f64)], abs_tol: f64, rel_tol: f64) -> Self {
        let mut wavelengths = Vec::with_capacity(samples.len());
        let mut values = Vec::with_capacity(samples.len());
        for &(nm, value) in samples {
            if !nm.is_finite() || nm < 0.0 || nm > u16::MAX as f64 {
                continue;
            }
            wavelengths.push(nm.round() as u16);
            values.push(clamp_transmission(value));
        }
        if wavelengths.len() < 2 {
            return Self::default();
        }

        let keep = simplify(&wavelengths, &values, abs_tol, rel_tol);
        Self {
            wavelengths_nm: keep.iter().map(|&i| wavelengths[i]).collect(),
            values: keep.iter().map(|&i| quantize(values[i])).collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.wavelengths_nm.len()
    }

    pub fn is_empty(&self) -> bool {
        self.wavelengths_nm.is_empty()
    }

    pub fn range_nm(&self) -> Option<(u16, u16)> {
        match (self.wavelengths_nm.first(), self.wavelengths_nm.last()) {
            (Some(&lo), Some(&hi)) => Some((lo, hi)),
            _ => None,
        }
    }

    /// Transmission at a wavelength, linearly interpolated.
    ///
    /// Interpolation is linear in the linear domain, matching the error model
    /// [`simplify`] guarantees. Outside the sampled range this returns 0.0
    /// rather than extrapolating.
    pub fn value_at(&self, nm: f64) -> f64 {
        if self.wavelengths_nm.len() < 2 {
            return self.values.first().map(|&v| dequantize(v)).unwrap_or(0.0);
        }
        let first = self.wavelengths_nm[0] as f64;
        let last = self.wavelengths_nm[self.wavelengths_nm.len() - 1] as f64;
        if nm < first || nm > last {
            return 0.0;
        }

        let index = match self
            .wavelengths_nm
            .binary_search_by(|probe| (*probe as f64).partial_cmp(&nm).unwrap())
        {
            Ok(i) => return dequantize(self.values[i]),
            Err(i) => i,
        };
        let (a, b) = (index - 1, index);
        let (x0, x1) = (self.wavelengths_nm[a] as f64, self.wavelengths_nm[b] as f64);
        let (y0, y1) = (dequantize(self.values[a]), dequantize(self.values[b]));
        if x1 == x0 {
            return y0;
        }
        y0 + (y1 - y0) * ((nm - x0) / (x1 - x0))
    }

    /// Peak transmission and the wavelength it occurs at.
    pub fn peak(&self) -> Option<(u16, f64)> {
        self.wavelengths_nm
            .iter()
            .zip(&self.values)
            .max_by_key(|(_, &v)| v)
            .map(|(&nm, &v)| (nm, dequantize(v)))
    }
}

/// How a record's numbers were arrived at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasurementKind {
    Measured,
    Theoretical,
    NominalFromDesignation,
    Unknown,
}

impl MeasurementKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Measured => "measured",
            Self::Theoretical => "theoretical",
            Self::NominalFromDesignation => "nominal_from_designation",
            Self::Unknown => "unknown",
        }
    }
}

/// Where a record came from and what may be done with it.
///
/// Recorded verbatim per source rather than inferred. Upstream licensing is not
/// uniform: FPbase dedicates its own data but cannot clear third-party rights on
/// vendor-deposited curves, so `redistributable` is a deliberate per-record
/// decision and not a property of the source alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    pub source: String,
    pub source_url: String,
    pub source_id: String,
    pub retrieved: String,
    pub license: String,
    pub measurement_kind: MeasurementKind,
    pub redistributable: bool,
}

/// A spectrum plus the metadata needed to attribute and re-fetch it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpectrumRecord {
    pub name: String,
    pub manufacturer: Option<String>,
    pub part: Option<String>,
    /// Upstream category: `F` filter, `D` dye, `P` protein, `L` light, `C` camera.
    pub category: String,
    /// Upstream subtype: `BP`, `BS`, `LP`, `SP`, `BM`, `BX`, `EX`, `EM`, ...
    pub subtype: String,
    pub curve: Curve,
    pub provenance: Provenance,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantization_round_trips_across_decades() {
        for exponent in 0..12 {
            let value = 10f64.powi(-exponent);
            let round_tripped = dequantize(quantize(value));
            let error = (round_tripped.log10() - value.log10()).abs();
            assert!(error < 1e-3, "{value} -> {round_tripped}");
        }
    }

    #[test]
    fn quantization_preserves_blocking() {
        // The point of log quantization: OD6 must not collapse to zero.
        assert!(quantize(1e-6) > 0);
        assert!(dequantize(quantize(1e-6)) < 1e-5);
        assert_ne!(quantize(1e-6), quantize(1e-3));
    }

    #[test]
    fn clamping_removes_measurement_artifacts() {
        assert_eq!(clamp_transmission(-0.0046), TRANSMISSION_FLOOR);
        assert_eq!(clamp_transmission(1.0043), 1.0);
        assert_eq!(clamp_transmission(f64::NAN), TRANSMISSION_FLOOR);
    }

    #[test]
    fn simplify_keeps_endpoints_and_drops_collinear_points() {
        let wavelengths: Vec<u16> = (400..=500).collect();
        let values: Vec<f64> = wavelengths.iter().map(|_| 0.5).collect();
        let keep = simplify(&wavelengths, &values, DEFAULT_ABS_TOL, DEFAULT_REL_TOL);
        assert_eq!(keep, vec![0, wavelengths.len() - 1]);
    }

    #[test]
    fn simplify_respects_both_tolerances() {
        // A steep edge plus a low-level blocking shoulder. Absolute error alone
        // would discard the shoulder entirely.
        let wavelengths: Vec<u16> = (400..=460).collect();
        let values: Vec<f64> = wavelengths
            .iter()
            .map(|&nm| if nm < 430 { 1e-6 } else { 0.9 })
            .collect();
        let keep = simplify(&wavelengths, &values, DEFAULT_ABS_TOL, DEFAULT_REL_TOL);

        let curve = Curve {
            wavelengths_nm: keep.iter().map(|&i| wavelengths[i]).collect(),
            values: keep.iter().map(|&i| quantize(values[i])).collect(),
        };
        assert!(
            curve.value_at(410.0) < 1e-4,
            "blocking region was flattened"
        );
        assert!(curve.value_at(450.0) > 0.8, "passband was lost");
    }

    #[test]
    fn curve_interpolates_and_bounds() {
        let samples = [(400.0, 0.0), (450.0, 1.0), (500.0, 0.0)];
        let curve = Curve::from_samples(&samples, DEFAULT_ABS_TOL, DEFAULT_REL_TOL);
        assert_eq!(curve.range_nm(), Some((400, 500)));
        assert!(curve.value_at(450.0) > 0.99);
        assert_eq!(curve.value_at(399.0), 0.0);
        assert_eq!(curve.value_at(501.0), 0.0);
        let (peak_nm, peak) = curve.peak().unwrap();
        assert_eq!(peak_nm, 450);
        assert!(peak > 0.99);
    }

    #[test]
    fn adaptive_sampling_reduces_point_count() {
        // Flat baseline, steep edges, flat top: the shape adaptive sampling wins on.
        let samples: Vec<(f64, f64)> = (300..=800)
            .map(|nm| {
                let value = if (480..=520).contains(&nm) {
                    0.95
                } else {
                    1e-6
                };
                (nm as f64, value)
            })
            .collect();
        let curve = Curve::from_samples(&samples, DEFAULT_ABS_TOL, DEFAULT_REL_TOL);
        assert!(curve.len() < samples.len() / 10, "kept {}", curve.len());
        assert!(curve.value_at(500.0) > 0.9);
    }
}
