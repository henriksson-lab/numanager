use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

pub const VID_TOUPTEK: u16 = 0x0547;
pub const VID_CYPRESS: u16 = 0x04b4;
pub const VID_TOUPTEK2: u16 = 0x232f;
pub const EP_IMAGE: u8 = 0x81;
pub const WIDTH: u32 = 3328;
pub const HEIGHT: u32 = 2548;
pub const PID_BENCH_CAM: u16 = 0x13a1;
pub const PID_U3CMOS03100KPA: u16 = 0x3310;
pub const FRAME_BYTES: usize = WIDTH as usize * HEIGHT as usize;
pub const BULK_CHUNK: usize = 512 * 1024;
pub const LINE_TIME_US: f64 = 37.983;
pub const MIN_FRAME_LINES: u32 = 2608;
pub const MAX_EXPOSURE_LINES: u32 = 0xffff;

// ---------------------------------------------------------------------------
// Wire protocol
//
// Implemented from the ToupTek USB camera interface specification
// (`docs/reverse/toupcam-protocol.md`). Vendor requests, all to the device
// recipient.
// ---------------------------------------------------------------------------

/// Presence probe. Its `wValue` announces the session token (see
/// [`SESSION_TOKEN`]); the first returned byte must be [`PROBE_READY`].
pub const REQ_PROBE: u8 = 0x16;
/// Single-register access for this device family, "form A": operands ride in
/// the setup packet and the single returned byte is status, not data.
pub const REQ_REGISTER: u8 = 0x0b;
/// Flat address-space read (identity/calibration blob).
pub const REQ_DP_READ: u8 = 0x20;
/// Stream start/stop.
pub const REQ_STREAM: u8 = 0x01;
/// `wIndex` for [`REQ_STREAM`].
pub const STREAM_INDEX: u16 = 0x000f;
pub const STREAM_START: u16 = 0x0003;
pub const STREAM_STOP: u16 = 0x0000;
/// Expected first byte from [`REQ_PROBE`].
pub const PROBE_READY: u8 = 0x08;

/// Integration time programmed at open, before the stream is started.
pub const DEFAULT_EXPOSURE_US: u32 = 94_000;

/// Session token announced in the probe.
///
/// The device masks register operands with a 16-bit value derived from this
/// token, but the derivation maps 0 to 0. Announcing 0 therefore selects an
/// identity mask and every register number and value travels in plaintext, so
/// this driver needs no masking arithmetic anywhere. The token is chosen freely
/// by the host and authenticates nothing.
pub const SESSION_TOKEN: u16 = 0;

// SMIA-style sensor registers used by the specified family.
pub const REG_RESET: u16 = 0x301a;
pub const REG_DATA_PEDESTAL: u16 = 0x301e;
pub const REG_VT_PIX_CLK_DIV: u16 = 0x302a;
pub const REG_VT_SYS_CLK_DIV: u16 = 0x302c;
pub const REG_PRE_PLL_CLK_DIV: u16 = 0x302e;
pub const REG_PLL_MULTIPLIER: u16 = 0x3030;
pub const REG_X_ADDR_START: u16 = 0x3004;
pub const REG_X_ADDR_END: u16 = 0x3008;
pub const REG_Y_ADDR_START: u16 = 0x3002;
pub const REG_Y_ADDR_END: u16 = 0x3006;
pub const REG_FRAME_LENGTH_LINES: u16 = 0x300a;
pub const REG_LINE_LENGTH_PCK: u16 = 0x300c;
pub const REG_COARSE_INTEGRATION_TIME: u16 = 0x3012;
pub const REG_ANALOG_GAIN: u16 = 0x3060;
pub const REG_READ_MODE: u16 = 0x3040;
pub const REG_X_ODD_INC: u16 = 0x30a2;
pub const REG_Y_ODD_INC: u16 = 0x30a6;

/// One step of a sensor bring-up table.
#[derive(Debug, Clone, Copy)]
pub enum InitStep {
    /// Write `value` to `register`.
    Reg(u16, u16),
    /// Pause before continuing.
    DelayMs(u64),
}

// `RESET_REGISTER` variants: the streaming bit plus two grouped-hold forms that
// let a group of registers change without tearing.
pub const RESET_POWER_ON: u16 = 0x00d8;
pub const RESET_STREAMING: u16 = 0x10d8;
pub const RESET_STANDBY: u16 = 0x10d0;
pub const RESET_HOLD_A: u16 = 0x10de;
pub const RESET_HOLD_B: u16 = 0x10dc;

/// Bring-up table for the 2048x1534 / 2.2 um sensor, up to the point where the
/// sensor is put in standby to accept the window and timing program.
///
/// Fixed for the device family and independent of the chosen resolution.
///
/// The published table ends this block with `RESET_REGISTER = 0x10D8`
/// (streaming). Traffic recorded from a working host writes `0x10D0` (standby)
/// at that point instead, and only returns to streaming after the window is
/// programmed. Standby is what the hardware needs — leaving the sensor
/// streaming across the window change yields no frames at all — so the standby
/// value is used here and the transition is part of [`SensorProfile`]
/// programming rather than of this table.
pub const INIT_U3CMOS03100KPA: &[InitStep] = &[
    InitStep::Reg(REG_RESET, RESET_POWER_ON),
    InitStep::DelayMs(100),
    InitStep::Reg(0x3021, 0x0100),
    InitStep::DelayMs(30),
    InitStep::Reg(REG_RESET, RESET_STREAMING),
    InitStep::Reg(0x30b0, 0x0000),
    InitStep::Reg(0x3064, 0x1902),
    InitStep::Reg(0x31ac, 0x0c0c),
    InitStep::Reg(0x3082, 0x0009),
    InitStep::Reg(0x30ba, 0x06ec),
    InitStep::Reg(0x3064, 0x1802),
    InitStep::Reg(0x31ae, 0x0301),
    InitStep::Reg(REG_RESET, RESET_STANDBY),
    InitStep::DelayMs(100),
    InitStep::Reg(REG_DATA_PEDESTAL, 0x0000),
    InitStep::Reg(REG_VT_PIX_CLK_DIV, 0x0006),
    InitStep::Reg(REG_PRE_PLL_CLK_DIV, 0x000a),
    InitStep::Reg(REG_PLL_MULTIPLIER, 0x0093),
    InitStep::Reg(REG_VT_SYS_CLK_DIV, 0x0001),
    InitStep::DelayMs(100),
];

/// How a model's sensor is programmed once the link is open.
#[derive(Debug, Clone, Copy)]
pub struct SensorProfile {
    /// Fixed bring-up sequence for the family.
    pub init: &'static [InitStep],
    /// Readout window. `end - start + 1` gives the frame geometry.
    pub x_addr_start: u16,
    pub x_addr_end: u16,
    pub y_addr_start: u16,
    pub y_addr_end: u16,
    pub frame_length_lines: u16,
    /// Row period in pixel clocks is twice this.
    pub line_length_pck: u16,
    /// Pixel-clock rate in MHz for the selected speed mode.
    pub pix_clk_mhz: u32,
}

impl SensorProfile {
    pub fn width(&self) -> u32 {
        (self.x_addr_end - self.x_addr_start + 1) as u32
    }
    pub fn height(&self) -> u32 {
        (self.y_addr_end - self.y_addr_start + 1) as u32
    }
}

/// Sensor programming for the 2048x1534 device. The window reproduces the
/// advertised geometry: `2181 - 134 + 1 = 2048`, `1539 - 6 + 1 = 1534`.
pub const SENSOR_U3CMOS03100KPA: SensorProfile = SensorProfile {
    init: INIT_U3CMOS03100KPA,
    x_addr_start: 134,
    x_addr_end: 2181,
    y_addr_start: 6,
    y_addr_end: 1539,
    frame_length_lines: 1560,
    line_length_pck: 1150,
    pix_clk_mhz: 98,
};

/// Row period in pixel clocks, twice `LINE_LENGTH_PCK`.
fn row_period(line_length_pck: u16) -> u32 {
    2 * line_length_pck as u32
}

/// Exposure in microseconds to `(COARSE_INTEGRATION_TIME, LINE_LENGTH_PCK)`.
///
/// Long exposures stretch the row period rather than overflow the 16-bit
/// integration-time field, so this can also change `LINE_LENGTH_PCK`; the
/// caller writes that register only when it differs from the current value.
pub fn coarse_integration_time(us: u32, profile: &SensorProfile) -> (u16, u16) {
    let mut period = row_period(profile.line_length_pck);
    let mut coarse =
        (us as u64 * profile.pix_clk_mhz as u64 + period as u64 / 2) / period.max(1) as u64;
    while coarse > 0xffff {
        coarse >>= 1;
        period <<= 1;
    }
    (coarse as u16, (period / 2) as u16)
}

/// Gain in hundredths (100 = 1.00x) to `ANALOG_GAIN`.
///
/// A step ladder: the code is that of the first threshold the gain falls under.
pub fn analog_gain_code(gain_hundredths: u16) -> u16 {
    const LADDER: &[(u16, u16)] = &[
        (104, 0x06),
        (108, 0x07),
        (113, 0x08),
        (118, 0x09),
        (123, 0x0a),
        (130, 0x0b),
        (137, 0x0c),
        (144, 0x0d),
        (153, 0x0e),
        (162, 0x0f),
        (173, 0x10),
        (186, 0x12),
        (200, 0x14),
        (217, 0x16),
        (236, 0x18),
        (260, 0x1a),
        (289, 0x1c),
    ];
    for (threshold, code) in LADDER {
        if gain_hundredths < *threshold {
            return *code;
        }
    }
    0x1e
}

/// How this driver brings a model up.
#[derive(Debug, Clone, Copy)]
pub enum ToupcamOpen {
    /// Program the sensor from the interface specification. Exposure and gain
    /// are computed, so any value in range can be set.
    Sensor(SensorProfile),
    /// Replay a recorded vendor open sequence verbatim. Reproduces exactly the
    /// state it was captured at and nothing else: the register semantics for
    /// this model were never derived, so exposure and gain cannot be computed.
    Replay(&'static str),
}

/// Per-model wire facts for a Toupcam-compatible camera.
///
/// Geometry and bring-up are model-specific. Models whose sensor register map
/// is specified are programmed directly; the rest fall back to a recorded
/// replay of the vendor's own open sequence.
#[derive(Debug, Clone, Copy)]
pub struct ToupcamModel {
    /// USB product id this profile is keyed on.
    pub product_id: u16,
    /// Vendor model string.
    pub model: &'static str,
    /// Full-frame sensor geometry in pixels.
    pub width: u32,
    pub height: u32,
    /// Bytes the device appends after the RAW8 pixel plane in each bulk frame.
    pub frame_trailer_bytes: usize,
    /// Bring-up strategy.
    pub open: ToupcamOpen,
}

impl ToupcamModel {
    /// RAW8 pixel bytes in one full frame, excluding any device trailer.
    pub fn pixel_bytes(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// Total bytes the device sends per bulk frame, including the trailer.
    pub fn frame_bytes(&self) -> usize {
        self.pixel_bytes() + self.frame_trailer_bytes
    }

    /// Sensor programming for this model, when it has a specified register map.
    pub fn sensor(&self) -> Option<SensorProfile> {
        match self.open {
            ToupcamOpen::Sensor(profile) => Some(profile),
            ToupcamOpen::Replay(_) => None,
        }
    }

    /// Whether exposure and gain can be computed for this model.
    pub fn tunable_registers(&self) -> bool {
        self.sensor().is_some()
    }
}

/// The bench camera the original clean-room backend was built against. Its
/// sensor register map was never derived, so it stays on the recorded replay.
pub const MODEL_U3CMOS08500KPA: ToupcamModel = ToupcamModel {
    product_id: PID_BENCH_CAM,
    model: "U3CMOS08500KPA",
    width: WIDTH,
    height: HEIGHT,
    frame_trailer_bytes: 0,
    open: ToupcamOpen::Replay(include_str!("toupcam_init_seq.jsonl")),
};

/// 3.1 MP model, programmed from the interface specification.
pub const MODEL_U3CMOS03100KPA: ToupcamModel = ToupcamModel {
    product_id: PID_U3CMOS03100KPA,
    model: "U3CMOS03100KPA",
    width: 2048,
    height: 1534,
    frame_trailer_bytes: 1,
    open: ToupcamOpen::Sensor(SENSOR_U3CMOS03100KPA),
};

/// Every model profile this driver carries a recorded open sequence for.
pub fn models() -> Vec<ToupcamModel> {
    vec![MODEL_U3CMOS08500KPA, MODEL_U3CMOS03100KPA]
}

/// The profile for a USB product id, if this driver has one.
pub fn model_for_product_id(product_id: u16) -> Option<ToupcamModel> {
    models()
        .into_iter()
        .find(|model| model.product_id == product_id)
}

/// Catalogue identity for a camera this driver can recognize but has no
/// profile for.
///
/// Streaming needs a per-model sensor register map or a recorded open sequence.
/// Identity and geometry, though, are known for the whole catalogue, which is
/// what turns "device hangs waiting for a frame that never comes" into a named,
/// actionable error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToupcamIdentity {
    pub product_id: u16,
    pub model: &'static str,
    /// Full-frame geometry. A minority of catalogue rows carry a name and
    /// product id but no geometry, so this is optional — a camera is still
    /// worth naming when its geometry is unknown.
    pub geometry: Option<(u32, u32)>,
}

/// Camera catalogue; see `docs/reverse/toupcam-model-registry.md` for contents
/// and known gaps.
const MODEL_REGISTRY: &str = include_str!("toupcam_models.tsv");

/// Look up a USB product id in the vendor registry.
pub fn identity_for_product_id(product_id: u16) -> Option<ToupcamIdentity> {
    for line in MODEL_REGISTRY.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut f = line.split('\t');
        let (Some(name), Some(_vid), Some(pid)) = (f.next(), f.next(), f.next()) else {
            continue;
        };
        let Ok(pid) = u16::from_str_radix(pid.trim_start_matches("0x"), 16) else {
            continue;
        };
        if pid != product_id {
            continue;
        }
        // Geometry is absent for a minority of rows; still report the model.
        let geometry = match (f.next(), f.next()) {
            (Some(w), Some(h)) => match (w.parse::<u32>(), h.parse::<u32>()) {
                (Ok(width), Ok(height)) => Some((width, height)),
                _ => None,
            },
            _ => None,
        };
        // `name` is borrowed from a `&'static str`, so the identity is 'static.
        return Some(ToupcamIdentity {
            product_id: pid,
            model: name,
            geometry,
        });
    }
    None
}

