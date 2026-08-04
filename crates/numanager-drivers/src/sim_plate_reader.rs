//! Composed multiwell plate reader/imager simulation.
//!
//! One hub offers a plate carrier, three detectors, an incubator, a gas controller, a
//! camera, XY/Z stages, an objective turret and a lamp. Every one of them observes the
//! same [`Specimen`](crate::sim_sample::Specimen), so an absorbance read and an image of
//! the same well agree — which is the property that makes a simulator worth developing
//! against rather than merely worth compiling.
//!
//! Two things it deliberately does **not** do:
//!
//! * **It simulates no wire protocol.** Commands arrive as typed capability requests. A
//!   reference-firmware simulator that framed bytes would test the framing, not the
//!   application.
//! * **It computes no OD, RFU or RLU.** [`CapabilityKind::Measure`] answers in raw counts
//!   plus the settings they were taken under. What a count *means* needs assay context the
//!   instrument does not have, and keeping the counts is what lets a run be recomputed when
//!   the photometry is corrected.
//!
//! The biology lives outside this driver: supply it with
//! [`SimPlateReaderConfig::with_specimen`]. What is here is the instrument.
//!
//! Started as a copy of [`crate::sim_microscope`], which carries the stream, motion and
//! completion machinery this shares.

use numanager_core::*;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::sim_sample::{self, SimSampleConfig, Specimen};

#[doc(hidden)]
pub mod shared_sample {
    pub use crate::sim_lsm_model::{
        fluorescence_at, render_confocal_raster, render_line_profile, LsmFluorescenceConfig,
        LsmImage, LsmRasterSpec,
    };
    pub use crate::sim_sample::{sample_plane_um, SimCell, SimSampleConfig};
}

/// Graph role connecting the objective turret to the camera it feeds. `Role` has
/// no objective variant, so the string is part of this driver's published
/// contract and is documented on the device page.
pub const OBJECTIVE_ROLE: &str = "objective";

const RESOURCE_OFFSET: u64 = 870;
const HUB_OFFSET: u64 = 871;
const CAMERA_OFFSET: u64 = 872;
const XY_OFFSET: u64 = 873;
const Z_OFFSET: u64 = 874;
const TURRET_OFFSET: u64 = 875;
const LAMP_OFFSET: u64 = 876;
const TRANSPORT_OFFSET: u64 = 877;
const ABSORBANCE_OFFSET: u64 = 878;
const FLUORESCENCE_OFFSET: u64 = 879;
const LUMINESCENCE_OFFSET: u64 = 880;
const TEMPERATURE_OFFSET: u64 = 881;
const GAS_OFFSET: u64 = 882;

/// Widest blur this camera will ask the specimen to draw, in image pixels. Beyond this a
/// feature is spread so thin that further growth only costs time. It is the camera's
/// policy rather than the specimen's, so it travels in the field request.
const BLUR_PIXEL_LIMIT: f64 = 48.0;
const VIGNETTE_K: f64 = 0.35;
const STRAY_LIGHT: f64 = 0.002;
/// Exposure at which the lamp fills the well to `BACKGROUND_FILL`.
const EXPOSURE_REFERENCE_S: f64 = 0.02;
const BACKGROUND_FILL: f64 = 0.78;
const SETTLE: Duration = Duration::from_millis(20);

/// One objective in the turret.
/// Where the wells are on a piece of plastic.
///
/// The application owns plate *types* — a plate-type library is an application concern, and
/// which plastic is on the deck changes per run. What the instrument owns is turning a well
/// label into a position its own stage can reach, which is this. Defaults describe a
/// standard SBS 96-well plate.
#[derive(Debug, Clone, Copy)]
pub struct PlateGeometry {
    pub rows: u32,
    pub columns: u32,
    /// Centre-to-centre spacing. Square on every SBS plate, so one number.
    pub well_pitch_um: f64,
    /// Centre of well A1, from the plate's top-left corner.
    pub a1_x_um: f64,
    pub a1_y_um: f64,
    pub well_diameter_um: f64,
    /// Depth of the well bottom below the carrier reference. Where focus starts looking.
    pub well_bottom_um: f64,
}

impl Default for PlateGeometry {
    fn default() -> Self {
        // ANSI/SLAS 1-2004 through 4-2004 for a 96-well plate.
        Self {
            rows: 8,
            columns: 12,
            well_pitch_um: 9_000.0,
            a1_x_um: 14_380.0,
            a1_y_um: 11_240.0,
            well_diameter_um: 6_960.0,
            well_bottom_um: 4_250.0,
        }
    }
}

impl PlateGeometry {
    /// Centre of a well in plate coordinates, or `None` if it is off the plate.
    pub fn well_center_um(&self, row: u32, column: u32) -> Option<(f64, f64)> {
        if row >= self.rows || column >= self.columns {
            return None;
        }
        Some((
            self.a1_x_um + column as f64 * self.well_pitch_um,
            self.a1_y_um + row as f64 * self.well_pitch_um,
        ))
    }

    /// Parse a well label — `A1`, `A01`, `h12`, all accepted — into zero-based
    /// `(row, column)`.
    ///
    /// Rejects anything outside the plate rather than clamping: a protocol that asks for
    /// H13 on a 96-well plate has a real mistake in it, and reading G12 instead would
    /// attribute one well's numbers to another.
    pub fn parse_well(&self, label: &str) -> Option<(u32, u32)> {
        let label = label.trim();
        let mut chars = label.chars();
        let row_char = chars.next()?.to_ascii_uppercase();
        if !row_char.is_ascii_alphabetic() {
            return None;
        }
        let row = row_char as u32 - 'A' as u32;
        let column_text = chars.as_str();
        if column_text.is_empty() || !column_text.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let column_1 = column_text.parse::<u32>().ok()?;
        if column_1 == 0 {
            return None;
        }
        let column = column_1 - 1;
        if row >= self.rows || column >= self.columns {
            return None;
        }
        Some((row, column))
    }

    /// Canonical label for a well, zero-padded to the plate's widest column number so
    /// labels sort lexicographically the way a person expects.
    pub fn well_label(&self, row: u32, column: u32) -> String {
        let width = self.columns.to_string().len();
        format!(
            "{}{:0width$}",
            (b'A' + row as u8) as char,
            column + 1,
            width = width
        )
    }

    /// How far the carrier must travel in X to reach past the last column, plus the margin
    /// a field of view needs around it.
    pub fn x_extent_um(&self) -> f64 {
        self.a1_x_um
            + self.columns.saturating_sub(1) as f64 * self.well_pitch_um
            + self.well_diameter_um
    }

    /// The same in Y.
    pub fn y_extent_um(&self) -> f64 {
        self.a1_y_um
            + self.rows.saturating_sub(1) as f64 * self.well_pitch_um
            + self.well_diameter_um
    }

    /// Every well label, row-major — the order a plate reader sweeps in.
    pub fn well_labels(&self) -> Vec<String> {
        (0..self.rows)
            .flat_map(|row| (0..self.columns).map(move |column| (row, column)))
            .map(|(row, column)| self.well_label(row, column))
            .collect()
    }

    /// How far a point sits from the centre of the well containing it, as a fraction of
    /// the well radius. Above 1.0 the point is on plastic rather than over liquid, which
    /// is what puts a rim in an image and what makes an off-centre read meaningless.
    pub fn radial_fraction(&self, row: u32, column: u32, x_um: f64, y_um: f64) -> f64 {
        let Some((cx, cy)) = self.well_center_um(row, column) else {
            return f64::INFINITY;
        };
        let dx = x_um - cx;
        let dy = y_um - cy;
        (dx * dx + dy * dy).sqrt() / (self.well_diameter_um / 2.0)
    }
}

#[derive(Debug, Clone)]
pub struct SimObjective {
    pub magnification: f64,
    pub numerical_aperture: f64,
    pub label: String,
}

impl SimObjective {
    pub fn new(magnification: f64, numerical_aperture: f64) -> Self {
        Self {
            label: format!("{magnification}x / {numerical_aperture} NA air"),
            magnification,
            numerical_aperture,
        }
    }
}

#[derive(Clone)]
pub struct SimPlateReaderConfig {
    /// Seed for the cell-culture model. The same seed always yields the same
    /// sample, so recorded output and screenshots are reproducible.
    pub seed: u64,
    pub sensor_width: u32,
    pub sensor_height: u32,
    /// Physical size of one sensor pixel.
    pub pixel_pitch: Position,
    pub full_well_electrons: f64,
    pub read_noise_electrons: f64,
    pub objectives: Vec<SimObjective>,
    pub xy_travel: Position,
    pub z_travel: Position,
    /// Height of the culture surface at the origin.
    pub focal_plane: Position,
    /// Culture surface tilt, in micrometres of height per millimetre of travel.
    pub sample_tilt_um_per_mm: (f64, f64),
    pub cells_per_tile: (u32, u32),
    pub xy_speed: Velocity,
    pub z_speed: Velocity,
    pub objective_switch: TimeInterval,
    pub exposure: TimeInterval,
    pub frame_interval: TimeInterval,
    pub illumination_wavelength: Wavelength,
    /// Where the wells are.
    pub plate: PlateGeometry,
    /// Absorbance detector's initial wavelength.
    pub absorbance_wavelength: Wavelength,
    /// Fluorescence excitation band.
    pub fluorescence_wavelength: Wavelength,
    /// Counts a detector reports with nothing in the light path — the reference read that
    /// absorbance is computed against, and the ceiling every other count sits under.
    pub detector_full_scale_counts: f64,
    /// Counts a detector reports in the dark. Real detectors do not read zero, and code
    /// that assumes they do produces an absorbance of infinity on a blocked path.
    pub detector_dark_counts: f64,
    /// Incubator setpoints at power-on.
    pub temperature_target: Temperature,
    pub co2_target: GasConcentration,
    /// What the camera is pointed at. Defaults to the built-in cell culture described by
    /// the `seed`/`focal_plane`/`sample_tilt_um_per_mm`/`cells_per_tile` fields above; set
    /// it with [`SimPlateReaderConfig::with_specimen`] to observe something else.
    pub specimen: Arc<dyn Specimen>,
}

impl std::fmt::Debug for SimPlateReaderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimPlateReaderConfig")
            .field("seed", &self.seed)
            .field("sensor_width", &self.sensor_width)
            .field("sensor_height", &self.sensor_height)
            .field("pixel_pitch", &self.pixel_pitch)
            .field("objectives", &self.objectives)
            .field("focal_plane", &self.focal_plane)
            .field("specimen", &"<dyn Specimen>")
            .finish_non_exhaustive()
    }
}

