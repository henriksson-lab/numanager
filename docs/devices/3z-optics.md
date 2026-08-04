# 3Z Optics IRIS Light Sources

## Status

| Field | Value |
| --- | --- |
| Driver module | `numanager_drivers::three_z_optics` |
| Families | 3Z Optics IRIS LED light sources, including IRIS-400, IRIS-400HP/P, and IRIS-600HP/P class devices |
| Support level | Source-backed configured discovery plus opt-in Modbus-style serial control/readback for mode, global output, global intensity, channel output, channel intensity, model id, and dirty-bit refresh |
| Protocol evidence | Micro-Manager `3Z_Optics` adapter source; official product pages confirm IRIS serial/TTL/controller modes and product-level channel/wavelength/intensity specs |
| Transport | USB serial / Modbus RTU-style frames, slave address `0x01` |
| Discovery | Configured discovery; live serial requires `connect = true` and a configured `serial_port` |
| Validation | Driver compile-checked; real 3Z hardware validation pending |
| Evidence gaps | Official register map, serial line settings, model-id catalog, model JSON catalog, fault behavior, and optical output validation |

Protocol details are recorded in
[`../reverse/3z-optics-protocol.md`](../reverse/3z-optics-protocol.md).

## Logical Devices

| Device | Kind tags | Role |
| --- | --- | --- |
| `3z-optics-hub` | `hub`, `light.engine`, `shutter`, `serial.modbus_rtu` | Owns the serial session, mode, software shutter/global output state, global intensity, model id, dirty bit, and readback refresh commands |
| `3z-channel-1..N` | `light.source`, `led.channel`, `trigger.sink` | Individual LED channels with enable/selection, intensity, configured wavelength, and label |

## Resources

| Resource | Kind | Notes |
| --- | --- | --- |
| `3z-optics-serial` | `serial.modbus_rtu` | Shared serial resource; metadata records configured port, default serial settings, slave address, connected state, and Micro-Manager source provenance |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing |
| --- | --- | --- | --- | --- | --- |
| `TriggerSink` | Hub | `None` or `CapabilityRequest::Trigger` | `Bool` | Writes mapped global/channel switch state when connected; configured acceptance otherwise | `enabled` and `global_intensity` are sequenceable |
| `GenericCommand` | Hub | `refresh_identity`, `refresh_readbacks`, or `poll_dirty` with no params | refreshed value or summary map | Sends only mapped register/coil reads; no raw Modbus command surface | No |
| `TriggerSink` | Channel | `None` or `CapabilityRequest::Trigger` | `Bool` | Writes channel switch coil when connected; configured acceptance otherwise | `enabled` and `selected` are sequenceable |
| `Dac` | Channel | `CapabilityRequest::Dac` with `Ratio` | `Ratio` | Writes channel intensity holding register when connected; configured acceptance otherwise | `intensity` is sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums | Sequenceable | Mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `product` | Hub | `String` | none | R | configured product string | No | Config/model metadata |
| `serial_number` | Hub | `String` | none | R | configured identity | No | Config metadata |
| `serial_port` | Hub | `String` | none | R | configured port or empty | No | Serial resource |
| `connected` | Hub | `Bool` | none | R | true when live serial is open | No | Runtime transport state |
| `serial_timeout` | Hub | `TimeInterval` | ms | R | configured read window | No | Config metadata |
| `model_id` | Hub | `I64` | none | R | input register value | No | Input register `0x01` |
| `mode` | Hub | `String` | none | R/W | `Global`, `Independent`, `TTL` | No | Holding register `0x20` |
| `brightness_min` | Hub | `I64` | native scalar | R | configured/model metadata | No | Adapter model JSON |
| `brightness_max` | Hub | `I64` | native scalar | R | configured/model metadata | No | Adapter model JSON |
| `enabled` | Hub | `Bool` | none | R/W | `true`/`false` | Yes | Global switch coil `0x30` in global mode; software shutter plus channel coils otherwise |
| `global_intensity` | Hub | `Ratio` | percent | R/W | `brightness_min..=brightness_max` | Yes | Holding register `0x30` in global mode |
| `dirty` | Hub | `Bool` | none | R | `true`/`false` | No | Coil `0x21` |
| `last_transaction` | Hub | `Map` | none | R | action, function, live-serial flag, byte counts | No | Diagnostic transaction summary |
| `enabled` | Channel | `Bool` | none | R/W | `true`/`false` | Yes | Coil `0x31 + channel_index` |
| `selected` | Channel | `Bool` | none | R/W | `true`/`false` | Yes | Alias for channel enable |
| `intensity` | Channel | `Ratio` | percent | R/W | `brightness_min..=brightness_max` | Yes | Holding register `0x31 + channel_index` |
| `wavelength` | Channel | `Wavelength` | nm | R | configured/model metadata | No | Product/model metadata |
| `label` | Channel | `String` | none | R | configured/model metadata | No | Product/model metadata |

## Config

| Key | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "3z_optics"` | Yes | string | Selects the 3Z Optics configured provider; aliases `3z` and `3Z_Optics` are accepted |
| `serial_port` | Required when `connect = true` | string | Serial port path/name |
| `connect` | No | `Bool` | Open the serial port and use live Modbus-style transactions |
| `serial_timeout_ms` | No | `I64` or `TimeInterval` | Serial read window; default 500 ms |
| `product`, `serial_number`, `model_id` | No | string / integer | Discovery-lock identity and configured model metadata |
| `mode` | No | string enum | Initial mode; `Global`, `Independent`, or `TTL` |
| `brightness_min`, `brightness_max` | No | integer | Native brightness scalar limits; default `0..100` |
| `enabled`, `global_intensity` | No | `Bool`, `Ratio` | Initial hub state |
| `channel_count` | No | integer | Number of channels to expose, clamped to `1..=16` |
| `channel_1_label..channel_N_label` | No | string | Configured channel labels |
| `channel_1_wavelength..channel_N_wavelength` | No | `Wavelength` | Configured channel wavelengths |
| `channel_1_enabled..channel_N_enabled` | No | `Bool` | Initial channel states |
| `channel_1_intensity..channel_N_intensity` | No | `Ratio` | Initial channel intensities |

## Examples

| Example | Coverage |
| --- | --- |
| `light_source` with a configured 3Z device | Generic light-source `Dac` and `TriggerSink`, typed intensity/enable properties, timing-plan endpoint application, completion waits, and readback |
| `discover_devices` with a configured 3Z device | Shows the hub, channel devices, serial resource, capabilities, and typed properties |

## Remaining Work

| Area | Needed evidence |
| --- | --- |
| Hardware validation | Record exact model, firmware/software version, serial settings, model id, channel inventory, enable/intensity output behavior, dirty-bit behavior, completion, and any fault/interlock response |
| Official protocol | Obtain the official 3Z serial/register manual if available and compare every mapped address, mode value, model id, brightness range, and reply rule |
| Serial settings | The Micro-Manager adapter relies on the configured serial port; validate baud/parity/stop-bit defaults for each IRIS controller class |
| Model metadata | Pin model ids and channel labels/limits from official model files, vendor docs, or hardware readback |
| Safety | Identify fault, interlock, overtemperature, overcurrent, and output-disable semantics before advertising safety readbacks |