/// Number of camera variants in the vendor registry.
pub fn registry_len() -> usize {
    MODEL_REGISTRY
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .count()
}

pub fn is_toupcam_vendor(vid: u16) -> bool {
    matches!(vid, VID_TOUPTEK | VID_CYPRESS | VID_TOUPTEK2)
}

pub fn exposure_registers(us: u32) -> Vec<(u16, u16)> {
    let lines = ((us as f64 / LINE_TIME_US).round() as u32).clamp(1, 0xffff);
    let frame = lines.saturating_add(22).max(2608);
    vec![
        (0xa6ee, wval(1)),
        (0xa5e8, wval((lines >> 8) as u8)),
        (0xa5e9, wval(lines as u8)),
        (0xa4aa, wval((frame >> 8) as u8)),
        (0xa4ab, wval(frame as u8)),
        (0x96ea, wval(0)),
        (0x95fa, wval(0)),
        (0xa6ee, wval(0)),
    ]
}

pub fn gain_registers(percent: u16) -> Vec<(u16, u16)> {
    let percent = percent.max(100) as f64;
    let code = (1024.0 - 102400.0 / percent).round().clamp(0.0, 1023.0) as u32;
    vec![
        (0xa6ee, wval(1)),
        (0xa5ee, wval((code >> 8) as u8)),
        (0xa5ef, wval(code as u8)),
        (0xa6ee, wval(0)),
    ]
}

pub fn vendor_ids() -> Vec<u16> {
    vec![VID_TOUPTEK, VID_CYPRESS, VID_TOUPTEK2]
}

/// USB vendor ids this driver claims. Hosts that need raw USB access (udev
/// rules on Linux) must cover these; see
/// `usb_discovery::builtin_usb_vendor_claims`.
pub fn usb_vendor_ids() -> Vec<u16> {
    vendor_ids()
}

#[derive(Debug, Clone)]
struct ToupcamUsbIdentity {
    label: String,
    product: String,
    serial: Option<String>,
    vendor_id: u16,
    product_id: u16,
    bus_number: u8,
    device_address: u8,
}

impl ToupcamUsbIdentity {
    fn value(&self) -> Value {
        let mut fields = BTreeMap::from([
            ("label".into(), Value::String(self.label.clone())),
            ("product".into(), Value::String(self.product.clone())),
            ("vendor_id".into(), Value::I64(self.vendor_id as i64)),
            ("product_id".into(), Value::I64(self.product_id as i64)),
            (
                "vendor_id_hex".into(),
                Value::String(format!("0x{:04x}", self.vendor_id)),
            ),
            (
                "product_id_hex".into(),
                Value::String(format!("0x{:04x}", self.product_id)),
            ),
            ("bus_number".into(), Value::I64(self.bus_number as i64)),
            (
                "device_address".into(),
                Value::I64(self.device_address as i64),
            ),
            ("image_endpoint".into(), Value::I64(EP_IMAGE as i64)),
            (
                "sensor_width".into(),
                Value::PixelCount(PixelCount::new(WIDTH)),
            ),
            (
                "sensor_height".into(),
                Value::PixelCount(PixelCount::new(HEIGHT)),
            ),
        ]);
        if let Some(serial) = &self.serial {
            fields.insert("serial".into(), Value::String(serial.clone()));
        }
        Value::Map(fields)
    }
}

fn wval(byte: u8) -> u16 {
    0xa700 | (byte ^ 0xEA) as u16
}

pub struct ToupcamDriver {
    id: DriverId,
    camera: DeviceId,
    control: ResourceId,
    stream: ResourceId,
    label: String,
    product: String,
    serial_number: Option<String>,
    sensor_width: u32,
    sensor_height: u32,
    exposure_s: f64,
    gain_percent: i64,
    pixel_format: String,
    bayer_phase: BayerPhase,
    trigger_mode: String,
    roi_width: u32,
    roi_height: u32,
    binning: i64,
    black_level: i64,
    white_balance_red_percent: i64,
    white_balance_blue_percent: i64,
    sensor_temperature_c: f64,
    control_registers: BTreeMap<u16, u16>,
    usb_identity: Option<ToupcamUsbIdentity>,
    next_token: u64,
    events: VecDeque<DriverEvent>,
    worker_tx: Sender<DriverEvent>,
    worker_rx: Receiver<DriverEvent>,
    #[cfg(feature = "os-usb")]
    live: Option<live_toupcam::LiveToupcam>,
}

pub struct ToupcamDiscovery {
    next_id: DriverId,
    probes: Vec<ToupcamConfiguredProbe>,
    simulated: bool,
    #[cfg(feature = "os-usb")]
    live_usb: bool,
}

impl ToupcamDiscovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: Vec::new(),
            simulated: true,
            #[cfg(feature = "os-usb")]
            live_usb: false,
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| {
                matches!(
                    device.driver.as_str(),
                    "toupcam" | "touptek" | "toupcam_camera" | "toupcam-camera"
                )
            })
            .map(ToupcamConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            next_id,
            probes,
            simulated: false,
            #[cfg(feature = "os-usb")]
            live_usb: false,
        })
    }

    #[cfg(feature = "os-usb")]
    pub fn os_usb(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: Vec::new(),
            simulated: false,
            live_usb: true,
        }
    }
}

impl DriverDiscovery for ToupcamDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        let mut candidates = Vec::new();
        for (index, probe) in std::mem::take(&mut self.probes).into_iter().enumerate() {
            let id = DriverId(self.next_id.0 + index as u64);
            #[cfg(feature = "os-usb")]
            if probe.connect {
                let usb_index = probe.usb_index;
                let info = live_toupcam::list_cameras()?
                    .into_iter()
                    .nth(usb_index)
                    .ok_or_else(|| {
                        Error::new(
                            ErrorCode::Transport,
                            format!(
                                "no Toupcam USB device found for configured usb_index {usb_index}"
                            ),
                        )
                    })?;
                candidates.push(DriverCandidate::from_driver(
                    format!("Configured live Toupcam camera {}", probe.label),
                    Box::new(ToupcamDriver::configured_usb(id, probe, info)?),
                ));
                continue;
            }
            #[cfg(not(feature = "os-usb"))]
            if probe.connect {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    "Toupcam property.connect requires numanager-drivers/os-usb",
                ));
            }
            candidates.push(DriverCandidate::from_driver(
                format!("Configured Toupcam camera {}", probe.label),
                Box::new(ToupcamDriver::configured(id, probe)),
            ));
        }
        #[cfg(feature = "os-usb")]
        if self.live_usb {
            let base = self.next_id.0 + candidates.len() as u64;
            for (index, info) in live_toupcam::list_cameras()?.into_iter().enumerate() {
                candidates.push(DriverCandidate::from_driver(
                    info.label.clone(),
                    Box::new(ToupcamDriver::open_usb(
                        DriverId(base + index as u64),
                        index,
                        info,
                    )?),
                ));
            }
        }
        if self.simulated {
            candidates.push(DriverCandidate::from_driver(
                "Simulated Toupcam camera",
                Box::new(ToupcamDriver::simulated(self.next_id)),
            ));
        }
        Ok(candidates)
    }
}

#[derive(Debug, Clone)]
struct ToupcamConfiguredProbe {
    label: String,
    product: String,
    serial_number: Option<String>,
    connect: bool,
    usb_index: usize,
    sensor_width: u32,
    sensor_height: u32,
    roi_width: u32,
    roi_height: u32,
    exposure: TimeInterval,
    gain: Ratio,
    pixel_format: String,
    bayer_phase: BayerPhase,
    trigger_mode: String,
    /// Geometry exactly as written in config, before defaults were filled in, so
    /// a live open can fall back to the opened model's geometry rather than the
    /// bench camera's.
    configured_geometry: ConfiguredGeometry,
}

#[derive(Debug, Clone, Copy, Default)]
struct ConfiguredGeometry {
    sensor_width: Option<u32>,
    sensor_height: Option<u32>,
    roi_width: Option<u32>,
    roi_height: Option<u32>,
}

impl ToupcamConfiguredProbe {
    fn fixture() -> Self {
        Self {
            label: "toupcam-0".into(),
            product: "clean-room USB camera".into(),
            serial_number: None,
            connect: false,
            usb_index: 0,
            sensor_width: WIDTH,
            sensor_height: HEIGHT,
            roi_width: WIDTH,
            roi_height: HEIGHT,
            exposure: TimeInterval::from_seconds(0.1),
            gain: Ratio::from_percent(100.0),
            pixel_format: ImageEncoding::Raw8.property_value().into(),
            bayer_phase: BayerPhase::Unknown,
            trigger_mode: "software".into(),
            configured_geometry: ConfiguredGeometry::default(),
        }
    }

    /// Re-resolves geometry against the model actually opened. Config always
    /// wins; anything the config left out comes from the model instead of the
    /// bench-camera default.
    #[cfg(feature = "os-usb")]
    fn apply_model_geometry(&mut self, model: &ToupcamModel) {
        let cfg = self.configured_geometry;
        self.sensor_width = cfg.sensor_width.unwrap_or(model.width);
        self.sensor_height = cfg.sensor_height.unwrap_or(model.height);
        self.roi_width = cfg
            .roi_width
            .unwrap_or(self.sensor_width)
            .clamp(64, self.sensor_width.max(64));
        self.roi_height = cfg
            .roi_height
            .unwrap_or(self.sensor_height)
            .clamp(64, self.sensor_height.max(64));
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = Self::fixture();
        if !device.label.is_empty() {
            probe.label = device.label.clone();
        }
        probe.product = string_prop(device, "product").unwrap_or(probe.product);
        probe.serial_number = optional_string_prop(device, "serial_number", probe.serial_number);
        probe.connect = bool_prop(device, "connect").unwrap_or(probe.connect);
        let usb_index = i64_prop(device, "usb_index").unwrap_or(probe.usb_index as i64);
        probe.usb_index = usize::try_from(usb_index).map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Toupcam usb_index must be a non-negative integer",
            )
        })?;
        probe.configured_geometry = ConfiguredGeometry {
            sensor_width: pixel_count_prop(device, "sensor_width")?,
            sensor_height: pixel_count_prop(device, "sensor_height")?,
            roi_width: pixel_count_prop(device, "roi_width")?,
            roi_height: pixel_count_prop(device, "roi_height")?,
        };
        probe.sensor_width = probe
            .configured_geometry
            .sensor_width
            .unwrap_or(probe.sensor_width);
        probe.sensor_height = probe
            .configured_geometry
            .sensor_height
            .unwrap_or(probe.sensor_height);
        probe.roi_width = probe
            .configured_geometry
            .roi_width
            .unwrap_or(probe.sensor_width);
        probe.roi_height = probe
            .configured_geometry
            .roi_height
            .unwrap_or(probe.sensor_height);
        probe.roi_width = probe.roi_width.clamp(64, probe.sensor_width.max(64));
        probe.roi_height = probe.roi_height.clamp(64, probe.sensor_height.max(64));
        probe.exposure = time_interval_prop(device, "exposure")?.unwrap_or(probe.exposure);
        probe.gain = ratio_prop(device, "gain")?.unwrap_or(probe.gain);
        if let Some(pixel_format) = string_prop(device, "pixel_format") {
            probe.pixel_format = canonical_image_encoding_name(&pixel_format)
                .ok_or_else(|| {
                    Error::new(
                        ErrorCode::InvalidProperty,
                        format!("unsupported Toupcam pixel_format {pixel_format}"),
                    )
                })?
                .into();
        }
        if let Some(phase) = string_prop(device, "bayer_phase") {
            probe.bayer_phase = BayerPhase::parse(&phase)?;
        }
        if let Some(mode) = string_prop(device, "trigger_mode") {
            if matches!(mode.as_str(), "software" | "external" | "bulb") {
                probe.trigger_mode = mode;
            } else {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unsupported Toupcam trigger_mode {mode}"),
                ));
            }
        }
        Ok(probe)
    }
}

