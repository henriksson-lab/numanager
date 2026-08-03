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
    pub const SEND_ENDING: LineEnding = LineEnding::CrLf;
    pub const RECV_ENDING: LineEnding = LineEnding::CrLf;
    pub const PROBE_SETTINGS: [&str; 8] = [
        "device.id",
        "system.serial",
        "peripheral.id",
        "limit.max",
        "resolution",
        "pos",
        "maxspeed",
        "accel",
    ];

    #[derive(Debug, Clone, PartialEq)]
    pub struct ZaberAsciiProbe {
        pub address: u8,
        pub axis: u8,
        pub device_id: String,
        pub peripheral_id: String,
        pub serial_number: String,
        pub travel_um: f64,
        pub microstep_size_um: f64,
        pub position_um: f64,
        pub velocity_um_s: f64,
        pub acceleration_um_s2: f64,
        pub status: String,
        pub warning: Option<String>,
    }

    impl ZaberAsciiProbe {
        pub fn simulated() -> Self {
            Self {
                address: 1,
                axis: 1,
                device_id: "sim-x-series-linear-stage".into(),
                peripheral_id: "sim-axis".into(),
                serial_number: "ZABER-SIM-0001".into(),
                travel_um: 50_000.0,
                microstep_size_um: 0.099_218_75,
                position_um: 0.0,
                velocity_um_s: 5_000.0,
                acceleration_um_s2: 50_000.0,
                status: "IDLE".into(),
                warning: None,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ZaberReply {
        pub address: u8,
        pub axis: u8,
        pub ok: bool,
        pub status: String,
        pub warning: Option<String>,
        pub data: String,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ZaberWarningKind {
        None,
        Recoverable,
        LimitOrSafety,
        CommandOrData,
        Unknown,
    }

    impl ZaberWarningKind {
        pub fn name(self) -> &'static str {
            match self {
                ZaberWarningKind::None => "none",
                ZaberWarningKind::Recoverable => "recoverable",
                ZaberWarningKind::LimitOrSafety => "limit_or_safety",
                ZaberWarningKind::CommandOrData => "command_or_data",
                ZaberWarningKind::Unknown => "unknown",
            }
        }

        pub fn severity(self) -> &'static str {
            match self {
                ZaberWarningKind::None => "none",
                ZaberWarningKind::Recoverable => "warning",
                ZaberWarningKind::LimitOrSafety => "fault",
                ZaberWarningKind::CommandOrData => "error",
                ZaberWarningKind::Unknown => "warning",
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ZaberProbeScript {
        pub address: u8,
        pub axis: u8,
        pub commands: Vec<String>,
    }

    impl ZaberProbeScript {
        pub fn for_axis(address: u8, axis: u8) -> Self {
            Self {
                address,
                axis,
                commands: PROBE_SETTINGS
                    .into_iter()
                    .map(|setting| {
                        encode(
                            address,
                            axis,
                            &ZaberCommand::Get {
                                setting: setting.into(),
                            },
                            1.0,
                        )
                    })
                    .collect(),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum ZaberCommand {
        Get { setting: String },
        Set { setting: String, value: String },
        SetNative { setting: String, value: i64 },
        MoveAbsolute { position_um: f64 },
        MoveRelative { distance_um: f64 },
        Home,
        Stop,
    }

    pub fn encode(address: u8, axis: u8, command: &ZaberCommand, microstep_size_um: f64) -> String {
        match command {
            ZaberCommand::Get { setting } => format!("/{address} {axis} get {setting}"),
            ZaberCommand::Set { setting, value } => {
                format!("/{address} {axis} set {setting} {value}")
            }
            ZaberCommand::SetNative { setting, value } => {
                format!("/{address} {axis} set {setting} {value}")
            }
            ZaberCommand::MoveAbsolute { position_um } => format!(
                "/{address} {axis} move abs {}",
                native_units(*position_um, microstep_size_um)
            ),
            ZaberCommand::MoveRelative { distance_um } => format!(
                "/{address} {axis} move rel {}",
                native_units(*distance_um, microstep_size_um)
            ),
            ZaberCommand::Home => format!("/{address} {axis} home"),
            ZaberCommand::Stop => format!("/{address} {axis} stop"),
        }
    }

    pub fn native_units(um: f64, microstep_size_um: f64) -> i64 {
        (um / microstep_size_um).round() as i64
    }

    pub fn micrometers(native_units: i64, microstep_size_um: f64) -> f64 {
        native_units as f64 * microstep_size_um
    }

    pub fn native_velocity(um_s: f64, microstep_size_um: f64) -> i64 {
        native_units(um_s, microstep_size_um).max(0)
    }

    pub fn native_acceleration(um_s2: f64, microstep_size_um: f64) -> i64 {
        native_units(um_s2, microstep_size_um).max(0)
    }

    pub fn is_idle_status(reply: &str) -> Result<bool> {
        let status = reply
            .split_whitespace()
            .nth(4)
            .ok_or_else(|| Error::new(ErrorCode::Transport, "missing Zaber status field"))?;
        match status {
            "IDLE" => Ok(true),
            "BUSY" => Ok(false),
            other => Err(Error::new(
                ErrorCode::Transport,
                format!("unknown Zaber status field {other}"),
            )),
        }
    }

    pub fn parse_reply_data(reply: &str) -> Result<String> {
        Ok(parse_reply(reply)?.data)
    }

    pub fn classify_warning(warning: Option<&str>) -> ZaberWarningKind {
        match warning {
            None | Some("--") => ZaberWarningKind::None,
            Some("WR") | Some("WV") | Some("WT") | Some("WP") => ZaberWarningKind::Recoverable,
            Some("WL") | Some("WM") | Some("WH") | Some("WS") => ZaberWarningKind::LimitOrSafety,
            Some("BADCOMMAND") | Some("BADDATA") | Some("RJ") => ZaberWarningKind::CommandOrData,
            Some(_) => ZaberWarningKind::Unknown,
        }
    }

    pub fn warning_summary(warning: Option<&str>) -> Value {
        let kind = classify_warning(warning);
        let raw = warning.unwrap_or("--");
        Value::Map(BTreeMap::from([
            ("raw".into(), Value::String(raw.into())),
            ("kind".into(), Value::String(kind.name().into())),
            ("severity".into(), Value::String(kind.severity().into())),
            (
                "description".into(),
                Value::String(warning_description(raw, kind).into()),
            ),
        ]))
    }

    fn warning_description(raw: &str, kind: ZaberWarningKind) -> &'static str {
        match raw {
            "--" => "no warning reported",
            "WR" => "warning flag reported by controller",
            "WV" => "value was adjusted or constrained by controller",
            "WT" => "temperature warning reported by controller",
            "WP" => "power or supply warning reported by controller",
            "WL" => "limit warning reported by controller",
            "WM" => "motion warning reported by controller",
            "WH" => "homing or reference warning reported by controller",
            "WS" => "safety or interlock warning reported by controller",
            "BADCOMMAND" => "command token was rejected",
            "BADDATA" => "command data was rejected",
            "RJ" => "command was rejected by controller",
            _ if kind == ZaberWarningKind::Unknown => "unclassified Zaber warning token",
            _ => "Zaber warning token",
        }
    }

    pub fn execute_probe_script(
        address: u8,
        axis: u8,
        serial: &mut dyn SerialIo,
        polls_per_command: usize,
    ) -> Result<ZaberAsciiProbe> {
        let mut codec = SerialLineCodec::new(SEND_ENDING, RECV_ENDING);
        let mut replies = Vec::new();
        for setting in PROBE_SETTINGS {
            let command = encode(
                address,
                axis,
                &ZaberCommand::Get {
                    setting: setting.into(),
                },
                1.0,
            );
            serial.write(&codec.encode(&command))?;
            let mut reply = None;
            for _ in 0..polls_per_command.max(1) {
                let bytes = serial.read_available()?;
                for line in codec.push(&bytes) {
                    reply = Some(line);
                    break;
                }
                if reply.is_some() {
                    break;
                }
            }
            let reply = reply.ok_or_else(|| {
                Error::new(
                    ErrorCode::Transport,
                    format!("timed out waiting for Zaber probe reply to {setting}"),
                )
            })?;
            replies.push((setting.to_string(), reply));
        }
        probe_from_replies(address, axis, &replies)
    }

    pub fn parse_reply(reply: &str) -> Result<ZaberReply> {
        let mut parts = reply.split_whitespace();
        let prefix = parts
            .next()
            .ok_or_else(|| Error::new(ErrorCode::Transport, "empty Zaber reply"))?;
        if prefix != "@" {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("unexpected Zaber reply prefix {prefix}"),
            ));
        }
        let address = parts
            .next()
            .ok_or_else(|| Error::new(ErrorCode::Transport, "missing Zaber address"))?
            .parse::<u8>()
            .map_err(|error| Error::new(ErrorCode::Transport, error.to_string()))?;
        let axis = parts
            .next()
            .ok_or_else(|| Error::new(ErrorCode::Transport, "missing Zaber axis"))?
            .parse::<u8>()
            .map_err(|error| Error::new(ErrorCode::Transport, error.to_string()))?;
        let response = parts
            .next()
            .ok_or_else(|| Error::new(ErrorCode::Transport, "missing Zaber response flag"))?;
        let status = parts
            .next()
            .ok_or_else(|| Error::new(ErrorCode::Transport, "missing Zaber status"))?
            .to_string();
        let warning = parts
            .next()
            .ok_or_else(|| Error::new(ErrorCode::Transport, "missing Zaber warning flag"))?;
        Ok(ZaberReply {
            address,
            axis,
            ok: response == "OK",
            status,
            warning: (warning != "--").then(|| warning.to_string()),
            data: parts.collect::<Vec<_>>().join(" "),
        })
    }

    pub fn probe_from_replies(
        address: u8,
        axis: u8,
        replies: &[(impl AsRef<str>, impl AsRef<str>)],
    ) -> Result<ZaberAsciiProbe> {
        let mut probe = ZaberAsciiProbe::simulated();
        probe.address = address;
        probe.axis = axis;
        let mut limit_max_native = None;
        let mut position_native = None;
        let mut maxspeed_native = None;
        let mut accel_native = None;
        for (setting, reply) in replies {
            let parsed = parse_reply(reply.as_ref())?;
            if !parsed.ok {
                return Err(Error::new(
                    ErrorCode::Transport,
                    format!("Zaber probe command failed for {}", setting.as_ref()),
                ));
            }
            if parsed.address != address || parsed.axis != axis {
                return Err(Error::new(
                    ErrorCode::Transport,
                    "Zaber probe reply address/axis did not match request",
                ));
            }
            probe.status = parsed.status.clone();
            if parsed.warning.is_some() {
                probe.warning = parsed.warning.clone();
            }
            match setting.as_ref() {
                "device.id" => probe.device_id = parsed.data,
                "system.serial" => probe.serial_number = parsed.data,
                "peripheral.id" => probe.peripheral_id = parsed.data,
                "limit.max" => limit_max_native = Some(parse_i64_data("limit.max", &parsed.data)?),
                "resolution" => {
                    let native_per_um = parse_f64_data("resolution", &parsed.data)?;
                    if native_per_um > 0.0 {
                        probe.microstep_size_um = 1.0 / native_per_um;
                    }
                }
                "pos" => position_native = Some(parse_i64_data("pos", &parsed.data)?),
                "maxspeed" => maxspeed_native = Some(parse_i64_data("maxspeed", &parsed.data)?),
                "accel" => accel_native = Some(parse_i64_data("accel", &parsed.data)?),
                _ => {}
            }
        }
        if let Some(value) = limit_max_native {
            probe.travel_um = micrometers(value, probe.microstep_size_um);
        }
        if let Some(value) = position_native {
            probe.position_um = micrometers(value, probe.microstep_size_um);
        }
        if let Some(value) = maxspeed_native {
            probe.velocity_um_s = micrometers(value, probe.microstep_size_um);
        }
        if let Some(value) = accel_native {
            probe.acceleration_um_s2 = micrometers(value, probe.microstep_size_um);
        }
        Ok(probe)
    }

    pub(crate) fn parse_i64_data(setting: &str, value: &str) -> Result<i64> {
        value.parse::<i64>().map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("invalid Zaber {setting} integer {value}: {error}"),
            )
        })
    }

    pub(crate) fn parse_f64_data(setting: &str, value: &str) -> Result<f64> {
        value.parse::<f64>().map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("invalid Zaber {setting} float {value}: {error}"),
            )
        })
    }
}

