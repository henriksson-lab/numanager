//! Laser-scanning microscopy simulator over the shared specimen model.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use numanager_core::*;

use crate::sim_lsm_model::{
    render_confocal_raster_for_detectors, render_scan_row_profiles, LsmFluorescenceConfig,
    LsmRasterSpec,
};
use crate::sim_sample::SimSampleConfig;

const RESOURCE_OFFSET: u64 = 920;
const HUB_OFFSET: u64 = 921;
const CAPTURE_CAPABILITY: CapabilityId = CapabilityId(1);
const IMAGE_STREAM_CAPABILITY: CapabilityId = CapabilityId(2);
const SIGNAL_STREAM_CAPABILITY: CapabilityId = CapabilityId(3);

pub struct SimLsmDriver {
    id: DriverId,
    resource: ResourceId,
    hub: DeviceId,
    /// Shared so a running stream picks up live property writes such as
    /// detector gain and noise, instead of rendering from a snapshot taken
    /// when the stream started.
    model: Arc<Mutex<LsmFluorescenceConfig>>,
    next_token: u64,
    frames: AtomicU64,
    events: VecDeque<DriverEvent>,
    streams: HashMap<DriverToken, Arc<AtomicBool>>,
    worker_tx: Sender<DriverEvent>,
    worker_rx: Receiver<DriverEvent>,
}

