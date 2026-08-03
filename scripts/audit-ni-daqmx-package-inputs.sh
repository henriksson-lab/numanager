#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: scripts/audit-ni-daqmx-package-inputs.sh <installer-file-or-directory>

Inventories user-provided NI-DAQmx installer/package inputs for evidence intake.
The output is Markdown intended for docs/devices or validation notes. This
script records package file identity and, where tools are available, package
metadata plus embedded license/copyright file identities. It does not prove SDK
header contents, FFI binding correctness, runtime loading, redistribution
permission, or hardware behavior.
USAGE
}

if [[ $# -ne 1 ]]; then
  usage
  exit 2
fi

root=$1
if [[ ! -e "$root" ]]; then
  echo "NI-DAQmx package path does not exist: $root" >&2
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

file_type() {
  if command -v file >/dev/null 2>&1; then
    file -b "$1"
  else
    echo "unknown; file(1) is not installed"
  fi
}

markdown_escape() {
  sed 's/|/\\|/g'
}

audit_deb() {
  local deb=$1
  local label=$2

  echo
  echo "### \`$label\`"

  if ! command -v dpkg-deb >/dev/null 2>&1; then
    echo
    echo "dpkg-deb is not installed; Debian package metadata and license files were not inspected."
    return
  fi

  local package_name package_version package_arch package_maintainer package_homepage
  package_name=$(dpkg-deb -f "$deb" Package 2>/dev/null || true)
  package_version=$(dpkg-deb -f "$deb" Version 2>/dev/null || true)
  package_arch=$(dpkg-deb -f "$deb" Architecture 2>/dev/null || true)
  package_maintainer=$(dpkg-deb -f "$deb" Maintainer 2>/dev/null || true)
  package_homepage=$(dpkg-deb -f "$deb" Homepage 2>/dev/null || true)

  echo
  echo "| Field | Value |"
  echo "| --- | --- |"
  echo "| Package | \`${package_name:-unknown}\` |"
  echo "| Version | \`${package_version:-unknown}\` |"
  echo "| Architecture | \`${package_arch:-unknown}\` |"
  echo "| Maintainer | $(printf '%s' "${package_maintainer:-unknown}" | markdown_escape) |"
  echo "| Homepage | $(printf '%s' "${package_homepage:-unknown}" | markdown_escape) |"

  local extract_dir
  extract_dir=$(mktemp -d "$tmp_dir/deb.XXXXXX")
  if ! dpkg-deb -x "$deb" "$extract_dir" >/dev/null 2>&1; then
    echo
    echo "Package payload could not be extracted for license-file inventory."
    return
  fi

  local license_list
  license_list=$(mktemp "$tmp_dir/licenses.XXXXXX")
  find "$extract_dir" -type f | LC_ALL=C sort |
    while IFS= read -r candidate; do
      local lower
      lower=$(basename "$candidate" | tr '[:upper:]' '[:lower:]')
      case "$lower" in
        *license*|*eula*|*copyright*) printf '%s\n' "$candidate" ;;
      esac
    done >"$license_list"

  if [[ ! -s "$license_list" ]]; then
    echo
    echo "No embedded license, EULA, or copyright files were found in the package payload."
    return
  fi

  echo
  echo "| SHA-256 | Bytes | Payload path |"
  echo "| --- | ---: | --- |"
  while IFS= read -r license_file; do
    local digest size rel
    digest=$(hash_file "$license_file")
    size=$(wc -c <"$license_file" | tr -d ' ')
    rel=${license_file#"$extract_dir"/}
    echo "| \`$digest\` | $size | \`$rel\` |"
  done <"$license_list"
}

audit_extracted_license_files() {
  local extract_dir=$1
  local package_kind=$2

  local license_list
  license_list=$(mktemp "$tmp_dir/licenses.XXXXXX")
  find "$extract_dir" -type f | LC_ALL=C sort |
    while IFS= read -r candidate; do
      local lower
      lower=$(basename "$candidate" | tr '[:upper:]' '[:lower:]')
      case "$lower" in
        *license*|*eula*|*copyright*) printf '%s\n' "$candidate" ;;
      esac
    done >"$license_list"

  if [[ ! -s "$license_list" ]]; then
    echo
    echo "No embedded license, EULA, or copyright files were found in the $package_kind payload."
    return
  fi

  echo
  echo "| SHA-256 | Bytes | Payload path |"
  echo "| --- | ---: | --- |"
  while IFS= read -r license_file; do
    local digest size rel
    digest=$(hash_file "$license_file")
    size=$(wc -c <"$license_file" | tr -d ' ')
    rel=${license_file#"$extract_dir"/}
    echo "| \`$digest\` | $size | \`$rel\` |"
  done <"$license_list"
}

audit_rpm() {
  local rpm_file=$1
  local label=$2
  local rpm_path
  rpm_path=$(readlink -f "$rpm_file" 2>/dev/null || printf '%s' "$rpm_file")

  echo
  echo "### \`$label\`"
  echo

  if command -v rpm >/dev/null 2>&1; then
    local package_name package_version package_release package_arch package_license package_url
    package_name=$(rpm -qp --queryformat '%{NAME}' "$rpm_path" 2>/dev/null || true)
    package_version=$(rpm -qp --queryformat '%{VERSION}' "$rpm_path" 2>/dev/null || true)
    package_release=$(rpm -qp --queryformat '%{RELEASE}' "$rpm_path" 2>/dev/null || true)
    package_arch=$(rpm -qp --queryformat '%{ARCH}' "$rpm_path" 2>/dev/null || true)
    package_license=$(rpm -qp --queryformat '%{LICENSE}' "$rpm_path" 2>/dev/null || true)
    package_url=$(rpm -qp --queryformat '%{URL}' "$rpm_path" 2>/dev/null || true)

    echo "| Field | Value |"
    echo "| --- | --- |"
    echo "| Package | \`${package_name:-unknown}\` |"
    echo "| Version | \`${package_version:-unknown}\` |"
    echo "| Release | \`${package_release:-unknown}\` |"
    echo "| Architecture | \`${package_arch:-unknown}\` |"
    echo "| License tag | $(printf '%s' "${package_license:-unknown}" | markdown_escape) |"
    echo "| URL | $(printf '%s' "${package_url:-unknown}" | markdown_escape) |"
  else
    echo "rpm is not installed; RPM package metadata fields were not inspected."
  fi

  if ! command -v rpm2cpio >/dev/null 2>&1 || ! command -v cpio >/dev/null 2>&1; then
    echo
    echo "rpm2cpio and cpio are not both installed; RPM payload license files were not inspected."
    return
  fi

  local extract_dir
  extract_dir=$(mktemp -d "$tmp_dir/rpm.XXXXXX")
  if ! (cd "$extract_dir" && rpm2cpio "$rpm_path" | cpio -idm --quiet) >/dev/null 2>&1; then
    echo
    echo "RPM payload could not be extracted for license-file inventory."
    return
  fi

  audit_extracted_license_files "$extract_dir" "RPM"
}

first_field_from_file() {
  local pattern=$1
  local file=$2
  awk -F'= ' -v pattern="$pattern" '$0 ~ pattern { print $2; exit }' "$file"
}

first_comment_from_file() {
  local pattern=$1
  local file=$2
  awk -v pattern="$pattern" '
    $0 ~ pattern {
      sub(/^[[:space:]]*/, "")
      print
      exit
    }
  ' "$file"
}

audit_windows_installer() {
  local exe=$1
  local label=$2

  echo
  echo "### \`$label\`"

  if ! command -v 7z >/dev/null 2>&1; then
    echo
    echo "7z is not installed; Windows PE installer payload was not inspected."
    return
  fi

  local listing
  listing=$(mktemp "$tmp_dir/7z-listing.XXXXXX")
  if ! 7z l "$exe" >"$listing"; then
    echo
    echo "7z could not list this Windows installer."
    return
  fi

  local pe_type cpu image_version file_version product_version product_name company_name
  pe_type=$(first_field_from_file '^Type = ' "$listing")
  cpu=$(first_field_from_file '^CPU = ' "$listing")
  image_version=$(first_field_from_file '^Image Version = ' "$listing")
  file_version=$(first_comment_from_file '^FileVersion:' "$listing")
  product_version=$(first_comment_from_file '^ProductVersion:' "$listing")
  product_name=$(first_comment_from_file '^ProductName:' "$listing")
  company_name=$(first_comment_from_file '^CompanyName:' "$listing")

  echo
  echo "| Field | Value |"
  echo "| --- | --- |"
  echo "| PE type | \`${pe_type:-unknown}\` |"
  echo "| CPU | \`${cpu:-unknown}\` |"
  echo "| Image version | \`${image_version:-unknown}\` |"
  echo "| File version | $(printf '%s' "${file_version:-unknown}" | markdown_escape) |"
  echo "| Product version | $(printf '%s' "${product_version:-unknown}" | markdown_escape) |"
  echo "| Product name | $(printf '%s' "${product_name:-unknown}" | markdown_escape) |"
  echo "| Company name | $(printf '%s' "${company_name:-unknown}" | markdown_escape) |"

  echo
  echo "| Embedded path | Type | Size |"
  echo "| --- | --- | ---: |"
  awk '
    function flush() {
      if (path != "") {
        printf("| `%s` | `%s` | %s |\n", path, type, size)
        path=""
        type=""
        size=""
      }
    }
    /^Path = \.rsrc/ {
      if (path != "" && path != $3) {
        flush()
      }
      path=$3
      type=""
      next
    }
    path != "" && /^Type = / {
      type=$3
      next
    }
    path != "" && /^Size = / {
      size=$3
      next
    }
    END { flush() }
  ' "$listing"

  local extract_dir
  extract_dir=$(mktemp -d "$tmp_dir/pe.XXXXXX")
  if ! 7z x -y "-o$extract_dir" "$exe" >/dev/null; then
    echo
    echo "7z could not extract this Windows installer payload."
    return
  fi

  local payload
  payload=$(find "$extract_dir" -maxdepth 1 -type f -name 'NIPKG_PAYLOAD~' -print -quit)
  if [[ -z "$payload" ]]; then
    echo
    echo "No extracted \`NIPKG_PAYLOAD~\` file was found."
    return
  fi

  echo
  echo "| Payload file | SHA-256 | Bytes | Type |"
  echo "| --- | --- | ---: | --- |"
  echo "| \`NIPKG_PAYLOAD~\` | \`$(hash_file "$payload")\` | $(wc -c <"$payload" | tr -d ' ') | $(file_type "$payload" | markdown_escape) |"

  echo
  echo "| Payload entry | Bytes |"
  echo "| --- | ---: |"
  7z l "$payload" |
    awk '
      /^[0-9-]+[[:space:]]+[0-9:]+[[:space:]]+/ && $3 ~ /^(D|\.)\.\.\.\.$/ {
        size=$4
        name=$6
        for (i=7; i<=NF; i++) {
          name=name " " $i
        }
        if (name != "") {
          gsub(/\|/, "\\|", name)
          printf("| `%s` | %s |\n", name, size)
        }
      }
    '

  local license_list
  license_list=$(mktemp "$tmp_dir/pe-licenses.XXXXXX")
  find "$extract_dir" -type f | LC_ALL=C sort |
    while IFS= read -r candidate; do
      local lower
      lower=$(basename "$candidate" | tr '[:upper:]' '[:lower:]')
      case "$lower" in
        *license*|*eula*|*copyright*) printf '%s\n' "$candidate" ;;
      esac
    done >"$license_list"

  if [[ ! -s "$license_list" ]]; then
    echo
    echo "No standalone license, EULA, or copyright files were found in the extracted Windows installer payload."
    return
  fi

  echo
  echo "| SHA-256 | Bytes | Payload path |"
  echo "| --- | ---: | --- |"
  while IFS= read -r license_file; do
    local digest size rel
    digest=$(hash_file "$license_file")
    size=$(wc -c <"$license_file" | tr -d ' ')
    rel=${license_file#"$extract_dir"/}
    echo "| \`$digest\` | $size | \`$rel\` |"
  done <"$license_list"
}

tmp_list=$(mktemp)
tmp_dir=$(mktemp -d)
trap 'rm -f "$tmp_list"; rm -rf "$tmp_dir"' EXIT

if [[ -f "$root" ]]; then
  printf '%s\n' "$root" >"$tmp_list"
else
  find "$root" -maxdepth 1 -type f | sort >"$tmp_list"
fi

if [[ ! -s "$tmp_list" ]]; then
  echo "No package files found under: $root" >&2
  exit 1
fi

echo "# NI-DAQmx Package Input Inventory"
echo
echo "| Item | Value |"
echo "| --- | --- |"
echo "| Input path | \`$root\` |"
echo "| Package file count | $(wc -l <"$tmp_list" | tr -d ' ') |"
echo
echo "## Package Files"
echo
echo "| SHA-256 | Bytes | Path | Type |"
echo "| --- | ---: | --- | --- |"
while IFS= read -r package; do
  digest=$(hash_file "$package")
  size=$(wc -c <"$package" | tr -d ' ')
  kind=$(file_type "$package" | sed 's/|/\\|/g')
  echo "| \`$digest\` | $size | \`$package\` | $kind |"
done <"$tmp_list"

echo
echo "## Archive Contents"
while IFS= read -r package; do
  case "$package" in
    *.zip|*.ZIP)
      echo
      echo "### \`$package\`"
      if command -v unzip >/dev/null 2>&1; then
        echo
        echo "| Bytes | Entry |"
        echo "| ---: | --- |"
        unzip -l "$package" |
          awk '
            /^[[:space:]]*[0-9]+[[:space:]]+[0-9-]+[[:space:]]+[0-9:]+[[:space:]]+/ {
              size=$1
              name=$4
              for (i=5; i<=NF; i++) {
                name=name " " $i
              }
              if (name != "") {
                gsub(/\|/, "\\|", name)
                printf("| %s | `%s` |\n", size, name)
              }
            }
          '
      else
        echo
        echo "unzip is not installed; archive entries were not listed."
      fi
      ;;
  esac
done <"$tmp_list"

echo
echo "## Debian Package Metadata And License Files"
while IFS= read -r package; do
  case "$package" in
    *.deb|*.DEB)
      audit_deb "$package" "$package"
      ;;
    *.zip|*.ZIP)
      if command -v unzip >/dev/null 2>&1; then
        while IFS= read -r entry; do
          case "$entry" in
            *.deb|*.DEB)
              deb_extract="$tmp_dir/$(basename "$entry")"
              if unzip -p "$package" "$entry" >"$deb_extract"; then
                audit_deb "$deb_extract" "$package::$entry"
              else
                echo
                echo "### \`$package::$entry\`"
                echo
                echo "Could not extract this Debian package from the zip archive."
              fi
              ;;
          esac
        done < <(unzip -Z1 "$package")
      fi
      ;;
  esac
