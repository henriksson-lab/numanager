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

    println!("# NI-DAQmx Bench Validation Note");
    println!();
    println!("This generated note is a scaffold, not a validation result.");
    println!("It does not create NI tasks, write outputs, read inputs, or claim hardware support.");
    println!();
    print_run_identity(&hub.label, &capture, &signal);
    print_evidence_sources();
    print_setup_and_safety();
    print_required_artifacts();
    println!("## Public API Plan Source");
    println!();
    println!("| Field | Value |");
    println!("| --- | --- |");
    println!("| Source | imswitch |");
    println!("| Hub | {} |", hub.label);
    println!(
        "| Capture API | {} |",
        capability_brief(&capture_capability)
    );
    println!("| Signal API | {} |", capability_brief(&signal_capability));
    println!("| Execution gate | not_live_task_execution |");
    println!();
    print_backend_readiness(&backend_status, &capture, &signal);
    print_backend_inventory(&backend_status);
    print_external_promotion_gates(&backend_status);
    println!("## Current Task Plans");
    println!();
    if let Some(summary) = crate::lsm_common::daqmx_task_plan_summary(&capture) {
        println!("- Capture: {summary}");
    }
    if let Some(summary) = crate::lsm_common::daqmx_task_plan_summary(&signal) {
        println!("- Signal: {summary}");
    }
    println!();
    println!("## Preflight Evidence Targets");
    println!();
    print_preflight_targets("Capture", &capture);
    print_preflight_targets("Signal", &signal);
    print_physical_channel_mapping(&capture, &signal);
    print_output_input_validation(&capture, &signal);
    print_lsm_task_execution_gate();

    let commands = required_commands(&capture, &signal);
    println!("## Required Commands");
    println!();
    println!(
        "Set `NUMANAGER_DAQMX_DEVICE_NAME`, `NUMANAGER_DAQMX_LSM_X_GALVO`, `NUMANAGER_DAQMX_LSM_Y_GALVO`, `NUMANAGER_DAQMX_LSM_LASER_GATE`, `NUMANAGER_DAQMX_LSM_DETECTOR`, `NUMANAGER_DAQMX_LSM_SAMPLE_CLOCK`, `NUMANAGER_DAQMX_LSM_SAMPLE_CLOCK_SOURCE`, `NUMANAGER_DAQMX_LSM_START_TRIGGER_SOURCE`, `NUMANAGER_DAQMX_SIGNAL_AI`, `NUMANAGER_DAQMX_SIGNAL_CHANNELS`, `NUMANAGER_DAQMX_TIMEOUT_SECONDS`, and `NUMANAGER_DAQMX_HELPER_TIMEOUT_SECONDS` before generating this note when the bench mapping, DAQmx timeout, or helper supervision timeout differs from the defaults."
    );
    println!();
    println!("```sh");
    for command in &commands {
        println!("{command}");
    }
    println!("```");
    println!();
    println!(
        "Commands containing `--execute` are bench-only I/O smoke checks; review wiring, load, safe output state, and cleanup before running them."
    );
    println!(
        "Commands containing `--simulate-error-after-start` without `--execute` are no-DAQmx cleanup-log simulations."
    );
    println!(
        "`NUMANAGER_DAQMX_CONFIG_ONLY=1` should print effective `probe_config`, `connected: Bool(false)`, and a no-runtime `backend_status` with `connect_requested=false`; it must not load the NI-DAQmx vendor runtime."
    );
    println!();
    println!("## Command Output Log");
    println!();
    println!("| Command | Exit status | Stdout/stderr artifact | Result | Notes |");
    println!("| --- | --- | --- | --- | --- |");
    for command in &commands {
        println!("| `{}` |  |  | Unknown |  |", markdown_table_code(command));
    }
    println!();
    println!("## Evidence Checklist");
    println!();
    println!("| Evidence | Result | Notes |");
    println!("| --- | --- | --- |");
    for evidence in [
        "Evidence-input audit covering local package, installed header, and FFI source inventory markers",
        "External-gates audit showing legal, installed-header, NI-PAL, bench-safety, runtime-publication, and live-task gates remain explicit",
        "Package input inventory",
        "Passing header inventory with NIDAQmx.h count/path, title/copyright, required symbols, runtime-version accessors, and literal package-version macro status",
        "Bindgen regeneration command and FFI-source inventory from the same installed target-platform NIDAQmx.h",
        "FFI source inventory with fork revision, bindgen inputs, platform link cfgs, and runtime-version bindings",
        "numanager NI-DAQmx target-scope audit with Linux/Windows dependency and helper-wrapper boundaries",
        "No-hardware helper audit covering dry-run, preflight-only, simulated-cleanup, and invalid-input guard paths",
        "Plan-validation audit showing valid helper commands stay runnable and invalid plans suppress setup/preflight helpers",
        "Live-gate audit showing live-task intent remains not-live until bench evidence exists",
        "Task-plan live readiness showing per-plan blocker, missing evidence, runtime-version comparison, backend-status agreement, and pending hardware validation",
        "Runtime-probe audit covering config-only metadata and process-isolated runtime probing",
        "Example-output sync audit covering generated DAQmx scaffold documentation markers",
        "Runtime probe config-only",
        "Runtime probe",
        "Backend inventory readiness showing helper isolation, requested inventory state, detected-device count, configured-device identity, and contained helper/configured-device errors",
        "LSM bring-up plan with backend_readiness and promotion_gate_statuses captured before helper commands",
        "Bench safety preconditions recorded before --execute helper commands",
        "Helper build",
        "Isolated Linux runtime probe",
        "Device inventory",
        "Raster plan preflight",
        "Signal plan preflight",
        "Task lifecycle dry run",
        "Task lifecycle cleanup-log simulation",
        "Plan setup cleanup-log simulation",
        "Helper invalid numeric/range/transfer/raster/signal input guard for non-finite/non-positive timing and frequency, non-finite/out-of-range duty cycle, empty route sources, whitespace-padded route sources, empty channels/task labels, leading/trailing whitespace in helper identifiers, duplicate physical channels, duplicate active task labels, invalid signal line/chunk metadata, single-channel empty channel inputs, empty explicit task names, incomplete raster dimensions, raster dimension overflow, raster frame-product overflow, reversed ranges, AO smoke ranges that exclude the 0 V final write, oversized transfers, raster mismatches, and I/O smoke --execute without --bench-safety-reviewed",
        "Empty task lifecycle",
        "Channel setup dry run",
        "Channel setup",
        "Raster plan setup",
        "Signal plan setup",
        "Output/input readback",
        "Runtime ConfocalImageCapture FrameReady output with frame handle, final-frame dimensions, pixel format, scan/reconstruction metadata, timing metadata, detector metadata, reconstruction pixel size, and saturated-pixel status",
        "Runtime ConfocalImageStream FrameReady output with stream id, repeated frame handles, dirty-region/update metadata, dimensions, pixel format, scan/reconstruction metadata, timing metadata, detector metadata, reconstruction pixel size, and progress/status events",
        "Runtime ScanSignalStream ScanSignalChunk output with stream id, channel names, timing origin, line/chunk/first-sample indices, sample count, sample rate, sample period, sample values, dropped sample/chunk counters, overflow status, and progress/status events",
        "User stop/cancel and cleanup",
    ] {
        println!("| {evidence} | Unknown |  |");
    }
    println!();
    println!("## Remaining Uncertainty");
    println!();
    println!("| Behavior | Uncertainty | Evidence needed before support claim |");
    println!("| --- | --- | --- |");
    for (behavior, uncertainty, evidence) in remaining_uncertainties() {
        println!("| {behavior} | {uncertainty} | {evidence} |");
    }
    println!();
    println!("## Promotion Gate");
    println!();
    println!(
        "Keep live NI-DAQmx task execution disabled until every checklist row has bench evidence."
    );
    Ok(())
}

