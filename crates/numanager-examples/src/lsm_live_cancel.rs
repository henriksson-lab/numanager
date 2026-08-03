use std::time::Duration;

use numanager_core::runtime::Runtime;
use numanager_core::{CancelResult, CapabilityKind, Event, EventFilter, EventKind, Value};
use numanager_examples::capability_brief;
use numanager_examples::example_arg;

pub fn run() -> numanager_core::Result<()> {
    let source = example_arg(0).unwrap_or_else(|| "sim-lsm".into());
    let (runtime, hub) = crate::lsm_common::runtime_for_source(&source)?;
    let capability = runtime.capability_by_kind(&hub, CapabilityKind::ConfocalImageStream)?;
    let frames = runtime.subscribe(EventFilter::device(&hub).with_kind(EventKind::FrameReady));
    let request = crate::lsm_common::continuous_live_image_request(256, 256);
    let operation = runtime.submit_request(&hub, request)?;

    let observed = drain_frames(&runtime, &frames, 2)?;
    let cancel = runtime.cancel(operation.id)?;

    println!("source: {source}");
    println!("hub: {}", hub.label);
    println!("api: {}", capability_brief(&capability));
    println!("request: continuous 256x256 live image stream, cancel after two frames");
    println!(
        "frames: observed={} latest={}x{} {} bytes format={}",
        observed.frames, observed.width, observed.height, observed.bytes, observed.pixel_format
    );
    if let Some(dirty) = observed.dirty_region {
        println!(
            "dirty_region: x={} y={} width={} height={} update_policy={} basis={}",
            dirty.x,
            dirty.y,
            dirty.width,
            dirty.height,
            observed.update_policy.as_deref().unwrap_or("unknown"),
            observed.dirty_region_basis.as_deref().unwrap_or("unknown")
        );
    }
    println!("cancel: {}", cancel_summary(cancel));
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
}

fn drain_frames(
    runtime: &numanager_core::runtime::LocalRuntime,
    frames: &numanager_core::runtime::Subscription,
    target: u64,
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
    };
    while summary.frames < target {
        let Some(event) = frames.recv_timeout(Duration::from_secs(3)) else {
            break;
        };
        if let Event::FrameReady(event) = event {
            summary.frames += 1;
            if let Some(frame) = runtime.frame(event.handle)? {
                summary.width = frame.width;
                summary.height = frame.height;
                summary.bytes = frame.data.len();
                summary.pixel_format = frame.pixel_format;
                summary.update_policy = string_metadata(&frame.metadata, "update_policy");
                summary.dirty_region_basis = string_metadata(&frame.metadata, "dirty_region_basis");
                summary.dirty_region = dirty_region_metadata(&frame.metadata);
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

fn cancel_summary(cancel: CancelResult) -> &'static str {
    match cancel {
        CancelResult::Cancelled => "cancelled",
        CancelResult::AlreadyFinished => "already_finished",
        CancelResult::Unsupported => "unsupported",
    }
}
