use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::SerialIo;
#[cfg(feature = "os-serial")]
use numanager_core::serial::{LineEnding, SerialLineCodec};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
#[cfg(feature = "os-serial")]
use std::time::{Duration, Instant};

pub const BAUD_RATE: u32 = 115_200;
pub const TERMINATOR: &str = "\r\n";
pub const DATA_BITS: u8 = 8;
pub const STOP_BITS: u8 = 2;
pub const PARITY: &str = "even";
pub const ANSWER_TIMEOUT_MS: u64 = 4_000;

#[derive(Debug, Clone)]
pub struct Ix85ConfiguredProbe {
    label: String,
    serial_port: Option<String>,
    connect_real_transport: bool,
    model: String,
    serial_number: Option<String>,
    controller_version: String,
    unit_summary: String,
    focus_present: bool,
    focus_position: Position,
    nosepiece_present: bool,
    nosepiece_position: i64,
    light_path_present: bool,
    light_path_position: i64,
    mirror_unit_1_present: bool,
    mirror_unit_1_position: i64,
    dia_shutter_present: bool,
    dia_shutter_open: bool,
    epi_shutter_1_present: bool,
    epi_shutter_1_open: bool,
    autofocus_present: bool,
    autofocus_state: String,
}

pub struct Ix85Discovery {
    next_id: DriverId,
    probes: Vec<Ix85ConfiguredProbe>,
}

impl Ix85Discovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![Ix85ConfiguredProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| {
                matches!(
                    device.driver.as_str(),
                    "evident_ix85" | "evident-ix85" | "ix85" | "olympus_ix85"
                )
            })
            .map(Ix85ConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for Ix85Discovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .iter()
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let driver: Box<dyn Driver> = if probe.connect_real_transport {
                    Box::new(Ix85Driver::serial(id, probe.clone())?)
                } else {
                    Box::new(Ix85Driver::configured(id, probe.clone()))
                };
                Ok(DriverCandidate::from_driver(
                    probe.discovery_label(),
                    driver,
                ))
            })
            .collect()
    }
}

impl Ix85ConfiguredProbe {
    pub fn fixture() -> Self {
        Self {
            label: "Configured Evident IX85 microscope body".into(),
            serial_port: None,
            connect_real_transport: false,
            model: "IX85".into(),
            serial_number: Some("IX85-CONFIG-0001".into()),
            controller_version: "configured".into(),
            unit_summary: "configured IX85 body".into(),
            focus_present: true,
            focus_position: Position::from_micrometers(0.0),
            nosepiece_present: true,
            nosepiece_position: 1,
            light_path_present: true,
            light_path_position: 1,
            mirror_unit_1_present: true,
            mirror_unit_1_position: 1,
            dia_shutter_present: true,
            dia_shutter_open: false,
            epi_shutter_1_present: true,
            epi_shutter_1_open: false,
            autofocus_present: true,
            autofocus_state: "Unavailable".into(),
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::fixture();
        if !device.label.is_empty() {
            configured.label = device.label.clone();
        }
        configured.serial_port = string_prop(device, "serial_port")?;
        configured.connect_real_transport = bool_prop(device, "connect")?.unwrap_or(false);
        configured.model = string_prop(device, "model")?.unwrap_or(configured.model);
        configured.serial_number =
            optional_string_prop(device, "serial_number", configured.serial_number)?;
        configured.controller_version =
            string_prop(device, "controller_version")?.unwrap_or(configured.controller_version);
        configured.unit_summary =
            string_prop(device, "unit_summary")?.unwrap_or(configured.unit_summary);
        configured.focus_present = bool_prop(device, "focus_present")?.unwrap_or(true);
        configured.focus_position =
            position_prop(device, "focus_position")?.unwrap_or(configured.focus_position);
        configured.nosepiece_present = bool_prop(device, "nosepiece_present")?.unwrap_or(true);
        configured.nosepiece_position = i64_range_prop(device, "nosepiece_position", 1, 6)?
            .unwrap_or(configured.nosepiece_position);
        configured.light_path_present = bool_prop(device, "light_path_present")?.unwrap_or(true);
        configured.light_path_position = i64_range_prop(device, "light_path_position", 1, 4)?
            .unwrap_or(configured.light_path_position);
        configured.mirror_unit_1_present =
            bool_prop(device, "mirror_unit_1_present")?.unwrap_or(true);
        configured.mirror_unit_1_position = i64_range_prop(device, "mirror_unit_1_position", 1, 8)?
            .unwrap_or(configured.mirror_unit_1_position);
        configured.dia_shutter_present = bool_prop(device, "dia_shutter_present")?.unwrap_or(true);
        configured.dia_shutter_open =
            bool_prop(device, "dia_shutter_open")?.unwrap_or(configured.dia_shutter_open);
        configured.epi_shutter_1_present =
            bool_prop(device, "epi_shutter_1_present")?.unwrap_or(true);
        configured.epi_shutter_1_open =
            bool_prop(device, "epi_shutter_1_open")?.unwrap_or(configured.epi_shutter_1_open);
        configured.autofocus_present = bool_prop(device, "autofocus_present")?.unwrap_or(true);
        configured.autofocus_state =
            string_prop(device, "autofocus_state")?.unwrap_or(configured.autofocus_state);
        Ok(configured)
    }

    fn discovery_label(&self) -> String {
        match &self.serial_number {
            Some(serial) => format!("{} ({serial})", self.label),
            None => self.label.clone(),
        }
    }
}

pub struct Ix85Driver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    focus: DeviceId,
    nosepiece: DeviceId,
    light_path: DeviceId,
    mirror_unit_1: DeviceId,
    dia_shutter: DeviceId,
    epi_shutter_1: DeviceId,
    autofocus: DeviceId,
    configured: Ix85ConfiguredProbe,
    serial: Option<Box<dyn SerialIo>>,
    #[cfg(feature = "os-serial")]
    codec: SerialLineCodec,
    next_token: u64,
    events: VecDeque<DriverEvent>,
}

