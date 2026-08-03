use std::collections::BTreeMap;

use numanager_core::{
    CapabilityDescriptor, CapabilityExposure, CapabilityKind, Decibel, DeviceDescriptor, DeviceId,
    DeviceSequence, Driver, DriverId, Error, ErrorCode, Event, NumericalAperture, OperationStatus,
    PixelCount, PropertySchema, Ratio, Result, StateWrite, Value, ValueType,
};

pub fn boxed_driver<F>(factory: F) -> Result<Box<dyn Driver>>
where
    F: FnOnce(DriverId) -> Result<Box<dyn Driver>>,
{
    factory(DriverId(1))
}

pub fn driver_value<D, F>(factory: F) -> D
where
    F: FnOnce(DriverId) -> D,
{
    factory(DriverId(1))
}

pub fn example_arg(index: usize) -> Option<String> {
    std::env::args().skip(2).nth(index)
}

pub fn completion_summary(value: &Value) -> String {
    match value {
        Value::Map(map) => {
            if let Some(result) = single_public_result(map) {
                return completion_summary(result);
            }
            let mut key_counts = BTreeMap::<String, usize>::new();
            for key in map
                .keys()
                .filter(|key| !is_diagnostic_completion_key(key))
                .map(|key| public_completion_key(key))
            {
                *key_counts.entry(key).or_default() += 1;
            }
            let keys = key_counts
                .into_iter()
                .map(|(key, count)| {
                    if count == 1 {
                        key
                    } else {
                        format!("{key} x{count}")
                    }
                })
                .collect::<Vec<_>>();
            if keys.is_empty() {
                "driver-owned completion".into()
            } else {
                format!("map keys=[{}]", keys.join(", "))
            }
        }
        Value::List(values) => format!("list len={}", values.len()),
        Value::Bytes(bytes) => format!("bytes len={}", bytes.len()),
        other => format!("{other:?}"),
    }
}

fn single_public_result(map: &BTreeMap<String, Value>) -> Option<&Value> {
    let mut public_entries = map
        .iter()
        .filter(|(key, _)| !is_diagnostic_completion_key(key));
    let (key, value) = public_entries.next()?;
    (public_entries.next().is_none() && key == "result").then_some(value)
}

fn is_diagnostic_completion_key(key: &str) -> bool {
    matches!(key, "commands" | "physical_transactions" | "registers")
        || is_diagnostic_property_key(key)
}

fn public_completion_key(key: &str) -> String {
    match key.split_once(':') {
        Some((prefix, property)) if prefix.chars().all(|ch| ch.is_ascii_digit()) => {
            property.to_string()
        }
        _ => key.to_string(),
    }
}

pub fn operation_status_summary(status: &OperationStatus) -> String {
    match status {
        OperationStatus::Queued => "queued".into(),
        OperationStatus::Running { progress } => match progress {
            Some(progress) => format!("running progress={progress:?}"),
            None => "running".into(),
        },
        OperationStatus::Completed(value) => format!("completed {}", completion_summary(value)),
        OperationStatus::Failed(report) => format!("failed {:?}: {}", report.code, report.message),
        OperationStatus::Cancelled => "cancelled".into(),
        OperationStatus::TimedOut => "timed out".into(),
        OperationStatus::Unknown => "unknown".into(),
    }
}

