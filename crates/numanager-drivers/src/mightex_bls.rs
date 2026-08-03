use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::hid::{HidDeviceIdentity, HidFeatureIo};
#[cfg(feature = "os-hid")]
use numanager_core::hid::{OsHidFeatureConfig, OsHidFeatureDevice};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::*;
use numanager_core::{Error, ErrorCode, Result};
use std::collections::{BTreeMap, VecDeque};

const MODULE_CODES: &[&str] = &[
    "AA", "AV", "SA", "SV", "MA", "CA", "HA", "HV", "FA", "FV", "XA", "XV", "QA",
];
const CURRENT_RAW_BRINGUP_MAX: u32 = 100;
const HUB_SUPPORT_LEVEL: &str = "diagnostic";
const CHANNEL_SUPPORT_LEVEL: &str = "output";
const RESOURCE_SUPPORT_LEVEL: &str = "discovery_only";
const MODE_DISABLED: u8 = 0;
const MODE_NORMAL: u8 = 1;
const MODE_STROBE: u8 = 2;
const MODE_TRIGGER: u8 = 3;
const BLS_TRIGGER_CURRENT_MAX: i64 = 1000;
const BLS_TRIGGER_TIME_MAX: i64 = 9_999_999;
const BLS_TRIGGER_REPEAT_MAX: i64 = 1_000_000;
const SLC_NORMAL_CURRENT_RAW_MAX: i64 = 1000;
const SLC_PROFILE_CURRENT_RAW_MAX: i64 = 1000;
const SLC_PROFILE_REPEAT_MAX: i64 = 1_000_000;
const SLC_PROFILE_FREQUENCY_MAX_HZ: f64 = 25_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MightexSiriusFamily {
    Bls,
    Slc,
}

impl MightexSiriusFamily {
    pub fn product_string(self) -> &'static str {
        match self {
            MightexSiriusFamily::Bls => "Sirius BLS",
            MightexSiriusFamily::Slc => "Sirius SLC",
        }
    }

    pub fn matches_product(self, product: &str) -> bool {
        product.contains(self.product_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MightexSiriusCandidate {
    pub family: MightexSiriusFamily,
    pub vendor_id: u16,
    pub product_id: u16,
    pub product_string: String,
    pub serial_number: Option<String>,
    pub channel_count: Option<u8>,
    pub module_type: Option<String>,
    pub discovery_label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlsTriggerProgram {
    Pulse,
    Follow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RawProfileStep {
    current_raw: i64,
    time_raw: i64,
}

impl BlsTriggerProgram {
    fn as_str(self) -> &'static str {
        match self {
            BlsTriggerProgram::Pulse => "pulse",
            BlsTriggerProgram::Follow => "follow",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "pulse" => Ok(BlsTriggerProgram::Pulse),
            "follow" => Ok(BlsTriggerProgram::Follow),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unsupported Mightex BLS trigger_program {value}"),
            )),
        }
    }
}

pub(crate) mod protocol {
    use super::*;

    pub const REPORT_ID: u8 = 0;
    pub const ASCII_REPORT_TYPE: u8 = 1;
    pub const FEATURE_REPORT_LEN: usize = 19;
    pub const PAYLOAD_OFFSET: usize = 3;
    pub const PAYLOAD_LEN: usize = FEATURE_REPORT_LEN - PAYLOAD_OFFSET;
    pub const COMMAND_TERMINATOR: &[u8] = b"\n\r";
    pub const COMMAND_TERMINATOR_LABEL: &str = "\\n\\r";

    pub fn classify_identity(identity: &HidDeviceIdentity) -> Option<MightexSiriusCandidate> {
        let product = identity.product_string.as_ref()?;
        let family = if MightexSiriusFamily::Bls.matches_product(product) {
            MightexSiriusFamily::Bls
        } else if MightexSiriusFamily::Slc.matches_product(product) {
            MightexSiriusFamily::Slc
        } else {
            return None;
        };
        Some(MightexSiriusCandidate {
            family,
            vendor_id: identity.vendor_id,
            product_id: identity.product_id,
            product_string: product.clone(),
            serial_number: identity.serial_number.clone(),
            channel_count: None,
            module_type: None,
            discovery_label: None,
        })
    }

    pub fn filter_sirius_devices<'a>(
        identities: impl IntoIterator<Item = &'a HidDeviceIdentity>,
    ) -> Vec<MightexSiriusCandidate> {
        identities
            .into_iter()
            .filter_map(classify_identity)
            .collect()
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum SiriusCommand {
        Mode {
            channel: u8,
            mode: u8,
        },
        Current {
            channel: u8,
            value: u32,
        },
        Normal {
            channel: u8,
            current_max: i64,
            current_set: i64,
        },
        QueryImax {
            channel: u8,
        },
        QueryOdRules {
            channel: u8,
        },
        QueryMode {
            channel: u8,
        },
        QueryCurrent {
            channel: u8,
        },
        QueryStrobe {
            channel: u8,
        },
        QueryStrobeProfile {
            channel: u8,
        },
        QuerySlcTrigger {
            channel: u8,
        },
        QuerySlcTriggerProfile {
            channel: u8,
        },
        SlcStrobe {
            channel: u8,
            current_max: i64,
            repeat_count: i64,
        },
        SlcStrobeStep {
            channel: u8,
            line: u8,
            current: i64,
            time: i64,
        },
        SlcTrigger {
            channel: u8,
            current_max: i64,
            polarity: i64,
        },
        SlcTriggerStep {
            channel: u8,
            line: u8,
            current: i64,
            time: i64,
        },
        ReadBinaryVoltage {
            channel: u8,
        },
        EchoOff,
        SoftStart {
            channel: u8,
        },
        TriggerProfile {
            channel: u8,
            polarity: u8,
            current_max: i64,
            repeat_count: i64,
        },
        TriggerPulseStep {
            channel: u8,
            step: u8,
            current: i64,
            time: i64,
        },
        DisableAll,
    }

    impl SiriusCommand {
        pub fn ascii(&self) -> String {
            match self {
                SiriusCommand::Mode { channel, mode } => format!("MODE {channel} {mode}"),
                SiriusCommand::Current { channel, value } => format!("CURRENT {channel} {value}"),
                SiriusCommand::Normal {
                    channel,
                    current_max,
                    current_set,
                } => format!("NORMAL {channel} {current_max} {current_set}"),
                SiriusCommand::QueryImax { channel } => format!("?GetImax {channel}"),
                SiriusCommand::QueryOdRules { channel } => format!("?GetODRules {channel}"),
                SiriusCommand::QueryMode { channel } => format!("?MODE {channel}"),
                SiriusCommand::QueryCurrent { channel } => format!("?CURRENT {channel}"),
                SiriusCommand::QueryStrobe { channel } => format!("?STROBE {channel}"),
                SiriusCommand::QueryStrobeProfile { channel } => format!("?STRP {channel}"),
                SiriusCommand::QuerySlcTrigger { channel } => format!("?TRIGGER {channel}"),
                SiriusCommand::QuerySlcTriggerProfile { channel } => format!("?TRIGP {channel}"),
                SiriusCommand::SlcStrobe {
                    channel,
                    current_max,
                    repeat_count,
                } => format!("STROBE {channel} {current_max} {repeat_count}"),
                SiriusCommand::SlcStrobeStep {
                    channel,
                    line,
                    current,
                    time,
                } => format!("STRP {channel} {line} {current} {time}"),
                SiriusCommand::SlcTrigger {
                    channel,
                    current_max,
                    polarity,
                } => format!("TRIGGER {channel} {current_max} {polarity}"),
                SiriusCommand::SlcTriggerStep {
                    channel,
                    line,
                    current,
                    time,
                } => format!("TRIGP {channel} {line} {current} {time}"),
                SiriusCommand::ReadBinaryVoltage { channel } => format!("ReadBinaryV {channel}"),
                SiriusCommand::EchoOff => "ECHOOFF".into(),
                SiriusCommand::SoftStart { channel } => format!("SoftStart {channel}"),
                SiriusCommand::TriggerProfile {
                    channel,
                    polarity,
                    current_max,
                    repeat_count,
                } => format!("Trigger {channel} {current_max} {polarity} {repeat_count}"),
                SiriusCommand::TriggerPulseStep {
                    channel,
                    step,
                    current,
                    time,
                } => format!("TrigP {channel} {step} {current} {time}"),
                SiriusCommand::DisableAll => "MODE 88 0".into(),
            }
        }

        pub fn expects_reply(&self) -> bool {
            !matches!(self, SiriusCommand::ReadBinaryVoltage { .. })
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct FeatureReport {
        pub bytes: [u8; FEATURE_REPORT_LEN],
    }

    impl FeatureReport {
        pub fn as_bytes(&self) -> &[u8] {
            &self.bytes
        }
    }

    pub fn encode_ascii_command(command: &str) -> Result<Vec<FeatureReport>> {
        let bytes = command.as_bytes();
        if bytes.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Mightex Sirius command cannot be empty",
            ));
        }
        let mut wire_bytes = Vec::with_capacity(bytes.len() + COMMAND_TERMINATOR.len());
        wire_bytes.extend_from_slice(bytes);
        wire_bytes.extend_from_slice(COMMAND_TERMINATOR);
        Ok(wire_bytes
            .chunks(PAYLOAD_LEN)
            .map(|chunk| {
                let mut report = [0; FEATURE_REPORT_LEN];
                report[0] = REPORT_ID;
                report[1] = ASCII_REPORT_TYPE;
                report[2] = chunk.len() as u8;
                report[PAYLOAD_OFFSET..PAYLOAD_OFFSET + chunk.len()].copy_from_slice(chunk);
                FeatureReport { bytes: report }
            })
            .collect())
    }

    pub fn encode_command(command: &SiriusCommand) -> Result<Vec<FeatureReport>> {
        encode_ascii_command(&command.ascii())
    }

    #[derive(Debug, Clone, Default)]
    pub struct ReplyAssembler {
        bytes: Vec<u8>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SiriusReply {
        pub text: String,
        pub report_count: usize,
    }

    impl ReplyAssembler {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn push_report(&mut self, report: &[u8]) -> Result<bool> {
            if report.len() < PAYLOAD_OFFSET {
                return Err(Error::new(
                    ErrorCode::Transport,
                    "Mightex Sirius feature reply is too short",
                ));
            }
            if report[0] != REPORT_ID {
                return Err(Error::new(
                    ErrorCode::Transport,
                    "Mightex Sirius feature reply used unexpected report id",
                ));
            }
            let len = report[2] as usize;
            if len == 0 {
                return Ok(true);
            }
            if len > PAYLOAD_LEN || report.len() < PAYLOAD_OFFSET + len {
                return Err(Error::new(
                    ErrorCode::Transport,
                    "Mightex Sirius feature reply has invalid payload length",
                ));
            }
            self.bytes
                .extend_from_slice(&report[PAYLOAD_OFFSET..PAYLOAD_OFFSET + len]);
            Ok(false)
        }

        pub fn finish(self) -> String {
            String::from_utf8_lossy(&self.bytes).to_string()
        }
    }

    pub fn send_command(io: &mut dyn HidFeatureIo, command: &SiriusCommand) -> Result<()> {
        for report in encode_command(command)? {
            io.set_feature(report.as_bytes())?;
        }
        Ok(())
    }

    pub fn read_reply_limited_with_count(
        io: &mut dyn HidFeatureIo,
        max_reports: usize,
    ) -> Result<SiriusReply> {
        let mut assembler = ReplyAssembler::new();
        for report_count in 1..=max_reports {
            let report = io.get_feature(REPORT_ID, FEATURE_REPORT_LEN)?;
            if assembler.push_report(&report)? {
                return Ok(SiriusReply {
                    text: assembler.finish(),
                    report_count,
                });
            }
        }
        Err(Error::new(
            ErrorCode::Timeout,
            "Mightex Sirius reply did not terminate",
        ))
    }

    pub fn read_binary_echo_u16(
        io: &mut dyn HidFeatureIo,
        max_reports: usize,
    ) -> Result<(u16, usize)> {
        let mut head_count = 0usize;
        let mut payload = Vec::with_capacity(2);
        for report_count in 1..=max_reports {
            let report = io.get_feature(REPORT_ID, FEATURE_REPORT_LEN)?;
            if report.len() < PAYLOAD_OFFSET {
                return Err(Error::new(
                    ErrorCode::Transport,
                    "Mightex Sirius binary reply is too short",
                ));
            }
            let len = report[2] as usize;
            if len > PAYLOAD_LEN || report.len() < PAYLOAD_OFFSET + len {
                return Err(Error::new(
                    ErrorCode::Transport,
                    "Mightex Sirius binary reply has invalid payload length",
                ));
            }
            for byte in &report[PAYLOAD_OFFSET..PAYLOAD_OFFSET + len] {
                if *byte == 0xEE && payload.is_empty() {
                    head_count += 1;
                    continue;
                }
                if head_count == 2 {
                    payload.push(*byte);
                    if payload.len() == 2 {
                        return Ok((
                            u16::from(payload[0]) << 8 | u16::from(payload[1]),
                            report_count,
                        ));
                    }
                } else {
                    head_count = 0;
                    payload.clear();
                }
            }
        }
        Err(Error::new(
            ErrorCode::Timeout,
            "Mightex Sirius binary echo did not contain a u16 payload",
        ))
    }
}

pub struct MightexBlsDiscovery {
    next_id: DriverId,
    candidates: Option<Vec<MightexSiriusCandidate>>,
}

impl MightexBlsDiscovery {
    pub fn from_identities(next_id: DriverId, identities: Vec<HidDeviceIdentity>) -> Self {
        Self {
            next_id,
            candidates: Some(protocol::filter_sirius_devices(&identities)),
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let candidates = config
            .devices
            .iter()
            .filter(|device| matches!(device.driver.as_str(), "mightex_bls" | "mightex_sirius"))
            .map(candidate_from_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            next_id,
            candidates: Some(candidates),
        })
    }

    #[cfg(feature = "os-hid")]
    pub fn os_hid(next_id: DriverId) -> Self {
        Self {
            next_id,
            candidates: None,
        }
    }
}

fn candidate_from_config(device: &DeviceConfig) -> Result<MightexSiriusCandidate> {
    let product_string = string_prop(device, "product_string").or_else(|| {
        string_prop(device, "family").map(|family| normalize_family_product_string(&family))
    });
    let product_string = product_string.ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidProperty,
            "Mightex Sirius config requires product_string or family",
        )
    })?;
    let identity = HidDeviceIdentity {
        vendor_id: u16_prop(device, "vendor_id")?,
        product_id: u16_prop(device, "product_id")?,
        product_string: Some(product_string),
        serial_number: string_prop(device, "serial_number"),
    };
    let mut candidate = protocol::classify_identity(&identity).ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidProperty,
            "Mightex Sirius config product_string/family must identify Sirius BLS or Sirius SLC",
        )
    })?;
    candidate.channel_count = optional_channel_count_prop(device)?;
    candidate.module_type = optional_module_type_prop(device)?;
    candidate.discovery_label = (!device.label.trim().is_empty()).then(|| device.label.clone());
    Ok(candidate)
}

