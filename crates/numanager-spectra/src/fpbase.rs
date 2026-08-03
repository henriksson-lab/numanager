//! FPbase client.
//!
//! FPbase is the primary upstream: it has an open data policy, a keyless API,
//! and per-record provenance. See `docs/reference/filter_spectra_databases.md`.
//!
//! Two upstream quirks are handled here because they are not obvious:
//!
//! - There is no bulk endpoint. `spectra` lists ids but carries no curve data,
//!   so curves must be fetched one id at a time. GraphQL aliases batch them.
//! - Requesting `tavg` fails the *entire batch* with
//!   `Float cannot represent non numeric value: nan`. It is never requested.

use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::{Curve, Error, ErrorCode, MeasurementKind, Provenance, Result, SpectrumRecord};

pub const ENDPOINT: &str = "https://www.fpbase.org/graphql/";

/// FPbase's stated terms for the data it hosts.
pub const LICENSE: &str = "FPbase: free of copyright restrictions, commercial and \
non-commercial use, attribution requested; upstream third-party rights not cleared";

/// Curves are large; 20 per request keeps responses near 350 KB.
pub const CURVE_BATCH: usize = 20;

/// Metadata-only queries are small, so batch harder.
pub const META_BATCH: usize = 50;

/// Politeness delay between requests.
pub const REQUEST_DELAY: Duration = Duration::from_millis(120);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpectrumIndexEntry {
    pub id: String,
    pub category: String,
    pub subtype: String,
    pub owner_name: String,
}

