//! Parquet storage.
//!
//! Wide layout: one row per spectrum, curves held as `List<u16>` columns.
//! Measured against the alternatives in `docs/reference/filter_spectra_databases.md` --
//! wide beats a long (one-row-per-sample) layout by ~40%, and separating
//! wavelength and value into distinct columns is what lets the wavelength
//! column delta-compress, which is why this beats a hand-packed blob.
//!
//! The file is not compressed externally. Parquet already compresses per column
//! chunk; wrapping it in gzip saves ~1% and costs mmap and random access.

use std::fs::File;
use std::path::Path;

use polars::prelude::*;
// polars re-exports `ParquetCompression` but not the level type inside it.
use polars_parquet::write::ZstdLevel;

use crate::{Curve, Error, ErrorCode, MeasurementKind, Provenance, Result, SpectrumRecord};

/// Highest zstd level. This is a build-time cost paid once; reads are unaffected
/// by the level, and it is ~14% smaller than the default.
pub const ZSTD_LEVEL: i32 = 22;

fn storage_error(error: impl std::fmt::Display) -> Error {
    Error::new(ErrorCode::Storage, error.to_string())
}

fn list_column(name: &str, curves: impl Iterator<Item = Vec<u16>>, rows: usize) -> Column {
    let mut builder = ListPrimitiveChunkedBuilder::<UInt16Type>::new(
        name.into(),
        rows,
        rows * 200,
        DataType::UInt16,
    );
    for values in curves {
        builder.append_slice(&values);
    }
    builder.finish().into_series().into_column()
}

pub fn to_frame(records: &[SpectrumRecord]) -> Result<DataFrame> {
    let rows = records.len();
    let text = |values: Vec<String>, name: &str| -> Column {
        Series::new(name.into(), values).into_column()
    };
    let optional = |values: Vec<Option<String>>, name: &str| -> Column {
        Series::new(name.into(), values).into_column()
    };

    let frame = DataFrame::new_infer_height(vec![
        text(
            records
                .iter()
                .map(|r| r.provenance.source_id.clone())
                .collect(),
            "source_id",
        ),
        text(records.iter().map(|r| r.name.clone()).collect(), "name"),
        optional(
            records.iter().map(|r| r.manufacturer.clone()).collect(),
            "manufacturer",
        ),
        optional(records.iter().map(|r| r.part.clone()).collect(), "part"),
        text(
            records.iter().map(|r| r.category.clone()).collect(),
            "category",
        ),
        text(
            records.iter().map(|r| r.subtype.clone()).collect(),
            "subtype",
        ),
        list_column(
            "wavelengths_nm",
            records.iter().map(|r| r.curve.wavelengths_nm.clone()),
            rows,
        ),
        list_column(
            "values",
            records.iter().map(|r| r.curve.values.clone()),
            rows,
        ),
        text(
            records
                .iter()
                .map(|r| r.provenance.source.clone())
                .collect(),
            "source",
        ),
        text(
            records
                .iter()
                .map(|r| r.provenance.source_url.clone())
                .collect(),
            "source_url",
        ),
        text(
            records
                .iter()
                .map(|r| r.provenance.retrieved.clone())
                .collect(),
            "retrieved",
        ),
        text(
            records
                .iter()
                .map(|r| r.provenance.license.clone())
                .collect(),
            "license",
        ),
        text(
            records
                .iter()
                .map(|r| r.provenance.measurement_kind.as_str().to_string())
                .collect(),
            "measurement_kind",
        ),
        Series::new(
            "redistributable".into(),
            records
                .iter()
                .map(|r| r.provenance.redistributable)
                .collect::<Vec<bool>>(),
        )
        .into_column(),
    ])
    .map_err(storage_error)?;

    Ok(frame)
}

pub fn write(records: &[SpectrumRecord], path: impl AsRef<Path>) -> Result<()> {
    let mut frame = to_frame(records)?;
    let file = File::create(path.as_ref()).map_err(storage_error)?;
    let level = ZstdLevel::try_new(ZSTD_LEVEL).map_err(storage_error)?;
    ParquetWriter::new(file)
        .with_compression(ParquetCompression::Zstd(Some(level)))
        .finish(&mut frame)
        .map_err(storage_error)?;
    Ok(())
}

