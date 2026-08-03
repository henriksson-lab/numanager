#!/usr/bin/env bash
set -euo pipefail

manifest=${1:-crates/numanager-imswitch-daqmx/Cargo.toml}
bin_dir=${2:-crates/numanager-imswitch-daqmx/src/bin}
lib_file=${3:-crates/numanager-imswitch-daqmx/src/lib.rs}

missing=0

require_file() {
  local path=$1
  if [[ ! -f "$path" ]]; then
    printf 'missing required file: %s\n' "$path" >&2
    missing=1
  fi
}

require_literal() {
  local path=$1
  local literal=$2
  local description=$3
  if ! rg -F "$literal" "$path" >/dev/null; then
    printf 'missing %s in %s: %s\n' "$description" "$path" "$literal" >&2
    missing=1
  fi
}

require_absent() {
  local path=$1
  local pattern=$2
  local description=$3
  if rg -n "$pattern" "$path" >/dev/null; then
    printf 'unexpected %s in %s:\n' "$description" "$path" >&2
    rg -n "$pattern" "$path" >&2
    missing=1
  fi
}

require_file "$manifest"
require_file "$lib_file"

if [[ $missing -ne 0 ]]; then
  exit 1
fi

require_literal "$manifest" '[target.'"'"'cfg(any(target_os = "linux", target_os = "windows"))'"'"'.dependencies]' 'Linux/Windows target-scoped dependency table'
require_literal "$manifest" 'ni-daqmx-sys = { path = "/home/mahogny/github/claude/ni-daqmx-sys", optional = true }' 'optional ni-daqmx-sys dependency'
require_literal "$manifest" 'ni-daqmx-sdk = ["dep:ni-daqmx-sys"]' 'ni-daqmx-sdk feature dependency'

top_level_dep_lines=$(
  awk '
    /^\[dependencies\]$/ { in_deps=1; next }
    /^\[/ { in_deps=0 }
    in_deps && /ni-daqmx-sys/ { print }
  ' "$manifest"
)
if [[ -n "$top_level_dep_lines" ]]; then
  printf 'ni-daqmx-sys must not be a top-level dependency:\n%s\n' "$top_level_dep_lines" >&2
  missing=1
fi

required_feature_count=$(rg -F 'required-features = ["ni-daqmx-sdk"]' "$manifest" | wc -l | tr -d ' ')
if [[ "$required_feature_count" != "5" ]]; then
  printf 'expected 5 DAQmx helper bins gated by required-features; found %s\n' "$required_feature_count" >&2
  missing=1
fi

helpers=(
  daqmx_inventory_helper
  daqmx_task_lifecycle_helper
  daqmx_channel_setup_helper
  daqmx_plan_setup_helper
  daqmx_io_smoke_helper
)

for helper in "${helpers[@]}"; do
  wrapper="$bin_dir/${helper}.rs"
  impl="$bin_dir/${helper}_impl.rs"
  require_file "$wrapper"
  require_file "$impl"
  if [[ ! -f "$wrapper" || ! -f "$impl" ]]; then
    continue
  fi
  require_literal "$wrapper" '#[cfg(any(target_os = "linux", target_os = "windows"))]' "$helper supported-target cfg"
  require_literal "$wrapper" '#[cfg(not(any(target_os = "linux", target_os = "windows")))]' "$helper unsupported-target cfg"
  require_literal "$wrapper" "#[path = \"${helper}_impl.rs\"]" "$helper supported implementation path"
  require_literal "$wrapper" 'ExitCode::FAILURE' "$helper unsupported-target failure exit"
  require_literal "$wrapper" 'requires a Linux or Windows NI-DAQmx SDK target' "$helper unsupported-target message"
  require_absent "$wrapper" 'ni_daqmx_sys::' "$helper direct NI-DAQmx FFI reference in target wrapper"
  require_literal "$impl" 'ni_daqmx_sys::' "$helper NI-DAQmx implementation FFI reference"
done

require_literal "$lib_file" '#[cfg(all(' 'library SDK-supported cfg boundary'
require_literal "$lib_file" 'any(target_os = "linux", target_os = "windows")' 'library Linux/Windows target gate'
require_literal "$lib_file" 'not(any(target_os = "linux", target_os = "windows"))' 'library unsupported-target gate'
require_literal "$lib_file" '"target_platform_linux_or_windows"' 'unsupported-target readiness blocker'
require_literal "$lib_file" 'cfg!(any(target_os = "linux", target_os = "windows"))' 'runtime target_supported metadata'

if [[ $missing -ne 0 ]]; then
  exit 1
fi

printf '# NI-DAQmx Target Scope Audit\n\n'
printf '| Boundary | Status |\n'
printf '| --- | --- |\n'
printf '| `ni-daqmx-sys` dependency target-scoped to Linux/Windows | ok |\n'
printf '| `ni-daqmx-sdk` feature maps only to optional `ni-daqmx-sys` | ok |\n'
printf '| Helper binaries require `ni-daqmx-sdk` | ok |\n'
printf '| Helper wrappers use Linux/Windows implementation cfgs | ok |\n'
printf '| Helper wrappers provide unsupported-target failure stubs | ok |\n'
printf '| Helper wrappers do not reference NI-DAQmx FFI directly | ok |\n'
printf '| Helper implementation files contain NI-DAQmx FFI references | ok |\n'
printf '| Runtime readiness reports Linux/Windows target support boundary | ok |\n'
printf '\nThis audit checks numanager source boundaries only. It does not prove Windows ABI compatibility, NI-DAQmx runtime installation, task behavior, or hardware behavior.\n'
