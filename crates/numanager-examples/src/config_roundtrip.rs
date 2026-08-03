use numanager_core::config::HardwareConfig;
use numanager_core::*;
use std::collections::BTreeMap;

pub fn run() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut builder = HardwareConfig::builder_from(100);
    let camera_transport = builder.add_resource(
        "Camera USB transport",
        "toupcam",
        BTreeMap::from([
            (
                "transfer_size".into(),
                Value::ByteCount(ByteCount::new(1_048_576)),
            ),
            (
                "frame_interval".into(),
                Value::TimeInterval(TimeInterval::from_milliseconds(20.0)),
            ),
        ]),
    );
    let camera = builder.add_device(
        "Main camera",
        "toupcam",
        BTreeMap::from([
            (
                "exposure".into(),
                Value::TimeInterval(TimeInterval::from_milliseconds(12.5)),
            ),
            ("gain".into(), Value::Ratio(Ratio::from_percent(120.0))),
            ("roi_width".into(), Value::PixelCount(PixelCount::new(1024))),
            ("roi_height".into(), Value::PixelCount(PixelCount::new(768))),
            ("pixel_format".into(), Value::String("Mono8".into())),
        ]),
    );
    let z = builder.add_device(
        "Z focus drive",
        "asi",
        BTreeMap::from([(
            "position".into(),
            Value::Position(Position::from_micrometers(2500.0)),
        )]),
    );
    builder
        .add_dependency(z, camera, Role::ZStage)
        .add_remux_group(
            "camera-and-focus-transport",
            [camera, z],
            Some(camera_transport),
        );
    let config = builder.build();

    let toml = config.to_toml();
    println!("serialized config:");
    print!("{toml}");

    let path = std::env::temp_dir().join("numanager-config-roundtrip.toml");
    config.save(&path)?;
    let loaded = HardwareConfig::load(&path)?;

    println!("loaded resources: {}", loaded.resources.len());
    for resource in &loaded.resources {
        let mut param_keys = resource.params.keys().cloned().collect::<Vec<_>>();
        param_keys.sort();
        println!(
            "resource {} driver={} params={:?}",
            resource.label, resource.driver, param_keys
        );
        for key in param_keys {
            println!("  param.{key}: {:?}", resource.params[&key]);
        }
    }

    println!("loaded devices: {}", loaded.devices.len());
    for device in &loaded.devices {
        let mut property_keys = device.properties.keys().cloned().collect::<Vec<_>>();
        property_keys.sort();
        println!(
            "device {} driver={} properties={:?}",
            device.label, device.driver, property_keys
        );
        for key in property_keys {
            println!("  property.{key}: {:?}", device.properties[&key]);
        }
    }

    println!("loaded remux groups: {}", loaded.remux_groups.len());
    println!("loaded dependencies: {}", loaded.dependencies.len());

    Ok(())
}
