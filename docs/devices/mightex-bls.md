# Mightex Sirius BLS / SLC

## Status And Provenance

| Item | Value |
| --- | --- |
| Driver module | `numanager_drivers::mightex_bls` |
| Families | Mightex Sirius BLS control module and Sirius SLC LED driver over USB HID |
| Support level | HID output driver with typed light control, trigger/strobe setup, and disable-all helper |
| Protocol evidence | Reverse engineered HID feature-report framing and ASCII command construction |
| Transport | USB HID feature reports, report ID 0, 19-byte write report including report ID, ASCII payload terminated with reverse engineered LF/CR (`\n\r`) |
| Discovery | HID product string filter: `Sirius BLS` for BLS, `Sirius SLC` for SLC |
| Validation | No numanager hardware validation note yet |
| Runtime/evidence notes | Core HID feature-report trait and optional OS HID backend exist; hardware traces still needed to validate completion, scaling, and fault semantics |

## Logical Devices

| Device | Kind tags | Resource/remultiplexing |
| --- | --- | --- |
| `mightex-bls-hub` | `hub`, `light.engine`, `hid.device` | Owns one HID device and serializes feature-report writes/reads |
| `mightex-bls-channel-*` | `light.source`, `led.channel`, `trigger.sink` | Per-channel logical lights remultiplexed through the HID hub |
| `mightex-slc-hub` | `hub`, `light.engine`, `hid.device` | Same HID framing, product string `Sirius SLC` |
| `mightex-slc-channel-*` | `light.source`, `led.channel`, `trigger.sink` | Per-channel logical lights remultiplexed through the HID hub |

## Resources

| Resource | Kind | Purpose |
| --- | --- | --- |
| `<prefix>-hid-feature` | `usb.hid.feature` | Chunked ASCII command transport using `HidD_SetFeature`/`HidD_GetFeature` style operations |
| `<prefix>-reply-buffer` | `ascii.reply` | Buffered textual reply assembled from repeated feature reads until zero-length chunk |

## Capabilities

Current runtime driver exposes channel output capabilities and a hub-only
diagnostic ASCII command surface. These are intended for hardware bring-up;
completion, unit scaling, safe ranges, and fault semantics still need
trace-backed validation.

| Capability | Device | Request | Response | Completion | Timing support |
| --- | --- | --- | --- | --- | --- |
| `Dac` | BLS/SLC channel | `CapabilityRequest::Dac` with `Ratio` percent | `Ratio` written value or failed operation | HID write plus reply drain; obvious textual error/fail/nack replies fail the runtime token, but ACK vocabulary still needs validation | Runtime timing plans apply first/last `intensity` or `current_raw` endpoints through the same HID output path |
| `TriggerSink` | BLS/SLC channel | `None` or `CapabilityRequest::Trigger` | `Bool` enabled state or failed operation | HID write plus reply drain; obvious textual error/fail/nack replies fail the runtime token, but mode-code semantics still need validation | Runtime timing plans apply first/last `enabled` endpoints through the same HID output path; raw trigger/strobe profile timing is not exposed because timing semantics are not evidenced |
| `GenericCommand` | Hub | `CapabilityRequest::GenericCommand` with an empty `params` map and `disable_all` | Command/reply telemetry map or failed operation | Reply-draining for reverse engineered disable-all; reset, persistent-store, and default-restore remain hidden maintenance operations | Bring-up only; not user workflow |

## Properties

Current runtime properties:

