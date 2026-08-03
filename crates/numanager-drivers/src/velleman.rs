use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::usb::{ScriptedUsbPacket, UsbPacketIo};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};

/// USB vendor ids this driver claims. Hosts that need raw USB access
/// (udev rules on Linux) must cover these; see
/// `usb_discovery::builtin_usb_vendor_claims`.
pub fn usb_vendor_ids() -> Vec<u16> {
    vec![protocol::VELLEMAN_USB_VENDOR_ID]
}

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const K8055_PACKET_LEN: usize = 8;
    pub const K8061_PACKET_LEN: usize = 64;
    pub const K8055_CMD_RESET: u8 = 0x00;
    pub const K8055_CMD_DEBOUNCE_1_TIME: u8 = 0x01;
    pub const K8055_CMD_DEBOUNCE_2_TIME: u8 = 0x02;
    pub const K8055_CMD_RESET_COUNTER_1: u8 = 0x03;
    pub const K8055_CMD_RESET_COUNTER_2: u8 = 0x04;
    pub const K8055_CMD_WRITE_ANALOG_DIGITAL: u8 = 0x05;
    pub const K8055_DIGITAL_INPUT_REG: usize = 0x00;
    pub const K8055_DIGITAL_OUTPUT_REG: usize = 0x01;
    pub const K8055_ANALOG_OUTPUT_1_REG: usize = 0x02;
    pub const K8055_ANALOG_OUTPUT_2_REG: usize = 0x03;
    pub const K8055_ANALOG_INPUT_1_REG: usize = 0x02;
    pub const K8055_ANALOG_INPUT_2_REG: usize = 0x03;
    pub const K8055_COUNTER_1_REG: usize = 0x04;
    pub const K8055_COUNTER_2_REG: usize = 0x06;
    pub const K8061_CHANNEL_REG: usize = 0x01;
    pub const K8061_DIGITAL_INPUT_REG: usize = 0x01;
    pub const K8061_DIGITAL_OUTPUT_REG: usize = 0x01;
    pub const K8061_PWM_REG_1: usize = 0x01;
    pub const K8061_PWM_REG_2: usize = 0x02;
    pub const K8061_COUNTER_REG: usize = 0x02;
    pub const K8061_ANALOG_OUTPUT_REG: usize = 0x02;
    pub const K8061_ANALOG_INPUT_REG_1: usize = 0x02;
    pub const K8061_ANALOG_INPUT_REG_2: usize = 0x03;
    pub const K8061_CMD_READ_ANALOG_INPUT: u8 = 0x00;
    pub const K8061_CMD_SET_ANALOG_OUTPUT: u8 = 0x02;
    pub const K8061_CMD_OUTPUT_PWM: u8 = 0x04;
    pub const K8061_CMD_READ_DIGITAL_INPUT: u8 = 0x05;
    pub const K8061_CMD_CLEAR_DIGITAL_OUTPUT: u8 = 0x07;
    pub const K8061_CMD_SET_DIGITAL_OUTPUT: u8 = 0x08;
    pub const K8061_CMD_READ_COUNTER: u8 = 0x09;
    pub const K8061_CMD_RESET_COUNTER: u8 = 0x0a;
    pub const K8061_CMD_READ_DIGITAL_OUTPUT: u8 = 0x0e;
    pub const K8061_CMD_READ_ANALOG_OUTPUT: u8 = 0x0f;
    pub const K8061_CMD_READ_PWM: u8 = 0x10;
    pub const VELLEMAN_USB_VENDOR_ID: u16 = 0x10cf;
    pub const K8055_USB_PRODUCT_IDS: [u16; 4] = [0x5500, 0x5501, 0x5502, 0x5503];
    pub const K8061_USB_PRODUCT_IDS: [u16; 8] = [
        0x8061, 0x8062, 0x8063, 0x8064, 0x8065, 0x8066, 0x8067, 0x8068,
    ];

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum VellemanModel {
        K8055,
        K8061,
    }

    impl VellemanModel {
        pub fn label(self) -> &'static str {
            match self {
                VellemanModel::K8055 => "K8055/VM110",
                VellemanModel::K8061 => "K8061/VM140",
            }
        }

        pub fn packet_len(self) -> usize {
            match self {
                VellemanModel::K8055 => K8055_PACKET_LEN,
                VellemanModel::K8061 => K8061_PACKET_LEN,
            }
        }

        pub fn analog_input_count(self) -> usize {
            match self {
                VellemanModel::K8055 => 2,
                VellemanModel::K8061 => 8,
            }
        }

        pub fn analog_output_count(self) -> usize {
            match self {
                VellemanModel::K8055 => 2,
                VellemanModel::K8061 => 8,
            }
        }

        pub fn digital_input_count(self) -> i64 {
            match self {
                VellemanModel::K8055 => 5,
                VellemanModel::K8061 => 8,
            }
        }

        pub fn analog_input_max(self) -> u16 {
            match self {
                VellemanModel::K8055 => 0x00ff,
                VellemanModel::K8061 => 0x03ff,
            }
        }

        pub fn analog_output_max(self) -> u16 {
            0x00ff
        }

        pub fn supports_pwm(self) -> bool {
            matches!(self, VellemanModel::K8061)
        }

        pub fn counter_count(self) -> usize {
            2
        }

        pub fn counter_max(self) -> u16 {
            0xffff
        }

        pub fn supports_counter_debounce(self) -> bool {
            matches!(self, VellemanModel::K8055)
        }

        pub fn usb_endpoint_style(self) -> &'static str {
            match self {
                VellemanModel::K8055 => "interrupt",
                VellemanModel::K8061 => "bulk",
            }
        }

        pub fn command_summary(self) -> &'static str {
            match self {
                VellemanModel::K8055 => {
                    "DEBOUNCE_1=0x01, DEBOUNCE_2=0x02, WRITE_ANALOG_DIGITAL=0x05"
                }
                VellemanModel::K8061 => {
                    "RD_AI=0x00, SET_AO=0x02, OUT_PWM=0x04, RD_DI=0x05, CLR_DO=0x07, SET_DO=0x08, RD_CNT=0x09, RD_DO=0x0e, RD_AO=0x0f, RD_PWM=0x10"
                }
            }
        }
    }

    pub fn model_for_usb_product(product_id: u16) -> Option<VellemanModel> {
        if K8055_USB_PRODUCT_IDS.contains(&product_id) {
            Some(VellemanModel::K8055)
        } else if K8061_USB_PRODUCT_IDS.contains(&product_id) {
            Some(VellemanModel::K8061)
        } else {
            None
        }
    }

    pub fn board_address_for_usb_product(product_id: u16) -> Option<u8> {
        if let Some(index) = K8055_USB_PRODUCT_IDS
            .iter()
            .position(|candidate| *candidate == product_id)
        {
            return Some(index as u8);
        }
        K8061_USB_PRODUCT_IDS
            .iter()
            .position(|candidate| *candidate == product_id)
            .map(|index| index as u8)
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct VellemanProbe {
        pub model: VellemanModel,
        pub serial_number: String,
        pub board_address: u8,
        pub digital_output_mask: u8,
        pub digital_input_mask: u8,
        pub analog_outputs: Vec<u16>,
        pub analog_inputs: Vec<u16>,
        pub counters: Vec<u16>,
        pub counter_debounce_ms: Vec<u16>,
        pub pwm_output: Option<u16>,
    }

    impl VellemanProbe {
        pub fn configured_fixture() -> Self {
            Self {
                model: VellemanModel::K8055,
                serial_number: "K8055-CONFIG-0001".into(),
                board_address: 0,
                digital_output_mask: 0,
                digital_input_mask: 0,
                analog_outputs: vec![0, 0],
                analog_inputs: vec![127, 127],
                counters: vec![0, 0],
                counter_debounce_ms: vec![2, 2],
                pwm_output: None,
            }
        }

        pub fn configured_k8061_fixture() -> Self {
            Self {
                model: VellemanModel::K8061,
                serial_number: "K8061-CONFIG-0001".into(),
                board_address: 0,
                digital_output_mask: 0,
                digital_input_mask: 0,
                analog_outputs: vec![0; 8],
                analog_inputs: vec![511; 8],
                counters: vec![0, 0],
                counter_debounce_ms: Vec::new(),
                pwm_output: Some(0),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum VellemanCommand {
        Reset,
        WriteAnalogDigital {
            digital_output_mask: u8,
            analog_outputs: Vec<u16>,
        },
        SetK8055CounterDebounce {
            channel: usize,
            debounce_ms: u16,
        },
        ResetK8055Counter {
            channel: usize,
        },
        ReadK8061AnalogInput {
            channel: usize,
        },
        ReadK8061Counter,
        ResetK8061Counter,
        WriteK8061AnalogOutput {
            channel: usize,
            value: u16,
        },
        ReadK8061DigitalInput,
        SetK8061DigitalOutputBit {
            channel: usize,
            enabled: bool,
        },
        WriteK8061Pwm {
            value: u16,
        },
    }

    pub fn encode_k8055(command: &VellemanCommand) -> [u8; K8055_PACKET_LEN] {
        let mut packet = [0_u8; K8055_PACKET_LEN];
        match command {
            VellemanCommand::Reset => {
                packet[0] = K8055_CMD_RESET;
            }
            VellemanCommand::WriteAnalogDigital {
                digital_output_mask,
                analog_outputs,
            } => {
                packet[0] = K8055_CMD_WRITE_ANALOG_DIGITAL;
                packet[K8055_DIGITAL_OUTPUT_REG] = *digital_output_mask;
                packet[K8055_ANALOG_OUTPUT_1_REG] = analog_outputs[0] as u8;
                packet[K8055_ANALOG_OUTPUT_2_REG] = analog_outputs[1] as u8;
            }
            VellemanCommand::SetK8055CounterDebounce {
                channel,
                debounce_ms,
            } => {
                packet[0] = if *channel == 0 {
                    K8055_CMD_DEBOUNCE_1_TIME
                } else {
                    K8055_CMD_DEBOUNCE_2_TIME
                };
                packet[6 + channel] = debounce_ms_to_register(*debounce_ms);
            }
            VellemanCommand::ResetK8055Counter { channel } => {
                packet[0] = if *channel == 0 {
                    K8055_CMD_RESET_COUNTER_1
                } else {
                    K8055_CMD_RESET_COUNTER_2
                };
                packet[if *channel == 0 {
                    K8055_COUNTER_1_REG
                } else {
                    K8055_COUNTER_2_REG
                }] = 0;
            }
            _ => {}
        }
        packet
    }

    pub fn encode_k8061(command: &VellemanCommand) -> [u8; K8061_PACKET_LEN] {
        let mut packet = [0_u8; K8061_PACKET_LEN];
        match command {
            VellemanCommand::ReadK8061AnalogInput { channel } => {
                packet[0] = K8061_CMD_READ_ANALOG_INPUT;
                packet[K8061_CHANNEL_REG] = *channel as u8;
            }
            VellemanCommand::WriteK8061AnalogOutput { channel, value } => {
                packet[0] = K8061_CMD_SET_ANALOG_OUTPUT;
                packet[K8061_CHANNEL_REG] = *channel as u8;
                packet[K8061_ANALOG_OUTPUT_REG] = *value as u8;
            }
            VellemanCommand::ReadK8061DigitalInput => {
                packet[0] = K8061_CMD_READ_DIGITAL_INPUT;
            }
            VellemanCommand::ReadK8061Counter => {
                packet[0] = K8061_CMD_READ_COUNTER;
            }
            VellemanCommand::ResetK8061Counter => {
                packet[0] = K8061_CMD_RESET_COUNTER;
            }
            VellemanCommand::SetK8061DigitalOutputBit { channel, enabled } => {
                let bit = 1_u8 << channel;
                if *enabled {
                    packet[0] = K8061_CMD_SET_DIGITAL_OUTPUT;
                    packet[K8061_DIGITAL_OUTPUT_REG] = bit;
                } else {
                    packet[0] = K8061_CMD_CLEAR_DIGITAL_OUTPUT;
                    packet[K8061_DIGITAL_OUTPUT_REG] = !bit;
                }
            }
            VellemanCommand::WriteK8061Pwm { value } => {
                packet[0] = K8061_CMD_OUTPUT_PWM;
                packet[K8061_PWM_REG_1] = (*value & 0x03) as u8;
                packet[K8061_PWM_REG_2] = ((*value >> 2) & 0xff) as u8;
            }
            _ => {}
        }
        packet
    }

    pub fn parse_k8055_input(packet: &[u8]) -> Result<K8055Input> {
        if packet.len() < K8055_PACKET_LEN {
            return Err(Error::new(
                ErrorCode::Transport,
                "K8055 input packet must be at least 8 bytes",
            ));
        }
        Ok(K8055Input {
            digital_input_mask: decode_k8055_digital_inputs(packet[K8055_DIGITAL_INPUT_REG]),
            analog_inputs: [
                packet[K8055_ANALOG_INPUT_1_REG],
                packet[K8055_ANALOG_INPUT_2_REG],
            ],
            counters: [
                u16::from_le_bytes([packet[K8055_COUNTER_1_REG], packet[K8055_COUNTER_1_REG + 1]]),
                u16::from_le_bytes([packet[K8055_COUNTER_2_REG], packet[K8055_COUNTER_2_REG + 1]]),
            ],
        })
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct K8055Input {
        pub digital_input_mask: u8,
        pub analog_inputs: [u8; 2],
        pub counters: [u16; 2],
    }

    pub fn decode_k8055_digital_inputs(raw: u8) -> u8 {
        ((raw >> 4) & 0x03) | ((raw << 2) & 0x04) | ((raw >> 3) & 0x18)
    }

    pub fn ratio_to_u8(value: Ratio) -> Result<u8> {
        let percent = value.percent();
        if !(0.0..=100.0).contains(&percent) || !percent.is_finite() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "K8055 analog output ratio must be in 0..=100 percent",
            ));
        }
        Ok((percent * 255.0 / 100.0).round() as u8)
    }

    pub fn u8_to_ratio(value: u8) -> Ratio {
        Ratio::from_percent(value as f64 * 100.0 / 255.0)
    }

    pub fn ratio_to_count(value: Ratio, max: u16, label: &str) -> Result<u16> {
        let percent = value.percent();
        if !(0.0..=100.0).contains(&percent) || !percent.is_finite() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("{label} ratio must be in 0..=100 percent"),
            ));
        }
        Ok((percent * max as f64 / 100.0).round() as u16)
    }

    pub fn count_to_ratio(value: u16, max: u16) -> Ratio {
        Ratio::from_percent(value as f64 * 100.0 / max as f64)
    }

    pub fn debounce_ms_to_register(value: u16) -> u8 {
        let debounce_ms = value.clamp(1, 7450) as u32;
        let target = debounce_ms * 1000 / 115;
        let mut register = (target as f64).sqrt() as u32;
        if (register + 1) * register < target {
            register += 1;
        }
        register.min(u8::MAX as u32) as u8
    }
}