fn print_run_identity(hub_label: &str, capture: &Value, signal: &Value) {
    println!("## Run Identity");
    println!();
    println!("| Field | Value |");
    println!("| --- | --- |");
    println!("| Driver crate | `numanager-imswitch-daqmx` |");
    println!("| Device page | `docs/devices/imswitch-daqmx.md` |");
    println!("| Hub | `{}` |", markdown_table_text(hub_label));
    println!("| NI device model |  |");
    println!(
        "| NI device name | `{}` |",
        markdown_table_code(&crate::lsm_daqmx_commands::device_name())
    );
    println!("| Serial number or asset tag |  |");
    println!("| Firmware/software version |  |");
    println!("| Transport | NI-DAQmx vendor runtime / PCIe, PXI, USB, Ethernet, or cDAQ chassis |");
    println!(
        "| NI-DAQmx runtime version | {} |",
        optional_env_table_value("NUMANAGER_DAQMX_RUNTIME_VERSION")
            .or_else(|| optional_env_table_value("NIDAQMX_RUNTIME_VERSION"))
            .unwrap_or_default()
    );
    println!(
        "| NI-DAQmx package / installer | {} |",
        optional_env_table_value("NIDAQMX_RUNTIME_PACKAGE").unwrap_or_default()
    );
    println!(
        "| Host OS and driver stack | `{}/{}` |",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("| Date | YYYY-MM-DD |");
    println!("| Operator |  |");
    println!("| Config file or discovery record | generated from public `imswitch` descriptor |");
    println!(
        "| `lsm_x_galvo` / `lsm_y_galvo` | `{}` / `{}` |",
        markdown_table_code(&role_channel(capture, "x_galvo").unwrap_or_default()),
        markdown_table_code(&role_channel(capture, "y_galvo").unwrap_or_default())
    );
    println!(
        "| `lsm_laser_gate` | `{}` |",
        markdown_table_code(&role_channel(capture, "laser_gate").unwrap_or_default())
    );
    println!(
        "| `lsm_detector` | `{}` |",
        markdown_table_code(&role_channel(capture, "detector").unwrap_or_default())
    );
    println!(
        "| `lsm_sample_clock` | `{}` |",
        markdown_table_code(&role_channel(capture, "sample_clock").unwrap_or_default())
    );
    println!(
        "| `lsm_sample_clock_source` | `{}` |",
        markdown_table_code(
            &route_source(capture, "sample_clock").unwrap_or_else(|| "<unset>".into())
        )
    );
    println!(
        "| `lsm_start_trigger_source` | `{}` |",
        markdown_table_code(
            &route_source(capture, "start_trigger").unwrap_or_else(|| "<unset>".into())
        )
    );
    println!(
        "| Signal channels | `{}` |",
        markdown_table_code(&signal_channels(signal).join(","))
    );
    println!(
        "| `daqmx_timeout` | `{}` |",
        markdown_table_code(&cleanup_timeout(capture).unwrap_or_else(|| "<unset>".into()))
    );
    println!(
        "| `inventory_helper_timeout` | {} |",
        optional_env_table_value("NUMANAGER_DAQMX_HELPER_TIMEOUT_SECONDS")
            .unwrap_or_else(|| "`<driver_default>`".into())
    );
    println!();
}

fn print_backend_readiness(backend_status: &Value, capture: &Value, signal: &Value) {
    let Value::Map(status) = backend_status else {
        return;
    };
    println!("## Backend Readiness");
    println!();
    println!("| Field | Value |");
    println!("| --- | --- |");
    for (label, key) in [
        ("Execution status", "execution_status"),
        ("Live task execution ready", "live_task_execution_ready"),
        ("Live task execution blocker", "live_task_execution_blocker"),
        (
            "Live task execution requested",
            "live_task_execution_requested",
        ),
        ("Feature requested", "feature_requested"),
        ("Feature enabled", "feature_enabled"),
        ("Target supported", "target_supported"),
        ("Runtime detected", "runtime_detected"),
        ("Runtime version comparison", "runtime_version_comparison"),
        ("Runtime version matches", "runtime_version_matches"),
        (
            "Runtime version comparison basis",
            "runtime_version_comparison_basis",
        ),
        ("Package identity recorded", "package_identity_recorded"),
        ("SDK header recorded", "sdk_header_recorded"),
        ("Hardware validation status", "hardware_validation_status"),
        ("Evidence status", "evidence_status"),
    ] {
        println!(
            "| {label} | `{}` |",
            markdown_table_code(&status_value(status, key))
        );
    }
    println!(
        "| Missing evidence | `{}` |",
        markdown_table_code(
            &list_field(status, "missing")
                .filter(|missing| !missing.is_empty())
                .map(|missing| missing.join("+"))
                .unwrap_or_else(|| "none".into())
        )
    );
    println!(
        "| External promotion gates | `{}` |",
        markdown_table_code(
            &list_field(status, "external_promotion_gates")
                .filter(|gates| !gates.is_empty())
                .map(|gates| gates.join("+"))
                .unwrap_or_else(|| "none".into())
        )
    );
    println!(
        "| Task-plan readiness agreement | `{}` |",
        markdown_table_code(&task_plan_readiness_agreement(status, capture, signal))
    );
    println!();
}

fn print_backend_inventory(backend_status: &Value) {
    let Value::Map(status) = backend_status else {
        return;
    };
    println!("## Backend Inventory");
    println!();
    println!("| Field | Value |");
    println!("| --- | --- |");
    for (label, value) in [
        (
            "Device inventory requested",
            status_value(status, "device_inventory_requested"),
        ),
        (
            "Inventory helper configured",
            status_value(status, "inventory_helper_configured"),
        ),
        (
            "Inventory helper timeout",
            status_value(status, "inventory_helper_timeout"),
        ),
        (
            "Detected device count",
            list_field(status, "detected_devices")
                .map(|devices| devices.len().to_string())
                .unwrap_or_else(|| "0".into()),
        ),
        (
            "Detected devices",
            list_field(status, "detected_devices")
                .filter(|devices| !devices.is_empty())
                .map(|devices| devices.join("+"))
                .unwrap_or_else(|| "none".into()),
        ),
        (
            "Configured device detected",
            status_value(status, "configured_device_detected"),
        ),
        (
            "Configured device identity",
            configured_device_identity_summary(status).unwrap_or_else(|| "none".into()),
        ),
        (
            "Device inventory error",
            string_field(status, "device_inventory_error").unwrap_or_else(|| "none".into()),
        ),
        (
            "Configured device error",
            string_field(status, "configured_device_error").unwrap_or_else(|| "none".into()),
        ),
    ] {
        println!("| {label} | `{}` |", markdown_table_code(&value));
    }
    println!();
}

fn configured_device_identity_summary(status: &BTreeMap<String, Value>) -> Option<String> {
    let Some(Value::Map(device)) = status.get("configured_device_identity") else {
        return None;
    };
    let name = string_field(device, "name")?;
    let product = string_field(device, "product_type").unwrap_or_else(|| "unknown_product".into());
    let serial = status_value(device, "serial_number");
    let analog_inputs = list_field(device, "analog_inputs")
        .filter(|values| !values.is_empty())
        .map(|values| values.join("+"))
        .unwrap_or_else(|| "none".into());
    let analog_outputs = list_field(device, "analog_outputs")
        .filter(|values| !values.is_empty())
        .map(|values| values.join("+"))
        .unwrap_or_else(|| "none".into());
    let counter_inputs = list_field(device, "counter_inputs")
        .filter(|values| !values.is_empty())
        .map(|values| values.join("+"))
        .unwrap_or_else(|| "none".into());
    let counter_outputs = list_field(device, "counter_outputs")
        .filter(|values| !values.is_empty())
        .map(|values| values.join("+"))
        .unwrap_or_else(|| "none".into());
    Some(format!(
        "name={name};product={product};serial={serial};ai={analog_inputs};ao={analog_outputs};ci={counter_inputs};co={counter_outputs}"
    ))
}

fn print_external_promotion_gates(backend_status: &Value) {
    let Value::Map(status) = backend_status else {
        return;
    };
    let Some(gates) =
        list_field(status, "external_promotion_gates").filter(|gates| !gates.is_empty())
    else {
        return;
    };

    println!("## External Promotion Gates");
    println!();
    println!("| Gate | Required evidence | Status |");
    println!("| --- | --- | --- |");
    for gate in gates {
        let (evidence, gate_status) = promotion_gate_status(status, &gate);
        println!(
            "| `{}` | {} | {} |",
            markdown_table_code(&gate),
            markdown_table_text(&evidence),
            markdown_table_text(&gate_status)
        );
    }
    println!();
}

fn promotion_gate_status(status: &BTreeMap<String, Value>, gate: &str) -> (String, String) {
    let Some(Value::Map(gate_statuses)) = status.get("external_promotion_gate_statuses") else {
        return (promotion_gate_evidence(gate).into(), "Unknown".into());
    };
    let Some(Value::Map(gate_status)) = gate_statuses.get(gate) else {
        return (promotion_gate_evidence(gate).into(), "Unknown".into());
    };
    let evidence = string_field(gate_status, "evidence_required")
        .unwrap_or_else(|| promotion_gate_evidence(gate).into());
    let status = string_field(gate_status, "status").unwrap_or_else(|| "Unknown".into());
    (evidence, status)
}

fn promotion_gate_evidence(gate: &str) -> &'static str {
    match gate {
        "legal_review" => "Completed package-intake legal review for exact Linux and Windows inputs",
        "installed_windows_package_license_review" => {
            "Installed Windows package/license boundary audit recorded"
        }
        "installed_linux_26_5_header_audit" => {
            "Installed Linux 26.5 NIDAQmx.h inventory, digest, and bindgen command recorded"
        }
        "installed_windows_26_5_header_audit" => {
            "Installed Windows 26.5 NIDAQmx.h inventory, digest, and bindgen command recorded"
        }
        "ni_pal_device_inventory" => {
            "Process-isolated NI-PAL/device inventory and configured-device identity recorded"
        }
        "bench_safety_preconditions" => {
            "Completed Setup And Safety table plus reviewed wiring, load, safe output state, interlocks, emergency stop, cleanup, and fault-recovery constraints"
        }
        "task_ordering_routing_completion_cleanup_bench_validation" => {
            "Bench logs for task order, routing, completion, stop/clear, cleanup, and safe output state"
        }
        "runtime_publication_hardware_validation" => {
            "Hardware-backed FrameReady and ScanSignalChunk runtime output logs"
        }
        "hardware_validation_note" => {
            "Completed hardware validation note following docs/devices/hardware-validation-template.md"
        }
        _ => "Documented evidence for this promotion gate",
    }
}

