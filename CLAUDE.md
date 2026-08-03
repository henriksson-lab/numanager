# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

`numanager` is a pure-Rust, SDK-free lab/microscope hardware control substrate
(a next-generation Micro-Manager). It is a driver collection plus a small
runtime — there is deliberately no "core" device model like `CMMCore`, and no
GUI beyond a test harness. See `README.md` (user view) and `DESIGN.md`
(architecture rationale).

## Commands

```sh
cargo check --workspace --all-targets     # fast compile check
cargo test --workspace                    # all tests (see note below)
cargo fmt
cargo clippy --workspace --all-targets
bash scripts/audit-reverse-evidence-boundary.sh   # repo-policy audit; REQUIRES ripgrep (rg)
```

Tests: every test in the workspace lives in `numanager-spectra`. `numanager-core`,
`numanager-drivers`, and `numanager-examples` have none by design (see
"Driver evidence policy"). Run a single test with
`cargo test -p numanager-spectra <name>`; parquet store tests need
`--features store`.

`scripts/audit-reverse-evidence-boundary.sh` depends on `rg`. If ripgrep is missing it
prints `rg: command not found` and its checks silently pass — install ripgrep
before trusting a green audit.

Examples (each is a `[[bin]]`, most take a driver-family argument):

```sh
cargo run -p numanager-examples --bin camera_acquisition [toupcam|toupcam-live|platform|gige|usb3|genicam]
cargo run -p numanager-examples --bin motion_stage [asi|zaber|pi-gcs|...]
cargo run -p numanager-examples --features gui --bin software_gui -- --smoke   # headless GUI check
```

`README.md` has the full example table. Feature flags: `os-serial`, `os-hid`,
and `os-usb` gate real OS transports (default off; drivers otherwise run configured/fixture
paths), `gui` gates the Slint software-test GUI, and `numanager-spectra` has
`store` (polars/parquet) and `fetch` (network download).

## Architecture

Four workspace crates, strictly layered:

- **`numanager-core`** — the whole model and runtime. `lib.rs` (~100k) holds the
  device graph, typed `Value` quantities, `PropertySchema`, `CapabilityKind`/
  `CapabilityRequest`, `Command`, `TimingPlan`, and the `Transport`/`Session`/
  `Driver` traits. `runtime.rs` holds `LocalRuntime`, the `Runtime` and
  `DriverDiscovery` traits, event bus, operation table, and frame store.
  `config.rs` holds `HardwareConfig` (TOML-shaped declarative topology plus
  discovery lock). `serial.rs`/`hid.rs`/`usb.rs` are thin `*Io` trait
  abstractions with `cfg(feature)`-gated OS backends.
- **`numanager-drivers`** — one flat module per device family (~50 modules), all
  re-exported from `lib.rs`. Deliberately a single consolidated crate; the audit
  script rejects re-splitting it into per-driver crates.
- **`numanager-spectra`** — independent: filter/fluorophore spectral curves,
  parquet store, FPbase fetcher. Not part of the device runtime.
- **`numanager-examples`** — user-facing API documentation in executable form,
  plus shared display helpers in `src/lib.rs`.

### Model

Hardware is a **DAG**, not a device list: resources (exclusive I/O things) →
hubs (drivers owning resources) → logical devices (`DeviceDescriptor` with kind
tags such as `camera`, `axis.xy`, `light.source`) → capabilities
(`CapabilityDescriptor`, e.g. `CameraCapture`, `StageMove`, `Autofocus`) →
operations. One physical controller commonly exposes many logical devices, and a
driver may coalesce cross-device commands into one `PhysicalTransaction`.

Composed/meta devices (software autofocus, `sim.rs`) are ordinary drivers that
declare `UsesDevice` graph edges with roles (`Camera`, `ZStage`, …); clients find
them via `capability_providers()` and dependency role, never raw node IDs.

### Driver module shape

Each driver module follows the same layout:

