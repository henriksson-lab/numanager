use numanager_core::runtime::{LocalRuntime, Runtime};
use numanager_core::*;
use numanager_drivers::esp32::Esp32Driver;
use numanager_drivers::evident_ix85::{Ix85ConfiguredProbe, Ix85Driver};
use numanager_drivers::thorlabs_sc10::Sc10Driver;
use numanager_examples::{
    capability_brief, completion_summary, device_by_kind, driver_capability_by_kind, event_summary,
    is_public_property, property, public_kind_summary,
};
use std::time::Duration;

pub fn run() -> numanager_core::Result<()> {
    let source = numanager_examples::example_arg(0).unwrap_or_else(|| "sc10".into());
    let driver = numanager_examples::boxed_driver(|id| shutter_driver(&source, id))?;
    let devices = driver.descriptors();
    let shutter = device_by_kind(&devices, "shutter")?;
    let trigger = driver_capability_by_kind(&*driver, shutter, CapabilityKind::TriggerSink)?;

    println!("selected shutter family: {source}");
    println!(
        "selected shutter: {} [{}]",
        shutter.label,
        public_kind_summary(shutter)
    );
    println!("capabilities: shutter={}", capability_brief(&trigger));
    for schema in shutter
        .properties
        .iter()
        .filter(|schema| is_public_property(schema))
    {
        println!(
            "shutter property: {} type={:?} writable={} sequenceable={}",
            schema.key, schema.value_type, schema.writable, schema.sequenceable
        );
    }

    let runtime = LocalRuntime::from_drivers(vec![driver]);
    let runtime_devices = runtime.devices().into_iter().cloned().collect::<Vec<_>>();
    let safety = runtime.safety_summary(shutter, Duration::from_secs(1))?;
    println!(
        "shutter safety: {} {}",
        safety.state.name(),
        completion_summary(&safety.as_value())
    );
    let events = runtime.subscribe(
        EventFilter::devices([shutter])
            .with_kinds([EventKind::OperationChanged, EventKind::PropertyChanged]),
    );

    let mut setup = StateSet::immediate("shutter setup");
    let mut setup_writes = 0;
    if property(shutter, "mode").is_some() {
        setup = setup.with_write(shutter, "mode", Value::String("Manual".into()));
        setup_writes += 1;
    }
    if property(shutter, "open_time").is_some() {
        setup = setup.with_write(
            shutter,
            "open_time",
            Value::TimeInterval(TimeInterval::from_milliseconds(15.0)),
        );
        setup_writes += 1;
    }
    if property(shutter, "close_time").is_some() {
        setup = setup.with_write(
            shutter,
            "close_time",
            Value::TimeInterval(TimeInterval::from_milliseconds(15.0)),
        );
        setup_writes += 1;
    }
    if property(shutter, "trigger_mode").is_some() {
        setup = setup.with_write(shutter, "trigger_mode", Value::String("Internal".into()));
        setup_writes += 1;
    }
    if property(shutter, "repeat_count").is_some() {
        setup = setup.with_write(shutter, "repeat_count", Value::I64(1));
        setup_writes += 1;
    }
    if setup_writes > 0 {
        let setup = runtime.submit(setup.into_command())?;
        let value = runtime.wait_completed(setup.id, Duration::from_secs(1))?;
        println!("shutter setup completed: {}", completion_summary(&value));
    } else {
        println!("shutter setup skipped: no generic setup properties");
    }

    let opened = runtime.submit_capability(
        shutter,
        CapabilityKind::TriggerSink,
        CapabilityRequest::Trigger(TriggerRequest::enable()),
    )?;
    let value = runtime.wait_completed(opened.id, Duration::from_secs(1))?;
    println!("shutter open completed: {}", completion_summary(&value));

    let pulsed = runtime.submit_capability(
        shutter,
        CapabilityKind::TriggerSink,
        CapabilityRequest::None,
    )?;
    let value = runtime.wait_completed(pulsed.id, Duration::from_secs(1))?;
    println!("shutter pulse completed: {}", completion_summary(&value));

    let closed = runtime.submit_capability(
        shutter,
        CapabilityKind::TriggerSink,
        CapabilityRequest::Trigger(TriggerRequest::disable()),
    )?;
    let value = runtime.wait_completed(closed.id, Duration::from_secs(1))?;
    println!("shutter close completed: {}", completion_summary(&value));

    let mut read_keys = Vec::new();
    for key in [
        shutter_open_key(shutter),
        "enabled",
        "open",
        "mode",
        "open_time",
        "close_time",
        "trigger_mode",
        "repeat_count",
        "interlock_closed",
        "fault",
        "state_summary",
    ] {
        if !read_keys.contains(&key) {
            read_keys.push(key);
        }
    }
    for key in read_keys {
        if property(shutter, key).is_none() {
            continue;
        }
        let read = runtime.submit(Command::read_property(shutter.id, key))?;
        let value = runtime.wait_completed(read.id, Duration::from_secs(1))?;
        println!("{key}: {}", completion_summary(&value));
    }

    while let Some(event) = events.recv_timeout(Duration::from_millis(50)) {
        println!("event: {}", event_summary(&runtime_devices, &event));
    }

    Ok(())
}

fn shutter_driver(source: &str, id: DriverId) -> numanager_core::Result<Box<dyn Driver>> {
    match source {
        "sc10" => Ok(Box::new(Sc10Driver::configured_fixture(id))),
        "esp32" => Ok(Box::new(Esp32Driver::simulated(id))),
        "ix85" | "evident-ix85" | "evident_ix85" | "olympus-ix85" | "olympus_ix85" => Ok(Box::new(
            Ix85Driver::configured(id, Ix85ConfiguredProbe::fixture()),
        )),
        other => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("unknown shutter family {other}; expected sc10, esp32, or ix85"),
        )),
    }
}

fn shutter_open_key(shutter: &DeviceDescriptor) -> &'static str {
    if property(shutter, "enabled").is_some() {
        "enabled"
    } else if property(shutter, "open").is_some() {
        "open"
    } else if property(shutter, "dia_shutter_open").is_some() {
        "dia_shutter_open"
    } else if property(shutter, "epi_shutter_1_open").is_some() {
        "epi_shutter_1_open"
    } else {
        "open"
    }
}
