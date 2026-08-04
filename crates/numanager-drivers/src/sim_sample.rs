//! Shared deterministic specimen model for simulator drivers.
//!
//! The model is defined in world coordinates and has no dependency on cameras,
//! DAQ tasks, GUI state, frame handles, or runtime operations. Modality-specific
//! renderers can query the same seeded cell culture for brightfield, fluorescence,
//! and scanning simulations.
//!
//! [`SimSampleConfig`] and the free functions below are that one built-in model, and
//! they are unchanged. [`Specimen`] generalizes them: a simulator device owns optics,
//! motion and detector behaviour, while a specimen owns what is *there* to be observed,
//! so one simulated instrument can look at different biology without knowing which.

/// Sample tile edge in micrometres. Cells are generated per tile from a hash of
/// the tile index, so the culture extends without limit in XY and nothing is
/// retained between frames.
pub const TILE_UM: f64 = 128.0;
pub const MAX_CELL_RADIUS_UM: f64 = 12.0;
const CELL_Z_SPREAD_UM: f64 = 3.0;

/// Tiles gathered per axis for one query, so an unusual field of view keeps a
/// fixed maximum amount of sample work.
pub const TILE_SPAN_LIMIT: i64 = 24;

/// Radial profile sharpness of the cell interior.
pub const CELL_EDGE_K: f64 = 2.2;
/// Share of a cell's absorbance carried by its membrane rim rather than its interior.
/// Unstained cells in transmitted light read mostly as an outline.
pub const CELL_RIM_SHARE: f64 = 0.72;
/// Rim thickness as a fraction of the cell radius, before defocus widens it.
pub const CELL_RIM_WIDTH: f64 = 0.13;

#[derive(Debug, Clone, Copy)]
pub struct SimSampleConfig {
    /// Seed for the cell-culture model. The same seed always yields the same
    /// sample, so recorded output and screenshots are reproducible.
    pub seed: u64,
    /// Height of the culture surface at the origin.
    pub focal_plane_um: f64,
    /// Culture surface tilt, in micrometres of height per millimetre of travel.
    pub tilt_um_per_mm: (f64, f64),
    pub cells_per_tile: (u32, u32),
}

/// One cell of the culture, in world micrometres.
#[derive(Debug, Clone, Copy)]
pub struct SimCell {
    pub cx_um: f64,
    pub cy_um: f64,
    pub a_um: f64,
    pub b_um: f64,
    pub cos_t: f64,
    pub sin_t: f64,
    pub z_um: f64,
    pub density: f64,
    pub nucleus_dx: f64,
    pub nucleus_dy: f64,
    pub nucleus_r_um: f64,
    pub nucleus_density: f64,
}

