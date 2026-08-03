use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DiscoveryRegistry, DriverCandidate, DriverDiscovery};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};

const DRIVER_NAME: &str = "imswitch_daqmx";
const RESOURCE_OFFSET: u64 = 10;
const HUB_OFFSET: u64 = 20;
const DRIVER_ID_BLOCK: u64 = 10_000;
const AO_OFFSET: u64 = 1_000;
const DO_OFFSET: u64 = 2_000;
const AI_OFFSET: u64 = 3_000;
const CI_OFFSET: u64 = 4_000;
const CO_OFFSET: u64 = 5_000;

#[derive(Debug, Clone)]
pub struct ImSwitchDaqmxDiscovery {
    next_id: DriverId,
    configs: Vec<ImSwitchDaqmxConfig>,
}

impl ImSwitchDaqmxDiscovery {
    pub fn configured(next_id: DriverId, hardware: &HardwareConfig) -> Result<Self> {
        let configs = hardware
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), DRIVER_NAME | "imswitch-daqmx"))
            .map(ImSwitchDaqmxConfig::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, configs })
    }

    pub fn fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            configs: vec![ImSwitchDaqmxConfig::fixture()],
        }
    }
}

pub fn register_configured(
    registry: &mut DiscoveryRegistry,
    hardware: &HardwareConfig,
) -> Result<()> {
    registry.register_factory_result(|id| ImSwitchDaqmxDiscovery::configured(id, hardware))
}

pub fn register_fixture(registry: &mut DiscoveryRegistry) {
    registry.register_factory(ImSwitchDaqmxDiscovery::fixture);
}

impl DriverDiscovery for ImSwitchDaqmxDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        std::mem::take(&mut self.configs)
            .into_iter()
            .enumerate()
            .map(|(index, config)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = config.label.clone();
                let driver = ImSwitchDaqmxDriver::new(id, config)?;
                Ok(DriverCandidate::from_driver(label, Box::new(driver)))
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct ImSwitchDaqmxConfig {
    pub label: String,
    pub device_name: String,
    pub product: String,
    pub serial_number: String,
    pub runtime_package: Option<String>,
    pub runtime_version: Option<String>,
    pub runtime_platform: Option<String>,
    pub runtime_license: Option<String>,
    pub sdk_header_path: Option<String>,
    pub sdk_header_sha256: Option<String>,
    pub connect: bool,
    pub live_task_execution: bool,
    pub inventory_devices: bool,
    pub inventory_helper_path: Option<String>,
    pub inventory_helper_timeout: TimeInterval,
    pub analog_output_count: usize,
    pub digital_output_count: usize,
    pub analog_input_count: usize,
    pub counter_input_count: usize,
    pub counter_output_count: usize,
    pub lsm_x_galvo: String,
    pub lsm_y_galvo: String,
    pub lsm_laser_gate: String,
    pub lsm_detector: String,
    pub lsm_sample_clock: String,
    pub lsm_sample_clock_source: Option<String>,
    pub lsm_start_trigger_source: Option<String>,
    pub default_sample_rate: Frequency,
    pub daqmx_timeout: TimeInterval,
    pub analog_min: Voltage,
    pub analog_max: Voltage,
    pub analog_outputs: Vec<Voltage>,
    pub analog_inputs: Vec<Voltage>,
    pub digital_outputs: Vec<bool>,
    pub counter_inputs: Vec<i64>,
    pub counter_output_frequencies: Vec<Frequency>,
    pub last_transaction: Value,
}

impl ImSwitchDaqmxConfig {
    pub fn fixture() -> Self {
        Self {
            label: "Configured ImSwitch NI-DAQmx fixture".into(),
            device_name: "Dev1".into(),
            product: "National Instruments DAQmx-compatible device".into(),
            serial_number: "IMS-DAQMX-CONFIG-0001".into(),
            runtime_package: None,
            runtime_version: None,
            runtime_platform: None,
            runtime_license: None,
            sdk_header_path: None,
            sdk_header_sha256: None,
            connect: false,
            live_task_execution: false,
            inventory_devices: false,
            inventory_helper_path: None,
            inventory_helper_timeout: TimeInterval::from_seconds(8.0),
            analog_output_count: 4,
            digital_output_count: 8,
            analog_input_count: 2,
            counter_input_count: 2,
            counter_output_count: 1,
            lsm_x_galvo: "ao0".into(),
            lsm_y_galvo: "ao1".into(),
            lsm_laser_gate: "do0".into(),
            lsm_detector: "counter0".into(),
            lsm_sample_clock: "counter2".into(),
            lsm_sample_clock_source: None,
            lsm_start_trigger_source: None,
            default_sample_rate: Frequency::from_hertz(100_000.0),
            daqmx_timeout: TimeInterval::from_seconds(10.0),
            analog_min: Voltage::from_volts(-10.0),
            analog_max: Voltage::from_volts(10.0),
            analog_outputs: vec![Voltage::from_volts(0.0); 4],
            analog_inputs: vec![Voltage::from_volts(0.0); 2],
            digital_outputs: vec![false; 8],
            counter_inputs: vec![0; 2],
            counter_output_frequencies: vec![Frequency::from_hertz(1_000_000.0)],
            last_transaction: Value::Map(BTreeMap::from([(
                "completion_basis".into(),
                Value::String("configured_state_only".into()),
            )])),
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut config = Self::fixture();
        config.label = if device.label.is_empty() {
            config.label
        } else {
            device.label.clone()
        };
        config.device_name =
            string_prop(device, "device_name").unwrap_or_else(|| config.device_name.clone());
        config.product = string_prop(device, "product").unwrap_or_else(|| config.product.clone());
        config.serial_number =
            string_prop(device, "serial_number").unwrap_or_else(|| config.serial_number.clone());
        config.runtime_package = string_prop(device, "runtime_package");
        config.runtime_version = string_prop(device, "runtime_version");
        config.runtime_platform = string_prop(device, "runtime_platform");
        config.runtime_license = string_prop(device, "runtime_license");
        config.sdk_header_path = string_prop(device, "sdk_header_path");
        config.sdk_header_sha256 = string_prop(device, "sdk_header_sha256");
        config.connect = bool_prop(device, "connect").unwrap_or(false);
        config.live_task_execution = bool_prop(device, "live_task_execution").unwrap_or(false);
        config.inventory_devices = bool_prop(device, "inventory_devices").unwrap_or(false);
        config.inventory_helper_path = string_prop(device, "inventory_helper_path");
        config.inventory_helper_timeout = time_interval_prop(device, "inventory_helper_timeout")
            .unwrap_or(config.inventory_helper_timeout);
        let inventory_helper_timeout_seconds = config.inventory_helper_timeout.seconds();
        if !inventory_helper_timeout_seconds.is_finite() || inventory_helper_timeout_seconds <= 0.0
        {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "inventory_helper_timeout must be finite and positive",
            ));
        }
        config.analog_output_count = count_prop(
            device,
            "analog_output_count",
            config.analog_output_count,
            0,
            32,
        )?;
        config.digital_output_count = count_prop(
            device,
            "digital_output_count",
            config.digital_output_count,
            0,
            256,
        )?;
        config.analog_input_count = count_prop(
            device,
            "analog_input_count",
            config.analog_input_count,
            0,
            32,
        )?;
        config.counter_input_count = count_prop(
            device,
            "counter_input_count",
            config.counter_input_count,
            0,
            16,
        )?;
        config.counter_output_count = count_prop(
            device,
            "counter_output_count",
            config.counter_output_count,
            0,
            16,
        )?;
        config.lsm_x_galvo = string_prop(device, "lsm_x_galvo").unwrap_or(config.lsm_x_galvo);
        config.lsm_y_galvo = string_prop(device, "lsm_y_galvo").unwrap_or(config.lsm_y_galvo);
        config.lsm_laser_gate =
            string_prop(device, "lsm_laser_gate").unwrap_or(config.lsm_laser_gate);
        config.lsm_detector = string_prop(device, "lsm_detector").unwrap_or(config.lsm_detector);
        config.lsm_sample_clock =
            string_prop(device, "lsm_sample_clock").unwrap_or(config.lsm_sample_clock);
        config.lsm_sample_clock_source = string_prop(device, "lsm_sample_clock_source");
        config.lsm_start_trigger_source = string_prop(device, "lsm_start_trigger_source");
        config.default_sample_rate =
            frequency_prop(device, "default_sample_rate").unwrap_or(config.default_sample_rate);
        config.daqmx_timeout =
            time_interval_prop(device, "daqmx_timeout").unwrap_or(config.daqmx_timeout);
        config.analog_min = voltage_prop(device, "analog_min").unwrap_or(config.analog_min);
        config.analog_max = voltage_prop(device, "analog_max").unwrap_or(config.analog_max);
        if config.analog_min.volts() > config.analog_max.volts() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "analog_min exceeds analog_max",
            ));
        }
        config.analog_outputs = (1..=config.analog_output_count)
            .map(|channel| {
                voltage_prop(device, &format!("analog_output_{channel}"))
                    .unwrap_or_else(|| Voltage::from_volts(0.0))
            })
            .collect();
        config.analog_inputs = (1..=config.analog_input_count)
            .map(|channel| {
                voltage_prop(device, &format!("analog_input_{channel}"))
                    .unwrap_or_else(|| Voltage::from_volts(0.0))
            })
            .collect();
        config.digital_outputs = (1..=config.digital_output_count)
            .map(|line| bool_prop(device, &format!("digital_output_{line}")).unwrap_or(false))
            .collect();
        config.counter_inputs = (1..=config.counter_input_count)
            .map(|channel| i64_prop(device, &format!("counter_input_{channel}")).unwrap_or(0))
            .collect();
        config.counter_output_frequencies = (1..=config.counter_output_count)
            .map(|channel| {
                frequency_prop(device, &format!("counter_output_{channel}_frequency"))
                    .unwrap_or_else(|| Frequency::from_hertz(1_000_000.0))
            })
            .collect();
        Ok(config)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChildKind {
    Hub,
    AnalogOutput(usize),
    DigitalOutput(usize),
    AnalogInput(usize),
    CounterInput(usize),
    CounterOutput(usize),
}

#[derive(Debug)]
pub struct ImSwitchDaqmxDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    config: ImSwitchDaqmxConfig,
    runtime_probe: Option<DaqmxRuntimeProbe>,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
}

impl ImSwitchDaqmxDriver {
    pub fn new(id: DriverId, config: ImSwitchDaqmxConfig) -> Result<Self> {
        let runtime_probe = if config.connect {
            Some(probe_daqmx_runtime(&config)?)
        } else {
            None
        };
        Ok(Self {
            id,
            resource: ResourceId(NodeId(id.0 * DRIVER_ID_BLOCK + RESOURCE_OFFSET)),
            hub: DeviceId(NodeId(id.0 * DRIVER_ID_BLOCK + HUB_OFFSET)),
            config,
            runtime_probe,
            next_token: 1,
            pending: VecDeque::new(),
        })
    }

    pub fn configured(id: DriverId, config: ImSwitchDaqmxConfig) -> Result<Self> {
        Self::new(id, config)
    }

