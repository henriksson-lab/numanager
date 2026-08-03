use numanager_core::runtime::{DiscoveryRegistry, DriverDiscovery, LocalRuntime, Runtime};
use numanager_core::*;
use numanager_drivers::asi::{AsiMs2000Discovery, AsiMs2000Driver};
use numanager_drivers::chuo_seiki_qt::{ChuoQtConfiguredProbe, ChuoQtDiscovery, ChuoQtDriver};
use numanager_drivers::corvus::{CorvusConfiguredProbe, CorvusDiscovery, CorvusDriver};
use numanager_drivers::esp32::{Esp32Discovery, Esp32Driver};
use numanager_drivers::marzhauser::{
    MarzhauserConfiguredProbe, MarzhauserDiscovery, MarzhauserDriver,
};
use numanager_drivers::openstage::{OpenStageConfiguredProbe, OpenStageDiscovery, OpenStageDriver};
use numanager_drivers::openuc2::{OpenUc2Discovery, OpenUc2Driver};
use numanager_drivers::pi_gcs::{PiGcsConfiguredProbe, PiGcsDiscovery, PiGcsDriver};
use numanager_drivers::prior::{PriorDiscovery, PriorDriver};
use numanager_drivers::standa::{StandaConfiguredProbe, StandaDiscovery, StandaDriver};
use numanager_drivers::sutter_mp285::{Mp285ConfiguredProbe, Mp285Discovery, Mp285Driver};
use numanager_drivers::sutter_stage::{
    SutterStageConfiguredProbe, SutterStageDiscovery, SutterStageDriver,
};
use numanager_drivers::thorlabs_apt::{
    ThorlabsAptConfiguredProbe, ThorlabsAptDiscovery, ThorlabsAptDriver,
};
use numanager_drivers::triggerscope::{
    TriggerScopeConfiguredProbe, TriggerScopeDiscovery, TriggerScopeDriver,
};
use numanager_drivers::trinamic_tmcl::{TmclConfiguredProbe, TmclDiscovery, TmclDriver};
use numanager_drivers::wosm::{WosmConfiguredProbe, WosmDiscovery, WosmDriver};
use numanager_drivers::zaber::{ZaberAsciiDiscovery, ZaberAsciiDriver};
use numanager_examples::{
    capability_brief, completion_summary, event_summary, property, public_kind_summary,
    schema_state_write,
};
use std::time::Duration;

