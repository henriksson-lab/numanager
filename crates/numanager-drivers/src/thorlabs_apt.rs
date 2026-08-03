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

    pub const DEVICE_HOSTPC: u8 = 0x01;
    pub const DEVICE_CONTROLLER: u8 = 0x11;
    pub const DEVICE_CHANNEL0: u8 = 0x21;
    pub const DATA_FOLLOWS: u8 = 0x80;

    pub const STATUS_IN_MOTION_CW: u32 = 0x0000_0010;
    pub const STATUS_IN_MOTION_CCW: u32 = 0x0000_0020;
    pub const STATUS_JOGGING_CW: u32 = 0x0000_0040;
    pub const STATUS_JOGGING_CCW: u32 = 0x0000_0080;
    pub const STATUS_CONNECTED: u32 = 0x0000_0100;
    pub const STATUS_HOMING: u32 = 0x0000_0200;
    pub const STATUS_HOMED: u32 = 0x0000_0400;
    pub const STATUS_INTERLOCK: u32 = 0x0000_1000;
    pub const STATUS_POSITION_ERROR: u32 = 0x0001_0000;

    #[derive(Debug, Clone, PartialEq)]
    pub struct AptProbe {
        pub serial_number: String,
        pub model: String,
        pub channel: u8,
        pub travel_um: f64,
        pub encoder_counts_per_um: f64,
        pub homed: bool,
        pub connected: bool,
    }

    impl AptProbe {
        pub fn configured_fixture() -> Self {
            Self {
                serial_number: "APT-CONFIG-0001".into(),
                model: "Thorlabs APT-compatible motor".into(),
                channel: 1,
                travel_um: 25_000.0,
                encoder_counts_per_um: 100.0,
                homed: true,
                connected: true,
            }
        }

        pub fn counts(&self, um: f64) -> i32 {
            (um * self.encoder_counts_per_um).round() as i32
        }

        pub fn micrometers(&self, counts: i32) -> f64 {
            counts as f64 / self.encoder_counts_per_um
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum AptCommand {
        RequestHardwareInfo,
        EnableChannel,
        MoveHome,
        RequestPosition,
        MoveAbsolute {
            position_counts: i32,
        },
        MoveRelative {
            distance_counts: i32,
        },
        KeepAlive,
        StopImmediate,
        RequestStatus,
        RequestVelocityProfile,
        SetVelocityProfile {
            min_velocity_counts_s: u32,
            acceleration_counts_s2: u32,
            max_velocity_counts_s: u32,
        },
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct AptFrame {
        pub message_id: u16,
        pub param1: u8,
        pub param2: u8,
        pub destination: u8,
        pub source: u8,
        pub payload: Vec<u8>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct AptStatus {
        pub channel: u16,
        pub position_counts: i32,
        pub velocity_counts_s: u16,
        pub status_bits: u32,
    }

    impl AptStatus {
        pub fn is_busy(&self) -> bool {
            self.status_bits
                & (STATUS_IN_MOTION_CW
                    | STATUS_IN_MOTION_CCW
                    | STATUS_JOGGING_CW
                    | STATUS_JOGGING_CCW
                    | STATUS_HOMING)
                != 0
        }

        pub fn has_position_error(&self) -> bool {
            self.status_bits & STATUS_POSITION_ERROR != 0
        }

        pub fn is_homed(&self) -> bool {
            self.status_bits & STATUS_HOMED != 0
        }
    }

    pub fn encode(channel: u8, command: &AptCommand) -> Vec<u8> {
        match command {
            AptCommand::RequestHardwareInfo => short(0x0005, 0, 0, DEVICE_CONTROLLER),
            AptCommand::EnableChannel => short(0x0210, channel, 1, channel_dest(channel)),
            AptCommand::MoveHome => short(0x0443, channel, 0, channel_dest(channel)),
            AptCommand::RequestPosition => short(0x0411, channel, 0, channel_dest(channel)),
            AptCommand::MoveAbsolute { position_counts } => long(
                0x0453,
                channel_dest(channel),
                &channel_position(channel, *position_counts),
            ),
            AptCommand::MoveRelative { distance_counts } => long(
                0x0448,
                channel_dest(channel),
                &channel_position(channel, *distance_counts),
            ),
            AptCommand::KeepAlive => short(0x0492, 0, 0, channel_dest(channel)),
            AptCommand::StopImmediate => short(0x0465, channel, 1, channel_dest(channel)),
            AptCommand::RequestStatus => short(0x0490, channel, 0, channel_dest(channel)),
            AptCommand::RequestVelocityProfile => short(0x0414, channel, 0, channel_dest(channel)),
            AptCommand::SetVelocityProfile {
                min_velocity_counts_s,
                acceleration_counts_s2,
                max_velocity_counts_s,
            } => {
                let mut payload = Vec::with_capacity(14);
                payload.extend_from_slice(&(channel as u16).to_le_bytes());
                payload.extend_from_slice(&min_velocity_counts_s.to_le_bytes());
                payload.extend_from_slice(&acceleration_counts_s2.to_le_bytes());
                payload.extend_from_slice(&max_velocity_counts_s.to_le_bytes());
                long(0x0413, channel_dest(channel), &payload)
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct AptHardwareInfo {
        pub serial_number: Option<String>,
        pub model: Option<String>,
        pub raw_payload: Vec<u8>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct AptVelocityProfile {
        pub channel: u16,
        pub min_velocity_counts_s: u32,
        pub acceleration_counts_s2: u32,
        pub max_velocity_counts_s: u32,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct AptProbeResult {
        pub channel: u8,
        pub hardware: Option<AptHardwareInfo>,
        pub position_counts: Option<i32>,
        pub status: Option<AptStatus>,
        pub velocity_profile: Option<AptVelocityProfile>,
        pub frames: Vec<AptFrame>,
    }

    pub fn short(message_id: u16, param1: u8, param2: u8, destination: u8) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(6);
        bytes.extend_from_slice(&message_id.to_le_bytes());
        bytes.push(param1);
        bytes.push(param2);
        bytes.push(destination);
        bytes.push(DEVICE_HOSTPC);
        bytes
    }

    pub fn long(message_id: u16, destination: u8, payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(6 + payload.len());
        bytes.extend_from_slice(&message_id.to_le_bytes());
        bytes.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        bytes.push(destination | DATA_FOLLOWS);
        bytes.push(DEVICE_HOSTPC);
        bytes.extend_from_slice(payload);
        bytes
    }

    pub fn channel_dest(channel: u8) -> u8 {
        DEVICE_CHANNEL0 + channel.saturating_sub(1)
    }

    pub fn channel_position(channel: u8, position_counts: i32) -> Vec<u8> {
        let mut payload = Vec::with_capacity(6);
        payload.extend_from_slice(&(channel as u16).to_le_bytes());
        payload.extend_from_slice(&position_counts.to_le_bytes());
        payload
    }

    pub fn parse_frames(buffer: &mut Vec<u8>, bytes: &[u8]) -> Result<Vec<AptFrame>> {
        buffer.extend_from_slice(bytes);
        let mut frames = Vec::new();
        while buffer.len() >= 6 {
            let message_id = u16::from_le_bytes([buffer[0], buffer[1]]);
            let param1 = buffer[2];
            let param2 = buffer[3];
            let destination = buffer[4];
            let source = buffer[5];
            let payload_len = if destination & DATA_FOLLOWS != 0 {
                u16::from_le_bytes([param1, param2]) as usize
            } else {
                0
            };
            if buffer.len() < 6 + payload_len {
                break;
            }
            let payload = if payload_len == 0 {
                Vec::new()
            } else {
                buffer[6..6 + payload_len].to_vec()
            };
            buffer.drain(..6 + payload_len);
            frames.push(AptFrame {
                message_id,
                param1,
                param2,
                destination,
                source,
                payload,
            });
        }
        Ok(frames)
    }

    pub fn parse_status_payload(payload: &[u8]) -> Result<AptStatus> {
        if payload.len() != 14 {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("invalid APT status payload length {}", payload.len()),
            ));
        }
        Ok(AptStatus {
            channel: u16::from_le_bytes([payload[0], payload[1]]),
            position_counts: i32::from_le_bytes([payload[2], payload[3], payload[4], payload[5]]),
            velocity_counts_s: u16::from_le_bytes([payload[6], payload[7]]),
            status_bits: u32::from_le_bytes([payload[10], payload[11], payload[12], payload[13]]),
        })
    }

    pub fn parse_position_payload(payload: &[u8]) -> Result<(u16, i32)> {
        if payload.len() != 6 {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("invalid APT position payload length {}", payload.len()),
            ));
        }
        Ok((
            u16::from_le_bytes([payload[0], payload[1]]),
            i32::from_le_bytes([payload[2], payload[3], payload[4], payload[5]]),
        ))
    }

    pub fn parse_velocity_payload(payload: &[u8]) -> Result<AptVelocityProfile> {
        if payload.len() != 14 {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("invalid APT velocity payload length {}", payload.len()),
            ));
        }
        Ok(AptVelocityProfile {
            channel: u16::from_le_bytes([payload[0], payload[1]]),
            min_velocity_counts_s: u32::from_le_bytes([
                payload[2], payload[3], payload[4], payload[5],
            ]),
            acceleration_counts_s2: u32::from_le_bytes([
                payload[6], payload[7], payload[8], payload[9],
            ]),
            max_velocity_counts_s: u32::from_le_bytes([
                payload[10],
                payload[11],
                payload[12],
                payload[13],
            ]),
        })
    }

    pub fn parse_hardware_info_payload(payload: &[u8]) -> AptHardwareInfo {
        let serial_number = if payload.len() >= 4 {
            Some(u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]).to_string())
        } else {
            None
        };
        let model = payload.get(4..).and_then(|tail| {
            let end = tail
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(tail.len());
            let text = String::from_utf8_lossy(&tail[..end]).trim().to_string();
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        });
        AptHardwareInfo {
            serial_number,
            model,
            raw_payload: payload.to_vec(),
        }
    }

    pub fn probe_commands(_channel: u8) -> Vec<AptCommand> {
        vec![
            AptCommand::RequestHardwareInfo,
            AptCommand::EnableChannel,
            AptCommand::RequestPosition,
            AptCommand::RequestStatus,
            AptCommand::RequestVelocityProfile,
        ]
    }

    pub fn probe_script(channel: u8) -> Vec<String> {
        probe_commands(channel)
            .iter()
            .map(|command| hex_bytes(&encode(channel, command)))
            .collect()
    }

    pub fn execute_probe_script(
        serial: &mut dyn SerialIo,
        channel: u8,
        polls_per_command: usize,
    ) -> Result<AptProbeResult> {
        let mut rx_buffer = Vec::new();
        let mut result = AptProbeResult {
            channel,
            hardware: None,
            position_counts: None,
            status: None,
            velocity_profile: None,
            frames: Vec::new(),
        };
        for command in probe_commands(channel) {
            serial.write(&encode(channel, &command))?;
            let expected = expected_reply(&command);
            if let Some(expected) = expected {
                let frame =
                    read_expected_frame(serial, &mut rx_buffer, expected, polls_per_command)?;
                apply_probe_frame(&mut result, &frame)?;
                result.frames.push(frame);
            }
        }
        Ok(result)
    }

    fn expected_reply(command: &AptCommand) -> Option<u16> {
        match command {
            AptCommand::RequestHardwareInfo => Some(0x0006),
            AptCommand::RequestPosition => Some(0x0412),
            AptCommand::RequestStatus => Some(0x0491),
            AptCommand::RequestVelocityProfile => Some(0x0415),
            AptCommand::EnableChannel
            | AptCommand::MoveHome
            | AptCommand::MoveAbsolute { .. }
            | AptCommand::MoveRelative { .. }
            | AptCommand::KeepAlive
            | AptCommand::StopImmediate
            | AptCommand::SetVelocityProfile { .. } => None,
        }
    }

    fn apply_probe_frame(result: &mut AptProbeResult, frame: &AptFrame) -> Result<()> {
        match frame.message_id {
            0x0006 => result.hardware = Some(parse_hardware_info_payload(&frame.payload)),
            0x0412 => {
                let (_, position_counts) = parse_position_payload(&frame.payload)?;
                result.position_counts = Some(position_counts);
            }
            0x0415 => result.velocity_profile = Some(parse_velocity_payload(&frame.payload)?),
            0x0491 | 0x0464 => result.status = Some(parse_status_payload(&frame.payload)?),
            _ => {}
        }
        Ok(())
    }

    fn read_expected_frame(
        serial: &mut dyn SerialIo,
        rx_buffer: &mut Vec<u8>,
        expected: u16,
        polls_per_command: usize,
    ) -> Result<AptFrame> {
        for _ in 0..polls_per_command.max(1) {
            for frame in parse_frames(rx_buffer, &serial.read_available()?)? {
                if frame.message_id == expected {
                    return Ok(frame);
                }
            }
        }
        Err(Error::new(
            ErrorCode::Transport,
            format!("timed out waiting for APT frame 0x{expected:04x}"),
        ))
    }

    fn hex_bytes(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

pub struct ThorlabsAptDiscovery {
    next_id: DriverId,
    probes: Vec<ThorlabsAptConfiguredProbe>,
}

impl ThorlabsAptDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![ThorlabsAptConfiguredProbe::fixture()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| {
                matches!(
                    device.driver.as_str(),
                    "thorlabs_apt" | "thorlabs_apt_motor"
                )
            })
            .map(ThorlabsAptConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for ThorlabsAptDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver = if configured.connect_real_transport {
                    let endpoint = configured.endpoint.clone().ok_or_else(|| {
                        Error::new(
                            ErrorCode::InvalidProperty,
                            "Thorlabs APT config requires serial_port when connect is true",
                        )
                    })?;
                    Box::new(ThorlabsAptDriver::serial(
                        id,
                        configured.probe,
                        endpoint.port_name,
                        endpoint.baud_rate,
                        endpoint.timeout_ms,
                    )?) as Box<dyn Driver>
                } else {
                    Box::new(ThorlabsAptDriver::configured(id, configured)) as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct ThorlabsAptConfiguredProbe {
    pub label: String,
    pub endpoint: Option<ThorlabsAptSerialEndpoint>,
    pub connect_real_transport: bool,
    probe: protocol::AptProbe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThorlabsAptSerialEndpoint {
    pub port_name: String,
    pub baud_rate: u32,
    pub timeout_ms: u64,
}

impl ThorlabsAptConfiguredProbe {
    pub fn fixture() -> Self {
        Self {
            label: "Configured Thorlabs APT motor fixture".into(),
            endpoint: None,
            connect_real_transport: false,
            probe: protocol::AptProbe::configured_fixture(),
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = Self::fixture();
        configured.label = if device.label.is_empty() {
            "Configured Thorlabs APT motor".into()
        } else {
            device.label.clone()
        };
        configured.probe.serial_number = string_prop(device, "serial_number")
            .unwrap_or_else(|| configured.probe.serial_number.clone());
        configured.probe.model =
            string_prop(device, "model").unwrap_or_else(|| configured.probe.model.clone());
        configured.probe.channel = u8_prop(device, "channel").unwrap_or(configured.probe.channel);
        configured.probe.travel_um =
            position_config_um(device, "travel", "travel_um").unwrap_or(configured.probe.travel_um);
        if let Some(step_um) =
            position_config_um(device, "encoder_step_size", "encoder_step_size_um")
        {
            if step_um <= 0.0 {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Thorlabs APT encoder_step_size must be positive",
                ));
            }
            configured.probe.encoder_counts_per_um = 1.0 / step_um;
        }
        configured.probe.homed = bool_prop(device, "homed").unwrap_or(configured.probe.homed);
        configured.probe.connected =
            bool_prop(device, "connected").unwrap_or(configured.probe.connected);
        configured.endpoint =
            string_prop(device, "serial_port").map(|port_name| ThorlabsAptSerialEndpoint {
                port_name,
                baud_rate: u32_prop(device, "baud_rate").unwrap_or(115_200),
                timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(100),
            });
        configured.connect_real_transport = bool_prop(device, "connect").unwrap_or(false);
        Ok(configured)
    }
}

pub struct ThorlabsAptDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    stage: DeviceId,
    probe: protocol::AptProbe,
    position_um: f64,
    target_um: f64,
    min_velocity_um_s: f64,
    acceleration_um_s2: f64,
    max_velocity_um_s: f64,
    status_bits: u32,
    busy: bool,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    rx_buffer: Vec<u8>,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
}

impl ThorlabsAptDriver {
    pub fn configured_fixture(id: DriverId) -> Self {
        Self::configured(id, ThorlabsAptConfiguredProbe::fixture())
    }

    pub fn configured(id: DriverId, configured: ThorlabsAptConfiguredProbe) -> Self {
        Self::new_configured(id, configured, Box::new(ScriptedSerial::new()), false)
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(
        id: DriverId,
        probe: protocol::AptProbe,
        port_name: impl Into<String>,
        baud_rate: u32,
        timeout_ms: u64,
    ) -> Result<Self> {
        let port_name = port_name.into();
        let mut serial = numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(port_name.clone(), baud_rate)
                .timeout(Duration::from_millis(timeout_ms)),
        )?;
        let probe_result = protocol::execute_probe_script(&mut serial, probe.channel, 4)?;
        let mut driver = Self::new(id, probe, Box::new(serial)).with_probe_result(probe_result);
        driver.serial_port = Some(port_name);
        driver.baud_rate = baud_rate;
        driver.serial_timeout_ms = timeout_ms;
        driver.connected = true;
        Ok(driver)
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(
        _id: DriverId,
        _probe: protocol::AptProbe,
        _port_name: impl Into<String>,
        _baud_rate: u32,
        _timeout_ms: u64,
    ) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Thorlabs APT real serial transport requires the numanager-drivers os-serial feature",
        ))
    }

    pub fn new(id: DriverId, probe: protocol::AptProbe, serial: Box<dyn SerialIo>) -> Self {
        let mut status_bits = protocol::STATUS_CONNECTED;
        if probe.homed {
            status_bits |= protocol::STATUS_HOMED;
        }
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 1601)),
            hub: DeviceId(NodeId(id.0 * 1000 + 1610)),
            stage: DeviceId(NodeId(id.0 * 1000 + 1611)),
            probe,
            position_um: 0.0,
            target_um: 0.0,
            min_velocity_um_s: 0.0,
            acceleration_um_s2: 10_000.0,
            max_velocity_um_s: 5_000.0,
            status_bits,
            busy: false,
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            rx_buffer: Vec::new(),
            serial_port: None,
            baud_rate: 115_200,
            serial_timeout_ms: 100,
            connected: false,
        }
    }

    pub fn new_configured(
        id: DriverId,
        configured: ThorlabsAptConfiguredProbe,
        serial: Box<dyn SerialIo>,
        connected: bool,
    ) -> Self {
        let mut driver = Self::new(id, configured.probe, serial);
        driver.serial_port = configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.port_name.clone());
        driver.baud_rate = configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.baud_rate)
            .unwrap_or(115_200);
        driver.serial_timeout_ms = configured
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.timeout_ms)
            .unwrap_or(100);
        driver.connected = connected;
        driver
    }

    #[cfg(feature = "os-serial")]
    fn with_probe_result(mut self, probe_result: protocol::AptProbeResult) -> Self {
        if let Some(hardware) = probe_result.hardware {
            if let Some(serial_number) = hardware.serial_number {
                self.probe.serial_number = serial_number;
            }
            if let Some(model) = hardware.model {
                self.probe.model = model;
            }
        }
        if let Some(position_counts) = probe_result.position_counts {
            self.position_um = self
                .probe
                .micrometers(position_counts)
                .clamp(0.0, self.probe.travel_um);
            self.target_um = self.position_um;
        }
        if let Some(status) = probe_result.status {
            self.status_bits = status.status_bits;
            self.busy = status.is_busy();
            self.probe.homed = status.is_homed();
            self.probe.connected = self.status_bits & protocol::STATUS_CONNECTED != 0;
            self.position_um = self
                .probe
                .micrometers(status.position_counts)
                .clamp(0.0, self.probe.travel_um);
            self.target_um = self.position_um;
        }
        if let Some(profile) = probe_result.velocity_profile {
            self.min_velocity_um_s =
                profile.min_velocity_counts_s as f64 / self.probe.encoder_counts_per_um;
            self.acceleration_um_s2 =
                profile.acceleration_counts_s2 as f64 / self.probe.encoder_counts_per_um;
            self.max_velocity_um_s =
                profile.max_velocity_counts_s as f64 / self.probe.encoder_counts_per_um;
        }
        self
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn send(&mut self, command: protocol::AptCommand) -> Result<()> {
        self.serial
            .write(&protocol::encode(self.probe.channel, &command))
    }

    fn read_expected_frame_if_available(&mut self, expected: u16) -> Result<()> {
        for _ in 0..4 {
            let bytes = self.serial.read_available()?;
            if bytes.is_empty() {
                return Ok(());
            }
            for frame in protocol::parse_frames(&mut self.rx_buffer, &bytes)? {
                self.apply_frame_readback(&frame)?;
                if frame.message_id == expected {
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    fn apply_frame_readback(&mut self, frame: &protocol::AptFrame) -> Result<()> {
        match frame.message_id {
            0x0006 => {
                let hardware = protocol::parse_hardware_info_payload(&frame.payload);
                if let Some(serial_number) = hardware.serial_number {
                    self.probe.serial_number = serial_number;
                    self.emit_property(
                        self.hub,
                        "serial_number",
                        Value::String(self.probe.serial_number.clone()),
                    );
                }
                if let Some(model) = hardware.model {
                    self.probe.model = model;
                    self.emit_property(self.hub, "model", Value::String(self.probe.model.clone()));
                }
            }
            0x0412 => {
                let (_, position_counts) = protocol::parse_position_payload(&frame.payload)?;
                self.position_um = self
                    .probe
                    .micrometers(position_counts)
                    .clamp(0.0, self.probe.travel_um);
                self.emit_property(self.stage, "position", position(self.position_um));
            }
            0x0415 => {
                let profile = protocol::parse_velocity_payload(&frame.payload)?;
                self.min_velocity_um_s =
                    profile.min_velocity_counts_s as f64 / self.probe.encoder_counts_per_um;
                self.acceleration_um_s2 =
                    profile.acceleration_counts_s2 as f64 / self.probe.encoder_counts_per_um;
                self.max_velocity_um_s =
                    profile.max_velocity_counts_s as f64 / self.probe.encoder_counts_per_um;
                self.emit_property(self.stage, "min_velocity", velocity(self.min_velocity_um_s));
                self.emit_property(
                    self.stage,
                    "acceleration",
                    acceleration(self.acceleration_um_s2),
                );
                self.emit_property(self.stage, "max_velocity", velocity(self.max_velocity_um_s));
            }
            0x0491 | 0x0464 => {
                let status = protocol::parse_status_payload(&frame.payload)?;
                self.position_um = self
                    .probe
                    .micrometers(status.position_counts)
                    .clamp(0.0, self.probe.travel_um);
                self.status_bits = status.status_bits;
                self.busy = status.is_busy();
                self.emit_property(self.hub, "busy", Value::Bool(self.busy));
                self.emit_property(self.stage, "busy", Value::Bool(self.busy));
                self.emit_property(self.stage, "position", position(self.position_um));
                self.emit_property(
                    self.stage,
                    "homed",
                    Value::Bool(self.status_bits & protocol::STATUS_HOMED != 0),
                );
                self.emit_property(
                    self.stage,
                    "connected",
                    Value::Bool(self.status_bits & protocol::STATUS_CONNECTED != 0),
                );
                self.emit_property(
                    self.stage,
                    "position_error",
                    Value::Bool(self.status_bits & protocol::STATUS_POSITION_ERROR != 0),
                );
                self.emit_property(
                    self.stage,
                    "status_bits",
                    Value::I64(self.status_bits as i64),
                );
                self.emit_property(self.stage, "status_summary", self.status_summary());
            }
            _ => {
                self.pending
                    .push_back(DriverEvent::Event(Event::Log(LogEvent {
                        driver: Some(self.id),
                        message: format!("thorlabs apt frame 0x{:04x}", frame.message_id),
                    })));
            }
        }
        Ok(())
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        vec![
            DeviceDescriptor {
                id: self.hub,
                driver: self.id,
                label: "thorlabs-apt-hub".into(),
                vendor: Some("Thorlabs".into()),
                model: Some(self.probe.model.clone()),
                serial: Some(self.probe.serial_number.clone()),
                kinds: vec![
                    "hub".into(),
                    "motion.controller".into(),
                    "binary.apt".into(),
                ],
                properties: vec![
                    property("model", "Model", ValueType::String, None, false, None),
                    property(
                        "serial_number",
                        "Serial number",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                ],
                metadata: BTreeMap::from([
                    ("channel".into(), Value::I64(self.probe.channel as i64)),
                    (
                        "encoder_step_size".into(),
                        position(1.0 / self.probe.encoder_counts_per_um),
                    ),
                    (
                        "legacy_encoder_step_size_um".into(),
                        position(1.0 / self.probe.encoder_counts_per_um),
                    ),
                    (
                        "startup_readback_supported".into(),
                        Value::List(
                            protocol::probe_script(self.probe.channel)
                                .into_iter()
                                .map(Value::String)
                                .collect(),
                        ),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.stage,
                driver: self.id,
                label: "thorlabs-apt-axis-1".into(),
                vendor: Some("Thorlabs".into()),
                model: Some(self.probe.model.clone()),
                serial: Some(self.probe.serial_number.clone()),
                kinds: vec!["axis.x".into(), "stage.x".into(), "motion.apt".into()],
                properties: vec![
                    sequenceable_position_property(
                        "position",
                        "Position",
                        true,
                        self.probe.travel_um,
                    ),
                    position_property("target", "Target", true, self.probe.travel_um),
                    velocity_property("min_velocity", "Minimum velocity", true, 100_000.0),
                    acceleration_property("acceleration", "Acceleration", true, 1_000_000.0),
                    velocity_property("max_velocity", "Maximum velocity", true, 100_000.0),
                    property("homed", "Homed", ValueType::Bool, None, false, None),
                    property("connected", "Connected", ValueType::Bool, None, false, None),
                    property(
                        "position_error",
                        "Position error",
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
                    property("busy", "Busy", ValueType::Bool, None, false, None),
                ],
                metadata: BTreeMap::from([
                    ("travel".into(), position(self.probe.travel_um)),
                    ("legacy_travel_um".into(), position(self.probe.travel_um)),
                    (
                        "encoder_step_size".into(),
                        position(1.0 / self.probe.encoder_counts_per_um),
                    ),
                    (
                        "legacy_encoder_step_size_um".into(),
                        position(1.0 / self.probe.encoder_counts_per_um),
                    ),
                ]),
            },
        ]
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        match (device, key) {
            (device, "model") if device == self.hub => Ok(Value::String(self.probe.model.clone())),
            (device, "serial_number") if device == self.hub => {
                Ok(Value::String(self.probe.serial_number.clone()))
            }
            (device, "busy") if device == self.hub || device == self.stage => {
                Ok(Value::Bool(self.busy))
            }
            (device, "position") if device == self.stage => Ok(position(self.position_um)),
            (device, "target") if device == self.stage => Ok(position(self.target_um)),
            (device, "min_velocity") if device == self.stage => {
                Ok(velocity(self.min_velocity_um_s))
            }
            (device, "acceleration") if device == self.stage => {
                Ok(acceleration(self.acceleration_um_s2))
            }
            (device, "max_velocity") if device == self.stage => {
                Ok(velocity(self.max_velocity_um_s))
            }
            (device, "homed") if device == self.stage => {
                Ok(Value::Bool(self.status_bits & protocol::STATUS_HOMED != 0))
            }
            (device, "connected") if device == self.stage => Ok(Value::Bool(
                self.status_bits & protocol::STATUS_CONNECTED != 0,
            )),
            (device, "position_error") if device == self.stage => Ok(Value::Bool(
                self.status_bits & protocol::STATUS_POSITION_ERROR != 0,
            )),
            (device, "status_bits") if device == self.stage => {
                Ok(Value::I64(self.status_bits as i64))
            }
            (device, "status_summary") if device == self.stage => Ok(self.status_summary()),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Thorlabs APT property {key}"),
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
            (device, "position", value) if device == self.stage => {
                let position_um = position_um(value)?.clamp(0.0, self.probe.travel_um);
                self.move_absolute(position_um)?;
                Ok(position(self.position_um))
            }
            (device, "target", value) if device == self.stage => {
                self.target_um = position_um(value)?.clamp(0.0, self.probe.travel_um);
                Ok(position(self.target_um))
            }
            (device, "min_velocity", value) if device == self.stage => {
                self.min_velocity_um_s = velocity_um_s(value)?;
                self.set_velocity_profile()?;
                Ok(velocity(self.min_velocity_um_s))
            }
            (device, "acceleration", value) if device == self.stage => {
                self.acceleration_um_s2 = acceleration_um_s2(value)?;
                self.set_velocity_profile()?;
                Ok(acceleration(self.acceleration_um_s2))
            }
            (device, "max_velocity", value) if device == self.stage => {
                self.max_velocity_um_s = velocity_um_s(value)?;
                self.set_velocity_profile()?;
                Ok(velocity(self.max_velocity_um_s))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid Thorlabs APT write {key}"),
            )),
        }
    }

    fn move_absolute(&mut self, position_um: f64) -> Result<()> {
        self.target_um = position_um;
        self.send(protocol::AptCommand::KeepAlive)?;
        self.send(protocol::AptCommand::MoveAbsolute {
            position_counts: self.probe.counts(position_um),
        })?;
        self.finish_motion(
            position_um,
            "thorlabs apt MGMSG_MOT_MOVE_COMPLETED received",
        )
    }

    fn set_velocity_profile(&mut self) -> Result<()> {
        self.send(protocol::AptCommand::SetVelocityProfile {
            min_velocity_counts_s: self.probe.counts(self.min_velocity_um_s).max(0) as u32,
            acceleration_counts_s2: self.probe.counts(self.acceleration_um_s2).max(0) as u32,
            max_velocity_counts_s: self.probe.counts(self.max_velocity_um_s).max(0) as u32,
        })?;
        self.issue_read_command(self.stage, "max_velocity")
    }

    fn move_relative(&mut self, distance_um: f64) -> Result<()> {
        let final_position_um = (self.position_um + distance_um).clamp(0.0, self.probe.travel_um);
        let clamped_distance_um = final_position_um - self.position_um;
        self.target_um = final_position_um;
        self.send(protocol::AptCommand::KeepAlive)?;
        self.send(protocol::AptCommand::MoveRelative {
            distance_counts: self.probe.counts(clamped_distance_um),
        })?;
        self.finish_motion(
            final_position_um,
            "thorlabs apt MGMSG_MOT_MOVE_COMPLETED received",
        )
    }

    fn validate_stage_move(&self, device: DeviceId, request: &StageMoveRequest) -> Result<()> {
        if device != self.stage {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Thorlabs APT StageMove targets the stage axis device",
            ));
        }
        if request.target.len() != 1 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Thorlabs APT StageMove expects exactly one axis target",
            ));
        }
        let Some((axis, _)) = request.target.iter().next() else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Thorlabs APT StageMove target must contain one axis",
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
                "Thorlabs APT StageMove supports only the configured X axis",
            ));
        }
        Ok(())
    }

    fn apply_stage_move_profile(&mut self, request: &StageMoveRequest) -> Result<()> {
        let Some(profile) = &request.profile else {
            return Ok(());
        };
        if let Some(velocity) = &profile.velocity {
            self.max_velocity_um_s = velocity.micrometers_per_second();
        }
        if let Some(acceleration) = &profile.acceleration {
            self.acceleration_um_s2 = acceleration.micrometers_per_second_squared();
        }
        self.set_velocity_profile()
    }

    fn stage_move(&mut self, device: DeviceId, request: StageMoveRequest) -> Result<Value> {
        self.validate_stage_move(device, &request)?;
        self.apply_stage_move_profile(&request)?;
        let target_um = request
            .target
            .values()
            .next()
            .expect("validated one target")
            .micrometers();
        if request.relative {
            self.move_relative(target_um)?;
        } else {
            self.move_absolute(target_um.clamp(0.0, self.probe.travel_um))?;
        }
        self.emit_property(self.stage, "position", position(self.position_um));
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
            ("max_velocity".into(), velocity(self.max_velocity_um_s)),
            ("acceleration".into(), acceleration(self.acceleration_um_s2)),
        ])))
    }

    fn apply_state_set(&mut self, set: StateSet) -> Result<Value> {
        let mut next_position = self.position_um;
        let mut position_changed = false;
        let mut changed = BTreeMap::new();

        for write in set.writes {
            self.validate_write(write.device, &write.property, &write.value)?;
            match (write.device, write.property.as_str(), &write.value) {
                (device, "position", value) if device == self.stage => {
                    next_position = position_um(value)?.clamp(0.0, self.probe.travel_um);
                    position_changed = true;
                }
                _ => {
                    let value = self.write_property(write.device, &write.property, &write.value)?;
                    self.emit_property(write.device, &write.property, value.clone());
                    changed.insert(format!("{}:{}", (write.device.0).0, write.property), value);
                }
            }
        }

        if position_changed {
            self.move_absolute(next_position)?;
            self.emit_property(self.stage, "position", position(self.position_um));
            changed.insert(
                format!("{}:position", (self.stage.0).0),
                position(self.position_um),
            );
        }

        Ok(Value::Map(changed))
    }

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| sequence.device == self.stage)
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequences(plan) {
            if sequence.property != "position" {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Thorlabs APT timing plans only support position sequences",
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
            ("stage".into(), Value::I64(self.stage.0 .0 as i64)),
            (
                "stage_participant".into(),
                Value::Bool(plan.participants.contains(&self.stage)),
            ),
            ("position".into(), position(self.position_um)),
            ("target".into(), position(self.target_um)),
            (
                "sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
        ]))
    }

    fn status_summary(&self) -> Value {
        let moving = self.status_bits
            & (protocol::STATUS_IN_MOTION_CW
                | protocol::STATUS_IN_MOTION_CCW
                | protocol::STATUS_JOGGING_CW
                | protocol::STATUS_JOGGING_CCW
                | protocol::STATUS_HOMING)
            != 0;
        Value::Map(BTreeMap::from([
            ("status_bits".into(), Value::I64(self.status_bits as i64)),
            ("busy".into(), Value::Bool(self.busy)),
            ("moving".into(), Value::Bool(moving)),
            (
                "homed".into(),
                Value::Bool(self.status_bits & protocol::STATUS_HOMED != 0),
            ),
            (
                "connected".into(),
                Value::Bool(self.status_bits & protocol::STATUS_CONNECTED != 0),
            ),
            (
                "interlock".into(),
                Value::Bool(self.status_bits & protocol::STATUS_INTERLOCK != 0),
            ),
            (
                "position_error".into(),
                Value::Bool(self.status_bits & protocol::STATUS_POSITION_ERROR != 0),
            ),
            ("position".into(), position(self.position_um)),
            ("target".into(), position(self.target_um)),
            ("min_velocity".into(), velocity(self.min_velocity_um_s)),
            ("max_velocity".into(), velocity(self.max_velocity_um_s)),
            ("acceleration".into(), acceleration(self.acceleration_um_s2)),
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
                "thorlabs apt timing start sequence".into()
            } else {
                "thorlabs apt timing stop sequence".into()
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
                "unknown Thorlabs APT capability",
            ));
        };
        match (capability.kind, request) {
            (CapabilityKind::StageMove, CapabilityRequest::StageMove(request))
                if device == self.stage =>
            {
                self.stage_move(device, request)
            }
            (CapabilityKind::StageMove, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Thorlabs APT StageMove expects a StageMoveRequest",
            )),
            (CapabilityKind::StageHome, CapabilityRequest::None) if device == self.stage => {
                self.send(protocol::AptCommand::MoveHome)?;
                self.finish_motion(0.0, "thorlabs apt home completed")?;
                self.status_bits |= protocol::STATUS_HOMED;
                self.emit_property(self.stage, "position", position(self.position_um));
                self.refresh_motion_readback()?;
                Ok(Value::String("homed".into()))
            }
            (CapabilityKind::StageStop, CapabilityRequest::None) if device == self.stage => {
                self.send(protocol::AptCommand::StopImmediate)?;
                self.busy = false;
                self.status_bits &= !(protocol::STATUS_IN_MOTION_CW
                    | protocol::STATUS_IN_MOTION_CCW
                    | protocol::STATUS_JOGGING_CW
                    | protocol::STATUS_JOGGING_CCW
                    | protocol::STATUS_HOMING);
                self.refresh_motion_readback()?;
                Ok(Value::String("stopped".into()))
            }
            (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Thorlabs APT home/stop capabilities take no request",
            )),
            (CapabilityKind::GenericCommand, CapabilityRequest::GenericCommand(request))
                if device == self.stage =>
            {
                self.apply_generic_command(request)
            }
            (CapabilityKind::GenericCommand, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Thorlabs APT GenericCommand expects a GenericCommandRequest",
            )),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Thorlabs APT capability",
            )),
        }
    }

    fn issue_read_command(&mut self, device: DeviceId, key: &str) -> Result<()> {
        match (device, key) {
            (device, "model") | (device, "serial_number") if device == self.hub => {
                self.send(protocol::AptCommand::RequestHardwareInfo)?;
                self.read_expected_frame_if_available(0x0006)?;
            }
            (device, "position") if device == self.stage => {
                self.send(protocol::AptCommand::RequestPosition)?;
                self.read_expected_frame_if_available(0x0412)?;
            }
            (device, "busy")
            | (device, "homed")
            | (device, "connected")
            | (device, "position_error")
            | (device, "status_bits")
            | (device, "status_summary")
                if device == self.stage || device == self.hub =>
            {
                self.send(protocol::AptCommand::RequestStatus)?;
                self.read_expected_frame_if_available(0x0491)?;
            }
            (device, "min_velocity") | (device, "acceleration") | (device, "max_velocity")
                if device == self.stage =>
            {
                self.send(protocol::AptCommand::RequestVelocityProfile)?;
                self.read_expected_frame_if_available(0x0415)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn refresh_motion_readback(&mut self) -> Result<()> {
        self.issue_read_command(self.stage, "status_summary")?;
        self.issue_read_command(self.stage, "position")
    }

    fn refresh_keys_for(command: &str) -> Result<Vec<&'static str>> {
        match command {
            "refresh_telemetry" => Ok(vec![
                "model",
                "status_summary",
                "position",
                "max_velocity",
            ]),
            "refresh_identity" => Ok(vec!["model"]),
            "refresh_position" => Ok(vec!["position"]),
            "refresh_status" => Ok(vec!["status_summary"]),
            "refresh_velocity_profile" => Ok(vec!["max_velocity"]),
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "Thorlabs APT GenericCommand supports refresh_telemetry, refresh_identity, refresh_position, refresh_status, refresh_velocity_profile, and keep_alive; got {other}"
                ),
            )),
        }
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
                "Thorlabs APT GenericCommand does not take parameters",
            ));
        }
        if request.command == "keep_alive" {
            return Ok(());
        }
        let _ = Self::refresh_keys_for(&request.command)?;
        Ok(())
    }

    fn apply_generic_command(&mut self, request: GenericCommandRequest) -> Result<Value> {
        self.validate_generic_command(&request)?;
        if request.command == "keep_alive" {
            self.send(protocol::AptCommand::KeepAlive)?;
            return Ok(Value::Map(BTreeMap::from([
                ("command".into(), Value::String(request.command)),
                ("commands".into(), Value::I64(1)),
                (
                    "completion_basis".into(),
                    Value::String("Thorlabs APT keepalive frame sent; no reply expected".into()),
                ),
            ])));
        }
        let keys = Self::refresh_keys_for(&request.command)?;
        for key in &keys {
            let device = match *key {
                "model" | "serial_number" => self.hub,
                _ => self.stage,
            };
            self.issue_read_command(device, key)?;
        }
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("commands".into(), Value::I64(keys.len() as i64)),
            ("telemetry".into(), self.status_summary()),
            (
                "completion_basis".into(),
                Value::String("Thorlabs APT request-frame readback".into()),
            ),
        ])))
    }

    fn finish_motion(&mut self, final_position_um: f64, message: &str) -> Result<()> {
        self.busy = true;
        self.status_bits |= protocol::STATUS_IN_MOTION_CW;
        self.pending
            .push_back(DriverEvent::Event(Event::Log(LogEvent {
                driver: Some(self.id),
                message: "thorlabs apt status in motion".into(),
            })));
        self.position_um = final_position_um;
        self.status_bits &= !(protocol::STATUS_IN_MOTION_CW
            | protocol::STATUS_IN_MOTION_CCW
            | protocol::STATUS_JOGGING_CW
            | protocol::STATUS_JOGGING_CCW
            | protocol::STATUS_HOMING);
        self.busy = false;
        self.pending
            .push_back(DriverEvent::Event(Event::Log(LogEvent {
                driver: Some(self.id),
                message: message.into(),
            })));
        self.refresh_motion_readback()?;
        Ok(())
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

impl Driver for ThorlabsAptDriver {
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
            label: "thorlabs-apt-binary".into(),
            kind: "serial.binary".into(),
            metadata: BTreeMap::from([
                ("protocol".into(), Value::String("Thorlabs APT".into())),
                (
                    "completion".into(),
                    Value::String(
                        "MGMSG_MOT_MOVE_COMPLETED and status bits report motion completion".into(),
                    ),
                ),
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
                (
                    "startup_readback_supported".into(),
                    Value::List(
                        protocol::probe_script(self.probe.channel)
                            .into_iter()
                            .map(Value::String)
                            .collect(),
                    ),
                ),
            ]),
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.stage {
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
                        description: format!("thorlabs apt read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("thorlabs apt write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "thorlabs apt stage state set".into(),
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
                            Error::new(ErrorCode::Unsupported, "unknown Thorlabs APT capability")
                        })?;
                    match (&candidate.kind, request) {
                        (CapabilityKind::StageMove, CapabilityRequest::StageMove(request)) => {
                            self.validate_stage_move(*device, request)?;
                        }
                        (
                            CapabilityKind::StageHome | CapabilityKind::StageStop,
                            CapabilityRequest::None,
                        ) => {}
                        (
                            CapabilityKind::GenericCommand,
                            CapabilityRequest::GenericCommand(request),
                        ) => {
                            self.validate_generic_command(request)?;
                        }
                        (CapabilityKind::StageMove, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Thorlabs APT StageMove expects a StageMoveRequest",
                            ));
                        }
                        (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Thorlabs APT home/stop capabilities take no request",
                            ));
                        }
                        (CapabilityKind::GenericCommand, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Thorlabs APT GenericCommand expects a GenericCommandRequest",
                            ));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported Thorlabs APT capability",
                            ));
                        }
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("thorlabs apt invoke {}", capability.0),
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
                            CapabilityRequest::GenericCommand(request) => {
                                if request.command == "keep_alive" {
                                    Value::List(vec![Value::String("keep_alive".into())])
                                } else {
                                    Value::List(
                                        Self::refresh_keys_for(&request.command)?
                                            .into_iter()
                                            .map(|key| Value::String(key.into()))
                                            .collect(),
                                    )
                                }
                            }
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
                    self.issue_read_command(device, &key)?;
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
            if let Ok(frames) = protocol::parse_frames(&mut self.rx_buffer, &bytes) {
                for frame in frames {
                    let _ = self.apply_frame_readback(&frame);
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
                description: "thorlabs apt timing arm summary".into(),
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
                description: "thorlabs apt timing start sequence".into(),
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
                description: "thorlabs apt timing stop sequence".into(),
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

fn acceleration_property(
    key: &str,
    display_name: &str,
    writable: bool,
    max_um_s2: f64,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Acceleration,
        Some("um/s^2"),
        writable,
        Some(Range {
            min: acceleration(0.0),
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

fn position_config_um(device: &DeviceConfig, key: &str, legacy_key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::Position(position)) => Some(position.micrometers()),
        _ => f64_prop(device, legacy_key),
    }
}

fn f64_prop(device: &DeviceConfig, key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
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
