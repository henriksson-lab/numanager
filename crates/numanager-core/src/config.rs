use crate::{
    Acceleration, ByteCount, ControllerScalar, Decibel, DeviceId, DriverId, ElectricCurrent,
    FlowRate, Frequency, GasConcentration, NodeId, OpticalPower, PixelCount, Position, Pressure,
    Ratio, ResourceId, Role, StepCount, Temperature, TimeInterval, Timestamp, Value, Velocity,
    Voltage, Wavelength,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub type Result<T> = std::result::Result<T, ConfigError>;

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Parse(message) => write!(f, "config parse error: {message}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone, Default)]
pub struct HardwareConfig {
    pub resources: Vec<ResourceConfig>,
    pub devices: Vec<DeviceConfig>,
    pub dependencies: Vec<DependencyConfig>,
    pub remux_groups: Vec<RemuxGroup>,
}

#[derive(Debug, Clone)]
pub struct HardwareConfigBuilder {
    config: HardwareConfig,
    next_node_id: u64,
}

#[derive(Debug, Clone)]
pub struct ResourceConfig {
    pub id: ResourceId,
    pub label: String,
    pub driver: String,
    pub params: BTreeMap<String, Value>,
}

#[derive(Debug, Clone)]
pub struct DeviceConfig {
    pub id: DeviceId,
    pub label: String,
    pub driver: String,
    pub properties: BTreeMap<String, Value>,
}

#[derive(Debug, Clone)]
pub struct DependencyConfig {
    pub from: DeviceId,
    pub to: DeviceId,
    pub role: Role,
}

#[derive(Debug, Clone)]
pub struct RemuxGroup {
    pub name: String,
    pub devices: Vec<DeviceId>,
    pub resource: Option<ResourceId>,
}

impl ResourceConfig {
    pub fn new(
        id: u64,
        label: impl Into<String>,
        driver: impl Into<String>,
        params: BTreeMap<String, Value>,
    ) -> Self {
        Self {
            id: ResourceId(NodeId(id)),
            label: label.into(),
            driver: driver.into(),
            params,
        }
    }
}

impl DeviceConfig {
    pub fn new(
        id: u64,
        label: impl Into<String>,
        driver: impl Into<String>,
        properties: BTreeMap<String, Value>,
    ) -> Self {
        Self {
            id: DeviceId(NodeId(id)),
            label: label.into(),
            driver: driver.into(),
            properties,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DiscoveryLock {
    pub entries: Vec<DiscoveryEntry>,
}

#[derive(Debug, Clone)]
pub struct DiscoveryEntry {
    pub persistent_id: Option<String>,
    pub label: String,
    pub aliases: Vec<String>,
    pub driver: DriverId,
    pub serial: Option<String>,
    pub firmware: Option<String>,
    pub metadata: BTreeMap<String, Value>,
}

impl HardwareConfig {
    pub fn builder() -> HardwareConfigBuilder {
        HardwareConfigBuilder::new(1)
    }

    pub fn builder_from(first_node_id: u64) -> HardwareConfigBuilder {
        HardwareConfigBuilder::new(first_node_id)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        parse_config(&fs::read_to_string(path)?)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        fs::write(path, self.to_toml())?;
        Ok(())
    }

    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        for resource in &self.resources {
            out.push_str("[[resources]]\n");
            out.push_str(&format!("id = {}\n", (resource.id).0 .0));
            out.push_str(&format!("label = \"{}\"\n", escape(&resource.label)));
            out.push_str(&format!("driver = \"{}\"\n", escape(&resource.driver)));
            for (key, value) in &resource.params {
                out.push_str(&format!("param.{} = {}\n", key, value_to_toml(value)));
            }
            out.push('\n');
        }
        for device in &self.devices {
            out.push_str("[[devices]]\n");
            out.push_str(&format!("id = {}\n", (device.id).0 .0));
            out.push_str(&format!("label = \"{}\"\n", escape(&device.label)));
            out.push_str(&format!("driver = \"{}\"\n", escape(&device.driver)));
            for (key, value) in &device.properties {
                out.push_str(&format!("property.{} = {}\n", key, value_to_toml(value)));
            }
            out.push('\n');
        }
        for group in &self.remux_groups {
            out.push_str("[[remux_groups]]\n");
            out.push_str(&format!("name = \"{}\"\n", escape(&group.name)));
            let ids = group
                .devices
                .iter()
                .map(|id| (id.0).0.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("devices = [{}]\n", ids));
            if let Some(resource) = group.resource {
                out.push_str(&format!("resource = {}\n", resource.0 .0));
            }
            out.push('\n');
        }
        for dep in &self.dependencies {
            out.push_str("[[dependencies]]\n");
            out.push_str(&format!("from = {}\n", dep.from.0 .0));
            out.push_str(&format!("to = {}\n", dep.to.0 .0));
            out.push_str(&format!("role = \"{}\"\n\n", role_to_string(&dep.role)));
        }
        out
    }
}

impl HardwareConfigBuilder {
    pub fn new(first_node_id: u64) -> Self {
        Self {
            config: HardwareConfig::default(),
            next_node_id: first_node_id,
        }
    }

    pub fn add_resource(
        &mut self,
        label: impl Into<String>,
        driver: impl Into<String>,
        params: BTreeMap<String, Value>,
    ) -> ResourceId {
        let id = ResourceId(self.next_node());
        self.config.resources.push(ResourceConfig {
            id,
            label: label.into(),
            driver: driver.into(),
            params,
        });
        id
    }

    pub fn add_device(
        &mut self,
        label: impl Into<String>,
        driver: impl Into<String>,
        properties: BTreeMap<String, Value>,
    ) -> DeviceId {
        let id = DeviceId(self.next_node());
        self.config.devices.push(DeviceConfig {
            id,
            label: label.into(),
            driver: driver.into(),
            properties,
        });
        id
    }

    pub fn add_dependency(
        &mut self,
        from: impl Into<DeviceId>,
        to: impl Into<DeviceId>,
        role: Role,
    ) -> &mut Self {
        self.config.dependencies.push(DependencyConfig {
            from: from.into(),
            to: to.into(),
            role,
        });
        self
    }

    pub fn add_remux_group(
        &mut self,
        name: impl Into<String>,
        devices: impl IntoIterator<Item = impl Into<DeviceId>>,
        resource: Option<ResourceId>,
    ) -> &mut Self {
        self.config.remux_groups.push(RemuxGroup {
            name: name.into(),
            devices: devices.into_iter().map(Into::into).collect(),
            resource,
        });
        self
    }

    pub fn build(self) -> HardwareConfig {
        self.config
    }

    fn next_node(&mut self) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        id
    }
}

impl DiscoveryLock {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        parse_lock(&fs::read_to_string(path)?)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let mut out = String::new();
        for entry in &self.entries {
            out.push_str("[[discovered]]\n");
            if let Some(persistent_id) = &entry.persistent_id {
                out.push_str(&format!("persistent_id = \"{}\"\n", escape(persistent_id)));
            }
            out.push_str(&format!("label = \"{}\"\n", escape(&entry.label)));
            if !entry.aliases.is_empty() {
                let aliases = entry
                    .aliases
                    .iter()
                    .map(|alias| format!("\"{}\"", escape(alias)))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!("aliases = [{aliases}]\n"));
            }
            out.push_str(&format!("driver = {}\n", entry.driver.0));
            if let Some(serial) = &entry.serial {
                out.push_str(&format!("serial = \"{}\"\n", escape(serial)));
            }
            if let Some(firmware) = &entry.firmware {
                out.push_str(&format!("firmware = \"{}\"\n", escape(firmware)));
            }
            for (key, value) in &entry.metadata {
                out.push_str(&format!("metadata.{} = {}\n", key, value_to_toml(value)));
            }
            out.push('\n');
        }
        fs::write(path, out)?;
        Ok(())
    }
}

fn parse_config(src: &str) -> Result<HardwareConfig> {
    let mut config = HardwareConfig::default();
    let mut section = "";
    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[resources]]" {
            config.resources.push(ResourceConfig {
                id: ResourceId(NodeId(0)),
                label: String::new(),
                driver: String::new(),
                params: BTreeMap::new(),
            });
            section = "resources";
            continue;
        }
        if line == "[[devices]]" {
            config.devices.push(DeviceConfig {
                id: DeviceId(NodeId(0)),
                label: String::new(),
                driver: String::new(),
                properties: BTreeMap::new(),
            });
            section = "devices";
            continue;
        }
        if line == "[[remux_groups]]" {
            config.remux_groups.push(RemuxGroup {
                name: String::new(),
                devices: Vec::new(),
                resource: None,
            });
            section = "remux_groups";
            continue;
        }
        if line == "[[dependencies]]" {
            config.dependencies.push(DependencyConfig {
                from: DeviceId(NodeId(0)),
                to: DeviceId(NodeId(0)),
                role: Role::Custom(String::new()),
            });
            section = "dependencies";
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| ConfigError::Parse(format!("expected key=value: {line}")))?;
        let key = key.trim();
        let value = value.trim();
        match section {
            "resources" => {
                let resource = config
                    .resources
                    .last_mut()
                    .expect("resource section exists");
                match key {
                    "id" => resource.id = ResourceId(NodeId(parse_u64(value)?)),
                    "label" => resource.label = parse_string(value),
                    "driver" => resource.driver = parse_string(value),
                    k if k.starts_with("param.") => {
                        let param_key = k.trim_start_matches("param.");
                        resource
                            .params
                            .insert(param_key.to_string(), parse_value_for_key(param_key, value));
                    }
                    _ => {}
                }
            }
            "devices" => {
                let device = config.devices.last_mut().expect("device section exists");
                match key {
                    "id" => device.id = DeviceId(NodeId(parse_u64(value)?)),
                    "label" => device.label = parse_string(value),
                    "driver" => device.driver = parse_string(value),
                    k if k.starts_with("property.") => {
                        let property_key = k.trim_start_matches("property.");
                        device.properties.insert(
                            property_key.to_string(),
                            parse_value_for_key(property_key, value),
                        );
                    }
                    _ => {}
                }
            }
            "remux_groups" => {
                let group = config
                    .remux_groups
                    .last_mut()
                    .expect("remux group section exists");
                match key {
                    "name" => group.name = parse_string(value),
                    "devices" => group.devices = parse_device_list(value),
                    "resource" => group.resource = Some(ResourceId(NodeId(parse_u64(value)?))),
                    _ => {}
                }
            }
            "dependencies" => {
                let dep = config
                    .dependencies
                    .last_mut()
                    .expect("dependency section exists");
                match key {
                    "from" => dep.from = DeviceId(NodeId(parse_u64(value)?)),
                    "to" => dep.to = DeviceId(NodeId(parse_u64(value)?)),
                    "role" => dep.role = parse_role(&parse_string(value)),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    Ok(config)
}

fn parse_lock(src: &str) -> Result<DiscoveryLock> {
    let mut lock = DiscoveryLock::default();
    let mut in_entry = false;
    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[discovered]]" {
            lock.entries.push(DiscoveryEntry {
                persistent_id: None,
                label: String::new(),
                aliases: Vec::new(),
                driver: DriverId(0),
                serial: None,
                firmware: None,
                metadata: BTreeMap::new(),
            });
            in_entry = true;
            continue;
        }
        if !in_entry {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| ConfigError::Parse(format!("expected key=value: {line}")))?;
        let entry = lock.entries.last_mut().expect("discovery entry exists");
        let key = key.trim();
        match key {
            "persistent_id" => entry.persistent_id = Some(parse_string(value.trim())),
            "label" => entry.label = parse_string(value.trim()),
            "aliases" => entry.aliases = parse_string_list(value.trim()),
            "driver" => entry.driver = DriverId(parse_u64(value.trim())?),
            "serial" => entry.serial = Some(parse_string(value.trim())),
            "firmware" => entry.firmware = Some(parse_string(value.trim())),
            k if k.starts_with("metadata.") => {
                let metadata_key = k.trim_start_matches("metadata.");
                entry.metadata.insert(
                    metadata_key.to_string(),
                    parse_value_for_key(metadata_key, value.trim()),
                );
            }
            _ => {}
        }
    }
    Ok(lock)
}

