# Opentrons OT-2 Research Note

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::opentrons_ot2` |
| Families | Opentrons OT-2 liquid handling robot and OT-2-compatible modules |
| Support level | Active HTTP health, inventory, first-module readback, v2 temperature-module set/deactivate, current-run/command-summary refresh, constrained current-run actions, documented gantry home/absolute move, and camera snapshot capture; arbitrary robot command execution needs documented command schemas and completion semantics |
| Protocol evidence | Opentrons HTTP API documentation, Opentrons open-source robot stack, OT-2 architecture note, module G-code documentation |
| Transport | Primary: HTTP/JSON to robot-server over Ethernet or Wi-Fi. Internal: UART G-code from Raspberry Pi to modified Smoothieboard. Modules: USB serial G-code. |
| Discovery | Configured host/IP fixture with optional `/health` probe; runtime `/modules` and `/runs` inventory refresh; first-module identity/status/temperature readback through robot-server; network robot discovery needs documented non-invasive behavior; module discovery should be delegated to robot-server first |
| Validation | No local OT-2 hardware validation recorded |
| Runtime/evidence notes | Arbitrary command enqueueing, pipetting, broader module actuation, calibration, image interpretation, relative gantry moves, pipette/nozzle-target moves, and recovery behavior require documented robot-server semantics or other protocol evidence |

## Source Summary

The OT-2 is not a simple directly attached serial device. Opentrons describes the
robot as a Raspberry Pi 3 running Linux, with client communication over a network
connection. The side USB connection is presented to the host as Ethernet through
an internal USB-to-Ethernet adapter, and Wi-Fi is also available. The robot
server runs on the robot and exposes the routine app/control interface.

The lower hardware layer is a modified Smoothieboard in the gantry. Opentrons'
architecture note says the Pi commands that board over UART using G-code. The
open-source Smoothie driver documents that the driver is the component that
knows about G-codes and Smoothie communication. It sends a command and then a
wait command for completion, with Smoothie replies/error/alarm handling.

The public HTTP API is the better integration boundary for numanager. The
OpenAPI reference says robots expose an OpenAPI spec from port `31950` at
`/openapi`, and requests require an `opentrons-version` header with version `2`
or higher. The API exposes health, protocol upload/analysis, run creation,
run actions, command enqueueing, command status, command errors, current run
state, module inventory/control, motor disengage, and camera picture endpoints.
For numanager, protocol upload/analysis/run should be treated as out of scope:
the useful surface is direct command enqueueing and status readback through
robot-server.

Python protocols do not strictly have to be uploaded through the Opentrons app:
Opentrons documents an interactive Python path through Jupyter on the robot.
That path is not a numanager integration target. It is useful only as evidence
that the Python API can drive hardware directly through the Opentrons stack, and
as a warning that competing control processes can conflict with robot-server.

Opentrons modules are separate serial devices attached to the OT-2 USB ports.
The module docs describe newline-terminated G-code commands, response or `OK`
acknowledgements, `ERRNNN:` errors, and VID/PID-based USB serial discovery.
However, for an OT-2 driver, those modules should usually be represented through
the robot-server run/module model first, not by independently opening module
serial ports and racing the Opentrons stack.

## Protocol Layers

| Layer | Evidence | Role | Suggested numanager treatment |
| --- | --- | --- | --- |
| HTTP robot-server | Opentrons HTTP API reference | Stable client boundary for direct command queueing, robot state, modules, camera, and health | Implement first as an HTTP-backed hub/resource |
| Robot Jupyter/Python execution | Opentrons Jupyter docs | Interactive Python `ProtocolContext` control running on the robot | Out of scope for numanager integration; evidence only |
| Python Protocol API | Opentrons docs and open source | High-level command vocabulary and semantics used by Opentrons runtime | Use as semantic reference only; do not support protocol upload as a numanager workflow |
| Engine command model | HTTP `/runs/{runId}/commands` command queue | JSON command queue with statuses, errors, and run state | Map selected commands to capabilities only after auditing command schemas and real execution semantics |
| OT-2 Smoothie UART | Opentrons architecture and Smoothie driver source | Internal gantry/pipette motor controller protocol | Keep private/diagnostic; do not make the default integration path |
| Module USB serial G-code | Opentrons module docs | Direct protocol for Thermocycler, Temperature, Magnetic, Heater-Shaker, and related modules | Prefer robot-server mediation; direct module drivers are separate devices only when explicitly configured |

## Important Observed Protocol Facts

| Fact | Evidence | Implementation consequence |
| --- | --- | --- |
| Host communication is networked, including USB-as-Ethernet | OT-2 architecture note | Model discovery as network discovery/config, not local USB serial discovery |
| Robot-server is the routine external control process | OT-2 architecture note and HTTP API docs | The hub should own an HTTP client session and version negotiation |
| Jupyter on the robot can run Python API commands interactively | Opentrons Jupyter docs | Direct Python control exists, but numanager should not integrate through Jupyter |
| Only one process can own GPIO/hardware resources at a time | OT-2 architecture note | Avoid local SSH/Python hardware-control sidecars while robot-server is active |
| HTTP requests require `opentrons-version` `2` or higher | HTTP API docs | Store negotiated/requested API version as a resource property |
| `/health` reports server readiness and may return 503 when the motor controller is not ready | HTTP API docs | Use health as discovery/readiness, but not as motion validation |
| Direct HTTP command endpoints support queued/running/completed/error status | HTTP API docs | Represent command completion from HTTP command status, not low-level G-code acknowledgements |
| OT-2 has six motion axes: X, Y, Z, A, B, C | OT-2 architecture note and Smoothie constants | Expose gantry/mount/plunger concepts, not raw axes, except diagnostics |
| Smoothie motion commands are line-oriented G-code terminated by `\r\n\r\n`, expecting `ok\r\nok\r\n` acknowledgements in Opentrons' driver | Opentrons Smoothie constants | Treat as private diagnostics; numanager motion uses documented robot-server commands |
| Opentrons modules use newline-terminated G-code with `OK` and `ERRNNN:` response vocabulary | Opentrons module G-code docs | Direct module protocol can be spec-backed, but independent module access must avoid robot-server conflicts |

## Logical Devices

Model the OT-2 as a network hub with logical child devices. The hub owns one
HTTP robot-server resource. Child devices expose numanager capabilities at the
workflow level and rely on robot-server for planning, calibration, collision
avoidance, module routing, and recovery state.

This model intentionally excludes protocol upload as a supported workflow.
numanager should issue direct robot-server operations such as home, move, raw
pipette actuator commands, module setpoints, pause/resume/cancel, status reads,
and snapshots. Higher-level routines such as tip pickup and transfer require
orchestration across gantry motion, pipette state, labware geometry, deck
calibration, and module occupancy. They are represented as composed workflow
behavior rather than raw pipette-device commands.

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `opentrons-ot2` | `hub`, `liquid_handler.robot`, `network.http` | Owns the configured robot-server HTTP session identity |
| `opentrons-ot2-gantry` | `stage.xyz`, `motion.robot` | Robot-server gantry home and absolute configured-mount move control |
| `opentrons-ot2-left-pipette` | `pipette`, `liquid_handler.axis`, `mount.left` | Raw pipette actuator/readback device present only when configured/reported left pipette inventory exists |
| `opentrons-ot2-right-pipette` | `pipette`, `liquid_handler.axis`, `mount.right` | Raw pipette actuator/readback device present only when configured/reported right pipette inventory exists |
| `opentrons-ot2-deck` | `deck`, `labware.host` | Read-only labware/module inventory counts |
| `opentrons-ot2-camera` | `camera.snapshot`, `inspection.camera` | Present when configured/reported camera inventory exists; snapshot uses the robot-server camera endpoint and stores the returned HTTP image bytes as a native frame |
| `opentrons-ot2-temperature-module-*` | `module.temperature`, `module.opentrons` | Temperature module child device |
| `opentrons-ot2-thermocycler-*` | `module.thermocycler`, `module.temperature`, `module.opentrons` | Thermocycler child device |
| `opentrons-ot2-magnetic-module-*` | `module.magnetic`, `module.opentrons` | Magnetic module child device |
| `opentrons-ot2-heater-shaker-*` | `module.heater_shaker`, `module.temperature`, `module.shaker`, `module.opentrons` | Heater-Shaker child device |

Avoid presenting individual Smoothie axes as normal public devices. They are
useful diagnostics, but the user-facing model should describe mounts, deck
positions, labware, pipettes, and module tasks.

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `Health` or `DeviceStatus` | Hub | None | Robot/server/software readiness, API version, motor-controller readiness | Implemented as an optional config-time `/health` probe plus runtime `refresh_health` readback | Pollable later |
| `GenericCommand` | Hub | `refresh_health`, `refresh_inventory`, `refresh_current_run`, `refresh_run_commands`, `play_run`, `pause_run`, or `stop_run` | Health, inventory, first-module readback, current-run, command-summary, or run-action metadata map | Plain HTTP `/health`, `/modules`, `/runs`, `/runs/{runId}`, `/runs/{runId}/commands`, or `/runs/{runId}/actions` response; emits changed hub/deck/module metadata | Health/readiness, inventory, first-module status/temperature readback, run/command-status readback, and constrained run actions only; no arbitrary command enqueueing |
| `StageHome` | Gantry | `None` | Homed state/readback | `POST /robot/home` response | Manual/setup only |
| `StageMove` | Gantry | Absolute X/Y/Z `CapabilityRequest::StageMove`; no relative moves or motion profiles | Position/state | `POST /robot/move` response for the configured mount nominal position | Not sequenceable |
| `TemperatureControl` | Temperature module | Target temperature and optional enabled flag | Current/target/status | API v2 `POST /modules/{serial}` with `set_Temperature` or `deactivate`; API v3+ fails closed until the run-command replacement is audited | Direct module action only |
| `CameraCapture` | Camera | `CapabilityRequest::CameraCapture` or `None`; only native HTTP image encoding is accepted | Frame handle plus HTTP content metadata | `POST /camera/picture` response completion; image dimensions are not inferred | Not stream-oriented initially |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Protocol mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `host` | Hub | `String` | none | config | hostname/IP | No | HTTP base URL |
| `port` | Hub | `I64` | none | R/config | default `31950` | No | HTTP robot-server port |
| `api_version` | Hub | `String` | none | R/config | HTTP API version header | No | `opentrons-version` |
| `server_version` | Hub | `String` | none | R | none | No | `/health` |
| `robot_serial` | Hub | `String` | none | R | OT2 serial format when available | No | health/server/settings endpoint |
| `robot_type` | Hub | `String` | none | R | `OT-2`/reported value | No | protocol/run metadata |
| `status` | Hub | `String` | none | R | `idle`, `running`, `paused`, `blocked`, `failed`, etc. | No | current run/health |
| `door_open` | Hub | `Bool` | none | R | none | No | robot-server state if exposed |
| `current_run` | Hub | `String` | none | R | run id or none | No | current run endpoints |
| `module_count` | Hub | `I64` | none | R/config | non-negative | No | `/modules` `data` array count |
| `run_count` | Hub | `I64` | none | R/config | non-negative | No | `/runs` `data` array count |
| `command_count` | Hub | `I64` | none | R/config | non-negative | No | `/runs/{runId}/commands` `data` array count from the current refresh page |
| `current_command` | Hub | `String` | none | R/config | command id or none | No | first command id found in `/runs/{runId}/commands` response |
| `current_command_status` | Hub | `String` | none | R/config | queued/running/completed/failed or reported status | No | first command status found in `/runs/{runId}/commands` response |
| `module_inventory_state` | Hub | `String` | none | R/config | configured or HTTP refresh state | No | `/modules` status |
| `run_inventory_state` | Hub | `String` | none | R/config | configured or HTTP refresh state | No | `/runs` status |
| `last_http_status` | Hub | `String` | none | R/config | last read-only HTTP request status summary | No | read-only HTTP refreshes |
| `x`, `y`, `z` | Gantry | `Position` | typed | R | cached absolute deck coordinate after configured state or `StageMove` | No | `POST /robot/move` point values in mm |
| `homed` | Gantry | `Bool` | none | R | none | No | Set true after successful `POST /robot/home` |
| `mount` | Gantry | `String` | none | R/config | `left`, `right` | No | `POST /robot/move` mount selector |
| `mount` | Pipette | `String` | none | R | `left`, `right` | No | run pipette inventory |
| `model` | Pipette/module | `String` | none | R | Opentrons model names | No | run/module inventory |
| `serial` | Pipette/module | `String` | none | R | none | No | inventory endpoints |
| `has_tip` | Pipette | `Bool` | none | R | none | No | current run state tip states |
| `volume` | Pipette | typed volume when available | ul | R/W | pipette model constraints | Yes, through raw pipette actuator commands | command params/results |
| `plunger_position` | Pipette | typed position or ratio | model-dependent | R/W | pipette model constraints | Yes | raw pipette actuator command/result |
| `temperature` | Temperature/Thermocycler module | typed temperature | degC | R | module-specific range | No | first matching current-temperature field from `/modules` readback |
| `target_temperature` | Temperature module | `Temperature` | typed | R/W | 4..=95 degC | No | first matching target-temperature field from `/modules` readback; writes use API v2 `set_Temperature` |
| `enabled` | Temperature module | `Bool` | none | R/W | none | No | status is enabled when not `idle`; writes with `false` use API v2 `deactivate`; writes with `true` require or reuse a target |
| `position` | Magnetic module | typed position | mm | R/W | documented module range | Yes | engage/disengage command/result |
| `speed` | Heater-Shaker module | typed frequency | rpm | R/W | documented module range | Yes | set shake speed command/result |
| `latch_closed` | Heater-Shaker module | `Bool` | none | R/W | none | No | latch command/result |

The driver implements configured inventory resources and can optionally probe
`GET /health` over plain HTTP at discovery time when `property.connect = true`.
The live probe updates cached server/version/status metadata. It does
not advertise arbitrary robot command execution capabilities. A constrained hub
`GenericCommand` named `refresh_health` repeats the same read-only `/health`
request at runtime and updates cached server/version/status metadata.
`refresh_inventory` performs read-only `GET /modules` and `GET /runs` requests,
updates cached module/run counts, first-module model/serial/status/temperature
readback, and current run id when reported, and emits changed hub/deck/module
metadata.
`refresh_current_run` performs a read-only `GET /runs/{runId}` request when a
current run id is known, updates cached run/status metadata from shallow JSON
fields, and emits changed hub metadata.
`refresh_run_commands` performs a read-only
`GET /runs/{runId}/commands?pageLength=20` request when a current run id is
known, updates cached command count/id/status metadata from shallow JSON fields,
and emits changed hub metadata. It does not enqueue commands or wait for command
completion.
`play_run`, `pause_run`, and `stop_run` perform `POST /runs/{runId}/actions`
with `actionType` values `play`, `pause`, and `stop` for the current run only.
These commands report action submission and HTTP status; they are not motion or
pipetting completion claims.
`StageHome` on the gantry performs `POST /robot/home` with target `robot`.
`StageMove` on the gantry performs `POST /robot/move` with target `mount`,
configured mount `left` or `right`, and absolute X/Y/Z deck coordinates
converted to millimeters.
Relative moves, motion profiles, and pipette/nozzle-target movement are not
advertised without documented schemas and calibration assumptions.

When a camera device is present, `CameraCapture` performs `POST /camera/picture`
and stores the returned bytes as a native HTTP-image frame with content type,
HTTP status, and byte count metadata. The driver does not parse JPEG/PNG
headers, infer dimensions, or claim optical validation.
When a temperature-module child device is present, `TemperatureControl` and
the writable `target_temperature`/`enabled` properties use the deprecated API
v2 module command endpoint, not arbitrary run-command enqueueing. The driver
sends `set_Temperature` with one Celsius argument for targets in the documented
4..=95 degC range and `deactivate` for `enabled=false`. Because Opentrons
documents that endpoint as removed with `Opentrons-Version: 3`, configured API
versions `3` and higher fail closed until the replacement command schema is
audited.

Property names intentionally avoid unit suffixes where the value type carries
the unit. Native Opentrons command names, enum spellings, and G-code tokens
should remain metadata or diagnostics, not the normal public property surface.

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "opentrons_ot2"` or `"opentrons-ot2"` | Yes | string | Selects the configured OT-2 provider |
| `property.host` | No | string | Configured robot-server host/IP; empty values are rejected |
| `property.port` | No | `I64` | Robot-server HTTP port; defaults to `31950` |
| `property.connect` | No | `Bool` | Probe `GET /health` during discovery when true; runtime refresh commands use the configured robot-server endpoint |
| `property.connect_timeout_ms`, `property.response_timeout_ms` | No | `I64` | TCP connect and HTTP response timeouts for the active health probe |
| `property.api_version` | No | numeric string | Configured `opentrons-version` header; values below `2` are rejected |
| `property.server_version`, `robot_serial`, `robot_type`, `status`, `current_run` | No | string | Configured read-only robot/server metadata |
| `property.module_count`, `run_count`, `command_count` | No | `I64` | Configured non-negative read-only inventory/count metadata |
| `property.current_command`, `current_command_status` | No | string | Configured read-only current-command metadata |
| `property.module_inventory_state`, `run_inventory_state`, `last_http_status` | No | string | Configured read-only HTTP inventory/status metadata |
| `property.gantry_mount` | No | string `left` or `right` | Configured mount passed to `POST /robot/move`; default `left` |
| `property.gantry_x`, `gantry_y`, `gantry_z` | No | `Position`, `I64`, or `F64` millimeters | Configured cached gantry position before any successful `StageMove` |
| `property.gantry_homed` | No | `Bool` | Configured cached gantry home state before any successful `StageHome` |
| `property.door_open`, `camera_present` | No | `Bool` | Configured read-only robot/camera inventory metadata |
| `property.left_pipette_model`, `left_pipette_serial`, `right_pipette_model`, `right_pipette_serial`, `module_model`, `module_serial` | No | string or empty/`none` | Configured child-device inventory metadata |
| `property.module_status` | No | string | Configured read-only module status metadata |
| `property.module_temperature`, `module_target_temperature` | No | `Temperature` | Configured module temperature metadata; `module_target_temperature` is writable at runtime through the module child device |

