# Cephla Squid

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::squid` |
| Families | Cephla Squid/Octopi controller-style firmware |
| Support level | Protocol-backed control plus opt-in configured real serial startup status ingestion, runtime transport, and named generic command aliases |
| Protocol evidence | Open Squid controller firmware and host-wrapper protocol notes in [`../reverse/squid-protocol.md`](../reverse/squid-protocol.md) |
| Transport | Fixed-length binary command/status frames over serial-like `Transport`; configured real serial uses 2,000,000 baud USB serial |
| Discovery | Simulated discovery plus config-backed discovery; configured real serial drains any immediately available status frame before registration; no active port scan |
| Validation | Local simulated transport and real serial frame backend compile; real controller validation pending |
| Runtime/evidence notes | Real serial requires `numanager-drivers/os-serial` and explicit `connect = true` |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `squid-controller` | `hub`, `serial.controller` | Owns one serial controller resource |
| `squid-xy-stage` | `stage.xy` | X/Y share controller frame stream |
| `squid-z-stage` | `stage.z` | Shares controller frame stream; dependency for autofocus |
| `squid-theta` | `stage.theta` | Shares controller frame stream |
| `squid-filter-wheel-w` | `filter.wheel` | Shares controller frame stream |
| `squid-filter-wheel-w2` | `filter.wheel` | Shares controller frame stream |
| `squid-autofocus` | `autofocus` | General autofocus provider backed by Squid firmware pin 15; depends on Z and illumination |
| `squid-led-matrix` | `light.source`, `illumination.matrix` | LED matrix pattern/color command on the shared controller |
| `squid-illumination-d1..d5` | `light.source`, `illumination.port` | Per-port logical light devices on one controller |
| `squid-trigger-1..4` | `trigger.source`, `camera.trigger` | Per-channel logical trigger outputs on one controller |
| `squid-onboard-dac-1..8` | `analog.output`, `diagnostic.raw` | Diagnostic raw DAC80508 channel-count outputs on one controller |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `squid-serial-controller` | `usb.serial` | Fixed-length binary command/status frame path shared by controller, motion, illumination, trigger, filter, and autofocus commands; resource metadata records configured `serial_port`, `baud_rate`, `connected` state, and last decoded status-frame metadata |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `StageMove` | XY and Z stages | `CapabilityRequest::StageMove` absolute/relative position targets | Map with serial-frame counts and final position state | Squid status frame matching command id | Stage property sequences through runtime timing plans |
| `StageHome` | XY and Z stages | `CapabilityRequest::None` | Map with serial-frame counts and zeroed position state | Squid status frame matching command id | Direct capability workflow |
| `Dac` | Illumination ports | `CapabilityRequest::Dac` percent intensity value | Map with serial-frame counts and intensity state | Squid status frame matching command id | Light property sequences through runtime timing plans |
| `TriggerSource` | Trigger outputs | `CapabilityRequest::Trigger` pulse | Map/string | Squid status frame matching command id | Trigger pulse emitted on timing start |
| `Autofocus` | `squid-autofocus` | `CapabilityRequest::Autofocus` | Provider-neutral autofocus state map | Squid status frame matching command id | Enable/disable bool sequences; acquisition-plan integration is not exposed because timing and coordination evidence is absent |
| `GenericCommand` | Hub | `disable_all_ports` or `heartbeat` with no params | Map with serial-frame counts | Squid status frame matching command id | Bring-up/diagnostic only; reset-like maintenance commands remain hidden from regular and advanced command surfaces |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `firmware_version` | Controller | `String` | none | R | none | No | Status frame firmware fields |
| `watchdog_timeout` | Controller | `TimeInterval` | s | R/W | 0..3600 s fixture clamp | No | Watchdog timeout command |
| `x` | XY stage | `Position` | um | R/W | fixture range | Yes | `MoveToX` |
| `y` | XY stage | `Position` | um | R/W | fixture range | Yes | `MoveToY` |
| `z` | Z stage | `Position` | um | R/W | fixture range | Yes | `MoveToZ` |
| `position_steps` | Theta | `StepCount` | steps | R/W | i32 command payload | No | `MoveTheta`; scalar steps accepted as legacy writes |
| `position` | Filter wheels | `StepCount` | steps | R/W | i32 command payload | No | `MoveToW`, `MoveToW2`; scalar steps accepted as legacy writes |
| `enabled` | Illumination ports | `Bool` | none | R/W | none | Yes | Port enable command |
| `intensity` | Illumination ports | `Ratio` | percent | R/W | 0..100 | Yes | Port intensity command |
| `wavelength` | Illumination ports | `Wavelength` | named wavelength value | R | configured 405/488/561/638/730 nm | No | Descriptor metadata |
| `pattern` | LED matrix | `String` | none | R/W | `FullArray`, `LeftHalf`, `RightHalf`, `LeftBlueRightRed`, `LowNa`, `LeftDot`, `RightDot`, `TopHalf`, `BottomHalf`, `ExternalFet` | Yes | `SET_ILLUMINATION_LED_MATRIX` pattern byte |
| `red`, `green`, `blue` | LED matrix | `Ratio` | percent | R/W | 0..100 host clamp to byte payload | Yes | `SET_ILLUMINATION_LED_MATRIX` color bytes |
| `raw_counts` | Onboard DAC channels | `I64` | counts | R/W | 0..65535 | No | `ANALOG_WRITE_ONBOARD_DAC` channel/count payload |
| `mode` | Trigger outputs | `String` | none | R/W | fixture trigger modes | No | Trigger mode command |
| `enabled` | Autofocus | `Bool` | none | R/W | none | Yes | `SET_PIN_LEVEL` pin 15 |
| `mode` | Autofocus | `String` | none | R | autofocus mode labels | No | Local provider state |
| `status` | Autofocus | `String` | none | R | provider status labels | No | Local provider state |
| `focus_score` | Autofocus | `F64` | none | R | none | No | Local provider state |
| `kind` | Autofocus | `String` | none | R | `laser triangulation` | No | Descriptor/local provider state |
| `laser_enabled` | Autofocus | `Bool` | none | R/W | deprecated alias | Yes | `SET_PIN_LEVEL` pin 15 |

## Config Keys

| Key | Type | Status | Meaning |
| --- | --- | --- | --- |
| `driver = "squid"` | string | Canonical | Selects Squid configured discovery |
| `serial_port` | string | Required for real serial | OS serial port name |
| `baud_rate` | integer | Optional | Defaults to `2000000` |
| `serial_timeout_ms` | integer | Optional | Defaults to `200` |
| `connect` | bool | Optional | When true, opens the configured serial port behind `numanager-drivers/os-serial` |
| `accept_zero_status_crc` | bool | Compatibility | Accepts legacy status frames whose CRC byte is zero |

`GenericCommand` accepts only the fixed hub/theta/filter aliases listed above.
It does not expose arbitrary Squid command codes, payload bytes, serial frames,
or shared serial discovery.

The LED matrix uses typed pattern/color properties over the documented matrix
command. Onboard DAC channels expose only diagnostic `raw_counts` because the
available protocol evidence defines the 16-bit wire value but not a calibrated
voltage range. Direct arbitrary `SET_PIN_LEVEL` access remains intentionally
unexposed.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- squid` | Config-backed discovery, graph dependencies, controller demultiplexing, illumination, triggers, autofocus invocation, timing-plan stage/light/autofocus sequences, trigger pulse on start, and hardware-owned command completion |
| `cargo run -p numanager-examples -- autofocus` | Provider-neutral autofocus selection across Squid and simulation-backed providers |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate frame encoding, CRC/status handling, command id completion, and motion/trigger semantics against real Squid hardware |
| Autofocus | Move Squid-specific autofocus behavior further into the generic autofocus contract where possible |
| Discovery | Config-to-device graph reconciliation beyond the fixed Squid device graph |
| Timing | Hardware-accurate acquisition-plan integration beyond current software sequence endpoints and trigger pulse |
| Protocol | Current evidenced motion, homing, illumination-port, LED-matrix, trigger, watchdog, autofocus-pin, and onboard-DAC command paths are implemented; stage-configuration, limit, PID, strobe-delay, DAC reference, and arbitrary pin/configuration commands need a safe typed endpoint plus source/trace/bench evidence for side effects before exposure |