| Property | Device | Type | Unit | Access | Notes |
| --- | --- | --- | --- | --- | --- |
| `product_string` | Hub | `String` | none | R | HID product string |
| `serial_number` | Hub | `String` | none | R | HID serial string, nullable |
| `vendor_id` | Hub | `I64` | none | R | USB VID |
| `product_id` | Hub | `I64` | none | R | USB PID |
| `channel_count` | Hub | `I64` | count | R | parsed from reverse engineered `SL<..><module><count>` product token where present, overridden by config when provided, otherwise conservative fallback/default 4 |
| `module_type` | Hub when available | `String` | none | R | parsed from reverse engineered `SL<..><module><count>` product token where present or overridden by config; omitted when unavailable |
| `support_level` | Hub/channel | `String` | none | R | `diagnostic` on hub, `output` on channels; HID resource metadata remains `discovery_only` |
| `command_count` | Hub | `I64` | count | R | Number of HID commands sent by this driver instance |
| `last_command` | Hub | `String` | none | R | Last ASCII command sent through HID |
| `last_reply` | Hub | `String` | none | R | Last assembled reply string |
| `last_reply_kind` | Hub | `String` | none | R | `ascii`, `none`, or `binary_echo_u16` |
| `last_outcome` | Hub | `String` | none | R | `accepted_unvalidated_reply` when no obvious textual failure token was seen, or `failed_obvious_reply` when one was classified |
| `last_error` | Hub | `String` | none | R | Last obvious textual error/fail/nack reply, or null after a later accepted reply |
| `last_reply_report_count` | Hub | `I64` | count | R | Number of HID feature reports read for the last reply |
| `last_transaction` | Hub | `Map` | none | R | Single trace-note-friendly summary containing command, reply, reply kind, reply expected flag, reply report count, outcome, reply error, command count, support level, wire terminator, and module type when known |
| `channel_index` | Channel | `I64` | count | R | one-based logical channel index |
| `output_supported` | Channel | `Bool` | none | R | `true` for output-capable channels |
| `enabled` | Channel | `Bool` | none | R/W | Output state; `false` writes `MODE <channel> 0` without changing configured `mode`; `true` applies configured `mode`, defaulting to `normal` when no mode was configured |
| `mode` | Channel | `String` enum | none | R/W | Configured output mode: BLS accepts `disabled`, `normal`, `trigger`; SLC also accepts `strobe`; trigger/strobe modes are sent when `enabled` is true |
| `mode_code` | Channel | `I64` | none | R/W | 0..255; low-level bring-up alias for raw `MODE <channel> <mode>` values whose mode semantics are unclassified |
| `current_raw` | Channel | `I64` | none | R/W | 0..100 bring-up limit; writes raw `CURRENT <channel> <value>` |
| `intensity` | Channel | `Ratio` | percent | R/W | 0..100%; writes rounded/clamped percent through `CURRENT`; scaling needs validation |
| `soft_start` | BLS channel | `Bool` | none | R/W | BLS-only setup flag; when true, enabling `trigger` mode sends `SoftStart <channel>` after `MODE <channel> 3` |
| `trigger_program` | BLS channel | `String` enum | none | R/W | BLS-only staged trigger setup: `pulse` sends reverse engineered `Trigger` plus `TrigP` profile lines when trigger output is enabled; `follow` sends reverse engineered follow-mode `TrigP` lines |
| `trigger_repeat_count` | BLS channel | `I64` | count | R/W | BLS-only staged repeat count used in `Trigger <channel> 100 1 <repeat>` when `trigger_program = "pulse"` |
| `trigger_pulse_current_1` | BLS channel | `I64` | count | R/W | BLS-only staged raw current count for pulse segment 1; sent as `TrigP <channel> 0 <current> <time>` |
| `trigger_pulse_current_2` | BLS channel | `I64` | count | R/W | BLS-only staged raw current count for pulse segment 2; sent as `TrigP <channel> 1 <current> <time>` |
| `trigger_pulse_current_3` | BLS channel | `I64` | count | R/W | BLS-only staged raw current count for pulse segment 3; sent as `TrigP <channel> 2 <current> <time>` |
| `trigger_pulse_time_1` | BLS channel | `I64` | count | R/W | BLS-only staged raw time/count field for pulse segment 1; physical unit not promoted without hardware/manual evidence |
| `trigger_pulse_time_2` | BLS channel | `I64` | count | R/W | BLS-only staged raw time/count field for pulse segment 2; physical unit not promoted without hardware/manual evidence |
| `trigger_pulse_time_3` | BLS channel | `I64` | count | R/W | BLS-only staged raw time/count field for pulse segment 3; physical unit not promoted without hardware/manual evidence |
| `trigger_follow_on_current` | BLS channel | `I64` | count | R/W | BLS-only staged raw on-current for follow mode; sent as `TrigP <channel> 1 <current> 9999` |
| `trigger_follow_off_current` | BLS channel | `I64` | count | R/W | BLS-only staged raw off-current for follow mode; sent as `TrigP <channel> 0 <current> 9999` |
| `overdrive_current_limit` | Channel | `Ratio` | percent | R/volatile | Reads `?GetImax <channel>`; adapter comments describe raw tenths-of-percent values such as `2000 = 200%` |
| `overdrive_duty_cycle_limit` | Channel | `Ratio` | percent | R/volatile | Reads `?GetODRules <channel>` parameter 1; adapter defaults imply raw tenths-of-percent values such as `100 = 10%` |
| `overdrive_pulse_width_limit` | Channel | `TimeInterval` | time | R/volatile | Reads `?GetODRules <channel>` parameter 2 in microseconds |
| `normal_current_max_raw` | SLC channel | `I64` | count | R/W | SLC-only staged raw maximum current; writes reverse engineered `NORMAL <channel> <max> <set>` and clamps `normal_current_set_raw` down when needed |
| `normal_current_set_raw` | SLC channel | `I64` | count | R/W | SLC-only staged raw normal-current setpoint; writes reverse engineered `NORMAL <channel> <max> <set>` and then `CURRENT <channel> <set>` to apply the setpoint |
| `strobe_current_max_raw` | SLC channel | `I64` | count | R/W | SLC-only staged raw strobe maximum current; used in `STROBE <channel> <max> <repeat>` when strobe mode is enabled |
| `strobe_repeat_count_raw` | SLC channel | `I64` | count | R/W | SLC-only staged raw strobe repeat count; used in `STROBE <channel> <max> <repeat>` when strobe mode is enabled |
| `trigger_current_max_raw` | SLC channel | `I64` | count | R/W | SLC-only staged raw trigger maximum current; used in `TRIGGER <channel> <max> <polarity>` when trigger mode is enabled |
| `trigger_polarity_raw` | SLC channel | `I64` | count | R/W | SLC-only staged raw trigger polarity; used in `TRIGGER <channel> <max> <polarity>` when trigger mode is enabled |
| `profile_frequency` | SLC channel | `Frequency` | frequency | R/W | SLC-only staged profile frequency; source derives profile row times from `1_000_000 / frequency` before enabling strobe or trigger output |
| `profile_duty_cycle` | SLC channel | `Ratio` | percent | R/W | SLC-only staged profile duty cycle; source derives off/on row times from frequency and ratio before enabling strobe or trigger output |
| `profile_current_1_raw` | SLC channel | `I64` | count | R/W | SLC-only staged raw current for profile row 0; copied to both strobe and trigger profile setup rows |
| `profile_current_2_raw` | SLC channel | `I64` | count | R/W | SLC-only staged raw current for profile row 1; copied to both strobe and trigger profile setup rows |
| `mode_code_readback` | SLC channel | `I64` | none | R/volatile | Sends reverse engineered `ECHOOFF` flush, then reads `?MODE <channel>` parameter 1 for trace collection |
| `current_max_raw_readback` | SLC channel | `I64` | count | R/volatile | Sends reverse engineered `ECHOOFF` flush, then reads `?CURRENT <channel>` current-max parameter; parameter index is `7` for MA/CA modules and `11` otherwise |
| `current_raw_readback` | SLC channel | `I64` | count | R/volatile | Sends reverse engineered `ECHOOFF` flush, then reads `?CURRENT <channel>` current-set parameter; parameter index is `8` for MA/CA modules and `12` otherwise |
| `strobe_current_max_raw_readback` | SLC channel | `I64` | count | R/volatile | Sends reverse engineered `ECHOOFF` flush, then reads `?STROBE <channel>` parameter 1 |
| `strobe_repeat_count_raw_readback` | SLC channel | `I64` | count | R/volatile | Sends reverse engineered `ECHOOFF` flush, then reads `?STROBE <channel>` parameter 2 |
| `strobe_profile_raw_readback` | SLC channel | `List` | count | R/volatile | Sends reverse engineered `ECHOOFF` flush, then reads `?STRP <channel>` current/time raw pairs until the first zero-time terminator |
| `trigger_current_max_raw_readback` | SLC channel | `I64` | count | R/volatile | Sends reverse engineered `ECHOOFF` flush, then reads `?TRIGGER <channel>` parameter 1 |
| `trigger_polarity_raw_readback` | SLC channel | `I64` | count | R/volatile | Sends reverse engineered `ECHOOFF` flush, then reads `?TRIGGER <channel>` parameter 2 |
| `trigger_profile_raw_readback` | SLC channel | `List` | count | R/volatile | Sends reverse engineered `ECHOOFF` flush, then reads `?TRIGP <channel>` current/time raw pairs until the first zero-time terminator |
| `load_voltage_raw` | SLC channel | `I64` | count | R/volatile | Sends `ReadBinaryV <channel>` and reads reverse engineered `0xEE 0xEE <MSB> <LSB>` binary echo as an uncalibrated raw count |

