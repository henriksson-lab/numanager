use std::ffi::{CStr, CString};
use std::io::Write;
use std::os::raw::c_char;
use std::process::ExitCode;
use std::ptr;

const MAX_HELPER_SAMPLES_PER_CHANNEL: u64 = i32::MAX as u64;
const MAX_HELPER_TRANSFER_ELEMENTS: u64 = i32::MAX as u64;

pub(super) fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut plan = PlanSetup::default();
    let mut preflight_only = false;
    let mut simulate_setup_error_after = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--ao" => plan.ao.push(required_arg(&mut args, "--ao")?),
            "--do" => plan.do_lines.push(required_arg(&mut args, "--do")?),
            "--ai" => plan.ai.push(required_arg(&mut args, "--ai")?),
            "--ci" => plan.ci.push(required_arg(&mut args, "--ci")?),
            "--co" => plan.co.push(required_arg(&mut args, "--co")?),
            "--ao-task" => plan.labels.ao = required_arg(&mut args, "--ao-task")?,
            "--do-task" => plan.labels.do_lines = required_arg(&mut args, "--do-task")?,
            "--ai-task" => plan.labels.ai = required_arg(&mut args, "--ai-task")?,
            "--ci-task" => plan.labels.ci = required_arg(&mut args, "--ci-task")?,
            "--co-task" => plan.labels.co = required_arg(&mut args, "--co-task")?,
            "--sample-rate" => {
                plan.sample_rate_hz = parse_f64(&required_arg(&mut args, "--sample-rate")?)?
            }
            "--samples" => {
                plan.samples_per_channel = parse_u64(&required_arg(&mut args, "--samples")?)?
            }
            "--width" => plan.width = Some(parse_u64(&required_arg(&mut args, "--width")?)?),
            "--height" => plan.height = Some(parse_u64(&required_arg(&mut args, "--height")?)?),
            "--frames" => plan.frames = Some(parse_u64(&required_arg(&mut args, "--frames")?)?),
            "--signal-lines" => {
                plan.signal_lines = Some(parse_u64(&required_arg(&mut args, "--signal-lines")?)?)
            }
            "--chunk-size" => {
                plan.chunk_size = Some(parse_u64(&required_arg(&mut args, "--chunk-size")?)?)
            }
            "--sample-clock-source" => {
                plan.sample_clock_source = Some(required_arg(&mut args, "--sample-clock-source")?)
            }
            "--start-trigger" => {
                plan.start_trigger = Some(required_arg(&mut args, "--start-trigger")?)
            }
            "--min-volts" => plan.min_volts = parse_f64(&required_arg(&mut args, "--min-volts")?)?,
            "--max-volts" => plan.max_volts = parse_f64(&required_arg(&mut args, "--max-volts")?)?,
            "--timeout" => {
                plan.timeout_seconds = parse_f64(&required_arg(&mut args, "--timeout")?)?
            }
            "--preflight-only" => preflight_only = true,
            "--simulate-setup-error-after" => {
                simulate_setup_error_after = Some(parse_u64(&required_arg(
                    &mut args,
                    "--simulate-setup-error-after",
                )?)?)
            }
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }

    plan.validate()?;
    validate_setup_cleanup_simulation(&plan, preflight_only, simulate_setup_error_after)?;
    print_preflight_plan(&plan)?;
    if preflight_only {
        if let Some(task_count) = simulate_setup_error_after {
            print_setup_cleanup_simulation(&plan, task_count)?;
            return Ok(());
        }
        println!("preflight_only\ttrue");
        println!("created_tasks\t0");
        println!("configured_timing\tfalse");
        println!("configured_start_trigger\tfalse");
        println!("started_tasks\tfalse");
        println!("wrote_output\tfalse");
        println!("read_input\tfalse");
        return Ok(());
    }
    let mut tasks = Vec::new();
    if let Err(error) = configure_plan_tasks(&plan, &mut tasks) {
        let cleanup_error = clear_all(tasks).err();
        println!("cleanup_after_setup_error\ttrue");
        return Err(match cleanup_error {
            Some(cleanup_error) => {
                format!("{error}; cleanup after partial setup failed: {cleanup_error}")
            }
            None => error,
        });
    }

    println!("created_tasks\t{}", tasks.len());
    println!("configured_timing\ttrue");
    println!("configured_start_trigger\t{}", plan.start_trigger.is_some());
    println!("started_tasks\tfalse");
    println!("wrote_output\tfalse");
    println!("read_input\tfalse");
    clear_all(tasks)?;
    Ok(())
}

fn configure_plan_tasks(plan: &PlanSetup, tasks: &mut Vec<Task>) -> Result<(), String> {
    if !plan.ao.is_empty() {
        tasks.push(configure_ao_task(plan)?);
    }
    if !plan.do_lines.is_empty() {
        tasks.push(configure_do_task(plan)?);
    }
    for (index, channel) in plan.ci.iter().enumerate() {
        tasks.push(configure_ci_task(index, channel, plan)?);
    }
    if !plan.ai.is_empty() {
        tasks.push(configure_ai_task(plan)?);
    }
    for (index, channel) in plan.co.iter().enumerate() {
        tasks.push(configure_co_task(index, channel, plan)?);
    }
    Ok(())
}

#[derive(Debug)]
struct PlanSetup {
    ao: Vec<String>,
    do_lines: Vec<String>,
    ai: Vec<String>,
    ci: Vec<String>,
    co: Vec<String>,
    labels: TaskLabels,
    sample_rate_hz: f64,
    samples_per_channel: u64,
    width: Option<u64>,
    height: Option<u64>,
    frames: Option<u64>,
    signal_lines: Option<u64>,
    chunk_size: Option<u64>,
    sample_clock_source: Option<String>,
    start_trigger: Option<String>,
    min_volts: f64,
    max_volts: f64,
    timeout_seconds: f64,
}

impl Default for PlanSetup {
    fn default() -> Self {
        Self {
            ao: Vec::new(),
            do_lines: Vec::new(),
            ai: Vec::new(),
            ci: Vec::new(),
            co: Vec::new(),
            labels: TaskLabels::default(),
            sample_rate_hz: 100_000.0,
            samples_per_channel: 1,
            width: None,
            height: None,
            frames: None,
            signal_lines: None,
            chunk_size: None,
            sample_clock_source: None,
            start_trigger: None,
            min_volts: -10.0,
            max_volts: 10.0,
            timeout_seconds: 10.0,
        }
    }
}

