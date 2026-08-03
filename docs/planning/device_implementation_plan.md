# Device Implementation Plan

This plan turns the protocol-evidence inventory into implementation work. It
prioritizes drivers with manufacturer manuals, public standards, open firmware,
or clear command protocols. Reverse-engineered fallback evidence is explicitly last-resort
work for small, high-value devices where no better source exists.

Repository rule: driver test policy is defined in `AGENTS.md`. Do not generate
driver tests for hardware drivers, and do not add self-confirming protocol
fixtures. Evidence belongs in source audits, device pages, trace notes,
hardware-validation notes, and bench logs.

## Principles

1. Prefer original manufacturer protocol manuals, public standards, or open
   firmware over compatibility-adapter behavior.
2. Use compatibility evidence as secondary evidence for working defaults,
   device variants, quirks, and command ordering.
3. Do not treat self-authored fixtures as evidence. Scripted replies, parser
   round-trips, and command-byte snapshots are only useful when they are derived
   from manufacturer documentation, public standards, open firmware, captured
   hardware traces, or real hardware runs.
4. Delete protocol scaffolding that cannot be tied to an external source or a
   concrete runtime behavior. It creates maintenance load without increasing
   confidence.
5. Avoid proprietary SDK dependencies in normal drivers.
6. Treat open SDKs differently from closed SDKs: open source can be audited,
   vendored, or used as protocol evidence if the license permits.
7. When a device requires vendor firmware, a loader, or a runtime package,
   the interim repository solution is to ship the original vendor package as
   third-party excluded data when redistribution terms permit it, or load a
   user-configured local copy when they do not, behind an explicit optional
   backend until a project-owned firmware or open replacement exists. Treat this
   as the default implementation path for every firmware-dependent device, not a
   reason to omit protocol-supported behavior. Record file identity, upstream
   package/version, digest, platform, redistribution status, and license
   boundary under `data/third_party/`, load or read the package only on demand
   through explicit configuration, and do not infer protocol behavior from
   package presence alone.
8. Do not use reverse-engineered fallback evidence as a first strategy for camera SDKs. For cameras,
   prefer standards such as GenICam/GigE Vision/USB3 Vision, OS camera stacks,
   or existing open implementations.
9. Every hardware driver must define:
   - detection/probe behavior;
   - advertised devices and properties;
   - command serialization/remultiplexing rules;
   - hardware-driven completion;
   - event/frame/status delivery;
   - safety states for motion, laser, voltage, temperature, pressure, or gas.
10. Documentation should keep `README.md` as a concise project entry point. The
   README should contain a linked table of supported devices/drivers, while
   detailed per-device documentation should live in separate device pages under
   `docs/devices/`.
11. Keep device drivers in one `numanager-drivers` crate. Device families should
    be modules of that crate, not separate workspace crates.

Evidence policy:

- A driver feature is supported only when its protocol behavior has one of these
  evidence sources:
  - manufacturer manual, command reference, public standard, or open firmware;
  - audited vendor/open SDK source or headers with a compatible license;
  - hardware traffic capture or bench run recorded in a device evidence note;
  - reverse engineered compatibility evidence as secondary reference material,
    never as the sole reason to invent behavior.
- Do not generate driver tests for hardware drivers. Public runtime behavior
  can be exercised through generic examples and hardware-validation notes;
  protocol evidence should be audited from original sources, public standards,
  open firmware/source, traces, or bench logs.
- If a protocol detail cannot be justified by external evidence, mark the
  feature as unknown or pending hardware validation instead of writing a fake.
- Hardware validation notes should use
  `docs/devices/hardware-validation-template.md` so every support claim records
  the hardware identity, firmware/software version, transport, evidence source,
  observed completion/event behavior, safety behavior, and remaining
  uncertainty.

Documentation refactor target:

- Replace the long README driver inventory with a compact support matrix table
  that links each driver/device family to its device page.
- Add one device page per supported driver family under `docs/devices/`.
- Use a standard device-page format:
  - status/provenance table: support level, source of protocol evidence,
    transport, discovery mode, real-hardware validation state, runtime
    requirements, and evidence gaps;
  - device table: advertised logical devices, kind tags, graph/dependency role,
    physical resource/remultiplexing behavior;
  - capability table: capability kind, device, request type, response type,
    completion semantics, timing support;
  - property table: property key, device, typed value kind, unit, read/write,
    range/enums/increment, sequenceable flag, hardware address or wire mapping;
  - examples table: example binaries and what workflow each demonstrates;
  - remaining-work table: hardware validation, protocol gaps, safety gaps, and
    model-specific limitations.
- Keep README examples to one short command table and move verbose behavior
  descriptions into the linked device pages.

Driver module consolidation target:

- New driver work lands in `crates/numanager-drivers`.
- Public imports use `numanager_drivers::<device_family>::...`.
- Per-driver packages such as `numanager_drivers::pi_gcs` are not part of the
  workspace target architecture.
- Driver source lives in normal module files under
  `crates/numanager-drivers/src`.
- Keep optional hardware transport features on the consolidated crate, starting
  with a shared `os-serial` feature.
- Low-level protocol helper modules are driver implementation details. They
  should stay hidden from generated docs and should not be used as user-facing
  APIs; expose typed properties/capabilities first.

Example refactor target:

- `crates/numanager-examples` is user-facing API documentation. Examples in
  this crate must demonstrate capability-level workflows, not hardware protocol
  packets. They should not print raw serial commands, construct protocol bytes,
  import driver `protocol` modules, or configure `ScriptedSerial` replies.
- Example output should report public operation results, typed properties,
  frame handles, stream status, and hardware-owned completion state. Do not
  expose diagnostic completion keys such as serial frames, raw registers, wire
  packets, protocol command lists, or physical transaction internals as the
  interesting result of a user workflow. When examples show discovered
  capabilities or filtered events, print capability kinds/request kinds and
  device labels instead of raw `CapabilityId`, `DeviceId`, or graph `NodeId`
  debug values unless the handle itself is the API concept being demonstrated.
- Keep recorded output for user-facing examples in `docs/example_outputs.md`
  so reviewers can compare the observable workflow surface without adding
  self-confirming driver tests.
- Replace per-hardware examples with a small set of general workflow examples:
  - `discover_devices`: two-stage detect/select/add flow across configured and
    simulated drivers.
  - `motion_stage`: XY/Z `StageMove`, typed position properties,
    remultiplexed state sets, homing, stop, and `Runtime::wait_completed`.
  - `camera_acquisition`: camera selection, typed acquisition properties,
    one-shot capture, frame metadata, and stream/ring-buffer use.
  - `light_source`: laser/LED/shutter `Dac` and `TriggerSink` workflows with
    typed optical power/current/wavelength properties.
  - `digital_io`: `DigitalIo`, `TriggerSource`, `TriggerSink`, `PulseProgram`,
    and counter/measurement workflows.
  - `environment_control`: temperature and gas controller workflows with typed
    setpoints, enabled state, safety summaries, completion waits, readback, and
    events.
  - `plate_reader`: plate transport, detector measurement, imaging-head, and
    camera-binding workflows with typed requests and driver-owned completion.
  - `fluidics`: valve-positioning workflows with `ValveSelect`, typed state
    readback, driver-owned completion, and events.
  - `filters`: filter-wheel selection workflows with `FilterSelect`, position
    state writes, driver-owned completion, readback, and events.
  - `autofocus`: provider-neutral autofocus selection and camera/Z/light
    dependencies.
  - `timing_plan`: cross-device timing plans that coordinate camera, stage,
    trigger, and light endpoints.
  - `biology_simulation`: whole-system biological simulation examples only;
    do not add standalone independent device simulations.
- Hardware-specific examples should be rare and justified only when they expose
  a genuinely distinct public workflow that cannot be expressed through a
  generic capability example, such as a multi-axis hub topology or a unique
  acquisition pipeline. Even then, they must stay on public runtime APIs.
- Remove protocol packing, configured-startup readback scripts, scripted serial replies,
  parser demonstrations, raw register frames, and protocol conformance coverage
  from `numanager-examples`. Do not move that material into generated driver
  tests; keep externally evidenced findings in evidence notes, reverse notes,
  trace notes, or hardware-validation records.
- Device pages may still document protocol provenance and wire mappings, but
  their examples tables should link to capability/workflow examples unless a
  hardware-specific public workflow is justified.
- README should advertise the workflow examples above, not a long list of
  one-example-per-driver binaries.

Example migration steps:

1. Inventory all binaries in `crates/numanager-examples/src` and classify
   each as public workflow, hardware-specific public workflow, or internal
   test fixture.
2. Create the generic workflow binaries listed above and make them select
   devices by `CapabilityKind`, `DeviceDescriptor.kinds`, and typed property
   schemas rather than by raw driver protocol details.
3. Audit protocol-heavy content from current hardware examples. Keep only the
   parts backed by external evidence and move those into evidence notes,
   reverse notes, trace notes, or hardware-validation records; delete circular
   scripted examples and packet demos.
4. Delete or stop advertising hardware-specific examples after their public
   workflow has been covered by a generic example.
5. Update README and all `docs/devices/*.md` example tables so device pages
   point to the generic workflow examples plus any justified hardware-specific
   public workflow.
6. Add evidence notes for each hardware driver before expanding functionality:
   source document/link or local hardware log, audited commands/properties,
   uncertainty, and real-hardware validation status.

Current generic workflow examples:

- `camera_acquisition`
- `camera_stream`
- `timing_plan`
- `motion_stage`
- `light_source`
- `digital_io`
- `environment_control`
- `plate_reader`
- `fluidics`
- `filters`
- `discover_devices`
- `autofocus`
- `biology_simulation`
- `software_gui` behind the `gui` feature

Current hardware-specific public workflows:

- `squid`: justified by Squid graph topology, controller demultiplexing, and
  firmware-backed autofocus/trigger coordination not fully covered by one
  generic workflow. Public Squid workflows must use typed `StageMove`,
  `StageHome`, `Dac`, `TriggerSource`, and `Autofocus` capabilities; the
  remaining `GenericCommand` Squid surfaces are diagnostic/bring-up escape
  hatches for the hub, theta, and filter-wheel paths.
- `spark_cyto`: justified by Spark Cyto plate-reader graph topology,
  plate/detector/environment/FIM/camera-binding acquisition state, and TDCL
  transaction remultiplexing.

## Track A: Spec-Backed Drivers

These are the drivers we should implement before any proprietary binary-artifact analysis.

### A1. Serial/Open-Firmware Controllers

Priority:

1. `Arduino` / `Arduino32bitBoards`
2. `ESP32`
3. `OpenUC2`
4. `TeensyPulseGenerator`
5. `ArduinoCounter`

Why:

- Protocols are small and firmware/source-defined.
- Good proving ground for discovery, serial transport, completion events, and
  digital IO abstractions.

Deliverables:

- Shared serial transport abstraction with framed text/binary codecs.
- Protocol tests only for externally documented frames, open-firmware behavior,
  or captured hardware traffic. Do not add scripted protocol fakes for examples.
- Generic `DigitalIo`, `TriggerSource`, `TriggerSink`, `Dac`, `Adc`, and
  `PulseProgram` capabilities.
- Two-stage discovery providers. Serial transports use explicit configured
  endpoints; see [`serial-discovery-design.md`](serial-discovery-design.md).
  Individual drivers should keep real serial startup paths configured and
  explicitly opted in.

### A2. Stage and Motion Controllers

Priority:

1. ASI MS-2000 / `ASIStage`
2. ASI Tiger / `ASITiger`
3. Prior ProScan / OptiScan
4. Sutter `MP285`
5. Sutter/Ludl-compatible stage controllers
6. Marzhauser TANGO/L-Step
7. PI GCS / GCS2
8. Thorlabs APT
9. Standa 8SMC
10. Trinamic TMCL direct-mode controllers
11. Zaber ASCII

Why:

- These have manufacturer command manuals, public protocol references, or
  open-source implementation evidence.
- They stress the core DAG/remultiplexing problem: one physical controller
  exposes multiple logical axes/devices.

Deliverables:

- Common `Axis`, `Stage1D`, `Stage2D`, `Stage3D`, `Home`, `Stop`, and
  `MotionProfile` capability enums.
- Typed position, velocity, acceleration, and limit values.
- Status-driven completion for absolute/relative moves.
- Multi-axis state-set coalescing for controllers that require combined moves.
- Capability probing based on controller firmware, cards, axes, and modules.

Implementation order details:

- Start with ASI MS-2000 because it is hub-shaped, serial ASCII, and rich in
  synchronized features.
- Build ASI Tiger after MS-2000 patterns are stable, because Tiger adds card
  addressing and larger module discovery.
- Implement Prior and Sutter next to validate that the motion abstraction does
  not overfit ASI.
- Implement PI GCS and Zaber as standards/manual-based SDK-backed
  counterexamples.
- For Standa, use the official 8SMC4-USB communication protocol directly for
  transport behavior; do not bind to SDK binaries.
- For Trinamic/TMCL controllers, use official ADI/Trinamic TMCL firmware
  manuals for the direct-mode binary frame, command/reply status, and axis
  parameter semantics before consulting compatibility defaults.
- Corvus now has an opt-in serial startup-readback/write support in
  `numanager_drivers::corvus` based on reverse engineered serial command
  evidence. It sends host mode, version/status/error startup queries, move,
  calibrate/home, abort, speed, acceleration, and joystick commands over
  `os-serial` when `connect = true`, validates sequenceable X/Y/Z runtime
  timing-plan endpoints, and applies first/last endpoints through the same
  software absolute move paths. Move/home/abort paths now request mapped `st`
  busy-bit polling plus `p` position and `ge` error readbacks when serial is
  connected. Broader status/error completion, coordinate semantics,
  synchronized timing, limit behavior, and hardware support claims need the
  exact Corvus manual/command-list revision or hardware traces.

Standa source decision:

- Do not bind to Standa SDK binaries in default builds.
- Do not copy behavior from a black-box SDK or binary-only dependency.
- Reverse engineered compatibility evidence reaches SDK-level calls such as
  `command_move`, `command_home`, `command_stop`, `get_status`,
  `get_position`, `get_engine_settings`, and `get_edges_settings`; it does not
  expose the transport protocol directly.
- The official Standa 8SMC4-USB Communication protocol specification v18.3 is
  now the accepted clean transport/spec source for the current support. It defines
  fixed serial settings, 4-byte command identifiers, command echo replies,
  CRC-16/MODBUS over data sections, `gpos`, `gets`, `gser`, `move`, `movr`,
  `home`, and `stop`.
- The exported runtime driver must follow documented protocol behavior
  until hardware traces validate status bits, limits, faults, motion-profile
  settings, and multi-axis coordination.

### A3. Lasers, Light Engines, Filters, and Illumination

Priority:

1. Cobolt / Hubner serial lasers
2. Coherent OBIS
3. Omicron serial lasers
4. CoolLED pE series
5. Lumencor Spectra/Sola/Gen3
6. Thorlabs DC2010/DC2100/DC3100/DC4100
7. Thorlabs KURIOS LCTF
8. Thorlabs DC2200, if SCPI/USBTMC path is confirmed
9. Agilent/Keysight Laser Combiner only if documentation appears

Why:

- Serial/SCPI-style command devices are implementable without SDKs.
- Illumination hardware must integrate tightly with global state sets and
  acquisition timing.

Deliverables:

- Typed optical quantities: wavelength, optical power, irradiance, current,
  exposure gating mode, TTL polarity, modulation mode.
- Common safety model: interlock, fault state, CDRH delay, emission enable,
  shutter state, key switch, warmup, and fault reset.
- Remultiplexed state sets for multi-channel devices.
- Hardware-trigger profiles for acquisition synchronization.

Implemented so far:

- Bluebox Optics niji now has an opt-in serial output support in
  `numanager_drivers::bluebox_niji` based on reverse engineered serial command
  evidence. It exposes the controller as a light-engine/shutter hub plus seven
  LED channels with typed ratio, wavelength, temperature, and safety-adjacent
  properties, sends startup status/temperature queries plus channel
  state/intensity, global intensity, TTL, and output mode commands over
  `os-serial` when `connect = true`, requests `?` status refresh after
  connected output/trigger/mode writes, records timeout-limited line-read telemetry, and
  implements runtime timing-plan arm/start/stop hooks that validate
  sequenceable hub/channel endpoints and apply first/last endpoints through
  the same global and per-channel output paths. A non-empty `Firmware,` status
  banner seeds firmware metadata. Detailed reply/error parsing,
  temperature/status refresh, lockouts, low-output behavior, hardware-accurate
  timing, and disable/readback semantics need a manufacturer command manual or
  hardware traces.

### A4. Generic Standards Backends

Priority:

