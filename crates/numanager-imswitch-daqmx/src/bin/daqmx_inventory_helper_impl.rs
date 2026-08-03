use std::ffi::{CStr, CString};
use std::io::Write;
use std::os::raw::c_char;
use std::process::ExitCode;

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
    let mut configured_device = None;
    let mut include_version = false;
    let mut version_only = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--device" => configured_device = args.next(),
            "--include-version" => include_version = true,
            "--version-only" => {
                include_version = true;
                version_only = true;
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }
    let configured_device = configured_device.unwrap_or_else(|| "Dev1".into());

    if include_version {
        let major = get_version_component(ni_daqmx_sys::DAQmxGetSysNIDAQMajorVersion)?;
        let minor = get_version_component(ni_daqmx_sys::DAQmxGetSysNIDAQMinorVersion)?;
        let update = get_version_component(ni_daqmx_sys::DAQmxGetSysNIDAQUpdateVersion)?;
        println!("runtime_version\t{major}.{minor}.{update}");
        println!("runtime_version_major\t{major}");
        println!("runtime_version_minor\t{minor}");
        println!("runtime_version_update\t{update}");
        std::io::stdout()
            .flush()
            .map_err(|error| format!("failed to flush runtime version: {error}"))?;
        if version_only {
            return Ok(());
        }
    }

    let devices = split_daqmx_list(&query_string(ni_daqmx_sys::DAQmxGetSysDevNames)?);
    println!("devices\t{}", devices.join(", "));

    if devices.iter().any(|device| device == &configured_device) {
        match probe_device(&configured_device) {
            Ok(device) => print_device(device),
            Err(error) => println!("configured_device_error\t{error}"),
        }
    }
    Ok(())
}

fn get_version_component(
    getter: unsafe extern "C" fn(*mut ni_daqmx_sys::uInt32) -> ni_daqmx_sys::int32,
) -> Result<ni_daqmx_sys::uInt32, String> {
    let mut value = 0;
    let status = unsafe { getter(&mut value) };
    if status < 0 {
        return Err(error_string(status));
    }
    Ok(value)
}

struct DeviceInventory {
    name: String,
    product_type: Option<String>,
    serial_number: Option<u32>,
    analog_inputs: Vec<String>,
    analog_outputs: Vec<String>,
    digital_inputs: Vec<String>,
    digital_outputs: Vec<String>,
    counter_inputs: Vec<String>,
    counter_outputs: Vec<String>,
}

fn print_device(device: DeviceInventory) {
    println!("configured_device\t{}", device.name);
    if let Some(product_type) = device.product_type {
        println!("product_type\t{product_type}");
    }
    if let Some(serial_number) = device.serial_number {
        println!("serial_number\t{serial_number}");
    }
    println!("analog_inputs\t{}", device.analog_inputs.join(", "));
    println!("analog_outputs\t{}", device.analog_outputs.join(", "));
    println!("digital_inputs\t{}", device.digital_inputs.join(", "));
    println!("digital_outputs\t{}", device.digital_outputs.join(", "));
    println!("counter_inputs\t{}", device.counter_inputs.join(", "));
    println!("counter_outputs\t{}", device.counter_outputs.join(", "));
}

fn probe_device(device_name: &str) -> Result<DeviceInventory, String> {
    let device = CString::new(device_name)
        .map_err(|_| format!("device name contains an interior NUL: {device_name:?}"))?;
    Ok(DeviceInventory {
        name: device_name.into(),
        product_type: query_device_string(&device, ni_daqmx_sys::DAQmxGetDevProductType).ok(),
        serial_number: query_device_u32(&device, ni_daqmx_sys::DAQmxGetDevSerialNum).ok(),
        analog_inputs: query_device_string(&device, ni_daqmx_sys::DAQmxGetDevAIPhysicalChans)
            .map(|value| split_daqmx_list(&value))
            .unwrap_or_default(),
        analog_outputs: query_device_string(&device, ni_daqmx_sys::DAQmxGetDevAOPhysicalChans)
            .map(|value| split_daqmx_list(&value))
            .unwrap_or_default(),
        digital_inputs: query_device_string(&device, ni_daqmx_sys::DAQmxGetDevDILines)
            .map(|value| split_daqmx_list(&value))
            .unwrap_or_default(),
        digital_outputs: query_device_string(&device, ni_daqmx_sys::DAQmxGetDevDOLines)
            .map(|value| split_daqmx_list(&value))
            .unwrap_or_default(),
        counter_inputs: query_device_string(&device, ni_daqmx_sys::DAQmxGetDevCIPhysicalChans)
            .map(|value| split_daqmx_list(&value))
            .unwrap_or_default(),
        counter_outputs: query_device_string(&device, ni_daqmx_sys::DAQmxGetDevCOPhysicalChans)
            .map(|value| split_daqmx_list(&value))
            .unwrap_or_default(),
    })
}

fn query_string(
    getter: unsafe extern "C" fn(*mut c_char, ni_daqmx_sys::uInt32) -> ni_daqmx_sys::int32,
) -> Result<String, String> {
    let mut buffer = vec![0 as c_char; 16_384];
    let status = unsafe { getter(buffer.as_mut_ptr(), buffer.len() as ni_daqmx_sys::uInt32) };
    if status < 0 {
        return Err(error_string(status));
    }
    Ok(unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned())
}

fn query_device_string(
    device: &CStr,
    getter: unsafe extern "C" fn(
        *const c_char,
        *mut c_char,
        ni_daqmx_sys::uInt32,
    ) -> ni_daqmx_sys::int32,
) -> Result<String, String> {
    let mut buffer = vec![0 as c_char; 16_384];
    let status = unsafe {
        getter(
            device.as_ptr(),
            buffer.as_mut_ptr(),
            buffer.len() as ni_daqmx_sys::uInt32,
        )
    };
    if status < 0 {
        return Err(error_string(status));
    }
    Ok(unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned())
}

fn query_device_u32(
    device: &CStr,
    getter: unsafe extern "C" fn(*const c_char, *mut ni_daqmx_sys::uInt32) -> ni_daqmx_sys::int32,
) -> Result<ni_daqmx_sys::uInt32, String> {
    let mut value = 0;
    let status = unsafe { getter(device.as_ptr(), &mut value) };
    if status < 0 {
        return Err(error_string(status));
    }
    Ok(value)
}

fn split_daqmx_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn error_string(status: ni_daqmx_sys::int32) -> String {
    let mut buffer = vec![0 as c_char; 2048];
    let result = unsafe {
        ni_daqmx_sys::DAQmxGetErrorString(
            status,
            buffer.as_mut_ptr(),
            buffer.len() as ni_daqmx_sys::uInt32,
        )
    };
    if result < 0 {
        return format!("DAQmx error {status}; DAQmxGetErrorString returned {result}");
    }
    unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}