impl PlanSetup {
    fn validate(&self) -> Result<(), String> {
        if !self.sample_rate_hz.is_finite() || self.sample_rate_hz <= 0.0 {
            return Err("--sample-rate must be positive and finite".into());
        }
        if self.samples_per_channel == 0 {
            return Err("--samples must be positive".into());
        }
        if self.samples_per_channel > MAX_HELPER_SAMPLES_PER_CHANNEL {
            return Err("--samples exceeds conservative helper i32 sample count range".into());
        }
        if self.width == Some(0) {
            return Err("--width must be positive".into());
        }
        if self.height == Some(0) {
            return Err("--height must be positive".into());
        }
        if self.frames == Some(0) {
            return Err("--frames must be positive".into());
        }
        if self.signal_lines == Some(0) {
            return Err("--signal-lines must be positive".into());
        }
        if self.chunk_size == Some(0) {
            return Err("--chunk-size must be positive".into());
        }
        self.validate_raster_dimensions_complete()?;
        self.validate_signal_timing_metadata()?;
        if !self.min_volts.is_finite() || !self.max_volts.is_finite() {
            return Err("--min-volts and --max-volts must be finite".into());
        }
        if self.min_volts > self.max_volts {
            return Err("--min-volts must not exceed --max-volts".into());
        }
        if !self.timeout_seconds.is_finite() || self.timeout_seconds <= 0.0 {
            return Err("--timeout must be positive and finite".into());
        }
        validate_non_empty_values("--ao", &self.ao)?;
        validate_non_empty_values("--do", &self.do_lines)?;
        validate_non_empty_values("--ai", &self.ai)?;
        validate_non_empty_values("--ci", &self.ci)?;
        validate_non_empty_values("--co", &self.co)?;
        self.validate_physical_channels_unique()?;
        self.labels.validate()?;
        self.validate_active_task_labels_unique()?;
        validate_optional_non_empty("--sample-clock-source", self.sample_clock_source.as_deref())?;
        validate_optional_non_empty("--start-trigger", self.start_trigger.as_deref())?;
        if let Some(raster_samples) = self.raster_samples_per_channel()? {
            if raster_samples != self.samples_per_channel {
                return Err(format!(
                    "--samples ({}) must match --width * --height * --frames ({raster_samples}) when raster dimensions are supplied",
                    self.samples_per_channel
                ));
            }
        }
        self.validate_transfer_elements()?;
        if self.ao.is_empty()
            && self.do_lines.is_empty()
            && self.ai.is_empty()
            && self.ci.is_empty()
            && self.co.is_empty()
        {
            return Err("at least one --ao, --do, --ai, --ci, or --co channel is required".into());
        }
        Ok(())
    }

    fn validate_physical_channels_unique(&self) -> Result<(), String> {
        let channels = [
            ("--ao", self.ao.as_slice()),
            ("--do", self.do_lines.as_slice()),
            ("--ai", self.ai.as_slice()),
            ("--ci", self.ci.as_slice()),
            ("--co", self.co.as_slice()),
        ];
        let mut seen: Vec<(&str, &str)> = Vec::new();
        for (flag, values) in channels {
            for value in values {
                if let Some((first_flag, _)) = seen
                    .iter()
                    .find(|(_, seen_value)| *seen_value == value.as_str())
                {
                    return Err(format!(
                        "physical channels must be unique within a plan; {value:?} is used by {first_flag} and {flag}"
                    ));
                }
                seen.push((flag, value));
            }
        }
        Ok(())
    }

    fn validate_active_task_labels_unique(&self) -> Result<(), String> {
        let active_labels = [
            ("--ao-task", self.labels.ao.as_str(), !self.ao.is_empty()),
            (
                "--do-task",
                self.labels.do_lines.as_str(),
                !self.do_lines.is_empty(),
            ),
            ("--ai-task", self.labels.ai.as_str(), !self.ai.is_empty()),
            ("--ci-task", self.labels.ci.as_str(), !self.ci.is_empty()),
            ("--co-task", self.labels.co.as_str(), !self.co.is_empty()),
        ];
        for (index, (left_flag, left_label, left_active)) in active_labels.iter().enumerate() {
            if !left_active {
                continue;
            }
            for (right_flag, right_label, right_active) in active_labels.iter().skip(index + 1) {
                if *right_active && left_label == right_label {
                    return Err(format!(
                        "active task labels must be unique; {left_label:?} is used by {left_flag} and {right_flag}"
                    ));
                }
            }
        }
        Ok(())
    }

    fn raster_samples_per_channel(&self) -> Result<Option<u64>, String> {
        let (Some(width), Some(height), Some(frames)) = (self.width, self.height, self.frames)
        else {
            return Ok(None);
        };
        let pixels = width
            .checked_mul(height)
            .ok_or_else(|| "--width * --height overflows u64".to_owned())?;
        let samples = pixels
            .checked_mul(frames)
            .ok_or_else(|| "--width * --height * --frames overflows u64".to_owned())?;
        Ok(Some(samples))
    }

    fn validate_raster_dimensions_complete(&self) -> Result<(), String> {
        let supplied = [
            ("--width", self.width.is_some()),
            ("--height", self.height.is_some()),
            ("--frames", self.frames.is_some()),
        ];
        if supplied.iter().any(|(_, present)| *present)
            && supplied.iter().any(|(_, present)| !*present)
        {
            let missing = supplied
                .iter()
                .filter_map(|(flag, present)| (!*present).then_some(*flag))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "--width, --height, and --frames must be supplied together; missing {missing}"
            ));
        }
        Ok(())
    }

    fn validate_signal_timing_metadata(&self) -> Result<(), String> {
        if self.signal_lines.is_some()
            && (self.width.is_some() || self.height.is_some() || self.frames.is_some())
        {
            return Err(
                "--signal-lines must not be combined with raster --width/--height/--frames".into(),
            );
        }
        if self.chunk_size.is_some() && self.signal_lines.is_none() {
            return Err("--chunk-size requires --signal-lines for signal timing preview".into());
        }
        if let Some(lines) = self.signal_lines {
            if self.samples_per_channel % lines != 0 {
                return Err("--samples must be divisible by --signal-lines".into());
            }
        }
        if let Some(chunk_size) = self.chunk_size {
            if chunk_size > self.samples_per_channel {
                return Err("--chunk-size must not exceed --samples".into());
            }
        }
        Ok(())
    }

    fn validate_transfer_elements(&self) -> Result<(), String> {
        for (role, channels) in [
            ("analog output", self.ao.len()),
            ("digital output", self.do_lines.len()),
            ("analog input", self.ai.len()),
            ("counter input", self.ci.len()),
            ("counter output", self.co.len()),
        ] {
            if channels == 0 {
                continue;
            }
            let total = self
                .samples_per_channel
                .checked_mul(channels as u64)
                .ok_or_else(|| format!("{role} transfer element count overflows u64"))?;
            if total > MAX_HELPER_TRANSFER_ELEMENTS {
                return Err(format!(
                    "{role} transfer element count {total} exceeds conservative helper i32 range"
                ));
            }
        }
        Ok(())
    }

    fn effective_sample_clock_source(&self) -> Option<String> {
        self.sample_clock_source.clone().or_else(|| {
            self.co
                .first()
                .and_then(|channel| counter_internal_output_source(channel))
        })
    }

    fn sample_clock_source_origin(&self) -> &'static str {
        if self.sample_clock_source.is_some() {
            "explicit"
        } else if !self.co.is_empty() && self.effective_sample_clock_source().is_some() {
            "derived_counter_output_internal"
        } else {
            "default_task_timebase"
        }
    }
}