1. Modbus RTU/TCP
2. GenICam node model
3. GigE Vision / USB3 Vision camera transport through Aravis or a clean Rust
   abstraction over open transport code
4. OS camera fallback: V4L2, GStreamer, DirectShow/OpenCV only for basic capture
5. Andor userspace USB camera support from reverse engineered evidence,
   starting with SDK2 discovery, identity/status, acquisition start/abort, and
   raw bulk frame readout only
6. Photometrics/PVCAM support from reverse engineered notes,
   including configured discovery, USB descriptor evidence, vendor-runtime
   package identity, writable exposure setting, verified vendor-runtime
   one-shot capture, and repeated one-shot stream support

Why:

- These unlock many adapters at once.
- They align with the high-throughput ring-buffer/event model already needed
  for cameras.
- Reverse engineered evidence records a userspace USB implementation
  path with confirmed VID/PID classification, nusb transport, FX2/FX3
  firmware-load anchors, SDK2 EP0 vendor request codes, and SDK2 bulk-IN frame
  readout. This is stronger than treating Andor as a generic black-box SDK
  target. Exposure, temperature, detector geometry, SDK2 control, and native
  feature-register mapping remain without a public surface where register
  evidence is absent.
- Reverse engineered PVCAM notes record the vendor-library ABI, parameter
  probing rules, acquisition API surface, native USB/PCIe framing clues, and
  host-command code map. This justifies the visible evidence/package surface,
  runtime-backed one-shot capture, repeated one-shot stream support, and
  exposure setting, but not a default SDK-free transport or broader
  parameter-control driver yet.

Deliverables:

- Generic register/coil/property mapping for Modbus devices.
- GenICam XML/node parser and typed property bridge.
- Camera stream object with fixed-capacity ring buffers, frame handles, dropped-frame
  telemetry, chunk metadata, hardware timestamps, and event channels.
- Discovery by transport, then user/config selection.
- Andor device page and evidence row that explicitly separate:
  - reverse engineered SDK2 USB surfaces: VID/PID discovery, identity,
    FIFO reset, status byte, acquisition start/abort, raw 16-bit
    big-endian EP 0x82 frame readout padded to 512 pixels;
  - surfaces needing additional mappings/evidence: SDK2 exposure/temperature/cooler/detector/capability
    mapping until register-window traces exist, and SDK3 native feature-register
    control until the bulk/register write protocol and feature map are
    recovered.
- If implemented, expose Andor only through generic `CameraCapture` /
  `CameraStream` workflows and runtime frame handles; do not add Andor-specific
  low-level examples or driver tests.
- Expose Photometrics/PVCAM through `discover_devices`, generic
  `CameraCapture`, and repeated one-shot `CameraStream` workflows only.
  Current writable `exposure` sets the next one-shot capture setup value.
  Native continuous streaming, broader parameter control, raw host-command,
  CCL/SCCL, and PCIe ioctl details must stay out of user APIs until separately
  evidenced.

## Track B: Reverse-Engineered Fallback Evidence

The active source policy for this track is
[`../protocol_evidence_plan.md`](../protocol_evidence_plan.md). That file is the
authority for clean-room spec criteria, evidence gates, and implementation
boundaries.

This track only starts when:

- no manufacturer command manual, public standard, open firmware, or open SDK
  source exists;
- the device is still strategically useful;
- the legal/licensing posture is acceptable;
- we can compare findings against real hardware or independent traces.

### B1. Good Candidates

These reverse-engineered cases are fallback evidence targets because the
source ladder has already failed. Acceptance here does not mean driver
implementation is allowed yet; each target still has to pass the spec gate in
[`../protocol_evidence_plan.md`](../protocol_evidence_plan.md).

| Adapter | Evidence | Why Consider It | Preferred First Step |
| --- | --- | --- | --- |
| `Okolab` | Reverse engineered | Serial/configured support exists from the compact serial grammar and shipped command dictionary; opt-in connected numeric temperature/CO2 read/write uses configured command codes | capture serial traffic and record output/readback/fault behavior before hardware-support claims |
| `AgilentLaserCombiner` | Reverse engineered | Small laser controller with high timing and safety value | capture serial traffic and hardware output/readback for safe low-output operations |
| `Mightex` / `Mightex_BLS` | Reverse engineered | BLS/SLC output driver and protocol serialization exist; the currently identified BLS/SLC command/readback surface is implemented; camera one-shot capture and repeated one-shot stream can use the verified vendor runtime, while native frame transport and native continuous streaming need native protocol evidence | capture BLS/SLC HID traces to validate output completion/errors/units/safety/timing before claiming more hardware behavior; validate camera runtime capture/stream and require traces or another clean source before native transport work |
| `MCL_MicroDrive` / `MCL_NanoDrive` | Reverse engineered | valuable motion devices with recovered USB/readback facts, but motion semantics are not hardware-validated | keep the raw encoder/status descriptor support; require hardware traces before motion, live default support, or position scaling |
| `ABS` | Reverse engineered | small legacy camera SDK; digest-verified vendor-runtime one-shot capture and repeated one-shot stream exist, but SDK-free native transport and native continuous streaming need native protocol evidence | validate runtime capture/stream on hardware; identify camera family and endpoints before native transport work |

### B2. Poor Candidates

Avoid reverse-engineered fallback evidence for these unless the project goal changes:

| Adapter | Reason |
| --- | --- |
| `PCO_Generic` SC2 path | complex camera SDK; use GenICam only for models that expose it |
| legacy `BaumerOptronic` / `ScionCam` | low strategic value unless exact hardware is needed |
| broad Mightex camera SDK work beyond the runtime-backed one-shot capture/repeated-capture stream target in Track B1 | native USB transport, native continuous streaming, gain/color controls, and broader SDK-free acquisition need native protocol evidence or additional documented SDK behavior before exposure |
| `ParallelPort` / `AOTF` `inpout` | platform utility only; replace with generic TTL backend |
| OpenCV runtime libraries | dependency libraries, not device protocols |

Andor is no longer treated as a blanket poor fallback candidate because reverse
engineered evidence contains a transport/readout audit.
Do not generalize that into full SDK2/SDK3 support: only implement support whose
USB requests, frame layout, completion behavior, and property mappings are
recorded in the Andor device page and evidence register.

### B3. Protocol-Evidence Workflow

For accepted fallback candidates:

1. Collect public headers, manuals, examples, and Micro-Manager call sites.
2. Use reverse engineered evidence only to identify candidate wire behavior;
   do not commit proprietary binaries, analysis tools, raw dumps, private
   function names, addresses, or call graphs.
3. Prefer runtime tracing over static artifact analysis:
   - serial port logs;
   - USB control/bulk transfer captures;
   - TCP/UDP packet captures;
   - HID report captures;
   - hardware status/event captures.
   Use `docs/reverse/trace-capture-guide.md` for the required capture metadata
   and transport-specific fields.
4. Curate the trace into `docs/reverse/<target>.md`, the device page evidence
   gate, and `docs/devices/evidence.md` before writing driver code.
5. If repeatable local checks are unavoidable for a hardware bring-up audit,
   keep them outside generated driver tests unless explicitly requested for a
   documented hardware-validation workflow. A replay fixture is not evidence by
   itself and must not become a user-facing example.
6. Implement only the command/property support covered by curated evidence, then
   validate with real hardware before broadening support claims.
7. Document provenance for every command:
   - manufacturer/manual;
   - open source;
   - reverse engineered compatibility evidence;
   - observed trace;
   - reverse engineered note.
8. Do not copy proprietary-binary-derived code or reproduce proprietary SDK internals.

## Track C: Integrated Biological-System Simulation

Postpone additional standalone device simulators. Simulating a camera, stage, or
light source independently is useful only as temporary protocol scaffolding for
examples and smoke checks; it does not exercise the software behavior that
matters for microscopy.

The simulator track should instead model complete microscope configurations
connected to biological sample models. Device drivers in that environment should
share one simulated physical world so stage moves, focus, illumination,
exposure, photobleaching, motion blur, fluorescence, transmitted-light contrast,
noise, and acquisition timing are coupled.

Priority when simulation work resumes:

1. Brightfield transmitted-light microscope with XY/Z motion, camera exposure,
   focus-dependent contrast, and sample drift. Implemented as one composed
   microscope hub in `numanager_drivers::sim_microscope`, whose camera, XY
   stage, Z stage, objective turret, and lamp share a single procedural
   cell-culture model, and which publishes the sensor pixel pitch, binning, and
   objective magnification a client needs to convert image pixels to
   micrometres.
2. Fluorescence time-lapse rig with camera, filter/light engine, bleaching,
   shot noise, dark current, and hardware-triggered acquisition.
3. Autofocus rig where focus metrics depend on the simulated biological sample,
   Z-stage position, camera settings, and illumination state.
4. Plate reader/imager with well geometry, stage tiling, illumination channels,
   camera field of view, and per-well biological variation.
5. High-throughput acquisition stress rig with ring-buffer pressure,
   dropped-frame telemetry, hardware timestamps, and backpressure behavior.

## Runtime/API Work Needed Before Broad Driver Work

1. Stable typed capability enums and request/response enums.
2. Typed physical quantities for position, velocity, acceleration, time,
  temperature, wavelength, power, current, voltage, frequency, numerical
  aperture, pressure, gas concentration, and flow.
3. Driver-owned completion model:
   - command accepted;
   - in progress;
   - completed;
   - failed;
   - cancelled/interrupted;
   - hardware fault.
4. Multi-listener event bus with device and operation filters.
5. High-throughput frame API:
   - driver-owned producer;
   - fixed-capacity ring buffers;
   - frame handles;
   - zero-copy or copy-on-demand paths;
   - backpressure policy;
   - dropped-frame telemetry.
6. State-set planner that lets hubs coalesce logical device writes into one
   physical transaction.
7. Config/discovery model:
   - detect candidates;
   - user/config claims candidates;
   - runtime adds/removes drivers dynamically;
   - persistent hardware identity and aliases.

Implemented so far:

- The runtime frame store keeps fixed-capacity per-stream rings keyed by
  `FrameHandle`, applies the requested `OverflowPolicy`, annotates
  `FrameReady` metadata with ring depth/capacity/drop counters, and emits
  stream-scoped `Telemetry` events when frames are dropped under `DropOldest`
  or `DropNewest`; `OverflowPolicy::Error` overflows publish `Fault` events.
  `Runtime::stream_status()` exposes retained frame handles, ring depth,
  capacity, overflow policy, and dropped-frame counters for clients that need
  pull-style inspection in addition to event delivery. This gives all camera
  drivers a shared backpressure reporting path.
- `LocalRuntime::devices()` returns a deterministic `DeviceId`-ordered view of
  runtime-owned descriptors, and `DeviceDescriptor::has_kind`/`has_kinds`
  centralize public kind-tag matching for examples, GUIs, and config-driven
  selection without exposing protocol-specific internals. `LocalRuntime` also
  exposes `device_by_kind`, `device_by_kinds`, `devices_by_kind`,
  `device_by_capability`, and `devices_by_capability` so applications can query
  the runtime-owned device view directly after driver registration.
- `numanager-examples` now centralizes generic device/capability selection
  helpers (`device_by_kind`, `device_by_kinds`, `device_by_id`, and
  capability-by-kind helpers) so workflow examples do not each reimplement
  descriptor scans.
- `docs/devices/evidence.md` now provides a cross-driver evidence register that
  separates public standards, manufacturer protocols, open firmware/source,
  hardware traces, and fixture/local-only behavior before protocol expansion or
  claimed hardware support.
- `docs/devices/hardware-validation-template.md` defines the standard evidence
  note format for bench runs, captured traces, completion/event observations,
  stream validation, safety checks, and remaining uncertainty.
- `numanager-examples -- camera_acquisition` selects Toupcam, platform-camera,
  GigE Vision, USB3 Vision, or GenICam camera sources by advertised camera
  descriptors and `CapabilityKind::CameraCapture`, applies only schema-valid
  typed camera properties, waits on driver-owned operation completion, and
  extracts the completed `FrameHandle` through `CapturedFrame::from_completion`
  before fetching frames from the runtime frame store, without exposing raw
  protocol or register operations. Recorded output now includes source-specific
  standards excerpts for GigE Vision, USB3 Vision, and GenICam showing public
  chunk/timestamp/transport metadata.
- `numanager-examples -- camera_stream` selects the same camera source
  families by advertised `CapabilityKind::CameraStream` and exercises `DropOldest`,
  `DropNewest`, and `Error` ring-buffer policies, parses stream completions
  through `CameraStreamStarted::from_completion` with compatibility for
  `frames` and `frame_count` completion fields, and reports retained/missing
  frame handles, dropped-frame telemetry, fault reporting, and runtime stream
  status snapshots.
- `numanager-core::Value` now has first-class typed physical quantities for
  temperature, position, velocity, acceleration, time interval, wavelength,
  optical power, electric current, voltage, pressure, gas concentration, and
  flow rate. The typed quantity set now also includes frequency for
  modulation/timing-rate properties. Each quantity keeps its named unit until a
  driver converts at the protocol boundary, avoiding naked unit-suffixed floats
  in new driver APIs.
- `numanager-core::CapabilityRequest` now has typed request variants for
  `DigitalIo`, `Dac`, `Trigger`, `Measure`, `PulseProgram`, `PlateMove`,
  `TemperatureControl`, `CameraBinding`, and `Adc` in addition to camera,
  stage, and autofocus requests. Public workflow examples use these variants
  for ordinary device operations instead of ad hoc
  `GenericCommandRequest` maps; `GenericCommand` remains an explicit escape
  hatch for documented raw/protocol bring-up surfaces.
- `CapabilityKind`, `CapabilityDescriptor`, and `CapabilityRequest` now expose
  request-kind helpers. Clients can discover a capability, inspect
  `preferred_request_kind()`, and construct the matching typed request enum
  without hard-coding capability names or falling back to dynamic maps.
  Capability descriptors now derive their advertised request value schema from
  the preferred request kind through core constructors, including an explicit
  `ValueType::Null` for no-request capabilities such as stage home/stop.
  `CapabilityKind::is_diagnostic()` and
  `CapabilityDescriptor::is_diagnostic()` centralize the classification of
  raw/register/generic/custom bring-up surfaces so examples and GUIs can hide
  them without reimplementing that policy.
- `Command` exposes public constructors for property read/write, typed
  capability invocation, timing-plan arm/start/stop, and state-set conversion.
  User-facing examples use these helpers for normal workflows so they read as
  device/capability operations rather than manual enum-field assembly.
- `TimingPlan::from_parts()` is the checked low-level constructor for dynamic
  timing plans. It derives participants from routes, property sequences, arm
  order, and external-trigger starts, and rejects inconsistent sequence/arm
  inputs before runtime submission. `TimingPlan::builder()` provides the
  higher-level Rust API for hand-written plans, accepting `&DeviceDescriptor`
  as well as `DeviceId` so examples do not repeat `.id` for every participant,
  route, sequence, and arm-order entry.
- `LocalRuntime::submit_capability()` and `LocalRuntime::execute_capability()`
  let applications invoke capabilities by `CapabilityKind` plus typed
  `CapabilityRequest`. The runtime resolves the advertised `CapabilityId`
  internally before dispatch, so examples do not need to pass opaque capability
  handles around for ordinary typed workflows.
- `CapabilityRequest::inferred_capability_kind()` plus
  `LocalRuntime::submit_request()` and `LocalRuntime::execute_request()` remove
  duplicated kind spelling for unambiguous typed requests. Examples now submit
  `CameraCaptureRequest`, `CameraStreamRequest`, `StageMoveRequest`,
  `AutofocusRequest`, DAC/ADC/measure/control requests, and plate/FIM/gas/camera
  binding requests directly. Explicit `CapabilityKind` calls remain for
  `CapabilityRequest::None`, trigger source/sink operations, generic commands,
  and custom/diagnostic paths where the request type does not identify exactly
  one capability.
- `numanager-core::capability_providers()` provides a reusable graph query for
  discovering devices that expose a `CapabilityKind` and their dependency
  devices by `Role`. The autofocus example now uses this shared helper instead
  of carrying an example-local provider scanner or resolving dependency labels
  by hand.
- `numanager_drivers::evident_ix85` provides configured Evident/Olympus IX85
  body inventory plus opt-in active serial startup/readback/control support
  from reverse engineered direct serial evidence. It advertises the hub, focus
  drive, nosepiece, light path, mirror unit, DIA/EPI shutters, and ZDC/autofocus
  state as logical devices. Focus motion/stop, state-device selection, shutter
  control, body readback, and hub refresh helpers are implemented through the
  mapped serial tags; ZDC autofocus actions remain unexposed until `AF`
  parameter semantics, completion, notifications, errors, and safety behavior
  are known from official documentation or hardware traces.
