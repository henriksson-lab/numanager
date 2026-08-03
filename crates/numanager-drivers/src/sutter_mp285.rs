use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::{ScriptedSerial, SerialIo};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
#[cfg(feature = "os-serial")]
use std::time::Duration;

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const BAUD: u32 = 9_600;
    pub const TERMINATOR: u8 = 0x0d;
    pub const STOP: u8 = 0x03;

    #[derive(Debug, Clone, PartialEq)]
    pub struct Mp285Probe {
        pub firmware: String,
        pub serial_number: String,
        pub resolution_nm_per_microstep: u32,
        pub velocity_microsteps_per_s: u32,
        pub travel_um: f64,
    }

    impl Mp285Probe {
        pub fn simulated() -> Self {
            Self {
                firmware: "MP-285 fixture".into(),
                serial_number: "MP285-SIM-0001".into(),
                resolution_nm_per_microstep: 10,
                velocity_microsteps_per_s: 18_000,
                travel_um: 25_000.0,
            }
        }

        pub fn microsteps_per_um(&self) -> f64 {
            1000.0 / self.resolution_nm_per_microstep as f64
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Mp285ProbeResult {
        pub probe: Mp285Probe,
        pub status: Mp285Status,
        pub x_um: f64,
        pub y_um: f64,
        pub z_um: f64,
        pub velocity_ack: bool,
        pub replies: Vec<(String, Vec<u8>)>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Mp285Status {
        pub raw: Vec<u8>,
        pub text: String,
        pub high_resolution: Option<bool>,
        pub velocity_microsteps_per_s: Option<u32>,
    }

    impl Mp285ProbeResult {
        pub fn from_replies(
            template: &Mp285Probe,
            replies: &[(impl AsRef<str>, impl AsRef<[u8]>)],
        ) -> Result<Self> {
            let mut probe = template.clone();
            let mut status = Mp285Status {
                raw: Vec::new(),
                text: String::new(),
                high_resolution: None,
                velocity_microsteps_per_s: None,
            };
            let mut x_um = 0.0;
            let mut y_um = 0.0;
            let mut z_um = 0.0;
            let mut velocity_ack = false;
            let mut stored = Vec::new();

            for (command, reply) in replies {
                let command = command.as_ref();
                let reply = reply.as_ref();
                stored.push((command.to_string(), reply.to_vec()));
                if command == "RESET" {
                    ack_result(reply)?;
                } else if command == "STATUS" {
                    status = decode_status(reply);
                    if let Some(high_resolution) = status.high_resolution {
                        probe.resolution_nm_per_microstep = if high_resolution { 10 } else { 50 };
                    }
                    if let Some(velocity) = status.velocity_microsteps_per_s {
                        probe.velocity_microsteps_per_s = velocity;
                    }
                    if !status.text.is_empty() {
                        probe.firmware = status.text.clone();
                    }
                } else if command == "POSITION" {
                    (x_um, y_um, z_um) = decode_position(reply, probe.microsteps_per_um())?;
                } else if command == "SET_VELOCITY" {
                    ack_result(reply)?;
                    velocity_ack = true;
                }
            }

            Ok(Self {
                probe,
                status,
                x_um,
                y_um,
                z_um,
                velocity_ack,
                replies: stored,
            })
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MotionMode {
        Absolute,
        Relative,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum Mp285Command {
        Reset,
        GetStatus,
        GetPosition,
        MoveAbsolute {
            x_um: f64,
            y_um: f64,
            z_um: f64,
        },
        SetVelocity {
            microsteps_per_s: u32,
            high_resolution: bool,
        },
        SetOrigin,
        Stop,
    }

    pub fn encode(command: &Mp285Command, microsteps_per_um: f64) -> Vec<u8> {
        match command {
            Mp285Command::Reset => vec![0x00, TERMINATOR],
            Mp285Command::GetStatus => vec![b's', TERMINATOR],
            Mp285Command::GetPosition => vec![b'c', TERMINATOR],
            Mp285Command::MoveAbsolute { x_um, y_um, z_um } => {
                let mut bytes = Vec::with_capacity(14);
                bytes.push(b'm');
                bytes.extend_from_slice(&microsteps(*x_um, microsteps_per_um).to_le_bytes());
                bytes.extend_from_slice(&microsteps(*y_um, microsteps_per_um).to_le_bytes());
                bytes.extend_from_slice(&microsteps(*z_um, microsteps_per_um).to_le_bytes());
                bytes.push(TERMINATOR);
                bytes
            }
            Mp285Command::SetVelocity {
                microsteps_per_s,
                high_resolution,
            } => {
                let mut value = (*microsteps_per_s).min(0x7fff) as u16;
                if *high_resolution {
                    value |= 0x8000;
                }
                vec![b'V', (value >> 8) as u8, (value & 0xff) as u8, TERMINATOR]
            }
            Mp285Command::SetOrigin => vec![b'o', TERMINATOR],
            Mp285Command::Stop => vec![STOP, TERMINATOR],
        }
    }

    pub fn microsteps(um: f64, microsteps_per_um: f64) -> i32 {
        (um * microsteps_per_um).round() as i32
    }

    pub fn micrometers(microsteps: i32, microsteps_per_um: f64) -> f64 {
        microsteps as f64 / microsteps_per_um
    }

    pub fn decode_position(reply: &[u8], microsteps_per_um: f64) -> Result<(f64, f64, f64)> {
        if reply.len() < 12 {
            return Err(Error::new(
                ErrorCode::Transport,
                "MP-285 position reply is shorter than 12 bytes",
            ));
        }
        let x = i32::from_le_bytes(reply[0..4].try_into().unwrap());
        let y = i32::from_le_bytes(reply[4..8].try_into().unwrap());
        let z = i32::from_le_bytes(reply[8..12].try_into().unwrap());
        Ok((
            micrometers(x, microsteps_per_um),
            micrometers(y, microsteps_per_um),
            micrometers(z, microsteps_per_um),
        ))
    }

    pub fn ack_result(reply: &[u8]) -> Result<()> {
        match reply.first().copied() {
            Some(TERMINATOR) | Some(0) | None => Ok(()),
            Some(0x30) => Err(Error::new(ErrorCode::Transport, "MP-285 serial overrun")),
            Some(0x31) => Err(Error::new(ErrorCode::Transport, "MP-285 frame error")),
            Some(0x32) => Err(Error::new(ErrorCode::Transport, "MP-285 buffer overrun")),
            Some(0x34) => Err(Error::new(ErrorCode::Transport, "MP-285 bad command")),
            Some(0x38) => Err(Error::new(
                ErrorCode::Cancelled,
                "MP-285 move was interrupted",
            )),
            Some(other) => Err(Error::new(
                ErrorCode::Transport,
                format!("unexpected MP-285 status byte 0x{other:02x}"),
            )),
        }
    }

    pub fn probe_commands(template: &Mp285Probe) -> Vec<(String, Mp285Command, usize)> {
        vec![
            ("STATUS".into(), Mp285Command::GetStatus, 32),
            ("POSITION".into(), Mp285Command::GetPosition, 12),
            (
                "SET_VELOCITY".into(),
                Mp285Command::SetVelocity {
                    microsteps_per_s: template.velocity_microsteps_per_s,
                    high_resolution: template.resolution_nm_per_microstep == 10,
                },
                1,
            ),
        ]
    }

    pub fn probe_script(template: &Mp285Probe) -> Vec<String> {
        probe_commands(template)
            .into_iter()
            .map(|(label, command, _)| format!("{label}: {}", command_label(&command)))
            .collect()
    }

    pub fn execute_probe_script(
        serial: &mut dyn SerialIo,
        template: &Mp285Probe,
        polls_per_command: usize,
    ) -> Result<Mp285ProbeResult> {
        let mut replies = Vec::new();
        for (label, command, expected_len) in probe_commands(template) {
            serial.write(&encode(&command, template.microsteps_per_um()))?;
            let reply = read_expected(serial, expected_len, polls_per_command)?;
            replies.push((label, reply));
        }
        Mp285ProbeResult::from_replies(template, &replies)
    }

    pub fn decode_status(reply: &[u8]) -> Mp285Status {
        let text = String::from_utf8_lossy(reply.get(4..).unwrap_or_default())
            .chars()
            .filter(|ch| ch.is_ascii_graphic() || *ch == ' ')
            .collect::<String>()
            .trim()
            .to_string();
        let velocity = if reply.len() >= 4 {
            Some(u16::from_be_bytes([reply[2], reply[3]]) as u32 & 0x7fff)
        } else {
            None
        };
        let high_resolution = if reply.len() >= 4 {
            Some((u16::from_be_bytes([reply[2], reply[3]]) & 0x8000) != 0)
        } else {
            None
        };
        Mp285Status {
            raw: reply.to_vec(),
            text,
            high_resolution,
            velocity_microsteps_per_s: velocity,
        }
    }

    fn command_label(command: &Mp285Command) -> &'static str {
        match command {
            Mp285Command::Reset => "0x00 CR",
            Mp285Command::GetStatus => "s CR",
            Mp285Command::GetPosition => "c CR",
            Mp285Command::MoveAbsolute { .. } => "m <i32 x,y,z> CR",
            Mp285Command::SetVelocity { .. } => "V <u16 velocity|resolution> CR",
            Mp285Command::SetOrigin => "o CR",
            Mp285Command::Stop => "ETX CR",
        }
    }

    fn read_expected(
        serial: &mut dyn SerialIo,
        expected_len: usize,
        polls_per_command: usize,
    ) -> Result<Vec<u8>> {
        let mut reply = Vec::new();
        for _ in 0..polls_per_command.max(1) {
            reply.extend(serial.read_available()?);
            if reply.len() >= expected_len {
                reply.truncate(expected_len);
                return Ok(reply);
            }
        }
        Err(Error::new(
            ErrorCode::Transport,
            format!(
                "timed out waiting for MP-285 probe reply: got {} of {expected_len} bytes",
                reply.len()
            ),
        ))
    }
}

pub struct Mp285Discovery {
    next_id: DriverId,
    probes: Vec<Mp285ConfiguredProbe>,
}

impl Mp285Discovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![Mp285ConfiguredProbe::simulated()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| device.driver == "sutter-mp285")
            .map(Mp285ConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for Mp285Discovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = probe.label.clone();
                let driver = if probe.connect_real_transport {
                    Box::new(Mp285Driver::serial(id, probe)?) as Box<dyn Driver>
                } else {
                    Box::new(Mp285Driver::configured(id, probe)) as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct Mp285ConfiguredProbe {
    pub label: String,
    pub probe: protocol::Mp285Probe,
    pub endpoint: Option<Mp285SerialEndpoint>,
    pub connect_real_transport: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mp285SerialEndpoint {
    pub port_name: String,
    pub baud_rate: u32,
    pub timeout_ms: u64,
}

impl Mp285ConfiguredProbe {
    pub fn simulated() -> Self {
        Self {
            label: "Simulated Sutter MP-285 manipulator".into(),
            probe: protocol::Mp285Probe::simulated(),
            endpoint: None,
            connect_real_transport: false,
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = protocol::Mp285Probe::simulated();
        probe.firmware = string_prop(device, "firmware").unwrap_or(probe.firmware);
        probe.serial_number = string_prop(device, "serial_number").unwrap_or(probe.serial_number);
        probe.resolution_nm_per_microstep = u32_prop(device, "resolution_nm_per_microstep")
            .unwrap_or(probe.resolution_nm_per_microstep);
        probe.velocity_microsteps_per_s = u32_prop(device, "velocity_microsteps_per_s")
            .unwrap_or(probe.velocity_microsteps_per_s);
        probe.travel_um = position_config_um(device, "travel")
            .or_else(|| f64_prop(device, "travel_um"))
            .unwrap_or(probe.travel_um);

        let endpoint = string_prop(device, "serial_port").map(|port_name| Mp285SerialEndpoint {
            port_name,
            baud_rate: u32_prop(device, "baud_rate").unwrap_or(protocol::BAUD),
            timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(1),
        });

        Ok(Self {
            label: if device.label.is_empty() {
                "Configured Sutter MP-285 manipulator".into()
            } else {
                device.label.clone()
            },
            probe,
            endpoint,
            connect_real_transport: bool_prop(device, "connect").unwrap_or(false),
        })
    }
}

pub struct Mp285Driver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    xy: DeviceId,
    z: DeviceId,
    probe: protocol::Mp285Probe,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
    x_um: f64,
    y_um: f64,
    z_um: f64,
    target_x_um: f64,
    target_y_um: f64,
    target_z_um: f64,
    busy: bool,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
}

impl Mp285Driver {
    pub fn simulated(id: DriverId) -> Self {
        Self::configured(id, Mp285ConfiguredProbe::simulated())
    }

    pub fn configured(id: DriverId, configured: Mp285ConfiguredProbe) -> Self {
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
    pub fn serial(id: DriverId, configured: Mp285ConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "MP-285 serial probe is missing serial_port metadata",
            )
        })?;
        let mut serial = numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(
                endpoint.port_name.clone(),
                endpoint.baud_rate,
            )
            .timeout(Duration::from_millis(endpoint.timeout_ms)),
        )?;
        let probe_result = protocol::execute_probe_script(&mut serial, &configured.probe, 4)?;
        Ok(Self::new_with_transport_metadata(
            id,
            configured.probe,
            Some(endpoint),
            true,
            Box::new(serial),
        )
        .with_probe_result(probe_result))
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, _configured: Mp285ConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "MP-285 real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(id: DriverId, probe: protocol::Mp285Probe, serial: Box<dyn SerialIo>) -> Self {
        Self::new_with_transport_metadata(id, probe, None, false, serial)
    }

    fn new_with_transport_metadata(
        id: DriverId,
        probe: protocol::Mp285Probe,
        endpoint: Option<Mp285SerialEndpoint>,
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
            .unwrap_or(1);
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 930)),
            hub: DeviceId(NodeId(id.0 * 1000 + 931)),
            xy: DeviceId(NodeId(id.0 * 1000 + 932)),
            z: DeviceId(NodeId(id.0 * 1000 + 933)),
            probe,
            serial_port,
            baud_rate,
            serial_timeout_ms,
            connected,
            x_um: 0.0,
            y_um: 0.0,
            z_um: 0.0,
            target_x_um: 0.0,
            target_y_um: 0.0,
            target_z_um: 0.0,
            busy: false,
            next_token: 1,
            pending: VecDeque::new(),
            serial,
        }
    }

    #[cfg(feature = "os-serial")]
    fn with_probe_result(mut self, probe_result: protocol::Mp285ProbeResult) -> Self {
        self.probe = probe_result.probe;
        self.x_um = probe_result.x_um.clamp(0.0, self.probe.travel_um);
        self.y_um = probe_result.y_um.clamp(0.0, self.probe.travel_um);
        self.z_um = probe_result.z_um.clamp(0.0, self.probe.travel_um);
        self.target_x_um = self.x_um;
        self.target_y_um = self.y_um;
        self.target_z_um = self.z_um;
        self.busy = false;
        self
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn microsteps_per_um(&self) -> f64 {
        self.probe.microsteps_per_um()
    }

    fn velocity_um_s(&self, microsteps_per_s: u32) -> f64 {
        microsteps_per_s as f64 / self.microsteps_per_um()
    }

    fn velocity_microsteps_per_s(&self, value: &Value) -> Result<u32> {
        let Value::Velocity(velocity) = value else {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "expected typed velocity value",
            ));
        };
        let microsteps_per_s = (velocity.micrometers_per_second() * self.microsteps_per_um())
            .round()
            .clamp(0.0, 32_767.0);
        Ok(microsteps_per_s as u32)
    }

    fn send(&mut self, command: protocol::Mp285Command) -> Result<()> {
        let bytes = protocol::encode(&command, self.microsteps_per_um());
        self.serial.write(&bytes)
    }

    fn read_optional_reply(&mut self, expected_len: usize) -> Result<Option<Vec<u8>>> {
        let mut reply = Vec::new();
        for _ in 0..4 {
            reply.extend(self.serial.read_available()?);
            if reply.len() >= expected_len {
                reply.truncate(expected_len);
                return Ok(Some(reply));
            }
        }
        if reply.is_empty() {
            Ok(None)
        } else {
            Err(Error::new(
                ErrorCode::Transport,
                format!(
                    "timed out waiting for MP-285 readback: got {} of {expected_len} bytes",
                    reply.len()
                ),
            ))
        }
    }

    fn read_optional_ack(&mut self) -> Result<bool> {
        let bytes = self.serial.read_available()?;
        if bytes.is_empty() {
            return Ok(false);
        }
        protocol::ack_result(&bytes)?;
        Ok(true)
    }

    fn refresh_status_readback(&mut self) -> Result<()> {
        self.send(protocol::Mp285Command::GetStatus)?;
        let Some(reply) = self.read_optional_reply(32)? else {
            return Ok(());
        };
        let status = protocol::decode_status(&reply);
        if let Some(high_resolution) = status.high_resolution {
            self.probe.resolution_nm_per_microstep = if high_resolution { 10 } else { 50 };
        }
        if let Some(velocity) = status.velocity_microsteps_per_s {
            self.probe.velocity_microsteps_per_s = velocity;
        }
        if !status.text.is_empty() {
            self.probe.firmware = status.text;
            self.emit_property(
                self.hub,
                "firmware",
                Value::String(self.probe.firmware.clone()),
            );
        }
        self.emit_property(
            self.hub,
            "resolution",
            resolution(self.probe.resolution_nm_per_microstep),
        );
        self.emit_property(
            self.hub,
            "velocity",
            velocity(self.velocity_um_s(self.probe.velocity_microsteps_per_s)),
        );
        self.emit_property(self.hub, "status_summary", self.status_summary());
        Ok(())
    }

    fn refresh_motion_readback(&mut self) -> Result<()> {
        self.refresh_status_readback()?;
        self.refresh_position_readback()
    }

    fn refresh_position_readback(&mut self) -> Result<()> {
        self.send(protocol::Mp285Command::GetPosition)?;
        let Some(reply) = self.read_optional_reply(12)? else {
            return Ok(());
        };
        let (x_um, y_um, z_um) = protocol::decode_position(&reply, self.microsteps_per_um())?;
        self.x_um = x_um.clamp(0.0, self.probe.travel_um);
        self.y_um = y_um.clamp(0.0, self.probe.travel_um);
        self.z_um = z_um.clamp(0.0, self.probe.travel_um);
        self.emit_property(self.xy, "x", position(self.x_um));
        self.emit_property(self.xy, "y", position(self.y_um));
        self.emit_property(self.z, "z", position(self.z_um));
        self.emit_property(self.hub, "status_summary", self.status_summary());
        Ok(())
    }

    fn validate_generic_command(&self, request: &GenericCommandRequest) -> Result<()> {
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
                "MP-285 GenericCommand does not take parameters",
            ));
        }
        match request.command.as_str() {
            "refresh_readbacks" | "refresh_status" | "refresh_position" => Ok(()),
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "MP-285 GenericCommand supports refresh_readbacks, refresh_status, and refresh_position; got {other}"
                ),
            )),
        }
    }

    fn apply_generic_command(&mut self, request: GenericCommandRequest) -> Result<Value> {
        self.validate_generic_command(&request)?;
        let command_count = match request.command.as_str() {
            "refresh_readbacks" => {
                self.refresh_status_readback()?;
                self.refresh_position_readback()?;
                2
            }
            "refresh_status" => {
                self.refresh_status_readback()?;
                1
            }
            "refresh_position" => {
                self.refresh_position_readback()?;
                1
            }
            _ => unreachable!("validated MP-285 GenericCommand"),
        };
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("commands".into(), Value::I64(command_count)),
            ("state".into(), self.status_summary()),
            (
                "completion_basis".into(),
                Value::String("MP-285 status/position readback".into()),
            ),
        ])))
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        vec![
            DeviceDescriptor {
                id: self.hub,
                driver: self.id,
                label: "sutter-mp285-hub".into(),
                vendor: Some("Sutter Instrument".into()),
                model: Some("MP-285".into()),
                serial: Some(self.probe.serial_number.clone()),
                kinds: vec![
                    "hub".into(),
                    "motion.controller".into(),
                    "serial.binary".into(),
                ],
                properties: vec![
                    property("firmware", "Firmware", ValueType::String, None, false, None),
                    property(
                        "resolution",
                        "Resolution",
                        ValueType::Position,
                        Some("um"),
                        false,
                        None,
                    ),
                    velocity_property("velocity", "Velocity", true, self.velocity_um_s(32_767)),
                    property(
                        "status_summary",
                        "Status summary",
                        ValueType::Map,
                        None,
                        false,
                        None,
                    ),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                ],
                metadata: BTreeMap::from([
                    ("baud_rate".into(), Value::I64(protocol::BAUD as i64)),
                    (
                        "protocol".into(),
                        Value::String("Sutter MP-285 binary serial".into()),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.xy,
                driver: self.id,
                label: "sutter-mp285-xy".into(),
                vendor: Some("Sutter Instrument".into()),
                model: Some("MP-285 XY".into()),
                serial: Some(self.probe.serial_number.clone()),
                kinds: vec!["stage.xy".into(), "axis.x".into(), "axis.y".into()],
                properties: vec![
                    sequenceable_position_property("x", "X position", true, self.probe.travel_um),
                    sequenceable_position_property("y", "Y position", true, self.probe.travel_um),
                    position_property("target_x", "Target X", true, self.probe.travel_um),
                    position_property("target_y", "Target Y", true, self.probe.travel_um),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                ],
                metadata: BTreeMap::from([
                    (
                        "depends_on".into(),
                        Value::String("sutter-mp285-hub".into()),
                    ),
                    ("travel".into(), position(self.probe.travel_um)),
                    ("legacy_travel_um".into(), position(self.probe.travel_um)),
                    (
                        "microstep_size".into(),
                        position(1.0 / self.microsteps_per_um()),
                    ),
                    (
                        "legacy_microstep_size_um".into(),
                        position(1.0 / self.microsteps_per_um()),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.z,
                driver: self.id,
                label: "sutter-mp285-z".into(),
                vendor: Some("Sutter Instrument".into()),
                model: Some("MP-285 Z".into()),
                serial: Some(self.probe.serial_number.clone()),
                kinds: vec!["stage.z".into(), "axis.z".into()],
                properties: vec![
                    sequenceable_position_property("z", "Z position", true, self.probe.travel_um),
                    position_property("target_z", "Target Z", true, self.probe.travel_um),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                ],
                metadata: BTreeMap::from([
                    (
                        "depends_on".into(),
                        Value::String("sutter-mp285-hub".into()),
                    ),
                    ("travel".into(), position(self.probe.travel_um)),
                    ("legacy_travel_um".into(), position(self.probe.travel_um)),
                    (
                        "microstep_size".into(),
                        position(1.0 / self.microsteps_per_um()),
                    ),
                    (
                        "legacy_microstep_size_um".into(),
                        position(1.0 / self.microsteps_per_um()),
                    ),
                ]),
            },
        ]
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        match (device, key) {
            (device, "firmware") if device == self.hub => {
                Ok(Value::String(self.probe.firmware.clone()))
            }
            (device, "resolution") if device == self.hub => {
                Ok(resolution(self.probe.resolution_nm_per_microstep))
            }
            (device, "velocity") if device == self.hub => Ok(velocity(
                self.velocity_um_s(self.probe.velocity_microsteps_per_s),
            )),
            (device, "status_summary") if device == self.hub => Ok(self.status_summary()),
            (device, "busy") if device == self.hub || device == self.xy || device == self.z => {
                Ok(Value::Bool(self.busy))
            }
            (device, "x") if device == self.xy => Ok(position(self.x_um)),
            (device, "y") if device == self.xy => Ok(position(self.y_um)),
            (device, "target_x") if device == self.xy => Ok(position(self.target_x_um)),
            (device, "target_y") if device == self.xy => Ok(position(self.target_y_um)),
            (device, "z") if device == self.z => Ok(position(self.z_um)),
            (device, "target_z") if device == self.z => Ok(position(self.target_z_um)),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown MP-285 property {key}"),
            )),
        }
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
            (device, "velocity", value) if device == self.hub => {
                self.probe.velocity_microsteps_per_s = self.velocity_microsteps_per_s(value)?;
                self.send(protocol::Mp285Command::SetVelocity {
                    microsteps_per_s: self.probe.velocity_microsteps_per_s,
                    high_resolution: self.probe.resolution_nm_per_microstep == 10,
                })?;
                Ok(velocity(
                    self.velocity_um_s(self.probe.velocity_microsteps_per_s),
                ))
            }
            (device, "x", value) if device == self.xy => {
                let x = position_um(value)?.clamp(0.0, self.probe.travel_um);
                self.move_xyz(x, self.y_um, self.z_um)?;
                Ok(position(self.x_um))
            }
            (device, "y", value) if device == self.xy => {
                let y = position_um(value)?.clamp(0.0, self.probe.travel_um);
                self.move_xyz(self.x_um, y, self.z_um)?;
                Ok(position(self.y_um))
            }
            (device, "target_x", value) if device == self.xy => {
                self.target_x_um = position_um(value)?.clamp(0.0, self.probe.travel_um);
                Ok(position(self.target_x_um))
            }
            (device, "target_y", value) if device == self.xy => {
                self.target_y_um = position_um(value)?.clamp(0.0, self.probe.travel_um);
                Ok(position(self.target_y_um))
            }
            (device, "z", value) if device == self.z => {
                let z = position_um(value)?.clamp(0.0, self.probe.travel_um);
                self.move_xyz(self.x_um, self.y_um, z)?;
                Ok(position(self.z_um))
            }
            (device, "target_z", value) if device == self.z => {
                self.target_z_um = position_um(value)?.clamp(0.0, self.probe.travel_um);
                Ok(position(self.target_z_um))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid MP-285 write {key}"),
            )),
        }
    }

    fn move_xyz(&mut self, x_um: f64, y_um: f64, z_um: f64) -> Result<()> {
        self.target_x_um = x_um;
        self.target_y_um = y_um;
        self.target_z_um = z_um;
        self.send(protocol::Mp285Command::MoveAbsolute { x_um, y_um, z_um })?;
        self.read_optional_ack()?;
        self.finish_motion(x_um, y_um, z_um);
        self.refresh_motion_readback()?;
        Ok(())
    }

    fn validate_stage_move(&self, device: DeviceId, request: &StageMoveRequest) -> Result<()> {
        if request.target.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "MP-285 StageMove target must contain at least one axis",
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
                "MP-285 StageMove acceleration profiles are not supported by the documented velocity command surface",
            ));
        }
        for axis in request.target.keys() {
            match (device, axis) {
                (device, StageAxis::X | StageAxis::Y) if device == self.xy => {}
                (device, StageAxis::Z) if device == self.z => {}
                (device, StageAxis::Custom(name))
                    if device == self.xy && (name == "x" || name == "y") => {}
                (device, StageAxis::Custom(name)) if device == self.z && name == "z" => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "MP-285 StageMove axis does not belong to the target device",
                    ))
                }
            }
        }
        Ok(())
    }

    fn apply_stage_move_profile(&mut self, request: &StageMoveRequest) -> Result<()> {
        let Some(profile) = &request.profile else {
            return Ok(());
        };
        let Some(velocity) = profile.velocity.as_ref() else {
            return Ok(());
        };
        let velocity = Value::Velocity(velocity.clone());
        self.probe.velocity_microsteps_per_s = self.velocity_microsteps_per_s(&velocity)?;
        self.send(protocol::Mp285Command::SetVelocity {
            microsteps_per_s: self.probe.velocity_microsteps_per_s,
            high_resolution: self.probe.resolution_nm_per_microstep == 10,
        })
    }

    fn stage_move(&mut self, device: DeviceId, request: StageMoveRequest) -> Result<Value> {
        self.validate_stage_move(device, &request)?;
        self.apply_stage_move_profile(&request)?;
        let mut x = self.x_um;
        let mut y = self.y_um;
        let mut z = self.z_um;
        if device == self.xy {
            for (axis, target) in &request.target {
                match axis {
                    StageAxis::X => x = target.micrometers(),
                    StageAxis::Y => y = target.micrometers(),
                    StageAxis::Custom(name) if name == "x" => x = target.micrometers(),
                    StageAxis::Custom(name) if name == "y" => y = target.micrometers(),
                    _ => {}
                }
            }
            if request.relative {
                x = self.x_um + x;
                y = self.y_um + y;
            }
        } else if device == self.z {
            z = request
                .target
                .values()
                .next()
                .expect("validated one Z target")
                .micrometers();
            if request.relative {
                z += self.z_um;
            }
        } else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "MP-285 StageMove target device must be XY or Z stage",
            ));
        }
        self.move_xyz(
            x.clamp(0.0, self.probe.travel_um),
            y.clamp(0.0, self.probe.travel_um),
            z.clamp(0.0, self.probe.travel_um),
        )?;
        self.emit_property(self.xy, "x", position(self.x_um));
        self.emit_property(self.xy, "y", position(self.y_um));
        self.emit_property(self.z, "z", position(self.z_um));
        Ok(Value::Map(BTreeMap::from([
            (
                "mode".into(),
                Value::String(if request.relative {
                    "relative".into()
                } else {
                    "absolute".into()
                }),
            ),
            ("x".into(), position(self.x_um)),
            ("y".into(), position(self.y_um)),
            ("z".into(), position(self.z_um)),
            (
                "velocity".into(),
                velocity(self.velocity_um_s(self.probe.velocity_microsteps_per_s)),
            ),
        ])))
    }

    fn apply_state_set(&mut self, set: StateSet) -> Result<Value> {
        let mut next_x = self.x_um;
        let mut next_y = self.y_um;
        let mut next_z = self.z_um;
        let mut next_target_x = self.target_x_um;
        let mut next_target_y = self.target_y_um;
        let mut next_target_z = self.target_z_um;
        let mut next_velocity = self.probe.velocity_microsteps_per_s;

        for write in &set.writes {
            self.validate_write(write.device, &write.property, &write.value)?;
            match (write.device, write.property.as_str(), &write.value) {
                (device, "velocity", value) if device == self.hub => {
                    next_velocity = self.velocity_microsteps_per_s(value)?;
                }
                (device, "x", value) if device == self.xy => {
                    next_x = position_um(value)?.clamp(0.0, self.probe.travel_um);
                }
                (device, "y", value) if device == self.xy => {
                    next_y = position_um(value)?.clamp(0.0, self.probe.travel_um);
                }
                (device, "target_x", value) if device == self.xy => {
                    next_target_x = position_um(value)?.clamp(0.0, self.probe.travel_um);
                }
                (device, "target_y", value) if device == self.xy => {
                    next_target_y = position_um(value)?.clamp(0.0, self.probe.travel_um);
                }
                (device, "z", value) if device == self.z => {
                    next_z = position_um(value)?.clamp(0.0, self.probe.travel_um);
                }
                (device, "target_z", value) if device == self.z => {
                    next_target_z = position_um(value)?.clamp(0.0, self.probe.travel_um);
                }
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "unsupported MP-285 state-set write",
                    ))
                }
            }
        }

        let mut changed = BTreeMap::new();
        if next_velocity != self.probe.velocity_microsteps_per_s {
            self.probe.velocity_microsteps_per_s = next_velocity;
            self.send(protocol::Mp285Command::SetVelocity {
                microsteps_per_s: next_velocity,
                high_resolution: self.probe.resolution_nm_per_microstep == 10,
            })?;
            changed.insert(
                "hub:velocity".into(),
                velocity(self.velocity_um_s(next_velocity)),
            );
            self.emit_property(
                self.hub,
                "velocity",
                velocity(self.velocity_um_s(next_velocity)),
            );
        }
        if next_target_x != self.target_x_um {
            self.target_x_um = next_target_x;
            changed.insert("xy:target_x".into(), position(next_target_x));
            self.emit_property(self.xy, "target_x", position(next_target_x));
        }
        if next_target_y != self.target_y_um {
            self.target_y_um = next_target_y;
            changed.insert("xy:target_y".into(), position(next_target_y));
            self.emit_property(self.xy, "target_y", position(next_target_y));
        }
        if next_target_z != self.target_z_um {
            self.target_z_um = next_target_z;
            changed.insert("z:target_z".into(), position(next_target_z));
            self.emit_property(self.z, "target_z", position(next_target_z));
        }
        if next_x != self.x_um || next_y != self.y_um || next_z != self.z_um {
            self.move_xyz(next_x, next_y, next_z)?;
            changed.insert("xy:x".into(), position(self.x_um));
            changed.insert("xy:y".into(), position(self.y_um));
            changed.insert("z:z".into(), position(self.z_um));
            self.emit_property(self.xy, "x", position(self.x_um));
            self.emit_property(self.xy, "y", position(self.y_um));
            self.emit_property(self.z, "z", position(self.z_um));
        }
        Ok(Value::Map(changed))
    }

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| sequence.device == self.xy || sequence.device == self.z)
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequences(plan) {
            match (sequence.device, sequence.property.as_str()) {
                (device, "x" | "y") if device == self.xy => {}
                (device, "z") if device == self.z => {}
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "MP-285 timing sequences can only target x, y, or z",
                    ))
                }
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
            (
                "xy_participant".into(),
                Value::Bool(plan.participants.contains(&self.xy)),
            ),
            (
                "z_participant".into(),
                Value::Bool(plan.participants.contains(&self.z)),
            ),
            ("x".into(), position(self.x_um)),
            ("y".into(), position(self.y_um)),
            ("z".into(), position(self.z_um)),
            (
                "sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
        ]))
    }

    fn status_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            (
                "firmware".into(),
                Value::String(self.probe.firmware.clone()),
            ),
            (
                "serial_number".into(),
                Value::String(self.probe.serial_number.clone()),
            ),
            (
                "resolution".into(),
                resolution(self.probe.resolution_nm_per_microstep),
            ),
            (
                "legacy_resolution_nm_per_microstep".into(),
                resolution(self.probe.resolution_nm_per_microstep),
            ),
            (
                "high_resolution".into(),
                Value::Bool(self.probe.resolution_nm_per_microstep == 10),
            ),
            (
                "velocity_microsteps_per_s".into(),
                Value::I64(self.probe.velocity_microsteps_per_s as i64),
            ),
            (
                "velocity".into(),
                velocity(self.velocity_um_s(self.probe.velocity_microsteps_per_s)),
            ),
            ("busy".into(), Value::Bool(self.busy)),
            ("x".into(), position(self.x_um)),
            ("y".into(), position(self.y_um)),
            ("z".into(), position(self.z_um)),
            ("target_x".into(), position(self.target_x_um)),
            ("target_y".into(), position(self.target_y_um)),
            ("target_z".into(), position(self.target_z_um)),
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
                "mp285 timing start sequence".into()
            } else {
                "mp285 timing stop sequence".into()
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
                "unknown MP-285 capability",
            ));
        };
        match (capability.kind, request) {
            (CapabilityKind::StageMove, CapabilityRequest::StageMove(request))
                if device == self.xy || device == self.z =>
            {
                self.stage_move(device, request)
            }
            (CapabilityKind::StageMove, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "MP-285 StageMove expects a StageMoveRequest",
            )),
            (CapabilityKind::StageStop, CapabilityRequest::None)
                if device == self.xy || device == self.z =>
            {
                self.send(protocol::Mp285Command::Stop)?;
                self.read_optional_ack()?;
                self.busy = false;
                self.emit_busy(false);
                self.refresh_motion_readback()?;
                Ok(Value::String("stopped".into()))
            }
            (CapabilityKind::StageStop, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "MP-285 stop capability takes no request",
            )),
            (CapabilityKind::GenericCommand, CapabilityRequest::GenericCommand(request))
                if device == self.hub =>
            {
                self.apply_generic_command(request)
            }
            (CapabilityKind::GenericCommand, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "MP-285 GenericCommand expects GenericCommandRequest",
            )),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported MP-285 capability",
            )),
        }
    }

    fn finish_motion(&mut self, x_um: f64, y_um: f64, z_um: f64) {
        self.busy = true;
        self.emit_busy(true);
        self.pending
            .push_back(DriverEvent::Event(Event::Log(LogEvent {
                driver: Some(self.id),
                message: "mp285 motion accepted; waiting for controller ACK".into(),
            })));
        self.x_um = x_um;
        self.y_um = y_um;
        self.z_um = z_um;
        self.busy = false;
        self.emit_busy(false);
        self.pending
            .push_back(DriverEvent::Event(Event::Log(LogEvent {
                driver: Some(self.id),
                message: "mp285 controller ACK".into(),
            })));
    }

    fn emit_busy(&mut self, value: bool) {
        self.emit_property(self.hub, "busy", Value::Bool(value));
        self.emit_property(self.xy, "busy", Value::Bool(value));
        self.emit_property(self.z, "busy", Value::Bool(value));
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

impl Driver for Mp285Driver {
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
            label: "sutter-mp285-serial".into(),
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
                ("terminator".into(), Value::String("CR".into())),
                (
                    "completion".into(),
                    Value::String("motion commands complete on controller ACK/status byte".into()),
                ),
                (
                    "startup_readback_supported".into(),
                    Value::List(
                        protocol::probe_script(&self.probe)
                            .into_iter()
                            .map(Value::String)
                            .collect(),
                    ),
                ),
            ]),
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.hub {
            vec![capability(1, device, CapabilityKind::GenericCommand)]
        } else if device == self.xy || device == self.z {
            vec![
                capability(1, device, CapabilityKind::StageMove),
                capability(3, device, CapabilityKind::StageStop),
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
                        description: format!("mp285 read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("mp285 write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "mp285 remultiplexed xyz state set".into(),
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
                            Error::new(ErrorCode::Unsupported, "unknown MP-285 capability")
                        })?;
                    match (&candidate.kind, request) {
                        (CapabilityKind::StageMove, CapabilityRequest::StageMove(request)) => {
                            self.validate_stage_move(*device, request)?;
                        }
                        (CapabilityKind::StageStop, CapabilityRequest::None) => {}
                        (
                            CapabilityKind::GenericCommand,
                            CapabilityRequest::GenericCommand(request),
                        ) if *device == self.hub => {
                            self.validate_generic_command(request)?;
                        }
                        (CapabilityKind::StageMove, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "MP-285 StageMove expects a StageMoveRequest",
                            ));
                        }
                        (CapabilityKind::StageStop, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "MP-285 stop capability takes no request",
                            ));
                        }
                        (CapabilityKind::GenericCommand, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "MP-285 GenericCommand expects GenericCommandRequest",
                            ));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported MP-285 capability",
                            ));
                        }
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("mp285 invoke {}", capability.0),
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
                            CapabilityRequest::GenericCommand(request) => Value::String(
                                match request.command.as_str() {
                                    "refresh_readbacks" => "status,position",
                                    "refresh_status" => "status",
                                    "refresh_position" => "position",
                                    _ => "unsupported",
                                }
                                .into(),
                            ),
                            _ => Value::Null,
                        },
                    });
                }
                Command::Arm(plan) => {
                    self.validate_timing_plan(plan)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "mp285 timing arm summary".into(),
                        payload: self.timing_summary(plan, "arm"),
                    });
                }
                Command::Start(_) | Command::Stop(_) => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "MP-285 direct timing transitions are runtime-owned",
                    ));
                }
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
                    if device == self.xy && (key == "x" || key == "y") {
                        self.refresh_position_readback()?;
                    } else if device == self.z && key == "z" {
                        self.refresh_position_readback()?;
                    } else if device == self.hub
                        && (key == "firmware"
                            || key == "resolution"
                            || key == "velocity"
                            || key == "status_summary")
                    {
                        self.refresh_status_readback()?;
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
            if !bytes.is_empty() {
                match protocol::ack_result(&bytes) {
                    Ok(()) => self
                        .pending
                        .push_back(DriverEvent::Event(Event::Log(LogEvent {
                            driver: Some(self.id),
                            message: format!("mp285 serial {} byte(s)", bytes.len()),
                        }))),
                    Err(error) => {
                        self.pending
                            .push_back(DriverEvent::Event(Event::Fault(FaultEvent {
                                device: Some(self.hub),
                                report: error.into(),
                            })))
                    }
                }
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
                description: "mp285 timing arm summary".into(),
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
                description: "mp285 timing start sequence".into(),
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
                description: "mp285 timing stop sequence".into(),
                payload: Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "stop")),
                    ("changed".into(), changed),
                ])),
            }],
        })
    }
}

