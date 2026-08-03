# Thorlabs DC LED Controllers

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::thorlabs_dc` |
| Families | Thorlabs DC2010, DC2100, DC3100, DC2200, DC4100/DC4104 LED controllers |
| Support level | Configured opt-in serial control/readback, explicit-config DC2200 USBTMC control/readback, and refresh helpers |
| Protocol evidence | Public command manuals, SCPI-style DC2200 command strings, and USBTMC DEV_DEP bulk-message framing |
| Transport | Serial ASCII over `SerialIo`; DC2200 SCPI over USBTMC bulk endpoints when VID/PID/interface/endpoints are configured |
| Discovery | Config-backed discovery; live serial/USBTMC requires configured endpoints and explicit connect |
| Validation | Configured serial and USBTMC startup-readback/control paths are implemented; real hardware validation pending |
| Runtime/evidence notes | `numanager-drivers/os-serial` for explicitly constructed real serial ports; `numanager-drivers/os-usb` for explicit-config DC2200 USBTMC |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `thorlabs-dc*` controller | `led.controller`, `light.source`, `trigger.sink` | Single-output families expose controller as light source |
| `thorlabs-dc4100-led-*` | `light.source`, `led.channel`, `trigger.sink` | DC4100 channels share controller resource |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `thorlabs-dc-serial` | `serial` | Serial command path with CRLF framing and hardware error-query completion metadata; resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state |
| `thorlabs-dc2200-usbtmc` | `usb.usbtmc` | Explicit-config DC2200 SCPI USBTMC DEV_DEP bulk transport; resource metadata records configured USB VID/PID, interface, bulk endpoints, read size, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `TriggerSink` | Controller/channels | `None` or `CapabilityRequest::Trigger` | Output status map | Runtime token after command; output query replies refresh cached state | Controller/channel output endpoints |
| `Dac` | Single-output controller or DC4100 channel | `CapabilityRequest::Dac`; controller values are `ElectricCurrent`, channel values are `ElectricCurrent` or `Ratio` | Output setpoint map | Runtime token after command; current/brightness query replies refresh cached state | Current/brightness timing endpoints |
| `GenericCommand` | Controller | `refresh_readbacks`, `refresh_output`, `refresh_setpoints`, `refresh_status`, or `refresh_identity` with no params | Map with refreshed values and command count | Uses only mapped controller query readbacks; no arbitrary serial, SCPI, USBTMC, or setter command surface | Not sequenceable |
| `GenericCommand` | DC4100 channel | `refresh_readbacks`, `refresh_output`, `refresh_setpoints`, or `refresh_identity` with no params | Map with refreshed values and command count | Uses only mapped channel query readbacks; no arbitrary serial command or setter command surface | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `enabled` | Controller/channel | `Bool` | none | R/W | none | Yes | `o?` / `o? <ch>` readback; `o` writes |
| `operation_mode` | Controller | `String` | none | R/W | family-specific modes | No | `m?` or SCPI `SOUR:FUNC?` readback; mode write |
| `limit_current` | Controller/channel | `ElectricCurrent` | named current value | R/W | advertised maximum | No | `l?` / `l? <ch>` readback; limit-current write |
| `constant_current` | Controller/channel | `ElectricCurrent` | named current value | R/W | advertised maximum | Yes | `cc?` / `cc? <ch>` readback; constant-current write |
| `pwm_current` | Controller | `ElectricCurrent` | named current value | R/W where supported | advertised maximum | Yes | `pc?` readback; PWM-current write |
| `brightness` | DC4100 channel | `Ratio` | percent | R/W | 0..100 | Yes | `bp? <ch>` readback; brightness write; `brightness_percent` accepted as legacy alias |
| `pwm_frequency` / `pwm_duty_cycle` / `pwm_counts` | Controller | `Frequency` / `Ratio` / `I64` | Hz / percent / count | R/W | fixture ranges | No | `pf?` / `pd?` / `pn?` readback; PWM setup writes; `pwm_frequency_hz` accepted as a legacy alias |
| `modulation_current` / `modulation_frequency` / `modulation_depth` | DC3100 controller | `ElectricCurrent` / `Frequency` / `Ratio` | named current / Hz / percent | R/W where supported | fixture ranges | No | `cm?` / `f?` / `d?` readback; internal modulation writes; `modulation_frequency_hz` accepted as a legacy alias |
| `maximum_frequency` | Controller | `Frequency` | Hz | R | probe metadata where available | No | `mf?` readback; `maximum_frequency_hz` accepted as a legacy alias |
| `wavelength` | Controller/channel | `Wavelength` | named wavelength value | R | probe/channel metadata | No | `wl?` / `wl? <ch>` readback |
| `maximum_current` | Controller/channel | `ElectricCurrent` | named current value | R | probe/channel metadata | No | `ml?` / `ml? <ch>` readback |
| `forward_bias` | Controller/channel | `Voltage` | V | R | probe/channel metadata | No | `fb?` / `fb? <ch>` readback |
| `led_serial` | Controller/channel | `String` | none | R | LED head serial | No | `hs?` / `hs? <ch>` readback |
| `status` | Controller | `String` | none | R | status reply | No | `r?` or SCPI status readback |
| `status_register` | Controller | `I64` | none | R | raw status bits | No | `r?` or SCPI status readback |
| `firmware` | Controller | `String` | none | R | firmware reply | No | `v?` or SCPI version readback |

## Config Keys

| Key | Type | Status | Meaning |
| --- | --- | --- | --- |
| `driver = "thorlabs_dc"` | string | Canonical | Selects config-backed Thorlabs DC discovery |
| `family` | string | Canonical | One of `dc2010`, `dc2100`, `dc2200`, `dc3100`, `dc4100`, `dc4104`, or `ledd4` |
| `model` | string | Canonical | Configured controller model label |
| `serial_number` | string | Canonical | Configured controller serial number |
| `firmware` | string | Canonical | Configured firmware revision |
| `led_serial` | string | Canonical | Configured LED-head serial number |
| `wavelength` | `Wavelength` | Canonical | Single-output LED wavelength |
| `forward_bias` | `Voltage` | Canonical | Single-output LED forward bias |
| `maximum_current` | `ElectricCurrent` | Canonical | Controller maximum current |
| `maximum_frequency` | `Frequency` | Canonical | Controller maximum modulation/PWM frequency where known |
| `channel_wavelengths` | list of `Wavelength` | Canonical | DC4100/DC4104 channel wavelength metadata |
| `channel_maximum_currents` | list of `ElectricCurrent` | Canonical | DC4100/DC4104 channel current-limit metadata |
| `wavelength_nm`, `forward_bias_v`, `maximum_current_ma`, `maximum_frequency_hz` | scalar | Legacy aliases | Accepted for older configs |
| `serial_port` | string | Real transport | Serial device path; required when `connect = true` |
| `baud_rate` | integer | Real transport | Serial baud rate, default `115200` |
| `serial_timeout_ms` | integer milliseconds | Real transport | Serial timeout, default `100` |
| `usb_tmc` | bool | Real transport | Enables explicit DC2200 USBTMC config; implied by USB endpoint fields unless set false |
| `vendor_id`, `product_id` | integer | Real transport | USB VID/PID; required for USBTMC because no endpoint autodiscovery is claimed |
| `interface` | integer | Real transport | USB interface number, default `0` |
| `bulk_out_endpoint`, `bulk_in_endpoint` | integer | Real transport | USBTMC bulk endpoint addresses; required for USBTMC |
| `read_size` | integer bytes | Real transport | Maximum USBTMC DEV_DEP response payload, default `4096`, clamped to `64..1048576` |
| `connect` | bool | Real transport | Opens `serial_port` with `os-serial`, or the explicit USBTMC endpoint with `os-usb`; false keeps the configured state model |

When `connect = true`, discovery opens the configured serial endpoint or
explicit DC2200 USBTMC endpoint, runs the configured startup-readback script, and seeds cached
controller identity, LED-head metadata, output state, operation mode,
current/frequency limits, status, and DC4100 channel metadata from replies
before registering the driver.

Runtime property reads issue the mapped query before returning cached state.
Writable output, mode, current, PWM, modulation, and DC4100 channel properties
send the setter, query hardware error state, and then request the corresponding
readback when the controller returns it.

The controller and channel `GenericCommand` capabilities expose named
read-only refresh helpers over the same mapped query set used by runtime
property reads. They do not expose raw serial, SCPI, USBTMC, save, or setter
commands.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- light_source` | Generic light-source `Dac` and `TriggerSink`, typed intensity/enable properties, remultiplexed state set, `Runtime::wait_completed`, timing plan, and readback |
| `cargo run -p numanager-examples -- light_source dc2200` | Same workflow against the configured DC2200 SCPI fixture surface; real DC2200 USBTMC is selected only by config and `os-usb` |
| `cargo run -p numanager-examples -- light_source dc3100` | Same workflow against the configured DC3100 internal-modulation fixture surface |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate family-specific command variants, ingested current/status/channel reply forms, current limits, channel inventories, DC2200 USBTMC SCPI replies, and timing behavior |
| Discovery/config | USBTMC endpoint identification, VISA backend, configured serial endpoint confirmation, and model/channel reconciliation |
| Safety | LED head limits, thermal/fault states, modulation constraints, and polarity |