    fn child_kind(&self, device: DeviceId) -> Option<ChildKind> {
        let raw = device.0 .0;
        let base = self.id.0 * DRIVER_ID_BLOCK;
        if raw == base + HUB_OFFSET {
            return Some(ChildKind::Hub);
        }
        let channel = raw.checked_sub(base + AO_OFFSET)?;
        if (1..=self.config.analog_output_count as u64).contains(&channel) {
            return Some(ChildKind::AnalogOutput(channel as usize));
        }
        let channel = raw.checked_sub(base + DO_OFFSET)?;
        if (1..=self.config.digital_output_count as u64).contains(&channel) {
            return Some(ChildKind::DigitalOutput(channel as usize));
        }
        let channel = raw.checked_sub(base + AI_OFFSET)?;
        if (1..=self.config.analog_input_count as u64).contains(&channel) {
            return Some(ChildKind::AnalogInput(channel as usize));
        }
        let channel = raw.checked_sub(base + CI_OFFSET)?;
        if (1..=self.config.counter_input_count as u64).contains(&channel) {
            return Some(ChildKind::CounterInput(channel as usize));
        }
        let channel = raw.checked_sub(base + CO_OFFSET)?;
        if (1..=self.config.counter_output_count as u64).contains(&channel) {
            return Some(ChildKind::CounterOutput(channel as usize));
        }
        None
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        let kind = self
            .child_kind(device)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown DAQmx device"))?;
        match (kind, key) {
            (ChildKind::Hub, "device_name") => Ok(Value::String(self.config.device_name.clone())),
            (ChildKind::Hub, "product") => Ok(Value::String(self.config.product.clone())),
            (ChildKind::Hub, "serial_number") => {
                Ok(Value::String(self.config.serial_number.clone()))
            }
            (ChildKind::Hub, "runtime_package") => Ok(self
                .config
                .runtime_package
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null)),
            (ChildKind::Hub, "runtime_version") => Ok(self
                .config
                .runtime_version
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null)),
            (ChildKind::Hub, "runtime_platform") => Ok(self
                .config
                .runtime_platform
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null)),
            (ChildKind::Hub, "runtime_license") => Ok(self
                .config
                .runtime_license
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null)),
            (ChildKind::Hub, "sdk_header_path") => Ok(self
                .config
                .sdk_header_path
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null)),
            (ChildKind::Hub, "sdk_header_sha256") => Ok(self
                .config
                .sdk_header_sha256
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null)),
            (ChildKind::Hub, "backend_status") => {
                Ok(backend_status(&self.config, self.runtime_probe.as_ref()))
            }
            (ChildKind::Hub, "connected") => Ok(Value::Bool(self.runtime_probe.is_some())),
            (ChildKind::Hub, "inventory_devices") => Ok(Value::Bool(self.config.inventory_devices)),
            (ChildKind::Hub, "inventory_helper_path") => Ok(self
                .config
                .inventory_helper_path
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null)),
            (ChildKind::Hub, "inventory_helper_timeout") => {
                Ok(Value::TimeInterval(self.config.inventory_helper_timeout))
            }
            (ChildKind::Hub, "lsm_x_galvo") => Ok(Value::String(self.config.lsm_x_galvo.clone())),
            (ChildKind::Hub, "lsm_y_galvo") => Ok(Value::String(self.config.lsm_y_galvo.clone())),
            (ChildKind::Hub, "lsm_laser_gate") => {
                Ok(Value::String(self.config.lsm_laser_gate.clone()))
            }
            (ChildKind::Hub, "lsm_detector") => Ok(Value::String(self.config.lsm_detector.clone())),
            (ChildKind::Hub, "lsm_sample_clock") => {
                Ok(Value::String(self.config.lsm_sample_clock.clone()))
            }
            (ChildKind::Hub, "lsm_sample_clock_source") => Ok(self
                .config
                .lsm_sample_clock_source
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null)),
            (ChildKind::Hub, "lsm_start_trigger_source") => Ok(self
                .config
                .lsm_start_trigger_source
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null)),
            (ChildKind::Hub, "default_sample_rate") => {
                Ok(Value::Frequency(self.config.default_sample_rate))
            }
            (ChildKind::Hub, "daqmx_timeout") => Ok(Value::TimeInterval(self.config.daqmx_timeout)),
            (ChildKind::Hub, "last_transaction") => Ok(self.config.last_transaction.clone()),
            (ChildKind::AnalogOutput(channel), "channel") => Ok(Value::I64(channel as i64)),
            (ChildKind::AnalogOutput(channel), "physical_channel") => Ok(Value::String(format!(
                "{}/ao{}",
                self.config.device_name,
                channel - 1
            ))),
            (ChildKind::AnalogOutput(channel), "voltage") => {
                Ok(Value::Voltage(self.config.analog_outputs[channel - 1]))
            }
            (ChildKind::AnalogOutput(_), "voltage_min") => {
                Ok(Value::Voltage(self.config.analog_min))
            }
            (ChildKind::AnalogOutput(_), "voltage_max") => {
                Ok(Value::Voltage(self.config.analog_max))
            }
            (ChildKind::DigitalOutput(line), "line") => Ok(Value::I64(line as i64)),
            (ChildKind::DigitalOutput(line), "physical_line") => Ok(Value::String(format!(
                "{}/port0/line{}",
                self.config.device_name,
                line - 1
            ))),
            (ChildKind::DigitalOutput(line), "high") => {
                Ok(Value::Bool(self.config.digital_outputs[line - 1]))
            }
            (ChildKind::AnalogInput(channel), "channel") => Ok(Value::I64(channel as i64)),
            (ChildKind::AnalogInput(channel), "physical_channel") => Ok(Value::String(format!(
                "{}/ai{}",
                self.config.device_name,
                channel - 1
            ))),
            (ChildKind::AnalogInput(channel), "voltage") => {
                Ok(Value::Voltage(self.config.analog_inputs[channel - 1]))
            }
            (ChildKind::CounterInput(channel), "channel") => Ok(Value::I64(channel as i64)),
            (ChildKind::CounterInput(channel), "physical_channel") => Ok(Value::String(format!(
                "{}/ctr{}",
                self.config.device_name,
                channel - 1
            ))),
            (ChildKind::CounterInput(channel), "count") => {
                Ok(Value::I64(self.config.counter_inputs[channel - 1]))
            }
            (ChildKind::CounterInput(_), "edge") => Ok(Value::String("Rising".into())),
            (ChildKind::CounterOutput(channel), "channel") => Ok(Value::I64(channel as i64)),
            (ChildKind::CounterOutput(channel), "physical_channel") => Ok(Value::String(format!(
                "{}/ctr{}",
                self.config.device_name,
                self.config.counter_input_count + channel - 1
            ))),
            (ChildKind::CounterOutput(channel), "frequency") => Ok(Value::Frequency(
                self.config.counter_output_frequencies[channel - 1],
            )),
            _ => Err(Error::new(ErrorCode::InvalidProperty, "unknown property")),
        }
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: Value) -> Result<Value> {
        let kind = self
            .child_kind(device)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown DAQmx device"))?;
        match (kind, key, value) {
            (ChildKind::AnalogOutput(channel), "voltage", Value::Voltage(value)) => {
                if value.volts() < self.config.analog_min.volts()
                    || value.volts() > self.config.analog_max.volts()
                {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "voltage outside configured DAQmx range",
                    ));
                }
                self.config.analog_outputs[channel - 1] = value;
                let value = Value::Voltage(value);
                self.record_transaction(device, "write_analog_output", value.clone());
                Ok(value)
            }
            (ChildKind::DigitalOutput(line), "high", Value::Bool(value)) => {
                self.config.digital_outputs[line - 1] = value;
                let value = Value::Bool(value);
                self.record_transaction(device, "write_digital_output", value.clone());
                Ok(value)
            }
            (ChildKind::CounterOutput(channel), "frequency", Value::Frequency(value)) => {
                self.config.counter_output_frequencies[channel - 1] = value;
                let value = Value::Frequency(value);
                self.record_transaction(device, "write_counter_output", value.clone());
                Ok(value)
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                "property is not writable or value type is invalid",
            )),
        }
    }

    fn invoke(
        &mut self,
        device: DeviceId,
        capability: CapabilityId,
        request: CapabilityRequest,
    ) -> Result<Value> {
        let kind = self
            .child_kind(device)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown DAQmx device"))?;
        match (kind, capability.0, request) {
            (ChildKind::Hub, 1, CapabilityRequest::ConfocalImageCapture(request)) => Ok(self
                .api_summary(
                    device,
                    "confocal_image_capture",
                    BTreeMap::from([
                        ("scan_fields".into(), Value::I64(request.scan.len() as i64)),
                        (
                            "reconstruction_fields".into(),
                            Value::I64(request.reconstruction.len() as i64),
                        ),
                        ("result".into(), Value::String("final_image_pending".into())),
                        (
                            "daqmx_task_plan".into(),
                            lsm_raster_task_plan(
                                &self.config,
                                self.runtime_probe.as_ref(),
                                &request.scan,
                                &request.reconstruction,
                                false,
                            ),
                        ),
                    ]),
                )),
            (ChildKind::Hub, 2, CapabilityRequest::ConfocalImageStream(request)) => Ok(self
                .api_summary(
                    device,
                    "confocal_image_stream",
                    BTreeMap::from([
                        ("scan_fields".into(), Value::I64(request.scan.len() as i64)),
                        (
                            "reconstruction_fields".into(),
                            Value::I64(request.reconstruction.len() as i64),
                        ),
                        (
                            "update_policy".into(),
                            request
                                .update_policy
                                .map(Value::String)
                                .unwrap_or_else(|| Value::String("dirty_region".into())),
                        ),
                        (
                            "overwrite_previous_pixels".into(),
                            Value::Bool(request.overwrite_previous_pixels),
                        ),
                        (
                            "result".into(),
                            Value::String("live_image_stream_pending".into()),
                        ),
                        (
                            "daqmx_task_plan".into(),
                            lsm_raster_task_plan(
                                &self.config,
                                self.runtime_probe.as_ref(),
                                &request.scan,
                                &request.reconstruction,
                                true,
                            ),
                        ),
                    ]),
                )),
            (ChildKind::Hub, 3, CapabilityRequest::ScanSignalStream(request)) => Ok(self
                .api_summary(
                    device,
                    "scan_signal_stream",
                    BTreeMap::from([
                        (
                            "timing_fields".into(),
                            Value::I64(request.timing.len() as i64),
                        ),
                        (
                            "channel_count".into(),
                            Value::I64(request.channels.len() as i64),
                        ),
                        (
                            "channel_names".into(),
                            Value::List(
                                request
                                    .channels
                                    .iter()
                                    .cloned()
                                    .map(Value::String)
                                    .collect(),
                            ),
                        ),
                        (
                            "chunk_size".into(),
                            request
                                .chunk_size
                                .map(|value| Value::I64(value as i64))
                                .unwrap_or(Value::Null),
                        ),
                        (
                            "result".into(),
                            Value::String("raw_signal_stream_pending".into()),
                        ),
                        (
                            "daqmx_task_plan".into(),
                            scan_signal_task_plan(
                                &self.config,
                                self.runtime_probe.as_ref(),
                                &request.timing,
                                &request.channels,
                                request.chunk_size,
                            ),
                        ),
                    ]),
                )),
            (ChildKind::AnalogOutput(_), 1, CapabilityRequest::Dac(request)) => {
                self.write_property(device, "voltage", request.value)
            }
            (ChildKind::DigitalOutput(_), 1, CapabilityRequest::DigitalIo(request)) => {
                self.write_property(device, "high", Value::Bool(request.mask & 1 != 0))
            }
            (ChildKind::DigitalOutput(_), 2, CapabilityRequest::Trigger(request)) => {
                let high = match request.action {
                    TriggerAction::Enable | TriggerAction::Pulse => true,
                    TriggerAction::Disable => false,
                };
                self.write_property(device, "high", Value::Bool(high))
            }
            (ChildKind::DigitalOutput(_), 3, CapabilityRequest::Trigger(request)) => {
                let high = match request.action {
                    TriggerAction::Enable | TriggerAction::Pulse => true,
                    TriggerAction::Disable => false,
                };
                self.write_property(device, "high", Value::Bool(high))
            }
            (ChildKind::AnalogInput(_), 1, CapabilityRequest::Adc(_)) => {
                self.read_property(device, "voltage")
            }
            (ChildKind::CounterInput(_), 1, CapabilityRequest::Measure(_)) => {
                self.read_property(device, "count")
            }
            (ChildKind::CounterOutput(_), 1, CapabilityRequest::PulseProgram(request)) => {
                let frequency = request
                    .interval
                    .filter(|interval| interval.seconds() > 0.0)
                    .map(|interval| Frequency::from_hertz(1.0 / interval.seconds()))
                    .unwrap_or(self.config.default_sample_rate);
                self.write_property(device, "frequency", Value::Frequency(frequency))
            }
            (ChildKind::CounterOutput(_), 2, CapabilityRequest::Trigger(request)) => {
                let action = match request.action {
                    TriggerAction::Enable => "enable",
                    TriggerAction::Disable => "disable",
                    TriggerAction::Pulse => "pulse",
                };
                let value = Value::Map(BTreeMap::from([
                    ("action".into(), Value::String(action.into())),
                    (
                        "completion_basis".into(),
                        Value::String("configured_state_only".into()),
                    ),
                ]));
                self.record_transaction(device, "counter_output_trigger", value.clone());
                Ok(value)
            }
            _ => Err(Error::new(
                ErrorCode::InvalidCommand,
                "capability is not available on DAQmx device",
            )),
        }
    }

    fn apply_state_set(&mut self, set: StateSet) -> Result<Value> {
        let mut map = BTreeMap::new();
        for write in set.writes {
            let value = self.write_property(write.device, &write.property, write.value)?;
            map.insert(format!("{}:{}", write.device.0 .0, write.property), value);
        }
        Ok(Value::Map(map))
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        let kind = self
            .child_kind(device)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown DAQmx device"))?;
        match (kind, key, value) {
            (ChildKind::AnalogOutput(_), "voltage", Value::Voltage(value)) => {
                if value.volts() < self.config.analog_min.volts()
                    || value.volts() > self.config.analog_max.volts()
                {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "voltage outside configured DAQmx range",
                    ));
                }
                Ok(())
            }
            (ChildKind::DigitalOutput(_), "high", Value::Bool(_)) => Ok(()),
            (ChildKind::CounterOutput(_), "frequency", Value::Frequency(_)) => Ok(()),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                "property is not writable or value type is invalid",
            )),
        }
    }

    fn validate_invoke(
        &self,
        device: DeviceId,
        capability: CapabilityId,
        request: &CapabilityRequest,
    ) -> Result<()> {
        let kind = self
            .child_kind(device)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown DAQmx device"))?;
        match (kind, capability.0, request) {
            (ChildKind::Hub, 1, CapabilityRequest::ConfocalImageCapture(_))
            | (ChildKind::Hub, 2, CapabilityRequest::ConfocalImageStream(_))
            | (ChildKind::Hub, 3, CapabilityRequest::ScanSignalStream(_)) => Ok(()),
            (ChildKind::AnalogOutput(_), 1, CapabilityRequest::Dac(request)) => {
                self.validate_write(device, "voltage", &request.value)
            }
            (ChildKind::DigitalOutput(_), 1, CapabilityRequest::DigitalIo(_))
            | (ChildKind::DigitalOutput(_), 2, CapabilityRequest::Trigger(_))
            | (ChildKind::DigitalOutput(_), 3, CapabilityRequest::Trigger(_))
            | (ChildKind::AnalogInput(_), 1, CapabilityRequest::Adc(_))
            | (ChildKind::CounterInput(_), 1, CapabilityRequest::Measure(_))
            | (ChildKind::CounterOutput(_), 1, CapabilityRequest::PulseProgram(_))
            | (ChildKind::CounterOutput(_), 2, CapabilityRequest::Trigger(_)) => Ok(()),
            _ => Err(Error::new(
                ErrorCode::InvalidCommand,
                "capability is not available on DAQmx device",
            )),
        }
    }

    fn record_transaction(&mut self, device: DeviceId, action: &str, value: Value) {
        self.config.last_transaction = Value::Map(BTreeMap::from([
            ("device".into(), Value::I64(device.0 .0 as i64)),
            ("action".into(), Value::String(action.into())),
            ("value".into(), value),
            (
                "completion_basis".into(),
                Value::String("configured_state_only".into()),
            ),
            (
                "evidence_status".into(),
                Value::String("pending_ni_daqmx_runtime_evidence".into()),
            ),
        ]));
    }

    fn api_summary(
        &mut self,
        device: DeviceId,
        action: &str,
        mut fields: BTreeMap<String, Value>,
    ) -> Value {
        fields.insert(
            "completion_basis".into(),
            Value::String("configured_api_only".into()),
        );
        fields.insert(
            "evidence_status".into(),
            Value::String("pending_ni_daqmx_runtime_evidence".into()),
        );
        fields.insert(
            "api_status".into(),
            Value::String("declared_not_live".into()),
        );
        fields.insert(
            "live_task_execution_requested".into(),
            Value::Bool(self.config.live_task_execution),
        );
        fields.insert("live_task_execution_ready".into(), Value::Bool(false));
        fields.insert(
            "live_task_execution_blocker".into(),
            Value::String(
                live_task_execution_blocker(&self.config, self.runtime_probe.as_ref()).into(),
            ),
        );
        fields.insert(
            "live_task_execution_readiness".into(),
            live_task_execution_readiness_plan(&self.config, self.runtime_probe.as_ref()),
        );
        let value = Value::Map(fields);
        self.record_transaction(device, action, value.clone());
        value
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }
}

impl Driver for ImSwitchDaqmxDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: format!("{} NI-DAQmx runtime resource", self.config.device_name),
            kind: "vendor_runtime.daq.ni_daqmx".into(),
            metadata: BTreeMap::from([
                (
                    "device_name".into(),
                    Value::String(self.config.device_name.clone()),
                ),
                (
                    "connected".into(),
                    Value::Bool(self.runtime_probe.is_some()),
                ),
                (
                    "runtime_package".into(),
                    self.config
                        .runtime_package
                        .clone()
                        .map(Value::String)
                        .unwrap_or(Value::Null),
                ),
                (
                    "runtime_version".into(),
                    self.config
                        .runtime_version
                        .clone()
                        .map(Value::String)
                        .unwrap_or(Value::Null),
                ),
                (
                    "runtime_platform".into(),
                    self.config
                        .runtime_platform
                        .clone()
                        .map(Value::String)
                        .unwrap_or(Value::Null),
                ),
                (
                    "runtime_license".into(),
                    self.config
                        .runtime_license
                        .clone()
                        .map(Value::String)
                        .unwrap_or(Value::Null),
                ),
                (
                    "sdk_header_path".into(),
                    self.config
                        .sdk_header_path
                        .clone()
                        .map(Value::String)
                        .unwrap_or(Value::Null),
                ),
                (
                    "sdk_header_sha256".into(),
                    self.config
                        .sdk_header_sha256
                        .clone()
                        .map(Value::String)
                        .unwrap_or(Value::Null),
                ),
                (
                    "backend_status".into(),
                    backend_status(&self.config, self.runtime_probe.as_ref()),
                ),
                (
                    "live_task_execution_requested".into(),
                    Value::Bool(self.config.live_task_execution),
                ),
                (
                    "lsm_role_channels".into(),
                    lsm_role_channels_value(&self.config),
                ),
                (
                    "evidence_status".into(),
                    Value::String("pending_ni_daqmx_runtime_evidence".into()),
                ),
            ]),
        }]
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        let runtime_probe = self.runtime_probe.as_ref();
        let mut devices = vec![hub_descriptor(
            self.id,
            self.hub,
            &self.config,
            runtime_probe,
        )];
        devices.extend((1..=self.config.analog_output_count).map(|channel| {
            analog_output_descriptor(
                self.id,
                device_id(self.id, AO_OFFSET, channel),
                channel,
                &self.config,
                runtime_probe,
            )
        }));
        devices.extend((1..=self.config.digital_output_count).map(|line| {
            digital_output_descriptor(
                self.id,
                device_id(self.id, DO_OFFSET, line),
                line,
                &self.config,
                runtime_probe,
            )
        }));
        devices.extend((1..=self.config.analog_input_count).map(|channel| {
            analog_input_descriptor(
                self.id,
                device_id(self.id, AI_OFFSET, channel),
                channel,
                &self.config,
                runtime_probe,
            )
        }));
        devices.extend((1..=self.config.counter_input_count).map(|channel| {
            counter_input_descriptor(
                self.id,
                device_id(self.id, CI_OFFSET, channel),
                channel,
                &self.config,
                runtime_probe,
            )
        }));
        devices.extend((1..=self.config.counter_output_count).map(|channel| {
            counter_output_descriptor(
                self.id,
                device_id(self.id, CO_OFFSET, channel),
                channel,
                &self.config,
                runtime_probe,
            )
        }));
        devices
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        match self.child_kind(device) {
            Some(ChildKind::Hub) => vec![
                capability(
                    1,
                    device,
                    CapabilityKind::ConfocalImageCapture,
                    ValueType::Map,
                ),
                capability(
                    2,
                    device,
                    CapabilityKind::ConfocalImageStream,
                    ValueType::Map,
                ),
                capability(3, device, CapabilityKind::ScanSignalStream, ValueType::Map),
            ],
            Some(ChildKind::AnalogOutput(_)) => vec![capability(
                1,
                device,
                CapabilityKind::Dac,
                ValueType::Voltage,
            )],
            Some(ChildKind::DigitalOutput(_)) => vec![
                capability(1, device, CapabilityKind::DigitalIo, ValueType::Bool),
                capability(2, device, CapabilityKind::TriggerSource, ValueType::Bool),
                capability(3, device, CapabilityKind::TriggerSink, ValueType::Bool),
            ],
            Some(ChildKind::AnalogInput(_)) => vec![capability(
                1,
                device,
                CapabilityKind::Adc,
                ValueType::Voltage,
            )],
            Some(ChildKind::CounterInput(_)) => vec![capability(
                1,
                device,
                CapabilityKind::Measure,
                ValueType::I64,
            )],
            Some(ChildKind::CounterOutput(_)) => vec![
                capability(1, device, CapabilityKind::PulseProgram, ValueType::Map),
                capability(2, device, CapabilityKind::TriggerSource, ValueType::Map),
            ],
            None => Vec::new(),
        }
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    let _ = self.read_property(*device, key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => self.validate_invoke(*device, *capability, request)?,
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                }
                Command::Arm(_) | Command::Start(_) | Command::Stop(_) => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "DAQmx timing-plan execution is pending NI task evidence",
                    ))
                }
            }
        }
        Ok(PreparedBatch {
            id: batch.id,
            commands: batch.commands.clone(),
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "ImSwitch DAQmx configured-state transaction".into(),
                payload: Value::Map(BTreeMap::from([
                    (
                        "completion_basis".into(),
                        Value::String("configured_state_only".into()),
                    ),
                    (
                        "command_count".into(),
                        Value::I64(batch.commands.len() as i64),
                    ),
                ])),
            }],
        })
    }

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.token();
        let mut last = Value::Null;
        for command in prepared.commands {
            last = match command {
                Command::ReadProperty { device, key } => self.read_property(device, &key)?,
                Command::WriteProperty { device, key, value } => {
                    self.write_property(device, &key, value)?
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => self.invoke(device, capability, request)?,
                Command::ApplyStateSet(set) => self.apply_state_set(set)?,
                Command::Arm(_) | Command::Start(_) | Command::Stop(_) => Value::Null,
            };
        }
        self.pending
            .push_back(DriverEvent::TokenCompleted { token, value: last });
        Ok(token)
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        self.pending.drain(..).collect()
    }
}

fn device_id(driver: DriverId, offset: u64, channel: usize) -> DeviceId {
    DeviceId(NodeId(driver.0 * DRIVER_ID_BLOCK + offset + channel as u64))
}

fn hub_descriptor(
    driver: DriverId,
    id: DeviceId,
    config: &ImSwitchDaqmxConfig,
    runtime_probe: Option<&DaqmxRuntimeProbe>,
) -> DeviceDescriptor {
    DeviceDescriptor {
        id,
        driver,
        label: format!("{}-imswitch-daqmx-hub", config.device_name),
        vendor: Some("National Instruments".into()),
        model: Some(config.product.clone()),
        serial: Some(config.serial_number.clone()),
        kinds: vec![
            "hub".into(),
            "daq".into(),
            "ni.daqmx".into(),
            "imswitch.daqmx".into(),
        ],
        properties: vec![
            string_property("device_name", "Device name", false),
            string_property("product", "Product", false),
            string_property("serial_number", "Serial number", false),
            string_property("runtime_package", "Runtime package", false),
            string_property("runtime_version", "Runtime version", false),
            string_property("runtime_platform", "Runtime platform", false),
            string_property("runtime_license", "Runtime license", false),
            string_property("sdk_header_path", "SDK header path", false),
            string_property("sdk_header_sha256", "SDK header SHA-256", false),
            map_property("backend_status", "Backend status", false),
            bool_property("connected", "Connected", false, false),
            bool_property(
                "live_task_execution",
                "Live task execution requested",
                false,
                false,
            ),
            bool_property("inventory_devices", "Inventory devices", false, false),
            string_property("inventory_helper_path", "Inventory helper path", false),
            time_interval_property(
                "inventory_helper_timeout",
                "Inventory helper timeout",
                false,
            ),
            string_property("lsm_x_galvo", "LSM X galvo role channel", false),
            string_property("lsm_y_galvo", "LSM Y galvo role channel", false),
            string_property("lsm_laser_gate", "LSM laser gate role channel", false),
            string_property("lsm_detector", "LSM detector role channel", false),
            string_property("lsm_sample_clock", "LSM sample clock role channel", false),
            string_property(
                "lsm_sample_clock_source",
                "LSM sample clock source route",
                false,
            ),
            string_property(
                "lsm_start_trigger_source",
                "LSM start trigger source route",
                false,
            ),
            frequency_property("default_sample_rate", "Default sample rate", false),
            time_interval_property("daqmx_timeout", "DAQmx timeout", false),
            map_property("last_transaction", "Last transaction", false),
        ],
        metadata: common_metadata(config, runtime_probe),
    }
}

fn analog_output_descriptor(
    driver: DriverId,
    id: DeviceId,
    channel: usize,
    config: &ImSwitchDaqmxConfig,
    runtime_probe: Option<&DaqmxRuntimeProbe>,
) -> DeviceDescriptor {
    DeviceDescriptor {
        id,
        driver,
        label: format!("{}-ao{}", config.device_name, channel - 1),
        vendor: Some("National Instruments".into()),
        model: Some(config.product.clone()),
        serial: Some(config.serial_number.clone()),
        kinds: vec!["analog.output".into(), "dac".into(), "trigger.sink".into()],
        properties: vec![
            integer_property("channel", "Channel", false),
            string_property("physical_channel", "Physical channel", false),
            voltage_range_property(
                "voltage",
                "Voltage",
                true,
                true,
                config.analog_min,
                config.analog_max,
            ),
            voltage_range_property(
                "voltage_min",
                "Voltage min",
                false,
                false,
                config.analog_min,
                config.analog_max,
            ),
            voltage_range_property(
                "voltage_max",
                "Voltage max",
                false,
                false,
                config.analog_min,
                config.analog_max,
            ),
        ],
        metadata: channel_metadata(config, channel, runtime_probe),
    }
}

