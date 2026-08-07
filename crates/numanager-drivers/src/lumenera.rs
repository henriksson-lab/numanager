//! Bio-Rad Gel Doc EZ camera — Lumenera **Lu130** (Sony ICX205, 12-bit mono).
//!
//! This is a two-stage EZ-USB device. Cold, it enumerates as an anchor-download
//! **loader**; after its 8051 firmware is downloaded it renumerates as the
//! **imaging** camera. Both stages and the firmware-download sequence are
//! live-confirmed (2026-08-03) against real hardware.
//!
//! | Stage   | VID    | PID    | Notes |
//! |---------|--------|--------|-------|
//! | loader  | 0x5354 | 0x809A | EZ-USB anchor download (`0xA0`) |
//! | imaging | 0x5354 | 0x009A | product string "Lu130" |
//!
//! `0x5354` is what *this* Bio-Rad OEM unit reports; stock Lumenera cameras use
//! the registered USB-IF vendor id `0x1724`, so the driver claims both.
//!
//! ## What is and isn't implemented
//!
//! Implemented and evidenced: USB discovery of both stages and the two-stage
//! **firmware download** (validated on hardware 2026-08-03).
//!
//! Bring-up has three further stages, all recorded from captured traffic and
//! all implemented, none yet shown to produce a frame:
//!
//! 1. **Sensor-pipeline configuration** — a 98 KB image streamed to bulk
//!    endpoint `0x08` under alternate setting 1, bracketed by arm/finish writes
//!    on `wIndex 0x0008`. The device reports `0x80` when ready to receive it and
//!    steps `0x40` -> `0x00` as it takes it; a device already carrying an image
//!    reports `0xA0` and refuses another. Hardware-confirmed accepted.
//! 2. **Register load** — 510 recorded transfers, mostly 8-bit writes on
//!    `wIndex 0x0006` addressed by `wValue`. Replayed as recorded: the layout is
//!    understood but individual register meanings are not, and inventing names
//!    for them would be worse than replaying them.
//! 3. **Acquisition** — the sequence below.
//!
//! A run against hardware on 2026-08-05, before stages 1 and 2 existed, accepted
//! every control transfer and received 0 image bytes. Capture is therefore
//! **experimental and not hardware-validated**: the stages are evidenced
//! individually, the chain has not yet been shown to yield a frame.
//!
//! The captured acquisition is: configure geometry/exposure, select alternate
//! setting 2, arm, start, drain one frame off bulk endpoint `0x86`, stop,
//! restore alternate setting 0. A frame is 16 bits per pixel over the binned
//! dimensions — inferred from the vendor trace's bulk byte count being an exact
//! multiple of `1392 * 1040 * 2`. Numanager has not yet reproduced that frame.
//!
//! **Not evidenced:** `gain`. The sequence writes registers `0x0276`-`0x027b`
//! (four equal values then two others, consistent with per-tap gain/offset on
//! a dual-tap sensor), but nothing recorded maps them to a canonical unit, so
//! gain writes fail rather than guess. Several configuration steps likewise
//! have unrecorded meaning; they are replayed verbatim and named for their
//! wire index, because omitting them would be as much a guess as renaming them.

use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};

/// VID this Bio-Rad Gel Doc EZ unit enumerates under (live-confirmed).
pub const LUMENERA_OEM_VID: u16 = 0x5354;
/// Registered Lumenera USB-IF vendor id, used by stock (non-OEM) cameras.
pub const LUMENERA_USBIF_VID: u16 = 0x1724;
/// EZ-USB anchor-download loader stage.
pub const LOADER_PID: u16 = 0x809A;
/// Renumerated imaging stage (Lu130).
pub const IMAGING_PID: u16 = 0x009A;

// Sensor geometry is the Sony ICX205 nominal from the Gel Doc EZ guide; the
// exact active ROI must still be read from live descriptors once the imaging
// protocol is decoded.
const SENSOR_NAME: &str = "Sony ICX205";
const SENSOR_BITS: i64 = 12;
// Read off the wire: the dimension write carries 1392 x 1040, and the captured
// bulk byte count is an exact multiple of 1392*1040*2. The 1039 this once
// claimed came from datasheet arithmetic and was one row short.
const SENSOR_WIDTH: u32 = 1392;
const SENSOR_HEIGHT: u32 = 1040;

/// Default exposure if nothing configured one. Mid-range, not a wire value.
#[cfg(feature = "os-usb")]
const DEFAULT_EXPOSURE_US: u32 = 90_000;

/// What of the imaging protocol is recorded, and what is replayed blind.
const PROTOCOL_STATUS: &str =
    "acquisition control sequence, geometry, exposure and frame layout recorded \
     from captured vendor traffic; numanager's 2026-08-05 hardware run accepted \
     the control writes but received 0 image bytes; several configuration steps \
     are replayed verbatim with unrecorded meaning";

/// Why a capture cannot run without a live USB session.
const CAPTURE_REQUIRES_LIVE: &str =
    "Lumenera Lu130 capture requires a live USB session: build with \
     numanager-drivers/os-usb and configure connect=true on an imaging-stage device";

/// Gain remains unevidenced. The acquisition sequence writes registers
/// 0x0276-0x027b (four equal values then two others, consistent with per-tap
/// gain/offset on a dual-tap sensor), but nothing recorded maps those to a
/// canonical unit, so writes are refused rather than guessed.
const GAIN_UNEVIDENCED: &str =
    "Lumenera Lu130 gain control is not evidenced: the per-tap register mapping \
     to a canonical unit has not been recorded";

/// Byte/command encoding for the imaging stage.
///
/// Every tuple here was read off captured hardware traffic (2026-08-05); the
/// steps whose meaning was not recorded are replayed verbatim and named for
/// their wire index rather than given an invented purpose. Implementation
/// detail: never exported and never used from examples.
#[cfg(feature = "os-usb")]
mod protocol {
    /// Property/configuration request.
    pub(super) const REQ_PROPERTY: u8 = 0x12;
    /// Register/FPGA access request.
    pub(super) const REQ_REGISTER: u8 = 0x13;

    /// `wIndex` selectors on [`REQ_PROPERTY`].
    ///
    /// The `0x86` image endpoint maps to the second bulk-IN pin and uses
    /// `0x0218`. The neighboring `0x0214` lifecycle register belongs to the
    /// first bulk-IN pin (`0x82`) and must not be replayed on this path.
    pub(super) const IDX_DIMENSIONS: u16 = 0x400c;
    pub(super) const IDX_BINNING: u16 = 0x4018;
    pub(super) const IDX_FORMAT_MODE: u16 = 0x4010;
    pub(super) const IDX_FORMAT_08: u16 = 0x4008;
    pub(super) const IDX_EXPOSURE: u16 = 0x0540;
    pub(super) const IDX_ACQUISITION: u16 = 0x0218;
    pub(super) const IDX_OPAQUE_05A0: u16 = 0x05a0;
    pub(super) const IDX_OPAQUE_0550: u16 = 0x0550;
    pub(super) const IDX_OPAQUE_0610: u16 = 0x0610;
    pub(super) const IDX_OPAQUE_0670: u16 = 0x0670;

    /// Sensor-pipeline configuration, streamed to [`EP_CONFIG`] during
    /// imaging-stage bring-up. `wIndex 0x0008` is its control/status register.
    ///
    /// Recorded from captured hardware traffic (2026-08-06): with the device at
    /// the imaging id and configuration 1 selected, the host reads `0x0008`
    /// (`0x80`), selects alternate setting 1, writes `0xFFFFFFFF` to `0x0008`,
    /// streams the image to endpoint `0x08`, polls `0x0008` until it reads
    /// zero (`0x80` -> `0x40` -> `0x00`), then writes zero to `0x0008`.
    ///
    /// Until this runs, every control transfer is accepted and the image
    /// endpoint delivers nothing.
    pub(super) const IDX_CONFIG_STATUS: u16 = 0x0008;
    /// The only status in which an arm is accepted: ready to receive an image.
    pub(super) const CONFIG_READY: u32 = 0x80;
    pub(super) const CONFIG_ARM: u32 = 0xFFFF_FFFF;
    pub(super) const CONFIG_FINISH: u32 = 0;
    /// Bulk OUT endpoint carrying the configuration image, and the alternate
    /// setting that exposes it.
    pub(super) const EP_CONFIG: u8 = 0x08;
    pub(super) const ALT_CONFIG: u8 = 1;