Calibrated output-control properties:

| Property | Device | Type | Unit | Access | Range/enums/increment | Sequenceable | Wire mapping |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `model` | Hub | `String` | none | R | HID product string | No | HID product string |
| `serial_number` | Hub | `String` | none | R | HID serial string | No | HID serial string |
| `channel_count` | Hub | `I64` | count | R | parsed from reverse engineered `SL<..><module><count>` product token, config override, default 4 | No | Product-name parse from reverse engineered evidence or config metadata |
| `module_type` | Hub | `String` | none | R | `AA`, `AV`, `SA`, `SV`, `MA`, `CA`, `HA`, `HV`, `FA`, `FV`, `XA`, `XV`, `QA` | No | Product-name parse from reverse engineered evidence or config metadata |
| `max_current` | Channel | `ElectricCurrent` | named current value | R | queried from device | No | `?GetImax` / `?CURRENT` family, scaling still needs hardware validation |
| `mode` | Channel | `String` enum | none | R/W | BLS: disabled/normal/trigger; SLC: disabled/normal/strobe/trigger | Yes | `MODE <channel> <mode>` |
| `current` | Channel | `ElectricCurrent` or `Ratio` | named current value or percent | R/W | range from `max_current`; exact units need validation | Yes | `CURRENT <channel> <value>` |
| `pulse_profile` | Channel | `Map` | typed current/time values | R/W | pulse count and per-step constraints from controller | No | Typed replacement for current BLS raw staged trigger properties after units and trigger timing are validated |
| `follow_current_on` | Channel | `ElectricCurrent` or `Ratio` | named current value or percent | R/W | range from rules query | No | follow-mode `TrigP` sequence |
| `follow_current_off` | Channel | `ElectricCurrent` or `Ratio` | named current value or percent | R/W | range from rules query | No | follow-mode `TrigP` sequence |

