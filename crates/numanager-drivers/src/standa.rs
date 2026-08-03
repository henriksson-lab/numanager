use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::serial::{ScriptedSerial, SerialIo};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;
use std::time::Instant;

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const BAUD: u32 = 115_200;
    pub const DATA_BITS: u8 = 8;
    pub const STOP_BITS: u8 = 2;
    pub const PARITY: &str = "none";
    pub const FLOW_CONTROL: &str = "none";
    pub const MVCMD_RUNNING: u8 = 0x80;
    pub const STATE_IS_HOMED: u32 = 0x20;
    pub const STATE_ALARM: u32 = 0x40;
    pub const STATE_SECURITY_MASK: u32 = 0x73ffc0;
    pub const GPIO_RIGHT_EDGE: u32 = 0x1;
    pub const GPIO_LEFT_EDGE: u32 = 0x2;
    pub const MAX_SPEED_STEPS_S: f64 = 100_000.0;
    pub const MIN_ACCEL_STEPS_S2: f64 = 1.0;
    pub const MAX_ACCEL_STEPS_S2: f64 = 65_535.0;

    #[derive(Debug, Clone, PartialEq)]
    pub struct StandaProbe {
        pub controller: String,
        pub serial_number: String,
        pub axis: String,
        pub step_size_um: f64,
        pub travel_um: f64,
        pub position_um: f64,
        pub velocity_um_s: f64,
        pub acceleration_um_s2: f64,
        pub homed: bool,
        pub busy: bool,
        pub left_limit: bool,
        pub right_limit: bool,
        pub motor_enabled: bool,
        pub encoder_present: bool,
    }

    impl StandaProbe {
        pub fn simulated() -> Self {
            Self {
                controller: "8SMC4-USB configured model".into(),
                serial_number: "STANDA-SIM-0001".into(),
                axis: "x".into(),
                step_size_um: 0.15625,
                travel_um: 50_000.0,
                position_um: 0.0,
                velocity_um_s: 2_000.0,
                acceleration_um_s2: 20_000.0,
                homed: true,
                busy: false,
                left_limit: false,
                right_limit: false,
                motor_enabled: true,
                encoder_present: false,
            }
        }

        pub fn steps(&self, um: f64) -> i32 {
            (um / self.step_size_um).round() as i32
        }

        pub fn micrometers(&self, steps: i32, microsteps: i16) -> f64 {
            (steps as f64 + microsteps as f64 / 256.0) * self.step_size_um
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum StandaCommand {
        GetPosition,
        GetStatus,
        GetSerial,
        GetMoveSettings,
        GetEngineSettings,
        GetBrakeSettings,
        GetHomeSettings,
        SetMoveSettings { settings: MoveSettings },
        MoveAbsolute { position_um: f64 },
        MoveRelative { delta_um: f64 },
        Home,
        Stop,
    }

    impl StandaCommand {
        pub fn code(&self) -> &'static [u8; 4] {
            match self {
                StandaCommand::GetPosition => b"gpos",
                StandaCommand::GetStatus => b"gets",
                StandaCommand::GetSerial => b"gser",
                StandaCommand::GetMoveSettings => b"gmov",
                StandaCommand::GetEngineSettings => b"geng",
                StandaCommand::GetBrakeSettings => b"gbrk",
                StandaCommand::GetHomeSettings => b"ghom",
                StandaCommand::SetMoveSettings { .. } => b"smov",
                StandaCommand::MoveAbsolute { .. } => b"move",
                StandaCommand::MoveRelative { .. } => b"movr",
                StandaCommand::Home => b"home",
                StandaCommand::Stop => b"stop",
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct MoveSettings {
        pub speed_steps_s: u32,
        pub speed_microsteps_s: u8,
        pub acceleration_steps_s2: u16,
        pub deceleration_steps_s2: u16,
        pub antiplay_speed_steps_s: u32,
        pub antiplay_speed_microsteps_s: u8,
    }

    impl MoveSettings {
        pub fn from_probe(probe: &StandaProbe) -> Self {
            let speed_steps_s = speed_from_um_s(probe.velocity_um_s, probe);
            let acceleration_steps_s2 = acceleration_from_um_s2(probe.acceleration_um_s2, probe);
            Self {
                speed_steps_s: speed_steps_s.0,
                speed_microsteps_s: speed_steps_s.1,
                acceleration_steps_s2,
                deceleration_steps_s2: acceleration_steps_s2,
                antiplay_speed_steps_s: 0,
                antiplay_speed_microsteps_s: 0,
            }
        }

        pub fn velocity_um_s(&self, probe: &StandaProbe) -> f64 {
            (self.speed_steps_s as f64 + self.speed_microsteps_s as f64 / 256.0)
                * probe.step_size_um
        }

        pub fn acceleration_um_s2(&self, probe: &StandaProbe) -> f64 {
            self.acceleration_steps_s2 as f64 * probe.step_size_um
        }

        pub fn deceleration_um_s2(&self, probe: &StandaProbe) -> f64 {
            self.deceleration_steps_s2 as f64 * probe.step_size_um
        }

        pub fn antiplay_velocity_um_s(&self, probe: &StandaProbe) -> f64 {
            (self.antiplay_speed_steps_s as f64 + self.antiplay_speed_microsteps_s as f64 / 256.0)
                * probe.step_size_um
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct StandaStatus {
        pub moving: bool,
        pub current_position_um: Option<f64>,
        pub current_speed_steps_s: Option<i32>,
        pub homed: bool,
        pub security_flags: u32,
        pub power_state: u8,
        pub encoder_state: u8,
        pub move_state: u8,
        pub move_command_state: u8,
        pub gpio_flags: u32,
        pub raw_flags: u32,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct EngineSettings {
        pub nominal_voltage: u16,
        pub nominal_current: u16,
        pub nominal_speed_steps_s: u32,
        pub nominal_speed_microsteps_s: u8,
        pub engine_flags: u16,
        pub antiplay_steps: i16,
        pub microstep_mode: u8,
        pub steps_per_revolution: u16,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct BrakeSettings {
        pub t1_ms: u16,
        pub t2_ms: u16,
        pub t3_ms: u16,
        pub t4_ms: u16,
        pub brake_flags: u8,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct HomeSettings {
        pub fast_velocity_steps_s: u32,
        pub fast_velocity_microsteps_s: u8,
        pub slow_velocity_steps_s: u32,
        pub slow_velocity_microsteps_s: u8,
        pub delta_position_steps: i32,
        pub delta_position_microsteps: i16,
        pub home_flags: u16,
    }

    pub fn encode(command: &StandaCommand, probe: &StandaProbe) -> Vec<u8> {
        let mut bytes = command.code().to_vec();
        let data = match command {
            StandaCommand::MoveAbsolute { position_um } => {
                move_payload(probe.steps(*position_um), 0)
            }
            StandaCommand::MoveRelative { delta_um } => move_payload(probe.steps(*delta_um), 0),
            StandaCommand::SetMoveSettings { settings } => move_settings_payload(settings),
            _ => Vec::new(),
        };
        bytes.extend_from_slice(&data);
        if !data.is_empty() {
            bytes.extend_from_slice(&crc16_modbus(&data).to_le_bytes());
        }
        bytes
    }

    pub fn encode_position_steps(code: &'static [u8; 4], steps: i32, microsteps: i16) -> Vec<u8> {
        let mut bytes = code.to_vec();
        let data = move_payload(steps, microsteps);
        bytes.extend_from_slice(&data);
        bytes.extend_from_slice(&crc16_modbus(&data).to_le_bytes());
        bytes
    }

    fn move_payload(steps: i32, microsteps: i16) -> Vec<u8> {
        let mut data = Vec::with_capacity(12);
        data.extend_from_slice(&steps.to_le_bytes());
        data.extend_from_slice(&microsteps.to_le_bytes());
        data.extend_from_slice(&[0; 6]);
        data
    }

    fn move_settings_payload(settings: &MoveSettings) -> Vec<u8> {
        let mut data = Vec::with_capacity(24);
        data.extend_from_slice(&settings.speed_steps_s.to_le_bytes());
        data.push(settings.speed_microsteps_s);
        data.extend_from_slice(&settings.acceleration_steps_s2.to_le_bytes());
        data.extend_from_slice(&settings.deceleration_steps_s2.to_le_bytes());
        data.extend_from_slice(&settings.antiplay_speed_steps_s.to_le_bytes());
        data.push(settings.antiplay_speed_microsteps_s);
        data.extend_from_slice(&[0; 10]);
        data
    }

    pub fn speed_from_um_s(value_um_s: f64, probe: &StandaProbe) -> (u32, u8) {
        let steps = (value_um_s / probe.step_size_um).clamp(0.0, MAX_SPEED_STEPS_S);
        let whole = steps.floor() as u32;
        let fraction = ((steps - whole as f64) * 256.0).round().clamp(0.0, 255.0) as u8;
        (whole, fraction)
    }

    pub fn acceleration_from_um_s2(value_um_s2: f64, probe: &StandaProbe) -> u16 {
        (value_um_s2 / probe.step_size_um)
            .round()
            .clamp(MIN_ACCEL_STEPS_S2, MAX_ACCEL_STEPS_S2) as u16
    }

    pub fn crc16_modbus(data: &[u8]) -> u16 {
        let mut crc = 0xffffu16;
        for byte in data {
            crc ^= *byte as u16;
            for _ in 0..8 {
                if crc & 1 == 1 {
                    crc = (crc >> 1) ^ 0xa001;
                } else {
                    crc >>= 1;
                }
            }
        }
        crc
    }

    pub fn parse_ack(reply: &[u8], command: &StandaCommand) -> Result<()> {
        if reply.len() < 4 {
            return Ok(());
        }
        parse_error_reply(reply)?;
        if &reply[..4] != command.code() {
            return Err(Error::new(
                ErrorCode::Transport,
                "Standa reply command did not match request",
            ));
        }
        Ok(())
    }

    pub fn parse_position(reply: &[u8], probe: &StandaProbe) -> Result<Option<f64>> {
        if reply.is_empty() {
            return Ok(None);
        }
        parse_error_reply(reply)?;
        if reply.len() < 26 || &reply[..4] != b"gpos" {
            return Err(Error::new(
                ErrorCode::Transport,
                "Standa gpos reply must contain command, position fields, encoder, reserved bytes, and CRC",
            ));
        }
        verify_crc(&reply[4..24], &reply[24..26])?;
        let steps = i32::from_le_bytes(reply[4..8].try_into().expect("checked byte range"));
        let microsteps = i16::from_le_bytes(reply[8..10].try_into().expect("checked byte range"));
        Ok(Some(probe.micrometers(steps, microsteps)))
    }

    pub fn parse_serial(reply: &[u8]) -> Result<Option<String>> {
        if reply.is_empty() {
            return Ok(None);
        }
        parse_error_reply(reply)?;
        if reply.len() < 10 || &reply[..4] != b"gser" {
            return Err(Error::new(
                ErrorCode::Transport,
                "Standa gser reply must contain command, serial number, and CRC",
            ));
        }
        verify_crc(&reply[4..8], &reply[8..10])?;
        let serial = u32::from_le_bytes(reply[4..8].try_into().expect("checked byte range"));
        Ok(Some(serial.to_string()))
    }

    pub fn parse_move_settings(reply: &[u8], probe: &StandaProbe) -> Result<Option<MoveSettings>> {
        if reply.is_empty() {
            return Ok(None);
        }
        parse_error_reply(reply)?;
        if reply.len() < 30 || &reply[..4] != b"gmov" {
            return Err(Error::new(
                ErrorCode::Transport,
                "Standa gmov reply must contain command, movement settings, and CRC",
            ));
        }
        verify_crc(&reply[4..28], &reply[28..30])?;
        let settings = MoveSettings {
            speed_steps_s: u32::from_le_bytes(reply[4..8].try_into().expect("checked byte range")),
            speed_microsteps_s: reply[8],
            acceleration_steps_s2: u16::from_le_bytes(
                reply[9..11].try_into().expect("checked byte range"),
            ),
            deceleration_steps_s2: u16::from_le_bytes(
                reply[11..13].try_into().expect("checked byte range"),
            ),
            antiplay_speed_steps_s: u32::from_le_bytes(
                reply[13..17].try_into().expect("checked byte range"),
            ),
            antiplay_speed_microsteps_s: reply[17],
        };
        let _ = settings.velocity_um_s(probe);
        Ok(Some(settings))
    }

    pub fn parse_status(reply: &[u8], probe: &StandaProbe) -> Result<Option<StandaStatus>> {
        if reply.is_empty() {
            return Ok(None);
        }
        parse_error_reply(reply)?;
        if reply.len() < 54 || &reply[..4] != b"gets" {
            return Err(Error::new(
                ErrorCode::Transport,
                "Standa gets reply must contain command, state fields, telemetry, and CRC",
            ));
        }
        verify_crc(&reply[4..52], &reply[52..54])?;
        let move_state = reply[4];
        let move_command_state = reply[5];
        let power_state = reply[6];
        let encoder_state = reply[7];
        let steps = i32::from_le_bytes(reply[9..13].try_into().expect("checked byte range"));
        let microsteps = i16::from_le_bytes(reply[13..15].try_into().expect("checked byte range"));
        let speed = i32::from_le_bytes(reply[23..27].try_into().expect("checked byte range"));
        let raw_flags = u32::from_le_bytes(reply[39..43].try_into().expect("checked byte range"));
        let gpio_flags = u32::from_le_bytes(reply[43..47].try_into().expect("checked byte range"));
        Ok(Some(StandaStatus {
            moving: move_command_state & MVCMD_RUNNING != 0,
            current_position_um: Some(probe.micrometers(steps, microsteps)),
            current_speed_steps_s: Some(speed),
            homed: raw_flags & STATE_IS_HOMED != 0,
            security_flags: raw_flags & STATE_SECURITY_MASK,
            power_state,
            encoder_state,
            move_state,
            move_command_state,
            gpio_flags,
            raw_flags,
        }))
    }

    pub fn parse_engine_settings(reply: &[u8]) -> Result<Option<EngineSettings>> {
        if reply.is_empty() {
            return Ok(None);
        }
        parse_error_reply(reply)?;
        if reply.len() < 34 || &reply[..4] != b"geng" {
            return Err(Error::new(
                ErrorCode::Transport,
                "Standa geng reply must contain command, engine settings, and CRC",
            ));
        }
        verify_crc(&reply[4..32], &reply[32..34])?;
        Ok(Some(EngineSettings {
            nominal_voltage: u16::from_le_bytes(reply[4..6].try_into().expect("checked range")),
            nominal_current: u16::from_le_bytes(reply[6..8].try_into().expect("checked range")),
            nominal_speed_steps_s: u32::from_le_bytes(
                reply[8..12].try_into().expect("checked range"),
            ),
            nominal_speed_microsteps_s: reply[12],
            engine_flags: u16::from_le_bytes(reply[13..15].try_into().expect("checked range")),
            antiplay_steps: i16::from_le_bytes(reply[15..17].try_into().expect("checked range")),
            microstep_mode: reply[17],
            steps_per_revolution: u16::from_le_bytes(
                reply[18..20].try_into().expect("checked range"),
            ),
        }))
    }

    pub fn parse_brake_settings(reply: &[u8]) -> Result<Option<BrakeSettings>> {
        if reply.is_empty() {
            return Ok(None);
        }
        parse_error_reply(reply)?;
        if reply.len() < 25 || &reply[..4] != b"gbrk" {
            return Err(Error::new(
                ErrorCode::Transport,
                "Standa gbrk reply must contain command, brake settings, and CRC",
            ));
        }
        verify_crc(&reply[4..23], &reply[23..25])?;
        Ok(Some(BrakeSettings {
            t1_ms: u16::from_le_bytes(reply[4..6].try_into().expect("checked range")),
            t2_ms: u16::from_le_bytes(reply[6..8].try_into().expect("checked range")),
            t3_ms: u16::from_le_bytes(reply[8..10].try_into().expect("checked range")),
            t4_ms: u16::from_le_bytes(reply[10..12].try_into().expect("checked range")),
            brake_flags: reply[12],
        }))
    }

    pub fn parse_home_settings(reply: &[u8]) -> Result<Option<HomeSettings>> {
        if reply.is_empty() {
            return Ok(None);
        }
        parse_error_reply(reply)?;
        if reply.len() < 33 || &reply[..4] != b"ghom" {
            return Err(Error::new(
                ErrorCode::Transport,
                "Standa ghom reply must contain command, home settings, and CRC",
            ));
        }
        verify_crc(&reply[4..31], &reply[31..33])?;
        Ok(Some(HomeSettings {
            fast_velocity_steps_s: u32::from_le_bytes(
                reply[4..8].try_into().expect("checked range"),
            ),
            fast_velocity_microsteps_s: reply[8],
            slow_velocity_steps_s: u32::from_le_bytes(
                reply[9..13].try_into().expect("checked range"),
            ),
            slow_velocity_microsteps_s: reply[13],
            delta_position_steps: i32::from_le_bytes(
                reply[14..18].try_into().expect("checked range"),
            ),
            delta_position_microsteps: i16::from_le_bytes(
                reply[18..20].try_into().expect("checked range"),
            ),
            home_flags: u16::from_le_bytes(reply[20..22].try_into().expect("checked range")),
        }))
    }

    fn verify_crc(data: &[u8], crc_bytes: &[u8]) -> Result<()> {
        let expected = u16::from_le_bytes(crc_bytes.try_into().map_err(|_| {
            Error::new(
                ErrorCode::Transport,
                "Standa CRC field must contain exactly two bytes",
            )
        })?);
        let actual = crc16_modbus(data);
        if expected == actual {
            Ok(())
        } else {
            Err(Error::new(
                ErrorCode::Transport,
                "Standa reply CRC check failed",
            ))
        }
    }

    fn parse_error_reply(reply: &[u8]) -> Result<()> {
        let code = reply.get(..4).unwrap_or(reply);
        match code {
            b"errc" => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Standa controller reported command error",
            )),
            b"errd" => Err(Error::new(
                ErrorCode::Transport,
                "Standa controller reported data CRC error",
            )),
            b"errv" => Err(Error::new(
                ErrorCode::InvalidProperty,
                "Standa controller reported value error",
            )),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StandaConfiguredProbe {
    probe: protocol::StandaProbe,
    endpoint: Option<StandaSerialEndpoint>,
    #[allow(dead_code)]
    startup_readback: bool,
}

impl StandaConfiguredProbe {
    pub fn simulated() -> Self {
        Self {
            probe: protocol::StandaProbe::simulated(),
            endpoint: None,
            startup_readback: false,
        }
    }
}

#[derive(Debug, Clone)]
struct StandaSerialEndpoint {
    port_name: String,
    baud_rate: u32,
    timeout_ms: u64,
    connect: bool,
}

pub struct StandaDiscovery {
    id: DriverId,
    configured: Vec<StandaConfiguredProbe>,
}

impl StandaDiscovery {
    pub fn simulated(id: DriverId) -> Self {
        Self {
            id,
            configured: vec![StandaConfiguredProbe::simulated()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let configured = config
            .devices
            .iter()
            .filter(|device| device.driver == "standa" || device.driver == "standa-8smc")
            .map(StandaConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            id: next_id,
            configured,
        })
    }
}

impl DriverDiscovery for StandaDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.configured
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.id.0 + index as u64);
                let driver: Box<dyn Driver> = if configured
                    .endpoint
                    .as_ref()
                    .map(|endpoint| endpoint.connect)
                    .unwrap_or(false)
                {
                    Box::new(StandaDriver::serial(id, configured.clone())?)
                } else {
                    Box::new(StandaDriver::configured(id, configured.clone()))
                };
                Ok(DriverCandidate::from_driver(
                    format!(
                        "Standa 8SMC4 {} {}",
                        configured.probe.controller, configured.probe.serial_number
                    ),
                    driver,
                ))
            })
            .collect()
    }
}

impl StandaConfiguredProbe {
    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut probe = protocol::StandaProbe::simulated();
        probe.controller =
            string_prop(device, "controller").unwrap_or_else(|| device.label.clone());
        probe.serial_number =
            string_prop(device, "serial_number").unwrap_or_else(|| "configured".into());
        probe.axis = string_prop(device, "axis").unwrap_or_else(|| "x".into());
        probe.travel_um =
            position_config_um(device, "travel", "travel_um").unwrap_or(probe.travel_um);
        probe.step_size_um = position_config_um(device, "step_size", "step_size_um")
            .unwrap_or(probe.step_size_um)
            .max(f64::EPSILON);
        probe.position_um =
            position_config_um(device, "position", "position_um").unwrap_or(probe.position_um);
        probe.velocity_um_s = velocity_config_um_s(device, "velocity", "velocity_um_s")
            .unwrap_or(probe.velocity_um_s);
        probe.acceleration_um_s2 =
            acceleration_config_um_s2(device, "acceleration", "acceleration_um_s2")
                .unwrap_or(probe.acceleration_um_s2);
        probe.homed = bool_prop(device, "homed").unwrap_or(probe.homed);
        probe.left_limit = bool_prop(device, "left_limit").unwrap_or(probe.left_limit);
        probe.right_limit = bool_prop(device, "right_limit").unwrap_or(probe.right_limit);
        probe.motor_enabled = bool_prop(device, "motor_enabled").unwrap_or(probe.motor_enabled);
        probe.encoder_present =
            bool_prop(device, "encoder_present").unwrap_or(probe.encoder_present);
        Ok(Self {
            probe,
            endpoint: standa_endpoint_from_config(device),
            startup_readback: bool_prop(device, "startup_readback")
                .or_else(|| bool_prop(device, "active_probe"))
                .unwrap_or(false),
        })
    }
}

pub struct StandaDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    axis: DeviceId,
    probe: protocol::StandaProbe,
    serial_port: Option<String>,
    baud_rate: u32,
    serial_timeout_ms: u64,
    connected: bool,
    position_um: f64,
    target_um: f64,
    velocity_um_s: f64,
    acceleration_um_s2: f64,
    move_settings: protocol::MoveSettings,
    busy: bool,
    homed: bool,
    left_limit: bool,
    right_limit: bool,
    motor_enabled: bool,
    encoder_present: bool,
    alarm: bool,
    security_flags: u32,
    power_state: u8,
    encoder_state: u8,
    move_state: u8,
    move_command_state: u8,
    gpio_flags: u32,
    raw_flags: u32,
    engine_settings: Option<protocol::EngineSettings>,
    brake_settings: Option<protocol::BrakeSettings>,
    home_settings: Option<protocol::HomeSettings>,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
    serial: Box<dyn SerialIo>,
    fixture_mode: bool,
}

impl StandaDriver {
    pub fn configured(id: DriverId, configured: StandaConfiguredProbe) -> Self {
        Self::new_with_transport_metadata(
            id,
            configured.probe,
            configured.endpoint,
            false,
            Box::new(ScriptedSerial::new()),
            true,
        )
    }

    #[cfg(feature = "os-serial")]
    pub fn serial(id: DriverId, configured: StandaConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Standa serial probe is missing serial_port metadata",
            )
        })?;
        let mut serial = numanager_core::serial::OsSerialPort::open_config(
            numanager_core::serial::OsSerialConfig::new(
                endpoint.port_name.clone(),
                endpoint.baud_rate,
            )
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::None)
            .stop_bits(serialport::StopBits::Two)
            .flow_control(serialport::FlowControl::None)
            .timeout(Duration::from_millis(endpoint.timeout_ms)),
        )?;
        let mut probe = configured.probe;
        query_serial_number(&mut serial, &mut probe)?;
        query_position(&mut serial, &mut probe)?;
        query_status(&mut serial, &mut probe)?;
        query_move_settings(&mut serial, &mut probe)?;
        let mut driver = Self::new_with_transport_metadata(
            id,
            probe,
            Some(endpoint),
            true,
            Box::new(serial),
            false,
        );
        driver.refresh_engine_settings_once()?;
        driver.refresh_brake_settings_once()?;
        driver.refresh_home_settings_once()?;
        Ok(driver)
    }

    #[cfg(not(feature = "os-serial"))]
    pub fn serial(_id: DriverId, configured: StandaConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.map(|endpoint| {
            format!(
                "{} at {} baud timeout={}ms connect={}",
                endpoint.port_name, endpoint.baud_rate, endpoint.timeout_ms, endpoint.connect
            )
        });
        Err(Error::new(
            ErrorCode::Unsupported,
            format!(
                "Standa real serial transport requires the numanager-drivers os-serial feature{}",
                endpoint
                    .map(|endpoint| format!(" for {endpoint}"))
                    .unwrap_or_default()
            ),
        ))
    }

    pub fn new(
        id: DriverId,
        probe: protocol::StandaProbe,
        serial: Box<dyn SerialIo>,
        fixture_mode: bool,
    ) -> Self {
        Self::new_with_transport_metadata(id, probe, None, false, serial, fixture_mode)
    }

    fn new_with_transport_metadata(
        id: DriverId,
        probe: protocol::StandaProbe,
        endpoint: Option<StandaSerialEndpoint>,
        connected: bool,
        serial: Box<dyn SerialIo>,
        fixture_mode: bool,
    ) -> Self {
        let serial_port = endpoint.as_ref().map(|endpoint| endpoint.port_name.clone());
        let baud_rate = endpoint
            .as_ref()
            .map(|endpoint| endpoint.baud_rate)
            .unwrap_or(protocol::BAUD);
        let serial_timeout_ms = endpoint
            .as_ref()
            .map(|endpoint| endpoint.timeout_ms)
            .unwrap_or(200);
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 901)),
            hub: DeviceId(NodeId(id.0 * 1000 + 910)),
            axis: DeviceId(NodeId(id.0 * 1000 + 911)),
            serial_port,
            baud_rate,
            serial_timeout_ms,
            connected,
            position_um: probe.position_um,
            target_um: probe.position_um,
            velocity_um_s: probe.velocity_um_s,
            acceleration_um_s2: probe.acceleration_um_s2,
            move_settings: protocol::MoveSettings::from_probe(&probe),
            busy: probe.busy,
            homed: probe.homed,
            left_limit: probe.left_limit,
            right_limit: probe.right_limit,
            motor_enabled: probe.motor_enabled,
            encoder_present: probe.encoder_present,
            alarm: false,
            security_flags: 0,
            power_state: 0,
            encoder_state: 0,
            move_state: 0,
            move_command_state: 0,
            gpio_flags: 0,
            raw_flags: 0,
            engine_settings: None,
            brake_settings: None,
            home_settings: None,
            probe,
            next_token: 1,
            pending: VecDeque::new(),
            serial,
            fixture_mode,
        }
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn send(&mut self, command: &protocol::StandaCommand) -> Result<()> {
        self.serial.write(&protocol::encode(command, &self.probe))
    }

    fn read_ack(&mut self, command: &protocol::StandaCommand) -> Result<()> {
        let reply = self.serial.read_available()?;
        if reply.is_empty() && !self.fixture_mode {
            return Err(Error::new(
                ErrorCode::Transport,
                "Standa command echo was not received",
            ));
        }
        protocol::parse_ack(&reply, command)
    }

    fn read_position_reply(&mut self) -> Result<()> {
        let reply = self.serial.read_available()?;
        if reply.is_empty() && !self.fixture_mode {
            return Err(Error::new(
                ErrorCode::Transport,
                "Standa gpos reply was not received",
            ));
        }
        if let Some(position) = protocol::parse_position(&reply, &self.probe)? {
            self.position_um = position.clamp(0.0, self.probe.travel_um);
        }
        Ok(())
    }

    fn read_status_reply(&mut self) -> Result<()> {
        let reply = self.serial.read_available()?;
        if reply.is_empty() && !self.fixture_mode {
            return Err(Error::new(
                ErrorCode::Transport,
                "Standa gets reply was not received",
            ));
        }
        if let Some(status) = protocol::parse_status(&reply, &self.probe)? {
            self.busy = status.moving;
            self.homed = status.homed;
            self.left_limit = status.gpio_flags & protocol::GPIO_LEFT_EDGE != 0;
            self.right_limit = status.gpio_flags & protocol::GPIO_RIGHT_EDGE != 0;
            self.motor_enabled = matches!(status.power_state, 0x03 | 0x04 | 0x05);
            self.encoder_present = status.encoder_state != 0;
            self.alarm = status.raw_flags & protocol::STATE_ALARM != 0;
            self.security_flags = status.security_flags;
            self.power_state = status.power_state;
            self.encoder_state = status.encoder_state;
            self.move_state = status.move_state;
            self.move_command_state = status.move_command_state;
            self.gpio_flags = status.gpio_flags;
            self.raw_flags = status.raw_flags;
            if let Some(position) = status.current_position_um {
                self.position_um = position.clamp(0.0, self.probe.travel_um);
            }
        }
        Ok(())
    }

    fn read_move_settings_reply(&mut self) -> Result<()> {
        let reply = self.serial.read_available()?;
        if reply.is_empty() && !self.fixture_mode {
            return Err(Error::new(
                ErrorCode::Transport,
                "Standa gmov reply was not received",
            ));
        }
        if let Some(settings) = protocol::parse_move_settings(&reply, &self.probe)? {
            self.move_settings = settings.clone();
            self.velocity_um_s = settings.velocity_um_s(&self.probe);
            self.acceleration_um_s2 = settings.acceleration_um_s2(&self.probe);
            self.probe.velocity_um_s = self.velocity_um_s;
            self.probe.acceleration_um_s2 = self.acceleration_um_s2;
        }
        Ok(())
    }

    fn refresh_status_once(&mut self) -> Result<()> {
        self.send(&protocol::StandaCommand::GetStatus)?;
        self.read_status_reply()
    }

    fn refresh_position_once(&mut self) -> Result<()> {
        self.send(&protocol::StandaCommand::GetPosition)?;
        self.read_position_reply()
    }

    fn wait_until_idle(&mut self, target_um: Option<f64>, timeout: Duration) -> Result<()> {
        if self.fixture_mode {
            self.busy = false;
            if let Some(target_um) = target_um {
                self.position_um = target_um.clamp(0.0, self.probe.travel_um);
            }
            return Ok(());
        }

        let deadline = Instant::now() + timeout;
        loop {
            self.refresh_status_once()?;
            if !self.busy {
                if let Some(target_um) = target_um {
                    self.position_um = target_um.clamp(0.0, self.probe.travel_um);
                }
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(Error::new(
                    ErrorCode::Timeout,
                    "Standa motion did not report idle before timeout",
                ));
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn move_timeout(&self, distance_um: f64) -> Duration {
        let velocity_um_s = self.velocity_um_s.max(1.0);
        let estimated_ms = (distance_um.abs() / velocity_um_s * 1_000.0) as u64;
        Duration::from_millis(estimated_ms + self.serial_timeout_ms.max(1_000) + 2_000)
    }

    fn home_timeout(&self) -> Duration {
        self.move_timeout(self.probe.travel_um)
            .max(Duration::from_secs(10))
    }

    fn refresh_move_settings_once(&mut self) -> Result<()> {
        self.send(&protocol::StandaCommand::GetMoveSettings)?;
        self.read_move_settings_reply()
    }

    fn read_engine_settings_reply(&mut self) -> Result<()> {
        let reply = self.serial.read_available()?;
        if reply.is_empty() && !self.fixture_mode {
            return Err(Error::new(
                ErrorCode::Transport,
                "Standa geng reply was not received",
            ));
        }
        if let Some(settings) = protocol::parse_engine_settings(&reply)? {
            self.engine_settings = Some(settings);
        }
        Ok(())
    }

    fn read_brake_settings_reply(&mut self) -> Result<()> {
        let reply = self.serial.read_available()?;
        if reply.is_empty() && !self.fixture_mode {
            return Err(Error::new(
                ErrorCode::Transport,
                "Standa gbrk reply was not received",
            ));
        }
        if let Some(settings) = protocol::parse_brake_settings(&reply)? {
            self.brake_settings = Some(settings);
        }
        Ok(())
    }

    fn read_home_settings_reply(&mut self) -> Result<()> {
        let reply = self.serial.read_available()?;
        if reply.is_empty() && !self.fixture_mode {
            return Err(Error::new(
                ErrorCode::Transport,
                "Standa ghom reply was not received",
            ));
        }
        if let Some(settings) = protocol::parse_home_settings(&reply)? {
            self.home_settings = Some(settings);
        }
        Ok(())
    }

    fn refresh_engine_settings_once(&mut self) -> Result<()> {
        self.send(&protocol::StandaCommand::GetEngineSettings)?;
        self.read_engine_settings_reply()
    }

    fn refresh_brake_settings_once(&mut self) -> Result<()> {
        self.send(&protocol::StandaCommand::GetBrakeSettings)?;
        self.read_brake_settings_reply()
    }

    fn refresh_home_settings_once(&mut self) -> Result<()> {
        self.send(&protocol::StandaCommand::GetHomeSettings)?;
        self.read_home_settings_reply()
    }

    fn emit_property(&mut self, key: &str, value: Value) {
        self.pending
            .push_back(DriverEvent::Event(Event::PropertyChanged(
                PropertyChanged {
                    device: self.axis,
                    key: key.into(),
                    value,
                },
            )));
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        match (device, key) {
            (device, "controller") if device == self.hub => {
                Ok(Value::String(self.probe.controller.clone()))
            }
            (device, "serial_number") if device == self.hub => {
                Ok(Value::String(self.probe.serial_number.clone()))
            }
            (device, "protocol") if device == self.hub => Ok(Value::String("8SMC4 v18.3".into())),
            (device, "position") if device == self.axis => Ok(position(self.position_um)),
            (device, "target") if device == self.axis => Ok(position(self.target_um)),
            (device, "velocity") if device == self.axis => Ok(velocity(self.velocity_um_s)),
            (device, "acceleration") if device == self.axis => {
                Ok(acceleration(self.acceleration_um_s2))
            }
            (device, "deceleration") if device == self.axis => Ok(acceleration(
                self.move_settings.deceleration_um_s2(&self.probe),
            )),
            (device, "antiplay_velocity") if device == self.axis => Ok(velocity(
                self.move_settings.antiplay_velocity_um_s(&self.probe),
            )),
            (device, "busy") if device == self.axis => Ok(Value::Bool(self.busy)),
            (device, "homed") if device == self.axis => Ok(Value::Bool(self.homed)),
            (device, "left_limit") if device == self.axis => Ok(Value::Bool(self.left_limit)),
            (device, "right_limit") if device == self.axis => Ok(Value::Bool(self.right_limit)),
            (device, "motor_enabled") if device == self.axis => Ok(Value::Bool(self.motor_enabled)),
            (device, "encoder_present") if device == self.axis => {
                Ok(Value::Bool(self.encoder_present))
            }
            (device, "alarm") if device == self.axis => Ok(Value::Bool(self.alarm)),
            (device, "security_flags") if device == self.axis => {
                Ok(Value::I64(self.security_flags as i64))
            }
            (device, "power_state") if device == self.axis => {
                Ok(Value::I64(self.power_state as i64))
            }
            (device, "encoder_state") if device == self.axis => {
                Ok(Value::I64(self.encoder_state as i64))
            }
            (device, "move_state") if device == self.axis => Ok(Value::I64(self.move_state as i64)),
            (device, "move_command_state") if device == self.axis => {
                Ok(Value::I64(self.move_command_state as i64))
            }
            (device, "gpio_flags") if device == self.axis => Ok(Value::I64(self.gpio_flags as i64)),
            (device, "raw_flags") if device == self.axis => Ok(Value::I64(self.raw_flags as i64)),
            (device, "status_summary") if device == self.axis => Ok(self.status_summary()),
            (device, "engine_settings") if device == self.axis => Ok(self.engine_settings_map()),
            (device, "brake_settings") if device == self.axis => Ok(self.brake_settings_map()),
            (device, "home_settings") if device == self.axis => Ok(self.home_settings_map()),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Standa property {key}"),
            )),
        }
    }

    fn status_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("busy".into(), Value::Bool(self.busy)),
            ("homed".into(), Value::Bool(self.homed)),
            ("left_limit".into(), Value::Bool(self.left_limit)),
            ("right_limit".into(), Value::Bool(self.right_limit)),
            ("motor_enabled".into(), Value::Bool(self.motor_enabled)),
            ("encoder_present".into(), Value::Bool(self.encoder_present)),
            ("alarm".into(), Value::Bool(self.alarm)),
            (
                "security_flags".into(),
                Value::I64(self.security_flags as i64),
            ),
            ("power_state".into(), Value::I64(self.power_state as i64)),
            (
                "encoder_state".into(),
                Value::I64(self.encoder_state as i64),
            ),
            ("move_state".into(), Value::I64(self.move_state as i64)),
            (
                "move_command_state".into(),
                Value::I64(self.move_command_state as i64),
            ),
            ("gpio_flags".into(), Value::I64(self.gpio_flags as i64)),
            ("raw_flags".into(), Value::I64(self.raw_flags as i64)),
            ("position".into(), position(self.position_um)),
            ("target".into(), position(self.target_um)),
        ]))
    }

    fn engine_settings_map(&self) -> Value {
        let Some(settings) = &self.engine_settings else {
            return Value::Map(BTreeMap::from([("known".into(), Value::Bool(false))]));
        };
        Value::Map(BTreeMap::from([
            ("known".into(), Value::Bool(true)),
            (
                "nominal_voltage".into(),
                Value::I64(settings.nominal_voltage as i64),
            ),
            (
                "nominal_current".into(),
                Value::I64(settings.nominal_current as i64),
            ),
            (
                "nominal_speed_steps_s".into(),
                Value::I64(settings.nominal_speed_steps_s as i64),
            ),
            (
                "nominal_speed_microsteps_s".into(),
                Value::I64(settings.nominal_speed_microsteps_s as i64),
            ),
            (
                "engine_flags".into(),
                Value::I64(settings.engine_flags as i64),
            ),
            (
                "antiplay_steps".into(),
                Value::I64(settings.antiplay_steps as i64),
            ),
            (
                "microstep_mode".into(),
                Value::I64(settings.microstep_mode as i64),
            ),
            (
                "steps_per_revolution".into(),
                Value::I64(settings.steps_per_revolution as i64),
            ),
        ]))
    }

    fn brake_settings_map(&self) -> Value {
        let Some(settings) = &self.brake_settings else {
            return Value::Map(BTreeMap::from([("known".into(), Value::Bool(false))]));
        };
        Value::Map(BTreeMap::from([
            ("known".into(), Value::Bool(true)),
            (
                "t1".into(),
                Value::TimeInterval(TimeInterval::from_milliseconds(settings.t1_ms as f64)),
            ),
            (
                "t2".into(),
                Value::TimeInterval(TimeInterval::from_milliseconds(settings.t2_ms as f64)),
            ),
            (
                "t3".into(),
                Value::TimeInterval(TimeInterval::from_milliseconds(settings.t3_ms as f64)),
            ),
            (
                "t4".into(),
                Value::TimeInterval(TimeInterval::from_milliseconds(settings.t4_ms as f64)),
            ),
            (
                "brake_flags".into(),
                Value::I64(settings.brake_flags as i64),
            ),
            (
                "enabled".into(),
                Value::Bool(settings.brake_flags & 0x01 != 0),
            ),
            (
                "turns_off_motor_power".into(),
                Value::Bool(settings.brake_flags & 0x02 != 0),
            ),
        ]))
    }

    fn home_settings_map(&self) -> Value {
        let Some(settings) = &self.home_settings else {
            return Value::Map(BTreeMap::from([("known".into(), Value::Bool(false))]));
        };
        Value::Map(BTreeMap::from([
            ("known".into(), Value::Bool(true)),
            (
                "fast_velocity_steps_s".into(),
                Value::I64(settings.fast_velocity_steps_s as i64),
            ),
            (
                "fast_velocity_microsteps_s".into(),
                Value::I64(settings.fast_velocity_microsteps_s as i64),
            ),
            (
                "slow_velocity_steps_s".into(),
                Value::I64(settings.slow_velocity_steps_s as i64),
            ),
            (
                "slow_velocity_microsteps_s".into(),
                Value::I64(settings.slow_velocity_microsteps_s as i64),
            ),
            (
                "delta_position_steps".into(),
                Value::I64(settings.delta_position_steps as i64),
            ),
            (
                "delta_position_microsteps".into(),
                Value::I64(settings.delta_position_microsteps as i64),
            ),
            ("home_flags".into(), Value::I64(settings.home_flags as i64)),
        ]))
    }

    fn refresh_commands_for(command: &str) -> Result<Vec<&'static str>> {
        match command {
            "refresh_readbacks" => Ok(vec![
                "position",
                "status",
                "move_settings",
                "engine_settings",
                "brake_settings",
                "home_settings",
            ]),
            "refresh_position" => Ok(vec!["position"]),
            "refresh_status" => Ok(vec!["status"]),
            "refresh_move_settings" => Ok(vec!["move_settings"]),
            "refresh_engine_settings" => Ok(vec!["engine_settings"]),
            "refresh_brake_settings" => Ok(vec!["brake_settings"]),
            "refresh_home_settings" => Ok(vec!["home_settings"]),
            "refresh_static_settings" => {
                Ok(vec!["engine_settings", "brake_settings", "home_settings"])
            }
            other => Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "Standa GenericCommand supports refresh_readbacks, refresh_position, refresh_status, refresh_move_settings, refresh_engine_settings, refresh_brake_settings, refresh_home_settings, and refresh_static_settings; got {other}"
                ),
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
                ErrorCode::Unsupported,
                "Standa GenericCommand is available on the axis device",
            ));
        }
        if !request.params.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Standa GenericCommand does not take parameters",
            ));
        }
        let _ = Self::refresh_commands_for(&request.command)?;
        Ok(())
    }

    fn apply_generic_command(
        &mut self,
        device: DeviceId,
        request: GenericCommandRequest,
    ) -> Result<Value> {
        self.validate_generic_command(device, &request)?;
        let commands = Self::refresh_commands_for(&request.command)?;
        for command in &commands {
            match *command {
                "position" => self.refresh_position_once()?,
                "status" => self.refresh_status_once()?,
                "move_settings" => self.refresh_move_settings_once()?,
                "engine_settings" => self.refresh_engine_settings_once()?,
                "brake_settings" => self.refresh_brake_settings_once()?,
                "home_settings" => self.refresh_home_settings_once()?,
                _ => unreachable!("validated Standa refresh command"),
            }
        }
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(request.command)),
            ("commands".into(), Value::I64(commands.len() as i64)),
            ("state".into(), self.status_summary()),
            ("velocity".into(), velocity(self.velocity_um_s)),
            ("acceleration".into(), acceleration(self.acceleration_um_s2)),
            ("engine_settings".into(), self.engine_settings_map()),
            ("brake_settings".into(), self.brake_settings_map()),
            ("home_settings".into(), self.home_settings_map()),
            (
                "completion_basis".into(),
                Value::String("Standa mapped readback".into()),
            ),
        ])))
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        match (device, key, value) {
            (device, "position" | "target", Value::Position(_)) if device == self.axis => Ok(()),
            (device, "velocity", Value::Velocity(_)) if device == self.axis => Ok(()),
            (device, "acceleration", Value::Acceleration(_)) if device == self.axis => Ok(()),
            (device, _, _) if device == self.axis => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Standa property {key} is read-only or has the wrong type"),
            )),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                "Standa write targets an unknown device",
            )),
        }
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: &Value) -> Result<Value> {
        self.validate_write(device, key, value)?;
        match key {
            "position" | "target" => {
                let target = position_um(value)?.clamp(0.0, self.probe.travel_um);
                self.move_absolute(target)?;
                Ok(position(self.position_um))
            }
            "velocity" => {
                let value_um_s = velocity_um_s(value)?;
                self.write_velocity(value_um_s)?;
                Ok(velocity(self.velocity_um_s))
            }
            "acceleration" => {
                let value_um_s2 = acceleration_um_s2(value)?;
                self.write_acceleration(value_um_s2)?;
                Ok(acceleration(self.acceleration_um_s2))
            }
            _ => unreachable!("validated write"),
        }
    }

    fn validate_stage_move(&self, device: DeviceId, request: &StageMoveRequest) -> Result<()> {
        if device != self.axis {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Standa StageMove targets the axis device",
            ));
        }
        if request.target.len() != 1 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Standa StageMove expects exactly one axis target",
            ));
        }
        let Some((axis, _)) = request.target.iter().next() else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Standa StageMove target must contain one axis",
            ));
        };
        let supported = match axis {
            StageAxis::X => self.probe.axis == "x",
            StageAxis::Y => self.probe.axis == "y",
            StageAxis::Z => self.probe.axis == "z",
            StageAxis::Theta => self.probe.axis == "theta",
            StageAxis::Custom(name) => name == &self.probe.axis,
        };
        if !supported {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Standa StageMove axis does not match configured axis",
            ));
        }
        Ok(())
    }

    fn move_absolute(&mut self, target_um: f64) -> Result<()> {
        let target = target_um.clamp(0.0, self.probe.travel_um);
        self.target_um = target;
        let command = protocol::StandaCommand::MoveAbsolute {
            position_um: target,
        };
        self.send(&command)?;
        self.read_ack(&command)?;
        self.busy = true;
        self.wait_until_idle(Some(target), self.move_timeout(target - self.position_um))?;
        self.refresh_position_once()?;
        Ok(())
    }

    fn move_relative(&mut self, delta_um: f64) -> Result<()> {
        let final_position = (self.position_um + delta_um).clamp(0.0, self.probe.travel_um);
        let clamped_delta = final_position - self.position_um;
        self.target_um = final_position;
        let command = protocol::StandaCommand::MoveRelative {
            delta_um: clamped_delta,
        };
        self.send(&command)?;
        self.read_ack(&command)?;
        self.busy = true;
        self.wait_until_idle(Some(final_position), self.move_timeout(clamped_delta))?;
        self.refresh_position_once()?;
        Ok(())
    }

    fn current_move_settings(&mut self) -> Result<protocol::MoveSettings> {
        if !self.fixture_mode {
            self.refresh_move_settings_once()?;
        }
        Ok(self.move_settings.clone())
    }

    fn write_move_settings(&mut self, settings: protocol::MoveSettings) -> Result<()> {
        let command = protocol::StandaCommand::SetMoveSettings {
            settings: settings.clone(),
        };
        self.send(&command)?;
        self.read_ack(&command)?;
        self.move_settings = settings.clone();
        self.velocity_um_s = settings.velocity_um_s(&self.probe);
        self.acceleration_um_s2 = settings.acceleration_um_s2(&self.probe);
        self.probe.velocity_um_s = self.velocity_um_s;
        self.probe.acceleration_um_s2 = self.acceleration_um_s2;
        if !self.fixture_mode {
            self.refresh_move_settings_once()?;
        }
        self.refresh_status_once()?;
        Ok(())
    }

    fn write_velocity(&mut self, value_um_s: f64) -> Result<()> {
        let mut settings = self.current_move_settings()?;
        let (speed_steps_s, speed_microsteps_s) =
            protocol::speed_from_um_s(value_um_s, &self.probe);
        settings.speed_steps_s = speed_steps_s;
        settings.speed_microsteps_s = speed_microsteps_s;
        self.write_move_settings(settings)
    }

    fn write_acceleration(&mut self, value_um_s2: f64) -> Result<()> {
        let mut settings = self.current_move_settings()?;
        let acceleration = protocol::acceleration_from_um_s2(value_um_s2, &self.probe);
        settings.acceleration_steps_s2 = acceleration;
        settings.deceleration_steps_s2 = acceleration;
        self.write_move_settings(settings)
    }

    fn stage_move(&mut self, request: &StageMoveRequest) -> Result<Value> {
        self.validate_stage_move(self.axis, request)?;
        if request.profile.is_some() {
            let settings = request.profile.as_ref().expect("profile checked");
            if let Some(velocity) = settings.velocity {
                self.write_velocity(velocity.micrometers_per_second())?;
            }
            if let Some(acceleration) = settings.acceleration {
                self.write_acceleration(acceleration.micrometers_per_second_squared())?;
            }
        }
        let target = request
            .target
            .values()
            .next()
            .expect("validated one target")
            .micrometers();
        if request.relative {
            self.move_relative(target)?;
        } else {
            self.move_absolute(target)?;
        }
        self.emit_property("position", position(self.position_um));
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
            ("busy".into(), Value::Bool(self.busy)),
            ("velocity".into(), velocity(self.velocity_um_s)),
            ("acceleration".into(), acceleration(self.acceleration_um_s2)),
        ])))
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
            .ok_or_else(|| Error::new(ErrorCode::Unsupported, "unknown Standa capability"))?;
        match (descriptor.kind, request) {
            (CapabilityKind::StageMove, CapabilityRequest::StageMove(request)) => {
                self.stage_move(&request)
            }
            (CapabilityKind::StageHome, CapabilityRequest::None) => {
                let command = protocol::StandaCommand::Home;
                self.send(&command)?;
                self.read_ack(&command)?;
                self.busy = true;
                self.wait_until_idle(None, self.home_timeout())?;
                if !self.busy {
                    self.homed = true;
                    self.position_um = 0.0;
                }
                self.refresh_position_once()?;
                self.emit_property("position", position(self.position_um));
                self.emit_property("homed", Value::Bool(self.homed));
                self.emit_property("busy", Value::Bool(self.busy));
                Ok(Value::Map(BTreeMap::from([
                    ("homed".into(), Value::Bool(self.homed)),
                    ("position".into(), position(self.position_um)),
                    ("busy".into(), Value::Bool(self.busy)),
                ])))
            }
            (CapabilityKind::StageStop, CapabilityRequest::None) => {
                let command = protocol::StandaCommand::Stop;
                self.send(&command)?;
                self.read_ack(&command)?;
                self.busy = false;
                self.refresh_status_once()?;
                self.refresh_position_once()?;
                self.emit_property("position", position(self.position_um));
                self.emit_property("busy", Value::Bool(self.busy));
                Ok(Value::Map(BTreeMap::from([(
                    "busy".into(),
                    Value::Bool(self.busy),
                )])))
            }
            (CapabilityKind::StageMove, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Standa StageMove expects a StageMoveRequest",
            )),
            (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Standa home/stop capabilities take no request",
            )),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Standa capability",
            )),
        }
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
                    ErrorCode::InvalidCommand,
                    format!(
                        "Standa runtime timing supports only position, not {}",
                        sequence.property
                    ),
                ));
            }
            for value in &sequence.values {
                self.validate_write(sequence.device, &sequence.property, value)?;
            }
        }
        Ok(())
    }

    fn timing_summary(&self, plan: &TimingPlan, phase: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("phase".into(), Value::String(phase.into())),
            (
                "axis_participant".into(),
                Value::Bool(plan.participants.contains(&self.axis)),
            ),
            (
                "sequence_count".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
            ("position".into(), position(self.position_um)),
            ("target".into(), position(self.target_um)),
            ("busy".into(), Value::Bool(self.busy)),
            ("velocity".into(), velocity(self.velocity_um_s)),
            ("acceleration".into(), acceleration(self.acceleration_um_s2)),
            (
                "deceleration".into(),
                acceleration(self.move_settings.deceleration_um_s2(&self.probe)),
            ),
            (
                "antiplay_velocity".into(),
                velocity(self.move_settings.antiplay_velocity_um_s(&self.probe)),
            ),
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
                };
                value.map(|value| (sequence.device, sequence.property.clone(), value.clone()))
            })
            .collect::<Vec<_>>();
        let mut changed = BTreeMap::new();
        for (device, property, value) in writes {
            self.validate_write(device, &property, &value)?;
            let applied = self.write_property(device, &property, &value)?;
            self.emit_property(&property, applied.clone());
            changed.insert(property, applied);
        }
        Ok(Value::Map(changed))
    }
}