impl ToupcamDriver {
    pub fn simulated(id: DriverId) -> Self {
        let (worker_tx, worker_rx) = mpsc::channel();
        Self {
            id,
            camera: DeviceId(NodeId(100)),
            control: ResourceId(NodeId(101)),
            stream: ResourceId(NodeId(102)),
            label: "toupcam-0".into(),
            product: "clean-room USB camera".into(),
            serial_number: None,
            sensor_width: 640,
            sensor_height: 480,
            exposure_s: 0.1,
            gain_percent: 100,
            pixel_format: ImageEncoding::Raw8.property_value().into(),
            bayer_phase: BayerPhase::Unknown,
            trigger_mode: "software".into(),
            roi_width: 640,
            roi_height: 480,
            binning: 1,
            black_level: 0,
            white_balance_red_percent: 100,
            white_balance_blue_percent: 100,
            sensor_temperature_c: 28.0,
            control_registers: initial_control_registers(0.1, 100),
            usb_identity: None,
            next_token: 1,
            events: VecDeque::new(),
            worker_tx,
            worker_rx,
            #[cfg(feature = "os-usb")]
            live: None,
        }
    }

    fn configured(id: DriverId, probe: ToupcamConfiguredProbe) -> Self {
        let (worker_tx, worker_rx) = mpsc::channel();
        let exposure_s = probe.exposure.seconds();
        let gain_percent = probe.gain.percent().round() as i64;
        Self {
            id,
            camera: DeviceId(NodeId(id.0 * 1000 + 100)),
            control: ResourceId(NodeId(id.0 * 1000 + 101)),
            stream: ResourceId(NodeId(id.0 * 1000 + 102)),
            label: probe.label,
            product: probe.product,
            serial_number: probe.serial_number,
            sensor_width: probe.sensor_width.max(1),
            sensor_height: probe.sensor_height.max(1),
            exposure_s,
            gain_percent,
            pixel_format: probe.pixel_format,
            bayer_phase: probe.bayer_phase,
            trigger_mode: probe.trigger_mode,
            roi_width: probe.roi_width,
            roi_height: probe.roi_height,
            binning: 1,
            black_level: 0,
            white_balance_red_percent: 100,
            white_balance_blue_percent: 100,
            sensor_temperature_c: 28.0,
            control_registers: initial_control_registers(exposure_s, gain_percent as u16),
            usb_identity: None,
            next_token: 1,
            events: VecDeque::new(),
            worker_tx,
            worker_rx,
            #[cfg(feature = "os-usb")]
            live: None,
        }
    }

