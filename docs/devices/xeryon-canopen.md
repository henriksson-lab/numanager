# Xeryon Integrated CANopen Stages

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::xeryon_canopen` |
| Families | Xeryon XLA/XUMU integrated-controller devices using CANopen/CiA 402 |
| Support level | Configured CiA 402 axis model with optional live SocketCAN or SLCAN NMT/SDO execution, EDS object parsing, typed motion/home/stop planning/execution, and readback refresh helpers |
| Protocol evidence | Xeryon integrated-controller manual states CAN communication follows CiA 402; Xeryon CANopen examples publish EDS/example material and default Node ID context |
| Transport | Planned CANopen transactions by default; optional Linux SocketCAN behind `os-can` or serial SLCAN behind `os-serial` when `connect = true` |
| Discovery | Configured discovery only |
| Validation | No hardware validation |
| Runtime/evidence notes | Use `xeryon_ascii` for XD-M/XD-C/XD-OEM serial controllers; this page covers integrated XLA/XUMU CANopen devices |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `xeryon-canopen-hub` | `hub`, `motion.controller`, `canopen`, `cia402`, `xeryon.integrated` | Owns one planned CANopen node resource |
| `xeryon-canopen-axis` | `axis.x`, `stage.axis`, `motion.stage`, `canopen.cia402.axis`, `xeryon.integrated.axis` | One configured CiA 402 logical axis |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `xeryon-canopen-planned-bus`, `xeryon-canopen-socketcan-bus`, or `xeryon-canopen-slcan-bus` | `canopen.planned`, `canopen.socketcan`, or `canopen.slcan` | Records CANopen COB-IDs and 8-byte SDO payloads; with `connect = true`, sends NMT/SDO frames and validates SDO replies |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `StageMove` | Axis device | `CapabilityRequest::StageMove` with X target | Position/status map plus CANopen transactions | Uses CiA 402 Profile Position mode, target position, optional profile velocity/acceleration, and controlword transition writes; live mode validates SDO ACKs | `position` is sequenceable through software timing endpoints |
| `StageHome` | Axis device | `None` | Homed-position map plus CANopen transactions | Uses CiA 402 Homing mode and homing-start controlword writes; hardware homing completion still needs validation | Not sequenceable |
| `StageStop` | Axis device | `None` | Quick-stop transaction | Uses a CiA 402 quick-stop controlword write | Not sequenceable |
| `GenericCommand` | Hub or axis | `refresh_readbacks`, `refresh_status`, or `refresh_axis_summary` | Readback transactions and cached state | Uses SDO uploads for statusword, actual position, target position, and mode display; live mode parses upload replies | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `node_id` | Hub | `I64` | none | R | configured | No | NMT/SDO COB-ID base |
| `can_backend` | Hub | `String` | none | R | `planned`, `socketcan`, or `slcan` | No | Config/transport metadata |
| `connected` | Hub | `Bool` | none | R | transport-open state | No | Runtime transport state |
| `device_profile` | Hub | `String` | none | R | configured, normally `CiA 402` | No | Config/evidence metadata |
| `eds_path` | Hub | `String` | none | R | configured path | No | Config metadata only |
| `eds_status` | Hub | `String` | none | R | parse result | No | EDS parser result |
| `eds_object_count` | Hub | `I64` | count | R | parsed object entries | No | EDS parser result |
| `eds_objects` | Hub | `List` | none | R | parsed object dictionary metadata | No | EDS parser result |
| `last_transactions` | Hub | `List` | none | R | most recent planned frames | No | Runtime transaction cache |
| `last_can_frames` | Hub | `List` | none | R | frames sent in last live execution | No | Runtime CAN write cache |
| `state_summary` | Hub | `Map` | none | R | current configured state | No | Runtime cache |
| `position` | Axis | `Position` | um | R/W | configured limits | Yes | `0x607A` target-position write; cached actual position until live readback exists |
| `target` | Axis | `Position` | um | R/W | configured limits | No | `0x607A` target-position cache |
| `velocity` | Axis | `Velocity` | um/s | R/W | `0..500000` advertised until model-specific validation exists | No | `0x6081` profile velocity |
| `acceleration` | Axis | `Acceleration` | um/s^2 | R/W | `0..5000000` advertised until model-specific validation exists | No | `0x6083` profile acceleration |
| `statusword` | Axis | `I64` | none | R | cached | No | planned `0x6041` upload |
| `mode_of_operation` | Axis | `I64` | none | R | cached | No | `0x6060` write / planned `0x6061` upload |
| `low_limit`, `high_limit` | Axis | `Position` | um | R | configured | No | Config metadata |
| `encoder_unit` | Axis | `Position` | um | R | configured conversion | No | inverse of configured `encoder_units_per_um` |
| `axis_summary` | Axis | `Map` | none | R | current configured state | No | Runtime cache |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "xeryon_canopen"`, `"xeryon_integrated"`, or `"xeryon_xla_integrated"` | Yes | string | Selects the integrated CANopen provider |
| `property.node_id` | No | `I64` | CANopen node id, default `32` based on Xeryon examples |
| `property.connect` | No | `Bool` | If true, opens the configured CAN backend and executes NMT/SDO transactions |
| `property.can_backend` | No | string | `planned` by default when disconnected; `socketcan` or `slcan` for live transport |
| `property.can_interface` | For SocketCAN | string | Linux CAN network interface, for example `can0` |
| `property.serial_port` | For SLCAN | string | Serial device for a Lawicel/SLCAN adapter |
| `property.serial_baud_rate` / `property.baud_rate` | For SLCAN | `I64` | Serial baud rate, default `115200` |
| `property.slcan_bitrate_code` | No | `String`/`I64` | Optional Lawicel bitrate command code `S0`..`S8`; accepts values such as `"S6"`, `"6"`, or `6` |
| `property.slcan_open` | No | `Bool` | If true, sends Lawicel `O` after optional bitrate setup before CANopen traffic |
| `property.can_timeout_ms` | No | `I64` | SDO reply timeout, default `50` |
| `property.stage_model` | No | string | Persistent stage model label |
| `property.eds_path` | No | string | Path to an EDS file; the driver parses object sections and exposes object metadata |
| `property.require_eds` | No | `Bool` | If true, configured discovery fails when `eds_path` cannot be read or parsed |
| `property.encoder_units_per_um` | Yes for physical-unit correctness | `F64`/`I64` | Native counts per micrometer for target/actual position conversion |
| `property.low_limit`, `property.high_limit` | No | `Position` | Configured travel range; legacy scalar aliases `low_limit_um` and `high_limit_um` are accepted |
| `property.position`, `property.target` | No | `Position` | Initial configured position/target |
| `property.velocity` | No | `Velocity` | Initial profile velocity |
| `property.acceleration` | No | `Acceleration` | Initial profile acceleration |
| `property.homing_method` | No | `I64` | Optional CiA 402 homing method value to write before homing |

