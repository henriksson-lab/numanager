#[cfg(feature = "os-hid")]
use numanager_core::runtime::DiscoveryRegistry;
use numanager_core::runtime::{LocalRuntime, Runtime};
use numanager_core::*;
use numanager_drivers::agilent_laser_combiner::{
    AgilentLaserCombinerConfiguredProbe, AgilentLaserCombinerDriver,
};
use numanager_drivers::bluebox_niji::{NijiConfiguredProbe, NijiDriver};
use numanager_drivers::cobolt::CoboltDriver;
use numanager_drivers::coherent_obis::ObisDriver;
use numanager_drivers::coolled::{CoolLedPe300Driver, CoolLedPe4000Driver};
use numanager_drivers::lumencor::{LumencorCiaDriver, LumencorSpectraDriver};
#[cfg(feature = "os-hid")]
use numanager_drivers::mightex_bls::MightexBlsDiscovery;
use numanager_drivers::omicron::OmicronDriver;
use numanager_drivers::openuc2::OpenUc2Driver;
use numanager_drivers::spectral_lmm5::{Lmm5ConfiguredProbe, Lmm5Driver};
use numanager_drivers::thorlabs_dc::ThorlabsDcDriver;
use numanager_drivers::wosm::{WosmConfiguredProbe, WosmDriver};
use numanager_examples::{
    capability_brief, completion_summary, event_summary, is_public_property, property,
    public_kind_summary, push_schema_write,
};
use std::time::Duration;

