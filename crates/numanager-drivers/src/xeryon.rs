use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::{LineEnding, ScriptedSerial, SerialIo, SerialLineCodec};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
#[cfg(feature = "os-serial")]
use std::time::Duration;

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const BAUD: u32 = 115_200;
    pub const SEND_ENDING: LineEnding = LineEnding::Lf;
    pub const RECV_ENDING: LineEnding = LineEnding::Lf;
    pub const QUERY_TAGS: [&str; 8] = [
        "SRNO", "SOFT", "STAT", "EPOS", "DPOS", "SSPD", "LLIM", "HLIM",
    ];

    #[derive(Debug, Clone, PartialEq)]
    pub struct XeryonProbe {
        pub axis: char,
        pub stage_model: String,
        pub controller_model: String,
        pub serial_number: String,
        pub software_version: String,
        pub encoder_units_per_um: f64,
        pub low_limit_um: f64,
        pub high_limit_um: f64,
        pub position_um: f64,
        pub target_um: f64,
        pub velocity_um_s: f64,
        pub status_bits: u32,
    }

    impl XeryonProbe {
        pub fn simulated() -> Self {
            Self {
                axis: 'X',
                stage_model: "XLS-series configured stage".into(),
                controller_model: "XD-M/XD-C/XD-OEM ASCII controller".into(),
                serial_number: "XERYON-CONFIG-0001".into(),
                software_version: "configured".into(),
                encoder_units_per_um: 1.0,
                low_limit_um: 0.0,
                high_limit_um: 50_000.0,
                position_um: 0.0,
                target_um: 0.0,
                velocity_um_s: 10_000.0,
                status_bits: StatusBits::ENCODER_VALID | StatusBits::POSITION_REACHED,
            }
        }

        pub fn native_position(&self, um: f64) -> i64 {
            (um * self.encoder_units_per_um).round() as i64
        }

        pub fn micrometers(&self, native: i64) -> f64 {
            native as f64 / self.encoder_units_per_um
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct StatusBits;

    impl StatusBits {
        pub const END_STOP: u32 = 1 << 0;
        pub const THERMAL_PHASE_1: u32 = 1 << 1;
        pub const THERMAL_PHASE_2: u32 = 1 << 2;
        pub const FORCE_ZERO: u32 = 1 << 3;
        pub const MOTOR_ON: u32 = 1 << 4;
        pub const CLOSED_LOOP: u32 = 1 << 5;
        pub const ENCODER_AT_INDEX: u32 = 1 << 6;
        pub const ENCODER_VALID: u32 = 1 << 7;
        pub const SEARCHING_INDEX: u32 = 1 << 8;
        pub const POSITION_REACHED: u32 = 1 << 9;
        pub const ERROR_COMPENSATION: u32 = 1 << 10;
        pub const ENCODER_ERROR: u32 = 1 << 11;
        pub const SCANNING: u32 = 1 << 12;
        pub const LEFT_END_STOP: u32 = 1 << 13;
        pub const RIGHT_END_STOP: u32 = 1 << 14;
        pub const ERROR_LIMIT: u32 = 1 << 15;
        pub const SEARCHING_OPTIMAL_FREQUENCY: u32 = 1 << 16;
        pub const SAFETY_TIMEOUT: u32 = 1 << 17;
        pub const POSITION_FAIL: u32 = 1 << 20;
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum XeryonCommand {
        Query {
            axis: Option<char>,
            tag: String,
        },
        Set {
            axis: Option<char>,
            tag: String,
            value: i64,
        },
        NoValue {
            axis: Option<char>,
            tag: String,
        },
    }

    pub fn encode(command: &XeryonCommand) -> Result<String> {
        let line = match command {
            XeryonCommand::Query { axis, tag } => prefix(*axis, tag, "=?"),
            XeryonCommand::Set { axis, tag, value } => prefix(*axis, tag, &format!("={value}")),
            XeryonCommand::NoValue { axis, tag } => prefix(*axis, tag, ""),
        };
        if line.len() > 16 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                format!("Xeryon command exceeds 16 character limit: {line}"),
            ));
        }
        Ok(line)
    }

    fn prefix(axis: Option<char>, tag: &str, suffix: &str) -> String {
        match axis {
            Some(axis) => format!("{axis}:{tag}{suffix}"),
            None => format!("{tag}{suffix}"),
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct XeryonReply {
        pub axis: Option<char>,
        pub tag: String,
        pub value: i64,
    }

    pub fn parse_reply(line: &str) -> Result<XeryonReply> {
        let line = line.trim();
        let (axis, rest) = if line.len() >= 2 && line.as_bytes()[1] == b':' {
            let axis = line
                .chars()
                .next()
                .ok_or_else(|| Error::new(ErrorCode::Transport, "empty Xeryon reply"))?;
            (Some(axis), &line[2..])
        } else {
            (None, line)
        };
        let (tag, value) = rest.split_once('=').ok_or_else(|| {
            Error::new(
                ErrorCode::Transport,
                format!("Xeryon reply is missing '=': {line}"),
            )
        })?;
        if tag.len() != 4 {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("Xeryon reply tag is not four characters: {tag}"),
            ));
        }
        let value = value.trim().parse::<i64>().map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("invalid Xeryon {tag} integer reply {value}: {error}"),
            )
        })?;
        Ok(XeryonReply {
            axis,
            tag: tag.into(),
            value,
        })
    }

    pub fn status_flag(status: u32, bit: u32) -> bool {
        status & bit != 0
    }

    pub fn busy(status: u32) -> bool {
        status_flag(status, StatusBits::MOTOR_ON)
            || status_flag(status, StatusBits::SEARCHING_INDEX)
            || status_flag(status, StatusBits::SCANNING)
            || status_flag(status, StatusBits::SEARCHING_OPTIMAL_FREQUENCY)
    }

    pub fn fault_active(status: u32) -> bool {
        status_flag(status, StatusBits::THERMAL_PHASE_1)
            || status_flag(status, StatusBits::THERMAL_PHASE_2)
            || status_flag(status, StatusBits::ENCODER_ERROR)
            || status_flag(status, StatusBits::ERROR_LIMIT)
            || status_flag(status, StatusBits::SAFETY_TIMEOUT)
            || status_flag(status, StatusBits::POSITION_FAIL)
    }

    pub fn status_summary(status: u32) -> Value {
        Value::Map(BTreeMap::from([
            ("raw".into(), Value::I64(status as i64)),
            ("busy".into(), Value::Bool(busy(status))),
            ("fault_active".into(), Value::Bool(fault_active(status))),
            (
                "position_reached".into(),
                Value::Bool(status_flag(status, StatusBits::POSITION_REACHED)),
            ),
            (
                "indexed".into(),
                Value::Bool(status_flag(status, StatusBits::ENCODER_VALID)),
            ),
            (
                "encoder_at_index".into(),
                Value::Bool(status_flag(status, StatusBits::ENCODER_AT_INDEX)),
            ),
            (
                "left_end_stop".into(),
                Value::Bool(status_flag(status, StatusBits::LEFT_END_STOP)),
            ),
            (
                "right_end_stop".into(),
                Value::Bool(status_flag(status, StatusBits::RIGHT_END_STOP)),
            ),
            (
                "error_limit".into(),
                Value::Bool(status_flag(status, StatusBits::ERROR_LIMIT)),
            ),
            (
                "safety_timeout".into(),
                Value::Bool(status_flag(status, StatusBits::SAFETY_TIMEOUT)),
            ),
            (
                "encoder_error".into(),
                Value::Bool(status_flag(status, StatusBits::ENCODER_ERROR)),
            ),
        ]))
    }

    pub fn execute_probe_script(
        axis: char,
        serial: &mut dyn SerialIo,
        polls_per_command: usize,
        template: &XeryonProbe,
    ) -> Result<XeryonProbe> {
        let mut codec = SerialLineCodec::new(SEND_ENDING, RECV_ENDING);
        let mut probe = template.clone();
        for tag in QUERY_TAGS {
            let command = XeryonCommand::Query {
                axis: Some(axis),
                tag: tag.into(),
            };
            let line = encode(&command)?;
            serial.write(&codec.encode(&line))?;
            let mut reply = None;
            for _ in 0..polls_per_command.max(1) {
                let bytes = serial.read_available()?;
                for line in codec.push(&bytes) {
                    let parsed = parse_reply(&line)?;
                    if parsed.tag == tag && (parsed.axis == Some(axis) || parsed.axis.is_none()) {
                        reply = Some(parsed);
                        break;
                    }
                }
                if reply.is_some() {
                    break;
                }
            }
            if let Some(reply) = reply {
                apply_reply(&mut probe, &reply)?;
            }
        }
        Ok(probe)
    }

    pub fn apply_reply(probe: &mut XeryonProbe, reply: &XeryonReply) -> Result<()> {
        if let Some(axis) = reply.axis {
            if axis != probe.axis {
                return Err(Error::new(
                    ErrorCode::Transport,
                    "Xeryon reply axis did not match configured axis",
                ));
            }
        }
        match reply.tag.as_str() {
            "SRNO" => probe.serial_number = reply.value.to_string(),
            "SOFT" => probe.software_version = software_version(reply.value),
            "STAT" => probe.status_bits = reply.value as u32,
            "EPOS" => probe.position_um = probe.micrometers(reply.value),
            "DPOS" => probe.target_um = probe.micrometers(reply.value),
            "SSPD" => probe.velocity_um_s = reply.value as f64,
            "LLIM" => probe.low_limit_um = probe.micrometers(reply.value),
            "HLIM" => probe.high_limit_um = probe.micrometers(reply.value),
            _ => {}
        }
        Ok(())
    }

    fn software_version(value: i64) -> String {
        if value >= 10_000 {
            let major = value / 10_000;
            let minor = (value / 100) % 100;
            let patch = value % 100;
            format!("{major}.{minor}.{patch}")
        } else {
            value.to_string()
        }
    }
}