/// Something a simulator device can observe.
///
/// The device half of a simulation knows about optics, motion, exposure and noise; it
/// does not know what it is pointed at. This trait is that boundary, so a brightfield
/// microscope, a confocal scanner and a plate reader can share one device implementation
/// while a downstream crate supplies its own biology — a plain culture, a multiwell plate
/// with per-well contents, a calibration target.
///
/// All coordinates are world micrometres, the same frame [`cells_for_rect`] uses.
///
/// Implementations **must be deterministic**: the same query at the same observation time
/// must give the same answer, so recorded output, screenshots and tests reproduce. Derive
/// variation from a hash of the coordinates ([`mix3`], [`unit01`]) rather than from a
/// random-number generator.
///
/// # This shape is transitional
///
/// A specimen is, physically, **a source and absorber of photons**. A device should ask it
/// how much light leaves a region in a band, or how much is attenuated along a path — not
/// for a list of cells. [`SimCell`] is one specimen's internal model, and having it in the
/// signature means every implementation has to describe itself in cells whether or not
/// that fits: a bacterial suspension, a dye titration, a fluorescent bead slide and an
/// empty well are all badly served by it.
///
/// The cell-shaped methods below exist because the two current renderers
/// (`sim_microscope`, `sim_lsm_model`) draw cells directly, and changing that is a larger
/// refactor of working drivers. The intended destination is a radiometric interface —
/// emission per band and attenuation along a path, over a stated footprint at a point in
/// space and time. Treat the current methods as the compatibility layer they are, and do
/// not build new callers that assume every specimen is a cell culture.
///
/// # A specimen rasterizes expected photons; the device makes the image
///
/// A specimen holds *structure* — it knows there are cells, where they are and what they
/// are doing — and renders it onto the grid a caller asks for, producing a field of
/// **mean photon counts per pixel footprint**: real-valued, noise-free, deterministic.
/// The device turns that into an actual frame: exposure, shot noise, read noise, gain,
/// saturation, quantization to 8 or 16 bits.
///
/// The split is at expectation because that is where the physics divides. The mean is a
/// property of the scene and the optics; the scatter around it is a property of the
/// detector. Putting noise in the specimen would break two things that matter: averaging
/// repeated frames would no longer converge on the true scene, and two detectors observing
/// the same field would see *identical* noise instead of independent noise.
///
/// A query therefore carries the optical state it should be rendered through — band,
/// defocus, point-spread width — because the specimen must project its structure through
/// the optics to produce that field, while not owning the optics. Rendering a cell's
/// profile widened by a supplied PSF is what `sim_microscope` already does; the change is
/// where the code lives, not what it computes.
///
/// Implementations should generate structure procedurally from position hashes rather than
/// retaining it, exactly as the built-in model does, so memory does not scale with the area
/// observed. The payoff is that cost tracks the *output raster* rather than the region
/// covered: a whole 128 × 85 mm plate rendered at 2000 × 1300 is no more work than one
/// camera field — provided a query whose footprint is much larger than a cell can be
/// answered without enumerating individuals. The cell-shaped methods below cannot do that,
/// which is the concrete reason the radiometric interface is the destination rather than a
/// nicety.
pub trait Specimen: Send + Sync {
    /// Render the specimen onto the grid `request` describes.
    ///
    /// This is the method devices should call. The default implementation enumerates cells
    /// and splats them, which is correct for any cell culture but costs time proportional
    /// to the *area* covered; a specimen that can answer a coarse footprint analytically —
    /// mean confluence over a whole well, say — should override it and stay proportional to
    /// the output raster instead.
    fn render_field(&self, request: &FieldRequest) -> SpecimenField {
        render_cell_field(self, request)
    }

    /// Cells overlapping the rectangle, plus `margin_um` beyond every edge — a cell whose
    /// centre lies outside the field still contributes the part of its profile that
    /// reaches inside, and omitting it would make objects pop in at the edge.
    ///
    /// `time_s` is how far into the experiment the observation happens. A static specimen
    /// ignores it; one that models growth, motility or bleaching does not.
    ///
    /// A specimen that is not made of discrete cells may return an empty vector, provided
    /// it overrides [`Specimen::render_field`] — otherwise it renders as nothing.
    fn cells_for_rect(
        &self,
        min_x_um: f64,
        min_y_um: f64,
        max_x_um: f64,
        max_y_um: f64,
        margin_um: f64,
        time_s: f64,
    ) -> Vec<SimCell>;

    /// Height of the specimen surface under a point. What "in focus" means at `(x, y)`.
    fn surface_plane_um(&self, x_um: f64, y_um: f64) -> f64;

    /// Largest radius any cell may have.
    ///
    /// Callers size their query margin from this. A specimen with larger cells than the
    /// built-in model must say so, or its cells will be clipped at the edge of a field
    /// instead of half-drawn.
    fn max_cell_radius_um(&self) -> f64 {
        MAX_CELL_RADIUS_UM
    }
}

/// The grid a device wants the specimen rendered onto, and the optics to render it through.
///
/// `origin_*` is the outer corner of pixel `(0, 0)`, so pixel `(x, y)` covers
/// `origin + (x, y) * pixel_um` to `origin + (x+1, y+1) * pixel_um`. `pixel_um` is the
/// footprint in *sample* micrometres, which is what makes the same specimen serve a whole
/// plate and a single field: only this number changes.
#[derive(Debug, Clone, Copy)]
pub struct FieldRequest {
    pub origin_x_um: f64,
    pub origin_y_um: f64,
    pub pixel_um: f64,
    pub width: u32,
    pub height: u32,
    /// Height of the focal plane. Distance from a feature's own height sets its blur.
    pub focus_z_um: f64,
    /// Objective NA — sets both the defocus cone and the diffraction limit.
    pub numerical_aperture: f64,
    /// Observation band centre — what the detector collects. Blur scales with it, and a
    /// specimen with spectral content uses it to decide what absorbs or emits.
    pub wavelength_um: f64,
    /// Excitation band, when the specimen is being illuminated to make it emit.
    ///
    /// `None` means no excitation at all, which is not the same as "excitation we did not
    /// specify": a luminescent reaction emits in the dark, and a specimen asked with `None`
    /// must not report fluorescence it would only show under a lamp. Brightfield leaves
    /// this `None` too and reads `optical_depth` instead.
    pub excitation_um: Option<f64>,
    /// Cap on blur radius in pixels. A cost guard: a feature tens of micrometres out of
    /// focus spreads over an unbounded area otherwise. The device owns the policy because
    /// the device knows what it can afford to draw.
    pub blur_limit_px: f64,
    /// How far into the experiment this observation happens.
    pub time_s: f64,
}

