use std::time::Duration;

use numanager_core::runtime::Runtime;
use numanager_core::{CancelResult, CapabilityKind, Event, EventFilter, EventKind, Value};
use numanager_examples::capability_brief;
use numanager_examples::example_arg;

pub fn run() -> numanager_core::Result<()> {
    let source = example_arg(0).unwrap_or_else(|| "sim-lsm".into());
    let (runtime, hub) = crate::lsm_common::runtime_for_source(&source)?;
    let capability = runtime.capability_by_kind(&hub, CapabilityKind::ScanSignalStream)?;
    let chunks = runtime.subscribe(EventFilter::device(&hub).with_kind(EventKind::ScanSignalChunk));
    let request = crate::lsm_common::continuous_line_signal_request(512, 128);
    let operation = runtime.submit_request(&hub, request)?;
    let observed = drain_chunks(&chunks, 3);
    let cancel = runtime.cancel(operation.id)?;

    println!("source: {source}");
    println!("hub: {}", hub.label);
    println!("api: {}", capability_brief(&capability));
    println!("request: continuous line signal stream, cancel after three chunks");
    println!(
        "chunks: observed={} latest_line={} latest_chunk={} samples={} channels={}",
        observed.chunks,
        observed.latest_line,
        observed.latest_chunk,
        observed.samples,
        observed.channels
    );
    if let Some(first) = observed.first_chunk {
        println!("first_chunk: {first}");
    }
    println!("cancel: {}", cancel_summary(cancel));
    Ok(())
}

struct ChunkSummary {
    chunks: u64,
    latest_line: u64,
    latest_chunk: u64,
    samples: u64,
    channels: usize,
    first_chunk: Option<String>,
}

fn drain_chunks(events: &numanager_core::runtime::Subscription, target: u64) -> ChunkSummary {
    let mut summary = ChunkSummary {
        chunks: 0,
        latest_line: 0,
        latest_chunk: 0,
        samples: 0,
        channels: 0,
        first_chunk: None,
    };
    while summary.chunks < target {
        let Some(event) = events.recv_timeout(Duration::from_secs(3)) else {
            break;
        };
        if let Event::ScanSignalChunk(event) = event {
            if summary.first_chunk.is_none() {
                summary.first_chunk = Some(chunk_summary(&event));
            }
            summary.chunks += 1;
            summary.latest_line = event.line;
            summary.latest_chunk = event.chunk;
            summary.samples += event.sample_count;
            summary.channels = event.channels.len();
        }
    }
    summary
}

fn chunk_summary(event: &numanager_core::ScanSignalChunkEvent) -> String {
    let channels = if event.channels.is_empty() {
        "none".into()
    } else {
        event.channels.join("+")
    };
    let dropped_chunks = i64_metadata(&event.metadata, "dropped_chunks").unwrap_or(0);
    let dropped_samples = i64_metadata(&event.metadata, "dropped_samples").unwrap_or(0);
    let overflowed = bool_metadata(&event.metadata, "overflowed").unwrap_or(false);
    let mut summary = format!(
        "channels={channels}, line={}, chunk={}, first_sample={}, sample_rate_hz={:.0}, sample_period_s={:.9}, dropped_chunks={dropped_chunks}, dropped_samples={dropped_samples}, overflowed={overflowed}",
        event.line,
        event.chunk,
        event.first_sample,
        event.sample_rate.hertz(),
        event.sample_period.seconds(),
    );
    if let Some(scene) = crate::lsm_common::scene_metadata_summary(&event.metadata) {
        summary.push_str(&format!(", scene=[{scene}]"));
    }
    summary
}

fn i64_metadata(metadata: &std::collections::BTreeMap<String, Value>, key: &str) -> Option<i64> {
    match metadata.get(key) {
        Some(Value::I64(value)) => Some(*value),
        Some(Value::PixelCount(value)) => Some(i64::from(value.0)),
        _ => None,
    }
}

fn bool_metadata(metadata: &std::collections::BTreeMap<String, Value>, key: &str) -> Option<bool> {
    match metadata.get(key) {
        Some(Value::Bool(value)) => Some(*value),
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
