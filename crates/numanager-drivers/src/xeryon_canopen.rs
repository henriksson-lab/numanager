use numanager_core::can::{CanFrame, CanIo};
use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::time::Duration;

#[doc(hidden)]
#[allow(dead_code)]
pub(crate) mod protocol {
    use super::*;

    pub const DEFAULT_NODE_ID: u8 = 32;
    pub const PROFILE_POSITION_MODE: i8 = 1;
    pub const HOMING_MODE: i8 = 6;

    pub const CONTROLWORD: Object = Object::new(0x6040, 0x00);
    pub const STATUSWORD: Object = Object::new(0x6041, 0x00);
    pub const MODES_OF_OPERATION: Object = Object::new(0x6060, 0x00);
    pub const MODES_OF_OPERATION_DISPLAY: Object = Object::new(0x6061, 0x00);
    pub const POSITION_ACTUAL_VALUE: Object = Object::new(0x6064, 0x00);
    pub const TARGET_POSITION: Object = Object::new(0x607A, 0x00);
    pub const PROFILE_VELOCITY: Object = Object::new(0x6081, 0x00);
    pub const PROFILE_ACCELERATION: Object = Object::new(0x6083, 0x00);
    pub const PROFILE_DECELERATION: Object = Object::new(0x6084, 0x00);
    pub const HOMING_METHOD: Object = Object::new(0x6098, 0x00);

    pub const CW_SHUTDOWN: u16 = 0x0006;
    pub const CW_SWITCH_ON: u16 = 0x0007;
    pub const CW_ENABLE_OPERATION: u16 = 0x000F;
    pub const CW_NEW_SETPOINT: u16 = 0x001F;
    pub const CW_QUICK_STOP: u16 = 0x0002;
    pub const CW_START_HOMING: u16 = 0x001F;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Object {
        pub index: u16,
        pub subindex: u8,
    }