pub struct XeryonDiscovery {
    next_id: DriverId,
    probes: Vec<XeryonConfiguredProbe>,
}

impl XeryonDiscovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![XeryonConfiguredProbe::simulated()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), "xeryon" | "xeryon_ascii"))
            .map(XeryonConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for XeryonDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = probe.label.clone();
                let driver = if probe.connect_real_transport {
                    Box::new(XeryonDriver::serial(id, probe)?) as Box<dyn Driver>
                } else {
                    Box::new(XeryonDriver::configured(id, probe)) as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct XeryonConfiguredProbe {
    pub label: String,
    pub probe: protocol::XeryonProbe,
    pub endpoint: Option<XeryonSerialEndpoint>,
    pub connect_real_transport: bool,
    pub startup_readback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XeryonSerialEndpoint {
    pub port_name: String,
    pub baud_rate: u32,
    pub timeout_ms: u64,
}

impl XeryonConfiguredProbe {
    pub fn simulated() -> Self {
        Self {
            label: "Configured Xeryon ASCII stage".into(),
            probe: protocol::XeryonProbe::simulated(),
            endpoint: None,
            connect_real_transport: false,
            startup_readback: false,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = protocol::XeryonProbe::simulated();
        probe.axis = axis_prop(device, "axis").unwrap_or(probe.axis);
        probe.stage_model = string_prop(device, "stage_model").unwrap_or(probe.stage_model);
        probe.controller_model =
            string_prop(device, "controller_model").unwrap_or(probe.controller_model);
        probe.serial_number = string_prop(device, "serial_number").unwrap_or(probe.serial_number);
        probe.software_version =
            string_prop(device, "software_version").unwrap_or(probe.software_version);
        probe.encoder_units_per_um =
            f64_prop(device, "encoder_units_per_um").unwrap_or(probe.encoder_units_per_um);
        if probe.encoder_units_per_um <= 0.0 {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "xeryon encoder_units_per_um must be positive",
            ));
        }
        probe.low_limit_um =
            position_config_um(device, "low_limit", "low_limit_um").unwrap_or(probe.low_limit_um);
        probe.high_limit_um = position_config_um(device, "high_limit", "high_limit_um")
            .unwrap_or(probe.high_limit_um);
        probe.position_um =
            position_config_um(device, "position", "position_um").unwrap_or(probe.position_um);
        probe.target_um =
            position_config_um(device, "target", "target_um").unwrap_or(probe.target_um);
        probe.velocity_um_s = velocity_config_um_s(device, "velocity", "velocity_um_s")
            .unwrap_or(probe.velocity_um_s);
        probe.status_bits = u32_prop(device, "status_bits").unwrap_or(probe.status_bits);

        let endpoint = string_prop(device, "serial_port").map(|port_name| XeryonSerialEndpoint {
            port_name,
            baud_rate: u32_prop(device, "baud_rate").unwrap_or(protocol::BAUD),
            timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(5),
        });

        Ok(Self {
            label: if device.label.is_empty() {
                format!("Configured Xeryon {} axis", probe.axis)
            } else {
                device.label.clone()
            },
            probe,
            endpoint,
            connect_real_transport: bool_prop(device, "connect").unwrap_or(false),
            startup_readback: bool_prop(device, "startup_readback").unwrap_or(false),
        })
    }
}

pub struct XeryonDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    axis_device: DeviceId,
    probe: protocol::XeryonProbe,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
    position_um: f64,
    target_um: f64,
    velocity_um_s: f64,
    status_bits: u32,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
}

impl XeryonDriver {
    pub fn simulated(id: DriverId) -> Self {
        Self::configured(id, XeryonConfiguredProbe::simulated())
    }

    pub fn configured(id: DriverId, configured: XeryonConfiguredProbe) -> Self {
        Self::new_with_transport_metadata(
            id,
            configured.probe,
            configured.endpoint,
            false,
            Box::new(ScriptedSerial::new()),
        )
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: XeryonConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Xeryon serial probe is missing serial_port metadata",
            )
        })?;
        let mut serial = numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(
                endpoint.port_name.clone(),
                endpoint.baud_rate,
            )
            .timeout(Duration::from_millis(endpoint.timeout_ms)),
        )?;
        let probe = if configured.startup_readback {
            protocol::execute_probe_script(
                configured.probe.axis,
                &mut serial,
                32,
                &configured.probe,
            )?
        } else {
            configured.probe
        };
        Ok(Self::new_with_transport_metadata(
            id,
            probe,
            Some(endpoint),
            true,
            Box::new(serial),
        ))
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _configured: XeryonConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Xeryon real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(id: DriverId, probe: protocol::XeryonProbe, serial: Box<dyn SerialIo>) -> Self {
        Self::new_with_transport_metadata(id, probe, None, false, serial)
    }

    fn new_with_transport_metadata(
        id: DriverId,
        probe: protocol::XeryonProbe,
        endpoint: Option<XeryonSerialEndpoint>,
        connected: bool,
        serial: Box<dyn SerialIo>,
    ) -> Self {
        let serial_port = endpoint.as_ref().map(|endpoint| endpoint.port_name.clone());
        let baud_rate = endpoint
            .as_ref()
            .map(|endpoint| endpoint.baud_rate)
            .unwrap_or(protocol::BAUD);
        let serial_timeout_ms = endpoint
            .as_ref()
            .map(|endpoint| endpoint.timeout_ms)
            .unwrap_or(5);
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 901)),
            hub: DeviceId(NodeId(id.0 * 1000 + 910)),
            axis_device: DeviceId(NodeId(id.0 * 1000 + 911)),
            position_um: probe.position_um,
            target_um: probe.target_um,
            velocity_um_s: probe.velocity_um_s,
            status_bits: probe.status_bits,
            probe,
            serial_port,
            baud_rate,
            serial_timeout_ms,
            connected,
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            codec: SerialLineCodec::new(protocol::SEND_ENDING, protocol::RECV_ENDING),
        }
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn send(&mut self, command: protocol::XeryonCommand) -> Result<()> {
        let line = protocol::encode(&command)?;
        self.serial.write(&self.codec.encode(&line))
    }

    fn read_available_replies(&mut self) -> Result<Vec<protocol::XeryonReply>> {
        let bytes = self.serial.read_available()?;
        let mut replies = Vec::new();
        for line in self.codec.push(&bytes) {
            replies.push(protocol::parse_reply(&line)?);
        }
        Ok(replies)
    }

    fn query_for_property(&self, device: DeviceId, key: &str) -> Option<&'static str> {
        match (device, key) {
            (device, "serial_number") if device == self.hub => Some("SRNO"),
            (device, "software_version") if device == self.hub => Some("SOFT"),
            (device, "position") if device == self.axis_device => Some("EPOS"),
            (
                device,
                "busy" | "status_bits" | "status_summary" | "fault_active" | "indexed"
                | "position_reached" | "axis_summary",
            ) if device == self.axis_device => Some("STAT"),
            (device, "target") if device == self.axis_device => Some("DPOS"),
            (device, "velocity") if device == self.axis_device => Some("SSPD"),
            _ => None,
        }
    }

    fn refresh_tag(&mut self, tag: &str) -> Result<()> {
        self.send(protocol::XeryonCommand::Query {
            axis: Some(self.probe.axis),
            tag: tag.into(),
        })?;
        for reply in self.read_available_replies()? {
            self.apply_reply(&reply)?;
        }
        Ok(())
    }

    fn apply_reply(&mut self, reply: &protocol::XeryonReply) -> Result<()> {
        protocol::apply_reply(&mut self.probe, reply)?;
        self.position_um = self.probe.position_um;
        self.target_um = self.probe.target_um;
        self.velocity_um_s = self.probe.velocity_um_s;
        self.status_bits = self.probe.status_bits;
        self.emit_property(self.axis_device, "position", position(self.position_um));
        self.emit_property(self.axis_device, "target", position(self.target_um));
        self.emit_property(self.axis_device, "velocity", velocity(self.velocity_um_s));
        self.emit_status_properties();
        Ok(())
    }

    fn emit_status_properties(&mut self) {
        self.emit_property(
            self.axis_device,
            "busy",
            Value::Bool(protocol::busy(self.status_bits)),
        );
        self.emit_property(
            self.axis_device,
            "fault_active",
            Value::Bool(protocol::fault_active(self.status_bits)),
        );
        self.emit_property(
            self.axis_device,
            "status_bits",
            Value::I64(self.status_bits as i64),
        );
        self.emit_property(
            self.axis_device,
            "status_summary",
            protocol::status_summary(self.status_bits),
        );
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        vec![
            DeviceDescriptor {
                id: self.hub,
                driver: self.id,
                label: "xeryon-ascii-hub".into(),
                vendor: Some("Xeryon".into()),
                model: Some(self.probe.controller_model.clone()),
                serial: Some(self.probe.serial_number.clone()),
                kinds: vec![
                    "hub".into(),
                    "motion.controller".into(),
                    "serial.ascii".into(),
                    "xeryon.ascii".into(),
                ],
                properties: vec![
                    property(
                        "controller_model",
                        "Controller model",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                    property(
                        "serial_number",
                        "Serial number",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                    property(
                        "software_version",
                        "Software version",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                    property(
                        "state_summary",
                        "State summary",
                        ValueType::Map,
                        None,
                        false,
                        None,
                    ),
                ],
                metadata: BTreeMap::from([
                    ("axis".into(), Value::String(self.probe.axis.to_string())),
                    (
                        "startup_readback_tags".into(),
                        Value::List(
                            protocol::QUERY_TAGS
                                .into_iter()
                                .map(|tag| Value::String(tag.into()))
                                .collect(),
                        ),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.axis_device,
                driver: self.id,
                label: format!("xeryon-ascii-axis-{}", self.probe.axis),
                vendor: Some("Xeryon".into()),
                model: Some(self.probe.stage_model.clone()),
                serial: Some(self.probe.serial_number.clone()),
                kinds: vec![
                    format!("axis.{}", self.probe.axis.to_ascii_lowercase()),
                    "stage.axis".into(),
                    "motion.stage".into(),
                    "xeryon.ascii.axis".into(),
                ],
                properties: vec![
                    sequenceable_position_property_range(
                        "position",
                        "Position",
                        Some("um"),
                        true,
                        self.probe.low_limit_um,
                        self.probe.high_limit_um,
                    ),
                    property_range(
                        "target",
                        "Target",
                        Some("um"),
                        true,
                        self.probe.low_limit_um,
                        self.probe.high_limit_um,
                    ),
                    velocity_property_range(
                        "velocity",
                        "Velocity",
                        Some("um/s"),
                        true,
                        0.0,
                        500_000.0,
                    ),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                    property("indexed", "Indexed", ValueType::Bool, None, false, None),
                    property(
                        "position_reached",
                        "Position reached",
                        ValueType::Bool,
                        None,
                        false,
                        None,
                    ),
                    property(
                        "fault_active",
                        "Fault active",
                        ValueType::Bool,
                        None,
                        false,
                        None,
                    ),
                    property(
                        "status_bits",
                        "Status bits",
                        ValueType::I64,
                        None,
                        false,
                        None,
                    ),
                    property(
                        "status_summary",
                        "Status summary",
                        ValueType::Map,
                        None,
                        false,
                        None,
                    ),
                    position_property("low_limit", "Low limit", Some("um"), false),
                    position_property("high_limit", "High limit", Some("um"), false),
                    position_property("encoder_unit", "Encoder unit", Some("um"), false),
                    property(
                        "axis_summary",
                        "Axis summary",
                        ValueType::Map,
                        None,
                        false,
                        None,
                    ),
                ],
                metadata: BTreeMap::from([
                    ("axis".into(), Value::String(self.probe.axis.to_string())),
                    (
                        "stage_model".into(),
                        Value::String(self.probe.stage_model.clone()),
                    ),
                    ("low_limit".into(), position(self.probe.low_limit_um)),
                    ("high_limit".into(), position(self.probe.high_limit_um)),
                    (
                        "encoder_units_per_um".into(),
                        Value::F64(self.probe.encoder_units_per_um),
                    ),
                    (
                        "encoder_unit".into(),
                        position(1.0 / self.probe.encoder_units_per_um),
                    ),
                    ("probed_position".into(), position(self.probe.position_um)),
                    ("velocity".into(), velocity(self.velocity_um_s)),
                    (
                        "legacy_low_limit_um".into(),
                        position(self.probe.low_limit_um),
                    ),
                    (
                        "legacy_high_limit_um".into(),
                        position(self.probe.high_limit_um),
                    ),
                    (
                        "legacy_position_um".into(),
                        position(self.probe.position_um),
                    ),
                    ("legacy_velocity_um_s".into(), velocity(self.velocity_um_s)),
                ]),
            },
        ]
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        match (device, key) {
            (device, "controller_model") if device == self.hub => {
                Ok(Value::String(self.probe.controller_model.clone()))
            }
            (device, "serial_number") if device == self.hub => {
                Ok(Value::String(self.probe.serial_number.clone()))
            }
            (device, "software_version") if device == self.hub => {
                Ok(Value::String(self.probe.software_version.clone()))
            }
            (device, "state_summary") if device == self.hub => Ok(self.state_summary()),
            (device, "position") if device == self.axis_device => Ok(position(self.position_um)),
            (device, "target") if device == self.axis_device => Ok(position(self.target_um)),
            (device, "velocity") if device == self.axis_device => Ok(velocity(self.velocity_um_s)),
            (device, "busy") if device == self.axis_device => {
                Ok(Value::Bool(protocol::busy(self.status_bits)))
            }
            (device, "indexed") if device == self.axis_device => Ok(Value::Bool(
                protocol::status_flag(self.status_bits, protocol::StatusBits::ENCODER_VALID),
            )),
            (device, "position_reached") if device == self.axis_device => Ok(Value::Bool(
                protocol::status_flag(self.status_bits, protocol::StatusBits::POSITION_REACHED),
            )),
            (device, "fault_active") if device == self.axis_device => {
                Ok(Value::Bool(protocol::fault_active(self.status_bits)))
            }
            (device, "status_bits") if device == self.axis_device => {
                Ok(Value::I64(self.status_bits as i64))
            }
            (device, "status_summary") if device == self.axis_device => {
                Ok(protocol::status_summary(self.status_bits))
            }
            (device, "low_limit") if device == self.axis_device => {
                Ok(position(self.probe.low_limit_um))
            }
            (device, "high_limit") if device == self.axis_device => {
                Ok(position(self.probe.high_limit_um))
            }
            (device, "encoder_unit") if device == self.axis_device => {
                Ok(position(1.0 / self.probe.encoder_units_per_um))
            }
            (device, "axis_summary") if device == self.axis_device => Ok(self.axis_summary()),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Xeryon property {key}"),
            )),
        }
    }

    fn state_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("hub".into(), Value::I64(self.hub.0 .0 as i64)),
            (
                "controller_model".into(),
                Value::String(self.probe.controller_model.clone()),
            ),
            (
                "serial_number".into(),
                Value::String(self.probe.serial_number.clone()),
            ),
            ("axis".into(), self.axis_summary()),
        ]))
    }

    fn axis_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("device".into(), Value::I64(self.axis_device.0 .0 as i64)),
            ("axis".into(), Value::String(self.probe.axis.to_string())),
            (
                "stage_model".into(),
                Value::String(self.probe.stage_model.clone()),
            ),
            ("position".into(), position(self.position_um)),
            ("target".into(), position(self.target_um)),
            ("velocity".into(), velocity(self.velocity_um_s)),
            ("low_limit".into(), position(self.probe.low_limit_um)),
            ("high_limit".into(), position(self.probe.high_limit_um)),
            ("busy".into(), Value::Bool(protocol::busy(self.status_bits))),
            (
                "fault_active".into(),
                Value::Bool(protocol::fault_active(self.status_bits)),
            ),
            (
                "status_summary".into(),
                protocol::status_summary(self.status_bits),
            ),
        ]))
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        let descriptor = self
            .descriptors_for()
            .into_iter()
            .find(|descriptor| descriptor.id == device)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown device"))?;
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
        match (device, key, value) {
            (device, "position", value) if device == self.axis_device => {
                let position_um =
                    position_um(value)?.clamp(self.probe.low_limit_um, self.probe.high_limit_um);
                self.move_absolute(position_um)?;
                Ok(position(self.position_um))
            }
            (device, "target", value) if device == self.axis_device => {
                let target_um =
                    position_um(value)?.clamp(self.probe.low_limit_um, self.probe.high_limit_um);
                self.target_um = target_um;
                Ok(position(self.target_um))
            }
            (device, "velocity", value) if device == self.axis_device => {
                let velocity_um_s = velocity_um_s(value)?.clamp(0.0, 500_000.0);
                self.set_velocity(velocity_um_s)?;
                Ok(velocity(self.velocity_um_s))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid Xeryon write {key}"),
            )),
        }
    }

    fn set_velocity(&mut self, velocity_um_s: f64) -> Result<()> {
        let native = velocity_um_s.round() as i64;
        self.velocity_um_s = native as f64;
        self.probe.velocity_um_s = self.velocity_um_s;
        self.send(protocol::XeryonCommand::Set {
            axis: Some(self.probe.axis),
            tag: "SSPD".into(),
            value: native,
        })
    }

    fn move_absolute(&mut self, position_um: f64) -> Result<()> {
        self.target_um = position_um;
        self.probe.target_um = position_um;
        let native = self.probe.native_position(position_um);
        self.send(protocol::XeryonCommand::Set {
            axis: Some(self.probe.axis),
            tag: "DPOS".into(),
            value: native,
        })?;
        self.finish_motion(position_um)
    }

    fn move_relative(&mut self, distance_um: f64) -> Result<()> {
        let final_position_um = (self.position_um + distance_um)
            .clamp(self.probe.low_limit_um, self.probe.high_limit_um);
        let native = self
            .probe
            .native_position(final_position_um - self.position_um);
        self.target_um = final_position_um;
        self.probe.target_um = final_position_um;
        self.send(protocol::XeryonCommand::Set {
            axis: Some(self.probe.axis),
            tag: "STEP".into(),
            value: native,
        })?;
        self.finish_motion(final_position_um)
    }

    fn finish_motion(&mut self, final_position_um: f64) -> Result<()> {
        for reply in self.read_available_replies()? {
            self.apply_reply(&reply)?;
        }
        if !self.connected {
            self.status_bits |= protocol::StatusBits::MOTOR_ON;
            self.emit_status_properties();
            self.position_um = final_position_um;
            self.probe.position_um = final_position_um;
            self.status_bits &= !protocol::StatusBits::MOTOR_ON;
            self.status_bits |= protocol::StatusBits::POSITION_REACHED;
            self.probe.status_bits = self.status_bits;
            self.emit_property(self.axis_device, "position", position(self.position_um));
            self.emit_status_properties();
        } else {
            self.refresh_tag("EPOS")?;
            self.refresh_tag("STAT")?;
        }
        Ok(())
    }

    fn apply_motion_profile(&mut self, profile: &MotionProfile) -> Result<()> {
        if let Some(profile_velocity) = profile.velocity {
            self.set_velocity(
                profile_velocity
                    .micrometers_per_second()
                    .clamp(0.0, 500_000.0),
            )?;
            self.emit_property(self.axis_device, "velocity", velocity(self.velocity_um_s));
        }
        Ok(())
    }

    fn validate_stage_move(&self, device: DeviceId, request: &StageMoveRequest) -> Result<()> {
        if device != self.axis_device {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Xeryon StageMove targets the axis device",
            ));
        }
        if request.target.len() != 1 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Xeryon StageMove expects exactly one axis target",
            ));
        }
        let Some((axis, _)) = request.target.iter().next() else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Xeryon StageMove target must contain one axis",
            ));
        };
        let axis_name = self.probe.axis.to_ascii_lowercase().to_string();
        let supported_axis = match axis {
            StageAxis::X if self.probe.axis == 'X' => true,
            StageAxis::Y if self.probe.axis == 'Y' => true,
            StageAxis::Z if self.probe.axis == 'Z' => true,
            StageAxis::Custom(name) => name.eq_ignore_ascii_case(&axis_name),
            _ => false,
        };
        if !supported_axis {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                format!("Xeryon StageMove supports only axis {}", self.probe.axis),
            ));
        }
        Ok(())
    }

    fn stage_move(&mut self, request: &StageMoveRequest) -> Result<Value> {
        self.validate_stage_move(self.axis_device, request)?;
        if let Some(profile) = &request.profile {
            self.apply_motion_profile(profile)?;
        }
        let distance_um = request
            .target
            .values()
            .next()
            .expect("validated one target")
            .micrometers();
        if request.relative {
            self.move_relative(distance_um)?;
        } else {
            self.move_absolute(
                distance_um.clamp(self.probe.low_limit_um, self.probe.high_limit_um),
            )?;
        }
        Ok(Value::Map(BTreeMap::from([
            (
                "mode".into(),
                Value::String(if request.relative {
                    "relative".into()
                } else {
                    "absolute".into()
                }),
            ),
            ("position".into(), position(self.position_um)),
            ("target".into(), position(self.target_um)),
            ("velocity".into(), velocity(self.velocity_um_s)),
            (
                "status_summary".into(),
                protocol::status_summary(self.status_bits),
            ),
        ])))
    }

    fn invoke(
        &mut self,
        device: DeviceId,
        capability: CapabilityId,
        request: CapabilityRequest,
    ) -> Result<Value> {
        let Some(capability) = self
            .capabilities(device)
            .into_iter()
            .find(|candidate| candidate.id == capability)
        else {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "unknown Xeryon capability",
            ));
        };
        match (capability.kind, request) {
            (CapabilityKind::StageMove, CapabilityRequest::StageMove(request))
                if device == self.axis_device =>
            {
                self.stage_move(&request)
            }
            (CapabilityKind::StageMove, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Xeryon StageMove expects a StageMoveRequest",
            )),
            (CapabilityKind::StageHome, CapabilityRequest::None) if device == self.axis_device => {
                self.send(protocol::XeryonCommand::NoValue {
                    axis: Some(self.probe.axis),
                    tag: "HOME".into(),
                })?;
                self.finish_motion(0.0)?;
                Ok(Value::String("homed".into()))
            }
            (CapabilityKind::StageStop, CapabilityRequest::None) if device == self.axis_device => {
                self.send(protocol::XeryonCommand::NoValue {
                    axis: Some(self.probe.axis),
                    tag: "STOP".into(),
                })?;
                for reply in self.read_available_replies()? {
                    self.apply_reply(&reply)?;
                }
                if !self.connected {
                    self.status_bits &= !protocol::StatusBits::MOTOR_ON;
                    self.status_bits &= !protocol::StatusBits::SCANNING;
                    self.probe.status_bits = self.status_bits;
                    self.emit_status_properties();
                }
                Ok(Value::String("stopped".into()))
            }
            (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Xeryon home/stop capabilities take no request",
            )),
            (CapabilityKind::GenericCommand, CapabilityRequest::GenericCommand(request))
                if device == self.axis_device =>
            {
                self.apply_generic_command(request)
            }
            (CapabilityKind::GenericCommand, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Xeryon GenericCommand expects a GenericCommandRequest",
            )),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Xeryon capability",
            )),
        }
    }

    fn validate_generic_command(
        &self,
        request: &GenericCommandRequest,
    ) -> Result<Vec<&'static str>> {
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
                "Xeryon refresh commands do not take parameters",
            ));
        }
        let tags = match request.command.as_str() {
            "refresh_readbacks" => vec!["EPOS", "DPOS", "SSPD", "STAT"],
            "refresh_position" => vec!["EPOS"],
            "refresh_target" => vec!["DPOS"],
            "refresh_velocity" => vec!["SSPD"],
            "refresh_status" | "refresh_axis_summary" => vec!["STAT"],
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    format!("unsupported Xeryon refresh command {}", request.command),
                ))
            }
        };
        Ok(tags)
    }

    fn apply_generic_command(&mut self, request: GenericCommandRequest) -> Result<Value> {
        let tags = self.validate_generic_command(&request)?;
        for tag in &tags {
            self.refresh_tag(tag)?;
        }
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            (
                "tags".into(),
                Value::List(
                    tags.into_iter()
                        .map(|tag| Value::String(tag.into()))
                        .collect(),
                ),
            ),
            ("axis_summary".into(), self.axis_summary()),
        ])))
    }

    fn apply_state_set(&mut self, set: StateSet) -> Result<Value> {
        let mut next_position = self.position_um;
        let mut next_target = self.target_um;
        let mut next_velocity = self.velocity_um_s;
        for write in &set.writes {
            self.validate_write(write.device, &write.property, &write.value)?;
            match (write.device, write.property.as_str(), &write.value) {
                (device, "position", value) if device == self.axis_device => {
                    next_position = position_um(value)?
                        .clamp(self.probe.low_limit_um, self.probe.high_limit_um);
                }
                (device, "target", value) if device == self.axis_device => {
                    next_target = position_um(value)?
                        .clamp(self.probe.low_limit_um, self.probe.high_limit_um);
                }
                (device, "velocity", value) if device == self.axis_device => {
                    next_velocity = velocity_um_s(value)?.clamp(0.0, 500_000.0);
                }
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "unsupported Xeryon state-set write",
                    ))
                }
            }
        }
        let mut changed = BTreeMap::new();
        if next_velocity != self.velocity_um_s {
            self.set_velocity(next_velocity)?;
            changed.insert(
                format!("{}:velocity", (self.axis_device.0).0),
                velocity(self.velocity_um_s),
            );
            self.emit_property(self.axis_device, "velocity", velocity(self.velocity_um_s));
        }
        if next_target != self.target_um {
            self.target_um = next_target;
            changed.insert(
                format!("{}:target", (self.axis_device.0).0),
                position(next_target),
            );
            self.emit_property(self.axis_device, "target", position(next_target));
        }
        if next_position != self.position_um {
            self.move_absolute(next_position)?;
            changed.insert(
                format!("{}:position", (self.axis_device.0).0),
                position(self.position_um),
            );
        }
        Ok(Value::Map(changed))
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in plan
            .sequences
            .iter()
            .filter(|sequence| sequence.device == self.axis_device)
        {
            if sequence.property != "position" {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Xeryon timing sequences can only target position",
                ));
            }
            for value in &sequence.values {
                let _ = position_um(value)?;
            }
        }
        Ok(())
    }

    fn apply_timing_sequence_step(&mut self, plan: &TimingPlan, first: bool) -> Result<Value> {
        let mut writes = Vec::new();
        for sequence in plan
            .sequences
            .iter()
            .filter(|sequence| sequence.device == self.axis_device)
        {
            let value = if first {
                sequence.values.first()
            } else {
                sequence.values.last()
            };
            if let Some(value) = value {
                writes.push(StateWrite {
                    device: sequence.device,
                    property: sequence.property.clone(),
                    value: value.clone(),
                });
            }
        }
        if writes.is_empty() {
            return Ok(Value::Map(BTreeMap::new()));
        }
        self.apply_state_set(StateSet {
            name: Some(if first {
                "xeryon timing start sequence".into()
            } else {
                "xeryon timing stop sequence".into()
            }),
            writes,
            commit: CommitMode::Immediate,
        })
    }

    fn emit_property(&mut self, device: DeviceId, key: &str, value: Value) {
        self.pending
            .push_back(DriverEvent::Event(Event::PropertyChanged(
                PropertyChanged {
                    device,
                    key: key.into(),
                    value,
                },
            )));
    }
}