pub struct ZaberAsciiDiscovery {
    next_id: DriverId,
    probes: Vec<ZaberConfiguredProbe>,
}

impl ZaberAsciiDiscovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![ZaberConfiguredProbe::simulated()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| device.driver == "zaber-ascii")
            .map(ZaberConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }

    pub fn from_multi_axis_probe(next_id: DriverId, probe: ZaberMultiAxisProbe) -> Self {
        Self {
            next_id,
            probes: probe.into_configured_probes(),
        }
    }
}

impl DriverDiscovery for ZaberAsciiDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = probe.label.clone();
                let driver = if probe.connect_real_transport {
                    Box::new(ZaberAsciiDriver::serial(id, probe)?) as Box<dyn Driver>
                } else {
                    Box::new(ZaberAsciiDriver::configured(id, probe)) as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct ZaberConfiguredProbe {
    pub label: String,
    pub probe: protocol::ZaberAsciiProbe,
    pub endpoint: Option<ZaberSerialEndpoint>,
    pub connect_real_transport: bool,
    pub startup_readback: bool,
}

#[derive(Debug, Clone)]
pub struct ZaberMultiAxisProbe {
    pub label: String,
    pub serial_endpoint: Option<ZaberSerialEndpoint>,
    pub connect_real_transport: bool,
    pub axes: Vec<protocol::ZaberAsciiProbe>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZaberSerialEndpoint {
    pub port_name: String,
    pub baud_rate: u32,
    pub timeout_ms: u64,
}

impl ZaberConfiguredProbe {
    pub fn simulated() -> Self {
        Self {
            label: "Simulated Zaber ASCII stage".into(),
            probe: protocol::ZaberAsciiProbe::simulated(),
            endpoint: None,
            connect_real_transport: false,
            startup_readback: false,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = protocol::ZaberAsciiProbe::simulated();
        probe.address = u8_prop(device, "address").unwrap_or(probe.address);
        probe.axis = u8_prop(device, "axis").unwrap_or(probe.axis);
        probe.device_id = string_prop(device, "device_id").unwrap_or(probe.device_id);
        probe.peripheral_id = string_prop(device, "peripheral_id").unwrap_or(probe.peripheral_id);
        probe.serial_number = string_prop(device, "serial_number").unwrap_or(probe.serial_number);
        probe.travel_um =
            position_config_um(device, "travel", "travel_um").unwrap_or(probe.travel_um);
        probe.microstep_size_um = position_config_um(device, "microstep_size", "microstep_size_um")
            .unwrap_or(probe.microstep_size_um);
        probe.position_um =
            position_config_um(device, "position", "position_um").unwrap_or(probe.position_um);
        probe.velocity_um_s = velocity_config_um_s(device, "velocity", "velocity_um_s")
            .unwrap_or(probe.velocity_um_s);
        probe.acceleration_um_s2 =
            acceleration_config_um_s2(device, "acceleration", "acceleration_um_s2")
                .unwrap_or(probe.acceleration_um_s2);
        probe.status = string_prop(device, "status").unwrap_or(probe.status);
        probe.warning = string_prop(device, "warning");

        let endpoint = string_prop(device, "serial_port").map(|port_name| ZaberSerialEndpoint {
            port_name,
            baud_rate: u32_prop(device, "baud_rate").unwrap_or(protocol::BAUD),
            timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(1),
        });

        Ok(Self {
            label: if device.label.is_empty() {
                "Configured Zaber ASCII stage".into()
            } else {
                device.label.clone()
            },
            probe,
            endpoint,
            connect_real_transport: bool_prop(device, "connect").unwrap_or(false),
            startup_readback: bool_prop(device, "startup_readback")
                .or_else(|| bool_prop(device, "active_probe"))
                .unwrap_or(false),
        })
    }
}

impl ZaberMultiAxisProbe {
    pub fn from_probed_axes(
        label: impl Into<String>,
        axes: Vec<protocol::ZaberAsciiProbe>,
        endpoint: Option<ZaberSerialEndpoint>,
        connect_real_transport: bool,
    ) -> Result<Self> {
        if axes.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Zaber multi-axis probe requires at least one axis",
            ));
        }
        let mut seen = BTreeMap::new();
        for axis in &axes {
            let key = (axis.address, axis.axis);
            if seen.insert(key, true).is_some() {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!(
                        "duplicate Zaber probe for address {} axis {}",
                        axis.address, axis.axis
                    ),
                ));
            }
        }
        Ok(Self {
            label: label.into(),
            serial_endpoint: endpoint,
            connect_real_transport,
            axes,
        })
    }

    pub fn into_configured_probes(self) -> Vec<ZaberConfiguredProbe> {
        self.axes
            .into_iter()
            .map(|probe| ZaberConfiguredProbe {
                label: format!(
                    "{} address {} axis {}",
                    self.label, probe.address, probe.axis
                ),
                probe,
                endpoint: self.serial_endpoint.clone(),
                connect_real_transport: self.connect_real_transport,
                startup_readback: false,
            })
            .collect()
    }

    pub fn discovery_metadata(&self) -> Value {
        Value::List(
            self.axes
                .iter()
                .map(|probe| {
                    Value::Map(BTreeMap::from([
                        ("address".into(), Value::I64(probe.address as i64)),
                        ("axis".into(), Value::I64(probe.axis as i64)),
                        ("device_id".into(), Value::String(probe.device_id.clone())),
                        (
                            "peripheral_id".into(),
                            Value::String(probe.peripheral_id.clone()),
                        ),
                        (
                            "serial_number".into(),
                            Value::String(probe.serial_number.clone()),
                        ),
                        ("travel".into(), position(probe.travel_um)),
                        ("microstep_size".into(), position(probe.microstep_size_um)),
                        ("position".into(), position(probe.position_um)),
                        ("velocity".into(), velocity(probe.velocity_um_s)),
                        (
                            "acceleration".into(),
                            acceleration(probe.acceleration_um_s2),
                        ),
                        ("legacy_travel_um".into(), position(probe.travel_um)),
                        (
                            "legacy_microstep_size_um".into(),
                            position(probe.microstep_size_um),
                        ),
                        ("legacy_position_um".into(), position(probe.position_um)),
                        ("legacy_velocity_um_s".into(), velocity(probe.velocity_um_s)),
                        (
                            "legacy_acceleration_um_s2".into(),
                            acceleration(probe.acceleration_um_s2),
                        ),
                        ("status".into(), Value::String(probe.status.clone())),
                        (
                            "warning".into(),
                            probe
                                .warning
                                .as_ref()
                                .map(|warning| Value::String(warning.clone()))
                                .unwrap_or(Value::Null),
                        ),
                        (
                            "warning_summary".into(),
                            protocol::warning_summary(probe.warning.as_deref()),
                        ),
                    ]))
                })
                .collect(),
        )
    }
}

pub struct ZaberAsciiDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    axis: DeviceId,
    probe: protocol::ZaberAsciiProbe,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
    position_um: f64,
    target_um: f64,
    velocity_um_s: f64,
    acceleration_um_s2: f64,
    busy: bool,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
}

impl ZaberAsciiDriver {
    pub fn simulated(id: DriverId) -> Self {
        Self::configured(id, ZaberConfiguredProbe::simulated())
    }

