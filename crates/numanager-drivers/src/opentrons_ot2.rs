use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct OpentronsOt2ConfiguredProbe {
    label: String,
    host: String,
    port: u16,
    connect_real_transport: bool,
    connect_timeout_ms: u64,
    response_timeout_ms: u64,
    api_version: String,
    server_version: String,
    robot_serial: String,
    robot_type: String,
    status: String,
    door_open: bool,
    current_run: String,
    module_count: i64,
    run_count: i64,
    command_count: i64,
    current_command: String,
    current_command_status: String,
    module_inventory_state: String,
    run_inventory_state: String,
    last_http_status: String,
    left_pipette_model: Option<String>,
    left_pipette_serial: Option<String>,
    right_pipette_model: Option<String>,
    right_pipette_serial: Option<String>,
    camera_present: bool,
    module_model: Option<String>,
    module_serial: Option<String>,
    module_status: String,
    module_temperature: Temperature,
    module_target_temperature: Temperature,
    gantry_mount: String,
    gantry_x: Position,
    gantry_y: Position,
    gantry_z: Position,
    gantry_homed: bool,
}

pub struct OpentronsOt2Discovery {
    next_id: DriverId,
    probes: Vec<OpentronsOt2ConfiguredProbe>,
}

impl OpentronsOt2Discovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![OpentronsOt2ConfiguredProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), "opentrons_ot2" | "opentrons-ot2"))
            .map(OpentronsOt2ConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for OpentronsOt2Discovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, mut configured)| {
                if configured.connect_real_transport {
                    configured.refresh_health()?;
                }
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                Ok(DriverCandidate::from_driver(
                    label,
                    Box::new(OpentronsOt2Driver::configured(id, configured)),
                ))
            })
            .collect()
    }
}

impl OpentronsOt2ConfiguredProbe {
    pub fn fixture() -> Self {
        Self {
            label: "Configured Opentrons OT-2 robot".into(),
            host: "opentrons-ot2.local".into(),
            port: 31_950,
            connect_real_transport: false,
            connect_timeout_ms: 1_000,
            response_timeout_ms: 2_000,
            api_version: "2".into(),
            server_version: "configured".into(),
            robot_serial: "OT2-CONFIG-0001".into(),
            robot_type: "OT-2".into(),
            status: "idle".into(),
            door_open: false,
            current_run: "none".into(),
            module_count: 1,
            run_count: 0,
            command_count: 0,
            current_command: "none".into(),
            current_command_status: "unknown".into(),
            module_inventory_state: "configured".into(),
            run_inventory_state: "configured".into(),
            last_http_status: "not_connected".into(),
            left_pipette_model: Some("p300_single_gen2".into()),
            left_pipette_serial: Some("PIP-L-CONFIG-0001".into()),
            right_pipette_model: None,
            right_pipette_serial: None,
            camera_present: true,
            module_model: Some("temperatureModuleV2".into()),
            module_serial: Some("TEMP-MOD-CONFIG-0001".into()),
            module_status: "idle".into(),
            module_temperature: Temperature::from_celsius(22.0),
            module_target_temperature: Temperature::from_celsius(4.0),
            gantry_mount: "left".into(),
            gantry_x: Position::from_millimeters(0.0),
            gantry_y: Position::from_millimeters(0.0),
            gantry_z: Position::from_millimeters(0.0),
            gantry_homed: false,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::fixture();
        if !device.label.is_empty() {
            configured.label = device.label.clone();
        }
        configured.host = host_prop(device, "host")?.unwrap_or(configured.host);
        configured.port = u16_prop(device, "port")?.unwrap_or(configured.port);
        configured.connect_real_transport =
            bool_prop(device, "connect").unwrap_or(configured.connect_real_transport);
        configured.connect_timeout_ms =
            u64_prop(device, "connect_timeout_ms")?.unwrap_or(configured.connect_timeout_ms);
        configured.response_timeout_ms =
            u64_prop(device, "response_timeout_ms")?.unwrap_or(configured.response_timeout_ms);
        configured.api_version =
            api_version_prop(device, "api_version")?.unwrap_or(configured.api_version);
        configured.server_version =
            string_prop(device, "server_version").unwrap_or(configured.server_version);
        configured.robot_serial =
            string_prop(device, "robot_serial").unwrap_or(configured.robot_serial);
        configured.robot_type = string_prop(device, "robot_type").unwrap_or(configured.robot_type);
        configured.status = string_prop(device, "status").unwrap_or(configured.status);
        configured.door_open = bool_prop(device, "door_open").unwrap_or(configured.door_open);
        configured.current_run =
            string_prop(device, "current_run").unwrap_or(configured.current_run);
        configured.module_count =
            i64_prop(device, "module_count")?.unwrap_or(configured.module_count);
        configured.run_count = i64_prop(device, "run_count")?.unwrap_or(configured.run_count);
        configured.command_count =
            i64_prop(device, "command_count")?.unwrap_or(configured.command_count);
        configured.current_command =
            string_prop(device, "current_command").unwrap_or(configured.current_command);
        configured.current_command_status = string_prop(device, "current_command_status")
            .unwrap_or(configured.current_command_status);
        configured.module_inventory_state = string_prop(device, "module_inventory_state")
            .unwrap_or(configured.module_inventory_state);
        configured.run_inventory_state =
            string_prop(device, "run_inventory_state").unwrap_or(configured.run_inventory_state);
        configured.last_http_status =
            string_prop(device, "last_http_status").unwrap_or(configured.last_http_status);
        configured.left_pipette_model =
            optional_string_prop(device, "left_pipette_model", configured.left_pipette_model);
        configured.left_pipette_serial = optional_string_prop(
            device,
            "left_pipette_serial",
            configured.left_pipette_serial,
        );
        configured.right_pipette_model = optional_string_prop(
            device,
            "right_pipette_model",
            configured.right_pipette_model,
        );
        configured.right_pipette_serial = optional_string_prop(
            device,
            "right_pipette_serial",
            configured.right_pipette_serial,
        );
        configured.camera_present =
            bool_prop(device, "camera_present").unwrap_or(configured.camera_present);
        configured.module_model =
            optional_string_prop(device, "module_model", configured.module_model);
        configured.module_serial =
            optional_string_prop(device, "module_serial", configured.module_serial);
        configured.module_status =
            string_prop(device, "module_status").unwrap_or(configured.module_status);
        configured.module_temperature =
            temperature_prop(device, "module_temperature").unwrap_or(configured.module_temperature);
        configured.module_target_temperature =
            temperature_prop(device, "module_target_temperature")
                .unwrap_or(configured.module_target_temperature);
        configured.gantry_mount =
            mount_prop(device, "gantry_mount")?.unwrap_or(configured.gantry_mount);
        configured.gantry_x = position_prop(device, "gantry_x").unwrap_or(configured.gantry_x);
        configured.gantry_y = position_prop(device, "gantry_y").unwrap_or(configured.gantry_y);
        configured.gantry_z = position_prop(device, "gantry_z").unwrap_or(configured.gantry_z);
        configured.gantry_homed =
            bool_prop(device, "gantry_homed").unwrap_or(configured.gantry_homed);
        Ok(configured)
    }

    fn refresh_health(&mut self) -> Result<()> {
        let response = http_get_health(
            &self.host,
            self.port,
            &self.api_version,
            self.connect_timeout_ms,
            self.response_timeout_ms,
        )?;
        self.server_version = first_json_string(
            &response.body,
            &["server_version", "serverVersion", "version"],
        )
        .unwrap_or_else(|| format!("http {}", response.status_code));
        if let Some(serial) =
            first_json_string(&response.body, &["robot_serial", "robotSerial", "serial"])
        {
            self.robot_serial = serial;
        }
        if let Some(robot_type) =
            first_json_string(&response.body, &["robot_type", "robotType", "name"])
        {
            self.robot_type = robot_type;
        }
        self.status = if (200..300).contains(&response.status_code) {
            "idle".into()
        } else {
            format!("health_http_{}", response.status_code)
        };
        self.last_http_status = format!("GET /health {}", response.status_code);
        Ok(())
    }

    fn refresh_inventory(&mut self) -> Result<OpentronsInventoryRefresh> {
        let modules = http_get_json(
            &self.host,
            self.port,
            &self.api_version,
            "/modules",
            self.connect_timeout_ms,
            self.response_timeout_ms,
        )?;
        self.module_count = top_level_data_array_count(&modules.body)
            .map(|count| count as i64)
            .unwrap_or(self.module_count);
        self.module_inventory_state = if (200..300).contains(&modules.status_code) {
            "http_refreshed".into()
        } else {
            format!("http_{}", modules.status_code)
        };
        if (200..300).contains(&modules.status_code) && self.module_count > 0 {
            if let Some(model) =
                first_json_string(&modules.body, &["moduleModel", "model", "moduleType"])
            {
                self.module_model = Some(model);
            }
            if let Some(serial) =
                first_json_string(&modules.body, &["serialNumber", "serial", "serialNo"])
            {
                self.module_serial = Some(serial);
            }
            if let Some(status) = first_json_string(&modules.body, &["status", "state"]) {
                self.module_status = status;
            }
            if let Some(temperature) =
                first_json_number(&modules.body, &["currentTemperature", "temperature"])
            {
                self.module_temperature = Temperature::from_celsius(temperature);
            }
            if let Some(target) = first_json_number(&modules.body, &["targetTemperature", "target"])
            {
                self.module_target_temperature = Temperature::from_celsius(target);
            }
        }

        let runs = http_get_json(
            &self.host,
            self.port,
            &self.api_version,
            "/runs",
            self.connect_timeout_ms,
            self.response_timeout_ms,
        )?;
        self.run_count = top_level_data_array_count(&runs.body)
            .map(|count| count as i64)
            .unwrap_or(self.run_count);
        if let Some(run_id) = first_json_string(&runs.body, &["id", "runId"]) {
            self.current_run = run_id;
        } else if self.run_count == 0 {
            self.current_run = "none".into();
        }
        self.run_inventory_state = if (200..300).contains(&runs.status_code) {
            "http_refreshed".into()
        } else {
            format!("http_{}", runs.status_code)
        };
        self.last_http_status = format!(
            "GET /modules {}; GET /runs {}",
            modules.status_code, runs.status_code
        );

        Ok(OpentronsInventoryRefresh {
            modules_status: modules.status_code,
            runs_status: runs.status_code,
            module_count: self.module_count,
            run_count: self.run_count,
            current_run: self.current_run.clone(),
            module_model: self.module_model.clone(),
            module_serial: self.module_serial.clone(),
            module_status: self.module_status.clone(),
            module_temperature: self.module_temperature,
            module_target_temperature: self.module_target_temperature,
        })
    }

    fn refresh_run_commands(&mut self) -> Result<OpentronsCommandRefresh> {
        if self.current_run == "none" || self.current_run.trim().is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Opentrons OT-2 refresh_run_commands requires a current run id",
            ));
        }
        let path = format!("/runs/{}/commands?pageLength=20", self.current_run);
        let response = http_get_json(
            &self.host,
            self.port,
            &self.api_version,
            &path,
            self.connect_timeout_ms,
            self.response_timeout_ms,
        )?;
        self.command_count = top_level_data_array_count(&response.body)
            .map(|count| count as i64)
            .unwrap_or(self.command_count);
        self.current_command = first_json_string(&response.body, &["id", "commandId"])
            .unwrap_or_else(|| {
                if self.command_count == 0 {
                    "none".into()
                } else {
                    self.current_command.clone()
                }
            });
        self.current_command_status = first_json_string(&response.body, &["status"])
            .unwrap_or_else(|| {
                if self.command_count == 0 {
                    "none".into()
                } else {
                    self.current_command_status.clone()
                }
            });
        self.run_inventory_state = if (200..300).contains(&response.status_code) {
            "run_commands_refreshed".into()
        } else {
            format!("http_{}", response.status_code)
        };
        self.last_http_status = format!("GET {path} {}", response.status_code);

        Ok(OpentronsCommandRefresh {
            http_status: response.status_code,
            command_count: self.command_count,
            current_command: self.current_command.clone(),
            current_command_status: self.current_command_status.clone(),
        })
    }
}

