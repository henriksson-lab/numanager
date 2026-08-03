use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DiscoveryRegistry, DriverDiscovery, LocalRuntime, Runtime};
use numanager_core::*;
use numanager_drivers::arduino::{ArduinoDiscovery, ArduinoDriver};
use numanager_drivers::arduino_counter::{ArduinoCounterDiscovery, ArduinoCounterDriver};
use numanager_drivers::asi::AsiTigerDriver;
use numanager_drivers::esp32::Esp32Discovery;
use numanager_drivers::modbus::ModbusDiscovery;
use numanager_drivers::teensy_pulse::{TeensyPulseDiscovery, TeensyPulseDriver};
use numanager_drivers::triggerscope::TriggerScopeDiscovery;
use numanager_drivers::velleman::VellemanDiscovery;
use numanager_drivers::wosm::WosmDiscovery;
use numanager_examples::{
    capability_brief, completion_summary, device_by_kind,
    driver_capability_by_kind as capability_by_kind, event_summary, public_kind_summary,
    schema_state_write,
};
use std::time::Duration;

pub fn run() -> numanager_core::Result<()> {
    if let Some(source) = numanager_examples::example_arg(0) {
        return run_configured_source(&source);
    }

    let arduino = numanager_examples::driver_value(ArduinoDriver::simulated);
    let arduino_devices = arduino.descriptors();
    let digital = device_by_kind(&arduino_devices, "digital.io")?;
    let shutter = device_by_kind(&arduino_devices, "trigger.sink")?;
    let adc = device_by_kind(&arduino_devices, "analog.input")?;
    let dac = device_by_kind(&arduino_devices, "analog.output")?;
    let digital_io = capability_by_kind(&arduino, digital, CapabilityKind::DigitalIo)?;
    let shutter_trigger = capability_by_kind(&arduino, shutter, CapabilityKind::TriggerSink)?;
    let adc_read = capability_by_kind(&arduino, adc, CapabilityKind::Adc)?;
    let dac_write = capability_by_kind(&arduino, dac, CapabilityKind::Dac)?;

    let counter = numanager_examples::driver_value(ArduinoCounterDriver::simulated);
    let counter_devices = counter.descriptors();
    let counter_device = device_by_kind(&counter_devices, "counter")?;
    let pulse = device_by_kind(&counter_devices, "pulse.generator")?;
    let measure = capability_by_kind(&counter, counter_device, CapabilityKind::Measure)?;
    let pulse_program = capability_by_kind(&counter, counter_device, CapabilityKind::PulseProgram)?;
    let pulse_trigger = capability_by_kind(&counter, pulse, CapabilityKind::TriggerSource)?;

    let tiger = numanager_examples::driver_value(AsiTigerDriver::simulated);
    let tiger_devices = tiger.descriptors();
    let tiger_ttl = device_by_kind(&tiger_devices, "trigger.source")?;
    let tiger_ring = device_by_kind(&tiger_devices, "ring.buffer")?;
    let tiger_trigger = capability_by_kind(&tiger, tiger_ttl, CapabilityKind::TriggerSource)?;
    let tiger_program = capability_by_kind(&tiger, tiger_ring, CapabilityKind::PulseProgram)?;

    let teensy = numanager_examples::driver_value(TeensyPulseDriver::simulated);
    let teensy_devices = teensy.descriptors();
    let standalone_pulse = device_by_kind(&teensy_devices, "pulse.generator")?;
    let standalone_program =
        capability_by_kind(&teensy, standalone_pulse, CapabilityKind::PulseProgram)?;
    let standalone_trigger =
        capability_by_kind(&teensy, standalone_pulse, CapabilityKind::TriggerSource)?;

    println!(
        "selected digital output: {} [{}]",
        digital.label,
        public_kind_summary(digital)
    );
    println!(
        "selected shutter input: {} [{}]",
        shutter.label,
        public_kind_summary(shutter)
    );
    println!(
        "selected analog input: {} [{}]",
        adc.label,
        public_kind_summary(adc)
    );
    println!(
        "selected analog output: {} [{}]",
        dac.label,
        public_kind_summary(dac)
    );
    println!(
        "selected counter: {} [{}]; pulse output: {} [{}]",
        counter_device.label,
        public_kind_summary(counter_device),
        pulse.label,
        public_kind_summary(pulse)
    );
    println!(
        "selected ASI Tiger TTL: {} [{}]; ring buffer: {} [{}]",
        tiger_ttl.label,
        public_kind_summary(tiger_ttl),
        tiger_ring.label,
        public_kind_summary(tiger_ring)
    );
    println!(
        "selected standalone pulse generator: {} [{}]",
        standalone_pulse.label,
        public_kind_summary(standalone_pulse)
    );
    println!(
        "capabilities: digital={}; shutter={}; adc={}; dac={}; measure={}; pulse_program={}; pulse_trigger={}; tiger_trigger={}; tiger_program={}; standalone_program={}; standalone_trigger={}",
        capability_brief(&digital_io),
        capability_brief(&shutter_trigger),
        capability_brief(&adc_read),
        capability_brief(&dac_write),
        capability_brief(&measure),
        capability_brief(&pulse_program),
        capability_brief(&pulse_trigger),
        capability_brief(&tiger_trigger),
        capability_brief(&tiger_program),
        capability_brief(&standalone_program),
        capability_brief(&standalone_trigger)
    );

    let runtime = LocalRuntime::from_drivers(vec![
        Box::new(arduino),
        Box::new(counter),
        Box::new(tiger),
        Box::new(teensy),
    ]);
    let runtime_devices = runtime.devices().into_iter().cloned().collect::<Vec<_>>();
    let events = runtime.subscribe(
        EventFilter::devices([
            digital,
            shutter,
            dac,
            counter_device,
            pulse,
            standalone_pulse,
            tiger_ttl,
            tiger_ring,
        ])
        .with_kinds([EventKind::OperationChanged, EventKind::PropertyChanged]),
    );

    let setup = runtime.submit(
        StateSet::immediate("digital io setup")
            .with_write(digital, "mask", Value::I64(0b0000_0011))
            .with_write(
                digital,
                "timed_delays",
                Value::List(vec![
                    Value::TimeInterval(TimeInterval::from_milliseconds(2.0)),
                    Value::TimeInterval(TimeInterval::from_milliseconds(5.0)),
                ]),
            )
            .with_write(shutter, "open", Value::Bool(false))
            .with_write(dac, "channel_0", Value::I64(128))
            .with_write(
                counter_device,
                "gate",
                Value::TimeInterval(TimeInterval::from_milliseconds(25.0)),
            )
            .with_write(
                counter_device,
                "interval",
                Value::TimeInterval(TimeInterval::from_microseconds(1_000.0)),
            )
            .with_write(pulse, "level", Value::Bool(false))
            .with_write(
                standalone_pulse,
                "interval",
                Value::TimeInterval(TimeInterval::from_microseconds(2_000.0)),
            )
            .with_write(
                standalone_pulse,
                "duration",
                Value::TimeInterval(TimeInterval::from_microseconds(250.0)),
            )
            .with_write(standalone_pulse, "number_of_pulses", Value::I64(3))
            .with_write(standalone_pulse, "wait_for_input", Value::Bool(false))
            .into_command(),
    )?;
    let value = runtime.wait_completed(setup.id, Duration::from_secs(1))?;
    println!("state set completed: {}", completion_summary(&value));

    let digital_write = runtime.submit_request(digital, DigitalIoRequest { mask: 0b0000_0101 })?;
    let value = runtime.wait_completed(digital_write.id, Duration::from_secs(1))?;
    println!("digital write completed: {}", completion_summary(&value));

    let analog_output = runtime.submit_request(
        dac,
        DacRequest {
            value: Value::I64(256),
        },
    )?;
    let value = runtime.wait_completed(analog_output.id, Duration::from_secs(1))?;
    println!("analog output completed: {}", completion_summary(&value));

    let pulse_programmed = runtime.submit_request(
        counter_device,
        PulseProgramRequest {
            interval: Some(TimeInterval::from_microseconds(500.0)),
            duration: None,
            count: None,
            wait_for_input: None,
        },
    )?;
    let value = runtime.wait_completed(pulse_programmed.id, Duration::from_secs(1))?;
    println!("pulse program completed: {}", completion_summary(&value));

    let pulse_fired = runtime.submit_capability(
        pulse,
        CapabilityKind::TriggerSource,
        CapabilityRequest::None,
    )?;
    let value = runtime.wait_completed(pulse_fired.id, Duration::from_secs(1))?;
    println!("pulse trigger completed: {}", completion_summary(&value));

    let standalone_programmed = runtime.submit_request(
        standalone_pulse,
        PulseProgramRequest {
            interval: Some(TimeInterval::from_microseconds(1_500.0)),
            duration: Some(TimeInterval::from_microseconds(200.0)),
            count: Some(4),
            wait_for_input: Some(false),
        },
    )?;
    let value = runtime.wait_completed(standalone_programmed.id, Duration::from_secs(1))?;
    println!(
        "standalone pulse program completed: {}",
        completion_summary(&value)
    );

    let standalone_pulsed = runtime.submit_capability(
        standalone_pulse,
        CapabilityKind::TriggerSource,
        CapabilityRequest::Trigger(TriggerRequest::pulse()),
    )?;
    let value = runtime.wait_completed(standalone_pulsed.id, Duration::from_secs(1))?;
    println!("standalone pulse completed: {}", completion_summary(&value));

    let tiger_ttl_pulsed = runtime.submit_capability(
        tiger_ttl,
        CapabilityKind::TriggerSource,
        CapabilityRequest::Trigger(TriggerRequest::pulse()),
    )?;
    let value = runtime.wait_completed(tiger_ttl_pulsed.id, Duration::from_secs(1))?;
    println!(
        "ASI Tiger TTL pulse completed: {}",
        completion_summary(&value)
    );

    let tiger_ring_started = runtime.submit_request(
        tiger_ring,
        PulseProgramRequest {
            interval: None,
            duration: None,
            count: Some(8),
            wait_for_input: Some(false),
        },
    )?;
    let value = runtime.wait_completed(tiger_ring_started.id, Duration::from_secs(1))?;
    println!(
        "ASI Tiger ring program completed: {}",
        completion_summary(&value)
    );

    let shutter_pulse = runtime.submit_capability(
        shutter,
        CapabilityKind::TriggerSink,
        CapabilityRequest::None,
    )?;
    let value = runtime.wait_completed(shutter_pulse.id, Duration::from_secs(1))?;
    println!("shutter pulse completed: {}", completion_summary(&value));

    let analog = runtime.submit_request(
        adc,
        AdcRequest {
            channel: Some("0".into()),
            integration_time: None,
        },
    )?;
    let value = runtime.wait_completed(analog.id, Duration::from_secs(1))?;
    println!("analog read completed: {}", completion_summary(&value));

    let counted = runtime.submit_request(
        counter_device,
        MeasureRequest {
            integration_time: Some(TimeInterval::from_milliseconds(10.0)),
        },
    )?;
    let value = runtime.wait_completed(counted.id, Duration::from_secs(1))?;
    println!("counter measure completed: {}", completion_summary(&value));

    let armed = runtime.submit(
        TimingPlan::builder()
            .sequence(
                digital,
                "mask",
                [Value::I64(0b0000_0001), Value::I64(0b0000_0010)],
            )
            .sequence(shutter, "open", [Value::Bool(true), Value::Bool(false)])
            .sequence(
                counter_device,
                "interval",
                [
                    Value::TimeInterval(TimeInterval::from_microseconds(500.0)),
                    Value::TimeInterval(TimeInterval::from_microseconds(1_000.0)),
                ],
            )
            .sequence(pulse, "level", [Value::Bool(true), Value::Bool(false)])
            .arm_order([digital, shutter, counter_device, pulse])
            .stop(StopCondition::Count(2))
            .into_command()?,
    )?;
    let value = runtime.wait_completed(armed.id, Duration::from_secs(1))?;
    println!("timing arm completed: {}", completion_summary(&value));
    let started = runtime.submit(Command::start(armed.id))?;
    let value = runtime.wait_completed(started.id, Duration::from_secs(1))?;
    println!("timing start completed: {}", completion_summary(&value));
    let stopped = runtime.submit(Command::stop(armed.id))?;
    let value = runtime.wait_completed(stopped.id, Duration::from_secs(1))?;
    println!("timing stop completed: {}", completion_summary(&value));

    for (device, key) in [
        (digital, "mask"),
        (digital, "timed_delays"),
        (shutter, "open"),
        (adc, "input_summary"),
        (dac, "channel_0"),
        (counter_device, "gate"),
        (counter_device, "count"),
        (counter_device, "interval"),
        (pulse, "level"),
        (standalone_pulse, "interval"),
        (standalone_pulse, "duration"),
        (standalone_pulse, "number_of_pulses"),
        (standalone_pulse, "running"),
    ] {
        let value = runtime.execute(Command::read_property(device, key), Duration::from_secs(1))?;
        println!("{key}: {}", completion_summary(&value));
    }

    while let Some(event) = events.recv_timeout(Duration::from_millis(50)) {
        println!("event: {}", event_summary(&runtime_devices, &event));
    }

    Ok(())
}

