#[cfg(feature = "os-usb")]
use numanager_core::runtime::{DriverCandidate, DriverDiscovery};
use numanager_core::*;
#[cfg(feature = "os-usb")]
use std::collections::{BTreeMap, VecDeque};

pub const CYPRESS_VID: u16 = 0x04b4;
pub const CYPRESS_FX2_PID: u16 = 0x8613;
pub const CYPRESS_FX3_PID: u16 = 0x00f3;

/// EZ-USB anchor-download vendor request, and the `CPUCS` register that holds
/// (`1`) or releases (`0`) the 8051. Both are properties of the Cypress part,
/// not of any vendor's device, so every FX2/FX3 driver here writes the same
/// two values in the same order.
pub const REQ_ANCHOR: u8 = 0xA0;
pub const CPUCS: u16 = 0xE600;

/// One Intel-HEX data record: where the bytes go, and the bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexRecord {
    pub address: u16,
    pub data: Vec<u8>,
}

/// Parse Intel-HEX text into its data records (type `00`), in file order,
/// stopping at the end-of-file record (type `01`).
///
/// Records are returned individually rather than coalesced into segments: a
/// driver that must reproduce a recorded download byte-for-byte needs the
/// original record boundaries, and one that would rather write larger blocks
/// can always merge them itself. Extended-address records are rejected instead
/// of being silently ignored, since a firmware image that uses them would be
/// loaded to the wrong place.
pub fn parse_ihex(text: &str) -> Result<Vec<HexRecord>> {
    let mut out = Vec::new();
    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let line_number = lineno + 1;
        let body = line.strip_prefix(':').ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!("Intel-HEX line {line_number}: missing ':' start code"),
            )
        })?;
        if body.len() < 10 || body.len() % 2 != 0 {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Intel-HEX line {line_number}: malformed record"),
            ));
        }
        let bytes = (0..body.len() / 2)
            .map(|i| u8::from_str_radix(&body[i * 2..i * 2 + 2], 16))
            .collect::<std::result::Result<Vec<u8>, _>>()
            .map_err(|error| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Intel-HEX line {line_number}: bad hex: {error}"),
                )
            })?;
        let len = bytes[0] as usize;
        if bytes.len() != len + 5 {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Intel-HEX line {line_number}: length byte disagrees with record"),
            ));
        }
        // Two's complement of the sum of every byte but the checksum itself.
        let sum = bytes[..bytes.len() - 1]
            .iter()
            .fold(0u8, |acc, byte| acc.wrapping_add(*byte));
        if sum.wrapping_neg() != bytes[bytes.len() - 1] {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("Intel-HEX line {line_number}: checksum mismatch"),
            ));
        }
        match bytes[3] {
            0x00 => out.push(HexRecord {
                address: u16::from_be_bytes([bytes[1], bytes[2]]),
                data: bytes[4..4 + len].to_vec(),
            }),
            0x01 => break,
            other => {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("Intel-HEX line {line_number}: unsupported record type {other:#04x}"),
                ))
            }
        }
    }
    Ok(out)
}

/// One anchor-download write: `bRequest 0xA0`, `wValue` = target address.
#[cfg(feature = "os-usb")]
pub fn anchor_write(
    interface: &nusb::Interface,
    address: u16,
    data: &[u8],
    timeout: std::time::Duration,
) -> Result<()> {
    use nusb::transfer::{Control, ControlType, Recipient};

    let control = Control {
        control_type: ControlType::Vendor,
        recipient: Recipient::Device,
        request: REQ_ANCHOR,
        value: address,
        index: 0,
    };
    let sent = interface
        .control_out_blocking(control, data, timeout)
        .map_err(|error| {
            Error::new(
                ErrorCode::Transport,
                format!("EZ-USB anchor write to {address:#06x} failed: {error}"),
            )
        })?;
    if sent != data.len() {
        return Err(Error::new(
            ErrorCode::Transport,
            format!(
                "EZ-USB anchor write to {address:#06x} short: {sent}/{}",
                data.len()
            ),
        ));
    }
    Ok(())
}

/// Hold the 8051 in reset (`true`) or release it (`false`). Releasing is what
/// makes the part renumerate under its firmware's identity.
#[cfg(feature = "os-usb")]
pub fn hold_8051(
    interface: &nusb::Interface,
    held: bool,
    timeout: std::time::Duration,
) -> Result<()> {
    anchor_write(interface, CPUCS, &[u8::from(held)], timeout)
}

/// USB vendor ids this passive loader discovery claims.
pub fn usb_vendor_ids() -> Vec<u16> {
    vec![CYPRESS_VID]
}

#[cfg(feature = "os-usb")]
pub struct EzUsbLoaderDiscovery {
    next_id: DriverId,
}

#[cfg(feature = "os-usb")]
impl EzUsbLoaderDiscovery {
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

#[cfg(feature = "os-usb")]
fn ez_usb_loader_name(product_id: u16) -> &'static str {
    match product_id {
        CYPRESS_FX2_PID => "Generic Cypress EZ-USB FX2 pre-firmware device",
        CYPRESS_FX3_PID => "Generic Cypress EZ-USB FX3 pre-firmware device",
        _ => "Generic Cypress EZ-USB pre-firmware device",
    }
}

#[cfg(feature = "os-usb")]
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

#[cfg(feature = "os-usb")]
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

#[cfg(feature = "os-usb")]
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

#[cfg(feature = "os-usb")]
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

#[cfg(feature = "os-usb")]
fn string_property(key: &str, display_name: &str) -> PropertySchema {
    property(key, display_name, ValueType::String)
}
