use std::collections::BTreeMap;

use numanager_core::Value;

pub fn device_name() -> String {
    std::env::var("NUMANAGER_DAQMX_DEVICE_NAME")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Dev1".into())
}

pub fn inventory_helper_command(version_only: bool) -> String {
    let version_only = if version_only { " --version-only" } else { "" };
    format!(
        "target/debug/numanager-daqmx-inventory-helper --device {}{version_only}",
        device_name()
    )
}

pub fn runtime_probe_commands() -> Vec<String> {
    let probe_env_prefix = daqmx_probe_env_prefix();
    vec![
        format!("{probe_env_prefix}NUMANAGER_DAQMX_CONFIG_ONLY=1 cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe"),
        format!("{probe_env_prefix}cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe"),
    ]
}

pub fn inventory_commands() -> Vec<String> {
    let probe_env_prefix = daqmx_probe_env_prefix();
    vec![
        inventory_helper_command(true),
        format!("{probe_env_prefix}NUMANAGER_DAQMX_RUNTIME_HELPER=target/debug/numanager-daqmx-inventory-helper cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe"),
        inventory_helper_command(false),
        format!("{probe_env_prefix}NUMANAGER_DAQMX_INVENTORY=1 NUMANAGER_DAQMX_INVENTORY_HELPER=target/debug/numanager-daqmx-inventory-helper cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe"),
    ]
}

pub fn daqmx_probe_env_prefix() -> String {
    env_prefix(&[
        "NUMANAGER_DAQMX_DEVICE_NAME",
        "NIDAQMX_RUNTIME_PACKAGE",
        "NUMANAGER_DAQMX_RUNTIME_VERSION",
        "NIDAQMX_RUNTIME_VERSION",
        "NIDAQMX_RUNTIME_PLATFORM",
        "NIDAQMX_RUNTIME_LICENSE",
        "NIDAQMX_HEADER_PATH",
        "NIDAQMX_HEADER_SHA256",
        "NUMANAGER_DAQMX_HELPER_TIMEOUT_SECONDS",
        "NUMANAGER_DAQMX_LIVE_TASK_EXECUTION",
    ])
}

pub fn daqmx_lsm_env_prefix() -> String {
    env_prefix(&[
        "NUMANAGER_DAQMX_DEVICE_NAME",
        "NUMANAGER_DAQMX_LSM_X_GALVO",
        "NUMANAGER_DAQMX_LSM_Y_GALVO",
        "NUMANAGER_DAQMX_LSM_LASER_GATE",
        "NUMANAGER_DAQMX_LSM_DETECTOR",
        "NUMANAGER_DAQMX_LSM_SAMPLE_CLOCK",
        "NUMANAGER_DAQMX_LSM_SAMPLE_CLOCK_SOURCE",
        "NUMANAGER_DAQMX_LSM_START_TRIGGER_SOURCE",
        "NUMANAGER_DAQMX_SIGNAL_AI",
        "NUMANAGER_DAQMX_SIGNAL_CHANNELS",
        "NUMANAGER_DAQMX_TIMEOUT_SECONDS",
        "NUMANAGER_DAQMX_HELPER_TIMEOUT_SECONDS",
        "NUMANAGER_DAQMX_LIVE_TASK_EXECUTION",
    ])
}

