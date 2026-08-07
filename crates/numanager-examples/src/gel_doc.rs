//! Bio-Rad Gel Doc EZ (Lumenera Lu130) bring-up.
//!
//! The camera is a two-stage EZ-USB device: cold it enumerates as a firmware
//! loader, and only renumerates into the imaging stage once its 8051 firmware
//! has been pushed. This example shows both stages through the public runtime
//! API, including a real acquisition on `capture`.
//!
//! Without a live USB session the capture path reports that rather than
//! inventing a frame; `gain` stays refused in every mode because its register
//! mapping is unevidenced.
//!
//! ```sh
//! cargo run -p numanager-examples -- gel_doc                       # configured
//! cargo run -p numanager-examples --features os-usb -- gel_doc live
//! cargo run -p numanager-examples --features os-usb -- gel_doc initialize-firmware
//! cargo run -p numanager-examples --features os-usb -- gel_doc capture [exposure_ms] [out.raw]
//! ```
//!
//! `capture` reports the frame's size and how much of it is non-zero. Give it a
//! third argument to write the pixels somewhere — without one it writes nothing,
//! because running an example should not leave megabytes in the working
//! directory. The result is `Raw16` little-endian at the reported dimensions.
//!
//! `initialize-firmware` is the only mode that writes to hardware. On Windows
//! the loader node must be bound to WinUSB first; a failed interface claim says
//! so.

use numanager_core::config::{HardwareConfig, HardwareConfigBuilder};
use numanager_core::runtime::{DiscoveryRegistry, LocalRuntime, Runtime};
use numanager_core::*;
use numanager_drivers::register_builtin_discovery;
use numanager_examples::{example_arg, is_public_property, public_kind_summary};
use std::collections::BTreeMap;
use std::time::Duration;

/// The Gel Doc EZ in its cold, pre-firmware loader stage.
const LOADER_PID: i64 = 0x809A;

pub fn run() -> Result<()> {
    let mode = example_arg(0).unwrap_or_else(|| "configured".into());
    match mode.as_str() {
        "configured" => configured(),
        "live" => live(false),
        "initialize-firmware" => live(true),
        "capture" => capture(),
        other => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!(
                "unknown gel_doc mode {other}; expected configured, live, initialize-firmware, or capture"
            ),
        )),
    }
}

/// Declarative topology: what a `hardware.toml` entry for this camera looks
/// like, and what the runtime reports back for it.
fn configured() -> Result<()> {
    let config = gel_doc_config();
    println!("configured topology:");
    print!("{}", config.to_toml());

    let mut registry = DiscoveryRegistry::new();
    register_builtin_discovery(&mut registry, &config)?;
    let candidates = registry.detect_all()?;
    println!("detected {} candidate driver(s)", candidates.len());

    let mut runtime = LocalRuntime::new();
    let mut devices = Vec::new();
    for candidate in candidates {
        println!("candidate: {}", candidate.label());
        devices.extend(runtime.add_candidate(candidate)?);
    }

    for device in &devices {
        report_device(&runtime, device)?;
    }
    Ok(())
}

/// Live USB enumeration. Without `initialize`, this only reads descriptors —
/// nothing is written to the camera.
fn live(initialize: bool) -> Result<()> {
    #[cfg(not(feature = "os-usb"))]
    {
        let _ = initialize;
        Err(Error::new(
            ErrorCode::Unsupported,
            "gel_doc live modes require --features os-usb",
        ))
    }

    #[cfg(feature = "os-usb")]
    {
        use numanager_drivers::lumenera::LumeneraDiscovery;

        let mut discovery = LumeneraDiscovery::os_usb(DriverId(4200));
        if initialize {
            println!("firmware initialization enabled: loader-stage units will be brought up");
            discovery = discovery.with_firmware_initialization(None);
        } else {
            println!("read-only enumeration: no firmware is pushed");
        }

        let mut registry = DiscoveryRegistry::new();
        registry.register(discovery);
        let candidates = registry.detect_all()?;
        if candidates.is_empty() {
            println!("no Lumenera Gel Doc EZ device found on USB");
            return Ok(());
        }

        let mut runtime = LocalRuntime::new();
        let mut devices = Vec::new();
        for candidate in candidates {
            println!("candidate: {}", candidate.label());
            devices.extend(runtime.add_candidate(candidate)?);
        }
        for device in &devices {
            report_device(&runtime, device)?;
        }
        Ok(())
    }
}