## Protocol Surface

| Operation | Evidence | Notes |
| --- | --- | --- |
| HID discovery | Reverse engineered HID implementation | Enumerates HID devices and filters by product string |
| Feature write framing | Reverse engineered HID implementation | Report byte 0 is report ID 0; byte 1 is ASCII type 1; byte 2 is payload length; bytes 3..18 carry command chunks; typed commands are terminated with LF/CR (`\n\r`) on the wire |
| Feature reply drain | Reverse engineered HID implementation | Repeated feature reads append bytes 3..N while byte 2 length is nonzero |
| BLS channel count/module type | Reverse engineered BLS implementation | Parses the HID product-name token starting at `SL`, reads module code at token bytes 4..6 and two-digit channel count at bytes 6..8, and defaults to four channels when parsing fails |
| BLS current/rule queries | Reverse engineered BLS implementation | Uses `?GetImax` and `?GetODRules` during open |
| BLS output writes | Reverse engineered BLS implementation | Uses `MODE`, `CURRENT`, `Trigger`, `TrigP`, and `SoftStart` command families; when LED output is enabled in trigger mode, the adapter sends `MODE`, then pulse or follow `Trigger`/`TrigP` setup, then optional `SoftStart`; maintenance helpers are hidden |
| SLC parameter readback and diagnostics | Reverse engineered SLC implementation | Writes normal max/set with `NORMAL`, applies normal current with `CURRENT`, derives strobe/trigger profile times from frequency and ratio before output enable, writes `STROBE` plus `STRP` rows or `TRIGGER` plus `TRIGP` rows before `MODE`, uses `ECHOOFF` to flush the receive buffer before profile readback, reads `?MODE` parameter 1, reads `?CURRENT` max/set parameters 7/8 for MA/CA modules or 11/12 otherwise, reads `?STROBE` max/repeat and `?STRP` current/time pairs, reads `?TRIGGER` max/polarity and `?TRIGP` current/time pairs, and keeps maintenance helpers hidden; `ReadBinaryV` reads load voltage as a binary echo with two `0xEE` header bytes followed by two payload bytes |