/// Copy of the shared model. Rendering runs on the copy so the lock is never
/// held across a frame or a line.
fn snapshot_model(model: &Arc<Mutex<LsmFluorescenceConfig>>) -> LsmFluorescenceConfig {
    *model
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl SimLsmDriver {
    fn model_snapshot(&self) -> LsmFluorescenceConfig {
        snapshot_model(&self.model)
    }

    fn model_lock(&self) -> MutexGuard<'_, LsmFluorescenceConfig> {
        self.model
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn simulated(id: DriverId) -> Self {
        Self::with_model(id, LsmFluorescenceConfig::default())
    }

    pub(crate) fn simulated_with_sample(id: DriverId, sample: SimSampleConfig) -> Self {
        let mut model = LsmFluorescenceConfig::default();
        model.sample = sample;
        Self::with_model(id, model)
    }

    fn with_model(id: DriverId, model: LsmFluorescenceConfig) -> Self {
        let (worker_tx, worker_rx) = mpsc::channel();
        Self {
            id,
            resource: ResourceId(NodeId(id.0 * 1000 + RESOURCE_OFFSET)),
            hub: DeviceId(NodeId(id.0 * 1000 + HUB_OFFSET)),
            model: Arc::new(Mutex::new(model)),
            next_token: 1,
            frames: AtomicU64::new(1),
            events: VecDeque::new(),
            streams: HashMap::new(),
            worker_tx,
            worker_rx,
        }
    }

    fn next_token(&mut self) -> DriverToken {
        let token = DriverToken(self.next_token);
        self.next_token += 1;
        token
    }

    fn capture(&mut self, request: ConfocalImageCaptureRequest) -> Value {
        let scan = RasterScan::from_maps(&request.scan, &request.reconstruction);
        let handle = self.publish_frame(&scan, "capture", None);
        Value::Map(BTreeMap::from([
            ("stream".into(), Value::I64(handle.stream.0 as i64)),
            ("frame".into(), Value::I64(handle.frame.0 as i64)),
            (
                "width".into(),
                Value::PixelCount(PixelCount::new(scan.frame_width())),
            ),
            (
                "height".into(),
                Value::PixelCount(PixelCount::new(scan.frame_height())),
            ),
            (
                "pixel_format".into(),
                Value::String(scan.pixel_format.name().into()),
            ),
            (
                "sample_pixel_size".into(),
                Value::Position(Position::from_micrometers(scan.frame_pixel_size_um())),
            ),
        ]))
    }

    fn image_stream(&mut self, token: DriverToken, request: ConfocalImageStreamRequest) -> Value {
        let scan = RasterScan::from_maps(&request.scan, &request.reconstruction);
        let frames = map_i64(&request.scan, "frames")
            .filter(|value| *value > 0)
            .unwrap_or(4)
            .clamp(1, 32) as u64;
        let update_policy = request.update_policy.as_deref().unwrap_or("complete_frame");
        let update = FrameUpdate {
            policy: update_policy,
            overwrite_previous_pixels: request.overwrite_previous_pixels,
        };
        let first = self.publish_frame(&scan, "image_stream", Some(update));
        let mut latest = first;
        self.events.push_back(DriverEvent::TokenProgress {
            token,
            progress: Progress {
                completed: 1.0,
                total: frames as f64,
            },
        });
        for completed in 2..=frames {
            latest = self.publish_frame(&scan, "image_stream", Some(update));
            self.events.push_back(DriverEvent::TokenProgress {
                token,
                progress: Progress {
                    completed: completed as f64,
                    total: frames as f64,
                },
            });
        }
        Value::Map(BTreeMap::from([
            ("stream".into(), Value::I64(latest.stream.0 as i64)),
            ("first_frame".into(), Value::I64(first.frame.0 as i64)),
            ("latest_frame".into(), Value::I64(latest.frame.0 as i64)),
            ("frames".into(), Value::I64(frames as i64)),
            (
                "width".into(),
                Value::PixelCount(PixelCount::new(scan.frame_width())),
            ),
            (
                "height".into(),
                Value::PixelCount(PixelCount::new(scan.frame_height())),
            ),
            (
                "pixel_format".into(),
                Value::String(scan.pixel_format.name().into()),
            ),
            ("update_policy".into(), Value::String(update_policy.into())),
            (
                "overwrite_previous_pixels".into(),
                Value::Bool(request.overwrite_previous_pixels),
            ),
        ]))
    }

    fn start_continuous_image_stream(
        &mut self,
        token: DriverToken,
        request: ConfocalImageStreamRequest,
    ) {
        let scan = RasterScan::from_maps(&request.scan, &request.reconstruction);
        let stream = StreamId(token.0);
        let stop = Arc::new(AtomicBool::new(false));
        self.streams.insert(token, Arc::clone(&stop));
        let tx = self.worker_tx.clone();
        let model = Arc::clone(&self.model);
        let device = self.hub;
        let update_policy = request
            .update_policy
            .unwrap_or_else(|| "complete_frame".into());
        let overwrite = request.overwrite_previous_pixels;
        let sample_rate_hz = scan.sample_rate_hz;
        thread::spawn(move || {
            let mut sequence = 0u64;
            let period = Duration::from_secs_f64(
                (scan.width as f64 * scan.height as f64 / sample_rate_hz.max(1.0))
                    .clamp(0.033, 2.0),
            );
            while !stop.load(Ordering::Relaxed) {
                let started = Instant::now();
                let frame_index = sequence + 1;
                // Re-read per frame so detector changes apply to the next frame.
                let snapshot = snapshot_model(&model);
                let frame = lsm_frame(
                    &snapshot,
                    &scan,
                    "image_stream",
                    device,
                    stream,
                    FrameId(sequence),
                    frame_index,
                    Some(FrameUpdate {
                        policy: update_policy.as_str(),
                        overwrite_previous_pixels: overwrite,
                    }),
                );
                let _ = tx.send(DriverEvent::FrameReady(frame));
                let _ = tx.send(DriverEvent::TokenProgress {
                    token,
                    progress: Progress {
                        completed: frame_index as f64,
                        total: 0.0,
                    },
                });
                sequence += 1;
                let spent = started.elapsed();
                if period > spent {
                    thread::sleep(period - spent);
                }
            }
            let _ = tx.send(DriverEvent::TokenCompleted {
                token,
                value: image_stream_completion(
                    stream,
                    sequence,
                    scan,
                    update_policy.as_str(),
                    overwrite,
                ),
            });
        });
    }

    fn signal_stream(&mut self, token: DriverToken, request: ScanSignalStreamRequest) -> Value {
        let samples = map_i64(&request.timing, "samples_per_line")
            .unwrap_or(1024)
            .clamp(1, 65_536) as u32;
        let lines = map_i64(&request.timing, "lines").unwrap_or(1).clamp(1, 256) as u32;
        let chunk_size = request.chunk_size.unwrap_or(256).clamp(1, samples as u64) as u32;
        let total_chunks = u64::from(lines) * u64::from(samples.div_ceil(chunk_size));
        let origin = Timestamp::from_controller_ticks(0);
        let scan = RasterScan::from_line_timing(&request.timing, samples);
        let sample_rate_hz = scan.sample_rate_hz;
        let sample_period_s = 1.0 / sample_rate_hz.max(f64::EPSILON);
        let stream = StreamId(self.hub.0 .0 + 10);
        let mut first_sample = 0u32;
        let mut chunk_index = 0u32;
        let mut last_chunk_samples = 0u32;
        let frame_index = self.frames.fetch_add(1, Ordering::Relaxed);
        for line in 0..lines {
            let profiles = render_scan_row_profiles(
                &self.model_snapshot(),
                scan.spec(),
                &request.channels,
                samples,
                line,
                frame_index,
            );
            let line_samples = profiles
                .first()
                .map(|(_, samples)| samples.as_slice())
                .unwrap_or(&[]);
            for chunk in line_samples.chunks(chunk_size as usize) {
                let chunk_start = first_sample as usize;
                let sample_count = chunk.len() as u64;
                last_chunk_samples = chunk.len() as u32;
                let mut metadata = scan_metadata(&scan);
                metadata.extend(detector_metadata(&self.model_snapshot()));
                metadata.extend([
                    ("source".into(), Value::String("sim_lsm".into())),
                    ("mode".into(), Value::String("line_scan".into())),
                    (
                        "channels".into(),
                        Value::List(
                            request
                                .channels
                                .iter()
                                .cloned()
                                .map(Value::String)
                                .collect(),
                        ),
                    ),
                    ("line".into(), Value::I64(i64::from(line))),
                    ("chunk_index".into(), Value::I64(i64::from(chunk_index))),
                    ("first_sample".into(), Value::I64(i64::from(first_sample))),
                    ("dropped_chunks".into(), Value::I64(0)),
                    ("dropped_samples".into(), Value::I64(0)),
                    ("overflowed".into(), Value::Bool(false)),
                    (
                        "chunk_size".into(),
                        Value::PixelCount(PixelCount::new(chunk_size)),
                    ),
                    (
                        "samples_per_line".into(),
                        Value::PixelCount(PixelCount::new(samples)),
                    ),
                    ("lines".into(), Value::PixelCount(PixelCount::new(lines))),
                    (
                        "sample_rate".into(),
                        Value::Frequency(Frequency::from_hertz(sample_rate_hz)),
                    ),
                    ("timing_origin".into(), Value::Timestamp(origin)),
                ]);
                self.events
                    .push_back(DriverEvent::Event(Event::ScanSignalChunk(
                        ScanSignalChunkEvent {
                            device: self.hub,
                            stream,
                            channels: request.channels.clone(),
                            origin,
                            line: line as u64,
                            chunk: chunk_index as u64,
                            first_sample: first_sample as u64,
                            sample_count,
                            sample_rate: Frequency::from_hertz(sample_rate_hz),
                            sample_period: TimeInterval::from_seconds(sample_period_s),
                            samples: channel_samples(
                                &request.channels,
                                &profiles,
                                chunk_start,
                                chunk.len(),
                            ),
                            metadata,
                        },
                    )));
                self.events.push_back(DriverEvent::TokenProgress {
                    token,
                    progress: Progress {
                        completed: f64::from(chunk_index + 1),
                        total: total_chunks as f64,
                    },
                });
                first_sample += chunk.len() as u32;
                chunk_index += 1;
            }
        }
        Value::Map(BTreeMap::from([
            ("stream".into(), Value::I64(stream.0 as i64)),
            ("chunks".into(), Value::I64(chunk_index as i64)),
            ("channels".into(), Value::I64(request.channels.len() as i64)),
            (
                "channel_names".into(),
                Value::List(
                    request
                        .channels
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect(),
                ),
            ),
            (
                "samples_per_line".into(),
                Value::PixelCount(PixelCount::new(samples)),
            ),
            ("lines".into(), Value::PixelCount(PixelCount::new(lines))),
            ("chunk_size".into(), Value::I64(chunk_size as i64)),
            (
                "last_chunk_samples".into(),
                Value::I64(last_chunk_samples as i64),
            ),
            ("dropped_chunks".into(), Value::I64(0)),
            ("dropped_samples".into(), Value::I64(0)),
            ("overflowed".into(), Value::Bool(false)),
            (
                "sample_rate".into(),
                Value::Frequency(Frequency::from_hertz(sample_rate_hz)),
            ),
            (
                "sample_period".into(),
                Value::TimeInterval(TimeInterval::from_seconds(sample_period_s)),
            ),
            ("timing_origin".into(), Value::Timestamp(origin)),
        ]))
    }

    fn start_continuous_signal_stream(
        &mut self,
        token: DriverToken,
        request: ScanSignalStreamRequest,
    ) {
        let samples = map_i64(&request.timing, "samples_per_line")
            .unwrap_or(1024)
            .clamp(1, 65_536) as u32;
        let chunk_size = request.chunk_size.unwrap_or(256).clamp(1, samples as u64) as u32;
        let origin = Timestamp::from_controller_ticks(0);
        let scan = RasterScan::from_line_timing(&request.timing, samples);
        let sample_rate_hz = scan.sample_rate_hz;
        let sample_period_s = 1.0 / sample_rate_hz.max(f64::EPSILON);
        let channels = request.channels;
        let stream = StreamId(self.hub.0 .0 + token.0 + 10_000);
        let stop = Arc::new(AtomicBool::new(false));
        self.streams.insert(token, Arc::clone(&stop));
        let tx = self.worker_tx.clone();
        let model = Arc::clone(&self.model);
        let device = self.hub;
        thread::spawn(move || {
            let mut line = 0u64;
            let mut chunk_index = 0u64;
            let dropped_chunks = 0i64;
            let dropped_samples = 0i64;
            let period = Duration::from_secs_f64(
                (f64::from(samples) / sample_rate_hz.max(1.0)).clamp(0.005, 1.0),
            );
            while !stop.load(Ordering::Relaxed) {
                let started = Instant::now();
                // Sweep down the raster: successive lines are successive rows,
                // so a client filling a framebuffer reconstructs the same image
                // the capture/stream capabilities render.
                let rows = u64::from(scan.spec().height.max(1));
                // Re-read per line so detector changes apply to the next line.
                let snapshot = snapshot_model(&model);
                let profiles = render_scan_row_profiles(
                    &snapshot,
                    scan.spec(),
                    &channels,
                    samples,
                    (line % rows) as u32,
                    line / rows + 1,
                );
                let line_samples = profiles
                    .first()
                    .map(|(_, samples)| samples.as_slice())
                    .unwrap_or(&[]);
                let mut first_sample = 0u64;
                for chunk in line_samples.chunks(chunk_size as usize) {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let chunk_start = first_sample as usize;
                    let mut metadata = scan_metadata(&scan);
                    metadata.extend(detector_metadata(&snapshot));
                    metadata.extend([
                        ("source".into(), Value::String("sim_lsm".into())),
                        ("mode".into(), Value::String("continuous_line_scan".into())),
                        (
                            "channels".into(),
                            Value::List(channels.iter().cloned().map(Value::String).collect()),
                        ),
                        ("line".into(), Value::I64(line.min(i64::MAX as u64) as i64)),
                        (
                            "chunk_index".into(),
                            Value::I64(chunk_index.min(i64::MAX as u64) as i64),
                        ),
                        (
                            "first_sample".into(),
                            Value::I64(first_sample.min(i64::MAX as u64) as i64),
                        ),
                        ("dropped_chunks".into(), Value::I64(dropped_chunks)),
                        ("dropped_samples".into(), Value::I64(dropped_samples)),
                        ("overflowed".into(), Value::Bool(false)),
                        (
                            "chunk_size".into(),
                            Value::PixelCount(PixelCount::new(chunk_size)),
                        ),
                        (
                            "samples_per_line".into(),
                            Value::PixelCount(PixelCount::new(samples)),
                        ),
                        ("lines".into(), Value::Null),
                        (
                            "sample_rate".into(),
                            Value::Frequency(Frequency::from_hertz(sample_rate_hz)),
                        ),
                        ("timing_origin".into(), Value::Timestamp(origin)),
                    ]);
                    let sample_count = chunk.len() as u64;
                    let _ = tx.send(DriverEvent::Event(Event::ScanSignalChunk(
                        ScanSignalChunkEvent {
                            device,
                            stream,
                            channels: channels.clone(),
                            origin,
                            line,
                            chunk: chunk_index,
                            first_sample,
                            sample_count,
                            sample_rate: Frequency::from_hertz(sample_rate_hz),
                            sample_period: TimeInterval::from_seconds(sample_period_s),
                            samples: channel_samples(
                                &channels,
                                &profiles,
                                chunk_start,
                                chunk.len(),
                            ),
                            metadata,
                        },
                    )));
                    let _ = tx.send(DriverEvent::TokenProgress {
                        token,
                        progress: Progress {
                            completed: (chunk_index + 1) as f64,
                            total: 0.0,
                        },
                    });
                    first_sample = first_sample.saturating_add(sample_count);
                    chunk_index = chunk_index.saturating_add(1);
                }
                line = line.saturating_add(1);
                let spent = started.elapsed();
                if period > spent {
                    thread::sleep(period - spent);
                }
            }
            let _ = tx.send(DriverEvent::TokenCompleted {
                token,
                value: continuous_signal_completion(
                    stream,
                    line,
                    chunk_index,
                    samples,
                    chunk_size,
                    channels,
                    sample_rate_hz,
                    sample_period_s,
                    origin,
                    dropped_chunks,
                    dropped_samples,
                ),
            });
        });
    }

    fn publish_frame(
        &mut self,
        scan: &RasterScan,
        mode: &str,
        update: Option<FrameUpdate<'_>>,
    ) -> FrameHandle {
        let frame_index = self.frames.fetch_add(1, Ordering::Relaxed);
        let handle = FrameHandle {
            stream: StreamId(self.hub.0 .0),
            frame: FrameId(frame_index),
        };
        self.events.push_back(DriverEvent::FrameReady(lsm_frame(
            &self.model_snapshot(),
            scan,
            mode,
            self.hub,
            handle.stream,
            handle.frame,
            frame_index,
            update,
        )));
        handle
    }
}

impl Driver for SimLsmDriver {
    fn id(&self) -> DriverId {
        self.id
    }

    fn descriptors(&self) -> Vec<DeviceDescriptor> {
        vec![DeviceDescriptor {
            id: self.hub,
            driver: self.id,
            label: "sim-lsm".into(),
            vendor: Some("numanager".into()),
            model: Some("laser-scanning microscope simulation".into()),
            serial: None,
            kinds: vec![
                "hub".into(),
                "lsm".into(),
                "camera".into(),
                "simulator".into(),
            ],
            properties: vec![
                property_schema("model", "Model", ValueType::String, false),
                property_schema("sample_seed", "Sample seed", ValueType::I64, false),
                ratio_property("detector_gain", "Detector gain", 0.0, 500.0),
                ratio_property("detector_noise", "Detector noise", 0.0, 500.0),
            ],
            metadata: BTreeMap::from([
                (
                    "model".into(),
                    Value::String("confocal scan over shared cell-culture model".into()),
                ),
                (
                    "sample_seed".into(),
                    Value::I64(self.model_snapshot().sample.seed as i64),
                ),
                (
                    "detector_gain".into(),
                    Value::Ratio(Ratio::from_fraction(self.model_snapshot().detector_gain)),
                ),
                (
                    "detector_noise".into(),
                    Value::Ratio(Ratio::from_fraction(self.model_snapshot().detector_noise)),
                ),
            ]),
        }]
    }

    fn resources(&self) -> Vec<ResourceDescriptor> {
        vec![ResourceDescriptor {
            id: self.resource,
            driver: self.id,
            label: "sim-lsm-sample".into(),
            kind: "simulated.specimen".into(),
            metadata: BTreeMap::from([
                (
                    "model".into(),
                    Value::String("procedural adherent cell culture".into()),
                ),
                (
                    "sample_seed".into(),
                    Value::I64(self.model_snapshot().sample.seed as i64),
                ),
            ]),
        }]
    }

    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor> {
        if device == self.hub {
            vec![
                CapabilityDescriptor::with_name(
                    CAPTURE_CAPABILITY,
                    device,
                    CapabilityKind::ConfocalImageCapture,
                    "SimLsmConfocalImageCapture",
                    ValueType::Map,
                ),
                CapabilityDescriptor::with_name(
                    IMAGE_STREAM_CAPABILITY,
                    device,
                    CapabilityKind::ConfocalImageStream,
                    "SimLsmConfocalImageStream",
                    ValueType::Map,
                ),
                CapabilityDescriptor::with_name(
                    SIGNAL_STREAM_CAPABILITY,
                    device,
                    CapabilityKind::ScanSignalStream,
                    "SimLsmScanSignalStream",
                    ValueType::Map,
                ),
            ]
        } else {
            Vec::new()
        }
    }

    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch> {
        for command in &batch.commands {
            match command {
                Command::Invoke {
                    device,
                    capability,
                    request,
                } if *device == self.hub => {
                    if !capability_accepts(*capability, request) {
                        return Err(Error::new(
                            ErrorCode::InvalidCommand,
                            "Sim LSM capability received the wrong request type",
                        ));
                    }
                }
                Command::ReadProperty { device, key } if *device == self.hub => {
                    let _ = self.read_property(key)?;
                }
                Command::WriteProperty { device, key, value } if *device == self.hub => {
                    self.validate_write_property(key, value)?;
                }
                Command::Invoke { device, .. } if *device == self.hub => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "unsupported Sim LSM capability",
                    ));
                }
                _ => {}
            }
        }
        Ok(PreparedBatch {
            id: batch.id,
            commands: batch.commands.clone(),
            physical_transactions: Vec::new(),
        })
    }

    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken> {
        let token = self.next_token();
        let mut value = Value::Null;
        for command in prepared.commands {
            match command {
                Command::ReadProperty { device, key } if device == self.hub => {
                    value = self.read_property(&key)?;
                }
                Command::WriteProperty {
                    device,
                    key,
                    value: property_value,
                } if device == self.hub => {
                    value = self.write_property(&key, property_value)?;
                }
                Command::Invoke {
                    device,
                    capability,
                    request: CapabilityRequest::ConfocalImageCapture(request),
                } if device == self.hub && capability == CAPTURE_CAPABILITY => {
                    value = self.capture(request);
                }
                Command::Invoke {
                    device,
                    capability,
                    request: CapabilityRequest::ConfocalImageStream(request),
                } if device == self.hub
                    && capability == IMAGE_STREAM_CAPABILITY
                    && continuous_stream_requested(&request) =>
                {
                    self.start_continuous_image_stream(token, request);
                    return Ok(token);
                }
                Command::Invoke {
                    device,
                    capability,
                    request: CapabilityRequest::ConfocalImageStream(request),
                } if device == self.hub && capability == IMAGE_STREAM_CAPABILITY => {
                    value = self.image_stream(token, request);
                }
                Command::Invoke {
                    device,
                    capability,
                    request: CapabilityRequest::ScanSignalStream(request),
                } if device == self.hub
                    && capability == SIGNAL_STREAM_CAPABILITY
                    && continuous_signal_stream_requested(&request) =>
                {
                    self.start_continuous_signal_stream(token, request);
                    return Ok(token);
                }
                Command::Invoke {
                    device,
                    capability,
                    request: CapabilityRequest::ScanSignalStream(request),
                } if device == self.hub && capability == SIGNAL_STREAM_CAPABILITY => {
                    value = self.signal_stream(token, request);
                }
                _ => {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "unsupported Sim LSM command",
                    ));
                }
            }
        }
        self.events
            .push_back(DriverEvent::TokenCompleted { token, value });
        Ok(token)
    }

    fn poll(&mut self) -> Vec<DriverEvent> {
        while let Ok(event) = self.worker_rx.try_recv() {
            if let DriverEvent::TokenCompleted { token, .. }
            | DriverEvent::TokenFailed { token, .. } = &event
            {
                self.streams.remove(token);
            }
            self.events.push_back(event);
        }
        self.events.drain(..).collect()
    }

    fn cancel(&mut self, token: DriverToken) -> CancelResult {
        if let Some(stop) = self.streams.remove(&token) {
            stop.store(true, Ordering::Relaxed);
            CancelResult::Cancelled
        } else {
            CancelResult::Unsupported
        }
    }
}

