# Thorlabs APT Motors

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::thorlabs_apt` |
| Families | Thorlabs APT motor controllers |
| Support level | Configured opt-in serial control/readback for APT motion, home, stop, status/profile/identity queries, and refresh helpers |
| Protocol evidence | Public APT binary-message behavior |
| Transport | Binary serial/USB-style packet resource |
| Discovery | Config-backed discovery; live serial requires configured endpoint and explicit connect |
| Validation | Configured serial startup-readback/control path is implemented; real hardware validation pending |
| Runtime/evidence notes | `numanager-drivers/os-serial` for explicitly constructed real serial ports |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `thorlabs-apt-hub` | `hub`, `motion.controller`, `thorlabs.apt` | Owns one binary command resource |
| `thorlabs-apt-axis-1` | `axis.x`, `stage.x`, `motion.apt` | One logical stage axis |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `thorlabs-apt-binary` | `serial.binary` | APT binary packet command path with move-complete/status-bit completion metadata; resource metadata records configured `serial_port`, `baud_rate`, `serial_timeout`, and `connected` state |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `StageMove` | Axis | `CapabilityRequest::StageMove` | Move/status map | Status and position frame readback when available | Position sequences |
| `StageHome` | Axis | `None` | Status string plus property events | Home command plus status and position frame readback when available | Not sequenceable |
| `StageStop` | Axis | `None` | Status string plus property events | Stop command plus status and position frame readback when available | Not sequenceable |
| `GenericCommand` | Axis | `refresh_telemetry`, `refresh_identity`, `refresh_position`, `refresh_status`, `refresh_velocity_profile`, or `keep_alive` with no params | Map with command count and status summary, or keep-alive command acceptance | Uses mapped APT request-frame readbacks plus the `MGMSG_MOT_ACK_DCSTATUSUPDATE` keep-alive frame; no arbitrary binary packet surface | Not sequenceable |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `model` | Hub | `String` | none | R | model string | No | Hardware-info frame/readback |
| `serial_number` | Hub | `String` | none | R | serial string | No | Hardware-info frame/readback |
| `busy` | Hub/axis | `Bool` | none | R | none | No | Status frame/readback |
| `position` | Axis | `Position` | um | R/W | configured travel | Yes | Move absolute/position/status readback packet |
| `target` | Axis | `Position` | um | R/W | configured travel | No | Target cache |
| `min_velocity` | Axis | `Velocity` | um/s | R/W | configured range | No | Velocity-params packet/readback |
| `acceleration` | Axis | `Acceleration` | um/s^2 | R/W | configured range | No | Velocity-params packet/readback |
| `max_velocity` | Axis | `Velocity` | um/s | R/W | configured range | No | Velocity-params packet/readback |
| `homed` | Axis | `Bool` | none | R | none | No | Status bits |
| `connected` | Axis | `Bool` | none | R | none | No | Status bits |
| `position_error` | Axis | `Bool` | none | R | none | No | Status bits |
| `status_bits` | Axis | `I64` | bitfield | R | APT status bits | No | Status packet |
| `status_summary` | Axis | `Map` | none | R | decoded status fields | No | Status bits plus current position/profile cache |

## Metadata

| Key | Scope | Type | Status | Meaning |
| --- | --- | --- | --- | --- |
| `travel` | Axis metadata | `Position` | Canonical | Axis travel range used for clamping and property ranges |
| `encoder_step_size` | Hub/axis metadata | `Position` | Canonical | Physical size of one encoder count |
| `legacy_travel_um` | Axis metadata | `Position` | Legacy marker | Compatibility label for former `travel_um` metadata |
| `legacy_encoder_step_size_um` | Hub/axis metadata | `Position` | Legacy marker | Compatibility label for former `encoder_step_size_um` metadata |

## Config Keys

| Key | Type | Status | Meaning |
| --- | --- | --- | --- |
| `driver = "thorlabs_apt"` | string | Canonical | Selects config-backed APT discovery |
| `model` | string | Canonical | Configured model label for the probe |
| `serial_number` | string | Canonical | Configured controller/device serial number |
| `channel` | integer | Canonical | One-based APT channel number |
| `travel` | `Position` | Canonical | Axis travel range used for public property bounds |
| `encoder_step_size` | `Position` | Canonical | Physical size of one encoder count |
| `travel_um`, `encoder_step_size_um` | scalar micrometers | Legacy aliases | Accepted for older configs |
| `homed`, `connected` | bool | Canonical | Initial configured status flags |
| `serial_port` | string | Real transport | Serial device path; required when `connect = true` |
| `baud_rate` | integer | Real transport | Serial baud rate, default `115200` |
| `serial_timeout_ms` | integer milliseconds | Real transport | Serial timeout, default `100` |
| `connect` | bool | Real transport | Opens `serial_port` with `os-serial`; false keeps the configured state model |

When `connect = true`, discovery opens the configured serial endpoint, runs the
configured startup-readback script, and seeds cached hardware identity, position, target,
status bits, busy/homed/connected state, and velocity profile from controller
frames before registering the driver.
Runtime property reads request and ingest the matching hardware-info, position,
status, or velocity-profile frame when a controller frame is available before
returning the cached public value. Motion, home, stop, and velocity-profile
write paths request the corresponding status, position, or velocity-profile
readback frames after command writes while retaining cached configured state
if no live frame is available.

The axis `GenericCommand` capability exposes read-only refresh helpers
and `keep_alive`. Refresh helpers issue the same mapped APT request frames used
by runtime property reads and update cached state from parsed reply frames;
`keep_alive` sends only the encoded keep-alive frame and does not claim status
streaming or timing behavior.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- motion_stage` | Generic XY/Z `StageMove`, typed position properties, remultiplexed state set, `Runtime::wait_completed`, timing plan, stop, and homing |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate packet headers, scaling, status bits, and homing behavior against real controllers |
| Discovery/config | USB enumeration and automatic controller/channel inventory |
| Safety | Limit switch, motor fault, and disconnect handling |
