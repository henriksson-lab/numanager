//! The wire layer for this driver lives in [`crate::spark`].
//!
//! It used to be sketched inline here — a five-type frame with a little-endian length and
//! no checksum — which disagreed with the captured traces on every field. That sketch was
//! never reachable (no transport was ever constructed for its session), but keeping it
//! beside the real codec would invite someone to bind a transport to the wrong one, so it
//! has been removed rather than left as an alternative.
//!
//! What remains here is the part that was always right: the device graph, its typed
//! capabilities and its properties.

use crate::spark::backend::{self, Detector, Intent};
use crate::spark::catalog::{MoveableCarrier, MtpMotor};
use crate::spark::session::{BoxedTransport, Progress, SparkSession};
use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::*;
use std::collections::{BTreeMap, HashMap, VecDeque};

/// The live half of the driver: a session over a real transport, plus what each outstanding
/// command was for.
///
/// Absent by default. Without it the driver answers from its own modeled state, which is
/// what every existing example relies on; with it, capability requests become TDCL commands
/// and completion arrives from the instrument instead of immediately.
struct Backend {
    session: SparkSession<BoxedTransport, DriverToken>,
    /// Commands sent and not yet answered, in submission order per token. A token completes
    /// when its *last* transaction does — setting a wavelength and then measuring is one
    /// operation to a client, not three.
    outstanding: VecDeque<(DriverToken, Intent, bool)>,
    /// What each in-flight operation has learned so far.
    ///
    /// The transaction that *answers* an operation is rarely its last one: a measurement ends
    /// with `MEASUREMENT END`, which acknowledges and says nothing, while the counts arrived
    /// two commands earlier. Completing with whatever the final command happened to return
    /// would hand back `acknowledged: true` and drop the reading.
    results: HashMap<DriverToken, BTreeMap<String, Value>>,
}

pub struct SparkCytoDriver {
    id: DriverId,
    backend: Option<Backend>,
    next_token: u64,
    events: VecDeque<DriverEvent>,
    devices: Vec<DeviceDescriptor>,
    label: String,
    serial_number: Option<String>,
    support_level: String,
    well: String,
    absorbance_wavelength: Wavelength,
    fluorescence_wavelength: Wavelength,
    luminescence_enabled: bool,
    fluorescence_enabled: bool,
    temperature_target: Temperature,
    temperature_enabled: bool,
    /// Chamber temperature as the instrument last reported it.
    ///
    /// `None` means nothing has been read. On a live instrument that is the state until the
    /// first `SENSORVALUE` reply arrives, and it is reported as `Null` rather than falling
    /// back to the setpoint — a setpoint presented as a reading is how an incubator that is
    /// not heating looks exactly like one that is.
    temperature_actual: Option<Temperature>,
    gas_target: GasConcentration,
    gas_actual: Option<GasConcentration>,
    o2_target: GasConcentration,
    o2_actual: Option<GasConcentration>,
    gas_enabled: bool,
    gas_fault: bool,
    /// Which unit each motion axis counts in, once its range reply has said so.
    axis_units: BTreeMap<StageAxis, backend::AxisUnit>,
    /// Last position readback per axis, in the unit that axis declared.
    axis_positions: BTreeMap<StageAxis, f64>,
    /// Selected position per optics carrier, keyed by device node.
    carrier_positions: BTreeMap<u64, u8>,
    /// How many positions each carrier's fitted slide has, keyed by its wire name.
    carrier_slots: BTreeMap<String, u8>,
    /// What each carrier reported is fitted to it, verbatim.
    carrier_inventory: BTreeMap<String, String>,
    barcode: Option<String>,
    camera: backend::CameraState,
    shake_mode: String,
    shake_amplitude: Position,
    shake_frequency: Frequency,
    lid_state: String,
    autofocus: Option<(f64, f64)>,
    /// What the instrument said it is, once asked.
    identity: BTreeMap<String, String>,
    camera_exposure: TimeInterval,
    next_frame: u64,
    modules: backend::Modules,
    /// The label index a `SCAN` reports its readings under.
    label_index: u32,
    /// How to reach the reader over USB, when a configuration says.
    usb: Option<crate::spark::usb::UsbConfig>,
    fim_objective: i64,
    fim_mode: String,
    fim_interlock_closed: bool,
    fim_fault: bool,
    camera_bound: bool,
    imaging_mode: String,
}

pub struct SparkCytoDiscovery {
    next_id: DriverId,
    simulated: bool,
    configured: Vec<SparkCytoConfiguredProbe>,
}

impl SparkCytoDiscovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            simulated: true,
            configured: Vec::new(),
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let configured = config
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), "spark_cyto" | "spark-cyto"))
            .map(SparkCytoConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            next_id,
            simulated: false,
            configured,
        })
    }
}

impl DriverDiscovery for SparkCytoDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        if self.simulated {
            return Ok(vec![DriverCandidate::from_driver(
                "Simulated Spark Cyto",
                Box::new(SparkCytoDriver::simulated(self.next_id)),
            )]);
        }
        Ok(self
            .configured
            .iter()
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                DriverCandidate::from_driver(
                    configured.discovery_label(),
                    Box::new(SparkCytoDriver::configured(id, configured.clone())),
                )
            })
            .collect())
    }
}

#[derive(Debug, Clone)]
pub struct SparkCytoConfiguredProbe {
    label: String,
    serial_number: Option<String>,
    // --- fitted options ---
    // A Spark is configured per order: the injectors, the barcode reader and the filter and
    // mirror carriers are all things a given machine may simply not have. Zero (or false)
    // leaves the device out of the graph entirely rather than publishing something that
    // would fail on first use, which is what an application reads to decide whether to
    // offer the control at all.
    injector_pumps: i64,
    barcode_fitted: bool,
    filter_positions: i64,
    mirror_positions: i64,
    well: String,
    absorbance_wavelength: Wavelength,
    fluorescence_wavelength: Wavelength,
    luminescence_enabled: bool,
    fluorescence_enabled: bool,
    temperature_target: Temperature,
    temperature_enabled: bool,
    gas_target: GasConcentration,
    gas_actual: GasConcentration,
    o2_target: GasConcentration,
    o2_actual: GasConcentration,
    gas_enabled: bool,
    gas_fault: bool,
    fim_objective: i64,
    fim_mode: String,
    fim_interlock_closed: bool,
    fim_fault: bool,
    camera_bound: bool,
    imaging_mode: String,
    /// The reader's USB identity. Absent by default: it is not in the recovered evidence,
    /// and a driver that guessed one would open whatever device happened to carry it.
    vendor_id: Option<u16>,
    product_id: Option<u16>,
    /// CAN module numbers, which are assigned at enumeration rather than fixed.
    modules: backend::Modules,
}

impl SparkCytoConfiguredProbe {
    pub fn fixture() -> Self {
        Self {
            label: "Modeled Spark Cyto".into(),
            serial_number: None,
            injector_pumps: 2,
            barcode_fitted: true,
            filter_positions: 4,
            mirror_positions: 2,
            well: "A01".into(),
            absorbance_wavelength: Wavelength::from_nanometers(600.0),
            fluorescence_wavelength: Wavelength::from_nanometers(520.0),
            luminescence_enabled: false,
            fluorescence_enabled: false,
            temperature_target: Temperature::from_celsius(25.0),
            temperature_enabled: false,
            gas_target: GasConcentration::from_percent(5.0),
            gas_actual: GasConcentration::from_percent(0.04),
            // Ambient air, which is where an unpressurised chamber sits.
            o2_target: GasConcentration::from_percent(21.0),
            o2_actual: GasConcentration::from_percent(21.0),
            gas_enabled: false,
            gas_fault: false,
            fim_objective: 1,
            fim_mode: "brightfield".into(),
            fim_interlock_closed: true,
            fim_fault: false,
            camera_bound: false,
            imaging_mode: "brightfield".into(),
            vendor_id: None,
            product_id: None,
            modules: backend::Modules::default(),
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::fixture();
        configured.label = if device.label.is_empty() {
            string_prop(device, "label").unwrap_or(configured.label)
        } else {
            device.label.clone()
        };
        configured.serial_number = optional_string_prop(device, "serial_number");
        configured.well = string_prop(device, "well").unwrap_or(configured.well);
        configured.absorbance_wavelength = wavelength_prop(device, "absorbance_wavelength")
            .unwrap_or(configured.absorbance_wavelength);
        configured.fluorescence_wavelength = wavelength_prop(device, "fluorescence_wavelength")
            .unwrap_or(configured.fluorescence_wavelength);
        configured.luminescence_enabled =
            bool_prop(device, "luminescence_enabled").unwrap_or(configured.luminescence_enabled);
        configured.fluorescence_enabled =
            bool_prop(device, "fluorescence_enabled").unwrap_or(configured.fluorescence_enabled);
        configured.temperature_target =
            temperature_prop(device, "temperature_target").unwrap_or(configured.temperature_target);
        configured.temperature_enabled =
            bool_prop(device, "temperature_enabled").unwrap_or(configured.temperature_enabled);
        configured.gas_target = gas_prop(device, "co2_target").unwrap_or(configured.gas_target);
        configured.gas_actual = gas_prop(device, "co2_actual").unwrap_or(configured.gas_actual);
        configured.o2_target = gas_prop(device, "o2_target").unwrap_or(configured.o2_target);
        configured.o2_actual = gas_prop(device, "o2_actual").unwrap_or(configured.o2_actual);
        configured.gas_enabled = bool_prop(device, "gas_enabled").unwrap_or(configured.gas_enabled);
        configured.gas_fault = bool_prop(device, "gas_fault").unwrap_or(configured.gas_fault);
        configured.fim_objective =
            i64_prop(device, "fim_objective").unwrap_or(configured.fim_objective);
        configured.fim_mode = string_prop(device, "fim_mode").unwrap_or(configured.fim_mode);
        configured.fim_interlock_closed =
            bool_prop(device, "fim_interlock_closed").unwrap_or(configured.fim_interlock_closed);
        configured.fim_fault = bool_prop(device, "fim_fault").unwrap_or(configured.fim_fault);
        configured.camera_bound =
            bool_prop(device, "camera_bound").unwrap_or(configured.camera_bound);
        configured.imaging_mode =
            string_prop(device, "imaging_mode").unwrap_or(configured.imaging_mode);
        configured.injector_pumps =
            i64_prop(device, "injector_pumps").unwrap_or(configured.injector_pumps);
        configured.barcode_fitted =
            bool_prop(device, "barcode_fitted").unwrap_or(configured.barcode_fitted);
        configured.filter_positions =
            i64_prop(device, "filter_positions").unwrap_or(configured.filter_positions);
        configured.mirror_positions =
            i64_prop(device, "mirror_positions").unwrap_or(configured.mirror_positions);
        configured.vendor_id = u16_prop(device, "vendor_id");
        configured.product_id = u16_prop(device, "product_id");
        configured.modules = backend::Modules {
            imaging: u32_prop(device, "imaging_module"),
            injector: u32_prop(device, "injector_module"),
            gas: u32_prop(device, "gas_module"),
            barcode: u32_prop(device, "barcode_module"),
            // Which imaging module is fitted is discovered, not configured.
            cell_imaging: false,
        };
        Ok(configured)
    }

    fn discovery_label(&self) -> String {
        match &self.serial_number {
            Some(serial) => format!("{} ({serial})", self.label),
            None => self.label.clone(),
        }
    }
}

impl SparkCytoDriver {
    pub fn simulated(id: DriverId) -> Self {
        Self::configured(id, SparkCytoConfiguredProbe::fixture())
    }

