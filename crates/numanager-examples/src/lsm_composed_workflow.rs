use std::time::Duration;

use numanager_core::runtime::Runtime;
use numanager_core::*;
use numanager_examples::{capability_brief, completion_summary, public_kind_tags};

pub fn run() -> numanager_core::Result<()> {
    let (runtime, lsm) = crate::lsm_common::composed_runtime()?;
    let microscope = runtime
        .devices()
        .iter()
        .find(|device| device.label == "sim-microscope")
        .cloned()
        .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "missing sim-microscope hub"))?;
    let camera = runtime
        .device_by_capability(CapabilityKind::CameraCapture)?
        .clone();
    let xy = runtime.device_by_kind("stage.xy")?.clone();
    let z = runtime.device_by_kind("stage.z")?.clone();

    println!("source: sim-composed");
    println!("camera: {} {:?}", camera.label, public_kind_tags(&camera));
    println!("lsm: {} {:?}", lsm.label, public_kind_tags(&lsm));
    println!("xy: {} {:?}", xy.label, public_kind_tags(&xy));
    println!("z: {} {:?}", z.label, public_kind_tags(&z));

    let move_state = StateSet::immediate("composed simulator field of view").with_writes([
        StateWrite::new(
            xy.id,
            "x",
            Value::Position(Position::from_micrometers(320.0)),
        ),
        StateWrite::new(
            xy.id,
            "y",
            Value::Position(Position::from_micrometers(-180.0)),
        ),
        StateWrite::new(
            z.id,
            "z",
            Value::Position(Position::from_micrometers(4_252.0)),
        ),
    ]);
    let move_op = runtime.submit(move_state.into_command())?;
    let move_value = runtime.wait_completed(move_op.id, Duration::from_secs(1))?;
    println!("shared stage state: {}", completion_summary(&move_value));

    let frames = runtime.subscribe(EventFilter::device(&camera).with_kind(EventKind::FrameReady));
    let brightfield = runtime.submit_request(
        &camera,
        CameraCaptureRequest {
            encoding: Some(ImageEncoding::Mono8),
            buffer: Some(FrameBufferSpec::default()),
        },
    )?;
    let value = runtime.wait_completed(brightfield.id, Duration::from_secs(2))?;
    println!("brightfield capture: {}", completion_summary(&value));
    if let Some(Event::FrameReady(event)) = frames.recv_timeout(Duration::from_secs(1)) {
        println!(
            "brightfield frame: {}x{} format={} stream={}",
            event.width, event.height, event.pixel_format, event.handle.stream.0
        );
    }

    let sample_pixel_size = runtime.execute(
        Command::read_property(camera.id, "sample_pixel_size"),
        Duration::from_secs(1),
    )?;
    println!("shared sample_pixel_size: {:?}", sample_pixel_size);
    let microscope_seed = runtime.execute(
        Command::read_property(microscope.id, "sample_seed"),
        Duration::from_secs(1),
    )?;
    let lsm_seed = runtime.execute(
        Command::read_property(lsm.id, "sample_seed"),
        Duration::from_secs(1),
    )?;
    println!(
        "shared sample_seed: microscope={:?} lsm={:?}",
        microscope_seed, lsm_seed
    );
    let detector_gain = runtime.execute(
        Command::write_property(
            lsm.id,
            "detector_gain",
            Value::Ratio(Ratio::from_percent(125.0)),
        ),
        Duration::from_secs(1),
    )?;
    let detector_noise = runtime.execute(
        Command::write_property(
            lsm.id,
            "detector_noise",
            Value::Ratio(Ratio::from_percent(80.0)),
        ),
        Duration::from_secs(1),
    )?;
    println!(
        "lsm detector controls: gain={:?} noise={:?}",
        detector_gain, detector_noise
    );

    for kind in [
        CapabilityKind::ConfocalImageCapture,
        CapabilityKind::ConfocalImageStream,
        CapabilityKind::ScanSignalStream,
    ] {
        let capability = runtime.capability_by_kind(&lsm, kind.clone())?;
        println!("lsm api: {}", capability_brief(&capability));
    }

    let lsm_frames = runtime.subscribe(EventFilter::device(&lsm).with_kind(EventKind::FrameReady));
    let lsm_chunks =
        runtime.subscribe(EventFilter::device(&lsm).with_kind(EventKind::ScanSignalChunk));

    let capture = crate::lsm_common::run_request(
        &runtime,
        &lsm,
        crate::lsm_common::snapshot_request(512, 512),
    )?;
    println!(
        "confocal capture: {}",
        crate::lsm_common::api_result(&capture)
    );
    if let Some(Event::FrameReady(event)) = lsm_frames.recv_timeout(Duration::from_secs(1)) {
        if let Some(frame) = runtime.frame(event.handle)? {
            if let Some(summary) = crate::lsm_common::scene_metadata_summary(&frame.metadata) {
                println!("confocal capture scene: {summary}");
            }
        }
    }

    let stream = crate::lsm_common::run_request(
        &runtime,
        &lsm,
        crate::lsm_common::live_image_request(256, 256),
    )?;
    println!(
        "confocal stream: {}",
        crate::lsm_common::api_result(&stream)
    );
    while let Some(event) = lsm_frames.recv_timeout(Duration::from_millis(100)) {
        if let Event::FrameReady(event) = event {
            if let Some(frame) = runtime.frame(event.handle)? {
                if let Some(summary) = crate::lsm_common::scene_metadata_summary(&frame.metadata) {
                    println!("confocal stream scene: {summary}");
                }
            }
            break;
        }
    }

    let signal = crate::lsm_common::run_request(
        &runtime,
        &lsm,
        crate::lsm_common::line_signal_request(512, 128),
    )?;
    println!("scan signal: {}", crate::lsm_common::api_result(&signal));
    while let Some(event) = lsm_chunks.recv_timeout(Duration::from_millis(100)) {
        if let Event::ScanSignalChunk(event) = event {
            if let Some(summary) = crate::lsm_common::scene_metadata_summary(&event.metadata) {
                println!("scan signal scene: {summary}");
            }
            break;
        }
    }

    Ok(())
}
