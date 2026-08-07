use crate::config::DiscoveryEntry;
use crate::*;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub trait Runtime {
    fn submit(&self, command: Command) -> Result<OperationHandle>;
    fn status(&self, op: OperationId) -> OperationStatus;
    fn wait(&self, op: OperationId, timeout: Duration) -> Result<OperationStatus>;
    fn frame(&self, handle: FrameHandle) -> Result<Option<Frame>>;
    fn stream_status(&self, stream: StreamId) -> Result<Option<FrameStreamStatus>>;
    fn cancel(&self, op: OperationId) -> Result<CancelResult>;
    fn subscribe(&self, filter: EventFilter) -> Subscription;

    fn wait_completed(&self, op: OperationId, timeout: Duration) -> Result<Value> {
        self.wait(op, timeout)?.into_completed()
    }

    fn execute(&self, command: Command, timeout: Duration) -> Result<Value> {
        let op = self.submit(command)?;
        self.wait_completed(op.id, timeout)
    }
}

pub struct Subscription {
    rx: Receiver<Event>,
}

impl Subscription {
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Event> {
        self.rx.recv_timeout(timeout).ok()
    }

    pub fn try_recv(&self) -> Option<Event> {
        self.rx.try_recv().ok()
    }
}

pub trait DriverDiscovery {
    fn detect(&mut self) -> Result<Vec<DriverCandidate>>;
}

struct DriverIdAllocator {
    next: u64,
}

impl DriverIdAllocator {
    pub fn new(next_id: DriverId) -> Self {
        Self { next: next_id.0 }
    }

    pub fn next_id(&mut self) -> DriverId {
        let id = DriverId(self.next);
        self.next += 1;
        id
    }

    pub fn reserve(&mut self, width: u64) -> DriverId {
        let id = DriverId(self.next);
        self.next += width;
        id
    }
}

pub struct DiscoveryRegistry {
    discoveries: Vec<Box<dyn DriverDiscovery>>,
    ids: DriverIdAllocator,
}

impl DiscoveryRegistry {
    pub fn new() -> Self {
        Self::with_next_id(DriverId(1))
    }

    pub fn with_next_id(next_id: DriverId) -> Self {
        Self {
            discoveries: Vec::new(),
            ids: DriverIdAllocator::new(next_id),
        }
    }

    pub fn register<D>(&mut self, discovery: D)
    where
        D: DriverDiscovery + 'static,
    {
        self.discoveries.push(Box::new(discovery));
    }

    pub fn register_boxed(&mut self, discovery: Box<dyn DriverDiscovery>) {
        self.discoveries.push(discovery);
    }

    pub fn register_factory<D, F>(&mut self, factory: F)
    where
        D: DriverDiscovery + 'static,
        F: FnOnce(DriverId) -> D,
    {
        let id = self.next_driver_id();
        self.register(factory(id));
    }

    pub fn register_factory_block<D, F>(&mut self, width: u64, factory: F)
    where
        D: DriverDiscovery + 'static,
        F: FnOnce(DriverId) -> D,
    {
        let id = self.reserve_driver_ids(width);
        self.register(factory(id));
    }

    pub fn register_factory_result<D, F>(&mut self, factory: F) -> Result<()>
    where
        D: DriverDiscovery + 'static,
        F: FnOnce(DriverId) -> Result<D>,
    {
        let id = self.next_driver_id();
        self.register(factory(id)?);
        Ok(())
    }

    pub fn register_factory_block_result<D, F>(&mut self, width: u64, factory: F) -> Result<()>
    where
        D: DriverDiscovery + 'static,
        F: FnOnce(DriverId) -> Result<D>,
    {
        let id = self.reserve_driver_ids(width);
        self.register(factory(id)?);
        Ok(())
    }

    pub fn register_boxed_factory_result<F>(&mut self, factory: F) -> Result<()>
    where
        F: FnOnce(DriverId) -> Result<Box<dyn DriverDiscovery>>,
    {
        let id = self.next_driver_id();
        self.register_boxed(factory(id)?);
        Ok(())
    }

    pub fn next_driver_id(&mut self) -> DriverId {
        self.ids.next_id()
    }

    pub fn reserve_driver_ids(&mut self, width: u64) -> DriverId {
        self.ids.reserve(width)
    }

    pub fn len(&self) -> usize {
        self.discoveries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.discoveries.is_empty()
    }

    /// Every candidate the registered discoveries can find.
    ///
    /// One discovery failing says nothing about the others: drivers scan
    /// independent buses, and a single unreachable or unrecognized device must
    /// not cost the user every *other* instrument on the machine. So a failure
    /// is remembered rather than propagated, and only reported when the sweep
    /// found nothing at all — which keeps a real, total failure (no USB access,
    /// no permissions) loud, while a bad apple stays local.
    pub fn detect_all(&mut self) -> Result<Vec<DriverCandidate>> {
        let mut candidates = Vec::new();
        let mut failure = None;
        for discovery in &mut self.discoveries {
            match discovery.detect() {
                Ok(found) => candidates.extend(found),
                // The first failure is the one reported: later ones are usually
                // the same cause seen again, and the first is nearest to it.
                Err(error) => failure = failure.or(Some(error)),
            }
        }
        match failure {
            Some(error) if candidates.is_empty() => Err(error),
            _ => Ok(candidates),
        }
    }
}

impl Default for DiscoveryRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct DriverCandidate {
    id: DriverId,
    label: String,
    devices: Vec<DeviceDescriptor>,
    resources: Vec<ResourceDescriptor>,
    driver: Box<dyn Driver>,
}

impl DriverCandidate {
    pub fn from_driver(label: impl Into<String>, driver: Box<dyn Driver>) -> Self {
        let id = driver.id();
        let devices = driver.descriptors();
        let resources = driver.resources();
        Self {
            id,
            label: label.into(),
            devices,
            resources,
            driver,
        }
    }

    pub fn id(&self) -> DriverId {
        self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn devices(&self) -> &[DeviceDescriptor] {
        &self.devices
    }

    pub fn resources(&self) -> &[ResourceDescriptor] {
        &self.resources
    }

    pub fn persistent_id(&self) -> String {
        format!("driver:{}:{}", self.id.0, self.label)
    }

    pub fn aliases(&self) -> Vec<String> {
        self.devices
            .iter()
            .map(|device| device.label.clone())
            .collect()
    }

    pub fn serial(&self) -> Option<String> {
        self.devices.iter().find_map(|device| device.serial.clone())
    }

    pub fn firmware(&self) -> Option<String> {
        self.devices
            .iter()
            .find_map(|device| match device.metadata.get("firmware_version") {
                Some(Value::String(version)) => Some(version.clone()),
                _ => None,
            })
    }

    pub fn discovery_metadata(&self) -> BTreeMap<String, Value> {
        let mut metadata = BTreeMap::from([
            ("device_count".into(), Value::I64(self.devices.len() as i64)),
            (
                "resource_count".into(),
                Value::I64(self.resources.len() as i64),
            ),
        ]);
        if let Some(vendor) = self.devices.iter().find_map(|device| device.vendor.clone()) {
            metadata.insert("vendor".into(), Value::String(vendor));
        }
        if let Some(model) = self.devices.iter().find_map(|device| device.model.clone()) {
            metadata.insert("model".into(), Value::String(model));
        }
        for key in [
            "family",
            "product_string",
            "vendor_id",
            "product_id",
            "channel_count",
            "module_type",
            "support_level",
        ] {
            if let Some(value) = self.discovery_metadata_value(key) {
                metadata.insert(key.into(), value);
            }
        }
        metadata
    }

    fn discovery_metadata_value(&self, key: &str) -> Option<Value> {
        self.devices
            .iter()
            .find_map(|device| device.metadata.get(key).cloned())
            .or_else(|| {
                self.resources
                    .iter()
                    .find_map(|resource| resource.metadata.get(key).cloned())
            })
    }

    pub fn to_discovery_entry(&self) -> DiscoveryEntry {
        DiscoveryEntry {
            persistent_id: Some(self.persistent_id()),
            label: self.label.clone(),
            aliases: self.aliases(),
            driver: self.id,
            serial: self.serial(),
            firmware: self.firmware(),
            metadata: self.discovery_metadata(),
        }
    }

    pub fn into_driver(self) -> Box<dyn Driver> {
        self.driver
    }
}

#[derive(Clone)]
struct EventBus {
    subscribers: Arc<Mutex<Vec<Subscriber>>>,
}

struct Subscriber {
    filter: EventFilter,
    tx: Sender<Event>,
}

impl EventBus {
    fn new() -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn subscribe(&self, filter: EventFilter) -> Subscription {
        let (tx, rx) = mpsc::channel();
        self.subscribers
            .lock()
            .expect("event subscribers poisoned")
            .push(Subscriber { filter, tx });
        Subscription { rx }
    }