- `numanager-examples` has shared display helpers for capability summaries,
  completion summaries, and event device labels. Generic workflow examples use
  capability kind/request-kind text and descriptor labels in their output,
  while keeping opaque IDs as internal runtime handles for command submission
  and waiting.
- Low-level driver protocol helper modules are crate-private within
  `numanager-drivers` instead of public user APIs. They remain available for
  implementation, discovery, and evidence-backed protocol audit work inside the
  consolidated driver crate, but applications and examples are expected to use
  typed descriptors, properties, capabilities, discovery candidates, and runtime
  commands.
- `LocalRuntime` now performs conservative pre-dispatch validation for target
  devices, property schemas, state-set values, capability IDs, and typed
  capability request kinds. Property validation enforces readable/writable
  access, advertised enum values, numeric/physical-quantity ranges, and static
  property increments in canonical units before a command reaches a driver
  lane. Driver-owned dynamic constraints such as GenICam `pMin`/`pMax`/`pInc`
  remain validated inside the driver against current hardware/node state.
  Timing-plan sequences must target writable `sequenceable` properties. Raw
  `GenericCommand` requests are accepted only by explicit raw/generic command
  capability surfaces, and `Custom` requests are accepted only by explicit
  custom capability surfaces.
- `SafetyState` and `SafetySummary` provide the first shared safety readback
  contract for common property names such as `enabled`, `interlock_closed`,
  `emission_permitted`, `fault_active`, `fault`, and related fault flags.
  `LocalRuntime::safety_summary()` reads the advertised readable safety
  properties for a device and normalizes them into `safe`, `active`,
  `interlocked`, `fault`, or `unknown` without hiding the raw values.
  `numanager-examples -- light_source` demonstrates this for laser and
  illumination devices.
- `OperationStatus::into_completed()` and `Runtime::wait_completed()`
  centralize completion-status handling for callers that submit an operation
  and then wait for the driver/hardware-owned completion result. Public
  examples now use these core helpers instead of reimplementing local
  `expect_completed` functions or treating sleeps as completion evidence.
- Generic workflow examples now avoid fixture label lookup for ordinary device
  selection. `timing_plan` selects participants through advertised kind tags
  and capabilities, while `autofocus` selects the composed simulation through
  provider dependency roles. Hardware-specific topology examples such as
  `squid` and `spark_cyto` may still name logical devices because their purpose
  is to demonstrate a particular controller graph.
- `LocalRuntime` now exposes read-only `devices`, `device`, `capabilities`, and
  `capability_by_kind` introspection methods. Generic examples use this
  runtime-owned capability view after driver registration instead of retaining
  driver objects just to inspect capabilities.
- `numanager-core` now includes common typed motion, valve, and filter request surface
  pieces: `StageAxis`, `StageGeometry`, `MotionProfile`, and
  `StageMoveRequest`, with `CapabilityRequest::StageMove` for structured
  absolute/relative moves, `ValveSelectRequest` and `ValveDirection` for
  ordinal fluidics valve selection, and `FilterSelectRequest` for ordinal
  filter-wheel selection without using raw command strings.
- `EventFilter` now supports kind, device, and operation selectors.
  `OperationChanged` events carry their target devices, so device filters also
  work for operation-status listeners. `EventFilter`, `DeviceSelector`, and
  `OperationSelector` expose constructors and builder helpers for single or
  multi-device, single or multi-operation, and multi-kind subscriptions so
  clients can attach listeners to composed device sets without constructing
  selector structs by hand. Public examples use these helpers, including a
  camera subscription filtered to one camera and one capture operation.
- `LocalRuntime` supports dynamic `add_driver`, `add_candidate`,
  `remove_driver`, `drivers`, and `contains_driver` operations with
  `DeviceArrived`/`DeviceRemoved` events. Driver removal deterministically
  removes descriptors/capability indexes, purges buffered frames for removed
  devices, invalidates armed timing plans that mention removed devices, and
  keeps later lane events from overwriting cancelled operation status.
  `DiscoveryLock` now round-trips persistent hardware IDs, user-facing aliases,
  serial/firmware identifiers, and metadata entries. `DriverCandidate` now
  exposes a canonical `to_discovery_entry()` conversion so UIs, CLIs, and config
  tools do not invent separate lock metadata conventions, and
  `numanager-examples -- discover_devices` exercises the two-stage
  detect-then-claim flow plus lock-file save/load through that core API.
- `HardwareConfig::builder()` and `HardwareConfig::builder_from()` now allocate
  typed resource/device handles for config assembly, then use those handles for
  dependencies and remux groups. User-facing config examples should use this
  builder path instead of constructing `NodeId`, `DeviceId`, or `ResourceId`
  wrappers directly. `DeviceConfig::new()` and `ResourceConfig::new()` are the
  narrower constructor path for static configured-discovery fixture lists that
  still need stable persisted numeric IDs.
- The internal `software_gui` test driver dispatches camera operations by typed
  `CapabilityRequest::CameraCapture` and `CapabilityRequest::CameraStream`
  variants rather than assuming fixed raw `CapabilityId` values for capture and
  stream.
- `LocalRuntime` supports runtime-owned multi-driver `StateSet` submission.
  State-set writes are split by owning driver lane, each driver still
  remultiplexes its local logical writes into hardware transactions, and the
  runtime publishes one operation lifecycle with a merged completion value.
  `StateSet` and `StateWrite` expose constructors and builder helpers for
  immediate, prepare-then-commit, and hardware-timed state sets, so examples and
  applications can express global property intent without manually assembling
  struct internals.
  `numanager-examples -- digital_io` exercises this with Arduino digital
  IO/ADC and Arduino Counter devices in one global setup command.
- `LocalRuntime` now handles `Command::Arm(TimingPlan)`, `Command::Start`, and
  `Command::Stop` as runtime-owned operations, so a timing plan can span
  devices from multiple drivers instead of being forced through one hub lane.
  The runtime validates participants, trigger-route endpoints, sequences,
  arm-order entries, and external trigger start devices, checks trigger-route
  endpoints against advertised `TriggerSource`/`TriggerSink` capabilities,
  checks sequence entries against advertised `sequenceable` property schemas
  and value types, asks each involved driver lane to prepare its timing-plan
  support through `Driver::prepare_timing_plan`, stores the resulting physical
  arm transactions with the armed plan, asks each prepared driver for
  `start_timing_plan`/`stop_timing_plan` transition transactions on runtime
  `Command::Start`/`Command::Stop`, and returns structured plan summaries
  through operation completion events.

## First Implementation Milestones

### Milestone 1: Transport and Local Fixture Foundation

- Serial text and serial binary transports.
- Two-stage discovery providers backed by real probes, config, or temporary
  local fixtures.
- Hardware-status completion events.
- Generic digital IO and motion abstractions.

Implemented so far:

- `numanager-core::serial` provides reusable serial IO traits, line framing,
  fixed-length binary framing, and a scripted serial fixture transport.
- `numanager-core` exposes an optional `os-serial` feature with
  `serial::OsSerialPort`, allowing real OS serial ports to be used through the
  same nonblocking `SerialIo::read_available` interface as simulated serial.
- `numanager_drivers::arduino` provides the first A1 open-firmware driver support:
  simulated two-stage discovery plus config-backed discovery with an
  optional `os-serial` startup firmware-identification/capacity readback path,
  Micro-Manager Arduino firmware-identification
  opcodes, digital output, shutter, ADC, DAC, digital sequence upload/start,
  timed-pattern delay/repeat/start properties, blanking mode, blank-trigger
  polarity, logic inversion, analog/digital input snapshot decoding,
  read-only ADC `input_summary`, remultiplexed state sets, and runtime-owned
  completion. The implemented opcode surface now covers
  firmware commands `1`, `3`, `5`, `6`, `8`, `9`, `10`, `11`, `12`, `20`,
  `21`, `22`, `30`, `31`, `32`, `33`, `34`, `35`, `40`, `41`, and `42` at
  the protocol layer. Runtime timing-plan arm/start/stop hooks now map plan
  transitions onto explicit sequenceable digital mask, digital sequence,
  timed-output, and shutter-open endpoints through those same property-backed
  firmware commands. Direct `DigitalIo`, `Adc`, `Dac`, and trigger invocations
  accept typed `CapabilityRequest` variants for public workflow use, and direct
  driver preparation now rejects `GenericCommand`/`Custom` requests on those
  typed capabilities.
- `numanager_drivers::arduino_counter` provides an A1 open-firmware counter
  support: simulated two-stage discovery plus config-backed discovery with an
  optional `os-serial` startup `p?` snapshot readback path, CR text command builders for `gNNN`,
  `s`, `i`, `p?`, `pi`, and `pd`, count and `count=<n>;level=<0|1>`
  snapshot reply parsers, logical counter and pulse-output devices, typed
  `Value::TimeInterval` gate/interval properties, count and pulse-level
  properties plus read-only `counter_summary`, direct `Measure` invocation for
  timed counts, direct `PulseProgram` invocation for interval setup, direct
  `TriggerSource` invocation for pulse/high/low output, runtime-owned
  completion, and timing-plan arm/start/stop hooks that map transitions onto
  sequenceable gate-time, pulse-interval, and output-level endpoints, with
  default `pi`/`pd` pulse output commands when no explicit level sequence is
  present. Direct measure, pulse-program, and pulse-output invocations accept
  typed `CapabilityRequest::Measure`, `CapabilityRequest::PulseProgram`, and
  `CapabilityRequest::Trigger` requests, and direct driver preparation now
  rejects `GenericCommand`/`Custom` requests on those typed capabilities.
- `numanager_drivers::spark_cyto` provides a Spark Cyto TDCL graph model:
  simulated two-stage discovery plus config-backed graph/state discovery, TDCL frame encode/decode, Symbio command
  string builders, a mainboard hub plus absorbance, fluorescence,
  luminescence, temperature, gas, FIM, and camera-binding logical devices,
  typed and sequenceable plate well, detector wavelength/enable, temperature
  target/enable, gas CO2 target/enable, FIM objective/mode, and camera
  binding/mode properties, read-only gas CO2/fault and FIM interlock/fault
  readback, remultiplexed state sets over one TDCL command resource,
  `PlateMove`, `Measure`, `TemperatureControl`, `GasControl`, `ImagingHead`,
  and `CameraBinding` capabilities, and timing-plan arm/start/stop hooks that
  apply first/last plate, detector, environmental, gas, FIM, and camera-binding
  sequence endpoints through the same property-backed TDCL transaction model.
  Direct plate movement, detector measurement, temperature-control,
  gas-control, imaging-head, and camera-binding invocations now accept typed
  `CapabilityRequest::PlateMove`,
  `CapabilityRequest::Measure`, `CapabilityRequest::TemperatureControl`,
  `CapabilityRequest::GasControl`, `CapabilityRequest::ImagingHead`, and
  `CapabilityRequest::CameraBinding` requests instead of ad hoc maps.
- `numanager-examples -- spark_cyto` exercises Spark Cyto graph topology,
  capability inspection, typed plate/measure/temperature/gas/FIM/camera-binding
  invocation, typed remultiplexed state-set submission,
  acquisition-style timing sequences for
  plate/detector/temperature/gas/FIM/camera binding state,
  `Runtime::wait_completed`, typed gas/FIM readback, runtime log delivery, and
  driver removal. Example completion summaries filter driver diagnostic keys
  such as protocol command lists, serial frames, and physical transaction
  internals so public output remains capability-level.
- `numanager_drivers::esp32` provides the second A1 open-firmware driver support:
  simulated two-stage discovery plus config-backed discovery with an
  optional `os-serial` startup `V`, `U,<axis>`, and `W` readback path,
  CRLF text command builders for `V`, `U,<axis>`,
  digital, PWM, XY, and Z operations, mapped `W,<x>,<y>,<z>` position reply
  parsing and poll/readback ingestion for XY/Z properties, read-only hub
  `state_summary` with hardware-position refresh, typed `Value::Position`
  travel-range metadata, logical digital, shutter, PWM, ADC, XY, and Z
  devices, typed `Value::Position` public stage positions, sequenceable XY/Z
  position plus PWM/shutter properties, typed `StageMove` invocation for XY/Z
  targets, direct PWM `Dac` invocation through `CapabilityRequest::Dac` for
  percent duty, direct shutter `TriggerSink` invocation through
  `CapabilityRequest::Trigger` for pulse/open/close, direct driver preparation
  that rejects mismatched request kinds on typed capabilities, remultiplexed state sets,
  property-change events for hardware-driven position replies, runtime-owned
  completion, and timing-plan arm/start/stop hooks that apply
  first/last motion/PWM/shutter endpoints while coalescing XY transitions into
  one controller command.
- `numanager_drivers::openuc2` provides the third A1 open-firmware driver support:
  simulated two-stage discovery plus config-backed discovery with an
  optional `os-serial` startup `/state_get` readback path, LF/CR JSON-line command builders for
  `/state_get`, `/motor_act`, and `/laser_act`, mapped `/state_get`
  JSON-line reply parsing and poll/readback ingestion for controller, XY/Z
  position, laser-enable, and laser-power fields, read-only hub `state_summary`
  metadata/property with hardware-state refresh, logical
  XY, Z, and laser devices, typed `Value::Position` public stage positions and
  XY/Z travel metadata, typed laser wavelength metadata, typed `StageMove`
  invocation for XY/Z targets, direct laser `Dac` invocation through
  `CapabilityRequest::Dac` for percent power, direct laser `TriggerSink`
  invocation through `CapabilityRequest::Trigger` for pulse/enable/disable,
  direct driver preparation that rejects mismatched request kinds on typed
  capabilities, remultiplexed motor/laser state sets, sequenceable XY/Z position plus laser
  enable/power properties, property-change events for hardware-driven
  `/state_get` replies, runtime-owned completion, and timing-plan
  arm/start/stop hooks that apply first/last motor/laser endpoints through one
  remultiplexed controller-state flush.
- `numanager_drivers::openstage` provides an open-hardware microscope-stage
  support from the OpenStage paper's published serial control tables. It exposes
  one controller hub plus XY and Z stage devices, typed `Position` properties
  and travel metadata, typed `StageMove` support for absolute and relative
  moves with post-motion `p` position readback, `$`-terminated ASCII command handling for go-to, position readback,
  controller information, step-size, velocity, acceleration, speed-mode, and
  beep surfaces, constrained hub `GenericCommand` actions for
  `read_information`, `read_velocity`, `read_acceleration`, and `beep`,
  remultiplexed X/Y/Z state-set moves through one shared controller command,
  config-backed two-stage discovery, and optional configured real serial
  construction behind `os-serial` that reads controller information, current
  position, step size, velocity, and acceleration before registration. Runtime
  timing-plan arm/start/stop hooks validate sequenceable X/Y/Z endpoint values
  and apply first/last endpoints through the same absolute XYZ move path.
  Hardware validation of completion terminators, post-motion position readback,
  skipped-step behavior, limits, synchronized timing, and safe stop/disable
  behavior is not recorded.
- `numanager_drivers::wosm` provides a Warwick Open-Source Microscope
  controller support from reverse engineered protocol evidence. It exposes one
  hub plus switch, shutter, XY/Z stage, input, and four light-output devices
  over a shared `tcp.text` resource, uses typed `Position` and `Ratio`
  properties, supports `StageMove`, `DigitalIo`, `Dac`, `TriggerSink`, `Adc`,
  and `Measure` capabilities through runtime-owned completion, remultiplexes
  stage DAC channels and switch/shutter/light state through the single
  controller, and appears in the config-backed discovery flow. Prompt-based TCP
  output commands, sequence run/end, blanking controls, aggregate digital-input
  reads, and raw analog-input reads are available behind opt-in `connect`;
  analog raw-count scaling, sequence timing, light-current calibration,
  safe-disable output evidence, and hardware validation are not recorded.
- `numanager_drivers::opentrons_ot2` provides an OT-2 HTTP inventory and
  run-action support
  from the Opentrons HTTP API, architecture, and open-source robot stack
  research note. It exposes the robot-server HTTP hub, gantry, deck, configured
  pipette inventory, camera availability, and temperature-module
  inventory/readback/control as logical devices in the config-backed discovery
  flow.
  When `property.connect = true`, it connects to the configured host/port,
  sends `GET /health` with the configured `opentrons-version` header, and
  updates cached server/status metadata. The hub now exposes
  constrained `GenericCommand` runtime readback for `refresh_health` and
  `refresh_inventory`; the latter issues read-only `GET /modules` and
  `GET /runs` requests, updates cached module/run counts and current run
  metadata, and emits changed read-only hub/deck metadata. It also supports
  `refresh_current_run`, a read-only `GET /runs/{runId}` refresh that updates
  cached run/status metadata when a current run id is known, and
  `refresh_run_commands`, a read-only
  `GET /runs/{runId}/commands?pageLength=20` refresh that updates cached
  command count/id/status metadata without enqueueing commands. It intentionally
  supports `play_run`, `pause_run`, and `stop_run` as current-run-only
  `POST /runs/{runId}/actions` submissions. A temperature-module child supports
  `TemperatureControl` plus writable `target_temperature` and `enabled`
  through API v2 `POST /modules/{serial}` using `set_Temperature` for
  documented 4..=95 degC targets and `deactivate` for disable; API v3+ fails
  closed because that endpoint is documented as removed. A camera device now supports
  `CameraCapture` through `POST /camera/picture`, storing the returned native
  HTTP image bytes with content metadata. It exposes gantry home and absolute
  configured-mount moves, but not relative moves, pipetting, broader
  module-actuation, protocol-run creation/upload, image interpretation, or
  arbitrary robot command enqueueing until the HTTP OpenAPI schemas, command
  completion semantics, and safety/recovery behavior are recorded.