#[derive(Debug, Clone)]
pub struct VellemanConfiguredProbe {
    label: String,
    connect_real_transport: bool,
    endpoint: Option<VellemanUsbEndpoint>,
    usb_identity: Option<VellemanUsbIdentity>,
    probe: protocol::VellemanProbe,
}

#[derive(Debug, Clone)]
pub struct VellemanUsbEndpoint {
    vendor_id: u16,
    product_id: u16,
    interface: u8,
    out_endpoint: u8,
    in_endpoint: u8,
    transfer_kind: UsbTransferKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VellemanUsbIdentity {
    vendor_id: u16,
    product_id: u16,
    product: Option<String>,
    serial_number: Option<String>,
    bus: Option<u8>,
    address: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbTransferKind {
    Bulk,
    Interrupt,
}

impl UsbTransferKind {
    fn from_config(value: Option<String>, model: protocol::VellemanModel) -> Result<Self> {
        let Some(value) = value else {
            return Ok(match model {
                protocol::VellemanModel::K8055 => Self::Interrupt,
                protocol::VellemanModel::K8061 => Self::Bulk,
            });
        };
        match value.to_ascii_lowercase().as_str() {
            "bulk" => Ok(Self::Bulk),
            "interrupt" => Ok(Self::Interrupt),
            other => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Velleman USB transfer kind {other}"),
            )),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Bulk => "bulk",
            Self::Interrupt => "interrupt",
        }
    }
}

pub struct VellemanDiscovery {
    next_id: DriverId,
    probes: Vec<VellemanConfiguredProbe>,
    #[cfg(feature = "os-usb")]
    active_usb: bool,
}

impl VellemanDiscovery {
    pub fn configured_fixture(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![VellemanConfiguredProbe::fixture()],
            #[cfg(feature = "os-usb")]
            active_usb: false,
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| {
                matches!(
                    device.driver.as_str(),
                    "velleman" | "k8055" | "vm110" | "k8061" | "vm140"
                )
            })
            .map(VellemanConfiguredProbe::from_device_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            next_id,
            probes,
            #[cfg(feature = "os-usb")]
            active_usb: false,
        })
    }

    #[cfg(feature = "os-usb")]
    pub fn os_usb(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: Vec::new(),
            active_usb: true,
        }
    }
}

impl DriverDiscovery for VellemanDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        let mut probes = std::mem::take(&mut self.probes);
        #[cfg(feature = "os-usb")]
        if self.active_usb {
            probes.extend(active_usb_probes()?);
        }
        probes
            .drain(..)
            .enumerate()
            .map(|(index, configured)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = configured.label.clone();
                let driver: Box<dyn Driver> = if configured.connect_real_transport {
                    Box::new(VellemanDriver::usb(id, configured)?)
                } else {
                    Box::new(VellemanDriver::configured(id, configured))
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

impl VellemanConfiguredProbe {
    pub fn fixture() -> Self {
        Self {
            label: "Configured Velleman K8055 fixture".into(),
            connect_real_transport: false,
            endpoint: None,
            usb_identity: None,
            probe: protocol::VellemanProbe::configured_fixture(),
        }
    }

    fn from_device_config(device: &DeviceConfig) -> Result<Self> {
        let mut configured = match configured_model(device)? {
            protocol::VellemanModel::K8055 => Self::fixture(),
            protocol::VellemanModel::K8061 => Self {
                label: "Configured Velleman K8061 fixture".into(),
                connect_real_transport: false,
                endpoint: None,
                usb_identity: None,
                probe: protocol::VellemanProbe::configured_k8061_fixture(),
            },
        };
        configured.label = if device.label.is_empty() {
            configured.label
        } else {
            device.label.clone()
        };
        configured.probe.serial_number = string_prop(device, "serial_number")
            .unwrap_or_else(|| configured.probe.serial_number.clone());
        configured.probe.board_address =
            u8_prop(device, "board_address")?.unwrap_or(configured.probe.board_address);
        configured.probe.digital_output_mask =
            u8_prop(device, "digital_output_mask")?.unwrap_or(configured.probe.digital_output_mask);
        configured.probe.digital_input_mask =
            u8_prop(device, "digital_input_mask")?.unwrap_or(configured.probe.digital_input_mask);
        for index in 0..configured.probe.model.analog_output_count() {
            let key = format!("analog_output_{}", index + 1);
            if let Some(ratio) = ratio_prop(device, &key) {
                configured.probe.analog_outputs[index] = protocol::ratio_to_count(
                    ratio,
                    configured.probe.model.analog_output_max(),
                    &key,
                )?;
            }
        }
        for index in 0..configured.probe.model.analog_input_count() {
            let key = format!("analog_input_{}", index + 1);
            if let Some(ratio) = ratio_prop(device, &key) {
                configured.probe.analog_inputs[index] = protocol::ratio_to_count(
                    ratio,
                    configured.probe.model.analog_input_max(),
                    &key,
                )?;
            }
        }
        for index in 0..configured.probe.model.counter_count() {
            let key = format!("counter_{}", index + 1);
            if let Some(value) = u16_prop(device, &key)? {
                configured.probe.counters[index] = value;
            }
        }
        if configured.probe.model.supports_counter_debounce() {
            for index in 0..configured.probe.counter_debounce_ms.len() {
                let key = format!("counter_{}_debounce", index + 1);
                if let Some(value) = interval_ms_prop(device, &key)? {
                    configured.probe.counter_debounce_ms[index] = value.clamp(1, 7450);
                }
            }
        }
        if let Some(ratio) = ratio_prop(device, "pwm_output") {
            if configured.probe.model.supports_pwm() {
                configured.probe.pwm_output =
                    Some(protocol::ratio_to_count(ratio, 0x03ff, "pwm_output")?);
            }
        }
        configured.connect_real_transport = bool_prop(device, "connect").unwrap_or(false);
        configured.endpoint = velleman_endpoint_from_config(device, configured.probe.model)?;
        if configured.connect_real_transport && configured.endpoint.is_none() {
            #[cfg(feature = "os-usb")]
            {
                configured.endpoint = autodiscover_velleman_endpoint(device, &configured.probe)?;
            }
        }
        if configured.connect_real_transport && configured.endpoint.is_none() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                "Velleman connect=true requires explicit endpoint metadata or an auto-discoverable known Velleman USB device",
            ));
        }
        let max_board_address = match configured.probe.model {
            protocol::VellemanModel::K8055 => 3,
            protocol::VellemanModel::K8061 => 7,
        };
        if configured.probe.board_address > max_board_address {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!(
                    "{} board_address must be in 0..={max_board_address}",
                    configured.probe.model.label()
                ),
            ));
        }
        Ok(configured)
    }
}

