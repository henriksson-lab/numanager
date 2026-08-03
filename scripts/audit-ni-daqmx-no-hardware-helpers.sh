#!/usr/bin/env bash
set -euo pipefail

tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

run_ok() {
  local name=$1
  shift
  local output="$tmp_dir/${name}.out"
  printf 'running %s\n' "$name" >&2
  "$@" >"$output" 2>&1
  printf '%s\n' "$output"
}

run_fail() {
  local name=$1
  shift
  local output="$tmp_dir/${name}.out"
  printf 'running %s\n' "$name" >&2
  set +e
  "$@" >"$output" 2>&1
  local status=$?
  set -e
  if [[ $status -eq 0 ]]; then
    printf 'expected %s to fail, but it exited 0\n' "$name" >&2
    sed -n '1,120p' "$output" >&2
    exit 1
  fi
  printf '%s\n' "$output"
}

require_line() {
  local file=$1
  local pattern=$2
  local description=$3
  if ! rg -F -- "$pattern" "$file" >/dev/null; then
    printf 'missing %s in %s: %s\n' "$description" "$file" "$pattern" >&2
    printf '\n--- output ---\n' >&2
    sed -n '1,160p' "$file" >&2
    exit 1
  fi
}

require_absent() {
  local file=$1
  local pattern=$2
  local description=$3
  if rg -F -- "$pattern" "$file" >/dev/null; then
    printf 'unexpected %s in %s: %s\n' "$description" "$file" "$pattern" >&2
    printf '\n--- output ---\n' >&2
    sed -n '1,160p' "$file" >&2
    exit 1
  fi
}

build=$(run_ok helper_build cargo build -p numanager-imswitch-daqmx --features ni-daqmx-sdk --bins)
require_line "$build" 'Finished' 'helper build completion'

lifecycle=$(run_ok lifecycle_dry target/debug/numanager-daqmx-task-lifecycle-helper --dry-run)
require_line "$lifecycle" 'task_lifecycle_plan	true' 'lifecycle dry-run plan marker'
require_line "$lifecycle" 'execute	false' 'lifecycle dry-run execution gate'
require_line "$lifecycle" 'created_task	false' 'lifecycle no task creation'
require_line "$lifecycle" 'cleared_task	false' 'lifecycle no clear call'

lifecycle_start=$(run_ok lifecycle_start_dry target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --start --wait-seconds 0.250000)
require_line "$lifecycle_start" 'planned_api	DAQmxCreateTask,DAQmxStartTask,DAQmxWaitUntilTaskDone,DAQmxStopTask,DAQmxClearTask' 'lifecycle start/wait plan'
require_line "$lifecycle_start" 'started_task	false' 'lifecycle start dry-run gate'
require_line "$lifecycle_start" 'waited_until_done	false' 'lifecycle wait dry-run gate'

lifecycle_cleanup=$(run_ok lifecycle_cleanup_sim target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --start --wait-seconds 0.250000 --simulate-error-after-start)
require_line "$lifecycle_cleanup" 'simulated_failure	true' 'lifecycle simulated failure marker'
require_line "$lifecycle_cleanup" 'cleanup_after_lifecycle_error	true' 'lifecycle cleanup marker'
require_line "$lifecycle_cleanup" 'stopped_task_after_error	simulated_no_task' 'lifecycle simulated stop marker'