impl Driver for StandaDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: format!("{} transport", self.probe.controller),
            kind: "serial".into(),
            metadata: BTreeMap::from([
                ("baud_rate".into(), Value::I64(self.baud_rate as i64)),
                ("data_bits".into(), Value::I64(protocol::DATA_BITS as i64)),
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
                ("stop_bits".into(), Value::I64(protocol::STOP_BITS as i64)),
                ("parity".into(), Value::String(protocol::PARITY.into())),
                (
                    "flow_control".into(),
                    Value::String(protocol::FLOW_CONTROL.into()),
                ),
                ("protocol".into(), Value::String("8SMC4 v18.3".into())),
                (
                    "source".into(),
                    Value::String("Standa 8SMC4-USB communication protocol specification".into()),
                ),
                (
                    "support_scope".into(),
                    Value::String(
                        "single-axis gser/gpos/gets/gmov/geng/gbrk/ghom/smov/move/movr/home/stop command helpers"
                            .into(),
                    ),
                ),
            ]),
        }]
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        vec![
            DeviceDescriptor {
                id: self.hub,
                driver: self.id,
                label: "standa-8smc4-hub".into(),
                vendor: Some("Standa".into()),
                model: Some(self.probe.controller.clone()),
                serial: Some(self.probe.serial_number.clone()),
                kinds: vec![
                    "hub".into(),
                    "motion.controller".into(),
                    "standa.8smc4".into(),
                ],
                properties: vec![
                    string_property("controller", "Controller", false),
                    string_property("serial_number", "Serial number", false),
                    string_property("protocol", "Protocol", false),
                ],
                metadata: BTreeMap::from([
                    ("protocol_version".into(), Value::String("18.3".into())),
                    (
                        "transport".into(),
                        Value::String("serial 115200 8N2".into()),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.axis,
                driver: self.id,
                label: format!("standa-8smc4-{}", self.probe.axis),
                vendor: Some("Standa".into()),
                model: Some(self.probe.controller.clone()),
                serial: Some(self.probe.serial_number.clone()),
                kinds: vec![
                    format!("axis.{}", self.probe.axis),
                    "stage.1d".into(),
                    "standa.8smc4.axis".into(),
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
                    velocity_property("velocity", "Velocity", Some("um/s"), true),
                    acceleration_property("acceleration", "Acceleration", Some("um/s^2"), true),
                    acceleration_property("deceleration", "Deceleration", Some("um/s^2"), false),
                    velocity_property(
                        "antiplay_velocity",
                        "Antiplay velocity",
                        Some("um/s"),
                        false,
                    ),
                    bool_property("busy", "Busy", false),
                    bool_property("homed", "Homed", false),
                    bool_property("left_limit", "Left limit", false),
                    bool_property("right_limit", "Right limit", false),
                    bool_property("motor_enabled", "Motor enabled", false),
                    bool_property("encoder_present", "Encoder present", false),
                    bool_property("alarm", "Alarm", false),
                    integer_property("security_flags", "Security flags", false),
                    integer_property("power_state", "Power state", false),
                    integer_property("encoder_state", "Encoder state", false),
                    integer_property("move_state", "Move state", false),
                    integer_property("move_command_state", "Move command state", false),
                    integer_property("gpio_flags", "GPIO flags", false),
                    integer_property("raw_flags", "Raw flags", false),
                    map_property("status_summary", "Status summary", false),
                    map_property("engine_settings", "Engine settings", false),
                    map_property("brake_settings", "Brake settings", false),
                    map_property("home_settings", "Home settings", false),
                ],
                metadata: BTreeMap::from([
                    ("axis".into(), Value::String(self.probe.axis.clone())),
                    ("travel".into(), position(self.probe.travel_um)),
                    ("step_size".into(), position(self.probe.step_size_um)),
                ]),
            },
        ]
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
                        description: format!("standa read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("standa write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "standa remultiplexed stage state set".into(),
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
                    let candidate = self
                        .capabilities(*device)
                        .into_iter()
                        .find(|candidate| candidate.id == *capability)
                        .ok_or_else(|| {
                            Error::new(ErrorCode::Unsupported, "unknown Standa capability")
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
                            self.validate_generic_command(*device, request)?;
                        }
                        (CapabilityKind::StageMove, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Standa StageMove expects a StageMoveRequest",
                            ));
                        }
                        (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Standa home/stop capabilities take no request",
                            ));
                        }
                        (CapabilityKind::GenericCommand, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Standa GenericCommand expects GenericCommandRequest",
                            ));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported Standa capability",
                            ));
                        }
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("standa invoke {}", capability.0),
                        payload: Value::String(format!("{:?}", candidate.kind)),
                    });
                }
                Command::Arm(plan) => {
                    self.validate_timing_plan(plan)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "standa timing arm summary".into(),
                        payload: self.timing_summary(plan, "arm"),
                    });
                }
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
                    if device == self.axis && key == "position" {
                        self.send(&protocol::StandaCommand::GetPosition)?;
                        self.read_position_reply()?;
                    } else if device == self.axis && is_status_property(&key) {
                        self.send(&protocol::StandaCommand::GetStatus)?;
                        self.read_status_reply()?;
                    } else if device == self.axis && is_move_settings_property(&key) {
                        self.refresh_move_settings_once()?;
                    } else if device == self.axis && key == "engine_settings" {
                        self.refresh_engine_settings_once()?;
                    } else if device == self.axis && key == "brake_settings" {
                        self.refresh_brake_settings_once()?;
                    } else if device == self.axis && key == "home_settings" {
                        self.refresh_home_settings_once()?;
                    }
                    last = self.read_property(device, &key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    last = self.write_property(device, &key, &value)?;
                    self.emit_property(&key, last.clone());
                }
                Command::ApplyStateSet(set) => {
                    let mut map = BTreeMap::new();
                    for write in set.writes {
                        let value =
                            self.write_property(write.device, &write.property, &write.value)?;
                        self.emit_property(&write.property, value.clone());
                        map.insert(write.property, value);
                    }
                    last = Value::Map(map);
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    let descriptor = self
                        .capabilities(device)
                        .into_iter()
                        .find(|candidate| candidate.id == capability)
                        .ok_or_else(|| {
                            Error::new(ErrorCode::Unsupported, "unknown Standa capability")
                        })?;
                    last = match descriptor.kind {
                        CapabilityKind::GenericCommand => {
                            let CapabilityRequest::GenericCommand(request) = request else {
                                return Err(Error::new(
                                    ErrorCode::InvalidCommand,
                                    "Standa GenericCommand expects GenericCommandRequest",
                                ));
                            };
                            self.apply_generic_command(device, request)?
                        }
                        _ => self.invoke(device, capability, request)?,
                    };
                }
                Command::Arm(_) | Command::Start(_) | Command::Stop(_) => {}
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
                description: "standa timing arm summary".into(),
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
                description: "standa timing start sequence".into(),
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
                description: "standa timing stop sequence".into(),
                payload: Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "stop")),
                    ("changed".into(), changed),
                ])),
            }],
        })
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        self.pending.drain(..).collect()
    }
}

