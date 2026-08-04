//! The Spark Cyto command vocabulary: typed enums paired with the exact ASCII tokens the
//! instrument's command parser accepts.
//!
//! **These are protocol keywords, not firmware.** Nothing here is a firmware image, a
//! loader or a vendor package; it is the spelling of the words that go over the wire. The
//! naming says `wire_token` rather than `firmware_name` so that distinction survives being
//! read quickly in a repository where "firmware" means redistributable vendor binaries.
//!
//! Every string is the exact byte sequence the instrument emits or parses. Do **not** edit
//! a keyword to look nicer: the bytes are the contract.
//!
//! Dependency-free by design (no serde, no strum). Each enum offers
//! `wire_token(&self) -> &'static str` and `from_wire_token(&str) -> Option<Self>`.

// Generate a wire-token enum with a bidirectional map.
//
// Both directions come from one declaration because writing them separately is how they
// drift: a token corrected on the encode side and missed on the decode side is a bug that
// only shows up against hardware. `from_wire_token` returns the first matching variant;
// enums whose token repeats across variants are written by hand instead (see
// FluorescenceCarrier).
macro_rules! wire_enum {
 (
 $(#[$emeta:meta])*
 pub enum $name:ident { $( $(#[$vmeta:meta])* $variant:ident = $fw:literal ),* $(,)? }
 ) => {
 $(#[$emeta])*
 #[derive(Debug, Clone, Copy, PartialEq, Eq)]
 pub enum $name { $( $(#[$vmeta])* $variant ),* }

 impl $name {
 /// The exact token this variant is spelled as on the wire.
 pub fn wire_token(&self) -> &'static str {
 match self { $( $name::$variant => $fw ),* }
 }
 /// Parse a wire token back to its variant.
 pub fn from_wire_token(s: &str) -> Option<Self> {
 match s { $( $fw => Some($name::$variant), )* _ => None }
 }
 }
 };
}

// =============================================================================
// § Measurement mode — the top-level `MODE=` value
// =============================================================================
wire_enum! {
 /// Measurement mode.
 pub enum MeasurementMode {
 Absorbance = "ABS",
 Cuvette = "CUV",
 Luminescence = "LUM",
 Alpha = "ALPHA",
 FluorescenceTop = "FITOP",
 FluorescenceBottom = "FIBOTTOM",
 FluorescencePolarization = "FP",
 Cell = "CELL",
 Injector = "INJ",
 WellTemperature = "WELL_TEMP",
 Barcode = "BARCODE",
 FluorescenceImaging = "FIM",
 }
}

// =============================================================================
// § Optics — filters / mirrors / carriers / objective / areas
// =============================================================================

wire_enum! {
 /// Optical filter type.
 /// Filter type.
 pub enum FilterType {
 Undefined = "UNDEFINED",
 Shortpass = "SP",
 Longpass = "LP",
 Bandpass = "BP",
 OpticalDensity = "OD",
 Empty = "EMPTY",
 Dark = "DARK",
 Automatic = "AUTOMATIC",
 }
}

wire_enum! {
 /// Temperature-controlled devices addressed by `TEMPERATURE DEVICE=…` and
 /// `?SENSORVALUE TEMPERATURE {device}`.
 pub enum TemperatureDevice {
 Cuvette = "CUV",
 Injector = "INJ",
 Lower = "LOWER",
 Upper = "UPPER",
 AmbientControl = "AMBIENTCONTROL",
 Heating = "HEATING",
 Cooling = "COOLING",
 Control = "CTRL",
 }
}

wire_enum! {
 /// The two injector pumps, addressed by `PUMP=`.
 pub enum InjectorPump {
 A = "A",
 B = "B",
 }
}

wire_enum! {
 /// Optical mirror type.
 /// Mirror type.
 pub enum MirrorType {
 /// 50/50 beam splitter.
 HalfHalf = "50_50",
 Dichroic = "DICHROIC",
 Bottom = "BOTTOM",
 Unused = "UNUSED",
 Automatic = "AUTOMATIC",
 }
}

///
///
/// Hand-written (not via `wire_enum!`) because `MONO` maps to **two** variants
/// (`MonochromatorExcitation` and `MonochromatorEmission`); `from_wire_token`
/// resolves `MONO` to `MonochromatorExcitation` (the lower enum value).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FluorescenceCarrier {
    FilterExcitation,
    FilterEmission,
    FilterDualEmission,
    MonochromatorExcitation,
    MonochromatorEmission,
}

impl FluorescenceCarrier {
    pub fn wire_token(&self) -> &'static str {
        match self {
            FluorescenceCarrier::FilterExcitation => "FILTER_EX",
            FluorescenceCarrier::FilterEmission => "FILTER_EM1",
            FluorescenceCarrier::FilterDualEmission => "FILTER_EM2",
            FluorescenceCarrier::MonochromatorExcitation => "MONO",
            FluorescenceCarrier::MonochromatorEmission => "MONO",
        }
    }
    pub fn from_wire_token(s: &str) -> Option<Self> {
        match s {
            "FILTER_EX" => Some(FluorescenceCarrier::FilterExcitation),
            "FILTER_EM1" => Some(FluorescenceCarrier::FilterEmission),
            "FILTER_EM2" => Some(FluorescenceCarrier::FilterDualEmission),
            // Ambiguous: MONO is both Ex and Em; pick Ex by convention. // verify against context
            "MONO" => Some(FluorescenceCarrier::MonochromatorExcitation),
            _ => None,
        }
    }
}

wire_enum! {
 /// Fluorescence measurement direction. Keywords coincide with
 /// `MeasurementMode::FluorescenceTop/Bottom`.
 ///
 pub enum FluorescenceMeasurementDirection {
 Top = "FITOP",
 Bottom = "FIBOTTOM",
 }
}

wire_enum! {
 /// Physically movable optics carriers.
 pub enum MoveableCarrier {
 ExcitationFilter = "FILTER_EX",
 EmissionFilter = "FILTER_EM1",
 DualPmtEmissionFilter = "FILTER_EM2",
 Mirror = "MIRROR1",
 DualPmtMirror = "MIRROR2",
 All = "ALL",
 }
}

wire_enum! {
 /// Position of a moveable carrier (in/out).
 /// Moveable carrier position.
 pub enum MoveableCarrierPosition {
 In = "IN",
 Out = "OUT",
 }
}

wire_enum! {
 /// A retractable hardware component.
 /// Retractable.
 pub enum Retractable {
 FilterSlide = "FILTER_SLIDE",
 }
}

wire_enum! {
 /// Imaging objective magnification.
 /// Objective type.
 pub enum ObjectiveType {
 Undefined = "XX",
 TwoTimes = "2X",
 FourTimes = "4X",
 TenTimes = "10X",
 }
}

wire_enum! {
 /// Microtiter-plate scan area size.
 /// Microtiter-plate scan area size.
 pub enum MtpAreaType {
 Small = "SMALL",
 Medium = "MEDIUM",
 Large = "LARGE",
 }
}

// =============================================================================
// § Light sources
// =============================================================================

wire_enum! {
 /// Excitation light source. The "colour" names are dual-band excitation
 /// labels, not literal colours.
 ///
 pub enum LightingName {
 Autofocus = "AF",
 Brightfield = "BF",
 Blue = "UV_BLUE",
 Green = "BLUE_GREEN",
 Red = "LIME_RED",
 FarRed = "RED_FARRED",
 }
}

wire_enum! {
 /// LED hardware class.
 /// LED hardware class.
 pub enum LightingType {
 Autofocus = "AF_LED",
 Brightfield = "BF_LED",
 Fluorescence = "FI_LED",
 }
}

wire_enum! {
 /// Status-LED colour.
 pub enum LedColor {
 Red = "RED",
 Green = "GREEN",
 Blue = "BLUE",
 Yellow = "YELLOW",
 Magenta = "MAGENTA",
 Cyan = "CYAN",
 White = "WHITE",
 Black = "BLACK",
 }
}

wire_enum! {
 /// Status-LED / instrument state.
 pub enum LedState {
 Off = "OFF",
 Standby = "STANDBY",
 Idle = "IDLE",
 IdleAcquired = "IDLE_ACQUIRED",
 Run = "RUN",
 Error = "ERROR",
 UserInteraction = "USER_INTERACTION",
 Pause = "PAUSE",
 ShutDown = "SHUT_DOWN",
 ActionImpossible = "ACTION_NOT_POSSIBLE",
 }
}

// =============================================================================
// § Plate transport / positions
// =============================================================================

wire_enum! {
 /// Plate transport position.
 /// Plate position.
 pub enum PlatePosition {
 Undefined = "UNDEFINED",
 OutLeft = "OUT_LEFT",
 OutRight = "OUT_RIGHT",
 PlateIn = "PLATE_IN",
 PickNPlace = "PICK_N_PLACE",
 LidLifter = "LIDLIFTER",
 Check = "CHECK",
 Heating = "HEATING",
 Incubation = "INCUBATION",
 Cooling = "COOLING",
 BarcodeLeft = "BARCODE_LEFT",
 BarcodeRight = "BARCODE_RIGHT",
 }
}

wire_enum! {
 /// Barcode reader position.
 /// Barcode position.
 pub enum BarcodePosition {
 Left = "LEFT",
 Right = "RIGHT",
 }
}

// =============================================================================
// § Motion
// =============================================================================

wire_enum! {
 /// Microtiter-plate stage axis.
 pub enum MtpMotor {
 X = "X",
 Y = "Y",
 Z = "Z",
 }
}

wire_enum! {
 /// Stage movement speed.
 /// Movement speed.
 pub enum MovementSpeed {
 Normal = "NORMAL",
 Smallstep = "SMALLSTEP",
 Automatic = "AUTOMATIC",
 Smooth = "SMOOTH",
 Slow = "SLOW",
 FlashInjection = "FLASH_INJECTION",
 }
}

// =============================================================================
// § Temperature / gas / power state
// =============================================================================

wire_enum! {
 /// Temperature target mode.
 pub enum TargetMode {
 Ambient = "AMBIENT",
 Fix = "FIX",
 }
}

wire_enum! {
 /// Generic on/off state.
 /// Generic on/off.
 pub enum State {
 On = "ON",
 Off = "OFF",
 }
}

// =============================================================================
// § Camera
// =============================================================================

wire_enum! {
 /// Defines the token order of `CAMERA AOI =x =y =w =h`.
 ///
 pub enum AreaOfInterestProperty {
 X = "X",
 Y = "Y",
 Width = "WIDTH",
 Height = "HEIGHT",
 }
}

wire_enum! {
 /// Camera frame-capture execution result.
 /// Camera execution details.
 pub enum CameraExecutionDetails {
 Successful = "SUCCESSFUL",
 FrameDrop = "FRAMEDROP",
 }
}

// =============================================================================
// § Hardware buttons
// =============================================================================

wire_enum! {
 /// Front-panel buttons.
 pub enum HardwareButtons {
 StartStop = "START_STOP",
 FilterOut = "FILTER_OUT",
 Plate = "PLATE",
 Injector = "INJECTOR",
 Power = "POWER",
 All = "ALL",
 }
}

// =============================================================================
// § Counters
// =============================================================================

wire_enum! {
 /// Leading keyword of `?COUNTER …`.
 /// Counter type — the leading keyword of `?COUNTER …`.
 pub enum CounterType {
 Firmware = "COUNTER",
 Software = "SW_COUNTER",
 }
}

wire_enum! {
 /// Hardware wear/usage counters.
 pub enum FirmwareCounter {
 LidLifted = "LIDLIFT_TAKEN",
 XMovement = "X_DISTANCE_MOVED",
 YMovement = "Y_DISTANCE_MOVED",
 ZMovement = "Z_DISTANCE_MOVED",
 Shaking = "SHAKING_TIME",
 HeatingStandard = "HEATING_STANDARD",
 HeatingEnhanced = "HEATING_ENHANCED",
 InstrumentOnTime = "INSTRUMENT_ONTIME",
 Flashes = "FLASH",
 LaserOnTime = "LASER_ONTIME",
 ValveSwitchesO2 = "O2_VALVE_SWITCH",
 ValveSwitchesCo2 = "CO2_VALVE_SWITCH",
 PumpedVolumeInjectorA = "PUMP_A_VOLUME",
 PumpedVolumeInjectorB = "PUMP_B_VOLUME",
 PumpedVolumeInjectorC = "PUMP_C_VOLUME",
 OnTime = "ONTIME",
 ValveSwitches = "VALVE_SWITCH",
 Reading = "READ",
 }
}

wire_enum! {
 /// Driver-side usage counters.
 pub enum SoftwareCounter {
 Plate0001Well = "PLATE_1",
 Plate0004Well = "PLATE_4",
 Plate0006Well = "PLATE_6",
 Plate0008Well = "PLATE_8",
 Plate0012Well = "PLATE_12",
 Plate0016Well = "PLATE_16",
 Plate0024Well = "PLATE_24",
 Plate0048Well = "PLATE_48",
 Plate0096Well = "PLATE_96",
 Plate0384Well = "PLATE_384",
 Plate1536Well = "PLATE_1536",
 CuvetteSlot = "CUV_SLOT",
 CuvetteAdapter = "CUV_ADPT",
 CellSlideAdapter = "CELL_ADPT",
 Nanoquant = "NANOQUANT",
 Absorbance = "MEAS_ABS",
 AbsorbanceScan = "MEAS_ABSSCAN",
 FluorescenceIntensity = "MEAS_FI",
 FluorescenceIntensityScan = "MEAS_FISCAN",
 FluorescencePolarization = "MEAS_FP",
 Luminescence = "MEAS_LUM",
 LuminescenceScan = "MEAS_LUMSCAN",
 Alpha = "MEAS_ALPHA",
 Cell = "MEAS_CELL",
 FlashDropout = "DROPOUT",
 OnBoardStart = "BUTTON_START",
 OnBoardContinue = "BUTTON_CONTINUE",
 OnBoardStop = "BUTTON_STOP",
 }
}

// =============================================================================
// § Messages / errors
// =============================================================================

wire_enum! {
 /// The `MESSAGE TYPE=` value.
 /// Instrument message type — the `MESSAGE TYPE=` value.
 pub enum InstrumentMessageType {
 All = "ALL",
 Temperature = "TEMPERATURE",
 }
}

/// The numeric firmware error codes exposed by the driver (the full code space
/// is firmware-side). Carries both the keyword and its numeric code.
/// Hand-written to expose `code`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceError {
    TemperatureSensorReading,
    InjectorCarrierInserted,
    GasTimeout,
    GasSignalCorrupt,
}

impl DeviceError {
    pub fn wire_token(&self) -> &'static str {
        match self {
            DeviceError::TemperatureSensorReading => "ERR_READING_TEMP_SENSOR",
            DeviceError::InjectorCarrierInserted => "ERR_INJ_CARRIER_INSERTED",
            DeviceError::GasTimeout => "ERR_GAS_TIMEOUT",
            DeviceError::GasSignalCorrupt => "ERR_GAS_SIGNAL_CORRUPT",
        }
    }
    /// Numeric firmware error code.
    pub fn code(&self) -> u16 {
        match self {
            DeviceError::TemperatureSensorReading => 1104,
            DeviceError::InjectorCarrierInserted => 1213,
            DeviceError::GasTimeout => 1240,
            DeviceError::GasSignalCorrupt => 1242,
        }
    }
    pub fn from_wire_token(s: &str) -> Option<Self> {
        match s {
            "ERR_READING_TEMP_SENSOR" => Some(DeviceError::TemperatureSensorReading),
            "ERR_INJ_CARRIER_INSERTED" => Some(DeviceError::InjectorCarrierInserted),
            "ERR_GAS_TIMEOUT" => Some(DeviceError::GasTimeout),
            "ERR_GAS_SIGNAL_CORRUPT" => Some(DeviceError::GasSignalCorrupt),
            _ => None,
        }
    }
    pub fn from_code(code: u16) -> Option<Self> {
        match code {
            1104 => Some(DeviceError::TemperatureSensorReading),
            1213 => Some(DeviceError::InjectorCarrierInserted),
            1240 => Some(DeviceError::GasTimeout),
            1242 => Some(DeviceError::GasSignalCorrupt),
            _ => None,
        }
    }
}

wire_enum! {
 /// The command operation prefix.
 ///
 /// Mirrors [`crate::spark::commands::Op`] (which is the builder-facing form); kept here
 /// for catalog completeness with the exact wire tokens.
 /// The command operation prefix.
 pub enum Prefix {
 /// SET / execute — no prefix character.
 Action = "",
 /// `#` — get definition / allowed range / list.
 Range = "#",
 /// `?` — get current value / state.
 Request = "?",
 }
}

// =============================================================================
// § Config-range parameter tokens (per-mode calibration/limit tables)
// These enums are NOT commands; they name the parameters returned by the
// mode-configuration range reads (`#CONFIG …`-style). Kept for completeness.
// =============================================================================

wire_enum! {
 /// Absorbance config-range parameter names.
 /// Config-range parameter names.
 pub enum AbsorbanceParam {
 ReferenceDataRangeLow = "REF_DATARANGE_LOW",
 SignalDataRangeLow = "SIG_DATARANGE_LOW",
 ReferenceDataRangeHigh = "REF_DATARANGE_HIGH",
 SignalDataRangeHigh = "SIG_DATARANGE_HIGH",
 OdMax = "OD_MAX",
 FlashDropout = "FLASH_DROPOUT",
 }
}

wire_enum! {
 /// Luminescence config-range parameter names.
 /// `TEMP_SLOPE`/`TEMP_INTERCEPT` are the ALPHA temperature-correction terms.
 /// Luminescence config-range parameter names.
 pub enum LuminescenceParam {
 DarkTimeMin = "DARKTIMEMIN",
 LidCheckMax = "LIDCHECKMAX",
 DarkMax = "DARKMAX",
 DarkTimeMax = "DARKTIMEMAX",
 AlphaTempSlope = "TEMP_SLOPE",
 AlphaTempIntercept = "TEMP_INTERCEPT",
 }
}

wire_enum! {
 /// Fluorescence config-range parameter names.
 /// Config-range parameter names.
 pub enum FluorescenceParam {
 ReferenceFilterRangeLow = "REF_FIL_RANGE_LOW",
 ReferenceFilterRangeHigh = "REF_FIL_RANGE_HIGH",
 ReferenceMonoRangeLow = "REF_MONO_RANGE_LOW",
 ReferenceMonoRangeHigh = "REF_MONO_RANGE_HIGH",
 SignalRangeLow = "SIGNAL_RANGE_LOW",
 SignalRangeHigh = "SIGNAL_RANGE_HIGH",
 FlashDropout = "FLASH_DROPOUT",
 ExWlFilterTopMin = "EX_WL_FIL_TOP_MIN",
 ExWlFilterTopMax = "EX_WL_FIL_TOP_MAX",
 EmWlFilterTopMin = "EM_WL_FIL_TOP_MIN",
 EmWlFilTopMax = "EM_WL_FIL_TOP_MAX",
 ExWlFilterBottomMin = "EX_WL_FIL_BOTTOM_MIN",
 ExWlFilterBottomMax = "EX_WL_FIL_BOTTOM_MAX",
 EmWlFilterBottomMin = "EM_WL_FIL_BOTTOM_MIN",
 EmWlFilterBottomMax = "EM_WL_FIL_BOTTOM_MAX",
 ExWlFpMin = "EX_WL_FP_MIN",
 ExWlFpMax = "EX_WL_FP_MAX",
 EmWlFpMin = "EM_WL_FP_MIN",
 EmWlFpMax = "EM_WL_FP_MAX",
 ExWlMonoMin = "EX_WL_MONO_MIN",
 ExWlMonoMax = "EX_WL_MONO_MAX",
 EmWlMonoMin = "EM_WL_MONO_MIN",
 EmWlMonoMax = "EM_WL_MONO_MAX",
 MaxChartersInFilterDescription = "MAX_CHAR_FILTERDESCR",
 }
}

wire_enum! {
 /// Camera config-range parameter names.
 /// Config-range parameter names.
 pub enum CameraParam {
 RoiStartX = "ROI_START_X",
 RoiStartY = "ROI_START_Y",
 LaserOffset = "LASER_OFFSET",
 IntTimeThin = "INT_TIME_THIN",
 IntTimeThick = "INT_TIME_THICK",
 }
}

wire_enum! {
 /// Plate-transport config-range parameter names.
 /// Config-range parameter names.
 pub enum PlateTransportParam {
 MinPlateHeight = "MIN_PLATEHEIGHT",
 MaxPlateHeight = "MAX_PLATEHEIGHT",
 PlateReferenceWidth = "PLATE_REF_WIDTH",
 MaxPlateFormat = "MAX_PLATEFORMAT",
 DeltaWellTempLumX = "DELTAX_WELLTEMP_LUM",
 DeltaWellTempLumY = "DELTAY_WELLTEMP_LUM",
 }
}

// =============================================================================
// § Units
// The wire unit token (inside `[...]` of a range reply) is the exact unit
// name, parsed case-sensitively. Values are integer-scaled per the divisor
// noted below.
// =============================================================================

/// Physical unit of a range/value reply. `wire_token` returns the exact
/// token the firmware places inside `[...]` (e.g. `ang`, `c100`, `ulPerS`).
///
/// See the per-variant docs for the scale/divisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// dimensionless — bracket omitted on the wire.
    None,
    /// seconds
    Seconds,
    /// nanoseconds
    Nanoseconds,
    /// microseconds (micro-sign form)
    MicrosecondsSign,
    /// microseconds (ASCII form)
    MicrosecondsAscii,
    /// micrometres (µm) — lengths / positions
    Micrometres,
    /// metres (generic length; e.g. altitude)
    Metres,
    /// percent (%)
    Percent,
    /// motor steps (raw stepper counts)
    Step,
    /// milliseconds
    Milliseconds,
    /// frames per second (camera)
    Fps,
    /// milli-hertz (0.001 Hz)
    MilliHertz,
    /// tens/decimation of hertz — value scaled by 10 (semantics unconfirmed)
    Hz10,
    /// ångström (0.1 nm) — wavelengths/bandwidths are in ångström, not nm
    Angstrom,
    /// hertz
    Hertz,
    /// centi-°C (0.01 °C) — temperatures are integer hundredths of a degree
    CentiCelsius,
    /// microlitres (micro-sign)
    MicrolitresSign,
    /// microlitres (ASCII)
    MicrolitresAscii,
    /// microlitres per second (µl/s) — injector dispense speed
    MicrolitresPerSecond,
    /// deci-hertz (0.1 Hz)
    DeciHertz,
    /// parts per million (gas concentration)
    Ppm,
    /// hours
    Hours,
    /// millilitres
    Millilitres,
    /// kilohertz
    KiloHertz,
}

impl Unit {
    /// Exact on-wire unit token.
    pub fn wire_token(&self) -> &'static str {
        match self {
            Unit::None => "None",
            Unit::Seconds => "s",
            Unit::Nanoseconds => "ns",
            Unit::MicrosecondsSign => "µs",
            Unit::MicrosecondsAscii => "us",
            Unit::Micrometres => "um",
            Unit::Metres => "m",
            Unit::Percent => "percent",
            Unit::Step => "step",
            Unit::Milliseconds => "ms",
            Unit::Fps => "fps",
            Unit::MilliHertz => "mHz",
            Unit::Hz10 => "hz10",
            Unit::Angstrom => "ang",
            Unit::Hertz => "hz",
            Unit::CentiCelsius => "c100",
            Unit::MicrolitresSign => "µl",
            Unit::MicrolitresAscii => "ul",
            Unit::MicrolitresPerSecond => "ulPerS",
            Unit::DeciHertz => "dHz",
            Unit::Ppm => "ppm",
            Unit::Hours => "h",
            Unit::Millilitres => "ml",
            Unit::KiloHertz => "kHz",
        }
    }
    /// Parse an on-wire unit token (case-sensitive).
    pub fn from_wire_token(s: &str) -> Option<Self> {
        match s {
            "None" => Some(Unit::None),
            "s" => Some(Unit::Seconds),
            "ns" => Some(Unit::Nanoseconds),
            "µs" => Some(Unit::MicrosecondsSign),
            "us" => Some(Unit::MicrosecondsAscii),
            "um" => Some(Unit::Micrometres),
            "m" => Some(Unit::Metres),
            "percent" => Some(Unit::Percent),
            "step" => Some(Unit::Step),
            "ms" => Some(Unit::Milliseconds),
            "fps" => Some(Unit::Fps),
            "mHz" => Some(Unit::MilliHertz),
            "hz10" => Some(Unit::Hz10),
            "ang" => Some(Unit::Angstrom),
            "hz" => Some(Unit::Hertz),
            "c100" => Some(Unit::CentiCelsius),
            "µl" => Some(Unit::MicrolitresSign),
            "ul" => Some(Unit::MicrolitresAscii),
            "ulPerS" => Some(Unit::MicrolitresPerSecond),
            "dHz" => Some(Unit::DeciHertz),
            "ppm" => Some(Unit::Ppm),
            "h" => Some(Unit::Hours),
            "ml" => Some(Unit::Millilitres),
            "kHz" => Some(Unit::KiloHertz),
            _ => None,
        }
    }
}

// =============================================================================
// § Module-info / config keyword literals
// 32 constant keywords used by module info / config commands.
// =============================================================================

/// The 32 module-info / config keyword constants.
pub mod literals {
    pub const ALIAS: &str = "ALIAS";
    pub const MODULE_NUMBER: &str = "MODULE_NUMBER";
    pub const CLEAR: &str = "CLEAR";
    pub const MODULE_MODE: &str = "MODULE_MODE";
    pub const CONFIG: &str = "CONFIG";
    pub const MODULE_TYPE: &str = "MODULE_TYPE";
    pub const DYNAMIC: &str = "DYNAMIC";
    pub const NAME: &str = "NAME";
    pub const ERROR: &str = "ERROR";
    pub const NUMBER: &str = "NUMBER";
    pub const EXPECTED: &str = "EXPECTED";
    pub const OPERATIONAL: &str = "OPERATIONAL";
    pub const EXPECTED_CAN: &str = "EXPECTED_CAN";
    pub const SAP_NR_INSTRUMENT: &str = "SAP_NR_INSTRUMENT";
    pub const EXPECTED_USB: &str = "EXPECTED_USB";
    pub const SAP_NR_MODULE: &str = "SAP_NR_MODULE";
    pub const FUNCTION: &str = "FUNCTION";
    pub const SAP_SERIAL_INSTR: &str = "SAP_SERIAL_INSTR";
    pub const IDENTIFICATION: &str = "IDENTIFICATION";
    pub const SAP_SERIAL_MODULE: &str = "SAP_SERIAL_MODULE";
    pub const INDEX: &str = "INDEX";
    pub const SUB: &str = "SUB";
    pub const INFO: &str = "INFO";
    pub const TEXT: &str = "TEXT";
    pub const RESET: &str = "RESET";
    pub const TIME: &str = "TIME";
    pub const INSTRUMENT_TYPE: &str = "INSTRUMENT_TYPE";
    pub const VERSION: &str = "VERSION";
    pub const HARDWARE_VERSION: &str = "HARDWARE_VERSION";
    pub const MAX: &str = "MAX";
    pub const LASTERROR: &str = "LASTERROR";
    pub const MODULE: &str = "MODULE";

    /// All 32 literals, for exhaustiveness checks.
    pub const ALL: [&str; 32] = [
        ALIAS,
        MODULE_NUMBER,
        CLEAR,
        MODULE_MODE,
        CONFIG,
        MODULE_TYPE,
        DYNAMIC,
        NAME,
        ERROR,
        NUMBER,
        EXPECTED,
        OPERATIONAL,
        EXPECTED_CAN,
        SAP_NR_INSTRUMENT,
        EXPECTED_USB,
        SAP_NR_MODULE,
        FUNCTION,
        SAP_SERIAL_INSTR,
        IDENTIFICATION,
        SAP_SERIAL_MODULE,
        INDEX,
        SUB,
        INFO,
        TEXT,
        RESET,
        TIME,
        INSTRUMENT_TYPE,
        VERSION,
        HARDWARE_VERSION,
        MAX,
        LASTERROR,
        MODULE,
    ];
}
