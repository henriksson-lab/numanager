use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::fs;

#[derive(Debug, Clone, PartialEq)]
pub struct GenicamProbe {
    pub label: String,
    pub vendor: String,
    pub model: String,
    pub serial: String,
    pub transport: GenicamTransport,
    pub xml: String,
    pub fixture_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenicamTransport {
    Fixture,
    GigeVision,
    Usb3Vision,
    CameraLink,
    Custom(String),
}

impl GenicamTransport {
    pub fn kind(&self) -> String {
        match self {
            GenicamTransport::Fixture => "fixture".into(),
            GenicamTransport::GigeVision => "gige_vision".into(),
            GenicamTransport::Usb3Vision => "usb3_vision".into(),
            GenicamTransport::CameraLink => "camera_link".into(),
            GenicamTransport::Custom(kind) => kind.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenicamNodeMap {
    pub nodes: BTreeMap<String, GenicamNode>,
    pub categories: BTreeMap<String, GenicamCategory>,
    pub category_order: Vec<String>,
    pub ports: BTreeMap<String, GenicamPort>,
    pub registers: BTreeMap<String, GenicamRegister>,
}

impl GenicamNodeMap {
    pub fn parse(xml: &str) -> Result<Self> {
        let mut nodes = BTreeMap::new();
        for kind in [
            GenicamNodeKind::Integer,
            GenicamNodeKind::Float,
            GenicamNodeKind::Boolean,
            GenicamNodeKind::Enumeration,
            GenicamNodeKind::String,
            GenicamNodeKind::Command,
            GenicamNodeKind::IntSwissKnife,
            GenicamNodeKind::SwissKnife,
            GenicamNodeKind::Converter,
        ] {
            for tag in kind.xml_tags() {
                for block in element_blocks(xml, tag) {
                    let Some(name) = attr(&block.opening_tag, "Name") else {
                        continue;
                    };
                    let node = parse_node(&name, kind.clone(), &block.body)?;
                    nodes.insert(name, node);
                }
            }
        }
        populate_reverse_invalidators(&mut nodes);
        let (categories, category_order) = parse_categories(xml);
        let ports = parse_ports(xml);
        let registers = parse_registers(xml);
        if nodes.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "GenICam XML contained no supported nodes",
            ));
        }
        Ok(Self {
            nodes,
            categories,
            category_order,
            ports,
            registers,
        })
    }
}

fn populate_reverse_invalidators(nodes: &mut BTreeMap<String, GenicamNode>) {
    let edges = nodes
        .values()
        .flat_map(|node| {
            node.invalidated_by
                .iter()
                .cloned()
                .chain(
                    node.variables
                        .iter()
                        .map(|variable| variable.node_ref.clone()),
                )
                .map(|invalidator| (invalidator, node.name.clone()))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    for (invalidator, invalidated) in edges {
        if let Some(node) = nodes.get_mut(&invalidator) {
            if !node.invalidates.contains(&invalidated) {
                node.invalidates.push(invalidated);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenicamCategory {
    pub name: String,
    pub display_name: String,
    pub visibility: Option<String>,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenicamPort {
    pub name: String,
    pub display_name: String,
    pub access: GenicamAccess,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenicamRegister {
    pub name: String,
    pub display_name: String,
    pub access: GenicamAccess,
    pub port_ref: Option<String>,
    pub address: Option<String>,
    pub address_ref: Option<String>,
    pub length: Option<String>,
    pub length_ref: Option<String>,
    pub endian: Option<String>,
    pub category: Option<String>,
    pub visibility: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenicamNodeKind {
    Integer,
    Float,
    Boolean,
    Enumeration,
    String,
    Command,
    IntSwissKnife,
    SwissKnife,
    Converter,
}

impl GenicamNodeKind {
    fn xml_tags(&self) -> &'static [&'static str] {
        match self {
            GenicamNodeKind::Integer => &["Integer", "IntReg", "MaskedIntReg"],
            GenicamNodeKind::Float => &["Float", "FloatReg"],
            GenicamNodeKind::Boolean => &["Boolean"],
            GenicamNodeKind::Enumeration => &["Enumeration"],
            GenicamNodeKind::String => &["String", "StringReg"],
            GenicamNodeKind::Command => &["Command"],
            GenicamNodeKind::IntSwissKnife => &["IntSwissKnife"],
            GenicamNodeKind::SwissKnife => &["SwissKnife"],
            GenicamNodeKind::Converter => &["Converter"],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenicamFormulaVariable {
    pub name: String,
    pub node_ref: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenicamNode {
    pub name: String,
    pub display_name: String,
    pub tooltip: Option<String>,
    pub description: Option<String>,
    pub doc_url: Option<String>,
    pub kind: GenicamNodeKind,
    pub access: GenicamAccess,
    pub imposed_access: Option<GenicamAccess>,
    pub value: Value,
    pub min: Option<Value>,
    pub max: Option<Value>,
    pub min_ref: Option<String>,
    pub max_ref: Option<String>,
    pub unit: Option<String>,
    pub enum_values: Vec<GenicamEnumEntry>,
    pub address: Option<String>,
    pub address_ref: Option<String>,
    pub port_ref: Option<String>,
    pub length: Option<String>,
    pub length_ref: Option<String>,
    pub struct_ref: Option<String>,
    pub offset: Option<String>,
    pub endian: Option<String>,
    pub increment: Option<Value>,
    pub increment_ref: Option<String>,
    pub bit: Option<u8>,
    pub lsb: Option<u8>,
    pub msb: Option<u8>,
    pub sign: Option<String>,
    pub representation: Option<String>,
    pub visibility: Option<String>,
    pub polling_time_ms: Option<i64>,
    pub streamable: Option<bool>,
    pub category: Option<String>,
    pub selects: Vec<String>,
    pub selected_by: Vec<String>,
    pub available_ref: Option<String>,
    pub implemented_ref: Option<String>,
    pub locked_ref: Option<String>,
    pub value_ref: Option<String>,
    pub value_copy_ref: Option<String>,
    pub formula: Option<String>,
    pub formula_to: Option<String>,
    pub formula_from: Option<String>,
    pub variables: Vec<GenicamFormulaVariable>,
    pub command_value: Option<i64>,
    pub cache_mode: Option<String>,
    pub invalidated_by: Vec<String>,
    pub invalidates: Vec<String>,
    pub event_id: Option<String>,
    pub event_timestamp_ref: Option<String>,
    pub event_notification_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenicamAccess {
    ReadOnly,
    WriteOnly,
    ReadWrite,
    NotAvailable,
    NotImplemented,
}

impl GenicamAccess {
    fn readable(&self) -> bool {
        matches!(self, GenicamAccess::ReadOnly | GenicamAccess::ReadWrite)
    }

    fn writable(&self) -> bool {
        matches!(self, GenicamAccess::WriteOnly | GenicamAccess::ReadWrite)
    }

    fn unsupported(&self) -> Option<ErrorCode> {
        match self {
            GenicamAccess::NotAvailable | GenicamAccess::NotImplemented => {
                Some(ErrorCode::Unsupported)
            }
            GenicamAccess::ReadOnly | GenicamAccess::WriteOnly | GenicamAccess::ReadWrite => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenicamEnumEntry {
    pub symbol: String,
    pub display_name: String,
    pub value: Option<i64>,
    pub available_ref: Option<String>,
    pub implemented_ref: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GenicamDiscovery {
    next_id: DriverId,
    probes: Vec<GenicamProbe>,
}

impl GenicamDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![GenicamProbe::fixture()],
        }
    }

    pub fn from_probe(next_id: DriverId, probe: GenicamProbe) -> Self {
        Self {
            next_id,
            probes: vec![probe],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| {
                matches!(
                    device.driver.as_str(),
                    "genicam" | "genicam_fixture" | "genicam-fixture" | "genicam_node_map"
                )
            })
            .map(GenicamProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for GenicamDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        let mut candidates = Vec::new();
        for (index, probe) in self.probes.drain(..).enumerate() {
            let id = DriverId(self.next_id.0 + index as u64);
            candidates.push(DriverCandidate::from_driver(
                format!("Configured GenICam node map {}", probe.label),
                Box::new(GenicamDriver::from_probe(id, probe)?),
            ));
        }
        Ok(candidates)
    }
}

impl GenicamProbe {
    pub fn fixture() -> Self {
        Self {
            label: "genicam-local-camera".into(),
            vendor: "GenICam".into(),
            model: "Local NodeMap".into(),
            serial: "GENICAM-LOCAL-0001".into(),
            transport: GenicamTransport::Fixture,
            xml: FIXTURE_XML.into(),
            fixture_path: None,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = Self::fixture();
        probe.label = string_prop(device, "label").unwrap_or_else(|| device.label.clone());
        if probe.label.is_empty() {
            probe.label = "genicam-configured-camera".into();
        }
        if let Some(vendor) = string_prop(device, "vendor") {
            probe.vendor = vendor;
        }
        if let Some(model) = string_prop(device, "model") {
            probe.model = model;
        }
        if let Some(serial) =
            string_prop(device, "serial_number").or_else(|| string_prop(device, "serial"))
        {
            probe.serial = serial;
        }
        if let Some(transport) = string_prop(device, "transport") {
            probe.transport = parse_genicam_transport(&transport);
        }
        if let Some(xml) = string_prop(device, "xml") {
            GenicamNodeMap::parse(&xml)?;
            probe.xml = xml;
        }
        probe.fixture_path = string_prop(device, "fixture_path");
        Ok(probe)
    }
}

fn string_prop(device: &DeviceConfig, key: &str) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn parse_genicam_transport(value: &str) -> GenicamTransport {
    match value.to_ascii_lowercase().replace('-', "_").as_str() {
        "fixture" => GenicamTransport::Fixture,
        "gige_vision" | "gige" | "gev" => GenicamTransport::GigeVision,
        "usb3_vision" | "usb3" | "u3v" => GenicamTransport::Usb3Vision,
        "camera_link" | "cameralink" => GenicamTransport::CameraLink,
        _ => GenicamTransport::Custom(value.into()),
    }
}

pub struct GenicamDriver {
    id: DriverId,
    resource: ResourceId,
    camera: DeviceId,
    probe: GenicamProbe,
    node_map: GenicamNodeMap,
    values: BTreeMap<String, Value>,
    register_values: BTreeMap<String, Vec<u8>>,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
}

impl GenicamDriver {
    pub fn from_probe(id: DriverId, probe: GenicamProbe) -> Result<Self> {
        let node_map = GenicamNodeMap::parse(&probe.xml)?;
        let values = initial_node_values(&node_map)?;
        let register_values = initial_register_values(&node_map)?;
        Ok(Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 951)),
            camera: DeviceId(NodeId(id.0 * 1000 + 960)),
            probe,
            node_map,
            values,
            register_values,
            next_token: 1,
            pending: VecDeque::new(),
        })
    }

    pub fn configured_fixture(id: DriverId) -> Self {
        Self::from_probe(id, GenicamProbe::fixture()).expect("fixture XML is valid")
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device != self.camera {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown GenICam device",
            ));
        }
        self.read_node_value(key, &mut Vec::new())
    }

    fn read_node_value(&self, key: &str, stack: &mut Vec<String>) -> Result<Value> {
        if stack.iter().any(|seen| seen == key) {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("GenICam formula cycle involving {key}"),
            ));
        }
        let node = self
            .node_map
            .nodes
            .get(key)
            .ok_or_else(|| Error::new(ErrorCode::InvalidProperty, "unknown GenICam node"))?;
        self.validate_read_access(node)?;
        if is_formula_node(node) {
            stack.push(key.into());
            let value = self.evaluate_formula_node(node, stack);
            stack.pop();
            return value;
        }
        if let Some(value) = self.read_enum_backed_by_value_node(node, stack)? {
            return Ok(value);
        }
        let storage_key = self.storage_key(key)?;
        if let Some(value) = self.read_register_node(&storage_key)? {
            return Ok(value);
        }
        self.values
            .get(&storage_key)
            .cloned()
            .ok_or_else(|| Error::new(ErrorCode::InvalidProperty, "unknown GenICam node"))
    }

    fn evaluate_formula_node(&self, node: &GenicamNode, stack: &mut Vec<String>) -> Result<Value> {
        let formula = node
            .formula
            .as_ref()
            .or(node.formula_to.as_ref())
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("GenICam formula node {} has no formula", node.name),
                )
            })?;
        let mut variables = BTreeMap::new();
        for variable in &node.variables {
            let value = self.read_node_value(&variable.node_ref, stack)?;
            variables.insert(
                variable.name.clone(),
                numeric_value(&value, &variable.node_ref)?,
            );
        }
        let value = eval_formula(formula, &variables)?;
        match node.kind {
            GenicamNodeKind::IntSwissKnife => Ok(Value::I64(value.round() as i64)),
            GenicamNodeKind::SwissKnife | GenicamNodeKind::Converter => Ok(Value::F64(value)),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("GenICam node {} is not a formula node", node.name),
            )),
        }
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: Value) -> Result<Value> {
        if device != self.camera {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown GenICam device",
            ));
        }
        self.validate_write(key, &value)?;
        self.write_validated_node(key, value)
    }

    fn validate_write(&self, key: &str, value: &Value) -> Result<()> {
        let node = self
            .node_map
            .nodes
            .get(key)
            .ok_or_else(|| Error::new(ErrorCode::InvalidProperty, "unknown GenICam node"))?;
        self.validate_node_conditions(node)?;
        let access = effective_access(node);
        if let Some(code) = access.unsupported() {
            return Err(Error::new(
                code,
                format!("GenICam node {key} is not accessible"),
            ));
        }
        if self.condition_value(&node.locked_ref, false)? {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("GenICam node {key} is locked"),
            ));
        }
        let schema = self
            .property_schema(key)
            .ok_or_else(|| Error::new(ErrorCode::InvalidProperty, "unknown GenICam node"))?;
        if !schema.writable {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("GenICam node {key} is read-only"),
            ));
        }
        schema.validate(value)?;
        self.validate_enum_entry_write(node, value)?;
        self.validate_numeric_constraints(node, &schema, value)
    }

    fn validate_numeric_constraints(
        &self,
        node: &GenicamNode,
        schema: &PropertySchema,
        value: &Value,
    ) -> Result<()> {
        let Some(value_numeric) = numeric_value(value, &node.name).ok() else {
            return Ok(());
        };
        if let Some(min) = self.numeric_constraint(node.min.as_ref(), &node.min_ref)? {
            if value_numeric < min {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("property {} is below its minimum", schema.key),
                ));
            }
        }
        if let Some(max) = self.numeric_constraint(node.max.as_ref(), &node.max_ref)? {
            if value_numeric > max {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("property {} is above its maximum", schema.key),
                ));
            }
        }
        if let Some(increment) =
            self.numeric_constraint(node.increment.as_ref(), &node.increment_ref)?
        {
            if increment > 0.0 {
                let base = self
                    .numeric_constraint(node.min.as_ref(), &node.min_ref)?
                    .unwrap_or(0.0);
                let steps = (value_numeric - base) / increment;
                if (steps - steps.round()).abs() > 1e-9 {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        format!("property {} does not match its increment", schema.key),
                    ));
                }
            }
        }
        Ok(())
    }

    fn numeric_constraint(
        &self,
        literal: Option<&Value>,
        node_ref: &Option<String>,
    ) -> Result<Option<f64>> {
        if let Some(node_ref) = node_ref {
            let value = self.read_node_value(node_ref, &mut Vec::new())?;
            return Ok(Some(numeric_value(&value, node_ref)?));
        }
        literal
            .map(|value| numeric_value(value, "GenICam numeric constraint").map(Some))
            .unwrap_or(Ok(None))
    }

    fn validate_enum_entry_write(&self, node: &GenicamNode, value: &Value) -> Result<()> {
        if node.kind != GenicamNodeKind::Enumeration {
            return Ok(());
        }
        let Value::String(symbol) = value else {
            return Ok(());
        };
        let entry = node
            .enum_values
            .iter()
            .find(|entry| entry.symbol == *symbol)
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown GenICam enum entry {}.{symbol}", node.name),
                )
            })?;
        if !self.condition_value(&entry.implemented_ref, true)? {
            return Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "GenICam enum entry {}.{} is not supported by the current node model",
                    node.name, entry.symbol
                ),
            ));
        }
        if !self.condition_value(&entry.available_ref, true)? {
            return Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "GenICam enum entry {}.{} is not available",
                    node.name, entry.symbol
                ),
            ));
        }
        Ok(())
    }

    fn write_validated_node(&mut self, key: &str, value: Value) -> Result<Value> {
        if let Some((target_key, target_value)) = self.converter_write(key, &value)? {
            self.validate_write(&target_key, &target_value)?;
            self.write_plain_node(&target_key, target_value.clone())?;
            self.emit_property(&target_key, target_value);
            self.invalidate_after_write(&target_key);
            let converted = self.read_node_value(key, &mut Vec::new())?;
            self.emit_property(key, converted.clone());
            return Ok(converted);
        }
        if let Some((target_key, target_value)) = self.enum_value_write(key, &value)? {
            self.validate_write(&target_key, &target_value)?;
            self.write_plain_node(&target_key, target_value)?;
            let current = self.read_node_value(key, &mut Vec::new())?;
            self.emit_property(key, current.clone());
            self.invalidate_after_write(key);
            return Ok(current);
        }

        self.write_plain_node(key, value.clone())?;
        self.emit_property(key, value.clone());
        self.invalidate_after_write(key);
        Ok(value)
    }

    fn write_plain_node(&mut self, key: &str, value: Value) -> Result<()> {
        let storage_key = self.storage_key(key)?;
        self.write_register_node(&storage_key, &value)?;
        self.values.insert(storage_key, value);
        Ok(())
    }

    fn converter_write(&self, key: &str, value: &Value) -> Result<Option<(String, Value)>> {
        let Some(node) = self.node_map.nodes.get(key) else {
            return Ok(None);
        };
        if node.kind != GenicamNodeKind::Converter {
            return Ok(None);
        }
        let Some(formula) = &node.formula_from else {
            return Ok(None);
        };
        let input = numeric_value(value, key)?;
        let target = converter_write_target(node).ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("GenICam converter node {key} has no writable target variable"),
            )
        })?;
        let mut variables = BTreeMap::new();
        for variable in &node.variables {
            let variable_value = if variable.name == "TO" {
                input
            } else {
                let value = self.read_node_value(&variable.node_ref, &mut Vec::new())?;
                numeric_value(&value, &variable.node_ref)?
            };
            variables.insert(variable.name.clone(), variable_value);
        }
        if !variables.contains_key("TO") {
            variables.insert("TO".into(), input);
        }
        let converted = eval_formula(formula, &variables)?;
        let target_node = self.node_map.nodes.get(target).ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("GenICam converter node {key} references unknown target {target}"),
            )
        })?;
        let target_value = match target_node.kind {
            GenicamNodeKind::Integer => Value::I64(converted.round() as i64),
            GenicamNodeKind::Float => Value::F64(converted),
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!(
                        "GenICam converter node {key} cannot write target {}",
                        target_node.name
                    ),
                ))
            }
        };
        Ok(Some((target.into(), target_value)))
    }

    fn read_enum_backed_by_value_node(
        &self,
        node: &GenicamNode,
        stack: &mut Vec<String>,
    ) -> Result<Option<Value>> {
        if node.kind != GenicamNodeKind::Enumeration {
            return Ok(None);
        }
        let Some(value_ref) = &node.value_ref else {
            return Ok(None);
        };
        let value = self.read_node_value(value_ref, stack)?;
        let raw = value_i64(&value)?;
        let entry = node
            .enum_values
            .iter()
            .find(|entry| entry.value == Some(raw))
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!(
                        "GenICam enum node {} has no entry for backing value {raw}",
                        node.name
                    ),
                )
            })?;
        Ok(Some(Value::String(entry.symbol.clone())))
    }

    fn enum_value_write(&self, key: &str, value: &Value) -> Result<Option<(String, Value)>> {
        let Some(node) = self.node_map.nodes.get(key) else {
            return Ok(None);
        };
        if node.kind != GenicamNodeKind::Enumeration {
            return Ok(None);
        }
        let Some(value_ref) = &node.value_ref else {
            return Ok(None);
        };
        let Value::String(symbol) = value else {
            return Ok(None);
        };
        let entry = node
            .enum_values
            .iter()
            .find(|entry| entry.symbol == *symbol)
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown GenICam enum entry {}.{symbol}", node.name),
                )
            })?;
        let Some(raw) = entry.value else {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!(
                    "GenICam enum entry {}.{} has no numeric backing value",
                    node.name, entry.symbol
                ),
            ));
        };
        Ok(Some((value_ref.clone(), Value::I64(raw))))
    }

    fn read_register_node(&self, key: &str) -> Result<Option<Value>> {
        let Some(node) = self.node_map.nodes.get(key) else {
            return Ok(None);
        };
        let Some(location) = self.register_location(node)? else {
            return Ok(None);
        };
        let Some(bytes) = self.register_values.get(&location.key) else {
            return Ok(None);
        };
        let value = decode_register_value(node, bytes, location.little_endian)?;
        Ok(Some(apply_masked_read(node, value)?))
    }

    fn write_register_node(&mut self, key: &str, value: &Value) -> Result<()> {
        let Some(node) = self.node_map.nodes.get(key) else {
            return Ok(());
        };
        let Some(location) = self.register_location(node)? else {
            return Ok(());
        };
        let value = if let Some((shift, mask)) = masked_field(node) {
            let current = self
                .register_values
                .get(&location.key)
                .map(|bytes| decode_i64(bytes, location.little_endian))
                .transpose()?
                .unwrap_or(0) as u64;
            let field = value_i64(value)? as u64;
            let raw = (current & !mask) | ((field << shift) & mask);
            Value::I64(raw as i64)
        } else {
            value.clone()
        };
        let bytes = encode_register_value(node, &value, location.length, location.little_endian)?;
        self.register_values.insert(location.key, bytes);
        Ok(())
    }

    fn validate_read_access(&self, node: &GenicamNode) -> Result<()> {
        self.validate_node_conditions(node)?;
        let access = effective_access(node);
        if let Some(code) = access.unsupported() {
            return Err(Error::new(
                code,
                format!("GenICam node {} is not accessible", node.name),
            ));
        }
        if !access.readable() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("GenICam node {} is write-only", node.name),
            ));
        }
        Ok(())
    }

    fn validate_command_node(&self, command: &str) -> Result<()> {
        let Some(node) = self.node_map.nodes.get(command) else {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown GenICam command node {command}"),
            ));
        };
        if node.kind != GenicamNodeKind::Command {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("GenICam node {command} is not a command node"),
            ));
        }
        self.validate_node_conditions(node)?;
        let access = effective_access(node);
        if let Some(code) = access.unsupported() {
            return Err(Error::new(
                code,
                format!("GenICam command node {command} is not accessible"),
            ));
        }
        if !access.writable() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("GenICam command node {command} is not executable"),
            ));
        }
        if self.condition_value(&node.locked_ref, false)? {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("GenICam command node {command} is locked"),
            ));
        }
        Ok(())
    }

    fn validate_node_conditions(&self, node: &GenicamNode) -> Result<()> {
        if !self.condition_value(&node.implemented_ref, true)? {
            return Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "GenICam node {} is not supported by the current node model",
                    node.name
                ),
            ));
        }
        if !self.condition_value(&node.available_ref, true)? {
            return Err(Error::new(
                ErrorCode::Unsupported,
                format!("GenICam node {} is not available", node.name),
            ));
        }
        Ok(())
    }

    fn condition_value(&self, node_ref: &Option<String>, default: bool) -> Result<bool> {
        let Some(node_ref) = node_ref else {
            return Ok(default);
        };
        match self.read_condition_node(node_ref, &mut Vec::new())? {
            Value::Bool(value) => Ok(value),
            Value::I64(value) => Ok(value != 0),
            Value::F64(value) => Ok(value != 0.0),
            other => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("GenICam condition node {node_ref} is not boolean-like: {other:?}"),
            )),
        }
    }

    fn read_condition_node(&self, key: &str, stack: &mut Vec<String>) -> Result<Value> {
        if stack.iter().any(|seen| seen == key) {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("GenICam conditional access cycle involving {key}"),
            ));
        }
        let node = self.node_map.nodes.get(key).ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown GenICam condition node {key}"),
            )
        })?;
        let access = effective_access(node);
        if !access.readable() {
            if let Some(code) = access.unsupported() {
                return Err(Error::new(
                    code,
                    format!("GenICam condition node {key} is not accessible"),
                ));
            }
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("GenICam condition node {key} is not readable"),
            ));
        }
        if is_formula_node(node) {
            stack.push(key.into());
            let value = self.evaluate_formula_node(node, stack);
            stack.pop();
            return value;
        }
        let storage_key = self.storage_key(key)?;
        self.values.get(&storage_key).cloned().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown GenICam condition node {key}"),
            )
        })
    }

    fn property_schema(&self, key: &str) -> Option<PropertySchema> {
        self.node_map.nodes.get(key).and_then(node_schema)
    }

    fn storage_key(&self, key: &str) -> Result<String> {
        let node = self
            .node_map
            .nodes
            .get(key)
            .ok_or_else(|| Error::new(ErrorCode::InvalidProperty, "unknown GenICam node"))?;
        if let Some(value_ref) = &node.value_ref {
            if !self.node_map.nodes.contains_key(value_ref) {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("GenICam node {key} references unknown value node {value_ref}"),
                ));
            }
            Ok(value_ref.clone())
        } else {
            Ok(key.into())
        }
    }

    fn command_nodes(&self) -> Vec<String> {
        self.node_map
            .nodes
            .values()
            .filter(|node| node.kind == GenicamNodeKind::Command && !is_hidden_genicam_node(node))
            .map(|node| node.name.clone())
            .collect()
    }

    fn invoke_command(
        &mut self,
        device: DeviceId,
        capability: CapabilityId,
        request: CapabilityRequest,
    ) -> Result<Value> {
        if device != self.camera {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown GenICam device",
            ));
        }
        if capability != self.command_capability_id() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown GenICam capability",
            ));
        }
        let CapabilityRequest::GenericCommand(request) = request else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "GenICam command nodes expect GenericCommand",
            ));
        };
        if request.is_hidden_maintenance() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "GenericCommand {} is a hidden maintenance operation",
                    request.command
                ),
            ));
        }
        if is_hidden_genicam_command(&request.command)
            || self
                .node_map
                .nodes
                .get(&request.command)
                .is_some_and(is_hidden_genicam_node)
        {
            return Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "GenICam command node {} is a hidden maintenance command",
                    request.command
                ),
            ));
        }
        self.validate_command_node(&request.command)?;
        let command_register = self.write_command_register(&request.command)?;
        if request.command == "AcquisitionStart" {
            self.set_internal_value("AcquisitionActive", Value::Bool(true));
            self.emit_genicam_event("ExposureEnd")?;
        } else if request.command == "AcquisitionStop" {
            self.set_internal_value("AcquisitionActive", Value::Bool(false));
        }
        let mut result = BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("executed".into(), Value::Bool(true)),
        ]);
        if let Some((address, value)) = command_register {
            result.insert("register".into(), Value::String(address));
            result.insert("command_value".into(), Value::I64(value));
        }
        Ok(Value::Map(result))
    }

    fn write_command_register(&mut self, command: &str) -> Result<Option<(String, i64)>> {
        let Some(node) = self.node_map.nodes.get(command).cloned() else {
            return Ok(None);
        };
        let value = node.command_value.unwrap_or(1);
        if let Some(value_ref) = node.value_ref.clone() {
            let target = self
                .node_map
                .nodes
                .get(&value_ref)
                .cloned()
                .ok_or_else(|| {
                    Error::new(
                        ErrorCode::InvalidProperty,
                        format!(
                        "GenICam command node {command} references unknown value node {value_ref}"
                    ),
                    )
                })?;
            self.validate_node_conditions(&target)?;
            let access = effective_access(&target);
            if let Some(code) = access.unsupported() {
                return Err(Error::new(
                    code,
                    format!("GenICam command target {value_ref} is not accessible"),
                ));
            }
            if !access.writable() {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("GenICam command target {value_ref} is not writable"),
                ));
            }
            if self.condition_value(&target.locked_ref, false)? {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("GenICam command target {value_ref} is locked"),
                ));
            }
            let location = self.register_location(&target)?;
            self.write_plain_node(&value_ref, Value::I64(value))?;
            let address = location.map(|location| location.key).unwrap_or(value_ref);
            return Ok(Some((address, value)));
        }
        let Some(location) = self.register_location(&node)? else {
            return Ok(None);
        };
        let bytes = encode_i64(value, location.length, location.little_endian)?;
        self.register_values.insert(location.key.clone(), bytes);
        Ok(Some((location.key, value)))
    }

    fn register_location(&self, node: &GenicamNode) -> Result<Option<RegisterLocation>> {
        if let Some(struct_ref) = &node.struct_ref {
            let Some(register) = self.node_map.registers.get(struct_ref) else {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown GenICam StructReg {struct_ref}"),
                ));
            };
            let (Some(port), Some(base_address), Some(offset), Some(length)) = (
                register.port_ref.as_ref(),
                register_address_from_metadata(register)?,
                node.offset.as_ref(),
                self.register_length(node)?,
            ) else {
                return Ok(None);
            };
            let address = base_address + parse_register_address(offset)?;
            return Ok(Some(RegisterLocation {
                key: register_storage_key(port, address),
                length,
                little_endian: is_little_endian(node)
                    && !matches!(
                        register.endian.as_deref(),
                        Some("BigEndian") | Some("BigEndianess")
                    ),
            }));
        }
        let Some(port) = &node.port_ref else {
            return Ok(None);
        };
        let Some(address) = self.register_address(node)? else {
            return Ok(None);
        };
        let Some(length) = self.register_length(node)? else {
            return Ok(None);
        };
        Ok(Some(RegisterLocation {
            key: register_storage_key(port, address),
            length,
            little_endian: is_little_endian(node),
        }))
    }

    fn register_address(&self, node: &GenicamNode) -> Result<Option<u64>> {
        if let Some(address) = &node.address {
            return parse_register_address(address).map(Some);
        }
        let Some(address_ref) = &node.address_ref else {
            return Ok(None);
        };
        let value = self.read_node_value(address_ref, &mut Vec::new())?;
        parse_register_address_value(address_ref, &value).map(Some)
    }

    fn register_length(&self, node: &GenicamNode) -> Result<Option<usize>> {
        if let Some(length) = &node.length {
            return parse_register_length(length).map(Some);
        }
        let Some(length_ref) = &node.length_ref else {
            return Ok(None);
        };
        let value = self.read_node_value(length_ref, &mut Vec::new())?;
        parse_register_length_value(length_ref, &value).map(Some)
    }

    fn emit_genicam_event(&mut self, event_node: &str) -> Result<()> {
        let Some(node) = self.node_map.nodes.get(event_node) else {
            return Ok(());
        };
        let mut values = BTreeMap::from([
            (
                "source".into(),
                Value::String("genicam.event_channel".into()),
            ),
            ("event_node".into(), Value::String(node.name.clone())),
        ]);
        if let Some(event_id) = &node.event_id {
            values.insert("event_id".into(), Value::String(event_id.clone()));
        }
        if let Some(timestamp_ref) = &node.event_timestamp_ref {
            let timestamp = self.read_node_value(timestamp_ref, &mut Vec::new())?;
            values.insert("event_timestamp".into(), timestamp);
            values.insert(
                "event_timestamp_ref".into(),
                Value::String(timestamp_ref.clone()),
            );
        }
        if let Some(notification_ref) = &node.event_notification_ref {
            let notification = self.read_node_value(notification_ref, &mut Vec::new())?;
            values.insert("event_notification".into(), notification);
            values.insert(
                "event_notification_ref".into(),
                Value::String(notification_ref.clone()),
            );
        }
        self.pending
            .push_back(DriverEvent::Event(Event::Telemetry(TelemetryEvent {
                device: self.camera,
                values,
            })));
        Ok(())
    }

    fn set_internal_value(&mut self, key: &str, value: Value) {
        if self.node_map.nodes.contains_key(key) {
            if let Ok(storage_key) = self.storage_key(key) {
                self.values.insert(storage_key, value.clone());
                self.emit_property(key, value.clone());
                self.invalidate_after_write(key);
            }
        }
    }

    fn command_capability_id(&self) -> CapabilityId {
        CapabilityId(self.id.0 * 1000 + 970)
    }

    fn capture_capability_id(&self) -> CapabilityId {
        CapabilityId(self.id.0 * 1000 + 971)
    }

    fn stream_capability_id(&self) -> CapabilityId {
        CapabilityId(self.id.0 * 1000 + 972)
    }

    fn raw_register_capability_id(&self) -> CapabilityId {
        CapabilityId(self.id.0 * 1000 + 973)
    }

    fn trigger_sink_capability_id(&self) -> CapabilityId {
        CapabilityId(self.id.0 * 1000 + 974)
    }

    fn trigger_source_capability_id(&self) -> CapabilityId {
        CapabilityId(self.id.0 * 1000 + 975)
    }

    fn supports_acquisition_trigger(&self) -> bool {
        self.node_map
            .nodes
            .get("AcquisitionStart")
            .is_some_and(|node| node.kind == GenicamNodeKind::Command)
            && self
                .node_map
                .nodes
                .get("AcquisitionStop")
                .is_some_and(|node| node.kind == GenicamNodeKind::Command)
    }

    fn trigger_transaction(
        &self,
        kind: CapabilityKind,
        action: GenicamTriggerAction,
    ) -> PhysicalTransaction {
        PhysicalTransaction {
            resource: Some(self.resource),
            description: format!("GenICam {} {}", kind.name(), action.command_label()),
            payload: Value::Map(BTreeMap::from([
                ("capability".into(), Value::String(kind.name().into())),
                ("action".into(), Value::String(action.name().into())),
                (
                    "commands".into(),
                    Value::List(
                        action
                            .commands()
                            .into_iter()
                            .map(|command| Value::String(command.into()))
                            .collect(),
                    ),
                ),
            ])),
        }
    }

    fn invoke_trigger(
        &mut self,
        kind: CapabilityKind,
        action: GenicamTriggerAction,
    ) -> Result<Value> {
        let mut commands = Vec::new();
        let mut registers = Vec::new();
        for command in action.commands() {
            self.validate_command_node(command)?;
            let command_register = self.write_command_register(command)?;
            if command == "AcquisitionStart" {
                self.set_internal_value("AcquisitionActive", Value::Bool(true));
                self.emit_genicam_event("ExposureEnd")?;
            } else if command == "AcquisitionStop" {
                self.set_internal_value("AcquisitionActive", Value::Bool(false));
            }
            commands.push(Value::String(command.into()));
            if let Some((address, value)) = command_register {
                registers.push(Value::Map(BTreeMap::from([
                    ("command".into(), Value::String(command.into())),
                    ("register".into(), Value::String(address)),
                    ("command_value".into(), Value::I64(value)),
                ])));
            }
        }
        self.pending
            .push_back(DriverEvent::Event(Event::Telemetry(TelemetryEvent {
                device: self.camera,
                values: BTreeMap::from([
                    ("protocol".into(), Value::String("GenICam node map".into())),
                    ("capability".into(), Value::String(kind.name().into())),
                    ("action".into(), Value::String(action.name().into())),
                ]),
            })));
        Ok(Value::Map(BTreeMap::from([
            ("protocol".into(), Value::String("GenICam node map".into())),
            ("capability".into(), Value::String(kind.name().into())),
            ("action".into(), Value::String(action.name().into())),
            ("commands".into(), Value::List(commands)),
            ("registers".into(), Value::List(registers)),
        ])))
    }

    fn parse_raw_register_request(
        &self,
        request: &CapabilityRequest,
    ) -> Result<GenicamRawRegisterRequest> {
        let CapabilityRequest::GenericCommand(request) = request else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "GenICam RawRegisterAccess expects GenericCommand",
            ));
        };
        if request.is_hidden_maintenance() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "GenericCommand {} is a hidden maintenance operation",
                    request.command
                ),
            ));
        }
        let target = self.raw_register_target(request)?;
        match request.command.as_str() {
            "read" | "ReadRegister" | "read_register" => Ok(GenicamRawRegisterRequest::Read {
                target,
                byte_count: request
                    .params
                    .get("byte_count")
                    .or_else(|| request.params.get("length"))
                    .map(value_usize)
                    .transpose()?,
            }),
            "write" | "WriteRegister" | "write_register" => {
                if target.node.is_none() {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "GenICam RawRegisterAccess writes require a non-maintenance node target",
                    ));
                }
                let bytes = request.params.get("bytes").map(value_bytes).transpose()?;
                let value = request.params.get("value").cloned();
                if bytes.is_none() && value.is_none() {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "GenICam RawRegisterAccess write missing value or bytes",
                    ));
                }
                Ok(GenicamRawRegisterRequest::Write {
                    target,
                    bytes,
                    value,
                })
            }
            other => Err(Error::new(
                ErrorCode::InvalidCommand,
                format!("unsupported GenICam RawRegisterAccess command {other}"),
            )),
        }
    }

    fn raw_register_target(
        &self,
        request: &GenericCommandRequest,
    ) -> Result<GenicamRawRegisterTarget> {
        if let Some(Value::String(node_name)) = request.params.get("node") {
            if is_hidden_genicam_command(node_name) {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    format!("GenICam node {node_name} is a hidden maintenance target"),
                ));
            }
            let node = self.node_map.nodes.get(node_name).ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown GenICam node {node_name}"),
                )
            })?;
            if is_hidden_genicam_node(node) {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    format!("GenICam node {node_name} is a hidden maintenance target"),
                ));
            }
            let location = self.register_location(node)?.ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("GenICam node {node_name} has no register location"),
                )
            })?;
            return Ok(GenicamRawRegisterTarget {
                key: location.key,
                node: Some(node_name.clone()),
                length: Some(location.length),
                little_endian: location.little_endian,
            });
        }
        if let Some(Value::String(key)) = request.params.get("register") {
            return Ok(GenicamRawRegisterTarget {
                key: key.clone(),
                node: None,
                length: request
                    .params
                    .get("byte_count")
                    .or_else(|| request.params.get("length"))
                    .map(value_usize)
                    .transpose()?,
                little_endian: true,
            });
        }
        let address = request.params.get("address").ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidCommand,
                "GenICam RawRegisterAccess missing node, register, or address",
            )
        })?;
        let address = parse_register_address_value("address", address)?;
        let port = request
            .params
            .get("port")
            .map(value_string)
            .transpose()?
            .or_else(|| {
                if self.node_map.ports.len() == 1 {
                    self.node_map.ports.keys().next().cloned()
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidCommand,
                    "GenICam RawRegisterAccess address requires port when the node map has multiple ports",
                )
            })?;
        Ok(GenicamRawRegisterTarget {
            key: register_storage_key(&port, address),
            node: None,
            length: request
                .params
                .get("byte_count")
                .or_else(|| request.params.get("length"))
                .map(value_usize)
                .transpose()?,
            little_endian: true,
        })
    }

    fn raw_register_transaction(&self, request: &GenicamRawRegisterRequest) -> PhysicalTransaction {
        PhysicalTransaction {
            resource: Some(self.resource),
            description: match request {
                GenicamRawRegisterRequest::Read { target, .. } => {
                    format!("GenICam RawRegisterAccess read {}", target.label())
                }
                GenicamRawRegisterRequest::Write { target, .. } => {
                    format!("GenICam RawRegisterAccess write {}", target.label())
                }
            },
            payload: request.payload(),
        }
    }

    fn invoke_raw_register(&mut self, request: GenicamRawRegisterRequest) -> Result<Value> {
        match request {
            GenicamRawRegisterRequest::Read { target, byte_count } => {
                let bytes = self
                    .register_values
                    .get(&target.key)
                    .cloned()
                    .ok_or_else(|| {
                        Error::new(
                            ErrorCode::InvalidProperty,
                            format!("unknown GenICam register {}", target.key),
                        )
                    })?;
                let bytes = if let Some(byte_count) = byte_count {
                    bytes.into_iter().take(byte_count).collect()
                } else {
                    bytes
                };
                let mut result = BTreeMap::from([
                    ("protocol".into(), Value::String("GenICam register".into())),
                    ("operation".into(), Value::String("read".into())),
                    ("register".into(), Value::String(target.key.clone())),
                    ("bytes".into(), Value::Bytes(bytes.clone())),
                    ("byte_count".into(), Value::I64(bytes.len() as i64)),
                ]);
                if let Some(node_name) = target.node {
                    if let Some(node) = self.node_map.nodes.get(&node_name) {
                        result.insert("node".into(), Value::String(node_name.clone()));
                        if let Ok(decoded) =
                            decode_register_value(node, &bytes, target.little_endian)
                        {
                            result.insert("value".into(), decoded);
                        }
                    }
                }
                Ok(Value::Map(result))
            }
            GenicamRawRegisterRequest::Write {
                target,
                bytes,
                value,
            } => {
                let bytes = self.raw_register_write_bytes(&target, bytes, value)?;
                self.register_values
                    .insert(target.key.clone(), bytes.clone());
                let mut result = BTreeMap::from([
                    ("protocol".into(), Value::String("GenICam register".into())),
                    ("operation".into(), Value::String("write".into())),
                    ("register".into(), Value::String(target.key.clone())),
                    ("bytes".into(), Value::Bytes(bytes.clone())),
                    ("byte_count".into(), Value::I64(bytes.len() as i64)),
                ]);
                if let Some(node_name) = target.node {
                    result.insert("node".into(), Value::String(node_name.clone()));
                    if let Some(node) = self.node_map.nodes.get(&node_name) {
                        if let Ok(decoded) =
                            decode_register_value(node, &bytes, target.little_endian)
                        {
                            result.insert("value".into(), decoded.clone());
                            self.emit_property(&node_name, decoded);
                            self.invalidate_after_write(&node_name);
                        }
                    }
                }
                Ok(Value::Map(result))
            }
        }
    }

    fn raw_register_write_bytes(
        &self,
        target: &GenicamRawRegisterTarget,
        bytes: Option<Vec<u8>>,
        value: Option<Value>,
    ) -> Result<Vec<u8>> {
        if let Some(bytes) = bytes {
            return Ok(bytes);
        }
        let Some(value) = value else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "GenICam RawRegisterAccess write missing value or bytes",
            ));
        };
        let Some(node_name) = &target.node else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "GenICam RawRegisterAccess typed value writes require a node target",
            ));
        };
        let node = self.node_map.nodes.get(node_name).ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown GenICam node {node_name}"),
            )
        })?;
        let length = target.length.ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("GenICam node {node_name} has unknown register length"),
            )
        })?;
        self.validate_write(node_name, &value)?;
        encode_register_value(node, &value, length, target.little_endian)
    }

    fn invoke_camera_capture(
        &mut self,
        device: DeviceId,
        request: CapabilityRequest,
        token: DriverToken,
    ) -> Result<Value> {
        let request = match request {
            CapabilityRequest::CameraCapture(request) => request,
            CapabilityRequest::None => CameraCaptureRequest::default_frame(),
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "CameraCapture expects CameraCaptureRequest",
                ))
            }
        };
        let encoding = request.encoding.unwrap_or(ImageEncoding::Native);
        let buffer = request.buffer.unwrap_or_default();
        let handle = FrameHandle {
            stream: StreamId(device.0 .0),
            frame: FrameId(token.0),
        };
        let frame = self.gen_frame(device, handle, encoding, buffer, None)?;
        let width = frame.width;
        let height = frame.height;
        let pixel_format = frame.pixel_format.clone();
        self.pending.push_back(DriverEvent::FrameReady(frame));
        Ok(Value::Map(BTreeMap::from([
            ("width".into(), Value::PixelCount(PixelCount::new(width))),
            ("height".into(), Value::PixelCount(PixelCount::new(height))),
            ("pixel_format".into(), Value::String(pixel_format)),
            ("stream".into(), Value::I64(handle.stream.0 as i64)),
            ("frame".into(), Value::I64(handle.frame.0 as i64)),
        ])))
    }

    fn invoke_camera_stream(
        &mut self,
        device: DeviceId,
        request: CapabilityRequest,
        token: DriverToken,
    ) -> Result<Value> {
        let request = match request {
            CapabilityRequest::CameraStream(request) => request,
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "CameraStream expects CameraStreamRequest",
                ))
            }
        };
        let encoding = request.encoding.unwrap_or(ImageEncoding::Native);
        let frame_count = request.frame_count.unwrap_or(8);
        let stream = StreamId(token.0);
        let mut width = None;
        let mut height = None;
        let mut pixel_format = None;
        for index in 0..frame_count {
            let handle = FrameHandle {
                stream,
                frame: FrameId(index),
            };
            let frame = self.gen_frame(
                device,
                handle,
                encoding.clone(),
                request.buffer.clone(),
                Some(index),
            )?;
            width.get_or_insert(frame.width);
            height.get_or_insert(frame.height);
            pixel_format.get_or_insert_with(|| frame.pixel_format.clone());
            self.pending.push_back(DriverEvent::FrameReady(frame));
        }
        let mut values = BTreeMap::from([
            ("stream".into(), Value::I64(stream.0 as i64)),
            ("frame_count".into(), Value::I64(frame_count as i64)),
        ]);
        if let Some(width) = width {
            values.insert("width".into(), Value::PixelCount(PixelCount::new(width)));
        }
        if let Some(height) = height {
            values.insert("height".into(), Value::PixelCount(PixelCount::new(height)));
        }
        if let Some(pixel_format) = pixel_format {
            values.insert("pixel_format".into(), Value::String(pixel_format));
        }
        Ok(Value::Map(values))
    }

    fn gen_frame(
        &self,
        device: DeviceId,
        handle: FrameHandle,
        encoding: ImageEncoding,
        buffer: FrameBufferSpec,
        index: Option<u64>,
    ) -> Result<Frame> {
        let width = self
            .read_node_value("Width", &mut Vec::new())
            .ok()
            .and_then(|value| value_i64(&value).ok())
            .unwrap_or(640)
            .clamp(1, u32::MAX as i64) as u32;
        let height = self
            .read_node_value("Height", &mut Vec::new())
            .ok()
            .and_then(|value| value_i64(&value).ok())
            .unwrap_or(480)
            .clamp(1, u32::MAX as i64) as u32;
        let exposure_s = self
            .read_node_value("ExposureSeconds", &mut Vec::new())
            .ok()
            .and_then(|value| numeric_value(&value, "ExposureSeconds").ok())
            .unwrap_or(0.01);
        let native_format = self
            .read_node_value("PixelFormat", &mut Vec::new())
            .ok()
            .and_then(|value| match value {
                Value::String(value) => Some(value),
                _ => None,
            })
            .unwrap_or_else(|| "Mono8".into());
        let pixel_format = pixel_format_name(&encoding, &native_format).to_string();
        let (width, height, data, source) = if let Some(path) = &self.probe.fixture_path {
            let bytes = fs::read(path).map_err(|err| {
                Error::new(
                    ErrorCode::Driver,
                    format!("failed to read GenICam fixture frame {path}: {err}"),
                )
            })?;
            let frame = crate::platform_camera::decode_portable_pixmap(&bytes, &pixel_format)?;
            (
                frame.width,
                frame.height,
                frame.pixels,
                "genicam-fixture-file",
            )
        } else {
            (
                width,
                height,
                synthetic_frame_data(width, height, &pixel_format, exposure_s, index),
                "genicam-fixture-stream",
            )
        };
        let frame_index = index.unwrap_or(handle.frame.0);
        let hardware_timestamp = self
            .read_i64_node("EventExposureEndTimestamp")
            .unwrap_or(1_000_000)
            + (frame_index as i64 * 10_000);
        let mut metadata = BTreeMap::from([
            ("source".into(), Value::String(source.into())),
            ("chunk_frame_id".into(), Value::I64(frame_index as i64)),
            ("hardware_timestamp".into(), timestamp(hardware_timestamp)),
            ("chunk_metadata".into(), Value::Bool(true)),
            (
                "exposure".into(),
                Value::TimeInterval(TimeInterval::from_seconds(exposure_s)),
            ),
            ("width_node".into(), Value::I64(width as i64)),
            ("height_node".into(), Value::I64(height as i64)),
            ("pixel_format_node".into(), Value::String(native_format)),
            (
                "payload_size".into(),
                self.read_node_value("PayloadSize", &mut Vec::new())
                    .unwrap_or(Value::I64(data.len() as i64)),
            ),
            (
                "gain_db".into(),
                self.read_node_value("Gain", &mut Vec::new())
                    .unwrap_or(Value::Null),
            ),
            (
                "acquisition_frame_rate_hz".into(),
                self.read_node_value("AcquisitionFrameRate", &mut Vec::new())
                    .unwrap_or(Value::Null),
            ),
            (
                "line_time_s".into(),
                self.read_f64_node("LineTime")
                    .map(TimeInterval::from_seconds)
                    .map(Value::TimeInterval)
                    .unwrap_or(Value::Null),
            ),
            ("streamable_nodes".into(), self.streamable_node_values()),
        ]);
        if let Some(path) = &self.probe.fixture_path {
            metadata.insert("fixture_path".into(), Value::String(path.clone()));
        }
        if let Some(index) = index {
            metadata.insert("index".into(), Value::I64(index as i64));
        }
        Ok(Frame {
            handle,
            device,
            width,
            height,
            pixel_format,
            data,
            metadata,
            buffer,
        })
    }

    fn read_i64_node(&self, key: &str) -> Result<i64> {
        let value = self.read_node_value(key, &mut Vec::new())?;
        value_i64(&value)
    }

    fn read_f64_node(&self, key: &str) -> Result<f64> {
        let value = self.read_node_value(key, &mut Vec::new())?;
        numeric_value(&value, key)
    }

    fn streamable_node_values(&self) -> Value {
        Value::Map(
            self.node_map
                .nodes
                .values()
                .filter(|node| node.streamable == Some(true))
                .filter_map(|node| {
                    self.read_node_value(&node.name, &mut Vec::new())
                        .ok()
                        .map(|value| (node.name.clone(), value))
                })
                .collect(),
        )
    }

    fn apply_state_set(&mut self, set: StateSet) -> Result<Value> {
        for write in &set.writes {
            if write.device != self.camera {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "GenICam state set contains an unknown device",
                ));
            }
            self.validate_write(&write.property, &write.value)?;
        }

        let mut changed = BTreeMap::new();
        for write in set.writes {
            let value = self.write_validated_node(&write.property, write.value)?;
            changed.insert(write.property.clone(), value);
        }
        Ok(Value::Map(changed))
    }

    fn emit_property(&mut self, key: &str, value: Value) {
        self.pending
            .push_back(DriverEvent::Event(Event::PropertyChanged(
                PropertyChanged {
                    device: self.camera,
                    key: key.into(),
                    value,
                },
            )));
    }

    fn invalidate_after_write(&mut self, key: &str) {
        let mut invalidated = self
            .node_map
            .nodes
            .get(key)
            .map(|node| node.invalidates.clone())
            .unwrap_or_default();
        if let Ok(storage_key) = self.storage_key(key) {
            if storage_key != key {
                if let Some(node) = self.node_map.nodes.get(&storage_key) {
                    invalidated.extend(node.invalidates.clone());
                }
            }
        }
        invalidated.sort();
        invalidated.dedup();
        for node in invalidated {
            if let Ok(value) = self.read_property(self.camera, &node) {
                self.emit_property(&node, value);
            }
        }
    }

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| sequence.device == self.camera)
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequences(plan) {
            if sequence.values.is_empty() {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "GenICam timing sequence must contain at least one value",
                ));
            }
            let node =
                self.node_map.nodes.get(&sequence.property).ok_or_else(|| {
                    Error::new(ErrorCode::InvalidProperty, "unknown GenICam node")
                })?;
            let schema = node_schema(node).ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("GenICam node {} is not a value property", sequence.property),
                )
            })?;
            if !schema.sequenceable {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("GenICam property {} is not sequenceable", sequence.property),
                ));
            }
            for value in &sequence.values {
                self.validate_write(&sequence.property, value)?;
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

    fn timing_readback(&self, key: &str) -> Value {
        self.read_property(self.camera, key).unwrap_or(Value::Null)
    }

    fn timing_summary(&self, plan: &TimingPlan, phase: &str, applied: Value) -> Value {
        Value::Map(BTreeMap::from([
            ("camera".into(), Value::I64(self.camera.0 .0 as i64)),
            ("phase".into(), Value::String(phase.into())),
            (
                "sequences".into(),
                Value::List(self.timing_sequence_summary(plan)),
            ),
            ("ExposureTime".into(), self.timing_readback("ExposureTime")),
            ("Gain".into(), self.timing_readback("Gain")),
            (
                "AcquisitionFrameRate".into(),
                self.timing_readback("AcquisitionFrameRate"),
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
                    "GenICam timing sequence must contain at least one value",
                )
            })?
            .clone();
            let applied_value = self.write_property(self.camera, &sequence.property, value)?;
            applied.insert(sequence.property, applied_value);
        }
        Ok(Value::Map(applied))
    }
}