## Implementation Recommendation

1. Keep the HTTP robot-server boundary.
   The current driver already supports `/health`, `/modules`, `/runs`,
   current-run and command-summary refresh, constrained run actions, camera
   snapshot capture, gantry home/absolute move, and API v2 temperature-module
   set/deactivate. Further expansion should stay at this service boundary until
   a specific lower-level backend has separate evidence.

2. Direct command submission is not exposed because command schemas and completion semantics are not audited.
   The OT-2 safety model lives in Opentrons' planner/calibration stack, so
   numanager should submit only audited robot-server commands instead of
   translating arbitrary stage/pipette operations into Smoothie G-code.

3. Add a constrained command bridge.
   Support only command types whose HTTP schema, command result, completion
   status, and failure states have been audited. Keep command-type metadata
   visible for diagnostics, but expose public operations as capabilities.

4. Add raw pipette devices before a liquid-handler meta-device.
   The first pipette API should expose explicit actuator/readback operations,
   not tip pickup or transfer. Tip pickup requires coordinated gantry motion,
   deck/labware geometry, tip rack state, pipette state, and recovery handling;
   that should be modeled later as a composed liquid-handler meta-device.

5. Add first-class module capabilities for module behavior that does not fit
   existing capability kinds. Temperature module set/deactivate is implemented
   through the API v2 module endpoint. Magnetic engage/disengage and
   heater-shaker shake/latch control should get distinct capability types once
   their HTTP command schemas are audited.

