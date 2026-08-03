use std::ffi::{CStr, CString};
use std::io::Write;
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
    let mut task_name = Some("numanager-daqmx-io-smoke-probe".to_owned());
    let mut kind = None;
    let mut channel = None;
    let mut execute = false;
    let mut min_volts = -10.0;
    let mut max_volts = 10.0;
    let mut volts = 0.0;
    let mut line_state = false;
    let mut terminal = TerminalConfig::Default;
    let mut edge = Edge::Rising;
    let mut direction = CountDirection::Up;
    let mut idle_state = IdleState::Low;
    let mut frequency_hz = 10.0;
    let mut duty_cycle = 0.5;
    let mut samples = 1_u64;
    let mut timeout_seconds = 10.0;
    let mut simulate_error_after_start = false;
    let mut bench_safety_reviewed = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--name" => task_name = Some(required_arg(&mut args, "--name")?),
            "--unnamed" => task_name = None,
            "--kind" => kind = Some(parse_kind(&required_arg(&mut args, "--kind")?)?),
            "--channel" => channel = Some(required_arg(&mut args, "--channel")?),
            "--execute" => execute = true,
            "--min-volts" => min_volts = parse_f64(&required_arg(&mut args, "--min-volts")?)?,
            "--max-volts" => max_volts = parse_f64(&required_arg(&mut args, "--max-volts")?)?,
            "--volts" => volts = parse_f64(&required_arg(&mut args, "--volts")?)?,
            "--line-state" => line_state = parse_bool(&required_arg(&mut args, "--line-state")?)?,
            "--terminal" => terminal = parse_terminal(&required_arg(&mut args, "--terminal")?)?,
            "--edge" => edge = parse_edge(&required_arg(&mut args, "--edge")?)?,
            "--direction" => direction = parse_direction(&required_arg(&mut args, "--direction")?)?,
            "--idle-state" => {
                idle_state = parse_idle_state(&required_arg(&mut args, "--idle-state")?)?
            }
            "--frequency" => frequency_hz = parse_f64(&required_arg(&mut args, "--frequency")?)?,
            "--duty-cycle" => duty_cycle = parse_f64(&required_arg(&mut args, "--duty-cycle")?)?,
            "--samples" => samples = parse_u64(&required_arg(&mut args, "--samples")?)?,
            "--timeout" => timeout_seconds = parse_f64(&required_arg(&mut args, "--timeout")?)?,
            "--simulate-error-after-start" => simulate_error_after_start = true,
            "--bench-safety-reviewed" => bench_safety_reviewed = true,
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
    validate(
        kind,
        min_volts,
        max_volts,
        volts,
        frequency_hz,
        duty_cycle,
        samples,
        timeout_seconds,
        simulate_error_after_start,
        execute,
        bench_safety_reviewed,
    )?;

    print_plan(
        kind,
        &channel,
        execute,
        min_volts,
        max_volts,
        volts,
        line_state,
        terminal,
        edge,
        direction,
        idle_state,
        frequency_hz,
        duty_cycle,
        samples,
        timeout_seconds,
        simulate_error_after_start,
        bench_safety_reviewed,
    );
    if !execute {
        if simulate_error_after_start {
            println!("simulated_failure\ttrue");
            println!("simulated_error_message\tsimulated I/O error after task start");
            println!("created_task\tfalse");
            println!("started_task\tsimulated");
            println!("cleanup_after_io_error\ttrue");
            println!("stopped_task_after_error\tsimulated_no_task");
            println!("wrote_output\tfalse");
            println!("read_input\tfalse");
            println!("generated_output\tfalse");
            println!("cleared_task\tfalse");
            std::io::stdout()
                .flush()
                .map_err(|error| format!("failed to flush simulated failure output: {error}"))?;
            return Ok(());
        }
        println!("created_task\tfalse");
        println!("started_task\tfalse");
        println!("wrote_output\tfalse");
        println!("read_input\tfalse");
        println!("generated_output\tfalse");
        println!("cleared_task\tfalse");
        return Ok(());
    }

    let mut task = create_task(task_name.as_deref())?;
    let result = execute_io(
        &mut task,
        kind,
        &channel,
        min_volts,
        max_volts,
        volts,
        line_state,
        terminal,
        edge,
        direction,
        idle_state,
        frequency_hz,
        duty_cycle,
        samples,
        timeout_seconds,
        simulate_error_after_start,
    );
    let stop_after_error = if result.is_err() {
        println!("cleanup_after_io_error\ttrue");
        task.stop_started_after_error().err()
    } else {
        None
    };
    let clear_result = task.clear_inner();
    if clear_result.is_ok() {
        println!("cleared_task\ttrue");
    }

    match (result, stop_after_error, clear_result) {
        (Ok(()), _, Ok(())) => Ok(()),
        (Ok(()), _, Err(clear_error)) => Err(clear_error),
        (Err(error), None, Ok(())) => Err(error),
        (Err(error), Some(stop_error), Ok(())) => {
            Err(format!("{error}; cleanup stop after I/O error failed: {stop_error}"))
        }
        (Err(error), None, Err(clear_error)) => {
            Err(format!("{error}; cleanup clear after I/O error failed: {clear_error}"))
        }
        (Err(error), Some(stop_error), Err(clear_error)) => Err(format!(
            "{error}; cleanup stop after I/O error failed: {stop_error}; cleanup clear after I/O error failed: {clear_error}"
        )),
    }
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
fn validate(
    kind: ChannelKind,
    min_volts: f64,
    max_volts: f64,
    volts: f64,
    frequency_hz: f64,
    duty_cycle: f64,
    samples: u64,
    timeout_seconds: f64,
    simulate_error_after_start: bool,
    execute: bool,
    bench_safety_reviewed: bool,
) -> Result<(), String> {
    if execute && !bench_safety_reviewed {
        return Err(
            "--execute requires --bench-safety-reviewed after completing the bench safety preconditions"
                .into(),
        );
    }
    if !min_volts.is_finite() || !max_volts.is_finite() {
        return Err("--min-volts and --max-volts must be finite".into());
    }
    if min_volts > max_volts {
        return Err("--min-volts must not exceed --max-volts".into());
    }
    if !volts.is_finite() {
        return Err("--volts must be finite".into());
    }
    if matches!(kind, ChannelKind::Ao) && !(min_volts..=max_volts).contains(&volts) {
        return Err("--volts must be inside --min-volts/--max-volts for AO".into());
    }
    if matches!(kind, ChannelKind::Ao) && !(min_volts..=max_volts).contains(&0.0) {
        return Err("--min-volts/--max-volts must include 0.0 for AO safe final write".into());
    }
    if !frequency_hz.is_finite() || frequency_hz <= 0.0 {
        return Err("--frequency must be positive and finite".into());
    }
    if !duty_cycle.is_finite() || !(0.0..=1.0).contains(&duty_cycle) {
        return Err("--duty-cycle must be finite and between 0.0 and 1.0".into());
    }
    if samples == 0 {
        return Err("--samples must be positive".into());
    }
    if samples > i32::MAX as u64 {
        return Err("--samples exceeds NI-DAQmx i32 sample count range".into());
    }
    if !timeout_seconds.is_finite() || timeout_seconds <= 0.0 {
        return Err("--timeout must be positive and finite".into());
    }
    if simulate_error_after_start && matches!(kind, ChannelKind::Ao | ChannelKind::Do) {
        return Err("--simulate-error-after-start is supported only for ai, ci, or co".into());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn print_plan(
    kind: ChannelKind,
    channel: &str,
    execute: bool,
    min_volts: f64,
    max_volts: f64,
    volts: f64,
    line_state: bool,
    terminal: TerminalConfig,
    edge: Edge,
    direction: CountDirection,
    idle_state: IdleState,
    frequency_hz: f64,
    duty_cycle: f64,
    samples: u64,
    timeout_seconds: f64,
    simulate_error_after_start: bool,
    bench_safety_reviewed: bool,
) {
    println!("io_smoke_plan\ttrue");
    println!("execute\t{execute}");
    println!("bench_safety_reviewed\t{bench_safety_reviewed}");
    println!("kind\t{}", kind.as_str());
    println!("channel\t{channel}");
    println!("samples_per_channel\t{samples}");
    println!("timeout_s\t{timeout_seconds:.6}");
    println!("simulate_error_after_start\t{simulate_error_after_start}");
    match kind {
        ChannelKind::Ao => {
            println!("planned_api\tDAQmxCreateTask,DAQmxCreateAOVoltageChan,DAQmxWriteAnalogF64,DAQmxWriteAnalogF64,DAQmxClearTask");
            println!("analog_range_volts\t{min_volts:.6}\t{max_volts:.6}");
            println!("write_volts\t{volts:.6}");
            println!("final_safe_state\t0.000000 V before clear");
            println!("auto_start\ttrue");
        }
        ChannelKind::Do => {
            println!("planned_api\tDAQmxCreateTask,DAQmxCreateDOChan,DAQmxWriteDigitalLines,DAQmxWriteDigitalLines,DAQmxClearTask");
            println!("line_state\t{}", u8::from(line_state));
            println!("final_safe_state\tlow before clear");
            println!("auto_start\ttrue");
        }
        ChannelKind::Ai => {
            println!("planned_api\tDAQmxCreateTask,DAQmxCreateAIVoltageChan,DAQmxStartTask,DAQmxReadAnalogF64,DAQmxStopTask,DAQmxClearTask");
            println!("analog_range_volts\t{min_volts:.6}\t{max_volts:.6}");
            println!("terminal\t{}", terminal.as_str());
        }
        ChannelKind::Ci => {
            println!("planned_api\tDAQmxCreateTask,DAQmxCreateCICountEdgesChan,DAQmxStartTask,DAQmxReadCounterU32,DAQmxStopTask,DAQmxClearTask");
            println!("edge\t{}", edge.as_str());
            println!("direction\t{}", direction.as_str());
        }
        ChannelKind::Co => {
            println!("planned_api\tDAQmxCreateTask,DAQmxCreateCOPulseChanFreq,DAQmxCfgImplicitTiming,DAQmxStartTask,DAQmxWaitUntilTaskDone,DAQmxStopTask,DAQmxClearTask");
            println!("frequency_hz\t{frequency_hz:.6}");
            println!("duty_cycle\t{duty_cycle:.6}");
            println!("idle_state\t{}", idle_state.as_str());
            println!(
                "final_safe_state\tidle_state={} after stop",
                idle_state.as_str()
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_io(
    task: &mut Task,
    kind: ChannelKind,
    channel: &str,
    min_volts: f64,
    max_volts: f64,
    volts: f64,
    line_state: bool,
    terminal: TerminalConfig,
    edge: Edge,
    direction: CountDirection,
    idle_state: IdleState,
    frequency_hz: f64,
    duty_cycle: f64,
    samples: u64,
    timeout_seconds: f64,
    simulate_error_after_start: bool,
) -> Result<(), String> {
    match kind {
        ChannelKind::Ao => {
            create_ao_voltage_channel(task.handle, channel, min_volts, max_volts)?;
            println!("configured_channel\ttrue");
            let written = write_analog_f64(task.handle, volts, timeout_seconds)?;
            let final_written = write_analog_f64(task.handle, 0.0, timeout_seconds)?;
            println!("started_task\tauto");
            println!("wrote_output\ttrue");
            println!("samples_written\t{written}");
            println!("final_safe_state\t0.000000 V before clear");
            println!("final_safe_samples_written\t{final_written}");
            println!("read_input\tfalse");
            println!("generated_output\tfalse");
        }
        ChannelKind::Do => {
            create_do_lines(task.handle, channel)?;
            println!("configured_channel\ttrue");
            let written = write_digital_lines(task.handle, line_state, timeout_seconds)?;
            let final_written = write_digital_lines(task.handle, false, timeout_seconds)?;
            println!("started_task\tauto");
            println!("wrote_output\ttrue");
            println!("samples_written\t{written}");
            println!("final_safe_state\tlow before clear");
            println!("final_safe_samples_written\t{final_written}");
            println!("read_input\tfalse");
            println!("generated_output\tfalse");
        }
        ChannelKind::Ai => {
            create_ai_voltage_channel(task.handle, channel, terminal, min_volts, max_volts)?;
            println!("configured_channel\ttrue");
            start_task(task.handle)?;
            task.started = true;
            println!("started_task\ttrue");
            fail_after_start_if_requested(simulate_error_after_start)?;
            let (read, value) = read_analog_f64(task.handle, samples, timeout_seconds)?;
            task.stop_started()?;
            println!("stopped_task\ttrue");
            println!("wrote_output\tfalse");
            println!("read_input\ttrue");
            println!("samples_read\t{read}");
            println!("first_sample_volts\t{value:.9}");
            println!("generated_output\tfalse");
        }
        ChannelKind::Ci => {
            create_ci_count_edges_channel(task.handle, channel, edge, direction)?;
            println!("configured_channel\ttrue");
            start_task(task.handle)?;
            task.started = true;
            println!("started_task\ttrue");
            fail_after_start_if_requested(simulate_error_after_start)?;
            let (read, value) = read_counter_u32(task.handle, samples, timeout_seconds)?;
            task.stop_started()?;
            println!("stopped_task\ttrue");
            println!("wrote_output\tfalse");
            println!("read_input\ttrue");
            println!("samples_read\t{read}");
            println!("first_sample_count\t{value}");
            println!("generated_output\tfalse");
        }
        ChannelKind::Co => {
            create_co_pulse_channel_freq(
                task.handle,
                channel,
                idle_state,
                frequency_hz,
                duty_cycle,
            )?;
            cfg_implicit_timing(task.handle, samples)?;
            println!("configured_channel\ttrue");
            start_task(task.handle)?;
            task.started = true;
            println!("started_task\ttrue");
            fail_after_start_if_requested(simulate_error_after_start)?;
            wait_until_done(task.handle, timeout_seconds)?;
            println!("waited_until_done\ttrue");
            task.stop_started()?;
            println!("stopped_task\ttrue");
            println!("wrote_output\tfalse");
            println!("read_input\tfalse");
            println!("generated_output\ttrue");
            println!("samples_generated\t{samples}");
            println!(
                "final_safe_state\tidle_state={} after stop",
                idle_state.as_str()
            );
        }
    }
    Ok(())
}

fn fail_after_start_if_requested(simulate_error_after_start: bool) -> Result<(), String> {
    if simulate_error_after_start {
        Err("simulated I/O error after task start".into())
    } else {
        Ok(())
    }
}

fn create_task(name: Option<&str>) -> Result<Task, String> {
    let name = optional_cstring(name)?;
    let mut handle = ptr::null_mut();
    check_status(
        unsafe { ni_daqmx_sys::DAQmxCreateTask(cstr_ptr(name.as_ref()), &mut handle) },
        "DAQmxCreateTask",
    )?;
    println!("created_task\ttrue");
    println!("task_handle_null\t{}", handle.is_null());
    Ok(Task {
        handle,
        started: false,
        cleared: false,
    })
}

struct Task {
    handle: ni_daqmx_sys::TaskHandle,
    started: bool,
    cleared: bool,
}

impl Task {
    fn stop_started(&mut self) -> Result<bool, String> {
        if !self.started || self.handle.is_null() {
            return Ok(false);
        }
        stop_task(self.handle)?;
        self.started = false;
        Ok(true)
    }

    fn stop_started_after_error(&mut self) -> Result<(), String> {
        match self.stop_started() {
            Ok(true) => {
                println!("stopped_task_after_error\ttrue");
                Ok(())
            }
            Ok(false) => {
                println!("stopped_task_after_error\tfalse");
                Ok(())
            }
            Err(error) => {
                println!("stopped_task_after_error\tfalse");
                Err(error)
            }
        }
    }

    fn clear_inner(&mut self) -> Result<(), String> {
        if self.cleared || self.handle.is_null() {
            return Ok(());
        }
        let status = unsafe { ni_daqmx_sys::DAQmxClearTask(self.handle) };
        if status >= 0 {
            self.cleared = true;
            self.started = false;
            self.handle = ptr::null_mut();
        }
        check_status(status, "DAQmxClearTask")
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        let _ = self.clear_inner();
    }
}

fn create_ao_voltage_channel(
    handle: ni_daqmx_sys::TaskHandle,
    channel: &str,
    min_volts: f64,
    max_volts: f64,
) -> Result<(), String> {
    let channel = required_cstring("channel", channel)?;
    check_status(
        unsafe {
            ni_daqmx_sys::DAQmxCreateAOVoltageChan(
                handle,
                channel.as_ptr(),
                ptr::null(),
                min_volts,
                max_volts,
                ni_daqmx_sys::DAQmx_Val_Volts,
                ptr::null(),
            )
        },
        "DAQmxCreateAOVoltageChan",
    )
}

fn create_do_lines(handle: ni_daqmx_sys::TaskHandle, lines: &str) -> Result<(), String> {
    let lines = required_cstring("channel", lines)?;
    check_status(
        unsafe {
            ni_daqmx_sys::DAQmxCreateDOChan(
                handle,
                lines.as_ptr(),
                ptr::null(),
                ni_daqmx_sys::DAQmx_Val_ChanForAllLines,
            )
        },
        "DAQmxCreateDOChan",
    )
}

fn create_ai_voltage_channel(
    handle: ni_daqmx_sys::TaskHandle,
    channel: &str,
    terminal: TerminalConfig,
    min_volts: f64,
    max_volts: f64,
) -> Result<(), String> {
    let channel = required_cstring("channel", channel)?;
    check_status(
        unsafe {
            ni_daqmx_sys::DAQmxCreateAIVoltageChan(
                handle,
                channel.as_ptr(),
                ptr::null(),
                terminal.as_raw(),
                min_volts,
                max_volts,
                ni_daqmx_sys::DAQmx_Val_Volts,
                ptr::null(),
            )
        },
        "DAQmxCreateAIVoltageChan",
    )
}

fn create_ci_count_edges_channel(
    handle: ni_daqmx_sys::TaskHandle,
    counter: &str,
    edge: Edge,
    direction: CountDirection,
) -> Result<(), String> {
    let counter = required_cstring("channel", counter)?;
    check_status(
        unsafe {
            ni_daqmx_sys::DAQmxCreateCICountEdgesChan(
                handle,
                counter.as_ptr(),
                ptr::null(),
                edge.as_raw(),
                0,
                direction.as_raw(),
            )
        },
        "DAQmxCreateCICountEdgesChan",
    )
}

fn create_co_pulse_channel_freq(
    handle: ni_daqmx_sys::TaskHandle,
    counter: &str,
    idle_state: IdleState,
    frequency_hz: f64,
    duty_cycle: f64,
) -> Result<(), String> {
    let counter = required_cstring("channel", counter)?;
    check_status(
        unsafe {
            ni_daqmx_sys::DAQmxCreateCOPulseChanFreq(
                handle,
                counter.as_ptr(),
                ptr::null(),
                ni_daqmx_sys::DAQmx_Val_Hz,
                idle_state.as_raw(),
                0.0,
                frequency_hz,
                duty_cycle,
            )
        },
        "DAQmxCreateCOPulseChanFreq",
    )
}

fn cfg_implicit_timing(handle: ni_daqmx_sys::TaskHandle, samples: u64) -> Result<(), String> {
    check_status(
        unsafe {
            ni_daqmx_sys::DAQmxCfgImplicitTiming(
                handle,
                ni_daqmx_sys::DAQmx_Val_FiniteSamps,
                samples,
            )
        },
        "DAQmxCfgImplicitTiming",
    )
}

fn start_task(handle: ni_daqmx_sys::TaskHandle) -> Result<(), String> {
    check_status(
        unsafe { ni_daqmx_sys::DAQmxStartTask(handle) },
        "DAQmxStartTask",
    )
}

fn stop_task(handle: ni_daqmx_sys::TaskHandle) -> Result<(), String> {
    check_status(
        unsafe { ni_daqmx_sys::DAQmxStopTask(handle) },
        "DAQmxStopTask",
    )
}

fn wait_until_done(handle: ni_daqmx_sys::TaskHandle, timeout_seconds: f64) -> Result<(), String> {
    check_status(
        unsafe { ni_daqmx_sys::DAQmxWaitUntilTaskDone(handle, timeout_seconds) },
        "DAQmxWaitUntilTaskDone",
    )
}

fn write_analog_f64(
    handle: ni_daqmx_sys::TaskHandle,
    volts: f64,
    timeout_seconds: f64,
) -> Result<i32, String> {
    let mut written = 0;
    check_status(
        unsafe {
            ni_daqmx_sys::DAQmxWriteAnalogF64(
                handle,
                1,
                1,
                timeout_seconds,
                ni_daqmx_sys::DAQmx_Val_GroupByScanNumber as _,
                &volts,
                &mut written,
                ptr::null_mut(),
            )
        },
        "DAQmxWriteAnalogF64",
    )?;
    Ok(written)
}

fn write_digital_lines(
    handle: ni_daqmx_sys::TaskHandle,
    line_state: bool,
    timeout_seconds: f64,
) -> Result<i32, String> {
    let data = u8::from(line_state);
    let mut written = 0;
    check_status(
        unsafe {
            ni_daqmx_sys::DAQmxWriteDigitalLines(
                handle,
                1,
                1,
                timeout_seconds,
                ni_daqmx_sys::DAQmx_Val_GroupByScanNumber as _,
                &data,
                &mut written,
                ptr::null_mut(),
            )
        },
        "DAQmxWriteDigitalLines",
    )?;
    Ok(written)
}

fn read_analog_f64(
    handle: ni_daqmx_sys::TaskHandle,
    samples: u64,
    timeout_seconds: f64,
) -> Result<(i32, f64), String> {
    let mut buffer = vec![0.0; samples as usize];
    let mut read = 0;
    check_status(
        unsafe {
            ni_daqmx_sys::DAQmxReadAnalogF64(
                handle,
                samples as i32,
                timeout_seconds,
                ni_daqmx_sys::DAQmx_Val_GroupByScanNumber as _,
                buffer.as_mut_ptr(),
                buffer.len() as ni_daqmx_sys::uInt32,
                &mut read,
                ptr::null_mut(),
            )
        },
        "DAQmxReadAnalogF64",
    )?;
    Ok((read, buffer.first().copied().unwrap_or_default()))
}

fn read_counter_u32(
    handle: ni_daqmx_sys::TaskHandle,
    samples: u64,
    timeout_seconds: f64,
) -> Result<(i32, u32), String> {
    let mut buffer = vec![0_u32; samples as usize];
    let mut read = 0;
    check_status(
        unsafe {
            ni_daqmx_sys::DAQmxReadCounterU32(
                handle,
                samples as i32,
                timeout_seconds,
                buffer.as_mut_ptr(),
                buffer.len() as ni_daqmx_sys::uInt32,
                &mut read,
                ptr::null_mut(),
            )
        },
        "DAQmxReadCounterU32",
    )?;
    Ok((read, buffer.first().copied().unwrap_or_default()))
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
}

#[derive(Debug, Clone, Copy)]
enum TerminalConfig {
    Default,
    Differential,
    Rse,
    Nrse,
}

impl TerminalConfig {
    fn as_raw(self) -> ni_daqmx_sys::int32 {
        match self {
            Self::Default => ni_daqmx_sys::DAQmx_Val_Cfg_Default,
            Self::Differential => ni_daqmx_sys::DAQmx_Val_Diff,
            Self::Rse => ni_daqmx_sys::DAQmx_Val_RSE,
            Self::Nrse => ni_daqmx_sys::DAQmx_Val_NRSE,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Differential => "differential",
            Self::Rse => "rse",
            Self::Nrse => "nrse",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Edge {
    Rising,
    Falling,
}

impl Edge {
    fn as_raw(self) -> ni_daqmx_sys::int32 {
        match self {
            Self::Rising => ni_daqmx_sys::DAQmx_Val_Rising,
            Self::Falling => ni_daqmx_sys::DAQmx_Val_Falling,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Rising => "rising",
            Self::Falling => "falling",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum CountDirection {
    Up,
    Down,
}

impl CountDirection {
    fn as_raw(self) -> ni_daqmx_sys::int32 {
        match self {
            Self::Up => ni_daqmx_sys::DAQmx_Val_CountUp,
            Self::Down => ni_daqmx_sys::DAQmx_Val_CountDown,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum IdleState {
    Low,
    High,
}

impl IdleState {
    fn as_raw(self) -> ni_daqmx_sys::int32 {
        match self {
            Self::Low => ni_daqmx_sys::DAQmx_Val_Low,
            Self::High => ni_daqmx_sys::DAQmx_Val_High,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::High => "high",
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
        _ => Err("kind must be ao, do, ai, ci, or co".into()),
    }
}

fn parse_terminal(value: &str) -> Result<TerminalConfig, String> {
    match value {
        "default" => Ok(TerminalConfig::Default),
        "differential" => Ok(TerminalConfig::Differential),
        "rse" => Ok(TerminalConfig::Rse),
        "nrse" => Ok(TerminalConfig::Nrse),
        _ => Err("terminal must be default, differential, rse, or nrse".into()),
    }
}

fn parse_edge(value: &str) -> Result<Edge, String> {
    match value {
        "rising" => Ok(Edge::Rising),
        "falling" => Ok(Edge::Falling),
        _ => Err("edge must be rising or falling".into()),
    }
}

fn parse_direction(value: &str) -> Result<CountDirection, String> {
    match value {
        "up" => Ok(CountDirection::Up),
        "down" => Ok(CountDirection::Down),
        _ => Err("direction must be up or down".into()),
    }
}

fn parse_idle_state(value: &str) -> Result<IdleState, String> {
    match value {
        "low" => Ok(IdleState::Low),
        "high" => Ok(IdleState::High),
        _ => Err("idle-state must be low or high".into()),
    }
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match value {
        "0" | "false" | "False" | "FALSE" | "low" => Ok(false),
        "1" | "true" | "True" | "TRUE" | "high" => Ok(true),
        _ => Err("boolean value must be true/false, 1/0, or high/low".into()),
    }
}

fn parse_f64(value: &str) -> Result<f64, String> {
    value
        .parse()
        .map_err(|error| format!("invalid float {value:?}: {error}"))
}

fn parse_u64(value: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|error| format!("invalid integer {value:?}: {error}"))
}

fn required_arg(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn optional_cstring(value: Option<&str>) -> Result<Option<CString>, String> {
    value
        .map(|value| CString::new(value).map_err(|_| "string contains interior NUL".to_owned()))
        .transpose()
}

fn required_cstring(label: &str, value: &str) -> Result<CString, String> {
    CString::new(value).map_err(|_| format!("{label} contains interior NUL"))
}

fn cstr_ptr(value: Option<&CString>) -> *const c_char {
    value.map(|value| value.as_ptr()).unwrap_or(ptr::null())
}

fn check_status(status: ni_daqmx_sys::int32, api: &str) -> Result<(), String> {
    if status >= 0 {
        Ok(())
    } else {
        Err(format_status(status, api))
    }
}

fn format_status(status: ni_daqmx_sys::int32, api: &str) -> String {
    format!("{api} failed: {}", daqmx_error(status))
}

fn daqmx_error(status: ni_daqmx_sys::int32) -> String {
    let mut buffer = vec![0 as c_char; 2048];
    let result = unsafe {
        ni_daqmx_sys::DAQmxGetErrorString(status, buffer.as_mut_ptr(), buffer.len() as _)
    };
    if result >= 0 {
        let message = unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .trim()
            .to_owned();
        if message.is_empty() {
            return format!("DAQmx error {status}");
        }
        return format!("DAQmx error {status}: {message}");
    }
    format!("DAQmx error {status}; DAQmxGetErrorString returned {result}")
}

fn print_usage() {
    eprintln!("usage: numanager-daqmx-io-smoke-helper --kind ao|do|ai|ci|co --channel PHYSICAL_CHANNEL [options] [--execute --bench-safety-reviewed]");
    eprintln!(
        "without --execute, prints the planned NI-DAQmx calls and performs no DAQmx task calls"
    );
    eprintln!(
        "with --execute --bench-safety-reviewed, performs a minimal single-channel bench I/O operation and clears the task"
    );
    eprintln!("options: --min-volts V --max-volts V --volts V --line-state true|false");
    eprintln!("         --terminal default|differential|rse|nrse");
    eprintln!("         --edge rising|falling --direction up|down");
    eprintln!("         --idle-state low|high --frequency HZ --duty-cycle FRACTION");
    eprintln!("         --samples N --timeout SECONDS --name NAME --unnamed");
    eprintln!("         --simulate-error-after-start");
    eprintln!("         --bench-safety-reviewed");
}