    pub fn configured(id: DriverId, configured: SparkCytoConfiguredProbe) -> Self {
        let mut devices = vec![
            descriptor(
                id,
                300,
                "spark-mainboard",
                configured.serial_number.clone(),
                &["hub", "plate.transport"],
                vec![
                    sequenceable_property("well", "Well", ValueType::String, None, true),
                    property(
                        "support_level",
                        "Support level",
                        ValueType::String,
                        None,
                        false,
                    ),
                    property(
                        "instrument_type",
                        "Instrument type",
                        ValueType::String,
                        None,
                        false,
                    ),
                    property(
                        "hardware_version",
                        "Hardware version",
                        ValueType::String,
                        None,
                        false,
                    ),
                    property("state", "State", ValueType::String, None, false),
                    property("modules", "Modules", ValueType::String, None, false),
                ],
            ),
            descriptor(
                id,
                301,
                "spark-absorbance",
                configured.serial_number.clone(),
                &["detector.absorbance"],
                vec![sequenceable_property(
                    "wavelength",
                    "Wavelength",
                    ValueType::Wavelength,
                    Some("nm"),
                    true,
                )],
            ),
            descriptor(
                id,
                302,
                "spark-fluorescence",
                configured.serial_number.clone(),
                &["detector.fluorescence", "light.source"],
                vec![
                    sequenceable_property(
                        "wavelength",
                        "Wavelength",
                        ValueType::Wavelength,
                        Some("nm"),
                        true,
                    ),
                    sequenceable_property("enabled", "Enabled", ValueType::Bool, None, true),
                ],
            ),
            descriptor(
                id,
                303,
                "spark-luminescence",
                configured.serial_number.clone(),
                &["detector.luminescence"],
                vec![sequenceable_property(
                    "enabled",
                    "Enabled",
                    ValueType::Bool,
                    None,
                    true,
                )],
            ),
            descriptor(
                id,
                304,
                "spark-temperature",
                configured.serial_number.clone(),
                &["environment.temperature"],
                vec![
                    sequenceable_property(
                        "target",
                        "Target",
                        ValueType::Temperature,
                        Some("degC"),
                        true,
                    ),
                    property(
                        "actual",
                        "Actual",
                        ValueType::Temperature,
                        Some("degC"),
                        false,
                    ),
                    sequenceable_property("enabled", "Enabled", ValueType::Bool, None, true),
                ],
            ),
            descriptor(
                id,
                305,
                "spark-gas",
                configured.serial_number.clone(),
                &["environment.gas"],
                vec![
                    sequenceable_property(
                        "co2_target",
                        "CO2 target",
                        ValueType::GasConcentration,
                        Some("percent"),
                        true,
                    ),
                    property(
                        "co2_actual",
                        "CO2 actual",
                        ValueType::GasConcentration,
                        Some("percent"),
                        false,
                    ),
                    sequenceable_property(
                        "o2_target",
                        "O2 target",
                        ValueType::GasConcentration,
                        Some("percent"),
                        true,
                    ),
                    property(
                        "o2_actual",
                        "O2 actual",
                        ValueType::GasConcentration,
                        Some("percent"),
                        false,
                    ),
                    sequenceable_property("enabled", "Enabled", ValueType::Bool, None, true),
                    property("fault", "Fault", ValueType::Bool, None, false),
                ],
            ),
            descriptor(
                id,
                306,
                "spark-fim",
                configured.serial_number.clone(),
                &["imaging.head", "objective.turret"],
                vec![
                    sequenceable_property(
                        "objective",
                        "Objective position",
                        ValueType::I64,
                        None,
                        true,
                    ),
                    sequenceable_property("mode", "Imaging mode", ValueType::String, None, true),
                    property(
                        "interlock_closed",
                        "Interlock closed",
                        ValueType::Bool,
                        None,
                        false,
                    ),
                    property("fault", "Fault", ValueType::Bool, None, false),
                ],
            ),
            descriptor(
                id,
                307,
                "spark-camera-binding",
                configured.serial_number.clone(),
                &["camera.binding"],
                vec![
                    sequenceable_property("bound", "Bound", ValueType::Bool, None, true),
                    sequenceable_property(
                        "imaging_mode",
                        "Imaging mode",
                        ValueType::String,
                        None,
                        true,
                    ),
                ],
            ),
            // The imaging module's axes. Focus is motion on this instrument, not a camera
            // setting: the objective's height is an ordinary axis addressed with `ABSOLUTE`,
            // which is why a viewer can drive it by hand.
            descriptor(
                id,
                308,
                "spark-stage-xy",
                configured.serial_number.clone(),
                &["stage.xy", "axis.xy"],
                vec![
                    property("x", "X", ValueType::Position, Some("um"), false),
                    property("y", "Y", ValueType::Position, Some("um"), false),
                    property("unit", "Axis unit", ValueType::String, None, false),
                ],
            ),
            descriptor(
                id,
                309,
                "spark-stage-z",
                configured.serial_number.clone(),
                &["stage.z", "axis.z"],
                vec![
                    property("z", "Z", ValueType::Position, Some("um"), false),
                    property("unit", "Axis unit", ValueType::String, None, false),
                ],
            ),
            descriptor(
                id,
                310,
                "spark-filter-excitation",
                configured.serial_number.clone(),
                &["filter.wheel"],
                vec![
                    sequenceable_property("position", "Position", ValueType::I64, None, true),
                    property("slots", "Positions fitted", ValueType::I64, None, false),
                    property("fitted", "Fitted slide", ValueType::String, None, false),
                ],
            ),
            descriptor(
                id,
                311,
                "spark-mirror",
                configured.serial_number.clone(),
                &["mirror.turret"],
                vec![
                    sequenceable_property("position", "Position", ValueType::I64, None, true),
                    property("slots", "Positions fitted", ValueType::I64, None, false),
                    property("fitted", "Fitted carrier", ValueType::String, None, false),
                ],
            ),
            // One device for both pumps: the request names which pump acts, so a device per
            // pump would ask the same question twice and let the two answers disagree.
            descriptor(
                id,
                312,
                "spark-injector",
                configured.serial_number.clone(),
                &["injector"],
                vec![property("pumps", "Pumps", ValueType::String, None, false)],
            ),
            // The imaging camera, as the reader presents it. Its pixels come back over the
            // TDCL data channel in answer to `CAMERA TAKEIMAGE`, the same 0x88 header plus
            // 0x83 payload framing a measurement package uses — so the camera is reachable
            // through the reader without touching the vendor camera SDK.
            descriptor(
                id,
                315,
                "spark-camera",
                configured.serial_number.clone(),
                &["camera"],
                vec![
                    sequenceable_property(
                        "exposure",
                        "Exposure",
                        ValueType::TimeInterval,
                        Some("s"),
                        true,
                    ),
                    property("width", "Width", ValueType::PixelCount, None, false),
                    property("height", "Height", ValueType::PixelCount, None, false),
                    property(
                        "pixel_format",
                        "Pixel format",
                        ValueType::String,
                        None,
                        false,
                    ),
                ],
            ),
            descriptor(
                id,
                316,
                "spark-shaker",
                configured.serial_number.clone(),
                &["shaker"],
                vec![
                    sequenceable_property("mode", "Mode", ValueType::String, None, true),
                    sequenceable_property(
                        "amplitude",
                        "Amplitude",
                        ValueType::Position,
                        Some("um"),
                        true,
                    ),
                    sequenceable_property(
                        "frequency",
                        "Frequency",
                        ValueType::Frequency,
                        Some("Hz"),
                        true,
                    ),
                ],
            ),
            descriptor(
                id,
                317,
                "spark-lid",
                configured.serial_number.clone(),
                &["lid"],
                vec![sequenceable_property(
                    "state",
                    "State",
                    ValueType::String,
                    None,
                    true,
                )],
            ),
            descriptor(
                id,
                318,
                "spark-autofocus",
                configured.serial_number.clone(),
                &["autofocus"],
                vec![
                    property("max_value", "Peak value", ValueType::F64, None, false),
                    property("std_dev", "Peak spread", ValueType::F64, None, false),
                ],
            ),
            descriptor(
                id,
                314,
                "spark-barcode",
                configured.serial_number.clone(),
                &["barcode.reader"],
                vec![property(
                    "barcode",
                    "Barcode",
                    ValueType::String,
                    None,
                    false,
                )],
            ),
        ];
        // Optional hardware leaves the graph when the machine was not ordered with it. An
        // application asking what this instrument can do gets an answer it can act on, rather
        // than a list of everything the model line has ever been sold with.
        devices.retain(|device| match device.id.0 .0 {
            310 => configured.filter_positions > 0,
            311 => configured.mirror_positions > 0,
            312 => configured.injector_pumps > 0,
            314 => configured.barcode_fitted,
            _ => true,
        });
        for device in &mut devices {
            match device.id.0 .0 {
                310 => declare_positions(device, configured.filter_positions),
                311 => declare_positions(device, configured.mirror_positions),
                312 => {
                    device
                        .metadata
                        .insert("pumps".into(), Value::I64(configured.injector_pumps));
                }
                _ => {}
            }
        }
        let serial = configured.serial_number.clone();
        Self {
            id,
            backend: None,
            next_token: 1,
            events: VecDeque::new(),
            devices,
            label: configured.label,
            serial_number: configured.serial_number,
            support_level: "TDCL/CAN graph and transaction model with typed state operations"
                .into(),
            well: configured.well,
            absorbance_wavelength: configured.absorbance_wavelength,
            fluorescence_wavelength: configured.fluorescence_wavelength,
            luminescence_enabled: configured.luminescence_enabled,
            fluorescence_enabled: configured.fluorescence_enabled,
            temperature_target: configured.temperature_target,
            temperature_enabled: configured.temperature_enabled,
            temperature_actual: Some(configured.temperature_target),
            gas_target: configured.gas_target,
            gas_actual: Some(configured.gas_actual),
            o2_target: configured.o2_target,
            o2_actual: Some(configured.o2_actual),
            gas_enabled: configured.gas_enabled,
            gas_fault: configured.gas_fault,
            fim_objective: configured.fim_objective,
            fim_mode: configured.fim_mode,
            fim_interlock_closed: configured.fim_interlock_closed,
            fim_fault: configured.fim_fault,
            camera_bound: configured.camera_bound,
            imaging_mode: configured.imaging_mode,
            axis_units: BTreeMap::new(),
            axis_positions: BTreeMap::new(),
            carrier_positions: BTreeMap::from([(310, 1), (311, 1)]),
            carrier_slots: BTreeMap::new(),
            carrier_inventory: BTreeMap::new(),
            barcode: None,
            shake_mode: "LINEAR".into(),
            shake_amplitude: Position::from_micrometers(2000.0),
            shake_frequency: Frequency::from_hertz(6.0),
            lid_state: "DOWN".into(),
            autofocus: None,
            identity: BTreeMap::new(),
            camera: backend::CameraState {
                exposure_us: Some(10_000),
                ..backend::CameraState::default()
            },
            camera_exposure: TimeInterval::from_milliseconds(10.0),
            next_frame: 0,
            modules: configured.modules,
            label_index: 1,
            usb: configured
                .vendor_id
                .zip(configured.product_id)
                .map(|(vendor_id, product_id)| crate::spark::usb::UsbConfig {
                    serial: serial.clone(),
                    ..crate::spark::usb::UsbConfig::new(vendor_id, product_id)
                }),
        }
    }