    #[cfg(feature = "os-usb")]
    fn configured_usb(
        id: DriverId,
        mut probe: ToupcamConfiguredProbe,
        info: live_toupcam::LiveToupcamInfo,
    ) -> Result<Self> {
        let (worker_tx, worker_rx) = mpsc::channel();
        let exposure_s = probe.exposure.seconds();
        let gain_percent = probe.gain.percent().round() as i64;
        if !(100..=1600).contains(&gain_percent) {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Toupcam configured live gain must be in 100..=1600 percent",
            ));
        }
        let live = live_toupcam::LiveToupcam::open(probe.usb_index)?;
        let model = live.model();
        probe.apply_model_geometry(&model);
        // A model whose register encoding is undecoded keeps whatever exposure
        // and gain its recorded open sequence reproduces; applying the bench
        // camera's register writes to it would be inventing behavior.
        let mut open_log = format!(
            "opened configured live Toupcam USB device {} as model {} ({}x{})",
            info.label, model.model, model.width, model.height
        );
        if model.tunable_registers() {
            live.set_exposure_us(seconds_to_us(exposure_s))?;
            live.set_gain_percent(gain_percent as u16)?;
        } else {
            open_log.push_str(
                "; configured exposure/gain not applied: register encoding not decoded for \
                 this model",
            );
        }
        Ok(Self {
            id,
            camera: DeviceId(NodeId(id.0 * 1000 + 100)),
            control: ResourceId(NodeId(id.0 * 1000 + 101)),
            stream: ResourceId(NodeId(id.0 * 1000 + 102)),
            label: probe.label,
            product: probe.product,
            serial_number: probe.serial_number,
            sensor_width: probe.sensor_width.max(1),
            sensor_height: probe.sensor_height.max(1),
            exposure_s,
            gain_percent,
            pixel_format: probe.pixel_format,
            bayer_phase: probe.bayer_phase,
            trigger_mode: probe.trigger_mode,
            roi_width: probe.roi_width,
            roi_height: probe.roi_height,
            binning: 1,
            black_level: 0,
            white_balance_red_percent: 100,
            white_balance_blue_percent: 100,
            sensor_temperature_c: 28.0,
            control_registers: initial_control_registers(exposure_s, gain_percent as u16),
            usb_identity: Some(info.identity.clone()),
            next_token: 1,
            events: VecDeque::from([DriverEvent::Event(Event::Log(LogEvent {
                driver: Some(id),
                message: open_log,
            }))]),
            worker_tx,
            worker_rx,
            live: Some(live),
        })
    }

    #[cfg(feature = "os-usb")]
    pub fn open_first_usb(id: DriverId) -> Result<Self> {
        let info = live_toupcam::list_cameras()?
            .into_iter()
            .next()
            .ok_or_else(|| Error::new(ErrorCode::Transport, "no Toupcam USB device found"))?;
        Self::open_usb(id, 0, info)
    }

    #[cfg(feature = "os-usb")]
    pub fn open_usb(
        id: DriverId,
        index: usize,
        info: live_toupcam::LiveToupcamInfo,
    ) -> Result<Self> {
        let (worker_tx, worker_rx) = mpsc::channel();
        let live = live_toupcam::LiveToupcam::open(index)?;
        let model = live.model();
        Ok(Self {
            id,
            camera: DeviceId(NodeId(100 + id.0)),
            control: ResourceId(NodeId(101 + id.0)),
            stream: ResourceId(NodeId(102 + id.0)),
            label: info.identity.label.clone(),
            product: info.identity.product.clone(),
            serial_number: info.identity.serial.clone(),
            sensor_width: model.width,
            sensor_height: model.height,
            exposure_s: 0.1,
            gain_percent: 100,
            pixel_format: ImageEncoding::Raw8.property_value().into(),
            bayer_phase: BayerPhase::Unknown,
            trigger_mode: "software".into(),
            roi_width: model.width,
            roi_height: model.height,
            binning: 1,
            black_level: 0,
            white_balance_red_percent: 100,
            white_balance_blue_percent: 100,
            sensor_temperature_c: 28.0,
            control_registers: initial_control_registers(0.1, 100),
            usb_identity: Some(info.identity.clone()),
            next_token: 1,
            events: VecDeque::from([DriverEvent::Event(Event::Log(LogEvent {
                driver: Some(id),
                message: format!(
                    "opened live Toupcam USB device {} as model {} ({}x{})",
                    info.label, model.model, model.width, model.height
                ),
            }))]),
            worker_tx,
            worker_rx,
            live: Some(live),
        })
    }

    fn descriptor(&self) -> DeviceDescriptor {
        DeviceDescriptor {
            id: self.camera,
            driver: self.id,
            label: self.label.clone(),
            vendor: Some("ToupTek".to_string()),
            model: Some(self.product.clone()),
            serial: self.serial_number.clone(),
            kinds: vec![
                "camera".into(),
                "trigger.sink".into(),
                "raw.register".into(),
            ],
            properties: vec![
                property_range(
                    "exposure",
                    "Exposure",
                    ValueType::TimeInterval,
                    Some("s"),
                    true,
                    time_interval(toupcam_min_exposure_s()),
                    time_interval(toupcam_max_exposure_s()),
                ),
                property_range(
                    "gain",
                    "Gain",
                    ValueType::Ratio,
                    Some("percent"),
                    true,
                    Value::Ratio(Ratio::from_percent(100.0)),
                    Value::Ratio(Ratio::from_percent(1_600.0)),
                ),
                property_enum(
                    "pixel_format",
                    "Pixel format",
                    ValueType::String,
                    None,
                    true,
                    ["Native", "Raw8", "Mono8", "Rgb8", "Bgr8"],
                ),
                property_enum(
                    "bayer_phase",
                    "Bayer phase",
                    ValueType::String,
                    None,
                    true,
                    ["Unknown", "Rggb", "Grbg", "Gbrg", "Bggr"],
                ),
                property_enum(
                    "trigger_mode",
                    "Trigger mode",
                    ValueType::String,
                    None,
                    true,
                    ["software", "external", "bulb"],
                ),
                property_range(
                    "roi_width",
                    "ROI width",
                    ValueType::PixelCount,
                    Some("px"),
                    true,
                    Value::PixelCount(PixelCount::new(64)),
                    Value::PixelCount(PixelCount::new(self.sensor_width.max(64))),
                ),
                property_range(
                    "roi_height",
                    "ROI height",
                    ValueType::PixelCount,
                    Some("px"),
                    true,
                    Value::PixelCount(PixelCount::new(64)),
                    Value::PixelCount(PixelCount::new(self.sensor_height.max(64))),
                ),
                property_enum("binning", "Binning", ValueType::I64, None, true, ["1", "2", "4"]),
                property_range(
                    "black_level",
                    "Black level",
                    ValueType::I64,
                    None,
                    true,
                    Value::I64(0),
                    Value::I64(255),
                ),
                property_range(
                    "white_balance_red",
                    "White balance red",
                    ValueType::Ratio,
                    Some("percent"),
                    true,
                    Value::Ratio(Ratio::from_percent(50.0)),
                    Value::Ratio(Ratio::from_percent(200.0)),
                ),
                property_range(
                    "white_balance_blue",
                    "White balance blue",
                    ValueType::Ratio,
                    Some("percent"),
                    true,
                    Value::Ratio(Ratio::from_percent(50.0)),
                    Value::Ratio(Ratio::from_percent(200.0)),
                ),
                property(
                    "sensor_temperature",
                    "Sensor temperature",
                    ValueType::Temperature,
                    Some("degC"),
                    false,
                ),
                property(
                    "usb_identity",
                    "USB identity",
                    ValueType::Map,
                    None,
                    false,
                ),
                property(
                    "supported_pixel_formats",
                    "Supported pixel formats",
                    ValueType::List,
                    None,
                    false,
                ),
                property(
                    "feature_summary",
                    "Feature summary",
                    ValueType::Map,
                    None,
                    false,
                ),
            ],
            metadata: BTreeMap::from([
                ("image_endpoint".into(), Value::I64(EP_IMAGE as i64)),
                (
                    "sensor_width".into(),
                    Value::PixelCount(PixelCount::new(self.sensor_width)),
                ),
                (
                    "sensor_height".into(),
                    Value::PixelCount(PixelCount::new(self.sensor_height)),
                ),
                (
                    "width".into(),
                    Value::PixelCount(PixelCount::new(self.frame_width())),
                ),
                (
                    "height".into(),
                    Value::PixelCount(PixelCount::new(self.frame_height())),
                ),
                ("usb_identity".into(), self.usb_identity_value()),
                ("feature_summary".into(), self.feature_summary_value()),
                (
                    "control_provenance".into(),
                    Value::String(
                        "clean-room Toupcam-like fixture; registers cover evidenced exposure/gain paths"
                            .into(),
                    ),
                ),
            ]),
        }
    }

    fn frame_width(&self) -> u32 {
        (self.roi_width / self.binning as u32).max(1)
    }

    fn frame_height(&self) -> u32 {
        (self.roi_height / self.binning as u32).max(1)
    }

    fn expected_raw_frame_bytes(&self) -> usize {
        self.sensor_width as usize * self.sensor_height as usize
    }

    fn supported_pixel_formats(&self) -> Value {
        Value::List(
            ["Native", "Raw8", "Mono8", "Rgb8", "Bgr8"]
                .into_iter()
                .map(|format| Value::String(format.into()))
                .collect(),
        )
    }

    fn usb_identity_value(&self) -> Value {
        if let Some(identity) = &self.usb_identity {
            let mut value = match identity.value() {
                Value::Map(fields) => fields,
                _ => BTreeMap::new(),
            };
            value.insert(
                "sensor_width".into(),
                Value::PixelCount(PixelCount::new(self.sensor_width)),
            );
            value.insert(
                "sensor_height".into(),
                Value::PixelCount(PixelCount::new(self.sensor_height)),
            );
            Value::Map(value)
        } else {
            usb_identity_value(self.sensor_width, self.sensor_height)
        }
    }

    fn feature_summary_value(&self) -> Value {
        let Value::Map(mut fields) = feature_summary_value() else {
            return Value::Null;
        };
        fields.insert(
            "live_usb_identity".into(),
            Value::Bool(self.usb_identity.is_some()),
        );
        if let Some(identity) = &self.usb_identity {
            fields.insert(
                "live_product".into(),
                Value::String(identity.product.clone()),
            );
            if let Some(serial) = &identity.serial {
                fields.insert("live_serial".into(), Value::String(serial.clone()));
            }
        }
        Value::Map(fields)
    }

    fn control_resource_metadata(&self) -> BTreeMap<String, Value> {
        BTreeMap::from([
            ("connected".into(), Value::Bool(self.usb_identity.is_some())),
            ("usb_identity".into(), self.usb_identity_value()),
            (
                "control_request".into(),
                Value::String("vendor request 0x0b for exposure/gain registers".into()),
            ),
        ])
    }

    fn stream_resource_metadata(&self) -> BTreeMap<String, Value> {
        BTreeMap::from([
            ("connected".into(), Value::Bool(self.usb_identity.is_some())),
            ("endpoint".into(), Value::I64(EP_IMAGE as i64)),
            ("usb_identity".into(), self.usb_identity_value()),
            ("bulk_chunk".into(), Value::I64(BULK_CHUNK as i64)),
            (
                "frame_bytes".into(),
                Value::ByteCount(ByteCount::new(self.expected_raw_frame_bytes() as u64)),
            ),
        ])
    }

    fn next_token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn validate_property(&self, key: &str, value: &Value) -> Result<()> {
        let key = public_camera_key(key);
        let descriptor = self.descriptor();
        let schema = descriptor
            .properties
            .iter()
            .find(|property| property.key == key)
            .ok_or_else(|| Error::new(ErrorCode::InvalidProperty, "unknown Toupcam property"))?;
        if !schema.writable {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "property is read-only",
            ));
        }
        schema.validate(value)
    }

    fn validate_property_value(&self, key: &str, value: &Value) -> Result<()> {
        self.validate_property(key, value)?;
        let key = public_camera_key(key);
        match (key, value) {
            ("pixel_format", Value::String(value)) => {
                if supported_toupcam_pixel_format(value).is_some() {
                    Ok(())
                } else {
                    Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "unsupported Toupcam pixel_format",
                    ))
                }
            }
            ("trigger_mode", Value::String(value)) => {
                if matches!(value.as_str(), "software" | "external" | "bulb") {
                    Ok(())
                } else {
                    Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "unsupported Toupcam trigger_mode",
                    ))
                }
            }
            ("binning", Value::I64(value)) => {
                if matches!(value, 1 | 2 | 4) {
                    Ok(())
                } else {
                    Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Toupcam binning expects 1, 2, or 4",
                    ))
                }
            }
            _ => Ok(()),
        }
    }

    fn apply_property_value(&mut self, key: &str, value: &Value) -> Result<()> {
        match (public_camera_key(key), value) {
            ("exposure", value) => {
                if let Ok(v) = time_seconds(value) {
                    #[cfg(feature = "os-usb")]
                    if let Some(live) = self.live.as_mut() {
                        live.set_exposure_us(seconds_to_us(v))?;
                    }
                    self.exposure_s = v;
                    self.apply_register_sequence(exposure_registers(seconds_to_us(v)));
                }
            }
            ("gain", Value::Ratio(v)) => {
                let percent = v.percent().round() as i64;
                #[cfg(feature = "os-usb")]
                if let Some(live) = self.live.as_mut() {
                    live.set_gain_percent(percent as u16)?;
                }
                self.gain_percent = percent;
                self.apply_register_sequence(gain_registers(percent as u16));
            }
            ("pixel_format", Value::String(v)) => {
                if let Some(format) = canonical_image_encoding_name(v) {
                    self.pixel_format = format.into();
                }
            }
            ("bayer_phase", Value::String(v)) => self.bayer_phase = BayerPhase::parse(v)?,
            ("trigger_mode", Value::String(v)) => self.trigger_mode = v.clone(),
            ("roi_width", Value::PixelCount(v)) => {
                self.roi_width = v.pixels().clamp(64, self.sensor_width.max(64))
            }
            ("roi_height", Value::PixelCount(v)) => {
                self.roi_height = v.pixels().clamp(64, self.sensor_height.max(64))
            }
            ("binning", Value::I64(v)) => self.binning = *v,
            ("black_level", Value::I64(v)) => self.black_level = (*v).clamp(0, 255),
            ("white_balance_red", Value::Ratio(v)) => {
                self.white_balance_red_percent = (v.percent().round() as i64).clamp(50, 200);
            }
            ("white_balance_blue", Value::Ratio(v)) => {
                self.white_balance_blue_percent = (v.percent().round() as i64).clamp(50, 200);
            }
            _ => {}
        }
        Ok(())
    }

    fn apply_register_sequence(&mut self, sequence: Vec<(u16, u16)>) {
        for (index, value) in sequence {
            self.control_registers.insert(index, value);
        }
    }

    fn raw_register_transaction(&self, request: &RawRegisterRequest) -> PhysicalTransaction {
        let (operation, index, value) = match request {
            RawRegisterRequest::Read { index } => ("read", *index, self.raw_register_value(*index)),
        };
        let completion = "cached register map";
        PhysicalTransaction {
            resource: Some(self.control),
            description: format!("toupcam raw USB control {operation} wIndex=0x{index:04x}"),
            payload: raw_register_result(operation, index, value, completion),
        }
    }

    fn trigger_transaction(&self, action: &ToupcamTriggerAction) -> PhysicalTransaction {
        PhysicalTransaction {
            resource: Some(self.control),
            description: "toupcam trigger sink".into(),
            payload: Value::Map(BTreeMap::from([
                ("action".into(), Value::String(action.name().into())),
                (
                    "trigger_mode".into(),
                    Value::String(match action {
                        ToupcamTriggerAction::SetMode(mode) => mode.to_string(),
                        ToupcamTriggerAction::Pulse => self.trigger_mode.clone(),
                    }),
                ),
                (
                    "completion".into(),
                    Value::String("fixture control endpoint ack".into()),
                ),
            ])),
        }
    }

    fn invoke_trigger_sink(&mut self, action: ToupcamTriggerAction) -> Value {
        match action {
            ToupcamTriggerAction::Pulse => {
                self.events
                    .push_back(DriverEvent::Event(Event::Telemetry(TelemetryEvent {
                        device: self.camera,
                        values: BTreeMap::from([
                            ("triggered".into(), Value::Bool(true)),
                            (
                                "trigger_mode".into(),
                                Value::String(self.trigger_mode.clone()),
                            ),
                            (
                                "completion".into(),
                                Value::String("fixture trigger ack".into()),
                            ),
                        ]),
                    })));
                Value::Map(BTreeMap::from([
                    ("triggered".into(), Value::Bool(true)),
                    (
                        "trigger_mode".into(),
                        Value::String(self.trigger_mode.clone()),
                    ),
                ]))
            }
            ToupcamTriggerAction::SetMode(mode) => {
                self.trigger_mode = mode.to_string();
                let value = Value::String(self.trigger_mode.clone());
                self.events
                    .push_back(DriverEvent::Event(Event::PropertyChanged(
                        PropertyChanged {
                            device: self.camera,
                            key: "trigger_mode".into(),
                            value: value.clone(),
                        },
                    )));
                Value::Map(BTreeMap::from([
                    ("triggered".into(), Value::Bool(false)),
                    ("trigger_mode".into(), value),
                ]))
            }
        }
    }

    fn invoke_raw_register(&mut self, request: RawRegisterRequest) -> Result<Value> {
        match request {
            RawRegisterRequest::Read { index } => Ok(raw_register_result(
                "read",
                index,
                self.raw_register_value(index),
                "cached register map",
            )),
        }
    }

    fn raw_register_value(&self, index: u16) -> u16 {
        self.control_registers.get(&index).copied().unwrap_or(0)
    }

    fn local_timing_routes(&self, plan: &TimingPlan) -> Vec<Value> {
        plan.routes
            .iter()
            .filter(|route| route.from == self.camera || route.to == self.camera)
            .map(|route| {
                Value::Map(BTreeMap::from([
                    ("from".into(), Value::I64(route.from.0 .0 as i64)),
                    ("to".into(), Value::I64(route.to.0 .0 as i64)),
                    (
                        "signal".into(),
                        Value::String(format!("{:?}", route.signal)),
                    ),
                    ("edge".into(), Value::String(format!("{:?}", route.edge))),
                    (
                        "delay".into(),
                        Value::TimeInterval(TimeInterval::from_seconds(route.delay.as_secs_f64())),
                    ),
                ]))
            })
            .collect()
    }

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| sequence.device == self.camera)
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        let descriptor = self.descriptor();
        for sequence in self.local_timing_sequences(plan) {
            if sequence.values.is_empty() {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "Toupcam timing sequence must contain at least one value",
                ));
            }
            let schema = descriptor
                .properties
                .iter()
                .find(|property| property.key == sequence.property)
                .ok_or_else(|| {
                    Error::new(ErrorCode::InvalidProperty, "unknown Toupcam property")
                })?;
            if !schema.sequenceable {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Toupcam property {} is not sequenceable", sequence.property),
                ));
            }
            for value in &sequence.values {
                self.validate_property_value(&sequence.property, value)?;
            }
        }
        Ok(())
    }

    fn timing_sequence_summary(&self, plan: &TimingPlan) -> Vec<Value> {
        self.local_timing_sequences(plan)
            .into_iter()
            .map(|sequence| {
                Value::Map(BTreeMap::from([
                    ("property".into(), Value::String(sequence.property.clone())),
                    ("count".into(), Value::I64(sequence.values.len() as i64)),
                ]))
            })
            .collect()
    }

    fn current_property_value(&self, key: &str) -> Value {
        match public_camera_key(key) {
            "exposure" => time_interval(self.exposure_s),
            "gain" => Value::Ratio(Ratio::from_percent(self.gain_percent as f64)),
            "pixel_format" => Value::String(self.pixel_format.clone()),
            "bayer_phase" => Value::String(self.bayer_phase.name().into()),
            _ => Value::Null,
        }
    }

    fn timing_summary(&self, plan: &TimingPlan, phase: &str, applied: Value) -> Value {
        Value::Map(BTreeMap::from([
            ("camera".into(), Value::I64(self.camera.0 .0 as i64)),
            ("phase".into(), Value::String(phase.into())),
            ("routes".into(), Value::List(self.local_timing_routes(plan))),
            (
                "sequences".into(),
                Value::List(self.timing_sequence_summary(plan)),
            ),
            ("exposure".into(), time_interval(self.exposure_s)),
            (
                "gain".into(),
                Value::Ratio(Ratio::from_percent(self.gain_percent as f64)),
            ),
            (
                "pixel_format".into(),
                Value::String(self.pixel_format.clone()),
            ),
            ("applied".into(), applied),
        ]))
    }

    fn apply_timing_sequence_step(&mut self, plan: &TimingPlan, start: bool) -> Result<Value> {
        let sequences = self
            .local_timing_sequences(plan)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let mut applied = BTreeMap::new();
        for sequence in sequences {
            let value = (if start {
                sequence.values.first()
            } else {
                sequence.values.last()
            })
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidCommand,
                    "Toupcam timing sequence must contain at least one value",
                )
            })?
            .clone();
            self.validate_property_value(&sequence.property, &value)?;
            self.apply_property_value(&sequence.property, &value)?;
            let applied_value = self.current_property_value(&sequence.property);
            self.events
                .push_back(DriverEvent::Event(Event::PropertyChanged(
                    PropertyChanged {
                        device: sequence.device,
                        key: sequence.property.clone(),
                        value: applied_value.clone(),
                    },
                )));
            applied.insert(
                format!("{}:{}", sequence.device.0 .0, sequence.property),
                applied_value,
            );
        }
        Ok(Value::Map(applied))
    }
}