#[cfg(feature = "os-usb")]
fn active_usb_probes() -> Result<Vec<VellemanConfiguredProbe>> {
    let devices = nusb::list_devices().map_err(|error| {
        Error::new(
            ErrorCode::Transport,
            format!("Velleman USB device listing failed: {error}"),
        )
    })?;
    Ok(devices
        .filter(|device| device.vendor_id() == protocol::VELLEMAN_USB_VENDOR_ID)
        .filter_map(|device| {
            let product_id = device.product_id();
            let model = protocol::model_for_usb_product(product_id)?;
            let board_address = protocol::board_address_for_usb_product(product_id).unwrap_or(0);
            let product = device
                .product_string()
                .map(str::to_string)
                .unwrap_or_else(|| model.label().into());
            let serial_number = device.serial_number().map(str::to_string);
            let label = format!(
                "Velleman {} {:04x}:{:04x} bus {} addr {}",
                model.label(),
                protocol::VELLEMAN_USB_VENDOR_ID,
                product_id,
                device.bus_number(),
                device.device_address()
            );
            let mut probe = match model {
                protocol::VellemanModel::K8055 => protocol::VellemanProbe::configured_fixture(),
                protocol::VellemanModel::K8061 => {
                    protocol::VellemanProbe::configured_k8061_fixture()
                }
            };
            probe.serial_number = serial_number
                .clone()
                .unwrap_or_else(|| format!("VELLEMAN-{:04X}", product_id));
            probe.board_address = board_address;
            Some(VellemanConfiguredProbe {
                label,
                connect_real_transport: false,
                endpoint: None,
                usb_identity: Some(VellemanUsbIdentity {
                    vendor_id: protocol::VELLEMAN_USB_VENDOR_ID,
                    product_id,
                    product: Some(product),
                    serial_number,
                    bus: Some(device.bus_number()),
                    address: Some(device.device_address()),
                }),
                probe,
            })
        })
        .collect())
}

#[cfg(feature = "os-usb")]
fn autodiscover_velleman_endpoint(
    device_config: &DeviceConfig,
    probe: &protocol::VellemanProbe,
) -> Result<Option<VellemanUsbEndpoint>> {
    let vendor_id =
        u16_prop(device_config, "vendor_id")?.unwrap_or(protocol::VELLEMAN_USB_VENDOR_ID);
    let product_id = match u16_prop(device_config, "product_id")? {
        Some(product_id) => product_id,
        None => match probe.model {
            protocol::VellemanModel::K8055 => protocol::K8055_USB_PRODUCT_IDS
                .get(probe.board_address as usize)
                .copied(),
            protocol::VellemanModel::K8061 => protocol::K8061_USB_PRODUCT_IDS
                .get(probe.board_address as usize)
                .copied(),
        }
        .ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Velleman board_address does not map to a known USB product id",
            )
        })?,
    };
    let expected_model = protocol::model_for_usb_product(product_id).ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidProperty,
            format!("Velleman product_id 0x{product_id:04x} is not a known K8055/K8061 id"),
        )
    })?;
    if expected_model != probe.model {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "Velleman product_id does not match configured model",
        ));
    }
    let serial_filter = string_prop(device_config, "serial_number");
    let mut matches = nusb::list_devices()
        .map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("Velleman USB device listing failed: {error}"),
            )
        })?
        .filter(|device| device.vendor_id() == vendor_id && device.product_id() == product_id)
        .filter(|device| {
            serial_filter
                .as_ref()
                .is_none_or(|serial| device.serial_number() == Some(serial.as_str()))
        })
        .collect::<Vec<_>>();
    match matches.len() {
        0 => Ok(None),
        1 => discover_velleman_endpoint(&matches.remove(0), probe.model).map(Some),
        count => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!(
                "Velleman endpoint autodiscovery found {count} matching USB devices; configure serial_number or explicit endpoints"
            ),
        )),
    }
}

#[cfg(feature = "os-usb")]
fn discover_velleman_endpoint(
    device: &nusb::DeviceInfo,
    model: protocol::VellemanModel,
) -> Result<VellemanUsbEndpoint> {
    let opened = device.open().map_err(|error| {
        Error::new(
            ErrorCode::Transport,
            format!(
                "open Velleman {:04x}:{:04x} for descriptor endpoint discovery failed: {error}",
                device.vendor_id(),
                device.product_id()
            ),
        )
    })?;
    let configuration = opened.active_configuration().map_err(|error| {
        Error::new(
            ErrorCode::Transport,
            format!("read Velleman active USB configuration failed: {error}"),
        )
    })?;
    let transfer_kind = UsbTransferKind::from_config(None, model)?;
    let expected_transfer = match transfer_kind {
        UsbTransferKind::Bulk => nusb::transfer::EndpointType::Bulk,
        UsbTransferKind::Interrupt => nusb::transfer::EndpointType::Interrupt,
    };
    let mut candidates = Vec::new();
    for interface in configuration.interface_alt_settings() {
        if interface.alternate_setting() != 0 {
            continue;
        }
        let mut out_endpoint = None;
        let mut in_endpoint = None;
        for endpoint in interface.endpoints() {
            if endpoint.transfer_type() != expected_transfer {
                continue;
            }
            match endpoint.direction() {
                nusb::transfer::Direction::Out => {
                    if out_endpoint.is_none() {
                        out_endpoint = Some(endpoint.address());
                    }
                }
                nusb::transfer::Direction::In => {
                    if in_endpoint.is_none() {
                        in_endpoint = Some(endpoint.address());
                    }
                }
            }
        }
        if let (Some(out_endpoint), Some(in_endpoint)) = (out_endpoint, in_endpoint) {
            candidates.push(VellemanUsbEndpoint {
                vendor_id: device.vendor_id(),
                product_id: device.product_id(),
                interface: interface.interface_number(),
                out_endpoint,
                in_endpoint,
                transfer_kind,
            });
        }
    }
    match candidates.len() {
        1 => Ok(candidates.remove(0)),
        0 => Err(Error::new(
            ErrorCode::InvalidProperty,
            "Velleman endpoint autodiscovery found no matching IN/OUT endpoint pair",
        )),
        count => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!(
                "Velleman endpoint autodiscovery found {count} matching endpoint pairs; configure explicit endpoints"
            ),
        )),
    }
}

pub struct VellemanDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    digital_input: DeviceId,
    digital_output: DeviceId,
    counters: Vec<DeviceId>,
    analog_inputs: Vec<DeviceId>,
    analog_outputs: Vec<DeviceId>,
    pwm_output: Option<DeviceId>,
    probe: protocol::VellemanProbe,
    io: Box<dyn UsbPacketIo>,
    configured_endpoint: Option<VellemanUsbEndpoint>,
    live_endpoint: Option<VellemanUsbEndpoint>,
    usb_identity: Option<VellemanUsbIdentity>,
    last_transaction: Value,
    next_token: u64,
    pending: VecDeque<DriverEvent>,
}

impl VellemanDriver {
    pub fn configured(id: DriverId, configured: VellemanConfiguredProbe) -> Self {
        Self::new(
            id,
            configured.probe,
            Box::new(ScriptedUsbPacket::new()),
            configured.endpoint,
            None,
            configured.usb_identity,
        )
    }