    pub fn configured(id: DriverId, configured: ZaberConfiguredProbe) -> Self {
        let serial = ScriptedSerial::new();
        Self::new_with_transport_metadata(
            id,
            configured.probe,
            configured.endpoint,
            false,
            Box::new(serial),
        )
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: ZaberConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Zaber ASCII serial probe is missing serial_port metadata",
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
                configured.probe.address,
                configured.probe.axis,
                &mut serial,
                32,
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
    pub fn serial(_id: DriverId, _configured: ZaberConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Zaber ASCII real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(id: DriverId, probe: protocol::ZaberAsciiProbe, serial: Box<dyn SerialIo>) -> Self {
        Self::new_with_transport_metadata(id, probe, None, false, serial)
    }

    fn new_with_transport_metadata(
        id: DriverId,
        probe: protocol::ZaberAsciiProbe,
        endpoint: Option<ZaberSerialEndpoint>,
        connected: bool,
        serial: Box<dyn SerialIo>,
    ) -> Self {
        let initial_position_um = probe.position_um;
        let velocity_um_s = probe.velocity_um_s;
        let acceleration_um_s2 = probe.acceleration_um_s2;
        let initial_busy = probe.status == "BUSY";
        let serial_port = endpoint.as_ref().map(|endpoint| endpoint.port_name.clone());
        let baud_rate = endpoint
            .as_ref()
            .map(|endpoint| endpoint.baud_rate)
            .unwrap_or(protocol::BAUD);
        let serial_timeout_ms = endpoint
            .as_ref()
            .map(|endpoint| endpoint.timeout_ms)
            .unwrap_or(1);
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 901)),
            hub: DeviceId(NodeId(id.0 * 1000 + 910)),
            axis: DeviceId(NodeId(id.0 * 1000 + 911)),
            probe,
            serial_port,
            baud_rate,
            serial_timeout_ms,
            connected,
            position_um: initial_position_um,
            target_um: initial_position_um,
            velocity_um_s,
            acceleration_um_s2,
            busy: initial_busy,
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

    fn send(&mut self, command: protocol::ZaberCommand) -> Result<()> {
        let line = protocol::encode(
            self.probe.address,
            self.probe.axis,
            &command,
            self.probe.microstep_size_um,
        );
        self.serial.write(&self.codec.encode(&line))
    }

    fn query_for_property(&self, device: DeviceId, key: &str) -> Option<protocol::ZaberCommand> {
        let setting = match (device, key) {
            (device, "device_id") if device == self.hub => "device.id",
            (device, "serial_number") if device == self.hub => "system.serial",
            (device, "state_summary") if device == self.hub => "pos",
            (
                device,
                "position" | "busy" | "status" | "warning" | "warning_summary" | "axis_summary",
            ) if device == self.axis => "pos",
            (device, "velocity") if device == self.axis => "maxspeed",
            (device, "acceleration") if device == self.axis => "accel",
            _ => return None,
        };
        Some(protocol::ZaberCommand::Get {
            setting: setting.into(),
        })
    }

    fn read_query_reply(&mut self, command: &protocol::ZaberCommand) -> Result<()> {
        let bytes = self.serial.read_available()?;
        if bytes.is_empty() {
            return Ok(());
        }
        for line in self.codec.push(&bytes) {
            self.apply_readback_reply(command, &line)?;
        }
        Ok(())
    }

    fn refresh_position_readback(&mut self) -> Result<()> {
        let command = protocol::ZaberCommand::Get {
            setting: "pos".into(),
        };
        self.send(command.clone())?;
        self.read_query_reply(&command)
    }

    fn read_command_reply(&mut self, action: &str) -> Result<bool> {
        let bytes = self.serial.read_available()?;
        if bytes.is_empty() {
            return Ok(false);
        }
        let mut saw_reply = false;
        for line in self.codec.push(&bytes) {
            self.apply_command_reply(action, &line)?;
            saw_reply = true;
        }
        Ok(saw_reply)
    }

    fn apply_command_reply(&mut self, action: &str, reply: &str) -> Result<()> {
        let parsed = protocol::parse_reply(reply)?;
        if !parsed.ok {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("Zaber {action} failed: {reply}"),
            ));
        }
        if parsed.address != self.probe.address || parsed.axis != self.probe.axis {
            return Err(Error::new(
                ErrorCode::Transport,
                "Zaber command reply address/axis did not match request",
            ));
        }
        self.probe.status = parsed.status;
        self.probe.warning = parsed.warning;
        self.busy = self.probe.status == "BUSY";
        self.emit_property(self.axis, "busy", Value::Bool(self.busy));
        self.emit_property(
            self.axis,
            "status",
            Value::String(self.probe.status.clone()),
        );
        self.emit_property(
            self.axis,
            "warning",
            Value::String(self.probe.warning.clone().unwrap_or_else(|| "--".into())),
        );
        self.emit_property(
            self.axis,
            "warning_summary",
            protocol::warning_summary(self.probe.warning.as_deref()),
        );
        Ok(())
    }

    fn apply_readback_reply(
        &mut self,
        command: &protocol::ZaberCommand,
        reply: &str,
    ) -> Result<()> {
        let protocol::ZaberCommand::Get { setting } = command else {
            return Ok(());
        };
        let parsed = protocol::parse_reply(reply)?;
        if !parsed.ok {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("Zaber get {setting} failed: {reply}"),
            ));
        }
        if parsed.address != self.probe.address || parsed.axis != self.probe.axis {
            return Err(Error::new(
                ErrorCode::Transport,
                "Zaber readback reply address/axis did not match request",
            ));
        }
        self.probe.status = parsed.status;
        self.probe.warning = parsed.warning;
        self.busy = self.probe.status == "BUSY";

        match setting.as_str() {
            "device.id" => {
                self.probe.device_id = parsed.data;
                self.emit_property(
                    self.hub,
                    "device_id",
                    Value::String(self.probe.device_id.clone()),
                );
            }
            "system.serial" => {
                self.probe.serial_number = parsed.data;
                self.emit_property(
                    self.hub,
                    "serial_number",
                    Value::String(self.probe.serial_number.clone()),
                );
            }
            "pos" => {
                let native = protocol::parse_i64_data("pos", &parsed.data)?;
                self.position_um = protocol::micrometers(native, self.probe.microstep_size_um);
                self.emit_property(self.axis, "position", position(self.position_um));
            }
            "maxspeed" => {
                let native = protocol::parse_i64_data("maxspeed", &parsed.data)?;
                self.velocity_um_s = protocol::micrometers(native, self.probe.microstep_size_um);
                self.emit_property(self.axis, "velocity", velocity(self.velocity_um_s));
            }
            "accel" => {
                let native = protocol::parse_i64_data("accel", &parsed.data)?;
                self.acceleration_um_s2 =
                    protocol::micrometers(native, self.probe.microstep_size_um);
                self.emit_property(
                    self.axis,
                    "acceleration",
                    acceleration(self.acceleration_um_s2),
                );
            }
            _ => {}
        }
        self.emit_property(self.axis, "busy", Value::Bool(self.busy));
        self.emit_property(
            self.axis,
            "status",
            Value::String(self.probe.status.clone()),
        );
        self.emit_property(
            self.axis,
            "warning",
            Value::String(self.probe.warning.clone().unwrap_or_else(|| "--".into())),
        );
        self.emit_property(
            self.axis,
            "warning_summary",
            protocol::warning_summary(self.probe.warning.as_deref()),
        );
        self.emit_property(self.axis, "axis_summary", self.axis_summary());
        self.emit_property(self.hub, "state_summary", self.state_summary());
        Ok(())
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        vec![
            DeviceDescriptor {
                id: self.hub,
                driver: self.id,
                label: "zaber-ascii-hub".into(),
                vendor: Some("Zaber".into()),
                model: Some(self.probe.device_id.clone()),
                serial: Some(self.probe.serial_number.clone()),
                kinds: vec![
                    "hub".into(),
                    "motion.controller".into(),
                    "serial.ascii".into(),
                ],
                properties: vec![
                    property(
                        "device_id",
                        "Device ID",
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
                        "state_summary",
                        "State summary",
                        ValueType::Map,
                        None,
                        false,
                        None,
                    ),
                ],
                metadata: BTreeMap::from([
                    ("address".into(), Value::I64(self.probe.address as i64)),
                    (
                        "startup_readback_commands".into(),
                        Value::List(
                            protocol::ZaberProbeScript::for_axis(
                                self.probe.address,
                                self.probe.axis,
                            )
                            .commands
                            .into_iter()
                            .map(Value::String)
                            .collect(),
                        ),
                    ),
                    (
                        "probe_status".into(),
                        Value::String(self.probe.status.clone()),
                    ),
                    (
                        "microstep_size".into(),
                        position(self.probe.microstep_size_um),
                    ),
                    (
                        "legacy_microstep_size_um".into(),
                        position(self.probe.microstep_size_um),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.axis,
                driver: self.id,
                label: format!("zaber-ascii-axis-{}", self.probe.axis),
                vendor: Some("Zaber".into()),
                model: Some(self.probe.peripheral_id.clone()),
                serial: Some(self.probe.serial_number.clone()),
                kinds: vec![
                    format!("axis.{}", self.probe.axis),
                    "stage.axis".into(),
                    "stage.x".into(),
                ],
                properties: vec![
                    sequenceable_position_property_range(
                        "position",
                        "Position",
                        Some("um"),
                        true,
                        0.0,
                        self.probe.travel_um,
                    ),
                    property_range(
                        "target",
                        "Target",
                        Some("um"),
                        true,
                        0.0,
                        self.probe.travel_um,
                    ),
                    velocity_property_range(
                        "velocity",
                        "Velocity",
                        Some("um/s"),
                        true,
                        0.0,
                        200_000.0,
                    ),
                    acceleration_property_range(
                        "acceleration",
                        "Acceleration",
                        Some("um/s^2"),
                        true,
                        0.0,
                        2_000_000.0,
                    ),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                    property("status", "Status", ValueType::String, None, false, None),
                    property("warning", "Warning", ValueType::String, None, false, None),
                    property(
                        "peripheral_id",
                        "Peripheral ID",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                    position_property("travel", "Travel", Some("um"), false),
                    position_property("microstep_size", "Microstep size", Some("um"), false),
                    property(
                        "warning_summary",
                        "Warning summary",
                        ValueType::Map,
                        None,
                        false,
                        None,
                    ),
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
                    ("axis".into(), Value::I64(self.probe.axis as i64)),
                    ("travel".into(), position(self.probe.travel_um)),
                    ("probed_position".into(), position(self.probe.position_um)),
                    ("velocity".into(), velocity(self.velocity_um_s)),
                    ("acceleration".into(), acceleration(self.acceleration_um_s2)),
                    (
                        "microstep_size".into(),
                        position(self.probe.microstep_size_um),
                    ),
                    ("legacy_travel_um".into(), position(self.probe.travel_um)),
                    (
                        "legacy_probed_position_um".into(),
                        position(self.probe.position_um),
                    ),
                    ("legacy_velocity_um_s".into(), velocity(self.velocity_um_s)),
                    (
                        "legacy_acceleration_um_s2".into(),
                        acceleration(self.acceleration_um_s2),
                    ),
                    (
                        "legacy_microstep_size_um".into(),
                        position(self.probe.microstep_size_um),
                    ),
                    (
                        "probe_warning".into(),
                        self.probe
                            .warning
                            .as_ref()
                            .map(|warning| Value::String(warning.clone()))
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "probe_warning_summary".into(),
                        protocol::warning_summary(self.probe.warning.as_deref()),
                    ),
                ]),
            },
        ]
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        match (device, key) {
            (device, "device_id") if device == self.hub => {
                Ok(Value::String(self.probe.device_id.clone()))
            }
            (device, "serial_number") if device == self.hub => {
                Ok(Value::String(self.probe.serial_number.clone()))
            }
            (device, "state_summary") if device == self.hub => Ok(self.state_summary()),
            (device, "position") if device == self.axis => Ok(position(self.position_um)),
            (device, "target") if device == self.axis => Ok(position(self.target_um)),
            (device, "velocity") if device == self.axis => Ok(velocity(self.velocity_um_s)),
            (device, "acceleration") if device == self.axis => {
                Ok(acceleration(self.acceleration_um_s2))
            }
            (device, "busy") if device == self.axis => Ok(Value::Bool(self.busy)),
            (device, "status") if device == self.axis => {
                Ok(Value::String(self.probe.status.clone()))
            }
            (device, "warning") if device == self.axis => Ok(Value::String(
                self.probe.warning.clone().unwrap_or_else(|| "--".into()),
            )),
            (device, "peripheral_id") if device == self.axis => {
                Ok(Value::String(self.probe.peripheral_id.clone()))
            }
            (device, "travel") if device == self.axis => Ok(position(self.probe.travel_um)),
            (device, "microstep_size") if device == self.axis => {
                Ok(position(self.probe.microstep_size_um))
            }
            (device, "warning_summary") if device == self.axis => {
                Ok(protocol::warning_summary(self.probe.warning.as_deref()))
            }
            (device, "axis_summary") if device == self.axis => Ok(self.axis_summary()),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Zaber property {key}"),
            )),
        }
    }

    fn state_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("hub".into(), Value::I64(self.hub.0 .0 as i64)),
            (
                "device_id".into(),
                Value::String(self.probe.device_id.clone()),
            ),
            (
                "serial_number".into(),
                Value::String(self.probe.serial_number.clone()),
            ),
            ("address".into(), Value::I64(self.probe.address as i64)),
            ("axis_count".into(), Value::I64(1)),
            ("axis".into(), self.axis_summary()),
        ]))
    }

    fn axis_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("device".into(), Value::I64(self.axis.0 .0 as i64)),
            ("address".into(), Value::I64(self.probe.address as i64)),
            ("axis".into(), Value::I64(self.probe.axis as i64)),
            (
                "peripheral_id".into(),
                Value::String(self.probe.peripheral_id.clone()),
            ),
            ("position".into(), position(self.position_um)),
            ("target".into(), position(self.target_um)),
            ("travel".into(), position(self.probe.travel_um)),
            (
                "microstep_size".into(),
                position(self.probe.microstep_size_um),
            ),
            ("velocity".into(), velocity(self.velocity_um_s)),
            ("acceleration".into(), acceleration(self.acceleration_um_s2)),
            ("busy".into(), Value::Bool(self.busy)),
            ("status".into(), Value::String(self.probe.status.clone())),
            (
                "warning".into(),
                self.probe
                    .warning
                    .as_ref()
                    .map(|warning| Value::String(warning.clone()))
                    .unwrap_or(Value::Null),
            ),
            (
                "warning_summary".into(),
                protocol::warning_summary(self.probe.warning.as_deref()),
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
            (device, "position", value) if device == self.axis => {
                let position_um = position_um(value)?.clamp(0.0, self.probe.travel_um);
                self.move_absolute(position_um)?;
                Ok(position(self.position_um))
            }
            (device, "target", value) if device == self.axis => {
                let target = position_um(value)?.clamp(0.0, self.probe.travel_um);
                self.target_um = target;
                Ok(position(self.target_um))
            }
            (device, "velocity", value) if device == self.axis => {
                let velocity_um_s = velocity_um_s(value)?.clamp(0.0, 200_000.0);
                self.set_velocity(velocity_um_s)?;
                Ok(velocity(self.velocity_um_s))
            }
            (device, "acceleration", value) if device == self.axis => {
                let acceleration_um_s2 = acceleration_um_s2(value)?.clamp(0.0, 2_000_000.0);
                self.set_acceleration(acceleration_um_s2)?;
                Ok(acceleration(self.acceleration_um_s2))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid Zaber write {key}"),
            )),
        }
    }

    fn set_velocity(&mut self, velocity_um_s: f64) -> Result<()> {
        self.velocity_um_s = velocity_um_s;
        self.send(protocol::ZaberCommand::SetNative {
            setting: "maxspeed".into(),
            value: protocol::native_velocity(velocity_um_s, self.probe.microstep_size_um),
        })
    }

    fn set_acceleration(&mut self, acceleration_um_s2: f64) -> Result<()> {
        self.acceleration_um_s2 = acceleration_um_s2;
        self.send(protocol::ZaberCommand::SetNative {
            setting: "accel".into(),
            value: protocol::native_acceleration(acceleration_um_s2, self.probe.microstep_size_um),
        })
    }

    fn apply_motion_profile(&mut self, profile: &MotionProfile) -> Result<()> {
        if let Some(profile_velocity) = profile.velocity {
            self.set_velocity(
                profile_velocity
                    .micrometers_per_second()
                    .clamp(0.0, 200_000.0),
            )?;
            self.emit_property(self.axis, "velocity", velocity(self.velocity_um_s));
        }
        if let Some(profile_acceleration) = profile.acceleration {
            self.set_acceleration(
                profile_acceleration
                    .micrometers_per_second_squared()
                    .clamp(0.0, 2_000_000.0),
            )?;
            self.emit_property(
                self.axis,
                "acceleration",
                acceleration(self.acceleration_um_s2),
            );
        }
        Ok(())
    }

    fn move_absolute(&mut self, position_um: f64) -> Result<()> {
        self.target_um = position_um;
        self.send(protocol::ZaberCommand::MoveAbsolute { position_um })?;
        if self.read_command_reply("move abs")? {
            self.refresh_position_readback()?;
        } else {
            self.finish_motion(position_um);
        }
        Ok(())
    }

    fn move_relative(&mut self, distance_um: f64) -> Result<()> {
        let final_position_um = (self.position_um + distance_um).clamp(0.0, self.probe.travel_um);
        let clamped_distance_um = final_position_um - self.position_um;
        self.target_um = final_position_um;
        self.send(protocol::ZaberCommand::MoveRelative {
            distance_um: clamped_distance_um,
        })?;
        if self.read_command_reply("move rel")? {
            self.refresh_position_readback()?;
        } else {
            self.finish_motion(final_position_um);
        }
        Ok(())
    }

    fn validate_stage_move(&self, device: DeviceId, request: &StageMoveRequest) -> Result<()> {
        if device != self.axis {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Zaber StageMove targets the axis device",
            ));
        }
        if request.target.len() != 1 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Zaber StageMove expects exactly one axis target",
            ));
        }
        let Some((axis, _)) = request.target.iter().next() else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Zaber StageMove target must contain one axis",
            ));
        };
        let supported_axis = match axis {
            StageAxis::X => true,
            StageAxis::Custom(name) => name == "1" || name == "axis1" || name == "x",
            _ => false,
        };
        if !supported_axis {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Zaber StageMove supports only the configured X axis",
            ));
        }
        Ok(())
    }

    fn stage_move(&mut self, request: &StageMoveRequest) -> Result<Value> {
        self.validate_stage_move(self.axis, request)?;
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
            self.move_absolute(distance_um.clamp(0.0, self.probe.travel_um))?;
        }
        self.emit_property(self.axis, "position", position(self.position_um));
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
            ("velocity".into(), velocity(self.velocity_um_s)),
            ("acceleration".into(), acceleration(self.acceleration_um_s2)),
        ])))
    }

    fn apply_state_set(&mut self, set: StateSet) -> Result<Value> {
        let mut next_position = self.position_um;
        let mut next_target = self.target_um;
        let mut next_velocity = self.velocity_um_s;
        let mut next_acceleration = self.acceleration_um_s2;
        for write in &set.writes {
            self.validate_write(write.device, &write.property, &write.value)?;
            match (write.device, write.property.as_str(), &write.value) {
                (device, "position", value) if device == self.axis => {
                    next_position = position_um(value)?.clamp(0.0, self.probe.travel_um);
                }
                (device, "target", value) if device == self.axis => {
                    next_target = position_um(value)?.clamp(0.0, self.probe.travel_um);
                }
                (device, "velocity", value) if device == self.axis => {
                    next_velocity = velocity_um_s(value)?.clamp(0.0, 200_000.0);
                }
                (device, "acceleration", value) if device == self.axis => {
                    next_acceleration = acceleration_um_s2(value)?.clamp(0.0, 2_000_000.0);
                }
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "unsupported Zaber state-set write",
                    ))
                }
            }
        }

        let mut changed = BTreeMap::new();
        if next_velocity != self.velocity_um_s {
            self.set_velocity(next_velocity)?;
            changed.insert(
                format!("{}:velocity", (self.axis.0).0),
                velocity(self.velocity_um_s),
            );
            self.emit_property(self.axis, "velocity", velocity(self.velocity_um_s));
        }
        if next_acceleration != self.acceleration_um_s2 {
            self.set_acceleration(next_acceleration)?;
            changed.insert(
                format!("{}:acceleration", (self.axis.0).0),
                acceleration(self.acceleration_um_s2),
            );
            self.emit_property(
                self.axis,
                "acceleration",
                acceleration(self.acceleration_um_s2),
            );
        }
        if next_target != self.target_um {
            self.target_um = next_target;
            changed.insert(format!("{}:target", (self.axis.0).0), position(next_target));
            self.emit_property(self.axis, "target", position(next_target));
        }
        if next_position != self.position_um {
            self.move_absolute(next_position)?;
            changed.insert(
                format!("{}:position", (self.axis.0).0),
                position(self.position_um),
            );
            self.emit_property(self.axis, "position", position(self.position_um));
        }
        Ok(Value::Map(changed))
    }

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| sequence.device == self.axis)
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequences(plan) {
            if sequence.property != "position" {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Zaber timing sequences can only target position",
                ));
            }
            for value in &sequence.values {
                let _ = position_um(value)?;
            }
        }
        Ok(())
    }

    fn timing_summary(&self, plan: &TimingPlan, phase: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("phase".into(), Value::String(phase.into())),
            ("axis".into(), Value::I64(self.axis.0 .0 as i64)),
            (
                "axis_participant".into(),
                Value::Bool(plan.participants.contains(&self.axis)),
            ),
            ("position".into(), position(self.position_um)),
            ("target".into(), position(self.target_um)),
            (
                "sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
        ]))
    }

    fn apply_timing_sequence_step(&mut self, plan: &TimingPlan, first: bool) -> Result<Value> {
        let mut writes = Vec::new();
        for sequence in self.local_timing_sequences(plan) {
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
                "zaber timing start sequence".into()
            } else {
                "zaber timing stop sequence".into()
            }),
            writes,
            commit: CommitMode::Immediate,
        })
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
                "unknown Zaber capability",
            ));
        };
        match (capability.kind, request) {
            (CapabilityKind::StageMove, CapabilityRequest::StageMove(request))
                if device == self.axis =>
            {
                self.stage_move(&request)
            }
            (CapabilityKind::GenericCommand, CapabilityRequest::GenericCommand(request))
                if device == self.axis =>
            {
                self.apply_generic_command(device, request)
            }
            (CapabilityKind::GenericCommand, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Zaber GenericCommand expects a GenericCommandRequest",
            )),
            (CapabilityKind::StageMove, CapabilityRequest::None) if device == self.axis => {
                Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "Zaber StageMove requires a StageMoveRequest",
                ))
            }
            (CapabilityKind::StageHome, CapabilityRequest::None) if device == self.axis => {
                self.send(protocol::ZaberCommand::Home)?;
                if self.read_command_reply("home")? {
                    self.refresh_position_readback()?;
                } else {
                    self.finish_motion(0.0);
                    self.emit_property(self.axis, "position", position(self.position_um));
                }
                Ok(Value::String("homed".into()))
            }
            (CapabilityKind::StageStop, CapabilityRequest::None) if device == self.axis => {
                self.send(protocol::ZaberCommand::Stop)?;
                if self.read_command_reply("stop")? {
                    self.refresh_position_readback()?;
                } else {
                    self.busy = false;
                    self.emit_property(self.axis, "busy", Value::Bool(false));
                }
                Ok(Value::String("stopped".into()))
            }
            (CapabilityKind::StageMove, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Zaber StageMove expects a StageMoveRequest",
            )),
            (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Zaber home/stop capabilities take no request",
            )),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Zaber capability",
            )),
        }
    }

    fn validate_generic_command(
        &self,
        device: DeviceId,
        request: &GenericCommandRequest,
    ) -> Result<()> {
        if request.is_hidden_maintenance() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "GenericCommand {} is a hidden maintenance operation",
                    request.command
                ),
            ));
        }
        if device != self.axis {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Zaber GenericCommand requires an axis device",
            ));
        }
        if !request.params.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Zaber refresh commands do not take parameters",
            ));
        }
        let _ = zaber_refresh_properties(&request.command)?;
        Ok(())
    }

    fn apply_generic_command(
        &mut self,
        device: DeviceId,
        request: GenericCommandRequest,
    ) -> Result<Value> {
        self.validate_generic_command(device, &request)?;
        let properties = zaber_refresh_properties(&request.command)?;
        let mut values = BTreeMap::new();
        for property in properties {
            let query = self.query_for_property(device, property).ok_or_else(|| {
                Error::new(
                    ErrorCode::Unsupported,
                    format!("Zaber cannot refresh property {property}"),
                )
            })?;
            self.send(query.clone())?;
            self.read_query_reply(&query)?;
            values.insert(
                (*property).to_string(),
                self.read_property(device, property)?,
            );
        }
        Ok(zaber_refresh_result(request.command, properties, values))
    }

    fn finish_motion(&mut self, final_position_um: f64) {
        self.busy = true;
        self.pending
            .push_back(DriverEvent::Event(Event::Log(LogEvent {
                driver: Some(self.id),
                message: "zaber status BUSY".into(),
            })));
        self.position_um = final_position_um;
        self.busy = false;
        self.pending
            .push_back(DriverEvent::Event(Event::Log(LogEvent {
                driver: Some(self.id),
                message: "zaber status IDLE".into(),
            })));
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

impl Driver for ZaberAsciiDriver {
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
            label: "zaber-ascii-serial".into(),
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
                ("terminator".into(), Value::String("CRLF".into())),
                (
                    "startup_readback_supported".into(),
                    Value::List(
                        protocol::PROBE_SETTINGS
                            .into_iter()
                            .map(|setting| Value::String(setting.into()))
                            .collect(),
                    ),
                ),
                (
                    "completion".into(),
                    Value::String("ASCII status handling for configured/probed axis".into()),
                ),
            ]),
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.axis {
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
                        description: format!("zaber read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("zaber write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "zaber remultiplexed stage state set".into(),
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
                            Error::new(ErrorCode::Unsupported, "unknown Zaber capability")
                        })?;
                    match (&candidate.kind, request) {
                        (CapabilityKind::StageMove, CapabilityRequest::StageMove(request)) => {
                            self.validate_stage_move(*device, request)?;
                        }
                        (
                            CapabilityKind::GenericCommand,
                            CapabilityRequest::GenericCommand(request),
                        ) => {
                            self.validate_generic_command(*device, request)?;
                        }
                        (CapabilityKind::GenericCommand, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Zaber GenericCommand expects a GenericCommandRequest",
                            ));
                        }
                        (
                            CapabilityKind::StageHome | CapabilityKind::StageStop,
                            CapabilityRequest::None,
                        ) => {}
                        (CapabilityKind::StageMove, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Zaber StageMove expects a StageMoveRequest",
                            ));
                        }
                        (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Zaber home/stop capabilities take no request",
                            ));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported Zaber capability",
                            ));
                        }
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("zaber invoke {}", capability.0),
                        payload: match request {
                            CapabilityRequest::StageMove(request) => Value::Map(BTreeMap::from([
                                ("relative".into(), Value::Bool(request.relative)),
                                (
                                    "axes".into(),
                                    Value::List(
                                        request
                                            .target
                                            .keys()
                                            .map(|axis| Value::String(axis.name().into()))
                                            .collect(),
                                    ),
                                ),
                            ])),
                            _ => Value::Null,
                        },
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
                    if let Some(query) = self.query_for_property(device, &key) {
                        self.send(query.clone())?;
                        self.read_query_reply(&query)?;
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
        if let Ok(bytes) = self.serial.read_available() {
            for line in self.codec.push(&bytes) {
                self.pending
                    .push_back(DriverEvent::Event(Event::Log(LogEvent {
                        driver: Some(self.id),
                        message: format!("zaber serial: {line}"),
                    })));
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
                description: "zaber timing arm summary".into(),
                payload: self.timing_summary(plan, "arm"),
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
                description: "zaber timing start sequence".into(),
                payload: Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "start")),
                    ("changed".into(), changed),
                ])),
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
                description: "zaber timing stop sequence".into(),
                payload: Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "stop")),
                    ("changed".into(), changed),
                ])),
            }],
        })
    }
}