    pub fn graph(&self) -> DeviceGraph {
        let mut graph = DeviceGraph::default();
        let hub = NodeId(300);
        let _ = graph.insert_node(GraphNode {
            id: hub,
            kind: NodeKind::Hub,
            label: "spark-mainboard".into(),
        });
        for device in &self.devices {
            if device.id.0 != hub {
                let _ = graph.insert_node(GraphNode {
                    id: device.id.0,
                    kind: NodeKind::Device,
                    label: device.label.clone(),
                });
                let _ = graph.insert_edge(GraphEdge {
                    from: hub,
                    to: device.id.0,
                    kind: EdgeKind::OffersDevice,
                });
            }
        }
        graph
    }

    /// Attach a transport, so capability requests become TDCL commands.
    ///
    /// Until this is called the driver answers from its own modeled state — which is what
    /// the examples and the device graph exercise, and what keeps this driver useful with
    /// no instrument present.
    pub fn attach(&mut self, transport: impl Transport + 'static) {
        self.backend = Some(Backend {
            session: SparkSession::new(BoxedTransport::new(transport)),
            outstanding: VecDeque::new(),
            results: HashMap::new(),
        });
        // Everything the model was standing in for is now the instrument's to say. Readings
        // in particular: a modeled chamber temperature carried past this point would be
        // presented as a measurement of hardware nobody has asked yet.
        self.temperature_actual = None;
        self.gas_actual = None;
        self.o2_actual = None;
        self.axis_units.clear();
        self.axis_positions.clear();
        self.barcode = None;
        self.carrier_slots.clear();
        self.carrier_inventory.clear();
        self.identity.clear();
        self.modules = backend::Modules::default();
        // Ask what this is and which modules it carries first. Everything else is addressed
        // *at* a module, so the rest of the bring-up waits for the enumeration to answer.
        let _ = self.identify();
    }

