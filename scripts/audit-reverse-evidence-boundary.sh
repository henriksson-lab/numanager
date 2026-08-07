#!/usr/bin/env bash
set -euo pipefail

driver_lib=${1:-crates/numanager-drivers/src/lib.rs}
driver_src_dir=$(dirname "$driver_lib")
evidence_file=${EVIDENCE_FILE:-docs/devices/evidence.md}
device_docs_dir=${DEVICE_DOCS_DIR:-docs/devices}
reverse_docs_dir=${REVERSE_DOCS_DIR:-docs/reverse}
artifact_summary_file=${ARTIFACT_SUMMARY_FILE:-${reverse_docs_dir}/artifact-inspection-summary.md}
reverse_index_file=${REVERSE_INDEX_FILE:-${reverse_docs_dir}/README.md}
evidence_gate_audit_file=${EVIDENCE_GATE_AUDIT_FILE:-${reverse_docs_dir}/evidence-gate-audit.md}
protocol_evidence_plan_file=${PROTOCOL_EVIDENCE_PLAN_FILE:-docs/protocol_evidence_plan.md}
device_index_file=${DEVICE_INDEX_FILE:-docs/devices/README.md}
readme_file=${README_FILE:-README.md}
run_examples_file=${RUN_EXAMPLES_FILE:-docs/run_examples.md}
example_outputs_file=${EXAMPLE_OUTPUTS_FILE:-docs/example_outputs.md}
audit_root=${AUDIT_ROOT:-.}
workspace_file=${WORKSPACE_FILE:-Cargo.toml}

missing=0

require_file() {
  path=$1
  if [ ! -f "$path" ]; then
    printf 'missing required reverse-evidence artifact: %s\n' "$path" >&2
    missing=1
  fi
}

require_evidence_row() {
  module=$1
  if ! rg -n "^\\| (pending \`${module}\`|\`numanager_drivers::${module}\`)" "$evidence_file" >/dev/null; then
    printf 'missing reverse-evidence register row: %s\n' "$module" >&2
    missing=1
  fi
}

require_device_index_row() {
  module=$1
  page=$2
  row_prefix='| `numanager_drivers::'"${module}"'`'
  link="[${page}](${page})"
  if ! rg -F "$row_prefix" "$device_index_file" | rg -F "$link" >/dev/null; then
    printf 'missing reverse-evidence device-index row: %s -> %s\n' "$module" "$page" >&2
    missing=1
  fi
}

require_readme_index_row() {
  family=$1
  page=$2
  linked_family="[${family}](docs/devices/${page})"
  if ! rg -F "| ${linked_family} |" "$readme_file" >/dev/null; then
    printf 'missing reverse-evidence README index row: %s -> %s\n' "$family" "$page" >&2
    missing=1
  fi
}