pub fn run() -> numanager_core::Result<()> {
    maybe_run_mightex_output()?;

    let source = numanager_examples::example_arg(0).unwrap_or_else(|| "coolled".into());
    let mut runtime = LocalRuntime::new();
    runtime.add_boxed_driver_factory(|id| light_engine_driver(&source, id))?;
    runtime.add_driver_factory(CoboltDriver::simulated)?;
    runtime.add_driver_factory(LumencorCiaDriver::configured_fixture)?;
    let devices = runtime.devices().into_iter().cloned().collect::<Vec<_>>();
    let channels = devices
        .iter()
        .filter(|device| {
            device.has_kind("light.source")
                && runtime
                    .capabilities(device.id)
                    .unwrap_or(&[])
                    .iter()
                    .any(|capability| capability.kind == CapabilityKind::Dac)
        })
        .collect::<Vec<_>>();
    let channel = *channels
        .first()
        .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "missing light source channel"))?;
    let hub = devices
        .iter()
        .find(|device| device.has_kind("light.engine"))
        .unwrap_or(channel);

    let hub_trigger = runtime
        .capabilities(hub.id)?
        .into_iter()
        .find(|capability| capability.kind == CapabilityKind::TriggerSink);
    let channel_trigger = runtime.capability_by_kind(channel.id, CapabilityKind::TriggerSink)?;
    let channel_dac = runtime.capability_by_kind(channel.id, CapabilityKind::Dac)?;

    let laser = runtime.device_by_kind("laser")?;
    let laser_dac = runtime.capability_by_kind(laser.id, CapabilityKind::Dac)?;

    let cia = runtime.device_by_kind("pulse.program")?;
    let cia_program = runtime.capability_by_kind(cia.id, CapabilityKind::PulseProgram)?;
    let cia_trigger = runtime.capability_by_kind(cia.id, CapabilityKind::TriggerSink)?;

    println!("selected light source family: {source}");
    println!(
        "selected light hub: {} [{}]",
        hub.label,
        public_kind_summary(hub)
    );
    println!(
        "selected light channel: {} [{}]",
        channel.label,
        public_kind_summary(channel)
    );
    println!(
        "selected laser: {} [{}]",
        laser.label,
        public_kind_summary(laser)
    );
    println!(
        "selected trigger controller: {} [{}]",
        cia.label,
        public_kind_summary(cia)
    );
    println!(
        "capabilities: hub trigger={}; channel trigger={} dac={}; laser dac={}; cia program={} trigger={}",
        hub_trigger
            .as_ref()
            .map(|capability| capability_brief(capability))
            .unwrap_or_else(|| "none".into()),
        capability_brief(&channel_trigger),
        capability_brief(&channel_dac),
        capability_brief(&laser_dac),
        capability_brief(&cia_program),
        capability_brief(&cia_trigger)
    );
    for property in channel
        .properties
        .iter()
        .filter(|property| is_public_property(property))
    {
        println!(
            "channel property: {} type={:?} writable={} sequenceable={}",
            property.key, property.value_type, property.writable, property.sequenceable
        );
    }
    for property in &laser.properties {
        if property.key == "power" || property.key == "actual_power" || property.key == "wavelength"
        {
            println!(
                "laser property: {} type={:?} writable={} sequenceable={}",
                property.key, property.value_type, property.writable, property.sequenceable
            );
        }
    }
    for property in &cia.properties {
        if property.key == "run_state" || property.key == "event_count" {
            println!(
                "cia property: {} type={:?} writable={} sequenceable={}",
                property.key, property.value_type, property.writable, property.sequenceable
            );
        }
    }

    let runtime_devices = runtime.devices().into_iter().cloned().collect::<Vec<_>>();
    let channel_safety = runtime.safety_summary(channel, Duration::from_secs(1))?;
    println!(
        "channel safety: {} {}",
        channel_safety.state.name(),
        completion_summary(&channel_safety.as_value())
    );
    let laser_safety = runtime.safety_summary(laser, Duration::from_secs(1))?;
    println!(
        "laser safety: {} {}",
        laser_safety.state.name(),
        completion_summary(&laser_safety.as_value())
    );
    let event_devices = unique_devices([hub.id, channel.id, laser.id, cia.id]);
    let events = runtime.subscribe(
        EventFilter::devices(event_devices)
            .with_kinds([EventKind::OperationChanged, EventKind::PropertyChanged]),
    );

    let output_property = output_property(channel)?;
    let setup_value = light_output_value(channel, output_property, 25.0)?;
    let dac_value = light_output_value(channel, output_property, 42.0)?;

    let mut setup_writes = Vec::new();
    push_schema_write(channel, &mut setup_writes, output_property, setup_value);
    push_schema_write(channel, &mut setup_writes, "selected", Value::Bool(true));
    push_schema_write(channel, &mut setup_writes, "enabled", Value::Bool(true));
    push_schema_write(hub, &mut setup_writes, "enabled", Value::Bool(true));
    push_schema_write(hub, &mut setup_writes, "open", Value::Bool(true));
    let setup = runtime.submit(
        StateSet::immediate("light source setup")
            .with_writes(setup_writes)
            .into_command(),
    )?;
    let value = runtime.wait_completed(setup.id, Duration::from_secs(1))?;
    println!("state set completed: {}", completion_summary(&value));

    let dac = runtime.submit_request(channel, DacRequest { value: dac_value })?;
    let value = runtime.wait_completed(dac.id, Duration::from_secs(1))?;
    println!("dac set completed: {}", completion_summary(&value));

    let laser_power = runtime.submit_request(
        laser,
        DacRequest {
            value: Value::OpticalPower(OpticalPower::from_milliwatts(15.0)),
        },
    )?;
    let value = runtime.wait_completed(laser_power.id, Duration::from_secs(1))?;
    println!(
        "laser optical power completed: {}",
        completion_summary(&value)
    );

    let cia_program = runtime.submit_request(
        cia,
        PulseProgramRequest {
            interval: None,
            duration: None,
            count: None,
            wait_for_input: None,
        },
    )?;
    let value = runtime.wait_completed(cia_program.id, Duration::from_secs(1))?;
    println!("cia program completed: {}", completion_summary(&value));

    let cia_pulse = runtime.submit_capability(
        cia,
        CapabilityKind::TriggerSink,
        CapabilityRequest::Trigger(TriggerRequest::pulse()),
    )?;
    let value = runtime.wait_completed(cia_pulse.id, Duration::from_secs(1))?;
    println!(
        "cia trigger pulse completed: {}",
        completion_summary(&value)
    );

    let pulse = runtime.submit_capability(
        channel,
        CapabilityKind::TriggerSink,
        CapabilityRequest::None,
    )?;
    let value = runtime.wait_completed(pulse.id, Duration::from_secs(1))?;
    println!("channel pulse completed: {}", completion_summary(&value));

    if hub_trigger.is_some() {
        let disable_hub = runtime.submit_capability(
            hub,
            CapabilityKind::TriggerSink,
            CapabilityRequest::Trigger(TriggerRequest::disable()),
        )?;
        let value = runtime.wait_completed(disable_hub.id, Duration::from_secs(1))?;
        println!("hub disable completed: {}", completion_summary(&value));
    }

    let mut timing = TimingPlan::builder().sequence(
        channel,
        output_property,
        [
            light_output_value(channel, output_property, 10.0)?,
            light_output_value(channel, output_property, 50.0)?,
            light_output_value(channel, output_property, 0.0)?,
        ],
    );
    if property(channel, "enabled").is_some() {
        timing = timing.sequence(
            channel,
            "enabled",
            [Value::Bool(true), Value::Bool(true), Value::Bool(false)],
        );
    }
    if property(hub, "enabled").is_some() {
        timing = timing.sequence(
            hub,
            "enabled",
            [Value::Bool(true), Value::Bool(true), Value::Bool(false)],
        );
    } else if property(hub, "open").is_some() {
        timing = timing.sequence(
            hub,
            "open",
            [Value::Bool(true), Value::Bool(true), Value::Bool(false)],
        );
    }
    let armed = runtime.submit(
        timing
            .arm_order(unique_devices([hub.id, channel.id]))
            .stop(StopCondition::Count(3))
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
        (hub, "enabled"),
        (hub, "timing_state"),
        (hub, "fault"),
        (hub, "interlock_closed"),
        (hub, "output_temperature"),
        (hub, "ambient_temperature"),
        (channel, "enabled"),
        (channel, "selected"),
        (channel, "intensity"),
        (channel, "transmission"),
        (channel, "brightness"),
        (channel, "constant_current"),
        (channel, "output"),
        (channel, "power"),
        (laser, "power"),
        (cia, "run_state"),
    ]
    .into_iter()
    .filter(|(device, key)| property(device, key).is_some())
    {
        let value = runtime.execute(Command::read_property(device, key), Duration::from_secs(1))?;
        println!("{key}: {}", completion_summary(&value));
    }

    while let Some(event) = events.recv_timeout(Duration::from_millis(50)) {
        println!("event: {}", event_summary(&runtime_devices, &event));
    }

    Ok(())
}

