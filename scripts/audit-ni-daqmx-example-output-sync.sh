#!/usr/bin/env bash
set -euo pipefail

tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

run_example() {
  local name=$1
  shift
  local output="$tmp_dir/${name}.out"
  printf 'running %s\n' "$name" >&2
  "$@" >"$output" 2>&1
  printf '%s\n' "$output"
}

require_line() {
  local file=$1
  local pattern=$2
  local description=$3
  if ! rg -F -- "$pattern" "$file" >/dev/null; then
    printf 'missing %s in %s: %s\n' "$description" "$file" "$pattern" >&2
    printf '\n--- output ---\n' >&2
    sed -n '1,200p' "$file" >&2
    exit 1
  fi
}

bringup=$(run_example lsm_daqmx_bringup_plan cargo run -p numanager-examples -- lsm_daqmx_bringup_plan)
note=$(run_example lsm_daqmx_validation_note cargo run -p numanager-examples -- lsm_daqmx_validation_note)

for marker in \
  'Installed target-platform NIDAQmx.h used for bindgen' \
  'Bindgen regeneration command' \
  'Bindgen regeneration command and FFI-source inventory from the same installed target-platform NIDAQmx.h' \
  'Passing header inventory, recorded bindgen regeneration command, and bindgen-source audit from the same installed Linux or Windows 26.5 target-platform NIDAQmx.h before publishing regenerated 26.5 bindings'
do
  require_line "$note" "$marker" "validation-note emitted $marker"
  require_line docs/example_outputs.md "$marker" "recorded docs include $marker"
done

for command in \
  'scripts/audit-ni-daqmx-external-gates.sh' \
  'scripts/audit-ni-daqmx-target-scope.sh' \
  'scripts/audit-ni-daqmx-no-hardware-helpers.sh' \
  'scripts/audit-ni-daqmx-plan-validation.sh' \
  'scripts/audit-ni-daqmx-live-gate.sh' \
  'scripts/audit-ni-daqmx-runtime-probe.sh'
do
  require_line "$bringup" "$command" "bring-up emitted $command"
  require_line "$note" "$command" "validation-note emitted $command"
  require_line docs/example_outputs.md "$command" "recorded docs include $command"
done

bringup_backend_readiness='backend_readiness: execution=not_live_backend; live_ready=false; live_requested=false; blocker=feature_ni_daqmx_sdk'
require_line "$bringup" "$bringup_backend_readiness" 'bring-up emitted backend readiness summary'
require_line docs/example_outputs.md "$bringup_backend_readiness" 'recorded docs include bring-up backend readiness summary'
require_line "$bringup" 'runtime_version=not_configured(matches=unknown,basis=configured_runtime_version_missing)' 'bring-up emitted runtime-version readiness summary'
require_line docs/example_outputs.md 'runtime_version=not_configured(matches=unknown,basis=configured_runtime_version_missing)' 'recorded docs include bring-up runtime-version readiness summary'
require_line "$bringup" 'promotion_gate_statuses=[pending=9]' 'bring-up emitted promotion gate status summary'
require_line docs/example_outputs.md 'promotion_gate_statuses=[pending=9]' 'recorded docs include bring-up promotion gate status summary'

ao_range_guard='target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --min-volts -1 --max-volts 1 --volts 2'
require_line "$bringup" "$ao_range_guard" 'bring-up emitted AO setpoint range guard'
require_line "$note" "$ao_range_guard" 'validation-note emitted AO setpoint range guard'
require_line docs/example_outputs.md "$ao_range_guard" 'recorded docs include AO setpoint range guard'

ao_safe_zero_guard='target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --min-volts 1 --max-volts 5 --volts 2'
require_line "$bringup" "$ao_safe_zero_guard" 'bring-up emitted AO safe-zero range guard'
require_line "$note" "$ao_safe_zero_guard" 'validation-note emitted AO safe-zero range guard'
require_line docs/example_outputs.md "$ao_safe_zero_guard" 'recorded docs include AO safe-zero range guard'

lifecycle_cleanup_mode_guard='target/debug/numanager-daqmx-task-lifecycle-helper --simulate-error-after-start'
require_line "$bringup" "$lifecycle_cleanup_mode_guard" 'bring-up emitted lifecycle cleanup mode guard'
require_line "$note" "$lifecycle_cleanup_mode_guard" 'validation-note emitted lifecycle cleanup mode guard'
require_line docs/example_outputs.md "$lifecycle_cleanup_mode_guard" 'recorded docs include lifecycle cleanup mode guard'

lifecycle_cleanup_start_guard='target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --simulate-error-after-start'
require_line "$bringup" "$lifecycle_cleanup_start_guard" 'bring-up emitted lifecycle cleanup start guard'
require_line "$note" "$lifecycle_cleanup_start_guard" 'validation-note emitted lifecycle cleanup start guard'
require_line docs/example_outputs.md "$lifecycle_cleanup_start_guard" 'recorded docs include lifecycle cleanup start guard'

