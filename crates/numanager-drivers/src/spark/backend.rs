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
//! # Three answers, not two
//!
//! [`plan_request`] returns [`Planned`], which distinguishes "here are the commands" from
//! "this instrument has no established command for that" from "nothing to send". The
//! distinction is the whole point: a driver with a transport attached that answered an
//! unsupported request from its own model would look like it was working, which is worse
//! than failing.
//!
//! # Status
//!
//! Written from `docs/reverse/spark-cyto-protocol.md`, not yet run against an instrument.
//! Commands whose keyword and parameters both come from the recovered command dictionary are
//! marked `// dictionary`; the rest are inferred around them and are the ones most likely to
//! be wrong. They fail visibly the first time they meet hardware, which is the intent — a
//! wrong-but-declared command is better than a silently absent one.

use super::catalog::{
    BarcodePosition, InjectorPump, MeasurementMode, MoveableCarrier, MtpMotor, ObjectiveType,
    PlatePosition, TemperatureDevice,
};
use super::commands::Command;
use super::data;
use super::parse::parse_kv_map;
use super::session::Outcome;
use super::tdcl::FrameType;
use numanager_core::{
    CapabilityRequest, DriverToken, GasConcentration, InjectAction, PixelCount, Position,
    StageAxis, Temperature, TimeInterval, Value, Wavelength,
};

/// The imaging module's focus parameter. The main board's `ABSOLUTE` takes `X`/`Y`/`Z`; the
/// imaging module's takes `Z_OBJECTIVE`, and answering a focus move with a bare `Z` would
/// move the plate stage instead.
const Z_OBJECTIVE: &str = "Z_OBJECTIVE";
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
    Read {
        key: String,
    },
    /// A `#`-prefixed range query, whose reply declares an axis's travel *and its unit*.
    AxisRange {
        axis: StageAxis,
    },
    /// A position readback, whose reply carries one `KEY=VALUE` per axis.
    Position,
    PlateMove {
        position: PlatePosition,
    },
    /// A barcode read, whose reply carries the decoded text.
    Barcode,
    /// A pre-measurement reference read, whose data package is the blank.
    Prepare {
        detector: Detector,
    },
    /// An autofocus sweep, whose detail reply reports the peak it found.
    Autofocus,
    /// An identity or state query, whose `KEY=VALUE` reply names the instrument.
    Identity {
        key: String,
    },
    /// A module enumeration, whose reply maps module names to their numbers. `final_bus` is
    /// set on the last one asked, so a driver knows when the picture is complete.
    ModuleMap {
        final_bus: bool,
    },
    /// A carrier's slide inventory: what is fitted, and therefore how many positions exist.
    CarrierInventory {
        carrier: MoveableCarrier,
    },
    /// An image acquisition, whose data frames carry raw pixels.
    Capture {
        width: u32,
        height: u32,
        bits_per_pixel: u8,
    },
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

/// What a device *is* to the instrument, resolved once by the driver from its kind tags.
///
/// A capability request says what to do; this says to whom. Keeping them apart is what lets
/// one planner serve eight devices without the driver passing a widening list of positional
/// arguments for state that only some subjects need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    PlateTransport,
    Detector(Detector),
    Temperature,
    Gas,
    ImagingHead,
    CameraBinding,
    /// A motion axis. The XY device owns two of them and issues one command per axis.
    Axes(Vec<MtpMotor>),
    /// A moveable optics carrier — an excitation/emission filter slide or a mirror.
    Carrier(MoveableCarrier),
    /// The injector pair. Which pump acts is carried by the request.
    Injector,
    Barcode,
    /// The imaging camera, as the reader presents it.
    Camera,
    /// The camera's own autofocus sweep.
    Autofocus,
    /// The plate shaker.
    Shaker,
    /// The lid lifter.
    Lid,
}

/// Driver-side state a request does not carry, and the module numbers to address.
#[derive(Debug, Clone, Default)]
pub struct PlanState {
    /// The well the driver believes the next measurement addresses.
    pub well: String,
    /// The detector's tuned wavelength, when it has one.
    pub wavelength_nm: Option<u32>,
    /// The label index a `SCAN` reports under.
    pub label: u32,
    /// Which unit each axis speaks, once its range reply has said so. An axis missing from
    /// this map has not been asked yet, and cannot be commanded in canonical units.
    pub axis_units: BTreeMap<StageAxis, AxisUnit>,
    /// How many positions each carrier's fitted slide has, once it has said. A carrier
    /// missing from this map has not answered, which is not the same as having none.
    pub carrier_slots: BTreeMap<String, u8>,
    pub modules: Modules,
    /// What the camera is currently set to, as last read back from it.
    pub camera: CameraState,
}

/// The camera geometry and depth an acquisition will produce.
///
/// Read from the instrument (`?CAMERA AOI`, `?CAMERA BITSPERPIXEL`) rather than assumed: the
/// sensor's size is queried at runtime by the vendor stack too, and nothing in the evidence
/// fixes it. Until it has been read, the payload cannot be shaped into an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CameraState {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub bits_per_pixel: Option<u8>,
    /// The exposure the driver holds, in microseconds — the unit the command takes.
    pub exposure_us: Option<i64>,
    /// Which imaging module answers `TAKEIMAGE`. The brightfield path wants a `TYPE=`.
    pub cell_imaging: bool,
}

/// What an axis's range reply said its positions are counted in.
///
/// The instrument declares this per axis (`{from}~{to}%{step} [unit]`), which is why nothing
/// here converts steps to micrometres: on an axis that counts steps the conversion is a
/// property of the mechanism, and inventing one would put a plausible wrong number into
/// every archived position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisUnit {
    Micrometres,
    Steps,
}