impl SimLsmDriver {
    fn read_property(&self, key: &str) -> Result<Value> {
        match key {
            "model" => Ok(Value::String(
                "confocal scan over shared cell-culture model".into(),
            )),
            "sample_seed" => Ok(Value::I64(self.model_snapshot().sample.seed as i64)),
            "detector_gain" => Ok(Value::Ratio(Ratio::from_fraction(
                self.model_snapshot().detector_gain,
            ))),
            "detector_noise" => Ok(Value::Ratio(Ratio::from_fraction(
                self.model_snapshot().detector_noise,
            ))),
            other => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Sim LSM property {other}"),
            )),
        }
    }

    fn write_property(&mut self, key: &str, value: Value) -> Result<Value> {
        let ratio = self.validate_write_property(key, &value)?;
        match key {
            "detector_gain" => self.model_lock().detector_gain = ratio,
            "detector_noise" => self.model_lock().detector_noise = ratio,
            _ => unreachable!("validate_write_property accepted an unsupported key"),
        }
        self.read_property(key)
    }

    fn validate_write_property(&self, key: &str, value: &Value) -> Result<f64> {
        match key {
            "detector_gain" | "detector_noise" => {
                let Value::Ratio(value) = value else {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        format!("{key} expects a Ratio value"),
                    ));
                };
                let fraction = value.fraction();
                if !fraction.is_finite() || !(0.0..=5.0).contains(&fraction) {
                    return Err(Error::new(
                        ErrorCode::InvalidProperty,
                        format!("{key} must be finite and between 0% and 500%"),
                    ));
                }
                Ok(fraction)
            }
            other => Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("unknown Sim LSM property {other}"),
            )),
        }
    }
}