impl Ix85Driver {
    pub fn configured(id: DriverId, configured: Ix85ConfiguredProbe) -> Self {
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 990)),
            hub: DeviceId(NodeId(id.0 * 1000 + 991)),
            focus: DeviceId(NodeId(id.0 * 1000 + 992)),
            nosepiece: DeviceId(NodeId(id.0 * 1000 + 993)),
            light_path: DeviceId(NodeId(id.0 * 1000 + 994)),
            mirror_unit_1: DeviceId(NodeId(id.0 * 1000 + 995)),
            dia_shutter: DeviceId(NodeId(id.0 * 1000 + 996)),
            epi_shutter_1: DeviceId(NodeId(id.0 * 1000 + 997)),
            autofocus: DeviceId(NodeId(id.0 * 1000 + 998)),
            configured,
            serial: None,
            #[cfg(feature = "os-serial")]
            codec: SerialLineCodec::new(LineEnding::CrLf, LineEnding::CrLf),
            next_token: 1,
            events: VecDeque::new(),
        }
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: Ix85ConfiguredProbe) -> Result<Self> {
        let port_name = configured.serial_port.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "IX85 config requires serial_port when connect is true",
            )
        })?;
        let serial = Box::new(numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(port_name, BAUD_RATE)
                .timeout(Duration::from_millis(ANSWER_TIMEOUT_MS))
                .data_bits(serialport::DataBits::Eight)
                .flow_control(serialport::FlowControl::None)
                .parity(serialport::Parity::Even)
                .stop_bits(serialport::StopBits::Two),
        )?);
        let mut driver = Self::configured(id, configured);
        driver.serial = Some(serial);
        if let Ok(reply) = driver.query("V") {
            if !reply.trim().is_empty() {
                driver.configured.controller_version = reply.trim().into();
            }
        }
        if let Ok(reply) = driver.query("U") {
            if !reply.trim().is_empty() {
                driver.configured.unit_summary = reply.trim().into();
            }
        }
        let _ = driver.refresh_connected_readbacks();
        Ok(driver)
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _configured: Ix85ConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "IX85 real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn descriptors_inner(&self) -> Vec<DeviceDescriptor> {
        let mut devices = vec![self.hub_descriptor()];
        if self.configured.focus_present {
            devices.push(self.focus_descriptor());
        }
        if self.configured.nosepiece_present {
            devices.push(self.state_descriptor(
                self.nosepiece,
                "ix85-nosepiece",
                &["objective.turret", "state.device"],
                "nosepiece_position",
                "Nosepiece position",
                1,
                6,
                "OB",
            ));
        }
        if self.configured.light_path_present {
            devices.push(self.state_descriptor(
                self.light_path,
                "ix85-light-path",
                &["light.path", "state.device"],
                "light_path_position",
                "Light path position",
                1,
                4,
                "BIL",
            ));
        }
        if self.configured.mirror_unit_1_present {
            devices.push(self.state_descriptor(
                self.mirror_unit_1,
                "ix85-mirror-unit-1",
                &["filter.cube", "mirror.unit", "state.device"],
                "mirror_unit_1_position",
                "Mirror unit 1 position",
                1,
                8,
                "MU1",
            ));
        }
        if self.configured.dia_shutter_present {
            devices.push(self.shutter_descriptor(
                self.dia_shutter,
                "ix85-dia-shutter",
                "dia_shutter_open",
                "DSH",
            ));
        }
        if self.configured.epi_shutter_1_present {
            devices.push(self.shutter_descriptor(
                self.epi_shutter_1,
                "ix85-epi-shutter-1",
                "epi_shutter_1_open",
                "ESH1",
            ));
        }
        if self.configured.autofocus_present {
            devices.push(self.autofocus_descriptor());
        }
        devices
    }

    fn hub_descriptor(&self) -> DeviceDescriptor {
        DeviceDescriptor {
            id: self.hub,
            driver: self.id,
            label: "ix85-hub".into(),
            vendor: Some("Evident/Olympus".into()),
            model: Some(self.configured.model.clone()),
            serial: self.configured.serial_number.clone(),
            kinds: vec![
                "hub".into(),
                "microscope.body".into(),
                "serial.ascii".into(),
            ],
            properties: vec![
                string_property("model", "Model"),
                string_property("serial_number", "Serial number"),
                string_property("controller_version", "Controller version"),
                string_property("unit_summary", "Unit summary"),
                string_property("serial_settings", "Serial settings"),
                string_property("serial_port", "Serial port"),
                bool_property("connected", "Connected"),
                string_property("support_level", "Support level"),
                string_property("action_gate", "Action gate"),
                map_property("feature_summary", "Feature summary"),
                map_property("protocol_tags", "Protocol tags"),
            ],
            metadata: shared_metadata(),
        }
    }

    fn focus_descriptor(&self) -> DeviceDescriptor {
        DeviceDescriptor {
            id: self.focus,
            driver: self.id,
            label: "ix85-focus".into(),
            vendor: Some("Evident/Olympus".into()),
            model: Some(self.configured.model.clone()),
            serial: self.configured.serial_number.clone(),
            kinds: vec!["axis.z".into(), "stage.z".into(), "microscope.focus".into()],
            properties: vec![
                writable_property("position", "Position", ValueType::Position),
                property("minimum_position", "Minimum position", ValueType::Position),
                property("maximum_position", "Maximum position", ValueType::Position),
                string_property("wire_tag", "Wire tag"),
                string_property("support_level", "Support level"),
                string_property("action_gate", "Action gate"),
                map_property("command_summary", "Command summary"),
            ],
            metadata: BTreeMap::from([
                ("wire_tag".into(), Value::String("FP/FG/FM/FSTP".into())),
                (
                    "command_summary".into(),
                    command_summary(&[
                        ("read_position", "FP"),
                        ("move_absolute", "FG"),
                        ("move_relative", "FM"),
                        ("stop", "FSTP"),
                    ]),
                ),
                (
                    "support_level".into(),
                    Value::String("configured_or_opt_in_serial_control".into()),
                ),
            ]),
        }
    }

    fn state_descriptor(
        &self,
        id: DeviceId,
        label: &str,
        kinds: &[&str],
        position_key: &str,
        display_name: &str,
        min: i64,
        max: i64,
        wire_tag: &str,
    ) -> DeviceDescriptor {
        DeviceDescriptor {
            id,
            driver: self.id,
            label: label.into(),
            vendor: Some("Evident/Olympus".into()),
            model: Some(self.configured.model.clone()),
            serial: self.configured.serial_number.clone(),
            kinds: strings(kinds),
            properties: vec![
                writable_property(position_key, display_name, ValueType::I64),
                integer_metadata_property("minimum_position", "Minimum position", min),
                integer_metadata_property("maximum_position", "Maximum position", max),
                string_property("wire_tag", "Wire tag"),
                string_property("support_level", "Support level"),
                string_property("action_gate", "Action gate"),
                map_property("command_summary", "Command summary"),
            ],
            metadata: BTreeMap::from([
                ("wire_tag".into(), Value::String(wire_tag.into())),
                (
                    "command_summary".into(),
                    command_summary(&[("read_position", wire_tag), ("select_position", wire_tag)]),
                ),
                (
                    "support_level".into(),
                    Value::String("configured_or_opt_in_serial_control".into()),
                ),
            ]),
        }
    }

    fn shutter_descriptor(
        &self,
        id: DeviceId,
        label: &str,
        open_key: &str,
        wire_tag: &str,
    ) -> DeviceDescriptor {
        DeviceDescriptor {
            id,
            driver: self.id,
            label: label.into(),
            vendor: Some("Evident/Olympus".into()),
            model: Some(self.configured.model.clone()),
            serial: self.configured.serial_number.clone(),
            kinds: vec!["shutter".into(), "light.gate".into(), "state.device".into()],
            properties: vec![
                writable_property(open_key, "Open", ValueType::Bool),
                string_property("wire_tag", "Wire tag"),
                string_property("support_level", "Support level"),
                string_property("action_gate", "Action gate"),
                map_property("command_summary", "Command summary"),
            ],
            metadata: BTreeMap::from([
                ("wire_tag".into(), Value::String(wire_tag.into())),
                (
                    "command_summary".into(),
                    command_summary(&[("read_open", wire_tag), ("set_open", wire_tag)]),
                ),
                (
                    "support_level".into(),
                    Value::String("configured_or_opt_in_serial_control".into()),
                ),
            ]),
        }
    }

    fn autofocus_descriptor(&self) -> DeviceDescriptor {
        DeviceDescriptor {
            id: self.autofocus,
            driver: self.id,
            label: "ix85-zdc-autofocus".into(),
            vendor: Some("Evident/Olympus".into()),
            model: Some(self.configured.model.clone()),
            serial: self.configured.serial_number.clone(),
            kinds: vec!["autofocus".into(), "zdc".into(), "state.device".into()],
            properties: vec![
                string_property("state", "State"),
                string_property("wire_tag", "Wire tag"),
                string_property("support_level", "Support level"),
                string_property("action_gate", "Action gate"),
                map_property("command_summary", "Command summary"),
            ],
            metadata: BTreeMap::from([
                ("wire_tag".into(), Value::String("AF/AFST".into())),
                (
                    "command_summary".into(),
                    command_summary(&[("autofocus", "AF"), ("status", "AFST")]),
                ),
                (
                    "support_level".into(),
                    Value::String("configured_read_only".into()),
                ),
            ]),
        }
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        match (device, key) {
            (device, "model") if device == self.hub => {
                Ok(Value::String(self.configured.model.clone()))
            }
            (device, "serial_number") if device == self.hub => {
                Ok(optional_string_value(&self.configured.serial_number))
            }
            (device, "controller_version") if device == self.hub => {
                Ok(Value::String(self.configured.controller_version.clone()))
            }
            (device, "unit_summary") if device == self.hub => {
                Ok(Value::String(self.configured.unit_summary.clone()))
            }
            (device, "serial_settings") if device == self.hub => {
                Ok(Value::String("115200 8E2 no-flow CRLF".into()))
            }
            (device, "serial_port") if device == self.hub => Ok(Value::String(
                self.configured.serial_port.clone().unwrap_or_default(),
            )),
            (device, "connected") if device == self.hub => Ok(Value::Bool(self.serial.is_some())),
            (device, "support_level") if device == self.hub => {
                Ok(Value::String(
                    "configured_or_opt_in_serial_control".into(),
                ))
            }
            (device, "action_gate") if device == self.hub => Ok(Value::String(
                "typed focus, state-device, and shutter commands use documented serial tags; ZDC action commands are not exposed because AF parameter semantics are absent".into(),
            )),
            (device, "feature_summary") if device == self.hub => Ok(Value::Map(BTreeMap::from([
                ("serial_shape_known".into(), Value::Bool(true)),
                ("inventory_shape_known".into(), Value::Bool(true)),
                ("active_readback_connected".into(), Value::Bool(self.serial.is_some())),
                ("active_serial_validated".into(), Value::Bool(false)),
                ("writes_supported".into(), Value::Bool(true)),
                ("motion_supported".into(), Value::Bool(true)),
                ("shutters_supported".into(), Value::Bool(true)),
                ("autofocus_supported".into(), Value::Bool(false)),
            ]))),
            (device, "protocol_tags") if device == self.hub => Ok(protocol_tags()),
            (device, "position") if device == self.focus => {
                Ok(Value::Position(self.configured.focus_position))
            }
            (device, "minimum_position") if device == self.focus => {
                Ok(Value::Position(Position::from_micrometers(0.0)))
            }
            (device, "maximum_position") if device == self.focus => {
                Ok(Value::Position(Position::from_micrometers(10_500.0)))
            }
            (device, "nosepiece_position") if device == self.nosepiece => {
                Ok(Value::I64(self.configured.nosepiece_position))
            }
            (device, "light_path_position") if device == self.light_path => {
                Ok(Value::I64(self.configured.light_path_position))
            }
            (device, "mirror_unit_1_position") if device == self.mirror_unit_1 => {
                Ok(Value::I64(self.configured.mirror_unit_1_position))
            }
            (device, "dia_shutter_open") if device == self.dia_shutter => {
                Ok(Value::Bool(self.configured.dia_shutter_open))
            }
            (device, "epi_shutter_1_open") if device == self.epi_shutter_1 => {
                Ok(Value::Bool(self.configured.epi_shutter_1_open))
            }
            (device, "state") if device == self.autofocus => {
                Ok(Value::String(self.configured.autofocus_state.clone()))
            }
            (_, "wire_tag") => self
                .descriptors_inner()
                .into_iter()
                .find(|descriptor| descriptor.id == device)
                .and_then(|descriptor| descriptor.metadata.get("wire_tag").cloned())
                .ok_or_else(|| Error::new(ErrorCode::InvalidProperty, "unknown IX85 wire tag")),
            (_, "command_summary") => self
                .descriptors_inner()
                .into_iter()
                .find(|descriptor| descriptor.id == device)
                .and_then(|descriptor| descriptor.metadata.get("command_summary").cloned())
                .ok_or_else(|| {
                    Error::new(ErrorCode::InvalidProperty, "unknown IX85 command summary")
                }),
            (device, "action_gate") if device == self.focus => Ok(Value::String(
                "focus writes use FG/FM/FSTP plus FP readback; hardware busy/completion and recovery semantics need validation".into(),
            )),
            (device, "action_gate")
                if device == self.nosepiece
                    || device == self.light_path
                    || device == self.mirror_unit_1 =>
            {
                Ok(Value::String(
                    "state selection uses the mapped state tag plus readback; notification and position-count behavior needs validation".into(),
                ))
            }
            (device, "action_gate")
                if device == self.dia_shutter || device == self.epi_shutter_1 =>
            {
                Ok(Value::String(
                    "shutter writes use the mapped shutter tag plus readback; cover/interlock behavior needs validation".into(),
                ))
            }
            (device, "action_gate") if device == self.autofocus => Ok(Value::String(
                "ZDC autofocus actions are not exposed because state, limit, and failure semantics are absent"
                    .into(),
            )),
            (device, "support_level") if device == self.autofocus => {
                Ok(Value::String("configured_read_only".into()))
            }
            (_, "support_level") => Ok(Value::String(
                "configured_or_opt_in_serial_control".into(),
            )),
            (device, "minimum_position")
                if device == self.nosepiece
                    || device == self.light_path
                    || device == self.mirror_unit_1 =>
            {
                Ok(Value::I64(1))
            }
            (device, "maximum_position") if device == self.nosepiece => Ok(Value::I64(6)),
            (device, "maximum_position") if device == self.light_path => Ok(Value::I64(4)),
            (device, "maximum_position") if device == self.mirror_unit_1 => Ok(Value::I64(8)),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown IX85 property {key}"),
            )),
        }
    }

    fn validate_write_property(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        match (device, key, value) {
            (device, "position", Value::Position(position)) if device == self.focus => {
                validate_focus_position(*position)
            }
            (device, "nosepiece_position", Value::I64(value)) if device == self.nosepiece => {
                validate_ix85_range("nosepiece_position", *value, 1, 6)
            }
            (device, "light_path_position", Value::I64(value)) if device == self.light_path => {
                validate_ix85_range("light_path_position", *value, 1, 4)
            }
            (device, "mirror_unit_1_position", Value::I64(value))
                if device == self.mirror_unit_1 =>
            {
                validate_ix85_range("mirror_unit_1_position", *value, 1, 8)
            }
            (device, "dia_shutter_open", Value::Bool(_)) if device == self.dia_shutter => Ok(()),
            (device, "epi_shutter_1_open", Value::Bool(_)) if device == self.epi_shutter_1 => {
                Ok(())
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unsupported IX85 writable property {key}"),
            )),
        }
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: Value) -> Result<Value> {
        self.validate_write_property(device, key, &value)?;
        let command = self.write_command(device, key, &value)?;
        self.execute_control_command(&command)?;
        self.apply_cached_write(device, key, &value);
        self.refresh_readback_property(device, key)?;
        self.read_property(device, key)
    }

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| {
                sequence.device == self.focus
                    || sequence.device == self.nosepiece
                    || sequence.device == self.light_path
                    || sequence.device == self.mirror_unit_1
                    || sequence.device == self.dia_shutter
                    || sequence.device == self.epi_shutter_1
            })
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequences(plan) {
            if sequence.values.is_empty() {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "IX85 timing sequence must contain at least one value",
                ));
            }
            for value in &sequence.values {
                self.validate_write_property(sequence.device, &sequence.property, value)?;
            }
        }
        Ok(())
    }

    fn timing_summary(&self, plan: &TimingPlan, phase: &str, applied: Value) -> Value {
        Value::Map(BTreeMap::from([
            ("phase".into(), Value::String(phase.into())),
            (
                "participants".into(),
                Value::I64(plan.participants.len() as i64),
            ),
            (
                "local_sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
            ("applied".into(), applied),
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
                Some((sequence.device, sequence.property.clone(), value.clone()))
            })
            .collect::<Vec<_>>();

        let mut changed = BTreeMap::new();
        for (device, property, value) in writes {
            let applied = self.write_property(device, &property, value)?;
            changed.insert(format!("{}:{}", device.0 .0, property), applied);
        }
        Ok(Value::Map(changed))
    }

    fn write_command(&self, device: DeviceId, key: &str, value: &Value) -> Result<Ix85Command> {
        match (device, key, value) {
            (device, "position", Value::Position(position)) if device == self.focus => {
                Ok(Ix85Command::ack(
                    "FG",
                    Some(focus_steps_from_position(*position)?.to_string()),
                ))
            }
            (device, "nosepiece_position", Value::I64(value)) if device == self.nosepiece => {
                Ok(Ix85Command::ack("OB", Some(value.to_string())))
            }
            (device, "light_path_position", Value::I64(value)) if device == self.light_path => {
                Ok(Ix85Command::ack("BIL", Some(value.to_string())))
            }
            (device, "mirror_unit_1_position", Value::I64(value))
                if device == self.mirror_unit_1 =>
            {
                Ok(Ix85Command::ack("MU1", Some(value.to_string())))
            }
            (device, "dia_shutter_open", Value::Bool(open)) if device == self.dia_shutter => Ok(
                Ix85Command::ack("DSH", Some(if *open { "1" } else { "0" }.into())),
            ),
            (device, "epi_shutter_1_open", Value::Bool(open)) if device == self.epi_shutter_1 => {
                Ok(Ix85Command::ack(
                    "ESH1",
                    Some(if *open { "1" } else { "0" }.into()),
                ))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unsupported IX85 writable property {key}"),
            )),
        }
    }

    fn apply_cached_write(&mut self, device: DeviceId, key: &str, value: &Value) {
        match (device, key, value) {
            (device, "position", Value::Position(position)) if device == self.focus => {
                self.configured.focus_position = *position;
            }
            (device, "nosepiece_position", Value::I64(value)) if device == self.nosepiece => {
                self.configured.nosepiece_position = *value;
            }
            (device, "light_path_position", Value::I64(value)) if device == self.light_path => {
                self.configured.light_path_position = *value;
            }
            (device, "mirror_unit_1_position", Value::I64(value))
                if device == self.mirror_unit_1 =>
            {
                self.configured.mirror_unit_1_position = *value;
            }
            (device, "dia_shutter_open", Value::Bool(open)) if device == self.dia_shutter => {
                self.configured.dia_shutter_open = *open;
            }
            (device, "epi_shutter_1_open", Value::Bool(open)) if device == self.epi_shutter_1 => {
                self.configured.epi_shutter_1_open = *open;
            }
            _ => {}
        }
    }

    fn validate_stage_move(&self, request: &StageMoveRequest) -> Result<()> {
        if request.target.len() != 1 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "IX85 focus StageMove expects exactly one Z target",
            ));
        }
        if request
            .profile
            .as_ref()
            .and_then(|profile| profile.acceleration.as_ref())
            .is_some()
        {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "IX85 focus acceleration profile is not mapped to a serial command",
            ));
        }
        let Some((axis, position)) = request.target.iter().next() else {
            unreachable!("checked non-empty IX85 focus target");
        };
        let axis_matches_focus = match axis {
            StageAxis::Z => true,
            StageAxis::Custom(name) => name == "focus",
            _ => false,
        };
        if !axis_matches_focus {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "IX85 focus StageMove target must use the Z or focus axis",
            ));
        }
        if request.relative {
            validate_relative_focus_delta(*position)
        } else {
            validate_focus_position(*position)
        }
    }

    fn stage_move(&mut self, request: StageMoveRequest) -> Result<Value> {
        self.validate_stage_move(&request)?;
        let (_, position) = request
            .target
            .iter()
            .next()
            .expect("validated IX85 focus target");
        let command = if request.relative {
            Ix85Command::ack("FM", Some(focus_steps_from_delta(*position)?.to_string()))
        } else {
            Ix85Command::ack(
                "FG",
                Some(focus_steps_from_position(*position)?.to_string()),
            )
        };
        self.execute_control_command(&command)?;
        if request.relative {
            let current = self.configured.focus_position.micrometers();
            self.configured.focus_position =
                Position::from_micrometers(current + position.micrometers());
        } else {
            self.configured.focus_position = *position;
        }
        self.refresh_readback_property(self.focus, "position")?;
        self.read_property(self.focus, "position")
    }

    fn stop_focus(&mut self) -> Result<Value> {
        self.execute_control_command(&Ix85Command::ack("FSTP", None))?;
        self.refresh_readback_property(self.focus, "position")?;
        Ok(Value::Map(BTreeMap::from([
            ("stopped".into(), Value::Bool(true)),
            (
                "position".into(),
                Value::Position(self.configured.focus_position),
            ),
        ])))
    }

    fn filter_select(&mut self, device: DeviceId, request: FilterSelectRequest) -> Result<Value> {
        let value = Value::I64(request.position as i64);
        let key = if device == self.nosepiece {
            "nosepiece_position"
        } else if device == self.light_path {
            "light_path_position"
        } else if device == self.mirror_unit_1 {
            "mirror_unit_1_position"
        } else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "IX85 FilterSelect targets a known state device",
            ));
        };
        self.write_property(device, key, value)
    }

    fn trigger_shutter(&mut self, device: DeviceId, request: CapabilityRequest) -> Result<Value> {
        let action = match request {
            CapabilityRequest::None => TriggerAction::Pulse,
            CapabilityRequest::Trigger(request) => request.action,
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "IX85 shutter TriggerSink expects None or Trigger",
                ))
            }
        };
        let key = if device == self.dia_shutter {
            "dia_shutter_open"
        } else if device == self.epi_shutter_1 {
            "epi_shutter_1_open"
        } else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "IX85 TriggerSink targets a known shutter",
            ));
        };
        match action {
            TriggerAction::Enable => self.write_property(device, key, Value::Bool(true)),
            TriggerAction::Disable => self.write_property(device, key, Value::Bool(false)),
            TriggerAction::Pulse => {
                self.write_property(device, key, Value::Bool(true))?;
                self.write_property(device, key, Value::Bool(false))
            }
        }
    }

    fn validate_invoke(
        &self,
        device: DeviceId,
        capability: CapabilityId,
        request: &CapabilityRequest,
    ) -> Result<()> {
        match (device, capability, request) {
            (device, CapabilityId(1), CapabilityRequest::GenericCommand(_))
                if device == self.hub =>
            {
                Ok(())
            }
            (device, CapabilityId(2), CapabilityRequest::StageMove(_)) if device == self.focus => {
                Ok(())
            }
            (device, CapabilityId(3), CapabilityRequest::None) if device == self.focus => Ok(()),
            (device, CapabilityId(4), CapabilityRequest::FilterSelect(_))
                if device == self.nosepiece
                    || device == self.light_path
                    || device == self.mirror_unit_1 =>
            {
                Ok(())
            }
            (device, CapabilityId(5), CapabilityRequest::Trigger(_))
            | (device, CapabilityId(5), CapabilityRequest::None)
                if device == self.dia_shutter || device == self.epi_shutter_1 =>
            {
                Ok(())
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported IX85 capability request",
            )),
        }
    }

    fn execute_control_command(&mut self, command: &Ix85Command) -> Result<()> {
        self.execute_control_command_impl(command)
    }

    #[cfg(feature = "os-serial")]
    fn active_serial(&mut self) -> Result<&mut (dyn SerialIo + 'static)> {
        self.serial.as_deref_mut().ok_or_else(|| {
            Error::new(
                ErrorCode::Unsupported,
                "IX85 active serial is not connected",
            )
        })
    }

    #[cfg(feature = "os-serial")]
    fn query(&mut self, command: &str) -> Result<String> {
        let bytes = self.codec.encode(command);
        self.active_serial()?.write(&bytes)?;
        let deadline = Instant::now() + Duration::from_millis(ANSWER_TIMEOUT_MS);
        loop {
            let bytes = self.active_serial()?.read_available()?;
            let lines = self.codec.push(&bytes);
            if let Some(line) = lines.into_iter().find(|line| !line.trim().is_empty()) {
                return Ok(line.trim().into());
            }
            if Instant::now() >= deadline {
                return Ok(String::new());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[cfg(feature = "os-serial")]
    fn execute_control_command_impl(&mut self, command: &Ix85Command) -> Result<()> {
        if self.serial.is_none() {
            return Ok(());
        }
        let reply = self.query(&command.command)?;
        if reply.trim().is_empty() {
            return Ok(());
        }
        validate_ix85_ack(command.tag, &reply)
    }

    #[cfg(not(feature = "os-serial"))]
    fn execute_control_command_impl(&mut self, _command: &Ix85Command) -> Result<()> {
        Ok(())
    }

    #[cfg(feature = "os-serial")]
    fn refresh_connected_readbacks(&mut self) -> Result<()> {
        if self.serial.is_none() {
            return Ok(());
        }
        if self.configured.focus_present {
            self.refresh_readback_property(self.focus, "position")?;
        }
        if self.configured.nosepiece_present {
            self.refresh_readback_property(self.nosepiece, "nosepiece_position")?;
        }
        if self.configured.light_path_present {
            self.refresh_readback_property(self.light_path, "light_path_position")?;
        }
        if self.configured.mirror_unit_1_present {
            self.refresh_readback_property(self.mirror_unit_1, "mirror_unit_1_position")?;
        }
        if self.configured.dia_shutter_present {
            self.refresh_readback_property(self.dia_shutter, "dia_shutter_open")?;
        }
        if self.configured.epi_shutter_1_present {
            self.refresh_readback_property(self.epi_shutter_1, "epi_shutter_1_open")?;
        }
        if self.configured.autofocus_present {
            self.refresh_readback_property(self.autofocus, "state")?;
        }
        Ok(())
    }

    fn refresh_readback_property(&mut self, device: DeviceId, key: &str) -> Result<()> {
        self.refresh_readback_property_impl(device, key)
    }

    fn refresh_identity_readbacks(&mut self) -> Result<Vec<&'static str>> {
        let mut refreshed = Vec::new();
        self.refresh_readback_property(self.hub, "controller_version")?;
        refreshed.push("controller_version");
        self.refresh_readback_property(self.hub, "unit_summary")?;
        refreshed.push("unit_summary");
        Ok(refreshed)
    }

    fn refresh_status_readbacks(&mut self) -> Result<Vec<&'static str>> {
        let mut refreshed = Vec::new();
        if self.configured.focus_present {
            self.refresh_readback_property(self.focus, "position")?;
            refreshed.push("position");
        }
        if self.configured.nosepiece_present {
            self.refresh_readback_property(self.nosepiece, "nosepiece_position")?;
            refreshed.push("nosepiece_position");
        }
        if self.configured.light_path_present {
            self.refresh_readback_property(self.light_path, "light_path_position")?;
            refreshed.push("light_path_position");
        }
        if self.configured.mirror_unit_1_present {
            self.refresh_readback_property(self.mirror_unit_1, "mirror_unit_1_position")?;
            refreshed.push("mirror_unit_1_position");
        }
        if self.configured.dia_shutter_present {
            self.refresh_readback_property(self.dia_shutter, "dia_shutter_open")?;
            refreshed.push("dia_shutter_open");
        }
        if self.configured.epi_shutter_1_present {
            self.refresh_readback_property(self.epi_shutter_1, "epi_shutter_1_open")?;
            refreshed.push("epi_shutter_1_open");
        }
        if self.configured.autofocus_present {
            self.refresh_readback_property(self.autofocus, "state")?;
            refreshed.push("state");
        }
        Ok(refreshed)
    }

    fn refresh_all_readbacks(&mut self) -> Result<Vec<&'static str>> {
        let mut refreshed = self.refresh_identity_readbacks()?;
        refreshed.extend(self.refresh_status_readbacks()?);
        Ok(refreshed)
    }

    fn invoke_generic(&mut self, request: GenericCommandRequest) -> Result<Value> {
        if request.is_hidden_maintenance() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "GenericCommand {} is a hidden maintenance operation",
                    request.command
                ),
            ));
        }
        if !request.params.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "IX85 GenericCommand refresh commands do not accept params",
            ));
        }
        let refreshed = match request.command.as_str() {
            "refresh_readbacks" => self.refresh_all_readbacks()?,
            "refresh_identity" => self.refresh_identity_readbacks()?,
            "refresh_status" => self.refresh_status_readbacks()?,
            other => {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    format!("unsupported IX85 GenericCommand {other}"),
                ))
            }
        };
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("connected".into(), Value::Bool(self.serial.is_some())),
            (
                "refreshed".into(),
                Value::List(
                    refreshed
                        .into_iter()
                        .map(|key| Value::String(key.into()))
                        .collect(),
                ),
            ),
            (
                "controller_version".into(),
                Value::String(self.configured.controller_version.clone()),
            ),
            (
                "unit_summary".into(),
                Value::String(self.configured.unit_summary.clone()),
            ),
        ])))
    }

    #[cfg(feature = "os-serial")]
    fn refresh_readback_property_impl(&mut self, device: DeviceId, key: &str) -> Result<()> {
        if self.serial.is_none() {
            return Ok(());
        }
        let Some(tag) = self.readback_tag(device, key) else {
            return Ok(());
        };
        let reply = self.query(tag)?;
        if reply.trim().is_empty() {
            return Ok(());
        }
        self.apply_readback(device, key, &reply)
    }

    #[cfg(not(feature = "os-serial"))]
    fn refresh_readback_property_impl(&mut self, _device: DeviceId, _key: &str) -> Result<()> {
        Ok(())
    }

    fn readback_tag(&self, device: DeviceId, key: &str) -> Option<&'static str> {
        match (device, key) {
            (device, "controller_version") if device == self.hub => Some("V"),
            (device, "unit_summary") if device == self.hub => Some("U"),
            (device, "position") if device == self.focus => Some("FP"),
            (device, "nosepiece_position") if device == self.nosepiece => Some("OB"),
            (device, "light_path_position") if device == self.light_path => Some("BIL"),
            (device, "mirror_unit_1_position") if device == self.mirror_unit_1 => Some("MU1"),
            (device, "dia_shutter_open") if device == self.dia_shutter => Some("DSH"),
            (device, "epi_shutter_1_open") if device == self.epi_shutter_1 => Some("ESH1"),
            (device, "state") if device == self.autofocus => Some("AFST"),
            _ => None,
        }
    }

    #[cfg(feature = "os-serial")]
    fn apply_readback(&mut self, device: DeviceId, key: &str, reply: &str) -> Result<()> {
        match (device, key) {
            (device, "controller_version") if device == self.hub => {
                self.configured.controller_version = clean_reply_text(reply).into();
            }
            (device, "unit_summary") if device == self.hub => {
                self.configured.unit_summary = clean_reply_text(reply).into();
            }
            (device, "position") if device == self.focus => {
                let raw = parse_ix85_i64_reply(reply, "IX85 focus position")?;
                if !(0..=1_050_000).contains(&raw) {
                    return Err(Error::new(
                        ErrorCode::Transport,
                        format!("IX85 focus position reply out of range: {raw}"),
                    ));
                }
                self.configured.focus_position = Position::from_micrometers(raw as f64 * 0.01);
            }
            (device, "nosepiece_position") if device == self.nosepiece => {
                self.configured.nosepiece_position =
                    parse_ix85_slot_reply(reply, "IX85 nosepiece position", 1, 6)?;
            }
            (device, "light_path_position") if device == self.light_path => {
                self.configured.light_path_position =
                    parse_ix85_slot_reply(reply, "IX85 light path position", 1, 4)?;
            }
            (device, "mirror_unit_1_position") if device == self.mirror_unit_1 => {
                self.configured.mirror_unit_1_position =
                    parse_ix85_slot_reply(reply, "IX85 mirror unit 1 position", 1, 8)?;
            }
            (device, "dia_shutter_open") if device == self.dia_shutter => {
                self.configured.dia_shutter_open =
                    parse_ix85_i64_reply(reply, "IX85 DIA shutter")? != 0;
            }
            (device, "epi_shutter_1_open") if device == self.epi_shutter_1 => {
                self.configured.epi_shutter_1_open =
                    parse_ix85_i64_reply(reply, "IX85 EPI shutter 1")? != 0;
            }
            (device, "state") if device == self.autofocus => {
                self.configured.autofocus_state = clean_reply_text(reply).into();
            }
            _ => {}
        }
        Ok(())
    }
}