    #[cfg(feature = "os-usb")]
    pub fn usb(id: DriverId, configured: VellemanConfiguredProbe) -> Result<Self> {
        let endpoint = configured.endpoint.clone().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Velleman USB endpoint metadata is required for connect=true",
            )
        })?;
        let io = Box::new(live_velleman::LiveVellemanUsb::open(&endpoint)?);
        Ok(Self::new(
            id,
            configured.probe,
            io,
            Some(endpoint.clone()),
            Some(endpoint),
            configured.usb_identity,
        ))
    }

    #[cfg(not(feature = "os-usb"))]
    pub fn usb(_id: DriverId, _configured: VellemanConfiguredProbe) -> Result<Self> {
        Err(Error::new(
            ErrorCode::Unsupported,
            "Velleman real USB transport requires the numanager-drivers os-usb feature",
        ))
    }

    fn new(
        id: DriverId,
        probe: protocol::VellemanProbe,
        io: Box<dyn UsbPacketIo>,
        configured_endpoint: Option<VellemanUsbEndpoint>,
        live_endpoint: Option<VellemanUsbEndpoint>,
        usb_identity: Option<VellemanUsbIdentity>,
    ) -> Self {
        let analog_inputs = (0..probe.model.analog_input_count())
            .map(|index| DeviceId(NodeId(id.0 * 1000 + 974 + index as u64)))
            .collect::<Vec<_>>();
        let counters = (0..probe.model.counter_count())
            .map(|index| DeviceId(NodeId(id.0 * 1000 + 982 + index as u64)))
            .collect::<Vec<_>>();
        let analog_outputs = (0..probe.model.analog_output_count())
            .map(|index| DeviceId(NodeId(id.0 * 1000 + 990 + index as u64)))
            .collect::<Vec<_>>();
        let pwm_output = probe
            .model
            .supports_pwm()
            .then_some(DeviceId(NodeId(id.0 * 1000 + 989)));
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 970)),
            hub: DeviceId(NodeId(id.0 * 1000 + 971)),
            digital_input: DeviceId(NodeId(id.0 * 1000 + 972)),
            digital_output: DeviceId(NodeId(id.0 * 1000 + 973)),
            counters,
            analog_inputs,
            analog_outputs,
            pwm_output,
            probe,
            io,
            configured_endpoint,
            live_endpoint,
            usb_identity,
            last_transaction: Value::Map(BTreeMap::new()),
            next_token: 1,
            pending: VecDeque::new(),
        }
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn write_outputs(&mut self, previous_digital_output_mask: u8) -> Result<()> {
        match self.probe.model {
            protocol::VellemanModel::K8055 => {
                let command = protocol::VellemanCommand::WriteAnalogDigital {
                    digital_output_mask: self.probe.digital_output_mask,
                    analog_outputs: self.probe.analog_outputs.clone(),
                };
                let packet = protocol::encode_k8055(&command);
                self.io.write_packet(&packet)?;
            }
            protocol::VellemanModel::K8061 => {
                self.write_k8061_digital_output_bits(previous_digital_output_mask)?;
                self.refresh_k8061_digital_output()?;
            }
        }
        let completion_basis = if self.probe.model == protocol::VellemanModel::K8061 {
            "usb_readback"
        } else {
            "usb_write"
        };
        self.last_transaction = self.output_transaction("write_outputs", completion_basis);
        Ok(())
    }

    fn write_k8061_digital_output_bits(&mut self, previous_mask: u8) -> Result<()> {
        let changed = previous_mask ^ self.probe.digital_output_mask;
        for channel in 0..8 {
            let bit = 1_u8 << channel;
            if changed & bit == 0 {
                continue;
            }
            let command = protocol::VellemanCommand::SetK8061DigitalOutputBit {
                channel,
                enabled: self.probe.digital_output_mask & bit != 0,
            };
            let packet = protocol::encode_k8061(&command);
            self.io.write_packet(&packet)?;
        }
        Ok(())
    }

    fn refresh_inputs(&mut self) -> Result<()> {
        match self.probe.model {
            protocol::VellemanModel::K8055 => {
                let mut packet = self.io.read_packet(protocol::K8055_PACKET_LEN)?;
                if self.live_endpoint.is_none() && packet.iter().all(|byte| *byte == 0) {
                    packet[protocol::K8055_DIGITAL_INPUT_REG] =
                        encode_fixture_digital_inputs(self.probe.digital_input_mask);
                    packet[protocol::K8055_ANALOG_INPUT_1_REG] = self.probe.analog_inputs[0] as u8;
                    packet[protocol::K8055_ANALOG_INPUT_2_REG] = self.probe.analog_inputs[1] as u8;
                    let counter_1 = self.probe.counters[0].to_le_bytes();
                    let counter_2 = self.probe.counters[1].to_le_bytes();
                    packet[protocol::K8055_COUNTER_1_REG] = counter_1[0];
                    packet[protocol::K8055_COUNTER_1_REG + 1] = counter_1[1];
                    packet[protocol::K8055_COUNTER_2_REG] = counter_2[0];
                    packet[protocol::K8055_COUNTER_2_REG + 1] = counter_2[1];
                }
                let input = protocol::parse_k8055_input(&packet)?;
                self.probe.digital_input_mask = input.digital_input_mask;
                self.probe.analog_inputs = input
                    .analog_inputs
                    .iter()
                    .map(|value| *value as u16)
                    .collect();
                self.probe.counters = input.counters.into_iter().collect();
            }
            protocol::VellemanModel::K8061 => {
                let command = protocol::VellemanCommand::ReadK8061DigitalInput;
                let packet = protocol::encode_k8061(&command);
                self.io.write_packet(&packet)?;
                let mut packet = self.io.read_packet(protocol::K8061_PACKET_LEN)?;
                if self.live_endpoint.is_none() && packet.iter().all(|byte| *byte == 0) {
                    packet[protocol::K8061_DIGITAL_INPUT_REG] = self.probe.digital_input_mask;
                }
                self.probe.digital_input_mask = packet[protocol::K8061_DIGITAL_INPUT_REG];
                for index in 0..self.probe.analog_inputs.len() {
                    let command =
                        protocol::VellemanCommand::ReadK8061AnalogInput { channel: index };
                    let request = protocol::encode_k8061(&command);
                    self.io.write_packet(&request)?;
                    let mut packet = self.io.read_packet(protocol::K8061_PACKET_LEN)?;
                    if self.live_endpoint.is_none() && packet.iter().all(|byte| *byte == 0) {
                        let value = self.probe.analog_inputs[index];
                        packet[protocol::K8061_ANALOG_INPUT_REG_1] = (value & 0xff) as u8;
                        packet[protocol::K8061_ANALOG_INPUT_REG_2] = (value >> 8) as u8;
                    }
                    self.probe.analog_inputs[index] = packet[protocol::K8061_ANALOG_INPUT_REG_1]
                        as u16
                        + 256 * packet[protocol::K8061_ANALOG_INPUT_REG_2] as u16;
                }
            }
        }
        self.last_transaction = self.input_transaction("read_inputs", "usb_packet");
        Ok(())
    }

    fn refresh_counter(&mut self, index: usize) -> Result<()> {
        match self.probe.model {
            protocol::VellemanModel::K8055 => {
                self.refresh_inputs()?;
            }
            protocol::VellemanModel::K8061 => {
                let command = protocol::VellemanCommand::ReadK8061Counter;
                let request = protocol::encode_k8061(&command);
                self.io.write_packet(&request)?;
                let mut packet = self.io.read_packet(protocol::K8061_PACKET_LEN)?;
                if self.live_endpoint.is_none() && packet.iter().all(|byte| *byte == 0) {
                    let value = self.probe.counters[index];
                    let base = protocol::K8061_COUNTER_REG * (index + 1) + 1;
                    packet[base] = (value & 0xff) as u8;
                    packet[protocol::K8061_COUNTER_REG * 2 + 2] = (value >> 8) as u8;
                }
                let base = protocol::K8061_COUNTER_REG * (index + 1) + 1;
                self.probe.counters[index] =
                    packet[base] as u16 + 256 * packet[protocol::K8061_COUNTER_REG * 2 + 2] as u16;
                self.last_transaction = self.counter_transaction("read_counter", index);
            }
        }
        Ok(())
    }

    fn write_counter_debounce(&mut self, index: usize, interval: TimeInterval) -> Result<Value> {
        if self.probe.model != protocol::VellemanModel::K8055 {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "only K8055 counters expose debounce writes",
            ));
        }
        let debounce_ms = interval_to_debounce_ms(interval)?;
        let command = protocol::VellemanCommand::SetK8055CounterDebounce {
            channel: index,
            debounce_ms,
        };
        let packet = protocol::encode_k8055(&command);
        self.io.write_packet(&packet)?;
        self.probe.counter_debounce_ms[index] = debounce_ms;
        let value = Value::TimeInterval(TimeInterval::from_milliseconds(debounce_ms as f64));
        self.last_transaction = self.counter_transaction("set_counter_debounce", index);
        self.emit_property(self.counters[index], "debounce", value.clone());
        Ok(value)
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device == self.hub {
            return match key {
                "model" => Ok(Value::String(self.probe.model.label().into())),
                "serial_number" => Ok(Value::String(self.probe.serial_number.clone())),
                "board_address" => Ok(Value::I64(self.probe.board_address as i64)),
                "protocol" => Ok(Value::String(format!(
                    "Velleman {} packet protocol",
                    self.probe.model.label()
                ))),
                "packet_len" => Ok(Value::I64(self.probe.model.packet_len() as i64)),
                "usb_endpoint_style" => {
                    Ok(Value::String(self.probe.model.usb_endpoint_style().into()))
                }
                "packet_backend" => Ok(Value::String(self.packet_backend_label())),
                "connected" => Ok(Value::Bool(self.live_endpoint.is_some())),
                "usb_endpoint" => Ok(Value::String(self.usb_endpoint_label())),
                "usb_identity" => Ok(self.usb_identity_map()),
                "command_summary" => Ok(Value::String(self.probe.model.command_summary().into())),
                "last_transaction" => Ok(self.last_transaction.clone()),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown Velleman hub property {key}"),
                )),
            };
        }
        if device == self.digital_input {
            return match key {
                "mask" => Ok(Value::I64(self.probe.digital_input_mask as i64)),
                "input_count" => Ok(Value::I64(self.probe.model.digital_input_count())),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown Velleman digital input property {key}"),
                )),
            };
        }
        if device == self.digital_output {
            return match key {
                "mask" => Ok(Value::I64(self.probe.digital_output_mask as i64)),
                "output_count" => Ok(Value::I64(8)),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown Velleman digital output property {key}"),
                )),
            };
        }
        if let Some(index) = self.counter_index(device) {
            return match key {
                "count" => Ok(Value::I64(self.probe.counters[index] as i64)),
                "max_count" => Ok(Value::I64(self.probe.model.counter_max() as i64)),
                "debounce" if self.probe.model.supports_counter_debounce() => {
                    Ok(Value::TimeInterval(TimeInterval::from_milliseconds(
                        self.probe.counter_debounce_ms[index] as f64,
                    )))
                }
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown Velleman counter property {key}"),
                )),
            };
        }
        if let Some(index) = self.analog_input_index(device) {
            return match key {
                "value" => Ok(Value::Ratio(protocol::count_to_ratio(
                    self.probe.analog_inputs[index],
                    self.probe.model.analog_input_max(),
                ))),
                "resolution" => Ok(Value::I64(
                    if self.probe.model == protocol::VellemanModel::K8061 {
                        10
                    } else {
                        8
                    },
                )),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown Velleman analog input property {key}"),
                )),
            };
        }
        if let Some(index) = self.analog_output_index(device) {
            return match key {
                "value" => Ok(Value::Ratio(protocol::count_to_ratio(
                    self.probe.analog_outputs[index],
                    self.probe.model.analog_output_max(),
                ))),
                "resolution" => Ok(Value::I64(8)),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown Velleman analog output property {key}"),
                )),
            };
        }
        if Some(device) == self.pwm_output {
            return match key {
                "value" => Ok(Value::Ratio(protocol::count_to_ratio(
                    self.probe.pwm_output.unwrap_or(0),
                    0x03ff,
                ))),
                "resolution" => Ok(Value::I64(10)),
                "frequency" => Ok(Value::Frequency(Frequency::from_hertz(15_600.0))),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown Velleman PWM property {key}"),
                )),
            };
        }
        Err(Error::new(
            ErrorCode::InvalidProperty,
            "unknown Velleman device",
        ))
    }

    fn validate_write(&self, device: DeviceId, key: &str, value: &Value) -> Result<()> {
        if device == self.digital_output && key == "mask" {
            return match value {
                Value::I64(mask) if (0..=255).contains(mask) => Ok(()),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Velleman digital output mask must be in 0..=255",
                )),
            };
        }
        if self.analog_output_index(device).is_some() && key == "value" {
            return match value {
                Value::Ratio(ratio) => protocol::ratio_to_count(
                    *ratio,
                    self.probe.model.analog_output_max(),
                    "analog output value",
                )
                .map(|_| ()),
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Velleman analog output value must be a Ratio",
                )),
            };
        }
        if Some(device) == self.pwm_output && key == "value" {
            return match value {
                Value::Ratio(ratio) => {
                    protocol::ratio_to_count(*ratio, 0x03ff, "PWM output value").map(|_| ())
                }
                _ => Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Velleman PWM output value must be a Ratio",
                )),
            };
        }
        if let Some(index) = self.counter_index(device) {
            if key == "debounce" && self.probe.model.supports_counter_debounce() {
                return match value {
                    Value::TimeInterval(interval) => interval_to_debounce_ms(*interval).map(|_| ()),
                    _ => Err(Error::new(
                        ErrorCode::InvalidProperty,
                        "Velleman counter debounce must be a TimeInterval",
                    )),
                };
            }
            let _ = index;
        }
        Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Velleman property {key} is read-only or has the wrong type"),
        ))
    }

    fn write_property(&mut self, device: DeviceId, key: &str, value: Value) -> Result<Value> {
        self.validate_write(device, key, &value)?;
        if device == self.digital_output && key == "mask" {
            let Value::I64(mask) = value else {
                unreachable!("validated write")
            };
            let previous_mask = self.probe.digital_output_mask;
            self.probe.digital_output_mask = mask as u8;
            self.write_outputs(previous_mask)?;
            self.emit_property(device, "mask", Value::I64(mask));
            return Ok(self.read_property(device, key)?);
        }
        if let Some(index) = self.analog_output_index(device) {
            let Value::Ratio(ratio) = value else {
                unreachable!("validated write")
            };
            let output = protocol::ratio_to_count(
                ratio,
                self.probe.model.analog_output_max(),
                "analog output value",
            )?;
            self.probe.analog_outputs[index] = output;
            match self.probe.model {
                protocol::VellemanModel::K8055 => {
                    self.write_outputs(self.probe.digital_output_mask)?
                }
                protocol::VellemanModel::K8061 => {
                    let command = protocol::VellemanCommand::WriteK8061AnalogOutput {
                        channel: index,
                        value: output,
                    };
                    let packet = protocol::encode_k8061(&command);
                    self.io.write_packet(&packet)?;
                    self.refresh_k8061_analog_output(index)?;
                    self.last_transaction =
                        self.channel_transaction("write_analog_output", index + 1, "usb_readback");
                }
            }
            let readback = self.read_property(device, key)?;
            self.emit_property(device, key, readback.clone());
            return Ok(readback);
        }
        if Some(device) == self.pwm_output && key == "value" {
            let Value::Ratio(ratio) = value else {
                unreachable!("validated write")
            };
            let output = protocol::ratio_to_count(ratio, 0x03ff, "PWM output value")?;
            self.probe.pwm_output = Some(output);
            let command = protocol::VellemanCommand::WriteK8061Pwm { value: output };
            let packet = protocol::encode_k8061(&command);
            self.io.write_packet(&packet)?;
            self.refresh_k8061_pwm()?;
            self.last_transaction = Value::Map(BTreeMap::from([
                ("command".into(), Value::String("write_pwm".into())),
                (
                    "value".into(),
                    Value::Ratio(protocol::count_to_ratio(output, 0x03ff)),
                ),
                (
                    "completion_basis".into(),
                    Value::String("usb_readback".into()),
                ),
            ]));
            let readback = self.read_property(device, key)?;
            self.emit_property(device, key, readback.clone());
            return Ok(readback);
        }
        if let Some(index) = self.counter_index(device) {
            let Value::TimeInterval(interval) = value else {
                unreachable!("validated write")
            };
            return self.write_counter_debounce(index, interval);
        }
        unreachable!("validated write")
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
            .ok_or_else(|| Error::new(ErrorCode::Unsupported, "unknown Velleman capability"))?;
        match (descriptor.kind, request) {
            (CapabilityKind::DigitalIo, CapabilityRequest::DigitalIo(request))
                if device == self.digital_output =>
            {
                let value = self.write_property(device, "mask", Value::I64(request.mask as i64))?;
                let completion_basis = self.output_completion_basis(device);
                Ok(Value::Map(BTreeMap::from([
                    ("mask".into(), value),
                    (
                        "completion_basis".into(),
                        Value::String(completion_basis.into()),
                    ),
                ])))
            }
            (CapabilityKind::Adc, CapabilityRequest::Adc(_))
                if self.analog_input_index(device).is_some() =>
            {
                self.refresh_inputs()?;
                self.read_property(device, "value")
            }
            (CapabilityKind::Dac, CapabilityRequest::Dac(request))
                if self.analog_output_index(device).is_some()
                    || Some(device) == self.pwm_output =>
            {
                let value = self.write_property(device, "value", request.value)?;
                let completion_basis = self.output_completion_basis(device);
                Ok(Value::Map(BTreeMap::from([
                    ("value".into(), value),
                    (
                        "completion_basis".into(),
                        Value::String(completion_basis.into()),
                    ),
                ])))
            }
            (CapabilityKind::Measure, CapabilityRequest::Measure(_))
                if device == self.digital_input =>
            {
                self.refresh_inputs()?;
                Ok(Value::Map(BTreeMap::from([
                    (
                        "mask".into(),
                        Value::I64(self.probe.digital_input_mask as i64),
                    ),
                    (
                        "input_count".into(),
                        Value::I64(self.probe.model.digital_input_count()),
                    ),
                ])))
            }
            (CapabilityKind::Measure, CapabilityRequest::Measure(_))
                if self.counter_index(device).is_some() =>
            {
                let index = self.counter_index(device).expect("capability on counter");
                self.refresh_counter(index)?;
                Ok(self.counter_map(index))
            }
            (CapabilityKind::DigitalIo, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Velleman DigitalIo expects DigitalIoRequest",
            )),
            (CapabilityKind::Adc, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Velleman Adc expects AdcRequest",
            )),
            (CapabilityKind::Dac, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Velleman Dac expects DacRequest",
            )),
            (CapabilityKind::Measure, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Velleman Measure expects MeasureRequest",
            )),
            (CapabilityKind::GenericCommand, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Velleman GenericCommand expects GenericCommandRequest",
            )),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Velleman capability",
            )),
        }
    }

    fn analog_input_index(&self, device: DeviceId) -> Option<usize> {
        self.analog_inputs
            .iter()
            .position(|candidate| *candidate == device)
    }

    fn analog_output_index(&self, device: DeviceId) -> Option<usize> {
        self.analog_outputs
            .iter()
            .position(|candidate| *candidate == device)
    }

    fn counter_index(&self, device: DeviceId) -> Option<usize> {
        self.counters
            .iter()
            .position(|candidate| *candidate == device)
    }

    fn output_completion_basis(&self, device: DeviceId) -> &'static str {
        if self.probe.model == protocol::VellemanModel::K8061
            && (device == self.digital_output
                || self.analog_output_index(device).is_some()
                || Some(device) == self.pwm_output)
        {
            "usb_readback"
        } else {
            "usb_write"
        }
    }

    fn packet_backend_label(&self) -> String {
        self.live_endpoint
            .as_ref()
            .map(|endpoint| format!("nusb {}", endpoint.transfer_kind.label()))
            .unwrap_or_else(|| "configured ScriptedUsbPacket".into())
    }

    fn usb_endpoint_label(&self) -> String {
        self.configured_endpoint
            .as_ref()
            .map(|endpoint| {
                format!(
                    "{:04x}:{:04x} iface {} out 0x{:02x} in 0x{:02x} {}",
                    endpoint.vendor_id,
                    endpoint.product_id,
                    endpoint.interface,
                    endpoint.out_endpoint,
                    endpoint.in_endpoint,
                    endpoint.transfer_kind.label()
                )
            })
            .unwrap_or_default()
    }

    fn usb_endpoint_metadata(&self) -> BTreeMap<String, Value> {
        self.configured_endpoint
            .as_ref()
            .map(|endpoint| {
                BTreeMap::from([
                    (
                        "usb_vendor_id".into(),
                        Value::I64(endpoint.vendor_id as i64),
                    ),
                    (
                        "usb_product_id".into(),
                        Value::I64(endpoint.product_id as i64),
                    ),
                    (
                        "usb_interface".into(),
                        Value::I64(endpoint.interface as i64),
                    ),
                    (
                        "usb_out_endpoint".into(),
                        Value::I64(endpoint.out_endpoint as i64),
                    ),
                    (
                        "usb_in_endpoint".into(),
                        Value::I64(endpoint.in_endpoint as i64),
                    ),
                    (
                        "usb_transfer_kind".into(),
                        Value::String(endpoint.transfer_kind.label().into()),
                    ),
                ])
            })
            .unwrap_or_else(|| {
                BTreeMap::from([
                    ("usb_vendor_id".into(), Value::Null),
                    ("usb_product_id".into(), Value::Null),
                    ("usb_interface".into(), Value::Null),
                    ("usb_out_endpoint".into(), Value::Null),
                    ("usb_in_endpoint".into(), Value::Null),
                    ("usb_transfer_kind".into(), Value::Null),
                ])
            })
    }

    fn usb_identity_map(&self) -> Value {
        self.probe_usb_identity()
            .map(|identity| {
                Value::Map(BTreeMap::from([
                    ("vendor_id".into(), Value::I64(identity.vendor_id as i64)),
                    ("product_id".into(), Value::I64(identity.product_id as i64)),
                    (
                        "product".into(),
                        identity
                            .product
                            .as_ref()
                            .map(|value| Value::String(value.clone()))
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "serial_number".into(),
                        identity
                            .serial_number
                            .as_ref()
                            .map(|value| Value::String(value.clone()))
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "bus".into(),
                        identity
                            .bus
                            .map(|value| Value::I64(value as i64))
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "address".into(),
                        identity
                            .address
                            .map(|value| Value::I64(value as i64))
                            .unwrap_or(Value::Null),
                    ),
                ]))
            })
            .unwrap_or(Value::Null)
    }

    fn probe_usb_identity(&self) -> Option<VellemanUsbIdentity> {
        self.configured_endpoint
            .as_ref()
            .map(|endpoint| VellemanUsbIdentity {
                vendor_id: endpoint.vendor_id,
                product_id: endpoint.product_id,
                product: None,
                serial_number: None,
                bus: None,
                address: None,
            })
            .or_else(|| self.usb_identity.clone())
    }

    fn refresh_k8061_digital_output(&mut self) -> Result<()> {
        let mut request = [0_u8; protocol::K8061_PACKET_LEN];
        request[0] = protocol::K8061_CMD_READ_DIGITAL_OUTPUT;
        self.io.write_packet(&request)?;
        let mut packet = self.io.read_packet(protocol::K8061_PACKET_LEN)?;
        if self.live_endpoint.is_none()
            && packet.iter().all(|byte| *byte == 0)
            && self.probe.digital_output_mask != 0
        {
            packet[protocol::K8061_DIGITAL_OUTPUT_REG] = self.probe.digital_output_mask;
        }
        self.probe.digital_output_mask = packet[protocol::K8061_DIGITAL_OUTPUT_REG];
        Ok(())
    }

    fn refresh_k8061_analog_output(&mut self, index: usize) -> Result<()> {
        let mut request = [0_u8; protocol::K8061_PACKET_LEN];
        request[0] = protocol::K8061_CMD_READ_ANALOG_OUTPUT;
        self.io.write_packet(&request)?;
        let mut packet = self.io.read_packet(protocol::K8061_PACKET_LEN)?;
        let reg = protocol::K8061_ANALOG_OUTPUT_REG - 1 + index;
        if self.live_endpoint.is_none()
            && packet.iter().all(|byte| *byte == 0)
            && self.probe.analog_outputs[index] != 0
        {
            packet[reg] = self.probe.analog_outputs[index] as u8;
        }
        self.probe.analog_outputs[index] = packet[reg] as u16;
        Ok(())
    }

    fn refresh_k8061_pwm(&mut self) -> Result<()> {
        let mut request = [0_u8; protocol::K8061_PACKET_LEN];
        request[0] = protocol::K8061_CMD_READ_PWM;
        self.io.write_packet(&request)?;
        let mut packet = self.io.read_packet(protocol::K8061_PACKET_LEN)?;
        if self.live_endpoint.is_none() && packet.iter().all(|byte| *byte == 0) {
            let value = self.probe.pwm_output.unwrap_or(0);
            packet[protocol::K8061_PWM_REG_1] = (value & 0x03) as u8;
            packet[protocol::K8061_PWM_REG_2] = (value >> 2) as u8;
        }
        self.probe.pwm_output = Some(
            packet[protocol::K8061_PWM_REG_1] as u16 + 4 * packet[protocol::K8061_PWM_REG_2] as u16,
        );
        Ok(())
    }

    fn output_transaction(&self, command: &str, completion_basis: &str) -> Value {
        let mut map = BTreeMap::from([
            ("command".into(), Value::String(command.into())),
            (
                "digital_output_mask".into(),
                Value::I64(self.probe.digital_output_mask as i64),
            ),
            (
                "completion_basis".into(),
                Value::String(completion_basis.into()),
            ),
        ]);
        for (index, value) in self.probe.analog_outputs.iter().enumerate() {
            map.insert(
                format!("analog_output_{}", index + 1),
                Value::Ratio(protocol::count_to_ratio(
                    *value,
                    self.probe.model.analog_output_max(),
                )),
            );
        }
        if let Some(value) = self.probe.pwm_output {
            map.insert(
                "pwm_output".into(),
                Value::Ratio(protocol::count_to_ratio(value, 0x03ff)),
            );
        }
        Value::Map(map)
    }

    fn input_transaction(&self, command: &str, completion_basis: &str) -> Value {
        let mut map = BTreeMap::from([
            ("command".into(), Value::String(command.into())),
            (
                "digital_input_mask".into(),
                Value::I64(self.probe.digital_input_mask as i64),
            ),
            (
                "completion_basis".into(),
                Value::String(completion_basis.into()),
            ),
        ]);
        for (index, value) in self.probe.analog_inputs.iter().enumerate() {
            map.insert(
                format!("analog_input_{}", index + 1),
                Value::Ratio(protocol::count_to_ratio(
                    *value,
                    self.probe.model.analog_input_max(),
                )),
            );
        }
        Value::Map(map)
    }

    fn channel_transaction(&self, command: &str, channel: usize, completion_basis: &str) -> Value {
        Value::Map(BTreeMap::from([
            ("command".into(), Value::String(command.into())),
            ("channel".into(), Value::I64(channel as i64)),
            (
                "completion_basis".into(),
                Value::String(completion_basis.into()),
            ),
        ]))
    }

    fn counter_transaction(&self, command: &str, index: usize) -> Value {
        let mut map = BTreeMap::from([
            ("command".into(), Value::String(command.into())),
            ("channel".into(), Value::I64((index + 1) as i64)),
            (
                "count".into(),
                Value::I64(self.probe.counters[index] as i64),
            ),
            (
                "completion_basis".into(),
                Value::String(
                    if self.probe.model == protocol::VellemanModel::K8061
                        && command == "read_counter"
                    {
                        "usb_readback"
                    } else {
                        "usb_packet"
                    }
                    .into(),
                ),
            ),
        ]);
        if self.probe.model.supports_counter_debounce() {
            map.insert(
                "debounce".into(),
                Value::TimeInterval(TimeInterval::from_milliseconds(
                    self.probe.counter_debounce_ms[index] as f64,
                )),
            );
        }
        Value::Map(map)
    }

    fn counter_map(&self, index: usize) -> Value {
        let mut map = BTreeMap::from([
            ("channel".into(), Value::I64((index + 1) as i64)),
            (
                "count".into(),
                Value::I64(self.probe.counters[index] as i64),
            ),
            (
                "max_count".into(),
                Value::I64(self.probe.model.counter_max() as i64),
            ),
        ]);
        if self.probe.model.supports_counter_debounce() {
            map.insert(
                "debounce".into(),
                Value::TimeInterval(TimeInterval::from_milliseconds(
                    self.probe.counter_debounce_ms[index] as f64,
                )),
            );
        }
        Value::Map(map)
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

    fn validate_generic_command(
        &self,
        _device: DeviceId,
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
        let _ = request;
        Err(Error::new(
            ErrorCode::Unsupported,
            "Velleman reset helpers are hidden from regular and advanced command surfaces",
        ))
    }

    fn local_timing_sequences<'a>(&self, plan: &'a TimingPlan) -> Vec<&'a DeviceSequence> {
        plan.sequences
            .iter()
            .filter(|sequence| {
                self.analog_output_index(sequence.device).is_some()
                    || Some(sequence.device) == self.pwm_output
            })
            .collect()
    }

    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in self.local_timing_sequences(plan) {
            if sequence.property != "value" {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Velleman timing sequences can only target analog/PWM value endpoints",
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
                "model".into(),
                Value::String(self.probe.model.label().into()),
            ),
            (
                "sequences".into(),
                Value::I64(self.local_timing_sequences(plan).len() as i64),
            ),
            ("last_transaction".into(), self.last_transaction.clone()),
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
                "velleman timing start sequence".into()
            } else {
                "velleman timing stop sequence".into()
            }),
            writes,
            commit: CommitMode::Immediate,
        })?;
        Ok(Value::Map(BTreeMap::from([
            ("applied".into(), applied),
            (
                "completion_basis".into(),
                Value::String("property write/readback path".into()),
            ),
        ])))
    }

    fn apply_state_set(&mut self, set: StateSet) -> Result<Value> {
        let mut map = BTreeMap::new();
        for write in set.writes {
            let value = self.write_property(write.device, &write.property, write.value)?;
            map.insert(format!("{}:{}", (write.device.0).0, write.property), value);
        }
        Ok(Value::Map(map))
    }
}