The implemented object set is the standard CiA 402 subset: controlword
`0x6040`, statusword `0x6041`, modes of operation `0x6060`, mode display
`0x6061`, actual position `0x6064`, target position `0x607A`, profile velocity
`0x6081`, profile acceleration `0x6083`, profile deceleration `0x6084`, and
homing method `0x6098`.

Common Lawicel/SLCAN bitrate codes are adapter-defined but usually follow the
standard mapping: `S0` 10 kbit/s, `S1` 20 kbit/s, `S2` 50 kbit/s, `S3` 100
kbit/s, `S4` 125 kbit/s, `S5` 250 kbit/s, `S6` 500 kbit/s, `S7` 800 kbit/s,
and `S8` 1 Mbit/s. Leave `slcan_bitrate_code` unset when the adapter is already
configured externally.

## Examples

| Example | Demonstrates |
| --- | --- |
| `discover_devices` | Shows configured integrated Xeryon CANopen devices in the discovery flow |
| `motion_stage` | Generic `StageMove` and software timing endpoint planning through the common stage API |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate NMT state handling, SDO acknowledgements, CiA 402 state-machine transitions, homing completion, position scaling, limit/fault statusword bits, and target-reached behavior |
| EDS semantics | Current parser records object metadata; model-specific limits, scaling, PDO mappings, and supported modes still need audited mapping into public properties |
| Multi-node | Daisy-chain node discovery and remultiplexing need real CAN traffic or EDS-backed inventory |
