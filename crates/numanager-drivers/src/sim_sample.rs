//! Shared deterministic specimen model for simulator drivers.
//!
//! The model is defined in world coordinates and has no dependency on cameras,
//! DAQ tasks, GUI state, frame handles, or runtime operations. Modality-specific
//! renderers can query the same seeded cell culture for brightfield, fluorescence,
//! and scanning simulations.

/// Sample tile edge in micrometres. Cells are generated per tile from a hash of
/// the tile index, so the culture extends without limit in XY and nothing is
/// retained between frames.
pub const TILE_UM: f64 = 128.0;
pub const MAX_CELL_RADIUS_UM: f64 = 12.0;
const CELL_Z_SPREAD_UM: f64 = 3.0;

/// Tiles gathered per axis for one query, so an unusual field of view keeps a
/// fixed maximum amount of sample work.
pub const TILE_SPAN_LIMIT: i64 = 24;

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