impl Driver for Ix85Driver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "ix85-serial".into(),
            kind: "serial.ascii".into(),
            metadata: BTreeMap::from([
                ("baud_rate".into(), Value::I64(BAUD_RATE as i64)),
                (
                    "serial_port".into(),
                    self.configured
                        .serial_port
                        .as_ref()
                        .map(|port| Value::String(port.clone()))
                        .unwrap_or(Value::Null),
                ),
                (
                    "serial_timeout".into(),
                    Value::TimeInterval(TimeInterval::from_milliseconds(ANSWER_TIMEOUT_MS as f64)),
                ),
                ("connected".into(), Value::Bool(self.serial.is_some())),
                ("data_bits".into(), Value::I64(DATA_BITS as i64)),
                ("parity".into(), Value::String(PARITY.into())),
                ("stop_bits".into(), Value::I64(STOP_BITS as i64)),
                ("terminator".into(), Value::String("CRLF".into())),
                (
                    "answer_timeout".into(),
                    Value::TimeInterval(TimeInterval::from_milliseconds(ANSWER_TIMEOUT_MS as f64)),
                ),
                (
                    "support_level".into(),
                    Value::String("configured_or_opt_in_serial_control".into()),
                ),
                (
                    "evidence_class".into(),
                    Value::String("reverse engineered".into()),
                ),
                (
                    "action_gate".into(),
                    Value::String("typed IX85 control uses mapped serial tags; ZDC action is not exposed because AF parameter semantics are absent".into()),
                ),
            ]),
        }]
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        self.descriptors_inner()
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.hub {
            vec![capability(1, device, CapabilityKind::GenericCommand)]
        } else if device == self.focus {
            vec![
                capability(2, device, CapabilityKind::StageMove),
                capability(3, device, CapabilityKind::StageStop),
            ]
        } else if device == self.nosepiece
            || device == self.light_path
            || device == self.mirror_unit_1
        {
            vec![capability(4, device, CapabilityKind::FilterSelect)]
        } else if device == self.dia_shutter || device == self.epi_shutter_1 {
            vec![capability(5, device, CapabilityKind::TriggerSink)]
        } else {
            Vec::new()
        }
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut physical_transactions = Vec::new();
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    self.read_property(*device, key)?;
                    if self.readback_tag(*device, key).is_some() {
                        physical_transactions.push(PhysicalTransaction {
                            resource: Some(self.resource),
                            description: format!("ix85 serial readback {key}"),
                            payload: Value::String(key.clone()),
                        });
                    }
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write_property(*device, key, value)?;
                    let command = self.write_command(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("ix85 serial write {key}"),
                        payload: Value::String(command.command),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write_property(write.device, &write.property, &write.value)?;
                        let command =
                            self.write_command(write.device, &write.property, &write.value)?;
                        physical_transactions.push(PhysicalTransaction {
                            resource: Some(self.resource),
                            description: format!("ix85 serial write {}", write.property),
                            payload: Value::String(command.command),
                        });
                    }
                }
                Command::Invoke {
                    device,
                    request: CapabilityRequest::GenericCommand(request),
                    ..
                } if *device == self.hub => {
                    if request.is_hidden_maintenance() {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "GenericCommand {} is a hidden maintenance operation",
                                request.command
                            ),
                        ));
                    }
                    if !matches!(
                        request.command.as_str(),
                        "refresh_readbacks" | "refresh_identity" | "refresh_status"
                    ) {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            format!("unsupported IX85 GenericCommand {}", request.command),
                        ));
                    }
                    if !request.params.is_empty() {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "IX85 GenericCommand refresh commands do not accept params",
                        ));
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("ix85 documented {}", request.command),
                        payload: Value::String(request.command.clone()),
                    });
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    self.validate_invoke(*device, *capability, request)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("ix85 {}", capability.0),
                        payload: Value::String(capability.0.to_string()),
                    });
                }
                Command::Arm(plan) => self.validate_timing_plan(plan)?,
                Command::Start(_) | Command::Stop(_) => {}
            }
        }
        Ok(PreparedBatch {
            id: batch.id,
            commands: batch.commands.clone(),
            physical_transactions,
        })
    }

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.token();
        let mut last = Value::Null;
        for command in prepared.commands {
            last = match command {
                Command::ReadProperty { device, key } => {
                    self.refresh_readback_property(device, &key)?;
                    self.read_property(device, &key)?
                }
                Command::Invoke {
                    device,
                    request: CapabilityRequest::GenericCommand(request),
                    ..
                } if device == self.hub => self.invoke_generic(request)?,
                Command::WriteProperty { device, key, value } => {
                    self.write_property(device, &key, value)?
                }
                Command::ApplyStateSet(set) => {
                    let mut changed = BTreeMap::new();
                    for write in set.writes {
                        let value =
                            self.write_property(write.device, &write.property, write.value)?;
                        changed.insert(write.property, value);
                    }
                    Value::Map(changed)
                }
                Command::Invoke {
                    device,
                    capability: CapabilityId(2),
                    request: CapabilityRequest::StageMove(request),
                } if device == self.focus => self.stage_move(request)?,
                Command::Invoke {
                    device,
                    capability: CapabilityId(3),
                    request: CapabilityRequest::None,
                } if device == self.focus => self.stop_focus()?,
                Command::Invoke {
                    device,
                    capability: CapabilityId(4),
                    request: CapabilityRequest::FilterSelect(request),
                } => self.filter_select(device, request)?,
                Command::Invoke {
                    device,
                    capability: CapabilityId(5),
                    request,
                } => self.trigger_shutter(device, request)?,
                Command::Arm(_) | Command::Start(_) | Command::Stop(_) => Value::Null,
                Command::Invoke { .. } => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "unsupported IX85 command",
                    ));
                }
            };
        }
        self.events
            .push_back(DriverEvent::TokenCompleted { token, value: last });
        Ok(token)
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
                resource: Some(self.resource),
                description: "ix85 timing arm summary".into(),
                payload: self.timing_summary(plan, "arm", Value::Map(BTreeMap::new())),
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
                resource: Some(self.resource),
                description: "ix85 timing start sequence".into(),
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
                resource: Some(self.resource),
                description: "ix85 timing stop sequence".into(),
                payload: self.timing_summary(&armed.plan, "stop", applied),
            }],
        })
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        self.events.drain(..).collect()
    }
}

