# numanager Design

`numanager` is a clean Rust microscope and hardware-control substrate inspired
by Micro-Manager, but it is not a line-by-line API clone. The central model is a
graph of hardware resources, logical devices, capabilities, and operations. A
single physical controller may expose many logical devices, and a single logical
operation may have to be remultiplexed back into one hardware command.

The project is a pure driver collection plus a small runtime. It should be able
to control microscopes, plate readers, fluidics, incubators, robot arms, cameras,
spectrometers, timing hardware, and future instruments that do not fit the
classic microscope device taxonomy.

No vendor SDKs are assumed. Drivers may use open OS APIs and clean-room protocol
implementations. When an SDK is the only practical path, it should be isolated
behind the same transport/session boundary as protocol-evidence drivers, not
allowed to shape the core API.

## Goals

- Represent hardware as a DAG, not a flat device list or hub tree.
- Let hubs expose logical devices while retaining ownership of shared transports.
- Allow cross-device commands to be coalesced into one physical transaction.
- Avoid blocking UI/control threads on slow serial, USB, SDK, or motion calls.
- Provide operation handles and event streams for completion, progress, frames,
  telemetry, and faults.
- Support multiple listeners subscribing to multiple devices.
- Make device capabilities and properties inspectable at runtime.
- Combine plug-and-play discovery with explicit configuration files.
- Ship simulators for hardware protocols, whole instruments, and biological
  scenes.

## Non-Goals

- Do not preserve Micro-Manager's `CMMCore` as the primary API.
- Do not require one Rust trait per possible hardware class.
- Do not flatten complex controllers into one undifferentiated property bag.
- Do not hide real hardware coupling behind fake independent devices.
- Do not use global singletons for hub state.
- Do not spawn a process per device. Use processes only for crash isolation,
  incompatible dependencies, or hard real-time helpers.

## Core Concepts

### Resource

A resource is an exclusive thing that can perform I/O or owns physical state:
USB interface, serial port, CAN gateway, TCP socket, SDK session, FPGA timing
engine, camera stream, or simulator instance.

Resources are owned by actors called lanes. A lane serializes access to one
exclusive resource and runs blocking calls away from callers.

### Hub

A hub is a driver that owns one or more resources and can offer logical devices.
Examples:

- ASI Tiger controller offering XY, Z, wheels, TTL, and scanners.
- Spark Cyto mainboard offering plate motion, readers, gas, temperature, and
  CAN modules.
- Toupcam USB camera exposing camera, stream, trigger, and raw-register
  capabilities.

Hubs do not expose children by sharing mutable references. They expose
descriptors and accept command batches addressed to logical endpoints.

### Device

A device is a logical endpoint with identity, type tags, properties,
capabilities, events, and dependency edges. Device kinds are broad tags for UI
and orchestration, not an exhaustive trait hierarchy.

Examples:

- `camera`
- `axis.z`
- `axis.xy`
- `shutter`
- `light.source`
- `filter.wheel`
- `objective.turret`
- `autofocus`
- `timing.ttl`
- `timing.dac`
- `plate.transport`
- `environment.temperature`
- `environment.gas`
- `generic`

A device may have several tags. For example, a laser can be `light.source`,
`shutter`, `power.analog`, and `trigger.sink`.

### Capability

A capability is a typed operation surface advertised by a device or hub.
Capabilities are preferable to deep inheritance. A client asks whether a device
has `CameraCapture`, `AxisMove`, `TriggerSink`, `WaveformUpload`, or
`AutofocusSearch` and receives typed metadata.

Autofocus is a general device/capability pair, not a Squid-specific laser pin
or light-gate device. Squid may provide one concrete autofocus endpoint, but
the endpoint is an implementation of the shared autofocus model rather than a
Squid API surface. A hardware controller may offer autofocus directly, as ASI
does through CRISP or Squid does through its firmware-controlled focus gate. A
software autofocus service may offer the same `CapabilityKind::Autofocus` while
using graph edges to depend on a camera, Z stage, and optional light or laser
device. Clients select autofocus providers by `CapabilityKind::Autofocus` and
dependency metadata, then invoke the same typed `AutofocusRequest` either way
and wait on the returned operation; completion is reported by the provider from
hardware status or service telemetry.

