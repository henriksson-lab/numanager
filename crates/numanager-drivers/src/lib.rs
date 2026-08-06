pub mod abs_camera;
pub mod agilent_laser_combiner;
pub mod andor_camera;
pub mod arduino;
pub mod arduino_counter;
pub mod asi;
pub mod bluebox_niji;
// Not a device family: third-party firmware images compiled into the binary,
// shared by the drivers that have to reload a device after a power cycle.
mod bundled_firmware;
pub mod chuo_seiki_qt;
pub mod cobolt;
pub mod coherent_obis;
pub mod coolled;
pub mod corvus;
pub mod egrabber_framegrabber;
pub mod esp32;
pub mod evident_ix85;
pub mod genicam;
pub mod gige_vision;
pub mod hamilton_mvp;
pub mod lumencor;
pub mod lumenera;
pub mod marzhauser;
pub mod mcl;
pub mod mightex_bls;
pub mod mightex_camera;
pub mod modbus;
pub mod okolab;
pub mod omicron;
pub mod openstage;
pub mod opentrons_ot2;
pub mod openuc2;
pub mod photometrics_pvcam;
pub mod pi_gcs;
pub mod platform_camera;
pub mod prior;
pub mod sim;
pub mod sim_lsm;
mod sim_lsm_model;
pub mod sim_microscope;
pub mod sim_microscope_lsm;
pub mod sim_plate_reader;
pub mod sim_sample;
pub mod spark;
pub mod spark_cyto;
pub mod spectral_lmm5;
pub mod squid;
pub mod standa;
pub mod starlight_xpress;
pub mod sutter_mp285;
pub mod sutter_stage;
pub mod teensy_pulse;
pub mod thorlabs_apt;
pub mod thorlabs_dc;
pub mod thorlabs_kurios;
pub mod thorlabs_sc10;
pub mod three_z_optics;
pub mod toupcam;
pub mod triggerscope;
pub mod trinamic_tmcl;
pub mod usb3_vision;
pub mod usb_discovery;
pub mod velleman;
/// Windows USB access provisioning. Always compiled on Windows; elsewhere it
/// needs the `winusb` feature (see the module docs).
#[cfg(any(windows, feature = "winusb"))]
pub mod winusb_access;
pub mod wosm;
pub mod xeryon;
pub mod xeryon_canopen;
pub mod zaber;

use numanager_core::config::{DeviceConfig, HardwareConfig};
use numanager_core::runtime::DiscoveryRegistry;
use numanager_core::Result;
use numanager_core::{
    Acceleration, ByteCount, Decibel, ElectricCurrent, PixelCount, Position, Ratio, Temperature,
    TimeInterval, Value, Velocity, Voltage, Wavelength,
};

const BUILTIN_DISCOVERY_ID_BLOCK: u64 = 100;

