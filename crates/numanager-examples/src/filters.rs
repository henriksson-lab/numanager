use numanager_core::runtime::{LocalRuntime, Runtime};
use numanager_core::*;
use numanager_drivers::evident_ix85::{Ix85ConfiguredProbe, Ix85Driver};
use numanager_drivers::prior::PriorDriver;
use numanager_drivers::starlight_xpress::{SxFilterWheelConfiguredProbe, SxFilterWheelDriver};
use numanager_drivers::thorlabs_kurios::KuriosDriver;
use numanager_examples::{
    capability_brief, completion_summary, device_by_kind,
    driver_capability_by_kind as capability_by_kind, event_summary, is_public_property, property,
    public_kind_summary,
};
use std::time::Duration;

pub fn run() -> numanager_core::Result<()> {
    let source = numanager_examples::example_arg(0).unwrap_or_else(|| "starlight".into());
    let driver = numanager_examples::boxed_driver(|id| filter_driver(&source, id))?;
    let devices = driver.descriptors();
    if let Ok(filter) = device_by_kind(&devices, "filter.tunable") {
        return run_tunable_filter(source, driver, filter.clone());
    }
    let selector = device_by_kind(&devices, "filter.wheel")
        .or_else(|_| device_with_filter_select(&*driver, &devices))?
        .clone();
    run_filter_wheel(source, driver, selector)
}

fn run_filter_wheel(
    source: String,
    driver: Box<dyn Driver>,
    wheel: DeviceDescriptor,
) -> numanager_core::Result<()> {
    let filter_select = capability_by_kind(&*driver, wheel.id, CapabilityKind::FilterSelect)?;
    let position_key = filter_position_key(&wheel)?;
    println!("selected filter family: {source}");
    if wheel.has_kind("filter.wheel") {
        println!(
            "selected filter wheel: {} [{}]",
            wheel.label,
            public_kind_summary(&wheel)
        );
    } else {
        println!(
            "selected filter selector: {} [{}]",
            wheel.label,
            public_kind_summary(&wheel)
        );
    }
    println!("capabilities: wheel={}", capability_brief(&filter_select));
    for property in wheel
        .properties
        .iter()
        .filter(|property| is_public_property(property))
    {
        println!(
            "filter property: {} type={:?} writable={} sequenceable={}",
            property.key, property.value_type, property.writable, property.sequenceable
        );
    }

    let runtime = LocalRuntime::from_drivers(vec![driver]);
    let runtime_devices = runtime.devices().into_iter().cloned().collect::<Vec<_>>();
    let events = runtime.subscribe(EventFilter::device(wheel.id).with_kinds([
        EventKind::OperationChanged,
        EventKind::PropertyChanged,
        EventKind::Log,
    ]));

    let selected = runtime.submit_request(wheel.id, FilterSelectRequest::position(3))?;
    let value = runtime.wait_completed(selected.id, Duration::from_secs(1))?;
    println!("filter select completed: {}", completion_summary(&value));

    let moved_by_property = runtime.submit(
        StateSet::immediate("filter wheel route")
            .with_write(wheel.id, position_key, Value::I64(2))
            .into_command(),
    )?;
    let value = runtime.wait_completed(moved_by_property.id, Duration::from_secs(1))?;
    println!("filter state set completed: {}", completion_summary(&value));

    let mut read_keys = vec![
        position_key,
        "position",
        "positions",
        "moving",
        "last_transaction",
    ];
    read_keys.dedup();
    for key in read_keys {
        if wheel.properties.iter().any(|property| property.key == key) {
            let value = runtime.execute(
                Command::read_property(wheel.id, key),
                Duration::from_secs(1),
            )?;
            println!("{key}: {}", completion_summary(&value));
        }
    }

    while let Some(event) = events.recv_timeout(Duration::from_millis(50)) {
        println!("event: {}", event_summary(&runtime_devices, &event));
    }

    Ok(())
}

