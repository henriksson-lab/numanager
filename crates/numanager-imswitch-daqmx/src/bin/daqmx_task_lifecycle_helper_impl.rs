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
    let mut task_name = Some("numanager-daqmx-lifecycle-probe".to_owned());
    let mut start = false;
    let mut wait_seconds = None;
    let mut dry_run = false;
    let mut simulate_error_after_start = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--name" => {
                task_name = Some(
                    args.next()
                        .ok_or_else(|| "--name requires a value".to_owned())?,
                );
            }
            "--unnamed" => task_name = None,
            "--start" => start = true,
            "--dry-run" => dry_run = true,
            "--simulate-error-after-start" => simulate_error_after_start = true,
            "--wait-seconds" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--wait-seconds requires a value".to_owned())?;
                let value = value
                    .parse::<f64>()
                    .map_err(|error| format!("invalid --wait-seconds value: {error}"))?;
                if !value.is_finite() || value < 0.0 {
                    return Err("--wait-seconds must be finite and non-negative".into());
                }
                wait_seconds = Some(value);
            }
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }

    if simulate_error_after_start && !dry_run {
        return Err("--simulate-error-after-start requires --dry-run".into());
    }
    if simulate_error_after_start && !start {
        return Err("--simulate-error-after-start requires --start".into());
    }
    validate_optional_task_name(task_name.as_deref())?;

    if dry_run {
        print_dry_run(
            task_name.as_deref(),
            start,
            wait_seconds,
            simulate_error_after_start,
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

    let mut clear_status = None;
    let mut started = false;
    let result: Result<(), String> = (|| {
        if start {
            check_status(
                unsafe { ni_daqmx_sys::DAQmxStartTask(handle) },
                "DAQmxStartTask",
            )?;
            started = true;
            println!("started_task\ttrue");
        }
        if let Some(wait_seconds) = wait_seconds {
            check_status(
                unsafe { ni_daqmx_sys::DAQmxWaitUntilTaskDone(handle, wait_seconds) },
                "DAQmxWaitUntilTaskDone",
            )?;
            println!("wait_until_done\ttrue");
        }
        if start {
            check_status(
                unsafe { ni_daqmx_sys::DAQmxStopTask(handle) },
                "DAQmxStopTask",
            )?;
            started = false;
            println!("stopped_task\ttrue");
        }
        Ok(())
    })();

    let stop_after_error = if result.is_err() && started {
        println!("cleanup_after_lifecycle_error\ttrue");
        match check_status(
            unsafe { ni_daqmx_sys::DAQmxStopTask(handle) },
            "DAQmxStopTask",
        ) {
            Ok(()) => {
                println!("stopped_task_after_error\ttrue");
                None
            }
            Err(error) => Some(error),
        }
    } else {
        None
    };

    let status = unsafe { ni_daqmx_sys::DAQmxClearTask(handle) };
    if status < 0 {
        clear_status = Some(format_status(status, "DAQmxClearTask"));
    } else {
        println!("cleared_task\ttrue");
    }

    match (result, stop_after_error, clear_status) {
        (Ok(()), _, None) => Ok(()),
        (Ok(()), _, Some(clear_error)) => Err(clear_error),
        (Err(error), None, None) => Err(error),
        (Err(error), Some(stop_error), None) => {
            Err(format!("{error}; cleanup stop after lifecycle error failed: {stop_error}"))
        }
        (Err(error), None, Some(clear_error)) => {
            Err(format!("{error}; cleanup clear after lifecycle error failed: {clear_error}"))
        }
        (Err(error), Some(stop_error), Some(clear_error)) => Err(format!(
            "{error}; cleanup stop after lifecycle error failed: {stop_error}; cleanup clear after lifecycle error failed: {clear_error}"
        )),
    }
}

fn print_usage() {
    eprintln!(
        "usage: numanager-daqmx-task-lifecycle-helper [--name NAME|--unnamed] [--start] [--wait-seconds SECONDS] [--dry-run] [--simulate-error-after-start]"
    );
    eprintln!("default behavior creates and clears an empty task without starting it");
    eprintln!("--dry-run prints the planned lifecycle calls without creating a task");
    eprintln!(
        "--simulate-error-after-start requires --dry-run --start and prints no-DAQmx cleanup-log rows"
    );
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

fn print_dry_run(
    task_name: Option<&str>,
    start: bool,
    wait_seconds: Option<f64>,
    simulate_error_after_start: bool,
) {
    let mut calls = vec!["DAQmxCreateTask"];
    if start {
        calls.push("DAQmxStartTask");
    }
    if wait_seconds.is_some() {
        calls.push("DAQmxWaitUntilTaskDone");
    }
    if start {
        calls.push("DAQmxStopTask");
    }
    calls.push("DAQmxClearTask");

    println!("task_lifecycle_plan\ttrue");
    println!("execute\tfalse");
    println!("task_name\t{}", task_name.unwrap_or("<unnamed>"));
    println!("start\t{start}");
    match wait_seconds {
        Some(value) => println!("wait_seconds\t{value:.6}"),
        None => println!("wait_seconds\t<none>"),
    }
    println!("planned_api\t{}", calls.join(","));
    println!("created_task\tfalse");
    println!("started_task\tfalse");
    println!("waited_until_done\tfalse");
    println!("stopped_task\tfalse");
    if simulate_error_after_start {
        println!("simulated_failure\ttrue");
        println!("simulated_error_message\tsimulated lifecycle error after task start");
        println!("cleanup_after_lifecycle_error\ttrue");
        println!("stopped_task_after_error\tsimulated_no_task");
    }
    println!("cleared_task\tfalse");
}

fn optional_cstring(value: Option<&str>) -> Result<Option<CString>, String> {
    value
        .map(|value| {
            CString::new(value).map_err(|_| "task name contains an interior NUL byte".to_owned())
        })
        .transpose()
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