fn validate_optional_non_empty(flag: &str, value: Option<&str>) -> Result<(), String> {
    if let Some(value) = value {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(format!("{flag} must not be empty when supplied"));
        }
        validate_no_surrounding_whitespace(flag, value)?;
    }
    Ok(())
}

fn validate_non_empty_values(flag: &str, values: &[String]) -> Result<(), String> {
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(format!("{flag} values must not be empty"));
        }
        validate_no_surrounding_whitespace(flag, value)?;
    }
    Ok(())
}

fn validate_no_surrounding_whitespace(flag: &str, value: &str) -> Result<(), String> {
    if value.trim() != value {
        return Err(format!(
            "{flag} must not have leading or trailing whitespace"
        ));
    }
    Ok(())
}

fn counter_internal_output_source(channel: &str) -> Option<String> {
    let (device, counter) = channel.rsplit_once('/')?;
    let index = counter
        .strip_prefix("ctr")
        .or_else(|| counter.strip_prefix("Ctr"))?
        .parse::<u32>()
        .ok()?;
    let device = device.trim_start_matches('/');
    (!device.is_empty()).then(|| format!("/{device}/Ctr{index}InternalOutput"))
}

#[derive(Debug)]
struct TaskLabels {
    ao: String,
    do_lines: String,
    ai: String,
    ci: String,
    co: String,
}

impl Default for TaskLabels {
    fn default() -> Self {
        Self {
            ao: "analog_output".into(),
            do_lines: "digital_output".into(),
            ai: "analog_input".into(),
            ci: "counter_input".into(),
            co: "counter_output".into(),
        }
    }
}

impl TaskLabels {
    fn validate(&self) -> Result<(), String> {
        validate_task_label("--ao-task", &self.ao)?;
        validate_task_label("--do-task", &self.do_lines)?;
        validate_task_label("--ai-task", &self.ai)?;
        validate_task_label("--ci-task", &self.ci)?;
        validate_task_label("--co-task", &self.co)?;
        Ok(())
    }
}

fn validate_task_label(flag: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{flag} must not be empty"));
    }
    validate_no_surrounding_whitespace(flag, value)
}

fn validate_setup_cleanup_simulation(
    plan: &PlanSetup,
    preflight_only: bool,
    simulate_setup_error_after: Option<u64>,
) -> Result<(), String> {
    let Some(task_count) = simulate_setup_error_after else {
        return Ok(());
    };
    if !preflight_only {
        return Err("--simulate-setup-error-after requires --preflight-only".into());
    }
    if task_count == 0 {
        return Err("--simulate-setup-error-after must be positive".into());
    }
    let planned_tasks = setup_order(plan);
    if task_count as usize > planned_tasks.len() {
        return Err(format!(
            "--simulate-setup-error-after ({task_count}) exceeds planned task count ({})",
            planned_tasks.len()
        ));
    }
    Ok(())
}

fn print_preflight_plan(plan: &PlanSetup) -> Result<(), String> {
    println!("preflight_plan\ttrue");
    println!("sample_rate_hz\t{:.6}", plan.sample_rate_hz);
    println!("samples_per_channel\t{}", plan.samples_per_channel);
    println!(
        "sample_clock_source\t{}",
        plan.effective_sample_clock_source()
            .as_deref()
            .unwrap_or("<empty>")
    );
    println!(
        "sample_clock_source_origin\t{}",
        plan.sample_clock_source_origin()
    );
    println!(
        "start_trigger\t{}",
        plan.start_trigger.as_deref().unwrap_or("<empty>")
    );
    println!(
        "analog_range_volts\t{:.6}\t{:.6}",
        plan.min_volts, plan.max_volts
    );
    println!("cleanup_timeout_s\t{:.6}", plan.timeout_seconds);
    print_planned_tasks(plan);
    print_planned_order(plan);
    print_planned_routes(plan);
    print_planned_timing(plan);
    print_raster_timing_preview(plan);
    print_signal_timing_preview(plan);
    print_waveform_intent(plan);
    print_transfer_plan(plan);
    print_planned_runtime_sequence(plan);
    print_planned_execution_contract(plan);
    print_planned_live_executor(plan);
    print_planned_reconstruction(plan);
    print_planned_publication(plan);
    print_planned_cleanup(plan);
    std::io::stdout()
        .flush()
        .map_err(|error| format!("failed to flush preflight plan: {error}"))
}

fn print_setup_cleanup_simulation(plan: &PlanSetup, task_count: u64) -> Result<(), String> {
    let created = setup_order(plan)
        .into_iter()
        .take(task_count as usize)
        .collect::<Vec<_>>();
    let Some(partial_task) = created.last() else {
        return Err("setup cleanup simulation requires at least one planned task".into());
    };
    println!("preflight_only\ttrue");
    println!("simulated_failure\ttrue");
    println!(
        "simulated_error_message\tsimulated DAQmx setup error after {task_count} created task(s)"
    );
    println!("simulated_created_tasks\t{}", created.join(","));
    println!("cleared_partial_task\t{partial_task}");
    for task in created.iter().rev().skip(1) {
        println!("cleared_task\t{task}");
    }
    println!("cleanup_after_setup_error\ttrue");
    println!("created_tasks\t0");
    println!("configured_timing\tsimulated");
    println!("configured_start_trigger\tsimulated");
    println!("started_tasks\tfalse");
    println!("wrote_output\tfalse");
    println!("read_input\tfalse");
    std::io::stdout()
        .flush()
        .map_err(|error| format!("failed to flush setup cleanup simulation: {error}"))
}