signal_plan=$(run_ok signal_preflight target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1024 --signal-lines 1 --chunk-size 256 --ci Dev1/ctr0 --ai Dev1/ai0 --timeout 10 --preflight-only)
require_line "$signal_plan" 'preflight_plan	true' 'signal preflight marker'
require_line "$signal_plan" 'planned_timing	counter_input	sample_clock	source=<empty>	rate_hz=100000.000000	edge=Rising	mode=FiniteSamps	samples_per_channel=1024' 'signal CI sample-clock timing plan'
require_line "$signal_plan" 'planned_timing	analog_input	sample_clock	source=<empty>	rate_hz=100000.000000	edge=Rising	mode=FiniteSamps	samples_per_channel=1024' 'signal AI sample-clock timing plan'
require_line "$signal_plan" 'signal_timing_preview	sample_period_s=0.000010000	samples_per_line=1024	lines=1	line_period_s=0.010240000	chunk_size=256	chunk_period_s=0.002560000	total_period_s=0.010240000	evidence=pending_hardware_validation' 'signal timing preview'
require_line "$signal_plan" 'planned_runtime_sequence	step=3	phase=start	tasks=counter_input,analog_input	basis=inputs_outputs_then_clock	evidence=pending_hardware_validation' 'signal runtime start sequence'
require_line "$signal_plan" 'planned_runtime_sequence	step=4	phase=read	tasks=counter_input,analog_input	basis=finite_samples	evidence=pending_hardware_validation' 'signal runtime read sequence'
require_line "$signal_plan" 'planned_execution_contract	mode=signal_finite	write=none	read=counter_input,analog_input	wait=none	write_policy=buffered_before_start	write_auto_start=false	write_layout=GroupByScanNumber	read_policy=finite_expected_samples	read_layout=GroupByScanNumber_for_analog_input	timeout_s=10.000000	publication_policy=publish_only_after_validated_read_and_reconstruction	evidence=pending_hardware_validation' 'signal execution contract preflight'
require_line "$signal_plan" 'planned_live_executor	mode=signal_finite	status=not_enabled_pending_hardware_validation	backend=ni_daqmx_sdk_task_wrapper	target_scope=linux_windows_optional_sdk_backend	required_validation=legal_review,installed_header_audit,ni_pal_device_inventory,bench_safety_preconditions,task_ordering_routing_completion_cleanup_bench_validation,runtime_publication_hardware_validation,hardware_validation_note	evidence=pending_hardware_validation' 'signal live executor preflight'
require_line "$signal_plan" 'planned_live_executor_phase	step=5	phase=read	tasks=counter_input,analog_input	api_surface=DAQmxReadCounterU32+DAQmxReadAnalogF64_finite_expected_samples	evidence=pending_hardware_validation' 'signal live executor read phase'
require_line "$signal_plan" 'planned_completion	mode=finite	samples_per_channel=1024	timeout_s=10.000000	evidence=pending_hardware_validation' 'signal finite completion plan'
require_line "$signal_plan" 'planned_publication	event=ScanSignalChunk	mode=raw_signal_chunks	channels=counter_input,analog_input	samples_per_line=1024	lines=1	chunk_size=256	required_metadata=stream,channel_names,timing_origin,line_index,chunk_index,first_sample_index,sample_count,sample_values,sample_rate,sample_period,dropped_samples,dropped_chunks,overflowed	evidence=pending_hardware_validation' 'signal publication preflight plan'
require_line "$signal_plan" 'planned_cleanup	failure_modes=partial_setup_failure,post_start_failure,buffered_write_failure,finite_read_failure,counter_output_wait_timeout	started_task_cleanup=stop_started_tasks_before_clear	safe_output_state=pending_hardware_validation	evidence=pending_hardware_validation' 'signal cleanup failure-mode preflight plan'
require_line "$signal_plan" 'planned_cleanup_order	stop=analog_input,counter_input	clear=analog_input,counter_input	timeout_s=10.000000	evidence=pending_hardware_validation' 'signal cleanup order preflight plan'
require_line "$signal_plan" 'preflight_only	true' 'signal preflight-only gate'
require_line "$signal_plan" 'created_tasks	0' 'signal preflight no task creation'
require_line "$signal_plan" 'read_input	false' 'signal preflight no input reads'