struct OpentronsInventoryRefresh {
    modules_status: u16,
    runs_status: u16,
    module_count: i64,
    run_count: i64,
    current_run: String,
    module_model: Option<String>,
    module_serial: Option<String>,
    module_status: String,
    module_temperature: Temperature,
    module_target_temperature: Temperature,
}

struct OpentronsCommandRefresh {
    http_status: u16,
    command_count: i64,
    current_command: String,
    current_command_status: String,
}

pub struct OpentronsOt2Driver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    gantry: DeviceId,
    left_pipette: Option<DeviceId>,
    right_pipette: Option<DeviceId>,
    deck: DeviceId,
    camera: Option<DeviceId>,
    module: Option<DeviceId>,
    configured: OpentronsOt2ConfiguredProbe,
    next_token: u64,
    events: VecDeque<DriverEvent>,
}

impl OpentronsOt2Driver {
    pub fn configured(id: DriverId, configured: OpentronsOt2ConfiguredProbe) -> Self {
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 970)),
            hub: DeviceId(NodeId(id.0 * 1000 + 971)),
            gantry: DeviceId(NodeId(id.0 * 1000 + 972)),
            left_pipette: configured
                .left_pipette_model
                .as_ref()
                .map(|_| DeviceId(NodeId(id.0 * 1000 + 973))),
            right_pipette: configured
                .right_pipette_model
                .as_ref()
                .map(|_| DeviceId(NodeId(id.0 * 1000 + 974))),
            deck: DeviceId(NodeId(id.0 * 1000 + 975)),
            camera: configured
                .camera_present
                .then_some(DeviceId(NodeId(id.0 * 1000 + 976))),
            module: configured
                .module_model
                .as_ref()
                .map(|_| DeviceId(NodeId(id.0 * 1000 + 977))),
            configured,
            next_token: 1,
            events: VecDeque::new(),
        }
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "host" => Ok(Value::String(self.configured.host.clone())),
                "port" => Ok(Value::I64(self.configured.port as i64)),
                "api_version" => Ok(Value::String(self.configured.api_version.clone())),
                "server_version" => Ok(Value::String(self.configured.server_version.clone())),
                "robot_serial" => Ok(Value::String(self.configured.robot_serial.clone())),
                "robot_type" => Ok(Value::String(self.configured.robot_type.clone())),
                "status" => Ok(Value::String(self.configured.status.clone())),
                "door_open" => Ok(Value::Bool(self.configured.door_open)),
                "current_run" => Ok(Value::String(self.configured.current_run.clone())),
                "module_count" => Ok(Value::I64(self.configured.module_count)),
                "run_count" => Ok(Value::I64(self.configured.run_count)),
                "command_count" => Ok(Value::I64(self.configured.command_count)),
                "current_command" => Ok(Value::String(self.configured.current_command.clone())),
                "current_command_status" => Ok(Value::String(
                    self.configured.current_command_status.clone(),
                )),
                "module_inventory_state" => Ok(Value::String(
                    self.configured.module_inventory_state.clone(),
                )),
                "run_inventory_state" => {
                    Ok(Value::String(self.configured.run_inventory_state.clone()))
                }
                "last_http_status" => Ok(Value::String(self.configured.last_http_status.clone())),
                "ready" => Ok(Value::Bool(self.configured.status == "idle")),
                _ => invalid_property("unknown Opentrons hub property", key),
            };
        }
        if device == self.gantry {
            return match key {
                "homed" => Ok(Value::Bool(self.configured.gantry_homed)),
                "status" => Ok(Value::String(self.configured.status.clone())),
                "mount" => Ok(Value::String(self.configured.gantry_mount.clone())),
                "x" => Ok(Value::Position(self.configured.gantry_x)),
                "y" => Ok(Value::Position(self.configured.gantry_y)),
                "z" => Ok(Value::Position(self.configured.gantry_z)),
                _ => invalid_property("unknown Opentrons gantry property", key),
            };
        }
        if self.left_pipette == Some(device) {
            return match key {
                "mount" => Ok(Value::String("left".into())),
                "model" => Ok(Value::String(
                    self.configured
                        .left_pipette_model
                        .clone()
                        .unwrap_or_default(),
                )),
                "serial" => Ok(Value::String(
                    self.configured
                        .left_pipette_serial
                        .clone()
                        .unwrap_or_default(),
                )),
                "has_tip" => Ok(Value::Bool(false)),
                _ => invalid_property("unknown Opentrons pipette property", key),
            };
        }
        if self.right_pipette == Some(device) {
            return match key {
                "mount" => Ok(Value::String("right".into())),
                "model" => Ok(Value::String(
                    self.configured
                        .right_pipette_model
                        .clone()
                        .unwrap_or_default(),
                )),
                "serial" => Ok(Value::String(
                    self.configured
                        .right_pipette_serial
                        .clone()
                        .unwrap_or_default(),
                )),
                "has_tip" => Ok(Value::Bool(false)),
                _ => invalid_property("unknown Opentrons pipette property", key),
            };
        }
        if device == self.deck {
            return match key {
                "loaded_labware" => Ok(Value::I64(0)),
                "loaded_modules" => Ok(Value::I64(self.configured.module_count)),
                _ => invalid_property("unknown Opentrons deck property", key),
            };
        }
        if self.camera == Some(device) {
            return match key {
                "available" => Ok(Value::Bool(true)),
                "snapshot_supported" => Ok(Value::Bool(true)),
                _ => invalid_property("unknown Opentrons camera property", key),
            };
        }
        if self.module == Some(device) {
            return match key {
                "model" => Ok(Value::String(
                    self.configured.module_model.clone().unwrap_or_default(),
                )),
                "serial" => Ok(Value::String(
                    self.configured.module_serial.clone().unwrap_or_default(),
                )),
                "temperature" => Ok(Value::Temperature(self.configured.module_temperature)),
                "target_temperature" => Ok(Value::Temperature(
                    self.configured.module_target_temperature,
                )),
                "enabled" => Ok(Value::Bool(self.configured.module_status != "idle")),
                "status" => Ok(Value::String(self.configured.module_status.clone())),
                _ => invalid_property("unknown Opentrons module property", key),
            };
        }
        Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("unknown Opentrons device {device:?}"),
        ))
    }

    fn refresh_health(&mut self) -> Result<Value> {
        let before_server = self.configured.server_version.clone();
        let before_serial = self.configured.robot_serial.clone();
        let before_type = self.configured.robot_type.clone();
        let before_status = self.configured.status.clone();
        let before_last_http_status = self.configured.last_http_status.clone();
        self.configured.refresh_health()?;
        if self.configured.server_version != before_server {
            self.emit_hub_property(
                "server_version",
                Value::String(self.configured.server_version.clone()),
            );
        }
        if self.configured.robot_serial != before_serial {
            self.emit_hub_property(
                "robot_serial",
                Value::String(self.configured.robot_serial.clone()),
            );
        }
        if self.configured.robot_type != before_type {
            self.emit_hub_property(
                "robot_type",
                Value::String(self.configured.robot_type.clone()),
            );
        }
        if self.configured.status != before_status {
            self.emit_hub_property("status", Value::String(self.configured.status.clone()));
            self.emit_hub_property("ready", Value::Bool(self.configured.status == "idle"));
        }
        if self.configured.last_http_status != before_last_http_status {
            self.emit_hub_property(
                "last_http_status",
                Value::String(self.configured.last_http_status.clone()),
            );
        }
        Ok(Value::Map(BTreeMap::from([
            (
                "server_version".into(),
                Value::String(self.configured.server_version.clone()),
            ),
            (
                "robot_serial".into(),
                Value::String(self.configured.robot_serial.clone()),
            ),
            (
                "robot_type".into(),
                Value::String(self.configured.robot_type.clone()),
            ),
            (
                "status".into(),
                Value::String(self.configured.status.clone()),
            ),
            (
                "ready".into(),
                Value::Bool(self.configured.status == "idle"),
            ),
            (
                "last_http_status".into(),
                Value::String(self.configured.last_http_status.clone()),
            ),
        ])))
    }

    fn refresh_inventory(&mut self) -> Result<Value> {
        let before_modules = self.configured.module_count;
        let before_runs = self.configured.run_count;
        let before_module_state = self.configured.module_inventory_state.clone();
        let before_run_state = self.configured.run_inventory_state.clone();
        let before_current_run = self.configured.current_run.clone();
        let before_last_http_status = self.configured.last_http_status.clone();
        let before_module_model = self.configured.module_model.clone();
        let before_module_serial = self.configured.module_serial.clone();
        let before_module_status = self.configured.module_status.clone();
        let before_module_temperature = self.configured.module_temperature;
        let before_module_target_temperature = self.configured.module_target_temperature;
        let refresh = self.configured.refresh_inventory()?;
        if self.configured.module_count != before_modules {
            self.emit_hub_property("module_count", Value::I64(self.configured.module_count));
            self.emit_deck_property("loaded_modules", Value::I64(self.configured.module_count));
        }
        if self.configured.run_count != before_runs {
            self.emit_hub_property("run_count", Value::I64(self.configured.run_count));
        }
        if self.configured.module_inventory_state != before_module_state {
            self.emit_hub_property(
                "module_inventory_state",
                Value::String(self.configured.module_inventory_state.clone()),
            );
        }
        if self.configured.run_inventory_state != before_run_state {
            self.emit_hub_property(
                "run_inventory_state",
                Value::String(self.configured.run_inventory_state.clone()),
            );
        }
        if self.configured.current_run != before_current_run {
            self.emit_hub_property(
                "current_run",
                Value::String(self.configured.current_run.clone()),
            );
        }
        if self.configured.last_http_status != before_last_http_status {
            self.emit_hub_property(
                "last_http_status",
                Value::String(self.configured.last_http_status.clone()),
            );
        }
        if self.module.is_some() {
            if self.configured.module_model != before_module_model {
                self.emit_module_property(
                    "model",
                    Value::String(self.configured.module_model.clone().unwrap_or_default()),
                );
            }
            if self.configured.module_serial != before_module_serial {
                self.emit_module_property(
                    "serial",
                    Value::String(self.configured.module_serial.clone().unwrap_or_default()),
                );
            }
            if self.configured.module_status != before_module_status {
                self.emit_module_property(
                    "status",
                    Value::String(self.configured.module_status.clone()),
                );
            }
            if self.configured.module_temperature != before_module_temperature {
                self.emit_module_property(
                    "temperature",
                    Value::Temperature(self.configured.module_temperature),
                );
            }
            if self.configured.module_target_temperature != before_module_target_temperature {
                self.emit_module_property(
                    "target_temperature",
                    Value::Temperature(self.configured.module_target_temperature),
                );
            }
        }
        let mut values = BTreeMap::from([
            (
                "modules_http_status".into(),
                Value::I64(refresh.modules_status as i64),
            ),
            (
                "runs_http_status".into(),
                Value::I64(refresh.runs_status as i64),
            ),
            ("module_count".into(), Value::I64(refresh.module_count)),
            ("run_count".into(), Value::I64(refresh.run_count)),
            ("current_run".into(), Value::String(refresh.current_run)),
            ("module_status".into(), Value::String(refresh.module_status)),
            (
                "module_temperature".into(),
                Value::Temperature(refresh.module_temperature),
            ),
            (
                "module_target_temperature".into(),
                Value::Temperature(refresh.module_target_temperature),
            ),
        ]);
        if let Some(model) = refresh.module_model {
            values.insert("module_model".into(), Value::String(model));
        }
        if let Some(serial) = refresh.module_serial {
            values.insert("module_serial".into(), Value::String(serial));
        }
        Ok(Value::Map(values))
    }

    fn write_module_property(&mut self, key: &str, value: Value) -> Result<Value> {
        match (key, value) {
            ("target_temperature", Value::Temperature(target)) => {
                self.apply_module_temperature_control(TemperatureControlRequest {
                    target: Some(target),
                    enabled: Some(true),
                })?;
                Ok(Value::Temperature(
                    self.configured.module_target_temperature,
                ))
            }
            ("enabled", Value::Bool(enabled)) => {
                self.apply_module_temperature_control(TemperatureControlRequest {
                    target: None,
                    enabled: Some(enabled),
                })?;
                Ok(Value::Bool(self.configured.module_status != "idle"))
            }
            ("target_temperature", _) => Err(Error::new(
                ErrorCode::InvalidProperty,
                "Opentrons module target_temperature expects Temperature",
            )),
            ("enabled", _) => Err(Error::new(
                ErrorCode::InvalidProperty,
                "Opentrons module enabled expects Bool",
            )),
            _ => invalid_property("unknown writable Opentrons module property", key),
        }
    }

    fn apply_module_temperature_control(
        &mut self,
        request: TemperatureControlRequest,
    ) -> Result<Value> {
        if self.module.is_none() {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Opentrons OT-2 has no configured or discovered temperature module",
            ));
        }
        let serial = self.configured.module_serial.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidCommand,
                "Opentrons module TemperatureControl requires a module serial",
            )
        })?;
        let api_version = self.configured.api_version.parse::<u16>().map_err(|_| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Opentrons OT-2 api_version must be numeric",
            )
        })?;
        if api_version >= 3 {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "Opentrons module direct command endpoint is removed for Opentrons-Version 3",
            ));
        }

        let mut http_status = None;
        let mut action = "cached".to_string();
        if request.target.is_some() && request.enabled == Some(false) {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Opentrons module TemperatureControl cannot set a target and disable in one request",
            ));
        }
        let target_to_set = request.target.or_else(|| {
            request
                .enabled
                .unwrap_or(false)
                .then_some(self.configured.module_target_temperature)
        });
        if let Some(target) = target_to_set {
            validate_module_temperature_target(target)?;
            let celsius = target.celsius();
            let path = format!("/modules/{serial}");
            let body = format!(
                r#"{{"command_type":"set_Temperature","args":[{}]}}"#,
                finite_json_number(celsius, "Opentrons temperature target")?
            );
            let response = http_post_json(
                &self.configured.host,
                self.configured.port,
                &self.configured.api_version,
                &path,
                &body,
                self.configured.connect_timeout_ms,
                self.configured.response_timeout_ms,
            )?;
            self.configured.last_http_status = format!("POST {path} {}", response.status_code);
            if !(200..300).contains(&response.status_code) {
                return Err(Error::new(
                    ErrorCode::Transport,
                    format!(
                        "Opentrons module set_Temperature returned HTTP {}",
                        response.status_code
                    ),
                ));
            }
            self.configured.module_target_temperature = target;
            self.configured.module_status = "target_submitted".into();
            http_status = Some(response.status_code);
            action = "set_Temperature".into();
        }
        if request.enabled == Some(false) {
            let path = format!("/modules/{serial}");
            let body = r#"{"command_type":"deactivate","args":[]}"#;
            let response = http_post_json(
                &self.configured.host,
                self.configured.port,
                &self.configured.api_version,
                &path,
                body,
                self.configured.connect_timeout_ms,
                self.configured.response_timeout_ms,
            )?;
            self.configured.last_http_status = format!("POST {path} {}", response.status_code);
            if !(200..300).contains(&response.status_code) {
                return Err(Error::new(
                    ErrorCode::Transport,
                    format!(
                        "Opentrons module deactivate returned HTTP {}",
                        response.status_code
                    ),
                ));
            }
            self.configured.module_status = "idle".into();
            http_status = Some(response.status_code);
            action = "deactivate".into();
        }

        self.emit_module_property(
            "target_temperature",
            Value::Temperature(self.configured.module_target_temperature),
        );
        self.emit_module_property(
            "enabled",
            Value::Bool(self.configured.module_status != "idle"),
        );
        self.emit_module_property(
            "status",
            Value::String(self.configured.module_status.clone()),
        );
        self.emit_hub_property(
            "last_http_status",
            Value::String(self.configured.last_http_status.clone()),
        );
        Ok(Value::Map(BTreeMap::from([
            ("action".into(), Value::String(action)),
            (
                "http_status".into(),
                http_status
                    .map(|status| Value::I64(status as i64))
                    .unwrap_or(Value::Null),
            ),
            (
                "target_temperature".into(),
                Value::Temperature(self.configured.module_target_temperature),
            ),
            (
                "enabled".into(),
                Value::Bool(self.configured.module_status != "idle"),
            ),
            (
                "status".into(),
                Value::String(self.configured.module_status.clone()),
            ),
            (
                "completion_basis".into(),
                Value::String("http_module_command_response".into()),
            ),
        ])))
    }

    fn refresh_current_run(&mut self) -> Result<Value> {
        if self.configured.current_run == "none" || self.configured.current_run.trim().is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Opentrons OT-2 refresh_current_run requires a current run id",
            ));
        }
        let before_status = self.configured.status.clone();
        let before_run_state = self.configured.run_inventory_state.clone();
        let before_last_http_status = self.configured.last_http_status.clone();
        let path = format!("/runs/{}", self.configured.current_run);
        let response = http_get_json(
            &self.configured.host,
            self.configured.port,
            &self.configured.api_version,
            &path,
            self.configured.connect_timeout_ms,
            self.configured.response_timeout_ms,
        )?;
        if let Some(status) = first_json_string(
            &response.body,
            &["status", "runStatus", "state", "currentState"],
        ) {
            self.configured.status = status;
        }
        self.configured.run_inventory_state = if (200..300).contains(&response.status_code) {
            "current_run_refreshed".into()
        } else {
            format!("http_{}", response.status_code)
        };
        self.configured.last_http_status = format!("GET {path} {}", response.status_code);
        if self.configured.status != before_status {
            self.emit_hub_property("status", Value::String(self.configured.status.clone()));
            self.emit_hub_property("ready", Value::Bool(self.configured.status == "idle"));
        }
        if self.configured.run_inventory_state != before_run_state {
            self.emit_hub_property(
                "run_inventory_state",
                Value::String(self.configured.run_inventory_state.clone()),
            );
        }
        if self.configured.last_http_status != before_last_http_status {
            self.emit_hub_property(
                "last_http_status",
                Value::String(self.configured.last_http_status.clone()),
            );
        }
        Ok(Value::Map(BTreeMap::from([
            (
                "http_status".into(),
                Value::I64(response.status_code as i64),
            ),
            (
                "current_run".into(),
                Value::String(self.configured.current_run.clone()),
            ),
            (
                "status".into(),
                Value::String(self.configured.status.clone()),
            ),
            (
                "run_inventory_state".into(),
                Value::String(self.configured.run_inventory_state.clone()),
            ),
        ])))
    }

    fn refresh_run_commands(&mut self) -> Result<Value> {
        let before_command_count = self.configured.command_count;
        let before_current_command = self.configured.current_command.clone();
        let before_current_command_status = self.configured.current_command_status.clone();
        let before_run_state = self.configured.run_inventory_state.clone();
        let before_last_http_status = self.configured.last_http_status.clone();
        let refresh = self.configured.refresh_run_commands()?;
        if self.configured.command_count != before_command_count {
            self.emit_hub_property("command_count", Value::I64(self.configured.command_count));
        }
        if self.configured.current_command != before_current_command {
            self.emit_hub_property(
                "current_command",
                Value::String(self.configured.current_command.clone()),
            );
        }
        if self.configured.current_command_status != before_current_command_status {
            self.emit_hub_property(
                "current_command_status",
                Value::String(self.configured.current_command_status.clone()),
            );
        }
        if self.configured.run_inventory_state != before_run_state {
            self.emit_hub_property(
                "run_inventory_state",
                Value::String(self.configured.run_inventory_state.clone()),
            );
        }
        if self.configured.last_http_status != before_last_http_status {
            self.emit_hub_property(
                "last_http_status",
                Value::String(self.configured.last_http_status.clone()),
            );
        }
        Ok(Value::Map(BTreeMap::from([
            ("http_status".into(), Value::I64(refresh.http_status as i64)),
            (
                "current_run".into(),
                Value::String(self.configured.current_run.clone()),
            ),
            ("command_count".into(), Value::I64(refresh.command_count)),
            (
                "current_command".into(),
                Value::String(refresh.current_command),
            ),
            (
                "current_command_status".into(),
                Value::String(refresh.current_command_status),
            ),
            (
                "run_inventory_state".into(),
                Value::String(self.configured.run_inventory_state.clone()),
            ),
        ])))
    }

    fn run_action(&mut self, action_type: &str) -> Result<Value> {
        if self.configured.current_run == "none" || self.configured.current_run.trim().is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Opentrons OT-2 run action requires a current run id",
            ));
        }
        let path = format!("/runs/{}/actions", self.configured.current_run);
        let body = format!(r#"{{"data":{{"actionType":"{action_type}"}}}}"#);
        let response = http_post_json(
            &self.configured.host,
            self.configured.port,
            &self.configured.api_version,
            &path,
            &body,
            self.configured.connect_timeout_ms,
            self.configured.response_timeout_ms,
        )?;
        self.configured.last_http_status = format!("POST {path} {}", response.status_code);
        self.emit_hub_property(
            "last_http_status",
            Value::String(self.configured.last_http_status.clone()),
        );
        let action_id = first_json_string(&response.body, &["id"]).unwrap_or_default();
        let action = first_json_string(&response.body, &["actionType"])
            .unwrap_or_else(|| action_type.to_string());
        if (200..300).contains(&response.status_code) {
            self.configured.run_inventory_state = "run_action_submitted".into();
            self.emit_hub_property(
                "run_inventory_state",
                Value::String(self.configured.run_inventory_state.clone()),
            );
        } else {
            self.configured.run_inventory_state = format!("http_{}", response.status_code);
            self.emit_hub_property(
                "run_inventory_state",
                Value::String(self.configured.run_inventory_state.clone()),
            );
        }
        let mut values = BTreeMap::from([
            (
                "http_status".into(),
                Value::I64(response.status_code as i64),
            ),
            (
                "current_run".into(),
                Value::String(self.configured.current_run.clone()),
            ),
            ("action_type".into(), Value::String(action)),
            (
                "run_inventory_state".into(),
                Value::String(self.configured.run_inventory_state.clone()),
            ),
            (
                "last_http_status".into(),
                Value::String(self.configured.last_http_status.clone()),
            ),
        ]);
        if !action_id.is_empty() {
            values.insert("action_id".into(), Value::String(action_id));
        }
        Ok(Value::Map(values))
    }

    fn capture_camera_snapshot(
        &mut self,
        device: DeviceId,
        token: DriverToken,
        request: CapabilityRequest,
    ) -> Result<Value> {
        let request = match request {
            CapabilityRequest::CameraCapture(request) => request,
            CapabilityRequest::None => CameraCaptureRequest::default_frame(),
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "Opentrons camera snapshot expects CameraCaptureRequest",
                ))
            }
        };
        if let Some(encoding) = request.encoding {
            if encoding != ImageEncoding::Native {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    "Opentrons camera snapshot only supports native HTTP image encoding",
                ));
            }
        }
        let response = http_post_binary(
            &self.configured.host,
            self.configured.port,
            &self.configured.api_version,
            "/camera/picture",
            self.configured.connect_timeout_ms,
            self.configured.response_timeout_ms,
        )?;
        self.configured.last_http_status = format!("POST /camera/picture {}", response.status_code);
        self.emit_hub_property(
            "last_http_status",
            Value::String(self.configured.last_http_status.clone()),
        );
        if !(200..300).contains(&response.status_code) {
            return Err(Error::new(
                ErrorCode::Transport,
                format!(
                    "Opentrons camera snapshot returned HTTP {} with {} response bytes",
                    response.status_code,
                    response.body.len()
                ),
            ));
        }
        let handle = FrameHandle {
            stream: StreamId(device.0 .0),
            frame: FrameId(token.0),
        };
        let content_type = response
            .content_type
            .unwrap_or_else(|| "application/octet-stream".into());
        let byte_count = response.body.len() as i64;
        self.events.push_back(DriverEvent::FrameReady(Frame {
            handle,
            device,
            width: 0,
            height: 0,
            pixel_format: "NativeHttpImage".into(),
            data: response.body,
            metadata: BTreeMap::from([
                (
                    "source".into(),
                    Value::String("opentrons-camera-picture".into()),
                ),
                ("content_type".into(), Value::String(content_type.clone())),
                (
                    "http_status".into(),
                    Value::I64(response.status_code as i64),
                ),
                ("byte_count".into(), Value::I64(byte_count)),
            ]),
            buffer: request.buffer.unwrap_or_default(),
        }));
        Ok(Value::Map(BTreeMap::from([
            ("stream".into(), Value::I64(handle.stream.0 as i64)),
            ("frame".into(), Value::I64(handle.frame.0 as i64)),
            (
                "pixel_format".into(),
                Value::String("NativeHttpImage".into()),
            ),
            ("content_type".into(), Value::String(content_type)),
            (
                "http_status".into(),
                Value::I64(response.status_code as i64),
            ),
            ("byte_count".into(), Value::I64(byte_count)),
        ])))
    }

    fn apply_gantry_home(&mut self) -> Result<Value> {
        let response = http_post_json(
            &self.configured.host,
            self.configured.port,
            &self.configured.api_version,
            "/robot/home",
            r#"{"target":"robot"}"#,
            self.configured.connect_timeout_ms,
            self.configured.response_timeout_ms,
        )?;
        self.configured.last_http_status = format!("POST /robot/home {}", response.status_code);
        self.emit_hub_property(
            "last_http_status",
            Value::String(self.configured.last_http_status.clone()),
        );
        if !(200..300).contains(&response.status_code) {
            return Err(Error::new(
                ErrorCode::Transport,
                format!(
                    "Opentrons gantry home returned HTTP {} with {} response bytes",
                    response.status_code,
                    response.body.len()
                ),
            ));
        }
        self.configured.gantry_homed = true;
        self.configured.status = "homed".into();
        self.emit_gantry_property("homed", Value::Bool(true));
        self.emit_gantry_property("status", Value::String(self.configured.status.clone()));
        Ok(Value::Map(BTreeMap::from([
            ("endpoint".into(), Value::String("POST /robot/home".into())),
            (
                "http_status".into(),
                Value::I64(response.status_code as i64),
            ),
            ("homed".into(), Value::Bool(true)),
        ])))
    }

    fn apply_gantry_move(&mut self, request: StageMoveRequest) -> Result<Value> {
        let (x, y, z) = gantry_absolute_xyz(&request)?;
        let mount = self.configured.gantry_mount.clone();
        let body = format!(
            r#"{{"target":"mount","mount":"{}","point":[{:.6},{:.6},{:.6}]}}"#,
            mount,
            x.micrometers() / 1000.0,
            y.micrometers() / 1000.0,
            z.micrometers() / 1000.0
        );
        let response = http_post_json(
            &self.configured.host,
            self.configured.port,
            &self.configured.api_version,
            "/robot/move",
            &body,
            self.configured.connect_timeout_ms,
            self.configured.response_timeout_ms,
        )?;
        self.configured.last_http_status = format!("POST /robot/move {}", response.status_code);
        self.emit_hub_property(
            "last_http_status",
            Value::String(self.configured.last_http_status.clone()),
        );
        if !(200..300).contains(&response.status_code) {
            return Err(Error::new(
                ErrorCode::Transport,
                format!(
                    "Opentrons gantry move returned HTTP {} with {} response bytes",
                    response.status_code,
                    response.body.len()
                ),
            ));
        }
        self.configured.gantry_x = x;
        self.configured.gantry_y = y;
        self.configured.gantry_z = z;
        self.configured.status = "move_submitted".into();
        self.emit_gantry_property("x", Value::Position(x));
        self.emit_gantry_property("y", Value::Position(y));
        self.emit_gantry_property("z", Value::Position(z));
        self.emit_gantry_property("status", Value::String(self.configured.status.clone()));
        Ok(Value::Map(BTreeMap::from([
            ("endpoint".into(), Value::String("POST /robot/move".into())),
            (
                "http_status".into(),
                Value::I64(response.status_code as i64),
            ),
            ("x".into(), Value::Position(x)),
            ("y".into(), Value::Position(y)),
            ("z".into(), Value::Position(z)),
            ("mount".into(), Value::String(mount)),
            (
                "target".into(),
                Value::String("mount nominal position".into()),
            ),
        ])))
    }

    fn invoke(
        &mut self,
        device: DeviceId,
        capability: CapabilityId,
        request: CapabilityRequest,
        token: DriverToken,
    ) -> Result<Value> {
        let descriptor = self
            .capabilities(device)
            .into_iter()
            .find(|candidate| candidate.id == capability)
            .ok_or_else(|| Error::new(ErrorCode::Unsupported, "unknown Opentrons capability"))?;
        match (descriptor.kind, request) {
            (CapabilityKind::GenericCommand, CapabilityRequest::GenericCommand(request)) => {
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
                        "Opentrons OT-2 read-only refresh commands do not take parameters",
                    ));
                }
                match request.command.as_str() {
                    "refresh_health" => self.refresh_health(),
                    "refresh_inventory" => self.refresh_inventory(),
                    "refresh_current_run" => self.refresh_current_run(),
                    "refresh_run_commands" => self.refresh_run_commands(),
                    "play_run" => self.run_action("play"),
                    "pause_run" => self.run_action("pause"),
                    "stop_run" => self.run_action("stop"),
                    _ => Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "Opentrons OT-2 GenericCommand supports refresh_health, refresh_inventory, refresh_current_run, refresh_run_commands, play_run, pause_run, and stop_run only",
                    )),
                }
            }
            (CapabilityKind::GenericCommand, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Opentrons OT-2 GenericCommand expects GenericCommandRequest",
            )),
            (
                CapabilityKind::TemperatureControl,
                CapabilityRequest::TemperatureControl(request),
            ) if self.module == Some(device) => self.apply_module_temperature_control(request),
            (CapabilityKind::StageHome, CapabilityRequest::None) if device == self.gantry => {
                self.apply_gantry_home()
            }
            (CapabilityKind::StageMove, CapabilityRequest::StageMove(request))
                if device == self.gantry =>
            {
                self.apply_gantry_move(request)
            }
            (CapabilityKind::TemperatureControl, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Opentrons module TemperatureControl expects TemperatureControlRequest",
            )),
            (CapabilityKind::StageHome, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Opentrons gantry StageHome expects no request",
            )),
            (CapabilityKind::StageMove, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Opentrons gantry StageMove expects StageMoveRequest",
            )),
            (CapabilityKind::CameraCapture, request) if self.camera == Some(device) => {
                self.capture_camera_snapshot(device, token, request)
            }
            (CapabilityKind::CameraCapture, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Opentrons CameraCapture expects CameraCaptureRequest",
            )),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Opentrons capability",
            )),
        }
    }

    fn emit_hub_property(&mut self, key: &str, value: Value) {
        self.events
            .push_back(DriverEvent::Event(Event::PropertyChanged(
                PropertyChanged {
                    device: self.hub,
                    key: key.into(),
                    value,
                },
            )));
    }

    fn emit_deck_property(&mut self, key: &str, value: Value) {
        self.events
            .push_back(DriverEvent::Event(Event::PropertyChanged(
                PropertyChanged {
                    device: self.deck,
                    key: key.into(),
                    value,
                },
            )));
    }

    fn emit_module_property(&mut self, key: &str, value: Value) {
        if let Some(module) = self.module {
            self.events
                .push_back(DriverEvent::Event(Event::PropertyChanged(
                    PropertyChanged {
                        device: module,
                        key: key.into(),
                        value,
                    },
                )));
        }
    }

    fn emit_gantry_property(&mut self, key: &str, value: Value) {
        self.events
            .push_back(DriverEvent::Event(Event::PropertyChanged(
                PropertyChanged {
                    device: self.gantry,
                    key: key.into(),
                    value,
                },
            )));
    }
}