pub fn builtin_demo_hardware_config() -> HardwareConfig {
    let hardware_config = HardwareConfig {
        devices: vec![
            DeviceConfig::new(
                28_001,
                "Configured Mightex Sirius BLS",
                "mightex_bls",
                std::collections::BTreeMap::from([
                    ("vendor_id".into(), Value::String("0x1234".into())),
                    ("product_id".into(), Value::String("0x5678".into())),
                    ("family".into(), Value::String("Mightex BLS".into())),
                ]),
            ),
            DeviceConfig::new(
                33_001,
                "Configured platform fixture camera",
                "platform_camera",
                std::collections::BTreeMap::from([
                    ("backend".into(), Value::String("fixture".into())),
                    ("width".into(), Value::PixelCount(PixelCount::new(800))),
                    ("height".into(), Value::PixelCount(PixelCount::new(600))),
                    (
                        "exposure".into(),
                        Value::TimeInterval(TimeInterval::from_milliseconds(12.5)),
                    ),
                    ("gain".into(), Value::Ratio(Ratio::from_percent(125.0))),
                    ("pixel_format".into(), Value::String("Mono8".into())),
                    (
                        "frame_interval".into(),
                        Value::TimeInterval(TimeInterval::from_milliseconds(40.0)),
                    ),
                    (
                        "fixture_path".into(),
                        Value::String("biological-fixture://gel-scene".into()),
                    ),
                ]),
            ),
            DeviceConfig::new(
                32_001,
                "Configured Toupcam geometry",
                "toupcam",
                std::collections::BTreeMap::from([
                    (
                        "product".into(),
                        Value::String("Configured Toupcam-compatible camera".into()),
                    ),
                    (
                        "serial_number".into(),
                        Value::String("TOUP-CONFIG-0001".into()),
                    ),
                    (
                        "sensor_width".into(),
                        Value::PixelCount(PixelCount::new(1920)),
                    ),
                    (
                        "sensor_height".into(),
                        Value::PixelCount(PixelCount::new(1080)),
                    ),
                    ("roi_width".into(), Value::PixelCount(PixelCount::new(1280))),
                    ("roi_height".into(), Value::PixelCount(PixelCount::new(720))),
                    (
                        "exposure".into(),
                        Value::TimeInterval(TimeInterval::from_milliseconds(25.0)),
                    ),
                    ("gain".into(), Value::Ratio(Ratio::from_percent(150.0))),
                    ("pixel_format".into(), Value::String("Raw8".into())),
                    ("bayer_phase".into(), Value::String("Unknown".into())),
                ]),
            ),
            DeviceConfig::new(
                34_001,
                "Configured GigE Vision fixture camera",
                "gige_vision",
                std::collections::BTreeMap::from([
                    (
                        "serial_number".into(),
                        Value::String("GV-CONFIG-0002".into()),
                    ),
                    ("width".into(), Value::PixelCount(PixelCount::new(1024))),
                    ("height".into(), Value::PixelCount(PixelCount::new(768))),
                    (
                        "exposure".into(),
                        Value::TimeInterval(TimeInterval::from_milliseconds(8.0)),
                    ),
                    ("gain".into(), Value::Decibel(Decibel::new(3.0))),
                    ("pixel_format".into(), Value::String("Mono8".into())),
                    ("packet_size".into(), Value::ByteCount(ByteCount::new(1500))),
                    (
                        "inter_packet_delay".into(),
                        Value::TimeInterval(TimeInterval::from_microseconds(2.0)),
                    ),
                    ("stream_channel_port".into(), Value::I64(49160)),
                ]),
            ),
            DeviceConfig::new(
                35_001,
                "Configured USB3 Vision fixture camera",
                "usb3_vision",
                std::collections::BTreeMap::from([
                    (
                        "serial_number".into(),
                        Value::String("U3V-CONFIG-0002".into()),
                    ),
                    ("width".into(), Value::PixelCount(PixelCount::new(1280))),
                    ("height".into(), Value::PixelCount(PixelCount::new(720))),
                    (
                        "exposure".into(),
                        Value::TimeInterval(TimeInterval::from_milliseconds(6.0)),
                    ),
                    ("gain".into(), Value::Decibel(Decibel::new(2.0))),
                    ("pixel_format".into(), Value::String("Mono8".into())),
                    (
                        "transfer_size".into(),
                        Value::ByteCount(ByteCount::new(1_048_576)),
                    ),
                    ("transfer_queue_depth".into(), Value::I64(12)),
                    ("stream_endpoint".into(), Value::I64(1)),
                ]),
            ),
            DeviceConfig::new(
                36_001,
                "Configured GenICam local node-map camera",
                "genicam",
                std::collections::BTreeMap::from([
                    ("vendor".into(), Value::String("GenICam".into())),
                    (
                        "model".into(),
                        Value::String("Configured Local NodeMap".into()),
                    ),
                    (
                        "serial_number".into(),
                        Value::String("GENICAM-CONFIG-0002".into()),
                    ),
                    ("transport".into(), Value::String("fixture".into())),
                ]),
            ),
            DeviceConfig::new(
                68_001,
                "Configured Agilent Laser Combiner",
                "agilent_laser_combiner",
                std::collections::BTreeMap::from([
                    (
                        "model".into(),
                        Value::String("Agilent Laser Combiner".into()),
                    ),
                    (
                        "serial_number".into(),
                        Value::String("AGILENT-CONFIG-0002".into()),
                    ),
                    (
                        "firmware_version".into(),
                        Value::String("0.12 configured".into()),
                    ),
                    (
                        "hardware_version".into(),
                        Value::String("configured".into()),
                    ),
                    ("line_count".into(), Value::I64(4)),
                    (
                        "line_1_wavelength".into(),
                        Value::Wavelength(Wavelength::from_nanometers(405.0)),
                    ),
                    (
                        "line_2_wavelength".into(),
                        Value::Wavelength(Wavelength::from_nanometers(488.0)),
                    ),
                    (
                        "line_3_wavelength".into(),
                        Value::Wavelength(Wavelength::from_nanometers(561.0)),
                    ),
                    (
                        "line_4_wavelength".into(),
                        Value::Wavelength(Wavelength::from_nanometers(640.0)),
                    ),
                ]),
            ),
            DeviceConfig::new(
                69_001,
                "Configured Arduino controller",
                "arduino",
                std::collections::BTreeMap::from([
                    (
                        "controller_id".into(),
                        Value::String("ARDUINO-CONFIG-0002".into()),
                    ),
                    ("version".into(), Value::I64(4)),
                    ("extended_version".into(), Value::I64(4)),
                    ("pattern_count".into(), Value::I64(6)),
                    ("dac_channels".into(), Value::I64(2)),
                    ("digital_pins".into(), Value::I64(8)),
                ]),
            ),
            DeviceConfig::new(
                70_001,
                "Configured Arduino Counter",
                "arduino_counter",
                std::collections::BTreeMap::from([
                    (
                        "gate".into(),
                        Value::TimeInterval(TimeInterval::from_milliseconds(100.0)),
                    ),
                    (
                        "interval".into(),
                        Value::TimeInterval(TimeInterval::from_milliseconds(1.0)),
                    ),
                    ("count".into(), Value::I64(42)),
                    ("pulse_level".into(), Value::Bool(false)),
                ]),
            ),
            DeviceConfig::new(
                71_001,
                "Configured ESP32 controller",
                "esp32",
                std::collections::BTreeMap::from([
                    ("firmware".into(), Value::String("MM-ESP32,5".into())),
                    (
                        "x_travel".into(),
                        Value::Position(Position::from_micrometers(75_000.0)),
                    ),
                    (
                        "y_travel".into(),
                        Value::Position(Position::from_micrometers(75_000.0)),
                    ),
                    (
                        "z_travel".into(),
                        Value::Position(Position::from_micrometers(20_000.0)),
                    ),
                    ("pwm_channels".into(), Value::I64(4)),
                ]),
            ),
            DeviceConfig::new(
                72_001,
                "Configured OpenUC2 Feather controller",
                "openuc2",
                std::collections::BTreeMap::from([
                    ("controller".into(), Value::String("OpenUC2 Feather".into())),
                    (
                        "x_travel".into(),
                        Value::Position(Position::from_micrometers(50_000.0)),
                    ),
                    (
                        "y_travel".into(),
                        Value::Position(Position::from_micrometers(50_000.0)),
                    ),
                    (
                        "z_travel".into(),
                        Value::Position(Position::from_micrometers(10_000.0)),
                    ),
                    (
                        "laser_wavelength".into(),
                        Value::Wavelength(Wavelength::from_nanometers(488.0)),
                    ),
                ]),
            ),
            DeviceConfig::new(
                73_001,
                "Configured Teensy pulse generator",
                "teensy_pulse",
                std::collections::BTreeMap::from([
                    ("version".into(), Value::I64(1)),
                    (
                        "interval".into(),
                        Value::TimeInterval(TimeInterval::from_milliseconds(100.0)),
                    ),
                    (
                        "duration".into(),
                        Value::TimeInterval(TimeInterval::from_milliseconds(1.0)),
                    ),
                    ("wait_for_input".into(), Value::Bool(false)),
                ]),
            ),
            DeviceConfig::new(
                38_001,
                "Configured Standa 8SMC4 axis",
                "standa-8smc",
                std::collections::BTreeMap::from([
                    ("controller".into(), Value::String("8SMC4-USB".into())),
                    (
                        "serial_number".into(),
                        Value::String("STANDA-CONFIG-0002".into()),
                    ),
                    ("axis".into(), Value::String("x".into())),
                    (
                        "travel".into(),
                        Value::Position(Position::from_micrometers(50_000.0)),
                    ),
                    (
                        "step_size".into(),
                        Value::Position(Position::from_micrometers(0.15625)),
                    ),
                    (
                        "velocity".into(),
                        Value::Velocity(Velocity::from_micrometers_per_second(2_000.0)),
                    ),
                    (
                        "acceleration".into(),
                        Value::Acceleration(Acceleration::from_micrometers_per_second_squared(
                            20_000.0,
                        )),
                    ),
                ]),
            ),
            DeviceConfig::new(
                39_001,
                "Configured Hamilton Serial MVP valve",
                "hamilton_mvp",
                std::collections::BTreeMap::from([
                    ("model".into(), Value::String("Serial MVP".into())),
                    (
                        "serial_number".into(),
                        Value::String("MVP-CONFIG-0002".into()),
                    ),
                    ("address".into(), Value::String("a".into())),
                    ("port_count".into(), Value::I64(8)),
                    ("position".into(), Value::I64(1)),
                    ("firmware".into(), Value::String("MV configured".into())),
                ]),
            ),
            DeviceConfig::new(
                40_001,
                "Configured Trinamic TMCL stage controller",
                "trinamic_tmcl",
                std::collections::BTreeMap::from([
                    ("model".into(), Value::String("TMCM-3212-TMCL".into())),
                    (
                        "serial_number".into(),
                        Value::String("TMCL-CONFIG-0002".into()),
                    ),
                    ("module_address".into(), Value::I64(1)),
                    ("host_address".into(), Value::I64(2)),
                    ("axes".into(), Value::I64(3)),
                    (
                        "step_size".into(),
                        Value::Position(Position::from_micrometers(0.1)),
                    ),
                    (
                        "travel".into(),
                        Value::Position(Position::from_micrometers(25_000.0)),
                    ),
                    ("max_positioning_speed".into(), Value::I64(51_200)),
                    ("max_acceleration".into(), Value::I64(10_000)),
                ]),
            ),
            DeviceConfig::new(
                41_001,
                "Configured Velleman K8055 IO board",
                "velleman",
                std::collections::BTreeMap::from([
                    (
                        "serial_number".into(),
                        Value::String("K8055-CONFIG-0002".into()),
                    ),
                    ("board_address".into(), Value::I64(0)),
                    ("digital_output_mask".into(), Value::I64(0)),
                    ("digital_input_mask".into(), Value::I64(3)),
                    (
                        "analog_output_1".into(),
                        Value::Ratio(Ratio::from_percent(0.0)),
                    ),
                    (
                        "analog_output_2".into(),
                        Value::Ratio(Ratio::from_percent(50.0)),
                    ),
                    (
                        "analog_input_1".into(),
                        Value::Ratio(Ratio::from_percent(25.0)),
                    ),
                    (
                        "analog_input_2".into(),
                        Value::Ratio(Ratio::from_percent(75.0)),
                    ),
                ]),
            ),
            DeviceConfig::new(
                42_001,
                "Configured Velleman K8061 IO board",
                "k8061",
                std::collections::BTreeMap::from([
                    (
                        "serial_number".into(),
                        Value::String("K8061-CONFIG-0002".into()),
                    ),
                    ("board_address".into(), Value::I64(1)),
                    ("digital_output_mask".into(), Value::I64(0)),
                    ("digital_input_mask".into(), Value::I64(12)),
                    (
                        "analog_output_1".into(),
                        Value::Ratio(Ratio::from_percent(25.0)),
                    ),
                    (
                        "analog_output_8".into(),
                        Value::Ratio(Ratio::from_percent(75.0)),
                    ),
                    (
                        "analog_input_1".into(),
                        Value::Ratio(Ratio::from_percent(10.0)),
                    ),
                    (
                        "analog_input_8".into(),
                        Value::Ratio(Ratio::from_percent(90.0)),
                    ),
                    ("pwm_output".into(), Value::Ratio(Ratio::from_percent(33.0))),
                ]),
            ),
            DeviceConfig::new(
                43_001,
                "Configured Starlight Xpress filter wheel",
                "starlight_xpress",
                std::collections::BTreeMap::from([
                    (
                        "product".into(),
                        Value::String("SX Universal/Maxi USB Filter Wheel".into()),
                    ),
                    (
                        "serial_number".into(),
                        Value::String("SXFW-CONFIG-0002".into()),
                    ),
                    ("positions".into(), Value::I64(7)),
                    ("position".into(), Value::I64(3)),
                    ("completion_polls".into(), Value::I64(20)),
                ]),
            ),
            DeviceConfig::new(
                44_001,
                "Configured Spectral LMM5",
                "spectral_lmm5",
                std::collections::BTreeMap::from([
                    (
                        "product".into(),
                        Value::String("Laser Merge Module LMM5".into()),
                    ),
                    (
                        "serial_number".into(),
                        Value::String("LMM5-CONFIG-0002".into()),
                    ),
                    ("line_count".into(), Value::I64(5)),
                    (
                        "line_1_wavelength".into(),
                        Value::Wavelength(Wavelength::from_nanometers(405.0)),
                    ),
                    (
                        "line_2_wavelength".into(),
                        Value::Wavelength(Wavelength::from_nanometers(440.0)),
                    ),
                    (
                        "line_3_wavelength".into(),
                        Value::Wavelength(Wavelength::from_nanometers(488.0)),
                    ),
                    (
                        "line_4_wavelength".into(),
                        Value::Wavelength(Wavelength::from_nanometers(561.0)),
                    ),
                    (
                        "line_5_wavelength".into(),
                        Value::Wavelength(Wavelength::from_nanometers(640.0)),
                    ),
                    (
                        "line_3_transmission".into(),
                        Value::Ratio(Ratio::from_percent(10.0)),
                    ),
                ]),
            ),
            DeviceConfig::new(
                45_001,
                "Configured OpenStage controller",
                "openstage",
                std::collections::BTreeMap::from([
                    (
                        "product".into(),
                        Value::String("OpenStage Arduino Mega controller".into()),
                    ),
                    (
                        "serial_number".into(),
                        Value::String("OPENSTAGE-CONFIG-0002".into()),
                    ),
                    (
                        "x".into(),
                        Value::Position(Position::from_micrometers(1000.0)),
                    ),
                    (
                        "y".into(),
                        Value::Position(Position::from_micrometers(2000.0)),
                    ),
                    (
                        "z".into(),
                        Value::Position(Position::from_micrometers(100.0)),
                    ),
                    (
                        "x_travel".into(),
                        Value::Position(Position::from_micrometers(50_000.0)),
                    ),
                    (
                        "y_travel".into(),
                        Value::Position(Position::from_micrometers(50_000.0)),
                    ),
                    (
                        "z_travel".into(),
                        Value::Position(Position::from_micrometers(10_000.0)),
                    ),
                    (
                        "step_size".into(),
                        Value::Position(Position::from_micrometers(1.0)),
                    ),
                    ("speed_mode".into(), Value::I64(2)),
                ]),
            ),
            DeviceConfig::new(
                46_001,
                "Configured WOSM controller",
                "wosm",
                std::collections::BTreeMap::from([
                    ("host".into(), Value::String("192.168.10.100".into())),
                    ("port".into(), Value::I64(23)),
                    (
                        "product".into(),
                        Value::String("Warwick Open-Source Microscope controller".into()),
                    ),
                    (
                        "serial_number".into(),
                        Value::String("WOSM-CONFIG-0002".into()),
                    ),
                    ("firmware_version".into(), Value::I64(99)),
                    (
                        "x".into(),
                        Value::Position(Position::from_micrometers(10.0)),
                    ),
                    (
                        "y".into(),
                        Value::Position(Position::from_micrometers(15.0)),
                    ),
                    ("z".into(), Value::Position(Position::from_micrometers(5.0))),
                    (
                        "x_travel".into(),
                        Value::Position(Position::from_micrometers(100.0)),
                    ),
                    (
                        "y_travel".into(),
                        Value::Position(Position::from_micrometers(100.0)),
                    ),
                    (
                        "z_travel".into(),
                        Value::Position(Position::from_micrometers(100.0)),
                    ),
                    (
                        "light_1_output".into(),
                        Value::Ratio(Ratio::from_percent(5.0)),
                    ),
                    (
                        "light_2_output".into(),
                        Value::Ratio(Ratio::from_percent(0.0)),
                    ),
                    (
                        "light_3_output".into(),
                        Value::Ratio(Ratio::from_percent(0.0)),
                    ),
                    (
                        "light_4_output".into(),
                        Value::Ratio(Ratio::from_percent(0.0)),
                    ),
                ]),
            ),
            DeviceConfig::new(
                47_001,
                "Configured TriggerScope controller",
                "triggerscope",
                std::collections::BTreeMap::from([
                    (
                        "product".into(),
                        Value::String("ARC TriggerScope 16".into()),
                    ),
                    (
                        "serial_number".into(),
                        Value::String("TRIGGERSCOPE-CONFIG-0002".into()),
                    ),
                    (
                        "firmware_version".into(),
                        Value::String("ARC TRIGGERSCOPE 16 v1.65".into()),
                    ),
                    ("dac_bits".into(), Value::I64(16)),
                    ("ttl_count".into(), Value::I64(4)),
                    ("dac_count".into(), Value::I64(4)),
                    ("cam_count".into(), Value::I64(2)),
                    (
                        "focus".into(),
                        Value::Position(Position::from_micrometers(250.0)),
                    ),
                    (
                        "focus_lower".into(),
                        Value::Position(Position::from_micrometers(0.0)),
                    ),
                    (
                        "focus_upper".into(),
                        Value::Position(Position::from_micrometers(1000.0)),
                    ),
                    (
                        "dac_1_voltage".into(),
                        Value::Voltage(Voltage::from_volts(1.0)),
                    ),
                    (
                        "dac_2_voltage".into(),
                        Value::Voltage(Voltage::from_volts(0.0)),
                    ),
                    (
                        "dac_3_voltage".into(),
                        Value::Voltage(Voltage::from_volts(0.0)),
                    ),
                    (
                        "dac_4_voltage".into(),
                        Value::Voltage(Voltage::from_volts(0.0)),
                    ),
                ]),
            ),
            DeviceConfig::new(
                48_001,
                "Configured Chuo Seiki QT controller",
                "chuo_seiki_qt",
                std::collections::BTreeMap::from([
                    (
                        "product".into(),
                        Value::String("Chuo Seiki QT-series controller".into()),
                    ),
                    (
                        "serial_number".into(),
                        Value::String("CHUO-QT-CONFIG-0002".into()),
                    ),
                    ("expose_z".into(), Value::Bool(true)),
                    ("z_axis".into(), Value::String("C".into())),
                    (
                        "x".into(),
                        Value::Position(Position::from_micrometers(1_000.0)),
                    ),
                    (
                        "y".into(),
                        Value::Position(Position::from_micrometers(2_000.0)),
                    ),
                    (
                        "z".into(),
                        Value::Position(Position::from_micrometers(500.0)),
                    ),
                    (
                        "x_travel".into(),
                        Value::Position(Position::from_micrometers(100_000.0)),
                    ),
                    (
                        "y_travel".into(),
                        Value::Position(Position::from_micrometers(100_000.0)),
                    ),
                    (
                        "z_travel".into(),
                        Value::Position(Position::from_micrometers(25_000.0)),
                    ),
                    (
                        "step_size".into(),
                        Value::Position(Position::from_micrometers(1.0)),
                    ),
                    ("high_speed".into(), Value::I64(2_000)),
                    ("low_speed".into(), Value::I64(500)),
                    (
                        "acceleration_time".into(),
                        Value::TimeInterval(TimeInterval::from_milliseconds(100.0)),
                    ),
                ]),
            ),
            DeviceConfig::new(
                49_001,
                "Configured ITK Corvus controller",
                "corvus",
                std::collections::BTreeMap::from([
                    (
                        "product".into(),
                        Value::String("Marzhauser/ITK Corvus controller".into()),
                    ),
                    (
                        "serial_number".into(),
                        Value::String("CORVUS-CONFIG-0002".into()),
                    ),
                    ("baud_rate".into(), Value::I64(115_200)),
                    ("version".into(), Value::String("configured".into())),
                    ("expose_z".into(), Value::Bool(true)),
                    (
                        "x".into(),
                        Value::Position(Position::from_micrometers(1_000.0)),
                    ),
                    (
                        "y".into(),
                        Value::Position(Position::from_micrometers(1_500.0)),
                    ),
                    (
                        "z".into(),
                        Value::Position(Position::from_micrometers(250.0)),
                    ),
                    (
                        "x_travel".into(),
                        Value::Position(Position::from_micrometers(100_000.0)),
                    ),
                    (
                        "y_travel".into(),
                        Value::Position(Position::from_micrometers(100_000.0)),
                    ),
                    (
                        "z_travel".into(),
                        Value::Position(Position::from_micrometers(25_000.0)),
                    ),
                    (
                        "speed".into(),
                        Value::Velocity(Velocity::from_millimeters_per_second(40.0)),
                    ),
                    (
                        "acceleration".into(),
                        Value::Acceleration(Acceleration::from_meters_per_second_squared(0.2)),
                    ),
                    ("joystick_enabled".into(), Value::Bool(false)),
                ]),
            ),
            DeviceConfig::new(
                50_001,
                "Configured Bluebox Optics niji",
                "bluebox_niji",
                std::collections::BTreeMap::from([
                    (
                        "product".into(),
                        Value::String("Bluebox Optics niji LED illuminator".into()),
                    ),
                    (
                        "serial_number".into(),
                        Value::String("NIJI-CONFIG-0002".into()),
                    ),
                    (
                        "firmware_version".into(),
                        Value::String("V2.101.000 configured".into()),
                    ),
                    ("enabled".into(), Value::Bool(false)),
                    (
                        "global_intensity".into(),
                        Value::Ratio(Ratio::from_percent(100.0)),
                    ),
                    ("channel_1_enabled".into(), Value::Bool(false)),
                    (
                        "channel_1_intensity".into(),
                        Value::Ratio(Ratio::from_percent(10.0)),
                    ),
                    (
                        "channel_1_wavelength".into(),
                        Value::Wavelength(Wavelength::from_nanometers(395.0)),
                    ),
                    ("trigger_source".into(), Value::String("Internal".into())),
                    ("trigger_logic".into(), Value::String("ActiveHigh".into())),
                    ("trigger_resistor".into(), Value::String("PullUp".into())),
                    (
                        "output_mode".into(),
                        Value::String("ConstantCurrent".into()),
                    ),
                ]),
            ),
            DeviceConfig::new(
                51_001,
                "Configured Opentrons OT-2 robot",
                "opentrons_ot2",
                std::collections::BTreeMap::from([
                    ("host".into(), Value::String("opentrons-ot2.local".into())),
                    ("api_version".into(), Value::String("2".into())),
                    ("server_version".into(), Value::String("configured".into())),
                    (
                        "robot_serial".into(),
                        Value::String("OT2-CONFIG-0002".into()),
                    ),
                    ("robot_type".into(), Value::String("OT-2".into())),
                    ("status".into(), Value::String("idle".into())),
                    ("door_open".into(), Value::Bool(false)),
                    ("current_run".into(), Value::String("none".into())),
                    (
                        "left_pipette_model".into(),
                        Value::String("p300_single_gen2".into()),
                    ),
                    (
                        "left_pipette_serial".into(),
                        Value::String("PIP-L-CONFIG-0002".into()),
                    ),
                    ("right_pipette_model".into(), Value::String("none".into())),
                    ("camera_present".into(), Value::Bool(true)),
                    (
                        "module_model".into(),
                        Value::String("temperatureModuleV2".into()),
                    ),
                    (
                        "module_serial".into(),
                        Value::String("TEMP-MOD-CONFIG-0002".into()),
                    ),
                    (
                        "module_temperature".into(),
                        Value::Temperature(Temperature::from_celsius(22.0)),
                    ),
                    (
                        "module_target_temperature".into(),
                        Value::Temperature(Temperature::from_celsius(4.0)),
                    ),
                ]),
            ),
            DeviceConfig::new(
                52_001,
                "Configured Thorlabs SC10 shutter controller",
                "thorlabs_sc10",
                std::collections::BTreeMap::from([
                    ("model".into(), Value::String("SC10".into())),
                    (
                        "serial_number".into(),
                        Value::String("SC10-CONFIG-0002".into()),
                    ),
                    (
                        "firmware_version".into(),
                        Value::String("configured".into()),
                    ),
                    ("mode".into(), Value::String("Manual".into())),
                    ("enabled".into(), Value::Bool(false)),
                    (
                        "open_time".into(),
                        Value::TimeInterval(TimeInterval::from_milliseconds(10.0)),
                    ),
                    (
                        "close_time".into(),
                        Value::TimeInterval(TimeInterval::from_milliseconds(10.0)),
                    ),
                    ("trigger_mode".into(), Value::String("Internal".into())),
                    ("repeat_count".into(), Value::I64(1)),
                    ("interlock_closed".into(), Value::Bool(true)),
                    ("fault".into(), Value::Bool(false)),
                ]),
            ),
            DeviceConfig::new(
                53_001,
                "Configured CoolLED pE-340",
                "coolled-pe340",
                std::collections::BTreeMap::from([
                    ("model".into(), Value::String("CoolLED pE-340".into())),
                    ("version".into(), Value::String("configured".into())),
                    (
                        "device_prefix".into(),
                        Value::String("coolled-pe340".into()),
                    ),
                    (
                        "wavelengths_nm".into(),
                        Value::String(
                            "365,385,405,435,460,470,490,500,525,550,580,595,635,660,740,770"
                                .into(),
                        ),
                    ),
                ]),
            ),
            DeviceConfig::new(
                54_001,
                "Configured Andor SDK2 camera",
                "andor_camera",
                std::collections::BTreeMap::from([
                    ("vendor_id".into(), Value::String("0x136e".into())),
                    ("product_id".into(), Value::String("0x0012".into())),
                    ("product".into(), Value::String("Andor iXon Ultra".into())),
                    (
                        "serial_number".into(),
                        Value::String("ANDOR-CONFIG-0002".into()),
                    ),
                    ("identity".into(), Value::Bytes(vec![0, 0, 0, 0, 0, 0])),
                    ("status_byte".into(), Value::I64(0)),
                    ("firmware_loaded".into(), Value::Bool(true)),
                ]),
            ),
            DeviceConfig::new(
                55_001,
                "Configured Photometrics PVCAM camera",
                "photometrics_pvcam",
                std::collections::BTreeMap::from([
                    (
                        "camera_name".into(),
                        Value::String("PVCAM-CONFIG-0002".into()),
                    ),
                    (
                        "product".into(),
                        Value::String("Photometrics Prime BSI Express".into()),
                    ),
                    (
                        "serial_number".into(),
                        Value::String("PVCAM-CONFIG-0002".into()),
                    ),
                    (
                        "chip_name".into(),
                        Value::String("configured sCMOS sensor".into()),
                    ),
                    (
                        "firmware_version".into(),
                        Value::String("configured".into()),
                    ),
                    ("interface_type".into(), Value::String("USB".into())),
                    (
                        "sensor_width".into(),
                        Value::PixelCount(PixelCount::new(2048)),
                    ),
                    (
                        "sensor_height".into(),
                        Value::PixelCount(PixelCount::new(2048)),
                    ),
                    ("bit_depth".into(), Value::I64(16)),
                    ("pixel_format".into(), Value::String("Mono16".into())),
                    (
                        "sensor_temperature".into(),
                        Value::Temperature(Temperature::from_celsius(-20.0)),
                    ),
                    (
                        "temperature_setpoint".into(),
                        Value::Temperature(Temperature::from_celsius(-20.0)),
                    ),
                ]),
            ),
            DeviceConfig::new(
                58_001,
                "Configured ABS camera reverse engineered support",
                "abs_camera",
                std::collections::BTreeMap::from([
                    ("product".into(), Value::String("ABS CamUSB camera".into())),
                    (
                        "serial_number".into(),
                        Value::String("ABS-CONFIG-0002".into()),
                    ),
                    (
                        "transport_hint".into(),
                        Value::String("vendor runtime or platform route required".into()),
                    ),
                ]),
            ),
            DeviceConfig::new(
                59_001,
                "Configured Mightex camera reverse engineered support",
                "mightex_camera",
                std::collections::BTreeMap::from([
                    (
                        "product".into(),
                        Value::String("Mightex buffered USB camera".into()),
                    ),
                    (
                        "serial_number".into(),
                        Value::String("MIGHTEX-CAM-CONFIG-0002".into()),
                    ),
                    (
                        "endpoint_hint".into(),
                        Value::String("bulk endpoint evidence exists; frame layout unknown".into()),
                    ),
                ]),
            ),
            DeviceConfig::new(
                60_001,
                "Configured MCL reverse engineered support",
                "mcl",
                std::collections::BTreeMap::from([
                    (
                        "product".into(),
                        Value::String("Mad City Labs MicroDrive/NanoDrive".into()),
                    ),
                    (
                        "serial_number".into(),
                        Value::String("MCL-CONFIG-0002".into()),
                    ),
                    ("family".into(), Value::String("MicroDrive".into())),
                    ("vendor_id".into(), Value::String("0x1569".into())),
                    ("product_id".into(), Value::String("0x2588".into())),
                    ("axis_count".into(), Value::I64(3)),
                    ("raw_status".into(), Value::I64(0)),
                    ("encoder_count_1".into(), Value::I64(1250)),
                    ("encoder_count_2".into(), Value::I64(-250)),
                    ("encoder_count_3".into(), Value::I64(0)),
                ]),
            ),
            DeviceConfig::new(
                56_001,
                "Configured Evident IX85 microscope body",
                "evident_ix85",
                std::collections::BTreeMap::from([
                    ("model".into(), Value::String("IX85".into())),
                    (
                        "serial_number".into(),
                        Value::String("IX85-CONFIG-0002".into()),
                    ),
                    (
                        "controller_version".into(),
                        Value::String("configured".into()),
                    ),
                    (
                        "unit_summary".into(),
                        Value::String("configured IX85 body".into()),
                    ),
                    (
                        "focus_position".into(),
                        Value::Position(Position::from_micrometers(125.0)),
                    ),
                    ("nosepiece_position".into(), Value::I64(1)),
                    ("light_path_position".into(), Value::I64(2)),
                    ("mirror_unit_1_position".into(), Value::I64(3)),
                    ("dia_shutter_open".into(), Value::Bool(false)),
                    ("epi_shutter_1_open".into(), Value::Bool(false)),
                    (
                        "autofocus_state".into(),
                        Value::String("Unavailable".into()),
                    ),
                ]),
            ),
            DeviceConfig::new(
                57_001,
                "Configured Okolab environmental controller",
                "okolab",
                std::collections::BTreeMap::from([
                    ("product".into(), Value::String("H201 T Unit-BL".into())),
                    (
                        "serial_number".into(),
                        Value::String("OKOLAB-CONFIG-0002".into()),
                    ),
                    (
                        "firmware_version".into(),
                        Value::String("configured".into()),
                    ),
                    ("temperature_target_c".into(), Value::F64(37.0)),
                    ("temperature_actual_c".into(), Value::F64(36.8)),
                    ("co2_target_percent".into(), Value::F64(5.0)),
                    ("co2_actual_percent".into(), Value::F64(4.8)),
                ]),
            ),
            DeviceConfig::new(
                30_001,
                "Configured Thorlabs APT stage",
                "thorlabs_apt",
                std::collections::BTreeMap::from([
                    ("model".into(), Value::String("APT-compatible motor".into())),
                    (
                        "serial_number".into(),
                        Value::String("APT-CONFIG-0002".into()),
                    ),
                    ("channel".into(), Value::I64(1)),
                    (
                        "travel".into(),
                        Value::Position(Position::from_micrometers(25_000.0)),
                    ),
                    (
                        "encoder_step_size".into(),
                        Value::Position(Position::from_micrometers(0.01)),
                    ),
                ]),
            ),
            DeviceConfig::new(
                31_001,
                "Configured Thorlabs KURIOS filter",
                "thorlabs_kurios",
                std::collections::BTreeMap::from([
                    ("model".into(), Value::String("KURIOS-WB1".into())),
                    (
                        "serial_number".into(),
                        Value::String("KURIOS-CONFIG-0002".into()),
                    ),
                    (
                        "min_wavelength".into(),
                        Value::Wavelength(Wavelength::from_nanometers(420.0)),
                    ),
                    (
                        "max_wavelength".into(),
                        Value::Wavelength(Wavelength::from_nanometers(730.0)),
                    ),
                    (
                        "min_bandwidth".into(),
                        Value::Wavelength(Wavelength::from_nanometers(10.0)),
                    ),
                    (
                        "max_bandwidth".into(),
                        Value::Wavelength(Wavelength::from_nanometers(40.0)),
                    ),
                ]),
            ),
            DeviceConfig::new(
                32_001,
                "Configured Thorlabs DC4100 LED controller",
                "thorlabs_dc",
                std::collections::BTreeMap::from([
                    ("family".into(), Value::String("dc4100".into())),
                    ("model".into(), Value::String("DC4100".into())),
                    (
                        "serial_number".into(),
                        Value::String("MDC4100-CONFIG-0002".into()),
                    ),
                    (
                        "channel_wavelengths".into(),
                        Value::List(vec![
                            Value::Wavelength(Wavelength::from_nanometers(405.0)),
                            Value::Wavelength(Wavelength::from_nanometers(470.0)),
                            Value::Wavelength(Wavelength::from_nanometers(565.0)),
                            Value::Wavelength(Wavelength::from_nanometers(625.0)),
                        ]),
                    ),
                    (
                        "channel_maximum_currents".into(),
                        Value::List(vec![
                            Value::ElectricCurrent(ElectricCurrent::from_milliamps(1000.0)),
                            Value::ElectricCurrent(ElectricCurrent::from_milliamps(1000.0)),
                            Value::ElectricCurrent(ElectricCurrent::from_milliamps(1000.0)),
                            Value::ElectricCurrent(ElectricCurrent::from_milliamps(1000.0)),
                        ]),
                    ),
                ]),
            ),
        ],
        ..HardwareConfig::default()
    };
    let mightex_slc_config = HardwareConfig {
        devices: vec![DeviceConfig::new(
            37_001,
            "Configured Mightex Sirius SLC",
            "mightex_bls",
            std::collections::BTreeMap::from([
                ("vendor_id".into(), Value::String("0x1234".into())),
                ("product_id".into(), Value::String("0x5679".into())),
                ("family".into(), Value::String("Mightex SLC".into())),
                ("channel_count".into(), Value::I64(2)),
                ("module_type".into(), Value::String("CA".into())),
            ]),
        )],
        ..HardwareConfig::default()
    };
    let andor_sdk3_config = HardwareConfig {
        devices: vec![DeviceConfig::new(
            54_002,
            "Configured Andor SDK3 camera",
            "andor_sdk3",
            std::collections::BTreeMap::from([
                ("vendor_id".into(), Value::String("0x136e".into())),
                ("product_id".into(), Value::String("0x0014".into())),
                ("product".into(), Value::String("Andor Zyla USB3".into())),
                (
                    "serial_number".into(),
                    Value::String("ANDOR-SDK3-CONFIG-0002".into()),
                ),
                ("firmware_loaded".into(), Value::Bool(true)),
                ("width".into(), Value::PixelCount(PixelCount::new(2048))),
                ("height".into(), Value::PixelCount(PixelCount::new(2048))),
                (
                    "exposure".into(),
                    Value::TimeInterval(TimeInterval::from_milliseconds(10.0)),
                ),
                ("pixel_format".into(), Value::String("Mono16".into())),
                ("cycle_mode".into(), Value::String("Fixed".into())),
                ("trigger_mode".into(), Value::String("Internal".into())),
            ]),
        )],
        ..HardwareConfig::default()
    };
    let mut discovery_config = hardware_config.clone();
    discovery_config
        .devices
        .extend(mightex_slc_config.devices.clone());
    discovery_config
        .devices
        .extend(andor_sdk3_config.devices.clone());
    discovery_config
}