    fn publish(&self, event: Event) {
        let mut subscribers = self.subscribers.lock().expect("event subscribers poisoned");
        subscribers.retain(|sub| {
            if sub.filter.matches(&event) {
                sub.tx.send(event.clone()).is_ok()
            } else {
                true
            }
        });
    }
}

enum LaneCommand {
    Run {
        operation: OperationId,
        batch: CommandBatch,
    },
    RunFragment {
        batch: CommandBatch,
        reply: Sender<Result<Value>>,
    },
    PrepareTimingPlan {
        plan: TimingPlan,
        command_id: CommandId,
        reply: Sender<Result<PreparedBatch>>,
    },
    PrepareTimingTransition {
        armed: ArmedTimingPlan,
        action: TimingTransitionAction,
        command_id: CommandId,
        reply: Sender<Result<PreparedBatch>>,
    },
    Cancel {
        operation: OperationId,
        reply: Sender<CancelResult>,
    },
    Shutdown,
}

#[derive(Clone)]
struct LaneHandle {
    tx: Sender<LaneCommand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimingTransitionAction {
    Start,
    Stop,
}

impl TimingTransitionAction {
    fn name(self) -> &'static str {
        match self {
            TimingTransitionAction::Start => "start",
            TimingTransitionAction::Stop => "stop",
        }
    }
}

struct OperationTable {
    statuses: Mutex<HashMap<OperationId, OperationStatus>>,
    devices: Mutex<HashMap<OperationId, Vec<DeviceId>>>,
    changed: Condvar,
}

impl OperationTable {
    fn new() -> Self {
        Self {
            statuses: Mutex::new(HashMap::new()),
            devices: Mutex::new(HashMap::new()),
            changed: Condvar::new(),
        }
    }

    fn register(&self, operation: OperationId, devices: Vec<DeviceId>, status: OperationStatus) {
        self.devices
            .lock()
            .expect("operation devices poisoned")
            .insert(operation, devices);
        self.insert(operation, status);
    }

    fn insert(&self, operation: OperationId, status: OperationStatus) {
        self.statuses
            .lock()
            .expect("operations poisoned")
            .insert(operation, status);
        self.changed.notify_all();
    }

    fn devices(&self, operation: OperationId) -> Vec<DeviceId> {
        self.devices
            .lock()
            .expect("operation devices poisoned")
            .get(&operation)
            .cloned()
            .unwrap_or_default()
    }

    fn get(&self, operation: OperationId) -> OperationStatus {
        self.statuses
            .lock()
            .expect("operations poisoned")
            .get(&operation)
            .cloned()
            .unwrap_or(OperationStatus::Unknown)
    }

    fn wait(&self, operation: OperationId, timeout: Duration) -> Result<OperationStatus> {
        let deadline = Instant::now() + timeout;
        let mut statuses = self.statuses.lock().expect("operations poisoned");
        loop {
            let status = statuses
                .get(&operation)
                .cloned()
                .unwrap_or(OperationStatus::Unknown);
            match status {
                OperationStatus::Queued | OperationStatus::Running { .. } => {
                    let now = Instant::now();
                    if now >= deadline {
                        return Err(Error::new(ErrorCode::Timeout, "operation did not finish"));
                    }
                    let remaining = deadline.saturating_duration_since(now);
                    let (next_statuses, wait) = self
                        .changed
                        .wait_timeout(statuses, remaining)
                        .expect("operations poisoned");
                    statuses = next_statuses;
                    if wait.timed_out() {
                        return Err(Error::new(ErrorCode::Timeout, "operation did not finish"));
                    }
                }
                OperationStatus::Unknown => {
                    return Err(Error::new(ErrorCode::InvalidCommand, "unknown operation"));
                }
                terminal => return Ok(terminal),
            }
        }
    }
}

struct FrameRing {
    spec: FrameBufferSpec,
    frames: VecDeque<Frame>,
    dropped_frames: u64,
}

struct FrameStore {
    streams: Mutex<HashMap<StreamId, FrameRing>>,
}

impl FrameStore {
    fn new() -> Self {
        Self {
            streams: Mutex::new(HashMap::new()),
        }
    }

    fn insert(&self, frame: Frame) -> Result<Vec<Event>> {
        let mut streams = self.streams.lock().expect("frame store poisoned");
        let ring = streams
            .entry(frame.handle.stream)
            .or_insert_with(|| FrameRing {
                spec: frame.buffer.clone(),
                frames: VecDeque::with_capacity(frame.buffer.capacity_frames.max(1)),
                dropped_frames: 0,
            });
        ring.spec = frame.buffer.clone();
        let capacity = ring.spec.capacity_frames.max(1);
        let mut dropped_this_insert = false;
        if ring.frames.len() >= capacity {
            match ring.spec.overflow {
                OverflowPolicy::DropOldest => {
                    ring.frames.pop_front();
                    ring.dropped_frames += 1;
                    dropped_this_insert = true;
                }
                OverflowPolicy::DropNewest => {
                    ring.dropped_frames += 1;
                    dropped_this_insert = true;
                }
                OverflowPolicy::Error => {
                    return Err(Error::new(ErrorCode::Driver, "frame ring buffer is full"));
                }
            }
        }

        let device = frame.device;
        let handle = frame.handle;
        let width = frame.width;
        let height = frame.height;
        let pixel_format = frame.pixel_format.clone();
        let mut metadata = frame.metadata.clone();
        if !matches!(ring.spec.overflow, OverflowPolicy::DropNewest) || !dropped_this_insert {
            ring.frames.push_back(frame);
        }

        metadata.insert("ring_capacity".into(), Value::I64(capacity as i64));
        metadata.insert("ring_depth".into(), Value::I64(ring.frames.len() as i64));
        metadata.insert(
            "dropped_frames".into(),
            Value::I64(ring.dropped_frames as i64),
        );
        metadata.insert(
            "overflow_policy".into(),
            Value::String(frame_overflow_policy_name(&ring.spec.overflow).into()),
        );

        let mut events = vec![Event::FrameReady(FrameEvent {
            device,
            handle,
            width,
            height,
            pixel_format,
            metadata,
        })];
        if dropped_this_insert {
            events.push(Event::Telemetry(TelemetryEvent {
                device,
                values: BTreeMap::from([
                    ("stream".into(), Value::I64(handle.stream.0 as i64)),
                    (
                        "dropped_frames".into(),
                        Value::I64(ring.dropped_frames as i64),
                    ),
                    ("ring_capacity".into(), Value::I64(capacity as i64)),
                    ("ring_depth".into(), Value::I64(ring.frames.len() as i64)),
                    (
                        "overflow_policy".into(),
                        Value::String(frame_overflow_policy_name(&ring.spec.overflow).into()),
                    ),
                ]),
            }));
        }
        Ok(events)
    }

    fn get(&self, handle: FrameHandle) -> Option<Frame> {
        self.streams
            .lock()
            .expect("frame store poisoned")
            .get(&handle.stream)
            .and_then(|ring| {
                ring.frames
                    .iter()
                    .find(|frame| frame.handle == handle)
                    .cloned()
            })
    }

    fn status(&self, stream: StreamId) -> Option<FrameStreamStatus> {
        self.streams
            .lock()
            .expect("frame store poisoned")
            .get(&stream)
            .map(|ring| FrameStreamStatus {
                stream,
                buffer: ring.spec.clone(),
                retained_frames: ring.frames.iter().map(|frame| frame.handle).collect(),
                dropped_frames: ring.dropped_frames,
            })
    }