fn print_planned_tasks(plan: &PlanSetup) {
    if !plan.ao.is_empty() {
        println!(
            "planned_task\t{}\tanalog_output\t{}",
            plan.labels.ao,
            plan.ao.join(",")
        );
    }
    if !plan.do_lines.is_empty() {
        println!(
            "planned_task\t{}\tdigital_output\t{}",
            plan.labels.do_lines,
            plan.do_lines.join(",")
        );
    }
    if !plan.ci.is_empty() {
        println!(
            "planned_task\t{}\tcounter_input\t{}",
            plan.labels.ci,
            plan.ci.join(",")
        );
    }
    if !plan.ai.is_empty() {
        println!(
            "planned_task\t{}\tanalog_input\t{}",
            plan.labels.ai,
            plan.ai.join(",")
        );
    }
    if !plan.co.is_empty() {
        println!(
            "planned_task\t{}\tcounter_output\t{}",
            plan.labels.co,
            plan.co.join(",")
        );
    }
}

fn print_planned_order(plan: &PlanSetup) {
    print_order_row("planned_setup_order", &setup_order(plan));
    print_order_row("planned_start_order", &start_order(plan));
    print_order_row("planned_read_order", &read_order(plan));
    print_order_row("planned_stop_order", &stop_order(plan));
    print_order_row("planned_clear_order", &clear_order(plan));
    println!("cleanup_policy\tstop_started_tasks_then_clear_all_created_tasks");
}

fn print_planned_routes(plan: &PlanSetup) {
    let consumers = sample_clock_consumers(plan);
    let source = plan.effective_sample_clock_source();
    println!(
        "planned_sample_clock_route\tsource={}\tproducer={}\tconsumers={}\tedge=Rising",
        source.as_deref().unwrap_or("<empty>"),
        sample_clock_producer(plan).unwrap_or_else(|| "none".into()),
        join_or_none(&consumers),
    );
    println!(
        "planned_start_trigger_route\tsource={}\tconsumers={}\tedge=Rising",
        plan.start_trigger.as_deref().unwrap_or("<empty>"),
        join_or_none(&consumers),
    );
}

fn print_planned_timing(plan: &PlanSetup) {
    let source = plan.effective_sample_clock_source();
    let source = source.as_deref().unwrap_or("<empty>");
    for task in sample_clock_consumers(plan) {
        println!(
            "planned_timing\t{task}\tsample_clock\tsource={source}\trate_hz={:.6}\tedge=Rising\tmode=FiniteSamps\tsamples_per_channel={}",
            plan.sample_rate_hz, plan.samples_per_channel
        );
    }
    if !plan.co.is_empty() {
        println!(
            "planned_timing\t{}\timplicit\tmode=FiniteSamps\tsamples_per_channel={}\tpulse_frequency_hz={:.6}\tidle_state=Low\tduty_cycle=0.500000",
            plan.labels.co, plan.samples_per_channel, plan.sample_rate_hz
        );
    }
}

fn print_raster_timing_preview(plan: &PlanSetup) {
    let (Some(width), Some(height), Some(frames)) = (plan.width, plan.height, plan.frames) else {
        return;
    };
    if width == 0 || height == 0 || frames == 0 {
        return;
    }
    let pixel_period = 1.0 / plan.sample_rate_hz;
    let line_period = width as f64 / plan.sample_rate_hz;
    let frame_period = width as f64 * height as f64 / plan.sample_rate_hz;
    let total_period = frame_period * frames as f64;
    println!(
        "raster_timing_preview\tpixel_period_s={pixel_period:.9}\tline_period_s={line_period:.9}\tframe_period_s={frame_period:.9}\ttotal_period_s={total_period:.9}\tevidence=pending_hardware_validation"
    );
}

fn print_signal_timing_preview(plan: &PlanSetup) {
    let Some(lines) = plan.signal_lines else {
        return;
    };
    if lines == 0 {
        return;
    }
    let samples_per_line = plan.samples_per_channel / lines;
    let sample_period = 1.0 / plan.sample_rate_hz;
    let line_period = samples_per_line as f64 / plan.sample_rate_hz;
    let total_period = plan.samples_per_channel as f64 / plan.sample_rate_hz;
    let chunk_period = plan
        .chunk_size
        .map(|chunk_size| format!("{:.9}", chunk_size as f64 / plan.sample_rate_hz))
        .unwrap_or_else(|| "<unspecified>".into());
    println!(
        "signal_timing_preview\tsample_period_s={sample_period:.9}\tsamples_per_line={samples_per_line}\tlines={lines}\tline_period_s={line_period:.9}\tchunk_size={}\tchunk_period_s={chunk_period}\ttotal_period_s={total_period:.9}\tevidence=pending_hardware_validation",
        plan.chunk_size
            .map(|value| value.to_string())
            .unwrap_or_else(|| "<unspecified>".into())
    );
}

fn sample_clock_consumers(plan: &PlanSetup) -> Vec<String> {
    ordered_roles(plan, &[Role::Ci, Role::Ai, Role::Ao, Role::Do])
}

fn sample_clock_producer(plan: &PlanSetup) -> Option<String> {
    (!plan.co.is_empty()).then(|| Role::Co.label(plan))
}

fn join_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".into()
    } else {
        values.join(",")
    }
}

fn setup_order(plan: &PlanSetup) -> Vec<String> {
    ordered_roles(plan, &[Role::Ao, Role::Do, Role::Ci, Role::Ai, Role::Co])
}

fn start_order(plan: &PlanSetup) -> Vec<String> {
    ordered_roles(plan, &[Role::Ci, Role::Ai, Role::Ao, Role::Do, Role::Co])
}

fn read_order(plan: &PlanSetup) -> Vec<String> {
    ordered_roles(plan, &[Role::Ci, Role::Ai])
}

fn stop_order(plan: &PlanSetup) -> Vec<String> {
    let mut order = start_order(plan);
    order.reverse();
    order
}

fn clear_order(plan: &PlanSetup) -> Vec<String> {
    let mut order = setup_order(plan);
    order.reverse();
    order
}

fn ordered_roles(plan: &PlanSetup, roles: &[Role]) -> Vec<String> {
    roles
        .iter()
        .copied()
        .filter(|role| role.is_present(plan))
        .map(|role| role.label(plan))
        .collect()
}

fn print_order_row(label: &str, roles: &[String]) {
    println!("{label}\t{}", roles.join(","));
}

#[derive(Debug, Clone, Copy)]
enum Role {
    Ao,
    Do,
    Ai,
    Ci,
    Co,
}

impl Role {
    fn is_present(self, plan: &PlanSetup) -> bool {
        match self {
            Self::Ao => !plan.ao.is_empty(),
            Self::Do => !plan.do_lines.is_empty(),
            Self::Ai => !plan.ai.is_empty(),
            Self::Ci => !plan.ci.is_empty(),
            Self::Co => !plan.co.is_empty(),
        }
    }

    fn label(self, plan: &PlanSetup) -> String {
        match self {
            Self::Ao => plan.labels.ao.clone(),
            Self::Do => plan.labels.do_lines.clone(),
            Self::Ai => plan.labels.ai.clone(),
            Self::Ci => plan.labels.ci.clone(),
            Self::Co => plan.labels.co.clone(),
        }
    }
}

