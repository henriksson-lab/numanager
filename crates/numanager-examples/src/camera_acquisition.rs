use numanager_core::runtime::{LocalRuntime, Runtime};
use numanager_core::*;
use numanager_drivers::genicam::GenicamDriver;
use numanager_drivers::gige_vision::GigEVisionDriver;
use numanager_drivers::platform_camera::{PlatformCameraBackend, PlatformCameraDriver};
use numanager_drivers::toupcam::ToupcamDriver;
use numanager_drivers::usb3_vision::Usb3VisionDriver;
use numanager_examples::{
    capability_brief, completion_summary, device_label_list, operation_status_summary, property,
    public_kind_tags, push_schema_sequence, push_schema_write, schema_numeric_pair,
    schema_numeric_value,
};
use std::time::Duration;

pub fn run() -> numanager_core::Result<()> {
    let source = numanager_examples::example_arg(0).unwrap_or_else(|| "toupcam".into());
    let driver = camera_driver(&source)?;
    let driver_id = driver.id();

    let mut runtime = LocalRuntime::new();
    let arrivals = runtime.subscribe(EventFilter::kind(EventKind::DeviceArrived));
    let added_devices = runtime.add_driver(driver)?;
    println!("added driver with {} device(s)", added_devices.len());
    if let Some(Event::DeviceArrived(device)) = arrivals.recv_timeout(Duration::from_secs(1)) {
        println!("device arrived: {}", device.label);
    }
    let camera = runtime
        .device_by_capability(CapabilityKind::CameraCapture)?
        .clone();

    println!("source: {source}");
    println!("camera: {} {:?}", camera.label, public_kind_tags(&camera));

    let capture_capability = runtime.capability_by_kind(&camera, CapabilityKind::CameraCapture)?;
    let trigger_capability = runtime
        .capabilities(&camera)?
        .iter()
        .find(|capability| capability.kind == CapabilityKind::TriggerSink)
        .cloned();
    println!(
        "capture capability: {} ({})",
        capability_brief(&capture_capability),
        capture_capability.name
    );
    if let Some(capability) = &trigger_capability {
        println!(
            "trigger capability: {} ({})",
            capability_brief(capability),
            capability.name
        );
    }
    for property in camera
        .properties
        .iter()
        .filter(|property| user_facing_camera_property(&property.key))
    {
        println!(
            "property: {} type={:?} writable={} sequenceable={}",
            property.key, property.value_type, property.writable, property.sequenceable
        );
    }

    let frames = runtime.subscribe(EventFilter::device(&camera).with_kind(EventKind::FrameReady));

    let setup_writes = camera_setup_writes(&camera)?;
    if !setup_writes.is_empty() {
        let setup = runtime.submit(
            StateSet::immediate("camera capture controls")
                .with_writes(setup_writes)
                .into_command(),
        )?;
        let value = runtime.wait_completed(setup.id, Duration::from_secs(1))?;
        println!("camera setup completed: {}", completion_summary(&value));
    }

    if trigger_capability.is_some() {
        let trigger_mode = trigger_mode_write(&camera);
        if let Some(write) = trigger_mode {
            let mode = runtime.submit(
                StateSet::immediate("camera trigger mode")
                    .with_writes([write])
                    .into_command(),
            )?;
            let value = runtime.wait_completed(mode.id, Duration::from_secs(1))?;
            println!("trigger mode completed: {}", completion_summary(&value));
        }
        let pulse = runtime.submit_capability(
            &camera,
            CapabilityKind::TriggerSink,
            CapabilityRequest::None,
        )?;
        let value = runtime.wait_completed(pulse.id, Duration::from_secs(1))?;
        println!("trigger pulse completed: {}", completion_summary(&value));
    }

    let plan_sequences = camera_timing_sequences(&camera);
    if !plan_sequences.is_empty() {
        let plan = TimingPlan::from_parts(
            Vec::new(),
            plan_sequences,
            [&camera],
            StartCondition::Software,
            StopCondition::Manual,
        )?;
        let armed = runtime.submit(Command::arm(plan))?;
        let value = runtime.wait_completed(armed.id, Duration::from_secs(1))?;
        println!("timing arm completed: {}", completion_summary(&value));
        let started = runtime.submit(Command::start(armed.id))?;
        let value = runtime.wait_completed(started.id, Duration::from_secs(1))?;
        println!("timing start completed: {}", completion_summary(&value));
        let stopped = runtime.submit(Command::stop(armed.id))?;
        let value = runtime.wait_completed(stopped.id, Duration::from_secs(1))?;
        println!("timing stop completed: {}", completion_summary(&value));
    }

    let capture = runtime.submit_request(
        &camera,
        CameraCaptureRequest {
            encoding: Some(ImageEncoding::Mono8),
            buffer: Some(FrameBufferSpec {
                capacity_frames: 16,
                overflow: OverflowPolicy::DropOldest,
            }),
        },
    )?;
    let capture_status = runtime.subscribe(
        EventFilter::device(&camera)
            .with_operation(capture.id)
            .with_kind(EventKind::OperationChanged),
    );
    let value = runtime.wait_completed(capture.id, Duration::from_secs(5))?;
    let captured = CapturedFrame::from_completion(&value)?;
    println!("capture completed: {}", completion_summary(&value));
    println!(
        "capture frame handle: {:?} size={:?}x{:?} format={:?}",
        captured.handle, captured.width, captured.height, captured.pixel_format
    );
    while let Some(Event::OperationChanged(change)) =
        capture_status.recv_timeout(Duration::from_millis(100))
    {
        println!(
            "device+operation-filtered status: {:?} devices=[{}] -> {}",
            change.operation,
            device_label_list(std::slice::from_ref(&camera), &change.devices),
            operation_status_summary(&change.status)
        );
        if matches!(change.status, OperationStatus::Completed(_)) {
            break;
        }
    }

    match frames.recv_timeout(Duration::from_secs(1)) {
        Some(Event::FrameReady(event)) => {
            let frame = runtime
                .frame(event.handle)?
                .expect("frame event should reference a buffered frame");
            println!(
                "frame: {}x{} {} bytes format={} metadata keys=[{}]",
                event.width,
                event.height,
                frame.data.len(),
                event.pixel_format,
                public_metadata_keys(&event.metadata).join(", ")
            );
        }
        _ => println!("capture completed, but no frame event was received"),
    }

    for key in readable_camera_keys(&camera) {
        println!(
            "{key} readback: {:?}",
            read_property(&runtime, &camera, key)?
        );
    }

    let removed = runtime.remove_driver(driver_id)?;
    println!("removed driver with {} device(s)", removed.len());

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

fn camera_setup_writes(camera: &DeviceDescriptor) -> numanager_core::Result<Vec<StateWrite>> {
    let mut writes = Vec::new();
    push_schema_write(
        camera,
        &mut writes,
        "exposure",
        Value::TimeInterval(TimeInterval::from_milliseconds(20.0)),
    );
    push_schema_write(
        camera,
        &mut writes,
        "gain",
        schema_numeric_value(camera, "gain", 12.0).unwrap_or(Value::Null),
    );
    push_schema_write(
        camera,
        &mut writes,
        "width",
        schema_numeric_value(camera, "width", 640.0).unwrap_or(Value::Null),
    );
    push_schema_write(
        camera,
        &mut writes,
        "height",
        schema_numeric_value(camera, "height", 480.0).unwrap_or(Value::Null),
    );
    if let Some(value) = pixel_format_value(camera, "pixel_format") {
        push_schema_write(camera, &mut writes, "pixel_format", value);
    }
    push_schema_write(
        camera,
        &mut writes,
        "frame_interval",
        Value::TimeInterval(TimeInterval::from_milliseconds(33.0)),
    );
    Ok(writes)
}

fn camera_timing_sequences(camera: &DeviceDescriptor) -> Vec<DeviceSequence> {
    let mut sequences = Vec::new();
    push_schema_sequence(
        camera,
        &mut sequences,
        "exposure",
        vec![
            Value::TimeInterval(TimeInterval::from_milliseconds(15.0)),
            Value::TimeInterval(TimeInterval::from_milliseconds(25.0)),
        ],
    );
    push_schema_sequence(
        camera,
        &mut sequences,
        "gain",
        schema_numeric_pair(camera, "gain", 14.0, 10.0),
    );
    if let Some(value) = pixel_format_value(camera, "pixel_format") {
        push_schema_sequence(
            camera,
            &mut sequences,
            "pixel_format",
            vec![value.clone(), value],
        );
    }
    sequences
}

fn trigger_mode_write(camera: &DeviceDescriptor) -> Option<StateWrite> {
    property(camera, "trigger_mode").and_then(|schema| {
        enum_string(schema, &["external", "External", "On"])
            .map(|mode| StateWrite::new(camera, "trigger_mode", Value::String(mode)))
    })
}

fn pixel_format_value(camera: &DeviceDescriptor, key: &str) -> Option<Value> {
    property(camera, key)
        .and_then(|schema| enum_string(schema, &["Mono8", "Native"]))
        .map(Value::String)
}

fn enum_string(schema: &PropertySchema, preferred: &[&str]) -> Option<String> {
    for wanted in preferred {
        if schema
            .enum_values
            .iter()
            .any(|entry| entry.value == Value::String((*wanted).into()))
        {
            return Some((*wanted).into());
        }
    }
    schema
        .enum_values
        .iter()
        .find_map(|entry| match &entry.value {
            Value::String(value) => Some(value.clone()),
            _ => None,
        })
}

fn readable_camera_keys(camera: &DeviceDescriptor) -> Vec<&str> {
    camera
        .properties
        .iter()
        .filter(|property| property.readable && user_facing_camera_property(&property.key))
        .map(|property| property.key.as_str())
        .collect()
}

fn user_facing_camera_property(key: &str) -> bool {
    matches!(
        key,
        "exposure"
            | "gain"
            | "pixel_format"
            | "width"
            | "height"
            | "frame_interval"
            | "trigger_mode"
            | "roi_width"
            | "roi_height"
            | "binning"
            | "black_level"
            | "white_balance_red"
            | "white_balance_blue"
            | "sensor_temperature"
            | "supported_pixel_formats"
    )
}

fn public_metadata_keys(metadata: &std::collections::BTreeMap<String, Value>) -> Vec<String> {
    metadata
        .keys()
        .filter(|key| user_facing_camera_property(key) || user_facing_frame_metadata(key))
        .cloned()
        .collect()
}

fn user_facing_frame_metadata(key: &str) -> bool {
    matches!(
        key,
        "source"
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
    )
}

fn read_property(
    runtime: &LocalRuntime,
    device: impl Into<DeviceId>,
    key: &str,
) -> numanager_core::Result<Value> {
    runtime.execute(Command::read_property(device, key), Duration::from_secs(1))
}