fn normalize_family_product_string(family: &str) -> String {
    match family.trim().to_ascii_lowercase().as_str() {
        "bls" | "sirius bls" | "mightex bls" | "mightex sirius bls" => "Sirius BLS".into(),
        "slc" | "sirius slc" | "mightex slc" | "mightex sirius slc" => "Sirius SLC".into(),
        _ => family.to_string(),
    }
}

impl DriverDiscovery for MightexBlsDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        let sirius_candidates = match &self.candidates {
            Some(candidates) => candidates.clone(),
            None => protocol::filter_sirius_devices(&os_hid_identities()?),
        };
        let mut candidates = Vec::new();
        for candidate in sirius_candidates {
            let id = DriverId(self.next_id.0 + candidates.len() as u64);
            let label = candidate.discovery_label.clone().unwrap_or_else(|| {
                format!(
                    "Mightex {} HID controller {}",
                    family_label(candidate.family),
                    candidate.product_string
                )
            });
            candidates.push(DriverCandidate::from_driver(
                label,
                Box::new(MightexBlsDriver::discovered(id, candidate)),
            ));
        }
        Ok(candidates)
    }
}

#[cfg(feature = "os-hid")]
fn os_hid_identities() -> Result<Vec<HidDeviceIdentity>> {
    numanager_core::hid::enumerate_hid_devices()
}

#[cfg(not(feature = "os-hid"))]
fn os_hid_identities() -> Result<Vec<HidDeviceIdentity>> {
    Ok(Vec::new())
}

pub struct MightexBlsDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    channels: Vec<DeviceId>,
    candidate: MightexSiriusCandidate,
    channel_count: u8,
    module_type: Option<String>,
    current_raw: Vec<u32>,
    intensity_percent: Vec<f64>,
    mode_code: Vec<u8>,
    enabled: Vec<bool>,
    soft_start: Vec<bool>,
    trigger_program: Vec<BlsTriggerProgram>,
    trigger_repeat_count: Vec<i64>,
    trigger_pulse_current_raw: Vec<[i64; 3]>,
    trigger_pulse_time_raw: Vec<[i64; 3]>,
    trigger_follow_on_current_raw: Vec<i64>,
    trigger_follow_off_current_raw: Vec<i64>,
    normal_current_max_raw: Vec<i64>,
    normal_current_set_raw: Vec<i64>,
    strobe_current_max_raw: Vec<i64>,
    strobe_repeat_count_raw: Vec<i64>,
    trigger_current_max_raw: Vec<i64>,
    trigger_polarity_raw: Vec<i64>,
    profile_frequency: Vec<Frequency>,
    profile_duty_cycle: Vec<Ratio>,
    profile_current_1_raw: Vec<i64>,
    profile_current_2_raw: Vec<i64>,
    overdrive_current_limit: Vec<Option<Ratio>>,
    overdrive_duty_cycle_limit: Vec<Option<Ratio>>,
    overdrive_pulse_width_limit: Vec<Option<TimeInterval>>,
    mode_code_readback: Vec<Option<u8>>,
    current_max_raw_readback: Vec<Option<i64>>,
    current_raw_readback: Vec<Option<i64>>,
    strobe_current_max_raw_readback: Vec<Option<i64>>,
    strobe_repeat_count_raw_readback: Vec<Option<i64>>,
    strobe_profile_raw_readback: Vec<Option<Vec<RawProfileStep>>>,
    trigger_current_max_raw_readback: Vec<Option<i64>>,
    trigger_polarity_raw_readback: Vec<Option<i64>>,
    trigger_profile_raw_readback: Vec<Option<Vec<RawProfileStep>>>,
    load_voltage_raw: Vec<Option<i64>>,
    io: Option<Box<dyn HidFeatureIo>>,
    command_count: u64,
    last_command: Option<String>,
    last_reply: Option<String>,
    last_reply_report_count: Option<usize>,
    last_reply_kind: Option<String>,
    last_outcome: Option<String>,
    last_error: Option<String>,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
}

impl MightexBlsDriver {
    pub fn discovered(id: DriverId, candidate: MightexSiriusCandidate) -> Self {
        let channel_count = candidate
            .channel_count
            .unwrap_or_else(|| parse_channel_count(&candidate.product_string));
        let module_type = candidate
            .module_type
            .clone()
            .or_else(|| parse_module_type(&candidate.product_string));
        let base = id.0 * 1000 + 900;
        Self {
            id,
            resource: ResourceId(NodeId(base)),
            hub: DeviceId(NodeId(base + 1)),
            channels: (0..channel_count)
                .map(|index| DeviceId(NodeId(base + 10 + index as u64)))
                .collect(),
            candidate,
            channel_count,
            module_type,
            current_raw: vec![0; channel_count as usize],
            intensity_percent: vec![0.0; channel_count as usize],
            mode_code: vec![0; channel_count as usize],
            enabled: vec![false; channel_count as usize],
            soft_start: vec![false; channel_count as usize],
            trigger_program: vec![BlsTriggerProgram::Pulse; channel_count as usize],
            trigger_repeat_count: vec![1; channel_count as usize],
            trigger_pulse_current_raw: vec![[0, 50, 0]; channel_count as usize],
            trigger_pulse_time_raw: vec![[500_000, 500_000, 500_000]; channel_count as usize],
            trigger_follow_on_current_raw: vec![50; channel_count as usize],
            trigger_follow_off_current_raw: vec![0; channel_count as usize],
            normal_current_max_raw: vec![20; channel_count as usize],
            normal_current_set_raw: vec![10; channel_count as usize],
            strobe_current_max_raw: vec![20; channel_count as usize],
            strobe_repeat_count_raw: vec![1; channel_count as usize],
            trigger_current_max_raw: vec![20; channel_count as usize],
            trigger_polarity_raw: vec![1; channel_count as usize],
            profile_frequency: vec![Frequency::from_hertz(1.0); channel_count as usize],
            profile_duty_cycle: vec![Ratio::from_percent(50.0); channel_count as usize],
            profile_current_1_raw: vec![0; channel_count as usize],
            profile_current_2_raw: vec![10; channel_count as usize],
            overdrive_current_limit: vec![None; channel_count as usize],
            overdrive_duty_cycle_limit: vec![None; channel_count as usize],
            overdrive_pulse_width_limit: vec![None; channel_count as usize],
            mode_code_readback: vec![None; channel_count as usize],
            current_max_raw_readback: vec![None; channel_count as usize],
            current_raw_readback: vec![None; channel_count as usize],
            strobe_current_max_raw_readback: vec![None; channel_count as usize],
            strobe_repeat_count_raw_readback: vec![None; channel_count as usize],
            strobe_profile_raw_readback: vec![None; channel_count as usize],
            trigger_current_max_raw_readback: vec![None; channel_count as usize],
            trigger_polarity_raw_readback: vec![None; channel_count as usize],
            trigger_profile_raw_readback: vec![None; channel_count as usize],
            load_voltage_raw: vec![None; channel_count as usize],
            io: None,
            command_count: 0,
            last_command: None,
            last_reply: None,
            last_reply_report_count: None,
            last_reply_kind: None,
            last_outcome: None,
            last_error: None,
            next_token: 1,
            pending: VecDeque::new(),
        }
    }

