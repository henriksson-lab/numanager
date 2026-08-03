//! Download spectra from FPbase and write them as Parquet.
//!
//! This is a build-time tool, not a runtime dependency. Microscope machines are
//! routinely offline, so the GUI reads a generated file rather than the network.
//!
//! ```sh
//! cargo run -p numanager-spectra --features fetch --bin fetch-spectra -- \
//!     --out data/spectra-filters.parquet
//! ```

use std::process::ExitCode;

use numanager_spectra::{fpbase, store, DEFAULT_ABS_TOL, DEFAULT_REL_TOL};

struct Options {
    out: String,
    input: String,
    kind: String,
    category: String,
    limit: Option<usize>,
    abs_tol: f64,
    rel_tol: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            out: "spectra.parquet".to_string(),
            input: "filter-cubes.parquet".to_string(),
            kind: "spectra".to_string(),
            category: "F".to_string(),
            limit: None,
            abs_tol: DEFAULT_ABS_TOL,
            rel_tol: DEFAULT_REL_TOL,
        }
    }
}

const USAGE: &str = "\
fetch-spectra: download spectra from FPbase into Parquet

    --out <path>       output file (default: spectra.parquet)
    --kind <k>         spectra (curves), cubes (filter set composition),
                       or bands (nominal bands parsed from designations)
    --in <path>        cube table to read for --kind bands
    --category <c>     F filters, D dyes, P proteins, L lights, C cameras (default: F)
    --limit <n>        stop after n spectra, for smoke tests
    --abs-tol <f>      absolute transmission tolerance (default: 0.01)
    --rel-tol <f>      relative tolerance (default: 0.10)
    -h, --help         this message";

fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut index = 0;
    while index < args.len() {
        let key = args[index].as_str();
        let mut value = || -> Result<String, String> {
            index += 1;
            args.get(index)
                .cloned()
                .ok_or_else(|| format!("{key} needs a value"))
        };
        match key {
            "--out" => options.out = value()?,
            "--kind" => options.kind = value()?,
            "--in" => options.input = value()?,
            "--category" => options.category = value()?,
            "--limit" => {
                options.limit = Some(value()?.parse().map_err(|_| "--limit needs a number")?)
            }
            "--abs-tol" => {
                options.abs_tol = value()?.parse().map_err(|_| "--abs-tol needs a number")?
            }
            "--rel-tol" => {
                options.rel_tol = value()?.parse().map_err(|_| "--rel-tol needs a number")?
            }
            "-h" | "--help" => return Err(USAGE.to_string()),
            other => return Err(format!("unrecognized argument: {other}\n\n{USAGE}")),
        }
        index += 1;
    }
    Ok(options)
}

fn derive_bands(options: &Options) -> ExitCode {
    // Offline: designations are already in the cube table.
    let cubes = match store::read_frame(&options.input) {
        Ok(frame) => frame,
        Err(error) => {
            eprintln!("could not read {}: {error}", options.input);
            return ExitCode::FAILURE;
        }
    };

    let (mut frame, stats) = match store::derive_bands(&cubes) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("parse failed: {error}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(error) = store::write_frame(&mut frame, &options.out) {
        eprintln!("write failed: {error}");
        return ExitCode::FAILURE;
    }

    let percent = |n: usize| 100.0 * n as f64 / stats.filters.max(1) as f64;
    eprintln!(
        "{} distinct filters, {} parsed ({:.1}%), {} bands",
        stats.filters,
        stats.parsed,
        percent(stats.parsed),
        stats.bands
    );
    if stats.inconsistent > 0 {
        eprintln!(
            "{} parsed with a band count differing from the prefix",
            stats.inconsistent
        );
    }
    eprintln!(
        "cross-check against upstream bandcenter: {} compared, {} disagreed by >1 nm",
        stats.compared, stats.center_mismatches
    );
    if stats.upstream_implausible > 0 {
        eprintln!(
            "{} upstream bandcenters are not plausible wavelengths and were not compared",
            stats.upstream_implausible
        );
    }

    let size = std::fs::metadata(&options.out)
        .map(|m| m.len())
        .unwrap_or(0);
    eprintln!("wrote {:.2} MB -> {}", size as f64 / 1e6, options.out);
    ExitCode::SUCCESS
}

fn fetch_cubes(client: &fpbase::Client, options: &Options) -> ExitCode {
    eprintln!("fetching optical configurations ...");
    let placements = match client.cubes() {
        Ok(placements) => placements,
        Err(error) => {
            eprintln!("cube fetch failed: {error}");
            return ExitCode::FAILURE;
        }
    };

    let cubes: std::collections::BTreeSet<&str> =
        placements.iter().map(|p| p.config_id.as_str()).collect();
    let with_spectrum = placements
        .iter()
        .filter(|p| p.spectrum_id.is_some())
        .count();

    if let Err(error) = store::write_cubes(&placements, &options.out) {
        eprintln!("write failed: {error}");
        return ExitCode::FAILURE;
    }

    let size = std::fs::metadata(&options.out)
        .map(|m| m.len())
        .unwrap_or(0);
    eprintln!(
        "wrote {} placements across {} cubes ({} joinable to a spectrum), {:.2} MB -> {}",
        placements.len(),
        cubes.len(),
        with_spectrum,
        size as f64 / 1e6,
        options.out
    );
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse(&args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };

    let client = fpbase::Client::new();

    match options.kind.as_str() {
        "cubes" => return fetch_cubes(&client, &options),
        "bands" => return derive_bands(&options),
        "spectra" => {}
        other => {
            eprintln!("unknown --kind {other}\n\n{USAGE}");
            return ExitCode::FAILURE;
        }
    }

    eprintln!("listing category {} ...", options.category);
    let mut index = match client.index(&options.category) {
        Ok(index) => index,
        Err(error) => {
            eprintln!("index failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(limit) = options.limit {
        index.truncate(limit);
    }
    eprintln!("{} spectra to fetch", index.len());

    // manufacturer/part only exist on filter-owned spectra.
    let filters = options.category == "F";
    let mut last_report = 0usize;
    let records = client.records(
        &index,
        filters,
        options.abs_tol,
        options.rel_tol,
        |done, total| {
            if done >= last_report + 200 || done == total {
                eprintln!("  {done}/{total}");
                last_report = done;
            }
        },
    );

    let records = match records {
        Ok(records) => records,
        Err(error) => {
            eprintln!("fetch failed: {error}");
            return ExitCode::FAILURE;
        }
    };

    let vertices: usize = records.iter().map(|r| r.curve.len()).sum();
    let dropped = index.len() - records.len();
    if dropped > 0 {
        eprintln!("{dropped} spectra dropped (null or empty curves)");
    }

    if let Err(error) = store::write(&records, &options.out) {
        eprintln!("write failed: {error}");
        return ExitCode::FAILURE;
    }

    let size = std::fs::metadata(&options.out)
        .map(|m| m.len())
        .unwrap_or(0);
    eprintln!(
        "wrote {} records, {vertices} vertices ({:.1} avg), {:.2} MB -> {}",
        records.len(),
        vertices as f64 / records.len().max(1) as f64,
        size as f64 / 1e6,
        options.out
    );
    ExitCode::SUCCESS
}