pub fn run() -> numanager_core::Result<()> {
    let source = numanager_examples::example_arg(0).unwrap_or_else(|| "asi".into());
    let mut discovery = DiscoveryRegistry::new();
    discovery.register_boxed_factory_result(|id| motion_discovery(&source, id))?;
    let candidates = discovery.detect_all()?;
    println!("detected {} motion candidate(s)", candidates.len());
    for candidate in &candidates {
        println!("candidate: {}", candidate.label());
        for device in candidate.devices() {
            println!("  {} [{}]", device.label, public_kind_summary(device));
        }
    }

    let mut runtime = LocalRuntime::new();
    let driver = numanager_examples::boxed_driver(|id| motion_driver(&source, id))?;
    let added_devices = runtime.add_driver(driver)?;
    let stages = added_devices
        .iter()
        .filter(|device| {
            runtime
                .capability_by_kind(device.id, CapabilityKind::StageMove)
                .is_ok()
        })
        .filter_map(StageSelection::from_device)
        .collect::<Vec<_>>();
    if stages.is_empty() {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            "selected source exposes no movable stage devices",
        ));
    }

    println!("source: {source}");
    println!("selected {} stage device(s)", stages.len());
    for stage in &stages {
        let move_capability =
            runtime.capability_by_kind(&stage.device, CapabilityKind::StageMove)?;
        let home = runtime
            .capability_by_kind(&stage.device, CapabilityKind::StageHome)
            .ok()
            .map(|capability| capability_brief(&capability))
            .unwrap_or_else(|| "none".into());
        let stop = runtime
            .capability_by_kind(&stage.device, CapabilityKind::StageStop)
            .ok()
            .map(|capability| capability_brief(&capability))
            .unwrap_or_else(|| "none".into());
        println!(
            "selected stage: {} [{}] axes={} move={} home={} stop={}",
            stage.device.label,
            public_kind_summary(&stage.device),
            stage.axis_summary(),
            capability_brief(&move_capability),
            home,
            stop
        );
    }

    let event_devices = stages
        .iter()
        .map(|stage| stage.device.id)
        .collect::<Vec<_>>();
    let events = runtime.subscribe(
        EventFilter::devices(event_devices)
            .with_kinds([EventKind::OperationChanged, EventKind::PropertyChanged]),
    );

    let mut setup_writes = Vec::new();
    for stage in &stages {
        for axis in &stage.axes {
            if let Some(write) = schema_state_write(
                &stage.device,
                axis.property,
                Value::Position(axis.initial_position),
            ) {
                setup_writes.push(write);
            }
        }
    }
    let setup = runtime.submit(
        StateSet::immediate("initial stage positions")
            .with_writes(setup_writes)
            .into_command(),
    )?;
    let value = runtime.wait_completed(setup.id, Duration::from_secs(1))?;
    println!("state set completed: {}", completion_summary(&value));

    for stage in &stages {
        let move_command = runtime.submit_request(
            &stage.device,
            StageMoveRequest {
                target: stage.absolute_targets(),
                relative: false,
                profile: None,
            },
        )?;
        let value = runtime.wait_completed(move_command.id, Duration::from_secs(1))?;
        println!(
            "move completed for {}: {}",
            stage.device.label,
            completion_summary(&value)
        );
    }

    let mut timing = TimingPlan::builder();
    for stage in &stages {
        for axis in &stage.axes {
            timing = timing.sequence(
                &stage.device,
                axis.property,
                [
                    Value::Position(axis.sequence[0]),
                    Value::Position(axis.sequence[1]),
                    Value::Position(axis.sequence[2]),
                ],
            );
        }
    }
    let armed = runtime.submit(
        timing
            .arm_order(stages.iter().map(|stage| &stage.device))
            .stop(StopCondition::Count(3))
            .into_command()?,
    )?;
    let value = runtime.wait_completed(armed.id, Duration::from_secs(1))?;
    println!("timing arm completed: {}", completion_summary(&value));
    let started = runtime.submit(Command::start(armed.id))?;
    let value = runtime.wait_completed(started.id, Duration::from_secs(1))?;
    println!("timing start completed: {}", completion_summary(&value));
    let stopped_plan = runtime.submit(Command::stop(armed.id))?;
    let value = runtime.wait_completed(stopped_plan.id, Duration::from_secs(1))?;
    println!("timing stop completed: {}", completion_summary(&value));

    for stage in &stages {
        for axis in &stage.axes {
            let value = runtime.execute(
                Command::read_property(&stage.device, axis.property),
                Duration::from_secs(1),
            )?;
            println!(
                "{} {}: {}",
                stage.device.label,
                axis.property,
                completion_summary(&value)
            );
        }
    }

    for stage in &stages {
        if runtime
            .capability_by_kind(&stage.device, CapabilityKind::StageStop)
            .is_ok()
        {
            let stop = runtime.submit_capability(
                &stage.device,
                CapabilityKind::StageStop,
                CapabilityRequest::None,
            )?;
            let value = runtime.wait_completed(stop.id, Duration::from_secs(1))?;
            println!(
                "stop completed for {}: {}",
                stage.device.label,
                completion_summary(&value)
            );
        }
    }

    for stage in &stages {
        if runtime
            .capability_by_kind(&stage.device, CapabilityKind::StageHome)
            .is_ok()
        {
            let home = runtime.submit_capability(
                &stage.device,
                CapabilityKind::StageHome,
                CapabilityRequest::None,
            )?;
            let value = runtime.wait_completed(home.id, Duration::from_secs(1))?;
            println!(
                "home completed for {}: {}",
                stage.device.label,
                completion_summary(&value)
            );
        }
    }

    while let Some(event) = events.recv_timeout(Duration::from_millis(50)) {
        println!("event: {}", event_summary(&added_devices, &event));
    }

    Ok(())
}