    fn remove_devices(&self, devices: &[DeviceId]) {
        let mut streams = self.streams.lock().expect("frame store poisoned");
        streams.retain(|_, ring| {
            ring.frames.retain(|frame| !devices.contains(&frame.device));
            !ring.frames.is_empty()
        });
    }
}

fn timing_plan_summary(
    state: &str,
    armed: &ArmedTimingPlan,
    transitions: &[TimingPlanTransition],
) -> Value {
    let plan = &armed.plan;
    Value::Map(BTreeMap::from([
        ("state".into(), Value::String(state.into())),
        (
            "participants".into(),
            Value::List(
                plan.participants
                    .iter()
                    .map(|device| Value::I64(device.0 .0 as i64))
                    .collect(),
            ),
        ),
        (
            "routes".into(),
            Value::List(plan.routes.iter().map(trigger_route_summary).collect()),
        ),
        (
            "sequences".into(),
            Value::List(plan.sequences.iter().map(device_sequence_summary).collect()),
        ),
        (
            "arm_order".into(),
            Value::List(
                plan.arm_order
                    .iter()
                    .map(|device| Value::I64(device.0 .0 as i64))
                    .collect(),
            ),
        ),
        ("start".into(), start_condition_summary(&plan.start)),
        ("stop".into(), stop_condition_summary(&plan.stop)),
        (
            "prepared_drivers".into(),
            Value::List(
                armed
                    .preparations
                    .iter()
                    .map(timing_plan_preparation_summary)
                    .collect(),
            ),
        ),
        (
            "transition_drivers".into(),
            Value::List(
                transitions
                    .iter()
                    .map(timing_plan_transition_summary)
                    .collect(),
            ),
        ),
    ]))
}

fn timing_plan_preparation_summary(preparation: &TimingPlanPreparation) -> Value {
    Value::Map(BTreeMap::from([
        ("driver".into(), Value::I64(preparation.driver.0 as i64)),
        (
            "physical_transactions".into(),
            Value::I64(preparation.physical_transactions.len() as i64),
        ),
        (
            "transactions".into(),
            Value::List(
                preparation
                    .physical_transactions
                    .iter()
                    .map(|transaction| {
                        Value::Map(BTreeMap::from([
                            (
                                "resource".into(),
                                transaction
                                    .resource
                                    .map(|resource| Value::I64(resource.0 .0 as i64))
                                    .unwrap_or(Value::Null),
                            ),
                            (
                                "description".into(),
                                Value::String(transaction.description.clone()),
                            ),
                            ("payload".into(), transaction.payload.clone()),
                        ]))
                    })
                    .collect(),
            ),
        ),
    ]))
}

fn timing_plan_transition_summary(transition: &TimingPlanTransition) -> Value {
    Value::Map(BTreeMap::from([
        ("driver".into(), Value::I64(transition.driver.0 as i64)),
        ("action".into(), Value::String(transition.action.clone())),
        (
            "physical_transactions".into(),
            Value::I64(transition.physical_transactions.len() as i64),
        ),
        (
            "transactions".into(),
            Value::List(
                transition
                    .physical_transactions
                    .iter()
                    .map(|transaction| {
                        Value::Map(BTreeMap::from([
                            (
                                "resource".into(),
                                transaction
                                    .resource
                                    .map(|resource| Value::I64(resource.0 .0 as i64))
                                    .unwrap_or(Value::Null),
                            ),
                            (
                                "description".into(),
                                Value::String(transaction.description.clone()),
                            ),
                            ("payload".into(), transaction.payload.clone()),
                        ]))
                    })
                    .collect(),
            ),
        ),
    ]))
}

fn trigger_route_summary(route: &TriggerRoute) -> Value {
    Value::Map(BTreeMap::from([
        ("from".into(), Value::I64(route.from.0 .0 as i64)),
        ("to".into(), Value::I64(route.to.0 .0 as i64)),
        (
            "signal".into(),
            Value::String(trigger_signal_name(&route.signal).into()),
        ),
        (
            "edge".into(),
            Value::String(trigger_edge_name(&route.edge).into()),
        ),
        (
            "delay".into(),
            Value::TimeInterval(TimeInterval::from_seconds(route.delay.as_secs_f64())),
        ),
    ]))
}

fn device_sequence_summary(sequence: &DeviceSequence) -> Value {
    Value::Map(BTreeMap::from([
        ("device".into(), Value::I64(sequence.device.0 .0 as i64)),
        ("property".into(), Value::String(sequence.property.clone())),
        ("count".into(), Value::I64(sequence.values.len() as i64)),
        (
            "values".into(),
            Value::List(sequence.values.iter().take(8).cloned().collect()),
        ),
    ]))
}

fn start_condition_summary(condition: &StartCondition) -> Value {
    match condition {
        StartCondition::Software => Value::String("software".into()),
        StartCondition::ExternalTrigger(device) => Value::Map(BTreeMap::from([
            ("kind".into(), Value::String("external_trigger".into())),
            ("device".into(), Value::I64(device.0 .0 as i64)),
        ])),
        StartCondition::At(time) => Value::Map(BTreeMap::from([
            ("kind".into(), Value::String("at".into())),
            ("ticks".into(), Value::I64(time.ticks as i64)),
            (
                "clock".into(),
                time.clock
                    .map(|device| Value::I64(device.0 .0 as i64))
                    .unwrap_or(Value::Null),
            ),
        ])),
    }
}

fn stop_condition_summary(condition: &StopCondition) -> Value {
    match condition {
        StopCondition::Manual => Value::String("manual".into()),
        StopCondition::Count(count) => Value::Map(BTreeMap::from([
            ("kind".into(), Value::String("count".into())),
            ("count".into(), Value::I64(*count as i64)),
        ])),
        StopCondition::Duration(duration) => Value::Map(BTreeMap::from([
            ("kind".into(), Value::String("duration".into())),
            (
                "duration".into(),
                Value::TimeInterval(TimeInterval::from_seconds(duration.as_secs_f64())),
            ),
        ])),
    }
}

fn trigger_signal_name(signal: &TriggerSignal) -> &'static str {
    match signal {
        TriggerSignal::Ttl => "ttl",
        TriggerSignal::Analog => "analog",
        TriggerSignal::Software => "software",
        TriggerSignal::Clock => "clock",
    }
}

fn trigger_edge_name(edge: &TriggerEdge) -> &'static str {
    match edge {
        TriggerEdge::Rising => "rising",
        TriggerEdge::Falling => "falling",
        TriggerEdge::Both => "both",
        TriggerEdge::LevelHigh => "level_high",
        TriggerEdge::LevelLow => "level_low",
    }
}

pub struct LocalRuntime {
    ids: DriverIdAllocator,
    next_command: AtomicU64,
    next_operation: AtomicU64,
    operations: Arc<OperationTable>,
    op_tokens: Arc<Mutex<HashMap<OperationId, (DriverId, DriverToken)>>>,
    armed_plans: Arc<Mutex<HashMap<OperationId, ArmedTimingPlan>>>,
    frames: Arc<FrameStore>,
    device_drivers: HashMap<DeviceId, DriverId>,
    device_descriptors: HashMap<DeviceId, DeviceDescriptor>,
    device_capabilities: HashMap<DeviceId, Vec<CapabilityDescriptor>>,
    lanes: HashMap<DriverId, LaneHandle>,
    bindings: Vec<DeviceBinding>,
    bus: EventBus,
}

/// A device serving a role for another device, across driver boundaries.
///
/// A driver's own [`Driver::graph`] can only express dependencies between devices it owns,
/// which is right for a composed device built from one controller's parts. It cannot say
/// that *this* camera is the one bolted to *that* reader, because the two are different
/// drivers — and that is the ordinary case for an instrument whose camera enumerates as its
/// own USB device. The runtime holds those bindings instead, so a client can ask what serves
/// a role without knowing which driver answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceBinding {
    /// The device that provides the role.
    pub provider: DeviceId,
    /// The device that needs it.
    pub consumer: DeviceId,
    pub role: Role,
}

