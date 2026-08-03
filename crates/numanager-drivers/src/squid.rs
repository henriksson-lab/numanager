use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
#[cfg(feature = "os-serial")]
use numanager_core::serial::{FixedBinaryCodec, OsSerialConfig, OsSerialPort, SerialIo};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

const DEFAULT_BAUD_RATE: u32 = 2_000_000;
const COMMAND_LEN: usize = 8;
const STATUS_LEN: usize = 24;
const STATUS_INTERVAL: Duration = Duration::from_millis(10);

const BASE_NODE: u64 = 400;
const HUB_NODE: u64 = BASE_NODE;
const XY_NODE: u64 = BASE_NODE + 1;
const Z_NODE: u64 = BASE_NODE + 2;
const THETA_NODE: u64 = BASE_NODE + 3;
const FILTER_W_NODE: u64 = BASE_NODE + 4;
const FILTER_W2_NODE: u64 = BASE_NODE + 5;
const AUTOFOCUS_NODE: u64 = BASE_NODE + 6;
const LED_MATRIX_NODE: u64 = BASE_NODE + 7;
const ILLUMINATION_BASE_NODE: u64 = BASE_NODE + 10;
const TRIGGER_BASE_NODE: u64 = BASE_NODE + 30;
const ONBOARD_DAC_BASE_NODE: u64 = BASE_NODE + 50;
const ONBOARD_DAC_COUNT: usize = 8;
const SERIAL_RESOURCE_NODE: u64 = BASE_NODE + 100;
const AUTOFOCUS_LASER_PIN: u8 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum SquidCommandCode {
    MoveX = 0,
    MoveY = 1,
    MoveZ = 2,
    MoveTheta = 3,
    MoveW = 4,
    HomeOrZero = 5,
    MoveToX = 6,
    MoveToY = 7,
    MoveToZ = 8,
    SetLimit = 9,
    TurnOnIllumination = 10,
    TurnOffIllumination = 11,
    SetIllumination = 12,
    SetIlluminationLedMatrix = 13,
    AckJoystickButtonPressed = 14,
    AnalogWriteOnboardDac = 15,
    SetDac80508RefdivGain = 16,
    SetIlluminationIntensityFactor = 17,
    MoveToW = 18,
    MoveW2 = 19,
    SetLimitSwitchPolarity = 20,
    ConfigureStepperDriver = 21,
    SetMaxVelocityAcceleration = 22,
    SetLeadScrewPitch = 23,
    SetOffsetVelocity = 24,
    ConfigureStagePid = 25,
    EnableStagePid = 26,
    DisableStagePid = 27,
    SetHomeSafetyMargin = 28,
    SetPidArguments = 29,
    SendHardwareTrigger = 30,
    SetStrobeDelay = 31,
    SetAxisDisableEnable = 32,
    SetTriggerMode = 33,
    SetPortIntensity = 34,
    TurnOnPort = 35,
    TurnOffPort = 36,
    SetPortIllumination = 37,
    SetMultiPortMask = 38,
    TurnOffAllPorts = 39,
    SetWatchdogTimeout = 40,
    SetPinLevel = 41,
    Heartbeat = 42,
    MoveToW2 = 43,
    #[doc(hidden)]
    InitFilterWheelW2 = 252,
    #[doc(hidden)]
    InitFilterWheel = 253,
    #[doc(hidden)]
    Initialize = 254,
    #[doc(hidden)]
    Reset = 255,
}