fn task_plan_readiness_agreement(
    backend_status: &BTreeMap<String, Value>,
    capture: &Value,
    signal: &Value,
) -> String {
    let backend_ready = bool_field(backend_status, "live_task_execution_ready").unwrap_or(false);
    let backend_blocker = string_field(backend_status, "live_task_execution_blocker")
        .unwrap_or_else(|| "unknown".into());
    let backend_hardware = string_field(backend_status, "hardware_validation_status")
        .unwrap_or_else(|| "unknown".into());
    let backend_missing = list_field(backend_status, "missing").unwrap_or_default();
    let backend_gates = list_field(backend_status, "external_promotion_gates").unwrap_or_default();
    let backend_gate_statuses = backend_status.get("external_promotion_gate_statuses");
    let backend_runtime_version_comparison =
        string_field(backend_status, "runtime_version_comparison")
            .unwrap_or_else(|| "unknown".into());
    let backend_runtime_version_matches = backend_status.get("runtime_version_matches");
    let backend_runtime_version_basis =
        string_field(backend_status, "runtime_version_comparison_basis")
            .unwrap_or_else(|| "unknown".into());
    let capture_agrees = readiness_matches(
        capture,
        backend_ready,
        &backend_blocker,
        &backend_hardware,
        &backend_missing,
        &backend_gates,
        backend_gate_statuses,
        &backend_runtime_version_comparison,
        backend_runtime_version_matches,
        &backend_runtime_version_basis,
    );
    let signal_agrees = readiness_matches(
        signal,
        backend_ready,
        &backend_blocker,
        &backend_hardware,
        &backend_missing,
        &backend_gates,
        backend_gate_statuses,
        &backend_runtime_version_comparison,
        backend_runtime_version_matches,
        &backend_runtime_version_basis,
    );
    format!(
        "capture={capture_agrees};signal={signal_agrees};basis=backend_status_runtime_version_and_daqmx_task_plan"
    )
}

fn readiness_matches(
    value: &Value,
    backend_ready: bool,
    backend_blocker: &str,
    backend_hardware: &str,
    backend_missing: &[String],
    backend_gates: &[String],
    backend_gate_statuses: Option<&Value>,
    backend_runtime_version_comparison: &str,
    backend_runtime_version_matches: Option<&Value>,
    backend_runtime_version_basis: &str,
) -> bool {
    let Some(plan) = daqmx_plan(value) else {
        return false;
    };
    let Some(Value::Map(readiness)) = plan.get("live_task_execution_readiness") else {
        return false;
    };
    bool_field(readiness, "live_task_execution_ready") == Some(backend_ready)
        && string_field(readiness, "live_task_execution_blocker").as_deref()
            == Some(backend_blocker)
        && string_field(readiness, "hardware_validation_status").as_deref()
            == Some(backend_hardware)
        && list_field(readiness, "missing").as_deref() == Some(backend_missing)
        && list_field(readiness, "external_promotion_gates").as_deref() == Some(backend_gates)
        && readiness.get("external_promotion_gate_statuses") == backend_gate_statuses
        && string_field(readiness, "runtime_version_comparison").as_deref()
            == Some(backend_runtime_version_comparison)
        && readiness.get("runtime_version_matches") == backend_runtime_version_matches
        && string_field(readiness, "runtime_version_comparison_basis").as_deref()
            == Some(backend_runtime_version_basis)
}

fn status_value(map: &BTreeMap<String, Value>, key: &str) -> String {
    map.get(key)
        .and_then(value_display)
        .unwrap_or_else(|| "unknown".into())
}

fn print_evidence_sources() {
    println!("## Evidence Sources");
    println!();
    println!("| Source class | Reference | Covered behavior |");
    println!("| --- | --- | --- |");
    for (source, reference, covered) in [
        (
            "Audited SDK/header",
            "Header inventory output",
            "Available NI-DAQmx symbols and header identity only",
        ),
        (
            "Audited FFI source",
            "FFI source inventory output",
            "Generated binding source, platform cfgs, and symbol availability only",
        ),
        (
            "Audited target scope",
            "Target-scope audit output",
            "numanager Cargo feature, target cfg, helper-wrapper, and readiness boundary only",
        ),
        (
            "Vendor package/runtime",
            "Package input inventory and runtime probe outputs",
            "Package identity and loaded runtime version only",
        ),
        (
            "Bench run",
            "Command output log, inventory output, electrical readback, and runtime API output",
            "Physical channel mapping, task behavior, I/O behavior, cleanup, and runtime publication",
        ),
    ] {
        println!("| {source} | {reference} | {covered} |");
    }
    println!();
}

fn print_setup_and_safety() {
    println!("## Setup And Safety");
    println!();
    println!("| Area | Observed or enforced behavior |");
    println!("| --- | --- |");
    for area in [
        "Motion limits and homing state",
        "Laser/light output limits and interlocks",
        "Voltage/current/load limits",
        "Emergency stop or safe shutdown",
        "DAQmx safe output state after stop/clear",
        "Fault injection or recovery tested",
    ] {
        println!("| {area} | Unknown |");
    }
    println!();
}

