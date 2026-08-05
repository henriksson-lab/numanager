use numanager_core::runtime::{LocalRuntime, Runtime};
use numanager_core::*;
use numanager_drivers::spark_cyto::SparkCytoDriver;
use numanager_examples::{
    capability_brief, completion_summary, driver_capability_by_kind as capability_by_kind,
    public_capability_summary, public_kind_tags,
};
use std::time::Duration;

pub fn run() -> numanager_core::Result<()> {
    let driver = numanager_examples::driver_value(SparkCytoDriver::simulated);
    let driver_id = driver.id();
    let devices = driver.descriptors();

    println!("spark devices:");
    for device in &devices {
        let capabilities = public_capability_summary(driver.capabilities(device.id));
        println!(
            "  {} {:?} typed caps=[{}]",
            device.label,
            public_kind_tags(device),
            capabilities
        );
    }

    let graph_order = driver.graph().initialization_order()?;
    println!("initialization order has {} graph nodes", graph_order.len());

    let plate = device_by_label(&devices, "spark-mainboard")?;
    let absorbance = device_by_label(&devices, "spark-absorbance")?;
    let fluorescence = device_by_label(&devices, "spark-fluorescence")?;
    let luminescence = device_by_label(&devices, "spark-luminescence")?;
    let temperature = device_by_label(&devices, "spark-temperature")?;
    let gas = device_by_label(&devices, "spark-gas")?;
    let fim = device_by_label(&devices, "spark-fim")?;
    let camera = device_by_label(&devices, "spark-camera-binding")?;
    let plate_move = capability_by_kind(&driver, &plate, CapabilityKind::PlateMove)?;
    let absorbance_measure = capability_by_kind(&driver, &absorbance, CapabilityKind::Measure)?;
    let temperature_control =
        capability_by_kind(&driver, &temperature, CapabilityKind::TemperatureControl)?;
    let gas_control = capability_by_kind(&driver, &gas, CapabilityKind::GasControl)?;
    let fim_control = capability_by_kind(&driver, &fim, CapabilityKind::ImagingHead)?;
    let camera_binding = capability_by_kind(&driver, &camera, CapabilityKind::CameraBinding)?;

    println!(
        "capabilities: plate={}; absorbance={}; temperature={}; gas={}; fim={}; camera={}",
        capability_brief(&plate_move),
        capability_brief(&absorbance_measure),
        capability_brief(&temperature_control),
        capability_brief(&gas_control),
        capability_brief(&fim_control),
        capability_brief(&camera_binding)
    );

    let mut runtime = LocalRuntime::new();
    let added_devices = runtime.add_driver(Box::new(driver))?;
    println!("added spark driver with {} device(s)", added_devices.len());

    let logs = runtime.subscribe(EventFilter::kind(EventKind::Log));

    let state = StateSet::prepare_then_commit("plate-read-position")
        .with_write(&plate, "well", Value::String("A01".into()))
        .with_write(
            &absorbance,
            "wavelength",
            Value::Wavelength(Wavelength::from_nanometers(600.0)),
        )
        .with_write(
            &temperature,
            "target",
            Value::Temperature(Temperature::from_celsius(37.0)),
        )
        .with_write(
            &gas,
            "co2_target",
            Value::GasConcentration(GasConcentration::from_percent(5.0)),
        )
        .with_write(&gas, "enabled", Value::Bool(true))
        .with_write(&fim, "objective", Value::I64(2))
        .with_write(&fim, "mode", Value::String("brightfield".into()));

    let value = runtime.execute(state.into_command(), Duration::from_secs(1))?;
    println!("state set completed: {}", completion_summary(&value));

    let value = runtime.execute_request(
        &plate,
        PlateMoveRequest { well: "B03".into() },
        Duration::from_secs(1),
    )?;
    println!("plate move completed: {}", completion_summary(&value));

    let value = runtime.execute_request(
        &absorbance,
        MeasureRequest {
            integration_time: Some(TimeInterval::from_milliseconds(100.0)),
        },
        Duration::from_secs(1),
    )?;
    println!(
        "absorbance measure completed: {}",
        completion_summary(&value)
    );

    let value = runtime.execute_request(
        &temperature,
        TemperatureControlRequest {
            target: Some(Temperature::from_celsius(36.5)),
            enabled: Some(true),
        },
        Duration::from_secs(1),
    )?;
    println!(
        "temperature control completed: {}",
        completion_summary(&value)
    );

    let value = runtime.execute_request(
        &gas,
        GasControlRequest {
            co2_target: Some(GasConcentration::from_percent(4.5)),
            // The Cyto's chamber controls oxygen too, down to hypoxic levels.
            o2_target: Some(GasConcentration::from_percent(5.0)),
            enabled: Some(true),
        },
        Duration::from_secs(1),
    )?;
    println!("gas control completed: {}", completion_summary(&value));

    let value = runtime.execute_request(
        &fim,
        ImagingHeadRequest {
            objective: Some(3),
            mode: Some("fluorescence".into()),
        },
        Duration::from_secs(1),
    )?;
    println!("imaging head completed: {}", completion_summary(&value));

    let value = runtime.execute_request(
        &camera,
        CameraBindingRequest {
            bound: Some(true),
            imaging_mode: Some("fluorescence".into()),
        },
        Duration::from_secs(1),
    )?;
    println!("camera binding completed: {}", completion_summary(&value));

    // Focus is motion on this instrument, not a camera setting: the objective's height is an
    // ordinary axis, so a client drives it the same way it drives any stage.
    let stage_z = device_by_label(&devices, "spark-stage-z")?;
    let value = runtime.execute_request(
        &stage_z,
        StageMoveRequest::absolute([(StageAxis::Z, Position::from_micrometers(1250.0))]),
        Duration::from_secs(1),
    )?;
    println!("focus move completed: {}", completion_summary(&value));

    let filter = device_by_label(&devices, "spark-filter-excitation")?;
    let value = runtime.execute_request(
        &filter,
        FilterSelectRequest::position(2),
        Duration::from_secs(1),
    )?;
    println!(
        "excitation filter completed: {}",
        completion_summary(&value)
    );

    let injector = device_by_label(&devices, "spark-injector")?;
    let value = runtime.execute_request(
        &injector,
        InjectRequest::dispense(2, Volume::from_microliters(25.0)),
        Duration::from_secs(1),
    )?;
    println!(
        "injector dispense completed: {}",
        completion_summary(&value)
    );

    // The camera is reachable through the reader itself: `CAMERA TAKEIMAGE` uploads the
    // raster on the TDCL data channel. Without an instrument there are no pixels, and this
    // driver says so rather than inventing a picture.
    let camera_device = device_by_label(&devices, "spark-camera")?;
    match runtime.execute_request(
        &camera_device,
        CameraCaptureRequest::default_frame(),
        Duration::from_secs(1),
    ) {
        Ok(value) => println!("capture completed: {}", completion_summary(&value)),
        Err(error) => println!("capture refused without an instrument: {}", error.message),
    }

    let shaker = device_by_label(&devices, "spark-shaker")?;
    let value = runtime.execute_request(
        &shaker,
        ShakeRequest::new()
            .with_mode("orbital")
            .with_amplitude(Position::from_micrometers(3000.0))
            .with_frequency(Frequency::from_hertz(8.0))
            .for_duration(TimeInterval::from_seconds(30.0)),
        Duration::from_secs(1),
    )?;
    println!("shake completed: {}", completion_summary(&value));

    let barcode = device_by_label(&devices, "spark-barcode")?;
    let value =
        runtime.execute_request(&barcode, BarcodeRequest::read(), Duration::from_secs(1))?;
    println!("barcode read completed: {}", completion_summary(&value));

    let plan = TimingPlan::from_parts(
        Vec::new(),
        vec![
            DeviceSequence::new(
                &plate,
                "well",
                [Value::String("A01".into()), Value::String("A02".into())],
            ),
            DeviceSequence::new(
                &absorbance,
                "wavelength",
                [
                    Value::Wavelength(Wavelength::from_nanometers(600.0)),
                    Value::Wavelength(Wavelength::from_nanometers(450.0)),
                ],
            ),
            DeviceSequence::new(
                &fluorescence,
                "enabled",
                [Value::Bool(true), Value::Bool(false)],
            ),
            DeviceSequence::new(
                &fluorescence,
                "wavelength",
                [
                    Value::Wavelength(Wavelength::from_nanometers(520.0)),
                    Value::Wavelength(Wavelength::from_nanometers(610.0)),
                ],
            ),
            DeviceSequence::new(
                &luminescence,
                "enabled",
                [Value::Bool(true), Value::Bool(false)],
            ),
            DeviceSequence::new(
                &temperature,
                "enabled",
                [Value::Bool(true), Value::Bool(false)],
            ),
            DeviceSequence::new(
                &temperature,
                "target",
                [
                    Value::Temperature(Temperature::from_celsius(37.0)),
                    Value::Temperature(Temperature::from_celsius(25.0)),
                ],
            ),
            DeviceSequence::new(
                &gas,
                "co2_target",
                [
                    Value::GasConcentration(GasConcentration::from_percent(5.0)),
                    Value::GasConcentration(GasConcentration::from_percent(0.04)),
                ],
            ),
            DeviceSequence::new(&gas, "enabled", [Value::Bool(true), Value::Bool(false)]),
            DeviceSequence::new(&fim, "objective", [Value::I64(2), Value::I64(1)]),
            DeviceSequence::new(
                &fim,
                "mode",
                [
                    Value::String("fluorescence".into()),
                    Value::String("brightfield".into()),
                ],
            ),
            DeviceSequence::new(&camera, "bound", [Value::Bool(true), Value::Bool(false)]),
            DeviceSequence::new(
                &camera,
                "imaging_mode",
                [
                    Value::String("brightfield".into()),
                    Value::String("fluorescence".into()),
                ],
            ),
        ],
        vec![
            &temperature,
            &gas,
            &fim,
            &plate,
            &absorbance,
            &fluorescence,
            &luminescence,
            &camera,
        ],
        StartCondition::Software,
        StopCondition::Manual,
    )?;
    let armed = runtime.submit(Command::arm(plan))?;
    let value = runtime.wait_completed(armed.id, Duration::from_secs(1))?;
    println!("timing arm: {}", completion_summary(&value));
    let started = runtime.submit(Command::start(armed.id))?;
    let value = runtime.wait_completed(started.id, Duration::from_secs(1))?;
    println!("timing start: {}", completion_summary(&value));
    let stopped = runtime.submit(Command::stop(armed.id))?;
    let value = runtime.wait_completed(stopped.id, Duration::from_secs(1))?;
    println!("timing stop: {}", completion_summary(&value));
    for (label, device, key) in [
        ("gas", &gas, "co2_target"),
        ("gas", &gas, "co2_actual"),
        ("gas", &gas, "enabled"),
        ("gas", &gas, "fault"),
        ("fim", &fim, "objective"),
        ("fim", &fim, "mode"),
        ("fim", &fim, "interlock_closed"),
        ("fim", &fim, "fault"),
    ] {
        let value = runtime.execute(Command::read_property(device, key), Duration::from_secs(1))?;
        println!("{label}.{key}: {}", completion_summary(&value));
    }

    match logs.recv_timeout(Duration::from_secs(1)) {
        Some(Event::Log(_)) => println!("runtime emitted a driver log event"),
        _ => println!("state set completed without a log event"),
    }

    let removed = runtime.remove_driver(driver_id)?;
    println!("removed spark driver with {} device(s)", removed.len());

    Ok(())
}

fn device_by_label(
    devices: &[DeviceDescriptor],
    label: &str,
) -> numanager_core::Result<DeviceDescriptor> {
    devices
        .iter()
        .find(|device| device.label == label)
        .cloned()
        .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, format!("missing device {label}")))
}