fn print_waveform_intent(plan: &PlanSetup) {
    if !plan.ao.is_empty() {
        println!(
            "planned_waveform\t{}\tanalog_output\tpattern=x_fast_sawtooth_y_slow_step\tsample_order=row_major_unidirectional\twidth={}\theight={}\tframes={}\tchannels={}\tvoltage_min={:.6}\tvoltage_max={:.6}\tevidence=pending_hardware_validation",
            plan.labels.ao,
            display_optional_u64(plan.width),
            display_optional_u64(plan.height),
            display_optional_u64(plan.frames),
            plan.ao.len(),
            plan.min_volts,
            plan.max_volts,
        );
    }
    if !plan.do_lines.is_empty() {
        println!(
            "planned_waveform\t{}\tdigital_output\tpattern=high_during_active_pixels\tsample_order=row_major_unidirectional\tline_indexing=zero_based\twidth={}\theight={}\tframes={}\tchannels={}\tevidence=pending_hardware_validation",
            plan.labels.do_lines,
            display_optional_u64(plan.width),
            display_optional_u64(plan.height),
            display_optional_u64(plan.frames),
            plan.do_lines.len(),
        );
    }
    print_raster_waveform_preview(plan);
}

fn print_raster_waveform_preview(plan: &PlanSetup) {
    let (Some(width), Some(height), Some(frames)) = (plan.width, plan.height, plan.frames) else {
        return;
    };
    if width == 0 || height == 0 || frames == 0 {
        return;
    }
    let preview_indices = raster_preview_indices(width, height, frames);
    if !plan.ao.is_empty() {
        let preview = preview_indices
            .iter()
            .map(|sample| {
                let (x, y) =
                    raster_xy_volts(*sample, width, height, plan.min_volts, plan.max_volts);
                format!("{sample}:x={x:.3},y={y:.3}")
            })
            .collect::<Vec<_>>()
            .join("|");
        println!(
            "waveform_preview\t{}\tanalog_output\tpattern=x_fast_sawtooth_y_slow_step\tsamples={preview}\tevidence=pending_hardware_validation",
            plan.labels.ao
        );
    }
    if !plan.do_lines.is_empty() {
        let preview = preview_indices
            .iter()
            .map(|sample| {
                format!(
                    "{sample}:gate={}",
                    raster_gate_state(*sample, width, height)
                )
            })
            .collect::<Vec<_>>()
            .join("|");
        println!(
            "waveform_preview\t{}\tdigital_output\tpattern=high_during_active_pixels\tsamples={preview}\tevidence=pending_hardware_validation",
            plan.labels.do_lines
        );
    }
}

fn raster_preview_indices(width: u64, height: u64, frames: u64) -> Vec<u64> {
    let total = width.saturating_mul(height).saturating_mul(frames);
    let last = total.saturating_sub(1);
    let middle_line = height / 2;
    let middle_column = width / 2;
    let middle = middle_line
        .saturating_mul(width)
        .saturating_add(middle_column)
        .min(last);
    let mut samples = Vec::new();
    for sample in [0, middle, last] {
        if !samples.contains(&sample) {
            samples.push(sample);
        }
    }
    samples
}

fn raster_xy_volts(
    sample: u64,
    width: u64,
    height: u64,
    min_volts: f64,
    max_volts: f64,
) -> (f64, f64) {
    let pixel = sample % width.saturating_mul(height);
    let x_index = pixel % width;
    let y_index = pixel / width;
    (
        interpolate_index(x_index, width, min_volts, max_volts),
        interpolate_index(y_index, height, min_volts, max_volts),
    )
}

fn interpolate_index(index: u64, count: u64, min_volts: f64, max_volts: f64) -> f64 {
    if count <= 1 {
        return (min_volts + max_volts) / 2.0;
    }
    let fraction = index as f64 / (count - 1) as f64;
    min_volts + (max_volts - min_volts) * fraction
}

fn raster_gate_state(sample: u64, width: u64, height: u64) -> u8 {
    u8::from(sample < width.saturating_mul(height))
}

fn display_optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "<unspecified>".into())
}

fn print_transfer_plan(plan: &PlanSetup) {
    let samples = plan.samples_per_channel;
    if !plan.ao.is_empty() {
        println!(
            "planned_transfer\t{}\tanalog_output\twrite\tf64_volts\tchannels={}\tsamples_per_channel={samples}\ttotal_elements={}\tlayout=GroupByScanNumber\tauto_start=false\ttimeout_s={:.6}",
            plan.labels.ao,
            plan.ao.len(),
            plan.ao.len() as u64 * samples,
            plan.timeout_seconds,
        );
    }
    if !plan.do_lines.is_empty() {
        println!(
            "planned_transfer\t{}\tdigital_output\twrite\tu8_line_state\tchannels={}\tsamples_per_channel={samples}\ttotal_elements={}\tlayout=GroupByScanNumber\tauto_start=false\ttimeout_s={:.6}",
            plan.labels.do_lines,
            plan.do_lines.len(),
            plan.do_lines.len() as u64 * samples,
            plan.timeout_seconds,
        );
    }
    if !plan.ci.is_empty() {
        println!(
            "planned_transfer\t{}\tcounter_input\tread\tu32_counts\tchannels={}\tsamples_per_channel={samples}\ttotal_elements={}\ttimeout_s={:.6}",
            plan.labels.ci,
            plan.ci.len(),
            plan.ci.len() as u64 * samples,
            plan.timeout_seconds,
        );
    }
    if !plan.ai.is_empty() {
        println!(
            "planned_transfer\t{}\tanalog_input\tread\tf64_volts\tchannels={}\tsamples_per_channel={samples}\ttotal_elements={}\tlayout=GroupByScanNumber\ttimeout_s={:.6}",
            plan.labels.ai,
            plan.ai.len(),
            plan.ai.len() as u64 * samples,
            plan.timeout_seconds,
        );
    }
    if !plan.co.is_empty() {
        println!(
            "planned_transfer\t{}\tcounter_output\tgenerate\tcounter_pulse_train\tchannels={}\tsamples_per_channel={samples}\ttotal_elements={}\ttiming=implicit_finite",
            plan.labels.co,
            plan.co.len(),
            plan.co.len() as u64 * samples,
        );
    }
}

