use numanager_core::runtime::{LocalRuntime, Runtime};
use numanager_core::*;
use numanager_drivers::asi::AsiTigerDriver;
use numanager_drivers::sim::SimComposedAutofocusDriver;
use numanager_drivers::squid::SquidDriver;
use numanager_drivers::sutter_stage::SutterStageDriver;
use numanager_examples::{capability_brief, completion_summary};
use std::time::Duration;

pub fn run() -> numanager_core::Result<()> {
    let drivers: Vec<Box<dyn Driver>> = vec![
        Box::new(numanager_examples::driver_value(SquidDriver::simulated)),
        Box::new(numanager_examples::driver_value(AsiTigerDriver::simulated)),
        Box::new(numanager_examples::driver_value(
            SutterStageDriver::simulated,
        )),
        Box::new(numanager_examples::driver_value(
            SimComposedAutofocusDriver::simulated,
        )),
    ];

    let providers = capability_providers(
        drivers.iter().map(|driver| driver.as_ref()),
        CapabilityKind::Autofocus,
    );
    if providers.is_empty() {
        return Err(Error::new(
            ErrorCode::Unsupported,
            "no autofocus providers were discovered",
        ));
    }
    println!("generic autofocus providers:");
    for provider in &providers {
        println!(
            "  device={} capability={}",
            provider.device.label,
            capability_brief(&provider.capability)
        );
        for dependency in &provider.dependencies {
            println!(
                "    depends on {} as {:?}",
                dependency.label, dependency.role
            );
        }
    }

    let runtime = LocalRuntime::from_drivers(drivers);
    for provider in &providers {
        let op = runtime.submit_request(
            provider.device.id,
            AutofocusRequest {
                mode: AutofocusMode::Hold,
                range: None,
            },
        )?;
        let value = runtime.wait_completed(op.id, Duration::from_secs(1))?;
        println!(
            "{} autofocus hold completed: {}",
            provider.device.label,
            completion_summary(&value)
        );
    }

    let composed = providers
        .iter()
        .find(|provider| {
            provider.has_dependency_devices(&[Role::Camera, Role::ZStage, Role::LightSource])
        })
        .ok_or_else(|| {
            Error::new(
                ErrorCode::Unsupported,
                "no composed camera/Z/light autofocus provider found",
            )
        })?;
    let sim_camera = dependency_device(composed, Role::Camera)?;
    let sim_z = dependency_device(composed, Role::ZStage)?;
    let sim_light = dependency_device(composed, Role::LightSource)?;
    let sim_autofocus = &composed.device;
    let armed = runtime.submit(
        TimingPlan::builder()
            .sequence(
                sim_camera,
                "exposure",
                [
                    Value::TimeInterval(TimeInterval::from_milliseconds(15.0)),
                    Value::TimeInterval(TimeInterval::from_milliseconds(5.0)),
                ],
            )
            .sequence(
                sim_z,
                "z",
                [
                    Value::Position(Position::from_micrometers(4_000.0)),
                    Value::Position(Position::from_micrometers(4_250.0)),
                ],
            )
            .sequence(
                sim_light,
                "enabled",
                [Value::Bool(true), Value::Bool(false)],
            )
            .sequence(
                sim_light,
                "power",
                [
                    Value::Ratio(Ratio::from_percent(40.0)),
                    Value::Ratio(Ratio::from_percent(10.0)),
                ],
            )
            .sequence(
                sim_autofocus,
                "mode",
                [Value::String("hold".into()), Value::String("stop".into())],
            )
            .arm_order([sim_light, sim_z, sim_camera, sim_autofocus])
            .into_command()?,
    )?;
    let value = runtime.wait_completed(armed.id, Duration::from_secs(1))?;
    println!(
        "composed autofocus timing arm: {}",
        completion_summary(&value)
    );
    let started = runtime.submit(Command::start(armed.id))?;
    let value = runtime.wait_completed(started.id, Duration::from_secs(1))?;
    println!(
        "composed autofocus timing start: {}",
        completion_summary(&value)
    );
    let stopped = runtime.submit(Command::stop(armed.id))?;
    let value = runtime.wait_completed(stopped.id, Duration::from_secs(1))?;
    println!(
        "composed autofocus timing stop: {}",
        completion_summary(&value)
    );

    Ok(())
}

fn dependency_device<'a>(
    provider: &'a CapabilityProvider,
    role: Role,
) -> numanager_core::Result<&'a DeviceDescriptor> {
    provider.dependency_device(&role).ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidGraph,
            format!(
                "{} autofocus provider is missing an advertised {role:?} dependency device",
                provider.device.label
            ),
        )
    })
}