fn digital_output_descriptor(
    driver: DriverId,
    id: DeviceId,
    line: usize,
    config: &ImSwitchDaqmxConfig,
    runtime_probe: Option<&DaqmxRuntimeProbe>,
) -> DeviceDescriptor {
    DeviceDescriptor {
        id,
        driver,
        label: format!("{}-port0-line{}", config.device_name, line - 1),
        vendor: Some("National Instruments".into()),
        model: Some(config.product.clone()),
        serial: Some(config.serial_number.clone()),
        kinds: vec![
            "digital.output".into(),
            "ttl.output".into(),
            "trigger.source".into(),
            "trigger.sink".into(),
        ],
        properties: vec![
            integer_property("line", "Line", false),
            string_property("physical_line", "Physical line", false),
            bool_property("high", "High", true, true),
        ],
        metadata: channel_metadata(config, line, runtime_probe),
    }
}

fn analog_input_descriptor(
    driver: DriverId,
    id: DeviceId,
    channel: usize,
    config: &ImSwitchDaqmxConfig,
    runtime_probe: Option<&DaqmxRuntimeProbe>,
) -> DeviceDescriptor {
    DeviceDescriptor {
        id,
        driver,
        label: format!("{}-ai{}", config.device_name, channel - 1),
        vendor: Some("National Instruments".into()),
        model: Some(config.product.clone()),
        serial: Some(config.serial_number.clone()),
        kinds: vec!["analog.input".into(), "adc".into()],
        properties: vec![
            integer_property("channel", "Channel", false),
            string_property("physical_channel", "Physical channel", false),
            voltage_range_property(
                "voltage",
                "Voltage",
                false,
                false,
                config.analog_min,
                config.analog_max,
            ),
        ],
        metadata: channel_metadata(config, channel, runtime_probe),
    }
}

fn counter_input_descriptor(
    driver: DriverId,
    id: DeviceId,
    channel: usize,
    config: &ImSwitchDaqmxConfig,
    runtime_probe: Option<&DaqmxRuntimeProbe>,
) -> DeviceDescriptor {
    DeviceDescriptor {
        id,
        driver,
        label: format!("{}-ci{}", config.device_name, channel - 1),
        vendor: Some("National Instruments".into()),
        model: Some(config.product.clone()),
        serial: Some(config.serial_number.clone()),
        kinds: vec![
            "counter".into(),
            "counter.input".into(),
            "digital.input.counter".into(),
        ],
        properties: vec![
            integer_property("channel", "Channel", false),
            string_property("physical_channel", "Physical channel", false),
            integer_property("count", "Count", false),
            enum_string_property("edge", "Edge", false, &["Rising", "Falling"]),
        ],
        metadata: channel_metadata(config, channel, runtime_probe),
    }
}

fn counter_output_descriptor(
    driver: DriverId,
    id: DeviceId,
    channel: usize,
    config: &ImSwitchDaqmxConfig,
    runtime_probe: Option<&DaqmxRuntimeProbe>,
) -> DeviceDescriptor {
    DeviceDescriptor {
        id,
        driver,
        label: format!("{}-co{}", config.device_name, channel - 1),
        vendor: Some("National Instruments".into()),
        model: Some(config.product.clone()),
        serial: Some(config.serial_number.clone()),
        kinds: vec![
            "counter.output".into(),
            "clock.output".into(),
            "trigger.source".into(),
        ],
        properties: vec![
            integer_property("channel", "Channel", false),
            string_property("physical_channel", "Physical channel", false),
            frequency_property("frequency", "Frequency", true),
        ],
        metadata: channel_metadata(config, channel, runtime_probe),
    }
}

fn common_metadata(
    config: &ImSwitchDaqmxConfig,
    runtime_probe: Option<&DaqmxRuntimeProbe>,
) -> BTreeMap<String, Value> {
    BTreeMap::from([
        (
            "source_context".into(),
            Value::String("ImSwitch NidaqManager role model".into()),
        ),
        (
            "support_status".into(),
            Value::String(if runtime_probe.is_some() {
                "runtime_probe_only".into()
            } else {
                "configured_descriptor_only".into()
            }),
        ),
        (
            "evidence_status".into(),
            Value::String("pending_ni_daqmx_runtime_evidence".into()),
        ),
        (
            "device_name".into(),
            Value::String(config.device_name.clone()),
        ),
        (
            "backend_status".into(),
            backend_status(config, runtime_probe),
        ),
        (
            "live_task_execution_requested".into(),
            Value::Bool(config.live_task_execution),
        ),
        ("lsm_role_channels".into(), lsm_role_channels_value(config)),
        ("lsm_routing".into(), lsm_routing_value(config)),
    ])
}

fn lsm_raster_task_plan(
    config: &ImSwitchDaqmxConfig,
    runtime_probe: Option<&DaqmxRuntimeProbe>,
    scan: &BTreeMap<String, Value>,
    reconstruction: &BTreeMap<String, Value>,
    streaming: bool,
) -> Value {
    let width = map_pixel_count(scan, "width")
        .or_else(|| map_pixel_count(reconstruction, "image_width"))
        .unwrap_or(512)
        .max(1);
    let height = map_pixel_count(scan, "height")
        .or_else(|| map_pixel_count(reconstruction, "image_height"))
        .unwrap_or(512)
        .max(1);
    let frames = map_i64(scan, "frames").unwrap_or(if streaming { 0 } else { 1 });
    let finite_frames = if streaming { 1 } else { frames.max(1) as u64 };
    let samples_per_frame = width as u64 * height as u64;
    let planned_samples = samples_per_frame.saturating_mul(finite_frames);
    let sample_rate_hz =
        map_frequency_hz(scan, "sample_rate").unwrap_or_else(|| config.default_sample_rate.hertz());
    let reconstruction_width = map_pixel_count(reconstruction, "image_width").unwrap_or(width);
    let reconstruction_height = map_pixel_count(reconstruction, "image_height").unwrap_or(height);
    let pixel_format =
        map_string(reconstruction, "pixel_format").unwrap_or_else(|| "Mono16".into());

    let x_galvo = map_string(scan, "x_galvo").unwrap_or_else(|| config.lsm_x_galvo.clone());
    let y_galvo = map_string(scan, "y_galvo").unwrap_or_else(|| config.lsm_y_galvo.clone());
    let laser_gate =
        map_string(scan, "laser_gate").unwrap_or_else(|| config.lsm_laser_gate.clone());
    let detector = map_string(scan, "detector").unwrap_or_else(|| config.lsm_detector.clone());
    let sample_clock =
        map_string(scan, "sample_clock").unwrap_or_else(|| config.lsm_sample_clock.clone());
    let start_trigger =
        map_string(scan, "start_trigger").or_else(|| config.lsm_start_trigger_source.clone());
    let sample_clock_source =
        map_string(scan, "sample_clock_source").or_else(|| config.lsm_sample_clock_source.clone());
    let physical_sample_clock = physical_channel(config, &sample_clock);
    let effective_sample_clock_source = sample_clock_source
        .clone()
        .or_else(|| counter_internal_output_source(&physical_sample_clock));
    let sample_clock_source_origin = if sample_clock_source.is_some() {
        "explicit"
    } else if effective_sample_clock_source.is_some() {
        "derived_counter_output_internal"
    } else {
        "default_task_timebase"
    };
    let role_channels = role_channels_value(
        config,
        &[
            ("x_galvo", &x_galvo),
            ("y_galvo", &y_galvo),
            ("laser_gate", &laser_gate),
            ("detector", &detector),
            ("sample_clock", &sample_clock),
        ],
    );
    let invalid_role_channels =
        invalid_raster_role_channels(&x_galvo, &y_galvo, &laser_gate, &detector, &sample_clock);
    let validation_status = if invalid_role_channels.is_empty() {
        "valid"
    } else {
        "invalid_role_channels"
    };

    let mut tasks = vec![
        task_plan(
            "ao_scan",
            "analog_output",
            vec![
                physical_channel(config, &x_galvo),
                physical_channel(config, &y_galvo),
            ],
            vec![
                "DAQmxCreateTask",
                "DAQmxCreateAOVoltageChan",
                "DAQmxCfgSampClkTiming",
                "DAQmxWriteAnalogF64",
            ],
            planned_samples,
            sample_rate_hz,
            config.daqmx_timeout,
            Some(ao_raster_waveform_plan(
                width,
                height,
                finite_frames,
                config.analog_min,
                config.analog_max,
            )),
        ),
        task_plan(
            "do_laser_gate",
            "digital_output",
            vec![physical_channel(config, &laser_gate)],
            vec![
                "DAQmxCreateTask",
                "DAQmxCreateDOChan",
                "DAQmxCfgSampClkTiming",
                "DAQmxWriteDigitalLines",
            ],
            planned_samples,
            sample_rate_hz,
            config.daqmx_timeout,
            Some(do_laser_gate_waveform_plan(width, height, finite_frames)),
        ),
    ];
    let mut task_names = vec!["ao_scan".to_owned(), "do_laser_gate".to_owned()];
    let mut input_tasks = Vec::new();

    let detector_physical = physical_channel(config, &detector);
    if is_counter_channel(&detector) {
        input_tasks.push("ci_detector".to_owned());
        task_names.push("ci_detector".to_owned());
        tasks.push(task_plan(
            "ci_detector",
            "counter_input",
            vec![detector_physical],
            vec![
                "DAQmxCreateTask",
                "DAQmxCreateCICountEdgesChan",
                "DAQmxCfgSampClkTiming",
                "DAQmxReadCounterU32",
            ],
            planned_samples,
            sample_rate_hz,
            config.daqmx_timeout,
            None,
        ));
    } else if is_ai_channel(&detector) {
        input_tasks.push("ai_detector".to_owned());
        task_names.push("ai_detector".to_owned());
        tasks.push(task_plan(
            "ai_detector",
            "analog_input",
            vec![detector_physical],
            vec![
                "DAQmxCreateTask",
                "DAQmxCreateAIVoltageChan",
                "DAQmxCfgSampClkTiming",
                "DAQmxReadAnalogF64",
            ],
            planned_samples,
            sample_rate_hz,
            config.daqmx_timeout,
            None,
        ));
    }

    if config.counter_output_count > 0 {
        task_names.push("co_sample_clock".to_owned());
        tasks.push(task_plan(
            "co_sample_clock",
            "counter_output",
            vec![physical_sample_clock],
            vec![
                "DAQmxCreateTask",
                "DAQmxCreateCOPulseChanFreq",
                "DAQmxCfgImplicitTiming",
            ],
            planned_samples,
            sample_rate_hz,
            config.daqmx_timeout,
            None,
        ));
    }
    let clock_role = if config.counter_output_count > 0 {
        Some("co_sample_clock")
    } else {
        None
    };
    let mut buffered_tasks = input_tasks.clone();
    buffered_tasks.push("ao_scan".into());
    buffered_tasks.push("do_laser_gate".into());
    let mut start_order = input_tasks.clone();
    start_order.push("ao_scan".into());
    start_order.push("do_laser_gate".into());
    if let Some(clock_role) = clock_role {
        start_order.push(clock_role.into());
    }
    let write_order = vec!["ao_scan".to_owned(), "do_laser_gate".to_owned()];
    let wait_order = clock_role
        .map(|role| vec![role.to_owned()])
        .unwrap_or_default();
    let stop_order = reversed_string_list(&start_order);
    let clear_order = reversed_string_list(&task_names);
    let plan_setup_helper_command = plan_setup_helper_command(
        &tasks,
        sample_rate_hz,
        planned_samples,
        effective_sample_clock_source.as_deref(),
        start_trigger.as_deref(),
        config.analog_min,
        config.analog_max,
        config.daqmx_timeout,
        Some((width as u64, height as u64, finite_frames)),
        None,
    );
    let plan_preflight_helper_command = preflight_helper_command(&plan_setup_helper_command);
    let helper_command_runnable = invalid_role_channels.is_empty() && !tasks.is_empty();

    Value::Map(BTreeMap::from([
        (
            "planner_status".into(),
            Value::String("configured_task_plan_only".into()),
        ),
        (
            "plan_validation".into(),
            task_plan_validation(tasks.len(), &[], &invalid_role_channels, validation_status),
        ),
        (
            "execution_status".into(),
            Value::String("not_live_task_execution".into()),
        ),
        (
            "live_task_execution_requested".into(),
            Value::Bool(config.live_task_execution),
        ),
        ("live_task_execution_ready".into(), Value::Bool(false)),
        (
            "live_task_execution_blocker".into(),
            Value::String(live_task_execution_blocker(config, runtime_probe).into()),
        ),
        (
            "live_task_execution_readiness".into(),
            live_task_execution_readiness_plan(config, runtime_probe),
        ),
        (
            "execution_gate".into(),
            Value::String("not_live_task_execution".into()),
        ),
        ("streaming".into(), Value::Bool(streaming)),
        ("role_channels".into(), role_channels),
        ("width".into(), Value::PixelCount(PixelCount::new(width))),
        ("height".into(), Value::PixelCount(PixelCount::new(height))),
        ("frames".into(), Value::I64(frames)),
        (
            "sample_rate".into(),
            Value::Frequency(Frequency::from_hertz(sample_rate_hz)),
        ),
        (
            "samples_per_frame".into(),
            Value::I64(samples_per_frame.min(i64::MAX as u64) as i64),
        ),
        (
            "planned_samples_per_channel".into(),
            i64_value_u64(planned_samples),
        ),
        (
            "scan_buffer_plan".into(),
            raster_buffer_plan(width, height, finite_frames, planned_samples),
        ),
        (
            "plan_setup_helper_command".into(),
            helper_command_value(&plan_setup_helper_command, helper_command_runnable),
        ),
        (
            "plan_preflight_helper_command".into(),
            helper_command_value(&plan_preflight_helper_command, helper_command_runnable),
        ),
        (
            "sample_clock_source".into(),
            effective_sample_clock_source
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        ),
        (
            "sample_clock_source_origin".into(),
            Value::String(sample_clock_source_origin.into()),
        ),
        (
            "clock_task".into(),
            clock_role
                .map(|role| Value::String(role.into()))
                .unwrap_or(Value::Null),
        ),
        (
            "start_trigger_source".into(),
            start_trigger
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        ),
        (
            "routing_plan".into(),
            routing_plan(
                clock_role,
                &buffered_tasks,
                effective_sample_clock_source.as_deref(),
                start_trigger.as_deref(),
            ),
        ),
        (
            "runtime_sequence".into(),
            runtime_sequence_plan(RuntimeSequencePlan {
                setup_order: &task_names,
                write_order: &write_order,
                start_order: &start_order,
                read_order: &input_tasks,
                wait_order: &wait_order,
                stop_order: &stop_order,
                clear_order: &clear_order,
            }),
        ),
        (
            "completion_plan".into(),
            completion_plan(planned_samples, config.daqmx_timeout),
        ),
        (
            "execution_contract".into(),
            execution_contract(
                "raster_finite",
                &write_order,
                &input_tasks,
                &wait_order,
                config.daqmx_timeout,
            ),
        ),
        (
            "live_executor_plan".into(),
            live_executor_plan(
                "raster_finite",
                LiveExecutorOrders {
                    setup_order: &task_names,
                    write_order: &write_order,
                    start_order: &start_order,
                    read_order: &input_tasks,
                    wait_order: &wait_order,
                    stop_order: &stop_order,
                    clear_order: &clear_order,
                },
                config,
                runtime_probe,
            ),
        ),
        (
            "reconstruction_plan".into(),
            raster_reconstruction_plan(
                width,
                height,
                reconstruction_width,
                reconstruction_height,
                &pixel_format,
                &input_tasks,
            ),
        ),
        (
            "publication_plan".into(),
            raster_publication_plan(
                width,
                height,
                reconstruction_width,
                reconstruction_height,
                &pixel_format,
                streaming,
            ),
        ),
        ("start_order".into(), string_list(&start_order)),
        ("read_order".into(), string_list(&input_tasks)),
        ("stop_order".into(), string_list(&stop_order)),
        ("clear_order".into(), string_list(&clear_order)),
        (
            "cleanup_policy".into(),
            Value::String("stop_started_tasks_then_clear_all_created_tasks".into()),
        ),
        (
            "cleanup_plan".into(),
            cleanup_plan(&start_order, &clear_order, config.daqmx_timeout),
        ),
        (
            "cancel_plan".into(),
            cancel_plan(&start_order, &clear_order, config.daqmx_timeout),
        ),
        (
            "routing_evidence_status".into(),
            Value::String("pending_hardware_validation".into()),
        ),
        ("tasks".into(), Value::List(tasks)),
    ]))
}