pub struct Client {
    endpoint: String,
    delay: Duration,
    agent: ureq::Agent,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    pub fn new() -> Self {
        Self {
            endpoint: ENDPOINT.to_string(),
            delay: REQUEST_DELAY,
            agent: ureq::Agent::new_with_defaults(),
        }
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    fn query(&self, query: &str) -> Result<Value> {
        let body = serde_json::json!({ "query": query }).to_string();
        let mut response = self
            .agent
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .header("User-Agent", "numanager-spectra")
            .send(body.as_str())
            .map_err(|e| Error::new(ErrorCode::Transport, e.to_string()))?;

        let text = response
            .body_mut()
            .read_to_string()
            .map_err(|e| Error::new(ErrorCode::Transport, e.to_string()))?;

        let parsed: Value = serde_json::from_str(&text)
            .map_err(|e| Error::new(ErrorCode::Protocol, e.to_string()))?;

        // GraphQL returns partial data alongside errors: a handful of filters
        // have no spectrum, and erroring out would discard 11k valid configs
        // over 12 bad ones. Only a wholly absent `data` is fatal.
        if parsed.get("data").map(Value::is_null).unwrap_or(true) {
            let errors = parsed
                .get("errors")
                .map(ToString::to_string)
                .unwrap_or_else(|| "no data in response".to_string());
            return Err(Error::new(ErrorCode::Protocol, errors));
        }
        Ok(parsed)
    }

    /// List every spectrum of a category. `category` is `F`, `D`, `P`, `L`, or `C`.
    pub fn index(&self, category: &str) -> Result<Vec<SpectrumIndexEntry>> {
        let query = format!(
            "{{ spectra(category:\"{category}\") {{ id category subtype owner {{ name }} }} }}"
        );
        let parsed = self.query(&query)?;
        let items = parsed["data"]["spectra"]
            .as_array()
            .ok_or_else(|| Error::new(ErrorCode::Protocol, "spectra: expected an array"))?;

        Ok(items
            .iter()
            .filter_map(|item| {
                Some(SpectrumIndexEntry {
                    id: item["id"].as_str()?.to_string(),
                    category: item["category"].as_str().unwrap_or_default().to_string(),
                    subtype: item["subtype"].as_str().unwrap_or_default().to_string(),
                    owner_name: item["owner"]["name"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                })
            })
            .collect())
    }

    /// Fetch curves for the given ids, batching with GraphQL aliases.
    ///
    /// `include_filter_metadata` pulls manufacturer/part, which only exists on
    /// filter-owned spectra. A batch that fails is retried id by id, so one bad
    /// record costs one record rather than the whole batch.
    pub fn records(
        &self,
        entries: &[SpectrumIndexEntry],
        include_filter_metadata: bool,
        abs_tol: f64,
        rel_tol: f64,
        mut progress: impl FnMut(usize, usize),
    ) -> Result<Vec<SpectrumRecord>> {
        let retrieved = today_iso();
        let mut records = Vec::with_capacity(entries.len());

        for chunk in entries.chunks(CURVE_BATCH) {
            let payload = self
                .query(&curve_query(chunk, include_filter_metadata))
                .ok();

            match payload {
                Some(value) => {
                    for (index, entry) in chunk.iter().enumerate() {
                        let node = &value["data"][format!("s{index}")];
                        if let Some(record) =
                            record_from_node(node, entry, &retrieved, abs_tol, rel_tol)
                        {
                            records.push(record);
                        }
                    }
                }
                None => {
                    // Fall back to one request per id so a single bad value
                    // does not take the other 19 with it.
                    for entry in chunk {
                        let single = std::slice::from_ref(entry);
                        if let Ok(value) = self.query(&curve_query(single, include_filter_metadata))
                        {
                            let node = &value["data"]["s0"];
                            if let Some(record) =
                                record_from_node(node, entry, &retrieved, abs_tol, rel_tol)
                            {
                                records.push(record);
                            }
                        }
                        sleep(self.delay);
                    }
                }
            }

            progress(records.len(), entries.len());
            sleep(self.delay);
        }

        Ok(records)
    }
}

/// One filter in one cube: a single row of the cube table.
///
/// Cube composition is factual (which parts a product contains) rather than
/// measured, which is why these are marked redistributable while curves are
/// not. See `docs/reference/filter_spectra_databases.md`.
#[derive(Debug, Clone, PartialEq)]
pub struct CubePlacement {
    pub config_id: String,
    pub cube_name: String,
    pub collection: String,
    pub collection_id: String,
    /// `EX`, `BS`, or `EM`.
    pub role: String,
    pub reflects: bool,
    pub filter_id: String,
    pub filter_name: String,
    pub manufacturer: Option<String>,
    pub part: Option<String>,
    pub bandcenter_nm: Option<f64>,
    pub bandwidth_nm: Option<f64>,
    /// Joins to `SpectrumRecord::provenance.source_id`.
    pub spectrum_id: Option<String>,
    pub provenance: Provenance,
}

const CUBE_QUERY: &str = "{ opticalConfigs { id name microscope { id name } \
filters { path reflects spectrumId \
filter { id name manufacturer part bandcenter bandwidth } } } }";

impl Client {
    /// Fetch every optical configuration and flatten it to one row per filter.
    ///
    /// A single request returns the lot (~11k configs, ~29k placements, under a
    /// megabyte compressed), so there is no batching here.
    pub fn cubes(&self) -> Result<Vec<CubePlacement>> {
        let parsed = self.query(CUBE_QUERY)?;
        let configs = parsed["data"]["opticalConfigs"]
            .as_array()
            .ok_or_else(|| Error::new(ErrorCode::Protocol, "opticalConfigs: expected an array"))?;
        let retrieved = today_iso();

        let mut placements = Vec::new();
        for config in configs {
            let config_id = config["id"].as_str().unwrap_or_default().to_string();
            let cube_name = config["name"].as_str().unwrap_or_default().to_string();
            let collection = config["microscope"]["name"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let collection_id = config["microscope"]["id"]
                .as_str()
                .unwrap_or_default()
                .to_string();

            let Some(filters) = config["filters"].as_array() else {
                continue;
            };
            for placement in filters {
                let filter = &placement["filter"];
                let text = |value: &Value| -> Option<String> {
                    value
                        .as_str()
                        .filter(|text| !text.is_empty())
                        .map(str::to_string)
                };
                placements.push(CubePlacement {
                    config_id: config_id.clone(),
                    cube_name: cube_name.clone(),
                    collection: collection.clone(),
                    collection_id: collection_id.clone(),
                    role: placement["path"].as_str().unwrap_or_default().to_string(),
                    reflects: placement["reflects"].as_bool().unwrap_or(false),
                    filter_id: filter["id"].as_str().unwrap_or_default().to_string(),
                    filter_name: filter["name"].as_str().unwrap_or_default().to_string(),
                    manufacturer: text(&filter["manufacturer"]),
                    part: text(&filter["part"]),
                    bandcenter_nm: filter["bandcenter"].as_f64(),
                    bandwidth_nm: filter["bandwidth"].as_f64(),
                    spectrum_id: text(&placement["spectrumId"]),
                    provenance: Provenance {
                        source: "fpbase".to_string(),
                        source_url: format!("https://www.fpbase.org/microscope/{collection_id}/"),
                        source_id: config_id.clone(),
                        retrieved: retrieved.clone(),
                        license: LICENSE.to_string(),
                        measurement_kind: MeasurementKind::NominalFromDesignation,
                        // Composition is factual: which parts a product
                        // contains, plus catalogue numbers. Unlike measured
                        // curves, this carries no third-party measurement right.
                        redistributable: true,
                    },
                });
            }
        }
        Ok(placements)
    }
}

fn curve_query(entries: &[SpectrumIndexEntry], include_filter_metadata: bool) -> String {
    // `tavg` is deliberately absent: it returns nan and fails the whole batch.
    let owner = if include_filter_metadata {
        " ownerFilter { manufacturer part name bandcenter bandwidth }"
    } else {
        ""
    };
    let mut query = String::from("{");
    for (index, entry) in entries.iter().enumerate() {
        query.push_str(&format!(
            " s{index}: spectrum(id:{}) {{ id category subtype data{owner} }}",
            entry.id
        ));
    }
    query.push_str(" }");
    query
}

fn record_from_node(
    node: &Value,
    entry: &SpectrumIndexEntry,
    retrieved: &str,
    abs_tol: f64,
    rel_tol: f64,
) -> Option<SpectrumRecord> {
    let points = node.get("data")?.as_array()?;

    // Upstream curves contain nulls; roughly 0.3% are null throughout.
    let samples: Vec<(f64, f64)> = points
        .iter()
        .filter_map(|point| {
            let pair = point.as_array()?;
            Some((pair.first()?.as_f64()?, pair.get(1)?.as_f64()?))
        })
        .collect();
    if samples.len() < 2 {
        return None;
    }

    let owner = node.get("ownerFilter");
    let text = |key: &str| -> Option<String> {
        owner?
            .get(key)?
            .as_str()
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };

    let name = text("name").unwrap_or_else(|| entry.owner_name.clone());
    let subtype = node["subtype"]
        .as_str()
        .unwrap_or(&entry.subtype)
        .to_string();
    let category = node["category"]
        .as_str()
        .unwrap_or(&entry.category)
        .to_string();

    Some(SpectrumRecord {
        name,
        manufacturer: text("manufacturer"),
        part: text("part"),
        category,
        subtype,
        curve: Curve::from_samples(&samples, abs_tol, rel_tol),
        provenance: Provenance {
            source: "fpbase".to_string(),
            source_url: format!("https://www.fpbase.org/spectra/{}/", entry.id),
            source_id: entry.id.clone(),
            retrieved: retrieved.to_string(),
            license: LICENSE.to_string(),
            // FPbase does not distinguish measured from theoretical curves.
            measurement_kind: MeasurementKind::Unknown,
            // FPbase dedicates its own data, but vendor-deposited curves carry
            // rights FPbase explicitly cannot clear. Opt in per record instead.
            redistributable: false,
        },
    })
}

/// Current UTC date as `YYYY-MM-DD`.
///
/// Hand-rolled rather than pulling in a date crate, matching the workspace's
/// minimal-dependency style. Uses the civil-from-days algorithm.
pub fn today_iso() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_query_never_requests_tavg() {
        let entries = vec![SpectrumIndexEntry {
            id: "6636".into(),
            category: "F".into(),
            subtype: "BS".into(),
            owner_name: "test".into(),
        }];
        let query = curve_query(&entries, true);
        assert!(!query.contains("tavg"), "tavg fails the whole batch");
        assert!(query.contains("s0: spectrum(id:6636)"));
        assert!(query.contains("ownerFilter"));
    }

    #[test]
    fn curve_query_batches_with_aliases() {
        let entries: Vec<_> = (0..3)
            .map(|i| SpectrumIndexEntry {
                id: i.to_string(),
                category: "F".into(),
                subtype: "BP".into(),
                owner_name: String::new(),
            })
            .collect();
        let query = curve_query(&entries, false);
        assert!(query.contains("s0:") && query.contains("s1:") && query.contains("s2:"));
        assert!(!query.contains("ownerFilter"));
    }

    #[test]
    fn record_skips_all_null_curves() {
        let entry = SpectrumIndexEntry {
            id: "6954".into(),
            category: "F".into(),
            subtype: "BS".into(),
            owner_name: "nulls".into(),
        };
        let node = serde_json::json!({
            "id": "6954", "category": "F", "subtype": "BS",
            "data": [[400.0, null], [401.0, null], [402.0, null]]
        });
        assert!(record_from_node(&node, &entry, "2026-01-01", 0.01, 0.10).is_none());
    }

    #[test]
    fn record_drops_partial_nulls_but_keeps_curve() {
        let entry = SpectrumIndexEntry {
            id: "1".into(),
            category: "F".into(),
            subtype: "BP".into(),
            owner_name: "partial".into(),
        };
        let node = serde_json::json!({
            "id": "1", "category": "F", "subtype": "BP",
            "data": [[400.0, 0.0], [401.0, null], [402.0, 0.9], [403.0, 0.0]]
        });
        let record = record_from_node(&node, &entry, "2026-01-01", 0.01, 0.10).unwrap();
        assert!(!record.curve.is_empty());
        assert_eq!(record.provenance.source, "fpbase");
        assert!(!record.provenance.redistributable);
    }

    #[test]
    fn civil_from_days_matches_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }
}
