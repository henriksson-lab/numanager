# Okolab Command Database

`okolib.db` is shipped as third-party Okolab data for protocol command lookup.
It is not licensed under this repository's MIT or Apache-2.0 license terms.

Do not treat this file as project source code. Changes to it should preserve
the upstream database contents and should be reviewed as third-party data
updates.

`okolib.json` is a derived extract of that database and carries the same
third-party terms. The driver embeds it at compile time, so **the extracted
vendor data is compiled into every binary built from this repository** — worth
confirming against Okolab's terms before redistributing builds.

Current files:

| File | Status |
| --- | --- |
| `okolib.db` | Third-party Okolab database; excluded from the repository license |
| `okolib.json` | Generated extract of `okolib.db`; same third-party terms |

## Regenerating the extract

`okolib.json` is what the Okolab driver actually reads — nothing at build or run
time opens the `.db`. That is deliberate: reading SQLite directly would put a
SQLite dependency on the whole crate, which does not link on Windows (no system
`sqlite3`) and pulls a C toolchain into every downstream build, all for one
driver's static lookup table.

After replacing `okolib.db` with a newer vendor database:

```sh
scripts/extract-okolab-db.sh          # rewrite okolib.json
scripts/extract-okolab-db.sh --check  # verify the extract matches the database
cargo test -p numanager-drivers okolab
```

The extract is line-per-record and sorted, so a vendor update shows up as a
readable diff. It records the source database's SHA-256, and `--check` fails
when the two have drifted apart. `sqlite3` (>= 3.33) is needed only to
regenerate — never to build or run numanager.
