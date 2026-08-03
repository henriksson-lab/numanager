# Evident IX85

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::evident_ix85` |
| Families | Evident/Olympus IX85 microscope body and directly attached body devices |
| Support level | Configured inventory plus configured opt-in serial readback/control for `V`, `U`, focus motion/stop, state-device selection, shutter control, software timing endpoint application, ZDC status tags, and hub refresh commands; ZDC autofocus actions are not exposed because `AF` parameter semantics are absent |
| Protocol evidence | Reverse engineered direct serial evidence records serial constants, command tags, device inventory model, focus limits, state-device ranges, and ZDC/autofocus tags |
| Transport | Configured state by default. Optional active transport is RS-232 ASCII, 115200 baud, 8 data bits, even parity, 2 stop bits, CRLF terminator, and 4000 ms answer timeout; active construction queries `V`, `U`, and mapped readback tags where matching logical devices are present |
| Discovery | Config-backed two-stage discovery |
| Validation | No numanager IX85 hardware validation note |
| Runtime/evidence notes | Runtime timing plans apply first/last focus, state-device, and shutter endpoint values through the same typed write/readback paths. ZDC autofocus actions, hardware-synchronized timing, notification streaming, busy/completion semantics, and safety/fault handling require `AF` parameter semantics from documentation or hardware traces |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `ix85-hub` | `hub`, `microscope.body`, `serial.ascii` | Owns the body serial command path and configured inventory |
| `ix85-focus` | `axis.z`, `stage.z`, `microscope.focus` | Logical focus drive remultiplexed through the body controller |
| `ix85-nosepiece` | `objective.turret`, `state.device` | Objective turret state endpoint |
| `ix85-light-path` | `light.path`, `state.device` | Left/right/binocular light-path state endpoint |
| `ix85-mirror-unit-1` | `filter.cube`, `mirror.unit`, `state.device` | Reflected-light mirror/filter-cube turret endpoint |
| `ix85-dia-shutter` | `shutter`, `light.gate`, `state.device` | Transmitted-light shutter endpoint |
| `ix85-epi-shutter-1` | `shutter`, `light.gate`, `state.device` | Reflected-light shutter endpoint |
| `ix85-zdc-autofocus` | `autofocus`, `zdc`, `state.device` | ZDC/autofocus state endpoint; action commands are not exposed because `AF` parameter semantics are absent |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `ix85-serial` | `serial.ascii` | Configured provenance plus optional active readback/control path for `V`, `U`, `FP`, `FG`, `FM`, `FSTP`, `OB`, `BIL`, `MU1`, `DSH`, `ESH1`, and `AFST`; resource metadata records configured `serial_port`, fixed `baud_rate`, fixed `serial_timeout`, and `connected` state; hub `GenericCommand` exposes `refresh_readbacks`, `refresh_identity`, and `refresh_status` helpers; reset/maintenance commands remain hidden |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `GenericCommand` | Hub | `refresh_readbacks`, `refresh_identity`, or `refresh_status` with no params | Map containing command, connection state, refreshed keys, controller version, and unit summary | Readback refresh; connected transport issues only the documented read tags and configured mode returns cached state | No |
| `StageMove`/`StageStop` | Focus | `CapabilityRequest::StageMove` with one `Z`/`focus` target, or `None` for stop | Final focus position/readback | Configured mode updates cached position; connected mode sends `FG`/`FM`/`FSTP`, accepts positive ACK, then refreshes `FP` when a reply is available | Sequenceable focus position |
| `FilterSelect` | Nosepiece/light path/mirror unit | `CapabilityRequest::FilterSelect` | Final state readback | Configured mode updates cached state; connected mode sends `OB`, `BIL`, or `MU1`, accepts positive ACK, then refreshes the same tag when a reply is available | Sequenceable state |
| `TriggerSink` | Shutters | `CapabilityRequest::Trigger` or `None` for pulse | Final open state | Configured mode updates cached shutter state; connected mode sends `DSH` or `ESH1`, accepts positive ACK, then refreshes the same tag when a reply is available | Sequenceable shutter state |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `model` | Hub | `String` | none | R | configured identity | No | configured inventory |
| `serial_number` | Hub | `String` or `Null` | none | R | configured identity | No | configured inventory |
| `controller_version` | Hub | `String` | none | R | configured/version reply | No | `V` controller version path |
| `unit_summary` | Hub | `String` | none | R | configured unit reply | No | `U` unit information path |
| `serial_settings` | Hub | `String` | none | R | `115200 8E2 no-flow CRLF` | No | adapter serial constants |
| `serial_port` | Hub | `String` | none | R | configured serial port, empty when unset | No | Config metadata |
| `connected` | Hub | `Bool` | none | R | configured serial transport opened during construction | No | Runtime transport state |
| `protocol_tags` | Hub | `Map` | none | R | public readback-tag summary plus hidden-control marker | No | adapter tag constants |
| `feature_summary` | Hub | `Map` | none | R | known serial/inventory/control shape plus unexposed autofocus action | No | Runtime evidence gate |
| `action_gate` | All devices | `String` | none | R | command validation summary | No | Runtime evidence metadata |
| `position` | Focus | `Position` | named position value | R/W | configured, 0..10500 um; serial `FP` reports 10 nm steps converted to micrometers | Yes | `FP`; writes use `FG`; relative moves use `FM`; stop uses `FSTP` |
| `minimum_position`, `maximum_position` | Focus | `Position` | named position value | R | 0 and 10500 um | No | adapter focus constants |
| `nosepiece_position` | Nosepiece | `I64` | slot | R/W | 1..6 | Yes | `OB` query/set |
| `light_path_position` | Light path | `I64` | slot | R/W | 1..4 | Yes | `BIL` query/set |
| `mirror_unit_1_position` | Mirror unit 1 | `I64` | slot | R/W | 1..8 | Yes | `MU1` query/set |
| `dia_shutter_open` | DIA shutter | `Bool` | none | R/W | configured state | Yes | `DSH` query/set |
| `epi_shutter_1_open` | EPI shutter 1 | `Bool` | none | R/W | configured state | Yes | `ESH1` query/set |
| `state` | ZDC/autofocus | `String` | none | R | configured state | No | `AF` / `AFST` |
| `wire_tag` | All non-hub devices | `String` | none | R | adapter tag summary | No | device-specific tag |
| `command_summary` | All non-hub devices | `Map` | none | R | public typed command/readback tag summary | No | device-specific tags |
| `support_level` | All devices | `String` | none | R | configured or opt-in serial control support level; ZDC action remains read-only | No | Runtime evidence metadata |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "evident_ix85"` | Yes | string | Selects the configured IX85 provider |
| `driver = "evident-ix85"`, `"ix85"`, or `"olympus_ix85"` | Yes | string | Discovery aliases |
| `property.serial_port` | Required when `property.connect = true` | string | Serial port path/name for active startup readback |
| `property.connect` | No | `Bool` | Open the real serial transport and refresh mapped readback tags during construction and matching property reads |
| `property.model` | No | string | Body model label; wrong types are rejected instead of silently falling back |
| `property.serial_number` | No | string or `Null` | Configured body serial; wrong types are rejected instead of silently falling back |
| `property.controller_version` | No | string | Configured controller version; wrong types are rejected instead of silently falling back |
| `property.unit_summary` | No | string | Configured `U` command summary; wrong types are rejected instead of silently falling back |
| `property.focus_present`, `nosepiece_present`, `light_path_present`, `mirror_unit_1_present`, `dia_shutter_present`, `epi_shutter_1_present`, `autofocus_present` | No | bool | Controls which logical devices are advertised; wrong types are rejected instead of silently falling back |
| `property.focus_position` | No | `Position` | Configured focus readback; values outside 0..10500 um are rejected instead of silently advertising impossible readback |
| `property.nosepiece_position`, `light_path_position`, `mirror_unit_1_position` | No | integer | Configured state-device readbacks; values outside the documented 1..6, 1..4, and 1..8 ranges are rejected |
| `property.dia_shutter_open`, `epi_shutter_1_open` | No | bool | Configured shutter readbacks; wrong types are rejected instead of silently falling back |
| `property.autofocus_state` | No | string | Configured ZDC/autofocus state; wrong types are rejected instead of silently falling back |

## Examples

| Example | Demonstrates |
| --- | --- |
| `discover_devices` | Two-stage configured IX85 candidate detection and add flow |

## Remaining Work

| Area | Gap |
| --- | --- |
| Source provenance | Pin official Evident/Olympus serial protocol documentation if available; current direct serial evidence is reverse engineered |
| Configured serial | Readback/control for `V`, `U`, `FP`, `FG`, `FM`, `FSTP`, `OB`, `BIL`, `MU1`, `DSH`, `ESH1`, and `AFST` is implemented behind opt-in serial and typed capabilities/properties; validate login, remote mode, broader command replies, notifications, timeouts, and negative ACK/error behavior on real IX85 hardware |
| Motion | Focus absolute/relative/stop is implemented from the known serial tags; validate busy/completion, limit behavior, and recovery before claiming hardware timing behavior |
| State devices | Nosepiece, light-path, and mirror-unit selection are implemented from the known serial tags; validate zero/unknown states, notifications, and position-count detection |
| Shutters | DIA and EPI shutter open/close are implemented from the known serial tags; validate cover/interlock behavior and final safe state |
| Autofocus | Treat ZDC as a general autofocus provider only after `AF` parameter values, `AFST`, table/parameter, limit, and failure-state semantics are known |
