# Starlight Xpress Filter Wheel

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::starlight_xpress` |
| Families | Starlight Xpress Mini/Midi/Universal/Standard/Maxi USB filter wheels using the published SX wheel command set |
| Support level | Documented serial filter-wheel protocol behind `os-serial`; explicit-config or single-match autodiscovered USB HID input/output-report backend behind `os-hid`; both real transports read filter count/current position before registration and retain configured transport metadata; readback refresh helpers; configured state model remains for examples |
| Protocol evidence | Starlight Xpress wheel handbooks document total-filter, current-filter, and select-filter commands for two-byte HID reports and four-byte serial frames; product pages state that Standard and Maxi wheels use the same protocol |
| Transport | Serial binary transport, 9600 baud, 8 data bits, no parity, 1 stop bit; USB HID fixed two-byte input/output reports |
| Discovery | Config-backed two-stage discovery; `connect=true` plus `usb_hid=true` can select exactly one enumerated HID device whose product string identifies an SX/Starlight filter wheel; real serial/HID construction runs documented filter-total and current-filter readbacks before registration |
| Validation | No hardware validation |
| Runtime/evidence notes | Real serial requires `numanager-drivers/os-serial`; real USB HID requires `numanager-drivers/os-hid`; HID autodiscovery is product-string/serial-number constrained and fails closed on zero or multiple candidates |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `starlight-xpress-filter-wheel` | `filter.wheel`, `state.device` | One logical wheel on one serial binary or USB HID report endpoint |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| Starlight Xpress serial endpoint | `serial.binary` | Sends four-byte startup/select/readback command frames and reads four-byte response frames; resource metadata records configured `serial_port`, `baud_rate`, and `connected` state |
| Starlight Xpress HID endpoint | `usb.hid.report` | Sends documented two-byte startup/select/readback output reports and reads documented two-byte input reports; resource metadata records explicit or autodiscovered `usb_vendor_id`, `usb_product_id`, `hid_report_id`, `hid_timeout`, optional `hid_serial_number`, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `FilterSelect` | Filter wheel | `CapabilityRequest::FilterSelect` | Final readback position | Sends filter-total guard read, select-filter command, then current-filter readback until the documented moving-zero state resolves; configured state responses are explicitly synthesized | State-set sequencing only |
| `GenericCommand` | Filter wheel | `refresh_readbacks`, `refresh_position`, or `refresh_positions` with no params | Position/count/moving map | Uses documented current-filter and filter-total readbacks | Not sequenceable |
| Property write | Filter wheel | Write `position` | Final position | Same path as `FilterSelect`; real serial fails closed if readback is missing or movement does not resolve before the poll limit | State-set sequencing only |
| Property read | Filter wheel | Read `position` or `positions` | `I64` | Uses current-filter or filter-total readback; real serial fails closed when no documented response is received | No |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `product` | Filter wheel | `String` | none | R | configured product label | No | Config/probe metadata |
| `serial_number` | Filter wheel | `String` | none | R | configured serial label | No | Config/probe metadata |
| `protocol` | Filter wheel | `String` | none | R | Starlight Xpress filter-wheel protocol | No | Protocol metadata |
| `positions` | Filter wheel | `I64` | count | R | `1..16` advertised; common wheels are 5, 7, or 9 positions | No | `Get Filter Total` serial command |
| `position` | Filter wheel | `I64` | slot | R/W | `1..positions` | No | `Select Filter` and `Request Current Filter` serial commands |
| `moving` | Filter wheel | `Bool` | none | R | `true` while readback reports zero | No | Zero data byte in documented replies |
| `last_transaction` | Filter wheel | `Map` | none | R | command, position, positions, moving, completion basis | No | Runtime transaction cache |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "starlight_xpress"`, `"sx_filter_wheel"`, or `"sx-wheel"` | Yes | string | Selects the Starlight Xpress filter-wheel provider |
| `property.product` | No | string | Persistent product/model label |
| `property.serial_number` | No | string | Persistent serial label |
| `property.positions` or `property.filter_count` | No | `I64` | Configured filter count |
| `property.position` | No | `I64` | Initial fixture/current position |
| `property.completion_polls` | No | `I64` | Maximum current-filter polls after a select command |
| `property.serial_port` | For real serial | string | OS serial port name; when present, discovery opens the documented serial protocol behind `numanager-drivers/os-serial` and reads filter total/current filter before registration |
| `property.baud_rate` | No | `I64` | Serial baud rate; default 9600 |
| `property.usb_hid` | For autodiscovered USB HID | bool | With `connect=true` and no serial or explicit VID/PID endpoint, enables constrained HID enumeration by product string; explicit HID endpoint fields imply HID unless set false |
| `property.vendor_id`, `product_id` | For explicit real USB HID | `I64` | USB VID/PID; optional when `connect=true` and `usb_hid=true` finds exactly one SX/Starlight filter-wheel HID identity |
| `property.hid_serial_number` | No | string | Optional HID serial selection; autodiscovery also honors an explicitly configured `serial_number` |
| `property.report_id` | No | `I64` | HID report ID prefix used by HIDAPI writes; default `0` |
| `property.hid_timeout_ms` | No | `I64` | HID input-report read timeout, default `100` |
| `property.connect` | Deprecated | `Bool` | Legacy guard; real transport is selected by `serial_port` or HID endpoint fields, and `connect = true` without either is rejected |

`GenericCommand` accepts only the named read-only refresh helpers over the
documented current-filter and filter-total commands. It does not expose raw
serial frames, HID report bytes, calibration, or product-specific VID/PID
catalog commands. HID autodiscovery uses passive HID identity enumeration and
single-candidate selection before the documented readback probe.

## Examples

| Example | Demonstrates |
| --- | --- |
| `discover_devices` | Shows a configured Starlight Xpress filter wheel in the two-stage discovery flow |
| `filters` | Runs the generic filter-wheel workflow with `FilterSelect`, position state-set write, completion waits, final position/moving readback, `last_transaction` completion-basis readback, and events without exposing raw protocol bytes |

## Remaining Work

| Area | Gap |
| --- | --- |
| USB HID | Product-specific VID/PID catalog needs hardware inventory; use explicit VID/PID config or constrained single-candidate HID identity autodiscovery |
| Hardware validation | Record construction-time serial/HID filter-total/current-filter readback, command stdout/stderr, requested filter position, readback/moving states, final position, timeout behavior, and wheel identity |
| Calibration | Automatic filter-count calibration is documented as slow; keep it as an explicit action when hardware behavior is not validated |
| Safety | Validate behavior for requested positions greater than fitted positions before relaxing configured range checks |
