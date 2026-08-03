use std::collections::BTreeMap;
use std::time::Duration;

use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverDiscovery, LocalRuntime, Runtime};
use numanager_core::{
    CapabilityKind, ConfocalImageCaptureRequest, ConfocalImageStreamRequest, DeviceDescriptor,
    DriverId, Error, ErrorCode, Event, Frame, Frequency, OperationId, OperationStatus, PixelCount,
    Result, ScanSignalStreamRequest, TimeInterval, Value,
};
use numanager_drivers::sim_lsm::SimLsmDriver;
use numanager_drivers::sim_microscope_lsm::SimMicroscopeLsmDriver;
use numanager_imswitch_daqmx::ImSwitchDaqmxDiscovery;

pub fn runtime_for_source(source: &str) -> Result<(LocalRuntime, DeviceDescriptor)> {
    match source {
        "imswitch" | "imswitch-daqmx" | "daqmx" => imswitch_runtime(),
        "sim-lsm" | "sim_lsm" | "sim" => sim_lsm_runtime(),
        "sim-composed" | "sim_microscope_lsm" | "sim-microscope-lsm" => composed_runtime(),
        other => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("unknown LSM source {other}"),
        )),
    }
}

fn imswitch_runtime() -> Result<(LocalRuntime, DeviceDescriptor)> {
    let mut properties = BTreeMap::new();
    insert_env_string(
        &mut properties,
        "device_name",
        "NUMANAGER_DAQMX_DEVICE_NAME",
    );
    insert_env_string(
        &mut properties,
        "lsm_x_galvo",
        "NUMANAGER_DAQMX_LSM_X_GALVO",
    );
    insert_env_string(
        &mut properties,
        "lsm_y_galvo",
        "NUMANAGER_DAQMX_LSM_Y_GALVO",
    );
    insert_env_string(
        &mut properties,
        "lsm_laser_gate",
        "NUMANAGER_DAQMX_LSM_LASER_GATE",
    );
    insert_env_string(
        &mut properties,
        "lsm_detector",
        "NUMANAGER_DAQMX_LSM_DETECTOR",
    );
    insert_env_string(
        &mut properties,
        "lsm_sample_clock",
        "NUMANAGER_DAQMX_LSM_SAMPLE_CLOCK",
    );
    insert_env_string(
        &mut properties,
        "lsm_sample_clock_source",
        "NUMANAGER_DAQMX_LSM_SAMPLE_CLOCK_SOURCE",
    );
    insert_env_string(
        &mut properties,
        "lsm_start_trigger_source",
        "NUMANAGER_DAQMX_LSM_START_TRIGGER_SOURCE",
    );
    insert_env_bool(
        &mut properties,
        "live_task_execution",
        "NUMANAGER_DAQMX_LIVE_TASK_EXECUTION",
    );
    insert_env_timeout_seconds(
        &mut properties,
        "daqmx_timeout",
        "NUMANAGER_DAQMX_TIMEOUT_SECONDS",
    )?;
    insert_env_timeout_seconds(
        &mut properties,
        "inventory_helper_timeout",
        "NUMANAGER_DAQMX_HELPER_TIMEOUT_SECONDS",
    )?;

    let hardware = HardwareConfig {
        devices: vec![DeviceConfig::new(
            1,
            "Configured ImSwitch DAQmx",
            "imswitch_daqmx",
            properties,
        )],
        ..HardwareConfig::default()
    };
    let mut discovery = ImSwitchDaqmxDiscovery::configured(DriverId(1), &hardware)?;
    let driver = discovery
        .detect()?
        .into_iter()
        .next()
        .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "missing ImSwitch DAQmx fixture"))?
        .into_driver();
    let mut runtime = LocalRuntime::new();
    runtime.add_driver(driver)?;
    let hub = runtime
        .device_by_capability(CapabilityKind::ConfocalImageCapture)?
        .clone();
    Ok((runtime, hub))
}

fn insert_env_string(properties: &mut BTreeMap<String, Value>, property: &str, env: &str) {
    if let Ok(value) = std::env::var(env) {
        properties.insert(property.into(), Value::String(value));
    }
}

fn insert_env_bool(properties: &mut BTreeMap<String, Value>, property: &str, env: &str) {
    if let Ok(value) = std::env::var(env) {
        let value = matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        );
        properties.insert(property.into(), Value::Bool(value));
    }
}

