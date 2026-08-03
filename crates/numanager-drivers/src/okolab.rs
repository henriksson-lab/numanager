use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::sync::OnceLock;

/// The Okolab command dictionary, extracted from the vendor's SQLite database
/// by `scripts/extract-okolab-db.sh` and **embedded at compile time**.
///
/// Reading the `.db` directly would mean a SQLite dependency for the whole
/// crate — which does not link on Windows (no system sqlite3) and pulls a C
/// toolchain into every downstream build, for one driver's static lookup table.
/// It would also only work when running from a source checkout, since the
/// database was located by a path relative to the build directory.
///
/// Regenerate with `scripts/extract-okolab-db.sh` after a vendor database
/// refresh; `--check` fails when the extract is stale.
const OKOLAB_DICTIONARY_JSON: &str = include_str!("../../../data/third_party/okolab/okolib.json");

const OKOLAB_DB_PATH: &str = "data/third_party/okolab/okolib.json";

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const BAUD_PRIMARY: u32 = 115_200;
    pub const BAUD_FALLBACK: u32 = 4_800;
    pub const TERMINATOR: u8 = b'\r';

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CommandKind {
        Read,
        Write,
        VolatileWrite,
    }

    impl CommandKind {
        pub fn checksum_prefix(self) -> char {
            match self {
                CommandKind::Read => 'G',
                CommandKind::Write => 'S',
                CommandKind::VolatileWrite => 'R',
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct CommandFrame {
        pub code: u16,
        pub payload: Option<String>,
        pub kind: CommandKind,
        pub checksum: bool,
    }

    impl CommandFrame {
        pub fn read(code: u16) -> Self {
            Self {
                code,
                payload: None,
                kind: CommandKind::Read,
                checksum: false,
            }
        }

        pub fn write(code: u16, payload: impl Into<String>, volatile: bool) -> Self {
            Self {
                code,
                payload: Some(payload.into()),
                kind: if volatile {
                    CommandKind::VolatileWrite
                } else {
                    CommandKind::Write
                },
                checksum: false,
            }
        }

        pub fn with_checksum(mut self, checksum: bool) -> Self {
            self.checksum = checksum;
            self
        }

        pub fn encode(&self) -> Result<Vec<u8>> {
            let code = format!("{:03}", self.code);
            let payload = self.payload.as_deref().unwrap_or("");
            let mut out = Vec::new();
            if self.checksum {
                out.push(self.kind.checksum_prefix() as u8);
                out.extend_from_slice(code.as_bytes());
                out.extend_from_slice(payload.as_bytes());
                let checksum = checksum16_signed(&out[1..]);
                out.push(b'#');
                out.push((checksum >> 8) as u8);
                out.push(checksum as u8);
                out.push(TERMINATOR);
            } else {
                out.extend_from_slice(code.as_bytes());
                out.extend_from_slice(payload.as_bytes());
                out.push(TERMINATOR);
            }
            Ok(out)
        }
    }

    pub fn checksum16_signed(bytes: &[u8]) -> u16 {
        let sum = bytes
            .iter()
            .fold(0i32, |acc, byte| acc + (*byte as i8 as i32));
        sum as i16 as u16
    }

    pub fn parse_reply(frame: &CommandFrame, bytes: &[u8]) -> Result<String> {
        let data = if frame.checksum {
            bytes.strip_suffix(&[TERMINATOR]).unwrap_or(bytes)
        } else if let Some(pos) = bytes.iter().position(|byte| *byte == TERMINATOR) {
            &bytes[..pos]
        } else {
            bytes
        };
        if data.first() == Some(&b'E') {
            let text = std::str::from_utf8(data)
                .map_err(|_| Error::new(ErrorCode::Transport, "Okolab error is not UTF-8"))?;
            let error = text.strip_prefix('E').unwrap_or(text);
            return Err(Error::new(
                ErrorCode::Driver,
                format!("Okolab controller returned error E{error}"),
            ));
        }
        if frame.checksum {
            parse_checksum_reply(frame, data)
        } else {
            parse_plain_reply(frame.code, data)
        }
    }

    fn parse_plain_reply(code: u16, data: &[u8]) -> Result<String> {
        let text = std::str::from_utf8(data)
            .map_err(|_| Error::new(ErrorCode::Transport, "Okolab reply is not UTF-8"))?;
        let expected = format!("{:03}", code);
        text.strip_prefix(&expected)
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::Transport,
                    format!("Okolab reply did not echo command code {expected}"),
                )
            })
    }

    fn parse_checksum_reply(frame: &CommandFrame, data: &[u8]) -> Result<String> {
        if data.len() < 7 {
            return Err(Error::new(
                ErrorCode::Transport,
                "Okolab checksum reply is too short",
            ));
        }
        let expected_prefix = frame.kind.checksum_prefix() as u8;
        if data.first() != Some(&expected_prefix) {
            return Err(Error::new(
                ErrorCode::Transport,
                "Okolab checksum reply did not echo command type",
            ));
        }
        let Some(hash_index) = data.iter().position(|byte| *byte == b'#') else {
            return Err(Error::new(
                ErrorCode::Transport,
                "Okolab checksum reply is missing checksum marker",
            ));
        };
        if data.len() != hash_index + 3 {
            return Err(Error::new(
                ErrorCode::Transport,
                "Okolab checksum reply has trailing data after checksum",
            ));
        }
        let received = ((data[hash_index + 1] as u16) << 8) | data[hash_index + 2] as u16;
        let computed = checksum16_signed(&data[1..hash_index]);
        if computed != received {
            return Err(Error::new(
                ErrorCode::Transport,
                format!(
                    "Okolab checksum mismatch: received 0x{received:04x}, computed 0x{computed:04x}"
                ),
            ));
        }
        let expected = format!("{:03}", frame.code);
        let body = std::str::from_utf8(&data[1..hash_index]).map_err(|_| {
            Error::new(
                ErrorCode::Transport,
                "Okolab checksum reply payload is not UTF-8",
            )
        })?;
        body.strip_prefix(&expected)
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::Transport,
                    format!("Okolab checksum reply did not echo command code {expected}"),
                )
            })
    }

    pub fn reply_complete(bytes: &[u8], checksum: bool) -> bool {
        if !checksum || bytes.first() == Some(&b'E') {
            return bytes.contains(&TERMINATOR);
        }
        let Some(hash_index) = bytes.iter().position(|byte| *byte == b'#') else {
            return false;
        };
        bytes.len() >= hash_index + 4 && bytes.last() == Some(&TERMINATOR)
    }
}

#[derive(Debug, Clone)]
pub struct OkolabConfiguredProbe {
    label: String,
    port_name: Option<String>,
    connect_real_transport: bool,
    checksum_enabled: bool,
    module: OkolabModuleConfig,
}

#[derive(Debug, Clone)]
pub struct OkolabModuleConfig {
    product: String,
    serial_number: Option<String>,
    firmware_version: String,
    name_code: u16,
    temperature: Option<OkolabTemperatureConfig>,
    gas: Option<OkolabGasConfig>,
    humidity_percent: Option<f64>,
    humidity_enabled: Option<bool>,
    fault: String,
}

#[derive(Debug, Clone)]
pub struct OkolabTemperatureConfig {
    actual_c: f64,
    target_c: f64,
    actual_code: u16,
    target_read_code: u16,
    target_write_code: u16,
    status_read_code: u16,
    enabled: bool,
    status: String,
}

#[derive(Debug, Clone)]
pub struct OkolabGasConfig {
    co2_actual_percent: f64,
    co2_target_percent: f64,
    co2_read_code: u16,
    co2_target_read_code: u16,
    co2_target_write_code: u16,
    o2_actual_percent: f64,
    o2_target_percent: f64,
    o2_read_code: u16,
    o2_target_read_code: u16,
    o2_target_write_code: u16,
    co2_status_read_code: u16,
    enabled: bool,
    status: String,
}

pub struct OkolabDiscovery {
    next_id: DriverId,
    probes: Vec<OkolabConfiguredProbe>,
}

impl OkolabDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![OkolabConfiguredProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), "okolab" | "oko-lab"))
            .map(OkolabConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for OkolabDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .iter()
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let driver: Box<dyn Driver> = if probe.connect_real_transport {
                    Box::new(OkolabDriver::serial(id, probe.clone())?)
                } else {
                    Box::new(OkolabDriver::configured(id, probe.clone()))
                };
                Ok(DriverCandidate::from_driver(
                    probe.discovery_label(),
                    driver,
                ))
            })
            .collect()
    }
}