fn light_engine_driver(source: &str, id: DriverId) -> numanager_core::Result<Box<dyn Driver>> {
    match source {
        "coolled" | "coolled-pe300" | "pe300" => Ok(Box::new(CoolLedPe300Driver::simulated(id))),
        "coolled-pe4000" | "pe4000" => Ok(Box::new(CoolLedPe4000Driver::simulated(id))),
        "coolled-pe340" | "pe340" => Ok(Box::new(CoolLedPe4000Driver::pe340_simulated(id))),
        "agilent" | "agilent-laser-combiner" | "agilent_combiner" => Ok(Box::new(
            AgilentLaserCombinerDriver::configured(
                id,
                AgilentLaserCombinerConfiguredProbe::fixture(),
            ),
        )),
        "obis" | "coherent-obis" | "coherent_obis" => Ok(Box::new(ObisDriver::simulated(id))),
        "omicron" => Ok(Box::new(OmicronDriver::simulated(id))),
        "lumencor" | "spectra" => Ok(Box::new(LumencorSpectraDriver::configured_fixture(id))),
        "spectral-lmm5" | "lmm5" => Ok(Box::new(Lmm5Driver::configured(
            id,
            Lmm5ConfiguredProbe::fixture(),
        ))),
        "thorlabs-dc" | "dc2010" | "dc2100" => Ok(Box::new(
            ThorlabsDcDriver::dc2xxx_configured_fixture(id),
        )),
        "thorlabs-dc2200" | "dc2200" => Ok(Box::new(
            ThorlabsDcDriver::dc2200_scpi_configured_fixture(id),
        )),
        "thorlabs-dc3100" | "dc3100" => {
            Ok(Box::new(ThorlabsDcDriver::dc3100_configured_fixture(id)))
        }
        "thorlabs-dc4100" | "dc4100" | "dc4104" => {
            Ok(Box::new(ThorlabsDcDriver::dc4100_configured_fixture(id)))
        }
        "niji" | "bluebox-niji" | "bluebox_niji" => Ok(Box::new(NijiDriver::configured(
            id,
            NijiConfiguredProbe::fixture(),
        ))),
        "openuc2" | "open-uc2" | "uc2" => Ok(Box::new(OpenUc2Driver::simulated(id))),
        "wosm" | "warwick-wosm" | "warwick_wosm" => {
            Ok(Box::new(WosmDriver::configured(id, WosmConfiguredProbe::fixture())))
        }
        other => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!(
                "unknown light source family {other:?}; use one of: coolled, pe4000, pe340, agilent, obis, omicron, lumencor, lmm5, thorlabs-dc, dc2200, dc3100, dc4100, niji, openuc2, wosm"
            ),
        )),
    }
}