/// Set an exposure and take one frame off a live camera.
fn capture() -> Result<()> {
    let exposure = TimeInterval::from_milliseconds(
        example_arg(1)
            .and_then(|arg| arg.parse::<f64>().ok())
            .unwrap_or(90.0),
    );

    #[cfg(not(feature = "os-usb"))]
    {
        let _ = exposure;
        Err(Error::new(
            ErrorCode::Unsupported,
            "gel_doc capture requires --features os-usb",
        ))
    }

    #[cfg(feature = "os-usb")]
    {
        use numanager_drivers::lumenera::LumeneraDiscovery;

        let mut registry = DiscoveryRegistry::new();
        registry
            .register(LumeneraDiscovery::os_usb(DriverId(4200)).with_firmware_initialization(None));
        let candidates = registry.detect_all()?;

        let mut runtime = LocalRuntime::new();
        let mut devices = Vec::new();
        for candidate in candidates {
            devices.extend(runtime.add_candidate(candidate)?);
        }
        let Some(camera) = devices
            .iter()
            .find(|device| device.kinds.iter().any(|kind| kind == "camera"))
        else {
            println!("no Lumenera Gel Doc EZ camera found on USB");
            return Ok(());
        };
        println!("camera: {}", camera.label);

        // Exposure is programmed as part of the acquisition sequence, so this
        // takes effect on the next capture rather than reaching the wire now.
        runtime.execute(
            Command::write_property(camera.id, "exposure", Value::TimeInterval(exposure)),
            Duration::from_secs(2),
        )?;
        println!("exposure set to {exposure:?}");

        // A capture blocks for the exposure plus readout, so allow for both.
        let timeout = Duration::from_secs_f64(exposure.seconds() + 20.0);
        let result = runtime.execute_request(
            camera.id,
            CapabilityRequest::CameraCapture(CameraCaptureRequest::default_frame()),
            timeout,
        )?;
        println!("capture: {}", summarize(&result));

        // The response carries a frame handle; the pixels live in the runtime's
        // frame store. Report their shape always, but only write a file when the
        // caller asked for one — an example should not drop megabytes into the
        // working directory as a side effect of being run.
        if let Value::Map(fields) = &result {
            if let (Some(Value::I64(frame_id)), Some(Value::I64(stream_id))) =
                (fields.get("frame"), fields.get("stream"))
            {
                let handle = FrameHandle {
                    stream: StreamId(*stream_id as u64),
                    frame: FrameId(*frame_id as u64),
                };
                if let Some(frame) = runtime.frame(handle)? {
                    let nonzero = frame.data.iter().filter(|byte| **byte != 0).count();
                    println!(
                        "frame: {} bytes, {:.1}% non-zero",
                        frame.data.len(),
                        100.0 * nonzero as f64 / frame.data.len().max(1) as f64
                    );
                    match example_arg(2) {
                        Some(path) => {
                            std::fs::write(&path, &frame.data).map_err(|error| {
                                Error::new(ErrorCode::Driver, format!("writing {path}: {error}"))
                            })?;
                            println!("wrote {path}");
                        }
                        None => println!(
                            "pass a third argument to write the pixels, \
                             e.g. `gel_doc capture 100 frame.raw`"
                        ),
                    }
                }
            }
        }
        Ok(())
    }
}

/// A `hardware.toml`-shaped declaration of one cold Gel Doc EZ.
fn gel_doc_config() -> HardwareConfig {
    let mut builder: HardwareConfigBuilder = HardwareConfig::builder_from(4200);
    builder.add_device(
        "Gel Doc EZ camera",
        "geldoc_ez",
        BTreeMap::from([
            ("product_id".into(), Value::I64(LOADER_PID)),
            (
                "firmware_dir".into(),
                Value::String("data/third_party/lumenera".into()),
            ),
            // The gate that authorizes writing firmware to the device. Left off
            // here so the example stays read-only; `initialize-firmware` is the
            // mode that turns it on.
            ("connect".into(), Value::Bool(false)),
        ]),
    );
    builder.build()
}

/// Read every public property, then show the capture path refusing.
fn report_device(runtime: &LocalRuntime, device: &DeviceDescriptor) -> Result<()> {
    println!("device: {} {}", device.label, public_kind_summary(device));

    for schema in device.properties.iter().filter(|p| is_public_property(p)) {
        match runtime.execute(
            Command::read_property(device.id, &schema.key),
            Duration::from_secs(1),
        ) {
            Ok(value) => println!("  {} = {}", schema.key, summarize(&value)),
            Err(error) => println!("  {} unavailable: {}", schema.key, error.message),
        }
    }

    // The device advertises CameraCapture because the hardware really has it.
    // Invoking it reports the missing evidence instead of fabricating a frame.
    match runtime.execute_request(
        device.id,
        CapabilityRequest::CameraCapture(CameraCaptureRequest::default_frame()),
        Duration::from_secs(1),
    ) {
        Ok(_) => println!("  capture: returned a frame"),
        Err(error) => println!("  capture refused: {}", error.message),
    }
    Ok(())
}

fn summarize(value: &Value) -> String {
    match value {
        Value::String(text) if text.len() > 72 => format!("{}…", &text[..72]),
        Value::Map(fields) => {
            let mut keys = fields.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            format!("map({})", keys.join(", "))
        }
        other => format!("{other:?}"),
    }
}