#[derive(Debug, Clone)]
struct RasterScan {
    width: u32,
    height: u32,
    reconstruction_width: u32,
    reconstruction_height: u32,
    pixel_size_um: f64,
    sample_rate_hz: f64,
    line_dwell_s: f64,
    laser_power: f64,
    numerical_aperture: f64,
    magnification: f64,
    stage_x_um: f64,
    stage_y_um: f64,
    stage_z_um: f64,
    pixel_format: LsmPixelFormat,
    fast_axis: String,
    slow_axis: String,
    detectors: Vec<String>,
    laser_gate_enabled: bool,
    accumulation: String,
    background_subtraction: bool,
}

impl RasterScan {
    fn from_maps(scan: &BTreeMap<String, Value>, reconstruction: &BTreeMap<String, Value>) -> Self {
        let width = map_pixel_count(scan, "width")
            .or_else(|| map_pixel_count(reconstruction, "image_width"))
            .unwrap_or(512)
            .clamp(1, 2048);
        let height = map_pixel_count(scan, "height")
            .or_else(|| map_pixel_count(reconstruction, "image_height"))
            .unwrap_or(512)
            .clamp(1, 2048);
        let timing = resolved_line_timing(scan, width, 100_000.0);
        Self {
            width,
            height,
            reconstruction_width: map_pixel_count(reconstruction, "image_width")
                .unwrap_or(width)
                .clamp(1, 2048),
            reconstruction_height: map_pixel_count(reconstruction, "image_height")
                .unwrap_or(height)
                .clamp(1, 2048),
            pixel_size_um: map_f64(scan, "pixel_size_um").unwrap_or(0.325),
            sample_rate_hz: timing.sample_rate_hz,
            line_dwell_s: timing.line_dwell_s,
            laser_power: effective_laser_power(scan),
            numerical_aperture: map_numerical_aperture(scan, "numerical_aperture").unwrap_or(0.45),
            magnification: map_f64(scan, "magnification").unwrap_or(20.0),
            stage_x_um: map_position_um(scan, "stage_x").unwrap_or(0.0),
            stage_y_um: map_position_um(scan, "stage_y").unwrap_or(0.0),
            stage_z_um: map_position_um(scan, "stage_z").unwrap_or(4_250.0),
            pixel_format: map_pixel_format(reconstruction, "pixel_format"),
            fast_axis: map_string(scan, "fast_axis").unwrap_or_else(|| "x".into()),
            slow_axis: map_string(scan, "slow_axis").unwrap_or_else(|| "y".into()),
            detectors: map_string_list(scan, "detectors")
                .or_else(|| map_string(scan, "detector").map(|detector| vec![detector]))
                .unwrap_or_else(|| vec!["counter0".into()]),
            laser_gate_enabled: map_bool(scan, "laser_gate_enabled").unwrap_or(true),
            accumulation: map_string(reconstruction, "accumulation")
                .unwrap_or_else(|| "sum".into()),
            background_subtraction: map_bool(reconstruction, "background_subtraction")
                .unwrap_or(false),
        }
    }