raster_plan=$(run_ok raster_preflight target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 262144 --width 512 --height 512 --frames 1 --ao-task ao_scan --do-task do_laser_gate --ci-task ci_detector --co-task co_sample_clock --ao Dev1/ao0 --ao Dev1/ao1 --do Dev1/port0/line0 --ci Dev1/ctr0 --co Dev1/ctr2 --min-volts -10 --max-volts 10 --timeout 10 --preflight-only)
require_line "$raster_plan" 'planned_task	ao_scan	analog_output	Dev1/ao0,Dev1/ao1' 'raster AO task plan'
require_line "$raster_plan" 'planned_start_order	ci_detector,ao_scan,do_laser_gate,co_sample_clock' 'raster start order plan'
require_line "$raster_plan" 'sample_clock_source_origin	derived_counter_output_internal' 'raster derived sample-clock source origin'
require_line "$raster_plan" 'planned_sample_clock_route	source=/Dev1/Ctr2InternalOutput	producer=co_sample_clock	consumers=ci_detector,ao_scan,do_laser_gate	edge=Rising' 'raster derived sample-clock route plan'
require_line "$raster_plan" 'planned_timing	ci_detector	sample_clock	source=/Dev1/Ctr2InternalOutput	rate_hz=100000.000000	edge=Rising	mode=FiniteSamps	samples_per_channel=262144' 'raster CI sample-clock timing plan'
require_line "$raster_plan" 'planned_timing	ao_scan	sample_clock	source=/Dev1/Ctr2InternalOutput	rate_hz=100000.000000	edge=Rising	mode=FiniteSamps	samples_per_channel=262144' 'raster AO sample-clock timing plan'
require_line "$raster_plan" 'planned_timing	do_laser_gate	sample_clock	source=/Dev1/Ctr2InternalOutput	rate_hz=100000.000000	edge=Rising	mode=FiniteSamps	samples_per_channel=262144' 'raster DO sample-clock timing plan'
require_line "$raster_plan" 'planned_timing	co_sample_clock	implicit	mode=FiniteSamps	samples_per_channel=262144	pulse_frequency_hz=100000.000000	idle_state=Low	duty_cycle=0.500000' 'raster CO implicit timing plan'
require_line "$raster_plan" 'raster_timing_preview	pixel_period_s=0.000010000	line_period_s=0.005120000	frame_period_s=2.621440000	total_period_s=2.621440000	evidence=pending_hardware_validation' 'raster timing preview'
require_line "$raster_plan" 'waveform_preview	ao_scan	analog_output	pattern=x_fast_sawtooth_y_slow_step	samples=0:x=-10.000,y=-10.000|131328:x=0.020,y=0.020|262143:x=10.000,y=10.000	evidence=pending_hardware_validation' 'raster AO waveform preview'
require_line "$raster_plan" 'waveform_preview	do_laser_gate	digital_output	pattern=high_during_active_pixels	samples=0:gate=1|131328:gate=1|262143:gate=1	evidence=pending_hardware_validation' 'raster DO waveform preview'
require_line "$raster_plan" 'planned_runtime_sequence	step=2	phase=write	tasks=ao_scan,do_laser_gate	basis=buffered_output_before_start	evidence=pending_hardware_validation' 'raster runtime write sequence'
require_line "$raster_plan" 'planned_runtime_sequence	step=3	phase=start	tasks=ci_detector,ao_scan,do_laser_gate,co_sample_clock	basis=inputs_outputs_then_clock	evidence=pending_hardware_validation' 'raster runtime start sequence'
require_line "$raster_plan" 'planned_runtime_sequence	step=5	phase=wait	tasks=co_sample_clock	basis=counter_output_done_or_timeout	evidence=pending_hardware_validation' 'raster runtime wait sequence'
require_line "$raster_plan" 'planned_execution_contract	mode=raster_finite	write=ao_scan,do_laser_gate	read=ci_detector	wait=co_sample_clock	write_policy=buffered_before_start	write_auto_start=false	write_layout=GroupByScanNumber	read_policy=finite_expected_samples	read_layout=GroupByScanNumber_for_analog_input	timeout_s=10.000000	publication_policy=publish_only_after_validated_read_and_reconstruction	evidence=pending_hardware_validation' 'raster execution contract preflight'
require_line "$raster_plan" 'planned_live_executor	mode=raster_finite	status=not_enabled_pending_hardware_validation	backend=ni_daqmx_sdk_task_wrapper	target_scope=linux_windows_optional_sdk_backend	required_validation=legal_review,installed_header_audit,ni_pal_device_inventory,bench_safety_preconditions,task_ordering_routing_completion_cleanup_bench_validation,runtime_publication_hardware_validation,hardware_validation_note	evidence=pending_hardware_validation' 'raster live executor preflight'
require_line "$raster_plan" 'planned_live_executor_phase	step=3	phase=write	tasks=ao_scan,do_laser_gate	api_surface=DAQmxWriteAnalogF64+DAQmxWriteDigitalLines_buffered_auto_start_false	evidence=pending_hardware_validation' 'raster live executor write phase'
require_line "$raster_plan" 'planned_live_executor_phase	step=6	phase=wait	tasks=co_sample_clock	api_surface=DAQmxWaitUntilTaskDone	evidence=pending_hardware_validation' 'raster live executor wait phase'
require_line "$raster_plan" 'planned_completion	mode=finite	samples_per_channel=262144	timeout_s=10.000000	evidence=pending_hardware_validation' 'raster finite completion plan'
require_line "$raster_plan" 'planned_reconstruction	mode=one_detector_sample_per_pixel	input=ci_detector	scan=512x512	frames=1	reconstruction=512x512	pixel_format=pending_runtime_reconstruction	sample_to_pixel_mapping=row_major_unidirectional_one_sample_per_pixel	accumulation=sum_samples_per_reconstructed_pixel	background_subtraction=disabled_until_hardware_validated	saturation_policy=clip_to_pixel_format_and_report_saturated_pixels	publication_gate=publish_after_validated_read_and_reconstruction	evidence=pending_hardware_validation' 'raster reconstruction preflight plan'
require_line "$raster_plan" 'planned_publication	event=FrameReady	mode=raster_frame_payload	scan=512x512	frames=1	pixel_format=pending_runtime_reconstruction	required_metadata=frame_handle,stream,scan_width,scan_height,reconstruction_width,reconstruction_height,reconstruction_pixel_size,sample_rate,line_dwell,detectors,saturated_pixels,progress_status	evidence=pending_hardware_validation' 'raster publication preflight plan'
require_line "$raster_plan" 'planned_cleanup	failure_modes=partial_setup_failure,post_start_failure,buffered_write_failure,finite_read_failure,counter_output_wait_timeout	started_task_cleanup=stop_started_tasks_before_clear	safe_output_state=pending_hardware_validation	evidence=pending_hardware_validation' 'raster cleanup failure-mode preflight plan'
require_line "$raster_plan" 'planned_cleanup_order	stop=co_sample_clock,do_laser_gate,ao_scan,ci_detector	clear=co_sample_clock,ci_detector,do_laser_gate,ao_scan	timeout_s=10.000000	evidence=pending_hardware_validation' 'raster cleanup order preflight plan'
require_line "$raster_plan" 'preflight_only	true' 'raster preflight-only gate'
require_line "$raster_plan" 'created_tasks	0' 'raster preflight no task creation'
require_line "$raster_plan" 'wrote_output	false' 'raster preflight no output writes'