    pub fn with_io(
        id: DriverId,
        candidate: MightexSiriusCandidate,
        io: Box<dyn HidFeatureIo>,
    ) -> Self {
        let mut driver = Self::discovered(id, candidate);
        driver.io = Some(io);
        driver
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        let mut hub_metadata = BTreeMap::from([
            (
                "support_level".into(),
                Value::String(HUB_SUPPORT_LEVEL.into()),
            ),
            (
                "write_support".into(),
                Value::String("reverse_engineered_output".into()),
            ),
            (
                "family".into(),
                Value::String(family_label(self.candidate.family).into()),
            ),
            (
                "product_string".into(),
                Value::String(self.candidate.product_string.clone()),
            ),
            (
                "channel_count".into(),
                Value::I64(self.channel_count as i64),
            ),
        ]);
        if let Some(module_type) = &self.module_type {
            hub_metadata.insert("module_type".into(), Value::String(module_type.clone()));
        }
        let mut hub_properties = vec![
            property("product_string", "Product string", ValueType::String),
            property("serial_number", "Serial number", ValueType::String),
            property("vendor_id", "USB vendor ID", ValueType::I64),
            property("product_id", "USB product ID", ValueType::I64),
            property("channel_count", "Channel count", ValueType::I64),
            property("support_level", "Support level", ValueType::String),
            property("command_count", "Command count", ValueType::I64),
            property("last_command", "Last command", ValueType::String),
            property("last_reply", "Last reply", ValueType::String),
            property("last_reply_kind", "Last reply kind", ValueType::String),
            property("last_outcome", "Last outcome", ValueType::String),
            property("last_error", "Last error", ValueType::String),
            property(
                "last_reply_report_count",
                "Last reply report count",
                ValueType::I64,
            ),
            property("last_transaction", "Last transaction", ValueType::Map),
        ];
        if self.module_type.is_some() {
            hub_properties.insert(5, property("module_type", "Module type", ValueType::String));
        }
        let mut descriptors = vec![DeviceDescriptor {
            id: self.hub,
            driver: self.id,
            label: format!("mightex-{}-hub", family_key(self.candidate.family)),
            vendor: Some("Mightex".into()),
            model: Some(self.candidate.product_string.clone()),
            serial: self.candidate.serial_number.clone(),
            kinds: vec!["hub".into(), "light.engine".into(), "hid.device".into()],
            properties: hub_properties,
            metadata: hub_metadata,
        }];

        for (index, device) in self.channels.iter().enumerate() {
            let mut channel_properties = vec![
                property("channel_index", "Channel index", ValueType::I64),
                property("output_supported", "Output supported", ValueType::Bool),
                sequenceable_writable_property("enabled", "Enabled", ValueType::Bool, None),
                mode_property(mode_values_for_family(self.candidate.family)),
                ranged_writable_property(
                    "mode_code",
                    "Mode code",
                    ValueType::I64,
                    None,
                    Value::I64(0),
                    Value::I64(u8::MAX as i64),
                ),
                ranged_sequenceable_writable_property(
                    "current_raw",
                    "Raw current",
                    ValueType::I64,
                    None,
                    Value::I64(0),
                    Value::I64(CURRENT_RAW_BRINGUP_MAX as i64),
                ),
                ranged_sequenceable_writable_property(
                    "intensity",
                    "Intensity",
                    ValueType::Ratio,
                    Some("percent"),
                    Value::Ratio(Ratio::from_percent(0.0)),
                    Value::Ratio(Ratio::from_percent(100.0)),
                ),
                property("support_level", "Support level", ValueType::String),
                volatile_property(
                    "overdrive_current_limit",
                    "Overdrive current limit",
                    ValueType::Ratio,
                    Some("percent"),
                ),
                volatile_property(
                    "overdrive_duty_cycle_limit",
                    "Overdrive duty-cycle limit",
                    ValueType::Ratio,
                    Some("percent"),
                ),
                volatile_property(
                    "overdrive_pulse_width_limit",
                    "Overdrive pulse-width limit",
                    ValueType::TimeInterval,
                    Some("time"),
                ),
            ];
            if self.candidate.family == MightexSiriusFamily::Bls {
                channel_properties.extend([
                    writable_property("soft_start", "Soft start", ValueType::Bool, None),
                    trigger_program_property(),
                    ranged_writable_property(
                        "trigger_repeat_count",
                        "Trigger repeat count",
                        ValueType::I64,
                        Some("count"),
                        Value::I64(1),
                        Value::I64(BLS_TRIGGER_REPEAT_MAX),
                    ),
                    ranged_writable_property(
                        "trigger_pulse_current_1",
                        "Trigger pulse current 1",
                        ValueType::I64,
                        Some("count"),
                        Value::I64(0),
                        Value::I64(BLS_TRIGGER_CURRENT_MAX),
                    ),
                    ranged_writable_property(
                        "trigger_pulse_current_2",
                        "Trigger pulse current 2",
                        ValueType::I64,
                        Some("count"),
                        Value::I64(0),
                        Value::I64(BLS_TRIGGER_CURRENT_MAX),
                    ),
                    ranged_writable_property(
                        "trigger_pulse_current_3",
                        "Trigger pulse current 3",
                        ValueType::I64,
                        Some("count"),
                        Value::I64(0),
                        Value::I64(BLS_TRIGGER_CURRENT_MAX),
                    ),
                    ranged_writable_property(
                        "trigger_pulse_time_1",
                        "Trigger pulse time 1",
                        ValueType::I64,
                        Some("count"),
                        Value::I64(1),
                        Value::I64(BLS_TRIGGER_TIME_MAX),
                    ),
                    ranged_writable_property(
                        "trigger_pulse_time_2",
                        "Trigger pulse time 2",
                        ValueType::I64,
                        Some("count"),
                        Value::I64(1),
                        Value::I64(BLS_TRIGGER_TIME_MAX),
                    ),
                    ranged_writable_property(
                        "trigger_pulse_time_3",
                        "Trigger pulse time 3",
                        ValueType::I64,
                        Some("count"),
                        Value::I64(1),
                        Value::I64(BLS_TRIGGER_TIME_MAX),
                    ),
                    ranged_writable_property(
                        "trigger_follow_on_current",
                        "Trigger follow on current",
                        ValueType::I64,
                        Some("count"),
                        Value::I64(0),
                        Value::I64(BLS_TRIGGER_CURRENT_MAX),
                    ),
                    ranged_writable_property(
                        "trigger_follow_off_current",
                        "Trigger follow off current",
                        ValueType::I64,
                        Some("count"),
                        Value::I64(0),
                        Value::I64(BLS_TRIGGER_CURRENT_MAX),
                    ),
                ]);
            } else {
                channel_properties.extend([
                    ranged_writable_property(
                        "normal_current_max_raw",
                        "Raw normal maximum current",
                        ValueType::I64,
                        Some("count"),
                        Value::I64(0),
                        Value::I64(SLC_NORMAL_CURRENT_RAW_MAX),
                    ),
                    ranged_writable_property(
                        "normal_current_set_raw",
                        "Raw normal current setpoint",
                        ValueType::I64,
                        Some("count"),
                        Value::I64(0),
                        Value::I64(SLC_NORMAL_CURRENT_RAW_MAX),
                    ),
                    ranged_writable_property(
                        "strobe_current_max_raw",
                        "Raw strobe maximum current",
                        ValueType::I64,
                        Some("count"),
                        Value::I64(0),
                        Value::I64(SLC_PROFILE_CURRENT_RAW_MAX),
                    ),
                    ranged_writable_property(
                        "strobe_repeat_count_raw",
                        "Raw strobe repeat count",
                        ValueType::I64,
                        Some("count"),
                        Value::I64(1),
                        Value::I64(SLC_PROFILE_REPEAT_MAX),
                    ),
                    ranged_writable_property(
                        "trigger_current_max_raw",
                        "Raw trigger maximum current",
                        ValueType::I64,
                        Some("count"),
                        Value::I64(0),
                        Value::I64(SLC_PROFILE_CURRENT_RAW_MAX),
                    ),
                    ranged_writable_property(
                        "trigger_polarity_raw",
                        "Raw trigger polarity",
                        ValueType::I64,
                        Some("count"),
                        Value::I64(0),
                        Value::I64(1),
                    ),
                    ranged_writable_property(
                        "profile_frequency",
                        "Profile frequency",
                        ValueType::Frequency,
                        Some("frequency"),
                        Value::Frequency(Frequency::from_hertz(1.0)),
                        Value::Frequency(Frequency::from_hertz(SLC_PROFILE_FREQUENCY_MAX_HZ)),
                    ),
                    ranged_writable_property(
                        "profile_duty_cycle",
                        "Profile duty cycle",
                        ValueType::Ratio,
                        Some("percent"),
                        Value::Ratio(Ratio::from_percent(0.0)),
                        Value::Ratio(Ratio::from_percent(100.0)),
                    ),
                    ranged_writable_property(
                        "profile_current_1_raw",
                        "Raw profile current 1",
                        ValueType::I64,
                        Some("count"),
                        Value::I64(0),
                        Value::I64(SLC_PROFILE_CURRENT_RAW_MAX),
                    ),
                    ranged_writable_property(
                        "profile_current_2_raw",
                        "Raw profile current 2",
                        ValueType::I64,
                        Some("count"),
                        Value::I64(0),
                        Value::I64(SLC_PROFILE_CURRENT_RAW_MAX),
                    ),
                    volatile_property(
                        "mode_code_readback",
                        "Mode code readback",
                        ValueType::I64,
                        None,
                    ),
                    volatile_property(
                        "current_max_raw_readback",
                        "Raw maximum current readback",
                        ValueType::I64,
                        Some("count"),
                    ),
                    volatile_property(
                        "current_raw_readback",
                        "Raw current readback",
                        ValueType::I64,
                        Some("count"),
                    ),
                    volatile_property(
                        "strobe_current_max_raw_readback",
                        "Raw strobe maximum current readback",
                        ValueType::I64,
                        Some("count"),
                    ),
                    volatile_property(
                        "strobe_repeat_count_raw_readback",
                        "Raw strobe repeat count readback",
                        ValueType::I64,
                        Some("count"),
                    ),
                    volatile_property(
                        "strobe_profile_raw_readback",
                        "Raw strobe profile readback",
                        ValueType::List,
                        Some("count"),
                    ),
                    volatile_property(
                        "trigger_current_max_raw_readback",
                        "Raw trigger maximum current readback",
                        ValueType::I64,
                        Some("count"),
                    ),
                    volatile_property(
                        "trigger_polarity_raw_readback",
                        "Raw trigger polarity readback",
                        ValueType::I64,
                        Some("count"),
                    ),
                    volatile_property(
                        "trigger_profile_raw_readback",
                        "Raw trigger profile readback",
                        ValueType::List,
                        Some("count"),
                    ),
                    volatile_property(
                        "load_voltage_raw",
                        "Raw load voltage",
                        ValueType::I64,
                        Some("count"),
                    ),
                ]);
            }
            descriptors.push(DeviceDescriptor {
                id: *device,
                driver: self.id,
                label: format!(
                    "mightex-{}-channel-{}",
                    family_key(self.candidate.family),
                    index + 1
                ),
                vendor: Some("Mightex".into()),
                model: Some(self.candidate.product_string.clone()),
                serial: self.candidate.serial_number.clone(),
                kinds: vec![
                    "light.source".into(),
                    "led.channel".into(),
                    "trigger.sink".into(),
                ],
                properties: channel_properties,
                metadata: BTreeMap::from([
                    ("channel_index".into(), Value::I64(index as i64 + 1)),
                    (
                        "current_raw_bringup_max".into(),
                        Value::I64(CURRENT_RAW_BRINGUP_MAX as i64),
                    ),
                    (
                        "calibration_status".into(),
                        Value::String("unvalidated_percent_to_current_mapping".into()),
                    ),
                    (
                        "support_level".into(),
                        Value::String(CHANNEL_SUPPORT_LEVEL.into()),
                    ),
                ]),
            });
        }

        descriptors
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "product_string" => Ok(Value::String(self.candidate.product_string.clone())),
                "serial_number" => Ok(self
                    .candidate
                    .serial_number
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null)),
                "vendor_id" => Ok(Value::I64(self.candidate.vendor_id as i64)),
                "product_id" => Ok(Value::I64(self.candidate.product_id as i64)),
                "channel_count" => Ok(Value::I64(self.channel_count as i64)),
                "module_type" => Ok(self
                    .module_type
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null)),
                "support_level" => Ok(Value::String(HUB_SUPPORT_LEVEL.into())),
                "command_count" => Ok(Value::I64(self.command_count as i64)),
                "last_command" => Ok(self
                    .last_command
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null)),
                "last_reply" => Ok(self
                    .last_reply
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null)),
                "last_reply_kind" => Ok(self
                    .last_reply_kind
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null)),
                "last_outcome" => Ok(self
                    .last_outcome
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null)),
                "last_error" => Ok(self
                    .last_error
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null)),
                "last_reply_report_count" => Ok(self
                    .last_reply_report_count
                    .map(|count| Value::I64(count as i64))
                    .unwrap_or(Value::Null)),
                "last_transaction" => Ok(self.last_transaction()),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown Mightex Sirius hub property {key}"),
                )),
            };
        }

        if let Some(index) = self.channels.iter().position(|channel| *channel == device) {
            return match key {
                "channel_index" => Ok(Value::I64(index as i64 + 1)),
                "output_supported" => Ok(Value::Bool(true)),
                "enabled" => Ok(Value::Bool(self.enabled[index])),
                "soft_start" if self.candidate.family == MightexSiriusFamily::Bls => {
                    Ok(Value::Bool(self.soft_start[index]))
                }
                "trigger_program" if self.candidate.family == MightexSiriusFamily::Bls => {
                    Ok(Value::String(self.trigger_program[index].as_str().into()))
                }
                "trigger_repeat_count" if self.candidate.family == MightexSiriusFamily::Bls => {
                    Ok(Value::I64(self.trigger_repeat_count[index]))
                }
                "trigger_pulse_current_1" if self.candidate.family == MightexSiriusFamily::Bls => {
                    Ok(Value::I64(self.trigger_pulse_current_raw[index][0]))
                }
                "trigger_pulse_current_2" if self.candidate.family == MightexSiriusFamily::Bls => {
                    Ok(Value::I64(self.trigger_pulse_current_raw[index][1]))
                }
                "trigger_pulse_current_3" if self.candidate.family == MightexSiriusFamily::Bls => {
                    Ok(Value::I64(self.trigger_pulse_current_raw[index][2]))
                }
                "trigger_pulse_time_1" if self.candidate.family == MightexSiriusFamily::Bls => {
                    Ok(Value::I64(self.trigger_pulse_time_raw[index][0]))
                }
                "trigger_pulse_time_2" if self.candidate.family == MightexSiriusFamily::Bls => {
                    Ok(Value::I64(self.trigger_pulse_time_raw[index][1]))
                }
                "trigger_pulse_time_3" if self.candidate.family == MightexSiriusFamily::Bls => {
                    Ok(Value::I64(self.trigger_pulse_time_raw[index][2]))
                }
                "trigger_follow_on_current"
                    if self.candidate.family == MightexSiriusFamily::Bls =>
                {
                    Ok(Value::I64(self.trigger_follow_on_current_raw[index]))
                }
                "trigger_follow_off_current"
                    if self.candidate.family == MightexSiriusFamily::Bls =>
                {
                    Ok(Value::I64(self.trigger_follow_off_current_raw[index]))
                }
                "mode" => Ok(Value::String(mode_name(self.mode_code[index]).into())),
                "mode_code" => Ok(Value::I64(self.mode_code[index] as i64)),
                "current_raw" => Ok(Value::I64(self.current_raw[index] as i64)),
                "normal_current_max_raw" if self.candidate.family == MightexSiriusFamily::Slc => {
                    Ok(Value::I64(self.normal_current_max_raw[index]))
                }
                "normal_current_set_raw" if self.candidate.family == MightexSiriusFamily::Slc => {
                    Ok(Value::I64(self.normal_current_set_raw[index]))
                }
                "strobe_current_max_raw" if self.candidate.family == MightexSiriusFamily::Slc => {
                    Ok(Value::I64(self.strobe_current_max_raw[index]))
                }
                "strobe_repeat_count_raw" if self.candidate.family == MightexSiriusFamily::Slc => {
                    Ok(Value::I64(self.strobe_repeat_count_raw[index]))
                }
                "trigger_current_max_raw" if self.candidate.family == MightexSiriusFamily::Slc => {
                    Ok(Value::I64(self.trigger_current_max_raw[index]))
                }
                "trigger_polarity_raw" if self.candidate.family == MightexSiriusFamily::Slc => {
                    Ok(Value::I64(self.trigger_polarity_raw[index]))
                }
                "profile_frequency" if self.candidate.family == MightexSiriusFamily::Slc => {
                    Ok(Value::Frequency(self.profile_frequency[index]))
                }
                "profile_duty_cycle" if self.candidate.family == MightexSiriusFamily::Slc => {
                    Ok(Value::Ratio(self.profile_duty_cycle[index]))
                }
                "profile_current_1_raw" if self.candidate.family == MightexSiriusFamily::Slc => {
                    Ok(Value::I64(self.profile_current_1_raw[index]))
                }
                "profile_current_2_raw" if self.candidate.family == MightexSiriusFamily::Slc => {
                    Ok(Value::I64(self.profile_current_2_raw[index]))
                }
                "intensity" => Ok(Value::Ratio(Ratio::from_percent(
                    self.intensity_percent[index],
                ))),
                "overdrive_current_limit" => Ok(self.overdrive_current_limit[index]
                    .map(Value::Ratio)
                    .unwrap_or(Value::Null)),
                "overdrive_duty_cycle_limit" => Ok(self.overdrive_duty_cycle_limit[index]
                    .map(Value::Ratio)
                    .unwrap_or(Value::Null)),
                "overdrive_pulse_width_limit" => Ok(self.overdrive_pulse_width_limit[index]
                    .map(Value::TimeInterval)
                    .unwrap_or(Value::Null)),
                "mode_code_readback" if self.candidate.family == MightexSiriusFamily::Slc => {
                    Ok(self.mode_code_readback[index]
                        .map(|value| Value::I64(value as i64))
                        .unwrap_or(Value::Null))
                }
                "current_raw_readback" if self.candidate.family == MightexSiriusFamily::Slc => {
                    Ok(self.current_raw_readback[index]
                        .map(Value::I64)
                        .unwrap_or(Value::Null))
                }
                "current_max_raw_readback" if self.candidate.family == MightexSiriusFamily::Slc => {
                    Ok(self.current_max_raw_readback[index]
                        .map(Value::I64)
                        .unwrap_or(Value::Null))
                }
                "strobe_current_max_raw_readback"
                    if self.candidate.family == MightexSiriusFamily::Slc =>
                {
                    Ok(self.strobe_current_max_raw_readback[index]
                        .map(Value::I64)
                        .unwrap_or(Value::Null))
                }
                "strobe_repeat_count_raw_readback"
                    if self.candidate.family == MightexSiriusFamily::Slc =>
                {
                    Ok(self.strobe_repeat_count_raw_readback[index]
                        .map(Value::I64)
                        .unwrap_or(Value::Null))
                }
                "strobe_profile_raw_readback"
                    if self.candidate.family == MightexSiriusFamily::Slc =>
                {
                    Ok(self.strobe_profile_raw_readback[index]
                        .as_deref()
                        .map(raw_profile_value)
                        .unwrap_or(Value::Null))
                }
                "trigger_current_max_raw_readback"
                    if self.candidate.family == MightexSiriusFamily::Slc =>
                {
                    Ok(self.trigger_current_max_raw_readback[index]
                        .map(Value::I64)
                        .unwrap_or(Value::Null))
                }
                "trigger_polarity_raw_readback"
                    if self.candidate.family == MightexSiriusFamily::Slc =>
                {
                    Ok(self.trigger_polarity_raw_readback[index]
                        .map(Value::I64)
                        .unwrap_or(Value::Null))
                }
                "trigger_profile_raw_readback"
                    if self.candidate.family == MightexSiriusFamily::Slc =>
                {
                    Ok(self.trigger_profile_raw_readback[index]
                        .as_deref()
                        .map(raw_profile_value)
                        .unwrap_or(Value::Null))
                }
                "load_voltage_raw" if self.candidate.family == MightexSiriusFamily::Slc => Ok(self
                    .load_voltage_raw[index]
                    .map(Value::I64)
                    .unwrap_or(Value::Null)),
                "support_level" => Ok(Value::String(CHANNEL_SUPPORT_LEVEL.into())),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown Mightex Sirius channel property {key}"),
                )),
            };
        }

        Err(Error::new(
            ErrorCode::InvalidCommand,
            "unknown Mightex Sirius device",
        ))
    }

    fn read_property_live(&mut self, device: DeviceId, key: &str) -> Result<Value> {
        if let Some(index) = self.channel_index(device) {
            let channel = (index + 1) as u8;
            match key {
                "overdrive_current_limit" => {
                    let reply =
                        self.send_checked(protocol::SiriusCommand::QueryImax { channel })?;
                    let raw = parse_reply_integer(&reply, 2).ok_or_else(|| {
                        Error::new(
                            ErrorCode::Transport,
                            "Mightex Sirius ?GetImax reply did not contain parameter 2",
                        )
                    })?;
                    let value = overdrive_tenths_percent(raw)?;
                    self.overdrive_current_limit[index] = Some(value);
                    let value = Value::Ratio(value);
                    self.emit_property(device, key, value.clone());
                    return Ok(value);
                }
                "overdrive_duty_cycle_limit" | "overdrive_pulse_width_limit" => {
                    let reply =
                        self.send_checked(protocol::SiriusCommand::QueryOdRules { channel })?;
                    let duty_raw = parse_reply_integer(&reply, 1).ok_or_else(|| {
                        Error::new(
                            ErrorCode::Transport,
                            "Mightex Sirius ?GetODRules reply did not contain parameter 1",
                        )
                    })?;
                    let pulse_width_us = parse_reply_integer(&reply, 2).ok_or_else(|| {
                        Error::new(
                            ErrorCode::Transport,
                            "Mightex Sirius ?GetODRules reply did not contain parameter 2",
                        )
                    })?;
                    let duty = overdrive_tenths_percent(duty_raw)?;
                    let pulse_width =
                        TimeInterval::from_microseconds(nonnegative_f64(pulse_width_us)?);
                    self.overdrive_duty_cycle_limit[index] = Some(duty);
                    self.overdrive_pulse_width_limit[index] = Some(pulse_width);
                    self.emit_property(device, "overdrive_duty_cycle_limit", Value::Ratio(duty));
                    self.emit_property(
                        device,
                        "overdrive_pulse_width_limit",
                        Value::TimeInterval(pulse_width),
                    );
                    return self.read_property(device, key);
                }
                "mode_code_readback" if self.candidate.family == MightexSiriusFamily::Slc => {
                    self.flush_slc_ascii_readback()?;
                    let reply =
                        self.send_checked(protocol::SiriusCommand::QueryMode { channel })?;
                    let raw = parse_reply_integer(&reply, 1).ok_or_else(|| {
                        Error::new(
                            ErrorCode::Transport,
                            "Mightex Sirius ?MODE reply did not contain parameter 1",
                        )
                    })?;
                    let value = u8::try_from(raw).map_err(|_| {
                        Error::new(
                            ErrorCode::Transport,
                            "Mightex Sirius ?MODE reply was outside u8 range",
                        )
                    })?;
                    self.mode_code_readback[index] = Some(value);
                    let value = Value::I64(value as i64);
                    self.emit_property(device, key, value.clone());
                    return Ok(value);
                }
                "current_raw_readback" | "current_max_raw_readback"
                    if self.candidate.family == MightexSiriusFamily::Slc =>
                {
                    self.read_slc_current_readback(index, channel, device)?;
                    return self.read_property(device, key);
                }
                "strobe_current_max_raw_readback"
                | "strobe_repeat_count_raw_readback"
                | "strobe_profile_raw_readback"
                    if self.candidate.family == MightexSiriusFamily::Slc =>
                {
                    self.read_slc_strobe_readback(index, channel, device)?;
                    return self.read_property(device, key);
                }
                "trigger_current_max_raw_readback"
                | "trigger_polarity_raw_readback"
                | "trigger_profile_raw_readback"
                    if self.candidate.family == MightexSiriusFamily::Slc =>
                {
                    self.read_slc_trigger_readback(index, channel, device)?;
                    return self.read_property(device, key);
                }
                "load_voltage_raw" if self.candidate.family == MightexSiriusFamily::Slc => {
                    let raw = self.read_slc_binary_voltage(channel)?;
                    self.load_voltage_raw[index] = Some(i64::from(raw));
                    let value = Value::I64(i64::from(raw));
                    self.emit_property(device, key, value.clone());
                    return Ok(value);
                }
                _ => {}
            }
        }
        self.read_property(device, key)
    }

    fn channel_index(&self, device: DeviceId) -> Option<usize> {
        self.channels.iter().position(|channel| *channel == device)
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
        let index = self.channel_index(device).ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidCommand,
                "Mightex Sirius output writes target channel devices",
            )
        })?;
        let channel = (index + 1) as u8;
        match (key, value) {
            ("enabled", Value::Bool(enabled)) => {
                let mode = if *enabled {
                    if self.mode_code[index] == MODE_DISABLED {
                        MODE_NORMAL
                    } else {
                        self.mode_code[index]
                    }
                } else {
                    MODE_DISABLED
                };
                self.send_mode(index, channel, mode)?;
                self.enabled[index] = *enabled;
                if *enabled && self.mode_code[index] == MODE_DISABLED {
                    self.mode_code[index] = MODE_NORMAL;
                }
                Ok(Value::Bool(*enabled))
            }
            ("soft_start", Value::Bool(enabled))
                if self.candidate.family == MightexSiriusFamily::Bls =>
            {
                self.soft_start[index] = *enabled;
                Ok(Value::Bool(*enabled))
            }
            ("trigger_program", Value::String(program))
                if self.candidate.family == MightexSiriusFamily::Bls =>
            {
                let program = BlsTriggerProgram::parse(program)?;
                self.trigger_program[index] = program;
                Ok(Value::String(program.as_str().into()))
            }
            ("trigger_repeat_count", Value::I64(count))
                if self.candidate.family == MightexSiriusFamily::Bls =>
            {
                self.trigger_repeat_count[index] = *count;
                Ok(Value::I64(*count))
            }
            ("trigger_pulse_current_1", Value::I64(raw))
                if self.candidate.family == MightexSiriusFamily::Bls =>
            {
                self.trigger_pulse_current_raw[index][0] = *raw;
                Ok(Value::I64(*raw))
            }
            ("trigger_pulse_current_2", Value::I64(raw))
                if self.candidate.family == MightexSiriusFamily::Bls =>
            {
                self.trigger_pulse_current_raw[index][1] = *raw;
                Ok(Value::I64(*raw))
            }
            ("trigger_pulse_current_3", Value::I64(raw))
                if self.candidate.family == MightexSiriusFamily::Bls =>
            {
                self.trigger_pulse_current_raw[index][2] = *raw;
                Ok(Value::I64(*raw))
            }
            ("trigger_pulse_time_1", Value::I64(raw))
                if self.candidate.family == MightexSiriusFamily::Bls =>
            {
                self.trigger_pulse_time_raw[index][0] = *raw;
                Ok(Value::I64(*raw))
            }
            ("trigger_pulse_time_2", Value::I64(raw))
                if self.candidate.family == MightexSiriusFamily::Bls =>
            {
                self.trigger_pulse_time_raw[index][1] = *raw;
                Ok(Value::I64(*raw))
            }
            ("trigger_pulse_time_3", Value::I64(raw))
                if self.candidate.family == MightexSiriusFamily::Bls =>
            {
                self.trigger_pulse_time_raw[index][2] = *raw;
                Ok(Value::I64(*raw))
            }
            ("trigger_follow_on_current", Value::I64(raw))
                if self.candidate.family == MightexSiriusFamily::Bls =>
            {
                self.trigger_follow_on_current_raw[index] = *raw;
                Ok(Value::I64(*raw))
            }
            ("trigger_follow_off_current", Value::I64(raw))
                if self.candidate.family == MightexSiriusFamily::Bls =>
            {
                self.trigger_follow_off_current_raw[index] = *raw;
                Ok(Value::I64(*raw))
            }
            ("mode", Value::String(mode)) => {
                let mode = parse_mode_name(self.candidate.family, mode)?;
                if mode_is_immediate(self.candidate.family, mode) {
                    self.send_mode(index, channel, mode)?;
                    self.enabled[index] = mode != MODE_DISABLED;
                }
                self.mode_code[index] = mode;
                Ok(Value::String(mode_name(mode).into()))
            }
            ("mode_code", Value::I64(mode)) => {
                let mode = *mode as u8;
                self.send_mode(index, channel, mode)?;
                self.mode_code[index] = mode;
                self.enabled[index] = mode != 0;
                Ok(Value::I64(mode as i64))
            }
            ("normal_current_max_raw", Value::I64(raw))
                if self.candidate.family == MightexSiriusFamily::Slc =>
            {
                let new_set = self.normal_current_set_raw[index].min(*raw);
                self.send_slc_normal(channel, *raw, new_set)?;
                self.send_checked(protocol::SiriusCommand::Current {
                    channel,
                    value: new_set as u32,
                })?;
                self.normal_current_max_raw[index] = *raw;
                self.normal_current_set_raw[index] = new_set;
                self.current_raw[index] = new_set as u32;
                self.intensity_percent[index] = new_set as f64;
                self.emit_property(device, "normal_current_set_raw", Value::I64(new_set));
                self.emit_property(device, "current_raw", Value::I64(new_set));
                self.emit_property(
                    device,
                    "intensity",
                    Value::Ratio(Ratio::from_percent(self.intensity_percent[index])),
                );
                Ok(Value::I64(*raw))
            }
            ("normal_current_set_raw", Value::I64(raw))
                if self.candidate.family == MightexSiriusFamily::Slc =>
            {
                if *raw > self.normal_current_max_raw[index] {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Mightex SLC normal_current_set_raw must be <= normal_current_max_raw",
                    ));
                }
                self.send_slc_normal(channel, self.normal_current_max_raw[index], *raw)?;
                self.send_checked(protocol::SiriusCommand::Current {
                    channel,
                    value: *raw as u32,
                })?;
                self.normal_current_set_raw[index] = *raw;
                self.current_raw[index] = *raw as u32;
                self.intensity_percent[index] = *raw as f64;
                self.emit_property(device, "current_raw", Value::I64(*raw));
                self.emit_property(
                    device,
                    "intensity",
                    Value::Ratio(Ratio::from_percent(self.intensity_percent[index])),
                );
                Ok(Value::I64(*raw))
            }
            ("strobe_current_max_raw", Value::I64(raw))
                if self.candidate.family == MightexSiriusFamily::Slc =>
            {
                self.strobe_current_max_raw[index] = *raw;
                Ok(Value::I64(*raw))
            }
            ("strobe_repeat_count_raw", Value::I64(raw))
                if self.candidate.family == MightexSiriusFamily::Slc =>
            {
                self.strobe_repeat_count_raw[index] = *raw;
                Ok(Value::I64(*raw))
            }
            ("trigger_current_max_raw", Value::I64(raw))
                if self.candidate.family == MightexSiriusFamily::Slc =>
            {
                self.trigger_current_max_raw[index] = *raw;
                Ok(Value::I64(*raw))
            }
            ("trigger_polarity_raw", Value::I64(raw))
                if self.candidate.family == MightexSiriusFamily::Slc =>
            {
                self.trigger_polarity_raw[index] = *raw;
                Ok(Value::I64(*raw))
            }
            ("profile_frequency", Value::Frequency(frequency))
                if self.candidate.family == MightexSiriusFamily::Slc =>
            {
                if !frequency.hertz().is_finite() || frequency.hertz() <= 0.0 {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Mightex SLC profile_frequency must be positive",
                    ));
                }
                self.profile_frequency[index] = *frequency;
                Ok(Value::Frequency(*frequency))
            }
            ("profile_duty_cycle", Value::Ratio(ratio))
                if self.candidate.family == MightexSiriusFamily::Slc =>
            {
                self.profile_duty_cycle[index] = *ratio;
                Ok(Value::Ratio(*ratio))
            }
            ("profile_current_1_raw", Value::I64(raw))
                if self.candidate.family == MightexSiriusFamily::Slc =>
            {
                self.profile_current_1_raw[index] = *raw;
                Ok(Value::I64(*raw))
            }
            ("profile_current_2_raw", Value::I64(raw))
                if self.candidate.family == MightexSiriusFamily::Slc =>
            {
                self.profile_current_2_raw[index] = *raw;
                Ok(Value::I64(*raw))
            }
            ("current_raw", Value::I64(raw)) => {
                let raw = *raw as u32;
                self.send_checked(protocol::SiriusCommand::Current {
                    channel,
                    value: raw,
                })?;
                self.current_raw[index] = raw;
                self.intensity_percent[index] = raw as f64;
                Ok(Value::I64(raw as i64))
            }
            ("intensity", Value::Ratio(ratio)) => {
                let percent = ratio.percent();
                let raw = percent.round().clamp(0.0, CURRENT_RAW_BRINGUP_MAX as f64) as u32;
                self.send_checked(protocol::SiriusCommand::Current {
                    channel,
                    value: raw,
                })?;
                self.current_raw[index] = raw;
                self.intensity_percent[index] = percent;
                Ok(Value::Ratio(Ratio::from_percent(percent)))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid Mightex Sirius write {key}"),
            )),
        }
    }

    fn apply_state_set(&mut self, set: StateSet) -> Result<Value> {
        let mut result = BTreeMap::new();
        for write in set.writes {
            let value = self.write_property(write.device, &write.property, &write.value)?;
            self.emit_property(write.device, &write.property, value.clone());
            result.insert(format!("{}:{}", (write.device.0).0, write.property), value);
        }
        Ok(Value::Map(result))
    }

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| self.channel_index(sequence.device).is_some())
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequences(plan) {
            if !matches!(
                sequence.property.as_str(),
                "enabled" | "current_raw" | "intensity"
            ) {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Mightex Sirius timing sequences can only target enabled, current_raw, or intensity",
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
            ("hub".into(), Value::I64(self.hub.0 .0 as i64)),
            (
                "family".into(),
                Value::String(family_label(self.candidate.family).into()),
            ),
            (
                "sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
            ("last_transaction".into(), self.last_transaction()),
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
                }?;
                Some(StateWrite {
                    device: sequence.device,
                    property: sequence.property.clone(),
                    value: value.clone(),
                })
            })
            .collect::<Vec<_>>();
        if writes.is_empty() {
            return Ok(Value::Map(BTreeMap::new()));
        }
        let applied = self.apply_state_set(StateSet {
            name: Some(if first {
                "mightex sirius timing start sequence".into()
            } else {
                "mightex sirius timing stop sequence".into()
            }),
            writes,
            commit: CommitMode::Immediate,
        })?;
        Ok(Value::Map(BTreeMap::from([
            ("applied".into(), applied),
            (
                "completion_basis".into(),
                Value::String("HID write and reply readback".into()),
            ),
            (
                "support_level".into(),
                Value::String(CHANNEL_SUPPORT_LEVEL.into()),
            ),
        ])))
    }

    fn send_mode(&mut self, index: usize, channel: u8, mode: u8) -> Result<()> {
        if self.candidate.family == MightexSiriusFamily::Slc && mode == MODE_STROBE {
            self.configure_slc_strobe(index, channel)?;
        } else if self.candidate.family == MightexSiriusFamily::Slc && mode == MODE_TRIGGER {
            self.configure_slc_trigger(index, channel)?;
        }
        self.send_checked(protocol::SiriusCommand::Mode { channel, mode })?;
        if self.candidate.family == MightexSiriusFamily::Bls && mode == MODE_TRIGGER {
            self.configure_bls_trigger(index, channel)?;
            if self.soft_start[index] {
                self.send_checked(protocol::SiriusCommand::SoftStart { channel })?;
            }
        }
        Ok(())
    }

    fn configure_slc_strobe(&mut self, index: usize, channel: u8) -> Result<()> {
        let profile = self.slc_profile(index);
        self.send_checked(protocol::SiriusCommand::SlcStrobe {
            channel,
            current_max: self.strobe_current_max_raw[index],
            repeat_count: self.strobe_repeat_count_raw[index],
        })?;
        for (line, step) in profile.iter().enumerate() {
            self.send_checked(protocol::SiriusCommand::SlcStrobeStep {
                channel,
                line: line as u8,
                current: step.current_raw,
                time: step.time_raw,
            })?;
        }
        Ok(())
    }

    fn configure_slc_trigger(&mut self, index: usize, channel: u8) -> Result<()> {
        let profile = self.slc_profile(index);
        self.send_checked(protocol::SiriusCommand::SlcTrigger {
            channel,
            current_max: self.trigger_current_max_raw[index],
            polarity: self.trigger_polarity_raw[index],
        })?;
        for (line, step) in profile.iter().enumerate() {
            self.send_checked(protocol::SiriusCommand::SlcTriggerStep {
                channel,
                line: line as u8,
                current: step.current_raw,
                time: step.time_raw,
            })?;
        }
        Ok(())
    }

    fn slc_profile(&self, index: usize) -> [RawProfileStep; 3] {
        let mut period = (1_000_000.0 / self.profile_frequency[index].hertz()).round() as i64;
        if matches!(self.module_type.as_deref(), Some("MA" | "CA")) {
            period = period / 100 * 100;
            period = period.max(2_000);
        } else {
            period = period / 20 * 20;
            period = period.max(40);
        }
        let mut on_time =
            (period as f64 * self.profile_duty_cycle[index].percent() / 100.0).round() as i64;
        on_time = on_time / 20 * 20;
        on_time = on_time.max(20);
        let mut off_time = period - on_time;
        if off_time == 0 {
            off_time = 20;
            on_time -= 20;
        }
        [
            RawProfileStep {
                current_raw: self.profile_current_1_raw[index],
                time_raw: off_time,
            },
            RawProfileStep {
                current_raw: self.profile_current_2_raw[index],
                time_raw: on_time,
            },
            RawProfileStep {
                current_raw: 0,
                time_raw: 0,
            },
        ]
    }

    fn send_slc_normal(&mut self, channel: u8, current_max: i64, current_set: i64) -> Result<()> {
        self.send_checked(protocol::SiriusCommand::Normal {
            channel,
            current_max,
            current_set,
        })?;
        Ok(())
    }

    fn configure_bls_trigger(&mut self, index: usize, channel: u8) -> Result<()> {
        match self.trigger_program[index] {
            BlsTriggerProgram::Pulse => {
                self.send_checked(protocol::SiriusCommand::TriggerProfile {
                    channel,
                    polarity: 1,
                    current_max: 100,
                    repeat_count: self.trigger_repeat_count[index],
                })?;
                for step in 0..3 {
                    self.send_checked(protocol::SiriusCommand::TriggerPulseStep {
                        channel,
                        step: step as u8,
                        current: self.trigger_pulse_current_raw[index][step],
                        time: self.trigger_pulse_time_raw[index][step],
                    })?;
                }
                self.send_checked(protocol::SiriusCommand::TriggerPulseStep {
                    channel,
                    step: 3,
                    current: 0,
                    time: 0,
                })?;
            }
            BlsTriggerProgram::Follow => {
                self.send_checked(protocol::SiriusCommand::TriggerPulseStep {
                    channel,
                    step: 0,
                    current: self.trigger_follow_off_current_raw[index],
                    time: 9_999,
                })?;
                self.send_checked(protocol::SiriusCommand::TriggerPulseStep {
                    channel,
                    step: 1,
                    current: self.trigger_follow_on_current_raw[index],
                    time: 9_999,
                })?;
                self.send_checked(protocol::SiriusCommand::TriggerPulseStep {
                    channel,
                    step: 2,
                    current: 0,
                    time: 0,
                })?;
            }
        }
        Ok(())
    }

    fn send_checked(&mut self, command: protocol::SiriusCommand) -> Result<String> {
        let command_text = command.ascii();
        let expects_reply = command.expects_reply();
        self.ensure_io()?;
        let io = self
            .io
            .as_mut()
            .ok_or_else(|| Error::new(ErrorCode::Transport, "Mightex Sirius HID device closed"))?;
        protocol::send_command(io.as_mut(), &command)?;
        let (reply_text, report_count) = if expects_reply {
            let reply = protocol::read_reply_limited_with_count(io.as_mut(), 64)?;
            (reply.text, reply.report_count)
        } else {
            (String::new(), 0)
        };
        let reply_error = sirius_reply_error(&reply_text);
        let outcome = if reply_error.is_some() {
            "failed_obvious_reply"
        } else if expects_reply {
            "accepted_unvalidated_reply"
        } else {
            "sent_no_reply_expected"
        };
        self.command_count += 1;
        self.last_command = Some(command_text.clone());
        self.last_reply = Some(reply_text.clone());
        self.last_reply_report_count = Some(report_count);
        self.last_reply_kind = Some(if expects_reply { "ascii" } else { "none" }.into());
        self.last_outcome = Some(outcome.into());
        self.last_error = reply_error.clone();
        self.emit_property(
            self.hub,
            "command_count",
            Value::I64(self.command_count as i64),
        );
        self.emit_property(
            self.hub,
            "last_command",
            Value::String(command_text.clone()),
        );
        self.emit_property(self.hub, "last_reply", Value::String(reply_text.clone()));
        self.emit_property(self.hub, "last_outcome", Value::String(outcome.into()));
        self.emit_property(
            self.hub,
            "last_reply_report_count",
            Value::I64(report_count as i64),
        );
        self.emit_property(
            self.hub,
            "last_error",
            self.last_error
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
        self.emit_property(self.hub, "last_transaction", self.last_transaction());
        let mut telemetry = BTreeMap::from([
            ("command".into(), Value::String(command_text.clone())),
            ("reply".into(), Value::String(reply_text.clone())),
            ("reply_report_count".into(), Value::I64(report_count as i64)),
            (
                "reply_kind".into(),
                Value::String(if expects_reply { "ascii" } else { "none" }.into()),
            ),
            ("outcome".into(), Value::String(outcome.into())),
            ("reply_expected".into(), Value::Bool(expects_reply)),
            (
                "command_count".into(),
                Value::I64(self.command_count as i64),
            ),
            (
                "support_level".into(),
                Value::String(HUB_SUPPORT_LEVEL.into()),
            ),
            (
                "wire_terminator".into(),
                Value::String(protocol::COMMAND_TERMINATOR_LABEL.into()),
            ),
        ]);
        if let Some(error) = &reply_error {
            telemetry.insert("reply_error".into(), Value::String(error.clone()));
        }
        self.pending
            .push_back(DriverEvent::Event(Event::Telemetry(TelemetryEvent {
                device: self.hub,
                values: telemetry,
            })));
        if let Some(error) = reply_error {
            return Err(Error::new(
                ErrorCode::Transport,
                format!("Mightex Sirius command {command_text} failed: {error}"),
            ));
        }
        Ok(reply_text)
    }

    fn flush_slc_ascii_readback(&mut self) -> Result<()> {
        let _ = self.send_checked(protocol::SiriusCommand::EchoOff)?;
        Ok(())
    }

    fn read_slc_current_readback(
        &mut self,
        index: usize,
        channel: u8,
        device: DeviceId,
    ) -> Result<()> {
        self.flush_slc_ascii_readback()?;
        let reply = self.send_checked(protocol::SiriusCommand::QueryCurrent { channel })?;
        let module_type = self.module_type.as_deref().unwrap_or_default();
        let max_raw = parse_slc_current_max(&reply, module_type).ok_or_else(|| {
            Error::new(
                ErrorCode::Transport,
                "Mightex Sirius ?CURRENT reply did not contain the expected current-max parameter",
            )
        })?;
        let set_raw = parse_slc_current_setpoint(&reply, module_type).ok_or_else(|| {
            Error::new(
                ErrorCode::Transport,
                "Mightex Sirius ?CURRENT reply did not contain the expected current-set parameter",
            )
        })?;
        self.current_max_raw_readback[index] = Some(max_raw);
        self.current_raw_readback[index] = Some(set_raw);
        self.normal_current_max_raw[index] = max_raw;
        self.normal_current_set_raw[index] = set_raw;
        self.emit_property(device, "current_max_raw_readback", Value::I64(max_raw));
        self.emit_property(device, "current_raw_readback", Value::I64(set_raw));
        self.emit_property(device, "normal_current_max_raw", Value::I64(max_raw));
        self.emit_property(device, "normal_current_set_raw", Value::I64(set_raw));
        Ok(())
    }

    fn read_slc_strobe_readback(
        &mut self,
        index: usize,
        channel: u8,
        device: DeviceId,
    ) -> Result<()> {
        self.flush_slc_ascii_readback()?;
        let strobe_reply = self.send_checked(protocol::SiriusCommand::QueryStrobe { channel })?;
        let current_max = parse_reply_integer(&strobe_reply, 1).ok_or_else(|| {
            Error::new(
                ErrorCode::Transport,
                "Mightex Sirius ?STROBE reply did not contain parameter 1",
            )
        })?;
        let repeat_count = parse_reply_integer(&strobe_reply, 2).ok_or_else(|| {
            Error::new(
                ErrorCode::Transport,
                "Mightex Sirius ?STROBE reply did not contain parameter 2",
            )
        })?;
        let profile_reply =
            self.send_checked(protocol::SiriusCommand::QueryStrobeProfile { channel })?;
        let profile = parse_raw_profile(&profile_reply);
        self.strobe_current_max_raw_readback[index] = Some(current_max);
        self.strobe_repeat_count_raw_readback[index] = Some(repeat_count);
        self.strobe_profile_raw_readback[index] = Some(profile.clone());
        self.emit_property(
            device,
            "strobe_current_max_raw_readback",
            Value::I64(current_max),
        );
        self.emit_property(
            device,
            "strobe_repeat_count_raw_readback",
            Value::I64(repeat_count),
        );
        self.emit_property(
            device,
            "strobe_profile_raw_readback",
            raw_profile_value(&profile),
        );
        Ok(())
    }

    fn read_slc_trigger_readback(
        &mut self,
        index: usize,
        channel: u8,
        device: DeviceId,
    ) -> Result<()> {
        self.flush_slc_ascii_readback()?;
        let trigger_reply =
            self.send_checked(protocol::SiriusCommand::QuerySlcTrigger { channel })?;
        let current_max = parse_reply_integer(&trigger_reply, 1).ok_or_else(|| {
            Error::new(
                ErrorCode::Transport,
                "Mightex Sirius ?TRIGGER reply did not contain parameter 1",
            )
        })?;
        let polarity = parse_reply_integer(&trigger_reply, 2).ok_or_else(|| {
            Error::new(
                ErrorCode::Transport,
                "Mightex Sirius ?TRIGGER reply did not contain parameter 2",
            )
        })?;
        let profile_reply =
            self.send_checked(protocol::SiriusCommand::QuerySlcTriggerProfile { channel })?;
        let profile = parse_raw_profile(&profile_reply);
        self.trigger_current_max_raw_readback[index] = Some(current_max);
        self.trigger_polarity_raw_readback[index] = Some(polarity);
        self.trigger_profile_raw_readback[index] = Some(profile.clone());
        self.emit_property(
            device,
            "trigger_current_max_raw_readback",
            Value::I64(current_max),
        );
        self.emit_property(
            device,
            "trigger_polarity_raw_readback",
            Value::I64(polarity),
        );
        self.emit_property(
            device,
            "trigger_profile_raw_readback",
            raw_profile_value(&profile),
        );
        Ok(())
    }

    fn last_transaction(&self) -> Value {
        if self.command_count == 0 && self.last_command.is_none() {
            return Value::Null;
        }
        let reply_expected =
            !matches!(self.last_outcome.as_deref(), Some("sent_no_reply_expected"));
        let mut transaction = BTreeMap::from([
            (
                "command_count".into(),
                Value::I64(self.command_count as i64),
            ),
            (
                "support_level".into(),
                Value::String(HUB_SUPPORT_LEVEL.into()),
            ),
            (
                "wire_terminator".into(),
                Value::String(protocol::COMMAND_TERMINATOR_LABEL.into()),
            ),
            (
                "command".into(),
                self.last_command
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            ),
            (
                "reply".into(),
                self.last_reply
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            ),
            (
                "reply_report_count".into(),
                self.last_reply_report_count
                    .map(|count| Value::I64(count as i64))
                    .unwrap_or(Value::Null),
            ),
            (
                "reply_kind".into(),
                self.last_reply_kind
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            ),
            (
                "outcome".into(),
                self.last_outcome
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            ),
            (
                "reply_error".into(),
                self.last_error
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            ),
            ("reply_expected".into(), Value::Bool(reply_expected)),
        ]);
        if let Some(module_type) = &self.module_type {
            transaction.insert("module_type".into(), Value::String(module_type.clone()));
        }
        Value::Map(transaction)
    }

    fn read_slc_binary_voltage(&mut self, channel: u8) -> Result<u16> {
        let command = protocol::SiriusCommand::ReadBinaryVoltage { channel };
        let command_text = command.ascii();
        self.ensure_io()?;
        let io = self
            .io
            .as_mut()
            .ok_or_else(|| Error::new(ErrorCode::Transport, "Mightex Sirius HID device closed"))?;
        protocol::send_command(io.as_mut(), &command)?;
        let (raw, report_count) = protocol::read_binary_echo_u16(io.as_mut(), 50)?;
        self.command_count += 1;
        self.last_command = Some(command_text.clone());
        self.last_reply = Some(format!("0x{raw:04X}"));
        self.last_reply_report_count = Some(report_count);
        self.last_reply_kind = Some("binary_echo_u16".into());
        self.last_outcome = Some("accepted_binary_echo_unvalidated".into());
        self.last_error = None;
        self.emit_property(
            self.hub,
            "command_count",
            Value::I64(self.command_count as i64),
        );
        self.emit_property(
            self.hub,
            "last_command",
            Value::String(command_text.clone()),
        );
        self.emit_property(
            self.hub,
            "last_reply",
            Value::String(format!("0x{raw:04X}")),
        );
        self.emit_property(
            self.hub,
            "last_outcome",
            Value::String("accepted_binary_echo_unvalidated".into()),
        );
        self.emit_property(
            self.hub,
            "last_reply_report_count",
            Value::I64(report_count as i64),
        );
        self.emit_property(self.hub, "last_error", Value::Null);
        self.emit_property(self.hub, "last_transaction", self.last_transaction());
        self.pending
            .push_back(DriverEvent::Event(Event::Telemetry(TelemetryEvent {
                device: self.hub,
                values: BTreeMap::from([
                    ("command".into(), Value::String(command_text)),
                    ("reply".into(), Value::String(format!("0x{raw:04X}"))),
                    ("reply_report_count".into(), Value::I64(report_count as i64)),
                    ("reply_kind".into(), Value::String("binary_echo_u16".into())),
                    ("reply_expected".into(), Value::Bool(true)),
                    (
                        "outcome".into(),
                        Value::String("accepted_binary_echo_unvalidated".into()),
                    ),
                    (
                        "command_count".into(),
                        Value::I64(self.command_count as i64),
                    ),
                    (
                        "support_level".into(),
                        Value::String(HUB_SUPPORT_LEVEL.into()),
                    ),
                    (
                        "wire_terminator".into(),
                        Value::String(protocol::COMMAND_TERMINATOR_LABEL.into()),
                    ),
                ]),
            })));
        Ok(raw)
    }

    fn ensure_io(&mut self) -> Result<()> {
        if self.io.is_some() {
            return Ok(());
        }
        #[cfg(feature = "os-hid")]
        {
            let mut config =
                OsHidFeatureConfig::new(self.candidate.vendor_id, self.candidate.product_id);
            if let Some(serial) = &self.candidate.serial_number {
                config = config.serial_number(serial.clone());
            }
            self.io = Some(Box::new(OsHidFeatureDevice::open_config(config)?));
            Ok(())
        }
        #[cfg(not(feature = "os-hid"))]
        {
            Err(Error::new(
                ErrorCode::Unsupported,
                "Mightex Sirius output requires the os-hid feature or an injected HID transport",
            ))
        }
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

    fn invoke_dac(&mut self, device: DeviceId, request: &CapabilityRequest) -> Result<Value> {
        let CapabilityRequest::Dac(request) = request else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Mightex Sirius Dac expects CapabilityRequest::Dac",
            ));
        };
        let Value::Ratio(_) = request.value else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Mightex Sirius Dac value must be Ratio percent",
            ));
        };
        self.write_property(device, "intensity", &request.value)
    }

    fn invoke_generic_command(
        &mut self,
        device: DeviceId,
        request: &CapabilityRequest,
    ) -> Result<Value> {
        if device != self.hub {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Mightex Sirius GenericCommand targets the hub",
            ));
        }
        let CapabilityRequest::GenericCommand(request) = request else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Mightex Sirius GenericCommand expects CapabilityRequest::GenericCommand",
            ));
        };
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
                "Mightex Sirius GenericCommand supports named aliases only",
            ));
        }
        let command = diagnostic_sirius_command(self.candidate.family, &request.command)?;
        let command_text = command.ascii();
        let reply = self.send_checked(command)?;
        let reply_expected =
            !matches!(self.last_outcome.as_deref(), Some("sent_no_reply_expected"));
        Ok(Value::Map(BTreeMap::from([
            ("command".into(), Value::String(command_text)),
            ("reply".into(), Value::String(reply)),
            ("reply_expected".into(), Value::Bool(reply_expected)),
            ("last_transaction".into(), self.last_transaction()),
            (
                "reply_report_count".into(),
                self.last_reply_report_count
                    .map(|count| Value::I64(count as i64))
                    .unwrap_or(Value::Null),
            ),
            (
                "support_level".into(),
                Value::String("diagnostic_bring_up".into()),
            ),
        ])))
    }

    fn invoke_trigger_sink(
        &mut self,
        device: DeviceId,
        request: &CapabilityRequest,
    ) -> Result<Value> {
        let actions = match request {
            CapabilityRequest::None => vec![true, false],
            CapabilityRequest::Trigger(request) => match request.action {
                TriggerAction::Enable => vec![true],
                TriggerAction::Disable => vec![false],
                TriggerAction::Pulse => vec![true, false],
            },
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "Mightex Sirius TriggerSink expects None or CapabilityRequest::Trigger",
                ))
            }
        };
        let mut last = Value::Null;
        for enabled in actions {
            last = self.write_property(device, "enabled", &Value::Bool(enabled))?;
            self.emit_property(device, "enabled", last.clone());
        }
        Ok(last)
    }

    fn invoke(
        &mut self,
        device: DeviceId,
        kind: CapabilityKind,
        request: &CapabilityRequest,
    ) -> Result<Value> {
        match kind {
            CapabilityKind::Dac => self.invoke_dac(device, request),
            CapabilityKind::TriggerSink => self.invoke_trigger_sink(device, request),
            CapabilityKind::GenericCommand => self.invoke_generic_command(device, request),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Mightex Sirius capability",
            )),
        }
    }
}

