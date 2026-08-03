use numanager_core::runtime::{LocalRuntime, Runtime};
use numanager_core::*;
use numanager_drivers::sim_microscope::SimMicroscopeDriver;

use numanager_examples::{completion_summary, is_public_property, public_kind_summary};
use slint::{
    Image, Model, ModelRc, Rgb8Pixel, SharedPixelBuffer, SharedString, Timer, TimerMode, VecModel,
};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Stage step used only when no device publishes an optical scale and no stage
/// publishes a step size. The readout says so rather than presenting it as a
/// calibrated number.
const DEFAULT_UM_PER_DRAG_PX: f64 = 0.5;

/// Dependency role naming the objective in a camera's light path. `Role` has no
/// objective variant, so this is a convention rather than a typed edge; a
/// device that does not use it is still found by its `objective.turret` kind.
const OBJECTIVE_ROLE: &str = "objective";

/// Lengths a scale bar is allowed to take, in micrometres.
const SCALE_BAR_STEPS_UM: [f64; 10] =
    [1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0];

/// Mono8 code at or above which a pixel counts as clipped and is drawn red.
const SATURATION_CODE: u8 = 255;

const HISTOGRAM_BINS: usize = 256;
const HISTOGRAM_SMOOTHING: usize = 2;
/// Heights of a property row and of a per-device table header, mirrored in the
/// `.slint` markup so the scroll viewport can be sized from the row count.
const ROW_HEIGHT_PX: f32 = 36.0;
const HEADER_HEIGHT_PX: f32 = 34.0;