fn parse_u64(value: &str) -> Result<u64> {
    value
        .parse()
        .map_err(|_| ConfigError::Parse(format!("invalid integer: {value}")))
}

fn parse_string(value: &str) -> String {
    value.trim_matches('"').replace("\\\"", "\"")
}

fn parse_device_list(value: &str) -> Vec<DeviceId> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .filter_map(|part| part.trim().parse::<u64>().ok())
        .map(|id| DeviceId(NodeId(id)))
        .collect()
}

fn parse_string_list(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(parse_string)
        .collect()
}

fn parse_value_for_key(key: &str, value: &str) -> Value {
    if value == "true" {
        Value::Bool(true)
    } else if value == "false" {
        Value::Bool(false)
    } else if value.starts_with('"') {
        let parsed = parse_string(value);
        parse_typed_string(key, &parsed).unwrap_or(Value::String(parsed))
    } else if value.contains('.') {
        value
            .parse()
            .map(Value::F64)
            .unwrap_or(Value::String(value.into()))
    } else {
        value
            .parse()
            .map(Value::I64)
            .unwrap_or(Value::String(value.into()))
    }
}

fn parse_typed_string(key: &str, value: &str) -> Option<Value> {
    let mut parts = value.split_whitespace();
    let number = parts.next()?.parse::<f64>().ok()?;
    let unit = parts.collect::<Vec<_>>().join(" ");
    if unit.is_empty() {
        return None;
    }
    let lower_key = key.to_ascii_lowercase();
    Some(match unit.as_str() {
        "degC" => Value::Temperature(Temperature::from_celsius(number)),
        "K" => Value::Temperature(Temperature::from_kelvin(number)),
        "degF" => Value::Temperature(Temperature::from_fahrenheit(number)),
        "m" => Value::Position(Position::from_meters(number)),
        "mm" => Value::Position(Position::from_millimeters(number)),
        "um" => Value::Position(Position::from_micrometers(number)),
        "m/s" => Value::Velocity(Velocity::from_meters_per_second(number)),
        "mm/s" => Value::Velocity(Velocity::from_millimeters_per_second(number)),
        "um/s" => Value::Velocity(Velocity::from_micrometers_per_second(number)),
        "m/s^2" => Value::Acceleration(Acceleration::from_meters_per_second_squared(number)),
        "mm/s^2" => Value::Acceleration(Acceleration::from_millimeters_per_second_squared(number)),
        "um/s^2" => Value::Acceleration(Acceleration::from_micrometers_per_second_squared(number)),
        "h" => Value::TimeInterval(TimeInterval::from_hours(number)),
        "s" => Value::TimeInterval(TimeInterval::from_seconds(number)),
        "ms" => Value::TimeInterval(TimeInterval::from_milliseconds(number)),
        "us" => Value::TimeInterval(TimeInterval::from_microseconds(number)),
        "ns" => Value::TimeInterval(TimeInterval::from_nanoseconds(number)),
        "controller_tick" if lower_key.contains("timestamp") => {
            Value::Timestamp(Timestamp::from_controller_ticks(number.round() as i64))
        }
        "controller_tick" => Value::TimeInterval(TimeInterval::from_controller_ticks(number)),
        "nm" => Value::Wavelength(Wavelength::from_nanometers(number)),
        "angstrom" => Value::Wavelength(Wavelength::from_nanometers(number * 0.1)),
        "W" => Value::OpticalPower(OpticalPower::from_watts(number)),
        "mW" => Value::OpticalPower(OpticalPower::from_milliwatts(number)),
        "uW" => Value::OpticalPower(OpticalPower::from_microwatts(number)),
        "A" => Value::ElectricCurrent(ElectricCurrent::from_amps(number)),
        "mA" => Value::ElectricCurrent(ElectricCurrent::from_milliamps(number)),
        "uA" => Value::ElectricCurrent(ElectricCurrent::from_microamps(number)),
        "V" => Value::Voltage(Voltage::from_volts(number)),
        "mV" => Value::Voltage(Voltage::from_millivolts(number)),
        "uV" => Value::Voltage(Voltage::from_microvolts(number)),
        "Hz" => Value::Frequency(Frequency::from_hertz(number)),
        "kHz" => Value::Frequency(Frequency::from_kilohertz(number)),
        "MHz" => Value::Frequency(Frequency::from_megahertz(number)),
        "dB" => Value::Decibel(Decibel::new(number)),
        "px" => Value::PixelCount(PixelCount::new(number.round().max(0.0) as u32)),
        "bytes" => Value::ByteCount(ByteCount::new(number.round().max(0.0) as u64)),
        "steps" => Value::StepCount(StepCount::new(number.round() as i64)),
        "controller_step" => Value::ControllerScalar(ControllerScalar::new(number.round() as i64)),
        "percent" if is_gas_key(&lower_key) => {
            Value::GasConcentration(GasConcentration::from_percent(number))
        }
        "ppm" => Value::GasConcentration(GasConcentration::from_ppm(number)),
        "fraction" if is_gas_key(&lower_key) => {
            Value::GasConcentration(GasConcentration::from_fraction(number))
        }
        "fraction" => Value::Ratio(Ratio::from_fraction(number)),
        "percent" => Value::Ratio(Ratio::from_percent(number)),
        "Pa" => Value::Pressure(Pressure::from_pascals(number)),
        "kPa" => Value::Pressure(Pressure::from_kilopascals(number)),
        "bar" => Value::Pressure(Pressure::from_bar(number)),
        "mbar" => Value::Pressure(Pressure::from_millibar(number)),
        "psi" => Value::Pressure(Pressure::from_psi(number)),
        "L/min" => Value::FlowRate(FlowRate::from_liters_per_minute(number)),
        "mL/min" => Value::FlowRate(FlowRate::from_milliliters_per_minute(number)),
        "uL/min" => Value::FlowRate(FlowRate::from_microliters_per_minute(number)),
        "sccm" => Value::FlowRate(FlowRate::from_standard_cubic_centimeters_per_minute(number)),
        _ => return None,
    })
}