impl Driver for MightexBlsDriver {
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
            label: format!("mightex-{}-hid-feature", family_key(self.candidate.family)),
            kind: "usb.hid.feature".into(),
            metadata: BTreeMap::from([
                (
                    "vendor_id".into(),
                    Value::I64(self.candidate.vendor_id as i64),
                ),
                (
                    "product_id".into(),
                    Value::I64(self.candidate.product_id as i64),
                ),
                (
                    "product_string".into(),
                    Value::String(self.candidate.product_string.clone()),
                ),
                (
                    "support_level".into(),
                    Value::String(RESOURCE_SUPPORT_LEVEL.into()),
                ),
            ]),
        }]
    }

    fn graph(&self) -> DeviceGraph {
        let mut graph = DeviceGraph::default();
        let _ = graph.insert_node(GraphNode {
            id: self.resource.0,
            kind: NodeKind::Resource,
            label: format!("mightex-{}-hid-feature", family_key(self.candidate.family)),
        });
        let _ = graph.insert_node(GraphNode {
            id: self.hub.0,
            kind: NodeKind::Hub,
            label: format!("mightex-{}-hub", family_key(self.candidate.family)),
        });
        let _ = graph.insert_edge(GraphEdge {
            from: self.hub.0,
            to: self.resource.0,
            kind: EdgeKind::OwnsResource,
        });
        for (index, channel) in self.channels.iter().enumerate() {
            let _ = graph.insert_node(GraphNode {
                id: channel.0,
                kind: NodeKind::Device,
                label: format!(
                    "mightex-{}-channel-{}",
                    family_key(self.candidate.family),
                    index + 1
                ),
            });
            let _ = graph.insert_edge(GraphEdge {
                from: self.hub.0,
                to: channel.0,
                kind: EdgeKind::OffersDevice,
            });
        }
        graph
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if self.channel_index(device).is_some() {
            vec![
                capability(1, device, CapabilityKind::TriggerSink),
                capability(2, device, CapabilityKind::Dac),
            ]
        } else if device == self.hub {
            vec![capability(1, device, CapabilityKind::GenericCommand)]
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
                        description: format!("mightex sirius discovery read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("mightex sirius write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "mightex sirius remultiplexed state set".into(),
                        payload: Value::I64(set.writes.len() as i64),
                    });
                }
                Command::Invoke {
                    device,
                    capability,
                    request,
                } => {
                    let Some(capability) = self
                        .capabilities(*device)
                        .into_iter()
                        .find(|candidate| candidate.id == *capability)
                    else {
                        return Err(Error::new(
                            ErrorCode::Unsupported,
                            "unknown Mightex Sirius capability",
                        ));
                    };
                    if !capability.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            format!(
                                "Mightex Sirius {:?} expects {:?}, got {:?}",
                                capability.kind,
                                capability.preferred_request_kind(),
                                request.request_kind()
                            ),
                        ));
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("mightex sirius {:?}", capability.kind),
                        payload: Value::String(format!("{:?}", request.request_kind())),
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
        let result = (|| -> Result<Value> {
            for command in prepared.commands {
                match command {
                    Command::ReadProperty { device, key } => {
                        last = self.read_property_live(device, &key)?;
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
                        let Some(capability) = self
                            .capabilities(device)
                            .into_iter()
                            .find(|candidate| candidate.id == capability)
                        else {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unknown Mightex Sirius capability",
                            ));
                        };
                        last = self.invoke(device, capability.kind, &request)?;
                    }
                    Command::Arm(plan) => {
                        self.validate_timing_plan(&plan)?;
                        last = self.timing_summary(&plan, "arm");
                    }
                    Command::Start(_) | Command::Stop(_) => {}
                }
            }
            Ok(last)
        })();
        match result {
            Ok(value) => self
                .pending
                .push_back(DriverEvent::TokenCompleted { token, value }),
            Err(error) => self.pending.push_back(DriverEvent::TokenFailed {
                token,
                report: error.into(),
            }),
        }
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
                description: "mightex sirius timing arm summary".into(),
                payload: self.timing_summary(plan, "arm"),
            }],
        })
    }

    fn start_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let applied = self.apply_timing_sequence_step(&armed.plan, true)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Start(OperationId(0))],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "mightex sirius timing start sequence".into(),
                payload: Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "start")),
                    ("applied".into(), applied),
                ])),
            }],
        })
    }

    fn stop_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let applied = self.apply_timing_sequence_step(&armed.plan, false)?;
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions: vec![PhysicalTransaction {
                resource: Some(self.resource),
                description: "mightex sirius timing stop sequence".into(),
                payload: Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "stop")),
                    ("applied".into(), applied),
                ])),
            }],
        })
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        self.pending.drain(..).collect()
    }
}