signal_preflight_guard='target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1024 --signal-lines 1 --chunk-size 256 --ci-task ci_signal --ai-task ai_signal --ci Dev1/ctr0 --ai Dev1/ai0 --timeout 10.000000 --preflight-only'
require_line "$bringup" "$signal_preflight_guard" 'bring-up emitted signal timing metadata preflight command'
require_line "$note" "$signal_preflight_guard" 'validation-note emitted signal timing metadata preflight command'
require_line docs/example_outputs.md "$signal_preflight_guard" 'recorded docs include signal timing metadata preflight command'
require_line "$note" 'Signal timing intent' 'validation-note emitted signal timing evidence row'
require_line docs/example_outputs.md 'Signal timing intent' 'recorded docs include signal timing evidence row'
require_line "$note" 'Task timing intent' 'validation-note emitted task timing evidence row'
require_line docs/example_outputs.md 'Task timing intent' 'recorded docs include task timing evidence row'
require_line "$note" 'Finite runtime sequence' 'validation-note emitted finite runtime sequence evidence row'
require_line docs/example_outputs.md 'Finite runtime sequence' 'recorded docs include finite runtime sequence evidence row'
require_line "$note" 'Execution contract intent' 'validation-note emitted execution contract evidence row'
require_line docs/example_outputs.md 'Execution contract intent' 'recorded docs include execution contract evidence row'
require_line "$note" 'Live executor intent' 'validation-note emitted live executor evidence row'
require_line docs/example_outputs.md 'Live executor intent' 'recorded docs include live executor evidence row'
require_line "$bringup" 'executor=[mode=raster_finite;status=not_enabled_pending_hardware_validation;backend=ni_daqmx_sdk_task_wrapper;phases=validate_readiness>setup>write>start>read>wait>publish>cleanup>clear;evidence=pending_hardware_validation]' 'bring-up emitted raster live-executor summary'
require_line "$bringup" 'executor=[mode=signal_finite;status=not_enabled_pending_hardware_validation;backend=ni_daqmx_sdk_task_wrapper;phases=validate_readiness>setup>write>start>read>wait>publish>cleanup>clear;evidence=pending_hardware_validation]' 'bring-up emitted signal live-executor summary'
require_line "$note" 'Live executor: mode=raster_finite:status=not_enabled_pending_hardware_validation:backend=ni_daqmx_sdk_task_wrapper' 'validation-note emitted raster live-executor preflight target'
require_line "$note" 'Live executor: mode=signal_finite:status=not_enabled_pending_hardware_validation:backend=ni_daqmx_sdk_task_wrapper' 'validation-note emitted signal live-executor preflight target'
require_line docs/example_outputs.md 'executor=[mode=raster_finite;status=not_enabled_pending_hardware_validation;backend=ni_daqmx_sdk_task_wrapper;phases=validate_readiness>setup>write>start>read>wait>publish>cleanup>clear;evidence=pending_hardware_validation]' 'recorded docs include raster live-executor summary'
require_line docs/example_outputs.md 'executor=[mode=signal_finite;status=not_enabled_pending_hardware_validation;backend=ni_daqmx_sdk_task_wrapper;phases=validate_readiness>setup>write>start>read>wait>publish>cleanup>clear;evidence=pending_hardware_validation]' 'recorded docs include signal live-executor summary'
require_line docs/example_outputs.md 'Live executor: mode=raster_finite:status=not_enabled_pending_hardware_validation:backend=ni_daqmx_sdk_task_wrapper' 'recorded docs include raster live-executor preflight target'
require_line docs/example_outputs.md 'Live executor: mode=signal_finite:status=not_enabled_pending_hardware_validation:backend=ni_daqmx_sdk_task_wrapper' 'recorded docs include signal live-executor preflight target'
require_line "$bringup" 'reconstruction=[mode=one_detector_sample_per_pixel;input=ci_detector;scan=512x512;recon=512x512;pixel_format=Mono16;evidence=pending_hardware_validation]' 'bring-up emitted raster reconstruction summary'
require_line "$note" 'Reconstruction: mode=one_detector_sample_per_pixel:input=ci_detector:scan=512x512:reconstruction=512x512:pixel_format=Mono16:mapping=row_major_unidirectional_one_sample_per_pixel:accumulation=sum_samples_per_reconstructed_pixel:saturation=clip_to_pixel_format_and_report_saturated_pixels:evidence=pending_hardware_validation' 'validation-note emitted raster reconstruction preflight target'
require_line docs/example_outputs.md 'reconstruction=[mode=one_detector_sample_per_pixel;input=ci_detector;scan=512x512;recon=512x512;pixel_format=Mono16;evidence=pending_hardware_validation]' 'recorded docs include raster reconstruction summary'
require_line docs/example_outputs.md 'Reconstruction: mode=one_detector_sample_per_pixel:input=ci_detector:scan=512x512:reconstruction=512x512:pixel_format=Mono16:mapping=row_major_unidirectional_one_sample_per_pixel:accumulation=sum_samples_per_reconstructed_pixel:saturation=clip_to_pixel_format_and_report_saturated_pixels:evidence=pending_hardware_validation' 'recorded docs include raster reconstruction preflight target'
require_line "$note" 'Runtime publication intent' 'validation-note emitted runtime publication intent evidence row'
require_line docs/example_outputs.md 'Runtime publication intent' 'recorded docs include runtime publication intent evidence row'
require_line "$note" 'Runtime capture frame publication' 'validation-note emitted capture publication evidence row'
require_line "$note" 'Runtime live frame stream publication' 'validation-note emitted live publication evidence row'
require_line "$note" 'Runtime signal chunk publication' 'validation-note emitted signal publication evidence row'
require_line docs/example_outputs.md 'Runtime capture frame publication' 'recorded docs include capture publication evidence row'
require_line docs/example_outputs.md 'Runtime live frame stream publication' 'recorded docs include live publication evidence row'
require_line docs/example_outputs.md 'Runtime signal chunk publication' 'recorded docs include signal publication evidence row'
cleanup_intent_row='`cleanup_plan` and Preflight `planned_cleanup` rows for failure modes, stop/clear order, configured `daqmx_timeout`, and safe-output-state evidence match the bench run'
require_line "$note" "$cleanup_intent_row" 'validation-note emitted cleanup preflight intent evidence row'
require_line docs/example_outputs.md "$cleanup_intent_row" 'recorded docs include cleanup preflight intent evidence row'
capture_preflight_sequence='- Runtime sequence: step=1:setup:ao_scan>do_laser_gate>ci_detector>co_sample_clock:create_channels_and_timing; step=2:write:ao_scan>do_laser_gate:buffered_output_before_start; step=3:start:ci_detector>ao_scan>do_laser_gate>co_sample_clock:inputs_outputs_then_clock; step=4:read:ci_detector:finite_samples; step=5:wait:co_sample_clock:counter_output_done_or_timeout; step=6:stop:co_sample_clock>do_laser_gate>ao_scan>ci_detector:reverse_started_order; step=7:clear:co_sample_clock>ci_detector>do_laser_gate>ao_scan:reverse_setup_order'
require_line "$note" "$capture_preflight_sequence" 'validation-note emitted capture preflight runtime sequence target'
require_line docs/example_outputs.md "$capture_preflight_sequence" 'recorded docs include capture preflight runtime sequence target'
capture_preflight_publication='- Publication: FrameReady:final_reconstructed_frame:scan=512x512:reconstruction=512x512:pixel_format=Mono16:required_metadata=frame_handle+stream+scan_width+scan_height+reconstruction_width+reconstruction_height+reconstruction_pixel_size+sample_rate+line_dwell+detectors+saturated_pixels+progress_status:evidence=pending_hardware_validation'
capture_preflight_reconstruction='- Reconstruction: mode=one_detector_sample_per_pixel:input=ci_detector:scan=512x512:reconstruction=512x512:pixel_format=Mono16:mapping=row_major_unidirectional_one_sample_per_pixel:accumulation=sum_samples_per_reconstructed_pixel:saturation=clip_to_pixel_format_and_report_saturated_pixels:evidence=pending_hardware_validation'
require_line "$note" "$capture_preflight_reconstruction" 'validation-note emitted capture preflight reconstruction target'
require_line docs/example_outputs.md "$capture_preflight_reconstruction" 'recorded docs include capture preflight reconstruction target'
require_line "$note" "$capture_preflight_publication" 'validation-note emitted capture preflight publication target'
require_line docs/example_outputs.md "$capture_preflight_publication" 'recorded docs include capture preflight publication target'
signal_preflight_publication='- Publication: ScanSignalChunk:raw_signal_chunks:channels=counter0+ai0:samples_per_line=1024:lines=1:chunk_size=256:required_metadata=stream+channel_names+timing_origin+line_index+chunk_index+first_sample_index+sample_count+sample_values+sample_rate+sample_period+dropped_samples+dropped_chunks+overflowed:evidence=pending_hardware_validation'
require_line "$note" "$signal_preflight_publication" 'validation-note emitted signal preflight publication target'
require_line docs/example_outputs.md "$signal_preflight_publication" 'recorded docs include signal preflight publication target'
capture_preflight_cleanup='- Cleanup: policy=stop_started_tasks_then_clear_all_created_tasks:failure_modes=partial_setup_failure+post_start_failure+buffered_write_failure+finite_read_failure+counter_output_wait_timeout:started_task_cleanup=stop_started_tasks_before_clear:stop=co_sample_clock>do_laser_gate>ao_scan>ci_detector:clear=co_sample_clock>ci_detector>do_laser_gate>ao_scan:safe_output_state=pending_hardware_validation:evidence=pending_hardware_validation'
require_line "$note" "$capture_preflight_cleanup" 'validation-note emitted capture preflight cleanup target'
require_line docs/example_outputs.md "$capture_preflight_cleanup" 'recorded docs include capture preflight cleanup target'
signal_preflight_cancel='- Cancel: strategy=request_stop_then_clear_created_tasks:stop=ai_signal>ci_signal:clear=ai_signal>ci_signal:safe_output_state=pending_hardware_validation:evidence=pending_hardware_validation'
require_line "$note" "$signal_preflight_cancel" 'validation-note emitted signal preflight cancel target'
require_line docs/example_outputs.md "$signal_preflight_cancel" 'recorded docs include signal preflight cancel target'
require_line "$note" 'Derived sample-clock source' 'validation-note emitted derived sample-clock evidence row'
require_line docs/example_outputs.md 'Derived sample-clock source' 'recorded docs include derived sample-clock evidence row'
require_line "$bringup" 'publication=[FrameReady:final_reconstructed_frame:scan=512x512:recon=512x512:Mono16:pending_hardware_validation]' 'bring-up emitted final-frame publication summary'
require_line "$bringup" 'publication=[ScanSignalChunk:raw_signal_chunks:channels=2:chunk=256:pending_hardware_validation]' 'bring-up emitted signal-chunk publication summary'
require_line "$bringup" 'cancel=[strategy=request_stop_then_clear_created_tasks;stop=co_sample_clock>do_laser_gate>ao_scan>ci_detector;clear=co_sample_clock>ci_detector>do_laser_gate>ao_scan;evidence=pending_hardware_validation]' 'bring-up emitted raster cancel summary'
require_line "$bringup" 'cancel=[strategy=request_stop_then_clear_created_tasks;stop=ai_signal>ci_signal;clear=ai_signal>ci_signal;evidence=pending_hardware_validation]' 'bring-up emitted signal cancel summary'
require_line "$note" 'publication=[FrameReady:final_reconstructed_frame:scan=512x512:recon=512x512:Mono16:pending_hardware_validation]' 'validation-note emitted final-frame publication summary'
require_line "$note" 'publication=[ScanSignalChunk:raw_signal_chunks:channels=2:chunk=256:pending_hardware_validation]' 'validation-note emitted signal-chunk publication summary'
require_line "$note" 'cancel=[strategy=request_stop_then_clear_created_tasks;stop=co_sample_clock>do_laser_gate>ao_scan>ci_detector;clear=co_sample_clock>ci_detector>do_laser_gate>ao_scan;evidence=pending_hardware_validation]' 'validation-note emitted raster cancel summary'
require_line "$note" 'cancel=[strategy=request_stop_then_clear_created_tasks;stop=ai_signal>ci_signal;clear=ai_signal>ci_signal;evidence=pending_hardware_validation]' 'validation-note emitted signal cancel summary'