The public autofocus contract should stay provider-neutral:

- device kind tag: `autofocus`
- capability: `CapabilityKind::Autofocus`
- request: `CapabilityRequest::Autofocus(AutofocusRequest)`
- common properties: `enabled`, `mode`, `status`, and `focus_score` when
  available
- dependencies: `UsesDevice` graph edges with roles such as `ZStage`, `Camera`,
  and `LightSource`

Provider-specific controls, such as a Squid firmware pin number or an ASI CRISP
axis parameter, belong in metadata or provider-specific diagnostic properties.
They must not define the core autofocus API.

### Property

Properties are typed, discoverable, and optionally sequenceable. They are the
universal escape hatch and the primary UI/config surface.

Properties should carry:

- stable key
- display name
- value type
- unit
- current value, if readable
- allowed range or enum values
- read/write permissions
- volatility/cache policy
- whether writes are staged, immediate, or require a commit
- hardware address metadata for protocol debugging

Properties are not enough for all control. High-value operations like image
capture, motion, trigger arming, and autofocus should also have typed
capabilities.

## Graph Model

The runtime stores a DAG of nodes and edges.

```rust
pub struct NodeId(u64);
pub struct DeviceId(NodeId);
pub struct ResourceId(NodeId);

pub enum NodeKind {
    Resource,
    Hub,
    Device,
    Service,
    Simulator,
}

pub enum EdgeKind {
    OwnsResource,
    OffersDevice,
    UsesDevice { role: Role },
    SharesClock,
    SharesTransport,
    RequiresConfig,
}

pub enum Role {
    ParentHub,
    Camera,
    ZStage,
    XYStage,
    LightSource,
    TimingSource,
    TriggerSink,
    TriggerSource,
    Autofocus,
    Environment,
    Custom(String),
}
```

Initialization is a topological walk. Resources initialize first, then hubs,
then offered devices, then composed devices such as software autofocus. Cycles
are configuration errors.

## Command Model

The public API should not be "call a method and block". It should submit
commands and return operation handles.

```rust
pub struct CommandId(u64);
pub struct OperationId(u64);

pub enum Command {
    ReadProperty { device: DeviceId, key: String },
    WriteProperty { device: DeviceId, key: String, value: Value },
    Invoke { device: DeviceId, capability: CapabilityId, request: CapabilityRequest },
    ApplyStateSet(StateSet),
    Arm(TimingPlan),
    Start(OperationId),
    Stop(OperationId),
}

pub struct OperationHandle {
    pub id: OperationId,
    pub devices: Vec<DeviceId>,
}
```

`submit(command)` returns quickly with an `OperationHandle`. Callers can poll,
wait with a timeout, cancel if supported, or subscribe to events.

The internal scheduler routes commands to lanes. If two logical device writes
belong to the same hub and can be represented by one hardware transaction, the
scheduler sends one hub transaction. This is the core remultiplexing mechanism.

## State Sets

State sets are the preferred API for coordinated changes. They represent desired
state across devices at one logical time.

```rust
pub struct StateSet {
    pub name: Option<String>,
    pub writes: Vec<StateWrite>,
    pub commit: CommitMode,
}

pub struct StateWrite {
    pub device: DeviceId,
    pub property: String,
    pub value: Value,
}

pub enum CommitMode {
    Immediate,
    PrepareThenCommit,
    HardwareTimed { at: TimePoint },
}
```

A hub receives the subset of a state set that targets its offered devices. It
can translate `x`, `y`, and `z` writes into one `MOVE X=... Y=... Z=...` command
if the hardware requires or benefits from that.

This solves the XY/Z split problem: the system may expose independent `xy` and
`z` devices, but the hub still sees a coherent multi-axis state write.

## Driver Boundary

Drivers implement three layers when possible.

```rust
pub trait Transport {
    fn send(&mut self, bytes: &[u8]) -> Result<()>;
    fn poll_recv(&mut self) -> Result<Option<Vec<u8>>>;
}

pub trait Session {
    fn submit_packet(&mut self, packet: Packet) -> Result<SessionToken>;
    fn poll(&mut self) -> Vec<SessionEvent>;
}

pub trait Driver {
    fn descriptors(&self) -> Vec<DeviceDescriptor>;
    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor>;
    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch>;
    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken>;
    fn poll(&mut self) -> Vec<DriverEvent>;
}
```