    impl Object {
        pub const fn new(index: u16, subindex: u8) -> Self {
            Self { index, subindex }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum SdoValue {
        I8(i8),
        U16(u16),
        I32(i32),
        U32(u32),
    }

    impl SdoValue {
        pub fn command_byte(&self) -> u8 {
            match self {
                SdoValue::I8(_) => 0x2F,
                SdoValue::U16(_) => 0x2B,
                SdoValue::I32(_) | SdoValue::U32(_) => 0x23,
            }
        }

        pub fn bytes(&self) -> [u8; 4] {
            match self {
                SdoValue::I8(value) => [*value as u8, 0, 0, 0],
                SdoValue::U16(value) => {
                    let bytes = value.to_le_bytes();
                    [bytes[0], bytes[1], 0, 0]
                }
                SdoValue::I32(value) => value.to_le_bytes(),
                SdoValue::U32(value) => value.to_le_bytes(),
            }
        }

        pub fn as_value(&self) -> Value {
            match self {
                SdoValue::I8(value) => Value::I64(*value as i64),
                SdoValue::U16(value) => Value::I64(*value as i64),
                SdoValue::I32(value) => Value::I64(*value as i64),
                SdoValue::U32(value) => Value::I64(*value as i64),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum CanopenTransaction {
        NmtStart,
        SdoDownload { object: Object, value: SdoValue },
        SdoUpload { object: Object },
    }

    impl CanopenTransaction {
        pub fn frame(&self, node_id: u8) -> Result<CanFrame> {
            CanFrame::standard(self.cob_id(node_id) as u32, self.data(node_id))
        }

        pub fn cob_id(&self, node_id: u8) -> u16 {
            match self {
                CanopenTransaction::NmtStart => 0x000,
                CanopenTransaction::SdoDownload { .. } | CanopenTransaction::SdoUpload { .. } => {
                    0x600 + node_id as u16
                }
            }
        }

        pub fn data(&self, node_id: u8) -> Vec<u8> {
            match self {
                CanopenTransaction::NmtStart => vec![0x01, node_id],
                CanopenTransaction::SdoUpload { object } => {
                    let index = object.index.to_le_bytes();
                    vec![0x40, index[0], index[1], object.subindex, 0, 0, 0, 0]
                }
                CanopenTransaction::SdoDownload { object, value } => {
                    let index = object.index.to_le_bytes();
                    let value_bytes = value.bytes();
                    vec![
                        value.command_byte(),
                        index[0],
                        index[1],
                        object.subindex,
                        value_bytes[0],
                        value_bytes[1],
                        value_bytes[2],
                        value_bytes[3],
                    ]
                }
            }
        }

        pub fn as_value(&self, node_id: u8) -> Value {
            let mut map = BTreeMap::from([
                (
                    "cob_id".into(),
                    Value::String(format!("0x{:03X}", self.cob_id(node_id))),
                ),
                ("data".into(), Value::Bytes(self.data(node_id))),
            ]);
            match self {
                CanopenTransaction::NmtStart => {
                    map.insert("kind".into(), Value::String("nmt_start_remote_node".into()));
                }
                CanopenTransaction::SdoUpload { object } => {
                    map.insert("kind".into(), Value::String("sdo_upload".into()));
                    map.insert("object".into(), object_value(*object));
                }
                CanopenTransaction::SdoDownload { object, value } => {
                    map.insert("kind".into(), Value::String("sdo_download".into()));
                    map.insert("object".into(), object_value(*object));
                    map.insert("value".into(), value.as_value());
                }
            }
            Value::Map(map)
        }
    }

    pub fn object_value(object: Object) -> Value {
        Value::Map(BTreeMap::from([
            (
                "index".into(),
                Value::String(format!("0x{:04X}", object.index)),
            ),
            ("subindex".into(), Value::I64(object.subindex as i64)),
        ]))
    }

    pub fn profile_position_sequence(
        target_counts: i32,
        velocity_counts_s: Option<u32>,
        acceleration_counts_s2: Option<u32>,
    ) -> Vec<CanopenTransaction> {
        let mut transactions = vec![
            CanopenTransaction::SdoDownload {
                object: MODES_OF_OPERATION,
                value: SdoValue::I8(PROFILE_POSITION_MODE),
            },
            CanopenTransaction::SdoDownload {
                object: TARGET_POSITION,
                value: SdoValue::I32(target_counts),
            },
        ];
        if let Some(velocity) = velocity_counts_s {
            transactions.push(CanopenTransaction::SdoDownload {
                object: PROFILE_VELOCITY,
                value: SdoValue::U32(velocity),
            });
        }
        if let Some(acceleration) = acceleration_counts_s2 {
            transactions.push(CanopenTransaction::SdoDownload {
                object: PROFILE_ACCELERATION,
                value: SdoValue::U32(acceleration),
            });
        }
        transactions.extend([
            CanopenTransaction::SdoDownload {
                object: CONTROLWORD,
                value: SdoValue::U16(CW_SHUTDOWN),
            },
            CanopenTransaction::SdoDownload {
                object: CONTROLWORD,
                value: SdoValue::U16(CW_SWITCH_ON),
            },
            CanopenTransaction::SdoDownload {
                object: CONTROLWORD,
                value: SdoValue::U16(CW_ENABLE_OPERATION),
            },
            CanopenTransaction::SdoDownload {
                object: CONTROLWORD,
                value: SdoValue::U16(CW_NEW_SETPOINT),
            },
        ]);
        transactions
    }

    pub fn homing_sequence(method: Option<i8>) -> Vec<CanopenTransaction> {
        let mut transactions = vec![CanopenTransaction::SdoDownload {
            object: MODES_OF_OPERATION,
            value: SdoValue::I8(HOMING_MODE),
        }];
        if let Some(method) = method {
            transactions.push(CanopenTransaction::SdoDownload {
                object: HOMING_METHOD,
                value: SdoValue::I8(method),
            });
        }
        transactions.extend([
            CanopenTransaction::SdoDownload {
                object: CONTROLWORD,
                value: SdoValue::U16(CW_ENABLE_OPERATION),
            },
            CanopenTransaction::SdoDownload {
                object: CONTROLWORD,
                value: SdoValue::U16(CW_START_HOMING),
            },
        ]);
        transactions
    }

    pub fn refresh_sequence() -> Vec<CanopenTransaction> {
        vec![
            CanopenTransaction::SdoUpload { object: STATUSWORD },
            CanopenTransaction::SdoUpload {
                object: POSITION_ACTUAL_VALUE,
            },
            CanopenTransaction::SdoUpload {
                object: TARGET_POSITION,
            },
            CanopenTransaction::SdoUpload {
                object: MODES_OF_OPERATION_DISPLAY,
            },
        ]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct XeryonCanopenProbe {
    pub label: String,
    pub node_id: u8,
    pub connect: bool,
    pub can_backend: String,
    pub can_interface: Option<String>,
    pub serial_port: Option<String>,
    pub serial_baud_rate: u32,
    pub slcan_bitrate_code: Option<char>,
    pub slcan_open: bool,
    pub can_timeout_ms: u64,
    pub stage_model: String,
    pub device_profile: String,
    pub encoder_units_per_um: f64,
    pub low_limit_um: f64,
    pub high_limit_um: f64,
    pub position_um: f64,
    pub target_um: f64,
    pub velocity_um_s: f64,
    pub acceleration_um_s2: f64,
    pub statusword: u16,
    pub mode_of_operation: i8,
    pub homing_method: Option<i8>,
    pub eds_path: Option<String>,
    pub eds_status: String,
    pub eds_objects: Vec<EdsObject>,
}

impl XeryonCanopenProbe {
    pub fn simulated() -> Self {
        Self {
            label: "Configured Xeryon integrated CANopen stage".into(),
            node_id: protocol::DEFAULT_NODE_ID,
            connect: false,
            can_backend: "planned".into(),
            can_interface: None,
            serial_port: None,
            serial_baud_rate: 115_200,
            slcan_bitrate_code: None,
            slcan_open: false,
            can_timeout_ms: 50,
            stage_model: "XLA/XUMU integrated controller".into(),
            device_profile: "CiA 402".into(),
            encoder_units_per_um: 1.0,
            low_limit_um: 0.0,
            high_limit_um: 100_000.0,
            position_um: 0.0,
            target_um: 0.0,
            velocity_um_s: 10_000.0,
            acceleration_um_s2: 100_000.0,
            statusword: 0,
            mode_of_operation: protocol::PROFILE_POSITION_MODE,
            homing_method: None,
            eds_path: None,
            eds_status: "not configured".into(),
            eds_objects: Vec::new(),
        }
    }

    fn native_position(&self, um: f64) -> i32 {
        (um * self.encoder_units_per_um).round() as i32
    }

    fn native_velocity(&self, um_s: f64) -> u32 {
        (um_s * self.encoder_units_per_um).round().max(0.0) as u32
    }

    fn native_acceleration(&self, um_s2: f64) -> u32 {
        (um_s2 * self.encoder_units_per_um).round().max(0.0) as u32
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdsObject {
    pub section: String,
    pub index: u16,
    pub subindex: Option<u8>,
    pub parameter_name: Option<String>,
    pub object_type: Option<String>,
    pub data_type: Option<String>,
    pub access_type: Option<String>,
    pub default_value: Option<String>,
    pub pdo_mapping: Option<String>,
}

pub struct XeryonCanopenDiscovery {
    next_id: DriverId,
    probes: Vec<XeryonCanopenProbe>,
}

impl XeryonCanopenDiscovery {
    pub fn simulated(next_id: DriverId) -> Self {
        Self {
            next_id,
            probes: vec![XeryonCanopenProbe::simulated()],
        }
    }

    pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self> {
        let probes = config
            .devices
            .iter()
            .filter(|device| {
                matches!(
                    device.driver.as_str(),
                    "xeryon_canopen" | "xeryon_integrated" | "xeryon_xla_integrated"
                )
            })
            .map(probe_from_config)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { next_id, probes })
    }
}

impl DriverDiscovery for XeryonCanopenDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        self.probes
            .drain(..)
            .enumerate()
            .map(|(index, probe)| {
                let id = DriverId(self.next_id.0 + index as u64);
                let label = probe.label.clone();
                let driver = if probe.connect {
                    Box::new(XeryonCanopenDriver::live(id, probe)?) as Box<dyn Driver>
                } else {
                    Box::new(XeryonCanopenDriver::configured(id, probe)) as Box<dyn Driver>
                };
                Ok(DriverCandidate::from_driver(label, driver))
            })
            .collect()
    }
}

pub struct XeryonCanopenDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    axis: DeviceId,
    probe: XeryonCanopenProbe,
    pending: VecDeque<DriverEvent>,
    next_token: u64,
    last_transactions: Vec<protocol::CanopenTransaction>,
    last_can_frames: Vec<CanFrame>,
    connected: bool,
    can: Option<Box<dyn CanIo>>,
}

impl XeryonCanopenDriver {
    pub fn simulated(id: DriverId) -> Self {
        Self::configured(id, XeryonCanopenProbe::simulated())
    }

    pub fn configured(id: DriverId, probe: XeryonCanopenProbe) -> Self {
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 931)),
            hub: DeviceId(NodeId(id.0 * 1000 + 940)),
            axis: DeviceId(NodeId(id.0 * 1000 + 941)),
            probe,
            pending: VecDeque::new(),
            next_token: 1,
            last_transactions: Vec::new(),
            last_can_frames: Vec::new(),
            connected: false,
            can: None,
        }
    }

    pub fn live(id: DriverId, probe: XeryonCanopenProbe) -> Result<Self> {
        let can = open_can_transport(&probe)?;
        let mut driver = Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + 931)),
            hub: DeviceId(NodeId(id.0 * 1000 + 940)),
            axis: DeviceId(NodeId(id.0 * 1000 + 941)),
            probe,
            pending: VecDeque::new(),
            next_token: 1,
            last_transactions: Vec::new(),
            last_can_frames: Vec::new(),
            connected: true,
            can: Some(can),
        };
        let _ = driver.execute_transactions(vec![protocol::CanopenTransaction::NmtStart])?;
        Ok(driver)
    }

    fn token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn plan(&mut self, transactions: Vec<protocol::CanopenTransaction>) -> Value {
        self.last_transactions = transactions;
        Value::List(
            self.last_transactions
                .iter()
                .map(|transaction| transaction.as_value(self.probe.node_id))
                .collect(),
        )
    }

    fn execute_transactions(
        &mut self,
        transactions: Vec<protocol::CanopenTransaction>,
    ) -> Result<Value> {
        self.last_can_frames.clear();
        if !self.connected {
            return Ok(self.plan(transactions));
        }
        self.last_transactions = transactions;
        let mut results = Vec::new();
        for transaction in self.last_transactions.clone() {
            let result = self.execute_transaction(&transaction)?;
            results.push(result);
        }
        Ok(Value::Map(BTreeMap::from([
            (
                "transactions".into(),
                Value::List(
                    self.last_transactions
                        .iter()
                        .map(|transaction| transaction.as_value(self.probe.node_id))
                        .collect(),
                ),
            ),
            (
                "frames".into(),
                Value::List(self.last_can_frames.iter().map(can_frame_value).collect()),
            ),
            ("responses".into(), Value::List(results)),
        ])))
    }

    fn execute_transaction(&mut self, transaction: &protocol::CanopenTransaction) -> Result<Value> {
        let frame = transaction.frame(self.probe.node_id)?;
        let Some(can) = self.can.as_mut() else {
            return Err(Error::new(
                ErrorCode::Transport,
                "Xeryon CANopen live transport is not open",
            ));
        };
        can.write_frame(&frame)?;
        self.last_can_frames.push(frame);
        match transaction {
            protocol::CanopenTransaction::NmtStart => Ok(Value::String("nmt_start_sent".into())),
            protocol::CanopenTransaction::SdoDownload { object, .. } => {
                let response = wait_for_sdo_response(
                    can.as_mut(),
                    self.probe.node_id,
                    *object,
                    self.probe.can_timeout_ms,
                )?;
                validate_sdo_download_ack(*object, &response)?;
                Ok(can_frame_value(&response))
            }
            protocol::CanopenTransaction::SdoUpload { object } => {
                let response = wait_for_sdo_response(
                    can.as_mut(),
                    self.probe.node_id,
                    *object,
                    self.probe.can_timeout_ms,
                )?;
                self.apply_sdo_upload(*object, &response)?;
                Ok(can_frame_value(&response))
            }
        }
    }

    fn apply_sdo_upload(&mut self, object: protocol::Object, frame: &CanFrame) -> Result<()> {
        let value = parse_sdo_upload_i64(object, frame)?;
        match object {
            protocol::STATUSWORD => self.probe.statusword = value as u16,
            protocol::POSITION_ACTUAL_VALUE => {
                self.probe.position_um = value as f64 / self.probe.encoder_units_per_um;
            }
            protocol::TARGET_POSITION => {
                self.probe.target_um = value as f64 / self.probe.encoder_units_per_um;
            }
            protocol::MODES_OF_OPERATION_DISPLAY => self.probe.mode_of_operation = value as i8,
            _ => {}
        }
        Ok(())
    }

    fn descriptors_for(&self) -> Vec<DeviceDescriptor> {
        vec![
            DeviceDescriptor {
                id: self.hub,
                driver: self.id,
                label: "xeryon-canopen-hub".into(),
                vendor: Some("Xeryon".into()),
                model: Some("Integrated CANopen controller".into()),
                serial: None,
                kinds: vec![
                    "hub".into(),
                    "motion.controller".into(),
                    "canopen".into(),
                    "cia402".into(),
                    "xeryon.integrated".into(),
                ],
                properties: vec![
                    property("node_id", "Node ID", ValueType::I64, None, false, None),
                    property(
                        "can_backend",
                        "CAN backend",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                    property("connected", "Connected", ValueType::Bool, None, false, None),
                    property(
                        "device_profile",
                        "Device profile",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                    property("eds_path", "EDS path", ValueType::String, None, false, None),
                    property(
                        "eds_status",
                        "EDS status",
                        ValueType::String,
                        None,
                        false,
                        None,
                    ),
                    property(
                        "eds_object_count",
                        "EDS object count",
                        ValueType::I64,
                        None,
                        false,
                        None,
                    ),
                    property(
                        "eds_objects",
                        "EDS objects",
                        ValueType::List,
                        None,
                        false,
                        None,
                    ),
                    property(
                        "last_transactions",
                        "Last transactions",
                        ValueType::List,
                        None,
                        false,
                        None,
                    ),
                    property(
                        "last_can_frames",
                        "Last CAN frames",
                        ValueType::List,
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
                    ("node_id".into(), Value::I64(self.probe.node_id as i64)),
                    (
                        "can_backend".into(),
                        Value::String(self.probe.can_backend.clone()),
                    ),
                    ("connected".into(), Value::Bool(self.connected)),
                    (
                        "eds_path".into(),
                        self.probe
                            .eds_path
                            .as_ref()
                            .map(|path| Value::String(path.clone()))
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "eds_status".into(),
                        Value::String(self.probe.eds_status.clone()),
                    ),
                ]),
            },
            DeviceDescriptor {
                id: self.axis,
                driver: self.id,
                label: "xeryon-canopen-axis".into(),
                vendor: Some("Xeryon".into()),
                model: Some(self.probe.stage_model.clone()),
                serial: None,
                kinds: vec![
                    "axis.x".into(),
                    "stage.axis".into(),
                    "motion.stage".into(),
                    "canopen.cia402.axis".into(),
                    "xeryon.integrated.axis".into(),
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
                    acceleration_property_range(
                        "acceleration",
                        "Acceleration",
                        Some("um/s^2"),
                        true,
                        0.0,
                        5_000_000.0,
                    ),
                    property(
                        "statusword",
                        "Statusword",
                        ValueType::I64,
                        None,
                        false,
                        None,
                    ),
                    property(
                        "mode_of_operation",
                        "Mode of operation",
                        ValueType::I64,
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
                    ("node_id".into(), Value::I64(self.probe.node_id as i64)),
                    (
                        "stage_model".into(),
                        Value::String(self.probe.stage_model.clone()),
                    ),
                    (
                        "encoder_units_per_um".into(),
                        Value::F64(self.probe.encoder_units_per_um),
                    ),
                    (
                        "encoder_unit".into(),
                        position(1.0 / self.probe.encoder_units_per_um),
                    ),
                ]),
            },
        ]
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        match (device, key) {
            (device, "node_id") if device == self.hub => Ok(Value::I64(self.probe.node_id as i64)),
            (device, "can_backend") if device == self.hub => {
                Ok(Value::String(self.probe.can_backend.clone()))
            }
            (device, "connected") if device == self.hub => Ok(Value::Bool(self.connected)),
            (device, "device_profile") if device == self.hub => {
                Ok(Value::String(self.probe.device_profile.clone()))
            }
            (device, "eds_path") if device == self.hub => Ok(self
                .probe
                .eds_path
                .as_ref()
                .map(|path| Value::String(path.clone()))
                .unwrap_or(Value::Null)),
            (device, "eds_status") if device == self.hub => {
                Ok(Value::String(self.probe.eds_status.clone()))
            }
            (device, "eds_object_count") if device == self.hub => {
                Ok(Value::I64(self.probe.eds_objects.len() as i64))
            }
            (device, "eds_objects") if device == self.hub => Ok(Value::List(
                self.probe
                    .eds_objects
                    .iter()
                    .map(eds_object_value)
                    .collect(),
            )),
            (device, "last_transactions") if device == self.hub => Ok(Value::List(
                self.last_transactions
                    .iter()
                    .map(|transaction| transaction.as_value(self.probe.node_id))
                    .collect(),
            )),
            (device, "last_can_frames") if device == self.hub => Ok(Value::List(
                self.last_can_frames.iter().map(can_frame_value).collect(),
            )),
            (device, "state_summary") if device == self.hub => Ok(self.state_summary()),
            (device, "position") if device == self.axis => Ok(position(self.probe.position_um)),
            (device, "target") if device == self.axis => Ok(position(self.probe.target_um)),
            (device, "velocity") if device == self.axis => Ok(velocity(self.probe.velocity_um_s)),
            (device, "acceleration") if device == self.axis => {
                Ok(acceleration(self.probe.acceleration_um_s2))
            }
            (device, "statusword") if device == self.axis => {
                Ok(Value::I64(self.probe.statusword as i64))
            }
            (device, "mode_of_operation") if device == self.axis => {
                Ok(Value::I64(self.probe.mode_of_operation as i64))
            }
            (device, "low_limit") if device == self.axis => Ok(position(self.probe.low_limit_um)),
            (device, "high_limit") if device == self.axis => Ok(position(self.probe.high_limit_um)),
            (device, "encoder_unit") if device == self.axis => {
                Ok(position(1.0 / self.probe.encoder_units_per_um))
            }
            (device, "axis_summary") if device == self.axis => Ok(self.axis_summary()),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Xeryon CANopen property {key}"),
            )),
        }
    }

    fn state_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("hub".into(), Value::I64(self.hub.0 .0 as i64)),
            ("node_id".into(), Value::I64(self.probe.node_id as i64)),
            (
                "can_backend".into(),
                Value::String(self.probe.can_backend.clone()),
            ),
            ("connected".into(), Value::Bool(self.connected)),
            (
                "device_profile".into(),
                Value::String(self.probe.device_profile.clone()),
            ),
            ("axis".into(), self.axis_summary()),
        ]))
    }

    fn axis_summary(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("device".into(), Value::I64(self.axis.0 .0 as i64)),
            (
                "stage_model".into(),
                Value::String(self.probe.stage_model.clone()),
            ),
            ("position".into(), position(self.probe.position_um)),
            ("target".into(), position(self.probe.target_um)),
            ("velocity".into(), velocity(self.probe.velocity_um_s)),
            (
                "acceleration".into(),
                acceleration(self.probe.acceleration_um_s2),
            ),
            (
                "statusword".into(),
                Value::I64(self.probe.statusword as i64),
            ),
            (
                "mode_of_operation".into(),
                Value::I64(self.probe.mode_of_operation as i64),
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
                let position_um =
                    position_um(value)?.clamp(self.probe.low_limit_um, self.probe.high_limit_um);
                self.move_absolute(position_um, None)?;
                Ok(position(self.probe.position_um))
            }
            (device, "target", value) if device == self.axis => {
                self.probe.target_um =
                    position_um(value)?.clamp(self.probe.low_limit_um, self.probe.high_limit_um);
                Ok(position(self.probe.target_um))
            }
            (device, "velocity", value) if device == self.axis => {
                self.probe.velocity_um_s = velocity_um_s(value)?.clamp(0.0, 500_000.0);
                let _ =
                    self.execute_transactions(vec![protocol::CanopenTransaction::SdoDownload {
                        object: protocol::PROFILE_VELOCITY,
                        value: protocol::SdoValue::U32(
                            self.probe.native_velocity(self.probe.velocity_um_s),
                        ),
                    }])?;
                Ok(velocity(self.probe.velocity_um_s))
            }
            (device, "acceleration", value) if device == self.axis => {
                self.probe.acceleration_um_s2 = acceleration_um_s2(value)?.clamp(0.0, 5_000_000.0);
                let _ =
                    self.execute_transactions(vec![protocol::CanopenTransaction::SdoDownload {
                        object: protocol::PROFILE_ACCELERATION,
                        value: protocol::SdoValue::U32(
                            self.probe
                                .native_acceleration(self.probe.acceleration_um_s2),
                        ),
                    }])?;
                Ok(acceleration(self.probe.acceleration_um_s2))
            }
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("invalid Xeryon CANopen write {key}"),
            )),
        }
    }

    fn move_absolute(&mut self, target_um: f64, profile: Option<&MotionProfile>) -> Result<Value> {
        if let Some(profile) = profile {
            if let Some(velocity) = profile.velocity {
                self.probe.velocity_um_s = velocity.micrometers_per_second().clamp(0.0, 500_000.0);
            }
            if let Some(acceleration) = profile.acceleration {
                self.probe.acceleration_um_s2 = acceleration
                    .micrometers_per_second_squared()
                    .clamp(0.0, 5_000_000.0);
            }
        }
        self.probe.target_um = target_um;
        if !self.connected {
            self.probe.position_um = target_um;
        }
        self.probe.mode_of_operation = protocol::PROFILE_POSITION_MODE;
        let transactions = protocol::profile_position_sequence(
            self.probe.native_position(target_um),
            Some(self.probe.native_velocity(self.probe.velocity_um_s)),
            Some(
                self.probe
                    .native_acceleration(self.probe.acceleration_um_s2),
            ),
        );
        let transactions = self.execute_transactions(transactions)?;
        if !self.connected {
            self.emit_property(self.axis, "position", position(self.probe.position_um));
        }
        self.emit_property(self.axis, "target", position(self.probe.target_um));
        Ok(Value::Map(BTreeMap::from([
            ("position".into(), position(self.probe.position_um)),
            ("target".into(), position(self.probe.target_um)),
            ("canopen_transactions".into(), transactions),
        ])))
    }

    fn stage_move(&mut self, request: &StageMoveRequest) -> Result<Value> {
        self.validate_stage_move(request)?;
        let requested_um = request
            .target
            .values()
            .next()
            .expect("validated one target")
            .micrometers();
        let target_um = if request.relative {
            self.probe.position_um + requested_um
        } else {
            requested_um
        }
        .clamp(self.probe.low_limit_um, self.probe.high_limit_um);
        self.move_absolute(target_um, request.profile.as_ref())
    }

    fn validate_stage_move(&self, request: &StageMoveRequest) -> Result<()> {
        if request.target.len() != 1 {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Xeryon CANopen StageMove expects exactly one axis target",
            ));
        }
        let axis = request
            .target
            .keys()
            .next()
            .expect("validated target exists");
        let supported_axis = match axis {
            StageAxis::X => true,
            StageAxis::Custom(name) => name.eq_ignore_ascii_case("x"),
            _ => false,
        };
        if !supported_axis {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "Xeryon CANopen StageMove supports the configured X axis",
            ));
        }
        Ok(())
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
                "unknown Xeryon CANopen capability",
            ));
        };
        match (capability.kind, request) {
            (CapabilityKind::StageMove, CapabilityRequest::StageMove(request))
                if device == self.axis =>
            {
                self.stage_move(&request)
            }
            (CapabilityKind::StageHome, CapabilityRequest::None) if device == self.axis => {
                self.probe.mode_of_operation = protocol::HOMING_MODE;
                let transactions =
                    self.execute_transactions(protocol::homing_sequence(self.probe.homing_method))?;
                if !self.connected {
                    self.probe.position_um = 0.0;
                    self.probe.target_um = 0.0;
                    self.emit_property(self.axis, "position", position(self.probe.position_um));
                    self.emit_property(self.axis, "target", position(self.probe.target_um));
                }
                Ok(Value::Map(BTreeMap::from([
                    ("position".into(), position(self.probe.position_um)),
                    ("target".into(), position(self.probe.target_um)),
                    ("canopen_transactions".into(), transactions),
                ])))
            }
            (CapabilityKind::StageStop, CapabilityRequest::None) if device == self.axis => {
                let transactions =
                    self.execute_transactions(vec![protocol::CanopenTransaction::SdoDownload {
                        object: protocol::CONTROLWORD,
                        value: protocol::SdoValue::U16(protocol::CW_QUICK_STOP),
                    }])?;
                Ok(Value::Map(BTreeMap::from([(
                    "canopen_transactions".into(),
                    transactions,
                )])))
            }
            (CapabilityKind::GenericCommand, CapabilityRequest::GenericCommand(request))
                if device == self.axis || device == self.hub =>
            {
                self.apply_generic_command(request)
            }
            (CapabilityKind::StageMove, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Xeryon CANopen StageMove expects a StageMoveRequest",
            )),
            (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Xeryon CANopen home/stop capabilities take no request",
            )),
            (CapabilityKind::GenericCommand, _) => Err(Error::new(
                ErrorCode::InvalidCommand,
                "Xeryon CANopen GenericCommand expects a GenericCommandRequest",
            )),
            _ => Err(Error::new(
                ErrorCode::Unsupported,
                "unsupported Xeryon CANopen capability",
            )),
        }
    }

    fn apply_generic_command(&mut self, request: GenericCommandRequest) -> Result<Value> {
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
                "Xeryon CANopen refresh commands do not take parameters",
            ));
        }
        match request.command.as_str() {
            "refresh_readbacks" | "refresh_status" | "refresh_axis_summary" => {
                let transactions = self.execute_transactions(protocol::refresh_sequence())?;
                Ok(Value::Map(BTreeMap::from([
                    ("command".into(), Value::String(request.command)),
                    ("canopen_transactions".into(), transactions),
                    ("axis_summary".into(), self.axis_summary()),
                ])))
            }
            other => Err(Error::new(
                ErrorCode::InvalidCommand,
                format!("unsupported Xeryon CANopen refresh command {other}"),
            )),
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
}