fn print_required_artifacts() {
    println!("## Required Artifacts");
    println!();
    println!("| Artifact | Path or value |");
    println!("| --- | --- |");
    for (artifact, value) in [
        (
            "External-gates audit command",
            "`scripts/audit-ni-daqmx-external-gates.sh`",
        ),
        ("External-gates audit output", ""),
        (
            "Package input inventory command",
            "`scripts/audit-ni-daqmx-package-inputs.sh <installer-file-or-directory>`",
        ),
        ("Package input inventory output", ""),
        ("SDK header path or archive", ""),
        (
            "Header inventory command",
            "`scripts/audit-ni-daqmx-sdk-headers.sh <header-file-or-directory>`",
        ),
        ("Header inventory SHA-256", ""),
        ("Header inventory NIDAQmx.h count", ""),
        ("Header inventory NIDAQmx.h path", ""),
        ("Installed target-platform NIDAQmx.h used for bindgen", ""),
        ("Bindgen regeneration command", ""),
        (
            "FFI source inventory command",
            "`scripts/audit-ni-daqmx-sys-source.sh <ni-daqmx-sys-repo>`",
        ),
        ("FFI source inventory output", ""),
        (
            "Target-scope audit command",
            "`scripts/audit-ni-daqmx-target-scope.sh`",
        ),
        ("Target-scope audit output", ""),
        (
            "No-hardware helper audit command",
            "`scripts/audit-ni-daqmx-no-hardware-helpers.sh`",
        ),
        ("No-hardware helper audit output", ""),
        (
            "Plan-validation audit command",
            "`scripts/audit-ni-daqmx-plan-validation.sh`",
        ),
        ("Plan-validation audit output", ""),
        (
            "Live-gate audit command",
            "`scripts/audit-ni-daqmx-live-gate.sh`",
        ),
        ("Live-gate audit output", ""),
        (
            "Runtime-probe audit command",
            "`scripts/audit-ni-daqmx-runtime-probe.sh`",
        ),
        ("Runtime-probe audit output", ""),
        (
            "Example-output sync audit command",
            "`scripts/audit-ni-daqmx-example-output-sync.sh`",
        ),
        ("Example-output sync audit output", ""),
        ("Runtime probe output", ""),
        (
            "Backend inventory readiness table",
            "`## Backend Inventory`",
        ),
        ("Bench safety preconditions table", "`## Setup And Safety`"),
        ("LSM bring-up plan output", ""),
        (
            "LSM bring-up backend_readiness line",
            "`backend_readiness: ... runtime_version=... promotion_gate_statuses=[pending=9]`",
        ),
        ("Helper build output", ""),
        ("Inventory helper output", ""),
        ("Task lifecycle helper output", ""),
        ("Channel setup helper output", ""),
        ("Plan setup helper output", ""),
        ("Electrical readback or loopback log", ""),
        ("Runtime API output for promoted operation", ""),
    ] {
        println!("| {artifact} | {value} |");
    }
    println!();
}

fn print_physical_channel_mapping(capture: &Value, signal: &Value) {
    println!("## Physical Channel Mapping");
    println!();
    println!("| Role | Configured channel | Inventory channel | Bench note |");
    println!("| --- | --- | --- | --- |");
    for (role, channel) in [
        (
            "X galvo / piezo AO",
            role_channel(capture, "x_galvo").unwrap_or_default(),
        ),
        (
            "Y galvo / piezo AO",
            role_channel(capture, "y_galvo").unwrap_or_default(),
        ),
        (
            "Laser gate DO",
            role_channel(capture, "laser_gate").unwrap_or_default(),
        ),
        (
            "Frame or line trigger DO",
            route_source(capture, "start_trigger").unwrap_or_default(),
        ),
        (
            "Analog detector AI",
            first_channel_by_setup_kind(&[signal], "ai").unwrap_or_default(),
        ),
        (
            "APD counter CI",
            first_channel_by_setup_kind(&[capture, signal], "ci").unwrap_or_default(),
        ),
        (
            "Sample clock CO",
            role_channel(capture, "sample_clock").unwrap_or_default(),
        ),
    ] {
        let channel = if channel.trim().is_empty() {
            String::new()
        } else {
            format!("`{}`", markdown_table_code(&channel))
        };
        println!("| {role} | {channel} |  |  |");
    }
    println!();
}

fn print_output_input_validation(capture: &Value, signal: &Value) {
    let ao_channel = first_channel_by_setup_kind(&[capture], "ao").unwrap_or_default();
    let do_channel = first_channel_by_setup_kind(&[capture], "do").unwrap_or_default();
    let ai_channel = first_channel_by_setup_kind(&[signal], "ai").unwrap_or_default();
    let ci_channel = first_channel_by_setup_kind(&[capture, signal], "ci").unwrap_or_default();
    let co_channel = first_channel_by_setup_kind(&[capture], "co").unwrap_or_default();

    println!("## Output And Input Validation");
    println!();
    println!(
        "Output-writing and input-reading validation requires completed channel setup evidence and recorded hardware safety constraints before any channel is driven."
    );
    println!();
    println!("| Capability | Request or setpoint | Planned channel | Runtime output | Hardware readback | Result | Notes |");
    println!("| --- | --- | --- | --- | --- | --- | --- |");
    for (capability, request, channel, readback) in [
        (
            "AO voltage",
            "Low safe voltage",
            ao_channel,
            "Meter or loopback voltage",
        ),
        (
            "DO TTL",
            "Low/high transition",
            do_channel,
            "Scope, meter, or loopback",
        ),
        (
            "AI voltage",
            "Known source or AO loopback",
            ai_channel,
            "Reported voltage vs source",
        ),
        (
            "CI count",
            "Known pulse source or CO loopback",
            ci_channel,
            "Count rate/count total",
        ),
        (
            "CO pulse",
            "Safe frequency and count",
            co_channel,
            "Scope or CI loopback",
        ),
    ] {
        let channel = if channel.trim().is_empty() {
            String::new()
        } else {
            format!("`{}`", markdown_table_code(&channel))
        };
        println!("| {capability} | {request} | {channel} |  | {readback} | Unknown |  |");
    }
    println!();
}

fn print_lsm_task_execution_gate() {
    println!("## LSM Task Execution Gate");
    println!();
    println!(
        "Do not expose live `ConfocalImageCapture`, `ConfocalImageStream`, or `ScanSignalStream` until these rows have evidence."
    );
    println!();
    println!("| Behavior | Evidence required | Result |");
    println!("| --- | --- | --- |");
    for (behavior, evidence) in [
        (
            "Finite task creation order",
            "Bench log for AO/DO/AI/CI/CO tasks",
        ),
        (
            "Routing plan topology",
            "`routing_plan` clock producer/consumers and trigger consumers match the bench wiring",
        ),
        (
            "Sample-clock routing",
            "Confirmed source and dependent-task route names",
        ),
        (
            "Derived sample-clock source",
            "If no explicit sample-clock source is configured, the derived `/Device/CtrNInternalOutput` route for the counter-output sample clock is accepted by DAQmx for all AO/DO/AI/CI consumers",
        ),
        (
            "Start-trigger routing",
            "Confirmed digital edge route and start order",
        ),
        (
            "Planned buffer dimensions",
            "`scan_buffer_plan`, `signal_buffer_plan`, and task `buffer_plan` dimensions match the bench request",
        ),
        (
            "Task timing intent",
            "Preflight `planned_timing` rows match configured sample-clock and implicit finite counter-output timing before setup or reads/writes are enabled",
        ),
        (
            "Finite runtime sequence",
            "Preflight `planned_runtime_sequence` and `planned_completion` rows match expected buffered-write, start, read, wait, stop, and clear ordering before live execution is enabled",
        ),
        (
            "Execution contract intent",
            "Public `daqmx_task_plan.execution_contract` and Preflight `planned_execution_contract` rows for raster and signal plans match the intended buffered-before-start write policy, `auto_start=false`, finite read order, wait order, timeout, layout, and publish-after-validated-read policy",
        ),
        (
            "Live executor intent",
            "Public `daqmx_task_plan.live_executor_plan` and preflight `planned_live_executor` rows match the intended SDK task-wrapper backend, readiness gate, phase order, DAQmx API surface, and required validation gates while `executor_status=not_enabled_pending_hardware_validation`",
        ),
        (
            "Reconstruction intent",
            "Public raster `daqmx_task_plan.reconstruction_plan` and preflight `planned_reconstruction` rows match the intended sample-to-pixel mapping, dimensions, accumulation, saturation, and publish-after-reconstruction gate before hardware-derived frames are enabled",
        ),
        (
            "Runtime publication intent",
            "Preflight `planned_publication` rows match the configured raster `FrameReady` or signal `ScanSignalChunk` output contract before hardware-derived runtime events are enabled, using public metadata names such as `frame_handle`, `stream`, `line_index`, `chunk_index`, `first_sample_index`, `sample_count`, and `sample_values`",
        ),
        (
            "Raster timing intent",
            "Preflight `raster_timing_preview` rows match configured sample rate, pixel period, line period, frame period, and total duration before any live writes are enabled",
        ),
        (
            "Signal timing intent",
            "Preflight `signal_timing_preview` rows match configured sample rate, samples_per_line, lines, chunk size, chunk period, line period, and total duration before reads are enabled",
        ),
        (
            "Waveform intent",
            "Raster AO/DO `waveform_plan` and preflight `waveform_preview` rows match expected scan and laser-gate timing before any live writes are enabled",
        ),
        (
            "Cleanup plan",
            "`cleanup_plan` and Preflight `planned_cleanup` rows for failure modes, stop/clear order, configured `daqmx_timeout`, and safe-output-state evidence match the bench run",
        ),
        (
            "Buffered AO/DO writes",
            "Written sample counts and idle/safe final state",
        ),
        (
            "AI/CI reads",
            "Expected sample count, timeout behavior, data layout",
        ),
        (
            "Runtime capture frame publication",
            "`ConfocalImageCapture` `FrameReady` output from numanager with frame handle, final-frame width/height, pixel format, scan/reconstruction dimensions, reconstructed pixel size, sample rate, line dwell, detector metadata, and saturated-pixel status",
        ),
        (
            "Runtime live frame stream publication",
            "`ConfocalImageStream` `FrameReady` output from numanager with stream id, repeated frame handles, dirty-region/update metadata, frame dimensions, pixel format, scan/reconstruction dimensions, reconstructed pixel size, timing metadata, detector metadata, and progress/status events",
        ),
        (
            "Runtime signal chunk publication",
            "`ScanSignalStream` `ScanSignalChunk` output with stream id, channel names, timing origin, line/chunk/first-sample indices, sample count, sample rate, sample period, sample values, dropped sample/chunk counters, overflow status, and progress/status events",
        ),
        (
            "User stop/cancel",
            "Observed stop, clear, and safe output state",
        ),
        (
            "Failure cleanup",
            "Partial setup/start/wait/read failure clears all created tasks; lifecycle-helper failures after task start should capture `cleanup_after_lifecycle_error` and `stopped_task_after_error` rows, setup-helper failures should capture `cleared_partial_task` and `cleanup_after_setup_error` rows when applicable, and I/O-smoke failures after task start should capture `cleanup_after_io_error` and `stopped_task_after_error` rows",
        ),
    ] {
        println!("| {behavior} | {evidence} | Unknown |");
    }
    println!();
}

