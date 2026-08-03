#!/usr/bin/env bash
set -euo pipefail

tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

run_live_request() {
  local name=$1
  shift
  local output="$tmp_dir/${name}.out"
  printf 'running %s\n' "$name" >&2
  NUMANAGER_DAQMX_LIVE_TASK_EXECUTION=1 "$@" >"$output" 2>&1
  printf '%s\n' "$output"
}

require_line() {
  local file=$1
  local pattern=$2
  local description=$3
  if ! rg -F -- "$pattern" "$file" >/dev/null; then
    printf 'missing %s in %s: %s\n' "$description" "$file" "$pattern" >&2
    printf '\n--- output ---\n' >&2
    sed -n '1,180p' "$file" >&2
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
    sed -n '1,180p' "$file" >&2
    exit 1
  fi
}

capture=$(run_live_request confocal_capture cargo run -p numanager-examples -- lsm_confocal_capture imswitch)
require_line "$capture" 'api_status=declared_not_live' 'capture non-live API status'
require_line "$capture" 'live_task_execution_requested=true' 'capture live intent marker'
require_line "$capture" 'live_task_execution_ready=false' 'capture live readiness gate'
require_line "$capture" 'execution=not_live_task_execution' 'capture non-live execution plan'
require_line "$capture" 'result=final_image_pending' 'capture pending result'
require_absent "$capture" 'frame: ' 'capture hardware frame output'

stream=$(run_live_request confocal_stream cargo run -p numanager-examples -- lsm_confocal_stream imswitch)
require_line "$stream" 'api_status=declared_not_live' 'stream non-live API status'
require_line "$stream" 'live_task_execution_requested=true' 'stream live intent marker'
require_line "$stream" 'live_task_execution_ready=false' 'stream live readiness gate'
require_line "$stream" 'execution=not_live_task_execution' 'stream non-live execution plan'
require_line "$stream" 'result=live_image_stream_pending' 'stream pending result'
require_absent "$stream" 'frames: observed=' 'stream hardware frame count'

signal=$(run_live_request signal_stream cargo run -p numanager-examples -- lsm_signal_stream imswitch)
require_line "$signal" 'api_status=declared_not_live' 'signal non-live API status'
require_line "$signal" 'live_task_execution_requested=true' 'signal live intent marker'
require_line "$signal" 'live_task_execution_ready=false' 'signal live readiness gate'
require_line "$signal" 'execution=not_live_task_execution' 'signal non-live execution plan'
require_line "$signal" 'result=raw_signal_stream_pending' 'signal pending result'
require_absent "$signal" 'chunks: observed=' 'signal hardware chunk count'

gui=$(run_live_request lsm_gui cargo run -p numanager-examples --features gui -- software_gui imswitch --smoke)
require_line "$gui" 'source_summary: backend=not_live_backend; live_ready=false; live_requested=true;' 'GUI live gate summary'
require_line "$gui" 'promotion_gate_statuses=[pending=9]' 'GUI promotion gate status summary'
require_line "$gui" 'snapshot_frames: observed=0' 'GUI no snapshot frames'
require_line "$gui" 'live_frames: observed=0' 'GUI no live frames'
require_line "$gui" 'line_chunks: observed=0 samples=0 channels=0' 'GUI no signal chunks'
require_line "$gui" 'live_task_execution_requested=true' 'GUI live intent propagated to API results'
require_line "$gui" 'execution=not_live_task_execution' 'GUI non-live execution plans'
require_absent "$gui" 'scene_controls:' 'GUI simulator shared-scene control write on ImSwitch source'
require_absent "$gui" 'objective_control:' 'GUI simulator objective control write on ImSwitch source'
require_absent "$gui" 'detector_controls:' 'GUI simulator detector control write on ImSwitch source'

printf '# NI-DAQmx Live Gate Audit\n\n'
printf '| Workflow | Status |\n'
printf '| --- | --- |\n'
printf '| Confocal capture live intent remains gated | ok |\n'
printf '| Confocal stream live intent remains gated | ok |\n'
printf '| Scan-signal stream live intent remains gated | ok |\n'
printf '| LSM GUI ImSwitch live intent remains gated and simulator controls stay inactive | ok |\n'
printf '\nThis audit sets `NUMANAGER_DAQMX_LIVE_TASK_EXECUTION=1` and verifies that public ImSwitch DAQmx APIs record live-task intent while still reporting `live_task_execution_ready=false` and `execution=not_live_task_execution`. The GUI smoke path must not emit simulator-only scene, objective, or detector control writebacks for the configured ImSwitch source. It does not create NI-DAQmx tasks, write outputs, read inputs, publish hardware frames, or provide hardware evidence.\n'