impl Driver for XeryonCanopenDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        self.descriptors_for()
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        let (label, kind) = if self.connected {
            (
                format!("xeryon-canopen-{}-bus", self.probe.can_backend),
                format!("canopen.{}", self.probe.can_backend),
            )
        } else {
            (
                "xeryon-canopen-planned-bus".into(),
                "canopen.planned".into(),
            )
        };
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label,
            kind,
            metadata: BTreeMap::from([
                ("node_id".into(), Value::I64(self.probe.node_id as i64)),
                (
                    "can_backend".into(),
                    Value::String(self.probe.can_backend.clone()),
                ),
                ("connected".into(), Value::Bool(self.connected)),
                (
                    "can_interface".into(),
                    self.probe
                        .can_interface
                        .as_ref()
                        .map(|interface| Value::String(interface.clone()))
                        .unwrap_or(Value::Null),
                ),
                (
                    "serial_port".into(),
                    self.probe
                        .serial_port
                        .as_ref()
                        .map(|port| Value::String(port.clone()))
                        .unwrap_or(Value::Null),
                ),
                (
                    "slcan_bitrate_code".into(),
                    self.probe
                        .slcan_bitrate_code
                        .map(|code| Value::String(code.to_string()))
                        .unwrap_or(Value::Null),
                ),
                ("slcan_open".into(), Value::Bool(self.probe.slcan_open)),
                (
                    "can_timeout".into(),
                    Value::TimeInterval(TimeInterval::from_milliseconds(
                        self.probe.can_timeout_ms as f64,
                    )),
                ),
                ("device_profile".into(), Value::String("CiA 402".into())),
                (
                    "transport".into(),
                    Value::String(if self.connected {
                        "live CANopen SDO/NMT".into()
                    } else {
                        "planned/configured".into()
                    }),
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
        } else if device == self.hub {
            vec![capability(5, device, CapabilityKind::GenericCommand)]
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
                        description: format!("xeryon canopen read {key}"),
                        payload: Value::String(key.clone()),
                    });
                }
                Command::WriteProperty { device, key, value } => {
                    self.validate_write(*device, key, value)?;
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("xeryon canopen write {key}"),
                        payload: value.clone(),
                    });
                }
                Command::ApplyStateSet(set) => {
                    for write in &set.writes {
                        self.validate_write(write.device, &write.property, &write.value)?;
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: "xeryon canopen state set".into(),
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
                            Error::new(ErrorCode::Unsupported, "unknown Xeryon CANopen capability")
                        })?;
                    match (&candidate.kind, request) {
                        (CapabilityKind::StageMove, CapabilityRequest::StageMove(request)) => {
                            self.validate_stage_move(request)?;
                        }
                        (
                            CapabilityKind::GenericCommand,
                            CapabilityRequest::GenericCommand(request),
                        ) => {
                            if request.is_hidden_maintenance() {
                                return Err(Error::new(
                                    ErrorCode::InvalidCommand,
                                    "hidden maintenance command",
                                ));
                            }
                        }
                        (
                            CapabilityKind::StageHome | CapabilityKind::StageStop,
                            CapabilityRequest::None,
                        ) => {}
                        (CapabilityKind::StageMove, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Xeryon CANopen StageMove expects a StageMoveRequest",
                            ));
                        }
                        (CapabilityKind::StageHome | CapabilityKind::StageStop, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Xeryon CANopen home/stop capabilities take no request",
                            ));
                        }
                        (CapabilityKind::GenericCommand, _) => {
                            return Err(Error::new(
                                ErrorCode::InvalidCommand,
                                "Xeryon CANopen GenericCommand expects a GenericCommandRequest",
                            ));
                        }
                        _ => {
                            return Err(Error::new(
                                ErrorCode::Unsupported,
                                "unsupported Xeryon CANopen capability",
                            ));
                        }
                    }
                    physical_transactions.push(PhysicalTransaction {
                        resource: Some(self.resource),
                        description: format!("xeryon canopen invoke {}", capability.0),
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
                    last = self.read_property(device, &key)?;
                }
                Command::WriteProperty { device, key, value } => {
                    last = self.write_property(device, &key, &value)?;
                    self.emit_property(device, &key, last.clone());
                }
                Command::ApplyStateSet(set) => {
                    for write in set.writes {
                        last = self.write_property(write.device, &write.property, &write.value)?;
                    }
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
                description: "xeryon canopen timing arm summary".into(),
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
                description: "xeryon canopen timing start sequence".into(),
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
                description: "xeryon canopen timing stop sequence".into(),
                payload: changed,
            }],
        })
    }
}