fn run_tunable_filter(
    source: String,
    driver: Box<dyn Driver>,
    filter: DeviceDescriptor,
) -> numanager_core::Result<()> {
    let trigger = capability_by_kind(&*driver, filter.id, CapabilityKind::TriggerSink)?;
    println!("selected filter family: {source}");
    println!(
        "selected tunable filter: {} [{}]",
        filter.label,
        public_kind_summary(&filter)
    );
    println!("capabilities: tunable={}", capability_brief(&trigger));
    for property in filter
        .properties
        .iter()
        .filter(|property| is_public_property(property))
    {
        println!(
            "filter property: {} type={:?} writable={} sequenceable={}",
            property.key, property.value_type, property.writable, property.sequenceable
        );
    }

    let runtime = LocalRuntime::from_drivers(vec![driver]);
    let runtime_devices = runtime.devices().into_iter().cloned().collect::<Vec<_>>();
    let events = runtime.subscribe(EventFilter::device(filter.id).with_kinds([
        EventKind::OperationChanged,
        EventKind::PropertyChanged,
        EventKind::Log,
    ]));

    let tuned = runtime.submit(
        StateSet::immediate("tunable filter setup")
            .with_write(
                filter.id,
                "wavelength",
                Value::Wavelength(Wavelength::from_nanometers(520.0)),
            )
            .with_write(
                filter.id,
                "bandwidth",
                Value::Wavelength(Wavelength::from_nanometers(20.0)),
            )
            .with_write(filter.id, "output_enabled", Value::Bool(true))
            .into_command(),
    )?;
    let value = runtime.wait_completed(tuned.id, Duration::from_secs(1))?;
    println!(
        "tunable filter state set completed: {}",
        completion_summary(&value)
    );

    let disabled = runtime.submit_capability(
        filter.id,
        CapabilityKind::TriggerSink,
        CapabilityRequest::Trigger(TriggerRequest::disable()),
    )?;
    let value = runtime.wait_completed(disabled.id, Duration::from_secs(1))?;
    println!(
        "tunable filter disable completed: {}",
        completion_summary(&value)
    );

    let armed = runtime.submit(
        TimingPlan::builder()
            .sequence(
                filter.id,
                "wavelength",
                [
                    Value::Wavelength(Wavelength::from_nanometers(500.0)),
                    Value::Wavelength(Wavelength::from_nanometers(540.0)),
                    Value::Wavelength(Wavelength::from_nanometers(520.0)),
                ],
            )
            .sequence(
                filter.id,
                "output_enabled",
                [Value::Bool(true), Value::Bool(true), Value::Bool(false)],
            )
            .arm_order([filter.id])
            .stop(StopCondition::Count(3))
            .into_command()?,
    )?;
    let value = runtime.wait_completed(armed.id, Duration::from_secs(1))?;
    println!(
        "tunable filter timing arm completed: {}",
        completion_summary(&value)
    );
    let started = runtime.submit(Command::start(armed.id))?;
    let value = runtime.wait_completed(started.id, Duration::from_secs(1))?;
    println!(
        "tunable filter timing start completed: {}",
        completion_summary(&value)
    );
    let stopped = runtime.submit(Command::stop(armed.id))?;
    let value = runtime.wait_completed(stopped.id, Duration::from_secs(1))?;
    println!(
        "tunable filter timing stop completed: {}",
        completion_summary(&value)
    );

    for key in [
        "wavelength",
        "bandwidth",
        "output_enabled",
        "trigger_mode",
        "status",
        "firmware",
    ] {
        if property(&filter, key).is_some() {
            let value = runtime.execute(
                Command::read_property(filter.id, key),
                Duration::from_secs(1),
            )?;
            println!("{key}: {}", completion_summary(&value));
        }
    }

    while let Some(event) = events.recv_timeout(Duration::from_millis(50)) {
        println!("event: {}", event_summary(&runtime_devices, &event));
    }

    Ok(())
}

fn filter_driver(source: &str, id: DriverId) -> numanager_core::Result<Box<dyn Driver>> {
    match source {
        "starlight" | "starlight-xpress" | "sx" => Ok(Box::new(SxFilterWheelDriver::configured(
            id,
            SxFilterWheelConfiguredProbe::fixture(),
        ))),
        "prior" | "proscan" => Ok(Box::new(PriorDriver::simulated(id))),
        "ix85" | "evident-ix85" | "evident_ix85" | "olympus-ix85" | "olympus_ix85" => Ok(Box::new(
            Ix85Driver::configured(id, Ix85ConfiguredProbe::fixture()),
        )),
        "kurios" | "thorlabs-kurios" | "lctf" => Ok(Box::new(KuriosDriver::configured_fixture(id))),
        other => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("unknown filter family {other:?}; use one of: starlight, prior, ix85, kurios"),
        )),
    }
}

fn device_with_filter_select<'a>(
    driver: &dyn Driver,
    devices: &'a [DeviceDescriptor],
) -> numanager_core::Result<&'a DeviceDescriptor> {
    devices
        .iter()
        .find(|device| {
            driver
                .capabilities(device.id)
                .iter()
                .any(|capability| capability.kind == CapabilityKind::FilterSelect)
        })
        .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "missing filter selector device"))
}

fn filter_position_key(device: &DeviceDescriptor) -> numanager_core::Result<&str> {
    device
        .properties
        .iter()
        .find(|property| {
            property.writable
                && property.value_type == ValueType::I64
                && (property.key == "position" || property.key.ends_with("_position"))
        })
        .map(|property| property.key.as_str())
        .ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                format!(
                    "{} has no writable integer filter-position property",
                    device.label
                ),
            )
        })
}