    fn from_line_timing(timing: &BTreeMap<String, Value>, samples: u32) -> Self {
        let resolved = resolved_line_timing(timing, samples, 100_000.0);
        Self {
            width: samples.clamp(1, 65_536),
            // A line request without a height is a single line at the scan
            // centre; with one, it is a row sweep over a raster that tall.
            height: map_i64(timing, "height").unwrap_or(1).clamp(1, 65_536) as u32,
            reconstruction_width: samples.clamp(1, 65_536),
            reconstruction_height: 1,
            pixel_size_um: map_f64(timing, "pixel_size_um").unwrap_or(0.325),
            sample_rate_hz: resolved.sample_rate_hz,
            line_dwell_s: resolved.line_dwell_s,
            laser_power: effective_laser_power(timing),
            numerical_aperture: map_numerical_aperture(timing, "numerical_aperture")
                .unwrap_or(0.45),
            magnification: map_f64(timing, "magnification").unwrap_or(20.0),
            stage_x_um: map_position_um(timing, "stage_x").unwrap_or(0.0),
            stage_y_um: map_position_um(timing, "stage_y").unwrap_or(0.0),
            stage_z_um: map_position_um(timing, "stage_z").unwrap_or(4_250.0),
            pixel_format: LsmPixelFormat::Mono16,
            fast_axis: map_string(timing, "fast_axis").unwrap_or_else(|| "x".into()),
            slow_axis: map_string(timing, "slow_axis").unwrap_or_else(|| "y".into()),
            detectors: map_string_list(timing, "detectors")
                .or_else(|| map_string(timing, "detector").map(|detector| vec![detector]))
                .unwrap_or_else(|| vec!["counter0".into()]),
            laser_gate_enabled: map_bool(timing, "laser_gate_enabled").unwrap_or(true),
            accumulation: "sum".into(),
            background_subtraction: false,
        }
    }