impl SquidCommandCode {
    fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::MoveX,
            1 => Self::MoveY,
            2 => Self::MoveZ,
            3 => Self::MoveTheta,
            4 => Self::MoveW,
            5 => Self::HomeOrZero,
            6 => Self::MoveToX,
            7 => Self::MoveToY,
            8 => Self::MoveToZ,
            9 => Self::SetLimit,
            10 => Self::TurnOnIllumination,
            11 => Self::TurnOffIllumination,
            12 => Self::SetIllumination,
            13 => Self::SetIlluminationLedMatrix,
            14 => Self::AckJoystickButtonPressed,
            15 => Self::AnalogWriteOnboardDac,
            16 => Self::SetDac80508RefdivGain,
            17 => Self::SetIlluminationIntensityFactor,
            18 => Self::MoveToW,
            19 => Self::MoveW2,
            20 => Self::SetLimitSwitchPolarity,
            21 => Self::ConfigureStepperDriver,
            22 => Self::SetMaxVelocityAcceleration,
            23 => Self::SetLeadScrewPitch,
            24 => Self::SetOffsetVelocity,
            25 => Self::ConfigureStagePid,
            26 => Self::EnableStagePid,
            27 => Self::DisableStagePid,
            28 => Self::SetHomeSafetyMargin,
            29 => Self::SetPidArguments,
            30 => Self::SendHardwareTrigger,
            31 => Self::SetStrobeDelay,
            32 => Self::SetAxisDisableEnable,
            33 => Self::SetTriggerMode,
            34 => Self::SetPortIntensity,
            35 => Self::TurnOnPort,
            36 => Self::TurnOffPort,
            37 => Self::SetPortIllumination,
            38 => Self::SetMultiPortMask,
            39 => Self::TurnOffAllPorts,
            40 => Self::SetWatchdogTimeout,
            41 => Self::SetPinLevel,
            42 => Self::Heartbeat,
            43 => Self::MoveToW2,
            252 => Self::InitFilterWheelW2,
            253 => Self::InitFilterWheel,
            254 => Self::Initialize,
            255 => Self::Reset,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[allow(dead_code)]
enum SquidAxis {
    X = 0,
    Y = 1,
    Z = 2,
    Theta = 3,
    Xy = 4,
    W = 5,
    W2 = 6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum SquidHomeMode {
    Positive = 0,
    Negative = 1,
    Zero = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum SquidExecutionStatus {
    CompletedWithoutErrors = 0,
    InProgress = 1,
    ChecksumError = 2,
    InvalidCommand = 3,
    ExecutionError = 4,
}

impl SquidExecutionStatus {
    fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::CompletedWithoutErrors,
            1 => Self::InProgress,
            2 => Self::ChecksumError,
            3 => Self::InvalidCommand,
            4 => Self::ExecutionError,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum SquidTriggerMode {
    Edge = 0,
    Level = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LedMatrixPattern {
    FullArray,
    LeftHalf,
    RightHalf,
    LeftBlueRightRed,
    LowNa,
    LeftDot,
    RightDot,
    TopHalf,
    BottomHalf,
    ExternalFet,
}

impl LedMatrixPattern {
    fn code(self) -> u8 {
        match self {
            Self::FullArray => 0,
            Self::LeftHalf => 1,
            Self::RightHalf => 2,
            Self::LeftBlueRightRed => 3,
            Self::LowNa => 4,
            Self::LeftDot => 5,
            Self::RightDot => 6,
            Self::TopHalf => 7,
            Self::BottomHalf => 8,
            Self::ExternalFet => 20,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::FullArray => "FullArray",
            Self::LeftHalf => "LeftHalf",
            Self::RightHalf => "RightHalf",
            Self::LeftBlueRightRed => "LeftBlueRightRed",
            Self::LowNa => "LowNa",
            Self::LeftDot => "LeftDot",
            Self::RightDot => "RightDot",
            Self::TopHalf => "TopHalf",
            Self::BottomHalf => "BottomHalf",
            Self::ExternalFet => "ExternalFet",
        }
    }

    fn from_label(value: &str) -> Option<Self> {
        match value {
            "FullArray" | "full_array" | "full array" => Some(Self::FullArray),
            "LeftHalf" | "left_half" | "left half" => Some(Self::LeftHalf),
            "RightHalf" | "right_half" | "right half" => Some(Self::RightHalf),
            "LeftBlueRightRed" | "left_blue_right_red" | "left blue/right red" => {
                Some(Self::LeftBlueRightRed)
            }
            "LowNa" | "low_na" | "low NA" => Some(Self::LowNa),
            "LeftDot" | "left_dot" | "left dot" => Some(Self::LeftDot),
            "RightDot" | "right_dot" | "right dot" => Some(Self::RightDot),
            "TopHalf" | "top_half" | "top half" => Some(Self::TopHalf),
            "BottomHalf" | "bottom_half" | "bottom half" => Some(Self::BottomHalf),
            "ExternalFet" | "external_fet" | "external FET" => Some(Self::ExternalFet),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SquidCommandFrame {
    id: u8,
    code: SquidCommandCode,
    payload: [u8; 5],
}

impl SquidCommandFrame {
    fn new(id: u8, code: SquidCommandCode, payload: [u8; 5]) -> Self {
        Self { id, code, payload }
    }

    fn encode(&self) -> [u8; COMMAND_LEN] {
        let mut out = [0; COMMAND_LEN];
        out[0] = self.id;
        out[1] = self.code as u8;
        out[2..7].copy_from_slice(&self.payload);
        out[7] = crc8_ccitt(&out[..7]);
        out
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != COMMAND_LEN {
            return Err(Error::new(
                ErrorCode::Transport,
                "invalid Squid command length",
            ));
        }
        let expected = crc8_ccitt(&bytes[..7]);
        if expected != bytes[7] {
            return Err(Error::new(
                ErrorCode::Transport,
                "invalid Squid command CRC",
            ));
        }
        let code = SquidCommandCode::from_u8(bytes[1])
            .ok_or_else(|| Error::new(ErrorCode::Transport, "unknown Squid command code"))?;
        let mut payload = [0; 5];
        payload.copy_from_slice(&bytes[2..7]);
        Ok(Self {
            id: bytes[0],
            code,
            payload,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SquidStatusFrame {
    command_id: u8,
    status: SquidExecutionStatus,
    x: i32,
    y: i32,
    z: i32,
    theta: i32,
    button_switch: u8,
    firmware_version: (u8, u8),
}

impl SquidStatusFrame {
    fn encode(&self) -> [u8; STATUS_LEN] {
        let mut out = [0; STATUS_LEN];
        out[0] = self.command_id;
        out[1] = self.status as u8;
        out[2..6].copy_from_slice(&self.x.to_be_bytes());
        out[6..10].copy_from_slice(&self.y.to_be_bytes());
        out[10..14].copy_from_slice(&self.z.to_be_bytes());
        out[14..18].copy_from_slice(&self.theta.to_be_bytes());
        out[18] = self.button_switch;
        out[22] = (self.firmware_version.0 << 4) | (self.firmware_version.1 & 0x0f);
        out[23] = crc8_ccitt(&out[..23]);
        out
    }

    fn decode(bytes: &[u8], accept_zero_crc: bool) -> Result<Self> {
        if bytes.len() != STATUS_LEN {
            return Err(Error::new(
                ErrorCode::Transport,
                "invalid Squid status length",
            ));
        }
        let expected = crc8_ccitt(&bytes[..23]);
        if expected != bytes[23] && !(accept_zero_crc && bytes[23] == 0) {
            return Err(Error::new(ErrorCode::Transport, "invalid Squid status CRC"));
        }
        let status = SquidExecutionStatus::from_u8(bytes[1])
            .ok_or_else(|| Error::new(ErrorCode::Transport, "unknown Squid status code"))?;
        let version = bytes[22];
        Ok(Self {
            command_id: bytes[0],
            status,
            x: i32::from_be_bytes(bytes[2..6].try_into().expect("checked byte range")),
            y: i32::from_be_bytes(bytes[6..10].try_into().expect("checked byte range")),
            z: i32::from_be_bytes(bytes[10..14].try_into().expect("checked byte range")),
            theta: i32::from_be_bytes(bytes[14..18].try_into().expect("checked byte range")),
            button_switch: bytes[18],
            firmware_version: (version >> 4, version & 0x0f),
        })
    }
}

fn crc8_ccitt(bytes: &[u8]) -> u8 {
    let mut crc = 0u8;
    for byte in bytes {
        crc ^= *byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x07
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SquidWireCommand {
    Frame(SquidCommandFrame),
}

impl SquidWireCommand {
    fn bytes(&self) -> Vec<u8> {
        match self {
            Self::Frame(frame) => frame.encode().to_vec(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SimSquidTransport {
    x: i32,
    y: i32,
    z: i32,
    theta: i32,
    w: i32,
    w2: i32,
    illumination_on: [bool; 16],
    illumination_intensity: [u16; 16],
    onboard_dac_raw_counts: [u16; ONBOARD_DAC_COUNT],
    firmware_version: (u8, u8),
    pending: VecDeque<Vec<u8>>,
    pending_complete: Option<u8>,
    last_emit: Instant,
}

impl Default for SimSquidTransport {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            z: 0,
            theta: 0,
            w: 0,
            w2: 0,
            illumination_on: [false; 16],
            illumination_intensity: [0; 16],
            onboard_dac_raw_counts: [0; ONBOARD_DAC_COUNT],
            firmware_version: (1, 4),
            pending: VecDeque::new(),
            pending_complete: None,
            last_emit: Instant::now(),
        }
    }
}

impl SimSquidTransport {
    pub fn new() -> Self {
        Self::default()
    }

    fn queue_status(&mut self, command_id: u8, status: SquidExecutionStatus) {
        self.pending.push_back(
            SquidStatusFrame {
                command_id,
                status,
                x: self.x,
                y: self.y,
                z: self.z,
                theta: self.theta,
                button_switch: 0,
                firmware_version: self.firmware_version,
            }
            .encode()
            .to_vec(),
        );
    }

    fn apply(&mut self, frame: &SquidCommandFrame) -> SquidExecutionStatus {
        match frame.code {
            SquidCommandCode::MoveX => {
                self.x = self.x.saturating_add(i32_payload(&frame.payload[0..4]))
            }
            SquidCommandCode::MoveY => {
                self.y = self.y.saturating_add(i32_payload(&frame.payload[0..4]))
            }
            SquidCommandCode::MoveZ => {
                self.z = self.z.saturating_add(i32_payload(&frame.payload[0..4]))
            }
            SquidCommandCode::MoveTheta => {
                self.theta = self.theta.saturating_add(i32_payload(&frame.payload[0..4]))
            }
            SquidCommandCode::MoveW => {
                self.w = self.w.saturating_add(i32_payload(&frame.payload[0..4]))
            }
            SquidCommandCode::MoveW2 => {
                self.w2 = self.w2.saturating_add(i32_payload(&frame.payload[0..4]))
            }
            SquidCommandCode::MoveToX => self.x = i32_payload(&frame.payload[0..4]),
            SquidCommandCode::MoveToY => self.y = i32_payload(&frame.payload[0..4]),
            SquidCommandCode::MoveToZ => self.z = i32_payload(&frame.payload[0..4]),
            SquidCommandCode::MoveToW => self.w = i32_payload(&frame.payload[0..4]),
            SquidCommandCode::MoveToW2 => self.w2 = i32_payload(&frame.payload[0..4]),
            SquidCommandCode::HomeOrZero => {
                let axis = frame.payload[0];
                let mode = frame.payload[1];
                if mode == SquidHomeMode::Zero as u8
                    || mode == SquidHomeMode::Negative as u8
                    || mode == SquidHomeMode::Positive as u8
                {
                    match axis {
                        0 => self.x = 0,
                        1 => self.y = 0,
                        2 => self.z = 0,
                        3 => self.theta = 0,
                        4 => {
                            self.x = 0;
                            self.y = 0;
                        }
                        5 => self.w = 0,
                        6 => self.w2 = 0,
                        _ => return SquidExecutionStatus::InvalidCommand,
                    }
                }
            }
            SquidCommandCode::SetPortIntensity => {
                let port = frame.payload[0] as usize;
                if port >= self.illumination_intensity.len() {
                    return SquidExecutionStatus::InvalidCommand;
                }
                self.illumination_intensity[port] = u16_payload(&frame.payload[1..3]);
            }
            SquidCommandCode::TurnOnPort => {
                let port = frame.payload[0] as usize;
                if port >= self.illumination_on.len() {
                    return SquidExecutionStatus::InvalidCommand;
                }
                self.illumination_on[port] = true;
            }
            SquidCommandCode::TurnOffPort => {
                let port = frame.payload[0] as usize;
                if port >= self.illumination_on.len() {
                    return SquidExecutionStatus::InvalidCommand;
                }
                self.illumination_on[port] = false;
            }
            SquidCommandCode::SetPortIllumination => {
                let port = frame.payload[0] as usize;
                if port >= self.illumination_on.len() {
                    return SquidExecutionStatus::InvalidCommand;
                }
                self.illumination_intensity[port] = u16_payload(&frame.payload[1..3]);
                self.illumination_on[port] = frame.payload[3] != 0;
            }
            SquidCommandCode::SetMultiPortMask => {
                let port_mask = u16_payload(&frame.payload[0..2]);
                let on_mask = u16_payload(&frame.payload[2..4]);
                for port in 0..16 {
                    if port_mask & (1 << port) != 0 {
                        self.illumination_on[port] = on_mask & (1 << port) != 0;
                    }
                }
            }
            SquidCommandCode::TurnOffAllPorts => {
                self.illumination_on = [false; 16];
            }
            SquidCommandCode::AnalogWriteOnboardDac => {
                let channel = frame.payload[0] as usize;
                if channel >= self.onboard_dac_raw_counts.len() {
                    return SquidExecutionStatus::InvalidCommand;
                }
                self.onboard_dac_raw_counts[channel] = u16_payload(&frame.payload[1..3]);
            }
            SquidCommandCode::SetIlluminationLedMatrix => {}
            SquidCommandCode::Reset => {
                self.x = 0;
                self.y = 0;
                self.z = 0;
                self.theta = 0;
                self.w = 0;
                self.w2 = 0;
            }
            _ => {}
        }
        SquidExecutionStatus::CompletedWithoutErrors
    }
}

impl Transport for SimSquidTransport {
    fn send(&mut self, bytes: &[u8]) -> Result<()> {
        let frame = SquidCommandFrame::decode(bytes)?;
        self.queue_status(frame.id, SquidExecutionStatus::InProgress);
        let status = self.apply(&frame);
        if status == SquidExecutionStatus::CompletedWithoutErrors {
            self.pending_complete = Some(frame.id);
        } else {
            self.queue_status(frame.id, status);
        }
        Ok(())
    }

    fn poll_recv(&mut self) -> Result<Option<Vec<u8>>> {
        if let Some(bytes) = self.pending.pop_front() {
            self.last_emit = Instant::now();
            return Ok(Some(bytes));
        }
        if let Some(command_id) = self.pending_complete.take() {
            if self.last_emit.elapsed() >= STATUS_INTERVAL {
                self.queue_status(command_id, SquidExecutionStatus::CompletedWithoutErrors);
                return Ok(self.pending.pop_front());
            }
            self.pending_complete = Some(command_id);
        }
        Ok(None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogicalDevice {
    Hub,
    Xy,
    Z,
    Theta,
    FilterW,
    FilterW2,
    Autofocus,
    LedMatrix,
    OnboardDac { channel: u8 },
    Illumination { port: u8 },
    Trigger { channel: u8 },
}

pub struct SquidDriver<T: Transport = SimSquidTransport> {
    id: DriverId,
    transport: T,
    command_id: u8,
    next_token: u64,
    devices: Vec<DeviceDescriptor>,
    events: VecDeque<DriverEvent>,
    pending: VecDeque<PendingCommand>,
    accept_zero_status_crc: bool,
    serial_port: Option<String>,
    baud_rate: u32,
    connected: bool,
    firmware_version: (u8, u8),
    last_status_command_id: Option<u8>,
    last_status_code: Option<SquidExecutionStatus>,
    button_switch: u8,
    x_um: f64,
    y_um: f64,
    z_um: f64,
    theta_steps: i64,
    filter_w_position: i64,
    filter_w2_position: i64,
    illumination_enabled: [bool; 5],
    illumination_intensity_percent: [f64; 5],
    led_matrix_pattern: LedMatrixPattern,
    led_matrix_red_percent: f64,
    led_matrix_green_percent: f64,
    led_matrix_blue_percent: f64,
    onboard_dac_raw_counts: [u16; ONBOARD_DAC_COUNT],
    autofocus_laser_enabled: bool,
    autofocus_mode: AutofocusMode,
    watchdog_timeout_s: f64,
    trigger_mode: SquidTriggerMode,
}

struct PendingCommand {
    token: DriverToken,
    command_id: u8,
    device: Option<DeviceId>,
    value: Value,
}

struct LedMatrixStateUpdate {
    pattern: LedMatrixPattern,
    red_percent: f64,
    green_percent: f64,
    blue_percent: f64,
}

impl LedMatrixStateUpdate {
    fn from_driver_state(
        pattern: LedMatrixPattern,
        red_percent: f64,
        green_percent: f64,
        blue_percent: f64,
    ) -> Self {
        Self {
            pattern,
            red_percent,
            green_percent,
            blue_percent,
        }
    }

    fn apply(&mut self, key: &str, value: &Value) -> Result<()> {
        match key {
            "pattern" => self.pattern = led_matrix_pattern_value(value)?,
            "red" => self.red_percent = percent_value(value, key)?,
            "green" => self.green_percent = percent_value(value, key)?,
            "blue" => self.blue_percent = percent_value(value, key)?,
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unsupported Squid LED matrix property {key}"),
                ))
            }
        }
        Ok(())
    }
}

pub struct SquidDiscovery {
    next_id: DriverId,
    simulated: bool,
    configured: Vec<SquidConfiguredProbe>,
}

#[derive(Debug, Clone)]
pub struct SquidConfiguredProbe {
    label: String,
    endpoint: Option<SquidSerialEndpoint>,
    connect_real_transport: bool,
    accept_zero_status_crc: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SquidSerialEndpoint {
    pub port_name: String,
    pub baud_rate: u32,
    pub timeout_ms: u64,
}

impl SquidDiscovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            simulated: true,
            configured: Vec::new(),
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let configured = config
            .devices
            .iter()
            .filter(|device| device.driver == "squid")
            .map(SquidConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            next_id,
            simulated: false,
            configured,
        })
    }
}

impl DriverDiscovery for SquidDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        if self.simulated {
            return Ok(vec![DriverCandidate::from_driver(
                "Simulated Cephla Squid controller",
                Box::new(SquidDriver::simulated(self.next_id)),
            )]);
        }
        self.configured
            .iter()
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let driver: Box<dyn Driver> = if configured.connect_real_transport {
                    Box::new(SquidDriver::serial(id, configured.clone())?)
                } else {
                    let mut driver = SquidDriver::simulated(id);
                    driver.accept_zero_status_crc = configured.accept_zero_status_crc;
                    driver.apply_configured_endpoint_metadata(configured.endpoint.as_ref(), false);
                    Box::new(driver)
                };
                Ok(DriverCandidate::from_driver(
                    configured.label.clone(),
                    driver,
                ))
            })
            .collect()
    }
}

impl SquidConfiguredProbe {
    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let label = if device.label.is_empty() {
            "Configured Squid controller fixture".into()
        } else {
            device.label.clone()
        };
        let endpoint = string_prop(device, "serial_port").map(|port_name| SquidSerialEndpoint {
            port_name,
            baud_rate: u32_prop(device, "baud_rate").unwrap_or(DEFAULT_BAUD_RATE),
            timeout_ms: u64_prop(device, "serial_timeout_ms").unwrap_or(200),
        });
        Ok(Self {
            label,
            endpoint,
            connect_real_transport: bool_prop(device, "connect").unwrap_or(false),
            accept_zero_status_crc: bool_prop(device, "accept_zero_status_crc").unwrap_or(false),
        })
    }
}

impl SquidDriver<SimSquidTransport> {
    pub fn simulated(id: DriverId) -> Self {
        Self::new(id, SimSquidTransport::new())
    }
}

#[cfg(feature = "os-serial")]
pub struct OsSquidSerialTransport {
    serial: OsSerialPort,
    codec: FixedBinaryCodec,
    pending: VecDeque<Vec<u8>>,
}

#[cfg(feature = "os-serial")]
impl OsSquidSerialTransport {
    pub fn open(endpoint: SquidSerialEndpoint) -> Result<Self> {
        let serial = OsSerialPort::open_config(
            OsSerialConfig::new(endpoint.port_name, endpoint.baud_rate)
                .timeout(Duration::from_millis(endpoint.timeout_ms)),
        )?;
        Ok(Self {
            serial,
            codec: FixedBinaryCodec::new(STATUS_LEN),
            pending: VecDeque::new(),
        })
    }
}

#[cfg(feature = "os-serial")]
impl Transport for OsSquidSerialTransport {
    fn send(&mut self, bytes: &[u8]) -> Result<()> {
        self.serial.write(bytes)
    }

    fn poll_recv(&mut self) -> Result<Option<Vec<u8>>> {
        if let Some(frame) = self.pending.pop_front() {
            return Ok(Some(frame));
        }
        let bytes = self.serial.read_available()?;
        if bytes.is_empty() {
            return Ok(None);
        }
        self.pending.extend(self.codec.push(&bytes)?);
        Ok(self.pending.pop_front())
    }
}

#[cfg(feature = "os-serial")]
impl SquidDriver<OsSquidSerialTransport> {
    pub fn serial(id: DriverId, configured: SquidConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Squid real serial config requires serial_port",
            )
        })?;
        let mut driver = Self::new(id, OsSquidSerialTransport::open(endpoint.clone())?);
        driver.accept_zero_status_crc = configured.accept_zero_status_crc;
        driver.apply_configured_endpoint_metadata(Some(&endpoint), true);
        driver.ingest_available_status_frames()?;
        Ok(driver)
    }
}

#[cfg(not(feature = "os-serial"))]
impl SquidDriver<SimSquidTransport> {
    pub fn serial(_id: DriverId, configured: SquidConfiguredProbe) -> Result<Self> {
        let _ = configured.endpoint.as_ref();
        Err(Error::new(
            ErrorCode::Unsupported,
            "Squid real serial transport requires the numanager-drivers os-serial feature",
        ))
    }
}

impl<T: Transport> SquidDriver<T> {
    pub fn new(id: DriverId, transport: T) -> Self {
        Self {
            id,
            transport,
            command_id: 0,
            next_token: 1,
            devices: descriptors(id),
            events: VecDeque::new(),
            pending: VecDeque::new(),
            accept_zero_status_crc: false,
            serial_port: None,
            baud_rate: DEFAULT_BAUD_RATE,
            connected: false,
            firmware_version: (1, 4),
            last_status_command_id: None,
            last_status_code: None,
            button_switch: 0,
            x_um: 0.0,
            y_um: 0.0,
            z_um: 0.0,
            theta_steps: 0,
            filter_w_position: 0,
            filter_w2_position: 0,
            illumination_enabled: [false; 5],
            illumination_intensity_percent: [0.0; 5],
            led_matrix_pattern: LedMatrixPattern::FullArray,
            led_matrix_red_percent: 0.0,
            led_matrix_green_percent: 0.0,
            led_matrix_blue_percent: 0.0,
            onboard_dac_raw_counts: [0; ONBOARD_DAC_COUNT],
            autofocus_laser_enabled: false,
            autofocus_mode: AutofocusMode::Stop,
            watchdog_timeout_s: 5.0,
            trigger_mode: SquidTriggerMode::Edge,
        }
    }

    fn apply_configured_endpoint_metadata(
        &mut self,
        endpoint: Option<&SquidSerialEndpoint>,
        connected: bool,
    ) {
        if let Some(endpoint) = endpoint {
            self.serial_port = Some(endpoint.port_name.clone());
            self.baud_rate = endpoint.baud_rate;
        }
        self.connected = connected;
    }

    #[cfg(feature = "os-serial")]
    fn ingest_available_status_frames(&mut self) -> Result<()> {
        while let Some(bytes) = self.transport.poll_recv()? {
            let status = SquidStatusFrame::decode(&bytes, self.accept_zero_status_crc)?;
            self.apply_status_frame(&status);
            if status.status != SquidExecutionStatus::InProgress {
                break;
            }
        }
        Ok(())
    }

    fn apply_status_frame(&mut self, status: &SquidStatusFrame) {
        self.x_um = status.x as f64;
        self.y_um = status.y as f64;
        self.z_um = status.z as f64;
        self.theta_steps = status.theta as i64;
        self.button_switch = status.button_switch;
        self.firmware_version = status.firmware_version;
        self.last_status_command_id = Some(status.command_id);
        self.last_status_code = Some(status.status);
    }

    fn firmware_version_string(&self) -> String {
        format!("{}.{}", self.firmware_version.0, self.firmware_version.1)
    }

    pub fn graph(&self) -> DeviceGraph {
        let mut graph = DeviceGraph::default();
        let hub = NodeId(HUB_NODE);
        let _ = graph.insert_node(GraphNode {
            id: hub,
            kind: NodeKind::Hub,
            label: "squid-controller".into(),
        });
        for device in &self.devices {
            if device.id.0 != hub {
                let _ = graph.insert_node(GraphNode {
                    id: device.id.0,
                    kind: NodeKind::Device,
                    label: device.label.clone(),
                });
                let _ = graph.insert_edge(GraphEdge {
                    from: hub,
                    to: device.id.0,
                    kind: EdgeKind::OffersDevice,
                });
            }
        }
        let _ =
            graph.insert_device_dependency(NodeId(Z_NODE), NodeId(AUTOFOCUS_NODE), Role::ZStage);
        let _ = graph.insert_device_dependency(
            NodeId(ILLUMINATION_BASE_NODE),
            NodeId(AUTOFOCUS_NODE),
            Role::LightSource,
        );
        graph
    }

    fn next_driver_token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn next_command_id(&mut self) -> u8 {
        self.command_id = self.command_id.wrapping_add(1);
        self.command_id
    }

    fn classify(&self, device: DeviceId) -> Option<LogicalDevice> {
        match device.0 .0 {
            HUB_NODE => Some(LogicalDevice::Hub),
            XY_NODE => Some(LogicalDevice::Xy),
            Z_NODE => Some(LogicalDevice::Z),
            THETA_NODE => Some(LogicalDevice::Theta),
            FILTER_W_NODE => Some(LogicalDevice::FilterW),
            FILTER_W2_NODE => Some(LogicalDevice::FilterW2),
            AUTOFOCUS_NODE => Some(LogicalDevice::Autofocus),
            LED_MATRIX_NODE => Some(LogicalDevice::LedMatrix),
            node if (ILLUMINATION_BASE_NODE..ILLUMINATION_BASE_NODE + 5).contains(&node) => {
                Some(LogicalDevice::Illumination {
                    port: (node - ILLUMINATION_BASE_NODE) as u8,
                })
            }
            node if (TRIGGER_BASE_NODE..TRIGGER_BASE_NODE + 4).contains(&node) => {
                Some(LogicalDevice::Trigger {
                    channel: (node - TRIGGER_BASE_NODE) as u8,
                })
            }
            node if (ONBOARD_DAC_BASE_NODE..ONBOARD_DAC_BASE_NODE + ONBOARD_DAC_COUNT as u64)
                .contains(&node) =>
            {
                Some(LogicalDevice::OnboardDac {
                    channel: (node - ONBOARD_DAC_BASE_NODE) as u8,
                })
            }
            _ => None,
        }
    }

    fn owns_device(&self, device: DeviceId) -> bool {
        self.classify(device).is_some()
    }

    fn command_bytes_for(&self, command: &Command) -> Result<Vec<SquidWireCommand>> {
        let mut commands = Vec::new();
        match command {
            Command::WriteProperty { device, key, value } => {
                commands.extend(self.write_property_commands(*device, key, value)?);
            }
            Command::Invoke {
                device,
                capability,
                request,
            } => {
                commands.extend(self.invoke_commands(*device, *capability, request)?);
            }
            Command::ApplyStateSet(set) => {
                commands.extend(self.state_set_commands(set)?);
            }
            Command::ReadProperty { .. } => {}
            _ => {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    "Squid driver does not support this command",
                ))
            }
        }
        Ok(commands)
    }

    fn write_property_commands(
        &self,
        device: DeviceId,
        key: &str,
        value: &Value,
    ) -> Result<Vec<SquidWireCommand>> {
        let Some(kind) = self.classify(device) else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown Squid device",
            ));
        };
        let command = match (kind, key) {
            (LogicalDevice::Xy, "x") => {
                move_to(SquidCommandCode::MoveToX, position_um_to_steps(value)?)
            }
            (LogicalDevice::Xy, "y") => {
                move_to(SquidCommandCode::MoveToY, position_um_to_steps(value)?)
            }
            (LogicalDevice::Z, "z") => {
                move_to(SquidCommandCode::MoveToZ, position_um_to_steps(value)?)
            }
            (LogicalDevice::Theta, "position_steps") => {
                move_to(SquidCommandCode::MoveTheta, step_count_i32(value)?)
            }
            (LogicalDevice::FilterW, "position") => {
                move_to(SquidCommandCode::MoveToW, step_count_i32(value)?)
            }
            (LogicalDevice::FilterW2, "position") => {
                move_to(SquidCommandCode::MoveToW2, step_count_i32(value)?)
            }
            (LogicalDevice::Illumination { port }, key) if is_intensity_key(key) => {
                set_port_intensity(port, percent_to_u16(value)?)
            }
            (LogicalDevice::Illumination { port }, "enabled") => {
                let enabled = bool_value(value, "enabled")?;
                port_enabled(port, enabled)
            }
            (LogicalDevice::Hub, "watchdog_timeout") => {
                let timeout_ms = (time_seconds(value, "watchdog_timeout")? * 1000.0)
                    .round()
                    .clamp(0.0, 3_600_000.0) as u32;
                watchdog_timeout(timeout_ms)
            }
            (LogicalDevice::LedMatrix, "pattern") => {
                let pattern = led_matrix_pattern_value(value)?;
                set_led_matrix(
                    pattern,
                    percent_float_to_u8(self.led_matrix_green_percent),
                    percent_float_to_u8(self.led_matrix_red_percent),
                    percent_float_to_u8(self.led_matrix_blue_percent),
                )
            }
            (LogicalDevice::LedMatrix, "red" | "green" | "blue") => {
                let mut red = self.led_matrix_red_percent;
                let mut green = self.led_matrix_green_percent;
                let mut blue = self.led_matrix_blue_percent;
                let percent = percent_value(value, key)?;
                match key {
                    "red" => red = percent,
                    "green" => green = percent,
                    "blue" => blue = percent,
                    _ => {}
                }
                set_led_matrix(
                    self.led_matrix_pattern,
                    percent_float_to_u8(green),
                    percent_float_to_u8(red),
                    percent_float_to_u8(blue),
                )
            }
            (LogicalDevice::OnboardDac { channel }, "raw_counts") => {
                onboard_dac_write(channel, raw_u16_value(value, "raw_counts")?)
            }
            (LogicalDevice::Trigger { .. }, "mode") => {
                let mode = trigger_mode_value(value)?;
                simple_payload(SquidCommandCode::SetTriggerMode, [mode as u8, 0, 0, 0, 0])
            }
            (LogicalDevice::Autofocus, "enabled" | "laser_enabled") => {
                pin_level(AUTOFOCUS_LASER_PIN, bool_value(value, key)?)
            }
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unsupported Squid property {key}"),
                ))
            }
        };
        Ok(vec![SquidWireCommand::Frame(command)])
    }

    fn invoke_commands(
        &self,
        device: DeviceId,
        capability: CapabilityId,
        request: &CapabilityRequest,
    ) -> Result<Vec<SquidWireCommand>> {
        let Some(kind) = self.classify(device) else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown Squid device",
            ));
        };
        match (kind, capability.0, request) {
            (LogicalDevice::Xy, 401, CapabilityRequest::StageMove(request)) => {
                self.stage_move_commands(kind, request)
            }
            (LogicalDevice::Z, 402, CapabilityRequest::StageMove(request)) => {
                self.stage_move_commands(kind, request)
            }
            (LogicalDevice::Xy, 403, CapabilityRequest::None) => Ok(vec![SquidWireCommand::Frame(
                home_command(LogicalDevice::Xy)?,
            )]),
            (LogicalDevice::Z, 403, CapabilityRequest::None) => Ok(vec![SquidWireCommand::Frame(
                home_command(LogicalDevice::Z)?,
            )]),
            (LogicalDevice::Illumination { port }, 410, CapabilityRequest::Dac(request)) => {
                Ok(vec![SquidWireCommand::Frame(set_port_intensity(
                    port,
                    percent_to_u16(&request.value)?,
                ))])
            }
            (LogicalDevice::Trigger { channel }, 430, CapabilityRequest::Trigger(req)) => {
                if req.action != numanager_core::TriggerAction::Pulse {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "Squid trigger source supports pulse requests",
                    ));
                }
                let on_time_us = req
                    .duration
                    .map(|duration| duration.microseconds().round() as u32)
                    .unwrap_or(0);
                Ok(vec![SquidWireCommand::Frame(hardware_trigger(
                    channel,
                    req.control_illumination.unwrap_or(false),
                    on_time_us,
                ))])
            }
            (LogicalDevice::Autofocus, 440, CapabilityRequest::Autofocus(req)) => {
                Ok(vec![SquidWireCommand::Frame(pin_level(
                    AUTOFOCUS_LASER_PIN,
                    autofocus_mode_enables_laser(req.mode),
                ))])
            }
            (_, _, CapabilityRequest::GenericCommand(request)) => {
                if request.is_hidden_maintenance() {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        format!(
                            "GenericCommand {} is a hidden maintenance operation",
                            request.command
                        ),
                    ));
                }
                Ok(vec![SquidWireCommand::Frame(generic_command_frame(
                    kind, request,
                )?)])
            }
            (_, _, _)
                if matches!(
                    kind,
                    LogicalDevice::Hub
                        | LogicalDevice::Theta
                        | LogicalDevice::FilterW
                        | LogicalDevice::FilterW2
                ) =>
            {
                Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "Squid GenericCommand expects GenericCommandRequest",
                ))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidCommand,
                "unsupported Squid capability invocation",
            )),
        }
    }

    fn stage_move_commands(
        &self,
        kind: LogicalDevice,
        request: &StageMoveRequest,
    ) -> Result<Vec<SquidWireCommand>> {
        if request.target.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Squid StageMove requires at least one target axis",
            ));
        }
        let mut commands = Vec::new();
        match kind {
            LogicalDevice::Xy => {
                for (axis, target) in &request.target {
                    match axis {
                        StageAxis::X => {
                            let value = if request.relative {
                                self.x_um + target.micrometers()
                            } else {
                                target.micrometers()
                            };
                            commands.push(SquidWireCommand::Frame(move_to(
                                SquidCommandCode::MoveToX,
                                position_um_f64_to_steps(value)?,
                            )));
                        }
                        StageAxis::Y => {
                            let value = if request.relative {
                                self.y_um + target.micrometers()
                            } else {
                                target.micrometers()
                            };
                            commands.push(SquidWireCommand::Frame(move_to(
                                SquidCommandCode::MoveToY,
                                position_um_f64_to_steps(value)?,
                            )));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                format!("axis {} is not available on Squid XY", axis.name()),
                            ))
                        }
                    }
                }
            }
            LogicalDevice::Z => {
                for (axis, target) in &request.target {
                    match axis {
                        StageAxis::Z => {
                            let value = if request.relative {
                                self.z_um + target.micrometers()
                            } else {
                                target.micrometers()
                            };
                            commands.push(SquidWireCommand::Frame(move_to(
                                SquidCommandCode::MoveToZ,
                                position_um_f64_to_steps(value)?,
                            )));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                format!("axis {} is not available on Squid Z", axis.name()),
                            ))
                        }
                    }
                }
            }
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "Squid StageMove requires an XY or Z stage device",
                ))
            }
        }
        Ok(commands)
    }

    fn state_set_commands(&self, set: &StateSet) -> Result<Vec<SquidWireCommand>> {
        let mut commands = Vec::new();
        let mut illumination: BTreeMap<u8, (Option<f64>, Option<bool>)> = BTreeMap::new();
        let mut led_matrix: Option<LedMatrixStateUpdate> = None;

        for write in &set.writes {
            match self.classify(write.device) {
                Some(LogicalDevice::Illumination { port }) if is_intensity_key(&write.property) => {
                    illumination.entry(port).or_default().0 =
                        Some(percent_value(&write.value, "intensity")?);
                }
                Some(LogicalDevice::Illumination { port }) if write.property == "enabled" => {
                    illumination.entry(port).or_default().1 =
                        Some(bool_value(&write.value, "enabled")?);
                }
                Some(LogicalDevice::LedMatrix) => {
                    let update = led_matrix.get_or_insert_with(|| {
                        LedMatrixStateUpdate::from_driver_state(
                            self.led_matrix_pattern,
                            self.led_matrix_red_percent,
                            self.led_matrix_green_percent,
                            self.led_matrix_blue_percent,
                        )
                    });
                    update.apply(&write.property, &write.value)?;
                }
                _ => commands.extend(self.write_property_commands(
                    write.device,
                    &write.property,
                    &write.value,
                )?),
            }
        }

        for (port, (intensity, enabled)) in illumination {
            match (intensity, enabled) {
                (Some(intensity), Some(enabled)) => commands.push(SquidWireCommand::Frame(
                    set_port_illumination(port, percent_float_to_u16(intensity), enabled),
                )),
                (Some(intensity), None) => commands.push(SquidWireCommand::Frame(
                    set_port_intensity(port, percent_float_to_u16(intensity)),
                )),
                (None, Some(enabled)) => {
                    commands.push(SquidWireCommand::Frame(port_enabled(port, enabled)))
                }
                (None, None) => {}
            }
        }
        if let Some(update) = led_matrix {
            commands.push(SquidWireCommand::Frame(set_led_matrix(
                update.pattern,
                percent_float_to_u8(update.green_percent),
                percent_float_to_u8(update.red_percent),
                percent_float_to_u8(update.blue_percent),
            )));
        }
        Ok(commands)
    }

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| {
                matches!(
                    self.classify(sequence.device),
                    Some(
                        LogicalDevice::Xy
                            | LogicalDevice::Z
                            | LogicalDevice::Illumination { .. }
                            | LogicalDevice::LedMatrix
                            | LogicalDevice::Autofocus
                    )
                )
            })
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequences(plan) {
            match (self.classify(sequence.device), sequence.property.as_str()) {
                (Some(LogicalDevice::Xy), "x" | "y") => {
                    for value in &sequence.values {
                        let _ = position_um(value)?;
                    }
                }
                (Some(LogicalDevice::Z), "z") => {
                    for value in &sequence.values {
                        let _ = position_um(value)?;
                    }
                }
                (Some(LogicalDevice::Illumination { .. }), "enabled") => {
                    for value in &sequence.values {
                        let _ = bool_value(value, "enabled")?;
                    }
                }
                (Some(LogicalDevice::Illumination { .. }), key) if is_intensity_key(key) => {
                    for value in &sequence.values {
                        let _ = percent_value(value, "intensity")?;
                    }
                }
                (Some(LogicalDevice::LedMatrix), "red" | "green" | "blue") => {
                    for value in &sequence.values {
                        let _ = percent_value(value, &sequence.property)?;
                    }
                }
                (Some(LogicalDevice::LedMatrix), "pattern") => {
                    for value in &sequence.values {
                        let _ = led_matrix_pattern_value(value)?;
                    }
                }
                (Some(LogicalDevice::Autofocus), "enabled" | "laser_enabled") => {
                    for value in &sequence.values {
                        let _ = bool_value(value, &sequence.property)?;
                    }
                }
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Squid timing sequences can target stage positions, illumination, LED matrix state, or autofocus enable state",
                    ))
                }
            }
        }
        Ok(())
    }

    fn timing_summary(&self, plan: &TimingPlan, phase: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("phase".into(), Value::String(phase.into())),
            (
                "participants".into(),
                Value::List(
                    plan.participants
                        .iter()
                        .filter(|device| self.owns_device(**device))
                        .map(|device| Value::I64(device.0 .0 as i64))
                        .collect(),
                ),
            ),
            (
                "routes".into(),
                Value::List(
                    plan.routes
                        .iter()
                        .filter(|route| self.owns_device(route.from) || self.owns_device(route.to))
                        .map(|route| {
                            Value::Map(BTreeMap::from([
                                ("from".into(), Value::I64(route.from.0 .0 as i64)),
                                ("to".into(), Value::I64(route.to.0 .0 as i64)),
                                (
                                    "signal".into(),
                                    Value::String(format!("{:?}", route.signal)),
                                ),
                                ("edge".into(), Value::String(format!("{:?}", route.edge))),
                            ]))
                        })
                        .collect(),
                ),
            ),
            (
                "sequences".into(),
                Value::List(
                    self.local_timing_sequences(plan)
                        .into_iter()
                        .map(|sequence| {
                            Value::Map(BTreeMap::from([
                                ("device".into(), Value::I64(sequence.device.0 .0 as i64)),
                                ("property".into(), Value::String(sequence.property.clone())),
                                ("count".into(), Value::I64(sequence.values.len() as i64)),
                            ]))
                        })
                        .collect(),
                ),
            ),
        ]))
    }

    fn timing_trigger_commands(&self, plan: &TimingPlan) -> Vec<SquidWireCommand> {
        let mut channels = plan
            .participants
            .iter()
            .chain(
                plan.routes
                    .iter()
                    .flat_map(|route| [&route.from, &route.to]),
            )
            .filter_map(|device| match self.classify(*device) {
                Some(LogicalDevice::Trigger { channel }) => Some(channel),
                _ => None,
            })
            .collect::<Vec<_>>();
        channels.sort_unstable();
        channels.dedup();
        channels
            .into_iter()
            .map(|channel| SquidWireCommand::Frame(hardware_trigger(channel, false, 0)))
            .collect()
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
        let set = StateSet {
            name: Some(if first {
                "squid timing start sequence".into()
            } else {
                "squid timing stop sequence".into()
            }),
            writes,
            commit: CommitMode::Immediate,
        };
        let commands = self.state_set_commands(&set)?;
        self.send_timing_commands(&commands)?;
        let command = Command::ApplyStateSet(set);
        self.apply_local_state(&command);
        publish_property_events(&mut self.events, &command);
        Ok(Value::Map(BTreeMap::from([(
            "serial_frames".into(),
            Value::I64(commands.len() as i64),
        )])))
    }

    fn send_timing_commands(&mut self, commands: &[SquidWireCommand]) -> Result<Vec<Value>> {
        let mut payloads = Vec::new();
        for wire in commands {
            let mut frame = match wire {
                SquidWireCommand::Frame(frame) => frame.clone(),
            };
            frame.id = self.next_command_id();
            let bytes = frame.encode();
            self.transport.send(&bytes)?;
            payloads.push(Value::Bytes(bytes.to_vec()));
        }
        Ok(payloads)
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        let Some(kind) = self.classify(device) else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown Squid device",
            ));
        };
        Ok(match (kind, key) {
            (LogicalDevice::Hub, "firmware_version") => {
                Value::String(self.firmware_version_string())
            }
            (LogicalDevice::Hub, "watchdog_timeout") => time_interval(self.watchdog_timeout_s),
            (LogicalDevice::Xy, "x") => position(self.x_um),
            (LogicalDevice::Xy, "y") => position(self.y_um),
            (LogicalDevice::Z, "z") => position(self.z_um),
            (LogicalDevice::Theta, "position_steps") => step_count(self.theta_steps),
            (LogicalDevice::FilterW, "position") => step_count(self.filter_w_position),
            (LogicalDevice::FilterW2, "position") => step_count(self.filter_w2_position),
            (LogicalDevice::LedMatrix, "pattern") => {
                Value::String(self.led_matrix_pattern.label().into())
            }
            (LogicalDevice::LedMatrix, "red") => {
                Value::Ratio(Ratio::from_percent(self.led_matrix_red_percent))
            }
            (LogicalDevice::LedMatrix, "green") => {
                Value::Ratio(Ratio::from_percent(self.led_matrix_green_percent))
            }
            (LogicalDevice::LedMatrix, "blue") => {
                Value::Ratio(Ratio::from_percent(self.led_matrix_blue_percent))
            }
            (LogicalDevice::OnboardDac { channel }, "raw_counts") => {
                Value::I64(self.onboard_dac_raw_counts[channel as usize] as i64)
            }
            (LogicalDevice::Illumination { port }, key) if is_intensity_key(key) => Value::Ratio(
                Ratio::from_percent(self.illumination_intensity_percent[port as usize]),
            ),
            (LogicalDevice::Illumination { port }, "enabled") => {
                Value::Bool(self.illumination_enabled[port as usize])
            }
            (LogicalDevice::Illumination { port }, "wavelength") => {
                let nm = [405.0, 488.0, 561.0, 638.0, 730.0][port as usize];
                Value::Wavelength(Wavelength::from_nanometers(nm))
            }
            (LogicalDevice::Trigger { .. }, "mode") => Value::String(match self.trigger_mode {
                SquidTriggerMode::Edge => "edge".into(),
                SquidTriggerMode::Level => "level".into(),
            }),
            (LogicalDevice::Autofocus, "laser_enabled") => {
                Value::Bool(self.autofocus_laser_enabled)
            }
            (LogicalDevice::Autofocus, "enabled") => Value::Bool(self.autofocus_laser_enabled),
            (LogicalDevice::Autofocus, "mode") => {
                Value::String(autofocus_mode_name(self.autofocus_mode))
            }
            (LogicalDevice::Autofocus, "status") => {
                Value::String(if self.autofocus_laser_enabled {
                    "active".into()
                } else {
                    "idle".into()
                })
            }
            (LogicalDevice::Autofocus, "focus_score") => {
                Value::F64(if self.autofocus_laser_enabled {
                    1.0
                } else {
                    0.0
                })
            }
            (LogicalDevice::Autofocus, "kind") => Value::String("laser triangulation".into()),
            _ => Value::Null,
        })
    }

    fn apply_local_state(&mut self, command: &Command) {
        match command {
            Command::WriteProperty { device, key, value } => {
                self.apply_write(*device, key, value);
            }
            Command::ApplyStateSet(set) => {
                for write in &set.writes {
                    self.apply_write(write.device, &write.property, &write.value);
                }
            }
            Command::Invoke {
                device,
                capability,
                request,
            } => {
                if self.classify(*device) == Some(LogicalDevice::Xy)
                    && *capability == CapabilityId(403)
                    && matches!(request, CapabilityRequest::None)
                {
                    self.x_um = 0.0;
                    self.y_um = 0.0;
                }
                if self.classify(*device) == Some(LogicalDevice::Z)
                    && *capability == CapabilityId(403)
                    && matches!(request, CapabilityRequest::None)
                {
                    self.z_um = 0.0;
                }
                if let CapabilityRequest::StageMove(request) = request {
                    self.apply_stage_move(*device, request);
                }
                if let (Some(LogicalDevice::Illumination { port }), CapabilityId(410)) =
                    (self.classify(*device), *capability)
                {
                    if let CapabilityRequest::Dac(request) = request {
                        if let Ok(value) = percent_value(&request.value, "intensity") {
                            self.illumination_intensity_percent[port as usize] =
                                value.clamp(0.0, 100.0);
                        }
                    }
                }
                if self.classify(*device) == Some(LogicalDevice::Autofocus)
                    && *capability == CapabilityId(440)
                {
                    if let CapabilityRequest::Autofocus(req) = request {
                        self.autofocus_mode = req.mode;
                        self.autofocus_laser_enabled = autofocus_mode_enables_laser(req.mode);
                    }
                }
                if let CapabilityRequest::GenericCommand(request) = request {
                    self.apply_generic_local_state(*device, request);
                }
            }
            _ => {}
        }
    }

    fn apply_generic_local_state(&mut self, device: DeviceId, request: &GenericCommandRequest) {
        match (self.classify(device), request.command.as_str()) {
            (Some(LogicalDevice::Hub), "disable_all_ports") => {
                self.illumination_enabled = [false; 5];
            }
            _ => {}
        }
    }

    fn apply_stage_move(&mut self, device: DeviceId, request: &StageMoveRequest) {
        match self.classify(device) {
            Some(LogicalDevice::Xy) => {
                if let Some(target) = request.target.get(&StageAxis::X) {
                    self.x_um = if request.relative {
                        self.x_um + target.micrometers()
                    } else {
                        target.micrometers()
                    };
                }
                if let Some(target) = request.target.get(&StageAxis::Y) {
                    self.y_um = if request.relative {
                        self.y_um + target.micrometers()
                    } else {
                        target.micrometers()
                    };
                }
            }
            Some(LogicalDevice::Z) => {
                if let Some(target) = request.target.get(&StageAxis::Z) {
                    self.z_um = if request.relative {
                        self.z_um + target.micrometers()
                    } else {
                        target.micrometers()
                    };
                }
            }
            _ => {}
        }
    }

    fn apply_write(&mut self, device: DeviceId, key: &str, value: &Value) {
        match (self.classify(device), key) {
            (Some(LogicalDevice::Xy), "x") => {
                if let Ok(v) = position_um(value) {
                    self.x_um = v;
                }
            }
            (Some(LogicalDevice::Xy), "y") => {
                if let Ok(v) = position_um(value) {
                    self.y_um = v;
                }
            }
            (Some(LogicalDevice::Z), "z") => {
                if let Ok(v) = position_um(value) {
                    self.z_um = v;
                }
            }
            (Some(LogicalDevice::Illumination { port }), key) if is_intensity_key(key) => {
                if let Ok(v) = percent_value(value, "intensity") {
                    self.illumination_intensity_percent[port as usize] = v.clamp(0.0, 100.0);
                }
            }
            (Some(LogicalDevice::Illumination { port }), "enabled") => {
                if let Value::Bool(v) = value {
                    self.illumination_enabled[port as usize] = *v;
                }
            }
            (Some(LogicalDevice::Hub), "watchdog_timeout") => {
                if let Ok(v) = time_seconds(value, "watchdog_timeout") {
                    self.watchdog_timeout_s = v.clamp(0.0, 3600.0);
                }
            }
            (Some(LogicalDevice::LedMatrix), "pattern") => {
                if let Ok(pattern) = led_matrix_pattern_value(value) {
                    self.led_matrix_pattern = pattern;
                }
            }
            (Some(LogicalDevice::LedMatrix), "red") => {
                if let Ok(percent) = percent_value(value, "red") {
                    self.led_matrix_red_percent = percent.clamp(0.0, 100.0);
                }
            }
            (Some(LogicalDevice::LedMatrix), "green") => {
                if let Ok(percent) = percent_value(value, "green") {
                    self.led_matrix_green_percent = percent.clamp(0.0, 100.0);
                }
            }
            (Some(LogicalDevice::LedMatrix), "blue") => {
                if let Ok(percent) = percent_value(value, "blue") {
                    self.led_matrix_blue_percent = percent.clamp(0.0, 100.0);
                }
            }
            (Some(LogicalDevice::OnboardDac { channel }), "raw_counts") => {
                if let Ok(counts) = raw_u16_value(value, "raw_counts") {
                    self.onboard_dac_raw_counts[channel as usize] = counts;
                }
            }
            (Some(LogicalDevice::Trigger { .. }), "mode") => {
                if let Ok(mode) = trigger_mode_value(value) {
                    self.trigger_mode = mode;
                }
            }
            (Some(LogicalDevice::Autofocus), "enabled" | "laser_enabled") => {
                if let Value::Bool(v) = value {
                    self.autofocus_laser_enabled = *v;
                    self.autofocus_mode = if *v {
                        AutofocusMode::Hold
                    } else {
                        AutofocusMode::Stop
                    };
                }
            }
            _ => {}
        }
    }
}