impl Driver for GenicamDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        vec![DeviceDescriptor {
            id: self.camera,
            driver: self.id,
            label: self.probe.label.clone(),
            vendor: Some(self.probe.vendor.clone()),
            model: Some(self.probe.model.clone()),
            serial: Some(self.probe.serial.clone()),
            kinds: vec![
                "camera".into(),
                "genicam".into(),
                "genicam.node_map".into(),
                self.probe.transport.kind(),
            ],
            properties: self
                .node_map
                .nodes
                .values()
                .filter_map(node_schema)
                .collect(),
            metadata: BTreeMap::from([
                (
                    "standard".into(),
                    Value::String("GenICam Standard Features Naming Convention".into()),
                ),
                (
                    "node_count".into(),
                    Value::I64(self.node_map.nodes.len() as i64),
                ),
                (
                    "transport".into(),
                    Value::String(self.probe.transport.kind()),
                ),
                (
                    "command_nodes".into(),
                    Value::List(
                        self.command_nodes()
                            .into_iter()
                            .map(Value::String)
                            .collect(),
                    ),
                ),
                ("categories".into(), category_metadata(&self.node_map)),
                (
                    "category_order".into(),
                    Value::List(
                        self.node_map
                            .category_order
                            .iter()
                            .cloned()
                            .map(Value::String)
                            .collect(),
                    ),
                ),
                (
                    "root_category_order".into(),
                    Value::List(
                        root_category_order(&self.node_map)
                            .into_iter()
                            .map(Value::String)
                            .collect(),
                    ),
                ),
                (
                    "category_metadata".into(),
                    category_detail_metadata(&self.node_map),
                ),
                (
                    "category_tree".into(),
                    category_tree_metadata(&self.node_map),
                ),
                ("ports".into(), port_metadata(&self.node_map)),
                ("registers".into(), register_metadata(&self.node_map)),
                ("node_metadata".into(), node_metadata(&self.node_map)),
            ]),
        }]
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: format!("{}-node-map", self.probe.label),
            kind: "genicam.node_map".into(),
            metadata: BTreeMap::from([
                (
                    "transport".into(),
                    Value::String(self.probe.transport.kind()),
                ),
                (
                    "completion".into(),
                    Value::String(
                        "node writes complete when the register/transport layer accepts them"
                            .into(),
                    ),
                ),
            ]),
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device != self.camera {
            return Vec::new();
        }
        let mut capabilities = vec![
            CapabilityDescriptor::with_name(
                self.capture_capability_id(),
                device,
                CapabilityKind::CameraCapture,
                "GenICamCapture",
                ValueType::Map,
            ),
            CapabilityDescriptor::with_name(
                self.stream_capability_id(),
                device,
                CapabilityKind::CameraStream,
                "GenICamStream",
                ValueType::Map,
            ),
            CapabilityDescriptor::with_name(
                self.raw_register_capability_id(),
                device,
                CapabilityKind::RawRegisterAccess,
                "GenICamRawRegister",
                ValueType::Map,
            ),
        ];
        if !self.command_nodes().is_empty() {
            capabilities.push(CapabilityDescriptor::with_name(
                self.command_capability_id(),
                device,
                CapabilityKind::GenericCommand,
                "GenICamCommand",
                ValueType::Map,
            ));
        }
        if self.supports_acquisition_trigger() {
            capabilities.push(CapabilityDescriptor::with_name(
                self.trigger_sink_capability_id(),
                device,
                CapabilityKind::TriggerSink,
                "GenICamAcquisitionTriggerSink",
                ValueType::Map,
            ));
            capabilities.push(CapabilityDescriptor::with_name(
                self.trigger_source_capability_id(),
                device,
                CapabilityKind::TriggerSource,
                "GenICamAcquisitionTriggerSource",
                ValueType::Map,
            ));
        }
        capabilities
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut physical_transactions = Vec::new();
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    let _ = self.read_property(*device, key)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("genicam read node {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    if *device != self.camera {
                        return Err(Error::new(ErrorCode::InvalidCommand, "unknown device"));
                    }
                    self.validate_write(key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("genicam write node {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        if write.device != self.camera {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "GenICam state set contains an unknown device",
                            ));
                        }
                        self.validate_write(&write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "genicam coalesced node state set".into(),
                        payload: Value::List(
                            set.writes
                                .iter()
                                .map(|write| Value::String(write.property.clone()))
                                .collect(),
                        ),
                    });
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    if *device != self.camera {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "unknown GenICam device",
                        ));
                    }
                    if *capability == self.capture_capability_id() {
                        if !matches!(
                            request,
                            CapabilityRequest::CameraCapture(_) | CapabilityRequest::None
                        ) {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "CameraCapture expects CameraCaptureRequest",
                            ));
                        }
                        physical_transactions.push(PhysicalTransaction {
                            resource: Some(self.resource),
                            description: "genicam fixture camera capture".into(),
                            payload: Value::String("single frame".into()),
                        });
                    } else if *capability == self.stream_capability_id() {
                        if !matches!(request, CapabilityRequest::CameraStream(_)) {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "CameraStream expects CameraStreamRequest",
                            ));
                        }
                        physical_transactions.push(PhysicalTransaction {
                            resource: Some(self.resource),
                            description: "genicam fixture camera stream".into(),
                            payload: Value::String("frame stream".into()),
                        });
                    } else if *capability == self.raw_register_capability_id() {
                        let request = self.parse_raw_register_request(request)?;
                        physical_transactions.push(self.raw_register_transaction(&request));
                    } else if *capability == self.trigger_sink_capability_id() {
                        let action =
                            parse_genicam_trigger_action(request, &CapabilityKind::TriggerSink)?;
                        physical_transactions
                            .push(self.trigger_transaction(CapabilityKind::TriggerSink, action));
                    } else if *capability == self.trigger_source_capability_id() {
                        let action =
                            parse_genicam_trigger_action(request, &CapabilityKind::TriggerSource)?;
                        physical_transactions
                            .push(self.trigger_transaction(CapabilityKind::TriggerSource, action));
                    } else if *capability == self.command_capability_id() {
                        let CapabilityRequest::GenericCommand(request) = request else {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "GenICam command nodes expect GenericCommand",
                            ));
                        };
                        if is_hidden_genicam_command(&request.command)
                            || self
                                .node_map
                                .nodes
                                .get(&request.command)
                                .is_some_and(is_hidden_genicam_node)
                        {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                format!(
                                    "GenICam command node {} is a hidden maintenance command",
                                    request.command
                                ),
                            ));
                        }
                        self.validate_command_node(&request.command)?;
                        physical_transactions.push(PhysicalTransaction {
                            resource: Some(self.resource),
                            description: format!(
                                "genicam execute command node {}",
                                request.command
                            ),
                            payload: Value::String(request.command.clone()),
                        });
                    } else {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "unknown GenICam capability",
                        ));
                    }
                }
                _ => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "GenICam node-map support accepts property and command-node commands",
                    ))
                }
            }
        }
        Ok(PreparedBatch {
            id: batch.id,
            commands: batch.commands.clone(),
            physical_transactions,
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
                resource: Some(self.resource),
                description: "genicam timing arm summary".into(),
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
                resource: Some(self.resource),
                description: "genicam timing start summary".into(),
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
                description: "genicam timing stop summary".into(),
                payload: self.timing_summary(&armed.plan, "stop", applied),
            }],
        })
    }

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.token();
        let mut result = Value::Null;
        for command in prepared.commands {
            result = match command {
                Command::ReadProperty { device, key } => self.read_property(device, &key)?,
                Command::WriteProperty { device, key, value } => {
                    self.write_property(device, &key, value)?
                }
                Command::ApplyStateSet(set) => self.apply_state_set(set)?,
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if capability == self.capture_capability_id() => {
                    self.invoke_camera_capture(device, request, token)?
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if capability == self.stream_capability_id() => {
                    self.invoke_camera_stream(device, request, token)?
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if capability == self.raw_register_capability_id() => {
                    if device != self.camera {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "unknown GenICam device",
                        ));
                    }
                    let request = self.parse_raw_register_request(&request)?;
                    self.invoke_raw_register(request)?
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if capability == self.trigger_sink_capability_id() => {
                    if device != self.camera {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "unknown GenICam device",
                        ));
                    }
                    let action =
                        parse_genicam_trigger_action(&request, &CapabilityKind::TriggerSink)?;
                    self.invoke_trigger(CapabilityKind::TriggerSink, action)?
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if capability == self.trigger_source_capability_id() => {
                    if device != self.camera {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "unknown GenICam device",
                        ));
                    }
                    let action =
                        parse_genicam_trigger_action(&request, &CapabilityKind::TriggerSource)?;
                    self.invoke_trigger(CapabilityKind::TriggerSource, action)?
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => self.invoke_command(device, capability, request)?,
                _ => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "unsupported GenICam command",
                    ))
                }
            };
        }
        self.pending.push_back(DriverEvent::TokenCompleted {
            token,
            value: result,
        });
        Ok(token)
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        self.pending.drain(..).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GenicamTriggerAction {
    Enable,
    Disable,
    Pulse,
}