plan_cleanup=$(run_ok plan_cleanup_sim target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1024 --signal-lines 1 --chunk-size 256 --ci-task ci_signal --ai-task ai_signal --ci Dev1/ctr0 --ai Dev1/ai0 --timeout 10 --preflight-only --simulate-setup-error-after 1)
require_line "$plan_cleanup" 'preflight_only	true' 'plan cleanup simulation preflight gate'
require_line "$plan_cleanup" 'simulated_failure	true' 'plan setup simulated failure marker'
require_line "$plan_cleanup" 'cleared_partial_task	ci_signal' 'plan setup simulated partial clear marker'
require_line "$plan_cleanup" 'cleanup_after_setup_error	true' 'plan setup cleanup marker'
require_line "$plan_cleanup" 'started_tasks	false' 'plan setup cleanup no start marker'
require_line "$plan_cleanup" 'wrote_output	false' 'plan setup cleanup no output writes'
require_line "$plan_cleanup" 'read_input	false' 'plan setup cleanup no input reads'

for kind_channel in ai:Dev1/ai0 ao:Dev1/ao0 ci:Dev1/ctr0 co:Dev1/ctr2 do:Dev1/port0/line0; do
  kind=${kind_channel%%:*}
  channel=${kind_channel#*:}
  out=$(run_ok "channel_${kind}_dry" target/debug/numanager-daqmx-channel-setup-helper --kind "$kind" --channel "$channel" --dry-run)
  require_line "$out" 'channel_setup_plan	true' "channel $kind plan marker"
  require_line "$out" 'execute	false' "channel $kind dry-run gate"
  require_line "$out" 'created_task	false' "channel $kind no task creation"
  require_line "$out" 'configured_channel	false' "channel $kind no channel creation"
done

for smoke in \
  'ai Dev1/ai0 --samples 1' \
  'ao Dev1/ao0 --volts 0' \
  'ci Dev1/ctr0 --samples 1' \
  'co Dev1/ctr2 --frequency 10 --samples 1' \
  'do Dev1/port0/line0 --line-state false'
do
  read -r kind channel rest <<<"$smoke"
  out=$(run_ok "io_${kind}_dry" target/debug/numanager-daqmx-io-smoke-helper --kind "$kind" --channel "$channel" $rest)
  require_line "$out" 'io_smoke_plan	true' "I/O $kind plan marker"
  require_line "$out" 'execute	false' "I/O $kind execution gate"
  require_line "$out" 'bench_safety_reviewed	false' "I/O $kind safety acknowledgement gate"
  require_line "$out" 'created_task	false' "I/O $kind no task creation"
  require_line "$out" 'wrote_output	false' "I/O $kind no output write"
  require_line "$out" 'read_input	false' "I/O $kind no input read"
  case "$kind" in
    ao)
      require_line "$out" 'final_safe_state	0.000000 V before clear' 'I/O AO safe final state marker'
      ;;
    do)
      require_line "$out" 'final_safe_state	low before clear' 'I/O DO safe final state marker'
      ;;
    co)
      require_line "$out" 'final_safe_state	idle_state=low after stop' 'I/O CO idle final state marker'
      ;;
  esac
done

io_cleanup=$(run_ok io_cleanup_sim target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1 --simulate-error-after-start)
require_line "$io_cleanup" 'simulated_failure	true' 'I/O simulated failure marker'
require_line "$io_cleanup" 'cleanup_after_io_error	true' 'I/O cleanup marker'
require_line "$io_cleanup" 'stopped_task_after_error	simulated_no_task' 'I/O simulated stop marker'