impl<T: Transport> Driver for SquidDriver<T> {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        self.devices.clone()
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: ResourceId(NodeId(SERIAL_RESOURCE_NODE)),
            driver: self.id,
            label: "squid-serial-controller".into(),
            kind: "usb.serial".into(),
            metadata: BTreeMap::from([
                ("baud_rate".into(), Value::I64(self.baud_rate as i64)),
                ("connected".into(), Value::Bool(self.connected)),
                (
                    "serial_port".into(),
                    self.serial_port
                        .as_ref()
                        .map(|port| Value::String(port.clone()))
                        .unwrap_or(Value::Null),
                ),
                ("status_length".into(), Value::I64(STATUS_LEN as i64)),
                ("command_length".into(), Value::I64(COMMAND_LEN as i64)),
                (
                    "last_status_command_id".into(),
                    self.last_status_command_id
                        .map(|id| Value::I64(id as i64))
                        .unwrap_or(Value::Null),
                ),
                (
                    "last_status".into(),
                    self.last_status_code
                        .map(|status| Value::String(format!("{status:?}")))
                        .unwrap_or(Value::Null),
                ),
                (
                    "button_switch".into(),
                    Value::I64(self.button_switch as i64),
                ),
            ]),
        }]
    }

    fn graph(&self) -> DeviceGraph {
        SquidDriver::graph(self)
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        match self.classify(device) {
            Some(LogicalDevice::Hub) => vec![capability(
                400,
                device,
                CapabilityKind::GenericCommand,
                ValueType::Map,
                ValueType::Map,
            )],
            Some(LogicalDevice::Xy) => vec![
                capability(
                    401,
                    device,
                    CapabilityKind::StageMove,
                    ValueType::Map,
                    ValueType::Map,
                ),
                capability(
                    403,
                    device,
                    CapabilityKind::StageHome,
                    ValueType::Map,
                    ValueType::Map,
                ),
            ],
            Some(LogicalDevice::Z) => vec![
                capability(
                    402,
                    device,
                    CapabilityKind::StageMove,
                    ValueType::Map,
                    ValueType::Map,
                ),
                capability(
                    403,
                    device,
                    CapabilityKind::StageHome,
                    ValueType::Map,
                    ValueType::Map,
                ),
            ],
            Some(LogicalDevice::Trigger { .. }) => vec![capability(
                430,
                device,
                CapabilityKind::TriggerSource,
                ValueType::Map,
                ValueType::Map,
            )],
            Some(LogicalDevice::Illumination { .. }) => vec![capability(
                410,
                device,
                CapabilityKind::Dac,
                ValueType::Map,
                ValueType::Map,
            )],
            Some(LogicalDevice::LedMatrix | LogicalDevice::OnboardDac { .. }) => Vec::new(),
            Some(LogicalDevice::Autofocus) => vec![capability(
                440,
                device,
                CapabilityKind::Autofocus,
                ValueType::Map,
                ValueType::Map,
            )],
            Some(LogicalDevice::Theta | LogicalDevice::FilterW | LogicalDevice::FilterW2) => {
                Vec::new()
            }
            None => Vec::new(),
        }
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        let mut transactions = Vec::new();
        for command in &batch.commands {
            match command {
                Command::Arm(plan) => {
                    self.validate_timing_plan(plan)?;
                    transactions.push(PhysicalTransaction {
                        resource: Some(ResourceId(NodeId(SERIAL_RESOURCE_NODE))),
                        description: "squid timing arm summary".into(),
                        payload: self.timing_summary(plan, "arm"),
                    });
                }
                Command::Start(_) | Command::Stop(_) => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "Squid direct timing transitions are runtime-owned",
                    ));
                }
                _ => {
                    let wire = self.command_bytes_for(command)?;
                    if !wire.is_empty() {
                        transactions.push(PhysicalTransaction {
                            resource: Some(ResourceId(NodeId(SERIAL_RESOURCE_NODE))),
                            description: format!("{} Squid serial frame(s)", wire.len()),
                            payload: Value::List(
                                wire.iter()
                                    .map(|command| Value::Bytes(command.bytes()))
                                    .collect(),
                            ),
                        });
                    }
                }
            }
        }
        Ok(PreparedBatch {
            id: batch.id,
            commands: batch.commands.clone(),
            physical_transactions: transactions,
        })
    }

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.next_driver_token();
        let mut last_command_id = None;
        let mut serial_frames = 0i64;
        let mut result = Value::Null;

        for command in &prepared.commands {
            if let Command::ReadProperty { device, key } = command {
                result = self.read_property(*device, key)?;
                continue;
            }
            for wire in self.command_bytes_for(command)? {
                let mut frame = match wire {
                    SquidWireCommand::Frame(frame) => frame,
                };
                frame.id = self.next_command_id();
                let bytes = frame.encode();
                self.transport.send(&bytes)?;
                last_command_id = Some(frame.id);
                serial_frames += 1;
            }
            self.apply_local_state(command);
            publish_property_events(&mut self.events, command);
            result = Value::Map(BTreeMap::from([
                (
                    "physical_transactions".into(),
                    Value::I64(prepared.physical_transactions.len() as i64),
                ),
                ("serial_frames".into(), Value::I64(serial_frames)),
            ]));
        }

        if let Some(command_id) = last_command_id {
            self.pending.push_back(PendingCommand {
                token,
                command_id,
                device: prepared
                    .commands
                    .first()
                    .and_then(|command| command.target_devices().first().copied()),
                value: result,
            });
        } else {
            self.events.push_back(DriverEvent::TokenCompleted {
                token,
                value: result,
            });
        }
        Ok(token)
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        while let Ok(Some(bytes)) = self.transport.poll_recv() {
            let status = match SquidStatusFrame::decode(&bytes, self.accept_zero_status_crc) {
                Ok(status) => status,
                Err(error) => {
                    self.events
                        .push_back(DriverEvent::Event(Event::Fault(FaultEvent {
                            device: None,
                            report: error.into(),
                        })));
                    continue;
                }
            };

            self.apply_status_frame(&status);

            self.events
                .push_back(DriverEvent::Event(Event::Telemetry(TelemetryEvent {
                    device: DeviceId(NodeId(HUB_NODE)),
                    values: BTreeMap::from([
                        ("command_id".into(), Value::I64(status.command_id as i64)),
                        ("x".into(), position(self.x_um)),
                        ("y".into(), position(self.y_um)),
                        ("z".into(), position(self.z_um)),
                        ("theta_steps".into(), Value::I64(self.theta_steps)),
                        (
                            "firmware_version".into(),
                            Value::String(self.firmware_version_string()),
                        ),
                    ]),
                })));

            if status.status == SquidExecutionStatus::InProgress {
                continue;
            }

            if let Some(index) = self
                .pending
                .iter()
                .position(|pending| pending.command_id == status.command_id)
            {
                let pending = self.pending.remove(index).expect("known pending command");
                match status.status {
                    SquidExecutionStatus::CompletedWithoutErrors => {
                        self.events.push_back(DriverEvent::TokenCompleted {
                            token: pending.token,
                            value: pending.value,
                        });
                    }
                    other => {
                        self.events.push_back(DriverEvent::TokenFailed {
                            token: pending.token,
                            report: ErrorReport {
                                code: ErrorCode::Driver,
                                message: format!(
                                    "Squid command {} failed with {:?}",
                                    pending.command_id, other
                                ),
                            },
                        });
                        if let Some(device) = pending.device {
                            self.events
                                .push_back(DriverEvent::Event(Event::Fault(FaultEvent {
                                    device: Some(device),
                                    report: ErrorReport {
                                        code: ErrorCode::Driver,
                                        message: format!("Squid firmware reported {:?}", other),
                                    },
                                })));
                        }
                    }
                }
            }
        }
        self.events.drain(..).collect()
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
                resource: Some(ResourceId(NodeId(SERIAL_RESOURCE_NODE))),
                description: "squid timing arm summary".into(),
                payload: self.timing_summary(plan, "arm"),
            }],
        })
    }

    fn start_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let mut transactions = Vec::new();
        let sequence_value = self.apply_timing_sequence_step(&armed.plan, true)?;
        if !matches!(&sequence_value, Value::Map(map) if map.is_empty()) {
            transactions.push(PhysicalTransaction {
                resource: Some(ResourceId(NodeId(SERIAL_RESOURCE_NODE))),
                description: "squid timing start sequence".into(),
                payload: sequence_value,
            });
        }
        let trigger_commands = self.timing_trigger_commands(&armed.plan);
        let trigger_payloads = self.send_timing_commands(&trigger_commands)?;
        if !trigger_payloads.is_empty() {
            transactions.push(PhysicalTransaction {
                resource: Some(ResourceId(NodeId(SERIAL_RESOURCE_NODE))),
                description: "squid timing start trigger pulse".into(),
                payload: Value::List(trigger_payloads),
            });
        }
        transactions.push(PhysicalTransaction {
            resource: Some(ResourceId(NodeId(SERIAL_RESOURCE_NODE))),
            description: "squid timing start summary".into(),
            payload: self.timing_summary(&armed.plan, "start"),
        });
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Start(OperationId(0))],
            physical_transactions: transactions,
        })
    }

    fn stop_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let mut transactions = Vec::new();
        let sequence_value = self.apply_timing_sequence_step(&armed.plan, false)?;
        if !matches!(&sequence_value, Value::Map(map) if map.is_empty()) {
            transactions.push(PhysicalTransaction {
                resource: Some(ResourceId(NodeId(SERIAL_RESOURCE_NODE))),
                description: "squid timing stop sequence".into(),
                payload: sequence_value,
            });
        }
        transactions.push(PhysicalTransaction {
            resource: Some(ResourceId(NodeId(SERIAL_RESOURCE_NODE))),
            description: "squid timing stop summary".into(),
            payload: self.timing_summary(&armed.plan, "stop"),
        });
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions: transactions,
        })
    }
}