impl Default for SimPlateReaderConfig {
    fn default() -> Self {
        Self {
            seed: 0x5EED_0C11_A73E_0001,
            sensor_width: 512,
            sensor_height: 512,
            pixel_pitch: Position::from_micrometers(6.5),
            full_well_electrons: 12_000.0,
            read_noise_electrons: 2.4,
            objectives: vec![
                SimObjective::new(4.0, 0.13),
                SimObjective::new(20.0, 0.45),
                SimObjective::new(60.0, 0.9),
            ],
            xy_travel: Position::from_micrometers(50_000.0),
            z_travel: Position::from_micrometers(10_000.0),
            focal_plane: Position::from_micrometers(4_250.0),
            sample_tilt_um_per_mm: (2.0, -1.4),
            cells_per_tile: (4, 9),
            xy_speed: Velocity::from_micrometers_per_second(2_500.0),
            z_speed: Velocity::from_micrometers_per_second(400.0),
            objective_switch: TimeInterval::from_milliseconds(700.0),
            exposure: TimeInterval::from_milliseconds(20.0),
            frame_interval: TimeInterval::from_milliseconds(50.0),
            illumination_wavelength: Wavelength::from_nanometers(550.0),
            plate: PlateGeometry::default(),
            absorbance_wavelength: Wavelength::from_nanometers(600.0),
            fluorescence_wavelength: Wavelength::from_nanometers(485.0),
            detector_full_scale_counts: 52_000.0,
            detector_dark_counts: 300.0,
            temperature_target: Temperature::from_celsius(37.0),
            co2_target: GasConcentration::from_percent(5.0),
            specimen: Arc::new(SimSampleConfig {
                seed: 0x5EED_0C11_A73E_0001,
                focal_plane_um: 4_250.0,
                tilt_um_per_mm: (2.0, -1.4),
                cells_per_tile: (4, 9),
            }),
        }
    }
}

impl SimPlateReaderConfig {
    pub(crate) fn sample_config(&self) -> SimSampleConfig {
        SimSampleConfig {
            seed: self.seed,
            focal_plane_um: self.focal_plane.micrometers(),
            tilt_um_per_mm: self.sample_tilt_um_per_mm,
            cells_per_tile: self.cells_per_tile,
        }
    }

    /// Observe something other than the built-in cell culture.
    ///
    /// The sample fields above still describe the *default* specimen; once this is set they
    /// no longer describe what the camera sees, and `focal_plane`/`sample_tilt_um_per_mm`
    /// only still matter through whatever [`Specimen::surface_plane_um`] the replacement
    /// reports.
    pub fn with_specimen(mut self, specimen: Arc<dyn Specimen>) -> Self {
        self.specimen = specimen;
        self
    }
}

/// Everything an acquisition thread needs, captured in one atomic read so a
/// frame is never rendered from two different instants.
#[derive(Debug, Clone, Copy)]
struct SceneState {
    x_um: f64,
    y_um: f64,
    z_um: f64,
    objective: usize,
    light_path_open: bool,
    lamp_on: bool,
    lamp_power_percent: f64,
    exposure_s: f64,
    gain_percent: f64,
    binning: u32,
    /// When observation started. Carried in the scene rather than read at publish time so
    /// specimen time keeps advancing during a stream, which only republishes the scene when
    /// a control changes.
    epoch: Instant,
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct SimPlateReaderSceneSnapshot {
    pub x_um: f64,
    pub y_um: f64,
    pub z_um: f64,
    pub sample_pixel_size_um: f64,
    pub magnification: f64,
    pub numerical_aperture: f64,
    pub laser_gate_enabled: bool,
    pub lamp_power_fraction: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SimDevice {
    Hub,
    Camera,
    Xy,
    Z,
    Turret,
    Lamp,
    Transport,
    Absorbance,
    Fluorescence,
    Luminescence,
    Temperature,
    Gas,
}

/// Which detector a `Measure` is asking for. They differ in what physics answers them,
/// not in how they are driven, so one code path serves all three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Detector {
    /// Transmitted light through the well: the specimen's optical depth attenuates it.
    Absorbance,
    /// Excitation in, emission out — the specimen's emission field.
    Fluorescence,
    /// Emission with no excitation at all.
    Luminescence,
}

/// A stage move in flight. Progress is interpolated in `poll()`; the lane thread
/// is shared with every other command, so no dispatch path ever sleeps.
struct PendingMotion {
    token: DriverToken,
    device: DeviceId,
    started: Instant,
    duration: Duration,
    axes: Vec<(StageAxis, f64, f64)>,
    homing: bool,
}

/// A turret rotation in flight. The light path stays blocked until it lands.
struct PendingTurret {
    token: Option<DriverToken>,
    started: Instant,
    duration: Duration,
    target: usize,
}

pub struct SimPlateReaderDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    camera: DeviceId,
    xy: DeviceId,
    z: DeviceId,
    turret: DeviceId,
    lamp: DeviceId,
    transport: DeviceId,
    absorbance: DeviceId,
    fluorescence: DeviceId,
    luminescence: DeviceId,
    temperature: DeviceId,
    gas: DeviceId,
    /// Which well the carrier has under the optics.
    well: String,
    absorbance_wavelength_nm: f64,
    fluorescence_wavelength_nm: f64,
    fluorescence_enabled: bool,
    luminescence_enabled: bool,
    temperature_target_c: f64,
    temperature_enabled: bool,
    co2_target_percent: f64,
    gas_enabled: bool,
    config: Arc<SimPlateReaderConfig>,
    scene: Arc<Mutex<SceneState>>,
    frames: Arc<AtomicU64>,
    epoch: Instant,
    x_um: f64,
    y_um: f64,
    z_um: f64,
    objective: usize,
    requested_objective: usize,
    lamp_on: bool,
    lamp_power_percent: f64,
    exposure_s: f64,
    gain_percent: f64,
    frame_interval_s: f64,
    binning: u32,
    xy_speed_um_s: f64,
    z_speed_um_s: f64,
    motions: Vec<PendingMotion>,
    turret_move: Option<PendingTurret>,
    next_token: u64,
    events: VecDeque<DriverEvent>,
    worker_tx: Sender<DriverEvent>,
    worker_rx: Receiver<DriverEvent>,
    streams: HashMap<DriverToken, Arc<AtomicBool>>,
}

impl SimPlateReaderDriver {
    pub fn new(id: DriverId, config: SimPlateReaderConfig) -> Self {
        let node = |offset: u64| NodeId(id.0 * 1000 + offset);
        let (worker_tx, worker_rx) = mpsc::channel();
        let objective = config.objectives.len().min(2).saturating_sub(1);
        let (start_x, start_y) = config.plate.well_center_um(0, 0).unwrap_or((0.0, 0.0));
        let scene = SceneState {
            x_um: start_x,
            y_um: start_y,
            z_um: config.focal_plane.micrometers(),
            objective,
            light_path_open: true,
            lamp_on: true,
            lamp_power_percent: 100.0,
            exposure_s: config.exposure.seconds(),
            gain_percent: 100.0,
            binning: 1,
            epoch: Instant::now(),
        };
        let epoch = scene.epoch;
        Self {
            id,
            epoch,
            resource: ResourceId(node(RESOURCE_OFFSET)),
            hub: DeviceId(node(HUB_OFFSET)),
            camera: DeviceId(node(CAMERA_OFFSET)),
            xy: DeviceId(node(XY_OFFSET)),
            z: DeviceId(node(Z_OFFSET)),
            turret: DeviceId(node(TURRET_OFFSET)),
            lamp: DeviceId(node(LAMP_OFFSET)),
            transport: DeviceId(node(TRANSPORT_OFFSET)),
            absorbance: DeviceId(node(ABSORBANCE_OFFSET)),
            fluorescence: DeviceId(node(FLUORESCENCE_OFFSET)),
            luminescence: DeviceId(node(LUMINESCENCE_OFFSET)),
            temperature: DeviceId(node(TEMPERATURE_OFFSET)),
            gas: DeviceId(node(GAS_OFFSET)),
            well: config.plate.well_label(0, 0),
            absorbance_wavelength_nm: config.absorbance_wavelength.nanometers(),
            fluorescence_wavelength_nm: config.fluorescence_wavelength.nanometers(),
            fluorescence_enabled: false,
            luminescence_enabled: false,
            temperature_target_c: config.temperature_target.celsius(),
            temperature_enabled: true,
            co2_target_percent: config.co2_target.percent(),
            gas_enabled: true,
            x_um: scene.x_um,
            y_um: scene.y_um,
            z_um: scene.z_um,
            objective,
            requested_objective: objective,
            lamp_on: scene.lamp_on,
            lamp_power_percent: scene.lamp_power_percent,
            exposure_s: scene.exposure_s,
            gain_percent: scene.gain_percent,
            frame_interval_s: config.frame_interval.seconds(),
            binning: 1,
            xy_speed_um_s: config.xy_speed.micrometers_per_second(),
            z_speed_um_s: config.z_speed.micrometers_per_second(),
            scene: Arc::new(Mutex::new(scene)),
            frames: Arc::new(AtomicU64::new(0)),
            config: Arc::new(config),
            motions: Vec::new(),
            turret_move: None,
            next_token: 1,
            events: VecDeque::new(),
            worker_tx,
            worker_rx,
            streams: HashMap::new(),
        }
    }

    pub fn simulated(id: DriverId) -> Self {
        Self::new(id, SimPlateReaderConfig::default())
    }

    fn classify(&self, device: DeviceId) -> Option<SimDevice> {
        if device == self.hub {
            Some(SimDevice::Hub)
        } else if device == self.camera {
            Some(SimDevice::Camera)
        } else if device == self.xy {
            Some(SimDevice::Xy)
        } else if device == self.z {
            Some(SimDevice::Z)
        } else if device == self.turret {
            Some(SimDevice::Turret)
        } else if device == self.lamp {
            Some(SimDevice::Lamp)
        } else if device == self.transport {
            Some(SimDevice::Transport)
        } else if device == self.absorbance {
            Some(SimDevice::Absorbance)
        } else if device == self.fluorescence {
            Some(SimDevice::Fluorescence)
        } else if device == self.luminescence {
            Some(SimDevice::Luminescence)
        } else if device == self.temperature {
            Some(SimDevice::Temperature)
        } else if device == self.gas {
            Some(SimDevice::Gas)
        } else {
            None
        }
    }