    /// Open the configured reader over USB and attach it.
    ///
    /// Fails when no USB identity is configured, which is the state until someone runs
    /// `lsusb -v` on an instrument: the id is not in the recovered evidence and this driver
    /// has none to fall back on.
    pub fn connect(&mut self) -> Result<()> {
        let config = self.usb.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "this Spark Cyto has no configured USB vendor/product id, so there is nothing \
                 to open; see docs/devices/spark-cyto.md for the config keys and \
                 docs/reverse/spark-cyto.md for how to find the id",
            )
        })?;
        let transport = crate::spark::usb::UsbTransport::open(&config)?;
        self.attach(transport);
        Ok(())
    }

    /// How this driver would reach the instrument, for a client that wants to report it.
    pub fn usb_config(&self) -> Option<&crate::spark::usb::UsbConfig> {
        self.usb.as_ref()
    }

    pub fn detach(&mut self) {
        self.backend = None;
    }

    /// Is this driver talking to hardware, or answering from its model?
    pub fn is_live(&self) -> bool {
        self.backend.is_some()
    }

    /// Which detector a device is, for the measurement command.
    fn detector_of(&self, device: DeviceId) -> Option<Detector> {
        let descriptor = self.devices.iter().find(|d| d.id == device)?;
        if descriptor.kinds.iter().any(|k| k == "detector.absorbance") {
            Some(Detector::Absorbance)
        } else if descriptor
            .kinds
            .iter()
            .any(|k| k == "detector.fluorescence")
        {
            Some(Detector::Fluorescence)
        } else if descriptor
            .kinds
            .iter()
            .any(|k| k == "detector.luminescence")
        {
            Some(Detector::Luminescence)
        } else {
            None
        }
    }

    /// What a device is to the instrument, from the kind tags it publishes.
    fn subject_of(&self, device: DeviceId) -> Option<backend::Subject> {
        let descriptor = self.devices.iter().find(|d| d.id == device)?;
        let has = |kind: &str| descriptor.kinds.iter().any(|k| k == kind);
        if has("plate.transport") {
            return Some(backend::Subject::PlateTransport);
        }
        if let Some(detector) = self.detector_of(device) {
            return Some(backend::Subject::Detector(detector));
        }
        if has("environment.temperature") {
            return Some(backend::Subject::Temperature);
        }
        if has("environment.gas") {
            return Some(backend::Subject::Gas);
        }
        if has("imaging.head") || has("objective.turret") {
            return Some(backend::Subject::ImagingHead);
        }
        if has("camera.binding") {
            return Some(backend::Subject::CameraBinding);
        }
        if has("axis.xy") {
            return Some(backend::Subject::Axes(vec![MtpMotor::X, MtpMotor::Y]));
        }
        if has("axis.z") {
            return Some(backend::Subject::Axes(vec![MtpMotor::Z]));
        }
        if has("filter.wheel") {
            return Some(backend::Subject::Carrier(MoveableCarrier::ExcitationFilter));
        }
        if has("mirror.turret") {
            return Some(backend::Subject::Carrier(MoveableCarrier::Mirror));
        }
        if has("injector") {
            return Some(backend::Subject::Injector);
        }
        if has("barcode.reader") {
            return Some(backend::Subject::Barcode);
        }
        if has("camera") {
            return Some(backend::Subject::Camera);
        }
        if has("shaker") {
            return Some(backend::Subject::Shaker);
        }
        if has("autofocus") {
            return Some(backend::Subject::Autofocus);
        }
        if has("lid") {
            return Some(backend::Subject::Lid);
        }
        None
    }

    /// Driver-side state the planner needs and a request does not carry.
    fn plan_state(&self, device: DeviceId) -> backend::PlanState {
        let wavelength_nm = self
            .detector_of(device)
            .and_then(|detector| match detector {
                Detector::Absorbance => {
                    Some(self.absorbance_wavelength.nanometers().round() as u32)
                }
                Detector::Fluorescence => {
                    Some(self.fluorescence_wavelength.nanometers().round() as u32)
                }
                // Luminescence has no excitation and tunes nothing.
                Detector::Luminescence => None,
            });
        backend::PlanState {
            well: self.well.clone(),
            wavelength_nm,
            label: self.label_index,
            axis_units: self.axis_units.clone(),
            carrier_slots: self.carrier_slots.clone(),
            modules: self.modules.clone(),
            camera: self.camera,
        }
    }

    /// Send a capability request to the instrument, if there is one attached and it has a
    /// command for it.
    ///
    /// Returns `true` when the request went to the wire, in which case the token completes
    /// later from [`Driver::poll`] rather than now. An attached instrument that has no
    /// command for the request is an error, not a fallback: answering it from the model
    /// would report a state the hardware was never asked to reach.
    /// Refuse a request the addressed device cannot serve, before it reaches either the wire
    /// or the model.
    ///
    /// This has to sit ahead of both paths. Checking inside the modeled handler leaves the
    /// live path unguarded: the command would be planned and written to the instrument first,
    /// and firmware that clamps an out-of-range position into the slot holding different
    /// glass would report success for a move nobody asked for.
    ///
    /// What the instrument itself reported wins over what it was ordered with — a carrier's
    /// inventory reply is evidence, the configured count is only a declaration.
    fn validate_request(&self, device: DeviceId, request: &CapabilityRequest) -> Result<()> {
        let declared = |key: &str| {
            self.devices
                .iter()
                .find(|candidate| candidate.id == device)
                .and_then(|candidate| match candidate.metadata.get(key) {
                    Some(Value::I64(count)) => Some(*count),
                    _ => None,
                })
                .unwrap_or(0)
        };
        match request {
            CapabilityRequest::FilterSelect(select) => {
                let positions = self
                    .carrier_name(device)
                    .and_then(|name| self.carrier_slots.get(name).copied())
                    .map(i64::from)
                    .unwrap_or_else(|| declared("positions"));
                let position = select.position as i64;
                if positions > 0 && !(1..=positions).contains(&position) {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        format!("this carrier has positions 1..={positions}, not {position}"),
                    ));
                }
            }
            CapabilityRequest::Inject(inject) => {
                let pumps = declared("pumps");
                let pump = inject.pump as i64;
                if pumps > 0 && !(1..=pumps).contains(&pump) {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        format!("this injector has pumps 1..={pumps}, not {pump}"),
                    ));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn dispatch_to_instrument(
        &mut self,
        token: DriverToken,
        device: DeviceId,
        request: &CapabilityRequest,
    ) -> Result<bool> {
        if self.backend.is_none() {
            return Ok(false);
        }
        let Some(subject) = self.subject_of(device) else {
            return Ok(false);
        };
        let state = self.plan_state(device);
        let transactions = match backend::plan_request(request, &subject, &state) {
            backend::Planned::Wire(transactions) => transactions,
            backend::Planned::Local => return Ok(false),
            backend::Planned::Unsupported(reason) => {
                return Err(Error::new(ErrorCode::Unsupported, reason))
            }
        };
        if transactions.is_empty() {
            return Ok(false);
        }
        self.submit(token, transactions)?;
        Ok(true)
    }

    /// Send a property write to the instrument, if there is one attached and it has a command
    /// for that property.
    ///
    /// Without this a live driver would accept `wavelength` or `target`, change a number in
    /// memory and send nothing — the hardware would only find out at the next capability
    /// request that happened to carry the value along. A write is an instruction, and the
    /// instrument should hear it when it is given.
    fn write_to_instrument(
        &mut self,
        token: DriverToken,
        device: DeviceId,
        key: &str,
        value: &Value,
    ) -> Result<bool> {
        if self.backend.is_none() {
            return Ok(false);
        }
        let Some(subject) = self.subject_of(device) else {
            return Ok(false);
        };
        let state = self.plan_state(device);
        // The lid is a state write with a command of its own rather than a capability.
        if let (backend::Subject::Lid, Value::String(lid_state)) = (&subject, value) {
            let transaction = backend::lid_command(&lid_state.trim().to_ascii_uppercase(), None);
            self.submit(token, vec![transaction])?;
            return Ok(true);
        }
        let transactions = match backend::plan_write(&subject, key, value, &state) {
            backend::Planned::Wire(transactions) => transactions,
            backend::Planned::Local => return Ok(false),
            backend::Planned::Unsupported(reason) => {
                return Err(Error::new(ErrorCode::Unsupported, reason))
            }
        };
        if transactions.is_empty() {
            return Ok(false);
        }
        self.submit(token, transactions)?;
        Ok(true)
    }

    /// Queue transactions on the attached session, completing the token with the last one.
    fn submit(
        &mut self,
        token: DriverToken,
        transactions: Vec<backend::Transaction>,
    ) -> Result<()> {
        let Some(backend) = self.backend.as_mut() else {
            return Ok(());
        };
        let last = transactions.len().saturating_sub(1);
        for (index, transaction) in transactions.into_iter().enumerate() {
            backend
                .outstanding
                .push_back((token, transaction.intent, index == last));
            backend.session.submit(token, transaction.line)?;
        }
        Ok(())
    }

    /// Ask what the instrument is and which modules it carries.
    ///
    /// Submitted under a token nobody waits on. When the enumeration answers, the rest of the
    /// bring-up follows from [`Self::refresh`] — commands that name a module cannot be built
    /// before the module numbers are known.
    pub fn identify(&mut self) -> Result<()> {
        if self.backend.is_none() {
            return Ok(());
        }
        let token = self.next_token();
        let mut transactions = backend::identity_reads();
        transactions.extend(backend::module_reads());
        self.submit(token, transactions)
    }

    /// The wire name of the carrier a device stands for.
    fn carrier_name(&self, device: DeviceId) -> Option<&'static str> {
        match self.subject_of(device) {
            Some(backend::Subject::Carrier(carrier)) => Some(carrier.wire_token()),
            _ => None,
        }
    }

    fn identity_value(&self, key: &str) -> Value {
        optional(self.identity.get(key).cloned().map(Value::String))
    }

    /// The modules this driver knows the numbers for, for the hub's readback.
    fn module_summary(&self) -> String {
        let modules = &self.modules;
        let mut parts = Vec::new();
        for (name, number) in [
            (
                if modules.cell_imaging { "CELL" } else { "FIM" },
                modules.imaging,
            ),
            ("INJ", modules.injector),
            ("GCM", modules.gas),
            ("BARCODE", modules.barcode),
        ] {
            if let Some(number) = number {
                parts.push(format!("{name}:{number}"));
            }
        }
        if parts.is_empty() {
            // Not "none fitted" — nobody has asked yet, or the answer has not arrived.
            return "unknown".into();
        }
        parts.join("|")
    }

    /// Ask the instrument what it is doing: the chamber sensors, the axis units and the axis
    /// positions.
    ///
    /// Submitted under a token nobody is waiting on, because these answer properties rather
    /// than an operation. The axis-unit query is what lets a position be commanded in
    /// micrometres at all; until it answers, a move is refused rather than guessed.
    pub fn refresh(&mut self) -> Result<()> {
        if self.backend.is_none() {
            return Ok(());
        }
        let token = self.next_token();
        let mut transactions = backend::environment_reads(&self.modules);
        transactions.extend(backend::axis_range_reads(
            &[MtpMotor::X, MtpMotor::Y, MtpMotor::Z],
            &self.modules,
        ));
        transactions.push(backend::position_read(&self.modules));
        transactions.extend(backend::camera_reads(&self.modules));
        transactions.extend(backend::inventory_reads(&[
            MoveableCarrier::ExcitationFilter,
            MoveableCarrier::Mirror,
        ]));
        self.submit(token, transactions)
    }

    /// Drain the session and turn finished transactions into driver events.
    fn poll_instrument(&mut self) {
        let Some(backend) = self.backend.as_mut() else {
            return;
        };
        let progress = match backend.session.poll() {
            Ok(progress) => progress,
            Err(error) => {
                let report = ErrorReport {
                    code: error.code,
                    message: error.message.clone(),
                };
                // A transport failure ends every command riding on it; completing them
                // individually would leave a client waiting on the ones that never went.
                backend.results.clear();
                for (token, _, terminal) in backend.outstanding.drain(..) {
                    if terminal {
                        self.events.push_back(DriverEvent::TokenFailed {
                            token,
                            report: report.clone(),
                        });
                    }
                }
                return;
            }
        };

        // What the replies said about state, applied after the borrow on the session ends.
        let mut readbacks: Vec<(backend::Intent, Value)> = Vec::new();
        for event in progress {
            match event {
                Progress::Completed(outcome) => {
                    let Some((token, intent, terminal)) = backend.outstanding.pop_front() else {
                        continue;
                    };
                    let value = backend::completion(&intent, &outcome);
                    // Anything that is not a bare acknowledgement is part of the answer.
                    // A reference read is nested rather than merged: it reports the same
                    // `reference`/`measurement` keys as the sample read, and flattening the
                    // two would silently overwrite the blank with the sample.
                    match (&intent, &value) {
                        (backend::Intent::Acknowledge, _) => {}
                        (backend::Intent::Prepare { .. }, value) => {
                            backend
                                .results
                                .entry(token)
                                .or_default()
                                .insert("prepare".into(), value.clone());
                        }
                        (_, Value::Map(map)) => {
                            backend
                                .results
                                .entry(token)
                                .or_default()
                                .extend(map.clone());
                        }
                        (_, value) => {
                            backend
                                .results
                                .entry(token)
                                .or_default()
                                .insert("result".into(), value.clone());
                        }
                    }
                    // An acquisition's payload is the raster itself. Publish it as a frame
                    // before completing the operation, so a client that wakes on the
                    // completion finds the image already in the store.
                    if let backend::Intent::Capture {
                        width,
                        height,
                        bits_per_pixel,
                    } = intent
                    {
                        let device = DeviceId(NodeId(315));
                        match backend::decode_image(&outcome, width, height, bits_per_pixel) {
                            Ok(data) => {
                                let frame = self.next_frame;
                                self.next_frame += 1;
                                self.events.push_back(DriverEvent::FrameReady(Frame {
                                    handle: FrameHandle {
                                        stream: StreamId(315),
                                        frame: FrameId(frame),
                                    },
                                    device,
                                    width,
                                    height,
                                    pixel_format: backend::pixel_format(bits_per_pixel).into(),
                                    data,
                                    metadata: BTreeMap::from([
                                        (
                                            "exposure".into(),
                                            Value::TimeInterval(self.camera_exposure),
                                        ),
                                        (
                                            "bits_per_pixel".into(),
                                            Value::I64(bits_per_pixel as i64),
                                        ),
                                    ]),
                                    buffer: FrameBufferSpec::default(),
                                }));
                            }
                            Err(reason) => {
                                // A raster that does not match the geometry the camera
                                // reported is not an image of anything.
                                self.events.push_back(DriverEvent::TokenFailed {
                                    token,
                                    report: ErrorReport {
                                        code: ErrorCode::Transport,
                                        message: reason,
                                    },
                                });
                                continue;
                            }
                        }
                    }
                    readbacks.push((intent, value.clone()));
                    if terminal {
                        let value = match backend.results.remove(&token) {
                            Some(collected) if !collected.is_empty() => Value::Map(collected),
                            _ => value,
                        };
                        self.events
                            .push_back(DriverEvent::TokenCompleted { token, value });
                    }
                }
                Progress::Failed(failure) => {
                    // Everything queued behind a failed command belongs to operations that
                    // will now never run as asked, so they fail with it.
                    if let Some((token, _, _)) = backend.outstanding.pop_front() {
                        backend.results.remove(&token);
                        self.events.push_back(DriverEvent::TokenFailed {
                            token,
                            report: ErrorReport {
                                code: ErrorCode::Driver,
                                message: match failure.number {
                                    Some(number) => {
                                        format!("instrument error {number}: {}", failure.text)
                                    }
                                    None => failure.text.clone(),
                                },
                            },
                        });
                    }
                }
                Progress::Busy { .. } => {
                    // Still working. Nothing to report: the operation stays in progress.
                }
                Progress::Asynchronous { number, text } => {
                    self.events
                        .push_back(DriverEvent::Event(Event::Log(LogEvent {
                            driver: Some(self.id),
                            message: match number {
                                Some(number) => format!("Spark fault {number}: {text}"),
                                None => format!("Spark fault: {text}"),
                            },
                        })));
                }
            }
        }

        for (intent, value) in readbacks {
            self.apply_readback(&intent, &value);
        }
    }

    /// Fold a reply into driver state, so a property read reports what the instrument said.
    fn apply_readback(&mut self, intent: &backend::Intent, value: &Value) {
        match intent {
            backend::Intent::Read { key } if key == "AOI" => {
                let Value::Map(map) = value else { return };
                for (key, target) in [("WIDTH", true), ("HEIGHT", false)] {
                    if let Some(Value::I64(raw)) = map.get(key) {
                        let raw = u32::try_from(*raw).ok();
                        if target {
                            self.camera.width = raw;
                        } else {
                            self.camera.height = raw;
                        }
                    }
                }
            }

            backend::Intent::Read { key } => {
                let Value::I64(raw) = value else { return };
                match key.as_str() {
                    "TEMPERATURE" => {
                        let actual = backend::temperature_from_c100(*raw);
                        self.temperature_actual = Some(actual);
                        self.emit_property(
                            DeviceId(NodeId(304)),
                            "actual",
                            Value::Temperature(actual),
                        );
                    }
                    "BITSPERPIXEL" => {
                        self.camera.bits_per_pixel = u8::try_from(*raw).ok();
                    }
                    "ACTUAL_CONCENTRATION_CO2" => {
                        let actual = backend::gas_from_scaled(*raw);
                        self.gas_actual = Some(actual);
                        self.emit_property(
                            DeviceId(NodeId(305)),
                            "co2_actual",
                            Value::GasConcentration(actual),
                        );
                    }
                    "ACTUAL_CONCENTRATION_O2" => {
                        let actual = backend::gas_from_scaled(*raw);
                        self.o2_actual = Some(actual);
                        self.emit_property(
                            DeviceId(NodeId(305)),
                            "o2_actual",
                            Value::GasConcentration(actual),
                        );
                    }
                    _ => {}
                }
            }

            backend::Intent::AxisRange { axis } => {
                let Value::Map(map) = value else { return };
                let Some(Value::String(unit)) = map.get("unit") else {
                    return;
                };
                let unit = match unit.as_str() {
                    "um" => backend::AxisUnit::Micrometres,
                    _ => backend::AxisUnit::Steps,
                };
                self.axis_units.insert(axis.clone(), unit);
                let device = match axis {
                    StageAxis::Z => DeviceId(NodeId(309)),
                    _ => DeviceId(NodeId(308)),
                };
                self.emit_property(device, "unit", Value::String(unit_name(unit).into()));
            }

            backend::Intent::Position => {
                let Value::Map(map) = value else { return };
                for (axis, key, device) in [
                    (StageAxis::X, "x", 308u64),
                    (StageAxis::Y, "y", 308),
                    (StageAxis::Z, "z", 309),
                ] {
                    let Some(Value::F64(raw)) = map.get(key) else {
                        continue;
                    };
                    self.axis_positions.insert(axis.clone(), *raw);
                    let Some(unit) = self.axis_units.get(&axis).copied() else {
                        continue;
                    };
                    if let Some(position) = backend::position_from_raw(*raw, unit) {
                        self.emit_property(
                            DeviceId(NodeId(device)),
                            key,
                            Value::Position(position),
                        );
                    }
                }
            }

            backend::Intent::Identity { key } => {
                let Value::String(text) = value else { return };
                self.identity.insert(key.clone(), text.clone());
                let property = match key.as_str() {
                    "INSTRUMENT_TYPE" => "instrument_type",
                    "HARDWARE_VERSION" => "hardware_version",
                    "STATE" => "state",
                    // The serial is descriptor metadata, not a property.
                    _ => return,
                };
                self.emit_property(DeviceId(NodeId(300)), property, value.clone());
            }

            backend::Intent::ModuleMap { final_bus } => {
                let Value::Map(map) = value else { return };
                let enumerated = map
                    .iter()
                    .filter_map(|(name, value)| match value {
                        Value::I64(number) => {
                            u32::try_from(*number).ok().map(|n| (name.clone(), n))
                        }
                        _ => None,
                    })
                    .collect();
                self.modules.apply(&enumerated);
                self.camera.cell_imaging = self.modules.cell_imaging;
                self.emit_property(
                    DeviceId(NodeId(300)),
                    "modules",
                    Value::String(self.module_summary()),
                );
                // Module numbers are what the rest of the bring-up needs to address anything,
                // so it waits for the last enumeration rather than running once per bus.
                if *final_bus {
                    let _ = self.refresh();
                }
            }

            backend::Intent::CarrierInventory { carrier } => {
                let Value::Map(map) = value else { return };
                let name = carrier.wire_token();
                let device = match carrier {
                    MoveableCarrier::Mirror | MoveableCarrier::DualPmtMirror => {
                        DeviceId(NodeId(311))
                    }
                    _ => DeviceId(NodeId(310)),
                };
                if let Some(Value::I64(slots)) = map.get("slots") {
                    if let Ok(slots) = u8::try_from(*slots) {
                        self.carrier_slots.insert(name.to_string(), slots);
                        self.emit_property(device, "slots", Value::I64(slots as i64));
                    }
                }
                if let Some(Value::String(fitted)) = map.get("fitted") {
                    self.carrier_inventory
                        .insert(name.to_string(), fitted.clone());
                    self.emit_property(device, "fitted", Value::String(fitted.clone()));
                }
            }

            backend::Intent::Autofocus => {
                let Value::Map(map) = value else { return };
                let read = |key: &str| match map.get(key) {
                    Some(Value::F64(raw)) => Some(*raw),
                    Some(Value::I64(raw)) => Some(*raw as f64),
                    _ => None,
                };
                if let (Some(max), Some(dev)) = (read("maxvalue"), read("stddev")) {
                    self.autofocus = Some((max, dev));
                    let device = DeviceId(NodeId(318));
                    self.emit_property(device, "max_value", Value::F64(max));
                    self.emit_property(device, "std_dev", Value::F64(dev));
                }
            }

            backend::Intent::Barcode => {
                self.barcode = match value {
                    Value::String(text) => Some(text.clone()),
                    _ => None,
                };
                self.emit_property(DeviceId(NodeId(314)), "barcode", value.clone());
            }

            _ => {}
        }
    }

    fn next_token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    /// A modeled chamber reaches its setpoint; a real one is asked.
    ///
    /// The two are kept apart deliberately: with an instrument attached these readings only
    /// ever come from it, so a simulator stays useful without a live driver ever reporting a
    /// number nobody measured.
    fn settle_temperature(&mut self) {
        if self.is_live() {
            return;
        }
        self.temperature_actual = Some(if self.temperature_enabled {
            self.temperature_target
        } else {
            Temperature::from_celsius(25.0)
        });
    }

    fn settle_gas(&mut self) {
        if self.is_live() {
            return;
        }
        if self.gas_enabled && !self.gas_fault {
            self.gas_actual = Some(self.gas_target);
            self.o2_actual = Some(self.o2_target);
        }
    }

    /// An axis position, in the unit the instrument said that axis counts in.
    ///
    /// `Null` when the axis has not reported, and when it counts in motor steps: a step is
    /// not a length until the mechanism says how long one is, and reporting the raw count as
    /// micrometres would be a wrong number rather than a missing one.
    fn axis_value(&self, axis: StageAxis) -> Value {
        let Some(raw) = self.axis_positions.get(&axis).copied() else {
            return Value::Null;
        };
        match self.axis_units.get(&axis).copied() {
            Some(unit) => optional(backend::position_from_raw(raw, unit).map(Value::Position)),
            None => Value::Null,
        }
    }

    fn axis_unit_value(&self, axis: StageAxis) -> Value {
        match self.axis_units.get(&axis).copied() {
            Some(unit) => Value::String(unit_name(unit).into()),
            None => Value::Null,
        }
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        match (device.0 .0, key) {
            (300, "well") => Ok(Value::String(self.well.clone())),
            (300, "support_level") => Ok(Value::String(self.support_level.clone())),
            (300, "instrument_type") => Ok(self.identity_value("INSTRUMENT_TYPE")),
            (300, "hardware_version") => Ok(self.identity_value("HARDWARE_VERSION")),
            (300, "state") => Ok(self.identity_value("STATE")),
            (300, "modules") => Ok(Value::String(self.module_summary())),
            (301, "wavelength") => Ok(Value::Wavelength(self.absorbance_wavelength)),
            (302, "wavelength") => Ok(Value::Wavelength(self.fluorescence_wavelength)),
            (302, "enabled") => Ok(Value::Bool(self.fluorescence_enabled)),
            (303, "enabled") => Ok(Value::Bool(self.luminescence_enabled)),
            (304, "target") => Ok(Value::Temperature(self.temperature_target)),
            // `Null` until something has been read. See `temperature_actual`.
            (304, "actual") => Ok(optional(self.temperature_actual.map(Value::Temperature))),
            (304, "enabled") => Ok(Value::Bool(self.temperature_enabled)),
            (305, "co2_target") => Ok(Value::GasConcentration(self.gas_target)),
            (305, "co2_actual") => Ok(optional(self.gas_actual.map(Value::GasConcentration))),
            (305, "o2_target") => Ok(Value::GasConcentration(self.o2_target)),
            (305, "o2_actual") => Ok(optional(self.o2_actual.map(Value::GasConcentration))),
            (305, "enabled") => Ok(Value::Bool(self.gas_enabled)),
            (305, "fault") => Ok(Value::Bool(self.gas_fault)),
            (306, "objective") => Ok(Value::I64(self.fim_objective)),
            (306, "mode") => Ok(Value::String(self.fim_mode.clone())),
            (306, "interlock_closed") => Ok(Value::Bool(self.fim_interlock_closed)),
            (306, "fault") => Ok(Value::Bool(self.fim_fault)),
            (307, "bound") => Ok(Value::Bool(self.camera_bound)),
            (307, "imaging_mode") => Ok(Value::String(self.imaging_mode.clone())),
            (308, "x") => Ok(self.axis_value(StageAxis::X)),
            (308, "y") => Ok(self.axis_value(StageAxis::Y)),
            (308, "unit") => Ok(self.axis_unit_value(StageAxis::X)),
            (309, "z") => Ok(self.axis_value(StageAxis::Z)),
            (309, "unit") => Ok(self.axis_unit_value(StageAxis::Z)),
            (310 | 311, "slots") => Ok(optional(
                self.carrier_name(device)
                    .and_then(|name| self.carrier_slots.get(name).copied())
                    .map(|slots| Value::I64(slots as i64)),
            )),
            (310 | 311, "fitted") => Ok(optional(
                self.carrier_name(device)
                    .and_then(|name| self.carrier_inventory.get(name).cloned())
                    .map(Value::String),
            )),
            (310 | 311, "position") => Ok(Value::I64(
                self.carrier_positions
                    .get(&device.0 .0)
                    .copied()
                    .unwrap_or(1) as i64,
            )),
            (312, "pumps") => Ok(Value::String("A|B".into())),
            (314, "barcode") => Ok(optional(self.barcode.clone().map(Value::String))),
            (315, "exposure") => Ok(Value::TimeInterval(self.camera_exposure)),
            (316, "mode") => Ok(Value::String(self.shake_mode.clone())),
            (316, "amplitude") => Ok(Value::Position(self.shake_amplitude)),
            (316, "frequency") => Ok(Value::Frequency(self.shake_frequency)),
            (317, "state") => Ok(Value::String(self.lid_state.clone())),
            (318, "max_value") => Ok(optional(self.autofocus.map(|(max, _)| Value::F64(max)))),
            (318, "std_dev") => Ok(optional(self.autofocus.map(|(_, dev)| Value::F64(dev)))),
            (315, "width") => Ok(optional(
                self.camera
                    .width
                    .map(|width| Value::PixelCount(PixelCount::new(width))),
            )),
            (315, "height") => Ok(optional(
                self.camera
                    .height
                    .map(|height| Value::PixelCount(PixelCount::new(height))),
            )),
            (315, "pixel_format") => {
                Ok(optional(self.camera.bits_per_pixel.map(|bits| {
                    Value::String(backend::pixel_format(bits).into())
                })))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Spark Cyto property {key}"),
            )),
        }
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        let descriptor = self
            .devices
            .iter()
            .find(|descriptor| descriptor.id == device)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown Spark Cyto device"))?;
        let schema = descriptor
            .properties
            .iter()
            .find(|property| property.key == key)
            .ok_or_else(|| Error::new(ErrorCode::InvalidProperty, "unknown property"))?;
        if !schema.writable {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "property is read-only",
            ));
        }
        schema.validate(value)
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: &Value) -> Result<Value> {
        self.validate_write(device, key, value)?;
        match (device.0 .0, key, value) {
            (300, "well", Value::String(well)) => {
                self.well = well.clone();
                Ok(Value::String(self.well.clone()))
            }
            (301, "wavelength", Value::Wavelength(wavelength)) => {
                self.absorbance_wavelength = *wavelength;
                Ok(Value::Wavelength(self.absorbance_wavelength))
            }
            (302, "wavelength", Value::Wavelength(wavelength)) => {
                self.fluorescence_wavelength = *wavelength;
                Ok(Value::Wavelength(self.fluorescence_wavelength))
            }
            (302, "enabled", Value::Bool(enabled)) => {
                self.fluorescence_enabled = *enabled;
                Ok(Value::Bool(self.fluorescence_enabled))
            }
            (303, "enabled", Value::Bool(enabled)) => {
                self.luminescence_enabled = *enabled;
                Ok(Value::Bool(self.luminescence_enabled))
            }
            (304, "target", Value::Temperature(target)) => {
                self.temperature_target = *target;
                self.settle_temperature();
                Ok(Value::Temperature(self.temperature_target))
            }
            (304, "enabled", Value::Bool(enabled)) => {
                self.temperature_enabled = *enabled;
                self.settle_temperature();
                Ok(Value::Bool(self.temperature_enabled))
            }
            (305, "co2_target", Value::GasConcentration(target)) => {
                self.gas_target = *target;
                self.settle_gas();
                Ok(Value::GasConcentration(self.gas_target))
            }
            (305, "o2_target", Value::GasConcentration(target)) => {
                self.o2_target = *target;
                self.settle_gas();
                Ok(Value::GasConcentration(self.o2_target))
            }
            (305, "enabled", Value::Bool(enabled)) => {
                self.gas_enabled = *enabled;
                self.settle_gas();
                Ok(Value::Bool(self.gas_enabled))
            }
            (316, "mode", Value::String(mode)) => {
                self.shake_mode = mode.trim().to_ascii_uppercase();
                Ok(Value::String(self.shake_mode.clone()))
            }
            (316, "amplitude", Value::Position(amplitude)) => {
                self.shake_amplitude = *amplitude;
                Ok(Value::Position(self.shake_amplitude))
            }
            (316, "frequency", Value::Frequency(frequency)) => {
                self.shake_frequency = *frequency;
                Ok(Value::Frequency(self.shake_frequency))
            }
            (317, "state", Value::String(state)) => {
                self.lid_state = state.trim().to_ascii_uppercase();
                Ok(Value::String(self.lid_state.clone()))
            }
            (315, "exposure", Value::TimeInterval(exposure)) => {
                self.camera_exposure = *exposure;
                self.camera.exposure_us = Some((exposure.seconds() * 1e6).round() as i64);
                Ok(Value::TimeInterval(self.camera_exposure))
            }
            (310 | 311, "position", Value::I64(position)) => {
                let position = (*position).clamp(1, u8::MAX as i64) as u8;
                self.carrier_positions.insert(device.0 .0, position);
                Ok(Value::I64(position as i64))
            }
            (306, "objective", Value::I64(objective)) => {
                self.fim_objective = (*objective).clamp(1, 6);
                Ok(Value::I64(self.fim_objective))
            }
            (306, "mode", Value::String(mode)) => {
                self.fim_mode = mode.clone();
                Ok(Value::String(self.fim_mode.clone()))
            }
            (307, "bound", Value::Bool(bound)) => {
                self.camera_bound = *bound;
                Ok(Value::Bool(self.camera_bound))
            }
            (307, "imaging_mode", Value::String(mode)) => {
                self.imaging_mode = mode.clone();
                Ok(Value::String(self.imaging_mode.clone()))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid Spark Cyto write {key}"),
            )),
        }
    }

    fn emit_property(&mut self, device: DeviceId, key: &str, value: Value) {
        self.events
            .push_back(DriverEvent::Event(Event::PropertyChanged(
                PropertyChanged {
                    device,
                    key: key.into(),
                    value,
                },
            )));
    }

    fn apply_state_set(&mut self, set: StateSet) -> Result<Value> {
        let mut changed = BTreeMap::new();
        for write in set.writes {
            let value = self.write_property(write.device, &write.property, &write.value)?;
            self.emit_property(write.device, &write.property, value.clone());
            changed.insert(format!("{}:{}", write.device.0 .0, write.property), value);
        }
        Ok(Value::Map(changed))
    }

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| (300..=318).contains(&sequence.device.0 .0))
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequences(plan) {
            if sequence.values.is_empty() {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "Spark Cyto timing sequence must contain at least one value",
                ));
            }
            for value in &sequence.values {
                self.validate_write(sequence.device, &sequence.property, value)?;
            }
        }
        Ok(())
    }

    fn timing_summary(&self, plan: &TimingPlan, phase: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("phase".into(), Value::String(phase.into())),
            ("well".into(), Value::String(self.well.clone())),
            (
                "absorbance_wavelength".into(),
                Value::Wavelength(self.absorbance_wavelength),
            ),
            (
                "fluorescence_wavelength".into(),
                Value::Wavelength(self.fluorescence_wavelength),
            ),
            (
                "fluorescence_enabled".into(),
                Value::Bool(self.fluorescence_enabled),
            ),
            (
                "luminescence_enabled".into(),
                Value::Bool(self.luminescence_enabled),
            ),
            (
                "temperature_target".into(),
                Value::Temperature(self.temperature_target),
            ),
            (
                "temperature_enabled".into(),
                Value::Bool(self.temperature_enabled),
            ),
            (
                "gas_target".into(),
                Value::GasConcentration(self.gas_target),
            ),
            (
                "gas_actual".into(),
                optional(self.gas_actual.map(Value::GasConcentration)),
            ),
            ("gas_enabled".into(), Value::Bool(self.gas_enabled)),
            ("gas_fault".into(), Value::Bool(self.gas_fault)),
            ("fim_objective".into(), Value::I64(self.fim_objective)),
            ("fim_mode".into(), Value::String(self.fim_mode.clone())),
            (
                "fim_interlock_closed".into(),
                Value::Bool(self.fim_interlock_closed),
            ),
            ("fim_fault".into(), Value::Bool(self.fim_fault)),
            ("camera_bound".into(), Value::Bool(self.camera_bound)),
            (
                "imaging_mode".into(),
                Value::String(self.imaging_mode.clone()),
            ),
            (
                "sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
        ]))
    }

    fn apply_timing_sequence_step(&mut self, plan: &TimingPlan, first: bool) -> Result<Value> {
        let writes = self
            .local_timing_sequences(plan)
            .into_iter()
            .filter_map(|sequence| {
                let value = if first {
                    sequence.values.first()
                } else {
                    sequence.values.last()
                }?;
                Some(StateWrite {
                    device: sequence.device,
                    property: sequence.property.clone(),
                    value: value.clone(),
                })
            })
            .collect::<Vec<_>>();
        if writes.is_empty() {
            return Ok(Value::Map(BTreeMap::new()));
        }
        self.apply_state_set(StateSet {
            name: Some(if first {
                "spark cyto timing start".into()
            } else {
                "spark cyto timing stop".into()
            }),
            writes,
            commit: CommitMode::Immediate,
        })
    }

    fn capability_kind(
        &self,
        device: DeviceId,
        capability: CapabilityId,
    ) -> Result<CapabilityKind> {
        self.capabilities(device)
            .into_iter()
            .find(|descriptor| descriptor.id == capability)
            .map(|descriptor| descriptor.kind)
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::Unsupported,
                    format!(
                        "Spark Cyto device {:?} does not expose capability {:?}",
                        device, capability
                    ),
                )
            })
    }

    fn validate_invoke(
        &self,
        device: DeviceId,
        capability: CapabilityId,
        request: &CapabilityRequest,
    ) -> Result<()> {
        let kind = self.capability_kind(device, capability)?;
        if kind.preferred_request_kind().accepts(request)
            || matches!(request, CapabilityRequest::None)
            || matches!(
                (&kind, request),
                (
                    CapabilityKind::GenericCommand,
                    CapabilityRequest::GenericCommand(_)
                ) | (CapabilityKind::Custom(_), CapabilityRequest::Custom(_))
            )
        {
            if let CapabilityRequest::GenericCommand(request) = request {
                if request.is_hidden_maintenance() {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        format!(
                            "GenericCommand {} is a hidden maintenance operation",
                            request.command
                        ),
                    ));
                }
            }
            if let CapabilityRequest::Custom(value) = request {
                if generic_command_value_is_hidden_maintenance(value) {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "custom request contains a hidden maintenance operation",
                    ));
                }
            }
            Ok(())
        } else {
            Err(Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "{} expects {:?}, got {:?}",
                    kind.name(),
                    kind.preferred_request_kind(),
                    request.request_kind()
                ),
            ))
        }
    }

    fn invoke_capability(
        &mut self,
        device: DeviceId,
        capability: CapabilityId,
        request: CapabilityRequest,
    ) -> Result<Value> {
        match self.capability_kind(device, capability)? {
            CapabilityKind::PlateMove => self.invoke_plate_move(device, request),
            CapabilityKind::Measure => self.invoke_measure(device, request),
            CapabilityKind::TemperatureControl => self.invoke_temperature_control(device, request),
            CapabilityKind::GasControl => self.invoke_gas_control(device, request),
            CapabilityKind::ImagingHead => self.invoke_imaging_head(device, request),
            CapabilityKind::CameraBinding => self.invoke_camera_binding(device, request),
            CapabilityKind::StageMove => self.invoke_stage_move(device, request),
            CapabilityKind::StageHome => Ok(Value::Map(BTreeMap::from([(
                "homed".into(),
                Value::Bool(true),
            )]))),
            CapabilityKind::FilterSelect => self.invoke_filter_select(device, request),
            CapabilityKind::Inject => self.invoke_inject(device, request),
            CapabilityKind::CameraCapture => Err(Error::new(
                ErrorCode::Unsupported,
                "this driver has no instrument attached and no scene to render, so there are \
                 no pixels to return; attach a transport, or use a simulator driver for a \
                 modeled camera",
            )),
            CapabilityKind::Shake => Ok(Value::Map(BTreeMap::from([
                ("mode".into(), Value::String(self.shake_mode.clone())),
                ("amplitude".into(), Value::Position(self.shake_amplitude)),
                ("frequency".into(), Value::Frequency(self.shake_frequency)),
            ]))),
            CapabilityKind::Autofocus => Err(Error::new(
                ErrorCode::Unsupported,
                "an autofocus sweep needs an instrument to sweep; this driver has no scene \
                 to focus on",
            )),
            CapabilityKind::Barcode => Ok(Value::Map(BTreeMap::from([(
                "barcode".into(),
                optional(self.barcode.clone().map(Value::String)),
            )]))),
            CapabilityKind::GenericCommand | CapabilityKind::Custom(_) => {
                Ok(capability_request_summary(request))
            }
            kind => Err(Error::new(
                ErrorCode::Unsupported,
                format!("Spark Cyto does not implement {}", kind.name()),
            )),
        }
    }

    /// Move an axis, with no instrument attached.
    ///
    /// The modeled path accepts a position in micrometres, which is what a client that has
    /// only ever seen the model will send. With an instrument attached this is not reached:
    /// the planner refuses the move unless that axis declared micrometres.
    fn invoke_stage_move(&mut self, device: DeviceId, request: CapabilityRequest) -> Result<Value> {
        let CapabilityRequest::StageMove(request) = request else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "StageMove expects StageMoveRequest",
            ));
        };
        let mut moved = BTreeMap::new();
        for (axis, position) in &request.target {
            let owned = matches!(
                (device.0 .0, axis),
                (308, StageAxis::X | StageAxis::Y) | (309, StageAxis::Z)
            );
            if !owned {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    format!("this Spark stage does not carry the {} axis", axis.name()),
                ));
            }
            let micrometers = if request.relative {
                self.axis_positions.get(axis).copied().unwrap_or(0.0) + position.micrometers()
            } else {
                position.micrometers()
            };
            self.axis_positions.insert(axis.clone(), micrometers);
            self.axis_units
                .insert(axis.clone(), backend::AxisUnit::Micrometres);
            let value = Value::Position(Position::from_micrometers(micrometers));
            self.emit_property(device, axis.name(), value.clone());
            moved.insert(axis.name().to_string(), value);
        }
        Ok(Value::Map(moved))
    }

    fn invoke_filter_select(
        &mut self,
        device: DeviceId,
        request: CapabilityRequest,
    ) -> Result<Value> {
        let CapabilityRequest::FilterSelect(request) = request else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "FilterSelect expects FilterSelectRequest",
            ));
        };
        // The same refusal the live path makes: a position the fitted slide does not have is
        // rejected rather than clamped into whatever glass sits nearest.
        if let Some(slots) = self
            .carrier_name(device)
            .and_then(|name| self.carrier_slots.get(name).copied())
        {
            if request.position < 1 || request.position > slots {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    format!(
                        "this carrier has {slots} positions; there is no position {}",
                        request.position
                    ),
                ));
            }
        }
        let value =
            self.write_property(device, "position", &Value::I64(request.position as i64))?;
        self.emit_property(device, "position", value.clone());
        Ok(Value::Map(BTreeMap::from([("position".into(), value)])))
    }

    fn invoke_inject(&mut self, _device: DeviceId, request: CapabilityRequest) -> Result<Value> {
        let CapabilityRequest::Inject(request) = request else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Inject expects InjectRequest",
            ));
        };
        let mut summary = BTreeMap::from([
            ("pump".into(), Value::I64(request.pump as i64)),
            ("action".into(), Value::String(request.action.name().into())),
        ]);
        if let Some(volume) = request.volume {
            summary.insert("volume".into(), Value::Volume(volume));
        }
        if let Some(speed) = request.speed {
            summary.insert("speed".into(), Value::FlowRate(speed));
        }
        Ok(Value::Map(summary))
    }

    fn invoke_gas_control(
        &mut self,
        device: DeviceId,
        request: CapabilityRequest,
    ) -> Result<Value> {
        match request {
            CapabilityRequest::GasControl(request) => {
                let mut changed = BTreeMap::new();
                if let Some(target) = request.co2_target {
                    let value = self.write_property(
                        device,
                        "co2_target",
                        &Value::GasConcentration(target),
                    )?;
                    self.emit_property(device, "co2_target", value.clone());
                    changed.insert("co2_target".into(), value);
                    changed.insert(
                        "co2_actual".into(),
                        optional(self.gas_actual.map(Value::GasConcentration)),
                    );
                }
                if let Some(target) = request.o2_target {
                    let value =
                        self.write_property(device, "o2_target", &Value::GasConcentration(target))?;
                    self.emit_property(device, "o2_target", value.clone());
                    changed.insert("o2_target".into(), value);
                    changed.insert(
                        "o2_actual".into(),
                        optional(self.o2_actual.map(Value::GasConcentration)),
                    );
                }
                if let Some(enabled) = request.enabled {
                    let value = self.write_property(device, "enabled", &Value::Bool(enabled))?;
                    self.emit_property(device, "enabled", value.clone());
                    changed.insert("enabled".into(), value);
                    changed.insert(
                        "co2_actual".into(),
                        optional(self.gas_actual.map(Value::GasConcentration)),
                    );
                    changed.insert(
                        "o2_actual".into(),
                        optional(self.o2_actual.map(Value::GasConcentration)),
                    );
                }
                if changed.is_empty() {
                    changed.insert(
                        "co2_target".into(),
                        Value::GasConcentration(self.gas_target),
                    );
                    changed.insert(
                        "co2_actual".into(),
                        optional(self.gas_actual.map(Value::GasConcentration)),
                    );
                    changed.insert("enabled".into(), Value::Bool(self.gas_enabled));
                    changed.insert("fault".into(), Value::Bool(self.gas_fault));
                }
                Ok(Value::Map(changed))
            }
            CapabilityRequest::None => Ok(Value::Map(BTreeMap::from([
                (
                    "co2_target".into(),
                    Value::GasConcentration(self.gas_target),
                ),
                (
                    "co2_actual".into(),
                    optional(self.gas_actual.map(Value::GasConcentration)),
                ),
                ("enabled".into(), Value::Bool(self.gas_enabled)),
                ("fault".into(), Value::Bool(self.gas_fault)),
            ]))),
            other => Err(Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "GasControl expects GasControlRequest, got {:?}",
                    other.request_kind()
                ),
            )),
        }
    }

    fn invoke_imaging_head(
        &mut self,
        device: DeviceId,
        request: CapabilityRequest,
    ) -> Result<Value> {
        match request {
            CapabilityRequest::ImagingHead(request) => {
                let mut changed = BTreeMap::new();
                if let Some(objective) = request.objective {
                    let value = self.write_property(device, "objective", &Value::I64(objective))?;
                    self.emit_property(device, "objective", value.clone());
                    changed.insert("objective".into(), value);
                }
                if let Some(mode) = request.mode {
                    let value = self.write_property(device, "mode", &Value::String(mode))?;
                    self.emit_property(device, "mode", value.clone());
                    changed.insert("mode".into(), value);
                }
                if changed.is_empty() {
                    changed.insert("objective".into(), Value::I64(self.fim_objective));
                    changed.insert("mode".into(), Value::String(self.fim_mode.clone()));
                    changed.insert(
                        "interlock_closed".into(),
                        Value::Bool(self.fim_interlock_closed),
                    );
                    changed.insert("fault".into(), Value::Bool(self.fim_fault));
                }
                Ok(Value::Map(changed))
            }
            CapabilityRequest::None => Ok(Value::Map(BTreeMap::from([
                ("objective".into(), Value::I64(self.fim_objective)),
                ("mode".into(), Value::String(self.fim_mode.clone())),
                (
                    "interlock_closed".into(),
                    Value::Bool(self.fim_interlock_closed),
                ),
                ("fault".into(), Value::Bool(self.fim_fault)),
            ]))),
            other => Err(Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "ImagingHead expects ImagingHeadRequest, got {:?}",
                    other.request_kind()
                ),
            )),
        }
    }

    fn invoke_plate_move(&mut self, device: DeviceId, request: CapabilityRequest) -> Result<Value> {
        match request {
            CapabilityRequest::PlateMove(request) => {
                let value = self.write_property(device, "well", &Value::String(request.well))?;
                self.emit_property(device, "well", value.clone());
                Ok(Value::Map(BTreeMap::from([
                    ("well".into(), value),
                    ("moved".into(), Value::Bool(true)),
                ])))
            }
            CapabilityRequest::None => Ok(Value::Map(BTreeMap::from([(
                "well".into(),
                Value::String(self.well.clone()),
            )]))),
            other => Err(Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "PlateMove expects PlateMoveRequest, got {:?}",
                    other.request_kind()
                ),
            )),
        }
    }

    fn invoke_measure(&mut self, device: DeviceId, request: CapabilityRequest) -> Result<Value> {
        match request {
            CapabilityRequest::Measure(request) => {
                let mut values = BTreeMap::from([
                    ("device".into(), Value::I64(device.0 .0 as i64)),
                    (
                        "integration_time".into(),
                        request
                            .integration_time
                            .map(Value::TimeInterval)
                            .unwrap_or(Value::Null),
                    ),
                ]);
                match device.0 .0 {
                    301 => {
                        values.insert(
                            "wavelength".into(),
                            Value::Wavelength(self.absorbance_wavelength),
                        );
                        values.insert("signal".into(), Value::F64(0.42));
                    }
                    302 => {
                        values.insert(
                            "wavelength".into(),
                            Value::Wavelength(self.fluorescence_wavelength),
                        );
                        values.insert("enabled".into(), Value::Bool(self.fluorescence_enabled));
                        values.insert("signal".into(), Value::F64(12.5));
                    }
                    303 => {
                        values.insert("enabled".into(), Value::Bool(self.luminescence_enabled));
                        values.insert("signal".into(), Value::F64(2.5));
                    }
                    _ => {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "Spark Cyto measure target is not a detector",
                        ));
                    }
                }
                Ok(Value::Map(values))
            }
            other => Err(Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "Measure expects MeasureRequest, got {:?}",
                    other.request_kind()
                ),
            )),
        }
    }

    fn invoke_temperature_control(
        &mut self,
        device: DeviceId,
        request: CapabilityRequest,
    ) -> Result<Value> {
        match request {
            CapabilityRequest::TemperatureControl(request) => {
                let mut changed = BTreeMap::new();
                if let Some(target) = request.target {
                    let value =
                        self.write_property(device, "target", &Value::Temperature(target))?;
                    self.emit_property(device, "target", value.clone());
                    changed.insert("target".into(), value);
                }
                if let Some(enabled) = request.enabled {
                    let value = self.write_property(device, "enabled", &Value::Bool(enabled))?;
                    self.emit_property(device, "enabled", value.clone());
                    changed.insert("enabled".into(), value);
                }
                if changed.is_empty() {
                    changed.insert("target".into(), Value::Temperature(self.temperature_target));
                    changed.insert("enabled".into(), Value::Bool(self.temperature_enabled));
                }
                Ok(Value::Map(changed))
            }
            other => Err(Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "TemperatureControl expects TemperatureControlRequest, got {:?}",
                    other.request_kind()
                ),
            )),
        }
    }

    fn invoke_camera_binding(
        &mut self,
        device: DeviceId,
        request: CapabilityRequest,
    ) -> Result<Value> {
        match request {
            CapabilityRequest::CameraBinding(request) => {
                let mut changed = BTreeMap::new();
                if let Some(bound) = request.bound {
                    let value = self.write_property(device, "bound", &Value::Bool(bound))?;
                    self.emit_property(device, "bound", value.clone());
                    changed.insert("bound".into(), value);
                }
                if let Some(mode) = request.imaging_mode {
                    let value =
                        self.write_property(device, "imaging_mode", &Value::String(mode))?;
                    self.emit_property(device, "imaging_mode", value.clone());
                    changed.insert("imaging_mode".into(), value);
                }
                if changed.is_empty() {
                    changed.insert("bound".into(), Value::Bool(self.camera_bound));
                    changed.insert(
                        "imaging_mode".into(),
                        Value::String(self.imaging_mode.clone()),
                    );
                }
                Ok(Value::Map(changed))
            }
            other => Err(Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "CameraBinding expects CameraBindingRequest, got {:?}",
                    other.request_kind()
                ),
            )),
        }
    }
}