    /// `wIndex` selectors on [`REQ_REGISTER`].
    pub(super) const IDX_REGISTER_DATA: u16 = 0x0006;
    pub(super) const IDX_FPGA_WRITE: u16 = 0x0000;
    pub(super) const IDX_CMD_0F: u16 = 0x000f;

    /// Capability registers the camera answers on open, before any
    /// configuration. Four-byte IN transfers on [`REQ_PROPERTY`], in the order
    /// a working stack issues them.
    ///
    /// Read-only, and not part of the capture sequence. `0x1000` and `0x1014`
    /// are live-confirmed (2026-08-05); the rest are read but unidentified, so
    /// they are named for their wire index only.
    pub(super) const IDX_CAPABILITY_READS: [u16; 11] = [
        0x0004, 0x019c, 0x0280, 0x0284, 0x101c, 0x1000, 0x1004, 0x1008, 0x100c, 0x1014, 0x1040,
    ];

    /// `0x1000` reads back as two little-endian `u16` giving width and height.
    /// Read live from a Gel Doc EZ on 2026-08-05: `0x04100570` = 1392 x 1040,
    /// matching both the dimension write on [`IDX_DIMENSIONS`] and the captured
    /// frame size. `0x100c` returns the same pair. **[confirmed]**
    ///
    /// An earlier revision guessed `0x1004` here; the bench readout showed it
    /// holds `0x00080004`, so it is something else.
    pub(super) const IDX_CAPABILITY_DIMENSIONS: u16 = 0x1000;

    /// `0x1014` is a device **state code**, not bit depth. The same camera read
    /// `0x0c` on 2026-08-05 and `0x05` on 2026-08-06 after a driver change, and
    /// a working stack switches on its low byte over a small case set. Reported
    /// raw; an earlier revision read `0x0c` as "12 bpp", which the second
    /// reading showed to be a coincidence.
    pub(super) const IDX_CAPABILITY_STATE: u16 = 0x1014;

    /// Values written to [`IDX_ACQUISITION`], in the order the sequence uses
    /// them. Named for their observed position, not a documented meaning.
    pub(super) const ACQ_STOP: u32 = 0;
    pub(super) const ACQ_ARM: u32 = 4;
    pub(super) const ACQ_START: u32 = 6;

    /// Per-tap registers written on every acquisition.
    pub(super) const REG_TAP_FIRST: u16 = 0x0276;
    pub(super) const REG_TAP_VALUE: u32 = 0x3f;
    pub(super) const REG_TRAILER_A: u16 = 0x027a;
    pub(super) const REG_TRAILER_A_VALUE: u32 = 0x12;
    pub(super) const REG_TRAILER_B: u16 = 0x027b;

    /// The post-stop FPGA write, replayed verbatim.
    pub(super) const FPGA_TEARDOWN_ADDR: u16 = 0x0544;
    pub(super) const FPGA_TEARDOWN_DATA: [u8; 5] = [0x22, 0x0f, 0x00, 0xc2, 0x00];

    /// Bulk IN endpoint carrying image data in the working vendor trace. Alt 2
    /// also exposes `0x82`, but that first bulk-IN pin maps to lifecycle
    /// register `0x0214` and is not the observed image stream.
    pub(super) const EP_IMAGE: u8 = 0x86;
    pub(super) const ALT_STREAMING: u8 = 2;
    pub(super) const ALT_IDLE: u8 = 0;

    /// Constant leading word of the exposure payload.
    const EXPOSURE_FLAG: u32 = 0x8000_0000;

    /// Exposure is `[u32 flag][u32 microseconds]`, both little-endian.
    pub(super) fn exposure(microseconds: u32) -> [u8; 8] {
        let mut out = [0u8; 8];
        out[..4].copy_from_slice(&EXPOSURE_FLAG.to_le_bytes());
        out[4..].copy_from_slice(&microseconds.to_le_bytes());
        out
    }

    /// Width and height as two little-endian `u16`.
    pub(super) fn dimensions(width: u16, height: u16) -> [u8; 4] {
        let mut out = [0u8; 4];
        out[..2].copy_from_slice(&width.to_le_bytes());
        out[2..].copy_from_slice(&height.to_le_bytes());
        out
    }

    /// Horizontal and vertical binning as two little-endian `u16`.
    pub(super) fn binning(x: u16, y: u16) -> [u8; 4] {
        let mut out = [0u8; 4];
        out[..2].copy_from_slice(&x.to_le_bytes());
        out[2..].copy_from_slice(&y.to_le_bytes());
        out
    }

    /// A bare little-endian `u32` payload.
    pub(super) fn word(value: u32) -> [u8; 4] {
        value.to_le_bytes()
    }

    /// An 8-byte payload whose meaning was not recorded; replayed as captured.
    pub(super) fn opaque8(value: u64) -> [u8; 8] {
        value.to_le_bytes()
    }

    /// Bytes one frame occupies on the wire: 16 bits per pixel over the binned
    /// dimensions. Confirmed by the captured byte total being an exact
    /// multiple of this for 1392x1040.
    pub(super) fn frame_bytes(width: u32, height: u32, x_bin: u16, y_bin: u16) -> usize {
        (width as usize / x_bin.max(1) as usize) * (height as usize / y_bin.max(1) as usize) * 2
    }
}

/// The in-tree third-party package: this crate's own source tree if present
/// (an embedding application's cwd is not the repo root), else repo-relative.
fn default_firmware_dir() -> String {
    const IN_TREE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../data/third_party/lumenera"
    );
    if std::path::Path::new(IN_TREE).is_dir() {
        return IN_TREE.to_string();
    }
    "data/third_party/lumenera".to_string()
}

/// USB vendor ids this driver claims (both the OEM and stock Lumenera VIDs).
pub fn usb_vendor_ids() -> Vec<u16> {
    vec![LUMENERA_OEM_VID, LUMENERA_USBIF_VID]
}

/// Whether a `(vid, pid)` is a Lumenera Gel Doc EZ device in either stage.
/// Used by live USB discovery and the tests.
#[cfg(feature = "os-usb")]
fn is_lumenera_candidate(vendor_id: u16, product_id: u16) -> bool {
    matches!(vendor_id, LUMENERA_OEM_VID | LUMENERA_USBIF_VID)
        && matches!(product_id, LOADER_PID | IMAGING_PID)
}

/// The firmware image the loader selects, by its `bcdDevice` (USB REV) field.
/// REV_0001 -> img01 was confirmed to boot the real unit; slot 16 is the
/// documented catch-all for any other selector.
fn firmware_image_file(selector: u16) -> &'static str {
    match selector {
        0 => "lumenera_fw_img00.hex",
        1 => "lumenera_fw_img01.hex",
        _ => "lumenera_fw_img16.hex",
    }
}

/// Intel-HEX text of the image for `selector`: a configured `firmware_dir`
/// wins, otherwise the copy compiled in by [`crate::bundled_firmware`].
fn firmware_image_text(selector: u16, firmware_dir: Option<&str>) -> Result<String> {
    let name = firmware_image_file(selector);
    if let Some(dir) = firmware_dir {
        let file = std::path::Path::new(dir).join(name);
        if let Ok(text) = std::fs::read_to_string(&file) {
            return Ok(text);
        }
        // A stale or partial directory must not stop a camera reloading.
    }
    crate::bundled_firmware::image_by_name(name)
        .map(str::to_string)
        .ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("Lumenera firmware image {name} is neither bundled nor in firmware_dir"),
            )
        })
}

/// A discovered Gel Doc EZ device in one of its two stages.
#[derive(Debug, Clone)]
pub struct LumeneraProbe {
    label: String,
    vendor_id: u16,
    product_id: u16,
    product: String,
    serial_number: Option<String>,
    /// True once renumerated to the imaging PID.
    firmware_loaded: bool,
    /// `bcdDevice`, selects the firmware image while in the loader stage.
    image_selector: u16,
    firmware_dir: Option<String>,
    /// Gate for touching hardware (pushing firmware). Off during passive
    /// discovery, mirroring the Andor driver's `connect` gate.
    connect: bool,
    exposure: TimeInterval,
    gain: f64,
    usb: Option<LumeneraUsbIdentity>,
}