    /// Drive the carrier so `label` sits under the optics.
    ///
    /// A plate reader addresses wells, not coordinates — but underneath it is still a
    /// stage, so this sets XY to the well centre and Z to the well bottom. That is what
    /// lets one simulator serve both the `PlateMove` path a reader uses and the
    /// `StageMove` path a microscope with no carrier uses.
    fn move_to_well(&mut self, label: &str) -> Result<()> {
        let Some((row, column)) = self.config.plate.parse_well(label) else {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!(
                    "well {label:?} is not on a {}x{} plate",
                    self.config.plate.rows, self.config.plate.columns
                ),
            ));
        };
        let (x_um, y_um) = self
            .config
            .plate
            .well_center_um(row, column)
            .expect("parse_well already bounds-checked");
        self.well = self.config.plate.well_label(row, column);
        self.x_um = x_um;
        self.y_um = y_um;
        self.supersede(self.xy, StageAxis::X);
        self.supersede(self.xy, StageAxis::Y);
        self.publish_scene();
        Ok(())
    }

    /// Chamber temperature. Control pulls it toward the setpoint; with control off it
    /// decays toward ambient, which is what makes leaving the incubator disabled visible
    /// in the data rather than silent.
    fn actual_temperature_c(&self) -> f64 {
        const AMBIENT_C: f64 = 21.5;
        if !self.temperature_enabled {
            return AMBIENT_C;
        }
        // A tenth of a degree of ripple, deterministic in the setpoint so repeated reads
        // of an unchanged instrument agree.
        let ripple = sim_sample::unit01(sim_sample::mix64(
            (self.temperature_target_c * 100.0) as i64 as u64,
        )) - 0.5;
        self.temperature_target_c + ripple * 0.2
    }

    fn actual_co2_percent(&self) -> f64 {
        if !self.gas_enabled {
            return 0.04; // room air
        }
        let ripple = sim_sample::unit01(sim_sample::mix64(
            (self.co2_target_percent * 1000.0) as i64 as u64 ^ 0x9E37,
        )) - 0.5;
        (self.co2_target_percent + ripple * 0.1).max(0.0)
    }

    /// Read the current well with one detector, in raw counts.
    ///
    /// Counts, not OD or RFU — turning them into a physical quantity needs assay context
    /// this instrument does not have, and keeping the counts is what lets a run be
    /// recomputed when the photometry is corrected. See NUMANAGER_PLAN.md §4.1.
    ///
    /// The light path is the whole well, not a camera field: the detector integrates over
    /// the well aperture, so this asks the specimen for one coarse pixel covering the well
    /// rather than rendering an image and averaging it. That is the level-of-detail
    /// property the radiometric interface exists for.
    fn measure(&self, detector: Detector, integration_s: f64) -> Result<MeasuredCounts> {
        let plate = &self.config.plate;
        let Some((row, column)) = plate.parse_well(&self.well) else {
            return Err(Error::new(
                ErrorCode::Driver,
                format!("carrier is at {:?}, which is not a well", self.well),
            ));
        };
        let (cx, cy) = plate
            .well_center_um(row, column)
            .expect("parse_well already bounds-checked");

        // One pixel, one well aperture wide. The specimen decides how to answer a footprint
        // this coarse; a cell-based one enumerates, a plate model answers analytically.
        let aperture_um = plate.well_diameter_um;
        let wavelength_nm = match detector {
            Detector::Absorbance => self.absorbance_wavelength_nm,
            Detector::Fluorescence => self.fluorescence_wavelength_nm,
            // Luminescence has no excitation; the band is whatever the reaction emits, and
            // a broad mid-visible value stands in for it.
            Detector::Luminescence => 560.0,
        };
        let field = self
            .config
            .specimen
            .render_field(&sim_sample::FieldRequest {
                origin_x_um: cx - aperture_um / 2.0,
                origin_y_um: cy - aperture_um / 2.0,
                pixel_um: aperture_um,
                width: 1,
                height: 1,
                focus_z_um: plate.well_bottom_um,
                numerical_aperture: 0.25,
                wavelength_um: wavelength_nm / 1_000.0,
                // Fluorescence needs a lamp; luminescence emits in the dark and absorbance
                // reads transmitted light. Passing the excitation band is what keeps the
                // specimen from reporting fluorescence to a luminescence read.
                excitation_um: match detector {
                    Detector::Fluorescence => Some(self.fluorescence_wavelength_nm / 1_000.0),
                    Detector::Absorbance | Detector::Luminescence => None,
                },
                blur_limit_px: 1.0,
                time_s: self.epoch.elapsed().as_secs_f64(),
            });

        let full_scale = self.config.detector_full_scale_counts;
        let dark = self.config.detector_dark_counts;
        let integration = (integration_s / 0.02).clamp(0.01, 50.0);

        let (reference, measurement) = match detector {
            Detector::Absorbance => {
                // Reference is the lamp through an empty path; measurement is the same
                // light after Beer-Lambert attenuation by whatever is in the well.
                let transmitted = (-(field.optical_depth_at(0) as f64)).exp();
                (
                    dark + full_scale,
                    dark + full_scale * transmitted.clamp(0.0, 1.0),
                )
            }
            Detector::Fluorescence => {
                if !self.fluorescence_enabled {
                    return Err(Error::new(
                        ErrorCode::Driver,
                        "fluorescence detector is disabled",
                    ));
                }
                // The reference channel watches the excitation source, so a lamp that
                // drifts is visible in the data instead of silently scaling the result.
                let emission = field.emission_at(0) as f64 * integration;
                (
                    dark + full_scale * 0.85,
                    dark + (full_scale * emission).min(full_scale),
                )
            }
            Detector::Luminescence => {
                if !self.luminescence_enabled {
                    return Err(Error::new(
                        ErrorCode::Driver,
                        "luminescence detector is disabled",
                    ));
                }
                // No excitation, so no reference channel at all: a luminescence read is a
                // photon count and nothing else.
                let emission = field.emission_at(0) as f64 * integration;
                (0.0, dark + (full_scale * emission).min(full_scale))
            }
        };

        Ok(MeasuredCounts {
            reference: reference.round().max(0.0) as u64,
            measurement: measurement.round().max(0.0) as u64,
            wavelength_nm,
            integration_s,
        })
    }

    fn objective(&self) -> &SimObjective {
        &self.config.objectives[self.objective.min(self.config.objectives.len() - 1)]
    }

    /// Micrometres of sample per image pixel, for the applied objective and the
    /// current binning. This is the number the calibration chain exists for.
    fn sample_pixel_size_um(&self) -> f64 {
        self.config.pixel_pitch.micrometers() * self.binning as f64 / self.objective().magnification
    }

    #[doc(hidden)]
    pub fn runtime_scene_snapshot(&self) -> SimPlateReaderSceneSnapshot {
        SimPlateReaderSceneSnapshot {
            x_um: self.x_um,
            y_um: self.y_um,
            z_um: self.z_um,
            sample_pixel_size_um: self.sample_pixel_size_um(),
            magnification: self.objective().magnification,
            numerical_aperture: self.objective().numerical_aperture,
            laser_gate_enabled: self.lamp_on,
            lamp_power_fraction: if self.lamp_on {
                self.lamp_power_percent / 100.0
            } else {
                0.0
            },
        }
    }

    fn next_token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    /// Republishes the snapshot acquisition threads read. Called after every
    /// state change so a running stream follows the controls.
    fn publish_scene(&self) {
        if let Ok(mut scene) = self.scene.lock() {
            *scene = SceneState {
                x_um: self.x_um,
                y_um: self.y_um,
                z_um: self.z_um,
                objective: self.objective,
                light_path_open: self.turret_move.is_none(),
                lamp_on: self.lamp_on,
                lamp_power_percent: self.lamp_power_percent,
                exposure_s: self.exposure_s,
                gain_percent: self.gain_percent,
                binning: self.binning,
                epoch: self.epoch,
            };
        }
    }

    fn announce(&mut self, device: DeviceId, key: &str, value: Value) {
        self.events
            .push_back(DriverEvent::Event(Event::PropertyChanged(
                PropertyChanged {
                    device,
                    key: key.to_string(),
                    value,
                },
            )));
    }

    /// Derived optical values change without a client write, so they are
    /// announced whenever the turret lands or the binning changes.
    fn announce_optics(&mut self) {
        let magnification = self.objective().magnification;
        let aperture = self.objective().numerical_aperture;
        let pixel_size = self.sample_pixel_size_um();
        let turret = self.turret;
        let camera = self.camera;
        self.announce(turret, "magnification", Value::F64(magnification));
        self.announce(
            turret,
            "numerical_aperture",
            Value::NumericalAperture(NumericalAperture::new(aperture)),
        );
        self.announce(
            camera,
            "sample_pixel_size",
            Value::Position(Position::from_micrometers(pixel_size)),
        );
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        let Some(logical) = self.classify(device) else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown target device",
            ));
        };
        let config = &self.config;
        let value = match (logical, key) {
            (SimDevice::Hub, "model") => {
                Value::String("composed multiwell plate reader simulation".into())
            }
            (SimDevice::Hub, "sample_seed") => Value::I64(config.seed as i64),
            (SimDevice::Transport, "well") => Value::String(self.well.clone()),
            (SimDevice::Absorbance, "wavelength") => {
                Value::Wavelength(Wavelength::from_nanometers(self.absorbance_wavelength_nm))
            }
            (SimDevice::Fluorescence, "wavelength") => {
                Value::Wavelength(Wavelength::from_nanometers(self.fluorescence_wavelength_nm))
            }
            (SimDevice::Fluorescence, "enabled") => Value::Bool(self.fluorescence_enabled),
            (SimDevice::Luminescence, "enabled") => Value::Bool(self.luminescence_enabled),
            (SimDevice::Temperature, "target") => {
                Value::Temperature(Temperature::from_celsius(self.temperature_target_c))
            }
            (SimDevice::Temperature, "enabled") => Value::Bool(self.temperature_enabled),
            (SimDevice::Temperature, "actual") => {
                Value::Temperature(Temperature::from_celsius(self.actual_temperature_c()))
            }
            (SimDevice::Gas, "co2_target") => {
                Value::GasConcentration(GasConcentration::from_percent(self.co2_target_percent))
            }
            (SimDevice::Gas, "co2_actual") => {
                Value::GasConcentration(GasConcentration::from_percent(self.actual_co2_percent()))
            }
            (SimDevice::Gas, "enabled") => Value::Bool(self.gas_enabled),
            (SimDevice::Camera, "exposure") => {
                Value::TimeInterval(TimeInterval::from_seconds(self.exposure_s))
            }
            (SimDevice::Camera, "gain") => Value::Ratio(Ratio::from_percent(self.gain_percent)),
            (SimDevice::Camera, "frame_interval") => {
                Value::TimeInterval(TimeInterval::from_seconds(self.frame_interval_s))
            }
            (SimDevice::Camera, "binning") => Value::String(binning_name(self.binning).into()),
            (SimDevice::Camera, "pixel_pitch") => Value::Position(config.pixel_pitch),
            (SimDevice::Camera, "sensor_width") => {
                Value::PixelCount(PixelCount::new(config.sensor_width))
            }
            (SimDevice::Camera, "sensor_height") => {
                Value::PixelCount(PixelCount::new(config.sensor_height))
            }
            (SimDevice::Camera, "sample_pixel_size") => {
                Value::Position(Position::from_micrometers(self.sample_pixel_size_um()))
            }
            (SimDevice::Camera, "pixel_format") => {
                Value::String(ImageEncoding::Mono8.property_value().into())
            }
            (SimDevice::Xy, "x") => Value::Position(Position::from_micrometers(self.x_um)),
            (SimDevice::Xy, "y") => Value::Position(Position::from_micrometers(self.y_um)),
            (SimDevice::Xy, "speed") => {
                Value::Velocity(Velocity::from_micrometers_per_second(self.xy_speed_um_s))
            }
            (SimDevice::Xy, "busy") => Value::Bool(self.axis_busy(self.xy)),
            (SimDevice::Z, "z") => Value::Position(Position::from_micrometers(self.z_um)),
            (SimDevice::Z, "speed") => {
                Value::Velocity(Velocity::from_micrometers_per_second(self.z_speed_um_s))
            }
            (SimDevice::Z, "busy") => Value::Bool(self.axis_busy(self.z)),
            (SimDevice::Turret, "position") => Value::I64(self.requested_objective as i64 + 1),
            (SimDevice::Turret, "magnification") => Value::F64(self.objective().magnification),
            (SimDevice::Turret, "numerical_aperture") => Value::NumericalAperture(
                NumericalAperture::new(self.objective().numerical_aperture),
            ),
            (SimDevice::Turret, "busy") => Value::Bool(self.turret_move.is_some()),
            (SimDevice::Lamp, "enabled") => Value::Bool(self.lamp_on),
            (SimDevice::Lamp, "power") => {
                Value::Ratio(Ratio::from_percent(self.lamp_power_percent))
            }
            (SimDevice::Lamp, "interlock_closed") => Value::Bool(true),
            (SimDevice::Lamp, "fault") => Value::String("No Fault".into()),
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown property {key}"),
                ))
            }
        };
        Ok(value)
    }

    fn axis_busy(&self, device: DeviceId) -> bool {
        self.motions.iter().any(|motion| motion.device == device)
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        let Some(logical) = self.classify(device) else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown target device",
            ));
        };
        match (logical, key) {
            (SimDevice::Transport, "well") => {
                let label = match value {
                    Value::String(text) => text.clone(),
                    other => {
                        return Err(Error::new(
                            ErrorCode::InvalidProperty,
                            format!("well must be a string, got {other:?}"),
                        ))
                    }
                };
                self.move_to_well(&label)?;
            }
            (SimDevice::Absorbance, "wavelength") => {
                self.absorbance_wavelength_nm = wavelength_nm(value)?;
            }
            (SimDevice::Fluorescence, "wavelength") => {
                self.fluorescence_wavelength_nm = wavelength_nm(value)?;
            }
            (SimDevice::Fluorescence, "enabled") => {
                self.fluorescence_enabled = bool_value(value)?;
            }
            (SimDevice::Luminescence, "enabled") => {
                self.luminescence_enabled = bool_value(value)?;
            }
            (SimDevice::Temperature, "target") => {
                self.temperature_target_c = temperature_celsius(value)?;
            }
            (SimDevice::Temperature, "enabled") => {
                self.temperature_enabled = bool_value(value)?;
            }
            (SimDevice::Gas, "co2_target") => {
                self.co2_target_percent = gas_percent(value)?;
            }
            (SimDevice::Gas, "enabled") => {
                self.gas_enabled = bool_value(value)?;
            }
            (SimDevice::Camera, "exposure") => {
                self.exposure_s = time_seconds(value)?;
            }
            (SimDevice::Camera, "gain") => {
                self.gain_percent = ratio_percent(value)?;
            }
            (SimDevice::Camera, "frame_interval") => {
                self.frame_interval_s = time_seconds(value)?;
            }
            (SimDevice::Camera, "binning") => {
                self.binning = binning_factor(value)?;
                self.announce_optics();
            }
            (SimDevice::Xy, "x") => {
                self.x_um = position_um(value)?;
                self.supersede(self.xy, StageAxis::X);
            }
            (SimDevice::Xy, "y") => {
                self.y_um = position_um(value)?;
                self.supersede(self.xy, StageAxis::Y);
            }
            (SimDevice::Xy, "speed") => {
                self.xy_speed_um_s = velocity_um_s(value)?;
            }
            (SimDevice::Z, "z") => {
                self.z_um = position_um(value)?;
                self.supersede(self.z, StageAxis::Z);
            }
            (SimDevice::Z, "speed") => {
                self.z_speed_um_s = velocity_um_s(value)?;
            }
            (SimDevice::Turret, "position") => {
                let position = match value {
                    Value::I64(position) => *position,
                    _ => {
                        return Err(Error::new(
                            ErrorCode::InvalidProperty,
                            "turret position expects an integer",
                        ))
                    }
                };
                self.start_turret_move(position, None)?;
            }
            (SimDevice::Lamp, "enabled") => {
                self.lamp_on = match value {
                    Value::Bool(enabled) => *enabled,
                    _ => {
                        return Err(Error::new(
                            ErrorCode::InvalidProperty,
                            "lamp enabled expects a boolean",
                        ))
                    }
                };
            }
            (SimDevice::Lamp, "power") => {
                self.lamp_power_percent = ratio_percent(value)?;
            }
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("property {key} is not writable"),
                ))
            }
        }
        self.publish_scene();
        Ok(())
    }

    /// A direct position write wins over a move in flight: the GUI writes stage
    /// coordinates at drag rate, and failing those writes would be worse than
    /// ending the move early.
    fn supersede(&mut self, device: DeviceId, axis: StageAxis) {
        let mut finished = Vec::new();
        self.motions.retain(|motion| {
            let hit = motion.device == device
                && motion
                    .axes
                    .iter()
                    .any(|(candidate, _, _)| *candidate == axis);
            if hit {
                finished.push(motion.token);
            }
            !hit
        });
        for token in finished {
            self.events.push_back(DriverEvent::TokenCompleted {
                token,
                value: Value::Map(BTreeMap::from([
                    ("superseded".into(), Value::Bool(true)),
                    ("axis".into(), Value::String(axis.name().into())),
                ])),
            });
        }
    }

    fn start_turret_move(&mut self, position: i64, token: Option<DriverToken>) -> Result<()> {
        let count = self.config.objectives.len() as i64;
        if position < 1 || position > count {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("turret position must be 1..{count}"),
            ));
        }
        let target = (position - 1) as usize;
        self.requested_objective = target;
        if target == self.objective && self.turret_move.is_none() {
            if let Some(token) = token {
                self.events.push_back(DriverEvent::TokenCompleted {
                    token,
                    value: self.turret_completion(),
                });
            }
            return Ok(());
        }
        self.turret_move = Some(PendingTurret {
            token,
            started: Instant::now(),
            duration: Duration::from_secs_f64(self.config.objective_switch.seconds().max(0.0)),
            target,
        });
        let turret = self.turret;
        self.announce(turret, "busy", Value::Bool(true));
        self.announce(turret, "position", Value::I64(position));
        self.publish_scene();
        Ok(())
    }

    fn turret_completion(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("position".into(), Value::I64(self.objective as i64 + 1)),
            (
                "magnification".into(),
                Value::F64(self.objective().magnification),
            ),
            (
                "numerical_aperture".into(),
                Value::NumericalAperture(NumericalAperture::new(
                    self.objective().numerical_aperture,
                )),
            ),
            (
                "sample_pixel_size".into(),
                Value::Position(Position::from_micrometers(self.sample_pixel_size_um())),
            ),
        ]))
    }

    fn start_motion(
        &mut self,
        token: DriverToken,
        device: DeviceId,
        targets: Vec<(StageAxis, f64)>,
        homing: bool,
    ) -> Result<()> {
        let mut axes = Vec::new();
        let mut seconds: f64 = 0.0;
        for (axis, target) in targets {
            let (from, speed) = match axis {
                StageAxis::X => (self.x_um, self.xy_speed_um_s),
                StageAxis::Y => (self.y_um, self.xy_speed_um_s),
                StageAxis::Z => (self.z_um, self.z_speed_um_s),
                _ => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        format!("axis {} is not present", axis.name()),
                    ))
                }
            };
            let target = self.clamp_axis(&axis, target);
            seconds = seconds.max((target - from).abs() / speed.max(f64::MIN_POSITIVE));
            axes.push((axis, from, target));
        }
        if axes.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "stage move needs at least one axis",
            ));
        }
        self.motions.push(PendingMotion {
            token,
            device,
            started: Instant::now(),
            duration: Duration::from_secs_f64(seconds) + SETTLE,
            axes,
            homing,
        });
        self.announce(device, "busy", Value::Bool(true));
        Ok(())
    }

    fn clamp_axis(&self, axis: &StageAxis, value: f64) -> f64 {
        match axis {
            StageAxis::X | StageAxis::Y => {
                let half = self.config.xy_travel.micrometers() / 2.0;
                value.clamp(-half, half)
            }
            StageAxis::Z => value.clamp(0.0, self.config.z_travel.micrometers()),
            _ => value,
        }
    }

    fn set_axis(&mut self, axis: &StageAxis, value: f64) {
        match axis {
            StageAxis::X => self.x_um = value,
            StageAxis::Y => self.y_um = value,
            StageAxis::Z => self.z_um = value,
            _ => {}
        }
    }

    fn axis_key(axis: &StageAxis) -> &'static str {
        match axis {
            StageAxis::X => "x",
            StageAxis::Y => "y",
            StageAxis::Z => "z",
            _ => "position",
        }
    }

    /// Advances modeled motion and turret rotation. Called from `poll()`, which
    /// the runtime lane drives roughly every ten milliseconds.
    fn advance(&mut self) {
        let now = Instant::now();
        let mut updates: Vec<(DeviceId, StageAxis, f64)> = Vec::new();
        let mut finished: Vec<(DriverToken, DeviceId, Value)> = Vec::new();

        let mut index = 0;
        while index < self.motions.len() {
            let elapsed = now.duration_since(self.motions[index].started);
            let duration = self.motions[index].duration;
            let progress = if duration.is_zero() {
                1.0
            } else {
                (elapsed.as_secs_f64() / duration.as_secs_f64()).clamp(0.0, 1.0)
            };
            for (axis, from, to) in &self.motions[index].axes {
                updates.push((
                    self.motions[index].device,
                    axis.clone(),
                    from + (to - from) * progress,
                ));
            }
            if progress >= 1.0 {
                let motion = self.motions.remove(index);
                let mut summary = BTreeMap::from([(
                    "duration".into(),
                    Value::TimeInterval(TimeInterval::from_seconds(motion.duration.as_secs_f64())),
                )]);
                if motion.homing {
                    summary.insert("homed".into(), Value::Bool(true));
                }
                for (axis, _, to) in &motion.axes {
                    summary.insert(
                        Self::axis_key(axis).into(),
                        Value::Position(Position::from_micrometers(*to)),
                    );
                }
                finished.push((motion.token, motion.device, Value::Map(summary)));
            } else {
                index += 1;
            }
        }

        for (device, axis, value) in updates {
            self.set_axis(&axis, value);
            self.announce(
                device,
                Self::axis_key(&axis),
                Value::Position(Position::from_micrometers(value)),
            );
        }
        for (token, device, value) in finished {
            if !self.axis_busy(device) {
                self.announce(device, "busy", Value::Bool(false));
            }
            self.events
                .push_back(DriverEvent::TokenCompleted { token, value });
        }

        if let Some(turret) = &self.turret_move {
            if now.duration_since(turret.started) >= turret.duration {
                let token = turret.token;
                self.objective = turret.target;
                self.turret_move = None;
                let turret_device = self.turret;
                self.announce(turret_device, "busy", Value::Bool(false));
                self.announce_optics();
                if let Some(token) = token {
                    let value = self.turret_completion();
                    self.events
                        .push_back(DriverEvent::TokenCompleted { token, value });
                }
            }
        }
        self.publish_scene();
    }

    fn capture(&mut self, request: &CameraCaptureRequest) -> Value {
        let scene = self.scene_snapshot();
        let index = self.frames.fetch_add(1, Ordering::Relaxed);
        let frame = render_frame(
            &self.config,
            scene,
            index,
            self.camera,
            FrameHandle {
                stream: StreamId(self.camera.0 .0),
                frame: FrameId(index),
            },
            request.buffer.clone().unwrap_or_default(),
        );
        let summary = frame_completion(&frame);
        self.events.push_back(DriverEvent::FrameReady(frame));
        summary
    }

    fn scene_snapshot(&self) -> SceneState {
        self.scene
            .lock()
            .map(|scene| *scene)
            .unwrap_or_else(|poisoned| *poisoned.into_inner())
    }
}

