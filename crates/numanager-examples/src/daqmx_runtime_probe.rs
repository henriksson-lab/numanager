use std::collections::BTreeMap;
use std::time::Duration;

use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverDiscovery, LocalRuntime, Runtime};
use numanager_core::{Command, DriverId, TimeInterval, Value};
use numanager_imswitch_daqmx::ImSwitchDaqmxDiscovery;

const DEFAULT_NIDAQMX_HEADER_SHA256: &str =
    "86491926d3485439ba49efa1ac610ac1d2541dcff703b51c7f9be27c4b646164";

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let device_name = env_or("NUMANAGER_DAQMX_DEVICE_NAME", "Dev1");
    let runtime_package = env_or("NIDAQMX_RUNTIME_PACKAGE", "NI-DAQmx");
    let runtime_version = std::env::var("NUMANAGER_DAQMX_RUNTIME_VERSION")
        .ok()
        .or_else(|| std::env::var("NIDAQMX_RUNTIME_VERSION").ok());
    let runtime_platform = env_or(
        "NIDAQMX_RUNTIME_PLATFORM",
        &format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
    );
    let runtime_license = env_or(
        "NIDAQMX_RUNTIME_LICENSE",
        "user-provided third-party excluded data; redistribution terms unresolved",
    );
    let sdk_header_path = env_or("NIDAQMX_HEADER_PATH", "/usr/include/NIDAQmx.h");
    let sdk_header_sha256 = env_or("NIDAQMX_HEADER_SHA256", DEFAULT_NIDAQMX_HEADER_SHA256);
    let probe_config = ProbeConfig {
        device_name,
        runtime_package,
        runtime_version,
        runtime_platform,
        sdk_header_path,
        helper_timeout: env_positive_seconds("NUMANAGER_DAQMX_HELPER_TIMEOUT_SECONDS")?,
        live_task_execution: env_bool("NUMANAGER_DAQMX_LIVE_TASK_EXECUTION"),
    };

    let config_only = std::env::var_os("NUMANAGER_DAQMX_CONFIG_ONLY").is_some();

    let mut properties = BTreeMap::from([
        ("connect".into(), Value::Bool(!config_only)),
        (
            "inventory_devices".into(),
            Value::Bool(!config_only && std::env::var_os("NUMANAGER_DAQMX_INVENTORY").is_some()),
        ),
        (
            "live_task_execution".into(),
            Value::Bool(probe_config.live_task_execution),
        ),
        (
            "device_name".into(),
            Value::String(probe_config.device_name.clone()),
        ),
        (
            "runtime_package".into(),
            Value::String(probe_config.runtime_package.clone()),
        ),
        (
            "runtime_platform".into(),
            Value::String(probe_config.runtime_platform.clone()),
        ),
        (
            "runtime_license".into(),
            Value::String(runtime_license.clone()),
        ),
        (
            "sdk_header_path".into(),
            Value::String(probe_config.sdk_header_path.clone()),
        ),
        ("sdk_header_sha256".into(), Value::String(sdk_header_sha256)),
    ]);
    if let Some(version) = probe_config.runtime_version.clone() {
        properties.insert("runtime_version".into(), Value::String(version));
    }
    if let Ok(helper_path) = std::env::var("NUMANAGER_DAQMX_RUNTIME_HELPER")
        .or_else(|_| std::env::var("NUMANAGER_DAQMX_INVENTORY_HELPER"))
    {
        properties.insert("inventory_helper_path".into(), Value::String(helper_path));
    }
    if let Some(timeout) = probe_config.helper_timeout {
        properties.insert(
            "inventory_helper_timeout".into(),
            Value::TimeInterval(timeout),
        );
    }

    let (connected, backend_status) = configured_probe_status(properties)?;

    if config_only {
        println!("{}", probe_config.summary());
        println!("config_only: true");
        println!("connected: {connected:?}");
        println!("backend_status: {backend_status:?}");
        if let Some(summary) = configured_runtime_version_summary(&backend_status) {
            println!("configured_runtime_version: {summary}");
        }
        if let Some(summary) = runtime_version_summary(&backend_status) {
            println!("runtime_version: {summary}");
        }
        if let Some(summary) = runtime_version_comparison_summary(&backend_status) {
            println!("runtime_version_comparison: {summary}");
        }
        if let Some(summary) = readiness_summary(&backend_status) {
            println!("readiness: {summary}");
        }
        if let Some(summary) = bringup_helpers_summary(&backend_status) {
            println!("bringup_helpers: {summary}");
        }
        if let Some(summary) = inventory_summary(&backend_status) {
            println!("inventory: {summary}");
        }
        if let Some(summary) = missing_summary(&backend_status) {
            println!("missing: {summary}");
        }
        if let Some(summary) = promotion_gates_summary(&backend_status) {
            println!("promotion_gates: {summary}");
        }
        if let Some(summary) = promotion_gate_statuses_summary(&backend_status) {
            println!("promotion_gate_statuses: {summary}");
        }
        return Ok(());
    }

    println!("{}", probe_config.summary());
    println!("connected: {connected:?}");
    println!("backend_status: {backend_status:?}");
    if let Some(summary) = configured_runtime_version_summary(&backend_status) {
        println!("configured_runtime_version: {summary}");
    }
    if let Some(summary) = runtime_version_summary(&backend_status) {
        println!("runtime_version: {summary}");
    }
    if let Some(summary) = runtime_version_comparison_summary(&backend_status) {
        println!("runtime_version_comparison: {summary}");
    }
    if let Some(summary) = readiness_summary(&backend_status) {
        println!("readiness: {summary}");
    }
    if let Some(summary) = bringup_helpers_summary(&backend_status) {
        println!("bringup_helpers: {summary}");
    }
    if let Some(summary) = inventory_summary(&backend_status) {
        println!("inventory: {summary}");
    }
    if let Some(summary) = missing_summary(&backend_status) {
        println!("missing: {summary}");
    }
    if let Some(summary) = promotion_gates_summary(&backend_status) {
        println!("promotion_gates: {summary}");
    }
    if let Some(summary) = promotion_gate_statuses_summary(&backend_status) {
        println!("promotion_gate_statuses: {summary}");
    }
    Ok(())
}