impl OkolabConfiguredProbe {
    pub fn fixture() -> Self {
        Self {
            label: "Configured Okolab environmental controller".into(),
            port_name: None,
            connect_real_transport: false,
            checksum_enabled: false,
            module: OkolabModuleConfig {
                product: "H201 T Unit-BL".into(),
                serial_number: Some("OKOLAB-CONFIG-0001".into()),
                firmware_version: "configured".into(),
                name_code: 64,
                temperature: Some(OkolabTemperatureConfig {
                    actual_c: 37.0,
                    target_c: 37.0,
                    actual_code: 48,
                    target_read_code: 48,
                    target_write_code: 48,
                    status_read_code: 128,
                    enabled: true,
                    status: "unvalidated".into(),
                }),
                gas: Some(OkolabGasConfig {
                    co2_actual_percent: 5.0,
                    co2_target_percent: 5.0,
                    co2_read_code: 4,
                    co2_target_read_code: 7,
                    co2_target_write_code: 11,
                    o2_actual_percent: 20.0,
                    o2_target_percent: 20.0,
                    o2_read_code: 5,
                    o2_target_read_code: 8,
                    o2_target_write_code: 12,
                    co2_status_read_code: 129,
                    enabled: false,
                    status: "unvalidated".into(),
                }),
                humidity_percent: None,
                humidity_enabled: None,
                fault: "unknown_without_hardware_validation".into(),
            },
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = Self::fixture();
        if !device.label.is_empty() {
            probe.label = device.label.clone();
        }
        probe.module.product = string_prop(device, "product").unwrap_or(probe.module.product);
        probe.module.serial_number =
            optional_string_prop(device, "serial_number", probe.module.serial_number);
        probe.module.firmware_version =
            string_prop(device, "firmware_version").unwrap_or(probe.module.firmware_version);
        probe.module.name_code = u16_prop(device, "name_code")?.unwrap_or(probe.module.name_code);
        probe.port_name = optional_string_prop(device, "port", probe.port_name);
        probe.connect_real_transport =
            bool_prop(device, "connect_real_transport").unwrap_or(probe.connect_real_transport);
        probe.checksum_enabled =
            bool_prop(device, "checksum_enabled").unwrap_or(probe.checksum_enabled);
        probe.module.fault = string_prop(device, "fault").unwrap_or(probe.module.fault);
        if let Some(temp) = probe.module.temperature.as_mut() {
            temp.actual_c = f64_prop(device, "temperature_actual_c").unwrap_or(temp.actual_c);
            temp.target_c = f64_prop(device, "temperature_target_c").unwrap_or(temp.target_c);
            temp.actual_code =
                u16_prop(device, "temperature_read_code")?.unwrap_or(temp.actual_code);
            temp.target_read_code =
                u16_prop(device, "temperature_target_read_code")?.unwrap_or(temp.target_read_code);
            temp.target_write_code = u16_prop(device, "temperature_target_write_code")?
                .unwrap_or(temp.target_write_code);
            temp.status_read_code =
                u16_prop(device, "temperature_status_read_code")?.unwrap_or(temp.status_read_code);
            temp.enabled = bool_prop(device, "temperature_enabled").unwrap_or(temp.enabled);
            temp.status = string_prop(device, "temperature_status").unwrap_or(temp.status.clone());
        }
        if let Some(gas) = probe.module.gas.as_mut() {
            gas.co2_actual_percent =
                f64_prop(device, "co2_actual_percent").unwrap_or(gas.co2_actual_percent);
            gas.co2_target_percent =
                f64_prop(device, "co2_target_percent").unwrap_or(gas.co2_target_percent);
            gas.co2_read_code = u16_prop(device, "co2_read_code")?.unwrap_or(gas.co2_read_code);
            gas.co2_target_read_code =
                u16_prop(device, "co2_target_read_code")?.unwrap_or(gas.co2_target_read_code);
            gas.co2_target_write_code =
                u16_prop(device, "co2_target_write_code")?.unwrap_or(gas.co2_target_write_code);
            gas.o2_actual_percent =
                f64_prop(device, "o2_actual_percent").unwrap_or(gas.o2_actual_percent);
            gas.o2_target_percent =
                f64_prop(device, "o2_target_percent").unwrap_or(gas.o2_target_percent);
            gas.o2_read_code = u16_prop(device, "o2_read_code")?.unwrap_or(gas.o2_read_code);
            gas.o2_target_read_code =
                u16_prop(device, "o2_target_read_code")?.unwrap_or(gas.o2_target_read_code);
            gas.o2_target_write_code =
                u16_prop(device, "o2_target_write_code")?.unwrap_or(gas.o2_target_write_code);
            gas.co2_status_read_code =
                u16_prop(device, "co2_status_read_code")?.unwrap_or(gas.co2_status_read_code);
            gas.enabled = bool_prop(device, "gas_enabled").unwrap_or(gas.enabled);
            gas.status = string_prop(device, "gas_status").unwrap_or(gas.status.clone());
        }
        probe.module.humidity_percent =
            optional_f64_prop(device, "humidity_percent", probe.module.humidity_percent);
        probe.module.humidity_enabled =
            optional_bool_prop(device, "humidity_enabled", probe.module.humidity_enabled);
        Ok(probe)
    }

    fn discovery_label(&self) -> String {
        format!("{} ({})", self.label, self.module.product)
    }
}

pub struct OkolabDriver {
    id: DriverId,
    hub: DeviceId,
    temperature: Option<DeviceId>,
    gas: Option<DeviceId>,
    humidity: Option<DeviceId>,
    serial: ResourceId,
    database: ResourceId,
    probe: OkolabConfiguredProbe,
    dictionary_status: String,
    dictionary_parameters: Vec<OkolabParameter>,
    next_token: u64,
    events: VecDeque<DriverEvent>,
}

#[derive(Debug, Clone)]
struct OkolabParameter {
    name: String,
    unit: Option<String>,
    description: Option<String>,
    var_type: i64,
    main: bool,
    advanced: bool,
    oneshot: bool,
    read_code: u16,
    write_code: u16,
    write_code_ram: u16,
    min_code: u16,
    max_code: u16,
    enum_values: BTreeMap<i64, String>,
}

impl OkolabDriver {
    pub fn configured(id: DriverId, probe: OkolabConfiguredProbe) -> Self {
        let (dictionary_status, dictionary_parameters) =
            match load_dictionary_for_product(&probe.module.product) {
                Ok(parameters) => (
                    format!(
                        "loaded {} parameter(s) for {}",
                        parameters.len(),
                        probe.module.product
                    ),
                    parameters,
                ),
                Err(error) => (format!("unavailable: {error}"), Vec::new()),
            };
        let humidity_available = probe.module.humidity_percent.is_some()
            || probe.module.humidity_enabled.is_some()
            || select_humidity_parameter(&dictionary_parameters).is_some()
            || select_humidity_enabled_parameter(&dictionary_parameters).is_some();
        Self {
            id,
            hub: DeviceId(NodeId(id.0 * 1000 + 990)),
            temperature: probe
                .module
                .temperature
                .as_ref()
                .map(|_| DeviceId(NodeId(id.0 * 1000 + 991))),
            gas: probe
                .module
                .gas
                .as_ref()
                .map(|_| DeviceId(NodeId(id.0 * 1000 + 992))),
            humidity: humidity_available.then_some(DeviceId(NodeId(id.0 * 1000 + 993))),
            serial: ResourceId(NodeId(id.0 * 1000 + 994)),
            database: ResourceId(NodeId(id.0 * 1000 + 995)),
            probe,
            dictionary_status,
            dictionary_parameters,
            next_token: 1,
            events: VecDeque::new(),
        }
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, probe: OkolabConfiguredProbe) -> Result<Self> {
        let mut driver = Self::configured(id, probe);
        driver.refresh_connected_identity()?;
        Ok(driver)
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _probe: OkolabConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Okolab real serial transport requires the os-serial feature",
        ))
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        let mut devices = vec![self.hub_descriptor()];
        if let Some(device) = self.temperature {
            devices.push(self.temperature_descriptor(device));
        }
        if let Some(device) = self.gas {
            devices.push(self.gas_descriptor(device));
        }
        if let Some(device) = self.humidity {
            devices.push(self.humidity_descriptor(device));
        }
        devices
    }

    fn hub_descriptor(&self) -> DeviceDescriptor {
        DeviceDescriptor {
            id: self.hub,
            driver: self.id,
            label: format!("{} hub", self.probe.label),
            vendor: Some("Okolab".into()),
            model: Some(self.probe.module.product.clone()),
            serial: self.probe.module.serial_number.clone(),
            kinds: vec![
                "hub".into(),
                "environment.controller".into(),
                "serial.device".into(),
            ],
            properties: vec![
                string_property("model", "Model"),
                string_property("serial_number", "Serial number"),
                string_property("firmware", "Firmware"),
                string_property("support_level", "Support level"),
                string_property("database_path", "Command database path"),
                string_property("database_status", "Command database status"),
                property(
                    "database_parameter_count",
                    "Database parameter count",
                    ValueType::I64,
                ),
                property("name_code", "Name code", ValueType::I64),
                property("checksum_enabled", "Checksum enabled", ValueType::Bool),
                property("connected", "Connected", ValueType::Bool),
                property("fault_active", "Fault active", ValueType::Bool),
                string_property("fault", "Fault"),
                property("module_summary", "Module summary", ValueType::Map),
                property("parameter_summary", "Parameter summary", ValueType::Map),
            ],
            metadata: self.shared_metadata(),
        }
    }

    fn temperature_descriptor(&self, device: DeviceId) -> DeviceDescriptor {
        DeviceDescriptor {
            id: device,
            driver: self.id,
            label: format!("{} temperature", self.probe.label),
            vendor: Some("Okolab".into()),
            model: Some(self.probe.module.product.clone()),
            serial: self.probe.module.serial_number.clone(),
            kinds: vec!["environment.temperature".into(), "measure".into()],
            properties: vec![
                property("actual", "Actual temperature", ValueType::Temperature),
                property("target", "Temperature target", ValueType::Temperature).writable(),
                property("enabled", "Enabled", ValueType::Bool).writable(),
                string_property("status", "Status"),
                property("status_read_code", "Status read code", ValueType::I64),
                property("read_code", "Read code", ValueType::I64),
                property("write_code", "Write code", ValueType::I64),
            ],
            metadata: self.shared_metadata(),
        }
    }

    fn gas_descriptor(&self, device: DeviceId) -> DeviceDescriptor {
        let mut properties = vec![
            property("co2_actual", "CO2 actual", ValueType::GasConcentration),
            property("co2_target", "CO2 target", ValueType::GasConcentration).writable(),
            property("enabled", "Enabled", ValueType::Bool).writable(),
            string_property("status", "Status"),
            property(
                "co2_status_read_code",
                "CO2 status read code",
                ValueType::I64,
            ),
            property("co2_read_code", "CO2 read code", ValueType::I64),
            property("co2_write_code", "CO2 write code", ValueType::I64),
        ];
        if self.o2_available() {
            properties.extend([
                property("o2_actual", "O2 actual", ValueType::GasConcentration),
                property("o2_target", "O2 target", ValueType::GasConcentration).writable(),
                property("o2_read_code", "O2 read code", ValueType::I64),
                property("o2_write_code", "O2 write code", ValueType::I64),
            ]);
        }
        DeviceDescriptor {
            id: device,
            driver: self.id,
            label: format!("{} gas", self.probe.label),
            vendor: Some("Okolab".into()),
            model: Some(self.probe.module.product.clone()),
            serial: self.probe.module.serial_number.clone(),
            kinds: vec!["environment.gas".into(), "measure".into()],
            properties,
            metadata: self.shared_metadata(),
        }
    }

    fn humidity_descriptor(&self, device: DeviceId) -> DeviceDescriptor {
        DeviceDescriptor {
            id: device,
            driver: self.id,
            label: format!("{} humidity", self.probe.label),
            vendor: Some("Okolab".into()),
            model: Some(self.probe.module.product.clone()),
            serial: self.probe.module.serial_number.clone(),
            kinds: vec!["environment.humidity".into(), "measure".into()],
            properties: vec![
                property("relative_humidity", "Relative humidity", ValueType::Ratio),
                property("enabled", "Enabled", ValueType::Bool).writable(),
                property("read_code", "Read code", ValueType::I64),
                property("enabled_read_code", "Enabled read code", ValueType::I64),
                property("enabled_write_code", "Enabled write code", ValueType::I64),
            ],
            metadata: self.shared_metadata(),
        }
    }

    fn shared_metadata(&self) -> BTreeMap<String, Value> {
        BTreeMap::from([
            (
                "support_level".into(),
                Value::String("reverse engineered serial command helpers".into()),
            ),
            ("hardware_validated".into(), Value::Bool(false)),
            ("sdk_free".into(), Value::Bool(true)),
            (
                "command_database".into(),
                Value::String(OKOLAB_DB_PATH.into()),
            ),
        ])
    }

    fn read_property(&mut self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "model" => Ok(Value::String(self.probe.module.product.clone())),
                "serial_number" => Ok(Value::String(
                    self.probe.module.serial_number.clone().unwrap_or_default(),
                )),
                "firmware" => Ok(Value::String(self.probe.module.firmware_version.clone())),
                "support_level" => Ok(Value::String(
                    "reverse engineered serial command helpers".into(),
                )),
                "database_path" => Ok(Value::String(OKOLAB_DB_PATH.into())),
                "database_status" => Ok(Value::String(self.dictionary_status.clone())),
                "database_parameter_count" => {
                    Ok(Value::I64(self.dictionary_parameters.len() as i64))
                }
                "name_code" => Ok(Value::I64(self.probe.module.name_code as i64)),
                "checksum_enabled" => Ok(Value::Bool(self.probe.checksum_enabled)),
                "connected" => Ok(Value::Bool(self.probe.connect_real_transport)),
                "fault_active" => Ok(Value::Bool(self.probe.module.fault != "none")),
                "fault" => Ok(Value::String(self.probe.module.fault.clone())),
                "module_summary" => Ok(self.module_summary()),
                "parameter_summary" => Ok(self.parameter_summary()),
                _ => invalid_property("unknown Okolab hub property", key),
            };
        }
        if Some(device) == self.temperature {
            return self.read_temperature_property(key);
        }
        if Some(device) == self.gas {
            return self.read_gas_property(key);
        }
        if Some(device) == self.humidity {
            return match key {
                "relative_humidity" => self.read_humidity(),
                "enabled" => self.read_humidity_enabled(),
                "read_code" => Ok(Value::I64(
                    select_humidity_parameter(&self.dictionary_parameters)
                        .map(|parameter| parameter.read_code as i64)
                        .unwrap_or_default(),
                )),
                "enabled_read_code" => Ok(Value::I64(
                    select_humidity_enabled_parameter(&self.dictionary_parameters)
                        .map(|parameter| parameter.read_code as i64)
                        .unwrap_or_default(),
                )),
                "enabled_write_code" => Ok(Value::I64(
                    select_humidity_enabled_parameter(&self.dictionary_parameters)
                        .map(|parameter| parameter.write_code as i64)
                        .unwrap_or_default(),
                )),
                _ => invalid_property("unknown Okolab humidity property", key),
            };
        }
        Err(Error::new(
            ErrorCode::InvalidCommand,
            "unknown Okolab device",
        ))
    }

    fn read_temperature_property(&mut self, key: &str) -> Result<Value> {
        let temp =
            self.probe.module.temperature.as_ref().ok_or_else(|| {
                Error::new(ErrorCode::Unsupported, "Okolab temperature unavailable")
            })?;
        match key {
            "actual" => {
                let code = temp.actual_code;
                if let Some(actual) = self.read_live_f64(code, "temperature actual")? {
                    if let Some(temp) = self.probe.module.temperature.as_mut() {
                        temp.actual_c = actual;
                    }
                }
                let temp = self
                    .probe
                    .module
                    .temperature
                    .as_ref()
                    .expect("checked above");
                Ok(Value::Temperature(Temperature::from_celsius(temp.actual_c)))
            }
            "target" => {
                let code = temp.target_read_code;
                if let Some(target) = self.read_live_f64(code, "temperature target")? {
                    if let Some(temp) = self.probe.module.temperature.as_mut() {
                        temp.target_c = target;
                    }
                }
                let temp = self
                    .probe
                    .module
                    .temperature
                    .as_ref()
                    .expect("checked above");
                Ok(Value::Temperature(Temperature::from_celsius(temp.target_c)))
            }
            "enabled" => Ok(Value::Bool(temp.enabled)),
            "status" => {
                let code = temp.status_read_code;
                if let Some(status) = self.read_live_raw(code, "temperature status")? {
                    if let Some(temp) = self.probe.module.temperature.as_mut() {
                        temp.status = format!("raw:{status}");
                    }
                }
                let temp = self
                    .probe
                    .module
                    .temperature
                    .as_ref()
                    .expect("checked above");
                Ok(Value::String(temp.status.clone()))
            }
            "status_read_code" => Ok(Value::I64(temp.status_read_code as i64)),
            "read_code" => Ok(Value::I64(temp.actual_code as i64)),
            "write_code" => Ok(Value::I64(temp.target_write_code as i64)),
            _ => invalid_property("unknown Okolab temperature property", key),
        }
    }

    fn read_gas_property(&mut self, key: &str) -> Result<Value> {
        let gas = self
            .probe
            .module
            .gas
            .as_ref()
            .ok_or_else(|| Error::new(ErrorCode::Unsupported, "Okolab gas unavailable"))?;
        match key {
            "co2_actual" => {
                let code = gas.co2_read_code;
                if let Some(actual) = self.read_live_f64(code, "CO2 actual")? {
                    if let Some(gas) = self.probe.module.gas.as_mut() {
                        gas.co2_actual_percent = actual;
                    }
                }
                let gas = self.probe.module.gas.as_ref().expect("checked above");
                Ok(Value::GasConcentration(GasConcentration::from_percent(
                    gas.co2_actual_percent,
                )))
            }
            "co2_target" => {
                let code = gas.co2_target_read_code;
                if let Some(target) = self.read_live_f64(code, "CO2 target")? {
                    if let Some(gas) = self.probe.module.gas.as_mut() {
                        gas.co2_target_percent = target;
                    }
                }
                let gas = self.probe.module.gas.as_ref().expect("checked above");
                Ok(Value::GasConcentration(GasConcentration::from_percent(
                    gas.co2_target_percent,
                )))
            }
            "o2_actual" if self.o2_available() => {
                let code = self
                    .select_o2_parameter()
                    .map(|parameter| parameter.read_code)
                    .unwrap_or(gas.o2_read_code);
                if let Some(actual) = self.read_live_f64(code, "O2 actual")? {
                    if let Some(gas) = self.probe.module.gas.as_mut() {
                        gas.o2_actual_percent = actual;
                    }
                }
                let gas = self.probe.module.gas.as_ref().expect("checked above");
                Ok(Value::GasConcentration(GasConcentration::from_percent(
                    gas.o2_actual_percent,
                )))
            }
            "o2_target" if self.o2_available() => {
                let code = self
                    .select_o2_setpoint_parameter()
                    .map(|parameter| parameter.read_code)
                    .unwrap_or(gas.o2_target_read_code);
                if let Some(target) = self.read_live_f64(code, "O2 target")? {
                    if let Some(gas) = self.probe.module.gas.as_mut() {
                        gas.o2_target_percent = target;
                    }
                }
                let gas = self.probe.module.gas.as_ref().expect("checked above");
                Ok(Value::GasConcentration(GasConcentration::from_percent(
                    gas.o2_target_percent,
                )))
            }
            "enabled" => {
                if let Some(parameter) = select_gas_paused_parameter(&self.dictionary_parameters) {
                    if let Some(raw) = self.read_live_raw(parameter.read_code, &parameter.name)? {
                        let paused = parse_okolab_boolish(&raw, &parameter.name)?;
                        if let Some(gas) = self.probe.module.gas.as_mut() {
                            gas.enabled = !paused;
                        }
                    }
                }
                let gas = self.probe.module.gas.as_ref().expect("checked above");
                Ok(Value::Bool(gas.enabled))
            }
            "status" => {
                let code = gas.co2_status_read_code;
                if let Some(status) = self.read_live_raw(code, "CO2 status")? {
                    if let Some(gas) = self.probe.module.gas.as_mut() {
                        gas.status = format!("raw:{status}");
                    }
                }
                let gas = self.probe.module.gas.as_ref().expect("checked above");
                Ok(Value::String(gas.status.clone()))
            }
            "co2_status_read_code" => Ok(Value::I64(gas.co2_status_read_code as i64)),
            "co2_read_code" => Ok(Value::I64(gas.co2_read_code as i64)),
            "co2_write_code" => Ok(Value::I64(gas.co2_target_write_code as i64)),
            "o2_read_code" if self.o2_available() => Ok(Value::I64(
                self.select_o2_parameter()
                    .map(|parameter| parameter.read_code)
                    .unwrap_or(gas.o2_read_code) as i64,
            )),
            "o2_write_code" if self.o2_available() => Ok(Value::I64(
                self.select_o2_setpoint_parameter()
                    .map(|parameter| parameter.write_code)
                    .unwrap_or(gas.o2_target_write_code) as i64,
            )),
            _ => invalid_property("unknown Okolab gas property", key),
        }
    }

    fn read_humidity(&mut self) -> Result<Value> {
        if let Some(parameter) = select_humidity_parameter(&self.dictionary_parameters) {
            if let Some(value) = self.read_live_f64(parameter.read_code, &parameter.name)? {
                self.probe.module.humidity_percent = Some(value);
            }
        }
        Ok(Value::Ratio(Ratio::from_percent(
            self.probe.module.humidity_percent.unwrap_or_default(),
        )))
    }

    fn read_humidity_enabled(&mut self) -> Result<Value> {
        if let Some(parameter) = select_humidity_enabled_parameter(&self.dictionary_parameters) {
            if let Some(raw) = self.read_live_raw(parameter.read_code, &parameter.name)? {
                let enabled = parse_okolab_boolish(&raw, &parameter.name)?;
                self.probe.module.humidity_enabled = Some(enabled);
            }
        }
        Ok(Value::Bool(
            self.probe.module.humidity_enabled.unwrap_or(false),
        ))
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: Value) -> Result<Value> {
        self.validate_write(device, key, &value)?;
        if Some(device) == self.temperature {
            return self.write_temperature_property(key, value);
        }
        if Some(device) == self.gas {
            return self.write_gas_property(key, value);
        }
        if Some(device) == self.humidity {
            return self.write_humidity_property(key, value);
        }
        Err(Error::new(
            ErrorCode::Unsupported,
            "Okolab writable property is not available on this device",
        ))
    }

    fn write_temperature_property(&mut self, key: &str, value: Value) -> Result<Value> {
        let target_write_code = self
            .probe
            .module
            .temperature
            .as_ref()
            .ok_or_else(|| Error::new(ErrorCode::Unsupported, "Okolab temperature unavailable"))?
            .target_write_code;
        match (key, value) {
            ("target", Value::Temperature(value)) => {
                let encoded = format!("{:.3}", value.celsius());
                self.send_or_cache(protocol::CommandFrame::write(
                    target_write_code,
                    encoded,
                    true,
                ))?;
                if let Some(temp) = self.probe.module.temperature.as_mut() {
                    temp.target_c = value.celsius();
                }
                Ok(Value::Temperature(value))
            }
            ("enabled", Value::Bool(value)) => {
                if let Some(temp) = self.probe.module.temperature.as_mut() {
                    temp.enabled = value;
                }
                Ok(Value::Bool(value))
            }
            ("target", other) => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Okolab target expects Temperature, got {other:?}"),
            )),
            ("enabled", other) => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Okolab enabled expects Bool, got {other:?}"),
            )),
            _ => invalid_property("unknown Okolab temperature writable property", key),
        }
    }

    fn write_gas_property(&mut self, key: &str, value: Value) -> Result<Value> {
        let co2_target_write_code = self
            .probe
            .module
            .gas
            .as_ref()
            .ok_or_else(|| Error::new(ErrorCode::Unsupported, "Okolab gas unavailable"))?
            .co2_target_write_code;
        match (key, value) {
            ("co2_target", Value::GasConcentration(value)) => {
                let percent = value.fraction() * 100.0;
                let encoded = format!("{percent:.3}");
                self.send_or_cache(protocol::CommandFrame::write(
                    co2_target_write_code,
                    encoded,
                    true,
                ))?;
                if let Some(gas) = self.probe.module.gas.as_mut() {
                    gas.co2_target_percent = percent;
                }
                Ok(Value::GasConcentration(value))
            }
            ("o2_target", Value::GasConcentration(value)) if self.o2_available() => {
                let code = self
                    .select_o2_setpoint_parameter()
                    .map(|parameter| parameter.write_code)
                    .unwrap_or_else(|| {
                        self.probe
                            .module
                            .gas
                            .as_ref()
                            .map(|gas| gas.o2_target_write_code)
                            .unwrap_or(12)
                    });
                let percent = value.fraction() * 100.0;
                let encoded = format!("{percent:.3}");
                self.send_or_cache(protocol::CommandFrame::write(code, encoded, true))?;
                if let Some(gas) = self.probe.module.gas.as_mut() {
                    gas.o2_target_percent = percent;
                }
                Ok(Value::GasConcentration(value))
            }
            ("enabled", Value::Bool(value)) => {
                if let Some(parameter) = select_gas_paused_parameter(&self.dictionary_parameters) {
                    let encoded = if value { "0" } else { "1" };
                    self.send_or_cache(protocol::CommandFrame::write(
                        parameter.write_code,
                        encoded,
                        false,
                    ))?;
                }
                if let Some(gas) = self.probe.module.gas.as_mut() {
                    gas.enabled = value;
                }
                Ok(Value::Bool(value))
            }
            ("co2_target", other) => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Okolab co2_target expects GasConcentration, got {other:?}"),
            )),
            ("o2_target", other) if self.o2_available() => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Okolab o2_target expects GasConcentration, got {other:?}"),
            )),
            ("enabled", other) => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Okolab enabled expects Bool, got {other:?}"),
            )),
            _ => invalid_property("unknown Okolab gas writable property", key),
        }
    }

    fn write_humidity_property(&mut self, key: &str, value: Value) -> Result<Value> {
        match (key, value) {
            ("enabled", Value::Bool(value)) => {
                if let Some(parameter) =
                    select_humidity_enabled_parameter(&self.dictionary_parameters)
                {
                    let encoded = if value { "1" } else { "0" };
                    self.send_or_cache(protocol::CommandFrame::write(
                        parameter.write_code,
                        encoded,
                        false,
                    ))?;
                }
                self.probe.module.humidity_enabled = Some(value);
                Ok(Value::Bool(value))
            }
            ("enabled", other) => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Okolab humidity enabled expects Bool, got {other:?}"),
            )),
            _ => invalid_property("unknown Okolab humidity writable property", key),
        }
    }

    fn invoke(
        &mut self,
        device: DeviceId,
        capability: CapabilityId,
        request: CapabilityRequest,
    ) -> Result<Value> {
        let descriptor = self
            .capabilities(device)
            .into_iter()
            .find(|candidate| candidate.id == capability)
            .ok_or_else(|| Error::new(ErrorCode::Unsupported, "unknown Okolab capability"))?;
        match (descriptor.kind, request) {
            (CapabilityKind::Measure, CapabilityRequest::Measure(_))
                if Some(device) == self.temperature =>
            {
                let actual = self.read_property(device, "actual")?;
                Ok(Value::Map(BTreeMap::from([("actual".into(), actual)])))
            }
            (CapabilityKind::Measure, CapabilityRequest::Measure(_))
                if Some(device) == self.gas =>
            {
                let co2 = self.read_property(device, "co2_actual")?;
                Ok(Value::Map(BTreeMap::from([("co2_actual".into(), co2)])))
            }
            (CapabilityKind::Measure, CapabilityRequest::Measure(_))
                if Some(device) == self.humidity =>
            {
                let humidity = self.read_property(device, "relative_humidity")?;
                Ok(Value::Map(BTreeMap::from([(
                    "relative_humidity".into(),
                    humidity,
                )])))
            }
            (
                CapabilityKind::TemperatureControl,
                CapabilityRequest::TemperatureControl(request),
            ) if Some(device) == self.temperature => {
                if let Some(target) = request.target {
                    self.write_property(device, "target", Value::Temperature(target))?;
                }
                if let Some(enabled) = request.enabled {
                    self.write_property(device, "enabled", Value::Bool(enabled))?;
                }
                Ok(Value::Map(BTreeMap::from([
                    ("target".into(), self.read_property(device, "target")?),
                    ("enabled".into(), self.read_property(device, "enabled")?),
                    ("actual".into(), self.read_property(device, "actual")?),
                    (
                        "completion_basis".into(),
                        Value::String(self.completion_basis()),
                    ),
                ])))
            }
            (CapabilityKind::GasControl, CapabilityRequest::GasControl(request))
                if Some(device) == self.gas =>
            {
                if let Some(target) = request.co2_target {
                    self.write_property(device, "co2_target", Value::GasConcentration(target))?;
                }
                if let Some(enabled) = request.enabled {
                    self.write_property(device, "enabled", Value::Bool(enabled))?;
                }
                Ok(Value::Map(BTreeMap::from([
                    (
                        "co2_target".into(),
                        self.read_property(device, "co2_target")?,
                    ),
                    ("enabled".into(), self.read_property(device, "enabled")?),
                    (
                        "co2_actual".into(),
                        self.read_property(device, "co2_actual")?,
                    ),
                    (
                        "completion_basis".into(),
                        Value::String(self.completion_basis()),
                    ),
                ])))
            }
            (CapabilityKind::GenericCommand, CapabilityRequest::GenericCommand(request))
                if device == self.hub =>
            {
                self.invoke_generic(request)
            }
            (CapabilityKind::Measure, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Okolab Measure expects MeasureRequest",
            )),
            (CapabilityKind::TemperatureControl, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Okolab TemperatureControl expects TemperatureControlRequest",
            )),
            (CapabilityKind::GasControl, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Okolab GasControl expects GasControlRequest",
            )),
            (CapabilityKind::GenericCommand, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Okolab GenericCommand expects GenericCommandRequest",
            )),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Okolab capability",
            )),
        }
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
        if request.command == "refresh_parameter" {
            let parameter = self.parameter_from_request(&request)?;
            return self.refresh_dictionary_parameter(parameter);
        }
        if request.command == "write_parameter" {
            let parameter = self.parameter_from_request(&request)?;
            return self.write_dictionary_parameter(parameter, &request.params);
        }
        if !request.params.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Okolab typed refresh commands do not take parameters",
            ));
        }
        let (device, property) = self.generic_refresh_target(&request.command)?;
        let value = self.read_property(device, property)?;
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("property".into(), Value::String(property.into())),
            ("value".into(), value),
            (
                "completion_basis".into(),
                Value::String(self.completion_basis()),
            ),
        ])))
    }

    fn generic_refresh_target(&self, command: &str) -> Result<(DeviceId, &'static str)> {
        match command {
            "refresh_temperature_actual" => self
                .temperature
                .map(|device| (device, "actual"))
                .ok_or_else(|| Error::new(ErrorCode::Unsupported, "Okolab temperature unavailable")),
            "refresh_temperature_target" => self
                .temperature
                .map(|device| (device, "target"))
                .ok_or_else(|| Error::new(ErrorCode::Unsupported, "Okolab temperature unavailable")),
            "refresh_temperature_status" => self
                .temperature
                .map(|device| (device, "status"))
                .ok_or_else(|| Error::new(ErrorCode::Unsupported, "Okolab temperature unavailable")),
            "refresh_co2_actual" => self
                .gas
                .map(|device| (device, "co2_actual"))
                .ok_or_else(|| Error::new(ErrorCode::Unsupported, "Okolab gas unavailable")),
            "refresh_co2_target" => self
                .gas
                .map(|device| (device, "co2_target"))
                .ok_or_else(|| Error::new(ErrorCode::Unsupported, "Okolab gas unavailable")),
            "refresh_co2_status" => self
                .gas
                .map(|device| (device, "status"))
                .ok_or_else(|| Error::new(ErrorCode::Unsupported, "Okolab gas unavailable")),
            "refresh_o2_actual" if self.o2_available() => self
                .gas
                .map(|device| (device, "o2_actual"))
                .ok_or_else(|| Error::new(ErrorCode::Unsupported, "Okolab gas unavailable")),
            "refresh_o2_target" if self.o2_available() => self
                .gas
                .map(|device| (device, "o2_target"))
                .ok_or_else(|| Error::new(ErrorCode::Unsupported, "Okolab gas unavailable")),
            "refresh_humidity" => self
                .humidity
                .map(|device| (device, "relative_humidity"))
                .ok_or_else(|| Error::new(ErrorCode::Unsupported, "Okolab humidity unavailable")),
            "refresh_humidity_enabled" => self
                .humidity
                .map(|device| (device, "enabled"))
                .ok_or_else(|| Error::new(ErrorCode::Unsupported, "Okolab humidity unavailable")),
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "Okolab GenericCommand supports refresh_temperature_actual, refresh_temperature_target, refresh_temperature_status, refresh_co2_actual, refresh_co2_target, refresh_co2_status, refresh_o2_actual, refresh_o2_target, refresh_humidity, refresh_humidity_enabled, refresh_parameter, and write_parameter; got {other}"
                ),
            )),
        }
    }

    fn parameter_from_request(&self, request: &GenericCommandRequest) -> Result<OkolabParameter> {
        let name = match request
            .params
            .get("parameter")
            .or_else(|| request.params.get("name"))
        {
            Some(Value::String(value)) if !value.trim().is_empty() => value.trim(),
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "Okolab dictionary commands require a string parameter",
                ));
            }
        };
        self.dictionary_parameters
            .iter()
            .find(|parameter| parameter.name == name)
            .cloned()
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidCommand,
                    format!("Okolab parameter is not available for this product: {name}"),
                )
            })
    }

    fn refresh_dictionary_parameter(&mut self, parameter: OkolabParameter) -> Result<Value> {
        if parameter.read_code == 0 {
            return Err(Error::new(
                ErrorCode::Unsupported,
                format!("Okolab parameter {} has no read code", parameter.name),
            ));
        }
        let raw = self.read_live_raw(parameter.read_code, &parameter.name)?;
        let value = raw
            .as_deref()
            .map(|reply| decode_parameter_value(&parameter, reply))
            .transpose()?
            .unwrap_or(Value::Null);
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String("refresh_parameter".into())),
            ("parameter".into(), Value::String(parameter.name)),
            ("read_code".into(), Value::I64(parameter.read_code as i64)),
            ("raw".into(), raw.map(Value::String).unwrap_or(Value::Null)),
            ("value".into(), value),
            (
                "completion_basis".into(),
                Value::String(self.completion_basis()),
            ),
        ])))
    }

    fn write_dictionary_parameter(
        &mut self,
        parameter: OkolabParameter,
        params: &BTreeMap<String, Value>,
    ) -> Result<Value> {
        let volatile = matches!(params.get("volatile"), Some(Value::Bool(true)));
        let code = if volatile {
            parameter.write_code_ram
        } else {
            parameter.write_code
        };
        if code == 0 {
            return Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "Okolab parameter {} has no {}write code",
                    parameter.name,
                    if volatile { "volatile " } else { "" }
                ),
            ));
        }
        let value = params.get("value").ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidCommand,
                "Okolab write_parameter requires a value parameter",
            )
        })?;
        let payload = encode_parameter_value(&parameter, value)?;
        let raw = self.send_or_cache(protocol::CommandFrame::write(code, payload, volatile))?;
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String("write_parameter".into())),
            ("parameter".into(), Value::String(parameter.name)),
            ("write_code".into(), Value::I64(code as i64)),
            ("volatile".into(), Value::Bool(volatile)),
            ("raw".into(), raw.map(Value::String).unwrap_or(Value::Null)),
            (
                "completion_basis".into(),
                Value::String(self.completion_basis()),
            ),
        ])))
    }

    fn read_live_f64(&mut self, code: u16, label: &str) -> Result<Option<f64>> {
        let Some(reply) = self.read_live_raw(code, label)? else {
            return Ok(None);
        };
        let value = reply.trim().parse::<f64>().map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("Okolab {label} reply was not numeric: {error}"),
            )
        })?;
        if !value.is_finite() {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("Okolab {label} reply was not finite"),
            ));
        }
        Ok(Some(value))
    }

    fn read_live_raw(&mut self, code: u16, _label: &str) -> Result<Option<String>> {
        self.send_or_cache(protocol::CommandFrame::read(code))
    }

    fn o2_available(&self) -> bool {
        self.probe.module.gas.is_some()
            && (self.select_o2_parameter().is_some()
                || self.select_o2_setpoint_parameter().is_some()
                || string_looks_o2_capable(&self.probe.module.product))
    }

    fn select_o2_parameter(&self) -> Option<OkolabParameter> {
        select_named_percent_parameter(&self.dictionary_parameters, "O2", true, false)
    }

    fn select_o2_setpoint_parameter(&self) -> Option<OkolabParameter> {
        select_named_percent_parameter(&self.dictionary_parameters, "O2 setpoint", true, true)
    }

    #[cfg(feature = "os-serial")]
    fn refresh_connected_identity(&mut self) -> Result<()> {
        if !self.probe.connect_real_transport {
            return Ok(());
        }
        let Some(reply) = self.read_live_raw(self.probe.module.name_code, "product identity")?
        else {
            return Ok(());
        };
        let reply = reply.trim();
        if reply.is_empty() {
            return Ok(());
        }
        if let Some(product) = match_product_identity(self.probe.module.name_code, reply)? {
            self.probe.module.product = product;
            let (dictionary_status, dictionary_parameters) =
                match load_dictionary_for_product(&self.probe.module.product) {
                    Ok(parameters) => (
                        format!(
                            "loaded {} parameter(s) for {}",
                            parameters.len(),
                            self.probe.module.product
                        ),
                        parameters,
                    ),
                    Err(error) => (format!("unavailable: {error}"), Vec::new()),
                };
            self.dictionary_status = dictionary_status;
            self.dictionary_parameters = dictionary_parameters;
        }
        Ok(())
    }

    fn send_or_cache(&mut self, frame: protocol::CommandFrame) -> Result<Option<String>> {
        let frame = frame.with_checksum(self.probe.checksum_enabled);
        if !self.probe.connect_real_transport {
            return Ok(None);
        }
        #[cfg(feature = "os-serial")]
        {
            let port_name = self.probe.port_name.as_deref().ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    "Okolab connect_real_transport requires a configured port",
                )
            })?;
            let mut port = serialport::new(port_name, protocol::BAUD_PRIMARY)
                .timeout(std::time::Duration::from_millis(500))
                .open()
                .map_err(|error| Error::new(ErrorCode::Transport, error.to_string()))?;
            let encoded = frame.encode()?;
            std::io::Write::write_all(&mut port, &encoded)
                .map_err(|error| Error::new(ErrorCode::Transport, error.to_string()))?;
            std::io::Write::flush(&mut port)
                .map_err(|error| Error::new(ErrorCode::Transport, error.to_string()))?;
            let mut reply = Vec::new();
            let mut byte = [0u8; 1];
            loop {
                match std::io::Read::read(&mut port, &mut byte) {
                    Ok(0) => break,
                    Ok(_) => {
                        reply.push(byte[0]);
                        if protocol::reply_complete(&reply, frame.checksum) {
                            break;
                        }
                    }
                    Err(error) => return Err(Error::new(ErrorCode::Transport, error.to_string())),
                }
            }
            protocol::parse_reply(&frame, &reply).map(Some)
        }
        #[cfg(not(feature = "os-serial"))]
        {
            let _ = frame;
            Err(Error::new(
                ErrorCode::Unsupported,
                "Okolab real serial transport requires the os-serial feature",
            ))
        }
    }

    fn completion_basis(&self) -> String {
        if self.probe.connect_real_transport {
            "serial reply echo".into()
        } else {
            "configured cached value".into()
        }
    }

    fn module_summary(&self) -> Value {
        let mut map = BTreeMap::from([
            (
                "product".into(),
                Value::String(self.probe.module.product.clone()),
            ),
            (
                "name_code".into(),
                Value::I64(self.probe.module.name_code as i64),
            ),
            ("hardware_validated".into(), Value::Bool(false)),
        ]);
        map.insert(
            "temperature".into(),
            Value::Bool(self.temperature.is_some()),
        );
        map.insert("gas".into(), Value::Bool(self.gas.is_some()));
        map.insert("humidity".into(), Value::Bool(self.humidity.is_some()));
        Value::Map(map)
    }

    fn parameter_summary(&self) -> Value {
        let mut map = BTreeMap::from([
            (
                "status".into(),
                Value::String(self.dictionary_status.clone()),
            ),
            (
                "parameter_count".into(),
                Value::I64(self.dictionary_parameters.len() as i64),
            ),
        ]);
        let main = self
            .dictionary_parameters
            .iter()
            .filter(|parameter| parameter.main)
            .take(64)
            .map(parameter_summary_value)
            .collect::<Vec<_>>();
        map.insert("main".into(), Value::List(main));
        Value::Map(map)
    }
}