#[derive(Debug, Clone)]
struct LumeneraUsbIdentity {
    vendor_id: u16,
    product_id: u16,
    bus_number: u8,
    device_address: u8,
    device_version: u16,
    firmware_loaded: bool,
    serial: Option<String>,
}

impl LumeneraUsbIdentity {
    fn value(&self) -> Value {
        let mut fields = BTreeMap::from([
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
            (
                "device_version".into(),
                Value::I64(self.device_version as i64),
            ),
            ("firmware_loaded".into(), Value::Bool(self.firmware_loaded)),
        ]);
        if let Some(serial) = &self.serial {
            fields.insert("serial".into(), Value::String(serial.clone()));
        }
        Value::Map(fields)
    }
}

impl LumeneraProbe {
    /// An imaging-stage fixture (no hardware) for tests and headless demos.
    fn fixture() -> Self {
        Self {
            label: "Configured Lumenera Gel Doc EZ camera".into(),
            vendor_id: LUMENERA_OEM_VID,
            product_id: IMAGING_PID,
            product: "Lu130".into(),
            serial_number: None,
            firmware_loaded: true,
            image_selector: 1,
            firmware_dir: Some(default_firmware_dir()),
            connect: false,
            exposure: TimeInterval::from_milliseconds(50.0),
            gain: 1.0,
            usb: None,
        }
    }

    /// Exposure as the microseconds the wire format carries.
    #[cfg(feature = "os-usb")]
    fn exposure_micros(&self) -> u32 {
        let micros = self.exposure.microseconds();
        if micros.is_finite() && micros >= 1.0 {
            micros.min(u32::MAX as f64) as u32
        } else {
            DEFAULT_EXPOSURE_US
        }
    }

    fn stage(&self) -> &'static str {
        if self.firmware_loaded {
            "imaging"
        } else {
            "firmware-loader"
        }
    }

    fn discovery_label(&self) -> String {
        format!(
            "{} [{}] ({:04x}:{:04x})",
            self.label,
            self.stage(),
            self.vendor_id,
            self.product_id
        )
    }
}

pub struct LumeneraDiscovery {
    next_id: DriverId,
    probes: Vec<LumeneraProbe>,
    /// Caller opt-in to hidden firmware initialization; see
    /// [`LumeneraDiscovery::with_firmware_initialization`].
    connect: bool,
    /// Firmware directory override applied alongside `connect`.
    firmware_dir: Option<String>,
    #[cfg(feature = "os-usb")]
    active_usb: bool,
}

impl LumeneraDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![LumeneraProbe::fixture()],
            connect: false,
            firmware_dir: None,
            #[cfg(feature = "os-usb")]
            active_usb: false,
        }
    }

    /// Allow this discovery to push 8051 firmware to any loader-stage device it
    /// finds, from `firmware_dir` (or the in-tree package when `None`).
    ///
    /// Off by default, and deliberately so: `detect()` runs against whatever is
    /// plugged in, and passive enumeration must never write to a device the
    /// user has not claimed. Live USB discovery has no config file to carry a
    /// `connect` property, so this is how a caller opts in — it forces
    /// `connect` on for every probe this discovery yields, including configured
    /// ones.
    pub fn with_firmware_initialization(mut self, firmware_dir: Option<String>) -> Self {
        self.connect = true;
        self.firmware_dir = firmware_dir;
        self
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| {
                matches!(
                    device.driver.as_str(),
                    "lumenera" | "lumenera_camera" | "lumenera-camera" | "geldoc_ez" | "geldoc-ez"
                )
            })
            .map(LumeneraProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            next_id,
            probes,
            connect: false,
            firmware_dir: None,
            #[cfg(feature = "os-usb")]
            active_usb: false,
        })
    }

    #[cfg(feature = "os-usb")]
    pub fn os_usb(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: Vec::new(),
            connect: false,
            firmware_dir: None,
            active_usb: true,
        }
    }
}

impl LumeneraProbe {
    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = Self::fixture();
        if !device.label.is_empty() {
            probe.label = device.label.clone();
        }
        if let Some(Value::I64(vid)) = device.properties.get("vendor_id") {
            probe.vendor_id = *vid as u16;
        }
        if let Some(Value::I64(pid)) = device.properties.get("product_id") {
            probe.product_id = *pid as u16;
        }
        probe.firmware_loaded = probe.product_id == IMAGING_PID;
        if let Some(Value::String(product)) = device.properties.get("product") {
            probe.product = product.clone();
        }
        if let Some(Value::String(serial)) = device.properties.get("serial_number") {
            probe.serial_number = Some(serial.clone());
        }
        if let Some(Value::String(dir)) = device.properties.get("firmware_dir") {
            probe.firmware_dir = Some(dir.clone());
        }
        if let Some(Value::I64(selector)) = device.properties.get("image_selector") {
            probe.image_selector = *selector as u16;
        }
        if let Some(Value::Bool(connect)) = device.properties.get("connect") {
            probe.connect = *connect;
        }
        if let Some(Value::TimeInterval(exposure)) = device.properties.get("exposure") {
            probe.exposure = *exposure;
        }
        Ok(probe)
    }
}

impl DriverDiscovery for LumeneraDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        #[cfg(not(feature = "os-usb"))]
        let probes = self.probes.clone();
        #[cfg(feature = "os-usb")]
        let mut probes = self.probes.clone();
        #[cfg(feature = "os-usb")]
        if self.active_usb {
            probes.extend(active_usb_probes()?);
        }

        probes
            .iter()
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let mut probe = probe.clone();
                if self.connect {
                    probe.connect = true;
                    if let Some(dir) = &self.firmware_dir {
                        probe.firmware_dir = Some(dir.clone());
                    }
                }
                let mut driver = LumeneraCameraDriver::configured(id, probe.clone());
                driver.initialize_firmware()?;
                Ok(DriverCandidate::from_driver(
                    probe.discovery_label(),
                    Box::new(driver),
                ))
            })
            .collect()
    }
}

#[cfg(feature = "os-usb")]
fn active_usb_probes() -> Result<Vec<LumeneraProbe>> {
    let devices = nusb::list_devices().map_err(|error| {
        Error::new(
            ErrorCode::Transport,
            format!("Lumenera USB device listing failed: {error}"),
        )
    })?;
    Ok(devices
        .filter(|device| is_lumenera_candidate(device.vendor_id(), device.product_id()))
        .map(|device| {
            let vendor_id = device.vendor_id();
            let product_id = device.product_id();
            let firmware_loaded = product_id == IMAGING_PID;
            let serial_number = device.serial_number().map(str::to_string);
            let product = device
                .product_string()
                .map(str::to_string)
                .unwrap_or_else(|| {
                    if firmware_loaded {
                        "Lu130".into()
                    } else {
                        "Lumenera Gel Doc EZ loader".into()
                    }
                });
            LumeneraProbe {
                label: format!(
                    "{} {:04x}:{:04x} bus {} addr {}",
                    product,
                    vendor_id,
                    product_id,
                    device.bus_number(),
                    device.device_address()
                ),
                vendor_id,
                product_id,
                product,
                serial_number: serial_number.clone(),
                firmware_loaded,
                image_selector: device.device_version(),
                firmware_dir: Some(default_firmware_dir()),
                connect: false,
                exposure: TimeInterval::from_milliseconds(50.0),
                gain: 1.0,
                usb: Some(LumeneraUsbIdentity {
                    vendor_id,
                    product_id,
                    bus_number: device.bus_number(),
                    device_address: device.device_address(),
                    device_version: device.device_version(),
                    firmware_loaded,
                    serial: serial_number,
                }),
            }
        })
        .collect())
}

pub struct LumeneraCameraDriver {
    id: DriverId,
    camera: DeviceId,
    transport: ResourceId,
    probe: LumeneraProbe,
    next_token: u64,
    events: VecDeque<DriverEvent>,
    /// The open imaging session, kept between frames.
    ///
    /// Opening one is not cheap — it enumerates the bus, re-asserts the
    /// configuration, re-claims the interface and reads the capability block —
    /// and none of that describes the *frame* being asked for. Doing it per
    /// capture put roughly three quarters of a second in front of every frame,
    /// which reads as a live preview that ignores the exposure it was given.
    /// Dropped on error so the next capture rebuilds it.
    #[cfg(feature = "os-usb")]
    session: Option<live_imaging::ImagingSession>,
}