pub fn event_summary(devices: &[DeviceDescriptor], event: &Event) -> String {
    match event {
        Event::OperationChanged(event) => format!(
            "operation on [{}] {}",
            device_label_list(devices, &event.devices),
            operation_status_summary(&event.status)
        ),
        Event::PropertyChanged(event) => format!(
            "{}.{} changed to {:?}",
            device_label_or_id(devices, event.device),
            event.key,
            event.value
        ),
        Event::FrameReady(event) => format!(
            "frame ready from {} {}x{} {}",
            device_label_or_id(devices, event.device),
            event.width,
            event.height,
            event.pixel_format
        ),
        Event::ScanSignalChunk(event) => format!(
            "scan signal chunk from {} stream={} line={} chunk={} first_sample={} samples={} sample_period_s={:.9} channels={}",
            device_label_or_id(devices, event.device),
            event.stream.0,
            event.line,
            event.chunk,
            event.first_sample,
            event.sample_count,
            event.sample_period.seconds(),
            event.channels.join(",")
        ),
        Event::Telemetry(event) => format!(
            "telemetry from {}: {}",
            device_label_or_id(devices, event.device),
            metadata_key_summary(&event.values)
        ),
        Event::DeviceArrived(device) => format!("device arrived: {}", device.label),
        Event::DeviceRemoved(device) => {
            format!("device removed: {}", device_label_or_id(devices, *device))
        }
        Event::Fault(event) => match event.device {
            Some(device) => format!(
                "fault on {}: {:?}: {}",
                device_label_or_id(devices, device),
                event.report.code,
                event.report.message
            ),
            None => format!("fault: {:?}: {}", event.report.code, event.report.message),
        },
        Event::Log(event) => format!("driver log: {}", event.message),
    }
}

pub fn public_kind_tags(device: &DeviceDescriptor) -> Vec<&str> {
    device
        .kinds
        .iter()
        .filter_map(|kind| {
            if kind.contains("raw") || kind.contains("register") || kind.contains("protocol") {
                None
            } else {
                Some(kind.as_str())
            }
        })
        .collect()
}

pub fn public_kind_summary(device: &DeviceDescriptor) -> String {
    let tags = public_kind_tags(device);
    if tags.is_empty() {
        "none".into()
    } else {
        tags.join(", ")
    }
}

pub fn metadata_key_summary(metadata: &BTreeMap<String, Value>) -> String {
    metadata.keys().cloned().collect::<Vec<_>>().join(", ")
}

pub fn public_capability_summary(capabilities: Vec<CapabilityDescriptor>) -> String {
    let names = capabilities
        .into_iter()
        .filter(|capability| capability.exposure() == CapabilityExposure::User)
        .map(|capability| capability_brief(&capability))
        .collect::<Vec<_>>();
    if names.is_empty() {
        "none".into()
    } else {
        names.join(", ")
    }
}

pub fn is_public_property(property: &PropertySchema) -> bool {
    !is_diagnostic_property_key(&property.key)
}

pub fn is_diagnostic_property_key(key: &str) -> bool {
    matches!(key, "protocol" | "raw" | "wire" | "tdcl" | "serial_frames")
        || key.ends_with("_raw")
        || key.starts_with("last_")
        || key.ends_with("_transaction")
        || key == "command_count"
        || key == "reply_report_count"
}

pub fn capability_brief(capability: &CapabilityDescriptor) -> String {
    format!(
        "{:?} request={:?}",
        capability.kind,
        capability.preferred_request_kind()
    )
}

pub fn device_by_kind<'a>(
    devices: &'a [DeviceDescriptor],
    kind: &str,
) -> Result<&'a DeviceDescriptor> {
    devices
        .iter()
        .find(|device| device.has_kind(kind))
        .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, format!("missing {kind} device")))
}

pub fn device_by_kinds<'a>(
    devices: &'a [DeviceDescriptor],
    kinds: &[&str],
) -> Result<&'a DeviceDescriptor> {
    devices
        .iter()
        .find(|device| device.has_kinds(kinds))
        .ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidCommand,
                format!("missing device with kinds {}", kinds.join(", ")),
            )
        })
}

pub fn device_by_id(devices: &[DeviceDescriptor], id: DeviceId) -> Result<&DeviceDescriptor> {
    devices
        .iter()
        .find(|device| device.id == id)
        .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, format!("missing device {id:?}")))
}

pub fn device_label_or_id(devices: &[DeviceDescriptor], id: DeviceId) -> String {
    devices
        .iter()
        .find(|device| device.id == id)
        .map(|device| device.label.clone())
        .unwrap_or_else(|| format!("{id:?}"))
}