## Rust Protocol Layer

| Item | Location | Status |
| --- | --- | --- |
| HID feature-report trait | `numanager_core::hid::HidFeatureIo` | Available for real HID and scripted bring-up transports |
| Optional OS HID backend | `numanager_core::hid::OsHidFeatureDevice` behind `os-hid` | Compiles as an optional backend; not enabled by default |
| Feature report constants | internal Mightex BLS protocol helper | Encodes report ID 0, ASCII type 1, 19-byte reports, 16-byte payload chunks, and reverse engineered LF/CR command terminator |
| ASCII command builders | internal `SiriusCommand` helper | Implements exposed `MODE`, `CURRENT`, BLS `Trigger`/`TrigP`/`SoftStart`, `?GetImax`, `?GetODRules`, SLC `NORMAL`/`STROBE`/`STRP`/`TRIGGER`/`TRIGP`/`ECHOOFF`/`?MODE`/`?CURRENT`/`?STROBE`/`?STRP`/`?TRIGGER`/`?TRIGP`/`ReadBinaryV`, and reverse engineered disable-all; reset, persistent-store, and default-restore operations are hidden maintenance operations |
| Reply assembly | internal `ReplyAssembler` helper | Assembles repeated feature reads until a zero-length chunk |
| Discovery filter | `MightexBlsDiscovery` using public HID identity descriptors | Matches HID product strings `Sirius BLS` and `Sirius SLC` |
| Config-backed discovery | `MightexBlsDiscovery::from_config` | Claims configured BLS or SLC VID/PID/product identity when automatic HID discovery is insufficient; preserves the configured candidate label; optional `channel_count` and `module_type` override advertised topology metadata and are exposed on hub metadata; discovery locks persist identity/topology metadata for later audit |
| HID output driver | `MightexBlsDiscovery` / `MightexBlsDriver` | Enumerates matching HID identities, exposes `diagnostic` hub and `output` channel descriptors, and can send `MODE` plus low-range `CURRENT` output commands |
| Diagnostic hub command | `CapabilityKind::GenericCommand` on the hub | Accepts only named aliases for `disable_all`; `params` are rejected |
| Reply error classifier | `sirius_reply_error` | Conservative bring-up guard: only obvious `err`, `error`, `fail`, `failed`, `nak`, `nack`, or `invalid` leading tokens fail the runtime operation until hardware traces define the real vocabulary |

## Evidence Gate