fn scan_signal_task_plan(
    config: &ImSwitchDaqmxConfig,
    runtime_probe: Option<&DaqmxRuntimeProbe>,
    timing: &BTreeMap<String, Value>,
    channels: &[String],
    chunk_size: Option<u64>,
) -> Value {
    let samples_per_line = map_i64(timing, "samples_per_line").unwrap_or(512).max(1) as u64;
    let lines = map_i64(timing, "lines").unwrap_or(1).max(1) as u64;
    let planned_samples = samples_per_line.saturating_mul(lines);
    let sample_rate_hz = map_frequency_hz(timing, "sample_rate")
        .unwrap_or_else(|| config.default_sample_rate.hertz());
    let start_trigger =
        map_string(timing, "start_trigger").or_else(|| config.lsm_start_trigger_source.clone());
    let sample_clock_source = map_string(timing, "sample_clock_source")
        .or_else(|| config.lsm_sample_clock_source.clone());

    let mut ci_channels = Vec::new();
    let mut ai_channels = Vec::new();
    let mut ignored_channels = Vec::new();
    for channel in channels {
        let physical = physical_channel(config, channel);
        if is_counter_channel(channel) {
            ci_channels.push(physical);
        } else if is_ai_channel(channel) {
            ai_channels.push(physical);
        } else {
            ignored_channels.push(channel.clone());
        }
    }

    let mut tasks = Vec::new();
    let mut task_names = Vec::new();
    let mut read_order = Vec::new();
    if !ci_channels.is_empty() {
        task_names.push("ci_signal".to_owned());
        read_order.push("ci_signal".to_owned());
        tasks.push(task_plan(
            "ci_signal",
            "counter_input",
            ci_channels,
            vec![
                "DAQmxCreateTask",
                "DAQmxCreateCICountEdgesChan",
                "DAQmxCfgSampClkTiming",
                "DAQmxReadCounterU32",
            ],
            planned_samples,
            sample_rate_hz,
            config.daqmx_timeout,
            None,
        ));
    }
    if !ai_channels.is_empty() {
        task_names.push("ai_signal".to_owned());
        read_order.push("ai_signal".to_owned());
        tasks.push(task_plan(
            "ai_signal",
            "analog_input",
            ai_channels,
            vec![
                "DAQmxCreateTask",
                "DAQmxCreateAIVoltageChan",
                "DAQmxCfgSampClkTiming",
                "DAQmxReadAnalogF64",
            ],
            planned_samples,
            sample_rate_hz,
            config.daqmx_timeout,
            None,
        ));
    }
    let stop_order = reversed_string_list(&task_names);
    let clear_order = stop_order.clone();
    let empty_order = Vec::new();
    let validation_status = if tasks.is_empty() {
        "invalid_no_recognized_channels"
    } else if ignored_channels.is_empty() {
        "valid"
    } else {
        "partial_unrecognized_channels"
    };
    let helper_command_runnable = !tasks.is_empty() && ignored_channels.is_empty();
    let plan_setup_helper_command = plan_setup_helper_command(
        &tasks,
        sample_rate_hz,
        planned_samples,
        sample_clock_source.as_deref(),
        start_trigger.as_deref(),
        config.analog_min,
        config.analog_max,
        config.daqmx_timeout,
        None,
        Some((lines, chunk_size)),
    );
    let plan_preflight_helper_command = preflight_helper_command(&plan_setup_helper_command);

    Value::Map(BTreeMap::from([
        (
            "planner_status".into(),
            Value::String("configured_task_plan_only".into()),
        ),
        (
            "plan_validation".into(),
            task_plan_validation(tasks.len(), &ignored_channels, &[], validation_status),
        ),
        (
            "execution_status".into(),
            Value::String("not_live_task_execution".into()),
        ),
        (
            "live_task_execution_requested".into(),
            Value::Bool(config.live_task_execution),
        ),
        ("live_task_execution_ready".into(), Value::Bool(false)),
        (
            "live_task_execution_blocker".into(),
            Value::String(live_task_execution_blocker(config, runtime_probe).into()),
        ),
        (
            "live_task_execution_readiness".into(),
            live_task_execution_readiness_plan(config, runtime_probe),
        ),
        (
            "execution_gate".into(),
            Value::String("not_live_task_execution".into()),
        ),
        (
            "samples_per_line".into(),
            Value::I64(samples_per_line.min(i64::MAX as u64) as i64),
        ),
        (
            "lines".into(),
            Value::I64(lines.min(i64::MAX as u64) as i64),
        ),
        (
            "sample_rate".into(),
            Value::Frequency(Frequency::from_hertz(sample_rate_hz)),
        ),
        (
            "planned_samples_per_channel".into(),
            i64_value_u64(planned_samples),
        ),
        (
            "signal_buffer_plan".into(),
            signal_buffer_plan(samples_per_line, lines, chunk_size, planned_samples),
        ),
        (
            "plan_setup_helper_command".into(),
            helper_command_value(&plan_setup_helper_command, helper_command_runnable),
        ),
        (
            "plan_preflight_helper_command".into(),
            helper_command_value(&plan_preflight_helper_command, helper_command_runnable),
        ),
        (
            "chunk_size".into(),
            chunk_size
                .map(|value| Value::I64(value.min(i64::MAX as u64) as i64))
                .unwrap_or(Value::Null),
        ),
        (
            "sample_clock_source".into(),
            sample_clock_source
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        ),
        (
            "start_trigger_source".into(),
            start_trigger
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        ),
        (
            "routing_plan".into(),
            routing_plan(
                None,
                &task_names,
                sample_clock_source.as_deref(),
                start_trigger.as_deref(),
            ),
        ),
        (
            "runtime_sequence".into(),
            runtime_sequence_plan(RuntimeSequencePlan {
                setup_order: &task_names,
                write_order: &empty_order,
                start_order: &task_names,
                read_order: &read_order,
                wait_order: &empty_order,
                stop_order: &stop_order,
                clear_order: &clear_order,
            }),
        ),
        (
            "completion_plan".into(),
            completion_plan(planned_samples, config.daqmx_timeout),
        ),
        (
            "execution_contract".into(),
            execution_contract(
                "signal_finite",
                &empty_order,
                &read_order,
                &empty_order,
                config.daqmx_timeout,
            ),
        ),
        (
            "live_executor_plan".into(),
            live_executor_plan(
                "signal_finite",
                LiveExecutorOrders {
                    setup_order: &task_names,
                    write_order: &empty_order,
                    start_order: &task_names,
                    read_order: &read_order,
                    wait_order: &empty_order,
                    stop_order: &stop_order,
                    clear_order: &clear_order,
                },
                config,
                runtime_probe,
            ),
        ),
        (
            "publication_plan".into(),
            signal_publication_plan(channels, samples_per_line, lines, chunk_size),
        ),
        ("start_order".into(), string_list(&task_names)),
        ("read_order".into(), string_list(&read_order)),
        ("stop_order".into(), string_list(&stop_order)),
        ("clear_order".into(), string_list(&clear_order)),
        ("ignored_channels".into(), string_list(&ignored_channels)),
        (
            "cleanup_policy".into(),
            Value::String("stop_started_tasks_then_clear_all_created_tasks".into()),
        ),
        (
            "cleanup_plan".into(),
            cleanup_plan(&task_names, &clear_order, config.daqmx_timeout),
        ),
        (
            "cancel_plan".into(),
            cancel_plan(&task_names, &clear_order, config.daqmx_timeout),
        ),
        (
            "routing_evidence_status".into(),
            Value::String("pending_hardware_validation".into()),
        ),
        ("tasks".into(), Value::List(tasks)),
    ]))
}

struct RuntimeSequencePlan<'a> {
    setup_order: &'a [String],
    write_order: &'a [String],
    start_order: &'a [String],
    read_order: &'a [String],
    wait_order: &'a [String],
    stop_order: &'a [String],
    clear_order: &'a [String],
}

fn runtime_sequence_plan(plan: RuntimeSequencePlan<'_>) -> Value {
    let mut phases = vec![
        runtime_sequence_phase(1, "setup", plan.setup_order, "create_channels_and_timing"),
        runtime_sequence_phase(3, "start", plan.start_order, "inputs_outputs_then_clock"),
    ];
    if !plan.write_order.is_empty() {
        phases.insert(
            1,
            runtime_sequence_phase(2, "write", plan.write_order, "buffered_output_before_start"),
        );
    }
    if !plan.read_order.is_empty() {
        phases.push(runtime_sequence_phase(
            4,
            "read",
            plan.read_order,
            "finite_samples",
        ));
    }
    if !plan.wait_order.is_empty() {
        phases.push(runtime_sequence_phase(
            5,
            "wait",
            plan.wait_order,
            "counter_output_done_or_timeout",
        ));
    }
    phases.push(runtime_sequence_phase(
        6,
        "stop",
        plan.stop_order,
        "reverse_started_order",
    ));
    phases.push(runtime_sequence_phase(
        7,
        "clear",
        plan.clear_order,
        "reverse_setup_order",
    ));
    Value::List(phases)
}

fn runtime_sequence_phase(step: i64, phase: &str, tasks: &[String], basis: &str) -> Value {
    Value::Map(BTreeMap::from([
        ("step".into(), Value::I64(step)),
        ("phase".into(), Value::String(phase.into())),
        ("tasks".into(), string_list(tasks)),
        ("basis".into(), Value::String(basis.into())),
        (
            "evidence_status".into(),
            Value::String("pending_hardware_validation".into()),
        ),
    ]))
}

fn completion_plan(samples_per_channel: u64, daqmx_timeout: TimeInterval) -> Value {
    Value::Map(BTreeMap::from([
        ("mode".into(), Value::String("finite".into())),
        (
            "samples_per_channel".into(),
            i64_value_u64(samples_per_channel),
        ),
        ("timeout".into(), Value::TimeInterval(daqmx_timeout)),
        (
            "evidence_status".into(),
            Value::String("pending_hardware_validation".into()),
        ),
    ]))
}

fn execution_contract(
    mode: &str,
    write_order: &[String],
    read_order: &[String],
    wait_order: &[String],
    daqmx_timeout: TimeInterval,
) -> Value {
    Value::Map(BTreeMap::from([
        ("mode".into(), Value::String(mode.into())),
        ("write_order".into(), string_list(write_order)),
        (
            "write_policy".into(),
            Value::String("buffered_before_start".into()),
        ),
        ("write_auto_start".into(), Value::Bool(false)),
        (
            "write_layout".into(),
            Value::String("GroupByScanNumber".into()),
        ),
        ("read_order".into(), string_list(read_order)),
        (
            "read_policy".into(),
            Value::String("finite_expected_samples".into()),
        ),
        (
            "read_layout".into(),
            Value::String("GroupByScanNumber_for_analog_input".into()),
        ),
        ("wait_order".into(), string_list(wait_order)),
        (
            "wait_policy".into(),
            Value::String("counter_output_done_or_timeout".into()),
        ),
        ("timeout".into(), Value::TimeInterval(daqmx_timeout)),
        (
            "partial_output_policy".into(),
            Value::String("reject_until_hardware_validated".into()),
        ),
        (
            "publication_policy".into(),
            Value::String("publish_only_after_validated_read_and_reconstruction".into()),
        ),
        (
            "contract_evidence_status".into(),
            Value::String("pending_hardware_validation".into()),
        ),
    ]))
}

struct LiveExecutorOrders<'a> {
    setup_order: &'a [String],
    write_order: &'a [String],
    start_order: &'a [String],
    read_order: &'a [String],
    wait_order: &'a [String],
    stop_order: &'a [String],
    clear_order: &'a [String],
}

fn live_executor_plan(
    mode: &str,
    orders: LiveExecutorOrders<'_>,
    config: &ImSwitchDaqmxConfig,
    runtime_probe: Option<&DaqmxRuntimeProbe>,
) -> Value {
    Value::Map(BTreeMap::from([
        ("mode".into(), Value::String(mode.into())),
        (
            "executor_status".into(),
            Value::String("not_enabled_pending_hardware_validation".into()),
        ),
        (
            "backend".into(),
            Value::String("ni_daqmx_sdk_task_wrapper".into()),
        ),
        (
            "target_scope".into(),
            Value::String("linux_windows_optional_sdk_backend".into()),
        ),
        (
            "readiness".into(),
            live_task_execution_readiness_plan(config, runtime_probe),
        ),
        (
            "phases".into(),
            Value::List(vec![
                live_executor_phase(
                    1,
                    "validate_readiness",
                    &[],
                    "check_feature_target_package_header_runtime_live_request_and_external_gates",
                ),
                live_executor_phase(
                    2,
                    "setup",
                    orders.setup_order,
                    "DAQmxCreateTask+channel_creation+timing_and_trigger_configuration",
                ),
                live_executor_phase(
                    3,
                    "write",
                    orders.write_order,
                    "DAQmxWriteAnalogF64+DAQmxWriteDigitalLines_buffered_auto_start_false",
                ),
                live_executor_phase(4, "start", orders.start_order, "DAQmxStartTask"),
                live_executor_phase(
                    5,
                    "read",
                    orders.read_order,
                    "DAQmxReadCounterU32+DAQmxReadAnalogF64_finite_expected_samples",
                ),
                live_executor_phase(6, "wait", orders.wait_order, "DAQmxWaitUntilTaskDone"),
                live_executor_phase(
                    7,
                    "publish",
                    &[],
                    "publish_public_FrameReady_or_ScanSignalChunk_after_validated_read",
                ),
                live_executor_phase(
                    8,
                    "cleanup",
                    orders.stop_order,
                    "DAQmxStopTask_then_DAQmxClearTask_for_created_tasks",
                ),
                live_executor_phase(
                    9,
                    "clear",
                    orders.clear_order,
                    "DAQmxClearTask_reverse_setup_order",
                ),
            ]),
        ),
        (
            "required_validation".into(),
            string_list(&[
                "legal_review".into(),
                "installed_header_audit".into(),
                "ni_pal_device_inventory".into(),
                "bench_safety_preconditions".into(),
                "task_ordering_routing_completion_cleanup_bench_validation".into(),
                "runtime_publication_hardware_validation".into(),
                "hardware_validation_note".into(),
            ]),
        ),
        (
            "execution_evidence_status".into(),
            Value::String("pending_hardware_validation".into()),
        ),
    ]))
}

fn live_executor_phase(step: i64, phase: &str, tasks: &[String], api_surface: &str) -> Value {
    Value::Map(BTreeMap::from([
        ("step".into(), Value::I64(step)),
        ("phase".into(), Value::String(phase.into())),
        ("tasks".into(), string_list(tasks)),
        ("api_surface".into(), Value::String(api_surface.into())),
        (
            "evidence_status".into(),
            Value::String("pending_hardware_validation".into()),
        ),
    ]))
}

fn raster_reconstruction_plan(
    scan_width: u32,
    scan_height: u32,
    reconstruction_width: u32,
    reconstruction_height: u32,
    pixel_format: &str,
    input_tasks: &[String],
) -> Value {
    Value::Map(BTreeMap::from([
        (
            "mode".into(),
            Value::String("one_detector_sample_per_pixel".into()),
        ),
        ("input_tasks".into(), string_list(input_tasks)),
        (
            "source_element_type".into(),
            Value::String("u32_counts_or_f64_detector_samples".into()),
        ),
        (
            "sample_to_pixel_mapping".into(),
            Value::String("row_major_unidirectional_one_sample_per_pixel".into()),
        ),
        (
            "scan_width".into(),
            Value::PixelCount(PixelCount::new(scan_width)),
        ),
        (
            "scan_height".into(),
            Value::PixelCount(PixelCount::new(scan_height)),
        ),
        (
            "reconstruction_width".into(),
            Value::PixelCount(PixelCount::new(reconstruction_width)),
        ),
        (
            "reconstruction_height".into(),
            Value::PixelCount(PixelCount::new(reconstruction_height)),
        ),
        ("pixel_format".into(), Value::String(pixel_format.into())),
        (
            "accumulation".into(),
            Value::String("sum_samples_per_reconstructed_pixel".into()),
        ),
        (
            "background_subtraction".into(),
            Value::String("disabled_until_hardware_validated".into()),
        ),
        (
            "saturation_policy".into(),
            Value::String("clip_to_pixel_format_and_report_saturated_pixels".into()),
        ),
        (
            "publication_gate".into(),
            Value::String("publish_after_validated_read_and_reconstruction".into()),
        ),
        (
            "reconstruction_evidence_status".into(),
            Value::String("pending_hardware_validation".into()),
        ),
    ]))
}

fn raster_publication_plan(
    scan_width: u32,
    scan_height: u32,
    reconstruction_width: u32,
    reconstruction_height: u32,
    pixel_format: &str,
    streaming: bool,
) -> Value {
    Value::Map(BTreeMap::from([
        ("event_kind".into(), Value::String("FrameReady".into())),
        (
            "mode".into(),
            Value::String(if streaming {
                "live_dirty_region_updates".into()
            } else {
                "final_reconstructed_frame".into()
            }),
        ),
        (
            "scan_width".into(),
            Value::PixelCount(PixelCount::new(scan_width)),
        ),
        (
            "scan_height".into(),
            Value::PixelCount(PixelCount::new(scan_height)),
        ),
        (
            "reconstruction_width".into(),
            Value::PixelCount(PixelCount::new(reconstruction_width)),
        ),
        (
            "reconstruction_height".into(),
            Value::PixelCount(PixelCount::new(reconstruction_height)),
        ),
        ("pixel_format".into(), Value::String(pixel_format.into())),
        (
            "required_metadata".into(),
            string_list(&[
                "frame_handle".into(),
                "stream".into(),
                "scan_width".into(),
                "scan_height".into(),
                "reconstruction_width".into(),
                "reconstruction_height".into(),
                "reconstruction_pixel_size".into(),
                "sample_rate".into(),
                "line_dwell".into(),
                "detectors".into(),
                "saturated_pixels".into(),
                "progress_status".into(),
            ]),
        ),
        (
            "publication_evidence_status".into(),
            Value::String("pending_hardware_validation".into()),
        ),
    ]))
}

fn signal_publication_plan(
    channels: &[String],
    samples_per_line: u64,
    lines: u64,
    chunk_size: Option<u64>,
) -> Value {
    Value::Map(BTreeMap::from([
        ("event_kind".into(), Value::String("ScanSignalChunk".into())),
        ("mode".into(), Value::String("raw_signal_chunks".into())),
        ("channel_names".into(), string_list(channels)),
        ("samples_per_line".into(), i64_value_u64(samples_per_line)),
        ("lines".into(), i64_value_u64(lines)),
        (
            "chunk_size".into(),
            chunk_size.map(i64_value_u64).unwrap_or(Value::Null),
        ),
        (
            "required_metadata".into(),
            string_list(&[
                "stream".into(),
                "channel_names".into(),
                "timing_origin".into(),
                "line_index".into(),
                "chunk_index".into(),
                "first_sample_index".into(),
                "sample_count".into(),
                "sample_values".into(),
                "sample_rate".into(),
                "sample_period".into(),
                "dropped_samples".into(),
                "dropped_chunks".into(),
                "overflowed".into(),
            ]),
        ),
        (
            "publication_evidence_status".into(),
            Value::String("pending_hardware_validation".into()),
        ),
    ]))
}

fn task_plan(
    name: &str,
    role: &str,
    physical_channels: Vec<String>,
    daqmx_calls: Vec<&str>,
    samples_per_channel: u64,
    sample_rate_hz: f64,
    daqmx_timeout: TimeInterval,
    waveform_plan: Option<Value>,
) -> Value {
    let channel_count = physical_channels.len() as u64;
    let mut plan = BTreeMap::from([
        ("name".into(), Value::String(name.into())),
        ("role".into(), Value::String(role.into())),
        ("physical_channels".into(), string_list(&physical_channels)),
        (
            "buffer_plan".into(),
            task_buffer_plan(role, channel_count, samples_per_channel, daqmx_timeout),
        ),
        (
            "daqmx_calls".into(),
            Value::List(
                daqmx_calls
                    .into_iter()
                    .map(|call| Value::String(call.into()))
                    .collect(),
            ),
        ),
        (
            "samples_per_channel".into(),
            i64_value_u64(samples_per_channel),
        ),
        (
            "sample_rate".into(),
            Value::Frequency(Frequency::from_hertz(sample_rate_hz)),
        ),
    ]);
    if let Some(waveform_plan) = waveform_plan {
        plan.insert("waveform_plan".into(), waveform_plan);
    }
    Value::Map(plan)
}

fn task_plan_validation(
    task_count: usize,
    unrecognized_channels: &[String],
    invalid_role_channels: &[String],
    status: &str,
) -> Value {
    Value::Map(BTreeMap::from([
        ("status".into(), Value::String(status.into())),
        (
            "helper_command_runnable".into(),
            Value::Bool(
                task_count > 0
                    && unrecognized_channels.is_empty()
                    && invalid_role_channels.is_empty(),
            ),
        ),
        (
            "recognized_task_count".into(),
            Value::I64(task_count.min(i64::MAX as usize) as i64),
        ),
        (
            "unrecognized_channels".into(),
            string_list(unrecognized_channels),
        ),
        (
            "unrecognized_channel_count".into(),
            Value::I64(unrecognized_channels.len().min(i64::MAX as usize) as i64),
        ),
        (
            "invalid_role_channels".into(),
            string_list(invalid_role_channels),
        ),
        (
            "invalid_role_channel_count".into(),
            Value::I64(invalid_role_channels.len().min(i64::MAX as usize) as i64),
        ),
    ]))
}