/// Cube composition: one row per filter placement.
///
/// Stored long rather than wide -- it is only ~29k rows, and a tidy table joins
/// to the spectra table on `spectrum_id` without unnesting.
#[cfg(feature = "fetch")]
pub fn cubes_to_frame(placements: &[crate::fpbase::CubePlacement]) -> Result<DataFrame> {
    let text = |values: Vec<String>, name: &str| -> Column {
        Series::new(name.into(), values).into_column()
    };
    let optional = |values: Vec<Option<String>>, name: &str| -> Column {
        Series::new(name.into(), values).into_column()
    };
    let numbers = |values: Vec<Option<f64>>, name: &str| -> Column {
        Series::new(name.into(), values).into_column()
    };

    DataFrame::new_infer_height(vec![
        text(
            placements.iter().map(|p| p.config_id.clone()).collect(),
            "config_id",
        ),
        text(
            placements.iter().map(|p| p.cube_name.clone()).collect(),
            "cube_name",
        ),
        text(
            placements.iter().map(|p| p.collection.clone()).collect(),
            "collection",
        ),
        text(
            placements.iter().map(|p| p.collection_id.clone()).collect(),
            "collection_id",
        ),
        text(placements.iter().map(|p| p.role.clone()).collect(), "role"),
        Series::new(
            "reflects".into(),
            placements.iter().map(|p| p.reflects).collect::<Vec<bool>>(),
        )
        .into_column(),
        text(
            placements.iter().map(|p| p.filter_id.clone()).collect(),
            "filter_id",
        ),
        text(
            placements.iter().map(|p| p.filter_name.clone()).collect(),
            "filter_name",
        ),
        optional(
            placements.iter().map(|p| p.manufacturer.clone()).collect(),
            "manufacturer",
        ),
        optional(placements.iter().map(|p| p.part.clone()).collect(), "part"),
        numbers(
            placements.iter().map(|p| p.bandcenter_nm).collect(),
            "bandcenter_nm",
        ),
        numbers(
            placements.iter().map(|p| p.bandwidth_nm).collect(),
            "bandwidth_nm",
        ),
        optional(
            placements.iter().map(|p| p.spectrum_id.clone()).collect(),
            "spectrum_id",
        ),
        text(
            placements
                .iter()
                .map(|p| p.provenance.source.clone())
                .collect(),
            "source",
        ),
        text(
            placements
                .iter()
                .map(|p| p.provenance.source_url.clone())
                .collect(),
            "source_url",
        ),
        text(
            placements
                .iter()
                .map(|p| p.provenance.retrieved.clone())
                .collect(),
            "retrieved",
        ),
        text(
            placements
                .iter()
                .map(|p| p.provenance.license.clone())
                .collect(),
            "license",
        ),
        Series::new(
            "redistributable".into(),
            placements
                .iter()
                .map(|p| p.provenance.redistributable)
                .collect::<Vec<bool>>(),
        )
        .into_column(),
    ])
    .map_err(storage_error)
}

#[cfg(feature = "fetch")]
pub fn write_cubes(
    placements: &[crate::fpbase::CubePlacement],
    path: impl AsRef<Path>,
) -> Result<()> {
    let mut frame = cubes_to_frame(placements)?;
    write_frame(&mut frame, path)
}

/// How well the designation parser did, and whether it agrees with upstream.
#[derive(Debug, Clone, Copy, Default)]
pub struct BandStats {
    pub filters: usize,
    pub parsed: usize,
    pub bands: usize,
    /// Parsed, but the band count disagreed with what the prefix promised.
    pub inconsistent: usize,
    /// Filters where upstream also supplied a `bandcenter` to check against.
    pub compared: usize,
    /// Of those, how many disagreed by more than 1 nm.
    pub center_mismatches: usize,
    /// Upstream `bandcenter` values that are not plausible wavelengths at all.
    ///
    /// FPbase derives this field with its own parser, which mis-reads vendor
    /// series codes: `Semrock FF01-900/32` is recorded with a bandcenter of 1,
    /// taken from the `01` in `FF01`. Most apparent disagreements are upstream
    /// errors, not parser errors, which is why this is counted separately.
    pub upstream_implausible: usize,
}

