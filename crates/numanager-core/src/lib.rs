use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fmt;
use std::time::Duration;

pub mod can;
pub mod config;
pub mod hid;
pub mod runtime;
pub mod serial;
pub mod slots;
pub mod usb;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
}

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    InvalidGraph,
    InvalidCommand,
    InvalidProperty,
    Unsupported,
    Transport,
    Timeout,
    Cancelled,
    Driver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceId(pub NodeId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResourceId(pub NodeId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DriverId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CapabilityId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommandId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OperationId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StreamId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FrameId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameHandle {
    pub stream: StreamId,
    pub frame: FrameId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Resource,
    Hub,
    Device,
    Service,
    Simulator,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdgeKind {
    OwnsResource,
    OffersDevice,
    /// `from` is a required device for `to`, serving the given role.
    ///
    /// This direction keeps `DeviceGraph::initialization_order` dependency
    /// ordered: providers/resources appear before the logical service or device
    /// that uses them.
    UsesDevice {
        role: Role,
    },
    SharesClock,
    SharesTransport,
    RequiresConfig,
}

#[derive(Debug, Clone)]
pub struct GraphNode {
    pub id: NodeId,
    pub kind: NodeKind,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub kind: EdgeKind,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceGraph {
    nodes: BTreeMap<NodeId, GraphNode>,
    edges: Vec<GraphEdge>,
}

impl DeviceGraph {
    pub fn insert_node(&mut self, node: GraphNode) -> Result<()> {
        if self.nodes.contains_key(&node.id) {
            return Err(Error::new(
                ErrorCode::InvalidGraph,
                format!("duplicate node {:?}", node.id),
            ));
        }
        self.nodes.insert(node.id, node);
        Ok(())
    }

    pub fn insert_edge(&mut self, edge: GraphEdge) -> Result<()> {
        if !self.nodes.contains_key(&edge.from) || !self.nodes.contains_key(&edge.to) {
            return Err(Error::new(
                ErrorCode::InvalidGraph,
                "edge references missing node",
            ));
        }
        self.edges.push(edge);
        Ok(())
    }

    pub fn insert_device_dependency(
        &mut self,
        provider: NodeId,
        consumer: NodeId,
        role: Role,
    ) -> Result<()> {
        self.insert_edge(GraphEdge {
            from: provider,
            to: consumer,
            kind: EdgeKind::UsesDevice { role },
        })
    }

    pub fn nodes(&self) -> impl Iterator<Item = &GraphNode> {
        self.nodes.values()
    }

    pub fn edges(&self) -> &[GraphEdge] {
        &self.edges
    }

    pub fn initialization_order(&self) -> Result<Vec<NodeId>> {
        let mut indegree: BTreeMap<NodeId, usize> = self.nodes.keys().map(|id| (*id, 0)).collect();
        let mut outgoing: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for edge in &self.edges {
            *indegree.entry(edge.to).or_default() += 1;
            outgoing.entry(edge.from).or_default().push(edge.to);
        }

        let mut ready: VecDeque<NodeId> = indegree
            .iter()
            .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
            .collect();
        let mut order = Vec::with_capacity(self.nodes.len());

        while let Some(id) = ready.pop_front() {
            order.push(id);
            if let Some(children) = outgoing.get(&id) {
                for child in children {
                    let degree = indegree.get_mut(child).expect("known child");
                    *degree -= 1;
                    if *degree == 0 {
                        ready.push_back(*child);
                    }
                }
            }
        }

        if order.len() != self.nodes.len() {
            return Err(Error::new(
                ErrorCode::InvalidGraph,
                "graph contains a cycle",
            ));
        }
        Ok(order)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    I64(i64),
    F64(f64),
    Temperature(Temperature),
    Position(Position),
    Velocity(Velocity),
    Acceleration(Acceleration),
    TimeInterval(TimeInterval),
    Wavelength(Wavelength),
    OpticalPower(OpticalPower),
    ElectricCurrent(ElectricCurrent),
    Voltage(Voltage),
    Frequency(Frequency),
    Decibel(Decibel),
    PixelCount(PixelCount),
    ByteCount(ByteCount),
    StepCount(StepCount),
    ControllerScalar(ControllerScalar),
    Ratio(Ratio),
    NumericalAperture(NumericalAperture),
    Timestamp(Timestamp),
    Pressure(Pressure),
    GasConcentration(GasConcentration),
    FlowRate(FlowRate),
    String(String),
    Bytes(Vec<u8>),
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
    Null,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    Bool,
    I64,
    F64,
    Temperature,
    Position,
    Velocity,
    Acceleration,
    TimeInterval,
    Wavelength,
    OpticalPower,
    ElectricCurrent,
    Voltage,
    Frequency,
    Decibel,
    PixelCount,
    ByteCount,
    StepCount,
    ControllerScalar,
    Ratio,
    NumericalAperture,
    Timestamp,
    Pressure,
    GasConcentration,
    FlowRate,
    String,
    Bytes,
    List,
    Map,
    Null,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Temperature {
    pub value: f64,
    pub unit: TemperatureUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemperatureUnit {
    Celsius,
    Kelvin,
    Fahrenheit,
}

impl Temperature {
    pub fn from_kelvin(kelvin: f64) -> Self {
        Self {
            value: kelvin,
            unit: TemperatureUnit::Kelvin,
        }
    }

    pub fn from_celsius(celsius: f64) -> Self {
        Self {
            value: celsius,
            unit: TemperatureUnit::Celsius,
        }
    }

    pub fn from_fahrenheit(fahrenheit: f64) -> Self {
        Self {
            value: fahrenheit,
            unit: TemperatureUnit::Fahrenheit,
        }
    }

    pub fn kelvin(self) -> f64 {
        match self.unit {
            TemperatureUnit::Kelvin => self.value,
            TemperatureUnit::Celsius => self.value + 273.15,
            TemperatureUnit::Fahrenheit => (self.value - 32.0) * 5.0 / 9.0 + 273.15,
        }
    }

    pub fn celsius(self) -> f64 {
        match self.unit {
            TemperatureUnit::Celsius => self.value,
            TemperatureUnit::Kelvin => self.value - 273.15,
            TemperatureUnit::Fahrenheit => (self.value - 32.0) * 5.0 / 9.0,
        }
    }

    pub fn fahrenheit(self) -> f64 {
        match self.unit {
            TemperatureUnit::Fahrenheit => self.value,
            TemperatureUnit::Celsius => self.value * 9.0 / 5.0 + 32.0,
            TemperatureUnit::Kelvin => (self.value - 273.15) * 9.0 / 5.0 + 32.0,
        }
    }

    pub fn unit_symbol(self) -> &'static str {
        match self.unit {
            TemperatureUnit::Celsius => "degC",
            TemperatureUnit::Kelvin => "K",
            TemperatureUnit::Fahrenheit => "degF",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub value: f64,
    pub unit: PositionUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionUnit {
    Meters,
    Millimeters,
    Micrometers,
}

impl Position {
    pub fn from_meters(meters: f64) -> Self {
        Self {
            value: meters,
            unit: PositionUnit::Meters,
        }
    }

    pub fn from_millimeters(millimeters: f64) -> Self {
        Self {
            value: millimeters,
            unit: PositionUnit::Millimeters,
        }
    }

    pub fn from_micrometers(micrometers: f64) -> Self {
        Self {
            value: micrometers,
            unit: PositionUnit::Micrometers,
        }
    }

    pub fn meters(self) -> f64 {
        match self.unit {
            PositionUnit::Meters => self.value,
            PositionUnit::Millimeters => self.value * 1e-3,
            PositionUnit::Micrometers => self.value * 1e-6,
        }
    }

    pub fn micrometers(self) -> f64 {
        match self.unit {
            PositionUnit::Meters => self.value * 1e6,
            PositionUnit::Millimeters => self.value * 1e3,
            PositionUnit::Micrometers => self.value,
        }
    }

    pub fn unit_symbol(self) -> &'static str {
        match self.unit {
            PositionUnit::Meters => "m",
            PositionUnit::Millimeters => "mm",
            PositionUnit::Micrometers => "um",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Velocity {
    pub value: f64,
    pub unit: VelocityUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VelocityUnit {
    MetersPerSecond,
    MillimetersPerSecond,
    MicrometersPerSecond,
}

impl Velocity {
    pub fn from_meters_per_second(value: f64) -> Self {
        Self {
            value,
            unit: VelocityUnit::MetersPerSecond,
        }
    }

    pub fn from_millimeters_per_second(value: f64) -> Self {
        Self {
            value,
            unit: VelocityUnit::MillimetersPerSecond,
        }
    }

    pub fn from_micrometers_per_second(value: f64) -> Self {
        Self {
            value,
            unit: VelocityUnit::MicrometersPerSecond,
        }
    }

    pub fn meters_per_second(self) -> f64 {
        match self.unit {
            VelocityUnit::MetersPerSecond => self.value,
            VelocityUnit::MillimetersPerSecond => self.value * 1e-3,
            VelocityUnit::MicrometersPerSecond => self.value * 1e-6,
        }
    }

    pub fn micrometers_per_second(self) -> f64 {
        match self.unit {
            VelocityUnit::MetersPerSecond => self.value * 1e6,
            VelocityUnit::MillimetersPerSecond => self.value * 1e3,
            VelocityUnit::MicrometersPerSecond => self.value,
        }
    }

    pub fn unit_symbol(self) -> &'static str {
        match self.unit {
            VelocityUnit::MetersPerSecond => "m/s",
            VelocityUnit::MillimetersPerSecond => "mm/s",
            VelocityUnit::MicrometersPerSecond => "um/s",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Acceleration {
    pub value: f64,
    pub unit: AccelerationUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccelerationUnit {
    MetersPerSecondSquared,
    MillimetersPerSecondSquared,
    MicrometersPerSecondSquared,
}

impl Acceleration {
    pub fn from_meters_per_second_squared(value: f64) -> Self {
        Self {
            value,
            unit: AccelerationUnit::MetersPerSecondSquared,
        }
    }

    pub fn from_millimeters_per_second_squared(value: f64) -> Self {
        Self {
            value,
            unit: AccelerationUnit::MillimetersPerSecondSquared,
        }
    }

    pub fn from_micrometers_per_second_squared(value: f64) -> Self {
        Self {
            value,
            unit: AccelerationUnit::MicrometersPerSecondSquared,
        }
    }

    pub fn meters_per_second_squared(self) -> f64 {
        match self.unit {
            AccelerationUnit::MetersPerSecondSquared => self.value,
            AccelerationUnit::MillimetersPerSecondSquared => self.value * 1e-3,
            AccelerationUnit::MicrometersPerSecondSquared => self.value * 1e-6,
        }
    }

    pub fn micrometers_per_second_squared(self) -> f64 {
        match self.unit {
            AccelerationUnit::MetersPerSecondSquared => self.value * 1e6,
            AccelerationUnit::MillimetersPerSecondSquared => self.value * 1e3,
            AccelerationUnit::MicrometersPerSecondSquared => self.value,
        }
    }

    pub fn unit_symbol(self) -> &'static str {
        match self.unit {
            AccelerationUnit::MetersPerSecondSquared => "m/s^2",
            AccelerationUnit::MillimetersPerSecondSquared => "mm/s^2",
            AccelerationUnit::MicrometersPerSecondSquared => "um/s^2",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeInterval {
    pub value: f64,
    pub unit: TimeIntervalUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeIntervalUnit {
    Hours,
    Seconds,
    Milliseconds,
    Microseconds,
    Nanoseconds,
    ControllerTicks,
}

impl TimeInterval {
    pub fn from_hours(value: f64) -> Self {
        Self {
            value,
            unit: TimeIntervalUnit::Hours,
        }
    }

    pub fn from_seconds(value: f64) -> Self {
        Self {
            value,
            unit: TimeIntervalUnit::Seconds,
        }
    }

    pub fn from_milliseconds(value: f64) -> Self {
        Self {
            value,
            unit: TimeIntervalUnit::Milliseconds,
        }
    }

    pub fn from_microseconds(value: f64) -> Self {
        Self {
            value,
            unit: TimeIntervalUnit::Microseconds,
        }
    }

    pub fn from_nanoseconds(value: f64) -> Self {
        Self {
            value,
            unit: TimeIntervalUnit::Nanoseconds,
        }
    }

    pub fn from_controller_ticks(value: f64) -> Self {
        Self {
            value,
            unit: TimeIntervalUnit::ControllerTicks,
        }
    }

    pub fn seconds(self) -> f64 {
        match self.unit {
            TimeIntervalUnit::Hours => self.value * 3600.0,
            TimeIntervalUnit::Seconds => self.value,
            TimeIntervalUnit::Milliseconds => self.value * 1e-3,
            TimeIntervalUnit::Microseconds => self.value * 1e-6,
            TimeIntervalUnit::Nanoseconds => self.value * 1e-9,
            // Protocol-native delay ticks have no documented SI conversion.
            // Keep the raw tick count for range comparisons and defer physical
            // conversion until a device-specific timing spec exists.
            TimeIntervalUnit::ControllerTicks => self.value,
        }
    }

    pub fn hours(self) -> f64 {
        self.seconds() / 3600.0
    }

    pub fn microseconds(self) -> f64 {
        match self.unit {
            TimeIntervalUnit::Hours => self.value * 3.6e9,
            TimeIntervalUnit::Seconds => self.value * 1e6,
            TimeIntervalUnit::Milliseconds => self.value * 1e3,
            TimeIntervalUnit::Microseconds => self.value,
            TimeIntervalUnit::Nanoseconds => self.value * 1e-3,
            TimeIntervalUnit::ControllerTicks => self.value,
        }
    }

    pub fn nanoseconds(self) -> f64 {
        match self.unit {
            TimeIntervalUnit::Hours => self.value * 3.6e12,
            TimeIntervalUnit::Seconds => self.value * 1e9,
            TimeIntervalUnit::Milliseconds => self.value * 1e6,
            TimeIntervalUnit::Microseconds => self.value * 1e3,
            TimeIntervalUnit::Nanoseconds => self.value,
            TimeIntervalUnit::ControllerTicks => self.value,
        }
    }

    pub fn unit_symbol(self) -> &'static str {
        match self.unit {
            TimeIntervalUnit::Hours => "h",
            TimeIntervalUnit::Seconds => "s",
            TimeIntervalUnit::Milliseconds => "ms",
            TimeIntervalUnit::Microseconds => "us",
            TimeIntervalUnit::Nanoseconds => "ns",
            TimeIntervalUnit::ControllerTicks => "controller_tick",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wavelength {
    pub value: f64,
    pub unit: WavelengthUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WavelengthUnit {
    Meters,
    Millimeters,
    Micrometers,
    Nanometers,
    Angstroms,
}

impl Wavelength {
    pub fn from_meters(meters: f64) -> Self {
        Self {
            value: meters,
            unit: WavelengthUnit::Meters,
        }
    }

    pub fn from_micrometers(micrometers: f64) -> Self {
        Self {
            value: micrometers,
            unit: WavelengthUnit::Micrometers,
        }
    }

    pub fn from_nanometers(nanometers: f64) -> Self {
        Self {
            value: nanometers,
            unit: WavelengthUnit::Nanometers,
        }
    }

    pub fn meters(self) -> f64 {
        match self.unit {
            WavelengthUnit::Meters => self.value,
            WavelengthUnit::Millimeters => self.value * 1e-3,
            WavelengthUnit::Micrometers => self.value * 1e-6,
            WavelengthUnit::Nanometers => self.value * 1e-9,
            WavelengthUnit::Angstroms => self.value * 1e-10,
        }
    }

    pub fn nanometers(self) -> f64 {
        match self.unit {
            WavelengthUnit::Meters => self.value * 1e9,
            WavelengthUnit::Millimeters => self.value * 1e6,
            WavelengthUnit::Micrometers => self.value * 1e3,
            WavelengthUnit::Nanometers => self.value,
            WavelengthUnit::Angstroms => self.value * 0.1,
        }
    }

    pub fn unit_symbol(self) -> &'static str {
        match self.unit {
            WavelengthUnit::Meters => "m",
            WavelengthUnit::Millimeters => "mm",
            WavelengthUnit::Micrometers => "um",
            WavelengthUnit::Nanometers => "nm",
            WavelengthUnit::Angstroms => "angstrom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpticalPower {
    pub value: f64,
    pub unit: OpticalPowerUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpticalPowerUnit {
    Watts,
    Milliwatts,
    Microwatts,
}

impl OpticalPower {
    pub fn from_watts(watts: f64) -> Self {
        Self {
            value: watts,
            unit: OpticalPowerUnit::Watts,
        }
    }

    pub fn from_milliwatts(milliwatts: f64) -> Self {
        Self {
            value: milliwatts,
            unit: OpticalPowerUnit::Milliwatts,
        }
    }

    pub fn from_microwatts(microwatts: f64) -> Self {
        Self {
            value: microwatts,
            unit: OpticalPowerUnit::Microwatts,
        }
    }

    pub fn watts(self) -> f64 {
        match self.unit {
            OpticalPowerUnit::Watts => self.value,
            OpticalPowerUnit::Milliwatts => self.value * 1e-3,
            OpticalPowerUnit::Microwatts => self.value * 1e-6,
        }
    }

    pub fn milliwatts(self) -> f64 {
        match self.unit {
            OpticalPowerUnit::Watts => self.value * 1e3,
            OpticalPowerUnit::Milliwatts => self.value,
            OpticalPowerUnit::Microwatts => self.value * 1e-3,
        }
    }

    pub fn unit_symbol(self) -> &'static str {
        match self.unit {
            OpticalPowerUnit::Watts => "W",
            OpticalPowerUnit::Milliwatts => "mW",
            OpticalPowerUnit::Microwatts => "uW",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElectricCurrent {
    pub value: f64,
    pub unit: ElectricCurrentUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElectricCurrentUnit {
    Amps,
    Milliamps,
    Microamps,
}

impl ElectricCurrent {
    pub fn from_amps(amps: f64) -> Self {
        Self {
            value: amps,
            unit: ElectricCurrentUnit::Amps,
        }
    }

    pub fn from_milliamps(milliamps: f64) -> Self {
        Self {
            value: milliamps,
            unit: ElectricCurrentUnit::Milliamps,
        }
    }

    pub fn from_microamps(microamps: f64) -> Self {
        Self {
            value: microamps,
            unit: ElectricCurrentUnit::Microamps,
        }
    }

    pub fn amps(self) -> f64 {
        match self.unit {
            ElectricCurrentUnit::Amps => self.value,
            ElectricCurrentUnit::Milliamps => self.value * 1e-3,
            ElectricCurrentUnit::Microamps => self.value * 1e-6,
        }
    }

    pub fn milliamps(self) -> f64 {
        match self.unit {
            ElectricCurrentUnit::Amps => self.value * 1e3,
            ElectricCurrentUnit::Milliamps => self.value,
            ElectricCurrentUnit::Microamps => self.value * 1e-3,
        }
    }

    pub fn unit_symbol(self) -> &'static str {
        match self.unit {
            ElectricCurrentUnit::Amps => "A",
            ElectricCurrentUnit::Milliamps => "mA",
            ElectricCurrentUnit::Microamps => "uA",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Voltage {
    pub value: f64,
    pub unit: VoltageUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoltageUnit {
    Volts,
    Millivolts,
    Microvolts,
}

impl Voltage {
    pub fn from_volts(value: f64) -> Self {
        Self {
            value,
            unit: VoltageUnit::Volts,
        }
    }

    pub fn from_millivolts(value: f64) -> Self {
        Self {
            value,
            unit: VoltageUnit::Millivolts,
        }
    }

    pub fn from_microvolts(value: f64) -> Self {
        Self {
            value,
            unit: VoltageUnit::Microvolts,
        }
    }

    pub fn volts(self) -> f64 {
        match self.unit {
            VoltageUnit::Volts => self.value,
            VoltageUnit::Millivolts => self.value * 1e-3,
            VoltageUnit::Microvolts => self.value * 1e-6,
        }
    }

    pub fn millivolts(self) -> f64 {
        match self.unit {
            VoltageUnit::Volts => self.value * 1e3,
            VoltageUnit::Millivolts => self.value,
            VoltageUnit::Microvolts => self.value * 1e-3,
        }
    }

    pub fn unit_symbol(self) -> &'static str {
        match self.unit {
            VoltageUnit::Volts => "V",
            VoltageUnit::Millivolts => "mV",
            VoltageUnit::Microvolts => "uV",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frequency {
    pub value: f64,
    pub unit: FrequencyUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrequencyUnit {
    Hertz,
    Kilohertz,
    Megahertz,
}

impl Frequency {
    pub fn from_hertz(value: f64) -> Self {
        Self {
            value,
            unit: FrequencyUnit::Hertz,
        }
    }

    pub fn from_kilohertz(value: f64) -> Self {
        Self {
            value,
            unit: FrequencyUnit::Kilohertz,
        }
    }

    pub fn from_megahertz(value: f64) -> Self {
        Self {
            value,
            unit: FrequencyUnit::Megahertz,
        }
    }

    pub fn hertz(self) -> f64 {
        match self.unit {
            FrequencyUnit::Hertz => self.value,
            FrequencyUnit::Kilohertz => self.value * 1e3,
            FrequencyUnit::Megahertz => self.value * 1e6,
        }
    }

    pub fn kilohertz(self) -> f64 {
        self.hertz() * 1e-3
    }

    pub fn unit_symbol(self) -> &'static str {
        match self.unit {
            FrequencyUnit::Hertz => "Hz",
            FrequencyUnit::Kilohertz => "kHz",
            FrequencyUnit::Megahertz => "MHz",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decibel {
    pub value: f64,
}

impl Decibel {
    pub fn new(value: f64) -> Self {
        Self { value }
    }

    pub fn db(self) -> f64 {
        self.value
    }

    pub fn unit_symbol(self) -> &'static str {
        "dB"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PixelCount(pub u32);

impl PixelCount {
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    pub fn pixels(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ByteCount(pub u64);

impl ByteCount {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn bytes(self) -> u64 {
        self.0
    }

    pub fn unit_symbol(self) -> &'static str {
        "bytes"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct StepCount(pub i64);

impl StepCount {
    pub fn new(value: i64) -> Self {
        Self(value)
    }

    pub fn steps(self) -> i64 {
        self.0
    }

    pub fn unit_symbol(self) -> &'static str {
        "steps"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ControllerScalar(pub i64);

impl ControllerScalar {
    pub fn new(value: i64) -> Self {
        Self(value)
    }

    pub fn value(self) -> i64 {
        self.0
    }

    pub fn unit_symbol(self) -> &'static str {
        "controller_step"
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ratio {
    pub value: f64,
    pub unit: RatioUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatioUnit {
    Fraction,
    Percent,
}

impl Ratio {
    pub fn from_fraction(value: f64) -> Self {
        Self {
            value,
            unit: RatioUnit::Fraction,
        }
    }

    pub fn from_percent(value: f64) -> Self {
        Self {
            value,
            unit: RatioUnit::Percent,
        }
    }

    pub fn fraction(self) -> f64 {
        match self.unit {
            RatioUnit::Fraction => self.value,
            RatioUnit::Percent => self.value / 100.0,
        }
    }

    pub fn percent(self) -> f64 {
        match self.unit {
            RatioUnit::Fraction => self.value * 100.0,
            RatioUnit::Percent => self.value,
        }
    }

    pub fn unit_symbol(self) -> &'static str {
        match self.unit {
            RatioUnit::Fraction => "fraction",
            RatioUnit::Percent => "percent",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NumericalAperture {
    pub value: f64,
}

impl NumericalAperture {
    pub fn new(value: f64) -> Self {
        Self { value }
    }

    pub fn value(self) -> f64 {
        self.value
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timestamp {
    pub value: i64,
    pub unit: TimestampUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampUnit {
    ControllerTicks,
}

impl Timestamp {
    pub fn from_controller_ticks(value: i64) -> Self {
        Self {
            value,
            unit: TimestampUnit::ControllerTicks,
        }
    }

    pub fn ticks(self) -> i64 {
        self.value
    }

    pub fn unit_symbol(self) -> &'static str {
        match self.unit {
            TimestampUnit::ControllerTicks => "controller_tick",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pressure {
    pub value: f64,
    pub unit: PressureUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressureUnit {
    Pascals,
    Kilopascals,
    Bar,
    Millibar,
    Psi,
}

impl Pressure {
    pub fn from_pascals(value: f64) -> Self {
        Self {
            value,
            unit: PressureUnit::Pascals,
        }
    }

    pub fn from_kilopascals(value: f64) -> Self {
        Self {
            value,
            unit: PressureUnit::Kilopascals,
        }
    }

    pub fn from_bar(value: f64) -> Self {
        Self {
            value,
            unit: PressureUnit::Bar,
        }
    }

    pub fn from_millibar(value: f64) -> Self {
        Self {
            value,
            unit: PressureUnit::Millibar,
        }
    }

    pub fn from_psi(value: f64) -> Self {
        Self {
            value,
            unit: PressureUnit::Psi,
        }
    }

    pub fn pascals(self) -> f64 {
        match self.unit {
            PressureUnit::Pascals => self.value,
            PressureUnit::Kilopascals => self.value * 1e3,
            PressureUnit::Bar => self.value * 100_000.0,
            PressureUnit::Millibar => self.value * 100.0,
            PressureUnit::Psi => self.value * 6_894.757_293_168,
        }
    }

    pub fn millibar(self) -> f64 {
        self.pascals() / 100.0
    }

    pub fn unit_symbol(self) -> &'static str {
        match self.unit {
            PressureUnit::Pascals => "Pa",
            PressureUnit::Kilopascals => "kPa",
            PressureUnit::Bar => "bar",
            PressureUnit::Millibar => "mbar",
            PressureUnit::Psi => "psi",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GasConcentration {
    pub value: f64,
    pub unit: GasConcentrationUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GasConcentrationUnit {
    Fraction,
    Percent,
    Ppm,
}

impl GasConcentration {
    pub fn from_fraction(value: f64) -> Self {
        Self {
            value,
            unit: GasConcentrationUnit::Fraction,
        }
    }

    pub fn from_percent(value: f64) -> Self {
        Self {
            value,
            unit: GasConcentrationUnit::Percent,
        }
    }

    pub fn from_ppm(value: f64) -> Self {
        Self {
            value,
            unit: GasConcentrationUnit::Ppm,
        }
    }

    pub fn fraction(self) -> f64 {
        match self.unit {
            GasConcentrationUnit::Fraction => self.value,
            GasConcentrationUnit::Percent => self.value / 100.0,
            GasConcentrationUnit::Ppm => self.value * 1e-6,
        }
    }

    pub fn percent(self) -> f64 {
        match self.unit {
            GasConcentrationUnit::Fraction => self.value * 100.0,
            GasConcentrationUnit::Percent => self.value,
            GasConcentrationUnit::Ppm => self.value * 1e-4,
        }
    }

    pub fn unit_symbol(self) -> &'static str {
        match self.unit {
            GasConcentrationUnit::Fraction => "fraction",
            GasConcentrationUnit::Percent => "percent",
            GasConcentrationUnit::Ppm => "ppm",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlowRate {
    pub value: f64,
    pub unit: FlowRateUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowRateUnit {
    LitersPerMinute,
    MillilitersPerMinute,
    MicrolitersPerMinute,
    StandardCubicCentimetersPerMinute,
}

impl FlowRate {
    pub fn from_liters_per_minute(value: f64) -> Self {
        Self {
            value,
            unit: FlowRateUnit::LitersPerMinute,
        }
    }

    pub fn from_milliliters_per_minute(value: f64) -> Self {
        Self {
            value,
            unit: FlowRateUnit::MillilitersPerMinute,
        }
    }

    pub fn from_microliters_per_minute(value: f64) -> Self {
        Self {
            value,
            unit: FlowRateUnit::MicrolitersPerMinute,
        }
    }

    pub fn from_standard_cubic_centimeters_per_minute(value: f64) -> Self {
        Self {
            value,
            unit: FlowRateUnit::StandardCubicCentimetersPerMinute,
        }
    }

    pub fn milliliters_per_minute(self) -> f64 {
        match self.unit {
            FlowRateUnit::LitersPerMinute => self.value * 1e3,
            FlowRateUnit::MillilitersPerMinute => self.value,
            FlowRateUnit::MicrolitersPerMinute => self.value * 1e-3,
            FlowRateUnit::StandardCubicCentimetersPerMinute => self.value,
        }
    }

    pub fn microliters_per_minute(self) -> f64 {
        self.milliliters_per_minute() * 1e3
    }

    pub fn unit_symbol(self) -> &'static str {
        match self.unit {
            FlowRateUnit::LitersPerMinute => "L/min",
            FlowRateUnit::MillilitersPerMinute => "mL/min",
            FlowRateUnit::MicrolitersPerMinute => "uL/min",
            FlowRateUnit::StandardCubicCentimetersPerMinute => "sccm",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Range {
    pub min: Value,
    pub max: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumValue {
    pub value: Value,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit(pub String);

#[derive(Debug, Clone)]
pub struct PropertySchema {
    pub key: String,
    pub display_name: String,
    pub value_type: ValueType,
    pub unit: Option<Unit>,
    pub range: Option<Range>,
    pub increment: Option<Value>,
    pub enum_values: Vec<EnumValue>,
    pub readable: bool,
    pub writable: bool,
    pub volatile: bool,
    pub sequenceable: bool,
    pub hardware_address: Option<String>,
}

impl PropertySchema {
    pub fn validate(&self, value: &Value) -> Result<()> {
        let actual = match value {
            Value::Bool(_) => ValueType::Bool,
            Value::I64(_) => ValueType::I64,
            Value::F64(_) => ValueType::F64,
            Value::Temperature(_) => ValueType::Temperature,
            Value::Position(_) => ValueType::Position,
            Value::Velocity(_) => ValueType::Velocity,
            Value::Acceleration(_) => ValueType::Acceleration,
            Value::TimeInterval(_) => ValueType::TimeInterval,
            Value::Wavelength(_) => ValueType::Wavelength,
            Value::OpticalPower(_) => ValueType::OpticalPower,
            Value::ElectricCurrent(_) => ValueType::ElectricCurrent,
            Value::Voltage(_) => ValueType::Voltage,
            Value::Frequency(_) => ValueType::Frequency,
            Value::Decibel(_) => ValueType::Decibel,
            Value::PixelCount(_) => ValueType::PixelCount,
            Value::ByteCount(_) => ValueType::ByteCount,
            Value::StepCount(_) => ValueType::StepCount,
            Value::ControllerScalar(_) => ValueType::ControllerScalar,
            Value::Ratio(_) => ValueType::Ratio,
            Value::NumericalAperture(_) => ValueType::NumericalAperture,
            Value::Timestamp(_) => ValueType::Timestamp,
            Value::Pressure(_) => ValueType::Pressure,
            Value::GasConcentration(_) => ValueType::GasConcentration,
            Value::FlowRate(_) => ValueType::FlowRate,
            Value::String(_) => ValueType::String,
            Value::Bytes(_) => ValueType::Bytes,
            Value::List(_) => ValueType::List,
            Value::Map(_) => ValueType::Map,
            Value::Null => {
                return Err(Error::new(
                    ErrorCode::InvalidProperty,
                    format!("property {} cannot be null", self.key),
                ));
            }
        };
        if actual != self.value_type {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("property {} expected {:?}", self.key, self.value_type),
            ));
        }
        if !self.enum_values.is_empty() && !self.enum_values.iter().any(|v| &v.value == value) {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("property {} value is not in enum", self.key),
            ));
        }
        if let Some(range) = &self.range {
            self.validate_range(value, range)?;
        }
        if let Some(increment) = &self.increment {
            self.validate_increment(value, increment)?;
        }
        Ok(())
    }

    fn validate_range(&self, value: &Value, range: &Range) -> Result<()> {
        let Some(value) = canonical_range_value(value) else {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("property {} does not support range validation", self.key),
            ));
        };
        let Some(min) = canonical_range_value(&range.min) else {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("property {} has non-numeric range minimum", self.key),
            ));
        };
        let Some(max) = canonical_range_value(&range.max) else {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("property {} has non-numeric range maximum", self.key),
            ));
        };
        if range.min.value_type() != self.value_type || range.max.value_type() != self.value_type {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("property {} range type does not match schema", self.key),
            ));
        }
        if !value.is_finite() || !min.is_finite() || !max.is_finite() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("property {} range value is not finite", self.key),
            ));
        }
        if min > max {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("property {} range minimum exceeds maximum", self.key),
            ));
        }
        if value < min || value > max {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("property {} value is outside advertised range", self.key),
            ));
        }
        Ok(())
    }

    fn validate_increment(&self, value: &Value, increment: &Value) -> Result<()> {
        if increment.value_type() != self.value_type {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("property {} increment type does not match schema", self.key),
            ));
        }
        let Some(value) = canonical_range_value(value) else {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!(
                    "property {} does not support increment validation",
                    self.key
                ),
            ));
        };
        let Some(increment) = canonical_range_value(increment) else {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("property {} has non-numeric increment", self.key),
            ));
        };
        if !value.is_finite() || !increment.is_finite() || increment <= 0.0 {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("property {} increment value is invalid", self.key),
            ));
        }
        let base = self
            .range
            .as_ref()
            .and_then(|range| canonical_range_value(&range.min))
            .unwrap_or(0.0);
        if !base.is_finite() {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!("property {} increment base is invalid", self.key),
            ));
        }
        let steps = (value - base) / increment;
        let tolerance = 1e-9_f64.max(steps.abs() * 1e-9);
        if (steps - steps.round()).abs() > tolerance {
            return Err(Error::new(
                ErrorCode::InvalidProperty,
                format!(
                    "property {} value does not match advertised increment",
                    self.key
                ),
            ));
        }
        Ok(())
    }
}

impl Value {
    pub fn value_type(&self) -> ValueType {
        match self {
            Value::Bool(_) => ValueType::Bool,
            Value::I64(_) => ValueType::I64,
            Value::F64(_) => ValueType::F64,
            Value::Temperature(_) => ValueType::Temperature,
            Value::Position(_) => ValueType::Position,
            Value::Velocity(_) => ValueType::Velocity,
            Value::Acceleration(_) => ValueType::Acceleration,
            Value::TimeInterval(_) => ValueType::TimeInterval,
            Value::Wavelength(_) => ValueType::Wavelength,
            Value::OpticalPower(_) => ValueType::OpticalPower,
            Value::ElectricCurrent(_) => ValueType::ElectricCurrent,
            Value::Voltage(_) => ValueType::Voltage,
            Value::Frequency(_) => ValueType::Frequency,
            Value::Decibel(_) => ValueType::Decibel,
            Value::PixelCount(_) => ValueType::PixelCount,
            Value::ByteCount(_) => ValueType::ByteCount,
            Value::StepCount(_) => ValueType::StepCount,
            Value::ControllerScalar(_) => ValueType::ControllerScalar,
            Value::Ratio(_) => ValueType::Ratio,
            Value::NumericalAperture(_) => ValueType::NumericalAperture,
            Value::Timestamp(_) => ValueType::Timestamp,
            Value::Pressure(_) => ValueType::Pressure,
            Value::GasConcentration(_) => ValueType::GasConcentration,
            Value::FlowRate(_) => ValueType::FlowRate,
            Value::String(_) => ValueType::String,
            Value::Bytes(_) => ValueType::Bytes,
            Value::List(_) => ValueType::List,
            Value::Map(_) => ValueType::Map,
            Value::Null => ValueType::Null,
        }
    }
}

fn canonical_range_value(value: &Value) -> Option<f64> {
    match value {
        Value::I64(value) => Some(*value as f64),
        Value::F64(value) => Some(*value),
        Value::Temperature(value) => Some(value.kelvin()),
        Value::Position(value) => Some(value.meters()),
        Value::Velocity(value) => Some(value.meters_per_second()),
        Value::Acceleration(value) => Some(value.meters_per_second_squared()),
        Value::TimeInterval(value) => Some(value.seconds()),
        Value::Wavelength(value) => Some(value.meters()),
        Value::OpticalPower(value) => Some(value.watts()),
        Value::ElectricCurrent(value) => Some(value.amps()),
        Value::Voltage(value) => Some(value.volts()),
        Value::Frequency(value) => Some(value.hertz()),
        Value::Decibel(value) => Some(value.db()),
        Value::PixelCount(value) => Some(value.pixels() as f64),
        Value::ByteCount(value) => Some(value.bytes() as f64),
        Value::StepCount(value) => Some(value.steps() as f64),
        Value::ControllerScalar(value) => Some(value.value() as f64),
        Value::Ratio(value) => Some(value.fraction()),
        Value::NumericalAperture(value) => Some(value.value()),
        Value::Timestamp(value) => Some(value.ticks() as f64),
        Value::Pressure(value) => Some(value.pascals()),
        Value::GasConcentration(value) => Some(value.fraction()),
        Value::FlowRate(value) => Some(value.milliliters_per_minute()),
        Value::Bool(_)
        | Value::String(_)
        | Value::Bytes(_)
        | Value::List(_)
        | Value::Map(_)
        | Value::Null => None,
    }
}

#[derive(Debug, Clone)]
pub struct DeviceDescriptor {
    pub id: DeviceId,
    pub driver: DriverId,
    pub label: String,
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub kinds: Vec<String>,
    pub properties: Vec<PropertySchema>,
    pub metadata: BTreeMap<String, Value>,
}

impl DeviceDescriptor {
    pub fn has_kind(&self, kind: &str) -> bool {
        self.kinds.iter().any(|candidate| candidate == kind)
    }

    pub fn has_kinds(&self, kinds: &[&str]) -> bool {
        kinds.iter().all(|kind| self.has_kind(kind))
    }
}

impl From<&DeviceDescriptor> for DeviceId {
    fn from(device: &DeviceDescriptor) -> Self {
        device.id
    }
}

impl From<DeviceDescriptor> for DeviceId {
    fn from(device: DeviceDescriptor) -> Self {
        device.id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyState {
    Safe,
    Active,
    Interlocked,
    Fault,
    Unknown,
}

impl SafetyState {
    pub fn name(self) -> &'static str {
        match self {
            SafetyState::Safe => "safe",
            SafetyState::Active => "active",
            SafetyState::Interlocked => "interlocked",
            SafetyState::Fault => "fault",
            SafetyState::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SafetySummary {
    pub device: DeviceId,
    pub state: SafetyState,
    pub enabled: Option<bool>,
    pub interlock_closed: Option<bool>,
    pub emission_permitted: Option<bool>,
    pub fault_active: Option<bool>,
    pub fault: Option<String>,
    pub values: BTreeMap<String, Value>,
}

impl SafetySummary {
    pub fn from_values(device: DeviceId, values: BTreeMap<String, Value>) -> Self {
        let enabled = bool_value(&values, "enabled").or_else(|| bool_value(&values, "emission"));
        let interlock_closed = bool_value(&values, "interlock_closed");
        let emission_permitted = bool_value(&values, "emission_permitted");
        let fault_active = bool_value(&values, "fault_active")
            .or_else(|| bool_value(&values, "interlock_fault"))
            .or_else(|| bool_value(&values, "overtemperature_fault"))
            .or_else(|| bool_value(&values, "gas_fault"))
            .or_else(|| bool_value(&values, "overpressure_fault"))
            .or_else(|| bool_value(&values, "fault"));
        let fault = string_value(&values, "fault")
            .or_else(|| string_value(&values, "fault_flags"))
            .or_else(|| string_value(&values, "status"));

        let state = if fault_active == Some(true) || fault_text_is_active(fault.as_deref()) {
            SafetyState::Fault
        } else if interlock_closed == Some(false) || emission_permitted == Some(false) {
            SafetyState::Interlocked
        } else if enabled == Some(true) {
            SafetyState::Active
        } else if values.is_empty() {
            SafetyState::Unknown
        } else {
            SafetyState::Safe
        };

        Self {
            device,
            state,
            enabled,
            interlock_closed,
            emission_permitted,
            fault_active,
            fault,
            values,
        }
    }

    pub fn property_key_is_safety(key: &str) -> bool {
        matches!(
            key,
            "enabled"
                | "emission"
                | "emission_request"
                | "emission_permitted"
                | "interlock_closed"
                | "interlock_fault"
                | "overtemperature_fault"
                | "overpressure_fault"
                | "gas_fault"
                | "fault"
                | "fault_active"
                | "fault_bits"
                | "fault_code"
                | "fault_flags"
                | "status"
        )
    }

    pub fn as_value(&self) -> Value {
        let mut values = self.values.clone();
        values.insert("state".into(), Value::String(self.state.name().into()));
        values.insert("device".into(), Value::I64(self.device.0 .0 as i64));
        if let Some(enabled) = self.enabled {
            values.insert("enabled".into(), Value::Bool(enabled));
        }
        if let Some(interlock_closed) = self.interlock_closed {
            values.insert("interlock_closed".into(), Value::Bool(interlock_closed));
        }
        if let Some(emission_permitted) = self.emission_permitted {
            values.insert("emission_permitted".into(), Value::Bool(emission_permitted));
        }
        if let Some(fault_active) = self.fault_active {
            values.insert("fault_active".into(), Value::Bool(fault_active));
        }
        if let Some(fault) = &self.fault {
            values.insert("fault".into(), Value::String(fault.clone()));
        }
        Value::Map(values)
    }
}

fn bool_value(values: &BTreeMap<String, Value>, key: &str) -> Option<bool> {
    match values.get(key) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn string_value(values: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    match values.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn fault_text_is_active(fault: Option<&str>) -> bool {
    let Some(fault) = fault else {
        return false;
    };
    let normalized = fault.trim().to_ascii_lowercase();
    !matches!(
        normalized.as_str(),
        "" | "none" | "no fault" | "no_fault" | "no error" | "no_error" | "ok" | "0" | "false"
    )
}

#[derive(Debug, Clone)]
pub struct ResourceDescriptor {
    pub id: ResourceId,
    pub driver: DriverId,
    pub label: String,
    pub kind: String,
    pub metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CapabilityKind {
    CameraCapture,
    CameraStream,
    TriggerSink,
    TriggerSource,
    RawRegisterAccess,
    PlateMove,
    Measure,
    TemperatureControl,
    GasControl,
    ImagingHead,
    CameraBinding,
    DigitalIo,
    Dac,
    Adc,
    PulseProgram,
    /// Run a point-scanning/confocal acquisition and return a reconstructed
    /// final image or stack.
    ConfocalImageCapture,
    /// Run a point-scanning/confocal acquisition with live reconstructed image
    /// updates, typically as dirty regions in a mutable frame.
    ConfocalImageStream,
    /// Stream raw timed detector/DAQ samples for non-standard scan cycles or
    /// external reconstruction.
    ScanSignalStream,
    StageMove,
    StageHome,
    StageStop,
    ValveSelect,
    FilterSelect,
    /// A general focus-control surface. Implementations may be firmware
    /// autofocus units, laser triangulation gates, contrast autofocus services,
    /// or composed devices that depend on a camera, Z stage, and light source.
    Autofocus,
    GenericCommand,
    Custom(String),
}

impl CapabilityKind {
    pub fn name(&self) -> &str {
        match self {
            CapabilityKind::CameraCapture => "CameraCapture",
            CapabilityKind::CameraStream => "CameraStream",
            CapabilityKind::TriggerSink => "TriggerSink",
            CapabilityKind::TriggerSource => "TriggerSource",
            CapabilityKind::RawRegisterAccess => "RawRegisterAccess",
            CapabilityKind::PlateMove => "PlateMove",
            CapabilityKind::Measure => "Measure",
            CapabilityKind::TemperatureControl => "TemperatureControl",
            CapabilityKind::GasControl => "GasControl",
            CapabilityKind::ImagingHead => "ImagingHead",
            CapabilityKind::CameraBinding => "CameraBinding",
            CapabilityKind::DigitalIo => "DigitalIo",
            CapabilityKind::Dac => "Dac",
            CapabilityKind::Adc => "Adc",
            CapabilityKind::PulseProgram => "PulseProgram",
            CapabilityKind::ConfocalImageCapture => "ConfocalImageCapture",
            CapabilityKind::ConfocalImageStream => "ConfocalImageStream",
            CapabilityKind::ScanSignalStream => "ScanSignalStream",
            CapabilityKind::StageMove => "StageMove",
            CapabilityKind::StageHome => "StageHome",
            CapabilityKind::StageStop => "StageStop",
            CapabilityKind::ValveSelect => "ValveSelect",
            CapabilityKind::FilterSelect => "FilterSelect",
            CapabilityKind::Autofocus => "Autofocus",
            CapabilityKind::GenericCommand => "GenericCommand",
            CapabilityKind::Custom(name) => name.as_str(),
        }
    }

    pub fn preferred_request_kind(&self) -> CapabilityRequestKind {
        match self {
            CapabilityKind::CameraCapture => CapabilityRequestKind::CameraCapture,
            CapabilityKind::CameraStream => CapabilityRequestKind::CameraStream,
            CapabilityKind::TriggerSink | CapabilityKind::TriggerSource => {
                CapabilityRequestKind::Trigger
            }
            CapabilityKind::Measure => CapabilityRequestKind::Measure,
            CapabilityKind::PlateMove => CapabilityRequestKind::PlateMove,
            CapabilityKind::TemperatureControl => CapabilityRequestKind::TemperatureControl,
            CapabilityKind::GasControl => CapabilityRequestKind::GasControl,
            CapabilityKind::ImagingHead => CapabilityRequestKind::ImagingHead,
            CapabilityKind::CameraBinding => CapabilityRequestKind::CameraBinding,
            CapabilityKind::DigitalIo => CapabilityRequestKind::DigitalIo,
            CapabilityKind::Dac => CapabilityRequestKind::Dac,
            CapabilityKind::Adc => CapabilityRequestKind::Adc,
            CapabilityKind::PulseProgram => CapabilityRequestKind::PulseProgram,
            CapabilityKind::ConfocalImageCapture => CapabilityRequestKind::ConfocalImageCapture,
            CapabilityKind::ConfocalImageStream => CapabilityRequestKind::ConfocalImageStream,
            CapabilityKind::ScanSignalStream => CapabilityRequestKind::ScanSignalStream,
            CapabilityKind::StageMove => CapabilityRequestKind::StageMove,
            CapabilityKind::StageHome | CapabilityKind::StageStop => CapabilityRequestKind::None,
            CapabilityKind::ValveSelect => CapabilityRequestKind::ValveSelect,
            CapabilityKind::FilterSelect => CapabilityRequestKind::FilterSelect,
            CapabilityKind::Autofocus => CapabilityRequestKind::Autofocus,
            CapabilityKind::RawRegisterAccess
            | CapabilityKind::GenericCommand
            | CapabilityKind::Custom(_) => CapabilityRequestKind::GenericCommand,
        }
    }

    pub fn is_diagnostic(&self) -> bool {
        matches!(
            self,
            CapabilityKind::RawRegisterAccess
                | CapabilityKind::GenericCommand
                | CapabilityKind::Custom(_)
        )
    }

    pub fn is_hidden_maintenance(&self) -> bool {
        matches!(self, CapabilityKind::Custom(name) if generic_command_is_hidden_maintenance(name))
    }
}

#[derive(Debug, Clone)]
pub struct CapabilityDescriptor {
    pub id: CapabilityId,
    pub device: DeviceId,
    pub kind: CapabilityKind,
    pub name: String,
    pub request: ValueType,
    pub response: ValueType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityExposure {
    User,
    AdvancedDiagnostic,
    HiddenMaintenance,
}

impl CapabilityDescriptor {
    pub fn new(
        id: CapabilityId,
        device: DeviceId,
        kind: CapabilityKind,
        response: ValueType,
    ) -> Self {
        let name = kind.name().to_string();
        Self::with_name(id, device, kind, name, response)
    }

    pub fn with_name(
        id: CapabilityId,
        device: DeviceId,
        kind: CapabilityKind,
        name: impl Into<String>,
        response: ValueType,
    ) -> Self {
        Self {
            id,
            device,
            request: kind.preferred_request_kind().value_type(),
            response,
            kind,
            name: name.into(),
        }
    }

    pub fn preferred_request_kind(&self) -> CapabilityRequestKind {
        self.kind.preferred_request_kind()
    }

    pub fn request_kind(&self) -> CapabilityRequestKind {
        self.preferred_request_kind()
    }

    pub fn accepts_request(&self, request: &CapabilityRequest) -> bool {
        self.preferred_request_kind().accepts(request)
    }

    pub fn is_diagnostic(&self) -> bool {
        self.kind.is_diagnostic()
    }

    pub fn requires_driver_validated_command_aliases(&self) -> bool {
        matches!(
            self.kind,
            CapabilityKind::GenericCommand | CapabilityKind::RawRegisterAccess
        ) || matches!(self.kind, CapabilityKind::Custom(_))
    }

    pub fn is_hidden_maintenance(&self) -> bool {
        self.kind.is_hidden_maintenance()
            || (self.is_diagnostic() && generic_command_is_hidden_maintenance(&self.name))
    }

    pub fn exposure(&self) -> CapabilityExposure {
        if self.is_hidden_maintenance() {
            CapabilityExposure::HiddenMaintenance
        } else if self.requires_driver_validated_command_aliases() {
            CapabilityExposure::AdvancedDiagnostic
        } else {
            CapabilityExposure::User
        }
    }
}

#[derive(Debug, Clone)]
pub struct StateSet {
    pub name: Option<String>,
    pub writes: Vec<StateWrite>,
    pub commit: CommitMode,
}

impl StateSet {
    pub fn immediate(name: impl Into<String>) -> Self {
        Self::new(Some(name.into()), CommitMode::Immediate)
    }

    pub fn prepare_then_commit(name: impl Into<String>) -> Self {
        Self::new(Some(name.into()), CommitMode::PrepareThenCommit)
    }

    pub fn hardware_timed(name: impl Into<String>, at: TimePoint) -> Self {
        Self::new(Some(name.into()), CommitMode::HardwareTimed { at })
    }

    pub fn unnamed(commit: CommitMode) -> Self {
        Self::new(None, commit)
    }

    pub fn new(name: Option<String>, commit: CommitMode) -> Self {
        Self {
            name,
            writes: Vec::new(),
            commit,
        }
    }

    pub fn with_write(
        mut self,
        device: impl Into<DeviceId>,
        property: impl Into<String>,
        value: Value,
    ) -> Self {
        self.writes
            .push(StateWrite::new(device.into(), property, value));
        self
    }

    pub fn with_writes(mut self, writes: impl IntoIterator<Item = StateWrite>) -> Self {
        self.writes.extend(writes);
        self
    }

    pub fn push_write(
        &mut self,
        device: impl Into<DeviceId>,
        property: impl Into<String>,
        value: Value,
    ) -> &mut Self {
        self.writes
            .push(StateWrite::new(device.into(), property, value));
        self
    }

    pub fn extend_writes(&mut self, writes: impl IntoIterator<Item = StateWrite>) -> &mut Self {
        self.writes.extend(writes);
        self
    }

    pub fn into_command(self) -> Command {
        Command::ApplyStateSet(self)
    }
}

#[derive(Debug, Clone)]
pub struct StateWrite {
    pub device: DeviceId,
    pub property: String,
    pub value: Value,
}

impl StateWrite {
    pub fn new(device: impl Into<DeviceId>, property: impl Into<String>, value: Value) -> Self {
        Self {
            device: device.into(),
            property: property.into(),
            value,
        }
    }
}

#[macro_export]
macro_rules! state_writes {
    ($device:expr => { $($property:literal => $value:expr),* $(,)? }) => {
        vec![
            $($crate::StateWrite::new($device, $property, $value),)*
        ]
    };
}

#[macro_export]
macro_rules! push_writes {
    ($writes:expr, $device:expr => { $($property:literal => $value:expr),* $(,)? }) => {
        $(
            $writes.push($crate::StateWrite::new($device, $property, $value));
        )*
    };
}

#[derive(Debug, Clone)]
pub enum CommitMode {
    Immediate,
    PrepareThenCommit,
    HardwareTimed { at: TimePoint },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimePoint {
    pub ticks: u64,
    pub clock: Option<DeviceId>,
}

#[derive(Debug, Clone)]
pub enum Command {
    ReadProperty {
        device: DeviceId,
        key: String,
    },
    WriteProperty {
        device: DeviceId,
        key: String,
        value: Value,
    },
    Invoke {
        device: DeviceId,
        capability: CapabilityId,
        request: CapabilityRequest,
    },
    ApplyStateSet(StateSet),
    Arm(TimingPlan),
    Start(OperationId),
    Stop(OperationId),
}

#[derive(Debug, Clone, PartialEq)]
pub enum CapabilityRequest {
    None,
    CameraCapture(CameraCaptureRequest),
    CameraStream(CameraStreamRequest),
    StageMove(StageMoveRequest),
    PlateMove(PlateMoveRequest),
    DigitalIo(DigitalIoRequest),
    Dac(DacRequest),
    Adc(AdcRequest),
    Trigger(TriggerRequest),
    Measure(MeasureRequest),
    ConfocalImageCapture(ConfocalImageCaptureRequest),
    ConfocalImageStream(ConfocalImageStreamRequest),
    ScanSignalStream(ScanSignalStreamRequest),
    TemperatureControl(TemperatureControlRequest),
    GasControl(GasControlRequest),
    ImagingHead(ImagingHeadRequest),
    CameraBinding(CameraBindingRequest),
    PulseProgram(PulseProgramRequest),
    ValveSelect(ValveSelectRequest),
    FilterSelect(FilterSelectRequest),
    Autofocus(AutofocusRequest),
    GenericCommand(GenericCommandRequest),
    Custom(Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CapabilityRequestKind {
    None,
    CameraCapture,
    CameraStream,
    StageMove,
    PlateMove,
    DigitalIo,
    Dac,
    Adc,
    Trigger,
    Measure,
    ConfocalImageCapture,
    ConfocalImageStream,
    ScanSignalStream,
    TemperatureControl,
    GasControl,
    ImagingHead,
    CameraBinding,
    PulseProgram,
    ValveSelect,
    FilterSelect,
    Autofocus,
    GenericCommand,
    Custom,
}

impl CapabilityRequestKind {
    pub fn value_type(&self) -> ValueType {
        match self {
            CapabilityRequestKind::None => ValueType::Null,
            CapabilityRequestKind::CameraCapture
            | CapabilityRequestKind::CameraStream
            | CapabilityRequestKind::StageMove
            | CapabilityRequestKind::PlateMove
            | CapabilityRequestKind::DigitalIo
            | CapabilityRequestKind::Dac
            | CapabilityRequestKind::Adc
            | CapabilityRequestKind::Trigger
            | CapabilityRequestKind::Measure
            | CapabilityRequestKind::ConfocalImageCapture
            | CapabilityRequestKind::ConfocalImageStream
            | CapabilityRequestKind::ScanSignalStream
            | CapabilityRequestKind::TemperatureControl
            | CapabilityRequestKind::GasControl
            | CapabilityRequestKind::ImagingHead
            | CapabilityRequestKind::CameraBinding
            | CapabilityRequestKind::PulseProgram
            | CapabilityRequestKind::ValveSelect
            | CapabilityRequestKind::FilterSelect
            | CapabilityRequestKind::Autofocus
            | CapabilityRequestKind::GenericCommand
            | CapabilityRequestKind::Custom => ValueType::Map,
        }
    }

    pub fn accepts(&self, request: &CapabilityRequest) -> bool {
        match (self, request) {
            (CapabilityRequestKind::None, CapabilityRequest::None)
            | (CapabilityRequestKind::CameraCapture, CapabilityRequest::CameraCapture(_))
            | (CapabilityRequestKind::CameraStream, CapabilityRequest::CameraStream(_))
            | (CapabilityRequestKind::StageMove, CapabilityRequest::StageMove(_))
            | (CapabilityRequestKind::PlateMove, CapabilityRequest::PlateMove(_))
            | (CapabilityRequestKind::DigitalIo, CapabilityRequest::DigitalIo(_))
            | (CapabilityRequestKind::Dac, CapabilityRequest::Dac(_))
            | (CapabilityRequestKind::Adc, CapabilityRequest::Adc(_))
            | (CapabilityRequestKind::Trigger, CapabilityRequest::Trigger(_))
            | (CapabilityRequestKind::Measure, CapabilityRequest::Measure(_))
            | (
                CapabilityRequestKind::ConfocalImageCapture,
                CapabilityRequest::ConfocalImageCapture(_),
            )
            | (
                CapabilityRequestKind::ConfocalImageStream,
                CapabilityRequest::ConfocalImageStream(_),
            )
            | (CapabilityRequestKind::ScanSignalStream, CapabilityRequest::ScanSignalStream(_))
            | (
                CapabilityRequestKind::TemperatureControl,
                CapabilityRequest::TemperatureControl(_),
            )
            | (CapabilityRequestKind::GasControl, CapabilityRequest::GasControl(_))
            | (CapabilityRequestKind::ImagingHead, CapabilityRequest::ImagingHead(_))
            | (CapabilityRequestKind::CameraBinding, CapabilityRequest::CameraBinding(_))
            | (CapabilityRequestKind::PulseProgram, CapabilityRequest::PulseProgram(_))
            | (CapabilityRequestKind::ValveSelect, CapabilityRequest::ValveSelect(_))
            | (CapabilityRequestKind::FilterSelect, CapabilityRequest::FilterSelect(_))
            | (CapabilityRequestKind::Autofocus, CapabilityRequest::Autofocus(_))
            | (CapabilityRequestKind::GenericCommand, CapabilityRequest::GenericCommand(_))
            | (CapabilityRequestKind::Custom, CapabilityRequest::Custom(_)) => true,
            (CapabilityRequestKind::Trigger, CapabilityRequest::None)
            | (CapabilityRequestKind::PulseProgram, CapabilityRequest::None)
            | (CapabilityRequestKind::CameraCapture, CapabilityRequest::None) => true,
            _ => false,
        }
    }
}

impl CapabilityRequest {
    pub fn request_kind(&self) -> CapabilityRequestKind {
        match self {
            CapabilityRequest::None => CapabilityRequestKind::None,
            CapabilityRequest::CameraCapture(_) => CapabilityRequestKind::CameraCapture,
            CapabilityRequest::CameraStream(_) => CapabilityRequestKind::CameraStream,
            CapabilityRequest::StageMove(_) => CapabilityRequestKind::StageMove,
            CapabilityRequest::PlateMove(_) => CapabilityRequestKind::PlateMove,
            CapabilityRequest::DigitalIo(_) => CapabilityRequestKind::DigitalIo,
            CapabilityRequest::Dac(_) => CapabilityRequestKind::Dac,
            CapabilityRequest::Adc(_) => CapabilityRequestKind::Adc,
            CapabilityRequest::Trigger(_) => CapabilityRequestKind::Trigger,
            CapabilityRequest::Measure(_) => CapabilityRequestKind::Measure,
            CapabilityRequest::ConfocalImageCapture(_) => {
                CapabilityRequestKind::ConfocalImageCapture
            }
            CapabilityRequest::ConfocalImageStream(_) => CapabilityRequestKind::ConfocalImageStream,
            CapabilityRequest::ScanSignalStream(_) => CapabilityRequestKind::ScanSignalStream,
            CapabilityRequest::TemperatureControl(_) => CapabilityRequestKind::TemperatureControl,
            CapabilityRequest::GasControl(_) => CapabilityRequestKind::GasControl,
            CapabilityRequest::ImagingHead(_) => CapabilityRequestKind::ImagingHead,
            CapabilityRequest::CameraBinding(_) => CapabilityRequestKind::CameraBinding,
            CapabilityRequest::PulseProgram(_) => CapabilityRequestKind::PulseProgram,
            CapabilityRequest::ValveSelect(_) => CapabilityRequestKind::ValveSelect,
            CapabilityRequest::FilterSelect(_) => CapabilityRequestKind::FilterSelect,
            CapabilityRequest::Autofocus(_) => CapabilityRequestKind::Autofocus,
            CapabilityRequest::GenericCommand(_) => CapabilityRequestKind::GenericCommand,
            CapabilityRequest::Custom(_) => CapabilityRequestKind::Custom,
        }
    }

    pub fn inferred_capability_kind(&self) -> Option<CapabilityKind> {
        match self {
            CapabilityRequest::CameraCapture(_) => Some(CapabilityKind::CameraCapture),
            CapabilityRequest::CameraStream(_) => Some(CapabilityKind::CameraStream),
            CapabilityRequest::StageMove(_) => Some(CapabilityKind::StageMove),
            CapabilityRequest::PlateMove(_) => Some(CapabilityKind::PlateMove),
            CapabilityRequest::DigitalIo(_) => Some(CapabilityKind::DigitalIo),
            CapabilityRequest::Dac(_) => Some(CapabilityKind::Dac),
            CapabilityRequest::Adc(_) => Some(CapabilityKind::Adc),
            CapabilityRequest::Measure(_) => Some(CapabilityKind::Measure),
            CapabilityRequest::ConfocalImageCapture(_) => {
                Some(CapabilityKind::ConfocalImageCapture)
            }
            CapabilityRequest::ConfocalImageStream(_) => Some(CapabilityKind::ConfocalImageStream),
            CapabilityRequest::ScanSignalStream(_) => Some(CapabilityKind::ScanSignalStream),
            CapabilityRequest::TemperatureControl(_) => Some(CapabilityKind::TemperatureControl),
            CapabilityRequest::GasControl(_) => Some(CapabilityKind::GasControl),
            CapabilityRequest::ImagingHead(_) => Some(CapabilityKind::ImagingHead),
            CapabilityRequest::CameraBinding(_) => Some(CapabilityKind::CameraBinding),
            CapabilityRequest::PulseProgram(_) => Some(CapabilityKind::PulseProgram),
            CapabilityRequest::ValveSelect(_) => Some(CapabilityKind::ValveSelect),
            CapabilityRequest::FilterSelect(_) => Some(CapabilityKind::FilterSelect),
            CapabilityRequest::Autofocus(_) => Some(CapabilityKind::Autofocus),
            CapabilityRequest::None
            | CapabilityRequest::Trigger(_)
            | CapabilityRequest::GenericCommand(_)
            | CapabilityRequest::Custom(_) => None,
        }
    }
}

macro_rules! impl_capability_request_from {
    ($request:ty, $variant:ident) => {
        impl From<$request> for CapabilityRequest {
            fn from(request: $request) -> Self {
                CapabilityRequest::$variant(request)
            }
        }
    };
}

impl_capability_request_from!(CameraCaptureRequest, CameraCapture);
impl_capability_request_from!(CameraStreamRequest, CameraStream);
impl_capability_request_from!(StageMoveRequest, StageMove);
impl_capability_request_from!(PlateMoveRequest, PlateMove);
impl_capability_request_from!(DigitalIoRequest, DigitalIo);
impl_capability_request_from!(DacRequest, Dac);
impl_capability_request_from!(AdcRequest, Adc);
impl_capability_request_from!(MeasureRequest, Measure);
impl_capability_request_from!(ConfocalImageCaptureRequest, ConfocalImageCapture);
impl_capability_request_from!(ConfocalImageStreamRequest, ConfocalImageStream);
impl_capability_request_from!(ScanSignalStreamRequest, ScanSignalStream);
impl_capability_request_from!(TemperatureControlRequest, TemperatureControl);
impl_capability_request_from!(GasControlRequest, GasControl);
impl_capability_request_from!(ImagingHeadRequest, ImagingHead);
impl_capability_request_from!(CameraBindingRequest, CameraBinding);
impl_capability_request_from!(PulseProgramRequest, PulseProgram);
impl_capability_request_from!(ValveSelectRequest, ValveSelect);
impl_capability_request_from!(FilterSelectRequest, FilterSelect);
impl_capability_request_from!(AutofocusRequest, Autofocus);
impl_capability_request_from!(GenericCommandRequest, GenericCommand);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraCaptureRequest {
    pub encoding: Option<ImageEncoding>,
    pub buffer: Option<FrameBufferSpec>,
}

impl CameraCaptureRequest {
    pub fn default_frame() -> Self {
        Self {
            encoding: None,
            buffer: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedFrame {
    pub handle: FrameHandle,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub pixel_format: Option<String>,
}

impl CapturedFrame {
    pub fn from_completion(value: &Value) -> Result<Self> {
        let Value::Map(map) = value else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "camera capture completion must be a map",
            ));
        };
        let stream = completion_u64(map, "stream")?;
        let frame = completion_u64(map, "frame")?;
        let width = optional_completion_u32(map, "width")?;
        let height = optional_completion_u32(map, "height")?;
        let pixel_format = match map.get("pixel_format") {
            Some(Value::String(value)) => Some(value.clone()),
            Some(_) => {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "camera capture completion pixel_format must be a string",
                ))
            }
            None => None,
        };

        Ok(Self {
            handle: FrameHandle {
                stream: StreamId(stream),
                frame: FrameId(frame),
            },
            width,
            height,
            pixel_format,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraStreamRequest {
    pub encoding: Option<ImageEncoding>,
    pub frame_count: Option<u64>,
    pub buffer: FrameBufferSpec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraStreamStarted {
    pub stream: StreamId,
    pub frame_count: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub pixel_format: Option<String>,
}

impl CameraStreamStarted {
    pub fn from_completion(value: &Value) -> Result<Self> {
        let Value::Map(map) = value else {
            return Err(Error::new(
                ErrorCode::InvalidCommand,
                "camera stream completion must be a map",
            ));
        };
        let stream = completion_u64(map, "stream")?;
        let frame_count = match map.get("frames").or_else(|| map.get("frame_count")) {
            Some(Value::I64(value)) if *value >= 0 => Some(*value as u64),
            Some(_) => {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "camera stream completion frame count must be a non-negative integer",
                ))
            }
            None => None,
        };
        let width = optional_completion_u32(map, "width")?;
        let height = optional_completion_u32(map, "height")?;
        let pixel_format = match map.get("pixel_format") {
            Some(Value::String(value)) => Some(value.clone()),
            Some(_) => {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "camera stream completion pixel_format must be a string",
                ))
            }
            None => None,
        };

        Ok(Self {
            stream: StreamId(stream),
            frame_count,
            width,
            height,
            pixel_format,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameStreamStatus {
    pub stream: StreamId,
    pub buffer: FrameBufferSpec,
    pub retained_frames: Vec<FrameHandle>,
    pub dropped_frames: u64,
}

impl FrameStreamStatus {
    pub fn depth(&self) -> usize {
        self.retained_frames.len()
    }

    pub fn capacity(&self) -> usize {
        self.buffer.capacity_frames.max(1)
    }

    pub fn first(&self) -> Option<FrameHandle> {
        self.retained_frames.first().copied()
    }

    pub fn latest(&self) -> Option<FrameHandle> {
        self.retained_frames.last().copied()
    }

    pub fn as_value(&self) -> Value {
        Value::Map(BTreeMap::from([
            ("stream".into(), Value::I64(self.stream.0 as i64)),
            ("capacity".into(), Value::I64(self.capacity() as i64)),
            ("depth".into(), Value::I64(self.depth() as i64)),
            (
                "dropped_frames".into(),
                Value::I64(self.dropped_frames as i64),
            ),
            (
                "overflow_policy".into(),
                Value::String(frame_overflow_policy_name(&self.buffer.overflow).into()),
            ),
            (
                "retained_frames".into(),
                Value::List(
                    self.retained_frames
                        .iter()
                        .map(|handle| Value::I64(handle.frame.0 as i64))
                        .collect(),
                ),
            ),
        ]))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameBufferSpec {
    pub capacity_frames: usize,
    pub overflow: OverflowPolicy,
}

impl Default for FrameBufferSpec {
    fn default() -> Self {
        Self {
            capacity_frames: 64,
            overflow: OverflowPolicy::DropOldest,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverflowPolicy {
    DropOldest,
    DropNewest,
    Error,
}

pub fn frame_overflow_policy_name(policy: &OverflowPolicy) -> &'static str {
    match policy {
        OverflowPolicy::DropOldest => "drop_oldest",
        OverflowPolicy::DropNewest => "drop_newest",
        OverflowPolicy::Error => "error",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageEncoding {
    Native,
    Mono8,
    Mono16,
    Rgb8,
    Bgr8,
    Raw8,
    Raw16,
}

impl ImageEncoding {
    pub fn property_value(&self) -> &'static str {
        match self {
            ImageEncoding::Native => "Native",
            ImageEncoding::Mono8 => "Mono8",
            ImageEncoding::Mono16 => "Mono16",
            ImageEncoding::Rgb8 => "Rgb8",
            ImageEncoding::Bgr8 => "Bgr8",
            ImageEncoding::Raw8 => "Raw8",
            ImageEncoding::Raw16 => "Raw16",
        }
    }
}

pub fn canonical_image_encoding_name(name: &str) -> Option<&'static str> {
    match name {
        "Native" | "native" => Some(ImageEncoding::Native.property_value()),
        "Mono8" | "MONO8" | "Mono8Packed" => Some(ImageEncoding::Mono8.property_value()),
        "Mono16" | "MONO16" | "Mono16Packed" => Some(ImageEncoding::Mono16.property_value()),
        "Rgb8" | "RGB8" => Some(ImageEncoding::Rgb8.property_value()),
        "Bgr8" | "BGR8" => Some(ImageEncoding::Bgr8.property_value()),
        "Raw8" | "RAW8" => Some(ImageEncoding::Raw8.property_value()),
        "Raw16" | "RAW16" => Some(ImageEncoding::Raw16.property_value()),
        _ => None,
    }
}

fn completion_u64(map: &BTreeMap<String, Value>, key: &str) -> Result<u64> {
    let Some(value) = map.get(key) else {
        return Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("camera completion is missing {key}"),
        ));
    };
    match value {
        Value::I64(value) if *value >= 0 => Ok(*value as u64),
        _ => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("camera completion {key} must be a non-negative integer"),
        )),
    }
}

fn optional_completion_u32(map: &BTreeMap<String, Value>, key: &str) -> Result<Option<u32>> {
    match map.get(key) {
        Some(Value::I64(value)) if *value >= 0 && *value <= u32::MAX as i64 => {
            Ok(Some(*value as u32))
        }
        Some(Value::PixelCount(value)) => Ok(Some(value.pixels())),
        Some(_) => Err(Error::new(
            ErrorCode::InvalidCommand,
            format!("camera completion {key} must fit u32"),
        )),
        None => Ok(None),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StageAxis {
    X,
    Y,
    Z,
    Theta,
    Custom(String),
}

impl StageAxis {
    pub fn name(&self) -> &str {
        match self {
            StageAxis::X => "x",
            StageAxis::Y => "y",
            StageAxis::Z => "z",
            StageAxis::Theta => "theta",
            StageAxis::Custom(name) => name.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageGeometry {
    Stage1D {
        axis: StageAxis,
    },
    Stage2D {
        x: StageAxis,
        y: StageAxis,
    },
    Stage3D {
        x: StageAxis,
        y: StageAxis,
        z: StageAxis,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct MotionProfile {
    pub velocity: Option<Velocity>,
    pub acceleration: Option<Acceleration>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StageMoveRequest {
    pub target: BTreeMap<StageAxis, Position>,
    pub relative: bool,
    pub profile: Option<MotionProfile>,
}

impl StageMoveRequest {
    pub fn absolute(target: impl IntoIterator<Item = (StageAxis, Position)>) -> Self {
        Self {
            target: target.into_iter().collect(),
            relative: false,
            profile: None,
        }
    }

    pub fn relative(target: impl IntoIterator<Item = (StageAxis, Position)>) -> Self {
        Self {
            target: target.into_iter().collect(),
            relative: true,
            profile: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlateMoveRequest {
    pub well: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigitalIoRequest {
    pub mask: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DacRequest {
    /// Typed output value. Drivers advertise the accepted quantity through the
    /// target capability/property schema and convert units at the wire boundary.
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdcRequest {
    pub channel: Option<String>,
    pub integration_time: Option<TimeInterval>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TriggerRequest {
    pub action: TriggerAction,
    pub duration: Option<TimeInterval>,
    pub control_illumination: Option<bool>,
}

impl TriggerRequest {
    pub fn pulse() -> Self {
        Self {
            action: TriggerAction::Pulse,
            duration: None,
            control_illumination: None,
        }
    }

    pub fn enable() -> Self {
        Self {
            action: TriggerAction::Enable,
            duration: None,
            control_illumination: None,
        }
    }

    pub fn disable() -> Self {
        Self {
            action: TriggerAction::Disable,
            duration: None,
            control_illumination: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerAction {
    Enable,
    Disable,
    Pulse,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeasureRequest {
    pub integration_time: Option<TimeInterval>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfocalImageCaptureRequest {
    pub scan: BTreeMap<String, Value>,
    pub reconstruction: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfocalImageStreamRequest {
    pub scan: BTreeMap<String, Value>,
    pub reconstruction: BTreeMap<String, Value>,
    pub update_policy: Option<String>,
    pub overwrite_previous_pixels: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScanSignalStreamRequest {
    pub timing: BTreeMap<String, Value>,
    pub channels: Vec<String>,
    pub chunk_size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TemperatureControlRequest {
    pub target: Option<Temperature>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GasControlRequest {
    pub co2_target: Option<GasConcentration>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImagingHeadRequest {
    pub objective: Option<i64>,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraBindingRequest {
    pub bound: Option<bool>,
    pub imaging_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PulseProgramRequest {
    pub interval: Option<TimeInterval>,
    pub duration: Option<TimeInterval>,
    pub count: Option<u64>,
    pub wait_for_input: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValveSelectRequest {
    pub position: u8,
    pub direction: Option<ValveDirection>,
}

impl ValveSelectRequest {
    pub fn position(position: u8) -> Self {
        Self {
            position,
            direction: None,
        }
    }

    pub fn with_direction(mut self, direction: ValveDirection) -> Self {
        self.direction = Some(direction);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValveDirection {
    Clockwise,
    CounterClockwise,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterSelectRequest {
    pub position: u8,
}

impl FilterSelectRequest {
    pub fn position(position: u8) -> Self {
        Self { position }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AutofocusRequest {
    /// Requested autofocus behavior. Hardware decides completion from its own
    /// status/telemetry; callers should wait on the returned operation handle.
    pub mode: AutofocusMode,
    /// Optional search range for implementations that perform Z-stack or
    /// contrast search. Hardware autofocus gates may ignore this.
    pub range: Option<AutofocusRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutofocusMode {
    SingleShot,
    Continuous,
    Hold,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutofocusRange {
    pub center: Option<Position>,
    pub span: Position,
    pub step: Option<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenericCommandRequest {
    pub command: String,
    pub params: BTreeMap<String, Value>,
}

impl GenericCommandRequest {
    /// Returns true for maintenance operations that must stay out of user-facing
    /// command surfaces, including advanced UIs.
    pub fn is_hidden_maintenance(&self) -> bool {
        generic_command_request_is_hidden_maintenance(self)
    }
}

pub fn generic_command_request_is_hidden_maintenance(request: &GenericCommandRequest) -> bool {
    if generic_command_is_hidden_maintenance(&request.command) {
        return true;
    }

    request.params.iter().any(|(key, value)| {
        generic_command_is_hidden_maintenance(key)
            || (is_generic_command_target_param(key)
                && generic_command_value_is_hidden_maintenance(value))
    })
}

fn is_generic_command_target_param(key: &str) -> bool {
    let compact = key
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect::<String>();
    matches!(
        compact.as_str(),
        "command"
            | "operation"
            | "action"
            | "method"
            | "target"
            | "node"
            | "genicamnode"
            | "register"
            | "feature"
            | "property"
            | "endpoint"
            | "path"
            | "name"
    )
}

pub fn generic_command_value_is_hidden_maintenance(value: &Value) -> bool {
    match value {
        Value::String(value) => generic_command_is_hidden_maintenance(value),
        Value::List(values) => values
            .iter()
            .any(generic_command_value_is_hidden_maintenance),
        Value::Map(values) => values.iter().any(|(key, value)| {
            generic_command_is_hidden_maintenance(key)
                || generic_command_value_is_hidden_maintenance(value)
        }),
        _ => false,
    }
}

pub fn generic_command_is_hidden_maintenance(command: &str) -> bool {
    let compact = command
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect::<String>();

    const CONTAINS_MARKERS: &[&str] = &[
        "reset",
        "clearfault",
        "faultclear",
        "acknowledgefault",
        "faultacknowledge",
        "clearalarm",
        "alarmclear",
        "clearerror",
        "errorclear",
        "clearerrors",
        "errorsclear",
        "upload",
        "fw",
        "firmware",
        "updater",
        "bootloader",
        "loader",
        "deviceloader",
        "flash",
        "dfu",
        "burn",
        "eprom",
        "bootrom",
        "bootmode",
        "bootfirmware",
        "bitfile",
        "factoryreset",
        "factorydefault",
        "factory",
        "default",
        "restoredefault",
        "restoredefaults",
        "restoredef",
        "factoryrestore",
        "restorefactory",
        "resetdefault",
        "resetdefaults",
        "factorysettings",
        "restore",
        "store",
        "erase",
        "eeprom",
        "calibration",
        "calibrate",
        "maintenance",
        "service",
        "vendorservice",
        "reboot",
        "restart",
        "coldstart",
        "warmstart",
        "powercycle",
        "cyclepower",
        "renumerate",
        "reenumerate",
        "reinitialize",
        "reinitialise",
        "reinit",
        "setorigin",
        "zeroposition",
        "boot",
        "devicereset",
        "resetdevice",
        "loadfirmware",
        "firmwareload",
        "updatefirmware",
        "firmwareupdate",
        "upgradefirmware",
        "firmwareupgrade",
        "downloadfirmware",
        "firmwaredownload",
        "firmwareinit",
        "firmwareprogram",
        "programfirmware",
        "firmwareprogrammer",
        "programfirmwareimage",
        "firmwareimageprogram",
        "firmwareimagewrite",
        "writefirmware",
        "firmwarewrite",
        "burnfirmware",
        "firmwareburn",
        "loadfpga",
        "fpgaload",
        "fpga",
        "bitstream",
        "loadbitstream",
        "bitstreamload",
        "writebitstream",
        "bitstreamwrite",
        "flashprogram",
        "programflash",
        "writeflash",
        "flashwrite",
        "deviceprogram",
        "programdevice",
        "deviceupdate",
        "updatedevice",
        "eepromprogram",
        "programeeprom",
        "writeeeprom",
        "eepromwrite",
        "fpgaprogram",
        "programfpga",
        "nonvolatile",
        "persistent",
        "userset",
        "usersetsave",
        "usersetload",
        "usersetdefault",
        "savesettings",
        "saveconfiguration",
        "storeconfiguration",
        "persistconfiguration",
        "nvram",
        "fileaccess",
        "fileoperation",
        "fileoperationexecute",
        "fileselector",
        "fileopenmode",
    ];
    const EXACT_MARKERS: &[&str] = &[
        "init",
        "initialize",
        "initialise",
        "program",
        "programmer",
        "download",
        "save",
        "commit",
        "persist",
        "origin",
        "zero",
        "rom",
        "prom",
        "nvm",
    ];

    CONTAINS_MARKERS
        .iter()
        .any(|marker| compact.contains(marker))
        || EXACT_MARKERS.iter().any(|marker| compact == *marker)
}

impl Command {
    pub fn read_property(device: impl Into<DeviceId>, key: impl Into<String>) -> Self {
        Self::ReadProperty {
            device: device.into(),
            key: key.into(),
        }
    }

    pub fn write_property(
        device: impl Into<DeviceId>,
        key: impl Into<String>,
        value: Value,
    ) -> Self {
        Self::WriteProperty {
            device: device.into(),
            key: key.into(),
            value,
        }
    }

    pub fn invoke(
        device: impl Into<DeviceId>,
        capability: CapabilityId,
        request: CapabilityRequest,
    ) -> Self {
        Self::Invoke {
            device: device.into(),
            capability,
            request,
        }
    }

    pub fn arm(plan: TimingPlan) -> Self {
        Self::Arm(plan)
    }

    pub fn start(armed_operation: OperationId) -> Self {
        Self::Start(armed_operation)
    }

    pub fn stop(armed_operation: OperationId) -> Self {
        Self::Stop(armed_operation)
    }

    pub fn target_devices(&self) -> Vec<DeviceId> {
        match self {
            Command::ReadProperty { device, .. }
            | Command::WriteProperty { device, .. }
            | Command::Invoke { device, .. } => vec![*device],
            Command::ApplyStateSet(set) => unique_devices(set.writes.iter().map(|w| w.device)),
            Command::Arm(plan) => plan.participants.clone(),
            Command::Start(_) | Command::Stop(_) => Vec::new(),
        }
    }
}

fn unique_devices(devices: impl IntoIterator<Item = DeviceId>) -> Vec<DeviceId> {
    let mut unique = Vec::new();
    for device in devices {
        if !unique.contains(&device) {
            unique.push(device);
        }
    }
    unique
}

#[derive(Debug, Clone)]
pub struct CommandBatch {
    pub id: CommandId,
    pub commands: Vec<Command>,
}

#[derive(Debug, Clone)]
pub struct PreparedBatch {
    pub id: CommandId,
    pub commands: Vec<Command>,
    pub physical_transactions: Vec<PhysicalTransaction>,
}

#[derive(Debug, Clone)]
pub struct PhysicalTransaction {
    pub resource: Option<ResourceId>,
    pub description: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DriverToken(pub u64);

#[derive(Debug, Clone)]
pub enum TriggerDirection {
    Source,
    Sink,
}

#[derive(Debug, Clone)]
pub enum TriggerEdge {
    Rising,
    Falling,
    Both,
    LevelHigh,
    LevelLow,
}

#[derive(Debug, Clone)]
pub enum TriggerSignal {
    Ttl,
    Analog,
    Software,
    Clock,
}

#[derive(Debug, Clone)]
pub struct TriggerRoute {
    pub from: DeviceId,
    pub to: DeviceId,
    pub signal: TriggerSignal,
    pub edge: TriggerEdge,
    pub delay: Duration,
}

#[derive(Debug, Clone)]
pub struct DeviceSequence {
    pub device: DeviceId,
    pub property: String,
    pub values: Vec<Value>,
}

impl DeviceSequence {
    pub fn new<I>(device: impl Into<DeviceId>, property: impl Into<String>, values: I) -> Self
    where
        I: IntoIterator<Item = Value>,
    {
        Self {
            device: device.into(),
            property: property.into(),
            values: values.into_iter().collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum StartCondition {
    Software,
    ExternalTrigger(DeviceId),
    At(TimePoint),
}

#[derive(Debug, Clone)]
pub enum StopCondition {
    Manual,
    Count(u64),
    Duration(Duration),
}

#[derive(Debug, Clone)]
pub struct TimingPlan {
    pub participants: Vec<DeviceId>,
    pub routes: Vec<TriggerRoute>,
    pub sequences: Vec<DeviceSequence>,
    pub arm_order: Vec<DeviceId>,
    pub start: StartCondition,
    pub stop: StopCondition,
}

impl TimingPlan {
    pub fn builder() -> TimingPlanBuilder {
        TimingPlanBuilder::new()
    }

    pub fn from_parts<I, D>(
        routes: Vec<TriggerRoute>,
        sequences: Vec<DeviceSequence>,
        arm_order: I,
        start: StartCondition,
        stop: StopCondition,
    ) -> Result<Self>
    where
        I: IntoIterator<Item = D>,
        D: Into<DeviceId>,
    {
        Self::from_parts_with_participants(
            Vec::new(),
            routes,
            sequences,
            arm_order.into_iter().map(Into::into).collect(),
            start,
            stop,
        )
    }

    fn from_parts_with_participants(
        explicit_participants: Vec<DeviceId>,
        routes: Vec<TriggerRoute>,
        sequences: Vec<DeviceSequence>,
        arm_order: Vec<DeviceId>,
        start: StartCondition,
        stop: StopCondition,
    ) -> Result<Self> {
        for sequence in &sequences {
            if sequence.values.is_empty() {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    format!("timing sequence {} has no values", sequence.property),
                ));
            }
        }

        let mut checked_arm_order = Vec::new();
        for device in &arm_order {
            if checked_arm_order.contains(device) {
                return Err(Error::new(
                    ErrorCode::InvalidCommand,
                    "timing arm order contains a duplicate device",
                ));
            }
            checked_arm_order.push(*device);
        }

        let mut participants = Vec::new();
        for device in explicit_participants {
            push_unique_device(&mut participants, device);
        }
        for route in &routes {
            push_unique_device(&mut participants, route.from);
            push_unique_device(&mut participants, route.to);
        }
        for sequence in &sequences {
            push_unique_device(&mut participants, sequence.device);
        }
        for device in &arm_order {
            push_unique_device(&mut participants, *device);
        }
        if let StartCondition::ExternalTrigger(device) = &start {
            push_unique_device(&mut participants, *device);
        }

        let arm_order = effective_arm_order(arm_order, &participants);
        Ok(Self {
            participants,
            routes,
            sequences,
            arm_order,
            start,
            stop,
        })
    }
}

fn effective_arm_order(arm_order: Vec<DeviceId>, participants: &[DeviceId]) -> Vec<DeviceId> {
    if arm_order.is_empty() {
        participants.to_vec()
    } else {
        arm_order
    }
}

fn push_unique_device(devices: &mut Vec<DeviceId>, device: DeviceId) {
    if !devices.contains(&device) {
        devices.push(device);
    }
}

#[derive(Debug, Clone)]
pub struct TimingPlanBuilder {
    participants: Vec<DeviceId>,
    routes: Vec<TriggerRoute>,
    sequences: Vec<DeviceSequence>,
    arm_order: Vec<DeviceId>,
    start: StartCondition,
    stop: StopCondition,
}

impl Default for TimingPlanBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TimingPlanBuilder {
    pub fn new() -> Self {
        Self {
            participants: Vec::new(),
            routes: Vec::new(),
            sequences: Vec::new(),
            arm_order: Vec::new(),
            start: StartCondition::Software,
            stop: StopCondition::Manual,
        }
    }

    pub fn participant(mut self, device: impl Into<DeviceId>) -> Self {
        self.push_participant(device.into());
        self
    }

    pub fn participants<I, D>(mut self, devices: I) -> Self
    where
        I: IntoIterator<Item = D>,
        D: Into<DeviceId>,
    {
        for device in devices {
            self.push_participant(device.into());
        }
        self
    }

    pub fn route(
        mut self,
        from: impl Into<DeviceId>,
        to: impl Into<DeviceId>,
        signal: TriggerSignal,
        edge: TriggerEdge,
        delay: Duration,
    ) -> Self {
        let from = from.into();
        let to = to.into();
        self.push_participant(from);
        self.push_participant(to);
        self.routes.push(TriggerRoute {
            from,
            to,
            signal,
            edge,
            delay,
        });
        self
    }

    pub fn sequence<I>(
        mut self,
        device: impl Into<DeviceId>,
        property: impl Into<String>,
        values: I,
    ) -> Self
    where
        I: IntoIterator<Item = Value>,
    {
        let device = device.into();
        self.push_participant(device);
        self.sequences.push(DeviceSequence {
            device,
            property: property.into(),
            values: values.into_iter().collect(),
        });
        self
    }

    pub fn arm_order<I, D>(mut self, devices: I) -> Self
    where
        I: IntoIterator<Item = D>,
        D: Into<DeviceId>,
    {
        self.arm_order.clear();
        for device in devices {
            let device = device.into();
            self.push_participant(device);
            self.arm_order.push(device);
        }
        self
    }

    pub fn start(mut self, start: StartCondition) -> Self {
        if let StartCondition::ExternalTrigger(device) = start {
            self.push_participant(device);
            self.start = StartCondition::ExternalTrigger(device);
        } else {
            self.start = start;
        }
        self
    }

    pub fn stop(mut self, stop: StopCondition) -> Self {
        self.stop = stop;
        self
    }

    pub fn build(self) -> Result<TimingPlan> {
        TimingPlan::from_parts_with_participants(
            self.participants,
            self.routes,
            self.sequences,
            self.arm_order,
            self.start,
            self.stop,
        )
    }

    pub fn into_command(self) -> Result<Command> {
        Ok(Command::arm(self.build()?))
    }

    fn push_participant(&mut self, device: DeviceId) {
        if !self.participants.contains(&device) {
            self.participants.push(device);
        }
    }
}

#[derive(Debug, Clone)]
pub struct TimingPlanPreparation {
    pub driver: DriverId,
    pub physical_transactions: Vec<PhysicalTransaction>,
}

#[derive(Debug, Clone)]
pub struct ArmedTimingPlan {
    pub plan: TimingPlan,
    pub preparations: Vec<TimingPlanPreparation>,
}

#[derive(Debug, Clone)]
pub struct TimingPlanTransition {
    pub driver: DriverId,
    pub action: String,
    pub physical_transactions: Vec<PhysicalTransaction>,
}

#[derive(Debug, Clone)]
pub struct OperationHandle {
    pub id: OperationId,
    pub devices: Vec<DeviceId>,
}

#[derive(Debug, Clone)]
pub enum OperationStatus {
    Queued,
    Running { progress: Option<Progress> },
    Completed(Value),
    Failed(ErrorReport),
    Cancelled,
    TimedOut,
    Unknown,
}

impl OperationStatus {
    pub fn into_completed(self) -> Result<Value> {
        match self {
            OperationStatus::Completed(value) => Ok(value),
            OperationStatus::Failed(report) => Err(Error::new(report.code, report.message)),
            OperationStatus::Cancelled => Err(Error::new(ErrorCode::Cancelled, "cancelled")),
            OperationStatus::TimedOut => Err(Error::new(ErrorCode::Timeout, "timed out")),
            OperationStatus::Queued
            | OperationStatus::Running { .. }
            | OperationStatus::Unknown => Err(Error::new(
                ErrorCode::Driver,
                format!("operation did not complete: {self:?}"),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    pub completed: f64,
    pub total: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorReport {
    pub code: ErrorCode,
    pub message: String,
}

impl From<Error> for ErrorReport {
    fn from(error: Error) -> Self {
        Self {
            code: error.code,
            message: error.message,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelResult {
    Cancelled,
    AlreadyFinished,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    OperationChanged,
    PropertyChanged,
    FrameReady,
    ScanSignalChunk,
    Telemetry,
    DeviceArrived,
    DeviceRemoved,
    Fault,
    Log,
}

#[derive(Debug, Clone)]
pub enum DeviceSelector {
    All,
    One(DeviceId),
    Any(Vec<DeviceId>),
}

impl DeviceSelector {
    pub fn one(device: impl Into<DeviceId>) -> Self {
        Self::One(device.into())
    }

    pub fn any<I, D>(devices: I) -> Self
    where
        I: IntoIterator<Item = D>,
        D: Into<DeviceId>,
    {
        Self::Any(unique_devices(devices.into_iter().map(Into::into)))
    }

    pub fn matches(&self, device: Option<DeviceId>) -> bool {
        match (self, device) {
            (DeviceSelector::All, _) => true,
            (DeviceSelector::One(expected), Some(actual)) => expected == &actual,
            (DeviceSelector::Any(devices), Some(actual)) => devices.contains(&actual),
            _ => false,
        }
    }

    pub fn matches_any(&self, devices: &[DeviceId]) -> bool {
        match self {
            DeviceSelector::All => true,
            DeviceSelector::One(expected) => devices.contains(expected),
            DeviceSelector::Any(expected) => expected.iter().any(|device| devices.contains(device)),
        }
    }
}

#[derive(Debug, Clone)]
pub enum OperationSelector {
    All,
    One(OperationId),
    Any(Vec<OperationId>),
}

impl OperationSelector {
    pub fn one(operation: OperationId) -> Self {
        Self::One(operation)
    }

    pub fn any(operations: impl IntoIterator<Item = OperationId>) -> Self {
        let mut unique = Vec::new();
        for operation in operations {
            if !unique.contains(&operation) {
                unique.push(operation);
            }
        }
        Self::Any(unique)
    }

    pub fn matches(&self, operation: Option<OperationId>) -> bool {
        match (self, operation) {
            (OperationSelector::All, _) => true,
            (OperationSelector::One(expected), Some(actual)) => expected == &actual,
            (OperationSelector::Any(operations), Some(actual)) => operations.contains(&actual),
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EventFilter {
    pub devices: DeviceSelector,
    pub operations: OperationSelector,
    pub kinds: Vec<EventKind>,
}

impl EventFilter {
    pub fn all() -> Self {
        Self {
            devices: DeviceSelector::All,
            operations: OperationSelector::All,
            kinds: Vec::new(),
        }
    }

    pub fn kind(kind: EventKind) -> Self {
        Self::all().with_kind(kind)
    }

    pub fn kinds(kinds: impl IntoIterator<Item = EventKind>) -> Self {
        Self::all().with_kinds(kinds)
    }

    pub fn device(device: impl Into<DeviceId>) -> Self {
        Self::all().with_device(device)
    }

    pub fn devices<I, D>(devices: I) -> Self
    where
        I: IntoIterator<Item = D>,
        D: Into<DeviceId>,
    {
        Self::all().with_devices(devices)
    }

    pub fn operation(operation: OperationId) -> Self {
        Self::all().with_operation(operation)
    }

    pub fn operations(operations: impl IntoIterator<Item = OperationId>) -> Self {
        Self::all().with_operations(operations)
    }

    pub fn with_kind(mut self, kind: EventKind) -> Self {
        if !self.kinds.contains(&kind) {
            self.kinds.push(kind);
        }
        self
    }

    pub fn with_kinds(mut self, kinds: impl IntoIterator<Item = EventKind>) -> Self {
        for kind in kinds {
            if !self.kinds.contains(&kind) {
                self.kinds.push(kind);
            }
        }
        self
    }

    pub fn with_device(mut self, device: impl Into<DeviceId>) -> Self {
        self.devices = DeviceSelector::one(device);
        self
    }

    pub fn with_devices<I, D>(mut self, devices: I) -> Self
    where
        I: IntoIterator<Item = D>,
        D: Into<DeviceId>,
    {
        self.devices = DeviceSelector::any(devices);
        self
    }

    pub fn with_operation(mut self, operation: OperationId) -> Self {
        self.operations = OperationSelector::one(operation);
        self
    }

    pub fn with_operations(mut self, operations: impl IntoIterator<Item = OperationId>) -> Self {
        self.operations = OperationSelector::any(operations);
        self
    }

    pub fn matches(&self, event: &Event) -> bool {
        let kind_matches = self.kinds.is_empty() || self.kinds.contains(&event.kind());
        kind_matches
            && self.devices.matches_any(&event.devices())
            && self.operations.matches(event.operation())
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    OperationChanged(OperationChanged),
    PropertyChanged(PropertyChanged),
    FrameReady(FrameEvent),
    ScanSignalChunk(ScanSignalChunkEvent),
    Telemetry(TelemetryEvent),
    DeviceArrived(DeviceDescriptor),
    DeviceRemoved(DeviceId),
    Fault(FaultEvent),
    Log(LogEvent),
}

impl Event {
    pub fn kind(&self) -> EventKind {
        match self {
            Event::OperationChanged(_) => EventKind::OperationChanged,
            Event::PropertyChanged(_) => EventKind::PropertyChanged,
            Event::FrameReady(_) => EventKind::FrameReady,
            Event::ScanSignalChunk(_) => EventKind::ScanSignalChunk,
            Event::Telemetry(_) => EventKind::Telemetry,
            Event::DeviceArrived(_) => EventKind::DeviceArrived,
            Event::DeviceRemoved(_) => EventKind::DeviceRemoved,
            Event::Fault(_) => EventKind::Fault,
            Event::Log(_) => EventKind::Log,
        }
    }

    pub fn device(&self) -> Option<DeviceId> {
        self.devices().into_iter().next()
    }

    pub fn devices(&self) -> Vec<DeviceId> {
        match self {
            Event::OperationChanged(e) => e.devices.clone(),
            Event::PropertyChanged(e) => vec![e.device],
            Event::FrameReady(e) => vec![e.device],
            Event::ScanSignalChunk(e) => vec![e.device],
            Event::Telemetry(e) => vec![e.device],
            Event::DeviceArrived(e) => vec![e.id],
            Event::DeviceRemoved(id) => vec![*id],
            Event::Fault(e) => e.device.into_iter().collect(),
            Event::Log(_) => Vec::new(),
        }
    }

    pub fn operation(&self) -> Option<OperationId> {
        match self {
            Event::OperationChanged(event) => Some(event.operation),
            Event::PropertyChanged(_)
            | Event::FrameReady(_)
            | Event::ScanSignalChunk(_)
            | Event::Telemetry(_)
            | Event::DeviceArrived(_)
            | Event::DeviceRemoved(_)
            | Event::Fault(_)
            | Event::Log(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OperationChanged {
    pub operation: OperationId,
    pub devices: Vec<DeviceId>,
    pub status: OperationStatus,
}

#[derive(Debug, Clone)]
pub struct PropertyChanged {
    pub device: DeviceId,
    pub key: String,
    pub value: Value,
}

#[derive(Debug, Clone)]
pub struct FrameEvent {
    pub device: DeviceId,
    pub handle: FrameHandle,
    pub width: u32,
    pub height: u32,
    pub pixel_format: String,
    pub metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub handle: FrameHandle,
    pub device: DeviceId,
    pub width: u32,
    pub height: u32,
    pub pixel_format: String,
    pub data: Vec<u8>,
    pub metadata: BTreeMap<String, Value>,
    pub buffer: FrameBufferSpec,
}

#[derive(Debug, Clone)]
pub struct TelemetryEvent {
    pub device: DeviceId,
    pub values: BTreeMap<String, Value>,
}

#[derive(Debug, Clone)]
pub struct ScanSignalChunkEvent {
    pub device: DeviceId,
    pub stream: StreamId,
    pub channels: Vec<String>,
    pub origin: Timestamp,
    pub line: u64,
    pub chunk: u64,
    pub first_sample: u64,
    pub sample_count: u64,
    pub sample_rate: Frequency,
    pub sample_period: TimeInterval,
    pub samples: BTreeMap<String, Vec<Value>>,
    pub metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Clone)]
pub struct FaultEvent {
    pub device: Option<DeviceId>,
    pub report: ErrorReport,
}

#[derive(Debug, Clone)]
pub struct LogEvent {
    pub driver: Option<DriverId>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum DriverEvent {
    Event(Event),
    FrameReady(Frame),
    TokenProgress {
        token: DriverToken,
        progress: Progress,
    },
    TokenCompleted {
        token: DriverToken,
        value: Value,
    },
    TokenFailed {
        token: DriverToken,
        report: ErrorReport,
    },
}

pub trait Transport: Send {
    fn send(&mut self, bytes: &[u8]) -> Result<()>;
    fn poll_recv(&mut self) -> Result<Option<Vec<u8>>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionToken(pub u64);

#[derive(Debug, Clone)]
pub enum SessionEvent {
    Packet {
        token: SessionToken,
        packet: Packet,
    },
    Completed {
        token: SessionToken,
        response: Packet,
    },
    Failed {
        token: SessionToken,
        report: ErrorReport,
    },
}

pub trait Session: Send {
    fn submit_packet(&mut self, packet: Packet) -> Result<SessionToken>;
    fn poll(&mut self) -> Vec<SessionEvent>;
}

pub trait Driver: Send {
    fn id(&self) -> DriverId;
    fn descriptors(&self) -> Vec<DeviceDescriptor>;
    fn resources(&self) -> Vec<ResourceDescriptor> {
        Vec::new()
    }
    fn graph(&self) -> DeviceGraph {
        let mut graph = DeviceGraph::default();
        for resource in self.resources() {
            let _ = graph.insert_node(GraphNode {
                id: resource.id.0,
                kind: NodeKind::Resource,
                label: resource.label,
            });
        }
        for device in self.descriptors() {
            let _ = graph.insert_node(GraphNode {
                id: device.id.0,
                kind: NodeKind::Device,
                label: device.label,
            });
        }
        graph
    }
    fn capabilities(&self, device: DeviceId) -> Vec<CapabilityDescriptor>;
    fn prepare(&mut self, batch: &CommandBatch) -> Result<PreparedBatch>;
    fn prepare_timing_plan(
        &mut self,
        plan: &TimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let participants = self
            .descriptors()
            .into_iter()
            .filter(|device| plan.participants.contains(&device.id))
            .map(|device| Value::I64(device.id.0 .0 as i64))
            .collect::<Vec<_>>();
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Arm(plan.clone())],
            physical_transactions: vec![PhysicalTransaction {
                resource: None,
                description: "runtime timing-plan arm preparation".into(),
                payload: Value::Map(BTreeMap::from([
                    ("driver".into(), Value::I64(self.id().0 as i64)),
                    ("participants".into(), Value::List(participants)),
                ])),
            }],
        })
    }
    fn start_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let participants = self
            .descriptors()
            .into_iter()
            .filter(|device| armed.plan.participants.contains(&device.id))
            .map(|device| Value::I64(device.id.0 .0 as i64))
            .collect::<Vec<_>>();
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Start(OperationId(0))],
            physical_transactions: vec![PhysicalTransaction {
                resource: None,
                description: "runtime timing-plan start transition".into(),
                payload: Value::Map(BTreeMap::from([
                    ("driver".into(), Value::I64(self.id().0 as i64)),
                    ("participants".into(), Value::List(participants)),
                ])),
            }],
        })
    }
    fn stop_timing_plan(
        &mut self,
        armed: &ArmedTimingPlan,
        command_id: CommandId,
    ) -> Result<PreparedBatch> {
        let participants = self
            .descriptors()
            .into_iter()
            .filter(|device| armed.plan.participants.contains(&device.id))
            .map(|device| Value::I64(device.id.0 .0 as i64))
            .collect::<Vec<_>>();
        Ok(PreparedBatch {
            id: command_id,
            commands: vec![Command::Stop(OperationId(0))],
            physical_transactions: vec![PhysicalTransaction {
                resource: None,
                description: "runtime timing-plan stop transition".into(),
                payload: Value::Map(BTreeMap::from([
                    ("driver".into(), Value::I64(self.id().0 as i64)),
                    ("participants".into(), Value::List(participants)),
                ])),
            }],
        })
    }
    fn dispatch(&mut self, prepared: PreparedBatch) -> Result<DriverToken>;
    fn poll(&mut self) -> Vec<DriverEvent>;
    fn cancel(&mut self, _token: DriverToken) -> CancelResult {
        CancelResult::Unsupported
    }
}

#[derive(Debug, Clone)]
pub struct CapabilityDependency {
    pub role: Role,
    pub node: NodeId,
    pub label: String,
    pub device: Option<DeviceDescriptor>,
}

#[derive(Debug, Clone)]
pub struct CapabilityProvider {
    pub driver: DriverId,
    pub device: DeviceDescriptor,
    pub capability: CapabilityDescriptor,
    pub dependencies: Vec<CapabilityDependency>,
}

impl CapabilityProvider {
    pub fn dependency(&self, role: &Role) -> Option<&CapabilityDependency> {
        self.dependencies
            .iter()
            .find(|dependency| &dependency.role == role)
    }

    pub fn dependency_device(&self, role: &Role) -> Option<&DeviceDescriptor> {
        self.dependency(role)
            .and_then(|dependency| dependency.device.as_ref())
    }

    pub fn has_dependency_devices(&self, roles: &[Role]) -> bool {
        roles
            .iter()
            .all(|role| self.dependency_device(role).is_some())
    }
}

pub fn capability_providers<'a>(
    drivers: impl IntoIterator<Item = &'a dyn Driver>,
    kind: CapabilityKind,
) -> Vec<CapabilityProvider> {
    let mut providers = Vec::new();
    for driver in drivers {
        let devices = driver.descriptors();
        let graph = driver.graph();
        for device in &devices {
            let Some(capability) = driver
                .capabilities(device.id)
                .into_iter()
                .filter(|capability| !capability.is_hidden_maintenance())
                .find(|capability| capability.kind == kind)
            else {
                continue;
            };

            let dependencies = graph
                .edges()
                .iter()
                .filter_map(|edge| match &edge.kind {
                    EdgeKind::UsesDevice { role } if edge.to == device.id.0 => {
                        Some(CapabilityDependency {
                            role: role.clone(),
                            node: edge.from,
                            label: graph_node_label(&graph, edge.from),
                            device: devices
                                .iter()
                                .find(|candidate| candidate.id.0 == edge.from)
                                .cloned(),
                        })
                    }
                    _ => None,
                })
                .collect();

            providers.push(CapabilityProvider {
                driver: driver.id(),
                device: device.clone(),
                capability,
                dependencies,
            });
        }
    }
    providers
}

fn graph_node_label(graph: &DeviceGraph, id: NodeId) -> String {
    graph
        .nodes()
        .find(|node| node.id == id)
        .map(|node| node.label.clone())
        .unwrap_or_else(|| format!("{id:?}"))
}