- `numanager_drivers::triggerscope` provides an ARC TriggerScope opt-in serial
  direct-control support. It exposes one hub, one focus Z stage,
  camera-trigger outputs, TTL outputs, and DAC outputs over one shared
  `serial.ascii` resource; uses typed `Position` and `Voltage` properties;
  supports `StageMove`, `DigitalIo`, `TriggerSink`, `TriggerSource`, and
  `Dac`; exposes constrained hub `GenericCommand` clear/program/arm sequence
  commands; writes focus, TTL, camera-trigger, DAC, and sequence commands
  through `os-serial` when `property.connect = true`; and maps runtime timing
  plans only to evidenced TTL `high`, DAC `voltage`, and evenly stepped focus
  `z` sequence commands. Camera-trigger `high` remains writable direct control;
  sequenceable camera-trigger output needs an evidenced sequence command. Live
  construction sends the identification command and caches a
  non-empty banner as firmware metadata. Hardware response/error parsing,
  output/readback recording, and safety validation are not recorded.
- `numanager_drivers::chuo_seiki_qt` provides a Chuo Seiki QT opt-in serial
  write support. It exposes one hub, an XY stage using controller axes A/B, and
  an optional Z stage using configured axis A/B/C over one shared
  `serial.ascii` resource; uses typed `Position` and `TimeInterval`
  properties, keeps speed settings as native controller pulses/s until physical
  velocity calibration is evidenced, supports `StageMove`, `StageHome`, and
  `StageStop`, remultiplexes XY motion over the shared controller, sends
  startup identification, feedback setup, and move/home/stop/native speed
  commands over `os-serial` when `connect = true`, records timeout-limited line-read
  telemetry, polls position readbacks while known moving/homing state
  characters are reported, implements runtime timing-plan arm/start/stop hooks that validate
  sequenceable X/Y/Z endpoints and apply first/last endpoints through the
  existing XY/Z move paths, and appears in
  the config-backed discovery flow. Pinning the exact QT controller
  manual/command-list revision, reply/error parsing, hardware timeout tuning,
  limit/alarm handling, and hardware validation are not recorded.
- `numanager_drivers::teensy_pulse` provides the fourth A1 open-firmware driver
  support: simulated two-stage discovery plus config-backed discovery with an
  optional `os-serial` startup enquiry/readback path, binary command builders for version,
  start, stop, interval, duration, wait-for-input, number-of-pulses, and enquiry
  frames, reply decoding and poll/readback ingestion into read-only
  `program_summary`, explicit `CMD_ENQUIRE` read paths for pulse-program fields, logical pulse-generator
  device, typed `Value::TimeInterval` interval/duration properties,
  pulse-count/running-state properties, direct `PulseProgram` invocation for
  interval/duration/wait/count setup, direct `TriggerSource` invocation through
  `CapabilityRequest::Trigger` for start/stop/pulse, property-change events for
  hardware-driven reply frames, direct driver preparation that rejects generic
  command-map aliases on typed `PulseProgram`/`TriggerSource` capabilities,
  runtime-owned completion, and timing-plan arm/start/stop
  hooks that apply first/last interval, duration, wait-for-input,
  number-of-pulses, and running endpoints through the same firmware opcodes.
  It intentionally uses little-endian `u32` wire values because both the
  Micro-Manager `TeensyCom` implementation and firmware parser use little
  endian, despite an older firmware comment saying big endian.
- Standalone generic motion simulation has been removed from public examples and
  from `numanager_drivers::sim`; generic motion workflows now use hardware
  driver fixtures such as ASI MS-2000. Simulation work remains focused on
  composed biological models such as camera/Z/light autofocus.
- `numanager-examples -- biology_simulation` exercises the composed
  biological focus-plane model as a whole system: camera capture publishes
  runtime frame handles parsed through `CapturedFrame::from_completion`, Z
  motion changes focus score, autofocus locks against the shared model, and
  timing-plan transitions couple exposure, Z, light, and autofocus state.

Remaining:

- The evidenced Arduino firmware-identification, digital output, DAC, sequence,
  timed output, blanking, digital-input, ADC, and pull-up opcode surface is
  implemented. Further Arduino firmware opcodes are not exposed without firmware source,
  project documentation, captured traces, or bench logs; real response parsing
  validation for input reads remains hardware validation.
- Hardware validation for Arduino, ESP32, OpenUC2, ArduinoCounter, and
  TeensyPulseGenerator protocol coverage.
- Autofocus is now modeled as a first-class device/capability pair in core, not
  as a Squid-specific laser pin, light gate, or Squid device subtype. It is a
  general device/capability model that is separate from Squid; treat it as a
  reusable standalone device or composed service abstraction:
  Squid/Octopi offers one concrete provider, `squid.autofocus`, but the general
  device model lives in core and must not be derived from Squid naming or
  wiring. The Squid provider is config-backed and backed by the documented
  `SET_PIN_LEVEL` path for firmware pin 15, while ASI Tiger CRISP is another
  provider of the same core capability. General autofocus providers may be
  hardware modules, firmware focus-lock units, or composed services that depend
  on a Z stage, camera, and optional light/laser devices through graph edges.
  Clients should discover/select them through `CapabilityKind::Autofocus` and
  dependency metadata, not through provider-specific kind tags. `Driver::graph`
  gives all drivers a topology hook, `DeviceGraph` now has an explicit
  dependency helper for `UsesDevice` edges, the Squid graph advertises the
  Z-stage and light-source dependencies for its autofocus provider, the ASI
  Tiger graph advertises the Z-stage dependency for CRISP, and SutterStage
  advertises an autofocus provider backed by `AF <axis>=<parameter>` with a
  Z-stage dependency. Squid now exposes provider-neutral public autofocus
  properties (`enabled`, `mode`, `status`, `focus_score`) while keeping the
  firmware pin as metadata/diagnostic detail. Squid timing-plan hooks now
  validate sequenceable XY/Z position, illumination enabled/intensity, and
  autofocus enabled properties, apply first/last sequence endpoints through the
  same controller frame path at start/stop, and emit camera-trigger pulses for
  Squid trigger participants on timing start. Squid configured discovery can
  also open an explicit `os-serial` port, ingest immediately available startup
  status frames before registration, and use the same fixed-frame command and
  status parser against real hardware; hardware validation is not recorded.
  `numanager_drivers::sim` now includes a
  composed camera/Z/light autofocus service backed by a shared biological
  focus-plane model, with graph dependencies from its camera, Z stage, and light
  source into the provider-neutral autofocus endpoint. The composed simulation
  now accepts timing plans that sequence camera exposure, Z position, light
  enable/power, and autofocus enable/mode against the shared biological
  focus-plane model, recomputing focus score on each timing transition.
  Remaining work is normal illumination safety rules, hardware acquisition-plan
  integration beyond software sequence endpoints, and additional provider
  implementations.
  This is part of the real driver surface, not standalone simulation work.
  `autofocus` now exercises Squid autofocus, ASI Tiger CRISP, and
  SutterStage autofocus plus the composed camera/Z/light service through one
  provider-neutral `CapabilityKind::Autofocus` path, and demonstrates composed
  autofocus timing over the same biological focus-plane model.

### Milestone 2: First Real Protocol Families

- Arduino/ESP32/OpenUC2.
- ASI MS-2000.
- Cobolt or Omicron serial laser.
- CoolLED or Lumencor light engine.

Implemented so far:

- `numanager_drivers::asi` provides the current ASI MS-2000/RM-2000 driver support:
  simulated/configured two-stage discovery, config-file candidate creation with
  optional `os-serial` real-port construction that runs the configured startup readback before
  driver registration, serial ASCII command builders for
  `V`, `BU`, `/`, `HALT`, `W`, `M`, `R`, `SPEED`/`S`, `ACCEL`/`AC`, `HOME`,
  and `HERE`, controller hub plus XY/Z stage devices, configured-startup
  command execution and parsing for version, build, status, and XY/Z position
  replies over `SerialIo`, typed `Value::Position` public positions and travel
  metadata, ASI tenths-of-micron wire-unit conversion at the wire boundary,
  `StageMove`, `StageHome`, and `StageStop` capabilities, typed
  `CapabilityRequest::StageMove` support for absolute and relative XY/Z moves
  with optional velocity profiles mapped to `S` in mm/s and acceleration
  profiles mapped to `AC` ramp time in milliseconds, home/stop paths that
  consume immediate ACK when present and request `/` status and `W` position
  readback after command writes,
  status-driven completion,
  remultiplexed XY/Z state sets over one serial resource, sequenceable X/Y/Z
  position properties, and runtime timing-plan hooks that validate typed
  position `DeviceSequence`s and apply first/last sequence values through the
  same remultiplexed XY/Z state-set path.
- `numanager_drivers::asi` also provides the current ASI Tiger driver support:
  simulated/configured two-stage discovery, config-file candidate creation with
  optional `os-serial` real-port construction that runs the configured startup readback before
  driver registration, card-addressed command builders
  over the ASI serial ASCII transport, card inventory metadata, one Tiger hub
  plus logical XY, Z, TTL IO, ring-buffer, and CRISP autofocus devices,
  configured-startup command execution and parsing for controller
  version/build, card-addressed status, XY/Z position replies, and CRISP
  state/focus-score replies over `SerialIo`,
  typed `Value::Position` public stage positions and travel metadata,
  `StageMove`, `StageHome`, `StageStop`, `TriggerSource`, `PulseProgram`, and
  `Autofocus` capabilities, typed absolute/relative `CapabilityRequest::StageMove`
  invocation for XY/Z cards with card-addressed `S`/`AC` profile mapping,
  home/stop paths that consume immediate ACK when present and request
  card-addressed `/` status and `W` position readback after command writes,
  Tiger Z stop addressed to the Z-stage card,
  direct `TriggerSource` invocation through `CapabilityRequest::Trigger` for
  TTL enable/disable/pulse, direct `PulseProgram` invocation through
  `CapabilityRequest::PulseProgram` for ring-buffer start plus optional
  `count`/`wait_for_input` setup,
  busy/idle completion modeling, ASI CRISP
  `LK`/`LR`/`AL` command-family builders for
  state, score, typed position offset/range settings, and objective NA, and
  remultiplexed cross-card state sets over one serial resource. Runtime
  timing-plan hooks now map Tiger XY/Z position, TTL/ring-buffer, and CRISP
  participants onto card-addressed `M`, `TTL X=0 Y=<0/1>`, `RM X=<0/1>`, and
  `LK X=<state>` start/stop commands, with `x`, `y`, `z`, `ttl0`, `running`,
  and CRISP `state` advertised as sequenceable timing properties.
- `numanager-core` includes typed `OpticalPower` and `ElectricCurrent`
  quantities, so laser power/current properties do not need naked unit suffixes.
- `numanager_drivers::cobolt` provides the current serial laser driver support:
  simulated and config-backed two-stage discovery, Cobolt/Hübner serial ASCII command builders,
  configured-startup command execution and parsing for identity, usage hours,
  emission, power/current limits and telemetry, control mode, interlock, fault,
  and autostart replies over `SerialIo`, laser/light-source descriptor, typed wavelength/power/current telemetry and
  setpoints, typed usage-hours telemetry, interlock/fault/autostart/control-mode
  properties, local query-reply ingestion for `l?`, `p?`, `pa?`, `i?`,
  `gom?`, `@cobas?`, `ilk?`, `f?`, and `hrs?`, composite
  telemetry-summary readback, sequenceable typed optical-power and emission properties,
  direct `Dac` invocation through `CapabilityRequest::Dac` with
  `Value::OpticalPower` for public light workflows,
  direct `TriggerSink` invocation through `CapabilityRequest::Trigger`/`None`
  for software pulse, enable, and disable, direct driver preparation that
  rejects mismatched request kinds on typed/generic capabilities, safety-checked
  emission enable, remultiplexed laser state sets over one serial
  resource, and runtime timing-plan arm/start/stop hooks that apply first/last
  optical-power endpoints while mapping emission transitions onto the same
  interlock/fault-guarded enable/disable command path. The configured
  `os-serial` real-port constructor runs the configured startup readback before registering
  explicitly configured serial hardware.
- `numanager_drivers::coherent_obis` provides the first Coherent OBIS laser
  support: simulated and config-backed two-stage discovery, indexed SCPI-like command builders for
  `SYST<n>` and `SOUR<n>` queries/writes, configured-startup command
  execution and parsing for communication handshake/prompt disable, error
  clear/query, head identity/hours, wavelength, power limits/setpoint,
  analog/emission state, and mode over `SerialIo`, head serial, typed usage-hours
  telemetry, typed wavelength and optical-power properties, analog modulation state,
  local query-reply ingestion for `SOUR<n>:AM:STATE?`,
  `SOUR<n>:POW:LEV:IMM:AMPL?`, `SYST<n>:INF:WAV?`,
  `SOUR<n>:AM:SOUR?`, `SYST<n>:ERR?`, `SYST<n>:INF:SNUM?`, and
  `SYST<n>:DIOD:HOUR?`, composite telemetry-summary readback,
  CDRH/CW mode state, shutter-style emission gating through
  `SOUR<n>:AM:STATE`, sequenceable typed optical-power and emission
  properties, direct `Dac` invocation through `CapabilityRequest::Dac` for
  optical-power setpoints, direct `TriggerSink` invocation through
  `CapabilityRequest::Trigger` for software pulse, enable, and disable,
  direct driver preparation that rejects `GenericCommand`/`Custom` requests on
  typed `Dac`/`TriggerSink` capabilities, remultiplexed laser state sets over
  one serial resource, and runtime
  timing-plan arm/start/stop hooks that apply first/last optical-power
  endpoints while mapping emission transitions onto the same fault-guarded
  enable/disable command path. The configured `os-serial` real-port constructor
  runs the configured startup readback before registering explicitly configured serial
  hardware.
- `numanager_drivers::omicron` provides the first legacy Omicron serial laser
  support: simulated and config-backed two-stage discovery, CR-terminated command builders for
  `?GFw`, `?GOM`, `?SOM<hex>`, `?GFB`, `?GWH`, `?GLP`, `?SLP<hex>`, `?MDP`,
  `?GSN`, `?GSI`, `?MTA`, `?MTD`, `?GAS`, `?LOn`, `?LOf`, and `?RsC`,
  configured-startup command execution and parsing for `?GFw`, `?GSN`,
  `?GSI`, operating/fault bits, usage hours, power DAC/actual power, laser
  state, and baseplate/diode temperatures over `SerialIo`, LuxX/PhoxX/BrixX metadata, typed wavelength/power/temperature/usage-hours
  telemetry, 12-bit DAC-level power mapping, operating-mode and ACC/APC submode
  state, raw operating-bit exposure, decoded analog/digital/APC modulation
  status, fault-bit decoding with interlock status, local query-reply ingestion
  for `?GAS`, `?GLP`, `?MDP`, `?GSI`, `?GOM`, `?GFB`, `?GSN`,
  `?GWH`, `?MTD`, and `?MTA`, composite telemetry-summary readback,
  safety-checked emission
  enable, sequenceable typed optical-power and emission properties, direct
  `Dac` invocation through `CapabilityRequest::Dac` for optical-power or percent
  setpoints, direct `TriggerSink` invocation through
  `CapabilityRequest::Trigger` for software pulse, enable, and disable,
  direct driver preparation that rejects `GenericCommand`/`Custom` requests on
  typed `Dac`/`TriggerSink` capabilities, remultiplexed laser state sets over
  one serial resource, and runtime
  timing-plan arm/start/stop hooks that apply first/last optical-power
  endpoints while mapping emission transitions onto the same fault-guarded
  `?LOn`/`?LOf` command path. The configured `os-serial` real-port constructor
  runs the configured startup readback before registering explicitly configured serial
  hardware.