fn insert_env_timeout_seconds(
    properties: &mut BTreeMap<String, Value>,
    property: &str,
    env: &str,
) -> Result<()> {
    let Ok(value) = std::env::var(env) else {
        return Ok(());
    };
    let seconds = value.trim().parse::<f64>().map_err(|error| {
        Error::new(
            ErrorCode::InvalidProperty,
            format!("{env} must be a positive seconds value: {error}"),
        )
    })?;
    if seconds <= 0.0 || !seconds.is_finite() {
        return Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("{env} must be a positive finite seconds value"),
        ));
    }
    properties.insert(
        property.into(),
        Value::TimeInterval(TimeInterval::from_seconds(seconds)),
    );
    Ok(())
}

fn sim_lsm_runtime() -> Result<(LocalRuntime, DeviceDescriptor)> {
    let mut runtime = LocalRuntime::new();
    runtime.add_driver(Box::new(SimLsmDriver::simulated(DriverId(1))))?;
    let hub = runtime
        .device_by_capability(CapabilityKind::ConfocalImageCapture)?
        .clone();
    Ok((runtime, hub))
}

pub fn composed_runtime() -> Result<(LocalRuntime, DeviceDescriptor)> {
    let mut runtime = LocalRuntime::new();
    runtime.add_driver(Box::new(SimMicroscopeLsmDriver::simulated(DriverId(1))))?;
    let hub = runtime
        .device_by_capability(CapabilityKind::ConfocalImageCapture)?
        .clone();
    Ok((runtime, hub))
}

pub fn snapshot_request(width: i64, height: i64) -> ConfocalImageCaptureRequest {
    ConfocalImageCaptureRequest {
        scan: raster_scan(width, height, 1),
        reconstruction: raster_reconstruction(width, height),
    }
}

pub fn live_image_request(width: i64, height: i64) -> ConfocalImageStreamRequest {
    ConfocalImageStreamRequest {
        scan: raster_scan(width, height, 4),
        reconstruction: raster_reconstruction(width, height),
        update_policy: Some("dirty_region".into()),
        overwrite_previous_pixels: true,
    }
}

pub fn continuous_live_image_request(width: i64, height: i64) -> ConfocalImageStreamRequest {
    ConfocalImageStreamRequest {
        scan: raster_scan(width, height, 0),
        reconstruction: raster_reconstruction(width, height),
        update_policy: Some("dirty_region".into()),
        overwrite_previous_pixels: true,
    }
}

pub fn line_signal_request(width: i64, chunk_size: u64) -> ScanSignalStreamRequest {
    line_signal_request_channels(width, chunk_size, signal_channels())
}

pub fn continuous_line_signal_request(width: i64, chunk_size: u64) -> ScanSignalStreamRequest {
    let mut request = line_signal_request(width, chunk_size);
    request.timing.insert("lines".into(), Value::I64(0));
    request
}

/// Continuous form of [`line_signal_request_channels`]: `lines = 0` asks the
/// driver to keep scanning the line until the operation is cancelled, so a
/// client can fill a framebuffer from the chunks as they arrive.
pub fn continuous_line_signal_request_channels(
    width: i64,
    chunk_size: u64,
    channels: Vec<String>,
) -> ScanSignalStreamRequest {
    let mut request = line_signal_request_channels(width, chunk_size, channels);
    request.timing.insert("lines".into(), Value::I64(0));
    request
}

/// Continuous line scan that sweeps a whole raster: successive lines are
/// successive rows of a `width` x `height` scan, so a client can rebuild the
/// same image the capture and stream capabilities produce, row by row.
pub fn continuous_raster_line_signal_request(
    width: i64,
    height: i64,
    chunk_size: u64,
    channels: Vec<String>,
) -> ScanSignalStreamRequest {
    let mut request = continuous_line_signal_request_channels(width, chunk_size, channels);
    request.timing.insert(
        "height".into(),
        Value::PixelCount(PixelCount::new(pixel_count(height))),
    );
    request
}

pub fn line_signal_request_channels(
    width: i64,
    chunk_size: u64,
    channels: Vec<String>,
) -> ScanSignalStreamRequest {
    let detector_values = channels.iter().cloned().map(Value::String).collect();
    ScanSignalStreamRequest {
        timing: BTreeMap::from([
            ("mode".into(), Value::String("line_scan".into())),
            ("samples_per_line".into(), Value::I64(width)),
            ("lines".into(), Value::I64(1)),
            (
                "sample_rate".into(),
                Value::Frequency(Frequency::from_hertz(100_000.0)),
            ),
            ("bidirectional".into(), Value::Bool(false)),
            ("detectors".into(), Value::List(detector_values)),
        ]),
        channels,
        chunk_size: Some(chunk_size),
    }
}