impl GenicamTriggerAction {
    fn name(self) -> &'static str {
        match self {
            Self::Enable => "enable",
            Self::Disable => "disable",
            Self::Pulse => "pulse",
        }
    }

    fn commands(self) -> Vec<&'static str> {
        match self {
            Self::Enable => vec!["AcquisitionStart"],
            Self::Disable => vec!["AcquisitionStop"],
            Self::Pulse => vec!["AcquisitionStart", "AcquisitionStop"],
        }
    }

    fn command_label(self) -> &'static str {
        match self {
            Self::Enable => "AcquisitionStart",
            Self::Disable => "AcquisitionStop",
            Self::Pulse => "AcquisitionStart/AcquisitionStop",
        }
    }
}

#[derive(Debug, Clone)]
enum GenicamRawRegisterRequest {
    Read {
        target: GenicamRawRegisterTarget,
        byte_count: Option<usize>,
    },
    Write {
        target: GenicamRawRegisterTarget,
        bytes: Option<Vec<u8>>,
        value: Option<Value>,
    },
}

impl GenicamRawRegisterRequest {
    fn payload(&self) -> Value {
        let mut payload = BTreeMap::new();
        match self {
            Self::Read { target, byte_count } => {
                payload.insert("operation".into(), Value::String("read".into()));
                payload.insert("register".into(), Value::String(target.key.clone()));
                if let Some(node) = &target.node {
                    payload.insert("node".into(), Value::String(node.clone()));
                }
                if let Some(byte_count) = byte_count {
                    payload.insert("byte_count".into(), Value::I64(*byte_count as i64));
                }
            }
            Self::Write {
                target,
                bytes,
                value,
            } => {
                payload.insert("operation".into(), Value::String("write".into()));
                payload.insert("register".into(), Value::String(target.key.clone()));
                if let Some(node) = &target.node {
                    payload.insert("node".into(), Value::String(node.clone()));
                }
                if let Some(bytes) = bytes {
                    payload.insert("bytes".into(), Value::Bytes(bytes.clone()));
                }
                if let Some(value) = value {
                    payload.insert("value".into(), value.clone());
                }
            }
        }
        Value::Map(payload)
    }
}

