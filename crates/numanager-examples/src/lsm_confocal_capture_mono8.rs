use std::time::Duration;

use numanager_core::runtime::Runtime;
use numanager_core::{CapabilityKind, Event, EventFilter, EventKind, PixelCount, Value};
use numanager_examples::capability_brief;
use numanager_examples::example_arg;

pub fn run() -> numanager_core::Result<()> {
    let source = example_arg(0).unwrap_or_else(|| "sim-lsm".into());
    let (runtime, hub) = crate::lsm_common::runtime_for_source(&source)?;
    let capability = runtime.capability_by_kind(&hub, CapabilityKind::ConfocalImageCapture)?;
    let frames = runtime.subscribe(EventFilter::device(&hub).with_kind(EventKind::FrameReady));
    let mut request = crate::lsm_common::snapshot_request(256, 256);
    request
        .reconstruction
        .insert("pixel_format".into(), Value::String("Mono8".into()));
    request.reconstruction.insert(
        "image_width".into(),
        Value::PixelCount(PixelCount::new(128)),
    );
    request.reconstruction.insert(
        "image_height".into(),
        Value::PixelCount(PixelCount::new(128)),
    );
    let value = crate::lsm_common::run_request(&runtime, &hub, request)?;

    println!("source: {source}");
    println!("hub: {}", hub.label);
    println!("api: {}", capability_brief(&capability));
    println!("request: raster 256x256 scan reconstructed to 128x128 Mono8");
    println!("result: {}", crate::lsm_common::api_result(&value));
    if let Some(Event::FrameReady(event)) = frames.recv_timeout(Duration::from_millis(100)) {
        if let Some(frame) = runtime.frame(event.handle)? {
            println!(
                "frame: {}x{} {} bytes format={}",
                frame.width,
                frame.height,
                frame.data.len(),
                frame.pixel_format
            );
            if let Some(summary) = crate::lsm_common::frame_scan_metadata_summary(&frame) {
                println!("frame_metadata: {summary}");
            }
        }
    }
    Ok(())
}
