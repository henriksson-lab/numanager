#!/usr/bin/env bash
#
# Extract the Okolab command dictionary from the third-party SQLite database
# into a checked-in JSON file that the driver embeds at compile time.
#
#   scripts/extract-okolab-db.sh            # regenerate the extract
#   scripts/extract-okolab-db.sh --check    # fail if the extract is stale
#
# Why: only the Okolab driver ever needed SQLite, and only to read this static
# vendor dictionary. Extracting it here makes sqlite3 a *maintainer-time* tool —
# numanager itself then has no SQLite dependency at build or run time, which is
# what lets it build on Windows (no system sqlite3) with no C toolchain.
#
# To update after a vendor database refresh: replace okolib.db, re-run this
# script, and review the JSON diff — it shows exactly what changed upstream.
#
# Requires sqlite3 >= 3.33 (for the JSON1 functions used below).

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
db="$repo_root/data/third_party/okolab/okolib.db"
out="$repo_root/data/third_party/okolab/okolib.json"

check_only=0
if [ "${1:-}" = "--check" ]; then
    check_only=1
elif [ $# -gt 0 ]; then
    printf 'usage: %s [--check]\n' "$0" >&2
    exit 2
fi

command -v sqlite3 >/dev/null 2>&1 || {
    printf 'error: sqlite3 not found; it is required to regenerate the extract\n' >&2
    exit 1
}
[ -f "$db" ] || {
    printf 'error: %s not found\n' "$db" >&2
    exit 1
}

# Records the source database so a stale extract is identifiable. Deliberately
# no timestamp: the output must be byte-reproducible for --check to work.
sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

# Products, each carrying its alternate names and its fully resolved parameter
# list. `productId IN (-1, P.id)` folds in the shared parameters, the INNER JOIN
# drops rows referencing parameters that do not exist (10 of them), and DISTINCT
# collapses parameters reachable through both paths — exactly what the driver's
# original query did. The order is the driver's `main DESC, name ASC`, with id
# as a final tiebreaker so the output is deterministic.
products_sql="
SELECT json_object(
  'id', P.id,
  'name', P.name,
  'name_code', P.name_code,
  'code_alt', P.code_alt,
  'alt_names', (
    SELECT json_group_array(alt_name) FROM (
      SELECT A.alt_name AS alt_name FROM AltName A
      WHERE A.productId = P.id AND A.alt_name IS NOT NULL
      ORDER BY A.alt_name ASC
    )
  ),
  'parameter_ids', (
    SELECT json_group_array(id) FROM (
      SELECT DISTINCT V.id AS id, V.main AS main, V.name AS name
      FROM ProductVar PV JOIN Parameters V ON V.id = PV.variablesId
      WHERE PV.productId IN (-1, P.id)
      ORDER BY V.main DESC, V.name ASC, V.id ASC
    )
  )
)
FROM Product P ORDER BY P.id ASC;"

# Every parameter, keyed by id. Kept whole rather than filtered to the ones
# currently referenced, so a database refresh that adds a product mapping does
# not silently find its parameter missing.
parameters_sql="
SELECT json_object(
  'id', id,
  'name', name,
  'unit', unit,
  'description', description,
  'var_type', var_type,
  'main', main,
  'advanced', advanced,
  'oneshot', oneshot,
  'read_code', read_code,
  'write_code', write_code,
  'write_code_ram', write_code_ram,
  'min_code', min_code,
  'max_code', max_code,
  'enum_type_id', enum_type_id
)
FROM Parameters ORDER BY id ASC;"

# Enum value tables, one row per enum type.
enums_sql="
SELECT json_object(
  'enum_type_id', enum_type_id,
  'values', (
    SELECT json_group_array(json_object('value', enum_value, 'name', enum_name))
    FROM (
      SELECT E.enum_value AS enum_value, E.enum_name AS enum_name
      FROM EnumValues E
      WHERE E.enum_type_id = O.enum_type_id
      ORDER BY E.enum_value ASC
    )
  )
)
FROM (SELECT DISTINCT enum_type_id FROM EnumValues) O
ORDER BY O.enum_type_id ASC;"

# One record per line, comma-separated: keeps vendor-database updates reviewable
# as line diffs instead of one enormous changed line.
emit_rows() {
    sqlite3 "$db" "$1" |
        awk 'NR > 1 { printf ",\n" } { printf "    %s", $0 } END { if (NR) printf "\n" }'
}

generate() {
    printf '{\n'
    printf '  "source": {"file": "okolib.db", "sha256": "%s"},\n' "$(sha256 "$db")"
    printf '  "products": [\n'
    emit_rows "$products_sql"
    printf '  ],\n'
    printf '  "parameters": [\n'
    emit_rows "$parameters_sql"
    printf '  ],\n'
    printf '  "enums": [\n'
    emit_rows "$enums_sql"
    printf '  ]\n'
    printf '}\n'
}

if [ "$check_only" -eq 1 ]; then
    tmp="$(mktemp)"
    trap 'rm -f "$tmp"' EXIT
    generate > "$tmp"
    if ! diff -u "$out" "$tmp"; then
        printf '\nerror: %s is stale; re-run scripts/extract-okolab-db.sh\n' \
            "${out#"$repo_root"/}" >&2
        exit 1
    fi
    printf 'okolab extract is up to date\n'
else
    generate > "$out"
    printf 'wrote %s\n' "${out#"$repo_root"/}"
fi