#[derive(Debug, Clone)]
struct GenicamRawRegisterTarget {
    key: String,
    node: Option<String>,
    length: Option<usize>,
    little_endian: bool,
}

impl GenicamRawRegisterTarget {
    fn label(&self) -> String {
        if let Some(node) = &self.node {
            format!("{node} ({})", self.key)
        } else {
            self.key.clone()
        }
    }
}

fn node_schema(node: &GenicamNode) -> Option<PropertySchema> {
    if is_hidden_genicam_node(node) {
        return None;
    }
    let value_type = match node.kind {
        GenicamNodeKind::Integer => ValueType::I64,
        GenicamNodeKind::Float => ValueType::F64,
        GenicamNodeKind::Boolean => ValueType::Bool,
        GenicamNodeKind::Enumeration | GenicamNodeKind::String => ValueType::String,
        GenicamNodeKind::IntSwissKnife => ValueType::I64,
        GenicamNodeKind::SwissKnife | GenicamNodeKind::Converter => ValueType::F64,
        GenicamNodeKind::Command => return None,
    };
    let access = effective_access(node);
    Some(PropertySchema {
        key: node.name.clone(),
        display_name: node.display_name.clone(),
        value_type,
        unit: node.unit.clone().map(Unit),
        range: match (&node.min, &node.max) {
            (Some(min), Some(max)) => Some(Range {
                min: min.clone(),
                max: max.clone(),
            }),
            _ => None,
        },
        increment: node.increment.clone(),
        enum_values: node
            .enum_values
            .iter()
            .map(|entry| EnumValue {
                value: Value::String(entry.symbol.clone()),
                label: entry.display_name.clone(),
            })
            .collect(),
        readable: access.readable(),
        writable: access.writable(),
        volatile: is_volatile_node(node),
        sequenceable: matches!(
            node.name.as_str(),
            "ExposureTime" | "Gain" | "AcquisitionFrameRate"
        ),
        hardware_address: node.address.clone(),
    })
}

fn is_hidden_genicam_node(node: &GenicamNode) -> bool {
    is_hidden_genicam_command(&node.name) || is_hidden_genicam_command(&node.display_name)
}

fn is_hidden_genicam_register(name: &str, register: &GenicamRegister) -> bool {
    is_hidden_genicam_command(name) || is_hidden_genicam_command(&register.display_name)
}

fn is_hidden_genicam_command(command: &str) -> bool {
    generic_command_is_hidden_maintenance(command)
}

fn category_metadata(node_map: &GenicamNodeMap) -> Value {
    Value::Map(
        node_map
            .categories
            .iter()
            .map(|(category, metadata)| {
                (
                    category.clone(),
                    Value::List(
                        metadata
                            .features
                            .iter()
                            .filter(|feature| {
                                !node_map
                                    .nodes
                                    .get(*feature)
                                    .is_some_and(is_hidden_genicam_node)
                            })
                            .cloned()
                            .map(Value::String)
                            .collect(),
                    ),
                )
            })
            .collect(),
    )
}

fn category_detail_metadata(node_map: &GenicamNodeMap) -> Value {
    let order_index = node_map
        .category_order
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), index as i64))
        .collect::<BTreeMap<_, _>>();
    Value::Map(
        node_map
            .categories
            .iter()
            .map(|(name, category)| {
                let mut metadata = BTreeMap::from([
                    (
                        "display_name".into(),
                        Value::String(category.display_name.clone()),
                    ),
                    (
                        "features".into(),
                        Value::List(
                            category
                                .features
                                .iter()
                                .filter(|feature| {
                                    !node_map
                                        .nodes
                                        .get(*feature)
                                        .is_some_and(is_hidden_genicam_node)
                                })
                                .cloned()
                                .map(Value::String)
                                .collect(),
                        ),
                    ),
                ]);
                if let Some(visibility) = &category.visibility {
                    metadata.insert("visibility".into(), Value::String(visibility.clone()));
                }
                if let Some(index) = order_index.get(name) {
                    metadata.insert("order_index".into(), Value::I64(*index));
                }
                (name.clone(), Value::Map(metadata))
            })
            .collect(),
    )
}