impl Driver for ToupcamDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        vec![self.descriptor()]
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![
            ResourceDescriptor {
                id: self.control,
                driver: self.id,
                label: "toupcam-control".into(),
                kind: "usb.control".into(),
                metadata: self.control_resource_metadata(),
            },
            ResourceDescriptor {
                id: self.stream,
                driver: self.id,
                label: "toupcam-bulk-stream".into(),
                kind: "usb.bulk-in".into(),
                metadata: self.stream_resource_metadata(),
            },
        ]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device != self.camera {
            return Vec::new();
        }
        vec![
            capability(
                1,
                self.camera,
                CapabilityKind::CameraCapture,
                ValueType::Map,
                ValueType::Map,
            ),
            capability(
                2,
                self.camera,
                CapabilityKind::CameraStream,
                ValueType::Map,
                ValueType::Map,
            ),
            capability(
                3,
                self.camera,
                CapabilityKind::TriggerSink,
                ValueType::Map,
                ValueType::Map,
            ),
            capability(
                4,
                self.camera,
                CapabilityKind::RawRegisterAccess,
                ValueType::Map,
                ValueType::Map,
            ),
        ]
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut transactions = Vec::new();
        for command in &batch.commands {
            match command {
                Command::WriteProperty { device, key, value } if *device == self.camera => {
                    self.validate_property_value(key, value)?;
                    let key = public_camera_key(key);
                    if key == "exposure" {
                        let seconds = time_seconds(value)?;
                        let us = seconds_to_us(seconds);
                        transactions.push(PhysicalTransaction {
                            resource: Some(self.control),
                            description: "toupcam exposure register sequence".into(),
                            payload: Value::List(
                                exposure_registers(us)
                                    .into_iter()
                                    .map(|(index, value)| {
                                        Value::Map(BTreeMap::from([
                                            ("w_index".into(), Value::I64(index as i64)),
                                            ("w_value".into(), Value::I64(value as i64)),
                                        ]))
                                    })
                                    .collect(),
                            ),
                        });
                    } else if key == "gain" {
                        let percent = match value {
                            Value::Ratio(v) => v.percent().round() as u16,
                            _ => {
                                return Err(Error::new(
                                    ErrorCode::InvalidProperty,
                                    "gain expects Ratio",
                                ))
                            }
                        };
                        transactions.push(PhysicalTransaction {
                            resource: Some(self.control),
                            description: "toupcam gain register sequence".into(),
                            payload: Value::List(
                                gain_registers(percent)
                                    .into_iter()
                                    .map(|(index, value)| {
                                        Value::Map(BTreeMap::from([
                                            ("w_index".into(), Value::I64(index as i64)),
                                            ("w_value".into(), Value::I64(value as i64)),
                                        ]))
                                    })
                                    .collect(),
                            ),
                        });
                    }
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        if write.device == self.camera {
                            self.validate_property_value(&write.property, &write.value)?;
                        }
                    }
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if *device == self.camera && *capability == CapabilityId(1) => {
                    if !matches!(
                        request,
                        CapabilityRequest::CameraCapture(_) | CapabilityRequest::None
                    ) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "CameraCapture expects CameraCaptureRequest",
                        ));
                    }
                    transactions.push(PhysicalTransaction {
                        resource: Some(self.stream),
                        description: "queued bulk frame read".into(),
                        payload: Value::String("camera capture".into()),
                    });
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if *device == self.camera && *capability == CapabilityId(2) => {
                    if !matches!(request, CapabilityRequest::CameraStream(_)) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "CameraStream expects CameraStreamRequest",
                        ));
                    }
                    transactions.push(PhysicalTransaction {
                        resource: Some(self.stream),
                        description: "queued bulk stream read".into(),
                        payload: Value::String("camera stream".into()),
                    });
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if *device == self.camera && *capability == CapabilityId(3) => {
                    let action = parse_toupcam_trigger_action(request)?;
                    transactions.push(self.trigger_transaction(&action));
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if *device == self.camera && *capability == CapabilityId(4) => {
                    let request = parse_raw_register_request(request)?;
                    transactions.push(self.raw_register_transaction(&request));
                }
                _ => {}
            }
        }
        Ok(PreparedBatch {
            id: batch.id,
            commands: batch.commands.clone(),
            physical_transactions: transactions,
        })
    }

    fn prepare_timing_plan(
        &mut self,
        plan: &TimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        self.validate_timing_plan(plan)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Arm(plan.clone())],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.control),
                description: "toupcam timing arm".into(),
                payload: self.timing_summary(plan, "arm", Value::Null),
            }],
        })
    }

    fn start_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let applied = self.apply_timing_sequence_step(&armed.plan, true)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Start(OperationId(0))],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.control),
                description: "toupcam timing start".into(),
                payload: self.timing_summary(&armed.plan, "start", applied),
            }],
        })
    }

    fn stop_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let applied = self.apply_timing_sequence_step(&armed.plan, false)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.control),
                description: "toupcam timing stop".into(),
                payload: self.timing_summary(&armed.plan, "stop", applied),
            }],
        })
    }

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.next_token();
        let mut result = Value::Null;
        for command in prepared.commands {
            match command {
                Command::ReadProperty { device, key } if device == self.camera => {
                    result = match public_camera_key(&key) {
                        "exposure" => time_interval(self.exposure_s),
                        "gain" => Value::Ratio(Ratio::from_percent(self.gain_percent as f64)),
                        "pixel_format" => Value::String(self.pixel_format.clone()),
                        "bayer_phase" => Value::String(self.bayer_phase.name().into()),
                        "trigger_mode" => Value::String(self.trigger_mode.clone()),
                        "roi_width" => Value::PixelCount(PixelCount::new(self.roi_width)),
                        "roi_height" => Value::PixelCount(PixelCount::new(self.roi_height)),
                        "binning" => Value::I64(self.binning),
                        "black_level" => Value::I64(self.black_level),
                        "white_balance_red" => {
                            Value::Ratio(Ratio::from_percent(self.white_balance_red_percent as f64))
                        }
                        "white_balance_blue" => Value::Ratio(Ratio::from_percent(
                            self.white_balance_blue_percent as f64,
                        )),
                        "sensor_temperature" => {
                            Value::Temperature(Temperature::from_celsius(self.sensor_temperature_c))
                        }
                        "usb_identity" => self.usb_identity_value(),
                        "supported_pixel_formats" => self.supported_pixel_formats(),
                        "feature_summary" => self.feature_summary_value(),
                        _ => Value::Null,
                    };
                }
                Command::WriteProperty { device, key, value } if device == self.camera => {
                    self.validate_property_value(&key, &value)?;
                    self.apply_property_value(&key, &value)?;
                    self.events
                        .push_back(DriverEvent::Event(Event::PropertyChanged(
                            PropertyChanged { device, key, value },
                        )));
                    result = Value::Bool(true);
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if device == self.camera && capability == CapabilityId(1) => {
                    let exposure_s = self.exposure_s;
                    let width = self.frame_width();
                    let height = self.frame_height();
                    let configured_pixel_format = self.pixel_format.clone();
                    let bayer_phase = self.bayer_phase;
                    let trigger_mode = self.trigger_mode.clone();
                    let gain_percent = self.gain_percent;
                    let black_level = self.black_level;
                    let binning = self.binning;
                    let wb_red = self.white_balance_red_percent;
                    let wb_blue = self.white_balance_blue_percent;
                    let encoding = match request {
                        CapabilityRequest::CameraCapture(request) => request,
                        CapabilityRequest::None => CameraCaptureRequest::default_frame(),
                        _ => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "CameraCapture expects CameraCaptureRequest",
                            ))
                        }
                    };
                    #[cfg(feature = "os-usb")]
                    if let Some(live) = self.live.as_ref() {
                        let raw = live.read_frame(self.expected_raw_frame_bytes())?;
                        let mut encoded = encode_raw8_frame(
                            raw,
                            self.sensor_width,
                            self.sensor_height,
                            encoding.encoding.unwrap_or(ImageEncoding::Raw8),
                            &configured_pixel_format,
                            bayer_phase,
                            true,
                        )?;
                        apply_image_processing(&mut encoded, black_level, wb_red, wb_blue);
                        let handle = FrameHandle {
                            stream: StreamId(device.0 .0),
                            frame: FrameId(token.0),
                        };
                        self.events.push_back(DriverEvent::FrameReady(Frame {
                            handle,
                            device,
                            width: encoded.width,
                            height: encoded.height,
                            pixel_format: encoded.pixel_format.clone(),
                            data: encoded.data,
                            metadata: BTreeMap::from([
                                ("exposure".into(), time_interval(exposure_s)),
                                (
                                    "gain".into(),
                                    Value::Ratio(Ratio::from_percent(gain_percent as f64)),
                                ),
                                ("trigger_mode".into(), Value::String(trigger_mode)),
                                (
                                    "bayer_phase".into(),
                                    Value::String(bayer_phase.name().into()),
                                ),
                                ("black_level".into(), Value::I64(black_level)),
                                (
                                    "white_balance_red".into(),
                                    Value::Ratio(Ratio::from_percent(wb_red as f64)),
                                ),
                                (
                                    "white_balance_blue".into(),
                                    Value::Ratio(Ratio::from_percent(wb_blue as f64)),
                                ),
                                ("source".into(), Value::String("toupcam-live-usb".into())),
                            ]),
                            buffer: encoding.buffer.unwrap_or_default(),
                        }));
                        self.events.push_back(DriverEvent::TokenCompleted {
                            token,
                            value: Value::Map(BTreeMap::from([
                                (
                                    "width".into(),
                                    Value::PixelCount(PixelCount::new(encoded.width)),
                                ),
                                (
                                    "height".into(),
                                    Value::PixelCount(PixelCount::new(encoded.height)),
                                ),
                                ("pixel_format".into(), Value::String(encoded.pixel_format)),
                                ("stream".into(), Value::I64(handle.stream.0 as i64)),
                                ("frame".into(), Value::I64(handle.frame.0 as i64)),
                                ("source".into(), Value::String("toupcam-live-usb".into())),
                            ])),
                        });
                        return Ok(token);
                    }
                    let tx = self.worker_tx.clone();
                    thread::spawn(move || {
                        let frame = crate::sim::gel_scene(width, height, exposure_s);
                        let mut encoded = match encode_raw8_frame(
                            frame.pixels,
                            frame.width,
                            frame.height,
                            encoding.encoding.unwrap_or(ImageEncoding::Mono8),
                            &configured_pixel_format,
                            bayer_phase,
                            false,
                        ) {
                            Ok(encoded) => encoded,
                            Err(error) => {
                                let _ = tx.send(DriverEvent::TokenFailed {
                                    token,
                                    report: error.into(),
                                });
                                return;
                            }
                        };
                        apply_image_processing(&mut encoded, black_level, wb_red, wb_blue);
                        let handle = FrameHandle {
                            stream: StreamId(device.0 .0),
                            frame: FrameId(token.0),
                        };
                        let _ = tx.send(DriverEvent::FrameReady(Frame {
                            handle,
                            device,
                            width: encoded.width,
                            height: encoded.height,
                            pixel_format: encoded.pixel_format.clone(),
                            data: encoded.data,
                            metadata: BTreeMap::from([
                                ("exposure".into(), time_interval(exposure_s)),
                                (
                                    "gain".into(),
                                    Value::Ratio(Ratio::from_percent(gain_percent as f64)),
                                ),
                                ("black_level".into(), Value::I64(black_level)),
                                ("binning".into(), Value::I64(binning)),
                                ("trigger_mode".into(), Value::String(trigger_mode)),
                                (
                                    "bayer_phase".into(),
                                    Value::String(bayer_phase.name().into()),
                                ),
                                (
                                    "white_balance_red".into(),
                                    Value::Ratio(Ratio::from_percent(wb_red as f64)),
                                ),
                                (
                                    "white_balance_blue".into(),
                                    Value::Ratio(Ratio::from_percent(wb_blue as f64)),
                                ),
                                ("feature_summary".into(), feature_summary_value()),
                                ("source".into(), Value::String("toupcam-simulated".into())),
                            ]),
                            buffer: encoding.buffer.unwrap_or_default(),
                        }));
                        let _ = tx.send(DriverEvent::TokenCompleted {
                            token,
                            value: Value::Map(BTreeMap::from([
                                (
                                    "width".into(),
                                    Value::PixelCount(PixelCount::new(encoded.width)),
                                ),
                                (
                                    "height".into(),
                                    Value::PixelCount(PixelCount::new(encoded.height)),
                                ),
                                ("pixel_format".into(), Value::String(encoded.pixel_format)),
                                ("stream".into(), Value::I64(handle.stream.0 as i64)),
                                ("frame".into(), Value::I64(handle.frame.0 as i64)),
                            ])),
                        });
                    });
                    return Ok(token);
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if device == self.camera && capability == CapabilityId(2) => {
                    let exposure_s = self.exposure_s;
                    let width = self.frame_width();
                    let height = self.frame_height();
                    let configured_pixel_format = self.pixel_format.clone();
                    let bayer_phase = self.bayer_phase;
                    let trigger_mode = self.trigger_mode.clone();
                    let gain_percent = self.gain_percent;
                    let black_level = self.black_level;
                    let binning = self.binning;
                    let wb_red = self.white_balance_red_percent;
                    let wb_blue = self.white_balance_blue_percent;
                    let request = match request {
                        CapabilityRequest::CameraStream(request) => request,
                        _ => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "CameraStream expects CameraStreamRequest",
                            ))
                        }
                    };
                    #[cfg(feature = "os-usb")]
                    if let Some(live) = self.live.as_ref() {
                        let frame_count = request.frame_count.unwrap_or(8);
                        let stream = StreamId(token.0);
                        let mut completed_width = self.sensor_width;
                        let mut completed_height = self.sensor_height;
                        let mut completed_pixel_format =
                            ImageEncoding::Raw8.property_value().to_string();
                        for i in 0..frame_count {
                            let raw = live.read_frame(self.expected_raw_frame_bytes())?;
                            let mut encoded = encode_raw8_frame(
                                raw,
                                self.sensor_width,
                                self.sensor_height,
                                request.encoding.clone().unwrap_or(ImageEncoding::Raw8),
                                &configured_pixel_format,
                                bayer_phase,
                                true,
                            )?;
                            apply_image_processing(&mut encoded, black_level, wb_red, wb_blue);
                            completed_width = encoded.width;
                            completed_height = encoded.height;
                            completed_pixel_format = encoded.pixel_format.clone();
                            self.events.push_back(DriverEvent::FrameReady(Frame {
                                handle: FrameHandle {
                                    stream,
                                    frame: FrameId(i),
                                },
                                device,
                                width: encoded.width,
                                height: encoded.height,
                                pixel_format: encoded.pixel_format,
                                data: encoded.data,
                                metadata: BTreeMap::from([
                                    ("exposure".into(), time_interval(exposure_s)),
                                    (
                                        "gain".into(),
                                        Value::Ratio(Ratio::from_percent(gain_percent as f64)),
                                    ),
                                    ("trigger_mode".into(), Value::String(trigger_mode.clone())),
                                    (
                                        "bayer_phase".into(),
                                        Value::String(bayer_phase.name().into()),
                                    ),
                                    ("black_level".into(), Value::I64(black_level)),
                                    (
                                        "white_balance_red".into(),
                                        Value::Ratio(Ratio::from_percent(wb_red as f64)),
                                    ),
                                    (
                                        "white_balance_blue".into(),
                                        Value::Ratio(Ratio::from_percent(wb_blue as f64)),
                                    ),
                                    ("source".into(), Value::String("toupcam-live-usb".into())),
                                    ("index".into(), Value::I64(i as i64)),
                                ]),
                                buffer: request.buffer.clone(),
                            }));
                        }
                        self.events.push_back(DriverEvent::TokenCompleted {
                            token,
                            value: Value::Map(BTreeMap::from([
                                ("stream".into(), Value::I64(stream.0 as i64)),
                                ("frame".into(), Value::I64(0)),
                                (
                                    "width".into(),
                                    Value::PixelCount(PixelCount::new(completed_width)),
                                ),
                                (
                                    "height".into(),
                                    Value::PixelCount(PixelCount::new(completed_height)),
                                ),
                                ("frames".into(), Value::I64(frame_count as i64)),
                                ("pixel_format".into(), Value::String(completed_pixel_format)),
                                ("source".into(), Value::String("toupcam-live-usb".into())),
                            ])),
                        });
                        return Ok(token);
                    }
                    let tx = self.worker_tx.clone();
                    thread::spawn(move || {
                        let frame_count = request.frame_count.unwrap_or(8);
                        let stream = StreamId(token.0);
                        let mut completed_width = width;
                        let mut completed_height = height;
                        let mut completed_pixel_format = configured_pixel_format.clone();
                        for i in 0..frame_count {
                            let frame = crate::sim::gel_scene(width, height, exposure_s);
                            let mut encoded = match encode_raw8_frame(
                                frame.pixels,
                                frame.width,
                                frame.height,
                                request.encoding.clone().unwrap_or(ImageEncoding::Mono8),
                                &configured_pixel_format,
                                bayer_phase,
                                false,
                            ) {
                                Ok(encoded) => encoded,
                                Err(error) => {
                                    let _ = tx.send(DriverEvent::TokenFailed {
                                        token,
                                        report: error.into(),
                                    });
                                    return;
                                }
                            };
                            apply_image_processing(&mut encoded, black_level, wb_red, wb_blue);
                            completed_width = encoded.width;
                            completed_height = encoded.height;
                            completed_pixel_format = encoded.pixel_format.clone();
                            let handle = FrameHandle {
                                stream,
                                frame: FrameId(i),
                            };
                            let _ = tx.send(DriverEvent::FrameReady(Frame {
                                handle,
                                device,
                                width: encoded.width,
                                height: encoded.height,
                                pixel_format: encoded.pixel_format,
                                data: encoded.data,
                                metadata: BTreeMap::from([
                                    ("exposure".into(), time_interval(exposure_s)),
                                    (
                                        "gain".into(),
                                        Value::Ratio(Ratio::from_percent(gain_percent as f64)),
                                    ),
                                    ("black_level".into(), Value::I64(black_level)),
                                    ("binning".into(), Value::I64(binning)),
                                    ("trigger_mode".into(), Value::String(trigger_mode.clone())),
                                    (
                                        "bayer_phase".into(),
                                        Value::String(bayer_phase.name().into()),
                                    ),
                                    (
                                        "white_balance_red".into(),
                                        Value::Ratio(Ratio::from_percent(wb_red as f64)),
                                    ),
                                    (
                                        "white_balance_blue".into(),
                                        Value::Ratio(Ratio::from_percent(wb_blue as f64)),
                                    ),
                                    ("feature_summary".into(), feature_summary_value()),
                                    (
                                        "source".into(),
                                        Value::String("toupcam-stream-simulated".into()),
                                    ),
                                    ("index".into(), Value::I64(i as i64)),
                                ]),
                                buffer: request.buffer.clone(),
                            }));
                        }
                        let _ = tx.send(DriverEvent::TokenCompleted {
                            token,
                            value: Value::Map(BTreeMap::from([
                                ("stream".into(), Value::I64(stream.0 as i64)),
                                ("frame".into(), Value::I64(0)),
                                (
                                    "width".into(),
                                    Value::PixelCount(PixelCount::new(completed_width)),
                                ),
                                (
                                    "height".into(),
                                    Value::PixelCount(PixelCount::new(completed_height)),
                                ),
                                ("frames".into(), Value::I64(frame_count as i64)),
                                ("pixel_format".into(), Value::String(completed_pixel_format)),
                            ])),
                        });
                    });
                    return Ok(token);
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if device == self.camera && capability == CapabilityId(3) => {
                    let action = parse_owned_toupcam_trigger_action(request)?;
                    result = self.invoke_trigger_sink(action);
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if device == self.camera && capability == CapabilityId(4) => {
                    let request = parse_owned_raw_register_request(request)?;
                    result = self.invoke_raw_register(request)?;
                }
                Command::Invoke { .. } => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "unsupported Toupcam capability invocation",
                    ));
                }
                Command::ApplyStateSet(set) => {
                    let remuxed = set.writes.len() > 1;
                    for write in set.writes {
                        if write.device == self.camera {
                            self.validate_property_value(&write.property, &write.value)?;
                            self.apply_property_value(&write.property, &write.value)?;
                            self.events
                                .push_back(DriverEvent::Event(Event::PropertyChanged(
                                    PropertyChanged {
                                        device: write.device,
                                        key: write.property,
                                        value: write.value,
                                    },
                                )));
                        }
                    }
                    result = Value::Map(BTreeMap::from([("remuxed".into(), Value::Bool(remuxed))]));
                }
                _ => {}
            }
        }
        self.events.push_back(DriverEvent::TokenCompleted {
            token,
            value: result,
        });
        Ok(token)
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        while let Ok(event) = self.worker_rx.try_recv() {
            self.events.push_back(event);
        }
        self.events.drain(..).collect()
    }
}

