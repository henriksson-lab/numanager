#!/usr/bin/env bash
set -euo pipefail

require_line() {
  local file=$1
  local pattern=$2
  local description=$3
  if ! rg -F -- "$pattern" "$file" >/dev/null; then
    printf 'missing %s in %s: %s\n' "$description" "$file" "$pattern" >&2
    exit 1
  fi
}

require_line docs/devices/ni-daqmx-package-intake.md 'Complete legal review of the identified Linux license files' 'Linux license legal-review gate'
require_line docs/devices/ni-daqmx-package-intake.md 'Install or further extract the Windows package in an appropriate Windows' 'Windows installed package/license gate'
require_line docs/devices/ni-daqmx-package-intake.md 'Audit installed Linux or Windows 26.5 SDK headers' 'installed 26.5 header audit gate'
require_line docs/devices/ni-daqmx-package-intake.md 'Keep live task execution disabled until the bench checklist records runtime' 'package-intake live execution gate'
require_line docs/run_examples.md 'package/header/FFI-source and external-gates evidence commands' 'run examples bring-up external-gates prose'
require_line docs/run_examples.md 'bench safety' 'run examples external-gates safety prose'
require_line docs/devices/imswitch-daqmx.md 'package/header/FFI-source and external-gates evidence commands' 'ImSwitch bring-up external-gates prose'
require_line docs/devices/ni-daqmx-bench-validation-checklist.md 'Package/license boundary' 'bench checklist package/license uncertainty'
require_line docs/devices/ni-daqmx-bench-validation-checklist.md 'External-gates audit command' 'bench checklist external-gates artifact'
require_line docs/devices/ni-daqmx-bench-validation-checklist.md 'External-gates audit | `scripts/audit-ni-daqmx-external-gates.sh`' 'bench checklist external-gates safe sequence'
require_line docs/devices/ni-daqmx-bench-validation-checklist.md 'Installed 26.5 headers' 'bench checklist installed-header uncertainty'
require_line docs/devices/ni-daqmx-bench-validation-checklist.md 'Installed target-platform `NIDAQmx.h` used for bindgen' 'bench checklist installed header bindgen pairing artifact'
require_line docs/devices/ni-daqmx-bench-validation-checklist.md 'Bindgen regeneration command' 'bench checklist bindgen regeneration artifact'
require_line docs/devices/ni-daqmx-sdk-evidence-template.md 'External-gates audit command' 'SDK evidence template external-gates artifact'
require_line docs/devices/ni-daqmx-sdk-evidence-template.md '## External Gates Audit' 'SDK evidence template external-gates section'
require_line docs/devices/ni-daqmx-sdk-evidence-template.md 'Installed target-platform NIDAQmx.h used for bindgen' 'SDK evidence template bindgen header pairing artifact'
require_line docs/devices/ni-daqmx-sdk-evidence-template.md 'Bindgen regeneration command' 'SDK evidence template bindgen regeneration artifact'
require_line docs/devices/ni-daqmx-bench-validation-checklist.md 'Linux NI-PAL readiness' 'bench checklist NI-PAL readiness uncertainty'
require_line docs/devices/ni-daqmx-bench-validation-checklist.md 'Runtime publication' 'bench checklist runtime publication uncertainty'
require_line docs/devices/ni-daqmx-bench-validation-checklist.md 'Bench safety preconditions' 'bench checklist safety precondition gate'
require_line docs/devices/ni-daqmx-bench-validation-checklist.md '--execute --bench-safety-reviewed' 'bench checklist acknowledged execute gate'
require_line docs/devices/imswitch-daqmx.md 'live NI-DAQmx task execution is intentionally not exposed yet' 'ImSwitch live execution support boundary'
require_line docs/devices/evidence.md 'Live task execution remains unexposed' 'evidence table live execution boundary'
require_line docs/planning/lsm-simulation-and-daqmx-plan.md 'legal review, installed Windows' 'saved plan external review gate'
require_line docs/planning/lsm-simulation-and-daqmx-plan.md 'bench_safety_preconditions' 'saved plan structured bench-safety gate'
require_line docs/planning/lsm-simulation-and-daqmx-plan.md 'task execution awaiting hardware validation' 'saved plan hardware validation gate'

printf '# NI-DAQmx External Gates Audit\n\n'
printf '| Gate | Status |\n'
printf '| --- | --- |\n'
printf '| License and redistribution review remains explicit | ok |\n'
printf '| Installed Windows package/license review remains explicit | ok |\n'
printf '| Installed Linux/Windows 26.5 header audit remains explicit | ok |\n'
printf '| NI-PAL/device inventory and runtime publication need bench evidence | ok |\n'
printf '| Bench safety preconditions remain explicit before execute helpers | ok |\n'
printf '| Live NI-DAQmx task execution remains unexposed pending hardware validation | ok |\n'
printf '\nThis audit checks that non-code external gates for the NI-DAQmx backend remain documented and visible. It does not complete legal review, audit installed Windows headers, initialize NI-PAL, approve bench wiring/safety, create NI-DAQmx tasks, or provide hardware validation evidence.\n'
