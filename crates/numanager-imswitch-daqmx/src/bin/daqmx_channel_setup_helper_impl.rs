use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::process::ExitCode;
use std::ptr;

pub(super) fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut task_name = Some("numanager-daqmx-channel-setup-probe".to_owned());
    let mut kind = None;
    let mut channel = None;
    let mut min_volts = -10.0;
    let mut max_volts = 10.0;
    let mut terminal = TerminalConfig::Default;
    let mut edge = Edge::Rising;
    let mut direction = CountDirection::Up;
    let mut idle_state = IdleState::Low;
    let mut frequency_hz = 1_000.0;
    let mut duty_cycle = 0.5;
    let mut dry_run = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--name" => {
                task_name = Some(
                    args.next()
                        .ok_or_else(|| "--name requires a value".to_owned())?,
                );
            }
            "--unnamed" => task_name = None,
            "--kind" => kind = Some(parse_kind(&required_arg(&mut args, "--kind")?)?),
            "--channel" => channel = Some(required_arg(&mut args, "--channel")?),
            "--min-volts" => min_volts = parse_f64(&required_arg(&mut args, "--min-volts")?)?,
            "--max-volts" => max_volts = parse_f64(&required_arg(&mut args, "--max-volts")?)?,
            "--terminal" => terminal = parse_terminal(&required_arg(&mut args, "--terminal")?)?,
            "--edge" => edge = parse_edge(&required_arg(&mut args, "--edge")?)?,
            "--direction" => direction = parse_direction(&required_arg(&mut args, "--direction")?)?,
            "--idle-state" => {
                idle_state = parse_idle_state(&required_arg(&mut args, "--idle-state")?)?
            }
            "--frequency" => frequency_hz = parse_f64(&required_arg(&mut args, "--frequency")?)?,
            "--duty-cycle" => duty_cycle = parse_f64(&required_arg(&mut args, "--duty-cycle")?)?,
            "--dry-run" => dry_run = true,
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }

    let Some(kind) = kind else {
        print_usage();
        return Err("--kind is required".into());
    };
    let Some(channel) = channel else {
        print_usage();
        return Err("--channel is required".into());
    };
    validate_optional_task_name(task_name.as_deref())?;
    validate_channel(&channel)?;
    if !min_volts.is_finite() || !max_volts.is_finite() {
        return Err("--min-volts and --max-volts must be finite".into());
    }
    if min_volts > max_volts {
        return Err("--min-volts must not exceed --max-volts".into());
    }
    if !duty_cycle.is_finite() || !(0.0..=1.0).contains(&duty_cycle) {
        return Err("--duty-cycle must be finite and between 0.0 and 1.0".into());
    }
    if !frequency_hz.is_finite() || frequency_hz <= 0.0 {
        return Err("--frequency must be positive and finite".into());
    }

    if dry_run {
        print_dry_run(
            task_name.as_deref(),
            kind,
            &channel,
            min_volts,
            max_volts,
            terminal,
            edge,
            direction,
            idle_state,
            frequency_hz,
            duty_cycle,
        );
        return Ok(());
    }

    let task_name_c = optional_cstring(task_name.as_deref())?;
    let mut handle = ptr::null_mut();
    check_status(
        unsafe { ni_daqmx_sys::DAQmxCreateTask(cstr_ptr(task_name_c.as_ref()), &mut handle) },
        "DAQmxCreateTask",
    )?;
    println!("created_task\ttrue");
    println!("task_handle_null\t{}", handle.is_null());

    let result = configure_channel(
        handle,
        kind,
        &channel,
        min_volts,
        max_volts,
        terminal,
        edge,
        direction,
        idle_state,
        frequency_hz,
        duty_cycle,
    );

    let status = unsafe { ni_daqmx_sys::DAQmxClearTask(handle) };
    if status >= 0 {
        println!("cleared_task\ttrue");
    }

    result?;
    check_status(status, "DAQmxClearTask")?;
    Ok(())
}