impl LocalRuntime {
    pub fn new() -> Self {
        Self {
            ids: DriverIdAllocator::new(DriverId(1)),
            next_command: AtomicU64::new(1),
            next_operation: AtomicU64::new(1),
            operations: Arc::new(OperationTable::new()),
            op_tokens: Arc::new(Mutex::new(HashMap::new())),
            armed_plans: Arc::new(Mutex::new(HashMap::new())),
            frames: Arc::new(FrameStore::new()),
            device_drivers: HashMap::new(),
            device_descriptors: HashMap::new(),
            device_capabilities: HashMap::new(),
            lanes: HashMap::new(),
            bindings: Vec::new(),
            bus: EventBus::new(),
        }
    }

    pub fn from_drivers(drivers: Vec<Box<dyn Driver>>) -> Self {
        let mut runtime = Self::new();
        for driver in drivers {
            runtime
                .add_driver(driver)
                .expect("initial driver set must not contain duplicate driver or device ids");
        }
        runtime
    }

    pub fn add_driver(&mut self, driver: Box<dyn Driver>) -> Result<Vec<DeviceDescriptor>> {
        let id = driver.id();
        if self.lanes.contains_key(&id) {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                format!("driver {:?} is already registered", id),
            ));
        }

        let descriptors = driver.descriptors();
        let capabilities = descriptors
            .iter()
            .map(|descriptor| {
                (
                    descriptor.id,
                    driver
                        .capabilities(descriptor.id)
                        .into_iter()
                        .filter(|capability| !capability.is_hidden_maintenance())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        for descriptor in &descriptors {
            if self.device_drivers.contains_key(&descriptor.id) {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    format!("device {:?} is already registered", descriptor.id),
                ));
            }
        }

        for descriptor in &descriptors {
            self.device_drivers.insert(descriptor.id, id);
            self.device_descriptors
                .insert(descriptor.id, descriptor.clone());
        }
        for (device, capabilities) in capabilities {
            self.device_capabilities.insert(device, capabilities);
        }
        let lane = spawn_lane(
            driver,
            self.operations.clone(),
            self.op_tokens.clone(),
            self.frames.clone(),
            self.bus.clone(),
        );
        self.lanes.insert(id, lane);
        for descriptor in &descriptors {
            self.bus.publish(Event::DeviceArrived(descriptor.clone()));
        }
        Ok(descriptors)
    }

    pub fn add_driver_factory<D, F>(&mut self, factory: F) -> Result<Vec<DeviceDescriptor>>
    where
        D: Driver + 'static,
        F: FnOnce(DriverId) -> D,
    {
        let id = self.ids.next_id();
        self.add_driver(Box::new(factory(id)))
    }

    pub fn add_driver_factory_result<D, F>(&mut self, factory: F) -> Result<Vec<DeviceDescriptor>>
    where
        D: Driver + 'static,
        F: FnOnce(DriverId) -> Result<D>,
    {
        let id = self.ids.next_id();
        self.add_driver(Box::new(factory(id)?))
    }

    pub fn add_boxed_driver_factory<F>(&mut self, factory: F) -> Result<Vec<DeviceDescriptor>>
    where
        F: FnOnce(DriverId) -> Result<Box<dyn Driver>>,
    {
        let id = self.ids.next_id();
        self.add_driver(factory(id)?)
    }

    pub fn add_candidate(&mut self, candidate: DriverCandidate) -> Result<Vec<DeviceDescriptor>> {
        self.add_driver(candidate.into_driver())
    }

    pub fn remove_driver(&mut self, id: DriverId) -> Result<Vec<DeviceId>> {
        let lane = self
            .lanes
            .remove(&id)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "driver is not registered"))?;
        let _ = lane.tx.send(LaneCommand::Shutdown);

        let mut removed_devices = self
            .device_drivers
            .iter()
            .filter_map(|(device, driver)| (*driver == id).then_some(*device))
            .collect::<Vec<_>>();
        removed_devices.sort();
        for device in &removed_devices {
            self.device_drivers.remove(device);
            self.device_descriptors.remove(device);
            self.device_capabilities.remove(device);
            self.bus.publish(Event::DeviceRemoved(*device));
        }
        self.frames.remove_devices(&removed_devices);
        // A binding to a device that is gone would answer a role lookup with something the
        // runtime can no longer reach.
        self.bindings.retain(|binding| {
            !removed_devices.contains(&binding.provider)
                && !removed_devices.contains(&binding.consumer)
        });
        self.armed_plans
            .lock()
            .expect("armed plans poisoned")
            .retain(|_, armed| {
                !armed
                    .plan
                    .participants
                    .iter()
                    .any(|device| removed_devices.contains(device))
            });

        let cancelled_ops = {
            let mut tokens = self.op_tokens.lock().expect("operation tokens poisoned");
            let cancelled_ops = tokens
                .iter()
                .filter_map(|(operation, (driver, _))| (*driver == id).then_some(*operation))
                .collect::<Vec<_>>();
            tokens.retain(|_, (driver, _)| *driver != id);
            cancelled_ops
        };
        for operation in cancelled_ops {
            set_status(
                &self.operations,
                &self.bus,
                operation,
                OperationStatus::Cancelled,
            );
        }

        Ok(removed_devices)
    }

    pub fn drivers(&self) -> Vec<DriverId> {
        let mut drivers = self.lanes.keys().copied().collect::<Vec<_>>();
        drivers.sort();
        drivers
    }

    pub fn contains_driver(&self, id: DriverId) -> bool {
        self.lanes.contains_key(&id)
    }

    pub fn devices(&self) -> Vec<&DeviceDescriptor> {
        let mut devices = self.device_descriptors.values().collect::<Vec<_>>();
        devices.sort_by_key(|device| device.id);
        devices
    }

    pub fn device(&self, device: impl Into<DeviceId>) -> Option<&DeviceDescriptor> {
        let device = device.into();
        self.device_descriptors.get(&device)
    }

    /// Record that `provider` serves `role` for `consumer`.
    ///
    /// Both devices must already be registered — a binding to something absent would be a
    /// promise the runtime cannot keep. Re-binding the same role replaces the previous
    /// provider rather than accumulating two answers to one question.
    pub fn bind_device(
        &mut self,
        provider: impl Into<DeviceId>,
        consumer: impl Into<DeviceId>,
        role: Role,
    ) -> Result<()> {
        let provider = provider.into();
        let consumer = consumer.into();
        for (device, which) in [(provider, "provider"), (consumer, "consumer")] {
            if !self.device_descriptors.contains_key(&device) {
                return Err(Error::new(
                    ErrorCode::InvalidGraph,
                    format!("binding {which} {:?} is not a registered device", device),
                ));
            }
        }
        if provider == consumer {
            return Err(Error::new(
                ErrorCode::InvalidGraph,
                "a device cannot serve a role for itself",
            ));
        }
        self.bindings
            .retain(|binding| !(binding.consumer == consumer && binding.role == role));
        self.bindings.push(DeviceBinding {
            provider,
            consumer,
            role,
        });
        Ok(())
    }

    /// Apply the dependencies a hardware configuration declares.
    ///
    /// The config format has carried these all along; this is what makes them take effect.
    pub fn apply_config_dependencies(
        &mut self,
        config: &crate::config::HardwareConfig,
    ) -> Result<()> {
        for dependency in &config.dependencies {
            self.bind_device(dependency.from, dependency.to, dependency.role.clone())?;
        }
        Ok(())
    }

    /// Every cross-driver binding, in the order it was made.
    pub fn bindings(&self) -> &[DeviceBinding] {
        &self.bindings
    }

    /// The device serving `role` for `consumer`, if one is bound.
    pub fn bound_device(
        &self,
        consumer: impl Into<DeviceId>,
        role: &Role,
    ) -> Option<&DeviceDescriptor> {
        let consumer = consumer.into();
        let binding = self
            .bindings
            .iter()
            .find(|binding| binding.consumer == consumer && &binding.role == role)?;
        self.device_descriptors.get(&binding.provider)
    }

    /// Everything bound to `consumer`, by role.
    pub fn bound_devices(&self, consumer: impl Into<DeviceId>) -> Vec<(&Role, &DeviceDescriptor)> {
        let consumer = consumer.into();
        self.bindings
            .iter()
            .filter(|binding| binding.consumer == consumer)
            .filter_map(|binding| {
                Some((
                    &binding.role,
                    self.device_descriptors.get(&binding.provider)?,
                ))
            })
            .collect()
    }

    pub fn safety_summary(
        &self,
        device: impl Into<DeviceId>,
        timeout: Duration,
    ) -> Result<SafetySummary> {
        let device = device.into();
        let descriptor = self
            .device(device)
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown device"))?;
        let keys = descriptor
            .properties
            .iter()
            .filter(|property| property.readable)
            .filter(|property| SafetySummary::property_key_is_safety(&property.key))
            .map(|property| property.key.clone())
            .collect::<Vec<_>>();

        let mut values = BTreeMap::new();
        for key in keys {
            let value = self.execute(
                Command::ReadProperty {
                    device,
                    key: key.clone(),
                },
                timeout,
            )?;
            values.insert(key, value);
        }
        Ok(SafetySummary::from_values(device, values))
    }

    pub fn device_by_kind(&self, kind: &str) -> Result<&DeviceDescriptor> {
        self.devices()
            .into_iter()
            .find(|device| device.has_kind(kind))
            .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, format!("missing {kind} device")))
    }

    pub fn device_by_kinds(&self, kinds: &[&str]) -> Result<&DeviceDescriptor> {
        self.devices()
            .into_iter()
            .find(|device| device.has_kinds(kinds))
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidCommand,
                    format!("missing device with kinds {}", kinds.join(", ")),
                )
            })
    }

    pub fn devices_by_kind(&self, kind: &str) -> Vec<&DeviceDescriptor> {
        self.devices()
            .into_iter()
            .filter(|device| device.has_kind(kind))
            .collect()
    }

    pub fn device_by_capability(&self, kind: CapabilityKind) -> Result<&DeviceDescriptor> {
        self.devices()
            .into_iter()
            .find(|device| {
                self.device_capabilities
                    .get(&device.id)
                    .is_some_and(|capabilities| {
                        capabilities
                            .iter()
                            .any(|capability| capability.kind == kind)
                    })
            })
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidCommand,
                    format!("missing device with capability {}", kind.name()),
                )
            })
    }

    pub fn devices_by_capability(&self, kind: CapabilityKind) -> Vec<&DeviceDescriptor> {
        self.devices()
            .into_iter()
            .filter(|device| {
                self.device_capabilities
                    .get(&device.id)
                    .is_some_and(|capabilities| {
                        capabilities
                            .iter()
                            .any(|capability| capability.kind == kind)
                    })
            })
            .collect()
    }

    pub fn capabilities(&self, device: impl Into<DeviceId>) -> Result<&[CapabilityDescriptor]> {
        let device = device.into();
        self.device_capabilities
            .get(&device)
            .map(Vec::as_slice)
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidCommand,
                    format!("unknown target device {:?}", device),
                )
            })
    }

    pub fn capability_by_kind(
        &self,
        device: impl Into<DeviceId>,
        kind: CapabilityKind,
    ) -> Result<CapabilityDescriptor> {
        let device = device.into();
        self.capabilities(device)?
            .iter()
            .find(|capability| capability.kind == kind)
            .cloned()
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::Unsupported,
                    format!("device {:?} does not expose {}", device, kind.name()),
                )
            })
    }

    pub fn submit_capability(
        &self,
        device: impl Into<DeviceId>,
        kind: CapabilityKind,
        request: CapabilityRequest,
    ) -> Result<OperationHandle> {
        let device = device.into();
        let capability = self.capability_by_kind(device, kind)?;
        self.submit(Command::invoke(device, capability.id, request))
    }

    pub fn submit_request(
        &self,
        device: impl Into<DeviceId>,
        request: impl Into<CapabilityRequest>,
    ) -> Result<OperationHandle> {
        let device = device.into();
        let request = request.into();
        let kind = request.inferred_capability_kind().ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "request kind {:?} does not imply a unique capability kind",
                    request.request_kind()
                ),
            )
        })?;
        self.submit_capability(device, kind, request)
    }

    pub fn execute_capability(
        &self,
        device: impl Into<DeviceId>,
        kind: CapabilityKind,
        request: CapabilityRequest,
        timeout: Duration,
    ) -> Result<Value> {
        let device = device.into();
        let operation = self.submit_capability(device, kind, request)?;
        self.wait_completed(operation.id, timeout)
    }

    pub fn execute_request(
        &self,
        device: impl Into<DeviceId>,
        request: impl Into<CapabilityRequest>,
        timeout: Duration,
    ) -> Result<Value> {
        let device = device.into();
        let operation = self.submit_request(device, request)?;
        self.wait_completed(operation.id, timeout)
    }

    fn next_command_id(&self) -> CommandId {
        CommandId(self.next_command.fetch_add(1, Ordering::Relaxed))
    }

    fn next_operation_id(&self) -> OperationId {
        OperationId(self.next_operation.fetch_add(1, Ordering::Relaxed))
    }

    fn driver_for(&self, devices: &[DeviceId]) -> Result<DriverId> {
        let mut driver = None;
        for device in devices {
            let next = self.device_drivers.get(device).copied().ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidCommand,
                    format!("unknown target device {:?}", device),
                )
            })?;
            if let Some(existing) = driver {
                if existing != next {
                    return Err(Error::new(
                        ErrorCode::Unsupported,
                        "single command currently cannot span multiple drivers",
                    ));
                }
            }
            driver = Some(next);
        }
        driver.ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "command has no target device"))
    }

    fn is_multi_driver_state_set(&self, set: &StateSet) -> Result<bool> {
        let mut driver = None;
        for write in &set.writes {
            let next = self
                .device_drivers
                .get(&write.device)
                .copied()
                .ok_or_else(|| {
                    Error::new(
                        ErrorCode::InvalidCommand,
                        format!("unknown target device {:?}", write.device),
                    )
                })?;
            if let Some(existing) = driver {
                if existing != next {
                    return Ok(true);
                }
            }
            driver = Some(next);
        }
        Ok(false)
    }
}