fn capability(id: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    CapabilityDescriptor::new(CapabilityId(id), device, kind, ValueType::Map)
}

fn position(value_um: f64) -> Value {
    Value::Position(Position::from_micrometers(value_um))
}

fn resolution(nm_per_microstep: u32) -> Value {
    position(nm_per_microstep as f64 * 1e-3)
}

fn velocity(value_um_s: f64) -> Value {
    Value::Velocity(Velocity::from_micrometers_per_second(value_um_s))
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

fn position_property(key: &str, display_name: &str, writable: bool, max_um: f64) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Position,
        Some("um"),
        writable,
        Some(Range {
            min: position(0.0),
            max: position(max_um),
        }),
    )
}

fn sequenceable_position_property(
    key: &str,
    display_name: &str,
    writable: bool,
    max_um: f64,
) -> PropertySchema {
    let mut schema = position_property(key, display_name, writable, max_um);
    schema.sequenceable = true;
    schema
}

fn velocity_property(
    key: &str,
    display_name: &str,
    writable: bool,
    max_um_s: f64,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Velocity,
        Some("um/s"),
        writable,
        Some(Range {
            min: velocity(0.0),
            max: velocity(max_um_s),
        }),
    )
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

fn position_config_um(device: &DeviceConfig, key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::Position(position)) => Some(position.micrometers()),
        _ => None,
    }
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
