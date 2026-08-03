#!/usr/bin/env bash
set -euo pipefail

package_inputs=${NUMANAGER_DAQMX_PACKAGE_INPUTS:-/home/mahogny/github/claude/reveng-dll/nidaq}
header_root=${NUMANAGER_DAQMX_HEADER_ROOT:-/usr/include/NIDAQmx.h}
sys_repo=${NUMANAGER_DAQMX_SYS_REPO:-/home/mahogny/github/claude/ni-daqmx-sys}

tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

run_audit() {
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
    sed -n '1,220p' "$file" >&2
    exit 1
  fi
}

package=$(run_audit package_inputs scripts/audit-ni-daqmx-package-inputs.sh "$package_inputs")
require_line "$package" '# NI-DAQmx Package Input Inventory' 'package inventory heading'
require_line "$package" 'NILinux2026Q3DeviceDrivers.zip' 'Linux package-input archive identity'
require_line "$package" 'ni-daqmx_26.5_online.exe' 'Windows package-input installer identity'
require_line "$package" 'NI Released License Agreement - English.txt' 'Linux package license-file identity'
require_line "$package" 'Evidence Boundary' 'package evidence boundary'

header=$(run_audit sdk_headers scripts/audit-ni-daqmx-sdk-headers.sh "$header_root")
require_line "$header" '# NI-DAQmx SDK Header Inventory' 'header inventory heading'
require_line "$header" '| NIDAQmx.h count | 1 |' 'single NIDAQmx.h header count'
require_line "$header" 'DAQmxCreateTask' 'task lifecycle symbol'
require_line "$header" '| Runtime version | `DAQmxGetSysNIDAQMajorVersion`' 'runtime-version getter symbols'
require_line "$header" '| Literal package version macro | none |' 'literal package-version boundary'
require_line "$header" 'This inventory records SDK header identity and symbol availability only.' 'header evidence boundary'

source=$(run_audit ffi_source scripts/audit-ni-daqmx-sys-source.sh "$sys_repo")
require_line "$source" '# NI-DAQmx Sys Source Inventory' 'FFI source inventory heading'
require_line "$source" '| build.rs has Linux x86_64 library path | present |' 'Linux link path source boundary'
require_line "$source" '| build.rs has Windows library path | present |' 'Windows link path source boundary'
require_line "$source" '| macOS explicitly unsupported | ok |' 'macOS unsupported verdict'
require_line "$source" '| Other non-Linux/non-Windows targets explicitly unsupported | ok |' 'unsupported target verdict'
require_line "$source" '| Runtime version | `DAQmxGetSysNIDAQMajorVersion`' 'runtime-version binding symbols'
require_line "$source" 'This inventory records the local FFI source used by numanager' 'FFI evidence boundary'

printf '# NI-DAQmx Evidence Input Audit\n\n'
printf '| Input | Path |\n'
printf '| --- | --- |\n'
printf '| Package inputs | `%s` |\n' "$package_inputs"
printf '| Header root | `%s` |\n' "$header_root"
printf '| ni-daqmx-sys repo | `%s` |\n' "$sys_repo"
printf '\n| Workflow | Status |\n'
printf '| --- | --- |\n'
printf '| Package input inventory markers | ok |\n'
printf '| Installed SDK header inventory markers | ok |\n'
printf '| FFI source inventory markers | ok |\n'
printf '\nThis audit runs package-input, SDK-header, and FFI-source inventory scripts over the configured local paths. It records intake/source markers only; it does not load the NI-DAQmx runtime, create NI-DAQmx tasks, write outputs, read inputs, execute scans, establish redistribution permission, or provide hardware evidence.\n'