impl Driver for OkolabDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        self.descriptors_for()
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![
            ResourceDescriptor {
                id: self.serial,
                driver: self.id,
                label: format!("{} serial", self.probe.label),
                kind: "serial.ascii".into(),
                metadata: BTreeMap::from([
                    (
                        "baud_primary".into(),
                        Value::I64(protocol::BAUD_PRIMARY as i64),
                    ),
                    (
                        "baud_fallback".into(),
                        Value::I64(protocol::BAUD_FALLBACK as i64),
                    ),
                    ("terminator".into(), Value::String("CR".into())),
                    (
                        "checksum_enabled".into(),
                        Value::Bool(self.probe.checksum_enabled),
                    ),
                    (
                        "serial_port".into(),
                        self.probe
                            .port_name
                            .as_ref()
                            .map(|port| Value::String(port.clone()))
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "real_transport".into(),
                        Value::Bool(self.probe.connect_real_transport),
                    ),
                    (
                        "connected".into(),
                        Value::Bool(self.probe.connect_real_transport),
                    ),
                ]),
            },
            ResourceDescriptor {
                id: self.database,
                driver: self.id,
                label: "Okolab command database".into(),
                // The driver reads the extract, not the vendor SQLite file, so
                // the advertised resource is what it actually opens.
                kind: "third_party.database.json".into(),
                metadata: BTreeMap::from([
                    ("path".into(), Value::String(OKOLAB_DB_PATH.into())),
                    (
                        "source".into(),
                        Value::String("data/third_party/okolab/okolib.db".into()),
                    ),
                    (
                        "license".into(),
                        Value::String("third-party Okolab data".into()),
                    ),
                ]),
            },
        ]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        let mut capabilities = Vec::new();
        if Some(device) == self.temperature {
            capabilities.push(capability(1, device, CapabilityKind::TemperatureControl));
            capabilities.push(capability(2, device, CapabilityKind::Measure));
        } else if Some(device) == self.gas {
            capabilities.push(capability(1, device, CapabilityKind::GasControl));
            capabilities.push(capability(2, device, CapabilityKind::Measure));
        } else if Some(device) == self.humidity {
            capabilities.push(capability(1, device, CapabilityKind::Measure));
        } else if device == self.hub {
            capabilities.push(capability(1, device, CapabilityKind::GenericCommand));
        }
        capabilities
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut transactions = Vec::new();
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    self.validate_read(*device, key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    transactions.push(self.transaction_for_write(*device, key, value)?);
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    let descriptor = self
                        .capabilities(*device)
                        .into_iter()
                        .find(|candidate| candidate.id == *capability)
                        .ok_or_else(|| {
                            Error::new(ErrorCode::Unsupported, "unknown Okolab capability")
                        })?;
                    if descriptor.kind == CapabilityKind::GenericCommand {
                        let CapabilityRequest::GenericCommand(request) = request else {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Okolab GenericCommand expects GenericCommandRequest",
                            ));
                        };
                        if matches!(
                            request.command.as_str(),
                            "refresh_parameter" | "write_parameter"
                        ) {
                            let parameter = self.parameter_from_request(request)?;
                            if request.command == "refresh_parameter" && parameter.read_code == 0 {
                                return Err(Error::new(
                                    ErrorCode::Unsupported,
                                    format!("Okolab parameter {} has no read code", parameter.name),
                                ));
                            }
                            if request.command == "write_parameter" {
                                let volatile = matches!(
                                    request.params.get("volatile"),
                                    Some(Value::Bool(true))
                                );
                                let code = if volatile {
                                    parameter.write_code_ram
                                } else {
                                    parameter.write_code
                                };
                                if code == 0 {
                                    return Err(Error::new(
                                        ErrorCode::Unsupported,
                                        format!(
                                            "Okolab parameter {} has no write code",
                                            parameter.name
                                        ),
                                    ));
                                }
                                let value = request.params.get("value").ok_or_else(|| {
                                    Error::new(
                                        ErrorCode::InvalidCommand,
                                        "Okolab write_parameter requires a value parameter",
                                    )
                                })?;
                                let _ = encode_parameter_value(&parameter, value)?;
                            }
                        } else {
                            if !request.params.is_empty() {
                                return Err(Error::new(
                                    ErrorCode::InvalidCommand,
                                    "Okolab typed refresh commands do not take parameters",
                                ));
                            }
                            let _ = self.generic_refresh_target(&request.command)?;
                        }
                    }
                    transactions.push(PhysicalTransaction {
                        resource: Some(self.serial),
                        description: "Okolab capability invocation".into(),
                        payload: Value::String(format!("{:?}", request.request_kind())),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                        transactions.push(self.transaction_for_write(
                            write.device,
                            &write.property,
                            &write.value,
                        )?);
                    }
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

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.token();
        let result = self.dispatch_prepared(prepared);
        match result {
            Ok(value) => self
                .events
                .push_back(DriverEvent::TokenCompleted { token, value }),
            Err(error) => self.events.push_back(DriverEvent::TokenFailed {
                token,
                report: error.into(),
            }),
        }
        Ok(token)
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        self.events.drain(..).collect()
    }
}