impl LumeneraCameraDriver {
    pub fn configured(id: DriverId, probe: LumeneraProbe) -> Self {
        Self {
            id,
            camera: DeviceId(NodeId(id.0 * 1000 + 720)),
            transport: ResourceId(NodeId(id.0 * 1000 + 721)),
            probe,
            next_token: 1,
            events: VecDeque::new(),
            #[cfg(feature = "os-usb")]
            session: None,
        }
    }

    fn next_token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    /// Whether this process has been given the device node — on Windows, that
    /// WinUSB is bound to it. Only a deliberate, approved action puts a node in
    /// that state, so it doubles as the consent to write to the device.
    ///
    /// False for a configured probe with no live node, and false where WinUSB
    /// provisioning does not apply; both fall back to the `connect` gate.
    fn host_owns_node(&self) -> bool {
        #[cfg(any(windows, feature = "winusb"))]
        {
            if self.probe.usb.is_none() {
                return false;
            }
            let function =
                crate::winusb_access::UsbFunction::new(self.probe.vendor_id, self.probe.product_id);
            crate::winusb_access::access_state(function)
                .map(|state| state.is_winusb())
                .unwrap_or(false)
        }
        #[cfg(not(any(windows, feature = "winusb")))]
        false
    }

    /// Bring the camera to its imaging stage, pushing firmware if it is still a
    /// loader. Runs from the capture path, where the caller has asked this
    /// specific camera for a frame — authorisation enough to make it usable
    /// even if the node is not one we were handed.
    #[cfg(feature = "os-usb")]
    fn bring_up(&mut self) -> Result<()> {
        if self.probe.firmware_loaded {
            return Ok(());
        }
        let was = self.probe.connect;
        self.probe.connect = true;
        let outcome = self.initialize_firmware();
        self.probe.connect = was;
        outcome?;
        self.probe.product_id = IMAGING_PID;
        Ok(())
    }

    /// Push the 8051 firmware if this is a loader-stage device that is ours to
    /// bring up. A no-op for an already-imaging device.
    ///
    /// "Ours" means the host has bound WinUSB to the node — which only happens
    /// because someone approved it, so the claim and the consent are the same
    /// act. A loader-stage device is useless until its firmware is written, and
    /// the driver that owns the node is the one expected to write it; that is
    /// what the vendor stack does on device arrival. Where another driver owns
    /// the node, that driver does the download and this must not interfere.
    ///
    /// `connect` remains as an explicit override for configured probes, which
    /// have no live node to ask about.
    fn initialize_firmware(&mut self) -> Result<()> {
        if self.probe.firmware_loaded {
            return Ok(());
        }
        if !self.probe.connect && !self.host_owns_node() {
            return Ok(());
        }
        let image = firmware_image_text(
            self.probe.image_selector,
            self.probe.firmware_dir.as_deref(),
        )?;
        #[cfg(feature = "os-usb")]
        {
            live_lumenera::push_firmware(self.probe.vendor_id, self.probe.product_id, &image)?;
            // The device detaches and renumerates; give it a moment.
            std::thread::sleep(std::time::Duration::from_millis(1500));
            self.probe.firmware_loaded = true;
            if let Some(usb) = self.probe.usb.as_mut() {
                usb.firmware_loaded = true;
            }
            Ok(())
        }
        #[cfg(not(feature = "os-usb"))]
        {
            let _ = image;
            Err(Error::new(
                ErrorCode::Unsupported,
                "Lumenera firmware download requires numanager-drivers/os-usb",
            ))
        }
    }