/// Module numbers for the subsystems that are addressed by one.
///
/// Only the main board's number is fixed — it is always 0. The rest are assigned at
/// enumeration and come from `#MODULE EXPECTED_USB` / `EXPECTED_CAN`; until the instrument
/// has said, a command is sent without a `MODULE=` token rather than with a guessed one.
#[derive(Debug, Clone, Default)]
pub struct Modules {
    pub imaging: Option<u32>,
    pub injector: Option<u32>,
    pub gas: Option<u32>,
    pub barcode: Option<u32>,
    /// Whether the imaging module present is the brightfield cell imager rather than the
    /// fluorescence head. They take the same commands but `TAKEIMAGE` differs.
    pub cell_imaging: bool,
}

impl Modules {
    /// Fill in what an enumeration reply said.
    ///
    /// Names not in the reply are left alone, so a USB enumeration followed by a CAN one
    /// accumulates rather than the second erasing the first.
    pub fn apply(&mut self, enumerated: &BTreeMap<String, u32>) {
        for (name, number) in enumerated {
            match name.as_str() {
                "FIM" => self.imaging = Some(*number),
                "CELL" => {
                    self.imaging = Some(*number);
                    self.cell_imaging = true;
                }
                "INJ" => self.injector = Some(*number),
                "GCM" => self.gas = Some(*number),
                "BARCODE" => self.barcode = Some(*number),
                _ => {}
            }
        }
        // A fluorescence head takes precedence: an instrument carrying both answers
        // `TAKEIMAGE` on FIM without the brightfield module's `TYPE=`.
        if enumerated.contains_key("FIM") {
            self.cell_imaging = false;
        }
    }
}

/// `NAME:NUMBER|NAME:NUMBER` — how a module enumeration answers.
pub fn parse_module_map(text: &str) -> BTreeMap<String, u32> {
    text.split('|')
        .filter_map(|entry| {
            let (name, number) = entry.trim().rsplit_once(':')?;
            Some((
                name.trim().to_ascii_uppercase(),
                number.trim().parse::<u32>().ok()?,
            ))
        })
        .collect()
}

/// Ask the instrument what it is, before asking it to do anything.
///
/// A driver that connects without checking has no way to notice it reached the wrong
/// instrument, or one that is not ready to be driven.
pub fn identity_reads() -> Vec<Transaction> {
    [
        "SAP_SERIAL_INSTRUMENT",
        "INSTRUMENT_TYPE",
        "HARDWARE_VERSION",
    ]
    .into_iter()
    .map(|key| Transaction {
        line: Command::query("INFO").word(key).build(), // dictionary
        intent: Intent::Identity { key: key.into() },
    })
    .chain(std::iter::once(Transaction {
        line: Command::query("INSTRUMENT").word("STATE").build(), // dictionary
        intent: Intent::Identity {
            key: "STATE".into(),
        },
    }))
    .collect()
}

/// Ask which modules are fitted and what numbers they answer on.
pub fn module_reads() -> Vec<Transaction> {
    const BUSES: [&str; 2] = ["EXPECTED_USB", "EXPECTED_CAN"];
    BUSES
        .into_iter()
        .enumerate()
        .map(|(index, bus)| Transaction {
            line: Command::range("MODULE").word(bus).build(), // dictionary
            intent: Intent::ModuleMap {
                final_bus: index + 1 == BUSES.len(),
            },
        })
        .collect()
}

/// A command line to send, and what its reply will mean.
#[derive(Debug, Clone)]
pub struct Transaction {
    pub line: String,
    pub intent: Intent,
}

impl Transaction {
    fn ack(line: String) -> Self {
        Transaction {
            line,
            intent: Intent::Acknowledge,
        }
    }
}

/// What a capability request turns into.
#[derive(Debug, Clone)]
pub enum Planned {
    /// Send these, in order. The operation completes when the last one does.
    Wire(Vec<Transaction>),
    /// Nothing crosses the wire: the request changes only driver-side state.
    Local,
    /// No command for this is established. A driver with a transport attached must fail the
    /// operation with this sentence rather than answer from its model.
    Unsupported(String),
}