fn parse_channel_count(product: &str) -> u8 {
    if let Some(count) = parse_source_style_channel_count(product) {
        return count;
    }
    product
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|part| part.parse::<u8>().ok())
        .find(|count| (1..=16).contains(count))
        .unwrap_or(4)
}

fn parse_module_type(product: &str) -> Option<String> {
    parse_source_style_module_type(product).or_else(|| {
        MODULE_CODES
            .iter()
            .find(|code| product.contains(**code))
            .map(|code| (*code).to_string())
    })
}

fn parse_source_style_channel_count(product: &str) -> Option<u8> {
    let token = source_style_product_token(product)?;
    let count_text = token.get(6..8)?;
    let count = count_text.parse::<u8>().ok()?;
    (1..=32).contains(&count).then_some(count)
}

fn parse_source_style_module_type(product: &str) -> Option<String> {
    let token = source_style_product_token(product)?;
    let code = token.get(4..6)?;
    MODULE_CODES
        .iter()
        .find(|candidate| **candidate == code)
        .map(|candidate| (*candidate).to_string())
}

fn source_style_product_token(product: &str) -> Option<&str> {
    let start = product.find("SL")?;
    product.get(start..)
}

fn sirius_reply_error(reply: &str) -> Option<String> {
    let trimmed = reply.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed.to_ascii_lowercase();
    let first = normalized
        .split(|c: char| c.is_ascii_whitespace() || matches!(c, ':' | ',' | ';'))
        .next()
        .unwrap_or("");
    matches!(
        first,
        "err" | "error" | "fail" | "failed" | "nak" | "nack" | "invalid"
    )
    .then(|| trimmed.to_string())
}