impl Runtime for LocalRuntime {
    fn submit(&self, command: Command) -> Result<OperationHandle> {
        self.validate_command(&command)?;
        match &command {
            Command::Arm(plan) => return self.submit_runtime_arm(plan.clone()),
            Command::Start(armed_operation) => {
                return self.submit_runtime_plan_transition(*armed_operation, "started", false)
            }
            Command::Stop(armed_operation) => {
                return self.submit_runtime_plan_transition(*armed_operation, "stopped", true)
            }
            Command::ApplyStateSet(set) if self.is_multi_driver_state_set(set)? => {
                return self.submit_runtime_state_set(set.clone());
            }
            _ => {}
        }

        let devices = command.target_devices();
        let driver = self.driver_for(&devices)?;
        let lane = self
            .lanes
            .get(&driver)
            .ok_or_else(|| Error::new(ErrorCode::Driver, "driver lane missing"))?;
        let operation = self.next_operation_id();
        let batch = CommandBatch {
            id: self.next_command_id(),
            commands: vec![command],
        };
        self.operations
            .register(operation, devices.clone(), OperationStatus::Queued);
        lane.tx
            .send(LaneCommand::Run { operation, batch })
            .map_err(|_| Error::new(ErrorCode::Driver, "driver lane stopped"))?;
        Ok(OperationHandle {
            id: operation,
            devices,
        })
    }

    fn status(&self, op: OperationId) -> OperationStatus {
        self.operations.get(op)
    }

    fn wait(&self, op: OperationId, timeout: Duration) -> Result<OperationStatus> {
        self.operations.wait(op, timeout)
    }

    fn frame(&self, handle: FrameHandle) -> Result<Option<Frame>> {
        Ok(self.frames.get(handle))
    }

    fn stream_status(&self, stream: StreamId) -> Result<Option<FrameStreamStatus>> {
        Ok(self.frames.status(stream))
    }