    fn selected_image(&self) -> &'static str {
        firmware_image_file(self.probe.image_selector)
    }

    fn descriptor(&self) -> DeviceDescriptor {
        let mut metadata = BTreeMap::from([
            (
                "evidence_class".into(),
                Value::String("reverse engineered".into()),
            ),
            ("hardware_validated".into(), Value::Bool(false)),
            ("firmware_download_validated".into(), Value::Bool(true)),
            (
                "imaging_protocol_status".into(),
                Value::String(PROTOCOL_STATUS.into()),
            ),
            (
                "capture_gate".into(),
                Value::String(CAPTURE_REQUIRES_LIVE.into()),
            ),
            (
                "control_gate".into(),
                Value::String(GAIN_UNEVIDENCED.into()),
            ),
            (
                "firmware_stage".into(),
                Value::String(self.probe.stage().into()),
            ),
            (
                "firmware_image".into(),
                Value::String(self.selected_image().into()),
            ),
        ]);
        if let Some(usb) = &self.probe.usb {
            metadata.insert("usb_identity".into(), usb.value());
        }
        DeviceDescriptor {
            id: self.camera,
            driver: self.id,
            label: self.probe.label.clone(),
            vendor: Some("Lumenera".into()),
            model: Some(self.probe.product.clone()),
            serial: self.probe.serial_number.clone(),
            kinds: vec![
                "camera".into(),
                "camera.scientific".into(),
                "detector.mono".into(),
                "reverse.engineered".into(),
            ],
            properties: vec![
                string_property("model", "Model"),
                string_property("serial_number", "Serial number"),
                string_property("sensor", "Sensor"),
                property("bit_depth", "Bit depth", ValueType::I64),
                property("width", "Width", ValueType::PixelCount),
                property("height", "Height", ValueType::PixelCount),
                string_property("pixel_format", "Pixel format"),
                // Exposure writes update the value programmed at the next
                // acquisition; gain remains refused until its register mapping
                // is evidenced.
                writable_property("exposure", "Exposure", ValueType::TimeInterval),
                writable_property("gain", "Gain", ValueType::F64),
                property("firmware_loaded", "Firmware loaded", ValueType::Bool),
                property("connect", "Firmware initialization gate", ValueType::Bool),
                string_property("firmware_stage", "Firmware stage"),
                string_property("firmware_dir", "Firmware directory"),
                string_property("firmware_image", "Selected firmware image"),
                property("usb_vendor_id", "USB vendor ID", ValueType::I64),
                property("usb_product_id", "USB product ID", ValueType::I64),
                property("usb_identity", "USB identity", ValueType::Map),
                string_property("support_level", "Support level"),
                string_property("protocol_status", "Protocol status"),
                string_property("capture_gate", "Capture gate"),
                string_property("control_gate", "Control gate"),
            ],
            metadata,
        }
    }

    fn support_level(&self) -> &'static str {
        "USB discovery of both stages, hardware-validated two-stage firmware download, and \
         experimental live acquisition from captured-traffic evidence; the last hardware run \
         received 0 image bytes, so capture is not validated; gain is not exposed because its \
         register mapping is unevidenced"
    }

    fn read_property(&self, key: &str) -> Result<Value> {
        match key {
            "model" => Ok(Value::String(self.probe.product.clone())),
            "serial_number" => Ok(Value::String(
                self.probe.serial_number.clone().unwrap_or_default(),
            )),
            "sensor" => Ok(Value::String(SENSOR_NAME.into())),
            "bit_depth" => Ok(Value::I64(SENSOR_BITS)),
            "width" => Ok(Value::PixelCount(PixelCount::new(SENSOR_WIDTH))),
            "height" => Ok(Value::PixelCount(PixelCount::new(SENSOR_HEIGHT))),
            // [assumed] 12-bit sensor almost certainly transfers 16-bit packed.
            "pixel_format" => Ok(Value::String("Mono16".into())),
            "exposure" => Ok(Value::TimeInterval(self.probe.exposure)),
            "gain" => Ok(Value::F64(self.probe.gain)),
            "firmware_loaded" => Ok(Value::Bool(self.probe.firmware_loaded)),
            "connect" => Ok(Value::Bool(self.probe.connect)),
            "firmware_stage" => Ok(Value::String(self.probe.stage().into())),
            "firmware_dir" => Ok(Value::String(
                self.probe.firmware_dir.clone().unwrap_or_default(),
            )),
            "firmware_image" => Ok(Value::String(self.selected_image().into())),
            "usb_vendor_id" => Ok(Value::I64(self.probe.vendor_id as i64)),
            "usb_product_id" => Ok(Value::I64(self.probe.product_id as i64)),
            "usb_identity" => Ok(self
                .probe
                .usb
                .as_ref()
                .map(LumeneraUsbIdentity::value)
                .unwrap_or(Value::Null)),
            "support_level" => Ok(Value::String(self.support_level().into())),
            "protocol_status" => Ok(Value::String(PROTOCOL_STATUS.into())),
            "capture_gate" => Ok(Value::String(CAPTURE_REQUIRES_LIVE.into())),
            "control_gate" => Ok(Value::String(GAIN_UNEVIDENCED.into())),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Lumenera property {key}"),
            )),
        }
    }

    /// `exposure` is recorded and applied at the next acquisition — the
    /// sequence programs it as part of configure, so there is no separate
    /// apply. `gain` stays refused: its register mapping is unevidenced.
    fn write_property(&mut self, key: &str, value: Value) -> Result<Value> {
        match (key, value) {
            ("exposure", Value::TimeInterval(exposure)) => {
                self.probe.exposure = exposure;
                Ok(Value::TimeInterval(exposure))
            }
            ("gain", _) => Err(Error::new(ErrorCode::Unsupported, GAIN_UNEVIDENCED)),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Lumenera property {key} is not writable"),
            )),
        }
    }

    fn validate_write_property(&self, key: &str, value: &Value) -> Result<()> {
        match (key, value) {
            ("exposure", Value::TimeInterval(_)) => Ok(()),
            ("gain", Value::F64(_)) => Ok(()),
            ("exposure", _) => Err(Error::new(
                ErrorCode::InvalidProperty,
                "Lumenera exposure expects TimeInterval",
            )),
            ("gain", _) => Err(Error::new(
                ErrorCode::InvalidProperty,
                "Lumenera gain expects F64",
            )),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Lumenera property {key} is not writable"),
            )),
        }
    }

    #[cfg(feature = "os-usb")]
    fn capture_plan(&self) -> CapturePlan {
        CapturePlan {
            width: SENSOR_WIDTH as u16,
            height: SENSOR_HEIGHT as u16,
            x_bin: 1,
            y_bin: 1,
            exposure_us: self.probe.exposure_micros(),
        }
    }

    /// Run one acquisition and hand the frame to the runtime.
    fn capture_frame(
        &mut self,
        token: DriverToken,
        _request: CameraCaptureRequest,
    ) -> Result<Value> {
        #[cfg(feature = "os-usb")]
        {
            if self.probe.usb.is_some() {
                // Bring the camera to its imaging stage if it is not there yet.
                // That it arrives as a firmware loader and renumerates is an
                // implementation detail of this device, not something a caller
                // asking for a frame should have to sequence.
                self.bring_up()?;
                let plan = self.capture_plan();
                if self.session.is_none() {
                    self.session = Some(live_imaging::ImagingSession::open(
                        self.probe.vendor_id,
                        IMAGING_PID,
                    )?);
                }
                let data = match self
                    .session
                    .as_ref()
                    .expect("the session was just opened")
                    .acquire(&plan)
                {
                    Ok(data) => data,
                    Err(error) => {
                        // The session ends at the first failure rather than
                        // being reused: a camera that was unplugged, or an
                        // interface left mid-frame, is not something the next
                        // capture should inherit. `acquire` already returns the
                        // camera to idle on the way out, so the only state worth
                        // discarding is the claim itself.
                        self.session = None;
                        return Err(error);
                    }
                };
                let handle = FrameHandle {
                    stream: StreamId(self.camera.0 .0),
                    frame: FrameId(token.0),
                };
                self.events.push_back(DriverEvent::FrameReady(Frame {
                    handle,
                    device: self.camera,
                    width: plan.width as u32,
                    height: plan.height as u32,
                    pixel_format: "Raw16".into(),
                    data,
                    buffer: FrameBufferSpec::default(),
                    metadata: BTreeMap::from([
                        ("exposure".into(), Value::TimeInterval(self.probe.exposure)),
                        ("bit_depth".into(), Value::I64(SENSOR_BITS)),
                        ("sensor".into(), Value::String(SENSOR_NAME.into())),
                        ("source".into(), Value::String("lumenera-live-usb".into())),
                    ]),
                }));
                return Ok(Value::Map(BTreeMap::from([
                    (
                        "width".into(),
                        Value::PixelCount(PixelCount::new(plan.width as u32)),
                    ),
                    (
                        "height".into(),
                        Value::PixelCount(PixelCount::new(plan.height as u32)),
                    ),
                    ("pixel_format".into(), Value::String("Raw16".into())),
                    ("stream".into(), Value::I64(handle.stream.0 as i64)),
                    ("frame".into(), Value::I64(handle.frame.0 as i64)),
                    ("source".into(), Value::String("lumenera-live-usb".into())),
                ])));
            }
        }
        let _ = token;
        Err(Error::new(ErrorCode::Unsupported, CAPTURE_REQUIRES_LIVE))
    }
}

impl Driver for LumeneraCameraDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        vec![self.descriptor()]
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.transport,
            driver: self.id,
            label: "Lumenera EZ-USB transport".into(),
            kind: "usb.camera.transport".into(),
            metadata: BTreeMap::from([
                (
                    "vendor_id".into(),
                    Value::String(format!("0x{:04x}", self.probe.vendor_id)),
                ),
                (
                    "product_id".into(),
                    Value::String(format!("0x{:04x}", self.probe.product_id)),
                ),
                (
                    "firmware_stage".into(),
                    Value::String(self.probe.stage().into()),
                ),
                (
                    "firmware_initialization".into(),
                    Value::String("validated (EZ-USB anchor download, request 0xA0)".into()),
                ),
                (
                    "imaging_protocol".into(),
                    Value::String("not reverse engineered".into()),
                ),
            ]),
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.camera {
            // Declared to reflect the real device; invoking it errors until the
            // wire protocol is decoded (see `capture_frame`).
            return vec![capability(1, self.camera, CapabilityKind::CameraCapture)];
        }
        Vec::new()
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } if *device == self.camera => {
                    let _ = self.read_property(key)?;
                }
                Command::WriteProperty { device, key, value } if *device == self.camera => {
                    self.validate_write_property(key, value)?;
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
                }
                Command::Invoke { device, .. } if *device == self.camera => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "unsupported Lumenera capability",
                    ));
                }
                _ => {}
            }
        }
        Ok(PreparedBatch {
            id: batch.id,
            commands: batch.commands.clone(),
            physical_transactions: Vec::new(),
        })
    }

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.next_token();
        let mut result = Value::Null;
        for command in prepared.commands {
            match command {
                Command::ReadProperty { device, key } if device == self.camera => {
                    result = self.read_property(&key)?;
                }
                Command::WriteProperty { device, key, value } if device == self.camera => {
                    result = self.write_property(&key, value)?;
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if device == self.camera && capability == CapabilityId(1) => {
                    let capture = match request {
                        CapabilityRequest::CameraCapture(request) => request,
                        CapabilityRequest::None => CameraCaptureRequest::default_frame(),
                        _ => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "CameraCapture expects CameraCaptureRequest",
                            ))
                        }
                    };
                    result = self.capture_frame(token, capture)?;
                }
                Command::Invoke { device, .. } if device == self.camera => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "unsupported Lumenera capability",
                    ));
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
        self.events.drain(..).collect()
    }
}

fn property(key: &str, display_name: &str, value_type: ValueType) -> PropertySchema {
    PropertySchema {
        key: key.into(),
        display_name: display_name.into(),
        value_type,
        unit: None,
        range: None,
        increment: None,
        enum_values: Vec::new(),
        readable: true,
        writable: false,
        volatile: false,
        sequenceable: false,
        hardware_address: None,
    }
}

fn string_property(key: &str, display_name: &str) -> PropertySchema {
    property(key, display_name, ValueType::String)
}

fn writable_property(key: &str, display_name: &str, value_type: ValueType) -> PropertySchema {
    let mut schema = property(key, display_name, value_type);
    schema.writable = true;
    schema
}

