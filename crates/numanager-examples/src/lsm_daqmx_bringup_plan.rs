use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use numanager_core::runtime::Runtime;
use numanager_core::{CapabilityKind, Command, Value};
use numanager_examples::capability_brief;

pub fn run() -> numanager_core::Result<()> {
    let (runtime, hub) = crate::lsm_common::runtime_for_source("imswitch")?;
    let capture_capability =
        runtime.capability_by_kind(&hub, CapabilityKind::ConfocalImageCapture)?;
    let signal_capability = runtime.capability_by_kind(&hub, CapabilityKind::ScanSignalStream)?;
    let backend_status = runtime.execute(
        Command::read_property(hub.id, "backend_status"),
        Duration::from_secs(2),
    )?;

    let capture = crate::lsm_common::run_request(
        &runtime,
        &hub,
        crate::lsm_common::snapshot_request(512, 512),
    )?;
    let signal = crate::lsm_common::run_request(
        &runtime,
        &hub,
        crate::lsm_common::line_signal_request(1024, 256),
    )?;

    println!("source: imswitch");
    println!("hub: {}", hub.label);
    println!("capture_api: {}", capability_brief(&capture_capability));
    println!("signal_api: {}", capability_brief(&signal_capability));
    if let Some(summary) = backend_readiness_summary(&backend_status) {
        println!("backend_readiness: {summary}");
    }
    if let Some(summary) = crate::lsm_common::daqmx_task_plan_summary(&capture) {
        println!("capture_plan: {summary}");
    }
    if let Some(summary) = crate::lsm_common::daqmx_task_plan_summary(&signal) {
        println!("signal_plan: {summary}");
    }

    println!("bench_evidence_commands:");
    println!("scripts/audit-ni-daqmx-evidence-inputs.sh");
    println!("scripts/audit-ni-daqmx-external-gates.sh");
    println!("scripts/audit-ni-daqmx-package-inputs.sh <installer-file-or-directory>");
    println!("scripts/audit-ni-daqmx-sdk-headers.sh <header-file-or-directory>");
    println!("scripts/audit-ni-daqmx-sys-source.sh <ni-daqmx-sys-repo>");
    println!("scripts/audit-ni-daqmx-target-scope.sh");
    println!("scripts/audit-ni-daqmx-no-hardware-helpers.sh");
    println!("scripts/audit-ni-daqmx-plan-validation.sh");
    println!("scripts/audit-ni-daqmx-live-gate.sh");
    println!("scripts/audit-ni-daqmx-runtime-probe.sh");
    println!("scripts/audit-ni-daqmx-example-output-sync.sh");
    println!("bench_runtime_probe_commands:");
    for command in crate::lsm_daqmx_commands::runtime_probe_commands() {
        println!("{command}");
    }
    println!("bench_helper_build_commands:");
    println!("cargo build -p numanager-imswitch-daqmx --features ni-daqmx-sdk --bins");
    println!("bench_inventory_commands:");
    for command in crate::lsm_daqmx_commands::inventory_commands() {
        println!("{command}");
    }
    println!("bench_preflight_commands:");
    if let Some(command) = plan_preflight_command(&capture) {
        println!("{command}");
    }
    if let Some(command) = plan_preflight_command(&signal) {
        println!("{command}");
    }
    println!("bench_lifecycle_dry_run_commands:");
    for command in crate::lsm_daqmx_commands::task_lifecycle_dry_run_commands() {
        println!("{command}");
    }
    println!("bench_lifecycle_cleanup_simulation_commands:");
    for command in crate::lsm_daqmx_commands::task_lifecycle_cleanup_simulation_commands() {
        println!("{command}");
    }
    println!("bench_plan_setup_cleanup_simulation_commands:");
    for command in [
        crate::lsm_daqmx_commands::plan_setup_cleanup_simulation_command(&capture),
        crate::lsm_daqmx_commands::plan_setup_cleanup_simulation_command(&signal),
    ]
    .into_iter()
    .flatten()
    {
        println!("{command}");
    }
    println!("bench_invalid_numeric_guard_commands:");
    for command in crate::lsm_daqmx_commands::invalid_numeric_guard_commands(&[&capture, &signal]) {
        println!("{command}");
    }
    println!("bench_channel_setup_dry_run_commands:");
    for command in crate::lsm_daqmx_commands::channel_setup_commands(&capture, true)
        .into_iter()
        .chain(crate::lsm_daqmx_commands::channel_setup_commands(
            &signal, true,
        ))
        .collect::<BTreeSet<_>>()
    {
        println!("{command}");
    }
    println!("bench_setup_commands:");
    for command in crate::lsm_daqmx_commands::task_lifecycle_setup_commands() {
        println!("{command}");
    }
    println!("bench_plan_setup_commands:");
    if let Some(command) = plan_setup_command(&capture) {
        println!("{command}");
    }
    if let Some(command) = plan_setup_command(&signal) {
        println!("{command}");
    }
    println!("bench_channel_setup_commands:");
    for command in crate::lsm_daqmx_commands::channel_setup_commands(&capture, false)
        .into_iter()
        .chain(crate::lsm_daqmx_commands::channel_setup_commands(
            &signal, false,
        ))
        .collect::<BTreeSet<_>>()
    {
        println!("{command}");
    }
    println!("bench_io_smoke_dry_run_commands:");
    for command in crate::lsm_daqmx_commands::io_smoke_commands(&capture, false)
        .into_iter()
        .chain(crate::lsm_daqmx_commands::io_smoke_commands(&signal, false))
        .collect::<BTreeSet<_>>()
    {
        println!("{command}");
    }
    println!("bench_io_smoke_cleanup_simulation_commands:");
    for command in crate::lsm_daqmx_commands::io_smoke_cleanup_simulation_commands(&capture)
        .into_iter()
        .chain(crate::lsm_daqmx_commands::io_smoke_cleanup_simulation_commands(&signal))
        .collect::<BTreeSet<_>>()
    {
        println!("{command}");
    }
    println!("bench_io_smoke_execute_commands:");
    for command in crate::lsm_daqmx_commands::io_smoke_commands(&capture, true)
        .into_iter()
        .chain(crate::lsm_daqmx_commands::io_smoke_commands(&signal, true))
        .collect::<BTreeSet<_>>()
    {
        println!("{command}");
    }
    println!("execution_gate: not_live_task_execution");
    Ok(())
}

