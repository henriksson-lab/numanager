//! User-assigned names for the discrete positions of a device.
//!
//! A filter wheel reports that it has six positions. It cannot report that position 3 holds a
//! 600/30 bandpass, because nothing in the hardware knows: someone unscrewed a filter and put
//! another one in. The same is true of a dichroic turret, a valve selector, and an objective
//! nosepiece on any scope whose turret has no encoder chip.
//!
//! So the names are configuration, not discovery. They are stored per device in
//! [`crate::config::HardwareConfig`] under [`SLOT_LABELS_PROPERTY`] and applied here onto the
//! positional property's `enum_values`, which is where every consumer already looks for the
//! choices a property accepts. An application that renders a property editor from the schema
//! shows "600/30" instead of "3" without knowing this module exists.
//!
//! Positions are numbered from the low end of the property's declared range — position 1 on a
//! wheel declaring `1..=6`, position 0 on one declaring `0..=5` — so the labels line up with
//! what the device itself calls its positions rather than with a list index.

use crate::{EnumValue, PropertySchema, Value};
use std::collections::BTreeMap;

/// The device property that user-assigned position names are stored under.
pub const SLOT_LABELS_PROPERTY: &str = "slot_labels";

/// Render a configured value as the text a person typed for it.
///
/// Config parsing promotes `"500 nm"` to a [`Value::Wavelength`], which is right for a
/// setpoint and wrong for a label — someone naming a slot "500 nm" wants those characters
/// back. Reconstructing the text from the typed value returns it unchanged.
pub fn value_label(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(v) => v.to_string(),
        Value::I64(v) => v.to_string(),
        Value::F64(v) => v.to_string(),
        Value::Temperature(v) => format!("{} {}", v.value, v.unit_symbol()),
        Value::Position(v) => format!("{} {}", v.value, v.unit_symbol()),
        Value::Velocity(v) => format!("{} {}", v.value, v.unit_symbol()),
        Value::Acceleration(v) => format!("{} {}", v.value, v.unit_symbol()),
        Value::TimeInterval(v) => format!("{} {}", v.value, v.unit_symbol()),
        Value::Wavelength(v) => format!("{} {}", v.value, v.unit_symbol()),
        Value::OpticalPower(v) => format!("{} {}", v.value, v.unit_symbol()),
        Value::ElectricCurrent(v) => format!("{} {}", v.value, v.unit_symbol()),
        Value::Voltage(v) => format!("{} {}", v.value, v.unit_symbol()),
        Value::Frequency(v) => format!("{} {}", v.value, v.unit_symbol()),
        Value::Decibel(v) => format!("{} {}", v.value, v.unit_symbol()),
        Value::Ratio(v) => format!("{} {}", v.value, v.unit_symbol()),
        Value::Pressure(v) => format!("{} {}", v.value, v.unit_symbol()),
        Value::GasConcentration(v) => format!("{} {}", v.value, v.unit_symbol()),
        Value::Volume(v) => format!("{} {}", v.value, v.unit_symbol()),
        Value::FlowRate(v) => format!("{} {}", v.value, v.unit_symbol()),
        Value::Timestamp(v) => format!("{} {}", v.value, v.unit_symbol()),
        Value::PixelCount(v) => format!("{} px", v.pixels()),
        Value::ByteCount(v) => format!("{} {}", v.bytes(), v.unit_symbol()),
        Value::StepCount(v) => format!("{} {}", v.steps(), v.unit_symbol()),
        Value::ControllerScalar(v) => format!("{} {}", v.value(), v.unit_symbol()),
        Value::NumericalAperture(v) => v.value().to_string(),
        Value::Bytes(v) => format!("{} bytes", v.len()),
        Value::List(items) => items.iter().map(value_label).collect::<Vec<_>>().join(", "),
        Value::Map(_) | Value::Null => String::new(),
    }
}

/// The slot names configured for a device, in position order.
///
/// Empty when the device has none configured, which is the normal case for hardware nobody has
/// labelled yet — the caller shows bare position numbers rather than inventing names.
pub fn slot_labels(properties: &BTreeMap<String, Value>) -> Vec<String> {
    match properties.get(SLOT_LABELS_PROPERTY) {
        Some(Value::List(items)) => items.iter().map(value_label).collect(),
        // A single-slot device, or a config written by hand without the brackets.
        Some(other) => vec![value_label(other)],
        None => Vec::new(),
    }
}

/// Store slot names so they persist through [`crate::config::HardwareConfig::save`].
pub fn set_slot_labels(properties: &mut BTreeMap<String, Value>, labels: &[String]) {
    if labels.iter().all(|label| label.trim().is_empty()) {
        properties.remove(SLOT_LABELS_PROPERTY);
        return;
    }
    properties.insert(
        SLOT_LABELS_PROPERTY.to_string(),
        Value::List(
            labels
                .iter()
                .map(|label| Value::String(label.clone()))
                .collect(),
        ),
    );
}

