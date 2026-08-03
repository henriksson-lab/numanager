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
//! Implemented and evidenced: USB discovery of both stages, and the two-stage
//! **firmware download** (validated on hardware). Exposed as schema/capability
//! shape so the device model is complete.
//!
//! **Not yet reverse-engineered:** the imaging *wire protocol* — the vendor
//! control tuples `(bRequest, wValue, wIndex)` behind register/property access,
//! streaming setup, and frame framing. Everything that would need those tuples
//! (frame capture, applying exposure/gain) returns a clear error rather than a
//! fabricated result.

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

/// EZ-USB anchor-download vendor request. (Only the live download path uses it.)
#[cfg(feature = "os-usb")]
const REQ_ANCHOR: u8 = 0xA0;
/// EZ-USB `CPUCS` register: write 1 to hold the 8051 in reset, 0 to release.
#[cfg(feature = "os-usb")]
const CPUCS: u16 = 0xE600;

// Sensor geometry is the Sony ICX205 nominal from the Gel Doc EZ guide; the
// exact active ROI must still be read from live descriptors once the imaging
// protocol is decoded.
const SENSOR_NAME: &str = "Sony ICX205";
const SENSOR_BITS: i64 = 12;
const SENSOR_WIDTH: u32 = 1392; // [assumed] nominal; confirm against live descriptors
const SENSOR_HEIGHT: u32 = 1039; // [assumed] nominal; confirm against live descriptors

/// The single source of truth for "we can't do imaging yet" errors.
const IMAGING_PROTOCOL_UNEVIDENCED: &str =
    "Lumenera Lu130 imaging wire protocol is not yet reverse-engineered \
     (register/property control tuples, streaming setup, and frame framing are unknown)";

/// Default location of the firmware images: the in-tree third-party package.
fn default_firmware_dir() -> String {
    // Relative to the numanager repo root; override with the `firmware_dir`
    // property for an out-of-tree copy.
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

/// One Intel-HEX data record: load `data` at `addr`.
#[cfg(feature = "os-usb")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct HexRecord {
    addr: u16,
    data: Vec<u8>,
}

/// Parse Intel-HEX text into its data records (type 00), stopping at EOF
/// (type 01). All Lumenera images stay within a 16-bit address space, so no
/// extended-address records are expected. Used by the live download path and
/// the tests.
#[cfg(feature = "os-usb")]
fn parse_ihex(text: &str) -> Result<Vec<HexRecord>> {
    let mut out = Vec::new();
    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let body = line.strip_prefix(':').ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!(
                    "Lumenera firmware line {}: missing ':' start code",
                    lineno + 1
                ),
            )
        })?;
        if body.len() < 10 || body.len() % 2 != 0 {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Lumenera firmware line {}: malformed record", lineno + 1),
            ));
        }
        let bytes = (0..body.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&body[i..i + 2], 16))
            .collect::<std::result::Result<Vec<u8>, _>>()
            .map_err(|e| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Lumenera firmware line {}: bad hex: {e}", lineno + 1),
                )
            })?;
        let len = bytes[0] as usize;
        if bytes.len() != len + 5 {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!(
                    "Lumenera firmware line {}: length byte disagrees with record",
                    lineno + 1
                ),
            ));
        }
        // Checksum is the two's complement of the sum of every byte but the last.
        let sum = bytes[..bytes.len() - 1]
            .iter()
            .fold(0u8, |a, b| a.wrapping_add(*b));
        if sum.wrapping_neg() != bytes[bytes.len() - 1] {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Lumenera firmware line {}: checksum mismatch", lineno + 1),
            ));
        }
        match bytes[3] {
            0x00 => out.push(HexRecord {
                addr: u16::from_be_bytes([bytes[1], bytes[2]]),
                data: bytes[4..4 + len].to_vec(),
            }),
            0x01 => break,
            other => {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!(
                        "Lumenera firmware line {}: unsupported record type {other:#04x}",
                        lineno + 1
                    ),
                ))
            }
        }
    }
    Ok(out)
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
    #[cfg(feature = "os-usb")]
    active_usb: bool,
}