fn required_commands(capture: &Value, signal: &Value) -> Vec<String> {
    let lsm_env_prefix = crate::lsm_daqmx_commands::daqmx_lsm_env_prefix();
    let mut commands = vec![
        "scripts/audit-ni-daqmx-external-gates.sh".into(),
        "scripts/audit-ni-daqmx-package-inputs.sh <installer-file-or-directory>".into(),
        "scripts/audit-ni-daqmx-sdk-headers.sh <header-file-or-directory>".into(),
        "scripts/audit-ni-daqmx-sys-source.sh <ni-daqmx-sys-repo>".into(),
        "scripts/audit-ni-daqmx-target-scope.sh".into(),
        "scripts/audit-ni-daqmx-no-hardware-helpers.sh".into(),
        "scripts/audit-ni-daqmx-plan-validation.sh".into(),
        "scripts/audit-ni-daqmx-live-gate.sh".into(),
        "scripts/audit-ni-daqmx-runtime-probe.sh".into(),
        "scripts/audit-ni-daqmx-example-output-sync.sh".into(),
    ];
    commands.extend(crate::lsm_daqmx_commands::runtime_probe_commands());
    commands.push(format!(
        "{lsm_env_prefix}cargo run -p numanager-examples -- lsm_daqmx_bringup_plan"
    ));
    commands.push("cargo build -p numanager-imswitch-daqmx --features ni-daqmx-sdk --bins".into());
    commands.extend(crate::lsm_daqmx_commands::inventory_commands());
    commands.extend(preflight_commands(capture));
    commands.extend(preflight_commands(signal));
    commands.extend(crate::lsm_daqmx_commands::task_lifecycle_dry_run_commands());
    commands.extend(crate::lsm_daqmx_commands::task_lifecycle_cleanup_simulation_commands());
    commands.extend(
        [
            crate::lsm_daqmx_commands::plan_setup_cleanup_simulation_command(capture),
            crate::lsm_daqmx_commands::plan_setup_cleanup_simulation_command(signal),
        ]
        .into_iter()
        .flatten(),
    );
    commands.extend(crate::lsm_daqmx_commands::invalid_numeric_guard_commands(
        &[capture, signal],
    ));
    commands.extend(crate::lsm_daqmx_commands::task_lifecycle_setup_commands());
    commands.extend(
        crate::lsm_daqmx_commands::channel_setup_commands(capture, true)
            .into_iter()
            .chain(crate::lsm_daqmx_commands::channel_setup_commands(
                signal, true,
            ))
            .collect::<BTreeSet<_>>(),
    );
    commands.extend(
        crate::lsm_daqmx_commands::channel_setup_commands(capture, false)
            .into_iter()
            .chain(crate::lsm_daqmx_commands::channel_setup_commands(
                signal, false,
            ))
            .collect::<BTreeSet<_>>(),
    );
    commands.extend(setup_commands(capture));
    commands.extend(setup_commands(signal));
    commands.extend(
        crate::lsm_daqmx_commands::io_smoke_commands(capture, false)
            .into_iter()
            .chain(crate::lsm_daqmx_commands::io_smoke_commands(signal, false))
            .collect::<BTreeSet<_>>(),
    );
    commands.extend(
        crate::lsm_daqmx_commands::io_smoke_cleanup_simulation_commands(capture)
            .into_iter()
            .chain(crate::lsm_daqmx_commands::io_smoke_cleanup_simulation_commands(signal))
            .collect::<BTreeSet<_>>(),
    );
    commands.extend(
        crate::lsm_daqmx_commands::io_smoke_commands(capture, true)
            .into_iter()
            .chain(crate::lsm_daqmx_commands::io_smoke_commands(signal, true))
            .collect::<BTreeSet<_>>(),
    );
    commands
}

fn optional_env_table_value(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(|value| format!("`{}`", markdown_table_code(&value)))
}

fn role_channel(value: &Value, role: &str) -> Option<String> {
    let plan = daqmx_plan(value)?;
    let Some(Value::Map(roles)) = plan.get("role_channels") else {
        return None;
    };
    let Some(Value::Map(channel)) = roles.get(role) else {
        return None;
    };
    string_field(channel, "physical")
}

fn route_source(value: &Value, route: &str) -> Option<String> {
    let plan = daqmx_plan(value)?;
    let Some(Value::Map(routes)) = plan.get("routing_plan") else {
        return None;
    };
    let Some(Value::Map(route)) = routes.get(route) else {
        return None;
    };
    string_field(route, "source")
}

fn signal_channels(value: &Value) -> Vec<String> {
    let channels = daqmx_tasks(value)
        .into_iter()
        .flat_map(|task| first_physical_channel(task))
        .collect::<Vec<_>>();
    if channels.is_empty() {
        list_plan_field(value, "requested_signal_channels").unwrap_or_default()
    } else {
        channels
    }
}

fn cleanup_timeout(value: &Value) -> Option<String> {
    let plan = daqmx_plan(value)?;
    let Some(Value::Map(cleanup)) = plan.get("cleanup_plan") else {
        return None;
    };
    value_display(
        cleanup
            .get("wait_timeout")
            .or_else(|| cleanup.get("stop_timeout"))?,
    )
}

fn list_plan_field(value: &Value, key: &str) -> Option<Vec<String>> {
    let plan = daqmx_plan(value)?;
    list_field(plan, key)
}

fn first_channel_by_setup_kind(values: &[&Value], kind: &str) -> Option<String> {
    values.iter().find_map(|value| {
        daqmx_tasks(value).into_iter().find_map(|task| {
            (setup_kind(task) == Some(kind))
                .then(|| first_physical_channel(task))
                .flatten()
        })
    })
}