fn capability(id: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    CapabilityDescriptor::new(CapabilityId(id), device, kind, ValueType::Map)
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

fn writable_property(key: &str, display_name: &str, value_type: ValueType) -> PropertySchema {
    let mut schema = property(key, display_name, value_type);
    schema.writable = true;
    schema.sequenceable = true;
    schema
}

fn string_property(key: &str, display_name: &str) -> PropertySchema {
    property(key, display_name, ValueType::String)
}

fn map_property(key: &str, display_name: &str) -> PropertySchema {
    property(key, display_name, ValueType::Map)
}

fn bool_property(key: &str, display_name: &str) -> PropertySchema {
    property(key, display_name, ValueType::Bool)
}

fn integer_metadata_property(key: &str, display_name: &str, value: i64) -> PropertySchema {
    let mut schema = property(key, display_name, ValueType::I64);
    schema.range = Some(Range {
        min: Value::I64(value),
        max: Value::I64(value),
    });
    schema
}

fn shared_metadata() -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("family".into(), Value::String("Evident IX85".into())),
        (
            "support_level".into(),
            Value::String("configured_or_opt_in_serial_control".into()),
        ),
        (
            "evidence_class".into(),
            Value::String("reverse engineered".into()),
        ),
    ])
}

fn command_summary(values: &[(&str, &str)]) -> Value {
    Value::Map(
        values
            .iter()
            .map(|(key, tag)| ((*key).into(), Value::String((*tag).into())))
            .collect(),
    )
}