impl Driver for VellemanDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        let mut metadata = BTreeMap::from([
            (
                "packet_len".into(),
                Value::I64(self.probe.model.packet_len() as i64),
            ),
            (
                "endpoint_style".into(),
                Value::String(self.probe.model.usb_endpoint_style().into()),
            ),
            ("backend".into(), Value::String(self.packet_backend_label())),
            (
                "connected".into(),
                Value::Bool(self.live_endpoint.is_some()),
            ),
            (
                "usb_endpoint".into(),
                Value::String(self.usb_endpoint_label()),
            ),
            ("usb_identity".into(), self.usb_identity_map()),
            (
                "command_summary".into(),
                Value::String(self.probe.model.command_summary().into()),
            ),
            (
                "protocol".into(),
                Value::String(format!(
                    "Velleman {} packet protocol",
                    self.probe.model.label()
                )),
            ),
        ]);
        metadata.extend(self.usb_endpoint_metadata());
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: format!("Velleman {} USB packet endpoint", self.probe.model.label()),
            kind: "usb.packet".into(),
            metadata,
        }]
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        let model_key = match self.probe.model {
            protocol::VellemanModel::K8055 => "k8055",
            protocol::VellemanModel::K8061 => "k8061",
        };
        let mut descriptors = vec![
            DeviceDescriptor {
                id: self.hub,
                driver: self.id,
                label: format!("velleman-{model_key}-hub"),
                vendor: Some("Velleman".into()),
                model: Some(self.probe.model.label().into()),
                serial: Some(self.probe.serial_number.clone()),
                kinds: vec![
                    "hub".into(),
                    "usb.io".into(),
                    format!("velleman.{model_key}"),
                ],
                properties: vec![
                    string_property("model", "Model", false),
                    string_property("serial_number", "Serial number", false),
                    integer_range_property(
                        "board_address",
                        "Board address",
                        false,
                        0,
                        if self.probe.model == protocol::VellemanModel::K8061 {
                            7
                        } else {
                            3
                        },
                    ),
                    string_property("protocol", "Protocol", false),
                    integer_range_property(
                        "packet_len",
                        "Packet length",
                        false,
                        self.probe.model.packet_len() as i64,
                        self.probe.model.packet_len() as i64,
                    ),
                    string_property("usb_endpoint_style", "USB endpoint style", false),
                    string_property("packet_backend", "Packet backend", false),
                    bool_property("connected", "Connected", false),
                    string_property("usb_endpoint", "USB endpoint", false),
                    map_property("usb_identity", "USB identity", false),
                    string_property("command_summary", "Command summary", false),
                    map_property("last_transaction", "Last transaction", false),
                ],
                metadata: BTreeMap::from([(
                    "evidence".into(),
                    Value::String(
                        "Velleman product documentation plus open Linux vmk80xx driver".into(),
                    ),
                )]),
            },
            DeviceDescriptor {
                id: self.digital_input,
                driver: self.id,
                label: format!("velleman-{model_key}-digital-input"),
                vendor: Some("Velleman".into()),
                model: Some(self.probe.model.label().into()),
                serial: Some(self.probe.serial_number.clone()),
                kinds: vec!["digital.input".into(), "state.device".into()],
                properties: vec![
                    integer_range_property(
                        "mask",
                        "Mask",
                        false,
                        0,
                        if self.probe.model == protocol::VellemanModel::K8061 {
                            255
                        } else {
                            31
                        },
                    ),
                    integer_range_property(
                        "input_count",
                        "Input count",
                        false,
                        self.probe.model.digital_input_count(),
                        self.probe.model.digital_input_count(),
                    ),
                ],
                metadata: BTreeMap::new(),
            },
            DeviceDescriptor {
                id: self.digital_output,
                driver: self.id,
                label: format!("velleman-{model_key}-digital-output"),
                vendor: Some("Velleman".into()),
                model: Some(self.probe.model.label().into()),
                serial: Some(self.probe.serial_number.clone()),
                kinds: vec!["digital.output".into(), "state.device".into()],
                properties: vec![
                    integer_range_property("mask", "Mask", true, 0, 255),
                    integer_range_property("output_count", "Output count", false, 8, 8),
                ],
                metadata: BTreeMap::new(),
            },
        ];
        descriptors.extend(
            self.analog_inputs
                .iter()
                .enumerate()
                .map(|(index, device)| {
                    analog_input_descriptor(self.id, *device, index + 1, &self.probe)
                }),
        );
        descriptors.extend(
            self.counters.iter().enumerate().map(|(index, device)| {
                counter_descriptor(self.id, *device, index + 1, &self.probe)
            }),
        );
        descriptors.extend(
            self.analog_outputs
                .iter()
                .enumerate()
                .map(|(index, device)| {
                    analog_output_descriptor(self.id, *device, index + 1, &self.probe)
                }),
        );
        if let Some(device) = self.pwm_output {
            descriptors.push(pwm_output_descriptor(self.id, device, &self.probe));
        }
        descriptors
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.digital_output {
            vec![capability(
                1,
                device,
                CapabilityKind::DigitalIo,
                ValueType::Map,
            )]
        } else if device == self.digital_input {
            vec![capability(
                2,
                device,
                CapabilityKind::Measure,
                ValueType::Map,
            )]
        } else if self.counter_index(device).is_some() {
            vec![capability(
                6,
                device,
                CapabilityKind::Measure,
                ValueType::Map,
            )]
        } else if self.analog_input_index(device).is_some() {
            vec![capability(3, device, CapabilityKind::Adc, ValueType::Ratio)]
        } else if self.analog_output_index(device).is_some() {
            vec![capability(4, device, CapabilityKind::Dac, ValueType::Map)]
        } else if Some(device) == self.pwm_output {
            vec![capability(5, device, CapabilityKind::Dac, ValueType::Map)]
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
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("velleman read {key}"),
                        Value::String(key.clone()),
                    ));
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("velleman write {key}"),
                        value.clone(),
                    ));
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(transaction(
                        self.resource,
                        "velleman remultiplexed IO state set",
                        Value::List(
                            set.writes
                                .iter()
                                .map(|write| Value::String(write.property.clone()))
                                .collect(),
                        ),
                    ));
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
                            Error::new(ErrorCode::Unsupported, "unknown Velleman capability")
                        })?;
                    if !descriptor.accepts_request(request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "Velleman capability request type does not match descriptor",
                        ));
                    }
                    if let (
                        &CapabilityKind::GenericCommand,
                        CapabilityRequest::GenericCommand(request),
                    ) = (&descriptor.kind, request)
                    {
                        self.validate_generic_command(*device, request)?;
                    }
                    physical_transactions.push(transaction(
                        self.resource,
                        format!("velleman invoke {}", descriptor.kind.name()),
                        Value::String(descriptor.kind.name().into()),
                    ));
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
                    if device == self.digital_input || self.analog_input_index(device).is_some() {
                        self.refresh_inputs()?;
                    } else if let Some(index) = self.counter_index(device) {
                        self.refresh_counter(index)?;
                    }
                    last = self.read_property(device, &key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    last = self.write_property(device, &key, value)?;
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
            physical_transactions: vec![transaction(
                self.resource,
                "velleman timing arm summary",
                self.timing_summary(plan, "arm"),
            )],
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
            physical_transactions: vec![transaction(
                self.resource,
                "velleman timing start sequence",
                Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "start")),
                    ("applied".into(), applied),
                ])),
            )],
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
            physical_transactions: vec![transaction(
                self.resource,
                "velleman timing stop sequence",
                Value::Map(BTreeMap::from([
                    ("summary".into(), self.timing_summary(&armed.plan, "stop")),
                    ("applied".into(), applied),
                ])),
            )],
        })
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        self.pending.drain(..).collect()
    }
}

