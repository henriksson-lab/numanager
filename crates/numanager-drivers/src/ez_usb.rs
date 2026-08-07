use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::*;
use std::collections::{BTreeMap, VecDeque};

pub const CYPRESS_VID: u16 = 0x04b4;
pub const CYPRESS_FX2_PID: u16 = 0x8613;
pub const CYPRESS_FX3_PID: u16 = 0x00f3;

/// USB vendor ids this passive loader discovery claims.
pub fn usb_vendor_ids() -> Vec<u16> {
    vec![CYPRESS_VID]
}

pub struct EzUsbLoaderDiscovery {
    next_id: DriverId,
}

impl EzUsbLoaderDiscovery {
    #[cfg(feature = "os-usb")]
    pub fn os_usb(next_id: DriverId) -> Self {
        Self { next_id }
    }
}

#[cfg(feature = "os-usb")]
impl DriverDiscovery for EzUsbLoaderDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
        let devices = nusb::list_devices().map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("EZ-USB pre-firmware device listing failed: {error}"),
            )
        })?;
        Ok(devices
            .filter(|device| is_ez_usb_loader(device.vendor_id(), device.product_id()))
            .enumerate()
            .map(|(index, device)| {
                let vendor_id = device.vendor_id();
                let product_id = device.product_id();
                let product = device
                    .product_string()
                    .map(str::to_string)
                    .unwrap_or_else(|| ez_usb_loader_name(product_id).into());
                let serial_number = device.serial_number().map(str::to_string);
                let label = format!(
                    "{} {:04x}:{:04x} bus {} addr {}",
                    product,
                    vendor_id,
                    product_id,
                    device.bus_number(),
                    device.device_address()
                );
                let driver = EzUsbLoaderDriver {
                    id: DriverId(self.next_id.0 + index as u64),
                    device: DeviceId(NodeId((self.next_id.0 + index as u64) * 1000 + 930)),
                    label: label.clone(),
                    product,
                    serial_number,
                    vendor_id,
                    product_id,
                    bus_number: device.bus_number(),
                    device_address: device.device_address(),
                    pending: VecDeque::new(),
                };
                DriverCandidate::from_driver(label, Box::new(driver))
            })
            .collect())
    }
}

#[cfg(feature = "os-usb")]
fn is_ez_usb_loader(vendor_id: u16, product_id: u16) -> bool {
    vendor_id == CYPRESS_VID && matches!(product_id, CYPRESS_FX2_PID | CYPRESS_FX3_PID)
}

fn ez_usb_loader_name(product_id: u16) -> &'static str {
    match product_id {
        CYPRESS_FX2_PID => "Generic Cypress EZ-USB FX2 pre-firmware device",
        CYPRESS_FX3_PID => "Generic Cypress EZ-USB FX3 pre-firmware device",
        _ => "Generic Cypress EZ-USB pre-firmware device",
    }
}

struct EzUsbLoaderDriver {
    id: DriverId,
    device: DeviceId,
    label: String,
    product: String,
    serial_number: Option<String>,
    vendor_id: u16,
    product_id: u16,
    bus_number: u8,
    device_address: u8,
    pending: VecDeque<DriverEvent>,
}

impl EzUsbLoaderDriver {
    fn metadata(&self) -> BTreeMap<String, Value> {
        BTreeMap::from([
            ("vendor_id".into(), Value::I64(self.vendor_id as i64)),
            ("product_id".into(), Value::I64(self.product_id as i64)),
            ("bus_number".into(), Value::I64(self.bus_number as i64)),
            (
                "device_address".into(),
                Value::I64(self.device_address as i64),
            ),
            ("usb_stage".into(), Value::String("pre_firmware".into())),
            (
                "usb_identity_confidence".into(),
                Value::String("ambiguous".into()),
            ),
            (
                "identity_state".into(),
                Value::String("pre_firmware_usb".into()),
            ),
            ("active_probe_available".into(), Value::Bool(true)),
            ("active_probe_required_for_driver".into(), Value::Bool(true)),
            (
                "possible_drivers".into(),
                Value::List(self.possible_drivers()),
            ),
            (
                "source_evidence".into(),
                Value::String("cypress_fx2_trm_enumeration_and_renumeration".into()),
            ),
        ])
    }