| Claim | Current evidence | Default driver decision |
| --- | --- | --- |
| HID discovery | Reverse engineered behavior filters HID product strings containing `Sirius BLS` or `Sirius SLC` | Implemented for configured identities and optional OS HID enumeration |
| Feature-report framing | Reverse engineered feature reports use report ID 0, ASCII type 1, payload length byte, and 16-byte ASCII chunks in 19-byte reports | Implemented in crate-private Mightex BLS protocol helpers |
| Channel topology | Reverse engineered behavior parses channel count and module code from the reverse engineered `SL` product token, with four-channel fallback | Implemented as hub plus per-channel logical devices and a conservative fallback for configured product strings that omit the full token |
| Current/mode output | Reverse engineered behavior constructs `MODE` and `CURRENT` ASCII command families; mode constants are defined in adapter headers; exact unit scaling is not hardware-validated | Implemented as enum `mode`, `enabled`, low-range `current_raw`, and percent `intensity`; `mode` is configured state while `enabled` is output state, matching the adapter open/close split; `mode_code` remains a bring-up alias for unclassified raw values; public value bounds are runtime-validated before HID writes |
| BLS soft start | Reverse engineered behavior exposes `softStart` as a BLS property and sends `SoftStart <channel>` when trigger mode is enabled and soft-start is set | Implemented as BLS-only `soft_start` setup property; command is sent only when enabling configured trigger mode |
| BLS trigger/follow setup | Reverse engineered behavior exposes staged `repeatCnt`, `i1`/`i2`/`i3`, `t1`/`t2`/`t3`, `pulse_follow_mode`, `iON`, and `iOFF` properties, then sends `Trigger <channel> 100 1 <repeat>` plus `TrigP` pulse rows or the follow-mode `TrigP` rows when trigger output is enabled | Implemented as BLS-only raw staged trigger properties; sent only when enabling configured trigger mode; physical time/current units and timing-plan behavior remain unvalidated |
| SLC normal-current pair write | Reverse engineered behavior exposes `iMax` and `normal_CurrentSet`; changing `iMax` clamps the setpoint, writes `NORMAL <channel> <max> <set>`, then applies `CURRENT <channel> <set>`; changing the setpoint applies `CURRENT` after updating staged state | Implemented as SLC-only raw `normal_current_max_raw` and `normal_current_set_raw`; writes remain count-based bring-up only and are not promoted to calibrated current |
| SLC strobe/trigger setup writes | Reverse engineered behavior exposes staged `Strobe_CurrentMax`, `Strobe_RepeatCnt`, `Trigger_CurrentMax`, `Trigger_Polarity`, `frequency`, `ratio`, `i1`, and `i2`, derives two profile row durations with module-dependent rounding, sends strobe or trigger profile rows plus a zero-time terminator, then sends `MODE` | Implemented as SLC-only raw/typed staged setup properties; reverse engineered profile commands are sent only when enabling configured `strobe` or `trigger` mode; hardware trigger/strobe profile timing requires hardware traces |
| Overdrive rule queries | Reverse engineered behavior constructs `?GetImax` and `?GetODRules`; comments/defaults document raw tenths-of-percent and microsecond fields | Implemented as volatile read-only `overdrive_current_limit`, `overdrive_duty_cycle_limit`, and `overdrive_pulse_width_limit` properties for hardware bring-up |
| SLC raw readbacks | Reverse engineered behavior sends `ECHOOFF` before constructing profile readback queries; current max/set parameter indexes depend on module class; strobe/trigger profile loops read current/time pairs until a zero-time terminator | Implemented as SLC-only volatile `mode_code_readback`, current max/set, strobe max/repeat/profile, and trigger max/polarity/profile properties for hardware bring-up; current readback also refreshes staged normal max/set state; not promoted to calibrated units or timing-plan support |
| SLC load-voltage raw readback | Reverse engineered behavior constructs `ReadBinaryV`, then scans for two `0xEE` bytes and combines the following two bytes as `MSB << 8 | LSB`; scaling is not documented | Implemented as SLC-only volatile `load_voltage_raw` count for trace collection; not promoted to `Voltage` |
| Reply/error completion | Reverse engineered behavior shows reply draining for mode/current/query commands, while hidden maintenance helpers are send-only | Runtime token fails only for obvious textual failure replies when a reply is expected; hidden send-only helpers are not exposed through `GenericCommand` |
| Hub helpers | Reverse engineered ASCII command framing records broadcast mode control and separate maintenance operations | Implemented public `GenericCommand` accepts only the disable-all alias; reset, persistent-store, and default-restore remain hidden maintenance operations and arbitrary command strings are rejected |
| Remaining audited surface | Status/readback paths visible after the command audit are software-side `Status = No Fault`, `Busy() = false`, and dynamic-error helpers that do not request a hardware status frame | Do not add public fault/status/completion properties from those software defaults; require hardware traces or manufacturer protocol evidence |
| Safety/fault support | No trace-backed over-current, duty-cycle, thermal, output-fault, or trigger-lockout semantics are recorded | Do not promote calibrated output or safety claims until traces/manuals provide fault states and safe ranges |
| Timing support | No hardware trace validates mode/current sequencing latency or triggered profile semantics | Runtime timing plans apply first/last `enabled`, `current_raw`, and `intensity` endpoints through the same HID output path; BLS and SLC raw trigger/profile setup remains an explicit output bring-up path, while hardware trigger/profile timing is not exposed because timing semantics are not evidenced |

