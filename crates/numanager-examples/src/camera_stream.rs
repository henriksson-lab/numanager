use numanager_core::runtime::{LocalRuntime, Runtime, Subscription};
use numanager_core::*;
use numanager_drivers::genicam::GenicamDriver;
use numanager_drivers::gige_vision::GigEVisionDriver;
use numanager_drivers::platform_camera::{PlatformCameraBackend, PlatformCameraDriver};
use numanager_drivers::toupcam::ToupcamDriver;
use numanager_drivers::usb3_vision::Usb3VisionDriver;
use numanager_examples::{
    capability_brief, completion_summary, metadata_key_summary, public_kind_tags,
    push_schema_write, schema_numeric_value,
};
use std::time::Duration;

pub fn run() -> numanager_core::Result<()> {
    let source = numanager_examples::example_arg(0).unwrap_or_else(|| "toupcam".into());
    let driver = camera_driver(&source)?;

    let mut runtime = LocalRuntime::new();
    runtime.add_driver(driver)?;
    let camera = runtime
        .device_by_capability(CapabilityKind::CameraStream)?
        .clone();

    println!("source: {source}");
    println!("camera: {} {:?}", camera.label, public_kind_tags(&camera));
    let stream_capability = runtime.capability_by_kind(&camera, CapabilityKind::CameraStream)?;
    println!(
        "stream capability: {} ({})",
        capability_brief(&stream_capability),
        stream_capability.name
    );

    let setup_writes = camera_stream_setup_writes(&camera);
    if !setup_writes.is_empty() {
        let result = runtime.execute(
            StateSet::immediate("camera stream controls")
                .with_writes(setup_writes)
                .into_command(),
            Duration::from_secs(5),
        )?;
        println!("stream setup completed: {}", completion_summary(&result));
    }

    let events = runtime.subscribe(EventFilter::device(&camera).with_kinds([
        EventKind::FrameReady,
        EventKind::Telemetry,
        EventKind::Fault,
    ]));

    for overflow in [
        OverflowPolicy::DropOldest,
        OverflowPolicy::DropNewest,
        OverflowPolicy::Error,
    ] {
        run_stream_policy(&runtime, &events, &camera, overflow)?;
    }

    Ok(())
}

fn camera_driver(source: &str) -> numanager_core::Result<Box<dyn Driver>> {
    numanager_examples::boxed_driver(|id| match source {
        "toupcam" => Ok(Box::new(ToupcamDriver::simulated(id))),
        "toupcam-live" | "toupcam-usb" => toupcam_live_driver(id),
        "platform" | "platform-fixture" => Ok(Box::new(PlatformCameraDriver::simulated(
            id,
            PlatformCameraBackend::Fixture,
        ))),
        "gige" | "gige-vision" => Ok(Box::new(GigEVisionDriver::simulated(id))),
        "usb3" | "usb3-vision" => Ok(Box::new(Usb3VisionDriver::simulated(id))),
        "genicam" => Ok(Box::new(GenicamDriver::configured_fixture(id))),
        other => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("unknown camera source {other}"),
        )),
    })
}

#[cfg(feature = "os-usb")]
fn toupcam_live_driver(driver_id: DriverId) -> numanager_core::Result<Box<dyn Driver>> {
    Ok(Box::new(ToupcamDriver::open_first_usb(driver_id)?))
}

#[cfg(not(feature = "os-usb"))]
fn toupcam_live_driver(_driver_id: DriverId) -> numanager_core::Result<Box<dyn Driver>> {
    Err(Error::new(
        ErrorCode::Unsupported,
        "toupcam-live requires numanager-examples --features os-usb",
    ))
}