- `<X>ConfiguredProbe` — declarative/fixture configuration, with a `fixture()`
  constructor used by examples and the GUI.
- `<X>Discovery: DriverDiscovery` — stage 1 of two-stage discovery, returning
  `DriverCandidate`s that a UI or config claims before the driver is added.
- `<X>Driver: Driver` — `descriptors()`, `capabilities()`, `prepare()`,
  `dispatch()`, `poll()`, plus timing-plan hooks.
- a private `mod protocol` — byte/command encoding. Implementation detail:
  never exported as a user-facing API, never used from examples, and kept out of
  generated docs.

Applications go through `LocalRuntime`: submit a typed `CapabilityRequest` via
`submit_request()`/`execute_request()` and let the runtime infer the capability
kind; explicit `CapabilityKind` submission is for no-request operations (home,
stop) and ambiguous bring-up. Property/range validation happens in the runtime in
canonical units before a command reaches a driver; dynamic hardware constraints
stay inside the driver.

## Repo rules

`AGENTS.md` is the authoritative rule file — read it before touching drivers.
The load-bearing parts:

**Driver evidence policy.** Every command, reply, state transition, and unit
conversion must trace to manufacturer documentation, a public standard, open
firmware / audited open SDK source, captured hardware traffic, or a documented
bench run — recorded in `docs/devices/<device>.md` and the register in
`docs/devices/evidence.md`. Micro-Manager source may guide investigation but is
not sufficient evidence on its own. If no such evidence exists, mark the behavior
unknown/pending rather than inventing it.

**No driver tests.** Do not write tests for `numanager-drivers` — no inline
`#[cfg(test)]`, no `tests/` files, no encoder-matches-decoder or scripted-serial
fixtures. The audit script enforces this; do not route around it by relocating
the same self-confirming checks. Evidence belongs in device pages and hardware
validation notes (`docs/devices/hardware-validation-template.md`), not in code.

**Examples are public API docs.** They must use only public runtime, device,
property, capability, discovery, and timing APIs — no raw serial commands, no
`::protocol` access, no scripted transports, no `GenericCommand` (except the
`NUMANAGER_MIGHTEX_OUTPUT`-gated path in `light_source.rs`).

**Naming.** Public property keys are `snake_case` with no unit suffix when the
value type carries the unit (`exposure`, not `exposure_s`). Public physical
quantities use typed `Value` variants (`TimeInterval`, `Frequency`, `Decibel`,
`PixelCount`, `Ratio`, `NumericalAperture`) rather than naked scalars; convert to
protocol scalars only at the hardware boundary. Public enum strings are canonical
Rust-style (`Mono8`, `Raw16`, `Rgb8`, `Native`); native protocol spellings may be
accepted as aliases or kept in metadata.

**Docs move with code.** A driver change generally also touches its
`docs/devices/<device>.md` page, the `docs/devices/evidence.md` row, the
`docs/devices/README.md` and `README.md` index rows, and — if example output
changed — `docs/example_outputs.md`. `scripts/audit-reverse-evidence-boundary.sh`
asserts exact strings in those files (device-index rows, evidence rows, recorded
discovery counts, recorded frame lines), so run it after such changes.

**Reverse-evidence targets.** Lack of information is not a repo-level guardrail
against implementation attempts. If a driver cannot be completed from available
evidence, the code should fail explicitly for unsupported operations and the
docs should record the unknowns. The hard guardrail is evidence hygiene: do not
invent behavior, do not add self-confirming driver tests, and do not claim
hardware validation until linked validation evidence exists.
If firmware, a loader, or a vendor runtime package is required, ship or load the
original vendor package as third-party excluded data behind an optional backend
until a project-owned firmware/open replacement exists. Treat this as the
default implementation path, not an exception. Record the license boundary,
package identity, upstream package/version, SHA-256 digest, and platform; load
or read the package only on demand through explicit configuration; do not block
implementation solely because replacement firmware is not ready.