fn mode_values_for_family(family: MightexSiriusFamily) -> &'static [&'static str] {
    match family {
        MightexSiriusFamily::Bls => &["disabled", "normal", "trigger"],
        MightexSiriusFamily::Slc => &["disabled", "normal", "strobe", "trigger"],
    }
}

fn mode_name(mode: u8) -> &'static str {
    match mode {
        MODE_DISABLED => "disabled",
        MODE_NORMAL => "normal",
        MODE_STROBE => "strobe",
        MODE_TRIGGER => "trigger",
        _ => "unknown",
    }
}

fn parse_mode_name(family: MightexSiriusFamily, mode: &str) -> Result<u8> {
    let normalized = mode.trim().to_ascii_lowercase();
    let code = match normalized.as_str() {
        "disabled" | "disable" | "off" => MODE_DISABLED,
        "normal" | "on" => MODE_NORMAL,
        "strobe" if family == MightexSiriusFamily::Slc => MODE_STROBE,
        "trigger" => MODE_TRIGGER,
        _ => {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unsupported Mightex Sirius mode {mode}"),
            ))
        }
    };
    Ok(code)
}

fn mode_is_immediate(family: MightexSiriusFamily, mode: u8) -> bool {
    match family {
        MightexSiriusFamily::Bls => mode < MODE_TRIGGER,
        MightexSiriusFamily::Slc => mode < MODE_STROBE,
    }
}