fn run_configured_source(source: &str) -> numanager_core::Result<()> {
    let mut discovery = DiscoveryRegistry::new();
    discovery.register_boxed_factory_result(|id| digital_discovery(source, id))?;
    let mut candidates = discovery.detect_all()?;
    println!("selected digital IO source: {source}");
    println!("detected {} candidate(s)", candidates.len());
    let candidate = candidates.pop().ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidCommand,
            "no digital IO candidate detected",
        )
    })?;
    println!("candidate: {}", candidate.label());
    for device in candidate.devices() {
        println!("  {} [{}]", device.label, public_kind_summary(device));
    }

    let mut runtime = LocalRuntime::new();
    let added_devices = runtime.add_candidate(candidate)?;
    let runtime_devices = runtime.devices().into_iter().cloned().collect::<Vec<_>>();
    let devices = added_devices.iter().collect::<Vec<_>>();
    let digital = first_with_capability(&runtime, &devices, CapabilityKind::DigitalIo);
    let trigger_source = first_with_capability(&runtime, &devices, CapabilityKind::TriggerSource);
    let trigger_sink = first_with_capability(&runtime, &devices, CapabilityKind::TriggerSink);
    let dac = first_with_capability(&runtime, &devices, CapabilityKind::Dac);
    let adc = first_with_capability(&runtime, &devices, CapabilityKind::Adc);
    let measure = first_with_capability(&runtime, &devices, CapabilityKind::Measure);

    print_selection("digital device", digital);
    print_selection("trigger source", trigger_source);
    print_selection("trigger sink", trigger_sink);
    print_selection("analog output", dac);
    print_selection("analog input", adc);
    print_selection("measurement device", measure);

    let event_devices = [digital, trigger_source, trigger_sink, dac, adc, measure]
        .into_iter()
        .flatten()
        .map(|device| device.id)
        .collect::<Vec<_>>();
    let events = runtime.subscribe(
        EventFilter::devices(event_devices)
            .with_kinds([EventKind::OperationChanged, EventKind::PropertyChanged]),
    );

    let mut setup_writes = Vec::new();
    if let Some(device) = digital {
        push_optional_write(device, &mut setup_writes, "mask", Value::I64(3));
        push_optional_write(device, &mut setup_writes, "state", Value::I64(3));
        push_optional_write(device, &mut setup_writes, "high", Value::Bool(true));
    }
    if let Some(device) = trigger_sink {
        push_optional_write(device, &mut setup_writes, "open", Value::Bool(false));
        push_optional_write(device, &mut setup_writes, "enabled", Value::Bool(false));
        push_optional_write(device, &mut setup_writes, "high", Value::Bool(false));
    }
    if let Some(device) = dac {
        if let Some((key, value)) = dac_property_value(device, 25.0) {
            push_optional_write(device, &mut setup_writes, key, value);
        }
        push_optional_write(device, &mut setup_writes, "enabled", Value::Bool(true));
    }
    if !setup_writes.is_empty() {
        let setup = runtime.submit(
            StateSet::immediate("digital io source setup")
                .with_writes(setup_writes)
                .into_command(),
        )?;
        let value = runtime.wait_completed(setup.id, Duration::from_secs(1))?;
        println!("state set completed: {}", completion_summary(&value));
    } else {
        println!("state set completed: none");
    }

    if let Some(device) = devices
        .iter()
        .copied()
        .find(|device| device.has_kind("mapped.io"))
    {
        let mut writes = Vec::new();
        push_optional_write(device, &mut writes, "enabled", Value::Bool(true));
        push_optional_write(device, &mut writes, "target_register", Value::I64(42));
        if !writes.is_empty() {
            let command = runtime.submit(
                StateSet::immediate("mapped io setup")
                    .with_writes(writes)
                    .into_command(),
            )?;
            let value = runtime.wait_completed(command.id, Duration::from_secs(1))?;
            println!(
                "mapped IO state set completed: {}",
                completion_summary(&value)
            );
        }
        for key in ["enabled", "target_register", "measured_register"] {
            if device.properties.iter().any(|property| property.key == key) {
                let value =
                    runtime.execute(Command::read_property(device, key), Duration::from_secs(1))?;
                println!("{} {key}: {}", device.label, completion_summary(&value));
            }
        }
    }

    if let Some(device) = digital {
        match runtime
            .submit_request(device, DigitalIoRequest { mask: 5 })
            .and_then(|command| runtime.wait_completed(command.id, Duration::from_secs(1)))
        {
            Ok(value) => println!("digital write completed: {}", completion_summary(&value)),
            Err(error) => println!("digital write skipped: {}", error.message),
        }
    }

    if let Some(device) = dac {
        let value = dac_request_value(device, 42.0)?;
        let command = runtime.submit_request(device, DacRequest { value })?;
        let value = runtime.wait_completed(command.id, Duration::from_secs(1))?;
        println!("analog output completed: {}", completion_summary(&value));
    }

    if let Some(device) = trigger_source {
        match submit_trigger(
            &runtime,
            device,
            CapabilityKind::TriggerSource,
            CapabilityRequest::Trigger(TriggerRequest::pulse()),
        ) {
            Ok(value) => println!("trigger source completed: {}", completion_summary(&value)),
            Err(error) => println!("trigger source skipped: {}", error.message),
        }
    }

    if let Some(device) = trigger_sink {
        match submit_trigger(
            &runtime,
            device,
            CapabilityKind::TriggerSink,
            CapabilityRequest::Trigger(TriggerRequest::pulse()),
        ) {
            Ok(value) => println!("trigger sink completed: {}", completion_summary(&value)),
            Err(error) => println!("trigger sink skipped: {}", error.message),
        }
    }

    if let Some(device) = adc {
        match runtime
            .submit_request(
                device,
                AdcRequest {
                    channel: None,
                    integration_time: None,
                },
            )
            .and_then(|command| runtime.wait_completed(command.id, Duration::from_secs(1)))
        {
            Ok(value) => println!("analog read completed: {}", completion_summary(&value)),
            Err(error) => println!("analog read skipped: {}", error.message),
        }
    }

    if let Some(device) = measure {
        match runtime
            .submit_request(
                device,
                MeasureRequest {
                    integration_time: Some(TimeInterval::from_milliseconds(10.0)),
                },
            )
            .and_then(|command| runtime.wait_completed(command.id, Duration::from_secs(1)))
        {
            Ok(value) => println!("measure completed: {}", completion_summary(&value)),
            Err(error) => println!("measure skipped: {}", error.message),
        }
    }

    for device in [digital, trigger_source, trigger_sink, dac, adc, measure]
        .into_iter()
        .flatten()
    {
        for key in [
            "mask",
            "state",
            "high",
            "open",
            "enabled",
            "channel_0",
            "value",
            "voltage",
            "output",
            "digital_input",
            "input_summary",
        ] {
            if device.properties.iter().any(|property| property.key == key) {
                let value =
                    runtime.execute(Command::read_property(device, key), Duration::from_secs(1))?;
                println!("{} {key}: {}", device.label, completion_summary(&value));
            }
        }
    }
    for device in &devices {
        if device
            .properties
            .iter()
            .any(|property| property.key == "last_transaction")
        {
            let value = runtime.execute(
                Command::read_property(device.id, "last_transaction"),
                Duration::from_secs(1),
            )?;
            println!(
                "{} last_transaction: {}",
                device.label,
                completion_summary(&value)
            );
        }
    }

    while let Some(event) = events.recv_timeout(Duration::from_millis(50)) {
        println!("event: {}", event_summary(&runtime_devices, &event));
    }

    Ok(())
}