pub fn invalid_numeric_guard_commands(values: &[&Value]) -> Vec<String> {
    let mut commands = vec![
        "target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --wait-seconds NaN".into(),
        "target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --name ''".into(),
        "target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --name ' lifecycle '".into(),
        "target/debug/numanager-daqmx-task-lifecycle-helper --simulate-error-after-start".into(),
        "target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --simulate-error-after-start"
            .into(),
        "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --preflight-only"
            .into(),
    ];
    if let Some(channel) = first_channel_by_setup_kind(values, "ci") {
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate NaN --samples 1 --ci {channel} --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 0 --samples 1 --ci {channel} --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci {channel} --timeout NaN --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci {channel} --timeout 0 --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci {channel} --sample-clock-source '' --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci {channel} --sample-clock-source ' /{device}/Ctr0InternalOutput ' --preflight-only",
            device = device_name()
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci {channel} --start-trigger '' --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci {channel} --start-trigger ' /{device}/PFI0 ' --preflight-only",
            device = device_name()
        ));
        commands.push(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci '' --preflight-only".into()
        );
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci ' {channel} ' --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci {channel} --ci-task '' --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci {channel} --ci-task ' signal ' --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --signal-lines 0 --ci {channel} --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 5 --signal-lines 2 --ci {channel} --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --chunk-size 1 --ci {channel} --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --signal-lines 1 --chunk-size 2 --ci {channel} --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci {channel} --co {channel} --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci {channel} --ai {ai_channel} --ci-task signal --ai-task signal --preflight-only",
            ai_channel = first_channel_by_setup_kind(values, "ai").unwrap_or_else(|| "Dev1/ai0".into())
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 0 --ci {channel} --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 2147483648 --ci {channel} --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 3 --width 2 --height 2 --frames 1 --ci {channel} --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 1 --ci {channel} --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 18446744073709551615 --height 2 --frames 1 --ci {channel} --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 2 --height 2 --frames 4611686018427387904 --ci {channel} --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 0 --height 1 --frames 1 --ci {channel} --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 1 --height 0 --frames 1 --ci {channel} --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 1 --height 1 --frames 0 --ci {channel} --preflight-only"
        ));
    }
    if let Some(channels) =
        channels_by_setup_kind(values, "ao").filter(|channels| channels.len() >= 2)
    {
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1073741824 --ao {} --ao {} --preflight-only",
            channels[0], channels[1]
        ));
    }
    if let Some(channel) = first_channel_by_setup_kind(values, "ao") {
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ao {channel} --min-volts NaN --max-volts 1 --preflight-only"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ao {channel} --min-volts 1 --max-volts -1 --preflight-only"
        ));
    }
    if let Some(channel) = first_channel_by_setup_kind(values, "co") {
        commands.push(format!(
            "target/debug/numanager-daqmx-channel-setup-helper --kind co --channel {channel} --name '' --dry-run"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-channel-setup-helper --kind co --channel {channel} --name ' channel-setup ' --dry-run"
        ));
        commands.push(
            "target/debug/numanager-daqmx-channel-setup-helper --kind co --channel '' --dry-run"
                .into(),
        );
        commands.push(format!(
            "target/debug/numanager-daqmx-channel-setup-helper --kind co --channel ' {channel} ' --dry-run"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-channel-setup-helper --kind co --channel {channel} --frequency inf --dry-run"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-channel-setup-helper --kind co --channel {channel} --frequency 0 --dry-run"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-channel-setup-helper --kind co --channel {channel} --duty-cycle NaN --dry-run"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-channel-setup-helper --kind co --channel {channel} --duty-cycle 1.5 --dry-run"
        ));
        commands.push(
            "target/debug/numanager-daqmx-io-smoke-helper --kind co --channel '' --samples 1"
                .into(),
        );
        commands.push(format!(
            "target/debug/numanager-daqmx-io-smoke-helper --kind co --channel {channel} --name '' --samples 1"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-io-smoke-helper --kind co --channel {channel} --name ' io-smoke ' --samples 1"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-io-smoke-helper --kind co --channel {channel} --frequency inf --samples 1"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-io-smoke-helper --kind co --channel {channel} --frequency 0 --samples 1"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-io-smoke-helper --kind co --channel {channel} --duty-cycle NaN --samples 1"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-io-smoke-helper --kind co --channel {channel} --duty-cycle 1.5 --samples 1"
        ));
    }
    if let Some(channel) = first_channel_by_setup_kind(values, "ai") {
        commands.push(format!(
            "target/debug/numanager-daqmx-channel-setup-helper --kind ai --channel {channel} --min-volts NaN --max-volts 1 --dry-run"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-channel-setup-helper --kind ai --channel {channel} --min-volts 1 --max-volts -1 --dry-run"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel {channel} --samples 1 --timeout NaN"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel {channel} --samples 1 --timeout 0"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel {channel} --samples 0"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel {channel} --samples 2147483648"
        ));
    }
    if let Some(channel) = first_channel_by_setup_kind(values, "ao") {
        commands.push(format!(
            "target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel {channel} --min-volts NaN --max-volts 1 --volts 0"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel {channel} --min-volts 1 --max-volts -1 --volts 0"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel {channel} --volts NaN"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel {channel} --min-volts -1 --max-volts 1 --volts 2"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel {channel} --min-volts 1 --max-volts 5 --volts 2"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel {channel} --simulate-error-after-start"
        ));
        commands.push(format!(
            "target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel {channel} --volts 0 --execute"
        ));
    }
    commands
}

pub fn task_lifecycle_dry_run_commands() -> Vec<String> {
    vec![
        "target/debug/numanager-daqmx-task-lifecycle-helper --dry-run".into(),
        "target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --start --wait-seconds 0.250000".into(),
    ]
}

pub fn task_lifecycle_cleanup_simulation_commands() -> Vec<String> {
    vec![
        "target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --start --wait-seconds 0.250000 --simulate-error-after-start".into(),
    ]
}

pub fn plan_setup_cleanup_simulation_command(value: &Value) -> Option<String> {
    let plan = daqmx_plan(value)?;
    let command = string_field(plan, "plan_preflight_helper_command")?;
    Some(format!("{command} --simulate-setup-error-after 1"))
}

pub fn task_lifecycle_setup_commands() -> Vec<String> {
    vec!["target/debug/numanager-daqmx-task-lifecycle-helper".into()]
}

pub fn channel_setup_commands(value: &Value, dry_run: bool) -> Vec<String> {
    daqmx_tasks(value)
        .into_iter()
        .flat_map(|task| task_setup_commands(task, dry_run))
        .collect()
}

