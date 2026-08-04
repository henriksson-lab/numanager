//! Turning capability requests into Spark commands, and replies back into completions.
//!
//! This is the part of the driver that knows the instrument. Above it, a client asks for a
//! plate move or a measurement in the vocabulary every plate reader shares; below it, the
//! session speaks TDCL. Nothing on either side has to know about the other.
//!
//! # What comes back from a measurement
//!
//! Raw counts, plus the settings they were taken under — never an optical density. The
//! instrument has no idea whether a well is a blank, what path length the liquid makes, or
//! which of several OD conventions an assay wants, and a driver that guessed would bake
//! that guess into every archived run. Counts keep the run recomputable.
//!
//! # Status
//!
//! Written from the protocol notes, not yet run against an instrument. Command forms marked
//! "to confirm" below are the ones most likely to be wrong; they fail visibly the first time
//! they meet hardware, which is the intent — a wrong-but-declared command is better than a
//! silently absent one.

use super::catalog::MeasurementMode;
use super::commands::Command;
use super::data;
use super::parse::parse_kv_map;
use super::session::Outcome;
use super::tdcl::FrameType;
use numanager_core::{
    CapabilityRequest, DriverToken, GasConcentration, Temperature, TimeInterval, Value, Wavelength,
};
use std::collections::BTreeMap;

/// What a submitted command was for, so its reply can be turned back into the right shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// A command that only needs acknowledging.
    Acknowledge,
    /// A measurement, whose data frames carry counts.
    Measure {
        detector: Detector,
        well: String,
        wavelength_nm: Option<u32>,
    },
    /// A property read whose `KEY=VALUE` reply is the answer.
    Read { key: String },
    PlateMove { well: String },
}

/// Which detector a measurement used. They differ in what the data package contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detector {
    Absorbance,
    Fluorescence,
    Luminescence,
}

impl Detector {
    fn mode(self) -> MeasurementMode {
        match self {
            // Fluorescence is read from the top by default, which the instrument spells
            // FITOP rather than FI.
            Detector::Absorbance => MeasurementMode::Absorbance,
            Detector::Fluorescence => MeasurementMode::FluorescenceTop,
            Detector::Luminescence => MeasurementMode::Luminescence,
        }
    }
}

/// A command line to send, and what its reply will mean.
#[derive(Debug, Clone)]
pub struct Transaction {
    pub line: String,
    pub intent: Intent,
}

/// Build the commands a capability request needs.
///
/// Returns `None` for requests this instrument has no command for, which the caller should
/// treat as "handle it some other way" rather than as an error — the modeled path still
/// answers them.
pub fn plan_request(
    request: &CapabilityRequest,
    detector: Option<Detector>,
    well: &str,
    wavelength_nm: Option<u32>,
) -> Option<Vec<Transaction>> {
    match request {
        CapabilityRequest::PlateMove(move_request) => Some(vec![Transaction {
            // To confirm: the notes give `PLATEPOS <position>` for the carrier, but the
            // per-well addressing form has not been seen. A reader that positions by well
            // index would spell this differently.
            line: Command::set("PLATEPOS")
                .param("WELL", move_request.well.as_str())
                .build(),
            intent: Intent::PlateMove {
                well: move_request.well.clone(),
            },
        }]),

        CapabilityRequest::Measure(measure) => {
            let detector = detector?;
            let mut transactions = Vec::new();
            transactions.push(Transaction {
                line: Command::set("MODE")
                    .param("MEASUREMENT", detector.mode().wire_token())
                    .build(),
                intent: Intent::Acknowledge,
            });
            if let Some(nm) = wavelength_nm {
                // Wavelength on the command side is in ångström, not nanometres — the unit
                // vocabulary is device-driven and `ang` is what the range replies use.
                transactions.push(Transaction {
                    line: Command::set("WAVELENGTH").param("VALUE", nm * 10).build(),
                    intent: Intent::Acknowledge,
                });
            }
            if let Some(integration) = measure.integration_time {
                transactions.push(Transaction {
                    line: Command::set("INTEGRATION")
                        .param("TIME", (integration.seconds() * 1e6).round() as i64)
                        .build(),
                    intent: Intent::Acknowledge,
                });
            }
            transactions.push(Transaction {
                line: Command::set("MEASURE")
                    .param("WELL", well)
                    .param("MODE", detector.mode().wire_token())
                    .build(),
                intent: Intent::Measure {
                    detector,
                    well: well.to_string(),
                    wavelength_nm,
                },
            });
            Some(transactions)
        }

        CapabilityRequest::TemperatureControl(control) => {
            let mut transactions = Vec::new();
            if let Some(target) = control.target {
                transactions.push(Transaction {
                    // Hundredths of a degree: 37 C is TARGET=3700.
                    line: Command::set("TEMPERATURE")
                        .param("TARGET", (target.celsius() * 100.0).round() as i64)
                        .param("DEVICE", "AMBIENTCONTROL")
                        .build(),
                    intent: Intent::Acknowledge,
                });
            }
            if let Some(enabled) = control.enabled {
                transactions.push(Transaction {
                    line: Command::set("TEMPERATURE")
                        .param("CONTROL", if enabled { "ON" } else { "OFF" })
                        .param("DEVICE", "AMBIENTCONTROL")
                        .build(),
                    intent: Intent::Acknowledge,
                });
            }
            Some(transactions)
        }

        CapabilityRequest::GasControl(control) => {
            let mut transactions = Vec::new();
            if let Some(target) = control.co2_target {
                transactions.push(Transaction {
                    // Percent in hundred-thousandths: 5 % is 50000.
                    line: Command::set("GASCONTROL")
                        .param("GAS", "CO2")
                        .param("RATED_CONCENTRATION", (target.percent() * 10_000.0).round() as i64)
                        .build(),
                    intent: Intent::Acknowledge,
                });
            }
            if let Some(enabled) = control.enabled {
                transactions.push(Transaction {
                    line: Command::set("GASCONTROL")
                        .param("GAS", "CO2")
                        .param("CONTROL", if enabled { "ON" } else { "OFF" })
                        .build(),
                    intent: Intent::Acknowledge,
                });
            }
            Some(transactions)
        }

        _ => None,
    }
}