fn descriptors(driver: DriverId) -> Vec<DeviceDescriptor> {
    let mut devices = vec![
        DeviceDescriptor {
            id: DeviceId(NodeId(HUB_NODE)),
            driver,
            label: "squid-controller".into(),
            vendor: Some("Cephla".into()),
            model: Some("Squid controller".into()),
            serial: None,
            kinds: vec!["hub".into(), "serial.controller".into()],
            properties: vec![
                property(
                    "firmware_version",
                    "Firmware version",
                    ValueType::String,
                    None,
                    false,
                    true,
                ),
                property(
                    "watchdog_timeout",
                    "Watchdog timeout",
                    ValueType::TimeInterval,
                    Some("s"),
                    true,
                    false,
                ),
            ],
            metadata: BTreeMap::from([("baud_rate".into(), Value::I64(DEFAULT_BAUD_RATE as i64))]),
        },
        DeviceDescriptor {
            id: DeviceId(NodeId(XY_NODE)),
            driver,
            label: "squid-xy-stage".into(),
            vendor: Some("Cephla".into()),
            model: Some("Squid XY stage".into()),
            serial: None,
            kinds: vec!["stage.xy".into()],
            properties: vec![
                sequenceable_position_property("x", "X position", true, true),
                sequenceable_position_property("y", "Y position", true, true),
            ],
            metadata: BTreeMap::new(),
        },
        DeviceDescriptor {
            id: DeviceId(NodeId(Z_NODE)),
            driver,
            label: "squid-z-stage".into(),
            vendor: Some("Cephla".into()),
            model: Some("Squid Z stage".into()),
            serial: None,
            kinds: vec!["stage.z".into()],
            properties: vec![sequenceable_position_property(
                "z",
                "Z position",
                true,
                true,
            )],
            metadata: BTreeMap::new(),
        },
        DeviceDescriptor {
            id: DeviceId(NodeId(THETA_NODE)),
            driver,
            label: "squid-theta".into(),
            vendor: Some("Cephla".into()),
            model: Some("Squid theta axis".into()),
            serial: None,
            kinds: vec!["stage.theta".into()],
            properties: vec![property(
                "position_steps",
                "Position",
                ValueType::StepCount,
                Some("steps"),
                true,
                true,
            )],
            metadata: BTreeMap::new(),
        },
        filter_descriptor(driver, FILTER_W_NODE, "squid-filter-wheel-w"),
        filter_descriptor(driver, FILTER_W2_NODE, "squid-filter-wheel-w2"),
        led_matrix_descriptor(driver),
        DeviceDescriptor {
            id: DeviceId(NodeId(AUTOFOCUS_NODE)),
            driver,
            label: "squid-autofocus".into(),
            vendor: Some("Cephla".into()),
            model: Some("General autofocus provider backed by Squid firmware pin 15".into()),
            serial: None,
            kinds: vec!["autofocus".into()],
            properties: vec![
                sequenceable_property("enabled", "Enabled", ValueType::Bool, None, true, true),
                property("mode", "Mode", ValueType::String, None, false, true),
                property("status", "Status", ValueType::String, None, false, true),
                property(
                    "focus_score",
                    "Focus score",
                    ValueType::F64,
                    None,
                    false,
                    true,
                ),
                property("kind", "Kind", ValueType::String, None, false, false),
                sequenceable_property(
                    "laser_enabled",
                    "Autofocus laser",
                    ValueType::Bool,
                    None,
                    true,
                    true,
                ),
            ],
            metadata: BTreeMap::from([
                (
                    "firmware_pin".into(),
                    Value::I64(AUTOFOCUS_LASER_PIN as i64),
                ),
                ("role".into(), Value::String("autofocus".into())),
                (
                    "implementation".into(),
                    Value::String(
                        "Core autofocus provider using Squid SET_PIN_LEVEL pin 15 internally"
                            .into(),
                    ),
                ),
                (
                    "deprecated_property".into(),
                    Value::String("laser_enabled".into()),
                ),
            ]),
        },
    ];

    let wavelengths = [405.0, 488.0, 561.0, 638.0, 730.0];
    for (port, wavelength) in wavelengths.into_iter().enumerate() {
        devices.push(DeviceDescriptor {
            id: DeviceId(NodeId(ILLUMINATION_BASE_NODE + port as u64)),
            driver,
            label: format!("squid-illumination-d{}", port + 1),
            vendor: Some("Cephla".into()),
            model: Some("Squid illumination port".into()),
            serial: None,
            kinds: vec!["light.source".into(), "illumination.port".into()],
            properties: vec![
                sequenceable_property("enabled", "Enabled", ValueType::Bool, None, true, true),
                sequenceable_property(
                    "intensity",
                    "Intensity",
                    ValueType::Ratio,
                    Some("percent"),
                    true,
                    true,
                ),
                property(
                    "wavelength",
                    "Wavelength",
                    ValueType::Wavelength,
                    None,
                    false,
                    false,
                ),
            ],
            metadata: BTreeMap::from([
                ("port_index".into(), Value::I64(port as i64)),
                (
                    "wavelength".into(),
                    Value::Wavelength(Wavelength::from_nanometers(wavelength)),
                ),
            ]),
        });
    }

    for channel in 0..4 {
        devices.push(DeviceDescriptor {
            id: DeviceId(NodeId(TRIGGER_BASE_NODE + channel)),
            driver,
            label: format!("squid-trigger-{}", channel + 1),
            vendor: Some("Cephla".into()),
            model: Some("Squid camera trigger output".into()),
            serial: None,
            kinds: vec!["trigger.source".into(), "camera.trigger".into()],
            properties: vec![property(
                "mode",
                "Mode",
                ValueType::String,
                None,
                true,
                true,
            )],
            metadata: BTreeMap::from([("channel".into(), Value::I64(channel as i64))]),
        });
    }

    for channel in 0..ONBOARD_DAC_COUNT {
        devices.push(DeviceDescriptor {
            id: DeviceId(NodeId(ONBOARD_DAC_BASE_NODE + channel as u64)),
            driver,
            label: format!("squid-onboard-dac-{}", channel + 1),
            vendor: Some("Cephla".into()),
            model: Some("Squid onboard DAC channel".into()),
            serial: None,
            kinds: vec!["analog.output".into(), "diagnostic.raw".into()],
            properties: vec![raw_counts_property()],
            metadata: BTreeMap::from([
                ("channel".into(), Value::I64(channel as i64)),
                (
                    "wire_command".into(),
                    Value::String("ANALOG_WRITE_ONBOARD_DAC".into()),
                ),
            ]),
        });
    }

    devices
}