#[derive(Debug, Clone)]
struct ZaberAxisState {
    device: DeviceId,
    probe: protocol::ZaberAsciiProbe,
    position_um: f64,
    target_um: f64,
    velocity_um_s: f64,
    acceleration_um_s2: f64,
    busy: bool,
}

pub struct ZaberAsciiMultiAxisDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    axes: Vec<ZaberAxisState>,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    codec: SerialLineCodec,
}

impl ZaberAsciiMultiAxisDriver {
    pub fn simulated(id: DriverId, probe: ZaberMultiAxisProbe) -> Result<Self> {
        Self::new_with_transport_metadata(
            id,
            probe.axes,
            probe.serial_endpoint,
            false,
            Box::new(ScriptedSerial::new()),
        )
    }

    pub fn new(
        id: DriverId,
        probes: Vec<protocol::ZaberAsciiProbe>,
        serial: Box<dyn SerialIo>,
    ) -> Result<Self> {
        Self::new_with_transport_metadata(id, probes, None, false, serial)
    }

    fn new_with_transport_metadata(
        id: DriverId,
        probes: Vec<protocol::ZaberAsciiProbe>,
        endpoint: Option<ZaberSerialEndpoint>,
        connected: bool,
        serial: Box<dyn SerialIo>,
    ) -> Result<Self> {
        if probes.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Zaber multi-axis driver requires at least one axis",
            ));
        }
        let axes = probes
            .into_iter()
            .enumerate()
            .map(|(index, probe)| ZaberAxisState {
                device: DeviceId(NodeId(id.0 * 1000 + 930 + index as u64)),
                position_um: probe.position_um,
                target_um: probe.position_um,
                velocity_um_s: probe.velocity_um_s,
                acceleration_um_s2: probe.acceleration_um_s2,
                busy: probe.status == "BUSY",
                probe,
            })
            .collect();
        let serial_port = endpoint.as_ref().map(|endpoint| endpoint.port_name.clone());
        let baud_rate = endpoint
            .as_ref()
            .map(|endpoint| endpoint.baud_rate)
            .unwrap_or(protocol::BAUD);
        let serial_timeout_ms = endpoint
            .as_ref()
            .map(|endpoint| endpoint.timeout_ms)
            .unwrap_or(1);
        Ok(Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 901)),
            hub: DeviceId(NodeId(id.0 * 1000 + 910)),
            axes,
            serial_port,
            baud_rate,
            serial_timeout_ms,
            connected,
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            codec: SerialLineCodec::new(protocol::SEND_ENDING, protocol::RECV_ENDING),
        })
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn axis_index(&self, device: DeviceId) -> Result<usize> {
        self.axes
            .iter()
            .position(|axis| axis.device == device)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown Zaber axis device"))
    }

    fn send_axis(&mut self, index: usize, command: protocol::ZaberCommand) -> Result<()> {
        let axis = &self.axes[index];
        let line = protocol::encode(
            axis.probe.address,
            axis.probe.axis,
            &command,
            axis.probe.microstep_size_um,
        );
        self.serial.write(&self.codec.encode(&line))
    }

    fn query_for_property(
        &self,
        device: DeviceId,
        key: &str,
    ) -> Result<Option<(usize, protocol::ZaberCommand)>> {
        if device == self.hub {
            let setting = match key {
                "device_id" => "device.id",
                "serial_number" => "system.serial",
                "state_summary" => "pos",
                _ => return Ok(None),
            };
            return Ok(Some((
                0,
                protocol::ZaberCommand::Get {
                    setting: setting.into(),
                },
            )));
        }
        let index = self.axis_index(device)?;
        let setting = match key {
            "position" | "busy" | "status" | "warning" | "warning_summary" | "axis_summary" => {
                "pos"
            }
            "velocity" => "maxspeed",
            "acceleration" => "accel",
            _ => return Ok(None),
        };
        Ok(Some((
            index,
            protocol::ZaberCommand::Get {
                setting: setting.into(),
            },
        )))
    }

    fn read_query_reply(&mut self, index: usize, command: &protocol::ZaberCommand) -> Result<()> {
        let bytes = self.serial.read_available()?;
        if bytes.is_empty() {
            return Ok(());
        }
        for line in self.codec.push(&bytes) {
            self.apply_readback_reply(index, command, &line)?;
        }
        Ok(())
    }

    fn refresh_position_readback(&mut self, index: usize) -> Result<()> {
        let command = protocol::ZaberCommand::Get {
            setting: "pos".into(),
        };
        self.send_axis(index, command.clone())?;
        self.read_query_reply(index, &command)
    }

    fn read_command_reply(&mut self, index: usize, action: &str) -> Result<bool> {
        let bytes = self.serial.read_available()?;
        if bytes.is_empty() {
            return Ok(false);
        }
        let mut saw_reply = false;
        for line in self.codec.push(&bytes) {
            self.apply_command_reply(index, action, &line)?;
            saw_reply = true;
        }
        Ok(saw_reply)
    }

    fn apply_command_reply(&mut self, index: usize, action: &str, reply: &str) -> Result<()> {
        let parsed = protocol::parse_reply(reply)?;
        if !parsed.ok {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("Zaber {action} failed: {reply}"),
            ));
        }
        if parsed.address != self.axes[index].probe.address
            || parsed.axis != self.axes[index].probe.axis
        {
            return Err(Error::new(
                ErrorCode::Transport,
                "Zaber multi-axis command reply address/axis did not match request",
            ));
        }

        self.axes[index].probe.status = parsed.status;
        self.axes[index].probe.warning = parsed.warning;
        self.axes[index].busy = self.axes[index].probe.status == "BUSY";

        let device = self.axes[index].device;
        self.emit_property(device, "busy", Value::Bool(self.axes[index].busy));
        self.emit_property(
            device,
            "status",
            Value::String(self.axes[index].probe.status.clone()),
        );
        self.emit_property(
            device,
            "warning",
            Value::String(
                self.axes[index]
                    .probe
                    .warning
                    .clone()
                    .unwrap_or_else(|| "--".into()),
            ),
        );
        self.emit_property(
            device,
            "warning_summary",
            protocol::warning_summary(self.axes[index].probe.warning.as_deref()),
        );
        Ok(())
    }

    fn apply_readback_reply(
        &mut self,
        index: usize,
        command: &protocol::ZaberCommand,
        reply: &str,
    ) -> Result<()> {
        let protocol::ZaberCommand::Get { setting } = command else {
            return Ok(());
        };
        let parsed = protocol::parse_reply(reply)?;
        if !parsed.ok {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("Zaber get {setting} failed: {reply}"),
            ));
        }
        if parsed.address != self.axes[index].probe.address
            || parsed.axis != self.axes[index].probe.axis
        {
            return Err(Error::new(
                ErrorCode::Transport,
                "Zaber multi-axis readback reply address/axis did not match request",
            ));
        }

        self.axes[index].probe.status = parsed.status;
        self.axes[index].probe.warning = parsed.warning;
        self.axes[index].busy = self.axes[index].probe.status == "BUSY";

        match setting.as_str() {
            "device.id" => {
                self.axes[index].probe.device_id = parsed.data;
                if index == 0 {
                    self.emit_property(
                        self.hub,
                        "device_id",
                        Value::String(self.axes[index].probe.device_id.clone()),
                    );
                }
            }
            "system.serial" => {
                self.axes[index].probe.serial_number = parsed.data;
                if index == 0 {
                    self.emit_property(
                        self.hub,
                        "serial_number",
                        Value::String(self.axes[index].probe.serial_number.clone()),
                    );
                }
            }
            "pos" => {
                let native = protocol::parse_i64_data("pos", &parsed.data)?;
                self.axes[index].position_um =
                    protocol::micrometers(native, self.axes[index].probe.microstep_size_um);
                self.emit_property(
                    self.axes[index].device,
                    "position",
                    position(self.axes[index].position_um),
                );
            }
            "maxspeed" => {
                let native = protocol::parse_i64_data("maxspeed", &parsed.data)?;
                self.axes[index].velocity_um_s =
                    protocol::micrometers(native, self.axes[index].probe.microstep_size_um);
                self.emit_property(
                    self.axes[index].device,
                    "velocity",
                    velocity(self.axes[index].velocity_um_s),
                );
            }
            "accel" => {
                let native = protocol::parse_i64_data("accel", &parsed.data)?;
                self.axes[index].acceleration_um_s2 =
                    protocol::micrometers(native, self.axes[index].probe.microstep_size_um);
                self.emit_property(
                    self.axes[index].device,
                    "acceleration",
                    acceleration(self.axes[index].acceleration_um_s2),
                );
            }
            _ => {}
        }

        let device = self.axes[index].device;
        self.emit_property(device, "busy", Value::Bool(self.axes[index].busy));
        self.emit_property(
            device,
            "status",
            Value::String(self.axes[index].probe.status.clone()),
        );
        self.emit_property(
            device,
            "warning",
            Value::String(
                self.axes[index]
                    .probe
                    .warning
                    .clone()
                    .unwrap_or_else(|| "--".into()),
            ),
        );
        self.emit_property(
            device,
            "warning_summary",
            protocol::warning_summary(self.axes[index].probe.warning.as_deref()),
        );
        self.emit_property(device, "axis_summary", self.axis_summary(&self.axes[index]));
        self.emit_property(self.hub, "state_summary", self.state_summary());
        Ok(())
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        let mut descriptors = vec![DeviceDescriptor {
            id: self.hub,
            driver: self.id,
            label: "zaber-ascii-multi-axis-hub".into(),
            vendor: Some("Zaber".into()),
            model: self.axes.first().map(|axis| axis.probe.device_id.clone()),
            serial: self
                .axes
                .first()
                .map(|axis| axis.probe.serial_number.clone()),
            kinds: vec![
                "hub".into(),
                "motion.controller".into(),
                "serial.ascii".into(),
                "multi-axis".into(),
            ],
            properties: vec![
                property(
                    "axis_count",
                    "Axis count",
                    ValueType::I64,
                    None,
                    false,
                    None,
                ),
                property(
                    "device_id",
                    "Device ID",
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
                    "state_summary",
                    "State summary",
                    ValueType::Map,
                    None,
                    false,
                    None,
                ),
            ],
            metadata: BTreeMap::from([
                ("axis_count".into(), Value::I64(self.axes.len() as i64)),
                (
                    "axes".into(),
                    Value::List(
                        self.axes
                            .iter()
                            .map(|axis| {
                                Value::Map(BTreeMap::from([
                                    ("device".into(), Value::I64((axis.device.0).0 as i64)),
                                    ("address".into(), Value::I64(axis.probe.address as i64)),
                                    ("axis".into(), Value::I64(axis.probe.axis as i64)),
                                    (
                                        "peripheral_id".into(),
                                        Value::String(axis.probe.peripheral_id.clone()),
                                    ),
                                    ("travel".into(), position(axis.probe.travel_um)),
                                    ("legacy_travel_um".into(), position(axis.probe.travel_um)),
                                ]))
                            })
                            .collect(),
                    ),
                ),
            ]),
        }];

        descriptors.extend(self.axes.iter().map(|axis| {
            DeviceDescriptor {
                id: axis.device,
                driver: self.id,
                label: format!("zaber-ascii-axis-{}", axis.probe.axis),
                vendor: Some("Zaber".into()),
                model: Some(axis.probe.peripheral_id.clone()),
                serial: Some(axis.probe.serial_number.clone()),
                kinds: vec![
                    format!("axis.{}", axis.probe.axis),
                    "stage.axis".into(),
                    match axis.probe.axis {
                        1 => "stage.x".into(),
                        2 => "stage.y".into(),
                        3 => "stage.z".into(),
                        _ => "stage.custom".into(),
                    },
                ],
                properties: vec![
                    sequenceable_position_property_range(
                        "position",
                        "Position",
                        Some("um"),
                        true,
                        0.0,
                        axis.probe.travel_um,
                    ),
                    property_range(
                        "target",
                        "Target",
                        Some("um"),
                        true,
                        0.0,
                        axis.probe.travel_um,
                    ),
                    velocity_property_range(
                        "velocity",
                        "Velocity",
                        Some("um/s"),
                        true,
                        0.0,
                        200_000.0,
                    ),
                    acceleration_property_range(
                        "acceleration",
                        "Acceleration",
                        Some("um/s^2"),
                        true,
                        0.0,
                        2_000_000.0,
                    ),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                    property("status", "Status", ValueType::String, None, false, None),
                    property("warning", "Warning", ValueType::String, None, false, None),
                    property(
                        "peripheral_id",
                        "Peripheral ID",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                    position_property("travel", "Travel", Some("um"), false),
                    position_property("microstep_size", "Microstep size", Some("um"), false),
                    property(
                        "warning_summary",
                        "Warning summary",
                        ValueType::Map,
                        None,
                        false,
                        None,
                    ),
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
                    ("axis".into(), Value::I64(axis.probe.axis as i64)),
                    ("travel".into(), position(axis.probe.travel_um)),
                    ("probed_position".into(), position(axis.probe.position_um)),
                    ("velocity".into(), velocity(axis.velocity_um_s)),
                    ("acceleration".into(), acceleration(axis.acceleration_um_s2)),
                    (
                        "microstep_size".into(),
                        position(axis.probe.microstep_size_um),
                    ),
                    ("legacy_travel_um".into(), position(axis.probe.travel_um)),
                    (
                        "legacy_probed_position_um".into(),
                        position(axis.probe.position_um),
                    ),
                    ("legacy_velocity_um_s".into(), velocity(axis.velocity_um_s)),
                    (
                        "legacy_acceleration_um_s2".into(),
                        acceleration(axis.acceleration_um_s2),
                    ),
                    (
                        "legacy_microstep_size_um".into(),
                        position(axis.probe.microstep_size_um),
                    ),
                    (
                        "probe_warning".into(),
                        axis.probe
                            .warning
                            .as_ref()
                            .map(|warning| Value::String(warning.clone()))
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "probe_warning_summary".into(),
                        protocol::warning_summary(axis.probe.warning.as_deref()),
                    ),
                ]),
            }
        }));
        descriptors
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "axis_count" => Ok(Value::I64(self.axes.len() as i64)),
                "device_id" => Ok(Value::String(self.axes[0].probe.device_id.clone())),
                "serial_number" => Ok(Value::String(self.axes[0].probe.serial_number.clone())),
                "state_summary" => Ok(self.state_summary()),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown Zaber hub property {key}"),
                )),
            };
        }
        let axis = &self.axes[self.axis_index(device)?];
        match key {
            "position" => Ok(position(axis.position_um)),
            "target" => Ok(position(axis.target_um)),
            "velocity" => Ok(velocity(axis.velocity_um_s)),
            "acceleration" => Ok(acceleration(axis.acceleration_um_s2)),
            "busy" => Ok(Value::Bool(axis.busy)),
            "status" => Ok(Value::String(axis.probe.status.clone())),
            "warning" => Ok(Value::String(
                axis.probe.warning.clone().unwrap_or_else(|| "--".into()),
            )),
            "peripheral_id" => Ok(Value::String(axis.probe.peripheral_id.clone())),
            "travel" => Ok(position(axis.probe.travel_um)),
            "microstep_size" => Ok(position(axis.probe.microstep_size_um)),
            "warning_summary" => Ok(protocol::warning_summary(axis.probe.warning.as_deref())),
            "axis_summary" => Ok(self.axis_summary(axis)),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Zaber axis property {key}"),
            )),
        }
    }

    fn state_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("hub".into(), Value::I64(self.hub.0 .0 as i64)),
            (
                "device_id".into(),
                Value::String(self.axes[0].probe.device_id.clone()),
            ),
            (
                "serial_number".into(),
                Value::String(self.axes[0].probe.serial_number.clone()),
            ),
            ("axis_count".into(), Value::I64(self.axes.len() as i64)),
            (
                "axes".into(),
                Value::List(
                    self.axes
                        .iter()
                        .map(|axis| self.axis_summary(axis))
                        .collect(),
                ),
            ),
        ]))
    }

    fn axis_summary(&self, axis: &ZaberAxisState) -> Value {
        Value::Map(BTreeMap::from([
            ("device".into(), Value::I64(axis.device.0 .0 as i64)),
            ("address".into(), Value::I64(axis.probe.address as i64)),
            ("axis".into(), Value::I64(axis.probe.axis as i64)),
            (
                "peripheral_id".into(),
                Value::String(axis.probe.peripheral_id.clone()),
            ),
            ("position".into(), position(axis.position_um)),
            ("target".into(), position(axis.target_um)),
            ("travel".into(), position(axis.probe.travel_um)),
            (
                "microstep_size".into(),
                position(axis.probe.microstep_size_um),
            ),
            ("velocity".into(), velocity(axis.velocity_um_s)),
            ("acceleration".into(), acceleration(axis.acceleration_um_s2)),
            ("busy".into(), Value::Bool(axis.busy)),
            ("status".into(), Value::String(axis.probe.status.clone())),
            (
                "warning".into(),
                axis.probe
                    .warning
                    .as_ref()
                    .map(|warning| Value::String(warning.clone()))
                    .unwrap_or(Value::Null),
            ),
            (
                "warning_summary".into(),
                protocol::warning_summary(axis.probe.warning.as_deref()),
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

    fn set_velocity(&mut self, index: usize, velocity_um_s: f64) -> Result<()> {
        self.axes[index].velocity_um_s = velocity_um_s;
        self.send_axis(
            index,
            protocol::ZaberCommand::SetNative {
                setting: "maxspeed".into(),
                value: protocol::native_velocity(
                    velocity_um_s,
                    self.axes[index].probe.microstep_size_um,
                ),
            },
        )
    }

    fn set_acceleration(&mut self, index: usize, acceleration_um_s2: f64) -> Result<()> {
        self.axes[index].acceleration_um_s2 = acceleration_um_s2;
        self.send_axis(
            index,
            protocol::ZaberCommand::SetNative {
                setting: "accel".into(),
                value: protocol::native_acceleration(
                    acceleration_um_s2,
                    self.axes[index].probe.microstep_size_um,
                ),
            },
        )
    }

    fn move_absolute(&mut self, index: usize, position_um: f64) -> Result<()> {
        let clamped_position = position_um.clamp(0.0, self.axes[index].probe.travel_um);
        self.axes[index].target_um = clamped_position;
        self.send_axis(
            index,
            protocol::ZaberCommand::MoveAbsolute {
                position_um: clamped_position,
            },
        )?;
        if self.read_command_reply(index, "move abs")? {
            self.refresh_position_readback(index)?;
        } else {
            self.finish_motion(index, clamped_position);
        }
        Ok(())
    }

    fn move_relative(&mut self, index: usize, distance_um: f64) -> Result<()> {
        let final_position = (self.axes[index].position_um + distance_um)
            .clamp(0.0, self.axes[index].probe.travel_um);
        let clamped_distance = final_position - self.axes[index].position_um;
        self.axes[index].target_um = final_position;
        self.send_axis(
            index,
            protocol::ZaberCommand::MoveRelative {
                distance_um: clamped_distance,
            },
        )?;
        if self.read_command_reply(index, "move rel")? {
            self.refresh_position_readback(index)?;
        } else {
            self.finish_motion(index, final_position);
        }
        Ok(())
    }

    fn apply_motion_profile(&mut self, index: usize, profile: &MotionProfile) -> Result<()> {
        if let Some(profile_velocity) = profile.velocity {
            self.set_velocity(
                index,
                profile_velocity
                    .micrometers_per_second()
                    .clamp(0.0, 200_000.0),
            )?;
            let device = self.axes[index].device;
            self.emit_property(device, "velocity", velocity(self.axes[index].velocity_um_s));
        }
        if let Some(profile_acceleration) = profile.acceleration {
            self.set_acceleration(
                index,
                profile_acceleration
                    .micrometers_per_second_squared()
                    .clamp(0.0, 2_000_000.0),
            )?;
            let device = self.axes[index].device;
            self.emit_property(
                device,
                "acceleration",
                acceleration(self.axes[index].acceleration_um_s2),
            );
        }
        Ok(())
    }

    fn stage_move(&mut self, device: DeviceId, request: &StageMoveRequest) -> Result<Value> {
        let index = self.axis_index(device)?;
        if request.target.len() != 1 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Zaber multi-axis StageMove expects exactly one target for the selected axis device",
            ));
        }
        if let Some(profile) = &request.profile {
            self.apply_motion_profile(index, profile)?;
        }
        let distance_um = request
            .target
            .values()
            .next()
            .expect("validated one target")
            .micrometers();
        if request.relative {
            self.move_relative(index, distance_um)?;
        } else {
            self.move_absolute(index, distance_um)?;
        }
        self.emit_property(device, "position", position(self.axes[index].position_um));
        Ok(Value::Map(BTreeMap::from([
            (
                "axis".into(),
                Value::I64(self.axes[index].probe.axis as i64),
            ),
            (
                "mode".into(),
                Value::String(
                    if request.relative {
                        "relative"
                    } else {
                        "absolute"
                    }
                    .into(),
                ),
            ),
            ("position".into(), position(self.axes[index].position_um)),
            ("velocity".into(), velocity(self.axes[index].velocity_um_s)),
            (
                "acceleration".into(),
                acceleration(self.axes[index].acceleration_um_s2),
            ),
        ])))
    }

    fn apply_state_set(&mut self, set: StateSet) -> Result<Value> {
        for write in &set.writes {
            self.validate_write(write.device, &write.property, &write.value)?;
        }
        let mut changed = BTreeMap::new();
        for write in set.writes {
            let index = self.axis_index(write.device)?;
            match (write.property.as_str(), &write.value) {
                ("velocity", value) => {
                    let value = velocity_um_s(value)?.clamp(0.0, 200_000.0);
                    self.set_velocity(index, value)?;
                    changed.insert(
                        format!("{}:velocity", (write.device.0).0),
                        velocity(self.axes[index].velocity_um_s),
                    );
                    self.emit_property(
                        write.device,
                        "velocity",
                        velocity(self.axes[index].velocity_um_s),
                    );
                }
                ("acceleration", value) => {
                    let value = acceleration_um_s2(value)?.clamp(0.0, 2_000_000.0);
                    self.set_acceleration(index, value)?;
                    changed.insert(
                        format!("{}:acceleration", (write.device.0).0),
                        acceleration(self.axes[index].acceleration_um_s2),
                    );
                    self.emit_property(
                        write.device,
                        "acceleration",
                        acceleration(self.axes[index].acceleration_um_s2),
                    );
                }
                ("target", value) => {
                    let target = position_um(value)?.clamp(0.0, self.axes[index].probe.travel_um);
                    self.axes[index].target_um = target;
                    changed.insert(format!("{}:target", (write.device.0).0), position(target));
                    self.emit_property(write.device, "target", position(target));
                }
                ("position", value) => {
                    let target = position_um(value)?.clamp(0.0, self.axes[index].probe.travel_um);
                    self.move_absolute(index, target)?;
                    changed.insert(
                        format!("{}:position", (write.device.0).0),
                        position(self.axes[index].position_um),
                    );
                    self.emit_property(
                        write.device,
                        "position",
                        position(self.axes[index].position_um),
                    );
                }
                _ => unreachable!("validated writable property"),
            }
        }
        Ok(Value::Map(changed))
    }

    fn validate_generic_command(
        &self,
        device: DeviceId,
        request: &GenericCommandRequest,
    ) -> Result<()> {
        if request.is_hidden_maintenance() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "GenericCommand {} is a hidden maintenance operation",
                    request.command
                ),
            ));
        }
        let _ = self.axis_index(device)?;
        if !request.params.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Zaber multi-axis refresh commands do not take parameters",
            ));
        }
        let _ = zaber_refresh_properties(&request.command)?;
        Ok(())
    }

    fn apply_generic_command(
        &mut self,
        device: DeviceId,
        request: GenericCommandRequest,
    ) -> Result<Value> {
        self.validate_generic_command(device, &request)?;
        let properties = zaber_refresh_properties(&request.command)?;
        let mut values = BTreeMap::new();
        for property in properties {
            let (index, query) = self.query_for_property(device, property)?.ok_or_else(|| {
                Error::new(
                    ErrorCode::Unsupported,
                    format!("Zaber multi-axis cannot refresh property {property}"),
                )
            })?;
            self.send_axis(index, query.clone())?;
            self.read_query_reply(index, &query)?;
            values.insert(
                (*property).to_string(),
                self.read_property(device, property)?,
            );
        }
        Ok(zaber_refresh_result(request.command, properties, values))
    }

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| self.axes.iter().any(|axis| axis.device == sequence.device))
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequences(plan) {
            if sequence.property != "position" {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Zaber multi-axis timing sequences can only target position",
                ));
            }
            for value in &sequence.values {
                let _ = position_um(value)?;
            }
        }
        Ok(())
    }

    fn timing_summary(&self, plan: &TimingPlan, phase: &str) -> Value {
        let axes = self
            .axes
            .iter()
            .map(|axis| {
                Value::Map(BTreeMap::from([
                    ("device".into(), Value::I64(axis.device.0 .0 as i64)),
                    ("axis".into(), Value::I64(axis.probe.axis as i64)),
                    ("position".into(), position(axis.position_um)),
                    (
                        "participant".into(),
                        Value::Bool(plan.participants.contains(&axis.device)),
                    ),
                ]))
            })
            .collect::<Vec<_>>();
        Value::Map(BTreeMap::from([
            ("phase".into(), Value::String(phase.into())),
            ("axes".into(), Value::List(axes)),
            (
                "sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
        ]))
    }

    fn apply_timing_sequence_step(&mut self, plan: &TimingPlan, first: bool) -> Result<Value> {
        let mut writes = Vec::new();
        for sequence in self.local_timing_sequences(plan) {
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
                "zaber multi-axis timing start sequence".into()
            } else {
                "zaber multi-axis timing stop sequence".into()
            }),
            writes,
            commit: CommitMode::Immediate,
        })
    }

    fn finish_motion(&mut self, index: usize, final_position_um: f64) {
        self.axes[index].busy = true;
        self.pending
            .push_back(DriverEvent::Event(Event::Log(LogEvent {
                driver: Some(self.id),
                message: format!("zaber axis {} status BUSY", self.axes[index].probe.axis),
            })));
        self.axes[index].position_um = final_position_um;
        self.axes[index].busy = false;
        self.pending
            .push_back(DriverEvent::Event(Event::Log(LogEvent {
                driver: Some(self.id),
                message: format!("zaber axis {} status IDLE", self.axes[index].probe.axis),
            })));
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

impl Driver for ZaberAsciiMultiAxisDriver {
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
            label: "zaber-ascii-multi-axis-serial".into(),
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
                ("terminator".into(), Value::String("CRLF".into())),
                ("axis_count".into(), Value::I64(self.axes.len() as i64)),
                (
                    "remultiplexing".into(),
                    Value::String(
                        "one serial resource serializes commands for multiple logical axes".into(),
                    ),
                ),
                (
                    "support_scope".into(),
                    Value::String(
                        "ASCII motion control and status readback for configured/probed axes"
                            .into(),
                    ),
                ),
            ]),
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if self.axes.iter().any(|axis| axis.device == device) {
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
                        description: format!("zaber multi-axis read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("zaber multi-axis write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "zaber multi-axis coalesced state set".into(),
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
                            Error::new(ErrorCode::Unsupported, "unknown Zaber capability")
                        })?;
                    match (&candidate.kind, request) {
                        (CapabilityKind::StageMove, CapabilityRequest::StageMove(request)) => {
                            if request.target.len() != 1 {
                                return Err(Error::new(
                                    ErrorCode::InvalidCommand,
                                    "Zaber multi-axis StageMove expects one target",
                                ));
                            }
                        }
                        (
                            CapabilityKind::GenericCommand,
                            CapabilityRequest::GenericCommand(request),
                        ) => {
                            self.validate_generic_command(*device, request)?;
                        }
                        (CapabilityKind::GenericCommand, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Zaber multi-axis GenericCommand expects a GenericCommandRequest",
                            ));
                        }
                        (
                            CapabilityKind::StageHome | CapabilityKind::StageStop,
                            CapabilityRequest::None,
                        ) => {}
                        (CapabilityKind::StageMove, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Zaber StageMove expects a StageMoveRequest",
                            ));
                        }
                        (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Zaber home/stop capabilities take no request",
                            ));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported Zaber capability",
                            ));
                        }
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("zaber multi-axis invoke {}", capability.0),
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
                    if let Some((index, query)) = self.query_for_property(device, &key)? {
                        self.send_axis(index, query.clone())?;
                        self.read_query_reply(index, &query)?;
                    }
                    last = self.read_property(device, &key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    last = self.apply_state_set(StateSet {
                        name: Some("single Zaber multi-axis write".into()),
                        writes: vec![StateWrite {
                            device,
                            property: key.clone(),
                            value,
                        }],
                        commit: CommitMode::Immediate,
                    })?;
                    if let Value::Map(map) = &last {
                        if let Some(value) = map.values().next() {
                            self.emit_property(device, &key, value.clone());
                        }
                    }
                }
                Command::ApplyStateSet(set) => {
                    last = self.apply_state_set(set)?;
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    let index = self.axis_index(device)?;
                    let kind = self
                        .capabilities(device)
                        .into_iter()
                        .find(|candidate| candidate.id == capability)
                        .map(|candidate| candidate.kind)
                        .ok_or_else(|| {
                            Error::new(ErrorCode::Unsupported, "unknown Zaber capability")
                        })?;
                    last = match (kind, request) {
                        (CapabilityKind::StageMove, CapabilityRequest::StageMove(request)) => {
                            self.stage_move(device, &request)?
                        }
                        (
                            CapabilityKind::GenericCommand,
                            CapabilityRequest::GenericCommand(request),
                        ) => self.apply_generic_command(device, request)?,
                        (CapabilityKind::GenericCommand, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Zaber multi-axis GenericCommand expects a GenericCommandRequest",
                            ));
                        }
                        (CapabilityKind::StageHome, CapabilityRequest::None) => {
                            self.send_axis(index, protocol::ZaberCommand::Home)?;
                            if self.read_command_reply(index, "home")? {
                                self.refresh_position_readback(index)?;
                            } else {
                                self.finish_motion(index, 0.0);
                                self.emit_property(device, "position", position(0.0));
                            }
                            Value::String("homed".into())
                        }
                        (CapabilityKind::StageStop, CapabilityRequest::None) => {
                            self.send_axis(index, protocol::ZaberCommand::Stop)?;
                            if self.read_command_reply(index, "stop")? {
                                self.refresh_position_readback(index)?;
                            } else {
                                self.axes[index].busy = false;
                                self.emit_property(device, "busy", Value::Bool(false));
                            }
                            Value::String("stopped".into())
                        }
                        (CapabilityKind::StageMove, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Zaber StageMove expects a StageMoveRequest",
                            ));
                        }
                        (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Zaber home/stop capabilities take no request",
                            ));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported Zaber capability",
                            ));
                        }
                    };
                }
                Command::Arm(_) | Command::Start(_) | Command::Stop(_) => unreachable!(),
            }
        }
        self.pending
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
                description: "zaber multi-axis timing arm summary".into(),
                payload: self.timing_summary(plan, "arm"),
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
                description: "zaber multi-axis timing start sequence".into(),
                payload: Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "start")),
                    ("changed".into(), changed),
                ])),
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
                description: "zaber multi-axis timing stop sequence".into(),
                payload: Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "stop")),
                    ("changed".into(), changed),
                ])),
            }],
        })
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        if let Ok(bytes) = self.serial.read_available() {
            for line in self.codec.push(&bytes) {
                self.pending
                    .push_back(DriverEvent::Event(Event::Log(LogEvent {
                        driver: Some(self.id),
                        message: format!("zaber multi-axis serial: {line}"),
                    })));
            }
        }
        self.pending.drain(..).collect()
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