impl Driver for SimPlateReaderDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        let config = &self.config;
        let objective_values = config
            .objectives
            .iter()
            .enumerate()
            .map(|(index, objective)| EnumValue {
                value: Value::I64(index as i64 + 1),
                label: objective.label.clone(),
            })
            .collect::<Vec<_>>();
        vec![
            DeviceDescriptor {
                id: self.hub,
                driver: self.id,
                label: "sim-plate-reader".into(),
                vendor: Some("numanager".into()),
                model: Some("composed multiwell plate reader".into()),
                serial: None,
                kinds: vec!["hub".into(), "simulator".into()],
                properties: vec![
                    property("model", "Model", ValueType::String, None, false),
                    property("sample_seed", "Sample seed", ValueType::I64, None, false),
                ],
                metadata: BTreeMap::from([
                    (
                        "model".into(),
                        Value::String("multiwell plate over a pluggable specimen".into()),
                    ),
                    (
                        "objective_role".into(),
                        Value::String(OBJECTIVE_ROLE.into()),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.camera,
                driver: self.id,
                label: "sim-plate-camera".into(),
                vendor: Some("numanager".into()),
                model: Some("composed brightfield camera".into()),
                serial: None,
                kinds: vec!["camera".into(), "simulator".into()],
                properties: vec![
                    time_property(
                        "exposure",
                        "Exposure",
                        TimeInterval::from_milliseconds(0.1),
                        TimeInterval::from_seconds(10.0),
                    ),
                    ratio_property("gain", "Gain", 10.0, 1_000.0),
                    time_property(
                        "frame_interval",
                        "Frame interval",
                        TimeInterval::from_milliseconds(1.0),
                        TimeInterval::from_seconds(10.0),
                    ),
                    enum_property("binning", "Binning", &["1x1", "2x2", "4x4"]),
                    property(
                        "pixel_pitch",
                        "Sensor pixel pitch",
                        ValueType::Position,
                        Some("um"),
                        false,
                    ),
                    property(
                        "sensor_width",
                        "Sensor width",
                        ValueType::PixelCount,
                        Some("px"),
                        false,
                    ),
                    property(
                        "sensor_height",
                        "Sensor height",
                        ValueType::PixelCount,
                        Some("px"),
                        false,
                    ),
                    volatile_property(
                        "sample_pixel_size",
                        "Sample pixel size",
                        ValueType::Position,
                        Some("um"),
                    ),
                    property(
                        "pixel_format",
                        "Pixel format",
                        ValueType::String,
                        None,
                        false,
                    ),
                ],
                metadata: BTreeMap::from([
                    ("pixel_pitch".into(), Value::Position(config.pixel_pitch)),
                    (
                        "sensor_width".into(),
                        Value::PixelCount(PixelCount::new(config.sensor_width)),
                    ),
                    (
                        "sensor_height".into(),
                        Value::PixelCount(PixelCount::new(config.sensor_height)),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.xy,
                driver: self.id,
                label: "sim-plate-xy".into(),
                vendor: Some("numanager".into()),
                model: Some("composed sample-plane xy stage".into()),
                serial: None,
                kinds: vec!["stage.xy".into(), "axis.xy".into(), "simulator".into()],
                properties: vec![
                    plate_axis_property("x", "X position", config.plate.x_extent_um()),
                    plate_axis_property("y", "Y position", config.plate.y_extent_um()),
                    speed_property(),
                    volatile_property("busy", "Busy", ValueType::Bool, None),
                ],
                metadata: BTreeMap::from([("travel".into(), Value::Position(config.xy_travel))]),
            },
            DeviceDescriptor {
                id: self.z,
                driver: self.id,
                label: "sim-plate-z".into(),
                vendor: Some("numanager".into()),
                model: Some("composed focus drive".into()),
                serial: None,
                kinds: vec!["stage.z".into(), "axis.z".into(), "simulator".into()],
                properties: vec![
                    focus_property("z", "Z position", config.z_travel.micrometers()),
                    speed_property(),
                    volatile_property("busy", "Busy", ValueType::Bool, None),
                ],
                metadata: BTreeMap::from([("travel".into(), Value::Position(config.z_travel))]),
            },
            DeviceDescriptor {
                id: self.turret,
                driver: self.id,
                label: "sim-plate-objective".into(),
                vendor: Some("numanager".into()),
                model: Some("composed objective turret".into()),
                serial: None,
                kinds: vec![
                    "objective.turret".into(),
                    "state.device".into(),
                    "simulator".into(),
                ],
                properties: vec![
                    objective_position_property(objective_values, config.objectives.len() as i64),
                    volatile_property("magnification", "Magnification", ValueType::F64, Some("x")),
                    volatile_property(
                        "numerical_aperture",
                        "Numerical aperture",
                        ValueType::NumericalAperture,
                        None,
                    ),
                    volatile_property("busy", "Busy", ValueType::Bool, None),
                ],
                metadata: BTreeMap::from([("role".into(), Value::String(OBJECTIVE_ROLE.into()))]),
            },
            DeviceDescriptor {
                id: self.lamp,
                driver: self.id,
                label: "sim-plate-lamp".into(),
                vendor: Some("numanager".into()),
                model: Some("composed transmitted-light lamp".into()),
                serial: None,
                kinds: vec!["light.source".into(), "shutter".into(), "simulator".into()],
                properties: vec![
                    sequenceable_property("enabled", "Enabled", ValueType::Bool, None, true),
                    ratio_property("power", "Power", 0.0, 100.0),
                    property(
                        "interlock_closed",
                        "Interlock closed",
                        ValueType::Bool,
                        None,
                        false,
                    ),
                    property("fault", "Fault", ValueType::String, None, false),
                ],
                metadata: BTreeMap::from([(
                    "wavelength".into(),
                    Value::Wavelength(config.illumination_wavelength),
                )]),
            },
            DeviceDescriptor {
                id: self.transport,
                driver: self.id,
                label: "sim-plate-transport".into(),
                vendor: Some("numanager".into()),
                model: Some("simulated plate carrier".into()),
                serial: None,
                kinds: vec!["plate.transport".into(), "simulator".into()],
                properties: vec![{
                    let mut schema =
                        sequenceable_property("well", "Well", ValueType::String, None, true);
                    // Every well is enumerated rather than range-checked, so a client can
                    // populate a picker and a bad label is rejected by the schema before it
                    // ever reaches the carrier.
                    schema.enum_values = config
                        .plate
                        .well_labels()
                        .into_iter()
                        .map(|label| EnumValue {
                            value: Value::String(label.clone()),
                            label,
                        })
                        .collect();
                    schema
                }],
                metadata: BTreeMap::from([
                    ("rows".into(), Value::I64(config.plate.rows as i64)),
                    ("columns".into(), Value::I64(config.plate.columns as i64)),
                    (
                        "well_pitch".into(),
                        Value::Position(Position::from_micrometers(config.plate.well_pitch_um)),
                    ),
                    (
                        "well_diameter".into(),
                        Value::Position(Position::from_micrometers(config.plate.well_diameter_um)),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.absorbance,
                driver: self.id,
                label: "sim-plate-absorbance".into(),
                vendor: Some("numanager".into()),
                model: Some("simulated absorbance detector".into()),
                serial: None,
                kinds: vec!["detector.absorbance".into(), "simulator".into()],
                properties: vec![wavelength_property(
                    "wavelength",
                    "Wavelength",
                    200.0,
                    1000.0,
                )],
                metadata: BTreeMap::new(),
            },
            DeviceDescriptor {
                id: self.fluorescence,
                driver: self.id,
                label: "sim-plate-fluorescence".into(),
                vendor: Some("numanager".into()),
                model: Some("simulated fluorescence detector".into()),
                serial: None,
                kinds: vec![
                    "detector.fluorescence".into(),
                    "light.source".into(),
                    "simulator".into(),
                ],
                properties: vec![
                    wavelength_property("wavelength", "Excitation wavelength", 200.0, 1000.0),
                    sequenceable_property("enabled", "Enabled", ValueType::Bool, None, true),
                ],
                metadata: BTreeMap::new(),
            },
            DeviceDescriptor {
                id: self.luminescence,
                driver: self.id,
                label: "sim-plate-luminescence".into(),
                vendor: Some("numanager".into()),
                model: Some("simulated luminescence detector".into()),
                serial: None,
                kinds: vec!["detector.luminescence".into(), "simulator".into()],
                properties: vec![sequenceable_property(
                    "enabled",
                    "Enabled",
                    ValueType::Bool,
                    None,
                    true,
                )],
                metadata: BTreeMap::new(),
            },
            DeviceDescriptor {
                id: self.temperature,
                driver: self.id,
                label: "sim-plate-temperature".into(),
                vendor: Some("numanager".into()),
                model: Some("simulated incubator".into()),
                serial: None,
                kinds: vec!["environment.temperature".into(), "simulator".into()],
                properties: vec![
                    temperature_property("target", "Target", 4.0, 45.0),
                    sequenceable_property("enabled", "Enabled", ValueType::Bool, None, true),
                    property("actual", "Actual", ValueType::Temperature, None, false),
                ],
                metadata: BTreeMap::new(),
            },
            DeviceDescriptor {
                id: self.gas,
                driver: self.id,
                label: "sim-plate-gas".into(),
                vendor: Some("numanager".into()),
                model: Some("simulated gas controller".into()),
                serial: None,
                kinds: vec!["environment.gas".into(), "simulator".into()],
                properties: vec![
                    gas_property("co2_target", "CO2 target", 0.0, 20.0),
                    property(
                        "co2_actual",
                        "CO2 actual",
                        ValueType::GasConcentration,
                        None,
                        false,
                    ),
                    sequenceable_property("enabled", "Enabled", ValueType::Bool, None, true),
                ],
                metadata: BTreeMap::new(),
            },
        ]
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "sim-plate-specimen".into(),
            kind: "simulated.biological_scene".into(),
            metadata: BTreeMap::from([
                (
                    "model".into(),
                    Value::String("multiwell plate over a pluggable specimen".into()),
                ),
                ("seed".into(), Value::I64(self.config.seed as i64)),
                (
                    "completion".into(),
                    Value::String("modeled travel and acquisition timing".into()),
                ),
            ]),
        }]
    }

    fn graph(&self) -> DeviceGraph {
        let mut graph = DeviceGraph::default();
        let _ = graph.insert_node(GraphNode {
            id: self.resource.0,
            kind: NodeKind::Resource,
            label: "sim-plate-specimen".into(),
        });
        let _ = graph.insert_node(GraphNode {
            id: self.hub.0,
            kind: NodeKind::Hub,
            label: "sim-plate-reader".into(),
        });
        let _ = graph.insert_edge(GraphEdge {
            from: self.resource.0,
            to: self.hub.0,
            kind: EdgeKind::OwnsResource,
        });
        for device in self.descriptors() {
            if device.id == self.hub {
                continue;
            }
            let _ = graph.insert_node(GraphNode {
                id: device.id.0,
                kind: NodeKind::Device,
                label: device.label.clone(),
            });
            let _ = graph.insert_edge(GraphEdge {
                from: self.hub.0,
                to: device.id.0,
                kind: EdgeKind::OffersDevice,
            });
        }
        let _ = graph.insert_device_dependency(self.xy.0, self.camera.0, Role::XYStage);
        let _ = graph.insert_device_dependency(self.z.0, self.camera.0, Role::ZStage);
        let _ = graph.insert_device_dependency(self.lamp.0, self.camera.0, Role::LightSource);
        let _ = graph.insert_device_dependency(
            self.turret.0,
            self.camera.0,
            Role::Custom(OBJECTIVE_ROLE.into()),
        );
        graph
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        match self.classify(device) {
            Some(SimDevice::Camera) => vec![
                capability(1, device, CapabilityKind::CameraCapture),
                capability(2, device, CapabilityKind::CameraStream),
            ],
            Some(SimDevice::Xy) => vec![
                capability(3, device, CapabilityKind::StageMove),
                capability(4, device, CapabilityKind::StageHome),
                capability(5, device, CapabilityKind::StageStop),
            ],
            Some(SimDevice::Z) => vec![
                capability(6, device, CapabilityKind::StageMove),
                capability(7, device, CapabilityKind::StageHome),
                capability(8, device, CapabilityKind::StageStop),
            ],
            Some(SimDevice::Turret) => vec![capability(9, device, CapabilityKind::FilterSelect)],
            Some(SimDevice::Transport) => vec![capability(10, device, CapabilityKind::PlateMove)],
            Some(SimDevice::Absorbance) => vec![capability(11, device, CapabilityKind::Measure)],
            Some(SimDevice::Fluorescence) => vec![capability(12, device, CapabilityKind::Measure)],
            Some(SimDevice::Luminescence) => vec![capability(13, device, CapabilityKind::Measure)],
            Some(SimDevice::Temperature) => {
                vec![capability(14, device, CapabilityKind::TemperatureControl)]
            }
            Some(SimDevice::Gas) => vec![capability(15, device, CapabilityKind::GasControl)],
            _ => Vec::new(),
        }
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut physical_transactions = Vec::new();
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    let _ = self.read_property(*device, key)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("sim plate reader read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("sim plate reader write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("sim microscope state set {:?}", set.name),
                        payload: Value::I64(set.writes.len() as i64),
                    });
                }
                Command::Invoke {
                    device, request, ..
                } => {
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!(
                            "sim microscope {:?} on {}",
                            request.request_kind(),
                            self.label_of(*device)
                        ),
                        payload: Value::String(format!("{:?}", request.request_kind())),
                    });
                }
                Command::Arm(_) | Command::Start(_) | Command::Stop(_) => {
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "sim microscope timing transition".into(),
                        payload: Value::Null,
                    });
                }
            }
        }
        Ok(PreparedBatch {
            id: batch.id,
            commands: batch.commands.clone(),
            physical_transactions,
        })
    }

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.next_token();
        let mut completion = Value::Bool(true);
        let mut deferred = false;

        for command in prepared.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    completion = self.read_property(device, &key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    self.write_property(device, &key, &value)?;
                    self.announce(device, &key, value);
                }
                Command::ApplyStateSet(set) => {
                    for write in set.writes {
                        self.write_property(write.device, &write.property, &write.value)?;
                        self.announce(write.device, &write.property, write.value);
                    }
                }
                Command::Invoke {
                    request: CapabilityRequest::CameraCapture(request),
                    ..
                } => {
                    completion = self.capture(&request);
                }
                Command::Invoke {
                    request: CapabilityRequest::CameraStream(request),
                    ..
                } => {
                    self.start_stream(token, request);
                    deferred = true;
                }
                Command::Invoke {
                    device,
                    request: CapabilityRequest::StageMove(request),
                    ..
                } => {
                    let targets = self.move_targets(device, &request)?;
                    self.start_motion(token, device, targets, false)?;
                    deferred = true;
                }
                Command::Invoke {
                    device,
                    request: CapabilityRequest::FilterSelect(request),
                    ..
                } if device == self.turret => {
                    self.start_turret_move(request.position as i64, Some(token))?;
                    deferred = self.turret_move.is_some();
                }
                Command::Invoke {
                    request: CapabilityRequest::PlateMove(request),
                    ..
                } => {
                    self.move_to_well(&request.well)?;
                    self.announce(self.transport, "well", Value::String(self.well.clone()));
                    completion = Value::Map(BTreeMap::from([
                        ("well".into(), Value::String(self.well.clone())),
                        (
                            "x".into(),
                            Value::Position(Position::from_micrometers(self.x_um)),
                        ),
                        (
                            "y".into(),
                            Value::Position(Position::from_micrometers(self.y_um)),
                        ),
                    ]));
                }
                Command::Invoke {
                    device,
                    request: CapabilityRequest::Measure(request),
                    ..
                } => {
                    let detector = match self.classify(device) {
                        Some(SimDevice::Absorbance) => Detector::Absorbance,
                        Some(SimDevice::Fluorescence) => Detector::Fluorescence,
                        Some(SimDevice::Luminescence) => Detector::Luminescence,
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                format!("{} is not a detector", self.label_of(device)),
                            ))
                        }
                    };
                    let integration_s = request
                        .integration_time
                        .map(|interval| interval.seconds())
                        .unwrap_or(0.02);
                    let counts = self.measure(detector, integration_s)?;
                    completion = counts.completion(&self.well);
                }
                Command::Invoke {
                    request: CapabilityRequest::TemperatureControl(request),
                    ..
                } => {
                    if let Some(target) = request.target {
                        self.temperature_target_c = target.celsius();
                        self.announce(self.temperature, "target", Value::Temperature(target));
                    }
                    if let Some(enabled) = request.enabled {
                        self.temperature_enabled = enabled;
                        self.announce(self.temperature, "enabled", Value::Bool(enabled));
                    }
                    completion = Value::Map(BTreeMap::from([
                        (
                            "target".into(),
                            Value::Temperature(Temperature::from_celsius(
                                self.temperature_target_c,
                            )),
                        ),
                        (
                            "actual".into(),
                            Value::Temperature(Temperature::from_celsius(
                                self.actual_temperature_c(),
                            )),
                        ),
                        ("enabled".into(), Value::Bool(self.temperature_enabled)),
                    ]));
                }
                Command::Invoke {
                    request: CapabilityRequest::GasControl(request),
                    ..
                } => {
                    if let Some(target) = request.co2_target {
                        self.co2_target_percent = target.percent();
                        self.announce(self.gas, "co2_target", Value::GasConcentration(target));
                    }
                    if let Some(enabled) = request.enabled {
                        self.gas_enabled = enabled;
                        self.announce(self.gas, "enabled", Value::Bool(enabled));
                    }
                    completion = Value::Map(BTreeMap::from([
                        (
                            "co2_target".into(),
                            Value::GasConcentration(GasConcentration::from_percent(
                                self.co2_target_percent,
                            )),
                        ),
                        (
                            "co2_actual".into(),
                            Value::GasConcentration(GasConcentration::from_percent(
                                self.actual_co2_percent(),
                            )),
                        ),
                        ("enabled".into(), Value::Bool(self.gas_enabled)),
                    ]));
                }
                Command::Invoke {
                    device,
                    request: CapabilityRequest::None,
                    capability,
                } => {
                    deferred = self.stage_transition(token, device, capability)?;
                }
                Command::Invoke {
                    device, request, ..
                } => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        format!(
                            "{} does not accept {:?}",
                            self.label_of(device),
                            request.request_kind()
                        ),
                    ));
                }
                Command::Arm(_) | Command::Start(_) | Command::Stop(_) => {
                    completion = Value::Bool(true);
                }
            }
        }

        if !deferred {
            self.events.push_back(DriverEvent::TokenCompleted {
                token,
                value: completion,
            });
        }
        Ok(token)
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        while let Ok(event) = self.worker_rx.try_recv() {
            self.events.push_back(event);
        }
        if !self.motions.is_empty() || self.turret_move.is_some() {
            self.advance();
        }
        self.events.drain(..).collect()
    }

    fn cancel(&mut self, token: DriverToken) -> CancelResult {
        if let Some(stop) = self.streams.remove(&token) {
            stop.store(true, Ordering::Relaxed);
            return CancelResult::Cancelled;
        }
        if let Some(index) = self.motions.iter().position(|motion| motion.token == token) {
            let motion = self.motions.remove(index);
            let device = motion.device;
            if !self.axis_busy(device) {
                self.announce(device, "busy", Value::Bool(false));
            }
            return CancelResult::Cancelled;
        }
        CancelResult::Unsupported
    }
}