fn parse_reply_integer(reply: &str, one_based_index: usize) -> Option<i64> {
    if one_based_index == 0 {
        return None;
    }
    reply
        .split(|c: char| !(c.is_ascii_digit() || c == '-' || c == '+' || c.is_ascii_alphabetic()))
        .filter_map(|token| token.parse::<i64>().ok())
        .nth(one_based_index - 1)
}

fn parse_reply_integers(reply: &str) -> Vec<i64> {
    reply
        .split(|c: char| !(c.is_ascii_digit() || c == '-' || c == '+'))
        .filter_map(|token| token.parse::<i64>().ok())
        .collect()
}

fn parse_raw_profile(reply: &str) -> Vec<RawProfileStep> {
    parse_reply_integers(reply)
        .chunks_exact(2)
        .take_while(|pair| pair[1] != 0)
        .map(|pair| RawProfileStep {
            current_raw: pair[0],
            time_raw: pair[1],
        })
        .collect()
}

fn raw_profile_value(profile: &[RawProfileStep]) -> Value {
    Value::List(
        profile
            .iter()
            .map(|step| {
                Value::Map(BTreeMap::from([
                    ("current_raw".into(), Value::I64(step.current_raw)),
                    ("time_raw".into(), Value::I64(step.time_raw)),
                ]))
            })
            .collect(),
    )
}