fn run_stream_policy(
    runtime: &LocalRuntime,
    events: &Subscription,
    camera: impl Into<DeviceId>,
    overflow: OverflowPolicy,
) -> numanager_core::Result<()> {
    let camera = camera.into();
    let result = runtime.execute_request(
        camera,
        CameraStreamRequest {
            encoding: Some(ImageEncoding::Mono8),
            frame_count: Some(6),
            buffer: FrameBufferSpec {
                capacity_frames: 4,
                overflow: overflow.clone(),
            },
        },
        Duration::from_secs(15),
    )?;
    let stream = CameraStreamStarted::from_completion(&result)?;
    println!(
        "{} stream completed: {} stream={:?} frames={:?} size={} format={:?}",
        frame_overflow_policy_name(&overflow),
        completion_summary(&result),
        stream.stream,
        stream.frame_count,
        stream_geometry(&stream),
        stream.pixel_format
    );

    let mut seen_frames = 0;
    let mut seen_drop_reports = 0;
    let mut seen_faults = 0;
    while let Some(event) = events.recv_timeout(Duration::from_millis(100)) {
        match event {
            Event::FrameReady(event) => {
                seen_frames += 1;
                match runtime.frame(event.handle)? {
                    Some(frame) => println!(
                        "{} frame {:?}: {}x{} {} bytes metadata keys=[{}]",
                        frame_overflow_policy_name(&overflow),
                        event.handle,
                        event.width,
                        event.height,
                        frame.data.len(),
                        stream_metadata_key_summary(&event.metadata)
                    ),
                    None => println!(
                        "{} frame {:?}: not retained by ring buffer metadata keys=[{}]",
                        frame_overflow_policy_name(&overflow),
                        event.handle,
                        stream_metadata_key_summary(&event.metadata)
                    ),
                }
            }
            Event::Telemetry(event) => {
                seen_drop_reports += 1;
                println!(
                    "{} stream telemetry: keys=[{}]",
                    frame_overflow_policy_name(&overflow),
                    metadata_key_summary(&event.values)
                );
            }
            Event::Fault(event) => {
                seen_faults += 1;
                println!(
                    "{} stream fault: {:?}: {}",
                    frame_overflow_policy_name(&overflow),
                    event.report.code,
                    event.report.message
                );
            }
            _ => {}
        }
    }
    println!(
        "{} received {seen_frames} frame-ready event(s), {seen_drop_reports} drop telemetry event(s), and {seen_faults} fault event(s)",
        frame_overflow_policy_name(&overflow)
    );
    if let Some(status) = runtime.stream_status(stream.stream)? {
        println!(
            "{} stream status: depth={} capacity={} dropped={} latest={:?} {}",
            frame_overflow_policy_name(&overflow),
            status.depth(),
            status.capacity(),
            status.dropped_frames,
            status.latest(),
            completion_summary(&status.as_value())
        );
    }

    Ok(())
}

fn stream_geometry(stream: &CameraStreamStarted) -> String {
    match (stream.width, stream.height) {
        (Some(width), Some(height)) => format!("{width}x{height}"),
        _ => "unknown".into(),
    }
}

fn stream_metadata_key_summary(metadata: &std::collections::BTreeMap<String, Value>) -> String {
    metadata
        .keys()
        .filter(|key| {
            matches!(
                key.as_str(),
                "width"
                    | "height"
                    | "exposure"
                    | "frame_interval"
                    | "gain"
                    | "pixel_format"
                    | "source"
                    | "backend"
                    | "transport"
                    | "chunk_frame_id"
                    | "chunk_metadata"
                    | "hardware_timestamp"
                    | "payload_size"
                    | "packet_size"
                    | "transfer_size"
                    | "transfer_queue_depth"
                    | "stream_channel_port"
                    | "stream_endpoint"
                    | "inter_packet_delay"
                    | "gvsp_status"
                    | "u3v_status"
                    | "frame_rate"
                    | "line_time"
                    | "trigger_mode"
                    | "ring_capacity"
                    | "ring_depth"
                    | "dropped_frames"
                    | "overflow_policy"
            )
        })
        .cloned()
        .collect::<Vec<_>>()
        .join(", ")
}

fn camera_stream_setup_writes(camera: &DeviceDescriptor) -> Vec<StateWrite> {
    let mut writes = Vec::new();
    if let Some(value) = schema_numeric_value(camera, "width", 640.0) {
        push_schema_write(camera, &mut writes, "width", value);
    }
    if let Some(value) = schema_numeric_value(camera, "height", 480.0) {
        push_schema_write(camera, &mut writes, "height", value);
    }
    push_schema_write(
        camera,
        &mut writes,
        "exposure",
        Value::TimeInterval(TimeInterval::from_milliseconds(20.0)),
    );
    if let Some(value) = pixel_format_value(camera, "pixel_format") {
        push_schema_write(camera, &mut writes, "pixel_format", value);
    }
    writes
}

fn pixel_format_value(camera: &DeviceDescriptor, key: &str) -> Option<Value> {
    camera
        .properties
        .iter()
        .find(|property| property.key == key)
        .and_then(|schema| {
            for wanted in ["Mono8", "Native"] {
                if schema
                    .enum_values
                    .iter()
                    .any(|entry| entry.value == Value::String(wanted.into()))
                {
                    return Some(Value::String(wanted.into()));
                }
            }
            schema
                .enum_values
                .iter()
                .find_map(|entry| match &entry.value {
                    Value::String(value) => Some(Value::String(value.clone())),
                    _ => None,
                })
        })
}
