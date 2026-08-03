#!/usr/bin/env bash
set -euo pipefail

tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

run_example() {
  local name=$1
  shift
  local output="$tmp_dir/${name}.out"
  printf 'running %s\n' "$name" >&2
  cargo run -p numanager-examples "$@" >"$output"
  printf '%s\n' "$output"
}

run_gui_example() {
  local name=$1
  shift
  local output="$tmp_dir/${name}.out"
  printf 'running %s\n' "$name" >&2
  cargo run -p numanager-examples --features gui -- "$@" >"$output"
  printf '%s\n' "$output"
}

require_line() {
  local file=$1
  local pattern=$2
  local description=$3
  if ! rg -F "$pattern" "$file" >/dev/null; then
    printf 'missing %s in %s: %s\n' "$description" "$file" "$pattern" >&2
    printf '\n--- output ---\n' >&2
    sed -n '1,160p' "$file" >&2
    exit 1
  fi
}

capture=$(run_example lsm_confocal_capture -- lsm_confocal_capture sim-lsm)
require_line "$capture" 'api: ConfocalImageCapture request=ConfocalImageCapture' 'capture API marker'
require_line "$capture" 'frame: 512x512 524288 bytes format=Mono16' 'Mono16 frame output'
require_line "$capture" 'frame_metadata: scan=512x512, reconstruction=512x512' 'capture metadata'
require_line "$capture" 'detector_gain=1.000, detector_noise=1.000' 'capture detector gain/noise metadata'

mono8=$(run_example lsm_confocal_capture_mono8 -- lsm_confocal_capture_mono8 sim-lsm)
require_line "$mono8" 'frame: 128x128 16384 bytes format=Mono8' 'Mono8 reconstructed frame output'
require_line "$mono8" 'frame_metadata: scan=256x256, reconstruction=128x128' 'resized capture metadata'

stream=$(run_example lsm_confocal_stream -- lsm_confocal_stream sim-lsm)
require_line "$stream" 'api: ConfocalImageStream request=ConfocalImageStream' 'stream API marker'
require_line "$stream" 'progress: updates=4 last=4/4' 'stream progress'
require_line "$stream" 'dirty_region: x=0 y=96 width=256 height=32' 'dirty-region metadata'

signal=$(run_example lsm_signal_stream -- lsm_signal_stream sim-lsm)
require_line "$signal" 'api: ScanSignalStream request=ScanSignalStream' 'signal API marker'
require_line "$signal" 'chunks: observed=4 origin=0 first_sample=0 samples=1024' 'signal chunk summary'
require_line "$signal" 'chunk_size=256 sample_period_s=0.000010000 channels=2 dropped_chunks=0 dropped_samples=0 overflowed=false' 'signal chunk timing/drop metadata'
require_line "$signal" 'chunk_metadata: channels=counter0+ai0' 'signal chunk metadata'
require_line "$signal" 'detector_gain=1.000, detector_noise=1.000' 'signal detector gain/noise metadata'

line_dwell=$(run_example lsm_line_dwell_timing -- lsm_line_dwell_timing)
require_line "$line_dwell" 'sample_rate=Frequency(Frequency { value: 50000.0, unit: Hertz })' 'line-dwell-derived sample rate'
require_line "$line_dwell" 'first_chunk: sample_rate_hz=50000' 'line-dwell first chunk summary'

live_cancel=$(run_example lsm_live_cancel -- lsm_live_cancel sim-lsm)
require_line "$live_cancel" 'frames: observed=2 latest=256x256 131072 bytes format=Mono16' 'live cancel frame count'
require_line "$live_cancel" 'cancel: cancelled' 'live cancellation status'

signal_cancel=$(run_example lsm_signal_cancel -- lsm_signal_cancel sim-lsm)
require_line "$signal_cancel" 'chunks: observed=3 latest_line=0 latest_chunk=2' 'signal cancel chunk count'
require_line "$signal_cancel" 'first_chunk: channels=counter0+ai0, line=0, chunk=0, first_sample=0, sample_rate_hz=100000, sample_period_s=0.000010000, dropped_chunks=0, dropped_samples=0, overflowed=false' 'signal cancel first chunk timing/drop metadata'
require_line "$signal_cancel" 'scene=[stage_um=(0.000,0.000,4250.000), sample_pixel_size_um=0.325, laser_power=0.850, laser_gate_enabled=true, magnification=20.0, numerical_aperture=0.45, detector_gain=1.000, detector_noise=1.000]' 'signal cancel first chunk scene metadata'
require_line "$signal_cancel" 'cancel: cancelled' 'signal cancellation status'

