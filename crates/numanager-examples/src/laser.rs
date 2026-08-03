use numanager_core::runtime::{LocalRuntime, Runtime};
use numanager_core::*;
use numanager_drivers::cobolt::CoboltDriver;
use numanager_drivers::coherent_obis::ObisDriver;
use numanager_drivers::omicron::OmicronDriver;
use numanager_examples::{
    capability_brief, completion_summary, device_by_kind,
    driver_capability_by_kind as capability_by_kind, event_summary, is_public_property, property,
    public_kind_summary,
};
use std::time::Duration;

pub fn run() -> numanager_core::Result<()> {
    let source = numanager_examples::example_arg(0).unwrap_or_else(|| "cobolt".into());
    let driver = numanager_examples::boxed_driver(|id| laser_driver(&source, id))?;
    let devices = driver.descriptors();
    let laser = device_by_kind(&devices, "laser")?;
    let dac = capability_by_kind(&*driver, laser, CapabilityKind::Dac)?;
    let trigger = capability_by_kind(&*driver, laser, CapabilityKind::TriggerSink)?;

    println!("selected laser family: {source}");
    println!(
        "selected laser: {} [{}]",
        laser.label,
        public_kind_summary(laser)
    );
    println!(
        "capabilities: dac={} trigger={}",
        capability_brief(&dac),
        capability_brief(&trigger)
    );

    for property in laser
        .properties
        .iter()
        .filter(|property| is_public_property(property))
        .filter(|property| user_facing_laser_property(&property.key))
    {
        println!(
            "laser property: {} type={:?} writable={} sequenceable={}",
            property.key, property.value_type, property.writable, property.sequenceable
        );
    }

    let runtime = LocalRuntime::from_drivers(vec![driver]);
    let runtime_devices = runtime.devices().into_iter().cloned().collect::<Vec<_>>();
    let safety = runtime.safety_summary(laser, Duration::from_secs(1))?;
    println!(
        "laser safety: {} {}",
        safety.state.name(),
        completion_summary(&safety.as_value())
    );

    let events = runtime.subscribe(
        EventFilter::devices([laser])
            .with_kinds([EventKind::OperationChanged, EventKind::PropertyChanged]),
    );

    let output = runtime.submit_request(
        laser,
        DacRequest {
            value: Value::OpticalPower(OpticalPower::from_milliwatts(5.0)),
        },
    )?;
    let value = runtime.wait_completed(output.id, Duration::from_secs(1))?;
    println!(
        "laser output request completed: {}",
        completion_summary(&value)
    );

    let enable = runtime.submit_capability(
        laser,
        CapabilityKind::TriggerSink,
        CapabilityRequest::Trigger(TriggerRequest::enable()),
    )?;
    let value = runtime.wait_completed(enable.id, Duration::from_secs(1))?;
    println!("laser enable completed: {}", completion_summary(&value));

    let disable = runtime.submit_capability(
        laser,
        CapabilityKind::TriggerSink,
        CapabilityRequest::Trigger(TriggerRequest::disable()),
    )?;
    let value = runtime.wait_completed(disable.id, Duration::from_secs(1))?;
    println!("laser disable completed: {}", completion_summary(&value));

    for key in [
        "enabled",
        "power",
        "relative_power",
        "actual_power",
        "current",
        "actual_current",
        "wavelength",
        "interlock_closed",
        "fault",
        "analog_modulation_enabled",
        "digital_modulation_enabled",
    ]
    .into_iter()
    .filter(|key| property(laser, key).is_some())
    {
        let value = runtime.execute(Command::read_property(laser, key), Duration::from_secs(1))?;
        println!("{key}: {}", completion_summary(&value));
    }

    while let Some(event) = events.recv_timeout(Duration::from_millis(50)) {
        println!("event: {}", event_summary(&runtime_devices, &event));
    }

    Ok(())
}

fn laser_driver(source: &str, id: DriverId) -> numanager_core::Result<Box<dyn Driver>> {
    match source {
        "cobolt" | "hubner" => Ok(Box::new(CoboltDriver::simulated(id))),
        "obis" | "coherent-obis" | "coherent_obis" => Ok(Box::new(ObisDriver::simulated(id))),
        "omicron" => Ok(Box::new(OmicronDriver::simulated(id))),
        other => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("unknown laser source {other}; use one of: cobolt, obis, omicron"),
        )),
    }
}

fn user_facing_laser_property(key: &str) -> bool {
    matches!(
        key,
        "enabled"
            | "power"
            | "relative_power"
            | "actual_power"
            | "current"
            | "actual_current"
            | "wavelength"
            | "interlock_closed"
            | "fault"
            | "analog_modulation_enabled"
            | "digital_modulation_enabled"
            | "hours"
            | "head_hours"
            | "diode_temperature"
            | "baseplate_temperature"
    )
}
