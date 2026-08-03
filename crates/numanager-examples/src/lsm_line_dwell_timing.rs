use std::time::Duration;

use numanager_core::runtime::Runtime;
use numanager_core::{CapabilityKind, Event, EventFilter, EventKind, TimeInterval, Value};
use numanager_examples::capability_brief;

pub fn run() -> numanager_core::Result<()> {
    let (runtime, hub) = crate::lsm_common::runtime_for_source("sim-lsm")?;
    let capability = runtime.capability_by_kind(&hub, CapabilityKind::ScanSignalStream)?;
    let chunks = runtime.subscribe(EventFilter::device(&hub).with_kind(EventKind::ScanSignalChunk));
    let mut request = crate::lsm_common::line_signal_request(500, 125);
    request.timing.remove("sample_rate");
    request.timing.insert(
        "line_dwell".into(),
        Value::TimeInterval(TimeInterval::from_seconds(0.010)),
    );

    let operation = runtime.submit_request(&hub, request)?;
    let value = runtime.wait_completed(operation.id, Duration::from_secs(5))?;
    let chunk = first_chunk(&chunks);

    println!("source: sim-lsm");
    println!("hub: {}", hub.label);
    println!("api: {}", capability_brief(&capability));
    println!("request: 500 samples over 10 ms line dwell, no explicit sample_rate");
    println!("result: {}", crate::lsm_common::api_result(&value));
    if let Some(chunk) = chunk {
        println!(
            "first_chunk: sample_rate_hz={:.0} sample_period_s={:.9} samples={} channels={}",
            chunk.sample_rate_hz, chunk.sample_period_s, chunk.samples, chunk.channels
        );
    }
    Ok(())
}

struct ChunkTiming {
    sample_rate_hz: f64,
    sample_period_s: f64,
    samples: u64,
    channels: usize,
}

fn first_chunk(events: &numanager_core::runtime::Subscription) -> Option<ChunkTiming> {
    while let Some(event) = events.recv_timeout(Duration::from_millis(100)) {
        if let Event::ScanSignalChunk(event) = event {
            return Some(ChunkTiming {
                sample_rate_hz: event.sample_rate.hertz(),
                sample_period_s: event.sample_period.seconds(),
                samples: event.sample_count,
                channels: event.channels.len(),
            });
        }
    }
    None
}