#[cfg(feature = "os-serial")]
fn query_serial_number(
    serial: &mut numanager_core::serial::OsSerialPort,
    probe: &mut protocol::StandaProbe,
) -> Result<()> {
    serial.write(&protocol::encode(
        &protocol::StandaCommand::GetSerial,
        probe,
    ))?;
    let reply = serial.read_available()?;
    if reply.is_empty() {
        return Err(Error::new(
            ErrorCode::Transport,
            "Standa gser active probe did not receive a reply",
        ));
    }
    if let Some(serial_number) = protocol::parse_serial(&reply)? {
        probe.serial_number = serial_number;
    }
    Ok(())
}

#[cfg(feature = "os-serial")]
fn query_position(
    serial: &mut numanager_core::serial::OsSerialPort,
    probe: &mut protocol::StandaProbe,
) -> Result<()> {
    serial.write(&protocol::encode(
        &protocol::StandaCommand::GetPosition,
        probe,
    ))?;
    let reply = serial.read_available()?;
    if reply.is_empty() {
        return Err(Error::new(
            ErrorCode::Transport,
            "Standa gpos active probe did not receive a reply",
        ));
    }
    if let Some(position) = protocol::parse_position(&reply, probe)? {
        probe.position_um = position.clamp(0.0, probe.travel_um);
    }
    Ok(())
}