fn protocol_tags() -> Value {
    Value::Map(BTreeMap::from([
        ("login".into(), Value::String("L".into())),
        ("unit".into(), Value::String("U".into())),
        ("version".into(), Value::String("V".into())),
        ("error".into(), Value::String("ER".into())),
        ("focus_position".into(), Value::String("FP".into())),
        ("nosepiece".into(), Value::String("OB".into())),
        ("light_path".into(), Value::String("BIL".into())),
        ("mirror_unit_1".into(), Value::String("MU1".into())),
        ("dia_shutter".into(), Value::String("DSH".into())),
        ("epi_shutter_1".into(), Value::String("ESH1".into())),
        ("autofocus_status".into(), Value::String("AFST".into())),
        (
            "hidden_control_tags".into(),
            Value::String("AF action parameters".into()),
        ),
    ]))
}

#[cfg_attr(not(feature = "os-serial"), allow(dead_code))]
struct Ix85Command {
    tag: &'static str,
    command: String,
}

impl Ix85Command {
    fn ack(tag: &'static str, parameter: Option<String>) -> Self {
        let command = match parameter {
            Some(parameter) => format!("{tag} {parameter}"),
            None => tag.into(),
        };
        Self { tag, command }
    }
}

#[cfg(feature = "os-serial")]
fn validate_ix85_ack(tag: &str, reply: &str) -> Result<()> {
    let trimmed = reply.trim();
    if trimmed.starts_with(&format!("{tag} +")) {
        return Ok(());
    }
    if trimmed.starts_with(&format!("{tag} !")) {
        return Err(Error::new(
            ErrorCode::Transport,
            format!("IX85 command {tag} returned negative acknowledgement"),
        ));
    }
    if trimmed.starts_with(&format!("{tag} X")) {
        return Err(Error::new(
            ErrorCode::Transport,
            format!("IX85 command {tag} returned unknown response"),
        ));
    }
    Err(Error::new(
        ErrorCode::Transport,
        format!("IX85 command {tag} returned unexpected response: {trimmed}"),
    ))
}