impl Driver for OpentronsOt2Driver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "opentrons-ot2-http".into(),
            kind: "network.http".into(),
            metadata: BTreeMap::from([
                ("host".into(), Value::String(self.configured.host.clone())),
                (
                    "api_version".into(),
                    Value::String(self.configured.api_version.clone()),
                ),
                ("support_level".into(), Value::String(self.support_level())),
            ]),
        }]
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        let mut devices = vec![
            DeviceDescriptor {
                id: self.hub,
                driver: self.id,
                label: "opentrons-ot2".into(),
                vendor: Some("Opentrons".into()),
                model: Some(self.configured.robot_type.clone()),
                serial: Some(self.configured.robot_serial.clone()),
                kinds: strings(&["hub", "liquid_handler.robot", "network.http"]),
                properties: vec![
                    string_property("host", "Host"),
                    integer_property("port", "Port"),
                    string_property("api_version", "API version"),
                    string_property("server_version", "Server version"),
                    string_property("robot_serial", "Robot serial"),
                    string_property("robot_type", "Robot type"),
                    string_property("status", "Status"),
                    bool_property("door_open", "Door open"),
                    string_property("current_run", "Current run"),
                    integer_property("module_count", "Module count"),
                    integer_property("run_count", "Run count"),
                    integer_property("command_count", "Command count"),
                    string_property("current_command", "Current command"),
                    string_property("current_command_status", "Current command status"),
                    string_property("module_inventory_state", "Module inventory state"),
                    string_property("run_inventory_state", "Run inventory state"),
                    string_property("last_http_status", "Last HTTP status"),
                    bool_property("ready", "Ready"),
                ],
                metadata: BTreeMap::from([
                    ("family".into(), Value::String("Opentrons OT-2".into())),
                    ("support_level".into(), Value::String(self.support_level())),
                    (
                        "server_version".into(),
                        Value::String(self.configured.server_version.clone()),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.gantry,
                driver: self.id,
                label: "opentrons-ot2-gantry".into(),
                vendor: Some("Opentrons".into()),
                model: Some(self.configured.robot_type.clone()),
                serial: Some(self.configured.robot_serial.clone()),
                kinds: strings(&["stage.xyz", "motion.robot"]),
                properties: vec![
                    bool_property("homed", "Homed"),
                    string_property("status", "Status"),
                    string_property("mount", "Move mount"),
                    position_property("x", "X position"),
                    position_property("y", "Y position"),
                    position_property("z", "Z position"),
                ],
                metadata: BTreeMap::new(),
            },
            DeviceDescriptor {
                id: self.deck,
                driver: self.id,
                label: "opentrons-ot2-deck".into(),
                vendor: Some("Opentrons".into()),
                model: Some(self.configured.robot_type.clone()),
                serial: Some(self.configured.robot_serial.clone()),
                kinds: strings(&["deck", "labware.host"]),
                properties: vec![
                    integer_property("loaded_labware", "Loaded labware"),
                    integer_property("loaded_modules", "Loaded modules"),
                ],
                metadata: BTreeMap::new(),
            },
        ];
        if self.left_pipette.is_some() {
            devices.push(pipette_descriptor(
                self.id,
                self.left_pipette.expect("left pipette id exists"),
                "opentrons-ot2-left-pipette",
                "left",
                self.configured.left_pipette_model.clone(),
                self.configured.left_pipette_serial.clone(),
            ));
        }
        if self.right_pipette.is_some() {
            devices.push(pipette_descriptor(
                self.id,
                self.right_pipette.expect("right pipette id exists"),
                "opentrons-ot2-right-pipette",
                "right",
                self.configured.right_pipette_model.clone(),
                self.configured.right_pipette_serial.clone(),
            ));
        }
        if let Some(camera) = self.camera {
            devices.push(DeviceDescriptor {
                id: camera,
                driver: self.id,
                label: "opentrons-ot2-camera".into(),
                vendor: Some("Opentrons".into()),
                model: Some("OT-2 inspection camera".into()),
                serial: Some(self.configured.robot_serial.clone()),
                kinds: strings(&["camera.snapshot", "inspection.camera"]),
                properties: vec![
                    bool_property("available", "Available"),
                    bool_property("snapshot_supported", "Snapshot supported"),
                ],
                metadata: BTreeMap::from([(
                    "http_endpoint".into(),
                    Value::String("POST /camera/picture".into()),
                )]),
            });
        }
        if let Some(module) = self.module {
            devices.push(DeviceDescriptor {
                id: module,
                driver: self.id,
                label: "opentrons-ot2-module-1".into(),
                vendor: Some("Opentrons".into()),
                model: self.configured.module_model.clone(),
                serial: self.configured.module_serial.clone(),
                kinds: strings(&["module.temperature", "module.opentrons"]),
                properties: vec![
                    string_property("model", "Model"),
                    string_property("serial", "Serial"),
                    temperature_property("temperature", "Temperature"),
                    writable_property(
                        "target_temperature",
                        "Target temperature",
                        ValueType::Temperature,
                    ),
                    writable_property("enabled", "Enabled", ValueType::Bool),
                    string_property("status", "Status"),
                ],
                metadata: BTreeMap::from([(
                    "http_endpoint".into(),
                    Value::String("POST /modules/{serial}".into()),
                )]),
            });
        }
        devices
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.hub {
            vec![CapabilityDescriptor::new(
                CapabilityId(1),
                device,
                CapabilityKind::GenericCommand,
                ValueType::Map,
            )]
        } else if device == self.gantry {
            vec![
                CapabilityDescriptor::new(
                    CapabilityId(4),
                    device,
                    CapabilityKind::StageHome,
                    ValueType::Null,
                ),
                CapabilityDescriptor::new(
                    CapabilityId(5),
                    device,
                    CapabilityKind::StageMove,
                    ValueType::Map,
                ),
            ]
        } else if self.camera == Some(device) {
            vec![CapabilityDescriptor::new(
                CapabilityId(2),
                device,
                CapabilityKind::CameraCapture,
                ValueType::Map,
            )]
        } else if self.module == Some(device) {
            vec![CapabilityDescriptor::new(
                CapabilityId(3),
                device,
                CapabilityKind::TemperatureControl,
                ValueType::Map,
            )]
        } else {
            Vec::new()
        }
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    self.read_property(*device, key)?;
                }
                Command::WriteProperty { device, key, value } if self.module == Some(*device) => {
                    match (key.as_str(), value) {
                        ("target_temperature", Value::Temperature(target)) => {
                            validate_module_temperature_target(*target)?;
                        }
                        ("enabled", Value::Bool(_)) => {}
                        ("target_temperature", _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidProperty,
                                "Opentrons module target_temperature expects Temperature",
                            ));
                        }
                        ("enabled", _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidProperty,
                                "Opentrons module enabled expects Bool",
                            ));
                        }
                        _ => invalid_property("unknown writable Opentrons module property", key)?,
                    }
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if *device == self.hub
                    || *device == self.gantry
                    || self.camera == Some(*device)
                    || self.module == Some(*device) =>
                {
                    let descriptor = self
                        .capabilities(*device)
                        .into_iter()
                        .find(|candidate| candidate.id == *capability)
                        .ok_or_else(|| {
                            Error::new(ErrorCode::Unsupported, "unknown Opentrons capability")
                        })?;
                    match (&descriptor.kind, request) {
                        (
                            CapabilityKind::GenericCommand,
                            CapabilityRequest::GenericCommand(request),
                        ) if matches!(
                            request.command.as_str(),
                            "refresh_health"
                                | "refresh_inventory"
                                | "refresh_current_run"
                                | "refresh_run_commands"
                                | "play_run"
                                | "pause_run"
                                | "stop_run"
                        ) && request.params.is_empty() => {}
                        (CapabilityKind::GenericCommand, CapabilityRequest::GenericCommand(_)) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Opentrons OT-2 GenericCommand supports refresh_health, refresh_inventory, refresh_current_run, refresh_run_commands, play_run, pause_run, and stop_run only",
                            ));
                        }
                        (CapabilityKind::GenericCommand, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Opentrons OT-2 GenericCommand expects GenericCommandRequest",
                            ));
                        }
                        (
                            CapabilityKind::CameraCapture,
                            CapabilityRequest::CameraCapture(request),
                        ) => {
                            if matches!(&request.encoding, Some(encoding) if *encoding != ImageEncoding::Native)
                            {
                                return Err(Error::new(
                                    ErrorCode::Unsupported,
                                    "Opentrons camera snapshot only supports native HTTP image encoding",
                                ));
                            }
                        }
                        (CapabilityKind::CameraCapture, CapabilityRequest::None) => {}
                        (CapabilityKind::CameraCapture, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Opentrons camera snapshot expects CameraCaptureRequest",
                            ));
                        }
                        (
                            CapabilityKind::TemperatureControl,
                            CapabilityRequest::TemperatureControl(request),
                        ) => {
                            if let Some(target) = request.target {
                                validate_module_temperature_target(target)?;
                            }
                        }
                        (CapabilityKind::StageHome, CapabilityRequest::None) => {}
                        (CapabilityKind::StageMove, CapabilityRequest::StageMove(request)) => {
                            let _ = gantry_absolute_xyz(request)?;
                        }
                        (CapabilityKind::TemperatureControl, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Opentrons module TemperatureControl expects TemperatureControlRequest",
                            ));
                        }
                        (CapabilityKind::StageHome, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Opentrons gantry StageHome expects no request",
                            ));
                        }
                        (CapabilityKind::StageMove, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Opentrons gantry StageMove expects StageMoveRequest",
                            ));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported Opentrons capability",
                            ));
                        }
                    }
                }
                Command::WriteProperty { device, .. } | Command::Invoke { device, .. }
                    if self.descriptors().iter().any(|desc| desc.id == *device) =>
                {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "Opentrons OT-2 exposes read-only robot properties, constrained run-refresh/action commands, gantry home/absolute move, temperature-module control, and camera snapshots; broader HTTP commands need documented schemas and completion behavior",
                    ));
                }
                Command::ApplyStateSet(set) => {
                    if set.writes.iter().any(|write| {
                        self.descriptors()
                            .iter()
                            .any(|desc| desc.id == write.device)
                    }) {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "Opentrons OT-2 configured support does not accept state writes",
                        ));
                    }
                }
                _ => {}
            }
        }
        Ok(PreparedBatch {
            id: batch.id,
            commands: batch.commands.clone(),
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "opentrons ot-2 http inventory/run-control".into(),
                payload: Value::Map(BTreeMap::from([
                    ("host".into(), Value::String(self.configured.host.clone())),
                    ("port".into(), Value::I64(self.configured.port as i64)),
                    (
                        "api_version".into(),
                        Value::String(self.configured.api_version.clone()),
                    ),
                ])),
            }],
        })
    }

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.token();
        let mut result = Value::Null;
        for command in prepared.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    result = self.read_property(device, &key)?;
                }
                Command::WriteProperty { device, key, value } if self.module == Some(device) => {
                    result = self.write_module_property(&key, value)?;
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    result = self.invoke(device, capability, request, token)?;
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

impl OpentronsOt2Driver {
    fn support_level(&self) -> String {
        if self.configured.connect_real_transport {
            "active_http_inventory_run_gantry_control".into()
        } else {
            "configured_inventory_run_gantry_control".into()
        }
    }
}

fn pipette_descriptor(
    driver: DriverId,
    id: DeviceId,
    label: &str,
    mount: &str,
    model: Option<String>,
    serial: Option<String>,
) -> DeviceDescriptor {
    DeviceDescriptor {
        id,
        driver,
        label: label.into(),
        vendor: Some("Opentrons".into()),
        model,
        serial,
        kinds: strings(&["liquid_handler.pipette", mount_kind(mount)]),
        properties: vec![
            string_property("mount", "Mount"),
            string_property("model", "Model"),
            string_property("serial", "Serial"),
            bool_property("has_tip", "Has tip"),
        ],
        metadata: BTreeMap::new(),
    }
}

fn mount_kind(mount: &str) -> &'static str {
    match mount {
        "left" => "mount.left",
        "right" => "mount.right",
        _ => "mount.unknown",
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

fn bool_property(key: &str, display_name: &str) -> PropertySchema {
    property(key, display_name, ValueType::Bool)
}

fn integer_property(key: &str, display_name: &str) -> PropertySchema {
    property(key, display_name, ValueType::I64)
}

fn temperature_property(key: &str, display_name: &str) -> PropertySchema {
    property(key, display_name, ValueType::Temperature)
}

fn position_property(key: &str, display_name: &str) -> PropertySchema {
    property(key, display_name, ValueType::Position)
}

fn writable_property(key: &str, display_name: &str, value_type: ValueType) -> PropertySchema {
    let mut schema = property(key, display_name, value_type);
    schema.writable = true;
    schema
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

fn invalid_property<T>(context: &str, key: &str) -> Result<T> {
    Err(Error::new(
        ErrorCode::InvalidProperty,
        format!("{context}: {key}"),
    ))
}

fn string_prop(device: &DeviceConfig, key: &str) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn host_prop(device: &DeviceConfig, key: &str) -> Result<Option<String>> {
    match device.properties.get(key) {
        Some(Value::String(value)) if !value.trim().is_empty() => Ok(Some(value.clone())),
        Some(Value::String(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            "Opentrons OT-2 host must not be empty",
        )),
        _ => Ok(None),
    }
}

fn u16_prop(device: &DeviceConfig, key: &str) -> Result<Option<u16>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if (0..=u16::MAX as i64).contains(value) => Ok(Some(*value as u16)),
        Some(Value::I64(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Opentrons OT-2 property {key} must fit in an unsigned 16-bit integer"),
        )),
        _ => Ok(None),
    }
}

fn u64_prop(device: &DeviceConfig, key: &str) -> Result<Option<u64>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if *value >= 0 => Ok(Some(*value as u64)),
        Some(Value::I64(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Opentrons OT-2 property {key} must be non-negative"),
        )),
        _ => Ok(None),
    }
}