fn digital_discovery(
    source: &str,
    id: DriverId,
) -> numanager_core::Result<Box<dyn DriverDiscovery>> {
    match source {
        "arduino" | "mm-arduino" => Ok(Box::new(ArduinoDiscovery::from_config(
            id,
            &digital_hardware_config(source),
        )?)),
        "arduino_counter" | "arduino-counter" | "counter" => Ok(Box::new(
            ArduinoCounterDiscovery::from_config(id, &digital_hardware_config(source))?,
        )),
        "esp32" => Ok(Box::new(Esp32Discovery::simulated(id))),
        "modbus" => Ok(Box::new(ModbusDiscovery::configured_fixture(id))),
        "teensy" | "teensy_pulse" | "teensy-pulse" => Ok(Box::new(
            TeensyPulseDiscovery::from_config(id, &digital_hardware_config(source))?,
        )),
        "triggerscope" => Ok(Box::new(TriggerScopeDiscovery::configured_fixture(id))),
        "velleman" | "k8055" => Ok(Box::new(VellemanDiscovery::configured_fixture(id))),
        "wosm" => Ok(Box::new(WosmDiscovery::configured_fixture(id))),
        other => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("unknown digital IO source {other}; expected one of: arduino, arduino_counter, esp32, modbus, teensy_pulse, triggerscope, velleman, wosm"),
        )),
    }
}