impl SimPlateReaderDriver {
    fn label_of(&self, device: DeviceId) -> String {
        self.descriptors()
            .into_iter()
            .find(|descriptor| descriptor.id == device)
            .map(|descriptor| descriptor.label)
            .unwrap_or_else(|| "unknown device".into())
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        let Some(schema) = self
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.id == device)
            .and_then(|descriptor| {
                descriptor
                    .properties
                    .into_iter()
                    .find(|schema| schema.key == key)
            })
        else {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown property {key}"),
            ));
        };
        if !schema.writable {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("property {key} is read-only"),
            ));
        }
        schema.validate(value)
    }

    fn move_targets(
        &self,
        device: DeviceId,
        request: &StageMoveRequest,
    ) -> Result<Vec<(StageAxis, f64)>> {
        let allowed: &[StageAxis] = match self.classify(device) {
            Some(SimDevice::Xy) => &[StageAxis::X, StageAxis::Y],
            Some(SimDevice::Z) => &[StageAxis::Z],
            _ => {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    "device does not move a stage axis",
                ))
            }
        };
        let mut targets = Vec::new();
        for (axis, position) in &request.target {
            if !allowed.contains(axis) {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    format!(
                        "{} does not drive axis {}",
                        self.label_of(device),
                        axis.name()
                    ),
                ));
            }
            let current = match axis {
                StageAxis::X => self.x_um,
                StageAxis::Y => self.y_um,
                _ => self.z_um,
            };
            let target = if request.relative {
                current + position.micrometers()
            } else {
                position.micrometers()
            };
            targets.push((axis.clone(), target));
        }
        Ok(targets)
    }

    /// `StageHome` and `StageStop` carry no request payload, so the capability
    /// descriptor identifies which of the two was invoked.
    fn stage_transition(
        &mut self,
        token: DriverToken,
        device: DeviceId,
        capability: CapabilityId,
    ) -> Result<bool> {
        let kind = self
            .capabilities(device)
            .into_iter()
            .find(|descriptor| descriptor.id == capability)
            .map(|descriptor| descriptor.kind)
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidCommand,
                    "capability is not offered by this device",
                )
            })?;
        match kind {
            CapabilityKind::StageHome => {
                let targets = match self.classify(device) {
                    Some(SimDevice::Xy) => vec![(StageAxis::X, 0.0), (StageAxis::Y, 0.0)],
                    Some(SimDevice::Z) => vec![(StageAxis::Z, 0.0)],
                    _ => {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "device does not move a stage axis",
                        ))
                    }
                };
                self.start_motion(token, device, targets, true)?;
                Ok(true)
            }
            CapabilityKind::StageStop => {
                let mut stopped = Vec::new();
                self.motions.retain(|motion| {
                    if motion.device == device {
                        stopped.push(motion.token);
                        false
                    } else {
                        true
                    }
                });
                self.announce(device, "busy", Value::Bool(false));
                for stopped_token in stopped {
                    self.events.push_back(DriverEvent::TokenCompleted {
                        token: stopped_token,
                        value: Value::Map(BTreeMap::from([("stopped".into(), Value::Bool(true))])),
                    });
                }
                self.events.push_back(DriverEvent::TokenCompleted {
                    token,
                    value: Value::Map(BTreeMap::from([("stopped".into(), Value::Bool(true))])),
                });
                Ok(true)
            }
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!("{} needs a typed request", other.name()),
            )),
        }
    }

    fn start_stream(&mut self, token: DriverToken, request: CameraStreamRequest) {
        let stream = StreamId(token.0);
        let stop = Arc::new(AtomicBool::new(false));
        self.streams.insert(token, Arc::clone(&stop));
        let tx = self.worker_tx.clone();
        let config = Arc::clone(&self.config);
        let scene = Arc::clone(&self.scene);
        let frames = Arc::clone(&self.frames);
        let camera = self.camera;
        let buffer = request.buffer.clone();
        let frame_count = request.frame_count;
        let frame_interval_s = self.frame_interval_s;
        let sensor = (self.config.sensor_width, self.config.sensor_height);
        thread::spawn(move || {
            let mut sequence = 0u64;
            let mut geometry = (sensor.0, sensor.1);
            while !stop.load(Ordering::Relaxed) {
                if frame_count.is_some_and(|limit| sequence >= limit) {
                    let _ = tx.send(DriverEvent::TokenCompleted {
                        token,
                        value: stream_completion(stream, sequence, geometry),
                    });
                    return;
                }
                let snapshot = scene
                    .lock()
                    .map(|scene| *scene)
                    .unwrap_or_else(|poisoned| *poisoned.into_inner());
                let started = Instant::now();
                let index = frames.fetch_add(1, Ordering::Relaxed);
                let frame = render_frame(
                    &config,
                    snapshot,
                    index,
                    camera,
                    FrameHandle {
                        stream,
                        frame: FrameId(sequence),
                    },
                    buffer.clone(),
                );
                geometry = (frame.width, frame.height);
                let _ = tx.send(DriverEvent::FrameReady(frame));
                sequence += 1;
                let period = Duration::from_secs_f64(
                    frame_interval_s.max(snapshot.exposure_s).clamp(0.001, 10.0),
                );
                let spent = started.elapsed();
                if period > spent {
                    thread::sleep(period - spent);
                }
            }
        });
    }
}