readme_hardware_pages() {
  awk '
    /^### Hardware devices$/ { in_table=1; next }
    /^### / && in_table { exit }
    in_table && /^\| \[/ {
      line=$0
      while (match(line, /\(docs\/devices\/[a-z0-9-]+\.md\)/)) {
        page=substr(line, RSTART + 14, RLENGTH - 15)
        print page
        line=substr(line, RSTART + RLENGTH)
      }
    }
  ' "$readme_file" | sort -u
}

readme_hardware_rows_with_bad_checkbox() {
  awk '
    /^### Hardware devices$/ { in_table=1; next }
    /^### / && in_table { exit }
    in_table && /^\| \[/ {
      if ($0 !~ /\| (-|✓) \|$/) {
        print
      }
    }
  ' "$readme_file"
}

readme_hardware_rows_with_stale_scope_words() {
  awk '
    /^### Hardware devices$/ { in_table=1; next }
    /^### / && in_table { exit }
    in_table && /^\| \[/ {
      if ($0 ~ /(Bounded|bounded|slice|fixture fallback|fallback surface|configured fixture|limited evidence surface)/) {
        print
      }
    }
  ' "$readme_file"
}

readme_simulator_pages() {
  awk '
    /^### Simulators$/ { in_table=1; next }
    /^### / && in_table { exit }
    in_table && /^\| \[/ {
      line=$0
      while (match(line, /\(docs\/devices\/[a-z0-9-]+\.md\)/)) {
        page=substr(line, RSTART + 14, RLENGTH - 15)
        print page
        line=substr(line, RSTART + RLENGTH)
      }
    }
  ' "$readme_file" | sort -u
}

device_index_pages() {
  awk '
    /^## Supported Drivers$/ { in_table=1; next }
    /^## / && in_table { exit }
    in_table && /^\| `numanager_drivers::/ {
      line=$0
      while (match(line, /\[[^]]+\]\([a-z0-9-]+\.md\)/)) {
        entry=substr(line, RSTART, RLENGTH)
        sub(/^.*\]\(/, "", entry)
        sub(/\)$/, "", entry)
        if (entry !~ /^sim(-[a-z0-9-]+)?\.md$/) {
          print entry
        }
        line=substr(line, RSTART + RLENGTH)
      }
    }
  ' "$device_index_file" | sort -u
}

device_index_modules_for_page() {
  page=$1
  awk -v page="$page" '
    /^## Supported Drivers$/ { in_table=1; next }
    /^## / && in_table { exit }
    in_table && /^\| `numanager_drivers::/ && index($0, "](" page ")") {
      line=$0
      while (match(line, /`numanager_drivers::[a-z0-9_]+`/)) {
        module=substr(line, RSTART, RLENGTH)
        sub(/^`numanager_drivers::/, "", module)
        sub(/`$/, "", module)
        print module
        line=substr(line, RSTART + RLENGTH)
      }
    }
  ' "$device_index_file" | sort -u
}

exported_driver_modules() {
  awk '
    /^pub mod [a-z0-9_]+;/ {
      module=$3
      sub(/;$/, "", module)
      # Host-side USB support modules, not device drivers: they describe no
      # hardware, so they have no device page or index row.
      if (module == "usb_discovery" || module == "winusb_access" || module == "spark") {
        next
      }
      print module
    }
  ' "$driver_lib" | sort -u
}

device_index_modules() {
  awk '
    /^## Supported Drivers$/ { in_table=1; next }
    /^## / && in_table { exit }
    in_table && /^\| `numanager_drivers::/ {
      line=$0
      while (match(line, /`numanager_drivers::[a-z0-9_]+`/)) {
        module=substr(line, RSTART, RLENGTH)
        sub(/^`numanager_drivers::/, "", module)
        sub(/`$/, "", module)
        print module
        line=substr(line, RSTART + RLENGTH)
      }
    }
  ' "$device_index_file" | sort -u
}

require_hardware_checklist_consistency() {
  local readme_pages simulator_pages index_pages exported_modules index_modules missing_from_index missing_from_readme missing_from_device_index unexported_index_modules simulator_index_pages missing_simulator_pages bad_checkbox_rows stale_scope_rows page path modules module
  readme_pages=$(readme_hardware_pages)
  simulator_pages=$(readme_simulator_pages)
  index_pages=$(device_index_pages)
  exported_modules=$(exported_driver_modules)
  index_modules=$(device_index_modules)

  bad_checkbox_rows=$(readme_hardware_rows_with_bad_checkbox)
  if [ -n "$bad_checkbox_rows" ]; then
    printf 'README hardware checklist rows must end with Tested on hardware marker - or ✓:\n%s\n' "$bad_checkbox_rows" >&2
    missing=1
  fi
  stale_scope_rows=$(readme_hardware_rows_with_stale_scope_words)
  if [ -n "$stale_scope_rows" ]; then
    printf 'README hardware checklist rows contain stale bounded/fallback wording:\n%s\n' "$stale_scope_rows" >&2
    missing=1
  fi

  missing_from_device_index=$(
    comm -23 <(printf '%s\n' "$exported_modules") <(printf '%s\n' "$index_modules") \
      || true
  )
  if [ -n "$missing_from_device_index" ]; then
    printf 'exported driver module(s) missing from docs/devices supported-driver index:\n%s\n' "$missing_from_device_index" >&2
    missing=1
  fi

  unexported_index_modules=$(
    comm -13 <(printf '%s\n' "$exported_modules") <(printf '%s\n' "$index_modules") \
      || true
  )
  if [ -n "$unexported_index_modules" ]; then
    printf 'docs/devices supported-driver index references unexported module(s):\n%s\n' "$unexported_index_modules" >&2
    missing=1
  fi

  simulator_index_pages=$(
    awk '
      /^## Supported Drivers$/ { in_table=1; next }
      /^## / && in_table { exit }
      in_table && /^\| `numanager_drivers::sim(_[a-z0-9_]+)?`/ {
        line=$0
        while (match(line, /\[[^]]+\]\([a-z0-9-]+\.md\)/)) {
          entry=substr(line, RSTART, RLENGTH)
          sub(/^.*\]\(/, "", entry)
          sub(/\)$/, "", entry)
          print entry
          line=substr(line, RSTART + RLENGTH)
        }
      }
    ' "$device_index_file" | sort -u
  )
  missing_simulator_pages=$(
    comm -23 <(printf '%s\n' "$simulator_index_pages") <(printf '%s\n' "$simulator_pages") \
      || true
  )
  if [ -n "$missing_simulator_pages" ]; then
    printf 'docs/devices simulator page(s) missing from README simulator section:\n%s\n' "$missing_simulator_pages" >&2
    missing=1
  fi

  missing_from_index=$(
    comm -23 <(printf '%s\n' "$readme_pages") <(printf '%s\n' "$index_pages") \
      | rg -v '^(andor-camera|evidence|hardware-validation-template|README)\.md$' \
      || true
  )
  if [ -n "$missing_from_index" ]; then
    printf 'README hardware checklist page(s) missing from docs/devices supported-driver index:\n%s\n' "$missing_from_index" >&2
    missing=1
  fi

  missing_from_readme=$(
    comm -13 <(printf '%s\n' "$readme_pages") <(printf '%s\n' "$index_pages") \
      | rg -v '^(andor-camera|sim)\.md$' \
      || true
  )
  if [ -n "$missing_from_readme" ]; then
    printf 'docs/devices supported-driver page(s) missing from README hardware checklist:\n%s\n' "$missing_from_readme" >&2
    missing=1
  fi

  while IFS= read -r page; do
    [ -n "$page" ] || continue
    path="${device_docs_dir}/${page}"
    require_file "$path"
    if ! rg -n '^## (Status|Status And Provenance)$' "$path" >/dev/null; then
      printf 'README hardware checklist page lacks status/provenance section: %s\n' "$page" >&2
      missing=1
    fi
    require_device_page_section "$page" "Capabilities"

    if ! rg -n '^\| Support level \|' "$path" >/dev/null; then
      printf 'README hardware checklist page lacks support-level row: %s\n' "$page" >&2
      missing=1
    fi
    if ! rg -n '^\| `[^`]+` \|' "$path" >/dev/null; then
      printf 'README hardware checklist page lacks capability/property rows: %s\n' "$page" >&2
      missing=1
    fi

    modules=$(device_index_modules_for_page "$page")
    if [ -z "$modules" ]; then
      printf 'README hardware checklist page has no driver module in docs/devices index: %s\n' "$page" >&2
      missing=1
      continue
    fi

    while IFS= read -r module; do
      [ -n "$module" ] || continue
      require_export "$module"
      require_evidence_row "$module"
    done <<< "$modules"
  done <<< "$readme_pages"
}

require_reverse_index_row() {
  target=$1
  page=$2
  required_text=$3
  link='[`'"${page}"'`]('"${page}"')'
  if ! rg -F "| ${target} |" "$reverse_index_file" | rg -F "$link" | rg -F "$required_text" >/dev/null; then
    printf 'missing reverse-evidence reverse-index row: %s -> %s\n' "$target" "$required_text" >&2
    missing=1
  fi
}

require_text() {
  path=$1
  text=$2
  description=$3
  if ! rg -F "$text" "$path" >/dev/null; then
    printf 'missing reverse-evidence text: %s -> %s\n' "$description" "$text" >&2
    missing=1
  fi
}

reject_text() {
  path=$1
  text=$2
  description=$3
  if rg -F "$text" "$path" >/dev/null; then
    printf 'forbidden reverse-evidence text: %s -> %s\n' "$description" "$text" >&2
    missing=1
  fi
}

require_device_page_section() {
  page=$1
  section=$2
  path="${device_docs_dir}/${page}"
  if ! rg -n "^## ${section}$" "$path" >/dev/null; then
    printf 'missing reverse-evidence device page section: %s -> %s\n' "$page" "$section" >&2
    missing=1
  fi
}

require_limited_device_page_shape() {
  page=$1
  require_device_page_section "$page" "Status And Provenance"
  require_device_page_section "$page" "Logical Devices"
  require_device_page_section "$page" "Resources"
  require_device_page_section "$page" "Capabilities"
  require_device_page_section "$page" "Properties"
  require_device_page_section "$page" "Evidence Gate"
  require_device_page_section "$page" "Examples"
  require_device_page_section "$page" "Remaining Work"
  require_device_page_section "$page" "Unblock Trace Checklist"
}

require_reverse_note_section() {
  page=$1
  section=$2
  path="${reverse_docs_dir}/${page}"
  if ! rg -n "^## ${section}$" "$path" >/dev/null; then
    printf 'missing reverse-evidence reverse note section: %s -> %s\n' "$page" "$section" >&2
    missing=1
  fi
}

require_reverse_note_shape() {
  page=$1
  require_reverse_note_section "$page" "Status"
  require_reverse_note_section "$page" "Protocol Evidence Summary"
  require_reverse_note_section "$page" "Evidence To Collect"
  require_reverse_note_section "$page" "Protocol Questions"
  require_reverse_note_section "$page" "Candidate Public Surface"
  require_reverse_note_section "$page" "Stop/Proceed Decision"
  require_reverse_note_section "$page" "Implementation Gate"
}

require_artifact_summary_row() {
  target=$1
  artifact_pattern=$2
  if ! rg -F "| ${target} |" "$artifact_summary_file" | rg -F "$artifact_pattern" >/dev/null; then
    printf 'missing reverse-evidence artifact summary row: %s -> %s\n' "$target" "$artifact_pattern" >&2
    missing=1
  fi
}

reject_driver_test_path() {
  module=$1
  if rg --files "$audit_root" | rg "(^|/)${module}(_|-).*test|(^|/)test.*(${module})|(^|/)tests?/.*(${module})" >/dev/null; then
    printf 'reverse-evidence driver test artifact exists: %s\n' "$module" >&2
    missing=1
  fi
}

reject_inline_tests() {
  module=$1
  path="${driver_src_dir}/${module}.rs"
  if [ -f "$path" ] && rg -n '#\[cfg\(test\)\]|mod[[:space:]]+tests' "$path" >/dev/null; then
    printf 'reverse-evidence driver has inline generated-style tests: %s\n' "$path" >&2
    missing=1
  fi
}

reject_driver_crate_tests() {
  if rg -n '#\[cfg\(test\)\]|mod[[:space:]]+tests' "$driver_src_dir" >/dev/null; then
    printf 'hardware-driver crate contains inline tests; record evidence in docs or hardware-validation notes instead\n' >&2
    missing=1
  fi
  if rg --files "$(dirname "$driver_src_dir")" | rg '(^|/)tests?/|(_test|_tests)\.rs$' >/dev/null; then
    printf 'hardware-driver crate contains generated-style test files; record evidence in docs or hardware-validation notes instead\n' >&2
    missing=1
  fi
}

reject_example_protocol_internals() {
  examples_dir="${audit_root}/crates/numanager-examples/src"
  if rg -n '::protocol|protocol::|ScriptedSerial|SerialIo|RawRegisterAccess' "$examples_dir" >/dev/null; then
    printf 'examples expose protocol internals or scripted transport fixtures\n' >&2
    missing=1
  fi
  if rg -n 'GenericCommand|GenericCommandRequest' "$examples_dir" | rg -v 'light_source\.rs' >/dev/null; then
    printf 'examples use GenericCommand outside the opt-in Mightex hardware bring-up path\n' >&2
    missing=1
  fi
  if rg -n 'GenericCommand|GenericCommandRequest' "${examples_dir}/light_source.rs" >/dev/null &&
    ! rg -F 'NUMANAGER_MIGHTEX_OUTPUT' "${examples_dir}/light_source.rs" >/dev/null; then
    printf 'light_source GenericCommand use is not gated by NUMANAGER_MIGHTEX_OUTPUT\n' >&2
    missing=1
  fi
}

reject_public_low_level_driver_protocol_exports() {
  local hits
  hits=$(
    rg -n '^pub (mod protocol|enum .*Command|struct .*Command|struct .*Frame|enum .*Frame|struct .*Packet|enum .*Packet|struct .*Session|fn encode|fn decode|fn parse_|fn .*_payload)' "$driver_src_dir" \
      | rg -v 'sim\.rs:[0-9]+:pub struct SceneFrame' \
      || true
  )
  if [ -n "$hits" ]; then
    printf 'hardware driver exposes low-level protocol helpers as public API:\n%s\n' "$hits" >&2
    missing=1
  fi
}

reject_noncanonical_example_enums() {
  if rg -n 'format=Some\("(MONO|RGB|BGR|RAW)[0-9]+"\)|format=(MONO|RGB|BGR|RAW)[0-9]+' "$example_outputs_file" >/dev/null; then
    printf 'example outputs expose noncanonical uppercase pixel-format labels\n' >&2
    missing=1
  fi
}

reject_config_example_raw_graph_ids() {
  config_example="${audit_root}/crates/numanager-examples/src/config_roundtrip.rs"
  if rg -n 'NodeId|DeviceId[[:space:]]*\(|ResourceId[[:space:]]*\(' "$config_example" >/dev/null; then
    printf 'config_roundtrip example constructs raw graph IDs; use HardwareConfig builder handles instead\n' >&2
    missing=1
  fi
  discovery_example="$driver_lib"
  if rg -n 'DeviceId[[:space:]]*\([[:space:]]*NodeId' "$discovery_example" >/dev/null; then
    printf 'discover_devices example constructs raw graph IDs; use DeviceConfig constructors or HardwareConfig builder handles instead\n' >&2
    missing=1
  fi
}

reject_gui_raw_capability_dispatch() {
  gui_example="${audit_root}/crates/numanager-examples/src/software_gui.rs"
  if rg -n 'capability[[:space:]]*==[[:space:]]*CapabilityId|CapabilityId[[:space:]]*\([12]\)' "$gui_example" >/dev/null; then
    printf 'software_gui dispatches on raw capability IDs; dispatch on typed capability requests instead\n' >&2
    missing=1
  fi
}

reject_discover_devices_duplicate_driver_ids() {
  discovery_example="$driver_lib"
  local duplicates
  duplicates=$(
    rg -o 'DriverId\([0-9]+\)' "$discovery_example" \
      | sort \
      | uniq -d \
      || true
  )
  if [ -n "$duplicates" ]; then
    printf 'discover_devices example reuses DriverId literals:\n%s\n' "$duplicates" >&2
    missing=1
  fi
}

reject_public_maintenance_generic_commands() {
  if rg -n 'GenericCommand supports[^"\n]*(reset|clear[-_]?fault|fault[-_]?clear|clear[-_]?alarm|alarm[-_]?clear|clear[-_]?errors?|errors?[-_]?clear|firmware|upload|download[-_]?firmware|program[-_]?firmware|firmware[-_]?program|write[-_]?firmware|burn[-_]?firmware|loader|boot|flash|dfu|factory|restore|store|save[-_](settings|configuration)|persist|user[-_]?set|default|zero_position|set_origin|erase|eeprom|nvram|fpga|load[-_]?fpga|program[-_]?fpga|bitstream|reboot|restart|power[-_]?cycle|cycle[-_]?power|reinit|re[-_]?enumerate)' "$driver_src_dir" >/dev/null; then
    printf 'GenericCommand support text advertises hidden maintenance operations\n' >&2
    missing=1
  fi
  if rg -n 'GenericCommand supports[^"\n]*(loader|firmware package|firmware init|firmware initialization)' "$driver_src_dir" >/dev/null; then
    printf 'GenericCommand support text advertises hidden firmware initialization operations\n' >&2
    missing=1
  fi
  if rg -n '"(reset|device_reset|reset_device|clear_fault|fault_clear|acknowledge_fault|fault_acknowledge|clear_alarm|alarm_clear|clear_error|error_clear|clear_errors|errors_clear|upload_firmware|firmware_upload|load_firmware|firmware_load|init_firmware|firmware_init|update_firmware|firmware_update|upgrade_firmware|firmware_upgrade|download_firmware|firmware_download|program_firmware|firmware_program|program_firmware_image|firmware_image_program|write_firmware|firmware_write|burn_firmware|firmware_burn|boot|bootloader|flash|dfu|factory_reset|restore_defaults|restoredef|store|save|save_settings|save_configuration|persist_configuration|user_set_save|user_set_load|zero_position|set_origin|erase|write_eeprom|eeprom_write|write_flash|flash_write|fpga|load_fpga|fpga_load|program_fpga|fpga_program|load_bitstream|bitstream_load|write_bitstream|bitstream_write|bitstream|nvram|reboot|restart|power_cycle|cycle_power|reinit|reinitialize|reinitialise|reenumerate|re_enumerate)"[[:space:]]*=>[[:space:]]*(Ok|self)' "$driver_src_dir" >/dev/null; then
    printf 'GenericCommand-style match arm appears to accept a maintenance operation\n' >&2
    missing=1
  fi
  if rg -n '"[^"]*(reset|clear[_-]?fault|fault[_-]?clear|clear[_-]?alarm|alarm[_-]?clear|clear[_-]?errors?|errors?[_-]?clear|upload|download[_-]?firmware|program[_-]?firmware|firmware[_-]?program|write[_-]?firmware|burn[_-]?firmware|loader|boot|flash|dfu|factory|restore|user[_-]?set|save[_-](settings|configuration)|persist|zero[_-]?position|set[_-]?origin|erase|eeprom|nvram|fpga|load[_-]?fpga|program[_-]?fpga|bitstream|reboot|restart|power[_-]?cycle|cycle[_-]?power|reinit|re[-_]?enumerate)[^"]*"[[:space:]]*=>[[:space:]]*(Ok|self)' "$driver_src_dir" >/dev/null; then
    printf 'GenericCommand-style match arm appears to accept a maintenance-looking operation\n' >&2
    missing=1
  fi
  if rg -n 'CapabilityKind::Custom\("[^"]*(reset|clear[_-]?fault|fault[_-]?clear|clear[_-]?alarm|alarm[_-]?clear|clear[_-]?errors?|errors?[_-]?clear|upload|loader|boot|flash|dfu|factory|restore|store|save[_-](settings|configuration)|persist|user[_-]?set|default|zero[_-]?position|set[_-]?origin|erase|eeprom|nvram|fpga|bitstream|reboot|restart|power[_-]?cycle|cycle[_-]?power|reinit|re[-_]?enumerate)[^"]*"\)' "$driver_src_dir" >/dev/null; then
    printf 'custom capability name advertises a hidden maintenance-looking operation\n' >&2
    missing=1
  fi
}

reject_public_maintenance_command_docs() {
  local hits
  hits=$(
    rg -n '\|[^\n`]*(GenericCommand|RawRegisterAccess|Command)[^\n`]*`[^`]*(reset|clear_fault|fault_clear|clear_alarm|alarm_clear|clear_error|error_clear|clear_errors|errors_clear|firmware|upload|upload_firmware|firmware_upload|load_firmware|firmware_load|init_firmware|firmware_init|update_firmware|firmware_update|upgrade_firmware|firmware_upgrade|write_firmware|bootloader|flash|dfu|factory_reset|restore_defaults|restoredef|store|save_settings|save_configuration|persist_configuration|user_set_save|user_set_load|zero_position|set_origin|erase|write_eeprom|write_flash|load_fpga|bitstream|nvram|reboot|restart|power_cycle|cycle_power|reinit|re_enumerate)[^`]*`' "$device_docs_dir" "$readme_file" \
      | rg -v 'hidden|not advertised|no public aliases|remain hidden|remains hidden|are hidden|is hidden' \
      || true
  )
  if [ -n "$hits" ]; then
    printf 'public docs appear to advertise maintenance-looking commands:\n%s\n' "$hits" >&2
    missing=1
  fi
  hits=$(
    rg -n '\|[^|\n]*(GenericCommand|RawRegisterAccess|Command)[^|\n]*\|[^|\n]*(supports|exposes|accepts|allows)[^|\n]*hidden maintenance operations' "$device_docs_dir" "$readme_file" \
      || true
  )
  if [ -n "$hits" ]; then
    printf 'public docs blur command support with hidden maintenance operations:\n%s\n' "$hits" >&2
    missing=1
  fi
}

reject_hardware_validation_as_implementation_gate() {
  local hits
  hits=$(
    rg -n 'unsupported until hardware validation|blocked until hardware validation|unavailable until hardware validation|requires hardware validation before (implementing|implementation)|configured/simulated validation only|validation only; hardware validation|configured-only until hardware completion is validated|configured only until hardware completion is validated' "$driver_src_dir" "$device_docs_dir" docs/inventory "$evidence_file" "$readme_file" \
      || true
  )
  if [ -n "$hits" ]; then
    printf 'hardware validation is a validation column, not an implementation gate:\n%s\n' "$hits" >&2
    missing=1
  fi
}

reject_stale_public_control_gate_wording() {
  local hits
  hits=$(
      rg -n 'cannot be implemented yet|needs? .*evidence before becoming (a )?public control|needs? .*evidence before (they )?become public controls|needs explicit .*evidence|needs real .*trace validation|requires traces defining|outside recorded evidence|still incomplete|incomplete for|bounded|configured fallback only|configured fallback|configured fixture|fixture fallback|fixture behavior|fallback state|fallback value|fallback values|fallback resource|fallback transport|later meta-device|Open Questions Before Driver Work|add .* only (when|from|if|after|with)|Add .* only (when|from|if|after|with)|trace-pending|trace_pending|line reply pending|integration still pending|identity handling pending|readback pending|remain(s)? pending|await implementation evidence|await protocol evidence|waits for protocol evidence|waits for SDK family classification|waits for backend-specific implementation|not advertised without|not advertised because|not exposed as hardware timing support because|no supported public surface|defer unless frame transfer is externally evidenced|Real .* probing still needs|Real support still needs|probing still needs official' "$driver_src_dir" "$device_docs_dir" "$reverse_docs_dir" docs/planning "$readme_file" \
      || true
  )
  if [ -n "$hits" ]; then
    printf 'stale implementation-boundary wording found; state implemented support or absent evidence directly:\n%s\n' "$hits" >&2
    missing=1
  fi
}

reject_stale_inventory_implementation_notes() {
  local hits
  hits=$(
    rg -n 'Need extract the remaining digital output|Complete command extraction should cover|Start with discovery/version and axis-range parsing|Need extract intensity/current command syntax|planned SDK2 USB/readout support|live hardware support remains trace-gated|low priority until hardware family is identified|low for true SDK-free|low; SDK/API-first|Defer remaining proprietary-runtime-only scientific or controller SDKs|Add the Andor SDK2 userspace USB/readout track|Add the PVCAM evidence/discovery surface|support supports|remains unsupported until frame-transfer evidence exists|initial configured direct-mode driver exists|defer from adapter, but public ASCII protocol may be a separate clean-room target' docs/inventory \
      || true
  )
  if [ -n "$hits" ]; then
    printf 'inventory contains stale implementation-extraction notes for implemented drivers:\n%s\n' "$hits" >&2
    missing=1
  fi
}

reject_stale_planning_maintenance_notes() {
  local hits
  hits=$(
    rg -n 'sutter_mp285.*StageHome|reset ACK|move/origin/stop|after move/origin/stop|still advertises no motion|config-backed fallback|is not exposed because evidence is absent' docs/planning/device_implementation_plan.md \
      || true
  )
  if [ -n "$hits" ]; then
    printf 'planning docs contain stale implementation-boundary wording:\n%s\n' "$hits" >&2
    missing=1
  fi
}

reject_stale_device_support_notes() {
  local hits
  hits=$(
    rg -n 'Range-measure readback is not exposed because range reply evidence is absent' "$device_docs_dir" \
      || true
  )
  if [ -n "$hits" ]; then
    printf 'device docs contain stale support wording for implemented raw readback helpers:\n%s\n' "$hits" >&2
    missing=1
  fi
}

reject_stale_evidence_surface_labels() {
  local hits
  hits=$(
    rg -n 'evidence surface|evidence-surface|evidence\.surface|Configured .* evidence surface|Configured (ABS|Mightex) camera support' "$device_docs_dir" "$reverse_docs_dir" docs/planning docs/inventory "$example_outputs_file" "$driver_src_dir" \
      || true
  )
  if [ -n "$hits" ]; then
    printf 'stale evidence-surface label found; use reverse engineered support wording instead:\n%s\n' "$hits" >&2
    missing=1
  fi
}

reject_driver_placeholder_markers() {
  local hits
  hits=$(
    rg -n 'todo!|unimplemented!|TODO|FIXME' "$driver_src_dir" "$audit_root/crates/numanager-core/src" "$device_docs_dir" docs/planning \
      || true
  )
  if [ -n "$hits" ]; then
    printf 'driver/core/device documentation contains unresolved placeholder markers:\n%s\n' "$hits" >&2
    missing=1
  fi
}

reject_driver_source_placeholder_language() {
  local hits
  hits=$(
    rg -n 'stub|placeholder|dummy|temporary|not implemented|not yet implemented' "$driver_src_dir" "$audit_root/crates/numanager-core/src" \
      || true
  )
  if [ -n "$hits" ]; then
    printf 'driver/core source contains placeholder implementation language:\n%s\n' "$hits" >&2
    missing=1
  fi
}

require_core_maintenance_generic_command_gate() {
  core_lib="${audit_root}/crates/numanager-core/src/lib.rs"
  runtime_src="${audit_root}/crates/numanager-core/src/runtime.rs"
  require_text "${device_docs_dir}/README.md" "Firmware upload, bootloader entry, reset, factory/default restore, flash/DFU" "device docs hidden firmware/reset command policy"
  require_text "$protocol_evidence_plan_file" "Firmware upload, bootloader entry, reset, factory/default restore, flash/DFU" "protocol evidence hidden firmware/reset command policy"
  require_text "$protocol_evidence_plan_file" "advanced UI commands" "protocol evidence advanced UI hidden-maintenance policy"
  require_text "${audit_root}/docs/core_model.md" "CapabilityDescriptor::exposure()" "core model hidden-maintenance exposure policy"
  require_text "${audit_root}/docs/core_model.md" "HiddenMaintenance" "core model hidden-maintenance marker policy"
  require_text "${audit_root}/docs/core_model.md" "driver-validated aliases only, not as" "core model diagnostic GenericCommand advanced-UI policy"
  require_text "$core_lib" "pub enum CapabilityExposure {" "core capability exposure classifier"
  require_text "$core_lib" "AdvancedDiagnostic" "core advanced diagnostic exposure"
  require_text "$core_lib" "HiddenMaintenance" "core hidden maintenance exposure"
  require_text "$core_lib" "pub fn exposure(&self) -> CapabilityExposure {" "core capability descriptor exposure method"
  require_text "$core_lib" "pub fn generic_command_request_is_hidden_maintenance(request: &GenericCommandRequest) -> bool {" "core hidden maintenance GenericCommand request classifier"
  require_text "$core_lib" "pub fn is_hidden_maintenance(&self) -> bool {" "core hidden maintenance GenericCommand request method"
  require_text "$core_lib" "pub fn generic_command_is_hidden_maintenance(command: &str) -> bool {" "core hidden maintenance GenericCommand classifier"
  require_text "$core_lib" "pub fn requires_driver_validated_command_aliases(&self) -> bool {" "core diagnostic command alias descriptor marker"
  require_text "$core_lib" "CapabilityKind::GenericCommand | CapabilityKind::RawRegisterAccess" "core diagnostic command alias GenericCommand/RawRegisterAccess marker"
  require_text "$core_lib" "is_generic_command_target_param(key)" "core hidden maintenance GenericCommand target parameter classifier"
  require_text "$core_lib" "pub fn generic_command_value_is_hidden_maintenance(value: &Value) -> bool {" "core hidden maintenance generic value classifier"
  require_text "$core_lib" '"firmware",' "core hidden maintenance firmware token"
  require_text "$core_lib" '"clearfault",' "core hidden maintenance clear-fault token"
  require_text "$core_lib" '"fw",' "core hidden maintenance firmware shorthand token"
  require_text "$core_lib" '"upload",' "core hidden maintenance upload token"
  require_text "$core_lib" '"loader",' "core hidden maintenance loader token"
  require_text "$core_lib" '"loadfirmware",' "core hidden maintenance firmware-load token"
  require_text "$core_lib" '"firmwareload",' "core hidden maintenance firmware-load inverse token"
  require_text "$core_lib" '"updatefirmware",' "core hidden maintenance firmware-update token"
  require_text "$core_lib" '"firmwareupdate",' "core hidden maintenance firmware-update inverse token"
  require_text "$core_lib" '"upgradefirmware",' "core hidden maintenance firmware-upgrade token"
  require_text "$core_lib" '"firmwareupgrade",' "core hidden maintenance firmware-upgrade inverse token"
  require_text "$core_lib" '"downloadfirmware",' "core hidden maintenance firmware-download token"
  require_text "$core_lib" '"firmwaredownload",' "core hidden maintenance firmware-download inverse token"
  require_text "$core_lib" '"firmwareinit",' "core hidden maintenance firmware-init token"
  require_text "$core_lib" '"firmwareprogram",' "core hidden maintenance firmware-program token"
  require_text "$core_lib" '"programfirmware",' "core hidden maintenance program-firmware token"
  require_text "$core_lib" '"firmwareprogrammer",' "core hidden maintenance firmware-programmer token"
  require_text "$core_lib" '"programfirmwareimage",' "core hidden maintenance program-firmware-image token"
  require_text "$core_lib" '"firmwareimageprogram",' "core hidden maintenance firmware-image-program token"
  require_text "$core_lib" '"firmwareimagewrite",' "core hidden maintenance firmware-image-write token"
  require_text "$core_lib" '"writefirmware",' "core hidden maintenance firmware-write token"
  require_text "$core_lib" '"firmwarewrite",' "core hidden maintenance firmware-write inverse token"
  require_text "$core_lib" '"burnfirmware",' "core hidden maintenance firmware-burn token"
  require_text "$core_lib" '"firmwareburn",' "core hidden maintenance firmware-burn inverse token"
  require_text "$core_lib" '"fpga",' "core hidden maintenance FPGA token"
  require_text "$core_lib" '"bitstream",' "core hidden maintenance bitstream token"
  require_text "$core_lib" '"loadbitstream",' "core hidden maintenance bitstream-load token"
  require_text "$core_lib" '"bitstreamload",' "core hidden maintenance bitstream-load inverse token"
  require_text "$core_lib" '"writebitstream",' "core hidden maintenance bitstream-write token"
  require_text "$core_lib" '"bitstreamwrite",' "core hidden maintenance bitstream-write inverse token"
  require_text "$core_lib" '"flashprogram",' "core hidden maintenance flash-program token"
  require_text "$core_lib" '"programflash",' "core hidden maintenance program-flash token"
  require_text "$core_lib" '"writeflash",' "core hidden maintenance flash-write token"
  require_text "$core_lib" '"flashwrite",' "core hidden maintenance flash-write inverse token"
  require_text "$core_lib" '"writeeeprom",' "core hidden maintenance EEPROM-write token"
  require_text "$core_lib" '"eepromwrite",' "core hidden maintenance EEPROM-write inverse token"
  require_text "$core_lib" '"eepromprogram",' "core hidden maintenance EEPROM-program token"
  require_text "$core_lib" '"programeeprom",' "core hidden maintenance program-EEPROM token"
  require_text "$core_lib" '"nonvolatile",' "core hidden maintenance nonvolatile-memory token"
  require_text "$core_lib" '"persistent",' "core hidden maintenance persistent token"
  require_text "$core_lib" '"userset",' "core hidden maintenance persistent user-set token"
  require_text "$core_lib" '"save",' "core hidden maintenance exact save token"
  require_text "$core_lib" '"commit",' "core hidden maintenance exact commit token"
  require_text "$core_lib" '"persist",' "core hidden maintenance exact persist token"
  require_text "$core_lib" '"savesettings",' "core hidden maintenance settings-save token"
  require_text "$core_lib" '"saveconfiguration",' "core hidden maintenance configuration-save token"
  require_text "$core_lib" '"nvram",' "core hidden maintenance nonvolatile-memory token"
  require_text "$core_lib" '"factoryrestore",' "core hidden maintenance factory-restore token"
  require_text "$core_lib" '"eeprom",' "core hidden maintenance EEPROM token"
  require_text "$core_lib" '"calibration",' "core hidden maintenance calibration token"
  require_text "$core_lib" '"maintenance",' "core hidden maintenance maintenance token"
  require_text "$core_lib" '"service",' "core hidden maintenance service token"
  require_text "$core_lib" '"restart",' "core hidden maintenance restart token"
  require_text "$core_lib" '"powercycle",' "core hidden maintenance power-cycle token"
  require_text "$core_lib" '"reenumerate",' "core hidden maintenance re-enumerate token"
  require_text "$core_lib" '"origin",' "core hidden maintenance exact origin token"
  require_text "$core_lib" '"zero",' "core hidden maintenance exact zero token"
  require_text "$core_lib" '"programmer",' "core hidden maintenance exact programming token"
  require_text "$core_lib" '"default",' "core hidden maintenance default token"
  require_text "$core_lib" "pub fn is_hidden_maintenance(&self) -> bool {" "core hidden maintenance capability descriptor classifier"
  require_text "$runtime_src" ".filter(|capability| !capability.is_hidden_maintenance())" "runtime hidden maintenance capability descriptor filter"
  require_text "$core_lib" ".filter(|capability| !capability.is_hidden_maintenance())" "core capability-provider hidden maintenance filter"
  require_text "$runtime_src" "reject_hidden_maintenance_request(descriptor, request)?;" "runtime hidden maintenance descriptor-aware gate"
  require_text "$runtime_src" "request.is_hidden_maintenance()" "runtime hidden maintenance GenericCommand request gate"
  require_text "$runtime_src" "CapabilityRequest::Custom(value) if generic_command_value_is_hidden_maintenance(value)" "runtime hidden maintenance custom request gate"
  require_text "$runtime_src" "is a hidden maintenance capability" "runtime hidden maintenance capability error"
  require_text "$runtime_src" "GenericCommand {} is a hidden maintenance operation" "runtime hidden maintenance GenericCommand error"
  require_text "$runtime_src" "custom request contains a hidden maintenance operation" "runtime hidden maintenance custom request error"
}

reject_hidden_maintenance_typed_capabilities() {
  mp285_driver="${driver_src_dir}/sutter_mp285.rs"
  if rg -n 'CapabilityKind::StageHome|Mp285Command::SetOrigin.*\?' "$mp285_driver" >/dev/null; then
    printf 'MP-285 current-position-as-origin is a hidden maintenance operation, not StageHome\n' >&2
    missing=1
  fi
}

reject_hidden_maintenance_protocol_sends() {
  local hits
  hits=$(
    rg -n 'send\([^;\n]*(Reset|SetOrigin|SetXyOrigin|SetZOrigin|ZeroPosition|FirmwareUpload|UploadFirmware|LoadFirmware|FirmwareLoad|Bootloader|Flash|Dfu|FactoryRestore|RestoreDefault|Erase)' "$driver_src_dir" \
      || true
  )
  if [ -n "$hits" ]; then
    printf 'driver dispatch sends maintenance protocol primitive through a public path:\n%s\n' "$hits" >&2
    missing=1
  fi
}

reject_hidden_maintenance_variant_call_sites() {
  local hits
  hits=$(
    rg -n '(OpenStageCommand::ZeroPosition|QtCommand::SetOrigin|Mp285Command::Reset|Mp285Command::SetOrigin|SutterCommand::ResetModules|VellemanCommand::Reset|VellemanCommand::ResetK8055Counter|VellemanCommand::ResetK8061Counter|MarzhauserCommand::SetOriginXy|MarzhauserCommand::SetOriginZ|OmicronCommand::Reset|SquidCommandCode::Reset)' "$driver_src_dir" \
      | rg -v '=>|enum|[[:space:]]+(Reset|SetOrigin|SetOriginXy|SetOriginZ|ZeroPosition|ResetModules|ResetK8055Counter|ResetK8061Counter)[,}]|255 => Self::Reset' \
      || true
  )
  if [ -n "$hits" ]; then
    printf 'hidden reset/origin protocol primitive is referenced from an executable call site:\n%s\n' "$hits" >&2
    missing=1
  fi
}

require_driver_local_hidden_maintenance_guards() {
  local validators
  local guards
  local generic_files
  local missing_file_guards
  validators=$(rg -n 'fn validate_generic_command' "$driver_src_dir" | wc -l | tr -d ' ')
  guards=$(rg -n 'request\.is_hidden_maintenance\(\)' "$driver_src_dir" | wc -l | tr -d ' ')
  if [ "$guards" -lt "$validators" ]; then
    printf 'driver-local GenericCommand validators must reject hidden maintenance requests before allowlist dispatch: %s validators, %s guards\n' "$validators" "$guards" >&2
    missing=1
  fi
  generic_files=$(rg -l 'GenericCommandRequest|CapabilityRequest::GenericCommand' "$driver_src_dir" || true)
  missing_file_guards=$(
    for path in $generic_files; do
      if ! rg -n 'is_hidden_maintenance\(\)' "$path" >/dev/null; then
        printf '%s\n' "$path"
      fi
    done
  )
  if [ -n "$missing_file_guards" ]; then
    printf 'driver source handles GenericCommandRequest without a hidden-maintenance gate:\n%s\n' "$missing_file_guards" >&2
    missing=1
  fi
  require_text "${driver_src_dir}/mcl.rs" "MCL GenericCommand {} is a hidden maintenance operation" "MCL hidden-maintenance GenericCommand error"
}

reject_unreviewed_maintenance_command_literals() {
  local hits
  hits=$(
    rg -n '"(reset|device_reset|reset_device|upload|loader|firmware_upload|upload_firmware|firmware_load|load_firmware|firmware_init|init_firmware|firmware_update|update_firmware|firmware_upgrade|upgrade_firmware|firmware_download|download_firmware|firmware_program|program_firmware|firmware_write|write_firmware|firmware_burn|burn_firmware|boot|bootloader|flash|dfu|factory|factory_reset|factory_restore|restore_factory|restore|restore_defaults|restoredef|store|save|commit|persist|default|zero|zero_position|set_origin|origin|erase|write_flash|flash_write|write_eeprom|eeprom_write|fpga|load_fpga|fpga_load|program_fpga|fpga_program|load_bitstream|bitstream_load|write_bitstream|bitstream_write|bitstream|reboot|restart|power_cycle|cycle_power|reinit|reinitialize|reinitialise|reenumerate|re_enumerate)"' "$driver_src_dir" \
      | rg -v 'genicam\.rs:[0-9]+:[[:space:]]+"(reset|bootloader|flash|dfu|factory|default|restore|store|boot)",' \
      | rg -v 'mightex_bls\.rs:[0-9]+:[[:space:]]+"reset"[[:space:]]+\|[[:space:]]+"store"[[:space:]]+\|[[:space:]]+"restoredef"' \
      || true
  )
  if [ -n "$hits" ]; then
    printf 'unreviewed maintenance-looking command literal in driver source:\n%s\n' "$hits" >&2
    missing=1
  fi
}

reject_empty_driver_transaction_lists() {
  local hits
  hits=$(rg -n '=> Ok\(vec!\[\]\)|Ok\(vec!\[\]\)' "$driver_src_dir" || true)
  if [ -n "$hits" ]; then
    printf 'driver command preparation must not expose empty transaction-list placeholders:\n%s\n' "$hits" >&2
    missing=1
  fi
}

reject_invoke_noop_catchalls() {
  local hits
  hits=$(
    rg -n 'Command::Invoke \{ \.\. \} => \{\}|\| Command::Invoke \{ \.\. \}' "$driver_src_dir" \
      || true
  )
  if [ -n "$hits" ]; then
    printf 'driver invoke catch-all silently ignores unsupported command invocations:\n%s\n' "$hits" >&2
    missing=1
  fi
}

reject_writable_firmware_package_controls() {
  local hits
  hits=$(
    {
      rg -n 'writable_[a-z_]*property\("[^"]*(firmware|runtime|package|blob|loader|bootloader|flash|dfu|reset|factory|restore)[^"]*"' "$driver_src_dir"
      rg -n 'property\("[^"]*(firmware|runtime|package|blob|loader|bootloader|flash|dfu|reset|factory|restore)[^"]*".*true\)' "$driver_src_dir"
    } \
      || true
  )
  if [ -n "$hits" ]; then
    printf 'firmware/runtime package or reset-related controls must not be writable runtime properties:\n%s\n' "$hits" >&2
    missing=1
  fi
}

reject_camera_byte_config_scalars() {
  discovery_example="$driver_lib"
  if rg -n '"(packet_size|transfer_size)"\.into\(\),[[:space:]]*Value::I64' "$discovery_example" >/dev/null; then
    printf 'camera byte-sized configured properties use scalar I64; use Value::ByteCount instead\n' >&2
    missing=1
  fi
}

reject_toupcam_sensor_pixel_scalars() {
  toupcam_driver="${audit_root}/crates/numanager-drivers/src/toupcam.rs"
  if rg -n '"sensor_(width|height)"\.into\(\),[[:space:]]*Value::I64' "$toupcam_driver" >/dev/null; then
    printf 'Toupcam sensor dimensions are pixel counts; use Value::PixelCount in public identity maps\n' >&2
    missing=1
  fi
}

require_toupcam_stream_geometry_completion() {
  toupcam_driver="${audit_root}/crates/numanager-drivers/src/toupcam.rs"
  if ! rg -F 'Value::PixelCount(PixelCount::new(encoded.width))' "$toupcam_driver" >/dev/null; then
    printf 'Toupcam stream completion must include typed PixelCount width\n' >&2
    missing=1
  fi
  if ! rg -F 'Value::PixelCount(PixelCount::new(encoded.height))' "$toupcam_driver" >/dev/null; then
    printf 'Toupcam stream completion must include typed PixelCount height\n' >&2
    missing=1
  fi
}

require_toupcam_bayer_gate() {
  toupcam_driver="${audit_root}/crates/numanager-drivers/src/toupcam.rs"
  toupcam_doc="${audit_root}/docs/devices/toupcam.md"
  if ! rg -F 'MAX_EXPOSURE_LINES' "$toupcam_driver" >/dev/null; then
    printf 'Toupcam exposure range must be tied to the evidenced 16-bit line-count field\n' >&2
    return 1
  fi
  if ! rg -F '37.983 us..2.489215905 s' "$toupcam_doc" >/dev/null; then
    printf 'Toupcam docs must advertise the evidenced line-time exposure range\n' >&2
    return 1
  fi
  if ! rg -F '"bayer_phase"' "$toupcam_driver" >/dev/null; then
    printf 'Toupcam RGB/BGR conversion must expose a bayer_phase property\n' >&2
    return 1
  fi
  if ! rg -F 'Toupcam RGB/BGR output requires configured bayer_phase' "$toupcam_driver" >/dev/null; then
    printf 'Toupcam live RGB/BGR conversion must be gated by bayer_phase\n' >&2
    return 1
  fi
  if ! rg -F '`Native`, `Raw8`, `Mono8`, `Rgb8`, `Bgr8`' "$toupcam_doc" >/dev/null; then
    printf 'Toupcam docs must advertise only the implemented 8-bit/conversion formats\n' >&2
    return 1
  fi
  if ! rg -F 'pub(super) identity: ToupcamUsbIdentity' "$toupcam_driver" >/dev/null; then
    printf 'Toupcam live discovery must retain USB identity metadata\n' >&2
    return 1
  fi
  if ! rg -F 'pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self>' "$toupcam_driver" >/dev/null; then
    printf 'Toupcam discovery must include config-backed geometry/identity support\n' >&2
    return 1
  fi
  if ! rg -F 'fn expected_raw_frame_bytes(&self) -> usize' "$toupcam_driver" >/dev/null; then
    printf 'Toupcam live frame sizing must use configured sensor geometry\n' >&2
    return 1
  fi
  if ! rg -F 'live.read_frame(self.expected_raw_frame_bytes())' "$toupcam_driver" >/dev/null; then
    printf 'Toupcam live frame reads must request configured frame byte counts\n' >&2
    return 1
  fi
  if ! rg -F 'Toupcam raw register writes are hidden without a named safe control surface' "$toupcam_driver" >/dev/null; then
    printf 'Toupcam raw numeric writes must be hidden from RawRegisterAccess\n' >&2
    return 1
  fi
  if ! rg -F 'unsupported Toupcam capability invocation' "$toupcam_driver" >/dev/null; then
    printf 'Toupcam dispatch must fail closed for unsupported capability invocation\n' >&2
    return 1
  fi
  if ! rg -F 'raw numeric writes are hidden without a named safe control surface' "$toupcam_doc" >/dev/null; then
    printf 'Toupcam docs must describe hidden raw numeric writes\n' >&2
    return 1
  fi
  if ! rg -F 'arbitrary register-read semantics are not evidenced' "$toupcam_doc" >/dev/null; then
    printf 'Toupcam docs must keep raw reads mapped to cached metadata\n' >&2
    return 1
  fi
  if ! rg -F 'live USB descriptors retain product, serial when available, VID/PID, bus, and address metadata' "$evidence_file" >/dev/null; then
    printf 'Toupcam evidence must record retained live USB identity metadata\n' >&2
    return 1
  fi
  if ! rg -F 'typed exposure/gain properties use evidenced live register writes, raw numeric register writes are hidden without a named safe control surface' "$evidence_file" >/dev/null; then
    printf 'Toupcam evidence must record hidden raw numeric writes\n' >&2
    return 1
  fi
  if ! rg -F 'Config-backed geometry/identity plus live `os-usb` backend' "$evidence_file" >/dev/null; then
    printf 'Toupcam evidence must record config-backed geometry/identity support\n' >&2
    return 1
  fi
  if ! rg -F 'ToupcamDiscovery::from_config' "$driver_lib" >/dev/null; then
    printf 'numanager-drivers must register Toupcam config-backed discovery\n' >&2
    return 1
  fi
}

reject_unexpected_export() {
  module=$1
  if rg -n "^(pub[[:space:]]+)?mod[[:space:]]+${module};" "$driver_lib" >/dev/null; then
    printf 'unexpected reverse-evidence driver export: %s\n' "$module" >&2
    missing=1
  fi
}

reject_unexpected_driver_file() {
  module=$1
  path="${driver_src_dir}/${module}.rs"
  if [ -f "$path" ]; then
    printf 'unexpected reverse-evidence driver source exists: %s\n' "$path" >&2
    missing=1
  fi
}

require_export() {
  module=$1
  if ! rg -n "^pub[[:space:]]+mod[[:space:]]+${module};" "$driver_lib" >/dev/null; then
    printf 'expected reverse-evidence driver export is missing: %s\n' "$module" >&2
    missing=1
  fi
}

require_file "$protocol_evidence_plan_file"
require_file "$artifact_summary_file"
require_file "$reverse_index_file"
require_file "$evidence_gate_audit_file"
require_file "${reverse_docs_dir}/trace-capture-guide.md"
require_file "${reverse_docs_dir}/trace-note-template.md"
require_file "${reverse_docs_dir}/okolab-protocol.md"
require_file "$driver_lib"
require_file "$evidence_file"
require_file "$device_index_file"
require_file "$readme_file"
require_file "$example_outputs_file"
require_file "$workspace_file"

require_text "$workspace_file" "\"crates/numanager-drivers\"" "single consolidated driver crate workspace member"
if rg -n 'crates/numanager-runtime|crates/numanager-config|crates/numanager-drivers-' "$workspace_file" >/dev/null; then
  printf 'workspace contains deprecated split runtime/config or per-driver crate members\n' >&2
  missing=1
fi
if rg --files "$audit_root/crates" | rg '(^|/)numanager-runtime(/|$)|(^|/)numanager-config(/|$)|(^|/)numanager-drivers-[^/]+(/|$)' >/dev/null; then
  printf 'workspace contains deprecated split runtime/config or per-driver crates\n' >&2
  missing=1
fi

require_evidence_row abs_camera
require_evidence_row agilent_laser_combiner
require_evidence_row mcl
require_evidence_row mightex_bls
require_evidence_row mightex_camera
require_evidence_row okolab
require_evidence_row andor_camera
require_evidence_row evident_ix85
require_evidence_row photometrics_pvcam

require_device_index_row abs_camera abs-camera.md
require_device_index_row mcl mcl.md
require_device_index_row mightex_camera mightex-camera.md
require_text "$device_index_file" '| `numanager_drivers::abs_camera` | ABS legacy USB cameras | Runtime-package evidence with file-status/digest/loadability/ABI-symbol checks, writable exposure setting, explicit async software trigger, opt-in vendor-runtime capture, and repeated one-shot stream support; native transport, native continuous streaming, gain controls, persistent trigger modes, and broader acquisition behavior is not exposed because USB protocol evidence is absent | [abs-camera.md](abs-camera.md) | `discover_devices` |' "ABS device-index discovery row"
require_text "$device_index_file" '| `numanager_drivers::mightex_camera` | Mightex buffered USB cameras | Runtime-package evidence with file-status/digest/loadability/ABI-symbol checks, writable capture parameters, opt-in vendor-runtime `Mono16`/`Raw16` capture, and repeated one-shot stream support; native frame transport, native continuous streaming, native gain/color controls, ROI/binning beyond configured frame dimensions, and broader SDK-free acquisition behavior is not exposed because native protocol evidence is absent | [mightex-camera.md](mightex-camera.md) | `discover_devices` |' "Mightex camera device-index discovery row"
require_text "$device_index_file" '| `numanager_drivers::okolab` | Okolab environmental controllers | Reverse engineered serial/configured runtime support with opt-in connected read/write and refresh helpers | [okolab.md](okolab.md) | `discover_devices`, `environment_control` |' "Okolab device-index row"
require_text "$device_index_file" "| \`numanager_drivers::agilent_laser_combiner\` | Agilent/Keysight Laser Combiner | Implemented from external protocol evidence with typed control paths and mapped readback helpers | [agilent-laser-combiner.md](agilent-laser-combiner.md) | \`discover_devices\`, \`light_source\` |" "Agilent device-index row"
require_text "$device_index_file" "| \`numanager_drivers::arduino\` | Micro-Manager Arduino controller | Firmware protocol control plus opt-in configured real serial startup readback, output/control writes, ADC/digital input readback, and refresh helpers | [arduino.md](arduino.md) | \`discover_devices\`, \`digital_io\` |" "Arduino device-index discovery workflow row"
require_text "$device_index_file" "| \`numanager_drivers::arduino_counter\` | Arduino Counter | Counter/pulse protocol control plus opt-in configured real serial snapshot/count readback and refresh helper | [arduino-counter.md](arduino-counter.md) | \`discover_devices\`, \`digital_io\` |" "Arduino Counter device-index discovery workflow row"
require_text "$device_index_file" "| \`numanager_drivers::esp32\` | Micro-Manager ESP32 controller | Firmware protocol control plus opt-in configured real serial startup readback, GPIO/PWM/shutter/motion writes, ADC readback, and position refresh helpers | [esp32.md](esp32.md) | \`discover_devices\`, \`motion_stage\`, \`digital_io\`, \`shutter\` |" "ESP32 device-index discovery workflow row"
require_text "$device_index_file" "| \`numanager_drivers::openuc2\` | OpenUC2 Feather controller | JSON-line motion/light control plus opt-in configured real serial startup readback, typed wavelength metadata, and state refresh helper | [openuc2.md](openuc2.md) | \`discover_devices\`, \`motion_stage\`, \`light_source\` |" "OpenUC2 device-index discovery workflow row"
require_text "$device_index_file" "| \`numanager_drivers::teensy_pulse\` | Teensy pulse generator | Binary pulse control plus opt-in configured real serial startup/program readback path and enquiry refresh helpers | [teensy-pulse.md](teensy-pulse.md) | \`discover_devices\`, \`digital_io\` |" "Teensy Pulse device-index discovery workflow row"

require_readme_index_row "ABS legacy USB cameras" abs-camera.md
require_readme_index_row "Mad City Labs MicroDrive/NanoDrive" mcl.md
require_readme_index_row "Mightex buffered USB cameras" mightex-camera.md
require_text "$readme_file" "| [ABS legacy USB cameras](docs/devices/abs-camera.md) | Runtime-package evidence, writable exposure, explicit software trigger, opt-in vendor-runtime capture, and repeated-capture stream | - |" "ABS README package row"
require_text "$readme_file" "| [Mad City Labs MicroDrive/NanoDrive](docs/devices/mcl.md) | USB descriptor discovery, MicroDrive raw encoder/status readback, fixed-length raw control read/actions, and firmware/runtime package checks | - |" "MCL README package row"
require_text "$readme_file" "| [Mightex buffered USB cameras](docs/devices/mightex-camera.md) | Runtime-package evidence, writable capture settings, opt-in vendor-runtime Mono16/Raw16 capture, and repeated-capture stream | - |" "Mightex camera README package row"
require_text "$readme_file" "| [Okolab environmental controllers](docs/devices/okolab.md) | Serial environmental control and readback | - |" "Okolab README index row"
require_text "$readme_file" "| [Agilent/Keysight Laser Combiner](docs/devices/agilent-laser-combiner.md) | Laser control and readback | - |" "Agilent README index row"
require_text "$readme_file" "| [Andor SDK2 cameras](docs/devices/andor-sdk2.md) | Andor VID/PID USB discovery, firmware/runtime package checks, EP0 control helpers, opt-in live Mono16 capture, and vendor-runtime exposure/detector/cooler control | - |" "Andor SDK2 README index row"
require_text "$readme_file" "| [Andor SDK3 cameras](docs/devices/andor-sdk3.md) | Andor VID/PID USB discovery, hidden FX3 firmware init, EP0 status readback, runtime package checks, vendor-runtime feature control, and Mono16 capture | - |" "Andor SDK3 README index row"
require_text "$readme_file" "| [OS/platform cameras](docs/devices/platform-camera.md) | Descriptor-only V4L2 discovery plus explicit V4L2 read capture and local frame source | - |" "platform camera README index row"
require_text "$device_index_file" "| \`numanager_drivers::platform_camera\` | OS camera backends | Descriptor-only Linux V4L2 discovery, explicit configured V4L2 \`read()\` capture/stream for fixed-size raw frames, and local PGM/PPM frame source" "platform camera device-index row"
require_text "$evidence_file" "Linux V4L2 sysfs device descriptors and read-based device API" "platform camera evidence row"
require_text "${device_docs_dir}/platform-camera.md" "Optional local Netpbm \`P2\`, \`P3\`, \`P5\`, or \`P6\` fixture file" "platform camera fixture path documentation"
require_text "${driver_src_dir}/platform_camera.rs" "fn decode_portable_pixmap(" "platform camera PGM/PPM fixture decoder"
require_text "${driver_src_dir}/platform_camera.rs" "active_v4l2_probes" "platform camera active V4L2 descriptor discovery"
require_text "${driver_src_dir}/platform_camera.rs" "fn read_v4l2_frame(" "platform camera explicit V4L2 read capture"
require_text "${driver_src_dir}/platform_camera.rs" "can_generate_frames" "platform camera descriptor-only capture gate"
require_text "${driver_src_dir}/platform_camera.rs" "unsupported platform camera capability invocation" "platform camera dispatch fails closed for unsupported capability invocation"
require_text "${device_docs_dir}/platform-camera.md" "Descriptor-only OS cameras with no fixture source do not advertise capture/stream/trigger capabilities unless \`backend = \"v4l2\"\`, \`device_path\`, and \`connect = true\` are explicitly configured" "platform camera descriptor-only capability gate"
require_text "$readme_file" "| [GigE Vision cameras](docs/devices/gige-vision.md) | GVCP/GVSP model plus opt-in UDP GVCP mapped-property and raw-register access | - |" "GigE Vision README index row"
require_text "$readme_file" "| [USB3 Vision cameras](docs/devices/usb3-vision.md) | U3V command/stream model plus opt-in USB open, endpoint catalog, and live command ReadMem/WriteMem path | - |" "USB3 Vision README index row"
require_text "$readme_file" "| [GenICam node maps](docs/devices/genicam.md) | Node-map execution model with maintenance filtering and local frame source | - |" "GenICam README index row"
require_text "$device_index_file" "| \`numanager_drivers::gige_vision\` | GigE Vision cameras | GVCP/GVSP command/frame model with optional local PGM/PPM frame source plus opt-in UDP GVCP mapped-property and raw-register control" "GigE Vision device-index row"
require_text "$device_index_file" "| \`numanager_drivers::usb3_vision\` | USB3 Vision cameras | U3V control/stream/event model with optional local PGM/PPM frame source plus opt-in USB identity/open/endpoint-catalog and command-endpoint ReadMem/WriteMem path" "USB3 Vision device-index row"
require_text "$device_index_file" "| \`numanager_drivers::genicam\` | GenICam node maps | XML/register node-map execution model with maintenance-command filtering and optional local PGM/PPM frame source" "GenICam device-index row"
require_text "$evidence_file" "XML-derived node maps, and Netpbm PGM/PPM frame formats" "GenICam evidence row"
require_text "$evidence_file" "executable node names and XML-derived properties matching reset, firmware, upload, bootloader, flash, DFU, factory, default, restore, store, loader, or program are hidden from metadata and invocation" "GenICam hidden maintenance command evidence"
require_text "$evidence_file" "raw register writes require a named public node target rather than arbitrary address/register targets" "GenICam raw-register write boundary evidence"
require_text "$evidence_file" "raw register writes require a named public node target rather than arbitrary address targets" "GigE Vision raw-register write boundary evidence"
require_text "$evidence_file" "raw memory/register writes require a named public node target rather than arbitrary address targets" "USB3 Vision raw-register write boundary evidence"
require_text "${driver_src_dir}/genicam.rs" "fn is_hidden_genicam_command(command: &str) -> bool {" "GenICam hidden maintenance command filter"
require_text "${driver_src_dir}/genicam.rs" "generic_command_is_hidden_maintenance(command)" "GenICam uses core hidden maintenance classifier"
require_text "${driver_src_dir}/genicam.rs" "GenICam command node {} is a hidden maintenance command" "GenICam hidden command invocation gate"
require_text "${driver_src_dir}/genicam.rs" "if is_hidden_genicam_node(node) {" "GenICam raw-register resolved-node maintenance filter"
require_text "${driver_src_dir}/genicam.rs" "RawRegisterAccess writes require a non-maintenance node target" "GenICam raw-register write target gate"
require_text "${driver_src_dir}/gige_vision.rs" "RawRegisterAccess writes require a named public node target" "GigE Vision raw-register write target gate"
require_text "${driver_src_dir}/usb3_vision.rs" "RawRegisterAccess writes require a named public node target" "USB3 Vision raw-register write target gate"
require_text "${device_docs_dir}/gige-vision.md" "writes require a named public \`node\` target" "GigE Vision raw-register write docs"
require_text "${device_docs_dir}/usb3-vision.md" "writes require a named public \`node\` target" "USB3 Vision raw-register write docs"
require_text "${device_docs_dir}/genicam.md" "Optional local Netpbm \`P2\`, \`P3\`, \`P5\`, or \`P6\` file" "GenICam local frame path documentation"
require_text "${device_docs_dir}/gige-vision.md" "Optional local Netpbm \`P2\`, \`P3\`, \`P5\`, or \`P6\` file" "GigE Vision local frame path documentation"
require_text "${device_docs_dir}/gige-vision.md" "UDP GVCP ACK validation" "GigE Vision UDP GVCP documentation"
require_text "${device_docs_dir}/usb3-vision.md" "Optional local Netpbm \`P2\`, \`P3\`, \`P5\`, or \`P6\` file" "USB3 Vision local frame path documentation"
require_text "${device_docs_dir}/usb3-vision.md" "mapped property writes, trigger writes, mapped timing" "USB3 Vision live control evidence documentation"
require_text "${driver_src_dir}/genicam.rs" "fixture_path: Option<String>" "GenICam fixture path code"
require_text "${driver_src_dir}/gige_vision.rs" "fixture_path: Option<String>" "GigE Vision fixture path code"
require_text "${driver_src_dir}/gige_vision.rs" "fn send_gvcp_packet" "GigE Vision UDP GVCP raw-register control"
require_text "${driver_src_dir}/gige_vision.rs" "parse_gvcp_ack" "GigE Vision GVCP ACK parsing"
require_text "${driver_src_dir}/gige_vision.rs" "write_live_gvcp_property_if_mapped" "GigE Vision UDP GVCP mapped-property writes"
require_text "${driver_src_dir}/usb3_vision.rs" "fixture_path: Option<String>" "USB3 Vision fixture path code"
require_text "${driver_src_dir}/usb3_vision.rs" "fn open_usb3_vision_interface" "USB3 Vision USB open path"
require_text "${driver_src_dir}/usb3_vision.rs" "usb_claimed" "USB3 Vision USB claim metadata"
require_text "${driver_src_dir}/usb3_vision.rs" "descriptor_endpoints" "USB3 Vision descriptor endpoint metadata"
require_text "${driver_src_dir}/usb3_vision.rs" "fn live_u3v_command(" "USB3 Vision live command endpoint helper"
require_text "${driver_src_dir}/usb3_vision.rs" "u3v::decode_ack" "USB3 Vision live command ACK validation"
require_text "$evidence_file" "live U3V ReadMem/WriteMem for mapped property writes, trigger writes, mapped timing writes, and \`RawRegisterAccess\`" "USB3 Vision live command evidence"
require_text "${driver_src_dir}/usb3_vision.rs" "usb3_vision_endpoint_summary" "USB3 Vision endpoint summary code"
require_text "$readme_file" "| [Spark Cyto](docs/devices/spark-cyto.md) | TDCL over USB for plate, detector, environment, motion, optics-carrier, injector, barcode, imaging-head, and camera workflows, including image capture over the TDCL data channel; the reader's USB id is configured because it is not evidenced | - |" "Spark Cyto README index row"
require_text "$device_index_file" "| \`numanager_drivers::spark_cyto\` | Spark Cyto | TDCL over USB for plate, detector, environment, motion, optics-carrier, injector, barcode, imaging-head, and camera workflows, with image capture served over the TDCL data channel" "Spark Cyto device-index row"
require_text "$evidence_file" "image capture is served over the TDCL data channel" "Spark Cyto evidence row"
require_text "${device_docs_dir}/spark-cyto.md" "Simulated two-stage discovery plus config-backed discovery" "Spark Cyto documented config discovery"
require_text "${device_docs_dir}/spark-cyto.md" '| `GenericCommand` | Hub/gateway metadata devices | `CapabilityRequest::GenericCommand` | Echoed command/parameter summary | Runtime token completion | No |' "Spark Cyto documented GenericCommand summary capability"
require_text "${driver_src_dir}/spark_cyto.rs" "generic_command_value_is_hidden_maintenance(value)" "Spark Cyto custom hidden-maintenance request gate"
require_text "${driver_src_dir}/spark_cyto.rs" "pub fn from_config(next_id: DriverId, config: &HardwareConfig) -> Result<Self>" "Spark Cyto config discovery constructor"
require_text "${driver_src_dir}/spark_cyto.rs" "TDCL/CAN graph and transaction model with typed state operations" "Spark Cyto runtime support summary"
require_text "${device_docs_dir}/spark-cyto.md" "Null\` until the instrument answers" "Spark Cyto temperature readback boundary"
require_text "${device_docs_dir}/spark-cyto.md" "how many steps make a micrometre is a property of the mechanism" "Spark Cyto axis-unit boundary"
require_text "${driver_src_dir}/spark/usb.rs" "the id is not in the recovered evidence" "Spark Cyto USB identity boundary"
require_text "$run_examples_file" "light_source [coolled\\|pe4000\\|pe340\\|agilent\\|obis\\|omicron\\|lumencor\\|lmm5" "run examples light-source selector list"
require_text "$run_examples_file" "digital_io\` for the Arduino/Arduino Counter/ASI Tiger/Teensy workflow" "run examples default digital-IO workflow"
require_text "$run_examples_file" "digital_io [arduino\\|arduino_counter\\|esp32\\|teensy_pulse\\|triggerscope\\|wosm\\|modbus\\|velleman]" "run examples configured digital-IO selector list"
require_text "$run_examples_file" "environment_control [andor_sdk2\\|andor_sdk3\\|spark_cyto\\|okolab]" "run examples environment-control selector list"
require_text "$run_examples_file" "plate_reader [absorbance\\|fluorescence\\|luminescence]\` for Spark Cyto detector mode selection" "run examples plate-reader detector-mode selector list"
require_text "$run_examples_file" "cargo run -p numanager-examples --features gui -- software_gui --smoke" "run examples GUI smoke command"
require_text "$example_outputs_file" "software gui smoke" "recorded GUI smoke output"
require_text "$example_outputs_file" "sim-microscope-camera [camera, simulator]" "recorded GUI camera output"
require_text "$example_outputs_file" "sim-microscope-xy [stage.xy, axis.xy, simulator]" "recorded GUI pan-stage output"
require_text "$example_outputs_file" "optics: 0.325 um per image pixel" "recorded GUI derived optical scale"
require_text "$example_outputs_file" "stream status: depth=8 capacity=8 dropped=4" "recorded GUI stream-status output"
require_text "${audit_root}/crates/numanager-examples/src/main.rs" "#[cfg(feature = \"gui\")]" "software GUI feature gate"
require_text "${audit_root}/crates/numanager-examples/src/software_gui.rs" "if std::env::args().any(|arg| arg == \"--smoke\")" "software GUI smoke branch"
require_text "${device_docs_dir}/spark-cyto.md" "plate_reader fluorescence" "Spark Cyto fluorescence plate-reader example"
require_text "${device_docs_dir}/spark-cyto.md" "plate_reader luminescence" "Spark Cyto luminescence plate-reader example"
require_text "$readme_file" 'Exceptions under `data/third_party/` are third-party data' "README third-party license boundary"
require_text "$audit_root/data/third_party/README.md" "The general interim solution whenever firmware, a loader, or a vendor runtime" "third-party firmware/runtime default policy"
require_text "$audit_root/data/third_party/README.md" "project-owned firmware, loader, or open runtime" "third-party firmware/runtime replacement policy"
require_text "$audit_root/data/third_party/README.md" "default implementation path for every" "third-party firmware/runtime default implementation policy"
require_text "$audit_root/data/third_party/README.md" "redistribution terms" "third-party firmware/runtime redistribution policy"
require_text "$audit_root/data/third_party/README.md" "only when an explicit configured backend needs them" "third-party firmware/runtime demand-load policy"
require_text "$protocol_evidence_plan_file" "The general interim solution whenever firmware, a loader, or a vendor runtime" "protocol evidence firmware/runtime default policy"
require_text "$protocol_evidence_plan_file" "default path for every firmware-dependent device" "protocol evidence firmware/runtime default path policy"
require_text "$protocol_evidence_plan_file" "redistribution terms permit it" "protocol evidence firmware/runtime redistribution policy"
require_text "$protocol_evidence_plan_file" "only on demand through explicit configuration" "protocol evidence firmware/runtime demand-load policy"
require_text "${driver_src_dir}/arduino.rs" "ArduinoDriver::serial" "Arduino configured real serial constructor"
require_text "${driver_src_dir}/arduino.rs" "fn refresh_startup_probe(&mut self, timeout_ms: u64) -> Result<()> {" "Arduino startup probe readback"
require_text "${driver_src_dir}/arduino.rs" "protocol::CMD_READ_DIGITAL_INPUTS" "Arduino connected digital-input readback code"
require_text "${driver_src_dir}/arduino.rs" "protocol::CMD_READ_ANALOG_INPUT" "Arduino connected ADC readback code"
require_text "${driver_src_dir}/arduino.rs" "ArduinoCommand::SetInputPullUp" "Arduino input pull-up control code"
require_text "${driver_src_dir}/arduino.rs" "Arduino GenericCommand supports refresh_inputs, refresh_digital_inputs, and refresh_channel_0" "Arduino mapped GenericCommand validation"
require_text "${driver_src_dir}/arduino.rs" '"baud_rate".into(), Value::I64(self.baud_rate as i64)' "Arduino resource baud-rate metadata"
require_text "${driver_src_dir}/arduino.rs" '"connected".into(), Value::Bool(self.connected)' "Arduino resource connection metadata"
require_text "${device_docs_dir}/arduino.md" "| \`input_pullups\` | ADC | \`I64\`" "Arduino input pull-up property documentation"
require_text "${device_docs_dir}/arduino.md" "Config-backed firmware protocol plus opt-in configured real serial" "Arduino documented configured serial support"
require_text "${device_docs_dir}/arduino.md" '`GenericCommand` | ADC | `refresh_inputs`, `refresh_digital_inputs`, or `refresh_channel_0` with no params' "Arduino documented mapped GenericCommand"
require_text "$evidence_file" "input-pull-up paths write the same firmware commands" "Arduino evidence input pull-up wording"
require_text "$evidence_file" "connected construction reads controller ID, version, pattern count, DAC channel count, and digital pin count before registration" "Arduino evidence startup readback wording"
require_text "$evidence_file" "connected ADC and digital-input reads and named GenericCommand refresh helpers consume firmware reply frames" "Arduino evidence mapped GenericCommand wording"
require_text "${driver_src_dir}/esp32.rs" "Esp32Driver::serial" "ESP32 configured real serial constructor"
require_text "${driver_src_dir}/esp32.rs" "fn refresh_startup_probe(&mut self, timeout_ms: u64) -> Result<()> {" "ESP32 startup probe readback"
require_text "${driver_src_dir}/esp32.rs" "fn drain_position_replies(&mut self) -> Result<bool>" "ESP32 connected position readback"
require_text "${driver_src_dir}/esp32.rs" "ESP32 GenericCommand supports refresh_position/refresh_state on hub/stages and refresh_adc on ADC" "ESP32 mapped GenericCommand validation"
require_text "${driver_src_dir}/esp32.rs" 'Esp32Command::ReadAnalog { channel } => format!("A,{channel}")' "ESP32 ADC command encoder"
require_text "${driver_src_dir}/esp32.rs" "fn drain_analog_replies(&mut self) -> Result<bool>" "ESP32 connected ADC readback"
require_text "${driver_src_dir}/esp32.rs" 'position_config_um(device, "x_travel", "x_travel_um")' "ESP32 canonical x_travel config"
require_text "${driver_src_dir}/esp32.rs" 'position_config_um(device, "y_travel", "y_travel_um")' "ESP32 canonical y_travel config"
require_text "${driver_src_dir}/esp32.rs" 'position_config_um(device, "z_travel", "z_travel_um")' "ESP32 canonical z_travel config"
require_text "${driver_src_dir}/esp32.rs" '"baud_rate".into(), Value::I64(self.baud_rate as i64)' "ESP32 resource baud-rate metadata"
require_text "${driver_src_dir}/esp32.rs" '"connected".into(), Value::Bool(self.connected)' "ESP32 resource connection metadata"
require_text "${device_docs_dir}/esp32.md" "Config-backed firmware protocol plus opt-in configured real serial" "ESP32 documented configured serial support"
require_text "${device_docs_dir}/esp32.md" '| `GenericCommand` | Hub/XY/Z | `refresh_position` or `refresh_state` with no params' "ESP32 documented mapped GenericCommand"
require_text "${device_docs_dir}/esp32.md" '| `GenericCommand` | ADC | `refresh_adc` with no params' "ESP32 documented ADC refresh command"
require_text "${device_docs_dir}/esp32.md" "\`x_travel\`, \`y_travel\`, \`z_travel\`" "ESP32 canonical travel config docs"
require_text "$evidence_file" "connected construction reads firmware version, X/Y/Z travel, and current \`W\` position before registration" "ESP32 evidence startup readback wording"
require_text "$evidence_file" "connected position/state reads and named GenericCommand refresh helpers consume \`W\` reply frames" "ESP32 evidence mapped GenericCommand wording"
require_text "$evidence_file" "connected ADC reads and the named ADC GenericCommand helper consume \`A,0\` reply frames" "ESP32 evidence ADC readback wording"
require_text "${driver_src_dir}/arduino_counter.rs" "ArduinoCounterDriver::serial" "Arduino Counter configured real serial constructor"
require_text "${driver_src_dir}/arduino_counter.rs" "fn refresh_snapshot(&mut self) -> Result<()> {" "Arduino Counter snapshot readback"
require_text "${driver_src_dir}/arduino_counter.rs" "ArduinoCounter GenericCommand supports refresh_snapshot" "Arduino Counter mapped GenericCommand validation"
require_text "${driver_src_dir}/arduino_counter.rs" "protocol::parse_count(&reply)" "Arduino Counter connected count reply parsing"
require_text "${driver_src_dir}/arduino_counter.rs" '"baud_rate".into(), Value::I64(self.baud_rate as i64)' "Arduino Counter resource baud-rate metadata"
require_text "${driver_src_dir}/arduino_counter.rs" '"connected".into(), Value::Bool(self.connected)' "Arduino Counter resource connection metadata"
require_text "${device_docs_dir}/arduino-counter.md" "Config-backed counter protocol plus opt-in configured real serial" "Arduino Counter documented configured serial support"
require_text "${device_docs_dir}/arduino-counter.md" '`GenericCommand` | Hub/counter | `refresh_snapshot` with no params' "Arduino Counter documented mapped GenericCommand"
require_text "$evidence_file" "connected construction reads a \`p?\` snapshot before registration" "Arduino Counter evidence startup readback wording"
require_text "$evidence_file" 'named `refresh_snapshot` GenericCommand consume `p?` snapshots' "Arduino Counter evidence mapped GenericCommand wording"
require_text "${driver_src_dir}/teensy_pulse.rs" "TeensyPulseDriver::serial" "Teensy Pulse configured real serial constructor"
require_text "${driver_src_dir}/teensy_pulse.rs" "fn refresh_startup_state(&mut self) -> Result<()> {" "Teensy Pulse startup enquiry readback"
require_text "${driver_src_dir}/teensy_pulse.rs" "fn read_reply_until(&mut self) -> Result<()> {" "Teensy Pulse connected reply wait"
require_text "${driver_src_dir}/teensy_pulse.rs" "TeensyPulse GenericCommand supports refresh_readbacks, refresh_program, refresh_running, and refresh_counted_pulses" "Teensy Pulse mapped GenericCommand validation"
require_text "${driver_src_dir}/teensy_pulse.rs" '"baud_rate".into(), Value::I64(self.baud_rate as i64)' "Teensy Pulse resource baud-rate metadata"
require_text "${driver_src_dir}/teensy_pulse.rs" '"connected".into(), Value::Bool(self.connected)' "Teensy Pulse resource connection metadata"
require_text "${device_docs_dir}/teensy-pulse.md" "Config-backed binary protocol plus opt-in configured real serial" "Teensy Pulse documented configured serial support"
require_text "${device_docs_dir}/teensy-pulse.md" '`GenericCommand` | Pulse generator | `refresh_readbacks`, `refresh_program`, `refresh_running`, or `refresh_counted_pulses` with no params' "Teensy Pulse documented mapped GenericCommand"
require_text "$evidence_file" "connected construction enquires version, interval, duration, wait-for-input, number-of-pulses, and running/count state before registration" "Teensy Pulse evidence startup readback wording"
require_text "$evidence_file" "connected set/enquiry paths and named GenericCommand refresh helpers wait for decoded 5-byte replies" "Teensy Pulse evidence mapped GenericCommand wording"
require_text "$readme_file" "| [Evident/Olympus IX85](docs/devices/evident-ix85.md) | Serial focus, state-device, shutter, timing endpoints, and body readback/control | - |" "IX85 README index row"
require_text "$readme_file" "| [Photometrics/QImaging PVCAM cameras](docs/devices/photometrics-pvcam.md) | USB discovery, verified PVCAM runtime discovery, one-shot capture, repeated-capture stream, and temperature setpoint control | - |" "PVCAM README index row"

require_reverse_index_row "ABS Camera" abs-camera.md "one-shot capture can use an optional vendor runtime, loaded only through explicit user configuration"
require_reverse_index_row "Agilent Laser Combiner" agilent-laser-combiner.md "Transport grammar, full opcode table"
require_text "${driver_src_dir}/agilent_laser_combiner.rs" '"serial_port".into()' "Agilent serial-port resource metadata"
require_text "${driver_src_dir}/agilent_laser_combiner.rs" '"connected".into(), Value::Bool(self.connected)' "Agilent connected resource metadata"
require_text "${driver_src_dir}/agilent_laser_combiner.rs" "agilent-analog-output-" "Agilent analog-output descriptor"
require_text "${driver_src_dir}/agilent_laser_combiner.rs" "SetAnalogOutputRaw" "Agilent analog-output write command"
require_text "${driver_src_dir}/agilent_laser_combiner.rs" "GetAnalogOutputRaw" "Agilent analog-output read command"
require_text "${driver_src_dir}/agilent_laser_combiner.rs" "\"raw_counts\"" "Agilent analog-output raw-count property"
require_text "${driver_src_dir}/agilent_laser_combiner.rs" "Agilent GenericCommand supports refresh_identity, refresh_control_state, refresh_line_outputs, and refresh_line_metadata" "Agilent mapped GenericCommand validation"
require_text "${device_docs_dir}/agilent-laser-combiner.md" 'resource metadata records configured `serial_port`, fixed `baud_rate`, fixed `serial_timeout`, and `connected` state' "Agilent documented resource metadata"
require_text "${device_docs_dir}/agilent-laser-combiner.md" '| `agilent-analog-output-1..4` | `analog.output`, `diagnostic.raw` | Diagnostic raw analog-output channels remultiplexed through the combiner controller |' "Agilent documented analog-output devices"
require_text "${device_docs_dir}/agilent-laser-combiner.md" '| `raw_counts` | Analog output channels | `I64` | counts | R/W | `0..=65535` | No | Read cmd `0x2A`; write cmd `0x0C` |' "Agilent documented analog raw-count property"
require_text "${device_docs_dir}/agilent-laser-combiner.md" "does not claim a calibrated voltage range" "Agilent documented analog-output calibration boundary"
require_text "${device_docs_dir}/agilent-laser-combiner.md" '| `GenericCommand` | Hub | `refresh_identity`, `refresh_control_state`, `refresh_line_outputs`, or `refresh_line_metadata` with no params | Refreshed state map | Uses only typed request/reply getter paths already represented as properties; no register, EEPROM, AOTF, or hardware sequence command surface | Not sequenceable |' "Agilent documented mapped GenericCommand"
require_text "$evidence_file" 'hub `GenericCommand` is constrained to typed identity, control-state, line-output, and line-metadata readback helpers with no register, EEPROM, AOTF, or hardware sequence command surface' "Agilent evidence mapped GenericCommand wording"
require_text "$evidence_file" 'diagnostic analog-output devices expose raw `0x0C`/`0x2A` DAC counts without calibrated voltage claims' "Agilent evidence analog-output boundary"
require_text "$evidence_file" 'resource metadata records configured `serial_port`, fixed `baud_rate`, fixed `serial_timeout`, and `connected` state' "Agilent evidence resource metadata wording"
require_reverse_index_row "MCL MicroDrive/NanoDrive" mcl.md "typed motion/control is not exposed because payload fields"
require_reverse_index_row "Mightex / Mightex_BLS" mightex.md "camera one-shot capture and repeated one-shot stream can use an optional vendor runtime loaded only through explicit user configuration"
require_reverse_index_row "Okolab" okolab.md "Reverse engineered serial/configured runtime support"
require_reverse_index_row "Photometrics PVCAM" photometrics-pvcam.md "runtime temperature read/setpoint control exist"
require_reverse_index_row "Tecan Spark Cyto" spark-cyto.md "no command spelling has met hardware"
require_reverse_note_shape spark-cyto.md
require_file "${reverse_docs_dir}/spark-cyto-protocol.md"
require_text "${reverse_docs_dir}/spark-cyto-protocol.md" "usbmon/USBPcap capture of a live session exists" "Spark Cyto protocol capture boundary"
require_text "${reverse_docs_dir}/spark-cyto.md" "payload length matches the geometry the camera" "Spark Cyto camera evidence boundary"

require_export andor_camera
require_export agilent_laser_combiner
require_export evident_ix85
require_export photometrics_pvcam
require_file "${device_docs_dir}/andor-camera.md"
require_file "${device_docs_dir}/andor-sdk2.md"
require_file "${device_docs_dir}/andor-sdk3.md"
require_file "${device_docs_dir}/evident-ix85.md"
require_file "${device_docs_dir}/photometrics-pvcam.md"
require_file "${reverse_docs_dir}/photometrics-pvcam.md"
require_text "$device_index_file" "| \`numanager_drivers::andor_camera\` | Andor/Oxford Instruments SDK2 cameras | Andor VID/PID USB discovery, config-gated hidden firmware initialization from ambiguous EZ-USB devices, firmware/runtime package checks, EP0 identity/status/FIFO/acquisition helpers, opt-in live bulk-IN \`Mono16\` capture, and vendor-runtime exposure, full-frame capture, detector readback, and temperature/cooler control" "Andor SDK2 device-index row"
require_text "$device_index_file" "| \`numanager_drivers::andor_camera\` | Andor/Oxford Instruments SDK3 cameras | Andor VID/PID USB discovery, config-gated hidden FX3 firmware initialization from ambiguous EZ-USB devices, confirmed EP0 status readbacks, firmware/runtime package checks, vendor-runtime feature control/readback, cooler control, and opt-in \`Mono16\` capture" "Andor SDK3 device-index row"
require_text "$device_index_file" "| \`numanager_drivers::mcl\` | Mad City Labs MicroDrive/NanoDrive | Active USB descriptor discovery plus opt-in MicroDrive raw encoder/status readback, fixed-length raw MicroDrive control-read/action commands, and firmware/runtime package checks; typed stage motion is not exposed because units, status, and completion evidence are absent | [mcl.md](mcl.md) | \`discover_devices\` |" "MCL device-index package row"
require_text "$device_index_file" "| \`numanager_drivers::evident_ix85\` | Evident/Olympus IX85 microscope body | Configured opt-in serial focus motion/stop, state-device selection, shutter control, software timing endpoints, body readback, and hub refresh commands" "IX85 device-index row"
require_text "$device_index_file" "| \`numanager_drivers::photometrics_pvcam\` | Photometrics/QImaging PVCAM cameras | Configured and active USB evidence, verified runtime camera-name discovery, package checks, writable exposure setting, opt-in one-shot capture, repeated one-shot stream support, and runtime temperature read/setpoint control; native continuous streaming and broader parameter control are not exposed because documented ABI/native-transport evidence is absent" "PVCAM device-index row"
require_text "${device_docs_dir}/andor-sdk2.md" "| \`CameraCapture\` | Camera |" "Andor SDK2 capture capability"
require_text "${device_docs_dir}/andor-sdk2.md" "| \`TemperatureControl\` | Cooler |" "Andor SDK2 cooler capability"
require_text "${device_docs_dir}/andor-sdk2.md" "\`connect=true\`" "Andor SDK2 explicit live-capture gate"
require_text "${device_docs_dir}/andor-sdk2.md" "| \`vendor_id\`, \`product_id\`, \`usb_identity\` | Hub | \`I64\` / \`Map\`" "Andor SDK2 USB identity property"
require_text "${device_docs_dir}/andor-sdk2.md" "ABI-symbol state" "Andor SDK2 vendor-runtime ABI property"
require_text "${device_docs_dir}/andor-sdk2.md" "Firmware upload, FIFO reset, and acquisition-control requests are hidden driver-internal steps" "Andor SDK2 hidden firmware/reset control surface"
require_text "${device_docs_dir}/andor-sdk3.md" "| \`CameraCapture\` | Camera |" "Andor SDK3 vendor-runtime capture capability"
require_text "${device_docs_dir}/andor-sdk3.md" "| \`TemperatureControl\` | Cooler |" "Andor SDK3 vendor-runtime cooler capability"
require_text "${device_docs_dir}/andor-sdk3.md" "verified vendor runtime" "Andor SDK3 explicit runtime-capture gate"
require_text "${device_docs_dir}/andor-sdk3.md" "firmware upload is hidden and initialization-only" "Andor SDK3 hidden firmware init documentation"
require_text "${device_docs_dir}/andor-sdk3.md" "native USB write/acquisition framing is not exposed because request/ACK evidence is absent" "Andor SDK3 native write-framing evidence policy"
require_text "$evidence_file" "runtime-package file-status/digest/loadability/ABI-symbol checks" "Andor evidence ABI surface row"
require_text "${driver_src_dir}/andor_camera.rs" "vendor_runtime_state" "Andor vendor-runtime state code"
require_text "${driver_src_dir}/andor_camera.rs" "vendor_runtime_file_size" "Andor vendor-runtime file size code"
require_text "${driver_src_dir}/andor_camera.rs" "vendor_runtime_digest_state" "Andor vendor-runtime digest state code"
require_text "${driver_src_dir}/andor_camera.rs" "vendor_runtime_probe_state" "Andor vendor-runtime probe code"
require_text "${driver_src_dir}/andor_camera.rs" "vendor_runtime_abi_state" "Andor vendor-runtime ABI code"
require_text "${driver_src_dir}/andor_camera.rs" "AT_InitialiseLibrary" "Andor SDK3 ABI symbol probe"
require_text "${driver_src_dir}/andor_camera.rs" "GetAcquiredData16" "Andor SDK2 ABI symbol probe"
require_text "${driver_src_dir}/andor_camera.rs" "SetExposureTime" "Andor SDK2 exposure ABI symbol probe"
require_text "${driver_src_dir}/andor_camera.rs" "SetImage" "Andor SDK2 image setup ABI symbol probe"
require_text "${driver_src_dir}/andor_camera.rs" "GetDetector" "Andor SDK2 detector ABI symbol probe"
require_text "${driver_src_dir}/andor_camera.rs" "CoolerON" "Andor SDK2 cooler ABI symbol probe"
require_text "${driver_src_dir}/andor_camera.rs" "SDK2 temperature/cooler control uses verified vendor-runtime atmcd functions" "Andor SDK2 cooler support wording"
require_text "${driver_src_dir}/andor_camera.rs" "upload_fx2_firmware" "Andor hidden SDK2 firmware init code"
require_text "${driver_src_dir}/andor_camera.rs" "upload_fx3_firmware" "Andor hidden SDK3 firmware init code"
require_text "${driver_src_dir}/andor_camera.rs" "parse_fx3_image" "Andor SDK3 CY-image parser code"
require_text "${driver_src_dir}/andor_camera.rs" "hidden firmware initialization" "Andor hidden firmware init wording"
require_text "${driver_src_dir}/andor_camera.rs" "package_digest_allows_use" "Andor package digest gate code"
require_text "${driver_src_dir}/andor_camera.rs" "package_strategy" "Andor package strategy code"
require_text "${driver_src_dir}/andor_camera.rs" "active_usb_probes" "Andor active USB descriptor scan code"
require_text "${driver_src_dir}/andor_camera.rs" "is_andor_usb_candidate" "Andor USB VID/PID scan gate code"
require_text "${driver_src_dir}/andor_camera.rs" "SDK2 Andor VID/PID USB discovery, config-gated hidden firmware initialization from ambiguous EZ-USB devices, firmware/runtime package checks, EP0 command helpers, live bulk-IN Mono16 capture behind os-usb, and vendor-runtime exposure, full-frame capture, detector readback, and cooler control" "Andor SDK2 support-level wording"
require_text "${driver_src_dir}/andor_camera.rs" "SDK3 Andor VID/PID USB discovery, config-gated hidden FX3 firmware initialization from ambiguous EZ-USB devices, confirmed EP0 status readbacks, firmware/runtime package checks, vendor-runtime feature control/readback, cooler control, and capture backend" "Andor SDK3 support-level wording"
require_text "${driver_src_dir}/andor_camera.rs" "AT_QueueBuffer/AT_Command/AT_WaitBuffer" "Andor SDK3 vendor-runtime capture path"
require_text "${driver_src_dir}/andor_camera.rs" "AT_SetEnumString" "Andor SDK3 vendor-runtime feature setter"
require_text "${driver_src_dir}/andor_camera.rs" "CapabilityKind::TemperatureControl" "Andor SDK3 cooler capability code"
require_text "${driver_src_dir}/andor_camera.rs" "CapabilityKind::CameraCapture" "Andor SDK2 CameraCapture code"
require_text "${driver_src_dir}/andor_camera.rs" "SDK2_ACQUISITION_START" "Andor SDK2 acquisition control code"
require_text "${driver_src_dir}/andor_camera.rs" "RequestBuffer::new(padded * SDK2_READOUT_BYTES_PER_PIXEL as usize)" "Andor SDK2 bulk readout code"
require_text "$audit_root/data/third_party/andor/README.md" "Third-party data" "Andor trimmed third-party package README"
require_text "$audit_root/data/third_party/README.md" "The general interim solution whenever firmware, a loader, or a vendor runtime" "third-party data firmware/runtime policy"
require_text "$audit_root/data/third_party/README.md" "The package may include original vendor" "third-party data replacement policy"
require_text "$audit_root/data/third_party/README.md" "redistribution terms" "third-party data redistribution policy"
require_text "$audit_root/data/third_party/README.md" "only when an explicit configured backend needs them" "third-party data demand-load policy"
require_text "$audit_root/AGENTS.md" "replacement firmware is not ready" "agent firmware-package implementation rule"
require_text "$audit_root/data/third_party/README.md" "firmware-dependent device" "third-party firmware-package implementation rule"
require_text "$audit_root/docs/planning/device_implementation_plan.md" "default implementation path for every firmware-dependent device" "planning firmware-package implementation rule"
require_text "$audit_root/docs/planning/device_implementation_plan.md" "redistribution terms permit it" "planning firmware-package redistribution rule"
require_text "$protocol_evidence_plan_file" "The package requirement" "protocol evidence firmware-package implementation rule"
require_text "${device_docs_dir}/andor-sdk2.md" "firmware upload is hidden and initialization-only" "Andor SDK2 hidden firmware package note"
require_text "${device_docs_dir}/andor-sdk2.md" 'verified `vendor_runtime_sha256`' "Andor SDK2 runtime digest gate note"
require_text "${device_docs_dir}/andor-sdk3.md" 'verified `vendor_runtime_sha256`' "Andor SDK3 runtime digest gate note"
require_text "$audit_root/data/third_party/andor/manifest.example.toml" "firmware-package-file" "Andor firmware manifest example"
require_text "${device_docs_dir}/mcl.md" "| \`vendor_runtime_state\` | Hub | \`String\`" "MCL vendor-runtime state property"
require_text "${device_docs_dir}/mcl.md" "| \`usb_identity\` | Hub | \`Map\`" "MCL USB identity property"
require_text "${device_docs_dir}/mcl.md" "descriptor discovery lists evidenced MicroDrive/NanoDrive/pre-firmware IDs without opening them" "MCL descriptor discovery gate"
require_text "${device_docs_dir}/mcl.md" "| \`vendor_runtime_digest_state\` | Hub | \`String\`" "MCL vendor-runtime digest state property"
require_text "${device_docs_dir}/mcl.md" "| \`vendor_runtime_probe_state\` | Hub | \`String\`" "MCL vendor-runtime probe property"
require_text "${device_docs_dir}/mcl.md" "| \`firmware_blob_file_status\` | Hub | \`String\`" "MCL firmware blob file status property"
require_text "${device_docs_dir}/mcl.md" "| \`firmware_blob_digest_state\` | Hub | \`String\`" "MCL firmware blob digest state property"
require_text "${device_docs_dir}/mcl.md" "| \`firmware_blob_probe_state\` | Hub | \`String\`" "MCL firmware blob probe property"
require_text "${driver_src_dir}/mcl.rs" "vendor_runtime_state" "MCL vendor-runtime state code"
require_text "${driver_src_dir}/mcl.rs" "vendor_runtime_file_size" "MCL vendor-runtime file size code"
require_text "${driver_src_dir}/mcl.rs" "vendor_runtime_digest_state" "MCL vendor-runtime digest state code"
require_text "${driver_src_dir}/mcl.rs" "vendor_runtime_probe_state" "MCL vendor-runtime probe code"
require_text "${driver_src_dir}/mcl.rs" "firmware_package_state" "MCL firmware package state code"
require_text "${driver_src_dir}/mcl.rs" "firmware_blob_digest_state" "MCL firmware blob digest state code"
require_text "${driver_src_dir}/mcl.rs" "firmware_blob_probe_state" "MCL firmware blob probe code"
require_text "${driver_src_dir}/mcl.rs" "package_digest_allows_use" "MCL package digest gate code"
require_text "${driver_src_dir}/mcl.rs" "package_strategy" "MCL package strategy code"
require_text "${driver_src_dir}/mcl.rs" "is_mcl_candidate" "MCL active USB VID/PID candidate gate"
require_text "${driver_src_dir}/mcl.rs" "active_usb_probes" "MCL active USB descriptor discovery code"
require_text "${driver_src_dir}/mcl.rs" "MCL active USB descriptor discovery, raw MicroDrive status/encoder readback, documented raw MicroDrive control-read/action commands, and firmware/runtime package checks" "MCL support-level digest-state wording"
require_text "${device_docs_dir}/mcl.md" "| \`GenericCommand\` | Hub | \`refresh_readbacks\`, \`refresh_status\`, \`refresh_encoders\`, \`refresh_8bit_movement_status\`, \`refresh_move_status\`, \`refresh_assignments\`, \`refresh_wait_time\`, \`refresh_temperature\`, \`refresh_mode\`, \`refresh_rotations\`, \`refresh_mmt_state\`, or \`stop\`" "MCL mapped refresh capability"
require_text "${driver_src_dir}/mcl.rs" "refresh_readbacks" "MCL mapped refresh command code"
require_text "${driver_src_dir}/mcl.rs" "refresh_status" "MCL mapped status refresh code"
require_text "${driver_src_dir}/mcl.rs" "refresh_encoders" "MCL mapped encoder refresh code"
require_text "${driver_src_dir}/mcl.rs" "refresh_move_status" "MCL mapped move-status refresh code"
require_text "${driver_src_dir}/mcl.rs" "refresh_assignments" "MCL mapped assignments refresh code"
require_text "${driver_src_dir}/mcl.rs" "refresh_mmt_state" "MCL mapped MMT state refresh code"
require_text "${driver_src_dir}/mcl.rs" "CapabilityKind::GenericCommand" "MCL GenericCommand descriptor"
require_text "$evidence_file" "named hub \`GenericCommand\` helpers for raw readbacks plus fixed-length movement-status, assignments, wait-time, temperature, mode, rotations, MMT-state, and stop requests" "MCL evidence named refresh wording"
require_text "$audit_root/data/third_party/mcl/README.md" "third-party data" "MCL trimmed third-party package README"
require_text "${device_docs_dir}/mcl.md" "after SHA-256 verification" "MCL third-party digest gate note"
require_text "${device_docs_dir}/mcl.md" "read_firmware_blob=true" "MCL explicit firmware read note"
require_text "$audit_root/data/third_party/mcl/manifest.example.toml" "mcl-firmware-package-file" "MCL firmware manifest example"
require_text "${device_docs_dir}/evident-ix85.md" "| \`GenericCommand\` | Hub | \`refresh_readbacks\`, \`refresh_identity\`, or \`refresh_status\`" "IX85 mapped refresh capability"
require_text "${device_docs_dir}/evident-ix85.md" "| \`StageMove\`/\`StageStop\` | Focus |" "IX85 focus motion capability"
require_text "${device_docs_dir}/evident-ix85.md" "| \`FilterSelect\` | Nosepiece/light path/mirror unit |" "IX85 state selection capability"
require_text "${device_docs_dir}/evident-ix85.md" "| \`TriggerSink\` | Shutters |" "IX85 shutter capability"
require_text "${device_docs_dir}/evident-ix85.md" "| \`action_gate\` | All devices | \`String\`" "IX85 action gate property"
require_text "${device_docs_dir}/evident-ix85.md" "| \`command_summary\` | All non-hub devices | \`Map\`" "IX85 command summary property"
require_text "${driver_src_dir}/evident_ix85.rs" "command_summary" "IX85 command summary code"
require_text "${driver_src_dir}/evident_ix85.rs" "feature_summary" "IX85 feature summary code"
require_text "${driver_src_dir}/evident_ix85.rs" "refresh_readbacks" "IX85 mapped refresh command code"
require_text "${driver_src_dir}/evident_ix85.rs" "refresh_identity" "IX85 mapped identity refresh code"
require_text "${driver_src_dir}/evident_ix85.rs" "refresh_status" "IX85 mapped status refresh code"
require_text "${driver_src_dir}/evident_ix85.rs" "fn prepare_timing_plan" "IX85 timing arm hook"
require_text "${driver_src_dir}/evident_ix85.rs" "fn start_timing_plan" "IX85 timing start hook"
require_text "${driver_src_dir}/evident_ix85.rs" "fn stop_timing_plan" "IX85 timing stop hook"
require_text "${driver_src_dir}/evident_ix85.rs" "fn apply_timing_sequence_step" "IX85 timing endpoint application"
require_text "${driver_src_dir}/evident_ix85.rs" "CapabilityKind::GenericCommand" "IX85 GenericCommand descriptor"
require_text "${driver_src_dir}/evident_ix85.rs" "CapabilityKind::StageMove" "IX85 StageMove descriptor"
require_text "${driver_src_dir}/evident_ix85.rs" "CapabilityKind::FilterSelect" "IX85 FilterSelect descriptor"
require_text "${driver_src_dir}/evident_ix85.rs" "CapabilityKind::TriggerSink" "IX85 TriggerSink descriptor"
require_text "${driver_src_dir}/evident_ix85.rs" '("move_absolute", "FG")' "IX85 focus absolute command"
require_text "${driver_src_dir}/evident_ix85.rs" '("move_relative", "FM")' "IX85 focus relative command"
require_text "${driver_src_dir}/evident_ix85.rs" '("stop", "FSTP")' "IX85 focus stop command"
require_text "${driver_src_dir}/evident_ix85.rs" "driver.query(\"V\")" "IX85 construction-time version readback"
require_text "${driver_src_dir}/evident_ix85.rs" "driver.query(\"U\")" "IX85 construction-time unit readback"
require_text "${driver_src_dir}/evident_ix85.rs" "refresh_connected_readbacks" "IX85 connected readback refresh"
require_text "${driver_src_dir}/evident_ix85.rs" '"serial_port".into()' "IX85 serial-port resource metadata"
require_text "${driver_src_dir}/evident_ix85.rs" '"connected".into(), Value::Bool(self.serial.is_some())' "IX85 connected resource metadata"
require_text "$evidence_file" "hub \`GenericCommand\` exposes named \`refresh_readbacks\`, \`refresh_identity\`, and \`refresh_status\` helpers" "IX85 evidence named refresh wording"
require_text "$evidence_file" "runtime timing hooks validate sequenceable focus, state-device, and shutter endpoints and apply first/last values through the same typed write/readback paths" "IX85 evidence timing endpoint wording"
require_text "${device_docs_dir}/evident-ix85.md" 'resource metadata records configured `serial_port`, fixed `baud_rate`, fixed `serial_timeout`, and `connected` state' "IX85 documented resource metadata"
require_text "$evidence_file" 'resource metadata records configured `serial_port`, fixed `baud_rate`, fixed `serial_timeout`, and `connected` state' "IX85 evidence resource metadata wording"
require_text "${device_docs_dir}/evident-ix85.md" 'Readback/control for `V`, `U`, `FP`, `FG`, `FM`, `FSTP`, `OB`, `BIL`, `MU1`, `DSH`, `ESH1`, and `AFST` is implemented' "IX85 documented readback/control"
require_text "${device_docs_dir}/photometrics-pvcam.md" "| \`CameraCapture\` | Camera |" "PVCAM capture capability"
require_text "${device_docs_dir}/photometrics-pvcam.md" "| \`TemperatureControl\` | Cooler |" "PVCAM temperature-control documentation"
require_text "${device_docs_dir}/photometrics-pvcam.md" "| \`usb_identity\` | Hub | \`Map\` or \`Null\`" "PVCAM USB identity property"
require_text "${device_docs_dir}/photometrics-pvcam.md" "descriptor scanning does not open devices or imply capture/control support" "PVCAM descriptor scan gate"
require_text "${device_docs_dir}/photometrics-pvcam.md" "| \`vendor_runtime_state\` | Hub | \`String\`" "PVCAM vendor-runtime state property"
require_text "${device_docs_dir}/photometrics-pvcam.md" "| \`vendor_runtime_file_status\` | Hub | \`String\`" "PVCAM vendor-runtime file status property"
require_text "${device_docs_dir}/photometrics-pvcam.md" "| \`vendor_runtime_digest_state\` | Hub | \`String\`" "PVCAM vendor-runtime digest state property"
require_text "${device_docs_dir}/photometrics-pvcam.md" "| \`vendor_runtime_probe_state\` | Hub | \`String\`" "PVCAM vendor-runtime loadability property"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "vendor_runtime_state" "PVCAM vendor-runtime state code"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "vendor_runtime_file_size" "PVCAM vendor-runtime file size code"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "vendor_runtime_digest_state" "PVCAM vendor-runtime digest state code"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "vendor_runtime_probe_state" "PVCAM vendor-runtime loadability code"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "vendor_runtime_abi_state" "PVCAM vendor-runtime ABI code"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "pl_pvcam_init" "PVCAM ABI init symbol probe"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "pl_cam_get_total" "PVCAM ABI camera-count symbol probe"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "pl_exp_start_seq" "PVCAM ABI acquisition symbol probe"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "vendor_runtime_digest_allows_use" "PVCAM vendor-runtime digest gate code"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "package_strategy" "PVCAM package strategy code"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "active_usb_probes" "PVCAM active USB descriptor scan code"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "PHOTOMETRICS_USB_VID" "PVCAM active USB VID gate code"
require_text "${driver_src_dir}/photometrics_pvcam.rs" '"usb_identity".into()' "PVCAM USB identity metadata code"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "configured and active USB PVCAM evidence plus verified vendor-runtime camera-name discovery, writable exposure setting, one-shot capture, repeated one-shot stream support, and runtime temperature read/setpoint control" "PVCAM support-level digest-state wording"
require_text "${driver_src_dir}/photometrics_pvcam.rs" 'writable_property("exposure", "Exposure", ValueType::TimeInterval)' "PVCAM writable exposure schema"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "PVCAM exposes only the evidenced writable exposure and temperature_setpoint properties" "PVCAM write-property evidence policy"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "PVCAM exposure must be positive" "PVCAM positive exposure validation"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "CapabilityKind::TemperatureControl" "PVCAM temperature-control capability"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "pl_get_param" "PVCAM parameter read ABI"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "pl_set_param" "PVCAM parameter write ABI"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "pl_exp_setup_seq/pl_exp_start_seq/pl_exp_check_status" "PVCAM vendor-runtime capture path"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "CapabilityKind::CameraCapture" "PVCAM CameraCapture code"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "CapabilityKind::CameraStream" "PVCAM CameraStream code"
require_text "${device_docs_dir}/photometrics-pvcam.md" "| \`CameraStream\` | Camera |" "PVCAM CameraStream documentation"
require_text "${device_docs_dir}/photometrics-pvcam.md" "reset/maintenance operations" "PVCAM hidden maintenance operation documentation"
require_text "${device_docs_dir}/photometrics-pvcam.md" "| \`exposure\` | Camera | \`TimeInterval\` | typed | R/W | positive interval used by vendor-runtime one-shot capture" "PVCAM writable exposure documentation"
require_text "${device_docs_dir}/photometrics-pvcam.md" "| \`vendor_runtime_abi_state\` | Hub | \`String\`" "PVCAM vendor-runtime ABI property"
require_text "${device_docs_dir}/photometrics-pvcam.md" "| \`vendor_runtime_discovery_state\` | Hub | \`String\`" "PVCAM vendor-runtime discovery property"
require_text "${device_docs_dir}/photometrics-pvcam.md" "| \`vendor_runtime_camera_names\` | Hub | \`List\`" "PVCAM vendor-runtime camera-name property"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "fn vendor_runtime_camera_discovery(&self) -> (String, Vec<String>)" "PVCAM vendor-runtime camera discovery code"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "pl_cam_get_name" "PVCAM runtime camera-name symbol"
require_text "$evidence_file" 'descriptor-only `os-usb` discovery for USB VID `0x1f12`' "PVCAM evidence active USB descriptor row"
require_text "$audit_root/data/third_party/pvcam/README.md" "third-party data" "PVCAM trimmed third-party package README"
require_text "$reverse_index_file" "Configured evidence plus runtime-package file-status/digest/loadability/ABI-symbol checks, camera-name discovery, writable exposure setting, one-shot capture, repeated one-shot stream support, and runtime temperature read/setpoint control exist" "PVCAM reverse-index ABI wording"
require_text "$audit_root/docs/inventory/micro_manager_protocol_inventory.md" "runtime-package file-status/digest/loadability/ABI-symbol checks" "camera inventory ABI package wording"
require_text "${device_docs_dir}/photometrics-pvcam.md" "expected PVCAM exported symbols" "PVCAM third-party ABI probe note"
require_text "${device_docs_dir}/photometrics-pvcam.md" 'verified vendor-runtime backend with `load_vendor_runtime=true`' "PVCAM explicit runtime loading note"
require_text "${device_docs_dir}/photometrics-pvcam.md" "After digest verification" "PVCAM third-party digest gate note"
require_text "$audit_root/data/third_party/pvcam/manifest.example.toml" "pvcam-runtime-package-file" "PVCAM runtime manifest example"
require_text "${device_docs_dir}/abs-camera.md" "| \`vendor_runtime_state\` | Camera | \`String\`" "ABS vendor-runtime state property"
require_text "${device_docs_dir}/mightex-camera.md" "| \`vendor_runtime_state\` | Camera | \`String\`" "Mightex camera vendor-runtime state property"
require_text "${device_docs_dir}/abs-camera.md" "| \`vendor_runtime_file_status\` | Camera | \`String\`" "ABS vendor-runtime file status property"
require_text "${device_docs_dir}/mightex-camera.md" "| \`vendor_runtime_file_status\` | Camera | \`String\`" "Mightex camera vendor-runtime file status property"
require_text "${device_docs_dir}/abs-camera.md" "| \`vendor_runtime_digest_state\` | Camera | \`String\`" "ABS vendor-runtime digest state property"
require_text "${device_docs_dir}/mightex-camera.md" "| \`vendor_runtime_digest_state\` | Camera | \`String\`" "Mightex camera vendor-runtime digest state property"
require_text "${device_docs_dir}/abs-camera.md" "| \`vendor_runtime_probe_state\` | Camera | \`String\`" "ABS vendor-runtime probe property"
require_text "${device_docs_dir}/abs-camera.md" "| \`vendor_runtime_abi_state\` | Camera | \`String\`" "ABS vendor-runtime ABI property"
require_text "${device_docs_dir}/mightex-camera.md" "| \`vendor_runtime_probe_state\` | Camera | \`String\`" "Mightex camera vendor-runtime probe property"
require_text "${device_docs_dir}/mightex-camera.md" "| \`vendor_runtime_abi_state\` | Camera | \`String\`" "Mightex camera vendor-runtime ABI property"
require_text "${device_docs_dir}/mightex-camera.md" "| \`exposure\` | Camera | \`TimeInterval\` | s | R/W | Non-negative interval" "Mightex camera writable exposure documentation"
require_text "${device_docs_dir}/mightex-camera.md" "| \`width\` | Camera | \`PixelCount\` | px | R/W | Positive frame width" "Mightex camera writable width documentation"
require_text "${driver_src_dir}/abs_camera.rs" "vendor_runtime_state" "ABS vendor-runtime state code"
require_text "${driver_src_dir}/mightex_camera.rs" "vendor_runtime_state" "Mightex camera vendor-runtime state code"
require_text "${driver_src_dir}/abs_camera.rs" "vendor_runtime_file_size" "ABS vendor-runtime file size code"
require_text "${driver_src_dir}/mightex_camera.rs" "vendor_runtime_file_size" "Mightex camera vendor-runtime file size code"
require_text "${driver_src_dir}/abs_camera.rs" "vendor_runtime_digest_state" "ABS vendor-runtime digest state code"
require_text "${driver_src_dir}/mightex_camera.rs" "vendor_runtime_digest_state" "Mightex camera vendor-runtime digest state code"
require_text "${driver_src_dir}/abs_camera.rs" "vendor_runtime_digest_allows_use" "ABS vendor-runtime digest gate code"
require_text "${driver_src_dir}/mightex_camera.rs" "vendor_runtime_digest_allows_use" "Mightex camera vendor-runtime digest gate code"
require_text "${driver_src_dir}/abs_camera.rs" "vendor_runtime_probe_state" "ABS vendor-runtime probe code"
require_text "${driver_src_dir}/mightex_camera.rs" "vendor_runtime_probe_state" "Mightex camera vendor-runtime probe code"
require_text "${driver_src_dir}/abs_camera.rs" "vendor_runtime_abi_state" "ABS vendor-runtime ABI code"
require_text "${driver_src_dir}/mightex_camera.rs" "vendor_runtime_abi_state" "Mightex camera vendor-runtime ABI code"
require_text "${driver_src_dir}/abs_camera.rs" "CamUSB_GetImage" "ABS ABI image symbol probe"
require_text "${driver_src_dir}/abs_camera.rs" "CamUSB_ReleaseImage" "ABS ABI release symbol probe"
require_text "${driver_src_dir}/abs_camera.rs" "CamUSB_TriggerImage" "ABS ABI trigger symbol probe"
require_text "${driver_src_dir}/abs_camera.rs" "CapabilityKind::CameraCapture" "ABS CameraCapture code"
require_text "${driver_src_dir}/abs_camera.rs" "CapabilityKind::CameraStream" "ABS CameraStream code"
require_text "${device_docs_dir}/abs-camera.md" "| \`CameraStream\` | Camera |" "ABS CameraStream documentation"
require_text "${driver_src_dir}/abs_camera.rs" "CamUSB_InitCameraExS/SetCaptureMode/TriggerImage/GetImage/ReleaseImage" "ABS capture backend metadata"
require_text "${driver_src_dir}/mightex_camera.rs" "BUFCCDUSB_InitDevice" "Mightex camera ABI init symbol probe"
require_text "${driver_src_dir}/mightex_camera.rs" "BUFCCDUSB_SetSoftTrigger" "Mightex camera ABI trigger symbol probe"
require_text "${driver_src_dir}/mightex_camera.rs" "fn validate_write_property(&self, key: &str, value: &Value)" "Mightex camera writable capture-parameter validation"
require_text "${driver_src_dir}/mightex_camera.rs" "\"bit_depth\" => {" "Mightex camera writable bit-depth validation"
require_text "${driver_src_dir}/mightex_camera.rs" "CapabilityKind::CameraCapture" "Mightex camera CameraCapture code"
require_text "${driver_src_dir}/mightex_camera.rs" "CapabilityKind::CameraStream" "Mightex camera CameraStream code"
require_text "${device_docs_dir}/mightex-camera.md" "| \`CameraStream\` | Camera |" "Mightex camera CameraStream documentation"
require_text "${driver_src_dir}/mightex_camera.rs" "BUFCCDUSB_InitDevice/StartFrameGrab/SetSoftTrigger callback" "Mightex camera capture backend metadata"
require_text "${driver_src_dir}/abs_camera.rs" "runtime-package evidence with file-status/digest/loadability/ABI-symbol checks" "ABS support-level digest-state wording"
require_text "${driver_src_dir}/mightex_camera.rs" "runtime-package evidence with file-status/digest/loadability/ABI-symbol checks" "Mightex camera support-level digest-state wording"
require_text "$evidence_file" "Reverse engineered evidence records the CamUSB SDK API surface" "ABS evidence API surface row"
require_text "$audit_root/data/third_party/abs-camera/README.md" "third-party data" "ABS trimmed third-party package README"
require_text "$evidence_gate_audit_file" "Runtime-package evidence with file-status/digest/loadability/ABI-symbol checks, writable exposure setting, explicit async software trigger, opt-in vendor-runtime capture, and repeated one-shot stream support" "ABS evidence-gate ABI wording"
require_text "$evidence_gate_audit_file" "Mightex buffered USB cameras | \`docs/reverse/mightex.md\`, \`docs/devices/mightex-camera.md\`, evidence register row, third-party runtime package note | \`numanager_drivers::mightex_camera\` is exported | Runtime-package evidence with file-status/digest/loadability/ABI-symbol checks" "Mightex camera evidence-gate ABI wording"
require_text "$audit_root/data/third_party/mightex-camera/README.md" "third-party data" "Mightex camera trimmed third-party package README"
require_text "${device_docs_dir}/abs-camera.md" "expected CamUSB exported symbols" "ABS third-party ABI probe note"
require_text "${device_docs_dir}/mightex-camera.md" "expected Mightex SDK exports" "Mightex camera third-party ABI probe note"
require_text "${device_docs_dir}/mightex-camera.md" "BUFCCDUSB_SetSoftTrigger" "Mightex camera third-party ABI trigger note"
require_text "${device_docs_dir}/abs-camera.md" "After digest verification" "ABS third-party digest gate note"
require_text "${device_docs_dir}/mightex-camera.md" "After digest verification" "Mightex camera third-party digest gate note"
require_text "${device_docs_dir}/abs-camera.md" 'Requires `load_vendor_runtime=true`' "ABS explicit runtime loading note"
require_text "${device_docs_dir}/mightex-camera.md" 'Requires `load_vendor_runtime=true`' "Mightex camera explicit runtime loading note"

require_text "$evidence_gate_audit_file" "Recorded output" "protocol evidence audit recorded-output policy"
require_text "$protocol_evidence_plan_file" "Clean-Room Spec Criteria" "protocol evidence clean-room criteria"
require_text "${reverse_docs_dir}/okolab-protocol.md" "Plain Frame Grammar" "Okolab recovered plain frame grammar"
require_text "${reverse_docs_dir}/okolab-protocol.md" "Checksum Frame Grammar" "Okolab recovered checksum frame grammar"
require_text "${reverse_docs_dir}/okolab-protocol.md" "hardware validation yet" "Okolab static-spec validation boundary"
require_text "$protocol_evidence_plan_file" "matching runtime output" "protocol evidence runtime-output requirement"
require_text "$protocol_evidence_plan_file" "hardware validation note" "protocol evidence hardware-validation requirement"
require_text "${reverse_docs_dir}/trace-capture-guide.md" "Console/runtime output" "trace guide runtime-output artifact row"
require_text "${reverse_docs_dir}/trace-capture-guide.md" "Hardware output/readback" "trace guide hardware-output artifact row"
require_text "${reverse_docs_dir}/trace-note-template.md" "Do not summarize the console output away" "trace note exact-output rule"
require_text "$example_outputs_file" "detected " "recorded discovery output"
require_text "$example_outputs_file" "Configured Toupcam camera Configured Toupcam geometry, 1 device(s), 2 resource(s)" "Toupcam configured discovery output"
require_text "$example_outputs_file" "device: Configured Toupcam geometry [\"camera\", \"trigger.sink\"]" "Toupcam configured descriptor output"
require_text "$example_outputs_file" "Simulated Cephla Squid controller, 25 device(s), 1 resource(s)" "Squid discovery device count"
require_text "$example_outputs_file" "device: squid-led-matrix [\"light.source\", \"illumination.matrix\"]" "Squid discovery LED matrix output"
require_text "$example_outputs_file" "device: squid-onboard-dac-8 [\"analog.output\"]" "Squid discovery onboard DAC output"
require_text "$example_outputs_file" "Configured GenICam node map Configured GenICam local node-map camera" "GenICam current configured discovery label"
require_text "$example_outputs_file" "source: platform" "platform camera selector output"
require_text "$example_outputs_file" "camera: platform-camera-fixture [\"camera\", \"platform.camera\", \"trigger.sink\", \"trigger.source\"]" "platform camera descriptor output"
require_text "$example_outputs_file" "frame: 1280x720 921600 bytes format=Mono8 metadata keys=[backend, exposure, frame_interval, gain, pixel_format]" "platform camera acquisition frame output"
require_text "$example_outputs_file" "drop_oldest stream completed: map keys=[frame, frames, height, pixel_format, stream, width] stream=StreamId(2) frames=Some(6) size=1280x720 format=Some(\"Mono8\")" "platform camera stream completion output"
require_text "$example_outputs_file" "selected stage: asi-ms2000-xy [axis.xy, stage.xy] axes=x,y" "generic motion XY stage output"
require_text "$example_outputs_file" "move completed for asi-ms2000-z: map keys=[mode, z]" "generic motion Z completion output"
require_text "$example_outputs_file" "selected stage: thorlabs-apt-axis-1 [axis.x, stage.x, motion.apt] axes=position" "generic motion single-axis output"
require_text "$example_outputs_file" "selected stage: esp32-xy [axis.xy] axes=x,y" "generic ESP32 motion output"
require_text "$example_outputs_file" "selected stage: chuo-qt-xy-stage [axis.xy, stage.xy, motion.stage] axes=x,y" "generic Chuo QT motion output"
require_text "$example_outputs_file" "selected stage: marzhauser-xy-stage [axis.xy, stage.xy] axes=x,y" "generic Marzhauser motion output"
require_text "${driver_src_dir}/marzhauser.rs" "protocol::execute_probe_script(&mut serial, 4)" "Marzhauser construction-time configured startup readback"
require_text "${driver_src_dir}/marzhauser.rs" "fn refresh_property_readback(&mut self, device: DeviceId, key: &str) -> Result<()> {" "Marzhauser runtime readback helper"
require_text "${driver_src_dir}/marzhauser.rs" "fn refresh_xy_motion_readback(&mut self) -> Result<()> {" "Marzhauser home/stop XY readback helper"
require_text "${driver_src_dir}/marzhauser.rs" '(device, "last_error") | (device, "fault") if device == self.hub' "Marzhauser error readback refresh"
require_text "${driver_src_dir}/marzhauser.rs" "Marzhauser GenericCommand supports refresh_readbacks, refresh_identity, refresh_status, refresh_position, refresh_profiles, and refresh_limits" "Marzhauser mapped GenericCommand validation"
require_text "${driver_src_dir}/marzhauser.rs" "capability(2, device, CapabilityKind::StageHome)" "Marzhauser advertised StageHome"
require_text "${driver_src_dir}/marzhauser.rs" '"serial_port".into()' "Marzhauser serial-port resource metadata"
require_text "${driver_src_dir}/marzhauser.rs" '"connected".into(), Value::Bool(self.connected)' "Marzhauser connected resource metadata"
require_text "${device_docs_dir}/marzhauser.md" "| \`last_error\` | Hub | \`String\`" "Marzhauser documented error readback"
require_text "${device_docs_dir}/marzhauser.md" '| `StageHome` | XY/Z | `None` | Calibration status string | Writes documented `!cal` commands for X/Y or Z' "Marzhauser documented StageHome"
require_text "${device_docs_dir}/marzhauser.md" '| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_identity`, `refresh_status`, `refresh_position`, `refresh_profiles`, or `refresh_limits` with no params' "Marzhauser documented mapped GenericCommand"
require_text "${device_docs_dir}/marzhauser.md" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "Marzhauser documented resource metadata"
require_text "$evidence_file" 'move, home/calibrate, and stop capabilities use documented command paths' "Marzhauser evidence home support wording"
require_text "$evidence_file" 'runtime reads, hub GenericCommand refresh helpers, and home/stop paths refresh corresponding query-backed state including `?err` as `last_error`/`fault` when replies are available' "Marzhauser evidence readback wording"
require_text "${driver_src_dir}/pi_gcs.rs" '(device, "last_error") | (device, "fault") if device == self.hub' "PI GCS error readback refresh"
require_text "${driver_src_dir}/pi_gcs.rs" "refresh_error_readback(true)" "PI GCS write-path error readback"
require_text "${driver_src_dir}/pi_gcs.rs" "fn refresh_property_readback(&mut self, device: DeviceId, key: &str) -> Result<()> {" "PI GCS runtime readback helper"
require_text "${driver_src_dir}/pi_gcs.rs" "fn refresh_xy_motion_readback(&mut self) -> Result<()> {" "PI GCS motion readback helper"
require_text "${driver_src_dir}/pi_gcs.rs" "PI GCS GenericCommand supports refresh_readbacks, refresh_identity, refresh_status, refresh_position, refresh_profiles, and refresh_servo" "PI GCS mapped GenericCommand validation"
require_text "${driver_src_dir}/pi_gcs.rs" '"serial_port".into()' "PI GCS serial-port resource metadata"
require_text "${driver_src_dir}/pi_gcs.rs" '"connected".into(), Value::Bool(self.connected)' "PI GCS connected resource metadata"
require_text "${device_docs_dir}/pi-gcs.md" "| \`last_error\` | Hub | \`String\`" "PI GCS documented error readback"
require_text "${device_docs_dir}/pi-gcs.md" '| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_identity`, `refresh_status`, `refresh_position`, `refresh_profiles`, or `refresh_servo` with no params' "PI GCS documented mapped GenericCommand"
require_text "${device_docs_dir}/pi-gcs.md" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "PI GCS documented resource metadata"
require_text "$evidence_file" 'runtime property reads and hub GenericCommand refresh helpers ingest `ERR?`, moving-state, position, velocity, acceleration, and servo readbacks' "PI GCS evidence generic readback wording"
require_text "$evidence_file" 'writable motion, reference, and stop paths request mapped busy, position, and `ERR?` readbacks after controller commands' "PI GCS evidence motion readback wording"
require_text "${driver_src_dir}/prior.rs" "protocol::execute_probe_script(&mut serial, 4)" "Prior construction-time configured startup readback"
require_text "${driver_src_dir}/prior.rs" "fn read_optional_ack(&mut self) -> Result<bool> {" "Prior write acknowledgement readback"
require_text "${driver_src_dir}/prior.rs" "Prior GenericCommand supports refresh_readbacks, refresh_identity, refresh_status, refresh_position, refresh_profiles, and refresh_outputs" "Prior mapped GenericCommand validation"
require_text "${driver_src_dir}/prior.rs" '"serial_port".into()' "Prior serial-port resource metadata"
require_text "${driver_src_dir}/prior.rs" '"connected".into(), Value::Bool(self.connected)' "Prior connected resource metadata"
require_text "${device_docs_dir}/prior.md" "| \`last_ack\` | Hub | \`String\`" "Prior documented acknowledgement readback"
require_text "${device_docs_dir}/prior.md" '| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_identity`, `refresh_status`, `refresh_position`, `refresh_profiles`, or `refresh_outputs` with no params' "Prior documented mapped GenericCommand"
require_text "${device_docs_dir}/prior.md" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "Prior documented resource metadata"
require_text "$evidence_file" "runtime property reads and hub GenericCommand refresh helpers request mapped firmware-date, status, position, XY profile, shutter, and TTL readbacks" "Prior evidence generic readback wording"
require_text "${driver_src_dir}/sutter_stage.rs" "protocol::execute_probe_script(&mut serial, &configured.probe, 4)" "SutterStage construction-time configured startup readback"
require_text "${driver_src_dir}/sutter_stage.rs" "fn refresh_property_readback" "SutterStage write-path readback helper"
require_text "${driver_src_dir}/sutter_stage.rs" "SutterStage GenericCommand supports refresh_readbacks, refresh_identity, refresh_status, refresh_position, and refresh_profiles" "SutterStage mapped GenericCommand validation"
require_text "${driver_src_dir}/sutter_stage.rs" "capability(2, device, CapabilityKind::StageHome)" "SutterStage advertised XY StageHome"
require_text "${driver_src_dir}/sutter_stage.rs" '"baud_rate".into(), Value::I64(self.baud_rate as i64)' "SutterStage baud-rate resource metadata"
require_text "${driver_src_dir}/sutter_stage.rs" '"serial_port".into()' "SutterStage serial-port resource metadata"
require_text "${driver_src_dir}/sutter_stage.rs" '"connected".into(), Value::Bool(self.connected)' "SutterStage connected resource metadata"
require_text "${device_docs_dir}/sutter-stage.md" "Configured discovery/resource metadata" "SutterStage documented resource metadata"
require_text "$evidence_file" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "SutterStage evidence resource metadata wording"
require_text "${device_docs_dir}/sutter-stage.md" "Writable transmission delay, XY position, XY speed," "SutterStage documented write-path readback"
require_text "${device_docs_dir}/sutter-stage.md" '| `StageHome` | XY | `None` | Status string plus property events | Sends documented `HOME X Y`' "SutterStage documented StageHome"
require_text "${device_docs_dir}/sutter-stage.md" '| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_identity`, `refresh_status`, `refresh_position`, or `refresh_profiles` with no params' "SutterStage documented mapped GenericCommand"
require_text "$evidence_file" "module reset and XY origin commands remain hidden" "SutterStage hidden reset/origin wording"
require_text "$evidence_file" "runtime reads, hub GenericCommand refresh helpers, and writable transmission-delay, XY position/speed/start-speed/acceleration, XY home, Z position, and stop paths request mapped query readbacks" "SutterStage evidence readback wording"
require_text "${driver_src_dir}/sutter_mp285.rs" "protocol::execute_probe_script(&mut serial, &configured.probe, 4)" "Sutter MP-285 construction-time configured startup readback"
require_text "${driver_src_dir}/sutter_mp285.rs" "fn read_optional_ack(&mut self) -> Result<bool> {" "Sutter MP-285 optional ACK readback"
require_text "${driver_src_dir}/sutter_mp285.rs" "fn refresh_motion_readback(&mut self) -> Result<()> {" "Sutter MP-285 motion readback helper"
require_text "${driver_src_dir}/sutter_mp285.rs" "MP-285 GenericCommand supports refresh_readbacks, refresh_status, and refresh_position" "Sutter MP-285 mapped GenericCommand validation"
require_text "${driver_src_dir}/sutter_mp285.rs" '"baud_rate".into(), Value::I64(self.baud_rate as i64)' "Sutter MP-285 baud-rate metadata"
require_text "${driver_src_dir}/sutter_mp285.rs" '"serial_port".into()' "Sutter MP-285 serial-port resource metadata"
require_text "${driver_src_dir}/sutter_mp285.rs" '"connected".into(), Value::Bool(self.connected)' "Sutter MP-285 connected resource metadata"
require_text "${device_docs_dir}/sutter-mp285.md" '| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_status`, or `refresh_position` with no params' "Sutter MP-285 documented mapped GenericCommand"
require_text "${device_docs_dir}/sutter-mp285.md" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "Sutter MP-285 documented resource metadata"
reject_text "${device_docs_dir}/sutter-mp285.md" '| `StageHome` |' "Sutter MP-285 origin command hidden from documented capabilities"
reject_text "${driver_src_dir}/sutter_mp285.rs" "capability(2, device, CapabilityKind::StageHome)" "Sutter MP-285 origin command hidden from advertised capabilities"
require_text "$evidence_file" "runtime property reads and hub GenericCommand refresh helpers ingest status and XYZ position readbacks" "Sutter MP-285 evidence generic readback wording"
require_text "$evidence_file" "reset and current-position-as-origin remain hidden" "Sutter MP-285 hidden reset/origin wording"
require_text "$evidence_file" "move and stop paths consume optional ACK/error bytes and request status/position readbacks when replies are available" "Sutter MP-285 evidence motion readback wording"
require_text "$evidence_file" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "Sutter MP-285 evidence resource metadata wording"
require_text "${driver_src_dir}/pi_gcs.rs" "protocol::execute_probe_script(&mut serial, &configured.probe, 4)" "PI GCS construction-time configured startup readback"
require_text "${driver_src_dir}/cobolt.rs" "protocol::execute_probe_script(&mut serial, 4)" "Cobolt construction-time configured startup readback"
require_text "${driver_src_dir}/cobolt.rs" '"serial_port".into()' "Cobolt serial-port resource metadata"
require_text "${driver_src_dir}/cobolt.rs" '"connected".into(), Value::Bool(self.connected)' "Cobolt connected resource metadata"
require_text "${device_docs_dir}/cobolt.md" 'configured-startup readback metadata, and resource metadata for configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "Cobolt documented resource metadata"
require_text "$evidence_file" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "Cobolt evidence resource metadata wording"
require_text "${driver_src_dir}/cobolt.rs" "fn generic_refresh_property" "Cobolt mapped generic refresh mapping"
require_text "${driver_src_dir}/cobolt.rs" "Cobolt GenericCommand supports refresh_telemetry, refresh_enabled, refresh_power, refresh_actual_power, refresh_current, refresh_control_mode, refresh_autostart, refresh_interlock, refresh_fault, refresh_hours" "Cobolt mapped generic command validation"
require_text "${driver_src_dir}/cobolt.rs" "Cobolt GenericCommand refresh commands do not accept params" "Cobolt generic refresh params gate"
require_text "${device_docs_dir}/cobolt.md" '`refresh_telemetry`, `refresh_enabled`, `refresh_power`, `refresh_actual_power`, `refresh_current`, `refresh_control_mode`, `refresh_autostart`, `refresh_interlock`, `refresh_fault`, or `refresh_hours` with no params' "Cobolt documented mapped generic command"
require_text "$evidence_file" 'runtime `GenericCommand` is constrained to documented query-backed `refresh_telemetry`, `refresh_enabled`, `refresh_power`, `refresh_actual_power`, `refresh_current`, `refresh_control_mode`, `refresh_autostart`, `refresh_interlock`, `refresh_fault`, and `refresh_hours` helpers with no params' "Cobolt evidence mapped GenericCommand wording"
require_text "${driver_src_dir}/coherent_obis.rs" "protocol::execute_probe_script(&mut serial, configured.probe.index, 4)" "Coherent OBIS construction-time configured startup readback"
require_text "${driver_src_dir}/coherent_obis.rs" '"serial_port".into()' "Coherent OBIS serial-port resource metadata"
require_text "${driver_src_dir}/coherent_obis.rs" '"connected".into(), Value::Bool(self.connected)' "Coherent OBIS connected resource metadata"
require_text "${device_docs_dir}/coherent-obis.md" 'configured-startup readback metadata for configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "Coherent OBIS documented resource metadata"
require_text "$evidence_file" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "Coherent OBIS evidence resource metadata wording"
require_text "${driver_src_dir}/coherent_obis.rs" "fn confirm_write_readback(&mut self, commands: &[protocol::ObisCommand]) -> Result<()> {" "Coherent OBIS write-confirmation readback"
require_text "${driver_src_dir}/coherent_obis.rs" "Coherent OBIS GenericCommand supports refresh_telemetry, refresh_identity, refresh_power, refresh_status, and refresh_limits" "Coherent OBIS mapped GenericCommand validation"
require_text "${device_docs_dir}/coherent-obis.md" "Writable emission, power, analog-modulation, mode, and CDRH-delay paths request" "Coherent OBIS documented write-confirmation readback"
require_text "${device_docs_dir}/coherent-obis.md" '| `GenericCommand` | Laser | `refresh_telemetry`, `refresh_identity`, `refresh_power`, `refresh_status`, or `refresh_limits` with no params | Refreshed telemetry map | Uses only mapped OBIS query readbacks; no arbitrary serial command, communication setup, or error-clear surface | Not sequenceable |' "Coherent OBIS documented mapped GenericCommand"
require_text "$evidence_file" 'laser `GenericCommand` is constrained to mapped telemetry, identity, power, status, and power-limit refresh helpers with no raw serial, communication setup, or error-clear surface' "Coherent OBIS evidence mapped GenericCommand wording"
require_text "${driver_src_dir}/omicron.rs" "protocol::execute_probe_script(&mut serial, 4)" "Omicron construction-time configured startup readback"
require_text "${driver_src_dir}/omicron.rs" '"serial_port".into()' "Omicron serial-port resource metadata"
require_text "${driver_src_dir}/omicron.rs" '"connected".into(), Value::Bool(self.connected)' "Omicron connected resource metadata"
require_text "${device_docs_dir}/omicron.md" 'configured-startup readback metadata, and resource metadata for configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "Omicron documented resource metadata"
require_text "$evidence_file" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "Omicron evidence resource metadata wording"
require_text "${driver_src_dir}/omicron.rs" "fn confirm_write_readback(&mut self, commands: &[protocol::OmicronCommand]) -> Result<()> {" "Omicron write-confirmation readback"
require_text "${driver_src_dir}/omicron.rs" "Omicron GenericCommand supports refresh_telemetry, refresh_identity, refresh_power, refresh_status, and refresh_temperatures" "Omicron mapped GenericCommand validation"
require_text "${device_docs_dir}/omicron.md" "Writable emission, power, relative-power, operating-mode, and modulation paths" "Omicron documented write-confirmation readback"
  require_text "${device_docs_dir}/omicron.md" '| `GenericCommand` | Laser | `refresh_telemetry`, `refresh_identity`, `refresh_power`, `refresh_status`, or `refresh_temperatures` with no params | Refreshed telemetry map | Uses only mapped Omicron query readbacks; no arbitrary serial command surface; fault reset remains hidden from regular and advanced command surfaces | Not sequenceable |' "Omicron documented mapped GenericCommand"
require_text "$evidence_file" 'laser `GenericCommand` is constrained to mapped telemetry, identity, power, status, and temperature refresh helpers with no raw serial or fault-reset side-effect surface' "Omicron evidence mapped GenericCommand wording"
require_text "${driver_src_dir}/thorlabs_kurios.rs" "protocol::execute_probe_script(&mut serial, 4)" "Thorlabs KURIOS construction-time configured startup readback"
require_text "${driver_src_dir}/thorlabs_kurios.rs" "fn refresh_property_readback" "Thorlabs KURIOS write-path readback helper"
require_text "${driver_src_dir}/thorlabs_kurios.rs" "KURIOS GenericCommand supports refresh_telemetry, refresh_identity, refresh_wavelength, refresh_bandwidth, refresh_output, and refresh_status" "Thorlabs KURIOS mapped GenericCommand validation"
require_text "${driver_src_dir}/thorlabs_kurios.rs" '"serial_port".into()' "Thorlabs KURIOS serial-port resource metadata"
require_text "${driver_src_dir}/thorlabs_kurios.rs" '"connected".into(), Value::Bool(self.connected)' "Thorlabs KURIOS connected resource metadata"
require_text "${device_docs_dir}/thorlabs-kurios.md" "Writable wavelength, bandwidth, output, and trigger-mode properties" "Thorlabs KURIOS documented write-path readback"
require_text "${device_docs_dir}/thorlabs-kurios.md" '| `GenericCommand` | Tunable filter | `refresh_telemetry`, `refresh_identity`, `refresh_wavelength`, `refresh_bandwidth`, `refresh_output`, or `refresh_status` with no params' "Thorlabs KURIOS documented mapped GenericCommand"
require_text "${device_docs_dir}/thorlabs-kurios.md" 'configured `serial_port`, `serial_timeout`, and `connected` state' "Thorlabs KURIOS documented resource metadata"
require_text "$evidence_file" "runtime reads, writable wavelength/bandwidth/output/trigger-mode paths, and named GenericCommand refresh helpers request mapped query readbacks" "Thorlabs KURIOS evidence readback wording"
require_text "$evidence_file" 'resource metadata records configured `serial_port`, `serial_timeout`, and `connected` state' "Thorlabs KURIOS evidence resource metadata wording"
require_text "${driver_src_dir}/thorlabs_apt.rs" "protocol::execute_probe_script(&mut serial, probe.channel, 4)" "Thorlabs APT construction-time configured startup readback"
require_text "${driver_src_dir}/thorlabs_apt.rs" "fn read_expected_frame_if_available(&mut self, expected: u16) -> Result<()> {" "Thorlabs APT property-read frame ingestion"
require_text "${driver_src_dir}/thorlabs_apt.rs" "fn refresh_motion_readback(&mut self) -> Result<()> {" "Thorlabs APT motion readback helper"
require_text "${driver_src_dir}/thorlabs_apt.rs" "Thorlabs APT GenericCommand supports refresh_telemetry, refresh_identity, refresh_position, refresh_status, refresh_velocity_profile, and keep_alive" "Thorlabs APT mapped GenericCommand validation"
require_text "${driver_src_dir}/thorlabs_apt.rs" "self.send(protocol::AptCommand::KeepAlive)?" "Thorlabs APT keep-alive GenericCommand"
require_text "${driver_src_dir}/thorlabs_apt.rs" '"serial_port".into()' "Thorlabs APT serial-port resource metadata"
require_text "${driver_src_dir}/thorlabs_apt.rs" '"connected".into(), Value::Bool(self.connected)' "Thorlabs APT connected resource metadata"
require_text "${device_docs_dir}/thorlabs-apt.md" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "Thorlabs APT documented resource metadata"
require_text "${device_docs_dir}/thorlabs-apt.md" "Runtime property reads request and ingest the matching hardware-info, position," "Thorlabs APT documented property-read frame ingestion"
require_text "${device_docs_dir}/thorlabs-apt.md" '| `GenericCommand` | Axis | `refresh_telemetry`, `refresh_identity`, `refresh_position`, `refresh_status`, `refresh_velocity_profile`, or `keep_alive` with no params' "Thorlabs APT documented mapped GenericCommand"
require_text "$evidence_file" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "Thorlabs APT evidence resource metadata wording"
require_text "$evidence_file" "runtime property reads, motion/home/stop/velocity-profile write paths, and named GenericCommand refresh helpers request and ingest hardware-info, position, status, or velocity-profile frames when available" "Thorlabs APT evidence write-path readback wording"
require_text "$evidence_file" 'mapped `keep_alive` sends only the encoded APT keep-alive frame and makes no status-streaming claim' "Thorlabs APT evidence keep-alive wording"
require_text "${driver_src_dir}/asi.rs" "protocol::execute_ms2000_probe_script(&mut serial, 4)" "ASI MS-2000 construction-time configured startup readback"
require_text "${driver_src_dir}/asi.rs" "protocol::execute_tiger_probe_script(&mut serial, &configured.probe, 4)" "ASI Tiger construction-time configured startup readback"
require_text "${driver_src_dir}/asi.rs" "fn refresh_readback(&mut self, command: &protocol::AsiCommand)" "ASI MS-2000 runtime readback helper"
require_text "${driver_src_dir}/asi.rs" "fn refresh_readback(&mut self, command: &protocol::TigerCommand)" "ASI Tiger runtime readback helper"
require_text "${driver_src_dir}/asi.rs" "AsiCommand::Here" "ASI HERE command encoder"
reject_text "${driver_src_dir}/asi.rs" '"set_here"' "ASI HERE command hidden from GenericCommand"
require_text "${driver_src_dir}/asi.rs" '"refresh_readbacks"' "ASI mapped refresh-readbacks generic command"
require_text "${driver_src_dir}/asi.rs" '"refresh_identity"' "ASI mapped refresh-identity generic command"
require_text "${driver_src_dir}/asi.rs" '"refresh_status"' "ASI mapped refresh-status generic command"
require_text "${driver_src_dir}/asi.rs" '"refresh_position"' "ASI mapped refresh-position generic command"
require_text "${driver_src_dir}/asi.rs" '"refresh_crisp"' "ASI Tiger mapped CRISP refresh command"
require_text "${driver_src_dir}/asi.rs" "command_no_params(\"ASI" "ASI refresh commands reject params"
require_text "${driver_src_dir}/asi.rs" '"baud_rate".into(), Value::I64(self.baud_rate as i64)' "ASI baud-rate metadata"
require_text "${driver_src_dir}/asi.rs" '"serial_port".into()' "ASI serial-port resource metadata"
require_text "${driver_src_dir}/asi.rs" '"connected".into(), Value::Bool(self.connected)' "ASI connected resource metadata"
require_text "${device_docs_dir}/asi.md" "Runtime property reads request and ingest the mapped query reply" "ASI documented runtime readback"
require_text "${device_docs_dir}/asi.md" "not exposed through" "ASI documented HERE hidden boundary"
require_text "${device_docs_dir}/asi.md" "do not expose arbitrary serial command strings" "ASI documented mapped refresh boundary"
require_text "${device_docs_dir}/asi.md" "Configured discovery/resource metadata" "ASI documented resource metadata"
require_text "$evidence_file" "runtime reads request and ingest mapped identity, position, busy, and Tiger CRISP query replies" "ASI evidence runtime readback wording"
require_text "$evidence_file" '`HERE` coordinate-reference updates are retained only as internal protocol operations' "ASI evidence HERE hidden wording"
require_text "$evidence_file" "hub \`GenericCommand\` exposes named no-parameter readback refresh helpers" "ASI evidence named refresh wording"
require_text "$evidence_file" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "ASI evidence resource metadata wording"
require_text "${driver_src_dir}/triggerscope.rs" "protocol::TriggerScopeCommand::Identify" "TriggerScope construction-time active identification"
require_text "${driver_src_dir}/triggerscope.rs" "protocol::TriggerScopeCommand::ProgramTtl" "TriggerScope sequence programming command surface"
require_text "${driver_src_dir}/triggerscope.rs" "unsupported TriggerScope capability invocation" "TriggerScope dispatch fails closed for unsupported capability invocation"
require_text "${driver_src_dir}/triggerscope.rs" '"serial_port".into()' "TriggerScope serial-port resource metadata"
require_text "${driver_src_dir}/triggerscope.rs" '"connected".into()' "TriggerScope connected resource metadata"
require_text "${device_docs_dir}/triggerscope.md" 'resource metadata records configured `serial_port`, fixed `baud_rate`, `serial_timeout`, and `connected` state' "TriggerScope documented resource metadata"
require_text "$evidence_file" 'resource metadata records configured `serial_port`, fixed `baud_rate`, `serial_timeout`, and `connected` state' "TriggerScope evidence resource metadata wording"
require_text "${device_docs_dir}/triggerscope.md" "constrained hub commands and public timing-plan APIs" "TriggerScope constrained sequence-programming documentation"
require_text "${device_docs_dir}/marzhauser.md" "runs the probe script before adding the driver" "Marzhauser active-probe docs"
require_text "$example_outputs_file" "selected stage: openuc2-xy [axis.xy] axes=x,y" "generic OpenUC2 motion output"
require_text "${driver_src_dir}/openuc2.rs" "OpenUc2Driver::serial" "OpenUC2 configured real serial constructor"
require_text "${driver_src_dir}/openuc2.rs" "fn refresh_startup_state(&mut self, timeout_ms: u64) -> Result<()> {" "OpenUC2 startup state readback"
require_text "${driver_src_dir}/openuc2.rs" "OpenUC2 GenericCommand supports refresh_state" "OpenUC2 mapped GenericCommand validation"
require_text "${driver_src_dir}/openuc2.rs" "\"wavelength\"" "OpenUC2 typed wavelength property code"
require_text "${driver_src_dir}/openuc2.rs" 'position_config_um(device, "x_travel", "x_travel_um")' "OpenUC2 canonical x_travel config"
require_text "${driver_src_dir}/openuc2.rs" 'position_config_um(device, "y_travel", "y_travel_um")' "OpenUC2 canonical y_travel config"
require_text "${driver_src_dir}/openuc2.rs" 'position_config_um(device, "z_travel", "z_travel_um")' "OpenUC2 canonical z_travel config"
require_text "${driver_src_dir}/openuc2.rs" "\"serial_port\".into()" "OpenUC2 serial resource metadata code"
require_text "${driver_src_dir}/openuc2.rs" '"baud_rate".into(), Value::I64(self.baud_rate as i64)' "OpenUC2 resource baud-rate metadata"
require_text "${driver_src_dir}/openuc2.rs" '"connected".into(), Value::Bool(self.connected)' "OpenUC2 resource connection metadata"
require_text "${device_docs_dir}/openuc2.md" "Config-backed JSON-line protocol plus opt-in configured real serial" "OpenUC2 documented configured serial support"
require_text "${device_docs_dir}/openuc2.md" '| `GenericCommand` | Hub | `refresh_state` with no params' "OpenUC2 documented mapped GenericCommand"
require_text "${device_docs_dir}/openuc2.md" "Configured laser metadata exposed as a typed property" "OpenUC2 wavelength property documentation"
require_text "${device_docs_dir}/openuc2.md" "\`x_travel\`, \`y_travel\`, \`z_travel\`" "OpenUC2 canonical travel config docs"
require_text "$evidence_file" "mapped \`refresh_state\` GenericCommand helper also uses the same JSON command path" "OpenUC2 evidence mapped GenericCommand wording"
require_text "$evidence_file" 'connected construction sends `/state_get` and ingests startup state before registration' "OpenUC2 evidence startup readback wording"
require_text "$evidence_file" "typed wavelength metadata, and configured serial-resource metadata" "OpenUC2 evidence metadata wording"
require_text "$example_outputs_file" "selected stage: pi-gcs-xy-stage [axis.xy, stage.xy] axes=x,y" "generic PI GCS motion output"
require_text "$example_outputs_file" "selected stage: corvus-xy-stage [axis.xy, stage.xy, motion.stage] axes=x,y" "generic Corvus motion output"
require_text "$example_outputs_file" "selected stage: openstage-xy [axis.xy, stage.xy, motion.stage] axes=x,y" "generic OpenStage motion output"
require_text "${driver_src_dir}/openstage.rs" "driver.read_information()?" "OpenStage construction-time controller information readback"
require_text "${driver_src_dir}/openstage.rs" '"serial_port".into()' "OpenStage serial-port resource metadata"
require_text "${driver_src_dir}/openstage.rs" '"connected".into(), Value::Bool(self.connected)' "OpenStage connected resource metadata"
require_text "$evidence_file" 'resource metadata records configured `serial_port` and `connected` state' "OpenStage evidence resource metadata wording"
require_text "${driver_src_dir}/squid.rs" "\"serial_port\".into()" "Squid serial resource metadata code"
require_text "${driver_src_dir}/squid.rs" "SET_ILLUMINATION_LED_MATRIX" "Squid LED matrix descriptor metadata"
require_text "${driver_src_dir}/squid.rs" "ANALOG_WRITE_ONBOARD_DAC" "Squid onboard DAC descriptor metadata"
require_text "${driver_src_dir}/squid.rs" "fn set_led_matrix" "Squid LED matrix frame builder"
require_text "${driver_src_dir}/squid.rs" "fn onboard_dac_write" "Squid onboard DAC frame builder"
require_text "${driver_src_dir}/squid.rs" "Squid hub GenericCommand supports disable_all_ports and heartbeat" "Squid mapped hub GenericCommand validation"
require_text "${driver_src_dir}/squid.rs" "Squid filter GenericCommand has no public aliases" "Squid hidden filter maintenance GenericCommand validation"
require_text "${driver_src_dir}/squid.rs" "fn ingest_available_status_frames(&mut self) -> Result<()> {" "Squid construction-time status-frame ingestion"
require_text "${driver_src_dir}/squid.rs" "fn apply_status_frame(&mut self, status: &SquidStatusFrame)" "Squid cached decoded status state"
require_text "${driver_src_dir}/squid.rs" '"last_status_command_id".into()' "Squid status-frame resource metadata"
require_text "${device_docs_dir}/squid.md" "resource metadata records configured \`serial_port\`, \`baud_rate\`, \`connected\` state, and last decoded status-frame metadata" "Squid serial resource metadata documentation"
require_text "${device_docs_dir}/squid.md" '| `squid-led-matrix` | `light.source`, `illumination.matrix` | LED matrix pattern/color command on the shared controller |' "Squid documented LED matrix device"
require_text "${device_docs_dir}/squid.md" '| `raw_counts` | Onboard DAC channels | `I64` | counts | R/W | 0..65535 | No | `ANALOG_WRITE_ONBOARD_DAC` channel/count payload |' "Squid documented onboard DAC raw counts"
require_text "${device_docs_dir}/squid.md" "Direct arbitrary \`SET_PIN_LEVEL\` access remains intentionally" "Squid documented raw pin policy"
require_text "${device_docs_dir}/squid.md" '| `GenericCommand` | Hub | `disable_all_ports` or `heartbeat` with no params' "Squid documented hub GenericCommand aliases"
reject_text "${device_docs_dir}/squid.md" '`zero_position` with no params' "Squid reset-like maintenance aliases hidden from docs"
require_text "$evidence_file" "configured serial-resource metadata" "Squid evidence serial metadata wording"
require_text "$evidence_file" "LED matrix pattern/color" "Squid evidence LED matrix wording"
require_text "$evidence_file" "diagnostic raw onboard DAC counts" "Squid evidence onboard DAC wording"
require_text "${driver_src_dir}/openstage.rs" "driver.read_position()?" "OpenStage construction-time position readback"
require_text "${driver_src_dir}/openstage.rs" "driver.read_step_size()?" "OpenStage construction-time step-size readback"
require_text "${driver_src_dir}/openstage.rs" "driver.read_velocity()?" "OpenStage construction-time velocity readback"
require_text "${driver_src_dir}/openstage.rs" "driver.read_acceleration()?" "OpenStage construction-time acceleration readback"
require_text "${driver_src_dir}/openstage.rs" "fn refresh_position_after_motion(&mut self, action: &str) -> Result<()> {" "OpenStage post-motion position readback helper"
reject_text "${driver_src_dir}/openstage.rs" '"zero_position" => {' "OpenStage zero-position GenericCommand is hidden"
require_text "${driver_src_dir}/openstage.rs" "OpenStage hub GenericCommand supports read_information, read_velocity, read_acceleration, and beep" "OpenStage mapped hub generic command validation"
require_text "${driver_src_dir}/openstage.rs" "OpenStageCommand::SetVelocity" "OpenStage velocity write command"
require_text "${driver_src_dir}/openstage.rs" "OpenStageCommand::SetAcceleration" "OpenStage acceleration write command"
require_text "${device_docs_dir}/openstage.md" '| `GenericCommand` | `openstage-hub` | `read_information`, `read_velocity`, `read_acceleration`, or `beep`' "OpenStage documented hub command"
require_text "$evidence_file" 'runtime absolute/relative moves request `p` position readback after command completion' "OpenStage evidence post-motion readback wording"
require_text "$evidence_file" 'StageMove profiles apply velocity/acceleration through the documented settings commands before motion' "OpenStage evidence profile settings wording"
require_text "$example_outputs_file" "standa-8smc4-x position: Position(Position { value: 250.0, unit: Micrometers })" "generic Standa motion readback output"
require_text "${driver_src_dir}/standa.rs" "query_serial_number(&mut serial, &mut probe)?" "Standa construction-time serial-number readback"
require_text "${driver_src_dir}/standa.rs" "query_position(&mut serial, &mut probe)?" "Standa construction-time position readback"
require_text "${driver_src_dir}/standa.rs" "query_status(&mut serial, &mut probe)?" "Standa construction-time status readback"
require_text "${driver_src_dir}/standa.rs" "query_move_settings(&mut serial, &mut probe)?" "Standa construction-time movement-settings readback"
require_text "${driver_src_dir}/standa.rs" "driver.refresh_engine_settings_once()?" "Standa construction-time engine-settings readback"
require_text "${driver_src_dir}/standa.rs" "driver.refresh_brake_settings_once()?" "Standa construction-time brake-settings readback"
require_text "${driver_src_dir}/standa.rs" "driver.refresh_home_settings_once()?" "Standa construction-time home-settings readback"
require_text "${driver_src_dir}/standa.rs" 'endpoint.connect' "Standa explicit connect gate"
require_text "${driver_src_dir}/standa.rs" ".stop_bits(serialport::StopBits::Two)" "Standa 8N2 live serial stop bits"
require_text "${driver_src_dir}/standa.rs" '"baud_rate".into(), Value::I64(self.baud_rate as i64)' "Standa baud-rate resource metadata"
require_text "${driver_src_dir}/standa.rs" '"serial_port".into()' "Standa serial-port resource metadata"
require_text "${driver_src_dir}/standa.rs" '"connected".into(), Value::Bool(self.connected)' "Standa connected resource metadata"
require_text "${driver_src_dir}/standa.rs" "StandaCommand::SetMoveSettings" "Standa movement-settings write command"
require_text "${driver_src_dir}/standa.rs" "fn refresh_position_once(&mut self) -> Result<()> {" "Standa position readback helper"
require_text "${driver_src_dir}/standa.rs" "fn wait_until_idle(&mut self, target_um: Option<f64>, timeout: Duration) -> Result<()> {" "Standa move/home status-poll helper"
require_text "${driver_src_dir}/standa.rs" "Standa GenericCommand supports refresh_readbacks, refresh_position, refresh_status, refresh_move_settings, refresh_engine_settings, refresh_brake_settings, refresh_home_settings, and refresh_static_settings" "Standa mapped GenericCommand validation"
require_text "${driver_src_dir}/standa.rs" '"refresh_readbacks" => Ok(vec![' "Standa refresh-readbacks grouped command"
require_text "${driver_src_dir}/standa.rs" '"alarm") if device == self.axis' "Standa alarm status property"
require_text "${driver_src_dir}/standa.rs" '"security_flags") if device == self.axis' "Standa security-flags status property"
require_text "${driver_src_dir}/standa.rs" '"power_state") if device == self.axis' "Standa power-state status property"
require_text "${driver_src_dir}/standa.rs" '"encoder_state") if device == self.axis' "Standa encoder-state status property"
require_text "${driver_src_dir}/standa.rs" '"raw_flags") if device == self.axis' "Standa raw-flags status property"
require_text "${driver_src_dir}/standa.rs" '"engine_settings") if device == self.axis' "Standa engine-settings property"
require_text "${driver_src_dir}/standa.rs" '"brake_settings") if device == self.axis' "Standa brake-settings property"
require_text "${driver_src_dir}/standa.rs" '"home_settings") if device == self.axis' "Standa home-settings property"
require_text "${driver_src_dir}/standa.rs" "is_status_property(&key)" "Standa status-property readback refresh"
require_text "${driver_src_dir}/standa.rs" "is_move_settings_property(&key)" "Standa movement-settings property readback refresh"
require_text "${device_docs_dir}/standa.md" '| `GenericCommand` | Axis | `refresh_readbacks`, `refresh_position`, `refresh_status`, `refresh_move_settings`, `refresh_engine_settings`, `refresh_brake_settings`, `refresh_home_settings`, or `refresh_static_settings` with no params' "Standa documented mapped GenericCommand"
require_text "${device_docs_dir}/standa.md" '`refresh_readbacks` refreshes all of those mapped' "Standa documented grouped refresh-readbacks"
require_text "${device_docs_dir}/standa.md" '| `move_command_state` | Axis | `I64`' "Standa documented move-command-state property"
require_text "${device_docs_dir}/standa.md" '| `power_state` | Axis | `I64`' "Standa documented power-state property"
require_text "${device_docs_dir}/standa.md" '| `encoder_state` | Axis | `I64`' "Standa documented encoder-state property"
require_text "${device_docs_dir}/standa.md" '| `raw_flags` | Axis | `I64`' "Standa documented raw-flags property"
require_text "${device_docs_dir}/standa.md" '| `velocity` | Axis | `Velocity` | um/s | R/W' "Standa documented writable velocity"
require_text "${device_docs_dir}/standa.md" '| `deceleration` | Axis | `Acceleration` | um/s^2 | R |' "Standa documented read-only deceleration"
require_text "${device_docs_dir}/standa.md" '| `antiplay_velocity` | Axis | `Velocity` | um/s | R |' "Standa documented read-only antiplay velocity"
require_text "${device_docs_dir}/standa.md" '| `engine_settings` | Axis | `Map`' "Standa documented engine settings"
require_text "${device_docs_dir}/standa.md" '| `brake_settings` | Axis | `Map`' "Standa documented brake settings"
require_text "${device_docs_dir}/standa.md" '| `home_settings` | Axis | `Map`' "Standa documented home settings"
require_text "${device_docs_dir}/evidence.md" 'static-settings refresh helpers use `geng`, `gbrk`, and `ghom` for documented read-only native maps' "Standa evidence static-settings documentation"
require_text "${device_docs_dir}/evidence.md" 'position refresh helpers and move/home/stop paths refresh position through `gpos`' "Standa evidence motion-position refresh documentation"
require_text "${device_docs_dir}/evidence.md" 'move and home paths poll documented `gets` status until the moving flag clears or an estimated motion timeout expires' "Standa evidence motion polling documentation"
require_text "${device_docs_dir}/evidence.md" 'configured `serial_port` records endpoint intent unless `connect = true`' "Standa evidence explicit connect gate"
require_text "${device_docs_dir}/evidence.md" 'configured real serial construction uses explicit 8 data bits, no parity, 2 stop bits, and no flow control' "Standa evidence serial framing"
require_text "${device_docs_dir}/evidence.md" 'movement-settings refresh helpers use `gmov` for velocity, acceleration, read-only deceleration, and read-only antiplay velocity' "Standa evidence movement-settings documentation"
require_text "${device_docs_dir}/evidence.md" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "Standa evidence resource metadata wording"
require_text "${driver_src_dir}/zaber.rs" 'if self.read_command_reply("move abs")?' "Zaber single-axis move command readback"
require_text "${driver_src_dir}/zaber.rs" 'if self.read_command_reply(index, "move abs")?' "Zaber multi-axis move command readback"
require_text "${driver_src_dir}/zaber.rs" '"baud_rate".into(), Value::I64(self.baud_rate as i64)' "Zaber baud-rate metadata"
require_text "${driver_src_dir}/zaber.rs" '"serial_port".into()' "Zaber serial-port resource metadata"
require_text "${driver_src_dir}/zaber.rs" '"connected".into(), Value::Bool(self.connected)' "Zaber connected resource metadata"
require_text "${driver_src_dir}/zaber.rs" '"peripheral_id"' "Zaber peripheral-id property"
require_text "${driver_src_dir}/zaber.rs" 'position_property("travel", "Travel", Some("um"), false)' "Zaber travel property"
require_text "${driver_src_dir}/zaber.rs" 'position_property("microstep_size", "Microstep size", Some("um"), false)' "Zaber microstep-size property"
require_text "${driver_src_dir}/zaber.rs" 'Zaber GenericCommand supports refresh_readbacks, refresh_position, refresh_velocity, refresh_acceleration, refresh_status, refresh_warning, and refresh_axis_summary' "Zaber mapped refresh GenericCommand"
require_text "${device_docs_dir}/zaber.md" "Configured discovery/resource metadata" "Zaber documented resource metadata"
require_text "${device_docs_dir}/zaber.md" '| `peripheral_id` | Axis | `String` | none | R | peripheral id | No | Probe/config metadata from `peripheral.id` |' "Zaber documented peripheral-id property"
require_text "${device_docs_dir}/zaber.md" '| `travel` | Axis | `Position` | um | R | axis travel range | No | Probe/config metadata from `limit.max` scaled by `resolution` |' "Zaber documented travel property"
require_text "${device_docs_dir}/zaber.md" '| `microstep_size` | Axis | `Position` | um | R | native unit conversion | No | Probe/config metadata from `resolution` |' "Zaber documented microstep-size property"
reject_text "${driver_src_dir}/zaber.rs" 'setting: "limit.home.pos".into()' "Zaber target writes must not alter limit/home settings"
require_text "${device_docs_dir}/zaber.md" '| `GenericCommand` | Axis devices | `refresh_readbacks`, `refresh_position`, `refresh_velocity`, `refresh_acceleration`, `refresh_status`, `refresh_warning`, or `refresh_axis_summary` with no params | Refreshed property map | Sends selected Zaber ASCII `get` readback through the existing property path; no arbitrary ASCII command/settings surface | Not sequenceable |' "Zaber documented refresh helpers"
require_text "${device_docs_dir}/zaber.md" '| `target` | Axis | `Position` | um | R/W | configured travel | No | Target cache only; motion uses `position`/`StageMove` |' "Zaber target-cache docs"
require_text "${device_docs_dir}/evidence.md" 'probe/config identity and geometry values expose read-only axis `peripheral_id`, `travel`, and `microstep_size` properties' "Zaber evidence identity/geometry property wording"
require_text "${device_docs_dir}/evidence.md" 'move/home/stop paths ingest command status/warning replies when present and refresh `get pos`' "Zaber evidence move/home/stop readback wording"
require_text "${device_docs_dir}/evidence.md" '`target` is a local cache only, while movement uses `position`/`StageMove`' "Zaber target-cache evidence wording"
require_text "${device_docs_dir}/evidence.md" 'axis `GenericCommand` is constrained to selected readback helpers for position, velocity, acceleration, status, warning, axis summary, and combined readbacks' "Zaber evidence mapped refresh wording"
require_text "${device_docs_dir}/evidence.md" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "Zaber evidence resource metadata wording"
require_text "$example_outputs_file" "selected stage: prior-nanoscan-z [axis.z, stage.z, piezo.z] axes=z" "generic Prior NanoScan motion output"
require_text "$example_outputs_file" "selected stage: sutter-mp285-xy [stage.xy, axis.x, axis.y] axes=x,y" "generic Sutter MP-285 motion output"
require_text "$example_outputs_file" "selected stage: sutter-xy-stage [axis.xy, stage.xy] axes=x,y" "generic Sutter stage motion output"
require_text "$example_outputs_file" "selected stage: trinamic-tmcl-x-stage [stage.1d, motion.stage, state.device, trinamic.tmcl.axis] axes=position" "generic TMCL motion output"
require_text "$example_outputs_file" "stop completed for trinamic-tmcl-x-stage: map keys=[actual_speed, actual_steps, axis, axis_index, busy, home_switch, left_limit_switch, position, position_reached, right_limit_switch, target, target_steps]" "generic TMCL stop completion output"
require_text "$example_outputs_file" "selected stage: triggerscope-focus [axis.z, stage.z, motion.stage] axes=z" "generic motion focus-only output"
require_text "$example_outputs_file" "selected stage: wosm-xy-stage [axis.xy, stage.xy, motion.stage] axes=x,y" "generic motion small-travel configured output"
require_text "$example_outputs_file" "selected stage: zaber-ascii-axis-1 [axis.1, stage.axis, stage.x] axes=position" "generic Zaber motion output"
require_text "${driver_src_dir}/sutter_stage.rs" '"baud_rate".into(), Value::I64(self.baud_rate as i64)' "Sutter Stage baud-rate metadata"
require_text "${driver_src_dir}/thorlabs_kurios.rs" '"baud_rate".into(), Value::I64(protocol::BAUD as i64)' "Thorlabs Kurios baud-rate metadata"
require_text "$example_outputs_file" "Configured TriggerScope controller, 12 device(s), 1 resource(s)" "TriggerScope discovery output"
require_text "$example_outputs_file" "added Configured TriggerScope controller with 12 device(s)" "TriggerScope add output"
require_text "${device_docs_dir}/triggerscope.md" "wrong type are rejected instead of" "TriggerScope invalid config-type documentation"
require_text "${driver_src_dir}/triggerscope.rs" "TriggerScope property {key} must be Bool" "TriggerScope invalid bool config code"
require_text "${driver_src_dir}/triggerscope.rs" "TriggerScope property {key} must be Voltage" "TriggerScope invalid voltage config code"
require_text "${driver_src_dir}/triggerscope.rs" "fn program_timing_plan" "TriggerScope timing-plan programming"
require_text "${driver_src_dir}/triggerscope.rs" "TriggerScope timing routes have no evidenced route opcode" "TriggerScope timing route gate"
require_text "${device_docs_dir}/triggerscope.md" "public timing-plan APIs for TTL \`high\`, DAC \`voltage\`, and evenly stepped focus \`z\` sequences" "TriggerScope timing-plan documentation"
require_text "${driver_src_dir}/squid.rs" "pub struct OsSquidSerialTransport" "Squid configured real serial transport"
require_text "${driver_src_dir}/squid.rs" "FixedBinaryCodec::new(STATUS_LEN)" "Squid fixed status-frame codec"
require_text "${device_docs_dir}/squid.md" "Protocol-backed control plus opt-in configured real serial startup status ingestion, runtime transport" "Squid documented real serial support"
require_text "$evidence_file" "configured real serial drains immediately available startup status frames before registration" "Squid evidence startup status ingestion wording"
require_text "$evidence_file" "hub \`GenericCommand\` aliases for disable-all and heartbeat only" "Squid evidence named GenericCommand wording"
require_text "$evidence_file" 'opt-in configured `os-serial` backend for the documented 8-byte command and 24-byte status frames' "Squid evidence real serial wording"
require_text "${device_docs_dir}/trinamic-tmcl.md" "malformed types or out-of-range topology" "TMCL invalid config-type documentation"
require_text "${driver_src_dir}/trinamic_tmcl.rs" "TMCL axes must be in 1..=255" "TMCL invalid axes config code"
require_text "${driver_src_dir}/trinamic_tmcl.rs" "TMCL property {key} must be Bool" "TMCL invalid bool config code"
require_text "${driver_src_dir}/trinamic_tmcl.rs" "driver.refresh_startup_axes()?" "TMCL construction-time axis refresh"
require_text "${driver_src_dir}/trinamic_tmcl.rs" "GetFirmwareVersionRaw" "TMCL raw firmware-version command"
require_text "${driver_src_dir}/trinamic_tmcl.rs" '"firmware_version_raw"' "TMCL raw firmware-version property"
require_text "${driver_src_dir}/trinamic_tmcl.rs" "TMCL GenericCommand supports refresh_readbacks, refresh_motion, refresh_profile, and refresh_switches" "TMCL mapped GenericCommand validation"
require_text "${driver_src_dir}/trinamic_tmcl.rs" '"serial_port".into()' "TMCL serial-port resource metadata"
require_text "${driver_src_dir}/trinamic_tmcl.rs" '"connected".into(), Value::Bool(self.connected)' "TMCL connected resource metadata"
require_text "${device_docs_dir}/trinamic-tmcl.md" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "TMCL documented resource metadata"
require_text "${device_docs_dir}/trinamic-tmcl.md" 'control command 136 type 1 for raw binary firmware-version readback' "TMCL documented raw firmware-version readback"
require_text "${device_docs_dir}/trinamic-tmcl.md" '| `GenericCommand` | Axis stage | `refresh_readbacks`, `refresh_motion`, `refresh_profile`, or `refresh_switches` with no params' "TMCL documented mapped GenericCommand"
require_text "$evidence_file" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "TMCL evidence resource metadata wording"
require_text "$evidence_file" 'real serial construction refreshes raw binary firmware-version and documented `GAP` axis parameters' "TMCL evidence raw firmware-version wording"
require_text "$evidence_file" 'named axis `GenericCommand` helpers refresh documented `GAP` axis position' "TMCL evidence mapped GenericCommand wording"
require_text "$example_outputs_file" "Configured Chuo Seiki QT controller, 3 device(s), 1 resource(s)" "Chuo QT discovery output"
require_text "$example_outputs_file" "added Configured Chuo Seiki QT controller with 3 device(s)" "Chuo QT add output"
require_text "${driver_src_dir}/chuo_seiki_qt.rs" "protocol::QtCommand::Identify" "Chuo QT construction-time active identification"
require_text "${driver_src_dir}/chuo_seiki_qt.rs" "Chuo QT GenericCommand supports refresh_readbacks, refresh_busy, and refresh_position" "Chuo QT mapped generic refresh validation"
require_text "${driver_src_dir}/chuo_seiki_qt.rs" "Chuo QT GenericCommand refresh commands do not accept params" "Chuo QT generic refresh params gate"
require_text "${driver_src_dir}/chuo_seiki_qt.rs" "unsupported Chuo QT capability invocation" "Chuo QT dispatch fails closed for unsupported capability invocation"
require_text "${driver_src_dir}/chuo_seiki_qt.rs" "fn refresh_raw_readback_after_motion" "Chuo QT motion raw readback helper"
require_text "${driver_src_dir}/chuo_seiki_qt.rs" "position.state.is_motion_state()" "Chuo QT mapped position-state completion polling"
require_text "${driver_src_dir}/chuo_seiki_qt.rs" '"serial_port".into()' "Chuo QT serial-port resource metadata"
require_text "${driver_src_dir}/chuo_seiki_qt.rs" '"connected".into()' "Chuo QT connected resource metadata"
require_text "${device_docs_dir}/chuo-seiki-qt.md" 'resource metadata records configured `serial_port`, fixed `baud_rate`, `serial_timeout`, and `connected` state' "Chuo QT documented resource metadata"
require_text "$evidence_file" 'resource metadata records configured `serial_port`, fixed `baud_rate`, `serial_timeout`, and `connected` state' "Chuo QT evidence resource metadata wording"
require_text "${device_docs_dir}/chuo-seiki-qt.md" '| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_busy`, or `refresh_position` with no params' "Chuo QT documented mapped generic refresh"
require_text "${device_docs_dir}/chuo-seiki-qt.md" 'Runtime reads of `busy_reply` and `position_reply` issue the mapped query' "Chuo QT documented raw readback refresh"
require_text "$evidence_file" 'hub `GenericCommand` exposes named `refresh_readbacks`, `refresh_busy`, and `refresh_position` helpers with no params' "Chuo QT evidence named generic refresh wording"
require_text "$evidence_file" 'motion/home/stop paths request position readbacks until known `D`/`H` moving/homing state characters clear' "Chuo QT evidence position-state polling wording"
require_text "$example_outputs_file" "Configured ITK Corvus controller, 3 device(s), 1 resource(s)" "Corvus discovery output"
require_text "$example_outputs_file" "added Configured ITK Corvus controller with 3 device(s)" "Corvus add output"
require_text "${device_docs_dir}/corvus.md" "controls whether the logical Z device is advertised" "Corvus invalid config-type documentation"
require_text "${driver_src_dir}/corvus.rs" "Corvus property {key} must be Bool" "Corvus invalid bool config code"
require_text "${driver_src_dir}/corvus.rs" "protocol::CorvusCommand::Status" "Corvus construction-time active status readback"
require_text "${driver_src_dir}/corvus.rs" '"serial_port".into()' "Corvus serial-port resource metadata"
require_text "${device_docs_dir}/corvus.md" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "Corvus documented resource metadata"
require_text "$evidence_file" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "Corvus evidence resource metadata wording"
require_text "${driver_src_dir}/corvus.rs" "Corvus GenericCommand supports refresh_readbacks, refresh_status, refresh_error, refresh_position, refresh_limits, refresh_speed, and refresh_acceleration" "Corvus mapped generic refresh validation"
require_text "${driver_src_dir}/corvus.rs" "Corvus GenericCommand refresh commands do not accept params" "Corvus generic refresh params gate"
require_text "${driver_src_dir}/corvus.rs" "unsupported Corvus capability invocation" "Corvus dispatch fails closed for unsupported capability invocation"
require_text "${driver_src_dir}/corvus.rs" "fn refresh_motion_readback(&mut self) -> Result<()> {" "Corvus motion status/position/error readback helper"
require_text "${driver_src_dir}/corvus.rs" "protocol::busy_from_status(status)" "Corvus mapped busy-bit polling"
require_text "${device_docs_dir}/corvus.md" '| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_status`, `refresh_error`, `refresh_position`, `refresh_limits`, `refresh_speed`, or `refresh_acceleration` with no params' "Corvus documented mapped generic refresh"
require_text "$evidence_file" 'hub `GenericCommand` exposes named `refresh_readbacks`, `refresh_status`, `refresh_error`, `refresh_position`, `refresh_limits`, `refresh_speed`, and `refresh_acceleration` helpers with no params' "Corvus evidence named generic refresh wording"
require_text "${driver_src_dir}/corvus.rs" "capability(2, device, CapabilityKind::StageHome)" "Corvus advertised StageHome"
require_text "${device_docs_dir}/corvus.md" '| `StageHome` | XY/Z stage | `None` | Position map or `Position`' "Corvus documented StageHome"
require_text "$evidence_file" 'moves, home/calibrate, abort, speed, acceleration, and joystick state' "Corvus evidence home support wording"
require_text "$evidence_file" 'runtime reads and move/home/stop paths request `st` busy-bit polling plus `p` position and `ge` error readbacks' "Corvus evidence motion readback wording"
require_text "$example_outputs_file" "Configured Bluebox Optics niji, 8 device(s), 1 resource(s)" "niji discovery output"
require_text "$example_outputs_file" "added Configured Bluebox Optics niji with 8 device(s)" "niji add output"
require_text "${driver_src_dir}/bluebox_niji.rs" "protocol::NijiCommand::QueryStatus" "niji construction-time active status query"
require_text "${driver_src_dir}/bluebox_niji.rs" '"serial_port".into()' "niji serial-port resource metadata"
require_text "${device_docs_dir}/bluebox-niji.md" 'resource metadata records configured `serial_port`, fixed `baud_rate`, `serial_timeout`, and `connected` state' "niji documented resource metadata"
require_text "$evidence_file" 'resource metadata records configured `serial_port`, fixed `baud_rate`, `serial_timeout`, and `connected` state' "niji evidence resource metadata wording"
require_text "${driver_src_dir}/bluebox_niji.rs" "Niji GenericCommand supports refresh_readbacks, refresh_status, and refresh_temperatures" "niji mapped generic refresh validation"
require_text "${driver_src_dir}/bluebox_niji.rs" "Niji GenericCommand refresh commands do not accept params" "niji generic refresh params gate"
require_text "${driver_src_dir}/bluebox_niji.rs" "fn refresh_status_after_write(&mut self) -> Result<()> {" "niji connected write-path status refresh helper"
require_text "${device_docs_dir}/bluebox-niji.md" '| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_status`, or `refresh_temperatures` with no params' "niji documented mapped generic refresh"
require_text "$evidence_file" 'hub `GenericCommand` exposes named `refresh_readbacks`, `refresh_status`, and `refresh_temperatures` helpers with no params' "niji evidence named generic refresh wording"
require_text "$evidence_file" 'connected output/trigger/mode write paths request `?` status readback' "niji evidence write-path status readback wording"
require_text "$example_outputs_file" "Configured Opentrons OT-2 robot, 6 device(s), 1 resource(s)" "Opentrons OT-2 discovery output"
require_text "$example_outputs_file" "added Configured Opentrons OT-2 robot with 6 device(s)" "Opentrons OT-2 add output"
require_text "$example_outputs_file" "selected robot inventory source: opentrons" "Opentrons generic robot inventory output"
require_text "$example_outputs_file" "opentrons-ot2-module-1 [module.temperature, module.opentrons]" "Opentrons module inventory output"
require_text "$example_outputs_file" "  port: I64(31950)" "Opentrons robot-server port output"
require_text "$example_outputs_file" "target_temperature: Temperature(Temperature { value: 4.0, unit: Celsius })" "Opentrons typed module temperature output"
require_text "$example_outputs_file" "Configured Thorlabs SC10 shutter controller, 2 device(s), 1 resource(s)" "Thorlabs SC10 discovery output"
require_text "$example_outputs_file" "added Configured Thorlabs SC10 shutter controller with 2 device(s)" "Thorlabs SC10 add output"
require_text "${driver_src_dir}/thorlabs_sc10.rs" "driver.refresh_startup_state()?" "Thorlabs SC10 construction-time startup readback"
require_text "${driver_src_dir}/thorlabs_sc10.rs" "SC10 GenericCommand supports refresh_readbacks, refresh_identity, refresh_status, refresh_timing, and refresh_open" "Thorlabs SC10 mapped GenericCommand validation"
require_text "${driver_src_dir}/thorlabs_sc10.rs" '"serial_port".into()' "Thorlabs SC10 serial-port resource metadata"
require_text "${driver_src_dir}/thorlabs_sc10.rs" '"connected".into(), Value::Bool(self.serial.is_some())' "Thorlabs SC10 connected resource metadata"
require_text "${device_docs_dir}/thorlabs-sc10.md" 'resource metadata records configured `serial_port`, fixed `baud_rate`, `serial_timeout`, and `connected` state' "Thorlabs SC10 documented resource metadata"
require_text "${device_docs_dir}/thorlabs-sc10.md" '`GenericCommand` | `thorlabs-sc10-controller` | `refresh_readbacks`, `refresh_identity`, `refresh_status`, `refresh_timing`, or `refresh_open` with no params' "Thorlabs SC10 documented mapped GenericCommand"
require_text "$evidence_file" 'resource metadata records configured `serial_port`, fixed `baud_rate`, `serial_timeout`, and `connected` state' "Thorlabs SC10 evidence resource metadata wording"
require_text "$evidence_file" "named controller GenericCommand helpers refresh only mapped query readbacks" "Thorlabs SC10 evidence mapped GenericCommand wording"
require_text "$example_outputs_file" "Configured CoolLED pE-340, 5 device(s), 1 resource(s)" "CoolLED pE-340 discovery output"
require_text "$example_outputs_file" "added Configured CoolLED pE-340 with 5 device(s)" "CoolLED pE-340 add output"
require_text "$example_outputs_file" "detected 66 candidate driver(s)" "discover_devices total candidate count"
require_text "$example_outputs_file" "saved discovery lock with 66 persistent entrie(s)" "discover_devices persistent lock count"
require_text "$driver_lib" "AgilentLaserCombinerDiscovery::from_config" "Agilent discovery registration"
require_text "$example_outputs_file" "Configured Agilent Laser Combiner, 9 device(s), 1 resource(s)" "Agilent discovery output"
require_text "$example_outputs_file" "device: agilent-laser-line-1 [\"light.source\", \"laser\", \"trigger.sink\"]" "Agilent laser-line descriptor output"
require_text "$example_outputs_file" "added Configured Agilent Laser Combiner with 9 device(s)" "Agilent add output"
require_text "$driver_lib" "ArduinoDiscovery::from_config" "Arduino discovery registration"
require_text "$example_outputs_file" "Configured Arduino controller, 5 device(s), 1 resource(s)" "Arduino discovery output"
require_text "$example_outputs_file" "device: arduino-digital-out [\"digital.io\", \"trigger.source\"]" "Arduino digital descriptor output"
require_text "$example_outputs_file" "added Configured Arduino controller with 5 device(s)" "Arduino add output"
require_text "$driver_lib" "ArduinoCounterDiscovery::from_config" "Arduino Counter discovery registration"
require_text "$example_outputs_file" "Configured Arduino Counter, 3 device(s), 1 resource(s)" "Arduino Counter discovery output"
require_text "$example_outputs_file" "device: arduino-counter-pulse [\"trigger.source\", \"pulse.generator\"]" "Arduino Counter pulse descriptor output"
require_text "$example_outputs_file" "added Configured Arduino Counter with 3 device(s)" "Arduino Counter add output"
require_text "$driver_lib" "Esp32Discovery::from_config" "ESP32 discovery registration"
require_text "$example_outputs_file" "Configured ESP32 controller, 7 device(s), 1 resource(s)" "ESP32 discovery output"
require_text "$example_outputs_file" "device: esp32-pwm [\"analog.output\", \"pwm\"]" "ESP32 PWM descriptor output"
require_text "$example_outputs_file" "added Configured ESP32 controller with 7 device(s)" "ESP32 add output"
require_text "$driver_lib" "OpenUc2Discovery::from_config" "OpenUC2 discovery registration"
require_text "$example_outputs_file" "Configured OpenUC2 Feather controller, 4 device(s), 1 resource(s)" "OpenUC2 discovery output"
require_text "$example_outputs_file" "device: openuc2-laser [\"light.source\", \"shutter\", \"trigger.sink\"]" "OpenUC2 laser descriptor output"
require_text "$example_outputs_file" "added Configured OpenUC2 Feather controller with 4 device(s)" "OpenUC2 add output"
require_text "$driver_lib" "TeensyPulseDiscovery::from_config" "Teensy Pulse discovery registration"
require_text "$example_outputs_file" "Configured Teensy pulse generator, 2 device(s), 1 resource(s)" "Teensy Pulse discovery output"
require_text "$example_outputs_file" "device: teensy-pulse-generator [\"trigger.source\", \"pulse.generator\", \"timing.source\"]" "Teensy Pulse descriptor output"
require_text "$example_outputs_file" "added Configured Teensy pulse generator with 2 device(s)" "Teensy Pulse add output"
require_text "$example_outputs_file" "Configured Andor SDK2 camera (136e:0012), 3 device(s), 3 resource(s)" "Andor SDK2 discovery output"
require_text "$example_outputs_file" "device: Configured Andor SDK2 camera [\"camera\", \"camera.scientific\", \"detector.mono\", \"andor.sdk2\"]" "Andor SDK2 camera descriptor output"
require_text "$example_outputs_file" "added Configured Andor SDK2 camera (136e:0012) with 3 device(s)" "Andor SDK2 add output"
require_text "$example_outputs_file" "Configured Andor SDK3 camera (136e:0014), 3 device(s), 3 resource(s)" "Andor SDK3 discovery output"
require_text "$example_outputs_file" "device: Configured Andor SDK3 camera [\"camera\", \"camera.scientific\", \"detector.mono\", \"andor.sdk3\"]" "Andor SDK3 camera descriptor output"
require_text "$example_outputs_file" "added Configured Andor SDK3 camera (136e:0014) with 3 device(s)" "Andor SDK3 add output"
require_text "${device_docs_dir}/andor-sdk2.md" "invalid u16 values are rejected instead of silently falling back" "Andor invalid numeric config gate"
require_text "${driver_src_dir}/andor_camera.rs" "Andor property {key} must fit in an unsigned 16-bit integer" "Andor invalid u16 config code"
require_text "${driver_src_dir}/andor_camera.rs" "Andor property {key} must be Bool" "Andor invalid bool config code"
require_text "$example_outputs_file" "Configured Photometrics PVCAM camera (PVCAM-CONFIG-0002), 3 device(s), 2 resource(s)" "PVCAM discovery output"
require_text "$example_outputs_file" "device: Configured Photometrics PVCAM camera [\"camera\", \"camera.scientific\", \"detector.mono\", \"pvcam\"]" "PVCAM camera descriptor output"
require_text "$example_outputs_file" "added Configured Photometrics PVCAM camera (PVCAM-CONFIG-0002) with 3 device(s)" "PVCAM add output"
require_text "$driver_lib" "AbsCameraDiscovery::from_config" "ABS camera discovery registration"
require_text "$driver_lib" "MightexCameraDiscovery::from_config" "Mightex camera discovery registration"
require_text "$example_outputs_file" "Configured ABS camera reverse engineered support (ABS CamUSB camera), 1 device(s), 1 resource(s)" "ABS camera discovery output"
require_text "$example_outputs_file" "device: Configured ABS camera reverse engineered support [\"camera\", \"reverse.engineered\"]" "ABS camera descriptor output"
require_text "$example_outputs_file" "added Configured ABS camera reverse engineered support (ABS CamUSB camera) with 1 device(s)" "ABS camera add output"
require_text "$example_outputs_file" "Configured Mightex camera reverse engineered support (Mightex buffered USB camera), 1 device(s), 2 resource(s)" "Mightex camera discovery output"
require_text "$example_outputs_file" "device: Configured Mightex camera reverse engineered support [\"camera\", \"reverse.engineered\"]" "Mightex camera descriptor output"
require_text "$example_outputs_file" "added Configured Mightex camera reverse engineered support (Mightex buffered USB camera) with 1 device(s)" "Mightex camera add output"
require_text "${device_docs_dir}/photometrics-pvcam.md" "invalid pixel counts are rejected instead of silently falling back" "PVCAM invalid pixel-count config gate"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "PVCAM property {key} must fit in an unsigned 32-bit pixel count" "PVCAM invalid pixel-count config code"
require_text "${device_docs_dir}/photometrics-pvcam.md" "unknown values are rejected instead of silently falling back" "PVCAM invalid pixel-format config gate"
require_text "${driver_src_dir}/photometrics_pvcam.rs" "PVCAM property {key} must be Mono16, Mono8, or Bayer16" "PVCAM invalid pixel-format config code"
require_text "${device_docs_dir}/photometrics-pvcam.md" "\`Null\` config omits the metadata property" "PVCAM nullable cooler metadata documentation"
require_text "${device_docs_dir}/mightex-bls.md" "omitted when unavailable" "Mightex module-type availability gate"
require_text "${driver_src_dir}/mightex_bls.rs" "Mightex Sirius GenericCommand supports disable_all only" "Mightex mapped generic command validation"
require_text "${device_docs_dir}/mightex-bls.md" 'named aliases only, no params; `disable_all`' "Mightex named hub helper documentation"
reject_text "${driver_src_dir}/mightex_bls.rs" "SiriusCommand::Raw" "Mightex arbitrary command variant"
reject_text "${driver_src_dir}/mightex_bls.rs" "one printable ASCII line" "Mightex arbitrary command validation"
reject_text "${device_docs_dir}/mightex-bls.md" "one printable ASCII command line" "Mightex arbitrary command documentation"
reject_text "${device_docs_dir}/mightex-bls.md" "diagnostic raw command" "Mightex arbitrary command documentation"
reject_text "${reverse_docs_dir}/mightex.md" "raw diagnostics" "Mightex arbitrary command reverse note"
reject_text "${reverse_docs_dir}/mightex.md" 'hub-only named helpers for reverse engineered `disable_all`/`RESET`/`STORE`' "Mightex reset/store helpers hidden from reverse note"
require_text "$example_outputs_file" "Configured Evident IX85 microscope body (IX85-CONFIG-0002), 8 device(s), 1 resource(s)" "IX85 discovery output"
require_text "$example_outputs_file" "device: ix85-zdc-autofocus [\"autofocus\", \"zdc\", \"state.device\"]" "IX85 autofocus descriptor output"
require_text "$example_outputs_file" "added Configured Evident IX85 microscope body (IX85-CONFIG-0002) with 8 device(s)" "IX85 add output"
require_text "$driver_lib" "OkolabDiscovery::from_config" "Okolab discovery registration"
require_text "$example_outputs_file" "Configured Okolab environmental controller (H201 T Unit-BL), 3 device(s), 2 resource(s)" "Okolab discovery output"
require_text "$example_outputs_file" "device: Configured Okolab environmental controller temperature [\"environment.temperature\", \"measure\"]" "Okolab temperature descriptor output"
require_text "$example_outputs_file" "added Configured Okolab environmental controller (H201 T Unit-BL) with 3 device(s)" "Okolab add output"
require_text "${device_docs_dir}/evident-ix85.md" "values outside 0..10500 um are rejected instead of silently advertising impossible readback" "IX85 invalid focus config gate"
require_text "${driver_src_dir}/evident_ix85.rs" "IX85 property {key} must be in 0..=10500 um" "IX85 invalid focus config code"
require_text "${device_docs_dir}/evident-ix85.md" "wrong types are rejected instead of silently falling back" "IX85 invalid config-type documentation"
require_text "${driver_src_dir}/evident_ix85.rs" "IX85 property {key} must be Bool" "IX85 invalid bool config code"
require_text "${driver_src_dir}/evident_ix85.rs" "IX85 property {key} must be String" "IX85 invalid string config code"
require_text "$example_outputs_file" "selected shutter family: sc10" "SC10 generic shutter output"
require_text "$example_outputs_file" "shutter open completed: Bool(true)" "SC10 shutter open output"
require_text "$example_outputs_file" "interlock_closed: Bool(true)" "SC10 typed safety readback output"
require_text "$example_outputs_file" "state_summary: map keys=[enabled, fault, interlock_closed, mode, trigger_mode]" "SC10 state-summary readback output"
require_text "$run_examples_file" "shutter [sc10\\|esp32\\|ix85]" "run examples IX85 shutter selector list"
require_text "$example_outputs_file" "selected shutter family: esp32" "ESP32 generic shutter output"
require_text "$example_outputs_file" "shutter open completed: map keys=[open, triggered]" "ESP32 shutter open output"
require_text "$example_outputs_file" "selected shutter family: ix85" "IX85 generic shutter output"
require_text "$example_outputs_file" "dia_shutter_open: Bool(false)" "IX85 shutter readback output"
require_text "$example_outputs_file" "selected light source family: niji" "niji generic light-source output"
require_text "$example_outputs_file" "output_temperature: Temperature(Temperature { value: 22.5, unit: Celsius })" "niji typed temperature output"
require_text "$example_outputs_file" "selected light source family: agilent" "Agilent generic light-source output"
require_text "$example_outputs_file" "selected light channel: agilent-laser-line-1" "Agilent light-source channel output"
require_text "$example_outputs_file" "event: agilent-combiner-hub.state_mask changed to I64(1)" "Agilent light-source state-mask output"
require_text "$audit_root/crates/numanager-examples/src/light_source.rs" "AgilentLaserCombinerDriver::configured" "Agilent light-source selector"
require_text "$example_outputs_file" "selected light source family: obis" "OBIS generic light-source output"
require_text "$example_outputs_file" "selected light channel: coherent-obis-laser" "OBIS light-source channel output"
require_text "$example_outputs_file" "selected light source family: omicron" "Omicron generic light-source output"
require_text "$example_outputs_file" "selected light channel: omicron-serial-laser" "Omicron light-source channel output"
require_text "$audit_root/crates/numanager-examples/src/light_source.rs" "ObisDriver::simulated" "OBIS light-source selector"
require_text "$audit_root/crates/numanager-examples/src/light_source.rs" "OmicronDriver::simulated" "Omicron light-source selector"
require_text "$audit_root/crates/numanager-examples/src/light_source.rs" "ValueType::OpticalPower" "light-source optical-power output helper"
require_text "${device_docs_dir}/coherent-obis.md" "light_source obis" "OBIS light-source example selector docs"
require_text "${device_docs_dir}/omicron.md" "light_source omicron" "Omicron light-source example selector docs"
require_text "$example_outputs_file" "selected light source family: pe340" "CoolLED pE-340 generic light-source output"
require_text "$example_outputs_file" "selected light source family: pe4000" "CoolLED pE-4000 generic light-source output"
require_text "$example_outputs_file" "channel property: wavelength type=Wavelength writable=true sequenceable=false" "CoolLED pE-340 wavelength property output"
require_text "${driver_src_dir}/coolled.rs" "protocol::execute_pe300_probe_script(&mut serial, 4)" "CoolLED pE-300 construction-time configured startup readback"
require_text "${driver_src_dir}/coolled.rs" "protocol::execute_pe4000_probe_script(&mut serial, 4)" "CoolLED pE-4000 construction-time configured startup readback"
require_text "${driver_src_dir}/coolled.rs" "fn refresh_pe300_readback" "CoolLED pE-300 readback ingestion"
require_text "${driver_src_dir}/coolled.rs" "fn refresh_pe4000_readback" "CoolLED pE-4000 readback ingestion"
require_text "${driver_src_dir}/coolled.rs" "CoolLED pE-300 hub GenericCommand supports refresh_readbacks, refresh_identity, refresh_status, and refresh_channels" "CoolLED pE-300 mapped hub GenericCommand validation"
require_text "${driver_src_dir}/coolled.rs" "CoolLED pE-4000 hub GenericCommand supports refresh_readbacks, refresh_identity, refresh_status, and refresh_channels" "CoolLED pE-4000 mapped hub GenericCommand validation"
require_text "${driver_src_dir}/coolled.rs" '"serial_port".into()' "CoolLED serial-port resource metadata"
require_text "${driver_src_dir}/coolled.rs" '"connected".into(), Value::Bool(self.connected)' "CoolLED connected resource metadata"
require_text "${device_docs_dir}/coolled.md" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "CoolLED documented resource metadata"
require_text "${device_docs_dir}/coolled.md" '`GenericCommand` | Hub | `refresh_readbacks`, `refresh_identity`, `refresh_status`, or `refresh_channels` with no params' "CoolLED documented mapped hub GenericCommand"
require_text "${device_docs_dir}/coolled.md" '`GenericCommand` | Channels | `refresh_readbacks` or `refresh_channel` with no params' "CoolLED documented mapped channel GenericCommand"
require_text "$device_index_file" "pE-300/pE-4000/pE-340 configured opt-in serial control/readback and refresh helpers" "CoolLED support matrix readback wording"
require_text "$evidence_file" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "CoolLED evidence resource metadata wording"
require_text "$evidence_file" "property reads, named GenericCommand refresh helpers, and writable global/channel/intensity/wavelength paths request status or channel readbacks" "CoolLED evidence readback wording"
require_text "$example_outputs_file" "selected light source family: lumencor" "Lumencor generic light-source output"
require_text "$readme_file" "| [Lumencor Spectra/SpectraX/CIA](docs/devices/lumencor.md) | Serial illumination control and readback | - |" "Lumencor README row"
require_text "$device_index_file" "Configured opt-in serial startup/setup readback plus CIA info readback and CIA command helpers" "Lumencor device-index wording"
require_text "$evidence_file" "Lumencor Spectra/SpectraX/CIA" "Lumencor evidence wording"
require_text "${driver_src_dir}/lumencor.rs" "protocol::execute_spectra_probe_script(&mut serial, &configured.probe)" "Lumencor Spectra construction-time startup probe"
require_text "${driver_src_dir}/lumencor.rs" "protocol::execute_cia_probe_script(" "Lumencor CIA construction-time setup probe"
require_text "${driver_src_dir}/lumencor.rs" "fn refresh_cia_info" "Lumencor CIA info readback helper"
require_text "${driver_src_dir}/lumencor.rs" "fn configured_cia_program_commands" "Lumencor CIA typed program-loading command path"
require_text "${driver_src_dir}/lumencor.rs" "\"rewind\" =>" "Lumencor CIA mapped rewind command"
require_text "${driver_src_dir}/lumencor.rs" "Lumencor CIA GenericCommand commands do not accept params" "Lumencor CIA generic-command params gate"
require_text "${driver_src_dir}/lumencor.rs" '"serial_port".into()' "Lumencor serial-port resource metadata"
require_text "${driver_src_dir}/lumencor.rs" '"connected".into(), Value::Bool(self.connected)' "Lumencor connected resource metadata"
require_text "${device_docs_dir}/lumencor.md" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "Lumencor documented resource metadata"
require_text "${device_docs_dir}/lumencor.md" 'Runtime reads of `info` issue' "Lumencor CIA info readback documentation"
require_text "${device_docs_dir}/lumencor.md" 'GenericCommand` accepts only the named `run`, `stop`, `step`, `rewind`, and' "Lumencor documented named generic command"
require_text "$evidence_file" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "Lumencor evidence resource metadata wording"
require_text "$evidence_file" "runtime \`info\` reads and engine/polarity writes request \`#I\` refresh" "Lumencor evidence info readback wording"
require_text "$evidence_file" 'CIA program loading uses `PulseProgram`/timing arm paths' "Lumencor evidence typed program loading"
require_text "$example_outputs_file" "selected light source family: lmm5" "Spectral LMM5 generic light-source output"
require_text "${driver_src_dir}/spectral_lmm5.rs" "driver.read_shutter_status()?" "Spectral LMM5 construction-time shutter readback"
require_text "${driver_src_dir}/spectral_lmm5.rs" '"serial_port".into()' "Spectral LMM5 serial-port resource metadata"
require_text "${driver_src_dir}/spectral_lmm5.rs" '"connected".into(), Value::Bool(self.connected)' "Spectral LMM5 connected resource metadata"
require_text "${driver_src_dir}/spectral_lmm5.rs" "\"trigger_out_interval\"" "Spectral LMM5 typed trigger interval property"
require_text "${driver_src_dir}/spectral_lmm5.rs" "Spectral LMM5 GenericCommand supports refresh_readbacks, refresh_shutter_status, refresh_wavelengths, apply_trigger_in, apply_trigger_out, and apply_trigger_profiles" "Spectral LMM5 mapped GenericCommand validation"
require_text "${driver_src_dir}/spectral_lmm5.rs" "unsupported Spectral LMM5 capability invocation" "Spectral LMM5 dispatch fails closed for unsupported capability invocation"
require_text "${device_docs_dir}/spectral-lmm5.md" "trigger profile configuration" "Spectral LMM5 trigger profile documentation"
require_text "${device_docs_dir}/spectral-lmm5.md" '| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_shutter_status`, `refresh_wavelengths`, `apply_trigger_in`, `apply_trigger_out`, or `apply_trigger_profiles` with no params | Status, wavelength, or trigger-profile map | Uses only documented LMM5 shutter-status, wavelength-readback, and trigger-configure command paths; no arbitrary hex command surface | Not sequenceable |' "Spectral LMM5 documented mapped GenericCommand"
require_text "$evidence_file" 'resource metadata records configured `serial_port` and `connected` state' "Spectral LMM5 evidence resource metadata wording"
require_text "$evidence_file" 'hub `GenericCommand` is constrained to documented shutter-status, wavelength-readback, combined-readback, and trigger-configure refresh/apply helpers with no arbitrary hex command surface' "Spectral LMM5 evidence mapped GenericCommand wording"
require_text "$example_outputs_file" "selected light source family: thorlabs-dc" "Thorlabs DC generic light-source output"
require_text "$example_outputs_file" "selected light source family: dc2200" "Thorlabs DC2200 generic light-source output"
require_text "$example_outputs_file" "selected light source family: dc3100" "Thorlabs DC3100 generic light-source output"
require_text "$example_outputs_file" "selected light source family: dc4100" "Thorlabs DC4100 generic light-source output"
require_text "$example_outputs_file" "selected light source family: openuc2" "OpenUC2 generic light-source output"
require_text "$example_outputs_file" "selected light source family: wosm" "WOSM generic light-source output"
require_text "${driver_src_dir}/wosm.rs" "fn write_sequence_enabled(&mut self, enabled: bool)" "WOSM sequence run/end control"
require_text "${driver_src_dir}/wosm.rs" "(CapabilityKind::TriggerSource, CapabilityRequest::Trigger(request))" "WOSM switch TriggerSource invocation"
require_text "${driver_src_dir}/wosm.rs" "unsupported WOSM capability invocation" "WOSM dispatch fails closed for unsupported capability invocation"
require_text "${driver_src_dir}/wosm.rs" "fn program_timing_plan" "WOSM timing-plan programming"
require_text "${driver_src_dir}/wosm.rs" "WOSM timing routes have no evidenced route opcode" "WOSM timing route gate"
require_text "${driver_src_dir}/wosm.rs" "protocol::WosmCommand::Blanking(enabled)" "WOSM blanking control"
require_text "${driver_src_dir}/wosm.rs" "protocol::WosmCommand::PullUp" "WOSM input pull-up control"
require_text "${driver_src_dir}/wosm.rs" '"input_pullups"' "WOSM input pull-up property"
require_text "${driver_src_dir}/wosm.rs" '"prompt_timeout".into()' "WOSM prompt-timeout resource metadata"
require_text "${driver_src_dir}/wosm.rs" '"connected".into(), Value::Bool(self.tcp.is_some())' "WOSM connected resource metadata"
require_text "${device_docs_dir}/wosm.md" "Sequence run/end, switch-state sequence loading, and blanking controls are command-backed" "WOSM command-backed sequence/blanking documentation"
require_text "${device_docs_dir}/wosm.md" '| `TriggerSource` | `wosm-switch` | `CapabilityRequest::Trigger` enable/disable/pulse | Sequence-enabled bool | TCP `W>` prompt when `connect = true`; configured completion otherwise | Starts/stops the switch-state sequence path loaded by timing `Arm` |' "WOSM documented switch TriggerSource capability"
require_text "${device_docs_dir}/wosm.md" '| `input_pullups` | Input | `I64`' "WOSM input pull-up property documentation"
require_text "${device_docs_dir}/wosm.md" 'resource metadata records configured `host`, `port`, `prompt_timeout`, and `connected` state' "WOSM documented resource metadata"
require_text "$evidence_file" 'resource metadata records configured `host`, `port`, `prompt_timeout`, and `connected` state' "WOSM evidence resource metadata wording"
require_text "$evidence_file" "input-pull-up writes use the typed \`input_pullups\` bitmask" "WOSM evidence input pull-up wording"
require_text "$example_outputs_file" "constant_current: ElectricCurrent(ElectricCurrent { value: 0.0, unit: Milliamps })" "Thorlabs DC typed current readback"
require_text "$example_outputs_file" "channel property: pwm_duty_cycle type=Ratio writable=true sequenceable=false" "Thorlabs DC2200 typed duty-cycle property output"
require_text "${driver_src_dir}/thorlabs_dc.rs" "MSG_REQUEST_DEV_DEP_IN" "Thorlabs DC2200 USBTMC request code"
require_text "${driver_src_dir}/thorlabs_dc.rs" "protocol::execute_probe_script(&mut serial, probe.family, 4)" "Thorlabs DC construction-time configured startup readback"
require_text "${driver_src_dir}/thorlabs_dc.rs" "fn refresh_property_readback" "Thorlabs DC write-path readback helper"
require_text "${driver_src_dir}/thorlabs_dc.rs" "Thorlabs DC controller GenericCommand supports refresh_readbacks, refresh_output, refresh_setpoints, refresh_status, and refresh_identity" "Thorlabs DC mapped controller GenericCommand validation"
require_text "${driver_src_dir}/thorlabs_dc.rs" "Thorlabs DC channel GenericCommand supports refresh_readbacks, refresh_output, refresh_setpoints, and refresh_identity" "Thorlabs DC mapped channel GenericCommand validation"
require_text "${driver_src_dir}/thorlabs_dc.rs" '"serial_port".into()' "Thorlabs DC serial-port resource metadata"
require_text "${driver_src_dir}/thorlabs_dc.rs" '"usb_vendor_id".into()' "Thorlabs DC USBTMC vendor resource metadata"
require_text "${driver_src_dir}/thorlabs_dc.rs" '"connected".into(), Value::Bool(connected)' "Thorlabs DC connected resource metadata"
require_text "${device_docs_dir}/thorlabs-dc.md" "Runtime property reads issue the mapped query" "Thorlabs DC runtime readback documentation"
require_text "${device_docs_dir}/thorlabs-dc.md" '`GenericCommand` | Controller | `refresh_readbacks`, `refresh_output`, `refresh_setpoints`, `refresh_status`, or `refresh_identity` with no params' "Thorlabs DC documented controller GenericCommand"
require_text "${device_docs_dir}/thorlabs-dc.md" '`GenericCommand` | DC4100 channel | `refresh_readbacks`, `refresh_output`, `refresh_setpoints`, or `refresh_identity` with no params' "Thorlabs DC documented channel GenericCommand"
require_text "${device_docs_dir}/thorlabs-dc.md" 'resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state' "Thorlabs DC documented serial resource metadata"
require_text "${device_docs_dir}/thorlabs-dc.md" 'resource metadata records configured USB VID/PID, interface, bulk endpoints, read size, and `connected` state' "Thorlabs DC documented USBTMC resource metadata"
require_text "$evidence_file" 'resource metadata records configured serial or USBTMC endpoint fields plus `connected` state' "Thorlabs DC evidence resource metadata wording"
require_text "$evidence_file" "runtime reads, named GenericCommand helpers, and writable output/mode/current/PWM/modulation/channel paths request mapped query readbacks" "Thorlabs DC evidence readback wording"
require_text "$evidence_file" "GenericCommand helpers do not expose arbitrary serial, SCPI, USBTMC, save, or setter commands" "Thorlabs DC evidence mapped GenericCommand wording"
require_text "${device_docs_dir}/thorlabs-dc.md" "explicit DC2200 USBTMC endpoint" "Thorlabs DC2200 USBTMC docs"
require_text "$example_outputs_file" "channel property: modulation_current type=ElectricCurrent writable=true sequenceable=false" "Thorlabs DC3100 typed modulation-current property output"
require_text "$example_outputs_file" "brightness: Ratio(Ratio { value: 0.0, unit: Percent })" "Thorlabs DC4100 typed brightness readback"
require_text "$example_outputs_file" "transmission: Ratio(Ratio { value: 42.0, unit: Percent })" "Spectral LMM5 typed transmission readback"
require_text "$example_outputs_file" "power: Ratio(Ratio { value: 0.0, unit: Percent })" "OpenUC2 typed ratio-power readback"
require_text "$example_outputs_file" "output: Ratio(Ratio { value: 42.0, unit: Percent })" "WOSM typed output readback"
require_text "$example_outputs_file" "mightex hardware: no Sirius HID light controller detected" "Mightex HID no-hardware output"
require_text "$example_outputs_file" "mightex hardware output setup completed: map keys=[enabled, intensity, mode]" "Mightex HID low-output setup output"
require_text "$example_outputs_file" "mightex hardware active enabled: Bool(true)" "Mightex HID active-state readback output"
require_text "$example_outputs_file" "mightex hardware active safety: active map keys=[device, enabled, state]" "Mightex HID active safety-summary output"
require_text "$example_outputs_file" "mightex hardware output: holding 1% output for 1000 ms" "Mightex HID hold-duration output"
require_text "$example_outputs_file" "mightex hardware output observation required: record visible light or meter/readback before validation" "Mightex HID hardware observation output"
require_text "$example_outputs_file" "mightex hardware disable completed: map keys=[enabled, triggered]" "Mightex HID disable output"
require_text "$example_outputs_file" "mightex hardware final safety: safe map keys=[device, enabled, state]" "Mightex HID final safety-summary output"
require_text "$example_outputs_file" "mightex hardware hub last_transaction: map keys=[command, command_count, outcome, reply, reply_error, reply_expected, reply_kind, reply_report_count, support_level, wire_terminator]" "Mightex HID transaction readback output"
require_text "$example_outputs_file" "selected laser family: cobolt" "Cobolt generic laser output"
require_text "$example_outputs_file" "selected laser family: obis" "OBIS generic laser output"
require_text "$example_outputs_file" "selected laser family: omicron" "Omicron generic laser output"
require_text "$example_outputs_file" "laser output request completed" "generic laser output completion"
require_text "$example_outputs_file" "laser disable completed" "generic laser disable completion"
require_text "$example_outputs_file" "selected fluidics controller: hamilton-mvp-hub" "fluidics generic workflow output"
require_text "$example_outputs_file" "valve select completed: map keys=[address, busy, initialized, port_count, position, valve_error, valve_type]" "fluidics completion output"
require_text "$example_outputs_file" "controller last_transaction: map keys=[command, completion_basis, reply_len, response]" "fluidics transaction readback output"
require_text "$example_outputs_file" "selected filter wheel: starlight-xpress-filter-wheel" "starlight filter workflow output"
require_text "$example_outputs_file" "last_transaction: map keys=[command, completion_basis, moving, position, positions]" "starlight filter transaction readback output"
require_text "$example_outputs_file" "selected filter wheel: prior-filter-wheel-1" "prior filter workflow output"
require_text "$example_outputs_file" "capabilities: wheel=FilterSelect request=FilterSelect" "filter typed capability output"
require_text "$run_examples_file" "filters [starlight\\|prior\\|ix85\\|kurios]" "run examples IX85 filter selector list"
require_text "$example_outputs_file" "selected filter selector: ix85-nosepiece [objective.turret, state.device]" "IX85 filter selector workflow output"
require_text "$example_outputs_file" "nosepiece_position: I64(2)" "IX85 filter selector readback output"
require_text "$example_outputs_file" "selected filter family: kurios" "KURIOS generic filter output"
require_text "$example_outputs_file" "selected tunable filter: thorlabs-kurios-lctf" "KURIOS tunable-filter output"
require_text "$example_outputs_file" "tunable filter disable completed: map keys=[output_enabled, steps]" "KURIOS disable completion output"
require_text "$example_outputs_file" "selected temperature controller: spark-temperature" "environment generic workflow output"
require_text "$example_outputs_file" "gas control completed: map keys=[co2_actual, co2_target, enabled, o2_actual]" "environment gas control output"
require_text "$example_outputs_file" "target: Temperature(Temperature { value: 36.5, unit: Celsius })" "environment typed temperature readback"
require_text "$example_outputs_file" "selected environment family: andor_sdk2" "Andor SDK2 environment workflow output"
require_text "$example_outputs_file" "selected environment family: andor_sdk3" "Andor SDK3 environment workflow output"
require_text "$example_outputs_file" "selected gas controller: none" "temperature-only environment workflow output"
require_text "$example_outputs_file" "temperature_control: String(\"-20\")" "Andor configured cooler target output"
require_text "${device_docs_dir}/andor-sdk2.md" "With \`connect=false\`, updates configured cooler state" "Andor SDK2 configured cooler control docs"
require_text "${device_docs_dir}/andor-sdk3.md" "With \`connect=false\`, updates configured cooler state" "Andor SDK3 configured cooler control docs"
require_text "$example_outputs_file" "selected plate transport: spark-mainboard" "plate-reader generic workflow output"
require_text "$example_outputs_file" "detector measure completed: map keys=[device, integration_time, signal, wavelength]" "plate-reader detector output"
require_text "$example_outputs_file" "camera binding completed: map keys=[bound, imaging_mode]" "plate-reader camera binding output"
require_text "$example_outputs_file" "selected detector: spark-fluorescence [detector.fluorescence, light.source]" "plate-reader fluorescence detector output"
require_text "$example_outputs_file" "detector measure completed: map keys=[device, enabled, integration_time, signal, wavelength]" "plate-reader fluorescence measurement output"
require_text "$example_outputs_file" "selected detector: spark-luminescence [detector.luminescence]" "plate-reader luminescence detector output"
require_text "$example_outputs_file" "detector measure completed: map keys=[device, enabled, integration_time, signal]" "plate-reader luminescence measurement output"
require_text "$example_outputs_file" "selected digital IO source: arduino" "Arduino generic digital IO output"
require_text "$example_outputs_file" "arduino-adc input_summary: map keys=[adc_channel, adc_count, digital_inputs, input_pullups]" "Arduino ADC/input readback output"
require_text "$example_outputs_file" "selected digital IO source: arduino_counter" "Arduino Counter generic digital IO output"
require_text "$example_outputs_file" "measure completed: map keys=[count, counter_summary, gate]" "Arduino Counter measure output"
require_text "$example_outputs_file" "selected digital IO source: esp32" "ESP32 generic digital IO output"
require_text "$example_outputs_file" "trigger source completed: map keys=[mask, triggered]" "ESP32 trigger-source completion output"
require_text "$example_outputs_file" "selected digital IO source: teensy_pulse" "Teensy Pulse generic digital IO output"
require_text "$example_outputs_file" "trigger source completed: map keys=[counted_pulses, running, triggered]" "Teensy Pulse trigger output"
require_text "$example_outputs_file" "selected digital IO source: triggerscope" "TriggerScope generic digital IO output"
require_text "$example_outputs_file" "triggerscope-dac-1 voltage: Voltage(Voltage { value: 1.386, unit: Volts })" "TriggerScope typed voltage readback"
require_text "$example_outputs_file" "triggerscope-hub last_transaction: map keys=[action, completion_basis, encoded_length]" "TriggerScope transaction readback output"
require_text "$example_outputs_file" "selected digital IO source: wosm" "WOSM generic digital IO output"
require_text "$example_outputs_file" "wosm-input digital_input: I64(0)" "WOSM digital input readback"
require_text "$example_outputs_file" "selected digital IO source: modbus" "Modbus mapped IO output"
require_text "$example_outputs_file" "modbus-mapped-io measured_register: I64(23)" "Modbus mapped readback output"
require_text "${driver_src_dir}/modbus.rs" '"serial_port".into()' "Modbus RTU serial-port resource metadata"
require_text "${driver_src_dir}/modbus.rs" '"tcp_host".into()' "Modbus TCP host resource metadata"
require_text "${driver_src_dir}/modbus.rs" '"connected".into()' "Modbus connected resource metadata"
require_text "${driver_src_dir}/modbus.rs" '"read_coils"' "Modbus raw read-coils command"
require_text "${driver_src_dir}/modbus.rs" '"read_discrete_inputs"' "Modbus raw read-discrete-inputs command"
require_text "${driver_src_dir}/modbus.rs" '"read_input_registers"' "Modbus raw read-input-registers command"
require_text "${driver_src_dir}/modbus.rs" '"write_single_coil"' "Modbus raw write-single-coil command"
require_text "${driver_src_dir}/modbus.rs" '"write_multiple_coils"' "Modbus raw write-multiple-coils command"
require_text "${driver_src_dir}/modbus.rs" '"write_multiple_registers"' "Modbus raw write-multiple-registers command"
require_text "${device_docs_dir}/modbus.md" 'resource metadata records configured RTU serial or TCP endpoint fields, response timeout, retry count, and `connected` state' "Modbus documented resource metadata"
require_text "${device_docs_dir}/modbus.md" '`write_multiple_registers`' "Modbus documented raw multiple-register write"
require_text "$evidence_file" 'resource metadata records configured RTU serial or TCP endpoint fields, response timeout, retry count, and `connected` state' "Modbus evidence resource metadata wording"
require_text "$evidence_file" 'mapped `RawRegisterAccess` covers standard read coils' "Modbus evidence raw access wording"
require_text "$example_outputs_file" "selected digital IO source: velleman" "Velleman generic digital IO output"
require_text "$example_outputs_file" "velleman-k8055-digital-output mask: I64(5)" "Velleman digital output readback"
require_text "$example_outputs_file" "velleman-k8055-hub last_transaction: map keys=[analog_input_1, analog_input_2, command, completion_basis, digital_input_mask]" "Velleman transaction readback output"
require_text "$example_outputs_file" "Configured Velleman K8055 IO board, 9 device(s), 1 resource(s)" "Velleman K8055 counter discovery output"
require_text "$example_outputs_file" "Configured Velleman K8061 IO board, 22 device(s), 1 resource(s)" "Velleman K8061 counter discovery output"
require_text "$example_outputs_file" "device: velleman-k8055-counter-1 [\"counter\", \"digital.input.counter\"]" "Velleman K8055 counter descriptor output"
require_text "$example_outputs_file" "device: velleman-k8061-counter-1 [\"counter\", \"digital.input.counter\"]" "Velleman K8061 counter descriptor output"
require_text "${device_docs_dir}/velleman.md" "| \`packet_len\` | Hub | \`I64\` | bytes | R | K8055 \`8\`; K8061 \`64\` |" "Velleman packet-length metadata"
require_text "${device_docs_dir}/velleman.md" "| \`packet_backend\` | Hub | \`String\` | none | R | configured \`ScriptedUsbPacket\` or \`nusb\` live packet backend |" "Velleman backend metadata"
require_text "${device_docs_dir}/velleman.md" "K8055/K8061 counter readback and K8055 debounce are implemented from open driver evidence; K8055/K8061 counter-reset operations remain hidden from regular and advanced command surfaces" "Velleman counter documentation"
require_text "${driver_src_dir}/velleman.rs" '"usb_vendor_id".into()' "Velleman USB vendor resource metadata"
require_text "${driver_src_dir}/velleman.rs" '"usb_transfer_kind".into()' "Velleman USB transfer resource metadata"
require_text "${driver_src_dir}/velleman.rs" "VELLEMAN_USB_VENDOR_ID" "Velleman active USB VID gate"
require_text "${driver_src_dir}/velleman.rs" "model_for_usb_product" "Velleman active USB PID model gate"
require_text "${driver_src_dir}/velleman.rs" '"usb_identity".into()' "Velleman USB identity metadata"
require_text "${driver_src_dir}/velleman.rs" 'Value::Bool(self.live_endpoint.is_some())' "Velleman connected resource metadata"
require_text "${driver_src_dir}/velleman.rs" "fn autodiscover_velleman_endpoint(" "Velleman connect-time endpoint autodiscovery"
require_text "${driver_src_dir}/velleman.rs" "active_configuration()" "Velleman USB active-configuration endpoint discovery"
require_text "${device_docs_dir}/velleman.md" 'resource metadata records packet style, backend, configured or descriptor-discovered USB VID/PID, optional USB identity, interface, IN/OUT endpoints, transfer kind, and `connected` state' "Velleman documented resource metadata"
require_text "$evidence_file" 'resource metadata records configured or descriptor-discovered USB VID/PID, optional USB identity, interface, IN/OUT endpoints, transfer kind, packet style, backend, and `connected` state' "Velleman evidence resource metadata wording"
require_text "${device_docs_dir}/velleman.md" '`connect=true` can open one matching configured board and select the single active-interface IN/OUT endpoint pair' "Velleman documented endpoint autodiscovery"
require_text "$evidence_file" '`connect=true` endpoint autodiscovery for one matching board using active USB descriptors' "Velleman evidence endpoint autodiscovery"
require_text "${driver_src_dir}/velleman.rs" "protocol::VellemanCommand::ReadK8061Counter" "Velleman K8061 counter command"
require_text "${driver_src_dir}/velleman.rs" "ResetK8061Counter" "Velleman K8061 counter-reset command evidence"
require_text "${device_docs_dir}/velleman.md" "counter-reset operations remain hidden from regular and advanced command surfaces" "Velleman hidden counter-reset note"
require_text "${driver_src_dir}/velleman.rs" "ResetK8055Counter" "Velleman K8055 counter-reset command evidence"
require_text "${device_docs_dir}/starlight-xpress.md" "real serial fails closed if readback is missing or movement does not resolve before the poll limit" "Starlight fail-closed completion documentation"
require_text "${driver_src_dir}/starlight_xpress.rs" "Starlight Xpress filter wheel did not report completion before poll limit" "Starlight fail-closed completion code"
require_text "${driver_src_dir}/starlight_xpress.rs" "driver.refresh_startup_state()?" "Starlight construction-time filter readback"
require_text "${driver_src_dir}/starlight_xpress.rs" "Starlight Xpress GenericCommand supports refresh_readbacks, refresh_position, and refresh_positions" "Starlight mapped GenericCommand validation"
require_text "${driver_src_dir}/starlight_xpress.rs" "unsupported Starlight Xpress filter wheel capability invocation" "Starlight dispatch fails closed for unsupported capability invocation"
require_text "${driver_src_dir}/starlight_xpress.rs" '"serial_port".into()' "Starlight serial-port resource metadata"
require_text "${driver_src_dir}/starlight_xpress.rs" '"usb_vendor_id".into()' "Starlight HID vendor-id resource metadata"
require_text "${driver_src_dir}/starlight_xpress.rs" '"connected".into(), Value::Bool(self.connected)' "Starlight connected resource metadata"
require_text "${device_docs_dir}/starlight-xpress.md" 'resource metadata records configured `serial_port`, `baud_rate`, and `connected` state' "Starlight serial resource metadata documentation"
require_text "${driver_src_dir}/starlight_xpress.rs" "fn autodiscover_hid_endpoint(" "Starlight HID autodiscovery"
require_text "${driver_src_dir}/starlight_xpress.rs" "enumerate_hid_devices()?" "Starlight HID identity enumeration"
require_text "${driver_src_dir}/starlight_xpress.rs" "multiple Starlight Xpress HID filter-wheel candidates found" "Starlight HID autodiscovery ambiguity gate"
require_text "${device_docs_dir}/starlight-xpress.md" 'resource metadata records explicit or autodiscovered `usb_vendor_id`, `usb_product_id`, `hid_report_id`, `hid_timeout`, optional `hid_serial_number`, and `connected` state' "Starlight HID resource metadata documentation"
require_text "${device_docs_dir}/starlight-xpress.md" '| `GenericCommand` | Filter wheel | `refresh_readbacks`, `refresh_position`, or `refresh_positions` with no params' "Starlight documented mapped GenericCommand"
require_text "$evidence_file" 'serial resource metadata records configured `serial_port`, `baud_rate`, and `connected` state' "Starlight evidence serial resource metadata wording"
require_text "$evidence_file" 'HID autodiscovery uses passive HID identity enumeration by SX/Starlight filter-wheel product strings plus optional serial-number filtering' "Starlight evidence HID autodiscovery wording"
require_text "$evidence_file" 'HID resource metadata records explicit or autodiscovered `usb_vendor_id`, `usb_product_id`, `hid_report_id`, `hid_timeout`, optional `hid_serial_number`, and `connected` state' "Starlight evidence HID resource metadata wording"
require_text "$evidence_file" 'named filter `GenericCommand` refresh helpers use documented select/current/total readback completion' "Starlight evidence mapped GenericCommand wording"
require_text "crates/numanager-core/src/hid.rs" "pub trait HidReportIo" "HID input/output report abstraction"
require_text "${driver_src_dir}/starlight_xpress.rs" "SxHidSerialAdapter" "Starlight HID report adapter"
require_text "${device_docs_dir}/starlight-xpress.md" "explicit-config or single-match autodiscovered USB HID input/output-report backend" "Starlight HID docs"
require_text "${device_docs_dir}/corvus.md" "relative moves encode the clamped delta while cached readback stores the final position" "Corvus relative-move delta documentation"
require_text "${driver_src_dir}/hamilton_mvp.rs" "Hamilton MVP port_count must be in 1..=8" "Hamilton port-count validation code"
require_text "${device_docs_dir}/hamilton-mvp.md" "invalid configured topologies are rejected instead of clamped" "Hamilton port-count validation documentation"
require_text "${device_docs_dir}/hamilton-mvp.md" "wrong type or invalid count/address" "Hamilton invalid config-type documentation"
require_text "${driver_src_dir}/hamilton_mvp.rs" "Hamilton MVP property {key} must be Bool" "Hamilton invalid bool config code"
require_text "${driver_src_dir}/hamilton_mvp.rs" "Hamilton MVP address must not be empty" "Hamilton empty address config code"
require_text "${driver_src_dir}/hamilton_mvp.rs" "driver.read_firmware_for(index)?" "Hamilton construction-time firmware readback"
require_text "${driver_src_dir}/hamilton_mvp.rs" "Hamilton MVP daisy-chain address count must be at most 16" "Hamilton daisy-chain count validation"
require_text "${driver_src_dir}/hamilton_mvp.rs" '"serial_port".into()' "Hamilton serial-port resource metadata"
require_text "${driver_src_dir}/hamilton_mvp.rs" '"connected".into(), Value::Bool(self.connected)' "Hamilton connected resource metadata"
require_text "${device_docs_dir}/hamilton-mvp.md" 'resource metadata records configured `serial_port`, fixed `baud_rate`, `serial_timeout`, and `connected` state' "Hamilton documented resource metadata"
require_text "$evidence_file" 'resource metadata records configured `serial_port`, fixed `baud_rate`, `serial_timeout`, and `connected` state' "Hamilton evidence resource metadata wording"
require_text "${device_docs_dir}/hamilton-mvp.md" 'property.addresses' "Hamilton daisy-chain address config documentation"
require_text "${driver_src_dir}/hamilton_mvp.rs" "\"read_done\" =>" "Hamilton done refresh command"
require_text "${driver_src_dir}/hamilton_mvp.rs" "\"read_position\" =>" "Hamilton position refresh command"
require_text "${driver_src_dir}/hamilton_mvp.rs" "\"read_valve_type\" =>" "Hamilton valve-type refresh command"
require_text "${driver_src_dir}/hamilton_mvp.rs" "\"read_valve_error\" =>" "Hamilton valve-error refresh command"
require_text "${driver_src_dir}/hamilton_mvp.rs" "Hamilton MVP GenericCommand commands do not accept params" "Hamilton generic command params gate"
require_text "${driver_src_dir}/hamilton_mvp.rs" "Hamilton MVP GenericCommand supports refresh_status, read_done, read_position, read_valve_type, and read_valve_error" "Hamilton mapped generic command validation"
require_text "${device_docs_dir}/hamilton-mvp.md" '`refresh_status`, `read_done`, `read_position`, `read_valve_type`, or `read_valve_error` with no params' "Hamilton documented mapped GenericCommand"
require_text "$evidence_file" 'hub `GenericCommand` exposes explicit address-aggregated `refresh_status`, `read_done`, `read_position`, `read_valve_type`, and `read_valve_error` helpers with no params' "Hamilton evidence mapped GenericCommand wording"
require_text "${driver_src_dir}/opentrons_ot2.rs" "Opentrons OT-2 api_version must be 2 or higher" "Opentrons API-version validation code"
require_text "${driver_src_dir}/opentrons_ot2.rs" "\"/health\"" "Opentrons active health probe code"
require_text "${driver_src_dir}/opentrons_ot2.rs" "\"refresh_health\" => self.refresh_health()" "Opentrons runtime health refresh command"
require_text "${driver_src_dir}/opentrons_ot2.rs" "\"refresh_inventory\" => self.refresh_inventory()" "Opentrons runtime inventory refresh command"
require_text "${driver_src_dir}/opentrons_ot2.rs" "\"refresh_current_run\" => self.refresh_current_run()" "Opentrons runtime current-run refresh command"
require_text "${driver_src_dir}/opentrons_ot2.rs" "\"refresh_run_commands\" => self.refresh_run_commands()" "Opentrons runtime run-command refresh command"
require_text "${driver_src_dir}/opentrons_ot2.rs" "\"play_run\" => self.run_action(\"play\")" "Opentrons constrained play-run action"
require_text "${driver_src_dir}/opentrons_ot2.rs" "\"pause_run\" => self.run_action(\"pause\")" "Opentrons constrained pause-run action"
require_text "${driver_src_dir}/opentrons_ot2.rs" "\"stop_run\" => self.run_action(\"stop\")" "Opentrons constrained stop-run action"
require_text "${driver_src_dir}/opentrons_ot2.rs" "\"/modules\"" "Opentrons module inventory refresh path"
require_text "${driver_src_dir}/opentrons_ot2.rs" "\"/runs\"" "Opentrons run inventory refresh path"
require_text "${driver_src_dir}/opentrons_ot2.rs" "module_status" "Opentrons first-module status readback code"
require_text "${driver_src_dir}/opentrons_ot2.rs" "first_json_number(&modules.body" "Opentrons first-module temperature readback code"
require_text "${driver_src_dir}/opentrons_ot2.rs" "CapabilityKind::TemperatureControl" "Opentrons TemperatureControl capability code"
require_text "${driver_src_dir}/opentrons_ot2.rs" '"command_type":"set_Temperature"' "Opentrons API v2 module temperature command"
require_text "${driver_src_dir}/opentrons_ot2.rs" '"command_type":"deactivate"' "Opentrons API v2 module deactivate command"
require_text "${driver_src_dir}/opentrons_ot2.rs" "Opentrons module direct command endpoint is removed for Opentrons-Version 3" "Opentrons API v3 module-command gate"
require_text "${driver_src_dir}/opentrons_ot2.rs" "\"/runs/{}/commands?pageLength=20\"" "Opentrons run-command refresh path construction"
require_text "${driver_src_dir}/opentrons_ot2.rs" "\"/runs/{}/actions\"" "Opentrons run-action path construction"
require_text "${device_docs_dir}/opentrons-ot2.md" 'Configured `opentrons-version` header; values below `2` are rejected' "Opentrons API-version validation documentation"
require_text "${device_docs_dir}/opentrons-ot2.md" '`refresh_inventory` performs read-only `GET /modules` and `GET /runs` requests' "Opentrons inventory refresh documentation"
require_text "${device_docs_dir}/opentrons-ot2.md" 'first-module model/serial/status/temperature' "Opentrons first-module readback documentation"
require_text "${device_docs_dir}/opentrons-ot2.md" 'API v2 `POST /modules/{serial}` with `set_Temperature` or `deactivate`' "Opentrons documented module temperature control"
require_text "${device_docs_dir}/opentrons-ot2.md" '`refresh_current_run` performs a read-only `GET /runs/{runId}` request' "Opentrons current-run refresh documentation"
require_text "${device_docs_dir}/opentrons-ot2.md" '`refresh_run_commands` performs a read-only' "Opentrons run-command refresh documentation"
require_text "${device_docs_dir}/opentrons-ot2.md" '`play_run`, `pause_run`, and `stop_run` perform `POST /runs/{runId}/actions`' "Opentrons run-action documentation"
require_text "$evidence_file" 'refresh_run_commands' "Opentrons run-command refresh evidence wording"
require_text "$evidence_file" '/runs/{runId}/commands' "Opentrons run-command endpoint evidence wording"
require_text "$evidence_file" 'first-module model, serial, status, current temperature, and target temperature' "Opentrons first-module readback evidence wording"
require_text "$evidence_file" 'temperature-module `TemperatureControl` plus writable `target_temperature`/`enabled` use API v2 `POST /modules/{serial}` with `set_Temperature`' "Opentrons evidence module temperature control wording"

require_artifact_summary_row "Okolab" "Reverse engineered"
require_artifact_summary_row "Agilent Laser Combiner" "Reverse engineered"
require_artifact_summary_row "MCL MicroDrive" "Reverse engineered"
require_artifact_summary_row "MCL NanoDrive" "Reverse engineered"
require_artifact_summary_row "ABS camera" "Reverse engineered"
require_artifact_summary_row "Mightex buffered camera SDK" "Reverse engineered"
require_artifact_summary_row "Mightex USB helper" "Reverse engineered"

for target in okolab agilent-laser-combiner mcl abs-camera mightex-camera mightex-bls; do
  require_file "${device_docs_dir}/${target}.md"
done

require_limited_device_page_shape agilent-laser-combiner.md
require_limited_device_page_shape mcl.md
require_limited_device_page_shape abs-camera.md
require_limited_device_page_shape mightex-camera.md

for target in okolab agilent-laser-combiner mcl abs-camera mightex; do
  require_file "${reverse_docs_dir}/${target}.md"
done

require_reverse_note_shape okolab.md
require_reverse_note_shape agilent-laser-combiner.md
require_reverse_note_shape mcl.md
require_reverse_note_shape abs-camera.md
require_reverse_note_shape mightex.md

require_text "$driver_lib" "pub mod okolab;" "Okolab driver export"
require_file "${driver_src_dir}/okolab.rs"
require_text "${driver_src_dir}/okolab.rs" "fn read_live_f64" "Okolab connected numeric readback"
require_text "${driver_src_dir}/okolab.rs" "fn generic_refresh_target" "Okolab mapped generic refresh mapping"
require_text "${driver_src_dir}/okolab.rs" "Okolab GenericCommand supports refresh_temperature_actual, refresh_temperature_target, refresh_temperature_status, refresh_co2_actual, refresh_co2_target, refresh_co2_status, refresh_o2_actual, refresh_o2_target, refresh_humidity, refresh_humidity_enabled, refresh_parameter, and write_parameter" "Okolab mapped generic command validation"
require_text "${driver_src_dir}/okolab.rs" "fn select_o2_setpoint_parameter" "Okolab O2 database parameter selection"
require_text "${driver_src_dir}/okolab.rs" "fn validate_cached_enable_write" "Okolab live temperature enable write gate"
require_text "${driver_src_dir}/okolab.rs" "fn select_gas_paused_parameter" "Okolab gas enable database parameter selection"
require_text "${driver_src_dir}/okolab.rs" "Okolab {module} enable writes require command evidence" "Okolab live enable write error"
require_text "${driver_src_dir}/okolab.rs" "fn load_dictionary_for_product" "Okolab database-backed parameter dictionary"
require_text "${driver_src_dir}/okolab.rs" "fn match_product_identity" "Okolab database-backed product identity matcher"
require_text "${driver_src_dir}/okolab.rs" "driver.refresh_connected_identity()?" "Okolab connected construction identity readback"
require_text "${driver_src_dir}/okolab.rs" "pub fn checksum16_signed" "Okolab signed 16-bit checksum helper"
require_text "${driver_src_dir}/okolab.rs" "out.push(b'#')" "Okolab checksum marker encoder"
require_text "${driver_src_dir}/okolab.rs" "pub fn reply_complete(bytes: &[u8], checksum: bool) -> bool" "Okolab checksum-aware frame read completion"
require_text "${driver_src_dir}/okolab.rs" '"serial_port".into()' "Okolab serial-port resource metadata"
require_text "${driver_src_dir}/okolab.rs" '"real_transport".into()' "Okolab live-transport resource metadata"
require_text "${device_docs_dir}/okolab.md" "Connected temperature/CO2 reads issue configured read frames" "Okolab connected readback documentation"
require_text "${device_docs_dir}/okolab.md" "gas enable writes use the inverted named \`Gas control paused\` database parameter when available" "Okolab gas enable write documentation"
require_text "${device_docs_dir}/okolab.md" "temperature enable writes are rejected on live serial because the recorded module abstraction lists no temperature paused property" "Okolab temperature enable write documentation"
require_text "${device_docs_dir}/okolab.md" "no arbitrary numeric command surface" "Okolab documented mapped generic command"
require_text "${device_docs_dir}/okolab.md" 'connected construction reads the configured `name_code`' "Okolab documented connected identity readback"
require_text "${device_docs_dir}/okolab.md" 'checksum mode uses the recorded `#` marker plus signed 16-bit trailer' "Okolab documented checksum implementation"
require_text "${device_docs_dir}/okolab.md" 'resource metadata records configured `serial_port`, primary/fallback baud rates, checksum mode, and opt-in live-transport state' "Okolab documented resource metadata"
require_text "${device_docs_dir}/okolab.md" '`environment_control okolab`' "Okolab environment-control example documentation"
require_text "$evidence_file" 'resource metadata records configured `serial_port`, primary/fallback baud rates, checksum mode, and opt-in live-transport state' "Okolab evidence resource metadata wording"
require_text "$evidence_file" "gas enable read/write through the inverted named \`Gas control paused\` database parameter when present" "Okolab evidence gas enable implementation"
require_text "$evidence_file" "O2 read/write through the selected product dictionary or configured O2 codes when available" "Okolab evidence O2 implementation"
require_text "$evidence_file" "temperature enable cached-only because the recorded module abstraction lists no temperature paused property" "Okolab evidence temperature enable boundary"
require_export abs_camera
require_export mcl
require_export mightex_camera
require_text "${driver_src_dir}/mcl.rs" "parse_microdrive_encoder_values" "MCL raw encoder parser"
require_text "${driver_src_dir}/mcl.rs" "raw_encoder_count" "MCL raw encoder property"
require_text "${driver_src_dir}/mcl.rs" "refresh_microdrive_readbacks" "MCL opt-in raw USB readback"
require_text "${driver_src_dir}/mcl.rs" "MicroDrive product IDs only" "MCL live readback family gate"
require_text "$driver_lib" "MclDiscovery::from_config" "MCL discovery registration"
require_text "$example_outputs_file" "Configured MCL reverse engineered support (Mad City Labs MicroDrive/NanoDrive), 4 device(s), 1 resource(s)" "MCL discovery output"
require_text "$example_outputs_file" "device: mcl-x [\"stage.axis\", \"stage.x\", \"reverse.engineered\"]" "MCL axis descriptor output"
require_text "$example_outputs_file" "added Configured MCL reverse engineered support (Mad City Labs MicroDrive/NanoDrive) with 4 device(s)" "MCL add output"
require_text "${driver_src_dir}/mcl.rs" '"usb_vendor_id".into()' "MCL USB vendor resource metadata"
require_text "${driver_src_dir}/mcl.rs" '"usb_in_endpoint".into()' "MCL USB endpoint resource metadata"
require_text "${driver_src_dir}/mcl.rs" '"connected".into(), Value::Bool(self.io.is_some())' "MCL connected resource metadata"
require_text "${device_docs_dir}/mcl.md" "Active USB descriptor discovery plus opt-in MicroDrive USB raw encoder/status readback, fixed-length raw MicroDrive control-read/action commands, and vendor firmware/runtime package identity/file-status/digest-state/probe surface" "MCL documented raw USB readback and package surface"
require_text "${device_docs_dir}/mcl.md" "Raw encoder/status" "MCL raw encoder/status evidence gate"
require_text "${device_docs_dir}/mcl.md" "discover_devices" "MCL documented discovery workflow"
require_text "${device_docs_dir}/mcl.md" "no stage motion capability is advertised" "MCL documented motion capability gate"
require_text "${device_docs_dir}/mcl.md" "Stage move/home APIs are not exposed because move payloads, units, and status polling are not evidenced" "MCL documented motion evidence gate"
require_text "${device_docs_dir}/mcl.md" 'resource metadata records configured or descriptor-discovered USB VID/PID, optional descriptor identity, interface, IN endpoint, active-discovery state, and `connected` state' "MCL documented resource metadata"
require_text "$evidence_file" 'resource metadata records configured or descriptor-discovered USB VID/PID, optional USB identity, interface, IN endpoint, active-discovery state, and `connected` state' "MCL evidence resource metadata wording"

reject_driver_test_path okolab
reject_driver_test_path agilent_laser_combiner
reject_driver_test_path mightex_camera
reject_driver_test_path mcl
reject_driver_test_path abs_camera
reject_driver_test_path mightex_bls
reject_inline_tests mightex_bls
reject_driver_crate_tests
reject_example_protocol_internals
reject_public_low_level_driver_protocol_exports
reject_noncanonical_example_enums
reject_config_example_raw_graph_ids
reject_gui_raw_capability_dispatch
reject_discover_devices_duplicate_driver_ids
require_hardware_checklist_consistency
reject_public_maintenance_generic_commands
reject_public_maintenance_command_docs
reject_hardware_validation_as_implementation_gate
reject_stale_public_control_gate_wording
reject_stale_inventory_implementation_notes
reject_stale_planning_maintenance_notes
reject_stale_device_support_notes
reject_stale_evidence_surface_labels
reject_driver_placeholder_markers
reject_driver_source_placeholder_language
require_core_maintenance_generic_command_gate
reject_hidden_maintenance_typed_capabilities
reject_hidden_maintenance_protocol_sends
reject_hidden_maintenance_variant_call_sites
require_driver_local_hidden_maintenance_guards
reject_unreviewed_maintenance_command_literals
reject_empty_driver_transaction_lists
reject_invoke_noop_catchalls
reject_writable_firmware_package_controls
reject_camera_byte_config_scalars
reject_toupcam_sensor_pixel_scalars
require_toupcam_stream_geometry_completion
require_toupcam_bayer_gate

require_export mightex_bls

if [ "$missing" -ne 0 ]; then
  exit 1
fi

printf 'Reverse-evidence boundary audit passed\n'