/// Build the commands a capability request needs.
pub fn plan_request(request: &CapabilityRequest, subject: &Subject, state: &PlanState) -> Planned {
    match (subject, request) {
        (Subject::PlateTransport, CapabilityRequest::PlateMove(move_request)) => {
            // `PLATEPOS` addresses the carrier, not a well: the recovered `PlatePosition`
            // vocabulary is PLATE_IN/OUT_LEFT/OUT_RIGHT and nine other transport stations,
            // with no well among them. Wells are addressed by the measurement sequencer.
            match plate_position(&move_request.well) {
                Some(position) => Planned::Wire(vec![Transaction {
                    line: Command::set("PLATEPOS") // dictionary
                        .word(position.wire_token())
                        .build(),
                    intent: Intent::PlateMove { position },
                }]),
                None => Planned::Unsupported(format!(
                    "the plate transport moves between carrier stations, not to wells: \
                     '{}' is not one of {}. Well addressing happens inside a measurement; \
                     no per-well transport command is established for this instrument",
                    move_request.well,
                    plate_position_tokens()
                )),
            }
        }

        (Subject::Detector(detector), CapabilityRequest::Measure(measure)) => {
            let detector = *detector;
            // The mode and the optics are set *before* the window opens, and the reference
            // read happens before it too: `MODE` -> `PREPARE` -> optics -> `MEASUREMENT
            // START` -> `SCAN` -> `MEASUREMENT END`. Opening the window first would put the
            // configuration inside the measurement it configures.
            let mut transactions = vec![Transaction::ack(
                Command::set("MODE") // dictionary
                    .param("MEASUREMENT", detector.mode().wire_token())
                    .build(),
            )];
            if let Some(nm) = state.wavelength_nm {
                // Wavelength on the command side is in ångström, not nanometres — the unit
                // vocabulary is device-driven and `ang` is what the range replies use.
                //
                // To confirm: that wavelengths cross in ångström is recorded; the keyword
                // that tunes the monochromator is not in the dictionary, and this spelling
                // is inferred.
                transactions.push(Transaction::ack(
                    Command::set("WAVELENGTH").param("VALUE", nm * 10).build(),
                ));
            }
            if let Some(integration) = measure.integration_time {
                transactions.push(Transaction::ack(
                    Command::set("TIME") // dictionary
                        .param("INTEGRATION", (integration.seconds() * 1e6).round() as i64)
                        .build(),
                ));
            }
            // `PREPARE MODE=.. REFERENCE=YES|NO LABEL=..` runs the pre-measurement read and
            // emits its own data package — the blank the absorbance ratio divides by.
            // Luminescence has no reference channel, so it asks for none.
            let reference = !matches!(detector, Detector::Luminescence);
            transactions.push(Transaction {
                line: Command::set("PREPARE") // dictionary
                    .param("MODE", detector.mode().wire_token())
                    .param("REFERENCE", if reference { "YES" } else { "NO" })
                    .param("LABEL", state.label)
                    .build(),
                intent: Intent::Prepare { detector },
            });
            transactions.push(Transaction::ack(
                Command::set("MEASUREMENT").word("START").build(), // dictionary
            ));
            transactions.push(Transaction {
                line: Command::set("SCAN").param("LABEL", state.label).build(), // dictionary
                intent: Intent::Measure {
                    detector,
                    well: state.well.clone(),
                    wavelength_nm: state.wavelength_nm,
                },
            });
            transactions.push(Transaction::ack(
                Command::set("MEASUREMENT").word("END").build(), // dictionary
            ));
            Planned::Wire(transactions)
        }

        (Subject::Temperature, CapabilityRequest::TemperatureControl(control)) => {
            let mut transactions = Vec::new();
            if let Some(target) = control.target {
                transactions.push(Transaction::ack(
                    // Hundredths of a degree: 37 C is TARGET=3700.
                    Command::set("TEMPERATURE") // dictionary
                        .param("DEVICE", TemperatureDevice::AmbientControl.wire_token())
                        .param("TARGET", (target.celsius() * 100.0).round() as i64)
                        .build(),
                ));
            }
            if let Some(enabled) = control.enabled {
                // To confirm: the dictionary records `TEMPERATURE DEVICE=… TARGET=…` but not
                // how control is switched off.
                transactions.push(Transaction::ack(
                    Command::set("TEMPERATURE")
                        .param("DEVICE", TemperatureDevice::AmbientControl.wire_token())
                        .param("MODE", if enabled { "ON" } else { "OFF" })
                        .build(),
                ));
            }
            Planned::Wire(transactions)
        }

        (Subject::Gas, CapabilityRequest::GasControl(control)) => {
            let mut transactions = Vec::new();
            for (gas, target) in [("CO2", control.co2_target), ("O2", control.o2_target)] {
                let Some(target) = target else { continue };
                transactions.push(Transaction::ack(
                    // Percent in hundred-thousandths: 5 % is 50000.
                    Command::set("GASCONTROL") // dictionary
                        .param("GAS", gas)
                        .param(
                            "RATED_CONCENTRATION",
                            (target.percent() * 10_000.0).round() as i64,
                        )
                        .module_opt(state.modules.gas)
                        .build(),
                ));
            }
            if let Some(enabled) = control.enabled {
                // Both gases follow the chamber's control state; the instrument has one
                // enable per gas line, so this is sent for each configured gas.
                for gas in ["CO2", "O2"] {
                    transactions.push(Transaction::ack(
                        Command::set("GASCONTROL")
                            .param("GAS", gas)
                            .param("MODE", if enabled { "ON" } else { "OFF" }) // dictionary
                            .module_opt(state.modules.gas)
                            .build(),
                    ));
                }
            }
            Planned::Wire(transactions)
        }

        (Subject::ImagingHead, CapabilityRequest::ImagingHead(head)) => {
            let mut transactions = Vec::new();
            if let Some(objective) = head.objective {
                let Some(objective) = objective_at(objective) else {
                    return Planned::Unsupported(format!(
                        "objective position {objective} is not fitted: this head carries {}",
                        objective_tokens()
                    ));
                };
                transactions.push(Transaction::ack(
                    // `OBJECTIVE` and the type tokens are dictionary entries; that the changer
                    // takes the type as its value follows the `BEAM {0}={1}` shape, and is the
                    // part to confirm.
                    Command::set(format!("OBJECTIVE={}", objective.wire_token()))
                        .module_opt(state.modules.imaging)
                        .build(),
                ));
            }
            if let Some(mode) = &head.mode {
                let Some(mode) = imaging_mode(mode) else {
                    return Planned::Unsupported(format!(
                        "'{mode}' is not an imaging mode this instrument has; it reads \
                         CELL (brightfield cell imaging) or FIM (fluorescence imaging)"
                    ));
                };
                transactions.push(Transaction::ack(
                    Command::set("MODE") // dictionary
                        .param("MEASUREMENT", mode.wire_token())
                        .build(),
                ));
            }
            Planned::Wire(transactions)
        }

        (Subject::CameraBinding, CapabilityRequest::CameraBinding(_)) => Planned::Unsupported(
            "this driver does not own the imaging camera: the Spark's camera is a separate \
             USB device whose frames never cross the TDCL bus, and no open protocol evidence \
             for it exists here. Bind a camera device from a driver that can reach it, and \
             read the reader-side camera state from this device's properties"
                .into(),
        ),

        (Subject::Axes(axes), CapabilityRequest::StageMove(move_request)) => {
            if move_request.relative {
                // To confirm: `MOVE` is in the dictionary as the relative counterpart of
                // `ABSOLUTE`, but its parameter form is not recorded.
                return Planned::Unsupported(
                    "relative moves are not established for this instrument; command an \
                     absolute position instead"
                        .into(),
                );
            }
            let mut transactions = Vec::new();
            for (axis, position) in &move_request.target {
                let Some(motor) = motor_for(axis, axes) else {
                    return Planned::Unsupported(format!(
                        "this stage does not carry the {} axis",
                        axis.name()
                    ));
                };
                let raw = match state.axis_units.get(axis) {
                    Some(AxisUnit::Micrometres) => position.micrometers(),
                    Some(AxisUnit::Steps) => {
                        return Planned::Unsupported(format!(
                            "the {} axis reports its travel in motor steps, and how many \
                             steps make a micrometre is not recorded for this mechanism; a \
                             position in micrometres cannot be converted without inventing \
                             that number",
                            axis.name()
                        ))
                    }
                    None => {
                        return Planned::Unsupported(format!(
                            "the unit the {} axis counts in has not been read back yet; the \
                             instrument declares it in the range reply, which this driver \
                             asks for when a transport is attached",
                            axis.name()
                        ))
                    }
                };
                // The plate stage's X and Y are the main board's; focus is the imaging
                // module's `Z_OBJECTIVE`, which is a different parameter on a different
                // module rather than the same axis addressed twice.
                let (parameter, module) = match motor {
                    MtpMotor::Z => (Z_OBJECTIVE, state.modules.imaging),
                    _ => (motor.wire_token(), None),
                };
                transactions.push(Transaction::ack(
                    Command::set("ABSOLUTE") // dictionary
                        .param(parameter, raw.round() as i64)
                        .module_opt(module)
                        .build(),
                ));
            }
            Planned::Wire(transactions)
        }

        (Subject::Axes(axes), CapabilityRequest::None) => {
            // Homing: `INIT` with no axis parameter initialises the module's mechanism.
            let _ = axes;
            Planned::Wire(vec![Transaction::ack(
                Command::set("INIT") // dictionary
                    .module_opt(state.modules.imaging)
                    .build(),
            )])
        }

        (Subject::Carrier(carrier), CapabilityRequest::FilterSelect(select)) => {
            // Firmware may clamp an out-of-range position into one holding different glass
            // and report success, which turns a failed run into a wrong one. Refuse here —
            // but only when the slide has said how many positions it has, because refusing
            // against a guessed count would be its own kind of wrong.
            if select.position < 1 {
                return Planned::Unsupported(
                    "carrier positions are one-based; there is no position 0".into(),
                );
            }
            if let Some(slots) = state.carrier_slots.get(carrier.wire_token()) {
                if select.position > *slots {
                    return Planned::Unsupported(format!(
                        "the slide fitted to {} has {slots} positions; there is no position {}",
                        carrier.wire_token(),
                        select.position
                    ));
                }
            }
            Planned::Wire(vec![Transaction::ack(
                Command::set("MOVE") // dictionary
                    .param("CARRIER", carrier.wire_token())
                    .param("POSITION", select.position)
                    .module_opt(state.modules.imaging)
                    .build(),
            )])
        }

        (Subject::Injector, CapabilityRequest::Inject(inject)) => {
            let Some(pump) = injector_pump(inject.pump) else {
                return Planned::Unsupported(format!(
                    "this instrument carries two injector pumps, 1 and 2; there is no pump {}",
                    inject.pump
                ));
            };
            match inject.action {
                InjectAction::Dispense => {
                    let Some(volume) = inject.volume else {
                        return Planned::Unsupported(
                            "a dispense needs a volume; the instrument has no default and \
                             dispensing an unstated amount into a well is not recoverable"
                                .into(),
                        );
                    };
                    // The dictionary's template carries a `MODE=` alongside `VOLUME=`, but
                    // nothing records what its values are. Sending an invented one would be a
                    // guess dressed as a setting, so it is omitted until a bench records the
                    // vocabulary.
                    let mut setup = Command::set("INJECTOR") // dictionary
                        .param("PUMP", pump.wire_token())
                        .param("VOLUME", volume.microliters().round() as i64);
                    if let Some(speed) = inject.speed {
                        // The instrument counts dispense speed in microlitres per second.
                        setup = setup.param(
                            "SPEED",
                            (speed.milliliters_per_minute() * 1000.0 / 60.0).round() as i64,
                        );
                    }
                    Planned::Wire(vec![
                        Transaction::ack(setup.module_opt(state.modules.injector).build()),
                        Transaction::ack(
                            Command::set("INJECTOR") // dictionary
                                .word("DISPENSE")
                                .module_opt(state.modules.injector)
                                .build(),
                        ),
                    ])
                }
                action => {
                    if inject.volume.is_some() {
                        return Planned::Unsupported(format!(
                            "a {} takes no volume: the instrument decides how much it moves, \
                             and accepting a number it ignores would misreport what happened",
                            action.name()
                        ));
                    }
                    Planned::Wire(vec![Transaction::ack(
                        Command::set("INJECTOR") // dictionary
                            .word(injector_action(action))
                            .param("PUMP", pump.wire_token())
                            .module_opt(state.modules.injector)
                            .build(),
                    )])
                }
            }
        }

        (Subject::Camera, CapabilityRequest::CameraCapture(capture)) => {
            let camera = state.camera;
            let (Some(width), Some(height), Some(bits_per_pixel)) =
                (camera.width, camera.height, camera.bits_per_pixel)
            else {
                return Planned::Unsupported(
                    "the camera has not reported its area of interest and bit depth yet, so a \
                     payload could not be shaped into an image; this driver asks for both when \
                     a transport is attached"
                        .into(),
                );
            };
            // Exposure is a property, not part of the request, so it is sent from the state
            // the driver holds. It crosses in microseconds.
            let _ = capture;
            let mut transactions = Vec::new();
            if let Some(exposure_us) = camera.exposure_us {
                transactions.push(Transaction::ack(
                    Command::set("CAMERA") // dictionary
                        .param("EXPOSURETIME", exposure_us)
                        .build(),
                ));
            }
            let mut take = Command::set("CAMERA").word("TAKEIMAGE"); // dictionary
            if camera.cell_imaging {
                // The brightfield module rejects a bare TAKEIMAGE: "CELL needs TYPE=".
                take = take.param("TYPE", MeasurementMode::Cell.wire_token());
            }
            transactions.push(Transaction {
                line: take.module_opt(state.modules.imaging).build(),
                intent: Intent::Capture {
                    width,
                    height,
                    bits_per_pixel,
                },
            });
            Planned::Wire(transactions)
        }

        (Subject::Autofocus, CapabilityRequest::Autofocus(request)) => {
            match request.mode {
                numanager_core::AutofocusMode::SingleShot => {}
                mode => {
                    return Planned::Unsupported(format!(
                        "this instrument runs an autofocus sweep and reports what it found; \
                         {mode:?} focus is not something it offers"
                    ))
                }
            }
            Planned::Wire(vec![
                // The sweep is cleared, run, and then its peak read back.
                Transaction::ack(
                    Command::set("CAMERA") // dictionary
                        .word("AUTOFOCUS")
                        .word("CLEAR")
                        .module_opt(state.modules.imaging)
                        .build(),
                ),
                Transaction {
                    line: Command::query("CAMERA") // dictionary
                        .word("AUTOFOCUSDETAIL")
                        .param("IMAGE", 0)
                        .module_opt(state.modules.imaging)
                        .build(),
                    intent: Intent::Autofocus,
                },
            ])
        }

        (Subject::Shaker, CapabilityRequest::Shake(shake)) => {
            let mut transactions = Vec::new();
            if let Some(mode) = &shake.mode {
                transactions.push(Transaction::ack(
                    Command::set("MODE") // dictionary
                        .param("SHAKING", mode.trim().to_ascii_uppercase())
                        .build(),
                ));
            }
            if let Some(amplitude) = shake.amplitude {
                transactions.push(Transaction::ack(
                    // Amplitudes are declared in micrometres.
                    Command::set("SHAKING") // dictionary
                        .param("AMPLITUDE", amplitude.micrometers().round() as i64)
                        .build(),
                ));
            }
            if let Some(frequency) = shake.frequency {
                transactions.push(Transaction::ack(
                    // Frequencies are declared in tenths of a hertz (`hz10`).
                    Command::set("SHAKING") // dictionary
                        .param("FREQUENCY", (frequency.hertz() * 10.0).round() as i64)
                        .build(),
                ));
            }
            if let Some(duration) = shake.duration {
                transactions.push(Transaction::ack(
                    Command::set("SHAKING") // dictionary
                        .param("TIME", duration.seconds().round() as i64)
                        .build(),
                ));
            }
            transactions.push(Transaction::ack(
                Command::set("SHAKING").word("START").build(), // dictionary
            ));
            Planned::Wire(transactions)
        }

        (Subject::Barcode, CapabilityRequest::Barcode(read)) => {
            let position = match read.position.as_deref() {
                Some(name) => match barcode_position(name) {
                    Some(position) => Some(position),
                    None => {
                        return Planned::Unsupported(format!(
                            "'{name}' is not a barcode reading position; this reader serves \
                             LEFT and RIGHT"
                        ))
                    }
                },
                None => None,
            };
            let mut command = Command::set("BARCODE").word("READ");
            if let Some(position) = position {
                command = command.param("POSITION", position.wire_token());
            }
            Planned::Wire(vec![Transaction {
                // To confirm: `BARCODE` and `BarcodePosition` are dictionary entries; `READ`
                // as the subkeyword is inferred from the command grammar.
                line: command.module_opt(state.modules.barcode).build(),
                intent: Intent::Barcode,
            }])
        }

        (Subject::Barcode, _) => Planned::Wire(vec![Transaction {
            // To confirm: `BARCODE` is a dictionary keyword; `READ` as its subkeyword is
            // inferred from the command grammar.
            line: Command::set("BARCODE")
                .word("READ")
                .module_opt(state.modules.barcode)
                .build(),
            intent: Intent::Barcode,
        }]),

        _ => Planned::Local,
    }
}