- `numanager_drivers::agilent_laser_combiner` provides
  Agilent/Keysight combiner support from external protocol evidence: configured
  discovery, serial request/reply command encoding for identity, state-mask,
  shutter, external control, blanking, sync, line-power, wavelength, voltage,
  max-power, and calibration surfaces, typed `Ratio` and `OpticalPower`
  public line outputs with raw DAC counts kept at the wire boundary, line
  `TriggerSink` and `Dac` capabilities, hub shutter `TriggerSink`, remultiplexed
  line state sets over one serial resource, and runtime timing-plan hooks that
  validate sequenceable line `enabled`, `intensity`, and `power` endpoints and
  apply first/last values through the existing command echo request/reply paths.
  Hardware sequence opcodes, accepted sequence limits, trigger/sync timing,
  and the safety story for a protocol with no interlock/fault readback remain
  pending hardware validation.
- `numanager_drivers::coolled` provides the first CoolLED light-engine support:
  simulated and config-backed two-stage discovery through
  `CoolLedPe300Discovery::from_config`,
  `CoolLedPe4000Discovery::from_config`, and
  `CoolLedPe340Discovery::from_config`, optional pE-300 and pE-4000-family
  real serial construction behind `numanager-drivers/os-serial` that runs the
  configured startup readback before driver registration, CoolLED
  pE-4000/pE-340 serial ASCII command builders
  for `XMODEL`, `XVER`, `CSS?`, `LAMS`, `LOAD:<wavelength>`,
  `C{A-D}I<percent>`, `C{A-D}S`, `C{A-D}X`, `CSN`, `CSF`, and `PORT:P=...`,
  mapped pE-4000-family configured-startup command execution and parsing for
  `XMODEL`, `XVER`, `CSS?`, `LAMS`, and `C{A-D}?` replies over `SerialIo`,
  CoolLED pE-300 command builders for `XMODEL`, `XVER`, `CSS?`,
  `C{A-C}I<percent>`, `C{A-C}S`, `C{A-C}X`, `C{A-C}?`, `CSN`, `CSF`, and
  `PORT:P=...`, mapped pE-300 configured-startup command execution and parsing for
  `XMODEL`, `XVER`, `CSS?`, and `C{A-C}?` replies over `SerialIo`,
  model-specific hub/global shutter plus channel devices,
  model-specific pE-4000 and pE-340 device-prefix fixtures, channel-specific
  typed wavelength enum properties where the model supports wavelength loading,
  intensity and selected/enabled channel state, pod lock, global output state,
  direct channel `Dac` invocation for percent intensity, direct channel
  `TriggerSink` invocation for software pulse/enable/disable, hub
  `TriggerSink` invocation for global output gating, remultiplexed light state
  sets over one serial resource, sequenceable global enable plus channel
  enable/selection/intensity properties, hub state-summary readback, direct
  driver preparation that rejects `GenericCommand`/`Custom` requests on typed
  `Dac`/`TriggerSink` capabilities, and pE-300/pE-4000-family runtime
  timing-plan hooks that apply first/last light-state endpoints while pE-4000
  start/stop transitions also map onto `CSN`/`CSF` global output gating.
  Direct channel `Dac` and channel/hub `TriggerSink` invocations accept typed
  `CapabilityRequest::Dac` and `CapabilityRequest::Trigger` requests for the
  public light-source workflow.
- `numanager_drivers::lumencor` provides the first Lumencor Spectra/SpectraX
  light-engine and CIA trigger-controller support: binary legacy serial command
  builders for startup GPIO setup (`57 02 ff 50`, `57 03 ab 50`), channel
  enable masks (`4f <mask> 50`), per-channel DAC levels
  (`53 18/1a ... 50`), and white level updates, CIA newline command builders
  for `#H`, `#D`, `#E`, `#P`, `#R`, `#S`, `#T`, `#@`, and `#I`, configured
  discovery fixtures, configured-startup command execution for the legacy
  Spectra startup command surface and CIA `#I`/engine/polarity setup, active
  probe command-script metadata, one hub/global shutter device plus six
  color-channel devices, one `lumencor-cia` pulse-program/trigger-controller
  device, typed wavelength metadata for red, green, cyan, violet, blue, and
  teal, percentage intensity and enable properties, YG filter and shutter/open
  properties, direct channel `Dac` invocation through `CapabilityRequest::Dac`
  for percent intensity, direct channel `TriggerSink` invocation through
  `CapabilityRequest::Trigger` for software pulse/enable/disable, hub
  `TriggerSink` invocation through `CapabilityRequest::Trigger` for open/close
  shutter gating, direct CIA `PulseProgram` invocation with typed timing
  fields that downloads the configured `levels`/`events` properties, direct CIA
  `TriggerSink` invocation through `CapabilityRequest::Trigger` for run/stop/pulse
  control, direct driver preparation that rejects `GenericCommand`/`Custom`
  requests on typed Spectra and CIA capabilities while keeping CIA
  `GenericCommand` as an explicit diagnostic surface, per-channel trigger mode,
  TTL polarity, configured analog-level cache, config-backed
  discovery through `LumencorSpectraDiscovery::from_config` and
  `LumencorCiaDiscovery::from_config`, a hub trigger-profile summary,
  a hub state-summary readback, CIA engine/polarity/level/event/run-state
  properties, and remultiplexed light
  or trigger-program state sets over one serial resource. Runtime timing-plan
  hooks now map legacy Spectra arm/start/stop onto sequenceable global shutter,
  per-channel enable, and per-channel intensity endpoints through the same
  remultiplexed light-state path, while CIA timing maps to download/run/stop.
  Configured real serial construction runs the Spectra startup probe or CIA
  setup probe behind `numanager-drivers/os-serial`.
- `numanager_drivers::zaber` provides the first standards/manual-based
  SDK-counterexample motion support: simulated two-stage discovery, Zaber ASCII
  CRLF command builders for `get`, `set`, `move abs`, `move rel`, `home`, and
  `stop`, configured-startup command-script generation and reply parsing for
  `device.id`, `system.serial`, `peripheral.id`, `limit.max`, `resolution`,
  `pos`, `maxspeed`, `accel`, status, and accumulated warnings, mapped
  configured-startup execution over the shared nonblocking `SerialIo` interface,
  raw warning-token preservation plus read-only `warning_summary` properties
  that classify warnings into none/recoverable/limit-or-safety/command-or-data/
  unknown categories with severity metadata,
  multi-axis probe aggregation that turns parsed address/axis replies into one
  claim candidate per physical axis for the existing two-stage discovery flow,
  single-axis driver mode with one serial hub plus one stage-axis device,
  multi-axis hub driver mode that exposes multiple logical stage axes over one
  shared serial resource and remultiplexes cross-axis state sets, probed
  microstep-size
  conversion between native units and typed `Value::Position` public
  properties, typed `Value::Position` travel and microstep-size metadata plus
  typed `Value::Velocity` and `Value::Acceleration` motion settings,
  `StageMove`, `StageHome`, and `StageStop` capabilities, typed
  `CapabilityRequest::StageMove` support for absolute and relative single-axis
  moves with optional `MotionProfile`, sequenceable `position` properties,
  runtime timing-plan hooks that validate typed position
  `DeviceSequence`s and apply first/last sequence values on start/stop for
  both single-axis and multi-axis drivers, status-driven completion modeled
  from command replies and `get pos`, local query-reply ingestion for `get device.id`,
  `get system.serial`, `get pos`, `get maxspeed`, and `get accel` replies in
  both single-axis and multi-axis drivers, move/home/stop command-status ingestion
  plus `get pos` refresh when serial replies are available, `HardwareConfig`
  driven discovery through
  `ZaberAsciiDiscovery::from_config`, and an optional `os-serial` real-port
  constructor for explicitly configured serial hardware with opt-in
  configured-port readback.
- `numanager_drivers::standa` has been re-added as spec-backed
  8SMC4 serial support from the official communication protocol rather than the
  SDK-shaped Micro-Manager/libximc API surface. It exposes config-backed
  two-stage discovery, optional configured real serial construction behind
  `os-serial`, one controller hub and one logical `stage.1d` axis, typed
  `Position`, `Velocity`, and `Acceleration` properties, `StageMove`,
  `StageHome`, and `StageStop`, runtime timing-plan hooks that validate
  sequenceable `position` values and apply first/last endpoints through the
  same absolute `move` path, CRC-16/MODBUS frame serialization for `move` and
  `movr`, command echo handling for `move`/`movr`/`home`/`stop`, and mapped
  `gser`/`gpos`/`gets` configured-startup parsing, move/home/stop paths that refresh
  `gets` status and `gpos` position after command ACKs, including documented
  `MvCmdSts`, position, speed, flags, GPIO edge flags, power state, and encoder state
  fields. Broader status/error surfaces, engine-setting writes, multi-axis
  behavior, and hardware validation need protocol evidence or traces.
- `numanager_drivers::hamilton_mvp` adds spec-backed Hamilton Serial
  MVP startup-readback valve-positioner support for Protocol 1/RNO+. It exposes
  config-backed two-stage discovery, optional configured real serial
  construction behind `os-serial`, one fluidics controller hub plus one logical
  valve device per configured address, `ValveSelect` for ordinal valve-position
  selection, writable `position`, read-only `address`, `port_count`,
  `valve_type`, `busy`, `valve_error`, `status_raw`, and `state_summary` valve
  properties, address-list hub metadata, 7O1/CR serial resource metadata,
  Protocol 1/RNO+ `LPdppR`, `LQP`, `LQT`, `F`, `G`, `E1`, and `U` command
  handling, hidden `LXR` initialization, and status-driven real-serial
  completion polling from the documented `E1` valve-busy bit. Real serial
  construction reads firmware, valve type, current position, `E1` status, done,
  and valve-error state for every configured address before registration behind
  `numanager-drivers/os-serial`. Hub refresh commands return address-keyed maps,
  and cross-valve state sets remultiplex addressed `position` writes over the
  shared serial resource before refreshing `LQP`. DIN/BDZ+ command behavior,
  daisy-chain edge cases, and broader safety behavior are outside the recorded
  Protocol 1/RNO+ subset.
- `numanager_drivers::trinamic_tmcl` adds spec-backed TMCL
  direct-mode startup-refresh motion support. It exposes config-backed two-stage discovery,
  optional configured real serial construction behind `os-serial`, one
  controller hub plus one logical `stage.1d` device per configured axis, typed
  `Position` conversion through configured `step_size`, typed `StepCount`
  diagnostics for actual/target microsteps, `ControllerScalar` speed and
  acceleration properties for controller-specific `pps`/`pps2` values,
  `StageMove` via documented `MVP ABS`/`MVP REL`, `StageStop` via `MST`, and
  hardware-driven completion by polling documented `GAP` parameters for actual
  position, target position, actual speed, and position-reached state. Real
  serial construction refreshes documented `GAP` axis state before registration.
  Runtime timing-plan arm/start/stop hooks validate sequenceable `position` and
  `target` endpoints and apply first/last endpoints through the same `MVP`/`GAP`
  path. Homing, identity probing, physical velocity conversion, and synchronized
  hardware-timed multi-axis motion remain evidence-gated by missing timing
  evidence; hardware validation is tracked separately.
- `numanager_drivers::prior` provides the first Prior ProScan/OptiScan controller
  support: simulated and config-backed two-stage discovery through
  `PriorDiscovery::from_config`, optional configured real serial construction
  behind `numanager-drivers/os-serial` that runs the configured startup readback before driver
  registration, CR-terminated command builders for
  `COMP 0`, `$`, `DATE`, `G,<x>,<y>`, `GR,<dx>,<dy>`, `PS,0,0`, `PX`, `PY`,
  `SIS`, `K`, `SMS`, `SAS`, `PZ`, `U,<dz>`, `D,<dz>`, `RES,Z`,
  `7,<wheel>,<pos>`, `7,<wheel>,h`, `8,<id>,<0/1>`, and
  `TTL,<line>,<0/1>`, NanoScanZ `V <um>`/`PZ`, and Lumen 200Pro
  `Light,<intensity>`, configured-startup command execution and parsing for
  `COMP 0`, `DATE`, `$`, `PX`, `PY`, `PZ`, `RES,Z`, shutter, and TTL replies
  over `SerialIo`, controller hub plus XY stage, Z stage, NanoScanZ,
  filter wheel, shutter, Lumen, and TTL output devices, typed
  `Value::Position` public positions for XY, Z, and NanoScanZ, percentage
  scalar speed/acceleration settings, typed `Value::TimeInterval` Lumen delay,
  read-only hub `state_summary`, typed `Value::Position` travel and
  step-to-micrometer metadata, local query-reply ingestion for `DATE`, `$`,
  `PX`, `PY`, `SMS`, `SAS`, `PZ`, shutter, and TTL readbacks, `StageMove`, `StageHome`,
  `StageStop`, `TriggerSource`, and `TriggerSink` capabilities with direct
  TTL high/low/pulse, shutter open/close/pulse, and Lumen open/close/pulse
  invocation through `CapabilityRequest::Trigger` or `None`,
  typed
  `CapabilityRequest::StageMove` support for absolute and relative XY, Z, and
  NanoScanZ moves, status-driven completion based on `$` busy bits or command
  acknowledgement, home/stop paths that refresh `$` status plus mapped stage
  position after live ACK, remultiplexed XY/Z/light state sets over one serial
  resource, sequenceable X/Y/Z and NanoScanZ position properties plus
  sequenceable TTL/shutter/Lumen boolean output properties, and runtime
  timing-plan hooks that apply first/last stage and output sequence
  values through the same remultiplexed write path while preserving
  route/participant-only transition defaults for TTL high/low, shutter
  open/close, and Lumen open/close property paths.
- `numanager_drivers::sutter_stage` provides the first SutterStage/Ludl-compatible
  controller support: simulated two-stage discovery, CR/LF command builders for
  `VER`, `Rconfig`, `Remres`, `TRXDEL`, `STATUS <axis>`, `MOVE X=<x> Y=<y>`,
  `MOVREL X=<dx> Y=<dy>`, `WHERE X Y`, `HERE X=0 Y=0`, `HOME X Y`, `HALT`,
  `SPEED`, `STSPEED`, `ACCEL`, `MOVE <axis>=<pos>`, `WHERE <axis>`, and
  `AF <axis>=<param>`, configured-startup command execution and parsing for
  `VER`, `Rconfig`, `Remres`, `TRXDEL`, axis `STATUS`, `WHERE X Y`,
  `WHERE <z-axis>`, `SPEED`, `STSPEED`, and `ACCEL` replies over `SerialIo`,
  controller hub plus XY stage, Z stage, and generic autofocus provider devices,
  typed `Value::Position` step-to-micrometer conversion metadata, typed
  `Value::Position` and `Value::Velocity` public motion properties with step
  conversion at the wire boundary, integer controller-step acceleration plus
  integer-tick transmission-delay properties, `StageMove`, `StageHome`,
  `StageStop`, and `Autofocus` capabilities, typed
  `CapabilityRequest::StageMove` support for absolute and relative XY/Z moves
  with optional XY and Z velocity-only profiles while rejecting typed
  acceleration profiles until the native `ACCEL` controller scalar has
  calibration evidence, typed `CapabilityRequest::Autofocus`
  support through the `AF <axis>=<parameter>` command, a device-graph
  dependency from the autofocus provider to the Sutter Z stage, status-driven
  completion based on `STATUS <axis>`,
  remultiplexed XY/Z state sets over one serial resource, sequenceable X/Y/Z
  position properties plus sequenceable autofocus enable/mode properties,
  runtime timing-plan arm/start/stop hooks that apply first/last position and
  autofocus state sequence values through the same remultiplexed write path,
  hub state-summary readback covering XY/Z/autofocus state, local query-reply
  ingestion for `VER`, `Rconfig`, `TRXDEL`, `STATUS <axis>`, `WHERE X Y`,
  `WHERE <z-axis>`, `SPEED X Y`, `STSPEED X Y`, and `ACCEL X Y` readbacks,
  home/stop paths that request mapped status and position readbacks after
  command writes while retaining cached configured state when no reply is
  available,
  `HardwareConfig`
  driven discovery through `SutterStageDiscovery::from_config`, and an
  optional `os-serial` real-port constructor that runs the configured startup readback before
  registering explicitly configured serial hardware.