pub fn run_request(
    runtime: &LocalRuntime,
    hub: &DeviceDescriptor,
    request: impl Into<numanager_core::CapabilityRequest>,
) -> Result<Value> {
    let op = runtime.submit_request(hub, request)?;
    runtime.wait_completed(op.id, Duration::from_secs(5))
}

pub fn api_result(value: &Value) -> String {
    match value {
        Value::Map(map) => map
            .iter()
            .filter(|(key, _)| !key.starts_with("last_"))
            .map(|(key, value)| format!("{key}={}", value_brief(value)))
            .collect::<Vec<_>>()
            .join(", "),
        other => value_brief(other),
    }
}

pub fn daqmx_task_plan_summary(value: &Value) -> Option<String> {
    let Value::Map(result) = value else {
        return None;
    };
    let Value::Map(plan) = result.get("daqmx_task_plan")? else {
        return None;
    };
    let mut parts = Vec::new();
    if let Some(execution) = string_field(plan, "execution_status") {
        parts.push(format!("execution={execution}"));
    }
    if let Some(blocker) = string_field(plan, "live_task_execution_blocker") {
        parts.push(format!("blocker={blocker}"));
    }
    if let Some(readiness) = daqmx_live_readiness_summary(plan) {
        parts.push(format!("readiness=[{readiness}]"));
    }
    if let Some(validation) = daqmx_plan_validation_summary(plan) {
        parts.push(format!("validation={validation}"));
    }
    if let Some(routing) = string_field(plan, "routing_evidence_status") {
        parts.push(format!("routing={routing}"));
    }
    if let Some(roles) = role_channels_summary(plan) {
        parts.push(format!("roles=[{roles}]"));
    }
    if let Some(clock) = string_field(plan, "sample_clock_source") {
        parts.push(format!("clock={clock}"));
    }
    if let Some(trigger) = string_field(plan, "start_trigger_source") {
        parts.push(format!("trigger={trigger}"));
    }
    if let Some(buffers) = daqmx_buffer_plan_summary(plan) {
        parts.push(format!("buffers=[{buffers}]"));
    }
    if let Some(timeout_s) = cleanup_timeout_seconds(plan) {
        parts.push(format!("cleanup_timeout_s={timeout_s:.3}"));
    }
    if let Some(waveforms) = daqmx_waveform_plan_summary(plan) {
        parts.push(format!("waveforms=[{waveforms}]"));
    }
    if let Some(routes) = daqmx_routing_plan_summary(plan) {
        parts.push(format!("routes=[{routes}]"));
    }
    if let Some(sequence) = daqmx_runtime_sequence_summary(plan) {
        parts.push(format!("sequence=[{sequence}]"));
    }
    if let Some(completion) = daqmx_completion_plan_summary(plan) {
        parts.push(format!("completion=[{completion}]"));
    }
    if let Some(contract) = daqmx_execution_contract_summary(plan) {
        parts.push(format!("contract=[{contract}]"));
    }
    if let Some(executor) = daqmx_live_executor_plan_summary(plan) {
        parts.push(format!("executor=[{executor}]"));
    }
    if let Some(reconstruction) = daqmx_reconstruction_plan_summary(plan) {
        parts.push(format!("reconstruction=[{reconstruction}]"));
    }
    if let Some(publication) = daqmx_publication_plan_summary(plan) {
        parts.push(format!("publication=[{publication}]"));
    }
    if let Some(cancel) = daqmx_cancel_plan_summary(plan) {
        parts.push(format!("cancel=[{cancel}]"));
    }
    if let Some(start_order) = list_field(plan, "start_order") {
        parts.push(format!("start=[{}]", start_order.join(">")));
    }
    if let Some(read_order) = list_field(plan, "read_order") {
        parts.push(format!("read=[{}]", read_order.join(",")));
    }
    if let Some(clear_order) = list_field(plan, "clear_order") {
        parts.push(format!("clear=[{}]", clear_order.join(">")));
    }
    if let Some(cleanup) = string_field(plan, "cleanup_policy") {
        parts.push(format!("cleanup={cleanup}"));
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

pub fn daqmx_live_readiness_summary(plan: &BTreeMap<String, Value>) -> Option<String> {
    let Value::Map(readiness) = plan.get("live_task_execution_readiness")? else {
        return None;
    };
    let ready = bool_field(readiness, "live_task_execution_ready").unwrap_or(false);
    let blocker =
        string_field(readiness, "live_task_execution_blocker").unwrap_or_else(|| "unknown".into());
    let hardware =
        string_field(readiness, "hardware_validation_status").unwrap_or_else(|| "unknown".into());
    let missing = list_field(readiness, "missing")
        .filter(|values| !values.is_empty())
        .map(|values| values.join("+"))
        .unwrap_or_else(|| "none".into());
    Some(format!(
        "ready={ready};blocker={blocker};missing={missing};hardware={hardware}"
    ))
}

fn daqmx_plan_validation_summary(plan: &BTreeMap<String, Value>) -> Option<String> {
    let Value::Map(validation) = plan.get("plan_validation")? else {
        return None;
    };
    let status = string_field(validation, "status").unwrap_or_else(|| "unknown".into());
    let runnable = bool_field(validation, "helper_command_runnable").unwrap_or(false);
    if status == "valid" && runnable {
        return None;
    }
    let unrecognized = list_field(validation, "unrecognized_channels")
        .filter(|values| !values.is_empty())
        .map(|values| format!(" unrecognized={}", values.join("+")))
        .unwrap_or_default();
    let invalid_roles = list_field(validation, "invalid_role_channels")
        .filter(|values| !values.is_empty())
        .map(|values| format!(" invalid_roles={}", values.join("+")))
        .unwrap_or_default();
    Some(format!(
        "status={status} runnable={runnable}{unrecognized}{invalid_roles}"
    ))
}

fn daqmx_routing_plan_summary(plan: &BTreeMap<String, Value>) -> Option<String> {
    let Value::Map(routes) = plan.get("routing_plan")? else {
        return None;
    };
    let mut parts = Vec::new();
    if let Some(Value::Map(clock)) = routes.get("sample_clock") {
        let source = string_field(clock, "source").unwrap_or_else(|| "unspecified".into());
        let producer = string_field(clock, "producer_task").unwrap_or_else(|| "none".into());
        let consumers = list_field(clock, "consumers")
            .filter(|values| !values.is_empty())
            .map(|values| values.join("+"))
            .unwrap_or_else(|| "none".into());
        parts.push(format!("clock:{source}:{producer}->{consumers}"));
    }
    if let Some(Value::Map(trigger)) = routes.get("start_trigger") {
        let source = string_field(trigger, "source").unwrap_or_else(|| "none".into());
        let consumers = list_field(trigger, "consumers")
            .filter(|values| !values.is_empty())
            .map(|values| values.join("+"))
            .unwrap_or_else(|| "none".into());
        parts.push(format!("trigger:{source}->{consumers}"));
    }
    (!parts.is_empty()).then(|| parts.join(";"))
}

fn daqmx_runtime_sequence_summary(plan: &BTreeMap<String, Value>) -> Option<String> {
    let Some(Value::List(phases)) = plan.get("runtime_sequence") else {
        return None;
    };
    let mut parts = Vec::new();
    for phase in phases {
        let Value::Map(phase) = phase else {
            continue;
        };
        let name = string_field(phase, "phase")?;
        let tasks = list_field(phase, "tasks")
            .filter(|values| !values.is_empty())
            .map(|values| values.join(">"))
            .unwrap_or_else(|| "none".into());
        parts.push(format!("{name}:{tasks}"));
    }
    (!parts.is_empty()).then(|| parts.join(";"))
}

fn daqmx_completion_plan_summary(plan: &BTreeMap<String, Value>) -> Option<String> {
    let Value::Map(completion) = plan.get("completion_plan")? else {
        return None;
    };
    let mode = string_field(completion, "mode")?;
    let samples = i64_field(completion, "samples_per_channel")?;
    let timeout = time_interval_field(completion, "timeout")?;
    let evidence = string_field(completion, "evidence_status").unwrap_or_else(|| "unknown".into());
    Some(format!(
        "mode={mode};samples={samples};timeout_s={timeout:.3};evidence={evidence}"
    ))
}

fn daqmx_execution_contract_summary(plan: &BTreeMap<String, Value>) -> Option<String> {
    let Value::Map(contract) = plan.get("execution_contract")? else {
        return None;
    };
    let mode = string_field(contract, "mode")?;
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
    let timeout = time_interval_field(contract, "timeout")?;
    let evidence =
        string_field(contract, "contract_evidence_status").unwrap_or_else(|| "unknown".into());
    Some(format!(
        "mode={mode};write={write};read={read};wait={wait};auto_start={auto_start};timeout_s={timeout:.3};evidence={evidence}"
    ))
}

fn daqmx_live_executor_plan_summary(plan: &BTreeMap<String, Value>) -> Option<String> {
    let Value::Map(executor) = plan.get("live_executor_plan")? else {
        return None;
    };
    let mode = string_field(executor, "mode")?;
    let status = string_field(executor, "executor_status")?;
    let backend = string_field(executor, "backend").unwrap_or_else(|| "unknown".into());
    let phases = executor_phase_summary(executor).unwrap_or_else(|| "none".into());
    let evidence =
        string_field(executor, "execution_evidence_status").unwrap_or_else(|| "unknown".into());
    Some(format!(
        "mode={mode};status={status};backend={backend};phases={phases};evidence={evidence}"
    ))
}

fn executor_phase_summary(executor: &BTreeMap<String, Value>) -> Option<String> {
    let Some(Value::List(phases)) = executor.get("phases") else {
        return None;
    };
    let mut names = Vec::new();
    for phase in phases {
        let Value::Map(phase) = phase else {
            continue;
        };
        if let Some(name) = string_field(phase, "phase") {
            names.push(name);
        }
    }
    (!names.is_empty()).then(|| names.join(">"))
}

fn daqmx_reconstruction_plan_summary(plan: &BTreeMap<String, Value>) -> Option<String> {
    let Value::Map(reconstruction) = plan.get("reconstruction_plan")? else {
        return None;
    };
    let mode = string_field(reconstruction, "mode")?;
    let input = list_field(reconstruction, "input_tasks")
        .filter(|values| !values.is_empty())
        .map(|values| values.join("+"))
        .unwrap_or_else(|| "none".into());
    let scan_width = pixel_count_field(reconstruction, "scan_width")?;
    let scan_height = pixel_count_field(reconstruction, "scan_height")?;
    let reconstruction_width = pixel_count_field(reconstruction, "reconstruction_width")?;
    let reconstruction_height = pixel_count_field(reconstruction, "reconstruction_height")?;
    let pixel_format =
        string_field(reconstruction, "pixel_format").unwrap_or_else(|| "unknown".into());
    let evidence = string_field(reconstruction, "reconstruction_evidence_status")
        .unwrap_or_else(|| "unknown".into());
    Some(format!(
        "mode={mode};input={input};scan={}x{};recon={}x{};pixel_format={pixel_format};evidence={evidence}",
        scan_width, scan_height, reconstruction_width, reconstruction_height
    ))
}

fn daqmx_publication_plan_summary(plan: &BTreeMap<String, Value>) -> Option<String> {
    let Value::Map(publication) = plan.get("publication_plan")? else {
        return None;
    };
    let event = string_field(publication, "event_kind")?;
    let mode = string_field(publication, "mode")?;
    let evidence = string_field(publication, "publication_evidence_status")
        .unwrap_or_else(|| "unknown".into());
    if event == "FrameReady" {
        let scan_width = pixel_count_field(publication, "scan_width")?;
        let scan_height = pixel_count_field(publication, "scan_height")?;
        let reconstruction_width = pixel_count_field(publication, "reconstruction_width")?;
        let reconstruction_height = pixel_count_field(publication, "reconstruction_height")?;
        let pixel_format =
            string_field(publication, "pixel_format").unwrap_or_else(|| "unknown".into());
        return Some(format!(
            "{event}:{mode}:scan={}x{}:recon={}x{}:{pixel_format}:{evidence}",
            scan_width, scan_height, reconstruction_width, reconstruction_height
        ));
    }
    if event == "ScanSignalChunk" {
        let channels = list_field(publication, "channel_names")
            .map(|values| values.len())
            .unwrap_or(0);
        let chunk = i64_field(publication, "chunk_size")
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".into());
        return Some(format!(
            "{event}:{mode}:channels={channels}:chunk={chunk}:{evidence}"
        ));
    }
    Some(format!("{event}:{mode}:{evidence}"))
}

fn daqmx_cancel_plan_summary(plan: &BTreeMap<String, Value>) -> Option<String> {
    let Value::Map(cancel) = plan.get("cancel_plan")? else {
        return None;
    };
    let strategy = string_field(cancel, "strategy")?;
    let stop = list_field(cancel, "stop_order")
        .filter(|values| !values.is_empty())
        .map(|values| values.join(">"))
        .unwrap_or_else(|| "none".into());
    let clear = list_field(cancel, "clear_order")
        .filter(|values| !values.is_empty())
        .map(|values| values.join(">"))
        .unwrap_or_else(|| "none".into());
    let evidence =
        string_field(cancel, "cancel_evidence_status").unwrap_or_else(|| "unknown".into());
    Some(format!(
        "strategy={strategy};stop={stop};clear={clear};evidence={evidence}"
    ))
}

fn daqmx_waveform_plan_summary(plan: &BTreeMap<String, Value>) -> Option<String> {
    let Some(Value::List(tasks)) = plan.get("tasks") else {
        return None;
    };
    let mut parts = Vec::new();
    for task in tasks {
        let Value::Map(task) = task else {
            continue;
        };
        let Some(Value::Map(waveform)) = task.get("waveform_plan") else {
            continue;
        };
        let Some(name) = string_field(task, "name") else {
            continue;
        };
        let pattern = string_field(waveform, "pattern").unwrap_or_else(|| "unknown".into());
        let evidence =
            string_field(waveform, "waveform_evidence_status").unwrap_or_else(|| "unknown".into());
        parts.push(format!("{name}:{pattern}:{evidence}"));
    }
    (!parts.is_empty()).then(|| parts.join("|"))
}

fn cleanup_timeout_seconds(plan: &BTreeMap<String, Value>) -> Option<f64> {
    let Value::Map(cleanup) = plan.get("cleanup_plan")? else {
        return None;
    };
    time_interval_field(cleanup, "stop_timeout")
}

fn daqmx_buffer_plan_summary(plan: &BTreeMap<String, Value>) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(Value::Map(scan)) = plan.get("scan_buffer_plan") {
        let width = pixel_count_field(scan, "width")?;
        let height = pixel_count_field(scan, "height")?;
        let frames = i64_field(scan, "frames")?;
        let samples = i64_field(scan, "planned_samples")?;
        parts.push(format!(
            "scan={}x{}x{}:{} samples",
            width, height, frames, samples
        ));
    }
    if let Some(Value::Map(signal)) = plan.get("signal_buffer_plan") {
        let samples_per_line = i64_field(signal, "samples_per_line")?;
        let lines = i64_field(signal, "lines")?;
        let samples = i64_field(signal, "planned_samples")?;
        let chunk = i64_field(signal, "chunk_size")
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".into());
        parts.push(format!(
            "signal={}x{}:{} samples chunk={}",
            samples_per_line, lines, samples, chunk
        ));
    }
    if let Some(tasks) = daqmx_task_buffer_summary(plan) {
        parts.push(format!("tasks={tasks}"));
    }
    (!parts.is_empty()).then(|| parts.join(";"))
}

fn daqmx_task_buffer_summary(plan: &BTreeMap<String, Value>) -> Option<String> {
    let Some(Value::List(tasks)) = plan.get("tasks") else {
        return None;
    };
    let mut parts = Vec::new();
    for task in tasks {
        let Value::Map(task) = task else {
            continue;
        };
        let Some(name) = string_field(task, "name") else {
            continue;
        };
        let Some(Value::Map(buffer)) = task.get("buffer_plan") else {
            continue;
        };
        let direction = string_field(buffer, "direction").unwrap_or_else(|| "unknown".into());
        let element = string_field(buffer, "element_type").unwrap_or_else(|| "unknown".into());
        let channels = i64_field(buffer, "channel_count").unwrap_or(0);
        let samples = i64_field(buffer, "samples_per_channel").unwrap_or(0);
        parts.push(format!(
            "{name}:{direction}:{element}:{channels}chx{samples}"
        ));
    }
    (!parts.is_empty()).then(|| parts.join("|"))
}

fn role_channels_summary(plan: &BTreeMap<String, Value>) -> Option<String> {
    let Value::Map(roles) = plan.get("role_channels")? else {
        return None;
    };
    let mut parts = Vec::new();
    for role in [
        "x_galvo",
        "y_galvo",
        "laser_gate",
        "detector",
        "sample_clock",
    ] {
        let Some(Value::Map(channel)) = roles.get(role) else {
            continue;
        };
        let physical = string_field(channel, "physical")?;
        parts.push(format!("{role}={physical}"));
    }
    (!parts.is_empty()).then(|| parts.join(","))
}

pub fn frame_scan_metadata_summary(frame: &Frame) -> Option<String> {
    let metadata = &frame.metadata;
    let scan_width = pixel_count_field(metadata, "scan_width")?;
    let scan_height = pixel_count_field(metadata, "scan_height")?;
    let reconstruction_width = pixel_count_field(metadata, "reconstruction_width")?;
    let reconstruction_height = pixel_count_field(metadata, "reconstruction_height")?;
    let reconstruction_pixel_size = position_field(metadata, "reconstruction_pixel_size")
        .or_else(|| position_field(metadata, "sample_pixel_size"))?;
    let sample_rate = frequency_field(metadata, "sample_rate")?;
    let line_dwell = time_interval_field(metadata, "line_dwell")?;
    let detectors = list_field(metadata, "detectors")
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| vec!["unknown".into()]);
    let accumulation =
        string_field(metadata, "reconstruction_accumulation").unwrap_or_else(|| "unknown".into());
    let background_subtraction = bool_field(metadata, "background_subtraction")
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".into());
    let laser_gate = bool_field(metadata, "laser_gate_enabled")
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".into());
    let detector_gain = ratio_field(metadata, "detector_gain").unwrap_or(1.0);
    let detector_noise = ratio_field(metadata, "detector_noise").unwrap_or(1.0);
    Some(format!(
        "scan={}x{}, reconstruction={}x{}, reconstruction_pixel_size_um={:.3}, sample_rate_hz={:.0}, line_dwell_s={:.6}, detectors={}, laser_gate_enabled={}, detector_gain={:.3}, detector_noise={:.3}, accumulation={}, background_subtraction={}",
        scan_width,
        scan_height,
        reconstruction_width,
        reconstruction_height,
        reconstruction_pixel_size,
        sample_rate,
        line_dwell,
        detectors.join("+"),
        laser_gate,
        detector_gain,
        detector_noise,
        accumulation,
        background_subtraction
    ))
}