fn first_physical_channel(task: &BTreeMap<String, Value>) -> Option<String> {
    let Some(Value::List(channels)) = task.get("physical_channels") else {
        return None;
    };
    channels.iter().find_map(|channel| match channel {
        Value::String(channel) => Some(channel.clone()),
        _ => None,
    })
}

fn remaining_uncertainties() -> &'static [(&'static str, &'static str, &'static str)] {
    &[
        (
            "Package/license boundary",
            "Local installer identities, Linux package license-file identities, and Windows online-installer PE/payload metadata are recorded, but legal review has not established redistribution permission and the installed Windows package/license boundary has not been audited",
            "Completed package-intake note with legal review for exact Linux and Windows inputs",
        ),
        (
            "Installed 26.5 headers",
            "The 26.5 Linux package input and Windows online installer are identified, but no installed 26.5 NIDAQmx.h tree has been audited for either target platform",
            "Passing header inventory, recorded bindgen regeneration command, and bindgen-source audit from the same installed Linux or Windows 26.5 target-platform NIDAQmx.h before publishing regenerated 26.5 bindings",
        ),
        (
            "Linux NI-PAL readiness",
            "On the current Linux host, NI-PAL can abort the process during inventory or empty-task creation",
            "Bench host log showing runtime probe, process-isolated version probe, process-isolated inventory, and empty task create/clear without process abort",
        ),
        (
            "Physical channel mapping",
            "Configured Dev1 role channels are plan inputs, not proof that those channels exist or are safely wired",
            "Inventory output plus bench mapping for AO/DO/AI/CI/CO role channels",
        ),
        (
            "Routing semantics",
            "routing_plan records candidate clock/trigger topology, but route source strings and start order are not validated on hardware",
            "Plan-setup and bench logs showing accepted timing/trigger configuration and the observed task order",
        ),
        (
            "Output safety",
            "AO/DO/CO helper commands are gated, but safe voltage, TTL state, load, final idle state, and pulse count are not proven",
            "Meter/scope/loopback evidence for reviewed safe setpoints and cleanup behavior",
        ),
        (
            "Input semantics",
            "AI/CI reads are planned, but sample layout, counts, timeout behavior, and APD/count scaling are not proven",
            "Known-source or loopback readback logs for AI/CI, including sample count and timeout observations",
        ),
        (
            "Runtime publication",
            "Simulator publishes ConfocalImageCapture FrameReady, ConfocalImageStream FrameReady updates, and ScanSignalStream ScanSignalChunk output with the public metadata contract; the DAQmx backend does not yet publish hardware-derived frames/chunks",
            "Hardware-backed runtime output logs showing capture FrameReady final-frame metadata, live-stream FrameReady update/dirty-region/progress metadata, and ScanSignalChunk channel/timing/sample/drop/overflow/progress metadata after task execution behavior is validated",
        ),
        (
            "Failure cleanup",
            "Helper cleanup paths are implemented for lifecycle errors after start, partial setup, and post-start I/O failure, but real DAQmx failure modes are not characterized",
            "Bench logs capturing cleanup rows after controlled start/wait/setup/read/write failures",
        ),
    ]
}

fn markdown_table_code(value: &str) -> String {
    value
        .replace('|', r"\|")
        .replace('`', "'")
        .replace('\n', " ")
}

fn markdown_table_text(value: &str) -> String {
    value.replace('|', r"\|").replace('\n', " ")
}

fn preflight_commands(value: &Value) -> Vec<String> {
    string_plan_field(value, "plan_preflight_helper_command")
        .into_iter()
        .collect()
}

fn setup_commands(value: &Value) -> Vec<String> {
    string_plan_field(value, "plan_setup_helper_command")
        .into_iter()
        .collect()
}