impl FieldRequest {
    /// Region covered, as `(min_x, min_y, max_x, max_y)` in sample micrometres.
    pub fn extent_um(&self) -> (f64, f64, f64, f64) {
        (
            self.origin_x_um,
            self.origin_y_um,
            self.origin_x_um + self.width as f64 * self.pixel_um,
            self.origin_y_um + self.height as f64 * self.pixel_um,
        )
    }

    pub fn pixel_count(&self) -> usize {
        self.width as usize * self.height as usize
    }
}

/// What a specimen returns: the light it takes away, and the light it gives off.
///
/// Both are **means, free of detector noise** — the scatter around them belongs to whatever
/// is looking (see the trait docs). Both are per pixel of the requested grid.
#[derive(Debug, Clone)]
pub struct SpecimenField {
    pub width: u32,
    pub height: u32,
    /// Dimensionless optical depth `τ`. A device computing transmitted light uses
    /// `exp(-τ)`; zero means perfectly clear. This is what brightfield reads.
    pub optical_depth: Vec<f32>,
    /// Mean photons emitted into the pixel, in the device's own relative units — what
    /// fluorescence and luminescence read. Empty when the specimen does not emit, which
    /// callers must treat as "all zero" rather than as an error.
    pub emission: Vec<f32>,
}

impl SpecimenField {
    /// A field of the requested size that neither absorbs nor emits.
    pub fn empty(request: &FieldRequest) -> Self {
        Self {
            width: request.width,
            height: request.height,
            optical_depth: vec![0.0; request.pixel_count()],
            emission: Vec::new(),
        }
    }

    /// Emission at a pixel, treating an empty emission buffer as zero.
    pub fn emission_at(&self, index: usize) -> f32 {
        self.emission.get(index).copied().unwrap_or(0.0)
    }

    /// Optical depth at a pixel.
    pub fn optical_depth_at(&self, index: usize) -> f32 {
        self.optical_depth.get(index).copied().unwrap_or(0.0)
    }
}

/// The built-in seeded cell culture as a [`Specimen`]. Ignores `time_s`: the model is
/// static, and a culture that does not grow is the honest answer for it.
impl Specimen for SimSampleConfig {
    fn cells_for_rect(
        &self,
        min_x_um: f64,
        min_y_um: f64,
        max_x_um: f64,
        max_y_um: f64,
        margin_um: f64,
        _time_s: f64,
    ) -> Vec<SimCell> {
        cells_for_rect(self, min_x_um, min_y_um, max_x_um, max_y_um, margin_um)
    }

    fn surface_plane_um(&self, x_um: f64, y_um: f64) -> f64 {
        sample_plane_um(self, x_um, y_um)
    }
}

/// Standard deviation of the defocus profile, in micrometres, combining the geometric cone
/// with the diffraction limit. Depth of field therefore falls out of the numerical aperture
/// instead of being a separate constant.
pub fn defocus_sigma_um(delta_z_um: f64, numerical_aperture: f64, wavelength_um: f64) -> f64 {
    let aperture = numerical_aperture.max(0.01);
    let geometric = delta_z_um.abs() * aperture;
    let diffraction = 0.61 * wavelength_um / aperture;
    0.42 * (geometric * geometric + diffraction * diffraction).sqrt()
}

