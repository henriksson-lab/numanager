use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::{DriverDiscovery, LocalRuntime, Runtime};
use numanager_core::*;
use numanager_drivers::andor_camera::AndorCameraDiscovery;
use numanager_drivers::okolab::{OkolabConfiguredProbe, OkolabDriver};
use numanager_drivers::spark_cyto::SparkCytoDriver;
use numanager_examples::{
    capability_brief, completion_summary, device_by_kind,
    driver_capability_by_kind as capability_by_kind, event_summary, is_public_property,
    public_kind_summary, schema_state_write,
};
use std::time::Duration;

pub fn run() -> numanager_core::Result<()> {
    let source = numanager_examples::example_arg(0).unwrap_or_else(|| "spark_cyto".into());
    let driver = numanager_examples::boxed_driver(|id| environment_driver(&source, id))?;
    let devices = driver.descriptors();
    let temperature = device_by_kind(&devices, "environment.temperature")
        .or_else(|_| device_by_kind(&devices, "temperature.controller"))?;
    let gas = device_by_kind(&devices, "environment.gas").ok();
    let temperature_control =
        capability_by_kind(&*driver, temperature, CapabilityKind::TemperatureControl)?;
    let gas_control = gas
        .map(|gas| capability_by_kind(&*driver, gas, CapabilityKind::GasControl))
        .transpose()?;

    println!("selected environment family: {source}");
    println!(
        "selected temperature controller: {} [{}]",
        temperature.label,
        public_kind_summary(temperature)
    );
    match gas {
        Some(gas) => println!(
            "selected gas controller: {} [{}]",
            gas.label,
            public_kind_summary(gas)
        ),
        None => println!("selected gas controller: none"),
    }
    if let Some(gas_control) = &gas_control {
        println!(
            "capabilities: temperature={}; gas={}",
            capability_brief(&temperature_control),
            capability_brief(gas_control)
        );
    } else {
        println!(
            "capabilities: temperature={}; gas=none",
            capability_brief(&temperature_control)
        );
    }
    for property in temperature
        .properties
        .iter()
        .chain(gas.into_iter().flat_map(|gas| gas.properties.iter()))
        .filter(|property| is_public_property(property))
    {
        println!(
            "environment property: {} type={:?} writable={} sequenceable={}",
            property.key, property.value_type, property.writable, property.sequenceable
        );
    }

    let runtime = LocalRuntime::from_drivers(vec![driver]);
    let runtime_devices = runtime.devices().into_iter().cloned().collect::<Vec<_>>();
    let event_devices =
        gas.map_or_else(|| vec![temperature.id], |gas| vec![temperature.id, gas.id]);
    let events = runtime.subscribe(
        EventFilter::devices(event_devices)
            .with_kinds([EventKind::OperationChanged, EventKind::PropertyChanged]),
    );

    let mut setup_writes = Vec::new();
    push_environment_write(
        temperature,
        &mut setup_writes,
        "target",
        Value::Temperature(environment_target(temperature)),
    );
    push_environment_write(temperature, &mut setup_writes, "enabled", Value::Bool(true));
    push_environment_write(
        temperature,
        &mut setup_writes,
        "sensor_cooling",
        Value::Bool(true),
    );
    push_environment_write(
        temperature,
        &mut setup_writes,
        "temperature_control",
        Value::String(format!("{:.0}", environment_target(temperature).celsius())),
    );
    if let Some(gas) = gas {
        push_environment_write(
            gas,
            &mut setup_writes,
            "co2_target",
            Value::GasConcentration(GasConcentration::from_percent(5.0)),
        );
        push_environment_write(gas, &mut setup_writes, "enabled", Value::Bool(true));
    }
    if setup_writes.is_empty() {
        println!("environment state set skipped: no generic setup properties");
    } else {
        let setup = runtime.submit(
            StateSet::immediate("environment setup")
                .with_writes(setup_writes)
                .into_command(),
        )?;
        let value = runtime.wait_completed(setup.id, Duration::from_secs(1))?;
        println!(
            "environment state set completed: {}",
            completion_summary(&value)
        );
    }

    let value = runtime.execute_request(
        temperature,
        TemperatureControlRequest {
            target: Some(environment_target(temperature)),
            enabled: Some(true),
        },
        Duration::from_secs(1),
    )?;
    println!(
        "temperature control completed: {}",
        completion_summary(&value)
    );

    if let Some(gas) = gas {
        let value = runtime.execute_request(
            gas,
            GasControlRequest {
                co2_target: Some(GasConcentration::from_percent(4.5)),
                enabled: Some(true),
            },
            Duration::from_secs(1),
        )?;
        println!("gas control completed: {}", completion_summary(&value));
    } else {
        println!("gas control skipped: none");
    }

    let temperature_safety = runtime.safety_summary(temperature, Duration::from_secs(1))?;
    println!(
        "temperature safety: {} {}",
        temperature_safety.state.name(),
        completion_summary(&temperature_safety.as_value())
    );
    if let Some(gas) = gas {
        let gas_safety = runtime.safety_summary(gas, Duration::from_secs(1))?;
        println!(
            "gas safety: {} {}",
            gas_safety.state.name(),
            completion_summary(&gas_safety.as_value())
        );
    }

    for (device, key) in environment_readback_keys(temperature, gas) {
        if device.properties.iter().any(|property| property.key == key) {
            match runtime.execute(Command::read_property(device, key), Duration::from_secs(1)) {
                Ok(value) => println!("{key}: {}", completion_summary(&value)),
                Err(error) if optional_environment_readback(key) => {
                    println!("{key}: skipped ({})", error.message)
                }
                Err(error) => return Err(error),
            }
        }
    }

    while let Some(event) = events.recv_timeout(Duration::from_millis(50)) {
        println!("event: {}", event_summary(&runtime_devices, &event));
    }

    Ok(())
}