#[cfg(feature = "os-serial")]
fn query_status(
    serial: &mut numanager_core::serial::OsSerialPort,
    probe: &mut protocol::StandaProbe,
) -> Result<()> {
    serial.write(&protocol::encode(
        &protocol::StandaCommand::GetStatus,
        probe,
    ))?;
    let reply = serial.read_available()?;
    if reply.is_empty() {
        return Err(Error::new(
            ErrorCode::Transport,
            "Standa gets active probe did not receive a reply",
        ));
    }
    if let Some(status) = protocol::parse_status(&reply, probe)? {
        probe.busy = status.moving;
        probe.homed = status.homed;
        probe.left_limit = status.gpio_flags & protocol::GPIO_LEFT_EDGE != 0;
        probe.right_limit = status.gpio_flags & protocol::GPIO_RIGHT_EDGE != 0;
        probe.motor_enabled = matches!(status.power_state, 0x03 | 0x04 | 0x05);
        probe.encoder_present = status.encoder_state != 0;
        if let Some(position) = status.current_position_um {
            probe.position_um = position.clamp(0.0, probe.travel_um);
        }
    }
    Ok(())
}

#[cfg(feature = "os-serial")]
fn query_move_settings(
    serial: &mut numanager_core::serial::OsSerialPort,
    probe: &mut protocol::StandaProbe,
) -> Result<()> {
    serial.write(&protocol::encode(
        &protocol::StandaCommand::GetMoveSettings,
        probe,
    ))?;
    let reply = serial.read_available()?;
    if reply.is_empty() {
        return Err(Error::new(
            ErrorCode::Transport,
            "Standa gmov active probe did not receive a reply",
        ));
    }
    if let Some(settings) = protocol::parse_move_settings(&reply, probe)? {
        probe.velocity_um_s = settings.velocity_um_s(probe);
        probe.acceleration_um_s2 = settings.acceleration_um_s2(probe);
    }
    Ok(())
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
) -> PropertySchema {
    PropertySchema {
        key: key.into(),
        display_name: display_name.into(),
        value_type,
        unit: unit.map(|unit| Unit(unit.into())),
        range: None,
        increment: None,
        enum_values: Vec::new(),
        readable: true,
        writable,
        volatile: false,
        sequenceable: false,
        hardware_address: None,
    }
}