fn validate_optional_task_name(task_name: Option<&str>) -> Result<(), String> {
    if let Some(value) = task_name {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(
                "--name must not be empty; use --unnamed for a null DAQmx task name".into(),
            );
        }
        if trimmed != value {
            return Err("--name must not have leading or trailing whitespace".into());
        }
    }
    Ok(())
}

fn validate_channel(channel: &str) -> Result<(), String> {
    let trimmed = channel.trim();
    if trimmed.is_empty() {
        return Err("--channel must not be empty".into());
    }
    if trimmed != channel {
        return Err("--channel must not have leading or trailing whitespace".into());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn configure_channel(
    handle: ni_daqmx_sys::TaskHandle,
    kind: ChannelKind,
    channel: &str,
    min_volts: f64,
    max_volts: f64,
    terminal: TerminalConfig,
    edge: Edge,
    direction: CountDirection,
    idle_state: IdleState,
    frequency_hz: f64,
    duty_cycle: f64,
) -> Result<(), String> {
    let channel_c = required_cstring("channel", channel)?;
    match kind {
        ChannelKind::Ao => check_status(
            unsafe {
                ni_daqmx_sys::DAQmxCreateAOVoltageChan(
                    handle,
                    channel_c.as_ptr(),
                    ptr::null(),
                    min_volts,
                    max_volts,
                    ni_daqmx_sys::DAQmx_Val_Volts,
                    ptr::null(),
                )
            },
            "DAQmxCreateAOVoltageChan",
        )?,
        ChannelKind::Do => check_status(
            unsafe {
                ni_daqmx_sys::DAQmxCreateDOChan(
                    handle,
                    channel_c.as_ptr(),
                    ptr::null(),
                    ni_daqmx_sys::DAQmx_Val_ChanForAllLines,
                )
            },
            "DAQmxCreateDOChan",
        )?,
        ChannelKind::Ai => check_status(
            unsafe {
                ni_daqmx_sys::DAQmxCreateAIVoltageChan(
                    handle,
                    channel_c.as_ptr(),
                    ptr::null(),
                    terminal.as_raw(),
                    min_volts,
                    max_volts,
                    ni_daqmx_sys::DAQmx_Val_Volts,
                    ptr::null(),
                )
            },
            "DAQmxCreateAIVoltageChan",
        )?,
        ChannelKind::Ci => check_status(
            unsafe {
                ni_daqmx_sys::DAQmxCreateCICountEdgesChan(
                    handle,
                    channel_c.as_ptr(),
                    ptr::null(),
                    edge.as_raw(),
                    0,
                    direction.as_raw(),
                )
            },
            "DAQmxCreateCICountEdgesChan",
        )?,
        ChannelKind::Co => check_status(
            unsafe {
                ni_daqmx_sys::DAQmxCreateCOPulseChanFreq(
                    handle,
                    channel_c.as_ptr(),
                    ptr::null(),
                    ni_daqmx_sys::DAQmx_Val_Hz,
                    idle_state.as_raw(),
                    0.0,
                    frequency_hz,
                    duty_cycle,
                )
            },
            "DAQmxCreateCOPulseChanFreq",
        )?,
    }
    println!("configured_kind\t{}", kind.as_str());
    println!("configured_channel\t{channel}");
    println!("started_task\tfalse");
    println!("wrote_output\tfalse");
    println!("read_input\tfalse");
    Ok(())
}

fn print_usage() {
    eprintln!(
        "usage: numanager-daqmx-channel-setup-helper --kind ao|do|ai|ci|co --channel PHYSICAL_CHANNEL [options] [--dry-run]"
    );
    eprintln!("creates a task, configures one channel role, and clears the task");
    eprintln!("does not start tasks, write outputs, or read inputs");
    eprintln!("--dry-run prints planned channel setup calls without creating a task");
    eprintln!("options: --min-volts V --max-volts V --terminal default|differential|rse|nrse");
    eprintln!("         --edge rising|falling --direction up|down");
    eprintln!("         --idle-state low|high --frequency HZ --duty-cycle FRACTION");
}

#[allow(clippy::too_many_arguments)]
fn print_dry_run(
    task_name: Option<&str>,
    kind: ChannelKind,
    channel: &str,
    min_volts: f64,
    max_volts: f64,
    terminal: TerminalConfig,
    edge: Edge,
    direction: CountDirection,
    idle_state: IdleState,
    frequency_hz: f64,
    duty_cycle: f64,
) {
    println!("channel_setup_plan\ttrue");
    println!("execute\tfalse");
    println!("task_name\t{}", task_name.unwrap_or("<unnamed>"));
    println!("kind\t{}", kind.as_str());
    println!("channel\t{channel}");
    println!("planned_api\t{}", kind.planned_api());
    match kind {
        ChannelKind::Ao | ChannelKind::Ai => {
            println!("analog_range_volts\t{min_volts:.6}\t{max_volts:.6}");
        }
        _ => {}
    }
    if matches!(kind, ChannelKind::Ai) {
        println!("terminal\t{}", terminal.as_str());
    }
    if matches!(kind, ChannelKind::Ci) {
        println!("edge\t{}", edge.as_str());
        println!("direction\t{}", direction.as_str());
    }
    if matches!(kind, ChannelKind::Co) {
        println!("frequency_hz\t{frequency_hz:.6}");
        println!("duty_cycle\t{duty_cycle:.6}");
        println!("idle_state\t{}", idle_state.as_str());
    }
    println!("created_task\tfalse");
    println!("configured_channel\tfalse");
    println!("started_task\tfalse");
    println!("wrote_output\tfalse");
    println!("read_input\tfalse");
    println!("cleared_task\tfalse");
}

#[derive(Debug, Clone, Copy)]
enum ChannelKind {
    Ao,
    Do,
    Ai,
    Ci,
    Co,
}

impl ChannelKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ao => "ao",
            Self::Do => "do",
            Self::Ai => "ai",
            Self::Ci => "ci",
            Self::Co => "co",
        }
    }

    fn planned_api(self) -> &'static str {
        match self {
            Self::Ao => "DAQmxCreateTask,DAQmxCreateAOVoltageChan,DAQmxClearTask",
            Self::Do => "DAQmxCreateTask,DAQmxCreateDOChan,DAQmxClearTask",
            Self::Ai => "DAQmxCreateTask,DAQmxCreateAIVoltageChan,DAQmxClearTask",
            Self::Ci => "DAQmxCreateTask,DAQmxCreateCICountEdgesChan,DAQmxClearTask",
            Self::Co => "DAQmxCreateTask,DAQmxCreateCOPulseChanFreq,DAQmxClearTask",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum TerminalConfig {
    Default,
    Differential,
    Rse,
    Nrse,
}

impl TerminalConfig {
    fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Differential => "differential",
            Self::Rse => "rse",
            Self::Nrse => "nrse",
        }
    }

    fn as_raw(self) -> ni_daqmx_sys::int32 {
        match self {
            Self::Default => ni_daqmx_sys::DAQmx_Val_Cfg_Default,
            Self::Differential => ni_daqmx_sys::DAQmx_Val_Diff,
            Self::Rse => ni_daqmx_sys::DAQmx_Val_RSE,
            Self::Nrse => ni_daqmx_sys::DAQmx_Val_NRSE,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Edge {
    Rising,
    Falling,
}

impl Edge {
    fn as_str(self) -> &'static str {
        match self {
            Self::Rising => "rising",
            Self::Falling => "falling",
        }
    }

    fn as_raw(self) -> ni_daqmx_sys::int32 {
        match self {
            Self::Rising => ni_daqmx_sys::DAQmx_Val_Rising,
            Self::Falling => ni_daqmx_sys::DAQmx_Val_Falling,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum CountDirection {
    Up,
    Down,
}

impl CountDirection {
    fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
        }
    }

    fn as_raw(self) -> ni_daqmx_sys::int32 {
        match self {
            Self::Up => ni_daqmx_sys::DAQmx_Val_CountUp,
            Self::Down => ni_daqmx_sys::DAQmx_Val_CountDown,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum IdleState {
    Low,
    High,
}

impl IdleState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::High => "high",
        }
    }

    fn as_raw(self) -> ni_daqmx_sys::int32 {
        match self {
            Self::Low => ni_daqmx_sys::DAQmx_Val_Low,
            Self::High => ni_daqmx_sys::DAQmx_Val_High,
        }
    }
}

fn parse_kind(value: &str) -> Result<ChannelKind, String> {
    match value {
        "ao" => Ok(ChannelKind::Ao),
        "do" => Ok(ChannelKind::Do),
        "ai" => Ok(ChannelKind::Ai),
        "ci" => Ok(ChannelKind::Ci),
        "co" => Ok(ChannelKind::Co),
        _ => Err(format!("unsupported --kind {value:?}")),
    }
}

fn parse_terminal(value: &str) -> Result<TerminalConfig, String> {
    match value {
        "default" => Ok(TerminalConfig::Default),
        "differential" | "diff" => Ok(TerminalConfig::Differential),
        "rse" => Ok(TerminalConfig::Rse),
        "nrse" => Ok(TerminalConfig::Nrse),
        _ => Err(format!("unsupported --terminal {value:?}")),
    }
}

fn parse_edge(value: &str) -> Result<Edge, String> {
    match value {
        "rising" => Ok(Edge::Rising),
        "falling" => Ok(Edge::Falling),
        _ => Err(format!("unsupported --edge {value:?}")),
    }
}

fn parse_direction(value: &str) -> Result<CountDirection, String> {
    match value {
        "up" => Ok(CountDirection::Up),
        "down" => Ok(CountDirection::Down),
        _ => Err(format!("unsupported --direction {value:?}")),
    }
}

fn parse_idle_state(value: &str) -> Result<IdleState, String> {
    match value {
        "low" => Ok(IdleState::Low),
        "high" => Ok(IdleState::High),
        _ => Err(format!("unsupported --idle-state {value:?}")),
    }
}

fn required_arg(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_f64(value: &str) -> Result<f64, String> {
    value
        .parse::<f64>()
        .map_err(|error| format!("invalid numeric value {value:?}: {error}"))
}

fn optional_cstring(value: Option<&str>) -> Result<Option<CString>, String> {
    value
        .map(|value| required_cstring("task name", value))
        .transpose()
}

fn required_cstring(field: &str, value: &str) -> Result<CString, String> {
    CString::new(value).map_err(|_| format!("{field} contains an interior NUL byte"))
}

fn cstr_ptr(value: Option<&CString>) -> *const c_char {
    value.map(|value| value.as_ptr()).unwrap_or_else(ptr::null)
}

fn check_status(status: ni_daqmx_sys::int32, call: &str) -> Result<(), String> {
    if status < 0 {
        Err(format_status(status, call))
    } else {
        Ok(())
    }
}

fn format_status(status: ni_daqmx_sys::int32, call: &str) -> String {
    format!("{call} failed: {}", error_string(status))
}

fn error_string(status: ni_daqmx_sys::int32) -> String {
    let mut buffer = vec![0 as c_char; 4096];
    let extended_status = unsafe {
        ni_daqmx_sys::DAQmxGetExtendedErrorInfo(
            buffer.as_mut_ptr(),
            buffer.len() as ni_daqmx_sys::uInt32,
        )
    };
    if extended_status >= 0 {
        let message = unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        if !message.trim().is_empty() {
            return format!("DAQmx status {status}: {message}");
        }
    }

    let error_status = unsafe {
        ni_daqmx_sys::DAQmxGetErrorString(
            status,
            buffer.as_mut_ptr(),
            buffer.len() as ni_daqmx_sys::uInt32,
        )
    };
    if error_status >= 0 {
        let message = unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        format!("DAQmx status {status}: {message}")
    } else {
        format!(
            "DAQmx status {status}; DAQmxGetExtendedErrorInfo returned {extended_status}; DAQmxGetErrorString returned {error_status}"
        )
    }
}