fn print_planned_runtime_sequence(plan: &PlanSetup) {
    print_sequence_step(1, "setup", &setup_order(plan), "create_channels_and_timing");
    let write_order = write_order(plan);
    if !write_order.is_empty() {
        print_sequence_step(2, "write", &write_order, "buffered_output_before_start");
    }
    print_sequence_step(3, "start", &start_order(plan), "inputs_outputs_then_clock");
    let read_order = read_order(plan);
    if !read_order.is_empty() {
        print_sequence_step(4, "read", &read_order, "finite_samples");
    }
    if !plan.co.is_empty() {
        print_sequence_step(
            5,
            "wait",
            &[plan.labels.co.clone()],
            "counter_output_done_or_timeout",
        );
    }
    print_sequence_step(6, "stop", &stop_order(plan), "reverse_started_order");
    print_sequence_step(7, "clear", &clear_order(plan), "reverse_setup_order");
    println!(
        "planned_completion\tmode=finite\tsamples_per_channel={}\ttimeout_s={:.6}\tevidence=pending_hardware_validation",
        plan.samples_per_channel, plan.timeout_seconds
    );
}

fn print_planned_execution_contract(plan: &PlanSetup) {
    println!(
        "planned_execution_contract\tmode={}\twrite={}\tread={}\twait={}\twrite_policy=buffered_before_start\twrite_auto_start=false\twrite_layout=GroupByScanNumber\tread_policy=finite_expected_samples\tread_layout=GroupByScanNumber_for_analog_input\ttimeout_s={:.6}\tpublication_policy=publish_only_after_validated_read_and_reconstruction\tevidence=pending_hardware_validation",
        execution_contract_mode(plan),
        join_or_none(&write_order(plan)),
        join_or_none(&read_order(plan)),
        join_or_none(&wait_order(plan)),
        plan.timeout_seconds
    );
}

fn execution_contract_mode(plan: &PlanSetup) -> &'static str {
    if plan.width.is_some() && plan.height.is_some() && plan.frames.is_some() {
        "raster_finite"
    } else if plan.signal_lines.is_some() {
        "signal_finite"
    } else {
        "finite_task_plan"
    }
}

fn print_planned_live_executor(plan: &PlanSetup) {
    println!(
        "planned_live_executor\tmode={}\tstatus=not_enabled_pending_hardware_validation\tbackend=ni_daqmx_sdk_task_wrapper\ttarget_scope=linux_windows_optional_sdk_backend\trequired_validation=legal_review,installed_header_audit,ni_pal_device_inventory,bench_safety_preconditions,task_ordering_routing_completion_cleanup_bench_validation,runtime_publication_hardware_validation,hardware_validation_note\tevidence=pending_hardware_validation",
        execution_contract_mode(plan)
    );
    print_executor_phase(
        1,
        "validate_readiness",
        &[],
        "check_feature_target_package_header_runtime_live_request_and_external_gates",
    );
    print_executor_phase(
        2,
        "setup",
        &setup_order(plan),
        "DAQmxCreateTask+channel_creation+timing_and_trigger_configuration",
    );
    print_executor_phase(
        3,
        "write",
        &write_order(plan),
        "DAQmxWriteAnalogF64+DAQmxWriteDigitalLines_buffered_auto_start_false",
    );
    print_executor_phase(4, "start", &start_order(plan), "DAQmxStartTask");
    print_executor_phase(
        5,
        "read",
        &read_order(plan),
        "DAQmxReadCounterU32+DAQmxReadAnalogF64_finite_expected_samples",
    );
    print_executor_phase(6, "wait", &wait_order(plan), "DAQmxWaitUntilTaskDone");
    print_executor_phase(
        7,
        "publish",
        &[],
        "publish_public_FrameReady_or_ScanSignalChunk_after_validated_read",
    );
    print_executor_phase(
        8,
        "cleanup",
        &stop_order(plan),
        "DAQmxStopTask_then_DAQmxClearTask_for_created_tasks",
    );
    print_executor_phase(
        9,
        "clear",
        &clear_order(plan),
        "DAQmxClearTask_reverse_setup_order",
    );
}

fn print_executor_phase(step: u8, phase: &str, tasks: &[String], api_surface: &str) {
    println!(
        "planned_live_executor_phase\tstep={step}\tphase={phase}\ttasks={}\tapi_surface={api_surface}\tevidence=pending_hardware_validation",
        join_or_none(tasks)
    );
}

fn print_planned_reconstruction(plan: &PlanSetup) {
    if let (Some(width), Some(height), Some(frames)) = (plan.width, plan.height, plan.frames) {
        println!(
            "planned_reconstruction\tmode=one_detector_sample_per_pixel\tinput={}\tscan={}x{}\tframes={}\treconstruction={}x{}\tpixel_format=pending_runtime_reconstruction\tsample_to_pixel_mapping=row_major_unidirectional_one_sample_per_pixel\taccumulation=sum_samples_per_reconstructed_pixel\tbackground_subtraction=disabled_until_hardware_validated\tsaturation_policy=clip_to_pixel_format_and_report_saturated_pixels\tpublication_gate=publish_after_validated_read_and_reconstruction\tevidence=pending_hardware_validation",
            join_or_none(&read_order(plan)),
            width,
            height,
            frames,
            width,
            height
        );
    }
}

fn print_planned_publication(plan: &PlanSetup) {
    if let (Some(width), Some(height), Some(frames)) = (plan.width, plan.height, plan.frames) {
        println!(
            "planned_publication\tevent=FrameReady\tmode=raster_frame_payload\tscan={}x{}\tframes={}\tpixel_format=pending_runtime_reconstruction\trequired_metadata=frame_handle,stream,scan_width,scan_height,reconstruction_width,reconstruction_height,reconstruction_pixel_size,sample_rate,line_dwell,detectors,saturated_pixels,progress_status\tevidence=pending_hardware_validation",
            width, height, frames
        );
    }
    if let Some(lines) = plan.signal_lines {
        let samples_per_line = plan.samples_per_channel / lines;
        println!(
            "planned_publication\tevent=ScanSignalChunk\tmode=raw_signal_chunks\tchannels={}\tsamples_per_line={samples_per_line}\tlines={lines}\tchunk_size={}\trequired_metadata=stream,channel_names,timing_origin,line_index,chunk_index,first_sample_index,sample_count,sample_values,sample_rate,sample_period,dropped_samples,dropped_chunks,overflowed\tevidence=pending_hardware_validation",
            join_or_none(&read_order(plan)),
            plan.chunk_size
                .map(|value| value.to_string())
                .unwrap_or_else(|| "<unspecified>".into())
        );
    }
}

fn print_planned_cleanup(plan: &PlanSetup) {
    println!(
        "planned_cleanup\tfailure_modes=partial_setup_failure,post_start_failure,buffered_write_failure,finite_read_failure,counter_output_wait_timeout\tstarted_task_cleanup=stop_started_tasks_before_clear\tsafe_output_state=pending_hardware_validation\tevidence=pending_hardware_validation"
    );
    println!(
        "planned_cleanup_order\tstop={}\tclear={}\ttimeout_s={:.6}\tevidence=pending_hardware_validation",
        join_or_none(&stop_order(plan)),
        join_or_none(&clear_order(plan)),
        plan.timeout_seconds
    );
}