fn root_category_order(node_map: &GenicamNodeMap) -> Vec<String> {
    let referenced_categories = node_map
        .categories
        .values()
        .flat_map(|category| category.features.iter())
        .filter(|feature| node_map.categories.contains_key(feature.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let mut roots = node_map
        .category_order
        .iter()
        .filter(|category| node_map.categories.contains_key(category.as_str()))
        .filter(|category| !referenced_categories.contains(category))
        .cloned()
        .collect::<Vec<_>>();
    if roots.is_empty() {
        roots.extend(node_map.category_order.iter().cloned());
    }
    for category in node_map.categories.keys() {
        if !roots.contains(category) && !referenced_categories.contains(category) {
            roots.push(category.clone());
        }
    }
    roots.dedup();
    roots
}

fn category_tree_metadata(node_map: &GenicamNodeMap) -> Value {
    Value::Map(
        root_category_order(node_map)
            .into_iter()
            .map(|root| {
                let mut stack = Vec::new();
                let tree = category_tree_node(node_map, &root, &mut stack);
                (root, tree)
            })
            .collect(),
    )
}

fn category_tree_node(node_map: &GenicamNodeMap, category: &str, stack: &mut Vec<String>) -> Value {
    if stack.iter().any(|seen| seen == category) {
        return Value::Map(BTreeMap::from([
            ("cycle".into(), Value::Bool(true)),
            ("features".into(), Value::List(Vec::new())),
            ("categories".into(), Value::Map(BTreeMap::new())),
        ]));
    }

    stack.push(category.into());
    let category_metadata = node_map.categories.get(category).cloned();
    let features = category_metadata
        .as_ref()
        .map(|category| category.features.clone())
        .unwrap_or_default();
    let mut leaf_features = Vec::new();
    let mut categories = BTreeMap::new();
    let mut category_order = Vec::new();
    for feature in features {
        if node_map
            .nodes
            .get(&feature)
            .is_some_and(is_hidden_genicam_node)
        {
            continue;
        }
        if node_map.categories.contains_key(&feature) {
            category_order.push(Value::String(feature.clone()));
            categories.insert(
                feature.clone(),
                category_tree_node(node_map, &feature, stack),
            );
        } else {
            leaf_features.push(Value::String(feature));
        }
    }
    stack.pop();

    let mut metadata = BTreeMap::from([
        ("features".into(), Value::List(leaf_features)),
        ("category_order".into(), Value::List(category_order)),
        ("categories".into(), Value::Map(categories)),
    ]);
    if let Some(category_metadata) = category_metadata {
        metadata.insert(
            "display_name".into(),
            Value::String(category_metadata.display_name),
        );
        if let Some(visibility) = category_metadata.visibility {
            metadata.insert("visibility".into(), Value::String(visibility));
        }
    }
    Value::Map(metadata)
}

fn port_metadata(node_map: &GenicamNodeMap) -> Value {
    Value::Map(
        node_map
            .ports
            .iter()
            .map(|(name, port)| {
                (
                    name.clone(),
                    Value::Map(BTreeMap::from([
                        (
                            "display_name".into(),
                            Value::String(port.display_name.clone()),
                        ),
                        (
                            "access".into(),
                            Value::String(access_metadata(&port.access).into()),
                        ),
                    ])),
                )
            })
            .collect(),
    )
}

fn register_metadata(node_map: &GenicamNodeMap) -> Value {
    Value::Map(
        node_map
            .registers
            .iter()
            .filter(|(name, register)| !is_hidden_genicam_register(name, register))
            .map(|(name, register)| {
                let mut metadata = BTreeMap::from([
                    (
                        "display_name".into(),
                        Value::String(register.display_name.clone()),
                    ),
                    (
                        "access".into(),
                        Value::String(access_metadata(&register.access).into()),
                    ),
                ]);
                insert_opt_string(&mut metadata, "port_ref", &register.port_ref);
                insert_opt_string(&mut metadata, "address", &register.address);
                insert_opt_string(&mut metadata, "address_ref", &register.address_ref);
                insert_opt_string(&mut metadata, "length", &register.length);
                insert_opt_string(&mut metadata, "length_ref", &register.length_ref);
                insert_opt_string(&mut metadata, "endian", &register.endian);
                insert_opt_string(&mut metadata, "category", &register.category);
                insert_opt_string(&mut metadata, "visibility", &register.visibility);
                (name.clone(), Value::Map(metadata))
            })
            .collect(),
    )
}

fn node_metadata(node_map: &GenicamNodeMap) -> Value {
    Value::Map(
        node_map
            .nodes
            .iter()
            .filter(|(_, node)| !is_hidden_genicam_node(node))
            .map(|(name, node)| {
                let mut metadata = BTreeMap::from([(
                    "access".into(),
                    Value::String(access_metadata(&node.access).into()),
                )]);
                if let Some(imposed_access) = &node.imposed_access {
                    metadata.insert(
                        "imposed_access".into(),
                        Value::String(access_metadata(imposed_access).into()),
                    );
                }
                let effective = effective_access(node);
                if effective != &node.access {
                    metadata.insert(
                        "effective_access".into(),
                        Value::String(access_metadata(effective).into()),
                    );
                }
                insert_opt_string(&mut metadata, "tooltip", &node.tooltip);
                insert_opt_string(&mut metadata, "description", &node.description);
                insert_opt_string(&mut metadata, "doc_url", &node.doc_url);
                if let Some(increment) = &node.increment {
                    metadata.insert("increment".into(), increment.clone());
                }
                insert_opt_string(&mut metadata, "min_ref", &node.min_ref);
                insert_opt_string(&mut metadata, "max_ref", &node.max_ref);
                insert_opt_string(&mut metadata, "increment_ref", &node.increment_ref);
                if let Some(bit) = node.bit {
                    metadata.insert("bit".into(), Value::I64(bit as i64));
                }
                if let Some(lsb) = node.lsb {
                    metadata.insert("lsb".into(), Value::I64(lsb as i64));
                }
                if let Some(msb) = node.msb {
                    metadata.insert("msb".into(), Value::I64(msb as i64));
                }
                insert_opt_string(&mut metadata, "sign", &node.sign);
                if let Some(representation) = &node.representation {
                    metadata.insert(
                        "representation".into(),
                        Value::String(representation.clone()),
                    );
                }
                if let Some(visibility) = &node.visibility {
                    metadata.insert("visibility".into(), Value::String(visibility.clone()));
                }
                if let Some(polling_time_ms) = node.polling_time_ms {
                    metadata.insert(
                        "polling_time".into(),
                        Value::TimeInterval(TimeInterval::from_milliseconds(
                            polling_time_ms as f64,
                        )),
                    );
                }
                if let Some(streamable) = node.streamable {
                    metadata.insert("streamable".into(), Value::Bool(streamable));
                }
                if let Some(category) = &node.category {
                    metadata.insert("category".into(), Value::String(category.clone()));
                }
                if !node.selects.is_empty() {
                    metadata.insert(
                        "selects".into(),
                        Value::List(node.selects.iter().cloned().map(Value::String).collect()),
                    );
                }
                if !node.selected_by.is_empty() {
                    metadata.insert(
                        "selected_by".into(),
                        Value::List(
                            node.selected_by
                                .iter()
                                .cloned()
                                .map(Value::String)
                                .collect(),
                        ),
                    );
                }
                if let Some(value_ref) = &node.value_ref {
                    metadata.insert("value_ref".into(), Value::String(value_ref.clone()));
                }
                if let Some(value_copy_ref) = &node.value_copy_ref {
                    metadata.insert(
                        "value_copy_ref".into(),
                        Value::String(value_copy_ref.clone()),
                    );
                }
                insert_opt_string(&mut metadata, "available_ref", &node.available_ref);
                insert_opt_string(&mut metadata, "implemented_ref", &node.implemented_ref);
                insert_opt_string(&mut metadata, "locked_ref", &node.locked_ref);
                insert_opt_string(&mut metadata, "formula", &node.formula);
                insert_opt_string(&mut metadata, "formula_to", &node.formula_to);
                insert_opt_string(&mut metadata, "formula_from", &node.formula_from);
                if !node.variables.is_empty() {
                    metadata.insert(
                        "variables".into(),
                        Value::List(
                            node.variables
                                .iter()
                                .map(|variable| {
                                    Value::Map(BTreeMap::from([
                                        ("name".into(), Value::String(variable.name.clone())),
                                        (
                                            "node_ref".into(),
                                            Value::String(variable.node_ref.clone()),
                                        ),
                                    ]))
                                })
                                .collect(),
                        ),
                    );
                }
                insert_opt_string(&mut metadata, "port_ref", &node.port_ref);
                insert_opt_string(&mut metadata, "address_ref", &node.address_ref);
                insert_opt_string(&mut metadata, "length", &node.length);
                insert_opt_string(&mut metadata, "length_ref", &node.length_ref);
                insert_opt_string(&mut metadata, "struct_ref", &node.struct_ref);
                insert_opt_string(&mut metadata, "offset", &node.offset);
                insert_opt_string(&mut metadata, "endian", &node.endian);
                if let Some(cache_mode) = &node.cache_mode {
                    metadata.insert("cache_mode".into(), Value::String(cache_mode.clone()));
                }
                if !node.invalidated_by.is_empty() {
                    metadata.insert(
                        "invalidated_by".into(),
                        Value::List(
                            node.invalidated_by
                                .iter()
                                .cloned()
                                .map(Value::String)
                                .collect(),
                        ),
                    );
                }
                if !node.invalidates.is_empty() {
                    metadata.insert(
                        "invalidates".into(),
                        Value::List(
                            node.invalidates
                                .iter()
                                .cloned()
                                .map(Value::String)
                                .collect(),
                        ),
                    );
                }
                insert_opt_string(&mut metadata, "event_id", &node.event_id);
                insert_opt_string(
                    &mut metadata,
                    "event_timestamp_ref",
                    &node.event_timestamp_ref,
                );
                insert_opt_string(
                    &mut metadata,
                    "event_notification_ref",
                    &node.event_notification_ref,
                );
                if let Some(command_value) = node.command_value {
                    metadata.insert("command_value".into(), Value::I64(command_value));
                }
                if !node.enum_values.is_empty() {
                    metadata.insert("enum_entries".into(), enum_entry_metadata(node));
                }
                (name.clone(), Value::Map(metadata))
            })
            .collect(),
    )
}

fn enum_entry_metadata(node: &GenicamNode) -> Value {
    Value::Map(
        node.enum_values
            .iter()
            .map(|entry| {
                let mut metadata = BTreeMap::from([(
                    "display_name".into(),
                    Value::String(entry.display_name.clone()),
                )]);
                if let Some(value) = entry.value {
                    metadata.insert("value".into(), Value::I64(value));
                }
                insert_opt_string(&mut metadata, "available_ref", &entry.available_ref);
                insert_opt_string(&mut metadata, "implemented_ref", &entry.implemented_ref);
                (entry.symbol.clone(), Value::Map(metadata))
            })
            .collect(),
    )
}

fn insert_opt_string(metadata: &mut BTreeMap<String, Value>, key: &str, value: &Option<String>) {
    if let Some(value) = value {
        metadata.insert(key.into(), Value::String(value.clone()));
    }
}

fn access_metadata(access: &GenicamAccess) -> &'static str {
    match access {
        GenicamAccess::ReadOnly => "RO",
        GenicamAccess::WriteOnly => "WO",
        GenicamAccess::ReadWrite => "RW",
        GenicamAccess::NotAvailable => "NA",
        GenicamAccess::NotImplemented => "NI",
    }
}

fn effective_access(node: &GenicamNode) -> &GenicamAccess {
    node.imposed_access.as_ref().unwrap_or(&node.access)
}

fn is_volatile_node(node: &GenicamNode) -> bool {
    matches!(
        node.cache_mode.as_deref(),
        Some("NoCache") | Some("NoCacheable") | Some("false") | Some("False") | Some("FALSE")
    )
}

fn is_formula_node(node: &GenicamNode) -> bool {
    matches!(
        node.kind,
        GenicamNodeKind::IntSwissKnife | GenicamNodeKind::SwissKnife | GenicamNodeKind::Converter
    )
}

fn converter_write_target(node: &GenicamNode) -> Option<&str> {
    node.variables
        .iter()
        .find(|variable| variable.name == "TO")
        .or_else(|| {
            let mut unique = node
                .variables
                .iter()
                .map(|variable| variable.node_ref.as_str())
                .collect::<Vec<_>>();
            unique.sort_unstable();
            unique.dedup();
            if unique.len() == 1 {
                node.variables.first()
            } else {
                None
            }
        })
        .map(|variable| variable.node_ref.as_str())
}

fn numeric_value(value: &Value, key: &str) -> Result<f64> {
    match value {
        Value::I64(value) => Ok(*value as f64),
        Value::F64(value) => Ok(*value),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("GenICam formula variable {key} is not numeric"),
        )),
    }
}

fn value_i64(value: &Value) -> Result<i64> {
    match value {
        Value::I64(value) => Ok(*value),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            "expected integer value",
        )),
    }
}

fn value_usize(value: &Value) -> Result<usize> {
    match value {
        Value::I64(value) if *value >= 0 => Ok(*value as usize),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("expected non-negative byte count, got {value:?}"),
        )),
    }
}

fn value_string(value: &Value) -> Result<String> {
    match value {
        Value::String(value) => Ok(value.clone()),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("expected string value, got {value:?}"),
        )),
    }
}

fn value_bytes(value: &Value) -> Result<Vec<u8>> {
    match value {
        Value::Bytes(bytes) => Ok(bytes.clone()),
        Value::List(values) => values
            .iter()
            .map(|value| match value {
                Value::I64(value) if *value >= 0 && *value <= u8::MAX as i64 => Ok(*value as u8),
                _ => Err(Error::new(
                    ErrorCode::InvalidCommand,
                    format!("expected byte value, got {value:?}"),
                )),
            })
            .collect(),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("expected byte list, got {value:?}"),
        )),
    }
}

fn timestamp(ticks: i64) -> Value {
    Value::Timestamp(Timestamp::from_controller_ticks(ticks))
}

fn parse_genicam_trigger_action(
    request: &CapabilityRequest,
    kind: &CapabilityKind,
) -> Result<GenicamTriggerAction> {
    match request {
        CapabilityRequest::None => Ok(GenicamTriggerAction::Pulse),
        CapabilityRequest::Trigger(request) => match request.action {
            TriggerAction::Enable => Ok(GenicamTriggerAction::Enable),
            TriggerAction::Disable => Ok(GenicamTriggerAction::Disable),
            TriggerAction::Pulse => Ok(GenicamTriggerAction::Pulse),
        },
        _ => Err(Error::new(
            ErrorCode::Unsupported,
            format!("{} expects None or CapabilityRequest::Trigger", kind.name()),
        )),
    }
}

fn pixel_format_name(encoding: &ImageEncoding, native: &str) -> &'static str {
    match encoding {
        ImageEncoding::Native => match native {
            "Mono16" => "Mono16",
            "RGB8" | "Rgb8" => "Rgb8",
            "BGR8" | "Bgr8" => "Bgr8",
            "Raw8" => "Raw8",
            "Raw16" => "Raw16",
            _ => "Mono8",
        },
        ImageEncoding::Mono8 => "Mono8",
        ImageEncoding::Mono16 => "Mono16",
        ImageEncoding::Rgb8 => "Rgb8",
        ImageEncoding::Bgr8 => "Bgr8",
        ImageEncoding::Raw8 => "Raw8",
        ImageEncoding::Raw16 => "Raw16",
    }
}

fn synthetic_frame_data(
    width: u32,
    height: u32,
    pixel_format: &str,
    exposure_s: f64,
    index: Option<u64>,
) -> Vec<u8> {
    let exposure_scale = (exposure_s * 1000.0).clamp(1.0, 255.0) as u32;
    let frame_offset = index.unwrap_or_default() as u32;
    match pixel_format {
        "Mono16" | "MONO16" | "Raw16" | "RAW16" => {
            let mut data = Vec::with_capacity(width as usize * height as usize * 2);
            for y in 0..height {
                for x in 0..width {
                    let value =
                        (((x + y + frame_offset) * exposure_scale) % u16::MAX as u32) as u16;
                    data.extend_from_slice(&value.to_le_bytes());
                }
            }
            data
        }
        "Rgb8" | "RGB8" | "Bgr8" | "BGR8" => {
            let mut data = Vec::with_capacity(width as usize * height as usize * 3);
            for y in 0..height {
                for x in 0..width {
                    let a = ((x + frame_offset) * exposure_scale % 256) as u8;
                    let b = ((y + frame_offset) * exposure_scale % 256) as u8;
                    let c = (((x + y) / 2 + frame_offset) * exposure_scale % 256) as u8;
                    if matches!(pixel_format, "Bgr8" | "BGR8") {
                        data.extend_from_slice(&[c, b, a]);
                    } else {
                        data.extend_from_slice(&[a, b, c]);
                    }
                }
            }
            data
        }
        _ => {
            let mut data = Vec::with_capacity(width as usize * height as usize);
            for y in 0..height {
                for x in 0..width {
                    data.push(((x + y + frame_offset) * exposure_scale % 256) as u8);
                }
            }
            data
        }
    }
}

fn eval_formula(formula: &str, variables: &BTreeMap<String, f64>) -> Result<f64> {
    let mut parser = FormulaParser {
        input: formula.as_bytes(),
        offset: 0,
        variables,
    };
    let value = parser.expression()?;
    parser.skip_ws();
    if parser.offset != parser.input.len() {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            format!(
                "unsupported GenICam formula syntax near {}",
                parser.remaining()
            ),
        ));
    }
    Ok(value)
}

struct FormulaParser<'a> {
    input: &'a [u8],
    offset: usize,
    variables: &'a BTreeMap<String, f64>,
}

impl FormulaParser<'_> {
    fn expression(&mut self) -> Result<f64> {
        self.logical_or()
    }

    fn logical_or(&mut self) -> Result<f64> {
        let mut value = self.logical_and()?;
        loop {
            self.skip_ws();
            if self.consume_str("||") {
                value = bool_value(truthy(value) || truthy(self.logical_and()?));
            } else {
                return Ok(value);
            }
        }
    }

    fn logical_and(&mut self) -> Result<f64> {
        let mut value = self.bitwise_or()?;
        loop {
            self.skip_ws();
            if self.consume_str("&&") {
                value = bool_value(truthy(value) && truthy(self.bitwise_or()?));
            } else {
                return Ok(value);
            }
        }
    }

    fn bitwise_or(&mut self) -> Result<f64> {
        let mut value = self.bitwise_xor()?;
        loop {
            self.skip_ws();
            if self.starts_with("||") {
                return Ok(value);
            } else if self.consume(b'|') {
                value = bitwise_value(value, self.bitwise_xor()?, |left, right| left | right);
            } else {
                return Ok(value);
            }
        }
    }

    fn bitwise_xor(&mut self) -> Result<f64> {
        let mut value = self.bitwise_and()?;
        loop {
            self.skip_ws();
            if self.consume(b'^') {
                value = bitwise_value(value, self.bitwise_and()?, |left, right| left ^ right);
            } else {
                return Ok(value);
            }
        }
    }

    fn bitwise_and(&mut self) -> Result<f64> {
        let mut value = self.equality()?;
        loop {
            self.skip_ws();
            if self.starts_with("&&") {
                return Ok(value);
            } else if self.consume(b'&') {
                value = bitwise_value(value, self.equality()?, |left, right| left & right);
            } else {
                return Ok(value);
            }
        }
    }

    fn equality(&mut self) -> Result<f64> {
        let mut value = self.comparison()?;
        loop {
            self.skip_ws();
            if self.consume_str("==") {
                value = bool_value((value - self.comparison()?).abs() <= f64::EPSILON);
            } else if self.consume_str("!=") {
                value = bool_value((value - self.comparison()?).abs() > f64::EPSILON);
            } else {
                return Ok(value);
            }
        }
    }

    fn comparison(&mut self) -> Result<f64> {
        let mut value = self.shift()?;
        loop {
            self.skip_ws();
            if self.consume_str("<=") {
                value = bool_value(value <= self.shift()?);
            } else if self.consume_str(">=") {
                value = bool_value(value >= self.shift()?);
            } else if self.consume(b'<') {
                value = bool_value(value < self.shift()?);
            } else if self.consume(b'>') {
                value = bool_value(value > self.shift()?);
            } else {
                return Ok(value);
            }
        }
    }

    fn shift(&mut self) -> Result<f64> {
        let mut value = self.sum()?;
        loop {
            self.skip_ws();
            if self.consume_str("<<") {
                value = bitwise_value(value, self.sum()?, |left, right| {
                    left.wrapping_shl((right as u32).min(63))
                });
            } else if self.consume_str(">>") {
                value = bitwise_value(value, self.sum()?, |left, right| {
                    left.wrapping_shr((right as u32).min(63))
                });
            } else {
                return Ok(value);
            }
        }
    }

    fn sum(&mut self) -> Result<f64> {
        let mut value = self.term()?;
        loop {
            self.skip_ws();
            if self.consume(b'+') {
                value += self.term()?;
            } else if self.consume(b'-') {
                value -= self.term()?;
            } else {
                return Ok(value);
            }
        }
    }

    fn term(&mut self) -> Result<f64> {
        let mut value = self.factor()?;
        loop {
            self.skip_ws();
            if self.consume(b'*') {
                value *= self.factor()?;
            } else if self.consume(b'/') {
                value /= self.factor()?;
            } else if self.consume(b'%') {
                value %= self.factor()?;
            } else {
                return Ok(value);
            }
        }
    }

    fn factor(&mut self) -> Result<f64> {
        self.skip_ws();
        if self.consume(b'+') {
            return self.factor();
        }
        if self.consume(b'-') {
            return Ok(-self.factor()?);
        }
        if self.consume(b'!') {
            return Ok(bool_value(!truthy(self.factor()?)));
        }
        if self.consume(b'~') {
            return Ok((!(self.factor()? as i64)) as f64);
        }
        if self.consume(b'(') {
            let value = self.expression()?;
            self.skip_ws();
            if !self.consume(b')') {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "unterminated GenICam formula parenthesis",
                ));
            }
            return Ok(value);
        }
        if self
            .peek()
            .is_some_and(|byte| byte.is_ascii_digit() || byte == b'.')
        {
            return self.number();
        }
        self.identifier_or_function()
    }

    fn number(&mut self) -> Result<f64> {
        let start = self.offset;
        while self.peek().is_some_and(|byte| {
            byte.is_ascii_digit() || matches!(byte, b'.' | b'e' | b'E' | b'+' | b'-')
        }) {
            let byte = self.peek().expect("peeked byte exists");
            if matches!(byte, b'+' | b'-') && self.offset > start {
                let previous = self.input[self.offset - 1];
                if !matches!(previous, b'e' | b'E') {
                    break;
                }
            }
            self.offset += 1;
        }
        let raw = std::str::from_utf8(&self.input[start..self.offset]).map_err(|error| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid formula utf8: {error}"),
            )
        })?;
        raw.parse::<f64>().map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid GenICam formula number {raw}"),
            )
        })
    }

    fn identifier_or_function(&mut self) -> Result<f64> {
        let start = self.offset;
        while self
            .peek()
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.'))
        {
            self.offset += 1;
        }
        if self.offset == start {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("expected GenICam formula value near {}", self.remaining()),
            ));
        }
        let name = std::str::from_utf8(&self.input[start..self.offset]).map_err(|error| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid formula utf8: {error}"),
            )
        })?;
        self.skip_ws();
        if self.consume(b'(') {
            return self.function(name);
        }
        self.variables.get(name).copied().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown GenICam formula variable {name}"),
            )
        })
    }

    fn function(&mut self, name: &str) -> Result<f64> {
        let mut args = Vec::new();
        self.skip_ws();
        if !self.consume(b')') {
            loop {
                args.push(self.expression()?);
                self.skip_ws();
                if self.consume(b')') {
                    break;
                }
                if !self.consume(b',') {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        format!("expected comma or ')' in GenICam function {name}"),
                    ));
                }
            }
        }
        apply_formula_function(name, &args)
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.offset += 1;
        }
    }

    fn consume(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.offset += 1;
            true
        } else {
            false
        }
    }

    fn consume_str(&mut self, text: &str) -> bool {
        if self.starts_with(text) {
            self.offset += text.len();
            true
        } else {
            false
        }
    }

    fn starts_with(&self, text: &str) -> bool {
        self.input[self.offset..].starts_with(text.as_bytes())
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.offset).copied()
    }

    fn remaining(&self) -> String {
        String::from_utf8_lossy(&self.input[self.offset..]).into()
    }
}