done <"$tmp_list"

echo
echo "## RPM Package Metadata And License Files"
while IFS= read -r package; do
  case "$package" in
    *.rpm|*.RPM)
      audit_rpm "$package" "$package"
      ;;
    *.zip|*.ZIP)
      if command -v unzip >/dev/null 2>&1; then
        while IFS= read -r entry; do
          case "$entry" in
            *.rpm|*.RPM)
              rpm_extract="$tmp_dir/$(basename "$entry")"
              if unzip -p "$package" "$entry" >"$rpm_extract"; then
                audit_rpm "$rpm_extract" "$package::$entry"
              else
                echo
                echo "### \`$package::$entry\`"
                echo
                echo "Could not extract this RPM package from the zip archive."
              fi
              ;;
          esac
        done < <(unzip -Z1 "$package")
      fi
      ;;
  esac
done <"$tmp_list"

echo
echo "## Windows Installer Payload Inventory"
while IFS= read -r package; do
  case "$package" in
    *.exe|*.EXE)
      audit_windows_installer "$package" "$package"
      ;;
  esac
done <"$tmp_list"

echo
echo "## Evidence Boundary"
echo
echo "This inventory records local package identities, zip archive entries, package"
echo "metadata, and embedded license/copyright file identities where available. It"
echo "is not an SDK header audit, bindgen source audit, runtime probe, legal"
echo "redistribution determination, or hardware validation. Audit installed headers"
echo "with scripts/audit-ni-daqmx-sdk-headers.sh, audit the local FFI fork with"
echo "scripts/audit-ni-daqmx-sys-source.sh, review exact license terms before"
echo "redistributing any NI files, and keep live task execution disabled until bench"
echo "validation records real device behavior."