- `numanager_drivers::sutter_mp285` provides the first Sutter MP-285
  micromanipulator support: simulated two-stage discovery, binary serial command
  builders for status, position read (`c`), absolute XYZ move (`m` plus
  three little-endian 32-bit microstep coordinates), velocity setup (`V`), and
  stop (`0x03`), with reset and current-position-as-origin retained only as
  hidden protocol primitives, configured-startup command execution and
  parsing for status bytes, position readback, and configured
  velocity ACK over `SerialIo`, controller hub plus XY and Z logical stages,
  typed `Value::Position` stage properties and typed `Value::Velocity`
  controller velocity with microstep conversion only at the wire boundary,
  read-only hub `status_summary`, typed travel and microstep-size metadata,
  `StageMove` and `StageStop` capabilities, typed
  `CapabilityRequest::StageMove` support for absolute and relative XY/Z moves
  with optional controller velocity-only profiles while rejecting acceleration
  profiles because the documented command surface does not expose typed
  acceleration, optional controller ACK/error ingestion for move/stop,
  status/position readback after move/stop when replies are available,
  remultiplexed XYZ state
  sets over one serial resource, sequenceable X/Y/Z position properties,
  runtime timing-plan arm/start/stop hooks that apply first/last position
  sequence values through the same remultiplexed XYZ move path,
  `HardwareConfig` driven discovery
  through `Mp285Discovery::from_config`, and an optional `os-serial` real-port
  constructor that runs the configured startup readback before registering explicitly
  configured serial hardware.
- `numanager_drivers::marzhauser` provides the first Marzhauser TANGO/L-Step
  controller support: CR-terminated command builders for `?ver`, `?version`,
  `!autostatus 0`, `?det`, `?pitch`, `!dim`, `?vel`, `!vel`, `?accel`,
  `!accel`, `!moa`, `!mor`, `?pos`, `!pos`, `!cal`, `!speed`, `a`, `?err`,
  `?statusaxis`, and `?lim`, configured-startup command execution and
  parsing for version/controller, autostatus disable, `?det`, pitch, velocity,
  acceleration, position, `?err`, `?statusaxis`, and limit replies over
  `SerialIo`, controller hub plus XY and Z stage devices,
  typed `Value::Position` pitch-to-step metadata, typed `Value::Position`, `Value::Velocity`, and
  `Value::Acceleration` public motion properties with protocol conversions at
  the wire boundary, read-only limit-reply properties from `?lim`, read-only
  local query-reply ingestion for `?ver`, `?det`, `?statusaxis`, `?pos`,
  `?pos z`, `?vel`, `?accel`, `?err`, and `?lim` replies, read-only
  hub `state_summary` covering controller identity, limits, and typed X/Y/Z
  motion state, `StageMove`,
  `StageHome`, and `StageStop` capabilities,
  typed `CapabilityRequest::StageMove` support for absolute and relative XY/Z
  moves with optional velocity/acceleration profiles, remultiplexed XY/Z state
  sets over one serial resource, sequenceable X/Y/Z position plus speed and
  acceleration properties, runtime timing-plan arm/start/stop hooks that apply
  first/last position, speed, and acceleration sequence values through the same
  remultiplexed move/write path, home/stop paths that request mapped busy,
  position, and error readbacks after command writes while retaining configured
  cached configured state when no reply is available, `HardwareConfig`
  driven discovery through `MarzhauserDiscovery::from_config`, and an
  optional `os-serial` real-port constructor that runs the documented configured startup readback
  script and seeds cached identity, position, velocity, acceleration, limit,
  and busy state from controller replies before exposing the driver.
- `numanager_drivers::pi_gcs` provides the first Physik Instrumente GCS/GCS2
  controller support: LF-terminated command builders for `*IDN?`, `CSV?`,
  `SAI?`, `SVO`, `SVO?`, `FRF`, `MOV`, `MVR`, `POS?`, `VEL`, `VEL?`, `ACC`,
  `ACC?`, `HLT`, `STP`, `ERR?`, `ONT?`, and the PI moving-status byte, configured-startup
  command execution and parsing for controller ID, syntax version, axis list,
  servo state, position, velocity, acceleration, `ONT?`, `ERR?`, and the moving-status byte
  over `SerialIo`, a configured discovery fixture, controller hub plus XY and Z stage devices, typed
  `Value::Position`, `Value::Velocity`, and `Value::Acceleration` public motion properties with
  controller-default-unit conversion at the wire boundary, typed
  `Value::Position` travel and controller-default-unit-size metadata, servo and
  referenced-axis metadata, read-only referenced-axis properties, `StageMove`,
  `StageHome`, and `StageStop`
  capabilities, typed `CapabilityRequest::StageMove` support for absolute and
  relative XY/Z moves with optional velocity and acceleration profiles,
  sequenceable X/Y/Z position and servo properties, runtime timing-plan hooks
  that validate typed position and servo `DeviceSequence`s and apply first/last sequence
  values through the remultiplexed XY/Z state-set and write-property paths, remultiplexed XY moves
  over one serial resource, read-only hub `state_summary` covering controller
  feature flags and typed X/Y/Z axis state, local query-reply ingestion for
  `*IDN?`, `CSV?`, moving-status byte/`ONT?`, `POS?`, `VEL?`, `ACC?`,
  `SVO?`, and `ERR?` readbacks, move/home/stop paths that request mapped busy,
  position, and error readbacks after command writes while retaining configured
  cached configured state when no reply is available, `HardwareConfig` driven discovery through
  `PiGcsDiscovery::from_config`, and an optional `os-serial` real-port constructor
  that runs the configured startup readback before registering explicitly configured serial
  hardware.
- `numanager_drivers::thorlabs_apt` provides the first Thorlabs APT-compatible
  motor support: binary packet builders for `MGMSG_HW_REQ_INFO`,
  `MGMSG_MOD_SET_CHANENABLESTATE`, `MGMSG_MOT_MOVE_HOME`,
  `MGMSG_MOT_REQ_POSCOUNTER`, `MGMSG_MOT_MOVE_ABSOLUTE`,
  `MGMSG_MOT_MOVE_RELATIVE`, `MGMSG_MOT_ACK_DCSTATUSUPDATE`,
  `MGMSG_MOT_MOVE_STOP`, `MGMSG_MOT_REQ_DCSTATUSUPDATE`,
  `MGMSG_MOT_REQ_VELPARAMS`, and `MGMSG_MOT_SET_VELPARAMS`, configured-startup
  command-script generation and reply parsing for hardware info,
  position, status, and velocity profile frames, configured discovery,
  controller hub plus one logical stage axis, typed `Value::Position`,
  `Value::Velocity`, and `Value::Acceleration` public motion properties with
  encoder-count conversion at the wire boundary,
  typed `Value::Position` travel and encoder step-size metadata,
  velocity-profile, raw status-bit, and decoded status-summary properties, `StageMove`, `StageHome`, and
  `StageStop` capabilities, typed `CapabilityRequest::StageMove` support for
  absolute and relative single-axis moves with optional velocity/acceleration
  profiles, sequenceable `position` property, runtime timing-plan hooks
  that validate typed position `DeviceSequence`s and apply first/last sequence
  values on start/stop, status/position frame readback after motion/home/stop
  command writes, velocity-profile readback after profile writes,
  hardware-completion modeling from `MGMSG_MOT_MOVE_COMPLETED`/status bits, and an optional `os-serial` real-port
  constructor that runs the configured startup readback before registering explicitly
  configured serial hardware.
- `numanager_drivers::thorlabs_dc` provides the first Thorlabs DC2010/DC2100,
  DC2200, DC3100, and DC4100/DC4104/LEDD4 LED-controller support: CRLF command builders
  for `n?`, `s?`, `v?`, `hs?`, `wl?`, `fb?`, `o?`, `o <0/1>`, `l?`,
  `l <mA>`, `ml?`, `cc?`, `cc <mA>`, `pc?`, `pc <mA>`, `pf?`, `pf <Hz>`,
  `pd?`, `pd <percent>`, `pn?`, `pn <count>`, `m?`, `m <mode>`, `r?`, and
  `e?`, plus DC3100 amp-valued `l <A>`, `cc <A>`, `cm?`, `cm <A>`, `f?`,
  `f <Hz>`, `d?`, `d <percent>`, and `mf?`, plus a DC2200 SCPI-style
  config-backed command set for `*IDN?`, `SYST:SER?`, `SYST:VERS?`,
  `SYST:ERR?`, `OUTP?`, `OUTP <0/1>`, `CURR?`, `CURR <A>`,
  `CURR:LIM?`, `CURR:LIM <A>`, `PULS:CURR?`, `PULS:CURR <A>`,
  `PULS:FREQ?`, `PULS:FREQ <Hz>`, `PULS:DCYC?`,
  `PULS:DCYC <percent>`, `SOUR:FUNC?`, `SOUR:FUNC CURR/PULS/EXT`,
  and `STAT:QUES:COND?`, plus DC4100 channel/multi-select
  commands `sm?`, `sm 0`, `o -1 <0/1>`, `o? <channel>`,
  `o <channel> <0/1>`, `cc? <channel>`, `cc <channel> <mA>`,
  `bp? <channel>`, `bp <channel> <percent>`, `l? <channel>`,
  `l <channel> <mA>`, `ml? <channel>`, `wl? <channel>`, `fb? <channel>`, and
  `hs? <channel>`; configured-startup command-script generation and reply
  parsing for controller identity, firmware, LED-head metadata, output state,
  operation mode, current/frequency limits, status/error replies, and DC4100
  channel inventory; local query-reply ingestion for controller output,
  operation mode, current, PWM, modulation, maximum frequency, status,
  wavelength, forward-bias, firmware, LED-head serial, and DC4100 channel
  output/current/brightness/inventory readbacks; configured discovery
  fixtures, one logical LED
  controller/light-source/shutter device for single-channel controllers, one hub
  plus four channel devices for DC4100-class controllers, typed current,
  wavelength, forward-bias voltage, and PWM/internal-modulation frequency values
  where available, operation-mode enum values, remultiplexed state sets that
  configure mode/current/brightness/modulation before output enable, and
  completion through serial write acceptance plus `e?` hardware error polling.
  Direct `TriggerSink` invocation now accepts `CapabilityRequest::Trigger` for
  software pulse, enable, and disable by using the same output command path as
  the `enabled` property, and direct `Dac` invocation accepts
  `CapabilityRequest::Dac` for typed current or brightness requests mapped onto
  the same `constant_current`, `pwm_current`, and DC4100 channel `brightness`
  property-backed command paths.
  `HardwareConfig` driven discovery is available through
  `ThorlabsDcDiscovery::from_config`, with canonical typed keys for
  wavelength, voltage, current, frequency, and DC4100 channel metadata plus an
  optional `os-serial` real-port constructor that runs the configured startup readback before
  registering explicitly configured serial hardware.
  Runtime timing-plan hooks now validate local sequenceable LED endpoints and
  start/stop by applying first/last `enabled`, constant-current, PWM-current,
  and DC4100 channel brightness values through the same property-backed command
  paths, with legacy participant-only output-enable/output-disable behavior
  preserved for plans that do not provide explicit `enabled` sequences.
- `numanager_drivers::thorlabs_kurios` provides the first KURIOS LCTF CLI support:
  configured discovery, documented keyword-query and keyword-assignment command
  builders, configured-startup command-script generation and reply parsing
  for model, serial, firmware, status, wavelength, bandwidth, output state, and
  trigger mode, one tunable-filter device with typed wavelength and bandwidth
  properties, trigger-mode and output-enable state, serial resource metadata,
  remultiplexed filter state sets over one serial resource, direct
  `TriggerSink` invocation through `CapabilityRequest::Trigger` for software
  pulse/enable/disable through the `OUTPUT=<0/1>` path, direct driver
  preparation that rejects generic command-map aliases on the typed trigger
  capability, sequenceable wavelength, bandwidth, and `output_enabled`
  timing properties, runtime timing-plan hooks that validate typed
  filter/output sequences and apply first/last sequence values on start/stop,
  an optional `os-serial` real-port constructor that runs the configured startup readback before
  registering explicitly configured serial hardware, and runtime-owned
  completion through the driver token/event path. This implements the preferred
  CLI-first route before any fallback-evidence-informed work.
- `numanager_drivers::thorlabs_sc10` provides the first SC10 shutter-controller
  support: configured discovery, controller plus logical shutter devices, one
  `serial.ascii` resource, typed public properties for `enabled`, `mode`,
  `open_time`, `close_time`, `trigger_mode`, `repeat_count`,
  `interlock_closed`, and `fault`, enum-backed public mode/trigger values,
  `TimeInterval` conversion kept at the private protocol boundary, direct
  `TriggerSink` invocation for open/close/pulse, remultiplexed shutter state
  sets through the controller resource, runtime-owned completion through
  driver tokens/events, runtime timing-plan hooks that validate sequenceable
  shutter endpoints and apply first/last `open`, `mode`, `open_time`,
  `close_time`, `trigger_mode`, and `repeat_count` values through the same
  write/readback paths, a generic `shutter` workflow example with recorded
  output, and a device page/evidence entry. Configured real serial construction
  now reads `*idn?`, `ens?`, `mode?`, `open?`, `shut?`, `trig?`, and `rep?`
  before registration. Real SC10 prompt/readback/completion, hardware timing,
  and interlock/alarm behavior need hardware validation.

Remaining:

- Hardware validation of ASI MS-2000 configured serial startup readback and expansion from
  the current mapped readback (`V`, `BU`, `/`, `W X Y`, `W Z`) to fuller
  interrogation and model-specific feature discovery. Configured serial
  construction runs the configured startup readback behind
  `numanager-drivers/os-serial`.
- ASI hardware validation for move/home/halt/status semantics.
- ASI MS-2000 hardware-accurate timing and acquisition-plan validation beyond
  the current software position-sequence timing hook.
- Hardware validation of ASI Tiger configured serial startup readback and expansion from the
  current configured-card readback (controller `V`/`BU`, card-addressed
  `/`, `W X Y`, `W Z`, CRISP `LK X?`/`LK Y?`) to card address negotiation,
  module inventory parsing, axis/card capability mapping, and model-specific
  Tiger feature flags. Configured serial construction runs the configured
  startup readback behind `numanager-drivers/os-serial`.
- Real CRISP autofocus hardware validation, including lock-state parsing,
  firmware-dependent `LK`/`EXTRA` shortcut handling, wait-after-lock behavior,
  focus-curve acquisition, LED/turret, hardware-triggered TTL/ring-buffer
  routing beyond the current software start/stop timing hook, and scan-module
  support.
- Hardware validation of Prior configured serial startup readback and expansion from the
  current probe (`COMP 0`, `DATE`, `$`, `PX`, `PY`, `PZ`, `RES,Z`,
  shutter, TTL) to command availability checks, wheel/shutter/TTL inventory,
  and ProScan/OptiScan model-specific feature flags.
- Prior queueing, trigger-board, encoder, richer TTL/pattern support, and
  hardware validation for Lumen/NanoScanZ timing and error replies from the
  ProScan III command manual.
- Hardware validation of SutterStage/Ludl-compatible configured serial startup readback and
  expansion from the current probe (`VER`, `Rconfig`, `Remres`,
  `TRXDEL`, axis `STATUS`, `WHERE`, `SPEED`, `STSPEED`, `ACCEL`) to fuller axis
  IDs, command-level negotiation, status/error replies, and model-specific
  feature discovery. Configured serial connection is available behind
  `numanager-drivers/os-serial`.
- Hardware validation of Sutter MP-285 configured serial startup readback and expansion from
  the current probe (status bytes, position reply, configured velocity ACK) to
  fuller firmware status decoding, controller ACK/error-byte
  behavior, resolution/velocity negotiation, interrupt behavior, and
  motion-completion timing from real hardware, plus hardware streaming/timing
  behavior beyond the current software XYZ position-sequence hook. Configured
  serial connection is available behind `numanager-drivers/os-serial`.
- Sutter single-axis device discovery, module inventory parsing, shutter/filter
  modules, richer autofocus semantics, hardware-triggered queue/trigger
  support beyond the current software position-sequence timing hook, and
  model-specific limits.
- Hardware validation of Marzhauser TANGO/L-Step configured serial startup readback and
  expansion from the current probe and local readback ingestion
  (`?ver`, `?version`, `!autostatus 0`, `?det`,
  pitch/velocity/acceleration/position, `?err`, `?statusaxis`, `?lim`) to
  fuller controller version negotiation, `?det` axis/configuration decode,
  continuous motion, scan/trigger behavior beyond the current software
  position-sequence timing hook, and TANGO TTL/DAC/ADC modules. Configured serial connection is available behind
  `numanager-drivers/os-serial`.
- Hardware validation of PI GCS/GCS2 configured serial startup readback and expansion from
  the current probe (`*IDN?`, `CSV?`, `SAI?`, `SVO?`, `POS?`, `VEL?`,
  `ACC?`, `ONT?`, `ERR?`, moving-status byte) to stage assignment, axis referencing
  state, limit queries, controller-default-unit conversion, `ONT?` vs moving
  status byte model selection, error-code translation, wave generators, data
  recorders, trigger IO, hardware-accurate timing behavior beyond the current
  software position-sequence timing hook, USB/TCP transports, and multi-axis
  controller feature flags. Configured serial connection is available behind
  `numanager-drivers/os-serial`.