    fn cancel(&self, op: OperationId) -> Result<CancelResult> {
        let Some((driver, _)) = self
            .op_tokens
            .lock()
            .expect("operation tokens poisoned")
            .get(&op)
            .copied()
        else {
            return Ok(match self.status(op) {
                OperationStatus::Completed(_)
                | OperationStatus::Failed(_)
                | OperationStatus::Cancelled
                | OperationStatus::TimedOut => CancelResult::AlreadyFinished,
                _ => CancelResult::Unsupported,
            });
        };
        let lane = self
            .lanes
            .get(&driver)
            .ok_or_else(|| Error::new(ErrorCode::Driver, "driver lane missing"))?;
        let (tx, rx) = mpsc::channel();
        lane.tx
            .send(LaneCommand::Cancel {
                operation: op,
                reply: tx,
            })
            .map_err(|_| Error::new(ErrorCode::Driver, "driver lane stopped"))?;
        rx.recv()
            .map_err(|_| Error::new(ErrorCode::Driver, "driver lane stopped"))
    }

    fn subscribe(&self, filter: EventFilter) -> Subscription {
        self.bus.subscribe(filter)
    }
}

impl LocalRuntime {
    fn validate_command(&self, command: &Command) -> Result<()> {
        match command {
            Command::ReadProperty { device, key } => {
                self.require_readable_property(*device, key)?;
                Ok(())
            }
            Command::WriteProperty { device, key, value } => {
                let schema = self.require_writable_property(*device, key)?;
                schema.validate(value)?;
                Ok(())
            }
            Command::Invoke {
                device,
                capability,
                request,
            } => self.validate_invoke(*device, *capability, request),
            Command::ApplyStateSet(set) => {
                if set.writes.is_empty() {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "state set has no target device",
                    ));
                }
                for write in &set.writes {
                    let schema = self.require_writable_property(write.device, &write.property)?;
                    schema.validate(&write.value)?;
                }
                Ok(())
            }
            Command::Arm(_) | Command::Start(_) | Command::Stop(_) => Ok(()),
        }
    }

    fn validate_invoke(
        &self,
        device: DeviceId,
        capability: CapabilityId,
        request: &CapabilityRequest,
    ) -> Result<()> {
        let descriptor = self.capability_descriptor(device, capability)?;
        if descriptor.accepts_request(request)
            || descriptor_accepts_legacy_request(descriptor, request)
        {
            reject_hidden_maintenance_request(descriptor, request)?;
            Ok(())
        } else {
            Err(Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "{} on {:?} expects {:?}, got {:?}",
                    descriptor.kind.name(),
                    device,
                    descriptor.preferred_request_kind(),
                    request.request_kind()
                ),
            ))
        }
    }

    fn capability_descriptor(
        &self,
        device: DeviceId,
        capability: CapabilityId,
    ) -> Result<&CapabilityDescriptor> {
        let Some(capabilities) = self.device_capabilities.get(&device) else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                format!("unknown target device {:?}", device),
            ));
        };
        capabilities
            .iter()
            .find(|descriptor| descriptor.id == capability)
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::Unsupported,
                    format!(
                        "device {:?} does not expose capability {:?}",
                        device, capability
                    ),
                )
            })
    }

    fn submit_runtime_state_set(&self, set: StateSet) -> Result<OperationHandle> {
        if set.writes.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "state set has no target device",
            ));
        }

        let mut by_driver: HashMap<DriverId, Vec<StateWrite>> = HashMap::new();
        let mut devices = Vec::new();
        for write in &set.writes {
            if !devices.contains(&write.device) {
                devices.push(write.device);
            }
            let driver = self
                .device_drivers
                .get(&write.device)
                .copied()
                .ok_or_else(|| {
                    Error::new(
                        ErrorCode::InvalidCommand,
                        format!("unknown target device {:?}", write.device),
                    )
                })?;
            by_driver.entry(driver).or_default().push(write.clone());
        }

        let operation = self.next_operation_id();
        self.operations
            .register(operation, devices.clone(), OperationStatus::Queued);
        set_status(
            &self.operations,
            &self.bus,
            operation,
            OperationStatus::Running { progress: None },
        );

        let mut fragments = Vec::new();
        for (driver, writes) in by_driver {
            let lane = self
                .lanes
                .get(&driver)
                .ok_or_else(|| Error::new(ErrorCode::Driver, "driver lane missing"))?;
            let state_set = StateSet {
                name: set.name.clone(),
                writes,
                commit: set.commit.clone(),
            };
            fragments.push((
                lane.clone(),
                CommandBatch {
                    id: self.next_command_id(),
                    commands: vec![Command::ApplyStateSet(state_set)],
                },
            ));
        }

        let operations = self.operations.clone();
        let bus = self.bus.clone();
        thread::spawn(move || {
            let mut replies = Vec::new();
            for (lane, batch) in fragments {
                let (tx, rx) = mpsc::channel();
                if lane
                    .tx
                    .send(LaneCommand::RunFragment { batch, reply: tx })
                    .is_err()
                {
                    set_status(
                        &operations,
                        &bus,
                        operation,
                        OperationStatus::Failed(
                            Error::new(ErrorCode::Driver, "driver lane stopped").into(),
                        ),
                    );
                    return;
                }
                replies.push(rx);
            }

            let mut merged = BTreeMap::new();
            for rx in replies {
                match rx.recv() {
                    Ok(Ok(Value::Map(map))) => {
                        merged.extend(map);
                    }
                    Ok(Ok(value)) => {
                        merged.insert(format!("fragment_{}", merged.len()), value);
                    }
                    Ok(Err(error)) => {
                        set_status(
                            &operations,
                            &bus,
                            operation,
                            OperationStatus::Failed(error.into()),
                        );
                        return;
                    }
                    Err(_) => {
                        set_status(
                            &operations,
                            &bus,
                            operation,
                            OperationStatus::Failed(
                                Error::new(ErrorCode::Driver, "driver lane stopped").into(),
                            ),
                        );
                        return;
                    }
                }
            }

            set_status(
                &operations,
                &bus,
                operation,
                OperationStatus::Completed(Value::Map(merged)),
            );
        });

        Ok(OperationHandle {
            id: operation,
            devices,
        })
    }

    fn submit_runtime_arm(&self, plan: TimingPlan) -> Result<OperationHandle> {
        self.validate_plan_devices(&plan)?;
        let preparations = self.prepare_timing_plan_by_driver(&plan)?;
        let armed = ArmedTimingPlan {
            plan: plan.clone(),
            preparations,
        };
        let operation = self.next_operation_id();
        let devices = plan.participants.clone();
        self.operations
            .register(operation, devices.clone(), OperationStatus::Queued);
        set_status(
            &self.operations,
            &self.bus,
            operation,
            OperationStatus::Running { progress: None },
        );
        self.armed_plans
            .lock()
            .expect("armed plans poisoned")
            .insert(operation, armed.clone());
        set_status(
            &self.operations,
            &self.bus,
            operation,
            OperationStatus::Completed(timing_plan_summary("armed", &armed, &[])),
        );
        Ok(OperationHandle {
            id: operation,
            devices,
        })
    }

    fn submit_runtime_plan_transition(
        &self,
        armed_operation: OperationId,
        state: &str,
        remove: bool,
    ) -> Result<OperationHandle> {
        let armed = {
            let mut plans = self.armed_plans.lock().expect("armed plans poisoned");
            if remove {
                plans.remove(&armed_operation)
            } else {
                plans.get(&armed_operation).cloned()
            }
        }
        .ok_or_else(|| Error::new(ErrorCode::InvalidCommand, "unknown armed timing plan"))?;
        let transitions = self.prepare_timing_transition_by_driver(
            &armed,
            if remove {
                TimingTransitionAction::Stop
            } else {
                TimingTransitionAction::Start
            },
        )?;

        let operation = self.next_operation_id();
        let devices = armed.plan.participants.clone();
        self.operations
            .register(operation, devices.clone(), OperationStatus::Queued);
        set_status(
            &self.operations,
            &self.bus,
            operation,
            OperationStatus::Running { progress: None },
        );
        set_status(
            &self.operations,
            &self.bus,
            operation,
            OperationStatus::Completed(timing_plan_summary(state, &armed, &transitions)),
        );
        Ok(OperationHandle {
            id: operation,
            devices,
        })
    }

    fn prepare_timing_plan_by_driver(
        &self,
        plan: &TimingPlan,
    ) -> Result<Vec<TimingPlanPreparation>> {
        let mut drivers = plan
            .participants
            .iter()
            .map(|device| {
                self.device_drivers.get(device).copied().ok_or_else(|| {
                    Error::new(
                        ErrorCode::InvalidCommand,
                        format!("unknown timing-plan participant {:?}", device),
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?;
        drivers.sort();
        drivers.dedup();

        let mut preparations = Vec::new();
        for driver in drivers {
            let lane = self
                .lanes
                .get(&driver)
                .ok_or_else(|| Error::new(ErrorCode::Driver, "driver lane missing"))?;
            let (tx, rx) = mpsc::channel();
            lane.tx
                .send(LaneCommand::PrepareTimingPlan {
                    plan: plan.clone(),
                    command_id: self.next_command_id(),
                    reply: tx,
                })
                .map_err(|_| Error::new(ErrorCode::Driver, "driver lane stopped"))?;
            let prepared = rx
                .recv()
                .map_err(|_| Error::new(ErrorCode::Driver, "driver lane stopped"))??;
            preparations.push(TimingPlanPreparation {
                driver,
                physical_transactions: prepared.physical_transactions,
            });
        }
        Ok(preparations)
    }

    fn prepare_timing_transition_by_driver(
        &self,
        armed: &ArmedTimingPlan,
        action: TimingTransitionAction,
    ) -> Result<Vec<TimingPlanTransition>> {
        let mut transitions = Vec::new();
        for preparation in &armed.preparations {
            let lane = self
                .lanes
                .get(&preparation.driver)
                .ok_or_else(|| Error::new(ErrorCode::Driver, "driver lane missing"))?;
            let (tx, rx) = mpsc::channel();
            lane.tx
                .send(LaneCommand::PrepareTimingTransition {
                    armed: armed.clone(),
                    action,
                    command_id: self.next_command_id(),
                    reply: tx,
                })
                .map_err(|_| Error::new(ErrorCode::Driver, "driver lane stopped"))?;
            let prepared = rx
                .recv()
                .map_err(|_| Error::new(ErrorCode::Driver, "driver lane stopped"))??;
            transitions.push(TimingPlanTransition {
                driver: preparation.driver,
                action: action.name().into(),
                physical_transactions: prepared.physical_transactions,
            });
        }
        Ok(transitions)
    }

    fn validate_plan_devices(&self, plan: &TimingPlan) -> Result<()> {
        if plan.participants.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "timing plan has no participants",
            ));
        }
        for device in &plan.participants {
            if !self.device_drivers.contains_key(device) {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    format!("unknown timing-plan participant {:?}", device),
                ));
            }
        }
        for route in &plan.routes {
            for device in [route.from, route.to] {
                if !plan.participants.contains(&device) {
                    return Err(Error::new(
                        ErrorCode::InvalidCommand,
                        "trigger route endpoints must be timing-plan participants",
                    ));
                }
            }
            self.require_capability(route.from, CapabilityKind::TriggerSource)?;
            self.require_capability(route.to, CapabilityKind::TriggerSink)?;
        }
        for sequence in &plan.sequences {
            if !plan.participants.contains(&sequence.device) {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "device sequence target must be a timing-plan participant",
                ));
            }
            self.require_sequenceable_property(sequence.device, &sequence.property)?;
            for value in &sequence.values {
                self.validate_sequence_value(sequence.device, &sequence.property, value)?;
            }
        }
        for device in &plan.arm_order {
            if !plan.participants.contains(device) {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "arm order device must be a timing-plan participant",
                ));
            }
        }
        if let StartCondition::ExternalTrigger(device) = plan.start {
            if !plan.participants.contains(&device) {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "external trigger device must be a timing-plan participant",
                ));
            }
            self.require_capability(device, CapabilityKind::TriggerSource)?;
        }
        Ok(())
    }

    fn require_capability(&self, device: DeviceId, kind: CapabilityKind) -> Result<()> {
        let Some(capabilities) = self.device_capabilities.get(&device) else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                format!("unknown timing-plan device {:?}", device),
            ));
        };
        if capabilities
            .iter()
            .any(|capability| capability.kind == kind)
        {
            Ok(())
        } else {
            Err(Error::new(
                ErrorCode::Unsupported,
                format!("device {:?} does not expose {}", device, kind.name()),
            ))
        }
    }

    fn require_sequenceable_property(&self, device: DeviceId, property: &str) -> Result<()> {
        let schema = self.require_writable_property(device, property)?;
        if schema.sequenceable {
            Ok(())
        } else {
            Err(Error::new(
                ErrorCode::Unsupported,
                format!("property {property} on {:?} is not sequenceable", device),
            ))
        }
    }

    fn validate_sequence_value(
        &self,
        device: DeviceId,
        property: &str,
        value: &Value,
    ) -> Result<()> {
        self.property_schema(device, property)?.validate(value)
    }

    fn require_readable_property(
        &self,
        device: DeviceId,
        property: &str,
    ) -> Result<&PropertySchema> {
        let schema = self.property_schema(device, property)?;
        if schema.readable {
            Ok(schema)
        } else {
            Err(Error::new(
                ErrorCode::Unsupported,
                format!("property {property} on {:?} is not readable", device),
            ))
        }
    }

    fn require_writable_property(
        &self,
        device: DeviceId,
        property: &str,
    ) -> Result<&PropertySchema> {
        let schema = self.property_schema(device, property)?;
        if schema.writable {
            Ok(schema)
        } else {
            Err(Error::new(
                ErrorCode::Unsupported,
                format!("property {property} on {:?} is not writable", device),
            ))
        }
    }

    fn property_schema(&self, device: DeviceId, property: &str) -> Result<&PropertySchema> {
        let descriptor = self.device_descriptors.get(&device).ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidCommand,
                format!("unknown timing-plan device {:?}", device),
            )
        })?;
        descriptor
            .properties
            .iter()
            .find(|schema| schema.key == property)
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::InvalidProperty,
                    format!("unknown timing-plan property {property}"),
                )
            })
    }
}