impl LumeneraDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![LumeneraProbe::fixture()],
            #[cfg(feature = "os-usb")]
            active_usb: false,
        }
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
            #[cfg(feature = "os-usb")]
            active_usb: false,
        })
    }

    #[cfg(feature = "os-usb")]
    pub fn os_usb(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: Vec::new(),
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
        }
    }

    fn next_token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    /// Push the 8051 firmware if this is a loader-stage device we've been asked
    /// to bring up. A no-op for an already-imaging device, or during passive
    /// discovery (`connect == false`).
    fn initialize_firmware(&mut self) -> Result<()> {
        if self.probe.firmware_loaded || !self.probe.connect {
            return Ok(());
        }
        let dir = self.probe.firmware_dir.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Lumenera firmware download requires firmware_dir",
            )
        })?;
        #[cfg(feature = "os-usb")]
        {
            live_lumenera::push_firmware(
                self.probe.vendor_id,
                self.probe.product_id,
                self.probe.image_selector,
                &dir,
            )?;
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
            let _ = dir;
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
                Value::String("not reverse engineered".into()),
            ),
            (
                "capture_gate".into(),
                Value::String(IMAGING_PROTOCOL_UNEVIDENCED.into()),
            ),
            (
                "control_gate".into(),
                Value::String(IMAGING_PROTOCOL_UNEVIDENCED.into()),
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
                // Advertised with the writable shape other camera drivers use;
                // writes error until the control wire protocol is decoded. [assumed]
                writable_property("exposure", "Exposure", ValueType::TimeInterval),
                writable_property("gain", "Gain", ValueType::F64),
                property("firmware_loaded", "Firmware loaded", ValueType::Bool),
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
        "USB discovery of both stages and hardware-validated two-stage firmware download; \
         imaging capture and exposure/gain control are not exposed because the imaging wire \
         protocol is not yet reverse-engineered"
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
            "protocol_status" => Ok(Value::String(IMAGING_PROTOCOL_UNEVIDENCED.into())),
            "capture_gate" | "control_gate" => {
                Ok(Value::String(IMAGING_PROTOCOL_UNEVIDENCED.into()))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Lumenera property {key}"),
            )),
        }
    }

    /// Writes to `exposure`/`gain` are structurally valid but cannot be applied
    /// yet — the control tuples are unknown. Fail loudly rather than record a
    /// value the hardware never received.
    fn write_property(&mut self, key: &str, _value: Value) -> Result<Value> {
        match key {
            "exposure" | "gain" => Err(Error::new(
                ErrorCode::Unsupported,
                IMAGING_PROTOCOL_UNEVIDENCED,
            )),
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

    fn capture_frame(
        &mut self,
        _token: DriverToken,
        _request: CameraCaptureRequest,
    ) -> Result<Value> {
        Err(Error::new(
            ErrorCode::Unsupported,
            IMAGING_PROTOCOL_UNEVIDENCED,
        ))
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

/// Live firmware download — the EZ-USB anchor-download sequence, validated on
/// hardware. Only the parts that touch USB live here; the pure HEX/selection
/// helpers are ungated so they stay unit-testable without `os-usb`.
#[cfg(feature = "os-usb")]
mod live_lumenera {
    use super::*;
    use nusb::transfer::{Control, ControlType, Recipient};
    use std::path::Path;
    use std::time::Duration;

    const TIMEOUT: Duration = Duration::from_secs(5);

    pub(super) fn push_firmware(
        vendor_id: u16,
        product_id: u16,
        image_selector: u16,
        firmware_dir: &str,
    ) -> Result<()> {
        let file = Path::new(firmware_dir).join(firmware_image_file(image_selector));
        let text = std::fs::read_to_string(&file).map_err(|error| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!(
                    "Lumenera firmware image {} is unavailable: {error}",
                    file.display()
                ),
            )
        })?;
        let records = parse_ihex(&text)?;

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
        let interface = device.claim_interface(0).map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("claiming the Lumenera loader interface failed: {error}"),
            )
        })?;

        let write = |value: u16, data: &[u8]| -> Result<()> {
            let control = Control {
                control_type: ControlType::Vendor,
                recipient: Recipient::Device,
                request: REQ_ANCHOR,
                value,
                index: 0,
            };
            let sent = interface
                .control_out_blocking(control, data, TIMEOUT)
                .map_err(|error| {
                    Error::new(
                        ErrorCode::Transport,
                        format!("Lumenera anchor download to {value:#06x} failed: {error}"),
                    )
                })?;
            if sent != data.len() {
                return Err(Error::new(
                    ErrorCode::Transport,
                    format!(
                        "Lumenera anchor download to {value:#06x} short: {sent}/{}",
                        data.len()
                    ),
                ));
            }
            Ok(())
        };

        write(CPUCS, &[0x01])?; // hold 8051 in reset
        for record in &records {
            write(record.addr, &record.data)?;
        }
        write(CPUCS, &[0x00])?; // release -> renumerate to the imaging PID
        Ok(())
    }
}