- Thorlabs APT hardware-validation and expansion work covers message routing,
  bay/channel enumeration, scale-factor calibration per controller/stage, homing
  parameters, limit switch configuration, velocity/jog profiles, status
  streaming/keepalive, hardware-accurate trigger/timing behavior, multi-channel
  XY coordination, and Kinesis/APT model compatibility. The current support has
  configured-startup parsing for hardware info, position, status, and
  velocity-profile reply frames, configured serial construction behind
  `numanager-drivers/os-serial`, and runtime motion/home/stop/velocity-profile
  paths that request mapped readbacks when available.
- Thorlabs DC2010/DC2100/DC3100/DC4100/DC2200 hardware-validation and expansion
  work covers command availability, exact status/error-code translation,
  LED-head inventory edge cases, model-specific current limits, external-control
  semantics, and hardware trigger timing behavior. The current support has
  configured-startup parsing for identity, LED-head metadata, status/error,
  limit, DC4100 channel-inventory replies, local query-reply ingestion for
  controller and DC4100 channel properties, explicit-config DC2200 USBTMC
  control/readback, and software timing-plan start/stop output transitions.
  Direct driver preparation rejects `GenericCommand`/`Custom` requests on typed
  `Dac`/`TriggerSink` capabilities; PWM current remains exposed through the
  typed `pwm_current` property rather than an ad hoc direct command map.
- Thorlabs KURIOS hardware-validation and expansion work covers model-specific
  wavelength ranges, bandwidth modes, calibration/status/error replies,
  warmup/shutter behavior, and hardware trigger timing beyond the current
  software output-gating hook. The current support has configured-startup
  parsing for identity, status, wavelength, bandwidth, output state, and trigger
  mode, local query-reply ingestion for `WL?`, `BW?`, `OUTPUT?`, `TRIG?`,
  `STATUS?`, and `VERSION?`, and configured serial construction behind
  `numanager-drivers/os-serial`. Use reverse engineered evidence only to
  validate missing CLI coverage if the manual and hardware traces are
  insufficient.
- Hardware validation of Cobolt configured serial startup readback through the configured
  `numanager-drivers/os-serial` connection and expansion from the current
  probe and local readback ingestion (`@cob0`, identity, hours,
  emission, power/current, mode, interlock, fault, autostart) to real
  interlock/fault/emission transition timing and model-specific command quirks.
- Hardware validation of Coherent OBIS configured serial startup readback through the
  configured `numanager-drivers/os-serial` connection and
  expansion from the current probe and local readback ingestion
  (communication handshake/prompt disable, error clear/query, head
  identity/hours, wavelength, power limits/setpoint, analog/emission state,
  mode) to real CDRH delay timing, full error/fault transitions, modulation
  source variants, prompts, multi-laser remote behavior, and hardware-accurate
  timing transition semantics.
- Hardware validation of Omicron configured serial startup readback through the configured
  `numanager-drivers/os-serial` connection and expansion from the current
  probe and local readback ingestion (`?GFw`, `?GSN`, `?GSI`, `?GOM`,
  `?GFB`, `?GWH`, `?GLP`, `?MDP`, `?GAS`, `?MTA`, `?MTD`) to power
  calibration, reset timing, model-specific reply variants, real fault
  transitions, and hardware-accurate modulation/timing transition semantics.
  Analog and digital modulation are now writable, sequenceable boolean
  properties backed by the documented `?SOM<hex>` operating-bit command, so
  workflow code can select modulation state without string mode dispatch.
- Omicron xX USB/fallback-evidence-backed feature-completeness pass only after serial
  capability gaps are understood.
- Hardware validation of CoolLED pE-4000 configured serial startup readback and expansion
  through the configured `numanager-drivers/os-serial` construction path from
  the current probe (`XMODEL`, `XVER`, `CSS?`, `LAMS`, `C{A-D}?`) to
  exact status semantics, channel selection, pod lock, and global-output/timing
  behavior.
- Hardware validation of CoolLED pE-340 configured serial startup readback and expansion
  through the configured `numanager-drivers/os-serial` construction path from
  the shared pE-4000-family probe to exact wavelength-slot behavior,
  status semantics, channel selection, pod lock, and global-output timing
  behavior.
- Remaining Lumencor work is hardware validation for actual analog behavior, CIA
  TTL/function-generator timing, and model-specific support where public
  manufacturer protocols describe them. Lumencor now has configured-startup
  execution for the documented legacy Spectra startup command surface and CIA
  info/setup commands, plus per-channel trigger-mode, TTL-polarity,
  analog-level, direct channel/hub trigger invocation, configured trigger-profile
  state for acquisition planning, legacy Spectra timing-plan shutter/channel
  gating, CIA timing-plan arm/start/stop fallback binding, config-backed
  discovery, and `numanager-drivers/os-serial` configured serial construction
  that runs the active startup/setup probe.
- Lumencor Spectra/Sola/Gen3 serial or Ethernet expansion needs manufacturer
  command references and hardware traces for model detection, channel inventory,
  calibration, TTL/analog mode selection, trigger profiles, response/ACK
  behavior on newer models, and Gen3-specific telemetry.
- Hardware validation of live Zaber ASCII serial interrogation using
  configured-port readback. Configured serial connection is available behind
  `numanager-drivers/os-serial`; chain-level resource ownership and feature
  discovery need manufacturer database evidence or hardware traces.
- Richer Zaber multi-axis runtime coordination beyond serialized state-set
  remultiplexing, trigger IO, streams, and broader setting/property mapping
  from the public ASCII manual. The current alert support classifies accumulated
  warning tokens into stable summary categories but still needs hardware
  validation against the full public warning table.
- Standa follow-through: the driver uses the official 8SMC4 serial protocol for
  single-axis `gser`/`gpos`/`gets`/`gmov`/`geng`/`gbrk`/`ghom`/`smov` plus
  `move`/`movr`/`home`/`stop` support. Hardware-validation and expansion work
  covers status/error validation, additional encoder and engine settings,
  homing/motion-profile calibration, multi-axis controllers, TTL IO, and
  hardware-accurate trigger/stream timing. Configured real serial construction
  reads serial number, position, status, movement settings, engine settings,
  brake settings, and home settings before registration.

### Milestone 3: Remultiplexed Multi-Device Hubs

- ASI Tiger.
- Prior or Sutter stage controller.
- Multi-channel illumination state sets.
- Triggered acquisition plan involving camera, stage, and light source.

Implemented so far:

- The runtime has a first cross-driver timing-plan path: `Command::Arm` stores
  a validated `TimingPlan` above driver lanes, while `Command::Start` and
  `Command::Stop` transition that armed plan and report all participant
  devices through `OperationChanged` events. It snapshots registered
  descriptors/capabilities so trigger routes must connect
  `TriggerSource`/`TriggerSink` devices and sequences must target real
  `sequenceable` properties with schema-valid values. It also sends the plan to
  every involved driver lane for `prepare_timing_plan`, preserving each
  driver's returned physical arm transactions in the operation summary. Start
  and stop transitions now also call per-driver timing hooks and report their
  physical transition transactions. The CoolLED pE-300 fixture now persists
  readable timing execution state across arm/start/stop, proving that a
  hardware-facing driver can react to those transitions. Dispatching those
  transitions to real trigger engines remains the next integration step for
  drivers with hardware timing support.
- `numanager-examples -- timing_plan` exercises a camera/stage/
  light plan with separate platform-camera, simulated XY/Z motion, and CoolLED
  pE-300 drivers. It configures camera exposure/frame interval, stage position,
  and illumination state, arms a TTL route from camera exposure output to the
  light channel, validates sequenceable XY/camera properties, starts the plan,
  shows per-driver arm/start/stop preparation for camera, motion, and
  illumination lanes, reads back CoolLED timing state across the transitions,
  streams frames through the shared ring-buffer API, and stops the armed plan.
- `numanager-examples -- autofocus` also exercises a single-driver
  composed autofocus timing plan over the biological focus-plane fixture, using
  camera exposure, Z position, light enable/power, and autofocus mode sequences
  that update the same scene-derived focus score.

### Milestone 4: Standards Backends

- Modbus backend.
- GenICam node-map execution model.
- Aravis-backed or Rust-native camera stream backend.
- TIS camera path through open/platform stack.
- Velleman K8055/VM110 and K8061/VM140 configured IO support from manufacturer
  IO docs and open Linux packet evidence; real USB packet backend after
  transport design.
- Starlight Xpress filter-wheel configured serial support from the manufacturer
  wheel handbooks; USB HID backend after the input/output-report transport is
  added.
- Spectral LMM5 configured RS-232 support from the public LMM5 software manual;
  USB/HID backend and full trigger-profile behavior after transport traces and
  hardware validation.

Implemented so far:

- `numanager_drivers::modbus` provides the first generic standards-backend
  support: Modbus RTU CRC-16 and TCP MBAP frame builders, request builders for
  read coils, read discrete inputs, read holding/input registers, write single
  coil/register, and write multiple coils/registers, config-file and
  fixture-driven property mapping for coils plus 16-bit, 32-bit, and 64-bit
  register values (`u16`, `i16`, `u32`, `i32`, `u64`, `i64`, `f32`, `f64`),
  reusable built-in map profiles for the mapped-IO fixture, basic environment
  controllers, incubator environment controllers, live-cell chamber
  controllers, stage-top incubation chamber controllers, pressure/flow
  controllers, and shutter/safety IO controllers, plus laser safety interlock
  controllers, typed `quantity` mappings for
  temperature, gas concentration, pressure, flow, ratio/percent, and time
  values, canonical built-in profile keys such as `humidity`,
  `relative_humidity`, `valve_position`, `pulse_width`, and `cdrh_delay`
  without unit suffixes, typed `poll_intervals` descriptor metadata plus
  canonical typed config `map.<name>.poll_interval` with legacy
  `poll_interval_ms` acceptance, big-word, little-word,
  byte-swap, and full little-endian multi-register ordering, scaled numeric
  mappings, enum-label mappings with
  advertised string choices, single-register bitfield mappings for status/alarm
  bits and masked enum fields, writable holding-register bitfields through
  response-driven read-modify-write, hardware-address metadata on mapped
  properties, configured discovery providers, mapped state-set submission over
  one Modbus transport resource, per-property background polling intervals that
  emit `PropertyChanged` events on value changes, response-frame parsing for
  RTU/TCP, driver-owned response timeout/retry policy, real Modbus TCP stream
  transport enabled by explicit config, MBAP transaction-id response
  correlation, fixture-driven out-of-order TCP response release for exercising
  that correlation path, optional real Modbus RTU serial transport through
  core `OsSerialPort`, exception responses as operation failures, and
  `RawRegisterAccess` for explicit register reads/writes when properties are
  too narrow. Runtime timing-plan hooks now support writable bool coil
  `DeviceSequence` entries by writing each sequence's first value on `Start`
  and last value on `Stop`, using the same mapped coil request path.
- Modbus TCP in-flight response handling now correlates responses by MBAP
  transaction ID instead of assuming the first queued operation must receive the
  next frame. Timeouts scan outstanding TCP requests independently, while RTU
  remains ordered by frame stream. TCP/RTU resources advertise their response
  correlation policy in metadata, and `modbus_io` now exercises the TCP
  correlation path with deliberately out-of-order fixture replies.
- `numanager_core::usb` now defines a small `UsbPacketIo` abstraction plus a
  scripted packet implementation so USB packet drivers do not have to misuse
  serial or HID feature-report traits. `numanager_drivers::velleman` uses that
  abstraction for K8055/VM110 and K8061/VM140 IO support, with an explicit-config
  `os-usb` packet backend when VID/PID, interface, endpoints, and transfer kind
  are provided.
  K8055 exposes one hub, digital input/output devices, two analog input
  devices, and two analog output devices over the documented 8-byte packet.
  K8061 exposes one hub, digital input/output devices, eight 10-bit analog
  input devices, eight 8-bit analog output devices, and one 10-bit PWM output
  over the documented 64-byte packet. Public capabilities are `DigitalIo`,
  `Measure`, `Adc`, and `Dac`, analog/PWM values are typed `Ratio`, K8061 PWM
  frequency is typed `Frequency`, K8055 writes remultiplex digital and analog
  output state into the shared output packet, K8061 output writes use the
  documented readback commands for completion, and two counter devices expose
  readback for both models plus K8055 debounce/reset commands and K8061
  all-counter reset. Runtime timing-plan hooks validate sequenceable analog/PWM
  `value` endpoints and apply first/last values through the existing
  write/readback paths. Endpoint discovery, K8061 debounce, reset/safe-state
  behavior, hardware-accurate timing, and hardware validation are not recorded.
- `numanager_drivers::starlight_xpress` implements the documented Starlight
  Xpress filter-wheel protocol as a configured state model, a serial transport
  behind `os-serial`, and explicit-config or single-match autodiscovered USB
  HID input/output-report transport behind `os-hid`: one
  `filter.wheel`/`state.device`, writable
  `position`, readable `positions`, `moving`, and `last_transaction`, select
  command completion through current-filter readback where available, and a
  discovery fixture used by `discover_devices`. Real serial and HID
  construction now read filter total and current filter before registration.
  HID identity autodiscovery is available for a single enumerated SX/Starlight
  filter-wheel candidate; VID/PID cataloging and hardware validation are not
  recorded.
- `numanager_drivers::mightex_bls` implements Sirius BLS/SLC
  HID output support from reverse engineered evidence: configured and optional
  `os-hid` discovery, HID feature-report ASCII framing, per-channel
  `TriggerSink` and `Dac` capabilities, `enabled`, `current_raw`, and
  `intensity` output writes, SLC `NORMAL` and raw strobe/trigger setup
  properties, BLS raw trigger/follow setup properties, volatile SLC readbacks,
  named hub diagnostic helpers, telemetry for command/reply/outcome fields,
  and runtime timing-plan hooks that validate only sequenceable `enabled`,
  `current_raw`, and `intensity` endpoints and apply first/last values through
  the same HID output path. Raw trigger/strobe profile timing,
  calibrated units, safe ranges, fault states, and hardware timing have no
  recorded trace-backed validation.
- `numanager_drivers::spectral_lmm5` implements spec-backed
  Spectral LMM5 RS-232 startup-readback support from the public software manual.
  It exposes one
  light-engine hub plus configured laser-line devices, typed wavelength
  metadata/readback, typed percent transmission through `Dac`, shutter
  enable/disable through `TriggerSink` and the shared shutter mask,
  trigger-in/trigger-out enable properties, configured discovery used by
  `discover_devices`, and optional configured real serial construction behind
  `os-serial` that reads shutter status and wavelengths before registering the
  driver. Runtime timing-plan arm/start/stop hooks validate sequenceable
  per-line `enabled` and `transmission` endpoints and apply first/last
  endpoints through the same shutter-mask and transmission command paths.
  USB/HID transport, full trigger-profile timing, error/fault semantics, and
  low-output hardware validation are not recorded.