fn spawn_lane(
    mut driver: Box<dyn Driver>,
    operations: Arc<OperationTable>,
    op_tokens: Arc<Mutex<HashMap<OperationId, (DriverId, DriverToken)>>>,
    frames: Arc<FrameStore>,
    bus: EventBus,
) -> LaneHandle {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let driver_id = driver.id();
        let mut token_ops: HashMap<DriverToken, OperationId> = HashMap::new();
        loop {
            match rx.recv_timeout(Duration::from_millis(10)) {
                Ok(LaneCommand::Run { operation, batch }) => {
                    set_status(
                        &operations,
                        &bus,
                        operation,
                        OperationStatus::Running { progress: None },
                    );
                    match driver
                        .prepare(&batch)
                        .and_then(|prepared| driver.dispatch(prepared))
                    {
                        Ok(token) => {
                            token_ops.insert(token, operation);
                            op_tokens
                                .lock()
                                .expect("operation tokens poisoned")
                                .insert(operation, (driver_id, token));
                            drain_driver_events(
                                &mut *driver,
                                &operations,
                                &op_tokens,
                                &frames,
                                &bus,
                                &mut token_ops,
                            );
                        }
                        Err(error) => {
                            set_status(
                                &operations,
                                &bus,
                                operation,
                                OperationStatus::Failed(error.into()),
                            );
                        }
                    }
                }
                Ok(LaneCommand::RunFragment { batch, reply }) => {
                    let result = run_fragment(
                        &mut *driver,
                        &operations,
                        &op_tokens,
                        &frames,
                        &bus,
                        &mut token_ops,
                        batch,
                    );
                    let _ = reply.send(result);
                }
                Ok(LaneCommand::PrepareTimingPlan {
                    plan,
                    command_id,
                    reply,
                }) => {
                    let _ = reply.send(driver.prepare_timing_plan(&plan, command_id));
                }
                Ok(LaneCommand::PrepareTimingTransition {
                    armed,
                    action,
                    command_id,
                    reply,
                }) => {
                    let result = match action {
                        TimingTransitionAction::Start => {
                            driver.start_timing_plan(&armed, command_id)
                        }
                        TimingTransitionAction::Stop => driver.stop_timing_plan(&armed, command_id),
                    };
                    let _ = reply.send(result);
                }
                Ok(LaneCommand::Cancel { operation, reply }) => {
                    let token = op_tokens
                        .lock()
                        .expect("operation tokens poisoned")
                        .get(&operation)
                        .map(|(_, token)| *token);
                    let result = token
                        .map(|token| driver.cancel(token))
                        .unwrap_or(CancelResult::Unsupported);
                    if result == CancelResult::Cancelled {
                        set_status(&operations, &bus, operation, OperationStatus::Cancelled);
                    }
                    let _ = reply.send(result);
                }
                Ok(LaneCommand::Shutdown) => break,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            drain_driver_events(
                &mut *driver,
                &operations,
                &op_tokens,
                &frames,
                &bus,
                &mut token_ops,
            );
        }
    });
    LaneHandle { tx }
}