pub fn io_smoke_commands(value: &Value, execute: bool) -> Vec<String> {
    daqmx_tasks(value)
        .into_iter()
        .flat_map(|task| task_io_smoke_commands(task, execute))
        .collect()
}

pub fn io_smoke_cleanup_simulation_commands(value: &Value) -> Vec<String> {
    daqmx_tasks(value)
        .into_iter()
        .flat_map(task_io_smoke_cleanup_simulation_commands)
        .collect()
}

fn env_prefix(keys: &[&str]) -> String {
    let assignments = keys
        .iter()
        .filter_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .map(|value| format!("{key}={}", shell_quote_env_value(&value)))
        })
        .collect::<Vec<_>>();
    if assignments.is_empty() {
        String::new()
    } else {
        format!("{} ", assignments.join(" "))
    }
}

fn shell_quote_env_value(value: &str) -> String {
    if value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/' | b':')
    }) {
        value.into()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn first_channel_by_setup_kind(values: &[&Value], kind: &str) -> Option<String> {
    channels_by_setup_kind(values, kind).and_then(|channels| channels.into_iter().next())
}

fn channels_by_setup_kind(values: &[&Value], kind: &str) -> Option<Vec<String>> {
    values.iter().find_map(|value| {
        daqmx_tasks(value).into_iter().find_map(|task| {
            (setup_kind(task) == Some(kind))
                .then(|| physical_channels(task))
                .flatten()
        })
    })
}

fn task_setup_commands(task: &BTreeMap<String, Value>, dry_run: bool) -> Vec<String> {
    let Some(kind) = setup_kind(task) else {
        return Vec::new();
    };
    let Some(Value::List(channels)) = task.get("physical_channels") else {
        return Vec::new();
    };
    channels
        .iter()
        .filter_map(|channel| match channel {
            Value::String(channel) if dry_run => Some(format!(
                "target/debug/numanager-daqmx-channel-setup-helper --kind {kind} --channel {channel} --dry-run"
            )),
            Value::String(channel) => Some(format!(
                "target/debug/numanager-daqmx-channel-setup-helper --kind {kind} --channel {channel}"
            )),
            _ => None,
        })
        .collect()
}

fn task_io_smoke_commands(task: &BTreeMap<String, Value>, execute: bool) -> Vec<String> {
    let Some(kind) = setup_kind(task) else {
        return Vec::new();
    };
    let Some(Value::List(channels)) = task.get("physical_channels") else {
        return Vec::new();
    };
    channels
        .iter()
        .filter_map(|channel| match channel {
            Value::String(channel) => Some(io_smoke_command(kind, channel, execute)),
            _ => None,
        })
        .collect()
}

fn task_io_smoke_cleanup_simulation_commands(task: &BTreeMap<String, Value>) -> Vec<String> {
    let Some(kind) = setup_kind(task) else {
        return Vec::new();
    };
    let Some(Value::List(channels)) = task.get("physical_channels") else {
        return Vec::new();
    };
    channels
        .iter()
        .filter_map(|channel| match channel {
            Value::String(channel) => io_smoke_cleanup_simulation_command(kind, channel),
            _ => None,
        })
        .collect()
}

fn io_smoke_command(kind: &str, channel: &str, execute: bool) -> String {
    let extra = match kind {
        "ao" => "--volts 0",
        "do" => "--line-state false",
        "ai" | "ci" => "--samples 1",
        "co" => "--frequency 10 --samples 1",
        _ => "",
    };
    let execute = if execute {
        " --execute --bench-safety-reviewed"
    } else {
        ""
    };
    format!(
        "target/debug/numanager-daqmx-io-smoke-helper --kind {kind} --channel {channel} {extra}{execute}"
    )
}

fn io_smoke_cleanup_simulation_command(kind: &str, channel: &str) -> Option<String> {
    let extra = match kind {
        "ai" | "ci" => "--samples 1",
        "co" => "--frequency 10 --samples 1",
        _ => return None,
    };
    Some(format!(
        "target/debug/numanager-daqmx-io-smoke-helper --kind {kind} --channel {channel} {extra} --simulate-error-after-start"
    ))
}

fn physical_channels(task: &BTreeMap<String, Value>) -> Option<Vec<String>> {
    let Some(Value::List(channels)) = task.get("physical_channels") else {
        return None;
    };
    Some(
        channels
            .iter()
            .filter_map(|channel| match channel {
                Value::String(channel) => Some(channel.clone()),
                _ => None,
            })
            .collect(),
    )
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

fn daqmx_plan(value: &Value) -> Option<&BTreeMap<String, Value>> {
    let Value::Map(result) = value else {
        return None;
    };
    let Some(Value::Map(plan)) = result.get("daqmx_task_plan") else {
        return None;
    };
    Some(plan)
}

fn string_field(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    match map.get(key)? {
        Value::String(value) => Some(value.clone()),
        _ => None,
    }
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