plan_cleanup_guard='target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1024 --signal-lines 1 --chunk-size 256 --ci-task ci_signal --ai-task ai_signal --ci Dev1/ctr0 --ai Dev1/ai0 --timeout 10.000000 --preflight-only --simulate-setup-error-after 1'
require_line "$bringup" 'bench_plan_setup_cleanup_simulation_commands:' 'bring-up emitted plan setup cleanup simulation section'
require_line "$bringup" "$plan_cleanup_guard" 'bring-up emitted plan setup cleanup simulation command'
require_line "$note" "$plan_cleanup_guard" 'validation-note emitted plan setup cleanup simulation command'
require_line "$note" 'Plan setup cleanup-log simulation' 'validation-note emitted plan setup cleanup evidence row'
require_line docs/example_outputs.md 'bench_plan_setup_cleanup_simulation_commands:' 'recorded docs include plan setup cleanup simulation section'
require_line docs/example_outputs.md "$plan_cleanup_guard" 'recorded docs include plan setup cleanup simulation command'
require_line docs/example_outputs.md 'Plan setup cleanup-log simulation' 'recorded docs include plan setup cleanup evidence row'

plan_no_channels_guard='target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --preflight-only'
require_line "$bringup" "$plan_no_channels_guard" 'bring-up emitted plan no-channel guard'
require_line "$note" "$plan_no_channels_guard" 'validation-note emitted plan no-channel guard'
require_line docs/example_outputs.md "$plan_no_channels_guard" 'recorded docs include plan no-channel guard'

sample_rate_guard='target/debug/numanager-daqmx-plan-setup-helper --sample-rate NaN --samples 1 --ci Dev1/ctr0 --preflight-only'
require_line "$bringup" "$sample_rate_guard" 'bring-up emitted sample-rate guard'
require_line "$note" "$sample_rate_guard" 'validation-note emitted sample-rate guard'
require_line docs/example_outputs.md "$sample_rate_guard" 'recorded docs include sample-rate guard'

zero_sample_rate_guard='target/debug/numanager-daqmx-plan-setup-helper --sample-rate 0 --samples 1 --ci Dev1/ctr0 --preflight-only'
require_line "$bringup" "$zero_sample_rate_guard" 'bring-up emitted positive sample-rate guard'
require_line "$note" "$zero_sample_rate_guard" 'validation-note emitted positive sample-rate guard'
require_line docs/example_outputs.md "$zero_sample_rate_guard" 'recorded docs include positive sample-rate guard'

plan_zero_timeout_guard='target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --timeout 0 --preflight-only'
require_line "$bringup" "$plan_zero_timeout_guard" 'bring-up emitted positive timeout guard'
require_line "$note" "$plan_zero_timeout_guard" 'validation-note emitted positive timeout guard'
require_line docs/example_outputs.md "$plan_zero_timeout_guard" 'recorded docs include positive timeout guard'

empty_clock_source_guard="target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --sample-clock-source '' --preflight-only"
require_line "$bringup" "$empty_clock_source_guard" 'bring-up emitted empty sample-clock source guard'
require_line "$note" "$empty_clock_source_guard" 'validation-note emitted empty sample-clock source guard'
require_line docs/example_outputs.md "$empty_clock_source_guard" 'recorded docs include empty sample-clock source guard'

clock_source_whitespace_guard="target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --sample-clock-source ' /Dev1/Ctr0InternalOutput ' --preflight-only"
require_line "$bringup" "$clock_source_whitespace_guard" 'bring-up emitted sample-clock source whitespace guard'
require_line "$note" "$clock_source_whitespace_guard" 'validation-note emitted sample-clock source whitespace guard'
require_line docs/example_outputs.md "$clock_source_whitespace_guard" 'recorded docs include sample-clock source whitespace guard'

empty_start_trigger_guard="target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --start-trigger '' --preflight-only"
require_line "$bringup" "$empty_start_trigger_guard" 'bring-up emitted empty start-trigger guard'
require_line "$note" "$empty_start_trigger_guard" 'validation-note emitted empty start-trigger guard'
require_line docs/example_outputs.md "$empty_start_trigger_guard" 'recorded docs include empty start-trigger guard'

start_trigger_whitespace_guard="target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --start-trigger ' /Dev1/PFI0 ' --preflight-only"
require_line "$bringup" "$start_trigger_whitespace_guard" 'bring-up emitted start-trigger whitespace guard'
require_line "$note" "$start_trigger_whitespace_guard" 'validation-note emitted start-trigger whitespace guard'
require_line docs/example_outputs.md "$start_trigger_whitespace_guard" 'recorded docs include start-trigger whitespace guard'