pub fn scene_metadata_summary(metadata: &BTreeMap<String, Value>) -> Option<String> {
    let stage_x = position_field(metadata, "stage_x")?;
    let stage_y = position_field(metadata, "stage_y")?;
    let stage_z = position_field(metadata, "stage_z")?;
    let pixel_size = position_field(metadata, "sample_pixel_size")?;
    let laser_power = ratio_field(metadata, "laser_power")?;
    let laser_gate = bool_field(metadata, "laser_gate_enabled")
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".into());
    let magnification = f64_field(metadata, "magnification")?;
    let numerical_aperture = numerical_aperture_field(metadata, "numerical_aperture")?;
    let detector_gain = ratio_field(metadata, "detector_gain").unwrap_or(1.0);
    let detector_noise = ratio_field(metadata, "detector_noise").unwrap_or(1.0);
    Some(format!(
        "stage_um=({stage_x:.3},{stage_y:.3},{stage_z:.3}), sample_pixel_size_um={pixel_size:.3}, laser_power={laser_power:.3}, laser_gate_enabled={laser_gate}, magnification={magnification:.1}, numerical_aperture={numerical_aperture:.2}, detector_gain={detector_gain:.3}, detector_noise={detector_noise:.3}"
    ))
}

#[derive(Debug, Clone, Copy)]
pub struct ProgressSummary {
    pub updates: u64,
    pub completed: f64,
    pub total: f64,
}