impl Driver for SparkCytoDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        self.devices.clone()
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![
            ResourceDescriptor {
                id: ResourceId(NodeId(320)),
                driver: self.id,
                label: "spark-tdcl-command".into(),
                kind: "tdcl.command".into(),
                metadata: resource_metadata(self),
            },
            ResourceDescriptor {
                id: ResourceId(NodeId(321)),
                driver: self.id,
                label: "spark-tdcl-data".into(),
                kind: "tdcl.data".into(),
                metadata: resource_metadata(self),
            },
            ResourceDescriptor {
                id: ResourceId(NodeId(322)),
                driver: self.id,
                label: "spark-can-gateway".into(),
                kind: "can.gateway".into(),
                metadata: resource_metadata(self),
            },
        ]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        let Some(desc) = self.devices.iter().find(|d| d.id == device) else {
            return Vec::new();
        };
        let has = |kind: &str| desc.kinds.iter().any(|k| k == kind);
        let kinds = if has("plate.transport") {
            vec![CapabilityKind::PlateMove]
        } else if desc.kinds.iter().any(|k| k.starts_with("detector.")) {
            vec![CapabilityKind::Measure]
        } else if has("environment.temperature") {
            vec![CapabilityKind::TemperatureControl]
        } else if has("environment.gas") {
            vec![CapabilityKind::GasControl]
        } else if has("imaging.head") || has("objective.turret") {
            vec![CapabilityKind::ImagingHead]
        } else if has("camera.binding") {
            vec![CapabilityKind::CameraBinding]
        } else if has("axis.xy") || has("axis.z") {
            // Stop is absent deliberately: no command that halts a move in flight is
            // recorded, and advertising one that silently did nothing is worse than not
            // offering it.
            vec![CapabilityKind::StageMove, CapabilityKind::StageHome]
        } else if has("filter.wheel") || has("mirror.turret") {
            vec![CapabilityKind::FilterSelect]
        } else if has("injector") {
            vec![CapabilityKind::Inject]
        } else if has("barcode.reader") {
            vec![CapabilityKind::Barcode]
        } else if has("camera") {
            vec![CapabilityKind::CameraCapture]
        } else if has("shaker") {
            vec![CapabilityKind::Shake]
        } else if has("autofocus") {
            vec![CapabilityKind::Autofocus]
        } else {
            vec![CapabilityKind::GenericCommand]
        };
        kinds
            .into_iter()
            .enumerate()
            .map(|(index, kind)| {
                CapabilityDescriptor::new(
                    CapabilityId(device.0 .0 + index as u64 * 1000),
                    device,
                    kind,
                    ValueType::Map,
                )
            })
            .collect()
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut params: BTreeMap<String, Value> = BTreeMap::new();
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    let _ = self.read_property(*device, key)?;
                    params.insert(
                        "read".into(),
                        Value::String(format!("{}:{key}", device.0 .0)),
                    );
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    params.insert(format!("{}:{key}", device.0 .0), value.clone());
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                        params.insert(
                            format!("{}:{}", write.device.0 .0, write.property),
                            write.value.clone(),
                        );
                    }
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    self.validate_invoke(*device, *capability, request)?;
                    params.insert("invoke_device".into(), Value::I64(device.0 .0 as i64));
                    params.insert(
                        "invoke_request".into(),
                        capability_request_summary(request.clone()),
                    );
                }
                Command::Arm(plan) => self.validate_timing_plan(plan)?,
                Command::Start(_) | Command::Stop(_) => {}
            }
        }
        Ok(PreparedBatch {
            id: batch.id,
            commands: batch.commands.clone(),
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(ResourceId(NodeId(320))),
                description: "single TDCL command produced from logical batch".into(),
                payload: Value::Map(params),
            }],
        })
    }

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.next_token();
        let command_count = prepared.commands.len() as i64;
        let mut last = Value::Null;
        // With an instrument attached, a capability request goes to the wire and the token
        // completes from `poll` when the instrument says so. Without one, the modeled path
        // below answers immediately, exactly as it always has.
        let mut deferred = false;
        for command in prepared.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    last = self.read_property(device, &key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    // The model is updated either way, so a read stays consistent with what
                    // was asked for; with an instrument attached the completion comes from
                    // its reply rather than from here.
                    last = self.write_property(device, &key, &value)?;
                    self.emit_property(device, &key, last.clone());
                    if self.write_to_instrument(token, device, &key, &value)? {
                        deferred = true;
                    }
                }
                Command::ApplyStateSet(set) => {
                    let writes = set.writes.clone();
                    last = self.apply_state_set(set)?;
                    for write in writes {
                        if self.write_to_instrument(
                            token,
                            write.device,
                            &write.property,
                            &write.value,
                        )? {
                            deferred = true;
                        }
                    }
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    self.validate_request(device, &request)?;
                    if self.dispatch_to_instrument(token, device, &request)? {
                        // The instrument owns this one now. Local state is still updated so
                        // property reads stay consistent with what was asked for; the
                        // completion value comes from the reply, not from here.
                        let _ = self.invoke_capability(device, capability, request);
                        deferred = true;
                    } else {
                        last = self.invoke_capability(device, capability, request)?;
                    }
                }
                Command::Arm(_) | Command::Start(_) | Command::Stop(_) => unreachable!(),
            }
        }
        self.events
            .push_back(DriverEvent::Event(Event::Log(LogEvent {
                driver: Some(self.id),
                message: format!(
                    "Spark TDCL batch {} dispatched as {} physical transaction(s)",
                    prepared.id.0,
                    prepared.physical_transactions.len()
                ),
            })));
        if !deferred {
            self.events.push_back(DriverEvent::TokenCompleted {
                token,
                value: Value::Map(BTreeMap::from([
                    ("commands".into(), Value::I64(command_count)),
                    (
                        "physical_transactions".into(),
                        Value::I64(prepared.physical_transactions.len() as i64),
                    ),
                    ("result".into(), last),
                ])),
            });
        }
        Ok(token)
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        self.poll_instrument();
        self.events.drain(..).collect()
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
                resource: Some(ResourceId(NodeId(320))),
                description: "spark cyto timing arm".into(),
                payload: self.timing_summary(plan, "arm"),
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
                resource: Some(ResourceId(NodeId(320))),
                description: "spark cyto timing start".into(),
                payload: Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "start")),
                    ("applied".into(), applied),
                ])),
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
                resource: Some(ResourceId(NodeId(320))),
                description: "spark cyto timing stop".into(),
                payload: Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "stop")),
                    ("applied".into(), applied),
                ])),
            }],
        })
    }
}