fn cleanup_plan(
    start_order: &[String],
    clear_order: &[String],
    daqmx_timeout: TimeInterval,
) -> Value {
    let stop_order = reversed_string_list(start_order);
    Value::Map(BTreeMap::from([
        (
            "policy".into(),
            Value::String("stop_started_tasks_then_clear_all_created_tasks".into()),
        ),
        ("stop_order".into(), string_list(&stop_order)),
        ("clear_order".into(), string_list(clear_order)),
        ("wait_timeout".into(), Value::TimeInterval(daqmx_timeout)),
        ("stop_timeout".into(), Value::TimeInterval(daqmx_timeout)),
        (
            "failure_cleanup".into(),
            Value::String("clear_all_created_tasks_on_partial_setup_failure".into()),
        ),
        (
            "failure_cleanup_modes".into(),
            string_list(&[
                "partial_setup_failure".into(),
                "post_start_failure".into(),
                "buffered_write_failure".into(),
                "finite_read_failure".into(),
                "counter_output_wait_timeout".into(),
            ]),
        ),
        (
            "started_task_cleanup".into(),
            Value::String("stop_started_tasks_before_clear".into()),
        ),
        (
            "output_safe_state_after_failure".into(),
            Value::String("pending_hardware_validation".into()),
        ),
        (
            "safe_output_state".into(),
            Value::String("pending_hardware_validation".into()),
        ),
        (
            "cleanup_evidence_status".into(),
            Value::String("pending_hardware_validation".into()),
        ),
    ]))
}

fn cancel_plan(
    start_order: &[String],
    clear_order: &[String],
    daqmx_timeout: TimeInterval,
) -> Value {
    let stop_order = reversed_string_list(start_order);
    Value::Map(BTreeMap::from([
        (
            "strategy".into(),
            Value::String("request_stop_then_clear_created_tasks".into()),
        ),
        ("stop_order".into(), string_list(&stop_order)),
        ("clear_order".into(), string_list(clear_order)),
        ("stop_timeout".into(), Value::TimeInterval(daqmx_timeout)),
        (
            "safe_output_state".into(),
            Value::String("pending_hardware_validation".into()),
        ),
        (
            "cancel_evidence_status".into(),
            Value::String("pending_hardware_validation".into()),
        ),
    ]))
}

fn routing_plan(
    clock_producer_task: Option<&str>,
    buffered_tasks: &[String],
    sample_clock_source: Option<&str>,
    start_trigger_source: Option<&str>,
) -> Value {
    Value::Map(BTreeMap::from([
        (
            "sample_clock".into(),
            Value::Map(BTreeMap::from([
                (
                    "source".into(),
                    sample_clock_source
                        .map(|source| Value::String(source.into()))
                        .unwrap_or(Value::Null),
                ),
                (
                    "producer_task".into(),
                    clock_producer_task
                        .map(|task| Value::String(task.into()))
                        .unwrap_or(Value::Null),
                ),
                ("consumers".into(), string_list(buffered_tasks)),
                ("edge".into(), Value::String("Rising".into())),
                (
                    "evidence_status".into(),
                    Value::String("pending_hardware_validation".into()),
                ),
            ])),
        ),
        (
            "start_trigger".into(),
            Value::Map(BTreeMap::from([
                (
                    "source".into(),
                    start_trigger_source
                        .map(|source| Value::String(source.into()))
                        .unwrap_or(Value::Null),
                ),
                ("consumers".into(), string_list(buffered_tasks)),
                ("edge".into(), Value::String("Rising".into())),
                (
                    "evidence_status".into(),
                    Value::String("pending_hardware_validation".into()),
                ),
            ])),
        ),
        (
            "routing_evidence_status".into(),
            Value::String("pending_hardware_validation".into()),
        ),
    ]))
}

fn raster_buffer_plan(width: u32, height: u32, frames: u64, planned_samples: u64) -> Value {
    Value::Map(BTreeMap::from([
        (
            "pattern".into(),
            Value::String("raster_x_fast_y_slow".into()),
        ),
        (
            "pixel_order".into(),
            Value::String("row_major_unidirectional".into()),
        ),
        ("width".into(), Value::PixelCount(PixelCount::new(width))),
        ("height".into(), Value::PixelCount(PixelCount::new(height))),
        ("frames".into(), i64_value_u64(frames)),
        (
            "samples_per_frame".into(),
            i64_value_u64(width as u64 * height as u64),
        ),
        ("planned_samples".into(), i64_value_u64(planned_samples)),
        ("ao_channels_per_sample".into(), Value::I64(2)),
        ("laser_gate_channels_per_sample".into(), Value::I64(1)),
        (
            "sample_to_pixel_mapping".into(),
            Value::String("one_detector_sample_per_pixel".into()),
        ),
        (
            "buffer_evidence_status".into(),
            Value::String("pending_hardware_validation".into()),
        ),
    ]))
}

fn signal_buffer_plan(
    samples_per_line: u64,
    lines: u64,
    chunk_size: Option<u64>,
    planned_samples: u64,
) -> Value {
    Value::Map(BTreeMap::from([
        ("samples_per_line".into(), i64_value_u64(samples_per_line)),
        ("lines".into(), i64_value_u64(lines)),
        ("planned_samples".into(), i64_value_u64(planned_samples)),
        (
            "chunk_size".into(),
            chunk_size.map(i64_value_u64).unwrap_or(Value::Null),
        ),
        (
            "sample_order".into(),
            Value::String("line_major_contiguous".into()),
        ),
        (
            "buffer_evidence_status".into(),
            Value::String("pending_hardware_validation".into()),
        ),
    ]))
}

fn ao_raster_waveform_plan(
    width: u32,
    height: u32,
    frames: u64,
    analog_min: Voltage,
    analog_max: Voltage,
) -> Value {
    Value::Map(BTreeMap::from([
        (
            "pattern".into(),
            Value::String("x_fast_sawtooth_y_slow_step".into()),
        ),
        (
            "sample_order".into(),
            Value::String("row_major_unidirectional".into()),
        ),
        ("x_axis_channel_index".into(), Value::I64(0)),
        ("y_axis_channel_index".into(), Value::I64(1)),
        ("width".into(), Value::PixelCount(PixelCount::new(width))),
        ("height".into(), Value::PixelCount(PixelCount::new(height))),
        ("frames".into(), i64_value_u64(frames)),
        ("voltage_min".into(), Value::Voltage(analog_min)),
        ("voltage_max".into(), Value::Voltage(analog_max)),
        (
            "output_state_after_stop".into(),
            Value::String("pending_hardware_validation".into()),
        ),
        (
            "waveform_evidence_status".into(),
            Value::String("pending_hardware_validation".into()),
        ),
    ]))
}

fn do_laser_gate_waveform_plan(width: u32, height: u32, frames: u64) -> Value {
    Value::Map(BTreeMap::from([
        (
            "pattern".into(),
            Value::String("high_during_active_pixels".into()),
        ),
        (
            "sample_order".into(),
            Value::String("row_major_unidirectional".into()),
        ),
        ("line_indexing".into(), Value::String("zero_based".into())),
        ("width".into(), Value::PixelCount(PixelCount::new(width))),
        ("height".into(), Value::PixelCount(PixelCount::new(height))),
        ("frames".into(), i64_value_u64(frames)),
        (
            "idle_state_after_stop".into(),
            Value::String("pending_hardware_validation".into()),
        ),
        (
            "waveform_evidence_status".into(),
            Value::String("pending_hardware_validation".into()),
        ),
    ]))
}

fn task_buffer_plan(
    role: &str,
    channel_count: u64,
    samples_per_channel: u64,
    daqmx_timeout: TimeInterval,
) -> Value {
    let total_elements = channel_count.saturating_mul(samples_per_channel);
    let mut plan = BTreeMap::from([
        ("channel_count".into(), i64_value_u64(channel_count)),
        (
            "samples_per_channel".into(),
            i64_value_u64(samples_per_channel),
        ),
        ("total_elements".into(), i64_value_u64(total_elements)),
        (
            "buffer_evidence_status".into(),
            Value::String("pending_hardware_validation".into()),
        ),
    ]);

    match role {
        "analog_output" => {
            plan.insert("direction".into(), Value::String("write".into()));
            plan.insert("element_type".into(), Value::String("f64_volts".into()));
            plan.insert(
                "daqmx_transfer_api".into(),
                Value::String("DAQmxWriteAnalogF64".into()),
            );
            plan.insert(
                "candidate_layout".into(),
                Value::String("GroupByScanNumber".into()),
            );
            plan.insert("auto_start".into(), Value::Bool(false));
            plan.insert("timeout".into(), Value::TimeInterval(daqmx_timeout));
        }
        "digital_output" => {
            plan.insert("direction".into(), Value::String("write".into()));
            plan.insert("element_type".into(), Value::String("u8_line_state".into()));
            plan.insert(
                "daqmx_transfer_api".into(),
                Value::String("DAQmxWriteDigitalLines".into()),
            );
            plan.insert(
                "candidate_layout".into(),
                Value::String("GroupByScanNumber".into()),
            );
            plan.insert(
                "line_grouping".into(),
                Value::String("ChanForAllLines".into()),
            );
            plan.insert("auto_start".into(), Value::Bool(false));
            plan.insert("timeout".into(), Value::TimeInterval(daqmx_timeout));
        }
        "analog_input" => {
            plan.insert("direction".into(), Value::String("read".into()));
            plan.insert("element_type".into(), Value::String("f64_volts".into()));
            plan.insert(
                "daqmx_transfer_api".into(),
                Value::String("DAQmxReadAnalogF64".into()),
            );
            plan.insert(
                "candidate_layout".into(),
                Value::String("GroupByScanNumber".into()),
            );
            plan.insert("timeout".into(), Value::TimeInterval(daqmx_timeout));
        }
        "counter_input" => {
            plan.insert("direction".into(), Value::String("read".into()));
            plan.insert("element_type".into(), Value::String("u32_counts".into()));
            plan.insert(
                "daqmx_transfer_api".into(),
                Value::String("DAQmxReadCounterU32".into()),
            );
            plan.insert("timeout".into(), Value::TimeInterval(daqmx_timeout));
        }
        "counter_output" => {
            plan.insert("direction".into(), Value::String("generate".into()));
            plan.insert(
                "element_type".into(),
                Value::String("counter_pulse_train".into()),
            );
            plan.insert(
                "daqmx_transfer_api".into(),
                Value::String("DAQmxCreateCOPulseChanFreq".into()),
            );
            plan.insert("timing".into(), Value::String("implicit_finite".into()));
        }
        _ => {
            plan.insert("direction".into(), Value::String("unknown".into()));
            plan.insert("element_type".into(), Value::String("unknown".into()));
        }
    }

    Value::Map(plan)
}

fn i64_value_u64(value: u64) -> Value {
    Value::I64(value.min(i64::MAX as u64) as i64)
}

fn plan_setup_helper_command(
    tasks: &[Value],
    sample_rate_hz: f64,
    samples_per_channel: u64,
    sample_clock_source: Option<&str>,
    start_trigger: Option<&str>,
    analog_min: Voltage,
    analog_max: Voltage,
    daqmx_timeout: TimeInterval,
    raster_shape: Option<(u64, u64, u64)>,
    signal_shape: Option<(u64, Option<u64>)>,
) -> String {
    let mut command = format!(
        "target/debug/numanager-daqmx-plan-setup-helper --sample-rate {:.0} --samples {}",
        sample_rate_hz,
        samples_per_channel.max(1).min(i64::MAX as u64)
    );
    if let Some((width, height, frames)) = raster_shape {
        command.push_str(&format!(
            " --width {} --height {} --frames {}",
            width.max(1).min(i64::MAX as u64),
            height.max(1).min(i64::MAX as u64),
            frames.max(1).min(i64::MAX as u64)
        ));
    }
    if let Some((lines, chunk_size)) = signal_shape {
        command.push_str(&format!(
            " --signal-lines {}",
            lines.max(1).min(i64::MAX as u64)
        ));
        if let Some(chunk_size) = chunk_size {
            command.push_str(&format!(
                " --chunk-size {}",
                chunk_size.max(1).min(i64::MAX as u64)
            ));
        }
    }
    for task in tasks {
        let Value::Map(task) = task else {
            continue;
        };
        let Some(kind) = helper_kind(task) else {
            continue;
        };
        let Some(Value::String(name)) = task.get("name") else {
            continue;
        };
        command.push_str(&format!(" --{kind}-task {}", shell_arg(name)));
    }
    let mut has_ao = false;
    for task in tasks {
        let Value::Map(task) = task else {
            continue;
        };
        let Some(kind) = helper_kind(task) else {
            continue;
        };
        has_ao |= kind == "ao";
        let Some(Value::List(channels)) = task.get("physical_channels") else {
            continue;
        };
        for channel in channels {
            if let Value::String(channel) = channel {
                command.push_str(&format!(" --{kind} {}", shell_arg(channel)));
            }
        }
    }
    if has_ao {
        command.push_str(&format!(
            " --min-volts {:.6} --max-volts {:.6}",
            analog_min.volts(),
            analog_max.volts()
        ));
    }
    if let Some(source) = sample_clock_source {
        command.push_str(&format!(" --sample-clock-source {}", shell_arg(source)));
    }
    if let Some(trigger) = start_trigger {
        command.push_str(&format!(" --start-trigger {}", shell_arg(trigger)));
    }
    command.push_str(&format!(" --timeout {:.6}", daqmx_timeout.seconds()));
    command
}

fn preflight_helper_command(setup_command: &str) -> String {
    format!("{setup_command} --preflight-only")
}

fn helper_command_value(command: &str, runnable: bool) -> Value {
    if runnable {
        Value::String(command.into())
    } else {
        Value::Null
    }
}

fn helper_kind(task: &BTreeMap<String, Value>) -> Option<&'static str> {
    match task.get("role")? {
        Value::String(role) if role == "analog_output" => Some("ao"),
        Value::String(role) if role == "digital_output" => Some("do"),
        Value::String(role) if role == "analog_input" => Some("ai"),
        Value::String(role) if role == "counter_input" => Some("ci"),
        Value::String(role) if role == "counter_output" => Some("co"),
        _ => None,
    }
}