invalid_io_execute_safety=$(run_fail invalid_io_execute_safety target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --volts 0 --execute)
require_line "$invalid_io_execute_safety" '--execute requires --bench-safety-reviewed after completing the bench safety preconditions' 'invalid I/O execute safety acknowledgement guard'

invalid_wait=$(run_fail invalid_wait_seconds target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --wait-seconds NaN)
require_line "$invalid_wait" '--wait-seconds must be finite and non-negative' 'invalid wait guard'

invalid_lifecycle_empty_name=$(run_fail invalid_lifecycle_empty_name target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --name '')
require_line "$invalid_lifecycle_empty_name" '--name must not be empty; use --unnamed for a null DAQmx task name' 'invalid lifecycle empty task-name guard'

invalid_lifecycle_space_name=$(run_fail invalid_lifecycle_space_name target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --name ' lifecycle ')
require_line "$invalid_lifecycle_space_name" '--name must not have leading or trailing whitespace' 'invalid lifecycle task-name whitespace guard'

invalid_lifecycle_cleanup_mode=$(run_fail invalid_lifecycle_cleanup_mode target/debug/numanager-daqmx-task-lifecycle-helper --simulate-error-after-start)
require_line "$invalid_lifecycle_cleanup_mode" '--simulate-error-after-start requires --dry-run' 'invalid lifecycle cleanup simulation mode guard'

invalid_lifecycle_cleanup_start=$(run_fail invalid_lifecycle_cleanup_start target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --simulate-error-after-start)
require_line "$invalid_lifecycle_cleanup_start" '--simulate-error-after-start requires --start' 'invalid lifecycle cleanup simulation start guard'

invalid_plan_cleanup_mode=$(run_fail invalid_plan_cleanup_mode target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --simulate-setup-error-after 1)
require_line "$invalid_plan_cleanup_mode" '--simulate-setup-error-after requires --preflight-only' 'invalid plan setup cleanup simulation mode guard'

invalid_plan_cleanup_count=$(run_fail invalid_plan_cleanup_count target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --preflight-only --simulate-setup-error-after 0)
require_line "$invalid_plan_cleanup_count" '--simulate-setup-error-after must be positive' 'invalid plan setup cleanup simulation count guard'

invalid_signal_lines=$(run_fail invalid_signal_lines target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --signal-lines 0 --ci Dev1/ctr0 --preflight-only)
require_line "$invalid_signal_lines" '--signal-lines must be positive' 'invalid signal line-count guard'

invalid_signal_line_division=$(run_fail invalid_signal_line_division target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 5 --signal-lines 2 --ci Dev1/ctr0 --preflight-only)
require_line "$invalid_signal_line_division" '--samples must be divisible by --signal-lines' 'invalid signal line division guard'

invalid_chunk_without_signal_lines=$(run_fail invalid_chunk_without_signal_lines target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --chunk-size 1 --ci Dev1/ctr0 --preflight-only)
require_line "$invalid_chunk_without_signal_lines" '--chunk-size requires --signal-lines for signal timing preview' 'invalid chunk metadata guard'

invalid_chunk_size=$(run_fail invalid_chunk_size target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --signal-lines 1 --chunk-size 2 --ci Dev1/ctr0 --preflight-only)
require_line "$invalid_chunk_size" '--chunk-size must not exceed --samples' 'invalid chunk size guard'

invalid_plan_no_channels=$(run_fail invalid_plan_no_channels target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --preflight-only)
require_line "$invalid_plan_no_channels" 'at least one --ao, --do, --ai, --ci, or --co channel is required' 'invalid plan no-channel guard'

invalid_sample_rate=$(run_fail invalid_sample_rate target/debug/numanager-daqmx-plan-setup-helper --sample-rate NaN --samples 1 --ci Dev1/ctr0 --preflight-only)
require_line "$invalid_sample_rate" '--sample-rate must be positive and finite' 'invalid sample-rate guard'

invalid_zero_sample_rate=$(run_fail invalid_zero_sample_rate target/debug/numanager-daqmx-plan-setup-helper --sample-rate 0 --samples 1 --ci Dev1/ctr0 --preflight-only)
require_line "$invalid_zero_sample_rate" '--sample-rate must be positive and finite' 'invalid positive sample-rate guard'

invalid_timeout=$(run_fail invalid_plan_timeout target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --timeout NaN --preflight-only)
require_line "$invalid_timeout" '--timeout must be positive and finite' 'invalid timeout guard'

invalid_plan_zero_timeout=$(run_fail invalid_plan_zero_timeout target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --timeout 0 --preflight-only)
require_line "$invalid_plan_zero_timeout" '--timeout must be positive and finite' 'invalid positive timeout guard'