fn validate_focus_position(position: Position) -> Result<()> {
    let micrometers = position.micrometers();
    if micrometers.is_finite() && (0.0..=10_500.0).contains(&micrometers) {
        Ok(())
    } else {
        Err(Error::new(
            ErrorCode::InvalidProperty,
            "IX85 focus position must be in 0..=10500 um",
        ))
    }
}

fn validate_relative_focus_delta(delta: Position) -> Result<()> {
    let micrometers = delta.micrometers();
    if micrometers.is_finite() && (-10_500.0..=10_500.0).contains(&micrometers) {
        Ok(())
    } else {
        Err(Error::new(
            ErrorCode::InvalidProperty,
            "IX85 focus relative move must be finite and within +/-10500 um",
        ))
    }
}

fn focus_steps_from_position(position: Position) -> Result<i64> {
    validate_focus_position(position)?;
    Ok((position.micrometers() / 0.01).round() as i64)
}

fn focus_steps_from_delta(delta: Position) -> Result<i64> {
    validate_relative_focus_delta(delta)?;
    Ok((delta.micrometers() / 0.01).round() as i64)
}

fn validate_ix85_range(key: &str, value: i64, min: i64, max: i64) -> Result<()> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("IX85 property {key} must be in {min}..={max}"),
        ))
    }
}