    fn possible_drivers(&self) -> Vec<Value> {
        match self.product_id {
            CYPRESS_FX2_PID => vec![
                Value::String("andor_sdk2".into()),
                Value::String("mcl".into()),
                Value::String("other_ez_usb_fx2".into()),
            ],
            CYPRESS_FX3_PID => vec![
                Value::String("andor_sdk3".into()),
                Value::String("other_ez_usb_fx3".into()),
            ],
            _ => vec![Value::String("other_ez_usb".into())],
        }
    }

    fn read_property(&self, device: DeviceId, key: &str) -> Result<Value> {
        if device != self.device {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "unknown EZ-USB device",
            ));
        }
        match key {
            "product" => Ok(Value::String(self.product.clone())),
            "serial_number" => Ok(self
                .serial_number
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null)),
            "vendor_id" => Ok(Value::I64(self.vendor_id as i64)),
            "product_id" => Ok(Value::I64(self.product_id as i64)),
            "usb_stage" => Ok(Value::String("pre_firmware".into())),
            "usb_identity_confidence" => Ok(Value::String("ambiguous".into())),
            "identity_state" => Ok(Value::String("pre_firmware_usb".into())),
            "active_probe_available" => Ok(Value::Bool(true)),
            "active_probe_required_for_driver" => Ok(Value::Bool(true)),
            "usb_identity" => Ok(Value::Map(self.metadata())),
            _ => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown EZ-USB pre-firmware property {key}"),
            )),
        }
    }
}

impl Driver for EzUsbLoaderDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        vec![DeviceDescriptor {
            id: self.device,
            driver: self.id,
            label: self.label.clone(),
            vendor: Some("Cypress".into()),
            model: Some(self.product.clone()),
            serial: self.serial_number.clone(),
            kinds: vec![
                "usb.pre_firmware".into(),
                "usb.ez_usb".into(),
                "state.device".into(),
            ],
            properties: vec![
                string_property("product", "Product"),
                string_property("serial_number", "Serial number"),
                property("vendor_id", "USB vendor ID", ValueType::I64),
                property("product_id", "USB product ID", ValueType::I64),
                property("usb_identity", "USB identity", ValueType::Map),
                string_property("usb_stage", "USB stage"),
                string_property("usb_identity_confidence", "USB identity confidence"),
                string_property("identity_state", "Identity state"),
                property(
                    "active_probe_available",
                    "Active probe available",
                    ValueType::Bool,
                ),
                property(
                    "active_probe_required_for_driver",
                    "Active probe required for driver",
                    ValueType::Bool,
                ),
            ],
            metadata: self.metadata(),
        }]
    }

    fn capabilities(&self, _device: DeviceId) -> Vec<CapabilityDescriptor> {
        Vec::new()
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        for command in &batch.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    let _ = self.read_property(*device, key)?;
                }
                _ => return Err(Error::new(
                    ErrorCode::Unsupported,
                    "EZ-USB pre-firmware discovery is passive and exposes no hardware operations",
                )),
            }
        }
        Ok(PreparedBatch {
            id: batch.id,
            commands: batch.commands.clone(),
            physical_transactions: Vec::new(),
        })
    }

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = DriverToken(prepared.id.0);
        let mut last = Value::Null;
        for command in prepared.commands {
            match command {
                Command::ReadProperty { device, key } => {
                    last = self.read_property(device, &key)?;
                }
                _ => unreachable!("validated EZ-USB pre-firmware command"),
            }
        }
        self.pending
            .push_back(DriverEvent::TokenCompleted { token, value: last });
        Ok(token)
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        self.pending.drain(..).collect()
    }
}

fn property(key: &str, display_name: &str, value_type: ValueType) -> PropertySchema {
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

fn string_property(key: &str, display_name: &str) -> PropertySchema {
    property(key, display_name, ValueType::String)
}