The transport owns bytes. The session owns protocol sequencing and response
matching. The driver owns logical devices and remultiplexing.

This mirrors the useful pattern in the Spark Cyto driver: whole-frame transport,
sequence-number session, then command/engine logic. It also fits Toupcam: USB
control/bulk transport, camera register/stream session, then logical camera
capabilities.

## Runtime Lanes

Every blocking or exclusive I/O resource gets a lane:

- serial bus lane
- USB control endpoint lane
- USB bulk stream lane
- camera acquisition lane
- CAN gateway lane
- SDK/session lane
- simulator lane

Lanes can be implemented with standard threads and channels first. The public
API should be async-shaped without requiring Rust `async` at the trait boundary.

```rust
pub trait Runtime {
    fn submit(&self, command: Command) -> Result<OperationHandle>;
    fn status(&self, op: OperationId) -> OperationStatus;
    fn cancel(&self, op: OperationId) -> Result<CancelResult>;
    fn subscribe(&self, filter: EventFilter) -> Subscription;
}

pub enum OperationStatus {
    Queued,
    Running { progress: Option<Progress> },
    Completed(Value),
    Failed(ErrorReport),
    Cancelled,
    TimedOut,
}
```

Drivers may use blocking calls internally. The lane makes that acceptable by
keeping unrelated resources available.

## Events and Listeners

Events are first-class, fanout, and filterable. Multiple subscribers may attach
to the same device, and one subscriber may listen to many devices.

```rust
pub enum Event {
    OperationChanged(OperationChanged),
    PropertyChanged(PropertyChanged),
    FrameReady(FrameEvent),
    Telemetry(TelemetryEvent),
    DeviceArrived(DeviceDescriptor),
    DeviceRemoved(DeviceId),
    Fault(FaultEvent),
    Log(LogEvent),
}

pub struct EventFilter {
    pub devices: DeviceSelector,
    pub kinds: Vec<EventKind>,
}
```

The runtime event bus should be separate from hardware polling. Drivers emit
events to their lane; lanes publish onto the bus; subscribers receive bounded
queues with overflow reporting.

Event remultiplexing is symmetric with command remultiplexing. A hub event such
as "axis positions changed" can be split into `xy.position` and `z.position`
property events, while a camera stream event can be delivered to image viewers,
autofocus, storage, and analysis at the same time.

## Type Safety

Use a hybrid model.

- Device kinds and capabilities are runtime descriptors.
- Property values are typed with schema validation.
- High-value capabilities have typed Rust request/response structs.
- Dynamic plugins and config files use serialized values validated against the
  same schemas.

```rust
pub enum Value {
    Bool(bool),
    I64(i64),
    F64(f64),
    String(String),
    Bytes(Vec<u8>),
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
}

pub struct PropertySchema {
    pub key: String,
    pub value_type: ValueType,
    pub unit: Option<Unit>,
    pub range: Option<Range>,
    pub enum_values: Vec<EnumValue>,
}
```

This avoids forcing every user-visible feature into Rust trait objects while
still preventing arbitrary stringly-typed writes.

## Discovery and Config

Discovery is best-effort. Config is authoritative when discovery cannot prove
identity or when a system has multiple valid topologies.

Discovery sources:

- USB VID/PID, descriptors, serial numbers
- serial port probes
- network discovery
- hub `scan` commands
- known simulator descriptors
- saved config hints

Config should include:

- stable labels
- expected resources
- dependency edges
- selected driver
- protocol parameters
- property defaults
- calibration files
- remultiplexing groups, if automatic inference is insufficient

Suggested format: `TOML` for hand-edited configs, with optional generated lock
files containing discovered serial numbers and firmware details.

## Timing and Synchronization

Timing is modeled as capabilities and plans, not camera-only properties.

Core timing concepts:

- trigger source
- trigger sink
- clock
- edge/polarity
- delay
- gate
- exposure window
- waveform
- sequence table
- arm/start/stop ordering