impl Driver for XeryonDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        self.descriptors_for()
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "xeryon-ascii-serial".into(),
            kind: "serial".into(),
            metadata: BTreeMap::from([
                ("baud_rate".into(), Value::I64(self.baud_rate as i64)),
                (
                    "serial_port".into(),
                    self.serial_port
                        .as_ref()
                        .map(|port| Value::String(port.clone()))
                        .unwrap_or(Value::Null),
                ),
                (
                    "serial_timeout".into(),
                    Value::TimeInterval(TimeInterval::from_milliseconds(
                        self.serial_timeout_ms as f64,
                    )),
                ),
                ("connected".into(), Value::Bool(self.connected)),
                ("terminator".into(), Value::String("LF".into())),
                ("protocol".into(), Value::String("xeryon.ascii".into())),
            ]),
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.axis_device {
            vec![
                capability(1, device, CapabilityKind::StageMove),
                capability(2, device, CapabilityKind::StageHome),
                capability(3, device, CapabilityKind::StageStop),
                capability(4, device, CapabilityKind::GenericCommand),
            ]
        } else {
            Vec::new()
        }
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut physical_transactions = Vec::new();
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    let _ = self.read_property(*device, key)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("xeryon read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("xeryon write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "xeryon remultiplexed axis state set".into(),
                        payload: Value::List(
                            set.writes
                                .iter()
                                .map(|write| {
                                    Value::Map(BTreeMap::from([
                                        ("device".into(), Value::I64((write.device.0).0 as i64)),
                                        ("property".into(), Value::String(write.property.clone())),
                                        ("value".into(), write.value.clone()),
                                    ]))
                                })
                                .collect(),
                        ),
                    });
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    let candidate = self
                        .capabilities(*device)
                        .into_iter()
                        .find(|candidate| candidate.id == *capability)
                        .ok_or_else(|| {
                            Error::new(ErrorCode::Unsupported, "unknown Xeryon capability")
                        })?;
                    match (&candidate.kind, request) {
                        (CapabilityKind::StageMove, CapabilityRequest::StageMove(request)) => {
                            self.validate_stage_move(*device, request)?;
                        }
                        (
                            CapabilityKind::GenericCommand,
                            CapabilityRequest::GenericCommand(request),
                        ) => {
                            let _ = self.validate_generic_command(request)?;
                        }
                        (
                            CapabilityKind::StageHome | CapabilityKind::StageStop,
                            CapabilityRequest::None,
                        ) => {}
                        (CapabilityKind::StageMove, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Xeryon StageMove expects a StageMoveRequest",
                            ));
                        }
                        (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Xeryon home/stop capabilities take no request",
                            ));
                        }
                        (CapabilityKind::GenericCommand, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Xeryon GenericCommand expects a GenericCommandRequest",
                            ));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported Xeryon capability",
                            ));
                        }
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("xeryon invoke {}", capability.0),
                        payload: Value::Null,
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
            match command {
                Command::ReadProperty { device, key } => {
                    if let Some(tag) = self.query_for_property(device, &key) {
                        self.refresh_tag(tag)?;
                    }
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
                    last = self.invoke(device, capability, request)?;
                }
                Command::Arm(_) | Command::Start(_) | Command::Stop(_) => unreachable!(),
            }
        }
        self.pending
            .push_back(DriverEvent::TokenCompleted { token, value: last });
        Ok(token)
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        if let Ok(replies) = self.read_available_replies() {
            for reply in replies {
                let _ = self.apply_reply(&reply);
            }
        }
        self.pending.drain(..).collect()
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
                description: "xeryon timing arm summary".into(),
                payload: self.axis_summary(),
            }],
        })
    }

    fn start_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let changed = self.apply_timing_sequence_step(&armed.plan, true)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Start(OperationId(0))],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "xeryon timing start sequence".into(),
                payload: changed,
            }],
        })
    }

    fn stop_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let changed = self.apply_timing_sequence_step(&armed.plan, false)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "xeryon timing stop sequence".into(),
                payload: changed,
            }],
        })
    }
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

