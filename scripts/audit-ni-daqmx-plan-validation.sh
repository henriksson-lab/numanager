#!/usr/bin/env bash
set -euo pipefail

tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

output="$tmp_dir/lsm_daqmx_plan_validation.out"
cargo run -p numanager-examples -- lsm_daqmx_plan_validation >"$output" 2>&1

require_line() {
  local pattern=$1
  local description=$2
  if ! rg -F -- "$pattern" "$output" >/dev/null; then
    printf 'missing %s: %s\n' "$description" "$pattern" >&2
    printf '\n--- output ---\n' >&2
    sed -n '1,220p' "$output" >&2
    exit 1
  fi
}

require_line 'valid_raster_validation: status=valid runnable=true' 'valid raster plan validation marker'
require_line 'valid_raster_helper_commands: setup=string preflight=string' 'valid raster helper command marker'
require_line 'valid_raster_result: api_status=declared_not_live, completion_basis=configured_api_only, daqmx_task_plan=map(39 keys)' 'valid raster task-plan key count'
require_line 'valid_raster_plan: execution=not_live_task_execution, blocker=feature_ni_daqmx_sdk' 'valid raster task-plan blocker summary'
require_line 'readiness=[ready=false;blocker=feature_ni_daqmx_sdk' 'valid task-plan readiness summary'
require_line 'sequence=[setup:ao_scan>do_laser_gate>ci_detector>co_sample_clock;write:ao_scan>do_laser_gate;start:ci_detector>ao_scan>do_laser_gate>co_sample_clock;read:ci_detector;wait:co_sample_clock;stop:co_sample_clock>do_laser_gate>ao_scan>ci_detector;clear:co_sample_clock>ci_detector>do_laser_gate>ao_scan]' 'valid raster runtime sequence summary'
require_line 'completion=[mode=finite;samples=65536;timeout_s=10.000;evidence=pending_hardware_validation]' 'valid raster completion summary'
require_line 'contract=[mode=raster_finite;write=ao_scan>do_laser_gate;read=ci_detector;wait=co_sample_clock;auto_start=false;timeout_s=10.000;evidence=pending_hardware_validation]' 'valid raster execution contract summary'
require_line 'reconstruction=[mode=one_detector_sample_per_pixel;input=ci_detector;scan=256x256;recon=256x256;pixel_format=Mono16;evidence=pending_hardware_validation]' 'valid raster reconstruction summary'
require_line 'publication=[FrameReady:final_reconstructed_frame:scan=256x256:recon=256x256:Mono16:pending_hardware_validation]' 'valid raster publication summary'
require_line 'cancel=[strategy=request_stop_then_clear_created_tasks;stop=co_sample_clock>do_laser_gate>ao_scan>ci_detector;clear=co_sample_clock>ci_detector>do_laser_gate>ao_scan;evidence=pending_hardware_validation]' 'valid raster cancel summary'
require_line 'valid_signal_validation: status=valid runnable=true' 'valid signal plan validation marker'
require_line 'valid_signal_helper_commands: setup=string preflight=string' 'valid signal helper command marker'
require_line 'valid_signal_result: api_status=declared_not_live, channel_count=2, channel_names=list(2), chunk_size=128, completion_basis=configured_api_only, daqmx_task_plan=map(34 keys)' 'valid signal task-plan key count'
require_line 'valid_signal_plan: execution=not_live_task_execution, blocker=feature_ni_daqmx_sdk' 'valid signal task-plan blocker summary'
require_line 'sequence=[setup:ci_signal>ai_signal;start:ci_signal>ai_signal;read:ci_signal>ai_signal;stop:ai_signal>ci_signal;clear:ai_signal>ci_signal]' 'valid signal runtime sequence summary'
require_line 'completion=[mode=finite;samples=512;timeout_s=10.000;evidence=pending_hardware_validation]' 'valid signal completion summary'
require_line 'contract=[mode=signal_finite;write=none;read=ci_signal>ai_signal;wait=none;auto_start=false;timeout_s=10.000;evidence=pending_hardware_validation]' 'valid signal execution contract summary'
require_line 'publication=[ScanSignalChunk:raw_signal_chunks:channels=2:chunk=128:pending_hardware_validation]' 'valid signal publication summary'
require_line 'cancel=[strategy=request_stop_then_clear_created_tasks;stop=ai_signal>ci_signal;clear=ai_signal>ci_signal;evidence=pending_hardware_validation]' 'valid signal cancel summary'
require_line 'raster_validation: status=invalid_role_channels runnable=false' 'invalid raster role-channel marker'
require_line 'raster_helper_commands: setup=null preflight=null' 'invalid raster helper command suppression'
require_line 'signal_validation: status=invalid_no_recognized_channels runnable=false' 'invalid signal channel marker'
require_line 'signal_helper_commands: setup=null preflight=null' 'invalid signal helper command suppression'
require_line 'execution_gate: not_live_task_execution' 'non-live execution gate marker'
require_line 'live_task_execution_ready=false' 'live-task readiness marker'
require_line 'live_task_execution_requested=false' 'live-task request marker'

printf '# NI-DAQmx Plan Validation Audit\n\n'
printf '| Workflow | Status |\n'
printf '| --- | --- |\n'
printf '| Valid raster plan keeps helper commands runnable | ok |\n'
printf '| Valid signal plan keeps helper commands runnable | ok |\n'
printf '| Invalid raster role plan suppresses helper commands | ok |\n'
printf '| Invalid signal channel plan suppresses helper commands | ok |\n'
printf '| Execution gate remains non-live | ok |\n'
printf '\nThis audit runs the public `lsm_daqmx_plan_validation` example and checks configured plan-validation markers plus helper-command suppression for invalid plans. It does not create NI-DAQmx tasks, write outputs, read inputs, execute scans, or provide hardware evidence.\n'