fn digital_hardware_config(source: &str) -> HardwareConfig {
    let driver = match source {
        "arduino" | "mm-arduino" => "arduino",
        "arduino_counter" | "arduino-counter" | "counter" => "arduino_counter",
        "teensy" | "teensy_pulse" | "teensy-pulse" => "teensy_pulse",
        _ => source,
    };
    let (id, label, properties) = match driver {
        "arduino" => (
            69_001,
            "Configured Arduino controller",
            std::collections::BTreeMap::from([
                (
                    "controller_id".into(),
                    Value::String("ARDUINO-CONFIG-0002".into()),
                ),
                ("version".into(), Value::I64(4)),
                ("extended_version".into(), Value::I64(4)),
                ("pattern_count".into(), Value::I64(6)),
                ("dac_channels".into(), Value::I64(2)),
                ("digital_pins".into(), Value::I64(8)),
            ]),
        ),
        "arduino_counter" => (
            70_001,
            "Configured Arduino Counter",
            std::collections::BTreeMap::from([
                (
                    "gate".into(),
                    Value::TimeInterval(TimeInterval::from_milliseconds(100.0)),
                ),
                (
                    "interval".into(),
                    Value::TimeInterval(TimeInterval::from_milliseconds(1.0)),
                ),
                ("count".into(), Value::I64(42)),
                ("pulse_level".into(), Value::Bool(false)),
            ]),
        ),
        "teensy_pulse" => (
            73_001,
            "Configured Teensy pulse generator",
            std::collections::BTreeMap::from([
                ("version".into(), Value::I64(1)),
                (
                    "interval".into(),
                    Value::TimeInterval(TimeInterval::from_milliseconds(100.0)),
                ),
                (
                    "duration".into(),
                    Value::TimeInterval(TimeInterval::from_milliseconds(1.0)),
                ),
                ("wait_for_input".into(), Value::Bool(false)),
            ]),
        ),
        _ => unreachable!("digital_hardware_config is only used by explicit-config selectors"),
    };
    HardwareConfig {
        devices: vec![DeviceConfig::new(id, label, driver, properties)],
        ..Default::default()
    }
}