fn descriptor(
    driver: DriverId,
    node: u64,
    label: &str,
    serial: Option<String>,
    kinds: &[&str],
    properties: Vec<PropertySchema>,
) -> DeviceDescriptor {
    DeviceDescriptor {
        id: DeviceId(NodeId(node)),
        driver,
        label: label.to_string(),
        vendor: Some("Tecan".to_string()),
        model: Some("Spark Cyto".to_string()),
        serial,
        kinds: kinds.iter().map(|kind| kind.to_string()).collect(),
        properties,
        metadata: BTreeMap::new(),
    }
}

/// Say how many positions a carrier has, on the `position` property's range and in device
/// metadata.
///
/// Both come from the one number so they cannot disagree: a client drawing a slot list reads
/// the range, and this driver's own pre-dispatch check reads the metadata. It is what the
/// machine was ordered with; a fitted carrier's own inventory reply refines it into the
/// `slots` property, which is the authority once an instrument has answered.
fn declare_positions(device: &mut DeviceDescriptor, positions: i64) {
    if let Some(position) = device
        .properties
        .iter_mut()
        .find(|property| property.key == "position")
    {
        position.range = Some(Range {
            min: Value::I64(1),
            max: Value::I64(positions),
        });
    }
    device
        .metadata
        .insert("positions".into(), Value::I64(positions));
}