fn led_matrix_descriptor(driver: DriverId) -> DeviceDescriptor {
    DeviceDescriptor {
        id: DeviceId(NodeId(LED_MATRIX_NODE)),
        driver,
        label: "squid-led-matrix".into(),
        vendor: Some("Cephla".into()),
        model: Some("Squid LED matrix".into()),
        serial: None,
        kinds: vec!["light.source".into(), "illumination.matrix".into()],
        properties: vec![
            led_matrix_pattern_property(),
            sequenceable_property("red", "Red", ValueType::Ratio, Some("percent"), true, true),
            sequenceable_property(
                "green",
                "Green",
                ValueType::Ratio,
                Some("percent"),
                true,
                true,
            ),
            sequenceable_property(
                "blue",
                "Blue",
                ValueType::Ratio,
                Some("percent"),
                true,
                true,
            ),
        ],
        metadata: BTreeMap::from([(
            "wire_command".into(),
            Value::String("SET_ILLUMINATION_LED_MATRIX".into()),
        )]),
    }
}

fn filter_descriptor(driver: DriverId, node: u64, label: &str) -> DeviceDescriptor {
    DeviceDescriptor {
        id: DeviceId(NodeId(node)),
        driver,
        label: label.into(),
        vendor: Some("Cephla".into()),
        model: Some("Squid filter wheel".into()),
        serial: None,
        kinds: vec!["filter.wheel".into()],
        properties: vec![property(
            "position",
            "Position",
            ValueType::StepCount,
            Some("steps"),
            true,
            true,
        )],
        metadata: BTreeMap::new(),
    }
}

