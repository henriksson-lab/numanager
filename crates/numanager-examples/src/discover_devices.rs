use numanager_core::config::DiscoveryLock;
use numanager_core::runtime::{DiscoveryRegistry, LocalRuntime};
use numanager_core::*;
use numanager_drivers::{builtin_demo_hardware_config, register_builtin_demo_discovery};
use numanager_examples::{completion_summary, public_kind_tags};

pub fn run() -> numanager_core::Result<()> {
    let discovery_config = builtin_demo_hardware_config();
    let mut registry = DiscoveryRegistry::new();
    register_builtin_demo_discovery(&mut registry, &discovery_config)?;
    let candidates = registry.detect_all()?;

    println!("detected {} candidate driver(s)", candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        println!(
            "{index}: {}, {} device(s), {} resource(s)",
            candidate.label(),
            candidate.devices().len(),
            candidate.resources().len()
        );
        for device in candidate.devices() {
            println!(
                "    device: {} {:?}",
                device.label,
                public_kind_tags(device)
            );
        }
    }

    let lock = DiscoveryLock {
        entries: candidates
            .iter()
            .map(|candidate| candidate.to_discovery_entry())
            .collect(),
    };
    let lock_path = std::env::temp_dir().join("numanager-discovery-lock.toml");
    lock.save(&lock_path)
        .map_err(|error| Error::new(ErrorCode::Driver, format!("{error:?}")))?;
    let loaded_lock = DiscoveryLock::load(&lock_path)
        .map_err(|error| Error::new(ErrorCode::Driver, format!("{error:?}")))?;
    println!(
        "saved discovery lock with {} persistent entrie(s) to {}",
        loaded_lock.entries.len(),
        lock_path.display()
    );
    if let Some(entry) = loaded_lock.entries.first() {
        println!(
            "first lock entry: id={:?} label={} aliases={} metadata={}",
            entry.persistent_id,
            entry.label,
            entry.aliases.len(),
            completion_summary(&Value::Map(entry.metadata.clone()))
        );
    }
    if let Some(entry) = loaded_lock
        .entries
        .iter()
        .find(|entry| entry.label == "Configured Mightex Sirius SLC")
    {
        println!(
            "mightex slc lock entry: id={:?} aliases={} metadata={}",
            entry.persistent_id,
            entry.aliases.len(),
            completion_summary(&Value::Map(entry.metadata.clone()))
        );
        if let Some(support_level) = entry.metadata.get("support_level") {
            println!(
                "mightex slc support level: {}",
                completion_summary(support_level)
            );
        }
    }

    let mut runtime = LocalRuntime::new();

    // This is the second stage. A UI or config file would decide which detected
    // candidates to claim; this example adds all local fixture candidates.
    for candidate in candidates {
        let label = candidate.label().to_string();
        let devices = runtime.add_candidate(candidate)?;
        println!("added {label} with {} device(s)", devices.len());
    }

    Ok(())
}