slint::slint! {
import { Button, CheckBox, ComboBox, LineEdit, ScrollView, Slider, VerticalBox, HorizontalBox } from "std-widgets.slint";

export struct PropertyRow {
    device: string,
    kinds: string,
    key: string,
    display: string,
    value: string,
    // Editor for this property, picked from the property schema rather than
    // from the key name: "readonly", "bool", "choice", "slider" or "text".
    control: string,
    checked: bool,
    options: [string],
    slider-min: float,
    slider-max: float,
    slider-value: float,
    table-start: bool,
}

export struct SafetyRow {
    device: string,
    state: string,
    detail: string,
    alarm: bool,
}

export component MainWindow inherits Window {
    title: "numanager software test GUI";
    preferred-width: 1240px;
    preferred-height: 820px;

    in property <image> camera-frame;
    in property <[string]> camera-sources;
    in-out property <int> selected-camera;
    in property <[string]> pan-stages;
    in-out property <int> selected-pan-stage;
    in property <[PropertyRow]> properties;
    in property <length> properties-height;
    in property <[SafetyRow]> safety;
    in property <string> histogram-path;
    in-out property <bool> mark-saturated;
    in property <string> status;
    in property <string> xy-readout;
    in property <string> pan-hint;
    in property <bool> streaming;
    in property <bool> stream-supported;
    // Scanning instruments only: acquisition is driven one line at a time, and
    // the scan grid is chosen here rather than read from a sensor.
    in property <bool> line-scan-supported;
    in property <bool> line-scanning;
    in-out property <float> scan-width: 512;
    in-out property <float> scan-height: 512;
    // Display-only overlay marking the row the scan is currently on.
    in-out property <bool> mark-scan-line: true;
    in property <bool> has-pan-stage;
    in property <[string]> focus-stages;
    in-out property <int> selected-focus-stage;
    in property <string> focus-readout;
    in property <string> focus-fine-label;
    in property <string> focus-coarse-label;
    in property <bool> has-focus-stage;
    in property <float> scale-bar-pixels;
    in property <string> scale-bar-label;

    callback capture();
    callback start-stream();
    callback stop-stream();
    callback line-scan();
    callback property-edited(string, string, string);
    callback property-slider(string, string, float);
    callback pan(float, float);
    callback focus-step(float);
    callback redraw();

    HorizontalBox {
        padding: 12px;
        spacing: 12px;

        VerticalBox {
            spacing: 8px;
            width: 780px;

            HorizontalBox {
                spacing: 8px;
                alignment: start;
                Text { text: "Imager"; vertical-alignment: center; }
                ComboBox {
                    model: root.camera-sources;
                    current-index <=> root.selected-camera;
                    enabled: !root.streaming && !root.line-scanning;
                    width: 240px;
                }
                Button {
                    text: "Capture";
                    enabled: !root.streaming && !root.line-scanning;
                    clicked => { root.capture(); }
                }
                Button {
                    text: root.streaming ? "Stop streaming" : "Start streaming";
                    primary: root.streaming;
                    enabled: root.stream-supported && !root.line-scanning;
                    width: 150px;
                    clicked => {
                        if root.streaming {
                            root.stop-stream();
                        } else {
                            root.start-stream();
                        }
                    }
                }
                if root.line-scan-supported: Button {
                    text: root.line-scanning ? "Stop" : "Line scanning";
                    primary: root.line-scanning;
                    enabled: !root.streaming;
                    width: 150px;
                    clicked => { root.line-scan(); }
                }
                Rectangle {
                    width: 14px;
                    height: 14px;
                    border-radius: 7px;
                    y: (parent.height - self.height) / 2;
                    background: (root.streaming || root.line-scanning) ? #e0463c : #c3cad4;
                    animate background { duration: 150ms; }
                }
                Text {
                    text: root.streaming ? "LIVE" : root.line-scanning ? "LINE SCAN" : "idle";
                    color: (root.streaming || root.line-scanning) ? #e0463c : #8b95a3;
                    font-weight: 700;
                    vertical-alignment: center;
                }
            }

            frame := Rectangle {
                vertical-stretch: 1;
                background: #12161c;
                border-color: (root.streaming || root.line-scanning) ? #e0463c : #303844;
                border-width: (root.streaming || root.line-scanning) ? 2px : 1px;

                // Last reported drag position: the stage moves by the step since
                // the previous event, not by the whole offset from the press.
                property <length> drag-x;
                property <length> drag-y;
                // The frame is letterboxed by image-fit, so a screen pixel is
                // not an image pixel; the stage step is quoted per image pixel.
                property <float> display-scale: min(
                    self.width / max(1px, root.camera-frame.width * 1px),
                    self.height / max(1px, root.camera-frame.height * 1px));

                Image {
                    source: root.camera-frame;
                    image-fit: contain;
                    image-rendering: ImageRendering.pixelated;
                    width: parent.width;
                    height: parent.height;
                }

                touch := TouchArea {
                    width: parent.width;
                    height: parent.height;
                    mouse-cursor: self.pressed ? MouseCursor.grabbing : MouseCursor.grab;
                    pointer-event(event) => {
                        if event.kind == PointerEventKind.down {
                            frame.drag-x = self.mouse-x;
                            frame.drag-y = self.mouse-y;
                        }
                    }
                    enabled: root.has-pan-stage;
                    moved => {
                        if self.pressed {
                            root.pan(
                                (touch.mouse-x - frame.drag-x) / 1px / max(0.001, frame.display-scale),
                                (touch.mouse-y - frame.drag-y) / 1px / max(0.001, frame.display-scale));
                            frame.drag-x = touch.mouse-x;
                            frame.drag-y = touch.mouse-y;
                        }
                    }
                }

                Rectangle {
                    visible: root.scale-bar-pixels > 0;
                    x: (parent.width - root.camera-frame.width * 1px * frame.display-scale) / 2 + 16px;
                    y: parent.height - (parent.height - root.camera-frame.height * 1px * frame.display-scale) / 2 - 30px;
                    width: max(2px, root.scale-bar-pixels * 1px * frame.display-scale);
                    height: 4px;
                    background: #f2f6ff;
                }
                Text {
                    visible: root.scale-bar-pixels > 0;
                    x: (parent.width - root.camera-frame.width * 1px * frame.display-scale) / 2 + 16px;
                    y: parent.height - (parent.height - root.camera-frame.height * 1px * frame.display-scale) / 2 - 26px;
                    text: root.scale-bar-label;
                    color: #f2f6ff;
                }
            }

            HorizontalBox {
                spacing: 8px;
                alignment: start;
                visible: root.has-pan-stage;
                Text { text: "Pan stage"; vertical-alignment: center; }
                ComboBox {
                    model: root.pan-stages;
                    current-index <=> root.selected-pan-stage;
                    width: 240px;
                }
            }

            HorizontalBox {
                spacing: 8px;
                alignment: start;
                visible: root.has-focus-stage;
                Text { text: "Focus"; vertical-alignment: center; }
                ComboBox {
                    model: root.focus-stages;
                    current-index <=> root.selected-focus-stage;
                    width: 200px;
                }
                Button {
                    text: "\u{2212}" + root.focus-coarse-label;
                    clicked => { root.focus-step(-100); }
                }
                Button {
                    text: "\u{2212}" + root.focus-fine-label;
                    clicked => { root.focus-step(-10); }
                }
                Button {
                    text: "+" + root.focus-fine-label;
                    clicked => { root.focus-step(10); }
                }
                Button {
                    text: "+" + root.focus-coarse-label;
                    clicked => { root.focus-step(100); }
                }
                Text { text: root.focus-readout; vertical-alignment: center; }
            }

            Text { text: root.xy-readout; color: #1d2733; wrap: word-wrap; }
            Text { text: root.pan-hint; color: #6b7787; wrap: word-wrap; }

            if root.line-scan-supported: HorizontalBox {
                spacing: 8px;
                alignment: start;
                Text {
                    text: "Scan " + round(root.scan-width) + " x " + round(root.scan-height);
                    color: #536070;
                    vertical-alignment: center;
                }
                Slider {
                    minimum: 64;
                    maximum: 2048;
                    value <=> root.scan-width;
                    enabled: !root.line-scanning;
                    width: 150px;
                }
                Slider {
                    minimum: 1;
                    maximum: 2048;
                    value <=> root.scan-height;
                    enabled: !root.line-scanning;
                    width: 150px;
                }
                CheckBox {
                    text: "Mark current scan line";
                    checked <=> root.mark-scan-line;
                }
            }

            HorizontalBox {
                spacing: 8px;
                alignment: start;
                Text {
                    text: "Intensity histogram 0 -> 255";
                    color: #536070;
                    vertical-alignment: center;
                }
                CheckBox {
                    text: "Mark saturated (255)";
                    checked <=> root.mark-saturated;
                    toggled => { root.redraw(); }
                }
            }

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

            Text { text: root.status; color: #536070; }
        }

        VerticalBox {
            spacing: 8px;
            width: 440px;

            Text { text: "Emission safety"; font-size: 18px; font-weight: 700; }
            Text {
                text: "Interlock and emission state the runtime derives from the safety properties "
                    + "a device publishes (enabled, interlock_closed, fault). Only devices that "
                    + "publish them appear here; toggling a light source's enabled property below "
                    + "moves it between safe and active.";
                color: #6b7787;
                wrap: word-wrap;
            }
            for row in root.safety: Rectangle {
                height: 50px;
                background: row.alarm ? #fff3f2 : #ffffff;
                border-color: row.alarm ? #e0463c : #d8dee8;
                border-width: 1px;
                VerticalLayout {
                    padding: 8px;
                    spacing: 2px;
                    Text { text: row.device + " / " + row.state; color: #1d2733; font-weight: 700; }
                    Text { text: row.detail; color: #536070; overflow: elide; }
                }
            }

            Text { text: "Device properties"; font-size: 18px; font-weight: 700; }
            ScrollView {
                vertical-stretch: 1;
                viewport-height: root.properties-height;
                VerticalLayout {
                    alignment: start;
                    padding-right: 14px;
                    for row in root.properties: VerticalLayout {
                        if row.table-start: Rectangle {
                            height: 34px;
                            background: #e6ecf5;
                            border-color: #cfd8e6;
                            border-width: 1px;
                            HorizontalLayout {
                                padding-left: 8px;
                                padding-right: 8px;
                                spacing: 8px;
                                Text {
                                    text: row.device;
                                    color: #1d2733;
                                    font-weight: 700;
                                    vertical-alignment: center;
                                }
                                Text {
                                    text: row.kinds;
                                    color: #6b7787;
                                    vertical-alignment: center;
                                    horizontal-alignment: right;
                                    overflow: elide;
                                }
                            }
                        }
                        Rectangle {
                            height: 36px;
                            background: #ffffff;
                            border-color: #e4e9f0;
                            border-width: 1px;
                            HorizontalLayout {
                                padding-left: 8px;
                                padding-right: 8px;
                                padding-top: 3px;
                                padding-bottom: 3px;
                                spacing: 8px;
                                Text {
                                    text: row.display;
                                    width: 140px;
                                    color: #536070;
                                    vertical-alignment: center;
                                    overflow: elide;
                                }
                                if row.control == "readonly": Text {
                                    text: row.value;
                                    color: #1d2733;
                                    vertical-alignment: center;
                                    overflow: elide;
                                }
                                if row.control == "bool": CheckBox {
                                    checked: row.checked;
                                    toggled => {
                                        root.property-edited(row.device, row.key, self.checked ? "true" : "false");
                                    }
                                }
                                if row.control == "choice": ComboBox {
                                    model: row.options;
                                    current-value: row.value;
                                    selected(value) => { root.property-edited(row.device, row.key, value); }
                                }
                                if row.control == "slider": Slider {
                                    minimum: row.slider-min;
                                    maximum: row.slider-max;
                                    value: row.slider-value;
                                    step: (row.slider-max - row.slider-min) / 200;
                                    released(value) => { root.property-slider(row.device, row.key, value); }
                                }
                                if row.control == "slider": LineEdit {
                                    width: 104px;
                                    text: row.value;
                                    accepted(text) => { root.property-edited(row.device, row.key, text); }
                                }
                                if row.control == "text": LineEdit {
                                    text: row.value;
                                    accepted(text) => { root.property-edited(row.device, row.key, text); }
                                }
                            }
                        }
                    }
                }
            }
            Text {
                text: "Each control follows the property schema: a checkbox for booleans, a "
                    + "drop-down for advertised values, a slider plus value box for ranged numbers "
                    + "(log scale when the range spans decades). Boxes commit on Enter.";
                color: #8b95a3;
                wrap: word-wrap;
            }
        }
    }
}
}

/// The only place this GUI names a concrete driver. Every device below is found
/// by kind tag, capability kind, or graph dependency role, so another
/// instrument can be substituted here without touching the rest of the GUI.
fn load_drivers(source: &str) -> Result<Vec<Box<dyn Driver>>> {
    match source {
        "sim-microscope" => Ok(vec![Box::new(SimMicroscopeDriver::simulated(DriverId(1)))]),
        other => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("unknown device source: {other}"),
        )),
    }
}

/// Sources whose runtime is assembled by [`crate::lsm_common`] rather than from
/// a plain driver list, because they are configured topologies rather than a
/// single simulated instrument.
fn is_scanning_source(source: &str) -> bool {
    matches!(
        source,
        "sim-lsm"
            | "sim_lsm"
            | "sim"
            | "sim-composed"
            | "sim_microscope_lsm"
            | "sim-microscope-lsm"
            | "imswitch"
            | "imswitch-daqmx"
            | "daqmx"
    )
}

/// First positional argument after the example name, ignoring flags such as
/// `--smoke`.
fn driver_choice() -> String {
    std::env::args()
        .skip(2)
        .find(|argument| !argument.starts_with('-'))
        .unwrap_or_else(|| "sim-microscope".into())
}

pub fn run() -> Result<()> {
    if std::env::args().any(|arg| arg == "--smoke") {
        let source = driver_choice();
        // The device survey lists every imager the source offers, which is how a
        // combined camera + scanning instrument shows up as two entries. A
        // source whose runtime cannot be surveyed still reports its acquisition
        // workflows below rather than failing outright.
        match GuiApp::new(&source) {
            Ok(mut app) => {
                if let Err(error) = app.print_smoke_output() {
                    // A scanning instrument still reports its acquisition
                    // workflows below, so an incomplete survey is noted rather
                    // than swallowing the rest of the output.
                    if !is_scanning_source(&source) {
                        return Err(error);
                    }
                    println!("device survey incomplete: {}", error.message);
                }
            }
            Err(error) if is_scanning_source(&source) => {
                println!("device survey unavailable: {}", error.message);
            }
            Err(error) => return Err(error),
        }
        // A scanning instrument is additionally exercised through its own
        // acquisition workflows, which the survey does not cover.
        if is_scanning_source(&source) {
            return crate::lsm_common::smoke(&source);
        }
        return Ok(());
    }

    let ui = MainWindow::new().map_err(|e| Error::new(ErrorCode::Driver, e.to_string()))?;
    let mut app = GuiApp::new(&driver_choice())?;
    app.refresh_ui(&ui)?;

    let app = Rc::new(RefCell::new(app));

    {
        let ui_weak = ui.as_weak();
        let app = Rc::clone(&app);
        ui.on_capture(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let result = app.borrow_mut().capture(&ui);
                report(&ui, result);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let app = Rc::clone(&app);
        ui.on_start_stream(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let result = app.borrow_mut().start_stream(&ui);
                report(&ui, result);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let app = Rc::clone(&app);
        ui.on_line_scan(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let result = app.borrow_mut().toggle_line_scan(&ui);
                report(&ui, result);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let app = Rc::clone(&app);
        ui.on_stop_stream(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let result = app.borrow_mut().stop_stream(&ui);
                report(&ui, result);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let app = Rc::clone(&app);
        ui.on_property_edited(move |device, key, value| {
            if let Some(ui) = ui_weak.upgrade() {
                let result = app.borrow_mut().edit_property(&ui, &device, &key, &value);
                report(&ui, result);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let app = Rc::clone(&app);
        ui.on_property_slider(move |device, key, position| {
            if let Some(ui) = ui_weak.upgrade() {
                let result = app
                    .borrow_mut()
                    .set_property_from_slider(&ui, &device, &key, position);
                report(&ui, result);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let app = Rc::clone(&app);
        ui.on_pan(move |dx, dy| {
            if let Some(ui) = ui_weak.upgrade() {
                let result = app.borrow_mut().pan(&ui, dx as f64, dy as f64);
                report(&ui, result);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let app = Rc::clone(&app);
        ui.on_focus_step(move |steps| {
            if let Some(ui) = ui_weak.upgrade() {
                let result = app.borrow_mut().focus_step(&ui, steps as f64);
                report(&ui, result);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        let app = Rc::clone(&app);
        ui.on_redraw(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let result = app.borrow_mut().redraw(&ui);
                report(&ui, result);
            }
        });
    }

    // Frames and property changes arrive asynchronously on the runtime event
    // bus, so the UI polls the subscription instead of blocking on a call.
    let timer = Timer::default();
    {
        let ui_weak = ui.as_weak();
        let app = Rc::clone(&app);
        timer.start(TimerMode::Repeated, Duration::from_millis(20), move || {
            if let Some(ui) = ui_weak.upgrade() {
                let result = app.borrow_mut().tick(&ui);
                report(&ui, result);
            }
        });
    }

    ui.run()
        .map_err(|e| Error::new(ErrorCode::Driver, e.to_string()))
}

fn report(ui: &MainWindow, result: Result<()>) {
    if let Err(error) = result {
        ui.set_status(format!("error: {}", error.message).into());
    }
}

struct GuiApp {
    runtime: LocalRuntime,
    devices: Vec<DeviceDescriptor>,
    imagers: Vec<ImagingSource>,
    pan_stages: Vec<DeviceDescriptor>,
    focus_stages: Vec<DeviceDescriptor>,
    events: numanager_core::runtime::Subscription,
    properties: BTreeMap<(DeviceId, String), Value>,
    rows: Rc<VecModel<PropertyRow>>,
    row_keys: Vec<(DeviceId, String)>,
    stream: Option<StreamSession>,
    last_frame: Option<Frame>,
    /// Raw detector samples, subscribed only so a scanning instrument can be
    /// driven a line at a time. Cameras never publish these.
    signal_events: numanager_core::runtime::Subscription,
    line_scan: Option<OperationId>,
    line_buffer: LineFramebuffer,
}

/// Framebuffer filled row by row from continuous line-scan chunks.
///
/// The scan sweeps down the raster, so line `n` is row `n % height` of the same
/// image the capture capability renders. Chunks land as partial rows and the
/// display is rebuilt from whatever has arrived, rather than waiting for a
/// complete frame.
#[derive(Default)]
struct LineFramebuffer {
    width: u32,
    height: u32,
    pixels: Vec<u16>,
    rows_written: u64,
    current_line: Option<u64>,
    dirty: bool,
}

impl LineFramebuffer {
    fn reset(&mut self, width: u32, height: u32) {
        self.width = width.clamp(1, 4096);
        self.height = height.clamp(1, 4096);
        self.pixels = vec![0; self.width as usize * self.height as usize];
        self.rows_written = 0;
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
            self.rows_written = self.rows_written.saturating_add(1);
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

    /// Mono8 view of the framebuffer, shaped like a camera frame so the rest of
    /// the GUI — preview, histogram, saturation marking — treats it identically.
    ///
    /// `mark_current` draws the row being scanned in white. It is applied to the
    /// copy handed to the display, so the stored sample codes are never
    /// modified and the marker cannot contaminate the acquired image.
    fn frame(&self, device: DeviceId, mark_current: bool) -> Frame {
        let mut data: Vec<u8> = self.pixels.iter().map(|code| (code >> 8) as u8).collect();
        if mark_current {
            if let Some(line) = self.current_line {
                let row = (line % u64::from(self.height)) as usize;
                let start = row * self.width as usize;
                data[start..start + self.width as usize].fill(u8::MAX);
            }
        }
        Frame {
            handle: FrameHandle {
                stream: StreamId(0),
                frame: FrameId(self.rows_written),
            },
            device,
            width: self.width,
            height: self.height,
            pixel_format: "Mono8".into(),
            data,
            metadata: BTreeMap::new(),
            buffer: FrameBufferSpec::default(),
        }
    }
}

/// A device that can produce images, and the capabilities it offers to do so.
///
/// A camera advertises `CameraCapture` and usually `CameraStream`. A laser
/// scanning microscope advertises `ConfocalImageCapture`, and when it can also
/// publish raw detector samples it advertises `ScanSignalStream`, which is what
/// makes line-by-line acquisition possible.
struct ImagingSource {
    device: DeviceDescriptor,
    /// Capability used for a single image.
    capture: CapabilityKind,
    /// Capability used for continuous whole images, when one is advertised.
    stream: Option<CapabilityKind>,
    /// True when the device can be driven one scan line at a time.
    line_scan: bool,
    /// Objective in this camera's light path, resolved from the device graph at
    /// startup. Absent when the instrument does not publish one.
    objective: Option<DeviceDescriptor>,
}

struct StreamSession {
    operation: OperationId,
    device: DeviceId,
    started: Instant,
    frames: u64,
}

/// Micrometres of sample per image pixel, and where that number came from.
struct Optics {
    um_per_image_px: f64,
    derived: bool,
    source: String,
}

impl GuiApp {
    fn new(source: &str) -> Result<Self> {
        // Scanning topologies come pre-assembled; a driver-list source is built
        // here so the device graph can be inspected before the runtime takes
        // ownership of the drivers.
        let (runtime, providers) = if is_scanning_source(source) {
            let (runtime, _) = crate::lsm_common::runtime_for_source(source)?;
            (runtime, Vec::new())
        } else {
            let drivers = load_drivers(source)?;
            let providers = capability_providers(
                drivers.iter().map(|driver| driver.as_ref()),
                CapabilityKind::CameraCapture,
            );
            let mut runtime = LocalRuntime::new();
            for driver in drivers {
                runtime.add_driver(driver)?;
            }
            (runtime, providers)
        };
        let objective_role = Role::Custom(OBJECTIVE_ROLE.into());

        let devices = runtime
            .devices()
            .into_iter()
            .cloned()
            .collect::<Vec<DeviceDescriptor>>();

        let ids_with = |kind| {
            runtime
                .devices_by_capability(kind)
                .into_iter()
                .map(|device| device.id)
                .collect::<Vec<_>>()
        };
        let camera_streams = ids_with(CapabilityKind::CameraStream);
        let confocal_streams = ids_with(CapabilityKind::ConfocalImageStream);
        let line_scanners = ids_with(CapabilityKind::ScanSignalStream);
        let turrets = devices
            .iter()
            .filter(|device| device.has_kind("objective.turret"))
            .cloned()
            .collect::<Vec<_>>();
        // Both kinds of imager are collected the same way; only the capability
        // used to acquire differs, so the rest of the GUI does not care which
        // one the instrument turned out to be.
        let capture_kinds = [
            CapabilityKind::CameraCapture,
            CapabilityKind::ConfocalImageCapture,
        ];
        let imagers = capture_kinds
            .into_iter()
            .flat_map(|capture| {
                runtime
                    .devices_by_capability(capture.clone())
                    .into_iter()
                    .cloned()
                    .map(move |device| (capture.clone(), device))
                    .collect::<Vec<_>>()
            })
            .map(|(capture, device)| {
                let objective = providers
                    .iter()
                    .find(|provider| provider.device.id == device.id)
                    .and_then(|provider| {
                        provider
                            .dependency_device(&objective_role)
                            .or_else(|| {
                                provider.dependencies.iter().find_map(|dependency| {
                                    dependency
                                        .device
                                        .as_ref()
                                        .filter(|device| device.has_kind("objective.turret"))
                                })
                            })
                            .cloned()
                    })
                    .or_else(|| turrets.first().filter(|_| turrets.len() == 1).cloned());
                let stream = if capture == CapabilityKind::CameraCapture {
                    camera_streams
                        .contains(&device.id)
                        .then_some(CapabilityKind::CameraStream)
                } else {
                    confocal_streams
                        .contains(&device.id)
                        .then_some(CapabilityKind::ConfocalImageStream)
                };
                ImagingSource {
                    capture,
                    stream,
                    line_scan: line_scanners.contains(&device.id),
                    objective,
                    device,
                }
            })
            .collect::<Vec<_>>();
        if imagers.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "no device advertises CameraCapture or ConfocalImageCapture",
            ));
        }
        let pan_stages = runtime
            .devices_by_kind("axis.xy")
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let focus_stages = runtime
            .devices_by_kind("axis.z")
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();

        let events = runtime.subscribe(EventFilter::kinds([
            EventKind::FrameReady,
            EventKind::PropertyChanged,
        ]));
        let signal_events = runtime.subscribe(EventFilter::kinds([EventKind::ScanSignalChunk]));

        let mut app = Self {
            runtime,
            devices,
            imagers,
            pan_stages,
            focus_stages,
            events,
            properties: BTreeMap::new(),
            rows: Rc::new(VecModel::default()),
            row_keys: Vec::new(),
            stream: None,
            last_frame: None,
            signal_events,
            line_scan: None,
            line_buffer: LineFramebuffer::default(),
        };
        app.seed_properties();
        Ok(app)
    }

    /// Reads every public property once through the runtime. A device that
    /// refuses a read is skipped rather than fatal, so an instrument with
    /// write-only or unavailable properties still opens.
    fn seed_properties(&mut self) {
        let reads = self
            .devices
            .iter()
            .flat_map(|device| {
                device
                    .properties
                    .iter()
                    .filter(|schema| schema.readable && is_public_property(schema))
                    .map(|schema| (device.id, schema.key.clone()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        for (device, key) in reads {
            if let Ok(value) = self
                .runtime
                .execute(Command::read_property(device, &key), Duration::from_secs(1))
            {
                self.properties.insert((device, key), value);
            }
        }
    }

    /// Re-reads properties the driver marks volatile, which change without a
    /// client write — the derived optical scale is the important one.
    fn refresh_volatile(&mut self) {
        let reads = self
            .devices
            .iter()
            .flat_map(|device| {
                device
                    .properties
                    .iter()
                    .filter(|schema| schema.readable && schema.volatile)
                    .map(|schema| (device.id, schema.key.clone()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        for (device, key) in reads {
            if let Ok(value) = self
                .runtime
                .execute(Command::read_property(device, &key), Duration::from_secs(1))
            {
                self.properties.insert((device, key), value);
            }
        }
    }

    fn refresh_ui(&mut self, ui: &MainWindow) -> Result<()> {
        ui.set_camera_sources(ModelRc::new(VecModel::from(
            self.imagers
                .iter()
                .map(|camera| SharedString::from(camera.device.label.as_str()))
                .collect::<Vec<_>>(),
        )));
        ui.set_pan_stages(ModelRc::new(VecModel::from(
            self.pan_stages
                .iter()
                .map(|stage| SharedString::from(stage.label.as_str()))
                .collect::<Vec<_>>(),
        )));
        ui.set_focus_stages(ModelRc::new(VecModel::from(
            self.focus_stages
                .iter()
                .map(|stage| SharedString::from(stage.label.as_str()))
                .collect::<Vec<_>>(),
        )));
        ui.set_has_pan_stage(!self.pan_stages.is_empty());
        ui.set_has_focus_stage(!self.focus_stages.is_empty());
        ui.set_stream_supported(
            self.selected_imager(ui)
                .is_ok_and(|camera| camera.stream.is_some()),
        );
        self.build_property_model(ui);
        self.set_safety_model(ui)?;
        self.refresh_optics(ui);
        ui.set_status(
            "Ready — Capture takes one frame, Start streaming runs until you stop it".into(),
        );
        Ok(())
    }

    fn print_smoke_output(&mut self) -> Result<()> {
        println!("software gui smoke");
        println!("imagers:");
        for imager in &self.imagers {
            // capture/stream/line_scan are the acquisition modes the GUI offers
            // for this device, so a combined instrument shows a camera and a
            // scanner side by side with different modes each.
            println!(
                "  {} [{}] capture={} stream={} line_scan={}",
                imager.device.label,
                public_kind_summary(&imager.device),
                imager.capture.name(),
                imager.stream.is_some(),
                imager.line_scan
            );
        }
        println!("pan stages:");
        for stage in &self.pan_stages {
            println!("  {} [{}]", stage.label, public_kind_summary(stage));
        }
        println!("focus stages:");
        for stage in &self.focus_stages {
            println!("  {} [{}]", stage.label, public_kind_summary(stage));
        }
        println!("objectives:");
        for camera in &self.imagers {
            match &camera.objective {
                Some(objective) => println!(
                    "  {} -> {} [{}]",
                    camera.device.label,
                    objective.label,
                    public_kind_summary(objective)
                ),
                None => println!("  {} -> none", camera.device.label),
            }
        }
        // The camera acquisition survey below speaks CameraCapture, so it runs
        // against the first camera. An instrument with only a scanner has its
        // acquisition covered by the scanning workflows instead.
        let camera_imager = self
            .imagers
            .iter()
            .find(|imager| imager.capture == CapabilityKind::CameraCapture)
            .map(|imager| imager.device.id);
        let camera = camera_imager.unwrap_or(self.imagers[0].device.id);
        println!("optics: {}", self.optics(camera).summary());

        println!("properties:");
        for device in &self.devices {
            for schema in &device.properties {
                let value = self
                    .properties
                    .get(&(device.id, schema.key.clone()))
                    .map(format_value)
                    .unwrap_or_default();
                println!(
                    "  {}.{} = {} writable={}",
                    device.label, schema.key, value, schema.writable
                );
            }
        }
        println!("safety:");
        for device in &self.devices {
            if device
                .properties
                .iter()
                .any(|property| SafetySummary::property_key_is_safety(&property.key))
            {
                let summary = self
                    .runtime
                    .safety_summary(device.id, Duration::from_secs(1))?;
                println!(
                    "  {} = {} {}",
                    device.label,
                    summary.state.name(),
                    safety_detail(&summary)
                );
            }
        }

        // Selecting a different objective must move the derived scale without
        // the GUI knowing anything about the instrument behind it. Where the
        // turret advertises FilterSelect the operation waits out the rotation,
        // so no polling is needed.
        if let Some(objective) = self.imagers[0].objective.clone() {
            let selects_by_capability = self
                .runtime
                .capabilities(objective.id)?
                .iter()
                .any(|capability| capability.kind == CapabilityKind::FilterSelect);
            let schema = objective
                .properties
                .iter()
                .find(|schema| schema.key == "position" && schema.writable)
                .cloned();
            if let Some(schema) = schema {
                let selected = self
                    .properties
                    .get(&(objective.id, "position".into()))
                    .cloned();
                let mut order = schema
                    .enum_values
                    .iter()
                    .filter(|choice| Some(&choice.value) != selected.as_ref())
                    .take(1)
                    .cloned()
                    .collect::<Vec<_>>();
                order.extend(
                    selected
                        .and_then(|value| {
                            schema
                                .enum_values
                                .iter()
                                .find(|choice| choice.value == value)
                                .cloned()
                        })
                        .into_iter(),
                );
                for choice in order {
                    if selects_by_capability {
                        if let Value::I64(position) = choice.value {
                            self.runtime.execute_request(
                                objective.id,
                                FilterSelectRequest::position(position as u8),
                                Duration::from_secs(10),
                            )?;
                        }
                    } else {
                        self.runtime.execute(
                            Command::write_property(objective.id, "position", choice.value.clone()),
                            Duration::from_secs(10),
                        )?;
                    }
                    self.refresh_volatile();
                    println!(
                        "objective {}: {}",
                        choice.label,
                        self.optics(camera).summary()
                    );
                }
            }
        }

        if camera_imager.is_none() {
            println!("capture: skipped, no imager advertises CameraCapture");
            return Ok(());
        }

        let capture = self.runtime.execute_request(
            camera,
            CameraCaptureRequest {
                encoding: Some(ImageEncoding::Mono8),
                buffer: Some(FrameBufferSpec::default()),
            },
            Duration::from_secs(10),
        )?;
        let (capture_frame, _) = self.drain_last_frame(camera)?;
        println!(
            "capture: {} {}",
            completion_summary(&capture),
            frame_smoke_summary(capture_frame.as_ref())
        );

        let stream = self.runtime.execute_request(
            camera,
            CameraStreamRequest {
                encoding: Some(ImageEncoding::Mono8),
                frame_count: Some(12),
                buffer: FrameBufferSpec {
                    capacity_frames: 8,
                    overflow: OverflowPolicy::DropOldest,
                },
            },
            Duration::from_secs(30),
        )?;
        let (stream_frame, frames) = self.drain_last_frame(camera)?;
        println!(
            "stream: {} frames={frames} {}",
            completion_summary(&stream),
            frame_smoke_summary(stream_frame.as_ref())
        );
        if let Some(frame) = stream_frame {
            if let Some(status) = self.runtime.stream_status(frame.handle.stream)? {
                println!(
                    "stream status: depth={} capacity={} dropped={} latest={:?} {}",
                    status.depth(),
                    status.capacity(),
                    status.dropped_frames,
                    status.latest(),
                    completion_summary(&status.as_value())
                );
            }
        }
        Ok(())
    }

    fn capture(&mut self, ui: &MainWindow) -> Result<()> {
        if self.stream.is_some() {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "camera is streaming; stop the stream before a single capture",
            ));
        }
        let imager = self.selected_imager(ui)?;
        let device = imager.device.id;
        // One image, through whichever capability this instrument offers.
        if imager.capture == CapabilityKind::CameraCapture {
            self.runtime.execute_request(
                device,
                CameraCaptureRequest {
                    encoding: Some(ImageEncoding::Mono8),
                    buffer: Some(FrameBufferSpec::default()),
                },
                Duration::from_secs(10),
            )?;
        } else {
            let width = ui.get_scan_width().round().clamp(64.0, 2048.0) as i64;
            let height = ui.get_scan_height().round().clamp(1.0, 2048.0) as i64;
            self.runtime.execute_request(
                device,
                crate::lsm_common::snapshot_request(width, height),
                Duration::from_secs(30),
            )?;
        }
        let camera = device;
        let (frame, _) = self.drain_last_frame(camera)?;
        if let Some(frame) = frame {
            self.show_frame(ui, frame);
        }
        ui.set_status("captured one frame".into());
        Ok(())
    }

    /// Starts an open-ended stream: `frame_count: None` asks the camera to keep
    /// delivering frames until the operation is cancelled.
    fn start_stream(&mut self, ui: &MainWindow) -> Result<()> {
        if self.stream.is_some() {
            return Ok(());
        }
        let imager = self.selected_imager(ui)?;
        let Some(stream_kind) = imager.stream.clone() else {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "device does not advertise a stream capability",
            ));
        };
        let camera = imager.device.id;
        let operation = if stream_kind == CapabilityKind::CameraStream {
            self.runtime.submit_request(
                camera,
                CameraStreamRequest {
                    encoding: Some(ImageEncoding::Mono8),
                    frame_count: None,
                    buffer: FrameBufferSpec {
                        capacity_frames: 8,
                        overflow: OverflowPolicy::DropOldest,
                    },
                },
            )?
        } else {
            let width = ui.get_scan_width().round().clamp(64.0, 2048.0) as i64;
            let height = ui.get_scan_height().round().clamp(1.0, 2048.0) as i64;
            self.runtime.submit_request(
                camera,
                crate::lsm_common::continuous_live_image_request(width, height),
            )?
        };
        self.stream = Some(StreamSession {
            operation: operation.id,
            device: camera,
            started: Instant::now(),
            frames: 0,
        });
        ui.set_streaming(true);
        ui.set_status("streaming…".into());
        Ok(())
    }

    fn stop_stream(&mut self, ui: &MainWindow) -> Result<()> {
        let Some(session) = self.stream.take() else {
            return Ok(());
        };
        ui.set_streaming(false);
        let result = self.runtime.cancel(session.operation)?;
        ui.set_status(
            format!(
                "stream {} after {} frames ({:.1} fps)",
                match result {
                    CancelResult::Cancelled => "stopped",
                    CancelResult::AlreadyFinished => "already finished",
                    CancelResult::Unsupported => "stop unsupported",
                },
                session.frames,
                session.rate()
            )
            .into(),
        );
        Ok(())
    }

    /// Polled from the UI event loop: moves newly published frames into the
    /// image view, folds property changes into the cached values, and reflects
    /// the live operation status in the controls.
    /// Starts or stops line-by-line acquisition on a scanning instrument.
    fn toggle_line_scan(&mut self, ui: &MainWindow) -> Result<()> {
        if self.line_scan.is_some() {
            return self.stop_line_scan(ui);
        }
        let imager = self.selected_imager(ui)?;
        if !imager.line_scan {
            return Err(Error::new(
                ErrorCode::Unsupported,
                "device does not advertise ScanSignalStream",
            ));
        }
        let device = imager.device.id;
        let width = ui.get_scan_width().round().clamp(64.0, 2048.0) as u32;
        let height = ui.get_scan_height().round().clamp(1.0, 2048.0) as u32;
        // Without detector channels the scan publishes chunks carrying no
        // samples, so the framebuffer would stay empty.
        let request = crate::lsm_common::continuous_raster_line_signal_request(
            i64::from(width),
            i64::from(height),
            256,
            crate::lsm_common::signal_channels(),
        );
        let operation = self.runtime.submit_request(device, request)?;
        self.line_buffer.reset(width, height);
        self.draw_line_buffer(ui, device);
        self.line_scan = Some(operation.id);
        ui.set_line_scanning(true);
        ui.set_status("line scanning — building image row by row".into());
        Ok(())
    }

    fn stop_line_scan(&mut self, ui: &MainWindow) -> Result<()> {
        if let Some(operation) = self.line_scan.take() {
            let _ = self.runtime.cancel(operation);
        }
        ui.set_line_scanning(false);
        ui.set_status(
            format!(
                "line scanning stopped after {} rows",
                self.line_buffer.rows_written
            )
            .into(),
        );
        Ok(())
    }

    /// Drains whatever chunks arrived since the last tick into the framebuffer.
    fn drain_line_scan(&mut self, ui: &MainWindow, device: DeviceId) -> Result<()> {
        while let Some(event) = self.signal_events.try_recv() {
            let Event::ScanSignalChunk(event) = event else {
                continue;
            };
            // The raster averages the detectors it was given, so the
            // framebuffer averages the channels the chunk carries.
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
        }

        if self.line_buffer.dirty {
            self.line_buffer.dirty = false;
            self.draw_line_buffer(ui, device);
        }

        if let Some(operation) = self.line_scan {
            match self.runtime.status(operation) {
                OperationStatus::Queued | OperationStatus::Running { .. } => {
                    ui.set_status(
                        format!(
                            "line scanning — {} rows written",
                            self.line_buffer.rows_written
                        )
                        .into(),
                    );
                }
                _ => {
                    self.line_scan = None;
                    ui.set_line_scanning(false);
                    ui.set_status("line scanning ended".into());
                }
            }
        }
        Ok(())
    }

    /// Publishes the framebuffer as if it were a captured frame, so preview,
    /// histogram and saturation marking all work unchanged.
    fn draw_line_buffer(&mut self, ui: &MainWindow, device: DeviceId) {
        let frame = self.line_buffer.frame(device, ui.get_mark_scan_line());
        self.show_frame(ui, frame);
    }

    fn tick(&mut self, ui: &MainWindow) -> Result<()> {
        let device = match &self.stream {
            Some(session) => session.device,
            None => self.selected_imager(ui)?.device.id,
        };
        let (frame, count, changed) = self.drain_events(device)?;
        if let Some(frame) = frame {
            self.show_frame(ui, frame);
        }
        if changed {
            self.refresh_property_values();
            self.refresh_optics(ui);
        }
        ui.set_stream_supported(
            self.selected_imager(ui)
                .is_ok_and(|camera| camera.stream.is_some()),
        );
        ui.set_line_scan_supported(
            self.selected_imager(ui)
                .is_ok_and(|imager| imager.line_scan),
        );
        if self.line_scan.is_some() {
            let scanner = self.selected_imager(ui)?.device.id;
            self.drain_line_scan(ui, scanner)?;
        }
        let Some(session) = self.stream.as_mut() else {
            return Ok(());
        };
        session.frames += count as u64;
        match self.runtime.status(session.operation) {
            OperationStatus::Queued | OperationStatus::Running { .. } => {
                ui.set_status(
                    format!(
                        "streaming — {} frames, {:.1} fps",
                        session.frames,
                        session.rate()
                    )
                    .into(),
                );
            }
            _ => {
                let frames = session.frames;
                self.stream = None;
                ui.set_streaming(false);
                ui.set_status(format!("stream ended after {frames} frames").into());
            }
        }
        Ok(())
    }

    fn show_frame(&mut self, ui: &MainWindow, frame: Frame) {
        self.draw_frame(ui, &frame);
        self.last_frame = Some(frame);
        self.refresh_optics(ui);
    }

    /// Re-renders the frame already on screen, for display-only changes.
    fn redraw(&mut self, ui: &MainWindow) -> Result<()> {
        if let Some(frame) = self.last_frame.take() {
            self.draw_frame(ui, &frame);
            self.last_frame = Some(frame);
        }
        Ok(())
    }

    fn draw_frame(&self, ui: &MainWindow, frame: &Frame) {
        let saturation = ui.get_mark_saturated().then_some(SATURATION_CODE);
        ui.set_camera_frame(image_from_mono8(frame, saturation));
        ui.set_histogram_path(histogram_path(&histogram(frame)).into());
    }

    fn edit_property(
        &mut self,
        ui: &MainWindow,
        device_label: &str,
        key: &str,
        text: &str,
    ) -> Result<()> {
        let Some((device, schema)) = self.writable(device_label, key) else {
            return Ok(());
        };
        // An enum property is edited by picking one of the labels the schema
        // advertises, so map the label back to the value it stands for.
        let value = schema
            .enum_values
            .iter()
            .find(|candidate| candidate.label == text)
            .map(|candidate| candidate.value.clone())
            .unwrap_or_else(|| parse_property_value(schema.value_type, text));
        self.commit(ui, device, key, value)
    }

    /// Slider positions arrive in the domain [`slider_scale`] chose for the
    /// property, so the same scale converts them back to a canonical value.
    fn set_property_from_slider(
        &mut self,
        ui: &MainWindow,
        device_label: &str,
        key: &str,
        position: f32,
    ) -> Result<()> {
        let Some((device, schema)) = self.writable(device_label, key) else {
            return Ok(());
        };
        let Some(scale) = slider_scale(schema) else {
            return Ok(());
        };
        let canonical = scale.to_canonical(position as f64);
        let value = parse_property_value(schema.value_type, &format!("{canonical}"));
        self.commit(ui, device, key, value)
    }

    fn commit(&mut self, ui: &MainWindow, device: DeviceId, key: &str, value: Value) -> Result<()> {
        let value = self
            .schema(&(device, key.to_string()))
            .map(|schema| snap_to_increment(schema, value.clone()))
            .unwrap_or(value);
        self.runtime.execute(
            Command::write_property(device, key, value.clone()),
            Duration::from_secs(5),
        )?;
        self.properties.insert((device, key.to_string()), value);
        self.refresh_volatile();
        // Rebuild rather than patch: a widget the user has typed into or dragged
        // no longer tracks its model binding, so it needs a fresh item to show
        // the canonical value the driver accepted.
        self.build_property_model(ui);
        self.set_safety_model(ui)?;
        self.refresh_optics(ui);
        Ok(())
    }

    fn writable(&self, device_label: &str, key: &str) -> Option<(DeviceId, &PropertySchema)> {
        let device = self
            .devices
            .iter()
            .find(|device| device.label == device_label)?;
        let schema = device
            .properties
            .iter()
            .find(|schema| schema.key == key)
            .filter(|schema| schema.writable)?;
        Some((device.id, schema))
    }

    fn pan(&mut self, ui: &MainWindow, dx: f64, dy: f64) -> Result<()> {
        let Some(stage) = self.selected_pan_stage(ui).cloned() else {
            return Ok(());
        };
        let camera = self.selected_imager(ui)?.device.id;
        let step = self.optics(camera).um_per_image_px;
        let x_key = (stage.id, "x".to_string());
        let y_key = (stage.id, "y".to_string());
        let x = self.clamped_axis(&stage, "x", -dx * step);
        let y = self.clamped_axis(&stage, "y", -dy * step);
        let state = StateSet::immediate("mouse-pan")
            .with_write(
                stage.id,
                "x",
                Value::Position(Position::from_micrometers(x)),
            )
            .with_write(
                stage.id,
                "y",
                Value::Position(Position::from_micrometers(y)),
            );
        self.runtime
            .execute(state.into_command(), Duration::from_secs(5))?;
        self.properties
            .insert(x_key, Value::Position(Position::from_micrometers(x)));
        self.properties
            .insert(y_key, Value::Position(Position::from_micrometers(y)));
        self.refresh_property_values();
        self.refresh_optics(ui);
        Ok(())
    }

    /// Nudges focus by whole steps of whatever increment the Z stage advertises.
    fn focus_step(&mut self, ui: &MainWindow, steps: f64) -> Result<()> {
        let Some(stage) = self.selected_focus_stage(ui).cloned() else {
            return Ok(());
        };
        let Some(schema) = stage.properties.iter().find(|schema| schema.key == "z") else {
            return Ok(());
        };
        let step = value_as_f64(schema.increment.as_ref())
            .filter(|step| *step > 0.0)
            .unwrap_or(1.0);
        let target = self.snapped_axis(&stage, "z", steps * step);
        self.commit(
            ui,
            stage.id,
            "z",
            Value::Position(Position::from_micrometers(target)),
        )
    }

    /// Applies a relative move, clips it to the travel range the stage
    /// advertises, and lands it on an advertised step, so the runtime never
    /// rejects the write.
    fn snapped_axis(&self, stage: &DeviceDescriptor, key: &str, delta_um: f64) -> f64 {
        let target = self.clamped_axis(stage, key, delta_um);
        let Some(schema) = stage.properties.iter().find(|p| p.key == key) else {
            return target;
        };
        value_as_f64(Some(&snap_to_increment(
            schema,
            Value::Position(Position::from_micrometers(target)),
        )))
        .unwrap_or(target)
    }

    /// Applies a relative move and clips it to the travel range the stage
    /// advertises, so the runtime never sees an out-of-range write.
    fn clamped_axis(&self, stage: &DeviceDescriptor, key: &str, delta_um: f64) -> f64 {
        let current =
            value_as_f64(self.properties.get(&(stage.id, key.to_string()))).unwrap_or(0.0);
        let target = current + delta_um;
        let Some(schema) = stage.properties.iter().find(|p| p.key == key) else {
            return target;
        };
        let Some(range) = &schema.range else {
            return target;
        };
        let min = value_as_f64(Some(&range.min)).unwrap_or(f64::NEG_INFINITY);
        let max = value_as_f64(Some(&range.max)).unwrap_or(f64::INFINITY);
        target.clamp(min, max)
    }

    /// Micrometres of sample per image pixel, from the camera's pixel pitch and
    /// binning and the magnification of the objective in its light path. Falls
    /// back to the stage's advertised increment, then to a fixed step, and says
    /// which of the three it used.
    fn optics(&self, camera: DeviceId) -> Optics {
        let pitch = value_as_f64(self.properties.get(&(camera, "pixel_pitch".into())));
        let binning = binning_factor(self.properties.get(&(camera, "binning".into())));
        let objective = self
            .imagers
            .iter()
            .find(|source| source.device.id == camera)
            .and_then(|source| source.objective.as_ref());
        let magnification = match objective {
            Some(objective) => {
                value_as_f64(self.properties.get(&(objective.id, "magnification".into())))
            }
            None => Some(1.0),
        };
        if let (Some(pitch), Some(magnification)) = (pitch, magnification) {
            if magnification > 0.0 {
                let camera_label = self
                    .devices
                    .iter()
                    .find(|device| device.id == camera)
                    .map(|device| device.label.as_str())
                    .unwrap_or("camera");
                let source = match objective {
                    Some(objective) => format!(
                        "{camera_label} pixel pitch {} um x binning {binning} / {} magnification {}",
                        format_scalar(pitch),
                        objective.label,
                        format_scalar(magnification)
                    ),
                    None => format!(
                        "{camera_label} pixel pitch {} um x binning {binning}",
                        format_scalar(pitch)
                    ),
                };
                return Optics {
                    um_per_image_px: pitch * binning as f64 / magnification,
                    derived: true,
                    source,
                };
            }
        }
        if let Some(stage) = self.pan_stages.first() {
            if let Some(step) = stage
                .properties
                .iter()
                .find(|schema| schema.key == "x")
                .and_then(|schema| value_as_f64(schema.increment.as_ref()))
                .filter(|step| *step > 0.0)
            {
                return Optics {
                    um_per_image_px: step,
                    derived: false,
                    source: format!("{} advertised step size", stage.label),
                };
            }
        }
        Optics {
            um_per_image_px: DEFAULT_UM_PER_DRAG_PX,
            derived: false,
            source: "a built-in step, since no device publishes an optical scale".into(),
        }
    }

    fn refresh_optics(&self, ui: &MainWindow) {
        let Ok(camera) = self.selected_imager(ui) else {
            return;
        };
        let optics = self.optics(camera.device.id);
        ui.set_pan_hint(
            if optics.derived {
                format!(
                    "Drag on the image to move the stage one image pixel per screen pixel: {}. \
                     Moves are clipped to the travel range the stage advertises.",
                    optics.summary()
                )
            } else {
                format!(
                    "Drag on the image to move the stage {} um per image pixel, taken from {}. \
                     No device publishes a pixel pitch and a magnification, so this is not a \
                     calibrated scale.",
                    format_scalar(optics.um_per_image_px),
                    optics.source
                )
            }
            .into(),
        );
        self.set_xy_readout(ui, &optics);
        self.set_focus_readout(ui);
        self.set_scale_bar(ui, &optics);
    }

    fn set_scale_bar(&self, ui: &MainWindow, optics: &Optics) {
        let Some(frame) = &self.last_frame else {
            ui.set_scale_bar_pixels(0.0);
            return;
        };
        if !optics.derived || optics.um_per_image_px <= 0.0 {
            ui.set_scale_bar_pixels(0.0);
            return;
        }
        let target_um = frame.width as f64 * optics.um_per_image_px * 0.2;
        let length_um = SCALE_BAR_STEPS_UM
            .iter()
            .rev()
            .copied()
            .find(|candidate| *candidate <= target_um)
            .unwrap_or(SCALE_BAR_STEPS_UM[0]);
        ui.set_scale_bar_pixels((length_um / optics.um_per_image_px) as f32);
        ui.set_scale_bar_label(format!("{} um", format_scalar(length_um)).into());
    }

    fn drain_last_frame(&mut self, selected_imager: DeviceId) -> Result<(Option<Frame>, usize)> {
        let (frame, count, _) = self.drain_events(selected_imager)?;
        Ok((frame, count))
    }

    fn drain_events(&mut self, selected_imager: DeviceId) -> Result<(Option<Frame>, usize, bool)> {
        let mut last = None;
        let mut count = 0;
        let mut changed = false;
        while let Some(event) = self.events.try_recv() {
            match event {
                Event::FrameReady(event) if event.device == selected_imager => {
                    last = Some(event.handle);
                    count += 1;
                }
                Event::PropertyChanged(event) => {
                    self.properties
                        .insert((event.device, event.key), event.value);
                    changed = true;
                }
                _ => {}
            }
        }
        let frame = last
            .map(|handle| self.runtime.frame(handle))
            .transpose()?
            .flatten();
        Ok((frame, count, changed))
    }

    /// Builds one table per device: a header row carrying the device label and
    /// its public kinds, followed by one row per property. Each row's editor is
    /// chosen from the property schema — see [`property_row`].
    fn build_property_model(&mut self, ui: &MainWindow) {
        let mut rows = Vec::new();
        let mut keys = Vec::new();
        for device in &self.devices {
            for (index, schema) in device.properties.iter().enumerate() {
                let value = self.properties.get(&(device.id, schema.key.clone()));
                rows.push(property_row(device, schema, value, index == 0));
                keys.push((device.id, schema.key.clone()));
            }
        }
        let tables = self.devices.len() as f32;
        ui.set_properties_height(rows.len() as f32 * ROW_HEIGHT_PX + tables * HEADER_HEIGHT_PX);
        self.rows = Rc::new(VecModel::from(rows));
        self.row_keys = keys;
        ui.set_properties(ModelRc::from(Rc::clone(&self.rows)));
    }

    /// Updates value cells in place for changes the user did not type in
    /// (panning, for instance), so the table does not flicker at drag rate.
    fn refresh_property_values(&self) {
        for (index, key) in self.row_keys.iter().enumerate() {
            let (Some(mut row), Some(device), Some(schema)) = (
                self.rows.row_data(index),
                self.device(key.0),
                self.schema(key),
            ) else {
                continue;
            };
            let fresh = property_row(device, schema, self.properties.get(key), row.table_start);
            if row.value != fresh.value {
                row.value = fresh.value;
                row.checked = fresh.checked;
                row.slider_value = fresh.slider_value;
                self.rows.set_row_data(index, row);
            }
        }
    }

    fn device(&self, id: DeviceId) -> Option<&DeviceDescriptor> {
        self.devices.iter().find(|device| device.id == id)
    }

    fn schema(&self, key: &(DeviceId, String)) -> Option<&PropertySchema> {
        self.device(key.0)?
            .properties
            .iter()
            .find(|schema| schema.key == key.1)
    }

    fn set_safety_model(&self, ui: &MainWindow) -> Result<()> {
        let rows = self
            .devices
            .iter()
            .filter(|device| {
                device
                    .properties
                    .iter()
                    .any(|property| SafetySummary::property_key_is_safety(&property.key))
            })
            .map(|device| {
                let summary = self
                    .runtime
                    .safety_summary(device.id, Duration::from_secs(1))?;
                Ok(SafetyRow {
                    device: device.label.as_str().into(),
                    state: summary.state.name().into(),
                    detail: safety_readout(&summary).into(),
                    alarm: summary.state != SafetyState::Safe,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        ui.set_safety(ModelRc::new(VecModel::from(rows)));
        Ok(())
    }

    fn set_xy_readout(&self, ui: &MainWindow, optics: &Optics) {
        let mut parts = Vec::new();
        for stage in &self.pan_stages {
            parts.push(format!(
                "{}: x={:.2} y={:.2} um",
                stage.label,
                self.axis(stage, "x"),
                self.axis(stage, "y")
            ));
        }
        parts.push(format!(
            "1 image px = {} um{}",
            format_scalar(optics.um_per_image_px),
            if optics.derived {
                ""
            } else {
                " (uncalibrated)"
            }
        ));
        ui.set_xy_readout(parts.join("   ").into());
    }

    fn set_focus_readout(&self, ui: &MainWindow) {
        let Some(stage) = self.selected_focus_stage(ui) else {
            ui.set_focus_readout(SharedString::default());
            return;
        };
        let step = stage
            .properties
            .iter()
            .find(|schema| schema.key == "z")
            .and_then(|schema| value_as_f64(schema.increment.as_ref()))
            .unwrap_or(1.0);
        ui.set_focus_fine_label(format_scalar(step * 10.0).into());
        ui.set_focus_coarse_label(format_scalar(step * 100.0).into());
        ui.set_focus_readout(format!("z={:.2} um   (steps in um)", self.axis(stage, "z")).into());
    }

    fn axis(&self, stage: &DeviceDescriptor, key: &str) -> f64 {
        value_as_f64(self.properties.get(&(stage.id, key.to_string()))).unwrap_or(0.0)
    }

    fn selected_imager(&self, ui: &MainWindow) -> Result<&ImagingSource> {
        usize::try_from(ui.get_selected_camera())
            .ok()
            .and_then(|index| self.imagers.get(index))
            .or_else(|| self.imagers.first())
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "invalid camera selection"))
    }

    fn selected_pan_stage(&self, ui: &MainWindow) -> Option<&DeviceDescriptor> {
        usize::try_from(ui.get_selected_pan_stage())
            .ok()
            .and_then(|index| self.pan_stages.get(index))
            .or_else(|| self.pan_stages.first())
    }

    fn selected_focus_stage(&self, ui: &MainWindow) -> Option<&DeviceDescriptor> {
        usize::try_from(ui.get_selected_focus_stage())
            .ok()
            .and_then(|index| self.focus_stages.get(index))
            .or_else(|| self.focus_stages.first())
    }
}

impl Optics {
    fn summary(&self) -> String {
        format!(
            "{} um per image pixel, from {}",
            format_scalar(self.um_per_image_px),
            self.source
        )
    }
}

impl StreamSession {
    fn rate(&self) -> f64 {
        let elapsed = self.started.elapsed().as_secs_f64();
        if elapsed <= 0.0 {
            0.0
        } else {
            self.frames as f64 / elapsed
        }
    }
}

/// Chooses the editor for a property from what its schema advertises: booleans
/// get a checkbox, enumerated values get a drop-down, ranged numbers get a
/// slider next to a box holding the exact value, everything else a text box.
fn property_row(
    device: &DeviceDescriptor,
    schema: &PropertySchema,
    value: Option<&Value>,
    table_start: bool,
) -> PropertyRow {
    let scale = slider_scale(schema);
    let canonical = value_as_f64(value);
    let control = if !schema.writable {
        "readonly"
    } else if schema.value_type == ValueType::Bool {
        "bool"
    } else if !schema.enum_values.is_empty() {
        "choice"
    } else if scale.is_some() && canonical.is_some() {
        "slider"
    } else {
        "text"
    };
    let text = match value {
        Some(value) => schema
            .enum_values
            .iter()
            .find(|candidate| &candidate.value == value)
            .map(|candidate| candidate.label.clone())
            .unwrap_or_else(|| format_value(value)),
        None => String::new(),
    };
    PropertyRow {
        device: device.label.as_str().into(),
        kinds: public_kind_summary(device).into(),
        key: schema.key.as_str().into(),
        display: if schema.display_name.is_empty() {
            schema.key.as_str().into()
        } else {
            schema.display_name.as_str().into()
        },
        value: text.into(),
        control: control.into(),
        checked: matches!(value, Some(Value::Bool(true))),
        options: ModelRc::new(VecModel::from(
            schema
                .enum_values
                .iter()
                .map(|candidate| SharedString::from(candidate.label.as_str()))
                .collect::<Vec<_>>(),
        )),
        slider_min: scale.map(|scale| scale.min as f32).unwrap_or(0.0),
        slider_max: scale.map(|scale| scale.max as f32).unwrap_or(1.0),
        slider_value: scale
            .zip(canonical)
            .map(|(scale, value)| scale.from_canonical(value) as f32)
            .unwrap_or_default(),
        table_start,
    }
}

/// Slider domain for a ranged numeric property. Ranges spanning two decades or
/// more (exposure times, stage speeds) are driven on a log scale; the rest
/// (percentages, stage coordinates) linearly.
#[derive(Clone, Copy)]
struct SliderScale {
    min: f64,
    max: f64,
    logarithmic: bool,
}

impl SliderScale {
    fn from_canonical(&self, value: f64) -> f64 {
        if self.logarithmic {
            value.max(f64::MIN_POSITIVE).log10()
        } else {
            value
        }
        .clamp(self.min, self.max)
    }

    fn to_canonical(&self, position: f64) -> f64 {
        let position = position.clamp(self.min, self.max);
        if self.logarithmic {
            10f64.powf(position)
        } else {
            position
        }
    }
}

/// Rounds a numeric value onto the step the schema advertises. The runtime
/// validates writes against `increment`, so a control that produces continuous
/// values has to land on a real step before the write is submitted.
fn snap_to_increment(schema: &PropertySchema, value: Value) -> Value {
    let (Some(increment), Some(current)) = (
        schema
            .increment
            .as_ref()
            .and_then(|increment| value_as_f64(Some(increment)))
            .filter(|increment| *increment > 0.0),
        value_as_f64(Some(&value)),
    ) else {
        return value;
    };
    let base = schema
        .range
        .as_ref()
        .and_then(|range| value_as_f64(Some(&range.min)))
        .unwrap_or(0.0);
    let snapped = base + ((current - base) / increment).round() * increment;
    parse_property_value(schema.value_type, &format!("{snapped}"))
}

fn slider_scale(schema: &PropertySchema) -> Option<SliderScale> {
    if schema.value_type == ValueType::Bool || !schema.enum_values.is_empty() {
        return None;
    }
    let range = schema.range.as_ref()?;
    let min = value_as_f64(Some(&range.min))?;
    let max = value_as_f64(Some(&range.max))?;
    if !(min.is_finite() && max.is_finite()) || max <= min {
        return None;
    }
    if min > 0.0 && max / min >= 100.0 {
        Some(SliderScale {
            min: min.log10(),
            max: max.log10(),
            logarithmic: true,
        })
    } else {
        Some(SliderScale {
            min,
            max,
            logarithmic: false,
        })
    }
}

/// Binning as a plain factor. Drivers publish it either as an `NxN` mode string
/// or as a bare count, so both are accepted.
fn binning_factor(value: Option<&Value>) -> u32 {
    match value {
        Some(Value::String(mode)) => mode
            .split(['x', 'X'])
            .next()
            .and_then(|factor| factor.parse().ok())
            .filter(|factor| *factor > 0)
            .unwrap_or(1),
        Some(Value::I64(factor)) if *factor > 0 => *factor as u32,
        Some(Value::PixelCount(factor)) if factor.pixels() > 0 => factor.pixels(),
        _ => 1,
    }
}

fn safety_detail(summary: &SafetySummary) -> String {
    let mut parts = Vec::new();
    if let Some(enabled) = summary.enabled {
        parts.push(format!("enabled={enabled}"));
    }
    if let Some(interlock_closed) = summary.interlock_closed {
        parts.push(format!("interlock={interlock_closed}"));
    }
    if let Some(emission_permitted) = summary.emission_permitted {
        parts.push(format!("permitted={emission_permitted}"));
    }
    if let Some(fault_active) = summary.fault_active {
        parts.push(format!("fault_active={fault_active}"));
    }
    if let Some(fault) = &summary.fault {
        parts.push(format!("fault={fault}"));
    }
    if parts.is_empty() {
        "no safety properties".into()
    } else {
        parts.join("  ")
    }
}

/// Same summary as [`safety_detail`], worded for the GUI panel rather than for
/// a terminal listing.
fn safety_readout(summary: &SafetySummary) -> String {
    let mut parts = Vec::new();
    if let Some(enabled) = summary.enabled {
        parts.push(format!(
            "emission {}",
            if enabled { "requested" } else { "off" }
        ));
    }
    if let Some(closed) = summary.interlock_closed {
        parts.push(format!(
            "interlock {}",
            if closed { "closed" } else { "OPEN" }
        ));
    }
    if let Some(permitted) = summary.emission_permitted {
        parts.push(format!(
            "emission {}",
            if permitted { "permitted" } else { "blocked" }
        ));
    }
    match (&summary.fault, summary.fault_active) {
        (Some(fault), Some(true)) => parts.push(format!("fault: {fault}")),
        (_, Some(true)) => parts.push("fault active".into()),
        _ => parts.push("no fault".into()),
    }
    parts.join(" · ")
}

fn parse_property_value(value_type: ValueType, text: &str) -> Value {
    if let Some(value) = parse_unit_value(value_type, text) {
        return value;
    }
    let number = numeric_prefix(text);
    match value_type {
        ValueType::Bool => Value::Bool(matches!(text, "true" | "1" | "yes" | "on")),
        ValueType::I64 => number
            .and_then(|value| value.parse().ok())
            .map(Value::I64)
            .unwrap_or(Value::I64(0)),
        ValueType::F64 => number
            .and_then(|value| value.parse().ok())
            .map(Value::F64)
            .unwrap_or(Value::F64(0.0)),
        ValueType::String => Value::String(text.into()),
        ValueType::Temperature => number
            .and_then(|value| value.parse().ok())
            .map(Temperature::from_celsius)
            .map(Value::Temperature)
            .unwrap_or(Value::Temperature(Temperature::from_celsius(0.0))),
        ValueType::Position => number
            .and_then(|value| value.parse().ok())
            .map(Position::from_micrometers)
            .map(Value::Position)
            .unwrap_or(Value::Position(Position::from_micrometers(0.0))),
        ValueType::Velocity => number
            .and_then(|value| value.parse().ok())
            .map(Velocity::from_micrometers_per_second)
            .map(Value::Velocity)
            .unwrap_or(Value::Velocity(Velocity::from_micrometers_per_second(0.0))),
        ValueType::Acceleration => number
            .and_then(|value| value.parse().ok())
            .map(Acceleration::from_micrometers_per_second_squared)
            .map(Value::Acceleration)
            .unwrap_or(Value::Acceleration(
                Acceleration::from_micrometers_per_second_squared(0.0),
            )),
        ValueType::TimeInterval => number
            .and_then(|value| value.parse().ok())
            .map(TimeInterval::from_seconds)
            .map(Value::TimeInterval)
            .unwrap_or(Value::TimeInterval(TimeInterval::from_seconds(0.0))),
        ValueType::Wavelength => number
            .and_then(|value| value.parse().ok())
            .map(Wavelength::from_nanometers)
            .map(Value::Wavelength)
            .unwrap_or(Value::Wavelength(Wavelength::from_nanometers(0.0))),
        ValueType::OpticalPower => number
            .and_then(|value| value.parse().ok())
            .map(OpticalPower::from_milliwatts)
            .map(Value::OpticalPower)
            .unwrap_or(Value::OpticalPower(OpticalPower::from_milliwatts(0.0))),
        ValueType::ElectricCurrent => number
            .and_then(|value| value.parse().ok())
            .map(ElectricCurrent::from_milliamps)
            .map(Value::ElectricCurrent)
            .unwrap_or(Value::ElectricCurrent(ElectricCurrent::from_milliamps(0.0))),
        ValueType::Voltage => number
            .and_then(|value| value.parse().ok())
            .map(Voltage::from_volts)
            .map(Value::Voltage)
            .unwrap_or(Value::Voltage(Voltage::from_volts(0.0))),
        ValueType::Frequency => number
            .and_then(|value| value.parse().ok())
            .map(Frequency::from_hertz)
            .map(Value::Frequency)
            .unwrap_or(Value::Frequency(Frequency::from_hertz(0.0))),
        ValueType::Decibel => number
            .and_then(|value| value.parse().ok())
            .map(Decibel::new)
            .map(Value::Decibel)
            .unwrap_or(Value::Decibel(Decibel::new(0.0))),
        ValueType::PixelCount => number
            .and_then(|value| value.parse().ok())
            .map(PixelCount::new)
            .map(Value::PixelCount)
            .unwrap_or(Value::PixelCount(PixelCount::new(0))),
        ValueType::ByteCount => number
            .and_then(|value| value.parse().ok())
            .map(ByteCount::new)
            .map(Value::ByteCount)
            .unwrap_or(Value::ByteCount(ByteCount::new(0))),
        ValueType::StepCount => number
            .and_then(|value| value.parse().ok())
            .map(StepCount::new)
            .map(Value::StepCount)
            .unwrap_or(Value::StepCount(StepCount::new(0))),
        ValueType::ControllerScalar => number
            .and_then(|value| value.parse().ok())
            .map(ControllerScalar::new)
            .map(Value::ControllerScalar)
            .unwrap_or(Value::ControllerScalar(ControllerScalar::new(0))),
        ValueType::Ratio => number
            .and_then(|value| value.parse().ok())
            .map(Ratio::from_percent)
            .map(Value::Ratio)
            .unwrap_or(Value::Ratio(Ratio::from_percent(0.0))),
        ValueType::NumericalAperture => number
            .and_then(|value| value.parse().ok())
            .map(NumericalAperture::new)
            .map(Value::NumericalAperture)
            .unwrap_or(Value::NumericalAperture(NumericalAperture::new(0.0))),
        ValueType::Timestamp => number
            .and_then(|value| value.parse().ok())
            .map(Timestamp::from_controller_ticks)
            .map(Value::Timestamp)
            .unwrap_or(Value::Timestamp(Timestamp::from_controller_ticks(0))),
        ValueType::Pressure => number
            .and_then(|value| value.parse().ok())
            .map(Pressure::from_millibar)
            .map(Value::Pressure)
            .unwrap_or(Value::Pressure(Pressure::from_millibar(0.0))),
        ValueType::GasConcentration => number
            .and_then(|value| value.parse().ok())
            .map(GasConcentration::from_percent)
            .map(Value::GasConcentration)
            .unwrap_or(Value::GasConcentration(GasConcentration::from_percent(0.0))),
        ValueType::FlowRate => number
            .and_then(|value| value.parse().ok())
            .map(FlowRate::from_milliliters_per_minute)
            .map(Value::FlowRate)
            .unwrap_or(Value::FlowRate(FlowRate::from_milliliters_per_minute(0.0))),
        ValueType::Bytes | ValueType::List | ValueType::Map => Value::String(text.into()),
        ValueType::Null => Value::Null,
    }
}

fn parse_unit_value(value_type: ValueType, text: &str) -> Option<Value> {
    let mut parts = text.split_whitespace();
    let number = parts.next()?.parse::<f64>().ok()?;
    let unit = parts.collect::<Vec<_>>().join(" ");
    if unit.is_empty() {
        return None;
    }
    match (value_type, unit.as_str()) {
        (ValueType::Temperature, "degC") => {
            Some(Value::Temperature(Temperature::from_celsius(number)))
        }
        (ValueType::Temperature, "K") => Some(Value::Temperature(Temperature::from_kelvin(number))),
        (ValueType::Temperature, "degF") => {
            Some(Value::Temperature(Temperature::from_fahrenheit(number)))
        }
        (ValueType::Position, "m") => Some(Value::Position(Position::from_meters(number))),
        (ValueType::Position, "mm") => Some(Value::Position(Position::from_millimeters(number))),
        (ValueType::Position, "um") => Some(Value::Position(Position::from_micrometers(number))),
        (ValueType::Velocity, "m/s") => {
            Some(Value::Velocity(Velocity::from_meters_per_second(number)))
        }
        (ValueType::Velocity, "mm/s") => Some(Value::Velocity(
            Velocity::from_millimeters_per_second(number),
        )),
        (ValueType::Velocity, "um/s") => Some(Value::Velocity(
            Velocity::from_micrometers_per_second(number),
        )),
        (ValueType::Acceleration, "m/s^2") => Some(Value::Acceleration(
            Acceleration::from_meters_per_second_squared(number),
        )),
        (ValueType::Acceleration, "mm/s^2") => Some(Value::Acceleration(
            Acceleration::from_millimeters_per_second_squared(number),
        )),
        (ValueType::Acceleration, "um/s^2") => Some(Value::Acceleration(
            Acceleration::from_micrometers_per_second_squared(number),
        )),
        (ValueType::TimeInterval, "h") => {
            Some(Value::TimeInterval(TimeInterval::from_hours(number)))
        }
        (ValueType::TimeInterval, "s") => {
            Some(Value::TimeInterval(TimeInterval::from_seconds(number)))
        }
        (ValueType::TimeInterval, "ms") => {
            Some(Value::TimeInterval(TimeInterval::from_milliseconds(number)))
        }
        (ValueType::TimeInterval, "us") => {
            Some(Value::TimeInterval(TimeInterval::from_microseconds(number)))
        }
        (ValueType::TimeInterval, "ns") => {
            Some(Value::TimeInterval(TimeInterval::from_nanoseconds(number)))
        }
        (ValueType::TimeInterval, "controller_tick") => Some(Value::TimeInterval(
            TimeInterval::from_controller_ticks(number),
        )),
        (ValueType::Wavelength, "nm") => {
            Some(Value::Wavelength(Wavelength::from_nanometers(number)))
        }
        (ValueType::Wavelength, "angstrom") => {
            Some(Value::Wavelength(Wavelength::from_nanometers(number * 0.1)))
        }
        (ValueType::OpticalPower, "W") => {
            Some(Value::OpticalPower(OpticalPower::from_watts(number)))
        }
        (ValueType::OpticalPower, "mW") => {
            Some(Value::OpticalPower(OpticalPower::from_milliwatts(number)))
        }
        (ValueType::OpticalPower, "uW") => {
            Some(Value::OpticalPower(OpticalPower::from_microwatts(number)))
        }
        (ValueType::ElectricCurrent, "A") => {
            Some(Value::ElectricCurrent(ElectricCurrent::from_amps(number)))
        }
        (ValueType::ElectricCurrent, "mA") => Some(Value::ElectricCurrent(
            ElectricCurrent::from_milliamps(number),
        )),
        (ValueType::ElectricCurrent, "uA") => Some(Value::ElectricCurrent(
            ElectricCurrent::from_microamps(number),
        )),
        (ValueType::Voltage, "V") => Some(Value::Voltage(Voltage::from_volts(number))),
        (ValueType::Voltage, "mV") => Some(Value::Voltage(Voltage::from_millivolts(number))),
        (ValueType::Voltage, "uV") => Some(Value::Voltage(Voltage::from_microvolts(number))),
        (ValueType::Frequency, "Hz") => Some(Value::Frequency(Frequency::from_hertz(number))),
        (ValueType::Frequency, "kHz") => Some(Value::Frequency(Frequency::from_kilohertz(number))),
        (ValueType::Frequency, "MHz") => Some(Value::Frequency(Frequency::from_megahertz(number))),
        (ValueType::Decibel, "dB") => Some(Value::Decibel(Decibel::new(number))),
        (ValueType::PixelCount, "px") => Some(Value::PixelCount(PixelCount::new(
            number.round().max(0.0) as u32,
        ))),
        (ValueType::ByteCount, "bytes") => Some(Value::ByteCount(ByteCount::new(
            number.round().max(0.0) as u64,
        ))),
        (ValueType::StepCount, "steps") => {
            Some(Value::StepCount(StepCount::new(number.round() as i64)))
        }
        (ValueType::ControllerScalar, "controller_step") => Some(Value::ControllerScalar(
            ControllerScalar::new(number.round() as i64),
        )),
        (ValueType::Ratio, "percent" | "%") => Some(Value::Ratio(Ratio::from_percent(number))),
        (ValueType::Ratio, "fraction") => Some(Value::Ratio(Ratio::from_fraction(number))),
        (ValueType::Timestamp, "controller_tick") => Some(Value::Timestamp(
            Timestamp::from_controller_ticks(number.round() as i64),
        )),
        (ValueType::Pressure, "Pa") => Some(Value::Pressure(Pressure::from_pascals(number))),
        (ValueType::Pressure, "kPa") => Some(Value::Pressure(Pressure::from_kilopascals(number))),
        (ValueType::Pressure, "bar") => Some(Value::Pressure(Pressure::from_bar(number))),
        (ValueType::Pressure, "mbar") => Some(Value::Pressure(Pressure::from_millibar(number))),
        (ValueType::Pressure, "psi") => Some(Value::Pressure(Pressure::from_psi(number))),
        (ValueType::GasConcentration, "percent" | "%") => Some(Value::GasConcentration(
            GasConcentration::from_percent(number),
        )),
        (ValueType::GasConcentration, "ppm") => {
            Some(Value::GasConcentration(GasConcentration::from_ppm(number)))
        }
        (ValueType::GasConcentration, "fraction") => Some(Value::GasConcentration(
            GasConcentration::from_fraction(number),
        )),
        (ValueType::FlowRate, "L/min") => {
            Some(Value::FlowRate(FlowRate::from_liters_per_minute(number)))
        }
        (ValueType::FlowRate, "mL/min") => Some(Value::FlowRate(
            FlowRate::from_milliliters_per_minute(number),
        )),
        (ValueType::FlowRate, "uL/min") => Some(Value::FlowRate(
            FlowRate::from_microliters_per_minute(number),
        )),
        _ => None,
    }
}

fn numeric_prefix(text: &str) -> Option<&str> {
    text.trim()
        .split(|ch: char| ch.is_whitespace() || ch == ',')
        .next()
        .filter(|value| !value.is_empty())
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Bool(v) => v.to_string(),
        Value::I64(v) => v.to_string(),
        Value::F64(v) => format_scalar(*v),
        Value::Temperature(v) => {
            format!("{} {}", format_scalar(v.value), unit_label(v.unit_symbol()))
        }
        Value::Position(v) => format!("{} {}", format_scalar(v.value), unit_label(v.unit_symbol())),
        Value::Velocity(v) => format!("{} {}", format_scalar(v.value), unit_label(v.unit_symbol())),
        Value::Acceleration(v) => {
            format!("{} {}", format_scalar(v.value), unit_label(v.unit_symbol()))
        }
        Value::TimeInterval(v) => {
            format!("{} {}", format_scalar(v.value), unit_label(v.unit_symbol()))
        }
        Value::Wavelength(v) => {
            format!("{} {}", format_scalar(v.value), unit_label(v.unit_symbol()))
        }
        Value::OpticalPower(v) => {
            format!("{} {}", format_scalar(v.value), unit_label(v.unit_symbol()))
        }
        Value::ElectricCurrent(v) => {
            format!("{} {}", format_scalar(v.value), unit_label(v.unit_symbol()))
        }
        Value::Voltage(v) => format!("{} {}", format_scalar(v.value), unit_label(v.unit_symbol())),
        Value::Frequency(v) => {
            format!("{} {}", format_scalar(v.value), unit_label(v.unit_symbol()))
        }
        Value::Decibel(v) => format!("{} {}", format_scalar(v.value), unit_label(v.unit_symbol())),
        Value::PixelCount(v) => format!("{} px", v.pixels()),
        Value::ByteCount(v) => format!("{} {}", v.bytes(), unit_label(v.unit_symbol())),
        Value::StepCount(v) => format!("{} {}", v.steps(), unit_label(v.unit_symbol())),
        Value::ControllerScalar(v) => format!("{} {}", v.value(), unit_label(v.unit_symbol())),
        Value::Ratio(v) => format!("{} {}", format_scalar(v.value), unit_label(v.unit_symbol())),
        Value::NumericalAperture(v) => format_scalar(v.value()),
        Value::Timestamp(v) => format!("{} {}", v.ticks(), unit_label(v.unit_symbol())),
        Value::Pressure(v) => format!("{} {}", format_scalar(v.value), unit_label(v.unit_symbol())),
        Value::GasConcentration(v) => {
            format!("{} {}", format_scalar(v.value), unit_label(v.unit_symbol()))
        }
        Value::FlowRate(v) => format!("{} {}", format_scalar(v.value), unit_label(v.unit_symbol())),
        Value::String(v) => v.clone(),
        Value::Bytes(v) => format!("{} bytes", v.len()),
        Value::List(v) => format!("{} items", v.len()),
        Value::Map(v) => format!("{} fields", v.len()),
        Value::Null => String::new(),
    }
}

/// Compact fixed-point rendering: sliders produce values with far more digits
/// than a readout needs, and trailing zeros only add noise.
/// Units are rendered as their conventional symbol where one exists.
fn unit_label(symbol: &str) -> &str {
    match symbol {
        "percent" => "%",
        other => other,
    }
}

fn format_scalar(value: f64) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    if value == value.trunc() && value.abs() < 1e15 {
        return format!("{}", value as i64);
    }
    let text = format!("{value:.6}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn value_as_f64(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::F64(v)) => Some(*v),
        Some(Value::I64(v)) => Some(*v as f64),
        Some(Value::Position(v)) => Some(v.micrometers()),
        Some(Value::Velocity(v)) => Some(v.micrometers_per_second()),
        Some(Value::Acceleration(v)) => Some(v.micrometers_per_second_squared()),
        Some(Value::TimeInterval(v)) => Some(v.seconds()),
        Some(Value::Wavelength(v)) => Some(v.nanometers()),
        Some(Value::OpticalPower(v)) => Some(v.milliwatts()),
        Some(Value::ElectricCurrent(v)) => Some(v.milliamps()),
        Some(Value::Voltage(v)) => Some(v.volts()),
        Some(Value::Frequency(v)) => Some(v.hertz()),
        Some(Value::PixelCount(v)) => Some(v.pixels() as f64),
        Some(Value::ByteCount(v)) => Some(v.bytes() as f64),
        Some(Value::StepCount(v)) => Some(v.steps() as f64),
        Some(Value::ControllerScalar(v)) => Some(v.value() as f64),
        Some(Value::Ratio(v)) => Some(v.percent()),
        Some(Value::NumericalAperture(v)) => Some(v.value()),
        Some(Value::Timestamp(v)) => Some(v.ticks() as f64),
        Some(Value::Pressure(v)) => Some(v.millibar()),
        Some(Value::GasConcentration(v)) => Some(v.percent()),
        Some(Value::FlowRate(v)) => Some(v.milliliters_per_minute()),
        _ => None,
    }
}

fn frame_smoke_summary(frame: Option<&Frame>) -> String {
    match frame {
        Some(frame) => format!(
            "frame={:?} {}x{} {} histogram_bins={}",
            frame.handle,
            frame.width,
            frame.height,
            frame.pixel_format,
            histogram(frame).len()
        ),
        None => "frame=none".into(),
    }
}

/// One bin per Mono8 code, lightly smoothed and square-root scaled so the
/// background peak does not flatten everything else.
/// Detector samples arrive as counts or volts depending on the channel; both
/// are mapped onto the same 16-bit code the raster reconstruction uses.
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

fn histogram(frame: &Frame) -> Vec<f32> {
    let mut counts = [0u32; HISTOGRAM_BINS];
    for value in &frame.data {
        counts[*value as usize] += 1;
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

/// Grey rendering of a Mono8 frame, with pixels at or above `saturation` drawn
/// red so clipped regions stand out.
fn image_from_mono8(frame: &Frame, saturation: Option<u8>) -> Image {
    let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(frame.width, frame.height);
    for (pixel, value) in pixels.make_mut_bytes().chunks_exact_mut(3).zip(&frame.data) {
        if saturation.is_some_and(|threshold| *value >= threshold) {
            pixel[0] = 255;
            pixel[1] = 32;
            pixel[2] = 32;
        } else {
            pixel[0] = *value;
            pixel[1] = *value;
            pixel[2] = *value;
        }
    }
    Image::from_rgb8(pixels)
}
