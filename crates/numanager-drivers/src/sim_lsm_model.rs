//! Confocal/LSM sampling helpers over the shared simulator specimen.
//!
//! This module is not a hardware driver and does not publish runtime operations.
//! It converts the shared cell-culture model into synthetic fluorescence detector
//! data that a future `sim_lsm` driver can expose through public LSM APIs.

use crate::sim_sample::{self, SimCell, SimSampleConfig};

const EXACT_POISSON_LIMIT_PHOTONS: f64 = 64.0;

#[derive(Debug, Clone, Copy)]
pub struct LsmFluorescenceConfig {
    pub sample: SimSampleConfig,
    pub psf_xy_um: f64,
    pub psf_z_um: f64,
    pub background: f64,
    pub cytoplasm_gain: f64,
    pub nucleus_gain: f64,
    pub photon_scale: f64,
    pub read_noise: f64,
    pub detector_gain: f64,
    pub detector_noise: f64,
    pub pinhole_airy_units: f64,
}

impl Default for LsmFluorescenceConfig {
    fn default() -> Self {
        Self {
            sample: SimSampleConfig {
                seed: 0x5EED_0C11_A73E_0001,
                focal_plane_um: 4_250.0,
                tilt_um_per_mm: (2.0, -1.4),
                cells_per_tile: (4, 9),
            },
            psf_xy_um: 0.42,
            psf_z_um: 1.2,
            background: 0.018,
            cytoplasm_gain: 0.7,
            nucleus_gain: 1.4,
            photon_scale: 9_000.0,
            read_noise: 3.0,
            detector_gain: 1.0,
            detector_noise: 1.0,
            pinhole_airy_units: 1.2,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LsmRasterSpec {
    pub center_x_um: f64,
    pub center_y_um: f64,
    pub z_um: f64,
    pub width: u32,
    pub height: u32,
    pub pixel_size_um: f64,
    pub laser_power: f64,
    pub numerical_aperture: f64,
    pub magnification: f64,
}

#[derive(Debug, Clone)]
pub struct LsmImage {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u16>,
    pub saturated: u32,
}

pub fn render_confocal_raster(
    config: &LsmFluorescenceConfig,
    spec: LsmRasterSpec,
    frame_index: u64,
) -> LsmImage {
    render_confocal_raster_for_detectors(config, spec, &[], frame_index)
}

pub fn render_confocal_raster_for_detectors(
    config: &LsmFluorescenceConfig,
    spec: LsmRasterSpec,
    detectors: &[String],
    frame_index: u64,
) -> LsmImage {
    let geometry = ScanGeometry::new(config, &spec);
    let cells = geometry.cells(config);
    let width = geometry.width;
    let height = geometry.height;

    let mut data = Vec::with_capacity((width * height) as usize);
    let mut saturated = 0;
    for row in 0..height {
        geometry.render_row(
            config,
            &spec,
            &cells,
            detectors,
            width,
            row,
            frame_index,
            &mut data,
        );
    }
    for code in &data {
        if *code == u16::MAX {
            saturated += 1;
        }
    }
    LsmImage {
        width,
        height,
        data,
        saturated,
    }
}

pub fn render_line_profile(
    config: &LsmFluorescenceConfig,
    spec: LsmRasterSpec,
    samples: u32,
    frame_index: u64,
) -> Vec<u16> {
    render_line_profile_for_detector(config, spec, "counter0", samples, frame_index)
}

/// Detector traces for one row of the raster described by `spec`.
///
/// This is the single scan primitive: [`render_confocal_raster_for_detectors`]
/// builds its image out of exactly these rows, so scanning row `row` here
/// reproduces row `row` of the image — same sample positions, same optics, same
/// detector response, same quantisation. A `spec` of height 1 puts row 0 at the
/// scan centre, which is what a single-line request asks for.
pub fn render_scan_row_profiles(
    config: &LsmFluorescenceConfig,
    spec: LsmRasterSpec,
    detectors: &[String],
    samples: u32,
    row: u32,
    frame_index: u64,
) -> Vec<(String, Vec<u16>)> {
    let geometry = ScanGeometry::new(config, &spec);
    let cells = geometry.cells(config);
    let samples = samples.max(1);

    let channels: Vec<String> = if detectors.is_empty() {
        vec!["counter0".into()]
    } else {
        detectors.to_vec()
    };

    channels
        .into_iter()
        .map(|detector| {
            let mut codes = Vec::with_capacity(samples as usize);
            // One detector at a time: `detector_signal` over a single-channel
            // list is the same expression the raster averages over.
            geometry.render_row(
                config,
                &spec,
                &cells,
                std::slice::from_ref(&detector),
                samples,
                row,
                frame_index,
                &mut codes,
            );
            (detector, codes)
        })
        .collect()
}

pub fn render_line_profile_for_detector(
    config: &LsmFluorescenceConfig,
    spec: LsmRasterSpec,
    detector: &str,
    samples: u32,
    frame_index: u64,
) -> Vec<u16> {
    render_scan_row_profiles(
        config,
        spec,
        std::slice::from_ref(&detector.to_owned()),
        samples,
        0,
        frame_index,
    )
    .into_iter()
    .next()
    .map(|(_, codes)| codes)
    .unwrap_or_default()
}

/// Sample positions and optics for a scan, derived once and shared by every row
/// so the raster and a line scan cannot drift apart.
struct ScanGeometry {
    width: u32,
    height: u32,
    pixel_um: f64,
    origin_x_um: f64,
    origin_y_um: f64,
    max_x_um: f64,
    max_y_um: f64,
    optics: EffectiveOptics,
}

impl ScanGeometry {
    fn new(config: &LsmFluorescenceConfig, spec: &LsmRasterSpec) -> Self {
        let width = spec.width.max(1);
        let height = spec.height.max(1);
        let pixel_um = spec.pixel_size_um.max(0.001);
        let half_w_um = width as f64 * pixel_um / 2.0;
        let half_h_um = height as f64 * pixel_um / 2.0;
        Self {
            width,
            height,
            pixel_um,
            origin_x_um: spec.center_x_um - half_w_um,
            origin_y_um: spec.center_y_um - half_h_um,
            max_x_um: spec.center_x_um + half_w_um,
            max_y_um: spec.center_y_um + half_h_um,
            optics: EffectiveOptics::new(config, spec),
        }
    }

    /// Cells for the whole scan rectangle. A single row uses the same set as the
    /// full frame so a line scan and the image see identical sample content.
    fn cells(&self, config: &LsmFluorescenceConfig) -> Vec<SimCell> {
        let margin_um =
            sim_sample::MAX_CELL_RADIUS_UM + 4.0 * self.optics.psf_xy_um.max(self.pixel_um);
        sim_sample::cells_for_rect(
            &config.sample,
            self.origin_x_um,
            self.origin_y_um,
            self.max_x_um,
            self.max_y_um,
            margin_um,
        )
    }

    /// Pixel-centre sampling across the scan width. With `samples == width` this
    /// is exactly the raster's pixel grid.
    fn x_um(&self, column: u32, samples: u32) -> f64 {
        let step = self.width as f64 * self.pixel_um / f64::from(samples.max(1));
        self.origin_x_um + (f64::from(column) + 0.5) * step
    }

    fn y_um(&self, row: u32) -> f64 {
        self.origin_y_um + (f64::from(row) + 0.5) * self.pixel_um
    }

    #[allow(clippy::too_many_arguments)]
    fn render_row(
        &self,
        config: &LsmFluorescenceConfig,
        spec: &LsmRasterSpec,
        cells: &[SimCell],
        detectors: &[String],
        samples: u32,
        row: u32,
        frame_index: u64,
        out: &mut Vec<u16>,
    ) {
        let y_um = self.y_um(row);
        for column in 0..samples {
            let components = fluorescence_components_at_cells(
                config,
                self.optics,
                cells,
                self.x_um(column, samples),
                y_um,
                spec.z_um,
            );
            let signal =
                detector_signal(config, components, detectors, spec.laser_power, self.optics);
            // Noise is seeded by the position in the frame, so a row scanned on
            // its own carries the same realisation as that row of the image.
            let sample_index = u64::from(row) * u64::from(samples) + u64::from(column);
            out.push(detector_code(config, signal, frame_index, sample_index));
        }
    }
}

pub fn fluorescence_at(config: &LsmFluorescenceConfig, x_um: f64, y_um: f64, z_um: f64) -> f64 {
    let optics = EffectiveOptics::default_for(config);
    let margin_um = sim_sample::MAX_CELL_RADIUS_UM + 4.0 * optics.psf_xy_um;
    let cells = sim_sample::cells_for_rect(&config.sample, x_um, y_um, x_um, y_um, margin_um);
    let components = fluorescence_components_at_cells(config, optics, &cells, x_um, y_um, z_um);
    DetectorResponse::default().apply(config, components)
}

#[derive(Debug, Clone, Copy)]
struct FluorescenceComponents {
    background: f64,
    cytoplasm: f64,
    nucleus: f64,
}

#[derive(Debug, Clone, Copy)]
struct DetectorResponse {
    background: f64,
    cytoplasm: f64,
    nucleus: f64,
    gain: f64,
    dark_offset: f64,
}

impl Default for DetectorResponse {
    fn default() -> Self {
        Self {
            background: 1.0,
            cytoplasm: 1.0,
            nucleus: 1.0,
            gain: 1.0,
            dark_offset: 0.0,
        }
    }
}

impl DetectorResponse {
    fn for_channel(channel: &str) -> Self {
        let channel = channel.to_ascii_lowercase();
        if channel.contains("nucleus") || channel.contains("dapi") || channel.contains("405") {
            Self {
                background: 0.8,
                cytoplasm: 0.18,
                nucleus: 1.7,
                gain: 1.05,
                dark_offset: 0.002,
            }
        } else if channel.contains("cyto")
            || channel.contains("fitc")
            || channel.contains("488")
            || channel.contains("green")
        {
            Self {
                background: 0.9,
                cytoplasm: 1.35,
                nucleus: 0.35,
                gain: 0.95,
                dark_offset: 0.003,
            }
        } else if channel.contains("background") || channel.contains("dark") {
            Self {
                background: 1.4,
                cytoplasm: 0.08,
                nucleus: 0.04,
                gain: 0.45,
                dark_offset: 0.012,
            }
        } else if channel.starts_with("ai") {
            Self {
                background: 1.05,
                cytoplasm: 0.85,
                nucleus: 1.15,
                gain: 0.78,
                dark_offset: 0.006,
            }
        } else {
            Self::default()
        }
    }

    fn apply(self, config: &LsmFluorescenceConfig, components: FluorescenceComponents) -> f64 {
        self.dark_offset
            + self.gain
                * (self.background * components.background
                    + self.cytoplasm * config.cytoplasm_gain * components.cytoplasm
                    + self.nucleus * config.nucleus_gain * components.nucleus)
    }
}

fn detector_signal(
    config: &LsmFluorescenceConfig,
    components: FluorescenceComponents,
    detectors: &[String],
    laser_power: f64,
    optics: EffectiveOptics,
) -> f64 {
    let active_power = laser_power.clamp(0.0, 1.0);
    let signal = if detectors.is_empty() {
        DetectorResponse::default().apply(config, components)
    } else {
        let accumulated = detectors
            .iter()
            .map(|detector| DetectorResponse::for_channel(detector).apply(config, components))
            .sum::<f64>();
        accumulated / detectors.len().max(1) as f64
    };
    signal * active_power * optics.collection_gain * config.detector_gain.max(0.0)
}

fn fluorescence_components_at_cells(
    config: &LsmFluorescenceConfig,
    optics: EffectiveOptics,
    cells: &[SimCell],
    x_um: f64,
    y_um: f64,
    z_um: f64,
) -> FluorescenceComponents {
    let mut components = FluorescenceComponents {
        background: config.background.max(0.0),
        cytoplasm: 0.0,
        nucleus: 0.0,
    };
    let psf_xy = optics.psf_xy_um;
    let psf_z = optics.psf_z_um;
    let psf_xy2 = psf_xy * psf_xy;
    for cell in cells {
        let dz_um = z_um - cell.z_um;
        let dz = dz_um / psf_z;
        let excitation_z = (-0.5 * dz * dz).exp();
        let pinhole_z = optics.pinhole_z_um.max(f64::EPSILON);
        let pinhole_rejection = 1.0 / (1.0 + (dz_um.abs() / pinhole_z).powi(4));
        let z_weight = excitation_z * pinhole_rejection;
        if z_weight < 0.0001 {
            continue;
        }

        let dx = x_um - cell.cx_um;
        let dy = y_um - cell.cy_um;
        let u = (dx * cell.cos_t + dy * cell.sin_t) / (cell.a_um + psf_xy);
        let v = (-dx * cell.sin_t + dy * cell.cos_t) / (cell.b_um + psf_xy);
        let q = u * u + v * v;
        if q < 9.0 {
            let cell_body = (-0.5 * q).exp();
            let edge = ((1.0 - q.sqrt()).abs() / 0.22).powi(2);
            components.cytoplasm +=
                z_weight * cell.density * (0.35 * cell_body + 0.65 * (-0.5 * edge).exp());
        }

        if cell.nucleus_r_um > 0.0 {
            let nx = cell.cx_um + cell.nucleus_dx;
            let ny = cell.cy_um + cell.nucleus_dy;
            let nr = (cell.nucleus_r_um * cell.nucleus_r_um + psf_xy2).sqrt();
            let nq = ((x_um - nx).powi(2) + (y_um - ny).powi(2)) / (nr * nr);
            if nq < 9.0 {
                components.nucleus += z_weight * cell.nucleus_density * (-0.5 * nq).exp();
            }
        }
    }
    components
}

#[derive(Debug, Clone, Copy)]
struct EffectiveOptics {
    psf_xy_um: f64,
    psf_z_um: f64,
    pinhole_z_um: f64,
    collection_gain: f64,
}

impl EffectiveOptics {
    fn default_for(config: &LsmFluorescenceConfig) -> Self {
        Self {
            psf_xy_um: config.psf_xy_um.max(0.001),
            psf_z_um: config.psf_z_um.max(0.001),
            pinhole_z_um: (config.psf_z_um * config.pinhole_airy_units).max(0.001),
            collection_gain: 1.0,
        }
    }

    fn new(config: &LsmFluorescenceConfig, spec: &LsmRasterSpec) -> Self {
        let base = Self::default_for(config);
        let na = spec.numerical_aperture;
        if !na.is_finite() || na <= 0.0 {
            return base;
        }

        let reference_na = 0.45;
        let na_scale = (reference_na / na.clamp(0.05, 1.45)).clamp(0.25, 4.0);
        let magnification_scale = if spec.magnification.is_finite() && spec.magnification > 0.0 {
            (20.0 / spec.magnification).sqrt().clamp(0.5, 2.0)
        } else {
            1.0
        };
        let psf_xy_um = (base.psf_xy_um * na_scale.sqrt() * magnification_scale).max(0.04);
        let psf_z_um = (base.psf_z_um * na_scale * na_scale).max(0.08);
        Self {
            psf_xy_um,
            psf_z_um,
            pinhole_z_um: (psf_z_um * config.pinhole_airy_units.max(0.1)).max(0.02),
            collection_gain: (na / reference_na).powi(2).clamp(0.1, 8.0),
        }
    }
}

fn detector_code(
    config: &LsmFluorescenceConfig,
    signal: f64,
    frame_index: u64,
    sample_index: u64,
) -> u16 {
    let photons = signal.max(0.0) * config.photon_scale.max(1.0);
    let hash = sim_sample::mix3(config.sample.seed, frame_index, sample_index);
    let detected = poisson_photons(photons, hash);
    let read = config.read_noise.max(0.0)
        * config.detector_noise.max(0.0)
        * sim_sample::normal_deviate(sim_sample::mix64(hash ^ 0x1A5E_2D17));
    ((detected + read) / config.photon_scale.max(1.0) * f64::from(u16::MAX))
        .round()
        .clamp(0.0, f64::from(u16::MAX)) as u16
}

fn poisson_photons(lambda: f64, hash: u64) -> f64 {
    if !lambda.is_finite() || lambda <= 0.0 {
        return 0.0;
    }
    if lambda > EXACT_POISSON_LIMIT_PHOTONS {
        return (lambda + lambda.sqrt() * sim_sample::normal_deviate(hash)).max(0.0);
    }

    let threshold = (-lambda).exp();
    let mut product = 1.0;
    let mut count = 0u32;
    let mut state = hash;
    loop {
        state = sim_sample::mix64(state);
        product *= sim_sample::unit01(state).max(f64::MIN_POSITIVE);
        if product <= threshold {
            break;
        }
        count += 1;
    }
    f64::from(count)
}