## Config

| Config field | Required | Type | Meaning |
| --- | --- | --- | --- |
| `driver = "mightex_bls"` | Yes | string | Selects the Mightex Sirius BLS/SLC discovery provider |
| `property.vendor_id` | Yes | `I64` or hex string | USB vendor ID |
| `property.product_id` | Yes | `I64` or hex string | USB product ID |
| `property.product_string` | Yes, unless `family` is set | string | HID product string containing `Sirius BLS` or `Sirius SLC` |
| `property.family` | Alternative to product string | string | Case-insensitive `BLS`, `SLC`, `Sirius BLS`, `Sirius SLC`, `Mightex BLS`, or `Mightex SLC` aliases |
| `property.serial_number` | No | string | Optional serial selector for opening the HID device |
| `property.channel_count` | No | `I64` in 1..=32 | Overrides advertised logical channel count when the HID product string does not contain the reverse engineered `SL<..><module><count>` token |
| `property.module_type` | No | string enum | Overrides advertised module type; accepted values are `AA`, `AV`, `SA`, `SV`, `MA`, `CA`, `HA`, `HV`, `FA`, `FV`, `XA`, `XV`, and `QA` |

Discovery locks persist the configured label, aliases, vendor/model identity,
USB VID/PID, product string, support level, channel count, and module type when
those fields are available from HID identity, product-token parsing, or config.
For Mightex BLS/SLC this support level comes from the hub descriptor, so it
identifies the candidate as a diagnostic/output bring-up surface
rather than discovery-only metadata.

## Output Bounds

| Surface | Bound | Rationale |
| --- | --- | --- |
| Channel `intensity` | `Ratio` 0..100 percent | Generic light-source workflow surface; converted to the currently evidenced `CURRENT` ASCII command as a percent mapping |
| Channel `current_raw` | integer 0..100 | Low bring-up range for the public property path while calibrated units and hardware-safe current ranges are unknown |
| Hub `GenericCommand` | named aliases only, no params; `disable_all` | Bring-up trace collection only; `disable_all` maps to reverse engineered `MODE 88 0` as a final bring-up safe-state command, but hardware fault/safety semantics remain unvalidated |

```toml
[[devices]]
id = 28001
label = "Mightex Sirius BLS"
driver = "mightex_bls"
property.vendor_id = 0x1234
property.product_id = 0x5678
property.family = "BLS"
property.serial_number = "optional-serial"
property.channel_count = 4
property.module_type = "CA"
```
Validation telemetry from the hub emits command, reply, `reply_kind`,
`reply_expected`, reply report count, command count, the LF/CR wire terminator,
conservative `outcome`, and `reply_error` when an obvious textual failure is
reported.

## Examples

| Example | Demonstrates |
| --- | --- |
| `cargo run -p numanager-examples -- light_source` | Generic fixture workflow for `Dac`, `TriggerSink`, typed intensity properties, and `Runtime::wait_completed` |
| `NUMANAGER_MIGHTEX_OUTPUT=1 cargo run -p numanager-examples --features os-hid -- light_source` | Opt-in real Mightex HID output through the same generic runtime workflow, holding 1% output for one second and printing public completion, typed readback, and prompts for physical output/readback observation |

## Hardware Bring-Up