```rust
pub struct TimingPlan {
    pub participants: Vec<DeviceId>,
    pub routes: Vec<TriggerRoute>,
    pub sequences: Vec<DeviceSequence>,
    pub arm_order: Vec<DeviceId>,
    pub start: StartCondition,
    pub stop: StopCondition,
}
```

The runtime validates and executes primitive timing operations. Higher-level
experiment software decides whether a plan is a z-stack, time series, FRAP,
confocal raster, plate scan, or autofocus loop.

## First Driver Targets

### Toupcam Camera

Source reference: `/home/mahogny/github/claude/opengel/src/camera`.

Initial device:

- one hub-like USB camera driver owning the claimed interface
- one `camera` device
- properties: exposure, gain, resolution, pixel format, trigger mode
- capabilities: `CameraCapture`, `CameraStream`, `TriggerSink`,
  `RawRegisterAccess`
- separate control lane and bulk stream lane

Important implementation details from the existing driver:

- device discovery by Toupcam/ToupTek/Cypress vendor IDs
- captured initialization replay
- `0x0b` obfuscated register writes for exposure/gain
- queued bulk-IN reads for frame streaming
- RAW8 frame path with later debayer support

The current `capture()` call blocks while reading a full frame. In `numanager`,
capture should submit an operation and report `FrameReady` or failure events.

### Spark Cyto

Source reference: `/home/mahogny/github/claude/sparkcyto`.

Initial shape:

- Spark mainboard hub owns the TDCL transport/session
- the hub offers plate transport, absorbance, fluorescence, luminescence,
  temperature, gas, imaging head, objective changer, LEDs, and camera binding
  descriptors when discovered
- TDCL command/data channels are resources
- CAN modules are logical children behind the mainboard hub
- IDS/uEye camera is a separate physical camera resource but depends on CELL or
  FIM imaging head metadata

Important implementation details from the existing driver:

- whole-TDCL-frame transport
- `Busy -> Ready/Error` session state machine
- sequence-number response matching
- module topology from `ScanModules` XML and `StringDescriptor3`
- command builder for Symbio ASCII payloads
- simulator as reference firmware

Spark is a strong test case for the DAG: the camera is physically independent,
but semantically depends on the imaging module; plate motion, optics, detector,
and environment devices all share one controller/session.

## Simulators

Simulators should exist at three levels.

### Protocol Simulators

Protocol simulators emulate transport/session behavior byte-for-byte enough to
develop and debug drivers without hardware. Spark's loopback TDCL simulator is
the model.

### Instrument Simulators

Instrument simulators expose realistic device graphs: inverted microscope,
upright fluorescence scope, confocal, light sheet, plate reader, incubated
imaging system, fluidics rig.

### Biological Scene Simulators

Scene simulators produce frames or readings from synthetic biological systems:
gel lanes, fluorescent cells, focus curves, drifting samples, photobleaching,
well-plate kinetics, noisy sensors, saturated images, and bad calibrations.

The simulator API should use the same driver/runtime interfaces as hardware.

## Proposed Crate Layout

```text
crates/
  numanager-core/        graph, descriptors, schemas, enum-backed capabilities,
                         commands, events,
                         runtime lanes, scheduler, config, discovery locks
  numanager-sim/         reusable simulator primitives
  drivers/
    toupcam/
    spark-cyto/
    sim-microscope/
    sim-plate-reader/
```

Keep driver crates small and protocol-oriented. Shared abstractions, scheduling,
and configuration live in `numanager-core` so driver authors depend on one API
crate.

## Implementation Order

1. Create `numanager-core` descriptor, property, command, event, and graph
   types.
2. Create a simple thread/channel runtime with operation handles and event
   subscriptions.
3. Port the Toupcam camera as a driver with one camera device and two lanes.
4. Port Spark TDCL transport/session and expose the mainboard hub descriptors.
5. Add state-set batching and hub remultiplexing.
6. Add config load/save and discovery locking.
7. Add Spark instrument simulator and generic microscope simulator.
8. Add timing plan primitives.
9. Add composed software autofocus using camera + z-stage dependencies.
10. Add more hardware families.

Tests are intentionally not included in this design pass.