pub fn register_builtin_discovery(
    registry: &mut DiscoveryRegistry,
    config: &HardwareConfig,
) -> Result<()> {
    macro_rules! register_config {
        ($constructor:path) => {{
            let id = registry.reserve_driver_ids(BUILTIN_DISCOVERY_ID_BLOCK);
            registry.register($constructor(id, config)?);
        }};
    }

    register_config!(abs_camera::AbsCameraDiscovery::from_config);
    register_config!(agilent_laser_combiner::AgilentLaserCombinerDiscovery::from_config);
    register_config!(andor_camera::AndorCameraDiscovery::from_config);
    register_config!(arduino::ArduinoDiscovery::from_config);
    register_config!(arduino_counter::ArduinoCounterDiscovery::from_config);
    register_config!(asi::AsiMs2000Discovery::from_config);
    register_config!(asi::AsiTigerDiscovery::from_config);
    register_config!(bluebox_niji::NijiDiscovery::from_config);
    register_config!(chuo_seiki_qt::ChuoQtDiscovery::from_config);
    register_config!(cobolt::CoboltDiscovery::from_config);
    register_config!(coherent_obis::ObisDiscovery::from_config);
    register_config!(coolled::CoolLedPe300Discovery::from_config);
    register_config!(coolled::CoolLedPe4000Discovery::from_config);
    register_config!(coolled::CoolLedPe340Discovery::from_config);
    register_config!(corvus::CorvusDiscovery::from_config);
    register_config!(egrabber_framegrabber::EGrabberFramegrabberDiscovery::from_config);
    register_config!(esp32::Esp32Discovery::from_config);
    register_config!(evident_ix85::Ix85Discovery::from_config);
    register_config!(genicam::GenicamDiscovery::from_config);
    register_config!(gige_vision::GigEVisionDiscovery::from_config);
    register_config!(hamilton_mvp::HamiltonMvpDiscovery::from_config);
    register_config!(lumencor::LumencorSpectraDiscovery::from_config);
    register_config!(lumencor::LumencorCiaDiscovery::from_config);
    register_config!(lumenera::LumeneraDiscovery::from_config);
    register_config!(marzhauser::MarzhauserDiscovery::from_config);
    register_config!(mcl::MclDiscovery::from_config);
    register_config!(mightex_bls::MightexBlsDiscovery::from_config);
    register_config!(mightex_camera::MightexCameraDiscovery::from_config);
    register_config!(modbus::ModbusDiscovery::from_config);
    register_config!(okolab::OkolabDiscovery::from_config);
    register_config!(omicron::OmicronDiscovery::from_config);
    register_config!(openstage::OpenStageDiscovery::from_config);
    register_config!(opentrons_ot2::OpentronsOt2Discovery::from_config);
    register_config!(openuc2::OpenUc2Discovery::from_config);
    register_config!(photometrics_pvcam::PvcamDiscovery::from_config);
    register_config!(pi_gcs::PiGcsDiscovery::from_config);
    register_config!(platform_camera::PlatformCameraDiscovery::from_config);
    register_config!(spark_cyto::SparkCytoDiscovery::from_config);
    register_config!(spectral_lmm5::Lmm5Discovery::from_config);
    register_config!(squid::SquidDiscovery::from_config);
    register_config!(standa::StandaDiscovery::from_config);
    register_config!(starlight_xpress::SxFilterWheelDiscovery::from_config);
    register_config!(sutter_mp285::Mp285Discovery::from_config);
    register_config!(sutter_stage::SutterStageDiscovery::from_config);
    register_config!(teensy_pulse::TeensyPulseDiscovery::from_config);
    register_config!(thorlabs_apt::ThorlabsAptDiscovery::from_config);
    register_config!(thorlabs_dc::ThorlabsDcDiscovery::from_config);
    register_config!(thorlabs_kurios::KuriosDiscovery::from_config);
    register_config!(thorlabs_sc10::Sc10Discovery::from_config);
    register_config!(three_z_optics::ThreeZOpticsDiscovery::from_config);
    register_config!(toupcam::ToupcamDiscovery::from_config);
    register_config!(triggerscope::TriggerScopeDiscovery::from_config);
    register_config!(trinamic_tmcl::TmclDiscovery::from_config);
    register_config!(usb3_vision::Usb3VisionDiscovery::from_config);
    register_config!(velleman::VellemanDiscovery::from_config);
    register_config!(wosm::WosmDiscovery::from_config);
    register_config!(xeryon::XeryonDiscovery::from_config);
    register_config!(xeryon_canopen::XeryonCanopenDiscovery::from_config);
    register_config!(zaber::ZaberAsciiDiscovery::from_config);

    usb_discovery::register_builtin_usb_vid_pid_discovery(registry);

    #[cfg(target_os = "linux")]
    {
        let id = registry.reserve_driver_ids(BUILTIN_DISCOVERY_ID_BLOCK);
        registry.register(platform_camera::PlatformCameraDiscovery::v4l2(id));
    }

    #[cfg(feature = "os-hid")]
    {
        let id = registry.reserve_driver_ids(BUILTIN_DISCOVERY_ID_BLOCK);
        registry.register(mightex_bls::MightexBlsDiscovery::os_hid(id));
    }

    Ok(())
}