- `numanager_drivers::genicam` provides the first GenICam node-map execution model:
  SDK-free parsing for a focused XML node-map subset, `Integer`/`IntReg`,
  `Float`/`FloatReg`, `Boolean`, `Enumeration`, `String`/`StringReg`,
  `IntSwissKnife`, `SwissKnife`, `Converter`, and `Command` nodes,
  value-bearing nodes bridged to typed runtime properties, command nodes exposed
  through a `GenericCommand` capability with metadata advertisement,
  access-mode handling for `RO`, `WO`, `RW`, `NA`, and `NI`,
  `ImposedAccessMode` metadata and effective access enforcement,
  range/unit/enum-choice/hardware-address metadata, flat and nested
  category membership with XML category order, root category order, category
  display/visibility metadata, selector dependencies, conditional access node
  references, increment, dynamic `pMin`/`pMax`/`pInc` constraint references,
  representation, node visibility hints, `ToolTip`, `Description`, `DocuURL`,
  `PollingTime`, and streamable hints in device-level node metadata,
  fixture-backed enforcement of `pIsAvailable`, `pIsImplemented`, and
  `pIsLocked` references for node reads, writes, and command execution,
  fixture-backed numeric `Min`/`Max`/`Inc` and dynamic
  `pMin`/`pMax`/`pInc` validation for writable integer/float nodes,
  `NA`/`NI` rejection as unsupported on node reads, writes, and command
  execution, enum-entry numeric-value metadata plus per-entry
  `pIsAvailable`/`pIsImplemented` guards that reject unavailable or
  unimplemented enum selections, formula-node metadata for `Formula`,
  `FormulaTo`, `FormulaFrom`, and `pVariable` references with scoped live
  arithmetic, comparison, logical, bitwise, and shift evaluation plus `IF`,
  `MIN`, `MAX`, `ABS`, `FLOOR`, `CEIL`, `ROUND`, `TRUNC`, `MOD`, `POW`,
  `SQRT`, `LOG`, `LOG2`, `LOG10`, `EXP`, trig functions, `ATAN2`, and `SGN`
  function calls for formula-node readback, writable `Converter` nodes that use
  `FormulaFrom` to route public-unit writes into referenced transport-unit
  nodes, `Port` and raw
  `Register`/`MaskedIntReg`/`StructReg` metadata, node `pPort`/`pAddress`,
  length, and endian hints for future transport binding, fixture-backed
  register byte storage for simple `Integer`, `Float`, `Boolean`, and
  fixed-length `StringReg` nodes with direct register hints, signed/unsigned
  integer register encoding and decoding from `Sign` metadata, fixture-backed dynamic register locations through
  `pAddress` and `pLength` references, fixture-backed `MaskedIntReg`
  extraction and read/modify/write merging for `Bit` or `LSB`/`MSB` fields,
  fixture-backed `StructReg` field locations through `pStructReg` and `Offset`,
  simple `pValue` aliases that route reads/writes through referenced value
  nodes, `pValueCopy` startup copies that initialize from referenced nodes and
  then remain independent, enum `pValue` backing nodes that translate public
  enum symbols to numeric values stored in integer nodes/registers,
  command-node `CommandValue` metadata with fixture-backed writes to addressed
  command registers or command `pValue` backing nodes on invocation,
  cache/invalidation metadata with volatile invalidated
  nodes and `PropertyChanged` refresh events after writes to invalidators,
  GenICam event-node metadata for `EventID`, `pEventTimestamp`, and
  `pEventNotification` plus fixture event-channel emission through runtime
  `Telemetry` events,
  configured discovery for camera-like node maps, fixture `CameraCapture` and
  `CameraStream` capabilities that emit frame handles through the runtime
  ring-buffer path using GenICam width/height/exposure/pixel-format nodes, with
  stream completions carrying width/height/pixel-format summaries and fixture
  frame metadata carrying node-derived chunk frame ids, hardware timestamps,
  payload size, gain, frame rate, line time, and streamable-node readbacks,
  `RawRegisterAccess` capability for fixture register reads/writes by
  XML-derived node, direct register key, or address/port with byte payloads and
  decoded typed values for node targets,
  `TriggerSink` and `TriggerSource` capabilities when acquisition start/stop
  command nodes are present, accepting only `None` or typed
  `CapabilityRequest::Trigger` requests and using `AcquisitionStart`/
  `AcquisitionStop` command-node execution for enable/disable/pulse semantics,
  coalesced node state-set submission, runtime timing-plan arm/start/stop hooks
  that validate sequenceable XML-derived acquisition nodes and apply
  first/last sequence endpoints through the normal node write path, and
  command-node invocation through the normal driver token/completion path.
- `numanager_drivers::platform_camera` provides the first SDK-free OS/platform
  camera fallback support: simulated two-stage discovery for a V4L2/GStreamer/
  DirectShow-style backend, config-backed/platform camera discovery
  with typed width/height/exposure/gain/pixel-format/frame-interval keys,
  configured-source provenance metadata, backend provenance metadata, one camera
  device, typed `Value::TimeInterval` exposure and frame-interval properties,
  gain/pixel-format controls, read-only active-format and supported-format
  properties, backend-specific fixture format inventories for V4L2, GStreamer,
  DirectShow, and local fixture paths, `CameraCapture`, `CameraStream`,
  `TriggerSink`, and `TriggerSource` capabilities with direct fixture
  trigger command/telemetry handling, schema-validated timing plans that apply
  exposure/gain/pixel-format/frame-interval endpoints, and frame-ready
  completion through the shared runtime ring-buffer API.
- `numanager_drivers::gige_vision` provides the first SDK-free GigE Vision
  control/stream surface: fixture-backed GVCP discovery/read-register/
  write-register packet builders, opt-in configured UDP GVCP raw-register
  reads/writes with ACK command/status/request-id/read-payload validation,
  separate control and stream resources, typed
  camera properties for width/height/exposure/gain/pixel-format/packet-size/
  inter-packet-delay/stream-port/hardware timestamps, config-backed
  discovery with typed camera/control keys, `CameraCapture`, `CameraStream`,
  `TriggerSink`, `TriggerSource`, and `RawRegisterAccess`
  capabilities, direct `TriggerSink`/`TriggerSource` execution through typed
  `CapabilityRequest::Trigger` requests mapped to GVCP
  `AcquisitionStart`/`AcquisitionStop` fixture writes with packet telemetry,
  `RawRegisterAccess` execution through `GenericCommandRequest` read/write
  requests by address or a standard GenICam/SFNC node-name subset
  (`Width`, `Height`, payload size, timestamp, acquisition start/stop, and
  device-mode nodes) that return GVCP packet bytes, resolved node/address, and
  fixture register values,
  GVSP-style frame metadata with chunk frame ids and hardware timestamps,
  fixture-level GVSP block reassembly with missing-packet detection and
  resend-fill handling, runtime frame handles, fixed-capacity ring-buffer behavior,
  dropped-frame telemetry, and timing-plan hooks that validate and apply
  first/last width, height, exposure, gain, and pixel-format endpoints through
  the fixture camera control path.
- `numanager_drivers::usb3_vision` provides the first SDK-free USB3 Vision
  control/stream surface: fixture-backed U3V memory-read/memory-write packet
  builders for GenICam register access over USB control transfers, opt-in
  configured USB device open/interface-claim identity metadata, separate
  control, stream, and event resources, typed camera properties for
  width/height/exposure/gain/pixel-format/transfer-size/transfer-queue-depth/
  stream-endpoint/hardware timestamps, config-backed discovery with
  typed camera/control keys, `CameraCapture`, `CameraStream`,
  `TriggerSink`, `TriggerSource`, and `RawRegisterAccess` capabilities,
  direct `TriggerSink`/`TriggerSource` execution through typed
  `CapabilityRequest::Trigger` requests mapped to U3V
  `AcquisitionStart`/`AcquisitionStop` fixture memory writes with control packet
  telemetry,
  `RawRegisterAccess` execution through `GenericCommandRequest` read/write
  requests by address or a standard GenICam/SFNC node-name subset
  (`Width`, `Height`, payload size, timestamp, acquisition start/stop,
  manifest, and device-capability nodes) that return U3V control packet bytes,
  resolved node/address, and fixture register data,
  U3V-style frame metadata with chunk frame ids and hardware timestamps,
  fixture-level U3V bulk-stream block reassembly with missing-transfer
  detection and fill handling, runtime frame handles, fixed-capacity ring-buffer
  behavior, dropped-frame telemetry, and timing-plan hooks that validate and
  apply first/last width, height, exposure, gain, and pixel-format endpoints
  through the fixture camera control path.

Remaining:

- Hardware validation of interleaved multi-request TCP pipelining against real
  Modbus TCP devices and hardware validation against real Modbus RTU devices.
  The fixture now exercises out-of-order TCP response correlation, but that is
  not a substitute for real-device validation.
- Device-specific Modbus maps and hardware-validated profile variants for real
  microscope-adjacent equipment such as named incubators, environmental
  controllers, pressure/flow controllers, shutters, IO modules, and safety
  interlocks. The current reusable profile set now includes a stage-top
  incubation chamber fixture, but named manufacturer maps and hardware
  validation remain.
- Full GenICam XML coverage, including real transport-backed conditional
  access coherency, register value extraction through real transport ports,
  complete SwissKnife/converter expression-language coverage beyond the current
  arithmetic/function subset, and real transport-backed cache coherency.
- GigE Vision / USB3 Vision real transport binding, including broadcast
  discovery, typed live GVCP camera-control properties, U3V memory control
  transfer execution,
  hardware-backed GVSP
  packet-resend/block handling beyond the current fixture reassembler, USB
  descriptor probing, active USB3 Vision discovery,
  hardware-backed U3V bulk-stream scheduling/reassembly beyond the current
  fixture reassembler, typed trigger request handling with raw generic maps
  confined to the explicit `RawRegisterAccess` capability, real GenICam chunk
  extraction/event-channel support, parsed model-specific XML binding beyond
  the current node-name bridge,
  Aravis or Rust-native transport integration, and hardware validation with the
  high-throughput camera stream ring-buffer API.
- Real platform-camera bindings for V4L2, GStreamer, DirectShow, or equivalent
  OS camera stacks; current support is a fixture that fixes the runtime-facing
  API, format-negotiation metadata contract, typed capture/stream/trigger
  request contract, and frame delivery behavior before adding backend-specific
  capture code.
- Public property naming is now a cross-driver contract: descriptors, examples,
  and device pages should advertise `snake_case` keys without unit suffixes when
  the value/schema carries the unit, and public string choices should use
  canonical Rust-style names such as `Mono8`, `Raw8`, `Rgb8`, and `Native`.
  Public physical quantities should use typed `Value` variants rather than
  naked scalars where units matter, including `TimeInterval`, `Frequency`,
  `Decibel`, `PixelCount`, and `Ratio`.
  Legacy/native spellings may remain as compatibility aliases or protocol
  metadata, but generic workflows should not probe names such as `ExposureTime`,
  `PixelFormat`, `exposure_s`, or `MONO8`.
- Configured camera discovery fixtures use `Value::ByteCount` for byte-sized
  stream properties such as `packet_size` and `transfer_size`. Scalar byte
  inputs remain driver-side legacy aliases only, not the public example path.
- Composite camera identity/readback maps should still use typed quantities for
  unit-bearing fields; for example Toupcam `usb_identity.sensor_width` and
  `usb_identity.sensor_height` are `PixelCount` values, not scalar integers.
- Camera capture and stream completion maps should include typed `PixelCount`
  `width`/`height` fields when frame geometry is known. Backends may still
  expose raw stream/frame handles as numeric API handles, but dimensions should
  not be naked integers. `CameraStreamStarted::from_completion` parses those
  fields so examples and GUI/debug tooling can report concrete stream geometry
  instead of only map-key presence.
- Prior public descriptor/config naming now uses canonical typed keys such as
  `x_travel`, `step_size_xy`, `x`, `nano_z`, and `lumen_delay`, while legacy
  `_um`/`_ms` scalar config aliases remain accepted for existing fixtures and
  legacy descriptor metadata is explicitly labeled as such. Continue this
  audit pattern for any remaining public metadata that exposes units in names
  where a typed `Value` already carries the unit.
- Marzhauser public descriptor/config naming now follows the same pattern:
  typed `Position` config keys such as `x_travel`, `pitch_x`, and
  `step_size_x` metadata are canonical, while legacy `_um`/`_mm` scalar config
  aliases and legacy metadata keys are explicitly labeled for compatibility.
- ESP32 descriptor metadata now advertises typed `Position` travel ranges as
  `x_travel`, `y_travel`, and `z_travel`; the former `_um` metadata names are
  retained only as explicitly labeled legacy entries.
- OpenUC2 descriptor metadata now uses the same canonical typed `Position`
  travel keys, with former `_um` metadata names retained only as explicitly
  labeled legacy entries.
- ASI MS-2000 and Tiger configured discovery now accept typed `Position`
  `x_travel`, `y_travel`, and `z_travel` keys, while legacy `_um` scalar
  aliases remain accepted; descriptor metadata advertises canonical travel keys
  and keeps old names only as explicitly labeled legacy entries.
- SutterStage configured discovery now accepts typed `Position` keys for
  travel ranges and `step_size`, while legacy `_um` aliases remain accepted;
  descriptor metadata uses canonical `step_size` and keeps the old name only as
  an explicitly labeled legacy entry.
- Zaber configured discovery now accepts typed `Position`, `Velocity`, and
  `Acceleration` keys for travel, microstep size, initial position, velocity,
  and acceleration. Discovery and descriptor metadata advertise canonical
  physical-quantity keys and retain old `_um`/`_um_s`/`_um_s2` names only as
  explicitly labeled legacy entries.
- Sutter MP-285 configured discovery now accepts typed `Position` `travel`;
  the former scalar `travel_um` config remains accepted as a legacy alias.
  Descriptor metadata advertises canonical typed `travel` and `microstep_size`
  keys, with old unit-suffixed names retained only as explicitly labeled
  legacy entries. Native `velocity_microsteps_per_s` remains a protocol
  register value rather than a public physical unit.
- PI GCS configured discovery now accepts typed `Position` `x_travel`,
  `y_travel`, and `z_travel` keys, with the former scalar `_um` config aliases
  retained for fixtures. Descriptor metadata advertises canonical typed travel
  and `default_unit_size` keys; old unit-suffixed names are retained only as
  explicitly labeled legacy metadata.
- Thorlabs APT descriptor metadata now advertises canonical typed `travel` and
  `encoder_step_size` keys, retaining old unit-suffixed names only as
  explicitly labeled legacy metadata. The current APT implementation has
  configured discovery and optional `os-serial` real-device construction through
  `ThorlabsAptDiscovery::from_config`; automatic USB enumeration remains part
  of the hardware-validation/discovery work.
- Cobolt, Coherent OBIS, and Omicron configured discovery now prefer typed
  `Wavelength`, `OpticalPower`, `ElectricCurrent`, `Temperature`,
  `TimeInterval`, and `Ratio` config keys where applicable, while keeping the
  former scalar `_nm`/`_mw`/`_ma`/`_c`/`_percent` aliases for older configs.
  Device pages list canonical typed config keys separately from legacy aliases.

### Milestone 5: Reverse-Engineered Fallback Candidates

- `numanager_drivers::toupcam` now covers the first Toupcam/AmScope property
  gap-analysis support beyond exposure: typed exposure, gain, pixel-format,
  trigger-mode, ROI width/height, binning, black-level, white-balance, and
  sensor-temperature properties; read-only USB identity,
  supported-pixel-format, and feature-summary properties carrying public vendor
  IDs, endpoint, sensor geometry, implemented controls, evidence provenance,
  and hardware-validation work; exposure/gain register-sequence provenance;
  direct typed `TriggerSink` invocation for trigger-mode setup and software
  pulse telemetry, with trigger-mode string changes routed through the typed
  `trigger_mode` property rather than generic command maps;
  `RawRegisterAccess` execution through `GenericCommandRequest` read/write
  requests against the clean-room exposure/gain USB-control register surface;
  schema-validated timing-plan hooks that apply first/last exposure, gain, and
  pixel-format endpoints through the same typed property/register paths;
  capture/stream frame metadata carrying the configured camera controls and
  feature-summary provenance; and runtime frame-ring integration for capture
  and high-throughput stream paths. `camera_acquisition` exercises the public
  camera workflow through typed setup, direct trigger-mode/pulse invocation,
  timing arm/start/stop endpoint application, capture completion, frame
  metadata, and typed readback; raw register access remains a documented driver
  capability for hardware bring-up, not a user example workflow.
  Remaining work is hardware-backed control execution, model-specific feature
  discovery, USB control/bulk transport binding, and comparison against public
  headers/examples before any fallback-evidence-informed work.
- The default-off Slint software-test GUI now uses the same typed
  `Value::TimeInterval` exposure property and frame metadata contract as the
  Toupcam-style camera provider. Its local test rig exposes multiple camera
  sources, multiple XY stages, and a safety-capable illumination device so the
  camera-source selector, pan-stage selector, histogram, editable typed
  properties, normalized `SafetySummary` readback, and mouse-panning workflow
  are exercised through public runtime/device/property APIs.
- KURIOS CLI implementation is present in `numanager_drivers::thorlabs_kurios`;
  configured discovery and optional `os-serial` real-device construction are
  available through `KuriosDiscovery::from_config` using typed `Wavelength`
  range keys. Remaining work is real model probing and hardware validation of
  wavelength ranges, bandwidth modes, calibration/status/error replies,
  warmup/shutter behavior, and trigger timing.
- Thorlabs DC2200 has a SCPI-style command set plus an explicit-config USBTMC
  bulk transport that runs the configured startup readback behind `os-usb` in
  `numanager_drivers::thorlabs_dc`; endpoint autodiscovery, a VISA backend, and
  broader command coverage remain without a public surface where
  command/endpoint evidence is absent. Hardware error/status validation is
  tracked separately from the implemented support.
- Omicron serial feature-completeness has advanced through decoded
  operating/fault bitfields, analog/digital/APC modulation flags, and
  interlock status; remaining work is manufacturer-command-list validation for
  real `?GSI`/`?GFw` parsing, model-specific operating-mode bitfields, power
  calibration, reset timing, and fault transitions on hardware.
- Decide whether any remaining proprietary-runtime-only controller justifies trace-based work.

## Explicit Non-Goals For Now

- No closed SDK bindings in default builds.
- No proprietary-binary-derived implementation.
- No broad scientific camera SDK artifact analysis.
- No GUI dependency in core driver modules.
- No sleeps as completion semantics when hardware status is available.
