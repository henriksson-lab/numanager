#!/usr/bin/env bash
set -euo pipefail

tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

run_probe() {
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

config_only=$(run_probe config_only env NUMANAGER_DAQMX_CONFIG_ONLY=1 cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe)
require_line "$config_only" 'config_only: true' 'config-only marker'
require_line "$config_only" 'connected: Bool(false)' 'config-only disconnected state'
require_line "$config_only" '"connect_requested": Bool(false)' 'config-only connect gate'
require_line "$config_only" '"execution_status": String("not_live_backend")' 'config-only non-live backend status'
require_line "$config_only" '"runtime_detected": Bool(false)' 'config-only no runtime detection'
require_line "$config_only" 'live_task_execution_ready=false' 'config-only live execution gate'
require_line "$config_only" 'inventory: requested=false, helper=false, detected_devices=0, configured_device_detected=false, configured_device=none, error=none' 'config-only inventory summary'
require_line "$config_only" 'promotion_gates: legal_review, installed_windows_package_license_review, installed_linux_26_5_header_audit, installed_windows_26_5_header_audit, ni_pal_device_inventory, bench_safety_preconditions, task_ordering_routing_completion_cleanup_bench_validation, runtime_publication_hardware_validation, hardware_validation_note' 'config-only external promotion gates'
require_line "$config_only" '"external_promotion_gate_statuses": Map' 'config-only structured promotion gate statuses'
require_line "$config_only" 'promotion_gate_statuses: pending=9' 'config-only promotion gate status summary'
require_absent "$config_only" 'runtime_version:' 'config-only runtime-version output'

metadata_only=$(run_probe metadata_only env NUMANAGER_DAQMX_CONFIG_ONLY=1 NUMANAGER_DAQMX_RUNTIME_VERSION=26.5 cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe)
require_line "$metadata_only" 'config_only: true' 'metadata-only config-only marker'
require_line "$metadata_only" 'configured_runtime_version: 26.5 (major=26, minor=5)' 'configured runtime version parse'
require_line "$metadata_only" '"runtime_version_comparison": String("not_detected")' 'metadata-only comparison gate'
require_line "$metadata_only" '"runtime_version_comparison_basis": String("runtime_probe_missing")' 'metadata-only comparison basis'
require_line "$metadata_only" 'live_task_execution_ready=false' 'metadata-only live execution gate'
require_line "$metadata_only" 'missing: runtime_version_unverified, api_audit_and_hardware_validation' 'metadata-only runtime-version unverified gate'
require_line "$metadata_only" 'inventory: requested=false, helper=false, detected_devices=0, configured_device_detected=false, configured_device=none, error=none' 'metadata-only inventory summary'
require_line "$metadata_only" 'promotion_gates: legal_review, installed_windows_package_license_review, installed_linux_26_5_header_audit, installed_windows_26_5_header_audit, ni_pal_device_inventory, bench_safety_preconditions, task_ordering_routing_completion_cleanup_bench_validation, runtime_publication_hardware_validation, hardware_validation_note' 'metadata-only external promotion gates'
require_line "$metadata_only" 'promotion_gate_statuses: pending=9' 'metadata-only promotion gate status summary'

build=$(run_probe helper_build cargo build -p numanager-imswitch-daqmx --features ni-daqmx-sdk --bin numanager-daqmx-inventory-helper)
require_line "$build" 'Finished' 'inventory helper build completion'

isolated=$(run_probe isolated_runtime_probe env NUMANAGER_DAQMX_RUNTIME_HELPER=target/debug/numanager-daqmx-inventory-helper cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe)
require_line "$isolated" 'connected: Bool(true)' 'isolated probe connected state'
require_line "$isolated" '"connect_requested": Bool(true)' 'isolated probe connect request'
require_line "$isolated" '"execution_status": String("runtime_probe_only")' 'isolated probe execution status'
require_line "$isolated" '"inventory_helper_configured": Bool(true)' 'isolated helper configured marker'
require_line "$isolated" '"device_inventory_requested": Bool(false)' 'isolated version-only inventory gate'
require_line "$isolated" 'runtime_version:' 'isolated runtime-version output or contained unknown'
require_line "$isolated" 'live_task_execution_ready=false' 'isolated probe live execution gate'
require_line "$isolated" 'inventory: requested=false, helper=true, detected_devices=0, configured_device_detected=false, configured_device=none, error=' 'isolated probe inventory summary'
require_line "$isolated" 'missing: api_audit_and_hardware_validation' 'isolated probe hardware-validation gate'
require_line "$isolated" 'promotion_gates: legal_review, installed_windows_package_license_review, installed_linux_26_5_header_audit, installed_windows_26_5_header_audit, ni_pal_device_inventory, bench_safety_preconditions, task_ordering_routing_completion_cleanup_bench_validation, runtime_publication_hardware_validation, hardware_validation_note' 'isolated probe external promotion gates'
require_line "$isolated" 'promotion_gate_statuses: pending=9' 'isolated probe promotion gate status summary'

isolated_version_unverified=$(run_probe isolated_runtime_version_unverified env NUMANAGER_DAQMX_RUNTIME_HELPER=target/debug/numanager-daqmx-inventory-helper NUMANAGER_DAQMX_RUNTIME_VERSION=26.5 cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe)
require_line "$isolated_version_unverified" 'configured_runtime_version: 26.5 (major=26, minor=5)' 'isolated configured runtime version parse'
require_line "$isolated_version_unverified" 'runtime_version_comparison: unknown (matches=unknown, basis=detected_runtime_version_partial)' 'isolated runtime-version unverified comparison'
require_line "$isolated_version_unverified" 'blocker=runtime_version_unverified' 'isolated runtime-version unverified blocker'
require_line "$isolated_version_unverified" 'missing: runtime_version_unverified, api_audit_and_hardware_validation' 'isolated runtime-version unverified missing evidence'
require_line "$isolated_version_unverified" 'live_task_execution_ready=false' 'isolated runtime-version unverified live gate'

hardware_gate=$(run_probe hardware_gate env NUMANAGER_DAQMX_RUNTIME_HELPER=target/debug/numanager-daqmx-inventory-helper NUMANAGER_DAQMX_LIVE_TASK_EXECUTION=1 cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe)
require_line "$hardware_gate" 'live_task_execution_requested=true' 'hardware-gate live intent'
require_line "$hardware_gate" 'blocker=pending_hardware_validation' 'hardware-gate blocker'
require_line "$hardware_gate" 'missing: api_audit_and_hardware_validation' 'hardware-gate missing evidence'
require_line "$hardware_gate" '"package_identity_recorded": Bool(true)' 'hardware-gate package metadata'
require_line "$hardware_gate" '"sdk_header_recorded": Bool(true)' 'hardware-gate header metadata'
require_line "$hardware_gate" '"runtime_detected": Bool(true)' 'hardware-gate runtime probe'
require_line "$hardware_gate" 'live_task_execution_ready=false' 'hardware-gate live gate remains closed'

inventory=$(run_probe inventory_probe env NUMANAGER_DAQMX_INVENTORY=1 NUMANAGER_DAQMX_INVENTORY_HELPER=target/debug/numanager-daqmx-inventory-helper cargo run -p numanager-examples --features ni-daqmx-sdk -- daqmx_runtime_probe)
require_line "$inventory" 'connected: Bool(true)' 'inventory probe connected state'
require_line "$inventory" '"device_inventory_requested": Bool(true)' 'inventory probe request marker'
require_line "$inventory" '"inventory_helper_configured": Bool(true)' 'inventory probe helper configured marker'
require_line "$inventory" 'inventory: requested=true, helper=true, detected_devices=0, configured_device_detected=false, configured_device=none, error=' 'inventory probe compact summary'
require_line "$inventory" 'live_task_execution_ready=false' 'inventory probe live execution gate'
require_line "$inventory" 'promotion_gate_statuses: pending=9' 'inventory probe promotion gate status summary'

printf '# NI-DAQmx Runtime Probe Audit\n\n'
printf '| Workflow | Status |\n'
printf '| --- | --- |\n'
printf '| Config-only readiness probe avoids runtime loading | ok |\n'
printf '| Configured package-version metadata parses without runtime loading | ok |\n'
printf '| Process-isolated runtime-version probe remains probe-only | ok |\n'
printf '| Configured runtime-version mismatch or partial detection blocks live execution | ok |\n'
printf '| Live-task intent with metadata and runtime probe reaches hardware-validation blocker | ok |\n'
printf '| Process-isolated inventory probe remains evidence-only | ok |\n'
printf '| Compact inventory readiness summaries are emitted | ok |\n'
printf '\nThis audit runs public `daqmx_runtime_probe` workflows through the optional SDK feature. The config-only paths avoid loading the vendor runtime. The isolated probe may load NI-DAQmx in the helper process, but the runtime process stays in `runtime_probe_only` and keeps `live_task_execution_ready=false`, even when the helper reports a contained runtime-version failure. When package/header metadata, runtime probing, and live-task intent are all present, the blocker advances only to `pending_hardware_validation`. It does not create NI-DAQmx tasks, write outputs, read inputs, execute scans, or provide hardware evidence.\n'
