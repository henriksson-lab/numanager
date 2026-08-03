use std::time::Duration;

use numanager_core::runtime::Runtime;
use numanager_core::{CapabilityKind, Event, EventFilter, EventKind, Value};
use numanager_examples::capability_brief;
use numanager_examples::example_arg;

pub fn run() -> numanager_core::Result<()> {
    let source = example_arg(0).unwrap_or_else(|| "imswitch".into());
    let (runtime, hub) = crate::lsm_common::runtime_for_source(&source)?;
    let capability = runtime.capability_by_kind(&hub, CapabilityKind::ScanSignalStream)?;
    let events = runtime.subscribe(EventFilter::device(&hub).with_kind(EventKind::ScanSignalChunk));
    let operations =
        runtime.subscribe(EventFilter::device(&hub).with_kind(EventKind::OperationChanged));
    let request = crate::lsm_common::line_signal_request(1024, 256);
    let operation = runtime.submit_request(&hub, request)?;
    let value = runtime.wait_completed(operation.id, Duration::from_secs(5))?;
    let chunks = drain_chunks(&events);
    let progress = crate::lsm_common::drain_operation_progress(&operations, operation.id);

    println!("source: {source}");
    println!("hub: {}", hub.label);
    println!("api: {}", capability_brief(&capability));
    println!("request: one 1024-sample line over counter0 + ai0, chunk_size=256");
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
    if chunks.observed > 0 {
        println!(
            "chunks: observed={} origin={} first_sample={} samples={} chunk_size={} sample_period_s={:.9} channels={} dropped_chunks={} dropped_samples={} overflowed={}",
            chunks.observed,
            chunks.origin,
            chunks.first_sample,
            chunks.samples,
            chunks.chunk_size,
            chunks.sample_period_s,
            chunks.channels,
            chunks.dropped_chunks,
            chunks.dropped_samples,
            chunks.overflowed
        );
        if !chunks.metadata.is_empty() {
            println!("chunk_metadata: {}", chunks.metadata);
        }
        if !chunks.sample_preview.is_empty() {
            println!("first_chunk_samples: {}", chunks.sample_preview);
        }
    }
    Ok(())
}

struct ChunkSummary {
    observed: u64,
    origin: i64,
    first_sample: u64,
    samples: u64,
    chunk_size: u32,
    sample_period_s: f64,
    channels: usize,
    dropped_chunks: i64,
    dropped_samples: i64,
    overflowed: bool,
    metadata: String,
    sample_preview: String,
}

fn drain_chunks(events: &numanager_core::runtime::Subscription) -> ChunkSummary {
    let mut summary = ChunkSummary {
        observed: 0,
        origin: 0,
        first_sample: 0,
        samples: 0,
        chunk_size: 0,
        sample_period_s: 0.0,
        channels: 0,
        dropped_chunks: 0,
        dropped_samples: 0,
        overflowed: false,
        metadata: String::new(),
        sample_preview: String::new(),
    };
    while let Some(event) = events.recv_timeout(Duration::from_millis(100)) {
        if let Event::ScanSignalChunk(event) = event {
            if summary.observed == 0 {
                summary.origin = event.origin.ticks();
                summary.first_sample = event.first_sample;
                summary.chunk_size = pixel_metadata(&event.metadata, "chunk_size").unwrap_or(0);
                summary.sample_period_s = event.sample_period.seconds();
                summary.channels = event.channels.len();
                summary.dropped_chunks =
                    i64_metadata(&event.metadata, "dropped_chunks").unwrap_or(0);
                summary.dropped_samples =
                    i64_metadata(&event.metadata, "dropped_samples").unwrap_or(0);
                summary.overflowed = bool_metadata(&event.metadata, "overflowed").unwrap_or(false);
                summary.metadata = chunk_metadata_summary(&event.metadata);
                summary.sample_preview = sample_preview(&event.samples);
            }
            summary.observed += 1;
            summary.samples += event.sample_count;
        }
    }
    summary
}

fn sample_preview(samples: &std::collections::BTreeMap<String, Vec<Value>>) -> String {
    samples
        .iter()
        .filter_map(|(channel, values)| {
            values
                .first()
                .map(|value| format!("{channel}={}", sample_value(value)))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn sample_value(value: &Value) -> String {
    match value {
        Value::I64(value) => value.to_string(),
        Value::Voltage(value) => format!("{:.4} V", value.volts()),
        other => format!("{other:?}"),
    }
}

fn chunk_metadata_summary(metadata: &std::collections::BTreeMap<String, Value>) -> String {
    let channels = string_list_metadata(metadata, "channels")
        .filter(|channels| !channels.is_empty())
        .map(|channels| channels.join("+"))
        .unwrap_or_else(|| "unknown".into());
    let detectors = string_list_metadata(metadata, "detectors")
        .filter(|detectors| !detectors.is_empty())
        .map(|detectors| detectors.join("+"))
        .unwrap_or_else(|| "unknown".into());
    let laser_gate = bool_metadata(metadata, "laser_gate_enabled")
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".into());
    let detector_gain = ratio_metadata(metadata, "detector_gain").unwrap_or(1.0);
    let detector_noise = ratio_metadata(metadata, "detector_noise").unwrap_or(1.0);
    let line = i64_metadata(metadata, "line").unwrap_or(0);
    let chunk = i64_metadata(metadata, "chunk_index").unwrap_or(0);
    let first = i64_metadata(metadata, "first_sample").unwrap_or(0);
    let origin = timestamp_metadata(metadata, "timing_origin").unwrap_or(0);
    format!(
        "channels={channels}, detectors={detectors}, laser_gate_enabled={laser_gate}, detector_gain={detector_gain:.3}, detector_noise={detector_noise:.3}, line={line}, chunk={chunk}, first_sample={first}, origin={origin}"
    )
}

fn string_list_metadata(
    metadata: &std::collections::BTreeMap<String, Value>,
    key: &str,
) -> Option<Vec<String>> {
    match metadata.get(key) {
        Some(Value::List(values)) => Some(
            values
                .iter()
                .filter_map(|value| match value {
                    Value::String(value) => Some(value.clone()),
                    _ => None,
                })
                .collect(),
        ),
        _ => None,
    }
}

fn i64_metadata(metadata: &std::collections::BTreeMap<String, Value>, key: &str) -> Option<i64> {
    match metadata.get(key) {
        Some(Value::I64(value)) => Some(*value),
        _ => None,
    }
}

fn bool_metadata(metadata: &std::collections::BTreeMap<String, Value>, key: &str) -> Option<bool> {
    match metadata.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn ratio_metadata(metadata: &std::collections::BTreeMap<String, Value>, key: &str) -> Option<f64> {
    match metadata.get(key) {
        Some(Value::Ratio(value)) => Some(value.fraction()),
        _ => None,
    }
}

fn timestamp_metadata(
    metadata: &std::collections::BTreeMap<String, Value>,
    key: &str,
) -> Option<i64> {
    match metadata.get(key) {
        Some(Value::Timestamp(value)) => Some(value.ticks()),
        _ => None,
    }
}

fn pixel_metadata(metadata: &std::collections::BTreeMap<String, Value>, key: &str) -> Option<u32> {
    match metadata.get(key) {
        Some(Value::PixelCount(value)) => Some(value.0),
        Some(Value::I64(value)) => u32::try_from(*value).ok(),
        _ => None,
    }
}
