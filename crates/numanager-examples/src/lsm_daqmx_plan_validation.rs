use numanager_core::{CapabilityKind, Value};
use numanager_examples::capability_brief;

pub fn run() -> numanager_core::Result<()> {
    let (runtime, hub) = crate::lsm_common::runtime_for_source("imswitch")?;
    let capture_capability =
        runtime.capability_by_kind(&hub, CapabilityKind::ConfocalImageCapture)?;
    let signal_capability = runtime.capability_by_kind(&hub, CapabilityKind::ScanSignalStream)?;

    let valid_raster_value = crate::lsm_common::run_request(
        &runtime,
        &hub,
        crate::lsm_common::snapshot_request(256, 256),
    )?;
    let valid_signal_value = crate::lsm_common::run_request(
        &runtime,
        &hub,
        crate::lsm_common::line_signal_request(512, 128),
    )?;

    let mut raster_request = crate::lsm_common::snapshot_request(256, 256);
    raster_request
        .scan
        .insert("x_galvo".into(), Value::String("ai0".into()));
    let raster_value = crate::lsm_common::run_request(&runtime, &hub, raster_request)?;

    let signal_request = crate::lsm_common::line_signal_request_channels(
        512,
        128,
        vec!["unsupported_detector".into()],
    );
    let signal_value = crate::lsm_common::run_request(&runtime, &hub, signal_request)?;

    println!("source: imswitch");
    println!("hub: {}", hub.label);
    println!("capture_api: {}", capability_brief(&capture_capability));
    println!("signal_api: {}", capability_brief(&signal_capability));
    println!("valid_raster_request: 256x256 with configured role channels");
    println!(
        "valid_raster_result: {}",
        crate::lsm_common::api_result(&valid_raster_value)
    );
    if let Some(summary) = crate::lsm_common::daqmx_task_plan_summary(&valid_raster_value) {
        println!("valid_raster_plan: {summary}");
    }
    println!(
        "valid_raster_validation: {}",
        validation_summary(&valid_raster_value)
    );
    println!(
        "valid_raster_helper_commands: {}",
        helper_command_summary(&valid_raster_value)
    );
    println!("valid_signal_request: one 512-sample line over configured channels, chunk_size=128");
    println!(
        "valid_signal_result: {}",
        crate::lsm_common::api_result(&valid_signal_value)
    );
    if let Some(summary) = crate::lsm_common::daqmx_task_plan_summary(&valid_signal_value) {
        println!("valid_signal_plan: {summary}");
    }
    println!(
        "valid_signal_validation: {}",
        validation_summary(&valid_signal_value)
    );
    println!(
        "valid_signal_helper_commands: {}",
        helper_command_summary(&valid_signal_value)
    );
    println!("raster_request: 256x256 with x_galvo mapped to ai0");
    println!(
        "raster_result: {}",
        crate::lsm_common::api_result(&raster_value)
    );
    if let Some(summary) = crate::lsm_common::daqmx_task_plan_summary(&raster_value) {
        println!("raster_plan: {summary}");
    }
    println!(
        "raster_helper_commands: {}",
        helper_command_summary(&raster_value)
    );
    println!("raster_validation: {}", validation_summary(&raster_value));
    println!("signal_request: one 512-sample line over unsupported_detector, chunk_size=128");
    println!(
        "signal_result: {}",
        crate::lsm_common::api_result(&signal_value)
    );
    if let Some(summary) = crate::lsm_common::daqmx_task_plan_summary(&signal_value) {
        println!("signal_plan: {summary}");
    }
    println!(
        "signal_helper_commands: {}",
        helper_command_summary(&signal_value)
    );
    println!("signal_validation: {}", validation_summary(&signal_value));
    println!("execution_gate: not_live_task_execution");
    Ok(())
}

fn helper_command_summary(value: &Value) -> String {
    let Some(plan) = daqmx_plan(value) else {
        return "setup=missing preflight=missing".into();
    };
    format!(
        "setup={} preflight={}",
        value_kind(plan.get("plan_setup_helper_command")),
        value_kind(plan.get("plan_preflight_helper_command"))
    )
}

fn validation_summary(value: &Value) -> String {
    let Some(plan) = daqmx_plan(value) else {
        return "missing".into();
    };
    let Some(Value::Map(validation)) = plan.get("plan_validation") else {
        return "missing".into();
    };
    let status = string_field(validation, "status").unwrap_or_else(|| "unknown".into());
    let runnable = bool_field(validation, "helper_command_runnable")
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".into());
    let task_count = i64_field(validation, "recognized_task_count")
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".into());
    let unrecognized_count = i64_field(validation, "unrecognized_channel_count")
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".into());
    let invalid_role_count = i64_field(validation, "invalid_role_channel_count")
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".into());
    let unrecognized = list_field(validation, "unrecognized_channels")
        .filter(|values| !values.is_empty())
        .map(|values| format!(" unrecognized={}", values.join("+")))
        .unwrap_or_default();
    let invalid_roles = list_field(validation, "invalid_role_channels")
        .filter(|values| !values.is_empty())
        .map(|values| format!(" invalid_roles={}", values.join("+")))
        .unwrap_or_default();
    format!(
        "status={status} runnable={runnable} recognized_tasks={task_count} unrecognized_count={unrecognized_count} invalid_role_count={invalid_role_count}{unrecognized}{invalid_roles}"
    )
}

fn daqmx_plan(value: &Value) -> Option<&std::collections::BTreeMap<String, Value>> {
    let Value::Map(result) = value else {
        return None;
    };
    let Some(Value::Map(plan)) = result.get("daqmx_task_plan") else {
        return None;
    };
    Some(plan)
}

fn value_kind(value: Option<&Value>) -> &'static str {
    match value {
        Some(Value::String(_)) => "string",
        Some(Value::Null) => "null",
        Some(_) => "non_string",
        None => "missing",
    }
}

fn string_field(map: &std::collections::BTreeMap<String, Value>, key: &str) -> Option<String> {
    match map.get(key)? {
        Value::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn bool_field(map: &std::collections::BTreeMap<String, Value>, key: &str) -> Option<bool> {
    match map.get(key)? {
        Value::Bool(value) => Some(*value),
        _ => None,
    }
}

fn i64_field(map: &std::collections::BTreeMap<String, Value>, key: &str) -> Option<i64> {
    match map.get(key)? {
        Value::I64(value) => Some(*value),
        _ => None,
    }
}

fn list_field(map: &std::collections::BTreeMap<String, Value>, key: &str) -> Option<Vec<String>> {
    let Value::List(values) = map.get(key)? else {
        return None;
    };
    Some(
        values
            .iter()
            .filter_map(|value| match value {
                Value::String(value) => Some(value.clone()),
                _ => None,
            })
            .collect(),
    )
}
