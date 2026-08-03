use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use numanager_core::runtime::{LocalRuntime, Runtime, Subscription};
use numanager_core::{
    CapabilityKind, Command, DeviceDescriptor, Error, ErrorCode, Event, EventFilter, EventKind,
    FilterSelectRequest, Frame, Frequency, OperationId, OperationStatus, Position, Ratio, Result,
    StateSet, StateWrite, TimeInterval, Value,
};
use slint::{
    Image, ModelRc, Rgb8Pixel, SharedPixelBuffer, SharedString, Timer, TimerMode, VecModel,
};

slint::slint! {
import { Button, CheckBox, ComboBox, ScrollView, Slider, VerticalBox, HorizontalBox } from "std-widgets.slint";

export component LsmWindow inherits Window {
    title: "numanager LSM";
    preferred-width: 1180px;
    preferred-height: 760px;

    in property <image> preview;
    in property <image> line-profile;
    in property <string> histogram-path;
    in property <[string]> source-options;
    in-out property <int> selected-source;
    in-out property <float> width-value: 512;
    in-out property <float> height-value: 512;
    in-out property <float> rate-value: 100;
    in-out property <float> dwell-value: 500;
    in-out property <float> chunk-value: 256;
    // Display-only overlay: highlight the row the scan is currently on.
    in-out property <bool> mark-scan-line: true;
    in-out property <bool> use-counter: true;
    in-out property <bool> use-analog: true;
    in-out property <bool> laser-gate: true;
    in-out property <float> detector-gain-value: 100;
    in-out property <float> detector-noise-value: 100;
    in-out property <float> stage-x-value: 0;
    in-out property <float> stage-y-value: 0;
    in-out property <float> focus-z-value: 4250;
    in-out property <float> lamp-power-value: 100;
    in-out property <float> objective-position-value: 2;
    in property <bool> line-scanning;
    in property <string> status;
    in property <bool> status-error;
    in property <string> source-summary;
    // False when the selected source reports it cannot execute a scan, so the
    // acquisition buttons produce no data. `backend-note` carries the reason.
    in property <bool> backend-live: true;
    in property <string> backend-note;
    in property <string> request-summary;
    in property <string> frame-summary;
    in property <string> line-summary;
    in property <string> progress-summary;

    callback snapshot();
    callback line-scan();
    callback source-changed(int);

    HorizontalBox {
        padding: 12px;
        spacing: 12px;

        VerticalBox {
            spacing: 8px;
            horizontal-stretch: 1;
            min-width: 420px;

            HorizontalBox {
                spacing: 8px;
                Button {
                    text: "Snapshot";
                    enabled: !root.line-scanning;
                    clicked => { root.snapshot(); }
                }
                Button {
                    text: root.line-scanning ? "Stop" : "Line scanning";
                    primary: root.line-scanning;
                    clicked => { root.line-scan(); }
                }
                Rectangle {
                    width: 12px;
                    height: 12px;
                    border-radius: 6px;
                    y: (parent.height - self.height) / 2;
                    background: root.line-scanning ? #e0463c : #8d98a7;
                }
                Text {
                    text: root.line-scanning ? "LINE SCAN" : "idle";
                    color: root.line-scanning ? #e0463c : #536170;
                    font-weight: 700;
                    vertical-alignment: center;
                }
            }

            Text {
                text: "Snapshot: one complete image, as if the LSM were a camera. Line scanning: the same image built one scan line at a time, refreshed as the sweep goes down the frame.";
                wrap: word-wrap;
                color: #536170;
            }

            if !root.backend-live: Rectangle {
                background: #fff4ed;
                border-color: #e08a3c;
                border-width: 1px;
                border-radius: 4px;
                VerticalLayout {
                    padding: 8px;
                    spacing: 2px;
                    Text {
                        text: "This source cannot scan — Snapshot and Line scanning will produce no data.";
                        color: #8a4b12;
                        font-weight: 700;
                        wrap: word-wrap;
                    }
                    Text { text: root.backend-note; color: #6b4a2a; wrap: word-wrap; }
                }
            }

            Rectangle {
                vertical-stretch: 1;
                background: #0b0f14;

                // The scan viewport keeps the configured scan aspect (square for
                // a 512x512 raster) and is centred, rather than stretching to
                // whatever shape the pane happens to be.
                Rectangle {
                    property <float> aspect: root.width-value / max(root.height-value, 1.0);
                    width: min(parent.width, parent.height * self.aspect);
                    height: self.width / self.aspect;
                    x: (parent.width - self.width) / 2;
                    y: (parent.height - self.height) / 2;
                    background: #10151b;
                    border-color: #2d3745;
                    border-width: 1px;
                    Image {
                        source: root.preview;
                        image-fit: contain;
                        image-rendering: ImageRendering.pixelated;
                        width: parent.width;
                        height: parent.height;
                    }
                }
            }

            Text { text: "Most recent scan line — detector signal along the row being scanned"; color: #536170; }
            Rectangle {
                height: 110px;
                background: #151b23;
                border-color: #303a47;
                border-width: 1px;
                Image {
                    source: root.line-profile;
                    image-fit: fill;
                    width: parent.width;
                    height: parent.height;
                }
            }

            Text { text: "Intensity histogram 0 -> 255 — whole image, not just the current line"; color: #536170; }
            Rectangle {
                height: 110px;
                background: #10141a;
                border-color: #303844;
                border-width: 1px;

                Path {
                    x: 1px;
                    y: 1px;
                    width: parent.width - 2px;
                    height: parent.height - 2px;
                    viewbox-x: 0;
                    viewbox-y: 0;
                    viewbox-width: 255;
                    viewbox-height: 100;
                    fit: fill;
                    commands: root.histogram-path;
                    fill: #2f80ed80;
                    stroke: #7ab2ff;
                    stroke-width: 1px;
                }
            }
        }

        // The control column is taller than the window on any realistic scan
        // configuration, so it scrolls vertically. Pinning the viewport to the
        // visible width keeps the word-wrapped summaries wrapping at the panel
        // width instead of forcing a horizontal scrollbar.
        ScrollView {
            width: 380px;
            viewport-width: self.visible-width;

            VerticalBox {
                spacing: 10px;
                alignment: start;
                padding-right: 14px;

                Text { text: "Source"; font-size: 16px; font-weight: 700; }
                ComboBox {
                    model: root.source-options;
                    current-index <=> root.selected-source;
                    selected(_) => { root.source-changed(root.selected-source); }
                }
                Text { text: "Backend status"; font-weight: 700; color: #536170; }
                Text { text: root.source-summary; wrap: word-wrap; color: #536170; }

                Text { text: "Scan"; font-size: 20px; font-weight: 700; }

                Text { text: "Width " + round(root.width-value); color: #536170; }
                Slider { minimum: 64; maximum: 2048; value <=> root.width-value; }

                Text { text: "Height " + round(root.height-value); color: #536170; }
                Slider { minimum: 1; maximum: 2048; value <=> root.height-value; }

                Text { text: "Rate " + round(root.rate-value) + " kHz"; color: #536170; }
                Slider { minimum: 1; maximum: 1000; value <=> root.rate-value; }

                Text { text: "Line dwell " + round(root.dwell-value) + " us"; color: #536170; }
                Slider { minimum: 10; maximum: 5000; value <=> root.dwell-value; }

                Text { text: "Chunk " + round(root.chunk-value); color: #536170; }
                Slider { minimum: 16; maximum: 4096; value <=> root.chunk-value; }

                CheckBox {
                    text: "Mark current scan line";
                    checked <=> root.mark-scan-line;
                }

                Text { text: "Devices"; font-size: 16px; font-weight: 700; }
                CheckBox {
                    text: "Laser gate do0";
                    checked <=> root.laser-gate;
                }
                CheckBox {
                    text: "Counter detector counter0";
                    checked <=> root.use-counter;
                }
                CheckBox {
                    text: "Analog monitor ai0";
                    checked <=> root.use-analog;
                }

                Text { text: "Detector"; font-size: 16px; font-weight: 700; }
                Text { text: "Gain " + round(root.detector-gain-value) + "%"; color: #536170; }
                Slider { minimum: 0; maximum: 500; value <=> root.detector-gain-value; }

                Text { text: "Noise " + round(root.detector-noise-value) + "%"; color: #536170; }
                Slider { minimum: 0; maximum: 500; value <=> root.detector-noise-value; }

                Text { text: "Shared Scene"; font-size: 16px; font-weight: 700; }
                Text { text: "X " + round(root.stage-x-value) + " um"; color: #536170; }
                Slider { minimum: -1000; maximum: 1000; value <=> root.stage-x-value; }

                Text { text: "Y " + round(root.stage-y-value) + " um"; color: #536170; }
                Slider { minimum: -1000; maximum: 1000; value <=> root.stage-y-value; }

                Text { text: "Focus " + round(root.focus-z-value) + " um"; color: #536170; }
                Slider { minimum: 0; maximum: 8500; value <=> root.focus-z-value; }

                Text { text: "Lamp " + round(root.lamp-power-value) + "%"; color: #536170; }
                Slider { minimum: 0; maximum: 100; value <=> root.lamp-power-value; }

                Text { text: "Objective " + round(root.objective-position-value); color: #536170; }
                Slider { minimum: 1; maximum: 3; value <=> root.objective-position-value; }

                Rectangle { height: 1px; background: #d5dbe3; }

                Text { text: "Last operation"; font-size: 16px; font-weight: 700; }
                Text {
                    text: "What the buttons above actually submitted to the runtime, and what came back. This example doubles as API documentation, so the typed request and the frame/stream metadata are shown verbatim.";
                    wrap: word-wrap;
                    color: #536170;
                }

                Text { text: "Request"; font-weight: 700; }
                Text { text: root.request-summary; wrap: word-wrap; color: #26313d; }
                Text { text: "Frame"; font-weight: 700; }
                Text { text: root.frame-summary; wrap: word-wrap; color: #26313d; }
                Text { text: "Line"; font-weight: 700; }
                Text { text: root.line-summary; wrap: word-wrap; color: #26313d; }
                Text { text: "Progress"; font-weight: 700; }
                Text { text: root.progress-summary; wrap: word-wrap; color: #26313d; }
                Text { text: root.status; wrap: word-wrap; color: root.status-error ? #b42318 : #1d6b42; }
            }
        }
    }
}
}

pub fn run() -> Result<()> {
    let source = driver_choice();
    if std::env::args().any(|argument| argument == "--smoke") {
        return smoke(&source);
    }

    let (runtime, hub) = crate::lsm_common::runtime_for_source(&source)?;
    let frame_events =
        runtime.subscribe(EventFilter::device(&hub).with_kind(EventKind::FrameReady));
    let signal_events =
        runtime.subscribe(EventFilter::device(&hub).with_kind(EventKind::ScanSignalChunk));
    let operation_events =
        runtime.subscribe(EventFilter::device(&hub).with_kind(EventKind::OperationChanged));
    let source_summary = source_summary(&hub);
    let (backend_live, backend_note) = backend_live_state(&hub);
    let ui = LsmWindow::new().map_err(|error| Error::new(ErrorCode::Driver, error.to_string()))?;
    let app = Rc::new(RefCell::new(LsmGui {
        runtime,
        hub,
        frame_events,
        signal_events,
        operation_events,
        line_scanning: false,
        line_operation: None,
        line_buffer: LineFramebuffer::default(),
    }));

    ui.set_preview(blank_image(512, 512));
    ui.set_line_profile(blank_image(512, 96));
    ui.set_source_options(source_options());
    ui.set_selected_source(source_index(&source));
    ui.set_status(format!("ready: {source}").into());
    ui.set_source_summary(source_summary.into());
    ui.set_backend_live(backend_live);
    ui.set_backend_note(backend_note.into());
    ui.set_status_error(false);
    ui.set_request_summary("no request submitted yet".into());
    ui.set_frame_summary("no frame".into());
    ui.set_line_summary("counter0 + ai0, chunk 256".into());
    ui.set_progress_summary("idle".into());

    {
        let ui_weak = ui.as_weak();
        let app = Rc::clone(&app);
        ui.on_source_changed(move |index| {
            if let Some(ui) = ui_weak.upgrade() {
                report(&ui, app.borrow_mut().set_source(&ui, index));
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let app = Rc::clone(&app);
        ui.on_snapshot(move || {
            if let Some(ui) = ui_weak.upgrade() {
                report(&ui, app.borrow_mut().snapshot(&ui));
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let app = Rc::clone(&app);
        ui.on_line_scan(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let mut app = app.borrow_mut();
                if app.line_scanning {
                    report(&ui, app.stop_line_scanning(&ui));
                } else {
                    report(&ui, app.start_line_scanning(&ui));
                }
            }
        });
    }

    let timer = Timer::default();
    {
        let ui_weak = ui.as_weak();
        let app = Rc::clone(&app);
        timer.start(TimerMode::Repeated, Duration::from_millis(120), move || {
            if let Some(ui) = ui_weak.upgrade() {
                let mut app = app.borrow_mut();
                if app.line_scanning {
                    if let Err(error) = app.drain_line_scanning(&ui) {
                        ui.set_status(format!("error: {}", error.message).into());
                        ui.set_status_error(true);
                    }
                }
            }
        });
    }

    ui.run()
        .map_err(|error| Error::new(ErrorCode::Driver, error.to_string()))
}

fn driver_choice() -> String {
    std::env::args()
        .skip(2)
        .find(|argument| !argument.starts_with('-'))
        .unwrap_or_else(|| "sim-lsm".into())
}

fn smoke(source: &str) -> Result<()> {
    let (runtime, hub) = crate::lsm_common::runtime_for_source(source)?;
    let frame_events =
        runtime.subscribe(EventFilter::device(&hub).with_kind(EventKind::FrameReady));
    let signal_events =
        runtime.subscribe(EventFilter::device(&hub).with_kind(EventKind::ScanSignalChunk));
    let operation_events =
        runtime.subscribe(EventFilter::device(&hub).with_kind(EventKind::OperationChanged));
    let scene_controls = apply_shared_scene_controls(&runtime, 180.0, -120.0, 4_252.0, 65.0, true)?;
    let objective_control = apply_objective_control(&runtime, 3)?;
    let detector_controls = apply_detector_controls(&runtime, &hub, 110.0, 90.0)?;

    let snapshot = crate::lsm_common::run_request(
        &runtime,
        &hub,
        crate::lsm_common::snapshot_request(128, 128),
    )?;
    let snapshot_frame = drain_smoke_frames(&runtime, &frame_events)?;

    let live_op = runtime.submit_request(&hub, crate::lsm_common::live_image_request(128, 128))?;
    let live = runtime.wait_completed(live_op.id, Duration::from_secs(5))?;
    let live_progress = crate::lsm_common::drain_operation_progress(&operation_events, live_op.id);
    let live_frames = drain_smoke_frames(&runtime, &frame_events)?;

    let line_op = runtime.submit_request(&hub, crate::lsm_common::line_signal_request(256, 64))?;
    let line = runtime.wait_completed(line_op.id, Duration::from_secs(5))?;
    let line_progress = crate::lsm_common::drain_operation_progress(&operation_events, line_op.id);
    let chunks = drain_smoke_chunks(&signal_events);

    println!("source: {source}");
    println!("hub: {}", hub.label);
    println!("source_summary: {}", source_summary(&hub));
    if let Some(scene) = scene_controls {
        println!("{scene}");
    }
    if let Some(objective) = objective_control {
        println!("{objective}");
    }
    if let Some((gain, noise)) = detector_controls {
        println!("detector_controls: gain={gain:.3}, noise={noise:.3}");
    }
    println!("snapshot: {}", result_text_with_plan(&snapshot));
    println!("snapshot_frames: {}", snapshot_frame);
    println!("live: {}", result_text_with_plan(&live));
    println!("live_progress: {}", smoke_progress_text(live_progress));
    println!("live_frames: {}", live_frames);
    println!("line: {}", result_text_with_plan(&line));
    println!("line_progress: {}", smoke_progress_text(line_progress));
    println!("line_chunks: {chunks}");
    Ok(())
}

struct LsmGui {
    runtime: LocalRuntime,
    hub: DeviceDescriptor,
    frame_events: Subscription,
    signal_events: Subscription,
    operation_events: Subscription,
    line_scanning: bool,
    line_operation: Option<OperationId>,
    line_buffer: LineFramebuffer,
}

/// Framebuffer filled row by row from continuous line-scan chunks.
///
/// The scan sweeps down the raster, so line `n` is row `n % height` of the same
/// image the capture and stream capabilities render. Chunks land as partial
/// rows, and the display is rebuilt from whatever has arrived rather than
/// waiting for a complete frame.
#[derive(Default)]
struct LineFramebuffer {
    width: u32,
    height: u32,
    pixels: Vec<u16>,
    lines_written: u64,
    current_line: Option<u64>,
    dirty: bool,
}

impl LineFramebuffer {
    fn reset(&mut self, width: u32, height: u32) {
        self.width = width.clamp(1, 4096);
        self.height = height.clamp(1, 4096);
        self.pixels = vec![0; self.width as usize * self.height as usize];
        self.lines_written = 0;
        self.current_line = None;
        self.dirty = true;
    }

    /// Write one chunk of a scan line. `first_sample` is the offset within the
    /// line, so a chunk only overwrites the columns it actually covers.
    fn write_chunk(&mut self, line: u64, first_sample: u64, samples: &[u16]) {
        if self.pixels.is_empty() || samples.is_empty() {
            return;
        }
        if self.current_line != Some(line) {
            self.current_line = Some(line);
            self.lines_written = self.lines_written.saturating_add(1);
        }
        let row = (line % u64::from(self.height)) as usize;
        let start = usize::try_from(first_sample).unwrap_or(usize::MAX);
        if start >= self.width as usize {
            return;
        }
        let base = row * self.width as usize + start;
        let span = samples.len().min(self.width as usize - start);
        self.pixels[base..base + span].copy_from_slice(&samples[..span]);
        self.dirty = true;
    }

    /// Displayed intensity of every pixel in the framebuffer.
    fn intensities(&self) -> impl Iterator<Item = u8> + '_ {
        self.pixels.iter().map(|code| (code >> 8) as u8)
    }

    /// Rows sit where the scan put them, so this is the image itself, partly
    /// refreshed wherever the sweep has reached.
    ///
    /// `mark_current` draws the row being scanned in white. It is an overlay
    /// applied while rendering — the stored sample codes are never modified, so
    /// the marker never contaminates the acquired image.
    fn image(&self, mark_current: bool) -> Image {
        let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(self.width, self.height);
        let bytes = pixels.make_mut_bytes();
        for (pixel, code) in bytes.chunks_exact_mut(3).zip(&self.pixels) {
            let value = (code >> 8) as u8;
            pixel[0] = value / 3;
            pixel[1] = value;
            pixel[2] = value.saturating_add(30);
        }
        if mark_current {
            if let Some(line) = self.current_line {
                let row = (line % u64::from(self.height)) as usize;
                let start = row * self.width as usize * 3;
                let end = start + self.width as usize * 3;
                bytes[start..end].fill(u8::MAX);
            }
        }
        Image::from_rgb8(pixels)
    }
}

const SOURCE_CHOICES: [(&str, &str); 3] = [
    ("sim-lsm", "Sim LSM"),
    ("sim-composed", "Composed simulator"),
    ("imswitch", "ImSwitch DAQmx"),
];

fn source_options() -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        SOURCE_CHOICES
            .iter()
            .map(|(_, label)| SharedString::from(*label))
            .collect::<Vec<_>>(),
    ))
}

fn source_id(index: i32) -> &'static str {
    usize::try_from(index)
        .ok()
        .and_then(|index| SOURCE_CHOICES.get(index))
        .map(|(source, _)| *source)
        .unwrap_or(SOURCE_CHOICES[0].0)
}

fn source_index(source: &str) -> i32 {
    match source {
        "sim-lsm" | "sim_lsm" | "sim" => 0,
        "sim-composed" | "sim_microscope_lsm" | "sim-microscope-lsm" => 1,
        "imswitch" | "imswitch-daqmx" | "daqmx" => 2,
        _ => 0,
    }
}

impl LsmGui {
    fn set_source(&mut self, ui: &LsmWindow, index: i32) -> Result<()> {
        self.cancel_line_scanning();
        let source = source_id(index);
        let (runtime, hub) = crate::lsm_common::runtime_for_source(source)?;
        let frame_events =
            runtime.subscribe(EventFilter::device(&hub).with_kind(EventKind::FrameReady));
        let signal_events =
            runtime.subscribe(EventFilter::device(&hub).with_kind(EventKind::ScanSignalChunk));
        let operation_events =
            runtime.subscribe(EventFilter::device(&hub).with_kind(EventKind::OperationChanged));
        self.runtime = runtime;
        self.hub = hub;
        self.frame_events = frame_events;
        self.signal_events = signal_events;
        self.operation_events = operation_events;
        self.clear_line_scanning(ui);
        ui.set_preview(blank_image(512, 512));
        ui.set_line_profile(blank_image(512, 96));
        ui.set_source_summary(source_summary(&self.hub).into());
        let (backend_live, backend_note) = backend_live_state(&self.hub);
        ui.set_backend_live(backend_live);
        ui.set_backend_note(backend_note.into());
        ui.set_request_summary("no request submitted yet".into());
        ui.set_frame_summary("no frame".into());
        ui.set_line_summary("counter0 + ai0, chunk 256".into());
        ui.set_progress_summary("idle".into());
        ui.set_status(format!("ready: {source}").into());
        ui.set_status_error(false);
        Ok(())
    }

    fn snapshot(&mut self, ui: &LsmWindow) -> Result<()> {
        let width = scan_width(ui);
        let height = scan_height(ui);
        self.sync_shared_scene(ui)?;
        self.sync_detector_controls(ui)?;
        let mut request = crate::lsm_common::snapshot_request(width as i64, height as i64);
        apply_scan_controls(ui, &mut request.scan);
        request.scan.insert(
            "laser_gate_enabled".into(),
            Value::Bool(ui.get_laser_gate()),
        );
        request
            .scan
            .insert("detectors".into(), Value::List(detector_values(ui)));
        let value = crate::lsm_common::run_request(&self.runtime, &self.hub, request)?;
        let has_frame = self.drain_frame(ui)?;
        ui.set_request_summary(
            format!(
                "snapshot {}x{} {} -> {}",
                width,
                height,
                timing_summary(ui),
                result_text_with_plan(&value)
            )
            .into(),
        );
        ui.set_status(
            if has_frame {
                "snapshot frame received"
            } else {
                "snapshot completed without frame"
            }
            .into(),
        );
        ui.set_status_error(false);
        Ok(())
    }

    fn start_line_scanning(&mut self, ui: &LsmWindow) -> Result<()> {
        let width = scan_width(ui);
        let height = scan_height(ui);
        let chunk = ui.get_chunk_value().round().clamp(16.0, 4096.0) as u64;
        self.sync_shared_scene(ui)?;
        self.sync_detector_controls(ui)?;
        let mut request = crate::lsm_common::continuous_raster_line_signal_request(
            width as i64,
            height as i64,
            chunk,
            detector_names(ui),
        );
        apply_scan_controls(ui, &mut request.timing);
        request.timing.insert(
            "laser_gate_enabled".into(),
            Value::Bool(ui.get_laser_gate()),
        );
        let operation = self.runtime.submit_request(&self.hub, request)?;
        self.line_buffer.reset(width, height);
        ui.set_preview(self.line_buffer.image(ui.get_mark_scan_line()));
        ui.set_histogram_path(histogram_path(&histogram(self.line_buffer.intensities())).into());
        self.line_scanning = true;
        self.line_operation = Some(operation.id);
        ui.set_line_scanning(true);
        ui.set_request_summary(
            format!(
                "line scanning {} samples, chunk {}, {} running",
                width,
                chunk,
                timing_summary(ui)
            )
            .into(),
        );
        ui.set_progress_summary("line scanning running".into());
        ui.set_status("line scanning running".into());
        ui.set_status_error(false);
        Ok(())
    }

    /// Drain whatever chunks have arrived since the last tick into the
    /// framebuffer, then refresh the preview if anything landed.
    fn drain_line_scanning(&mut self, ui: &LsmWindow) -> Result<()> {
        let mut latest_samples = Vec::new();
        let mut first_summary = None;
        while let Some(event) = self.signal_events.try_recv() {
            if let Event::ScanSignalChunk(event) = event {
                if first_summary.is_none() {
                    first_summary = Some(signal_chunk_summary(&event));
                }
                // The raster capability averages the selected detectors, so the
                // framebuffer averages the same channels to match it.
                let traces: Vec<Vec<u16>> = event
                    .samples
                    .values()
                    .map(|values| values.iter().filter_map(sample_u16).collect())
                    .filter(|trace: &Vec<u16>| !trace.is_empty())
                    .collect();
                if traces.is_empty() {
                    continue;
                }
                let samples: Vec<u16> = (0..traces.iter().map(Vec::len).min().unwrap_or(0))
                    .map(|index| {
                        let total: u32 = traces.iter().map(|trace| u32::from(trace[index])).sum();
                        (total / traces.len() as u32) as u16
                    })
                    .collect();
                self.line_buffer
                    .write_chunk(event.line, event.first_sample, &samples);
                latest_samples = samples;
            }
        }

        if self.line_buffer.dirty {
            self.line_buffer.dirty = false;
            ui.set_preview(self.line_buffer.image(ui.get_mark_scan_line()));
            // Whole framebuffer, so the histogram describes the image being
            // built rather than the row that just landed.
            ui.set_histogram_path(histogram_path(&histogram(self.line_buffer.intensities())).into());
            ui.set_frame_summary(
                format!(
                    "line-scan framebuffer {}x{}, {} rows written",
                    self.line_buffer.width, self.line_buffer.height, self.line_buffer.lines_written
                )
                .into(),
            );
        }
        if !latest_samples.is_empty() {
            ui.set_line_profile(line_image_from_samples(
                &latest_samples,
                self.line_buffer.width,
            ));
        }
        if let Some(first) = first_summary {
            ui.set_line_summary(format!("latest chunk=[{first}]").into());
        }

        if let Some(operation) = self.line_operation {
            if let Some(progress) = self.drain_progress(operation) {
                ui.set_progress_summary(progress_text("line scanning", progress).into());
            }
            match self.runtime.status(operation) {
                OperationStatus::Running { .. } | OperationStatus::Queued => {}
                OperationStatus::Completed(value) => {
                    self.clear_line_scanning(ui);
                    ui.set_request_summary(
                        format!("line scanning stopped -> {}", result_text_with_plan(&value))
                            .into(),
                    );
                    ui.set_status("line scanning completed".into());
                    ui.set_status_error(false);
                }
                OperationStatus::Failed(report) => {
                    self.clear_line_scanning(ui);
                    return Err(Error::new(report.code, report.message));
                }
                OperationStatus::Cancelled => {
                    self.clear_line_scanning(ui);
                    ui.set_status("line scanning stopped".into());
                    ui.set_status_error(false);
                }
                OperationStatus::TimedOut | OperationStatus::Unknown => {}
            }
        }
        Ok(())
    }

    fn stop_line_scanning(&mut self, ui: &LsmWindow) -> Result<()> {
        self.cancel_line_scanning();
        self.clear_line_scanning(ui);
        ui.set_progress_summary("line scanning stopped".into());
        ui.set_status("line scanning stopped".into());
        ui.set_status_error(false);
        Ok(())
    }

    fn clear_line_scanning(&mut self, ui: &LsmWindow) {
        self.line_scanning = false;
        self.line_operation = None;
        ui.set_line_scanning(false);
    }

    fn cancel_line_scanning(&mut self) {
        if let Some(operation) = self.line_operation.take() {
            let _ = self.runtime.cancel(operation);
        }
        self.line_scanning = false;
    }

    fn sync_shared_scene(&self, ui: &LsmWindow) -> Result<()> {
        apply_shared_scene_controls(
            &self.runtime,
            f64::from(ui.get_stage_x_value()).round(),
            f64::from(ui.get_stage_y_value()).round(),
            f64::from(ui.get_focus_z_value()).round(),
            f64::from(ui.get_lamp_power_value()).round(),
            ui.get_laser_gate(),
        )?;
        apply_objective_control(
            &self.runtime,
            f64::from(ui.get_objective_position_value())
                .round()
                .clamp(1.0, 3.0) as i64,
        )?;
        Ok(())
    }

    fn sync_detector_controls(&self, ui: &LsmWindow) -> Result<()> {
        let gain_percent = f64::from(ui.get_detector_gain_value())
            .round()
            .clamp(0.0, 500.0);
        let noise_percent = f64::from(ui.get_detector_noise_value())
            .round()
            .clamp(0.0, 500.0);
        apply_detector_controls(&self.runtime, &self.hub, gain_percent, noise_percent)?;
        Ok(())
    }

    fn drain_frame(&mut self, ui: &LsmWindow) -> Result<bool> {
        let mut latest = None;
        while let Some(event) = self.frame_events.try_recv() {
            if let Event::FrameReady(event) = event {
                latest = Some(event.handle);
            }
        }
        if let Some(handle) = latest {
            if let Some(frame) = self.runtime.frame(handle)? {
                ui.set_preview(frame_image(&frame));
                ui.set_histogram_path(
                    histogram_path(&histogram(frame_intensities(&frame).into_iter())).into(),
                );
                ui.set_frame_summary(frame_summary(&frame).into());
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn drain_progress(&mut self, operation: OperationId) -> Option<ProgressSummary> {
        let mut summary = None;
        while let Some(event) = self.operation_events.try_recv() {
            if let Event::OperationChanged(event) = event {
                if event.operation != operation {
                    continue;
                }
                if let OperationStatus::Running {
                    progress: Some(progress),
                } = event.status
                {
                    let updates = summary
                        .map(|summary: ProgressSummary| summary.updates + 1)
                        .unwrap_or(1);
                    summary = Some(ProgressSummary {
                        updates,
                        completed: progress.completed,
                        total: progress.total,
                    });
                }
            }
        }
        summary
    }
}

fn apply_shared_scene_controls(
    runtime: &LocalRuntime,
    x_um: f64,
    y_um: f64,
    z_um: f64,
    lamp_power_percent: f64,
    lamp_enabled: bool,
) -> Result<Option<String>> {
    let xy = device_with_writable_properties(runtime, &["stage.xy"], &["x", "y"]);
    let z = device_with_writable_properties(runtime, &["stage.z"], &["z"]);
    let lamp = device_with_writable_properties(runtime, &["light.source"], &["enabled", "power"]);
    let (Some(xy), Some(z), Some(lamp)) = (xy, z, lamp) else {
        return Ok(None);
    };
    let x_um = x_um.clamp(-1000.0, 1000.0);
    let y_um = y_um.clamp(-1000.0, 1000.0);
    let z_um = z_um.clamp(0.0, 8500.0);
    let lamp_power_percent = lamp_power_percent.clamp(0.0, 100.0);
    let state = StateSet::immediate("lsm gui shared simulator scene").with_writes([
        StateWrite::new(
            xy.id,
            "x",
            Value::Position(Position::from_micrometers(x_um)),
        ),
        StateWrite::new(
            xy.id,
            "y",
            Value::Position(Position::from_micrometers(y_um)),
        ),
        StateWrite::new(z.id, "z", Value::Position(Position::from_micrometers(z_um))),
        StateWrite::new(lamp.id, "enabled", Value::Bool(lamp_enabled)),
        StateWrite::new(
            lamp.id,
            "power",
            Value::Ratio(Ratio::from_percent(lamp_power_percent)),
        ),
    ]);
    runtime.execute(state.into_command(), Duration::from_secs(1))?;
    Ok(Some(format!(
        "scene_controls: stage_um=({x_um:.3},{y_um:.3},{z_um:.3}), lamp_power={:.3}, lamp_enabled={lamp_enabled}",
        lamp_power_percent / 100.0
    )))
}

fn apply_objective_control(runtime: &LocalRuntime, position: i64) -> Result<Option<String>> {
    let Some(turret) =
        device_with_writable_properties(runtime, &["objective.turret"], &["position"])
    else {
        return Ok(None);
    };
    let position = position.clamp(1, 3);
    let selects_by_capability = runtime
        .capabilities(turret.id)?
        .iter()
        .any(|capability| capability.kind == CapabilityKind::FilterSelect);
    if selects_by_capability {
        runtime.execute_request(
            turret.id,
            FilterSelectRequest::position(position as u8),
            Duration::from_secs(10),
        )?;
    } else {
        runtime.execute(
            Command::write_property(turret.id, "position", Value::I64(position)),
            Duration::from_secs(10),
        )?;
    }
    let magnification = runtime.execute(
        Command::read_property(turret.id, "magnification"),
        Duration::from_secs(1),
    )?;
    let numerical_aperture = runtime.execute(
        Command::read_property(turret.id, "numerical_aperture"),
        Duration::from_secs(1),
    )?;
    Ok(Some(format!(
        "objective_control: position={position}, magnification={:.1}, numerical_aperture={:.2}",
        f64_value(&magnification).unwrap_or_default(),
        numerical_aperture_value(&numerical_aperture).unwrap_or_default()
    )))
}

fn apply_detector_controls(
    runtime: &LocalRuntime,
    hub: &DeviceDescriptor,
    gain_percent: f64,
    noise_percent: f64,
) -> Result<Option<(f64, f64)>> {
    if !supports_writable_property(hub, "detector_gain")
        || !supports_writable_property(hub, "detector_noise")
    {
        return Ok(None);
    }
    let gain = runtime.execute(
        Command::write_property(
            hub.id,
            "detector_gain",
            Value::Ratio(Ratio::from_percent(gain_percent)),
        ),
        Duration::from_secs(1),
    )?;
    let noise = runtime.execute(
        Command::write_property(
            hub.id,
            "detector_noise",
            Value::Ratio(Ratio::from_percent(noise_percent)),
        ),
        Duration::from_secs(1),
    )?;
    Ok(Some((
        ratio_value(&gain).unwrap_or(gain_percent / 100.0),
        ratio_value(&noise).unwrap_or(noise_percent / 100.0),
    )))
}

fn supports_writable_property(hub: &DeviceDescriptor, key: &str) -> bool {
    hub.properties
        .iter()
        .any(|property| property.key == key && property.writable)
}

fn device_with_writable_properties<'a>(
    runtime: &'a LocalRuntime,
    kinds: &[&str],
    keys: &[&str],
) -> Option<&'a DeviceDescriptor> {
    runtime.devices().into_iter().find(|device| {
        device.has_kinds(kinds)
            && keys
                .iter()
                .all(|key| supports_writable_property(device, key))
    })
}

fn ratio_value(value: &Value) -> Option<f64> {
    match value {
        Value::Ratio(value) => Some(value.fraction()),
        _ => None,
    }
}

fn f64_value(value: &Value) -> Option<f64> {
    match value {
        Value::F64(value) => Some(*value),
        _ => None,
    }
}

fn numerical_aperture_value(value: &Value) -> Option<f64> {
    match value {
        Value::NumericalAperture(value) => Some(value.value()),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
struct ProgressSummary {
    updates: u64,
    completed: f64,
    total: f64,
}

fn detector_names(ui: &LsmWindow) -> Vec<String> {
    let mut channels = Vec::new();
    if ui.get_use_counter() {
        channels.push("counter0".into());
    }
    if ui.get_use_analog() {
        channels.push("ai0".into());
    }
    if channels.is_empty() {
        channels.push("counter0".into());
    }
    channels
}

fn detector_values(ui: &LsmWindow) -> Vec<Value> {
    detector_names(ui).into_iter().map(Value::String).collect()
}

fn report(ui: &LsmWindow, result: Result<()>) {
    if let Err(error) = result {
        ui.set_status(format!("error: {}", error.message).into());
        ui.set_status_error(true);
    }
}

fn scan_width(ui: &LsmWindow) -> u32 {
    ui.get_width_value().round().clamp(64.0, 2048.0) as u32
}

fn scan_height(ui: &LsmWindow) -> u32 {
    ui.get_height_value().round().clamp(1.0, 2048.0) as u32
}

fn apply_scan_controls(ui: &LsmWindow, map: &mut std::collections::BTreeMap<String, Value>) {
    map.insert(
        "sample_rate".into(),
        Value::Frequency(Frequency::from_kilohertz(f64::from(
            ui.get_rate_value().round().clamp(1.0, 1000.0),
        ))),
    );
    map.insert(
        "line_dwell".into(),
        Value::TimeInterval(TimeInterval::from_microseconds(f64::from(
            ui.get_dwell_value().round().clamp(10.0, 5000.0),
        ))),
    );
}

fn timing_summary(ui: &LsmWindow) -> String {
    format!(
        "{:.0} kHz, {:.0} us",
        ui.get_rate_value().round().clamp(1.0, 1000.0),
        ui.get_dwell_value().round().clamp(10.0, 5000.0)
    )
}

const HISTOGRAM_BINS: usize = 256;
const HISTOGRAM_SMOOTHING: usize = 2;

/// Intensity histogram over every pixel of the image, binned on the same 8-bit
/// value the preview draws. During a line scan this covers the whole
/// framebuffer, not only the row being scanned.
fn histogram(values: impl Iterator<Item = u8>) -> Vec<f32> {
    let mut counts = [0u32; HISTOGRAM_BINS];
    for value in values {
        counts[value as usize] += 1;
    }
    let smoothed = (0..HISTOGRAM_BINS)
        .map(|bin| {
            let low = bin.saturating_sub(HISTOGRAM_SMOOTHING);
            let high = (bin + HISTOGRAM_SMOOTHING).min(HISTOGRAM_BINS - 1);
            let window = &counts[low..=high];
            let mean = window.iter().sum::<u32>() as f32 / window.len() as f32;
            mean.sqrt()
        })
        .collect::<Vec<_>>();
    let max = smoothed.iter().copied().fold(0.0f32, f32::max).max(1.0);
    smoothed.into_iter().map(|value| value / max).collect()
}

/// Emits the histogram as one continuous filled outline instead of separate bars.
fn histogram_path(bins: &[f32]) -> String {
    let mut path = String::from("M 0 100");
    for (bin, value) in bins.iter().enumerate() {
        path.push_str(&format!(" L {bin} {:.2}", 100.0 - value * 100.0));
    }
    path.push_str(&format!(" L {} 100 Z", bins.len().saturating_sub(1)));
    path
}

fn frame_image(frame: &Frame) -> Image {
    let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(frame.width, frame.height);
    let intensities = frame_intensities(frame);
    for (pixel, value) in pixels.make_mut_bytes().chunks_exact_mut(3).zip(intensities) {
        pixel[0] = value / 3;
        pixel[1] = value;
        pixel[2] = value.saturating_add(30);
    }
    Image::from_rgb8(pixels)
}

fn frame_intensities(frame: &Frame) -> Vec<u8> {
    if frame.pixel_format == "Mono16" {
        frame
            .data
            .chunks_exact(2)
            .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]) >> 8)
            .map(|value| value as u8)
            .collect()
    } else {
        frame.data.clone()
    }
}

fn frame_summary(frame: &Frame) -> String {
    let mut summary = format!(
        "{}x{} {} bytes {}",
        frame.width,
        frame.height,
        frame.data.len(),
        frame.pixel_format
    );
    if let Some(dirty) = dirty_region_metadata(&frame.metadata) {
        let update_policy =
            string_metadata(&frame.metadata, "update_policy").unwrap_or_else(|| "unknown".into());
        summary.push_str(&format!(
            ", dirty {}x{} at {},{} ({})",
            dirty.width, dirty.height, dirty.x, dirty.y, update_policy
        ));
    }
    if let Some(scan) = crate::lsm_common::frame_scan_metadata_summary(frame) {
        summary.push_str(&format!("; scan=[{scan}]"));
    }
    if let Some(scene) = crate::lsm_common::scene_metadata_summary(&frame.metadata) {
        summary.push_str(&format!("; scene=[{scene}]"));
    }
    summary
}

#[derive(Debug, Clone, Copy)]
struct DirtyRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
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

fn string_metadata(
    metadata: &std::collections::BTreeMap<String, Value>,
    key: &str,
) -> Option<String> {
    match metadata.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn line_image_from_samples(samples: &[u16], width: u32) -> Image {
    let image_width = width.clamp(64, 2048);
    let image_height = 96;
    let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(image_width, image_height);
    {
        let bytes = pixels.make_mut_bytes();
        for pixel in bytes.chunks_exact_mut(3) {
            pixel[0] = 18;
            pixel[1] = 24;
            pixel[2] = 31;
        }
        for x in 0..image_width {
            let sample_index = ((x as usize) * samples.len() / image_width as usize)
                .min(samples.len().saturating_sub(1));
            let sample = samples.get(sample_index).copied().unwrap_or(0);
            let signal = sample as f32 / u16::MAX as f32;
            let y = ((1.0 - signal.clamp(0.0, 1.0)) * (image_height - 1) as f32) as u32;
            for dy in 0..3 {
                let yy = (y + dy).min(image_height - 1);
                let idx = ((yy * image_width + x) * 3) as usize;
                bytes[idx] = 68;
                bytes[idx + 1] = 210;
                bytes[idx + 2] = 156;
            }
        }
    }
    Image::from_rgb8(pixels)
}

fn blank_image(width: u32, height: u32) -> Image {
    let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
    for pixel in pixels.make_mut_bytes().chunks_exact_mut(3) {
        pixel[0] = 18;
        pixel[1] = 24;
        pixel[2] = 31;
    }
    Image::from_rgb8(pixels)
}

fn sample_u16(value: &Value) -> Option<u16> {
    match value {
        Value::I64(value) => Some((*value).clamp(0, u16::MAX as i64) as u16),
        Value::F64(value) => Some(value.round().clamp(0.0, u16::MAX as f64) as u16),
        Value::Voltage(value) => {
            Some(((value.volts() / 5.0) * u16::MAX as f64).clamp(0.0, u16::MAX as f64) as u16)
        }
        _ => None,
    }
}

fn result_text(value: &Value) -> String {
    crate::lsm_common::api_result(value)
}

fn result_text_with_plan(value: &Value) -> String {
    let mut text = result_text(value);
    if let Some(plan) = crate::lsm_common::daqmx_task_plan_summary(value) {
        text.push_str(" | ");
        text.push_str(&plan);
    }
    text
}

/// Whether the selected source can actually execute a scan, and if not, the
/// reason the backend gave. Sources that publish no `backend_status` metadata
/// (the simulator) scan normally.
fn backend_live_state(hub: &DeviceDescriptor) -> (bool, String) {
    let Some(Value::Map(status)) = hub.metadata.get("backend_status") else {
        return (true, String::new());
    };
    if map_bool(status, "live_task_execution_ready").unwrap_or(false) {
        return (true, String::new());
    }
    let mut note = match map_string(status, "live_task_execution_blocker") {
        Some(blocker) => format!("Blocked by: {blocker}."),
        None => "The backend reports it is not ready to execute a live scan.".to_owned(),
    };
    if let Some(execution) = map_string(status, "execution_status") {
        note.push_str(&format!(" Execution status: {execution}."));
    }
    note.push_str(
        " The scan plan, timing and channel mapping shown on the right are still computed and validated; only frame execution is blocked.",
    );
    (false, note)
}

fn source_summary(hub: &DeviceDescriptor) -> String {
    let mut parts = Vec::new();
    if let Some(Value::Map(status)) = hub.metadata.get("backend_status") {
        if let Some(execution) = map_string(status, "execution_status") {
            parts.push(format!("backend={execution}"));
        }
        if let Some(ready) = map_bool(status, "live_task_execution_ready") {
            parts.push(format!("live_ready={ready}"));
        }
        if let Some(requested) = map_bool(status, "live_task_execution_requested") {
            parts.push(format!("live_requested={requested}"));
        }
        if let Some(blocker) = map_string(status, "live_task_execution_blocker") {
            parts.push(format!("blocker={blocker}"));
        }
        if let Some(summary) = promotion_gate_statuses_summary(status) {
            parts.push(format!("promotion_gate_statuses=[{summary}]"));
        }
    }
    if let Some(roles) = physical_roles_summary(hub.metadata.get("lsm_role_channels")) {
        parts.push(format!("roles=[{roles}]"));
    }
    if parts.is_empty() {
        format!("source kinds: {}", hub.kinds.join(", "))
    } else {
        parts.join("; ")
    }
}

fn physical_roles_summary(value: Option<&Value>) -> Option<String> {
    let Some(Value::Map(roles)) = value else {
        return None;
    };
    let mut parts = Vec::new();
    for role in [
        "x_galvo",
        "y_galvo",
        "laser_gate",
        "detector",
        "sample_clock",
    ] {
        let Some(Value::Map(channel)) = roles.get(role) else {
            continue;
        };
        let physical = map_string(channel, "physical")?;
        parts.push(format!("{role}={physical}"));
    }
    (!parts.is_empty()).then(|| parts.join(","))
}

fn promotion_gate_statuses_summary(status: &BTreeMap<String, Value>) -> Option<String> {
    let Some(Value::Map(statuses)) = status.get("external_promotion_gate_statuses") else {
        return None;
    };
    let mut counts = BTreeMap::<String, usize>::new();
    for status in statuses.values() {
        let Value::Map(status) = status else {
            continue;
        };
        if let Some(status) = map_string(status, "status") {
            *counts.entry(status).or_default() += 1;
        }
    }
    (!counts.is_empty()).then(|| {
        counts
            .into_iter()
            .map(|(status, count)| format!("{status}={count}"))
            .collect::<Vec<_>>()
            .join(",")
    })
}

fn map_string(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    match map.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn map_bool(map: &BTreeMap<String, Value>, key: &str) -> Option<bool> {
    match map.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn progress_text(label: &str, progress: ProgressSummary) -> String {
    if progress.total > 0.0 {
        format!(
            "{} {:.0}/{:.0} ({} updates)",
            label, progress.completed, progress.total, progress.updates
        )
    } else {
        format!("{} {:.0} updates (open stream)", label, progress.completed)
    }
}

fn smoke_progress_text(progress: Option<crate::lsm_common::ProgressSummary>) -> String {
    match progress {
        Some(progress) if progress.total > 0.0 => format!(
            "updates={} last={:.0}/{:.0}",
            progress.updates, progress.completed, progress.total
        ),
        Some(progress) => format!(
            "updates={} completed={:.0}",
            progress.updates, progress.completed
        ),
        None => "none".into(),
    }
}

fn drain_smoke_frames(runtime: &LocalRuntime, events: &Subscription) -> Result<String> {
    let mut frames = 0u64;
    let mut latest = None;
    while let Some(event) = events.recv_timeout(Duration::from_millis(100)) {
        if let Event::FrameReady(event) = event {
            frames += 1;
            latest = runtime.frame(event.handle)?;
        }
    }
    Ok(match latest {
        Some(frame) => {
            let mut summary = format!(
                "observed={} latest={}x{} {}",
                frames, frame.width, frame.height, frame.pixel_format
            );
            if let Some(metadata) = crate::lsm_common::frame_scan_metadata_summary(&frame) {
                summary.push_str(&format!(" metadata=[{metadata}]"));
            }
            if let Some(scene) = crate::lsm_common::scene_metadata_summary(&frame.metadata) {
                summary.push_str(&format!(" scene=[{scene}]"));
            }
            summary
        }
        None => "observed=0".into(),
    })
}

fn drain_smoke_chunks(events: &Subscription) -> String {
    let mut chunks = 0u64;
    let mut samples = 0u64;
    let mut channels = 0usize;
    let mut first = None;
    while let Some(event) = events.recv_timeout(Duration::from_millis(100)) {
        if let Event::ScanSignalChunk(event) = event {
            if first.is_none() {
                first = Some(smoke_chunk_summary(&event));
            }
            chunks += 1;
            samples += event.sample_count;
            channels = event.channels.len();
        }
    }
    let mut summary = format!("observed={chunks} samples={samples} channels={channels}");
    if let Some(first) = first {
        summary.push_str(&format!(" first=[{first}]"));
    }
    summary
}

fn smoke_chunk_summary(event: &numanager_core::ScanSignalChunkEvent) -> String {
    signal_chunk_summary(event)
}

fn signal_chunk_summary(event: &numanager_core::ScanSignalChunkEvent) -> String {
    let channels = if event.channels.is_empty() {
        "none".into()
    } else {
        event.channels.join("+")
    };
    let dropped_chunks = i64_metadata(&event.metadata, "dropped_chunks").unwrap_or(0);
    let dropped_samples = i64_metadata(&event.metadata, "dropped_samples").unwrap_or(0);
    let overflowed = bool_metadata(&event.metadata, "overflowed").unwrap_or(false);
    let detector_gain = ratio_metadata(&event.metadata, "detector_gain").unwrap_or(1.0);
    let detector_noise = ratio_metadata(&event.metadata, "detector_noise").unwrap_or(1.0);
    let mut summary = format!(
        "channels={channels}, line={}, chunk={}, first_sample={}, sample_rate_hz={:.0}, sample_period_s={:.9}, detector_gain={detector_gain:.3}, detector_noise={detector_noise:.3}, dropped_chunks={dropped_chunks}, dropped_samples={dropped_samples}, overflowed={overflowed}",
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

fn i64_metadata(metadata: &BTreeMap<String, Value>, key: &str) -> Option<i64> {
    match metadata.get(key) {
        Some(Value::I64(value)) => Some(*value),
        Some(Value::PixelCount(value)) => Some(i64::from(value.0)),
        _ => None,
    }
}

fn bool_metadata(metadata: &BTreeMap<String, Value>, key: &str) -> Option<bool> {
    match metadata.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn ratio_metadata(metadata: &BTreeMap<String, Value>, key: &str) -> Option<f64> {
    match metadata.get(key) {
        Some(Value::Ratio(value)) => Some(value.fraction()),
        _ => None,
    }
}