composed=$(run_example lsm_composed_workflow -- lsm_composed_workflow)
require_line "$composed" 'shared stage state: Bool(true)' 'composed shared stage state'
require_line "$composed" 'shared sample_seed: microscope=I64(' 'composed shared specimen seed'
require_line "$composed" 'lsm detector controls: gain=Ratio(Ratio { value: 1.25, unit: Fraction }) noise=Ratio(Ratio { value: 0.8, unit: Fraction })' 'composed detector control write/readback'
require_line "$composed" 'confocal capture scene: stage_um=(320.000,-180.000,4252.000)' 'composed capture scene metadata'
require_line "$composed" 'scan signal scene: stage_um=(320.000,-180.000,4252.000)' 'composed signal scene metadata'
require_line "$composed" 'detector_gain=1.250, detector_noise=0.800' 'composed detector gain/noise metadata'

gui=$(run_gui_example lsm_gui_smoke lsm_gui sim-lsm --smoke)
require_line "$gui" 'source_summary: source kinds: hub, lsm, camera, simulator' 'GUI source metadata'
require_line "$gui" 'detector_controls: gain=1.100, noise=0.900' 'GUI detector gain/noise public property write/readback'
require_line "$gui" 'snapshot_frames: observed=1 latest=128x128 Mono16' 'GUI snapshot frame'
require_line "$gui" 'detector_gain=1.100, detector_noise=0.900' 'GUI frame detector gain/noise metadata'
require_line "$gui" 'live_progress: updates=4 last=4/4' 'GUI live progress'
require_line "$gui" 'line_chunks: observed=4 samples=256 channels=2' 'GUI line chunk summary'
require_line "$gui" 'first=[channels=counter0+ai0, line=0, chunk=0, first_sample=0, sample_rate_hz=100000, sample_period_s=0.000010000, detector_gain=1.100, detector_noise=0.900, dropped_chunks=0, dropped_samples=0, overflowed=false' 'GUI first chunk timing/drop metadata'

gui_composed=$(run_gui_example lsm_gui_composed_smoke lsm_gui sim-composed --smoke)
require_line "$gui_composed" 'source: sim-composed' 'composed GUI smoke source'
require_line "$gui_composed" 'scene_controls: stage_um=(180.000,-120.000,4252.000), lamp_power=0.650, lamp_enabled=true' 'composed GUI shared scene public state write'
require_line "$gui_composed" 'objective_control: position=3, magnification=60.0, numerical_aperture=0.90' 'composed GUI objective public capability selection'
require_line "$gui_composed" 'snapshot_frames: observed=1 latest=128x128 Mono16' 'composed GUI snapshot frame'
require_line "$gui_composed" 'scene=[stage_um=(180.000,-120.000,4252.000), sample_pixel_size_um=0.108, laser_power=0.650, laser_gate_enabled=true, magnification=60.0, numerical_aperture=0.90' 'composed GUI frame scene metadata'
require_line "$gui_composed" 'line_chunks: observed=4 samples=256 channels=2' 'composed GUI line chunk summary'
require_line "$gui_composed" 'scene=[stage_um=(180.000,-120.000,4252.000), sample_pixel_size_um=0.108, laser_power=0.650, laser_gate_enabled=true, magnification=60.0, numerical_aperture=0.90' 'composed GUI chunk scene metadata'

printf '# LSM Simulator Workflow Audit\n\n'
printf '| Workflow | Status |\n'
printf '| --- | --- |\n'
printf '| Confocal capture | ok |\n'
printf '| Mono8 reconstructed capture | ok |\n'
printf '| Confocal stream | ok |\n'
printf '| Scan-signal stream timing/drop metadata | ok |\n'
printf '| Line-dwell timing | ok |\n'
printf '| Live image cancellation | ok |\n'
printf '| Signal cancellation | ok |\n'
printf '| Composed brightfield/LSM simulator | ok |\n'
printf '| LSM GUI smoke frame/chunk metadata consumption | ok |\n'
printf '| LSM GUI composed shared scene/objective controls | ok |\n'
printf '\nThis audit runs simulator examples through public runtime APIs only. It does not create hardware tasks or provide NI-DAQmx evidence.\n'