fn capability(id: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    let name = kind.name().to_string();
    CapabilityDescriptor {
        id: CapabilityId(id),
        device,
        kind,
        name,
        request: ValueType::Map,
        response: ValueType::Map,
    }
}

/// Live imaging — the acquisition sequence read off captured hardware traffic.
///
/// Ordering, payloads and the alternate-setting changes are replayed as
/// captured. Steps whose meaning was not recorded are still issued, because
/// omitting them would be as much a guess as renaming them.
#[cfg(feature = "os-usb")]
mod live_imaging {
    use super::protocol::*;
    use super::*;
    use nusb::transfer::{Control, ControlType, Recipient, RequestBuffer};
    use std::time::{Duration, Instant};

    const CONTROL_TIMEOUT: Duration = Duration::from_secs(5);
    /// Budget for the pipeline image write and the status poll after it. The
    /// vendor's own write took ~0.7 s for 98 KB.
    const CONFIG_TIMEOUT: Duration = Duration::from_secs(10);
    /// Compiled-in sensor-pipeline configuration image.
    const FPGA_IMAGE_FILE: &str = "lumenera_fpga_lu130.bin";
    /// Compiled-in post-configuration transfer sequence.
    const INIT_SEQUENCE_FILE: &str = "lumenera_init_lu130.jsonl";

    /// One recorded control transfer. Field names match the capture tool's
    /// output so a fresh recording can be dropped in unedited.
    #[derive(serde::Deserialize)]
    struct InitStep {
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
        let text = <String as serde::Deserialize>::deserialize(d)?;
        u8::from_str_radix(text.trim_start_matches("0x"), 16).map_err(serde::de::Error::custom)
    }