empty_channel_guard="target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci '' --preflight-only"
require_line "$bringup" "$empty_channel_guard" 'bring-up emitted empty physical channel guard'
require_line "$note" "$empty_channel_guard" 'validation-note emitted empty physical channel guard'
require_line docs/example_outputs.md "$empty_channel_guard" 'recorded docs include empty physical channel guard'

channel_whitespace_guard="target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci ' Dev1/ctr0 ' --preflight-only"
require_line "$bringup" "$channel_whitespace_guard" 'bring-up emitted physical channel whitespace guard'
require_line "$note" "$channel_whitespace_guard" 'validation-note emitted physical channel whitespace guard'
require_line docs/example_outputs.md "$channel_whitespace_guard" 'recorded docs include physical channel whitespace guard'

empty_task_label_guard="target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --ci-task '' --preflight-only"
require_line "$bringup" "$empty_task_label_guard" 'bring-up emitted empty task label guard'
require_line "$note" "$empty_task_label_guard" 'validation-note emitted empty task label guard'
require_line docs/example_outputs.md "$empty_task_label_guard" 'recorded docs include empty task label guard'

task_label_whitespace_guard="target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --ci-task ' signal ' --preflight-only"
require_line "$bringup" "$task_label_whitespace_guard" 'bring-up emitted task-label whitespace guard'
require_line "$note" "$task_label_whitespace_guard" 'validation-note emitted task-label whitespace guard'
require_line docs/example_outputs.md "$task_label_whitespace_guard" 'recorded docs include task-label whitespace guard'

signal_lines_guard="target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --signal-lines 0 --ci Dev1/ctr0 --preflight-only"
require_line "$bringup" "$signal_lines_guard" 'bring-up emitted signal line-count guard'
require_line "$note" "$signal_lines_guard" 'validation-note emitted signal line-count guard'
require_line docs/example_outputs.md "$signal_lines_guard" 'recorded docs include signal line-count guard'

signal_line_division_guard="target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 5 --signal-lines 2 --ci Dev1/ctr0 --preflight-only"
require_line "$bringup" "$signal_line_division_guard" 'bring-up emitted signal line division guard'
require_line "$note" "$signal_line_division_guard" 'validation-note emitted signal line division guard'
require_line docs/example_outputs.md "$signal_line_division_guard" 'recorded docs include signal line division guard'

chunk_without_signal_lines_guard="target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --chunk-size 1 --ci Dev1/ctr0 --preflight-only"
require_line "$bringup" "$chunk_without_signal_lines_guard" 'bring-up emitted chunk metadata guard'
require_line "$note" "$chunk_without_signal_lines_guard" 'validation-note emitted chunk metadata guard'
require_line docs/example_outputs.md "$chunk_without_signal_lines_guard" 'recorded docs include chunk metadata guard'

chunk_size_guard="target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --signal-lines 1 --chunk-size 2 --ci Dev1/ctr0 --preflight-only"
require_line "$bringup" "$chunk_size_guard" 'bring-up emitted chunk size guard'
require_line "$note" "$chunk_size_guard" 'validation-note emitted chunk size guard'
require_line docs/example_outputs.md "$chunk_size_guard" 'recorded docs include chunk size guard'