fn resource_metadata(driver: &SparkCytoDriver) -> BTreeMap<String, Value> {
    let mut metadata = BTreeMap::from([
        ("label".into(), Value::String(driver.label.clone())),
        (
            "support_level".into(),
            Value::String(driver.support_level.clone()),
        ),
        ("hardware_validated".into(), Value::Bool(false)),
    ]);
    if let Some(serial) = &driver.serial_number {
        metadata.insert("serial_number".into(), Value::String(serial.clone()));
    }
    metadata
}

fn property(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    unit: Option<&str>,
    writable: bool,
) -> PropertySchema {
    PropertySchema {
        key: key.into(),
        display_name: display_name.into(),
        value_type,
        unit: unit.map(|unit| Unit(unit.into())),
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

fn sequenceable_property(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    unit: Option<&str>,
    writable: bool,
) -> PropertySchema {
    let mut schema = property(key, display_name, value_type, unit, writable);
    schema.sequenceable = true;
    schema
}

fn string_prop(device: &DeviceConfig, key: &str) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn optional_string_prop(device: &DeviceConfig, key: &str) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
        _ => None,
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

/// A USB id, written either as an integer or as a `0x`-prefixed string.
fn u16_prop(device: &DeviceConfig, key: &str) -> Option<u16> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => u16::try_from(*value).ok(),
        Some(Value::String(text)) => {
            let text = text.trim();
            match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
                Some(hex) => u16::from_str_radix(hex, 16).ok(),
                None => text.parse().ok(),
            }
        }
        _ => None,
    }
}