/// Derive nominal bands from filter designations in a cube table.
///
/// Emits one row per band. Cross-checks against upstream `bandcenter_nm`
/// wherever it is populated, which is the parser's regression signal against
/// real data rather than hand-written examples.
pub fn derive_bands(cubes: &DataFrame) -> Result<(DataFrame, BandStats)> {
    let text = |name: &str| -> Result<Vec<Option<String>>> {
        let column = cubes.column(name).map_err(storage_error)?;
        Ok(column
            .str()
            .map_err(storage_error)?
            .iter()
            .map(|v| v.map(str::to_string))
            .collect())
    };
    let number = |name: &str| -> Result<Vec<Option<f64>>> {
        let column = cubes.column(name).map_err(storage_error)?;
        Ok(column.f64().map_err(storage_error)?.iter().collect())
    };

    let filter_id = text("filter_id")?;
    let filter_name = text("filter_name")?;
    let manufacturer = text("manufacturer")?;
    let part = text("part")?;
    let upstream_center = number("bandcenter_nm")?;

    let mut seen = std::collections::HashSet::new();
    let mut stats = BandStats::default();
    let (mut ids, mut names, mut makers, mut parts) = (vec![], vec![], vec![], vec![]);
    let (mut indices, mut kinds, mut centers) = (vec![], vec![], vec![]);
    let (mut widths, mut lows, mut highs) = (vec![], vec![], vec![]);
    let (mut he, mut consistent) = (vec![], vec![]);

    for row in 0..cubes.height() {
        let id = filter_id[row].clone().unwrap_or_default();
        if id.is_empty() || !seen.insert(id.clone()) {
            continue;
        }
        stats.filters += 1;

        let name = filter_name[row].clone().unwrap_or_default();
        let Some(parsed) = crate::designation::parse(&name) else {
            continue;
        };
        stats.parsed += 1;
        if !parsed.is_consistent() {
            stats.inconsistent += 1;
        }

        // Where upstream states a plausible band centre, the parser should
        // agree. Implausible upstream values are counted, not compared.
        if let Some(expected) = upstream_center[row] {
            if !(crate::designation::MIN_CENTER_NM..=crate::designation::MAX_CENTER_NM)
                .contains(&expected)
            {
                stats.upstream_implausible += 1;
            } else {
                stats.compared += 1;
                let matched = parsed
                    .bands
                    .iter()
                    .any(|band| (band.center_nm() - expected).abs() <= 1.0);
                if !matched {
                    stats.center_mismatches += 1;
                }
            }
        }

        for (index, band) in parsed.bands.iter().enumerate() {
            stats.bands += 1;
            let (low, high) = match band.range_nm() {
                Some((low, high)) => (Some(low), Some(high)),
                None => (None, None),
            };
            let width = match band {
                crate::designation::Band::Bandpass { width_nm, .. } => Some(*width_nm),
                _ => None,
            };
            ids.push(id.clone());
            names.push(name.clone());
            makers.push(manufacturer[row].clone());
            parts.push(part[row].clone());
            indices.push(index as u32);
            kinds.push(band.kind().to_string());
            centers.push(band.center_nm());
            widths.push(width);
            lows.push(low);
            highs.push(high);
            he.push(parsed.high_efficiency);
            consistent.push(parsed.is_consistent());
        }
    }

    let frame = DataFrame::new_infer_height(vec![
        Series::new("filter_id".into(), ids).into_column(),
        Series::new("filter_name".into(), names).into_column(),
        Series::new("manufacturer".into(), makers).into_column(),
        Series::new("part".into(), parts).into_column(),
        Series::new("band_index".into(), indices).into_column(),
        Series::new("kind".into(), kinds).into_column(),
        Series::new("center_nm".into(), centers).into_column(),
        Series::new("width_nm".into(), widths).into_column(),
        Series::new("low_nm".into(), lows).into_column(),
        Series::new("high_nm".into(), highs).into_column(),
        Series::new("high_efficiency".into(), he).into_column(),
        Series::new("consistent".into(), consistent).into_column(),
    ])
    .map_err(storage_error)?;

    Ok((frame, stats))
}

pub fn write_frame(frame: &mut DataFrame, path: impl AsRef<Path>) -> Result<()> {
    let file = File::create(path.as_ref()).map_err(storage_error)?;
    let level = ZstdLevel::try_new(ZSTD_LEVEL).map_err(storage_error)?;
    ParquetWriter::new(file)
        .with_compression(ParquetCompression::Zstd(Some(level)))
        .finish(frame)
        .map_err(storage_error)?;
    Ok(())
}

/// Load any table written by this module.
pub fn read_frame(path: impl AsRef<Path>) -> Result<DataFrame> {
    let file = File::open(path.as_ref()).map_err(storage_error)?;
    ParquetReader::new(file).finish().map_err(storage_error)
}

pub fn read(path: impl AsRef<Path>) -> Result<Vec<SpectrumRecord>> {
    let file = File::open(path.as_ref()).map_err(storage_error)?;
    let frame = ParquetReader::new(file).finish().map_err(storage_error)?;
    from_frame(&frame)
}

