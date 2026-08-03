use numanager_core::runtime::{LocalRuntime, Runtime};
use numanager_core::*;
use numanager_drivers::sim::{SimComposedAutofocusConfig, SimComposedAutofocusDriver};
use numanager_examples::{
    capability_brief, completion_summary, device_by_kind,
    driver_capability_by_kind as capability_by_kind, metadata_key_summary, public_kind_summary,
};
use std::time::Duration;

pub fn run() -> numanager_core::Result<()> {
    let config = SimComposedAutofocusConfig {
        focal_plane_um: 4_250.0,
        search_span_um: 900.0,
        light_power_percent: 35.0,
        exposure: TimeInterval::from_milliseconds(20.0),
        ..SimComposedAutofocusConfig::default()
    };
    let driver = numanager_examples::driver_value(|id| SimComposedAutofocusDriver::new(id, config));
    let devices = driver.descriptors();

    let camera = device_by_kind(&devices, "camera")?;
    let z = device_by_kind(&devices, "axis.z")?;
    let light = device_by_kind(&devices, "light.source")?;
    let autofocus = device_by_kind(&devices, "autofocus")?;
    let capture = capability_by_kind(&driver, camera, CapabilityKind::CameraCapture)?;
    let z_move = capability_by_kind(&driver, z, CapabilityKind::StageMove)?;
    let autofocus_cap = capability_by_kind(&driver, autofocus, CapabilityKind::Autofocus)?;

    println!("biological simulation devices:");
    for device in [&camera, &z, &light, &autofocus] {
        println!("  {} [{}]", device.label, public_kind_summary(device));
    }
    println!(
        "capabilities: capture={} move={} autofocus={}",
        capability_brief(&capture),
        capability_brief(&z_move),
        capability_brief(&autofocus_cap)
    );

    let runtime = LocalRuntime::from_drivers(vec![Box::new(driver)]);

    let setup = runtime.submit(
        StateSet::prepare_then_commit("prepare off-focus biological scene")
            .with_write(
                camera,
                "exposure",
                Value::TimeInterval(TimeInterval::from_milliseconds(20.0)),
            )
            .with_write(light, "enabled", Value::Bool(true))
            .with_write(light, "power", Value::Ratio(Ratio::from_percent(35.0)))
            .into_command(),
    )?;
    let value = runtime.wait_completed(setup.id, Duration::from_secs(1))?;
    println!("scene setup completed: {}", completion_summary(&value));

    let off_focus = runtime.submit_request(
        z,
        StageMoveRequest::absolute([(StageAxis::Z, Position::from_micrometers(3_700.0))]),
    )?;
    let value = runtime.wait_completed(off_focus.id, Duration::from_secs(1))?;
    println!("off-focus Z move completed: {}", completion_summary(&value));

    let before = capture_frame(&runtime, camera)?;
    print_frame("before autofocus", &before);

    let autofocus_op = runtime.submit_request(
        autofocus,
        AutofocusRequest {
            mode: AutofocusMode::Hold,
            range: Some(AutofocusRange {
                center: Some(Position::from_micrometers(4_250.0)),
                span: Position::from_micrometers(900.0),
                step: Some(Position::from_micrometers(25.0)),
            }),
        },
    )?;
    let value = runtime.wait_completed(autofocus_op.id, Duration::from_secs(1))?;
    println!("autofocus completed: {}", completion_summary(&value));

    let after = capture_frame(&runtime, camera)?;
    print_frame("after autofocus", &after);

    let armed = runtime.submit(
        TimingPlan::builder()
            .sequence(
                camera,
                "exposure",
                [
                    Value::TimeInterval(TimeInterval::from_milliseconds(10.0)),
                    Value::TimeInterval(TimeInterval::from_milliseconds(25.0)),
                ],
            )
            .sequence(
                z,
                "z",
                [
                    Value::Position(Position::from_micrometers(4_100.0)),
                    Value::Position(Position::from_micrometers(4_250.0)),
                ],
            )
            .sequence(
                light,
                "power",
                [
                    Value::Ratio(Ratio::from_percent(20.0)),
                    Value::Ratio(Ratio::from_percent(45.0)),
                ],
            )
            .sequence(
                autofocus,
                "mode",
                [Value::String("hold".into()), Value::String("stop".into())],
            )
            .arm_order([light, z, camera, autofocus])
            .into_command()?,
    )?;
    let value = runtime.wait_completed(armed.id, Duration::from_secs(1))?;
    println!("timing arm completed: {}", completion_summary(&value));
    let started = runtime.submit(Command::start(armed.id))?;
    let value = runtime.wait_completed(started.id, Duration::from_secs(1))?;
    println!("timing start completed: {}", completion_summary(&value));
    let stopped = runtime.submit(Command::stop(armed.id))?;
    let value = runtime.wait_completed(stopped.id, Duration::from_secs(1))?;
    println!("timing stop completed: {}", completion_summary(&value));

    Ok(())
}

fn capture_frame(
    runtime: &LocalRuntime,
    camera: impl Into<DeviceId>,
) -> numanager_core::Result<Frame> {
    let camera = camera.into();
    let capture = runtime.submit_request(
        camera,
        CameraCaptureRequest {
            encoding: Some(ImageEncoding::Mono8),
            buffer: Some(FrameBufferSpec {
                capacity_frames: 4,
                overflow: OverflowPolicy::DropOldest,
            }),
        },
    )?;
    let value = runtime.wait_completed(capture.id, Duration::from_secs(1))?;
    let captured = CapturedFrame::from_completion(&value)?;
    runtime
        .frame(captured.handle)?
        .ok_or_else(|| Error::new(ErrorCode::Driver, "captured frame was not retained"))
}

fn print_frame(label: &str, frame: &Frame) {
    println!(
        "{label}: {}x{} {} bytes {} metadata=[{}]",
        frame.width,
        frame.height,
        frame.data.len(),
        frame.pixel_format,
        metadata_key_summary(&frame.metadata)
    );
    if let Some(score) = frame.metadata.get("focus_score") {
        println!("  focus_score={score:?}");
    }
    if let Some(z) = frame.metadata.get("z") {
        println!("  z={z:?}");
    }
}