fn string_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::String, None, writable)
}

fn bool_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::Bool, None, writable)
}

fn integer_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::I64, None, writable)
}

fn map_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::Map, None, writable)
}

fn property_range(
    key: &str,
    display_name: &str,
    unit: Option<&str>,
    writable: bool,
    min: f64,
    max: f64,
) -> PropertySchema {
    let mut schema = property(key, display_name, ValueType::Position, unit, writable);
    schema.range = Some(Range {
        min: position(min),
        max: position(max),
    });
    schema
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

fn velocity_property(
    key: &str,
    display_name: &str,
    unit: Option<&str>,
    writable: bool,
) -> PropertySchema {
    property(key, display_name, ValueType::Velocity, unit, writable)
}

fn acceleration_property(
    key: &str,
    display_name: &str,
    unit: Option<&str>,
    writable: bool,
) -> PropertySchema {
    property(key, display_name, ValueType::Acceleration, unit, writable)
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

fn is_status_property(key: &str) -> bool {
    matches!(
        key,
        "busy"
            | "homed"
            | "left_limit"
            | "right_limit"
            | "motor_enabled"
            | "encoder_present"
            | "alarm"
            | "security_flags"
            | "power_state"
            | "encoder_state"
            | "move_state"
            | "move_command_state"
            | "gpio_flags"
            | "raw_flags"
            | "status_summary"
    )
}

fn is_move_settings_property(key: &str) -> bool {
    matches!(
        key,
        "velocity" | "acceleration" | "deceleration" | "antiplay_velocity"
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

fn standa_endpoint_from_config(device: &DeviceConfig) -> Option<StandaSerialEndpoint> {
    let port_name = string_prop(device, "serial_port")?;
    Some(StandaSerialEndpoint {
        port_name,
        baud_rate: f64_prop(device, "baud_rate").unwrap_or(protocol::BAUD as f64) as u32,
        timeout_ms: f64_prop(device, "timeout_ms").unwrap_or(500.0) as u64,
        connect: bool_prop(device, "connect").unwrap_or(false),
    })
}