impl OkolabDriver {
    fn dispatch_prepared(&mut self, prepared: PreparedBatch) -> Result<Value> {
        let mut result = Value::Null;
        for command in prepared.commands {
            result = match command {
                Command::ReadProperty { device, key } => self.read_property(device, &key)?,
                Command::WriteProperty { device, key, value } => {
                    self.write_property(device, &key, value)?
                }
                Command::ApplyStateSet(set) => {
                    let mut values = BTreeMap::new();
                    for write in set.writes {
                        let value =
                            self.write_property(write.device, &write.property, write.value)?;
                        values.insert(format!("{}:{}", write.device.0 .0, write.property), value);
                    }
                    Value::Map(values)
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => self.invoke(device, capability, request)?,
                _ => Value::Null,
            };
        }
        Ok(result)
    }

    fn validate_read(&self, device: DeviceId, key: &str) -> Result<()> {
        if device == self.hub
            && matches!(
                key,
                "model"
                    | "serial_number"
                    | "firmware"
                    | "support_level"
                    | "database_path"
                    | "database_status"
                    | "database_parameter_count"
                    | "name_code"
                    | "checksum_enabled"
                    | "connected"
                    | "fault_active"
                    | "fault"
                    | "module_summary"
                    | "parameter_summary"
            )
        {
            return Ok(());
        }
        if Some(device) == self.temperature
            && matches!(
                key,
                "actual"
                    | "target"
                    | "enabled"
                    | "status"
                    | "status_read_code"
                    | "read_code"
                    | "write_code"
            )
        {
            return Ok(());
        }
        if Some(device) == self.gas
            && matches!(
                key,
                "co2_actual"
                    | "co2_target"
                    | "enabled"
                    | "status"
                    | "co2_status_read_code"
                    | "co2_read_code"
                    | "co2_write_code"
                    | "o2_actual"
                    | "o2_target"
                    | "o2_read_code"
                    | "o2_write_code"
            )
        {
            if key.starts_with("o2_") && !self.o2_available() {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Okolab O2 property is not available for this product",
                ));
            }
            return Ok(());
        }
        if Some(device) == self.humidity
            && matches!(
                key,
                "relative_humidity"
                    | "enabled"
                    | "read_code"
                    | "enabled_read_code"
                    | "enabled_write_code"
            )
        {
            return Ok(());
        }
        Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unknown Okolab readable property {key}"),
        ))
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        match (Some(device), key, value) {
            (device, "target", Value::Temperature(_)) if device == self.temperature => Ok(()),
            (device, "enabled", Value::Bool(_)) if device == self.temperature => {
                self.validate_cached_enable_write("temperature")
            }
            (device, "co2_target", Value::GasConcentration(_)) if device == self.gas => Ok(()),
            (device, "o2_target", Value::GasConcentration(_))
                if device == self.gas && self.o2_available() =>
            {
                Ok(())
            }
            (device, "enabled", Value::Bool(_)) if device == self.gas => self
                .validate_named_enable_write(
                    "gas",
                    select_gas_paused_parameter(&self.dictionary_parameters).is_some(),
                ),
            (device, "enabled", Value::Bool(_)) if device == self.humidity => Ok(()),
            (device, "target", _) if device == self.temperature => Err(Error::new(
                ErrorCode::InvalidProperty,
                "Okolab temperature target expects Temperature",
            )),
            (device, "co2_target", _) if device == self.gas => Err(Error::new(
                ErrorCode::InvalidProperty,
                "Okolab CO2 target expects GasConcentration",
            )),
            (device, "o2_target", _) if device == self.gas && self.o2_available() => {
                Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Okolab O2 target expects GasConcentration",
                ))
            }
            (device, "enabled", _)
                if device == self.temperature || device == self.gas || device == self.humidity =>
            {
                Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Okolab enabled expects Bool",
                ))
            }
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Okolab writable property",
            )),
        }
    }

    fn validate_cached_enable_write(&self, module: &str) -> Result<()> {
        self.validate_named_enable_write(module, false)
    }

    fn validate_named_enable_write(&self, module: &str, has_named_parameter: bool) -> Result<()> {
        if self.probe.connect_real_transport {
            if has_named_parameter {
                return Ok(());
            }
            return Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "Okolab {module} enable writes require command evidence; use target writes or named database parameters"
                ),
            ));
        }
        Ok(())
    }

    fn transaction_for_write(
        &self,
        device: DeviceId,
        key: &str,
        value: &Value,
    ) -> Result<PhysicalTransaction> {
        let frame = if Some(device) == self.temperature && key == "target" {
            let temp = self.probe.module.temperature.as_ref().ok_or_else(|| {
                Error::new(ErrorCode::Unsupported, "Okolab temperature unavailable")
            })?;
            let Value::Temperature(value) = value else {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "target expects Temperature",
                ));
            };
            protocol::CommandFrame::write(
                temp.target_write_code,
                format!("{:.3}", value.celsius()),
                true,
            )
            .with_checksum(self.probe.checksum_enabled)
        } else if Some(device) == self.gas && key == "co2_target" {
            let gas = self
                .probe
                .module
                .gas
                .as_ref()
                .ok_or_else(|| Error::new(ErrorCode::Unsupported, "Okolab gas unavailable"))?;
            let Value::GasConcentration(value) = value else {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "co2_target expects GasConcentration",
                ));
            };
            protocol::CommandFrame::write(
                gas.co2_target_write_code,
                format!("{:.3}", value.fraction() * 100.0),
                true,
            )
            .with_checksum(self.probe.checksum_enabled)
        } else if Some(device) == self.gas && key == "o2_target" && self.o2_available() {
            let gas = self
                .probe
                .module
                .gas
                .as_ref()
                .ok_or_else(|| Error::new(ErrorCode::Unsupported, "Okolab gas unavailable"))?;
            let Value::GasConcentration(value) = value else {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "o2_target expects GasConcentration",
                ));
            };
            let code = self
                .select_o2_setpoint_parameter()
                .map(|parameter| parameter.write_code)
                .unwrap_or(gas.o2_target_write_code);
            protocol::CommandFrame::write(code, format!("{:.3}", value.fraction() * 100.0), true)
                .with_checksum(self.probe.checksum_enabled)
        } else if Some(device) == self.gas && key == "enabled" {
            let Some(parameter) = select_gas_paused_parameter(&self.dictionary_parameters) else {
                return Ok(PhysicalTransaction {
                    resource: Some(self.serial),
                    description: "Okolab cached gas state update".into(),
                    payload: Value::String(key.into()),
                });
            };
            let Value::Bool(value) = value else {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "enabled expects Bool",
                ));
            };
            protocol::CommandFrame::write(
                parameter.write_code,
                if *value { "0" } else { "1" },
                false,
            )
            .with_checksum(self.probe.checksum_enabled)
        } else if Some(device) == self.humidity && key == "enabled" {
            let Some(parameter) = select_humidity_enabled_parameter(&self.dictionary_parameters)
            else {
                return Ok(PhysicalTransaction {
                    resource: Some(self.serial),
                    description: "Okolab cached humidity state update".into(),
                    payload: Value::String(key.into()),
                });
            };
            let Value::Bool(value) = value else {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "enabled expects Bool",
                ));
            };
            protocol::CommandFrame::write(
                parameter.write_code,
                if *value { "1" } else { "0" },
                false,
            )
            .with_checksum(self.probe.checksum_enabled)
        } else {
            return Ok(PhysicalTransaction {
                resource: Some(self.serial),
                description: "Okolab cached state update".into(),
                payload: Value::String(key.into()),
            });
        };
        Ok(PhysicalTransaction {
            resource: Some(self.serial),
            description: "Okolab serial write".into(),
            payload: Value::Bytes(frame.encode()?),
        })
    }
}

