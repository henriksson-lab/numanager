use std::time::Duration;

use numanager_core::runtime::Runtime;
use numanager_core::{CapabilityKind, Event, EventFilter, EventKind, PixelCount, Value};
use numanager_examples::capability_brief;
use numanager_examples::example_arg;

pub fn run() -> numanager_core::Result<()> {
    let source = example_arg(0).unwrap_or_else(|| "imswitch".into());
    let (runtime, hub) = crate::lsm_common::runtime_for_source(&source)?;
    let capability = runtime.capability_by_kind(&hub, CapabilityKind::ConfocalImageStream)?;
    let frames = runtime.subscribe(EventFilter::device(&hub).with_kind(EventKind::FrameReady));
    let operations =
        runtime.subscribe(EventFilter::device(&hub).with_kind(EventKind::OperationChanged));
    let mut request = crate::lsm_common::live_image_request(512, 512);
    request.reconstruction.insert(
        "image_width".into(),
        Value::PixelCount(PixelCount::new(256)),
    );
    request.reconstruction.insert(
        "image_height".into(),
        Value::PixelCount(PixelCount::new(256)),
    );
    let operation = runtime.submit_request(&hub, request)?;
    let value = runtime.wait_completed(operation.id, Duration::from_secs(5))?;
    let summary = drain_frames(&runtime, &frames)?;
    let progress = crate::lsm_common::drain_operation_progress(&operations, operation.id);

    println!("source: {source}");
    println!("hub: {}", hub.label);
    println!("api: {}", capability_brief(&capability));
    println!(
        "request: raster 512x512 scan reconstructed to 256x256 live stream, dirty-region updates"
    );
    println!("result: {}", crate::lsm_common::api_result(&value));
    if let Some(progress) = progress {
        println!(
            "progress: updates={} last={:.0}/{:.0}",
            progress.updates, progress.completed, progress.total
        );
    }
    if let Some(summary) = crate::lsm_common::daqmx_task_plan_summary(&value) {
        println!("daqmx_plan: {summary}");
    }
    if summary.frames > 0 {
        println!(
            "frames: observed={} latest={}x{} {} bytes format={}",
            summary.frames, summary.width, summary.height, summary.bytes, summary.pixel_format
        );
        if let Some(dirty) = summary.dirty_region {
            println!(
                "dirty_region: x={} y={} width={} height={} update_policy={} basis={}",
                dirty.x,
                dirty.y,
                dirty.width,
                dirty.height,
                summary.update_policy.as_deref().unwrap_or("unknown"),
                summary.dirty_region_basis.as_deref().unwrap_or("unknown")
            );
        }
        if let Some(metadata) = summary.metadata {
            println!("frame_metadata: {metadata}");
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct DirtyRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

struct FrameSummary {
    frames: u64,
    width: u32,
    height: u32,
    bytes: usize,
    pixel_format: String,
    update_policy: Option<String>,
    dirty_region_basis: Option<String>,
    dirty_region: Option<DirtyRegion>,
    metadata: Option<String>,
}

fn drain_frames(
    runtime: &numanager_core::runtime::LocalRuntime,
    frames: &numanager_core::runtime::Subscription,
) -> numanager_core::Result<FrameSummary> {
    let mut summary = FrameSummary {
        frames: 0,
        width: 0,
        height: 0,
        bytes: 0,
        pixel_format: String::new(),
        update_policy: None,
        dirty_region_basis: None,
        dirty_region: None,
        metadata: None,
    };
    while let Some(event) = frames.recv_timeout(Duration::from_millis(100)) {
        if let Event::FrameReady(event) = event {
            summary.frames += 1;
            if let Some(frame) = runtime.frame(event.handle)? {
                summary.width = frame.width;
                summary.height = frame.height;
                summary.bytes = frame.data.len();
                summary.pixel_format = frame.pixel_format.clone();
                summary.update_policy = string_metadata(&frame.metadata, "update_policy");
                summary.dirty_region_basis = string_metadata(&frame.metadata, "dirty_region_basis");
                summary.dirty_region = dirty_region_metadata(&frame.metadata);
                summary.metadata = crate::lsm_common::frame_scan_metadata_summary(&frame);
            }
        }
    }
    Ok(summary)
}

fn string_metadata(
    metadata: &std::collections::BTreeMap<String, Value>,
    key: &str,
) -> Option<String> {
    match metadata.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn dirty_region_metadata(
    metadata: &std::collections::BTreeMap<String, Value>,
) -> Option<DirtyRegion> {
    Some(DirtyRegion {
        x: pixel_metadata(metadata, "dirty_x")?,
        y: pixel_metadata(metadata, "dirty_y")?,
        width: pixel_metadata(metadata, "dirty_width")?,
        height: pixel_metadata(metadata, "dirty_height")?,
    })
}

fn pixel_metadata(metadata: &std::collections::BTreeMap<String, Value>, key: &str) -> Option<u32> {
    match metadata.get(key) {
        Some(Value::PixelCount(value)) => Some(value.0),
        Some(Value::I64(value)) => u32::try_from(*value).ok(),
        _ => None,
    }
}