fn shell_arg(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '_' | '-' | '.' | ':' | '+'))
    {
        return value.into();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn role_channels_value(config: &ImSwitchDaqmxConfig, roles: &[(&str, &String)]) -> Value {
    Value::Map(
        roles
            .iter()
            .map(|(role, channel)| {
                (
                    (*role).into(),
                    Value::Map(BTreeMap::from([
                        ("logical".into(), Value::String((*channel).clone())),
                        (
                            "physical".into(),
                            Value::String(physical_channel(config, channel)),
                        ),
                    ])),
                )
            })
            .collect(),
    )
}

fn lsm_role_channels_value(config: &ImSwitchDaqmxConfig) -> Value {
    role_channels_value(
        config,
        &[
            ("x_galvo", &config.lsm_x_galvo),
            ("y_galvo", &config.lsm_y_galvo),
            ("laser_gate", &config.lsm_laser_gate),
            ("detector", &config.lsm_detector),
            ("sample_clock", &config.lsm_sample_clock),
        ],
    )
}

fn lsm_routing_value(config: &ImSwitchDaqmxConfig) -> Value {
    Value::Map(BTreeMap::from([
        (
            "sample_clock_source".into(),
            config
                .lsm_sample_clock_source
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        ),
        (
            "start_trigger_source".into(),
            config
                .lsm_start_trigger_source
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        ),
        (
            "evidence_status".into(),
            Value::String("pending_hardware_validation".into()),
        ),
    ]))
}

fn physical_channel(config: &ImSwitchDaqmxConfig, channel: &str) -> String {
    if channel.contains('/') {
        return channel.into();
    }
    if let Some(index) = channel.strip_prefix("ao").and_then(parse_usize) {
        return format!("{}/ao{index}", config.device_name);
    }
    if let Some(index) = channel.strip_prefix("ai").and_then(parse_usize) {
        return format!("{}/ai{index}", config.device_name);
    }
    if let Some(index) = channel.strip_prefix("do").and_then(parse_usize) {
        return format!("{}/port0/line{index}", config.device_name);
    }
    if let Some(index) = channel.strip_prefix("line").and_then(parse_usize) {
        return format!("{}/port0/line{index}", config.device_name);
    }
    if let Some(index) = channel.strip_prefix("counter").and_then(parse_usize) {
        return format!("{}/ctr{index}", config.device_name);
    }
    if let Some(index) = channel.strip_prefix("ci").and_then(parse_usize) {
        return format!("{}/ctr{index}", config.device_name);
    }
    if let Some(index) = channel.strip_prefix("co").and_then(parse_usize) {
        return format!("{}/ctr{index}", config.device_name);
    }
    channel.into()
}

fn counter_internal_output_source(channel: &str) -> Option<String> {
    let (device, counter) = channel.rsplit_once('/')?;
    let index = counter
        .strip_prefix("ctr")
        .or_else(|| counter.strip_prefix("Ctr"))?
        .parse::<u32>()
        .ok()?;
    let device = device.trim_start_matches('/');
    (!device.is_empty()).then(|| format!("/{device}/Ctr{index}InternalOutput"))
}

fn invalid_raster_role_channels(
    x_galvo: &str,
    y_galvo: &str,
    laser_gate: &str,
    detector: &str,
    sample_clock: &str,
) -> Vec<String> {
    let mut invalid = Vec::new();
    if !is_ao_channel(x_galvo) {
        invalid.push(format!("x_galvo:{x_galvo}"));
    }
    if !is_ao_channel(y_galvo) {
        invalid.push(format!("y_galvo:{y_galvo}"));
    }
    if !is_do_line(laser_gate) {
        invalid.push(format!("laser_gate:{laser_gate}"));
    }
    if !is_ai_channel(detector) && !is_counter_channel(detector) {
        invalid.push(format!("detector:{detector}"));
    }
    if !is_counter_channel(sample_clock) {
        invalid.push(format!("sample_clock:{sample_clock}"));
    }
    invalid
}

fn is_ao_channel(channel: &str) -> bool {
    channel.strip_prefix("ao").and_then(parse_usize).is_some()
        || channel
            .rsplit_once('/')
            .is_some_and(|(_, name)| name.strip_prefix("ao").and_then(parse_usize).is_some())
}

fn is_ai_channel(channel: &str) -> bool {
    channel.strip_prefix("ai").and_then(parse_usize).is_some()
        || channel
            .rsplit_once('/')
            .is_some_and(|(_, name)| name.strip_prefix("ai").and_then(parse_usize).is_some())
}

fn is_do_line(channel: &str) -> bool {
    channel.strip_prefix("do").and_then(parse_usize).is_some()
        || channel.strip_prefix("line").and_then(parse_usize).is_some()
        || channel
            .rsplit_once('/')
            .is_some_and(|(_, name)| name.strip_prefix("line").and_then(parse_usize).is_some())
}

fn is_counter_channel(channel: &str) -> bool {
    channel
        .strip_prefix("counter")
        .and_then(parse_usize)
        .is_some()
        || channel.strip_prefix("ci").and_then(parse_usize).is_some()
        || channel.strip_prefix("co").and_then(parse_usize).is_some()
        || channel
            .rsplit_once('/')
            .is_some_and(|(_, name)| name.strip_prefix("ctr").and_then(parse_usize).is_some())
}

fn parse_usize(value: &str) -> Option<usize> {
    value.parse().ok()
}

fn map_pixel_count(map: &BTreeMap<String, Value>, key: &str) -> Option<u32> {
    match map.get(key) {
        Some(Value::PixelCount(value)) => Some(value.pixels()),
        Some(Value::I64(value)) if *value > 0 => Some((*value).min(u32::MAX as i64) as u32),
        _ => None,
    }
}

fn map_i64(map: &BTreeMap<String, Value>, key: &str) -> Option<i64> {
    match map.get(key) {
        Some(Value::I64(value)) => Some(*value),
        _ => None,
    }
}

fn map_frequency_hz(map: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
    match map.get(key) {
        Some(Value::Frequency(value)) => Some(value.hertz()),
        Some(Value::F64(value)) if *value > 0.0 => Some(*value),
        _ => None,
    }
}

fn map_string(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    match map.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

#[derive(Debug, Clone)]
struct DaqmxRuntimeProbe {
    version: String,
    version_major: Option<u32>,
    version_minor: Option<u32>,
    version_update: Option<u32>,
    device_names: Vec<String>,
    device_inventory_error: Option<String>,
    configured_device: Option<DaqmxDeviceProbe>,
    configured_device_error: Option<String>,
}

#[derive(Debug, Clone)]
struct DaqmxDeviceProbe {
    name: String,
    product_type: Option<String>,
    serial_number: Option<u32>,
    analog_inputs: Vec<String>,
    analog_outputs: Vec<String>,
    digital_inputs: Vec<String>,
    digital_outputs: Vec<String>,
    counter_inputs: Vec<String>,
    counter_outputs: Vec<String>,
}

fn probe_daqmx_runtime(config: &ImSwitchDaqmxConfig) -> Result<DaqmxRuntimeProbe> {
    if !ni_daqmx_sdk_feature_enabled() {
        return Err(Error::new(
            ErrorCode::Unsupported,
            backend_unavailable_message(config),
        ));
    }
    reject_linux_runtime_inventory(config)?;
    live_backend::probe_runtime(
        &config.device_name,
        config.inventory_devices,
        config.inventory_helper_path.as_deref(),
        config.inventory_helper_timeout,
    )
    .map_err(|message| {
        Error::new(
            ErrorCode::Unsupported,
            format!("NI-DAQmx runtime probe failed: {message}"),
        )
    })
}

#[cfg(target_os = "linux")]
fn reject_linux_runtime_inventory(config: &ImSwitchDaqmxConfig) -> Result<()> {
    if config.inventory_devices && config.inventory_helper_path.is_none() {
        Err(Error::new(
            ErrorCode::Unsupported,
            "NI-DAQmx device inventory is disabled from the Linux runtime driver unless inventory_helper_path points to a process-isolated numanager-daqmx-inventory-helper binary",
        ))
    } else {
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
fn reject_linux_runtime_inventory(_: &ImSwitchDaqmxConfig) -> Result<()> {
    Ok(())
}

#[cfg(all(
    feature = "ni-daqmx-sdk",
    any(target_os = "linux", target_os = "windows")
))]
mod live_backend {
    use super::{DaqmxDeviceProbe, DaqmxRuntimeProbe};
    use numanager_core::TimeInterval;
    use std::ffi::CStr;
    #[cfg(not(target_os = "linux"))]
    use std::ffi::CString;
    use std::os::raw::c_char;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    pub(super) fn probe_runtime(
        configured_device_name: &str,
        inventory_devices: bool,
        inventory_helper_path: Option<&str>,
        inventory_helper_timeout: TimeInterval,
    ) -> std::result::Result<DaqmxRuntimeProbe, String> {
        #[cfg(target_os = "linux")]
        if inventory_devices {
            let Some(path) = inventory_helper_path else {
                return Err(
                    "DAQmx device inventory is disabled from the Linux runtime driver unless inventory_helper_path points to a process-isolated numanager-daqmx-inventory-helper binary"
                        .into(),
                );
            };
            return query_runtime_with_inventory_helper(
                path,
                configured_device_name,
                inventory_helper_timeout,
            );
        }
        #[cfg(target_os = "linux")]
        if let Some(path) = inventory_helper_path {
            return query_runtime_version_with_helper(
                path,
                configured_device_name,
                inventory_helper_timeout,
            );
        }

        let major = get_version_component(ni_daqmx_sys::DAQmxGetSysNIDAQMajorVersion)?;
        let minor = get_version_component(ni_daqmx_sys::DAQmxGetSysNIDAQMinorVersion)?;
        let update = get_version_component(ni_daqmx_sys::DAQmxGetSysNIDAQUpdateVersion)?;
        let (device_names, device_inventory_error, configured_device, configured_device_error) =
            if inventory_devices {
                match query_inventory(
                    configured_device_name,
                    inventory_helper_path,
                    inventory_helper_timeout,
                ) {
                    Ok(inventory) => (
                        inventory.device_names,
                        inventory.device_inventory_error,
                        inventory.configured_device,
                        inventory.configured_device_error,
                    ),
                    Err(error) => (Vec::new(), Some(error), None, None),
                }
            } else {
                (Vec::new(), None, None, None)
            };
        Ok(DaqmxRuntimeProbe {
            version: format!("{major}.{minor}.{update}"),
            version_major: Some(major),
            version_minor: Some(minor),
            version_update: Some(update),
            device_names,
            device_inventory_error,
            configured_device,
            configured_device_error,
        })
    }

    #[allow(dead_code)]
    struct DaqmxInventory {
        runtime_version: Option<String>,
        runtime_version_major: Option<u32>,
        runtime_version_minor: Option<u32>,
        runtime_version_update: Option<u32>,
        device_names: Vec<String>,
        device_inventory_error: Option<String>,
        configured_device: Option<DaqmxDeviceProbe>,
        configured_device_error: Option<String>,
    }

    fn query_inventory(
        configured_device_name: &str,
        inventory_helper_path: Option<&str>,
        inventory_helper_timeout: TimeInterval,
    ) -> std::result::Result<DaqmxInventory, String> {
        #[cfg(target_os = "linux")]
        {
            let Some(path) = inventory_helper_path else {
                return Err(
                    "DAQmx device inventory is disabled from the Linux runtime driver unless inventory_helper_path points to a process-isolated numanager-daqmx-inventory-helper binary"
                        .into(),
                );
            };
            query_inventory_with_helper(path, configured_device_name, inventory_helper_timeout)
        }
        #[cfg(not(target_os = "linux"))]
        {
            if let Some(path) = inventory_helper_path {
                return query_inventory_with_helper(
                    path,
                    configured_device_name,
                    inventory_helper_timeout,
                );
            }
            query_inventory_in_process(configured_device_name)
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn query_inventory_in_process(
        configured_device_name: &str,
    ) -> std::result::Result<DaqmxInventory, String> {
        let device_names = query_system_device_names()?;
        let (configured_device, configured_device_error) = if device_names
            .iter()
            .any(|device| device == configured_device_name)
        {
            match probe_device(configured_device_name) {
                Ok(device) => (Some(device), None),
                Err(error) => (None, Some(error)),
            }
        } else {
            (None, None)
        };
        Ok(DaqmxInventory {
            runtime_version: None,
            runtime_version_major: None,
            runtime_version_minor: None,
            runtime_version_update: None,
            device_names,
            device_inventory_error: None,
            configured_device,
            configured_device_error,
        })
    }

    fn query_inventory_with_helper(
        path: &str,
        configured_device_name: &str,
        inventory_helper_timeout: TimeInterval,
    ) -> std::result::Result<DaqmxInventory, String> {
        let output = run_inventory_helper(
            path,
            configured_device_name,
            HelperMode::Inventory,
            inventory_helper_timeout,
        )?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "DAQmx inventory helper exited with {}; stderr={}",
                output.status,
                stderr.trim()
            ));
        }
        parse_inventory_output(&String::from_utf8_lossy(&output.stdout))
    }

    #[cfg(target_os = "linux")]
    fn query_runtime_with_inventory_helper(
        path: &str,
        configured_device_name: &str,
        inventory_helper_timeout: TimeInterval,
    ) -> std::result::Result<DaqmxRuntimeProbe, String> {
        let output = run_inventory_helper(
            path,
            configured_device_name,
            HelperMode::InventoryWithVersion,
            inventory_helper_timeout,
        )?;
        let inventory = parse_inventory_output(&String::from_utf8_lossy(&output.stdout))?;
        let helper_error = if output.status.success() {
            inventory.device_inventory_error.clone()
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Some(format!(
                "DAQmx inventory helper exited with {}; stderr={}",
                output.status,
                stderr.trim()
            ))
        };
        let version = inventory
            .runtime_version
            .unwrap_or_else(|| "unknown".into());
        Ok(DaqmxRuntimeProbe {
            version,
            version_major: inventory.runtime_version_major,
            version_minor: inventory.runtime_version_minor,
            version_update: inventory.runtime_version_update,
            device_names: inventory.device_names,
            device_inventory_error: helper_error,
            configured_device: inventory.configured_device,
            configured_device_error: inventory.configured_device_error,
        })
    }

    #[cfg(target_os = "linux")]
    fn query_runtime_version_with_helper(
        path: &str,
        configured_device_name: &str,
        inventory_helper_timeout: TimeInterval,
    ) -> std::result::Result<DaqmxRuntimeProbe, String> {
        let output = run_inventory_helper(
            path,
            configured_device_name,
            HelperMode::VersionOnly,
            inventory_helper_timeout,
        )?;
        let inventory = parse_inventory_output(&String::from_utf8_lossy(&output.stdout))?;
        let helper_error = if output.status.success() {
            None
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Some(format!(
                "DAQmx runtime-version helper exited with {}; stderr={}",
                output.status,
                stderr.trim()
            ))
        };
        let version = inventory
            .runtime_version
            .unwrap_or_else(|| "unknown".into());
        Ok(DaqmxRuntimeProbe {
            version,
            version_major: inventory.runtime_version_major,
            version_minor: inventory.runtime_version_minor,
            version_update: inventory.runtime_version_update,
            device_names: Vec::new(),
            device_inventory_error: helper_error,
            configured_device: None,
            configured_device_error: None,
        })
    }

    #[allow(dead_code)]
    enum HelperMode {
        Inventory,
        InventoryWithVersion,
        VersionOnly,
    }

    fn run_inventory_helper(
        path: &str,
        configured_device_name: &str,
        mode: HelperMode,
        inventory_helper_timeout: TimeInterval,
    ) -> std::result::Result<std::process::Output, String> {
        let mut command = Command::new(path);
        command
            .arg("--device")
            .arg(configured_device_name)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        match mode {
            HelperMode::Inventory => {}
            HelperMode::InventoryWithVersion => {
                command.arg("--include-version");
            }
            HelperMode::VersionOnly => {
                command.arg("--version-only");
            }
        }
        #[cfg(unix)]
        detach_helper_session(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| format!("failed to spawn DAQmx inventory helper {path:?}: {error}"))?;

        let timeout = Duration::from_secs_f64(inventory_helper_timeout.seconds());
        let deadline = Instant::now() + timeout;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(25));
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "DAQmx inventory helper timed out after {:.3} s",
                        timeout.as_secs_f64()
                    ));
                }
                Err(error) => {
                    let _ = child.kill();
                    return Err(format!(
                        "failed while waiting for DAQmx inventory helper: {error}"
                    ));
                }
            }
        }

        let output = child
            .wait_with_output()
            .map_err(|error| format!("failed to collect DAQmx inventory helper output: {error}"))?;
        Ok(output)
    }

    #[cfg(unix)]
    fn detach_helper_session(command: &mut Command) {
        use std::io;
        use std::os::unix::process::CommandExt;

        unsafe {
            command.pre_exec(|| {
                if setsid() < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
    }

    #[cfg(unix)]
    unsafe extern "C" {
        fn setsid() -> i32;
    }

    fn parse_inventory_output(output: &str) -> std::result::Result<DaqmxInventory, String> {
        let mut device_names = Vec::new();
        let mut runtime_version = None;
        let mut runtime_version_major = None;
        let mut runtime_version_minor = None;
        let mut runtime_version_update = None;
        let mut configured_device = None;
        let mut configured_device_error = None;

        for line in output.lines() {
            let Some((key, value)) = line.split_once('\t') else {
                continue;
            };
            match key {
                "runtime_version" => runtime_version = Some(value.into()),
                "runtime_version_major" => runtime_version_major = value.parse::<u32>().ok(),
                "runtime_version_minor" => runtime_version_minor = value.parse::<u32>().ok(),
                "runtime_version_update" => runtime_version_update = value.parse::<u32>().ok(),
                "devices" => device_names = split_daqmx_list(value),
                "configured_device_error" => configured_device_error = Some(value.into()),
                "configured_device" => {
                    configured_device = Some(DaqmxDeviceProbe {
                        name: value.into(),
                        product_type: None,
                        serial_number: None,
                        analog_inputs: Vec::new(),
                        analog_outputs: Vec::new(),
                        digital_inputs: Vec::new(),
                        digital_outputs: Vec::new(),
                        counter_inputs: Vec::new(),
                        counter_outputs: Vec::new(),
                    });
                }
                "product_type" => {
                    if let Some(device) = configured_device.as_mut() {
                        device.product_type = Some(value.into());
                    }
                }
                "serial_number" => {
                    if let Some(device) = configured_device.as_mut() {
                        device.serial_number = value.parse::<u32>().ok();
                    }
                }
                "analog_inputs" => {
                    if let Some(device) = configured_device.as_mut() {
                        device.analog_inputs = split_daqmx_list(value);
                    }
                }
                "analog_outputs" => {
                    if let Some(device) = configured_device.as_mut() {
                        device.analog_outputs = split_daqmx_list(value);
                    }
                }
                "digital_inputs" => {
                    if let Some(device) = configured_device.as_mut() {
                        device.digital_inputs = split_daqmx_list(value);
                    }
                }
                "digital_outputs" => {
                    if let Some(device) = configured_device.as_mut() {
                        device.digital_outputs = split_daqmx_list(value);
                    }
                }
                "counter_inputs" => {
                    if let Some(device) = configured_device.as_mut() {
                        device.counter_inputs = split_daqmx_list(value);
                    }
                }
                "counter_outputs" => {
                    if let Some(device) = configured_device.as_mut() {
                        device.counter_outputs = split_daqmx_list(value);
                    }
                }
                _ => {}
            }
        }

        Ok(DaqmxInventory {
            runtime_version,
            runtime_version_major,
            runtime_version_minor,
            runtime_version_update,
            device_names,
            device_inventory_error: None,
            configured_device,
            configured_device_error,
        })
    }

    #[cfg(not(target_os = "linux"))]
    fn probe_device(device_name: &str) -> std::result::Result<DaqmxDeviceProbe, String> {
        let device = CString::new(device_name)
            .map_err(|_| format!("device name contains an interior NUL: {device_name:?}"))?;
        Ok(DaqmxDeviceProbe {
            name: device_name.into(),
            product_type: query_device_string(&device, ni_daqmx_sys::DAQmxGetDevProductType).ok(),
            serial_number: query_device_u32(&device, ni_daqmx_sys::DAQmxGetDevSerialNum).ok(),
            analog_inputs: query_device_string(&device, ni_daqmx_sys::DAQmxGetDevAIPhysicalChans)
                .map(|value| split_daqmx_list(&value))
                .unwrap_or_default(),
            analog_outputs: query_device_string(&device, ni_daqmx_sys::DAQmxGetDevAOPhysicalChans)
                .map(|value| split_daqmx_list(&value))
                .unwrap_or_default(),
            digital_inputs: query_device_string(&device, ni_daqmx_sys::DAQmxGetDevDILines)
                .map(|value| split_daqmx_list(&value))
                .unwrap_or_default(),
            digital_outputs: query_device_string(&device, ni_daqmx_sys::DAQmxGetDevDOLines)
                .map(|value| split_daqmx_list(&value))
                .unwrap_or_default(),
            counter_inputs: query_device_string(&device, ni_daqmx_sys::DAQmxGetDevCIPhysicalChans)
                .map(|value| split_daqmx_list(&value))
                .unwrap_or_default(),
            counter_outputs: query_device_string(&device, ni_daqmx_sys::DAQmxGetDevCOPhysicalChans)
                .map(|value| split_daqmx_list(&value))
                .unwrap_or_default(),
        })
    }

    fn get_version_component(
        getter: unsafe extern "C" fn(*mut ni_daqmx_sys::uInt32) -> ni_daqmx_sys::int32,
    ) -> std::result::Result<ni_daqmx_sys::uInt32, String> {
        let mut value = 0;
        let status = unsafe { getter(&mut value) };
        if status < 0 {
            return Err(error_string(status));
        }
        Ok(value)
    }

    #[cfg(not(target_os = "linux"))]
    fn query_string(
        getter: unsafe extern "C" fn(*mut c_char, ni_daqmx_sys::uInt32) -> ni_daqmx_sys::int32,
    ) -> std::result::Result<String, String> {
        let mut buffer = vec![0 as c_char; 16_384];
        let status = unsafe { getter(buffer.as_mut_ptr(), buffer.len() as ni_daqmx_sys::uInt32) };
        if status < 0 {
            return Err(error_string(status));
        }
        Ok(unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned())
    }

    #[cfg(not(target_os = "linux"))]
    fn query_system_device_names() -> std::result::Result<Vec<String>, String> {
        query_string(ni_daqmx_sys::DAQmxGetSysDevNames).map(|value| split_daqmx_list(&value))
    }

    #[cfg(not(target_os = "linux"))]
    fn query_device_string(
        device: &CStr,
        getter: unsafe extern "C" fn(
            *const c_char,
            *mut c_char,
            ni_daqmx_sys::uInt32,
        ) -> ni_daqmx_sys::int32,
    ) -> std::result::Result<String, String> {
        let mut buffer = vec![0 as c_char; 16_384];
        let status = unsafe {
            getter(
                device.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len() as ni_daqmx_sys::uInt32,
            )
        };
        if status < 0 {
            return Err(error_string(status));
        }
        Ok(unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned())
    }

    #[cfg(not(target_os = "linux"))]
    fn query_device_u32(
        device: &CStr,
        getter: unsafe extern "C" fn(
            *const c_char,
            *mut ni_daqmx_sys::uInt32,
        ) -> ni_daqmx_sys::int32,
    ) -> std::result::Result<ni_daqmx_sys::uInt32, String> {
        let mut value = 0;
        let status = unsafe { getter(device.as_ptr(), &mut value) };
        if status < 0 {
            return Err(error_string(status));
        }
        Ok(value)
    }

    fn split_daqmx_list(value: &str) -> Vec<String> {
        value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect()
    }

    fn error_string(status: ni_daqmx_sys::int32) -> String {
        let mut buffer = vec![0 as c_char; 2048];
        let result = unsafe {
            ni_daqmx_sys::DAQmxGetErrorString(
                status,
                buffer.as_mut_ptr(),
                buffer.len() as ni_daqmx_sys::uInt32,
            )
        };
        if result < 0 {
            return format!("DAQmx error {status}; DAQmxGetErrorString returned {result}");
        }
        unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }
}

#[cfg(all(
    feature = "ni-daqmx-sdk",
    any(target_os = "linux", target_os = "windows")
))]
#[allow(dead_code)]
mod daqmx_task {
    use std::ffi::{CStr, CString};
    use std::fmt;
    use std::os::raw::c_char;
    use std::ptr;