    fn spec(&self) -> LsmRasterSpec {
        LsmRasterSpec {
            center_x_um: self.stage_x_um,
            center_y_um: self.stage_y_um,
            z_um: self.stage_z_um,
            width: self.width,
            height: self.height,
            pixel_size_um: self.pixel_size_um,
            laser_power: self.laser_power,
            numerical_aperture: self.numerical_aperture,
            magnification: self.magnification,
        }
    }

    fn frame_spec(&self) -> LsmRasterSpec {
        LsmRasterSpec {
            center_x_um: self.stage_x_um,
            center_y_um: self.stage_y_um,
            z_um: self.stage_z_um,
            width: self.frame_width(),
            height: self.frame_height(),
            pixel_size_um: self.frame_pixel_size_um(),
            laser_power: self.laser_power,
            numerical_aperture: self.numerical_aperture,
            magnification: self.magnification,
        }
    }

    fn frame_width(&self) -> u32 {
        self.reconstruction_width.max(1)
    }

    fn frame_height(&self) -> u32 {
        self.reconstruction_height.max(1)
    }

    fn frame_pixel_size_um(&self) -> f64 {
        self.pixel_size_um * f64::from(self.width.max(1)) / f64::from(self.frame_width())
    }
}

#[derive(Debug, Clone, Copy)]
enum LsmPixelFormat {
    Mono8,
    Mono16,
}

