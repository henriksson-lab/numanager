use std::process::ExitCode;

#[cfg(any(target_os = "linux", target_os = "windows"))]
#[path = "daqmx_io_smoke_helper_impl.rs"]
mod supported;

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn main() -> ExitCode {
    supported::main()
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn main() -> ExitCode {
    eprintln!("numanager-daqmx-io-smoke-helper requires a Linux or Windows NI-DAQmx SDK target");
    ExitCode::FAILURE
}