6. Exclude Jupyter/Python notebook integration.
   numanager direct control should be service/API based, not notebook-session
   based. Jupyter remains useful for manual investigation outside the driver.

7. Keep Smoothie UART as a separate, disabled diagnostic backend.
   Direct G-code can be valuable for recovery or hardware bring-up, but it
   bypasses robot-server coordination and is not exposed without
   real OT-2 trace validation.

8. Model modules as children discovered from robot-server.
   Direct USB serial module drivers require separate module-protocol evidence,
   but on a stock OT-2 they should not open module ports behind robot-server's
   back.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- discover_devices` | Shows the configured OT-2 read-only inventory in the two-stage detect/add flow |
| `cargo run -p numanager-examples -- robot_inventory opentrons` | Generic robot/lab-automation inventory workflow: discovery, runtime addition, and public property readback without motion, pipetting, broader module actuation, or robot command enqueueing |

## Open Questions for Expansion

| Area | Question |
| --- | --- |
| Discovery | Which network discovery protocol should numanager use: mDNS, configured host only, or Opentrons discovery behavior from the app/client source? |
| Authentication | Whether target robot software requires any auth, pairing, or lab policy controls for the intended deployment |
| API version | Which minimum robot software/API version numanager should support |
| Command scope | Which imperative commands are needed first: setup commands, maintenance moves, module control, or full pipetting primitives |
| Calibration | How labware offsets, pipette calibration, deck calibration, and current run state should be represented in numanager config |
| Safety | Door state, estop/current-state behavior, pause/cancel/recovery semantics, and post-error safe state need hardware validation |
| Timing | How numanager timing plans should drive OT-2 commands directly while respecting robot-server queueing and calibration state |

## Evidence Links

| Evidence | Link |
| --- | --- |
| OT-2 architecture, Raspberry Pi/network/GPIO/Smoothieboard/UART G-code/robot-server ownership | <https://github.com/Opentrons/opentrons/blob/edge/OT2_ARCHITECTURE.md> |
| Opentrons platform source repository | <https://github.com/Opentrons/opentrons> |
| Opentrons HTTP API reference and `/openapi` note | <https://docs.opentrons.com/http/api_reference.html> |
| Opentrons HTTP API module command endpoint: API v2 `POST /modules/{serial}` with `command_type`/`args`, deprecated and removed with `Opentrons-Version: 3`; sample `set_Temperature` payload | <https://docs.opentrons.com/http/api_reference.html> |
| Temperature Module Python API: `set_temperature`, `deactivate`, status, target/current temperature, and documented 4..=95 degC target range | <https://docs.opentrons.com/python-api/reference/temperature-module/> |
| Smoothie driver source: G-code/Smoothie boundary, command completion, error handling | <https://github.com/Opentrons/opentrons/blob/edge/api/src/opentrons/drivers/smoothie_drivers/driver_3_0.py> |
| Smoothie constants: motion/status/current/pipette G-code names, terminator, acknowledgement, axes, homed positions | <https://github.com/Opentrons/opentrons/blob/edge/api/src/opentrons/drivers/smoothie_drivers/constants.py> |
| OT-2 robot components, USB module ports, serial number format | <https://docs.opentrons.com/ot-2/system-description/robot/> |
| Thermocycler/module G-code concepts: newline terminator, OK/ERR response vocabulary, VID/PID discovery | <https://sandbox.docs.opentrons.com/edge/thermocycler/g-code-concepts/> |
| Thermocycler G-code command table example for direct module protocol evidence | <https://sandbox.docs.opentrons.com/edge/thermocycler/g-codes/> |
| Module compatibility and OT-2 module placement constraints | <https://docs.opentrons.com/protocol-designer/create-protocol/modules-fixtures/> |
| Python API robot motor control surface | <https://docs.opentrons.com/python-api/reference/robot-motors/> |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Create a bench note from a real OT-2 covering robot software version, API version, transport, `/health`, module inventory, current run status, a no-motion setup command, camera snapshot content type/byte count, play/pause/stop action submission, and observed error payloads |
| HTTP schema audit | Pin the OpenAPI schema version and identify the smallest stable set of command types to support |
| Discovery | mDNS/HTTP robot discovery is not exposed without documented non-invasive behavior; configured hosts remain the default path |
| Safety | Validate door/estop/pause/cancel/recovery behavior and map it into common safety properties |
| Motion/pipetting | Relative movement, pipette/nozzle-target movement, and pipetting is not exposed because documented command completion, calibration assumptions, bounds, and recovery behavior are absent |
| Direct Smoothie | Direct Smoothie access requires serial traces describing command framing, acknowledgements, alarm/error vocabulary, recovery, and safe homing behavior on the target robot revision |