impl LsmPixelFormat {
    fn name(self) -> &'static str {
        match self {
            Self::Mono8 => "Mono8",
            Self::Mono16 => "Mono16",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct FrameUpdate<'a> {
    policy: &'a str,
    overwrite_previous_pixels: bool,
}

#[derive(Debug, Clone, Copy)]
struct ResolvedLineTiming {
    sample_rate_hz: f64,
    line_dwell_s: f64,
}

fn resolved_line_timing(
    fields: &BTreeMap<String, Value>,
    samples_per_line: u32,
    default_sample_rate_hz: f64,
) -> ResolvedLineTiming {
    let samples = f64::from(samples_per_line.max(1));
    let explicit_rate =
        map_frequency_hz(fields, "sample_rate").filter(|value| value.is_finite() && *value > 0.0);
    let explicit_dwell = map_time_seconds(fields, "line_dwell")
        .or_else(|| map_f64(fields, "line_dwell_us").map(|value| value * 1e-6))
        .filter(|value| value.is_finite() && *value > 0.0);

    let sample_rate_hz = explicit_rate
        .or_else(|| explicit_dwell.map(|dwell| samples / dwell))
        .unwrap_or(default_sample_rate_hz)
        .max(f64::EPSILON);
    let line_dwell_s = explicit_dwell.unwrap_or(samples / sample_rate_hz);

    ResolvedLineTiming {
        sample_rate_hz,
        line_dwell_s,
    }
}

fn continuous_stream_requested(request: &ConfocalImageStreamRequest) -> bool {
    map_i64(&request.scan, "frames").is_some_and(|frames| frames <= 0)
}

fn continuous_signal_stream_requested(request: &ScanSignalStreamRequest) -> bool {
    map_i64(&request.timing, "lines").is_some_and(|lines| lines <= 0)
}

fn image_stream_completion(
    stream: StreamId,
    frames: u64,
    scan: RasterScan,
    update_policy: &str,
    overwrite: bool,
) -> Value {
    Value::Map(BTreeMap::from([
        ("stream".into(), Value::I64(stream.0 as i64)),
        ("frames".into(), Value::I64(frames as i64)),
        (
            "width".into(),
            Value::PixelCount(PixelCount::new(scan.frame_width())),
        ),
        (
            "height".into(),
            Value::PixelCount(PixelCount::new(scan.frame_height())),
        ),
        (
            "pixel_format".into(),
            Value::String(scan.pixel_format.name().into()),
        ),
        ("update_policy".into(), Value::String(update_policy.into())),
        ("overwrite_previous_pixels".into(), Value::Bool(overwrite)),
        ("completion_basis".into(), Value::String("cancelled".into())),
    ]))
}

fn continuous_signal_completion(
    stream: StreamId,
    lines: u64,
    chunks: u64,
    samples: u32,
    chunk_size: u32,
    channels: Vec<String>,
    sample_rate_hz: f64,
    sample_period_s: f64,
    origin: Timestamp,
    dropped_chunks: i64,
    dropped_samples: i64,
) -> Value {
    Value::Map(BTreeMap::from([
        ("stream".into(), Value::I64(stream.0 as i64)),
        (
            "lines".into(),
            Value::I64(lines.min(i64::MAX as u64) as i64),
        ),
        (
            "chunks".into(),
            Value::I64(chunks.min(i64::MAX as u64) as i64),
        ),
        ("channels".into(), Value::I64(channels.len() as i64)),
        (
            "channel_names".into(),
            Value::List(channels.into_iter().map(Value::String).collect()),
        ),
        (
            "samples_per_line".into(),
            Value::PixelCount(PixelCount::new(samples)),
        ),
        ("chunk_size".into(), Value::I64(i64::from(chunk_size))),
        ("dropped_chunks".into(), Value::I64(dropped_chunks)),
        ("dropped_samples".into(), Value::I64(dropped_samples)),
        ("overflowed".into(), Value::Bool(false)),
        (
            "sample_rate".into(),
            Value::Frequency(Frequency::from_hertz(sample_rate_hz)),
        ),
        (
            "sample_period".into(),
            Value::TimeInterval(TimeInterval::from_seconds(sample_period_s)),
        ),
        ("timing_origin".into(), Value::Timestamp(origin)),
        ("completion_basis".into(), Value::String("cancelled".into())),
    ]))
}

fn lsm_frame(
    model: &LsmFluorescenceConfig,
    scan: &RasterScan,
    mode: &str,
    device: DeviceId,
    stream: StreamId,
    frame: FrameId,
    frame_index: u64,
    update: Option<FrameUpdate<'_>>,
) -> Frame {
    let image = render_confocal_raster_for_detectors(
        model,
        scan.frame_spec(),
        &scan.detectors,
        frame_index,
    );
    let mut metadata = scan_metadata(scan);
    metadata.extend(detector_metadata(model));
    metadata.extend([
        ("source".into(), Value::String("sim_lsm".into())),
        ("mode".into(), Value::String(mode.into())),
        ("frame_index".into(), Value::I64(frame_index as i64)),
        (
            "reconstruction_accumulation".into(),
            Value::String(scan.accumulation.clone()),
        ),
        (
            "background_subtraction".into(),
            Value::Bool(scan.background_subtraction),
        ),
        (
            "saturated_pixels".into(),
            Value::PixelCount(PixelCount::new(image.saturated)),
        ),
    ]);
    if let Some(update) = update {
        metadata.insert("update_policy".into(), Value::String(update.policy.into()));
        metadata.insert(
            "overwrite_previous_pixels".into(),
            Value::Bool(update.overwrite_previous_pixels),
        );
        if update.policy == "dirty_region" {
            metadata.insert(
                "dirty_region_basis".into(),
                Value::String("horizontal_strip_full_frame_payload".into()),
            );
            let dirty = dirty_region(scan, frame_index);
            metadata.insert(
                "dirty_x".into(),
                Value::PixelCount(PixelCount::new(dirty.x)),
            );
            metadata.insert(
                "dirty_y".into(),
                Value::PixelCount(PixelCount::new(dirty.y)),
            );
            metadata.insert(
                "dirty_width".into(),
                Value::PixelCount(PixelCount::new(dirty.width)),
            );
            metadata.insert(
                "dirty_height".into(),
                Value::PixelCount(PixelCount::new(dirty.height)),
            );
        }
    }
    Frame {
        handle: FrameHandle { stream, frame },
        device,
        width: image.width,
        height: image.height,
        pixel_format: scan.pixel_format.name().into(),
        data: encode_image(&image.data, scan.pixel_format),
        metadata,
        buffer: FrameBufferSpec::default(),
    }
}

#[derive(Debug, Clone, Copy)]
struct DirtyRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

fn dirty_region(scan: &RasterScan, frame_index: u64) -> DirtyRegion {
    let height = scan.frame_height();
    let width = scan.frame_width();
    let strip_count = height.clamp(1, 8);
    let strip = ((frame_index.saturating_sub(1)) % u64::from(strip_count)) as u32;
    let strip_height = height.div_ceil(strip_count).max(1);
    let y = strip.saturating_mul(strip_height).min(height - 1);
    DirtyRegion {
        x: 0,
        y,
        width,
        height: strip_height.min(height - y),
    }
}

fn scan_metadata(scan: &RasterScan) -> BTreeMap<String, Value> {
    BTreeMap::from([
        (
            "stage_x".into(),
            Value::Position(Position::from_micrometers(scan.stage_x_um)),
        ),
        (
            "stage_y".into(),
            Value::Position(Position::from_micrometers(scan.stage_y_um)),
        ),
        (
            "stage_z".into(),
            Value::Position(Position::from_micrometers(scan.stage_z_um)),
        ),
        (
            "sample_pixel_size".into(),
            Value::Position(Position::from_micrometers(scan.pixel_size_um)),
        ),
        (
            "reconstruction_pixel_size".into(),
            Value::Position(Position::from_micrometers(scan.frame_pixel_size_um())),
        ),
        (
            "sample_rate".into(),
            Value::Frequency(Frequency::from_hertz(scan.sample_rate_hz)),
        ),
        (
            "line_dwell".into(),
            Value::TimeInterval(TimeInterval::from_seconds(scan.line_dwell_s)),
        ),
        (
            "scan_width".into(),
            Value::PixelCount(PixelCount::new(scan.width)),
        ),
        (
            "scan_height".into(),
            Value::PixelCount(PixelCount::new(scan.height)),
        ),
        (
            "reconstruction_width".into(),
            Value::PixelCount(PixelCount::new(scan.reconstruction_width)),
        ),
        (
            "reconstruction_height".into(),
            Value::PixelCount(PixelCount::new(scan.reconstruction_height)),
        ),
        ("fast_axis".into(), Value::String(scan.fast_axis.clone())),
        ("slow_axis".into(), Value::String(scan.slow_axis.clone())),
        (
            "detectors".into(),
            Value::List(scan.detectors.iter().cloned().map(Value::String).collect()),
        ),
        (
            "laser_gate_enabled".into(),
            Value::Bool(scan.laser_gate_enabled),
        ),
        (
            "laser_power".into(),
            Value::Ratio(Ratio::from_fraction(scan.laser_power)),
        ),
        ("magnification".into(), Value::F64(scan.magnification)),
        (
            "numerical_aperture".into(),
            Value::NumericalAperture(NumericalAperture::new(scan.numerical_aperture)),
        ),
    ])
}

fn detector_metadata(model: &LsmFluorescenceConfig) -> BTreeMap<String, Value> {
    BTreeMap::from([
        (
            "detector_gain".into(),
            Value::Ratio(Ratio::from_fraction(model.detector_gain)),
        ),
        (
            "detector_noise".into(),
            Value::Ratio(Ratio::from_fraction(model.detector_noise)),
        ),
    ])
}

fn capability_accepts(capability: CapabilityId, request: &CapabilityRequest) -> bool {
    matches!(
        (capability, request),
        (
            CAPTURE_CAPABILITY,
            CapabilityRequest::ConfocalImageCapture(_)
        ) | (
            IMAGE_STREAM_CAPABILITY,
            CapabilityRequest::ConfocalImageStream(_)
        ) | (
            SIGNAL_STREAM_CAPABILITY,
            CapabilityRequest::ScanSignalStream(_)
        )
    )
}

fn property_schema(
    key: &str,
    display_name: &str,
    value_type: ValueType,
    writable: bool,
) -> PropertySchema {
    PropertySchema {
        key: key.into(),
        display_name: display_name.into(),
        value_type,
        unit: None,
        range: None,
        increment: None,
        enum_values: Vec::new(),
        readable: true,
        writable,
        volatile: false,
        sequenceable: false,
        hardware_address: None,
    }
}

fn ratio_property(
    key: &str,
    display_name: &str,
    min_percent: f64,
    max_percent: f64,
) -> PropertySchema {
    let mut schema = property_schema(key, display_name, ValueType::Ratio, true);
    schema.unit = Some(Unit("percent".into()));
    schema.range = Some(Range {
        min: Value::Ratio(Ratio::from_percent(min_percent)),
        max: Value::Ratio(Ratio::from_percent(max_percent)),
    });
    schema
}

fn map_pixel_count(map: &BTreeMap<String, Value>, key: &str) -> Option<u32> {
    match map.get(key) {
        Some(Value::PixelCount(value)) => Some(value.0),
        Some(Value::I64(value)) => Some((*value).clamp(1, u32::MAX as i64) as u32),
        _ => None,
    }
}

fn map_f64(map: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
    match map.get(key) {
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn map_ratio(map: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
    match map.get(key) {
        Some(Value::Ratio(value)) => Some(value.percent() / 100.0),
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn map_i64(map: &BTreeMap<String, Value>, key: &str) -> Option<i64> {
    match map.get(key) {
        Some(Value::I64(value)) => Some(*value),
        Some(Value::PixelCount(value)) => Some(i64::from(value.0)),
        _ => None,
    }
}

fn map_frequency_hz(map: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
    match map.get(key) {
        Some(Value::Frequency(value)) => Some(value.hertz()),
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn map_time_seconds(map: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
    match map.get(key) {
        Some(Value::TimeInterval(value)) => Some(value.seconds()),
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn map_bool(map: &BTreeMap<String, Value>, key: &str) -> Option<bool> {
    match map.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn map_string(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    match map.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn map_string_list(map: &BTreeMap<String, Value>, key: &str) -> Option<Vec<String>> {
    match map.get(key) {
        Some(Value::List(values)) => {
            let strings = values
                .iter()
                .filter_map(|value| match value {
                    Value::String(value) => Some(value.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            (!strings.is_empty()).then_some(strings)
        }
        _ => None,
    }
}

fn effective_laser_power(map: &BTreeMap<String, Value>) -> f64 {
    let power = map_ratio(map, "laser_power").unwrap_or(0.85);
    if map_bool(map, "laser_gate_enabled").unwrap_or(true) {
        power
    } else {
        0.0
    }
}

fn map_numerical_aperture(map: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
    match map.get(key) {
        Some(Value::NumericalAperture(value)) => Some(value.value()),
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn map_position_um(map: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
    match map.get(key) {
        Some(Value::Position(value)) => Some(value.micrometers()),
        Some(Value::F64(value)) => Some(*value),
        Some(Value::I64(value)) => Some(*value as f64),
        _ => None,
    }
}

fn map_pixel_format(map: &BTreeMap<String, Value>, key: &str) -> LsmPixelFormat {
    match map.get(key) {
        Some(Value::String(value)) if value.eq_ignore_ascii_case("Mono8") => LsmPixelFormat::Mono8,
        _ => LsmPixelFormat::Mono16,
    }
}

fn encode_image(samples: &[u16], pixel_format: LsmPixelFormat) -> Vec<u8> {
    match pixel_format {
        LsmPixelFormat::Mono8 => samples.iter().map(|sample| (sample >> 8) as u8).collect(),
        LsmPixelFormat::Mono16 => {
            let mut data = Vec::with_capacity(samples.len() * 2);
            for sample in samples {
                data.extend_from_slice(&sample.to_le_bytes());
            }
            data
        }
    }
}

fn channel_samples(
    channels: &[String],
    profiles: &[(String, Vec<u16>)],
    start: usize,
    len: usize,
) -> BTreeMap<String, Vec<Value>> {
    channels
        .iter()
        .map(|channel| {
            let profile = profiles
                .iter()
                .find(|(name, _)| name == channel)
                .map(|(_, samples)| samples.as_slice())
                .unwrap_or(&[]);
            let end = start.saturating_add(len).min(profile.len());
            let values = profile[start.min(end)..end]
                .iter()
                .map(|sample| sample_value(channel, *sample))
                .collect();
            (channel.clone(), values)
        })
        .collect()
}

fn sample_value(channel: &str, sample: u16) -> Value {
    if channel.starts_with("ai") {
        let volts = (f64::from(sample) / f64::from(u16::MAX)) * 5.0;
        Value::Voltage(Voltage::from_volts(volts))
    } else {
        Value::I64(i64::from(sample))
    }
}