pub fn drain_operation_progress(
    operations: &numanager_core::runtime::Subscription,
    operation: OperationId,
) -> Option<ProgressSummary> {
    let mut summary = None;
    while let Some(event) = operations.recv_timeout(Duration::from_millis(100)) {
        if let Event::OperationChanged(event) = event {
            if event.operation != operation {
                continue;
            }
            if let OperationStatus::Running {
                progress: Some(progress),
            } = event.status
            {
                let updates = summary
                    .map(|summary: ProgressSummary| summary.updates + 1)
                    .unwrap_or(1);
                summary = Some(ProgressSummary {
                    updates,
                    completed: progress.completed,
                    total: progress.total,
                });
            }
        }
    }
    summary
}

fn raster_scan(width: i64, height: i64, frames: i64) -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("mode".into(), Value::String("raster".into())),
        ("fast_axis".into(), Value::String("x".into())),
        ("slow_axis".into(), Value::String("y".into())),
        (
            "width".into(),
            Value::PixelCount(PixelCount::new(pixel_count(width))),
        ),
        (
            "height".into(),
            Value::PixelCount(PixelCount::new(pixel_count(height))),
        ),
        ("frames".into(), Value::I64(frames)),
        (
            "sample_rate".into(),
            Value::Frequency(Frequency::from_hertz(100_000.0)),
        ),
        ("line_dwell_us".into(), Value::F64(500.0)),
        (
            "x_galvo".into(),
            Value::String(env_or("NUMANAGER_DAQMX_LSM_X_GALVO", "ao0")),
        ),
        (
            "y_galvo".into(),
            Value::String(env_or("NUMANAGER_DAQMX_LSM_Y_GALVO", "ao1")),
        ),
        (
            "laser_gate".into(),
            Value::String(env_or("NUMANAGER_DAQMX_LSM_LASER_GATE", "do0")),
        ),
        (
            "detector".into(),
            Value::String(env_or("NUMANAGER_DAQMX_LSM_DETECTOR", "counter0")),
        ),
    ])
}