fn i64_prop(device: &DeviceConfig, key: &str) -> Result<Option<i64>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if *value >= 0 => Ok(Some(*value)),
        Some(Value::I64(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Opentrons OT-2 property {key} must be non-negative"),
        )),
        _ => Ok(None),
    }
}

fn api_version_prop(device: &DeviceConfig, key: &str) -> Result<Option<String>> {
    match device.properties.get(key) {
        Some(Value::String(value)) => {
            let version = value.parse::<u16>().map_err(|_| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    "Opentrons OT-2 api_version must be numeric",
                )
            })?;
            if version >= 2 {
                Ok(Some(value.clone()))
            } else {
                Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Opentrons OT-2 api_version must be 2 or higher",
                ))
            }
        }
        _ => Ok(None),
    }
}

fn optional_string_prop(
    device: &DeviceConfig,
    key: &str,
    current: Option<String>,
) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) if value.eq_ignore_ascii_case("none") || value.is_empty() => {
            None
        }
        Some(Value::String(value)) => Some(value.clone()),
        _ => current,
    }
}

fn mount_prop(device: &DeviceConfig, key: &str) -> Result<Option<String>> {
    match device.properties.get(key) {
        Some(Value::String(value)) => match value.as_str() {
            "left" | "right" => Ok(Some(value.clone())),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Opentrons OT-2 property {key} must be left or right"),
            )),
        },
        _ => Ok(None),
    }
}