/// Read back the chamber, for the environmental telemetry a run records each cycle.
pub fn environment_reads() -> Vec<Transaction> {
    vec![
        Transaction {
            line: Command::query("SENSORVALUE")
                .word("TEMPERATURE")
                .param("DEVICE", "CUV")
                .build(),
            intent: Intent::Read {
                key: "TEMPERATURE".into(),
            },
        },
        Transaction {
            line: Command::query("GASCONTROL")
                .word("ACTUAL_CONCENTRATION")
                .param("GAS", "CO2")
                .build(),
            intent: Intent::Read {
                key: "ACTUAL_CONCENTRATION".into(),
            },
        },
    ]
}

/// Turn a completed transaction into the value a capability completion carries.
pub fn completion(intent: &Intent, outcome: &Outcome<DriverToken>) -> Value {
    match intent {
        Intent::Acknowledge => Value::Map(BTreeMap::from([(
            "acknowledged".into(),
            Value::Bool(true),
        )])),

        Intent::PlateMove { well } => Value::Map(BTreeMap::from([(
            "well".into(),
            Value::String(well.clone()),
        )])),

        Intent::Read { key } => {
            let map = parse_kv_map(&outcome.response.text);
            match map.get(key) {
                Some(text) => text
                    .trim()
                    .parse::<i64>()
                    .map(Value::I64)
                    .unwrap_or_else(|_| Value::String(text.clone())),
                None => Value::Null,
            }
        }

        Intent::Measure {
            detector,
            well,
            wavelength_nm,
        } => {
            let mut map = BTreeMap::from([("well".into(), Value::String(well.clone()))]);
            if let Some(nm) = wavelength_nm {
                map.insert(
                    "wavelength".into(),
                    Value::Wavelength(Wavelength::from_nanometers(*nm as f64)),
                );
            }
            match decode_counts(*detector, outcome) {
                Some(counts) => {
                    map.insert("reference".into(), Value::I64(counts.reference as i64));
                    map.insert("measurement".into(), Value::I64(counts.measurement as i64));
                    if let Some(gain) = counts.gain {
                        map.insert("gain".into(), Value::I64(gain as i64));
                    }
                }
                // No decodable data package. Reporting the acknowledgement without counts
                // is honest; inventing a zero would look like a real reading of nothing.
                None => {
                    map.insert("decoded".into(), Value::Bool(false));
                }
            }
            Value::Map(map)
        }
    }
}

struct Counts {
    reference: u32,
    measurement: u32,
    gain: Option<u32>,
}

/// Pull raw counts out of a measurement's data frames.
///
/// The header frame (`0x88`) announces the field layout and the binary frame (`0x83`)
/// carries the values; without both there is nothing to decode.
fn decode_counts(detector: Detector, outcome: &Outcome<DriverToken>) -> Option<Counts> {
    let header = outcome
        .data
        .iter()
        .find(|frame| frame.type_ == FrameType::DataHeader as u8)?;
    let payload = outcome
        .data
        .iter()
        .find(|frame| frame.type_ == FrameType::Binary as u8)?;
    let codes = data::parse_header(&header.payload).ok()?;
    let fields = data::decode(&codes, &payload.payload).ok()?;

    match detector {
        Detector::Absorbance | Detector::Fluorescence => {
            if fields.len() < 2 {
                return None;
            }
            Some(Counts {
                reference: fields[0].raw,
                measurement: fields[1].raw,
                gain: fields.get(2).map(|field| field.raw),
            })
        }
        // Luminescence has no excitation and so no reference channel: the package is a
        // single count.
        Detector::Luminescence => Some(Counts {
            reference: 0,
            measurement: fields.first()?.raw,
            gain: None,
        }),
    }
}

/// Integration time as the instrument reports it, for the settings that travel with counts.
pub fn integration_value(seconds: f64) -> Value {
    Value::TimeInterval(TimeInterval::from_seconds(seconds))
}

/// Chamber temperature from a `SENSORVALUE` reply, which is in hundredths of a degree.
pub fn temperature_from_c100(raw: i64) -> Temperature {
    Temperature::from_celsius(raw as f64 / 100.0)
}

/// CO2 from a `GASCONTROL` reply, which is in hundred-thousandths of a percent.
pub fn gas_from_scaled(raw: i64) -> GasConcentration {
    GasConcentration::from_percent(raw as f64 / 10_000.0)
}