fn string_plan_field(value: &Value, key: &str) -> Option<String> {
    let plan = daqmx_plan(value)?;
    string_field(plan, key)
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

fn daqmx_tasks(value: &Value) -> Vec<&BTreeMap<String, Value>> {
    let Some(plan) = daqmx_plan(value) else {
        return Vec::new();
    };
    let Some(Value::List(tasks)) = plan.get("tasks") else {
        return Vec::new();
    };
    tasks
        .iter()
        .filter_map(|task| match task {
            Value::Map(task) => Some(task),
            _ => None,
        })
        .collect()
}

fn print_preflight_targets(label: &str, value: &Value) {
    let Some(plan) = daqmx_plan(value) else {
        return;
    };
    println!("### {label}");
    println!();
    print_target_line("Tasks", task_expectations(plan));
    print_target_line("Live readiness", live_readiness_expectations(plan));
    print_target_line("Start order", order_expectation(plan, "start_order"));
    print_target_line("Read order", order_expectation(plan, "read_order"));
    print_target_line("Clear order", order_expectation(plan, "clear_order"));
    print_target_line("Routes", route_expectations(plan));
    print_target_line("Timing", timing_expectations(plan));
    print_target_line("Waveforms", waveform_expectations(plan));
    print_target_line("Transfers", transfer_expectations(plan));
    print_target_line("Runtime sequence", runtime_sequence_expectations(plan));
    print_target_line("Completion", completion_expectations(plan));
    print_target_line("Execution contract", execution_contract_expectations(plan));
    print_target_line("Live executor", live_executor_expectations(plan));
    print_target_line("Reconstruction", reconstruction_expectations(plan));
    print_target_line("Publication", publication_expectations(plan));
    print_target_line("Cancel", cancel_expectations(plan));
    print_target_line("Cleanup", cleanup_expectations(plan));
    println!();
}

fn print_target_line(label: &str, values: Vec<String>) {
    if values.is_empty() {
        println!("- {label}: none");
    } else {
        println!("- {label}: {}", values.join("; "));
    }
}

fn live_readiness_expectations(plan: &BTreeMap<String, Value>) -> Vec<String> {
    crate::lsm_common::daqmx_live_readiness_summary(plan)
        .map(|summary| vec![summary])
        .unwrap_or_default()
}

fn task_expectations(plan: &BTreeMap<String, Value>) -> Vec<String> {
    daqmx_tasks_from_plan(plan)
        .into_iter()
        .filter_map(|task| {
            let name = string_field(task, "name")?;
            let role = string_field(task, "role")?;
            let channels = list_field(task, "physical_channels")
                .filter(|channels| !channels.is_empty())
                .map(|channels| channels.join("+"))
                .unwrap_or_else(|| "none".into());
            Some(format!("{name}:{role}:{channels}"))
        })
        .collect()
}

fn order_expectation(plan: &BTreeMap<String, Value>, key: &str) -> Vec<String> {
    list_field(plan, key)
        .filter(|values| !values.is_empty())
        .map(|values| vec![values.join(">")])
        .unwrap_or_default()
}

fn route_expectations(plan: &BTreeMap<String, Value>) -> Vec<String> {
    let Some(Value::Map(routes)) = plan.get("routing_plan") else {
        return Vec::new();
    };
    let mut expectations = Vec::new();
    if let Some(Value::Map(clock)) = routes.get("sample_clock") {
        let source = string_field(clock, "source").unwrap_or_else(|| "<empty>".into());
        let producer = string_field(clock, "producer_task").unwrap_or_else(|| "none".into());
        let consumers = list_field(clock, "consumers")
            .filter(|values| !values.is_empty())
            .map(|values| values.join("+"))
            .unwrap_or_else(|| "none".into());
        expectations.push(format!(
            "sample_clock source={source} producer={producer} consumers={consumers}"
        ));
    }
    if let Some(Value::Map(trigger)) = routes.get("start_trigger") {
        let source = string_field(trigger, "source").unwrap_or_else(|| "<empty>".into());
        let consumers = list_field(trigger, "consumers")
            .filter(|values| !values.is_empty())
            .map(|values| values.join("+"))
            .unwrap_or_else(|| "none".into());
        expectations.push(format!(
            "start_trigger source={source} consumers={consumers}"
        ));
    }
    expectations
}

fn waveform_expectations(plan: &BTreeMap<String, Value>) -> Vec<String> {
    daqmx_tasks_from_plan(plan)
        .into_iter()
        .filter_map(|task| {
            let name = string_field(task, "name")?;
            let Some(Value::Map(waveform)) = task.get("waveform_plan") else {
                return None;
            };
            let pattern = string_field(waveform, "pattern").unwrap_or_else(|| "unknown".into());
            let evidence = string_field(waveform, "waveform_evidence_status")
                .unwrap_or_else(|| "unknown".into());
            Some(format!("{name}:{pattern}:{evidence}"))
        })
        .collect()
}

fn timing_expectations(plan: &BTreeMap<String, Value>) -> Vec<String> {
    daqmx_tasks_from_plan(plan)
        .into_iter()
        .filter_map(|task| {
            let name = string_field(task, "name")?;
            let role = string_field(task, "role")?;
            let samples = numeric_field(task, "samples_per_channel")
                .map(|value| value.to_string())
                .unwrap_or_else(|| "?".into());
            let sample_rate =
                value_display(task.get("sample_rate")?).unwrap_or_else(|| "unknown".into());
            Some(format!(
                "{name}:{role}:sample_rate={sample_rate}:samples={samples}"
            ))
        })
        .collect()
}

fn transfer_expectations(plan: &BTreeMap<String, Value>) -> Vec<String> {
    daqmx_tasks_from_plan(plan)
        .into_iter()
        .filter_map(|task| {
            let name = string_field(task, "name")?;
            let role = string_field(task, "role")?;
            let Some(Value::Map(buffer)) = task.get("buffer_plan") else {
                return None;
            };
            let direction = string_field(buffer, "direction").unwrap_or_else(|| "unknown".into());
            let element = string_field(buffer, "element_type").unwrap_or_else(|| "unknown".into());
            let channels = numeric_field(buffer, "channel_count")
                .map(|value| value.to_string())
                .unwrap_or_else(|| "?".into());
            let samples = numeric_field(buffer, "samples_per_channel")
                .map(|value| value.to_string())
                .unwrap_or_else(|| "?".into());
            Some(format!(
                "{name}:{role}:{direction}:{element}:{channels}chx{samples}"
            ))
        })
        .collect()
}

fn runtime_sequence_expectations(plan: &BTreeMap<String, Value>) -> Vec<String> {
    let Some(Value::List(phases)) = plan.get("runtime_sequence") else {
        return Vec::new();
    };
    phases
        .iter()
        .filter_map(|phase| {
            let Value::Map(phase) = phase else {
                return None;
            };
            let step = numeric_field(phase, "step")
                .map(|value| value.to_string())
                .unwrap_or_else(|| "?".into());
            let phase_name = string_field(phase, "phase")?;
            let basis = string_field(phase, "basis").unwrap_or_else(|| "unknown".into());
            let tasks = list_field(phase, "tasks")
                .filter(|values| !values.is_empty())
                .map(|values| values.join(">"))
                .unwrap_or_else(|| "none".into());
            Some(format!("step={step}:{phase_name}:{tasks}:{basis}"))
        })
        .collect()
}

fn completion_expectations(plan: &BTreeMap<String, Value>) -> Vec<String> {
    let Some(Value::Map(completion)) = plan.get("completion_plan") else {
        return Vec::new();
    };
    let mode = string_field(completion, "mode").unwrap_or_else(|| "unknown".into());
    let samples = numeric_field(completion, "samples_per_channel")
        .map(|value| value.to_string())
        .unwrap_or_else(|| "?".into());
    let timeout = completion
        .get("timeout")
        .and_then(value_display)
        .unwrap_or_else(|| "unknown".into());
    let evidence = string_field(completion, "evidence_status").unwrap_or_else(|| "unknown".into());
    vec![format!(
        "mode={mode}:samples={samples}:timeout={timeout}:evidence={evidence}"
    )]
}

fn execution_contract_expectations(plan: &BTreeMap<String, Value>) -> Vec<String> {
    let Some(Value::Map(contract)) = plan.get("execution_contract") else {
        return Vec::new();
    };
    let mode = string_field(contract, "mode").unwrap_or_else(|| "unknown".into());
    let write = list_field(contract, "write_order")
        .filter(|values| !values.is_empty())
        .map(|values| values.join(">"))
        .unwrap_or_else(|| "none".into());
    let read = list_field(contract, "read_order")
        .filter(|values| !values.is_empty())
        .map(|values| values.join(">"))
        .unwrap_or_else(|| "none".into());
    let wait = list_field(contract, "wait_order")
        .filter(|values| !values.is_empty())
        .map(|values| values.join(">"))
        .unwrap_or_else(|| "none".into());
    let auto_start = bool_field(contract, "write_auto_start")
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".into());
    let write_layout = string_field(contract, "write_layout").unwrap_or_else(|| "unknown".into());
    let read_layout = string_field(contract, "read_layout").unwrap_or_else(|| "unknown".into());
    let timeout = contract
        .get("timeout")
        .and_then(value_display)
        .unwrap_or_else(|| "unknown".into());
    let publication_policy =
        string_field(contract, "publication_policy").unwrap_or_else(|| "unknown".into());
    let evidence =
        string_field(contract, "contract_evidence_status").unwrap_or_else(|| "unknown".into());
    vec![format!(
        "mode={mode}:write={write}:read={read}:wait={wait}:write_auto_start={auto_start}:write_layout={write_layout}:read_layout={read_layout}:timeout={timeout}:publication_policy={publication_policy}:evidence={evidence}"
    )]
}

fn live_executor_expectations(plan: &BTreeMap<String, Value>) -> Vec<String> {
    let Some(Value::Map(executor)) = plan.get("live_executor_plan") else {
        return Vec::new();
    };
    let mode = string_field(executor, "mode").unwrap_or_else(|| "unknown".into());
    let status = string_field(executor, "executor_status").unwrap_or_else(|| "unknown".into());
    let backend = string_field(executor, "backend").unwrap_or_else(|| "unknown".into());
    let evidence =
        string_field(executor, "execution_evidence_status").unwrap_or_else(|| "unknown".into());
    let validation = list_field(executor, "required_validation")
        .filter(|values| !values.is_empty())
        .map(|values| values.join("+"))
        .unwrap_or_else(|| "none".into());
    let phases = executor_phase_expectations(executor)
        .into_iter()
        .map(|phase| format!("phase={phase}"))
        .collect::<Vec<_>>()
        .join(",");
    vec![format!(
        "mode={mode}:status={status}:backend={backend}:phases={phases}:required_validation={validation}:evidence={evidence}"
    )]
}

fn executor_phase_expectations(executor: &BTreeMap<String, Value>) -> Vec<String> {
    let Some(Value::List(phases)) = executor.get("phases") else {
        return Vec::new();
    };
    phases
        .iter()
        .filter_map(|phase| {
            let Value::Map(phase) = phase else {
                return None;
            };
            let step = numeric_field(phase, "step")
                .map(|value| value.to_string())
                .unwrap_or_else(|| "?".into());
            let phase_name = string_field(phase, "phase")?;
            let tasks = list_field(phase, "tasks")
                .filter(|values| !values.is_empty())
                .map(|values| values.join(">"))
                .unwrap_or_else(|| "none".into());
            let api_surface =
                string_field(phase, "api_surface").unwrap_or_else(|| "unknown".into());
            Some(format!("{step}:{phase_name}:{tasks}:{api_surface}"))
        })
        .collect()
}

fn reconstruction_expectations(plan: &BTreeMap<String, Value>) -> Vec<String> {
    let Some(Value::Map(reconstruction)) = plan.get("reconstruction_plan") else {
        return Vec::new();
    };
    let mode = string_field(reconstruction, "mode").unwrap_or_else(|| "unknown".into());
    let input = list_field(reconstruction, "input_tasks")
        .filter(|values| !values.is_empty())
        .map(|values| values.join("+"))
        .unwrap_or_else(|| "none".into());
    let scan_width = value_display(reconstruction.get("scan_width").unwrap_or(&Value::Null))
        .unwrap_or_else(|| "?".into());
    let scan_height = value_display(reconstruction.get("scan_height").unwrap_or(&Value::Null))
        .unwrap_or_else(|| "?".into());
    let reconstruction_width = value_display(
        reconstruction
            .get("reconstruction_width")
            .unwrap_or(&Value::Null),
    )
    .unwrap_or_else(|| "?".into());
    let reconstruction_height = value_display(
        reconstruction
            .get("reconstruction_height")
            .unwrap_or(&Value::Null),
    )
    .unwrap_or_else(|| "?".into());
    let pixel_format =
        string_field(reconstruction, "pixel_format").unwrap_or_else(|| "unknown".into());
    let mapping =
        string_field(reconstruction, "sample_to_pixel_mapping").unwrap_or_else(|| "unknown".into());
    let accumulation =
        string_field(reconstruction, "accumulation").unwrap_or_else(|| "unknown".into());
    let saturation =
        string_field(reconstruction, "saturation_policy").unwrap_or_else(|| "unknown".into());
    let evidence = string_field(reconstruction, "reconstruction_evidence_status")
        .unwrap_or_else(|| "unknown".into());
    vec![format!(
        "mode={mode}:input={input}:scan={scan_width}x{scan_height}:reconstruction={reconstruction_width}x{reconstruction_height}:pixel_format={pixel_format}:mapping={mapping}:accumulation={accumulation}:saturation={saturation}:evidence={evidence}"
    )]
}

fn publication_expectations(plan: &BTreeMap<String, Value>) -> Vec<String> {
    let Some(Value::Map(publication)) = plan.get("publication_plan") else {
        return Vec::new();
    };
    let event = string_field(publication, "event_kind").unwrap_or_else(|| "unknown".into());
    let mode = string_field(publication, "mode").unwrap_or_else(|| "unknown".into());
    let evidence = string_field(publication, "publication_evidence_status")
        .unwrap_or_else(|| "unknown".into());
    let required_metadata = list_field(publication, "required_metadata")
        .filter(|values| !values.is_empty())
        .map(|values| values.join("+"))
        .unwrap_or_else(|| "none".into());
    if event == "FrameReady" {
        let scan_width = value_display(publication.get("scan_width").unwrap_or(&Value::Null))
            .unwrap_or_else(|| "?".into());
        let scan_height = value_display(publication.get("scan_height").unwrap_or(&Value::Null))
            .unwrap_or_else(|| "?".into());
        let reconstruction_width = value_display(
            publication
                .get("reconstruction_width")
                .unwrap_or(&Value::Null),
        )
        .unwrap_or_else(|| "?".into());
        let reconstruction_height = value_display(
            publication
                .get("reconstruction_height")
                .unwrap_or(&Value::Null),
        )
        .unwrap_or_else(|| "?".into());
        let pixel_format =
            string_field(publication, "pixel_format").unwrap_or_else(|| "unknown".into());
        return vec![format!(
            "{event}:{mode}:scan={scan_width}x{scan_height}:reconstruction={reconstruction_width}x{reconstruction_height}:pixel_format={pixel_format}:required_metadata={required_metadata}:evidence={evidence}"
        )];
    }
    if event == "ScanSignalChunk" {
        let channels = list_field(publication, "channel_names")
            .filter(|values| !values.is_empty())
            .map(|values| values.join("+"))
            .unwrap_or_else(|| "none".into());
        let samples_per_line = numeric_field(publication, "samples_per_line")
            .map(|value| value.to_string())
            .unwrap_or_else(|| "?".into());
        let lines = numeric_field(publication, "lines")
            .map(|value| value.to_string())
            .unwrap_or_else(|| "?".into());
        let chunk_size = publication
            .get("chunk_size")
            .and_then(value_display)
            .unwrap_or_else(|| "none".into());
        return vec![format!(
            "{event}:{mode}:channels={channels}:samples_per_line={samples_per_line}:lines={lines}:chunk_size={chunk_size}:required_metadata={required_metadata}:evidence={evidence}"
        )];
    }
    vec![format!(
        "{event}:{mode}:required_metadata={required_metadata}:evidence={evidence}"
    )]
}

fn cancel_expectations(plan: &BTreeMap<String, Value>) -> Vec<String> {
    let Some(Value::Map(cancel)) = plan.get("cancel_plan") else {
        return Vec::new();
    };
    let strategy = string_field(cancel, "strategy").unwrap_or_else(|| "unknown".into());
    let stop = list_field(cancel, "stop_order")
        .filter(|values| !values.is_empty())
        .map(|values| values.join(">"))
        .unwrap_or_else(|| "none".into());
    let clear = list_field(cancel, "clear_order")
        .filter(|values| !values.is_empty())
        .map(|values| values.join(">"))
        .unwrap_or_else(|| "none".into());
    let safe_state = string_field(cancel, "safe_output_state").unwrap_or_else(|| "unknown".into());
    let evidence =
        string_field(cancel, "cancel_evidence_status").unwrap_or_else(|| "unknown".into());
    vec![format!(
        "strategy={strategy}:stop={stop}:clear={clear}:safe_output_state={safe_state}:evidence={evidence}"
    )]
}

fn cleanup_expectations(plan: &BTreeMap<String, Value>) -> Vec<String> {
    let Some(Value::Map(cleanup)) = plan.get("cleanup_plan") else {
        return Vec::new();
    };
    let policy = string_field(cleanup, "policy").unwrap_or_else(|| "unknown".into());
    let modes = list_field(cleanup, "failure_cleanup_modes")
        .filter(|values| !values.is_empty())
        .map(|values| values.join("+"))
        .unwrap_or_else(|| "none".into());
    let started = string_field(cleanup, "started_task_cleanup").unwrap_or_else(|| "unknown".into());
    let stop = list_field(cleanup, "stop_order")
        .filter(|values| !values.is_empty())
        .map(|values| values.join(">"))
        .unwrap_or_else(|| "none".into());
    let clear = list_field(cleanup, "clear_order")
        .filter(|values| !values.is_empty())
        .map(|values| values.join(">"))
        .unwrap_or_else(|| "none".into());
    let safe_state = string_field(cleanup, "safe_output_state").unwrap_or_else(|| "unknown".into());
    let evidence =
        string_field(cleanup, "cleanup_evidence_status").unwrap_or_else(|| "unknown".into());
    vec![format!(
        "policy={policy}:failure_modes={modes}:started_task_cleanup={started}:stop={stop}:clear={clear}:safe_output_state={safe_state}:evidence={evidence}"
    )]
}

fn daqmx_tasks_from_plan(plan: &BTreeMap<String, Value>) -> Vec<&BTreeMap<String, Value>> {
    let Some(Value::List(tasks)) = plan.get("tasks") else {
        return Vec::new();
    };
    tasks
        .iter()
        .filter_map(|task| match task {
            Value::Map(task) => Some(task),
            _ => None,
        })
        .collect()
}

fn setup_kind(task: &BTreeMap<String, Value>) -> Option<&'static str> {
    match task.get("role")? {
        Value::String(role) if role == "analog_output" => Some("ao"),
        Value::String(role) if role == "digital_output" => Some("do"),
        Value::String(role) if role == "analog_input" => Some("ai"),
        Value::String(role) if role == "counter_input" => Some("ci"),
        Value::String(role) if role == "counter_output" => Some("co"),
        _ => None,
    }
}

fn string_field(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    match map.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn list_field(map: &BTreeMap<String, Value>, key: &str) -> Option<Vec<String>> {
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

fn numeric_field(map: &BTreeMap<String, Value>, key: &str) -> Option<i64> {
    match map.get(key) {
        Some(Value::I64(value)) => Some(*value),
        _ => None,
    }
}

fn bool_field(map: &BTreeMap<String, Value>, key: &str) -> Option<bool> {
    match map.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn value_display(value: &Value) -> Option<String> {
    match value {
        Value::TimeInterval(value) => Some(format!("{:.6}s", value.seconds())),
        Value::Frequency(value) => Some(format!("{:.6}Hz", value.hertz())),
        Value::Position(value) => Some(format!("{value:?}")),
        Value::PixelCount(value) => Some(value.pixels().to_string()),
        Value::Ratio(value) => Some(format!("{value:?}")),
        Value::Decibel(value) => Some(format!("{value:?}")),
        Value::I64(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::String(value) => Some(value.clone()),
        _ => None,
    }
}