#[derive(Clone)]
struct StageSelection {
    device: DeviceDescriptor,
    axes: Vec<StageAxisSelection>,
}

impl StageSelection {
    fn from_device(device: &DeviceDescriptor) -> Option<Self> {
        let mut axes = Vec::new();
        for (axis, key, initial, absolute, sequence) in [
            (StageAxis::X, "x", 10.0, 50.0, [50.0, 60.0, 70.0]),
            (StageAxis::Y, "y", 15.0, 25.0, [25.0, 30.0, 35.0]),
            (StageAxis::Z, "z", 20.0, 40.0, [40.0, 50.0, 60.0]),
        ] {
            if property(device, key).is_some() {
                axes.push(StageAxisSelection {
                    request_axis: axis,
                    property: key,
                    initial_position: Position::from_micrometers(initial),
                    absolute_position: Position::from_micrometers(absolute),
                    sequence: sequence.map(Position::from_micrometers),
                });
            }
        }
        if property(device, "position").is_some() {
            let axis = descriptor_axis(device).unwrap_or(StageAxis::X);
            axes.push(StageAxisSelection {
                request_axis: axis,
                property: "position",
                initial_position: Position::from_micrometers(100.0),
                absolute_position: Position::from_micrometers(250.0),
                sequence: [250.0, 300.0, 350.0].map(Position::from_micrometers),
            });
        }
        if axes.is_empty() {
            None
        } else {
            Some(Self {
                device: device.clone(),
                axes,
            })
        }
    }

    fn axis_summary(&self) -> String {
        self.axes
            .iter()
            .map(|axis| axis.property)
            .collect::<Vec<_>>()
            .join(",")
    }

    fn absolute_targets(&self) -> std::collections::BTreeMap<StageAxis, Position> {
        self.axes
            .iter()
            .map(|axis| (axis.request_axis.clone(), axis.absolute_position))
            .collect()
    }
}

#[derive(Clone)]
struct StageAxisSelection {
    request_axis: StageAxis,
    property: &'static str,
    initial_position: Position,
    absolute_position: Position,
    sequence: [Position; 3],
}

fn descriptor_axis(device: &DeviceDescriptor) -> Option<StageAxis> {
    for kind in &device.kinds {
        match kind.as_str() {
            "axis.x" | "stage.x" => return Some(StageAxis::X),
            "axis.y" | "stage.y" => return Some(StageAxis::Y),
            "axis.z" | "stage.z" => return Some(StageAxis::Z),
            _ => {}
        }
    }
    if let Some(Value::String(axis)) = device.metadata.get("axis") {
        match axis.as_str() {
            "x" | "X" => return Some(StageAxis::X),
            "y" | "Y" => return Some(StageAxis::Y),
            "z" | "Z" => return Some(StageAxis::Z),
            _ => {}
        }
    }
    None
}

fn motion_discovery(
    source: &str,
    id: DriverId,
) -> numanager_core::Result<Box<dyn DriverDiscovery>> {
    match source {
        "asi" | "ms2000" => Ok(Box::new(AsiMs2000Discovery::simulated(id))),
        "chuo" | "chuo_seiki_qt" => Ok(Box::new(ChuoQtDiscovery::configured_fixture(id))),
        "corvus" | "itk_corvus" => Ok(Box::new(CorvusDiscovery::configured_fixture(id))),
        "esp32" => Ok(Box::new(Esp32Discovery::simulated(id))),
        "marzhauser" | "tango" | "lstep" => Ok(Box::new(MarzhauserDiscovery::simulated(id))),
        "openstage" => Ok(Box::new(OpenStageDiscovery::configured_fixture(id))),
        "openuc2" | "uc2" => Ok(Box::new(OpenUc2Discovery::simulated(id))),
        "pi" | "pi-gcs" | "gcs" => Ok(Box::new(PiGcsDiscovery::configured_fixture(id))),
        "prior" | "proscan" => Ok(Box::new(PriorDiscovery::simulated(id))),
        "standa" | "8smc4" => Ok(Box::new(StandaDiscovery::simulated(id))),
        "sutter-mp285" | "mp285" => Ok(Box::new(Mp285Discovery::simulated(id))),
        "sutter-stage" | "sutter" => Ok(Box::new(SutterStageDiscovery::simulated(id))),
        "thorlabs-apt" | "apt" => Ok(Box::new(ThorlabsAptDiscovery::configured_fixture(id))),
        "trinamic-tmcl" | "tmcl" => Ok(Box::new(TmclDiscovery::configured_fixture(id))),
        "triggerscope" => Ok(Box::new(TriggerScopeDiscovery::configured_fixture(id))),
        "wosm" | "warwick-wosm" | "warwick_wosm" => {
            Ok(Box::new(WosmDiscovery::configured_fixture(id)))
        }
        "zaber" | "zaber-ascii" => Ok(Box::new(ZaberAsciiDiscovery::simulated(id))),
        other => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("unknown motion source {other}; expected one of: asi, chuo, corvus, esp32, marzhauser, openstage, openuc2, pi-gcs, prior, standa, sutter-mp285, sutter-stage, thorlabs-apt, trinamic-tmcl, triggerscope, wosm, zaber"),
        )),
    }
}