fn optional_string_value(value: &Option<String>) -> Value {
    value.clone().map(Value::String).unwrap_or(Value::Null)
}

#[cfg(feature = "os-serial")]
fn clean_reply_text(reply: &str) -> &str {
    reply.trim()
}

#[cfg(feature = "os-serial")]
fn parse_ix85_i64_reply(reply: &str, context: &str) -> Result<i64> {
    let trimmed = reply.trim();
    if let Ok(value) = trimmed.parse::<i64>() {
        return Ok(value);
    }
    let mut last_number = None;
    let mut start = None;
    for (index, ch) in trimmed.char_indices() {
        if ch.is_ascii_digit() || (ch == '-' && start.is_none()) {
            if start.is_none() {
                start = Some(index);
            }
        } else if let Some(begin) = start.take() {
            if begin != index {
                last_number = Some(&trimmed[begin..index]);
            }
        }
    }
    if let Some(begin) = start {
        last_number = Some(&trimmed[begin..]);
    }
    let Some(value) = last_number else {
        return Err(Error::new(
            ErrorCode::Transport,
            format!("{context} reply has no numeric value: {trimmed}"),
        ));
    };
    value.parse::<i64>().map_err(|error| {
        Error::new(
            ErrorCode::Transport,
            format!("{context} reply numeric parse failed: {error}"),
        )
    })
}