fn configured_probe_status(
    properties: BTreeMap<String, Value>,
) -> Result<(Value, Value), Box<dyn std::error::Error>> {
    let hardware = HardwareConfig {
        devices: vec![DeviceConfig::new(
            1,
            "Local NI-DAQmx runtime",
            "imswitch_daqmx",
            properties,
        )],
        ..HardwareConfig::default()
    };

    let mut discovery = ImSwitchDaqmxDiscovery::configured(DriverId(1), &hardware)?;
    let candidate = discovery
        .detect()?
        .into_iter()
        .next()
        .ok_or("NI-DAQmx configured discovery returned no candidates")?;

    let mut runtime = LocalRuntime::new();
    runtime.add_driver(candidate.into_driver())?;
    let hub = runtime.device_by_kind("imswitch.daqmx")?.id;

    let connected = runtime.execute(
        Command::read_property(hub, "connected"),
        Duration::from_secs(2),
    )?;
    let backend_status = runtime.execute(
        Command::read_property(hub, "backend_status"),
        Duration::from_secs(2),
    )?;
    Ok((connected, backend_status))
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

fn env_bool(key: &str) -> bool {
    std::env::var(key)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn env_positive_seconds(key: &str) -> Result<Option<TimeInterval>, Box<dyn std::error::Error>> {
    let Ok(value) = std::env::var(key) else {
        return Ok(None);
    };
    let seconds = value.parse::<f64>()?;
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err(format!("{key} must be positive and finite").into());
    }
    Ok(Some(TimeInterval::from_seconds(seconds)))
}

struct ProbeConfig {
    device_name: String,
    runtime_package: String,
    runtime_version: Option<String>,
    runtime_platform: String,
    sdk_header_path: String,
    helper_timeout: Option<TimeInterval>,
    live_task_execution: bool,
}

impl ProbeConfig {
    fn summary(&self) -> String {
        format!(
            "probe_config: device_name={}, runtime_package={}, runtime_version={}, runtime_platform={}, sdk_header_path={}, helper_timeout={}, live_task_execution={}",
            self.device_name,
            self.runtime_package,
            self.runtime_version.as_deref().unwrap_or("<runtime_probe>"),
            self.runtime_platform,
            self.sdk_header_path,
            self.helper_timeout
                .map(|value| format!("{:.3}s", value.seconds()))
                .unwrap_or_else(|| "<driver_default>".into()),
            self.live_task_execution
        )
    }
}

fn readiness_summary(value: &Value) -> Option<String> {
    let Value::Map(map) = value else {
        return None;
    };
    let feature_requested = bool_field(map, "feature_requested")?;
    let target_supported = bool_field(map, "target_supported")?;
    let feature_enabled = bool_field(map, "feature_enabled")?;
    let metadata_configured = bool_field(map, "metadata_configured")?;
    let live_requested = bool_field(map, "live_task_execution_requested")?;
    let live_ready = bool_field(map, "live_task_execution_ready")?;
    let blocker = string_field(map, "live_task_execution_blocker")?;
    Some(format!(
        "feature_requested={feature_requested}, target_supported={target_supported}, feature_enabled={feature_enabled}, metadata_configured={metadata_configured}, live_task_execution_requested={live_requested}, live_task_execution_ready={live_ready}, blocker={blocker}"
    ))
}

fn runtime_version_summary(value: &Value) -> Option<String> {
    let Value::Map(map) = value else {
        return None;
    };
    let version = string_field(map, "detected_runtime_version")?;
    let major = i64_field(map, "detected_runtime_version_major");
    let minor = i64_field(map, "detected_runtime_version_minor");
    let update = i64_field(map, "detected_runtime_version_update");
    Some(match (major, minor, update) {
        (Some(major), Some(minor), Some(update)) => {
            format!("{version} (major={major}, minor={minor}, update={update})")
        }
        _ => version.to_owned(),
    })
}

fn configured_runtime_version_summary(value: &Value) -> Option<String> {
    let Value::Map(map) = value else {
        return None;
    };
    let version = string_field(map, "configured_runtime_version")?;
    let major = i64_field(map, "configured_runtime_version_major");
    let minor = i64_field(map, "configured_runtime_version_minor");
    let update = i64_field(map, "configured_runtime_version_update");
    Some(match (major, minor, update) {
        (Some(major), Some(minor), Some(update)) => {
            format!("{version} (major={major}, minor={minor}, update={update})")
        }
        (Some(major), Some(minor), None) => format!("{version} (major={major}, minor={minor})"),
        _ => version.to_owned(),
    })
}

fn runtime_version_comparison_summary(value: &Value) -> Option<String> {
    let Value::Map(map) = value else {
        return None;
    };
    let status = string_field(map, "runtime_version_comparison")?;
    let basis = string_field(map, "runtime_version_comparison_basis")?;
    let matches = optional_bool_field(map, "runtime_version_matches")
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".into());
    Some(format!("{status} (matches={matches}, basis={basis})"))
}

fn bringup_helpers_summary(value: &Value) -> Option<String> {
    let Value::Map(map) = value else {
        return None;
    };
    let Some(Value::Map(helpers)) = map.get("bringup_helpers_compiled") else {
        return None;
    };
    let mut parts = Vec::new();
    for key in [
        "inventory",
        "task_lifecycle",
        "channel_setup",
        "plan_setup",
        "io_smoke",
    ] {
        let value = bool_field(helpers, key)?;
        parts.push(format!("{key}={value}"));
    }
    Some(parts.join(", "))
}

fn inventory_summary(value: &Value) -> Option<String> {
    let Value::Map(map) = value else {
        return None;
    };
    let requested = bool_field(map, "device_inventory_requested")?;
    let helper = bool_field(map, "inventory_helper_configured")?;
    let detected_devices = list_len_field(map, "detected_devices")?;
    let configured_device_detected = bool_field(map, "configured_device_detected")?;
    let configured_device = configured_device_summary(map).unwrap_or_else(|| "none".into());
    let error = string_field(map, "device_inventory_error")
        .or_else(|| string_field(map, "configured_device_error"))
        .map(compact_error)
        .unwrap_or_else(|| "none".into());
    Some(format!(
        "requested={requested}, helper={helper}, detected_devices={detected_devices}, configured_device_detected={configured_device_detected}, configured_device={configured_device}, error={error}"
    ))
}

fn configured_device_summary(map: &BTreeMap<String, Value>) -> Option<String> {
    let Value::Map(device) = map.get("configured_device_identity")? else {
        return None;
    };
    let name = string_field(device, "name")?;
    let product = string_field(device, "product_type").unwrap_or("unknown_product");
    let serial = match device.get("serial_number") {
        Some(Value::I64(value)) => value.to_string(),
        Some(Value::String(value)) => value.clone(),
        _ => "unknown_serial".into(),
    };
    Some(format!("{name}/{product}/serial={serial}"))
}

fn compact_error(error: &str) -> String {
    error
        .split_whitespace()
        .take(16)
        .collect::<Vec<_>>()
        .join(" ")
}

fn missing_summary(value: &Value) -> Option<String> {
    let Value::Map(map) = value else {
        return None;
    };
    list_summary(map, "missing")
}

fn promotion_gates_summary(value: &Value) -> Option<String> {
    let Value::Map(map) = value else {
        return None;
    };
    list_summary(map, "external_promotion_gates")
}

fn promotion_gate_statuses_summary(value: &Value) -> Option<String> {
    let Value::Map(map) = value else {
        return None;
    };
    let Value::Map(statuses) = map.get("external_promotion_gate_statuses")? else {
        return None;
    };
    let mut counts = BTreeMap::<&str, usize>::new();
    for status in statuses.values() {
        let Value::Map(status) = status else {
            continue;
        };
        if let Some(status) = string_field(status, "status") {
            *counts.entry(status).or_default() += 1;
        }
    }
    if counts.is_empty() {
        return Some("none".into());
    }
    Some(
        counts
            .into_iter()
            .map(|(status, count)| format!("{status}={count}"))
            .collect::<Vec<_>>()
            .join(", "),
    )
}

fn list_summary(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    let Value::List(missing) = map.get(key)? else {
        return None;
    };
    let fields = missing
        .iter()
        .filter_map(|value| match value {
            Value::String(value) => Some(value.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    Some(if fields.is_empty() {
        "none".into()
    } else {
        fields.join(", ")
    })
}

fn list_len_field(map: &BTreeMap<String, Value>, key: &str) -> Option<usize> {
    match map.get(key)? {
        Value::List(values) => Some(values.len()),
        _ => None,
    }
}

fn bool_field(map: &BTreeMap<String, Value>, key: &str) -> Option<bool> {
    match map.get(key)? {
        Value::Bool(value) => Some(*value),
        _ => None,
    }
}

fn optional_bool_field(map: &BTreeMap<String, Value>, key: &str) -> Option<bool> {
    match map.get(key)? {
        Value::Bool(value) => Some(*value),
        Value::Null => None,
        _ => None,
    }
}

fn i64_field(map: &BTreeMap<String, Value>, key: &str) -> Option<i64> {
    match map.get(key)? {
        Value::I64(value) => Some(*value),
        _ => None,
    }
}

fn string_field<'a>(map: &'a BTreeMap<String, Value>, key: &str) -> Option<&'a str> {
    match map.get(key)? {
        Value::String(value) => Some(value.as_str()),
        _ => None,
    }
}