    #[derive(Debug, Clone)]
    pub(super) struct DaqmxError {
        pub(super) status: ni_daqmx_sys::int32,
        pub(super) message: String,
    }

    impl fmt::Display for DaqmxError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "NI-DAQmx error {}: {}", self.status, self.message)
        }
    }

    impl std::error::Error for DaqmxError {}

    pub(super) type DaqmxResult<T> = std::result::Result<T, DaqmxError>;

    #[derive(Debug, Clone, Copy)]
    pub(super) enum SampleMode {
        Finite,
        Continuous,
    }

    impl SampleMode {
        fn as_raw(self) -> ni_daqmx_sys::int32 {
            match self {
                Self::Finite => ni_daqmx_sys::DAQmx_Val_FiniteSamps,
                Self::Continuous => ni_daqmx_sys::DAQmx_Val_ContSamps,
            }
        }
    }

    #[derive(Debug, Clone, Copy)]
    pub(super) enum Edge {
        Rising,
        Falling,
    }

    impl Edge {
        fn as_raw(self) -> ni_daqmx_sys::int32 {
            match self {
                Self::Rising => ni_daqmx_sys::DAQmx_Val_Rising,
                Self::Falling => ni_daqmx_sys::DAQmx_Val_Falling,
            }
        }
    }

    #[derive(Debug, Clone, Copy)]
    pub(super) enum DataLayout {
        GroupByChannel,
        GroupByScanNumber,
    }

    impl DataLayout {
        fn as_raw(self) -> ni_daqmx_sys::bool32 {
            match self {
                Self::GroupByChannel => ni_daqmx_sys::DAQmx_Val_GroupByChannel as _,
                Self::GroupByScanNumber => ni_daqmx_sys::DAQmx_Val_GroupByScanNumber as _,
            }
        }
    }

    #[derive(Debug, Clone, Copy)]
    pub(super) enum LineGrouping {
        ChannelForAllLines,
    }

    impl LineGrouping {
        fn as_raw(self) -> ni_daqmx_sys::int32 {
            match self {
                Self::ChannelForAllLines => ni_daqmx_sys::DAQmx_Val_ChanForAllLines,
            }
        }
    }

    #[derive(Debug, Clone, Copy)]
    pub(super) enum CountDirection {
        Up,
        Down,
    }

    impl CountDirection {
        fn as_raw(self) -> ni_daqmx_sys::int32 {
            match self {
                Self::Up => ni_daqmx_sys::DAQmx_Val_CountUp,
                Self::Down => ni_daqmx_sys::DAQmx_Val_CountDown,
            }
        }
    }

    #[derive(Debug, Clone, Copy)]
    pub(super) enum IdleState {
        Low,
        High,
    }

    impl IdleState {
        fn as_raw(self) -> ni_daqmx_sys::int32 {
            match self {
                Self::Low => ni_daqmx_sys::DAQmx_Val_Low,
                Self::High => ni_daqmx_sys::DAQmx_Val_High,
            }
        }
    }

    #[derive(Debug, Clone, Copy)]
    pub(super) enum TerminalConfig {
        Default,
        Differential,
        Rse,
        Nrse,
    }

    impl TerminalConfig {
        fn as_raw(self) -> ni_daqmx_sys::int32 {
            match self {
                Self::Default => ni_daqmx_sys::DAQmx_Val_Cfg_Default,
                Self::Differential => ni_daqmx_sys::DAQmx_Val_Diff,
                Self::Rse => ni_daqmx_sys::DAQmx_Val_RSE,
                Self::Nrse => ni_daqmx_sys::DAQmx_Val_NRSE,
            }
        }
    }

    pub(super) struct DaqmxTask {
        handle: ni_daqmx_sys::TaskHandle,
        cleared: bool,
    }

    impl DaqmxTask {
        pub(super) fn create(name: Option<&str>) -> DaqmxResult<Self> {
            let name = optional_cstring(name)?;
            let mut handle = ptr::null_mut();
            let status =
                unsafe { ni_daqmx_sys::DAQmxCreateTask(cstr_ptr(name.as_ref()), &mut handle) };
            check_status(status)?;
            Ok(Self {
                handle,
                cleared: false,
            })
        }

        pub(super) fn start(&self) -> DaqmxResult<()> {
            check_status(unsafe { ni_daqmx_sys::DAQmxStartTask(self.handle) })
        }

        pub(super) fn stop(&self) -> DaqmxResult<()> {
            check_status(unsafe { ni_daqmx_sys::DAQmxStopTask(self.handle) })
        }

        pub(super) fn wait_until_done(&self, timeout_seconds: f64) -> DaqmxResult<()> {
            check_status(unsafe {
                ni_daqmx_sys::DAQmxWaitUntilTaskDone(self.handle, timeout_seconds)
            })
        }

        pub(super) fn clear(mut self) -> DaqmxResult<()> {
            self.clear_inner()
        }

        pub(super) fn create_ao_voltage_channel(
            &self,
            physical_channel: &str,
            name: Option<&str>,
            min_volts: f64,
            max_volts: f64,
        ) -> DaqmxResult<()> {
            let physical_channel = required_cstring("physical_channel", physical_channel)?;
            let name = optional_cstring(name)?;
            check_status(unsafe {
                ni_daqmx_sys::DAQmxCreateAOVoltageChan(
                    self.handle,
                    physical_channel.as_ptr(),
                    cstr_ptr(name.as_ref()),
                    min_volts,
                    max_volts,
                    ni_daqmx_sys::DAQmx_Val_Volts,
                    ptr::null(),
                )
            })
        }

        pub(super) fn create_do_lines(
            &self,
            lines: &str,
            name: Option<&str>,
            grouping: LineGrouping,
        ) -> DaqmxResult<()> {
            let lines = required_cstring("lines", lines)?;
            let name = optional_cstring(name)?;
            check_status(unsafe {
                ni_daqmx_sys::DAQmxCreateDOChan(
                    self.handle,
                    lines.as_ptr(),
                    cstr_ptr(name.as_ref()),
                    grouping.as_raw(),
                )
            })
        }

        pub(super) fn create_ai_voltage_channel(
            &self,
            physical_channel: &str,
            name: Option<&str>,
            terminal_config: TerminalConfig,
            min_volts: f64,
            max_volts: f64,
        ) -> DaqmxResult<()> {
            let physical_channel = required_cstring("physical_channel", physical_channel)?;
            let name = optional_cstring(name)?;
            check_status(unsafe {
                ni_daqmx_sys::DAQmxCreateAIVoltageChan(
                    self.handle,
                    physical_channel.as_ptr(),
                    cstr_ptr(name.as_ref()),
                    terminal_config.as_raw(),
                    min_volts,
                    max_volts,
                    ni_daqmx_sys::DAQmx_Val_Volts,
                    ptr::null(),
                )
            })
        }

        pub(super) fn create_ci_count_edges_channel(
            &self,
            counter: &str,
            name: Option<&str>,
            edge: Edge,
            initial_count: u32,
            direction: CountDirection,
        ) -> DaqmxResult<()> {
            let counter = required_cstring("counter", counter)?;
            let name = optional_cstring(name)?;
            check_status(unsafe {
                ni_daqmx_sys::DAQmxCreateCICountEdgesChan(
                    self.handle,
                    counter.as_ptr(),
                    cstr_ptr(name.as_ref()),
                    edge.as_raw(),
                    initial_count,
                    direction.as_raw(),
                )
            })
        }

        pub(super) fn create_co_pulse_channel_freq(
            &self,
            counter: &str,
            name: Option<&str>,
            idle_state: IdleState,
            initial_delay_seconds: f64,
            frequency_hz: f64,
            duty_cycle: f64,
        ) -> DaqmxResult<()> {
            let counter = required_cstring("counter", counter)?;
            let name = optional_cstring(name)?;
            check_status(unsafe {
                ni_daqmx_sys::DAQmxCreateCOPulseChanFreq(
                    self.handle,
                    counter.as_ptr(),
                    cstr_ptr(name.as_ref()),
                    ni_daqmx_sys::DAQmx_Val_Hz,
                    idle_state.as_raw(),
                    initial_delay_seconds,
                    frequency_hz,
                    duty_cycle,
                )
            })
        }

        pub(super) fn cfg_sample_clock_timing(
            &self,
            source: Option<&str>,
            rate_hz: f64,
            edge: Edge,
            sample_mode: SampleMode,
            samples_per_channel: u64,
        ) -> DaqmxResult<()> {
            let source = optional_cstring(source)?;
            check_status(unsafe {
                ni_daqmx_sys::DAQmxCfgSampClkTiming(
                    self.handle,
                    cstr_ptr(source.as_ref()),
                    rate_hz,
                    edge.as_raw(),
                    sample_mode.as_raw(),
                    samples_per_channel,
                )
            })
        }

        pub(super) fn cfg_implicit_timing(
            &self,
            sample_mode: SampleMode,
            samples_per_channel: u64,
        ) -> DaqmxResult<()> {
            check_status(unsafe {
                ni_daqmx_sys::DAQmxCfgImplicitTiming(
                    self.handle,
                    sample_mode.as_raw(),
                    samples_per_channel,
                )
            })
        }

        pub(super) fn cfg_digital_edge_start_trigger(
            &self,
            trigger_source: &str,
            edge: Edge,
        ) -> DaqmxResult<()> {
            let trigger_source = required_cstring("trigger_source", trigger_source)?;
            check_status(unsafe {
                ni_daqmx_sys::DAQmxCfgDigEdgeStartTrig(
                    self.handle,
                    trigger_source.as_ptr(),
                    edge.as_raw(),
                )
            })
        }

        pub(super) fn write_analog_f64(
            &self,
            samples_per_channel: i32,
            auto_start: bool,
            timeout_seconds: f64,
            layout: DataLayout,
            data: &[f64],
        ) -> DaqmxResult<i32> {
            let mut written = 0;
            check_status(unsafe {
                ni_daqmx_sys::DAQmxWriteAnalogF64(
                    self.handle,
                    samples_per_channel,
                    bool32(auto_start),
                    timeout_seconds,
                    layout.as_raw(),
                    data.as_ptr(),
                    &mut written,
                    ptr::null_mut(),
                )
            })?;
            Ok(written)
        }

        pub(super) fn write_digital_lines(
            &self,
            samples_per_channel: i32,
            auto_start: bool,
            timeout_seconds: f64,
            layout: DataLayout,
            data: &[u8],
        ) -> DaqmxResult<i32> {
            let mut written = 0;
            check_status(unsafe {
                ni_daqmx_sys::DAQmxWriteDigitalLines(
                    self.handle,
                    samples_per_channel,
                    bool32(auto_start),
                    timeout_seconds,
                    layout.as_raw(),
                    data.as_ptr(),
                    &mut written,
                    ptr::null_mut(),
                )
            })?;
            Ok(written)
        }

        pub(super) fn read_analog_f64(
            &self,
            samples_per_channel: i32,
            timeout_seconds: f64,
            layout: DataLayout,
            buffer: &mut [f64],
        ) -> DaqmxResult<i32> {
            let mut read = 0;
            check_status(unsafe {
                ni_daqmx_sys::DAQmxReadAnalogF64(
                    self.handle,
                    samples_per_channel,
                    timeout_seconds,
                    layout.as_raw(),
                    buffer.as_mut_ptr(),
                    buffer.len() as ni_daqmx_sys::uInt32,
                    &mut read,
                    ptr::null_mut(),
                )
            })?;
            Ok(read)
        }

        pub(super) fn read_counter_u32(
            &self,
            samples_per_channel: i32,
            timeout_seconds: f64,
            buffer: &mut [u32],
        ) -> DaqmxResult<i32> {
            let mut read = 0;
            check_status(unsafe {
                ni_daqmx_sys::DAQmxReadCounterU32(
                    self.handle,
                    samples_per_channel,
                    timeout_seconds,
                    buffer.as_mut_ptr(),
                    buffer.len() as ni_daqmx_sys::uInt32,
                    &mut read,
                    ptr::null_mut(),
                )
            })?;
            Ok(read)
        }

        fn clear_inner(&mut self) -> DaqmxResult<()> {
            if self.cleared || self.handle.is_null() {
                return Ok(());
            }
            let status = unsafe { ni_daqmx_sys::DAQmxClearTask(self.handle) };
            if status >= 0 {
                self.cleared = true;
                self.handle = ptr::null_mut();
            }
            check_status(status)
        }
    }

    impl Drop for DaqmxTask {
        fn drop(&mut self) {
            let _ = self.clear_inner();
        }
    }

    fn check_status(status: ni_daqmx_sys::int32) -> DaqmxResult<()> {
        if status < 0 {
            Err(DaqmxError {
                status,
                message: extended_error_message(status),
            })
        } else {
            Ok(())
        }
    }

    fn extended_error_message(status: ni_daqmx_sys::int32) -> String {
        let mut buffer = vec![0 as c_char; 4096];
        let extended_status = unsafe {
            ni_daqmx_sys::DAQmxGetExtendedErrorInfo(
                buffer.as_mut_ptr(),
                buffer.len() as ni_daqmx_sys::uInt32,
            )
        };
        if extended_status >= 0 {
            let message = unsafe { CStr::from_ptr(buffer.as_ptr()) }
                .to_string_lossy()
                .into_owned();
            if !message.trim().is_empty() {
                return message;
            }
        }

        let error_status = unsafe {
            ni_daqmx_sys::DAQmxGetErrorString(
                status,
                buffer.as_mut_ptr(),
                buffer.len() as ni_daqmx_sys::uInt32,
            )
        };
        if error_status >= 0 {
            unsafe { CStr::from_ptr(buffer.as_ptr()) }
                .to_string_lossy()
                .into_owned()
        } else {
            format!("DAQmxGetExtendedErrorInfo returned {extended_status}; DAQmxGetErrorString returned {error_status}")
        }
    }

    fn optional_cstring(value: Option<&str>) -> DaqmxResult<Option<CString>> {
        value
            .map(|value| required_cstring("optional string", value))
            .transpose()
    }

    fn required_cstring(field: &str, value: &str) -> DaqmxResult<CString> {
        CString::new(value).map_err(|_| DaqmxError {
            status: 0,
            message: format!("{field} contains an interior NUL byte"),
        })
    }

    fn cstr_ptr(value: Option<&CString>) -> *const c_char {
        value.map(|value| value.as_ptr()).unwrap_or_else(ptr::null)
    }

    fn bool32(value: bool) -> ni_daqmx_sys::bool32 {
        if value {
            1
        } else {
            0
        }
    }
}

#[cfg(any(
    not(feature = "ni-daqmx-sdk"),
    all(
        feature = "ni-daqmx-sdk",
        not(any(target_os = "linux", target_os = "windows"))
    )
))]
mod live_backend {
    use super::DaqmxRuntimeProbe;
    use numanager_core::TimeInterval;

    pub(super) fn probe_runtime(
        _: &str,
        _: bool,
        _: Option<&str>,
        _: TimeInterval,
    ) -> std::result::Result<DaqmxRuntimeProbe, String> {
        Err("crate was built without the ni-daqmx-sdk feature".into())
    }
}

fn backend_status(
    config: &ImSwitchDaqmxConfig,
    runtime_probe: Option<&DaqmxRuntimeProbe>,
) -> Value {
    let package_identity_recorded = daqmx_package_identity_recorded(config, runtime_probe);
    let sdk_header_recorded = daqmx_sdk_header_recorded(config);
    let feature_requested = ni_daqmx_sdk_cargo_feature_enabled();
    let target_supported = ni_daqmx_sdk_target_supported();
    let feature_enabled = ni_daqmx_sdk_feature_enabled();
    let runtime_detected = runtime_probe.is_some();
    let metadata_configured = package_identity_recorded && sdk_header_recorded && feature_enabled;
    let live_task_execution_blocker = live_task_execution_blocker(config, runtime_probe);
    let (
        configured_runtime_version_major,
        configured_runtime_version_minor,
        configured_runtime_version_update,
    ) = parse_runtime_version_components(config.runtime_version.as_deref());
    let runtime_version_comparison = compare_runtime_versions(
        config.runtime_version.is_some(),
        (
            configured_runtime_version_major,
            configured_runtime_version_minor,
            configured_runtime_version_update,
        ),
        runtime_probe,
    );

    let missing = daqmx_live_missing(config, runtime_probe);

    Value::Map(BTreeMap::from([
        ("feature_requested".into(), Value::Bool(feature_requested)),
        ("target_supported".into(), Value::Bool(target_supported)),
        ("feature_enabled".into(), Value::Bool(feature_enabled)),
        ("connect_requested".into(), Value::Bool(config.connect)),
        (
            "configured_runtime_version".into(),
            config
                .runtime_version
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        ),
        (
            "configured_runtime_version_major".into(),
            configured_runtime_version_major
                .map(|value| Value::I64(value as i64))
                .unwrap_or(Value::Null),
        ),
        (
            "configured_runtime_version_minor".into(),
            configured_runtime_version_minor
                .map(|value| Value::I64(value as i64))
                .unwrap_or(Value::Null),
        ),
        (
            "configured_runtime_version_update".into(),
            configured_runtime_version_update
                .map(|value| Value::I64(value as i64))
                .unwrap_or(Value::Null),
        ),
        (
            "live_task_execution_requested".into(),
            Value::Bool(config.live_task_execution),
        ),
        (
            "device_inventory_requested".into(),
            Value::Bool(config.inventory_devices),
        ),
        (
            "inventory_helper_configured".into(),
            Value::Bool(config.inventory_helper_path.is_some()),
        ),
        (
            "inventory_helper_timeout".into(),
            Value::TimeInterval(config.inventory_helper_timeout),
        ),
        ("runtime_detected".into(), Value::Bool(runtime_detected)),
        (
            "task_wrapper_compiled".into(),
            Value::Bool(daqmx_task_wrapper_compiled()),
        ),
        (
            "bringup_helpers_compiled".into(),
            bringup_helpers_compiled_value(),
        ),
        (
            "detected_runtime_version".into(),
            runtime_probe
                .map(|probe| Value::String(probe.version.clone()))
                .unwrap_or(Value::Null),
        ),
        (
            "detected_runtime_version_major".into(),
            runtime_probe
                .and_then(|probe| probe.version_major)
                .map(|value| Value::I64(value as i64))
                .unwrap_or(Value::Null),
        ),
        (
            "detected_runtime_version_minor".into(),
            runtime_probe
                .and_then(|probe| probe.version_minor)
                .map(|value| Value::I64(value as i64))
                .unwrap_or(Value::Null),
        ),
        (
            "detected_runtime_version_update".into(),
            runtime_probe
                .and_then(|probe| probe.version_update)
                .map(|value| Value::I64(value as i64))
                .unwrap_or(Value::Null),
        ),
        (
            "runtime_version_comparison".into(),
            Value::String(runtime_version_comparison.status.into()),
        ),
        (
            "runtime_version_matches".into(),
            runtime_version_comparison
                .matches
                .map(Value::Bool)
                .unwrap_or(Value::Null),
        ),
        (
            "runtime_version_comparison_basis".into(),
            Value::String(runtime_version_comparison.basis.into()),
        ),
        (
            "detected_devices".into(),
            runtime_probe
                .map(|probe| string_list(&probe.device_names))
                .unwrap_or_else(|| Value::List(Vec::new())),
        ),
        (
            "device_inventory_error".into(),
            runtime_probe
                .and_then(|probe| probe.device_inventory_error.clone())
                .map(Value::String)
                .unwrap_or(Value::Null),
        ),
        (
            "configured_device_detected".into(),
            Value::Bool(
                runtime_probe
                    .and_then(|probe| probe.configured_device.as_ref())
                    .is_some(),
            ),
        ),
        (
            "configured_device_identity".into(),
            runtime_probe
                .and_then(|probe| probe.configured_device.as_ref())
                .map(daqmx_device_probe_value)
                .unwrap_or(Value::Null),
        ),
        (
            "configured_device_error".into(),
            runtime_probe
                .and_then(|probe| probe.configured_device_error.clone())
                .map(Value::String)
                .unwrap_or(Value::Null),
        ),
        ("configured".into(), Value::Bool(missing.is_empty())),
        (
            "metadata_configured".into(),
            Value::Bool(metadata_configured),
        ),
        (
            "package_identity_recorded".into(),
            Value::Bool(package_identity_recorded),
        ),
        (
            "sdk_header_recorded".into(),
            Value::Bool(sdk_header_recorded),
        ),
        ("missing".into(), Value::List(missing)),
        (
            "external_promotion_gate_statuses".into(),
            daqmx_external_promotion_gate_statuses_value(),
        ),
        (
            "external_promotion_gates".into(),
            daqmx_external_promotion_gates_value(),
        ),
        (
            "execution_status".into(),
            Value::String(if runtime_probe.is_some() {
                "runtime_probe_only".into()
            } else {
                "not_live_backend".into()
            }),
        ),
        ("live_task_execution_ready".into(), Value::Bool(false)),
        (
            "live_task_execution_blocker".into(),
            Value::String(live_task_execution_blocker.into()),
        ),
        (
            "hardware_validation_status".into(),
            Value::String("pending".into()),
        ),
        (
            "evidence_status".into(),
            Value::String("pending_ni_daqmx_runtime_evidence".into()),
        ),
    ]))
}