fn bool_value(value: bool) -> f64 {
    if value {
        1.0
    } else {
        0.0
    }
}

fn truthy(value: f64) -> bool {
    value != 0.0
}

fn bitwise_value(left: f64, right: f64, op: impl FnOnce(i64, i64) -> i64) -> f64 {
    op(left as i64, right as i64) as f64
}

fn apply_formula_function(name: &str, args: &[f64]) -> Result<f64> {
    match (name.to_ascii_uppercase().as_str(), args.len()) {
        ("IF", 3) => Ok(if truthy(args[0]) { args[1] } else { args[2] }),
        ("MIN", 2) => Ok(args[0].min(args[1])),
        ("MAX", 2) => Ok(args[0].max(args[1])),
        ("ABS", 1) => Ok(args[0].abs()),
        ("FLOOR", 1) => Ok(args[0].floor()),
        ("CEIL", 1) => Ok(args[0].ceil()),
        ("ROUND", 1) => Ok(args[0].round()),
        ("TRUNC", 1) => Ok(args[0].trunc()),
        ("MOD", 2) => Ok(args[0] % args[1]),
        ("POW", 2) => Ok(args[0].powf(args[1])),
        ("SQRT", 1) => Ok(args[0].sqrt()),
        ("LOG", 1) | ("LN", 1) => Ok(args[0].ln()),
        ("LOG2", 1) => Ok(args[0].log2()),
        ("LOG10", 1) => Ok(args[0].log10()),
        ("EXP", 1) => Ok(args[0].exp()),
        ("SIN", 1) => Ok(args[0].sin()),
        ("COS", 1) => Ok(args[0].cos()),
        ("TAN", 1) => Ok(args[0].tan()),
        ("ASIN", 1) => Ok(args[0].asin()),
        ("ACOS", 1) => Ok(args[0].acos()),
        ("ATAN", 1) => Ok(args[0].atan()),
        ("ATAN2", 2) => Ok(args[0].atan2(args[1])),
        ("SGN", 1) => Ok(args[0].signum()),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!(
                "unsupported GenICam formula function {name} with {} argument(s)",
                args.len()
            ),
        )),
    }
}

#[derive(Debug, Clone)]
struct RegisterLocation {
    key: String,
    length: usize,
    little_endian: bool,
}

fn initial_node_values(node_map: &GenicamNodeMap) -> Result<BTreeMap<String, Value>> {
    node_map
        .nodes
        .iter()
        .map(|(name, node)| {
            let value = if let Some(value_copy_ref) = &node.value_copy_ref {
                node_map
                    .nodes
                    .get(value_copy_ref)
                    .map(|source| source.value.clone())
                    .ok_or_else(|| {
                        Error::new(
                            ErrorCode::InvalidProperty,
                            format!(
                                "GenICam node {name} copies unknown value node {value_copy_ref}"
                            ),
                        )
                    })?
            } else {
                node.value.clone()
            };
            Ok((name.clone(), value))
        })
        .collect()
}

fn initial_register_values(node_map: &GenicamNodeMap) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut values = BTreeMap::new();
    for node in node_map.nodes.values() {
        let Some(location) = initial_register_location(node_map, node)? else {
            continue;
        };
        if let Ok(bytes) =
            encode_register_value(node, &node.value, location.length, location.little_endian)
        {
            values.insert(location.key, bytes);
        }
    }
    Ok(values)
}

fn initial_register_location(
    node_map: &GenicamNodeMap,
    node: &GenicamNode,
) -> Result<Option<RegisterLocation>> {
    if let Some(struct_ref) = &node.struct_ref {
        let Some(register) = node_map.registers.get(struct_ref) else {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown GenICam StructReg {struct_ref}"),
            ));
        };
        let (Some(port), Some(base_address), Some(offset), Some(length)) = (
            register.port_ref.as_ref(),
            register_address_from_metadata(register)?,
            node.offset.as_ref(),
            initial_register_length(node_map, node)?,
        ) else {
            return Ok(None);
        };
        let address = base_address + parse_register_address(offset)?;
        return Ok(Some(RegisterLocation {
            key: register_storage_key(port, address),
            length,
            little_endian: is_little_endian(node)
                && !matches!(
                    register.endian.as_deref(),
                    Some("BigEndian") | Some("BigEndianess")
                ),
        }));
    }
    let Some(port) = &node.port_ref else {
        return Ok(None);
    };
    let Some(address) = initial_register_address(node_map, node)? else {
        return Ok(None);
    };
    let Some(length) = initial_register_length(node_map, node)? else {
        return Ok(None);
    };
    Ok(Some(RegisterLocation {
        key: register_storage_key(port, address),
        length,
        little_endian: is_little_endian(node),
    }))
}

fn initial_register_address(node_map: &GenicamNodeMap, node: &GenicamNode) -> Result<Option<u64>> {
    if let Some(address) = &node.address {
        return parse_register_address(address).map(Some);
    }
    let Some(address_ref) = &node.address_ref else {
        return Ok(None);
    };
    let value = node_map
        .nodes
        .get(address_ref)
        .map(|node| &node.value)
        .ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown GenICam address node {address_ref}"),
            )
        })?;
    parse_register_address_value(address_ref, value).map(Some)
}

fn initial_register_length(node_map: &GenicamNodeMap, node: &GenicamNode) -> Result<Option<usize>> {
    if let Some(length) = &node.length {
        return parse_register_length(length).map(Some);
    }
    let Some(length_ref) = &node.length_ref else {
        return Ok(None);
    };
    let value = node_map
        .nodes
        .get(length_ref)
        .map(|node| &node.value)
        .ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown GenICam length node {length_ref}"),
            )
        })?;
    parse_register_length_value(length_ref, value).map(Some)
}

fn register_storage_key(port: &str, address: u64) -> String {
    format!("{port}@0x{address:x}")
}

fn register_address_from_metadata(register: &GenicamRegister) -> Result<Option<u64>> {
    register
        .address
        .as_ref()
        .map(|address| parse_register_address(address))
        .transpose()
}

fn is_little_endian(node: &GenicamNode) -> bool {
    !matches!(
        node.endian.as_deref(),
        Some("BigEndian") | Some("BigEndianess")
    )
}

fn parse_register_address(address: &str) -> Result<u64> {
    let raw = address
        .trim()
        .strip_prefix("genicam:")
        .unwrap_or(address.trim());
    if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16)
    } else {
        raw.parse::<u64>()
    }
    .map_err(|_| {
        Error::new(
            ErrorCode::InvalidProperty,
            format!("invalid GenICam register address {address}"),
        )
    })
}

fn parse_register_address_value(key: &str, value: &Value) -> Result<u64> {
    match value {
        Value::I64(value) if *value >= 0 => Ok(*value as u64),
        Value::String(value) => parse_register_address(value),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("GenICam register address reference {key} is not an address value"),
        )),
    }
}

fn parse_register_length(length: &str) -> Result<usize> {
    length.trim().parse::<usize>().map_err(|_| {
        Error::new(
            ErrorCode::InvalidProperty,
            format!("invalid GenICam register length {length}"),
        )
    })
}

fn parse_register_length_value(key: &str, value: &Value) -> Result<usize> {
    match value {
        Value::I64(value) if *value > 0 => Ok(*value as usize),
        Value::String(value) => parse_register_length(value),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("GenICam register length reference {key} is not a positive length"),
        )),
    }
}

fn apply_masked_read(node: &GenicamNode, value: Value) -> Result<Value> {
    let Some((shift, mask)) = masked_field(node) else {
        return Ok(value);
    };
    let raw = value_i64(&value)? as u64;
    Ok(Value::I64(((raw & mask) >> shift) as i64))
}

fn masked_field(node: &GenicamNode) -> Option<(u32, u64)> {
    if let Some(bit) = node.bit {
        let shift = bit as u32;
        return Some((shift, 1u64 << shift));
    }
    let (Some(lsb), Some(msb)) = (node.lsb, node.msb) else {
        return None;
    };
    if msb < lsb || msb >= 64 {
        return None;
    }
    let width = (msb - lsb + 1) as u32;
    let field_mask = if width == 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    };
    let shift = lsb as u32;
    Some((shift, field_mask << shift))
}

fn encode_register_value(
    node: &GenicamNode,
    value: &Value,
    length: usize,
    little_endian: bool,
) -> Result<Vec<u8>> {
    match (&node.kind, value) {
        (GenicamNodeKind::Integer, Value::I64(value)) => {
            encode_integer(node, *value, length, little_endian)
        }
        (GenicamNodeKind::Float, Value::F64(value)) => encode_f64(*value, length, little_endian),
        (GenicamNodeKind::Boolean, Value::Bool(value)) => {
            encode_i64(*value as i64, length, little_endian)
        }
        (GenicamNodeKind::String, Value::String(value)) => encode_string(value, length),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!(
                "GenICam node {} cannot be encoded as register bytes",
                node.name
            ),
        )),
    }
}

fn decode_register_value(node: &GenicamNode, bytes: &[u8], little_endian: bool) -> Result<Value> {
    match node.kind {
        GenicamNodeKind::Integer => Ok(Value::I64(decode_integer(node, bytes, little_endian)?)),
        GenicamNodeKind::Float => Ok(Value::F64(decode_f64(bytes, little_endian)?)),
        GenicamNodeKind::Boolean => Ok(Value::Bool(decode_i64(bytes, little_endian)? != 0)),
        GenicamNodeKind::String => Ok(Value::String(decode_string(bytes))),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!(
                "GenICam node {} cannot be decoded from register bytes",
                node.name
            ),
        )),
    }
}

fn encode_string(value: &str, length: usize) -> Result<Vec<u8>> {
    let bytes = value.as_bytes();
    if bytes.len() > length {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            format!(
                "GenICam string register value is {} byte(s), exceeds register length {length}",
                bytes.len()
            ),
        ));
    }
    let mut encoded = vec![0; length];
    encoded[..bytes.len()].copy_from_slice(bytes);
    Ok(encoded)
}

fn decode_string(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn encode_integer(
    node: &GenicamNode,
    value: i64,
    length: usize,
    little_endian: bool,
) -> Result<Vec<u8>> {
    if is_unsigned_integer(node) {
        if value < 0 {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("GenICam unsigned node {} cannot encode {value}", node.name),
            ));
        }
        encode_u64(value as u64, length, little_endian)
    } else {
        encode_i64(value, length, little_endian)
    }
}

fn decode_integer(node: &GenicamNode, bytes: &[u8], little_endian: bool) -> Result<i64> {
    if is_unsigned_integer(node) {
        let value = decode_u64(bytes, little_endian)?;
        i64::try_from(value).map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("GenICam unsigned node {} exceeds runtime I64", node.name),
            )
        })
    } else {
        decode_i64(bytes, little_endian)
    }
}

fn is_unsigned_integer(node: &GenicamNode) -> bool {
    matches!(
        node.sign.as_deref(),
        Some("Unsigned") | Some("unsigned") | Some("U")
    )
}

fn encode_i64(value: i64, length: usize, little_endian: bool) -> Result<Vec<u8>> {
    let bytes = if little_endian {
        value.to_le_bytes()
    } else {
        value.to_be_bytes()
    };
    match length {
        1 | 2 | 4 | 8 => {
            if little_endian {
                Ok(bytes[..length].to_vec())
            } else {
                Ok(bytes[8 - length..].to_vec())
            }
        }
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unsupported GenICam integer register length {length}"),
        )),
    }
}

fn encode_u64(value: u64, length: usize, little_endian: bool) -> Result<Vec<u8>> {
    let bytes = if little_endian {
        value.to_le_bytes()
    } else {
        value.to_be_bytes()
    };
    match length {
        1 | 2 | 4 | 8 => {
            if little_endian {
                Ok(bytes[..length].to_vec())
            } else {
                Ok(bytes[8 - length..].to_vec())
            }
        }
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unsupported GenICam unsigned integer register length {length}"),
        )),
    }
}

fn decode_i64(bytes: &[u8], little_endian: bool) -> Result<i64> {
    match bytes.len() {
        1 => Ok(i8::from_ne_bytes([bytes[0]]) as i64),
        2 => {
            let array = [bytes[0], bytes[1]];
            Ok(if little_endian {
                i16::from_le_bytes(array)
            } else {
                i16::from_be_bytes(array)
            } as i64)
        }
        4 => {
            let array = [bytes[0], bytes[1], bytes[2], bytes[3]];
            Ok(if little_endian {
                i32::from_le_bytes(array)
            } else {
                i32::from_be_bytes(array)
            } as i64)
        }
        8 => {
            let array = [
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ];
            Ok(if little_endian {
                i64::from_le_bytes(array)
            } else {
                i64::from_be_bytes(array)
            })
        }
        length => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unsupported GenICam integer register length {length}"),
        )),
    }
}

fn decode_u64(bytes: &[u8], little_endian: bool) -> Result<u64> {
    match bytes.len() {
        1 => Ok(bytes[0] as u64),
        2 => {
            let array = [bytes[0], bytes[1]];
            Ok(if little_endian {
                u16::from_le_bytes(array)
            } else {
                u16::from_be_bytes(array)
            } as u64)
        }
        4 => {
            let array = [bytes[0], bytes[1], bytes[2], bytes[3]];
            Ok(if little_endian {
                u32::from_le_bytes(array)
            } else {
                u32::from_be_bytes(array)
            } as u64)
        }
        8 => {
            let array = [
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ];
            Ok(if little_endian {
                u64::from_le_bytes(array)
            } else {
                u64::from_be_bytes(array)
            })
        }
        length => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unsupported GenICam unsigned integer register length {length}"),
        )),
    }
}

fn encode_f64(value: f64, length: usize, little_endian: bool) -> Result<Vec<u8>> {
    match length {
        4 => {
            let bytes = (value as f32).to_bits();
            Ok(if little_endian {
                bytes.to_le_bytes().to_vec()
            } else {
                bytes.to_be_bytes().to_vec()
            })
        }
        8 => {
            let bytes = value.to_bits();
            Ok(if little_endian {
                bytes.to_le_bytes().to_vec()
            } else {
                bytes.to_be_bytes().to_vec()
            })
        }
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unsupported GenICam float register length {length}"),
        )),
    }
}

fn decode_f64(bytes: &[u8], little_endian: bool) -> Result<f64> {
    match bytes.len() {
        4 => {
            let array = [bytes[0], bytes[1], bytes[2], bytes[3]];
            let bits = if little_endian {
                u32::from_le_bytes(array)
            } else {
                u32::from_be_bytes(array)
            };
            Ok(f32::from_bits(bits) as f64)
        }
        8 => {
            let array = [
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ];
            let bits = if little_endian {
                u64::from_le_bytes(array)
            } else {
                u64::from_be_bytes(array)
            };
            Ok(f64::from_bits(bits))
        }
        length => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unsupported GenICam float register length {length}"),
        )),
    }
}

fn parse_node(name: &str, kind: GenicamNodeKind, body: &str) -> Result<GenicamNode> {
    let enum_values = parse_enum_entries(body);
    let body_without_enum_entries;
    let body = if kind == GenicamNodeKind::Enumeration {
        body_without_enum_entries = strip_element_blocks(body, "EnumEntry");
        body_without_enum_entries.as_str()
    } else {
        body
    };
    let display_name = text(body, "DisplayName").unwrap_or_else(|| name.into());
    let access = if matches!(
        kind,
        GenicamNodeKind::IntSwissKnife | GenicamNodeKind::SwissKnife | GenicamNodeKind::Converter
    ) && text(body, "AccessMode").is_none()
    {
        GenicamAccess::ReadOnly
    } else {
        parse_access(text(body, "AccessMode").as_deref())
    };
    let unit = text(body, "Unit");
    let address = text(body, "Address").map(|value| format!("genicam:{value}"));
    let increment = match kind {
        GenicamNodeKind::Integer => text(body, "Inc")
            .map(|value| parse_i64(Some(&value), 0).map(Value::I64))
            .transpose()?,
        GenicamNodeKind::Float => text(body, "Inc")
            .map(|value| parse_f64(Some(&value), 0.0).map(Value::F64))
            .transpose()?,
        GenicamNodeKind::Boolean
        | GenicamNodeKind::Enumeration
        | GenicamNodeKind::String
        | GenicamNodeKind::IntSwissKnife
        | GenicamNodeKind::SwissKnife
        | GenicamNodeKind::Converter
        | GenicamNodeKind::Command => None,
    };
    let value = match kind {
        GenicamNodeKind::Integer => Value::I64(parse_i64(text(body, "Value").as_deref(), 0)?),
        GenicamNodeKind::Float => Value::F64(parse_f64(text(body, "Value").as_deref(), 0.0)?),
        GenicamNodeKind::Boolean => Value::Bool(parse_bool(text(body, "Value").as_deref())),
        GenicamNodeKind::Enumeration => {
            let selected = text(body, "Value")
                .or_else(|| enum_values.first().map(|entry| entry.symbol.clone()))
                .unwrap_or_default();
            Value::String(selected)
        }
        GenicamNodeKind::String => Value::String(text(body, "Value").unwrap_or_default()),
        GenicamNodeKind::IntSwissKnife => Value::I64(parse_i64(text(body, "Value").as_deref(), 0)?),
        GenicamNodeKind::SwissKnife | GenicamNodeKind::Converter => {
            Value::F64(parse_f64(text(body, "Value").as_deref(), 0.0)?)
        }
        GenicamNodeKind::Command => Value::Null,
    };
    Ok(GenicamNode {
        name: name.into(),
        display_name,
        tooltip: text(body, "ToolTip"),
        description: text(body, "Description"),
        doc_url: text(body, "DocuURL").or_else(|| text(body, "DocURL")),
        kind: kind.clone(),
        access,
        imposed_access: text(body, "ImposedAccessMode").map(|value| parse_access(Some(&value))),
        value,
        min: match kind {
            GenicamNodeKind::Integer => {
                Some(Value::I64(parse_i64(text(body, "Min").as_deref(), 0)?))
            }
            GenicamNodeKind::Float => {
                Some(Value::F64(parse_f64(text(body, "Min").as_deref(), 0.0)?))
            }
            GenicamNodeKind::IntSwissKnife
            | GenicamNodeKind::SwissKnife
            | GenicamNodeKind::Converter => None,
            GenicamNodeKind::Boolean
            | GenicamNodeKind::Enumeration
            | GenicamNodeKind::String
            | GenicamNodeKind::Command => None,
        },
        max: match kind {
            GenicamNodeKind::Integer => Some(Value::I64(parse_i64(
                text(body, "Max").as_deref(),
                i64::MAX,
            )?)),
            GenicamNodeKind::Float => Some(Value::F64(parse_f64(
                text(body, "Max").as_deref(),
                f64::MAX,
            )?)),
            GenicamNodeKind::IntSwissKnife
            | GenicamNodeKind::SwissKnife
            | GenicamNodeKind::Converter => None,
            GenicamNodeKind::Boolean
            | GenicamNodeKind::Enumeration
            | GenicamNodeKind::String
            | GenicamNodeKind::Command => None,
        },
        min_ref: text(body, "pMin"),
        max_ref: text(body, "pMax"),
        unit,
        enum_values,
        address,
        address_ref: text(body, "pAddress"),
        port_ref: text(body, "pPort"),
        length: text(body, "Length"),
        length_ref: text(body, "pLength"),
        struct_ref: text(body, "pStructReg"),
        offset: text(body, "Offset"),
        endian: text(body, "Endianess").or_else(|| text(body, "Endianness")),
        increment,
        increment_ref: text(body, "pInc"),
        bit: text(body, "Bit")
            .map(|value| parse_u8(Some(&value), 0))
            .transpose()?,
        lsb: text(body, "LSB")
            .map(|value| parse_u8(Some(&value), 0))
            .transpose()?,
        msb: text(body, "MSB")
            .map(|value| parse_u8(Some(&value), 0))
            .transpose()?,
        sign: text(body, "Sign"),
        representation: text(body, "Representation"),
        visibility: text(body, "Visibility"),
        polling_time_ms: text(body, "PollingTime")
            .map(|value| parse_i64(Some(&value), 0))
            .transpose()?,
        streamable: text(body, "IsStreamable")
            .or_else(|| text(body, "Streamable"))
            .as_deref()
            .map(parse_streamable),
        category: text(body, "pCategory"),
        selects: texts(body, "pSelected"),
        selected_by: texts(body, "pSelecting"),
        available_ref: text(body, "pIsAvailable"),
        implemented_ref: text(body, "pIsImplemented"),
        locked_ref: text(body, "pIsLocked"),
        value_ref: text(body, "pValue"),
        value_copy_ref: text(body, "pValueCopy"),
        formula: text(body, "Formula"),
        formula_to: text(body, "FormulaTo"),
        formula_from: text(body, "FormulaFrom"),
        variables: if matches!(
            kind,
            GenicamNodeKind::IntSwissKnife
                | GenicamNodeKind::SwissKnife
                | GenicamNodeKind::Converter
        ) {
            parse_formula_variables(body)
        } else {
            Vec::new()
        },
        command_value: text(body, "CommandValue")
            .map(|value| parse_i64(Some(&value), 1))
            .transpose()?,
        cache_mode: text(body, "CachingMode").or_else(|| text(body, "Cacheable")),
        invalidated_by: texts(body, "pInvalidator"),
        invalidates: Vec::new(),
        event_id: text(body, "EventID"),
        event_timestamp_ref: text(body, "pEventTimestamp"),
        event_notification_ref: text(body, "pEventNotification"),
    })
}