    fn hex_u16<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<u16, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(d)?;
        u16::from_str_radix(text.trim_start_matches("0x"), 16).map_err(serde::de::Error::custom)
    }

    /// Hex text to bytes, ignoring anything malformed — a recorded payload is
    /// either well-formed or the line is not worth failing the whole bring-up.
    fn hex_bytes(text: &str) -> Vec<u8> {
        (0..text.len() / 2)
            .filter_map(|i| u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok())
            .collect()
    }
    /// One URB's worth of image data; the device streams in 512 KiB chunks.
    const BULK_CHUNK: usize = 0x80000;
    /// Headroom over the exposure for readout and transfer.
    const READ_OVERHEAD: Duration = Duration::from_secs(5);

    pub(super) struct ImagingSession {
        interface: nusb::Interface,
    }

    /// Bind WinUSB to the camera's node if something else owns it.
    ///
    /// Best-effort: a failure here is not reported directly, because the
    /// interface claim that follows fails with a diagnosis naming the driver
    /// that actually owns the node — a better error than anything this could
    /// raise. Needs an elevated process; unelevated, the claim's message says
    /// so. A no-op where WinUSB provisioning is not a thing.
    #[cfg(any(windows, feature = "winusb"))]
    fn ensure_host_access(vendor_id: u16, product_id: u16) {
        use crate::winusb_access::{access_state, ensure_access, UsbFunction};

        let function = UsbFunction::new(vendor_id, product_id);
        match access_state(function) {
            Ok(state) if state.is_winusb() => {}
            Ok(_) => {
                let _ = ensure_access(function, &|_| true);
            }
            Err(_) => {}
        }
    }

    #[cfg(not(any(windows, feature = "winusb")))]
    fn ensure_host_access(_vendor_id: u16, _product_id: u16) {}

    impl ImagingSession {
        /// Open the imaging-stage device and claim its interface.
        pub(super) fn open(vendor_id: u16, product_id: u16) -> Result<Self> {
            // Granting this process access to the node is the driver's problem,
            // not the caller's. On Windows a userspace claim requires WinUSB
            // bound to the device, and an application asking a camera for a
            // frame should not also have to know that, or run a separate tool
            // first. Binding displaces whatever currently owns the node, so it
            // is done here — on an explicit capture against this camera — and
            // never during passive discovery, which must stay read-only.
            ensure_host_access(vendor_id, product_id);

            let info = nusb::list_devices()
                .map_err(|error| {
                    Error::new(
                        ErrorCode::Transport,
                        format!("Lumenera USB device listing failed: {error}"),
                    )
                })?
                .find(|device| {
                    device.vendor_id() == vendor_id && device.product_id() == product_id
                })
                .ok_or_else(|| {
                    Error::new(
                        ErrorCode::Transport,
                        format!(
                            "Lumenera imaging device {vendor_id:04x}:{product_id:04x} is not present"
                        ),
                    )
                })?;
            let device = info.open().map_err(|error| {
                Error::new(
                    ErrorCode::Transport,
                    format!("opening the Lumenera camera failed (WinUSB bound?): {error}"),
                )
            })?;
            // Re-select the configuration before claiming. The recorded bring-up
            // issues SET_CONFIGURATION(1) and only then finds the pipeline's
            // control register reporting ready; without it the register sits in
            // a state where the configuration image is ignored. The device is
            // already configured by enumeration, so this is a deliberate
            // re-assert rather than setup.
            let _ = device.set_configuration(1);

            let interface = device.claim_interface(0).map_err(|error| {
                // Fell through host access: report what owns the node.
                Error::new(
                    ErrorCode::Transport,
                    format!(
                        "claiming the Lumenera camera interface failed: {error}{}",
                        crate::usb_discovery::usb_claim_hint(vendor_id, product_id, 0)
                    ),
                )
            })?;
            // A previous session that died mid-capture leaves the interface in
            // the streaming alt-setting with the image endpoint part-way
            // through a frame. Returning to the idle setting on open resets the
            // endpoint, so the first capture of a session starts from a known
            // state rather than inheriting one.
            let _ = interface.set_alt_setting(ALT_IDLE);

            let session = Self { interface };
            // The camera answers this capability block on open, before any
            // configuration. Whether it *requires* the read is unrecorded, so
            // it is issued in the recorded order and the result discarded:
            // nothing in the capture path depends on it,
            // and a camera that refuses the read is left to fail later, on the
            // operation the caller actually asked for.
            for index in IDX_CAPABILITY_READS {
                let _ = session.read_property(index);
            }
            session.configure_pipeline()?;
            Ok(session)
        }

        /// Stream the sensor-pipeline configuration image and wait for the
        /// device to report it accepted. Runs once per session, before any
        /// capture; until it has, the image endpoint delivers nothing however
        /// correct the acquisition sequence is.
        ///
        /// Run unconditionally. The recorded sequence reads `0x0008` first but
        /// does not branch on it, and the value is not a reliable "already
        /// configured" flag: a device that has been configured and then reset
        /// by a driver change reads zero here while delivering no frames.
        fn configure_pipeline(&self) -> Result<()> {
            let before = self
                .read_property(IDX_CONFIG_STATUS)
                .map(u32::from_le_bytes)
                .unwrap_or(u32::MAX);

            // `0x80` is the ready-to-configure state, and the only one in which
            // an arm is accepted: a device already carrying an image reports
            // `0xA0` and ignores the sequence outright. Skipping the download
            // then is not an optimisation — re-arming a configured device is
            // simply refused, so the register load below is what remains to do.
            if before != CONFIG_READY {
                self.replay_init_sequence()?;
                return Ok(());
            }

            let image =
                crate::bundled_firmware::blob_by_name(FPGA_IMAGE_FILE).ok_or_else(|| {
                    Error::new(
                        ErrorCode::Unsupported,
                        format!("Lumenera pipeline image {FPGA_IMAGE_FILE} is not compiled in"),
                    )
                })?;

            self.interface
                .set_alt_setting(ALT_CONFIG)
                .map_err(|error| {
                    Error::new(
                        ErrorCode::Transport,
                        format!("Lumenera configuration alt-setting select failed: {error}"),
                    )
                })?;
            self.property(IDX_CONFIG_STATUS, &word(CONFIG_ARM))?;

            // One transfer for the whole image, as the recorded sequence does.
            let completion =
                futures_lite::future::block_on(self.interface.bulk_out(EP_CONFIG, image.to_vec()));
            completion.status.map_err(|error| {
                Error::new(
                    ErrorCode::Transport,
                    format!("Lumenera pipeline image write failed: {error}"),
                )
            })?;
            let sent = completion.data.actual_length();
            if sent != image.len() {
                return Err(Error::new(
                    ErrorCode::Transport,
                    format!(
                        "Lumenera pipeline image short write: {sent}/{} bytes",
                        image.len()
                    ),
                ));
            }

            // The device reports progress in the same register: it steps down
            // to zero when it has taken the image. Bounded, because a device
            // that never finishes must fail rather than hang the caller.
            let deadline = Instant::now() + CONFIG_TIMEOUT;
            loop {
                let status = u32::from_le_bytes(self.read_property(IDX_CONFIG_STATUS)?);
                if status == 0 {
                    break;
                }
                if Instant::now() >= deadline {
                    return Err(Error::new(
                        ErrorCode::Transport,
                        format!(
                            "Lumenera pipeline configuration did not complete: status \
                             {status:#010x} still set (was {before:#010x} before arming, \
                             {sent} bytes written)"
                        ),
                    ));
                }
                std::thread::sleep(Duration::from_millis(5));
            }

            self.property(IDX_CONFIG_STATUS, &word(CONFIG_FINISH))?;
            let _ = self.interface.set_alt_setting(ALT_IDLE);
            self.replay_init_sequence()?;
            Ok(())
        }

        /// Replay the recorded post-configuration transfers.
        ///
        /// A freshly configured device still yields no frames until its
        /// register file is loaded: 510 transfers, mostly 8-bit writes on
        /// `wIndex 6` addressed by `wValue`. Their layout is understood (a reset
        /// pulse, an ascending sweep, four per-channel blocks) but the meaning
        /// of individual registers is not, and cannot be established from a
        /// single observation of a single camera — so they are replayed as
        /// recorded rather than renamed into invented semantics.
        ///
        /// Reads in the sequence are issued and their results discarded: the
        /// recorded host read them, and a device that expects the traffic
        /// should see it.
        fn replay_init_sequence(&self) -> Result<()> {
            let script =
                crate::bundled_firmware::sequence_by_name(INIT_SEQUENCE_FILE).ok_or_else(|| {
                    Error::new(
                        ErrorCode::Unsupported,
                        format!("Lumenera init sequence {INIT_SEQUENCE_FILE} is not compiled in"),
                    )
                })?;

            for (lineno, line) in script.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let step: InitStep = serde_json::from_str(line).map_err(|error| {
                    Error::new(
                        ErrorCode::Driver,
                        format!("Lumenera init sequence line {}: {error}", lineno + 1),
                    )
                })?;
                if step.dir == "in" {
                    let mut buffer = vec![0u8; step.w_length as usize];
                    let control = Control {
                        control_type: ControlType::Vendor,
                        recipient: Recipient::Device,
                        request: step.b_request,
                        value: step.w_value,
                        index: step.w_index,
                    };
                    let _ =
                        self.interface
                            .control_in_blocking(control, &mut buffer, CONTROL_TIMEOUT);
                } else {
                    let data = hex_bytes(&step.data);
                    self.write(step.b_request, step.w_value, step.w_index, &data)?;
                }
            }
            Ok(())
        }

        fn write(&self, request: u8, value: u16, index: u16, data: &[u8]) -> Result<()> {
            let control = Control {
                control_type: ControlType::Vendor,
                recipient: Recipient::Device,
                request,
                value,
                index,
            };
            let sent = self
                .interface
                .control_out_blocking(control, data, CONTROL_TIMEOUT)
                .map_err(|error| {
                    Error::new(
                        ErrorCode::Transport,
                        format!(
                            "Lumenera control write req={request:#04x} idx={index:#06x} failed: {error}"
                        ),
                    )
                })?;
            if sent != data.len() {
                return Err(Error::new(
                    ErrorCode::Transport,
                    format!(
                        "Lumenera control write req={request:#04x} idx={index:#06x} short: {sent}/{}",
                        data.len()
                    ),
                ));
            }
            Ok(())
        }

        fn property(&self, index: u16, data: &[u8]) -> Result<()> {
            self.write(REQ_PROPERTY, 0, index, data)
        }

        /// One four-byte capability read. Read-only: see
        /// [`IDX_CAPABILITY_READS`].
        fn read_property(&self, index: u16) -> Result<[u8; 4]> {
            let control = Control {
                control_type: ControlType::Vendor,
                recipient: Recipient::Device,
                request: REQ_PROPERTY,
                value: 0,
                index,
            };
            let mut buffer = [0u8; 4];
            let read = self
                .interface
                .control_in_blocking(control, &mut buffer, CONTROL_TIMEOUT)
                .map_err(|error| {
                    Error::new(
                        ErrorCode::Transport,
                        format!("Lumenera capability read idx={index:#06x} failed: {error}"),
                    )
                })?;
            if read != buffer.len() {
                return Err(Error::new(
                    ErrorCode::Transport,
                    format!(
                        "Lumenera capability read idx={index:#06x} short: {read}/{}",
                        buffer.len()
                    ),
                ));
            }
            Ok(buffer)
        }

        /// Describe what the camera reports about itself, for a capture that
        /// produced no data. A camera that answers with plausible geometry is
        /// alive and configured — pointing at the bulk pipe; one that answers
        /// with zeros or fails outright points at the firmware stage instead.
        ///
        /// Best-effort by construction: this runs on a path that has already
        /// failed, so a read error becomes part of the report rather than
        /// replacing the original error.
        fn capability_report(&self) -> String {
            let mut parts = Vec::new();
            for index in IDX_CAPABILITY_READS {
                match self.read_property(index) {
                    Ok(value) => {
                        let word = u32::from_le_bytes(value);
                        if index == IDX_CAPABILITY_DIMENSIONS {
                            parts.push(format!(
                                "{index:#06x}={word:#010x} ({}x{})",
                                word & 0xffff,
                                word >> 16
                            ));
                        } else if index == IDX_CAPABILITY_STATE {
                            parts.push(format!("{index:#06x}={word:#010x} (state)"));
                        } else {
                            parts.push(format!("{index:#06x}={word:#010x}"));
                        }
                    }
                    Err(error) => parts.push(format!("{index:#06x}=<{error}>")),
                }
            }
            parts.join(", ")
        }

        fn register(&self, address: u16, value: u32) -> Result<()> {
            self.write(REQ_REGISTER, address, IDX_REGISTER_DATA, &word(value))
        }

        /// Registers written on both sides of a capture.
        fn write_tap_registers(&self, include_trailer: bool) -> Result<()> {
            for offset in 0..4u16 {
                self.register(REG_TAP_FIRST + offset, REG_TAP_VALUE)?;
            }
            if include_trailer {
                self.register(REG_TRAILER_A, REG_TRAILER_A_VALUE)?;
                self.register(REG_TRAILER_B, 0)?;
            }
            Ok(())
        }

        /// Program geometry and exposure. Ordering follows the capture, including
        /// the repeated format writes.
        fn configure(&self, plan: &CapturePlan) -> Result<()> {
            self.property(IDX_OPAQUE_0670, &opaque8(0))?;
            self.write(REQ_REGISTER, 0, IDX_CMD_0F, &[0])?;

            for mode in [0u32, 5] {
                self.property(IDX_FORMAT_MODE, &word(mode))?;
                self.property(IDX_FORMAT_08, &word(0))?;
                self.property(IDX_BINNING, &binning(plan.x_bin, plan.y_bin))?;
                self.property(IDX_DIMENSIONS, &dimensions(plan.width, plan.height))?;
                self.property(IDX_FORMAT_08, &word(0))?;
            }

            self.property(IDX_OPAQUE_05A0, &opaque8(0))?;
            self.property(IDX_EXPOSURE, &exposure(plan.exposure_us))?;
            self.property(IDX_OPAQUE_0550, &opaque8(0x0000_2800_0000_0000))?;
            self.property(IDX_OPAQUE_0610, &opaque8(0))?;
            Ok(())
        }

        /// Run one acquisition and return the raw 16-bit frame bytes.
        pub(super) fn acquire(&self, plan: &CapturePlan) -> Result<Vec<u8>> {
            // Where a frame's wall-clock goes, on demand. A capture is a long
            // chain of control transfers around one bulk read, and which link
            // dominates is not guessable from the outside — set
            // `NUMANAGER_TIME_CAPTURE=1` to have each phase say so.
            let timing = std::env::var_os("NUMANAGER_TIME_CAPTURE").is_some();
            let started = Instant::now();
            let mut mark = started;
            let mut lap = |phase: &str| {
                if timing {
                    eprintln!("  {phase}: {:?}", mark.elapsed());
                    mark = Instant::now();
                }
            };

            let expected = frame_bytes(
                plan.width as u32,
                plan.height as u32,
                plan.x_bin,
                plan.y_bin,
            );
            self.configure(plan)?;
            lap("configure");

            self.interface
                .set_alt_setting(ALT_STREAMING)
                .map_err(|error| {
                    Error::new(
                        ErrorCode::Transport,
                        format!("Lumenera streaming alt-setting select failed: {error}"),
                    )
                })?;

            // Clear any halt left on the image endpoint before reads are
            // queued: a stalled pipe accepts submissions and never completes
            // them, which is indistinguishable from a camera that produced no
            // frame until the read deadline expires.
            let _ = self.interface.clear_halt(EP_IMAGE);

            lap("alt-setting + clear-halt");

            let outcome = self.stream_frame(plan, expected);
            lap("stream frame (includes the exposure)");

            // Teardown runs whether or not the read succeeded, so a failed
            // capture still leaves the camera idle rather than streaming.
            let _ = self.property(IDX_ACQUISITION, &word(ACQ_STOP));
            let _ = self.write(
                REQ_REGISTER,
                FPGA_TEARDOWN_ADDR,
                IDX_FPGA_WRITE,
                &FPGA_TEARDOWN_DATA,
            );
            let _ = self.write_tap_registers(true);
            let _ = self.interface.set_alt_setting(ALT_IDLE);
            lap("teardown");
            if timing {
                eprintln!("  total: {:?}", started.elapsed());
            }

            // A failed capture is the one case where the camera's own view of
            // itself is worth the extra transfers, so attach it to the error
            // instead of making the caller run a separate probe.
            outcome.map_err(|error| Error {
                message: format!(
                    "{}; camera reports {}",
                    error.message,
                    self.capability_report()
                ),
                ..error
            })
        }

        /// Arm, start, and drain exactly one frame off the bulk endpoint.
        fn stream_frame(&self, plan: &CapturePlan, expected: usize) -> Result<Vec<u8>> {
            self.property(IDX_ACQUISITION, &word(ACQ_ARM))?;
            self.write_tap_registers(false)?;

            // Read on a dedicated thread. The completion future has to be
            // awaited to completion — racing it against a timer drops it and
            // loses the transfer — so the timeout lives on the channel instead.
            // Reads are queued before the start write because the device begins
            // streaming immediately and a late reader loses the frame head.
            let (tx, rx) = std::sync::mpsc::sync_channel::<std::result::Result<Vec<u8>, String>>(8);
            let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<()>(1);
            let iface = self.interface.clone();
            std::thread::spawn(move || {
                let mut queue = iface.bulk_in_queue(EP_IMAGE);
                // Size every read to what is still outstanding, never to a flat
                // chunk. A frame is not a whole number of chunks, and its final
                // piece is an exact multiple of the endpoint's packet size — so
                // it carries no short packet to terminate an over-long request,
                // and an oversized final read simply never completes. Asking for
                // exactly the remainder is what the recorded host does.
                let mut queued = 0usize;
                for _ in 0..4 {
                    if queued >= expected {
                        break;
                    }
                    let len = BULK_CHUNK.min(expected - queued);
                    queue.submit(RequestBuffer::new(len));
                    queued += len;
                }
                let _ = ready_tx.send(());
                loop {
                    let completion = futures_lite::future::block_on(queue.next_complete());
                    let message = completion
                        .status
                        .map(|_| completion.data.clone())
                        .map_err(|error| error.to_string());
                    let stop = message.is_err();
                    if tx.send(message).is_err() || stop {
                        return;
                    }
                    if queued < expected {
                        let len = BULK_CHUNK.min(expected - queued);
                        queue.submit(RequestBuffer::new(len));
                        queued += len;
                    }
                    // The whole frame is queued and drained: awaiting another
                    // completion on an empty queue is a panic, not a wait.
                    if queue.pending() == 0 {
                        return;
                    }
                }
            });

            ready_rx.recv_timeout(CONTROL_TIMEOUT).map_err(|_| {
                Error::new(
                    ErrorCode::Transport,
                    "Lumenera bulk reader did not queue initial reads before acquisition start",
                )
            })?;
            self.property(IDX_ACQUISITION, &word(ACQ_START))?;

            let deadline =
                Instant::now() + Duration::from_micros(plan.exposure_us as u64) + READ_OVERHEAD;
            let mut frame = Vec::with_capacity(expected);
            while frame.len() < expected {
                let now = Instant::now();
                let remaining = deadline.checked_duration_since(now).unwrap_or_default();
                match rx.recv_timeout(remaining) {
                    Ok(Ok(data)) => {
                        let take = (expected - frame.len()).min(data.len());
                        frame.extend_from_slice(&data[..take]);
                    }
                    Ok(Err(error)) => {
                        return Err(Error::new(
                            ErrorCode::Transport,
                            format!("Lumenera bulk read failed: {error}"),
                        ))
                    }
                    Err(_) => {
                        return Err(Error::new(
                            ErrorCode::Transport,
                            format!(
                                "Lumenera frame read timed out ({} of {expected} bytes)",
                                frame.len()
                            ),
                        ))
                    }
                }
            }
            Ok(frame)
        }
    }
}