/// The commands a property write becomes, when the instrument has one for it.
///
/// A write that only changes driver-side bookkeeping returns [`Planned::Local`] — the well a
/// measurement will address, or which camera is bound, are host concepts with no command
/// behind them.
///
/// Most writes are the same thing a capability request asks for, so this builds that request
/// and reuses [`plan_request`] rather than growing a second set of command builders that
/// could drift from the first.
pub fn plan_write(
    subject: &Subject,
    key: &str,
    value: &numanager_core::Value,
    state: &PlanState,
) -> Planned {
    use numanager_core::{
        FilterSelectRequest, GasControlRequest, ImagingHeadRequest, TemperatureControlRequest,
        Value,
    };

    let request = match (subject, key, value) {
        (Subject::Temperature, "target", Value::Temperature(target)) => Some(
            CapabilityRequest::TemperatureControl(TemperatureControlRequest {
                target: Some(*target),
                enabled: None,
            }),
        ),
        (Subject::Temperature, "enabled", Value::Bool(enabled)) => Some(
            CapabilityRequest::TemperatureControl(TemperatureControlRequest {
                target: None,
                enabled: Some(*enabled),
            }),
        ),
        (Subject::Gas, "co2_target", Value::GasConcentration(target)) => Some(
            CapabilityRequest::GasControl(GasControlRequest::co2(*target)),
        ),
        (Subject::Gas, "o2_target", Value::GasConcentration(target)) => Some(
            CapabilityRequest::GasControl(GasControlRequest::o2(*target)),
        ),
        (Subject::Gas, "enabled", Value::Bool(enabled)) => Some(CapabilityRequest::GasControl(
            GasControlRequest::enabled(*enabled),
        )),
        (Subject::ImagingHead, "objective", Value::I64(objective)) => {
            Some(CapabilityRequest::ImagingHead(ImagingHeadRequest {
                objective: Some(*objective),
                mode: None,
            }))
        }
        (Subject::ImagingHead, "mode", Value::String(mode)) => {
            Some(CapabilityRequest::ImagingHead(ImagingHeadRequest {
                objective: None,
                mode: Some(mode.clone()),
            }))
        }
        (Subject::Carrier(_), "position", Value::I64(position)) => {
            u8::try_from(*position).ok().map(|position| {
                CapabilityRequest::FilterSelect(FilterSelectRequest::position(position))
            })
        }
        // Exposure is sent as part of the acquisition it applies to, from the state the
        // driver holds, so writing it needs no command of its own.
        (Subject::Camera, "exposure", _) => return Planned::Local,
        (Subject::Camera, _, _) => return Planned::Local,
        _ => None,
    };

    match request {
        Some(request) => plan_request(&request, subject, state),
        // Everything else is driver-side bookkeeping: the well the next measurement will
        // address, a detector's tuned wavelength (which rides with that measurement), which
        // camera is bound. None of them has a command of its own.
        None => Planned::Local,
    }
}