/// Render a cell-based specimen onto a grid — the default behaviour of
/// [`Specimen::render_field`], and what every current renderer does inline today.
///
/// Cells absorb, so this fills `optical_depth` and leaves `emission` empty. A specimen
/// whose cells fluoresce should override `render_field` and fill both.
pub fn render_cell_field<S: Specimen + ?Sized>(
    specimen: &S,
    request: &FieldRequest,
) -> SpecimenField {
    let mut field = SpecimenField::empty(request);
    if request.pixel_um <= 0.0 || request.pixel_count() == 0 {
        return field;
    }

    let (min_x, min_y, max_x, max_y) = request.extent_um();
    // Reach far enough outside the field that a defocused cell just beyond the edge still
    // contributes the tail that falls inside it.
    let margin_um = specimen.max_cell_radius_um() + 3.0 * request.blur_limit_px * request.pixel_um;
    let cells = specimen.cells_for_rect(min_x, min_y, max_x, max_y, margin_um, request.time_s);

    for cell in &cells {
        let sigma_px = (defocus_sigma_um(
            cell.z_um - request.focus_z_um,
            request.numerical_aperture,
            request.wavelength_um,
        ) / request.pixel_um)
            .min(request.blur_limit_px);
        let center_x = (cell.cx_um - request.origin_x_um) / request.pixel_um;
        let center_y = (cell.cy_um - request.origin_y_um) / request.pixel_um;
        splat(
            &mut field.optical_depth,
            request.width,
            request.height,
            center_x,
            center_y,
            cell.a_um / request.pixel_um,
            cell.b_um / request.pixel_um,
            sigma_px,
            cell.cos_t,
            cell.sin_t,
            cell.density,
            true,
        );
        if cell.nucleus_r_um > 0.0 {
            splat(
                &mut field.optical_depth,
                request.width,
                request.height,
                center_x + cell.nucleus_dx / request.pixel_um,
                center_y + cell.nucleus_dy / request.pixel_um,
                cell.nucleus_r_um / request.pixel_um,
                cell.nucleus_r_um / request.pixel_um,
                sigma_px,
                1.0,
                0.0,
                cell.nucleus_density,
                false,
            );
        }
    }
    field
}

/// Adds one blurred elliptical Gaussian. Defocus widens the profile and lowers its peak so
/// total absorbance is conserved, which is exact for a Gaussian and avoids a separate
/// convolution pass.
#[allow(clippy::too_many_arguments)]
pub fn splat(
    target: &mut [f32],
    width: u32,
    height: u32,
    center_x: f64,
    center_y: f64,
    a_px: f64,
    b_px: f64,
    sigma_px: f64,
    cos_t: f64,
    sin_t: f64,
    density: f64,
    rim: bool,
) {
    let a2 = (a_px * a_px + sigma_px * sigma_px).sqrt();
    let b2 = (b_px * b_px + sigma_px * sigma_px).sqrt();
    if a2 <= 0.0 || b2 <= 0.0 {
        return;
    }
    let peak = density * (a_px * b_px) / (a2 * b2);
    if peak < 0.001 {
        return;
    }
    // Defocus widens the rim in the same normalized frame as the interior, so a cell far
    // from focus loses its outline and reads as one soft blob.
    let rim_width = CELL_RIM_WIDTH + 0.9 * (sigma_px / a2).min(1.0);
    let rim_share = if rim { CELL_RIM_SHARE } else { 0.0 };
    let reach = 3.0 * a2.max(b2);
    let first_x = (center_x - reach).floor().max(0.0) as u32;
    let first_y = (center_y - reach).floor().max(0.0) as u32;
    let last_x = ((center_x + reach).ceil().max(0.0) as u32).min(width.saturating_sub(1));
    let last_y = ((center_y + reach).ceil().max(0.0) as u32).min(height.saturating_sub(1));
    if first_x > last_x || first_y > last_y {
        return;
    }
    for y in first_y..=last_y {
        let dy = y as f64 + 0.5 - center_y;
        for x in first_x..=last_x {
            let dx = x as f64 + 0.5 - center_x;
            let u = (dx * cos_t + dy * sin_t) / a2;
            let v = (-dx * sin_t + dy * cos_t) / b2;
            let q = u * u + v * v;
            if q > 9.0 {
                continue;
            }
            let interior = (-0.5 * q * CELL_EDGE_K).exp();
            let contribution = if rim_share > 0.0 {
                let offset = (q.sqrt() - 1.0) / rim_width;
                peak * ((1.0 - rim_share) * interior
                    + rim_share * 2.4 * (-0.5 * offset * offset).exp())
            } else {
                peak * interior
            };
            target[(y * width + x) as usize] += contribution as f32;
        }
    }
}