/// The embedded dictionary, parsed once.
///
/// The JSON is compiled in, so a parse failure is a corrupt-extract bug rather
/// than a runtime condition — but it is still surfaced as an error so the
/// driver degrades exactly as it did when the database could not be opened.
fn dictionary() -> Result<&'static OkolabDictionary> {
    static DICTIONARY: OnceLock<std::result::Result<OkolabDictionary, String>> = OnceLock::new();
    DICTIONARY
        .get_or_init(|| {
            serde_json::from_str::<OkolabDictionaryFile>(OKOLAB_DICTIONARY_JSON)
                .map(OkolabDictionary::new)
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("Okolab command dictionary could not be parsed: {error}"),
            )
        })
}

/// Shape of `data/third_party/okolab/okolib.json`.
#[derive(Debug, serde::Deserialize)]
struct OkolabDictionaryFile {
    products: Vec<DictionaryProduct>,
    parameters: Vec<DictionaryParameter>,
    enums: Vec<DictionaryEnum>,
}

#[derive(Debug, serde::Deserialize)]
struct DictionaryProduct {
    /// `null` for products the vendor database leaves unnamed; those can never
    /// be matched by name or code, exactly as in the original SQL.
    name: Option<String>,
    // Identity matching is the serial transport's job, so without it these are
    // parsed but unused.
    #[cfg_attr(not(feature = "os-serial"), allow(dead_code))]
    name_code: Option<i64>,
    #[cfg_attr(not(feature = "os-serial"), allow(dead_code))]
    code_alt: Option<i64>,
    #[cfg_attr(not(feature = "os-serial"), allow(dead_code))]
    alt_names: Vec<String>,
    /// Already resolved (shared `-1` parameters folded in, unresolvable rows
    /// dropped, deduped) and already ordered `main DESC, name ASC` — so this is
    /// a lookup, not a join.
    parameter_ids: Vec<i64>,
}