/// The lid lifter's state, which is a write rather than a capability.
pub fn lid_command(state_token: &str, plate_height_um: Option<i64>) -> Transaction {
    let mut command = Command::set("LIDLIFT").param("STATE", state_token); // dictionary
    if let Some(height) = plate_height_um {
        command = command.param("PLATEHEIGHT", height);
    }
    Transaction::ack(command.build())
}

/// Read back the chamber, for the environmental telemetry a run records each cycle.
///
/// The chamber is `AMBIENTCONTROL`. `CUV` is the cuvette port and would report a different
/// body at a different temperature.
pub fn environment_reads(modules: &Modules) -> Vec<Transaction> {
    let mut reads = vec![Transaction {
        line: Command::query("SENSORVALUE") // dictionary
            .word("TEMPERATURE")
            .word(TemperatureDevice::AmbientControl.wire_token())
            .build(),
        intent: Intent::Read {
            key: "TEMPERATURE".into(),
        },
    }];
    for gas in ["CO2", "O2"] {
        reads.push(Transaction {
            line: Command::query("GASCONTROL") // dictionary
                .param("GAS", gas)
                .word("ACTUAL_CONCENTRATION")
                .module_opt(modules.gas)
                .build(),
            intent: Intent::Read {
                key: format!("ACTUAL_CONCENTRATION_{gas}"),
            },
        });
    }
    reads
}