fn signal_channels() -> Vec<String> {
    if let Ok(channels) = std::env::var("NUMANAGER_DAQMX_SIGNAL_CHANNELS") {
        let values = channels
            .split(',')
            .map(str::trim)
            .filter(|channel| !channel.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if !values.is_empty() {
            return values;
        }
    }
    vec![
        env_or("NUMANAGER_DAQMX_LSM_DETECTOR", "counter0"),
        env_or("NUMANAGER_DAQMX_SIGNAL_AI", "ai0"),
    ]
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

fn raster_reconstruction(width: i64, height: i64) -> BTreeMap<String, Value> {
    BTreeMap::from([
        (
            "image_width".into(),
            Value::PixelCount(PixelCount::new(pixel_count(width))),
        ),
        (
            "image_height".into(),
            Value::PixelCount(PixelCount::new(pixel_count(height))),
        ),
        ("pixel_format".into(), Value::String("Mono16".into())),
        ("accumulation".into(), Value::String("sum".into())),
        ("background_subtraction".into(), Value::Bool(false)),
    ])
}

fn string_field(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    match map.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn list_field(map: &BTreeMap<String, Value>, key: &str) -> Option<Vec<String>> {
    match map.get(key) {
        Some(Value::List(values)) => Some(
            values
                .iter()
                .filter_map(|value| match value {
                    Value::String(value) => Some(value.clone()),
                    _ => None,
                })
                .collect(),
        ),
        _ => None,
    }
}

fn pixel_count_field(map: &BTreeMap<String, Value>, key: &str) -> Option<u32> {
    match map.get(key) {
        Some(Value::PixelCount(value)) => Some(value.pixels()),
        Some(Value::I64(value)) => Some((*value).clamp(0, u32::MAX as i64) as u32),
        _ => None,
    }
}

fn i64_field(map: &BTreeMap<String, Value>, key: &str) -> Option<i64> {
    match map.get(key) {
        Some(Value::I64(value)) => Some(*value),
        _ => None,
    }
}

fn f64_field(map: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
    match map.get(key) {
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn position_field(map: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
    match map.get(key) {
        Some(Value::Position(value)) => Some(value.micrometers()),
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn frequency_field(map: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
    match map.get(key) {
        Some(Value::Frequency(value)) => Some(value.hertz()),
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn ratio_field(map: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
    match map.get(key) {
        Some(Value::Ratio(value)) => Some(value.fraction()),
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn numerical_aperture_field(map: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
    match map.get(key) {
        Some(Value::NumericalAperture(value)) => Some(value.value()),
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn time_interval_field(map: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
    match map.get(key) {
        Some(Value::TimeInterval(value)) => Some(value.seconds()),
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn bool_field(map: &BTreeMap<String, Value>, key: &str) -> Option<bool> {
    match map.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn value_brief(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::I64(value) => value.to_string(),
        Value::F64(value) => format!("{value:.3}"),
        Value::Bool(value) => value.to_string(),
        Value::Null => "null".into(),
        Value::Map(map) => format!("map({} keys)", map.len()),
        Value::List(values) => format!("list({})", values.len()),
        other => format!("{other:?}"),
    }
}

fn pixel_count(value: i64) -> u32 {
    value.clamp(1, u32::MAX as i64) as u32
}