fn u32_prop(device: &DeviceConfig, key: &str) -> Option<u32> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => u32::try_from(*value).ok(),
        _ => None,
    }
}

fn f64_prop(device: &DeviceConfig, key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn wavelength_prop(device: &DeviceConfig, key: &str) -> Option<Wavelength> {
    match device.properties.get(key) {
        Some(Value::Wavelength(value)) => Some(*value),
        Some(Value::F64(value)) => Some(Wavelength::from_nanometers(*value)),
        Some(Value::I64(value)) => Some(Wavelength::from_nanometers(*value as f64)),
        _ => None,
    }
}

fn temperature_prop(device: &DeviceConfig, key: &str) -> Option<Temperature> {
    match device.properties.get(key) {
        Some(Value::Temperature(value)) => Some(*value),
        _ => f64_prop(device, key).map(Temperature::from_celsius),
    }
}

fn gas_prop(device: &DeviceConfig, key: &str) -> Option<GasConcentration> {
    match device.properties.get(key) {
        Some(Value::GasConcentration(value)) => Some(*value),
        _ => f64_prop(device, key).map(GasConcentration::from_percent),
    }
}

fn capability_request_summary(request: CapabilityRequest) -> Value {
    match request {
        CapabilityRequest::None => Value::Null,
        CapabilityRequest::PlateMove(request) => Value::Map(BTreeMap::from([(
            "well".into(),
            Value::String(request.well),
        )])),
        CapabilityRequest::TemperatureControl(request) => Value::Map(BTreeMap::from([
            (
                "target".into(),
                request
                    .target
                    .map(Value::Temperature)
                    .unwrap_or(Value::Null),
            ),
            (
                "enabled".into(),
                request.enabled.map(Value::Bool).unwrap_or(Value::Null),
            ),
        ])),
        CapabilityRequest::GasControl(request) => Value::Map(BTreeMap::from([
            (
                "co2_target".into(),
                request
                    .co2_target
                    .map(Value::GasConcentration)
                    .unwrap_or(Value::Null),
            ),
            (
                "enabled".into(),
                request.enabled.map(Value::Bool).unwrap_or(Value::Null),
            ),
        ])),
        CapabilityRequest::ImagingHead(request) => Value::Map(BTreeMap::from([
            (
                "objective".into(),
                request.objective.map(Value::I64).unwrap_or(Value::Null),
            ),
            (
                "mode".into(),
                request.mode.map(Value::String).unwrap_or(Value::Null),
            ),
        ])),
        CapabilityRequest::CameraBinding(request) => Value::Map(BTreeMap::from([
            (
                "bound".into(),
                request.bound.map(Value::Bool).unwrap_or(Value::Null),
            ),
            (
                "imaging_mode".into(),
                request
                    .imaging_mode
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            ),
        ])),
        CapabilityRequest::Measure(request) => Value::Map(BTreeMap::from([(
            "integration_time".into(),
            request
                .integration_time
                .map(Value::TimeInterval)
                .unwrap_or(Value::Null),
        )])),
        CapabilityRequest::Custom(value) => value,
        CapabilityRequest::GenericCommand(request) => Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("params".into(), Value::Map(request.params)),
        ])),
        other => Value::String(format!("{other:?}")),
    }
}

/// A value the instrument has not reported yet reads as `Null`, not as a default.
fn optional(value: Option<Value>) -> Value {
    value.unwrap_or(Value::Null)
}

fn unit_name(unit: backend::AxisUnit) -> &'static str {
    match unit {
        backend::AxisUnit::Micrometres => "um",
        backend::AxisUnit::Steps => "step",
    }
}
