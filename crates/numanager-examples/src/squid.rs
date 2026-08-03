use numanager_core::runtime::{DiscoveryRegistry, LocalRuntime, Runtime};
use numanager_core::*;
use numanager_drivers::squid::{SquidDiscovery, SquidDriver};
use numanager_examples::{
    capability_brief, capability_descriptor_by_kind as capability_by_kind, completion_summary,
    driver_device_by_capability as device_by_capability, event_summary, operation_status_summary,
    public_capability_summary, public_kind_tags,
};
use std::fs;
use std::time::Duration;

pub fn run() -> numanager_core::Result<()> {
    let driver = numanager_examples::driver_value(SquidDriver::simulated);
    let devices = driver.descriptors();

    println!("squid devices:");
    for device in &devices {
        let caps = public_capability_summary(driver.capabilities(device.id));
        println!(
            "  {} {:?} typed caps=[{}]",
            device.label,
            public_kind_tags(device),
            caps
        );
    }

    let graph = driver.graph();
    let graph_order = graph.initialization_order()?;
    println!("initialization order has {} graph nodes", graph_order.len());
    println!("device dependencies:");
    for edge in graph.edges() {
        if let EdgeKind::UsesDevice { role } = &edge.kind {
            println!(
                "  {} -> {} as {:?}",
                graph_label(&graph, edge.from),
                graph_label(&graph, edge.to),
                role
            );
        }
    }

    let xy = device_by_label(&devices, "squid-xy-stage")?;
    let z = device_by_label(&devices, "squid-z-stage")?;
    let d1 = device_by_label(&devices, "squid-illumination-d1")?;
    let trigger = device_by_label(&devices, "squid-trigger-1")?;
    let autofocus = device_by_capability(&driver, &devices, CapabilityKind::Autofocus)?;
    let xy_move = capability_by_kind(&driver.capabilities(xy.id), CapabilityKind::StageMove)?;
    let xy_home = capability_by_kind(&driver.capabilities(xy.id), CapabilityKind::StageHome)?;
    let z_move = capability_by_kind(&driver.capabilities(z.id), CapabilityKind::StageMove)?;
    let d1_dac = capability_by_kind(&driver.capabilities(d1.id), CapabilityKind::Dac)?;
    let trigger_cap = capability_by_kind(
        &driver.capabilities(trigger.id),
        CapabilityKind::TriggerSource,
    )?;
    let autofocus_cap = capability_by_kind(
        &driver.capabilities(autofocus.id),
        CapabilityKind::Autofocus,
    )?;
    println!(
        "typed capabilities: xy move={} home={}; z move={}; d1 dac={}; trigger={}; autofocus={}",
        capability_brief(&xy_move),
        capability_brief(&xy_home),
        capability_brief(&z_move),
        capability_brief(&d1_dac),
        capability_brief(&trigger_cap),
        capability_brief(&autofocus_cap)
    );

    let runtime = LocalRuntime::from_drivers(vec![Box::new(driver)]);
    let events = runtime.subscribe(EventFilter::all());

    runtime.execute(
        Command::write_property(
            controller_id(&devices)?,
            "watchdog_timeout",
            Value::TimeInterval(TimeInterval::from_seconds(10.0)),
        ),
        Duration::from_secs(1),
    )?;

    let op = runtime.submit(Command::write_property(
        &xy,
        "x",
        Value::Position(Position::from_micrometers(1250.0)),
    ))?;
    let status = runtime.wait(op.id, Duration::from_secs(1))?;
    println!(
        "x move completed from firmware status: {}",
        operation_status_summary(&status)
    );

    let xy_move_op = runtime.submit_request(
        &xy,
        StageMoveRequest {
            target: [
                (StageAxis::X, Position::from_micrometers(1_500.0)),
                (StageAxis::Y, Position::from_micrometers(2_000.0)),
            ]
            .into_iter()
            .collect(),
            relative: false,
            profile: Some(MotionProfile {
                velocity: Some(Velocity::from_micrometers_per_second(4_000.0)),
                acceleration: Some(Acceleration::from_micrometers_per_second_squared(20_000.0)),
            }),
        },
    )?;
    let value = runtime.wait_completed(xy_move_op.id, Duration::from_secs(1))?;
    println!("typed XY move completed: {}", completion_summary(&value));

    runtime.execute(
        Command::write_property(&z, "z", Value::Position(Position::from_micrometers(80.0))),
        Duration::from_secs(1),
    )?;

    let z_move_op = runtime.submit_request(
        &z,
        StageMoveRequest::relative([(StageAxis::Z, Position::from_micrometers(20.0))]),
    )?;
    let value = runtime.wait_completed(z_move_op.id, Duration::from_secs(1))?;
    println!(
        "typed Z relative move completed: {}",
        completion_summary(&value)
    );

    let state = StateSet::prepare_then_commit("d1-on").with_writes(numanager_core::state_writes!(
        &d1 => {
            "intensity" => Value::Ratio(Ratio::from_percent(12.5)),
            "enabled" => Value::Bool(true),
        }
    ));
    let value = runtime.execute(state.into_command(), Duration::from_secs(1))?;
    println!("illumination state remuxed: {}", completion_summary(&value));

    let d1_dac_op = runtime.submit_request(
        &d1,
        DacRequest {
            value: Value::Ratio(Ratio::from_percent(33.0)),
        },
    )?;
    let value = runtime.wait_completed(d1_dac_op.id, Duration::from_secs(1))?;
    println!(
        "typed illumination DAC completed: {}",
        completion_summary(&value)
    );

    let value = runtime.execute_capability(
        &trigger,
        CapabilityKind::TriggerSource,
        CapabilityRequest::Trigger(TriggerRequest {
            action: TriggerAction::Pulse,
            duration: Some(TimeInterval::from_microseconds(5_000.0)),
            control_illumination: Some(true),
        }),
        Duration::from_secs(1),
    )?;
    println!("trigger pulse completed: {}", completion_summary(&value));

    let value = runtime.execute_request(
        &autofocus,
        AutofocusRequest {
            mode: AutofocusMode::Hold,
            range: None,
        },
        Duration::from_secs(1),
    )?;
    println!("autofocus hold completed: {}", completion_summary(&value));

    let xy_home_op =
        runtime.submit_capability(&xy, CapabilityKind::StageHome, CapabilityRequest::None)?;
    let value = runtime.wait_completed(xy_home_op.id, Duration::from_secs(1))?;
    println!("typed XY home completed: {}", completion_summary(&value));

    runtime.execute(
        Command::write_property(&autofocus, "enabled", Value::Bool(false)),
        Duration::from_secs(1),
    )?;

    let armed = runtime.submit(
        TimingPlan::builder()
            .sequence(
                &xy,
                "x",
                [
                    Value::Position(Position::from_micrometers(1_500.0)),
                    Value::Position(Position::from_micrometers(1_750.0)),
                ],
            )
            .sequence(
                &z,
                "z",
                [
                    Value::Position(Position::from_micrometers(90.0)),
                    Value::Position(Position::from_micrometers(110.0)),
                ],
            )
            .sequence(
                &d1,
                "intensity",
                [
                    Value::Ratio(Ratio::from_percent(25.0)),
                    Value::Ratio(Ratio::from_percent(5.0)),
                ],
            )
            .sequence(&d1, "enabled", [Value::Bool(true), Value::Bool(false)])
            .sequence(
                &autofocus,
                "enabled",
                [Value::Bool(true), Value::Bool(false)],
            )
            .arm_order([&xy, &z, &d1, &autofocus, &trigger])
            .into_command()?,
    )?;
    let value = runtime.wait_completed(armed.id, Duration::from_secs(1))?;
    println!("timing arm completion: {}", completion_summary(&value));
    let started = runtime.submit(Command::start(armed.id))?;
    let value = runtime.wait_completed(started.id, Duration::from_secs(1))?;
    println!("timing start completion: {}", completion_summary(&value));
    let stopped = runtime.submit(Command::stop(armed.id))?;
    let value = runtime.wait_completed(stopped.id, Duration::from_secs(1))?;
    println!("timing stop completion: {}", completion_summary(&value));

    let x = runtime.execute(Command::read_property(&xy, "x"), Duration::from_secs(1))?;
    println!("x now {:?}", x);

    for _ in 0..4 {
        if let Some(event) = events.recv_timeout(Duration::from_millis(100)) {
            println!("event: {}", event_summary(&devices, &event));
        }
    }

    let config_path = std::env::temp_dir().join("numanager-squid-example.toml");
    fs::write(&config_path, SQUID_CONFIG)
        .map_err(|error| Error::new(ErrorCode::Driver, error.to_string()))?;
    let config = numanager_core::config::HardwareConfig::load(&config_path)
        .map_err(|error| Error::new(ErrorCode::Driver, format!("{error:?}")))?;
    let mut discovery = DiscoveryRegistry::new();
    discovery.register_factory_result(|id| SquidDiscovery::from_config(id, &config))?;
    let candidates = discovery.detect_all()?;
    println!(
        "configured discovery found {} Squid candidate(s) from {}",
        candidates.len(),
        config_path.display()
    );

    Ok(())
}

const SQUID_CONFIG: &str = r#"
[[devices]]
id = 40100
label = "configured-squid"
driver = "squid"
property.connect = false
"#;

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

fn graph_label(graph: &DeviceGraph, id: NodeId) -> String {
    graph
        .nodes()
        .find(|node| node.id == id)
        .map(|node| node.label.clone())
        .unwrap_or_else(|| format!("{id:?}"))
}

fn controller_id(devices: &[DeviceDescriptor]) -> numanager_core::Result<DeviceId> {
    Ok(device_by_label(devices, "squid-controller")?.id)
}