fn acceleration_property_range(
    key: &str,
    display_name: &str,
    unit: Option<&str>,
    writable: bool,
    min_um_s2: f64,
    max_um_s2: f64,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Acceleration,
        unit,
        writable,
        Some(Range {
            min: acceleration(min_um_s2),
            max: acceleration(max_um_s2),
        }),
    )
}

fn position(value_um: f64) -> Value {
    Value::Position(Position::from_micrometers(value_um))
}

fn velocity(value_um_s: f64) -> Value {
    Value::Velocity(Velocity::from_micrometers_per_second(value_um_s))
}

fn acceleration(value_um_s2: f64) -> Value {
    Value::Acceleration(Acceleration::from_micrometers_per_second_squared(
        value_um_s2,
    ))
}

fn zaber_refresh_properties(command: &str) -> Result<&'static [&'static str]> {
    const POSITION: &[&str] = &["position"];
    const VELOCITY: &[&str] = &["velocity"];
    const ACCELERATION: &[&str] = &["acceleration"];
    const STATUS: &[&str] = &["status"];
    const WARNING: &[&str] = &["warning", "warning_summary"];
    const AXIS_SUMMARY: &[&str] = &["axis_summary"];
    const READBACKS: &[&str] = &["position", "velocity", "acceleration", "axis_summary"];

    match command {
        "refresh_position" => Ok(POSITION),
        "refresh_velocity" => Ok(VELOCITY),
        "refresh_acceleration" => Ok(ACCELERATION),
        "refresh_status" => Ok(STATUS),
        "refresh_warning" => Ok(WARNING),
        "refresh_axis_summary" => Ok(AXIS_SUMMARY),
        "refresh_readbacks" => Ok(READBACKS),
        other => Err(Error::new(
            ErrorCode::Unsupported,
            format!(
                "Zaber GenericCommand supports refresh_readbacks, refresh_position, refresh_velocity, refresh_acceleration, refresh_status, refresh_warning, and refresh_axis_summary; got {other}"
            ),
        )),
    }
}