fn parse_access(value: Option<&str>) -> GenicamAccess {
    match value.unwrap_or("RW").trim() {
        "RO" => GenicamAccess::ReadOnly,
        "WO" => GenicamAccess::WriteOnly,
        "NA" => GenicamAccess::NotAvailable,
        "NI" => GenicamAccess::NotImplemented,
        _ => GenicamAccess::ReadWrite,
    }
}

fn parse_i64(value: Option<&str>, default: i64) -> Result<i64> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value.parse::<i64>().map_err(|_| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("invalid GenICam integer literal {value}"),
                )
            })
        })
        .unwrap_or(Ok(default))
}

fn parse_u8(value: Option<&str>, default: u8) -> Result<u8> {
    let raw = parse_i64(value, default as i64)?;
    u8::try_from(raw).map_err(|_| {
        Error::new(
            ErrorCode::InvalidProperty,
            format!("invalid GenICam bit index {raw}"),
        )
    })
}

fn parse_f64(value: Option<&str>, default: f64) -> Result<f64> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value.parse::<f64>().map_err(|_| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("invalid GenICam float literal {value}"),
                )
            })
        })
        .unwrap_or(Ok(default))
}

fn parse_bool(value: Option<&str>) -> bool {
    matches!(
        value.unwrap_or("false").trim(),
        "1" | "true" | "True" | "TRUE" | "On" | "on" | "Yes" | "yes"
    )
}

fn parse_streamable(value: &str) -> bool {
    parse_bool(Some(value))
}

fn parse_enum_entries(body: &str) -> Vec<GenicamEnumEntry> {
    element_blocks(body, "EnumEntry")
        .into_iter()
        .filter_map(|block| {
            let symbol = attr(&block.opening_tag, "Name")?;
            let display_name = text(&block.body, "DisplayName").unwrap_or_else(|| symbol.clone());
            let value = text(&block.body, "Value").and_then(|value| value.parse::<i64>().ok());
            Some(GenicamEnumEntry {
                symbol,
                display_name,
                value,
                available_ref: text(&block.body, "pIsAvailable"),
                implemented_ref: text(&block.body, "pIsImplemented"),
            })
        })
        .collect()
}

fn parse_formula_variables(body: &str) -> Vec<GenicamFormulaVariable> {
    let mut variables = Vec::new();
    for tag in ["pVariable", "pValue"] {
        for block in element_blocks(body, tag) {
            let Some(node_ref) = Some(block.body.trim()).filter(|value| !value.is_empty()) else {
                continue;
            };
            let name = attr(&block.opening_tag, "Name")
                .or_else(|| attr(&block.opening_tag, "NameSpace"))
                .unwrap_or_else(|| node_ref.into());
            variables.push(GenicamFormulaVariable {
                name,
                node_ref: unescape(node_ref),
            });
        }
    }
    variables
}

fn parse_categories(xml: &str) -> (BTreeMap<String, GenicamCategory>, Vec<String>) {
    let mut categories = BTreeMap::new();
    let mut category_order = Vec::new();
    for block in element_blocks(xml, "Category") {
        let Some(name) = attr(&block.opening_tag, "Name") else {
            continue;
        };
        let display_name = text(&block.body, "DisplayName").unwrap_or_else(|| name.clone());
        let visibility = text(&block.body, "Visibility");
        let features = texts(&block.body, "pFeature")
            .into_iter()
            .chain(texts(&block.body, "pValue"))
            .collect::<Vec<_>>();
        if !categories.contains_key(&name) {
            category_order.push(name.clone());
        }
        categories.insert(
            name.clone(),
            GenicamCategory {
                name,
                display_name,
                visibility,
                features,
            },
        );
    }
    (categories, category_order)
}

fn parse_ports(xml: &str) -> BTreeMap<String, GenicamPort> {
    element_blocks(xml, "Port")
        .into_iter()
        .filter_map(|block| {
            let name = attr(&block.opening_tag, "Name")?;
            let display_name = text(&block.body, "DisplayName").unwrap_or_else(|| name.clone());
            Some((
                name.clone(),
                GenicamPort {
                    name,
                    display_name,
                    access: parse_access(text(&block.body, "AccessMode").as_deref()),
                },
            ))
        })
        .collect()
}

