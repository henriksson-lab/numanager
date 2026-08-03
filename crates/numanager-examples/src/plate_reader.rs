use numanager_core::runtime::{LocalRuntime, Runtime};
use numanager_core::*;
use numanager_drivers::spark_cyto::SparkCytoDriver;
use numanager_examples::{
    capability_brief, completion_summary, device_by_kind,
    driver_capability_by_kind as capability_by_kind, event_summary, is_public_property,
    public_kind_summary, push_schema_write,
};
use std::time::Duration;

pub fn run() -> numanager_core::Result<()> {
    let detector_choice = numanager_examples::example_arg(0).unwrap_or_else(|| "absorbance".into());
    let driver = numanager_examples::driver_value(plate_reader_driver);
    let devices = driver.descriptors();
    let detector_kind = detector_kind(&detector_choice)?;
    let imaging_mode = imaging_mode(&detector_choice)?;

    let plate = device_by_kind(&devices, "plate.transport")?;
    let detector = device_by_kind(&devices, detector_kind)?;
    let imaging_head = device_by_kind(&devices, "imaging.head")?;
    let camera = device_by_kind(&devices, "camera.binding")?;

    let plate_move = capability_by_kind(&*driver, plate, CapabilityKind::PlateMove)?;
    let measure = capability_by_kind(&*driver, detector, CapabilityKind::Measure)?;
    let imaging = capability_by_kind(&*driver, imaging_head, CapabilityKind::ImagingHead)?;
    let binding = capability_by_kind(&*driver, camera, CapabilityKind::CameraBinding)?;

    println!("selected plate-reader family: spark_cyto");
    println!(
        "selected plate transport: {} [{}]",
        plate.label,
        public_kind_summary(plate)
    );
    println!(
        "selected detector: {} [{}]",
        detector.label,
        public_kind_summary(detector)
    );
    println!(
        "selected imaging head: {} [{}]",
        imaging_head.label,
        public_kind_summary(imaging_head)
    );
    println!(
        "selected camera binding: {} [{}]",
        camera.label,
        public_kind_summary(camera)
    );
    println!(
        "capabilities: plate={}; detector={}; imaging={}; camera={}",
        capability_brief(&plate_move),
        capability_brief(&measure),
        capability_brief(&imaging),
        capability_brief(&binding)
    );
    for device in [plate, detector, imaging_head, camera] {
        for property in device
            .properties
            .iter()
            .filter(|property| is_public_property(property))
        {
            println!(
                "plate-reader property: {}.{} type={:?} writable={} sequenceable={}",
                device.label,
                property.key,
                property.value_type,
                property.writable,
                property.sequenceable
            );
        }
    }

    let runtime = LocalRuntime::from_drivers(vec![driver]);
    let runtime_devices = runtime.devices().into_iter().cloned().collect::<Vec<_>>();
    let events = runtime.subscribe(
        EventFilter::devices([plate, detector, imaging_head, camera])
            .with_kinds([EventKind::OperationChanged, EventKind::PropertyChanged]),
    );

    let mut writes = Vec::new();
    push_schema_write(plate, &mut writes, "well", Value::String("A01".into()));
    push_schema_write(
        detector,
        &mut writes,
        "wavelength",
        Value::Wavelength(setup_wavelength(&detector_choice)?),
    );
    push_schema_write(detector, &mut writes, "enabled", Value::Bool(true));
    push_schema_write(imaging_head, &mut writes, "objective", Value::I64(2));
    push_schema_write(
        imaging_head,
        &mut writes,
        "mode",
        Value::String(imaging_mode.into()),
    );
    push_schema_write(camera, &mut writes, "bound", Value::Bool(true));
    push_schema_write(
        camera,
        &mut writes,
        "imaging_mode",
        Value::String(imaging_mode.into()),
    );

    let setup = runtime.submit(
        StateSet::immediate("plate-reader setup")
            .with_writes(writes)
            .into_command(),
    )?;
    let value = runtime.wait_completed(setup.id, Duration::from_secs(1))?;
    println!(
        "plate-reader setup completed: {}",
        completion_summary(&value)
    );

    let moved = runtime.submit_request(plate, PlateMoveRequest { well: "B03".into() })?;
    let value = runtime.wait_completed(moved.id, Duration::from_secs(1))?;
    println!("plate move completed: {}", completion_summary(&value));

    let measured = runtime.submit_request(
        detector,
        MeasureRequest {
            integration_time: Some(TimeInterval::from_milliseconds(100.0)),
        },
    )?;
    let value = runtime.wait_completed(measured.id, Duration::from_secs(1))?;
    println!("detector measure completed: {}", completion_summary(&value));

    let image_configured = runtime.submit_request(
        imaging_head,
        ImagingHeadRequest {
            objective: Some(3),
            mode: Some(imaging_mode.into()),
        },
    )?;
    let value = runtime.wait_completed(image_configured.id, Duration::from_secs(1))?;
    println!("imaging head completed: {}", completion_summary(&value));

    let camera_bound = runtime.submit_request(
        camera,
        CameraBindingRequest {
            bound: Some(true),
            imaging_mode: Some(imaging_mode.into()),
        },
    )?;
    let value = runtime.wait_completed(camera_bound.id, Duration::from_secs(1))?;
    println!("camera binding completed: {}", completion_summary(&value));

    for (label, device, key) in [
        ("plate", plate, "well"),
        ("detector", detector, "wavelength"),
        ("detector", detector, "enabled"),
        ("imaging", imaging_head, "objective"),
        ("imaging", imaging_head, "mode"),
        ("imaging", imaging_head, "interlock_closed"),
        ("imaging", imaging_head, "fault"),
        ("camera", camera, "bound"),
        ("camera", camera, "imaging_mode"),
    ] {
        if device.properties.iter().any(|property| property.key == key) {
            let value =
                runtime.execute(Command::read_property(device, key), Duration::from_secs(1))?;
            println!("{label}.{key}: {}", completion_summary(&value));
        }
    }

    while let Some(event) = events.recv_timeout(Duration::from_millis(50)) {
        println!("event: {}", event_summary(&runtime_devices, &event));
    }

    Ok(())
}

fn plate_reader_driver(id: DriverId) -> Box<dyn Driver> {
    Box::new(SparkCytoDriver::simulated(id))
}

fn detector_kind(choice: &str) -> numanager_core::Result<&'static str> {
    match choice {
        "absorbance" => Ok("detector.absorbance"),
        "fluorescence" => Ok("detector.fluorescence"),
        "luminescence" => Ok("detector.luminescence"),
        other => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!(
                "unknown detector {other:?}; use one of: absorbance, fluorescence, luminescence"
            ),
        )),
    }
}

fn imaging_mode(choice: &str) -> numanager_core::Result<&'static str> {
    match choice {
        "absorbance" => Ok("brightfield"),
        "fluorescence" | "luminescence" => Ok("fluorescence"),
        other => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!(
                "unknown detector {other:?}; use one of: absorbance, fluorescence, luminescence"
            ),
        )),
    }
}

fn setup_wavelength(choice: &str) -> numanager_core::Result<Wavelength> {
    match choice {
        "absorbance" => Ok(Wavelength::from_nanometers(600.0)),
        "fluorescence" => Ok(Wavelength::from_nanometers(520.0)),
        "luminescence" => Ok(Wavelength::from_nanometers(0.0)),
        other => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!(
                "unknown detector {other:?}; use one of: absorbance, fluorescence, luminescence"
            ),
        )),
    }
}