fn run_fragment(
    driver: &mut dyn Driver,
    operations: &Arc<OperationTable>,
    op_tokens: &Arc<Mutex<HashMap<OperationId, (DriverId, DriverToken)>>>,
    frames: &Arc<FrameStore>,
    bus: &EventBus,
    token_ops: &mut HashMap<DriverToken, OperationId>,
    batch: CommandBatch,
) -> Result<Value> {
    let token = driver
        .prepare(&batch)
        .and_then(|prepared| driver.dispatch(prepared))?;
    loop {
        let mut completed = None;
        for event in driver.poll() {
            match event {
                DriverEvent::Event(event) => bus.publish(event),
                DriverEvent::FrameReady(frame) => match frames.insert(frame) {
                    Ok(events) => {
                        for event in events {
                            bus.publish(event);
                        }
                    }
                    Err(error) => bus.publish(Event::Fault(FaultEvent {
                        device: None,
                        report: error.into(),
                    })),
                },
                DriverEvent::TokenCompleted {
                    token: next_token,
                    value,
                } if next_token == token => {
                    completed = Some(Ok(value));
                }
                DriverEvent::TokenFailed {
                    token: next_token,
                    report,
                } if next_token == token => {
                    completed = Some(Err(Error::new(report.code, report.message)));
                }
                DriverEvent::TokenProgress { token, progress } => {
                    if let Some(operation) = token_ops.get(&token).copied() {
                        set_status(
                            operations,
                            bus,
                            operation,
                            OperationStatus::Running {
                                progress: Some(progress),
                            },
                        );
                    }
                }
                DriverEvent::TokenCompleted { token, value } => {
                    if let Some(operation) = token_ops.remove(&token) {
                        let was_active = op_tokens
                            .lock()
                            .expect("operation tokens poisoned")
                            .remove(&operation)
                            .is_some();
                        if was_active {
                            set_status(
                                operations,
                                bus,
                                operation,
                                OperationStatus::Completed(value),
                            );
                        }
                    }
                }
                DriverEvent::TokenFailed { token, report } => {
                    if let Some(operation) = token_ops.remove(&token) {
                        let was_active = op_tokens
                            .lock()
                            .expect("operation tokens poisoned")
                            .remove(&operation)
                            .is_some();
                        if was_active {
                            set_status(operations, bus, operation, OperationStatus::Failed(report));
                        }
                    }
                }
            }
        }
        if let Some(result) = completed {
            return result;
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn drain_driver_events(
    driver: &mut dyn Driver,
    operations: &Arc<OperationTable>,
    op_tokens: &Arc<Mutex<HashMap<OperationId, (DriverId, DriverToken)>>>,
    frames: &Arc<FrameStore>,
    bus: &EventBus,
    token_ops: &mut HashMap<DriverToken, OperationId>,
) {
    for event in driver.poll() {
        match event {
            DriverEvent::Event(event) => bus.publish(event),
            DriverEvent::FrameReady(frame) => match frames.insert(frame) {
                Ok(events) => {
                    for event in events {
                        bus.publish(event);
                    }
                }
                Err(error) => bus.publish(Event::Fault(FaultEvent {
                    device: None,
                    report: error.into(),
                })),
            },
            DriverEvent::TokenProgress { token, progress } => {
                if let Some(operation) = token_ops.get(&token).copied() {
                    set_status(
                        operations,
                        bus,
                        operation,
                        OperationStatus::Running {
                            progress: Some(progress),
                        },
                    );
                }
            }
            DriverEvent::TokenCompleted { token, value } => {
                if let Some(operation) = token_ops.remove(&token) {
                    let was_active = op_tokens
                        .lock()
                        .expect("operation tokens poisoned")
                        .remove(&operation)
                        .is_some();
                    if was_active {
                        set_status(
                            operations,
                            bus,
                            operation,
                            OperationStatus::Completed(value),
                        );
                    }
                }
            }
            DriverEvent::TokenFailed { token, report } => {
                if let Some(operation) = token_ops.remove(&token) {
                    let was_active = op_tokens
                        .lock()
                        .expect("operation tokens poisoned")
                        .remove(&operation)
                        .is_some();
                    if was_active {
                        set_status(operations, bus, operation, OperationStatus::Failed(report));
                    }
                }
            }
        }
    }
}

fn set_status(
    operations: &Arc<OperationTable>,
    bus: &EventBus,
    operation: OperationId,
    status: OperationStatus,
) {
    operations.insert(operation, status.clone());
    let devices = operations.devices(operation);
    bus.publish(Event::OperationChanged(OperationChanged {
        operation,
        devices,
        status,
    }));
}

fn reject_hidden_maintenance_request(
    descriptor: &CapabilityDescriptor,
    request: &CapabilityRequest,
) -> Result<()> {
    if descriptor.is_hidden_maintenance() {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            format!(
                "{} is a hidden maintenance capability",
                descriptor.kind.name()
            ),
        ));
    }
    match request {
        CapabilityRequest::GenericCommand(request) if request.is_hidden_maintenance() => {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                format!(
                    "GenericCommand {} is a hidden maintenance operation",
                    request.command
                ),
            ));
        }
        CapabilityRequest::Custom(value) if generic_command_value_is_hidden_maintenance(value) => {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "custom request contains a hidden maintenance operation",
            ));
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod discovery_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Fails, as a driver whose bus is unreachable or whose device is foreign
    /// to it does.
    struct Failing;
    impl DriverDiscovery for Failing {
        fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
            Err(Error::new(ErrorCode::Transport, "bus unreachable"))
        }
    }

    /// Records that it was reached, which is the whole question.
    struct Counting(Arc<AtomicUsize>);
    impl DriverDiscovery for Counting {
        fn detect(&mut self) -> Result<Vec<DriverCandidate>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(Vec::new())
        }
    }

    #[test]
    fn one_failing_discovery_does_not_end_the_sweep() {
        // Drivers scan independent buses. A USB camera driver that meets a
        // device it has no profile for must not cost the user the instruments
        // every *other* driver would have found — which is exactly what an
        // early return did, silently, to everything registered after it.
        let runs = Arc::new(AtomicUsize::new(0));
        let mut registry = DiscoveryRegistry::new();
        registry.register(Failing);
        registry.register(Counting(runs.clone()));
        registry.register(Counting(runs.clone()));

        let _ = registry.detect_all();
        assert_eq!(
            runs.load(Ordering::SeqCst),
            2,
            "every discovery after a failing one must still be asked"
        );
    }

    #[test]
    fn a_sweep_that_finds_nothing_reports_why() {
        // The other half of the contract: swallowing failures wholesale would
        // turn "no USB permission" into a silent empty list, which is the
        // hardest kind of bug to chase.
        let mut registry = DiscoveryRegistry::new();
        registry.register(Failing);
        registry.register(Counting(Arc::new(AtomicUsize::new(0))));

        // `DriverCandidate` is not `Debug` (it owns a live driver), so the
        // result is matched rather than unwrapped.
        let Err(error) = registry.detect_all() else {
            panic!("nothing was found, so the failure is the only news");
        };
        assert!(
            error.to_string().contains("bus unreachable"),
            "the original cause must survive: {error}"
        );
    }
}

fn descriptor_accepts_legacy_request(
    descriptor: &CapabilityDescriptor,
    request: &CapabilityRequest,
) -> bool {
    match request {
        CapabilityRequest::GenericCommand(_) => matches!(
            &descriptor.kind,
            CapabilityKind::RawRegisterAccess | CapabilityKind::GenericCommand
        ),
        CapabilityRequest::Custom(_) => matches!(&descriptor.kind, CapabilityKind::Custom(_)),
        _ => false,
    }
}