require_line "$bringup" '--sample-clock-source /Dev1/Ctr2InternalOutput' 'bring-up emitted derived raster sample-clock source'
require_line "$note" '--sample-clock-source /Dev1/Ctr2InternalOutput' 'validation-note emitted derived raster sample-clock source'
require_line docs/example_outputs.md '--sample-clock-source /Dev1/Ctr2InternalOutput' 'recorded docs include derived raster sample-clock source'
require_line docs/example_outputs.md 'planned_sample_clock_route	source=/Dev1/Ctr2InternalOutput	producer=co_sample_clock	consumers=ci_detector,ao_scan,do_laser_gate	edge=Rising' 'recorded docs include derived raster sample-clock route preview'
require_line docs/example_outputs.md 'planned_timing	ci_detector	sample_clock	source=/Dev1/Ctr2InternalOutput	rate_hz=100000.000000	edge=Rising	mode=FiniteSamps	samples_per_channel=262144' 'recorded docs include raster CI task timing preview'
require_line docs/example_outputs.md 'planned_timing	ci_signal	sample_clock	source=<empty>	rate_hz=100000.000000	edge=Rising	mode=FiniteSamps	samples_per_channel=1024' 'recorded docs include signal CI task timing preview'
require_line docs/example_outputs.md 'planned_runtime_sequence	step=2	phase=write	tasks=ao_scan,do_laser_gate	basis=buffered_output_before_start	evidence=pending_hardware_validation' 'recorded docs include raster buffered-write sequence preview'
require_line docs/example_outputs.md 'planned_runtime_sequence	step=3	phase=start	tasks=ci_detector,ao_scan,do_laser_gate,co_sample_clock	basis=inputs_outputs_then_clock	evidence=pending_hardware_validation' 'recorded docs include raster start sequence preview'
require_line docs/example_outputs.md 'planned_runtime_sequence	step=3	phase=start	tasks=ci_signal,ai_signal	basis=inputs_outputs_then_clock	evidence=pending_hardware_validation' 'recorded docs include signal start sequence preview'
require_line docs/example_outputs.md 'planned_completion	mode=finite	samples_per_channel=262144	timeout_s=10.000000	evidence=pending_hardware_validation' 'recorded docs include raster finite completion preview'
require_line docs/example_outputs.md 'planned_completion	mode=finite	samples_per_channel=1024	timeout_s=10.000000	evidence=pending_hardware_validation' 'recorded docs include signal finite completion preview'
require_line docs/example_outputs.md 'planned_execution_contract	mode=raster_finite	write=ao_scan,do_laser_gate	read=ci_detector	wait=co_sample_clock	write_policy=buffered_before_start	write_auto_start=false	write_layout=GroupByScanNumber	read_policy=finite_expected_samples	read_layout=GroupByScanNumber_for_analog_input	timeout_s=10.000000	publication_policy=publish_only_after_validated_read_and_reconstruction	evidence=pending_hardware_validation' 'recorded docs include raster execution contract preflight preview'
require_line docs/example_outputs.md 'planned_execution_contract	mode=signal_finite	write=none	read=ci_signal,ai_signal	wait=none	write_policy=buffered_before_start	write_auto_start=false	write_layout=GroupByScanNumber	read_policy=finite_expected_samples	read_layout=GroupByScanNumber_for_analog_input	timeout_s=10.000000	publication_policy=publish_only_after_validated_read_and_reconstruction	evidence=pending_hardware_validation' 'recorded docs include signal execution contract preflight preview'
require_line docs/example_outputs.md 'planned_live_executor	mode=raster_finite	status=not_enabled_pending_hardware_validation	backend=ni_daqmx_sdk_task_wrapper	target_scope=linux_windows_optional_sdk_backend	required_validation=legal_review,installed_header_audit,ni_pal_device_inventory,bench_safety_preconditions,task_ordering_routing_completion_cleanup_bench_validation,runtime_publication_hardware_validation,hardware_validation_note	evidence=pending_hardware_validation' 'recorded docs include raster live executor preflight preview'
require_line docs/example_outputs.md 'planned_live_executor_phase	step=3	phase=write	tasks=ao_scan,do_laser_gate	api_surface=DAQmxWriteAnalogF64+DAQmxWriteDigitalLines_buffered_auto_start_false	evidence=pending_hardware_validation' 'recorded docs include raster live executor write phase preview'
require_line docs/example_outputs.md 'planned_live_executor_phase	step=6	phase=wait	tasks=co_sample_clock	api_surface=DAQmxWaitUntilTaskDone	evidence=pending_hardware_validation' 'recorded docs include raster live executor wait phase preview'
require_line docs/example_outputs.md 'planned_live_executor	mode=signal_finite	status=not_enabled_pending_hardware_validation	backend=ni_daqmx_sdk_task_wrapper	target_scope=linux_windows_optional_sdk_backend	required_validation=legal_review,installed_header_audit,ni_pal_device_inventory,bench_safety_preconditions,task_ordering_routing_completion_cleanup_bench_validation,runtime_publication_hardware_validation,hardware_validation_note	evidence=pending_hardware_validation' 'recorded docs include signal live executor preflight preview'
require_line docs/example_outputs.md 'planned_live_executor_phase	step=5	phase=read	tasks=ci_signal,ai_signal	api_surface=DAQmxReadCounterU32+DAQmxReadAnalogF64_finite_expected_samples	evidence=pending_hardware_validation' 'recorded docs include signal live executor read phase preview'
require_line docs/example_outputs.md 'planned_publication	event=FrameReady	mode=raster_frame_payload	scan=512x512	frames=1	pixel_format=pending_runtime_reconstruction	required_metadata=frame_handle,stream,scan_width,scan_height,reconstruction_width,reconstruction_height,reconstruction_pixel_size,sample_rate,line_dwell,detectors,saturated_pixels,progress_status	evidence=pending_hardware_validation' 'recorded docs include raster publication preflight preview'
require_line docs/example_outputs.md 'planned_publication	event=ScanSignalChunk	mode=raw_signal_chunks	channels=ci_signal,ai_signal	samples_per_line=1024	lines=1	chunk_size=256	required_metadata=stream,channel_names,timing_origin,line_index,chunk_index,first_sample_index,sample_count,sample_values,sample_rate,sample_period,dropped_samples,dropped_chunks,overflowed	evidence=pending_hardware_validation' 'recorded docs include signal publication preflight preview'
require_line docs/example_outputs.md 'planned_cleanup	failure_modes=partial_setup_failure,post_start_failure,buffered_write_failure,finite_read_failure,counter_output_wait_timeout	started_task_cleanup=stop_started_tasks_before_clear	safe_output_state=pending_hardware_validation	evidence=pending_hardware_validation' 'recorded docs include cleanup failure-mode preflight preview'
require_line docs/example_outputs.md 'planned_cleanup_order	stop=co_sample_clock,do_laser_gate,ao_scan,ci_detector	clear=co_sample_clock,ci_detector,do_laser_gate,ao_scan	timeout_s=10.000000	evidence=pending_hardware_validation' 'recorded docs include raster cleanup order preflight preview'
require_line docs/example_outputs.md 'planned_cleanup_order	stop=ai_signal,ci_signal	clear=ai_signal,ci_signal	timeout_s=10.000000	evidence=pending_hardware_validation' 'recorded docs include signal cleanup order preflight preview'
require_line docs/example_outputs.md 'daqmx_task_plan=map(39 keys)' 'recorded docs include raster task-plan lifecycle metadata keys'
require_line docs/example_outputs.md 'daqmx_task_plan=map(34 keys)' 'recorded docs include signal task-plan lifecycle metadata keys'
require_line docs/example_outputs.md 'live_task_execution_readiness=map(17 keys)' 'recorded docs include task-plan live readiness external gate metadata'
require_line docs/example_outputs.md '| Runtime version comparison | `not_configured` |' 'recorded docs include validation-note runtime-version comparison row'
require_line docs/example_outputs.md '| Runtime version comparison basis | `configured_runtime_version_missing` |' 'recorded docs include validation-note runtime-version comparison basis row'
require_line docs/example_outputs.md '| Live-task intent with metadata and runtime probe reaches hardware-validation blocker | ok |' 'recorded docs include runtime-probe hardware-validation blocker row'
require_line docs/example_outputs.md 'execution=not_live_task_execution, blocker=feature_ni_daqmx_sdk' 'recorded docs include task-plan live execution blocker summary'
require_line docs/example_outputs.md 'readiness=[ready=false;blocker=feature_ni_daqmx_sdk' 'recorded docs include task-plan live readiness summary'
require_line docs/example_outputs.md '- Live readiness: ready=false;blocker=feature_ni_daqmx_sdk' 'recorded docs include validation-note live readiness target'
require_line docs/example_outputs.md 'sequence=[setup:ao_scan>do_laser_gate>ci_detector>co_sample_clock;write:ao_scan>do_laser_gate;start:ci_detector>ao_scan>do_laser_gate>co_sample_clock;read:ci_detector;wait:co_sample_clock;stop:co_sample_clock>do_laser_gate>ao_scan>ci_detector;clear:co_sample_clock>ci_detector>do_laser_gate>ao_scan]' 'recorded docs include raster runtime sequence summary'
require_line docs/example_outputs.md 'sequence=[setup:ci_signal>ai_signal;start:ci_signal>ai_signal;read:ci_signal>ai_signal;stop:ai_signal>ci_signal;clear:ai_signal>ci_signal]' 'recorded docs include signal runtime sequence summary'
require_line docs/example_outputs.md 'completion=[mode=finite;samples=262144;timeout_s=10.000;evidence=pending_hardware_validation]' 'recorded docs include raster completion summary'
require_line docs/example_outputs.md 'completion=[mode=finite;samples=1024;timeout_s=10.000;evidence=pending_hardware_validation]' 'recorded docs include signal completion summary'
require_line docs/example_outputs.md 'contract=[mode=raster_finite;write=ao_scan>do_laser_gate;read=ci_detector;wait=co_sample_clock;auto_start=false;timeout_s=10.000;evidence=pending_hardware_validation]' 'recorded docs include raster execution contract summary'
require_line docs/example_outputs.md 'contract=[mode=signal_finite;write=none;read=ci_signal>ai_signal;wait=none;auto_start=false;timeout_s=10.000;evidence=pending_hardware_validation]' 'recorded docs include signal execution contract summary'
require_line docs/example_outputs.md '- Execution contract: mode=raster_finite:write=ao_scan>do_laser_gate:read=ci_detector:wait=co_sample_clock:write_auto_start=false' 'recorded docs include validation-note raster execution contract target'
require_line docs/example_outputs.md '- Execution contract: mode=signal_finite:write=none:read=ci_signal>ai_signal:wait=none:write_auto_start=false' 'recorded docs include validation-note signal execution contract target'
require_line docs/example_outputs.md 'publication=[FrameReady:final_reconstructed_frame:scan=512x512:recon=512x512:Mono16:pending_hardware_validation]' 'recorded docs include final-frame publication summary'
require_line docs/example_outputs.md 'publication=[FrameReady:live_dirty_region_updates:scan=512x512:recon=256x256:Mono16:pending_hardware_validation]' 'recorded docs include live-frame publication summary'
require_line docs/example_outputs.md 'publication=[ScanSignalChunk:raw_signal_chunks:channels=2:chunk=256:pending_hardware_validation]' 'recorded docs include signal-chunk publication summary'
require_line docs/example_outputs.md 'cancel=[strategy=request_stop_then_clear_created_tasks;stop=co_sample_clock>do_laser_gate>ao_scan>ci_detector;clear=co_sample_clock>ci_detector>do_laser_gate>ao_scan;evidence=pending_hardware_validation]' 'recorded docs include raster cancel summary'
require_line docs/example_outputs.md 'cancel=[strategy=request_stop_then_clear_created_tasks;stop=ai_signal>ci_signal;clear=ai_signal>ci_signal;evidence=pending_hardware_validation]' 'recorded docs include signal cancel summary'