fn usb_identity_value(sensor_width: u32, sensor_height: u32) -> Value {
    Value::Map(BTreeMap::from([
        (
            "vendor_ids".into(),
            Value::List(
                vendor_ids()
                    .into_iter()
                    .map(|vid| Value::I64(vid as i64))
                    .collect(),
            ),
        ),
        ("image_endpoint".into(), Value::I64(EP_IMAGE as i64)),
        (
            "sensor_width".into(),
            Value::PixelCount(PixelCount::new(sensor_width)),
        ),
        (
            "sensor_height".into(),
            Value::PixelCount(PixelCount::new(sensor_height)),
        ),
    ]))
}

fn feature_summary_value() -> Value {
    Value::Map(BTreeMap::from([
        (
            "evidence".into(),
            Value::String("OpenGEL clean-room USB backend plus register fixture".into()),
        ),
        ("exposure_register_sequence".into(), Value::Bool(true)),
        ("gain_register_sequence".into(), Value::Bool(true)),
        ("roi".into(), Value::Bool(true)),
        ("binning".into(), Value::Bool(true)),
        ("white_balance".into(), Value::Bool(true)),
        ("black_level".into(), Value::Bool(true)),
        ("trigger_sink".into(), Value::Bool(true)),
        (
            "trigger_modes".into(),
            Value::List(
                ["software", "external", "bulb"]
                    .into_iter()
                    .map(|mode| Value::String(mode.into()))
                    .collect(),
            ),
        ),
        (
            "hardware_validation_work".into(),
            Value::List(
                [
                    "model-specific feature probing",
                    "pixel-format/debayer validation",
                    "stream cancellation and backpressure",
                ]
                .into_iter()
                .map(|item| Value::String(item.into()))
                .collect(),
            ),
        ),
    ]))
}

#[derive(Debug, Clone, Copy)]
enum RawRegisterRequest {
    Read { index: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ToupcamTriggerAction {
    Pulse,
    SetMode(ToupcamTriggerMode),
}

impl ToupcamTriggerAction {
    fn name(&self) -> &'static str {
        match self {
            ToupcamTriggerAction::Pulse => "pulse",
            ToupcamTriggerAction::SetMode(_) => "set_mode",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToupcamTriggerMode {
    Software,
    External,
}

impl ToupcamTriggerMode {
    fn to_string(self) -> String {
        match self {
            ToupcamTriggerMode::Software => "software",
            ToupcamTriggerMode::External => "external",
        }
        .into()
    }
}

fn initial_control_registers(exposure_s: f64, gain_percent: u16) -> BTreeMap<u16, u16> {
    exposure_registers(seconds_to_us(exposure_s))
        .into_iter()
        .chain(gain_registers(gain_percent))
        .collect()
}

fn raw_register_result(operation: &str, index: u16, value: u16, completion: &str) -> Value {
    Value::Map(BTreeMap::from([
        (
            "protocol".into(),
            Value::String("toupcam.usb_control_fixture".into()),
        ),
        ("operation".into(), Value::String(operation.into())),
        ("w_index".into(), Value::I64(index as i64)),
        (
            "w_index_hex".into(),
            Value::String(format!("0x{index:04x}")),
        ),
        ("w_value".into(), Value::I64(value as i64)),
        (
            "w_value_hex".into(),
            Value::String(format!("0x{value:04x}")),
        ),
        (
            "provenance".into(),
            Value::String("clean-room exposure/gain register fixture".into()),
        ),
        ("completion".into(), Value::String(completion.into())),
    ]))
}

fn parse_raw_register_request(request: &CapabilityRequest) -> Result<RawRegisterRequest> {
    let CapabilityRequest::GenericCommand(request) = request else {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            "Toupcam RawRegisterAccess expects GenericCommand",
        ));
    };
    raw_register_request_from_generic(request)
}

fn parse_owned_raw_register_request(request: CapabilityRequest) -> Result<RawRegisterRequest> {
    let CapabilityRequest::GenericCommand(request) = request else {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            "Toupcam RawRegisterAccess expects GenericCommand",
        ));
    };
    raw_register_request_from_generic(&request)
}

fn parse_toupcam_trigger_action(request: &CapabilityRequest) -> Result<ToupcamTriggerAction> {
    match request {
        CapabilityRequest::None => Ok(ToupcamTriggerAction::Pulse),
        CapabilityRequest::Trigger(request) => match request.action {
            TriggerAction::Enable => {
                Ok(ToupcamTriggerAction::SetMode(ToupcamTriggerMode::External))
            }
            TriggerAction::Disable => {
                Ok(ToupcamTriggerAction::SetMode(ToupcamTriggerMode::Software))
            }
            TriggerAction::Pulse => Ok(ToupcamTriggerAction::Pulse),
        },
        _ => Err(Error::new(
            ErrorCode::Unsupported,
            "Toupcam TriggerSink expects None or CapabilityRequest::Trigger",
        )),
    }
}

fn parse_owned_toupcam_trigger_action(request: CapabilityRequest) -> Result<ToupcamTriggerAction> {
    parse_toupcam_trigger_action(&request)
}

fn raw_register_request_from_generic(
    request: &GenericCommandRequest,
) -> Result<RawRegisterRequest> {
    if request.is_hidden_maintenance() {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            format!(
                "GenericCommand {} is a hidden maintenance operation",
                request.command
            ),
        ));
    }
    let index = request
        .params
        .get("w_index")
        .or_else(|| request.params.get("index"))
        .or_else(|| request.params.get("address"))
        .ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidCommand,
                "Toupcam raw access missing index",
            )
        })
        .and_then(value_u16)?;
    match request.command.as_str() {
        "read" | "read_register" | "ReadRegister" => Ok(RawRegisterRequest::Read { index }),
        "write" | "write_register" | "WriteRegister" => Err(Error::new(
            ErrorCode::Unsupported,
            "Toupcam raw register writes are hidden without a named safe control surface",
        )),
        other => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("unsupported Toupcam raw command {other}"),
        )),
    }
}

fn value_u16(value: &Value) -> Result<u16> {
    match value {
        Value::I64(value) if *value >= 0 && *value <= u16::MAX as i64 => Ok(*value as u16),
        Value::String(value) => parse_u16(value),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("expected u16 Toupcam raw value, got {value:?}"),
        )),
    }
}

#[derive(Debug, Clone)]
struct EncodedToupcamFrame {
    width: u32,
    height: u32,
    pixel_format: String,
    data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BayerPhase {
    Unknown,
    Rggb,
    Grbg,
    Gbrg,
    Bggr,
}

impl BayerPhase {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "Unknown" | "unknown" | "" => Ok(Self::Unknown),
            "Rggb" | "RGGB" | "rggb" => Ok(Self::Rggb),
            "Grbg" | "GRBG" | "grbg" => Ok(Self::Grbg),
            "Gbrg" | "GBRG" | "gbrg" => Ok(Self::Gbrg),
            "Bggr" | "BGGR" | "bggr" => Ok(Self::Bggr),
            other => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unsupported Toupcam bayer_phase {other}"),
            )),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Rggb => "Rggb",
            Self::Grbg => "Grbg",
            Self::Gbrg => "Gbrg",
            Self::Bggr => "Bggr",
        }
    }
}