fn environment_driver(source: &str, id: DriverId) -> numanager_core::Result<Box<dyn Driver>> {
    match source {
        "andor_sdk2" | "andor-sdk2" => andor_environment_driver(id, "andor_sdk2", 0x0012),
        "andor_sdk3" | "andor-sdk3" => andor_environment_driver(id, "andor_sdk3", 0x0014),
        "spark_cyto" | "spark-cyto" | "spark" => Ok(Box::new(SparkCytoDriver::simulated(id))),
        "okolab" | "oko-lab" | "oko_lab" => Ok(Box::new(OkolabDriver::configured(
            id,
            OkolabConfiguredProbe::fixture(),
        ))),
        other => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!(
                "unknown environment family {other:?}; use one of: andor_sdk2, andor_sdk3, spark_cyto, okolab"
            ),
        )),
    }
}

fn andor_environment_driver(
    id: DriverId,
    driver_name: &'static str,
    product_id: i64,
) -> numanager_core::Result<Box<dyn Driver>> {
    let config = HardwareConfig {
        devices: vec![DeviceConfig::new(
            id.0 * 1000 + 1,
            format!(
                "Configured {}",
                driver_name.replace('_', " ").to_uppercase()
            ),
            driver_name,
            std::collections::BTreeMap::from([
                ("product_id".into(), Value::I64(product_id)),
                ("sensor_cooling".into(), Value::Bool(false)),
                ("temperature_control".into(), Value::String("-20".into())),
            ]),
        )],
        ..Default::default()
    };
    let mut discovery = AndorCameraDiscovery::from_config(id, &config)?;
    let mut candidates = discovery.detect()?;
    candidates
        .pop()
        .map(|candidate| candidate.into_driver())
        .ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidCommand,
                "missing Andor environment driver",
            )
        })
}

fn push_environment_write(
    device: &DeviceDescriptor,
    writes: &mut Vec<StateWrite>,
    key: &str,
    value: Value,
) {
    if let Some(write) = schema_state_write(device, key, value) {
        writes.push(write);
    }
}

fn environment_target(device: &DeviceDescriptor) -> Temperature {
    if device
        .properties
        .iter()
        .any(|property| property.key == "temperature_control")
    {
        Temperature::from_celsius(-20.0)
    } else {
        Temperature::from_celsius(36.5)
    }
}

fn environment_readback_keys<'a>(
    temperature: &'a DeviceDescriptor,
    gas: Option<&'a DeviceDescriptor>,
) -> Vec<(&'a DeviceDescriptor, &'static str)> {
    let mut keys = vec![
        (temperature, "target"),
        (temperature, "temperature_control"),
        (temperature, "enabled"),
        (temperature, "sensor_cooling"),
        (temperature, "sensor_temperature"),
        (temperature, "temperature_status"),
    ];
    if let Some(gas) = gas {
        let gas_state_key = if gas
            .properties
            .iter()
            .any(|property| property.key == "fault")
        {
            "fault"
        } else {
            "status"
        };
        keys.extend([
            (gas, "co2_target"),
            (gas, "co2_actual"),
            (gas, "enabled"),
            (gas, gas_state_key),
        ]);
    }
    keys
}

fn optional_environment_readback(key: &str) -> bool {
    matches!(key, "sensor_temperature" | "temperature_status")
}