fn bool_prop(device: &DeviceConfig, key: &str) -> Option<bool> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn temperature_prop(device: &DeviceConfig, key: &str) -> Option<Temperature> {
    match device.properties.get(key) {
        Some(Value::Temperature(value)) => Some(*value),
        _ => None,
    }
}

fn position_prop(device: &DeviceConfig, key: &str) -> Option<Position> {
    match device.properties.get(key) {
        Some(Value::Position(value)) => Some(*value),
        Some(Value::I64(value)) => Some(Position::from_millimeters(*value as f64)),
        Some(Value::F64(value)) => Some(Position::from_millimeters(*value)),
        _ => None,
    }
}

fn gantry_absolute_xyz(request: &StageMoveRequest) -> Result<(Position, Position, Position)> {
    if request.relative {
        return Err(Error::new(
            ErrorCode::Unsupported,
            "Opentrons gantry StageMove supports only absolute deck coordinates",
        ));
    }
    if request.profile.is_some() {
        return Err(Error::new(
            ErrorCode::Unsupported,
            "Opentrons gantry StageMove does not accept motion profiles",
        ));
    }
    if request
        .target
        .keys()
        .any(|axis| !matches!(axis, StageAxis::X | StageAxis::Y | StageAxis::Z))
    {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            "Opentrons gantry StageMove accepts only X, Y, and Z axes",
        ));
    }
    let x = *request.target.get(&StageAxis::X).ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidCommand,
            "Opentrons gantry StageMove requires X",
        )
    })?;
    let y = *request.target.get(&StageAxis::Y).ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidCommand,
            "Opentrons gantry StageMove requires Y",
        )
    })?;
    let z = *request.target.get(&StageAxis::Z).ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidCommand,
            "Opentrons gantry StageMove requires Z",
        )
    })?;
    Ok((x, y, z))
}

