#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: scripts/audit-ni-daqmx-sdk-headers.sh <header-file-or-directory>

Inventories user-provided NI-DAQmx SDK headers for evidence intake. The input
must be a NIDAQmx.h file or a directory containing NIDAQmx.h. The output is
Markdown intended for docs/devices or hardware validation notes. This script
does not prove live behavior; it only records header identity and symbol
availability.
USAGE
}

if [[ $# -ne 1 ]]; then
  usage
  exit 2
fi

root=$1
if [[ ! -e "$root" ]]; then
  echo "NI-DAQmx header path does not exist: $root" >&2
  exit 1
fi

hash_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    echo "missing sha256sum or shasum" >&2
    exit 1
  fi
}

tmp_list=$(mktemp)
nidaqmx_list=$(mktemp)
trap 'rm -f "$tmp_list" "$nidaqmx_list"' EXIT

if [[ -f "$root" ]]; then
  printf '%s\n' "$root" >"$tmp_list"
else
  find "$root" -type f \( -name '*.h' -o -name '*.hpp' -o -name '*.hh' \) | sort >"$tmp_list"
fi

if [[ ! -s "$tmp_list" ]]; then
  echo "No C/C++ header files found under: $root" >&2
  exit 1
fi

while IFS= read -r header; do
  if [[ "$(basename "$header")" == "NIDAQmx.h" ]]; then
    printf '%s\n' "$header" >>"$nidaqmx_list"
  fi
done <"$tmp_list"

if [[ ! -s "$nidaqmx_list" ]]; then
  echo "No NIDAQmx.h file found under: $root" >&2
  exit 1
fi

combined_input=$(mktemp)
trap 'rm -f "$tmp_list" "$nidaqmx_list" "$combined_input"' EXIT
while IFS= read -r header; do
  digest=$(hash_file "$header")
  size=$(wc -c <"$header" | tr -d ' ')
  printf '%s  %s  %s\n' "$digest" "$size" "$header" >>"$combined_input"
done <"$tmp_list"
combined_digest=$(hash_file "$combined_input")

first_matching_line() {
  local pattern=$1
  local value
  value=$(xargs grep -h -m1 -E "$pattern" <"$tmp_list" 2>/dev/null || true)
  if [[ -z "$value" ]]; then
    echo "not found"
  else
    printf '%s\n' "$value" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//'
  fi
}

macro_report() {
  local label=$1
  local pattern=$2
  local exclude_pattern=${3:-}
  local matches
  matches=$(xargs grep -h -E "$pattern" <"$tmp_list" 2>/dev/null | sed 's/^[[:space:]]*//; s/[[:space:]]*$//' | sort -u || true)
  if [[ -n "$matches" && -n "$exclude_pattern" ]]; then
    matches=$(printf '%s\n' "$matches" | grep -Ev "$exclude_pattern" || true)
  fi
  if [[ -z "$matches" ]]; then
    echo "| $label | none |"
    return
  fi
  while IFS= read -r match; do
    echo "| $label | \`$match\` |"
    label=""
  done <<<"$matches"
}

echo "# NI-DAQmx SDK Header Inventory"
echo
echo "| Item | Value |"
echo "| --- | --- |"
echo "| Header root | \`$root\` |"
echo "| Header count | $(wc -l <"$tmp_list" | tr -d ' ') |"
echo "| NIDAQmx.h count | $(wc -l <"$nidaqmx_list" | tr -d ' ') |"
if [[ -s "$nidaqmx_list" ]]; then
  nidaqmx_paths=$(awk '{ printf "%s`%s`", sep, $0; sep=", " }' "$nidaqmx_list")
else
  nidaqmx_paths="none"
fi
echo "| NIDAQmx.h path | $nidaqmx_paths |"
echo "| Combined header inventory SHA-256 | \`$combined_digest\` |"
echo "| Header title line | \`$(first_matching_line 'Title:[[:space:]]+NIDAQmx\.h')\` |"
echo "| Copyright line | \`$(first_matching_line 'Copyright \(c\) National Instruments')\` |"
echo
echo "## Header Files"
echo
echo "| SHA-256 | Bytes | Header |"
echo "| --- | ---: | --- |"
while IFS= read -r line; do
  digest=$(printf '%s\n' "$line" | awk '{print $1}')
  size=$(printf '%s\n' "$line" | awk '{print $2}')
  header=${line#"$digest  $size  "}
  echo "| \`$digest\` | $size | \`$header\` |"
done <"$combined_input"

join_markdown_list() {
  local sep=""
  local item
  for item in "$@"; do
    printf '%s%s' "$sep" "$item"
    sep=", "
  done
}

symbol_report() {
  local label=$1
  shift
  local found=()
  local missing=()
  for symbol in "$@"; do
    if xargs grep -h -E "\\b${symbol}\\b" <"$tmp_list" >/dev/null 2>&1; then
      found+=("\`$symbol\`")
    else
      missing+=("\`$symbol\`")
    fi
  done
  local found_text="none"
  local missing_text="none"
  if [[ ${#found[@]} -gt 0 ]]; then
    found_text=$(join_markdown_list "${found[@]}")
  fi
  if [[ ${#missing[@]} -gt 0 ]]; then
    missing_text=$(join_markdown_list "${missing[@]}")
  fi
  echo "| $label | $found_text | $missing_text |"
}

echo
echo "## Expected Symbol Availability"
echo
echo "| Area | Found | Missing |"
echo "| --- | --- | --- |"
symbol_report "Task lifecycle" DAQmxCreateTask DAQmxStartTask DAQmxStopTask DAQmxClearTask DAQmxWaitUntilTaskDone
symbol_report "Analog output" DAQmxCreateAOVoltageChan DAQmxWriteAnalogF64
symbol_report "Digital output" DAQmxCreateDOChan DAQmxWriteDigitalLines
symbol_report "Analog input" DAQmxCreateAIVoltageChan DAQmxReadAnalogF64
symbol_report "Counter input" DAQmxCreateCICountEdgesChan DAQmxReadCounterU32 DAQmxReadCounterF64
symbol_report "Counter output" DAQmxCreateCOPulseChanFreq DAQmxCreateCOPulseChanTicks
symbol_report "Timing and triggers" DAQmxCfgSampClkTiming DAQmxCfgImplicitTiming DAQmxCfgDigEdgeStartTrig DAQmxCfgDigEdgeRefTrig
symbol_report "Errors" DAQmxGetExtendedErrorInfo DAQmxFailed DAQmxGetErrorString
symbol_report "Runtime version" DAQmxGetSysNIDAQMajorVersion DAQmxGetSysNIDAQMinorVersion DAQmxGetSysNIDAQUpdateVersion

echo
echo "## Version Metadata In Header"
echo
echo "| Item | Header text |"
echo "| --- | --- |"
macro_report "Runtime version property" '^#define[[:space:]]+DAQmx_Sys_NIDAQ(Major|Minor|Update)Version[[:space:]]'
macro_report "Literal package version macro" '^#define[[:space:]]+.*(DAQmx|NIDAQ).*(VERSION|Version)[[:space:]]+[0-9]' 'DAQmx_Sys_NIDAQ(Major|Minor|Update)Version'
echo
echo "The NI-DAQmx header exposes runtime-version property IDs and getter"
echo "functions. This audit does not infer the installed package version from the"
echo "header when no literal package-version macro is present; pair it with"
echo "\`daqmx_runtime_probe\` and package-intake evidence for version claims."

echo
echo "## Evidence Boundary"
echo
echo "This inventory records SDK header identity and symbol availability only. It is"
echo "not evidence of task ordering, trigger routing, completion semantics, safe stop,"
echo "or real hardware behavior. Those require API audit notes and bench validation."
echo "The audit exits non-zero when no \`NIDAQmx.h\` is found. When auditing an"
echo "installed SDK directory for NI-DAQmx binding generation, the reported"
echo "\`NIDAQmx.h path\` must be the target-platform header used for bindgen."