pub fn register_builtin_demo_discovery(
    registry: &mut DiscoveryRegistry,
    config: &HardwareConfig,
) -> Result<()> {
    macro_rules! register_one {
        ($constructor:path) => {{
            let id = registry.next_driver_id();
            registry.register($constructor(id));
        }};
    }

    register_one!(toupcam::ToupcamDiscovery::simulated);
    register_one!(spark_cyto::SparkCytoDiscovery::simulated);
    register_one!(squid::SquidDiscovery::simulated);
    register_one!(asi::AsiMs2000Discovery::simulated);
    register_one!(asi::AsiTigerDiscovery::simulated);
    register_one!(cobolt::CoboltDiscovery::simulated);
    register_one!(coolled::CoolLedPe4000Discovery::simulated);
    register_one!(coolled::CoolLedPe300Discovery::simulated);
    register_one!(xeryon::XeryonDiscovery::simulated);
    register_one!(xeryon_canopen::XeryonCanopenDiscovery::simulated);
    register_one!(zaber::ZaberAsciiDiscovery::simulated);
    register_one!(coherent_obis::ObisDiscovery::simulated);
    register_one!(omicron::OmicronDiscovery::simulated);
    register_one!(prior::PriorDiscovery::simulated);
    register_one!(sutter_stage::SutterStageDiscovery::simulated);
    register_one!(sutter_mp285::Mp285Discovery::simulated);
    register_one!(marzhauser::MarzhauserDiscovery::simulated);
    register_one!(pi_gcs::PiGcsDiscovery::configured_fixture);
    register_one!(thorlabs_apt::ThorlabsAptDiscovery::configured_fixture);
    register_one!(lumencor::LumencorSpectraDiscovery::configured_fixture);
    register_one!(lumencor::LumencorCiaDiscovery::configured_fixture);
    register_one!(thorlabs_dc::ThorlabsDcDiscovery::configured_fixture);
    register_one!(modbus::ModbusDiscovery::configured_fixture);
    register_one!(genicam::GenicamDiscovery::configured_fixture);
    register_one!(thorlabs_kurios::KuriosDiscovery::configured_fixture);

    register_builtin_discovery(registry, config)
}