#[cfg(feature = "os-serial")]
fn parse_ix85_slot_reply(reply: &str, context: &str, min: i64, max: i64) -> Result<i64> {
    let value = parse_ix85_i64_reply(reply, context)?;
    if !(min..=max).contains(&value) {
        return Err(Error::new(
            ErrorCode::Transport,
            format!("{context} reply must be in {min}..={max}, got {value}"),
        ));
    }
    Ok(value)
}

fn string_prop(device: &DeviceConfig, key: &str) -> Result<Option<String>> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("IX85 property {key} must be String"),
        )),
        None => Ok(None),
    }
}

fn optional_string_prop(
    device: &DeviceConfig,
    key: &str,
    fallback: Option<String>,
) -> Result<Option<String>> {
    match device.properties.get(key) {
        Some(Value::String(value)) if value.is_empty() => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(Value::Null) => Ok(None),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("IX85 property {key} must be String or Null"),
        )),
        None => Ok(fallback),
    }
}

fn bool_prop(device: &DeviceConfig, key: &str) -> Result<Option<bool>> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("IX85 property {key} must be Bool"),
        )),
        None => Ok(None),
    }
}

fn i64_range_prop(device: &DeviceConfig, key: &str, min: i64, max: i64) -> Result<Option<i64>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if (min..=max).contains(value) => Ok(Some(*value)),
        Some(Value::I64(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("IX85 property {key} must be in {min}..={max}"),
        )),
        _ => Ok(None),
    }
}

fn position_prop(device: &DeviceConfig, key: &str) -> Result<Option<Position>> {
    match device.properties.get(key) {
        Some(Value::Position(value))
            if (0.0..=10_500.0).contains(&value.micrometers())
                && value.micrometers().is_finite() =>
        {
            Ok(Some(*value))
        }
        Some(Value::Position(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("IX85 property {key} must be in 0..=10500 um"),
        )),
        _ => Ok(None),
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}