fn supported_toupcam_pixel_format(value: &str) -> Option<&'static str> {
    match canonical_image_encoding_name(value)? {
        "Native" => Some(ImageEncoding::Native.property_value()),
        "Raw8" => Some(ImageEncoding::Raw8.property_value()),
        "Mono8" => Some(ImageEncoding::Mono8.property_value()),
        "Rgb8" => Some(ImageEncoding::Rgb8.property_value()),
        "Bgr8" => Some(ImageEncoding::Bgr8.property_value()),
        _ => None,
    }
}

fn encode_raw8_frame(
    data: Vec<u8>,
    width: u32,
    height: u32,
    requested: ImageEncoding,
    configured: &str,
    bayer_phase: BayerPhase,
    source_is_bayer: bool,
) -> Result<EncodedToupcamFrame> {
    let requested = match requested {
        ImageEncoding::Native => configured,
        ImageEncoding::Raw8 => ImageEncoding::Raw8.property_value(),
        ImageEncoding::Mono8 => ImageEncoding::Mono8.property_value(),
        ImageEncoding::Rgb8 => ImageEncoding::Rgb8.property_value(),
        ImageEncoding::Bgr8 => ImageEncoding::Bgr8.property_value(),
        ImageEncoding::Mono16 | ImageEncoding::Raw16 => {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Toupcam live source currently provides 8-bit frames only",
            ))
        }
    };

    match requested {
        "Native" | "Raw8" | "Mono8" => Ok(EncodedToupcamFrame {
            width,
            height,
            pixel_format: if requested == "Native" {
                ImageEncoding::Raw8.property_value().into()
            } else {
                requested.into()
            },
            data,
        }),
        "Rgb8" | "Bgr8" if source_is_bayer => {
            let phase = if bayer_phase == BayerPhase::Unknown {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    "Toupcam RGB/BGR output requires configured bayer_phase",
                ));
            } else {
                bayer_phase
            };
            let (out_width, out_height, rgb) =
                debayer_half(&data, width as usize, height as usize, phase);
            let data = if requested == "Bgr8" {
                rgb_to_bgr(rgb)
            } else {
                rgb
            };
            Ok(EncodedToupcamFrame {
                width: out_width as u32,
                height: out_height as u32,
                pixel_format: requested.into(),
                data,
            })
        }
        "Rgb8" | "Bgr8" => {
            let mut out = Vec::with_capacity(data.len() * 3);
            for byte in data {
                if requested == "Bgr8" {
                    out.extend_from_slice(&[byte, byte, byte]);
                } else {
                    out.extend_from_slice(&[byte, byte, byte]);
                }
            }
            Ok(EncodedToupcamFrame {
                width,
                height,
                pixel_format: requested.into(),
                data: out,
            })
        }
        other => Err(Error::new(
            ErrorCode::Unsupported,
            format!("unsupported Toupcam pixel format {other}"),
        )),
    }
}

fn apply_image_processing(
    encoded: &mut EncodedToupcamFrame,
    black_level: i64,
    white_balance_red_percent: i64,
    white_balance_blue_percent: i64,
) {
    let black_level = black_level.clamp(0, 255) as u8;
    match encoded.pixel_format.as_str() {
        "Mono8" => {
            for byte in &mut encoded.data {
                *byte = byte.saturating_sub(black_level);
            }
        }
        "Rgb8" => apply_rgb_processing(
            &mut encoded.data,
            black_level,
            white_balance_red_percent,
            white_balance_blue_percent,
            false,
        ),
        "Bgr8" => apply_rgb_processing(
            &mut encoded.data,
            black_level,
            white_balance_red_percent,
            white_balance_blue_percent,
            true,
        ),
        _ => {}
    }
}

fn apply_rgb_processing(
    data: &mut [u8],
    black_level: u8,
    white_balance_red_percent: i64,
    white_balance_blue_percent: i64,
    bgr: bool,
) {
    let red_scale = (white_balance_red_percent.clamp(50, 200) as f64) / 100.0;
    let blue_scale = (white_balance_blue_percent.clamp(50, 200) as f64) / 100.0;
    for pixel in data.chunks_exact_mut(3) {
        let red_index = if bgr { 2 } else { 0 };
        let blue_index = if bgr { 0 } else { 2 };
        pixel[red_index] = scale_after_black(pixel[red_index], black_level, red_scale);
        pixel[1] = pixel[1].saturating_sub(black_level);
        pixel[blue_index] = scale_after_black(pixel[blue_index], black_level, blue_scale);
    }
}

fn scale_after_black(byte: u8, black_level: u8, scale: f64) -> u8 {
    ((byte.saturating_sub(black_level) as f64) * scale)
        .round()
        .clamp(0.0, 255.0) as u8
}

fn debayer_half(
    raw: &[u8],
    width: usize,
    height: usize,
    phase: BayerPhase,
) -> (usize, usize, Vec<u8>) {
    let out_width = width / 2;
    let out_height = height / 2;
    let mut out = vec![0u8; out_width * out_height * 3];
    let at = |x: usize, y: usize| raw[y * width + x] as u32;
    for y in 0..out_height {
        for x in 0..out_width {
            let sx = x * 2;
            let sy = y * 2;
            let p00 = at(sx, sy);
            let p01 = at(sx + 1, sy);
            let p10 = at(sx, sy + 1);
            let p11 = at(sx + 1, sy + 1);
            let (r, g, b) = match phase {
                BayerPhase::Rggb => (p00, (p01 + p10) / 2, p11),
                BayerPhase::Grbg => (p01, (p00 + p11) / 2, p10),
                BayerPhase::Gbrg => (p10, (p00 + p11) / 2, p01),
                BayerPhase::Bggr => (p11, (p01 + p10) / 2, p00),
                BayerPhase::Unknown => (p00, p00, p00),
            };
            let out_index = (y * out_width + x) * 3;
            out[out_index] = r as u8;
            out[out_index + 1] = g as u8;
            out[out_index + 2] = b as u8;
        }
    }
    (out_width, out_height, out)
}

fn rgb_to_bgr(mut rgb: Vec<u8>) -> Vec<u8> {
    for pixel in rgb.chunks_exact_mut(3) {
        pixel.swap(0, 2);
    }
    rgb
}

fn parse_u16(value: &str) -> Result<u16> {
    let raw = value.trim();
    let parsed = if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        u16::from_str_radix(hex, 16)
    } else {
        raw.parse::<u16>()
    };
    parsed.map_err(|_| {
        Error::new(
            ErrorCode::InvalidCommand,
            format!("invalid Toupcam raw u16 value {value}"),
        )
    })
}

fn seconds_to_us(seconds: f64) -> u32 {
    (seconds * 1_000_000.0).round().clamp(1.0, u32::MAX as f64) as u32
}

fn toupcam_min_exposure_s() -> f64 {
    LINE_TIME_US / 1_000_000.0
}

fn toupcam_max_exposure_s() -> f64 {
    MAX_EXPOSURE_LINES as f64 * LINE_TIME_US / 1_000_000.0
}

#[cfg(feature = "os-usb")]
mod live_toupcam {
    use super::*;
    use futures_lite::future::block_on;
    use nusb::transfer::{ControlIn, ControlOut, ControlType, Recipient, RequestBuffer};
    use nusb::Interface;
    use serde::Deserialize;
    use std::sync::mpsc::{channel, RecvTimeoutError};
    use std::time::{Duration, Instant};

    /// Vendor request that starts and stops the image stream; `wValue` selects
    /// the mode, `0x0000` stops. Recorded on both model captures.
    const REQ_STREAM: u8 = 0x01;
    const STREAM_INDEX: u16 = 0x000f;
    const STREAM_STOP: u16 = 0x0000;

    #[derive(Debug, Clone)]
    pub struct LiveToupcamInfo {
        pub label: String,
        pub(super) identity: ToupcamUsbIdentity,
        pub(super) model: Option<ToupcamModel>,
    }

    impl LiveToupcamInfo {
        /// The model profile matched from the USB product id, if this driver has
        /// a recorded open sequence for it.
        pub fn model(&self) -> Option<ToupcamModel> {
            self.model
        }
    }

    pub struct LiveToupcam {
        iface: Interface,
        model: ToupcamModel,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct Step {
        dir: String,
        #[serde(rename = "bRequest", deserialize_with = "hex_u8")]
        b_request: u8,
        #[serde(rename = "wValue", deserialize_with = "hex_u16")]
        w_value: u16,
        #[serde(rename = "wIndex", deserialize_with = "hex_u16")]
        w_index: u16,
        #[serde(rename = "wLength")]
        w_length: u16,
        #[serde(default)]
        data: String,
    }

    fn hex_u8<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<u8, D::Error> {
        let s = String::deserialize(d)?;
        u8::from_str_radix(s.trim_start_matches("0x"), 16).map_err(serde::de::Error::custom)
    }

    fn hex_u16<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<u16, D::Error> {
        let s = String::deserialize(d)?;
        u16::from_str_radix(s.trim_start_matches("0x"), 16).map_err(serde::de::Error::custom)
    }

    fn hex_bytes(s: &str) -> Vec<u8> {
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap_or(0))
            .collect()
    }

    fn usb_error(message: impl Into<String>) -> Error {
        Error::new(ErrorCode::Transport, message.into())
    }

    pub fn list_cameras() -> Result<Vec<LiveToupcamInfo>> {
        let devices = nusb::list_devices().map_err(|error| usb_error(error.to_string()))?;
        Ok(devices
            .filter(|device| is_toupcam_vendor(device.vendor_id()))
            .map(|device| {
                let product = device.product_string().unwrap_or("Toupcam USB Camera");
                let serial = device.serial_number().unwrap_or("");
                let serial_suffix = if serial.is_empty() {
                    String::new()
                } else {
                    format!(" serial {serial}")
                };
                let label = format!(
                    "Toupcam {product} {:04x}:{:04x} bus {} addr {}{}",
                    device.vendor_id(),
                    device.product_id(),
                    device.bus_number(),
                    device.device_address(),
                    serial_suffix
                );
                LiveToupcamInfo {
                    label: label.clone(),
                    identity: ToupcamUsbIdentity {
                        label,
                        product: product.into(),
                        serial: if serial.is_empty() {
                            None
                        } else {
                            Some(serial.into())
                        },
                        vendor_id: device.vendor_id(),
                        product_id: device.product_id(),
                        bus_number: device.bus_number(),
                        device_address: device.device_address(),
                    },
                    model: model_for_product_id(device.product_id()),
                }
            })
            .collect())
    }

    impl LiveToupcam {
        pub fn open(index: usize) -> Result<Self> {
            let device = nusb::list_devices()
                .map_err(|error| usb_error(error.to_string()))?
                .filter(|device| is_toupcam_vendor(device.vendor_id()))
                .nth(index)
                .ok_or_else(|| {
                    Error::new(
                        ErrorCode::Transport,
                        "no Toupcam USB device found for configured index",
                    )
                })?;
            let (vendor_id, product_id) = (device.vendor_id(), device.product_id());
            let model = model_for_product_id(product_id).ok_or_else(|| {
                let known = models()
                    .iter()
                    .map(|m| format!("{} (0x{:04x})", m.model, m.product_id))
                    .collect::<Vec<_>>()
                    .join(", ");
                match identity_for_product_id(product_id) {
                    Some(id) => Error::new(
                        ErrorCode::Unsupported,
                        format!(
                            "Toupcam {model} (0x{product_id:04x}{geometry}) is in the camera \
                             catalogue but this driver has no profile for it, so it cannot be \
                             streamed. Add one: either its sensor register map, or a recorded \
                             open sequence. Streamable models: {known}",
                            model = id.model,
                            geometry = match id.geometry {
                                Some((w, h)) => format!(", {w}x{h}"),
                                None => String::new(),
                            }
                        ),
                    ),
                    None => Error::new(
                        ErrorCode::Unsupported,
                        format!(
                            "no Toupcam profile for product id 0x{product_id:04x}, and it is not \
                             in the camera catalogue. Streamable models: {known}"
                        ),
                    ),
                }
            })?;
            let device = device.open().map_err(|error| {
                usb_error(format!(
                    "open failed; another application may hold the camera: {error}"
                ))
            })?;
            let iface = device.detach_and_claim_interface(0).map_err(|error| {
                usb_error(format!(
                    "claim interface 0 failed: {error}{}",
                    crate::usb_discovery::usb_claim_hint(vendor_id, product_id, 0)
                ))
            })?;
            let _ = iface.set_alt_setting(0);
            let live = Self { iface, model };
            live.init()?;
            Ok(live)
        }

        /// The model profile this handle was opened against.
        pub fn model(&self) -> ToupcamModel {
            self.model
        }

        fn vendor_out(&self, request: u8, value: u16, index: u16, data: &[u8]) -> Result<()> {
            block_on(self.iface.control_out(ControlOut {
                control_type: ControlType::Vendor,
                recipient: Recipient::Device,
                request,
                value,
                index,
                data,
            }))
            .into_result()
            .map(|_| ())
            .map_err(|error| {
                usb_error(format!(
                    "control_out req=0x{request:02x} val=0x{value:04x} idx=0x{index:04x}: {error}"
                ))
            })
        }