fn zaber_refresh_result(
    command: String,
    properties: &[&str],
    mut values: BTreeMap<String, Value>,
) -> Value {
    let mut result = BTreeMap::from([
        ("command".into(), Value::String(command)),
        (
            "completion_basis".into(),
            Value::String("selected Zaber ASCII get readback".into()),
        ),
        (
            "properties".into(),
            Value::List(
                properties
                    .iter()
                    .map(|property| Value::String((*property).into()))
                    .collect(),
            ),
        ),
        ("values".into(), Value::Map(values.clone())),
    ]);
    if properties.len() == 1 {
        let property = properties[0];
        if let Some(value) = values.remove(property) {
            result.insert("property".into(), Value::String(property.into()));
            result.insert("value".into(), value);
        }
    }
    Value::Map(result)
}

fn position_um(value: &Value) -> Result<f64> {
    match value {
        Value::Position(position) => Ok(position.micrometers()),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            "expected typed position value",
        )),
    }
}

fn velocity_um_s(value: &Value) -> Result<f64> {
    match value {
        Value::Velocity(velocity) => Ok(velocity.micrometers_per_second()),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            "expected typed velocity value",
        )),
    }
}

fn acceleration_um_s2(value: &Value) -> Result<f64> {
    match value {
        Value::Acceleration(acceleration) => Ok(acceleration.micrometers_per_second_squared()),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            "expected typed acceleration value",
        )),
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

fn position_config_um(device: &DeviceConfig, key: &str, legacy_key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::Position(value)) => Some(value.micrometers()),
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => f64_prop(device, legacy_key),
    }
}

fn velocity_config_um_s(device: &DeviceConfig, key: &str, legacy_key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::Velocity(value)) => Some(value.micrometers_per_second()),
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => f64_prop(device, legacy_key),
    }
}

fn acceleration_config_um_s2(device: &DeviceConfig, key: &str, legacy_key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::Acceleration(value)) => Some(value.micrometers_per_second_squared()),
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => f64_prop(device, legacy_key),
    }
}

fn u8_prop(device: &DeviceConfig, key: &str) -> Option<u8> {
    u64_prop(device, key).and_then(|value| value.try_into().ok())
}

fn u32_prop(device: &DeviceConfig, key: &str) -> Option<u32> {
    u64_prop(device, key).and_then(|value| value.try_into().ok())
}

fn u64_prop(device: &DeviceConfig, key: &str) -> Option<u64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => (*value >= 0).then_some(*value as u64),
        Some(Value::F64(value)) if value.is_finite() && *value >= 0.0 => Some(*value as u64),
        _ => None,
    }
}