fn write_order(plan: &PlanSetup) -> Vec<String> {
    ordered_roles(plan, &[Role::Ao, Role::Do])
}

fn wait_order(plan: &PlanSetup) -> Vec<String> {
    ordered_roles(plan, &[Role::Co])
}

fn print_sequence_step(step: u8, phase: &str, tasks: &[String], basis: &str) {
    println!(
        "planned_runtime_sequence\tstep={step}\tphase={phase}\ttasks={}\tbasis={basis}\tevidence=pending_hardware_validation",
        join_or_none(tasks)
    );
}

struct Task {
    name: String,
    handle: ni_daqmx_sys::TaskHandle,
    cleared: bool,
}

fn configure_ao_task(plan: &PlanSetup) -> Result<Task, String> {
    let task = create_task("numanager-plan-ao")?;
    finish_configured_task(task, |task| {
        for channel in &plan.ao {
            create_ao_voltage_channel(task.handle, channel, plan.min_volts, plan.max_volts)?;
            println!("configured_ao\t{channel}");
        }
        cfg_sample_clock_timing(task.handle, plan)?;
        cfg_start_trigger_if_needed(task.handle, plan)
    })
}

fn configure_do_task(plan: &PlanSetup) -> Result<Task, String> {
    let task = create_task("numanager-plan-do")?;
    finish_configured_task(task, |task| {
        for line in &plan.do_lines {
            create_do_lines(task.handle, line)?;
            println!("configured_do\t{line}");
        }
        cfg_sample_clock_timing(task.handle, plan)?;
        cfg_start_trigger_if_needed(task.handle, plan)
    })
}

fn configure_ai_task(plan: &PlanSetup) -> Result<Task, String> {
    let task = create_task("numanager-plan-ai")?;
    finish_configured_task(task, |task| {
        for channel in &plan.ai {
            create_ai_voltage_channel(task.handle, channel, plan.min_volts, plan.max_volts)?;
            println!("configured_ai\t{channel}");
        }
        cfg_sample_clock_timing(task.handle, plan)?;
        cfg_start_trigger_if_needed(task.handle, plan)
    })
}

fn configure_ci_task(index: usize, channel: &str, plan: &PlanSetup) -> Result<Task, String> {
    let task = create_task(&format!("numanager-plan-ci-{index}"))?;
    finish_configured_task(task, |task| {
        create_ci_count_edges_channel(task.handle, channel)?;
        println!("configured_ci\t{channel}");
        cfg_sample_clock_timing(task.handle, plan)?;
        cfg_start_trigger_if_needed(task.handle, plan)
    })
}

fn configure_co_task(index: usize, channel: &str, plan: &PlanSetup) -> Result<Task, String> {
    let task = create_task(&format!("numanager-plan-co-{index}"))?;
    finish_configured_task(task, |task| {
        create_co_pulse_channel_freq(task.handle, channel, plan.sample_rate_hz)?;
        println!("configured_co\t{channel}");
        cfg_implicit_timing(task.handle, plan.samples_per_channel)
    })
}

fn finish_configured_task<F>(mut task: Task, configure: F) -> Result<Task, String>
where
    F: FnOnce(&Task) -> Result<(), String>,
{
    match configure(&task) {
        Ok(()) => Ok(task),
        Err(error) => {
            let name = task.name.clone();
            let cleanup = task.clear_inner();
            match cleanup {
                Ok(()) => {
                    println!("cleared_partial_task\t{name}");
                    Err(error)
                }
                Err(cleanup_error) => Err(format!(
                    "{error}; failed to clear partial task {name}: {cleanup_error}"
                )),
            }
        }
    }
}

fn create_task(name: &str) -> Result<Task, String> {
    let name_c = required_cstring("task name", name)?;
    let mut handle = ptr::null_mut();
    check_status(
        unsafe { ni_daqmx_sys::DAQmxCreateTask(name_c.as_ptr(), &mut handle) },
        "DAQmxCreateTask",
    )?;
    println!("created_task\t{name}");
    Ok(Task {
        name: name.into(),
        handle,
        cleared: false,
    })
}

fn create_ao_voltage_channel(
    handle: ni_daqmx_sys::TaskHandle,
    channel: &str,
    min_volts: f64,
    max_volts: f64,
) -> Result<(), String> {
    let channel_c = required_cstring("ao channel", channel)?;
    check_status(
        unsafe {
            ni_daqmx_sys::DAQmxCreateAOVoltageChan(
                handle,
                channel_c.as_ptr(),
                ptr::null(),
                min_volts,
                max_volts,
                ni_daqmx_sys::DAQmx_Val_Volts,
                ptr::null(),
            )
        },
        "DAQmxCreateAOVoltageChan",
    )
}

fn create_do_lines(handle: ni_daqmx_sys::TaskHandle, line: &str) -> Result<(), String> {
    let line_c = required_cstring("do line", line)?;
    check_status(
        unsafe {
            ni_daqmx_sys::DAQmxCreateDOChan(
                handle,
                line_c.as_ptr(),
                ptr::null(),
                ni_daqmx_sys::DAQmx_Val_ChanForAllLines,
            )
        },
        "DAQmxCreateDOChan",
    )
}

fn create_ai_voltage_channel(
    handle: ni_daqmx_sys::TaskHandle,
    channel: &str,
    min_volts: f64,
    max_volts: f64,
) -> Result<(), String> {
    let channel_c = required_cstring("ai channel", channel)?;
    check_status(
        unsafe {
            ni_daqmx_sys::DAQmxCreateAIVoltageChan(
                handle,
                channel_c.as_ptr(),
                ptr::null(),
                ni_daqmx_sys::DAQmx_Val_Cfg_Default,
                min_volts,
                max_volts,
                ni_daqmx_sys::DAQmx_Val_Volts,
                ptr::null(),
            )
        },
        "DAQmxCreateAIVoltageChan",
    )
}

fn create_ci_count_edges_channel(
    handle: ni_daqmx_sys::TaskHandle,
    channel: &str,
) -> Result<(), String> {
    let channel_c = required_cstring("ci channel", channel)?;
    check_status(
        unsafe {
            ni_daqmx_sys::DAQmxCreateCICountEdgesChan(
                handle,
                channel_c.as_ptr(),
                ptr::null(),
                ni_daqmx_sys::DAQmx_Val_Rising,
                0,
                ni_daqmx_sys::DAQmx_Val_CountUp,
            )
        },
        "DAQmxCreateCICountEdgesChan",
    )
}

