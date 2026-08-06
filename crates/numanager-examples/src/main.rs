mod autofocus;
mod biology_simulation;
mod camera_acquisition;
mod camera_stream;
mod config_roundtrip;
mod daqmx_runtime_probe;
mod digital_io;
mod discover_devices;
mod environment_control;
mod filters;
mod fluidics;
mod gel_doc;
mod laser;
mod light_source;
mod lsm_common;
mod lsm_composed_workflow;
mod lsm_confocal_capture;
mod lsm_confocal_capture_mono8;
mod lsm_confocal_stream;
mod lsm_daqmx_bringup_plan;
mod lsm_daqmx_commands;
mod lsm_daqmx_plan_validation;
mod lsm_daqmx_validation_note;
mod lsm_line_dwell_timing;
mod lsm_live_cancel;
mod lsm_signal_cancel;
mod lsm_signal_stream;
mod motion_stage;
mod plate_reader;
mod robot_inventory;
mod shutter;
#[cfg(feature = "gui")]
mod software_gui;
mod spark_cyto;
mod squid;
mod timing_plan;
mod usb_access;

const EXAMPLES: &[&str] = &[
    "autofocus",
    "biology_simulation",
    "camera_acquisition",
    "camera_stream",
    "config_roundtrip",
    "daqmx_runtime_probe",
    "digital_io",
    "discover_devices",
    "environment_control",
    "filters",
    "fluidics",
    "gel_doc",
    "laser",
    "light_source",
    "lsm_composed_workflow",
    "lsm_confocal_capture",
    "lsm_confocal_capture_mono8",
    "lsm_confocal_stream",
    "lsm_daqmx_bringup_plan",
    "lsm_daqmx_plan_validation",
    "lsm_daqmx_validation_note",
    "lsm_line_dwell_timing",
    "lsm_live_cancel",
    "lsm_signal_cancel",
    "lsm_signal_stream",
    "motion_stage",
    "plate_reader",
    "robot_inventory",
    "shutter",
    "software_gui",
    "spark_cyto",
    "squid",
    "timing_plan",
    "usb_access",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(example) = std::env::args().nth(1) else {
        print_usage();
        std::process::exit(2);
    };

    match example.as_str() {
        "autofocus" => autofocus::run()?,
        "biology_simulation" => biology_simulation::run()?,
        "camera_acquisition" => camera_acquisition::run()?,
        "camera_stream" => camera_stream::run()?,
        "config_roundtrip" => config_roundtrip::run()?,
        "daqmx_runtime_probe" => daqmx_runtime_probe::run()?,
        "digital_io" => digital_io::run()?,
        "discover_devices" => discover_devices::run()?,
        "environment_control" => environment_control::run()?,
        "filters" => filters::run()?,
        "fluidics" => fluidics::run()?,
        "gel_doc" => gel_doc::run()?,
        "laser" => laser::run()?,
        "light_source" => light_source::run()?,
        "lsm_composed_workflow" => lsm_composed_workflow::run()?,
        "lsm_confocal_capture" => lsm_confocal_capture::run()?,
        "lsm_confocal_capture_mono8" => lsm_confocal_capture_mono8::run()?,
        "lsm_confocal_stream" => lsm_confocal_stream::run()?,
        "lsm_daqmx_bringup_plan" => lsm_daqmx_bringup_plan::run()?,
        "lsm_daqmx_plan_validation" => lsm_daqmx_plan_validation::run()?,
        "lsm_daqmx_validation_note" => lsm_daqmx_validation_note::run()?,
        "lsm_line_dwell_timing" => lsm_line_dwell_timing::run()?,
        "lsm_live_cancel" => lsm_live_cancel::run()?,
        "lsm_signal_cancel" => lsm_signal_cancel::run()?,
        "lsm_signal_stream" => lsm_signal_stream::run()?,
        "motion_stage" => motion_stage::run()?,
        "plate_reader" => plate_reader::run()?,
        "robot_inventory" => robot_inventory::run()?,
        "shutter" => shutter::run()?,
        "software_gui" => run_software_gui()?,
        "spark_cyto" => spark_cyto::run()?,
        "squid" => squid::run()?,
        "timing_plan" => timing_plan::run()?,
        "usb_access" => usb_access::run()?,
        _ => {
            eprintln!("unknown example: {example}");
            print_usage();
            std::process::exit(2);
        }
    }

    Ok(())
}

#[cfg(feature = "gui")]
fn run_software_gui() -> Result<(), Box<dyn std::error::Error>> {
    software_gui::run()?;
    Ok(())
}

#[cfg(not(feature = "gui"))]
fn run_software_gui() -> Result<(), Box<dyn std::error::Error>> {
    Err("software_gui requires --features gui".into())
}

fn print_usage() {
    eprintln!("usage: cargo run -p numanager-examples -- <example> [args]");
    eprintln!("examples: {}", EXAMPLES.join(", "));
}
