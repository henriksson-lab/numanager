use numanager_core::runtime::{DiscoveryRegistry, LocalRuntime, Runtime};
use numanager_core::*;
use numanager_drivers::opentrons_ot2::OpentronsOt2Discovery;
use numanager_examples::{completion_summary, is_public_property, public_kind_summary};
use std::time::Duration;

pub fn run() -> numanager_core::Result<()> {
    let source = numanager_examples::example_arg(0).unwrap_or_else(|| "opentrons".into());
    if source != "opentrons" && source != "ot2" {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("unsupported robot inventory source {source}"),
        ));
    }

    let mut discovery = DiscoveryRegistry::new();
    discovery.register_factory(OpentronsOt2Discovery::configured_fixture);
    let mut candidates = discovery.detect_all()?;
    let candidate = candidates
        .pop()
        .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "no robot inventory candidates"))?;

    println!(
        "selected robot inventory source: {source} ({}, {} device(s), {} resource(s))",
        candidate.label(),
        candidate.devices().len(),
        candidate.resources().len()
    );

    for resource in candidate.resources() {
        println!(
            "resource: {} kind={} metadata={}",
            resource.label,
            resource.kind,
            completion_summary(&Value::Map(resource.metadata.clone()))
        );
    }

    let mut runtime = LocalRuntime::new();
    let added = runtime.add_candidate(candidate)?;
    println!(
        "added robot inventory driver with {} device(s)",
        added.len()
    );

    for device in added
        .iter()
        .filter(|device| !device.has_kind("hub"))
        .chain(added.iter().filter(|device| device.has_kind("hub")))
    {
        println!("device: {} [{}]", device.label, public_kind_summary(device));
        for property in device
            .properties
            .iter()
            .filter(|property| is_public_property(property))
        {
            let value = runtime.execute(
                Command::read_property(device.id, property.key.clone()),
                Duration::from_secs(1),
            )?;
            println!("  {}: {}", property.key, completion_summary(&value));
        }
    }

    Ok(())
}