/// Ask the camera what it is set to, so a payload can be shaped into an image.
pub fn camera_reads(modules: &Modules) -> Vec<Transaction> {
    vec![
        Transaction {
            line: Command::query("CAMERA") // dictionary
                .word("AOI")
                .module_opt(modules.imaging)
                .build(),
            intent: Intent::Read { key: "AOI".into() },
        },
        Transaction {
            line: Command::query("CAMERA") // dictionary
                .word("BITSPERPIXEL")
                .module_opt(modules.imaging)
                .build(),
            intent: Intent::Read {
                key: "BITSPERPIXEL".into(),
            },
        },
    ]
}

/// Pull raw pixels out of an acquisition's data frames.
///
/// The image arrives the same way a measurement package does — one `0x88` header frame then
/// `0x83` payload frames chunked at 65530 bytes — but the payload is the raster itself, rows
/// of `width * bits_per_pixel / 8` bytes, not typed scalars.
///
/// A payload that does not match the geometry the camera reported is returned as an error
/// rather than cropped or padded: a mis-shaped raster looks like a picture of something.
pub fn decode_image(
    outcome: &Outcome<DriverToken>,
    width: u32,
    height: u32,
    bits_per_pixel: u8,
) -> Result<Vec<u8>, String> {
    let mut pixels = Vec::new();
    for frame in &outcome.data {
        if frame.type_ == FrameType::Binary as u8 {
            pixels.extend_from_slice(&frame.payload);
        }
    }
    if pixels.is_empty() {
        return Err("the acquisition returned no pixel payload".into());
    }
    let stride = width as usize * bits_per_pixel as usize / 8;
    let expected = stride * height as usize;
    if pixels.len() != expected {
        return Err(format!(
            "the acquisition returned {} bytes, but the camera reported {width}x{height} at \
             {bits_per_pixel} bits ({expected} bytes)",
            pixels.len()
        ));
    }
    Ok(pixels)
}

/// The pixel format a bit depth denotes, in this repository's canonical spelling.
pub fn pixel_format(bits_per_pixel: u8) -> &'static str {
    match bits_per_pixel {
        8 => "Mono8",
        10 => "Mono10",
        12 => "Mono12",
        _ => "Mono16",
    }
}

/// Which command lists what a carrier is carrying.
///
/// The excitation and emission slides answer their own keywords; the mirror carrier answers
/// `MIRROR`. All three take `CARRIER=` and list what is fitted, `|`-separated.
fn inventory_keyword(carrier: MoveableCarrier) -> &'static str {
    match carrier {
        MoveableCarrier::ExcitationFilter => "EXCITATION",
        MoveableCarrier::Mirror | MoveableCarrier::DualPmtMirror => "MIRROR",
        _ => "EMISSION",
    }
}

/// Ask each carrier what slide is fitted to it.
///
/// This is what makes a position selectable safely: the reply says how many positions exist,
/// so an out-of-range one can be refused instead of clamped by firmware into different glass.
pub fn inventory_reads(carriers: &[MoveableCarrier]) -> Vec<Transaction> {
    carriers
        .iter()
        .map(|carrier| Transaction {
            line: Command::range(inventory_keyword(*carrier)) // dictionary
                .param("CARRIER", carrier.wire_token())
                .build(),
            intent: Intent::CarrierInventory { carrier: *carrier },
        })
        .collect()
}

/// How many positions a `|`-separated inventory reply describes.
///
/// An empty reply means the carrier did not say — reported as `None` rather than zero, since
/// a slide with no positions and a slide nobody asked about are different things.
pub fn parse_inventory_slots(text: &str) -> Option<u8> {
    let entries = text
        .split('|')
        .filter(|entry| !entry.trim().is_empty())
        .count();
    u8::try_from(entries).ok().filter(|count| *count > 0)
}