fn is_gas_key(key: &str) -> bool {
    key.contains("co2") || key.contains("o2") || key.contains("gas")
}

fn value_to_toml(value: &Value) -> String {
    match value {
        Value::Bool(v) => v.to_string(),
        Value::I64(v) => v.to_string(),
        Value::F64(v) => v.to_string(),
        Value::Temperature(v) => format!("\"{} {}\"", v.value, v.unit_symbol()),
        Value::Position(v) => format!("\"{} {}\"", v.value, v.unit_symbol()),
        Value::Velocity(v) => format!("\"{} {}\"", v.value, v.unit_symbol()),
        Value::Acceleration(v) => format!("\"{} {}\"", v.value, v.unit_symbol()),
        Value::TimeInterval(v) => format!("\"{} {}\"", v.value, v.unit_symbol()),
        Value::Wavelength(v) => format!("\"{} {}\"", v.value, v.unit_symbol()),
        Value::OpticalPower(v) => format!("\"{} {}\"", v.value, v.unit_symbol()),
        Value::ElectricCurrent(v) => format!("\"{} {}\"", v.value, v.unit_symbol()),
        Value::Voltage(v) => format!("\"{} {}\"", v.value, v.unit_symbol()),
        Value::Frequency(v) => format!("\"{} {}\"", v.value, v.unit_symbol()),
        Value::Decibel(v) => format!("\"{} {}\"", v.value, v.unit_symbol()),
        Value::PixelCount(v) => format!("\"{} px\"", v.pixels()),
        Value::ByteCount(v) => format!("\"{} {}\"", v.bytes(), v.unit_symbol()),
        Value::StepCount(v) => format!("\"{} {}\"", v.steps(), v.unit_symbol()),
        Value::ControllerScalar(v) => format!("\"{} {}\"", v.value(), v.unit_symbol()),
        Value::Ratio(v) => format!("\"{} {}\"", v.value, v.unit_symbol()),
        Value::NumericalAperture(v) => v.value().to_string(),
        Value::Timestamp(v) => format!("\"{} {}\"", v.value, v.unit_symbol()),
        Value::Pressure(v) => format!("\"{} {}\"", v.value, v.unit_symbol()),
        Value::GasConcentration(v) => format!("\"{} {}\"", v.value, v.unit_symbol()),
        Value::FlowRate(v) => format!("\"{} {}\"", v.value, v.unit_symbol()),
        Value::String(v) => format!("\"{}\"", escape(v)),
        Value::Bytes(v) => format!("\"{} bytes\"", v.len()),
        Value::List(_) | Value::Map(_) | Value::Null => "\"unsupported\"".to_string(),
    }
}

fn escape(value: &str) -> String {
    value.replace('"', "\\\"")
}

fn role_to_string(role: &Role) -> String {
    match role {
        Role::ParentHub => "parent_hub".into(),
        Role::Camera => "camera".into(),
        Role::ZStage => "z_stage".into(),
        Role::XYStage => "xy_stage".into(),
        Role::LightSource => "light_source".into(),
        Role::TimingSource => "timing_source".into(),
        Role::TriggerSink => "trigger_sink".into(),
        Role::TriggerSource => "trigger_source".into(),
        Role::Autofocus => "autofocus".into(),
        Role::Environment => "environment".into(),
        Role::Custom(value) => value.clone(),
    }
}

fn parse_role(value: &str) -> Role {
    match value {
        "parent_hub" => Role::ParentHub,
        "camera" => Role::Camera,
        "z_stage" => Role::ZStage,
        "xy_stage" => Role::XYStage,
        "light_source" => Role::LightSource,
        "timing_source" => Role::TimingSource,
        "trigger_sink" => Role::TriggerSink,
        "trigger_source" => Role::TriggerSource,
        "autofocus" => Role::Autofocus,
        "environment" => Role::Environment,
        other => Role::Custom(other.to_string()),
    }
}