duplicate_channel_guard="target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --co Dev1/ctr0 --preflight-only"
require_line "$bringup" "$duplicate_channel_guard" 'bring-up emitted duplicate physical-channel guard'
require_line "$note" "$duplicate_channel_guard" 'validation-note emitted duplicate physical-channel guard'
require_line docs/example_outputs.md "$duplicate_channel_guard" 'recorded docs include duplicate physical-channel guard'

duplicate_task_label_guard="target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ci Dev1/ctr0 --ai Dev1/ai0 --ci-task signal --ai-task signal --preflight-only"
require_line "$bringup" "$duplicate_task_label_guard" 'bring-up emitted duplicate active task-label guard'
require_line "$note" "$duplicate_task_label_guard" 'validation-note emitted duplicate active task-label guard'
require_line docs/example_outputs.md "$duplicate_task_label_guard" 'recorded docs include duplicate active task-label guard'

empty_channel_setup_guard="target/debug/numanager-daqmx-channel-setup-helper --kind co --channel '' --dry-run"
require_line "$bringup" "$empty_channel_setup_guard" 'bring-up emitted channel-setup empty channel guard'
require_line "$note" "$empty_channel_setup_guard" 'validation-note emitted channel-setup empty channel guard'
require_line docs/example_outputs.md "$empty_channel_setup_guard" 'recorded docs include channel-setup empty channel guard'

empty_io_channel_guard="target/debug/numanager-daqmx-io-smoke-helper --kind co --channel '' --samples 1"
require_line "$bringup" "$empty_io_channel_guard" 'bring-up emitted I/O empty channel guard'
require_line "$note" "$empty_io_channel_guard" 'validation-note emitted I/O empty channel guard'
require_line docs/example_outputs.md "$empty_io_channel_guard" 'recorded docs include I/O empty channel guard'

empty_lifecycle_name_guard="target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --name ''"
require_line "$bringup" "$empty_lifecycle_name_guard" 'bring-up emitted lifecycle empty task-name guard'
require_line "$note" "$empty_lifecycle_name_guard" 'validation-note emitted lifecycle empty task-name guard'
require_line docs/example_outputs.md "$empty_lifecycle_name_guard" 'recorded docs include lifecycle empty task-name guard'

lifecycle_name_whitespace_guard="target/debug/numanager-daqmx-task-lifecycle-helper --dry-run --name ' lifecycle '"
require_line "$bringup" "$lifecycle_name_whitespace_guard" 'bring-up emitted lifecycle task-name whitespace guard'
require_line "$note" "$lifecycle_name_whitespace_guard" 'validation-note emitted lifecycle task-name whitespace guard'
require_line docs/example_outputs.md "$lifecycle_name_whitespace_guard" 'recorded docs include lifecycle task-name whitespace guard'

empty_channel_setup_name_guard="target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --name '' --dry-run"
require_line "$bringup" "$empty_channel_setup_name_guard" 'bring-up emitted channel-setup empty task-name guard'
require_line "$note" "$empty_channel_setup_name_guard" 'validation-note emitted channel-setup empty task-name guard'
require_line docs/example_outputs.md "$empty_channel_setup_name_guard" 'recorded docs include channel-setup empty task-name guard'

channel_setup_name_whitespace_guard="target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --name ' channel-setup ' --dry-run"
require_line "$bringup" "$channel_setup_name_whitespace_guard" 'bring-up emitted channel-setup task-name whitespace guard'
require_line "$note" "$channel_setup_name_whitespace_guard" 'validation-note emitted channel-setup task-name whitespace guard'
require_line docs/example_outputs.md "$channel_setup_name_whitespace_guard" 'recorded docs include channel-setup task-name whitespace guard'

channel_setup_channel_whitespace_guard="target/debug/numanager-daqmx-channel-setup-helper --kind co --channel ' Dev1/ctr2 ' --dry-run"
require_line "$bringup" "$channel_setup_channel_whitespace_guard" 'bring-up emitted channel-setup channel whitespace guard'
require_line "$note" "$channel_setup_channel_whitespace_guard" 'validation-note emitted channel-setup channel whitespace guard'
require_line docs/example_outputs.md "$channel_setup_channel_whitespace_guard" 'recorded docs include channel-setup channel whitespace guard'

empty_io_name_guard="target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --name '' --samples 1"
require_line "$bringup" "$empty_io_name_guard" 'bring-up emitted I/O empty task-name guard'
require_line "$note" "$empty_io_name_guard" 'validation-note emitted I/O empty task-name guard'
require_line docs/example_outputs.md "$empty_io_name_guard" 'recorded docs include I/O empty task-name guard'

io_name_whitespace_guard="target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --name ' io-smoke ' --samples 1"
require_line "$bringup" "$io_name_whitespace_guard" 'bring-up emitted I/O task-name whitespace guard'
require_line "$note" "$io_name_whitespace_guard" 'validation-note emitted I/O task-name whitespace guard'
require_line docs/example_outputs.md "$io_name_whitespace_guard" 'recorded docs include I/O task-name whitespace guard'

plan_zero_samples_guard='target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 0 --ci Dev1/ctr0 --preflight-only'
require_line "$bringup" "$plan_zero_samples_guard" 'bring-up emitted plan positive sample-count guard'
require_line "$note" "$plan_zero_samples_guard" 'validation-note emitted plan positive sample-count guard'
require_line docs/example_outputs.md "$plan_zero_samples_guard" 'recorded docs include plan positive sample-count guard'

raster_width_guard='target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 0 --height 1 --frames 1 --ci Dev1/ctr0 --preflight-only'
require_line "$bringup" "$raster_width_guard" 'bring-up emitted raster width guard'
require_line "$note" "$raster_width_guard" 'validation-note emitted raster width guard'
require_line docs/example_outputs.md "$raster_width_guard" 'recorded docs include raster width guard'

partial_raster_guard='target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 1 --ci Dev1/ctr0 --preflight-only'
require_line "$bringup" "$partial_raster_guard" 'bring-up emitted partial raster-dimensions guard'
require_line "$note" "$partial_raster_guard" 'validation-note emitted partial raster-dimensions guard'
require_line docs/example_outputs.md "$partial_raster_guard" 'recorded docs include partial raster-dimensions guard'

raster_overflow_guard='target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 18446744073709551615 --height 2 --frames 1 --ci Dev1/ctr0 --preflight-only'
require_line "$bringup" "$raster_overflow_guard" 'bring-up emitted raster dimension overflow guard'
require_line "$note" "$raster_overflow_guard" 'validation-note emitted raster dimension overflow guard'
require_line docs/example_outputs.md "$raster_overflow_guard" 'recorded docs include raster dimension overflow guard'

raster_frame_overflow_guard='target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 2 --height 2 --frames 4611686018427387904 --ci Dev1/ctr0 --preflight-only'
require_line "$bringup" "$raster_frame_overflow_guard" 'bring-up emitted raster frame-product overflow guard'
require_line "$note" "$raster_frame_overflow_guard" 'validation-note emitted raster frame-product overflow guard'
require_line docs/example_outputs.md "$raster_frame_overflow_guard" 'recorded docs include raster frame-product overflow guard'