| Task | Command/API | Notes |
| --- | --- | --- |
| Build with HID support | `cargo check -p numanager-drivers --features os-hid` | Enables `hidapi` through the optional `os-hid` feature |
| Run two-stage discovery with HID | `cargo run -p numanager-examples --features os-hid -- discover_devices` | Includes `MightexBlsDiscovery::os_hid` alongside the regular discovery flow |
| Run configured two-stage discovery | `cargo run -p numanager-examples -- discover_devices` | Includes config-backed Mightex Sirius BLS and SLC identity candidates without opening HID or driving output |
| Drive low Mightex output through the generic light-source workflow | `NUMANAGER_MIGHTEX_OUTPUT=1 cargo run -p numanager-examples --features os-hid -- light_source` | Adds detected Mightex candidates, writes `mode = "normal"` and `intensity = 1%`, enables the channel, invokes `Dac`, holds 1% output for one second, disables channel output, sends reverse engineered `disable_all`, reads overdrive rule properties, and prints public channel completion plus hub command/reply telemetry readback; the bench note must also record observed light output or instrument readback |
| Change output hold time | `NUMANAGER_MIGHTEX_OUTPUT=1 NUMANAGER_MIGHTEX_OUTPUT_HOLD_MS=5000 cargo run -p numanager-examples --features os-hid -- light_source` | Keeps the low output on for the requested number of milliseconds before disabling it |
| Discover Sirius devices | `MightexBlsDiscovery::os_hid(next_id)` | Matches HID product strings `Sirius BLS` and `Sirius SLC` |
| Discover from config | `MightexBlsDiscovery::from_config(next_id, &config)` | Uses configured VID/PID and product identity |
| Set output | Write channel `mode`, `intensity`, `current_raw`, BLS `soft_start`/raw trigger setup, `enabled`, or invoke `Dac`/`TriggerSink` | Use low values first; scaling, trigger timing, and fault behavior are still unvalidated |
| Send a Sirius helper | Invoke hub `GenericCommand` with `command` set to `disable_all` and no params | Bring-up only; record exact transaction telemetry before promoting behavior to typed properties; maintenance helpers remain hidden |
| Collect transaction evidence | Subscribe to telemetry or read hub `last_transaction`; individual `last_command`, `last_reply`, `last_reply_kind`, `last_reply_report_count`, `last_outcome`, `last_error`, and `command_count` properties remain available; read channel overdrive rule properties and SLC raw readbacks where advertised | Record exact replies before promoting raw output to calibrated properties |

Expected bring-up output includes lines with:

| Line prefix | Meaning |
| --- | --- |
| `mightex hardware initial safety:` | Normalized runtime safety state before output enable; currently derived from `enabled` only because interlock/fault semantics are not trace-validated |
| `mightex hardware output setup completed:` | `mode = "normal"`, `intensity = 1%`, and `enabled = true` state set completed |
| `mightex hardware dac completed:` | The generic `Dac` capability completed at 1% output |
| `mightex hardware active mode:` / `mightex hardware active enabled:` / `mightex hardware active intensity:` | Active-state readback printed before the hold window, so the trace note can compare requested output with reported software state while output should be observable |
| `mightex hardware active safety:` | Normalized runtime safety state during the hold window; `active` only means the channel is enabled, not that optical output has been validated safe |
| `mightex hardware output: holding 1% output for ... ms` | The software is intentionally leaving output enabled long enough to observe or meter it; this must be paired with a physical output/readback note before claiming validation |
| `mightex hardware disable completed:` | Output was disabled after the hold period |
| `mightex hardware disable-all completed:` | Hub diagnostic alias sent reverse engineered `MODE 88 0` after the channel disable |
| `mightex hardware final safety:` | Normalized runtime safety state after channel disable and hub disable-all |
| `mightex hardware hub last_transaction:` | Trace-note-friendly reverse engineered Sirius command/reply telemetry recorded by the hub |
| `mightex hardware hub last_command:` / `mightex hardware hub last_reply:` / `mightex hardware hub last_outcome:` | Individual transaction fields for quick terminal scanning |

## Remaining Work

| Area | Gap |
| --- | --- |
| Hardware validation | Exercise the output path and record trace-backed completion, physical light output/readback, error, unit, and range behavior |
| Hardware trace | Capture BLS/SLC feature reports for discovery, current write, mode write, reply/error behavior, and command completion using the HID section of [`../reverse/trace-capture-guide.md`](../reverse/trace-capture-guide.md) |
| Units | Confirm whether command current values are milliamps, DAC counts, percent of maximum, or module-dependent |
| Safety | Document over-current, duty-cycle, thermal, output fault, and trigger lockout behavior from hardware traces/manuals |
| Driver expansion | Replace raw output surfaces with calibrated properties once transport and completion semantics are validated enough to avoid an SDK-shaped black box |