fn output_property(device: &DeviceDescriptor) -> numanager_core::Result<&'static str> {
    for key in [
        "intensity",
        "transmission",
        "brightness",
        "constant_current",
        "output",
        "power",
    ] {
        if property(device, key).is_some() {
            return Ok(key);
        }
    }
    Err(Error::new(
        ErrorCode::InvalidCommand,
        "selected light source has no supported output property",
    ))
}

fn light_output_value(
    device: &DeviceDescriptor,
    key: &str,
    percent: f64,
) -> numanager_core::Result<Value> {
    let schema = property(device, key).ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidProperty,
            format!("selected light source has no {key} property"),
        )
    })?;
    match schema.value_type {
        ValueType::Ratio => Ok(Value::Ratio(Ratio::from_percent(percent))),
        ValueType::ElectricCurrent => Ok(Value::ElectricCurrent(ElectricCurrent::from_milliamps(
            percent / 100.0 * 10.0,
        ))),
        ValueType::OpticalPower => {
            let (min, max) = schema
                .range
                .as_ref()
                .and_then(|range| match (&range.min, &range.max) {
                    (Value::OpticalPower(min), Value::OpticalPower(max)) => {
                        Some((min.milliwatts(), max.milliwatts()))
                    }
                    _ => None,
                })
                .unwrap_or((0.0, 100.0));
            Ok(Value::OpticalPower(OpticalPower::from_milliwatts(
                min + (max - min) * (percent / 100.0),
            )))
        }
        _ => Err(Error::new(
            ErrorCode::InvalidProperty,
            format!("{key} is not a supported light output value type"),
        )),
    }
}

fn unique_devices(devices: impl IntoIterator<Item = DeviceId>) -> Vec<DeviceId> {
    let mut unique = Vec::new();
    for device in devices {
        if !unique.contains(&device) {
            unique.push(device);
        }
    }
    unique
}