fn parse_slc_current_setpoint(reply: &str, module_type: &str) -> Option<i64> {
    let index = if matches!(module_type, "MA" | "CA") {
        8
    } else {
        12
    };
    parse_reply_integer(reply, index)
}

fn parse_slc_current_max(reply: &str, module_type: &str) -> Option<i64> {
    let index = if matches!(module_type, "MA" | "CA") {
        7
    } else {
        11
    };
    parse_reply_integer(reply, index)
}

fn diagnostic_sirius_command(
    _family: MightexSiriusFamily,
    command: &str,
) -> Result<protocol::SiriusCommand> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            "Mightex Sirius GenericCommand supports disable_all only",
        ));
    }
    match trimmed
        .to_ascii_lowercase()
        .replace(['-', '_'], "")
        .as_str()
    {
        "disableall" | "alloff" | "safeoff" => Ok(protocol::SiriusCommand::DisableAll),
        "reset" | "store" | "restoredef" | "restoredefault" | "restoredefaults" => Err(Error::new(
            ErrorCode::Unsupported,
            "Mightex Sirius reset/store/default-restore commands are hidden maintenance operations",
        )),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            "Mightex Sirius GenericCommand supports disable_all only",
        )),
    }
}

fn overdrive_tenths_percent(raw: i64) -> Result<Ratio> {
    Ok(Ratio::from_percent(nonnegative_f64(raw)? / 10.0))
}

fn nonnegative_f64(raw: i64) -> Result<f64> {
    if raw < 0 {
        return Err(Error::new(
            ErrorCode::Transport,
            "Mightex Sirius rule reply contained a negative value",
        ));
    }
    Ok(raw as f64)
}

fn family_label(family: MightexSiriusFamily) -> &'static str {
    match family {
        MightexSiriusFamily::Bls => "BLS",
        MightexSiriusFamily::Slc => "SLC",
    }
}

fn family_key(family: MightexSiriusFamily) -> &'static str {
    match family {
        MightexSiriusFamily::Bls => "bls",
        MightexSiriusFamily::Slc => "slc",
    }
}

fn property(
    key: impl Into<String>,
    display_name: impl Into<String>,
    value_type: ValueType,
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
        writable: false,
        volatile: false,
        sequenceable: false,
        hardware_address: None,
    }
}

fn writable_property(
    key: impl Into<String>,
    display_name: impl Into<String>,
    value_type: ValueType,
    unit: Option<&str>,
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
        writable: true,
        volatile: false,
        sequenceable: false,
        hardware_address: None,
    }
}

fn sequenceable_writable_property(
    key: impl Into<String>,
    display_name: impl Into<String>,
    value_type: ValueType,
    unit: Option<&str>,
) -> PropertySchema {
    let mut schema = writable_property(key, display_name, value_type, unit);
    schema.sequenceable = true;
    schema
}

fn volatile_property(
    key: impl Into<String>,
    display_name: impl Into<String>,
    value_type: ValueType,
    unit: Option<&str>,
) -> PropertySchema {
    let mut schema = property(key, display_name, value_type);
    schema.unit = unit.map(|unit| Unit(unit.into()));
    schema.volatile = true;
    schema
}

fn mode_property(values: &[&str]) -> PropertySchema {
    let mut schema = writable_property("mode", "Mode", ValueType::String, None);
    schema.enum_values = values
        .iter()
        .map(|value| EnumValue {
            value: Value::String((*value).into()),
            label: (*value).into(),
        })
        .collect();
    schema
}

fn trigger_program_property() -> PropertySchema {
    let mut schema = writable_property(
        "trigger_program",
        "Trigger program",
        ValueType::String,
        None,
    );
    schema.enum_values = ["pulse", "follow"]
        .iter()
        .map(|value| EnumValue {
            value: Value::String((*value).into()),
            label: (*value).into(),
        })
        .collect();
    schema
}

fn ranged_writable_property(
    key: impl Into<String>,
    display_name: impl Into<String>,
    value_type: ValueType,
    unit: Option<&str>,
    min: Value,
    max: Value,
) -> PropertySchema {
    let mut schema = writable_property(key, display_name, value_type, unit);
    schema.range = Some(Range { min, max });
    schema
}

fn ranged_sequenceable_writable_property(
    key: impl Into<String>,
    display_name: impl Into<String>,
    value_type: ValueType,
    unit: Option<&str>,
    min: Value,
    max: Value,
) -> PropertySchema {
    let mut schema = ranged_writable_property(key, display_name, value_type, unit, min, max);
    schema.sequenceable = true;
    schema
}

fn capability(id: u64, device: DeviceId, kind: CapabilityKind) -> CapabilityDescriptor {
    let response = match kind {
        CapabilityKind::Dac => ValueType::Ratio,
        CapabilityKind::TriggerSink => ValueType::Bool,
        CapabilityKind::GenericCommand => ValueType::Map,
        _ => ValueType::Map,
    };
    CapabilityDescriptor::new(CapabilityId(id), device, kind, response)
}

fn string_prop(device: &DeviceConfig, key: &str) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn optional_channel_count_prop(device: &DeviceConfig) -> Result<Option<u8>> {
    match device.properties.get("channel_count") {
        Some(Value::I64(value)) if (1..=32).contains(value) => Ok(Some(*value as u8)),
        Some(Value::I64(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            "Mightex Sirius channel_count must be in 1..=32",
        )),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            "Mightex Sirius channel_count must be an integer",
        )),
        None => Ok(None),
    }
}

fn optional_module_type_prop(device: &DeviceConfig) -> Result<Option<String>> {
    let Some(module_type) = string_prop(device, "module_type") else {
        return Ok(None);
    };
    let module_type = module_type.trim().to_ascii_uppercase();
    if MODULE_CODES.iter().any(|code| *code == module_type) {
        Ok(Some(module_type))
    } else {
        Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unsupported Mightex Sirius module_type {module_type}"),
        ))
    }
}

fn u16_prop(device: &DeviceConfig, key: &str) -> Result<u16> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if (0..=u16::MAX as i64).contains(value) => Ok(*value as u16),
        Some(Value::String(value)) => parse_u16_string(value).ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("Mightex Sirius config property {key} is not a valid u16"),
            )
        }),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Mightex Sirius config property {key} must be an integer or hex string"),
        )),
        None => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Mightex Sirius config missing property {key}"),
        )),
    }
}

fn parse_u16_string(value: &str) -> Option<u16> {
    let trimmed = value.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u16::from_str_radix(hex, 16).ok()
    } else {
        trimmed.parse().ok()
    }
}
