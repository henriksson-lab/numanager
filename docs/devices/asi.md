# ASI MS-2000 and Tiger

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::asi` |
| Families | ASI MS-2000/RM-2000, ASI Tiger |
| Support level | Configured opt-in serial stage control/readback with explicit coordinate-reference command |
| Protocol evidence | ASI serial ASCII command families and Micro-Manager behavior as secondary evidence |
| Transport | CR-terminated ASCII over `SerialIo` |
| Discovery | Two-stage config discovery; live serial requires configured endpoints and explicit connect |
| Validation | Configured serial startup-readback/control paths are implemented; real hardware validation pending |
| Runtime/evidence notes | `numanager-drivers/os-serial` for configured real serial ports |

## Logical Devices

| Driver mode | Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- | --- |
| MS-2000 | `asi-ms2000-hub` | `hub`, `motion.controller`, `serial.ascii` | Owns one serial resource |
| MS-2000 | `asi-ms2000-xy` | `axis.xy`, `stage.xy` | X/Y moves coalesced into one `M X=... Y=...` or `R X=... Y=...` command |
| MS-2000 | `asi-ms2000-z` | `axis.z`, `stage.z` | Shares hub serial resource with XY |
| Tiger | `asi-tiger-hub` | `hub`, `motion.controller`, `serial.ascii`, `asi.tiger` | Owns one card-addressed serial resource |
| Tiger | `asi-tiger-xy` | `axis.xy`, `stage.xy`, `asi.tiger.card` | Card-addressed XY motion |
| Tiger | `asi-tiger-z` | `axis.z`, `stage.z`, `asi.tiger.card` | Card-addressed Z motion |
| Tiger | `asi-tiger-ttl` | `trigger.source`, `ttl`, `asi.tiger.card` | TTL output through Tiger card command |
| Tiger | `asi-tiger-ring-buffer` | `pulse.program`, `ring.buffer`, `asi.tiger.card` | Ring-buffer mode/start state through Tiger card commands |
| Tiger | `asi-tiger-crisp-autofocus` | `autofocus`, `asi.crisp`, `asi.tiger.card` | Generic autofocus provider depending on Tiger Z |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `asi-ms2000-serial` | `serial` | CR-terminated MS-2000 command path with `STATUS /` idle/busy completion metadata and configured-startup readback metadata |
| `asi-tiger-serial` | `serial` | CR-terminated Tiger card-addressed command path with card metadata, idle completion, and configured-startup readback metadata |

## Capabilities

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `StageMove` | MS-2000/Tiger XY and Z | `CapabilityRequest::StageMove` | Map with moved axes/profile metadata | Polls `/` busy/idle status; no complete firmware status/error model | Position sequences via software timing-plan endpoints for MS-2000 and Tiger |
| `StageHome` | MS-2000/Tiger XY and Z | `None` | Status string | Sends `HOME`, consumes immediate ACK when present, then refreshes `/` busy state plus `W` position readback when serial is connected; cached configured state records the home position when no live reply is available | Not sequenceable |
| `StageStop` | MS-2000/Tiger XY and Z | `None` | Status string | Sends `HALT`, consumes immediate ACK when present, then refreshes `/` busy state plus `W` position readback when serial is connected; cached configured state clears busy when no live reply is available | Not sequenceable |
| `GenericCommand` | MS-2000/Tiger hub | No-parameter `refresh_readbacks`, `refresh_identity`, `refresh_status`, `refresh_position`, and `refresh_positions`; Tiger also accepts no-parameter `refresh_crisp` when a CRISP card is configured | Refresh completion-basis map | Requests the mapped query replies already used by property reads | Not sequenceable |
| `TriggerSource` | Tiger TTL | `None` or `CapabilityRequest::Trigger` | Level/action map | Runtime token completion | `ttl0` bool start/stop timing |
| `PulseProgram` | Tiger ring buffer | `None` or `CapabilityRequest::PulseProgram` with optional `count`/`wait_for_input` | Ring-buffer mode/size/running map | Runtime token completion | `running` bool start/stop timing |
| `Autofocus` | Tiger CRISP | `CapabilityRequest::Autofocus` | Provider-neutral autofocus state map | Runtime token completion; hardware validation pending | CRISP `state` sequences |

## Properties

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `x` | MS-2000/Tiger XY | `Position` | um | R/W | Travel metadata | Yes | `W X Y`, `M X=...`, `R X=...` |
| `y` | MS-2000/Tiger XY | `Position` | um | R/W | Travel metadata | Yes | `W X Y`, `M Y=...`, `R Y=...` |
| `z` | MS-2000/Tiger Z | `Position` | um | R/W | Travel metadata | Yes | `W Z`, `M Z=...`, `R Z=...` |
| `busy` | Motion devices | `Bool` | none | R | none | No | `/` status |
| `ttl0` | Tiger TTL | `Bool` | none | R/W | none | Yes | Tiger TTL output command |
| `mode` | Tiger ring buffer | `String` | none | R/W | fixture modes | No | `RBMODE` |
| `size` | Tiger ring buffer | `I64` | frames/points | R/W | positive integer | No | fixture ring setup |
| `running` | Tiger ring buffer | `Bool` | none | R/W | none | Yes | `RM X=1/0` |
| `state` | Tiger CRISP | `String` | none | R/W | CRISP state labels | Yes | `LK X?`, `LK X=<state>` |
| `focus_score` | Tiger CRISP | `F64` | none | R | none | No | `LK Y?` |
| `offset` | Tiger CRISP | `Position` | um | R/W | fixture range | No | `LK Z?`, `LK Z=<um>` |
| `objective_na` | Tiger CRISP | `NumericalAperture` | none | R/W | 0..2 fixture range | No | `LR Y?`, `LR Y=<na>` |
| `lock_range` | Tiger CRISP | `Position` | um | R/W | fixture range | No | `LR Z?`, `LR Z=<mm>` |
| `in_focus_range` | Tiger CRISP | `Position` | um | R/W | fixture range | No | `AL Z?`, `AL Z=<um>` |

## Metadata And Config

| Key | Applies to | Type | Meaning |
| --- | --- | --- | --- |
| `x_travel`, `y_travel` | MS-2000/Tiger XY | `Position` | Axis travel ranges |
| `z_travel` | MS-2000/Tiger Z | `Position` | Axis travel range |
| `serial_units_per_um` | MS-2000 hub | `F64` | Protocol conversion metadata |
| `serial_port`, `baud_rate`, `serial_timeout_ms`, `connect` | Configured discovery/resource metadata | `String` / `I64` / `Bool` | Explicit serial endpoint and opt-in real transport connection |

Configured discovery accepts typed `Position` values for `x_travel`,
`y_travel`, and `z_travel`. Legacy scalar aliases `x_travel_um`,
`y_travel_um`, and `z_travel_um` remain accepted for existing configs and are
retained only as explicitly labeled `legacy_*` descriptor metadata.

When `connect = true`, discovery opens the configured serial endpoint, runs the
active MS-2000 or Tiger probe script, and seeds cached identity, position, busy
state, and Tiger CRISP state/focus-score from controller replies before
registering the driver.

Runtime property reads request and ingest the mapped query reply before
returning cached state for MS-2000/Tiger identity, position, busy state, and
Tiger CRISP state/focus/parameter fields.

Stage home and stop invocations consume an immediate `:A`/`:N` acknowledgement
when present, then request mapped `/` status plus `W` position readback after
the command. Tiger Z stop is addressed to the Z-stage card rather than the XY
card.

`HERE` is a coordinate-reference maintenance action, not a move. It is retained
only as an internal protocol operation and is not exposed through
`GenericCommand`.

Hub refresh commands are mapped query helpers. They accept no parameters, use
the same identity/status/position and CRISP readback commands as property reads
and startup readback, and do not expose arbitrary serial command strings.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- motion_stage` | Generic XY/Z `StageMove`, typed velocity/acceleration `MotionProfile` on ASI, typed position properties, remultiplexed state set, `Runtime::wait_completed`, timing plan, stop, and homing |
| `cargo run -p numanager-examples -- autofocus` | Provider-neutral autofocus invocation including Tiger CRISP |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Validate configured startup readback, move/home/halt/status semantics, and serial timing against real controllers |
| Discovery | Tiger card address negotiation and module inventory parsing require protocol/manual evidence or captured controller replies |
| Timing | Hardware-triggered TTL/ring-buffer routing and acquisition-plan validation beyond software start/stop hooks; Tiger X/Y/Z position and CRISP state sequencing are available as software timing endpoints |
| Autofocus | Real CRISP lock-state parsing, focus-curve acquisition, firmware-dependent shortcut behavior |
| Protocol expansion | Current command coverage includes MS-2000/Tiger position/status/identity readbacks, absolute/relative moves, home, halt, Tiger TTL output, Tiger ring-buffer state, and mapped CRISP readback/control helpers. Further ASI command families are not exposed without manufacturer documentation, public protocol evidence, or hardware traces, with coordinate-reference and reset/maintenance operations kept hidden |