fn motion_driver(source: &str, id: DriverId) -> numanager_core::Result<Box<dyn Driver>> {
    match source {
        "asi" | "ms2000" => Ok(Box::new(AsiMs2000Driver::simulated(id))),
        "chuo" | "chuo_seiki_qt" => Ok(Box::new(ChuoQtDriver::configured(
            id,
            ChuoQtConfiguredProbe::fixture(),
        ))),
        "corvus" | "itk_corvus" => Ok(Box::new(CorvusDriver::configured(
            id,
            CorvusConfiguredProbe::fixture(),
        ))),
        "esp32" => Ok(Box::new(Esp32Driver::simulated(id))),
        "marzhauser" | "tango" | "lstep" => Ok(Box::new(MarzhauserDriver::configured_fixture(
            id,
            MarzhauserConfiguredProbe::simulated(),
        ))),
        "openstage" => Ok(Box::new(OpenStageDriver::configured(
            id,
            OpenStageConfiguredProbe::fixture(),
        ))),
        "openuc2" | "uc2" => Ok(Box::new(OpenUc2Driver::simulated(id))),
        "pi" | "pi-gcs" | "gcs" => Ok(Box::new(PiGcsDriver::configured(
            id,
            PiGcsConfiguredProbe::fixture(),
        ))),
        "prior" | "proscan" => Ok(Box::new(PriorDriver::simulated(id))),
        "standa" | "8smc4" => Ok(Box::new(StandaDriver::configured(
            id,
            StandaConfiguredProbe::simulated(),
        ))),
        "sutter-mp285" | "mp285" => Ok(Box::new(Mp285Driver::configured(
            id,
            Mp285ConfiguredProbe::simulated(),
        ))),
        "sutter-stage" | "sutter" => Ok(Box::new(SutterStageDriver::configured(
            id,
            SutterStageConfiguredProbe::simulated(),
        ))),
        "thorlabs-apt" | "apt" => Ok(Box::new(ThorlabsAptDriver::configured(
            id,
            ThorlabsAptConfiguredProbe::fixture(),
        ))),
        "trinamic-tmcl" | "tmcl" => Ok(Box::new(TmclDriver::configured(
            id,
            TmclConfiguredProbe::fixture(),
        ))),
        "triggerscope" => Ok(Box::new(TriggerScopeDriver::configured(
            id,
            TriggerScopeConfiguredProbe::fixture(),
        ))),
        "wosm" | "warwick-wosm" | "warwick_wosm" => Ok(Box::new(WosmDriver::configured(
            id,
            WosmConfiguredProbe::fixture(),
        ))),
        "zaber" | "zaber-ascii" => Ok(Box::new(ZaberAsciiDriver::configured(
            id,
            numanager_drivers::zaber::ZaberConfiguredProbe::simulated(),
        ))),
        other => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("unknown motion source {other}; expected one of: asi, chuo, corvus, esp32, marzhauser, openstage, openuc2, pi-gcs, prior, standa, sutter-mp285, sutter-stage, thorlabs-apt, trinamic-tmcl, triggerscope, wosm, zaber"),
        )),
    }
}