#[cfg(feature = "os-hid")]
fn maybe_run_mightex_output() -> numanager_core::Result<()> {
    let mut discovery = DiscoveryRegistry::new();
    discovery.register_factory(MightexBlsDiscovery::os_hid);
    let candidates = discovery.detect_all()?;
    if candidates.is_empty() {
        println!("mightex hardware: no Sirius HID light controller detected");
        return Ok(());
    }

    println!(
        "mightex hardware: detected {} Sirius HID controller candidate(s)",
        candidates.len()
    );
    for candidate in &candidates {
        println!(
            "mightex hardware candidate: {} ({} device(s))",
            candidate.label(),
            candidate.devices().len()
        );
    }

    if std::env::var("NUMANAGER_MIGHTEX_OUTPUT").as_deref() != Ok("1") {
        println!(
            "mightex hardware output: skipped; set NUMANAGER_MIGHTEX_OUTPUT=1 to drive output"
        );
        return Ok(());
    }

    let mut runtime = LocalRuntime::new();
    for candidate in candidates {
        let label = candidate.label().to_string();
        let devices = runtime.add_candidate(candidate)?;
        println!(
            "mightex hardware: added {label} with {} device(s)",
            devices.len()
        );
    }

    let runtime_devices = runtime.devices().into_iter().cloned().collect::<Vec<_>>();
    let hub = runtime_devices
        .iter()
        .find(|device| device.has_kind("hid.device") && device.has_kind("light.engine"))
        .cloned()
        .ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidCommand,
                "detected Mightex controller did not expose a HID light hub",
            )
        })?;
    let channel = runtime
        .devices_by_capability(CapabilityKind::Dac)
        .into_iter()
        .find(|device| device.has_kind("light.source"))
        .cloned()
        .ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidCommand,
                "detected Mightex controller did not expose a light output channel",
            )
        })?;
    let events = runtime.subscribe(EventFilter::devices([hub.id, channel.id]).with_kinds([
        EventKind::OperationChanged,
        EventKind::PropertyChanged,
        EventKind::Telemetry,
    ]));

    println!(
        "mightex hardware output: selected {} [{}]",
        channel.label,
        public_kind_summary(&channel)
    );
    println!(
        "mightex hardware hub: {} [{}]",
        hub.label,
        public_kind_summary(&hub)
    );
    let initial_safety = runtime.safety_summary(channel.id, Duration::from_secs(2))?;
    println!(
        "mightex hardware initial safety: {} {}",
        initial_safety.state.name(),
        completion_summary(&initial_safety.as_value())
    );

    let setup = runtime.submit(
        StateSet::immediate("mightex light output")
            .with_write(
                channel.id,
                "intensity",
                Value::Ratio(Ratio::from_percent(1.0)),
            )
            .with_write(channel.id, "mode", Value::String("normal".into()))
            .with_write(channel.id, "enabled", Value::Bool(true))
            .into_command(),
    )?;
    let value = runtime.wait_completed(setup.id, Duration::from_secs(2))?;
    println!(
        "mightex hardware output setup completed: {}",
        completion_summary(&value)
    );

    let dac = runtime.submit_request(
        channel.id,
        DacRequest {
            value: Value::Ratio(Ratio::from_percent(1.0)),
        },
    )?;
    let value = runtime.wait_completed(dac.id, Duration::from_secs(2))?;
    println!(
        "mightex hardware dac completed: {}",
        completion_summary(&value)
    );
    for key in ["mode", "enabled", "intensity"] {
        let value = runtime.execute(
            Command::read_property(channel.id, key),
            Duration::from_secs(2),
        )?;
        println!(
            "mightex hardware active {key}: {}",
            completion_summary(&value)
        );
    }
    let active_safety = runtime.safety_summary(channel.id, Duration::from_secs(2))?;
    println!(
        "mightex hardware active safety: {} {}",
        active_safety.state.name(),
        completion_summary(&active_safety.as_value())
    );

    let hold = std::env::var("NUMANAGER_MIGHTEX_OUTPUT_HOLD_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_secs(1));
    println!(
        "mightex hardware output: holding 1% output for {} ms",
        hold.as_millis()
    );
    println!(
        "mightex hardware output observation required: record visible light or meter/readback before validation"
    );
    std::thread::sleep(hold);

    let disable = runtime.submit_capability(
        channel.id,
        CapabilityKind::TriggerSink,
        CapabilityRequest::Trigger(TriggerRequest::disable()),
    )?;
    let value = runtime.wait_completed(disable.id, Duration::from_secs(2))?;
    println!(
        "mightex hardware disable completed: {}",
        completion_summary(&value)
    );

    let disable_all = runtime.submit_capability(
        hub.id,
        CapabilityKind::GenericCommand,
        CapabilityRequest::GenericCommand(GenericCommandRequest {
            command: "disable_all".into(),
            params: Default::default(),
        }),
    )?;
    let value = runtime.wait_completed(disable_all.id, Duration::from_secs(2))?;
    println!(
        "mightex hardware disable-all completed: {}",
        completion_summary(&value)
    );
    let final_safety = runtime.safety_summary(channel.id, Duration::from_secs(2))?;
    println!(
        "mightex hardware final safety: {} {}",
        final_safety.state.name(),
        completion_summary(&final_safety.as_value())
    );

    let mut mightex_channel_readback = vec![
        "mode",
        "enabled",
        "intensity",
        "overdrive_current_limit",
        "overdrive_duty_cycle_limit",
        "overdrive_pulse_width_limit",
    ];
    if channel
        .properties
        .iter()
        .any(|property| property.key == "soft_start")
    {
        mightex_channel_readback.push("soft_start");
    }
    for bls_key in [
        "trigger_program",
        "trigger_repeat_count",
        "trigger_pulse_current_1",
        "trigger_pulse_current_2",
        "trigger_pulse_current_3",
        "trigger_pulse_time_1",
        "trigger_pulse_time_2",
        "trigger_pulse_time_3",
        "trigger_follow_on_current",
        "trigger_follow_off_current",
    ] {
        if channel
            .properties
            .iter()
            .any(|property| property.key == bls_key)
        {
            mightex_channel_readback.push(bls_key);
        }
    }
    for slc_key in [
        "normal_current_max_raw",
        "normal_current_set_raw",
        "strobe_current_max_raw",
        "strobe_repeat_count_raw",
        "trigger_current_max_raw",
        "trigger_polarity_raw",
        "profile_frequency",
        "profile_duty_cycle",
        "profile_current_1_raw",
        "profile_current_2_raw",
        "mode_code_readback",
        "current_max_raw_readback",
        "current_raw_readback",
        "strobe_current_max_raw_readback",
        "strobe_repeat_count_raw_readback",
        "strobe_profile_raw_readback",
        "trigger_current_max_raw_readback",
        "trigger_polarity_raw_readback",
        "trigger_profile_raw_readback",
        "load_voltage_raw",
    ] {
        if channel
            .properties
            .iter()
            .any(|property| property.key == slc_key)
        {
            mightex_channel_readback.push(slc_key);
        }
    }
    for key in mightex_channel_readback {
        let value = runtime.execute(
            Command::read_property(channel.id, key),
            Duration::from_secs(2),
        )?;
        println!("mightex hardware {key}: {}", completion_summary(&value));
    }
    for key in [
        "command_count",
        "last_command",
        "last_reply",
        "last_reply_kind",
        "last_outcome",
        "last_error",
        "last_reply_report_count",
        "last_transaction",
    ] {
        let value = runtime.execute(Command::read_property(hub.id, key), Duration::from_secs(2))?;
        println!("mightex hardware hub {key}: {}", completion_summary(&value));
    }

    while let Some(event) = events.recv_timeout(Duration::from_millis(50)) {
        println!(
            "mightex hardware event: {}",
            event_summary(&runtime_devices, &event)
        );
    }

    Ok(())
}

#[cfg(not(feature = "os-hid"))]
fn maybe_run_mightex_output() -> numanager_core::Result<()> {
    Ok(())
}