fn capability(
    id: u64,
    device: DeviceId,
    kind: CapabilityKind,
    request: ValueType,
    response: ValueType,
) -> CapabilityDescriptor {
    CapabilityDescriptor {
        id: CapabilityId(id),
        device,
        name: kind.name().to_string(),
        kind,
        request,
        response,
    }
}

fn property(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    unit: Option<&str>,
    writable: bool,
    volatile: bool,
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
        volatile,
        sequenceable: false,
        hardware_address: None,
    }
}

fn position_property(
    key: &str,
    display_name: &str,
    writable: bool,
    volatile: bool,
) -> PropertySchema {
    property(
        key,
        display_name,
        ValueType::Position,
        Some("um"),
        writable,
        volatile,
    )
}

fn sequenceable_property(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    unit: Option<&str>,
    writable: bool,
    volatile: bool,
) -> PropertySchema {
    let mut schema = property(key, display_name, value_type, unit, writable, volatile);
    schema.sequenceable = writable;
    schema
}

fn sequenceable_position_property(
    key: &str,
    display_name: &str,
    writable: bool,
    volatile: bool,
) -> PropertySchema {
    let mut schema = position_property(key, display_name, writable, volatile);
    schema.sequenceable = writable;
    schema
}

fn led_matrix_pattern_property() -> PropertySchema {
    let mut schema = property("pattern", "Pattern", ValueType::String, None, true, true);
    schema.sequenceable = true;
    schema.enum_values = [
        LedMatrixPattern::FullArray,
        LedMatrixPattern::LeftHalf,
        LedMatrixPattern::RightHalf,
        LedMatrixPattern::LeftBlueRightRed,
        LedMatrixPattern::LowNa,
        LedMatrixPattern::LeftDot,
        LedMatrixPattern::RightDot,
        LedMatrixPattern::TopHalf,
        LedMatrixPattern::BottomHalf,
        LedMatrixPattern::ExternalFet,
    ]
    .into_iter()
    .map(|pattern| EnumValue {
        value: Value::String(pattern.label().into()),
        label: pattern.label().into(),
    })
    .collect();
    schema
}