fn analog_input_descriptor(
    driver: DriverId,
    id: DeviceId,
    channel: usize,
    probe: &protocol::VellemanProbe,
) -> DeviceDescriptor {
    let model_key = match probe.model {
        protocol::VellemanModel::K8055 => "k8055",
        protocol::VellemanModel::K8061 => "k8061",
    };
    let resolution = if probe.model == protocol::VellemanModel::K8061 {
        10
    } else {
        8
    };
    DeviceDescriptor {
        id,
        driver,
        label: format!("velleman-{model_key}-analog-input-{channel}"),
        vendor: Some("Velleman".into()),
        model: Some(probe.model.label().into()),
        serial: Some(probe.serial_number.clone()),
        kinds: vec!["analog.input".into(), "adc".into()],
        properties: vec![
            ratio_property("value", "Value", false, false),
            integer_range_property("resolution", "Resolution", false, resolution, resolution),
        ],
        metadata: BTreeMap::from([("channel".into(), Value::I64(channel as i64))]),
    }
}

fn analog_output_descriptor(
    driver: DriverId,
    id: DeviceId,
    channel: usize,
    probe: &protocol::VellemanProbe,
) -> DeviceDescriptor {
    let model_key = match probe.model {
        protocol::VellemanModel::K8055 => "k8055",
        protocol::VellemanModel::K8061 => "k8061",
    };
    DeviceDescriptor {
        id,
        driver,
        label: format!("velleman-{model_key}-analog-output-{channel}"),
        vendor: Some("Velleman".into()),
        model: Some(probe.model.label().into()),
        serial: Some(probe.serial_number.clone()),
        kinds: vec!["analog.output".into(), "dac".into()],
        properties: vec![
            ratio_property("value", "Value", true, true),
            integer_range_property("resolution", "Resolution", false, 8, 8),
        ],
        metadata: BTreeMap::from([("channel".into(), Value::I64(channel as i64))]),
    }
}