impl XeryonCanopenDriver {
    fn validate_timing_plan(&self, plan: &TimingPlan) -> Result<()> {
        for sequence in plan
            .sequences
            .iter()
            .filter(|sequence| sequence.device == self.axis)
        {
            if sequence.property != "position" {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    "Xeryon CANopen timing sequences can only target position",
                ));
            }
            for value in &sequence.values {
                let _ = position_um(value)?;
            }
        }
        Ok(())
    }

    fn apply_timing_sequence_step(&mut self, plan: &TimingPlan, first: bool) -> Result<Value> {
        let mut result = Value::Map(BTreeMap::new());
        let values = plan
            .sequences
            .iter()
            .filter(|sequence| sequence.device == self.axis)
            .filter_map(|sequence| {
                if first {
                    sequence.values.first()
                } else {
                    sequence.values.last()
                }
            })
            .cloned()
            .collect::<Vec<_>>();
        for value in values {
            result = self.move_absolute(
                position_um(&value)?.clamp(self.probe.low_limit_um, self.probe.high_limit_um),
                None,
            )?;
        }
        Ok(result)
    }
}

fn probe_from_config(device: &DeviceConfig) -> Result<XeryonCanopenProbe> {
    let mut probe = XeryonCanopenProbe::simulated();
    probe.label = if device.label.is_empty() {
        probe.label
    } else {
        device.label.clone()
    };
    probe.connect = bool_prop(device, "connect").unwrap_or(false);
    probe.can_backend = string_prop(device, "can_backend")
        .or_else(|| string_prop(device, "backend"))
        .unwrap_or_else(|| {
            if probe.connect {
                "socketcan".into()
            } else {
                "planned".into()
            }
        });
    probe.can_interface = string_prop(device, "can_interface");
    probe.serial_port = string_prop(device, "serial_port");
    probe.serial_baud_rate = u32_prop(device, "serial_baud_rate")
        .or_else(|| u32_prop(device, "baud_rate"))
        .unwrap_or(probe.serial_baud_rate);
    probe.slcan_bitrate_code = slcan_bitrate_code_prop(device, "slcan_bitrate_code")?;
    probe.slcan_open = bool_prop(device, "slcan_open").unwrap_or(false);
    probe.can_timeout_ms = u64_prop(device, "can_timeout_ms").unwrap_or(probe.can_timeout_ms);
    probe.node_id = u8_prop(device, "node_id").unwrap_or(probe.node_id);
    probe.stage_model = string_prop(device, "stage_model").unwrap_or(probe.stage_model);
    probe.device_profile = string_prop(device, "device_profile").unwrap_or(probe.device_profile);
    probe.encoder_units_per_um =
        f64_prop(device, "encoder_units_per_um").unwrap_or(probe.encoder_units_per_um);
    if probe.encoder_units_per_um <= 0.0 {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            "xeryon_canopen encoder_units_per_um must be positive",
        ));
    }
    probe.low_limit_um =
        position_config_um(device, "low_limit", "low_limit_um").unwrap_or(probe.low_limit_um);
    probe.high_limit_um =
        position_config_um(device, "high_limit", "high_limit_um").unwrap_or(probe.high_limit_um);
    probe.position_um =
        position_config_um(device, "position", "position_um").unwrap_or(probe.position_um);
    probe.target_um = position_config_um(device, "target", "target_um").unwrap_or(probe.target_um);
    probe.velocity_um_s =
        velocity_config_um_s(device, "velocity", "velocity_um_s").unwrap_or(probe.velocity_um_s);
    probe.acceleration_um_s2 =
        acceleration_config_um_s2(device, "acceleration", "acceleration_um_s2")
            .unwrap_or(probe.acceleration_um_s2);
    probe.statusword = u16_prop(device, "statusword").unwrap_or(probe.statusword);
    probe.mode_of_operation =
        i8_prop(device, "mode_of_operation").unwrap_or(probe.mode_of_operation);
    probe.homing_method = i8_prop(device, "homing_method");
    probe.eds_path = string_prop(device, "eds_path");
    if let Some(path) = &probe.eds_path {
        match parse_eds_file(path) {
            Ok(objects) => {
                probe.eds_status = format!("parsed {} object entries", objects.len());
                probe.eds_objects = objects;
            }
            Err(error) => {
                if bool_prop(device, "require_eds").unwrap_or(false) {
                    return Err(error);
                }
                probe.eds_status = format!("not parsed: {}", error.message);
            }
        }
    }
    Ok(probe)
}