pub fn sample_plane_um(config: &SimSampleConfig, x_um: f64, y_um: f64) -> f64 {
    config.focal_plane_um
        + config.tilt_um_per_mm.0 * x_um / 1_000.0
        + config.tilt_um_per_mm.1 * y_um / 1_000.0
}

pub fn cells_for_rect(
    config: &SimSampleConfig,
    min_x_um: f64,
    min_y_um: f64,
    max_x_um: f64,
    max_y_um: f64,
    margin_um: f64,
) -> Vec<SimCell> {
    let first_x = ((min_x_um - margin_um) / TILE_UM).floor() as i64;
    let first_y = ((min_y_um - margin_um) / TILE_UM).floor() as i64;
    let last_x = ((max_x_um + margin_um) / TILE_UM).floor() as i64;
    let last_y = ((max_y_um + margin_um) / TILE_UM).floor() as i64;
    let last_x = last_x.min(first_x + TILE_SPAN_LIMIT);
    let last_y = last_y.min(first_y + TILE_SPAN_LIMIT);

    let mut cells = Vec::new();
    for tile_y in first_y..=last_y {
        for tile_x in first_x..=last_x {
            tile_cells(config, tile_x, tile_y, &mut cells);
        }
    }
    cells
}

/// SplitMix64 finalizer. The sample is addressed by hashing tile and cell
/// indices, so a limitless culture needs no storage and no random-number
/// generator dependency.
pub fn mix64(value: u64) -> u64 {
    let mut z = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

pub fn mix3(a: u64, b: u64, c: u64) -> u64 {
    mix64(mix64(a ^ b.wrapping_mul(0x9E37_79B9_7F4A_7C15)) ^ c.wrapping_mul(0xC2B2_AE3D_27D4_EB4F))
}

pub fn unit01(hash: u64) -> f64 {
    (hash >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
}

/// Approximately normal deviate from one hash, by summing three uniforms.
pub fn normal_deviate(hash: u64) -> f64 {
    let a = unit01(hash);
    let b = unit01(hash.rotate_left(21));
    let c = unit01(hash.rotate_left(42));
    (a + b + c - 1.5) * 2.0
}

fn tile_cells(config: &SimSampleConfig, tile_x: i64, tile_y: i64, out: &mut Vec<SimCell>) {
    let seed = mix3(config.seed, tile_x as u64, tile_y as u64);
    let low = config.cells_per_tile.0.max(1);
    let high = config.cells_per_tile.1.max(low);
    let count = low + ((seed >> 7) % u64::from(high - low + 1)) as u32;
    for index in 0..count {
        let cell = mix64(seed ^ u64::from(index).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let h = |salt: u64| unit01(mix64(cell.wrapping_add(salt)));
        let cx_um = (tile_x as f64 + h(1)) * TILE_UM;
        let cy_um = (tile_y as f64 + h(2)) * TILE_UM;
        let radius = 6.0 + (MAX_CELL_RADIUS_UM - 6.0) * h(3);
        let elongation = 1.0 + 0.8 * h(4);
        let theta = h(5) * std::f64::consts::PI;
        let rounded = h(10) < 0.08;
        let (a_um, b_um) = if rounded {
            (0.55 * radius, 0.55 * radius)
        } else {
            (radius * elongation.sqrt(), radius / elongation.sqrt())
        };
        let density = (0.22 + 0.30 * h(7)) * if rounded { 2.2 } else { 1.0 };
        out.push(SimCell {
            cx_um,
            cy_um,
            a_um,
            b_um,
            cos_t: theta.cos(),
            sin_t: theta.sin(),
            z_um: sample_plane_um(config, cx_um, cy_um) + (h(6) - 0.5) * 2.0 * CELL_Z_SPREAD_UM,
            density,
            nucleus_dx: (h(8) - 0.5) * 0.7 * radius,
            nucleus_dy: (h(9) - 0.5) * 0.7 * radius,
            nucleus_r_um: if rounded { 0.0 } else { 0.42 * radius },
            nucleus_density: density * (0.5 + 0.7 * h(11)),
        });
    }
}