fn create_co_pulse_channel_freq(
    handle: ni_daqmx_sys::TaskHandle,
    channel: &str,
    frequency_hz: f64,
) -> Result<(), String> {
    let channel_c = required_cstring("co channel", channel)?;
    check_status(
        unsafe {
            ni_daqmx_sys::DAQmxCreateCOPulseChanFreq(
                handle,
                channel_c.as_ptr(),
                ptr::null(),
                ni_daqmx_sys::DAQmx_Val_Hz,
                ni_daqmx_sys::DAQmx_Val_Low,
                0.0,
                frequency_hz,
                0.5,
            )
        },
        "DAQmxCreateCOPulseChanFreq",
    )
}

fn cfg_sample_clock_timing(
    handle: ni_daqmx_sys::TaskHandle,
    plan: &PlanSetup,
) -> Result<(), String> {
    let source = plan.effective_sample_clock_source();
    let source = optional_cstring(source.as_deref())?;
    check_status(
        unsafe {
            ni_daqmx_sys::DAQmxCfgSampClkTiming(
                handle,
                cstr_ptr(source.as_ref()),
                plan.sample_rate_hz,
                ni_daqmx_sys::DAQmx_Val_Rising,
                ni_daqmx_sys::DAQmx_Val_FiniteSamps,
                plan.samples_per_channel,
            )
        },
        "DAQmxCfgSampClkTiming",
    )
}

fn cfg_implicit_timing(
    handle: ni_daqmx_sys::TaskHandle,
    samples_per_channel: u64,
) -> Result<(), String> {
    check_status(
        unsafe {
            ni_daqmx_sys::DAQmxCfgImplicitTiming(
                handle,
                ni_daqmx_sys::DAQmx_Val_FiniteSamps,
                samples_per_channel,
            )
        },
        "DAQmxCfgImplicitTiming",
    )
}

fn cfg_start_trigger_if_needed(
    handle: ni_daqmx_sys::TaskHandle,
    plan: &PlanSetup,
) -> Result<(), String> {
    let Some(trigger) = plan.start_trigger.as_deref() else {
        return Ok(());
    };
    let trigger_c = required_cstring("start trigger", trigger)?;
    check_status(
        unsafe {
            ni_daqmx_sys::DAQmxCfgDigEdgeStartTrig(
                handle,
                trigger_c.as_ptr(),
                ni_daqmx_sys::DAQmx_Val_Rising,
            )
        },
        "DAQmxCfgDigEdgeStartTrig",
    )
}

fn clear_all(mut tasks: Vec<Task>) -> Result<(), String> {
    let mut first_error = None;
    while let Some(task) = tasks.pop() {
        match task.clear() {
            Ok(name) => println!("cleared_task\t{name}"),
            Err(error) if first_error.is_none() => first_error = Some(error),
            Err(_) => {}
        }
    }
    if let Some(error) = first_error {
        Err(error)
    } else {
        Ok(())
    }
}

impl Task {
    fn clear(mut self) -> Result<String, String> {
        self.clear_inner()?;
        Ok(self.name.clone())
    }

    fn clear_inner(&mut self) -> Result<(), String> {
        if self.cleared || self.handle.is_null() {
            return Ok(());
        }
        let status = unsafe { ni_daqmx_sys::DAQmxClearTask(self.handle) };
        if status >= 0 {
            self.cleared = true;
            self.handle = ptr::null_mut();
            Ok(())
        } else {
            Err(format_status(status, "DAQmxClearTask"))
        }
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        let _ = self.clear_inner();
    }
}

fn print_usage() {
    eprintln!(
        "usage: numanager-daqmx-plan-setup-helper [--ao CHAN] [--do LINE] [--ai CHAN] [--ci CTR] [--co CTR] [--ao-task NAME] [--do-task NAME] [--ai-task NAME] [--ci-task NAME] [--co-task NAME] --sample-rate HZ --samples N [--width PX] [--height PX] [--frames N] [--signal-lines N] [--chunk-size N] [--sample-clock-source SRC] [--start-trigger SRC] [--min-volts V] [--max-volts V] [--timeout S] [--preflight-only] [--simulate-setup-error-after N]"
    );
    eprintln!("creates planned tasks, configures channels/timing/triggers, and clears all tasks");
    eprintln!("prints planned transfer operations but does not start tasks, write outputs, or read inputs");
    eprintln!(
        "--preflight-only exits after printing the flushed plan without calling NI-DAQmx task APIs"
    );
    eprintln!(
        "--simulate-setup-error-after requires --preflight-only and prints no-DAQmx partial-setup cleanup rows"
    );
}

fn required_arg(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_f64(value: &str) -> Result<f64, String> {
    value
        .parse()
        .map_err(|error| format!("invalid floating-point value {value:?}: {error}"))
}

fn parse_u64(value: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|error| format!("invalid integer value {value:?}: {error}"))
}

fn required_cstring(field: &str, value: &str) -> Result<CString, String> {
    CString::new(value).map_err(|_| format!("{field} contains an interior NUL byte"))
}

fn optional_cstring(value: Option<&str>) -> Result<Option<CString>, String> {
    value
        .map(|value| required_cstring("optional string", value))
        .transpose()
}

fn cstr_ptr(value: Option<&CString>) -> *const c_char {
    value.map(|value| value.as_ptr()).unwrap_or_else(ptr::null)
}

fn check_status(status: ni_daqmx_sys::int32, call: &str) -> Result<(), String> {
    if status < 0 {
        Err(format_status(status, call))
    } else {
        Ok(())
    }
}

fn format_status(status: ni_daqmx_sys::int32, call: &str) -> String {
    format!("{call} failed: {}", error_string(status))
}

fn error_string(status: ni_daqmx_sys::int32) -> String {
    let mut buffer = vec![0 as c_char; 4096];
    let extended_status = unsafe {
        ni_daqmx_sys::DAQmxGetExtendedErrorInfo(
            buffer.as_mut_ptr(),
            buffer.len() as ni_daqmx_sys::uInt32,
        )
    };
    if extended_status >= 0 {
        let message = unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        if !message.trim().is_empty() {
            return format!("DAQmx status {status}: {message}");
        }
    }

    let error_status = unsafe {
        ni_daqmx_sys::DAQmxGetErrorString(
            status,
            buffer.as_mut_ptr(),
            buffer.len() as ni_daqmx_sys::uInt32,
        )
    };
    if error_status >= 0 {
        let message = unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        format!("DAQmx status {status}: {message}")
    } else {
        format!(
            "DAQmx status {status}; DAQmxGetExtendedErrorInfo returned {extended_status}; DAQmxGetErrorString returned {error_status}"
        )
    }
}