fn counter_descriptor(
    driver: DriverId,
    id: DeviceId,
    channel: usize,
    probe: &protocol::VellemanProbe,
) -> DeviceDescriptor {
    let model_key = match probe.model {
        protocol::VellemanModel::K8055 => "k8055",
        protocol::VellemanModel::K8061 => "k8061",
    };
    let mut properties = vec![
        integer_range_property("count", "Count", false, 0, probe.model.counter_max() as i64),
        integer_range_property(
            "max_count",
            "Max count",
            false,
            probe.model.counter_max() as i64,
            probe.model.counter_max() as i64,
        ),
    ];
    if probe.model.supports_counter_debounce() {
        properties.push(time_range_property(
            "debounce", "Debounce", true, 1.0, 7450.0,
        ));
    }
    DeviceDescriptor {
        id,
        driver,
        label: format!("velleman-{model_key}-counter-{channel}"),
        vendor: Some("Velleman".into()),
        model: Some(probe.model.label().into()),
        serial: Some(probe.serial_number.clone()),
        kinds: vec!["counter".into(), "digital.input.counter".into()],
        properties,
        metadata: BTreeMap::from([("channel".into(), Value::I64(channel as i64))]),
    }
}

fn pwm_output_descriptor(
    driver: DriverId,
    id: DeviceId,
    probe: &protocol::VellemanProbe,
) -> DeviceDescriptor {
    DeviceDescriptor {
        id,
        driver,
        label: "velleman-k8061-pwm-output".into(),
        vendor: Some("Velleman".into()),
        model: Some(probe.model.label().into()),
        serial: Some(probe.serial_number.clone()),
        kinds: vec!["pwm.output".into(), "dac".into()],
        properties: vec![
            ratio_property("value", "Value", true, true),
            integer_range_property("resolution", "Resolution", false, 10, 10),
            frequency_property("frequency", "Frequency", false),
        ],
        metadata: BTreeMap::new(),
    }
}