fn stream_completion(stream: StreamId, frames: u64, geometry: (u32, u32)) -> Value {
    Value::Map(BTreeMap::from([
        ("stream".into(), Value::I64(stream.0 as i64)),
        ("frames".into(), Value::I64(frames as i64)),
        (
            "width".into(),
            Value::PixelCount(PixelCount::new(geometry.0)),
        ),
        (
            "height".into(),
            Value::PixelCount(PixelCount::new(geometry.1)),
        ),
        (
            "pixel_format".into(),
            Value::String(ImageEncoding::Mono8.property_value().into()),
        ),
    ]))
}

// ---------------------------------------------------------------------------
// Image formation
// ---------------------------------------------------------------------------

struct RenderedImage {
    width: u32,
    height: u32,
    data: Vec<u8>,
    saturated: u32,
}

/// Turn the specimen's field into a frame: illumination, vignetting, Beer–Lambert,
/// exposure and gain, then the detector's own shot and read noise and its 8-bit clamp.
///
/// Everything about *what is there* — cells, their positions, how defocus spreads them —
/// belongs to the [`Specimen`](sim_sample::Specimen) and reaches this function as a field
/// of means. Everything from here down is the camera.
fn render_image(
    config: &SimPlateReaderConfig,
    scene: SceneState,
    frame_index: u64,
) -> RenderedImage {
    let objective = &config.objectives[scene.objective.min(config.objectives.len() - 1)];
    let binning = scene.binning.max(1);
    let width = (config.sensor_width / binning).max(1);
    let height = (config.sensor_height / binning).max(1);
    let pixel_um = config.pixel_pitch.micrometers() * binning as f64 / objective.magnification;
    let wavelength_um = config.illumination_wavelength.nanometers() / 1_000.0;

    let half_w_um = width as f64 * pixel_um / 2.0;
    let half_h_um = height as f64 * pixel_um / 2.0;

    let field = config.specimen.render_field(&sim_sample::FieldRequest {
        origin_x_um: scene.x_um - half_w_um,
        origin_y_um: scene.y_um - half_h_um,
        pixel_um,
        width,
        height,
        focus_z_um: scene.z_um,
        numerical_aperture: objective.numerical_aperture,
        wavelength_um,
        // The imaging camera is transmitted-light here; a fluorescence channel would set
        // an excitation band.
        excitation_um: None,
        blur_limit_px: BLUR_PIXEL_LIMIT,
        time_s: scene.epoch.elapsed().as_secs_f64(),
    });

    let illumination = if scene.lamp_on && scene.light_path_open {
        (scene.lamp_power_percent / 100.0).clamp(0.0, 1.0)
    } else {
        0.0
    } + STRAY_LIGHT;
    let exposure_gain = (scene.exposure_s / EXPOSURE_REFERENCE_S).clamp(0.0, 500.0);
    let electron_gain = config.full_well_electrons
        * BACKGROUND_FILL
        * exposure_gain
        * (scene.gain_percent / 100.0).max(0.0);
    let half_x = width as f64 / 2.0;
    let half_y = height as f64 / 2.0;
    let radius_norm = 1.0 / (half_x * half_x + half_y * half_y).sqrt();

    let mut data = vec![0u8; (width * height) as usize];
    let mut saturated = 0u32;
    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) as usize;
            let dx = x as f64 + 0.5 - half_x;
            let dy = y as f64 + 0.5 - half_y;
            let rho = (dx * dx + dy * dy).sqrt() * radius_norm;
            let vignette = 1.0 / (1.0 + VIGNETTE_K * rho * rho).powi(2);
            let transmitted =
                illumination * vignette * (-(field.optical_depth_at(index) as f64)).exp();
            // A specimen that emits adds to what it transmits; a brightfield culture
            // returns an empty emission buffer and this term is zero.
            let emitted = field.emission_at(index) as f64 * vignette;
            let electrons = (transmitted + emitted) * electron_gain;

            let hash = sim_sample::mix3(config.seed, frame_index, index as u64);
            let shot = electrons.max(0.0).sqrt() * sim_sample::normal_deviate(hash);
            let read = config.read_noise_electrons
                * sim_sample::normal_deviate(sim_sample::mix64(hash ^ 0x51ED_2701));
            let level = (electrons + shot + read) / config.full_well_electrons * 255.0;
            let code = level.round().clamp(0.0, 255.0);
            if code >= 255.0 {
                saturated += 1;
            }
            data[index] = code as u8;
        }
    }

    RenderedImage {
        width,
        height,
        data,
        saturated,
    }
}