fn open_can_transport(probe: &XeryonCanopenProbe) -> Result<Box<dyn CanIo>> {
    match probe.can_backend.as_str() {
        "socketcan" => open_socketcan_transport(probe),
        "slcan" => open_slcan_transport(probe),
        "planned" => Err(Error::new(
            ErrorCode::InvalidProperty,
            "xeryon_canopen connect=true requires can_backend socketcan or slcan",
        )),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unsupported Xeryon CANopen backend {other}"),
        )),
    }
}

#[cfg(all(feature = "os-can", target_os = "linux"))]
fn open_socketcan_transport(probe: &XeryonCanopenProbe) -> Result<Box<dyn CanIo>> {
    let interface = probe.can_interface.as_deref().ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidProperty,
            "socketcan backend requires can_interface, for example can0",
        )
    })?;
    Ok(Box::new(numanager_core::can::SocketCanIo::open(interface)?))
}

#[cfg(not(all(feature = "os-can", target_os = "linux")))]
fn open_socketcan_transport(_probe: &XeryonCanopenProbe) -> Result<Box<dyn CanIo>> {
    Err(Error::new(
        ErrorCode::Unsupported,
        "SocketCAN backend requires Linux and the numanager-drivers os-can feature",
    ))
}

#[cfg(feature = "os-serial")]
fn open_slcan_transport(probe: &XeryonCanopenProbe) -> Result<Box<dyn CanIo>> {
    let port_name = probe.serial_port.as_deref().ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidProperty,
            "slcan backend requires serial_port",
        )
    })?;
    let serial = numanager_core::serial::OsSerialPort::open_config(
        numanager_core::serial::OsSerialConfig::new(port_name, probe.serial_baud_rate)
            .timeout(Duration::from_millis(probe.can_timeout_ms)),
    )?;
    Ok(Box::new(numanager_core::can::SlcanIo::with_setup(
        Box::new(serial),
        probe.slcan_bitrate_code,
        probe.slcan_open,
    )?))
}