raster_height_guard='target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 1 --height 0 --frames 1 --ci Dev1/ctr0 --preflight-only'
require_line "$bringup" "$raster_height_guard" 'bring-up emitted raster height guard'
require_line "$note" "$raster_height_guard" 'validation-note emitted raster height guard'
require_line docs/example_outputs.md "$raster_height_guard" 'recorded docs include raster height guard'

raster_frames_guard='target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --width 1 --height 1 --frames 0 --ci Dev1/ctr0 --preflight-only'
require_line "$bringup" "$raster_frames_guard" 'bring-up emitted raster frame-count guard'
require_line "$note" "$raster_frames_guard" 'validation-note emitted raster frame-count guard'
require_line docs/example_outputs.md "$raster_frames_guard" 'recorded docs include raster frame-count guard'

transfer_guard='target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1073741824 --ao Dev1/ao0 --ao Dev1/ao1 --preflight-only'
require_line "$bringup" "$transfer_guard" 'bring-up emitted transfer element guard'
require_line "$note" "$transfer_guard" 'validation-note emitted transfer element guard'
require_line docs/example_outputs.md "$transfer_guard" 'recorded docs include transfer element guard'

plan_analog_finite_guard='target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ao Dev1/ao0 --min-volts NaN --max-volts 1 --preflight-only'
require_line "$bringup" "$plan_analog_finite_guard" 'bring-up emitted plan analog finite guard'
require_line "$note" "$plan_analog_finite_guard" 'validation-note emitted plan analog finite guard'
require_line docs/example_outputs.md "$plan_analog_finite_guard" 'recorded docs include plan analog finite guard'

plan_analog_range_guard='target/debug/numanager-daqmx-plan-setup-helper --sample-rate 100000 --samples 1 --ao Dev1/ao0 --min-volts 1 --max-volts -1 --preflight-only'
require_line "$bringup" "$plan_analog_range_guard" 'bring-up emitted plan analog range guard'
require_line "$note" "$plan_analog_range_guard" 'validation-note emitted plan analog range guard'
require_line docs/example_outputs.md "$plan_analog_range_guard" 'recorded docs include plan analog range guard'

duty_cycle_guard='target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --duty-cycle 1.5 --dry-run'
require_line "$bringup" "$duty_cycle_guard" 'bring-up emitted duty-cycle guard'
require_line "$note" "$duty_cycle_guard" 'validation-note emitted duty-cycle guard'
require_line docs/example_outputs.md "$duty_cycle_guard" 'recorded docs include duty-cycle guard'

zero_frequency_guard='target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --frequency 0 --dry-run'
require_line "$bringup" "$zero_frequency_guard" 'bring-up emitted positive frequency guard'
require_line "$note" "$zero_frequency_guard" 'validation-note emitted positive frequency guard'
require_line docs/example_outputs.md "$zero_frequency_guard" 'recorded docs include positive frequency guard'

duty_cycle_finite_guard='target/debug/numanager-daqmx-channel-setup-helper --kind co --channel Dev1/ctr2 --duty-cycle NaN --dry-run'
require_line "$bringup" "$duty_cycle_finite_guard" 'bring-up emitted finite duty-cycle guard'
require_line "$note" "$duty_cycle_finite_guard" 'validation-note emitted finite duty-cycle guard'
require_line docs/example_outputs.md "$duty_cycle_finite_guard" 'recorded docs include finite duty-cycle guard'

io_frequency_guard='target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --frequency inf --samples 1'
require_line "$bringup" "$io_frequency_guard" 'bring-up emitted I/O frequency guard'
require_line "$note" "$io_frequency_guard" 'validation-note emitted I/O frequency guard'
require_line docs/example_outputs.md "$io_frequency_guard" 'recorded docs include I/O frequency guard'

io_zero_frequency_guard='target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --frequency 0 --samples 1'
require_line "$bringup" "$io_zero_frequency_guard" 'bring-up emitted positive I/O frequency guard'
require_line "$note" "$io_zero_frequency_guard" 'validation-note emitted positive I/O frequency guard'
require_line docs/example_outputs.md "$io_zero_frequency_guard" 'recorded docs include positive I/O frequency guard'

io_duty_cycle_finite_guard='target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --duty-cycle NaN --samples 1'
require_line "$bringup" "$io_duty_cycle_finite_guard" 'bring-up emitted finite I/O duty-cycle guard'
require_line "$note" "$io_duty_cycle_finite_guard" 'validation-note emitted finite I/O duty-cycle guard'
require_line docs/example_outputs.md "$io_duty_cycle_finite_guard" 'recorded docs include finite I/O duty-cycle guard'

io_duty_cycle_guard='target/debug/numanager-daqmx-io-smoke-helper --kind co --channel Dev1/ctr2 --duty-cycle 1.5 --samples 1'
require_line "$bringup" "$io_duty_cycle_guard" 'bring-up emitted I/O duty-cycle guard'
require_line "$note" "$io_duty_cycle_guard" 'validation-note emitted I/O duty-cycle guard'
require_line docs/example_outputs.md "$io_duty_cycle_guard" 'recorded docs include I/O duty-cycle guard'

analog_finite_guard='target/debug/numanager-daqmx-channel-setup-helper --kind ai --channel Dev1/ai0 --min-volts NaN --max-volts 1 --dry-run'
require_line "$bringup" "$analog_finite_guard" 'bring-up emitted analog finite guard'
require_line "$note" "$analog_finite_guard" 'validation-note emitted analog finite guard'
require_line docs/example_outputs.md "$analog_finite_guard" 'recorded docs include analog finite guard'

analog_range_guard='target/debug/numanager-daqmx-channel-setup-helper --kind ai --channel Dev1/ai0 --min-volts 1 --max-volts -1 --dry-run'
require_line "$bringup" "$analog_range_guard" 'bring-up emitted analog range guard'
require_line "$note" "$analog_range_guard" 'validation-note emitted analog range guard'
require_line docs/example_outputs.md "$analog_range_guard" 'recorded docs include analog range guard'

io_timeout_guard='target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1 --timeout NaN'
require_line "$bringup" "$io_timeout_guard" 'bring-up emitted I/O timeout guard'
require_line "$note" "$io_timeout_guard" 'validation-note emitted I/O timeout guard'
require_line docs/example_outputs.md "$io_timeout_guard" 'recorded docs include I/O timeout guard'

io_zero_timeout_guard='target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 1 --timeout 0'
require_line "$bringup" "$io_zero_timeout_guard" 'bring-up emitted positive I/O timeout guard'
require_line "$note" "$io_zero_timeout_guard" 'validation-note emitted positive I/O timeout guard'
require_line docs/example_outputs.md "$io_zero_timeout_guard" 'recorded docs include positive I/O timeout guard'

io_samples_guard='target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 0'
require_line "$bringup" "$io_samples_guard" 'bring-up emitted I/O sample-count guard'
require_line "$note" "$io_samples_guard" 'validation-note emitted I/O sample-count guard'
require_line docs/example_outputs.md "$io_samples_guard" 'recorded docs include I/O sample-count guard'

io_sample_range_guard='target/debug/numanager-daqmx-io-smoke-helper --kind ai --channel Dev1/ai0 --samples 2147483648'
require_line "$bringup" "$io_sample_range_guard" 'bring-up emitted I/O sample range guard'
require_line "$note" "$io_sample_range_guard" 'validation-note emitted I/O sample range guard'
require_line docs/example_outputs.md "$io_sample_range_guard" 'recorded docs include I/O sample range guard'