#[derive(Debug, serde::Deserialize)]
struct DictionaryParameter {
    id: i64,
    name: String,
    unit: Option<String>,
    description: Option<String>,
    var_type: i64,
    main: i64,
    advanced: i64,
    oneshot: i64,
    read_code: i64,
    write_code: i64,
    write_code_ram: i64,
    min_code: i64,
    max_code: i64,
    enum_type_id: i64,
}

#[derive(Debug, serde::Deserialize)]
struct DictionaryEnum {
    enum_type_id: i64,
    values: Vec<DictionaryEnumValue>,
}

#[derive(Debug, serde::Deserialize)]
struct DictionaryEnumValue {
    value: i64,
    name: String,
}

/// The parsed dictionary with by-id indexes built once.
#[derive(Debug)]
struct OkolabDictionary {
    products: Vec<DictionaryProduct>,
    parameters: BTreeMap<i64, DictionaryParameter>,
    enums: BTreeMap<i64, BTreeMap<i64, String>>,
}

impl OkolabDictionary {
    fn new(file: OkolabDictionaryFile) -> Self {
        Self {
            products: file.products,
            parameters: file
                .parameters
                .into_iter()
                .map(|parameter| (parameter.id, parameter))
                .collect(),
            enums: file
                .enums
                .into_iter()
                .map(|entry| {
                    let values = entry
                        .values
                        .into_iter()
                        .map(|value| (value.value, value.name))
                        .collect();
                    (entry.enum_type_id, values)
                })
                .collect(),
        }
    }