fn render_frame(
    config: &SimPlateReaderConfig,
    scene: SceneState,
    frame_index: u64,
    camera: DeviceId,
    handle: FrameHandle,
    buffer: FrameBufferSpec,
) -> Frame {
    let image = render_image(config, scene, frame_index);
    let objective = &config.objectives[scene.objective.min(config.objectives.len() - 1)];
    let pixel_size =
        config.pixel_pitch.micrometers() * scene.binning.max(1) as f64 / objective.magnification;
    let focus_offset =
        scene.z_um - sim_sample::sample_plane_um(&config.sample_config(), scene.x_um, scene.y_um);
    Frame {
        handle,
        device: camera,
        width: image.width,
        height: image.height,
        pixel_format: ImageEncoding::Mono8.property_value().into(),
        data: image.data,
        metadata: BTreeMap::from([
            (
                "stage_x".into(),
                Value::Position(Position::from_micrometers(scene.x_um)),
            ),
            (
                "stage_y".into(),
                Value::Position(Position::from_micrometers(scene.y_um)),
            ),
            (
                "stage_z".into(),
                Value::Position(Position::from_micrometers(scene.z_um)),
            ),
            (
                "objective_position".into(),
                Value::I64(scene.objective as i64 + 1),
            ),
            ("magnification".into(), Value::F64(objective.magnification)),
            (
                "numerical_aperture".into(),
                Value::NumericalAperture(NumericalAperture::new(objective.numerical_aperture)),
            ),
            ("pixel_pitch".into(), Value::Position(config.pixel_pitch)),
            (
                "binning".into(),
                Value::String(binning_name(scene.binning).into()),
            ),
            (
                "sample_pixel_size".into(),
                Value::Position(Position::from_micrometers(pixel_size)),
            ),
            (
                "exposure".into(),
                Value::TimeInterval(TimeInterval::from_seconds(scene.exposure_s)),
            ),
            (
                "gain".into(),
                Value::Ratio(Ratio::from_percent(scene.gain_percent)),
            ),
            ("illumination_enabled".into(), Value::Bool(scene.lamp_on)),
            (
                "illumination_power".into(),
                Value::Ratio(Ratio::from_percent(scene.lamp_power_percent)),
            ),
            (
                "focus_offset".into(),
                Value::Position(Position::from_micrometers(focus_offset)),
            ),
            ("frame_index".into(), Value::I64(frame_index as i64)),
            (
                "saturated_pixels".into(),
                Value::PixelCount(PixelCount::new(image.saturated)),
            ),
        ]),
        buffer,
    }
}