io_analog_finite_guard='target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --min-volts NaN --max-volts 1 --volts 0'
require_line "$bringup" "$io_analog_finite_guard" 'bring-up emitted I/O analog finite guard'
require_line "$note" "$io_analog_finite_guard" 'validation-note emitted I/O analog finite guard'
require_line docs/example_outputs.md "$io_analog_finite_guard" 'recorded docs include I/O analog finite guard'

io_analog_range_guard='target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --min-volts 1 --max-volts -1 --volts 0'
require_line "$bringup" "$io_analog_range_guard" 'bring-up emitted I/O analog range guard'
require_line "$note" "$io_analog_range_guard" 'validation-note emitted I/O analog range guard'
require_line docs/example_outputs.md "$io_analog_range_guard" 'recorded docs include I/O analog range guard'

io_cleanup_kind_guard='target/debug/numanager-daqmx-io-smoke-helper --kind ao --channel Dev1/ao0 --simulate-error-after-start'
require_line "$bringup" "$io_cleanup_kind_guard" 'bring-up emitted I/O cleanup kind guard'
require_line "$note" "$io_cleanup_kind_guard" 'validation-note emitted I/O cleanup kind guard'
require_line docs/example_outputs.md "$io_cleanup_kind_guard" 'recorded docs include I/O cleanup kind guard'

for marker in \
  'bench_evidence_commands:' \
  'bench_runtime_probe_commands:' \
  'bench_inventory_commands:' \
  'bench_preflight_commands:' \
  'bench_io_smoke_execute_commands:' \
  '## Required Artifacts' \
  '## Required Commands' \
  '## Command Output Log' \
  '## Backend Readiness' \
  '## Backend Inventory' \
  '## Evidence Checklist'
do
  require_line docs/example_outputs.md "$marker" "recorded docs include $marker"
done

task_readiness_evidence='Task-plan live readiness showing per-plan blocker, missing evidence, runtime-version comparison, backend-status agreement, and pending hardware validation'
require_line "$note" "$task_readiness_evidence" 'validation-note emitted task-plan live readiness evidence row'
require_line docs/example_outputs.md "$task_readiness_evidence" 'recorded docs include task-plan live readiness evidence row'
readiness_agreement='Task-plan readiness agreement | `capture=true;signal=true;basis=backend_status_runtime_version_and_daqmx_task_plan`'
require_line "$note" "$readiness_agreement" 'validation-note emitted backend/task-plan readiness agreement'
require_line docs/example_outputs.md "$readiness_agreement" 'recorded docs include backend/task-plan readiness agreement'
bringup_readiness_artifact='LSM bring-up backend_readiness line | `backend_readiness: ... runtime_version=... promotion_gate_statuses=[pending=9]`'
require_line "$note" "$bringup_readiness_artifact" 'validation-note emitted bring-up readiness artifact row'
require_line docs/example_outputs.md "$bringup_readiness_artifact" 'recorded docs include bring-up readiness artifact row'
bringup_readiness_evidence='LSM bring-up plan with backend_readiness and promotion_gate_statuses captured before helper commands'
require_line "$note" "$bringup_readiness_evidence" 'validation-note emitted bring-up readiness evidence row'
require_line docs/example_outputs.md "$bringup_readiness_evidence" 'recorded docs include bring-up readiness evidence row'
backend_inventory_artifact='Backend inventory readiness table | `## Backend Inventory`'
require_line "$note" "$backend_inventory_artifact" 'validation-note emitted backend inventory artifact row'
require_line docs/example_outputs.md "$backend_inventory_artifact" 'recorded docs include backend inventory artifact row'
backend_inventory_evidence='Backend inventory readiness showing helper isolation, requested inventory state, detected-device count, configured-device identity, and contained helper/configured-device errors'
require_line "$note" "$backend_inventory_evidence" 'validation-note emitted backend inventory evidence row'
require_line docs/example_outputs.md "$backend_inventory_evidence" 'recorded docs include backend inventory evidence row'
require_line "$note" 'Device inventory requested | `false`' 'validation-note emitted backend inventory requested row'
require_line docs/example_outputs.md 'Device inventory requested | `false`' 'recorded docs include backend inventory requested row'
safety_artifact='Bench safety preconditions table | `## Setup And Safety`'
require_line "$note" "$safety_artifact" 'validation-note emitted bench safety artifact row'
require_line docs/example_outputs.md "$safety_artifact" 'recorded docs include bench safety artifact row'
safety_evidence='Bench safety preconditions recorded before --execute helper commands'
require_line "$note" "$safety_evidence" 'validation-note emitted bench safety evidence row'
require_line docs/example_outputs.md "$safety_evidence" 'recorded docs include bench safety evidence row'
acknowledged_execute='--execute --bench-safety-reviewed'
require_line "$bringup" "$acknowledged_execute" 'bring-up plan emitted acknowledged I/O execute command'
require_line "$note" "$acknowledged_execute" 'validation-note emitted acknowledged I/O execute command'
require_line docs/example_outputs.md "$acknowledged_execute" 'recorded docs include acknowledged I/O execute command'
external_gates='External promotion gates | `legal_review+installed_windows_package_license_review+installed_linux_26_5_header_audit+installed_windows_26_5_header_audit+ni_pal_device_inventory+bench_safety_preconditions+task_ordering_routing_completion_cleanup_bench_validation+runtime_publication_hardware_validation+hardware_validation_note`'
require_line "$note" "$external_gates" 'validation-note emitted external promotion gates'
require_line docs/example_outputs.md "$external_gates" 'recorded docs include external promotion gates'
require_line "$note" '## External Promotion Gates' 'validation-note emitted external promotion gate evidence section'
require_line docs/example_outputs.md '## External Promotion Gates' 'recorded docs include external promotion gate evidence section'
bench_gate='task_ordering_routing_completion_cleanup_bench_validation` | Bench logs for task order, routing, completion, stop/clear, cleanup, and safe output state | pending'
require_line "$note" "$bench_gate" 'validation-note emitted task-behavior promotion gate evidence row'
require_line docs/example_outputs.md "$bench_gate" 'recorded docs include task-behavior promotion gate evidence row'
safety_gate='bench_safety_preconditions` | Completed Setup And Safety table plus reviewed wiring, load, safe output state, interlocks, emergency stop, cleanup, and fault-recovery constraints | pending'
require_line "$note" "$safety_gate" 'validation-note emitted bench-safety promotion gate evidence row'
require_line docs/example_outputs.md "$safety_gate" 'recorded docs include bench-safety promotion gate evidence row'
require_line docs/example_outputs.md 'promotion_gate_statuses: pending=9' 'recorded docs include runtime-probe promotion gate status summary'

printf '# NI-DAQmx Example Output Sync Audit\n\n'
printf '| Workflow | Status |\n'
printf '| --- | --- |\n'
printf '| Bring-up plan emitted DAQmx audit commands | ok |\n'
printf '| Validation note emitted DAQmx audit commands | ok |\n'
printf '| Recorded example output includes DAQmx audit commands and scaffold sections | ok |\n'
printf '\nThis audit compares public DAQmx scaffold example output against recorded documentation markers. It does not create NI-DAQmx tasks, write outputs, read inputs, execute scans, or provide hardware evidence.\n'