    fn enum_values(&self, enum_type_id: i64) -> BTreeMap<i64, String> {
        self.enums.get(&enum_type_id).cloned().unwrap_or_default()
    }
}

fn load_dictionary_for_product(product_name: &str) -> Result<Vec<OkolabParameter>> {
    let dictionary = dictionary()?;
    let product = dictionary
        .products
        .iter()
        .find(|product| product.name.as_deref() == Some(product_name))
        .ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("Okolab product {product_name} is not in the command database"),
            )
        })?;
    let mut parameters = Vec::with_capacity(product.parameter_ids.len());
    for id in &product.parameter_ids {
        let Some(entry) = dictionary.parameters.get(id) else {
            continue;
        };
        parameters.push(OkolabParameter {
            name: entry.name.clone(),
            unit: entry.unit.clone(),
            description: entry.description.clone(),
            var_type: entry.var_type,
            main: int_to_bool(entry.main),
            advanced: int_to_bool(entry.advanced),
            oneshot: int_to_bool(entry.oneshot),
            read_code: db_code(entry.read_code),
            write_code: db_code(entry.write_code),
            write_code_ram: db_code(entry.write_code_ram),
            min_code: db_code(entry.min_code),
            max_code: db_code(entry.max_code),
            enum_values: if entry.enum_type_id != 0 {
                dictionary.enum_values(entry.enum_type_id)
            } else {
                BTreeMap::new()
            },
        });
    }
    Ok(parameters)
}