fn validate_module_temperature_target(target: Temperature) -> Result<()> {
    let celsius = target.celsius();
    if (4.0..=95.0).contains(&celsius) {
        Ok(())
    } else {
        Err(Error::new(
            ErrorCode::InvalidProperty,
            "Opentrons temperature module target must be 4..=95 degC",
        ))
    }
}

fn finite_json_number(value: f64, context: &str) -> Result<String> {
    if value.is_finite() {
        Ok(value.to_string())
    } else {
        Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("{context} must be finite"),
        ))
    }
}

struct HttpHealthResponse {
    status_code: u16,
    body: String,
}

struct HttpBinaryResponse {
    status_code: u16,
    content_type: Option<String>,
    body: Vec<u8>,
}

fn http_get_health(
    host: &str,
    port: u16,
    api_version: &str,
    connect_timeout_ms: u64,
    response_timeout_ms: u64,
) -> Result<HttpHealthResponse> {
    http_get_json(
        host,
        port,
        api_version,
        "/health",
        connect_timeout_ms,
        response_timeout_ms,
    )
}

fn http_get_json(
    host: &str,
    port: u16,
    api_version: &str,
    path: &str,
    connect_timeout_ms: u64,
    response_timeout_ms: u64,
) -> Result<HttpHealthResponse> {
    http_request_json(
        "GET",
        host,
        port,
        api_version,
        path,
        None,
        connect_timeout_ms,
        response_timeout_ms,
    )
}