/// The position numbers `schema` accepts, from its declared range.
///
/// Falls back to `1..=labels_len` when the property declares no range, so a driver that
/// publishes a bare position property still gets usable choices.
fn positions(schema: &PropertySchema, labels_len: usize) -> Vec<i64> {
    let bounds =
        schema
            .range
            .as_ref()
            .and_then(|range| match (as_i64(&range.min), as_i64(&range.max)) {
                (Some(min), Some(max)) if max >= min => Some((min, max)),
                _ => None,
            });
    match bounds {
        Some((min, max)) => (min..=max).collect(),
        None => (1..=labels_len as i64).collect(),
    }
}

fn as_i64(value: &Value) -> Option<i64> {
    match value {
        Value::I64(v) => Some(*v),
        Value::F64(v) => Some(*v as i64),
        Value::StepCount(v) => Some(v.steps() as i64),
        _ => None,
    }
}

/// Apply user-assigned names to a positional property's `enum_values`.
///
/// Positions with no name keep a plain number, so a wheel where only two slots have been
/// labelled still offers all six — a half-filled config must not hide hardware.
///
/// Does nothing when `labels` is empty: a driver that publishes real `enum_values` of its own
/// (a Kurios trigger mode, a motorised nosepiece that reads its objectives) keeps them.
pub fn apply_slot_labels(schema: &mut PropertySchema, labels: &[String]) {
    if labels.is_empty() {
        return;
    }
    schema.enum_values = positions(schema, labels.len())
        .into_iter()
        .enumerate()
        .map(|(index, position)| {
            let label = labels
                .get(index)
                .map(|label| label.trim())
                .filter(|label| !label.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| position.to_string());
            EnumValue {
                value: Value::I64(position),
                label,
            }
        })
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Range, ValueType};

    fn wheel_position(min: i64, max: i64) -> PropertySchema {
        PropertySchema {
            key: "position".into(),
            display_name: "Position".into(),
            value_type: ValueType::I64,
            unit: None,
            range: Some(Range {
                min: Value::I64(min),
                max: Value::I64(max),
            }),
            increment: None,
            enum_values: Vec::new(),
            readable: true,
            writable: true,
            volatile: false,
            sequenceable: false,
            hardware_address: None,
        }
    }

    #[test]
    fn labels_land_on_the_positions_the_device_declares() {
        let mut schema = wheel_position(1, 4);
        apply_slot_labels(
            &mut schema,
            &[
                "485/20".into(),
                "500/100".into(),
                "600/30".into(),
                "340/35".into(),
            ],
        );
        let pairs: Vec<(i64, &str)> = schema
            .enum_values
            .iter()
            .map(|entry| match entry.value {
                Value::I64(position) => (position, entry.label.as_str()),
                _ => panic!("positions are integers"),
            })
            .collect();
        assert_eq!(
            pairs,
            vec![(1, "485/20"), (2, "500/100"), (3, "600/30"), (4, "340/35")]
        );
    }

    #[test]
    fn a_wheel_numbered_from_zero_is_labelled_from_zero() {
        let mut schema = wheel_position(0, 2);
        apply_slot_labels(
            &mut schema,
            &["open".into(), "GFP".into(), "mCherry".into()],
        );
        assert_eq!(schema.enum_values[0].value, Value::I64(0));
        assert_eq!(schema.enum_values[0].label, "open");
    }

    #[test]
    fn unlabelled_slots_stay_selectable_as_numbers() {
        // Someone labelled the two filters they use and left the rest. The other four slots
        // still hold glass and must remain reachable.
        let mut schema = wheel_position(1, 6);
        apply_slot_labels(&mut schema, &["GFP".into(), "".into()]);
        assert_eq!(schema.enum_values.len(), 6);
        assert_eq!(schema.enum_values[0].label, "GFP");
        assert_eq!(schema.enum_values[1].label, "2");
        assert_eq!(schema.enum_values[5].label, "6");
    }

    #[test]
    fn a_driver_that_publishes_its_own_choices_keeps_them() {
        let mut schema = wheel_position(1, 3);
        schema.enum_values = vec![EnumValue {
            value: Value::I64(1),
            label: "read from the turret".into(),
        }];
        apply_slot_labels(&mut schema, &[]);
        assert_eq!(schema.enum_values.len(), 1);
        assert_eq!(schema.enum_values[0].label, "read from the turret");
    }

    #[test]
    fn labels_round_trip_through_the_property_map() {
        let mut properties = BTreeMap::new();
        let written = vec!["485/20".into(), "Dichroic 510, long pass".into()];
        set_slot_labels(&mut properties, &written);
        assert_eq!(slot_labels(&properties), written);
    }

    #[test]
    fn clearing_every_name_removes_the_property() {
        let mut properties = BTreeMap::new();
        set_slot_labels(&mut properties, &["GFP".into()]);
        set_slot_labels(&mut properties, &["".into(), "  ".into()]);
        assert!(!properties.contains_key(SLOT_LABELS_PROPERTY));
        assert!(slot_labels(&properties).is_empty());
    }

    #[test]
    fn a_name_that_looks_like_a_measurement_is_still_its_own_text() {
        // "500 nm" parses as a wavelength. As a slot name it has to read back verbatim.
        assert_eq!(
            value_label(&Value::Wavelength(crate::Wavelength::from_nanometers(
                500.0
            ))),
            "500 nm"
        );
    }
}