fn f64_prop(device: &DeviceConfig, key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn u32_prop(device: &DeviceConfig, key: &str) -> Option<u32> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => (*value).try_into().ok(),
        _ => None,
    }
}

fn u64_prop(device: &DeviceConfig, key: &str) -> Option<u64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => (*value).try_into().ok(),
        _ => None,
    }
}

fn axis_prop(device: &DeviceConfig, key: &str) -> Option<char> {
    string_prop(device, key).and_then(|value| {
        let mut chars = value.chars();
        let axis = chars.next()?.to_ascii_uppercase();
        matches!(axis, 'X' | 'Y' | 'Z' | 'A' | 'B' | 'C').then_some(axis)
    })
}

fn position_config_um(device: &DeviceConfig, key: &str, legacy_key: &str) -> Option<f64> {
    match device
        .properties
        .get(key)
        .or_else(|| device.properties.get(legacy_key))
    {
        Some(Value::Position(value)) => Some(value.micrometers()),
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn velocity_config_um_s(device: &DeviceConfig, key: &str, legacy_key: &str) -> Option<f64> {
    match device
        .properties
        .get(key)
        .or_else(|| device.properties.get(legacy_key))
    {
        Some(Value::Velocity(value)) => Some(value.micrometers_per_second()),
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn capability(id: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    CapabilityDescriptor::new(CapabilityId(id), device, kind, ValueType::Map)
}

fn property(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    unit: Option<&str>,
    writable: bool,
    range: Option<Range>,
) -> PropertySchema {
    PropertySchema {
        key: key.into(),
        display_name: display_name.into(),
        value_type,
        unit: unit.map(|unit| Unit(unit.into())),
        range,
        increment: None,
        enum_values: Vec::new(),
        readable: true,
        writable,
        volatile: false,
        sequenceable: false,
        hardware_address: None,
    }
}

fn property_range(
    key: &str,
    display_name: &str,
    unit: Option<&str>,
    writable: bool,
    min: f64,
    max: f64,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Position,
        unit,
        writable,
        Some(Range {
            min: position(min),
            max: position(max),
        }),
    )
}

fn position_property(
    key: &str,
    display_name: &str,
    unit: Option<&str>,
    writable: bool,
) -> PropertySchema {
    property(key, display_name, ValueType::Position, unit, writable, None)
}

fn sequenceable_position_property_range(
    key: &str,
    display_name: &str,
    unit: Option<&str>,
    writable: bool,
    min: f64,
    max: f64,
) -> PropertySchema {
    let mut schema = property_range(key, display_name, unit, writable, min, max);
    schema.sequenceable = true;
    schema
}

fn velocity_property_range(
    key: &str,
    display_name: &str,
    unit: Option<&str>,
    writable: bool,
    min_um_s: f64,
    max_um_s: f64,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Velocity,
        unit,
        writable,
        Some(Range {
            min: velocity(min_um_s),
            max: velocity(max_um_s),
        }),
    )
}

fn position(value_um: f64) -> Value {
    Value::Position(Position::from_micrometers(value_um))
}

fn velocity(value_um_s: f64) -> Value {
    Value::Velocity(Velocity::from_micrometers_per_second(value_um_s))
}

fn position_um(value: &Value) -> Result<f64> {
    match value {
        Value::Position(value) => Ok(value.micrometers()),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("expected Position value, got {:?}", other.value_type()),
        )),
    }
}

fn velocity_um_s(value: &Value) -> Result<f64> {
    match value {
        Value::Velocity(value) => Ok(value.micrometers_per_second()),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("expected Velocity value, got {:?}", other.value_type()),
        )),
    }
}