/// Everything a single acquisition needs, resolved before touching USB.
#[cfg(feature = "os-usb")]
#[derive(Debug, Clone, Copy)]
struct CapturePlan {
    width: u16,
    height: u16,
    x_bin: u16,
    y_bin: u16,
    exposure_us: u32,
}

/// Live firmware download — the EZ-USB anchor-download sequence, validated on
/// hardware. Only the parts that touch USB live here; the pure HEX/selection
/// helpers are ungated so they stay unit-testable without `os-usb`.
#[cfg(feature = "os-usb")]
mod live_lumenera {
    use super::*;
    use std::time::Duration;

    const TIMEOUT: Duration = Duration::from_secs(5);

    /// Download `image` (Intel-HEX text, already resolved from the bundled copy
    /// or from `firmware_dir` by [`super::firmware_image_text`]) to the loader.
    pub(super) fn push_firmware(vendor_id: u16, product_id: u16, image: &str) -> Result<()> {
        let records = crate::ez_usb::parse_ihex(image)?;

        let info = nusb::list_devices()
            .map_err(|error| {
                Error::new(
                    ErrorCode::Transport,
                    format!("Lumenera USB device listing failed: {error}"),
                )
            })?
            .find(|device| device.vendor_id() == vendor_id && device.product_id() == product_id)
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::Transport,
                    format!(
                        "Lumenera loader {vendor_id:04x}:{product_id:04x} is not present for firmware download"
                    ),
                )
            })?;
        let device = info.open().map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("opening the Lumenera loader failed (WinUSB bound?): {error}"),
            )
        })?;
        // Select the configuration before claiming, exactly as the imaging path
        // does. On Linux and Windows the device is already configured by
        // enumeration and this is a re-assert; on macOS it is not optional. A
        // vendor-specific device (class 0xff) that no kernel driver matches is
        // left *unconfigured* there, so it has no interfaces at all and the
        // claim fails with a bare "interface not found" — which reads like a
        // wiring fault rather than a missing SET_CONFIGURATION.
        let _ = device.set_configuration(1);

        let interface = device.claim_interface(0).map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!(
                    "claiming the Lumenera loader interface failed: {error}{}",
                    crate::usb_discovery::usb_claim_hint(vendor_id, product_id, 0)
                ),
            )
        })?;

        // One transfer per Intel-HEX record, not coalesced blocks: this is the
        // download a working stack performs, confirmed record-for-record
        // against captured traffic, and the record boundaries are part of what
        // was verified.

        crate::ez_usb::hold_8051(&interface, true, TIMEOUT)?;
        for record in &records {
            crate::ez_usb::anchor_write(&interface, record.address, &record.data, TIMEOUT)?;
        }
        crate::ez_usb::hold_8051(&interface, false, TIMEOUT)?;
        Ok(())
    }
}