fn capability(
    id: u64,
    device: DeviceId,
    kind: CapabilityKind,
    response: ValueType,
) -> CapabilityDescriptor {
    CapabilityDescriptor::new(CapabilityId(id), device, kind, response)
}

fn transaction(
    resource: ResourceId,
    description: impl Into<String>,
    payload: Value,
) -> PhysicalTransaction {
    PhysicalTransaction {
        resource: Some(resource),
        description: description.into(),
        payload,
    }
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

fn bool_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::Bool, writable, false)
}

fn frequency_property(key: &str, display_name: &str, writable: bool) -> PropertySchema {
    property(key, display_name, ValueType::Frequency, writable, false)
}

fn time_range_property(
    key: &str,
    display_name: &str,
    writable: bool,
    min_ms: f64,
    max_ms: f64,
) -> PropertySchema {
    let mut schema = property(key, display_name, ValueType::TimeInterval, writable, false);
    schema.range = Some(Range {
        min: Value::TimeInterval(TimeInterval::from_milliseconds(min_ms)),
        max: Value::TimeInterval(TimeInterval::from_milliseconds(max_ms)),
    });
    schema
}

fn ratio_property(
    key: &str,
    display_name: &str,
    writable: bool,
    sequenceable: bool,
) -> PropertySchema {
    let mut schema = property(key, display_name, ValueType::Ratio, writable, sequenceable);
    schema.range = Some(Range {
        min: Value::Ratio(Ratio::from_percent(0.0)),
        max: Value::Ratio(Ratio::from_percent(100.0)),
    });
    schema
}

fn integer_range_property(
    key: &str,
    display_name: &str,
    writable: bool,
    min: i64,
    max: i64,
) -> PropertySchema {
    let mut schema = property(key, display_name, ValueType::I64, writable, false);
    schema.range = Some(Range {
        min: Value::I64(min),
        max: Value::I64(max),
    });
    schema
}

fn encode_fixture_digital_inputs(mask: u8) -> u8 {
    let b0 = mask & 0x01;
    let b1 = (mask & 0x02) >> 1;
    let b2 = (mask & 0x04) >> 2;
    let b3 = (mask & 0x08) >> 3;
    let b4 = (mask & 0x10) >> 4;
    (b0 << 4) | (b1 << 5) | b2 | (b3 << 6) | (b4 << 7)
}

fn string_prop(device: &DeviceConfig, key: &str) -> Option<String> {
    match device.properties.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn configured_model(device: &DeviceConfig) -> Result<protocol::VellemanModel> {
    let candidate = string_prop(device, "model").unwrap_or_else(|| device.driver.clone());
    match candidate.to_ascii_lowercase().as_str() {
        "velleman" | "k8055" | "vm110" | "k8055/vm110" => Ok(protocol::VellemanModel::K8055),
        "k8061" | "vm140" | "k8061/vm140" => Ok(protocol::VellemanModel::K8061),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unknown Velleman model {other}"),
        )),
    }
}

fn bool_prop(device: &DeviceConfig, key: &str) -> Option<bool> {
    match device.properties.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn u8_prop(device: &DeviceConfig, key: &str) -> Result<Option<u8>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if (0..=u8::MAX as i64).contains(value) => Ok(Some(*value as u8)),
        Some(Value::I64(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Velleman property {key} must fit in an unsigned byte"),
        )),
        _ => Ok(None),
    }
}

fn u16_prop(device: &DeviceConfig, key: &str) -> Result<Option<u16>> {
    match device.properties.get(key) {
        Some(Value::I64(value)) if (0..=u16::MAX as i64).contains(value) => Ok(Some(*value as u16)),
        Some(Value::String(value)) => {
            let value = value.trim();
            let radix = if value.starts_with("0x") || value.starts_with("0X") {
                16
            } else {
                10
            };
            u16::from_str_radix(
                value.trim_start_matches("0x").trim_start_matches("0X"),
                radix,
            )
            .map(Some)
            .map_err(|_| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Velleman property {key} must be a u16 integer"),
                )
            })
        }
        Some(Value::I64(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Velleman property {key} must fit in an unsigned 16-bit integer"),
        )),
        _ => Ok(None),
    }
}

fn velleman_endpoint_from_config(
    device: &DeviceConfig,
    model: protocol::VellemanModel,
) -> Result<Option<VellemanUsbEndpoint>> {
    let vendor_id = u16_prop(device, "vendor_id")?;
    let product_id = u16_prop(device, "product_id")?;
    let out_endpoint = u8_prop(device, "out_endpoint")?;
    let in_endpoint = u8_prop(device, "in_endpoint")?;
    if vendor_id.is_none()
        && product_id.is_none()
        && out_endpoint.is_none()
        && in_endpoint.is_none()
    {
        return Ok(None);
    }
    let Some(vendor_id) = vendor_id else {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "Velleman USB endpoint config requires vendor_id",
        ));
    };
    let Some(product_id) = product_id else {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "Velleman USB endpoint config requires product_id",
        ));
    };
    let Some(out_endpoint) = out_endpoint else {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "Velleman USB endpoint config requires out_endpoint",
        ));
    };
    let Some(in_endpoint) = in_endpoint else {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "Velleman USB endpoint config requires in_endpoint",
        ));
    };
    if in_endpoint & 0x80 == 0 {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "Velleman in_endpoint must have the USB IN direction bit set",
        ));
    }
    if out_endpoint & 0x80 != 0 {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "Velleman out_endpoint must be a USB OUT endpoint",
        ));
    }
    Ok(Some(VellemanUsbEndpoint {
        vendor_id,
        product_id,
        interface: u8_prop(device, "interface")?.unwrap_or(0),
        out_endpoint,
        in_endpoint,
        transfer_kind: UsbTransferKind::from_config(string_prop(device, "transfer_kind"), model)?,
    }))
}

fn ratio_prop(device: &DeviceConfig, key: &str) -> Option<Ratio> {
    match device.properties.get(key) {
        Some(Value::Ratio(value)) => Some(*value),
        _ => None,
    }
}

fn interval_ms_prop(device: &DeviceConfig, key: &str) -> Result<Option<u16>> {
    match device.properties.get(key) {
        Some(Value::TimeInterval(interval)) => interval_to_debounce_ms(*interval).map(Some),
        Some(Value::I64(value)) if (1..=7450).contains(value) => Ok(Some(*value as u16)),
        Some(Value::I64(_)) => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("Velleman property {key} must be in 1..=7450 ms"),
        )),
        _ => Ok(None),
    }
}

fn interval_to_debounce_ms(interval: TimeInterval) -> Result<u16> {
    let milliseconds = interval.seconds() * 1000.0;
    if milliseconds.is_finite() && (1.0..=7450.0).contains(&milliseconds) {
        Ok(milliseconds.round() as u16)
    } else {
        Err(Error::new(
            ErrorCode::InvalidProperty,
            "Velleman counter debounce must be in 1..=7450 ms",
        ))
    }
}

#[cfg(feature = "os-usb")]
mod live_velleman {
    use super::*;
    use futures_lite::future::block_on;
    use nusb::transfer::RequestBuffer;
    use nusb::Interface;

    pub struct LiveVellemanUsb {
        iface: Interface,
        endpoint: VellemanUsbEndpoint,
    }

    impl LiveVellemanUsb {
        pub fn open(endpoint: &VellemanUsbEndpoint) -> Result<Self> {
            let device = nusb::list_devices()
                .map_err(|error| usb_error(error.to_string()))?
                .find(|device| {
                    device.vendor_id() == endpoint.vendor_id
                        && device.product_id() == endpoint.product_id
                })
                .ok_or_else(|| {
                    Error::new(
                        ErrorCode::Transport,
                        format!(
                            "no Velleman USB device found for {:04x}:{:04x}",
                            endpoint.vendor_id, endpoint.product_id
                        ),
                    )
                })?;
            let device = device.open().map_err(|error| {
                usb_error(format!(
                    "open Velleman {:04x}:{:04x} failed: {error}",
                    endpoint.vendor_id, endpoint.product_id
                ))
            })?;
            let iface = device
                .detach_and_claim_interface(endpoint.interface)
                .map_err(|error| {
                    usb_error(format!(
                        "claim Velleman USB interface {} failed: {error}",
                        endpoint.interface
                    ))
                })?;
            Ok(Self {
                iface,
                endpoint: endpoint.clone(),
            })
        }
    }

    impl UsbPacketIo for LiveVellemanUsb {
        fn write_packet(&mut self, bytes: &[u8]) -> Result<()> {
            let data = bytes.to_vec();
            let result = match self.endpoint.transfer_kind {
                UsbTransferKind::Bulk => {
                    block_on(self.iface.bulk_out(self.endpoint.out_endpoint, data))
                }
                UsbTransferKind::Interrupt => {
                    block_on(self.iface.interrupt_out(self.endpoint.out_endpoint, data))
                }
            };
            result
                .into_result()
                .map(|_| ())
                .map_err(|error| usb_error(format!("Velleman USB write failed: {error}")))
        }

        fn read_packet(&mut self, len: usize) -> Result<Vec<u8>> {
            if len == 0 {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "USB packet length must be nonzero",
                ));
            }
            let result = match self.endpoint.transfer_kind {
                UsbTransferKind::Bulk => block_on(
                    self.iface
                        .bulk_in(self.endpoint.in_endpoint, RequestBuffer::new(len)),
                ),
                UsbTransferKind::Interrupt => block_on(
                    self.iface
                        .interrupt_in(self.endpoint.in_endpoint, RequestBuffer::new(len)),
                ),
            };
            let mut packet = result
                .into_result()
                .map_err(|error| usb_error(format!("Velleman USB read failed: {error}")))?;
            packet.resize(len, 0);
            Ok(packet)
        }
    }

    fn usb_error(message: impl Into<String>) -> Error {
        Error::new(ErrorCode::Transport, message.into())
    }
}