struct RuntimeVersionComparison {
    status: &'static str,
    matches: Option<bool>,
    basis: &'static str,
}

fn compare_runtime_versions(
    configured_present: bool,
    configured: (Option<u32>, Option<u32>, Option<u32>),
    runtime_probe: Option<&DaqmxRuntimeProbe>,
) -> RuntimeVersionComparison {
    if !configured_present {
        return RuntimeVersionComparison {
            status: "not_configured",
            matches: None,
            basis: "configured_runtime_version_missing",
        };
    }
    let Some(runtime_probe) = runtime_probe else {
        return RuntimeVersionComparison {
            status: "not_detected",
            matches: None,
            basis: "runtime_probe_missing",
        };
    };
    let (Some(configured_major), Some(configured_minor), configured_update) = configured else {
        return RuntimeVersionComparison {
            status: "unknown",
            matches: None,
            basis: "configured_runtime_version_unparseable",
        };
    };
    let (Some(detected_major), Some(detected_minor)) =
        (runtime_probe.version_major, runtime_probe.version_minor)
    else {
        return RuntimeVersionComparison {
            status: "unknown",
            matches: None,
            basis: "detected_runtime_version_partial",
        };
    };

    if let Some(configured_update) = configured_update {
        let Some(detected_update) = runtime_probe.version_update else {
            return RuntimeVersionComparison {
                status: "unknown",
                matches: None,
                basis: "detected_runtime_version_update_missing",
            };
        };
        let matches = configured_major == detected_major
            && configured_minor == detected_minor
            && configured_update == detected_update;
        return RuntimeVersionComparison {
            status: if matches { "match" } else { "mismatch" },
            matches: Some(matches),
            basis: "configured_major_minor_update",
        };
    }

    let matches = configured_major == detected_major && configured_minor == detected_minor;
    RuntimeVersionComparison {
        status: if matches { "match" } else { "mismatch" },
        matches: Some(matches),
        basis: "configured_major_minor",
    }
}

fn parse_runtime_version_components(
    version: Option<&str>,
) -> (Option<u32>, Option<u32>, Option<u32>) {
    let Some(version) = version else {
        return (None, None, None);
    };
    let mut parts = version.split('.');
    let major = parts.next().and_then(parse_version_component);
    let minor = parts.next().and_then(parse_version_component);
    let update = parts.next().and_then(parse_version_component);
    (major, minor, update)
}

fn parse_version_component(value: &str) -> Option<u32> {
    let digits = value
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<u32>().ok()
    }
}

fn live_task_execution_blocker(
    config: &ImSwitchDaqmxConfig,
    runtime_probe: Option<&DaqmxRuntimeProbe>,
) -> &'static str {
    let package_identity_recorded = daqmx_package_identity_recorded(config, runtime_probe);
    let sdk_header_recorded = daqmx_sdk_header_recorded(config);
    if !ni_daqmx_sdk_cargo_feature_enabled() {
        "feature_ni_daqmx_sdk"
    } else if !ni_daqmx_sdk_target_supported() {
        "target_platform_linux_or_windows"
    } else if !package_identity_recorded || !sdk_header_recorded {
        "package_or_header_evidence_missing"
    } else if runtime_probe.is_none() {
        "runtime_not_detected"
    } else if let Some(blocker) = runtime_version_blocker(config, runtime_probe) {
        blocker
    } else if !config.live_task_execution {
        "live_task_execution_not_requested"
    } else {
        "pending_hardware_validation"
    }
}

fn live_task_execution_readiness_plan(
    config: &ImSwitchDaqmxConfig,
    runtime_probe: Option<&DaqmxRuntimeProbe>,
) -> Value {
    let runtime_version = parse_runtime_version_components(config.runtime_version.as_deref());
    let runtime_version_comparison = compare_runtime_versions(
        config.runtime_version.is_some(),
        runtime_version,
        runtime_probe,
    );
    Value::Map(BTreeMap::from([
        (
            "feature_requested".into(),
            Value::Bool(ni_daqmx_sdk_cargo_feature_enabled()),
        ),
        (
            "target_supported".into(),
            Value::Bool(ni_daqmx_sdk_target_supported()),
        ),
        (
            "feature_enabled".into(),
            Value::Bool(ni_daqmx_sdk_feature_enabled()),
        ),
        (
            "runtime_detected".into(),
            Value::Bool(runtime_probe.is_some()),
        ),
        (
            "runtime_version_comparison".into(),
            Value::String(runtime_version_comparison.status.into()),
        ),
        (
            "runtime_version_matches".into(),
            runtime_version_comparison
                .matches
                .map(Value::Bool)
                .unwrap_or(Value::Null),
        ),
        (
            "runtime_version_comparison_basis".into(),
            Value::String(runtime_version_comparison.basis.into()),
        ),
        (
            "package_identity_recorded".into(),
            Value::Bool(daqmx_package_identity_recorded(config, runtime_probe)),
        ),
        (
            "sdk_header_recorded".into(),
            Value::Bool(daqmx_sdk_header_recorded(config)),
        ),
        (
            "live_task_execution_requested".into(),
            Value::Bool(config.live_task_execution),
        ),
        ("live_task_execution_ready".into(), Value::Bool(false)),
        (
            "live_task_execution_blocker".into(),
            Value::String(live_task_execution_blocker(config, runtime_probe).into()),
        ),
        (
            "hardware_validation_status".into(),
            Value::String("pending".into()),
        ),
        (
            "evidence_status".into(),
            Value::String("pending_ni_daqmx_runtime_evidence".into()),
        ),
        (
            "missing".into(),
            Value::List(daqmx_live_missing(config, runtime_probe)),
        ),
        (
            "external_promotion_gates".into(),
            daqmx_external_promotion_gates_value(),
        ),
        (
            "external_promotion_gate_statuses".into(),
            daqmx_external_promotion_gate_statuses_value(),
        ),
    ]))
}

fn daqmx_package_identity_recorded(
    config: &ImSwitchDaqmxConfig,
    runtime_probe: Option<&DaqmxRuntimeProbe>,
) -> bool {
    config.runtime_package.is_some()
        && (config.runtime_version.is_some() || runtime_probe.is_some())
        && config.runtime_platform.is_some()
        && config.runtime_license.is_some()
}

fn daqmx_sdk_header_recorded(config: &ImSwitchDaqmxConfig) -> bool {
    config.sdk_header_path.is_some() && config.sdk_header_sha256.is_some()
}

fn daqmx_live_missing(
    config: &ImSwitchDaqmxConfig,
    runtime_probe: Option<&DaqmxRuntimeProbe>,
) -> Vec<Value> {
    let mut missing = Vec::new();
    if config.runtime_package.is_none() {
        missing.push(Value::String("runtime_package".into()));
    }
    if config.runtime_version.is_none() && runtime_probe.is_none() {
        missing.push(Value::String("runtime_version".into()));
    }
    if config.runtime_platform.is_none() {
        missing.push(Value::String("runtime_platform".into()));
    }
    if config.runtime_license.is_none() {
        missing.push(Value::String("runtime_license".into()));
    }
    if config.sdk_header_path.is_none() {
        missing.push(Value::String("sdk_header_path".into()));
    }
    if config.sdk_header_sha256.is_none() {
        missing.push(Value::String("sdk_header_sha256".into()));
    }
    if let Some(blocker) = runtime_version_blocker(config, runtime_probe) {
        missing.push(Value::String(blocker.into()));
    }
    if !ni_daqmx_sdk_cargo_feature_enabled() {
        missing.push(Value::String("feature_ni_daqmx_sdk".into()));
    }
    if !ni_daqmx_sdk_target_supported() {
        missing.push(Value::String("target_platform_linux_or_windows".into()));
    }
    missing.push(Value::String("api_audit_and_hardware_validation".into()));
    missing
}

fn runtime_version_blocker(
    config: &ImSwitchDaqmxConfig,
    runtime_probe: Option<&DaqmxRuntimeProbe>,
) -> Option<&'static str> {
    config.runtime_version.as_ref()?;
    let runtime_version = parse_runtime_version_components(config.runtime_version.as_deref());
    match compare_runtime_versions(true, runtime_version, runtime_probe).matches {
        Some(true) => None,
        Some(false) => Some("runtime_version_mismatch"),
        None => Some("runtime_version_unverified"),
    }
}

fn daqmx_external_promotion_gates_value() -> Value {
    string_list(
        &daqmx_external_promotion_gates()
            .iter()
            .map(|gate| (*gate).into())
            .collect::<Vec<_>>(),
    )
}

fn daqmx_external_promotion_gate_statuses_value() -> Value {
    Value::Map(
        daqmx_external_promotion_gates()
            .iter()
            .map(|gate| {
                (
                    (*gate).into(),
                    Value::Map(BTreeMap::from([
                        (
                            "evidence_required".into(),
                            Value::String(daqmx_external_promotion_gate_evidence(gate).into()),
                        ),
                        ("status".into(), Value::String("pending".into())),
                        (
                            "support_claim".into(),
                            Value::String("not_validated".into()),
                        ),
                    ])),
                )
            })
            .collect(),
    )
}

fn daqmx_external_promotion_gates() -> &'static [&'static str] {
    &[
        "legal_review",
        "installed_windows_package_license_review",
        "installed_linux_26_5_header_audit",
        "installed_windows_26_5_header_audit",
        "ni_pal_device_inventory",
        "bench_safety_preconditions",
        "task_ordering_routing_completion_cleanup_bench_validation",
        "runtime_publication_hardware_validation",
        "hardware_validation_note",
    ]
}

fn daqmx_external_promotion_gate_evidence(gate: &str) -> &'static str {
    match gate {
        "legal_review" => {
            "Completed package-intake legal review for exact Linux and Windows inputs"
        }
        "installed_windows_package_license_review" => {
            "Installed Windows package/license boundary audit recorded"
        }
        "installed_linux_26_5_header_audit" => {
            "Installed Linux 26.5 NIDAQmx.h inventory, digest, and bindgen command recorded"
        }
        "installed_windows_26_5_header_audit" => {
            "Installed Windows 26.5 NIDAQmx.h inventory, digest, and bindgen command recorded"
        }
        "ni_pal_device_inventory" => {
            "Process-isolated NI-PAL/device inventory and configured-device identity recorded"
        }
        "bench_safety_preconditions" => {
            "Completed Setup And Safety table plus reviewed wiring, load, safe output state, interlocks, emergency stop, cleanup, and fault-recovery constraints"
        }
        "task_ordering_routing_completion_cleanup_bench_validation" => {
            "Bench logs for task order, routing, completion, stop/clear, cleanup, and safe output state"
        }
        "runtime_publication_hardware_validation" => {
            "Hardware-backed FrameReady and ScanSignalChunk runtime output logs"
        }
        "hardware_validation_note" => {
            "Completed hardware validation note following docs/devices/hardware-validation-template.md"
        }
        _ => "Documented evidence for this promotion gate",
    }
}

fn daqmx_device_probe_value(device: &DaqmxDeviceProbe) -> Value {
    Value::Map(BTreeMap::from([
        ("name".into(), Value::String(device.name.clone())),
        (
            "product_type".into(),
            device
                .product_type
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        ),
        (
            "serial_number".into(),
            device
                .serial_number
                .map(|serial| Value::String(format!("{serial:X}")))
                .unwrap_or(Value::Null),
        ),
        ("analog_inputs".into(), string_list(&device.analog_inputs)),
        ("analog_outputs".into(), string_list(&device.analog_outputs)),
        ("digital_inputs".into(), string_list(&device.digital_inputs)),
        (
            "digital_outputs".into(),
            string_list(&device.digital_outputs),
        ),
        ("counter_inputs".into(), string_list(&device.counter_inputs)),
        (
            "counter_outputs".into(),
            string_list(&device.counter_outputs),
        ),
    ]))
}

fn string_list(values: &[String]) -> Value {
    Value::List(values.iter().cloned().map(Value::String).collect())
}

fn reversed_string_list(values: &[String]) -> Vec<String> {
    values.iter().rev().cloned().collect()
}

fn backend_unavailable_message(config: &ImSwitchDaqmxConfig) -> String {
    let Value::Map(status) = backend_status(config, None) else {
        return "live NI-DAQmx backend is unavailable".into();
    };
    let missing = match status.get("missing") {
        Some(Value::List(values)) => values
            .iter()
            .filter_map(|value| match value {
                Value::String(value) => Some(value.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
    };
    format!("live NI-DAQmx backend is unavailable; missing evidence/configuration: {missing}")
}

fn ni_daqmx_sdk_feature_enabled() -> bool {
    cfg!(all(
        feature = "ni-daqmx-sdk",
        any(target_os = "linux", target_os = "windows")
    ))
}

fn ni_daqmx_sdk_cargo_feature_enabled() -> bool {
    cfg!(feature = "ni-daqmx-sdk")
}

fn ni_daqmx_sdk_target_supported() -> bool {
    cfg!(any(target_os = "linux", target_os = "windows"))
}

fn daqmx_task_wrapper_compiled() -> bool {
    cfg!(all(
        feature = "ni-daqmx-sdk",
        any(target_os = "linux", target_os = "windows")
    ))
}

fn bringup_helpers_compiled_value() -> Value {
    let compiled = cfg!(all(
        feature = "ni-daqmx-sdk",
        any(target_os = "linux", target_os = "windows")
    ));
    Value::Map(BTreeMap::from([
        ("inventory".into(), Value::Bool(compiled)),
        ("task_lifecycle".into(), Value::Bool(compiled)),
        ("channel_setup".into(), Value::Bool(compiled)),
        ("plan_setup".into(), Value::Bool(compiled)),
        ("io_smoke".into(), Value::Bool(compiled)),
    ]))
}

fn channel_metadata(
    config: &ImSwitchDaqmxConfig,
    channel: usize,
    runtime_probe: Option<&DaqmxRuntimeProbe>,
) -> BTreeMap<String, Value> {
    let mut metadata = common_metadata(config, runtime_probe);
    metadata.insert("channel".into(), Value::I64(channel as i64));
    metadata
}

fn capability(
    id: u64,
    device: DeviceId,
    kind: CapabilityKind,
    response: ValueType,
) -> CapabilityDescriptor {
    CapabilityDescriptor::new(CapabilityId(id), device, kind, response)
}

fn property(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    writable: bool,
    sequenceable: bool,
) -> PropertySchema {
    PropertySchema {
        key: key.into(),
        display_name: display_name.into(),
        value_type,
        unit: None,
        range: None,
        increment: None,
        enum_values: Vec::new(),
        readable: true,
        writable,
        volatile: false,
        sequenceable,
        hardware_address: None,
    }
}

fn string_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::String, writable, false)
}

fn map_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::Map, writable, false)
}

fn integer_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::I64, writable, false)
}

fn bool_property(
    key: &str,
    display_name: &str,
    writable: bool,
    sequenceable: bool,
) -> PropertySchema {
    property(key, display_name, ValueType::Bool, writable, sequenceable)
}

fn frequency_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::Frequency, writable, true)
}

fn time_interval_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::TimeInterval, writable, false)
}

fn voltage_range_property(
    key: &str,
    display_name: &str,
    writable: bool,
    sequenceable: bool,
    min: Voltage,
    max: Voltage,
) -> PropertySchema {
    let mut schema = property(
        key,
        display_name,
        ValueType::Voltage,
        writable,
        sequenceable,
    );
    schema.range = Some(Range {
        min: Value::Voltage(min),
        max: Value::Voltage(max),
    });
    schema
}

fn enum_string_property(
    key: &str,
    display_name: &str,
    writable: bool,
    values: &[&str],
) -> PropertySchema {
    let mut schema = string_property(key, display_name, writable);
    schema.enum_values = values
        .iter()
        .map(|value| EnumValue {
            value: Value::String((*value).into()),
            label: (*value).into(),
        })
        .collect();
    schema
}

fn string_prop(device: &DeviceConfig, key: &str) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
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

fn voltage_prop(device: &DeviceConfig, key: &str) -> Option<Voltage> {
    match device.properties.get(key) {
        Some(Value::Voltage(value)) => Some(*value),
        _ => None,
    }
}

fn frequency_prop(device: &DeviceConfig, key: &str) -> Option<Frequency> {
    match device.properties.get(key) {
        Some(Value::Frequency(value)) => Some(*value),
        _ => None,
    }
}

fn time_interval_prop(device: &DeviceConfig, key: &str) -> Option<TimeInterval> {
    match device.properties.get(key) {
        Some(Value::TimeInterval(value)) => Some(*value),
        _ => None,
    }
}

fn count_prop(
    device: &DeviceConfig,
    key: &str,
    default: usize,
    min: usize,
    max: usize,
) -> Result<usize> {
    let Some(value) = i64_prop(device, key) else {
        return Ok(default);
    };
    if value < min as i64 || value > max as i64 {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("{key} outside supported descriptor range"),
        ));
    }
    Ok(value as usize)
}