invalid_plan_empty_clock_source=$(run_fail invalid_plan_empty_clock_source target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --sample-clock-source '' --preflight-only)
require_line "$invalid_plan_empty_clock_source" '--sample-clock-source must not be empty when supplied' 'invalid empty sample-clock source guard'

invalid_plan_clock_source_whitespace=$(run_fail invalid_plan_clock_source_whitespace target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --sample-clock-source ' /Dev1/Ctr0InternalOutput ' --preflight-only)
require_line "$invalid_plan_clock_source_whitespace" '--sample-clock-source must not have leading or trailing whitespace' 'invalid sample-clock source whitespace guard'

invalid_plan_empty_start_trigger=$(run_fail invalid_plan_empty_start_trigger target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --start-trigger '' --preflight-only)
require_line "$invalid_plan_empty_start_trigger" '--start-trigger must not be empty when supplied' 'invalid empty start-trigger guard'

invalid_plan_start_trigger_whitespace=$(run_fail invalid_plan_start_trigger_whitespace target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --start-trigger ' /Dev1/PFI0 ' --preflight-only)
require_line "$invalid_plan_start_trigger_whitespace" '--start-trigger must not have leading or trailing whitespace' 'invalid start-trigger whitespace guard'

invalid_plan_empty_channel=$(run_fail invalid_plan_empty_channel target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci '' --preflight-only)
require_line "$invalid_plan_empty_channel" '--ci values must not be empty' 'invalid empty physical channel guard'

invalid_plan_channel_whitespace=$(run_fail invalid_plan_channel_whitespace target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci ' Dev1/ctr0 ' --preflight-only)
require_line "$invalid_plan_channel_whitespace" '--ci must not have leading or trailing whitespace' 'invalid physical channel whitespace guard'

invalid_plan_empty_task_label=$(run_fail invalid_plan_empty_task_label target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --ci-task '' --preflight-only)
require_line "$invalid_plan_empty_task_label" '--ci-task must not be empty' 'invalid empty task label guard'

invalid_plan_task_label_whitespace=$(run_fail invalid_plan_task_label_whitespace target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --ci-task ' signal ' --preflight-only)
require_line "$invalid_plan_task_label_whitespace" '--ci-task must not have leading or trailing whitespace' 'invalid task-label whitespace guard'

invalid_plan_duplicate_channel=$(run_fail invalid_plan_duplicate_channel target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --co Dev1/ctr0 --preflight-only)
require_line "$invalid_plan_duplicate_channel" 'physical channels must be unique within a plan; "Dev1/ctr0" is used by --ci and --co' 'invalid duplicate physical channel guard'

invalid_plan_duplicate_task_label=$(run_fail invalid_plan_duplicate_task_label target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --ai Dev1/ai0 --ci-task signal --ai-task signal --preflight-only)
require_line "$invalid_plan_duplicate_task_label" 'active task labels must be unique; "signal" is used by --ai-task and --ci-task' 'invalid duplicate task label guard'

invalid_plan_zero_samples=$(run_fail invalid_plan_zero_samples target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 0 --ci Dev1/ctr0 --preflight-only)
require_line "$invalid_plan_zero_samples" '--samples must be positive' 'invalid plan positive sample-count guard'

invalid_samples=$(run_fail invalid_plan_samples target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 2147483648 --ci Dev1/ctr0 --preflight-only)
require_line "$invalid_samples" '--samples exceeds conservative helper i32 sample count range' 'invalid sample-count guard'

invalid_transfer=$(run_fail invalid_transfer_elements target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1073741824 --ao Dev1/ao0 --ao Dev1/ao1 --preflight-only)
require_line "$invalid_transfer" 'analog output transfer element count 2147483648 exceeds conservative helper i32 range' 'invalid transfer element guard'

invalid_plan_analog_finite=$(run_fail invalid_plan_analog_finite target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ao Dev1/ao0 --min-volts NaN --max-volts 1 --preflight-only)
require_line "$invalid_plan_analog_finite" '--min-volts and --max-volts must be finite' 'invalid plan analog finite guard'

invalid_plan_analog_range=$(run_fail invalid_plan_analog_range target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ao Dev1/ao0 --min-volts 1 --max-volts -1 --preflight-only)
require_line "$invalid_plan_analog_range" '--min-volts must not exceed --max-volts' 'invalid plan analog range guard'

invalid_raster=$(run_fail invalid_raster_product target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 3 --width 2 --height 2 --frames 1 --ci Dev1/ctr0 --preflight-only)
require_line "$invalid_raster" 'must match --width * --height * --frames' 'invalid raster product guard'

invalid_raster_partial=$(run_fail invalid_raster_partial target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 1 --ci Dev1/ctr0 --preflight-only)
require_line "$invalid_raster_partial" '--width, --height, and --frames must be supplied together; missing --height, --frames' 'invalid partial raster guard'