fn raw_counts_property() -> PropertySchema {
    let mut schema = property(
        "raw_counts",
        "Raw counts",
        ValueType::I64,
        Some("counts"),
        true,
        true,
    );
    schema.range = Some(Range {
        min: Value::I64(0),
        max: Value::I64(u16::MAX as i64),
    });
    schema
}

fn simple_payload(code: SquidCommandCode, payload: [u8; 5]) -> SquidCommandFrame {
    SquidCommandFrame::new(0, code, payload)
}

fn move_to(code: SquidCommandCode, steps: i32) -> SquidCommandFrame {
    let bytes = steps.to_be_bytes();
    simple_payload(code, [bytes[0], bytes[1], bytes[2], bytes[3], 0])
}

fn home_command(kind: LogicalDevice) -> Result<SquidCommandFrame> {
    match kind {
        LogicalDevice::Xy => Ok(simple_payload(
            SquidCommandCode::HomeOrZero,
            [
                SquidAxis::Xy as u8,
                SquidHomeMode::Negative as u8,
                SquidHomeMode::Negative as u8,
                0,
                0,
            ],
        )),
        LogicalDevice::Z => Ok(simple_payload(
            SquidCommandCode::HomeOrZero,
            [SquidAxis::Z as u8, SquidHomeMode::Positive as u8, 0, 0, 0],
        )),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            "Squid home requires an XY or Z stage device",
        )),
    }
}