#[cfg(not(feature = "os-serial"))]
fn open_slcan_transport(_probe: &XeryonCanopenProbe) -> Result<Box<dyn CanIo>> {
    Err(Error::new(
        ErrorCode::Unsupported,
        "SLCAN backend requires the numanager-drivers os-serial feature",
    ))
}

fn wait_for_sdo_response(
    can: &mut dyn CanIo,
    node_id: u8,
    object: protocol::Object,
    timeout_ms: u64,
) -> Result<CanFrame> {
    let expected_id = 0x580 + node_id as u32;
    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms.max(1));
    loop {
        for frame in can.read_available()? {
            if frame.id != expected_id {
                continue;
            }
            if sdo_response_matches_object(&frame, object) {
                validate_sdo_abort(&frame)?;
                return Ok(frame);
            }
        }
        if std::time::Instant::now() >= deadline {
            return Err(Error::new(
                ErrorCode::Timeout,
                format!(
                    "timed out waiting for SDO response 0x{expected_id:03X} object 0x{:04X}:{}",
                    object.index, object.subindex
                ),
            ));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn sdo_response_matches_object(frame: &CanFrame, object: protocol::Object) -> bool {
    if frame.data.len() < 4 {
        return false;
    }
    let index = u16::from_le_bytes([frame.data[1], frame.data[2]]);
    index == object.index && frame.data[3] == object.subindex
}

fn validate_sdo_abort(frame: &CanFrame) -> Result<()> {
    if frame.data.first() == Some(&0x80) {
        let code = if frame.data.len() >= 8 {
            u32::from_le_bytes([frame.data[4], frame.data[5], frame.data[6], frame.data[7]])
        } else {
            0
        };
        return Err(Error::new(
            ErrorCode::Transport,
            format!("CANopen SDO abort 0x{code:08X}"),
        ));
    }
    Ok(())
}

fn validate_sdo_download_ack(object: protocol::Object, frame: &CanFrame) -> Result<()> {
    if frame.data.first() != Some(&0x60) {
        return Err(Error::new(
            ErrorCode::Transport,
            format!(
                "expected SDO download ACK for 0x{:04X}:{}, got {:?}",
                object.index, object.subindex, frame.data
            ),
        ));
    }
    Ok(())
}

fn parse_sdo_upload_i64(object: protocol::Object, frame: &CanFrame) -> Result<i64> {
    if frame.data.len() < 8 {
        return Err(Error::new(
            ErrorCode::Transport,
            "short SDO upload response",
        ));
    }
    validate_sdo_abort(frame)?;
    match frame.data[0] {
        0x4F => Ok(frame.data[4] as i8 as i64),
        0x4B => Ok(i16::from_le_bytes([frame.data[4], frame.data[5]]) as i64),
        0x43 => Ok(
            i32::from_le_bytes([frame.data[4], frame.data[5], frame.data[6], frame.data[7]]) as i64,
        ),
        other => Err(Error::new(
            ErrorCode::Transport,
            format!(
                "unsupported SDO upload response command 0x{other:02X} for 0x{:04X}:{}",
                object.index, object.subindex
            ),
        )),
    }
}

fn can_frame_value(frame: &CanFrame) -> Value {
    Value::Map(BTreeMap::from([
        ("id".into(), Value::String(format!("0x{:03X}", frame.id))),
        ("extended".into(), Value::Bool(frame.extended)),
        ("data".into(), Value::Bytes(frame.data.clone())),
    ]))
}

fn eds_object_value(object: &EdsObject) -> Value {
    Value::Map(BTreeMap::from([
        ("section".into(), Value::String(object.section.clone())),
        (
            "index".into(),
            Value::String(format!("0x{:04X}", object.index)),
        ),
        (
            "subindex".into(),
            object
                .subindex
                .map(|subindex| Value::I64(subindex as i64))
                .unwrap_or(Value::Null),
        ),
        (
            "parameter_name".into(),
            object
                .parameter_name
                .as_ref()
                .map(|value| Value::String(value.clone()))
                .unwrap_or(Value::Null),
        ),
        (
            "object_type".into(),
            object
                .object_type
                .as_ref()
                .map(|value| Value::String(value.clone()))
                .unwrap_or(Value::Null),
        ),
        (
            "data_type".into(),
            object
                .data_type
                .as_ref()
                .map(|value| Value::String(value.clone()))
                .unwrap_or(Value::Null),
        ),
        (
            "access_type".into(),
            object
                .access_type
                .as_ref()
                .map(|value| Value::String(value.clone()))
                .unwrap_or(Value::Null),
        ),
        (
            "default_value".into(),
            object
                .default_value
                .as_ref()
                .map(|value| Value::String(value.clone()))
                .unwrap_or(Value::Null),
        ),
        (
            "pdo_mapping".into(),
            object
                .pdo_mapping
                .as_ref()
                .map(|value| Value::String(value.clone()))
                .unwrap_or(Value::Null),
        ),
    ]))
}

fn parse_eds_file(path: &str) -> Result<Vec<EdsObject>> {
    let text = fs::read_to_string(path).map_err(|error| {
        Error::new(
            ErrorCode::InvalidProperty,
            format!("failed to read EDS file {path}: {error}"),
        )
    })?;
    Ok(parse_eds_text(&text))
}

fn parse_eds_text(text: &str) -> Vec<EdsObject> {
    let mut objects = Vec::new();
    let mut section = String::new();
    let mut fields = BTreeMap::new();
    for raw_line in text.lines() {
        let line = raw_line.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            push_eds_object(&section, &fields, &mut objects);
            section = line[1..line.len() - 1].trim().to_string();
            fields.clear();
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            fields.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    push_eds_object(&section, &fields, &mut objects);
    objects
}

fn push_eds_object(section: &str, fields: &BTreeMap<String, String>, objects: &mut Vec<EdsObject>) {
    let Some((index, subindex)) = parse_eds_section(section) else {
        return;
    };
    objects.push(EdsObject {
        section: section.into(),
        index,
        subindex,
        parameter_name: fields.get("parametername").cloned(),
        object_type: fields.get("objecttype").cloned(),
        data_type: fields.get("datatype").cloned(),
        access_type: fields.get("accesstype").cloned(),
        default_value: fields.get("defaultvalue").cloned(),
        pdo_mapping: fields.get("pdomapping").cloned(),
    });
}

fn parse_eds_section(section: &str) -> Option<(u16, Option<u8>)> {
    let (index, subindex) = match section.split_once("sub") {
        Some((index, subindex)) => (index, Some(subindex)),
        None => (section, None),
    };
    if index.len() != 4 || !index.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    let index = u16::from_str_radix(index, 16).ok()?;
    let subindex = subindex.and_then(|value| {
        if value.len() == 2 && value.chars().all(|ch| ch.is_ascii_hexdigit()) {
            u8::from_str_radix(value, 16).ok()
        } else {
            None
        }
    });
    Some((index, subindex))
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

fn slcan_bitrate_code_prop(device: &DeviceConfig, key: &str) -> Result<Option<char>> {
    let Some(value) = device.properties.get(key) else {
        return Ok(None);
    };
    let code = match value {
        Value::String(value) => value
            .trim()
            .trim_start_matches('S')
            .trim_start_matches('s')
            .to_string(),
        Value::I64(value) => value.to_string(),
        other => {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!(
                    "SLCAN bitrate code expects String or I64, got {:?}",
                    other.value_type()
                ),
            ))
        }
    };
    let mut chars = code.chars();
    let Some(code) = chars.next() else {
        return Ok(None);
    };
    if chars.next().is_some() || !matches!(code, '0'..='8') {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("SLCAN bitrate code must be 0..8 or S0..S8, got {code}"),
        ));
    }
    Ok(Some(code))
}

fn f64_prop(device: &DeviceConfig, key: &str) -> Option<f64> {
    match device.properties.get(key) {
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn u8_prop(device: &DeviceConfig, key: &str) -> Option<u8> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => (*value).try_into().ok(),
        _ => None,
    }
}

fn u16_prop(device: &DeviceConfig, key: &str) -> Option<u16> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => (*value).try_into().ok(),
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

fn i8_prop(device: &DeviceConfig, key: &str) -> Option<i8> {
    match device.properties.get(key) {
        Some(Value::I64(value)) => (*value).try_into().ok(),
        _ => None,
    }
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

fn acceleration_config_um_s2(device: &DeviceConfig, key: &str, legacy_key: &str) -> Option<f64> {
    match device
        .properties
        .get(key)
        .or_else(|| device.properties.get(legacy_key))
    {
        Some(Value::Acceleration(value)) => Some(value.micrometers_per_second_squared()),
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

fn acceleration_um_s2(value: &Value) -> Result<f64> {
    match value {
        Value::Acceleration(value) => Ok(value.micrometers_per_second_squared()),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("expected Acceleration value, got {:?}", other.value_type()),
        )),
    }
}
