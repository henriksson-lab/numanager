use numanager_core::runtime::{LocalRuntime, Runtime};
use numanager_core::*;
use numanager_drivers::asi::AsiMs2000Driver;
use numanager_drivers::coolled::CoolLedPe300Driver;
use numanager_drivers::platform_camera::{PlatformCameraBackend, PlatformCameraDriver};
use numanager_examples::{completion_summary, event_summary, public_kind_summary};
use std::time::Duration;

pub fn run() -> numanager_core::Result<()> {
    let mut runtime = LocalRuntime::new();

    runtime.add_driver_factory(|id| {
        PlatformCameraDriver::simulated(id, PlatformCameraBackend::V4l2)
    })?;
    runtime.add_driver_factory(AsiMs2000Driver::simulated)?;
    runtime.add_driver_factory(CoolLedPe300Driver::simulated)?;

    let camera = runtime
        .device_by_capability(CapabilityKind::CameraStream)?
        .clone();
    let xy = runtime.device_by_kind("stage.xy")?.clone();
    let z = runtime.device_by_kind("stage.z")?.clone();
    let light_hub = runtime.device_by_kinds(&["hub", "light.engine"])?.clone();
    let channel_a = runtime
        .device_by_kinds(&["light.source", "trigger.sink"])?
        .clone();

    let participant_descriptors = vec![
        camera.clone(),
        xy.clone(),
        z.clone(),
        light_hub.clone(),
        channel_a.clone(),
    ];
    let operation_events = runtime.subscribe(
        EventFilter::devices([&camera, &xy, &z, &light_hub, &channel_a])
            .with_kinds([EventKind::OperationChanged, EventKind::FrameReady]),
    );

    println!("participants:");
    for device in [&camera, &xy, &z, &light_hub, &channel_a] {
        println!("  {} [{}]", device.label, public_kind_summary(device));
    }

    let camera_setup = runtime.submit(
        StateSet::immediate("triggered acquisition camera setup")
            .with_write(
                &camera,
                "exposure",
                Value::TimeInterval(TimeInterval::from_milliseconds(10.0)),
            )
            .with_write(
                &camera,
                "frame_interval",
                Value::TimeInterval(TimeInterval::from_milliseconds(50.0)),
            )
            .with_write(&camera, "pixel_format", Value::String("Mono8".into()))
            .into_command(),
    )?;
    let value = runtime.wait_completed(camera_setup.id, Duration::from_secs(1))?;
    println!("camera setup: {}", completion_summary(&value));

    let stage_setup = runtime.submit(
        StateSet::immediate("triggered acquisition stage position")
            .with_write(
                &xy,
                "x",
                Value::Position(Position::from_micrometers(1200.0)),
            )
            .with_write(&xy, "y", Value::Position(Position::from_micrometers(800.0)))
            .with_write(&z, "z", Value::Position(Position::from_micrometers(45.0)))
            .into_command(),
    )?;
    let value = runtime.wait_completed(stage_setup.id, Duration::from_secs(1))?;
    println!("stage setup: {}", completion_summary(&value));

    let light_setup = runtime.submit(
        StateSet::immediate("triggered acquisition light setup")
            .with_write(
                &channel_a,
                "intensity",
                Value::Ratio(Ratio::from_percent(20.0)),
            )
            .with_write(&channel_a, "selected", Value::Bool(true))
            .with_write(&light_hub, "enabled", Value::Bool(true))
            .into_command(),
    )?;
    let value = runtime.wait_completed(light_setup.id, Duration::from_secs(1))?;
    println!("light setup: {}", completion_summary(&value));

    let armed = runtime.submit(
        TimingPlan::builder()
            .route(
                &camera,
                &channel_a,
                TriggerSignal::Ttl,
                TriggerEdge::Rising,
                Duration::from_micros(50),
            )
            .sequence(
                &xy,
                "x",
                [
                    Value::Position(Position::from_micrometers(1200.0)),
                    Value::Position(Position::from_micrometers(2200.0)),
                    Value::Position(Position::from_micrometers(1200.0)),
                    Value::Position(Position::from_micrometers(2200.0)),
                ],
            )
            .sequence(
                &xy,
                "y",
                [
                    Value::Position(Position::from_micrometers(800.0)),
                    Value::Position(Position::from_micrometers(800.0)),
                    Value::Position(Position::from_micrometers(1800.0)),
                    Value::Position(Position::from_micrometers(1800.0)),
                ],
            )
            .sequence(
                &camera,
                "exposure",
                (0..4).map(|_| Value::TimeInterval(TimeInterval::from_milliseconds(10.0))),
            )
            .arm_order([&light_hub, &channel_a, &xy, &z, &camera])
            .stop(StopCondition::Count(4))
            .into_command()?,
    )?;
    let value = runtime.wait_completed(armed.id, Duration::from_secs(1))?;
    println!("armed timing plan: {}", completion_summary(&value));
    let value = read_property(&mut runtime, &light_hub, "timing_state")?;
    println!("light timing after arm: {}", completion_summary(&value));

    let started = runtime.submit(Command::start(armed.id))?;
    let value = runtime.wait_completed(started.id, Duration::from_secs(1))?;
    println!("started timing plan: {}", completion_summary(&value));
    let value = read_property(&mut runtime, &light_hub, "timing_state")?;
    println!("light timing after start: {}", completion_summary(&value));

    let stream = runtime.submit_request(
        &camera,
        CameraStreamRequest {
            encoding: Some(ImageEncoding::Mono8),
            frame_count: Some(4),
            buffer: FrameBufferSpec {
                capacity_frames: 4,
                overflow: OverflowPolicy::DropOldest,
            },
        },
    )?;
    let value = runtime.wait_completed(stream.id, Duration::from_secs(15))?;
    let stream = CameraStreamStarted::from_completion(&value)?;
    println!(
        "camera stream: {} stream={:?} frames={:?} format={:?}",
        completion_summary(&value),
        stream.stream,
        stream.frame_count,
        stream.pixel_format
    );

    let stopped = runtime.submit(Command::stop(armed.id))?;
    let value = runtime.wait_completed(stopped.id, Duration::from_secs(1))?;
    println!("stopped timing plan: {}", completion_summary(&value));
    let value = read_property(&mut runtime, &light_hub, "timing_state")?;
    println!("light timing after stop: {}", completion_summary(&value));

    while let Some(event) = operation_events.recv_timeout(Duration::from_millis(100)) {
        println!("event: {}", event_summary(&participant_descriptors, &event));
    }

    Ok(())
}

fn read_property(
    runtime: &mut LocalRuntime,
    device: impl Into<DeviceId>,
    key: &str,
) -> numanager_core::Result<Value> {
    let operation = runtime.submit(Command::read_property(device, key))?;
    runtime.wait_completed(operation.id, Duration::from_secs(1))
}