fn plan_setup_command(value: &Value) -> Option<String> {
    let plan = daqmx_plan(value)?;
    string_field(plan, "plan_setup_helper_command")
}

fn plan_preflight_command(value: &Value) -> Option<String> {
    let plan = daqmx_plan(value)?;
    string_field(plan, "plan_preflight_helper_command")
}

fn daqmx_plan(value: &Value) -> Option<&BTreeMap<String, Value>> {
    let Value::Map(result) = value else {
        return None;
    };
    let Some(Value::Map(plan)) = result.get("daqmx_task_plan") else {
        return None;
    };
    Some(plan)
}

fn backend_readiness_summary(value: &Value) -> Option<String> {
    let Value::Map(map) = value else {
        return None;
    };

    let mut parts = Vec::new();
    if let Some(execution) = string_field(map, "execution_status") {
        parts.push(format!("execution={execution}"));
    }
    if let Some(ready) = bool_field(map, "live_task_execution_ready") {
        parts.push(format!("live_ready={ready}"));
    }
    if let Some(requested) = bool_field(map, "live_task_execution_requested") {
        parts.push(format!("live_requested={requested}"));
    }
    if let Some(blocker) = string_field(map, "live_task_execution_blocker") {
        parts.push(format!("blocker={blocker}"));
    }
    if let Some(comparison) = runtime_version_comparison_summary(map) {
        parts.push(format!("runtime_version={comparison}"));
    }
    if let Some(missing) = string_list_field(map, "missing") {
        let missing = if missing.is_empty() {
            "none".to_string()
        } else {
            missing.join("+")
        };
        parts.push(format!("missing={missing}"));
    }
    if let Some(statuses) = promotion_gate_statuses_summary(map) {
        parts.push(format!("promotion_gate_statuses=[{statuses}]"));
    }

    (!parts.is_empty()).then(|| parts.join("; "))
}

fn runtime_version_comparison_summary(map: &BTreeMap<String, Value>) -> Option<String> {
    let comparison = string_field(map, "runtime_version_comparison")?;
    let matches = match map.get("runtime_version_matches") {
        Some(Value::Bool(value)) => value.to_string(),
        _ => "unknown".into(),
    };
    let basis =
        string_field(map, "runtime_version_comparison_basis").unwrap_or_else(|| "unknown".into());
    Some(format!("{comparison}(matches={matches},basis={basis})"))
}

fn promotion_gate_statuses_summary(map: &BTreeMap<String, Value>) -> Option<String> {
    let Some(Value::Map(statuses)) = map.get("external_promotion_gate_statuses") else {
        return None;
    };

    let mut counts = BTreeMap::<String, usize>::new();
    for status in statuses.values() {
        let Value::Map(status) = status else {
            continue;
        };
        if let Some(status) = string_field(status, "status") {
            *counts.entry(status).or_default() += 1;
        }
    }

    (!counts.is_empty()).then(|| {
        counts
            .into_iter()
            .map(|(status, count)| format!("{status}={count}"))
            .collect::<Vec<_>>()
            .join(",")
    })
}

fn bool_field(map: &BTreeMap<String, Value>, key: &str) -> Option<bool> {
    match map.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn string_list_field(map: &BTreeMap<String, Value>, key: &str) -> Option<Vec<String>> {
    let Some(Value::List(values)) = map.get(key) else {
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

fn string_field(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    match map.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}