#[cfg(feature = "os-serial")]
fn match_product_identity(name_code: u16, reply: &str) -> Result<Option<String>> {
    let dictionary = dictionary()?;
    let code = name_code as i64;
    // `ORDER BY Product.name ASC LIMIT 1` over products matching either code
    // and either the canonical name or one of its alternates.
    Ok(dictionary
        .products
        .iter()
        .filter_map(|product| product.name.as_deref().map(|name| (product, name)))
        .filter(|(product, name)| {
            (product.name_code == Some(code) || product.code_alt == Some(code))
                && (*name == reply || product.alt_names.iter().any(|alt| alt == reply))
        })
        .map(|(_, name)| name)
        .min()
        .map(str::to_string))
}

fn db_code(value: i64) -> u16 {
    if (0..=u16::MAX as i64).contains(&value) {
        value as u16
    } else {
        0
    }
}

fn int_to_bool(value: i64) -> bool {
    value != 0
}

fn parameter_summary_value(parameter: &OkolabParameter) -> Value {
    let mut map = BTreeMap::from([
        ("name".into(), Value::String(parameter.name.clone())),
        ("var_type".into(), Value::I64(parameter.var_type)),
        ("read_code".into(), Value::I64(parameter.read_code as i64)),
        ("write_code".into(), Value::I64(parameter.write_code as i64)),
        (
            "write_code_ram".into(),
            Value::I64(parameter.write_code_ram as i64),
        ),
        ("main".into(), Value::Bool(parameter.main)),
        ("advanced".into(), Value::Bool(parameter.advanced)),
        ("oneshot".into(), Value::Bool(parameter.oneshot)),
    ]);
    if let Some(unit) = &parameter.unit {
        map.insert("unit".into(), Value::String(unit.clone()));
    }
    if let Some(description) = &parameter.description {
        map.insert("description".into(), Value::String(description.clone()));
    }
    if parameter.min_code != 0 || parameter.max_code != 0 {
        map.insert("min_code".into(), Value::I64(parameter.min_code as i64));
        map.insert("max_code".into(), Value::I64(parameter.max_code as i64));
    }
    if !parameter.enum_values.is_empty() {
        map.insert(
            "enum_values".into(),
            Value::Map(
                parameter
                    .enum_values
                    .iter()
                    .map(|(value, name)| (value.to_string(), Value::String(name.clone())))
                    .collect(),
            ),
        );
    }
    Value::Map(map)
}

fn select_humidity_parameter(parameters: &[OkolabParameter]) -> Option<OkolabParameter> {
    const PREFERRED: &[&str] = &[
        "Humidity",
        "Input gas Humidity",
        "Sensing cell sensor humidity",
    ];
    PREFERRED.iter().find_map(|name| {
        parameters
            .iter()
            .find(|parameter| {
                parameter.name == *name
                    && parameter.unit.as_deref() == Some("%")
                    && parameter.read_code != 0
            })
            .cloned()
    })
}

fn select_humidity_enabled_parameter(parameters: &[OkolabParameter]) -> Option<OkolabParameter> {
    const PREFERRED: &[&str] = &["Humidity control", "HM activation status"];
    PREFERRED.iter().find_map(|name| {
        parameters
            .iter()
            .find(|parameter| {
                parameter.name == *name && parameter.read_code != 0 && parameter.write_code != 0
            })
            .cloned()
    })
}

fn select_named_percent_parameter(
    parameters: &[OkolabParameter],
    name: &str,
    require_read: bool,
    require_write: bool,
) -> Option<OkolabParameter> {
    parameters
        .iter()
        .find(|parameter| {
            parameter.name == name
                && parameter.unit.as_deref() == Some("%")
                && (!require_read || parameter.read_code != 0)
                && (!require_write || parameter.write_code != 0)
        })
        .cloned()
}

fn select_gas_paused_parameter(parameters: &[OkolabParameter]) -> Option<OkolabParameter> {
    parameters
        .iter()
        .find(|parameter| {
            parameter.name == "Gas control paused"
                && parameter.read_code != 0
                && parameter.write_code != 0
        })
        .cloned()
}

fn string_looks_o2_capable(value: &str) -> bool {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
        .contains("o2")
}

fn parse_okolab_boolish(raw: &str, label: &str) -> Result<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "enabled" | "true" | "on" => Ok(true),
        "0" | "disabled" | "false" | "off" => Ok(false),
        other => Err(Error::new(
            ErrorCode::Transport,
            format!("Okolab {label} reply was not a boolean value: {other}"),
        )),
    }
}

fn decode_parameter_value(parameter: &OkolabParameter, raw: &str) -> Result<Value> {
    match parameter.var_type {
        2 => raw.trim().parse::<i64>().map(Value::I64).map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!(
                    "Okolab parameter {} reply was not an integer: {error}",
                    parameter.name
                ),
            )
        }),
        3 => raw.trim().parse::<f64>().map(Value::F64).map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!(
                    "Okolab parameter {} reply was not a float: {error}",
                    parameter.name
                ),
            )
        }),
        4 => {
            let value = raw.trim().parse::<i64>().map_err(|error| {
                Error::new(
                    ErrorCode::Transport,
                    format!(
                        "Okolab parameter {} reply was not an enum integer: {error}",
                        parameter.name
                    ),
                )
            })?;
            Ok(parameter
                .enum_values
                .get(&value)
                .map(|name| Value::String(name.clone()))
                .unwrap_or(Value::I64(value)))
        }
        _ => Ok(Value::String(raw.to_string())),
    }
}

fn encode_parameter_value(parameter: &OkolabParameter, value: &Value) -> Result<String> {
    match (parameter.var_type, value) {
        (2, Value::I64(value)) => Ok(value.to_string()),
        (2, Value::F64(value)) if value.fract() == 0.0 => Ok((*value as i64).to_string()),
        (3, Value::F64(value)) => Ok(format!("{value:.3}")),
        (3, Value::I64(value)) => Ok(format!("{:.3}", *value as f64)),
        (4, Value::I64(value)) => Ok(value.to_string()),
        (4, Value::String(name)) => parameter
            .enum_values
            .iter()
            .find(|(_, candidate)| *candidate == name)
            .map(|(value, _)| value.to_string())
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!(
                        "Okolab enum value {name} is not valid for {}",
                        parameter.name
                    ),
                )
            }),
        (_, Value::String(value)) => Ok(value.clone()),
        (_, other) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!(
                "Okolab parameter {} cannot encode value {other:?} for var_type {}",
                parameter.name, parameter.var_type
            ),
        )),
    }
}

trait WritableSchema {
    fn writable(self) -> Self;
}

impl WritableSchema for PropertySchema {
    fn writable(mut self) -> Self {
        self.writable = true;
        self
    }
}

fn capability(index: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    CapabilityDescriptor::new(
        CapabilityId(device.0 .0 * 100 + index),
        device,
        kind,
        ValueType::Map,
    )
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

fn invalid_property(message: &str, key: &str) -> Result<Value> {
    Err(Error::new(
        ErrorCode::InvalidProperty,
        format!("{message}: {key}"),
    ))
}

fn string_prop(device: &DeviceConfig, key: &str) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn optional_string_prop(
    device: &DeviceConfig,
    key: &str,
    default: Option<String>,
) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        Some(Value::Null) => None,
        _ => default,
    }
}

fn bool_prop(device: &DeviceConfig, key: &str) -> Option<bool> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn optional_bool_prop(device: &DeviceConfig, key: &str, default: Option<bool>) -> Option<bool> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        Some(Value::Null) => None,
        _ => default,
    }
}

fn f64_prop(device: &DeviceConfig, key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn optional_f64_prop(device: &DeviceConfig, key: &str, default: Option<f64>) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        Some(Value::Null) => None,
        _ => default,
    }
}

fn u16_prop(device: &DeviceConfig, key: &str) -> Result<Option<u16>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if (0..=u16::MAX as i64).contains(value) => Ok(Some(*value as u16)),
        Some(Value::I64(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Okolab property {key} must fit in an unsigned 16-bit integer"),
        )),
        Some(Value::String(value)) => value.parse().map(Some).map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("Okolab property {key} must be an unsigned 16-bit integer"),
            )
        }),
        _ => Ok(None),
    }
}