fn first_with_capability<'a>(
    runtime: &LocalRuntime,
    devices: &[&'a DeviceDescriptor],
    kind: CapabilityKind,
) -> Option<&'a DeviceDescriptor> {
    devices
        .iter()
        .copied()
        .find(|device| runtime.capability_by_kind(device.id, kind.clone()).is_ok())
}

fn print_selection(label: &str, device: Option<&DeviceDescriptor>) {
    match device {
        Some(device) => println!(
            "selected {label}: {} [{}]",
            device.label,
            public_kind_summary(device)
        ),
        None => println!("selected {label}: none"),
    }
}

fn push_optional_write(
    device: &DeviceDescriptor,
    writes: &mut Vec<StateWrite>,
    key: &str,
    value: Value,
) {
    if let Some(write) = schema_state_write(device, key, value) {
        writes.push(write);
    }
}

fn dac_property_value(device: &DeviceDescriptor, percent: f64) -> Option<(&'static str, Value)> {
    for key in ["channel_0", "value", "voltage", "output"] {
        if let Some(property) = device
            .properties
            .iter()
            .find(|property| property.key == key)
        {
            return match property.value_type {
                ValueType::Ratio => Some((key, Value::Ratio(Ratio::from_percent(percent)))),
                ValueType::I64 => {
                    Some((key, Value::I64((percent / 100.0 * 1023.0).round() as i64)))
                }
                ValueType::Voltage => Some((
                    key,
                    Value::Voltage(Voltage::from_volts(percent / 100.0 * 3.3)),
                )),
                _ => None,
            };
        }
    }
    None
}

fn dac_request_value(device: &DeviceDescriptor, percent: f64) -> numanager_core::Result<Value> {
    let capability = device
        .properties
        .iter()
        .find(|property| {
            matches!(
                property.key.as_str(),
                "channel_0" | "value" | "voltage" | "output"
            )
        })
        .ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProperty,
                "Dac device has no supported output property",
            )
        })?;
    match capability.value_type {
        ValueType::Ratio => Ok(Value::Ratio(Ratio::from_percent(percent))),
        ValueType::I64 => Ok(Value::I64((percent / 100.0 * 1023.0).round() as i64)),
        ValueType::Voltage => Ok(Value::Voltage(Voltage::from_volts(percent / 100.0 * 3.3))),
        other => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("unsupported Dac property type {other:?}"),
        )),
    }
}

fn submit_trigger(
    runtime: &LocalRuntime,
    device: &DeviceDescriptor,
    kind: CapabilityKind,
    request: CapabilityRequest,
) -> numanager_core::Result<Value> {
    let command = match runtime.submit_capability(device, kind.clone(), request) {
        Ok(command) => command,
        Err(_) => runtime.submit_capability(device, kind, CapabilityRequest::None)?,
    };
    runtime.wait_completed(command.id, Duration::from_secs(1))
}