pub fn from_frame(frame: &DataFrame) -> Result<Vec<SpectrumRecord>> {
    let text = |name: &str| -> Result<Vec<Option<String>>> {
        let column = frame.column(name).map_err(storage_error)?;
        let values = column.str().map_err(storage_error)?;
        Ok(values.iter().map(|v| v.map(str::to_string)).collect())
    };
    let list = |name: &str| -> Result<Vec<Vec<u16>>> {
        let column = frame.column(name).map_err(storage_error)?;
        let values = column.list().map_err(storage_error)?;
        let mut out = Vec::with_capacity(values.len());
        // `iter()` on a ListChunked yields raw arrow arrays; the amortized
        // iterator hands back a reusable Series container instead.
        for entry in values.amortized_iter() {
            match entry {
                Some(series) => {
                    let inner = series.as_ref().u16().map_err(storage_error)?;
                    out.push(inner.iter().map(|v| v.unwrap_or(0)).collect());
                }
                None => out.push(Vec::new()),
            }
        }
        Ok(out)
    };

    let source_id = text("source_id")?;
    let name = text("name")?;
    let manufacturer = text("manufacturer")?;
    let part = text("part")?;
    let category = text("category")?;
    let subtype = text("subtype")?;
    let source = text("source")?;
    let source_url = text("source_url")?;
    let retrieved = text("retrieved")?;
    let license = text("license")?;
    let measurement_kind = text("measurement_kind")?;
    let wavelengths = list("wavelengths_nm")?;
    let values = list("values")?;
    let redistributable = frame
        .column("redistributable")
        .map_err(storage_error)?
        .bool()
        .map_err(storage_error)?
        .iter()
        .map(|v| v.unwrap_or(false))
        .collect::<Vec<bool>>();

    let take = |values: &[Option<String>], index: usize| -> String {
        values.get(index).cloned().flatten().unwrap_or_default()
    };

    Ok((0..frame.height())
        .map(|index| SpectrumRecord {
            name: take(&name, index),
            manufacturer: manufacturer.get(index).cloned().flatten(),
            part: part.get(index).cloned().flatten(),
            category: take(&category, index),
            subtype: take(&subtype, index),
            curve: Curve {
                wavelengths_nm: wavelengths.get(index).cloned().unwrap_or_default(),
                values: values.get(index).cloned().unwrap_or_default(),
            },
            provenance: Provenance {
                source: take(&source, index),
                source_url: take(&source_url, index),
                source_id: take(&source_id, index),
                retrieved: take(&retrieved, index),
                license: take(&license, index),
                measurement_kind: match take(&measurement_kind, index).as_str() {
                    "measured" => MeasurementKind::Measured,
                    "theoretical" => MeasurementKind::Theoretical,
                    "nominal_from_designation" => MeasurementKind::NominalFromDesignation,
                    _ => MeasurementKind::Unknown,
                },
                redistributable: redistributable.get(index).copied().unwrap_or(false),
            },
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DEFAULT_ABS_TOL, DEFAULT_REL_TOL};

    fn sample_record(id: &str) -> SpectrumRecord {
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
        SpectrumRecord {
            name: format!("Test BP 500/40 #{id}"),
            manufacturer: Some("Zeiss".into()),
            part: None,
            category: "F".into(),
            subtype: "BP".into(),
            curve: Curve::from_samples(&samples, DEFAULT_ABS_TOL, DEFAULT_REL_TOL),
            provenance: Provenance {
                source: "fpbase".into(),
                source_url: format!("https://www.fpbase.org/spectra/{id}/"),
                source_id: id.into(),
                retrieved: "2026-07-28".into(),
                license: "test".into(),
                measurement_kind: MeasurementKind::Unknown,
                redistributable: false,
            },
        }
    }

    #[test]
    fn round_trips_through_parquet() {
        let records = vec![sample_record("1"), sample_record("2")];
        let path = std::env::temp_dir().join("numanager-spectra-roundtrip.parquet");
        write(&records, &path).unwrap();
        let loaded = read(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(loaded.len(), records.len());
        assert_eq!(loaded[0].name, records[0].name);
        assert_eq!(loaded[0].manufacturer.as_deref(), Some("Zeiss"));
        assert_eq!(loaded[0].part, None);
        assert_eq!(loaded[0].curve, records[0].curve);
        assert_eq!(loaded[1].provenance.source_id, "2");
        assert!(!loaded[0].provenance.redistributable);
        assert!(loaded[0].curve.value_at(500.0) > 0.9);
    }

    #[test]
    fn frame_has_expected_schema() {
        let frame = to_frame(&[sample_record("1")]).unwrap();
        assert_eq!(frame.height(), 1);
        for column in [
            "source_id",
            "name",
            "manufacturer",
            "part",
            "category",
            "subtype",
            "wavelengths_nm",
            "values",
            "source",
            "source_url",
            "retrieved",
            "license",
            "measurement_kind",
            "redistributable",
        ] {
            assert!(frame.column(column).is_ok(), "missing column {column}");
        }
    }
}