fn generic_command_frame(
    kind: LogicalDevice,
    request: &GenericCommandRequest,
) -> Result<SquidCommandFrame> {
    if !request.params.is_empty() {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            "Squid GenericCommand does not take parameters",
        ));
    }
    match (kind, request.command.as_str()) {
        (LogicalDevice::Hub, "disable_all_ports") => {
            Ok(simple_payload(SquidCommandCode::TurnOffAllPorts, [0; 5]))
        }
        (LogicalDevice::Hub, "heartbeat") => {
            Ok(simple_payload(SquidCommandCode::Heartbeat, [0; 5]))
        }
        (LogicalDevice::Hub, other) => Err(Error::new(
            ErrorCode::Unsupported,
            format!(
                "Squid hub GenericCommand supports disable_all_ports and heartbeat; got {other}"
            ),
        )),
        (LogicalDevice::Theta, other) => Err(Error::new(
            ErrorCode::Unsupported,
            format!("Squid theta GenericCommand has no public aliases; got {other}"),
        )),
        (LogicalDevice::FilterW | LogicalDevice::FilterW2, other) => Err(Error::new(
            ErrorCode::Unsupported,
            format!("Squid filter GenericCommand has no public aliases; got {other}"),
        )),
        _ => Err(Error::new(
            ErrorCode::Unsupported,
            "Squid GenericCommand is available on the hub, theta, and filter wheels",
        )),
    }
}

fn set_port_intensity(port: u8, intensity: u16) -> SquidCommandFrame {
    let bytes = intensity.to_be_bytes();
    simple_payload(
        SquidCommandCode::SetPortIntensity,
        [port, bytes[0], bytes[1], 0, 0],
    )
}

fn set_port_illumination(port: u8, intensity: u16, enabled: bool) -> SquidCommandFrame {
    let bytes = intensity.to_be_bytes();
    simple_payload(
        SquidCommandCode::SetPortIllumination,
        [port, bytes[0], bytes[1], u8::from(enabled), 0],
    )
}

fn set_led_matrix(pattern: LedMatrixPattern, green: u8, red: u8, blue: u8) -> SquidCommandFrame {
    simple_payload(
        SquidCommandCode::SetIlluminationLedMatrix,
        [pattern.code(), green, red, blue, 0],
    )
}

fn onboard_dac_write(channel: u8, raw_counts: u16) -> SquidCommandFrame {
    let bytes = raw_counts.to_be_bytes();
    simple_payload(
        SquidCommandCode::AnalogWriteOnboardDac,
        [channel, bytes[0], bytes[1], 0, 0],
    )
}

fn port_enabled(port: u8, enabled: bool) -> SquidCommandFrame {
    simple_payload(
        if enabled {
            SquidCommandCode::TurnOnPort
        } else {
            SquidCommandCode::TurnOffPort
        },
        [port, 0, 0, 0, 0],
    )
}

fn watchdog_timeout(timeout_ms: u32) -> SquidCommandFrame {
    let bytes = timeout_ms.to_be_bytes();
    simple_payload(
        SquidCommandCode::SetWatchdogTimeout,
        [bytes[0], bytes[1], bytes[2], bytes[3], 0],
    )
}

fn hardware_trigger(channel: u8, control_illumination: bool, on_time_us: u32) -> SquidCommandFrame {
    let bytes = on_time_us.to_be_bytes();
    let flags = (u8::from(control_illumination) << 7) | (channel & 0x0f);
    simple_payload(
        SquidCommandCode::SendHardwareTrigger,
        [flags, bytes[0], bytes[1], bytes[2], bytes[3]],
    )
}

fn pin_level(pin: u8, high: bool) -> SquidCommandFrame {
    simple_payload(
        SquidCommandCode::SetPinLevel,
        [pin, u8::from(high), 0, 0, 0],
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

fn u32_prop(device: &DeviceConfig, key: &str) -> Option<u32> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => u32::try_from(*value).ok(),
        _ => None,
    }
}

fn u64_prop(device: &DeviceConfig, key: &str) -> Option<u64> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => u64::try_from(*value).ok(),
        _ => None,
    }
}

fn position(value_um: f64) -> Value {
    Value::Position(Position::from_micrometers(value_um))
}

fn step_count(steps: i64) -> Value {
    Value::StepCount(StepCount::new(steps))
}

fn time_interval(seconds: f64) -> Value {
    Value::TimeInterval(TimeInterval::from_seconds(seconds))
}

fn time_seconds(value: &Value, key: &str) -> Result<f64> {
    match value {
        Value::TimeInterval(interval) => Ok(interval.seconds()),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("{key} expects typed time interval"),
        )),
    }
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

fn position_um_to_steps(value: &Value) -> Result<i32> {
    position_um_f64_to_steps(position_um(value)?)
}

fn position_um_f64_to_steps(value: f64) -> Result<i32> {
    if value < i32::MIN as f64 || value > i32::MAX as f64 {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "position is out of i32 range",
        ));
    }
    Ok(value.round() as i32)
}

fn step_count_i32(value: &Value) -> Result<i32> {
    let steps = match value {
        Value::StepCount(value) => value.steps(),
        Value::I64(value) => *value,
        _ => {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "expected typed step count",
            ));
        }
    };
    steps
        .try_into()
        .map_err(|_| Error::new(ErrorCode::InvalidProperty, "step count is out of i32 range"))
}

fn percent_to_u16(value: &Value) -> Result<u16> {
    Ok(percent_float_to_u16(percent_value(value, "percent")?))
}

fn percent_float_to_u16(percent: f64) -> u16 {
    ((percent.clamp(0.0, 100.0) / 100.0) * 65535.0).round() as u16
}

fn percent_float_to_u8(percent: f64) -> u8 {
    ((percent.clamp(0.0, 100.0) / 100.0) * u8::MAX as f64).round() as u8
}

fn percent_value(value: &Value, key: &str) -> Result<f64> {
    match value {
        Value::Ratio(value) => Ok(value.percent()),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("{key} expects Ratio percent"),
        )),
    }
}

fn led_matrix_pattern_value(value: &Value) -> Result<LedMatrixPattern> {
    let Value::String(value) = value else {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "pattern expects string",
        ));
    };
    LedMatrixPattern::from_label(value).ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidProperty,
            "unknown Squid LED matrix pattern",
        )
    })
}

fn raw_u16_value(value: &Value, key: &str) -> Result<u16> {
    let Value::I64(value) = value else {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("{key} expects I64"),
        ));
    };
    u16::try_from(*value).map_err(|_| {
        Error::new(
            ErrorCode::InvalidProperty,
            format!("{key} is out of u16 range"),
        )
    })
}

fn is_intensity_key(key: &str) -> bool {
    matches!(key, "intensity" | "intensity_percent")
}

fn bool_value(value: &Value, key: &str) -> Result<bool> {
    match value {
        Value::Bool(value) => Ok(*value),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("{key} expects bool"),
        )),
    }
}

fn trigger_mode_value(value: &Value) -> Result<SquidTriggerMode> {
    match value {
        Value::String(value) if value == "edge" => Ok(SquidTriggerMode::Edge),
        Value::String(value) if value == "level" => Ok(SquidTriggerMode::Level),
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            "trigger mode expects \"edge\" or \"level\"",
        )),
    }
}

fn i32_payload(bytes: &[u8]) -> i32 {
    i32::from_be_bytes(bytes.try_into().expect("i32 payload length"))
}

fn u16_payload(bytes: &[u8]) -> u16 {
    u16::from_be_bytes(bytes.try_into().expect("u16 payload length"))
}

fn autofocus_mode_enables_laser(mode: AutofocusMode) -> bool {
    matches!(
        mode,
        AutofocusMode::SingleShot | AutofocusMode::Continuous | AutofocusMode::Hold
    )
}

fn autofocus_mode_name(mode: AutofocusMode) -> String {
    match mode {
        AutofocusMode::SingleShot => "single_shot",
        AutofocusMode::Continuous => "continuous",
        AutofocusMode::Hold => "hold",
        AutofocusMode::Stop => "stop",
    }
    .into()
}

fn publish_property_events(events: &mut VecDeque<DriverEvent>, command: &Command) {
    match command {
        Command::WriteProperty { device, key, value } => {
            events.push_back(DriverEvent::Event(Event::PropertyChanged(
                PropertyChanged {
                    device: *device,
                    key: key.clone(),
                    value: value.clone(),
                },
            )));
        }
        Command::ApplyStateSet(set) => {
            for write in &set.writes {
                events.push_back(DriverEvent::Event(Event::PropertyChanged(
                    PropertyChanged {
                        device: write.device,
                        key: write.property.clone(),
                        value: write.value.clone(),
                    },
                )));
            }
        }
        Command::Invoke {
            device,
            capability: CapabilityId(440),
            request: CapabilityRequest::Autofocus(request),
        } => {
            events.push_back(DriverEvent::Event(Event::PropertyChanged(
                PropertyChanged {
                    device: *device,
                    key: "mode".into(),
                    value: Value::String(autofocus_mode_name(request.mode)),
                },
            )));
            events.push_back(DriverEvent::Event(Event::PropertyChanged(
                PropertyChanged {
                    device: *device,
                    key: "enabled".into(),
                    value: Value::Bool(autofocus_mode_enables_laser(request.mode)),
                },
            )));
        }
        _ => {}
    }
}
