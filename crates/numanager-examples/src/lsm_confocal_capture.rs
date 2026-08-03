use std::time::Duration;

use numanager_core::runtime::Runtime;
use numanager_core::{CapabilityKind, Event, EventFilter, EventKind};
use numanager_examples::capability_brief;

pub fn run() -> numanager_core::Result<()> {
    let source = numanager_examples::example_arg(0).unwrap_or_else(|| "imswitch".into());
    let (runtime, hub) = crate::lsm_common::runtime_for_source(&source)?;
    let capability = runtime.capability_by_kind(&hub, CapabilityKind::ConfocalImageCapture)?;
    let frames = runtime.subscribe(EventFilter::device(&hub).with_kind(EventKind::FrameReady));
    let request = crate::lsm_common::snapshot_request(512, 512);
    let value = crate::lsm_common::run_request(&runtime, &hub, request)?;

    println!("source: {source}");
    println!("hub: {}", hub.label);
    println!("api: {}", capability_brief(&capability));
    println!("request: raster 512x512 final reconstructed image");
    println!("result: {}", crate::lsm_common::api_result(&value));
    if let Some(summary) = crate::lsm_common::daqmx_task_plan_summary(&value) {
        println!("daqmx_plan: {summary}");
    }
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