fn parse_registers(xml: &str) -> BTreeMap<String, GenicamRegister> {
    ["Register", "MaskedIntReg", "StructReg"]
        .into_iter()
        .flat_map(|tag| element_blocks(xml, tag))
        .filter_map(|block| {
            let name = attr(&block.opening_tag, "Name")?;
            let display_name = text(&block.body, "DisplayName").unwrap_or_else(|| name.clone());
            Some((
                name.clone(),
                GenicamRegister {
                    name,
                    display_name,
                    access: parse_access(text(&block.body, "AccessMode").as_deref()),
                    port_ref: text(&block.body, "pPort"),
                    address: text(&block.body, "Address").map(|value| format!("genicam:{value}")),
                    address_ref: text(&block.body, "pAddress"),
                    length: text(&block.body, "Length"),
                    length_ref: text(&block.body, "pLength"),
                    endian: text(&block.body, "Endianess")
                        .or_else(|| text(&block.body, "Endianness")),
                    category: text(&block.body, "pCategory"),
                    visibility: text(&block.body, "Visibility"),
                },
            ))
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ElementBlock {
    opening_tag: String,
    body: String,
}

fn element_blocks(xml: &str, tag: &str) -> Vec<ElementBlock> {
    let mut out = Vec::new();
    let open_prefix = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut offset = 0;
    while let Some(start_rel) = xml[offset..].find(&open_prefix) {
        let start = offset + start_rel;
        if !is_tag_boundary(xml, start + open_prefix.len()) {
            offset = start + open_prefix.len();
            continue;
        }
        let Some(open_end_rel) = xml[start..].find('>') else {
            break;
        };
        let open_end = start + open_end_rel;
        if xml[start..=open_end].ends_with("/>") {
            offset = open_end + 1;
            continue;
        }
        let body_start = open_end + 1;
        let Some(close_rel) = xml[body_start..].find(&close) else {
            break;
        };
        let body_end = body_start + close_rel;
        out.push(ElementBlock {
            opening_tag: xml[start..=open_end].to_string(),
            body: xml[body_start..body_end].to_string(),
        });
        offset = body_end + close.len();
    }
    out
}

fn strip_element_blocks(xml: &str, tag: &str) -> String {
    let open_prefix = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = String::new();
    let mut offset = 0;
    while let Some(start_rel) = xml[offset..].find(&open_prefix) {
        let start = offset + start_rel;
        if !is_tag_boundary(xml, start + open_prefix.len()) {
            out.push_str(&xml[offset..start + open_prefix.len()]);
            offset = start + open_prefix.len();
            continue;
        }
        let Some(open_end_rel) = xml[start..].find('>') else {
            break;
        };
        let open_end = start + open_end_rel;
        out.push_str(&xml[offset..start]);
        if xml[start..=open_end].ends_with("/>") {
            offset = open_end + 1;
            continue;
        }
        let body_start = open_end + 1;
        let Some(close_rel) = xml[body_start..].find(&close) else {
            offset = start;
            break;
        };
        offset = body_start + close_rel + close.len();
    }
    out.push_str(&xml[offset..]);
    out
}

fn is_tag_boundary(xml: &str, index: usize) -> bool {
    matches!(
        xml.as_bytes().get(index),
        Some(b'>') | Some(b'/') | Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n')
    )
}

fn text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(unescape(xml[start..end].trim()))
}

fn texts(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut values = Vec::new();
    let mut offset = 0;
    while let Some(start_rel) = xml[offset..].find(&open) {
        let start = offset + start_rel + open.len();
        let Some(end_rel) = xml[start..].find(&close) else {
            break;
        };
        let end = start + end_rel;
        values.push(unescape(xml[start..end].trim()));
        offset = end + close.len();
    }
    values
}

fn attr(opening_tag: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=\"");
    let start = opening_tag.find(&needle)? + needle.len();
    let end = opening_tag[start..].find('"')? + start;
    Some(unescape(&opening_tag[start..end]))
}

fn unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

const FIXTURE_XML: &str = r#"
<RegisterDescription>
  <Integer Name="Width">
    <DisplayName>Width</DisplayName>
    <ToolTip>Output image width in pixels</ToolTip>
    <Description>Number of active image columns after ROI and binning.</Description>
    <DocuURL>https://www.emva.org/standards-technology/genicam/</DocuURL>
    <Value>1024</Value>
    <Min>64</Min>
    <Max>4096</Max>
    <pInc>WidthIncrement</pInc>
    <Unit>px</Unit>
    <AccessMode>RW</AccessMode>
    <Representation>Linear</Representation>
    <Visibility>Beginner</Visibility>
    <IsStreamable>Yes</IsStreamable>
    <pCategory>ImageFormatControl</pCategory>
    <CachingMode>WriteThrough</CachingMode>
    <pPort>DevicePort</pPort>
    <Address>0x1000</Address>
    <Length>4</Length>
    <Endianess>LittleEndian</Endianess>
  </Integer>
  <Integer Name="WidthIncrement">
    <DisplayName>Width increment</DisplayName>
    <Value>16</Value>
    <Min>1</Min>
    <Max>128</Max>
    <Unit>px</Unit>
    <AccessMode>RO</AccessMode>
    <Visibility>Invisible</Visibility>
    <pCategory>ImageFormatControl</pCategory>
  </Integer>
  <Integer Name="WidthAlias">
    <DisplayName>Width alias</DisplayName>
    <pValue>Width</pValue>
    <Min>64</Min>
    <Max>4096</Max>
    <Inc>8</Inc>
    <Unit>px</Unit>
    <AccessMode>RW</AccessMode>
    <Representation>Linear</Representation>
    <Visibility>Expert</Visibility>
    <pCategory>ImageFormatControl</pCategory>
    <CachingMode>WriteAround</CachingMode>
  </Integer>
  <Integer Name="Height">
    <DisplayName>Height</DisplayName>
    <Value>768</Value>
    <Min>64</Min>
    <Max>3072</Max>
    <Inc>2</Inc>
    <Unit>px</Unit>
    <AccessMode>RW</AccessMode>
    <Representation>Linear</Representation>
    <Visibility>Beginner</Visibility>
    <pCategory>ImageFormatControl</pCategory>
    <CachingMode>WriteThrough</CachingMode>
    <pPort>DevicePort</pPort>
    <Address>0x1004</Address>
    <Length>4</Length>
    <Endianess>LittleEndian</Endianess>
  </Integer>
  <Integer Name="RoiOffsetX">
    <DisplayName>ROI offset X</DisplayName>
    <Value>0</Value>
    <Min>0</Min>
    <Max>4096</Max>
    <Inc>16</Inc>
    <Unit>px</Unit>
    <AccessMode>RW</AccessMode>
    <Representation>Linear</Representation>
    <Visibility>Expert</Visibility>
    <pCategory>ImageFormatControl</pCategory>
    <pStructReg>RoiStruct</pStructReg>
    <Offset>0</Offset>
    <Length>4</Length>
    <Endianess>LittleEndian</Endianess>
  </Integer>
  <Integer Name="RoiOffsetY">
    <DisplayName>ROI offset Y</DisplayName>
    <Value>0</Value>
    <Min>0</Min>
    <Max>3072</Max>
    <Inc>16</Inc>
    <Unit>px</Unit>
    <AccessMode>RW</AccessMode>
    <Representation>Linear</Representation>
    <Visibility>Expert</Visibility>
    <pCategory>ImageFormatControl</pCategory>
    <pStructReg>RoiStruct</pStructReg>
    <Offset>4</Offset>
    <Length>4</Length>
    <Endianess>LittleEndian</Endianess>
  </Integer>
  <Integer Name="PayloadSize">
    <DisplayName>Payload size</DisplayName>
    <Value>786432</Value>
    <Min>1</Min>
    <Max>67108864</Max>
    <Unit>B</Unit>
    <AccessMode>RO</AccessMode>
    <Representation>Linear</Representation>
    <Visibility>Expert</Visibility>
    <pCategory>ImageFormatControl</pCategory>
    <CachingMode>NoCache</CachingMode>
    <pInvalidator>Width</pInvalidator>
    <pInvalidator>Height</pInvalidator>
    <pInvalidator>PixelFormat</pInvalidator>
  </Integer>
  <IntSwissKnife Name="PixelArea">
    <DisplayName>Pixel area</DisplayName>
    <Formula>WIDTH * HEIGHT</Formula>
    <pVariable Name="WIDTH">Width</pVariable>
    <pVariable Name="HEIGHT">Height</pVariable>
    <Visibility>Expert</Visibility>
    <pCategory>ImageFormatControl</pCategory>
  </IntSwissKnife>
  <SwissKnife Name="LineTime">
    <DisplayName>Line time</DisplayName>
    <Formula>MAX(WIDTH, HEIGHT) / PIXEL_CLOCK</Formula>
    <pVariable Name="WIDTH">Width</pVariable>
    <pVariable Name="HEIGHT">Height</pVariable>
    <pVariable Name="PIXEL_CLOCK">DevicePixelClock</pVariable>
    <Visibility>Guru</Visibility>
    <pCategory>AcquisitionControl</pCategory>
  </SwissKnife>
  <SwissKnife Name="ExposureRiskScore">
    <DisplayName>Exposure risk score</DisplayName>
    <Formula>IF(EXP &gt; 1000, POW(EXP / 1000, 2), TRUNC(EXP / 100)) + SGN(EXP - 500)</Formula>
    <pVariable Name="EXP">ExposureTime</pVariable>
    <Visibility>Guru</Visibility>
    <pCategory>AcquisitionControl</pCategory>
  </SwissKnife>
  <SwissKnife Name="GainMathScore">
    <DisplayName>Gain math score</DisplayName>
    <Formula>SQRT(GAIN * GAIN) + LOG10(EXP(1))</Formula>
    <pVariable Name="GAIN">Gain</pVariable>
    <Visibility>Guru</Visibility>
    <pCategory>AnalogControl</pCategory>
  </SwissKnife>
  <IntSwissKnife Name="ModuloStrideScore">
    <DisplayName>Modulo stride score</DisplayName>
    <Formula>MOD(WIDTH, WIDTH_INC) + (HEIGHT % 7)</Formula>
    <pVariable Name="WIDTH">Width</pVariable>
    <pVariable Name="WIDTH_INC">WidthIncrement</pVariable>
    <pVariable Name="HEIGHT">Height</pVariable>
    <Visibility>Guru</Visibility>
    <pCategory>ImageFormatControl</pCategory>
  </IntSwissKnife>
  <Converter Name="ExposureSeconds">
    <DisplayName>Exposure seconds</DisplayName>
    <FormulaTo>FROM / 1000000.0</FormulaTo>
    <FormulaFrom>TO * 1000000.0</FormulaFrom>
    <pVariable Name="FROM">ExposureTime</pVariable>
    <pVariable Name="TO">ExposureTime</pVariable>
    <Unit>s</Unit>
    <AccessMode>RW</AccessMode>
    <Visibility>Expert</Visibility>
    <pCategory>AcquisitionControl</pCategory>
  </Converter>
  <Float Name="ExposureTime">
    <DisplayName>Exposure time</DisplayName>
    <Value>10000.0</Value>
    <Min>10.0</Min>
    <Max>1000000.0</Max>
    <Inc>1.0</Inc>
    <Unit>us</Unit>
    <AccessMode>RW</AccessMode>
    <Representation>Logarithmic</Representation>
    <Visibility>Beginner</Visibility>
    <pCategory>AcquisitionControl</pCategory>
    <pSelecting>ExposureAuto</pSelecting>
    <pPort>DevicePort</pPort>
    <Address>0x1010</Address>
    <Length>8</Length>
    <Endianess>LittleEndian</Endianess>
  </Float>
  <Float Name="StartupExposureTimeCopy">
    <DisplayName>Startup exposure copy</DisplayName>
    <pValueCopy>ExposureTime</pValueCopy>
    <Min>10.0</Min>
    <Max>1000000.0</Max>
    <Inc>1.0</Inc>
    <Unit>us</Unit>
    <AccessMode>RW</AccessMode>
    <Representation>Logarithmic</Representation>
    <Visibility>Guru</Visibility>
    <pCategory>AcquisitionControl</pCategory>
  </Float>
  <Enumeration Name="ExposureAuto">
    <DisplayName>Exposure auto</DisplayName>
    <Value>Off</Value>
    <AccessMode>RW</AccessMode>
    <Visibility>Beginner</Visibility>
    <pCategory>AcquisitionControl</pCategory>
    <pSelected>ExposureTime</pSelected>
    <EnumEntry Name="Off"><DisplayName>Off</DisplayName><Value>0</Value></EnumEntry>
    <EnumEntry Name="Continuous"><DisplayName>Continuous</DisplayName><Value>1</Value></EnumEntry>
  </Enumeration>
  <Float Name="Gain">
    <DisplayName>Gain</DisplayName>
    <Value>1.0</Value>
    <Min>0.0</Min>
    <Max>24.0</Max>
    <Unit>dB</Unit>
    <AccessMode>RW</AccessMode>
    <Representation>Linear</Representation>
    <Visibility>Expert</Visibility>
    <pCategory>AnalogControl</pCategory>
    <pPort>DevicePort</pPort>
    <pAddress>GainAddress</pAddress>
    <pLength>GainRegisterLength</pLength>
    <Endianess>LittleEndian</Endianess>
  </Float>
  <Integer Name="GainAddress">
    <DisplayName>Gain register address</DisplayName>
    <Value>4120</Value>
    <Min>0</Min>
    <Max>65535</Max>
    <AccessMode>RO</AccessMode>
    <Visibility>Invisible</Visibility>
    <pCategory>AnalogControl</pCategory>
  </Integer>
  <Integer Name="GainRegisterLength">
    <DisplayName>Gain register length</DisplayName>
    <Value>8</Value>
    <Min>4</Min>
    <Max>8</Max>
    <AccessMode>RO</AccessMode>
    <Visibility>Invisible</Visibility>
    <pCategory>AnalogControl</pCategory>
  </Integer>
  <Enumeration Name="PixelFormat">
    <DisplayName>Pixel format</DisplayName>
    <Value>Mono8</Value>
    <pValue>PixelFormatValue</pValue>
    <AccessMode>RW</AccessMode>
    <Visibility>Beginner</Visibility>
    <pCategory>ImageFormatControl</pCategory>
    <CachingMode>WriteThrough</CachingMode>
    <EnumEntry Name="Mono8"><DisplayName>Mono 8</DisplayName><Value>17301505</Value></EnumEntry>
    <EnumEntry Name="Mono16"><DisplayName>Mono 16</DisplayName><Value>17825799</Value></EnumEntry>
    <EnumEntry Name="RGB8"><DisplayName>RGB 8</DisplayName><Value>35127316</Value></EnumEntry>
    <EnumEntry Name="BayerRG8"><DisplayName>Bayer RG 8</DisplayName><Value>17301513</Value><pIsAvailable>PixelFormatBayerAvailable</pIsAvailable></EnumEntry>
  </Enumeration>
  <Integer Name="PixelFormatValue">
    <DisplayName>Pixel format backing value</DisplayName>
    <Value>17301505</Value>
    <Min>0</Min>
    <Max>2147483647</Max>
    <AccessMode>RW</AccessMode>
    <Visibility>Invisible</Visibility>
    <pCategory>ImageFormatControl</pCategory>
    <pPort>DevicePort</pPort>
    <Address>0x100c</Address>
    <Length>4</Length>
    <Endianess>LittleEndian</Endianess>
  </Integer>
  <Boolean Name="PixelFormatBayerAvailable">
    <DisplayName>Bayer pixel formats available</DisplayName>
    <Value>false</Value>
    <AccessMode>RO</AccessMode>
    <Visibility>Invisible</Visibility>
    <pCategory>ImageFormatControl</pCategory>
  </Boolean>
  <Boolean Name="AcquisitionFrameRateEnable">
    <DisplayName>Frame-rate limit enabled</DisplayName>
    <Value>true</Value>
    <AccessMode>RW</AccessMode>
    <pCategory>AcquisitionControl</pCategory>
  </Boolean>
  <Integer Name="TriggerControlRegister">
    <DisplayName>Trigger control register</DisplayName>
    <Value>0</Value>
    <Min>0</Min>
    <Max>255</Max>
    <AccessMode>RW</AccessMode>
    <Visibility>Invisible</Visibility>
    <pCategory>AcquisitionControl</pCategory>
    <pPort>DevicePort</pPort>
    <Address>0x1030</Address>
    <Length>1</Length>
    <Sign>Unsigned</Sign>
  </Integer>
  <MaskedIntReg Name="TriggerModeBits">
    <DisplayName>Trigger mode bits</DisplayName>
    <Value>0</Value>
    <Min>0</Min>
    <Max>3</Max>
    <AccessMode>RW</AccessMode>
    <Visibility>Expert</Visibility>
    <pCategory>AcquisitionControl</pCategory>
    <pPort>DevicePort</pPort>
    <Address>0x1030</Address>
    <Length>1</Length>
    <Sign>Unsigned</Sign>
    <LSB>1</LSB>
    <MSB>2</MSB>
  </MaskedIntReg>
  <Integer Name="StatusWord">
    <DisplayName>Status word</DisplayName>
    <Value>4026531841</Value>
    <Min>0</Min>
    <Max>4294967295</Max>
    <AccessMode>RW</AccessMode>
    <Visibility>Expert</Visibility>
    <pCategory>AcquisitionControl</pCategory>
    <pPort>DevicePort</pPort>
    <Address>0x1034</Address>
    <Length>4</Length>
    <Endianess>LittleEndian</Endianess>
    <Sign>Unsigned</Sign>
  </Integer>
  <IntSwissKnife Name="StatusNibbleShifted">
    <DisplayName>Status low nibble shifted</DisplayName>
    <Formula>(STATUS &amp; 15) &lt;&lt; 1</Formula>
    <pVariable Name="STATUS">StatusWord</pVariable>
    <Visibility>Expert</Visibility>
    <pCategory>AcquisitionControl</pCategory>
  </IntSwissKnife>
  <Boolean Name="FrameRateFeatureAvailable">
    <DisplayName>Frame-rate feature available</DisplayName>
    <Value>true</Value>
    <AccessMode>RO</AccessMode>
    <Visibility>Invisible</Visibility>
    <pCategory>AcquisitionControl</pCategory>
  </Boolean>
  <Boolean Name="FrameRateFeatureImplemented">
    <DisplayName>Frame-rate feature implemented</DisplayName>
    <Value>true</Value>
    <AccessMode>RO</AccessMode>
    <Visibility>Invisible</Visibility>
    <pCategory>AcquisitionControl</pCategory>
  </Boolean>
  <Boolean Name="FrameRateFeatureLocked">
    <DisplayName>Frame-rate feature locked</DisplayName>
    <Value>false</Value>
    <AccessMode>RW</AccessMode>
    <Visibility>Invisible</Visibility>
    <pCategory>AcquisitionControl</pCategory>
  </Boolean>
  <Float Name="AcquisitionFrameRate">
    <DisplayName>Acquisition frame rate</DisplayName>
    <Value>30.0</Value>
    <Min>0.1</Min>
    <Max>240.0</Max>
    <pMax>FrameRateLimitMax</pMax>
    <Unit>Hz</Unit>
    <AccessMode>RW</AccessMode>
    <Representation>Linear</Representation>
    <Visibility>Expert</Visibility>
    <pCategory>AcquisitionControl</pCategory>
    <pIsAvailable>FrameRateFeatureAvailable</pIsAvailable>
    <pIsImplemented>FrameRateFeatureImplemented</pIsImplemented>
    <pIsLocked>FrameRateFeatureLocked</pIsLocked>
  </Float>
  <Float Name="FrameRateLimitMax">
    <DisplayName>Frame-rate dynamic maximum</DisplayName>
    <Value>120.0</Value>
    <Min>1.0</Min>
    <Max>240.0</Max>
    <Unit>Hz</Unit>
    <AccessMode>RO</AccessMode>
    <Visibility>Invisible</Visibility>
    <pCategory>AcquisitionControl</pCategory>
  </Float>
  <String Name="DeviceFirmwareVersion">
    <DisplayName>Firmware version</DisplayName>
    <Value>fixture-1.0</Value>
    <AccessMode>RO</AccessMode>
    <Visibility>Beginner</Visibility>
    <pCategory>DeviceControl</pCategory>
  </String>
  <StringReg Name="DeviceUserId">
    <DisplayName>Device user ID</DisplayName>
    <Value>bench-cam</Value>
    <AccessMode>RW</AccessMode>
    <Visibility>Beginner</Visibility>
    <pCategory>DeviceControl</pCategory>
    <pPort>DevicePort</pPort>
    <Address>0x1060</Address>
    <Length>16</Length>
  </StringReg>
  <String Name="DeviceFactorySecret">
    <DisplayName>Factory secret</DisplayName>
    <Value>hidden</Value>
    <AccessMode>NA</AccessMode>
    <Visibility>Invisible</Visibility>
    <pCategory>DeviceControl</pCategory>
  </String>
  <String Name="DeviceFutureFeature">
    <DisplayName>Future feature</DisplayName>
    <Value>not-implemented</Value>
    <AccessMode>NI</AccessMode>
    <Visibility>Invisible</Visibility>
    <pCategory>DeviceControl</pCategory>
  </String>
  <Integer Name="DevicePixelClock">
    <DisplayName>Device pixel clock</DisplayName>
    <Value>100000000</Value>
    <Min>1000000</Min>
    <Max>1000000000</Max>
    <Unit>Hz</Unit>
    <AccessMode>RO</AccessMode>
    <Visibility>Expert</Visibility>
    <pCategory>DeviceControl</pCategory>
  </Integer>
  <Float Name="SensorTemperature">
    <DisplayName>Sensor temperature</DisplayName>
    <Value>22.5</Value>
    <Min>-20.0</Min>
    <Max>85.0</Max>
    <Unit>C</Unit>
    <AccessMode>RW</AccessMode>
    <ImposedAccessMode>RO</ImposedAccessMode>
    <Visibility>Expert</Visibility>
    <pCategory>DeviceControl</pCategory>
  </Float>
  <Integer Name="ResetControlRegister">
    <DisplayName>Reset control register</DisplayName>
    <Value>0</Value>
    <Min>0</Min>
    <Max>255</Max>
    <AccessMode>RW</AccessMode>
    <Visibility>Invisible</Visibility>
    <pCategory>DeviceControl</pCategory>
    <pPort>DevicePort</pPort>
    <Address>0x1050</Address>
    <Length>1</Length>
    <Sign>Unsigned</Sign>
  </Integer>
  <Boolean Name="AcquisitionActive">
    <DisplayName>Acquisition active</DisplayName>
    <Value>false</Value>
    <AccessMode>RO</AccessMode>
    <pCategory>AcquisitionControl</pCategory>
  </Boolean>
  <Enumeration Name="EventNotification">
    <DisplayName>Event notification</DisplayName>
    <Value>On</Value>
    <AccessMode>RW</AccessMode>
    <Visibility>Expert</Visibility>
    <pCategory>EventControl</pCategory>
    <EnumEntry Name="Off"><DisplayName>Off</DisplayName><Value>0</Value></EnumEntry>
    <EnumEntry Name="On"><DisplayName>On</DisplayName><Value>1</Value></EnumEntry>
  </Enumeration>
  <Integer Name="EventExposureEndTimestamp">
    <DisplayName>Exposure end timestamp</DisplayName>
    <ToolTip>Timestamp for the most recent exposure-end event</ToolTip>
    <Description>Fixture event timestamp used to exercise GenICam event metadata.</Description>
    <Value>123456789</Value>
    <Min>0</Min>
    <Max>9223372036854775807</Max>
    <Unit>ns</Unit>
    <AccessMode>RO</AccessMode>
    <Visibility>Expert</Visibility>
    <pCategory>EventControl</pCategory>
    <CachingMode>NoCache</CachingMode>
    <PollingTime>25</PollingTime>
  </Integer>
  <Integer Name="ExposureEnd">
    <DisplayName>Exposure end event</DisplayName>
    <Value>0</Value>
    <Min>0</Min>
    <Max>1</Max>
    <AccessMode>RO</AccessMode>
    <Visibility>Expert</Visibility>
    <pCategory>EventControl</pCategory>
    <EventID>ExposureEnd</EventID>
    <pEventTimestamp>EventExposureEndTimestamp</pEventTimestamp>
    <pEventNotification>EventNotification</pEventNotification>
  </Integer>
  <Command Name="AcquisitionStart">
    <DisplayName>Acquisition start</DisplayName>
    <AccessMode>WO</AccessMode>
    <CommandValue>1</CommandValue>
    <pCategory>AcquisitionControl</pCategory>
    <pPort>DevicePort</pPort>
    <Address>0x1020</Address>
    <Length>4</Length>
    <Endianess>LittleEndian</Endianess>
  </Command>
  <Command Name="AcquisitionStop">
    <DisplayName>Acquisition stop</DisplayName>
    <AccessMode>WO</AccessMode>
    <CommandValue>0</CommandValue>
    <pCategory>AcquisitionControl</pCategory>
    <pPort>DevicePort</pPort>
    <Address>0x1024</Address>
    <Length>4</Length>
    <Endianess>LittleEndian</Endianess>
  </Command>
  <Command Name="DeviceReset">
    <DisplayName>Device reset</DisplayName>
    <AccessMode>WO</AccessMode>
    <CommandValue>165</CommandValue>
    <pValue>ResetControlRegister</pValue>
    <pCategory>DeviceControl</pCategory>
  </Command>
  <Port Name="DevicePort">
    <DisplayName>Device register port</DisplayName>
    <AccessMode>RW</AccessMode>
  </Port>
  <Register Name="ImageWindowRegister">
    <DisplayName>Image window register block</DisplayName>
    <AccessMode>RW</AccessMode>
    <pPort>DevicePort</pPort>
    <Address>0x1000</Address>
    <Length>8</Length>
    <Endianess>LittleEndian</Endianess>
    <Visibility>Expert</Visibility>
    <pCategory>DeviceControl</pCategory>
  </Register>
  <StructReg Name="RoiStruct">
    <DisplayName>ROI struct register block</DisplayName>
    <AccessMode>RW</AccessMode>
    <pPort>DevicePort</pPort>
    <Address>0x1040</Address>
    <Length>8</Length>
    <Endianess>LittleEndian</Endianess>
    <Visibility>Expert</Visibility>
    <pCategory>DeviceControl</pCategory>
  </StructReg>
  <Category Name="ImageFormatControl">
    <DisplayName>Image format control</DisplayName>
    <Visibility>Beginner</Visibility>
    <pFeature>Width</pFeature>
    <pFeature>WidthIncrement</pFeature>
    <pFeature>WidthAlias</pFeature>
    <pFeature>Height</pFeature>
    <pFeature>RoiOffsetX</pFeature>
    <pFeature>RoiOffsetY</pFeature>
    <pFeature>PixelFormat</pFeature>
    <pFeature>PixelFormatValue</pFeature>
    <pFeature>PixelFormatBayerAvailable</pFeature>
    <pFeature>PayloadSize</pFeature>
    <pFeature>PixelArea</pFeature>
    <pFeature>ModuloStrideScore</pFeature>
  </Category>
  <Category Name="AcquisitionControl">
    <DisplayName>Acquisition control</DisplayName>
    <Visibility>Beginner</Visibility>
    <pFeature>ExposureAuto</pFeature>
    <pFeature>ExposureTime</pFeature>
    <pFeature>StartupExposureTimeCopy</pFeature>
    <pFeature>AcquisitionFrameRateEnable</pFeature>
    <pFeature>TriggerControlRegister</pFeature>
    <pFeature>TriggerModeBits</pFeature>
    <pFeature>StatusWord</pFeature>
    <pFeature>StatusNibbleShifted</pFeature>
    <pFeature>AcquisitionFrameRate</pFeature>
    <pFeature>ExposureSeconds</pFeature>
    <pFeature>LineTime</pFeature>
    <pFeature>ExposureRiskScore</pFeature>
    <pFeature>FrameRateFeatureAvailable</pFeature>
    <pFeature>FrameRateFeatureImplemented</pFeature>
    <pFeature>FrameRateFeatureLocked</pFeature>
    <pFeature>FrameRateLimitMax</pFeature>
    <pFeature>AcquisitionStart</pFeature>
    <pFeature>AcquisitionStop</pFeature>
  </Category>
  <Category Name="AnalogControl">
    <DisplayName>Analog control</DisplayName>
    <Visibility>Expert</Visibility>
    <pFeature>Gain</pFeature>
    <pFeature>GainMathScore</pFeature>
    <pFeature>GainAddress</pFeature>
    <pFeature>GainRegisterLength</pFeature>
  </Category>
  <Category Name="DeviceControl">
    <DisplayName>Device control</DisplayName>
    <Visibility>Beginner</Visibility>
    <pFeature>DeviceFirmwareVersion</pFeature>
    <pFeature>DeviceUserId</pFeature>
    <pFeature>DeviceFactorySecret</pFeature>
    <pFeature>DeviceFutureFeature</pFeature>
    <pFeature>DevicePixelClock</pFeature>
    <pFeature>SensorTemperature</pFeature>
    <pFeature>ResetControlRegister</pFeature>
    <pFeature>DeviceReset</pFeature>
    <pFeature>ImageWindowRegister</pFeature>
    <pFeature>RoiStruct</pFeature>
  </Category>
  <Category Name="EventControl">
    <DisplayName>Event control</DisplayName>
    <Visibility>Expert</Visibility>
    <pFeature>EventNotification</pFeature>
    <pFeature>EventExposureEndTimestamp</pFeature>
    <pFeature>ExposureEnd</pFeature>
  </Category>
  <Category Name="Root">
    <DisplayName>Root</DisplayName>
    <Visibility>Beginner</Visibility>
    <pFeature>DeviceControl</pFeature>
    <pFeature>ImageFormatControl</pFeature>
    <pFeature>AcquisitionControl</pFeature>
    <pFeature>AnalogControl</pFeature>
    <pFeature>EventControl</pFeature>
  </Category>
</RegisterDescription>
"#;
