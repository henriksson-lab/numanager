use numanager_core::runtime::{LocalRuntime, Runtime};
use numanager_core::*;
use numanager_drivers::hamilton_mvp::HamiltonMvpDriver;
use numanager_examples::{
    capability_brief, completion_summary, device_by_kind,
    driver_capability_by_kind as capability_by_kind, event_summary, is_public_property,
    public_kind_summary,
};
use std::time::Duration;

pub fn run() -> numanager_core::Result<()> {
    let driver = numanager_examples::driver_value(|id| {
        HamiltonMvpDriver::configured(
            id,
            numanager_drivers::hamilton_mvp::HamiltonMvpConfiguredProbe::fixture(),
        )
    });
    let devices = driver.descriptors();
    let controller = device_by_kind(&devices, "fluidics.controller")?;
    let valve = device_by_kind(&devices, "fluidics.valve")?;
    let valve_select = capability_by_kind(&driver, valve, CapabilityKind::ValveSelect)?;

    println!(
        "selected fluidics controller: {} [{}]",
        controller.label,
        public_kind_summary(controller)
    );
    println!(
        "selected valve: {} [{}]",
        valve.label,
        public_kind_summary(valve)
    );
    println!("capabilities: valve={}", capability_brief(&valve_select));
    for property in valve
        .properties
        .iter()
        .filter(|property| is_public_property(property))
    {
        println!(
            "valve property: {} type={:?} writable={} sequenceable={}",
            property.key, property.value_type, property.writable, property.sequenceable
        );
    }

    let runtime = LocalRuntime::from_drivers(vec![Box::new(driver)]);
    let runtime_devices = runtime.devices().into_iter().cloned().collect::<Vec<_>>();
    let events = runtime.subscribe(
        EventFilter::devices([controller, valve])
            .with_kinds([EventKind::OperationChanged, EventKind::PropertyChanged]),
    );

    let selected = runtime.submit_request(
        valve,
        ValveSelectRequest::position(3).with_direction(ValveDirection::Clockwise),
    )?;
    let value = runtime.wait_completed(selected.id, Duration::from_secs(1))?;
    println!("valve select completed: {}", completion_summary(&value));

    let moved_by_property = runtime.submit(
        StateSet::immediate("fluidics valve route")
            .with_write(valve, "position", Value::I64(5))
            .into_command(),
    )?;
    let value = runtime.wait_completed(moved_by_property.id, Duration::from_secs(1))?;
    println!("valve state set completed: {}", completion_summary(&value));

    for key in [
        "position",
        "port_count",
        "initialized",
        "busy",
        "valve_error",
        "state_summary",
    ] {
        let value = runtime.execute(Command::read_property(valve, key), Duration::from_secs(1))?;
        println!("{key}: {}", completion_summary(&value));
    }
    if controller
        .properties
        .iter()
        .any(|property| property.key == "last_transaction")
    {
        let value = runtime.execute(
            Command::read_property(controller, "last_transaction"),
            Duration::from_secs(1),
        )?;
        println!(
            "controller last_transaction: {}",
            completion_summary(&value)
        );
    }

    while let Some(event) = events.recv_timeout(Duration::from_millis(50)) {
        println!("event: {}", event_summary(&runtime_devices, &event));
    }

    Ok(())
}