invalid_raster_overflow=$(run_fail invalid_raster_overflow target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 18446744073709551615 --height 2 --frames 1 --ci Dev1/ctr0 --preflight-only)
require_line "$invalid_raster_overflow" '--width * --height overflows u64' 'invalid raster overflow guard'

invalid_raster_frame_overflow=$(run_fail invalid_raster_frame_overflow target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 2 --height 2 --frames 4611686018427387904 --ci Dev1/ctr0 --preflight-only)
require_line "$invalid_raster_frame_overflow" '--width * --height * --frames overflows u64' 'invalid raster frame-product overflow guard'

invalid_raster_width=$(run_fail invalid_raster_width target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 0 --height 1 --frames 1 --ci Dev1/ctr0 --preflight-only)
require_line "$invalid_raster_width" '--width must be positive' 'invalid raster width guard'

invalid_raster_height=$(run_fail invalid_raster_height target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 1 --height 0 --frames 1 --ci Dev1/ctr0 --preflight-only)
require_line "$invalid_raster_height" '--height must be positive' 'invalid raster height guard'

invalid_raster_frames=$(run_fail invalid_raster_frames target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 1 --height 1 --frames 0 --ci Dev1/ctr0 --preflight-only)
require_line "$invalid_raster_frames" '--frames must be positive' 'invalid raster frame-count guard'

invalid_channel_setup_empty_name=$(run_fail invalid_channel_setup_empty_name target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --name '' --dry-run)
require_line "$invalid_channel_setup_empty_name" '--name must not be empty; use --unnamed for a null DAQmx task name' 'invalid channel-setup empty task-name guard'

invalid_channel_setup_space_name=$(run_fail invalid_channel_setup_space_name target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --name ' channel-setup ' --dry-run)
require_line "$invalid_channel_setup_space_name" '--name must not have leading or trailing whitespace' 'invalid channel-setup task-name whitespace guard'

invalid_channel_setup_empty_channel=$(run_fail invalid_channel_setup_empty_channel target/debug/numanager-daqmx-channel-setup-helper --kind co --channel '' --dry-run)
require_line "$invalid_channel_setup_empty_channel" '--channel must not be empty' 'invalid channel-setup empty channel guard'

invalid_channel_setup_space_channel=$(run_fail invalid_channel_setup_space_channel target/debug/numanager-daqmx-channel-setup-helper --kind co --channel ' Dev1/ctr2 ' --dry-run)
require_line "$invalid_channel_setup_space_channel" '--channel must not have leading or trailing whitespace' 'invalid channel-setup channel whitespace guard'

invalid_frequency=$(run_fail invalid_frequency target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --frequency inf --dry-run)
require_line "$invalid_frequency" '--frequency must be positive and finite' 'invalid frequency guard'

invalid_zero_frequency=$(run_fail invalid_zero_frequency target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --frequency 0 --dry-run)
require_line "$invalid_zero_frequency" '--frequency must be positive and finite' 'invalid positive frequency guard'

invalid_duty_cycle_finite=$(run_fail invalid_duty_cycle_finite target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --duty-cycle NaN --dry-run)
require_line "$invalid_duty_cycle_finite" '--duty-cycle must be finite and between 0.0 and 1.0' 'invalid finite duty-cycle guard'

invalid_duty_cycle=$(run_fail invalid_duty_cycle target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --duty-cycle 1.5 --dry-run)
require_line "$invalid_duty_cycle" '--duty-cycle must be finite and between 0.0 and 1.0' 'invalid duty-cycle guard'

invalid_io_frequency=$(run_fail invalid_io_frequency target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --frequency inf --samples 1)
require_line "$invalid_io_frequency" '--frequency must be positive and finite' 'invalid I/O frequency guard'

invalid_io_zero_frequency=$(run_fail invalid_io_zero_frequency target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --frequency 0 --samples 1)
require_line "$invalid_io_zero_frequency" '--frequency must be positive and finite' 'invalid positive I/O frequency guard'

invalid_io_duty_cycle_finite=$(run_fail invalid_io_duty_cycle_finite target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --duty-cycle NaN --samples 1)
require_line "$invalid_io_duty_cycle_finite" '--duty-cycle must be finite and between 0.0 and 1.0' 'invalid finite I/O duty-cycle guard'

invalid_io_duty_cycle=$(run_fail invalid_io_duty_cycle target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --duty-cycle 1.5 --samples 1)
require_line "$invalid_io_duty_cycle" '--duty-cycle must be finite and between 0.0 and 1.0' 'invalid I/O duty-cycle guard'