fn http_post_json(
    host: &str,
    port: u16,
    api_version: &str,
    path: &str,
    body: &str,
    connect_timeout_ms: u64,
    response_timeout_ms: u64,
) -> Result<HttpHealthResponse> {
    http_request_json(
        "POST",
        host,
        port,
        api_version,
        path,
        Some(body),
        connect_timeout_ms,
        response_timeout_ms,
    )
}

fn http_post_binary(
    host: &str,
    port: u16,
    api_version: &str,
    path: &str,
    connect_timeout_ms: u64,
    response_timeout_ms: u64,
) -> Result<HttpBinaryResponse> {
    http_request_binary(
        "POST",
        host,
        port,
        api_version,
        path,
        connect_timeout_ms,
        response_timeout_ms,
    )
}

fn http_request_binary(
    method: &str,
    host: &str,
    port: u16,
    api_version: &str,
    path: &str,
    connect_timeout_ms: u64,
    response_timeout_ms: u64,
) -> Result<HttpBinaryResponse> {
    if !path.starts_with('/') {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("Opentrons OT-2 HTTP path must start with /: {path}"),
        ));
    }
    let host = http_host(host)?;
    let mut addresses = (host.as_str(), port).to_socket_addrs().map_err(|error| {
        Error::new(
            ErrorCode::Transport,
            format!("resolve Opentrons OT-2 host {host}:{port} failed: {error}"),
        )
    })?;
    let address = addresses.next().ok_or_else(|| {
        Error::new(
            ErrorCode::Transport,
            format!("Opentrons OT-2 host {host}:{port} did not resolve"),
        )
    })?;
    let mut stream =
        TcpStream::connect_timeout(&address, Duration::from_millis(connect_timeout_ms)).map_err(
            |error| {
                Error::new(
                    ErrorCode::Transport,
                    format!("connect Opentrons OT-2 {host}:{port} failed: {error}"),
                )
            },
        )?;
    stream
        .set_read_timeout(Some(Duration::from_millis(response_timeout_ms)))
        .map_err(|error| Error::new(ErrorCode::Transport, error.to_string()))?;
    stream
        .set_write_timeout(Some(Duration::from_millis(response_timeout_ms)))
        .map_err(|error| Error::new(ErrorCode::Transport, error.to_string()))?;
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nopentrons-version: {api_version}\r\nAccept: */*\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).map_err(|error| {
        Error::new(
            ErrorCode::Transport,
            format!("write Opentrons OT-2 {path} request failed: {error}"),
        )
    })?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).map_err(|error| {
        Error::new(
            ErrorCode::Transport,
            format!("read Opentrons OT-2 {path} response failed: {error}"),
        )
    })?;
    parse_binary_http_response(response)
}

fn http_request_json(
    method: &str,
    host: &str,
    port: u16,
    api_version: &str,
    path: &str,
    body: Option<&str>,
    connect_timeout_ms: u64,
    response_timeout_ms: u64,
) -> Result<HttpHealthResponse> {
    if !path.starts_with('/') {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("Opentrons OT-2 HTTP path must start with /: {path}"),
        ));
    }
    let host = http_host(host)?;
    let mut addresses = (host.as_str(), port).to_socket_addrs().map_err(|error| {
        Error::new(
            ErrorCode::Transport,
            format!("resolve Opentrons OT-2 host {host}:{port} failed: {error}"),
        )
    })?;
    let address = addresses.next().ok_or_else(|| {
        Error::new(
            ErrorCode::Transport,
            format!("Opentrons OT-2 host {host}:{port} did not resolve"),
        )
    })?;
    let mut stream =
        TcpStream::connect_timeout(&address, Duration::from_millis(connect_timeout_ms)).map_err(
            |error| {
                Error::new(
                    ErrorCode::Transport,
                    format!("connect Opentrons OT-2 {host}:{port} failed: {error}"),
                )
            },
        )?;
    stream
        .set_read_timeout(Some(Duration::from_millis(response_timeout_ms)))
        .map_err(|error| Error::new(ErrorCode::Transport, error.to_string()))?;
    stream
        .set_write_timeout(Some(Duration::from_millis(response_timeout_ms)))
        .map_err(|error| Error::new(ErrorCode::Transport, error.to_string()))?;
    let request = if let Some(body) = body {
        format!(
            "{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nopentrons-version: {api_version}\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    } else {
        format!(
            "{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nopentrons-version: {api_version}\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
        )
    };
    stream.write_all(request.as_bytes()).map_err(|error| {
        Error::new(
            ErrorCode::Transport,
            format!("write Opentrons OT-2 {path} request failed: {error}"),
        )
    })?;
    let mut response = String::new();
    stream.read_to_string(&mut response).map_err(|error| {
        Error::new(
            ErrorCode::Transport,
            format!("read Opentrons OT-2 {path} response failed: {error}"),
        )
    })?;
    let status_code = parse_http_status(&response)?;
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default();
    Ok(HttpHealthResponse { status_code, body })
}

fn parse_binary_http_response(response: Vec<u8>) -> Result<HttpBinaryResponse> {
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| {
            Error::new(
                ErrorCode::Transport,
                "Opentrons OT-2 HTTP response has no header boundary",
            )
        })?;
    let headers = String::from_utf8_lossy(&response[..split]);
    let status_code = parse_http_status(&headers)?;
    let content_type = headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-type")
            .then(|| value.trim().to_string())
    });
    Ok(HttpBinaryResponse {
        status_code,
        content_type,
        body: response[split + 4..].to_vec(),
    })
}

fn http_host(host: &str) -> Result<String> {
    let host = host.trim();
    if host.starts_with("https://") {
        return Err(Error::new(
            ErrorCode::Unsupported,
            "Opentrons OT-2 active HTTP health probe supports plain robot-server HTTP only",
        ));
    }
    let host = host.strip_prefix("http://").unwrap_or(host);
    let host = host.trim_end_matches('/');
    if host.is_empty() {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "Opentrons OT-2 host must not be empty",
        ));
    }
    Ok(host.into())
}

fn parse_http_status(response: &str) -> Result<u16> {
    let status = response
        .lines()
        .next()
        .ok_or_else(|| Error::new(ErrorCode::Transport, "empty Opentrons OT-2 HTTP response"))?;
    let code = status.split_whitespace().nth(1).ok_or_else(|| {
        Error::new(
            ErrorCode::Transport,
            format!("invalid Opentrons OT-2 HTTP status line {status:?}"),
        )
    })?;
    code.parse::<u16>().map_err(|_| {
        Error::new(
            ErrorCode::Transport,
            format!("invalid Opentrons OT-2 HTTP status code {code:?}"),
        )
    })
}

fn first_json_string(body: &str, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| json_string_value(body, key))
}

fn first_json_number(body: &str, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| json_number_value(body, key))
}

fn json_string_value(body: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\"");
    let start = body.find(&pattern)?;
    let after_key = &body[start + pattern.len()..];
    let colon = after_key.find(':')?;
    let after_colon = after_key[colon + 1..].trim_start();
    let value = after_colon.strip_prefix('"')?;
    let end = value.find('"')?;
    Some(value[..end].to_string())
}

fn json_number_value(body: &str, key: &str) -> Option<f64> {
    let pattern = format!("\"{key}\"");
    let start = body.find(&pattern)?;
    let after_key = &body[start + pattern.len()..];
    let colon = after_key.find(':')?;
    let after_colon = after_key[colon + 1..].trim_start();
    let end = after_colon
        .find(|ch: char| !(ch.is_ascii_digit() || matches!(ch, '-' | '+' | '.' | 'e' | 'E')))
        .unwrap_or(after_colon.len());
    if end == 0 {
        return None;
    }
    after_colon[..end].parse::<f64>().ok()
}

fn top_level_data_array_count(body: &str) -> Option<usize> {
    let pattern = "\"data\"";
    let start = body.find(pattern)?;
    let after_key = &body[start + pattern.len()..];
    let colon = after_key.find(':')?;
    let mut chars = after_key[colon + 1..].chars().peekable();
    while matches!(chars.peek(), Some(ch) if ch.is_whitespace()) {
        chars.next();
    }
    if chars.next()? != '[' {
        return None;
    }
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut count = 0usize;
    let mut seen_value_at_depth_one = false;
    for ch in chars {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                if depth == 0 {
                    seen_value_at_depth_one = true;
                }
            }
            '[' | '{' => {
                if depth == 0 {
                    seen_value_at_depth_one = true;
                    if ch == '{' {
                        count += 1;
                    }
                }
                depth += 1;
            }
            ']' if depth == 0 => {
                return Some(if seen_value_at_depth_one {
                    count.max(1)
                } else {
                    0
                });
            }
            ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                if seen_value_at_depth_one && count == 0 {
                    count += 1;
                }
            }
            ch if !ch.is_whitespace() && depth == 0 => seen_value_at_depth_one = true,
            _ => {}
        }
    }
    None
}
