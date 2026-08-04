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

use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use crate::spark::backend::{self, Detector, Intent};
use crate::spark::session::{BoxedTransport, Progress, SparkSession};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};

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
    gas_target: GasConcentration,
    gas_actual: GasConcentration,
    gas_enabled: bool,
    gas_fault: bool,
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
    well: String,
    absorbance_wavelength: Wavelength,
    fluorescence_wavelength: Wavelength,
    luminescence_enabled: bool,
    fluorescence_enabled: bool,
    temperature_target: Temperature,
    temperature_enabled: bool,
    gas_target: GasConcentration,
    gas_actual: GasConcentration,
    gas_enabled: bool,
    gas_fault: bool,
    fim_objective: i64,
    fim_mode: String,
    fim_interlock_closed: bool,
    fim_fault: bool,
    camera_bound: bool,
    imaging_mode: String,
}

impl SparkCytoConfiguredProbe {
    pub fn fixture() -> Self {
        Self {
            label: "Modeled Spark Cyto".into(),
            serial_number: None,
            well: "A01".into(),
            absorbance_wavelength: Wavelength::from_nanometers(600.0),
            fluorescence_wavelength: Wavelength::from_nanometers(520.0),
            luminescence_enabled: false,
            fluorescence_enabled: false,
            temperature_target: Temperature::from_celsius(25.0),
            temperature_enabled: false,
            gas_target: GasConcentration::from_percent(5.0),
            gas_actual: GasConcentration::from_percent(0.04),
            gas_enabled: false,
            gas_fault: false,
            fim_objective: 1,
            fim_mode: "brightfield".into(),
            fim_interlock_closed: true,
            fim_fault: false,
            camera_bound: false,
            imaging_mode: "brightfield".into(),
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
        let devices = vec![
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
        ];
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
            gas_target: configured.gas_target,
            gas_actual: configured.gas_actual,
            gas_enabled: configured.gas_enabled,
            gas_fault: configured.gas_fault,
            fim_objective: configured.fim_objective,
            fim_mode: configured.fim_mode,
            fim_interlock_closed: configured.fim_interlock_closed,
            fim_fault: configured.fim_fault,
            camera_bound: configured.camera_bound,
            imaging_mode: configured.imaging_mode,
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
        });
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
        } else if descriptor.kinds.iter().any(|k| k == "detector.fluorescence") {
            Some(Detector::Fluorescence)
        } else if descriptor.kinds.iter().any(|k| k == "detector.luminescence") {
            Some(Detector::Luminescence)
        } else {
            None
        }
    }

    /// Send a capability request to the instrument, if there is one attached and it has a
    /// command for it.
    ///
    /// Returns `true` when the request went to the wire, in which case the token completes
    /// later from [`Driver::poll`] rather than now.
    fn dispatch_to_instrument(
        &mut self,
        token: DriverToken,
        device: DeviceId,
        request: &CapabilityRequest,
    ) -> Result<bool> {
        let detector = self.detector_of(device);
        let wavelength_nm = detector.map(|detector| match detector {
            Detector::Fluorescence => self.fluorescence_wavelength.nanometers().round() as u32,
            _ => self.absorbance_wavelength.nanometers().round() as u32,
        });
        let well = self.well.clone();
        let Some(transactions) =
            backend::plan_request(request, detector, &well, wavelength_nm)
        else {
            return Ok(false);
        };
        if transactions.is_empty() {
            return Ok(false);
        }
        let Some(backend) = self.backend.as_mut() else {
            return Ok(false);
        };
        let last = transactions.len() - 1;
        for (index, transaction) in transactions.into_iter().enumerate() {
            backend
                .outstanding
                .push_back((token, transaction.intent, index == last));
            backend.session.submit(token, transaction.line)?;
        }
        Ok(true)
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

        for event in progress {
            match event {
                Progress::Completed(outcome) => {
                    let Some((token, intent, terminal)) = backend.outstanding.pop_front() else {
                        continue;
                    };
                    if terminal {
                        let value = backend::completion(&intent, &outcome);
                        self.events
                            .push_back(DriverEvent::TokenCompleted { token, value });
                    }
                    let _ = token;
                }
                Progress::Failed(failure) => {
                    // Everything queued behind a failed command belongs to operations that
                    // will now never run as asked, so they fail with it.
                    if let Some((token, _, _)) = backend.outstanding.pop_front() {
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
    }

    fn next_token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        match (device.0 .0, key) {
            (300, "well") => Ok(Value::String(self.well.clone())),
            (300, "support_level") => Ok(Value::String(self.support_level.clone())),
            (301, "wavelength") => Ok(Value::Wavelength(self.absorbance_wavelength)),
            (302, "wavelength") => Ok(Value::Wavelength(self.fluorescence_wavelength)),
            (302, "enabled") => Ok(Value::Bool(self.fluorescence_enabled)),
            (303, "enabled") => Ok(Value::Bool(self.luminescence_enabled)),
            (304, "target") => Ok(Value::Temperature(self.temperature_target)),
            (304, "enabled") => Ok(Value::Bool(self.temperature_enabled)),
            (305, "co2_target") => Ok(Value::GasConcentration(self.gas_target)),
            (305, "co2_actual") => Ok(Value::GasConcentration(self.gas_actual)),
            (305, "enabled") => Ok(Value::Bool(self.gas_enabled)),
            (305, "fault") => Ok(Value::Bool(self.gas_fault)),
            (306, "objective") => Ok(Value::I64(self.fim_objective)),
            (306, "mode") => Ok(Value::String(self.fim_mode.clone())),
            (306, "interlock_closed") => Ok(Value::Bool(self.fim_interlock_closed)),
            (306, "fault") => Ok(Value::Bool(self.fim_fault)),
            (307, "bound") => Ok(Value::Bool(self.camera_bound)),
            (307, "imaging_mode") => Ok(Value::String(self.imaging_mode.clone())),
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
                Ok(Value::Temperature(self.temperature_target))
            }
            (304, "enabled", Value::Bool(enabled)) => {
                self.temperature_enabled = *enabled;
                Ok(Value::Bool(self.temperature_enabled))
            }
            (305, "co2_target", Value::GasConcentration(target)) => {
                self.gas_target = *target;
                if self.gas_enabled && !self.gas_fault {
                    self.gas_actual = *target;
                }
                Ok(Value::GasConcentration(self.gas_target))
            }
            (305, "enabled", Value::Bool(enabled)) => {
                self.gas_enabled = *enabled;
                if self.gas_enabled && !self.gas_fault {
                    self.gas_actual = self.gas_target;
                }
                Ok(Value::Bool(self.gas_enabled))
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
            .filter(|sequence| {
                matches!(
                    sequence.device.0 .0,
                    300 | 301 | 302 | 303 | 304 | 305 | 306 | 307
                )
            })
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
                Value::GasConcentration(self.gas_actual),
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
            CapabilityKind::GenericCommand | CapabilityKind::Custom(_) => {
                Ok(capability_request_summary(request))
            }
            kind => Err(Error::new(
                ErrorCode::Unsupported,
                format!("Spark Cyto does not implement {}", kind.name()),
            )),
        }
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
                        Value::GasConcentration(self.gas_actual),
                    );
                }
                if let Some(enabled) = request.enabled {
                    let value = self.write_property(device, "enabled", &Value::Bool(enabled))?;
                    self.emit_property(device, "enabled", value.clone());
                    changed.insert("enabled".into(), value);
                    changed.insert(
                        "co2_actual".into(),
                        Value::GasConcentration(self.gas_actual),
                    );
                }
                if changed.is_empty() {
                    changed.insert(
                        "co2_target".into(),
                        Value::GasConcentration(self.gas_target),
                    );
                    changed.insert(
                        "co2_actual".into(),
                        Value::GasConcentration(self.gas_actual),
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
                    Value::GasConcentration(self.gas_actual),
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
        let kind = if desc.kinds.iter().any(|k| k == "plate.transport") {
            CapabilityKind::PlateMove
        } else if desc.kinds.iter().any(|k| k.starts_with("detector.")) {
            CapabilityKind::Measure
        } else if desc.kinds.iter().any(|k| k == "environment.temperature") {
            CapabilityKind::TemperatureControl
        } else if desc.kinds.iter().any(|k| k == "environment.gas") {
            CapabilityKind::GasControl
        } else if desc
            .kinds
            .iter()
            .any(|k| k == "imaging.head" || k == "objective.turret")
        {
            CapabilityKind::ImagingHead
        } else if desc.kinds.iter().any(|k| k == "camera.binding") {
            CapabilityKind::CameraBinding
        } else {
            CapabilityKind::GenericCommand
        };
        vec![CapabilityDescriptor::new(
            CapabilityId(device.0 .0),
            device,
            kind,
            ValueType::Map,
        )]
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
                    last = self.write_property(device, &key, &value)?;
                    self.emit_property(device, &key, last.clone());
                }
                Command::ApplyStateSet(set) => {
                    last = self.apply_state_set(set)?;
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
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