/// Ask each axis what its travel is, and in what unit.
///
/// This is what makes a position on this instrument mean something without a calibration
/// constant living in the driver: the reply's `[unit]` token is the instrument's own answer.
pub fn axis_range_reads(axes: &[MtpMotor], modules: &Modules) -> Vec<Transaction> {
    axes.iter()
        .map(|motor| Transaction {
            line: Command::range("ABSOLUTE") // dictionary
                .word(motor.wire_token())
                .word("POSITION")
                .module_opt(modules.imaging)
                .build(),
            intent: Intent::AxisRange {
                axis: stage_axis(*motor),
            },
        })
        .collect()
}

/// Read every axis position back in one command.
pub fn position_read(modules: &Modules) -> Transaction {
    Transaction {
        line: Command::query("ABSOLUTE") // dictionary
            .module_opt(modules.imaging)
            .build(),
        intent: Intent::Position,
    }
}

/// Turn a completed transaction into the value a capability completion carries.
pub fn completion(intent: &Intent, outcome: &Outcome<DriverToken>) -> Value {
    match intent {
        Intent::Acknowledge => {
            Value::Map(BTreeMap::from([("acknowledged".into(), Value::Bool(true))]))
        }

        Intent::PlateMove { position } => Value::Map(BTreeMap::from([(
            "position".into(),
            Value::String(position.wire_token().into()),
        )])),

        // The area-of-interest reply carries four keys at once; everything else is one value.
        Intent::Read { key } if key == "AOI" => Value::Map(
            parse_kv_map(&outcome.response.text)
                .into_iter()
                .map(|(key, text)| {
                    let value = match text.trim().parse::<i64>() {
                        Ok(raw) => Value::I64(raw),
                        Err(_) => Value::String(text),
                    };
                    (key, value)
                })
                .collect(),
        ),

        Intent::Read { key } => read_value(&outcome.response.text, key),

        Intent::Capture {
            width,
            height,
            bits_per_pixel,
        } => {
            let mut map = BTreeMap::from([
                ("width".into(), Value::PixelCount(PixelCount::new(*width))),
                ("height".into(), Value::PixelCount(PixelCount::new(*height))),
                (
                    "pixel_format".into(),
                    Value::String(pixel_format(*bits_per_pixel).into()),
                ),
            ]);
            // `EXECUTION_DETAIL=FRAMEDROP` is the instrument saying it lost the frame.
            if let Some(detail) = parse_kv_map(&outcome.response.text).get("EXECUTION_DETAIL") {
                map.insert("execution_detail".into(), Value::String(detail.clone()));
            }
            Value::Map(map)
        }

        Intent::Prepare { detector } => {
            let mut map = BTreeMap::from([("reference_read".into(), Value::Bool(true))]);
            if let Some(counts) = decode_counts(*detector, outcome) {
                map.insert("reference".into(), Value::I64(counts.reference as i64));
                map.insert("measurement".into(), Value::I64(counts.measurement as i64));
            }
            Value::Map(map)
        }

        Intent::Identity { key } => {
            let parsed = parse_kv_map(&outcome.response.text);
            match parsed.get(key) {
                Some(text) => Value::String(text.clone()),
                // A bare reply with no key is still the answer to the only thing asked.
                None if !outcome.response.text.trim().is_empty() => {
                    Value::String(outcome.response.text.trim().into())
                }
                None => Value::Null,
            }
        }

        Intent::ModuleMap { .. } => Value::Map(
            parse_module_map(&outcome.response.text)
                .into_iter()
                .map(|(name, number)| (name, Value::I64(number as i64)))
                .collect(),
        ),

        Intent::CarrierInventory { carrier } => {
            let text = outcome.response.text.trim();
            let mut map =
                BTreeMap::from([("carrier".into(), Value::String(carrier.wire_token().into()))]);
            match parse_inventory_slots(text) {
                Some(slots) => {
                    map.insert("slots".into(), Value::I64(slots as i64));
                    map.insert("fitted".into(), Value::String(text.into()));
                }
                // Nothing fitted, or nothing said. Either way there is no count to act on.
                None => {
                    map.insert("slots".into(), Value::Null);
                }
            }
            Value::Map(map)
        }

        Intent::Autofocus => {
            let parsed = parse_kv_map(&outcome.response.text);
            let mut map = BTreeMap::new();
            for key in ["MAXVALUE", "STDDEV"] {
                if let Some(text) = parsed.get(key) {
                    let value = match text.trim().parse::<f64>() {
                        Ok(raw) => Value::F64(raw),
                        Err(_) => Value::String(text.clone()),
                    };
                    map.insert(key.to_ascii_lowercase(), value);
                }
            }
            Value::Map(map)
        }

        Intent::Barcode => {
            let map = parse_kv_map(&outcome.response.text);
            match map.get("BARCODE").or_else(|| map.get("CODE")) {
                Some(text) => Value::String(text.clone()),
                // An empty reply is a plate with no label, which is a fact, not a failure.
                None => Value::Null,
            }
        }

        Intent::AxisRange { axis } => {
            let mut map = BTreeMap::from([("axis".into(), Value::String(axis.name().into()))]);
            if let Some((range, unit)) = axis_range(&outcome.response.text) {
                map.insert("from".into(), Value::F64(range.0));
                map.insert("to".into(), Value::F64(range.1));
                map.insert("unit".into(), Value::String(unit.into()));
            }
            Value::Map(map)
        }

        Intent::Position => {
            let parsed = parse_kv_map(&outcome.response.text);
            let mut map = BTreeMap::new();
            for motor in [MtpMotor::X, MtpMotor::Y, MtpMotor::Z] {
                if let Some(raw) = parsed
                    .get(motor.wire_token())
                    .and_then(|text| text.trim().parse::<f64>().ok())
                {
                    map.insert(motor.wire_token().to_ascii_lowercase(), Value::F64(raw));
                }
            }
            Value::Map(map)
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

/// A `KEY=VALUE` reply, read as a number where it is one.
///
/// The reply key for a sensor read is not recorded in the dictionary, only that replies are
/// `KEY=VALUE`. When the expected key is absent the first numeric value is taken instead,
/// which is right for the single-value replies these reads produce and reports `Null` rather
/// than a wrong number when a reply carries something else entirely.
fn read_value(text: &str, key: &str) -> Value {
    let map = parse_kv_map(text);
    // Both gases answer under `ACTUAL_CONCENTRATION`; the intent key carries which gas asked
    // so the reply can be routed, and that suffix is not part of what the instrument sends.
    let named = key
        .strip_suffix("_CO2")
        .or_else(|| key.strip_suffix("_O2"))
        .unwrap_or(key);
    if let Some(text) = map.get(key).or_else(|| map.get(named)) {
        return match text.trim().parse::<i64>() {
            Ok(raw) => Value::I64(raw),
            Err(_) => Value::String(text.clone()),
        };
    }
    map.values()
        .find_map(|text| text.trim().parse::<i64>().ok())
        .map(Value::I64)
        .unwrap_or(Value::Null)
}

/// `{from}~{to}%{step} [unit]` — the range reply that declares an axis's travel and unit.
fn axis_range(text: &str) -> Option<((f64, f64), &'static str)> {
    let range = parse_kv_map(text)
        .into_values()
        .find_map(|value| super::parse::parse_range(&value))?;
    let unit = match range.unit.as_deref() {
        Some("um") | Some("µm") => "um",
        Some("step") => "step",
        _ => return None,
    };
    Some(((range.from, range.to), unit))
}

/// The unit an axis range reply declared, if it is one this driver can act on.
pub fn axis_unit(intent: &Intent, outcome: &Outcome<DriverToken>) -> Option<(StageAxis, AxisUnit)> {
    let Intent::AxisRange { axis } = intent else {
        return None;
    };
    let (_, unit) = axis_range(&outcome.response.text)?;
    Some((
        axis.clone(),
        match unit {
            "um" => AxisUnit::Micrometres,
            _ => AxisUnit::Steps,
        },
    ))
}

fn stage_axis(motor: MtpMotor) -> StageAxis {
    match motor {
        MtpMotor::X => StageAxis::X,
        MtpMotor::Y => StageAxis::Y,
        MtpMotor::Z => StageAxis::Z,
    }
}

fn motor_for(axis: &StageAxis, axes: &[MtpMotor]) -> Option<MtpMotor> {
    axes.iter()
        .copied()
        .find(|motor| &stage_axis(*motor) == axis)
}

/// A carrier station name, however a caller spelled it.
fn plate_position(name: &str) -> Option<PlatePosition> {
    let token = name.trim().to_ascii_uppercase().replace([' ', '-'], "_");
    if let Some(position) = PlatePosition::from_wire_token(&token) {
        return Some(position);
    }
    match token.as_str() {
        "IN" => Some(PlatePosition::PlateIn),
        "OUT" => Some(PlatePosition::OutLeft),
        _ => None,
    }
}

fn plate_position_tokens() -> String {
    [
        PlatePosition::PlateIn,
        PlatePosition::OutLeft,
        PlatePosition::OutRight,
        PlatePosition::PickNPlace,
        PlatePosition::LidLifter,
        PlatePosition::Check,
        PlatePosition::Heating,
        PlatePosition::Incubation,
        PlatePosition::Cooling,
        PlatePosition::BarcodeLeft,
        PlatePosition::BarcodeRight,
    ]
    .iter()
    .map(|position| position.wire_token())
    .collect::<Vec<_>>()
    .join(", ")
}

/// Objective positions are one-based, in the order the turret carries them.
fn objective_at(position: i64) -> Option<ObjectiveType> {
    match position {
        1 => Some(ObjectiveType::TwoTimes),
        2 => Some(ObjectiveType::FourTimes),
        3 => Some(ObjectiveType::TenTimes),
        _ => None,
    }
}

fn objective_tokens() -> String {
    "1 = 2X, 2 = 4X, 3 = 10X".into()
}

/// Pumps are one-based in the order the instrument carries them: 1 is the A line.
fn injector_pump(pump: u8) -> Option<InjectorPump> {
    match pump {
        1 => Some(InjectorPump::A),
        2 => Some(InjectorPump::B),
        _ => None,
    }
}

fn injector_action(action: InjectAction) -> &'static str {
    match action {
        InjectAction::Prime => "PRIME",
        InjectAction::Refill => "REFILL",
        InjectAction::Rinse => "RINSE",
        InjectAction::Backflush => "BACKFLUSH",
        InjectAction::Dispense => "DISPENSE",
    }
}

fn barcode_position(name: &str) -> Option<BarcodePosition> {
    BarcodePosition::from_wire_token(&name.trim().to_ascii_uppercase())
}

fn imaging_mode(mode: &str) -> Option<MeasurementMode> {
    match mode.trim().to_ascii_uppercase().as_str() {
        "CELL" | "BRIGHTFIELD" | "CELLIMAGING" => Some(MeasurementMode::Cell),
        "FIM" | "FLUORESCENCE" | "FLUORESCENCEIMAGING" => {
            Some(MeasurementMode::FluorescenceImaging)
        }
        _ => None,
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

/// A gas concentration from a `GASCONTROL` reply, which is in hundred-thousandths of a
/// percent.
pub fn gas_from_scaled(raw: i64) -> GasConcentration {
    GasConcentration::from_percent(raw as f64 / 10_000.0)
}

/// An axis position from a readback, in the unit that axis declared.
pub fn position_from_raw(raw: f64, unit: AxisUnit) -> Option<Position> {
    match unit {
        AxisUnit::Micrometres => Some(Position::from_micrometers(raw)),
        // A step count is not a length until the mechanism says how long a step is.
        AxisUnit::Steps => None,
    }
}