pub fn device_label_list(devices: &[DeviceDescriptor], ids: &[DeviceId]) -> String {
    ids.iter()
        .map(|id| device_label_or_id(devices, *id))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn driver_device_by_capability(
    driver: &dyn Driver,
    devices: &[DeviceDescriptor],
    kind: CapabilityKind,
) -> Result<DeviceDescriptor> {
    devices
        .iter()
        .find(|device| {
            driver
                .capabilities(device.id)
                .iter()
                .any(|capability| capability.kind == kind)
        })
        .cloned()
        .ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidCommand,
                format!("missing device with capability {}", kind.name()),
            )
        })
}

pub fn driver_capability_by_kind(
    driver: &dyn Driver,
    device: impl Into<DeviceId>,
    kind: CapabilityKind,
) -> Result<CapabilityDescriptor> {
    let device = device.into();
    driver
        .capabilities(device)
        .into_iter()
        .find(|capability| capability.kind == kind)
        .ok_or_else(|| {
            Error::new(
                ErrorCode::Unsupported,
                format!("device {:?} does not expose {}", device, kind.name()),
            )
        })
}

pub fn capability_descriptor_by_kind(
    capabilities: &[CapabilityDescriptor],
    kind: CapabilityKind,
) -> Result<CapabilityDescriptor> {
    capabilities
        .iter()
        .find(|capability| capability.kind == kind)
        .cloned()
        .ok_or_else(|| Error::new(ErrorCode::Unsupported, format!("missing {}", kind.name())))
}

pub fn property<'a>(device: &'a DeviceDescriptor, key: &str) -> Option<&'a PropertySchema> {
    device
        .properties
        .iter()
        .find(|property| property.key == key)
}

pub fn schema_state_write(
    device: &DeviceDescriptor,
    key: &str,
    value: Value,
) -> Option<StateWrite> {
    let schema = property(device, key)?;
    (schema.writable && schema.validate(&value).is_ok())
        .then(|| StateWrite::new(device, key, value))
}

pub fn push_schema_write(
    device: &DeviceDescriptor,
    writes: &mut Vec<StateWrite>,
    key: &str,
    value: Value,
) {
    if let Some(write) = schema_state_write(device, key, value) {
        writes.push(write);
    }
}

pub fn schema_numeric_value(device: &DeviceDescriptor, key: &str, value: f64) -> Option<Value> {
    let schema = property(device, key)?;
    Some(match schema.value_type {
        ValueType::I64 => Value::I64(value.round() as i64),
        ValueType::F64 => Value::F64(value),
        ValueType::Decibel => Value::Decibel(Decibel::new(value)),
        ValueType::PixelCount => Value::PixelCount(PixelCount::new(value.round() as u32)),
        ValueType::Ratio => Value::Ratio(Ratio::from_percent(value)),
        ValueType::NumericalAperture => Value::NumericalAperture(NumericalAperture::new(value)),
        _ => return None,
    })
}

pub fn schema_numeric_pair(
    device: &DeviceDescriptor,
    key: &str,
    first: f64,
    second: f64,
) -> Vec<Value> {
    match (
        schema_numeric_value(device, key, first),
        schema_numeric_value(device, key, second),
    ) {
        (Some(first), Some(second)) => vec![first, second],
        _ => Vec::new(),
    }
}

pub fn push_schema_sequence(
    device: &DeviceDescriptor,
    sequences: &mut Vec<DeviceSequence>,
    key: &str,
    values: Vec<Value>,
) {
    let Some(schema) = property(device, key) else {
        return;
    };
    if schema.sequenceable
        && !values.is_empty()
        && values.iter().all(|value| schema.validate(value).is_ok())
    {
        sequences.push(DeviceSequence {
            device: device.into(),
            property: key.into(),
            values,
        });
    }
}