fn frame_completion(frame: &Frame) -> Value {
    Value::Map(BTreeMap::from([
        ("stream".into(), Value::I64(frame.handle.stream.0 as i64)),
        ("frame".into(), Value::I64(frame.handle.frame.0 as i64)),
        (
            "width".into(),
            Value::PixelCount(PixelCount::new(frame.width)),
        ),
        (
            "height".into(),
            Value::PixelCount(PixelCount::new(frame.height)),
        ),
        (
            "pixel_format".into(),
            Value::String(frame.pixel_format.clone()),
        ),
        (
            "sample_pixel_size".into(),
            frame
                .metadata
                .get("sample_pixel_size")
                .cloned()
                .unwrap_or(Value::Null),
        ),
    ]))
}

// ---------------------------------------------------------------------------
// Value helpers and property schemas
// ---------------------------------------------------------------------------

fn binning_name(factor: u32) -> &'static str {
    match factor {
        2 => "2x2",
        4 => "4x4",
        _ => "1x1",
    }
}

fn binning_factor(value: &Value) -> Result<u32> {
    let Value::String(mode) = value else {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "binning expects a mode string",
        ));
    };
    match mode.as_str() {
        "1x1" => Ok(1),
        "2x2" => Ok(2),
        "4x4" => Ok(4),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unknown binning mode {other}"),
        )),
    }
}

fn position_um(value: &Value) -> Result<f64> {
    match value {
        Value::Position(position) => Ok(position.micrometers()),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            "expected a position",
        )),
    }
}

fn velocity_um_s(value: &Value) -> Result<f64> {
    match value {
        Value::Velocity(velocity) => Ok(velocity.micrometers_per_second()),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            "expected a velocity",
        )),
    }
}

fn time_seconds(value: &Value) -> Result<f64> {
    match value {
        Value::TimeInterval(interval) => Ok(interval.seconds()),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            "expected a time interval",
        )),
    }
}

fn ratio_percent(value: &Value) -> Result<f64> {
    match value {
        Value::Ratio(ratio) => Ok(ratio.percent()),
        _ => Err(Error::new(ErrorCode::InvalidProperty, "expected a ratio")),
    }
}

fn property(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    unit: Option<&str>,
    writable: bool,
) -> PropertySchema {
    PropertySchema {
        key: key.to_string(),
        display_name: display_name.to_string(),
        value_type,
        unit: unit.map(|unit| Unit(unit.to_string())),
        range: None,
        increment: None,
        enum_values: Vec::new(),
        readable: true,
        writable,
        volatile: false,
        sequenceable: false,
        hardware_address: None,
    }
}

fn volatile_property(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    unit: Option<&str>,
) -> PropertySchema {
    let mut schema = property(key, display_name, value_type, unit, false);
    schema.volatile = true;
    schema
}

fn sequenceable_property(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    unit: Option<&str>,
    writable: bool,
) -> PropertySchema {
    let mut schema = property(key, display_name, value_type, unit, writable);
    schema.sequenceable = writable;
    schema
}

/// A plate-reader stage axis, in *plate* coordinates.
///
/// The origin is the plate's own corner rather than the middle of the stage's travel, so a
/// well centre computed from [`PlateGeometry`] is directly writable. A range centred on
/// zero — which is what a microscope advertises — would reject every well past the middle
/// of the plate while `PlateMove` to the same place succeeded, and a stage that accepts a
/// coordinate one way and refuses it the other is worse than one that simply cannot reach.
fn plate_axis_property(key: &str, display_name: &str, extent_um: f64) -> PropertySchema {
    let mut schema =
        sequenceable_property(key, display_name, ValueType::Position, Some("um"), true);
    schema.range = Some(Range {
        min: Value::Position(Position::from_micrometers(0.0)),
        max: Value::Position(Position::from_micrometers(extent_um)),
    });
    schema.increment = Some(Value::Position(Position::from_micrometers(0.1)));
    schema
}

fn focus_property(key: &str, display_name: &str, travel_um: f64) -> PropertySchema {
    let mut schema =
        sequenceable_property(key, display_name, ValueType::Position, Some("um"), true);
    schema.range = Some(Range {
        min: Value::Position(Position::from_micrometers(0.0)),
        max: Value::Position(Position::from_micrometers(travel_um)),
    });
    schema.increment = Some(Value::Position(Position::from_micrometers(0.05)));
    schema
}

fn speed_property() -> PropertySchema {
    let mut schema = property("speed", "Speed", ValueType::Velocity, Some("um/s"), true);
    schema.range = Some(Range {
        min: Value::Velocity(Velocity::from_micrometers_per_second(1.0)),
        max: Value::Velocity(Velocity::from_micrometers_per_second(20_000.0)),
    });
    schema
}

fn time_property(
    key: &str,
    display_name: &str,
    min: TimeInterval,
    max: TimeInterval,
) -> PropertySchema {
    let mut schema =
        sequenceable_property(key, display_name, ValueType::TimeInterval, Some("s"), true);
    schema.range = Some(Range {
        min: Value::TimeInterval(min),
        max: Value::TimeInterval(max),
    });
    schema.sequenceable = key == "exposure";
    schema
}

/// One detector read, in the instrument's own raw counts.
#[derive(Debug, Clone, Copy)]
struct MeasuredCounts {
    /// Reference-channel counts. Zero for luminescence, which has no reference.
    reference: u64,
    /// Signal-channel counts.
    measurement: u64,
    wavelength_nm: f64,
    integration_s: f64,
}

impl MeasuredCounts {
    /// The completion map a client reads. Raw counts plus the settings they were taken
    /// under — never a calibrated value, because deciding what a count means is the
    /// application's job (NUMANAGER_PLAN.md §4.1).
    fn completion(&self, well: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("well".into(), Value::String(well.into())),
            ("reference".into(), Value::I64(self.reference as i64)),
            ("measurement".into(), Value::I64(self.measurement as i64)),
            (
                "wavelength".into(),
                Value::Wavelength(Wavelength::from_nanometers(self.wavelength_nm)),
            ),
            (
                "integration_time".into(),
                Value::TimeInterval(TimeInterval::from_seconds(self.integration_s)),
            ),
        ]))
    }
}

fn wavelength_nm(value: &Value) -> Result<f64> {
    match value {
        Value::Wavelength(wavelength) => Ok(wavelength.nanometers()),
        Value::F64(nm) => Ok(*nm),
        Value::I64(nm) => Ok(*nm as f64),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("expected a wavelength, got {other:?}"),
        )),
    }
}

fn temperature_celsius(value: &Value) -> Result<f64> {
    match value {
        Value::Temperature(temperature) => Ok(temperature.celsius()),
        Value::F64(celsius) => Ok(*celsius),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("expected a temperature, got {other:?}"),
        )),
    }
}

fn gas_percent(value: &Value) -> Result<f64> {
    match value {
        Value::GasConcentration(gas) => Ok(gas.percent()),
        Value::F64(percent) => Ok(*percent),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("expected a gas concentration, got {other:?}"),
        )),
    }
}

fn bool_value(value: &Value) -> Result<bool> {
    match value {
        Value::Bool(flag) => Ok(*flag),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("expected a boolean, got {other:?}"),
        )),
    }
}

fn wavelength_property(key: &str, display_name: &str, min: f64, max: f64) -> PropertySchema {
    let mut schema =
        sequenceable_property(key, display_name, ValueType::Wavelength, Some("nm"), true);
    schema.range = Some(Range {
        min: Value::Wavelength(Wavelength::from_nanometers(min)),
        max: Value::Wavelength(Wavelength::from_nanometers(max)),
    });
    schema
}

fn temperature_property(key: &str, display_name: &str, min: f64, max: f64) -> PropertySchema {
    let mut schema = sequenceable_property(
        key,
        display_name,
        ValueType::Temperature,
        Some("degC"),
        true,
    );
    schema.range = Some(Range {
        min: Value::Temperature(Temperature::from_celsius(min)),
        max: Value::Temperature(Temperature::from_celsius(max)),
    });
    schema
}

fn gas_property(key: &str, display_name: &str, min: f64, max: f64) -> PropertySchema {
    let mut schema = sequenceable_property(
        key,
        display_name,
        ValueType::GasConcentration,
        Some("percent"),
        true,
    );
    schema.range = Some(Range {
        min: Value::GasConcentration(GasConcentration::from_percent(min)),
        max: Value::GasConcentration(GasConcentration::from_percent(max)),
    });
    schema
}

fn ratio_property(key: &str, display_name: &str, min: f64, max: f64) -> PropertySchema {
    let mut schema =
        sequenceable_property(key, display_name, ValueType::Ratio, Some("percent"), true);
    schema.range = Some(Range {
        min: Value::Ratio(Ratio::from_percent(min)),
        max: Value::Ratio(Ratio::from_percent(max)),
    });
    schema
}

fn enum_property(key: &str, display_name: &str, values: &[&str]) -> PropertySchema {
    let mut schema = property(key, display_name, ValueType::String, None, true);
    schema.enum_values = values
        .iter()
        .map(|value| EnumValue {
            value: Value::String((*value).to_string()),
            label: (*value).to_string(),
        })
        .collect();
    schema
}

/// Turret selection. Not sequenceable: the rotation is mechanical and takes
/// longer than a frame, so a timing plan could not honour one value per step.
fn objective_position_property(values: Vec<EnumValue>, count: i64) -> PropertySchema {
    let mut schema = property("position", "Objective", ValueType::I64, None, true);
    schema.range = Some(Range {
        min: Value::I64(1),
        max: Value::I64(count),
    });
    schema.enum_values = values;
    schema.volatile = true;
    schema
}

fn capability(id: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    CapabilityDescriptor::new(CapabilityId(id), device, kind, ValueType::Map)
}