        fn vendor_in(&self, request: u8, value: u16, index: u16, length: u16) -> Result<Vec<u8>> {
            block_on(self.iface.control_in(ControlIn {
                control_type: ControlType::Vendor,
                recipient: Recipient::Device,
                request,
                value,
                index,
                length,
            }))
            .into_result()
            .map_err(|error| {
                usb_error(format!(
                    "control_in req=0x{request:02x} val=0x{value:04x} idx=0x{index:04x}: {error}"
                ))
            })
        }

        /// Announce the session token and wait for the device to report ready.
        ///
        /// The token selects the mask applied to register operands; this driver
        /// always sends [`SESSION_TOKEN`], which selects the identity mask, so
        /// every later register transfer is plaintext.
        fn probe(&self) -> Result<()> {
            let deadline = Instant::now() + Duration::from_millis(2_000);
            let mut last = None;
            while Instant::now() < deadline {
                match self.vendor_in(REQ_PROBE, SESSION_TOKEN, 0x0000, 2) {
                    Ok(data) if data.first() == Some(&PROBE_READY) => return Ok(()),
                    Ok(data) => last = Some(format!("probe returned {data:02x?}")),
                    Err(error) => last = Some(error.to_string()),
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(usb_error(format!(
                "Toupcam {} did not become ready: {}",
                self.model.model,
                last.unwrap_or_else(|| "no response".into())
            )))
        }

        /// Write one sensor register. Operands ride in the setup packet and the
        /// returned byte is status, so a write is still an IN transfer.
        fn write_reg(&self, register: u16, value: u16) -> Result<()> {
            self.vendor_in(REQ_REGISTER, value, register, 1).map(|_| ())
        }

        fn set_streaming(&self, on: bool) -> Result<()> {
            let value = if on { STREAM_START } else { STREAM_STOP };
            self.vendor_out(REQ_STREAM, value, STREAM_INDEX, &[])
        }

        /// Bring the sensor up from the specification: fixed init table, then
        /// the window and timing registers for the full frame.
        fn init_sensor(&self, profile: &SensorProfile) -> Result<()> {
            let _ = self.set_streaming(false);
            self.probe()?;
            for step in profile.init {
                match *step {
                    InitStep::Reg(register, value) => self.write_reg(register, value)?,
                    InitStep::DelayMs(ms) => std::thread::sleep(Duration::from_millis(ms)),
                }
            }
            // Window and readout, still in standby.
            for (register, value) in [
                (REG_X_ODD_INC, 0x0001),
                (REG_Y_ODD_INC, 0x0001),
                (REG_X_ADDR_START, profile.x_addr_start),
                (REG_X_ADDR_END, profile.x_addr_end),
                (REG_Y_ADDR_START, profile.y_addr_start),
                (REG_Y_ADDR_END, profile.y_addr_end),
                (REG_FRAME_LENGTH_LINES, profile.frame_length_lines),
                (REG_READ_MODE, 0x0000),
            ] {
                self.write_reg(register, value)?;
            }
            // Row period and PLL are applied under grouped hold, then the sensor
            // is returned to streaming.
            self.write_reg(REG_RESET, RESET_HOLD_A)?;
            self.write_reg(REG_LINE_LENGTH_PCK, profile.line_length_pck)?;
            self.write_reg(REG_RESET, RESET_STREAMING)?;
            self.write_reg(REG_PLL_MULTIPLIER, 0x0093)?;
            self.write_reg(REG_RESET, RESET_HOLD_B)?;

            // The sensor needs a valid integration time and gain before the
            // stream is started; a frame is not produced otherwise.
            let (coarse, _) = coarse_integration_time(DEFAULT_EXPOSURE_US, &profile);
            self.write_reg(REG_COARSE_INTEGRATION_TIME, coarse)?;
            self.write_reg(REG_ANALOG_GAIN, analog_gain_code(100))?;

            let _ = self.iface.clear_halt(EP_IMAGE);
            self.set_streaming(true)?;
            Ok(())
        }

        fn init(&self) -> Result<()> {
            match self.model.open {
                ToupcamOpen::Sensor(profile) => return self.init_sensor(&profile),
                ToupcamOpen::Replay(script) => self.replay(script),
            }
        }

        fn replay(&self, script: &str) -> Result<()> {
            // A previous open may have left the stream running, which would make
            // the first bulk reads land mid-frame. Stop before replaying.
            let _ = self.vendor_out(REQ_STREAM, STREAM_STOP, STREAM_INDEX, &[]);
            for (lineno, line) in script.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let step: Step = serde_json::from_str(line).map_err(|error| {
                    Error::new(
                        ErrorCode::Driver,
                        format!("Toupcam init sequence line {}: {error}", lineno + 1),
                    )
                })?;
                if step.b_request == 0x37 && step.dir == "in" {
                    for _ in 0..200 {
                        match self.vendor_in(0x37, step.w_value, step.w_index, step.w_length) {
                            Ok(data) if data.first().map(|byte| byte & 1 == 1).unwrap_or(false) => {
                                break;
                            }
                            _ => std::thread::sleep(Duration::from_millis(5)),
                        }
                    }
                    continue;
                }
                let result = if step.dir == "out" {
                    self.vendor_out(
                        step.b_request,
                        step.w_value,
                        step.w_index,
                        &hex_bytes(&step.data),
                    )
                } else {
                    self.vendor_in(step.b_request, step.w_value, step.w_index, step.w_length)
                        .map(|_| ())
                };
                if result.is_err() {
                    continue;
                }
            }
            let _ = self.iface.clear_halt(EP_IMAGE);
            Ok(())
        }

        /// Fails for models with no specified sensor register map, rather than
        /// writing another sensor's registers to this one.
        fn require_tunable(&self, what: &str) -> Result<SensorProfile> {
            self.model.sensor().ok_or_else(|| {
                Error::new(
                    ErrorCode::Unsupported,
                    format!(
                        "Toupcam {}: {what} cannot be set because this model's sensor register \
                         map is not specified; it is opened by replaying a recorded vendor \
                         sequence and stays at the state that reproduces.",
                        self.model.model
                    ),
                )
            })
        }

        pub fn set_exposure_us(&self, us: u32) -> Result<()> {
            let profile = self.require_tunable("exposure")?;
            let (coarse, line_length_pck) = coarse_integration_time(us, &profile);
            // Long exposures stretch the row period instead of overflowing the
            // 16-bit integration-time field; only write it when it changes.
            if line_length_pck != profile.line_length_pck {
                self.write_reg(REG_LINE_LENGTH_PCK, line_length_pck)?;
            }
            self.write_reg(REG_COARSE_INTEGRATION_TIME, coarse)?;
            Ok(())
        }

        pub fn set_gain_percent(&self, percent: u16) -> Result<()> {
            self.require_tunable("gain")?;
            self.write_reg(REG_ANALOG_GAIN, analog_gain_code(percent))
        }

        /// Reads one frame's worth of pixel bytes.
        ///
        /// Reads the model's trailer bytes too and drops them, so the next read
        /// still starts on a frame boundary instead of drifting by the trailer
        /// length on every frame.
        pub fn read_frame(&self, expected_bytes: usize) -> Result<Vec<u8>> {
            let wire_bytes = expected_bytes + self.model.frame_trailer_bytes;
            let iface = self.iface.clone();
            let (tx, rx) = channel::<std::result::Result<Vec<u8>, String>>();
            std::thread::spawn(move || {
                let mut queue = iface.bulk_in_queue(EP_IMAGE);
                for _ in 0..16 {
                    queue.submit(RequestBuffer::new(BULK_CHUNK));
                }
                loop {
                    let completion = block_on(queue.next_complete());
                    let message = completion
                        .status
                        .map(|_| completion.data.clone())
                        .map_err(|error| error.to_string());
                    let stop = message.is_err();
                    if tx.send(message).is_err() || stop {
                        return;
                    }
                    queue.submit(RequestBuffer::new(BULK_CHUNK));
                }
            });
            // The device delimits frames with a short bulk transfer. Reading a
            // fixed byte count from wherever the stream happens to be returns a
            // torn frame (the camera is free-running and this read starts at an
            // arbitrary offset), so segment on that delimiter and keep the first
            // segment that holds a whole frame. Partial leading segments — and a
            // trailer that arrives in its own transfer — are discarded.
            let mut frame = Vec::with_capacity(wire_bytes + BULK_CHUNK);
            let deadline = Instant::now() + Duration::from_millis(15_000);
            loop {
                let now = Instant::now();
                if now >= deadline {
                    return Err(usb_error(format!(
                        "Toupcam {} frame read timed out ({} of {wire_bytes} bytes)",
                        self.model.model,
                        frame.len(),
                    )));
                }
                match rx.recv_timeout(deadline - now) {
                    Ok(Ok(data)) => {
                        let short = data.len() < BULK_CHUNK;
                        frame.extend_from_slice(&data);
                        if frame.len() >= wire_bytes {
                            frame.truncate(expected_bytes);
                            return Ok(frame);
                        }
                        if short {
                            // Frame boundary reached with too little data: this
                            // was a partial frame or a lone trailer. Resynchronize.
                            frame.clear();
                        }
                    }
                    Ok(Err(error)) => return Err(usb_error(format!("bulk stream error: {error}"))),
                    Err(RecvTimeoutError::Timeout) => {
                        return Err(usb_error(format!(
                            "short Toupcam {} frame: {} of {wire_bytes} bytes",
                            self.model.model,
                            frame.len(),
                        )));
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        return Err(usb_error("bulk stream thread ended"));
                    }
                }
            }
        }
    }
}

fn capability(
    id: u64,
    device: DeviceId,
    kind: CapabilityKind,
    request: ValueType,
    response: ValueType,
) -> CapabilityDescriptor {
    let name = kind.name().to_string();
    CapabilityDescriptor {
        id: CapabilityId(id),
        device,
        kind,
        name,
        request,
        response,
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
        unit: unit.map(|u| Unit(u.to_string())),
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

fn property_range(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    unit: Option<&str>,
    writable: bool,
    min: Value,
    max: Value,
) -> PropertySchema {
    let mut property = property(key, display_name, value_type, unit, writable);
    property.range = Some(Range { min, max });
    property.sequenceable = matches!(key, "exposure" | "gain");
    property
}

fn property_enum<const N: usize>(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    unit: Option<&str>,
    writable: bool,
    values: [&str; N],
) -> PropertySchema {
    let mut property = property(key, display_name, value_type, unit, writable);
    property.enum_values = values
        .into_iter()
        .map(|value| EnumValue {
            value: match value_type {
                ValueType::I64 => Value::I64(value.parse().unwrap_or_default()),
                _ => Value::String(value.into()),
            },
            label: value.into(),
        })
        .collect();
    property.sequenceable = key == "pixel_format";
    property
}

fn time_interval(seconds: f64) -> Value {
    Value::TimeInterval(TimeInterval::from_seconds(seconds))
}

fn time_seconds(value: &Value) -> Result<f64> {
    match value {
        Value::TimeInterval(interval) => Ok(interval.seconds()),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            "expected typed time interval value",
        )),
    }
}

fn public_camera_key(key: &str) -> &str {
    match key {
        "exposure_s" => "exposure",
        "gain_percent" => "gain",
        "sensor_temperature_c" => "sensor_temperature",
        "white_balance_red_percent" => "white_balance_red",
        "white_balance_blue_percent" => "white_balance_blue",
        _ => key,
    }
}

fn string_prop(device: &DeviceConfig, key: &str) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
        _ => None,
    }
}

fn optional_string_prop(
    device: &DeviceConfig,
    key: &str,
    current: Option<String>,
) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) if value.is_empty() || value.eq_ignore_ascii_case("none") => {
            None
        }
        Some(Value::String(value)) => Some(value.clone()),
        _ => current,
    }
}

fn bool_prop(device: &DeviceConfig, key: &str) -> Option<bool> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn i64_prop(device: &DeviceConfig, key: &str) -> Option<i64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => Some(*value),
        _ => None,
    }
}

fn pixel_count_prop(device: &DeviceConfig, key: &str) -> Result<Option<u32>> {
    match device.properties.get(key) {
        Some(Value::PixelCount(value)) => Ok(Some(value.pixels().max(1))),
        Some(Value::I64(value)) if *value > 0 => Ok(Some(*value as u32)),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Toupcam {key} expects PixelCount"),
        )),
        None => Ok(None),
    }
}

fn time_interval_prop(device: &DeviceConfig, key: &str) -> Result<Option<TimeInterval>> {
    match device.properties.get(key) {
        Some(Value::TimeInterval(value)) => Ok(Some(*value)),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Toupcam {key} expects TimeInterval"),
        )),
        None => Ok(None),
    }
}

fn ratio_prop(device: &DeviceConfig, key: &str) -> Result<Option<Ratio>> {
    match device.properties.get(key) {
        Some(Value::Ratio(value)) => Ok(Some(*value)),
        Some(Value::I64(value)) if *value >= 0 => Ok(Some(Ratio::from_percent(*value as f64))),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Toupcam {key} expects Ratio"),
        )),
        None => Ok(None),
    }
}