invalid_io_empty_channel=$(run_fail invalid_io_empty_channel target/debug/numanager-daqmx-io-smoke-helper --kind co --channel '' --samples 1)
require_line "$invalid_io_empty_channel" '--channel must not be empty' 'invalid I/O empty channel guard'

invalid_io_empty_name=$(run_fail invalid_io_empty_name target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --name '' --samples 1)
require_line "$invalid_io_empty_name" '--name must not be empty; use --unnamed for a null DAQmx task name' 'invalid I/O empty task-name guard'

invalid_io_space_name=$(run_fail invalid_io_space_name target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --name ' io-smoke ' --samples 1)
require_line "$invalid_io_space_name" '--name must not have leading or trailing whitespace' 'invalid I/O task-name whitespace guard'

invalid_analog_finite=$(run_fail invalid_analog_finite target/debug/numanager-daqmx-channel-setup-helper --kind ai --channel Dev1/ai0 --min-volts NaN --max-volts 1 --dry-run)
require_line "$invalid_analog_finite" '--min-volts and --max-volts must be finite' 'invalid analog finite guard'

invalid_analog_range=$(run_fail invalid_analog_range target/debug/numanager-daqmx-channel-setup-helper --kind ai --channel Dev1/ai0 --min-volts 1 --max-volts -1 --dry-run)
require_line "$invalid_analog_range" '--min-volts must not exceed --max-volts' 'invalid analog channel range guard'

invalid_io_timeout=$(run_fail invalid_io_timeout target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1 --timeout NaN)
require_line "$invalid_io_timeout" '--timeout must be positive and finite' 'invalid I/O timeout guard'

invalid_io_zero_timeout=$(run_fail invalid_io_zero_timeout target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1 --timeout 0)
require_line "$invalid_io_zero_timeout" '--timeout must be positive and finite' 'invalid positive I/O timeout guard'

invalid_io_samples=$(run_fail invalid_io_samples target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 0)
require_line "$invalid_io_samples" '--samples must be positive' 'invalid I/O sample-count guard'

invalid_io_sample_range=$(run_fail invalid_io_sample_range target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 2147483648)
require_line "$invalid_io_sample_range" '--samples exceeds NI-DAQmx i32 sample count range' 'invalid I/O sample range guard'

invalid_io_analog_finite=$(run_fail invalid_io_analog_finite target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --min-volts NaN --max-volts 1 --volts 0)
require_line "$invalid_io_analog_finite" '--min-volts and --max-volts must be finite' 'invalid I/O analog finite guard'

invalid_io_analog_range=$(run_fail invalid_io_analog_range target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --min-volts 1 --max-volts -1 --volts 0)
require_line "$invalid_io_analog_range" '--min-volts must not exceed --max-volts' 'invalid I/O analog range guard'

invalid_volts=$(run_fail invalid_volts target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --volts NaN)
require_line "$invalid_volts" '--volts must be finite' 'invalid voltage guard'

invalid_ao_range=$(run_fail invalid_ao_range target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --min-volts -1 --max-volts 1 --volts 2)
require_line "$invalid_ao_range" '--volts must be inside --min-volts/--max-volts for AO' 'invalid AO setpoint range guard'

invalid_ao_safe_zero=$(run_fail invalid_ao_safe_zero target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --min-volts 1 --max-volts 5 --volts 2)
require_line "$invalid_ao_safe_zero" '--min-volts/--max-volts must include 0.0 for AO safe final write' 'invalid AO safe-zero range guard'

invalid_io_cleanup_kind=$(run_fail invalid_io_cleanup_kind target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --simulate-error-after-start)
require_line "$invalid_io_cleanup_kind" '--simulate-error-after-start is supported only for ai, ci, or co' 'invalid I/O cleanup simulation kind guard'

for output in "$tmp_dir"/*.out; do
  require_absent "$output" 'execute	true' 'live execution marker in no-hardware audit'
done

printf '# NI-DAQmx No-Hardware Helper Audit\n\n'
printf '| Workflow | Status |\n'
printf '| --- | --- |\n'
printf '| SDK-feature helper build | ok |\n'
printf '| Task lifecycle dry run | ok |\n'
printf '| Task lifecycle cleanup simulation | ok |\n'
printf '| Raster/signal plan preflight | ok |\n'
printf '| Plan setup cleanup simulation | ok |\n'
printf '| Channel setup dry runs | ok |\n'
printf '| I/O smoke dry runs | ok |\n'
printf '| I/O cleanup simulation | ok |\n'
printf '| Invalid numeric/range/transfer/raster/signal guards | ok |\n'
printf '\nThis audit runs only helper build, dry-run, preflight-only, simulated-cleanup, and invalid-input paths. It does not execute NI-DAQmx tasks, write outputs, read inputs, or provide hardware evidence.\n'
